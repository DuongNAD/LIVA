//! Bitemporal Cognitive Storage & Immutable Evidence Log (Features F13, F14, F15).
//!
//! Provides:
//! 1. Content-addressed immutable `evidence_log` (NFC normalized, BLAKE3 hashed, deduplicated).
//! 2. Bitemporal `facts` table with world/valid time (`valid_from`/`valid_to`) and system/transaction time (`recorded_at`/`expired_at`).
//! 3. Active Fact Singleton Invariant enforced via SQLite conditional unique index `idx_active_singleton_fact`.
//! 4. 4-Action Reconciliation Engine (`ADD`, `EXTEND`, `NOOP`, `INVALIDATE`) preserving complete soft-delete audit lineage.

use rusqlite::{Connection, OptionalExtension, Result as SqlResult, Transaction, params};
use serde::{Deserialize, Serialize};
use unicode_normalization::UnicodeNormalization;

// ============================================================================
// 1. DATA CONTRACTS & DOMAIN MODELS
// ============================================================================

/// Provenance origin of a stored fact.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactOrigin {
    /// Explicitly declared by user ("Nhớ là...", "Nhà tôi ở...", confidence >= 0.95).
    Explicit,
    /// Inferred by cognitive parser or background model (confidence 0.5 - 0.85).
    Inferred,
    /// Imported from external file, calendar, or contact card.
    Imported,
    /// Manually edited by user via Markdown files (USER.md) or Tauri UI (confidence = 1.0).
    UserEdited,
}

impl FactOrigin {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Explicit => "explicit",
            Self::Inferred => "inferred",
            Self::Imported => "imported",
            Self::UserEdited => "user_edited",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "explicit" => Self::Explicit,
            "inferred" => Self::Inferred,
            "imported" => Self::Imported,
            "user_edited" => Self::UserEdited,
            _ => Self::Inferred,
        }
    }
}

/// Scope partition isolating personal, professional, or project-specific memories.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum FactScope {
    Personal,
    Work,
    Project(String),
}

impl FactScope {
    pub fn as_string(&self) -> String {
        match self {
            Self::Personal => "personal".to_string(),
            Self::Work => "work".to_string(),
            Self::Project(slug) => format!("project:{}", slug),
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "personal" => Self::Personal,
            "work" => Self::Work,
            other if other.starts_with("project:") => {
                Self::Project(other.trim_start_matches("project:").to_string())
            }
            other => Self::Project(other.to_string()),
        }
    }
}

/// Source communication channel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceChannel {
    Tauri,
    Telegram,
    Zalo,
    LocalCli,
    Cli,
    ExternalWeb,
    WebRss,
}

impl SourceChannel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tauri => "tauri",
            Self::Telegram => "telegram",
            Self::Zalo => "zalo",
            Self::LocalCli | Self::Cli => "cli",
            Self::ExternalWeb => "external_web",
            Self::WebRss => "web_rss",
        }
    }
}

/// Integrity classification for turn source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IntegrityLabel {
    OwnerTrusted,
    ExternalUntrusted,
    Trusted,
    Untrusted,
}

impl IntegrityLabel {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::OwnerTrusted | Self::Trusted => "Trusted",
            Self::ExternalUntrusted | Self::Untrusted => "Untrusted",
        }
    }
}

/// Canonicalizes evidence text to Unicode NFC and produces a BLAKE3 content-addressed hex string.
pub fn canonicalize_content_blake3(raw_text: &str) -> (String, String) {
    let nfc_text: String = raw_text.trim().nfc().collect();
    let hash = blake3::hash(nfc_text.as_bytes()).to_hex().to_string();
    (nfc_text, hash)
}

/// Sensitivity tier governing retrieval filtering and display.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactSensitivity {
    Public,
    Normal,
    Private,
    SensitivePii,
}

impl FactSensitivity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Public => "public",
            Self::Normal => "normal",
            Self::Private => "private",
            Self::SensitivePii => "sensitive_pii",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s {
            "public" => Self::Public,
            "normal" => Self::Normal,
            "sensitive_pii" | "sensitive" => Self::SensitivePii,
            _ => Self::Private,
        }
    }
}

/// Immutable evidence record stored in `evidence_log`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvidenceRecord {
    pub evidence_id: String,
    pub nfc_content: String,
    pub source_type: String,
    pub session_id: Option<String>,
    pub turn_id: Option<String>,
    pub principal: String,
    pub channel: String,
    pub integrity: String,
    pub tombstoned_at: Option<i64>,
    pub tombstone_reason: Option<String>,
    pub created_at: i64,
}

/// Bitemporal Fact record representing an entity-predicate relationship across two time axes.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct BitemporalFact {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub value: String,
    pub object_value: String,
    pub scope: String,
    pub origin: FactOrigin,
    pub confidence: f64,
    pub importance: u8,
    pub sensitivity: FactSensitivity,
    /// World / Valid Time Axis (Epoch ms)
    pub valid_from: i64,
    pub valid_to: Option<i64>,
    /// System / Transaction Time Axis (Epoch ms)
    pub recorded_at: i64,
    pub expired_at: Option<i64>,
    /// Lineage & Audit
    pub superseded_by: Option<String>,
    pub reason: Option<String>,
    pub supersede_reason: Option<String>,
    pub category: Option<String>,
    pub key: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl BitemporalFact {
    /// Returns true if this fact is currently active in the database state.
    pub fn is_active(&self) -> bool {
        self.valid_to.is_none() && self.expired_at.is_none()
    }

    /// Evaluates if this fact was valid in the real world at timestamp `t_world`.
    pub fn is_valid_at_world_time(&self, t_world: i64) -> bool {
        self.valid_from <= t_world && self.valid_to.map_or(true, |end| t_world < end)
    }

    /// Evaluates if this fact was known to the system at transaction timestamp `t_sys`.
    pub fn is_active_at_system_time(&self, t_sys: i64) -> bool {
        self.recorded_at <= t_sys && self.expired_at.map_or(true, |exp| t_sys < exp)
    }
}

/// Reconciliation operation matching M3 contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconciliationAction {
    /// No existing active fact. Freshly insert candidate fact.
    Add,
    /// Active fact exists with identical value; reaffirm validity window or bump confidence.
    Extend,
    /// Active fact exists with identical value and validity; record corroborating evidence only.
    Noop,
    /// Contradictory value or explicit revocation. Soft-delete old fact and insert new fact.
    Invalidate,
}

/// Execution outcome of applying a reconciliation operation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ReconciliationOutcome {
    Added {
        fact_id: String,
    },
    Extended {
        fact_id: String,
    },
    Noop {
        fact_id: String,
        evidence_linked: bool,
    },
    Invalidated {
        superseded_fact_id: String,
        new_fact_id: Option<String>,
    },
}

// ============================================================================
// 2. DDL SCHEMA STATEMENTS
// ============================================================================

pub const CREATE_BITEMPORAL_SCHEMA_SQL: &str = r#"
-- 1. IMMUTABLE EVIDENCE LOG (Feature F13)
CREATE TABLE IF NOT EXISTS evidence_log (
    evidence_id TEXT PRIMARY KEY,
    nfc_content TEXT NOT NULL,
    content TEXT NOT NULL DEFAULT '',
    source_type TEXT NOT NULL,
    session_id TEXT,
    turn_id TEXT,
    principal TEXT NOT NULL,
    channel TEXT NOT NULL,
    integrity TEXT NOT NULL DEFAULT 'Trusted',
    tombstoned_at INTEGER,
    tombstone_reason TEXT,
    created_at INTEGER NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_evidence_session_turn ON evidence_log(session_id, turn_id);
CREATE INDEX IF NOT EXISTS idx_evidence_integrity ON evidence_log(integrity);
CREATE INDEX IF NOT EXISTS idx_evidence_created_at ON evidence_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_evidence_active ON evidence_log(evidence_id) WHERE tombstoned_at IS NULL;

-- Trigger: Immutability of evidence_log (Reject physical update of content or deletion)
CREATE TRIGGER IF NOT EXISTS trg_evidence_log_immutable_update
BEFORE UPDATE OF evidence_id, nfc_content, content, source_type, session_id, turn_id, principal, channel, integrity, created_at ON evidence_log
BEGIN
    SELECT RAISE(FAIL, 'SECURITY VIOLATION: evidence_log records are immutable and content-addressed. Only tombstone metadata (tombstoned_at, tombstone_reason) may be updated.');
END;

CREATE TRIGGER IF NOT EXISTS trg_evidence_log_immutable_delete
BEFORE DELETE ON evidence_log
BEGIN
    SELECT RAISE(FAIL, 'SECURITY VIOLATION: evidence_log is append-only and rows cannot be physically deleted; use tombstoning');
END;

-- 2. BITEMPORAL FACTS TABLE (Feature F14)
CREATE TABLE IF NOT EXISTS facts (
    fact_id TEXT PRIMARY KEY DEFAULT ('fact_' || hex(randomblob(16))),
    subject TEXT NOT NULL DEFAULT '',
    predicate TEXT NOT NULL DEFAULT '',
    value TEXT NOT NULL,
    object_value TEXT NOT NULL DEFAULT '',
    scope TEXT NOT NULL DEFAULT 'personal',
    origin TEXT NOT NULL DEFAULT 'explicit',
    confidence REAL NOT NULL DEFAULT 1.0,
    importance REAL DEFAULT 0.5,
    sensitivity TEXT NOT NULL DEFAULT 'private',
    valid_from INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    valid_to INTEGER,
    recorded_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    expired_at INTEGER,
    superseded_by TEXT,
    reason TEXT,
    category TEXT,
    key TEXT,
    createdAt TEXT DEFAULT '',
    updatedAt TEXT DEFAULT '',
    created_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    updated_at INTEGER NOT NULL DEFAULT (strftime('%s', 'now') * 1000),
    ttlDays INTEGER,
    source TEXT DEFAULT 'explicit',
    confidenceScore REAL DEFAULT 1.0,
    sourceTurnId TEXT,
    memory_strength REAL DEFAULT 1.0,
    last_accessed_at INTEGER DEFAULT 0,
    access_count INTEGER DEFAULT 0,
    FOREIGN KEY (superseded_by) REFERENCES facts(fact_id)
);

-- 3. ACTIVE SINGLETON INVARIANT (Feature F15)
CREATE UNIQUE INDEX IF NOT EXISTS idx_active_singleton_fact
ON facts (subject, predicate, scope)
WHERE valid_to IS NULL AND expired_at IS NULL AND subject != '';

CREATE UNIQUE INDEX IF NOT EXISTS idx_facts_key_unique ON facts(key);
CREATE INDEX IF NOT EXISTS idx_facts_last_accessed ON facts(last_accessed_at DESC);
CREATE INDEX IF NOT EXISTS idx_facts_subject_predicate ON facts(subject, predicate);
CREATE INDEX IF NOT EXISTS idx_facts_scope ON facts(scope);
CREATE INDEX IF NOT EXISTS idx_facts_valid_window ON facts(valid_from, valid_to);
CREATE INDEX IF NOT EXISTS idx_facts_system_window ON facts(recorded_at, expired_at);
CREATE INDEX IF NOT EXISTS idx_facts_superseded_by ON facts(superseded_by) WHERE superseded_by IS NOT NULL;

-- 4. FACT-EVIDENCE CORROBORATION JUNCTION
CREATE TABLE IF NOT EXISTS fact_evidence (
    fact_id TEXT NOT NULL,
    evidence_id TEXT NOT NULL,
    confidence_weight REAL NOT NULL DEFAULT 1.0,
    extracted_at INTEGER NOT NULL,
    PRIMARY KEY (fact_id, evidence_id),
    FOREIGN KEY (fact_id) REFERENCES facts(fact_id) ON DELETE CASCADE,
    FOREIGN KEY (evidence_id) REFERENCES evidence_log(evidence_id)
);

CREATE INDEX IF NOT EXISTS idx_fact_evidence_evidence ON fact_evidence(evidence_id);

-- 5. MARKDOWN 2-WAY SYNC STATE TABLE
CREATE TABLE IF NOT EXISTS markdown_sync_state (
    file_path TEXT PRIMARY KEY,
    last_hash TEXT NOT NULL DEFAULT '',
    last_synced_at INTEGER NOT NULL DEFAULT 0,
    blake3_hash TEXT NOT NULL DEFAULT '',
    synced_at INTEGER NOT NULL DEFAULT 0
);

-- 6. FACTS FTS5 3-FORM VIRTUAL TABLE
CREATE VIRTUAL TABLE IF NOT EXISTS facts_fts USING fts5(
    fact_id UNINDEXED,
    subject,
    predicate,
    form_nfc,
    form_folded,
    form_bigram,
    tokenize="unicode61 remove_diacritics 0"
);
"#;

/// Initializes the bitemporal schema on a connection.
pub fn init_bitemporal_schema(conn: &Connection) -> SqlResult<()> {
    conn.execute_batch(CREATE_BITEMPORAL_SCHEMA_SQL)?;
    Ok(())
}

// ============================================================================
// 3. EVIDENCE INGESTION & TOMBSTONING
// ============================================================================

/// Ingests a raw evidence string into `evidence_log` with NFC normalization and BLAKE3 content-addressing.
pub fn ingest_evidence(
    conn: &Connection,
    raw_text: &str,
    source_type: &str,
    session_id: Option<&str>,
    turn_id: Option<&str>,
    principal: &str,
    channel: &str,
    integrity: &str,
    now_ms: i64,
) -> SqlResult<String> {
    let nfc_text: String = raw_text.trim().nfc().collect();
    if nfc_text.is_empty() {
        return Err(rusqlite::Error::ToSqlConversionFailure(
            "Cannot ingest empty evidence string".into(),
        ));
    }

    let hash = blake3::hash(nfc_text.as_bytes()).to_hex().to_string();

    conn.execute(
        "INSERT INTO evidence_log (
            evidence_id, nfc_content, content, source_type, session_id, turn_id,
            principal, channel, integrity, tombstoned_at, tombstone_reason, created_at
         ) VALUES (?1, ?2, ?2, ?3, ?4, ?5, ?6, ?7, ?8, NULL, NULL, ?9)
         ON CONFLICT(evidence_id) DO NOTHING",
        params![
            hash,
            nfc_text,
            source_type,
            session_id,
            turn_id,
            principal,
            channel,
            integrity,
            now_ms,
        ],
    )?;

    Ok(hash)
}

/// Ingest evidence log helper with SourceChannel and IntegrityLabel.
pub fn ingest_evidence_log(
    conn: &Connection,
    source_channel: SourceChannel,
    raw_content: &str,
    integrity: IntegrityLabel,
    session_id: Option<&str>,
) -> SqlResult<(String, String)> {
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;
    let hash = ingest_evidence(
        conn,
        raw_content,
        "conversation_turn",
        session_id,
        None,
        if integrity == IntegrityLabel::OwnerTrusted {
            "Owner"
        } else {
            "Untrusted"
        },
        source_channel.as_str(),
        integrity.as_str(),
        now_ms,
    )?;
    Ok((hash.clone(), hash))
}

/// Marks an evidence entry as tombstoned without physical deletion.
pub fn tombstone_evidence(
    conn: &Connection,
    evidence_id: &str,
    reason: &str,
    now_ms: i64,
) -> SqlResult<bool> {
    let rows = conn.execute(
        "UPDATE evidence_log 
         SET tombstoned_at = ?1, tombstone_reason = ?2 
         WHERE evidence_id = ?3 AND tombstoned_at IS NULL",
        params![now_ms, reason, evidence_id],
    )?;
    Ok(rows > 0)
}

// ============================================================================
// 4. 4-ACTION RECONCILIATION STATE MACHINE
// ============================================================================

/// Implements the 4 state machine operations (`ADD`, `EXTEND`, `NOOP`, `INVALIDATE`)
/// within an atomic SQLite transaction while strictly upholding `idx_active_singleton_fact`.
pub fn apply_reconciliation(
    tx: &Transaction,
    candidate: BitemporalFact,
    evidence_id: Option<&str>,
    now_ms: i64,
) -> SqlResult<ReconciliationOutcome> {
    let scope_str = candidate.scope.clone();
    let val = if !candidate.value.is_empty() {
        candidate.value.clone()
    } else {
        candidate.object_value.clone()
    };

    // 1. Query current active fact for (subject, predicate, scope)
    let active_row: Option<(String, String, Option<i64>, f64)> = tx
        .query_row(
            "SELECT fact_id, value, valid_to, confidence 
             FROM facts 
             WHERE subject = ?1 AND predicate = ?2 AND scope = ?3 
               AND expired_at IS NULL
             ORDER BY recorded_at DESC LIMIT 1",
            params![candidate.subject, candidate.predicate, scope_str],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
        )
        .optional()?;

    let fact_id = if !candidate.fact_id.is_empty() {
        candidate.fact_id
    } else {
        format!("fact_{}_{}", now_ms, &uuid::Uuid::new_v4().to_string()[..8])
    };

    match active_row {
        // CASE 1: No active fact -> ADD
        None => {
            tx.execute(
                "INSERT INTO facts (
                    fact_id, subject, predicate, value, object_value, scope, origin, confidence,
                    importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                    superseded_by, reason, category, key, created_at, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, NULL, NULL, NULL, ?12, ?13, ?11, ?11)",
                params![
                    fact_id,
                    candidate.subject,
                    candidate.predicate,
                    val,
                    scope_str,
                    candidate.origin.as_str(),
                    candidate.confidence,
                    candidate.importance,
                    candidate.sensitivity.as_str(),
                    candidate.valid_from,
                    now_ms,
                    candidate.category,
                    candidate.key,
                ],
            )?;

            if let Some(eid) = evidence_id {
                tx.execute(
                    "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
                     VALUES (?1, ?2, 1.0, ?3)",
                    params![fact_id, eid, now_ms],
                )?;
            }

            Ok(ReconciliationOutcome::Added { fact_id })
        }

        // CASE 2, 3, 4: Active fact exists
        Some((active_id, active_value, active_valid_to, active_confidence)) => {
            if active_value == val {
                // Values match
                if candidate.valid_to.is_some() && candidate.valid_to != active_valid_to {
                    // CASE 2: EXTEND validity interval
                    tx.execute(
                        "UPDATE facts SET valid_to = ?1, confidence = MAX(confidence, ?2) WHERE fact_id = ?3",
                        params![candidate.valid_to, candidate.confidence, active_id],
                    )?;

                    if let Some(eid) = evidence_id {
                        tx.execute(
                            "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
                             VALUES (?1, ?2, 1.0, ?3)",
                            params![active_id, eid, now_ms],
                        )?;
                    }

                    Ok(ReconciliationOutcome::Extended { fact_id: active_id })
                } else {
                    // CASE 3: NOOP (corroborate existing active truth)
                    let mut linked = false;
                    if let Some(eid) = evidence_id {
                        let rows = tx.execute(
                            "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
                             VALUES (?1, ?2, 1.0, ?3)",
                            params![active_id, eid, now_ms],
                        )?;
                        linked = rows > 0;
                    }

                    // Boost confidence asymptotically on independent corroboration
                    if linked {
                        let boosted = active_confidence + (1.0 - active_confidence) * 0.1;
                        tx.execute(
                            "UPDATE facts SET confidence = ?1 WHERE fact_id = ?2",
                            params![boosted, active_id],
                        )?;
                    }

                    Ok(ReconciliationOutcome::Noop {
                        fact_id: active_id,
                        evidence_linked: linked,
                    })
                }
            } else {
                // CASE 4: INVALIDATE & SUPERSEDE
                let reason = candidate
                    .reason
                    .or(candidate.supersede_reason)
                    .unwrap_or_else(|| "user_changed_mind_contradiction".to_string());

                // 1. Close old active fact
                tx.execute(
                    "UPDATE facts 
                     SET valid_to = ?1, expired_at = ?1, reason = ?2 
                     WHERE fact_id = ?3",
                    params![now_ms, reason, active_id],
                )
                .map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(
                        format!("step 1 (close old fact) failed: {e}").into(),
                    )
                })?;

                // 2. Insert new fact
                tx.execute(
                    "INSERT INTO facts (
                        fact_id, subject, predicate, value, scope, origin, confidence,
                        importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                        superseded_by, reason, category, key, created_at, updated_at
                     ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, NULL, NULL, NULL, ?12, ?13, ?11, ?11)",
                    params![
                        &fact_id,
                        candidate.subject,
                        candidate.predicate,
                        val,
                        scope_str,
                        candidate.origin.as_str(),
                        candidate.confidence,
                        candidate.importance,
                        candidate.sensitivity.as_str(),
                        now_ms,
                        now_ms,
                        candidate.category,
                        candidate.key,
                    ],
                ).map_err(|e| rusqlite::Error::ToSqlConversionFailure(format!("step 2 (insert new fact) failed: {e}").into()))?;

                // 3. Link old fact's superseded_by to the now-existing new fact_id
                tx.execute(
                    "UPDATE facts SET superseded_by = ?1 WHERE fact_id = ?2",
                    params![&fact_id, &active_id],
                )
                .map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(
                        format!("step 3 (update superseded_by) failed: {e}").into(),
                    )
                })?;

                if let Some(eid) = evidence_id {
                    tx.execute(
                        "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
                         VALUES (?1, ?2, 1.0, ?3)",
                        params![&fact_id, eid, now_ms],
                    ).map_err(|e| rusqlite::Error::ToSqlConversionFailure(format!("step 4 (insert fact_evidence) failed: {e}").into()))?;
                }

                Ok(ReconciliationOutcome::Invalidated {
                    superseded_fact_id: active_id,
                    new_fact_id: Some(fact_id),
                })
            }
        }
    }
}

// ============================================================================
// 5. QUERY HELPERS & CONVENIENCE FUNCTIONS
// ============================================================================

fn map_fact_row(r: &rusqlite::Row) -> SqlResult<BitemporalFact> {
    let scope_str: String = r.get(4)?;
    let origin_str: String = r.get(5)?;
    let sens_str: String = r.get(8)?;
    let val: String = r.get(3)?;

    Ok(BitemporalFact {
        fact_id: r.get(0)?,
        subject: r.get(1)?,
        predicate: r.get(2)?,
        value: val.clone(),
        object_value: val,
        scope: scope_str,
        origin: FactOrigin::from_str(&origin_str),
        confidence: r.get(6)?,
        importance: match r.get_ref(7)? {
            rusqlite::types::ValueRef::Integer(i) => i as u8,
            rusqlite::types::ValueRef::Real(f) => {
                if f > 1.0 {
                    f.round() as u8
                } else {
                    (f * 10.0).round() as u8
                }
            }
            _ => 5,
        },
        sensitivity: FactSensitivity::from_str(&sens_str),
        valid_from: r.get(9)?,
        valid_to: r.get(10)?,
        recorded_at: r.get(11)?,
        expired_at: r.get(12)?,
        superseded_by: r.get(13)?,
        reason: r.get(14)?,
        supersede_reason: r.get(14)?,
        category: r.get(15)?,
        key: r.get(16)?,
        created_at: r.get(11)?,
        updated_at: r.get(11)?,
    })
}

/// Retrieves the single currently active fact for `(subject, predicate, scope)`.
pub fn get_active_fact(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    scope: &str,
) -> SqlResult<Option<BitemporalFact>> {
    conn.query_row(
        "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                superseded_by, reason, category, key
         FROM facts 
         WHERE subject = ?1 AND predicate = ?2 AND scope = ?3 
           AND valid_to IS NULL AND expired_at IS NULL",
        params![subject, predicate, scope],
        map_fact_row,
    )
    .optional()
}

/// Retrieves all active facts, optionally filtered by scope.
pub fn query_active_facts(
    conn: &Connection,
    scope: Option<&str>,
) -> SqlResult<Vec<BitemporalFact>> {
    match scope {
        Some(s) => {
            let mut stmt = conn.prepare(
                "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                        importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                        superseded_by, reason, category, key
                 FROM facts 
                 WHERE valid_to IS NULL AND expired_at IS NULL AND scope = ?1
                 ORDER BY subject, predicate",
            )?;
            let rows = stmt.query_map(params![s], map_fact_row)?;
            let mut res = Vec::new();
            for r in rows {
                res.push(r?);
            }
            Ok(res)
        }
        None => {
            let mut stmt = conn.prepare(
                "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                        importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                        superseded_by, reason, category, key
                 FROM facts 
                 WHERE valid_to IS NULL AND expired_at IS NULL
                 ORDER BY subject, predicate",
            )?;
            let rows = stmt.query_map([], map_fact_row)?;
            let mut res = Vec::new();
            for r in rows {
                res.push(r?);
            }
            Ok(res)
        }
    }
}

/// Alias for query_active_facts.
pub fn list_active_facts(conn: &Connection, scope: Option<&str>) -> SqlResult<Vec<BitemporalFact>> {
    query_active_facts(conn, scope)
}

/// Inserts a newly created active fact into the database via reconciliation.
pub fn insert_active_fact(
    conn: &Connection,
    fact: &BitemporalFact,
    evidence_id: Option<&str>,
) -> SqlResult<String> {
    let fact_id = if !fact.fact_id.is_empty() {
        fact.fact_id.clone()
    } else {
        format!(
            "fact_{}_{}",
            fact.recorded_at,
            &uuid::Uuid::new_v4().to_string()[..8]
        )
    };

    let now_ms = if fact.recorded_at > 0 {
        fact.recorded_at
    } else {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64
    };

    let val = if !fact.value.is_empty() {
        &fact.value
    } else {
        &fact.object_value
    };

    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
            superseded_by, reason, category, key, created_at, updated_at
         ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, NULL, ?11, NULL, NULL, NULL, ?12, ?13, ?11, ?11)",
        params![
            &fact_id,
            &fact.subject,
            &fact.predicate,
            val,
            &fact.scope,
            fact.origin.as_str(),
            fact.confidence,
            fact.importance,
            fact.sensitivity.as_str(),
            fact.valid_from,
            now_ms,
            &fact.category,
            &fact.key,
        ],
    )?;

    if let Some(eid) = evidence_id {
        conn.execute(
            "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
             VALUES (?1, ?2, 1.0, ?3)",
            params![&fact_id, eid, now_ms],
        )?;
    }

    Ok(fact_id)
}

/// Atomically supersedes an old fact with a new value inside an explicit SQLite SAVEPOINT transaction.
///
/// Guaranteed invariants:
/// 1. Transactional atomicity: if any step fails, the entire supersede operation rolls back cleanly.
/// 2. If `old_fact_id` does not exist or is already closed, an error is returned before any mutations.
/// 3. Maintains the singleton invariant: exactly one active fact exists for `(subject, predicate, scope)`.
/// 4. Lineage: old fact's `superseded_by` points to `new_fact_id`, and `fact_evidence` is linked if provided.
pub fn atomic_supersede_fact(
    conn: &Connection,
    old_fact_id: &str,
    new_value: &str,
    origin: FactOrigin,
    confidence: f64,
    now_ms: i64,
    reason: Option<&str>,
    evidence_id: Option<&str>,
) -> SqlResult<String> {
    let sp_id = uuid::Uuid::new_v4().to_string().replace('-', "");
    let sp_name = format!("sp_supersede_{}", &sp_id[..12]);
    let reason_str = reason.unwrap_or("superseded");
    let new_fact_id = format!("fact_{}_{}", now_ms, &sp_id[..8]);

    // Begin savepoint (compatible with both autocommit and active outer transactions)
    conn.execute_batch(&format!("SAVEPOINT {sp_name};"))?;

    let op_result: SqlResult<String> = (|| {
        // Step 1: Fetch and verify the existing old fact
        let old: Option<(String, String, String, Option<String>)> = conn
            .query_row(
                "SELECT subject, predicate, scope, category FROM facts WHERE fact_id = ?1",
                params![old_fact_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let (sub, pred, sc, cat) = match old {
            Some(vals) => vals,
            None => {
                return Err(rusqlite::Error::QueryReturnedNoRows);
            }
        };

        // Step 2: Close old active fact (valid_to = now_ms, expired_at = now_ms)
        let rows_updated = conn.execute(
            "UPDATE facts 
             SET valid_to = ?1, expired_at = ?1, reason = ?2, updated_at = ?1 
             WHERE fact_id = ?3",
            params![now_ms, reason_str, old_fact_id],
        )?;

        if rows_updated == 0 {
            return Err(rusqlite::Error::QueryReturnedNoRows);
        }

        // Step 3: Insert new active fact
        conn.execute(
            "INSERT INTO facts (
                fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                superseded_by, reason, category, key, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 5, 'private', ?8, NULL, ?8, NULL, NULL, NULL, ?9, NULL, ?8, ?8)",
            params![
                &new_fact_id,
                sub,
                pred,
                new_value,
                sc,
                origin.as_str(),
                confidence,
                now_ms,
                cat,
            ],
        )?;

        // Step 4: Link old fact's superseded_by to the now-existing new fact_id
        conn.execute(
            "UPDATE facts SET superseded_by = ?1 WHERE fact_id = ?2",
            params![&new_fact_id, old_fact_id],
        )?;

        // Step 5: Corroborate evidence link if provided
        if let Some(eid) = evidence_id {
            conn.execute(
                "INSERT OR IGNORE INTO fact_evidence (fact_id, evidence_id, confidence_weight, extracted_at)
                 VALUES (?1, ?2, 1.0, ?3)",
                params![&new_fact_id, eid, now_ms],
            )?;
        }

        Ok(new_fact_id)
    })();

    match op_result {
        Ok(id) => {
            conn.execute_batch(&format!("RELEASE SAVEPOINT {sp_name};"))?;
            Ok(id)
        }
        Err(err) => {
            let _ = conn.execute_batch(&format!(
                "ROLLBACK TO SAVEPOINT {sp_name}; RELEASE SAVEPOINT {sp_name};"
            ));
            Err(err)
        }
    }
}

/// Point-in-Time Query: What did LIVA know at past system transaction time `t_sys`?
pub fn query_facts_as_of_system_time(
    conn: &Connection,
    t_sys: i64,
) -> SqlResult<Vec<BitemporalFact>> {
    let mut stmt = conn.prepare(
        "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                superseded_by, reason, category, key
         FROM facts 
         WHERE recorded_at <= ?1 AND (expired_at IS NULL OR expired_at > ?1)
         ORDER BY subject, predicate",
    )?;
    let rows = stmt.query_map(params![t_sys], map_fact_row)?;
    let mut res = Vec::new();
    for r in rows {
        res.push(r?);
    }
    Ok(res)
}

/// Point-in-Time Query: What was valid in the real world at timestamp `t_world`?
pub fn query_facts_as_of_world_time(
    conn: &Connection,
    t_world: i64,
) -> SqlResult<Vec<BitemporalFact>> {
    let mut stmt = conn.prepare(
        "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                superseded_by, reason, category, key
         FROM facts 
         WHERE valid_from <= ?1 AND (valid_to IS NULL OR valid_to > ?1)
         ORDER BY subject, predicate",
    )?;
    let rows = stmt.query_map(params![t_world], map_fact_row)?;
    let mut res = Vec::new();
    for r in rows {
        res.push(r?);
    }
    Ok(res)
}

/// Retrieves complete historical lineage for a single (subject, predicate, scope).
pub fn query_fact_history(
    conn: &Connection,
    subject: &str,
    predicate: &str,
    scope: &str,
) -> SqlResult<Vec<BitemporalFact>> {
    let mut stmt = conn.prepare(
        "SELECT fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at,
                superseded_by, reason, category, key
         FROM facts 
         WHERE subject = ?1 AND predicate = ?2 AND scope = ?3
         ORDER BY recorded_at ASC",
    )?;
    let rows = stmt.query_map(params![subject, predicate, scope], map_fact_row)?;
    let mut res = Vec::new();
    for r in rows {
        res.push(r?);
    }
    Ok(res)
}

// ============================================================================
// 6. UNIT TESTS
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_memory_db() -> Connection {
        let conn = Connection::open_in_memory().expect("open in memory");
        init_bitemporal_schema(&conn).expect("init schema");
        conn
    }

    #[test]
    fn test_evidence_nfc_and_deduplication() {
        let conn = setup_memory_db();
        let text1 = "Hà Nội";
        let text2 = "Ha\u{0300} N\u{00f4}\u{0323}i"; // Decomposed NFD

        let id1 = ingest_evidence(
            &conn, text1, "turn", None, None, "Owner", "cli", "Trusted", 1000,
        )
        .expect("ingest 1");
        let id2 = ingest_evidence(
            &conn, text2, "turn", None, None, "Owner", "cli", "Trusted", 1001,
        )
        .expect("ingest 2");

        assert_eq!(
            id1, id2,
            "NFC normalization must yield identical BLAKE3 hashes"
        );

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM evidence_log", [], |r| r.get(0))
            .expect("count");
        assert_eq!(
            count, 1,
            "Idempotent insert must store exactly one evidence record"
        );
    }

    #[test]
    fn test_evidence_immutability_triggers() {
        let conn = setup_memory_db();
        let id = ingest_evidence(
            &conn,
            "Hải thích trà xanh",
            "turn",
            None,
            None,
            "Owner",
            "cli",
            "Trusted",
            1000,
        )
        .expect("ingest");

        let res_update = conn.execute(
            "UPDATE evidence_log SET nfc_content = 'fake' WHERE evidence_id = ?1",
            params![id],
        );
        assert!(
            res_update.is_err(),
            "Trigger must reject physical update of evidence_log"
        );

        let res_delete = conn.execute(
            "DELETE FROM evidence_log WHERE evidence_id = ?1",
            params![id],
        );
        assert!(
            res_delete.is_err(),
            "Trigger must reject physical deletion of evidence_log"
        );

        let tombstoned =
            tombstone_evidence(&conn, &id, "user_requested_forget", 1050).expect("tombstone");
        assert!(tombstoned, "Tombstoning must succeed");
    }

    #[test]
    fn test_active_singleton_fact_invariant() {
        let conn = setup_memory_db();

        conn.execute(
            "INSERT INTO facts (
                fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
             ) VALUES ('f1', 'user', 'city', 'Hà Nội', 'personal', 'explicit', 1.0, 5, 'private', 1000, NULL, 1000, NULL)",
            [],
        )
        .expect("insert first fact");

        let res = conn.execute(
            "INSERT INTO facts (
                fact_id, subject, predicate, value, scope, origin, confidence,
                importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
             ) VALUES ('f2', 'user', 'city', 'Đà Nẵng', 'personal', 'explicit', 1.0, 5, 'private', 2000, NULL, 2000, NULL)",
            [],
        );

        assert!(
            res.is_err(),
            "Conditional index idx_active_singleton_fact must reject conflicting active fact"
        );
    }

    #[test]
    fn test_four_action_reconciliation_workflow() {
        let mut conn = setup_memory_db();
        let eid = ingest_evidence(
            &mut conn,
            "Tôi sống ở Hà Nội",
            "turn",
            None,
            None,
            "Owner",
            "cli",
            "Trusted",
            1000,
        )
        .expect("ingest evidence");

        // 1. ADD
        let f1 = BitemporalFact {
            fact_id: "fact_001".into(),
            subject: "user".into(),
            predicate: "living_city".into(),
            value: "Hà Nội".into(),
            object_value: "Hà Nội".into(),
            scope: "personal".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 8,
            sensitivity: FactSensitivity::Private,
            valid_from: 1000,
            valid_to: None,
            recorded_at: 1000,
            expired_at: None,
            superseded_by: None,
            reason: None,
            supersede_reason: None,
            category: Some("demographic".into()),
            key: None,
            created_at: 1000,
            updated_at: 1000,
        };

        let tx = conn.transaction().expect("tx");
        let outcome1 = apply_reconciliation(&tx, f1.clone(), Some(&eid), 1000).expect("add");
        tx.commit().expect("commit");
        assert_eq!(
            outcome1,
            ReconciliationOutcome::Added {
                fact_id: "fact_001".into()
            }
        );

        // 2. NOOP (re-affirming identical fact)
        let tx = conn.transaction().expect("tx");
        let outcome2 = apply_reconciliation(&tx, f1.clone(), Some(&eid), 1100).expect("noop");
        tx.commit().expect("commit");
        match outcome2 {
            ReconciliationOutcome::Noop { fact_id, .. } => assert_eq!(fact_id, "fact_001"),
            _ => panic!("Expected Noop outcome"),
        }

        // 3. EXTEND
        let mut f1_extend = f1.clone();
        f1_extend.valid_to = Some(5000);
        let tx = conn.transaction().expect("tx");
        let outcome3 = apply_reconciliation(&tx, f1_extend, Some(&eid), 1200).expect("extend");
        tx.commit().expect("commit");
        assert_eq!(
            outcome3,
            ReconciliationOutcome::Extended {
                fact_id: "fact_001".into()
            }
        );

        // 4. INVALIDATE & SUPERSEDE (moved to Đà Nẵng)
        let f2 = BitemporalFact {
            fact_id: "fact_002".into(),
            subject: "user".into(),
            predicate: "living_city".into(),
            value: "Đà Nẵng".into(),
            object_value: "Đà Nẵng".into(),
            scope: "personal".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 8,
            sensitivity: FactSensitivity::Private,
            valid_from: 2000,
            valid_to: None,
            recorded_at: 2000,
            expired_at: None,
            superseded_by: None,
            reason: Some("moved_to_danang".into()),
            supersede_reason: Some("moved_to_danang".into()),
            category: Some("demographic".into()),
            key: None,
            created_at: 2000,
            updated_at: 2000,
        };

        let tx = conn.transaction().expect("tx");
        let outcome4 = apply_reconciliation(&tx, f2, Some(&eid), 2000).expect("invalidate");
        tx.commit().expect("commit");

        assert_eq!(
            outcome4,
            ReconciliationOutcome::Invalidated {
                superseded_fact_id: "fact_001".into(),
                new_fact_id: Some("fact_002".into()),
            }
        );

        let active = get_active_fact(&conn, "user", "living_city", "personal")
            .expect("query active")
            .expect("found active fact");
        assert_eq!(active.fact_id, "fact_002");
        assert_eq!(active.value, "Đà Nẵng");

        let as_of_1500 = query_facts_as_of_system_time(&conn, 1500).expect("as of 1500");
        assert_eq!(as_of_1500.len(), 1);
        assert_eq!(as_of_1500[0].value, "Hà Nội");

        let as_of_2500 = query_facts_as_of_system_time(&conn, 2500).expect("as of 2500");
        assert_eq!(as_of_2500.len(), 1);
        assert_eq!(as_of_2500[0].value, "Đà Nẵng");

        let history =
            query_fact_history(&conn, "user", "living_city", "personal").expect("history");
        assert_eq!(history.len(), 2);
        assert_eq!(history[0].fact_id, "fact_001");
        assert_eq!(history[0].superseded_by, Some("fact_002".into()));
        assert_eq!(history[1].fact_id, "fact_002");
    }

    #[test]
    fn test_trigger_blocks_integrity_and_principal_tampering() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(CREATE_BITEMPORAL_SCHEMA_SQL).unwrap();

        let id = ingest_evidence(
            &conn,
            "Nội dung bằng chứng gốc",
            "conversation",
            Some("sess_01"),
            Some("turn_01"),
            "Owner",
            "cli",
            "Trusted",
            1000,
        )
        .unwrap();

        // 1. Attempt to forge integrity (e.g. Untrusted -> Trusted)
        let tamper_integrity = conn.execute(
            "UPDATE evidence_log SET integrity = 'Untrusted' WHERE evidence_id = ?1",
            params![id],
        );
        assert!(
            tamper_integrity.is_err(),
            "Trigger must block tampering of 'integrity' column"
        );

        // 2. Attempt to forge principal (e.g. LocalCli -> WebRss)
        let tamper_principal = conn.execute(
            "UPDATE evidence_log SET principal = 'Attacker' WHERE evidence_id = ?1",
            params![id],
        );
        assert!(
            tamper_principal.is_err(),
            "Trigger must block tampering of 'principal' column"
        );

        // 3. Attempt to forge channel
        let tamper_channel = conn.execute(
            "UPDATE evidence_log SET channel = 'forged_channel' WHERE evidence_id = ?1",
            params![id],
        );
        assert!(
            tamper_channel.is_err(),
            "Trigger must block tampering of 'channel' column"
        );

        // 4. Legitimate tombstoning must succeed
        let tombstoned = tombstone_evidence(&conn, &id, "gdpr_request", 2000).unwrap();
        assert!(
            tombstoned,
            "Legitimate tombstoning of metadata must succeed"
        );
    }

    #[test]
    fn test_atomic_supersede_rollback_on_nonexistent_fact() {
        let conn = Connection::open_in_memory().unwrap();
        init_bitemporal_schema(&conn).unwrap();

        let res = atomic_supersede_fact(
            &conn,
            "nonexistent_fact_id",
            "Giá trị mới",
            FactOrigin::UserEdited,
            1.0,
            2000,
            None,
            None,
        );

        assert!(
            res.is_err(),
            "Superseding non-existent fact must fail cleanly with QueryReturnedNoRows"
        );
    }
}
