pub mod agent;
mod artifact_trust;
mod authorization;
pub mod boot;
pub mod cognitive;
pub mod commands;
pub mod consent;
pub mod crypto;
pub mod db;
pub mod eval;
#[cfg(feature = "experimental")]
pub mod evolution;
pub mod governor;
pub mod integrations;
pub mod keystore;
pub mod llm;
pub mod mcp;
pub mod memory_consolidation;
pub mod memory_retention;
pub mod messaging;
pub mod openai_api;
#[cfg(feature = "experimental")]
pub mod passive;
mod paths;
pub mod persistence_backup;
pub mod preflight;
pub mod setup;
pub mod skills;
pub mod stt;
pub mod sysinfo;
mod system_status;
pub mod telegram;
pub mod tts;
pub mod vision;
pub mod wake;
pub mod wake_model;
pub mod webrtc;
pub mod websocket;

pub use artifact_trust::{
    embedded_file_hash, embedded_model_hash, embedded_runtime_artifact_hash, verify_model_artifact,
    verify_trusted_file,
};
pub use authorization::{CommandPrincipal, authorize_command};
pub use crypto::EncryptionEngine;
pub use db::DatabasePool;
pub use llm::LlamaRouterManager;
pub(crate) use paths::update_config_file_at;
pub use paths::{
    APP_DATA_DIR_NAME, DEFAULT_EXPERT_MODEL, DEFAULT_MODELS_DIR, DEFAULT_ROUTER_MODEL,
    config_file_path, configured_mmproj_path, configured_models_dir, configured_router_model_path,
    data_dir, default_vault_path, exe_dir, models_dir_fallback, resolve_resource_path,
    resource_candidate_paths, resource_write_root, stray_database_paths, user_home_dir,
    validate_model_path,
};
use std::sync::Arc;
pub use stt::SttManager;
pub use system_status::system_status;
pub use tts::TtsManager;
pub use tts::audio::TtsAudioPlayer;
pub use vision::{
    VisionConfig, VisionManager,
    capture::{Frame, PixelFormat, ScreenCapturer},
    diff::{DiffEngine, RegionDiffResult, ScreenRegion},
};

pub struct AppState {
    pub db: DatabasePool,
    pub crypto: EncryptionEngine,
    pub stt: tokio::sync::Mutex<SttManager>,
    pub tts: tokio::sync::Mutex<Option<TtsManager>>,
    pub tts_player: TtsAudioPlayer,
    pub llm: tokio::sync::Mutex<LlamaRouterManager>,
    pub vad: tokio::sync::Mutex<Option<webrtc::vad::VadEngine>>,
    pub denoiser: tokio::sync::Mutex<Option<webrtc::denoise::GtcrnDenoiser>>,
    pub turn_shadow: tokio::sync::Mutex<Option<webrtc::turn_shadow::SmartTurnClassifier>>,
    pub aec: tokio::sync::Mutex<Option<webrtc::aec::SelfEchoCanceller>>,
    pub mcp_server: Arc<mcp::server::NativeMcpServer>,
    pub vision: tokio::sync::Mutex<VisionManager>,
    /// Model embedding chuyên dụng cho bộ nhớ dài hạn (RAG).
    ///
    /// `None` khi chưa tải model về — khi đó recall/persist bị bỏ qua và hệ
    /// thống hành xử **đúng như trước khi có RAG**, không lỗi. Xem
    /// `llm::embedder` để biết vì sao nó tách khỏi model chat.
    pub embedder: tokio::sync::Mutex<Option<llm::embedder::EmbeddingEngine>>,
}

#[derive(serde::Serialize)]
struct IpcTokenChunkData<'a> {
    token: &'a str,
    done: bool,
}

#[derive(serde::Serialize)]
struct IpcTokenChunkRef<'a> {
    id: &'a str,
    status: &'static str,
    data: IpcTokenChunkData<'a>,
}

/// Đọc một biến môi trường dạng cờ bật/tắt.
///
/// Chấp nhận (không phân biệt hoa thường, bỏ khoảng trắng thừa):
/// - bật : `1`, `true`, `yes`, `on`
/// - tắt : `0`, `false`, `no`, `off`
/// - biến không tồn tại, rỗng, hoặc giá trị lạ → trả `default`
///
/// Vì sao cần: trước đây mỗi nơi tự đọc một kiểu. `LIVA_DB_IN_MEMORY` dùng
/// `.is_ok()` nên `LIVA_DB_IN_MEMORY=false` — đúng y như `.env.example` hướng
/// dẫn — lại bật DB in-memory và **xoá sạch dữ liệu người dùng mỗi lần khởi
/// động**. Các cờ khác thì chỉ nhận đúng chuỗi `"1"`, ai viết `=true` bị âm
/// thầm bỏ qua. Một hàm duy nhất diệt cả lớp lỗi đó.
///
/// Giá trị lạ trả `default` thay vì panic: một biến gõ sai không đáng làm hỏng
/// cả tiến trình, nhưng cũng không được im lặng đổi hành vi.
pub fn env_flag(key: &str, default: bool) -> bool {
    match std::env::var(key) {
        Ok(raw) => match raw.trim().to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "on" => true,
            "0" | "false" | "no" | "off" => false,
            "" => default,
            other => {
                tracing::warn!(
                    "{}=\"{}\" không phải giá trị bật/tắt hợp lệ (1/true/yes/on hoặc 0/false/no/off); dùng mặc định {}",
                    key,
                    other,
                    default
                );
                default
            }
        },
        Err(_) => default,
    }
}

/// Filter log cho `tracing`, đọc từ `RUST_LOG`.
///
/// Dùng CHUNG cho cả `main.rs` (gateway) lẫn vỏ Tauri để không trôi dạt — cùng
/// lý do [`resolve_and_rekey`] nằm ở đây.
///
/// ## Vì sao cần
///
/// Trước 26/07/2026 cả hai chỗ đều dựng subscriber bằng
/// `.with_max_level(Level::INFO)` **cứng**, không có `EnvFilter`. Hệ quả không ai
/// để ý: `RUST_LOG` bị bỏ qua hoàn toàn, nên **mọi `debug!` trong crate này là
/// code chết** — không bao giờ hiện ra ở bất kỳ cấu hình nào. Phát hiện khi kiểm
/// MCP client với server ngoài thật: server con crash, in stack trace ra stderr,
/// drain đọc được, mà log tuyệt đối im (xem [`mcp::client`]).
///
/// ## Hành vi
///
/// - `RUST_LOG` không đặt → `info`, **giữ đúng hành vi cũ**, không phải thay đổi
///   ngầm cho ai đang chạy.
/// - `RUST_LOG` đặt và hợp lệ → dùng nguyên.
/// - `RUST_LOG` đặt nhưng SAI cú pháp → `eprintln!` cảnh báo rồi rơi về `info`.
///   Không im lặng bỏ qua: một biến gõ sai đổi hành vi log mà không nói gì là
///   đúng loại bẫy đã sinh ra chính hàm này. Chưa có logger ở thời điểm gọi nên
///   phải dùng `eprintln!`.
///
/// Lưu ý cú pháp `EnvFilter`: directive tường minh **thay thế** mặc định, nên
/// `RUST_LOG=liva_native_core::mcp=debug` cho **chỉ** mcp ở debug và tắt phần
/// còn lại. Muốn giữ cả info thì viết
/// `RUST_LOG=info,liva_native_core::mcp=debug`.
pub fn tracing_env_filter() -> tracing_subscriber::EnvFilter {
    const MAC_DINH: &str = "info";
    match std::env::var("RUST_LOG") {
        Ok(raw) if !raw.trim().is_empty() => match tracing_subscriber::EnvFilter::try_new(&raw) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("RUST_LOG=\"{raw}\" sai cú pháp ({e}); dùng mặc định \"{MAC_DINH}\"");
                tracing_subscriber::EnvFilter::new(MAC_DINH)
            }
        },
        _ => tracing_subscriber::EnvFilter::new(MAC_DINH),
    }
}

/// Kết quả resolve khoá mã hoá lúc boot (xem [`resolve_and_rekey`]).
pub struct BootKey {
    /// Engine mã hoá THẬT để đặt vào `AppState.crypto`.
    pub engine: EncryptionEngine,
    /// `Some(hex)` nếu khoá vừa được SINH mới ⇒ boot phải ESCROW (hiện 1 lần
    /// cho người dùng sao lưu, vì DPAPI là điểm hỏng đơn). `None` nếu lấy từ env
    /// hoặc keystore đã có.
    pub escrow_hex: Option<String>,
    /// Số fact được mã hoá lại về khoá hiện tại (từ khoá mặc định / KEY_OLD).
    pub rekeyed: usize,
    /// Số fact KHÔNG khoá nào mở được (khoá-chết) — để cảnh báo, không mất.
    pub locked: usize,
    /// Số checkpoint + conversation turn được mã hóa mới hoặc đổi sang khóa hiện tại.
    pub personal_data_rekeyed: usize,
    /// Số checkpoint + conversation turn không khóa nào mở được; bản gốc được giữ nguyên.
    pub personal_data_locked: usize,
    /// Nguồn khoá, để log: `"env"` | `"device-key"` | `"device-key (mới)"` | `"in-memory"`.
    pub source: &'static str,
}

/// Resolve khoá mã hoá THẬT lúc boot (BỎ KHOÁ MẶC ĐỊNH) rồi rekey facts về nó.
///
/// Dùng CHUNG cho cả `main.rs` (gateway) lẫn vỏ Tauri để không trôi dạt (M4).
///
/// Thứ tự khoá:
/// 1. `LIVA_ENCRYPTION_KEY` nếu set và **≠ mặc định** → dùng nguyên
///    (power-user/CI/khôi phục); không đụng keystore, không escrow.
/// 2. ngược lại (chưa set, HOẶC == mặc định) → **khoá thiết bị DPAPI**
///    ([`keystore::load_or_create_device_key`]); sinh mới nếu chưa có (→ escrow).
///
/// Khoá MẶC ĐỊNH `"0"×32` KHÔNG bao giờ là khoá GHI: `== mặc định` bị coi như
/// chưa set. Nhưng nó (và `LIVA_ENCRYPTION_KEY_OLD`) làm **khoá phụ để CỨU**
/// dữ liệu: rekey giải bằng chúng rồi mã lại dưới khoá live — nên máy đang chạy
/// khoá mặc định nâng cấp lên là facts tự chuyển sang khoá thật, không mất.
///
/// `in_memory=true` (test/CI, `LIVA_DB_IN_MEMORY=1`): không có dữ liệu-at-rest
/// nên KHÔNG sinh khoá thiết bị/DPAPI — dùng thẳng env (cho phép cả mặc định).
pub fn resolve_and_rekey(
    db: &DatabasePool,
    db_path: &std::path::Path,
    in_memory: bool,
) -> Result<BootKey, String> {
    let env_key = std::env::var("LIVA_ENCRYPTION_KEY")
        .ok()
        .filter(|k| !k.is_empty());
    let real_env = env_key
        .clone()
        .filter(|k| k != crypto::DEFAULT_ENCRYPTION_KEY);

    let (passphrase, escrow_hex, source) = if let Some(k) = real_env {
        (k, None, "env")
    } else if in_memory {
        // Không có dữ liệu-at-rest → cho phép env (kể cả mặc định), không DPAPI.
        (
            env_key.unwrap_or_else(|| crypto::DEFAULT_ENCRYPTION_KEY.to_string()),
            None,
            "in-memory",
        )
    } else {
        let (hex, generated) = keystore::load_or_create_device_key(db_path)
            .map_err(|e| format!("không lấy được khoá thiết bị: {e}"))?;
        let escrow = if generated { Some(hex.clone()) } else { None };
        (
            hex,
            escrow,
            if generated {
                "device-key (mới)"
            } else {
                "device-key"
            },
        )
    };

    let live = EncryptionEngine::new(&passphrase);

    // Khoá phụ CỨU dữ liệu: mặc định (máy đang chạy "0"×32) + KEY_OLD (xoay khoá).
    let default_engine = EncryptionEngine::new(crypto::DEFAULT_ENCRYPTION_KEY);
    let old_engine = std::env::var("LIVA_ENCRYPTION_KEY_OLD")
        .ok()
        .filter(|k| !k.is_empty() && *k != passphrase)
        .map(|k| EncryptionEngine::new(&k));
    let mut extra: Vec<&EncryptionEngine> = vec![&default_engine];
    if let Some(ref o) = old_engine {
        extra.push(o);
    }

    let conn = db
        .writer
        .get()
        .map_err(|e| format!("không lấy được connection để rekey: {e}"))?;
    let (rekeyed, locked) = db::rekey_facts_encryption(&conn, &live, &extra)
        .map_err(|e| format!("rekey facts thất bại: {e}"))?;
    let personal = db::rekey_personal_data_encryption(&conn, &live, &extra)
        .map_err(|e| format!("rekey checkpoint/conversation thất bại: {e}"))?;
    if !in_memory && (personal.rekeyed > 0 || personal.fts_removed > 0) {
        db::purge_personal_data_plaintext_remnants(&conn)
            .map_err(|e| format!("không dọn được plaintext cũ khỏi SQLite/WAL: {e}"))?;
    }

    Ok(BootKey {
        engine: live,
        escrow_hex,
        rekeyed,
        locked,
        personal_data_rekeyed: personal.rekeyed,
        personal_data_locked: personal.locked,
        source,
    })
}

/// Dòng escrow hiện khoá thiết bị MỘT LẦN để người dùng sao lưu. Trả về khối
/// văn bản; caller in ra stderr (standalone) hoặc dialog (Tauri). Tách thuần để
/// test được.
pub fn escrow_message(hex_key: &str) -> String {
    format!(
        "\n╔══════════════════════════════════════════════════════════════════╗\n\
         ║  LIVA vừa SINH khoá mã hoá thiết bị mới cho dữ liệu của bạn.        ║\n\
         ║  HÃY SAO LƯU khoá này ở nơi an toàn (trình quản lý mật khẩu…).      ║\n\
         ║  Nếu Windows bị cài lại / reset mật khẩu, đây là cách DUY NHẤT để   ║\n\
         ║  đọc lại ký ức: đặt biến môi trường LIVA_ENCRYPTION_KEY = khoá này. ║\n\
         ╚══════════════════════════════════════════════════════════════════╝\n\
         LIVA_ENCRYPTION_KEY={hex_key}\n"
    )
}

/// Hướng khắc phục thêm cho lỗi khởi tạo DB, hoặc rỗng nếu không nhận ra.
///
/// Lỗi DB thường quy về một nguyên nhân mà thông điệp gốc giấu kín: thiếu `vec0`
/// (sqlite-vec, do gói npm cung cấp). Tách thuần (nhận `&str`) để cả gateway
/// standalone (`main.rs::die_db`) lẫn vỏ Tauri dùng chung — tránh trôi dạt (M4).
pub fn db_error_hint(err: &str) -> &'static str {
    let normalized = err.to_ascii_lowercase();
    if normalized.contains("vec0") || normalized.contains("no such module") {
        "\n\nNguyên nhân thường gặp: chưa chạy `npm ci` ở thư mục gốc repo — \
         vec0.dll do gói npm sqlite-vec cung cấp. Với bản đã cài, chạy Repair \
         hoặc cài lại đúng bộ cài LIVA; không tải DLL rời từ nguồn lạ."
    } else if normalized.contains("database disk image is malformed")
        || normalized.contains("file is not a database")
    {
        "\n\nCơ sở dữ liệu có dấu hiệu hỏng. Không xóa hoặc ghi đè file gốc. \
         Sao lưu nguyên file hiện tại, rồi khôi phục một backup đã qua \
         `quick_check` theo `docs/02-van-hanh/06-backup-restore-sqlite.md`."
    } else if normalized.contains("unable to open database file")
        || normalized.contains("readonly")
        || normalized.contains("read-only")
        || normalized.contains("permission denied")
        || normalized.contains("access denied")
    {
        "\n\nLIVA không có quyền ghi vào thư mục dữ liệu. Kiểm tra quyền ghi của \
         `%LOCALAPPDATA%\\com.liva.cognitive-os`, hoặc đặt `LIVA_HOME` tới một \
         thư mục riêng mà tài khoản hiện tại sở hữu."
    } else if normalized.contains("database or disk is full") || normalized.contains("disk full") {
        "\n\nỔ chứa dữ liệu LIVA đã đầy. Giải phóng dung lượng trên ổ của \
         `LIVA_HOME` rồi khởi động lại; không xóa thủ công file `-wal` khi app \
         còn chạy."
    } else {
        ""
    }
}

/// Origin được phép nối vào WebSocket gateway.
pub const DEFAULT_WS_ALLOWED_ORIGINS: [&str; 4] = [
    "http://localhost:5173",
    "http://127.0.0.1:5173",
    "tauri://localhost",
    "https://tauri.localhost",
];

/// Kiểm tra header `Origin` của một handshake WebSocket có được phép không.
///
/// **Vì sao tự kiểm:** WebSocket KHÔNG chịu Same-Origin Policy và không có CORS
/// preflight. Bind `127.0.0.1` chỉ chặn được mạng LAN, không chặn được trình
/// duyệt của chính người dùng: bất kỳ trang web nào họ mở đều có thể chạy
/// `new WebSocket("ws://127.0.0.1:8002/ws")` rồi gọi `llm:swap_model`, đọc/ghi
/// cấu hình, nghe kết quả STT. Allow-list này là hàng rào duy nhất.
///
/// **Đánh đổi có chủ ý:** không có header `Origin` (`None`) thì CHO QUA, vì
/// client gốc — vỏ Tauri, `verify_duplex`, script kiểm thử — không gửi
/// `Origin`. Nghĩa là một chương trình native trên cùng máy vẫn nối được. Chấp
/// nhận được: chương trình native đã chạy được trên máy thì có nhiều đường tấn
/// công dễ hơn nhiều. Hàng rào này nhắm vào **trang web**, nơi kẻ tấn công
/// không đặt được `Origin`.
///
/// Mở rộng bằng `LIVA_WS_ALLOWED_ORIGINS` (ngăn cách bằng dấu phẩy).
pub fn origin_allowed(origin: Option<&str>) -> bool {
    let Some(raw) = origin else {
        return true;
    };
    let origin = raw.trim();
    if origin.is_empty() {
        // `Origin:` rỗng là do trình duyệt gửi khi bị sandbox — coi như web.
        return false;
    }
    if DEFAULT_WS_ALLOWED_ORIGINS.contains(&origin) {
        return true;
    }
    std::env::var("LIVA_WS_ALLOWED_ORIGINS")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .any(|allowed| !allowed.is_empty() && allowed == origin)
}

/// Load the configured router model into the LLM engine. `force=false` only
/// fills an empty engine (startup autoload); `force=true` also swaps when the
/// configured file differs from the loaded one (after update_config).
pub async fn load_configured_router_model(state: Arc<AppState>, force: bool) {
    let Some(model_path) = configured_router_model_path() else {
        tracing::info!("LLM provider is not 'local'; skipping router model load");
        return;
    };
    let models_dir = configured_models_dir();
    let model_path = match verify_model_artifact(&models_dir, &model_path) {
        Ok(path) => path,
        Err(error) => {
            tracing::error!("Từ chối nạp router model {:?}: {}", model_path, error);
            return;
        }
    };
    let mut llm_manager = state.llm.lock().await;
    // Keep the vision projector path current so `vision:ask` can lazily build
    // the multimodal context for a VL model.
    let mmproj_path =
        configured_mmproj_path().and_then(|path| match verify_model_artifact(&models_dir, &path) {
            Ok(path) => Some(path),
            Err(error) => {
                tracing::error!("Từ chối nạp mmproj {:?}: {}", path, error);
                None
            }
        });
    llm_manager.set_mmproj_path(mmproj_path);
    if llm_manager.engine.is_some() && (!force || llm_manager.current_model_path == model_path) {
        return;
    }
    tracing::info!("Loading router model {:?}...", model_path);
    match llm_manager.swap_model(&model_path, None, None, None).await {
        Ok(()) => tracing::info!("Router model loaded: {:?}", model_path),
        Err(e) => tracing::error!("Failed to load router model {:?}: {}", model_path, e),
    }
}

/// Reload the currently-loaded router LLM with a different GPU-layer count.
///
/// Used by the game-aware GPU governor: when a foreground game is detected we
/// reload the model with fewer (or zero) GPU layers to free VRAM for the game,
/// then restore full offload once the game exits. This is a real model reload
/// (~seconds, resets the KV cache), so the caller must only invoke it on an
/// actual game-mode transition — never on every poll.
///
/// Returns `true` once a model is loaded and now sits at `n_gpu_layers` (either
/// just reloaded there or already matching); returns `false` when no model is
/// loaded yet, so the caller can retry on a later poll instead of latching the
/// game state prematurely (e.g. a game already running at startup while the
/// autoload is still in flight).
pub async fn reload_llm_gpu_layers(state: Arc<AppState>, n_gpu_layers: u32) -> bool {
    let mut llm = state.llm.lock().await;
    if llm.engine.is_none() {
        return false; // model not loaded yet — caller should retry
    }
    let path = llm.current_model_path.clone();
    if llm.n_gpu_layers == n_gpu_layers || path.as_os_str().is_empty() {
        return true; // already at target (or no path to reload from)
    }
    let n_ctx = llm.n_ctx;
    let vocab_only = llm.vocab_only;
    let from = llm.n_gpu_layers;
    tracing::info!(
        "Game-aware GPU: reloading {:?} (n_gpu_layers {} -> {})",
        path,
        from,
        n_gpu_layers
    );
    match llm
        .swap_model(&path, Some(n_ctx), Some(n_gpu_layers), Some(vocab_only))
        .await
    {
        Ok(()) => tracing::info!("Game-aware GPU: reloaded (n_gpu_layers={})", n_gpu_layers),
        Err(e) => tracing::error!("Game-aware GPU reload failed: {}", e),
    }
    true
}

pub async fn handle_chat_completion_scoped(
    state: Arc<AppState>,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
    memory_scope: agent::graph::ConversationMemoryScope,
) -> Result<serde_json::Value, String> {
    let messages_val = payload["messages"]
        .as_array()
        .ok_or_else(|| "Missing or invalid 'messages' array".to_string())?;

    let mut messages = Vec::with_capacity(messages_val.len() + 1);
    for value in messages_val {
        let message: llm::ChatMessage = serde_json::from_value(value.clone())
            .map_err(|e| format!("Invalid message object: {e}"))?;
        messages.push(message);
    }

    if !messages.iter().any(|message| message.role == "system") {
        messages.insert(
            0,
            llm::ChatMessage {
                role: "system".to_string(),
                content: llm::persona::PERSONA_LIVA.to_string(),
            },
        );
    }

    let last_user_text = messages
        .iter()
        .rev()
        .find(|message| message.role == "user")
        .map(|message| message.content.clone())
        .unwrap_or_default();
    if let Some(memories) =
        agent::graph::recall_context_scoped(&state, &last_user_text, &memory_scope).await
    {
        messages.insert(
            1,
            llm::ChatMessage {
                role: "system".to_string(),
                content: agent::graph::memory_system_message(&memories),
            },
        );
    }

    let temperature = payload["temperature"]
        .as_f64()
        .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
    let top_p = payload["top_p"]
        .as_f64()
        .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
    let stream = payload["stream"].as_bool().unwrap_or(false);
    let compiled_prompt = llm::compile_prompt(&messages)?;

    let state_clone = state.clone();
    let completion_output = tokio::task::spawn_blocking(move || {
        let mut llm_manager = state_clone.llm.blocking_lock();
        if stream {
            let tx_inner =
                tx.ok_or_else(|| "IPC output channel missing for streaming".to_string())?;
            let req_id_inner =
                req_id.ok_or_else(|| "Request ID missing for streaming".to_string())?;
            llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |piece| {
                if piece.is_empty() {
                    return true;
                }
                let chunk_response = IpcTokenChunkRef {
                    id: &req_id_inner,
                    status: "ok",
                    data: IpcTokenChunkData {
                        token: piece,
                        done: false,
                    },
                };
                if let Ok(chunk) = serde_json::to_string(&chunk_response) {
                    let _ = tx_inner.blocking_send(chunk);
                }
                true
            })
        } else {
            llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |_| true)
        }
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {e}"))??;

    agent::graph::persist_turn_scoped(
        &state,
        &last_user_text,
        &completion_output.text,
        &memory_scope,
    )
    .await;

    Ok(serde_json::json!({
        "text": completion_output.text,
        "done": true,
        "usage": {
            "prompt_tokens": completion_output.prompt_tokens,
            "completion_tokens": completion_output.completion_tokens,
            "total_tokens": completion_output.prompt_tokens + completion_output.completion_tokens
        }
    }))
}

fn parse_untrusted_memory_search_filter(
    payload: &serde_json::Value,
) -> Result<db::MetadataFilter, String> {
    let filter_value = payload
        .get("filter")
        .filter(|value| !value.is_null())
        .ok_or_else(|| {
            "`memory:search_hybrid` requires explicit non-conversation `filter.type`; \
             conversation_turn requires authenticated owner scope"
                .to_string()
        })?;
    let filter: db::MetadataFilter = serde_json::from_value(filter_value.clone())
        .map_err(|error| format!("Invalid filter: {error}"))?;

    match filter.r#type.as_deref().map(str::trim) {
        Some(memory_type)
            if !memory_type.is_empty()
                && !memory_type.eq_ignore_ascii_case("conversation_turn") =>
        {
            Ok(filter)
        }
        _ => Err(
            "`memory:search_hybrid` cannot query conversation_turn without authenticated owner \
             scope; provide an explicit non-conversation `filter.type`"
                .to_string(),
        ),
    }
}

pub async fn handle_command_as(
    principal: CommandPrincipal,
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<serde_json::Value, String> {
    authorize_command(principal, command)?;
    handle_command(state, command, payload, tx, req_id).await
}

pub async fn handle_command(
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<serde_json::Value, String> {
    // Định tuyến theo MIỀN trước khi vào `match` phẳng. Miền nào đã tách thì
    // thêm lệnh mới cho nó chỉ đụng đúng file của miền đó — xem `commands/mod.rs`.
    if let Some(verb) = command.strip_prefix("vision:") {
        return commands::vision::handle(state, verb, payload).await;
    }
    if let Some(verb) = command.strip_prefix("voice:") {
        return commands::voice::handle(state, verb, payload).await;
    }
    // Cổng đồng ý U20. Nằm ở đây, trong build MẶC ĐỊNH — không phải sau
    // `experimental` như `passive/`: một cổng chỉ tồn tại ở build thử nghiệm thì
    // không chặn được gì trong bản giao cho người dùng.
    if let Some(verb) = command.strip_prefix("consent:") {
        return commands::consent::handle(state, verb, payload).await;
    }
    // Miền cấu hình/trạng thái dùng tên PHẲNG (`ping`, `get_config`, …) do UI
    // đặt từ thời kiến trúc Node.js, nên hỏi module thay vì cắt tiền tố — đổi
    // tên chúng sẽ phá hợp đồng với client đang chạy.
    if commands::config::owns(command) {
        return commands::config::handle(state, command, payload).await;
    }
    if commands::task::owns(command) {
        return commands::task::handle(state, command, payload).await;
    }
    // Hai miền nhận `tx`/`req_id` vì cả hai đều stream: `llm` đẩy từng mẩu chữ
    // trong lúc sinh, `setup` đẩy tiến độ tải model (3,7 GB — không có tiến độ
    // thì người dùng không phân biệt được "đang tải" với "treo").
    if commands::llm::owns(command) {
        return commands::llm::handle(state, command, payload, tx, req_id).await;
    }
    if commands::setup::owns(command) {
        return commands::setup::handle(state, command, payload, tx, req_id).await;
    }
    if commands::memory::owns(command) {
        return commands::memory::handle(state, command, payload).await;
    }
    if commands::integrations::owns(command) {
        return commands::integrations::handle(state, command, payload).await;
    }
    if commands::messaging::owns(command) {
        return commands::messaging::handle(state, command, payload).await;
    }
    if commands::skill_store::owns(command) {
        return commands::skill_store::handle(state, command, payload).await;
    }

    match command {
        // ── MCP ────────────────────────────────────────────────────────────
        // `NativeMcpServer` được dựng trong AppState từ lâu nhưng không có
        // nhánh nào gọi tới, nên toàn bộ 4 tool là code mồ côi. Hai arm dưới
        // đây nối nó vào lớp lệnh.
        //
        // Ranh giới an toàn: mọi thao tác file đi qua `resolve_path`, chặn
        // đường dẫn tuyệt đối và `..`, và ghim mọi thứ dưới `LIVA_VAULT_PATH`.
        "mcp:list_tools" => Ok(serde_json::to_value(state.mcp_server.list_tools())
            .map_err(|e| format!("Failed to serialize tool list: {}", e))?),

        "mcp:call_tool" => {
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("Missing 'name' (ten tool). Dung mcp:list_tools de xem danh sach.")?
                .to_string();
            // Không có `arguments` thì coi như object rỗng — tool nào cần tham
            // số sẽ tự báo lỗi deserialize với thông tin cụ thể hơn.
            let arguments = payload
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Hàng rào allowlist — xem `llm::tool_calling::guard_direct_call`.
            // Tới 26/07/2026 nhánh này gọi thẳng `state.mcp_server` không kiểm gì,
            // nên `write_markdown` mở cho bất kỳ client nào nối được vào lớp lệnh
            // (WS 8002 chưa có xác thực). Phát hiện ở tài liệu 02 §C1.1.
            llm::tool_calling::guard_direct_call(llm::tool_calling::NATIVE_SERVER, &name)?;

            let result = state
                .mcp_server
                .call_tool(mcp::protocol::CallToolRequest { name, arguments })
                .await?;
            serde_json::to_value(result)
                .map_err(|e| format!("Failed to serialize tool result: {}", e))
        }

        // ── MCP client — chiều gọi RA ngoài (G0) ───────────────────────────
        // Ba arm trên là LIVA làm server; ba arm dưới là LIVA làm client, nối
        // `mcp/client.rs` (trước đây là code mồ côi) vào lớp lệnh. Từ đây các
        // server trong `mcp_config.json` là thật, không còn là trang trí.
        //
        // Registry là singleton phạm vi tiến trình chứ không nằm trong
        // `AppState`: mỗi client giữ một tiến trình con thật, mà `AppState`
        // được dựng ở 9 chỗ. Xem `mcp::client::global_registry`.
        "mcp_client:list_servers" => Ok(mcp::client::global_registry().list_servers().await),

        "mcp_client:list_tools" => {
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'server'. Dùng mcp_client:list_servers để xem danh sách.")?;
            let tools = mcp::client::global_registry().list_tools(server).await?;
            serde_json::to_value(tools).map_err(|e| format!("Failed to serialize tool list: {}", e))
        }

        "mcp_client:call_tool" => {
            let server = payload
                .get("server")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'server'. Dùng mcp_client:list_servers để xem danh sách.")?;
            let name = payload
                .get("name")
                .and_then(|v| v.as_str())
                .ok_or("Thiếu 'name' (tên tool). Dùng mcp_client:list_tools để xem danh sách.")?
                .to_string();
            let arguments = payload
                .get("arguments")
                .cloned()
                .unwrap_or_else(|| serde_json::json!({}));

            // Hàng rào allowlist. Nhánh này nghiêm trọng hơn `mcp:call_tool`: nó
            // tới được MỌI tool trên MỌI server MCP ngoài trong `mcp_config.json`
            // — tiến trình của người lạ, với đúng quyền chúng có. Mặc định
            // `ExecPolicy` cho tool ngoài là ProposeOnly, nên mặc định là TỪ CHỐI.
            llm::tool_calling::guard_direct_call(server, &name)?;

            let result = mcp::client::global_registry()
                .call_tool(server, mcp::protocol::CallToolRequest { name, arguments })
                .await?;
            serde_json::to_value(result)
                .map_err(|e| format!("Failed to serialize tool result: {}", e))
        }

        _ => Err(format!("Unknown command: {}", command)),
    }
}

#[cfg(test)]
mod lib_tests;
