//! Agent Actor Pool Management & Role-Based Routing
//!
//! Coordinates actor lifetimes, maintains point-to-point and broadcast routing maps,
//! and forwards messages from the central dispatcher channel to target actors.

use super::actor::ActorHandle;
use super::types::{ActorError, SwarmMessage, SwarmRole};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

/// Thread-safe registry and router for all active swarm actors.
pub struct AgentActorPool {
    actors: Arc<RwLock<HashMap<SwarmRole, Vec<ActorHandle>>>>,
    dispatcher_tx: mpsc::Sender<SwarmMessage>,
    router_task: Option<tokio::task::JoinHandle<()>>,
}

impl AgentActorPool {
    /// Constructs a new actor pool and starts the background central routing task.
    pub fn new() -> (Self, mpsc::Sender<SwarmMessage>) {
        let (tx, mut rx) = mpsc::channel::<SwarmMessage>(1024);
        let actors: Arc<RwLock<HashMap<SwarmRole, Vec<ActorHandle>>>> =
            Arc::new(RwLock::new(HashMap::new()));
        let actors_clone = actors.clone();

        let router_task = tokio::spawn(async move {
            while let Some(msg) = rx.recv().await {
                let current_actors = actors_clone.read().await;

                if let Some(target) = msg.target_role {
                    if let Some(handles) = current_actors.get(&target) {
                        for h in handles {
                            let _ = h.send(msg.clone()).await;
                        }
                    } else {
                        tracing::warn!("[AgentActorPool] No registered actors for target role {:?}", target);
                    }
                } else {
                    // Broadcast to all roles except the sender
                    for (role, handles) in current_actors.iter() {
                        if *role != msg.sender_role {
                            for h in handles {
                                let mut broadcast_msg = msg.clone();
                                broadcast_msg.target_role = Some(*role);
                                let _ = h.send(broadcast_msg).await;
                            }
                        }
                    }
                }
            }
        });

        let pool = Self {
            actors,
            dispatcher_tx: tx.clone(),
            router_task: Some(router_task),
        };

        (pool, tx)
    }

    /// Registers a newly spawned actor handle into the pool.
    pub async fn register(&self, handle: ActorHandle) {
        let mut actors = self.actors.write().await;
        actors.entry(handle.role).or_default().push(handle);
    }

    /// Sends a message into the pool's dispatcher.
    pub async fn dispatch(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        self.dispatcher_tx
            .send(msg)
            .await
            .map_err(|e| ActorError::MailboxSendError(e.to_string()))
    }

    /// Broadcasts a message to all actors in the pool.
    pub async fn broadcast(&self, mut msg: SwarmMessage) -> Result<(), ActorError> {
        msg.target_role = None;
        self.dispatch(msg).await
    }

    /// Returns list of all registered roles currently active in the pool.
    pub async fn registered_roles(&self) -> Vec<SwarmRole> {
        let actors = self.actors.read().await;
        actors.keys().copied().collect()
    }

    /// Checks if a role is registered.
    pub async fn has_role(&self, role: SwarmRole) -> bool {
        let actors = self.actors.read().await;
        actors.contains_key(&role) && !actors.get(&role).unwrap().is_empty()
    }

    /// Gracefully shuts down all actors in the pool.
    pub async fn shutdown(&mut self) -> Result<(), ActorError> {
        let mut actors = self.actors.write().await;
        for (_role, handles) in actors.drain() {
            for h in handles {
                let _ = h.shutdown().await;
            }
        }
        if let Some(router) = self.router_task.take() {
            router.abort();
        }
        Ok(())
    }
}
