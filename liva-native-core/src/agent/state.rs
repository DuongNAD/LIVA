use super::plan::TaskPlan;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Maximum number of keys allowed in the working memory scratchpad to prevent unbounded growth.
pub const MAX_SCRATCHPAD_ENTRIES: usize = 64;

/// Số tin nhắn tối đa giữ lại trong lịch sử hội thoại, KHÔNG kể tin `system`.
/// Đặt qua `LIVA_MAX_HISTORY_MESSAGES` (mặc định 20 ≈ 10 lượt hỏi–đáp).
///
/// Vì sao cần: `compile_prompt` nhét TOÀN BỘ `messages` vào prompt, còn
/// `prune_kv_cache` chỉ chạy trong vòng sinh token chứ không chạy lúc prefill
/// (`llm/engine.rs`). Không cắt thì prompt sẽ vượt `n_ctx` (mặc định 4096) sau
/// vài chục lượt và `decode()` hỏng.
pub fn max_history_messages() -> usize {
    std::env::var("LIVA_MAX_HISTORY_MESSAGES")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(20)
}

#[derive(Debug, Serialize, Deserialize, Clone, Default, PartialEq)]
pub struct AgentState {
    pub messages: Vec<Value>, // Can hold chat messages or tool calls
    pub current_node: String,
    pub context: HashMap<String, Value>,
    /// Hierarchical Working Memory: Structured multi-step task plan (M3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_plan: Option<TaskPlan>,
    /// Active plan step pointer.
    #[serde(default)]
    pub active_step_index: usize,
    /// Intermediate step execution outputs stored by step ID.
    #[serde(default)]
    pub step_outputs: HashMap<String, Value>,
    /// Working memory scratchpad for intermediate observations and reasoning state.
    #[serde(default)]
    pub scratchpad: HashMap<String, Value>,
    /// Execution step counter in the cyclic state graph DAG.
    #[serde(default)]
    pub execution_step: usize,
    /// History of visited nodes in current DAG path.
    #[serde(default)]
    pub visited_nodes: Vec<String>,
    /// Parent checkpoint ID for branch tracking.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_checkpoint_id: Option<String>,
    /// Active execution branch identifier for parallel branching.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branch_id: Option<String>,
}

impl AgentState {
    /// Record a visited node in DAG execution history.
    pub fn record_node_visit(&mut self, node: impl Into<String>) {
        let n = node.into();
        self.current_node = n.clone();
        self.visited_nodes.push(n);
    }

    /// Advance execution step counter.
    pub fn increment_step(&mut self) -> usize {
        self.execution_step += 1;
        self.execution_step
    }

    /// Set execution branch identifier.
    pub fn set_branch(&mut self, branch: impl Into<String>) {
        self.branch_id = Some(branch.into());
    }
    /// Cắt bớt lịch sử tại chỗ: luôn giữ tin `system` đầu tiên (persona) và
    /// `max_history_messages()` tin gần nhất.
    ///
    /// Gọi ở hai chỗ và cả hai đều cần thiết:
    /// - trước khi dựng prompt (`agent/graph.rs`) — để prompt không vượt `n_ctx`;
    /// - sau khi thêm câu trả lời (`agent/graph.rs`) — để checkpoint ghi xuống
    ///   `agent_checkpoints` không phình vô hạn theo thời gian.
    ///
    /// Đây là chốt chặn theo SỐ TIN NHẮN, không phải theo số token. Guard cứng
    /// theo token nằm ở `LlamaRouterManager::generate_completion`.
    pub fn trim_history(&mut self) {
        trim_messages(&mut self.messages);
    }

    /// Set an active task plan for working memory execution.
    pub fn set_plan(&mut self, plan: TaskPlan) {
        self.active_step_index = plan.current_step;
        self.active_plan = Some(plan);
    }

    /// Retrieve active plan reference.
    pub fn get_plan(&self) -> Option<&TaskPlan> {
        self.active_plan.as_ref()
    }

    /// Retrieve mutable active plan reference.
    pub fn get_plan_mut(&mut self) -> Option<&mut TaskPlan> {
        self.active_plan.as_mut()
    }

    /// Record an intermediate step output into working memory.
    pub fn record_step_output(&mut self, step_id: impl Into<String>, output: Value) {
        self.step_outputs.insert(step_id.into(), output);
    }

    /// Store a key-value pair in the bounded working memory scratchpad.
    /// If capacity is exceeded, trims earliest entries to preserve bounds.
    pub fn scratchpad_set(&mut self, key: impl Into<String>, value: Value) {
        let key_str = key.into();
        if self.scratchpad.len() >= MAX_SCRATCHPAD_ENTRIES && !self.scratchpad.contains_key(&key_str) {
            if let Some(first_key) = self.scratchpad.keys().min().cloned() {
                self.scratchpad.remove(&first_key);
            }
        }
        self.scratchpad.insert(key_str, value);
    }

    /// Read a value from the working memory scratchpad.
    pub fn scratchpad_get(&self, key: &str) -> Option<&Value> {
        self.scratchpad.get(key)
    }

    /// Remove a value from the scratchpad.
    pub fn scratchpad_remove(&mut self, key: &str) -> Option<Value> {
        self.scratchpad.remove(key)
    }

    /// Clear scratchpad for a new goal.
    pub fn clear_scratchpad(&mut self) {
        self.scratchpad.clear();
    }
}

/// Bản dùng được trên `Vec<Value>` trần, cho nơi chưa có `AgentState`.
pub fn trim_messages(messages: &mut Vec<Value>) {
    let cap = max_history_messages();

    let system_msg = messages
        .first()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
        .cloned();
    let body_start = usize::from(system_msg.is_some());

    if messages.len() - body_start <= cap {
        return;
    }

    let keep_from = messages.len() - cap;
    let tail: Vec<Value> = messages[keep_from..].to_vec();

    messages.clear();
    if let Some(sys) = system_msg {
        messages.push(sys);
    }
    messages.extend(tail);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::plan::PlanStep;
    use serde_json::json;

    fn msgs(n: usize, with_system: bool) -> Vec<Value> {
        let mut v = Vec::new();
        if with_system {
            v.push(json!({"role": "system", "content": "persona"}));
        }
        for i in 0..n {
            v.push(json!({"role": "user", "content": format!("m{}", i)}));
        }
        v
    }

    #[test]
    fn trim_giu_nguyen_khi_chua_vuot_nguong() {
        let mut v = msgs(5, true);
        let before = v.clone();
        trim_messages(&mut v);
        assert_eq!(v, before, "dưới ngưỡng thì không được đụng vào");
    }

    #[test]
    fn trim_cat_bot_va_giu_system() {
        let mut v = msgs(50, true);
        trim_messages(&mut v);
        assert_eq!(v.len(), 21, "1 system + 20 tin gần nhất");
        assert_eq!(v[0]["role"], "system", "tin system phải được giữ lại");
        assert_eq!(v[1]["content"], "m30", "phải giữ đúng 20 tin CUỐI");
        assert_eq!(v[20]["content"], "m49", "tin mới nhất không được mất");
    }

    #[test]
    fn trim_khong_co_system() {
        let mut v = msgs(50, false);
        trim_messages(&mut v);
        assert_eq!(v.len(), 20);
        assert_eq!(v[0]["content"], "m30");
        assert_eq!(v[19]["content"], "m49");
    }

    #[test]
    fn trim_dung_ngay_tai_nguong() {
        let mut v = msgs(20, true);
        let before = v.clone();
        trim_messages(&mut v);
        assert_eq!(v, before, "đúng bằng ngưỡng thì chưa cắt");

        let mut v = msgs(21, true);
        trim_messages(&mut v);
        assert_eq!(v.len(), 21, "vượt 1 tin thì cắt còn system + 20");
        assert_eq!(v[0]["role"], "system");
        assert_eq!(v[1]["content"], "m1", "tin cũ nhất bị loại là m0");
    }

    #[test]
    fn trim_rong_va_chi_co_system() {
        let mut v: Vec<Value> = Vec::new();
        trim_messages(&mut v);
        assert!(v.is_empty(), "danh sách rỗng không được panic");

        let mut v = msgs(0, true);
        trim_messages(&mut v);
        assert_eq!(v.len(), 1, "chỉ có system thì giữ nguyên");
    }

    #[test]
    fn trim_history_tren_agent_state() {
        let mut st = AgentState {
            messages: msgs(50, true),
            current_node: "router".to_string(),
            context: HashMap::new(),
            active_plan: None,
            active_step_index: 0,
            step_outputs: HashMap::new(),
            scratchpad: HashMap::new(),
            ..Default::default()
        };
        st.trim_history();
        assert_eq!(st.messages.len(), 21);
        assert_eq!(st.messages[0]["role"], "system");
        assert_eq!(st.current_node, "router", "trim không được đụng field khác");
    }

    #[test]
    fn trim_lap_lai_la_idempotent() {
        let mut v = msgs(50, true);
        trim_messages(&mut v);
        let once = v.clone();
        trim_messages(&mut v);
        assert_eq!(v, once, "gọi hai lần phải cho cùng kết quả");
    }

    #[test]
    fn test_working_memory_state_plan_and_scratchpad() {
        let mut state = AgentState::default();
        let plan = TaskPlan::new(
            "Configure Smart Home",
            vec![
                PlanStep::new("s1", "Check lighting state"),
                PlanStep::new("s2", "Turn on living room light"),
            ],
        );

        state.set_plan(plan);
        assert!(state.get_plan().is_some());
        assert_eq!(state.active_step_index, 0);

        state.record_step_output("s1", json!({"light": "off"}));
        assert_eq!(state.step_outputs.get("s1"), Some(&json!({"light": "off"})));

        state.scratchpad_set("user_intent", json!("evening_relaxation"));
        assert_eq!(
            state.scratchpad_get("user_intent"),
            Some(&json!("evening_relaxation"))
        );

        // Serialization roundtrip compatibility test
        let serialized = serde_json::to_string(&state).expect("serialize");
        let deserialized: AgentState = serde_json::from_str(&serialized).expect("deserialize");
        assert_eq!(state, deserialized);
    }
}
