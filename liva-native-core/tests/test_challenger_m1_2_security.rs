//! Challenger 2 Empirical Security & Adversarial Test Suite for Milestone 1
//! Targets:
//! - liva-native-core/src/messaging/normalized.rs
//! - liva-native-core/src/messaging/session.rs
//! - liva-native-core/src/gateway/control_plane.rs
//! - liva-native-core/src/gateway/pairing.rs

use bytes::Bytes;
use liva_native_core::gateway::control_plane::{
    ControlFrame, ControlOpcode, GatewayControlPlane, GatewayError,
    InMemoryGatewayControlPlane,
};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use liva_native_core::messaging::normalized::{
    Attachment, AttachmentSource, ChannelId, IncomingMessage, MessageSender,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionId, SessionManager,
};
use std::collections::HashSet;
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
// SUITE 1: PAIRING PROTOCOL REPLAY ATTACKS & CONCURRENCY
// ============================================================================

#[tokio::test]
async fn test_sec_replay_short_code_approval_rejected() {
    let registry = PairingRegistry::with_random_secret("server_pub_sec_1");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Mobile Device".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pub_key_1".to_string(),
        pairing_nonce: "nonce_1".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry
        .create_challenge(req, 300)
        .await
        .expect("challenge creation should succeed");

    // First approval attempt -> MUST succeed
    let resp1 = registry
        .approve_by_short_code(&challenge.short_code)
        .await
        .expect("first approval must succeed");
    assert!(resp1.paired);
    assert!(resp1.auth_token.is_some());

    // Replay attack: second approval attempt with the EXACT SAME short code -> MUST FAIL
    let resp2 = registry.approve_by_short_code(&challenge.short_code).await;
    assert!(
        resp2.is_err(),
        "Replaying an already-consumed short code MUST fail"
    );
}

#[tokio::test]
async fn test_sec_replay_challenge_id_approval_rejected() {
    let registry = PairingRegistry::with_random_secret("server_pub_sec_2");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Headless Node".to_string(),
        role: NodeRole::HeadlessNode,
        public_key: "ed25519_pub_key_2".to_string(),
        pairing_nonce: "nonce_2".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry
        .create_challenge(req, 300)
        .await
        .expect("challenge creation");

    // First approval -> succeeds
    let resp1 = registry
        .approve_by_challenge_id(&challenge.challenge_id)
        .await
        .expect("first approval succeeds");
    assert!(resp1.paired);

    // Replay attack: calling approve_by_challenge_id a second time -> MUST FAIL
    let resp2 = registry
        .approve_by_challenge_id(&challenge.challenge_id)
        .await;
    assert!(
        resp2.is_err(),
        "Replaying approval for challenge_id MUST fail closed"
    );
}

#[tokio::test]
async fn test_sec_replay_after_rejection_fails_closed() {
    let registry = PairingRegistry::with_random_secret("server_pub_sec_3");
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Suspicious Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pub_key_3".to_string(),
        pairing_nonce: "nonce_3".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry
        .create_challenge(req, 300)
        .await
        .expect("challenge creation");

    // Admin rejects the challenge
    registry
        .reject_challenge(&challenge.challenge_id, "Explicit admin rejection")
        .await
        .expect("reject challenge");

    // Attacker attempts to approve via short code -> MUST fail
    let approve_code_res = registry.approve_by_short_code(&challenge.short_code).await;
    assert!(approve_code_res.is_err(), "Rejected challenge cannot be approved via short code");

    // Attacker attempts to approve via challenge_id -> MUST fail
    let approve_id_res = registry
        .approve_by_challenge_id(&challenge.challenge_id)
        .await;
    assert!(approve_id_res.is_err(), "Rejected challenge cannot be approved via challenge_id");
}

#[tokio::test]
async fn test_sec_concurrent_race_condition_short_code_replay_atomic() {
    let registry = Arc::new(PairingRegistry::with_random_secret("server_pub_race"));
    let node_id = NodeId::new();

    let req = PairingRequest {
        node_id,
        node_name: "Race Target Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_race_pub".to_string(),
        pairing_nonce: "nonce_race".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry
        .create_challenge(req, 300)
        .await
        .expect("create challenge");
    let short_code = challenge.short_code.clone();

    // Spawn 20 concurrent tasks all racing to approve the EXACT SAME short code simultaneously
    let mut handles = Vec::new();
    for _ in 0..20 {
        let reg = registry.clone();
        let code = short_code.clone();
        handles.push(tokio::spawn(async move {
            reg.approve_by_short_code(&code).await
        }));
    }

    let mut success_count = 0;
    let mut failure_count = 0;

    for handle in handles {
        match handle.await.unwrap() {
            Ok(resp) => {
                assert!(resp.paired);
                success_count += 1;
            }
            Err(_) => {
                failure_count += 1;
            }
        }
    }

    // Exactly 1 approval must succeed, exactly 19 must fail
    assert_eq!(
        success_count, 1,
        "Atomic guarantee: exactly 1 concurrent approval should succeed"
    );
    assert_eq!(
        failure_count, 19,
        "Atomic guarantee: exactly 19 concurrent replays should fail"
    );
}

// ============================================================================
// SUITE 2: EXPIRED NONCES, CHALLENGES & TIME ATTACKS
// ============================================================================

#[tokio::test]
async fn test_sec_expired_challenge_ttl_zero_fails_approval() {
    let registry = PairingRegistry::with_random_secret("server_pub_exp_1");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Zero TTL Node".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "pubkey_0".to_string(),
        pairing_nonce: "nonce_0".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry
        .create_challenge(req, 0)
        .await
        .expect("create challenge");

    // Attempting approval on expired challenge -> MUST FAIL
    let res = registry.approve_by_short_code(&challenge.short_code).await;
    assert!(res.is_err(), "Expired challenge MUST fail approval");
}

#[tokio::test]
async fn test_sec_evict_expired_cleans_short_code_index_completely() {
    let registry = PairingRegistry::with_random_secret("server_pub_exp_2");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Transient Device".to_string(),
        role: NodeRole::MobileCompanion,
        public_key: "pubkey_transient".to_string(),
        pairing_nonce: "nonce_tr".to_string(),
        timestamp_unix: default_unix_now(),
    };

    let challenge = registry.create_challenge(req, 0).await.unwrap();
    let code = challenge.short_code.clone();

    // Sleep briefly to ensure clock tick
    tokio::time::sleep(Duration::from_millis(5)).await;

    let count = registry.evict_expired().await;
    assert!(count >= 1);

    // Short code index must be purged
    let res = registry.approve_by_short_code(&code).await;
    assert!(res.is_err());
    assert_eq!(res.err().unwrap(), "Invalid or expired pairing code");
}

#[test]
fn test_sec_expired_token_timestamp_boundary_checks() {
    let registry = PairingRegistry::with_random_secret("server_pub_exp_3");
    let node_id = NodeId::new();

    // 1. Token with timestamp 0 (1970 Unix epoch)
    let token_epoch_0 = registry.generate_token(node_id, NodeRole::MobileCompanion, 0);
    assert!(
        registry.verify_auth_token(&token_epoch_0).is_err(),
        "Epoch 0 token must be rejected as expired"
    );

    // 2. Token expired 1 second ago
    let now = default_unix_now();
    let token_past_1s = registry.generate_token(node_id, NodeRole::MobileCompanion, now - 1);
    assert!(
        registry.verify_auth_token(&token_past_1s).is_err(),
        "Token expired 1s ago must be rejected"
    );

    // 3. Token valid for 100 seconds
    let token_valid = registry.generate_token(node_id, NodeRole::MobileCompanion, now + 100);
    let verify_res = registry.verify_auth_token(&token_valid);
    assert!(verify_res.is_ok(), "Valid token must verify successfully");
}

// ============================================================================
// SUITE 3: TAMPERED HMAC SIGNATURES & PRIVILEGE ESCALATION
// ============================================================================

#[test]
fn test_sec_privilege_escalation_role_tampering_rejected() {
    let registry = PairingRegistry::with_random_secret("server_secret_priv_esc");
    let node_id = NodeId::new();
    let expires = default_unix_now() + 3600;

    // Generate token for a low-privilege MobileCompanion
    let token = registry.generate_token(node_id, NodeRole::MobileCompanion, expires);

    // Attack 1: Escalate role to DesktopUi (Full Admin Access)
    let tampered_desktop = token.replace("mobile_companion", "desktop_ui");
    assert!(
        registry.verify_auth_token(&tampered_desktop).is_err(),
        "Tampering role to desktop_ui MUST invalidate HMAC signature"
    );

    // Attack 2: Escalate role to CliTool (Full Admin Access)
    let tampered_cli = token.replace("mobile_companion", "cli_tool");
    assert!(
        registry.verify_auth_token(&tampered_cli).is_err(),
        "Tampering role to cli_tool MUST invalidate HMAC signature"
    );

    // Attack 3: Tamper to unknown role
    let tampered_unknown = token.replace("mobile_companion", "super_admin_root");
    assert!(
        registry.verify_auth_token(&tampered_unknown).is_err(),
        "Unknown role MUST fail validation"
    );
}

#[test]
fn test_sec_node_id_spoofing_tampering_rejected() {
    let registry = PairingRegistry::with_random_secret("server_secret_spoof");
    let legitimate_node_id = NodeId::new();
    let victim_node_id = NodeId::new();
    let expires = default_unix_now() + 3600;

    let token = registry.generate_token(legitimate_node_id, NodeRole::MobileCompanion, expires);

    // Attack: Replace legitimate node_id with victim_node_id
    let tampered_token = token.replace(
        &legitimate_node_id.to_string(),
        &victim_node_id.to_string(),
    );

    assert!(
        registry.verify_auth_token(&tampered_token).is_err(),
        "Tampering NodeId MUST invalidate HMAC signature"
    );
}

#[test]
fn test_sec_expiration_extension_tampering_rejected() {
    let registry = PairingRegistry::with_random_secret("server_secret_exp_ext");
    let node_id = NodeId::new();
    let original_exp = default_unix_now() + 60; // 1 minute
    let extended_exp = original_exp + 10 * 365 * 86400; // 10 years

    let token = registry.generate_token(node_id, NodeRole::MobileCompanion, original_exp);

    // Attack: Extend expiration timestamp
    let tampered_token = token.replace(
        &original_exp.to_string(),
        &extended_exp.to_string(),
    );

    assert!(
        registry.verify_auth_token(&tampered_token).is_err(),
        "Extending expiration timestamp MUST invalidate HMAC signature"
    );
}

#[test]
fn test_sec_version_confusion_tampering_rejected() {
    let registry = PairingRegistry::with_random_secret("server_secret_ver");
    let node_id = NodeId::new();
    let expires = default_unix_now() + 3600;

    let token = registry.generate_token(node_id, NodeRole::DesktopUi, expires);

    // Attack: Change version from v1 to v2 or v0
    let tampered_v2 = format!("v2{}", &token[2..]);
    let res = registry.verify_auth_token(&tampered_v2);
    assert!(res.is_err());
    assert!(res.err().unwrap().contains("Unsupported token version"));
}

#[test]
fn test_sec_signature_bitflip_and_truncation_rejected() {
    let registry = PairingRegistry::with_random_secret("server_secret_bits");
    let node_id = NodeId::new();
    let expires = default_unix_now() + 3600;

    let token = registry.generate_token(node_id, NodeRole::MobileCompanion, expires);
    let parts: Vec<&str> = token.split(':').collect();
    assert_eq!(parts.len(), 5);

    // 1. Truncated signature
    let truncated_sig = &parts[4][..16];
    let truncated_token = format!("{}:{}:{}:{}:{}", parts[0], parts[1], parts[2], parts[3], truncated_sig);
    assert!(
        registry.verify_auth_token(&truncated_token).is_err(),
        "Truncated HMAC signature MUST fail verification"
    );

    // 2. Bitflipped signature
    let mut sig_chars: Vec<char> = parts[4].chars().collect();
    sig_chars[0] = if sig_chars[0] == '0' { '1' } else { '0' };
    let bitflipped_sig: String = sig_chars.into_iter().collect();
    let bitflipped_token = format!("{}:{}:{}:{}:{}", parts[0], parts[1], parts[2], parts[3], bitflipped_sig);
    assert!(
        registry.verify_auth_token(&bitflipped_token).is_err(),
        "Bitflipped HMAC signature MUST fail verification"
    );
}

#[test]
fn test_sec_cross_server_secret_isolation() {
    let secret_a = [0x11u8; 32];
    let secret_b = [0x22u8; 32];

    let server_a = PairingRegistry::new(secret_a, "pubkey_a");
    let server_b = PairingRegistry::new(secret_b, "pubkey_b");

    let node_id = NodeId::new();
    let expires = default_unix_now() + 3600;

    // Token generated by Server A
    let token_a = server_a.generate_token(node_id, NodeRole::DesktopUi, expires);

    // Server A verifies -> OK
    assert!(server_a.verify_auth_token(&token_a).is_ok());

    // Server B verifies -> MUST FAIL (different HMAC secret)
    assert!(
        server_b.verify_auth_token(&token_a).is_err(),
        "Token signed by Server A MUST NOT be accepted by Server B"
    );
}

#[test]
fn test_sec_malformed_token_format_injection() {
    let registry = PairingRegistry::with_random_secret("server_secret_fuzz");

    let malformed_cases = vec![
        "",
        "v1",
        "v1:only_two_parts",
        "v1:node:role:expires",             // 4 parts, missing signature
        "v1:node:role:expires:sig:extra",   // 6 parts, extra field injection
        "::::",                             // Empty fields
        "v1:invalid-uuid:desktop_ui:1234567890:sig",
        "v1:00000000-0000-0000-0000-000000000000:invalid_role:1234567890:sig",
        "v1:00000000-0000-0000-0000-000000000000:desktop_ui:not_a_number:sig",
    ];

    for case in malformed_cases {
        assert!(
            registry.verify_auth_token(case).is_err(),
            "Malformed token {:?} must fail verification",
            case
        );
    }
}

// ============================================================================
// SUITE 4: UNAUTHORIZED TOPIC BROADCASTS & PUBSUB ISOLATION
// ============================================================================

#[tokio::test]
async fn test_sec_topic_isolation_strict_no_cross_leak() {
    let registry = PairingRegistry::with_random_secret("server_topic_iso");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node_a = NodeId::new();
    let (tx_a, mut rx_a) = mpsc::channel(16);

    let node_b = NodeId::new();
    let (tx_b, mut rx_b) = mpsc::channel(16);

    gateway.register_node(node_a, NodeRole::DesktopUi, tx_a).await.unwrap();
    gateway.register_node(node_b, NodeRole::MobileCompanion, tx_b).await.unwrap();

    // Node A subscribes to topic "secure.confidential.a"
    // Node B subscribes to topic "public.events"
    gateway.subscribe_topic(&node_a, "secure.confidential.a").await.unwrap();
    gateway.subscribe_topic(&node_b, "public.events").await.unwrap();

    // Broadcast secret event on topic "secure.confidential.a"
    let delivered = gateway
        .broadcast_event(
            "secure.confidential.a",
            serde_json::json!({ "secret_key": "CLASSIFIED_42" }),
        )
        .await
        .unwrap();

    assert_eq!(delivered, 1);

    // Node A must receive the message
    let msg_a = rx_a.try_recv().expect("Node A must receive its subscribed topic");
    assert_eq!(msg_a.opcode, ControlOpcode::Event);
    assert_eq!(msg_a.topic.as_deref(), Some("secure.confidential.a"));

    // Node B MUST NOT receive anything (Strict PubSub Isolation)
    assert!(
        rx_b.try_recv().is_err(),
        "Node B MUST NOT receive messages from un-subscribed topic secure.confidential.a"
    );
}

#[tokio::test]
async fn test_sec_unregistered_node_cleaned_up_from_subscriptions() {
    let registry = PairingRegistry::with_random_secret("server_topic_unreg");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node_id = NodeId::new();
    let (tx, mut rx) = mpsc::channel(16);

    gateway.register_node(node_id, NodeRole::MobileCompanion, tx).await.unwrap();
    gateway.subscribe_topic(&node_id, "channel.broadcast").await.unwrap();

    // First broadcast: node receives
    let sent1 = gateway
        .broadcast_event("channel.broadcast", serde_json::json!({ "id": 1 }))
        .await
        .unwrap();
    assert_eq!(sent1, 1);
    assert!(rx.try_recv().is_ok());

    // Unregister node
    gateway.unregister_node(&node_id).await.unwrap();

    // Second broadcast: node must receive 0 and not fail
    let sent2 = gateway
        .broadcast_event("channel.broadcast", serde_json::json!({ "id": 2 }))
        .await
        .unwrap();
    assert_eq!(sent2, 0, "Unregistered node should receive 0 messages");
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn test_sec_adversarial_topic_names_handled_safely() {
    let registry = PairingRegistry::with_random_secret("server_topic_adv");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let node_id = NodeId::new();
    let (tx, mut rx) = mpsc::channel(16);
    gateway.register_node(node_id, NodeRole::DesktopUi, tx).await.unwrap();

    let long_topic = "a".repeat(1000);
    let adversarial_topics = vec![
        "",
        "../../../etc/passwd",
        "topic with spaces and !@#$%^&*()",
        "🦀_unicode_topic_🚀_越南",
        long_topic.as_str(),
    ];

    for topic in adversarial_topics {
        gateway.subscribe_topic(&node_id, topic).await.unwrap();
        let sent = gateway
            .broadcast_event(topic, serde_json::json!({ "payload": "safe" }))
            .await
            .unwrap();
        assert_eq!(sent, 1);

        let received = rx.try_recv().expect("must receive frame safely");
        assert_eq!(received.topic.as_deref(), Some(topic));
    }
}

// ============================================================================
// SUITE 5: CORRUPTED CONTROL FRAMES & DESERIALIZATION FUZZING
// ============================================================================

#[test]
fn test_sec_corrupted_control_frame_json_fails_closed() {
    let corrupted_payloads = vec![
        "",
        "{",
        r#"{"frame_id": "123", "opcode": "invalid_unknown_opcode"}"#,
        r#"{"frame_id": 12345, "opcode": "event"}"#, // frame_id should be string
        r#"{"opcode": "event"}"#,                     // missing required frame_id
        r#"{"frame_id": "abc", "opcode": null}"#,
        r#"{"frame_id": "abc", "opcode": "command", "error": "not_an_object"}"#,
    ];

    for json_str in corrupted_payloads {
        let result: Result<ControlFrame, _> = serde_json::from_str(json_str);
        assert!(
            result.is_err(),
            "Corrupted JSON payload {:?} must fail deserialization",
            json_str
        );
    }
}

#[tokio::test]
async fn test_sec_control_frame_missing_topic_error_frames() {
    let registry = PairingRegistry::with_random_secret("server_frame_err");
    let gateway = InMemoryGatewayControlPlane::new(registry);
    let node_id = NodeId::new();

    // 1. Subscribe without topic
    let bad_sub = ControlFrame::new(ControlOpcode::Subscribe);
    let resp_sub = gateway
        .handle_frame(&node_id, bad_sub)
        .await
        .unwrap()
        .expect("should return error frame");
    assert_eq!(resp_sub.opcode, ControlOpcode::Error);
    assert_eq!(resp_sub.error.as_ref().unwrap().code, 400);

    // 2. Unsubscribe without topic
    let bad_unsub = ControlFrame::new(ControlOpcode::Unsubscribe);
    let resp_unsub = gateway
        .handle_frame(&node_id, bad_unsub)
        .await
        .unwrap()
        .expect("should return error frame");
    assert_eq!(resp_unsub.opcode, ControlOpcode::Error);
    assert_eq!(resp_unsub.error.as_ref().unwrap().code, 400);
}

#[test]
fn test_sec_control_frame_deeply_nested_json_payload() {
    // Generate a 50-level deeply nested JSON value
    let mut val = serde_json::json!("innermost_secret");
    for i in 0..50 {
        val = serde_json::json!({ format!("level_{}", i): val });
    }

    let frame = ControlFrame::event("deep.nesting", val);
    let serialized = serde_json::to_string(&frame).expect("serialize nested frame");
    let deserialized: ControlFrame =
        serde_json::from_str(&serialized).expect("deserialize nested frame");

    assert_eq!(frame.opcode, deserialized.opcode);
    assert_eq!(frame.topic, deserialized.topic);
}

#[tokio::test]
async fn test_sec_send_to_random_node_fails_closed() {
    let registry = PairingRegistry::with_random_secret("server_rnd_node");
    let gateway = InMemoryGatewayControlPlane::new(registry);

    let random_node = NodeId::new();
    let frame = ControlFrame::command("reboot", serde_json::json!({}));

    let res = gateway.send_node_frame(&random_node, frame).await;
    match res {
        Err(GatewayError::NodeNotFound(id)) => assert_eq!(id, random_node),
        other => panic!("Expected NodeNotFound, got {:?}", other),
    }
}

// ============================================================================
// SUITE 6: SESSION CONTEXT CONCURRENT ISOLATION & ROUTING FUZZING
// ============================================================================

#[tokio::test]
async fn test_sec_concurrent_session_variable_isolation_50_sessions() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let mut handles = Vec::new();

    // Spawn 50 concurrent tasks, each managing a unique user and writing unique secrets
    for i in 0..50 {
        let mgr_clone = mgr.clone();
        handles.push(tokio::spawn(async move {
            let user_id = format!("user_{:03}", i);
            let secret_val = format!("secret_token_val_{:03}", i);

            let session = mgr_clone
                .get_or_create_session(&ChannelId::Telegram, &user_id, None)
                .await
                .expect("create session");

            session
                .write()
                .await
                .set_variable("auth_token", serde_json::json!(secret_val));

            tokio::time::sleep(Duration::from_millis(5)).await;

            let read_secret = session
                .read()
                .await
                .get_variable("auth_token")
                .cloned()
                .expect("must have secret");

            assert_eq!(read_secret, serde_json::json!(secret_val));
            user_id
        }));
    }

    let mut completed_users = HashSet::new();
    for handle in handles {
        let user = handle.await.unwrap();
        completed_users.insert(user);
    }

    assert_eq!(completed_users.len(), 50);

    // Verify list_sessions contains 50 distinct sessions
    let all_sessions = mgr.list_sessions().await.expect("list sessions");
    assert_eq!(all_sessions.len(), 50);
}

#[test]
fn test_sec_session_route_resolver_adversarial_fuzzing() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);

    // Standard cases
    let (id1, scope1) = mgr.resolve_route("main");
    assert_eq!(id1, None);
    assert_eq!(scope1, MemoryScope::Persistent);

    let (id2, scope2) = mgr.resolve_route("isolated");
    assert_eq!(id2, None);
    assert_eq!(scope2, MemoryScope::Ephemeral);

    // Valid UUID route
    let test_uuid = Uuid::new_v4();
    let (id3, scope3) = mgr.resolve_route(&format!("session:{}", test_uuid));
    assert_eq!(id3, Some(SessionId::from_uuid(test_uuid)));
    assert_eq!(scope3, MemoryScope::Persistent);

    // Adversarial / Fuzzed routes
    let fuzzed_routes = vec![
        "",
        "   ",
        "session:",
        "session:not-a-uuid",
        "session:12345",
        "session:../../../etc/shadow",
        "session:SELECT * FROM users;",
        "session:🦀_unicode_🚀",
        "unknown_custom_route",
    ];

    for route in fuzzed_routes {
        let (id, scope) = mgr.resolve_route(route);
        if route.starts_with("session:") && route != "session:" {
            // Invalid UUID after "session:" falls back safely to None, Working
            assert_eq!(id, None, "Invalid UUID after session: prefix must produce None SessionId");
            assert_eq!(scope, MemoryScope::Working);
        } else {
            // Other non-matching routes fallback safely to default scope
            assert_eq!(id, None);
            assert_eq!(scope, MemoryScope::Working);
        }
    }
}

// ============================================================================
// SUITE 7: NORMALIZED INGRESS & EGRESS MESSAGE INTEGRITY
// ============================================================================

#[test]
fn test_sec_incoming_message_command_detection_adversarial() {
    let sender = MessageSender::user("u1", Some("User".into()));
    let session = SessionId::new();

    // Valid commands
    let valid_cmds = vec!["/help", "  /start  ", "/settings with args", "/tool_call_1"];
    for cmd in valid_cmds {
        let msg = IncomingMessage::text(ChannelId::Telegram, "msg1", session, sender.clone(), cmd);
        assert!(msg.is_command(), "Must identify {:?} as a command", cmd);
    }

    // Not commands
    let not_cmds = vec![
        "",
        "   ",
        "hello /help",
        "\\help",
        "!help",
        "This is not a /command",
    ];
    for not_cmd in not_cmds {
        let msg = IncomingMessage::text(ChannelId::Telegram, "msg2", session, sender.clone(), not_cmd);
        assert!(!msg.is_command(), "Must NOT identify {:?} as a command", not_cmd);
    }
}

#[test]
fn test_sec_incoming_message_extreme_payload_and_unicode_resilience() {
    let sender = MessageSender::user("u_vietnam", Some("Nguyễn Văn A".into()));
    let session = SessionId::new();

    let rich_unicode_text = "Xin chào LIVA! 🤖 Ứng dụng trợ lý ảo bảo mật cao 🔒 100% tiếng Việt: ă, â, đ, ê, ô, ơ, ư";
    let msg = IncomingMessage::text(
        ChannelId::WhatsApp,
        "wa_msg_vn_1",
        session,
        sender,
        rich_unicode_text,
    );

    let json = serde_json::to_string(&msg).expect("serialize unicode message");
    let parsed: IncomingMessage = serde_json::from_str(&json).expect("deserialize unicode message");

    assert_eq!(parsed.content.text_content(), Some(rich_unicode_text));
    assert_eq!(parsed.sender.display_name.as_deref(), Some("Nguyễn Văn A"));
}

#[test]
fn test_sec_attachment_sources_and_integrity_metadata() {
    let inline_data = Bytes::from_static(b"PK\x03\x04zip_binary_payload");
    let att = Attachment::from_inline_bytes("archive.zip", inline_data.clone(), "application/zip");

    assert_eq!(att.filename, "archive.zip");
    assert_eq!(att.size_bytes, inline_data.len() as u64);
    assert_eq!(att.mime_type, "application/zip");
    assert_eq!(att.source, AttachmentSource::Inline(inline_data));
}
