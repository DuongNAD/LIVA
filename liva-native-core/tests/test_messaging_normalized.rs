//! Integration & Verification Test Suite for Features 1 & 2:
//! - Feature 1: Unified Ingress Message Normalizer
//! - Feature 2: Session & Context Isolation Router

use bytes::Bytes;
use chrono::{Duration as ChronoDuration, Utc};
use liva_native_core::messaging::normalized::{
    Attachment, ChannelId, ContentPayload, DeliveryReceipt, DeliveryState, DeliveryUrgency,
    IncomingMessage, MessageId, MessageRecipient, MessageSender, OutgoingMessage, TextEntity,
    TextEntityType,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionContext, SessionId, SessionManager,
};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// FEATURE 1: TIER 1 - FEATURE COVERAGE TESTS (≥5 tests)
// ============================================================================

#[test]
fn test_tier1_incoming_message_normalization_all_channels() {
    let session_id = SessionId::new();
    let channels = vec![
        ChannelId::Telegram,
        ChannelId::WhatsApp,
        ChannelId::Discord,
        ChannelId::Slack,
        ChannelId::WebSocketWidget,
        ChannelId::WebSocketDashboard,
        ChannelId::WebSocketCompanion("node_ipad".into()),
        ChannelId::LocalCli,
        ChannelId::Custom("matrix_bridge".into()),
    ];

    for channel in channels {
        let sender = MessageSender::user("usr_42", Some("User Name".into()));
        let msg = IncomingMessage::text(
            channel.clone(),
            "msg_chan_001",
            session_id,
            sender,
            "Xin chào LIVA native!",
        );

        assert_eq!(msg.channel, channel);
        assert_eq!(msg.content.text_content(), Some("Xin chào LIVA native!"));
        assert!(!msg.is_command());

        // Test serialization roundtrip
        let json = serde_json::to_string(&msg).expect("serialize incoming message");
        let parsed: IncomingMessage =
            serde_json::from_str(&json).expect("deserialize incoming message");
        assert_eq!(parsed.id, msg.id);
        assert_eq!(parsed.channel, channel);
    }
}

#[test]
fn test_tier1_incoming_message_attachments_various_sources() {
    let inline_bytes = Bytes::from_static(b"audio-raw-pcm-data");
    let att1 = Attachment::from_inline_bytes("voice.wav", inline_bytes, "audio/wav");
    let att2 = Attachment::from_local_path(
        "report.pdf",
        PathBuf::from("/data/docs/report.pdf"),
        "application/pdf",
        10240,
    );
    let att3 = Attachment::from_url(
        "photo.jpg",
        "https://example.com/photo.jpg",
        "image/jpeg",
        2048,
    );

    let session_id = SessionId::new();
    let sender = MessageSender::user("alice", Some("Alice".into()));
    let msg = IncomingMessage::text(
        ChannelId::Telegram,
        "msg_123",
        session_id,
        sender,
        "Here are 3 files",
    )
    .with_attachments(vec![att1, att2, att3]);

    assert_eq!(msg.attachments.len(), 3);
    assert_eq!(msg.attachments[0].filename, "voice.wav");
    assert_eq!(msg.attachments[1].filename, "report.pdf");
    assert_eq!(msg.attachments[2].filename, "photo.jpg");
}

#[test]
fn test_tier1_incoming_message_rich_text_entities() {
    let session_id = SessionId::new();
    let sender = MessageSender::user("bob", Some("Bob".into()));
    let entities = vec![
        TextEntity {
            offset: 0,
            length: 5,
            entity_type: TextEntityType::BotCommand,
        },
        TextEntity {
            offset: 6,
            length: 5,
            entity_type: TextEntityType::Mention,
        },
        TextEntity {
            offset: 12,
            length: 8,
            entity_type: TextEntityType::Code,
        },
    ];

    let content = ContentPayload::RichText {
        text: "/help @liva `status`".to_string(),
        entities: entities.clone(),
    };

    let msg = IncomingMessage {
        id: MessageId::new(),
        channel: ChannelId::Discord,
        channel_message_id: "disc_999".into(),
        session_id,
        sender,
        timestamp: Utc::now(),
        content,
        attachments: Vec::new(),
        reply_to_message_id: None,
        metadata: HashMap::new(),
    };

    assert!(msg.is_command());
    assert_eq!(msg.content.text_content(), Some("/help @liva `status`"));
}

#[test]
fn test_tier1_outgoing_message_types() {
    let session_id = SessionId::new();
    let recipient = MessageRecipient::direct(ChannelId::Slack, "C12345");

    let text_msg = OutgoingMessage::text(recipient.clone(), session_id, "Standard answer");
    assert_eq!(text_msg.urgency, DeliveryUrgency::Standard);

    let stream_msg =
        OutgoingMessage::stream_chunk(recipient.clone(), session_id, "chunk #1", false);
    assert_eq!(stream_msg.urgency, DeliveryUrgency::Immediate);

    let att = Attachment::from_url("chart.png", "https://cdn/chart.png", "image/png", 5000);
    let media_msg = OutgoingMessage::media(recipient.clone(), session_id, att);
    assert_eq!(media_msg.urgency, DeliveryUrgency::Standard);
}

#[test]
fn test_tier1_delivery_receipt_lifecycle() {
    let msg_id = MessageId::new();
    let now = Utc::now();

    let states = vec![
        DeliveryState::Sent,
        DeliveryState::Delivered,
        DeliveryState::Read,
        DeliveryState::Failed("Network timeout after 3 retries".into()),
    ];

    for state in states {
        let receipt = DeliveryReceipt {
            message_id: msg_id,
            channel: ChannelId::WhatsApp,
            channel_message_id: "wa_biz_001".into(),
            delivered_at: now,
            state: state.clone(),
        };

        let json = serde_json::to_string(&receipt).expect("serialize receipt");
        let parsed: DeliveryReceipt = serde_json::from_str(&json).expect("deserialize receipt");
        assert_eq!(parsed.message_id, msg_id);
        assert_eq!(parsed.state, state);
    }
}

// ============================================================================
// FEATURE 1: TIER 2 - BOUNDARY & CORNER CASES (≥5 tests)
// ============================================================================

#[test]
fn test_tier2_empty_or_whitespace_payload_handling() {
    let empty_text = ContentPayload::Text("   \n\t  ".into());
    assert!(empty_text.is_empty());

    let empty_rich = ContentPayload::RichText {
        text: "".into(),
        entities: vec![],
    };
    assert!(empty_rich.is_empty());

    let non_empty = ContentPayload::Text(".".into());
    assert!(!non_empty.is_empty());
}

#[test]
fn test_tier2_very_large_attachment_handling() {
    let huge_bytes = Bytes::from(vec![0xAA; 5 * 1024 * 1024]); // 5 MB
    let att = Attachment::from_inline_bytes("huge_dump.bin", huge_bytes, "application/octet-stream");

    assert_eq!(att.size_bytes, 5 * 1024 * 1024);
    assert_eq!(att.filename, "huge_dump.bin");
}

#[test]
fn test_tier2_unicode_and_special_character_preservation() {
    let unicode_text = "Chào bạn! 🚀 Ứng dụng LIVA: 🔥 (100% tiếng Việt + math symbols: ∑ ∫ π ≠ ∞)";
    let session_id = SessionId::new();
    let sender = MessageSender::user("u1", Some("Đặng Nam".into()));
    let msg = IncomingMessage::text(
        ChannelId::Telegram,
        "tg_uni_1",
        session_id,
        sender,
        unicode_text,
    );

    let json = serde_json::to_string(&msg).unwrap();
    let parsed: IncomingMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.content.text_content(), Some(unicode_text));
}

#[test]
fn test_tier2_custom_channel_serialization_fidelity() {
    let channel = ChannelId::Custom("internal_subagent_mesh".into());
    assert_eq!(channel.as_str(), "internal_subagent_mesh");
    assert!(!channel.is_websocket());
    assert!(!channel.is_chat_platform());

    let json = serde_json::to_string(&channel).unwrap();
    let parsed: ChannelId = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed, channel);
}

#[test]
fn test_tier2_corrupted_json_payload_fail_closed() {
    let bad_json = r#"{"id":"not-a-uuid","channel":{"type":"unknown"}}"#;
    let res: Result<IncomingMessage, _> = serde_json::from_str(bad_json);
    assert!(res.is_err());
}

// ============================================================================
// FEATURE 2: TIER 1 - FEATURE COVERAGE TESTS (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_tier1_session_manager_crud_and_isolation() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let s_tg = mgr
        .get_or_create_session(&ChannelId::Telegram, "tg_user_1", None)
        .await
        .expect("create tg session");
    let s_wa = mgr
        .get_or_create_session(&ChannelId::WhatsApp, "wa_user_1", None)
        .await
        .expect("create wa session");

    let id_tg = s_tg.read().await.session_id;
    let id_wa = s_wa.read().await.session_id;
    assert_ne!(id_tg, id_wa);

    // Assert isolated state
    s_tg.write()
        .await
        .set_variable("secret_tg", serde_json::json!("tg_value"));
    s_wa.write()
        .await
        .set_variable("secret_wa", serde_json::json!("wa_value"));

    assert_eq!(
        s_tg.read().await.get_variable("secret_tg"),
        Some(&serde_json::json!("tg_value"))
    );
    assert_eq!(s_tg.read().await.get_variable("secret_wa"), None);
    assert_eq!(
        s_wa.read().await.get_variable("secret_wa"),
        Some(&serde_json::json!("wa_value"))
    );
    assert_eq!(s_wa.read().await.get_variable("secret_tg"), None);
}

#[test]
fn test_tier1_session_memory_scopes_classification() {
    assert!(MemoryScope::Ephemeral.is_ephemeral());
    assert!(!MemoryScope::Ephemeral.is_persisted());

    assert!(!MemoryScope::Working.is_ephemeral());
    assert!(!MemoryScope::Working.is_persisted());

    assert!(MemoryScope::Persistent.is_persisted());
    assert!(!MemoryScope::Persistent.is_ephemeral());

    assert!(MemoryScope::VaultBound.is_persisted());
    assert!(!MemoryScope::VaultBound.is_ephemeral());
}

#[tokio::test]
async fn test_tier1_session_variable_storage_and_mutation() {
    let mut ctx = SessionContext::new(
        ChannelId::LocalCli,
        "developer",
        None,
        MemoryScope::Working,
    );

    ctx.set_variable("model", serde_json::json!("qwen-2.5-7b"));
    ctx.set_variable("temperature", serde_json::json!(0.7));

    assert_eq!(
        ctx.get_variable("model"),
        Some(&serde_json::json!("qwen-2.5-7b"))
    );
    assert_eq!(
        ctx.get_variable("temperature"),
        Some(&serde_json::json!(0.7))
    );

    let removed = ctx.remove_variable("temperature");
    assert_eq!(removed, Some(serde_json::json!(0.7)));
    assert_eq!(ctx.get_variable("temperature"), None);
}

#[tokio::test]
async fn test_tier1_session_touch_and_expiration_detection() {
    let mut ctx = SessionContext::new(
        ChannelId::WebSocketWidget,
        "widget_user",
        None,
        MemoryScope::Ephemeral,
    );

    assert!(!ctx.is_expired(Duration::from_secs(10)));

    // Shift last_active_at into the past
    ctx.last_active_at = Utc::now() - ChronoDuration::seconds(30);
    assert!(ctx.is_expired(Duration::from_secs(10)));

    // Touch resets activity
    ctx.touch();
    assert!(!ctx.is_expired(Duration::from_secs(10)));
}

#[tokio::test]
async fn test_tier1_session_manager_eviction_of_expired_only() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let s_active = mgr
        .get_or_create_session(&ChannelId::Discord, "user_active", None)
        .await
        .unwrap();
    let s_expired = mgr
        .get_or_create_session(&ChannelId::Discord, "user_expired", None)
        .await
        .unwrap();

    let active_id = s_active.read().await.session_id;
    let expired_id = s_expired.read().await.session_id;

    // Age s_expired
    {
        let mut expired_guard = s_expired.write().await;
        expired_guard.last_active_at = Utc::now() - ChronoDuration::seconds(100);
    }

    let evicted = mgr.evict_expired(Duration::from_secs(30)).await.unwrap();
    assert_eq!(evicted, 1);

    assert!(mgr.get_session(&active_id).await.unwrap().is_some());
    assert!(mgr.get_session(&expired_id).await.unwrap().is_none());
}

// ============================================================================
// FEATURE 2: TIER 2 - BOUNDARY & CORNER CASES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_tier2_concurrent_session_access_no_deadlock() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let mut handles = Vec::new();

    for i in 0..20 {
        let mgr_clone = mgr.clone();
        let handle = tokio::spawn(async move {
            let user_id = format!("user_{}", i % 5);
            let s = mgr_clone
                .get_or_create_session(&ChannelId::Telegram, &user_id, None)
                .await
                .expect("concurrent session get");
            let mut guard = s.write().await;
            guard.set_variable("counter", serde_json::json!(i));
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("task join");
    }

    let sessions = mgr.list_sessions().await.unwrap();
    assert_eq!(sessions.len(), 5);
}

#[test]
fn test_tier2_session_route_resolver_edge_cases() {
    let mgr = InMemorySessionManager::default();

    // Normal paths
    assert_eq!(mgr.resolve_route("main"), (None, MemoryScope::Persistent));
    assert_eq!(mgr.resolve_route("isolated"), (None, MemoryScope::Ephemeral));

    // Whitespace trimming
    assert_eq!(
        mgr.resolve_route("   main   "),
        (None, MemoryScope::Persistent)
    );
    assert_eq!(
        mgr.resolve_route("\n isolated \t"),
        (None, MemoryScope::Ephemeral)
    );

    // Malformed session prefix
    let (id_bad, scope_bad) = mgr.resolve_route("session:not-a-uuid");
    assert_eq!(id_bad, None);
    assert_eq!(scope_bad, MemoryScope::Working);
}

#[tokio::test]
async fn test_tier2_evict_with_zero_ttl() {
    let mgr = InMemorySessionManager::new(MemoryScope::Ephemeral);
    let s = mgr
        .get_or_create_session(&ChannelId::Slack, "bot_runner", None)
        .await
        .unwrap();
    let id = s.read().await.session_id;

    // Small delay to ensure timestamp difference
    tokio::time::sleep(Duration::from_millis(5)).await;
    let evicted = mgr.evict_expired(Duration::from_millis(1)).await.unwrap();
    assert_eq!(evicted, 1);
    assert!(mgr.get_session(&id).await.unwrap().is_none());
}

#[tokio::test]
async fn test_tier2_terminate_nonexistent_session_returns_error() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let random_id = SessionId::new();
    let res = mgr.terminate_session(&random_id).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_tier2_persist_already_persistent_idempotent() {
    let mgr = InMemorySessionManager::new(MemoryScope::Persistent);
    let s = mgr
        .get_or_create_session(&ChannelId::Telegram, "owner", None)
        .await
        .unwrap();
    let id = s.read().await.session_id;

    assert_eq!(s.read().await.memory_scope, MemoryScope::Persistent);
    mgr.persist_session(&id).await.expect("persist 1");
    mgr.persist_session(&id).await.expect("persist 2");
    assert_eq!(s.read().await.memory_scope, MemoryScope::Persistent);
}

// ============================================================================
// TIER 3 - PAIRWISE & CROSS-FEATURE COMBINATIONS
// ============================================================================

#[tokio::test]
async fn test_tier3_incoming_message_routed_to_isolated_session() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let s = mgr
        .get_or_create_session(&ChannelId::Discord, "user_discord", Some("thread_general"))
        .await
        .unwrap();
    let session_id = s.read().await.session_id;

    let sender = MessageSender::user("user_discord", Some("Discord User".into()));
    let msg = IncomingMessage::text(
        ChannelId::Discord,
        "disc_101",
        session_id,
        sender,
        "Run search in thread",
    );

    // Assert message session aligns with session context
    assert_eq!(msg.session_id, session_id);
    assert_eq!(s.read().await.thread_id.as_deref(), Some("thread_general"));
}

#[tokio::test]
async fn test_tier3_multi_channel_session_isolation_cross_contamination_prevented() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let tg_session = mgr
        .get_or_create_session(&ChannelId::Telegram, "shared_user_id", None)
        .await
        .unwrap();
    let wa_session = mgr
        .get_or_create_session(&ChannelId::WhatsApp, "shared_user_id", None)
        .await
        .unwrap();

    let tg_id = tg_session.read().await.session_id;
    let wa_id = wa_session.read().await.session_id;

    // Even with the same user_id string, distinct channels MUST yield distinct session contexts
    assert_ne!(tg_id, wa_id);
}
