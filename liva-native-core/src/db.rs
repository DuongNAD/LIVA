#![allow(non_snake_case)]
#![allow(dead_code)]
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

        let ext = if cfg!(target_os = "windows") {
            ".dll"
        } else if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".so"
        };

        let platform_dirs = if cfg!(target_os = "windows") {
            vec!["sqlite-vec-windows-x64", "sqlite-vec-windows-arm64"]
        } else if cfg!(target_os = "macos") {
            vec!["sqlite-vec-darwin-x64", "sqlite-vec-darwin-arm64"]
        } else {
            vec!["sqlite-vec-linux-x64", "sqlite-vec-linux-arm64"]
        };

        let mut candidates = Vec::new();
        for dir in &platform_dirs {
            candidates.push(format!("node_modules/{}/vec0{}", dir, ext));
            candidates.push(format!("../node_modules/{}/vec0{}", dir, ext));
            candidates.push(format!("../../node_modules/{}/vec0{}", dir, ext));
        }
        candidates.push(format!("vec0{}", ext));
        candidates.push("vec0".to_string());

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
            let err_msg = last_err
                .map(|e| e.to_string())
                .unwrap_or_else(|| "Unknown extension load error".to_string());
            Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(std::io::ErrorKind::Other, err_msg),
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

        CREATE INDEX IF NOT EXISTS idx_events_pending ON events(eventId) WHERE consolidation_status = 'pending';
        CREATE INDEX IF NOT EXISTS idx_events_consolidated_ts ON events(consolidated, timestamp) WHERE consolidation_status = 'pending';

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
            "CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[384])",
            [],
        )?;
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
    let encrypted_val = match engine.encrypt(&fact.value) {
        Ok(v) => v,
        Err(e) => {
            return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                std::io::Error::new(
                    std::io::ErrorKind::Other,
                    format!("Encryption failed: {}", e),
                ),
            )));
        }
    };

    conn.execute(
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

    Ok(())
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
        let decrypted_value = engine.decrypt(&enc_value);

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

pub fn search_similar_vectors(
    conn: &Connection,
    query_vector: &[f32],
    top_k: usize,
    filter: &MetadataFilter,
) -> Result<Vec<VectorSearchResult>, rusqlite::Error> {
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
    use crate::crypto::EncryptionEngine;

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
        assert_eq!(raw_val.split(':').count(), 3);

        // Retrieve using get_fact, should be decrypted
        let retrieved = get_fact(&conn, &engine, "user_name").unwrap().unwrap();
        assert_eq!(retrieved.value, "Alice");
        assert_eq!(retrieved.source, "user_input");
        assert_eq!(retrieved.importance, 0.8);
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
}
