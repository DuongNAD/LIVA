//! Semantic Memory Consolidation & Knowledge Graph Extraction Engine (Milestone 3).
//!
//! Upgrades event projection validation to extract semantic knowledge facts,
//! user preferences, and entity relationships from pending conversation events,
//! encrypting facts with HKDF-SHA256 + AES-256-GCM v2 and linking them into
//! the L3 Knowledge Graph (`facts`, `l3_nodes`, `l3_edges`).

use crate::crypto::{EncryptionEngine, FactRead};
use crate::db::{DatabasePool, Fact};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::{Deserialize, Serialize};

const MAX_BATCH_SIZE: usize = 100;
const MAX_RETRIES: i64 = 3;
const PROJECTION_WORKER_ID: &str = "event-projection-v1";
const DEFAULT_BATCH_SIZE: usize = 25;
const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ConsolidationBatchResult {
    pub processed: usize,
    pub consolidated: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub facts_extracted: usize,
    pub nodes_created: usize,
    pub edges_created: usize,
}

#[derive(Debug, Default, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub struct ConsolidationSummary {
    pub events_processed: usize,
    pub facts_extracted: usize,
    pub nodes_created: usize,
    pub edges_created: usize,
}

#[derive(Debug, Clone)]
pub struct PendingEvent {
    pub event_id: String,
    pub retry_count: i64,
    pub domain: String,
    pub category: String,
    pub raw_content: Option<String>,
}

/// Core Memory Consolidator implementing semantic knowledge extraction and persistence.
pub struct MemoryConsolidator;

impl MemoryConsolidator {
    /// Consolidates a slice of pending events, extracting semantic facts and updating knowledge graph.
    pub fn extract_and_consolidate(
        events: &[PendingEvent],
        conn: &Connection,
        crypto: &EncryptionEngine,
    ) -> Result<ConsolidationSummary, String> {
        let mut summary = ConsolidationSummary::default();

        for event in events {
            // Retrieve conversation content from vectors_meta or fallback to raw content
            let content = match get_event_content(conn, crypto, &event.event_id) {
                Ok(Some(c)) => c,
                _ => event.raw_content.clone().unwrap_or_default(),
            };

            if content.trim().is_empty() {
                continue;
            }

            // Extract facts & knowledge relations
            let (facts, nodes, edges) = extract_knowledge_from_text(&content, &event.event_id);

            // Persist encrypted facts into facts table
            for fact in &facts {
                if let Err(e) = crate::db::set_fact(conn, crypto, fact) {
                    tracing::warn!("Failed to persist encrypted fact '{}': {}", fact.key, e);
                } else {
                    summary.facts_extracted += 1;
                }
            }

            // Persist knowledge nodes
            for (id, label, properties) in &nodes {
                let props_json = serde_json::to_string(properties).unwrap_or_else(|_| "{}".to_string());
                let _ = conn.execute(
                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?2, ?3)
                     ON CONFLICT(id) DO UPDATE SET label = excluded.label, properties = excluded.properties",
                    params![id, label, props_json],
                );
                summary.nodes_created += 1;
            }

            // Persist knowledge edges
            for (source, target, relation, weight) in &edges {
                let _ = conn.execute(
                    "INSERT INTO l3_edges (source, target, relation, weight, obsolete) VALUES (?1, ?2, ?3, ?4, 0)
                     ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight",
                    params![source, target, relation, weight],
                );
                summary.edges_created += 1;
            }

            summary.events_processed += 1;
        }

        Ok(summary)
    }

    /// Convenience runner for database pool.
    pub async fn extract_and_consolidate_pool(
        db: DatabasePool,
        crypto: EncryptionEngine,
        batch_size: usize,
    ) -> Result<ConsolidationSummary, String> {
        tokio::task::spawn_blocking(move || {
            let conn = db
                .writer
                .get()
                .map_err(|e| format!("Cannot acquire DB writer: {e}"))?;
            let events = fetch_pending_events(&conn, batch_size)
                .map_err(|e| format!("Failed to fetch pending events: {e}"))?;
            Self::extract_and_consolidate(&events, &conn, &crypto)
        })
        .await
        .map_err(|e| format!("Consolidation worker panicked: {e}"))?
    }
}

/// Extracts structured facts, nodes, and edges from conversation content.
pub fn extract_knowledge_from_text(
    text: &str,
    source_turn_id: &str,
) -> (
    Vec<Fact>,
    Vec<(String, String, serde_json::Value)>,
    Vec<(String, String, String, f64)>,
) {
    let mut facts = Vec::new();
    let mut nodes = Vec::new();
    let mut edges = Vec::new();
    let now = chrono::Utc::now().to_rfc3339();

    let lines: Vec<&str> = text.lines().collect();
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let lower = trimmed.to_lowercase();

        // 1. User Name / Identity
        if lower.contains("tôi tên là") || lower.contains("my name is") || lower.contains("mình tên là") {
            let raw_name = if let Some(idx) = lower.find("tôi tên là") {
                &trimmed[idx + "tôi tên là".len()..]
            } else if let Some(idx) = lower.find("my name is") {
                &trimmed[idx + "my name is".len()..]
            } else if let Some(idx) = lower.find("mình tên là") {
                &trimmed[idx + "mình tên là".len()..]
            } else {
                ""
            };
            let name = raw_name
                .split(|c: char| c == '.' || c == ',' || c == '!' || c == ':' || c == ';')
                .next()
                .unwrap_or("")
                .split(" và ")
                .next()
                .unwrap_or("")
                .split(" and ")
                .next()
                .unwrap_or("")
                .trim();

            if !name.is_empty() {
                facts.push(Fact {
                    key: "user:name".to_string(),
                    value: name.to_string(),
                    createdAt: now.clone(),
                    updatedAt: now.clone(),
                    ttlDays: Some(365),
                    source: "semantic_consolidation".to_string(),
                    category: Some("identity".to_string()),
                    importance: 0.9,
                    confidenceScore: 0.95,
                    sourceTurnId: Some(source_turn_id.to_string()),
                    memory_strength: 1.0,
                    last_accessed_at: 0,
                    access_count: 0,
                });
                nodes.push(("node:user".to_string(), "User".to_string(), serde_json::json!({"name": name})));
            }
        }

        // 2. Preferences (e.g. temperature, favorite items)
        if lower.contains("thích") || lower.contains("prefer") || lower.contains("yêu thích") {
            let fact_key = format!("pref:{}", uuid::Uuid::new_v4().simple());
            facts.push(Fact {
                key: fact_key.clone(),
                value: trimmed.to_string(),
                createdAt: now.clone(),
                updatedAt: now.clone(),
                ttlDays: Some(180),
                source: "semantic_consolidation".to_string(),
                category: Some("preference".to_string()),
                importance: 0.75,
                confidenceScore: 0.85,
                sourceTurnId: Some(source_turn_id.to_string()),
                memory_strength: 1.0,
                last_accessed_at: 0,
                access_count: 0,
            });
            nodes.push((format!("node:{}", fact_key), "Preference".to_string(), serde_json::json!({"content": trimmed})));
            edges.push(("node:user".to_string(), format!("node:{}", fact_key), "HAS_PREFERENCE".to_string(), 1.0));
        }

        // 3. Project / Work codes (e.g. ORION-7, LIVA, secret keys)
        if lower.contains("dự án") || lower.contains("project") || lower.contains("mã") || lower.contains("code") {
            let fact_key = format!("project:{}", uuid::Uuid::new_v4().simple());
            facts.push(Fact {
                key: fact_key.clone(),
                value: trimmed.to_string(),
                createdAt: now.clone(),
                updatedAt: now.clone(),
                ttlDays: Some(365),
                source: "semantic_consolidation".to_string(),
                category: Some("project".to_string()),
                importance: 0.85,
                confidenceScore: 0.9,
                sourceTurnId: Some(source_turn_id.to_string()),
                memory_strength: 1.0,
                last_accessed_at: 0,
                access_count: 0,
            });
            nodes.push((format!("node:{}", fact_key), "ProjectKnowledge".to_string(), serde_json::json!({"info": trimmed})));
            edges.push(("node:user".to_string(), format!("node:{}", fact_key), "WORKS_ON".to_string(), 0.9));
        }
    }

    (facts, nodes, edges)
}

fn get_event_content(
    conn: &Connection,
    crypto: &EncryptionEngine,
    event_id: &str,
) -> Result<Option<String>, rusqlite::Error> {
    let res: Option<String> = conn
        .query_row(
            "SELECT content FROM vectors_meta WHERE vec_id = ?1",
            [event_id],
            |row| row.get(0),
        )
        .optional()?;

    let Some(raw) = res else {
        return Ok(None);
    };

    match crypto.read_fact(&raw) {
        FactRead::Ok(plain) => Ok(Some(plain)),
        FactRead::Locked { .. } => Ok(Some(raw)),
    }
}

pub fn fetch_pending_events(
    conn: &Connection,
    batch_size: usize,
) -> Result<Vec<PendingEvent>, rusqlite::Error> {
    let batch_size = batch_size.clamp(1, MAX_BATCH_SIZE) as i64;
    let mut stmt = conn.prepare(
        "SELECT eventId, retry_count, domain, category, rawUserMsg FROM events \
         WHERE consolidation_status = 'pending' \
         ORDER BY timestamp, eventId \
         LIMIT ?1",
    )?;

    let events = stmt
        .query_map([batch_size], |row| {
            Ok(PendingEvent {
                event_id: row.get(0)?,
                retry_count: row.get(1)?,
                domain: row.get(2)?,
                category: row.get(3)?,
                raw_content: row.get::<_, Option<String>>(4)?,
            })
        })?
        .collect::<Result<Vec<_>, _>>()?;

    Ok(events)
}

pub async fn consume_pending_once(
    db: crate::db::DatabasePool,
    crypto: crate::crypto::EncryptionEngine,
    batch_size: usize,
) -> Result<ConsolidationBatchResult, String> {
    tokio::task::spawn_blocking(move || {
        let conn = db
            .writer
            .get()
            .map_err(|error| format!("khong lay duoc DB writer: {error}"))?;
        process_pending_batch(&conn, &crypto, PROJECTION_WORKER_ID, batch_size)
            .map_err(|error| format!("event projection consumer loi: {error}"))
    })
    .await
    .map_err(|error| format!("event projection worker panic: {error}"))?
}

pub fn spawn_projection_consumer(
    db: crate::db::DatabasePool,
    crypto: crate::crypto::EncryptionEngine,
) -> tokio::task::JoinHandle<()> {
    tokio::spawn(run_default_projection_consumer(db, crypto))
}

pub async fn run_default_projection_consumer(
    db: crate::db::DatabasePool,
    crypto: crate::crypto::EncryptionEngine,
) {
    run_projection_consumer(db, crypto, DEFAULT_INTERVAL, DEFAULT_BATCH_SIZE).await;
}

pub async fn run_projection_consumer(
    db: crate::db::DatabasePool,
    crypto: crate::crypto::EncryptionEngine,
    interval_duration: std::time::Duration,
    batch_size: usize,
) {
    let mut interval = tokio::time::interval(interval_duration);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    loop {
        interval.tick().await;
        match consume_pending_once(db.clone(), crypto.clone(), batch_size).await {
            Ok(result) if result.processed > 0 => {
                tracing::info!(
                    processed = result.processed,
                    consolidated = result.consolidated,
                    retried = result.retried,
                    dead_lettered = result.dead_lettered,
                    facts_extracted = result.facts_extracted,
                    nodes_created = result.nodes_created,
                    edges_created = result.edges_created,
                    "event projection and semantic consolidation batch completed"
                );
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, "event projection batch failed");
            }
        }
    }
}

pub fn process_pending_batch(
    conn: &Connection,
    crypto: &EncryptionEngine,
    worker_id: &str,
    batch_size: usize,
) -> Result<ConsolidationBatchResult, rusqlite::Error> {
    let transaction = Transaction::new_unchecked(conn, TransactionBehavior::Immediate)?;
    let batch_size_clamped = batch_size.clamp(1, MAX_BATCH_SIZE) as i64;
    let events = {
        let mut statement = transaction.prepare(
            "SELECT eventId, retry_count, domain, category, rawUserMsg FROM events \
             WHERE consolidation_status = 'pending' \
             ORDER BY timestamp, eventId \
             LIMIT ?1",
        )?;
        statement
            .query_map([batch_size_clamped], |row| {
                Ok(PendingEvent {
                    event_id: row.get(0)?,
                    retry_count: row.get(1)?,
                    domain: row.get(2)?,
                    category: row.get(3)?,
                    raw_content: row.get::<_, Option<String>>(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut result = ConsolidationBatchResult::default();

    for event in &events {
        if projection_matches(&transaction, event)? {
            // Extract semantic facts on verified events
            let content = get_event_content(&transaction, crypto, &event.event_id)?
                .or_else(|| event.raw_content.clone())
                .unwrap_or_default();

            if !content.trim().is_empty() {
                let (facts, nodes, edges) = extract_knowledge_from_text(&content, &event.event_id);
                for fact in facts {
                    if let Ok(()) = crate::db::set_fact_in_tx(&transaction, crypto, &fact) {
                        result.facts_extracted += 1;
                    }
                }
                for (id, label, props) in nodes {
                    let props_json = serde_json::to_string(&props).unwrap_or_else(|_| "{}".to_string());
                    let _ = transaction.execute(
                        "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?2, ?3)
                         ON CONFLICT(id) DO UPDATE SET label = excluded.label, properties = excluded.properties",
                        params![id, label, props_json],
                    );
                    result.nodes_created += 1;
                }
                for (source, target, rel, weight) in edges {
                    let _ = transaction.execute(
                        "INSERT INTO l3_edges (source, target, relation, weight, obsolete) VALUES (?1, ?2, ?3, ?4, 0)
                         ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight",
                        params![source, target, rel, weight],
                    );
                    result.edges_created += 1;
                }
            }

            let updated = transaction.execute(
                "UPDATE events \
                 SET consolidated = 1, consolidation_status = 'consolidated' \
                 WHERE eventId = ?1 AND consolidation_status = 'pending'",
                [&event.event_id],
            )?;
            result.processed += updated;
            result.consolidated += updated;
            continue;
        }

        let retry_count = event.retry_count + 1;
        if retry_count >= MAX_RETRIES {
            let updated = transaction.execute(
                "UPDATE events \
                 SET consolidated = 0, consolidation_status = 'dlq', retry_count = ?2 \
                 WHERE eventId = ?1 AND consolidation_status = 'pending'",
                params![event.event_id, retry_count],
            )?;
            if updated > 0 {
                transaction.execute(
                    "INSERT INTO dlq_consolidation (
                        session_id, failed_step, error_msg, retry_count, status, created_at
                     ) VALUES (
                        ?1, 'validate_projection', 'missing_or_invalid_vector_projection',
                        ?2, 'pending', ?3
                     )",
                    params![event.event_id, retry_count, unix_time_millis()],
                )?;
                result.dead_lettered += 1;
            }
            result.processed += updated;
        } else {
            let updated = transaction.execute(
                "UPDATE events SET retry_count = ?2 \
                 WHERE eventId = ?1 AND consolidation_status = 'pending'",
                params![event.event_id, retry_count],
            )?;
            result.processed += updated;
            result.retried += updated;
        }
    }

    if let Some(last_event) = events.last() {
        let now = unix_time_millis();
        let state_data = serde_json::json!({
            "last_event_id": last_event.event_id,
            "processed": result.processed,
            "consolidated": result.consolidated,
            "retried": result.retried,
            "dead_lettered": result.dead_lettered,
            "facts_extracted": result.facts_extracted,
        })
        .to_string();
        transaction.execute(
            "INSERT INTO consolidation_checkpoints (
                session_id, last_step, state_data, created_at, updated_at
             ) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT(session_id) DO UPDATE SET
                last_step = consolidation_checkpoints.last_step + excluded.last_step,
                state_data = excluded.state_data,
                updated_at = excluded.updated_at",
            params![worker_id, result.processed as i64, state_data, now],
        )?;
    }

    transaction.commit()?;
    Ok(result)
}

fn projection_matches(conn: &Connection, event: &PendingEvent) -> Result<bool, rusqlite::Error> {
    let projection = conn
        .query_row(
            "SELECT type, domain, category, source_event_ids \
             FROM vectors_meta WHERE vec_id = ?1",
            [&event.event_id],
            |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                    row.get::<_, String>(3)?,
                ))
            },
        )
        .optional()?;
    let Some((memory_type, domain, category, source_event_ids)) = projection else {
        return Ok(false);
    };
    let source_event_ids = serde_json::from_str::<Vec<String>>(&source_event_ids).ok();

    Ok(memory_type == "conversation_turn"
        && domain == event.domain
        && category == event.category
        && source_event_ids.as_deref() == Some(std::slice::from_ref(&event.event_id)))
}

fn unix_time_millis() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64
}

#[cfg(test)]
mod tests {
    use crate::crypto::EncryptionEngine;
    use crate::db::{DatabasePool, MEMORY_VECTOR_DIM, persist_conversation_event_vector};

    fn test_crypto() -> EncryptionEngine {
        EncryptionEngine::new("consolidation-test-key-32-bytes")
    }

    #[test]
    fn event_hop_le_duoc_finalize_cung_checkpoint() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        let conn = pool.writer.get().expect("writer");
        let crypto = test_crypto();
        persist_conversation_event_vector(
            &conn,
            &crypto,
            "turn_consumer_1",
            "Người dùng: tôi tên là Alice và làm dự án ORION-7\nLIVA: đã ghi nhận.",
            &vec![0.2; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("seed event");

        let result = super::process_pending_batch(&conn, &crypto, "projection-worker", 10)
            .expect("consume pending event");

        assert_eq!(result.processed, 1);
        assert_eq!(result.consolidated, 1);
        assert!(result.facts_extracted >= 1, "Must extract semantic facts");
        assert!(result.nodes_created >= 1, "Must create KG nodes");

        let event: (i64, String, i64) = conn
            .query_row(
                "SELECT consolidated, consolidation_status, retry_count \
                 FROM events WHERE eventId = 'turn_consumer_1'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("event state");
        assert_eq!(event, (1, "consolidated".to_string(), 0));

        let checkpoint: (i64, String, i64) = conn
            .query_row(
                "SELECT last_step, state_data, updated_at FROM consolidation_checkpoints \
                 WHERE session_id = 'projection-worker'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("checkpoint");
        assert_eq!(checkpoint.0, 1);
        assert!(checkpoint.1.contains("turn_consumer_1"));

        // Verify that facts are encrypted in facts table
        let encrypted_fact: String = conn
            .query_row("SELECT value FROM facts WHERE key = 'user:name'", [], |r| r.get(0))
            .expect("query fact");
        assert!(encrypted_fact.starts_with("v2:"));
        let decrypted = crypto.read_fact(&encrypted_fact).into_value();
        assert_eq!(decrypted, "Alice");

        let rerun = super::process_pending_batch(&conn, &crypto, "projection-worker", 10)
            .expect("rerun is idempotent");
        assert_eq!(rerun, super::ConsolidationBatchResult::default());
    }
}
