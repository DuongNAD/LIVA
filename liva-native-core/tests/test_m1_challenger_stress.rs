//! Empirical Adversarial Challenger Test Suite for Milestone 1
//! Native Gateway & Control Plane
//!
//! Subsystems tested:
//! 1. `liva_native_core::messaging::session` (InMemorySessionManager, SessionContext, MemoryScope)
//! 2. `liva_native_core::messaging::normalized` (IncomingMessage, OutgoingMessage, ContentPayload, Attachment)
//! 3. `liva_native_core::gateway::pairing` (PairingRegistry, NodeRole, Token verification)
//! 4. `liva_native_core::gateway::control_plane` (InMemoryGatewayControlPlane, broadcast, backpressure)

use bytes::Bytes;
use chrono::Utc;
use liva_native_core::gateway::control_plane::{
    GatewayControlPlane, InMemoryGatewayControlPlane,
};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use liva_native_core::messaging::normalized::{
    Attachment, ChannelId, IncomingMessage, MessageSender,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionContext, SessionId, SessionManager,
};
use std::collections::HashSet;
use std::panic::{self, AssertUnwindSafe};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio::time::timeout;

// ============================================================================
// CHALLENGE 1: Concurrency & Potential Deadlocks in InMemorySessionManager
// ============================================================================

#[tokio::test]
async fn challenge_concurrency_deadlock_session_manager() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let num_tasks = 50;
    let iterations_per_task = 50;

    let mut handles = Vec::new();

    // Group A: Concurrent session creators and writers
    for task_idx in 0..num_tasks {
        let mgr_clone = mgr.clone();
        let handle = tokio::spawn(async move {
            for iter in 0..iterations_per_task {
                let user_id = format!("user_{}", (task_idx + iter) % 15);
                let thread_id = if iter % 2 == 0 {
                    Some(format!("thread_{}", iter % 5))
                } else {
                    None
                };
                let channel = if task_idx % 2 == 0 {
                    ChannelId::Telegram
                } else {
                    ChannelId::Discord
                };

                let ctx_arc = mgr_clone
                    .get_or_create_session(&channel, &user_id, thread_id.as_deref())
                    .await
                    .expect("get_or_create_session");

                {
                    let mut ctx = ctx_arc.write().await;
                    ctx.set_variable("last_task", serde_json::json!(task_idx));
                    ctx.set_variable("iter_count", serde_json::json!(iter));
                }

                if iter % 10 == 0 {
                    let _ = mgr_clone.list_sessions().await;
                }
            }
        });
        handles.push(handle);
    }

    // Group B: Concurrent evictor running in background
    let mgr_evict = mgr.clone();
    let evict_handle = tokio::spawn(async move {
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let _ = mgr_evict.evict_expired(Duration::from_millis(1)).await;
        }
    });
    handles.push(evict_handle);

    // Group C: Concurrent session terminators
    let mgr_term = mgr.clone();
    let term_handle = tokio::spawn(async move {
        for _ in 0..25 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let list = mgr_term.list_sessions().await.unwrap_or_default();
            if let Some(first) = list.first() {
                let _ = mgr_term.terminate_session(&first.session_id).await;
            }
        }
    });
    handles.push(term_handle);

    // Wrap with timeout to detect deadlock
    let res = timeout(Duration::from_secs(10), async {
        for h in handles {
            h.await.expect("join handle");
        }
    })
    .await;

    assert!(
        res.is_ok(),
        "CRITICAL: Concurrency deadlock detected in InMemorySessionManager!"
    );
}

// ============================================================================
// CHALLENGE 2: Race Condition on Simultaneous Same-Key Session Creation
// ============================================================================

#[tokio::test]
async fn challenge_race_condition_identical_session_creation() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let num_threads = 50;

    let barrier = Arc::new(tokio::sync::Barrier::new(num_threads));
    let mut handles = Vec::new();

    for i in 0..num_threads {
        let mgr_clone = mgr.clone();
        let b = barrier.clone();
        let handle = tokio::spawn(async move {
            b.wait().await; // Synchronize start
            let session = mgr_clone
                .get_or_create_session(&ChannelId::Slack, "race_user", Some("same_thread"))
                .await
                .expect("get_or_create_session");
            let id = session.read().await.session_id;

            // Concurrent variable mutation
            session
                .write()
                .await
                .set_variable(&format!("key_{}", i), serde_json::json!(i));

            id
        });
        handles.push(handle);
    }

    let mut session_ids = HashSet::new();
    for h in handles {
        let id = h.await.expect("join");
        session_ids.insert(id);
    }

    // Exactly 1 unique session ID must be returned to all 50 concurrent callers
    assert_eq!(
        session_ids.len(),
        1,
        "Race condition: Multiple different sessions created for the identical user/thread key!"
    );

    let session = mgr
        .get_or_create_session(&ChannelId::Slack, "race_user", Some("same_thread"))
        .await
        .unwrap();
    let ctx = session.read().await;
    assert_eq!(ctx.variables.len(), num_threads);
}

// ============================================================================
// CHALLENGE 3: UTF-8 Multi-Byte Character Boundary Slicing in IncomingMessage::summary()
// ============================================================================

#[test]
fn challenge_utf8_char_boundary_panic_in_summary() {
    // Construct a string where byte 80 is in the middle of a multibyte UTF-8 char.
    let prefix = "A".repeat(79);
    let multibyte_char = "ế"; // 'ế' in UTF-8 is 3 bytes: 0xE1 0xBA 0xBF
    let test_text = format!("{}{}", prefix, multibyte_char);

    assert_eq!(test_text.len(), 82); // 79 + 3 = 82 bytes, > 80 bytes
    // Byte 80 is byte index 1 of 'ế', NOT a character boundary!

    let session_id = SessionId::new();
    let sender = MessageSender::user("u1", None);
    let msg = IncomingMessage::text(
        ChannelId::Telegram,
        "msg_utf8_panic",
        session_id,
        sender,
        test_text,
    );

    // Call summary() wrapped in catch_unwind
    let result = panic::catch_unwind(AssertUnwindSafe(|| {
        msg.summary()
    }));

    if let Err(err) = result {
        println!("CONFIRMED BUG: IncomingMessage::summary() panicked on UTF-8 char boundary!");
        panic!(
            "VULNERABILITY FOUND: IncomingMessage::summary() sliced &txt[..80] across UTF-8 boundary: {:?}",
            err
        );
    }
}

// ============================================================================
// CHALLENGE 4: TTL Expiry with Duration::MAX (ChronoDuration Out-of-Range)
// ============================================================================

#[test]
fn challenge_ttl_expiry_duration_max_behavior() {
    let mut ctx = SessionContext::new(
        ChannelId::LocalCli,
        "admin",
        None,
        MemoryScope::Working,
    );

    ctx.last_active_at = Utc::now();

    // If a user sets TTL to Duration::MAX (meaning: never expire),
    // ChronoDuration::from_std(Duration::MAX) returns Err, which unwrap_or(zero) turns into 0s TTL!
    let is_expired_max = ctx.is_expired(Duration::MAX);

    println!("is_expired(Duration::MAX) = {}", is_expired_max);
    assert!(
        !is_expired_max,
        "BUG: is_expired(Duration::MAX) returned true because ChronoDuration::from_std overflowed to 0s!"
    );
}

// ============================================================================
// CHALLENGE 5: Extreme Payloads, Fuzzing & Boundaries on Normalizer
// ============================================================================

#[test]
fn challenge_extreme_payloads_and_fuzzing() {
    let session_id = SessionId::new();
    let sender = MessageSender::admin("admin_root", Some("Admin".into()));

    // 1. Extreme 10MB text string
    let huge_text = "X".repeat(10 * 1024 * 1024);
    let msg_huge = IncomingMessage::text(
        ChannelId::Discord,
        "msg_huge",
        session_id,
        sender.clone(),
        &huge_text,
    );
    assert_eq!(msg_huge.content.text_content().map(|s| s.len()), Some(10 * 1024 * 1024));

    // 2. 1,000 attachments in single message
    let mut attachments = Vec::with_capacity(1000);
    for i in 0..1000 {
        attachments.push(Attachment::from_inline_bytes(
            format!("file_{}.dat", i),
            Bytes::from_static(b"data"),
            "application/octet-stream",
        ));
    }
    let msg_multi_att = IncomingMessage::text(
        ChannelId::Slack,
        "msg_att",
        session_id,
        sender.clone(),
        "Bulk files",
    )
    .with_attachments(attachments);

    assert_eq!(msg_multi_att.attachments.len(), 1000);

    // 3. Deeply nested metadata (fuzzing JSON serialization)
    let mut nested_json = serde_json::json!({ "depth": 0 });
    for d in 1..50 {
        nested_json = serde_json::json!({ "depth": d, "child": nested_json });
    }
    let msg_nested = IncomingMessage::text(
        ChannelId::WebSocketDashboard,
        "msg_nest",
        session_id,
        sender.clone(),
        "Deep JSON metadata",
    )
    .with_metadata("nested", nested_json.clone());

    let serialized = serde_json::to_string(&msg_nested).expect("serialize deeply nested JSON");
    let deserialized: IncomingMessage =
        serde_json::from_str(&serialized).expect("deserialize deeply nested JSON");
    assert_eq!(deserialized.metadata.get("nested"), Some(&nested_json));

    // 4. Special Characters & Null Bytes
    let special_payload = "NULL\0BYTE\r\n\t<script>alert(1)</script>'; DROP TABLE messages; -- \u{0000}\u{FFFF}";
    let msg_special = IncomingMessage::text(
        ChannelId::WhatsApp,
        "msg_spec",
        session_id,
        sender,
        special_payload,
    );
    let json_spec = serde_json::to_string(&msg_special).unwrap();
    let parsed_spec: IncomingMessage = serde_json::from_str(&json_spec).unwrap();
    assert_eq!(parsed_spec.content.text_content(), Some(special_payload));
}

// ============================================================================
// CHALLENGE 6: Zero-Trust Token Verification Fuzzing & Boundary Cases
// ============================================================================

#[test]
fn challenge_token_verification_fuzzing() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let node_id = NodeId::new();

    // 1. Valid token baseline
    let valid_token = registry.generate_token(node_id, NodeRole::MobileCompanion, 2_000_000_000);
    assert!(registry.verify_auth_token(&valid_token).is_ok());

    // 2. Malformed token formats (various colon counts)
    let malformed_cases = vec![
        "",
        "v1",
        "v1:node",
        "v1:node:role",
        "v1:node:role:expires",
        "v1:node:role:expires:sig:extra",
        "v2:node:role:expires:sig",
        "v1:not-a-uuid:mobile_companion:2000000000:sig",
        "v1:00000000-0000-0000-0000-000000000000:invalid_role:2000000000:sig",
        "v1:00000000-0000-0000-0000-000000000000:mobile_companion:not-a-number:sig",
        "v1:00000000-0000-0000-0000-000000000000:mobile_companion:-100:sig",
        "v1:00000000-0000-0000-0000-000000000000:mobile_companion:9999999999999999999999999999:sig",
    ];

    for bad_token in malformed_cases {
        let res = registry.verify_auth_token(bad_token);
        assert!(res.is_err(), "Expected error for bad token: {:?}", bad_token);
    }
}

// ============================================================================
// CHALLENGE 7: Gateway Broadcast Backpressure & Slow Receiver Isolation
// ============================================================================

#[tokio::test]
async fn challenge_gateway_slow_node_does_not_deadlock_broadcast() {
    let registry = PairingRegistry::with_random_secret("srv_pub");
    let gateway = Arc::new(InMemoryGatewayControlPlane::new(registry));

    // Node 1: Fast receiver
    let node1 = NodeId::new();
    let (tx1, _rx1) = mpsc::channel(100);
    gateway
        .register_node(node1, NodeRole::DesktopUi, tx1)
        .await
        .unwrap();
    gateway.subscribe_topic(&node1, "events.live").await.unwrap();

    // Node 2: Slow/blocked receiver with channel capacity = 1
    let node2 = NodeId::new();
    let (tx2, _rx2) = mpsc::channel(1); // will fill up immediately
    gateway
        .register_node(node2, NodeRole::MobileCompanion, tx2)
        .await
        .unwrap();
    gateway.subscribe_topic(&node2, "events.live").await.unwrap();

    // Fill node2's buffer
    let _ = gateway
        .broadcast_event("events.live", serde_json::json!({ "seq": 0 }))
        .await;

    // Concurrently broadcast 10 events with a timeout to verify no indefinite block
    let gw_clone = gateway.clone();
    let bcast_task = tokio::spawn(async move {
        for i in 1..=5 {
            let _ = gw_clone
                .broadcast_event("events.live", serde_json::json!({ "seq": i }))
                .await;
        }
    });

    let res = timeout(Duration::from_secs(2), bcast_task).await;
    assert!(
        res.is_ok(),
        "Gateway broadcast blocked indefinitely due to single slow/unresponsive node!"
    );
}

// ============================================================================
// CHALLENGE 8: Concurrent Pairing Challenge Storm & Code Collision Resilience
// ============================================================================

#[tokio::test]
async fn challenge_concurrent_pairing_challenge_storm() {
    let registry = Arc::new(PairingRegistry::with_random_secret("srv_pub"));
    let num_requests = 100;
    let mut handles = Vec::new();

    for i in 0..num_requests {
        let reg = registry.clone();
        let handle = tokio::spawn(async move {
            let req = PairingRequest {
                node_id: NodeId::new(),
                node_name: format!("Node_{}", i),
                role: NodeRole::MobileCompanion,
                public_key: format!("pubkey_{}", i),
                pairing_nonce: format!("nonce_{}", i),
                timestamp_unix: 1_700_000_000,
            };
            reg.create_challenge(req, 60).await
        });
        handles.push(handle);
    }

    let mut challenges = Vec::new();
    for h in handles {
        let ch = h.await.expect("join").expect("create_challenge");
        challenges.push(ch);
    }

    assert_eq!(challenges.len(), num_requests);
    let pending = registry.list_pending_challenges().await;
    assert!(pending.len() <= num_requests);
}
