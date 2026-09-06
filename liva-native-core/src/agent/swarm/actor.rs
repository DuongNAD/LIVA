//! Swarm Actor Lifecycle & Message Dispatch Loop
//!
//! Provides the runtime execution context for an individual actor:
//! - Priority mailbox continuous polling.
//! - Vector clock updating and causal ticking on message processing.
//! - Delegation token validation.
//! - Role handler dispatch and auto-reply routing.
//! - Clean termination and error reporting.

use super::mailbox::{PriorityMailboxReceiver, PriorityMailboxSender};
use super::roles::SwarmActorRole;
use super::types::{ActorError, ActorStatus, MessagePriority, SwarmMessage, SwarmPayload, SwarmRole};
use super::vector_clock::VectorClock;
use crate::agent::state::AgentState;
use std::sync::Arc;
use tokio::sync::{mpsc, watch, Mutex};

/// Context provided to a `SwarmActorRole` handler during message processing.
#[derive(Clone)]
pub struct ActorContext {
    pub actor_id: String,
    pub role: SwarmRole,
    dispatcher_tx: mpsc::Sender<SwarmMessage>,
}

impl ActorContext {
    pub fn new(
        actor_id: impl Into<String>,
        role: SwarmRole,
        dispatcher_tx: mpsc::Sender<SwarmMessage>,
    ) -> Self {
        Self {
            actor_id: actor_id.into(),
            role,
            dispatcher_tx,
        }
    }

    /// Dispatches an outgoing message to the central swarm router.
    pub async fn dispatch(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        self.dispatcher_tx
            .send(msg)
            .await
            .map_err(|e| ActorError::MailboxSendError(e.to_string()))
    }
}

/// SwarmActor encapsulates role logic, priority mailbox, vector clock, and execution loop.
pub struct SwarmActor {
    pub actor_id: String,
    pub role: SwarmRole,
    mailbox_rx: PriorityMailboxReceiver,
    mailbox_tx: PriorityMailboxSender,
    handler: Box<dyn SwarmActorRole>,
    vector_clock: VectorClock,
    status: Arc<Mutex<ActorStatus>>,
    context: ActorContext,
    state: AgentState,
}

impl SwarmActor {
    pub fn new(
        actor_id: impl Into<String>,
        handler: Box<dyn SwarmActorRole>,
        mailbox_tx: PriorityMailboxSender,
        mailbox_rx: PriorityMailboxReceiver,
        dispatcher_tx: mpsc::Sender<SwarmMessage>,
    ) -> Self {
        let aid = actor_id.into();
        let role = handler.role();
        let context = ActorContext::new(aid.clone(), role, dispatcher_tx);

        Self {
            actor_id: aid,
            role,
            mailbox_rx,
            mailbox_tx,
            handler,
            vector_clock: VectorClock::new(),
            status: Arc::new(Mutex::new(ActorStatus::Idle)),
            context,
            state: AgentState::default(),
        }
    }

    /// Set initial agent working state.
    pub fn with_state(mut self, state: AgentState) -> Self {
        self.state = state;
        self
    }

    /// Access current actor working state snapshot.
    pub fn state(&self) -> &AgentState {
        &self.state
    }

    /// Spawns the actor on a background Tokio task.
    pub fn spawn(mut self) -> ActorHandle {
        let (shutdown_tx, mut shutdown_rx) = watch::channel(false);
        let actor_id = self.actor_id.clone();
        let role = self.role;
        let sender = self.mailbox_tx.clone();
        let status_arc = self.status.clone();

        let join_handle = tokio::spawn(async move {
            {
                let mut st = status_arc.lock().await;
                *st = ActorStatus::Idle;
            }

            loop {
                tokio::select! {
                    _ = shutdown_rx.changed() => {
                        if *shutdown_rx.borrow() {
                            tracing::info!("[{:?}:{}] Received shutdown signal", self.role, self.actor_id);
                            break;
                        }
                    }
                    maybe_msg = self.mailbox_rx.recv() => {
                        match maybe_msg {
                            Some(msg) => {
                                {
                                    let mut st = status_arc.lock().await;
                                    *st = ActorStatus::Busy;
                                }

                                // 1. Update and tick logical vector clock
                                self.vector_clock.update(&msg.vector_clock);
                                self.vector_clock.tick(&self.actor_id);

                                // 2. Validate delegation token if attached
                                if let Some(ref token) = msg.delegation_token {
                                    let now = chrono::Utc::now().timestamp_millis() as u64;
                                    if token.budget.is_expired(now) || token.is_revoked {
                                        tracing::warn!(
                                            "[{:?}:{}] Discarded message: delegation token expired or revoked",
                                            self.role, self.actor_id
                                        );
                                        let mut st = status_arc.lock().await;
                                        *st = ActorStatus::Idle;
                                        continue;
                                    }
                                }

                                // 3. Invoke role handler
                                match self.handler.handle_message(msg.clone(), &self.context).await {
                                    Ok(Some(reply_payload)) => {
                                        let is_veto = matches!(reply_payload, SwarmPayload::SentinelVeto { .. });
                                        let reply_priority = if is_veto {
                                            MessagePriority::High
                                        } else {
                                            MessagePriority::Normal
                                        };

                                        let reply_msg = SwarmMessage {
                                            message_id: format!("msg-{}", uuid::Uuid::new_v4()),
                                            trace_id: msg.trace_id.clone(),
                                            sender_role: self.role,
                                            target_role: if is_veto { None } else { Some(msg.sender_role) },
                                            priority: reply_priority,
                                            payload: reply_payload,
                                            vector_clock: self.vector_clock.clone(),
                                            delegation_token: msg.delegation_token.clone(),
                                            correlation_id: Some(msg.message_id.clone()),
                                            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
                                        };

                                        if let Err(e) = self.context.dispatch(reply_msg).await {
                                            tracing::error!(
                                                "[{:?}:{}] Failed to dispatch reply message: {:?}",
                                                self.role, self.actor_id, e
                                            );
                                        }
                                    }
                                    Ok(None) => {}
                                    Err(e) => {
                                        tracing::error!(
                                            "[{:?}:{}] Error handling message: {:?}",
                                            self.role, self.actor_id, e
                                        );
                                    }
                                }

                                {
                                    let mut st = status_arc.lock().await;
                                    *st = ActorStatus::Idle;
                                }
                            }
                            None => {
                                // Mailbox channel closed
                                break;
                            }
                        }
                    }
                }
            }

            {
                let mut st = status_arc.lock().await;
                *st = ActorStatus::Terminated;
            }
        });

        ActorHandle {
            actor_id,
            role,
            sender,
            status: self.status,
            join_handle,
            shutdown_tx,
        }
    }
}

/// Handle to a running SwarmActor instance.
pub struct ActorHandle {
    pub actor_id: String,
    pub role: SwarmRole,
    pub sender: PriorityMailboxSender,
    pub status: Arc<Mutex<ActorStatus>>,
    pub join_handle: tokio::task::JoinHandle<()>,
    pub shutdown_tx: watch::Sender<bool>,
}

impl ActorHandle {
    /// Send a message to this actor's priority mailbox.
    pub async fn send(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        self.sender.send(msg).await
    }

    /// Retrieve the actor's current status.
    pub async fn status(&self) -> ActorStatus {
        let st = self.status.lock().await;
        st.clone()
    }

    /// Signals the actor to shut down gracefully and awaits task termination.
    pub async fn shutdown(self) -> Result<(), ActorError> {
        let _ = self.shutdown_tx.send(true);
        self.join_handle
            .await
            .map_err(|e| ActorError::Other(e.to_string()))
    }
}
