use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};

const MAX_BATCH_SIZE: usize = 100;
const MAX_RETRIES: i64 = 3;
const PROJECTION_WORKER_ID: &str = "event-projection-v1";
const DEFAULT_BATCH_SIZE: usize = 25;
const DEFAULT_INTERVAL: std::time::Duration = std::time::Duration::from_secs(30);

#[derive(Debug, Default, PartialEq, Eq)]
pub struct ConsolidationBatchResult {
    pub processed: usize,
    pub consolidated: usize,
    pub retried: usize,
    pub dead_lettered: usize,
    pub l3_triples_extracted: usize,
}

struct PendingEvent {
    event_id: String,
    retry_count: i64,
    domain: String,
    category: String,
}

pub async fn consume_pending_once(
    db: &crate::db::DatabasePool,
    crypto: &crate::crypto::EncryptionEngine,
    batch_size: usize,
) -> Result<ConsolidationBatchResult, String> {
    let csr_graph = db.csr_graph.clone();
    let crypto = crypto.clone();
    db.writer_actor
        .execute(move |conn| {
            let result = process_pending_batch(conn, &crypto, PROJECTION_WORKER_ID, batch_size)
                .map_err(|error| format!("event projection consumer loi: {error}"))?;

            // Re-synchronize In-Memory CsrGraph with newly extracted L3 knowledge triples
            #[allow(clippy::collapsible_if)]
            if result.l3_triples_extracted > 0 {
                if let Ok(graph) = crate::db::csr_graph::CsrGraph::from_db(conn) {
                    let _ = csr_graph.write().map(|mut g_lock| *g_lock = graph);
                }
            }

            Ok(result)
        })
        .await
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
        match consume_pending_once(&db, &crypto, batch_size).await {
            Ok(result) if result.processed > 0 => {
                tracing::info!(
                    processed = result.processed,
                    consolidated = result.consolidated,
                    retried = result.retried,
                    dead_lettered = result.dead_lettered,
                    l3_triples = result.l3_triples_extracted,
                    "event projection batch completed"
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
    crypto: &crate::crypto::EncryptionEngine,
    worker_id: &str,
    batch_size: usize,
) -> Result<ConsolidationBatchResult, rusqlite::Error> {
    let tx = if conn.is_autocommit() {
        Some(Transaction::new_unchecked(
            conn,
            TransactionBehavior::Immediate,
        )?)
    } else {
        None
    };
    let batch_size = batch_size.clamp(1, MAX_BATCH_SIZE) as i64;
    let events = {
        let mut statement = conn.prepare(
            "SELECT eventId, retry_count, domain, category FROM events \
             WHERE consolidation_status = 'pending' \
             ORDER BY timestamp, eventId \
             LIMIT ?1",
        )?;
        statement
            .query_map([batch_size], |row| {
                Ok(PendingEvent {
                    event_id: row.get(0)?,
                    retry_count: row.get(1)?,
                    domain: row.get(2)?,
                    category: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?
    };
    let mut result = ConsolidationBatchResult::default();

    for event in &events {
        if projection_matches(conn, event)? {
            // L3 Knowledge Graph extraction: extract triples from turn content
            let content: Option<String> = conn
                .query_row(
                    "SELECT content FROM vectors_meta WHERE vec_id = ?1",
                    [&event.event_id],
                    |row| row.get(0),
                )
                .optional()?;

            if let Some(raw_content) = content {
                let turn_content = match crypto.read_fact(&raw_content) {
                    crate::crypto::FactRead::Ok(plain) => plain,
                    crate::crypto::FactRead::Locked { reason } => {
                        tracing::warn!(
                            event_id = %event.event_id,
                            reason = %reason,
                            "Bỏ qua trích xuất tri thức L3 cho conversation turn bị khóa (sai LIVA_ENCRYPTION_KEY)"
                        );
                        String::new()
                    }
                };

                if !turn_content.is_empty() {
                    let triples =
                        crate::cognitive::triple_extractor::extract_triples(&turn_content);
                    for triple in &triples {
                        // 1. Insert Subject Node
                        conn.execute(
                            "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                             ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                            [&triple.subject],
                        )?;

                        // 2. Insert Object Node
                        conn.execute(
                            "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                             ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                            [&triple.object],
                        )?;

                        // 3. Insert Edge (Subject -> Object)
                        conn.execute(
                            "INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                             VALUES (?1, ?2, ?3, ?4, 0)
                             ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight, obsolete = 0",
                            params![triple.subject, triple.object, triple.relation, triple.weight as f64],
                        )?;
                        result.l3_triples_extracted += 1;
                    }
                }
            }

            let updated = conn.execute(
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
            let updated = conn.execute(
                "UPDATE events \
                 SET consolidated = 0, consolidation_status = 'dlq', retry_count = ?2 \
                 WHERE eventId = ?1 AND consolidation_status = 'pending'",
                params![event.event_id, retry_count],
            )?;
            if updated > 0 {
                conn.execute(
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
            let updated = conn.execute(
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
        })
        .to_string();
        conn.execute(
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

    if let Some(t) = tx {
        t.commit()?;
    }
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
        persist_conversation_event_vector(
            &conn,
            &test_crypto(),
            "turn_consumer_1",
            "Người dùng: nhớ ORION-7\nLIVA: đã nhớ.",
            &vec![0.2; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("seed event");

        let result = super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10)
            .expect("consume pending event");

        assert_eq!(result.processed, 1);
        assert_eq!(result.consolidated, 1);
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

        let rerun = super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10)
            .expect("rerun is idempotent");
        assert_eq!(rerun, super::ConsolidationBatchResult::default());
        let checkpoint_after: (i64, String, i64) = conn
            .query_row(
                "SELECT last_step, state_data, updated_at FROM consolidation_checkpoints \
                 WHERE session_id = 'projection-worker'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )
            .expect("checkpoint after rerun");
        assert_eq!(checkpoint_after, checkpoint);
    }

    #[test]
    fn projection_thieu_retry_ba_lan_roi_vao_dlq_mot_lan() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        let conn = pool.writer.get().expect("writer");
        conn.execute(
            "INSERT INTO events (
                eventId, timestamp, consolidated, domain, category,
                consolidation_status, retry_count, agentId
             ) VALUES (
                'turn_missing_projection', 1, 0, 'memory_owner:local',
                'conversation:default', 'pending', 0, 'liva_core'
             )",
            [],
        )
        .expect("seed invalid event");

        for expected_retry in 1..=2 {
            let result =
                super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10)
                    .expect("retry invalid event");
            assert_eq!(result.retried, 1);
            let state: (String, i64) = conn
                .query_row(
                    "SELECT consolidation_status, retry_count FROM events \
                     WHERE eventId = 'turn_missing_projection'",
                    [],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )
                .expect("retry state");
            assert_eq!(state, ("pending".to_string(), expected_retry));
        }

        let result = super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10)
            .expect("move invalid event to dlq");
        assert_eq!(result.dead_lettered, 1);
        let state: (String, i64) = conn
            .query_row(
                "SELECT consolidation_status, retry_count FROM events \
                 WHERE eventId = 'turn_missing_projection'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("dlq state");
        assert_eq!(state, ("dlq".to_string(), 3));
        let dlq_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM dlq_consolidation \
                 WHERE session_id = 'turn_missing_projection'",
                [],
                |row| row.get(0),
            )
            .expect("dlq count");
        assert_eq!(dlq_count, 1);

        let idle = super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10)
            .expect("dlq event is excluded");
        assert_eq!(idle.processed, 0);
        let dlq_count_after: i64 = conn
            .query_row(
                "SELECT count(*) FROM dlq_consolidation \
                 WHERE session_id = 'turn_missing_projection'",
                [],
                |row| row.get(0),
            )
            .expect("dlq count after rerun");
        assert_eq!(dlq_count_after, 1);
    }

    #[test]
    fn loi_checkpoint_rollback_trang_thai_event() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        let conn = pool.writer.get().expect("writer");
        persist_conversation_event_vector(
            &conn,
            &test_crypto(),
            "turn_atomic_consumer",
            "Người dùng: nhớ atomic\nLIVA: đã nhớ.",
            &vec![0.2; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("seed event");
        conn.execute("DROP TABLE consolidation_checkpoints", [])
            .expect("force checkpoint failure");

        let result = super::process_pending_batch(&conn, &test_crypto(), "projection-worker", 10);
        assert!(result.is_err(), "checkpoint lỗi phải làm cả batch thất bại");

        let event: (i64, String) = conn
            .query_row(
                "SELECT consolidated, consolidation_status FROM events \
                 WHERE eventId = 'turn_atomic_consumer'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .expect("event state after rollback");
        assert_eq!(event, (0, "pending".to_string()));
    }

    #[tokio::test]
    async fn async_consumer_dung_writer_pool_va_finalize_event() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        {
            let conn = pool.writer.get().expect("writer");
            persist_conversation_event_vector(
                &conn,
                &test_crypto(),
                "turn_async_consumer",
                "Người dùng: nhớ async\nLIVA: đã nhớ.",
                &vec![0.2; MEMORY_VECTOR_DIM],
                "memory_owner:local",
                "conversation:default",
            )
            .expect("seed event");
        }

        let result = super::consume_pending_once(&pool, &test_crypto(), 10)
            .await
            .expect("async consumer");
        assert_eq!(result.consolidated, 1);

        let conn = pool.writer.get().expect("writer after consume");
        let status: String = conn
            .query_row(
                "SELECT consolidation_status FROM events \
                 WHERE eventId = 'turn_async_consumer'",
                [],
                |row| row.get(0),
            )
            .expect("event status");
        assert_eq!(status, "consolidated");
    }

    #[tokio::test]
    async fn runner_nen_tu_dong_xu_ly_event_pending() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        {
            let conn = pool.writer.get().expect("writer");
            persist_conversation_event_vector(
                &conn,
                &test_crypto(),
                "turn_background_consumer",
                "Người dùng: nhớ background\nLIVA: đã nhớ.",
                &vec![0.2; MEMORY_VECTOR_DIM],
                "memory_owner:local",
                "conversation:default",
            )
            .expect("seed event");
        }

        let worker = tokio::spawn(super::run_projection_consumer(
            pool.clone(),
            test_crypto(),
            std::time::Duration::from_millis(5),
            10,
        ));
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let status: String = {
                    let conn = pool.writer.get().expect("writer while waiting");
                    conn.query_row(
                        "SELECT consolidation_status FROM events \
                         WHERE eventId = 'turn_background_consumer'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("event status")
                };
                if status == "consolidated" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("background consumer timeout");
        worker.abort();
        let _ = worker.await;
    }

    #[tokio::test]
    async fn spawn_helper_dung_cau_hinh_runtime_mac_dinh() {
        let pool = DatabasePool::new_in_memory().expect("in-memory db");
        {
            let conn = pool.writer.get().expect("writer");
            persist_conversation_event_vector(
                &conn,
                &test_crypto(),
                "turn_spawn_consumer",
                "Người dùng: nhớ spawn\nLIVA: đã nhớ.",
                &vec![0.2; MEMORY_VECTOR_DIM],
                "memory_owner:local",
                "conversation:default",
            )
            .expect("seed event");
        }

        let worker = super::spawn_projection_consumer(pool.clone(), test_crypto());
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            loop {
                let status: String = {
                    let conn = pool.writer.get().expect("writer while waiting");
                    conn.query_row(
                        "SELECT consolidation_status FROM events \
                         WHERE eventId = 'turn_spawn_consumer'",
                        [],
                        |row| row.get(0),
                    )
                    .expect("event status")
                };
                if status == "consolidated" {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(5)).await;
            }
        })
        .await
        .expect("spawn helper timeout");
        worker.abort();
        let _ = worker.await;
    }
}
