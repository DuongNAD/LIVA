pub mod agent;
mod artifact_trust;
mod authorization;
pub mod boot;
pub mod commands;
pub mod consent;
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
pub mod memory_retention;
pub mod messaging;
#[cfg(feature = "experimental")]
pub mod passive;
pub mod persistence_backup;
pub mod setup;
pub mod skills;
pub mod stt;
pub mod sysinfo;
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
const CONFIG_FILE_NAME: &str = "liva-config.json";
pub const DEFAULT_MODELS_DIR: &str = "E:\\AI_Models";
/// Model router mặc định khi chưa có config.
///
/// **Phải trùng file mà trình tải model thật sự tải về** (nhóm `chat` trong
/// `data/models-manifest.json`) — nếu không, một máy mới cài sẽ tải xong model
/// rồi vẫn không chat được, vì bộ nạp đi tìm một tên file chưa bao giờ được tải.
/// Ràng buộc đó được một test giữ: `setup::tests::router_mac_dinh_khop_manifest`.
/// Trước 28/07/2026 hằng này còn là `gemma-4-E4B-…`, một model đã không còn nằm
/// trong danh sách tải từ khi router chuyển sang Qwen3-VL.
pub const DEFAULT_ROUTER_MODEL: &str = "Qwen3-VL-2B-Instruct-GGUF/Qwen3-VL-2B-Instruct-Q4_K_M.gguf";
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

/// The working directory differs per entry point (repo root, liva-native-core,
/// or liva-desktop/src-tauri), so walk up to two levels to find the project's
/// real data/liva-config.json instead of silently reading an empty one.
///
/// Neo DUY NHẤT là [`data_dir`], nên config và database không bao giờ trôi khỏi
/// nhau. Bản cũ có bộ dò riêng và **rơi về đường dẫn tương đối** khi hụt: bản
/// cài ghi `data\liva-config.json` vào thư mục cài (mất khi gỡ/nâng cấp) hoặc
/// vào bất kỳ thư mục nào shortcut trỏ tới. Đó đúng là lỗi "ba database do cwd"
/// (commit 46afef4), chỉ đổi đối tượng từ database sang config.
pub fn config_file_path() -> std::path::PathBuf {
    data_dir().join(CONFIG_FILE_NAME)
}

/// Thư mục dữ liệu **ghi được** — một chỗ duy nhất, KHÔNG phụ thuộc cwd.
///
/// ## Vì sao cần, và vì sao nó khác `vieneu_model_dir`
///
/// Bộ giải dò-lên-hai-cấp dùng cho **model** là đúng, vì model chỉ-đọc và mọi
/// bản đều giống hệt nhau — tìm thấy bản nào cũng như nhau. Với **trạng thái
/// ghi được** thì cách đó SAI: nó tìm thấy bản *gần nhất*, nên mỗi cwd cho một
/// database khác nhau, và mỗi bản là một trạng thái riêng.
///
/// Đo được ngày 27/07/2026 — ba database `liva_core` cùng tồn tại trên một máy:
///
/// ```text
/// data/agents/liva_core/                        32 KB   (chạy từ gốc repo)
/// liva-desktop/src-tauri/data/agents/liva_core/ 32 KB   (npm run dev → tauri dev)
/// liva-native-core/data/agents/liva_core/      118 KB   (cargo run trong crate)
/// ```
///
/// Triệu chứng với người dùng: thêm một liên hệ vào sổ danh bạ, khởi động LIVA
/// bằng cách khác, danh bạ **trống** — LIVA chỉ nói "chưa có ai tên đó". Không
/// lỗi, không log, không có gì để lần ra. Đã cắn ba lần trong một buổi.
///
/// ## Neo được chọn
///
/// 1. Thư mục chứa `data/liva-config.json` nếu tìm thấy (dò lên hai cấp) — tức
///    **gốc repo** khi chạy từ mã nguồn. Cùng neo với [`config_file_path`], nên
///    cấu hình và dữ liệu luôn nằm cạnh nhau thay vì trôi khỏi nhau.
/// 2. Nếu không có (bản đóng gói, không có cây mã nguồn): thư mục dữ liệu theo
///    người dùng của HĐH — `%LOCALAPPDATA%\com.liva.cognitive-os\data`.
/// 3. Cùng đường bí: `./data` như cũ.
///
/// `LIVA_DB_PATH` vẫn thắng tất cả — đó là đường thoát đã tài liệu hoá.
pub fn data_dir() -> std::path::PathBuf {
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(CONFIG_REL_PATH);
        if candidate.exists()
            && let Some(parent) = candidate.parent()
        {
            return parent.to_path_buf();
        }
    }
    if let Some(home) = user_home_dir() {
        return home.join("data");
    }
    std::path::PathBuf::from("data")
}

/// Thư mục dữ liệu người dùng của bản cài, đặt theo bundle id — cùng quy ước với
/// `app_local_data_dir()` của Tauri (nơi vault Stronghold đã nằm sẵn).
pub const APP_DATA_DIR_NAME: &str = "com.liva.cognitive-os";

/// Neo dữ liệu của các bản trước 28/07/2026. Xem [`user_home_dir`].
const LEGACY_DATA_DIR_NAME: &str = "LIVA";

/// Có dấu vết dữ liệu THẬT của người dùng dưới `home` không?
///
/// Cố ý KHÔNG hỏi "thư mục `data` có tồn tại không": bộ cài NSIS đặt sẵn
/// `data\models-manifest.json` vào thư mục cài, nên phép kiểm ngây thơ đó sẽ
/// thấy một thư mục `data` trên máy vừa cài lần đầu và kết luận nhầm là đang
/// nâng cấp — rồi ghim dữ liệu vào đúng chỗ ta đang tìm cách rời khỏi.
///
/// Ba dấu vết dưới đây chỉ do LIVA **đang chạy** tạo ra, không do bộ cài.
fn co_du_lieu_nguoi_dung(home: &std::path::Path) -> bool {
    let data = home.join("data");
    if data.join(CONFIG_FILE_NAME).exists() {
        return true;
    }
    if data
        .join("agents")
        .join("liva_core")
        .join("structured_memory.sqlite")
        .exists()
    {
        return true;
    }
    // Model đã tải cũng là dữ liệu thật — bỏ qua nó là bắt người dùng tải lại
    // 2,28 GB. Bộ cài không bao giờ tạo `models/`, nên đây không thể là dấu vết
    // của trình cài đặt.
    std::fs::read_dir(home.join("models")).is_ok_and(|mut d| d.next().is_some())
}

/// Gốc dữ liệu **của người dùng**: `LIVA_HOME` → `%LOCALAPPDATA%\com.liva.cognitive-os`
/// (hoặc neo cũ `%LOCALAPPDATA%\LIVA` nếu dữ liệu thật đang nằm ở đó).
///
/// ## Vì sao KHÔNG dùng `%LOCALAPPDATA%\LIVA`
///
/// Đó chính là **thư mục cài**: NSIS đặt `StrCpy $INSTDIR "$LOCALAPPDATA\${PRODUCTNAME}"`
/// (`target/release/nsis/x64/installer.nsi:503`, `PRODUCTNAME = LIVA`). Dùng nó
/// làm thư mục dữ liệu thì 2,28 GB model nằm lẫn trong thư mục cài, và câu
/// "dữ liệu sống sót qua gỡ cài đặt" trong tài liệu là **sai** — trình gỡ dọn
/// thư mục cài. Bundle id thì đã là quy ước sẵn có: két Stronghold vốn nằm ở
/// `%LOCALAPPDATA%\com.liva.cognitive-os` (`app_local_data_dir()` của Tauri),
/// nên dồn về đó là gom một chỗ chứ không phải phát minh thêm chỗ thứ ba.
///
/// ## Vì sao vẫn phải nhìn chỗ cũ
///
/// Bản phát hành trước đã ghi dữ liệu vào neo cũ. Đổi neo mà bỏ qua vế này là
/// làm người đang dùng mất sạch — ký ức và model vẫn nằm nguyên đó, chỉ là không
/// ai đọc nữa. Thứ tự: có dữ liệu ở chỗ MỚI thì dùng chỗ mới (đã di trú, đừng
/// kéo ngược); không thì có dữ liệu ở chỗ CŨ thì dùng chỗ cũ; không nữa thì là
/// máy cài mới → chỗ mới.
///
/// `LIVA_HOME` thắng tất cả: người dùng muốn để model sang ổ khác, và test dựng
/// được môi trường bản cài mà không đụng `%LOCALAPPDATA%` thật.
pub fn user_home_dir() -> Option<std::path::PathBuf> {
    if let Some(h) = std::env::var_os("LIVA_HOME")
        && !h.is_empty()
    {
        return Some(std::path::PathBuf::from(h));
    }
    let local = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    let moi = local.join(APP_DATA_DIR_NAME);
    if co_du_lieu_nguoi_dung(&moi) {
        return Some(moi);
    }
    let cu = local.join(LEGACY_DATA_DIR_NAME);
    if co_du_lieu_nguoi_dung(&cu) {
        return Some(cu);
    }
    Some(moi)
}

/// Gốc để **ghi** tài nguyên lớn (model tải về) và để dò tài nguyên của bản cài.
///
/// Suy ra từ [`data_dir`] chứ không tự dò lại: hai bộ dò song song là cách chắc
/// chắn nhất để config nằm một nơi còn model nằm nơi khác.
/// Trong cây mã nguồn → gốc repo. Bản cài → `%LOCALAPPDATA%\com.liva.cognitive-os`.
pub fn resource_write_root() -> std::path::PathBuf {
    let d = data_dir();
    match d.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

/// Những chỗ database CÓ THỂ đã bị tạo nhầm do cwd, không tính chỗ đang dùng.
///
/// Chỉ để **báo cho người dùng biết**, không tự động di trú: gộp hai file
/// SQLite là thao tác mất mát tiềm tàng, và người dùng phải là người quyết định
/// giữ bản nào.
pub fn stray_database_paths(dang_dung: &std::path::Path) -> Vec<std::path::PathBuf> {
    const REL: &str = "data/agents/liva_core/structured_memory.sqlite";
    let dang_dung = dang_dung.canonicalize().ok();
    [
        "",
        "..",
        "../..",
        "liva-native-core",
        "liva-desktop/src-tauri",
    ]
    .iter()
    .map(|p| std::path::Path::new(p).join(REL))
    .filter(|p| p.exists())
    .filter(|p| p.canonicalize().ok() != dang_dung)
    .collect()
}

fn read_config_file() -> serde_json::Value {
    std::fs::read_to_string(config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Thư mục chứa executable đang chạy. `None` khi HĐH không cho biết.
pub fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
}

/// Nơi có thể chứa một tài nguyên **chỉ-đọc** (model, manifest, giọng đọc), theo
/// thứ tự ưu tiên.
///
/// Thuần (nhận `user_root`/`exe_dir` làm tham số) để test được mà không phải giả
/// lập vị trí executable — cùng cách [`db::vec0_candidate_paths`] đã làm cho
/// `vec0.dll`, và vì cùng một lý do: đó là hàm duy nhất trong repo từng được sửa
/// cho đúng bản cài, nên nó là mẫu đã chứng minh được.
///
/// Thứ tự KHÔNG được đổi:
///
/// 1. **cwd và hai cấp trên** — bản dev. Phải đứng đầu, nếu không một máy dev có
///    lỡ tải model vào `%LOCALAPPDATA%` sẽ âm thầm đọc model của bản cài thay vì
///    model trong cây làm việc.
/// 2. **Thư mục dữ liệu người dùng** — nơi trình cài đặt lần đầu tải model về.
/// 3. **Cạnh executable và `resources/`** — nơi `bundle.resources` của Tauri đặt
///    file. cwd của bản cài KHÔNG phải thư mục exe (shortcut trỏ đâu thì cwd ở
///    đó), nên hai ứng viên này không thay thế được nhau.
pub fn resource_candidate_paths(
    rel: &str,
    user_root: Option<&std::path::Path>,
    exe_dir: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let raw = std::path::Path::new(rel);
    let mut ds: Vec<std::path::PathBuf> = ["", "..", "../.."]
        .iter()
        .map(|p| std::path::Path::new(p).join(raw))
        .collect();
    if let Some(root) = user_root {
        ds.push(root.join(raw));
    }
    if let Some(dir) = exe_dir {
        ds.push(dir.join(raw));
        ds.push(dir.join("resources").join(raw));
    }
    ds
}

/// Resolve a repo-relative resource path (models/, node_modules/, ...) against
/// the actual project root, whatever the working directory is (repo root,
/// liva-native-core, or liva-desktop/src-tauri). Absolute paths pass through.
///
/// Từ bản cài trở đi còn dò thêm thư mục dữ liệu người dùng và thư mục exe —
/// xem [`resource_candidate_paths`].
pub fn resolve_resource_path(rel: &str) -> std::path::PathBuf {
    let raw = std::path::Path::new(rel);
    if raw.is_absolute() {
        return raw.to_path_buf();
    }
    let user_root = resource_write_root();
    let exe = exe_dir();
    for candidate in resource_candidate_paths(rel, Some(&user_root), exe.as_deref()) {
        if candidate.exists() {
            return candidate;
        }
    }
    raw.to_path_buf()
}

/// Thư mục vault Obsidian mặc định khi `LIVA_VAULT_PATH` không được đặt.
///
/// Trước đây là đường dẫn tuyệt đối của máy dev (`E:\Project\LIVA\...`) ghi
/// thẳng trong `boot.rs` — trên máy người dùng đó là một ổ đĩa không tồn tại.
pub fn default_vault_path() -> std::path::PathBuf {
    let trong_repo = resolve_resource_path("teamwork_projects/obsidian_llm_wiki/vault");
    if trong_repo.exists() {
        return trong_repo;
    }
    resource_write_root().join("vault")
}

/// Thư mục model LLM mặc định khi config chưa nói gì.
///
/// KHÔNG dùng [`DEFAULT_MODELS_DIR`] (`E:\AI_Models`, ổ đĩa của máy dev): trên
/// máy người dùng nó không tồn tại, nên router không nạp được model và LIVA lên
/// nhưng không trả lời — đúng kiểu hỏng im lặng mà `npm run doctor` sinh ra để
/// bắt. Neo vào cùng gốc với dữ liệu người dùng để trình tải model lần đầu và
/// bộ nạp LLM luôn nhìn vào một chỗ.
pub fn models_dir_fallback() -> std::path::PathBuf {
    resource_write_root().join("models").join("llm")
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
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback);
    let model = ai
        .get("routerModel")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_ROUTER_MODEL);
    Some(dir.join(model))
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
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback);
    let mmproj = ai
        .get("mmprojModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(dir.join(mmproj))
}

/// Thư mục model được cấu hình (`ai.localModelsDir`, fallback [`models_dir_fallback`]).
pub fn configured_models_dir() -> std::path::PathBuf {
    let config = read_config_file();
    config
        .get("ai")
        .and_then(|ai| ai.get("localModelsDir"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback)
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

/// Bảng sức khoẻ hệ thống cho Dashboard — **chỉ số đo thật**.
///
/// Bản trước in cứng 12 trường (`cpuUsage: 12`, `totalMemory: 16e9`,
/// `uptime: 3600`, `rssMemory: 100_000_000`, `voiceEngine.latencyMs: 5`, mọi
/// service `"online"`, `telegram: "online"`…). Chỉ `modelLoaded`/`model` là
/// thật. `SystemView.vue` poll nó **3 giây một lần** để vẽ 8 đèn xanh, nên
/// người dùng luôn thấy một hệ thống khoẻ mạnh — kể cả khi không có model nào,
/// không có ai kết nối, và bot Telegram chưa bao giờ chạy.
///
/// Hai quy ước của hàm này:
///
/// 1. **Không đo được thì `null`/`"unknown"`, không điền số mặc định.** UI đã
///    sẵn sàng cho việc đó (`?? -1`, `|| '--'`), nên một ô trống nói thật rẻ hơn
///    một con số đẹp nói dối.
/// 2. **`try_lock`, không `lock().await`.** Bản trước chờ `state.llm.lock()`:
///    trong lúc LLM đang sinh chữ, lock bị giữ suốt lượt sinh, nên một lệnh
///    "xem trạng thái" biến thành lệnh chờ vài giây — mà UI thì poll mỗi 3s,
///    hàng đợi dồn lại. Lock đang bận **cũng là thông tin thật**: báo `"busy"`.
///
/// Tách khỏi `handle_command` (đang là một `match` ~1 400 dòng) để test được
/// riêng và để phần thân này còn đọc được.
pub async fn system_status(state: Arc<AppState>) -> Result<serde_json::Value, String> {
    use serde_json::json;

    // --- LLM ---------------------------------------------------------------
    let (ai_status, ai_detail, model_name, model_loaded) = match state.llm.try_lock() {
        Ok(m) => {
            let name = m
                .current_model_path
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_default();
            let loaded = m.engine.is_some();
            let detail = if loaded {
                format!(
                    "n_ctx {} · {} lớp GPU · mmproj {}",
                    m.n_ctx,
                    m.n_gpu_layers,
                    if m.mmproj_path.is_some() {
                        "có"
                    } else {
                        "không"
                    }
                )
            } else {
                "chưa nạp model".to_string()
            };
            (
                if loaded { "online" } else { "offline" },
                detail,
                Some(name),
                Some(loaded),
            )
        }
        // Lock bận = engine đang sinh chữ. Đó là "đang chạy", không phải "hỏng".
        //
        // Tên model vẫn báo được: lấy từ CẤU HÌNH, tức chính file mà autoload và
        // `llm:swap_model` nạp. Nếu để `null` ở đây thì ô "Model" trên Dashboard
        // sẽ nhấp nháy về `--` mỗi lần LIVA trả lời — mất một thông tin ổn định
        // chỉ vì một lock tạm thời. `modelLoaded` thì vẫn `null`: cái đó đúng là
        // không biết được khi không cầm được lock.
        Err(_) => (
            "busy",
            "đang sinh chữ".to_string(),
            configured_router_model_path()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string())),
            None,
        ),
    };

    // --- STT ---------------------------------------------------------------
    // `LIVA_STT_VI_ENGINE` đọc thẳng từ env: đây đúng là nguồn sự thật mà
    // `SttManager::new` dùng, nên không có đường lệch giữa hai chỗ.
    let stt_engine_name = if std::env::var("LIVA_STT_VI_ENGINE")
        .map(|v| v.trim().eq_ignore_ascii_case("parakeet"))
        .unwrap_or(false)
    {
        "Parakeet-vi"
    } else {
        "Nemotron"
    };
    let (stt_status, stt_detail) = match state.stt.try_lock() {
        Ok(s) if s.model_dir.exists() => ("online", format!("{stt_engine_name} · model có sẵn")),
        Ok(s) => (
            "offline",
            format!("thiếu model tại {}", s.model_dir.display()),
        ),
        Err(_) => ("busy", "đang nhận dạng".to_string()),
    };

    // --- TTS + phụ trợ thoại ------------------------------------------------
    let (tts_status, tts_detail) = match state.tts.try_lock() {
        Ok(guard) => match guard.as_ref() {
            Some(t) => {
                let backends = t.loaded_backends();
                if backends.is_empty() {
                    (
                        "offline",
                        "TtsManager có nhưng KHÔNG backend nào nạp được".to_string(),
                    )
                } else {
                    (
                        "online",
                        format!("{} · giọng {}", backends.join(" → "), t.language()),
                    )
                }
            }
            None => ("offline", "TTS không khởi tạo được".to_string()),
        },
        Err(_) => ("busy", "đang phát".to_string()),
    };

    // Module phụ trợ có nạp được không. `try_lock` lỗi nghĩa là module đang
    // ĐƯỢC DÙNG ⇒ nó chắc chắn tồn tại. Viết bằng macro vì mỗi `Mutex` bọc một
    // kiểu khác nhau, mà closure Rust không nhận tham số `impl Trait`.
    macro_rules! co_module {
        ($m:expr) => {
            match $m.try_lock() {
                Ok(g) => g.is_some(),
                Err(_) => true,
            }
        };
    }
    let vad = co_module!(state.vad);
    let denoise = co_module!(state.denoiser);
    let aec = co_module!(state.aec);
    let turn_shadow = co_module!(state.turn_shadow);
    let embedder = co_module!(state.embedder);

    let voice_status = if stt_status == "offline" || tts_status == "offline" {
        "degraded"
    } else if stt_status == "busy" || tts_status == "busy" {
        "busy"
    } else {
        "online"
    };
    let voice_detail = format!(
        "TTS: {tts_detail} · VAD {} · khử ồn {} · AEC {} · turn-shadow {}",
        bat_tat(vad),
        bat_tat(denoise),
        bat_tat(aec),
        bat_tat(turn_shadow),
    );

    // --- DB ----------------------------------------------------------------
    // Truy vấn SQLite là I/O chặn — phải nằm trong `spawn_blocking`, nếu không
    // một lệnh poll 3 giây/lần sẽ chặn luồng async của cả runtime.
    let db_probe = {
        let db = state.db.clone();
        tokio::task::spawn_blocking(move || -> Result<(String, bool, i64), String> {
            let conn = db.readers.get().map_err(|e| e.to_string())?;
            let journal: String = conn
                .query_row("PRAGMA journal_mode", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            // Cùng cách phát hiện vec0 mà `db::load_sqlite_vec` dùng.
            let vec0 = conn
                .query_row("SELECT vec_version()", [], |r| r.get::<_, String>(0))
                .is_ok();
            let facts: i64 = conn
                .query_row("SELECT count(*) FROM facts", [], |r| r.get(0))
                .unwrap_or(-1);
            Ok((journal, vec0, facts))
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {e}"))?
    };
    let (mem_status, mem_detail) = match &db_probe {
        Ok((journal, vec0, facts)) => (
            if *vec0 { "online" } else { "degraded" },
            format!(
                "journal {journal} · vec0 {} · {} ký ức · RAG {}",
                if *vec0 { "có" } else { "THIẾU" },
                if *facts < 0 {
                    "?".to_string()
                } else {
                    facts.to_string()
                },
                bat_tat(embedder),
            ),
        ),
        Err(e) => ("offline", format!("không mở được DB: {e}")),
    };

    // --- GPU / VRAM ---------------------------------------------------------
    let vram = governor::gpu_vram_bytes();
    let gpu_pct = governor::system_gpu_percent();
    let (vram_status, vram_detail) = match vram {
        Some((tong, dung)) if tong > 0 => (
            "online",
            format!(
                "VRAM {:.1}/{:.1} GB ({}%){}",
                dung as f64 / 1024.0_f64.powi(3),
                tong as f64 / 1024.0_f64.powi(3),
                dung * 100 / tong,
                match gpu_pct {
                    Some(p) => format!(" · tải ngoài {p}%"),
                    None => String::new(),
                }
            ),
        ),
        // Không có NVIDIA/driver thì KHÔNG biết gì về VRAM. Bản trước báo
        // "online · 0% utilized" trên mọi máy, kể cả máy chỉ có iGPU.
        _ => (
            "unknown",
            "không đọc được NVML (không có GPU NVIDIA hoặc thiếu driver)".to_string(),
        ),
    };

    // --- Cổng vào / kỹ năng / điều khiển từ xa -------------------------------
    let ws_clients = websocket::ws_client_count();
    // Lấy độ dài từ CHÍNH mảng mà `get_skills_list` trả về, để hai lệnh không
    // bao giờ nói hai con số khác nhau.
    let skills = json!([integrations::smart_home::get_metadata()]);
    let skills_loaded = skills.as_array().map_or(0, |a| a.len());
    let mcp_tools = state.mcp_server.list_tools().tools.len();

    let tg_token = std::env::var("TELEGRAM_BOT_TOKEN")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let tg_running = telegram::bot_running();

    // --- Số đo hệ thống ------------------------------------------------------
    // `system_cpu_percent` so sánh hai mẫu liên tiếp nên lần gọi ĐẦU trả None —
    // UI poll 3s nên ô CPU trống đúng một nhịp rồi có số. Không lấp bằng 0.
    // MỘT lần lấy mẫu cho cả hai số. Gọi `system_cpu_percent()` rồi gọi tiếp
    // một hàm nữa sẽ làm hàm sau chỉ còn khoảng thời gian ~0 để chia — xem
    // cảnh báo ở `governor::cpu_sample`.
    let (cpu, liva_cpu) = match governor::cpu_sample() {
        Some((ngoai, cua_liva)) => (Some(ngoai), Some(cua_liva)),
        None => (None, None),
    };
    let ram = sysinfo::ram_bytes();
    let proc_mem = sysinfo::process_memory_bytes();

    Ok(json!({
        "healthChecks": {
            "gateway": {
                "status": "online",
                "wsClients": ws_clients,
                "skillsLoaded": skills_loaded,
                "detail": format!("{ws_clients} client · {skills_loaded} kỹ năng · {mcp_tools} công cụ MCP"),
            },
            "aiEngine": {
                "status": ai_status,
                // Đo độ trễ sinh chữ đòi phải CHẠY một lượt suy luận. Một bảng
                // trạng thái không được phép tự ý làm việc đó (tốn GPU/CPU và
                // làm nhiễu chính thứ nó đang đo) ⇒ không có số thì để trống.
                "latencyMs": serde_json::Value::Null,
                "detail": ai_detail,
            },
            // Không có "orchestrator" nào trong lõi Rust; thứ có thật là tầng
            // dispatch của `handle_command`. Nếu bạn đọc được phản hồi này thì
            // nó đang chạy — đó là toàn bộ những gì khẳng định được.
            "orchestrator": { "status": "online", "detail": "dispatch in-process" },
            "voiceEngine": {
                "status": voice_status,
                "latencyMs": serde_json::Value::Null,
                "detail": voice_detail,
            },
            "memory": { "status": mem_status, "detail": mem_detail },
            "vramGuard": {
                "status": vram_status,
                "detail": vram_detail,
                "isYielded": governor::game_mode_active_now(),
            },
            // Tên "whisper" là di sản của UI; engine thật là Nemotron/Parakeet.
            "whisper": { "status": stt_status, "detail": stt_detail },
            "remoteControl": {
                "enabled": tg_token,
                "telegram": {
                    "status": match (tg_token, tg_running) {
                        (false, _) => "not_configured",
                        (true, true) => "online",
                        // Có token mà bot không chạy: đúng tình trạng của vỏ
                        // Tauri, vì chỉ `main.rs` spawn bot.
                        (true, false) => "standby",
                    },
                },
                // Không có tích hợp Zalo trong mã nguồn. Trước đây báo
                // "offline" — nghe như một dịch vụ đang tắt, không phải một
                // dịch vụ chưa từng tồn tại.
                "zalo": { "status": "not_configured" },
            },
        },
        "osStats": {
            "cpuUsage": cpu,
            // Phần CPU của CHÍNH LIVA, cùng mẫu số với `cpuUsage` (U16). Có hai
            // số cạnh nhau mới nói được điều đáng nói: "máy bận 92 %, LIVA
            // chiếm 3 %". Một mình `cpuUsage` chỉ chứng minh máy đang bận, chứ
            // không chứng minh LIVA rẻ.
            "livaCpuUsage": liva_cpu,
            "gpuUsage": gpu_pct,
            "totalMemory": ram.map(|(t, _)| t),
            "freeMemory": ram.map(|(_, f)| f),
        },
        "telemetry": [],
        "uptime": sysinfo::process_uptime_secs(),
        // `memoryUsage` = commit charge. Rust không có heap do runtime quản lý
        // nên không có gì báo cáo dưới cái tên "heap" — xem `sysinfo`.
        "memoryUsage": proc_mem.map(|(_, commit)| commit),
        "rssMemory": proc_mem.map(|(rss, _)| rss),
        "engineMode": "native",
        "modelLoaded": model_loaded,
        "model": model_name,
    }))
}

/// `"có"`/`"không"` cho phần `detail` — gọn hơn `if` lặp lại sáu lần.
fn bat_tat(v: bool) -> &'static str {
    if v { "có" } else { "không" }
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
