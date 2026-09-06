use crate::agent::state::AgentState;
use crate::crypto::{EncryptionEngine, FactRead};
use crate::db::DatabasePool;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// RFC 6902 JSON Patch operation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum JsonPatchOp {
    Add { path: String, value: Value },
    Remove { path: String },
    Replace { path: String, value: Value },
}

/// Generate RFC 6902 JSON Patch diff between two JSON values.
pub fn generate_json_patch(from: &Value, to: &Value) -> Vec<JsonPatchOp> {
    let mut ops = Vec::new();
    diff_values("", from, to, &mut ops);
    ops
}

fn diff_values(path: &str, from: &Value, to: &Value, ops: &mut Vec<JsonPatchOp>) {
    if from == to {
        return;
    }

    match (from, to) {
        (Value::Object(map_from), Value::Object(map_to)) => {
            // Check keys in from that were removed or changed
            for (k, v_from) in map_from {
                let current_path = format!("{}/{}", path, escape_json_pointer(k));
                if let Some(v_to) = map_to.get(k) {
                    diff_values(&current_path, v_from, v_to, ops);
                } else {
                    ops.push(JsonPatchOp::Remove { path: current_path });
                }
            }
            // Check keys added in to
            for (k, v_to) in map_to {
                if !map_from.contains_key(k) {
                    let current_path = format!("{}/{}", path, escape_json_pointer(k));
                    ops.push(JsonPatchOp::Add {
                        path: current_path,
                        value: v_to.clone(),
                    });
                }
            }
        }
        (Value::Array(arr_from), Value::Array(arr_to)) => {
            let min_len = arr_from.len().min(arr_to.len());
            for i in 0..min_len {
                let current_path = format!("{}/{}", path, i);
                diff_values(&current_path, &arr_from[i], &arr_to[i], ops);
            }
            if arr_to.len() > arr_from.len() {
                for i in min_len..arr_to.len() {
                    let current_path = format!("{}/{}", path, i);
                    ops.push(JsonPatchOp::Add {
                        path: current_path,
                        value: arr_to[i].clone(),
                    });
                }
            } else if arr_from.len() > arr_to.len() {
                for i in (min_len..arr_from.len()).rev() {
                    let current_path = format!("{}/{}", path, i);
                    ops.push(JsonPatchOp::Remove { path: current_path });
                }
            }
        }
        _ => {
            ops.push(JsonPatchOp::Replace {
                path: if path.is_empty() { "".to_string() } else { path.to_string() },
                value: to.clone(),
            });
        }
    }
}

fn escape_json_pointer(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

fn unescape_json_pointer(segment: &str) -> String {
    segment.replace("~1", "/").replace("~0", "~")
}

/// Apply a list of RFC 6902 JSON patch operations to a target JSON value.
pub fn apply_json_patch(target: &Value, patch: &[JsonPatchOp]) -> Result<Value, String> {
    let mut root = target.clone();
    for op in patch {
        match op {
            JsonPatchOp::Add { path, value } => {
                if path.is_empty() {
                    root = value.clone();
                    continue;
                }
                let segments: Vec<&str> = path.split('/').skip(1).collect();
                add_at_path(&mut root, &segments, value.clone())?;
            }
            JsonPatchOp::Remove { path } => {
                if path.is_empty() {
                    root = Value::Null;
                    continue;
                }
                let segments: Vec<&str> = path.split('/').skip(1).collect();
                remove_at_path(&mut root, &segments)?;
            }
            JsonPatchOp::Replace { path, value } => {
                if path.is_empty() {
                    root = value.clone();
                    continue;
                }
                let segments: Vec<&str> = path.split('/').skip(1).collect();
                replace_at_path(&mut root, &segments, value.clone())?;
            }
        }
    }
    Ok(root)
}

fn add_at_path(target: &mut Value, segments: &[&str], value: Value) -> Result<(), String> {
    if segments.is_empty() {
        *target = value;
        return Ok(());
    }

    let key = unescape_json_pointer(segments[0]);
    if segments.len() == 1 {
        match target {
            Value::Object(map) => {
                map.insert(key, value);
                Ok(())
            }
            Value::Array(arr) => {
                if key == "-" {
                    arr.push(value);
                    Ok(())
                } else if let Ok(idx) = key.parse::<usize>() {
                    if idx <= arr.len() {
                        arr.insert(idx, value);
                        Ok(())
                    } else {
                        Err(format!("Array index out of bounds: {}", idx))
                    }
                } else {
                    Err(format!("Invalid array index key: {}", key))
                }
            }
            _ => Err(format!("Cannot add key '{}' to non-container", key)),
        }
    } else {
        match target {
            Value::Object(map) => {
                let entry = map
                    .entry(key)
                    .or_insert_with(|| Value::Object(serde_json::Map::new()));
                add_at_path(entry, &segments[1..], value)
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| format!("Invalid array index '{}'", key))?;
                if idx < arr.len() {
                    add_at_path(&mut arr[idx], &segments[1..], value)
                } else {
                    Err(format!("Array index out of bounds: {}", idx))
                }
            }
            _ => Err("Target is not an object or array".to_string()),
        }
    }
}

fn remove_at_path(target: &mut Value, segments: &[&str]) -> Result<(), String> {
    if segments.is_empty() {
        return Ok(());
    }

    let key = unescape_json_pointer(segments[0]);
    if segments.len() == 1 {
        match target {
            Value::Object(map) => {
                map.remove(&key)
                    .ok_or_else(|| format!("Key '{}' not found for removal", key))?;
                Ok(())
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| format!("Invalid array index '{}'", key))?;
                if idx < arr.len() {
                    arr.remove(idx);
                    Ok(())
                } else {
                    Err(format!("Array index {} out of bounds for removal", idx))
                }
            }
            _ => Err("Cannot remove from non-container".to_string()),
        }
    } else {
        match target {
            Value::Object(map) => {
                let next = map
                    .get_mut(&key)
                    .ok_or_else(|| format!("Path key '{}' not found", key))?;
                remove_at_path(next, &segments[1..])
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| format!("Invalid array index '{}'", key))?;
                if idx < arr.len() {
                    remove_at_path(&mut arr[idx], &segments[1..])
                } else {
                    Err(format!("Array index {} out of bounds", idx))
                }
            }
            _ => Err("Target is not an object or array".to_string()),
        }
    }
}

fn replace_at_path(target: &mut Value, segments: &[&str], value: Value) -> Result<(), String> {
    if segments.is_empty() {
        *target = value;
        return Ok(());
    }

    let key = unescape_json_pointer(segments[0]);
    if segments.len() == 1 {
        match target {
            Value::Object(map) => {
                if map.contains_key(&key) {
                    map.insert(key, value);
                    Ok(())
                } else {
                    Err(format!("Key '{}' not found for replace", key))
                }
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| format!("Invalid array index '{}'", key))?;
                if idx < arr.len() {
                    arr[idx] = value;
                    Ok(())
                } else {
                    Err(format!("Array index {} out of bounds for replace", idx))
                }
            }
            _ => Err("Cannot replace in non-container".to_string()),
        }
    } else {
        match target {
            Value::Object(map) => {
                let next = map
                    .get_mut(&key)
                    .ok_or_else(|| format!("Path key '{}' not found", key))?;
                replace_at_path(next, &segments[1..], value)
            }
            Value::Array(arr) => {
                let idx: usize = key
                    .parse()
                    .map_err(|_| format!("Invalid array index '{}'", key))?;
                if idx < arr.len() {
                    replace_at_path(&mut arr[idx], &segments[1..], value)
                } else {
                    Err(format!("Array index {} out of bounds", idx))
                }
            }
            _ => Err("Target is not an object or array".to_string()),
        }
    }
}

/// Metadata record for stored step checkpoints.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CheckpointRecord {
    pub thread_id: String,
    pub step: usize,
    pub checkpoint_node: String,
    pub status: String,
    pub created_at: i64,
    pub has_diff: bool,
}

/// Asynchronous checkpoint persistence trait for state graph execution.
#[async_trait::async_trait]
pub trait Checkpointer<S>: Send + Sync {
    async fn save_checkpoint(
        &self,
        thread_id: &str,
        step: usize,
        state: &S,
        node: &str,
        diff_data: Option<&str>,
        tool_outputs: Option<&Value>,
        status: Option<&str>,
    ) -> Result<(), String>;

    async fn load_checkpoint(&self, thread_id: &str, step: usize) -> Result<Option<S>, String>;
    async fn load_latest(&self, thread_id: &str) -> Result<Option<(usize, S)>, String>;
    async fn record_tool_output(
        &self,
        thread_id: &str,
        step: usize,
        tool_call_id: &str,
        output: &Value,
    ) -> Result<(), String>;
    async fn get_cached_tool_output(
        &self,
        thread_id: &str,
        step: usize,
        tool_call_id: &str,
    ) -> Result<Option<Value>, String>;
    async fn list_checkpoints(&self, thread_id: &str) -> Result<Vec<CheckpointRecord>, String>;
    async fn restore_time_travel(&self, thread_id: &str, target_step: usize) -> Result<S, String>;
}

/// SQLite-backed checkpointer with AES-256-GCM encryption and differential JSON diff storage.
pub struct SqliteCheckpointer {
    db: Arc<DatabasePool>,
    crypto: EncryptionEngine,
}

impl SqliteCheckpointer {
    pub fn new(db: Arc<DatabasePool>, crypto: EncryptionEngine) -> Self {
        Self { db, crypto }
    }

    /// Convenience wrapper for legacy single-step saving.
    pub async fn save_checkpoint_legacy(&self, thread_id: &str, state: &AgentState) -> Result<(), String> {
        let step = state.execution_step;
        let node = if state.current_node.is_empty() {
            "START"
        } else {
            &state.current_node
        };
        self.save_checkpoint(thread_id, step, state, node, None, None, Some("ACTIVE"))
            .await
    }

    /// Convenience wrapper for legacy loading latest state.
    pub async fn load_checkpoint_legacy(&self, thread_id: &str) -> Result<Option<AgentState>, String> {
        self.load_latest(thread_id).await.map(|opt| opt.map(|(_, s)| s))
    }
}

#[async_trait::async_trait]
impl Checkpointer<AgentState> for SqliteCheckpointer {
    async fn save_checkpoint(
        &self,
        thread_id: &str,
        step: usize,
        state: &AgentState,
        node: &str,
        diff_data: Option<&str>,
        tool_outputs: Option<&Value>,
        status: Option<&str>,
    ) -> Result<(), String> {
        let pool = self.db.clone();
        let crypto = self.crypto.clone();
        let tid = thread_id.to_string();
        let st = state.clone();
        let node_str = node.to_string();
        let diff_str = diff_data.map(|s| s.to_string());
        let tool_out_str = tool_outputs.map(|v| serde_json::to_string(v).unwrap_or_default());
        let status_str = status.unwrap_or("ACTIVE").to_string();
        let now = Utc::now().timestamp_millis();

        tokio::task::spawn_blocking(move || {
            let conn = pool.writer.get().map_err(|e| e.to_string())?;
            let state_json = serde_json::to_string(&st).map_err(|e| e.to_string())?;
            let encrypted_state = crypto.encrypt(&state_json)?;

            conn.execute(
                "INSERT INTO agent_checkpoints (thread_id, step, state_data, state_json, diff_data, tool_outputs, checkpoint_node, status, created_at) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9) \
                 ON CONFLICT(thread_id, step) DO UPDATE SET \
                 state_data = excluded.state_data, \
                 state_json = excluded.state_json, \
                 diff_data = excluded.diff_data, \
                 tool_outputs = excluded.tool_outputs, \
                 checkpoint_node = excluded.checkpoint_node, \
                 status = excluded.status, \
                 created_at = excluded.created_at",
                rusqlite::params![
                    tid,
                    step as i64,
                    encrypted_state,
                    encrypted_state,
                    diff_str,
                    tool_out_str,
                    node_str,
                    status_str,
                    now,
                ],
            )
            .map_err(|e| e.to_string())?;

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn load_checkpoint(&self, thread_id: &str, step: usize) -> Result<Option<AgentState>, String> {
        let pool = self.db.clone();
        let crypto = self.crypto.clone();
        let tid = thread_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT COALESCE(NULLIF(state_data, ''), state_json) \
                     FROM agent_checkpoints WHERE thread_id = ?1 AND step = ?2",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![tid, step as i64])
                .map_err(|e| e.to_string())?;

            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let stored: String = row.get(0).map_err(|e| e.to_string())?;
                let state_json = match crypto.read_fact(&stored) {
                    FactRead::Ok(plain) => plain,
                    FactRead::Locked { reason } => {
                        return Err(format!(
                            "Checkpoint locked ({reason}); valid LIVA_ENCRYPTION_KEY required"
                        ));
                    }
                };
                let state: AgentState =
                    serde_json::from_str(&state_json).map_err(|e| e.to_string())?;
                Ok(Some(state))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn load_latest(&self, thread_id: &str) -> Result<Option<(usize, AgentState)>, String> {
        let pool = self.db.clone();
        let crypto = self.crypto.clone();
        let tid = thread_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT step, COALESCE(NULLIF(state_data, ''), state_json) FROM agent_checkpoints \
                     WHERE thread_id = ?1 ORDER BY step DESC LIMIT 1",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![tid])
                .map_err(|e| e.to_string())?;

            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let step: i64 = row.get(0).map_err(|e| e.to_string())?;
                let stored: String = row.get(1).map_err(|e| e.to_string())?;
                let state_json = match crypto.read_fact(&stored) {
                    FactRead::Ok(plain) => plain,
                    FactRead::Locked { reason } => {
                        return Err(format!(
                            "Checkpoint locked ({reason}); valid LIVA_ENCRYPTION_KEY required"
                        ));
                    }
                };
                let state: AgentState =
                    serde_json::from_str(&state_json).map_err(|e| e.to_string())?;
                Ok(Some((step as usize, state)))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn record_tool_output(
        &self,
        thread_id: &str,
        step: usize,
        tool_call_id: &str,
        output: &Value,
    ) -> Result<(), String> {
        let pool = self.db.clone();
        let tid = thread_id.to_string();
        let t_id = tool_call_id.to_string();
        let out_val = output.clone();

        tokio::task::spawn_blocking(move || {
            let conn = pool.writer.get().map_err(|e| e.to_string())?;
            let existing_raw: Option<String> = conn
                .query_row(
                    "SELECT tool_outputs FROM agent_checkpoints WHERE thread_id = ?1 AND step = ?2",
                    rusqlite::params![tid, step as i64],
                    |row| row.get(0),
                )
                .ok();

            let mut map: HashMap<String, Value> = existing_raw
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default();

            map.insert(t_id, out_val);
            let updated_json = serde_json::to_string(&map).map_err(|e| e.to_string())?;

            conn.execute(
                "UPDATE agent_checkpoints SET tool_outputs = ?1 WHERE thread_id = ?2 AND step = ?3",
                rusqlite::params![updated_json, tid, step as i64],
            )
            .map_err(|e| e.to_string())?;

            Ok::<(), String>(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn get_cached_tool_output(
        &self,
        thread_id: &str,
        step: usize,
        tool_call_id: &str,
    ) -> Result<Option<Value>, String> {
        let pool = self.db.clone();
        let tid = thread_id.to_string();
        let t_id = tool_call_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;
            let raw: Option<String> = conn
                .query_row(
                    "SELECT tool_outputs FROM agent_checkpoints WHERE thread_id = ?1 AND step = ?2",
                    rusqlite::params![tid, step as i64],
                    |row| row.get(0),
                )
                .ok();

            if let Some(raw_str) = raw {
                let map: HashMap<String, Value> = serde_json::from_str(&raw_str).unwrap_or_default();
                Ok(map.get(&t_id).cloned())
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn list_checkpoints(&self, thread_id: &str) -> Result<Vec<CheckpointRecord>, String> {
        let pool = self.db.clone();
        let tid = thread_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;
            let mut stmt = conn
                .prepare(
                    "SELECT thread_id, step, checkpoint_node, status, created_at, diff_data IS NOT NULL \
                     FROM agent_checkpoints WHERE thread_id = ?1 ORDER BY step ASC",
                )
                .map_err(|e| e.to_string())?;

            let rows = stmt
                .query_map(rusqlite::params![tid], |row| {
                    Ok(CheckpointRecord {
                        thread_id: row.get(0)?,
                        step: row.get::<_, i64>(1)? as usize,
                        checkpoint_node: row.get(2)?,
                        status: row.get(3)?,
                        created_at: row.get(4)?,
                        has_diff: row.get(5)?,
                    })
                })
                .map_err(|e| e.to_string())?;

            let mut result = Vec::new();
            for r in rows {
                result.push(r.map_err(|e| e.to_string())?);
            }
            Ok(result)
        })
        .await
        .map_err(|e| e.to_string())?
    }

    async fn restore_time_travel(&self, thread_id: &str, target_step: usize) -> Result<AgentState, String> {
        let pool = self.db.clone();
        let crypto = self.crypto.clone();
        let tid = thread_id.to_string();

        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;

            // 1. Find nearest base checkpoint step <= target_step
            // A base checkpoint is one that does not have diff_data or is the direct base snapshot
            let mut stmt = conn
                .prepare(
                    "SELECT step, COALESCE(NULLIF(state_data, ''), state_json) FROM agent_checkpoints \
                     WHERE thread_id = ?1 AND step <= ?2 \
                     ORDER BY step DESC",
                )
                .map_err(|e| e.to_string())?;

            let mut rows = stmt
                .query(rusqlite::params![tid, target_step as i64])
                .map_err(|e| e.to_string())?;

            let base_step_found = if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let s: i64 = row.get(0).map_err(|e| e.to_string())?;
                let data: String = row.get(1).map_err(|e| e.to_string())?;
                Some((s as usize, data))
            } else {
                None
            };

            let (base_step, base_encrypted) = base_step_found
                .ok_or_else(|| format!("No base checkpoint found for thread '{}' at or before step {}", tid, target_step))?;

            // Decrypt base state
            let base_plain = match crypto.read_fact(&base_encrypted) {
                FactRead::Ok(plain) => plain,
                FactRead::Locked { reason } => {
                    return Err(format!("Checkpoint locked ({reason}); key required"));
                }
            };
            let mut current_state_val: Value =
                serde_json::from_str(&base_plain).map_err(|e| e.to_string())?;

            // 2. If target_step > base_step, fetch intermediate diffs in ascending order and apply them
            if target_step > base_step {
                let mut diff_stmt = conn
                    .prepare(
                        "SELECT step, diff_data FROM agent_checkpoints \
                         WHERE thread_id = ?1 AND step > ?2 AND step <= ?3 \
                         ORDER BY step ASC",
                    )
                    .map_err(|e| e.to_string())?;

                let diff_rows = diff_stmt
                    .query_map(
                        rusqlite::params![tid, base_step as i64, target_step as i64],
                        |row| Ok((row.get::<_, i64>(0)? as usize, row.get::<_, Option<String>>(1)?)),
                    )
                    .map_err(|e| e.to_string())?;

                for r in diff_rows {
                    let (_s, diff_opt) = r.map_err(|e| e.to_string())?;
                    if let Some(diff_json) = diff_opt {
                        let patch: Vec<JsonPatchOp> =
                            serde_json::from_str(&diff_json).map_err(|e| e.to_string())?;
                        current_state_val = apply_json_patch(&current_state_val, &patch)?;
                    }
                }
            }

            let reconstructed: AgentState =
                serde_json::from_value(current_state_val).map_err(|e| e.to_string())?;
            Ok(reconstructed)
        })
        .await
        .map_err(|e| e.to_string())?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::EncryptionEngine;
    use crate::db::DatabasePool;
    use serde_json::json;

    fn pool() -> Arc<DatabasePool> {
        Arc::new(DatabasePool::new_in_memory().expect("in-memory db"))
    }

    fn crypto() -> EncryptionEngine {
        EncryptionEngine::new("checkpoint-diff-key-32-bytes-long")
    }

    #[test]
    fn test_rfc_6902_json_diff_and_apply_basic() {
        let v1 = json!({
            "name": "LIVA",
            "active": true,
            "count": 10,
            "tags": ["ai", "fast"]
        });

        let v2 = json!({
            "name": "LIVA Core",
            "active": true,
            "count": 15,
            "tags": ["ai", "fast", "native"],
            "extra": "new_field"
        });

        let patch = generate_json_patch(&v1, &v2);
        assert!(!patch.is_empty());

        let applied = apply_json_patch(&v1, &patch).expect("apply patch");
        assert_eq!(applied, v2);
    }

    #[test]
    fn test_rfc_6902_json_diff_and_apply_complex_nesting() {
        let v1 = json!({
            "messages": [
                {"role": "system", "content": "persona"},
                {"role": "user", "content": "hello"}
            ],
            "context": {
                "step": 1,
                "vars": {"a": 1, "b": 2}
            }
        });

        let v2 = json!({
            "messages": [
                {"role": "system", "content": "persona"},
                {"role": "user", "content": "hello"},
                {"role": "assistant", "content": "hi there"}
            ],
            "context": {
                "step": 2,
                "vars": {"a": 1, "c": 3}
            }
        });

        let patch = generate_json_patch(&v1, &v2);
        let applied = apply_json_patch(&v1, &patch).expect("apply patch");
        assert_eq!(applied, v2);
    }

    #[tokio::test]
    async fn test_sqlite_checkpointer_composite_step_and_time_travel() {
        let db = pool();
        let enc = crypto();
        let cp = SqliteCheckpointer::new(db.clone(), enc);

        let mut st0 = AgentState::default();
        st0.current_node = "router".to_string();
        st0.messages.push(json!({"role": "user", "content": "Step 0"}));
        st0.execution_step = 0;

        let mut st1 = st0.clone();
        st1.current_node = "planner".to_string();
        st1.messages.push(json!({"role": "assistant", "content": "Step 1"}));
        st1.execution_step = 1;

        let mut st2 = st1.clone();
        st2.current_node = "executor".to_string();
        st2.messages.push(json!({"role": "tool", "content": "Step 2 output"}));
        st2.execution_step = 2;

        // Save step 0 as base
        cp.save_checkpoint("thread-tt", 0, &st0, "router", None, None, Some("ACTIVE"))
            .await
            .expect("save step 0");

        // Save step 1 with diff
        let patch1 = generate_json_patch(&serde_json::to_value(&st0).unwrap(), &serde_json::to_value(&st1).unwrap());
        let patch1_str = serde_json::to_string(&patch1).unwrap();
        cp.save_checkpoint("thread-tt", 1, &st1, "planner", Some(&patch1_str), None, Some("ACTIVE"))
            .await
            .expect("save step 1");

        // Save step 2 with diff
        let patch2 = generate_json_patch(&serde_json::to_value(&st1).unwrap(), &serde_json::to_value(&st2).unwrap());
        let patch2_str = serde_json::to_string(&patch2).unwrap();
        cp.save_checkpoint("thread-tt", 2, &st2, "executor", Some(&patch2_str), None, Some("ACTIVE"))
            .await
            .expect("save step 2");

        // Test list checkpoints
        let history = cp.list_checkpoints("thread-tt").await.expect("list");
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].step, 0);
        assert_eq!(history[1].step, 1);
        assert_eq!(history[2].step, 2);

        // Test Time-Travel State Reconstruction
        let restored0 = cp.restore_time_travel("thread-tt", 0).await.expect("restore step 0");
        assert_eq!(restored0.messages.len(), 1);
        assert_eq!(restored0.current_node, "router");

        let restored1 = cp.restore_time_travel("thread-tt", 1).await.expect("restore step 1");
        assert_eq!(restored1.messages.len(), 2);
        assert_eq!(restored1.current_node, "planner");

        let restored2 = cp.restore_time_travel("thread-tt", 2).await.expect("restore step 2");
        assert_eq!(restored2.messages.len(), 3);
        assert_eq!(restored2.current_node, "executor");
    }

    #[tokio::test]
    async fn test_tool_output_replay_cache() {
        let db = pool();
        let enc = crypto();
        let cp = SqliteCheckpointer::new(db.clone(), enc);

        let st = AgentState::default();
        cp.save_checkpoint("thread-cache", 1, &st, "tool_node", None, None, Some("ACTIVE"))
            .await
            .unwrap();

        // Record tool output
        let tool_result = json!({"status": "success", "file_created": "/tmp/a.txt"});
        cp.record_tool_output("thread-cache", 1, "call_abc_1", &tool_result)
            .await
            .unwrap();

        // Get cached tool output
        let cached = cp
            .get_cached_tool_output("thread-cache", 1, "call_abc_1")
            .await
            .unwrap();
        assert_eq!(cached, Some(tool_result));

        // Unknown call returns None
        let missing = cp
            .get_cached_tool_output("thread-cache", 1, "call_unknown")
            .await
            .unwrap();
        assert_eq!(missing, None);
    }
}
