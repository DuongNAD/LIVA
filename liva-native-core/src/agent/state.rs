use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct AgentState {
    pub messages: Vec<Value>, // Can hold chat messages or tool calls
    pub current_node: String,
    pub context: HashMap<String, Value>,
}
