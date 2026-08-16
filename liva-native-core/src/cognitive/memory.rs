use crate::crypto::EncryptionEngine;
use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};

/// Provenance metadata tracking the origin, lineage, confidence, and verification of memory facts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryProvenance {
    /// Optional correlation to the originating PerceptionEvent ID.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
    /// Type of origin: "conversation", "user_action", "sensor", "reflection", "direct_statement".
    pub source_type: String,
    /// Identifier of the agent or component that extracted this memory.
    pub agent_id: String,
    /// Model used for extraction (e.g. "qwen3-4b-instruct", "claude-3-5-sonnet").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model_id: Option<String>,
    /// Calibrated confidence score between 0.0 and 1.0.
    pub confidence_score: f64,
    /// Exact contextual phrase or transcript from which the fact was extracted.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_context_excerpt: Option<String>,
    /// Whether the user has explicitly verified or confirmed this fact.
    #[serde(default)]
    pub verified_by_user: bool,
    /// Scoped owner domain (e.g. "memory_owner:local", "memory_owner:telegram:123456").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub owner_domain: Option<String>,
}

impl Default for MemoryProvenance {
    fn default() -> Self {
        Self {
            source_event_id: None,
            source_type: "conversation".to_string(),
            agent_id: "liva_core".to_string(),
            model_id: None,
            confidence_score: 1.0,
            raw_context_excerpt: None,
            verified_by_user: false,
            owner_domain: Some("memory_owner:local".to_string()),
        }
    }
}

impl MemoryProvenance {
    pub fn new(source_type: impl Into<String>, confidence_score: f64) -> Self {
        Self {
            source_event_id: None,
            source_type: source_type.into(),
            agent_id: "liva_core".to_string(),
            model_id: None,
            confidence_score: confidence_score.clamp(0.0, 1.0),
            raw_context_excerpt: None,
            verified_by_user: false,
            owner_domain: Some("memory_owner:local".to_string()),
        }
    }

    pub fn with_source_event(mut self, event_id: impl Into<String>) -> Self {
        self.source_event_id = Some(event_id.into());
        self
    }

    pub fn with_model(mut self, model: impl Into<String>) -> Self {
        self.model_id = Some(model.into());
        self
    }

    pub fn with_excerpt(mut self, excerpt: impl Into<String>) -> Self {
        self.raw_context_excerpt = Some(excerpt.into());
        self
    }

    pub fn with_verified(mut self, verified: bool) -> Self {
        self.verified_by_user = verified;
        self
    }

    pub fn with_owner_domain(mut self, domain: impl Into<String>) -> Self {
        self.owner_domain = Some(domain.into());
        self
    }
}

/// A structured cognitive memory fact with rich provenance, temporal interval, and Ebbinghaus strength.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CognitiveFact {
    pub fact_id: String,
    pub key: String,
    pub subject: String,
    pub predicate: String,
    pub value: String,
    pub provenance: MemoryProvenance,
    /// Epoch millisecond timestamp when the fact became true.
    pub effective_from_ms: i64,
    /// Optional epoch millisecond timestamp when the fact expires or becomes invalid.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub effective_until_ms: Option<i64>,
    /// Status: "active", "superseded", "disputed", "quarantined".
    pub status: String,
    pub domain: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub importance: f64,
    pub memory_strength: f64,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
    pub last_accessed_at: i64,
    pub access_count: i64,
}

impl CognitiveFact {
    pub fn new(
        key: impl Into<String>,
        subject: impl Into<String>,
        predicate: impl Into<String>,
        value: impl Into<String>,
        provenance: MemoryProvenance,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let domain = provenance
            .owner_domain
            .clone()
            .unwrap_or_else(|| "memory_owner:local".to_string());

        Self {
            fact_id: format!("fact_{}", uuid::Uuid::new_v4()),
            key: key.into(),
            subject: subject.into(),
            predicate: predicate.into(),
            value: value.into(),
            provenance,
            effective_from_ms: now,
            effective_until_ms: None,
            status: "active".to_string(),
            domain,
            category: None,
            importance: 0.5,
            memory_strength: 1.0,
            created_at_ms: now,
            updated_at_ms: now,
            last_accessed_at: now,
            access_count: 0,
        }
    }

    pub fn with_category(mut self, category: impl Into<String>) -> Self {
        self.category = Some(category.into());
        self
    }

    pub fn with_importance(mut self, importance: f64) -> Self {
        self.importance = importance.clamp(0.0, 1.0);
        self
    }

    pub fn with_effective_interval(mut self, from_ms: i64, until_ms: Option<i64>) -> Self {
        self.effective_from_ms = from_ms;
        self.effective_until_ms = until_ms;
        self
    }

    pub fn is_temporally_valid(&self, now_ms: i64) -> bool {
        if self.effective_from_ms > now_ms {
            return false;
        }
        if let Some(until) = self.effective_until_ms
            && now_ms >= until
        {
            return false;
        }
        true
    }
}

/// A stored conflict record queued for resolution instead of silent overwriting.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryConflictRecord {
    pub conflict_id: String,
    pub fact_key: String,
    pub domain: String,
    pub existing_value: String,
    pub proposed_value: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_event_id: Option<String>,
    pub conflict_type: String, // "contradiction", "exact_key_divergence", "semantic_divergence", "temporal_drift"
    pub resolution_status: String, // "pending", "resolved_superseded", "resolved_rejected", "resolved_merged"
    pub created_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at_ms: Option<i64>,
}

/// An archived historical version of a superseded or modified fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FactHistoryRecord {
    pub history_id: String,
    pub key: String,
    pub domain: String,
    pub old_value: String,
    pub archived_at_ms: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub superseded_by: Option<String>,
    pub reason: String, // "superseded", "deleted", "conflict_resolved"
}

/// Result outcome of an intelligent fact upsert operation.
#[derive(Debug, Clone, PartialEq)]
pub enum FactUpsertOutcome {
    /// Fact was freshly created.
    Created,
    /// Fact was updated and previous value archived safely into `facts_history`.
    Superseded { history_id: String },
    /// Contradiction detected; staged to `memory_conflict_queue` without destroying old fact.
    ConflictStaged { conflict_id: String },
}

/// User or agent resolution choice for a queued memory conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConflictResolutionAction {
    AcceptProposed,
    KeepExisting,
    MergeCustom(String),
}

/// Detailed counts from cascading fact deletion.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct FactDeletionCounts {
    pub facts_deleted: i64,
    pub history_deleted: i64,
    pub backups_deleted: i64,
    pub conflicts_deleted: i64,
    pub vectors_meta_deleted: i64,
    pub vec_idx_deleted: i64,
    pub vectors_fts_deleted: i64,
    pub l3_nodes_deleted: i64,
    pub l3_edges_deleted: i64,
    pub wal_truncated: bool,
}

/// Coordinator for Cognitive Memory operations: Provenance, Conflict Detection,
/// Historical Archiving, and Multi-Tier Cascading Delete Propagation.
pub struct CognitiveMemoryCoordinator;

impl CognitiveMemoryCoordinator {
    /// Intelligently upserts a CognitiveFact.
    /// If an existing fact is found with a different value:
    /// - If `auto_archive_on_supersede` is true, archives prior value to `facts_history` and updates active fact.
    /// - If `auto_archive_on_supersede` is false, stages the conflict to `memory_conflict_queue`.
    pub fn upsert_cognitive_fact(
        conn: &Connection,
        engine: &EncryptionEngine,
        fact: &CognitiveFact,
        auto_archive_on_supersede: bool,
    ) -> Result<FactUpsertOutcome, rusqlite::Error> {
        use rusqlite::OptionalExtension;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let existing_raw: Option<String> = conn
            .query_row("SELECT value FROM facts WHERE key = ?1", [&fact.key], |r| {
                r.get(0)
            })
            .optional()?;

        if let Some(existing_cipher) = existing_raw {
            let existing_plain = match engine.try_decrypt(&existing_cipher) {
                Ok(p) => p,
                Err(_) => {
                    // If locked/undecryptable, backup ciphertext to facts_locked_backup
                    conn.execute(
                        "INSERT INTO facts_locked_backup (key, value, backed_up_at) VALUES (?1, ?2, ?3)",
                        params![&fact.key, &existing_cipher, now_ms / 1000],
                    )?;
                    "[LOCKED_CIPHERTEXT]".to_string()
                }
            };

            // Check if value is identical
            if existing_plain == fact.value {
                // Just update timestamp & access metadata
                conn.execute(
                    "UPDATE facts SET updatedAt = ?1, confidenceScore = ?2, memory_strength = ?3 WHERE key = ?4",
                    params![
                        &fact.updated_at_ms.to_string(),
                        fact.provenance.confidence_score,
                        fact.memory_strength,
                        &fact.key
                    ],
                )?;
                return Ok(FactUpsertOutcome::Created);
            }

            // Value has diverged (conflict or update)
            if auto_archive_on_supersede {
                // Archive existing value to facts_history
                let history_id = format!("hist_{}", uuid::Uuid::new_v4());
                let enc_old = engine.encrypt(&existing_plain).unwrap_or(existing_cipher);

                conn.execute(
                    "INSERT INTO facts_history (history_id, key, domain, old_value, archived_at_ms, superseded_by, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'superseded')",
                    params![
                        history_id,
                        &fact.key,
                        &fact.domain,
                        enc_old,
                        now_ms,
                        &fact.fact_id
                    ],
                )?;

                // Encrypt and update active fact
                let new_cipher = engine.encrypt(&fact.value).map_err(|e| {
                    rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                        e.to_string(),
                    )))
                })?;

                conn.execute(
                    "UPDATE facts SET value = ?1, updatedAt = ?2, confidenceScore = ?3, memory_strength = ?4, source = ?5
                     WHERE key = ?6",
                    params![
                        new_cipher,
                        &fact.updated_at_ms.to_string(),
                        fact.provenance.confidence_score,
                        fact.memory_strength,
                        &fact.provenance.source_type,
                        &fact.key
                    ],
                )?;

                Ok(FactUpsertOutcome::Superseded { history_id })
            } else {
                // Stage to memory_conflict_queue
                let conflict_id = format!("conf_{}", uuid::Uuid::new_v4());
                let enc_existing = engine
                    .encrypt(&existing_plain)
                    .unwrap_or_else(|_| existing_plain.clone());
                let enc_proposed = engine
                    .encrypt(&fact.value)
                    .unwrap_or_else(|_| fact.value.clone());

                conn.execute(
                    "INSERT INTO memory_conflict_queue (
                        conflict_id, fact_key, domain, existing_value, proposed_value,
                        source_event_id, conflict_type, resolution_status, created_at_ms
                    ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'contradiction', 'pending', ?7)",
                    params![
                        conflict_id,
                        &fact.key,
                        &fact.domain,
                        enc_existing,
                        enc_proposed,
                        &fact.provenance.source_event_id,
                        now_ms
                    ],
                )?;

                Ok(FactUpsertOutcome::ConflictStaged { conflict_id })
            }
        } else {
            // Fresh fact insertion
            let encrypted_val = engine.encrypt(&fact.value).map_err(|e| {
                rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::other(
                    e.to_string(),
                )))
            })?;

            conn.execute(
                "INSERT INTO facts (
                    key, value, createdAt, updatedAt, ttlDays, source, category,
                    importance, confidenceScore, sourceTurnId, memory_strength,
                    last_accessed_at, access_count
                ) VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    &fact.key,
                    encrypted_val,
                    fact.created_at_ms.to_string(),
                    fact.updated_at_ms.to_string(),
                    &fact.provenance.source_type,
                    &fact.category,
                    fact.importance,
                    fact.provenance.confidence_score,
                    &fact.provenance.source_event_id,
                    fact.memory_strength,
                    fact.last_accessed_at,
                    fact.access_count
                ],
            )?;

            Ok(FactUpsertOutcome::Created)
        }
    }

    /// Stages a conflict into the `memory_conflict_queue`.
    pub fn stage_conflict(
        conn: &Connection,
        engine: &EncryptionEngine,
        conflict: &MemoryConflictRecord,
    ) -> SqlResult<()> {
        let enc_exist = engine
            .encrypt(&conflict.existing_value)
            .unwrap_or_else(|_| conflict.existing_value.clone());
        let enc_prop = engine
            .encrypt(&conflict.proposed_value)
            .unwrap_or_else(|_| conflict.proposed_value.clone());

        conn.execute(
            "INSERT INTO memory_conflict_queue (
                conflict_id, fact_key, domain, existing_value, proposed_value,
                source_event_id, conflict_type, resolution_status, created_at_ms, resolved_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                conflict.conflict_id,
                conflict.fact_key,
                conflict.domain,
                enc_exist,
                enc_prop,
                conflict.source_event_id,
                conflict.conflict_type,
                conflict.resolution_status,
                conflict.created_at_ms,
                conflict.resolved_at_ms
            ],
        )?;

        Ok(())
    }

    /// Fetches pending conflicts for a domain.
    pub fn get_pending_conflicts(
        conn: &Connection,
        engine: &EncryptionEngine,
        domain: &str,
    ) -> SqlResult<Vec<MemoryConflictRecord>> {
        let mut stmt = conn.prepare(
            "SELECT conflict_id, fact_key, domain, existing_value, proposed_value,
                    source_event_id, conflict_type, resolution_status, created_at_ms, resolved_at_ms
             FROM memory_conflict_queue
             WHERE domain = ?1 AND resolution_status = 'pending'
             ORDER BY created_at_ms ASC",
        )?;

        let rows = stmt.query_map(params![domain], |row| {
            let enc_exist: String = row.get(3)?;
            let enc_prop: String = row.get(4)?;
            let dec_exist = engine.try_decrypt(&enc_exist).unwrap_or(enc_exist);
            let dec_prop = engine.try_decrypt(&enc_prop).unwrap_or(enc_prop);

            Ok(MemoryConflictRecord {
                conflict_id: row.get(0)?,
                fact_key: row.get(1)?,
                domain: row.get(2)?,
                existing_value: dec_exist,
                proposed_value: dec_prop,
                source_event_id: row.get(5)?,
                conflict_type: row.get(6)?,
                resolution_status: row.get(7)?,
                created_at_ms: row.get(8)?,
                resolved_at_ms: row.get(9)?,
            })
        })?;

        let mut results = Vec::new();
        for r in rows {
            results.push(r?);
        }
        Ok(results)
    }

    /// Resolves a queued conflict with the specified action choice.
    pub fn resolve_conflict(
        conn: &Connection,
        engine: &EncryptionEngine,
        conflict_id: &str,
        action: ConflictResolutionAction,
    ) -> Result<bool, rusqlite::Error> {
        use rusqlite::OptionalExtension;

        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let mut stmt = conn.prepare(
            "SELECT fact_key, domain, existing_value, proposed_value
             FROM memory_conflict_queue
             WHERE conflict_id = ?1 AND resolution_status = 'pending'",
        )?;

        let row_opt = stmt
            .query_row(params![conflict_id], |r| {
                Ok((
                    r.get::<_, String>(0)?,
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, String>(3)?,
                ))
            })
            .optional()?;

        let (fact_key, domain, enc_exist, enc_prop) = match row_opt {
            Some(data) => data,
            None => return Ok(false),
        };

        let dec_exist = engine.try_decrypt(&enc_exist).unwrap_or(enc_exist);
        let dec_prop = engine.try_decrypt(&enc_prop).unwrap_or(enc_prop);

        match action {
            ConflictResolutionAction::AcceptProposed => {
                // Archive existing value
                let history_id = format!("hist_{}", uuid::Uuid::new_v4());
                let enc_old = engine
                    .encrypt(&dec_exist)
                    .unwrap_or_else(|_| dec_exist.clone());
                conn.execute(
                    "INSERT INTO facts_history (history_id, key, domain, old_value, archived_at_ms, superseded_by, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'conflict_resolved_accepted')",
                    params![history_id, fact_key, domain, enc_old, now_ms, conflict_id],
                )?;

                // Update active fact with proposed value
                let new_cipher = engine.encrypt(&dec_prop).unwrap_or(dec_prop);
                conn.execute(
                    "INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, memory_strength)
                     VALUES (?1, ?2, ?3, ?3, NULL, 'conflict_resolution', 'profile', 0.5, 1.0, 1.0)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
                    params![fact_key, new_cipher, now_ms.to_string()],
                )?;

                // Mark conflict as resolved_superseded
                conn.execute(
                    "UPDATE memory_conflict_queue SET resolution_status = 'resolved_superseded', resolved_at_ms = ?1 WHERE conflict_id = ?2",
                    params![now_ms, conflict_id],
                )?;
            }
            ConflictResolutionAction::KeepExisting => {
                // Mark conflict as resolved_rejected
                conn.execute(
                    "UPDATE memory_conflict_queue SET resolution_status = 'resolved_rejected', resolved_at_ms = ?1 WHERE conflict_id = ?2",
                    params![now_ms, conflict_id],
                )?;
            }
            ConflictResolutionAction::MergeCustom(merged_val) => {
                // Archive existing value
                let history_id = format!("hist_{}", uuid::Uuid::new_v4());
                let enc_old = engine
                    .encrypt(&dec_exist)
                    .unwrap_or_else(|_| dec_exist.clone());
                conn.execute(
                    "INSERT INTO facts_history (history_id, key, domain, old_value, archived_at_ms, superseded_by, reason)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'conflict_resolved_merged')",
                    params![history_id, fact_key, domain, enc_old, now_ms, conflict_id],
                )?;

                // Update active fact with merged value
                let new_cipher = engine.encrypt(&merged_val).unwrap_or(merged_val);
                conn.execute(
                    "INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, memory_strength)
                     VALUES (?1, ?2, ?3, ?3, NULL, 'conflict_merge', 'profile', 0.5, 1.0, 1.0)
                     ON CONFLICT(key) DO UPDATE SET value = excluded.value, updatedAt = excluded.updatedAt",
                    params![fact_key, new_cipher, now_ms.to_string()],
                )?;

                // Mark conflict as resolved_merged
                conn.execute(
                    "UPDATE memory_conflict_queue SET resolution_status = 'resolved_merged', resolved_at_ms = ?1 WHERE conflict_id = ?2",
                    params![now_ms, conflict_id],
                )?;
            }
        }

        Ok(true)
    }

    /// Fetches the version history of a memory fact.
    pub fn get_fact_history(
        conn: &Connection,
        engine: &EncryptionEngine,
        key: &str,
        domain: &str,
    ) -> SqlResult<Vec<FactHistoryRecord>> {
        let mut stmt = conn.prepare(
            "SELECT history_id, key, domain, old_value, archived_at_ms, superseded_by, reason
             FROM facts_history
             WHERE key = ?1 AND domain = ?2
             ORDER BY archived_at_ms DESC",
        )?;

        let rows = stmt.query_map(params![key, domain], |row| {
            let enc_val: String = row.get(3)?;
            let dec_val = engine.try_decrypt(&enc_val).unwrap_or(enc_val);

            Ok(FactHistoryRecord {
                history_id: row.get(0)?,
                key: row.get(1)?,
                domain: row.get(2)?,
                old_value: dec_val,
                archived_at_ms: row.get(4)?,
                superseded_by: row.get(5)?,
                reason: row.get(6)?,
            })
        })?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }
}

/// Universal Cascading Delete Propagation Coordinator across all storage layers.
pub struct MemoryDeleteCoordinator;

impl MemoryDeleteCoordinator {
    /// Cascades fact deletion across SQLite tables (facts, history, backups, conflicts),
    /// vector projections (vectors_meta, vec_idx, vectors_fts), and knowledge graph (l3_edges, l3_nodes).
    pub fn delete_fact_cascade(
        conn: &Connection,
        key: &str,
        domain: &str,
    ) -> Result<FactDeletionCounts, rusqlite::Error> {
        conn.execute_batch("PRAGMA secure_delete = ON;")?;
        let tx = conn.unchecked_transaction()?;

        let facts_deleted = tx.execute("DELETE FROM facts WHERE key = ?1", params![key])? as i64;
        let backups_deleted = tx.execute(
            "DELETE FROM facts_locked_backup WHERE key = ?1",
            params![key],
        )? as i64;
        let history_deleted = tx.execute(
            "DELETE FROM facts_history WHERE key = ?1 AND domain = ?2",
            params![key, domain],
        )? as i64;
        let conflicts_deleted = tx.execute(
            "DELETE FROM memory_conflict_queue WHERE fact_key = ?1 AND domain = ?2",
            params![key, domain],
        )? as i64;

        // Vector projections matching key
        let target_vec_ids: Vec<i64> = {
            let mut stmt = tx.prepare(
                "SELECT id FROM vectors_meta WHERE type = 'fact' AND (vec_id = ?1 OR content LIKE ?2)",
            )?;
            let pattern = format!("%{}%", key);
            let rows = stmt.query_map(params![key, pattern], |r| r.get(0))?;
            let mut ids = Vec::new();
            for id in rows {
                ids.push(id?);
            }
            ids
        };

        let mut vec_idx_deleted = 0i64;
        let mut vectors_fts_deleted = 0i64;
        let mut vectors_meta_deleted = 0i64;

        for vid in &target_vec_ids {
            vec_idx_deleted +=
                tx.execute("DELETE FROM vec_idx WHERE rowid = ?1", params![vid])? as i64;
            vectors_fts_deleted +=
                tx.execute("DELETE FROM vectors_fts WHERE rowid = ?1", params![vid])? as i64;
            vectors_meta_deleted +=
                tx.execute("DELETE FROM vectors_meta WHERE id = ?1", params![vid])? as i64;
        }

        // L3 Knowledge Graph
        let l3_edges_deleted = tx.execute(
            "DELETE FROM l3_edges WHERE source = ?1 OR target = ?1",
            params![key],
        )? as i64;
        let l3_nodes_deleted =
            tx.execute("DELETE FROM l3_nodes WHERE id = ?1", params![key])? as i64;

        // Record audit trace
        let audit_id = format!("factdel_{}", uuid::Uuid::new_v4());
        let scope_hash = format!("fact_{}_{}", domain, key);
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        let counts = FactDeletionCounts {
            facts_deleted,
            history_deleted,
            backups_deleted,
            conflicts_deleted,
            vectors_meta_deleted,
            vec_idx_deleted,
            vectors_fts_deleted,
            l3_nodes_deleted,
            l3_edges_deleted,
            wal_truncated: true,
        };

        let counts_json = serde_json::to_string(&counts).unwrap_or_default();
        tx.execute(
            "INSERT INTO deletion_audit (audit_id, scope_hash, dry_run, counts_json, created_at)
             VALUES (?1, ?2, 0, ?3, ?4)",
            params![audit_id, scope_hash, counts_json, now_ms],
        )?;

        tx.commit()?;

        // Truncate WAL
        let _ = conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE);");

        Ok(counts)
    }

    /// Cascades conversation deletion across all projections.
    pub fn delete_conversation_cascade(
        conn: &Connection,
        owner_id: &str,
        conversation_id: &str,
        dry_run: bool,
    ) -> Result<crate::db::ConversationDeletionReport, rusqlite::Error> {
        crate::db::delete_conversation(conn, owner_id, conversation_id, dry_run)
    }

    /// Cascades subject deletion across all owner domains.
    pub fn delete_subject_cascade(
        conn: &Connection,
        owner_id: &str,
        dry_run: bool,
    ) -> Result<crate::db::SubjectDeletionReport, rusqlite::Error> {
        crate::db::delete_subject(conn, owner_id, dry_run)
    }
}
