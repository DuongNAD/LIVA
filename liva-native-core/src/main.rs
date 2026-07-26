use liva_native_core::{
    AppState, db, env_flag, governor, handle_command, llm, stt, telegram, tts, webrtc,
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

async fn stop_background_tasks(tasks: Vec<tokio::task::JoinHandle<()>>) {
    for task in &tasks {
        task.abort();
    }
    for task in tasks {
        match task.await {
            Ok(()) => {}
            Err(error) if error.is_cancelled() => {}
            Err(error) => {
                tracing::warn!(%error, "background service stopped with an error");
            }
        }
    }
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

    let voice_components = webrtc::session::VoiceRuntimeComponents::from_env(&stt_model_dir);

    let state = Arc::new(AppState {
        db,
        crypto: boot_crypto.engine,
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(voice_components.vad),
        denoiser: tokio::sync::Mutex::new(voice_components.denoiser),
        turn_shadow: tokio::sync::Mutex::new(voice_components.turn_shadow),
        aec: tokio::sync::Mutex::new(voice_components.aec),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: tokio::sync::Mutex::new(embedder),
    });

    let mut background_tasks = Vec::new();

    // Finalize the atomic event→vector projection off the chat hot path.
    // The worker uses the single SQLite writer, bounded batches and a 3-strike DLQ.
    background_tasks
        .push(liva_native_core::memory_consolidation::spawn_projection_consumer(state.db.clone()));

    // (Rekey mã hoá facts đã chạy trong resolve_and_rekey ở trên, trước khi
    // dựng AppState — không cần bước migrate riêng nữa.)

    // Autoload the configured router LLM in the background so chat works
    // without a manual llm:swap_model call.
    let state_llm = state.clone();
    background_tasks.push(tokio::spawn(async move {
        liva_native_core::load_configured_router_model(state_llm, false).await;
    }));

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
        background_tasks.push(tokio::spawn(async move {
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
        }));
    }

    // Spawn WebRTC/IPC WebSocket server
    let state_ws = state.clone();
    background_tasks.push(tokio::spawn(async move {
        match liva_native_core::websocket::WebSocketServer::bind_from_env().await {
            Ok(server) => {
                if let Err(error) = server.run(state_ws).await {
                    error!("WebSocket server error: {error}");
                }
            }
            Err(error) => {
                error!("WebSocket server bind error: {error}");
            }
        }
    }));

    // Spawn background task for idle TTS model unloading
    let state_unload_clone = state.clone();
    background_tasks.push(tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let tts_opt = state_unload_clone.tts.lock().await;
            if let Some(ref tts_mgr) = *tts_opt {
                tts_mgr.check_idle_unload();
            }
        }
    }));

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

        background_tasks.push(tokio::spawn(async move {
            let manager = Arc::new(telegram::TelegramBotManager::new(
                token,
                allowed_ids,
                state_tg,
                Some(tx_tg),
            ));
            manager.start().await;
        }));
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

    // Stop every process-owned service before closing stdout. Telegram owns a
    // sender for its whole polling lifetime; leaving it detached would keep
    // `rx` open forever after EOF. The other handles are drained here as well
    // so model, WebSocket and projection resources do not rely on runtime drop.
    stop_background_tasks(background_tasks).await;

    // Drop the main sender so rx knows no more messages are coming after all processing tasks finish
    drop(tx);

    // Wait for writer task to finish writing all pending responses
    let _ = writer_handle.await;

    info!("LIVA Native Core shutting down...");
}

#[cfg(test)]
mod tests {
    use super::*;
    use liva_native_core::crypto;

    #[tokio::test]
    async fn shutdown_aborts_service_holding_stdout_sender() {
        let (tx, mut rx) = mpsc::channel::<String>(1);
        let service = tokio::spawn(async move {
            let _held_sender = tx;
            std::future::pending::<()>().await;
        });

        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            stop_background_tasks(vec![service]),
        )
        .await
        .expect("service shutdown must not hang");

        let closed = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .expect("receiver must not hang after service shutdown");
        assert!(closed.is_none(), "service must release its sender");
    }

    #[tokio::test]
    async fn shutdown_aborts_every_owned_background_service() {
        let (tx, mut rx) = mpsc::channel::<String>(1);
        let first_tx = tx.clone();
        let first = tokio::spawn(async move {
            let _held_sender = first_tx;
            std::future::pending::<()>().await;
        });
        let second = tokio::spawn(async move {
            let _held_sender = tx;
            std::future::pending::<()>().await;
        });

        tokio::time::timeout(
            std::time::Duration::from_millis(100),
            stop_background_tasks(vec![first, second]),
        )
        .await
        .expect("all owned services must stop without hanging");

        let closed = tokio::time::timeout(std::time::Duration::from_millis(100), rx.recv())
            .await
            .expect("receiver must close after every service stops");
        assert!(closed.is_none(), "all service senders must be released");
    }

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
