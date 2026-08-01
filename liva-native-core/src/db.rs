#![allow(non_snake_case)]
mod deletion;

pub use deletion::{
    ConversationDeletionCounts, ConversationDeletionReport, RetentionSweepReport,
    SubjectDeletionCounts, SubjectDeletionReport, delete_conversation, delete_subject,
    sweep_conversation_retention,
};

use crate::crypto::EncryptionEngine;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{
    Connection, OpenFlags,
    types::{ToSql, Value},
};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct CustomSqliteManager {
    inner: Arc<SqliteConnectionManager>,
    read_only: bool,
}

impl r2d2::ManageConnection for CustomSqliteManager {
    type Connection = Connection;
    type Error = rusqlite::Error;

    fn connect(&self) -> Result<Self::Connection, Self::Error> {
        let conn = self.inner.connect()?;
        if let Err(e) = load_sqlite_vec(&conn) {
            eprintln!("Warning: Failed to load sqlite-vec: {:?}", e);
        }
        configure_connection(&conn, self.read_only)?;
        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        self.inner.is_valid(conn)
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        self.inner.has_broken(conn)
    }
}

fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        PRAGMA cache_size = -8192;
        PRAGMA page_size = 32768;
        PRAGMA mmap_size = 268435456;
    ",
    )?;

    if read_only {
        conn.execute("PRAGMA synchronous = NORMAL", [])?;
    } else {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA wal_autocheckpoint = 500;
        ",
        )?;
    }
    Ok(())
}

/// Danh sách đường dẫn thử nạp `vec0` (sqlite-vec), theo thứ tự ưu tiên. Tách
/// THUẦN (nhận `exe_dir`) để test được mà không phụ thuộc môi trường.
///
/// Bao ba tình huống:
/// - **dev** (chạy từ repo): `node_modules/…/vec0.dll` quanh cwd;
/// - **đóng gói** (app Tauri cài đặt, KHÔNG có node_modules): cạnh executable và
///   trong `resources/` — nơi `bundle.resources` của Tauri đặt file. Đây là lý do
///   H5 (thiếu vec0 → DB sập lúc boot): candidate cũ chỉ dựa vào cwd, còn app cài
///   đặt thì cwd không phải thư mục exe;
/// - **hệ thống**: `vec0` trần để `load_extension` dùng tìm kiếm DLL của OS (trên
///   Windows có kèm thư mục exe).
pub fn vec0_candidate_paths(exe_dir: Option<&std::path::Path>) -> Vec<String> {
    let ext = if cfg!(target_os = "windows") {
        ".dll"
    } else if cfg!(target_os = "macos") {
        ".dylib"
    } else {
        ".so"
    };
    let platform_dirs: &[&str] = if cfg!(target_os = "windows") {
        &["sqlite-vec-windows-x64", "sqlite-vec-windows-arm64"]
    } else if cfg!(target_os = "macos") {
        &["sqlite-vec-darwin-x64", "sqlite-vec-darwin-arm64"]
    } else {
        &["sqlite-vec-linux-x64", "sqlite-vec-linux-arm64"]
    };

    let dev_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")));
    let mut candidates = Vec::new();
    for dir in platform_dirs {
        candidates.push(
            dev_root
                .join("node_modules")
                .join(dir)
                .join(format!("vec0{ext}"))
                .to_string_lossy()
                .into_owned(),
        );
    }
    // đóng gói: cạnh exe + resources/ (Tauri bundle) — không phụ thuộc cwd
    if let Some(dir) = exe_dir {
        let s = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
        candidates.push(s(dir.join(format!("vec0{ext}"))));
        candidates.push(s(dir.join("resources").join(format!("vec0{ext}"))));
    }
    candidates
}

fn vec0_trust_candidates(
    exe_dir: Option<&std::path::Path>,
) -> Vec<(std::path::PathBuf, std::path::PathBuf)> {
    let paths = vec0_candidate_paths(exe_dir);
    let dev_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")))
        .to_path_buf();
    paths
        .into_iter()
        .map(std::path::PathBuf::from)
        .map(|path| {
            if path.starts_with(&dev_root) {
                let relative = path
                    .strip_prefix(&dev_root)
                    .expect("prefix checked")
                    .to_path_buf();
                (dev_root.clone(), relative)
            } else {
                let root = exe_dir
                    .expect("non-dev vec0 candidate requires exe_dir")
                    .to_path_buf();
                let relative = path
                    .strip_prefix(&root)
                    .expect("packaged candidate is below exe_dir")
                    .to_path_buf();
                (root, relative)
            }
        })
        .collect()
}

pub fn load_sqlite_vec(conn: &Connection) -> Result<(), rusqlite::Error> {
    // Check if vec0 functions are already loaded
    if conn
        .query_row("SELECT vec_version()", [], |_| Ok(()))
        .is_ok()
    {
        return Ok(());
    }

    unsafe {
        conn.load_extension_enable()?;

        let exe_dir = std::env::current_exe().ok();
        let candidates = vec0_trust_candidates(exe_dir.as_deref().and_then(|p| p.parent()));
        let expected_hash = crate::embedded_runtime_artifact_hash("vec0").map_err(|error| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(error)))
        })?;

        let mut success = false;
        let mut last_err = None::<String>;

        for (root, relative) in &candidates {
            let path = match crate::verify_trusted_file(root, relative, &expected_hash) {
                Ok(path) => path,
                Err(error) => {
                    last_err = Some(error);
                    continue;
                }
            };
            match conn.load_extension(&path, None) {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => {
                    last_err = Some(e.to_string());
                }
            }
        }

        conn.load_extension_disable()?;

        if success {
            Ok(())
        } else {
            // Thông báo phải nói được cách khắc phục: khi thiếu vec0 thì lỗi kế
            // tiếp mà người dùng thấy là "no such module: vec0" ở tận lúc tạo
            // bảng `vec_idx` — hoàn toàn không gợi ý được nguyên nhân thật.
            let err_msg = format!(
                "khong nap duoc sqlite-vec (vec0). Da thu {n} duong dan: {tried}. \
                 Nguyen nhan thuong gap: chua chay `npm ci` o thu muc goc repo — \
                 vec0 do goi npm `sqlite-vec` cung cap. Loi cuoi cung: {last}",
                n = candidates.len(),
                tried = candidates
                    .iter()
                    .map(|(root, relative)| root.join(relative).display().to_string())
                    .collect::<Vec<_>>()
                    .join(", "),
                last = last_err.unwrap_or_else(|| "khong ro".to_string()),
            );
            Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(err_msg),
            )))
        }
    }
}

#[derive(Clone)]
pub struct DatabasePool {
    pub writer: Pool<CustomSqliteManager>,
    pub readers: Pool<CustomSqliteManager>,
}

impl DatabasePool {
    pub fn new<P: AsRef<Path>>(path: P) -> Result<Self, Box<dyn std::error::Error>> {
        let write_manager = SqliteConnectionManager::file(path.as_ref())
            .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_CREATE);
        let read_manager = SqliteConnectionManager::file(path.as_ref())
            .with_flags(OpenFlags::SQLITE_OPEN_READ_ONLY);

        let writer = Pool::builder().max_size(1).build(CustomSqliteManager {
            inner: Arc::new(write_manager),
            read_only: false,
        })?;

        let readers = Pool::builder().max_size(4).build(CustomSqliteManager {
            inner: Arc::new(read_manager),
            read_only: true,
        })?;

        let conn = writer.get()?;
        init_schemas(&conn)?;

        Ok(DatabasePool { writer, readers })
    }

    pub fn new_in_memory() -> Result<Self, Box<dyn std::error::Error>> {
        // Shared cache is required for readers and writers to share the memory database
        let rand_val = rand::random::<u64>();
        let db_uri = format!("file:memdb_{}?mode=memory&cache=shared", rand_val);
        let write_manager = SqliteConnectionManager::file(&db_uri).with_flags(
            OpenFlags::SQLITE_OPEN_READ_WRITE
                | OpenFlags::SQLITE_OPEN_CREATE
                | OpenFlags::SQLITE_OPEN_URI,
        );
        let read_manager = SqliteConnectionManager::file(&db_uri)
            .with_flags(OpenFlags::SQLITE_OPEN_READ_WRITE | OpenFlags::SQLITE_OPEN_URI);

        let writer = Pool::builder().max_size(1).build(CustomSqliteManager {
            inner: Arc::new(write_manager),
            read_only: false,
        })?;

        let readers = Pool::builder().max_size(4).build(CustomSqliteManager {
            inner: Arc::new(read_manager),
            read_only: true,
        })?;

        let conn = writer.get()?;
        init_schemas(&conn)?;

        Ok(DatabasePool { writer, readers })
    }
}

fn init_schemas(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch("
        CREATE TABLE IF NOT EXISTS facts (
            key TEXT PRIMARY KEY,
            value TEXT NOT NULL,
            createdAt TEXT NOT NULL,
            updatedAt TEXT NOT NULL,
            ttlDays INTEGER,
            source TEXT NOT NULL,
            category TEXT,
            importance REAL DEFAULT 0.5,
            confidenceScore REAL DEFAULT 1.0,
            sourceTurnId TEXT,
            memory_strength REAL DEFAULT 1.0,
            last_accessed_at INTEGER DEFAULT 0,
            access_count INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS agent_checkpoints (
            thread_id TEXT PRIMARY KEY,
            state_json TEXT NOT NULL
        );

        -- Sao lưu bản ghi facts KHÔNG giải mã được (locked) TRƯỚC khi set_fact
        -- ghi đè. Chống mất vĩnh viễn khi đổi khoá: một fact đang locked (đọc ra
        -- rỗng) mà consolidation/LLM ghi đè thì bản gốc mã hoá sẽ mất; ở đây giữ
        -- lại để khôi phục được khi có đúng khoá.
        CREATE TABLE IF NOT EXISTS facts_locked_backup (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            key TEXT NOT NULL,
            value TEXT NOT NULL,
            backed_up_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS events (
            eventId TEXT PRIMARY KEY,
            timestamp INTEGER NOT NULL,
            phi_facts TEXT,
            phi_entities TEXT,
            psi_sentiment TEXT,
            psi_intent TEXT,
            psi_relational TEXT,
            rawUserMsg TEXT,
            rawAiReply TEXT,
            consolidated INTEGER DEFAULT 0,
            domain TEXT DEFAULT 'General',
            category TEXT DEFAULT 'Uncategorized',
            trace_keywords TEXT,
            last_accessed_at INTEGER DEFAULT 0,
            consolidation_status TEXT DEFAULT 'pending',
            retry_count INTEGER DEFAULT 0,
            agentId TEXT DEFAULT 'liva_core'
        );

        CREATE INDEX IF NOT EXISTS idx_events_pending_ts ON events(timestamp, eventId) WHERE consolidation_status = 'pending';

        CREATE TABLE IF NOT EXISTS vector_dlq (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            delete_filter TEXT NOT NULL,
            status TEXT DEFAULT 'pending',
            retry_count INTEGER DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS turn_layer_nodes (
            turnId TEXT PRIMARY KEY,
            temporal_anchor INTEGER NOT NULL,
            userMsg TEXT,
            aiReply TEXT,
            createdAt TEXT NOT NULL,
            agentId TEXT DEFAULT 'liva_core'
        );
        CREATE INDEX IF NOT EXISTS idx_turns_temporal ON turn_layer_nodes(temporal_anchor);

        CREATE TABLE IF NOT EXISTS daily_briefings (
            id TEXT PRIMARY KEY,
            created_at INTEGER NOT NULL,
            topics TEXT NOT NULL,
            content TEXT NOT NULL,
            is_read INTEGER DEFAULT 0,
            source TEXT DEFAULT 'tavily',
            expires_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS tasks (
            id TEXT PRIMARY KEY,
            title TEXT NOT NULL,
            description TEXT DEFAULT '',
            status TEXT DEFAULT 'pending',
            priority TEXT DEFAULT 'medium',
            result TEXT DEFAULT '',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS consolidation_checkpoints (
            session_id TEXT PRIMARY KEY,
            last_step INTEGER DEFAULT 0,
            state_data TEXT DEFAULT '{}',
            created_at INTEGER NOT NULL,
            updated_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS dlq_consolidation (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            session_id TEXT NOT NULL,
            failed_step TEXT NOT NULL,
            error_msg TEXT,
            retry_count INTEGER DEFAULT 0,
            status TEXT DEFAULT 'pending',
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS personality_state (
            agentId TEXT PRIMARY KEY,
            valence REAL NOT NULL DEFAULT 0.5,
            arousal REAL NOT NULL DEFAULT 0.5,
            friendliness REAL NOT NULL DEFAULT 0.8,
            verbosity REAL NOT NULL DEFAULT 0.6,
            assertiveness REAL NOT NULL DEFAULT 0.5,
            updatedAt INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS vectors_meta (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vec_id TEXT UNIQUE NOT NULL,
            type TEXT NOT NULL,
            content TEXT NOT NULL,
            domain TEXT DEFAULT 'General',
            category TEXT DEFAULT 'Uncategorized',
            trace_keywords TEXT DEFAULT '[]',
            file_target TEXT,
            created_at INTEGER NOT NULL,
            last_accessed_at INTEGER DEFAULT 0,
            decay_weight REAL DEFAULT 1.0,
            access_count INTEGER DEFAULT 0,
            source_event_ids TEXT DEFAULT '[]'
        );
        CREATE INDEX IF NOT EXISTS idx_vectors_meta_type_domain_category ON vectors_meta (type, domain, category);
        CREATE INDEX IF NOT EXISTS idx_vectors_meta_created_at ON vectors_meta (created_at);

        CREATE VIRTUAL TABLE IF NOT EXISTS vectors_fts USING fts5(
            content,
            tokenize=\"unicode61 remove_diacritics 0\"
        );

        CREATE TABLE IF NOT EXISTS l3_nodes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            properties TEXT DEFAULT '{}'
        );

        CREATE TABLE IF NOT EXISTS l3_edges (
            source TEXT NOT NULL,
            target TEXT NOT NULL,
            relation TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            obsolete INTEGER DEFAULT 0,
            PRIMARY KEY (source, target, relation),
            FOREIGN KEY(source) REFERENCES l3_nodes(id),
            FOREIGN KEY(target) REFERENCES l3_nodes(id)
        );
    ")?;

    let count: i64 = conn.query_row(
        "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='vec_idx'",
        [],
        |row| row.get(0),
    )?;
    if count == 0 {
        conn.execute(
            &format!(
                "CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[{MEMORY_VECTOR_DIM}])"
            ),
            [],
        )?;
    }

    run_migrations(conn)?;
    ensure_foreign_key_integrity(conn)?;

    Ok(())
}

fn ensure_foreign_key_integrity(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut statement = conn.prepare("PRAGMA foreign_key_check")?;
    let mut rows = statement.query([])?;
    if let Some(row) = rows.next()? {
        let table: String = row.get(0)?;
        let row_id: Option<i64> = row.get(1)?;
        let parent: String = row.get(2)?;
        let message = format!(
            "foreign key violation: table={table}, rowid={}, parent={parent}",
            row_id
                .map(|value| value.to_string())
                .unwrap_or_else(|| "<without-rowid>".to_string())
        );
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT_FOREIGNKEY),
            Some(message),
        ));
    }
    Ok(())
}

/// Phiên bản schema hiện tại. Baseline (mọi bảng `CREATE ... IF NOT EXISTS` ở
/// trên) là **1**. Mỗi lần đổi schema về sau: tăng số này lên và thêm một mục
/// vào [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 7;

/// Các bước migration tuyến tính. Mỗi mục là `(phiên_bản_đích, sql)` và được
/// áp khi DB đang ở phiên bản < đích, theo thứ tự tăng dần, mỗi bước một
/// transaction. Baseline (phiên bản 1) do `init_schemas` dựng nên KHÔNG nằm ở
/// đây — danh sách này bắt đầu từ 1→2.
///
/// Ví dụ khi cần đổi schema:
///   (2, "ALTER TABLE facts ADD COLUMN source TEXT DEFAULT '';")
const MIGRATIONS: &[(i64, &str)] = &[
    (
        2,
        "UPDATE vectors_meta \
         SET domain = 'memory_owner:legacy_unowned' \
         WHERE domain = 'General' AND type = 'conversation_turn';",
    ),
    (
        3,
        "CREATE INDEX IF NOT EXISTS idx_events_pending_ts \
         ON events(timestamp, eventId) \
         WHERE consolidation_status = 'pending'; \
         DROP INDEX IF EXISTS idx_events_pending; \
         DROP INDEX IF EXISTS idx_events_consolidated_ts;",
    ),
    // Rung G2 — kho skill cục bộ. Xem
    // docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md §3 (G2).
    //
    // Ba bảng, tách vai rõ ràng:
    //
    // - `skills`      : danh tính + bản hiện hành. Khoá chính là `skill_id` đọc từ
    //                   file `.skill_id` trong thư mục skill, KHÔNG phải `name` hay
    //                   đường dẫn — để đổi tên thư mục hoặc đổi `name:` không làm
    //                   mất lịch sử và tín hiệu đã tích luỹ.
    // - `skill_versions` : DAG qua `parent_id`. Mỗi lần nội dung đổi là một version
    //                   mới trỏ về cha. `body_sha` cho phép nhận ra "không đổi gì"
    //                   mà không phải so cả thân bài.
    // - `skill_signals`: sổ ghi thô cho G3. G2 chỉ DỰNG BẢNG và cho phép ghi; việc
    //                   dùng tín hiệu làm prior khi xếp hạng là G3, không phải đây.
    //                   Các cột `actionability`/`evidence_status`/`failure_signature`/
    //                   `merge_key` lấy đúng taxonomy ở §2 để G3 không phải migrate lại.
    //
    // `ON DELETE CASCADE` có ý: xoá một skill thì lịch sử và tín hiệu của nó đi
    // theo. Không giữ bản ghi mồ côi trỏ vào skill_id không còn tồn tại.
    (
        4,
        "CREATE TABLE IF NOT EXISTS skills (
             skill_id           TEXT PRIMARY KEY,
             name               TEXT NOT NULL,
             description        TEXT NOT NULL DEFAULT '',
             dir_path           TEXT NOT NULL,
             current_version_id TEXT,
             updated_at         INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_versions (
             version_id TEXT PRIMARY KEY,
             skill_id   TEXT NOT NULL REFERENCES skills(skill_id) ON DELETE CASCADE,
             parent_id  TEXT REFERENCES skill_versions(version_id),
             body       TEXT NOT NULL,
             body_sha   TEXT NOT NULL,
             created_at INTEGER NOT NULL
         );
         CREATE TABLE IF NOT EXISTS skill_signals (
             signal_id         INTEGER PRIMARY KEY AUTOINCREMENT,
             skill_id          TEXT NOT NULL REFERENCES skills(skill_id) ON DELETE CASCADE,
             version_id        TEXT,
             kind              TEXT NOT NULL,
             actionability     TEXT,
             evidence_status   TEXT,
             failure_signature TEXT,
             merge_key         TEXT,
             detail            TEXT,
             created_at        INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_skill_versions_skill ON skill_versions(skill_id, created_at);
         CREATE INDEX IF NOT EXISTS idx_skill_versions_parent ON skill_versions(parent_id);
         CREATE INDEX IF NOT EXISTS idx_skill_signals_skill ON skill_signals(skill_id, created_at);
         CREATE INDEX IF NOT EXISTS idx_skill_signals_merge ON skill_signals(merge_key);",
    ),
    // Sổ danh bạ cho việc nhắn tin ra ngoài.
    //
    // `lookup_key` là tên đã bỏ dấu + thường hoá, do `messaging::contacts` sinh
    // ra — người nói "nhắn cho Minh Hiến", STT trả "minh hien", và cả hai phải
    // tìm ra cùng một người. Nó là UNIQUE **cùng với** `platform`: một người có
    // thể vừa có Telegram vừa có Messenger, nhưng không thể có hai Telegram, vì
    // khi đó "nhắn cho Hiến" thành câu không có câu trả lời đúng.
    //
    // `handle` cố ý là TEXT cho cả hai nền: Telegram cần chat id dạng số (i64,
    // có thể âm với group), Messenger cần thread id/URL. Ép kiểu số ở đây là tự
    // chặn nền thứ hai.
    //
    // KHÔNG có cột nào chứa mật khẩu/token của người dùng — danh bạ chỉ là tên
    // và địa chỉ đích. Đăng nhập là việc của trình duyệt, không phải của LIVA.
    (
        5,
        "CREATE TABLE IF NOT EXISTS contacts (
             contact_id  TEXT PRIMARY KEY,
             display_name TEXT NOT NULL,
             lookup_key  TEXT NOT NULL,
             platform    TEXT NOT NULL,
             handle      TEXT NOT NULL,
             note        TEXT NOT NULL DEFAULT '',
             created_at  INTEGER NOT NULL,
             updated_at  INTEGER NOT NULL
         );
         CREATE UNIQUE INDEX IF NOT EXISTS idx_contacts_lookup
             ON contacts(lookup_key, platform);
         CREATE INDEX IF NOT EXISTS idx_contacts_platform ON contacts(platform);",
    ),
    // Hộp xác nhận gửi tin phải sống qua restart nhưng không được ghi plaintext.
    //
    // `seq` là khóa tăng đơn điệu do SQLite quản lý, dùng làm tie-break khi
    // nhiều bản nháp được tạo trong cùng một giây. Nội dung nằm ở
    // `text_ciphertext`; khóa không nằm trong DB.
    (
        6,
        "CREATE TABLE IF NOT EXISTS message_outbox (
             seq             INTEGER PRIMARY KEY AUTOINCREMENT,
             draft_id        TEXT NOT NULL UNIQUE,
             platform        TEXT NOT NULL CHECK(platform IN ('telegram', 'messenger')),
             display_name    TEXT NOT NULL,
             handle          TEXT NOT NULL,
             text_ciphertext TEXT NOT NULL,
             created_at      INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_message_outbox_age
             ON message_outbox(created_at, seq);",
    ),
    // Audit tối thiểu cho quyền quên. Chỉ giữ hash của scope, request id và số
    // hàng; không giữ owner/conversation plaintext sau khi người dùng đã xóa.
    (
        7,
        "CREATE TABLE IF NOT EXISTS deletion_audit (
             audit_id    TEXT PRIMARY KEY,
             scope_hash  TEXT NOT NULL,
             dry_run     INTEGER NOT NULL,
             counts_json TEXT NOT NULL,
             created_at  INTEGER NOT NULL
         );
         CREATE INDEX IF NOT EXISTS idx_deletion_audit_created
             ON deletion_audit(created_at);",
    ),
];

/// Đưa schema từ phiên bản hiện tại của DB lên [`SCHEMA_VERSION`].
///
/// Vì sao cần (lộ trình 0.2): trước đây toàn bộ schema dựng bằng
/// `CREATE TABLE IF NOT EXISTS` — không phiên bản, không đường nâng cấp. Với
/// beta tester đã cài, một thay đổi cột là không có cách áp mà không mất dữ
/// liệu. `PRAGMA user_version` + khung này biến việc đó thành tuyến tính, có
/// thể tái lập, chạy trong transaction.
///
/// DB cũ (chưa từng đánh số) ở `user_version = 0` nhưng đã có đủ bảng baseline
/// nhờ `init_schemas` idempotent — nên chỉ cần **đóng dấu** lên 1, không chạy
/// SQL phá huỷ nào.
fn run_migrations(conn: &Connection) -> Result<(), rusqlite::Error> {
    let mut version: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;

    if version > SCHEMA_VERSION {
        // DB được tạo bởi bản LIVA mới hơn: không hạ cấp mù. Báo lỗi rõ thay vì
        // âm thầm chạy trên schema mình không hiểu.
        return Err(rusqlite::Error::SqliteFailure(
            rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_ERROR),
            Some(format!(
                "DB ở schema version {version} mới hơn bản LIVA này ({SCHEMA_VERSION}). \
                 Cập nhật LIVA hoặc dùng đúng bản đã tạo DB."
            )),
        ));
    }

    // 0 → 1: baseline đã do init_schemas dựng, chỉ đóng dấu.
    if version < 1 {
        conn.execute_batch("PRAGMA user_version = 1;")?;
        version = 1;
    }

    for &(target, sql) in MIGRATIONS {
        if version < target {
            let tx = conn.unchecked_transaction()?;
            tx.execute_batch(sql)?;
            tx.execute_batch(&format!("PRAGMA user_version = {target};"))?;
            tx.commit()?;
            version = target;
            tracing::info!("DB migration: đã nâng schema lên version {target}");
        }
    }

    Ok(())
}

// Structs representing tables and query parameters

#[derive(Debug, Serialize, Deserialize, Clone)]
#[allow(non_snake_case)]
pub struct Fact {
    pub key: String,
    pub value: String,
    pub createdAt: String,
    pub updatedAt: String,
    pub ttlDays: Option<i64>,
    pub source: String,
    pub category: Option<String>,
    pub importance: f64,
    pub confidenceScore: f64,
    pub sourceTurnId: Option<String>,
    pub memory_strength: f64,
    pub last_accessed_at: i64,
    pub access_count: i64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MetadataFilter {
    pub r#type: Option<String>,
    pub domain: Option<String>,
    pub category: Option<String>,
    pub created_after: Option<i64>,
    pub created_before: Option<i64>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VectorSearchResult {
    pub id: i64,
    pub vec_id: String,
    pub content: String,
    pub r#type: String,
    pub domain: String,
    pub category: String,
    pub distance: f64,
    pub score: f64,
    pub trace_keywords: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub created_at: i64,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FtsSearchResult {
    pub id: i64,
    pub vec_id: String,
    pub content: String,
    pub r#type: String,
    pub domain: String,
    pub category: String,
    pub trace_keywords: Vec<String>,
    pub source_event_ids: Vec<String>,
    pub created_at: i64,
}

// Logic implementations

fn build_metadata_conditions(filter: &MetadataFilter) -> (String, Vec<Value>) {
    let mut conditions = Vec::new();
    let mut params = Vec::new();

    if let Some(ref t) = filter.r#type {
        conditions.push("m.type = ?");
        params.push(Value::Text(t.clone()));
    }
    if let Some(ref d) = filter.domain {
        conditions.push("m.domain = ?");
        params.push(Value::Text(d.clone()));
    }
    if let Some(ref c) = filter.category {
        conditions.push("m.category = ?");
        params.push(Value::Text(c.clone()));
    }
    if let Some(ca) = filter.created_after {
        conditions.push("m.created_at >= ?");
        params.push(Value::Integer(ca));
    }
    if let Some(cb) = filter.created_before {
        conditions.push("m.created_at <= ?");
        params.push(Value::Integer(cb));
    }

    let where_clause = if conditions.is_empty() {
        "1=1".to_string()
    } else {
        conditions.join(" AND ")
    };

    (where_clause, params)
}

pub fn set_fact(
    conn: &Connection,
    engine: &EncryptionEngine,
    fact: &Fact,
) -> Result<(), rusqlite::Error> {
    use rusqlite::OptionalExtension;

    let encrypted_val = match engine.encrypt(&fact.value) {
        Ok(v) => v,
        Err(e) => {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::other(format!("Encryption failed: {}", e)),
            )));
        }
    };

    // BACKUP-BEFORE-OVERWRITE (fail-closed): nếu value ĐANG lưu KHÔNG giải mã
    // được bằng khoá hiện tại (locked — vd đổi khoá, hoặc rekey chưa kịp chạy),
    // đè nó đi sẽ MẤT bản gốc mã hoá VĨNH VIỄN. Đây chính là kịch bản
    // "consolidation/LLM học lại rồi set_fact đè bản gốc" mà UI-disable không
    // với tới (caller tự động). Sao lưu ciphertext cũ vào facts_locked_backup
    // TRƯỚC khi ghi, atomic trong 1 transaction. Chỉ đụng ca locked — ghi đè
    // value đọc-được là hành vi bình thường, không sao lưu.
    let tx = conn.unchecked_transaction()?;
    {
        let existing: Option<String> = tx
            .query_row("SELECT value FROM facts WHERE key = ?1", [&fact.key], |r| {
                r.get(0)
            })
            .optional()?;
        if let Some(old) = existing
            && engine.read_fact(&old).is_locked()
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs() as i64)
                .unwrap_or(0);
            tx.execute(
                "INSERT INTO facts_locked_backup (key, value, backed_up_at) VALUES (?1, ?2, ?3)",
                (&fact.key, &old, now),
            )?;
            tracing::warn!(
                "set_fact: value cũ của '{}' KHÔNG giải mã được bằng khoá hiện tại — \
                 đã sao lưu ciphertext vào facts_locked_backup trước khi ghi đè (không mất bản gốc)",
                fact.key
            );
        }

        tx.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId, memory_strength, last_accessed_at, access_count)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                updatedAt = excluded.updatedAt,
                ttlDays = excluded.ttlDays,
                source = excluded.source,
                category = excluded.category,
                importance = excluded.importance,
                confidenceScore = excluded.confidenceScore,
                sourceTurnId = excluded.sourceTurnId,
                memory_strength = excluded.memory_strength,
                last_accessed_at = excluded.last_accessed_at,
                access_count = excluded.access_count",
            (
                &fact.key,
                &encrypted_val,
                &fact.createdAt,
                &fact.updatedAt,
                &fact.ttlDays,
                &fact.source,
                &fact.category,
                fact.importance,
                fact.confidenceScore,
                &fact.sourceTurnId,
                fact.memory_strength,
                fact.last_accessed_at,
                fact.access_count,
            ),
        )?;
    }
    tx.commit()?;

    Ok(())
}

/// Mã hoá lại facts về khoá HIỆN TẠI (`live`), cứu được cả dữ liệu do khoá
/// khác ghi (`extra_decryptors` — vd khoá mặc định, hoặc `LIVA_ENCRYPTION_KEY_OLD`).
/// Trả `(số_rekey, số_không_giải_mã_được)`.
///
/// Đây là nền của việc BỎ KHOÁ MẶC ĐỊNH mà không mất dữ liệu: máy đã mã hoá
/// facts bằng `"0"×32` truyền `default_engine` vào `extra_decryptors`, boot đầu
/// tiên sau nâng cấp sẽ nâng chúng sang khoá thật tại chỗ.
///
/// **Tiêu chí idempotent CHÍ MẠNG:** chỉ bỏ qua khi `value` **đã v2 VÀ khoá
/// `live` giải mã được**. TUYỆT ĐỐI không dùng riêng `starts_with("v2:")` như
/// bản migrate cũ: ciphertext của khoá mặc định/cũ CŨNG mang tiền tố `v2:`
/// nhưng `live` không mở được — nếu bỏ qua theo tiền tố thì khi gỡ khoá mặc
/// định khỏi tập giải mã, số fact đó mất VĨNH VIỄN. Ở đây `live` không mở được
/// ⇒ không skip ⇒ thử `extra_decryptors` để cứu.
///
/// An toàn: chỉ đụng bản GIẢI MÃ được (không bao giờ mã hoá lại rác); UPDATE có
/// điều kiện `value = bản_gốc` để không đè mất bản mới do tiến trình khác ghi
/// xen (lost-update). Bản không khoá nào mở được → để NGUYÊN + đếm + WARN.
pub fn rekey_facts_encryption(
    conn: &Connection,
    live: &EncryptionEngine,
    extra_decryptors: &[&EncryptionEngine],
) -> Result<(usize, usize), rusqlite::Error> {
    use crate::crypto::DecryptError;

    let reencrypt = |plain: &str| -> Result<String, rusqlite::Error> {
        live.encrypt(plain).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(format!(
                "re-encrypt fail: {e}"
            ))))
        })
    };

    // Bước 1: quét + quyết định (KHÔNG UPDATE — stmt còn mượn conn). Giữ value
    // gốc để chống lost-update ở bước 2.
    let mut can_rekey: Vec<(String, String, String)> = Vec::new(); // (key, value_gốc, v2_mới)
    let mut khong_giai_ma = 0usize;
    {
        let mut stmt = conn.prepare("SELECT key, value FROM facts")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (key, value) = row?;
            match live.try_decrypt(&value) {
                Ok(plain) => {
                    // Đã ở khoá live rồi. Nếu đúng định dạng v2 → idempotent, bỏ
                    // qua. Nếu là v1 (do live giải được bằng legacy_key) → nâng
                    // định dạng lên v2 dưới chính live.
                    if value.starts_with("v2:") {
                        continue;
                    }
                    can_rekey.push((key, value, reencrypt(&plain)?));
                }
                Err(DecryptError::NotEncrypted) => { /* plaintext cũ — để nguyên */ }
                Err(_) => {
                    // live KHÔNG mở được (sai khoá / hỏng). Thử các khoá phụ để CỨU.
                    let recovered = extra_decryptors
                        .iter()
                        .find_map(|d| d.try_decrypt(&value).ok());
                    match recovered {
                        Some(plain) => can_rekey.push((key, value, reencrypt(&plain)?)),
                        None => {
                            khong_giai_ma += 1;
                            tracing::warn!(
                                "rekey_facts_encryption: bỏ qua fact '{key}' (không khoá nào giải mã được — hỏng hoặc mất khoá)"
                            );
                        }
                    }
                }
            }
        }
    } // stmt thả ở đây

    // Bước 2: UPDATE trong MỘT transaction, có ĐIỀU KIỆN `value = bản_gốc`. Nếu
    // giữa bước 1 và 2 có tiến trình khác ghi đè fact (vd set_fact từ gateway
    // thứ hai cùng DB), value đã đổi → khớp 0 dòng → BỎ QUA, không đè bản mới.
    let mut so_rekey = 0usize;
    if !can_rekey.is_empty() {
        let tx = conn.unchecked_transaction()?;
        {
            let mut up = tx.prepare("UPDATE facts SET value = ?1 WHERE key = ?2 AND value = ?3")?;
            for (key, value_goc, v2) in &can_rekey {
                let n = up.execute((v2, key, value_goc))?;
                if n == 0 {
                    tracing::warn!(
                        "rekey_facts_encryption: fact '{key}' đã bị đổi bởi tiến trình khác giữa chừng — bỏ qua để không ghi đè bản mới"
                    );
                } else {
                    so_rekey += n;
                }
            }
        }
        tx.commit()?;
        if so_rekey > 0 {
            tracing::info!(
                "rekey_facts_encryption: đã mã hoá lại {so_rekey} fact dưới khoá hiện tại (v2)"
            );
        }
    }

    Ok((so_rekey, khong_giai_ma))
}

/// Kết quả nâng cấp mã hóa cho dữ liệu hội thoại/checkpoint nhạy cảm.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersonalDataRekeyReport {
    /// Số checkpoint + conversation turn được mã hóa mới hoặc đổi sang khóa hiện tại.
    pub rekeyed: usize,
    /// Số bản ghi có vẻ là ciphertext nhưng không khóa nào mở được; bản gốc được giữ nguyên.
    pub locked: usize,
    /// Số bản ghi FTS plaintext của conversation turn đã bị xóa.
    pub fts_removed: usize,
}

/// Mã hóa dữ liệu cá nhân từng bị lưu plaintext và đổi ciphertext khóa cũ sang khóa hiện tại.
///
/// Phạm vi cố ý hẹp:
/// - `agent_checkpoints.state_json`;
/// - `vectors_meta.content` khi `type = 'conversation_turn'`;
/// - mọi projection FTS của conversation turn bị xóa vì FTS5 không hỗ trợ tìm kiếm trên
///   ciphertext. Dense vector vẫn dùng để chọn ứng viên, rồi content mới được giải mã.
///
/// UPDATE luôn so khớp cả khóa bản ghi và giá trị đã đọc để không ghi đè thay đổi đồng thời.
/// Ciphertext không khóa nào mở được được giữ nguyên và đếm `locked`, không mã hóa chồng.
pub fn rekey_personal_data_encryption(
    conn: &Connection,
    live: &EncryptionEngine,
    extra_decryptors: &[&EncryptionEngine],
) -> Result<PersonalDataRekeyReport, rusqlite::Error> {
    use crate::crypto::DecryptError;

    fn replacement(
        value: &str,
        live: &EncryptionEngine,
        extra_decryptors: &[&EncryptionEngine],
    ) -> Result<Option<String>, bool> {
        let plaintext = match live.try_decrypt(value) {
            Ok(_) if value.starts_with("v2:") => return Ok(None),
            Ok(plain) => plain,
            Err(DecryptError::NotEncrypted) => value.to_string(),
            Err(DecryptError::BadFormat) if !value.starts_with("v2:") => value.to_string(),
            Err(_) => match extra_decryptors
                .iter()
                .find_map(|decryptor| decryptor.try_decrypt(value).ok())
            {
                Some(plain) => plain,
                None => return Err(true),
            },
        };
        live.encrypt(&plaintext).map(Some).map_err(|_| false)
    }

    let encryption_error = || {
        rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
            "không mã hóa được dữ liệu cá nhân trong migration",
        )))
    };
    let mut checkpoint_updates = Vec::<(String, String, String)>::new();
    let mut conversation_updates = Vec::<(i64, String, String)>::new();
    let mut report = PersonalDataRekeyReport::default();

    {
        let mut stmt =
            conn.prepare("SELECT thread_id, state_json FROM agent_checkpoints ORDER BY thread_id")?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (thread_id, original) = row?;
            match replacement(&original, live, extra_decryptors) {
                Ok(Some(encrypted)) => {
                    checkpoint_updates.push((thread_id, original, encrypted));
                }
                Ok(None) => {}
                Err(true) => {
                    report.locked += 1;
                    tracing::warn!(
                        "rekey_personal_data_encryption: checkpoint '{thread_id}' bị khóa; giữ nguyên"
                    );
                }
                Err(false) => return Err(encryption_error()),
            }
        }
    }

    {
        let mut stmt = conn.prepare(
            "SELECT id, content FROM vectors_meta \
             WHERE type = 'conversation_turn' ORDER BY id",
        )?;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (id, original) = row?;
            match replacement(&original, live, extra_decryptors) {
                Ok(Some(encrypted)) => conversation_updates.push((id, original, encrypted)),
                Ok(None) => {}
                Err(true) => {
                    report.locked += 1;
                    tracing::warn!(
                        "rekey_personal_data_encryption: conversation vector rowid={id} bị khóa; giữ nguyên"
                    );
                }
                Err(false) => return Err(encryption_error()),
            }
        }
    }

    conn.execute_batch("PRAGMA secure_delete = ON;")?;
    let tx = conn.unchecked_transaction()?;
    {
        let mut update_checkpoint = tx.prepare(
            "UPDATE agent_checkpoints SET state_json = ?1 \
             WHERE thread_id = ?2 AND state_json = ?3",
        )?;
        for (thread_id, original, encrypted) in &checkpoint_updates {
            report.rekeyed += update_checkpoint.execute((encrypted, thread_id, original))?;
        }
    }
    {
        let mut update_conversation =
            tx.prepare("UPDATE vectors_meta SET content = ?1 WHERE id = ?2 AND content = ?3")?;
        for (id, original, encrypted) in &conversation_updates {
            report.rekeyed += update_conversation.execute((encrypted, id, original))?;
        }
    }
    report.fts_removed = tx.execute(
        "DELETE FROM vectors_fts \
         WHERE rowid IN (SELECT id FROM vectors_meta WHERE type = 'conversation_turn')",
        [],
    )?;
    tx.commit()?;

    if report.rekeyed > 0 || report.fts_removed > 0 {
        tracing::info!(
            "rekey_personal_data_encryption: rekeyed={}, fts_removed={}, locked={}",
            report.rekeyed,
            report.fts_removed,
            report.locked
        );
    }
    Ok(report)
}

/// Buộc SQLite loại các bản plaintext cũ còn có thể nằm trong page/WAL sau migration.
///
/// Chỉ gọi cho DB trên đĩa và chỉ sau khi đã cập nhật/xóa dữ liệu nhạy cảm. `VACUUM`
/// xây lại file DB; hai checkpoint `TRUNCATE` dọn WAL trước và sau quá trình đó.
pub fn purge_personal_data_plaintext_remnants(conn: &Connection) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "PRAGMA wal_checkpoint(TRUNCATE);
         VACUUM;
         PRAGMA wal_checkpoint(TRUNCATE);",
    )
}

/// Nâng cấp mã hoá facts v1 → v2 dưới CÙNG một khoá (không đổi khoá). Là trường
/// hợp riêng của [`rekey_facts_encryption`] với không có khoá phụ. Giữ tên cũ
/// cho các call-site boot + test không đổi.
pub fn migrate_facts_encryption(
    conn: &Connection,
    engine: &EncryptionEngine,
) -> Result<(usize, usize), rusqlite::Error> {
    rekey_facts_encryption(conn, engine, &[])
}

pub fn get_fact(
    conn: &Connection,
    engine: &EncryptionEngine,
    key: &str,
) -> Result<Option<Fact>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId, memory_strength, last_accessed_at, access_count
         FROM facts WHERE key = ?"
    )?;

    let mut rows = stmt.query([key])?;
    if let Some(row) = rows.next()? {
        let enc_value: String = row.get(1)?;
        let decrypted_value = engine.decrypt_read(&enc_value);

        Ok(Some(Fact {
            key: row.get(0)?,
            value: decrypted_value,
            createdAt: row.get(2)?,
            updatedAt: row.get(3)?,
            ttlDays: row.get(4)?,
            source: row.get(5)?,
            category: row.get(6)?,
            importance: row.get(7)?,
            confidenceScore: row.get(8)?,
            sourceTurnId: row.get(9)?,
            memory_strength: row.get(10)?,
            last_accessed_at: row.get(11)?,
            access_count: row.get(12)?,
        }))
    } else {
        Ok(None)
    }
}

/// Số chiều vector bộ nhớ, dùng chung cho schema `vec_idx` và mọi guard.
///
/// Phải khớp `llm::embedder::EMBEDDING_DIM`. Đổi hằng số này thì **index cũ
/// không dùng lại được** — vector sinh bởi model N chiều không so sánh được
/// với vector M chiều; phải xoá `vec_idx` và index lại toàn bộ.
pub const MEMORY_VECTOR_DIM: usize = 384;

/// Kiểm chiều vector trước khi chạm sqlite-vec.
///
/// sqlite-vec **có** báo lỗi khi lệch chiều (đã kiểm chứng: `"Dimension
/// mismatch ... Expected 384 dimensions but received 2048"`), nên đây không
/// phải để chống ghi sai lặng lẽ. Lý do có hàm này là **vị trí báo lỗi**:
/// không có nó, lỗi nổ ở tận câu SQL và thông báo không nói được nguồn vector
/// sai từ đâu ra.
fn check_vector_dim(vector: &[f32], what: &str) -> Result<(), rusqlite::Error> {
    if vector.len() == MEMORY_VECTOR_DIM {
        return Ok(());
    }
    Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
        std::io::Error::other(format!(
            "{what}: vector {} chieu nhung bo nho can dung {}. \
             Nguyen nhan thuong gap: dung embedding cua model chat \
             (llm::embed::get_embedding tra ve n_embd cua model dang nap, \
             vi du 2048 voi Qwen3-VL-2B) thay vi model embedding chuyen dung. \
             Hay dung llm::embedder::EmbeddingEngine.",
            vector.len(),
            MEMORY_VECTOR_DIM
        )),
    )))
}

// Chữ ký phẳng có chủ ý: đây là một câu SQL upsert với 6 cột metadata TUỲ
// CHỌN — gói vào struct chỉ thêm nghi lễ ở 3 call site (handle_command,
// persist_turn, test) mà không thêm an toàn kiểu nào (toàn Option cùng kiểu).
// Nếu số cột còn tăng thì lúc đó mới đáng dựng struct VectorMeta.
#[allow(clippy::too_many_arguments)]
pub fn upsert_vector(
    conn: &Connection,
    engine: &EncryptionEngine,
    vec_id: &str,
    r#type: &str,
    content: &str,
    vector: &[f32],
    domain: Option<&str>,
    category: Option<&str>,
    trace_keywords: Option<&[String]>,
    file_target: Option<&str>,
    source_event_ids: Option<&[String]>,
) -> Result<(), rusqlite::Error> {
    check_vector_dim(vector, "upsert_vector")?;
    let stored_content = if r#type == "conversation_turn" {
        engine.encrypt(content).map_err(|error| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(error)))
        })?
    } else {
        content.to_string()
    };
    let domain = domain.unwrap_or("General");
    let category = category.unwrap_or("Uncategorized");
    let trace_keywords_json =
        serde_json::to_string(trace_keywords.unwrap_or(&[])).unwrap_or_else(|_| "[]".to_string());
    let file_target = file_target.map(|s| s.to_string());

    let event_ids_list = source_event_ids.unwrap_or(&[]);
    let capped_event_ids = &event_ids_list[..event_ids_list.len().min(50)];
    let source_event_ids_json =
        serde_json::to_string(capped_event_ids).unwrap_or_else(|_| "[]".to_string());

    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    // 1. Insert or ignore into vectors_meta
    let changes = conn.execute(
        "INSERT OR IGNORE INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at, decay_weight, access_count)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0, 1.0, 0)",
        (
            vec_id,
            r#type,
            &stored_content,
            domain,
            category,
            &trace_keywords_json,
            &file_target,
            &source_event_ids_json,
            now,
        ),
    )?;

    // 2. Fetch the ID
    let row_id: i64 = conn.query_row(
        "SELECT id FROM vectors_meta WHERE vec_id = ?",
        [vec_id],
        |row| row.get(0),
    )?;

    // 3. If INSERT was ignored, force UPDATE
    if changes == 0 {
        conn.execute(
            "UPDATE vectors_meta SET type=?1, content=?2, domain=?3, category=?4, trace_keywords=?5, file_target=?6, source_event_ids=?7, last_accessed_at=?8, decay_weight=1.0, access_count=access_count+1
             WHERE id=?9",
            (
                r#type,
                &stored_content,
                domain,
                category,
                &trace_keywords_json,
                &file_target,
                &source_event_ids_json,
                now,
                row_id,
            ),
        )?;

        // Delete from vec_idx to ensure replacement
        conn.execute("DELETE FROM vec_idx WHERE rowid = ?", [row_id])?;
    }

    // 4. Insert into vec_idx
    let blob = bytemuck::cast_slice::<f32, u8>(vector);
    conn.execute(
        "INSERT INTO vec_idx (rowid, embedding) VALUES (?, vec_quantize_int8(?, 'unit'))",
        (row_id, blob),
    )?;

    // Conversation transcripts must not be duplicated as plaintext in FTS.
    // Their dense vector remains searchable; content is decrypted only after
    // candidate selection. Other memory types retain sparse retrieval.
    if r#type == "conversation_turn" {
        conn.execute("DELETE FROM vectors_fts WHERE rowid = ?", [row_id])?;
    } else {
        conn.execute(
            "INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES (?, ?)",
            (row_id, content),
        )?;
    }

    Ok(())
}

/// Ghi một lượt hội thoại vào event ledger và các chỉ mục truy hồi như một đơn vị atomic.
///
/// `event_id == vec_id` là khóa lineage cố định cho consolidation. Event chỉ giữ metadata
/// điều phối; nội dung plaintext đã nằm trong `vectors_meta` nên không nhân bản vào
/// `rawUserMsg`/`rawAiReply`.
pub(crate) fn persist_conversation_event_vector(
    conn: &Connection,
    engine: &EncryptionEngine,
    event_id: &str,
    content: &str,
    vector: &[f32],
    domain: &str,
    category: &str,
) -> Result<(), rusqlite::Error> {
    let transaction = conn.unchecked_transaction()?;
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    transaction.execute(
        "INSERT INTO events (
            eventId, timestamp, consolidated, domain, category,
            consolidation_status, retry_count, agentId
         ) VALUES (?1, ?2, 0, ?3, ?4, 'pending', 0, 'liva_core')",
        (event_id, now, domain, category),
    )?;

    let source_event_ids = [event_id.to_string()];
    upsert_vector(
        &transaction,
        engine,
        event_id,
        "conversation_turn",
        content,
        vector,
        Some(domain),
        Some(category),
        None,
        None,
        Some(&source_event_ids),
    )?;

    transaction.commit()
}

pub fn search_similar_vectors(
    conn: &Connection,
    engine: &EncryptionEngine,
    query_vector: &[f32],
    top_k: usize,
    filter: &MetadataFilter,
) -> Result<Vec<VectorSearchResult>, rusqlite::Error> {
    check_vector_dim(query_vector, "search_similar_vectors")?;
    let blob = bytemuck::cast_slice::<f32, u8>(query_vector);

    // As in JS: if there are filter conditions, fetch top_k * 3 to allow post-filtering.
    let has_filter =
        filter.r#type.is_some() || filter.domain.is_some() || filter.category.is_some();
    let fetch_k = if has_filter { top_k * 3 } else { top_k };

    let (meta_conditions, meta_params) = build_metadata_conditions(filter);

    let sql = format!(
        "SELECT v.rowid, v.distance, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids, m.decay_weight, m.created_at \
         FROM vec_idx v \
         INNER JOIN vectors_meta m ON m.id = v.rowid \
         WHERE v.embedding MATCH vec_quantize_int8(?, 'unit') \
           AND v.k = ? \
           AND v.rowid IN (SELECT id FROM vectors_meta m WHERE {})",
        meta_conditions
    );

    let mut stmt = conn.prepare(&sql)?;

    let mut params: Vec<Value> = vec![Value::Blob(blob.to_vec()), Value::Integer(fetch_k as i64)];
    params.extend(meta_params);

    let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let mut rows = stmt.query(&params_refs[..])?;
    let mut results = Vec::new();

    while let Some(row) = rows.next()? {
        let rowid: i64 = row.get(0)?;
        let distance: f64 = row.get(1)?;
        let vec_id: String = row.get(2)?;
        let stored_content: String = row.get(3)?;
        let r#type: String = row.get(4)?;
        let domain: String = row.get(5)?;
        let category: String = row.get(6)?;
        let trace_keywords_raw: String = row.get(7)?;
        let source_event_ids_raw: String = row.get(8)?;
        let decay_weight: f64 = row.get(9)?;
        let created_at: i64 = row.get(10)?;

        let content = if r#type == "conversation_turn" {
            match engine.read_fact(&stored_content) {
                crate::crypto::FactRead::Ok(plain) => plain,
                crate::crypto::FactRead::Locked { .. } => {
                    tracing::warn!(
                        vec_id = %vec_id,
                        "Bỏ qua conversation memory bị khóa; cần đúng LIVA_ENCRYPTION_KEY"
                    );
                    continue;
                }
            }
        } else {
            stored_content
        };
        let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
        let source_event_ids = serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

        // Calculate similarity matching JS:
        // similarity = Math.max(0, 1.0 - (distF32 * distF32) / 2.0) where distF32 = distance / 120.0
        let dist_f32 = distance / 120.0;
        let similarity = (1.0 - (dist_f32 * dist_f32) / 2.0).max(0.0);
        let score = similarity * decay_weight;

        results.push(VectorSearchResult {
            id: rowid,
            vec_id,
            content,
            r#type,
            domain,
            category,
            distance,
            score,
            trace_keywords,
            source_event_ids,
            created_at,
        });
    }

    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Truncate to top_k only if we fetched extra for post-filtering
    if results.len() > top_k {
        results.truncate(top_k);
    }

    Ok(results)
}

fn prepare_fts_query(query_text: &str) -> String {
    let escaped = query_text.replace('"', "\"\"");
    let terms: Vec<String> = escaped
        .split_whitespace()
        .map(|word| format!("\"{}\"*", word))
        .collect();
    terms.join(" AND ")
}

pub fn search_fts_vectors(
    conn: &Connection,
    query_text: &str,
    top_k: usize,
    filter: &MetadataFilter,
) -> Result<Vec<FtsSearchResult>, rusqlite::Error> {
    let clean_query = prepare_fts_query(query_text);
    let (meta_conditions, meta_params) = build_metadata_conditions(filter);

    let has_filter =
        filter.r#type.is_some() || filter.domain.is_some() || filter.category.is_some();
    let limit_k = if has_filter { top_k * 3 } else { top_k };

    let sql = format!(
        "SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids, m.created_at \
         FROM vectors_fts f \
         INNER JOIN vectors_meta m ON m.id = f.rowid \
         WHERE f.content MATCH ? AND m.type != 'conversation_turn' AND {} \
         LIMIT ?",
        meta_conditions
    );

    let mut stmt = conn.prepare(&sql)?;
    let mut params = vec![Value::Text(clean_query.clone())];
    params.extend(meta_params);
    params.push(Value::Integer(limit_k as i64));

    let params_refs: Vec<&dyn ToSql> = params.iter().map(|p| p as &dyn ToSql).collect();
    let rows_res = stmt.query(&params_refs[..]);

    let mut results = Vec::new();

    match rows_res {
        Ok(mut rows) => {
            while let Some(row) = rows.next()? {
                let rowid: i64 = row.get(0)?;
                let vec_id: String = row.get(1)?;
                let content: String = row.get(2)?;
                let r#type: String = row.get(3)?;
                let domain: String = row.get(4)?;
                let category: String = row.get(5)?;
                let trace_keywords_raw: String = row.get(6)?;
                let source_event_ids_raw: String = row.get(7)?;
                let created_at: i64 = row.get(8)?;

                let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
                let source_event_ids =
                    serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

                results.push(FtsSearchResult {
                    id: rowid,
                    vec_id,
                    content,
                    r#type,
                    domain,
                    category,
                    trace_keywords,
                    source_event_ids,
                    created_at,
                });
            }
        }
        Err(e) => {
            eprintln!(
                "Warning: FTS query {:?} failed: {:?}. Retrying raw query.",
                clean_query, e
            );
            let (meta_conds, meta_p) = build_metadata_conditions(filter);
            let fallback_sql = format!(
                "SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids, m.created_at \
                 FROM vectors_fts f \
                 INNER JOIN vectors_meta m ON m.id = f.rowid \
                 WHERE f.content MATCH ? AND m.type != 'conversation_turn' AND {} \
                 LIMIT ?",
                meta_conds
            );
            let mut fb_stmt = conn.prepare(&fallback_sql)?;
            let mut fb_params = vec![Value::Text(query_text.to_string())];
            fb_params.extend(meta_p);
            fb_params.push(Value::Integer(limit_k as i64));
            let fb_refs: Vec<&dyn ToSql> = fb_params.iter().map(|p| p as &dyn ToSql).collect();
            let mut fb_rows = fb_stmt.query(&fb_refs[..])?;
            while let Some(row) = fb_rows.next()? {
                let rowid: i64 = row.get(0)?;
                let vec_id: String = row.get(1)?;
                let content: String = row.get(2)?;
                let r#type: String = row.get(3)?;
                let domain: String = row.get(4)?;
                let category: String = row.get(5)?;
                let trace_keywords_raw: String = row.get(6)?;
                let source_event_ids_raw: String = row.get(7)?;
                let created_at: i64 = row.get(8)?;

                let trace_keywords = serde_json::from_str(&trace_keywords_raw).unwrap_or_default();
                let source_event_ids =
                    serde_json::from_str(&source_event_ids_raw).unwrap_or_default();

                results.push(FtsSearchResult {
                    id: rowid,
                    vec_id,
                    content,
                    r#type,
                    domain,
                    category,
                    trace_keywords,
                    source_event_ids,
                    created_at,
                });
            }
        }
    }

    if results.len() > top_k {
        results.truncate(top_k);
    }

    Ok(results)
}

#[allow(clippy::too_many_arguments)]
pub fn search_hybrid_vectors(
    conn: &Connection,
    engine: &EncryptionEngine,
    query_text: &str,
    query_vector: &[f32],
    top_k: usize,
    filter: &MetadataFilter,
    dense_weight: f64,
    sparse_weight: f64,
) -> Result<Vec<VectorSearchResult>, rusqlite::Error> {
    // In hybrid search, we want to fetch a larger pool for fusion, top_k * 3
    let fusion_limit = top_k * 3;
    let dense_results = search_similar_vectors(conn, engine, query_vector, fusion_limit, filter)?;
    let sparse_results = search_fts_vectors(conn, query_text, fusion_limit, filter)?;

    let mut results: Vec<VectorSearchResult> = Vec::new();
    const K: f64 = 60.0;

    // 1. Incorporate Dense Ranks
    for (index, item) in dense_results.into_iter().enumerate() {
        let rank = (index + 1) as f64;
        let score = dense_weight * (1.0 / (K + rank));
        results.push(VectorSearchResult { score, ..item });
    }

    // 2. Incorporate Sparse Ranks
    for (index, item) in sparse_results.into_iter().enumerate() {
        let rank = (index + 1) as f64;
        let score = sparse_weight * (1.0 / (K + rank));

        if let Some(existing) = results.iter_mut().find(|r| r.vec_id == item.vec_id) {
            existing.score += score;
        } else {
            results.push(VectorSearchResult {
                id: item.id,
                vec_id: item.vec_id.clone(),
                content: item.content,
                r#type: item.r#type,
                domain: item.domain,
                category: item.category,
                distance: 999.0, // Sentinel value for FTS-only matches
                score,
                trace_keywords: item.trace_keywords,
                source_event_ids: item.source_event_ids,
                created_at: item.created_at,
            });
        }
    }

    // 3. Sort by aggregated score descending (stable sort)
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    results.truncate(top_k);

    Ok(results)
}

#[cfg(test)]
#[path = "db/tests.rs"]
mod db_tests;

#[cfg(test)]
#[path = "db/encryption_tests.rs"]
mod db_encryption_tests;
