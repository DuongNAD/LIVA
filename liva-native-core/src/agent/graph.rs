use super::state::AgentState;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use serde_json::json;
use crate::AppState;

pub type NodeFuture = Pin<Box<dyn Future<Output = Result<AgentState, String>> + Send>>;
pub type NodeFn = Box<dyn Fn(AgentState) -> NodeFuture + Send + Sync>;

pub struct StateGraph {
    nodes: HashMap<String, NodeFn>,
    edges: HashMap<String, String>,
    entry_point: String,
}

impl StateGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: "START".to_string(),
        }
    }

    pub fn add_node<F, Fut>(&mut self, name: &str, node: F)
    where
        F: Fn(AgentState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<AgentState, String>> + Send + 'static,
    {
        let wrapped = move |state| {
            let fut = node(state);
            Box::pin(fut) as NodeFuture
        };
        self.nodes.insert(name.to_string(), Box::new(wrapped));
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.insert(from.to_string(), to.to_string());
    }

    pub fn set_entry_point(&mut self, node: &str) {
        self.entry_point = node.to_string();
    }

    pub async fn run(&self, initial_state: AgentState) -> Result<AgentState, String> {
        let mut state = initial_state;
        if state.current_node.is_empty() || state.current_node == "START" {
            state.current_node = self.entry_point.clone();
        }

        while state.current_node != "__END__" {
            let current = state.current_node.clone();
            let node_fn = self.nodes.get(&current)
                .ok_or_else(|| format!("Node '{}' not found", current))?;
            
            state = node_fn(state.clone()).await?;

            if state.current_node == current {
                if let Some(next) = self.edges.get(&current) {
                    state.current_node = next.clone();
                } else {
                    state.current_node = "__END__".to_string();
                }
            }
        }

        Ok(state)
    }
}

pub fn build_pipeline_graph(
    state_shared: Arc<AppState>,
    llm_chunk_tx: mpsc::Sender<String>,
    session_id: u64,
    active_session_id: Arc<std::sync::atomic::AtomicU64>,
) -> StateGraph {
    let mut graph = StateGraph::new();

    let ss1 = Arc::clone(&state_shared);
    let tx1 = llm_chunk_tx.clone();
    let as1 = Arc::clone(&active_session_id);
    graph.add_node("router", move |mut state: AgentState| {
        let _ss = Arc::clone(&ss1);
        let _tx = tx1.clone();
        let _as = Arc::clone(&as1);
        async move {
            let last_msg = state.messages.last().ok_or("No messages in state")?;
            let text = last_msg.get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("");

            let text_lower = text.to_lowercase();
            let device = if text_lower.contains("light") {
                Some("light")
            } else if text_lower.contains("ac") {
                Some("ac")
            } else if text_lower.contains("fan") {
                Some("fan")
            } else {
                None
            };

            let action = if text_lower.contains("on") {
                Some("on")
            } else if text_lower.contains("off") {
                Some("off")
            } else {
                None
            };

            if let (Some(d), Some(a)) = (device, action) {
                state.context.insert("device".to_string(), json!(d));
                state.context.insert("action".to_string(), json!(a));
                state.current_node = "tool_exec".to_string();
            } else {
                state.current_node = "chat_completion".to_string();
            }

            Ok(state)
        }
    });

    graph.add_node("tool_exec", move |mut state: AgentState| {
        async move {
            let device_val = state.context.get("device").ok_or("device missing in context")?;
            let action_val = state.context.get("action").ok_or("action missing in context")?;
            let payload = json!({
                "device": device_val,
                "action": action_val
            });
            let result = crate::integrations::smart_home::execute(payload);
            let res_msg = match result {
                Ok(msg) => json!({"role": "tool", "content": msg}),
                Err(err) => json!({"role": "tool", "content": format!("Tool execution failed: {}", err)}),
            };
            state.messages.push(res_msg);
            state.current_node = "chat_completion".to_string();
            Ok(state)
        }
    });

    let ss3 = Arc::clone(&state_shared);
    let tx3 = llm_chunk_tx.clone();
    let as3 = Arc::clone(&active_session_id);
    graph.add_node("chat_completion", move |mut state: AgentState| {
        let ss = Arc::clone(&ss3);
        let tx = tx3.clone();
        let as_val = Arc::clone(&as3);
        async move {
            let mut chat_messages = Vec::new();
            for msg in &state.messages {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                chat_messages.push(crate::llm::ChatMessage { role, content });
            }

            // Fallback persona injection: sessions seeded before persona
            // support (e.g. legacy checkpoints) carry no system message.
            if !chat_messages.iter().any(|m| m.role == "system") {
                chat_messages.insert(0, crate::llm::ChatMessage {
                    role: "system".to_string(),
                    content: crate::llm::persona::PERSONA_LIVA.to_string(),
                });
            }

            let prompt = crate::llm::compile_gemma_prompt(&chat_messages)?;

            let res = tokio::task::spawn_blocking(move || {
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("LLM cancelled before lock".to_string());
                }
                let mut llm = ss.llm.blocking_lock();
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("LLM cancelled post-lock".to_string());
                }
                if llm.engine.is_none() {
                    return Err("LLM engine not loaded".to_string());
                }
                let completion = llm.generate_completion(
                    &prompt,
                    crate::llm::persona::TEMP_DEFAULT,
                    crate::llm::persona::TOP_P_DEFAULT,
                    |token| {
                        if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                            return false;
                        }
                        let _ = tx.blocking_send(token.to_string());
                        true
                    },
                )?;
                Ok(completion.text)
            })
            .await
            .map_err(|e| format!("LLM task panicked: {}", e))
            .and_then(|r| r)?;

            state.messages.push(json!({
                "role": "assistant",
                "content": res
            }));
            state.current_node = "__END__".to_string();

            Ok(state)
        }
    });

    graph.set_entry_point("router");
    graph
}
