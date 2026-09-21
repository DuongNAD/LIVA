//! Dedicated SQLite Writer Actor for LIVA Native Core.
//!
//! Architectural Solution for Defect RISK-01 (Silent Drop Turn) & SQLITE_BUSY Elimination.
//! Pins the single SQLite write connection to a dedicated background OS thread.
//! Receives write tasks via a bounded channel (capacity: 1024) with backpressure.

use crate::agent::graph::ConversationMemoryScope;
use crate::crypto::EncryptionEngine;
use crate::db::CustomSqliteManager;
use r2d2::Pool;
use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};

pub type DbWriterOp = Box<dyn FnOnce(&rusqlite::Connection) -> Result<(), String> + Send>;

/// Specialized error type for DbActor operations.
pub type DbActorError = String;

pub struct BatchTurnItem {
    pub scope: ConversationMemoryScope,
    pub content: String,
    pub vector: Vec<f32>,
    pub crypto: Arc<EncryptionEngine>,
}

pub enum DbWriteCommand {
    PersistTurn {
        scope: ConversationMemoryScope,
        content: String,
        vector: Vec<f32>,
        crypto: Arc<EncryptionEngine>,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    PersistTurnsBatch {
        turns: Vec<BatchTurnItem>,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    Execute {
        op: DbWriterOp,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    CheckpointWal {
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    ReinforceMemories {
        vec_ids: Vec<String>,
        now_ms: i64,
    },
    SaveAgentCheckpoint {
        thread_id: String,
        state_json: String,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    UpdateFactRecallStats {
        fact_key: String,
        memory_strength: f64,
        now_ts: i64,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    TouchFactAccess {
        fact_key: String,
        now_ts: i64,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    Flush {
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
        sync_tx: Option<std::sync::mpsc::SyncSender<Result<(), String>>>,
    },
    InsertL3Triple {
        subject: String,
        predicate: String,
        object: String,
        weight: f32,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
    InsertL3TriplesBatch {
        triples: Vec<(String, String, String, f32)>,
        result_tx: Option<oneshot::Sender<Result<(), String>>>,
    },
}

struct DbActorJoinGuard {
    join_handle: Option<std::thread::JoinHandle<()>>,
}

impl Drop for DbActorJoinGuard {
    fn drop(&mut self) {
        if let Some(handle) = self.join_handle.take() {
            let _ = handle.join();
        }
    }
}

#[derive(Clone)]
pub struct DbActorHandle {
    tx: mpsc::Sender<DbWriteCommand>,
    _guard: Arc<std::sync::Mutex<DbActorJoinGuard>>,
}

fn reject_command(cmd: DbWriteCommand, err: &str) {
    match cmd {
        DbWriteCommand::PersistTurn {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::PersistTurnsBatch {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::Execute {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::CheckpointWal {
            result_tx: Some(tx),
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::SaveAgentCheckpoint {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::UpdateFactRecallStats {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::TouchFactAccess {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::Flush { result_tx, sync_tx } => {
            if let Some(tx) = result_tx {
                let _ = tx.send(Err(err.to_string()));
            }
            if let Some(tx) = sync_tx {
                let _ = tx.send(Err(err.to_string()));
            }
        }
        DbWriteCommand::InsertL3Triple {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        DbWriteCommand::InsertL3TriplesBatch {
            result_tx: Some(tx),
            ..
        } => {
            let _ = tx.send(Err(err.to_string()));
        }
        _ => {}
    }
}

struct PendingNotification {
    result_tx: Option<oneshot::Sender<Result<(), String>>>,
    sync_tx: Option<std::sync::mpsc::SyncSender<Result<(), String>>>,
}

impl PendingNotification {
    fn from_result_tx(result_tx: Option<oneshot::Sender<Result<(), String>>>) -> Self {
        Self {
            result_tx,
            sync_tx: None,
        }
    }

    fn respond(self, res: Result<(), String>) {
        if let Some(tx) = self.result_tx {
            let _ = tx.send(res.clone());
        }
        if let Some(tx) = self.sync_tx {
            let _ = tx.send(res);
        }
    }
}

fn execute_write_operation(
    conn: &rusqlite::Connection,
    cmd: DbWriteCommand,
) -> (PendingNotification, Result<(), String>) {
    match cmd {
        DbWriteCommand::PersistTurn {
            scope,
            content,
            vector,
            crypto,
            result_tx,
        } => {
            let vec_id = format!("turn_{}", uuid::Uuid::new_v4());
            let res = crate::db::persist_conversation_event_vector(
                conn,
                &crypto,
                &vec_id,
                &content,
                &vector,
                scope.storage_domain(),
                scope.storage_category(),
            )
            .map_err(|e| format!("persist_conversation_event_vector error: {}", e));

            if let Err(ref err) = res {
                tracing::warn!("[DbActor] persist_turn error: {}", err);
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::PersistTurnsBatch { turns, result_tx } => {
            let res = (|| -> Result<(), rusqlite::Error> {
                if turns.is_empty() {
                    return Ok(());
                }
                let batch_items: Vec<_> = turns
                    .iter()
                    .map(|t| {
                        let vec_id = format!("turn_{}", uuid::Uuid::new_v4());
                        (vec_id, t)
                    })
                    .collect();
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_millis() as i64;
                {
                    let mut stmt_event = conn.prepare_cached(
                        "INSERT INTO events (
                            eventId, timestamp, consolidated, domain, category,
                            consolidation_status, retry_count, agentId
                         ) VALUES (?1, ?2, 0, ?3, ?4, 'pending', 0, 'liva_core')",
                    )?;
                    for (vec_id, t) in &batch_items {
                        stmt_event.execute(rusqlite::params![
                            vec_id,
                            now,
                            t.scope.storage_domain(),
                            t.scope.storage_category()
                        ])?;
                        let source_event_ids = [vec_id.clone()];
                        crate::db::upsert_vector(
                            conn,
                            &t.crypto,
                            vec_id,
                            "conversation_turn",
                            &t.content,
                            &t.vector,
                            Some(t.scope.storage_domain()),
                            Some(t.scope.storage_category()),
                            None,
                            None,
                            Some(&source_event_ids),
                        )?;
                    }
                }
                Ok(())
            })()
            .map_err(|e| format!("persist_turns_batch error: {e}"));

            if let Err(ref err) = res {
                tracing::warn!("[DbActor] persist_turns_batch error: {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::Execute { op, result_tx } => {
            let res = op(conn);
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::CheckpointWal { result_tx } => {
            let res = conn
                .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
                .map_err(|e| format!("wal_checkpoint error: {}", e));
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::ReinforceMemories { vec_ids, now_ms } => {
            let refs: Vec<&str> = vec_ids.iter().map(|s| s.as_str()).collect();
            let res = crate::db::reinforce_memory_access(conn, &refs, now_ms)
                .map(|_| ())
                .map_err(|e| format!("reinforce_memories error: {}", e));
            if let Err(ref err) = res {
                tracing::warn!("[DbActor] reinforce_memories error: {}", err);
            }
            (
                PendingNotification {
                    result_tx: None,
                    sync_tx: None,
                },
                res,
            )
        }
        DbWriteCommand::SaveAgentCheckpoint {
            thread_id,
            state_json,
            result_tx,
        } => {
            let res = conn
                .execute(
                    "INSERT OR REPLACE INTO agent_checkpoints (thread_id, state_json) VALUES (?1, ?2)",
                    rusqlite::params![thread_id, state_json],
                )
                .map(|_| ())
                .map_err(|e| format!("save_agent_checkpoint error: {e}"));

            if let Err(ref err) = res {
                tracing::warn!("[DbActor] save_agent_checkpoint error: {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::UpdateFactRecallStats {
            fact_key,
            memory_strength,
            now_ts,
            result_tx,
        } => {
            let res = crate::db::update_fact_recall_stats(conn, &fact_key, memory_strength, now_ts)
                .map(|_| ())
                .map_err(|e| format!("update_fact_recall_stats error for '{fact_key}': {e}"));
            if let Err(ref err) = res {
                tracing::warn!("[DbActor] {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::TouchFactAccess {
            fact_key,
            now_ts,
            result_tx,
        } => {
            let res = crate::db::touch_fact_access(conn, &fact_key, now_ts)
                .map(|_| ())
                .map_err(|e| format!("touch_fact_access error for '{fact_key}': {e}"));
            if let Err(ref err) = res {
                tracing::warn!("[DbActor] {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::Flush { result_tx, sync_tx } => {
            (PendingNotification { result_tx, sync_tx }, Ok(()))
        }
        DbWriteCommand::InsertL3Triple {
            subject,
            predicate,
            object,
            weight,
            result_tx,
        } => {
            let res = (|| -> Result<(), rusqlite::Error> {
                conn.execute(
                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                    [&subject],
                )?;
                conn.execute(
                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                    [&object],
                )?;
                conn.execute(
                    "INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                     VALUES (?1, ?2, ?3, ?4, 0)
                     ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight, obsolete = 0",
                    rusqlite::params![subject, object, predicate, weight as f64],
                )?;
                Ok(())
            })()
            .map_err(|e| format!("insert_l3_triple error: {e}"));

            if let Err(ref err) = res {
                tracing::warn!("[DbActor] insert_l3_triple error: {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
        DbWriteCommand::InsertL3TriplesBatch { triples, result_tx } => {
            let res = (|| -> Result<(), rusqlite::Error> {
                if triples.is_empty() {
                    return Ok(());
                }
                let mut stmt_node = conn.prepare_cached(
                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                )?;
                let mut stmt_edge = conn.prepare_cached(
                    "INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                     VALUES (?1, ?2, ?3, ?4, 0)
                     ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight, obsolete = 0",
                )?;
                for (subject, predicate, object, weight) in &triples {
                    stmt_node.execute([subject])?;
                    stmt_node.execute([object])?;
                    stmt_edge.execute(rusqlite::params![
                        subject,
                        object,
                        predicate,
                        *weight as f64
                    ])?;
                }
                Ok(())
            })()
            .map_err(|e| format!("insert_l3_triples_batch error: {e}"));

            if let Err(ref err) = res {
                tracing::warn!("[DbActor] insert_l3_triples_batch error: {err}");
            }
            (PendingNotification::from_result_tx(result_tx), res)
        }
    }
}

fn process_transactional_batch(conn: &rusqlite::Connection, cmds: Vec<DbWriteCommand>) {
    if cmds.is_empty() {
        return;
    }

    if let Err(e) = conn.execute_batch("BEGIN IMMEDIATE;") {
        let err_msg = format!("Failed to BEGIN IMMEDIATE: {e}");
        tracing::error!("[DbActor] {err_msg}");
        for cmd in cmds {
            reject_command(cmd, &err_msg);
        }
        return;
    }

    let mut pending_notifications: Vec<PendingNotification> = Vec::with_capacity(cmds.len());
    let mut iter = cmds.into_iter();

    while let Some(cmd) = iter.next() {
        let (notif, res) = execute_write_operation(conn, cmd);
        match res {
            Ok(()) => {
                pending_notifications.push(notif);
            }
            Err(err) => {
                let _ = conn.execute_batch("ROLLBACK;");
                let rb_msg = format!("Transaction rolled back due to error in batch: {err}");
                tracing::warn!("[DbActor] {rb_msg}");

                // Notify failing command
                notif.respond(Err(err));

                // Notify previously succeeded commands in this batch of rollback
                for prev in pending_notifications {
                    prev.respond(Err(rb_msg.clone()));
                }

                // Notify remaining commands in this batch of rollback
                for remaining in iter {
                    reject_command(remaining, &rb_msg);
                }
                return;
            }
        }
    }

    // All operations in this batch succeeded; commit the transaction
    match conn.execute_batch("COMMIT;") {
        Ok(()) => {
            for notif in pending_notifications {
                notif.respond(Ok(()));
            }
        }
        Err(commit_err) => {
            let _ = conn.execute_batch("ROLLBACK;");
            let err_msg = format!("Transaction COMMIT failed: {commit_err}");
            tracing::error!("[DbActor] {err_msg}");
            for notif in pending_notifications {
                notif.respond(Err(err_msg.clone()));
            }
        }
    }
}

fn process_write_batch(conn: &rusqlite::Connection, batch: Vec<DbWriteCommand>) {
    if batch.is_empty() {
        return;
    }

    // Split batch around any CheckpointWal commands because PRAGMA wal_checkpoint cannot run in an open transaction
    let mut current_tx_batch: Vec<DbWriteCommand> = Vec::new();

    for cmd in batch {
        if matches!(
            cmd,
            DbWriteCommand::CheckpointWal { .. } | DbWriteCommand::Execute { .. }
        ) {
            // Flush any current transactional writes first
            if !current_tx_batch.is_empty() {
                let tx_cmds = std::mem::take(&mut current_tx_batch);
                process_transactional_batch(conn, tx_cmds);
            }
            // Execute checkpoint or standalone operation outside of ambient transaction
            let (notif, res) = execute_write_operation(conn, cmd);
            notif.respond(res);
        } else {
            current_tx_batch.push(cmd);
        }
    }

    if !current_tx_batch.is_empty() {
        process_transactional_batch(conn, current_tx_batch);
    }
}

impl DbActorHandle {
    pub fn new(writer_pool: Pool<CustomSqliteManager>) -> Self {
        let (tx, mut rx) = mpsc::channel::<DbWriteCommand>(1024);

        let join_handle = std::thread::Builder::new()
            .name("liva-db-writer-actor".to_string())
            .spawn(move || {
                tracing::info!("[DbActor] Dedicated SQLite writer thread started");

                let mut persistent_conn = match writer_pool.get() {
                    Ok(c) => Some(c),
                    Err(e) => {
                        tracing::error!(
                            "[DbActor] Failed initial checkout of writer connection: {}",
                            e
                        );
                        None
                    }
                };

                while let Some(first_cmd) = rx.blocking_recv() {
                    let conn = match &mut persistent_conn {
                        Some(c) => c,
                        None => match writer_pool.get() {
                            Ok(c) => {
                                persistent_conn = Some(c);
                                persistent_conn.as_mut().unwrap()
                            }
                            Err(e) => {
                                let err_msg = format!("DB pool checkout error: {}", e);
                                tracing::error!(
                                    "[DbActor] Failed to checkout writer connection: {}",
                                    e
                                );
                                reject_command(first_cmd, &err_msg);
                                while let Ok(next_cmd) = rx.try_recv() {
                                    reject_command(next_cmd, &err_msg);
                                }
                                continue;
                            }
                        },
                    };

                    let mut batch = Vec::with_capacity(50);
                    let is_flush = matches!(first_cmd, DbWriteCommand::Flush { .. });
                    batch.push(first_cmd);

                    if !is_flush {
                        let start = std::time::Instant::now();
                        let time_limit = std::time::Duration::from_millis(5);

                        while batch.len() < 50 {
                            let elapsed = start.elapsed();
                            if elapsed >= time_limit {
                                break;
                            }
                            match rx.try_recv() {
                                Ok(next_cmd) => {
                                    let has_flush =
                                        matches!(next_cmd, DbWriteCommand::Flush { .. });
                                    batch.push(next_cmd);
                                    if has_flush {
                                        break;
                                    }
                                }
                                Err(mpsc::error::TryRecvError::Empty) => {
                                    let remaining = time_limit.saturating_sub(elapsed);
                                    if remaining.is_zero() {
                                        break;
                                    }
                                    std::thread::sleep(std::cmp::min(
                                        remaining,
                                        std::time::Duration::from_micros(250),
                                    ));
                                }
                                Err(mpsc::error::TryRecvError::Disconnected) => break,
                            }
                        }
                    }

                    process_write_batch(conn, batch);
                }
                tracing::info!("[DbActor] Dedicated SQLite writer thread terminated cleanly");
            })
            .expect("failed to spawn liva-db-writer-actor thread");

        Self {
            tx,
            _guard: Arc::new(std::sync::Mutex::new(DbActorJoinGuard {
                join_handle: Some(join_handle),
            })),
        }
    }

    /// Asynchronously submit a write command with backpressure.
    pub async fn send(&self, cmd: DbWriteCommand) -> Result<(), String> {
        self.tx
            .send(cmd)
            .await
            .map_err(|_| "DbActor channel closed".to_string())
    }

    /// Synchronously submit a write command with backpressure.
    ///
    /// Uses non-panicking `try_send` with brief backoff if queue is full,
    /// making it completely safe across single-threaded (current_thread),
    /// multi-threaded, and non-Tokio OS threads.
    pub fn blocking_send(&self, cmd: DbWriteCommand) -> Result<(), String> {
        let mut cur_cmd = cmd;
        for _ in 0..1000 {
            match self.tx.try_send(cur_cmd) {
                Ok(()) => return Ok(()),
                Err(mpsc::error::TrySendError::Full(c)) => {
                    cur_cmd = c;
                    std::thread::sleep(std::time::Duration::from_millis(1));
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    return Err("DbActor channel closed".to_string());
                }
            }
        }
        Err("DbActor channel full timeout".to_string())
    }

    /// Asynchronously execute a write closure on the dedicated DbActor thread.
    pub async fn execute<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let (reply_tx, reply_rx) = tokio::sync::oneshot::channel();

        self.send(DbWriteCommand::Execute {
            op: Box::new(move |conn| {
                let res = f(conn);
                let _ = reply_tx.send(res);
                Ok(())
            }),
            result_tx: None,
        })
        .await?;

        reply_rx
            .await
            .map_err(|_| "DbActor dropped reply channel".to_string())?
    }

    /// Synchronously execute a write closure on the dedicated DbActor thread.
    ///
    /// Completely safe across both single-threaded (current_thread) and multi-threaded Tokio runtimes.
    pub fn blocking_execute<F, R>(&self, f: F) -> Result<R, String>
    where
        F: FnOnce(&rusqlite::Connection) -> Result<R, String> + Send + 'static,
        R: Send + 'static,
    {
        let (tx, rx) = std::sync::mpsc::sync_channel(1);

        self.blocking_send(DbWriteCommand::Execute {
            op: Box::new(move |conn| {
                let res = f(conn);
                let _ = tx.send(res);
                Ok(())
            }),
            result_tx: None,
        })?;

        rx.recv()
            .map_err(|_| "DbActor thread dropped sender without responding".to_string())?
    }

    /// Asynchronously persist a conversation turn with pre-computed vector.
    pub async fn persist_turn(
        &self,
        scope: ConversationMemoryScope,
        content: String,
        vector: Vec<f32>,
        crypto: Arc<EncryptionEngine>,
    ) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::PersistTurn {
            scope,
            content,
            vector,
            crypto,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Asynchronously trigger a WAL checkpoint on the dedicated SQLite writer thread and wait for completion.
    pub async fn checkpoint_wal(&self) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::CheckpointWal {
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Submit a WAL checkpoint to the dedicated SQLite writer thread without awaiting completion.
    pub fn try_checkpoint_wal(&self) -> Result<(), String> {
        self.tx
            .try_send(DbWriteCommand::CheckpointWal { result_tx: None })
            .map_err(|e| format!("Failed to queue CheckpointWal: {e}"))
    }

    /// Asynchronously and non-blockingly reinforce access counts for memories recalled by RAG.
    pub fn reinforce_memories(&self, vec_ids: Vec<String>, now_ms: i64) {
        if vec_ids.is_empty() {
            return;
        }
        let _ = self
            .tx
            .try_send(DbWriteCommand::ReinforceMemories { vec_ids, now_ms });
    }

    /// Asynchronously save an encrypted agent checkpoint via DbActor with confirmation.
    pub async fn save_agent_checkpoint(
        &self,
        thread_id: String,
        state_json: String,
    ) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::SaveAgentCheckpoint {
            thread_id,
            state_json,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Asynchronously wait for all previously queued write commands to be processed and committed to SQLite WAL.
    pub async fn flush(&self) -> Result<(), DbActorError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::Flush {
            result_tx: Some(result_tx),
            sync_tx: None,
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Asynchronously wait for all previously queued write commands to be processed and committed to SQLite WAL (alias for flush).
    pub async fn sync(&self) -> Result<(), DbActorError> {
        self.flush().await
    }

    /// Synchronously wait for all previously queued write commands to be processed and committed to SQLite WAL.
    ///
    /// Completely safe across both single-threaded (current_thread) and multi-threaded Tokio runtimes.
    pub fn blocking_flush(&self) -> Result<(), DbActorError> {
        let (sync_tx, sync_rx) = std::sync::mpsc::sync_channel(1);
        self.blocking_send(DbWriteCommand::Flush {
            result_tx: None,
            sync_tx: Some(sync_tx),
        })?;

        sync_rx
            .recv()
            .map_err(|_| "DbActor thread dropped sender without responding".to_string())?
    }

    /// Synchronously wait for all previously queued write commands to be processed and committed to SQLite WAL (alias for blocking_flush).
    pub fn blocking_sync(&self) -> Result<(), DbActorError> {
        self.blocking_flush()
    }

    /// Non-blockingly update recall stats for a fact on the dedicated SQLite writer thread.
    pub fn update_fact_recall_stats(&self, fact_key: String, memory_strength: f64, now_ts: i64) {
        if let Err(e) = self.tx.try_send(DbWriteCommand::UpdateFactRecallStats {
            fact_key,
            memory_strength,
            now_ts,
            result_tx: None,
        }) {
            tracing::warn!("[DbActor] Failed to queue update_fact_recall_stats: {e}");
        }
    }

    /// Asynchronously update recall stats for a fact on the dedicated SQLite writer thread and await commit.
    pub async fn update_fact_recall_stats_async(
        &self,
        fact_key: String,
        memory_strength: f64,
        now_ts: i64,
    ) -> Result<(), DbActorError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::UpdateFactRecallStats {
            fact_key,
            memory_strength,
            now_ts,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Synchronously update recall stats for a fact on the dedicated SQLite writer thread and wait for commit.
    pub fn update_fact_recall_stats_sync(
        &self,
        fact_key: String,
        memory_strength: f64,
        now_ts: i64,
    ) -> Result<(), DbActorError> {
        self.blocking_execute(move |conn| {
            crate::db::update_fact_recall_stats(conn, &fact_key, memory_strength, now_ts)
                .map(|_| ())
                .map_err(|e| format!("update_fact_recall_stats_sync error: {e}"))
        })
    }

    /// Non-blockingly touch fact access timestamp on the dedicated SQLite writer thread.
    pub fn touch_fact_access(&self, fact_key: String, now_ts: i64) {
        if let Err(e) = self.tx.try_send(DbWriteCommand::TouchFactAccess {
            fact_key,
            now_ts,
            result_tx: None,
        }) {
            tracing::warn!("[DbActor] Failed to queue touch_fact_access: {e}");
        }
    }

    /// Asynchronously touch fact access timestamp on the dedicated SQLite writer thread and await commit.
    pub async fn touch_fact_access_async(
        &self,
        fact_key: String,
        now_ts: i64,
    ) -> Result<(), DbActorError> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::TouchFactAccess {
            fact_key,
            now_ts,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Synchronously touch fact access timestamp on the dedicated SQLite writer thread and wait for commit.
    pub fn touch_fact_access_sync(
        &self,
        fact_key: String,
        now_ts: i64,
    ) -> Result<(), DbActorError> {
        self.blocking_execute(move |conn| {
            crate::db::touch_fact_access(conn, &fact_key, now_ts)
                .map(|_| ())
                .map_err(|e| format!("touch_fact_access_sync error: {e}"))
        })
    }

    /// Asynchronously insert an L3 knowledge graph triple via DbActor with confirmation.
    pub async fn insert_l3_triple(
        &self,
        subject: String,
        predicate: String,
        object: String,
        weight: f32,
    ) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::InsertL3Triple {
            subject,
            predicate,
            object,
            weight,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Asynchronously insert multiple L3 knowledge graph triples via DbActor in a single batch transaction.
    pub async fn insert_l3_triples_batch(
        &self,
        triples: Vec<(String, String, String, f32)>,
    ) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::InsertL3TriplesBatch {
            triples,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }

    /// Asynchronously persist multiple conversation turns in a single batch transaction.
    pub async fn persist_turns_batch(&self, turns: Vec<BatchTurnItem>) -> Result<(), String> {
        let (result_tx, result_rx) = oneshot::channel();
        self.send(DbWriteCommand::PersistTurnsBatch {
            turns,
            result_tx: Some(result_tx),
        })
        .await?;

        result_rx
            .await
            .map_err(|_| "DbActor response dropped".to_string())?
    }
}
