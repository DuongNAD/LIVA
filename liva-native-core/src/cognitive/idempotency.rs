use super::observation::ToolObservation;
use rusqlite::{Connection, Result as SqlResult, params};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::{Arc, RwLock};

/// Execution state of an idempotent action.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdempotencyState {
    /// Action has been accepted and is currently executing.
    Pending,
    /// Action has finished execution successfully with cached response.
    Completed,
    /// Action execution failed.
    Failed,
}

impl IdempotencyState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }

    pub fn from_text(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "completed" => Self::Completed,
            "failed" => Self::Failed,
            _ => Self::Pending,
        }
    }
}

/// A stored idempotency record tracking state and cached result across time.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct IdempotencyRecord {
    pub idempotency_key: String,
    pub action_id: String,
    pub tool_id: String,
    pub status: IdempotencyState,
    pub response_json: Option<String>,
    pub created_at_ms: i64,
    pub expires_at_ms: i64,
}

/// Outcome of checking an idempotency key before tool execution.
#[derive(Debug, Clone, PartialEq)]
pub enum IdempotencyCheckResult {
    /// No existing valid execution found; acquired lock for new execution.
    New,
    /// An execution with this key is currently in progress.
    InProgress,
    /// Previously completed; returns cached ToolObservation if available.
    Completed(Option<ToolObservation>),
    /// Previously failed; returns error string if available.
    Failed(Option<String>),
}

/// Thread-safe Idempotency Manager with dual-layer In-Memory Cache and SQLite persistence.
#[derive(Debug, Clone)]
pub struct IdempotencyManager {
    memory_cache: Arc<RwLock<HashMap<String, IdempotencyRecord>>>,
}

impl Default for IdempotencyManager {
    fn default() -> Self {
        Self::new()
    }
}

impl IdempotencyManager {
    pub fn new() -> Self {
        Self {
            memory_cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Checks the idempotency status. If absent or expired, registers a Pending record
    /// both in memory and in SQLite.
    pub fn check_or_start(
        &self,
        key: &str,
        action_id: &str,
        tool_id: &str,
        ttl_ms: i64,
        conn: Option<&Connection>,
    ) -> Result<IdempotencyCheckResult, String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let expires_at_ms = now_ms + ttl_ms;

        // 1. Acquire write lock continuously across memory check, DB check, and Pending registration
        let mut cache = self
            .memory_cache
            .write()
            .map_err(|e| format!("Memory cache lock error: {e}"))?;

        if let Some(record) = cache.get(key) {
            if record.expires_at_ms > now_ms {
                match record.status {
                    IdempotencyState::Pending => return Ok(IdempotencyCheckResult::InProgress),
                    IdempotencyState::Completed => {
                        let obs = record
                            .response_json
                            .as_ref()
                            .and_then(|j| serde_json::from_str::<ToolObservation>(j).ok());
                        return Ok(IdempotencyCheckResult::Completed(obs));
                    }
                    IdempotencyState::Failed => {
                        return Ok(IdempotencyCheckResult::Failed(record.response_json.clone()));
                    }
                }
            } else {
                cache.remove(key);
            }
        }

        // 2. Check SQLite persistence if connection provided
        if let Some(c) = conn
            && let Ok(Some(record)) = Self::db_get(c, key)
            && record.expires_at_ms > now_ms
        {
            cache.insert(key.to_string(), record.clone());

            match record.status {
                IdempotencyState::Pending => return Ok(IdempotencyCheckResult::InProgress),
                IdempotencyState::Completed => {
                    let obs = record
                        .response_json
                        .as_ref()
                        .and_then(|j| serde_json::from_str::<ToolObservation>(j).ok());
                    return Ok(IdempotencyCheckResult::Completed(obs));
                }
                IdempotencyState::Failed => {
                    return Ok(IdempotencyCheckResult::Failed(record.response_json.clone()));
                }
            }
        }

        // 3. Register as Pending (New execution) under same write lock
        let record = IdempotencyRecord {
            idempotency_key: key.to_string(),
            action_id: action_id.to_string(),
            tool_id: tool_id.to_string(),
            status: IdempotencyState::Pending,
            response_json: None,
            created_at_ms: now_ms,
            expires_at_ms,
        };

        cache.insert(key.to_string(), record.clone());
        drop(cache);

        if let Some(c) = conn {
            Self::db_upsert(c, &record).map_err(|e| format!("DB upsert error: {e}"))?;
        }

        Ok(IdempotencyCheckResult::New)
    }

    /// Marks an action as Completed with its resulting ToolObservation.
    pub fn complete(
        &self,
        key: &str,
        observation: &ToolObservation,
        conn: Option<&Connection>,
    ) -> Result<(), String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;
        let response_json = serde_json::to_string(observation).map_err(|e| e.to_string())?;

        // Update memory cache
        if let Ok(mut cache) = self.memory_cache.write() {
            if let Some(rec) = cache.get_mut(key) {
                rec.status = IdempotencyState::Completed;
                rec.response_json = Some(response_json.clone());
            } else {
                cache.insert(
                    key.to_string(),
                    IdempotencyRecord {
                        idempotency_key: key.to_string(),
                        action_id: observation.action_id.clone(),
                        tool_id: observation.tool_id.clone(),
                        status: IdempotencyState::Completed,
                        response_json: Some(response_json.clone()),
                        created_at_ms: now_ms,
                        expires_at_ms: now_ms + 86_400_000, // 24h default TTL
                    },
                );
            }
        }

        // Update SQLite
        if let Some(c) = conn {
            c.execute(
                "UPDATE idempotency_records
                 SET status = 'completed', response_json = ?1
                 WHERE idempotency_key = ?2",
                params![response_json, key],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Marks an action as Failed with error description.
    pub fn fail(
        &self,
        key: &str,
        error_msg: &str,
        conn: Option<&Connection>,
    ) -> Result<(), String> {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as i64;

        if let Ok(mut cache) = self.memory_cache.write() {
            if let Some(rec) = cache.get_mut(key) {
                rec.status = IdempotencyState::Failed;
                rec.response_json = Some(error_msg.to_string());
            } else {
                cache.insert(
                    key.to_string(),
                    IdempotencyRecord {
                        idempotency_key: key.to_string(),
                        action_id: "unknown".to_string(),
                        tool_id: "unknown".to_string(),
                        status: IdempotencyState::Failed,
                        response_json: Some(error_msg.to_string()),
                        created_at_ms: now_ms,
                        expires_at_ms: now_ms + 3_600_000, // 1h failure TTL
                    },
                );
            }
        }

        if let Some(c) = conn {
            c.execute(
                "UPDATE idempotency_records
                 SET status = 'failed', response_json = ?1
                 WHERE idempotency_key = ?2",
                params![error_msg, key],
            )
            .map_err(|e| e.to_string())?;
        }

        Ok(())
    }

    /// Removes expired entries from both memory cache and SQLite.
    pub fn cleanup_expired(&self, now_ms: i64, conn: Option<&Connection>) -> Result<usize, String> {
        let mut count = 0;

        // Clean memory
        if let Ok(mut cache) = self.memory_cache.write() {
            cache.retain(|_, v| v.expires_at_ms > now_ms);
        }

        // Clean SQLite
        if let Some(c) = conn {
            count = c
                .execute(
                    "DELETE FROM idempotency_records WHERE expires_at_ms <= ?1",
                    params![now_ms],
                )
                .map_err(|e| e.to_string())?;
        }

        Ok(count)
    }

    // Helper: SQLite query
    pub fn db_get(conn: &Connection, key: &str) -> SqlResult<Option<IdempotencyRecord>> {
        let mut stmt = conn.prepare(
            "SELECT idempotency_key, action_id, tool_id, status, response_json, created_at_ms, expires_at_ms
             FROM idempotency_records
             WHERE idempotency_key = ?1",
        )?;

        let mut rows = stmt.query(params![key])?;
        if let Some(row) = rows.next()? {
            let status_str: String = row.get(3)?;
            Ok(Some(IdempotencyRecord {
                idempotency_key: row.get(0)?,
                action_id: row.get(1)?,
                tool_id: row.get(2)?,
                status: IdempotencyState::from_text(&status_str),
                response_json: row.get(4)?,
                created_at_ms: row.get(5)?,
                expires_at_ms: row.get(6)?,
            }))
        } else {
            Ok(None)
        }
    }

    // Helper: SQLite upsert
    pub fn db_upsert(conn: &Connection, record: &IdempotencyRecord) -> SqlResult<()> {
        conn.execute(
            "INSERT INTO idempotency_records (
                idempotency_key, action_id, tool_id, status, response_json, created_at_ms, expires_at_ms
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
            ON CONFLICT(idempotency_key) DO UPDATE SET
                status = excluded.status,
                response_json = excluded.response_json,
                expires_at_ms = excluded.expires_at_ms",
            params![
                record.idempotency_key,
                record.action_id,
                record.tool_id,
                record.status.as_str(),
                record.response_json,
                record.created_at_ms,
                record.expires_at_ms,
            ],
        )?;
        Ok(())
    }
}
