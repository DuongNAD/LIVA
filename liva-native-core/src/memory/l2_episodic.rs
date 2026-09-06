use std::sync::Arc;
use rusqlite::{params, Connection};

use crate::db::DatabasePool;
use crate::memory::enclave::MemoryEnclave;

pub const DEFAULT_BASE_HALF_LIFE_SECS: i64 = 604_800; // 7 days
pub const ACTIVE_RETENTION_THRESHOLD: f64 = 0.35;
pub const DORMANT_RETENTION_THRESHOLD: f64 = 0.10;
pub const ARCHIVE_RETENTION_THRESHOLD: f64 = 0.02;

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct EpisodicEvent {
    pub memory_id: String,
    pub session_id: String,
    pub domain: String,
    pub category: String,
    pub content: String,
    pub importance_score: f64,
    pub emotional_valence: f64,
    pub recall_count: u32,
    pub created_at: i64,
    pub last_recalled_at: i64,
    pub base_half_life_secs: i64,
    pub retention_score: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RetentionTier {
    Active,
    Dormant,
    Archive,
    PurgeCandidate,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct RetentionSweepReport {
    pub total_processed: usize,
    pub active_count: usize,
    pub dormant_count: usize,
    pub archive_count: usize,
    pub purged_count: usize,
}

/// Calculate dynamic half-life: \tau(m) = \tau_0 * (1 + \beta * n)^\gamma * \kappa(importance) * \omega(valence)
pub fn compute_half_life_secs(
    base_half_life: f64,
    recall_count: u32,
    importance: f64,
    valence: f64,
) -> f64 {
    let beta = 0.5_f64;
    let gamma = 1.2_f64;
    let reinforcement = (1.0 + beta * (recall_count as f64)).powf(gamma);
    let kappa = 0.5 + 0.25 * importance.clamp(1.0, 10.0);
    let omega = valence.clamp(0.8, 1.5);
    base_half_life * reinforcement * kappa * omega
}

/// Calculate Ebbinghaus retention: R(m, t) = 2^(- \Delta t / \tau) = exp(-ln(2) * \Delta t / \tau)
pub fn compute_retention(delta_time_secs: f64, half_life_secs: f64) -> f64 {
    if half_life_secs <= 0.0 {
        return 0.0;
    }
    if delta_time_secs <= 0.0 {
        return 1.0;
    }
    (-std::f64::consts::LN_2 * (delta_time_secs / half_life_secs)).exp()
}

/// Classify retention score into lifecycle tiers
pub fn classify_retention(score: f64) -> RetentionTier {
    if score >= ACTIVE_RETENTION_THRESHOLD {
        RetentionTier::Active
    } else if score >= DORMANT_RETENTION_THRESHOLD {
        RetentionTier::Dormant
    } else if score >= ARCHIVE_RETENTION_THRESHOLD {
        RetentionTier::Archive
    } else {
        RetentionTier::PurgeCandidate
    }
}

/// L2 Episodic Memory SQLite Store with dynamic Ebbinghaus retention decay
pub struct L2EpisodicStore {
    pool: DatabasePool,
    enclave: Arc<MemoryEnclave>,
}

impl L2EpisodicStore {
    pub fn new(pool: DatabasePool, enclave: Arc<MemoryEnclave>) -> Self {
        Self { pool, enclave }
    }

    /// Access database pool reference.
    pub fn pool(&self) -> &DatabasePool {
        &self.pool
    }

    /// Initialize the episodic memory table and indexes.
    pub fn init_schema(&self) -> Result<(), String> {
        let conn = self.pool.writer.get().map_err(|e| e.to_string())?;
        Self::create_schema_on_conn(&conn)
    }

    pub fn create_schema_on_conn(conn: &Connection) -> Result<(), String> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS episodic_memory (
                memory_id TEXT PRIMARY KEY,
                session_id TEXT NOT NULL,
                domain TEXT NOT NULL DEFAULT 'local',
                category TEXT NOT NULL,
                content_encrypted TEXT NOT NULL,
                importance_score REAL NOT NULL DEFAULT 5.0,
                emotional_valence REAL NOT NULL DEFAULT 1.0,
                recall_count INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                last_recalled_at INTEGER NOT NULL,
                base_half_life_secs INTEGER NOT NULL DEFAULT 604800,
                retention_score REAL NOT NULL DEFAULT 1.0
            );

            CREATE INDEX IF NOT EXISTS idx_episodic_retention 
            ON episodic_memory(domain, retention_score, last_recalled_at);

            CREATE INDEX IF NOT EXISTS idx_episodic_session
            ON episodic_memory(session_id, created_at);",
        ).map_err(|e| format!("Schema init error: {e}"))?;
        Ok(())
    }

    /// Insert a new episodic event, encrypting its content via MemoryEnclave.
    pub fn insert_event(&self, event: &EpisodicEvent) -> Result<String, String> {
        let encrypted_content = self.enclave.encrypt_string(&event.content)
            .map_err(|e| format!("Encryption error: {e}"))?;

        let conn = self.pool.writer.get().map_err(|e| e.to_string())?;
        conn.execute(
            "INSERT INTO episodic_memory (
                memory_id, session_id, domain, category, content_encrypted,
                importance_score, emotional_valence, recall_count,
                created_at, last_recalled_at, base_half_life_secs, retention_score
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                event.memory_id,
                event.session_id,
                event.domain,
                event.category,
                encrypted_content,
                event.importance_score,
                event.emotional_valence,
                event.recall_count,
                event.created_at,
                event.last_recalled_at,
                event.base_half_life_secs,
                event.retention_score,
            ],
        ).map_err(|e| format!("Insert error: {e}"))?;

        Ok(event.memory_id.clone())
    }

    /// Record a recall event: increments recall_count, updates last_recalled_at,
    /// and recalculates dynamic retention score.
    pub fn record_recall(&self, memory_id: &str, current_timestamp: i64) -> Result<f64, String> {
        let conn = self.pool.writer.get().map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT importance_score, emotional_valence, recall_count, base_half_life_secs 
             FROM episodic_memory WHERE memory_id = ?1",
        ).map_err(|e| e.to_string())?;

        let row = stmt.query_row(params![memory_id], |r| {
            Ok((
                r.get::<_, f64>(0)?,
                r.get::<_, f64>(1)?,
                r.get::<_, u32>(2)?,
                r.get::<_, i64>(3)?,
            ))
        }).map_err(|e| format!("Event not found: {e}"))?;

        let (importance, valence, old_recalls, base_half_life) = row;
        let new_recalls = old_recalls + 1;
        let _half_life = compute_half_life_secs(
            base_half_life as f64,
            new_recalls,
            importance,
            valence,
        );

        // Retention refreshed to 1.0 immediately upon recall
        let new_retention = 1.0;

        conn.execute(
            "UPDATE episodic_memory 
             SET recall_count = ?1, last_recalled_at = ?2, retention_score = ?3 
             WHERE memory_id = ?4",
            params![new_recalls, current_timestamp, new_retention, memory_id],
        ).map_err(|e| format!("Update error: {e}"))?;

        Ok(new_retention)
    }

    /// Sweep and update retention scores for all events based on elapsed time.
    pub fn sweep_retention(&self, current_timestamp: i64) -> Result<RetentionSweepReport, String> {
        let conn = self.pool.writer.get().map_err(|e| e.to_string())?;

        let mut stmt = conn.prepare(
            "SELECT memory_id, last_recalled_at, base_half_life_secs, recall_count, importance_score, emotional_valence 
             FROM episodic_memory",
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, i64>(2)?,
                r.get::<_, u32>(3)?,
                r.get::<_, f64>(4)?,
                r.get::<_, f64>(5)?,
            ))
        }).map_err(|e| e.to_string())?;

        let mut updates = Vec::new();
        let mut report = RetentionSweepReport {
            total_processed: 0,
            active_count: 0,
            dormant_count: 0,
            archive_count: 0,
            purged_count: 0,
        };

        for row in rows {
            let (id, last_recalled, base_half_life, recalls, importance, valence) = row.map_err(|e| e.to_string())?;
            let delta_secs = (current_timestamp - last_recalled).max(0) as f64;
            let half_life = compute_half_life_secs(base_half_life as f64, recalls, importance, valence);
            let score = compute_retention(delta_secs, half_life);

            report.total_processed += 1;
            match classify_retention(score) {
                RetentionTier::Active => report.active_count += 1,
                RetentionTier::Dormant => report.dormant_count += 1,
                RetentionTier::Archive => report.archive_count += 1,
                RetentionTier::PurgeCandidate => report.purged_count += 1,
            }

            updates.push((id, score));
        }

        drop(stmt);

        let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
        {
            let mut update_stmt = tx.prepare(
                "UPDATE episodic_memory SET retention_score = ?1 WHERE memory_id = ?2",
            ).map_err(|e| e.to_string())?;

            for (id, score) in updates {
                update_stmt.execute(params![score, id]).map_err(|e| e.to_string())?;
            }
        }
        tx.commit().map_err(|e| e.to_string())?;

        Ok(report)
    }

    /// Retrieve active episodic events for a domain above retention threshold.
    pub fn get_active_events(&self, domain: &str, threshold: f64) -> Result<Vec<EpisodicEvent>, String> {
        let conn = self.pool.readers.get().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT memory_id, session_id, domain, category, content_encrypted,
                    importance_score, emotional_valence, recall_count,
                    created_at, last_recalled_at, base_half_life_secs, retention_score
             FROM episodic_memory
             WHERE (domain = ?1 OR ?1 = '') AND retention_score >= ?2
             ORDER BY retention_score DESC, created_at DESC",
        ).map_err(|e| e.to_string())?;

        let rows = stmt.query_map(params![domain, threshold], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, u32>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, f64>(11)?,
            ))
        }).map_err(|e| e.to_string())?;

        let mut events = Vec::new();
        for row in rows {
            let (id, session_id, dom, category, encrypted, importance, valence, recalls, created, last_recalled, half_life, retention) = row.map_err(|e| e.to_string())?;
            let decrypted = self.enclave.decrypt_string(&encrypted)
                .map_err(|e| format!("Decryption failed for {id}: {e}"))?;

            events.push(EpisodicEvent {
                memory_id: id,
                session_id,
                domain: dom,
                category,
                content: (*decrypted).clone(),
                importance_score: importance,
                emotional_valence: valence,
                recall_count: recalls,
                created_at: created,
                last_recalled_at: last_recalled,
                base_half_life_secs: half_life,
                retention_score: retention,
            });
        }

        Ok(events)
    }

    /// Retrieve an individual event by its ID.
    pub fn get_event_by_id(&self, memory_id: &str) -> Result<Option<EpisodicEvent>, String> {
        let conn = self.pool.readers.get().map_err(|e| e.to_string())?;
        let mut stmt = conn.prepare(
            "SELECT memory_id, session_id, domain, category, content_encrypted,
                    importance_score, emotional_valence, recall_count,
                    created_at, last_recalled_at, base_half_life_secs, retention_score
             FROM episodic_memory WHERE memory_id = ?1",
        ).map_err(|e| e.to_string())?;

        let mut rows = stmt.query_map(params![memory_id], |r| {
            Ok((
                r.get::<_, String>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, f64>(5)?,
                r.get::<_, f64>(6)?,
                r.get::<_, u32>(7)?,
                r.get::<_, i64>(8)?,
                r.get::<_, i64>(9)?,
                r.get::<_, i64>(10)?,
                r.get::<_, f64>(11)?,
            ))
        }).map_err(|e| e.to_string())?;

        if let Some(row) = rows.next() {
            let (id, session_id, dom, category, encrypted, importance, valence, recalls, created, last_recalled, half_life, retention) = row.map_err(|e| e.to_string())?;
            let decrypted = self.enclave.decrypt_string(&encrypted)
                .map_err(|e| format!("Decryption failed for {id}: {e}"))?;

            Ok(Some(EpisodicEvent {
                memory_id: id,
                session_id,
                domain: dom,
                category,
                content: (*decrypted).clone(),
                importance_score: importance,
                emotional_valence: valence,
                recall_count: recalls,
                created_at: created,
                last_recalled_at: last_recalled,
                base_half_life_secs: half_life,
                retention_score: retention,
            }))
        } else {
            Ok(None)
        }
    }

    /// Purge decayed events below a threshold (default 0.02) from the store.
    pub fn purge_decayed_events(&self, threshold: f64) -> Result<usize, String> {
        let conn = self.pool.writer.get().map_err(|e| e.to_string())?;
        let affected = conn.execute(
            "DELETE FROM episodic_memory WHERE retention_score < ?1",
            params![threshold],
        ).map_err(|e| format!("Purge error: {e}"))?;
        Ok(affected)
    }
}
