use liva_native_core::{
    AppState, db, env_flag, governor, handle_command, llm, stt, telegram, tts, wake, webrtc,
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{Level, error, info, warn};
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
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });

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

/// Thoát sạch với một chẩn đoán rõ ràng thay vì `panic!`.
///
/// Vì sao (lộ trình 0.6): các `.expect()` lúc boot dựng backtrace Rust — nhiễu,
/// và với người dùng thường thì hoàn toàn không gợi ý được cách khắc phục. Ở
/// đây in một dòng lỗi có hành động cụ thể ra **stderr** (stdout dành cho IPC)
/// rồi `exit(1)`. Vỏ Tauri hiện lỗi này lên dialog là việc follow-up (cần
/// quyết định UI); binary standalone thì stderr + mã thoát ≠ 0 chính là "UI".
fn die(context: &str, err: impl std::fmt::Display) -> ! {
    tracing::error!("KHỞI ĐỘNG THẤT BẠI — {context}: {err}");
    eprintln!("\n❌ LIVA không khởi động được.\n   {context}:\n   {err}\n");
    std::process::exit(1);
}

/// Hướng khắc phục thêm cho lỗi khởi tạo DB, hoặc rỗng nếu không nhận ra.
/// Tách thuần để test được substring-match mà không đụng `process::exit`.
/// Lỗi khởi tạo DB thường quy về một nguyên nhân duy nhất mà thông điệp gốc
/// giấu kín: thiếu `vec0` (sqlite-vec). Bồi thêm hướng khắc phục (dùng chung
/// `liva_native_core::db_error_hint` với vỏ Tauri).
fn die_db(err: impl std::fmt::Display) -> ! {
    let e = err.to_string();
    die(
        &format!(
            "Không khởi tạo được cơ sở dữ liệu{}",
            liva_native_core::db_error_hint(&e)
        ),
        e,
    )
}

async fn async_main() {
    // Initialize tracing to stderr so it doesn't pollute stdout (which is used for IPC)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    // Chưa có logger ở đây nên không dùng `die`; nếu cái này hỏng thì đằng nào
    // cũng không log được gì — panic là hợp lý duy nhất còn lại.
    tracing::subscriber::set_global_default(subscriber)
        .expect("không đặt được tracing subscriber (chỉ xảy ra khi đã có subscriber khác)");

    info!("LIVA Native Core starting up...");

    let db_path = std::env::var("LIVA_DB_PATH")
        .unwrap_or_else(|_| "data/agents/liva_core/structured_memory.sqlite".to_string());

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Mặc định false = DB trên đĩa. KHÔNG dùng `.is_ok()`: nó chỉ hỏi biến có
    // tồn tại hay không, nên `LIVA_DB_IN_MEMORY=false` (đúng như .env.example
    // hướng dẫn) lại bật in-memory và xoá sạch dữ liệu mỗi lần khởi động.
    let is_in_memory = env_flag("LIVA_DB_IN_MEMORY", false);
    let db = if is_in_memory {
        db::DatabasePool::new_in_memory().unwrap_or_else(|e| die_db(e))
    } else {
        db::DatabasePool::new(&db_path).unwrap_or_else(|e| die_db(e))
    };

    // BỎ KHOÁ MẶC ĐỊNH: resolve khoá mã hoá thật (env → khoá thiết bị DPAPI,
    // sinh mới nếu chưa có) rồi rekey facts về nó (cứu dữ liệu đang mã bằng khoá
    // mặc định / KEY_OLD). Thiếu/khoá-chết → fail-fast có chỉ dẫn khôi phục.
    let boot_crypto =
        liva_native_core::resolve_and_rekey(&db, std::path::Path::new(&db_path), is_in_memory)
            .unwrap_or_else(|e| {
                die(
                    "Không thiết lập được khoá mã hoá. Nếu Windows vừa bị cài lại/đổi \
             user, đặt LIVA_ENCRYPTION_KEY = khoá đã sao lưu để khôi phục",
                    e,
                )
            });
    if let Some(hex) = &boot_crypto.escrow_hex {
        // Standalone: escrow ra stderr (stdout dành cho IPC). Vỏ Tauri hiện dialog.
        eprint!("{}", liva_native_core::escrow_message(hex));
    }
    info!(
        "Khoá mã hoá: nguồn={}, rekey {} fact, {} bản khoá-chết (không mất, đọc lại được khi đúng khoá)",
        boot_crypto.source, boot_crypto.rekeyed, boot_crypto.locked
    );

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
        &std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| "models/nemotron-asr".to_string()),
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
    let tts_manager = match tts::TtsManager::from_bin(&tts_model_path, &tts_voice_path, shared_sink)
    {
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
        .unwrap_or_else(|e| die("Không khởi tạo được engine LLM (llama.cpp)", e));

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

    let vault_path = std::env::var("LIVA_VAULT_PATH").unwrap_or_else(|_| {
        "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault".to_string()
    });
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
        &vault_path,
    ));

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
    let denoise_enabled = env_flag("LIVA_DENOISE_ENABLED", true);
    let denoiser = if denoise_enabled {
        let path = webrtc::denoise::resolve_model_path();
        if path.exists() {
            match webrtc::denoise::GtcrnDenoiser::new(&path) {
                Ok(d) => {
                    tracing::info!("GTCRN denoise enabled (model {:?})", path);
                    Some(d)
                }
                Err(e) => {
                    eprintln!(
                        "Failed to initialize GtcrnDenoiser: {}; running without denoise",
                        e
                    );
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
    let turn_shadow = if env_flag("LIVA_TURN_SHADOW_ENABLED", false) {
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
    let aec = if env_flag("LIVA_AEC_ENABLED", false) {
        Some(webrtc::aec::SelfEchoCanceller::new())
    } else {
        None
    };

    // Model embedding cho bộ nhớ dài hạn. Thiếu model KHÔNG phải lỗi chí mạng:
    // recall/persist sẽ bị bỏ qua và hệ thống chạy đúng như trước khi có RAG.
    let embedder = {
        let dir = llm::embedder::resolve_model_dir();
        match llm::embedder::EmbeddingEngine::load(&dir) {
            Ok(e) => {
                info!("Embedding model loaded from {:?} — bo nho dai han BAT", dir);
                Some(e)
            }
            Err(e) => {
                tracing::warn!("Bo nho dai han TAT: {}", e);
                None
            }
        }
    };

    let state = Arc::new(AppState {
        db,
        crypto: boot_crypto.engine,
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
        embedder: tokio::sync::Mutex::new(embedder),
    });

    // (Rekey mã hoá facts đã chạy trong resolve_and_rekey ở trên, trước khi
    // dựng AppState — không cần bước migrate riêng nữa.)

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
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
    use tokio_tungstenite::tungstenite::http::{Response as HttpResponse, StatusCode};

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
            // Từ chối ngay ở tầng HTTP bằng mã lỗi thật, thay vì hoàn tất
            // handshake rồi mới lặng lẽ đóng: trình duyệt nhận 403 và biết vì
            // sao, còn server không tốn công dựng WebSocketStream.
            let reject = |status: StatusCode, msg: &str| -> ErrorResponse {
                HttpResponse::builder()
                    .status(status)
                    .body(Some(msg.to_string()))
                    .expect("static rejection response is always valid")
            };

            // Kiểu Err (ErrorResponse ~136 byte) do chữ ký callback của
            // tungstenite quy định — không box được mà không đổi thư viện.
            #[allow(clippy::result_large_err)]
            let callback = |req: &Request, response: Response| {
                if req.uri().path() != "/ws" {
                    return Err(reject(StatusCode::NOT_FOUND, "invalid path"));
                }
                // WebSocket không chịu Same-Origin Policy — allow-list này là
                // hàng rào duy nhất chống một trang web bất kỳ nối vào 8002.
                let origin = req.headers().get("origin").and_then(|v| v.to_str().ok());
                if !liva_native_core::origin_allowed(origin) {
                    warn!(
                        "WebSocket rejected: origin {:?} không nằm trong allow-list \
                         (mở rộng bằng LIVA_WS_ALLOWED_ORIGINS)",
                        origin.unwrap_or("<none>")
                    );
                    return Err(reject(StatusCode::FORBIDDEN, "origin not allowed"));
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
    let memory_scope =
        liva_native_core::agent::graph::ConversationMemoryScope::new("local", &conversation_id)
            .expect("WebSocket conversation id must be valid");
    let voice_session =
        crate::webrtc::session::VoiceSessionAudio::from_app_state(state.as_ref()).await;
    let (pipeline_handle, actor) = crate::webrtc::pipeline::WebRTCActor::new(
        state.clone(),
        crate::webrtc::pipeline::VoiceOutbound::new(speaker_tx.clone(), control_tx.clone()),
        conversation_id.clone(),
        voice_session.aec_handle(),
    );
    let actor_handle = tokio::spawn(actor.run());

    enum DataMessage {
        Speaker(Option<VoiceFrame>),
        Text(Option<String>),
    }

    // One socket writer: control is strict priority; speaker/text remain fair.
    let send_task = tokio::spawn(async move {
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
    });

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
                                        liva_native_core::agent::graph::recall_context_scoped(
                                            &state_clone,
                                            &user_text,
                                            &memory_scope,
                                        )
                                        .await
                                    {
                                        messages.push(crate::llm::ChatMessage {
                                                role: "system".to_string(),
                                                content: liva_native_core::agent::graph::memory_system_message(&memories),
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
                                        liva_native_core::agent::graph::persist_turn_scoped(
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
                                    match liva_native_core::handle_chat_completion_scoped(
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

#[cfg(test)]
mod tests {
    use super::*;
    use liva_native_core::crypto;

    /// Lộ trình 0.6: lỗi thiếu vec0 phải kèm hướng khắc phục npm ci; lỗi khác
    /// thì không bịa gợi ý.
    #[test]
    fn goi_y_loi_db() {
        assert!(liva_native_core::db_error_hint("no such module: vec0").contains("npm ci"));
        assert!(
            liva_native_core::db_error_hint("khong nap duoc sqlite-vec (vec0.dll)")
                .contains("npm ci")
        );
        assert_eq!(
            liva_native_core::db_error_hint("disk I/O error"),
            "",
            "loi khac khong bia goi y"
        );
        assert_eq!(liva_native_core::db_error_hint(""), "");
    }

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
            mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
                "test_vault",
            )),
            embedder: tokio::sync::Mutex::new(None),
            vision: tokio::sync::Mutex::new(vision_manager),
        })
    }

    #[tokio::test]
    async fn two_websockets_keep_handshakes_isolated() {
        use bytes::{Bytes, BytesMut};
        use futures_util::{SinkExt, StreamExt};
        use liva_native_core::webrtc::aec::SelfEchoCanceller;
        use liva_native_core::webrtc::frame::{OP_AUTH_HANDSHAKE, VoiceFrame};
        use tokio_tungstenite::{accept_async, connect_async, tungstenite::Message};

        let state = test_state();
        *state.aec.lock().await = Some(SelfEchoCanceller::new());

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind test WebSocket listener");
        let address = listener.local_addr().expect("read test listener address");
        let server_state = Arc::clone(&state);
        let server = tokio::spawn(async move {
            let mut handlers = Vec::new();
            for _ in 0..2 {
                let (stream, _) = listener.accept().await.expect("accept test client");
                let websocket = accept_async(stream).await.expect("upgrade test client");
                let connection_state = Arc::clone(&server_state);
                handlers.push(tokio::spawn(async move {
                    handle_ws_connection(websocket, connection_state).await
                }));
            }

            for handler in handlers {
                handler
                    .await
                    .expect("join WebSocket handler")
                    .expect("run WebSocket handler");
            }
        });

        let url = format!("ws://{address}/ws");
        let (mut client_a, _) = connect_async(&url).await.expect("connect client A");
        let (mut client_b, _) = connect_async(&url).await.expect("connect client B");
        let expected = [
            (41, Bytes::from_static(b"client-a")),
            (73, Bytes::from_static(b"client-b")),
        ];

        client_a
            .send(Message::Binary(
                VoiceFrame {
                    op_code: OP_AUTH_HANDSHAKE,
                    seq_id: expected[0].0,
                    payload: expected[0].1.clone(),
                }
                .encode()
                .expect("encode client A frame")
                .to_vec(),
            ))
            .await
            .expect("send client A frame");
        client_b
            .send(Message::Binary(
                VoiceFrame {
                    op_code: OP_AUTH_HANDSHAKE,
                    seq_id: expected[1].0,
                    payload: expected[1].1.clone(),
                }
                .encode()
                .expect("encode client B frame")
                .to_vec(),
            ))
            .await
            .expect("send client B frame");

        for (client, (seq_id, payload)) in
            [(&mut client_a, &expected[0]), (&mut client_b, &expected[1])]
        {
            let message = tokio::time::timeout(std::time::Duration::from_secs(2), client.next())
                .await
                .expect("handshake response timeout")
                .expect("WebSocket closed before handshake response")
                .expect("receive handshake response");
            let Message::Binary(data) = message else {
                panic!("handshake response must be binary");
            };
            let frame = VoiceFrame::decode(&mut BytesMut::from(data.as_slice()))
                .expect("decode handshake response")
                .expect("complete handshake response");

            assert_eq!(frame.op_code, OP_AUTH_HANDSHAKE);
            assert_eq!(frame.seq_id, *seq_id);
            assert_eq!(frame.payload, *payload);
        }

        client_a.close(None).await.expect("close client A");
        client_b.close(None).await.expect("close client B");
        tokio::time::timeout(std::time::Duration::from_secs(2), server)
            .await
            .expect("server shutdown timeout")
            .expect("join test server");
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
        let res = handle_command(
            state.clone(),
            "vision:add_region",
            region_payload,
            None,
            None,
        )
        .await;
        assert!(res.is_ok());

        // 2. Set config
        let config_payload = serde_json::json!({
            "color_tolerance": 10,
            "max_regions": 10
        });
        let res = handle_command(
            state.clone(),
            "vision:set_config",
            config_payload,
            None,
            None,
        )
        .await;
        assert!(res.is_ok());

        // 3. Get changed regions (first time - baseline when last_frame is None)
        let res = handle_command(
            state.clone(),
            "vision:get_changed_regions",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert!(val.is_array());
        let arr = val.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["region_id"], "r1");
        assert_eq!(arr[0]["difference"], 1.0); // Baseline first frame
        assert_eq!(arr[0]["is_changed"], true);

        // 4. Capture screen
        let res = handle_command(
            state.clone(),
            "vision:capture",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val["width"], 1920);
        assert_eq!(val["height"], 1080);
        assert!(val["data"].is_string());

        // 5. Remove region
        let remove_payload = serde_json::json!({ "id": "r1" });
        let res = handle_command(
            state.clone(),
            "vision:remove_region",
            remove_payload,
            None,
            None,
        )
        .await;
        assert!(res.is_ok());
    }

    // ── Bảng lệnh handle_command (lộ trình 3.7) ─────────────────────────────
    // Lớp dispatch là chỗ một nhánh `Err` từng bị nuốt mà không ai biết vì mọi
    // test khác gọi thẳng hàm con, không đi qua đây. Các test dưới phủ những
    // nhánh KHÔNG cần model đã nạp: round-trip trí nhớ (qua chính lớp mã hoá),
    // các chốt bảo mật C2 của swap_model, và mcp/integrations.

    /// memory:set_fact → memory:get_fact round-trip QUA dispatcher: khẳng định
    /// value được mã hoá lúc ghi và giải mã lại đúng lúc đọc (đường
    /// `decrypt_read`), không chỉ ở hàm con mà xuyên suốt lệnh.
    #[tokio::test]
    async fn cmd_memory_set_roi_get_fact_round_trip() {
        let state = test_state();
        let fact = serde_json::json!({
            "key": "ten_meo", "value": "Bún — bí mật cần mã hoá",
            "createdAt": "2026-07-22T00:00:00Z", "updatedAt": "2026-07-22T00:00:00Z",
            "ttlDays": 30, "source": "test", "category": "pet",
            "importance": 0.9, "confidenceScore": 1.0, "sourceTurnId": null,
            "memory_strength": 1.0, "last_accessed_at": 0, "access_count": 0
        });
        let set = handle_command(state.clone(), "memory:set_fact", fact, None, None).await;
        assert_eq!(set.unwrap(), serde_json::json!({ "success": true }));

        let got = handle_command(
            state.clone(),
            "memory:get_fact",
            serde_json::json!({ "key": "ten_meo" }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(got["key"], "ten_meo");
        assert_eq!(
            got["value"], "Bún — bí mật cần mã hoá",
            "value phải giải mã lại đúng qua lớp lệnh"
        );

        // Khoá không tồn tại → Null, không lỗi.
        let missing = handle_command(
            state.clone(),
            "memory:get_fact",
            serde_json::json!({ "key": "khong_co" }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(missing, serde_json::Value::Null);
    }

    /// Payload méo phải trả `Err` gọn, KHÔNG panic (parser nhị phân/JSON là đầu
    /// vào không tin cậy từ bất kỳ ai nối được WebSocket).
    #[tokio::test]
    async fn cmd_memory_payload_meo_tra_err_khong_panic() {
        let state = test_state();
        // get_fact thiếu 'key'.
        let e = handle_command(
            state.clone(),
            "memory:get_fact",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(e.is_err() && e.unwrap_err().contains("key"));
        // set_fact thiếu trường bắt buộc.
        let e = handle_command(
            state.clone(),
            "memory:set_fact",
            serde_json::json!({ "key": "x" }),
            None,
            None,
        )
        .await;
        assert!(e.is_err(), "Fact thiếu trường phải Err, không panic");
    }

    /// C2 (lộ trình 0.4): `llm:swap_model` phải TỪ CHỐI đường dẫn độc TRƯỚC khi
    /// chạm parser C++ của llama.cpp — không cần model nào được nạp để kiểm.
    #[tokio::test]
    async fn cmd_swap_model_chan_path_doc_c2() {
        let state = test_state();
        // Thiếu model_path.
        let e = handle_command(
            state.clone(),
            "llm:swap_model",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(e.is_err() && e.unwrap_err().contains("model_path"));
        // Có '..' → chặn traversal.
        let e = handle_command(
            state.clone(),
            "llm:swap_model",
            serde_json::json!({ "model_path": "../evil.gguf" }),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err() && e.unwrap_err().contains(".."),
            "phải chặn '..'"
        );
        // Sai đuôi → không cho nạp file tuỳ ý vào parser C++.
        let e = handle_command(
            state.clone(),
            "llm:swap_model",
            serde_json::json!({ "model_path": "evil.txt" }),
            None,
            None,
        )
        .await;
        assert!(e.is_err() && e.unwrap_err().contains(".gguf"), "chỉ .gguf");
    }

    /// mcp:list_tools trả danh sách tool; mcp:call_tool thiếu 'name' → Err gọn.
    #[tokio::test]
    async fn cmd_mcp_list_va_call_tool() {
        let state = test_state();
        let tools = handle_command(
            state.clone(),
            "mcp:list_tools",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .unwrap();
        // ToolList serialize thành object { "tools": [...] }.
        let list = tools["tools"]
            .as_array()
            .expect("mcp:list_tools trả { tools: [...] }");
        assert!(!list.is_empty(), "phải liệt kê ít nhất một tool");

        let e = handle_command(
            state.clone(),
            "mcp:call_tool",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err() && e.unwrap_err().contains("name"),
            "thiếu tên tool phải báo rõ"
        );
    }

    /// integrations:list là dữ liệu tĩnh, luôn trả mảng metadata.
    #[tokio::test]
    async fn cmd_integrations_list() {
        let state = test_state();
        let val = handle_command(
            state,
            "integrations:list",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(val.is_array() && !val.as_array().unwrap().is_empty());
    }

    /// Vòng đời task CRUD qua dispatcher: add (id tường minh) → get thấy →
    /// delete → get không còn. Khoá bảng `tasks` đi qua đúng lớp lệnh.
    #[tokio::test]
    async fn cmd_task_crud_lifecycle() {
        let state = test_state();
        let add = handle_command(
            state.clone(),
            "add_task",
            serde_json::json!({ "id": "t-1", "title": "Mua cá cho Bún", "priority": "high" }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(add["success"], true);
        assert_eq!(add["id"], "t-1");

        let list = handle_command(
            state.clone(),
            "get_tasks",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .unwrap();
        let tasks = list["tasks"]
            .as_array()
            .expect("get_tasks trả { tasks: [...] }");
        assert!(
            tasks
                .iter()
                .any(|t| t["id"] == "t-1" && t["title"] == "Mua cá cho Bún"),
            "task vừa thêm phải xuất hiện"
        );

        let del = handle_command(
            state.clone(),
            "delete_task",
            serde_json::json!({ "id": "t-1" }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(del["success"], true);
        let list2 = handle_command(
            state.clone(),
            "get_tasks",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            list2["tasks"]
                .as_array()
                .unwrap()
                .iter()
                .all(|t| t["id"] != "t-1"),
            "sau delete phải biến mất"
        );
    }

    /// Payload task méo → `Err` gọn, không panic.
    #[tokio::test]
    async fn cmd_task_payload_meo_tra_err() {
        let state = test_state();
        let e = handle_command(state.clone(), "add_task", serde_json::json!({}), None, None).await;
        assert!(
            e.is_err() && e.unwrap_err().contains("title"),
            "add thiếu title → Err"
        );
        let e = handle_command(
            state.clone(),
            "delete_task",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err() && e.unwrap_err().contains("id"),
            "delete thiếu id → Err"
        );
    }

    /// memory:search_hybrid — ba nhánh KHÔNG cần model đã nạp:
    /// (1) thiếu `query_text` → Err; (2) không `query_vector` và không embedder →
    /// Err CÓ HƯỚNG KHẮC PHỤC, KHÔNG panic (khoá hợp đồng suy-giảm-an-toàn của
    /// 2.2 — thiếu model thì báo rõ, không sập); (3) tự cấp vector 384 chiều →
    /// chạy search thật trên DB rỗng → mảng rỗng.
    #[tokio::test]
    async fn cmd_search_hybrid_khong_can_model() {
        let state = test_state(); // embedder = None

        let e = handle_command(
            state.clone(),
            "memory:search_hybrid",
            serde_json::json!({}),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err() && e.unwrap_err().contains("query_text"),
            "thiếu query_text → Err"
        );

        let e = handle_command(
            state.clone(),
            "memory:search_hybrid",
            serde_json::json!({
                "query_text": "mèo tên gì",
                "filter": { "type": "fact" }
            }),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err(),
            "không embedder + không vector phải trả Err, không panic"
        );
        assert!(
            e.unwrap_err().to_lowercase().contains("model"),
            "Err phải chỉ cách khắc phục (nạp model embedding)"
        );

        let mut v = vec![0.0f32; 384];
        v[0] = 1.0;
        let got = handle_command(
            state.clone(),
            "memory:search_hybrid",
            serde_json::json!({
                "query_text": "x",
                "query_vector": v,
                "filter": { "type": "fact" }
            }),
            None,
            None,
        )
        .await
        .unwrap();
        assert!(
            got.is_array() && got.as_array().unwrap().is_empty(),
            "DB rỗng + vector tự cấp → không kết quả, không lỗi"
        );
    }

    /// Lệnh tìm kiếm thô không có identity phía server phải fail-closed với
    /// `conversation_turn`: client không được tự khai domain của owner khác.
    #[tokio::test]
    async fn cmd_search_hybrid_khong_identity_chan_conversation_turn() {
        let state = test_state();
        let vector = vec![0.0f32; 384];

        let missing_filter = handle_command(
            state.clone(),
            "memory:search_hybrid",
            serde_json::json!({ "query_text": "bí mật", "query_vector": vector }),
            None,
            None,
        )
        .await
        .expect_err("thiếu type phải bị chặn vì có thể đọc conversation_turn");
        assert!(
            missing_filter
                .to_lowercase()
                .contains("authenticated owner"),
            "lỗi phải nêu yêu cầu owner do server xác thực: {missing_filter}"
        );

        let forged_owner = handle_command(
            state,
            "memory:search_hybrid",
            serde_json::json!({
                "query_text": "bí mật",
                "query_vector": vec![0.0f32; 384],
                "filter": {
                    "type": "conversation_turn",
                    "domain": "memory_owner:telegram:other"
                }
            }),
            None,
            None,
        )
        .await
        .expect_err("client không được tự khai owner để đọc conversation_turn");
        assert!(
            forged_owner.to_lowercase().contains("authenticated owner"),
            "lỗi phải nêu yêu cầu owner do server xác thực: {forged_owner}"
        );
    }

    /// memory:upsert_vector — khoá GUARD CHIỀU của 2.3 tại lớp lệnh: vector sai
    /// chiều bị chặn với thông điệp nêu 384 (không ghi rác vào `vec_idx`), đúng
    /// 384 chiều thì ghi được; thiếu trường bắt buộc → `Err` gọn.
    #[tokio::test]
    async fn cmd_upsert_vector_guard_chieu_2_3() {
        let state = test_state();
        // Thiếu 'vector'.
        let msg = handle_command(
            state.clone(),
            "memory:upsert_vector",
            serde_json::json!({ "vecId": "v1", "type": "fact", "content": "x" }),
            None,
            None,
        )
        .await
        .expect_err("thiếu vector → Err");
        assert!(msg.contains("vector"), "phải nêu thiếu vector: {msg}");

        // Sai chiều (3 thay vì 384) → guard 2.3 chặn, thông điệp nêu 384.
        let msg = handle_command(state.clone(), "memory:upsert_vector",
            serde_json::json!({ "vecId": "v1", "type": "fact", "content": "x", "vector": [0.1, 0.2, 0.3] }),
            None, None).await.expect_err("vector lệch chiều phải Err");
        assert!(
            msg.contains("384"),
            "thông điệp guard phải nêu 384 chiều: {msg}"
        );

        // Đúng 384 chiều → ghi được.
        let v = vec![0.0f32; 384];
        let ok = handle_command(
            state.clone(),
            "memory:upsert_vector",
            serde_json::json!({ "vecId": "v1", "type": "fact", "content": "x", "vector": v }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(ok["success"], true);
    }

    /// Chèn thẳng một fact mã hoá bằng khoá KHÁC (locked dưới khoá test).
    fn chen_fact_locked(state: &std::sync::Arc<AppState>, key: &str) {
        let other = crypto::EncryptionEngine::new("khoa-khac-han-1234567890abcdef");
        let locked = other.encrypt("bí mật sau khoá sai").unwrap();
        let conn = state.db.writer.get().unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?1, ?2, 'd','d','t')",
            (key, &locked),
        ).unwrap();
    }

    /// delete_memory_fact FAIL-CLOSED: từ chối xoá hàng locked (ở tầng lệnh, cả
    /// caller không-UI), nhưng xoá bình thường hàng đọc được.
    #[tokio::test]
    async fn cmd_delete_memory_fact_tu_choi_locked() {
        let state = test_state();
        chen_fact_locked(&state, "locked");

        let e = handle_command(
            state.clone(),
            "delete_memory_fact",
            serde_json::json!({ "key": "locked" }),
            None,
            None,
        )
        .await;
        assert!(
            e.is_err() && e.unwrap_err().contains("KHOÁ"),
            "phải từ chối xoá hàng locked"
        );
        let con: i64 = state
            .db
            .readers
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM facts WHERE key='locked'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(con, 1, "hàng locked KHÔNG được xoá");

        // Fact đọc được (mã hoá bằng khoá test) → xoá OK.
        let val = state.crypto.encrypt("bình thường").unwrap();
        state.db.writer.get().unwrap().execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('ok', ?1, 'd','d','t')",
            [&val],
        ).unwrap();
        let ok = handle_command(
            state.clone(),
            "delete_memory_fact",
            serde_json::json!({ "key": "ok" }),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(ok["success"], true);
        let gone: i64 = state
            .db
            .readers
            .get()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM facts WHERE key='ok'", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(gone, 0, "fact đọc được phải xoá thành công");
    }

    /// get_memory_data gắn cờ `locked` per-fact + `lockedFactsCount`, value locked
    /// = "" (không rò ciphertext), KHÔNG rớt hàng.
    #[tokio::test]
    async fn cmd_get_memory_data_gan_co_locked() {
        let state = test_state();
        chen_fact_locked(&state, "lk");
        let val = state.crypto.encrypt("đọc được").unwrap();
        state.db.writer.get().unwrap().execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('ok', ?1, 'd','d','t')",
            [&val],
        ).unwrap();

        let data = handle_command(
            state.clone(),
            "get_memory_data",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .unwrap();
        assert_eq!(data["lockedFactsCount"], 1);
        let facts = data["facts"].as_array().unwrap();
        assert_eq!(facts.len(), 2, "không rớt hàng locked");
        let lk = facts.iter().find(|f| f["key"] == "lk").unwrap();
        assert_eq!(lk["locked"], true);
        assert_eq!(lk["value"], "", "locked -> value rỗng, không rò ciphertext");
        let ok = facts.iter().find(|f| f["key"] == "ok").unwrap();
        assert_eq!(ok["locked"], false);
        assert_eq!(ok["value"], "đọc được");
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
            .unwrap_or_else(|| {
                std::thread::available_parallelism()
                    .map(|n| n.get())
                    .unwrap_or(4)
            });

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
