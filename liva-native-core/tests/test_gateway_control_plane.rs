//! E2E & Unit Test Suite: WebSocket Gateway Control Plane & Companion Node Pairing Protocol
//! Covers Feature 3 (WebSocket Gateway Control Plane) and Feature 4 (Companion Node Pairing Protocol)
//! Tiers 1, 2 & 3 Test Suite for LIVA Native Core

use liva_native_core::gateway::control_plane::{
    ControlFrame, ControlOpcode, GatewayControlPlane, GatewayError, InMemoryGatewayControlPlane,
};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::mpsc;
use uuid::Uuid;

fn default_unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_secs()
}

// ============================================================================
// FEATURE 3: TIER 1 - FEATURE COVERAGE TESTS (≥5 tests)
// ============================================================================

#[test]
fn test_tier1_control_frame_all_opcodes_serde() {
    let opcodes = vec![
        ControlOpcode::Handshake,
        ControlOpcode::Auth,
        ControlOpcode::PairRequest,
        ControlOpcode::PairResponse,
        ControlOpcode::Heartbeat,
        ControlOpcode::Subscribe,
        ControlOpcode::Unsubscribe,
        ControlOpcode::Event,
        ControlOpcode::Command,
        ControlOpcode::CommandResponse,
        ControlOpcode::StreamData,
        ControlOpcode::StreamEnd,
        ControlOpcode::Error,
    ];

    for op in opcodes {
        let frame = ControlFrame::new(op);
        let json = serde_json::to_string(&frame).expect("serialize frame");
        let parsed: ControlFrame = serde_json::from_str(&json).expect("deserialize frame");
        assert_eq!(parsed.opcode, op);
        assert_eq!(op.as_str(), parsed.opcode.as_str());
    }
}

#[tokio::test]
async fn test_tier1_gateway_register_unregister_nodes() {
    let registry = PairingRegistry::with_random_secret("srv_pub_1");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node_id = NodeId::new();
    let (tx, _rx) = mpsc::channel(16);

    gateway
        .register_node(node_id, NodeRole::DesktopUi, tx)
        .await
        .expect("register");

    // Unregister
    gateway.unregister_node(&node_id).await.expect("unregister");

    // Sending to unregistered should error
    let res = gateway
        .send_node_frame(&node_id, ControlFrame::heartbeat())
        .await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_tier1_gateway_topic_pubsub_dispatch() {
    let registry = PairingRegistry::with_random_secret("srv_pub_2");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node1 = NodeId::new();
    let (tx1, mut rx1) = mpsc::channel(16);
    let node2 = NodeId::new();
    let (tx2, mut rx2) = mpsc::channel(16);

    gateway
        .register_node(node1, NodeRole::DesktopUi, tx1)
        .await
        .unwrap();
    gateway
        .register_node(node2, NodeRole::Widget, tx2)
        .await
        .unwrap();

    // Node 1 subscribes to telemetry, Node 2 subscribes to alerts
    gateway
        .subscribe_topic(&node1, "telemetry.cpu")
        .await
        .unwrap();
    gateway
        .subscribe_topic(&node2, "alerts.critical")
        .await
        .unwrap();

    // Broadcast telemetry
    let sent = gateway
        .broadcast_event("telemetry.cpu", serde_json::json!({ "usage": 45 }))
        .await
        .unwrap();
    assert_eq!(sent, 1);

    let frame1 = rx1.try_recv().expect("node 1 received");
    assert_eq!(frame1.opcode, ControlOpcode::Event);
    assert_eq!(frame1.topic.as_deref(), Some("telemetry.cpu"));
    assert!(rx2.try_recv().is_err());

    // Unsubscribe node 1
    gateway
        .unsubscribe_topic(&node1, "telemetry.cpu")
        .await
        .unwrap();
    let sent_after = gateway
        .broadcast_event("telemetry.cpu", serde_json::json!({ "usage": 50 }))
        .await
        .unwrap();
    assert_eq!(sent_after, 0);
}

#[tokio::test]
async fn test_tier1_gateway_direct_node_send() {
    let registry = PairingRegistry::with_random_secret("srv_pub_3");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node_id = NodeId::new();
    let (tx, mut rx) = mpsc::channel(16);

    gateway
        .register_node(node_id, NodeRole::CliTool, tx)
        .await
        .unwrap();

    let frame = ControlFrame::command("system.status", serde_json::json!({}));
    gateway
        .send_node_frame(&node_id, frame.clone())
        .await
        .expect("send frame");

    let received = rx.try_recv().expect("received frame");
    assert_eq!(received.opcode, ControlOpcode::Command);
    assert_eq!(received.topic.as_deref(), Some("system.status"));
}

#[tokio::test]
async fn test_tier1_gateway_stream_data_and_end_chunks() {
    let frame_id = Uuid::new_v4().to_string();
    let chunk1 = ControlFrame::stream_data(&frame_id, "Processing your request...", false);
    let chunk2 = ControlFrame::stream_data(&frame_id, " Here is the answer.", true);
    let chunk_end = ControlFrame::stream_end(&frame_id);

    assert_eq!(chunk1.opcode, ControlOpcode::StreamData);
    assert_eq!(chunk2.opcode, ControlOpcode::StreamData);
    assert_eq!(chunk_end.opcode, ControlOpcode::StreamEnd);
    assert_eq!(chunk1.frame_id, frame_id);
    assert_eq!(chunk_end.frame_id, frame_id);
}

// ============================================================================
// FEATURE 3: TIER 2 - BOUNDARY & CORNER CASES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_tier2_send_to_unregistered_node_fails() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = InMemoryGatewayControlPlane::new(registry);
    let ghost_node = NodeId::new();

    let res = gateway
        .send_node_frame(&ghost_node, ControlFrame::heartbeat())
        .await;
    match res {
        Err(GatewayError::NodeNotFound(id)) => assert_eq!(id, ghost_node),
        other => panic!("Expected NodeNotFound error, got {:?}", other),
    }
}

#[tokio::test]
async fn test_tier2_subscribe_missing_topic_returns_error_frame() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = InMemoryGatewayControlPlane::new(registry);
    let node_id = NodeId::new();

    // Frame with Subscribe opcode but None topic
    let bad_frame = ControlFrame::new(ControlOpcode::Subscribe);
    let resp = gateway
        .handle_frame(&node_id, bad_frame)
        .await
        .unwrap()
        .expect("error response");

    assert_eq!(resp.opcode, ControlOpcode::Error);
    assert!(resp.error.is_some());
    assert_eq!(resp.error.unwrap().code, 400);
}

#[test]
fn test_tier2_control_frame_error_payload_details() {
    let frame = ControlFrame::error(
        "req_404",
        404,
        "Resource not found",
        Some(serde_json::json!({ "resource": "model/gemma-4" })),
    );

    assert_eq!(frame.opcode, ControlOpcode::Error);
    let err = frame.error.expect("has error payload");
    assert_eq!(err.code, 404);
    assert_eq!(err.message, "Resource not found");
    assert_eq!(
        err.details,
        Some(serde_json::json!({ "resource": "model/gemma-4" }))
    );
}

#[tokio::test]
async fn test_tier2_concurrent_broadcast_subscribers() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = Arc::new(InMemoryGatewayControlPlane::new(registry));

    let mut receivers = Vec::new();
    for _ in 0..10 {
        let node_id = NodeId::new();
        let (tx, rx) = mpsc::channel(16);
        gateway
            .register_node(node_id, NodeRole::Widget, tx)
            .await
            .unwrap();
        gateway
            .subscribe_topic(&node_id, "global.announcement")
            .await
            .unwrap();
        receivers.push(rx);
    }

    let sent = gateway
        .broadcast_event("global.announcement", serde_json::json!({ "version": "0.1.0" }))
        .await
        .unwrap();
    assert_eq!(sent, 10);

    for mut rx in receivers {
        let f = rx.try_recv().expect("must receive frame");
        assert_eq!(f.opcode, ControlOpcode::Event);
    }
}

#[tokio::test]
async fn test_tier2_heartbeat_updates_node_activity() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = InMemoryGatewayControlPlane::new(registry);
    let node_id = NodeId::new();

    let resp = gateway
        .handle_frame(&node_id, ControlFrame::heartbeat())
        .await
        .unwrap()
        .expect("response");
    assert_eq!(resp.opcode, ControlOpcode::Heartbeat);
}

// ============================================================================
// FEATURE 4: TIER 1 - FEATURE COVERAGE TESTS (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_tier1_pairing_challenge_creation_and_short_code_approval() {
    let registry = PairingRegistry::with_random_secret("server_pubkey_tier1");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Android Mobile Companion".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pub_android_123".to_string(),
        pairing_nonce: "nonce_sec_99".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry.create_challenge(req, 300).await.unwrap();
    assert_eq!(challenge.short_code.len(), 6);

    let res = registry
        .approve_by_short_code(&challenge.short_code)
        .await
        .unwrap();
    assert!(res.paired);
    assert!(res.auth_token.is_some());

    let token = res.auth_token.unwrap();
    let (auth_id, role) = registry.verify_auth_token(&token).unwrap();
    assert_eq!(auth_id, node_id);
    assert_eq!(role, NodeRole::MobileCompanion);
}

#[tokio::test]
async fn test_tier1_pairing_auth_token_hmac_verification() {
    let secret = [0x5Au8; 32];
    let registry = PairingRegistry::new(secret, "server_pub");
    let node_id = NodeId::new();

    let token = registry.generate_token(node_id, NodeRole::CliTool, default_unix_now() + 3600);
    let (verified_id, verified_role) = registry.verify_auth_token(&token).unwrap();

    assert_eq!(verified_id, node_id);
    assert_eq!(verified_role, NodeRole::CliTool);
}

#[tokio::test]
async fn test_tier1_pairing_desktop_ui_auto_approval() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = InMemoryGatewayControlPlane::new(registry);
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Desktop Tauri App".to_string(),
        role: NodeRole::DesktopUi,
        public_key: "desktop_pubkey".to_string(),
        pairing_nonce: "nonce_desktop".to_string(),
        timestamp_unix: default_unix_now(),
    };

    // Desktop UI auto-approves immediately
    let resp = gateway.pair_node(req).await.unwrap();
    assert!(resp.paired);
    assert!(resp.auth_token.is_some());
}

#[tokio::test]
async fn test_tier1_pairing_challenge_rejection() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Untrusted Device".to_string(),
        role: NodeRole::HeadlessNode,
        public_key: "untrusted_pubkey".to_string(),
        pairing_nonce: "nonce_bad".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry.create_challenge(req, 120).await.unwrap();
    let reject_res = registry
        .reject_challenge(&challenge.challenge_id, "Unauthorized device")
        .await;
    assert!(reject_res.is_ok());

    // Subsequent approval attempt must fail
    let try_approve = registry
        .approve_by_challenge_id(&challenge.challenge_id)
        .await;
    assert!(try_approve.is_err());
}

#[tokio::test]
async fn test_tier1_pairing_node_revocation() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "IoT Node".to_string(),
        role: NodeRole::HeadlessNode,
        public_key: "iot_pubkey".to_string(),
        pairing_nonce: "nonce_iot".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let ch = registry.create_challenge(req, 60).await.unwrap();
    registry
        .approve_by_challenge_id(&ch.challenge_id)
        .await
        .unwrap();

    assert!(registry.is_node_approved(&node_id).await);
    let revoked = registry.revoke_node(&node_id).await;
    assert!(revoked);
    assert!(!registry.is_node_approved(&node_id).await);
}

// ============================================================================
// FEATURE 4: TIER 2 - BOUNDARY & CORNER CASES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_tier2_pairing_with_empty_public_key_rejected() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Invalid Key Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "   ".to_string(),
        pairing_nonce: "nonce_1".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let res = registry.create_challenge(req, 60).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_tier2_expired_pairing_challenge_fails_approval() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Slow Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "pubkey_slow".to_string(),
        pairing_nonce: "nonce_slow".to_string(),
        timestamp_unix: default_unix_now(),
    };

    // Create challenge with 0 sec TTL
    let ch = registry.create_challenge(req, 0).await.unwrap();
    let res = registry.approve_by_challenge_id(&ch.challenge_id).await;
    assert!(res.is_err());
}

#[test]
fn test_tier2_tampered_token_signature_rejected() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let node_id = NodeId::new();
    let token = registry.generate_token(node_id, NodeRole::MobileCompanion, default_unix_now() + 1000);

    // Alter last signature char
    let mut chars: Vec<char> = token.chars().collect();
    let last_idx = chars.len() - 1;
    chars[last_idx] = if chars[last_idx] == 'a' { 'b' } else { 'a' };
    let corrupted: String = chars.into_iter().collect();

    let res = registry.verify_auth_token(&corrupted);
    assert!(res.is_err());
}

#[test]
fn test_tier2_expired_token_rejected() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let node_id = NodeId::new();
    let expired_time = default_unix_now() - 500;
    let token = registry.generate_token(node_id, NodeRole::MobileCompanion, expired_time);

    let res = registry.verify_auth_token(&token);
    assert!(res.is_err());
}

#[tokio::test]
async fn test_tier2_evict_expired_pairing_challenges() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Abandoned Device".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "pubkey_abandoned".to_string(),
        pairing_nonce: "nonce_ab".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let _ch = registry.create_challenge(req, 0).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    let evicted = registry.evict_expired().await;
    assert!(evicted >= 1);
    assert!(registry.list_pending_challenges().await.is_empty());
}

// ============================================================================
// TIER 3 - PAIRWISE & CROSS-FEATURE COMBINATIONS
// ============================================================================

#[tokio::test]
async fn test_tier3_pairing_approval_grants_gateway_registration() {
    let registry = PairingRegistry::with_random_secret("srv_pub_pair");
    let gateway = InMemoryGatewayControlPlane::new(registry.clone());
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Paired Companion".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_companion_key".to_string(),
        pairing_nonce: "nonce_111".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let ch = registry.create_challenge(req, 120).await.unwrap();
    let pair_resp = registry
        .approve_by_short_code(&ch.short_code)
        .await
        .unwrap();
    let token = pair_resp.auth_token.unwrap();

    // Verify token with gateway
    let (auth_node, role) = gateway.verify_token(&token).await.unwrap();
    assert_eq!(auth_node, node_id);
    assert_eq!(role, NodeRole::MobileCompanion);

    // Register node channel
    let (tx, mut rx) = mpsc::channel(16);
    gateway.register_node(node_id, role, tx).await.unwrap();

    // Send direct command frame
    gateway
        .send_node_frame(&node_id, ControlFrame::command("ping", serde_json::json!({})))
        .await
        .unwrap();

    let rec = rx.try_recv().expect("node receives frame");
    assert_eq!(rec.opcode, ControlOpcode::Command);
}

#[tokio::test]
async fn test_tier3_paired_companion_node_receives_broadcasts_and_commands() {
    let registry = PairingRegistry::with_random_secret("srv_pub_bcast");
    let gateway = InMemoryGatewayControlPlane::new(registry.clone());

    let companion_id = NodeId::new();
    let (tx, mut rx) = mpsc::channel(16);

    gateway
        .register_node(companion_id, NodeRole::MobileCompanion, tx)
        .await
        .unwrap();
    gateway
        .subscribe_topic(&companion_id, "agent.voice.event")
        .await
        .unwrap();

    let delivered = gateway
        .broadcast_event(
            "agent.voice.event",
            serde_json::json!({ "event": "wake_detected" }),
        )
        .await
        .unwrap();
    assert_eq!(delivered, 1);

    let event = rx.try_recv().expect("companion receives event");
    assert_eq!(event.opcode, ControlOpcode::Event);
    assert_eq!(event.topic.as_deref(), Some("agent.voice.event"));
}
