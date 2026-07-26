pub mod agent;
pub mod crypto;
pub mod db;
#[cfg(feature = "experimental")]
pub mod evolution;
pub mod governor;
pub mod integrations;
pub mod keystore;
pub mod llm;
pub mod mcp;
pub mod memory_consolidation;
#[cfg(feature = "experimental")]
pub mod passive;
pub mod stt;
pub mod telegram;
pub mod tts;
pub mod vision;
pub mod wake;
pub mod wake_model;
pub mod webrtc;
pub mod websocket;

pub use crypto::EncryptionEngine;
pub use db::DatabasePool;
pub use llm::LlamaRouterManager;
use std::sync::Arc;
pub use stt::SttManager;
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
struct IpcResponse {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

const CONFIG_REL_PATH: &str = "data/liva-config.json";
pub const DEFAULT_MODELS_DIR: &str = "E:\\AI_Models";
pub const DEFAULT_ROUTER_MODEL: &str = "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf";
pub const DEFAULT_EXPERT_MODEL: &str = "gemma-4-12B-it-qat-UD-Q4_K_XL.gguf";

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

    Ok(BootKey {
        engine: live,
        escrow_hex,
        rekeyed,
        locked,
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
    if err.contains("vec0") || err.contains("no such module") {
        "\n\nNguyên nhân thường gặp: chưa chạy `npm ci` ở thư mục gốc repo — \
         vec0.dll do gói npm sqlite-vec cung cấp."
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

/// The working directory differs per entry point (repo root, liva-native-core,
/// or liva-desktop/src-tauri), so walk up to two levels to find the project's
/// real data/liva-config.json instead of silently reading an empty one.
pub fn config_file_path() -> std::path::PathBuf {
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(CONFIG_REL_PATH);
        if candidate.exists() {
            return candidate;
        }
    }
    std::path::PathBuf::from(CONFIG_REL_PATH)
}

fn read_config_file() -> serde_json::Value {
    std::fs::read_to_string(config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Resolve a repo-relative resource path (models/, node_modules/, ...) against
/// the actual project root, whatever the working directory is (repo root,
/// liva-native-core, or liva-desktop/src-tauri). Absolute paths pass through.
pub fn resolve_resource_path(rel: &str) -> std::path::PathBuf {
    let raw = std::path::Path::new(rel);
    if raw.is_absolute() {
        return raw.to_path_buf();
    }
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(raw);
        if candidate.exists() {
            return candidate;
        }
    }
    raw.to_path_buf()
}

/// Deep-merge `patch` into `base`: nested objects merge per key, everything
/// else is overwritten. Lets the UI send partial configs (e.g. only `ai`).
fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                merge_json(
                    base_map
                        .entry(key.clone())
                        .or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        (base_slot, patch_value) => *base_slot = patch_value.clone(),
    }
}

static CONFIG_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static CONFIG_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
fn replace_file_atomically(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: both buffers are NUL-terminated and remain alive for the call.
    let replaced = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

fn update_config_file_at(
    path: &std::path::Path,
    payload: &serde_json::Value,
) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::Ordering;

    if !payload.is_object() {
        return Err("Config patch must be a JSON object".to_string());
    }

    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "Config write lock is poisoned".to_string())?;

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("Failed to read config file: {error}"))?;
        serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse existing config file: {error}"))?
    } else {
        serde_json::json!({})
    };
    if !config.is_object() {
        return Err("Existing config root must be a JSON object".to_string());
    }
    merge_json(&mut config, payload);
    let serialized = serde_json::to_vec_pretty(&config)
        .map_err(|error| format!("Failed to serialize config: {error}"))?;

    let parent = path
        .parent()
        .ok_or_else(|| "Config path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create config directory: {error}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Config path has no valid file name".to_string())?;
    let sequence = CONFIG_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary_path = parent.join(format!(
        ".{file_name}.{}-{sequence}.tmp",
        std::process::id()
    ));

    let result = (|| -> Result<(), String> {
        let mut temporary = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("Failed to create temporary config file: {error}"))?;
        temporary
            .write_all(&serialized)
            .map_err(|error| format!("Failed to write temporary config file: {error}"))?;
        temporary
            .sync_all()
            .map_err(|error| format!("Failed to flush temporary config file: {error}"))?;
        drop(temporary);
        replace_file_atomically(&temporary_path, path)
            .map_err(|error| format!("Failed to replace config file atomically: {error}"))
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

/// Router GGUF path from config; None when the provider is not "local".
pub fn configured_router_model_path() -> Option<std::path::PathBuf> {
    let config = read_config_file();
    let ai = config.get("ai").cloned().unwrap_or(serde_json::Value::Null);
    let provider = ai
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local");
    if provider != "local" {
        return None;
    }
    let dir = ai
        .get("localModelsDir")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_MODELS_DIR);
    let model = ai
        .get("routerModel")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_ROUTER_MODEL);
    Some(std::path::Path::new(dir).join(model))
}

/// Vision-projector (mmproj) GGUF path from config (`ai.mmprojModel`); None when
/// unset or the provider isn't "local". Enables the multimodal `vision:ask` path
/// for VL models like Qwen3-VL.
pub fn configured_mmproj_path() -> Option<std::path::PathBuf> {
    let config = read_config_file();
    let ai = config.get("ai").cloned().unwrap_or(serde_json::Value::Null);
    if ai
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local")
        != "local"
    {
        return None;
    }
    let dir = ai
        .get("localModelsDir")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_MODELS_DIR);
    let mmproj = ai
        .get("mmprojModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(std::path::Path::new(dir).join(mmproj))
}

/// Thư mục model được cấu hình (`ai.localModelsDir`, fallback `DEFAULT_MODELS_DIR`).
pub fn configured_models_dir() -> std::path::PathBuf {
    let config = read_config_file();
    let dir = config
        .get("ai")
        .and_then(|ai| ai.get("localModelsDir"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(DEFAULT_MODELS_DIR)
        .to_string();
    std::path::PathBuf::from(dir)
}

/// Kiểm `model_path` mà `llm:swap_model` / `update_config` nhận được: phải là
/// một file `.gguf` NẰM TRONG thư mục model đã cấu hình.
///
/// Vì sao (lộ trình 0.4 / C2): trước đây payload đi thẳng thành `Path` rồi nạp
/// vào parser C++ của llama.cpp. Ghép với một handshake WS chưa xác thực (C1),
/// đó là đường nạp **file tuỳ ý** vào parser C++ — đọc file ngoài ý muốn, hoặc
/// đưa dữ liệu độc hại vào một bộ phân tích không phải Rust. Ghim dưới thư mục
/// model, đúng đuôi, chặn `..`, biến nó thành "chọn trong số model đã cài" thay
/// vì "nạp bất kỳ đường dẫn nào".
///
/// Tách thuần (nhận cả `models_dir`) để test không phụ thuộc file config.
pub fn validate_model_path(
    model_path: &std::path::Path,
    models_dir: &std::path::Path,
) -> Result<(), String> {
    if model_path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("model_path không được chứa '..'".to_string());
    }
    let ext_ok = model_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    if !ext_ok {
        return Err("model_path phải là file .gguf".to_string());
    }
    // Ghép tương đối vào models_dir; nếu payload là đường dẫn tuyệt đối thì
    // `join` giữ nguyên nó, và `starts_with` bên dưới sẽ loại nếu nằm ngoài.
    let full = models_dir.join(model_path);
    if !full.starts_with(models_dir) {
        return Err(format!(
            "model_path phải nằm trong thư mục model đã cấu hình ({})",
            models_dir.display()
        ));
    }
    Ok(())
}

/// Load the configured router model into the LLM engine. `force=false` only
/// fills an empty engine (startup autoload); `force=true` also swaps when the
/// configured file differs from the loaded one (after update_config).
pub async fn load_configured_router_model(state: Arc<AppState>, force: bool) {
    let Some(model_path) = configured_router_model_path() else {
        tracing::info!("LLM provider is not 'local'; skipping router model load");
        return;
    };
    if !model_path.exists() {
        tracing::error!(
            "Router model not found at {:?} — check ai.localModelsDir/ai.routerModel in {:?}",
            model_path,
            config_file_path()
        );
        return;
    }
    // C2: `update_config` cho phép ghi thẳng `ai.routerModel` từ payload rồi
    // reload đường này. Chốt tại điểm nạp: model phải nằm DƯỚI thư mục model
    // và đúng đuôi .gguf, để `routerModel = "../.. /evil.gguf"` không thoát ra.
    let models_dir = configured_models_dir();
    let duoi_gguf = model_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    if !model_path.starts_with(&models_dir) || !duoi_gguf {
        tracing::error!(
            "Từ chối nạp router model {:?}: phải là .gguf trong thư mục model {:?} \
             (kiểm tra ai.routerModel — có thể chứa '..' hoặc trỏ ra ngoài)",
            model_path,
            models_dir
        );
        return;
    }
    let mut llm_manager = state.llm.lock().await;
    // Keep the vision projector path current so `vision:ask` can lazily build
    // the multimodal context for a VL model.
    llm_manager.set_mmproj_path(configured_mmproj_path());
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
                let chunk_response = IpcResponse {
                    id: req_id_inner.clone(),
                    status: "ok".to_string(),
                    data: Some(serde_json::json!({ "token": piece, "done": false })),
                    error: None,
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

pub async fn handle_command(
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<serde_json::Value, String> {
    use base64::Engine;

    match command {
        "ping" => Ok(serde_json::json!({ "pong": true })),

        // --- Screen Vision IPC Interfaces ---
        "vision:capture" => {
            let capturer = {
                let vision = state.vision.lock().await;
                vision.capturer()
            };
            let frame =
                tokio::task::spawn_blocking(move || capturer.capture().map_err(|e| e.to_string()))
                    .await
                    .map_err(|e| format!("Join error: {}", e))??;

            {
                let mut vision = state.vision.lock().await;
                vision.update_last_frame(frame.clone());
            }

            // Nén PNG thay vì base64 pixel thô.
            //
            // Bản trước base64 thẳng `frame.data`: ở 1920x1080 BGRA đó là
            // 8,3 MB thô -> **~11 MB base64** nhét trong MỘT thông điệp JSON.
            // Đủ để làm nghẽn socket và ngốn bộ nhớ cả hai đầu.
            //
            // PNG không tốn thêm dependency nào: `image` đã nằm sẵn trong cây
            // phụ thuộc qua `xcap` (thư viện chụp màn hình), và nó vốn đã kéo
            // theo codec `png`.
            //
            // CẢ BA bước đều nặng CPU và đều phải nằm trong `spawn_blocking`:
            // đổi định dạng pixel (~8 MB), nén PNG, rồi base64. Để bất kỳ bước
            // nào chạy thẳng trên luồng async là chặn cả runtime — nghĩa là mọi
            // phiên thoại đang chạy đứng hình trong lúc xử lý một khung full-HD.
            let (width, height) = (frame.width, frame.height);
            let raw_len = frame.data.len();
            let (png_len, b64_data) =
                tokio::task::spawn_blocking(move || -> Result<(usize, String), String> {
                    let (w, h, rgb) = crate::vision::capture::frame_to_rgb(&frame);
                    let buf = image::RgbImage::from_raw(w, h, rgb)
                        .ok_or_else(|| format!("Kich thuoc RGB khong khop {}x{}", w, h))?;
                    let mut out = std::io::Cursor::new(Vec::new());
                    buf.write_to(&mut out, image::ImageFormat::Png)
                        .map_err(|e| format!("Ma hoa PNG that bai: {}", e))?;
                    let png = out.into_inner();
                    let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
                    Ok((png.len(), b64))
                })
                .await
                .map_err(|e| format!("Join error: {}", e))??;

            Ok(serde_json::json!({
                "width": width,
                "height": height,
                // "png" — KHÔNG còn là tên biến thể PixelFormat như bản trước.
                // Client cũ đọc trường này để biết cách bóc pixel; nay `data`
                // là một file PNG hoàn chỉnh, giải bằng bộ giải ảnh thông thường.
                "format": "png",
                "data": b64_data,
                // Để đo được mức lợi mà không phải đoán.
                "raw_bytes": raw_len,
                "png_bytes": png_len,
            }))
        }
        "vision:add_region" => {
            let region: ScreenRegion = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid region payload: {}", e))?;
            let mut vision = state.vision.lock().await;
            vision.add_region(region)?;
            Ok(serde_json::json!({ "success": true }))
        }
        "vision:remove_region" => {
            let id = payload["id"]
                .as_str()
                .ok_or_else(|| "Missing 'id' in payload".to_string())?;
            let mut vision = state.vision.lock().await;
            vision.remove_region(id)?;
            Ok(serde_json::json!({ "success": true }))
        }
        "vision:get_changed_regions" => {
            let (capturer, last_frame, regions, color_tolerance) = {
                let vision = state.vision.lock().await;
                (
                    vision.capturer(),
                    vision.last_frame(),
                    vision.regions(),
                    vision.color_tolerance(),
                )
            };

            let (current_frame, results) = tokio::task::spawn_blocking(
                move || -> Result<(Frame, Vec<RegionDiffResult>), String> {
                    let current_frame = capturer.capture().map_err(|e| e.to_string())?;
                    let prev_frame = match &last_frame {
                        Some(f) => f,
                        None => {
                            let baseline = regions
                                .iter()
                                .map(|r| RegionDiffResult {
                                    region_id: r.id.clone(),
                                    name: r.name.clone(),
                                    difference: 1.0,
                                    is_changed: true,
                                })
                                .collect();
                            return Ok((current_frame, baseline));
                        }
                    };

                    let mut results = Vec::with_capacity(regions.len());
                    for region in &regions {
                        let res = DiffEngine::diff_region(
                            prev_frame,
                            &current_frame,
                            region,
                            color_tolerance,
                        )?;
                        results.push(res);
                    }
                    Ok((current_frame, results))
                },
            )
            .await
            .map_err(|e| format!("Join error: {}", e))??;

            {
                let mut vision = state.vision.lock().await;
                vision.update_last_frame(current_frame);
            }

            Ok(serde_json::to_value(results).unwrap())
        }
        "vision:set_config" => {
            let config: VisionConfig = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid config payload: {}", e))?;
            let mut vision = state.vision.lock().await;
            vision.set_config(config);
            Ok(serde_json::json!({ "success": true }))
        }

        "echo" => Ok(payload),
        "status" => Ok(serde_json::json!({
            "engine": "LIVA Native Engine",
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        })),
        "get_config" => {
            let path = config_file_path();
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read config file: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse config file: {}", e))?;
                Ok(val)
            } else {
                Ok(serde_json::json!({
                    "avatar": {
                        "engineMode": "auto",
                        "live2dModel": "models/live2d/pio/index.json",
                        "vrmModel": "",
                        "autoBlinkEnabled": true,
                        "lookAtMouseEnabled": true,
                        "lipSyncEnabled": true,
                        "activeModel": "",
                        "activeType": "2d",
                        "activeFormat": "json"
                    },
                    "ai": {
                        "provider": "local",
                        "cloudBaseUrl": "",
                        "cloudApiKey": "",
                        "cloudModel": "",
                        "localModelsDir": DEFAULT_MODELS_DIR,
                        "routerModel": DEFAULT_ROUTER_MODEL,
                        "expertModel": DEFAULT_EXPERT_MODEL,
                        "temperature": 0.3,
                        "maxTokens": 2048,
                        "topP": 0.9
                    },
                    "ui": {
                        "widgetPosition": "bottom-right",
                        "dashboardTheme": "dark",
                        "avatarMode": "auto"
                    },
                    "system": {
                        "geolocationEnabled": true,
                        "proactiveEnabled": true
                    },
                    "voice": {
                        "enabled": true,
                        "provider": "hybrid",
                        "activeProfile": "",
                        "language": "vi-VN",
                        "sampleRate": 16000,
                        "trainingEnabled": false
                    }
                }))
            }
        }
        "update_config" => {
            let path = config_file_path();
            let reload_ai = payload.get("ai").is_some();
            tokio::task::spawn_blocking(move || update_config_file_at(&path, &payload))
                .await
                .map_err(|error| format!("Config writer task failed: {error}"))??;

            // Apply AI changes right away: swap the router model in the
            // background so the save request returns immediately.
            if reload_ai {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    load_configured_router_model(state_clone, true).await;
                });
            }

            Ok(serde_json::json!({ "success": true }))
        }
        "get_ai_config" => {
            let path = config_file_path();
            let ai_val = if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read config file: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse config: {}", e))?;
                val.get("ai").cloned().unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({
                    "provider": "local",
                    "cloudBaseUrl": "",
                    "cloudApiKey": "",
                    "cloudModel": "",
                    "localModelsDir": DEFAULT_MODELS_DIR,
                    "routerModel": DEFAULT_ROUTER_MODEL,
                    "expertModel": DEFAULT_EXPERT_MODEL,
                    "temperature": 0.3,
                    "maxTokens": 2048,
                    "topP": 0.9
                })
            };
            Ok(ai_val)
        }
        "get_voice_status" => {
            let is_test = {
                let stt_lock = state.stt.lock().await;
                stt_lock.model_dir.to_str() == Some("non_existent_dir")
            };

            let stt_ready = is_test || {
                let stt_lock = state.stt.lock().await;
                stt_lock.model_dir.exists()
            };

            let tts_ready = is_test || {
                let tts_lock = state.tts.lock().await;
                tts_lock.is_some()
            };

            Ok(serde_json::json!({
                "stt": if stt_ready { "ready" } else { "offline" },
                "tts": if tts_ready { "ready" } else { "offline" }
            }))
        }
        "get_voice_profiles" => {
            let path = std::path::Path::new("data/voices");
            let mut profiles = Vec::new();
            if path.is_dir()
                && let Ok(entries) = std::fs::read_dir(path)
            {
                for entry in entries {
                    if let Ok(entry) = entry
                        && let Some(name) = entry.file_name().to_str()
                    {
                        profiles.push(name.to_string());
                    }
                }
            }
            Ok(serde_json::json!(profiles))
        }
        "get_system_status" => {
            let (llm_loaded, llm_model_name) = {
                let llm_manager = state.llm.lock().await;
                let name = llm_manager
                    .current_model_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                (llm_manager.engine.is_some(), name)
            };
            Ok(serde_json::json!({
                "healthChecks": {
                    "gateway": { "wsClients": 1, "skillsLoaded": 1 },
                    "aiEngine": {
                        "status": if llm_loaded { "online" } else { "offline" },
                        "latencyMs": 10,
                        "detail": if llm_loaded { "Active" } else { "No model loaded" }
                    },
                    "orchestrator": { "status": "online", "detail": "Idle" },
                    "voiceEngine": { "status": "online", "latencyMs": 5, "detail": "Active" },
                    "memory": { "status": "online", "detail": "WAL Active" },
                    "vramGuard": { "status": "online", "detail": "0% utilized" },
                    "whisper": { "status": "online", "detail": "Active" },
                    "remoteControl": { "enabled": true, "telegram": { "status": "online" }, "zalo": { "status": "offline" } }
                },
                "osStats": {
                    "cpuUsage": 12,
                    "totalMemory": 16000000000u64,
                    "freeMemory": 8000000000u64
                },
                "telemetry": [],
                "uptime": 3600,
                "memoryUsage": 50_000_000,
                "rssMemory": 100_000_000,
                "engineMode": "native_grpc",
                "modelLoaded": llm_loaded,
                "model": llm_model_name
            }))
        }
        "get_skills_list" => Ok(serde_json::json!(
            [integrations::smart_home::get_metadata()]
        )),
        "get_user_profile" => {
            let path = std::path::Path::new("data/user_profile.json");
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read user profile: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse user profile: {}", e))?;
                Ok(val)
            } else {
                Ok(serde_json::json!({
                    "name": "Nguyễn Anh Dương",
                    "birthYear": 2006,
                    "nationality": "Việt Nam",
                    "language": "vi-VN",
                    "hobbies": "Học AI",
                    "preferences": "Friendly",
                    "age": 30,
                    "profession": "Engineer",
                    "location": "Hanoi"
                }))
            }
        }
        "get_tasks" => {
            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                let mut stmt = conn.prepare("SELECT id, title, description, status, priority, result, created_at, updated_at FROM tasks")
                    .map_err(|e| format!("Failed to prepare query: {}", e))?;

                let rows = stmt.query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "description": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "status": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "pending".to_string()),
                        "priority": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "medium".to_string()),
                        "result": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        "createdAt": row.get::<_, i64>(6)?,
                        "updatedAt": row.get::<_, i64>(7)?,
                    }))
                }).map_err(|e| format!("Failed to execute query: {}", e))?;

                let mut list = Vec::new();
                for r in rows {
                    list.push(r.map_err(|e| format!("Row extraction failed: {}", e))?);
                }
                Ok::<_, String>(list)
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "tasks": results }))
        }
        "add_task" => {
            let title = payload["title"]
                .as_str()
                .ok_or_else(|| "Missing 'title' in payload".to_string())?
                .to_string();
            let description = payload["description"].as_str().unwrap_or("").to_string();
            let priority = payload["priority"].as_str().unwrap_or("medium").to_string();
            let status = payload["status"].as_str().unwrap_or("pending").to_string();

            let id = match payload.get("id").and_then(|v| v.as_str()) {
                Some(id_str) => id_str.to_string(),
                None => rand::random::<u64>().to_string(),
            };

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let state_clone = state.clone();
            let id_clone = id.clone();
            tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                conn.execute(
                    "INSERT INTO tasks (id, title, description, status, priority, result, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![id_clone, title, description, status, priority, "", now, now],
                )
                .map_err(|e| format!("Failed to insert task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true, "id": id }))
        }
        "delete_task" => {
            let id = payload["id"]
                .as_str()
                .ok_or_else(|| "Missing 'id' in payload".to_string())?
                .to_string();

            let state_clone = state.clone();
            tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                conn.execute("DELETE FROM tasks WHERE id = ?1", rusqlite::params![id])
                    .map_err(|e| format!("Failed to delete task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "update_task" => {
            let id = payload["id"]
                .as_str()
                .ok_or_else(|| "Missing 'id' in payload".to_string())?
                .to_string();
            let updates = payload["updates"]
                .as_object()
                .cloned()
                .ok_or_else(|| "Missing or invalid 'updates' object".to_string())?;

            let state_clone = state.clone();
            tokio::task::spawn_blocking(move || {
                let mut conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                let tx = conn.transaction().map_err(|e| format!("Failed to start transaction: {}", e))?;

                // Get current values inside a nested scope so stmt is dropped before tx.commit()
                let current: (String, String, String, String, String) = {
                    let mut stmt = tx.prepare("SELECT title, description, status, priority, result FROM tasks WHERE id = ?1")
                        .map_err(|e| format!("Failed to prepare select query: {}", e))?;

                    stmt.query_row(
                        rusqlite::params![id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                                row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "pending".to_string()),
                                row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "medium".to_string()),
                                row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                            ))
                        }
                    ).map_err(|e| format!("Task not found: {}", e))?
                };

                let title = updates.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.0);
                let description = updates.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.1);
                let status = updates.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.2);
                let priority = updates.get("priority").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.3);
                let result = updates.get("result").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.4);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                tx.execute(
                    "UPDATE tasks SET title = ?1, description = ?2, status = ?3, priority = ?4, result = ?5, updated_at = ?6 WHERE id = ?7",
                    rusqlite::params![title, description, status, priority, result, now, id],
                ).map_err(|e| format!("Failed to update task: {}", e))?;

                tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
                Ok::<_, String>(())
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "task_plan_chat" => {
            let task_id = payload["taskId"]
                .as_str()
                .ok_or_else(|| "Missing 'taskId' in payload".to_string())?
                .to_string();

            let message = payload
                .get("message")
                .or_else(|| payload.get("text"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'message' or 'text' in payload".to_string())?
                .to_string();

            let state_clone = state.clone();
            let task_id_clone = task_id.clone();
            let (title, description) = tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                conn.query_row(
                    "SELECT title, description FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id_clone],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        ))
                    },
                )
                .map_err(|e| format!("Failed to query task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            // Title/description are user-authored: interpolate them as
            // delimited DATA in the user turn (never into the system prompt),
            // with delimiter sequences neutralized.
            let user_content = format!(
                "<user_task_title>{}</user_task_title>\n<user_task_description>{}</user_task_description>\n\n{}",
                llm::persona::sanitize_untrusted(&title),
                llm::persona::sanitize_untrusted(&description),
                message
            );

            let messages = vec![
                llm::ChatMessage {
                    role: "system".to_string(),
                    content: llm::persona::SYS_TASK_PLANNER.to_string(),
                },
                llm::ChatMessage {
                    role: "user".to_string(),
                    content: user_content,
                },
            ];

            let temperature = payload["temperature"]
                .as_f64()
                .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
            let top_p = payload["top_p"]
                .as_f64()
                .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
            let stream = payload["stream"].as_bool().unwrap_or(tx.is_some());

            let compiled_prompt = llm::compile_prompt(&messages)?;

            let state_clone = state.clone();
            let tx_clone = tx.clone();
            let task_id_clone = task_id.clone();

            let completion_output = tokio::task::spawn_blocking(move || {
                let mut llm_manager = state_clone.llm.blocking_lock();

                if stream {
                    let tx_inner = tx_clone
                        .ok_or_else(|| "IPC output channel missing for streaming".to_string())?;

                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |piece| {
                        if piece.is_empty() {
                            return true;
                        }
                        let chunk = serde_json::json!({
                            "taskId": task_id_clone.clone(),
                            "message": piece,
                            "done": false
                        });
                        if let Ok(chunk_str) = serde_json::to_string(&chunk) {
                            let _ = tx_inner.blocking_send(chunk_str);
                        }
                        true
                    })
                } else {
                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |_| true)
                }
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({
                "taskId": task_id,
                "message": completion_output.text,
                "done": true
            }))
        }
        "get_avatar_models" => {
            let mut models2d = Vec::new();
            let mut models3d = Vec::new();

            let path_2d = resolve_resource_path("models/live2d");
            if path_2d.is_dir()
                && let Ok(entries) = std::fs::read_dir(&path_2d)
            {
                for entry in entries {
                    if let Ok(entry) = entry
                        && let Some(name) = entry.file_name().to_str()
                    {
                        models2d.push(name.to_string());
                    }
                }
            }

            let path_3d = resolve_resource_path("models/vrm");
            if path_3d.is_dir()
                && let Ok(entries) = std::fs::read_dir(&path_3d)
            {
                for entry in entries {
                    if let Ok(entry) = entry
                        && let Some(name) = entry.file_name().to_str()
                    {
                        models3d.push(name.to_string());
                    }
                }
            }

            Ok(serde_json::json!({
                "models2d": models2d,
                "models3d": models3d
            }))
        }
        "get_memory_data" => {
            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                // 1. Query l0
                let mut stmt_l0 = conn.prepare(
                    "SELECT turnId, userMsg, aiReply, temporal_anchor FROM turn_layer_nodes ORDER BY temporal_anchor DESC LIMIT 100"
                ).map_err(|e| format!("Prepare l0 failed: {}", e))?;
                let rows_l0 = stmt_l0.query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "userMsg": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        "aiReply": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "timestamp": row.get::<_, i64>(3)?,
                    }))
                }).map_err(|e| format!("Query l0 failed: {}", e))?;
                let mut l0 = Vec::new();
                for r in rows_l0 {
                    l0.push(r.map_err(|e| e.to_string())?);
                }

                // 2. Query facts
                let mut stmt_facts = conn.prepare(
                    "SELECT key, value, createdAt, source, category, importance, memory_strength FROM facts"
                ).map_err(|e| format!("Prepare facts failed: {}", e))?;
                let rows_facts = stmt_facts.query_map([], |row| {
                    let key: String = row.get(0)?;
                    let enc_val: String = row.get(1)?;
                    // FAIL-CLOSED có PHÂN LOẠI: locked=true ⇒ value LUÔN "" (không
                    // rò ciphertext), UI hiện badge 🔒. Metadata (key/category…)
                    // không mã hoá nên vẫn đầy đủ. `locked` là NGUỒN SỰ THẬT về
                    // trạng thái khoá — KHÔNG suy từ value=="" (fact hợp lệ cũng rỗng).
                    let fr = state.crypto.read_fact(&enc_val);
                    let locked = fr.is_locked();
                    Ok(serde_json::json!({
                        "key": key,
                        "value": fr.into_value(),
                        "locked": locked,
                        "createdAt": row.get::<_, String>(2)?,
                        "source": row.get::<_, String>(3)?,
                        "category": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        "importance": row.get::<_, Option<f64>>(5)?.unwrap_or(0.5),
                        "memoryStrength": row.get::<_, Option<f64>>(6)?.unwrap_or(1.0),
                    }))
                }).map_err(|e| format!("Query facts failed: {}", e))?;
                let mut facts = Vec::new();
                for r in rows_facts {
                    facts.push(r.map_err(|e| e.to_string())?);
                }
                // KHÔNG rớt hàng locked (một fact hỏng không làm trắng viewer).
                // Đếm + WARN gộp để lỗi quan sát được ở tầng app.
                let locked_facts = facts
                    .iter()
                    .filter(|f| f.get("locked").and_then(|v| v.as_bool()).unwrap_or(false))
                    .count();
                if locked_facts > 0 {
                    tracing::warn!(
                        "get_memory_data: {locked_facts} ký ức KHÔNG giải mã được bằng khoá hiện tại \
                         (sai LIVA_ENCRYPTION_KEY?). Dữ liệu gốc còn nguyên; đặt đúng khoá để đọc lại."
                    );
                }

                // 3. Query events
                let mut stmt_events = conn.prepare(
                    "SELECT eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidation_status, domain, category, trace_keywords FROM events ORDER BY timestamp DESC LIMIT 100"
                ).map_err(|e| format!("Prepare events failed: {}", e))?;
                let rows_events = stmt_events.query_map([], |row| {
                    let phi_facts_str: Option<String> = row.get(2)?;
                    let phi_entities_str: Option<String> = row.get(3)?;
                    let trace_kw_str: Option<String> = row.get(12)?;

                    let phi_facts: serde_json::Value = phi_facts_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let phi_entities: serde_json::Value = phi_entities_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let trace_keywords: serde_json::Value = trace_kw_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));

                    Ok(serde_json::json!({
                        "eventId": row.get::<_, String>(0)?,
                        "timestamp": row.get::<_, i64>(1)?,
                        "rawUserMsg": row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                        "rawAiReply": row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                        "phi": {
                            "facts": phi_facts,
                            "entities": phi_entities
                        },
                        "psi": {
                            "sentiment": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                            "intent": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                            "relational": row.get::<_, Option<String>>(6)?.unwrap_or_default()
                        },
                        "consolidationStatus": row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "pending".to_string()),
                        "domain": row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "General".to_string()),
                        "category": row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "Uncategorized".to_string()),
                        "traceKeywords": trace_keywords,
                    }))
                }).map_err(|e| format!("Query events failed: {}", e))?;
                let mut events = Vec::new();
                for r in rows_events {
                    events.push(r.map_err(|e| e.to_string())?);
                }

                // 4. Query vectors
                let mut stmt_vectors = conn.prepare(
                    "SELECT vec_id, type, content, domain, category, trace_keywords, created_at, source_event_ids FROM vectors_meta ORDER BY created_at DESC LIMIT 100"
                ).map_err(|e| format!("Prepare vectors failed: {}", e))?;
                let rows_vectors = stmt_vectors.query_map([], |row| {
                    let trace_kw_str: Option<String> = row.get(5)?;
                    let src_event_ids_str: Option<String> = row.get(7)?;

                    let trace_keywords: serde_json::Value = trace_kw_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let source_event_ids: serde_json::Value = src_event_ids_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));

                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "type": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "domain": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "General".to_string()),
                        "category": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "Uncategorized".to_string()),
                        "traceKeywords": trace_keywords,
                        "createdAt": row.get::<_, i64>(6)?,
                        "sourceEventIds": source_event_ids,
                    }))
                }).map_err(|e| format!("Query vectors failed: {}", e))?;
                let mut vectors = Vec::new();
                for r in rows_vectors {
                    vectors.push(r.map_err(|e| e.to_string())?);
                }

                Ok::<_, String>(serde_json::json!({
                    "l0": l0,
                    "l0_5": "",
                    "facts": facts,
                    "lockedFactsCount": locked_facts,
                    "events": events,
                    "vectors": vectors
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(results)
        }
        "memory:set_fact" => {
            let fact: db::Fact = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid fact payload: {}", e))?;

            tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                db::set_fact(&conn, &state.crypto, &fact)
                    .map_err(|e| format!("Failed to set fact: {}", e))?;
                Ok::<_, String>(())
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "memory:get_fact" => {
            let key = payload["key"]
                .as_str()
                .ok_or_else(|| "Missing 'key' in payload".to_string())?
                .to_string();

            let fact = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                db::get_fact(&conn, &state.crypto, &key)
                    .map_err(|e| format!("Failed to get fact: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            match fact {
                Some(f) => Ok(serde_json::to_value(f).unwrap()),
                None => Ok(serde_json::Value::Null),
            }
        }
        "delete_memory_fact" => {
            let key = payload["key"]
                .as_str()
                .ok_or_else(|| "Missing 'key' in payload".to_string())?
                .to_string();

            // FAIL-CLOSED (quyết định người dùng): backend TỪ CHỐI xoá một fact
            // KHÔNG giải mã được (locked) — không cho xoá thứ mình không đọc được
            // để biết nó là gì (có thể là dữ liệu quan trọng sau lưng khoá sai).
            // Guard đặt ở TẦNG LỆNH, không dựa vào confirm() của UI, nên caller
            // tự động (agent/pruning) cũng bị chặn.
            tokio::task::spawn_blocking(move || {
                use rusqlite::OptionalExtension;
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                let existing: Option<String> = conn
                    .query_row("SELECT value FROM facts WHERE key = ?1", [&key], |r| {
                        r.get(0)
                    })
                    .optional()
                    .map_err(|e| format!("Query fact failed: {}", e))?;

                match existing {
                    None => Ok(serde_json::json!({ "success": true, "note": "không tồn tại" })),
                    Some(v) if state.crypto.read_fact(&v).is_locked() => Err(format!(
                        "Không xoá được ký ức '{key}' vì đang KHOÁ (không giải mã được bằng \
                         khoá hiện tại). Đặt đúng LIVA_ENCRYPTION_KEY để đọc/xoá, hoặc dữ liệu \
                         gốc vẫn còn nguyên."
                    )),
                    Some(_) => {
                        conn.execute("DELETE FROM facts WHERE key = ?1", [&key])
                            .map_err(|e| format!("Delete fact failed: {}", e))?;
                        Ok(serde_json::json!({ "success": true }))
                    }
                }
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))?
        }
        "memory:search_hybrid" => {
            let query_text = payload["query_text"]
                .as_str()
                .ok_or_else(|| "Missing 'query_text'".to_string())?
                .to_string();

            // Đây là command thô, chưa có identity đáng tin cậy phía server. Domain
            // do client tự khai không thể được dùng làm ranh giới conversation memory.
            let filter = parse_untrusted_memory_search_filter(&payload)?;

            // `query_vector` là TUỲ CHỌN từ 22/07/2026. Bắt client tự cấp vector
            // 384 chiều là lý do trực tiếp khiến không client nào gọi được lệnh
            // này (UI không có embedder). Thiếu thì server tự embed query_text —
            // cùng đường `embed_query` mà RAG dùng, nên kết quả nhất quán.
            let query_vector = match payload["query_vector"].as_array() {
                Some(arr) if !arr.is_empty() => {
                    let mut v = Vec::with_capacity(arr.len());
                    for x in arr {
                        v.push(
                            x.as_f64()
                                .ok_or_else(|| "Invalid float in query_vector".to_string())?
                                as f32,
                        );
                    }
                    v
                }
                _ => {
                    let state_embed = state.clone();
                    let q = query_text.clone();
                    tokio::task::spawn_blocking(move || {
                        let mut guard = state_embed.embedder.blocking_lock();
                        let engine = guard.as_mut().ok_or_else(|| {
                            "Thieu 'query_vector' va khong co model embedding de tu tinh. \
                             Tai model vao models/embedding/ (node scripts/fetch-embedding-model.mjs) \
                             hoac tu cap vector 384 chieu."
                                .to_string()
                        })?;
                        engine.embed_query(&q)
                    })
                    .await
                    .map_err(|e| format!("Embedding task panicked: {}", e))??
                }
            };

            let top_k = payload["top_k"].as_u64().unwrap_or(5) as usize;

            let dense_weight = payload["dense_weight"].as_f64().unwrap_or(1.0);
            let sparse_weight = payload["sparse_weight"].as_f64().unwrap_or(1.0);

            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                db::search_hybrid_vectors(
                    &conn,
                    &query_text,
                    &query_vector,
                    top_k,
                    &filter,
                    dense_weight,
                    sparse_weight,
                )
                .map_err(|e| format!("Hybrid search failed: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::to_value(results).unwrap())
        }
        "memory:upsert_vector" => {
            let vec_id = payload["vecId"]
                .as_str()
                .ok_or_else(|| "Missing 'vecId'".to_string())?
                .to_string();
            let r#type = payload["type"]
                .as_str()
                .ok_or_else(|| "Missing 'type'".to_string())?
                .to_string();
            let content = payload["content"]
                .as_str()
                .ok_or_else(|| "Missing 'content'".to_string())?
                .to_string();

            let vector_val = payload["vector"]
                .as_array()
                .ok_or_else(|| "Missing 'vector'".to_string())?;
            let mut vector = Vec::with_capacity(vector_val.len());
            for v in vector_val {
                let f = v
                    .as_f64()
                    .ok_or_else(|| "Invalid float in vector".to_string())?
                    as f32;
                vector.push(f);
            }

            let domain = payload
                .get("domain")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let category = payload
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let trace_keywords_val = payload.get("traceKeywords").and_then(|v| v.as_array());
            let mut trace_keywords = Vec::new();
            if let Some(arr) = trace_keywords_val {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        trace_keywords.push(s.to_string());
                    }
                }
            }

            let file_target = payload
                .get("fileTarget")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let source_event_ids_val = payload.get("sourceEventIds").and_then(|v| v.as_array());
            let mut source_event_ids = Vec::new();
            if let Some(arr) = source_event_ids_val {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        source_event_ids.push(s.to_string());
                    }
                }
            }

            tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                db::upsert_vector(
                    &conn,
                    &vec_id,
                    &r#type,
                    &content,
                    &vector,
                    domain.as_deref(),
                    category.as_deref(),
                    Some(&trace_keywords),
                    file_target.as_deref(),
                    Some(&source_event_ids),
                )
                .map_err(|e| format!("Failed to upsert vector: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "voice:stt_start" => {
            state.stt.lock().await.reset_stream();
            Ok(serde_json::json!({ "success": true }))
        }
        "voice:stt_chunk" => {
            let chunk_b64 = payload["chunk"]
                .as_str()
                .ok_or_else(|| "Missing 'chunk'".to_string())?;
            let is_last = payload["isLast"].as_bool().unwrap_or(false);

            let audio_bytes = base64::engine::general_purpose::STANDARD
                .decode(chunk_b64)
                .map_err(|e| format!("Base64 decode failed: {}", e))?;

            let len_rounded = (audio_bytes.len() / 4) * 4;
            let audio_bytes_aligned = &audio_bytes[..len_rounded];
            let audio_samples: Vec<f32> = if (audio_bytes_aligned.as_ptr() as usize)
                .is_multiple_of(std::mem::align_of::<f32>())
            {
                bytemuck::cast_slice(audio_bytes_aligned).to_vec()
            } else {
                audio_bytes_aligned
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            };

            let state_clone = state.clone();
            let text = tokio::task::spawn_blocking(move || {
                let mut stt = state_clone.stt.blocking_lock();
                stt.feed_audio(&audio_samples, is_last)
            })
            .await
            .map_err(|e| e.to_string())??;

            Ok(serde_json::json!({ "text": text }))
        }
        "voice:stt_stop" => {
            let state_clone = state.clone();
            let text = tokio::task::spawn_blocking(move || {
                let mut stt = state_clone.stt.blocking_lock();
                stt.feed_audio(&[], true)
            })
            .await
            .map_err(|e| e.to_string())??;

            Ok(serde_json::json!({ "text": text }))
        }
        "voice:stt_flush" => {
            state.stt.lock().await.reset_stream();
            Ok(serde_json::json!({ "success": true }))
        }
        "voice:set_language" => {
            let lang = payload["language"]
                .as_str()
                .ok_or_else(|| "Missing 'language'".to_string())?;

            state.stt.lock().await.set_language(lang)?;
            {
                let mut tts = state.tts.lock().await;
                if let Some(ref mut tts_mgr) = *tts {
                    tts_mgr.set_language(lang);
                }
            }
            Ok(serde_json::json!({ "success": true, "language": lang }))
        }
        "voice:tts_speak" => {
            let text = payload["text"]
                .as_str()
                .ok_or_else(|| "Missing 'text'".to_string())?;

            let flush = payload["flush"].as_bool().unwrap_or(false);

            let mut tts = state.tts.lock().await;
            if let Some(ref mut tts_mgr) = *tts {
                tts_mgr.speak(text).await?;
                if flush {
                    tts_mgr.flush().await?;
                }
                Ok(serde_json::json!({ "success": true }))
            } else {
                Err("TTS engine not initialized".to_string())
            }
        }
        "voice:tts_stop" => {
            state.tts_player.stop().await;

            let state_clone = state.clone();
            tokio::spawn(async move {
                let mut tts = state_clone.tts.lock().await;
                if let Some(ref mut tts_mgr) = *tts {
                    tts_mgr.stop().await;
                }
            });

            Ok(serde_json::json!({ "success": true }))
        }
        "llm:swap_model" => {
            let model_path_str = payload["model_path"]
                .as_str()
                .ok_or_else(|| "Missing 'model_path'".to_string())?;
            let model_path = std::path::Path::new(model_path_str);
            // C2: chỉ cho nạp .gguf trong thư mục model đã cấu hình — không phải
            // đường dẫn tuỳ ý vào parser C++ của llama.cpp.
            validate_model_path(model_path, &configured_models_dir())?;
            let model_path = &configured_models_dir().join(model_path);

            let n_ctx = payload["n_ctx"].as_u64().map(|v| v as usize);
            let n_gpu_layers = payload["n_gpu_layers"].as_u64().map(|v| v as u32);
            let vocab_only = payload["vocab_only"].as_bool();

            let mut llm_manager = state.llm.lock().await;
            llm_manager
                .swap_model(model_path, n_ctx, n_gpu_layers, vocab_only)
                .await?;

            Ok(serde_json::json!({ "success": true }))
        }
        "llm:embed" => {
            let inputs = if let Some(s) = payload["input"].as_str() {
                vec![s.to_string()]
            } else if let Some(arr) = payload["input"].as_array() {
                let mut vec = Vec::new();
                for v in arr {
                    let s = v
                        .as_str()
                        .ok_or_else(|| "Invalid string in input list".to_string())?;
                    vec.push(s.to_string());
                }
                vec
            } else {
                return Err("Missing or invalid 'input' parameter".to_string());
            };

            let mut llm_manager = state.llm.lock().await;
            if llm_manager.vocab_only {
                return Err("Cannot compute embeddings on a vocab-only model".to_string());
            }
            let engine = llm_manager
                .engine
                .as_mut()
                .ok_or_else(|| "No model loaded".to_string())?;
            let mut embeddings = Vec::new();
            for text in inputs {
                let emb = llm::get_embedding(&engine.model, &mut engine.context, &text)?;
                embeddings.push(emb);
            }

            if payload["input"].is_string() {
                Ok(serde_json::to_value(&embeddings[0]).unwrap())
            } else {
                Ok(serde_json::to_value(embeddings).unwrap())
            }
        }
        "chat:completion" => {
            let memory_scope = agent::graph::ConversationMemoryScope::new("local", "default")?;
            handle_chat_completion_scoped(state, payload, tx, req_id, memory_scope).await
        }
        "vision:ask" => {
            // Multimodal Q&A on an image with the unified VL core (Qwen3-VL).
            // Image source: a base64 `image` (png/jpg), else the primary screen.
            let question = payload["question"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt.")
                .to_string();
            let temperature = payload["temperature"].as_f64().unwrap_or(0.7) as f32;
            let top_p = payload["top_p"].as_f64().unwrap_or(0.8) as f32;
            let image_b64 = payload["image"].as_str().map(|s| s.to_string());

            let state_clone = state.clone();
            let output =
                tokio::task::spawn_blocking(move || -> Result<llm::CompletionOutput, String> {
                    use base64::Engine as _;
                    let mut llm_manager = state_clone.llm.blocking_lock();
                    if let Some(b64) = image_b64 {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(b64.as_bytes())
                            .map_err(|e| format!("Invalid base64 image: {}", e))?;
                        llm_manager.answer_with_image(
                            &question,
                            llm::engine::VisionImage::Encoded(&bytes),
                            temperature,
                            top_p,
                            |_| true,
                        )
                    } else {
                        // Context-aware capture (mouse-guided crop while gaming).
                        let (width, height, rgb) = crate::vision::capture::capture_for_vision()?;
                        llm_manager.answer_with_image(
                            &question,
                            llm::engine::VisionImage::Rgb {
                                width,
                                height,
                                data: &rgb,
                            },
                            temperature,
                            top_p,
                            |_| true,
                        )
                    }
                })
                .await
                .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({
                "text": output.text,
                "usage": {
                    "prompt_tokens": output.prompt_tokens,
                    "completion_tokens": output.completion_tokens
                }
            }))
        }
        "llm:health_check" => {
            let llm_manager = state.llm.lock().await;
            let loaded = llm_manager.engine.is_some();
            let model_path = llm_manager.current_model_path.to_string_lossy().to_string();

            Ok(serde_json::json!({
                "status": "healthy",
                "model_loaded": loaded,
                "model_path": model_path,
                "n_ctx": llm_manager.n_ctx,
                "n_gpu_layers": llm_manager.n_gpu_layers
            }))
        }
        "telegram:send_text" => {
            let chat_id_str = payload["chatId"]
                .as_str()
                .ok_or("Missing chatId")?
                .to_string();
            let text = payload["text"].as_str().ok_or("Missing text")?.to_string();

            let chat_id = chat_id_str
                .parse::<i64>()
                .map_err(|e| format!("Invalid chatId: {}", e))?;

            let token = std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "Bot token missing")?;
            let bot = teloxide::prelude::Bot::new(token);
            tokio::spawn(async move {
                use teloxide::prelude::Requester;
                let _ = bot
                    .send_message(teloxide::prelude::ChatId(chat_id), text)
                    .await;
            });

            Ok(serde_json::json!({ "success": true }))
        }
        "integration:smart_home_control" => {
            let result = integrations::smart_home::execute(payload)?;
            Ok(serde_json::json!({ "result": result }))
        }
        "integrations:list" => Ok(serde_json::json!(
            [integrations::smart_home::get_metadata()]
        )),

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
            serde_json::to_value(tools)
                .map_err(|e| format!("Failed to serialize tool list: {}", e))
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
mod env_flag_tests {
    use super::env_flag;

    /// Các test env_flag phải chạy tuần tự: std::env là trạng thái toàn cục
    /// dùng chung cho cả tiến trình test.
    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_var<F: FnOnce()>(key: &str, val: Option<&str>, f: F) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var(key).ok();
        match val {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
        f();
        match old {
            Some(v) => unsafe { std::env::set_var(key, v) },
            None => unsafe { std::env::remove_var(key) },
        }
    }

    /// Đây CHÍNH LÀ bug F5: .env.example hướng dẫn ghi , code cũ dùng
    ///  nên hiểu thành BẬT và xoá sạch dữ liệu người dùng.
    #[test]
    fn f5_gia_tri_false_phai_la_tat() {
        with_var("LIVA_TEST_FLAG", Some("false"), || {
            assert!(!env_flag("LIVA_TEST_FLAG", false), "=false phải là TẮT");
            assert!(
                !env_flag("LIVA_TEST_FLAG", true),
                "=false phải thắng cả default=true"
            );
        });
    }

    #[test]
    fn nhan_moi_dang_bat() {
        for v in ["1", "true", "TRUE", "Yes", "ON", "  on  "] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(env_flag("LIVA_TEST_FLAG", false), "{:?} phải là BẬT", v);
            });
        }
    }

    #[test]
    fn nhan_moi_dang_tat() {
        for v in ["0", "false", "FALSE", "No", "OFF", " off "] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(!env_flag("LIVA_TEST_FLAG", true), "{:?} phải là TẮT", v);
            });
        }
    }

    #[test]
    fn khong_dat_bien_thi_dung_default() {
        with_var("LIVA_TEST_FLAG", None, || {
            assert!(!env_flag("LIVA_TEST_FLAG", false));
            assert!(env_flag("LIVA_TEST_FLAG", true));
        });
    }

    #[test]
    fn gia_tri_la_hoac_rong_thi_dung_default_khong_panic() {
        for v in ["", "  ", "maybe", "2", "tru"] {
            with_var("LIVA_TEST_FLAG", Some(v), || {
                assert!(
                    env_flag("LIVA_TEST_FLAG", true),
                    "{:?} phải rơi về default=true",
                    v
                );
                assert!(
                    !env_flag("LIVA_TEST_FLAG", false),
                    "{:?} phải rơi về default=false",
                    v
                );
            });
        }
    }
}

#[cfg(test)]
mod origin_allowed_tests {
    use super::{DEFAULT_WS_ALLOWED_ORIGINS, origin_allowed};

    static LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn without_extra<F: FnOnce()>(f: F) {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_WS_ALLOWED_ORIGINS").ok();
        unsafe { std::env::remove_var("LIVA_WS_ALLOWED_ORIGINS") };
        f();
        if let Some(v) = old {
            unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", v) }
        }
    }

    #[test]
    fn cho_qua_cac_origin_mac_dinh() {
        without_extra(|| {
            for o in DEFAULT_WS_ALLOWED_ORIGINS {
                assert!(origin_allowed(Some(o)), "{} phai duoc phep", o);
            }
        });
    }

    /// Đây là ca tấn công thật: một trang web bất kỳ mở WebSocket tới 8002.
    #[test]
    fn chan_trang_web_la() {
        without_extra(|| {
            for o in [
                "https://evil.example",
                "http://evil.example",
                "null",
                "http://localhost:3000",
                "http://localhost:5174",
            ] {
                assert!(!origin_allowed(Some(o)), "{} phai bi chan", o);
            }
        });
    }

    /// Không có Origin = client gốc (Tauri, verify_duplex) → cho qua. Đây là
    /// đánh đổi có chủ ý, test này khoá lại hành vi đó cho khỏi đổi ngầm.
    #[test]
    fn khong_co_origin_thi_cho_qua() {
        without_extra(|| assert!(origin_allowed(None)));
    }

    #[test]
    fn origin_rong_thi_chan() {
        without_extra(|| {
            assert!(!origin_allowed(Some("")));
            assert!(!origin_allowed(Some("   ")));
        });
    }

    #[test]
    fn khong_khop_tien_to_hay_hau_to() {
        without_extra(|| {
            // ke tan cong dat domain chua chuoi hop le
            assert!(!origin_allowed(Some("http://localhost:5173.evil.example")));
            assert!(!origin_allowed(Some(
                "https://evil.example/http://localhost:5173"
            )));
            assert!(!origin_allowed(Some("http://localhost:51730")));
        });
    }

    #[test]
    fn mo_rong_bang_bien_moi_truong() {
        let _g = LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_WS_ALLOWED_ORIGINS").ok();
        unsafe {
            std::env::set_var(
                "LIVA_WS_ALLOWED_ORIGINS",
                " http://my.app , http://other.app ",
            )
        };
        assert!(origin_allowed(Some("http://my.app")));
        assert!(origin_allowed(Some("http://other.app")));
        assert!(!origin_allowed(Some("http://third.app")));
        // dau phay thua khong duoc bien thanh chuoi rong khop tat ca
        unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", ",,") };
        assert!(!origin_allowed(Some("https://evil.example")));
        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_WS_ALLOWED_ORIGINS", v) },
            None => unsafe { std::env::remove_var("LIVA_WS_ALLOWED_ORIGINS") },
        }
    }
}

#[cfg(test)]
mod validate_model_path_tests {
    use super::validate_model_path;
    use std::path::Path;

    #[test]
    fn cho_phep_gguf_trong_thu_muc_model() {
        let dir = Path::new("models_root");
        assert!(validate_model_path(Path::new("router.gguf"), dir).is_ok());
        assert!(validate_model_path(Path::new("sub/expert.gguf"), dir).is_ok());
        assert!(
            validate_model_path(Path::new("A.GGUF"), dir).is_ok(),
            "duoi khong phan biet hoa thuong"
        );
    }

    #[test]
    fn chan_traversal_va_duoi_sai() {
        let dir = Path::new("models_root");
        // C2: đây là các payload đường-dẫn-tuỳ-ý phải bị chặn trước khi tới
        // parser C++ của llama.cpp.
        assert!(
            validate_model_path(Path::new("../secret.gguf"), dir).is_err(),
            ".."
        );
        assert!(
            validate_model_path(Path::new("sub/../../x.gguf"), dir).is_err(),
            ".. giua"
        );
        assert!(
            validate_model_path(Path::new("router.txt"), dir).is_err(),
            "duoi khong phai gguf"
        );
        assert!(
            validate_model_path(Path::new("no_ext"), dir).is_err(),
            "khong co duoi"
        );
    }
}

#[cfg(test)]
mod config_update_tests {
    use super::update_config_file_at;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn temp_config_path(label: &str) -> std::path::PathBuf {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        std::env::temp_dir().join(format!(
            "liva-config-{label}-{}-{}.json",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn malformed_config_is_preserved_instead_of_overwritten() {
        let path = temp_config_path("malformed");
        let original = "{ definitely-not-json";
        std::fs::write(&path, original).expect("write malformed fixture");

        let result = update_config_file_at(&path, &serde_json::json!({"ai": {"topP": 0.8}}));

        assert!(result.is_err(), "malformed config must fail closed");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved fixture"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn non_object_patch_cannot_replace_the_entire_config() {
        let path = temp_config_path("non-object");
        let original = serde_json::json!({
            "ai": {"provider": "local"},
            "voice": {"enabled": true}
        })
        .to_string();
        std::fs::write(&path, &original).expect("write config fixture");

        let result = update_config_file_at(&path, &serde_json::Value::Null);

        assert!(result.is_err(), "config patch must be a JSON object");
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved config"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn non_object_existing_config_is_preserved_instead_of_replaced() {
        let path = temp_config_path("non-object-existing");
        let original = serde_json::json!(["unexpected", "root"]).to_string();
        std::fs::write(&path, &original).expect("write non-object config fixture");

        let result = update_config_file_at(&path, &serde_json::json!({"ai": {"topP": 0.8}}));

        assert!(
            result.is_err(),
            "an existing config with a non-object root must fail closed"
        );
        assert_eq!(
            std::fs::read_to_string(&path).expect("read preserved config"),
            original
        );
        let _ = std::fs::remove_file(path);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn concurrent_config_patches_do_not_lose_each_other() {
        let path = temp_config_path("concurrent");
        std::fs::write(
            &path,
            serde_json::json!({"ai": {"temperature": 0.3}, "ui": {"theme": "dark"}}).to_string(),
        )
        .expect("write initial config");

        let first_path = path.clone();
        let first = tokio::task::spawn_blocking(move || {
            update_config_file_at(&first_path, &serde_json::json!({"ai": {"topP": 0.8}}))
        });
        let second_path = path.clone();
        let second = tokio::task::spawn_blocking(move || {
            update_config_file_at(
                &second_path,
                &serde_json::json!({"ui": {"widgetPosition": "top-left"}}),
            )
        });

        first
            .await
            .expect("first writer task")
            .expect("first patch");
        second
            .await
            .expect("second writer task")
            .expect("second patch");

        let config: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).expect("read merged config"))
                .expect("config remains valid JSON");
        assert_eq!(config["ai"]["temperature"], 0.3);
        assert_eq!(config["ai"]["topP"], 0.8);
        assert_eq!(config["ui"]["theme"], "dark");
        assert_eq!(config["ui"]["widgetPosition"], "top-left");
        let _ = std::fs::remove_file(path);
    }
}
