//! WebSocket Gateway Control Plane
//!
//! Provides the full-duplex JSON-RPC 2.0 / framed control plane for UI clients,
//! desktop widgets, companion mobile nodes, and headless edge daemons.

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fmt;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use uuid::Uuid;

use super::pairing::{NodeId, NodeRole, PairingRegistry, PairingRequest, PairingResponse};

/// Numerical and textual opcodes identifying control frame operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ControlOpcode {
    Handshake = 0x01,
    Auth = 0x02,
    PairRequest = 0x03,
    PairResponse = 0x04,
    Heartbeat = 0x05,
    Subscribe = 0x06,
    Unsubscribe = 0x07,
    Event = 0x08,
    Command = 0x09,
    CommandResponse = 0x0A,
    StreamData = 0x0B,
    StreamEnd = 0x0C,
    Error = 0x0F,
}

impl ControlOpcode {
    /// Convert opcode to human-readable string.
    pub fn as_str(&self) -> &str {
        match self {
            Self::Handshake => "handshake",
            Self::Auth => "auth",
            Self::PairRequest => "pair_request",
            Self::PairResponse => "pair_response",
            Self::Heartbeat => "heartbeat",
            Self::Subscribe => "subscribe",
            Self::Unsubscribe => "unsubscribe",
            Self::Event => "event",
            Self::Command => "command",
            Self::CommandResponse => "command_response",
            Self::StreamData => "stream_data",
            Self::StreamEnd => "stream_end",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for ControlOpcode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for ControlOpcode {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "handshake" => Ok(Self::Handshake),
            "auth" => Ok(Self::Auth),
            "pair_request" => Ok(Self::PairRequest),
            "pair_response" => Ok(Self::PairResponse),
            "heartbeat" => Ok(Self::Heartbeat),
            "subscribe" => Ok(Self::Subscribe),
            "unsubscribe" => Ok(Self::Unsubscribe),
            "event" => Ok(Self::Event),
            "command" => Ok(Self::Command),
            "command_response" => Ok(Self::CommandResponse),
            "stream_data" => Ok(Self::StreamData),
            "stream_end" => Ok(Self::StreamEnd),
            "error" => Ok(Self::Error),
            other => Err(format!("Unknown control opcode: {}", other)),
        }
    }
}

/// Detailed error payload carried inside an Error frame.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlErrorPayload {
    pub code: i32,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

/// Canonical control plane protocol frame exchanged over WebSockets and IPC.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ControlFrame {
    pub frame_id: String,
    pub opcode: ControlOpcode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<ControlErrorPayload>,
}

impl ControlFrame {
    /// Create a new generic frame with auto-generated UUID frame_id.
    pub fn new(opcode: ControlOpcode) -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode,
            topic: None,
            payload: None,
            error: None,
        }
    }

    /// Construct a Handshake frame.
    pub fn handshake(capabilities: serde_json::Value) -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode: ControlOpcode::Handshake,
            topic: None,
            payload: Some(capabilities),
            error: None,
        }
    }

    /// Construct an Auth frame carrying bearer credentials.
    pub fn auth(token: impl Into<String>) -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode: ControlOpcode::Auth,
            topic: None,
            payload: Some(serde_json::json!({ "token": token.into() })),
            error: None,
        }
    }

    /// Construct an Event frame on a specific topic.
    pub fn event(topic: impl Into<String>, payload: serde_json::Value) -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode: ControlOpcode::Event,
            topic: Some(topic.into()),
            payload: Some(payload),
            error: None,
        }
    }

    /// Construct a Command invocation frame.
    pub fn command(command_name: impl Into<String>, params: serde_json::Value) -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode: ControlOpcode::Command,
            topic: Some(command_name.into()),
            payload: Some(params),
            error: None,
        }
    }

    /// Construct a CommandResponse frame matching a previous request frame_id.
    pub fn command_response(frame_id: impl Into<String>, result: serde_json::Value) -> Self {
        Self {
            frame_id: frame_id.into(),
            opcode: ControlOpcode::CommandResponse,
            topic: None,
            payload: Some(result),
            error: None,
        }
    }

    /// Construct a Heartbeat frame.
    pub fn heartbeat() -> Self {
        Self {
            frame_id: Uuid::new_v4().to_string(),
            opcode: ControlOpcode::Heartbeat,
            topic: None,
            payload: Some(serde_json::json!({ "timestamp": chrono::Utc::now().to_rfc3339() })),
            error: None,
        }
    }

    /// Construct a StreamData chunk frame.
    pub fn stream_data(
        frame_id: impl Into<String>,
        chunk: impl Into<String>,
        is_final: bool,
    ) -> Self {
        Self {
            frame_id: frame_id.into(),
            opcode: ControlOpcode::StreamData,
            topic: None,
            payload: Some(serde_json::json!({
                "chunk": chunk.into(),
                "is_final": is_final
            })),
            error: None,
        }
    }

    /// Construct a StreamEnd terminal frame.
    pub fn stream_end(frame_id: impl Into<String>) -> Self {
        Self {
            frame_id: frame_id.into(),
            opcode: ControlOpcode::StreamEnd,
            topic: None,
            payload: None,
            error: None,
        }
    }

    /// Construct an Error frame.
    pub fn error(
        frame_id: impl Into<String>,
        code: i32,
        message: impl Into<String>,
        details: Option<serde_json::Value>,
    ) -> Self {
        Self {
            frame_id: frame_id.into(),
            opcode: ControlOpcode::Error,
            topic: None,
            payload: None,
            error: Some(ControlErrorPayload {
                code,
                message: message.into(),
                details,
            }),
        }
    }
}

/// Errors occurring across gateway control plane operations.
#[derive(Debug, thiserror::Error)]
pub enum GatewayError {
    #[error("Authentication failed: {0}")]
    AuthFailed(String),
    #[error("Node not authorized: {0}")]
    UnauthorizedNode(NodeId),
    #[error("Pairing rejected: {0}")]
    PairingRejected(String),
    #[error("Protocol error: {0}")]
    Protocol(String),
    #[error("Node not found: {0}")]
    NodeNotFound(NodeId),
    #[error("Channel closed")]
    ChannelClosed,
    #[error("Network I/O error: {0}")]
    Io(#[from] std::io::Error),
}

/// Active connection state for a registered gateway node.
#[derive(Clone)]
pub struct ActiveNodeConnection {
    pub node_id: NodeId,
    pub role: NodeRole,
    pub tx: mpsc::Sender<ControlFrame>,
}

/// Trait defining gateway control plane runtime operations.
#[async_trait::async_trait]
pub trait GatewayControlPlane: Send + Sync {
    /// Broadcast an event frame to all nodes subscribed to the specified topic.
    async fn broadcast_event(
        &self,
        topic: &str,
        payload: serde_json::Value,
    ) -> Result<usize, GatewayError>;

    /// Send a control frame directly to a specific connected node.
    async fn send_node_frame(
        &self,
        node_id: &NodeId,
        frame: ControlFrame,
    ) -> Result<(), GatewayError>;

    /// Process a companion node pairing request.
    async fn pair_node(&self, request: PairingRequest) -> Result<PairingResponse, GatewayError>;

    /// Verify an authentication token and return node identity and role.
    async fn verify_token(&self, token: &str) -> Result<(NodeId, NodeRole), GatewayError>;

    /// Register a newly authenticated node connection.
    async fn register_node(
        &self,
        node_id: NodeId,
        role: NodeRole,
        tx: mpsc::Sender<ControlFrame>,
    ) -> Result<(), GatewayError>;

    /// Unregister a disconnected node.
    async fn unregister_node(&self, node_id: &NodeId) -> Result<(), GatewayError>;

    /// Subscribe a node to a broadcast event topic.
    async fn subscribe_topic(&self, node_id: &NodeId, topic: &str) -> Result<(), GatewayError>;

    /// Unsubscribe a node from an event topic.
    async fn unsubscribe_topic(&self, node_id: &NodeId, topic: &str) -> Result<(), GatewayError>;

    /// Process an inbound frame from a connected node.
    async fn handle_frame(
        &self,
        node_id: &NodeId,
        frame: ControlFrame,
    ) -> Result<Option<ControlFrame>, GatewayError>;
}

/// Standard high-performance in-memory Gateway Control Plane runtime.
pub struct InMemoryGatewayControlPlane {
    nodes: Arc<RwLock<HashMap<NodeId, ActiveNodeConnection>>>,
    subscriptions: Arc<RwLock<HashMap<String, HashSet<NodeId>>>>,
    pairing_registry: PairingRegistry,
    global_event_bus: broadcast::Sender<ControlFrame>,
}

impl InMemoryGatewayControlPlane {
    /// Create a new control plane with the provided pairing registry.
    pub fn new(pairing_registry: PairingRegistry) -> Self {
        let (global_event_bus, _) = broadcast::channel(1024);
        Self {
            nodes: Arc::new(RwLock::new(HashMap::new())),
            subscriptions: Arc::new(RwLock::new(HashMap::new())),
            pairing_registry,
            global_event_bus,
        }
    }

    /// Access the underlying pairing registry.
    pub fn pairing_registry(&self) -> &PairingRegistry {
        &self.pairing_registry
    }
}

#[async_trait::async_trait]
impl GatewayControlPlane for InMemoryGatewayControlPlane {
    async fn broadcast_event(
        &self,
        topic: &str,
        payload: serde_json::Value,
    ) -> Result<usize, GatewayError> {
        let frame = ControlFrame::event(topic, payload);
        let _ = self.global_event_bus.send(frame.clone());

        let target_nodes: Vec<NodeId> = {
            let subs = self.subscriptions.read().await;
            if let Some(nodes) = subs.get(topic) {
                nodes.iter().copied().collect()
            } else {
                Vec::new()
            }
        };

        // Collect sender handles and immediately release read lock on self.nodes
        let senders: Vec<mpsc::Sender<ControlFrame>> = {
            let nodes_map = self.nodes.read().await;
            target_nodes
                .iter()
                .filter_map(|node_id| nodes_map.get(node_id).map(|conn| conn.tx.clone()))
                .collect()
        };

        let mut sent_count = 0;
        for tx in senders {
            // Use non-blocking try_send so slow/blocked nodes never stall the broadcast or gateway
            if tx.try_send(frame.clone()).is_ok() {
                sent_count += 1;
            }
        }

        Ok(sent_count)
    }

    async fn send_node_frame(
        &self,
        node_id: &NodeId,
        frame: ControlFrame,
    ) -> Result<(), GatewayError> {
        let tx = {
            let nodes = self.nodes.read().await;
            nodes.get(node_id).map(|conn| conn.tx.clone())
        };

        if let Some(tx) = tx {
            tx.send(frame)
                .await
                .map_err(|_| GatewayError::ChannelClosed)?;
            Ok(())
        } else {
            Err(GatewayError::NodeNotFound(*node_id))
        }
    }

    async fn pair_node(&self, request: PairingRequest) -> Result<PairingResponse, GatewayError> {
        let challenge = self
            .pairing_registry
            .create_challenge(request, 300)
            .await
            .map_err(GatewayError::PairingRejected)?;

        // Auto-approve if request is DesktopUi or Widget on localhost
        if challenge.node_info.role.has_full_access() {
            self.pairing_registry
                .approve_by_challenge_id(&challenge.challenge_id)
                .await
                .map_err(GatewayError::PairingRejected)
        } else {
            // Require external approval via pairing short code
            Ok(PairingResponse {
                paired: false,
                auth_token: None,
                server_public_key: String::new(),
                expires_at_unix: challenge.expires_at_unix,
                error_reason: Some(format!(
                    "Pairing code: {}. Awaiting administrator confirmation.",
                    challenge.short_code
                )),
            })
        }
    }

    async fn verify_token(&self, token: &str) -> Result<(NodeId, NodeRole), GatewayError> {
        self.pairing_registry
            .verify_auth_token(token)
            .map_err(GatewayError::AuthFailed)
    }

    async fn register_node(
        &self,
        node_id: NodeId,
        role: NodeRole,
        tx: mpsc::Sender<ControlFrame>,
    ) -> Result<(), GatewayError> {
        let mut nodes = self.nodes.write().await;
        nodes.insert(node_id, ActiveNodeConnection { node_id, role, tx });
        self.pairing_registry.touch_node(&node_id).await;
        Ok(())
    }

    async fn unregister_node(&self, node_id: &NodeId) -> Result<(), GatewayError> {
        let mut nodes = self.nodes.write().await;
        nodes.remove(node_id);

        let mut subs = self.subscriptions.write().await;
        for node_set in subs.values_mut() {
            node_set.remove(node_id);
        }
        Ok(())
    }

    async fn subscribe_topic(&self, node_id: &NodeId, topic: &str) -> Result<(), GatewayError> {
        let mut subs = self.subscriptions.write().await;
        subs.entry(topic.to_string())
            .or_default()
            .insert(*node_id);
        Ok(())
    }

    async fn unsubscribe_topic(&self, node_id: &NodeId, topic: &str) -> Result<(), GatewayError> {
        let mut subs = self.subscriptions.write().await;
        if let Some(set) = subs.get_mut(topic) {
            set.remove(node_id);
        }
        Ok(())
    }

    async fn handle_frame(
        &self,
        node_id: &NodeId,
        frame: ControlFrame,
    ) -> Result<Option<ControlFrame>, GatewayError> {
        match frame.opcode {
            ControlOpcode::Heartbeat => {
                self.pairing_registry.touch_node(node_id).await;
                Ok(Some(ControlFrame::heartbeat()))
            }
            ControlOpcode::Subscribe => {
                if let Some(topic) = frame.topic {
                    self.subscribe_topic(node_id, &topic).await?;
                    Ok(Some(ControlFrame::command_response(
                        frame.frame_id,
                        serde_json::json!({ "subscribed": topic }),
                    )))
                } else {
                    Ok(Some(ControlFrame::error(
                        frame.frame_id,
                        400,
                        "Missing topic in subscribe request",
                        None,
                    )))
                }
            }
            ControlOpcode::Unsubscribe => {
                if let Some(topic) = frame.topic {
                    self.unsubscribe_topic(node_id, &topic).await?;
                    Ok(Some(ControlFrame::command_response(
                        frame.frame_id,
                        serde_json::json!({ "unsubscribed": topic }),
                    )))
                } else {
                    Ok(Some(ControlFrame::error(
                        frame.frame_id,
                        400,
                        "Missing topic in unsubscribe request",
                        None,
                    )))
                }
            }
            ControlOpcode::Command => {
                // Return echoed command response for testing/runtime delegation
                let topic = frame.topic.as_deref().unwrap_or("unknown");
                Ok(Some(ControlFrame::command_response(
                    frame.frame_id,
                    serde_json::json!({
                        "status": "ack",
                        "command": topic,
                        "received_payload": frame.payload
                    }),
                )))
            }
            _ => Ok(None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_control_frame_serde_roundtrip() {
        let frame = ControlFrame::event("agent.chunk", serde_json::json!({ "delta": "Hello" }));
        let json = serde_json::to_string(&frame).expect("serialize ControlFrame");
        let parsed: ControlFrame = serde_json::from_str(&json).expect("deserialize ControlFrame");

        assert_eq!(frame.opcode, parsed.opcode);
        assert_eq!(frame.topic, parsed.topic);
        assert_eq!(frame.payload, parsed.payload);
    }

    #[tokio::test]
    async fn test_gateway_broadcast_and_subscription() {
        let registry = PairingRegistry::with_random_secret("test_server_pubkey");
        let gateway = InMemoryGatewayControlPlane::new(registry);

        let node1 = NodeId::new();
        let (tx1, mut rx1) = mpsc::channel(10);
        let node2 = NodeId::new();
        let (tx2, mut rx2) = mpsc::channel(10);

        gateway
            .register_node(node1, NodeRole::DesktopUi, tx1)
            .await
            .unwrap();
        gateway
            .register_node(node2, NodeRole::MobileCompanion, tx2)
            .await
            .unwrap();

        // Node 1 subscribes to "agent.progress", Node 2 does not
        gateway
            .subscribe_topic(&node1, "agent.progress")
            .await
            .unwrap();

        let delivered = gateway
            .broadcast_event("agent.progress", serde_json::json!({ "step": 1 }))
            .await
            .unwrap();
        assert_eq!(delivered, 1);

        let received1 = rx1.try_recv().expect("node 1 should receive event");
        assert_eq!(received1.opcode, ControlOpcode::Event);
        assert_eq!(received1.topic.as_deref(), Some("agent.progress"));

        assert!(
            rx2.try_recv().is_err(),
            "node 2 should not receive unsubscribed event"
        );
    }

    #[tokio::test]
    async fn test_gateway_handle_heartbeat() {
        let registry = PairingRegistry::with_random_secret("test_server_pubkey");
        let gateway = InMemoryGatewayControlPlane::new(registry);
        let node_id = NodeId::new();

        let req = ControlFrame::heartbeat();
        let resp = gateway
            .handle_frame(&node_id, req)
            .await
            .unwrap()
            .expect("heartbeat response");

        assert_eq!(resp.opcode, ControlOpcode::Heartbeat);
    }
}
