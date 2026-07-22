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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::DatabasePool;
    use serde_json::json;

    fn pool() -> Arc<DatabasePool> {
        Arc::new(DatabasePool::new_in_memory().expect("dựng DB in-memory"))
    }

    fn state(node: &str, user_msg: &str) -> AgentState {
        let mut st = AgentState {
            messages: vec![
                json!({"role": "system", "content": "persona"}),
                json!({"role": "user", "content": user_msg}),
            ],
            current_node: node.to_string(),
            context: Default::default(),
        };
        st.context
            .insert("mood".to_string(), json!("vui"));
        st
    }

    /// Hợp đồng nền của trí nhớ đa lượt: ghi rồi đọc lại **cùng** `thread_id`
    /// phải khôi phục nguyên trạng AgentState.
    #[tokio::test]
    async fn checkpoint_round_trip_cung_thread_id() {
        let cp = SqliteCheckpointer::new(pool());
        let st = state("router", "chào LIVA");

        cp.save_checkpoint("conv-abc", &st).await.unwrap();
        let loaded = cp.load_checkpoint("conv-abc").await.unwrap()
            .expect("cùng thread_id phải đọc lại được");

        assert_eq!(loaded.messages, st.messages, "messages phải khôi phục đúng");
        assert_eq!(loaded.current_node, "router");
        assert_eq!(loaded.context.get("mood"), Some(&json!("vui")), "context phải giữ");
    }

    /// KHOÁ HỒI QUY cho bug 2.1: đây là chính lý do KHÔNG được dùng `session_id`
    /// (tăng mỗi lượt VAD) làm khoá checkpoint. Ghi dưới một khoá, đọc bằng khoá
    /// KHÁC (mô phỏng session_id 1,2,3… mỗi câu nói) → luôn `None` → trợ lý mất
    /// sạch trí nhớ đa lượt. `conversation_id` ổn định theo kết nối tránh đúng ca này.
    #[tokio::test]
    async fn thread_id_khac_tra_none_dung_bug_2_1() {
        let cp = SqliteCheckpointer::new(pool());
        cp.save_checkpoint("conversation-cố-định", &state("llm", "câu 1"))
            .await
            .unwrap();

        // session_id đổi mỗi câu → mỗi lần là một khoá khác → không thấy gì.
        for sid in ["1", "2", "3"] {
            assert!(
                cp.load_checkpoint(sid).await.unwrap().is_none(),
                "khoá đổi mỗi lượt (session_id={sid}) PHẢI trượt checkpoint — đây là bug 2.1"
            );
        }
        // Còn khoá ổn định thì vẫn đọc lại được.
        assert!(cp.load_checkpoint("conversation-cố-định").await.unwrap().is_some());
    }

    /// Ghi lại cùng `thread_id` phải ĐÈ (INSERT OR REPLACE), không nhân bản dòng.
    #[tokio::test]
    async fn save_cung_thread_id_ghi_de() {
        let cp = SqliteCheckpointer::new(pool());
        cp.save_checkpoint("t1", &state("router", "cũ")).await.unwrap();
        cp.save_checkpoint("t1", &state("llm", "mới")).await.unwrap();

        let loaded = cp.load_checkpoint("t1").await.unwrap().unwrap();
        assert_eq!(loaded.current_node, "llm", "bản ghi sau phải đè bản trước");
        assert_eq!(loaded.messages[1]["content"], "mới");
    }

    /// Đọc khi chưa từng ghi → `None`, không lỗi.
    #[tokio::test]
    async fn load_khi_chua_co_gi_tra_none() {
        let cp = SqliteCheckpointer::new(pool());
        assert!(cp.load_checkpoint("chưa-tồn-tại").await.unwrap().is_none());
    }
}
