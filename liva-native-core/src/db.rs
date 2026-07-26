#![allow(non_snake_case)]
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

        conn.execute_batch(
            "
            PRAGMA busy_timeout = 5000;
            PRAGMA cache_size = -8192;
            PRAGMA page_size = 32768;
            PRAGMA mmap_size = 268435456;
        ",
        )?;

        if self.read_only {
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

        Ok(conn)
    }

    fn is_valid(&self, conn: &mut Self::Connection) -> Result<(), Self::Error> {
        self.inner.is_valid(conn)
    }

    fn has_broken(&self, conn: &mut Self::Connection) -> bool {
        self.inner.has_broken(conn)
    }
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

    let mut candidates = Vec::new();
    // dev: node_modules quanh cwd
    for dir in platform_dirs {
        candidates.push(format!("node_modules/{dir}/vec0{ext}"));
        candidates.push(format!("../node_modules/{dir}/vec0{ext}"));
        candidates.push(format!("../../node_modules/{dir}/vec0{ext}"));
    }
    // đóng gói: cạnh exe + resources/ (Tauri bundle) — không phụ thuộc cwd
    if let Some(dir) = exe_dir {
        let s = |p: std::path::PathBuf| p.to_string_lossy().into_owned();
        candidates.push(s(dir.join(format!("vec0{ext}"))));
        candidates.push(s(dir.join("resources").join(format!("vec0{ext}"))));
        for d in platform_dirs {
            candidates.push(s(dir
                .join("resources")
                .join("node_modules")
                .join(d)
                .join(format!("vec0{ext}"))));
        }
    }
    // cwd + tìm kiếm hệ thống
    candidates.push(format!("vec0{ext}"));
    candidates.push("vec0".to_string());
    candidates
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
        let candidates = vec0_candidate_paths(exe_dir.as_deref().and_then(|p| p.parent()));

        let mut success = false;
        let mut last_err = None;

        for path in &candidates {
            match conn.load_extension(path, None) {
                Ok(_) => {
                    success = true;
                    break;
                }
                Err(e) => {
                    last_err = Some(e);
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
                tried = candidates.join(", "),
                last = last_err
                    .map(|e| e.to_string())
                    .unwrap_or_else(|| "khong ro".to_string()),
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

    Ok(())
}

/// Phiên bản schema hiện tại. Baseline (mọi bảng `CREATE ... IF NOT EXISTS` ở
/// trên) là **1**. Mỗi lần đổi schema về sau: tăng số này lên và thêm một mục
/// vào [`MIGRATIONS`].
pub const SCHEMA_VERSION: i64 = 4;

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
            content,
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
                content,
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

    // 5. Sync vectors_fts
    conn.execute(
        "INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES (?, ?)",
        (row_id, content),
    )?;

    Ok(())
}

/// Ghi một lượt hội thoại vào event ledger và các chỉ mục truy hồi như một đơn vị atomic.
///
/// `event_id == vec_id` là khóa lineage cố định cho consolidation. Event chỉ giữ metadata
/// điều phối; nội dung plaintext đã nằm trong `vectors_meta` nên không nhân bản vào
/// `rawUserMsg`/`rawAiReply`.
pub(crate) fn persist_conversation_event_vector(
    conn: &Connection,
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
        let content: String = row.get(3)?;
        let r#type: String = row.get(4)?;
        let domain: String = row.get(5)?;
        let category: String = row.get(6)?;
        let trace_keywords_raw: String = row.get(7)?;
        let source_event_ids_raw: String = row.get(8)?;
        let decay_weight: f64 = row.get(9)?;
        let created_at: i64 = row.get(10)?;

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
         WHERE f.content MATCH ? AND {} \
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
                 WHERE f.content MATCH ? AND {} \
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

pub fn search_hybrid_vectors(
    conn: &Connection,
    query_text: &str,
    query_vector: &[f32],
    top_k: usize,
    filter: &MetadataFilter,
    dense_weight: f64,
    sparse_weight: f64,
) -> Result<Vec<VectorSearchResult>, rusqlite::Error> {
    // In hybrid search, we want to fetch a larger pool for fusion, top_k * 3
    let fusion_limit = top_k * 3;
    let dense_results = search_similar_vectors(conn, query_vector, fusion_limit, filter)?;
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
mod tests {
    use super::*;

    /// H5: vec0 candidates phải phủ CẢ đóng gói (cạnh exe + resources/) chứ không
    /// chỉ dev (node_modules quanh cwd) — nếu không app Tauri cài đặt sẽ sập DB.
    #[test]
    fn vec0_candidates_gom_dev_dong_goi_va_he_thong() {
        let exe_dir = std::path::Path::new(if cfg!(windows) {
            r"C:\App\bin"
        } else {
            "/app/bin"
        });
        let ext = if cfg!(target_os = "windows") {
            ".dll"
        } else if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".so"
        };
        let vec_name = format!("vec0{ext}");
        let c = vec0_candidate_paths(Some(exe_dir));

        assert!(
            c.iter()
                .any(|p| p.contains("node_modules") && p.ends_with(&vec_name)),
            "phải có candidate node_modules (dev)"
        );
        assert!(
            c.iter().any(
                |p| std::path::Path::new(p).parent() == Some(exe_dir) && p.ends_with(&vec_name)
            ),
            "phải có candidate cạnh executable (đóng gói)"
        );
        assert!(
            c.iter()
                .any(|p| p.contains("resources") && p.ends_with(&vec_name)),
            "phải có candidate trong resources/ (Tauri bundle)"
        );
        assert!(
            c.contains(&"vec0".to_string()) && c.contains(&vec_name),
            "phải có candidate trần cho tìm kiếm hệ thống"
        );

        // Không có exe_dir (không xác định được) → ít candidate hơn, không có resources.
        let c_none = vec0_candidate_paths(None);
        assert!(c_none.len() < c.len(), "thiếu exe_dir thì ít candidate hơn");
        assert!(!c_none.iter().any(|p| p.contains("resources")));
    }
    use crate::crypto::EncryptionEngine;

    #[test]
    fn pending_consolidation_query_uses_ordered_partial_index() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");
        let conn = pool.writer.get().expect("get writer");
        let mut statement = conn
            .prepare(
                "EXPLAIN QUERY PLAN \
                 SELECT eventId FROM events \
                 WHERE consolidation_status = 'pending' \
                 ORDER BY timestamp, eventId LIMIT 20",
            )
            .expect("prepare query plan");
        let details = statement
            .query_map([], |row| row.get::<_, String>(3))
            .expect("query plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect query plan");

        assert!(
            details
                .iter()
                .any(|detail| detail.contains("idx_events_pending_ts")),
            "ordered partial index must serve the consolidation queue: {details:?}",
        );
        assert!(
            details
                .iter()
                .all(|detail| !detail.contains("USE TEMP B-TREE")),
            "consolidation query must not sort into a temporary B-tree: {details:?}",
        );
    }

    /// Lộ trình 0.2: DB mới phải được đóng dấu SCHEMA_VERSION, và mở lại một DB
    /// cũ (user_version=0 nhưng đủ bảng) phải nâng lên mà KHÔNG mất dữ liệu.
    #[test]
    fn migration_dong_dau_va_khong_mat_du_lieu() {
        use rusqlite::Connection;
        let path = "temp_test_migration.sqlite";
        let _ = std::fs::remove_file(path);

        // Lần tạo đầu: schema mới phải ở đúng SCHEMA_VERSION.
        {
            let pool = DatabasePool::new(path).expect("tao db");
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "db moi phai duoc dong dau");
            conn.execute(
                "INSERT INTO facts (key, value, createdAt, updatedAt, source) \
                 VALUES ('ten', 'Bun', '2026-07-22', '2026-07-22', 'test')",
                [],
            )
            .unwrap();
        }

        // Giả lập DB "cũ": hạ user_version về 0 như thể được tạo trước khi có
        // đánh số. Dữ liệu vẫn còn.
        {
            let c = Connection::open(path).unwrap();
            c.execute_batch("PRAGMA user_version = 0;").unwrap();
        }

        // Mở lại: phải nâng lên SCHEMA_VERSION, dữ liệu cũ còn nguyên.
        {
            let pool = DatabasePool::new(path).expect("mo lai db cu");
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "db cu phai duoc nang len");
            let val: String = conn
                .query_row("SELECT value FROM facts WHERE key='ten'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(val, "Bun", "du lieu cu KHONG duoc mat khi migrate");
        }

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn migration_cach_ly_conversation_turn_legacy_khong_owner() {
        let path = std::env::temp_dir().join(format!(
            "liva_legacy_unowned_migration_{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        {
            let pool = DatabasePool::new(&path).expect("tao legacy db");
            let conn = pool.writer.get().unwrap();
            for (vec_id, memory_type, domain) in [
                ("legacy-turn", "conversation_turn", "General"),
                ("semantic-general", "semantic_fact", "General"),
                (
                    "already-scoped",
                    "conversation_turn",
                    "memory_owner:telegram:100",
                ),
            ] {
                conn.execute(
                    "INSERT INTO vectors_meta \
                     (vec_id, type, content, domain, category, created_at) \
                     VALUES (?1, ?2, ?3, ?4, 'legacy', 1)",
                    rusqlite::params![vec_id, memory_type, vec_id, domain],
                )
                .unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
        }

        {
            let pool = DatabasePool::new(&path).expect("migrate legacy db");
            let conn = pool.writer.get().unwrap();
            let domain_of = |vec_id: &str| -> String {
                conn.query_row(
                    "SELECT domain FROM vectors_meta WHERE vec_id = ?1",
                    [vec_id],
                    |row| row.get(0),
                )
                .unwrap()
            };

            assert_eq!(domain_of("legacy-turn"), "memory_owner:legacy_unowned");
            assert_eq!(domain_of("semantic-general"), "General");
            assert_eq!(domain_of("already-scoped"), "memory_owner:telegram:100");
        }

        let _ = std::fs::remove_file(path);
    }

    /// Bổ sung khuyến nghị review #3: migration là MỘT LẦN (idempotent qua
    /// `user_version`). Sau khi đã ở v2, chèn một hàng `General` conversation_turn
    /// rồi mở lại DB — migration KHÔNG chạy lại nên hàng đó KHÔNG bị backfill.
    #[test]
    fn migration_idempotent_khong_chay_lai_sau_v2() {
        let path = std::env::temp_dir().join(format!(
            "liva_idem_migration_{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        // Lần 1: tạo + migrate lên SCHEMA_VERSION.
        {
            let pool = DatabasePool::new(&path).expect("tao db");
            let v: i64 = pool
                .writer
                .get()
                .unwrap()
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "lần đầu phải migrate lên SCHEMA_VERSION");
        }

        // Chèn một 'General' conversation_turn SAU khi DB đã ở v2.
        {
            let pool = DatabasePool::new(&path).unwrap();
            pool.writer
                .get()
                .unwrap()
                .execute(
                    "INSERT INTO vectors_meta (vec_id, type, content, domain, category, created_at) \
                     VALUES ('late', 'conversation_turn', 'x', 'General', 'c', 1)",
                    [],
                )
                .unwrap();
        }

        // Mở lại: migration KHÔNG chạy lại (đã v2) → 'late' vẫn 'General'.
        {
            let pool = DatabasePool::new(&path).unwrap();
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "mở lại không được đổi version");
            let domain: String = conn
                .query_row(
                    "SELECT domain FROM vectors_meta WHERE vec_id='late'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                domain, "General",
                "migration đã chạy 1 lần → mở lại KHÔNG backfill hàng mới (idempotent)"
            );
        }

        let _ = std::fs::remove_file(path);
    }

    /// DB do bản LIVA mới hơn tạo (version tương lai) phải bị TỪ CHỐI rõ ràng,
    /// không âm thầm chạy trên schema mình không hiểu.
    #[test]
    fn tu_choi_db_tu_tuong_lai() {
        use rusqlite::Connection;
        let path = "temp_test_future.sqlite";
        let _ = std::fs::remove_file(path);
        {
            let _ = DatabasePool::new(path).expect("tao db");
            let c = Connection::open(path).unwrap();
            c.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 5))
                .unwrap();
        }
        let res = DatabasePool::new(path);
        assert!(res.is_err(), "db tu tuong lai phai bi tu choi");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_database_pooling_and_wal() {
        let db_path = "temp_test_pooling.sqlite";
        let _ = std::fs::remove_file(db_path);

        let pool = DatabasePool::new(db_path).unwrap();
        let writer_conn = pool.writer.get().unwrap();
        let reader_conn = pool.readers.get().unwrap();

        // Check WAL mode
        let journal_mode: String = writer_conn
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .unwrap();
        assert_eq!(journal_mode.to_uppercase(), "WAL");

        // Check synchronous setting
        let synchronous: i64 = writer_conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(synchronous, 1); // 1 = NORMAL

        // Check readers also have normal sync
        let reader_sync: i64 = reader_conn
            .query_row("PRAGMA synchronous", [], |row| row.get(0))
            .unwrap();
        assert_eq!(reader_sync, 1);

        drop(pool);
        let _ = std::fs::remove_file(db_path);
    }

    #[test]
    fn test_facts_crud_and_encryption() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let conn = pool.writer.get().unwrap();

        let fact = Fact {
            key: "user_name".to_string(),
            value: "Alice".to_string(),
            createdAt: "2026-06-25T00:00:00Z".to_string(),
            updatedAt: "2026-06-25T00:00:00Z".to_string(),
            ttlDays: Some(30),
            source: "user_input".to_string(),
            category: Some("profile".to_string()),
            importance: 0.8,
            confidenceScore: 0.95,
            sourceTurnId: Some("turn_1".to_string()),
            memory_strength: 1.0,
            last_accessed_at: 0,
            access_count: 0,
        };

        // Save fact
        set_fact(&conn, &engine, &fact).unwrap();

        // Query directly to check that it is encrypted
        let raw_val: String = conn
            .query_row(
                "SELECT value FROM facts WHERE key = 'user_name'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(raw_val, "Alice");
        // set_fact nay mã hoá định dạng v2: "v2:salt:iv:tag:cipher" (5 phần).
        assert!(
            raw_val.starts_with("v2:"),
            "value trên đĩa phải là ciphertext v2"
        );
        assert_eq!(raw_val.split(':').count(), 5);

        // Retrieve using get_fact, should be decrypted
        let retrieved = get_fact(&conn, &engine, "user_name").unwrap().unwrap();
        assert_eq!(retrieved.value, "Alice");
        assert_eq!(retrieved.source, "user_input");
        assert_eq!(retrieved.importance, 0.8);
    }

    #[test]
    fn conversation_turn_tao_ledger_va_vector_cung_lineage_scope() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let vector = vec![0.25; MEMORY_VECTOR_DIM];
        let turn_id = "turn_ledger_1";
        let domain = "memory_owner:telegram:100";
        let category = "conversation:telegram_chat:-200";
        let content = "Người dùng: nhớ mã ORION-7\nLIVA: Tôi đã ghi nhớ.";

        persist_conversation_event_vector(&conn, turn_id, content, &vector, domain, category)
            .unwrap();

        let event: (String, String, String, String, bool, bool) = conn
            .query_row(
                "SELECT eventId, consolidation_status, domain, category, \
                        rawUserMsg IS NULL, rawAiReply IS NULL \
                 FROM events WHERE eventId = ?1",
                [turn_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        let memory: (String, String, String, String, String) = conn
            .query_row(
                "SELECT vec_id, content, domain, category, source_event_ids \
                 FROM vectors_meta WHERE vec_id = ?1",
                [turn_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(
            (event, memory),
            (
                (
                    turn_id.to_string(),
                    "pending".to_string(),
                    domain.to_string(),
                    category.to_string(),
                    true,
                    true,
                ),
                (
                    turn_id.to_string(),
                    content.to_string(),
                    domain.to_string(),
                    category.to_string(),
                    r#"["turn_ledger_1"]"#.to_string(),
                ),
            )
        );
    }

    #[test]
    fn conversation_turn_rollback_toan_bo_khi_vector_khong_ghi_duoc() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let invalid_vector = vec![0.25; MEMORY_VECTOR_DIM - 1];

        let result = persist_conversation_event_vector(
            &conn,
            "turn_atomic_failure",
            "Người dùng: dữ liệu không được ghi nửa vời\nLIVA: đã rõ.",
            &invalid_vector,
            "memory_owner:local",
            "conversation:default",
        );
        assert!(
            result.is_err(),
            "vector sai chiều phải làm lượt ghi thất bại"
        );

        let counts = (
            conn.query_row("SELECT count(*) FROM events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vectors_meta", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vectors_fts", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vec_idx", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        );
        assert_eq!(
            counts,
            (0, 0, 0, 0),
            "event, metadata, FTS và vec0 phải rollback cùng nhau"
        );
    }

    #[test]
    fn test_vector_and_fts_and_hybrid_search() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();

        // 1. Insert vector meta and vectors
        let vec1 = vec![1.0; 384];
        let vec2 = vec![-1.0; 384];

        upsert_vector(
            &conn,
            "v1",
            "fact",
            "the quick brown fox jumps over the lazy dog",
            &vec1,
            Some("test_domain"),
            Some("test_category"),
            Some(&["fox".to_string()]),
            None,
            None,
        )
        .unwrap();

        upsert_vector(
            &conn,
            "v2",
            "fact",
            "the quiet white cat sleeps on the red rug",
            &vec2,
            Some("test_domain"),
            Some("test_category"),
            Some(&["cat".to_string()]),
            None,
            None,
        )
        .unwrap();

        let filter = MetadataFilter {
            r#type: None,
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };

        // 2. Test FTS Search
        let fts_results = search_fts_vectors(&conn, "fox", 5, &filter).unwrap();
        assert_eq!(fts_results.len(), 1);
        assert_eq!(fts_results[0].vec_id, "v1");

        // 3. Test Vector Search
        let vector_results = search_similar_vectors(&conn, &vec1, 5, &filter).unwrap();
        assert_eq!(vector_results.len(), 2);
        assert_eq!(vector_results[0].vec_id, "v1");
        assert!(vector_results[0].score > vector_results[1].score);

        // 4. Test Hybrid Search
        let hybrid_results =
            search_hybrid_vectors(&conn, "white cat", &vec2, 5, &filter, 1.0, 1.0).unwrap();
        assert_eq!(hybrid_results.len(), 2);
        // "white cat" and vec2 are closest to v2
        assert_eq!(hybrid_results[0].vec_id, "v2");

        // 5. Test FTS and Hybrid search with non-empty MetadataFilter
        let filter_non_empty = MetadataFilter {
            r#type: Some("fact".to_string()),
            domain: Some("test_domain".to_string()),
            category: Some("test_category".to_string()),
            created_after: None,
            created_before: None,
        };

        let fts_filtered = search_fts_vectors(&conn, "fox", 5, &filter_non_empty).unwrap();
        assert_eq!(fts_filtered.len(), 1);
        assert_eq!(fts_filtered[0].vec_id, "v1");

        let hybrid_filtered =
            search_hybrid_vectors(&conn, "white cat", &vec2, 5, &filter_non_empty, 1.0, 1.0)
                .unwrap();
        assert_eq!(hybrid_filtered.len(), 2);
        assert_eq!(hybrid_filtered[0].vec_id, "v2");

        // Test with mismatching filter to verify it filters out results
        let filter_mismatch = MetadataFilter {
            r#type: Some("not_matching".to_string()),
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };
        let fts_mismatched = search_fts_vectors(&conn, "fox", 5, &filter_mismatch).unwrap();
        assert_eq!(fts_mismatched.len(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_wal_concurrency_stress() {
        use std::sync::Arc;

        let db_path = "temp_test_concurrency.sqlite";
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));

        let pool = Arc::new(DatabasePool::new(db_path).unwrap());
        let engine = Arc::new(EncryptionEngine::new("00000000000000000000000000000000"));

        // Pre-populate some facts to read
        {
            let conn = pool.writer.get().unwrap();
            for i in 0..50 {
                let fact = Fact {
                    key: format!("key_{}", i),
                    value: format!("value_{}", i),
                    createdAt: "2026-06-25T00:00:00Z".to_string(),
                    updatedAt: "2026-06-25T00:00:00Z".to_string(),
                    ttlDays: Some(30),
                    source: "stress_test".to_string(),
                    category: Some("test".to_string()),
                    importance: 0.5,
                    confidenceScore: 1.0,
                    sourceTurnId: None,
                    memory_strength: 1.0,
                    last_accessed_at: 0,
                    access_count: 0,
                };
                set_fact(&conn, &engine, &fact).unwrap();
            }
        }

        // Spawn 100 concurrent reads and 10 writes
        let mut handles = vec![];

        // 10 concurrent writes
        for w in 0..10 {
            let pool_clone = pool.clone();
            let engine_clone = engine.clone();
            let handle = tokio::spawn(async move {
                for i in 0..5 {
                    let fact = Fact {
                        key: format!("write_{}_{}", w, i),
                        value: format!("new_value_{}_{}", w, i),
                        createdAt: "2026-06-25T00:00:00Z".to_string(),
                        updatedAt: "2026-06-25T00:00:00Z".to_string(),
                        ttlDays: Some(30),
                        source: "stress_test".to_string(),
                        category: Some("test".to_string()),
                        importance: 0.5,
                        confidenceScore: 1.0,
                        sourceTurnId: None,
                        memory_strength: 1.0,
                        last_accessed_at: 0,
                        access_count: 0,
                    };
                    let pool_c = pool_clone.clone();
                    let engine_c = engine_clone.clone();
                    let fact_c = fact.clone();
                    tokio::task::spawn_blocking(move || {
                        let conn = pool_c.writer.get().expect("Failed to get write connection");
                        set_fact(&conn, &engine_c, &fact_c).expect("Failed to set fact");
                    })
                    .await
                    .expect("spawn_blocking failed");
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                }
            });
            handles.push(handle);
        }

        // 100 concurrent reads
        for r in 0..100 {
            let pool_clone = pool.clone();
            let engine_clone = engine.clone();
            let handle = tokio::spawn(async move {
                for i in 0..10 {
                    let key = format!("key_{}", (r + i) % 50);
                    let pool_c = pool_clone.clone();
                    let engine_c = engine_clone.clone();
                    let fact = tokio::task::spawn_blocking(move || {
                        let conn = pool_c.readers.get().expect("Failed to get read connection");
                        get_fact(&conn, &engine_c, &key).expect("Failed to get fact")
                    })
                    .await
                    .expect("spawn_blocking failed");
                    assert!(fact.is_some());
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        // Verify database contents
        {
            let conn = pool.writer.get().unwrap();
            let retrieved = get_fact(&conn, &engine, "write_9_4").unwrap().unwrap();
            assert_eq!(retrieved.value, "new_value_9_4");
        }

        // Clean up
        drop(pool);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }

    /// Guard chiều vector: dùng embedding của model chat (2048 chiều với
    /// Qwen3-VL-2B) thay vì model embedding chuyên dụng phải bị chặn NGAY tại
    /// hàm gọi, kèm thông báo chỉ ra nguyên nhân — không để lỗi nổ ở tận câu
    /// SQL với thông báo không nói được vector sai từ đâu ra.
    #[test]
    fn guard_chan_vector_sai_chieu_va_chi_ra_nguyen_nhan() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let filter = MetadataFilter {
            r#type: None,
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };

        // n_embd cua Qwen3-VL-2B
        let sai = vec![0.01f32; 2048];
        let e = upsert_vector(
            &conn, "v1", "fact", "noi dung", &sai, None, None, None, None, None,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("2048"), "phai neu so chieu that: {e}");
        assert!(e.contains("384"), "phai neu so chieu can: {e}");
        assert!(
            e.contains("EmbeddingEngine"),
            "phai chi ra cach dung dung: {e}"
        );

        let e2 = search_similar_vectors(&conn, &sai, 5, &filter)
            .unwrap_err()
            .to_string();
        assert!(
            e2.contains("search_similar_vectors"),
            "phai neu ten ham: {e2}"
        );

        // search_hybrid_vectors di qua search_similar_vectors nen cung bi chan
        assert!(search_hybrid_vectors(&conn, "q", &sai, 5, &filter, 1.0, 1.0).is_err());

        // Vector rong va vector thieu 1 chieu deu phai bi chan
        assert!(
            upsert_vector(&conn, "v2", "fact", "x", &[], None, None, None, None, None).is_err()
        );
        let thieu = vec![0.01f32; MEMORY_VECTOR_DIM - 1];
        assert!(
            upsert_vector(
                &conn, "v3", "fact", "x", &thieu, None, None, None, None, None
            )
            .is_err()
        );

        // Dung chieu thi qua
        let dung = vec![0.01f32; MEMORY_VECTOR_DIM];
        assert!(
            upsert_vector(
                &conn, "v4", "fact", "x", &dung, None, None, None, None, None
            )
            .is_ok()
        );
    }
}

#[cfg(test)]
mod encryption_migration_tests {
    use super::*;
    use crate::crypto::EncryptionEngine;

    /// Dựng một fact v1 (định dạng cũ) thật, chạy migration, kiểm: (1) nội dung
    /// giữ nguyên, (2) trên đĩa nay là v2, (3) idempotent.
    #[test]
    fn migrate_v1_len_v2_khong_mat_du_lieu() {
        use aes_gcm::aead::consts::U16;
        use aes_gcm::{
            AesGcm, Nonce,
            aead::{Aead, KeyInit},
        };
        type G = AesGcm<aes_gcm::aes::Aes256, U16>;

        let key_str = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key_str);
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        let mut raw = [0u8; 32];
        let kb = key_str.as_bytes();
        raw[..kb.len().min(32)].copy_from_slice(&kb[..kb.len().min(32)]);
        let iv = [3u8; 16];
        let ct = G::new_from_slice(&raw)
            .unwrap()
            .encrypt(Nonce::<U16>::from_slice(&iv), b"meo ten Bun".as_ref())
            .unwrap();
        let (c, tag) = ct.split_at(ct.len() - 16);
        let v1 = format!(
            "{}:{}:{}",
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(c)
        );
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd', 'd', 't')",
            [&v1],
        ).unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('p', 'plaintext-cu', 'd', 'd', 't')",
            [],
        ).unwrap();

        let (nang, khong) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang, 1);
        assert_eq!(khong, 0);

        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(on_disk.starts_with("v2:"));
        assert_eq!(
            get_fact(&conn, &engine, "k").unwrap().unwrap().value,
            "meo ten Bun"
        );
        let p: String = conn
            .query_row("SELECT value FROM facts WHERE key='p'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, "plaintext-cu");

        let (nang2, _) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang2, 0);
    }

    /// Guard chống lost-update: UPDATE có điều kiện `value = bản đã đọc` phải
    /// khớp 0 dòng khi value đã bị đổi (tiến trình khác ghi giữa chừng), tức
    /// KHÔNG đè mất bản mới. Kiểm chính cơ chế SQL mà migration dựa vào.
    #[test]
    fn guard_khong_de_mat_ban_moi() {
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', 'V_MOI', 'd', 'd', 't')",
            [],
        ).unwrap();

        // Migration đã đọc 'V_CU' rồi mới ghi — nhưng trên đĩa nay là 'V_MOI'.
        // UPDATE với guard value='V_CU' phải khớp 0 dòng.
        let n = conn
            .execute(
                "UPDATE facts SET value=?1 WHERE key='k' AND value=?2",
                ("V2_CU", "V_CU"),
            )
            .unwrap();
        assert_eq!(n, 0, "value đã đổi -> guard phải chặn, không đè");
        let con_nguyen: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(con_nguyen, "V_MOI", "bản mới phải được GIỮ");

        // Khi value còn đúng bản đã đọc -> UPDATE khớp 1 dòng.
        let n2 = conn
            .execute(
                "UPDATE facts SET value=?1 WHERE key='k' AND value=?2",
                ("V2_MOI", "V_MOI"),
            )
            .unwrap();
        assert_eq!(n2, 1);
    }

    /// Dữ liệu hỏng/sai khoá KHÔNG được đụng (mã hoá lại rác = mất bản gốc).
    #[test]
    fn migrate_khong_dung_du_lieu_hong() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let rac = format!(
            "{}:{}:{}",
            hex::encode([1u8; 16]),
            hex::encode([2u8; 16]),
            hex::encode([3u8; 16])
        );
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('x', ?1, 'd', 'd', 't')",
            [&rac],
        ).unwrap();

        let (nang, khong) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang, 0);
        assert_eq!(khong, 1);
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on_disk, rac);
    }

    /// KỊCH BẢN CỐT LÕI của "bỏ khoá mặc định": fact đang mã hoá bằng khoá MẶC
    /// ĐỊNH ("0"×32, giống máy dev) phải được rekey sang khoá THẬT tại chỗ, đọc
    /// lại nguyên vẹn, và sau đó khoá mặc định KHÔNG mở được nữa. Idempotent.
    #[test]
    fn rekey_chuyen_fact_tu_khoa_mac_dinh_sang_khoa_that() {
        let default_engine = EncryptionEngine::new(crate::crypto::DEFAULT_ENCRYPTION_KEY);
        let live = EncryptionEngine::new("khoa-that-su-bi-mat-cua-Duong-99");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        let enc_default = default_engine.encrypt("mèo tên Bún").unwrap();
        assert!(enc_default.starts_with("v2:"), "khoá mặc định cũng sinh v2");
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc_default],
        ).unwrap();
        // Trước rekey: khoá thật KHÔNG mở được (đây là ca 'v2 nhưng live sai khoá').
        assert!(live.read_fact(&enc_default).is_locked());

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!((so, khong), (1, 0), "1 fact chuyển sang khoá thật, 0 mất");

        // Đọc được bằng khoá THẬT; khoá mặc định KHÔNG còn mở được.
        assert_eq!(
            get_fact(&conn, &live, "k").unwrap().unwrap().value,
            "mèo tên Bún"
        );
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(
            default_engine.read_fact(&on_disk).is_locked(),
            "sau rekey, khoá mặc định phải hết mở được"
        );

        // Idempotent: đã v2 + live giải được -> lần hai không rekey gì.
        let (so2, _) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!(so2, 0, "chạy lại không rekey (idempotent)");
    }

    /// Fact đã ở khoá live rồi thì KHÔNG đụng (không đổi salt vô ích) — tiêu chí
    /// idempotent là 'v2 + live giải được', không phải chỉ tiền tố v2.
    #[test]
    fn rekey_de_nguyen_fact_da_o_khoa_live() {
        let live = EncryptionEngine::new("khoa-that-su-bi-mat-cua-Duong-99");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let enc = live.encrypt("đã ở khoá live").unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc],
        ).unwrap();
        let truoc: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[]).unwrap();
        assert_eq!((so, khong), (0, 0));
        let sau: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(truoc, sau, "đã ở khoá live -> giữ nguyên byte");
    }

    /// Không khoá nào (live + phụ) mở được -> KHÔNG đụng bản gốc + đếm. Chống
    /// mã hoá lại rác làm mất dữ liệu người dùng khoá thật khác/ dữ liệu hỏng.
    #[test]
    fn rekey_de_nguyen_fact_khong_khoa_nao_mo_duoc() {
        let live = EncryptionEngine::new("khoa-live-aaaaaaaaaaaaaaaaaaaaaa");
        let unknown = EncryptionEngine::new("khoa-la-khong-ai-biet-bbbbbbbbbb");
        let default_engine = EncryptionEngine::new(crate::crypto::DEFAULT_ENCRYPTION_KEY);
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let enc_unknown = unknown.encrypt("mã bằng khoá lạ").unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc_unknown],
        ).unwrap();

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!(
            (so, khong),
            (0, 1),
            "không khoá nào mở được -> đếm 1, không rekey"
        );
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on_disk, enc_unknown, "bản gốc GIỮ NGUYÊN, không mã lại rác");
    }

    fn mk_fact(key: &str, value: &str) -> Fact {
        Fact {
            key: key.into(),
            value: value.into(),
            createdAt: "d".into(),
            updatedAt: "d".into(),
            ttlDays: None,
            source: "t".into(),
            category: None,
            importance: 0.5,
            confidenceScore: 1.0,
            sourceTurnId: None,
            memory_strength: 1.0,
            last_accessed_at: 0,
            access_count: 0,
        }
    }

    /// set_fact BACKUP-BEFORE-OVERWRITE: đè một value ĐANG locked (sai khoá) phải
    /// SAO LƯU ciphertext gốc vào facts_locked_backup TRƯỚC — không mất bản gốc,
    /// khôi phục được khi có đúng khoá. Đây là chốt chống-mất ở tầng GHI.
    #[test]
    fn set_fact_sao_luu_value_locked_truoc_khi_de() {
        let a = EncryptionEngine::new("khoa-A-that-su-1234567890abcdef");
        let b = EncryptionEngine::new("khoa-B-khac-han-fedcba0987654321");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        set_fact(&conn, &a, &mk_fact("k", "bí mật gốc")).unwrap();
        let old_cipher: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(
            b.read_fact(&old_cipher).is_locked(),
            "dưới khoá B, value cũ là locked"
        );

        // Ghi đè bằng khoá B (value cũ locked) → phải sao lưu bản gốc.
        set_fact(&conn, &b, &mk_fact("k", "giá trị mới")).unwrap();

        let backup: String = conn
            .query_row(
                "SELECT value FROM facts_locked_backup WHERE key='k'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            backup, old_cipher,
            "ciphertext gốc phải được sao lưu nguyên vẹn"
        );
        assert_eq!(
            a.read_fact(&backup).into_value(),
            "bí mật gốc",
            "khôi phục được bằng khoá A"
        );
        assert_eq!(
            get_fact(&conn, &b, "k").unwrap().unwrap().value,
            "giá trị mới"
        );
    }

    /// Ghi đè value ĐỌC ĐƯỢC (khoá đúng) thì KHÔNG sao lưu — tránh bloat backup
    /// cho mọi lần ghi bình thường.
    #[test]
    fn set_fact_khong_sao_luu_khi_value_doc_duoc() {
        let a = EncryptionEngine::new("khoa-A-that-su-1234567890abcdef");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        set_fact(&conn, &a, &mk_fact("k", "v1")).unwrap();
        set_fact(&conn, &a, &mk_fact("k", "v2")).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM facts_locked_backup", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "value đọc được -> không sao lưu");
        assert_eq!(get_fact(&conn, &a, "k").unwrap().unwrap().value, "v2");
    }
}
