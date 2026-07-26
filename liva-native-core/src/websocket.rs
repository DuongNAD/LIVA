use crate::{AppState, handle_command, wake};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::net::TcpListener;
use tracing::{error, info, warn};

const MAX_WS_TEXT_BYTES: usize = 1024 * 1024;
const MAX_WS_MESSAGE_BYTES: usize = MAX_WS_TEXT_BYTES + 9;

struct AbortOnDropTask(tokio::task::JoinHandle<()>);

impl AbortOnDropTask {
    fn new(handle: tokio::task::JoinHandle<()>) -> Self {
        Self(handle)
    }

    fn abort(&self) {
        self.0.abort();
    }
}

impl Drop for AbortOnDropTask {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Debug, Deserialize)]
struct IpcRequest {
    id: String,
    command: String,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct IpcResponse {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

/// Reusable WebSocket transport shared by the standalone binary and Tauri host.
pub struct WebSocketServer {
    listener: TcpListener,
    address: SocketAddr,
}

impl WebSocketServer {
    pub async fn bind(address: &str) -> Result<Self, String> {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| format!("Failed to bind to {address}: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("Failed to resolve WebSocket address: {error}"))?;
        Ok(Self { listener, address })
    }

    pub async fn bind_from_env() -> Result<Self, String> {
        let port = std::env::var("LIVA_SERVER_PORT").unwrap_or_else(|_| "8002".to_string());
        let host = std::env::var("LIVA_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        Self::bind(&format!("{host}:{port}")).await
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub async fn run(self, state: Arc<AppState>) -> Result<(), String> {
        use tokio_tungstenite::accept_hdr_async_with_config;
        use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
        use tokio_tungstenite::tungstenite::http::{Response as HttpResponse, StatusCode};
        use tokio_tungstenite::tungstenite::protocol::WebSocketConfig;

        info!("WebSocket server listening on ws://{}/ws", self.address);
        let mut connections = tokio::task::JoinSet::new();
        loop {
            let accepted = tokio::select! {
                result = self.listener.accept() => result,
                completed = connections.join_next(), if !connections.is_empty() => {
                    if let Some(Err(error)) = completed {
                        error!("WebSocket connection task failed: {error}");
                    }
                    continue;
                }
            };
            let (stream, _) =
                accepted.map_err(|error| format!("WebSocket accept failed: {error}"))?;
            let connection_state = Arc::clone(&state);
            connections.spawn(async move {
                let reject = |status: StatusCode, message: &str| -> ErrorResponse {
                    HttpResponse::builder()
                        .status(status)
                        .body(Some(message.to_string()))
                        .expect("static rejection response is always valid")
                };

                #[allow(clippy::result_large_err)]
                let callback = |request: &Request, response: Response| {
                    if request.uri().path() != "/ws" {
                        return Err(reject(StatusCode::NOT_FOUND, "invalid path"));
                    }
                    let origin = request
                        .headers()
                        .get("origin")
                        .and_then(|value| value.to_str().ok());
                    if !crate::origin_allowed(origin) {
                        warn!(
                            "WebSocket rejected: origin {:?} is not allowed",
                            origin.unwrap_or("<none>")
                        );
                        return Err(reject(StatusCode::FORBIDDEN, "origin not allowed"));
                    }
                    Ok(response)
                };

                let websocket_config = WebSocketConfig {
                    max_message_size: Some(MAX_WS_MESSAGE_BYTES),
                    max_frame_size: Some(MAX_WS_MESSAGE_BYTES),
                    ..WebSocketConfig::default()
                };
                let websocket =
                    match accept_hdr_async_with_config(stream, callback, Some(websocket_config))
                        .await
                    {
                        Ok(websocket) => websocket,
                        Err(error) => {
                            error!("WebSocket handshake failed: {error}");
                            return;
                        }
                    };

                info!("New WebSocket client connected");
                if let Err(error) = handle_ws_connection(websocket, connection_state).await {
                    error!("WebSocket connection error: {error}");
                }
                info!("WebSocket client disconnected");
            });
        }
    }
}

async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
) -> Result<(), String> {
    use crate::webrtc::frame::{
        OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, SpeakerEpochGate, VoiceFrame,
    };
    use crate::webrtc::session::{TurnAudioAction, TurnAudioBuffer};
    use bytes::BytesMut;
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::mpsc;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (speaker_tx, mut speaker_rx) = mpsc::channel::<VoiceFrame>(128);
    let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(16);
    let (text_tx, mut text_rx) = mpsc::channel::<String>(128);

    // Spawn pipeline actor. conversation_id ổn định suốt kết nối này để bộ nhớ
    // hội thoại đọc lại được (session_id tăng mỗi lượt VAD nên không dùng được).
    let conversation_id = uuid::Uuid::new_v4().to_string();
    info!(
        "New WebSocket client connected (conversation {})",
        conversation_id
    );
    let memory_scope = crate::agent::graph::ConversationMemoryScope::new("local", &conversation_id)
        .expect("WebSocket conversation id must be valid");
    let voice_session =
        crate::webrtc::session::VoiceSessionAudio::from_app_state(state.as_ref()).await;
    let (pipeline_handle, actor) = crate::webrtc::pipeline::WebRTCActor::new(
        state.clone(),
        crate::webrtc::pipeline::VoiceOutbound::new(speaker_tx.clone(), control_tx.clone()),
        conversation_id.clone(),
        voice_session.aec_handle(),
    );
    let actor_handle = AbortOnDropTask::new(tokio::spawn(actor.run()));

    enum DataMessage {
        Speaker(Option<VoiceFrame>),
        Text(Option<String>),
    }

    // One socket writer: control is strict priority; speaker/text remain fair.
    let send_task = AbortOnDropTask::new(tokio::spawn(async move {
        let mut epoch_gate = SpeakerEpochGate::default();
        let mut control_open = true;
        let mut speaker_open = true;
        let mut text_open = true;

        while control_open || speaker_open || text_open {
            tokio::select! {
                biased;

                maybe_frame = control_rx.recv(), if control_open => {
                    match maybe_frame {
                        Some(frame) => {
                            if frame.op_code == OP_FLUSH {
                                epoch_gate.observe_flush(frame.seq_id);
                            }
                            match frame.encode() {
                                Ok(bytes) => {
                                    if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Binary(bytes.to_vec())).await {
                                        error!("Failed to send binary frame to client: {}", e);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to encode frame: {}", e);
                                }
                            }
                        }
                        None => control_open = false,
                    }
                }
                data = async {
                    tokio::select! {
                        frame = speaker_rx.recv(), if speaker_open => DataMessage::Speaker(frame),
                        text = text_rx.recv(), if text_open => DataMessage::Text(text),
                    }
                }, if speaker_open || text_open => match data {
                    DataMessage::Speaker(Some(frame)) => {
                        if !epoch_gate.accepts(&frame) {
                            continue;
                        }
                        match frame.encode() {
                            Ok(bytes) => {
                                if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Binary(bytes.to_vec())).await {
                                    error!("Failed to send binary frame to client: {}", e);
                                    break;
                                }
                            }
                            Err(e) => error!("Failed to encode frame: {}", e),
                        }
                    }
                    DataMessage::Speaker(None) => speaker_open = false,
                    DataMessage::Text(Some(text)) => {
                        if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(text)).await {
                            error!("Failed to send text frame to client: {}", e);
                            break;
                        }
                    }
                    DataMessage::Text(None) => text_open = false,
                }
            }
        }
    }));

    let mut turn_audio = TurnAudioBuffer::new(1536);
    let mut wake_gate = wake::WakeGate::from_env();
    if wake_gate.enabled() {
        info!("Wake-word gate enabled (mode {:?})", wake_gate.mode());
    }

    while let Some(msg_res) = ws_receiver.next().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(e) => {
                error!("WebSocket receive error: {}", e);
                break;
            }
        };

        match msg {
            tokio_tungstenite::tungstenite::Message::Binary(data) => {
                let mut bytes_mut = BytesMut::from(&data[..]);

                while bytes_mut.len() >= 9 {
                    let frame = match VoiceFrame::decode(&mut bytes_mut) {
                        Ok(Some(f)) => f,
                        Ok(None) => break,
                        Err(e) => {
                            error!("Frame decode error: {}", e);
                            break;
                        }
                    };

                    match frame.op_code {
                        OP_AUTH_HANDSHAKE => {
                            // Echo handshake back to acknowledge
                            let handshake_frame = VoiceFrame {
                                op_code: OP_AUTH_HANDSHAKE,
                                seq_id: frame.seq_id,
                                payload: frame.payload.clone(),
                            };
                            let _ = control_tx.send(handshake_frame).await;
                        }
                        OP_MIC_IN => {
                            let payload = &frame.payload;
                            let len_rounded = (payload.len() / 4) * 4;
                            let payload_aligned = &payload[..len_rounded];
                            let samples_vec: Vec<f32> = if (payload_aligned.as_ptr() as usize)
                                .is_multiple_of(std::mem::align_of::<f32>())
                            {
                                bytemuck::cast_slice(payload_aligned).to_vec()
                            } else {
                                payload_aligned
                                    .chunks_exact(4)
                                    .map(|chunk| {
                                        f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])
                                    })
                                    .collect()
                            };
                            // Mic pre-processing chain (both opt-in, off by default —
                            // see docs/99-luu-tru/bao-cao-lich-su/LIVA_OSS_Research_2026-07.md): AEC3 self-echo
                            // cancellation, then GTCRN denoise, then VAD — all in one
                            // blocking task with DSP state owned by this WebSocket.
                            let voice_session = voice_session.clone();
                            let (events, cleaned_samples) =
                                tokio::task::spawn_blocking(move || {
                                    voice_session.process_mic(samples_vec)
                                })
                                .await
                                .map_err(|e| format!("Audio pipeline task panicked: {}", e))??;

                            let samples_vec = cleaned_samples;

                            // Trained wake-word classifier (opt-in, LIVA_WAKE_MODE=trained_model):
                            // scans ambient audio continuously, independent of VAD state — a
                            // no-op in any other mode. A hit opens the gate exactly like the
                            // asr_prefix path's try_wake().
                            if let Some((name, score)) = wake_gate.check_streaming(&samples_vec) {
                                info!(
                                    "Wake word detected (trained model): {} ({:.3})",
                                    name, score
                                );
                            }

                            let vad_events = events
                                .into_iter()
                                .map(|(event, _confidence)| event)
                                .collect::<Vec<_>>();
                            for action in turn_audio.ingest(&samples_vec, &vad_events) {
                                match action {
                                    TurnAudioAction::Started => {
                                        // Barge-in only when awake — while the wake gate sleeps,
                                        // ambient speech (game chat, calls) must not cancel anything.
                                        if wake_gate.is_awake()
                                            && let Err(e) = pipeline_handle.on_vad_start()
                                        {
                                            error!("Failed on_vad_start: {}", e);
                                        }
                                    }
                                    TurnAudioAction::Ended(speech_audio) => {
                                        if wake_gate.is_awake() {
                                            wake_gate.note_activity();

                                            // Shadow-mode Smart Turn v3.2 (opt-in, off by default):
                                            // fire-and-forget, log-only, never gates the real
                                            // pipeline — see webrtc::turn_shadow module docs.
                                            let state_shadow = state.clone();
                                            let shadow_audio = speech_audio.clone();
                                            tokio::spawn(async move {
                                                let verdict =
                                                    tokio::task::spawn_blocking(move || {
                                                        let mut guard = state_shadow
                                                            .turn_shadow
                                                            .blocking_lock();
                                                        guard
                                                            .as_mut()
                                                            .map(|c| c.predict(&shadow_audio))
                                                    })
                                                    .await;
                                                if let Ok(Some(Ok(v))) = verdict {
                                                    info!(
                                                        "[shadow:smart-turn] probability={:.3} complete={} (VAD already decided: ended)",
                                                        v.probability, v.complete
                                                    );
                                                }
                                            });

                                            if let Err(e) = pipeline_handle.on_vad_end(speech_audio)
                                            {
                                                error!("Failed on_vad_end: {}", e);
                                            }
                                        } else if wake_gate.uses_stt_confirm() {
                                            // Asleep, tier-2 (asr_prefix/hybrid): transcribe once and
                                            // forward only if the transcript contains the wake phrase
                                            // ("LIVA, …" works in one breath — same utterance forwarded).
                                            // In hybrid this is the fallback when the tier-1 classifier
                                            // missed (typically a Vietnamese pronunciation).
                                            let state_wake = state.clone();
                                            let audio_for_stt = speech_audio.clone();
                                            let transcript =
                                                tokio::task::spawn_blocking(move || {
                                                    let mut stt = state_wake.stt.blocking_lock();
                                                    // Wake detection uses the light Nemotron path even
                                                    // in Parakeet mode — never load the 2.4GB model just
                                                    // to hear "liva" while asleep.
                                                    stt.transcribe_for_wake(&audio_for_stt)
                                                })
                                                .await;
                                            match transcript {
                                                Ok(Ok(Some(text))) => {
                                                    if wake_gate.try_wake(&text) {
                                                        info!(
                                                            "Wake word detected (tier-2 STT): {:?}",
                                                            text
                                                        );
                                                        if let Err(e) =
                                                            pipeline_handle.on_vad_end(speech_audio)
                                                        {
                                                            error!("Failed on_vad_end: {}", e);
                                                        }
                                                    }
                                                }
                                                Ok(Ok(None)) => {}
                                                Ok(Err(e)) => error!("Wake-gate STT failed: {}", e),
                                                Err(e) => {
                                                    error!("Wake-gate STT task panicked: {}", e)
                                                }
                                            }
                                        }
                                        // else: asleep + trained_model-only → tier-1 classifier
                                        // (check_streaming, above) is the sole gate; no STT run.
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }
            }
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                if text.len() > MAX_WS_TEXT_BYTES {
                    warn!(
                        size = text.len(),
                        limit = MAX_WS_TEXT_BYTES,
                        "WebSocket text message rejected: payload too large"
                    );
                    break;
                }
                let trim_text = text.trim();
                if !trim_text.is_empty() {
                    // Try parsing as legacy client event
                    if let Ok(legacy_val) = serde_json::from_str::<serde_json::Value>(trim_text)
                        && let Some(event_str) = legacy_val["event"].as_str()
                    {
                        let event_name = event_str.to_string();
                        let payload = legacy_val["payload"].clone();
                        let state_clone = state.clone();
                        let text_tx_clone = text_tx.clone();
                        let memory_scope = memory_scope.clone();

                        tokio::spawn(async move {
                            match event_name.as_str() {
                                "get_config" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_config",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "config_data",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_ai_config" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_ai_config",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "ai_config",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_voice_status" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_voice_status",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "voice_status",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_voice_profiles" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_voice_profiles",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "voice_profiles",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_system_status" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_system_status",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "system_status",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_skills_list" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_skills_list",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "skills_list",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_user_profile" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_user_profile",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "user_profile",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_tasks" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_tasks",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "tasks_list",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_avatar_models" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_avatar_models",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "avatar_models_list",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "get_memory_data" => {
                                    if let Ok(res) = handle_command(
                                        state_clone,
                                        "get_memory_data",
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "memory_data",
                                                    "payload": res
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                    }
                                }
                                "user_voice_command" => {
                                    let user_text =
                                        payload["text"].as_str().unwrap_or("").to_string();
                                    info!("Received user_voice_command text: {}", user_text);

                                    let _ = text_tx_clone
                                        .send(
                                            serde_json::json!({
                                                "event": "ai_thinking_start",
                                                "payload": {}
                                            })
                                            .to_string(),
                                        )
                                        .await;

                                    let _ = text_tx_clone
                                        .send(
                                            serde_json::json!({
                                                "event": "ai_stream_start",
                                                "payload": {}
                                            })
                                            .to_string(),
                                        )
                                        .await;

                                    // Screen-look intent → vision path (capture screen + VL core),
                                    // stream the answer, then finish. Leaves the text path below
                                    // untouched. Requires a VL model + mmproj (release build).
                                    let uv_lower = user_text.to_lowercase();
                                    if uv_lower.contains("màn hình") || uv_lower.contains("screen")
                                    {
                                        let q = user_text.clone();
                                        let sc = state_clone.clone();
                                        let text_tx_inner = text_tx_clone.clone();
                                        let vres = tokio::task::spawn_blocking(move || -> Result<String, String> {
                                                // Context-aware capture (mouse-guided crop while gaming).
                                                let (vw, vh, rgb) = crate::vision::capture::capture_for_vision()?;
                                                let mut llm_manager = sc.llm.blocking_lock();
                                                llm_manager
                                                    .answer_with_image(
                                                        &q,
                                                        crate::llm::engine::VisionImage::Rgb {
                                                            width: vw,
                                                            height: vh,
                                                            data: &rgb,
                                                        },
                                                        crate::llm::persona::TEMP_DEFAULT,
                                                        crate::llm::persona::TOP_P_DEFAULT,
                                                        |token| {
                                                            if token.is_empty() {
                                                                return true;
                                                            }
                                                            let chunk = serde_json::json!({
                                                                "event": "ai_stream_chunk",
                                                                "payload": { "textChunk": token, "isThought": false }
                                                            });
                                                            if let Ok(s) = serde_json::to_string(&chunk) {
                                                                let _ = text_tx_inner.blocking_send(s);
                                                            }
                                                            true
                                                        },
                                                    )
                                                    .map(|o| o.text)
                                            })
                                            .await;
                                        let final_text = match vres {
                                            Ok(Ok(t)) => t,
                                            _ => "Xin lỗi, hiện mình chưa xem được màn hình."
                                                .to_string(),
                                        };
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "ai_spoken_response",
                                                    "payload": { "text": final_text }
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                        let _ = text_tx_clone
                                            .send(
                                                serde_json::json!({
                                                    "event": "ai_thinking_end",
                                                    "payload": {}
                                                })
                                                .to_string(),
                                            )
                                            .await;
                                        return;
                                    }

                                    // RAG (22/07/2026): trước đây chỉ đường THOẠI (graph) có bộ
                                    // nhớ — gõ chữ qua UI thì LIVA "quên sạch". Dùng đúng cặp
                                    // recall/persist của graph để hai đường hành xử y hệt.
                                    // Thiếu model embedding thì recall trả None → như cũ.
                                    let mut messages = vec![crate::llm::ChatMessage {
                                        role: "system".to_string(),
                                        content: crate::llm::persona::PERSONA_LIVA.to_string(),
                                    }];
                                    if let Some(memories) =
                                        crate::agent::graph::recall_context_scoped(
                                            &state_clone,
                                            &user_text,
                                            &memory_scope,
                                        )
                                        .await
                                    {
                                        messages.push(crate::llm::ChatMessage {
                                            role: "system".to_string(),
                                            content: crate::agent::graph::memory_system_message(
                                                &memories,
                                            ),
                                        });
                                    }
                                    messages.push(crate::llm::ChatMessage {
                                        role: "user".to_string(),
                                        content: user_text.clone(),
                                    });

                                    // Handle riêng cho persist: closure spawn_blocking bên dưới
                                    // move mất `state_clone`.
                                    let state_persist = state_clone.clone();

                                    let compiled_prompt =
                                        match crate::llm::compile_prompt(&messages) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                error!("Failed to compile prompt: {}", e);
                                                let _ = text_tx_clone
                                                    .send(
                                                        serde_json::json!({
                                                            "event": "ai_thinking_end",
                                                            "payload": {}
                                                        })
                                                        .to_string(),
                                                    )
                                                    .await;
                                                return;
                                            }
                                        };

                                    let text_tx_inner = text_tx_clone.clone();
                                    let completion_res = tokio::task::spawn_blocking(move || {
                                        let mut llm_manager = state_clone.llm.blocking_lock();
                                        llm_manager.generate_completion(
                                            &compiled_prompt,
                                            crate::llm::persona::TEMP_DEFAULT,
                                            crate::llm::persona::TOP_P_DEFAULT,
                                            |token| {
                                                if token.is_empty() {
                                                    return true;
                                                }
                                                let chunk = serde_json::json!({
                                                    "event": "ai_stream_chunk",
                                                    "payload": {
                                                        "textChunk": token,
                                                        "isThought": false
                                                    }
                                                });
                                                if let Ok(chunk_str) = serde_json::to_string(&chunk)
                                                {
                                                    let _ = text_tx_inner.blocking_send(chunk_str);
                                                }
                                                true
                                            },
                                        )
                                    })
                                    .await;

                                    let (final_text, tra_loi_ok) = match completion_res {
                                        Ok(Ok(output)) => (output.text, true),
                                        _ => (
                                            "Xin lỗi, đã xảy ra lỗi trong quá trình xử lý."
                                                .to_string(),
                                            false,
                                        ),
                                    };

                                    // Lưu lượt này thành ký ức — cùng vị trí với graph (sau khi
                                    // có câu trả lời, trước khi gửi đi). CHỈ khi LLM thành công:
                                    // lưu câu xin lỗi mặc định sẽ làm bẩn kho nhớ bằng những
                                    // "ký ức" vô nghĩa. Lỗi ghi nhớ không làm hỏng câu trả lời
                                    // (persist_turn tự nuốt lỗi + log WARN).
                                    if tra_loi_ok {
                                        crate::agent::graph::persist_turn_scoped(
                                            &state_persist,
                                            &user_text,
                                            &final_text,
                                            &memory_scope,
                                        )
                                        .await;
                                    }

                                    let _ = text_tx_clone
                                        .send(
                                            serde_json::json!({
                                                "event": "ai_spoken_response",
                                                "payload": {
                                                    "text": final_text
                                                }
                                            })
                                            .to_string(),
                                        )
                                        .await;

                                    let _ = text_tx_clone
                                        .send(
                                            serde_json::json!({
                                                "event": "ai_thinking_end",
                                                "payload": {}
                                            })
                                            .to_string(),
                                        )
                                        .await;
                                }
                                "chat:completion" => {
                                    match crate::handle_chat_completion_scoped(
                                        state_clone,
                                        payload,
                                        None,
                                        None,
                                        memory_scope,
                                    )
                                    .await
                                    {
                                        Ok(res) => {
                                            let _ = text_tx_clone
                                                .send(
                                                    serde_json::json!({
                                                        "event": "chat:completion_response",
                                                        "payload": res
                                                    })
                                                    .to_string(),
                                                )
                                                .await;
                                        }
                                        Err(err) => {
                                            let _ = text_tx_clone
                                                .send(
                                                    serde_json::json!({
                                                        "event": "chat:completion_error",
                                                        "payload": { "error": err }
                                                    })
                                                    .to_string(),
                                                )
                                                .await;
                                        }
                                    }
                                }
                                _ => {
                                    // Try standard handle_command for other events
                                    let event_name_clone = event_name.clone();
                                    // Nhánh Err PHẢI gửi trả. Trước đây chỗ này
                                    // là `if let Ok(res)`, nên mọi lệnh lỗi qua
                                    // WebSocket biến mất không dấu vết: client
                                    // ngồi chờ tới lúc hết giờ rồi báo "timeout"
                                    // thay vì nói lý do thật. Ví dụ rõ nhất là
                                    // `vision:ask` ở build debug — lõi trả lỗi
                                    // "cần build release" ngay lập tức, nhưng
                                    // người dùng phải đợi 120 giây để nhận một
                                    // thông báo sai.
                                    match handle_command(
                                        state_clone,
                                        &event_name,
                                        payload,
                                        None,
                                        None,
                                    )
                                    .await
                                    {
                                        Ok(res) => {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                    "event": format!("{}_response", event_name_clone),
                                                    "payload": res
                                                }).to_string()).await;
                                        }
                                        Err(err) => {
                                            warn!("Lenh '{}' that bai: {}", event_name_clone, err);
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                    "event": format!("{}_error", event_name_clone),
                                                    "payload": {
                                                        "command": event_name_clone,
                                                        "error": err
                                                    }
                                                }).to_string()).await;
                                        }
                                    }
                                }
                            }
                        });
                        continue;
                    }

                    // Parse command
                    let req: IpcRequest = match serde_json::from_str(trim_text) {
                        Ok(r) => r,
                        Err(e) => {
                            let err_resp = IpcResponse {
                                id: "unknown".to_string(),
                                status: "error".to_string(),
                                data: None,
                                error: Some(format!("Invalid JSON query: {}", e)),
                            };
                            if let Ok(resp_str) = serde_json::to_string(&err_resp) {
                                let _ = text_tx.send(resp_str).await;
                            }
                            continue;
                        }
                    };

                    let req_id = req.id.clone();
                    info!("Received WS text command: {} (ID: {})", req.command, req_id);

                    let text_tx_clone = text_tx.clone();
                    let state_clone = state.clone();
                    let req_id_clone = req_id.clone();

                    tokio::spawn(async move {
                        let result = handle_command(
                            state_clone,
                            &req.command,
                            req.payload,
                            Some(text_tx_clone.clone()),
                            Some(req_id_clone),
                        )
                        .await;

                        let response = match result {
                            Ok(data) => IpcResponse {
                                id: req_id,
                                status: "ok".to_string(),
                                data: Some(data),
                                error: None,
                            },
                            Err(err_msg) => IpcResponse {
                                id: req_id,
                                status: "error".to_string(),
                                data: None,
                                error: Some(err_msg),
                            },
                        };

                        if let Ok(resp_str) = serde_json::to_string(&response) {
                            let _ = text_tx_clone.send(resp_str).await;
                        }
                    });
                }
            }
            tokio_tungstenite::tungstenite::Message::Close(_) => {
                break;
            }
            _ => {}
        }
    }

    // Clean up
    let _ = pipeline_handle.on_interrupted();
    send_task.abort();
    actor_handle.abort();
    Ok(())
}
