use std::collections::HashMap;
use std::sync::{Arc, RwLock};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot, Mutex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AgentRole {
    Research,
    Code,
    Review,
    Orchestrator,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentMessage {
    pub message_id: String,
    pub trace_id: String,
    pub from: AgentRole,
    pub to: AgentRole,
    pub content: String,
    pub correlation_id: Option<String>,
}

#[derive(Clone)]
pub struct AgentDispatcher {
    senders: Arc<RwLock<HashMap<AgentRole, mpsc::Sender<AgentMessage>>>>,
}

impl AgentDispatcher {
    pub fn new() -> Self {
        Self {
            senders: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_agent(&self, role: AgentRole, sender: mpsc::Sender<AgentMessage>) {
        let mut senders = self.senders.write().unwrap();
        senders.insert(role, sender);
    }

    pub async fn dispatch(&self, msg: AgentMessage) -> Result<(), String> {
        let sender = {
            let senders = self.senders.read().unwrap();
            senders.get(&msg.to).cloned()
        };
        if let Some(sender) = sender {
            sender.send(msg)
                .await
                .map_err(|e| format!("Failed to send message: {}", e))?;
            Ok(())
        } else {
            Err(format!("Agent with role {:?} not found", msg.to))
        }
    }
}

impl Default for AgentDispatcher {
    fn default() -> Self {
        Self::new()
    }
}

pub struct SwarmAgent {
    role: AgentRole,
    dispatcher: AgentDispatcher,
    receiver: mpsc::Receiver<AgentMessage>,
    pending_replies: Arc<Mutex<HashMap<String, oneshot::Sender<AgentMessage>>>>,
}

impl SwarmAgent {
    pub fn new(role: AgentRole, dispatcher: AgentDispatcher, receiver: mpsc::Receiver<AgentMessage>) -> Self {
        Self {
            role,
            dispatcher,
            receiver,
            pending_replies: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    pub fn start(mut self) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            while let Some(msg) = self.receiver.recv().await {
                let pending_replies = Arc::clone(&self.pending_replies);
                let dispatcher = self.dispatcher.clone();
                let role = self.role;

                tokio::spawn(async move {
                    if let Some(ref corr_id) = msg.correlation_id {
                        let mut pending = pending_replies.lock().await;
                        if let Some(reply_tx) = pending.remove(corr_id) {
                            let _ = reply_tx.send(msg);
                        } else {
                            eprintln!(
                                "[{:?}] Received reply with correlation_id {:?} but it was not found in pending_replies (late or duplicate)",
                                role, corr_id
                            );
                        }
                        return;
                    }

                    if let Err(e) = Self::handle_request(msg, role, dispatcher, pending_replies).await {
                        eprintln!("[{:?}] Error handling message: {}", role, e);
                    }
                });
            }
        })
    }

    async fn handle_request(
        msg: AgentMessage,
        role: AgentRole,
        dispatcher: AgentDispatcher,
        pending_replies: Arc<Mutex<HashMap<String, oneshot::Sender<AgentMessage>>>>,
    ) -> Result<(), String> {
        let reply_content = match role {
            AgentRole::Research => {
                if msg.content.contains("implement") {
                    let code_reply = Self::request_reply_internal(
                        AgentRole::Code,
                        format!("Write draft code for: {}", msg.content),
                        msg.trace_id.clone(),
                        role,
                        dispatcher.clone(),
                        pending_replies,
                    ).await?;
                    format!("Research results: Code completed: {}", code_reply.content)
                } else {
                    format!("Research findings on: {}", msg.content)
                }
            }
            AgentRole::Code => {
                format!("// Auto-generated Rust Code\nfn main() {{ println!(\"Done: {}\"); }}", msg.content)
            }
            _ => format!("Role {:?} stub response", role),
        };

        let reply = AgentMessage {
            message_id: uuid::Uuid::new_v4().to_string(),
            trace_id: msg.trace_id,
            from: role,
            to: msg.from,
            content: reply_content,
            correlation_id: Some(msg.message_id),
        };

        dispatcher.dispatch(reply).await
    }

    async fn request_reply_internal(
        to: AgentRole,
        content: String,
        trace_id: String,
        from_role: AgentRole,
        dispatcher: AgentDispatcher,
        pending_replies: Arc<Mutex<HashMap<String, oneshot::Sender<AgentMessage>>>>,
    ) -> Result<AgentMessage, String> {
        let msg_id = uuid::Uuid::new_v4().to_string();
        let (tx, rx) = oneshot::channel();

        pending_replies.lock().await.insert(msg_id.clone(), tx);

        let msg = AgentMessage {
            message_id: msg_id.clone(),
            trace_id,
            from: from_role,
            to,
            content,
            correlation_id: None,
        };

        if let Err(e) = dispatcher.dispatch(msg).await {
            pending_replies.lock().await.remove(&msg_id);
            return Err(e);
        }

        match tokio::time::timeout(Duration::from_secs(5), rx).await {
            Ok(Ok(reply)) => Ok(reply),
            Ok(Err(_)) => Err("Oneshot sender dropped".to_string()),
            Err(_) => {
                pending_replies.lock().await.remove(&msg_id);
                Err("Timeout waiting for agent reply".to_string())
            }
        }
    }
}

