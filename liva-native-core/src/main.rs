#![allow(dead_code, unused_imports, unused_variables)]
use liva_native_core::{
    crypto, db, governor, llm, stt, telegram, tts, wake, webrtc, AppState, handle_command
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

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

fn main() {
    let worker_threads = std::env::var("LIVA_TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

    let max_blocking_threads = std::env::var("LIVA_TOKIO_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(512);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(max_blocking_threads)
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async_main());
}

async fn async_main() {
    // Initialize tracing to stderr so it doesn't pollute stdout (which is used for IPC)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("LIVA Native Core starting up...");

    let db_path = std::env::var("LIVA_DB_PATH")
        .unwrap_or_else(|_| "data/agents/liva_core/structured_memory.sqlite".to_string());
    let encryption_key = std::env::var("LIVA_ENCRYPTION_KEY")
        .unwrap_or_else(|_| "00000000000000000000000000000000".to_string());

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let is_in_memory = std::env::var("LIVA_DB_IN_MEMORY").is_ok();
    let db = if is_in_memory {
        db::DatabasePool::new_in_memory().expect("Failed to initialize in-memory DB")
    } else {
        db::DatabasePool::new(&db_path).expect("Failed to initialize DatabasePool")
    };

    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok((s, h)) => (Some(s), Some(h)),
        Err(e) => {
            error!("Failed to initialize default audio output stream: {}", e);
            (None, None)
        }
    };
    let sink = handle.as_ref().and_then(|h| match rodio::Sink::try_new(h) {
        Ok(s) => Some(s),
        Err(e) => {
            error!("Failed to create rodio Sink: {}", e);
            None
        }
    });

    // Resolve repo-relative model paths against the real project root so the
    // binary works from any working directory (repo root or liva-native-core).
    let stt_model_dir = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_STT_MODEL_DIR")
            .unwrap_or_else(|_| "models/nemotron-asr".to_string()),
    )
    .to_string_lossy()
    .into_owned();
    let tts_model_path = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_TTS_MODEL_PATH")
            .unwrap_or_else(|_| "models/kokoro-v1.0.onnx".to_string()),
    )
    .to_string_lossy()
    .into_owned();
    let tts_voice_path = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_TTS_VOICE_PATH")
            .unwrap_or_else(|_| "node_modules/kokoro-js/voices/af_heart.bin".to_string()),
    )
    .to_string_lossy()
    .into_owned();

    let stt_manager = stt::SttManager::new(&stt_model_dir);
    let shared_sink = sink.map(Arc::new);
    let tts_player = tts::audio::TtsAudioPlayer::new(shared_sink.clone());
    let tts_manager = match tts::TtsManager::from_bin(&tts_model_path, &tts_voice_path, shared_sink) {
        Ok(m) => Some(m),
        Err(e) => {
            error!(
                "Failed to initialize TtsManager: {}. TTS commands will fail.",
                e
            );
            None
        }
    };

    let llm_n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .unwrap_or_else(|_| "4096".to_string())
        .parse::<usize>()
        .unwrap_or(4096);
    let llm_n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<u32>()
        .unwrap_or(0);
    let llm_manager = llm::LlamaRouterManager::new(llm_n_ctx, llm_n_gpu_layers)
        .expect("Failed to initialize LlamaRouterManager");

    // Game-mode governor: watches for fullscreen apps and lowers process
    // priority so LIVA never steals frame time (LIVA_GAME_MODE=auto|on|off).
    let game_governor = Arc::new(governor::Governor::from_env());
    {
        let gov = game_governor.clone();
        std::thread::spawn(move || {
            loop {
                let _ = gov.game_mode_active();
                std::thread::sleep(std::time::Duration::from_secs(5));
            }
        });
    }

    // Initialize VAD Engine globally (stt_model_dir is already resolved above)
    let vad_model_path = webrtc::vad::resolve_model_path(&stt_model_dir);
    let vad_engine = if vad_model_path.exists() {
        match webrtc::vad::VadEngine::new(&vad_model_path, webrtc::vad::VadConfig::from_env()) {
            Ok(e) => Some(e),
            Err(err) => {
                eprintln!("Failed to initialize VadEngine: {}", err);
                None
            }
        }
    } else {
        eprintln!("VAD model not found at {:?}", vad_model_path);
        None
    };

    let vault_path = std::env::var("LIVA_VAULT_PATH")
        .unwrap_or_else(|_| "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault".to_string());
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(&vault_path));

    let native_capturer = Arc::new(liva_native_core::vision::capture::NativeScreenCapturer::new(0));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        native_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    // GTCRN denoise pre-stage — ON by default: isolates the user's voice from
    // mechanical-keyboard / game / Discord noise before VAD/STT so barge-in and
    // recognition stay reliable mid-session. Ultra-light (23.7K params, ~CPU).
    // Opt out with LIVA_DENOISE_ENABLED=0. A missing model or init error is
    // non-fatal — the pipeline just runs without denoise.
    let denoise_enabled = !matches!(
        std::env::var("LIVA_DENOISE_ENABLED").as_deref(),
        Ok("0") | Ok("false") | Ok("off")
    );
    let denoiser = if denoise_enabled {
        let path = webrtc::denoise::resolve_model_path();
        if path.exists() {
            match webrtc::denoise::GtcrnDenoiser::new(&path) {
                Ok(d) => {
                    tracing::info!("GTCRN denoise enabled (model {:?})", path);
                    Some(d)
                }
                Err(e) => {
                    eprintln!("Failed to initialize GtcrnDenoiser: {}; running without denoise", e);
                    None
                }
            }
        } else {
            eprintln!(
                "GTCRN denoise model not found at {:?}; running without denoise \
                 (fetch models/gtcrn_simple.onnx or set LIVA_DENOISE_ENABLED=0)",
                path
            );
            None
        }
    } else {
        tracing::info!("GTCRN denoise disabled via LIVA_DENOISE_ENABLED");
        None
    };

    // Optional Smart Turn v3.2 SHADOW-MODE classifier (LIVA_TURN_SHADOW_ENABLED=1):
    // logs its verdict alongside the frame-count VAD end-of-turn decision,
    // never acts on it — Vietnamese is its weakest language (81% vs 94% en).
    let turn_shadow = if std::env::var("LIVA_TURN_SHADOW_ENABLED").as_deref() == Ok("1") {
        let path = webrtc::turn_shadow::resolve_model_path();
        if path.exists() {
            match webrtc::turn_shadow::SmartTurnClassifier::new(&path) {
                Ok(c) => Some(c),
                Err(e) => {
                    eprintln!("Failed to initialize SmartTurnClassifier: {}", e);
                    None
                }
            }
        } else {
            eprintln!("Smart Turn model not found at {:?}", path);
            None
        }
    } else {
        None
    };

    // Optional self-echo cancellation (LIVA_AEC_ENABLED=1); cancels LIVA's
    // own TTS voice bleeding back into the mic during barge-in.
    let aec = if std::env::var("LIVA_AEC_ENABLED").as_deref() == Ok("1") {
        Some(webrtc::aec::SelfEchoCanceller::new())
    } else {
        None
    };

    let state = Arc::new(AppState {
        db,
        crypto: crypto::EncryptionEngine::new(&encryption_key),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(vad_engine),
        denoiser: tokio::sync::Mutex::new(denoiser),
        turn_shadow: tokio::sync::Mutex::new(turn_shadow),
        aec: tokio::sync::Mutex::new(aec),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    // Autoload the configured router LLM in the background so chat works
    // without a manual llm:swap_model call.
    let state_llm = state.clone();
    tokio::spawn(async move {
        liva_native_core::load_configured_router_model(state_llm, false).await;
    });

    // Game-aware GPU downshift: while a foreground game runs, reload the LLM
    // with fewer GPU layers to hand VRAM back to the game, and restore full
    // offload once the game exits. Only fires on an actual game-mode
    // transition (the reload is expensive). Disabled unless the normal config
    // uses the GPU and the game count differs. Env: LIVA_GAME_N_GPU_LAYERS
    // (default 0 = fully on CPU while gaming).
    {
        let state_gpu = state.clone();
        let normal_layers = llm_n_gpu_layers;
        let game_layers = std::env::var("LIVA_GAME_N_GPU_LAYERS")
            .ok()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0);
        tokio::spawn(async move {
            if normal_layers == 0 || game_layers == normal_layers {
                return; // nothing to downshift (CPU-only build/config or no delta)
            }
            let mut last_active: Option<bool> = None;
            loop {
                let active = governor::game_mode_active_now();
                if last_active != Some(active) {
                    let target = if active { game_layers } else { normal_layers };
                    // Latch the game state only once the model actually reached
                    // the target; if it isn't loaded yet, retry on the next poll.
                    if liva_native_core::reload_llm_gpu_layers(state_gpu.clone(), target).await {
                        last_active = Some(active);
                    }
                }
                tokio::time::sleep(std::time::Duration::from_secs(5)).await;
            }
        });
    }

    // Spawn WebRTC/IPC WebSocket server
    let state_ws = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_websocket_server(state_ws).await {
            error!("WebSocket server error: {}", e);
        }
    });

    // Spawn background task for idle TTS model unloading
    let state_unload_clone = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let tts_opt = state_unload_clone.tts.lock().await;
            if let Some(ref tts_mgr) = *tts_opt {
                tts_mgr.check_idle_unload();
            }
        }
    });

    // Create an mpsc channel to safely serialize and write responses to stdout
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Spawn background Telegram bot service if token is set
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
    if let Some(token) = telegram_token {
        let allowed_ids_raw = std::env::var("TELEGRAM_ALLOWED_IDS").unwrap_or_default();
        let allowed_ids: std::collections::HashSet<String> = allowed_ids_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        let state_tg = state.clone();
        let tx_tg = tx.clone();
        
        tokio::spawn(async move {
            let manager = Arc::new(telegram::TelegramBotManager::new(
                token,
                allowed_ids,
                state_tg,
                Some(tx_tg),
            ));
            manager.start().await;
        });
    }

    // Spawn stdout writer task
    let writer_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Some(msg) = rx.recv().await {
            let mut bytes = msg.into_bytes();
            bytes.push(b'\n');
            if let Err(e) = stdout.write_all(&bytes).await {
                error!("Failed to write IPC response to stdout: {}", e);
            }
            if let Err(e) = stdout.flush().await {
                error!("Failed to flush stdout: {}", e);
            }
        }
    });

    // Read commands from stdin line-by-line using Tokio async io
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trim_line = line.trim();
                if trim_line.is_empty() {
                    continue;
                }

                // Parse command
                let req: IpcRequest = match serde_json::from_str(trim_line) {
                    Ok(r) => r,
                    Err(e) => {
                        let err_resp = IpcResponse {
                            id: "unknown".to_string(),
                            status: "error".to_string(),
                            data: None,
                            error: Some(format!("Invalid JSON query: {}", e)),
                        };
                        if let Ok(resp_str) = serde_json::to_string(&err_resp) {
                            let _ = tx.send(resp_str).await;
                        }
                        continue;
                    }
                };

                let req_id = req.id.clone();
                info!("Received command: {} (ID: {})", req.command, req_id);

                let tx_clone = tx.clone();
                let state_clone = state.clone();
                let req_id_clone = req_id.clone();
                // Process request asynchronously
                tokio::spawn(async move {
                    let result = handle_command(
                        state_clone,
                        &req.command,
                        req.payload,
                        Some(tx_clone.clone()),
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
                        let _ = tx_clone.send(resp_str).await;
                    }
                });
            }
            Ok(None) => {
                break;
            }
            Err(e) => {
                error!("Error reading from stdin: {}", e);
                break;
            }
        }
    }

    // Drop the main sender so rx knows no more messages are coming after all processing tasks finish
    drop(tx);

    // Wait for writer task to finish writing all pending responses
    let _ = writer_handle.await;

    info!("LIVA Native Core shutting down...");
}

// handle_command is now imported from liva_native_core

async fn start_websocket_server(state: Arc<AppState>) -> Result<(), String> {
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    let port = std::env::var("LIVA_SERVER_PORT").unwrap_or_else(|_| "8002".to_string());
    let host = std::env::var("LIVA_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    let addr = format!("{}:{}", host, port);
    let listener = TcpListener::bind(&addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;
    info!("WebSocket server listening on ws://{}/ws", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut is_ws_path = false;
            let callback = |req: &Request, response: Response| {
                if req.uri().path() == "/ws" {
                    is_ws_path = true;
                }
                Ok(response)
            };

            let ws_stream = match accept_hdr_async(stream, callback).await {
                Ok(ws) => ws,
                Err(e) => {
                    error!("WebSocket handshake failed: {}", e);
                    return;
                }
            };

            if !is_ws_path {
                error!("WebSocket connection rejected: invalid path");
                return;
            }

            info!("New WebSocket client connected");
            if let Err(e) = handle_ws_connection(ws_stream, state_clone).await {
                error!("WebSocket connection error: {}", e);
            }
            info!("WebSocket client disconnected");
        });
    }

    Ok(())
}

async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::mpsc;
    use bytes::BytesMut;
    use crate::webrtc::frame::{VoiceFrame, OP_AUTH_HANDSHAKE, OP_MIC_IN};
    use crate::webrtc::vad::VadEvent;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<VoiceFrame>(128);
    let (text_tx, mut text_rx) = mpsc::channel::<String>(128);

    // Spawn pipeline actor. conversation_id ổn định suốt kết nối này để bộ nhớ
    // hội thoại đọc lại được (session_id tăng mỗi lượt VAD nên không dùng được).
    let conversation_id = uuid::Uuid::new_v4().to_string();
    info!("New WebSocket client connected (conversation {})", conversation_id);
    let (pipeline_handle, actor) = crate::webrtc::pipeline::WebRTCActor::new(
        state.clone(),
        outgoing_tx.clone(),
        conversation_id,
    );
    let actor_handle = tokio::spawn(actor.run());

    // Spawn outgoing message forwarder task multiplexing both binary and text frames
    let send_task = tokio::spawn(async move {
        loop {
            tokio::select! {
                maybe_frame = outgoing_rx.recv() => {
                    match maybe_frame {
                        Some(frame) => {
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
                        None => break,
                    }
                }
                maybe_text = text_rx.recv() => {
                    match maybe_text {
                        Some(text) => {
                            if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(text)).await {
                                error!("Failed to send text frame to client: {}", e);
                                break;
                            }
                        }
                        None => break,
                    }
                }
            }
        }
    });

    let mut accumulating = false;
    let mut audio_buffer = Vec::new();
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
                            let _ = outgoing_tx.send(handshake_frame).await;
                        }
                        OP_MIC_IN => {
                            let payload = &frame.payload;
                            let len_rounded = (payload.len() / 4) * 4;
                            let payload_aligned = &payload[..len_rounded];
                            let samples_vec: Vec<f32> = if payload_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
                                bytemuck::cast_slice(payload_aligned).to_vec()
                            } else {
                                payload_aligned
                                    .chunks_exact(4)
                                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                                    .collect()
                            };
                            let samples_vec_clone = samples_vec.clone();

                            // Mic pre-processing chain (both opt-in, off by default —
                            // see docs/99-luu-tru/bao-cao-lich-su/LIVA_OSS_Research_2026-07.md): AEC3 self-echo
                            // cancellation, then GTCRN denoise, then VAD — all in one
                            // blocking task with the shared engines.
                            let state_clone = state.clone();
                            let (events_res, cleaned_samples) = tokio::task::spawn_blocking(move || {
                                let mut working = samples_vec_clone;

                                if let Some(ref mut aec) = *state_clone.aec.blocking_lock() {
                                    match aec.process_capture(&working) {
                                        Ok(out) => working = out,
                                        Err(e) => tracing::error!("AEC process_capture failed: {}", e),
                                    }
                                }
                                if let Some(ref mut denoiser) = *state_clone.denoiser.blocking_lock() {
                                    match denoiser.process_audio(&working) {
                                        Ok(out) => working = out,
                                        Err(e) => tracing::error!("GTCRN denoise failed: {}", e),
                                    }
                                }

                                let events = {
                                    let mut vad_guard = state_clone.vad.blocking_lock();
                                    if let Some(ref mut vad) = *vad_guard {
                                        vad.process_audio(&working)
                                    } else {
                                        Ok(Vec::new())
                                    }
                                };
                                (events, working)
                            })
                            .await
                            .map_err(|e| format!("Audio pipeline task panicked: {}", e))?;

                            let events = events_res.map_err(|e| format!("VAD processing failed: {}", e))?;
                            let samples_vec = cleaned_samples;

                            // Trained wake-word classifier (opt-in, LIVA_WAKE_MODE=trained_model):
                            // scans ambient audio continuously, independent of VAD state — a
                            // no-op in any other mode. A hit opens the gate exactly like the
                            // asr_prefix path's try_wake().
                            if let Some((name, score)) = wake_gate.check_streaming(&samples_vec) {
                                info!("Wake word detected (trained model): {} ({:.3})", name, score);
                            }

                            for (event, _) in events {
                                match event {
                                    VadEvent::SpeechStart => {
                                        // Barge-in only when awake — while the wake gate sleeps,
                                        // ambient speech (game chat, calls) must not cancel anything.
                                        if wake_gate.is_awake() {
                                            if let Err(e) = pipeline_handle.on_vad_start() {
                                                error!("Failed on_vad_start: {}", e);
                                            }
                                        }
                                        accumulating = true;
                                        audio_buffer.clear();

                                        // Pre-populate with recent samples to avoid clipping initial speech onset
                                        let pre_trigger_len = 1536.min(samples_vec.len());
                                        audio_buffer.extend_from_slice(&samples_vec[samples_vec.len() - pre_trigger_len..]);
                                    }
                                    VadEvent::SpeechEnd => {
                                        accumulating = false;
                                        let speech_audio = std::mem::take(&mut audio_buffer);
                                        if wake_gate.is_awake() {
                                            wake_gate.note_activity();

                                            // Shadow-mode Smart Turn v3.2 (opt-in, off by default):
                                            // fire-and-forget, log-only, never gates the real
                                            // pipeline — see webrtc::turn_shadow module docs.
                                            let state_shadow = state.clone();
                                            let shadow_audio = speech_audio.clone();
                                            tokio::spawn(async move {
                                                let verdict = tokio::task::spawn_blocking(move || {
                                                    let mut guard = state_shadow.turn_shadow.blocking_lock();
                                                    guard.as_mut().map(|c| c.predict(&shadow_audio))
                                                })
                                                .await;
                                                if let Ok(Some(Ok(v))) = verdict {
                                                    info!(
                                                        "[shadow:smart-turn] probability={:.3} complete={} (VAD already decided: ended)",
                                                        v.probability, v.complete
                                                    );
                                                }
                                            });

                                            if let Err(e) = pipeline_handle.on_vad_end(speech_audio) {
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
                                            let transcript = tokio::task::spawn_blocking(move || {
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
                                                        info!("Wake word detected (tier-2 STT): {:?}", text);
                                                        if let Err(e) = pipeline_handle.on_vad_end(speech_audio) {
                                                            error!("Failed on_vad_end: {}", e);
                                                        }
                                                    }
                                                }
                                                Ok(Ok(None)) => {}
                                                Ok(Err(e)) => error!("Wake-gate STT failed: {}", e),
                                                Err(e) => error!("Wake-gate STT task panicked: {}", e),
                                            }
                                        }
                                        // else: asleep + trained_model-only → tier-1 classifier
                                        // (check_streaming, above) is the sole gate; no STT run.
                                    }
                                    VadEvent::None => {}
                                }
                            }

                            if accumulating {
                                audio_buffer.extend_from_slice(&samples_vec);
                            }
                        }
                        _ => {}
                    }
                }
            }
            tokio_tungstenite::tungstenite::Message::Text(text) => {
                let trim_text = text.trim();
                if !trim_text.is_empty() {
                    // Try parsing as legacy client event
                    if let Ok(legacy_val) = serde_json::from_str::<serde_json::Value>(trim_text) {
                        if let Some(event_str) = legacy_val["event"].as_str() {
                            let event_name = event_str.to_string();
                            let payload = legacy_val["payload"].clone();
                            let state_clone = state.clone();
                            let text_tx_clone = text_tx.clone();
                            
                            tokio::spawn(async move {
                                match event_name.as_str() {
                                    "get_config" => {
                                        if let Ok(res) = handle_command(state_clone, "get_config", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "config_data",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_ai_config" => {
                                        if let Ok(res) = handle_command(state_clone, "get_ai_config", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "ai_config",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_voice_status" => {
                                        if let Ok(res) = handle_command(state_clone, "get_voice_status", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "voice_status",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_voice_profiles" => {
                                        if let Ok(res) = handle_command(state_clone, "get_voice_profiles", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "voice_profiles",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_system_status" => {
                                        if let Ok(res) = handle_command(state_clone, "get_system_status", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "system_status",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_skills_list" => {
                                        if let Ok(res) = handle_command(state_clone, "get_skills_list", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "skills_list",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_user_profile" => {
                                        if let Ok(res) = handle_command(state_clone, "get_user_profile", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "user_profile",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_tasks" => {
                                        if let Ok(res) = handle_command(state_clone, "get_tasks", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "tasks_list",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_avatar_models" => {
                                        if let Ok(res) = handle_command(state_clone, "get_avatar_models", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "avatar_models_list",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "get_memory_data" => {
                                        if let Ok(res) = handle_command(state_clone, "get_memory_data", payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "memory_data",
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                    "user_voice_command" => {
                                        let user_text = payload["text"].as_str().unwrap_or("").to_string();
                                        info!("Received user_voice_command text: {}", user_text);
                                        
                                        let _ = text_tx_clone.send(serde_json::json!({
                                            "event": "ai_thinking_start",
                                            "payload": {}
                                        }).to_string()).await;
                                        
                                        let _ = text_tx_clone.send(serde_json::json!({
                                            "event": "ai_stream_start",
                                            "payload": {}
                                        }).to_string()).await;
                                        
                                        // Screen-look intent → vision path (capture screen + VL core),
                                        // stream the answer, then finish. Leaves the text path below
                                        // untouched. Requires a VL model + mmproj (release build).
                                        let uv_lower = user_text.to_lowercase();
                                        if uv_lower.contains("màn hình") || uv_lower.contains("screen") {
                                            let q = user_text.clone();
                                            let sc = state_clone.clone();
                                            let text_tx_inner = text_tx_clone.clone();
                                            let vres = tokio::task::spawn_blocking(move || -> Result<String, String> {
                                                // Context-aware capture (mouse-guided crop while gaming).
                                                let (vw, vh, rgb) = liva_native_core::vision::capture::capture_for_vision()?;
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
                                                _ => "Xin lỗi, hiện mình chưa xem được màn hình.".to_string(),
                                            };
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "ai_spoken_response",
                                                "payload": { "text": final_text }
                                            }).to_string()).await;
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": "ai_thinking_end",
                                                "payload": {}
                                            }).to_string()).await;
                                            return;
                                        }

                                        let messages = vec![
                                            crate::llm::ChatMessage {
                                                role: "system".to_string(),
                                                content: crate::llm::persona::PERSONA_LIVA.to_string(),
                                            },
                                            crate::llm::ChatMessage {
                                                role: "user".to_string(),
                                                content: user_text,
                                            }
                                        ];
                                        
                                        let compiled_prompt = match crate::llm::compile_prompt(&messages) {
                                            Ok(p) => p,
                                            Err(e) => {
                                                error!("Failed to compile prompt: {}", e);
                                                let _ = text_tx_clone.send(serde_json::json!({
                                                    "event": "ai_thinking_end",
                                                    "payload": {}
                                                }).to_string()).await;
                                                return;
                                            }
                                        };
                                        
                                        let text_tx_inner = text_tx_clone.clone();
                                        let completion_res = tokio::task::spawn_blocking(move || {
                                            let mut llm_manager = state_clone.llm.blocking_lock();
                                            llm_manager.generate_completion(&compiled_prompt, crate::llm::persona::TEMP_DEFAULT, crate::llm::persona::TOP_P_DEFAULT, |token| {
                                                let chunk = serde_json::json!({
                                                    "event": "ai_stream_chunk",
                                                    "payload": {
                                                        "textChunk": token,
                                                        "isThought": false
                                                    }
                                                });
                                                if let Ok(chunk_str) = serde_json::to_string(&chunk) {
                                                    let _ = text_tx_inner.blocking_send(chunk_str);
                                                }
                                                true
                                            })
                                        }).await;
                                        
                                        let final_text = match completion_res {
                                            Ok(Ok(output)) => output.text,
                                            _ => "Xin lỗi, đã xảy ra lỗi trong quá trình xử lý.".to_string(),
                                        };
                                        
                                        let _ = text_tx_clone.send(serde_json::json!({
                                            "event": "ai_spoken_response",
                                            "payload": {
                                                "text": final_text
                                            }
                                        }).to_string()).await;
                                        
                                        let _ = text_tx_clone.send(serde_json::json!({
                                            "event": "ai_thinking_end",
                                            "payload": {}
                                        }).to_string()).await;
                                    }
                                    _ => {
                                        // Try standard handle_command for other events
                                        let event_name_clone = event_name.clone();
                                        if let Ok(res) = handle_command(state_clone, &event_name, payload, None, None).await {
                                            let _ = text_tx_clone.send(serde_json::json!({
                                                "event": format!("{}_response", event_name_clone),
                                                "payload": res
                                            }).to_string()).await;
                                        }
                                    }
                                }
                            });
                            continue;
                        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> Arc<AppState> {
        unsafe {
            std::env::set_var("LIVA_ENCRYPTION_KEY", "00000000000000000000000000000000");
        }
        let db = db::DatabasePool::new_in_memory().unwrap();
        let stt_manager = stt::SttManager::new("data/models/nemotron-asr");
        let llm_manager = llm::LlamaRouterManager::new(2048, 0).unwrap();
        let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
            1920,
            1080,
            liva_native_core::vision::capture::PixelFormat::Rgba,
        ));
        let vision_manager = liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        );
        Arc::new(AppState {
            db,
            crypto: crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(stt_manager),
            tts: tokio::sync::Mutex::new(None),
            tts_player: tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(llm_manager),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
            vision: tokio::sync::Mutex::new(vision_manager),
        })
    }

    #[tokio::test]
    async fn test_ping() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "ping", payload, None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, serde_json::json!({ "pong": true }));
    }

    #[tokio::test]
    async fn test_echo() {
        let state = test_state();
        let payload = serde_json::json!({ "hello": "world" });
        let res = handle_command(state, "echo", payload.clone(), None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, payload);
    }

    #[tokio::test]
    async fn test_status() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "status", payload, None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val["engine"], "LIVA Native Engine");
        assert_eq!(val["status"], "healthy");
        assert_eq!(val["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "unknown", payload, None, None).await;
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), "Unknown command: unknown");
    }

    #[tokio::test]
    async fn test_vision_ipc_commands() {
        let state = test_state();

        // 1. Add region
        let region_payload = serde_json::json!({
            "id": "r1",
            "name": "Test Region",
            "x": 0,
            "y": 0,
            "width": 100,
            "height": 100,
            "threshold": 0.05
        });
        let res = handle_command(state.clone(), "vision:add_region", region_payload, None, None).await;
        assert!(res.is_ok());

        // 2. Set config
        let config_payload = serde_json::json!({
            "color_tolerance": 10,
            "max_regions": 10
        });
        let res = handle_command(state.clone(), "vision:set_config", config_payload, None, None).await;
        assert!(res.is_ok());

        // 3. Get changed regions (first time - baseline when last_frame is None)
        let res = handle_command(state.clone(), "vision:get_changed_regions", serde_json::json!({}), None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert!(val.is_array());
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["region_id"], "r1");
        assert_eq!(arr[0]["difference"], 1.0); // Baseline first frame
        assert_eq!(arr[0]["is_changed"], true);

        // 4. Capture screen
        let res = handle_command(state.clone(), "vision:capture", serde_json::json!({}), None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val["width"], 1920);
        assert_eq!(val["height"], 1080);
        assert!(val["data"].is_string());

        // 5. Remove region
        let remove_payload = serde_json::json!({ "id": "r1" });
        let res = handle_command(state.clone(), "vision:remove_region", remove_payload, None, None).await;
        assert!(res.is_ok());
    }

    #[test]
    fn test_tokio_runtime_env_parsing() {
        unsafe {
            std::env::set_var("LIVA_TOKIO_WORKER_THREADS", "8");
            std::env::set_var("LIVA_TOKIO_MAX_BLOCKING_THREADS", "128");
        }

        let worker_threads = std::env::var("LIVA_TOKIO_WORKER_THREADS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

        let max_blocking_threads = std::env::var("LIVA_TOKIO_MAX_BLOCKING_THREADS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or(512);

        assert_eq!(worker_threads, 8);
        assert_eq!(max_blocking_threads, 128);

        unsafe {
            std::env::remove_var("LIVA_TOKIO_WORKER_THREADS");
            std::env::remove_var("LIVA_TOKIO_MAX_BLOCKING_THREADS");
        }
    }
}
