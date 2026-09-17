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

pub enum DbWriteCommand {
    PersistTurn {
        scope: ConversationMemoryScope,
        content: String,
        vector: Vec<f32>,
        crypto: Arc<EncryptionEngine>,
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
    },
    TouchFactAccess {
        fact_key: String,
        now_ts: i64,
    },
    InsertL3Triple {
        subject: String,
        predicate: String,
        object: String,
        weight: f32,
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

impl DbActorHandle {
    pub fn new(writer_pool: Pool<CustomSqliteManager>) -> Self {
        let (tx, mut rx) = mpsc::channel::<DbWriteCommand>(1024);

        let join_handle = std::thread::Builder::new()
            .name("liva-db-writer-actor".to_string())
            .spawn(move || {
                tracing::info!("[DbActor] Dedicated SQLite writer thread started");

                while let Some(cmd) = rx.blocking_recv() {
                    let conn = match writer_pool.get() {
                        Ok(c) => c,
                        Err(e) => {
                            tracing::error!(
                                "[DbActor] Failed to checkout writer connection from pool: {}",
                                e
                            );
                            match cmd {
                                DbWriteCommand::PersistTurn {
                                    result_tx: Some(tx),
                                    ..
                                } => {
                                    let _ = tx.send(Err(format!("DB pool checkout error: {}", e)));
                                }
                                DbWriteCommand::Execute {
                                    result_tx: Some(tx),
                                    ..
                                } => {
                                    let _ = tx.send(Err(format!("DB pool checkout error: {}", e)));
                                }
                                DbWriteCommand::CheckpointWal {
                                    result_tx: Some(tx),
                                } => {
                                    let _ = tx.send(Err(format!("DB pool checkout error: {}", e)));
                                }
                                DbWriteCommand::SaveAgentCheckpoint {
                                    result_tx: Some(tx),
                                    ..
                                } => {
                                    let _ = tx.send(Err(format!("DB pool checkout error: {}", e)));
                                }
                                DbWriteCommand::InsertL3Triple {
                                    result_tx: Some(tx),
                                    ..
                                } => {
                                    let _ = tx.send(Err(format!("DB pool checkout error: {}", e)));
                                }
                                _ => {}
                            }
                            continue;
                        }
                    };

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
                                &conn,
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
                            if let Some(reply) = result_tx {
                                let _ = reply.send(res);
                            }
                        }
                        DbWriteCommand::Execute { op, result_tx } => {
                            let res = op(&conn);
                            if let Some(reply) = result_tx {
                                let _ = reply.send(res);
                            }
                        }
                        DbWriteCommand::CheckpointWal { result_tx } => {
                            let res = conn
                                .execute_batch("PRAGMA wal_checkpoint(PASSIVE);")
                                .map_err(|e| format!("wal_checkpoint error: {}", e));
                            if let Some(reply) = result_tx {
                                let _ = reply.send(res);
                            }
                        }
                        DbWriteCommand::ReinforceMemories { vec_ids, now_ms } => {
                            let refs: Vec<&str> = vec_ids.iter().map(|s| s.as_str()).collect();
                            if let Err(e) = crate::db::reinforce_memory_access(&conn, &refs, now_ms)
                            {
                                tracing::warn!("[DbActor] reinforce_memories error: {}", e);
                            }
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
                            if let Some(reply) = result_tx {
                                let _ = reply.send(res);
                            }
                        }
                        DbWriteCommand::UpdateFactRecallStats {
                            fact_key,
                            memory_strength,
                            now_ts,
                        } => {
                            if let Err(e) = crate::db::update_fact_recall_stats(
                                &conn,
                                &fact_key,
                                memory_strength,
                                now_ts,
                            ) {
                                tracing::warn!(
                                    "[DbActor] update_fact_recall_stats error for '{fact_key}': {e}"
                                );
                            }
                        }
                        DbWriteCommand::TouchFactAccess { fact_key, now_ts } => {
                            if let Err(e) = crate::db::touch_fact_access(&conn, &fact_key, now_ts) {
                                tracing::warn!(
                                    "[DbActor] touch_fact_access error for '{fact_key}': {e}"
                                );
                            }
                        }
                        DbWriteCommand::InsertL3Triple {
                            subject,
                            predicate,
                            object,
                            weight,
                            result_tx,
                        } => {
                            let res = (|| -> Result<(), rusqlite::Error> {
                                let tx = conn.unchecked_transaction()?;
                                tx.execute(
                                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                                    [&subject],
                                )?;
                                tx.execute(
                                    "INSERT INTO l3_nodes (id, label, properties) VALUES (?1, ?1, '{}')
                                     ON CONFLICT(id) DO UPDATE SET label = excluded.label",
                                    [&object],
                                )?;
                                tx.execute(
                                    "INSERT INTO l3_edges (source, target, relation, weight, obsolete)
                                     VALUES (?1, ?2, ?3, ?4, 0)
                                     ON CONFLICT(source, target, relation) DO UPDATE SET weight = excluded.weight, obsolete = 0",
                                    rusqlite::params![subject, object, predicate, weight as f64],
                                )?;
                                tx.commit()?;
                                Ok(())
                            })()
                            .map_err(|e| format!("insert_l3_triple error: {e}"));

                            if let Err(ref err) = res {
                                tracing::warn!("[DbActor] insert_l3_triple error: {err}");
                            }
                            if let Some(reply) = result_tx {
                                let _ = reply.send(res);
                            }
                        }
                    }
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

    /// Synchronously update recall stats for a fact on the dedicated SQLite writer thread.
    pub fn update_fact_recall_stats(&self, fact_key: String, memory_strength: f64, now_ts: i64) {
        let _ = self.blocking_execute(move |conn| {
            crate::db::update_fact_recall_stats(conn, &fact_key, memory_strength, now_ts)
                .map_err(|e| e.to_string())
        });
    }

    /// Synchronously touch fact access timestamp on the dedicated SQLite writer thread.
    pub fn touch_fact_access(&self, fact_key: String, now_ts: i64) {
        let _ = self.blocking_execute(move |conn| {
            crate::db::touch_fact_access(conn, &fact_key, now_ts).map_err(|e| e.to_string())
        });
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
}
