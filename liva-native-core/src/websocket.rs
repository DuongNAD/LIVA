use crate::{AppState, CommandPrincipal, authorize_command, handle_command_as, wake};
pub use crate::webrtc::frame::{BufferPool, PooledBuffer};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::net::TcpListener;
use tracing::{error, info, warn};

mod dialogue;

#[derive(Serialize)]
struct AiStreamChunkPayload<'a> {
    #[serde(rename = "textChunk")]
    text_chunk: &'a str,
    #[serde(rename = "isThought")]
    is_thought: bool,
}

#[derive(Serialize)]
struct AiStreamChunkEvent<'a> {
    event: &'static str,
    payload: AiStreamChunkPayload<'a>,
}

/// Zero-copy compact serializer for streaming AI token chunks to WebSocket/IPC clients.
#[inline]
pub fn format_ai_stream_chunk(text_chunk: &str, is_thought: bool) -> Result<String, serde_json::Error> {
    serde_json::to_string(&AiStreamChunkEvent {
        event: "ai_stream_chunk",
        payload: AiStreamChunkPayload {
            text_chunk,
            is_thought,
        },
    })
}

const MAX_WS_TEXT_BYTES: usize = 1024 * 1024;
const MAX_WS_MESSAGE_BYTES: usize = MAX_WS_TEXT_BYTES + 9;
const MIN_WS_AUTH_TOKEN_BYTES: usize = 32;
const MAX_WS_AUTH_TOKEN_BYTES: usize = 4096;
const WS_SESSION_TTL: Duration = Duration::from_secs(30);
const MAX_OUTSTANDING_WS_SESSIONS: usize = 64;
/// The promoted classifier has already passed the configured owner-positive
/// and hard-negative gate. The runtime must therefore use that evaluated
/// threshold directly; an extra hard-coded cap would silently invalidate the
/// selection result and reject real owner scores that passed the gate.
fn wake_probe_classifier_direct_accept(score: Option<f32>, model_threshold: f32) -> bool {
    score.is_some_and(|value| value.is_finite() && value > model_threshold)
}

#[derive(Serialize)]
pub struct WebSocketSessionTicket {
    pub token: String,
    pub expires_in_ms: u64,
}

#[derive(Clone)]
pub struct WebSocketSessionAuthority {
    inner: Arc<Mutex<HashMap<[u8; 32], WebSocketSessionGrant>>>,
    ttl: Duration,
}

struct WebSocketSessionGrant {
    principal: CommandPrincipal,
    expires_at: Instant,
}

impl Default for WebSocketSessionAuthority {
    fn default() -> Self {
        Self::new()
    }
}

impl WebSocketSessionAuthority {
    pub fn new() -> Self {
        Self::with_ttl(WS_SESSION_TTL)
    }

    fn with_ttl(ttl: Duration) -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
            ttl,
        }
    }

    pub fn issue(&self, principal: CommandPrincipal) -> Result<WebSocketSessionTicket, String> {
        if !matches!(
            principal,
            CommandPrincipal::WebSocketWidget | CommandPrincipal::WebSocketDashboard
        ) {
            return Err("principal không được cấp WebSocket session đặc quyền".to_string());
        }

        let now = Instant::now();
        let mut grants = self
            .inner
            .lock()
            .map_err(|_| "WebSocket session authority bị lỗi đồng bộ".to_string())?;
        grants.retain(|_, grant| grant.expires_at > now);
        if grants.len() >= MAX_OUTSTANDING_WS_SESSIONS {
            return Err("đã đạt giới hạn WebSocket session đang chờ".to_string());
        }

        let (token, digest) = loop {
            let mut raw = [0_u8; 32];
            rand::rngs::OsRng.fill_bytes(&mut raw);
            let token = hex::encode(raw);
            let digest = websocket_session_digest(&token);
            if !grants.contains_key(&digest) {
                break (token, digest);
            }
        };
        grants.insert(
            digest,
            WebSocketSessionGrant {
                principal,
                expires_at: now + self.ttl,
            },
        );

        Ok(WebSocketSessionTicket {
            token,
            expires_in_ms: self.ttl.as_millis().min(u128::from(u64::MAX)) as u64,
        })
    }

    fn consume(&self, token: &str) -> Result<CommandPrincipal, String> {
        if token.len() != 64 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err("WebSocket session không hợp lệ".to_string());
        }
        let digest = websocket_session_digest(token);
        let mut grants = self
            .inner
            .lock()
            .map_err(|_| "WebSocket session authority bị lỗi đồng bộ".to_string())?;
        let grant = grants
            .remove(&digest)
            .ok_or_else(|| "WebSocket session không tồn tại hoặc đã được dùng".to_string())?;
        if grant.expires_at <= Instant::now() {
            return Err("WebSocket session đã hết hạn".to_string());
        }
        Ok(grant.principal)
    }
}

fn websocket_session_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn auth_token_for_ip(ip: IpAddr, configured: Option<&str>) -> Result<Option<String>, String> {
    if ip.is_loopback() {
        return Ok(None);
    }

    let token = configured.ok_or_else(|| {
        "LIVA_WS_AUTH_TOKEN is required when LIVA_SERVER_HOST is non-loopback".to_string()
    })?;
    if !(MIN_WS_AUTH_TOKEN_BYTES..=MAX_WS_AUTH_TOKEN_BYTES).contains(&token.len())
        || !token.bytes().all(|byte| byte.is_ascii_graphic())
    {
        return Err(format!(
            "LIVA_WS_AUTH_TOKEN must contain {MIN_WS_AUTH_TOKEN_BYTES}..={MAX_WS_AUTH_TOKEN_BYTES} visible ASCII bytes"
        ));
    }
    Ok(Some(token.to_string()))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (a, b)| difference | (a ^ b))
        == 0
}

fn bearer_token_matches(header: Option<&str>, expected: Option<&str>) -> bool {
    match expected {
        None => true,
        Some(expected) => header
            .and_then(|value| value.strip_prefix("Bearer "))
            .is_some_and(|provided| constant_time_eq(provided.as_bytes(), expected.as_bytes())),
    }
}

fn websocket_principal(
    peer_ip: IpAddr,
    query: Option<&str>,
    sessions: &WebSocketSessionAuthority,
) -> Result<CommandPrincipal, String> {
    let mut session = None;
    for pair in query.into_iter().flat_map(|query| query.split('&')) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "principal" {
            return Err("WebSocket không chấp nhận principal do client tự khai".to_string());
        }
        if key == "session" && (value.is_empty() || session.replace(value).is_some()) {
            return Err("WebSocket session bị thiếu hoặc khai báo lặp".to_string());
        }
    }

    match session {
        None => Ok(CommandPrincipal::WebSocketRemote),
        Some(_) if !peer_ip.is_loopback() => {
            Err("WebSocket session đặc quyền chỉ hợp lệ trên loopback".to_string())
        }
        Some(token) => sessions.consume(token),
    }
}

fn authorize_websocket_event(principal: CommandPrincipal, event_name: &str) -> Result<(), String> {
    let command = match event_name {
        "user_voice_command" | "chat:completion" => "chat:completion",
        command => command,
    };
    authorize_command(principal, command)
}

/// Số client WebSocket đang kết nối. Ô "Gateway" trên Dashboard trước đây in
/// cứng `wsClients: 1` — tức là báo "có một client" ngay cả khi không có ai,
/// và vẫn báo "một" khi có năm.
static WS_CLIENTS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Số client WebSocket đang kết nối ngay lúc này.
pub fn ws_client_count() -> usize {
    WS_CLIENTS.load(std::sync::atomic::Ordering::Relaxed)
}

/// Đếm bằng RAII thay vì `+1`/`-1` quanh lời gọi: task xử lý kết nối có thể
/// panic, hoặc bị abort lúc tắt máy. Một bộ đếm chỉ biết tăng là bộ đếm sai,
/// và sai theo hướng dễ chịu (luôn "có người dùng") — đúng loại lỗi mà bảng
/// sức khoẻ này sinh ra để diệt.
struct WsClientGuard;

impl WsClientGuard {
    fn new() -> Self {
        WS_CLIENTS.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        Self
    }
}

impl Drop for WsClientGuard {
    fn drop(&mut self) {
        WS_CLIENTS.fetch_sub(1, std::sync::atomic::Ordering::Relaxed);
    }
}

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
    auth_token: Option<Arc<str>>,
    sessions: WebSocketSessionAuthority,
}

impl WebSocketServer {
    pub async fn bind(address: &str) -> Result<Self, String> {
        let configured_token = std::env::var("LIVA_WS_AUTH_TOKEN").ok();
        Self::bind_with_auth(address, configured_token).await
    }

    pub async fn bind_with_auth(
        address: &str,
        configured_token: Option<String>,
    ) -> Result<Self, String> {
        let listener = TcpListener::bind(address)
            .await
            .map_err(|error| format!("Failed to bind to {address}: {error}"))?;
        let address = listener
            .local_addr()
            .map_err(|error| format!("Failed to resolve WebSocket address: {error}"))?;
        let auth_token =
            auth_token_for_ip(address.ip(), configured_token.as_deref())?.map(Arc::<str>::from);
        Ok(Self {
            listener,
            address,
            auth_token,
            sessions: WebSocketSessionAuthority::new(),
        })
    }

    pub async fn bind_from_env() -> Result<Self, String> {
        let port = std::env::var("LIVA_SERVER_PORT").unwrap_or_else(|_| "8002".to_string());
        let host = std::env::var("LIVA_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
        Self::bind(&format!("{host}:{port}")).await
    }

    pub fn local_addr(&self) -> SocketAddr {
        self.address
    }

    pub fn session_authority(&self) -> WebSocketSessionAuthority {
        self.sessions.clone()
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
            let (stream, peer_address) =
                accepted.map_err(|error| format!("WebSocket accept failed: {error}"))?;
            let connection_state = Arc::clone(&state);
            let connection_auth_token = self.auth_token.clone();
            let connection_sessions = self.sessions.clone();
            connections.spawn(async move {
                let principal_slot = Arc::new(std::sync::OnceLock::new());
                let callback_principal_slot = Arc::clone(&principal_slot);
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
                    let authorization = request
                        .headers()
                        .get("authorization")
                        .and_then(|value| value.to_str().ok());
                    if !bearer_token_matches(authorization, connection_auth_token.as_deref()) {
                        warn!("WebSocket rejected: authentication failed");
                        return Err(reject(StatusCode::UNAUTHORIZED, "authentication required"));
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
                    let principal = match websocket_principal(
                        peer_address.ip(),
                        request.uri().query(),
                        &connection_sessions,
                    ) {
                        Ok(principal) => principal,
                        Err(error) => {
                            warn!("WebSocket rejected: {error}");
                            return Err(reject(StatusCode::FORBIDDEN, "invalid session"));
                        }
                    };
                    if callback_principal_slot.set(principal).is_err() {
                        return Err(reject(
                            StatusCode::INTERNAL_SERVER_ERROR,
                            "principal already resolved",
                        ));
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
                let principal = *principal_slot
                    .get()
                    .expect("successful WebSocket handshake resolves principal");

                let _client = WsClientGuard::new();
                if let Err(error) =
                    handle_ws_connection(websocket, connection_state, principal).await
                {
                    error!("WebSocket connection error: {error}");
                }
                info!("WebSocket client disconnected");
            });
        }
    }
}

/// Ngắn hơn mức này thì Nemotron không có đủ ngữ cảnh để ra chữ; câu ứng viên
/// hợp lệ luôn được client đệm thêm pre-roll nên chạm ngưỡng dễ dàng.
const WAKE_PROBE_MIN_SECS: f32 = 0.3;
/// Dài hơn mức này thì đó là một câu nói, không phải cụm đánh thức — từ chối
/// trước khi tốn một lượt STT.
const WAKE_PROBE_MAX_SECS: f32 = 4.0;

/// Giải payload PCM f32 LE của khung thoại. Tách ra vì `OP_MIC_IN` và
/// `OP_WAKE_PROBE` dùng chung đúng một định dạng dây.
///
/// Nhánh `bytemuck` chỉ chạy khi con trỏ đã đúng canh lề f32 — `Bytes` không
/// đảm bảo điều đó, và `cast_slice` sẽ panic chứ không phải trả lỗi.
fn decode_f32_payload(payload: &[u8]) -> Vec<f32> {
    let len_rounded = (payload.len() / 4) * 4;
    let aligned = &payload[..len_rounded];
    if (aligned.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
        bytemuck::cast_slice(aligned).to_vec()
    } else {
        aligned
            .chunks_exact(4)
            .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
            .collect()
    }
}

/// Đổi một lỗi sinh-chữ thành câu LIVA nói ra.
///
/// **Vì sao tách khỏi chỗ dùng.** Bản cũ gộp mọi thất bại vào một nhánh `_ =>`
/// duy nhất, kể cả [`crate::llm::engine::ERR_NO_MODEL`] — thứ **không phải
/// hỏng**: router nạp bất đồng bộ lúc boot, nên mọi câu hỏi tới trong cửa sổ đó
/// đều rơi vào đây. Ngày 02/08/2026, sau khi đổi router sang model 4,2 GB, cửa
/// sổ ấy dài ra và người dùng chat trúng vào đó; LIVA đáp "đã xảy ra lỗi trong
/// quá trình xử lý" nên họ đi tìm một hỏng hóc không tồn tại.
///
/// Nhánh `match` cũ nằm giữa một hàm rất dài nên **không test được**, và một
/// nhánh không test được thì lần dọn dẹp sau sẽ gộp lại y như cũ. Hàm thuần này
/// tồn tại để có chỗ khoá hành vi bằng test.
///
/// `None` = tác vụ `spawn_blocking` chết (panic/huỷ) — đó mới là hỏng thật.
fn loi_chat_thanh_cau_noi(loi: Option<&str>) -> String {
    match loi {
        Some(e) if e.contains(crate::llm::engine::ERR_NO_MODEL) => {
            "Mình đang nạp mô hình, đợi vài giây rồi nhắn lại giúp mình nhé.".to_string()
        }
        _ => "Xin lỗi, đã xảy ra lỗi trong quá trình xử lý.".to_string(),
    }
}

/// Client có thể gửi Close đúng lúc writer vừa lấy một response khỏi channel.
/// Tungstenite đã tự chuyển socket sang trạng thái closing nên lần gửi kế tiếp
/// trả `SendAfterClosing`; đây là race teardown, không phải lỗi protocol đầu vào.
fn la_race_dong_websocket(error: &tokio_tungstenite::tungstenite::Error) -> bool {
    matches!(
        error,
        tokio_tungstenite::tungstenite::Error::Protocol(
            tokio_tungstenite::tungstenite::error::ProtocolError::SendAfterClosing
        )
    )
}

async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
    principal: CommandPrincipal,
) -> Result<(), String> {
    use crate::webrtc::frame::{
        OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, OP_WAKE_PROBE, SpeakerEpochGate, VoiceFrame,
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
        crate::webrtc::pipeline::VoiceOutbound::new(speaker_tx.clone(), control_tx.clone())
            .with_text_events(text_tx.clone()),
        conversation_id.clone(),
        voice_session.aec_handle(),
    );
    let actor_handle = AbortOnDropTask::new(tokio::spawn(actor.run()));
    let voice_message_dialogue = Arc::new(tokio::sync::Mutex::new(
        crate::messaging::VoiceMessageDialogue::default(),
    ));

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
                                        if la_race_dong_websocket(&e) {
                                            info!("WebSocket writer stopped during client close");
                                        } else {
                                            error!("Failed to send binary frame to client: {}", e);
                                        }
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
                        let op = frame.op_code;
                        let payload_len = frame.payload.len();
                        let send_start = std::time::Instant::now();
                        match frame.encode() {
                            Ok(bytes) => {
                                if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Binary(bytes.to_vec())).await {
                                    if la_race_dong_websocket(&e) {
                                        info!("WebSocket writer stopped during client close");
                                    } else {
                                        error!("Failed to send binary frame to client: {}", e);
                                    }
                                    break;
                                }
                                let transit_ms = send_start.elapsed().as_secs_f64() * 1000.0;
                                crate::telemetry::global_telemetry().record_ws_transit(op, transit_ms, payload_len);
                            }
                            Err(e) => error!("Failed to encode frame: {}", e),
                        }
                    }
                    DataMessage::Speaker(None) => speaker_open = false,
                    DataMessage::Text(Some(text)) => {
                        if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Text(text)).await {
                            if la_race_dong_websocket(&e) {
                                info!("WebSocket writer stopped during client close");
                            } else {
                                error!("Failed to send text frame to client: {}", e);
                            }
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
                        OP_WAKE_PROBE => {
                            // Cổng đánh thức của widget. Client tự cắt MỘT câu ứng
                            // viên bằng VAD năng lượng rẻ tiền rồi hỏi ở đây. UI chỉ
                            // chuyển PASSIVE → ACTIVE khi classifier vượt ngưỡng đã
                            // benchmark hoặc transcript thật sự chứa cụm đánh thức.
                            //
                            // Đường này KHÔNG chạm pipeline: không AEC/GTCRN/VAD,
                            // không TurnAudioBuffer, không on_vad_end. Nó chỉ trả
                            // lời một câu hỏi. Cũng không gọi try_wake — xem
                            // WakeGate::matches_phrase.
                            let samples_vec = decode_f32_payload(&frame.payload);
                            let duration_secs = samples_vec.len() as f32 / 16_000.0;
                            if !(WAKE_PROBE_MIN_SECS..=WAKE_PROBE_MAX_SECS).contains(&duration_secs)
                            {
                                // Quá ngắn thì STT không có gì để bám; quá dài thì
                                // đó là một câu nói dài, không phải cụm đánh thức.
                                // Bỏ im lặng — client fail-closed, không thức.
                                continue;
                            }

                            // ── Tầng 1: classifier đã train ──
                            // Chạy trước vì rẻ (~35 ms, không đụng STT) và vì nó
                            // là tầng DUY NHẤT xác minh được một cụm đánh thức
                            // đứng riêng: đo 27/07/2026, Nemotron trả chuỗi rỗng
                            // cho clip "Hey Liva" 3,05 s rms 0,0625 đỉnh 0,507 —
                            // audio to và sạch, ASR vẫn không ra chữ.
                            let best_score = wake_gate.score_clip(&samples_vec);
                            let model_threshold = wake_gate.model_threshold();
                            let clip_score = best_score.clone().filter(|(_, score)| {
                                wake_probe_classifier_direct_accept(Some(*score), model_threshold)
                            });

                            // ── Tầng 2: STT + so cụm từ ──
                            // Vẫn chạy khi tầng 1 trượt: classifier là mô hình
                            // English-centric, phát âm tiếng Việt nó bắt kém
                            // (models/README.md). Hai tầng bù nhau, OR với nhau.
                            let heard = if clip_score.is_some() {
                                String::new()
                            } else {
                                let state_probe = state.clone();
                                let audio_for_stt = samples_vec.clone();
                                let transcript = tokio::task::spawn_blocking(move || {
                                    let mut stt = state_probe.stt.blocking_lock();
                                    // Ép đường Nemotron nhẹ y như wake tier-2: không
                                    // bao giờ nạp Parakeet 2,4 GB chỉ để nghe "liva".
                                    stt.transcribe_for_wake(&audio_for_stt)
                                })
                                .await;

                                match transcript {
                                    Ok(Ok(Some(text))) => text,
                                    Ok(Ok(None)) => String::new(),
                                    Ok(Err(e)) => {
                                        error!("Wake probe STT failed: {}", e);
                                        String::new()
                                    }
                                    Err(e) => {
                                        error!("Wake probe STT task panicked: {}", e);
                                        String::new()
                                    }
                                }
                            };

                            let stt_matched =
                                !heard.trim().is_empty() && wake_gate.matches_phrase(&heard);
                            let matched = clip_score.is_some() || stt_matched;

                            match (&clip_score, matched) {
                                (Some((name, score)), _) => info!(
                                    "Wake word confirmed (widget probe, classifier {} = {:.3})",
                                    name, score
                                ),
                                (None, true) => {
                                    info!("Wake word confirmed (widget probe, STT): {:?}", heard)
                                }
                                // Ca bị từ chối PHẢI log ở đây. `logger` của UI chỉ
                                // ghi vào console webview Tauri — không thấy được từ
                                // terminal, nên nếu không log thì "gọi mà không thức"
                                // là câu hỏi không có dữ liệu nào trả lời được.
                                // Đánh đổi: tiếng nói xung quanh mic sẽ vào log core
                                // dưới dạng chữ. Cùng dữ liệu vốn đã gửi cho client,
                                // và chỉ nằm trên máy người dùng.
                                (None, false) => {
                                    // Kèm số đo của chính clip đã gửi. Transcript
                                    // rỗng có hai nguyên nhân hoàn toàn khác nhau —
                                    // clip câm (lỗi phía client) và clip có tiếng mà
                                    // ASR không ra chữ (giới hạn model) — và nếu chỉ
                                    // log transcript thì hai ca đó trông y hệt nhau.
                                    let peak = samples_vec.iter().fold(0f32, |m, s| m.max(s.abs()));
                                    let rms = (samples_vec.iter().map(|s| s * s).sum::<f32>()
                                        / samples_vec.len().max(1) as f32)
                                        .sqrt();
                                    let classifier = match &best_score {
                                        Some((name, score)) => format!(
                                            "{} {:.3} (threshold {:.2})",
                                            name, score, model_threshold
                                        ),
                                        None => "không nạp được".to_string(),
                                    };
                                    info!(
                                        "Wake probe rejected — nghe ra {:?} | classifier {} | clip {:.2}s rms {:.4} đỉnh {:.3}",
                                        heard, classifier, duration_secs, rms, peak
                                    );
                                }
                            }

                            let _ = text_tx
                                .send(
                                    serde_json::json!({
                                        "event": if matched {
                                            "wake_word_triggered"
                                        } else {
                                            "wake_probe_rejected"
                                        },
                                        "payload": {
                                            "source": "widget_probe",
                                            "tier": if clip_score.is_some() { "classifier" } else { "stt" },
                                            // Luon tra diem tho de UI hien thi dung muc phan biet,
                                            // ke ca khi probe bi tu choi hoac phai qua STT.
                                            "score": best_score.as_ref().map(|(_, s)| *s),
                                            // Cho panel chẩn đoán thấy nó NGHE ra gì —
                                            // không có cái này thì "sao không thức" là
                                            // một câu hỏi không tài nào trả lời được.
                                            "transcript": heard,
                                            "seq_id": frame.seq_id,
                                        }
                                    })
                                    .to_string(),
                                )
                                .await;
                        }
                        OP_MIC_IN => {
                            let samples_vec = decode_f32_payload(&frame.payload);
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
                        if let Err(error) = authorize_websocket_event(principal, &event_name) {
                            warn!("WebSocket event '{}' bị từ chối: {}", event_name, error);
                            let _ = text_tx
                                .send(
                                    serde_json::json!({
                                        "event": format!("{}_error", event_name),
                                        "payload": {
                                            "command": event_name,
                                            "error": error
                                        }
                                    })
                                    .to_string(),
                                )
                                .await;
                            continue;
                        }
                        let payload = legacy_val["payload"].clone();
                        let state_clone = state.clone();
                        let text_tx_clone = text_tx.clone();
                        let memory_scope = memory_scope.clone();
                        let voice_message_dialogue = Arc::clone(&voice_message_dialogue);
                        let pipeline_handle_clone = pipeline_handle.clone();

                        tokio::spawn(async move {
                            match event_name.as_str() {
                                "get_config" => {
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                    if let Ok(res) = handle_command_as(
                                        principal,
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
                                                            if let Ok(s) = format_ai_stream_chunk(token, false) {
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

                                    dialogue::handle_user_voice_text(
                                        state_clone,
                                        voice_message_dialogue,
                                        user_text,
                                        memory_scope,
                                        text_tx_clone,
                                        pipeline_handle_clone,
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
                                    match handle_command_as(
                                        principal,
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
                        let result = handle_command_as(
                            principal,
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
                send_task.abort();
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
mod security_tests {
    use super::{
        WebSocketSessionAuthority, auth_token_for_ip, authorize_websocket_event,
        bearer_token_matches, la_race_dong_websocket, loi_chat_thanh_cau_noi,
        wake_probe_classifier_direct_accept, websocket_principal, websocket_session_digest,
    };
    use crate::CommandPrincipal;
    use std::net::{IpAddr, Ipv4Addr};
    use std::time::Duration;

    #[test]
    fn gui_sau_close_duoc_phan_loai_la_race_shutdown() {
        use tokio_tungstenite::tungstenite::{Error, error::ProtocolError};

        assert!(la_race_dong_websocket(&Error::Protocol(
            ProtocolError::SendAfterClosing,
        )));
        assert!(!la_race_dong_websocket(&Error::Protocol(
            ProtocolError::ReceivedAfterClosing,
        )));
    }

    #[test]
    fn non_loopback_bat_buoc_token_du_manh() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        assert_eq!(auth_token_for_ip(loopback, None).unwrap(), None);

        let lan = IpAddr::V4(Ipv4Addr::new(0, 0, 0, 0));
        assert!(auth_token_for_ip(lan, None).is_err());
        assert!(auth_token_for_ip(lan, Some("too-short")).is_err());
        assert_eq!(
            auth_token_for_ip(lan, Some(&"x".repeat(32))).unwrap(),
            Some("x".repeat(32))
        );
    }

    #[test]
    fn bearer_auth_fail_closed_va_khong_chap_nhan_prefix() {
        let expected = "0123456789abcdef0123456789abcdef";
        assert!(bearer_token_matches(
            Some("Bearer 0123456789abcdef0123456789abcdef"),
            Some(expected)
        ));
        assert!(!bearer_token_matches(None, Some(expected)));
        assert!(!bearer_token_matches(
            Some("Bearer 0123456789abcdef"),
            Some(expected)
        ));
        assert!(!bearer_token_matches(
            Some("Bearer 0123456789abcdef0123456789abcdef-extra"),
            Some(expected)
        ));
        assert!(bearer_token_matches(None, None));
    }

    #[test]
    fn loopback_chi_nhan_principal_da_khai_bao() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let sessions = WebSocketSessionAuthority::new();
        assert!(websocket_principal(loopback, Some("principal=widget"), &sessions).is_err());
        assert!(websocket_principal(loopback, Some("principal=dashboard"), &sessions).is_err());
        assert_eq!(
            websocket_principal(loopback, None, &sessions).unwrap(),
            CommandPrincipal::WebSocketRemote
        );
        assert!(websocket_principal(loopback, Some("principal=admin"), &sessions).is_err());
    }

    #[test]
    fn client_ngoai_loopback_khong_the_nang_quyen_bang_query() {
        let lan = IpAddr::V4(Ipv4Addr::new(192, 168, 1, 12));
        let sessions = WebSocketSessionAuthority::new();
        assert!(websocket_principal(lan, Some("principal=dashboard"), &sessions).is_err());
        let ticket = sessions
            .issue(CommandPrincipal::WebSocketDashboard)
            .expect("issue dashboard ticket");
        assert!(
            websocket_principal(lan, Some(&format!("session={}", ticket.token)), &sessions)
                .is_err()
        );
    }

    #[test]
    fn session_ticket_ngan_han_mot_lan_va_khong_luu_plaintext() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let sessions = WebSocketSessionAuthority::new();
        let ticket = sessions
            .issue(CommandPrincipal::WebSocketDashboard)
            .expect("issue dashboard ticket");
        assert_eq!(ticket.token.len(), 64);
        assert!(ticket.token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        assert!(
            sessions
                .inner
                .lock()
                .unwrap()
                .contains_key(&websocket_session_digest(&ticket.token)),
            "authority phải lưu digest thay vì plaintext ticket"
        );
        assert_eq!(
            websocket_principal(
                loopback,
                Some(&format!("session={}", ticket.token)),
                &sessions,
            )
            .unwrap(),
            CommandPrincipal::WebSocketDashboard
        );
        assert!(
            websocket_principal(
                loopback,
                Some(&format!("session={}", ticket.token)),
                &sessions,
            )
            .is_err(),
            "ticket đã dùng phải bị từ chối replay"
        );
    }

    #[test]
    fn session_ticket_het_han_va_principal_khong_dac_quyen_bi_tu_choi() {
        let loopback = IpAddr::V4(Ipv4Addr::LOCALHOST);
        let sessions = WebSocketSessionAuthority::with_ttl(Duration::ZERO);
        let ticket = sessions
            .issue(CommandPrincipal::WebSocketWidget)
            .expect("issue widget ticket");
        assert!(
            websocket_principal(
                loopback,
                Some(&format!("session={}", ticket.token)),
                &sessions,
            )
            .is_err()
        );
        assert!(sessions.issue(CommandPrincipal::WebSocketRemote).is_err());
        let duplicate = sessions
            .issue(CommandPrincipal::WebSocketWidget)
            .expect("issue duplicate-query ticket");
        assert!(
            websocket_principal(
                loopback,
                Some(&format!(
                    "session={}&session={}",
                    duplicate.token, duplicate.token
                )),
                &sessions,
            )
            .is_err()
        );
    }

    #[test]
    fn legacy_event_cung_bi_phan_quyen() {
        assert!(
            authorize_websocket_event(CommandPrincipal::WebSocketRemote, "get_memory_data")
                .is_err()
        );
        assert!(
            authorize_websocket_event(CommandPrincipal::WebSocketRemote, "user_voice_command")
                .is_ok()
        );
        assert!(
            authorize_websocket_event(CommandPrincipal::WebSocketWidget, "update_config").is_err()
        );
        assert!(
            authorize_websocket_event(CommandPrincipal::WebSocketDashboard, "update_config")
                .is_ok()
        );
    }

    #[test]
    fn wake_probe_dung_nguong_production_da_hieu_chuan_cho_owner() {
        assert!(wake_probe_classifier_direct_accept(Some(0.95), 0.58));
        assert!(wake_probe_classifier_direct_accept(Some(0.641), 0.58));
        assert!(wake_probe_classifier_direct_accept(Some(0.595), 0.58));
        assert!(!wake_probe_classifier_direct_accept(Some(0.372), 0.58));
        assert!(!wake_probe_classifier_direct_accept(Some(0.58), 0.58));
        assert!(!wake_probe_classifier_direct_accept(None, 0.58));
        assert!(!wake_probe_classifier_direct_accept(Some(f32::NAN), 0.58));
    }

    #[test]
    fn wake_probe_ton_trong_nguong_model_cao_hon_nguong_an_toan() {
        assert!(!wake_probe_classifier_direct_accept(Some(0.95), 0.96));
        assert!(wake_probe_classifier_direct_accept(Some(0.97), 0.96));
    }

    /// Khoá đúng chỗ bản cũ làm sai: "chưa nạp model" phải nói là ĐANG NẠP, và
    /// mọi thứ khác mới là "đã xảy ra lỗi".
    ///
    /// Test này tồn tại vì chế độ hỏng cũ **không sinh ra lỗi nào** — LIVA vẫn
    /// trả lời trôi chảy, chỉ là nói sai bản chất, nên không cổng nào bắt được.
    #[test]
    fn chua_nap_model_khong_bi_goi_la_loi() {
        let dang_nap = loi_chat_thanh_cau_noi(Some(crate::llm::engine::ERR_NO_MODEL));
        assert!(
            dang_nap.contains("đang nạp"),
            "phải nói đang nạp, được: {dang_nap}"
        );
        assert!(
            !dang_nap.contains("lỗi"),
            "không được gọi việc khởi động là lỗi, được: {dang_nap}"
        );

        // Chuỗi thật từ engine có thể có tiền tố/hậu tố — so bằng `contains`,
        // nên một lỗi BỌC ERR_NO_MODEL vẫn phải được phân loại là đang nạp.
        let boc = loi_chat_thanh_cau_noi(Some(&format!(
            "llm error: {} (router)",
            crate::llm::engine::ERR_NO_MODEL
        )));
        assert!(boc.contains("đang nạp"), "được: {boc}");
    }

    #[test]
    fn hong_that_van_bao_la_loi() {
        for ca in [
            Some("Failed to load GGUF file"),
            Some("Prompt qua dai: 9000 token"),
            None, // spawn_blocking chết — hỏng thật
        ] {
            let s = loi_chat_thanh_cau_noi(ca);
            assert!(s.contains("đã xảy ra lỗi"), "ca {ca:?} → {s}");
        }
    }
}
