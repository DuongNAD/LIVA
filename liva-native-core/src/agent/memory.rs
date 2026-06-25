use super::state::AgentState;
use crate::db::DatabasePool;
use std::sync::Arc;

pub struct SqliteCheckpointer {
    db: Arc<DatabasePool>,
}

impl SqliteCheckpointer {
    pub fn new(db: Arc<DatabasePool>) -> Self {
        Self { db }
    }

    pub async fn save_checkpoint(&self, thread_id: &str, state: &AgentState) -> Result<(), String> {
        let pool = self.db.clone();
        let tid = thread_id.to_string();
        let st = state.clone();
        
        tokio::task::spawn_blocking(move || {
            let conn = pool.writer.get().map_err(|e| e.to_string())?;
            let state_json = serde_json::to_string(&st).map_err(|e| e.to_string())?;
            
            conn.execute(
                "INSERT OR REPLACE INTO agent_checkpoints (thread_id, state_json) VALUES (?1, ?2)",
                rusqlite::params![tid, state_json],
            ).map_err(|e| e.to_string())?;
            
            Ok::<(), String>(())
        })
        .await
        .map_err(|e| e.to_string())?
    }

    pub async fn load_checkpoint(&self, thread_id: &str) -> Result<Option<AgentState>, String> {
        let pool = self.db.clone();
        let tid = thread_id.to_string();
        
        tokio::task::spawn_blocking(move || {
            let conn = pool.readers.get().map_err(|e| e.to_string())?;
            let mut stmt = conn.prepare("SELECT state_json FROM agent_checkpoints WHERE thread_id = ?1")
                .map_err(|e| e.to_string())?;
                
            let mut rows = stmt.query(rusqlite::params![tid]).map_err(|e| e.to_string())?;
            
            if let Some(row) = rows.next().map_err(|e| e.to_string())? {
                let state_json: String = row.get(0).map_err(|e| e.to_string())?;
                let state: AgentState = serde_json::from_str(&state_json).map_err(|e| e.to_string())?;
                Ok(Some(state))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| e.to_string())?
    }
}
