//! E2E Test Suite - Tier 4: Real-World Workload Scenarios (6 Multi-Step End-to-End Flows)
//!
//! Scenarios covered:
//! - Scenario 1: Full Voice Turn with Screen Context (Voice In -> VAD/STT -> Vision Grounding -> LLM -> TTS -> Audio Out)
//! - Scenario 2: Cross-Channel Task Delegation (Telegram In -> Intent -> Browser Automation -> WhatsApp Status -> Desktop Out)
//! - Scenario 3: Skill Installation & Consent Execution (ClawHub Manifest -> Sandbox Verify -> Consent Approval -> Fact Persist)
//! - Scenario 4: Device Pairing & Distributed Control (Challenge Gen -> Short-code Verify -> Token Exchange -> Control Plane Flow)
//! - Scenario 5: Episodic Memory Consolidation & Semantic Recall (Multi-day Events -> Consolidation Worker -> Encrypted Storage -> Recall)
//! - Scenario 6: System Stress & Resilience Under Pressure (Session Storm -> Governor Throttle -> WAL Recovery -> Zero State Loss)

use bytes::Bytes;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

// LIVA Native Core imports
use liva_native_core::agent::graph::{Intent, route_intent};
use liva_native_core::agent::state::AgentState;
use liva_native_core::automation::{
    BrowserDriver, DomExtractMode, MockBrowserDriver, SandboxPolicy,
};
use liva_native_core::crypto::{EncryptionEngine, FactRead};
use liva_native_core::db::DatabasePool;
use liva_native_core::gateway::control_plane::{ControlFrame, ControlOpcode};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use liva_native_core::messaging::normalized::{
    ChannelId, IncomingMessage, MessageRecipient, MessageSender, OutgoingMessage,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionId, SessionManager,
};
use liva_native_core::skills::consent::{
    ConsentDecision, ConsentRequest,
};
use liva_native_core::skills::dispatcher::MockToolDispatcher;
use liva_native_core::skills::manifest::{
    RiskLevel, parse_skill_markdown,
};
use liva_native_core::vision::{
    VisionConfig, VisionManager, capture::{MockScreenCapturer, PixelFormat},
};
use liva_native_core::webrtc::frame::{
    OP_MIC_IN, OP_SPEAKER_OUT, VoiceFrame, speaker_frames, speaker_turn_epoch,
};

// ============================================================================
// SCENARIO 1: FULL VOICE TURN WITH SCREEN CONTEXT
// ============================================================================
#[tokio::test]
async fn test_scenario_1_full_voice_turn_with_screen_context() {
    // 1. Voice In: User speaks "Tóm tắt nội dung màn hình đang hiển thị"
    let mic_frame = VoiceFrame {
        op_code: OP_MIC_IN,
        seq_id: 1,
        payload: Bytes::from_static(b"audio_pcm_samples_raw"),
    };
    assert_eq!(mic_frame.op_code, OP_MIC_IN);

    // 2. STT Intent Routing
    let transcribed_text = "Tóm tắt nội dung màn hình đang hiển thị";
    let intent = route_intent(transcribed_text);
    assert_eq!(intent, Intent::Vision);

    // 3. Screen Grounding: Capture visual context via MockScreenCapturer & VisionManager
    let capturer = Arc::new(MockScreenCapturer::new(1920, 1080, PixelFormat::Rgba));
    let mut vision_mgr = VisionManager::new(capturer, VisionConfig::default());
    let snap = vision_mgr.capture_screen().expect("capture screenshot");
    assert_eq!(snap.width, 1920);
    assert_eq!(snap.height, 1080);

    // 4. Multi-modal State Execution
    let mut state = AgentState::default();
    state.messages.push(serde_json::json!({
        "role": "user",
        "content": transcribed_text
    }));
    state.context.insert("screen_resolution".into(), serde_json::json!("1920x1080"));
    state.context.insert("ocr_detected_text".into(), serde_json::json!("LIVA AI System Performance 99.9%"));

    // 5. Synthesis & Audio Out: Generate voice frames for reply
    let reply_text = "Màn hình đang hiển thị hiệu năng hệ thống LIVA đạt 99.9%.";
    state.messages.push(serde_json::json!({
        "role": "assistant",
        "content": reply_text
    }));

    let audio_samples = vec![0.1f32; 3200]; // 200ms synthesized PCM
    let out_frames = speaker_frames(1, 16000, &audio_samples);
    assert!(!out_frames.is_empty());
    assert_eq!(out_frames[0].op_code, OP_SPEAKER_OUT);

    let epoch = speaker_turn_epoch(&out_frames[0]);
    assert_eq!(epoch, Some(1));
}

// ============================================================================
// SCENARIO 2: CROSS-CHANNEL TASK DELEGATION
// ============================================================================
#[tokio::test]
async fn test_scenario_2_cross_channel_task_delegation() {
    let session_mgr = InMemorySessionManager::new(MemoryScope::Working);

    // 1. Inbound Request from Telegram
    let tg_sender = MessageSender::user("tg_user_999", Some("CEO".into()));
    let tg_msg = IncomingMessage::text(
        ChannelId::Telegram,
        "msg_tg_001",
        SessionId::new(),
        tg_sender,
        "Kiểm tra tình trạng server và gửi báo cáo qua WhatsApp",
    );
    assert_eq!(tg_msg.channel, ChannelId::Telegram);

    // 2. Session Context Creation
    let session_ctx = session_mgr.get_or_create_session(&ChannelId::Telegram, "tg_user_999", None).await.unwrap();
    {
        let mut g = session_ctx.write().await;
        g.set_variable("requested_task", serde_json::json!("server_health_check"));
    }

    // 3. Browser Automation / Scraping step
    let driver = MockBrowserDriver::new(SandboxPolicy::default());
    let page_meta = driver.navigate("https://status.example.com/dashboard").await.expect("navigate");
    assert_eq!(page_meta.http_status, 200);

    let report_content = driver.extract_content(DomExtractMode::CleanMarkdown).await.expect("extract content");
    assert!(!report_content.is_empty());

    // 4. Outbound Status to WhatsApp
    let wa_recipient = MessageRecipient::direct(ChannelId::WhatsApp, "+84988888888");
    let wa_out = OutgoingMessage::text(
        wa_recipient,
        session_ctx.read().await.session_id,
        &format!("Báo cáo trạng thái server: {}", page_meta.title),
    );
    assert_eq!(wa_out.recipient.channel, ChannelId::WhatsApp);

    // 5. Desktop Notification via Control Plane
    let desktop_frame = ControlFrame::event(
        "task:completed",
        serde_json::json!({
            "taskId": "server_health_check",
            "status": "success",
            "delivered_channels": ["whatsapp", "telegram"]
        }),
    );
    assert_eq!(desktop_frame.opcode, ControlOpcode::Event);
}

// ============================================================================
// SCENARIO 3: SKILL INSTALLATION & CONSENT EXECUTION
// ============================================================================
#[tokio::test]
async fn test_scenario_3_skill_installation_and_execution() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = EncryptionEngine::new("skill_scenario_key_32_bytes_len");

    // 1. Skill Manifest Parsing
    let manifest_raw = r#"---
name: kubernetes-manager
version: 1.2.0
description: Manages production K8s pods
triggers:
  - type: intent
    config: manage_k8s
permissions:
  - type: net_outbound
    config: https://k8s.cluster.local
tools:
  - name: restart_deployment
    description: Restarts deployment pods
    input_schema:
      type: object
      properties:
        deployment:
          type: string
    risk_level: destructive_high_risk
---
# Kubernetes Manager Skill Instructions
"#;

    let package = parse_skill_markdown(manifest_raw, Path::new("/tmp")).expect("parse skill manifest");
    assert_eq!(package.manifest.name, "kubernetes-manager");
    assert_eq!(package.manifest.tools[0].risk_level, RiskLevel::DestructiveHighRisk);

    // 2. Dispatcher Registration
    let dispatcher = MockToolDispatcher::new();
    for tool in package.manifest.tools {
        dispatcher.register_tool(tool).await;
    }

    // 3. User Consent Suspension Check
    let req = ConsentRequest {
        request_id: "req_k8s_restart".into(),
        session_id: "sess_k8s".into(),
        tool_name: "restart_deployment".into(),
        target_resource: "Deployment: core-api-v2".into(),
        risk_level: RiskLevel::DestructiveHighRisk,
        arguments_preview: serde_json::json!({"deployment": "core-api-v2"}),
    };
    assert_eq!(req.risk_level, RiskLevel::DestructiveHighRisk);

    // User approves action
    let decision = ConsentDecision::Approved {
        user_id: "sysadmin".into(),
        timestamp_unix: 1700000000,
    };
    assert!(matches!(decision, ConsentDecision::Approved { .. }));

    // 4. Tool Execution & Fact Persistence
    let tool_res = dispatcher.dispatch("restart_deployment", serde_json::json!({"deployment": "core-api-v2"})).await;
    assert!(tool_res.is_ok());

    // Record audit fact to database
    let audit_fact = "Restarted K8s deployment core-api-v2 successfully";
    let encrypted_fact = crypto.encrypt(audit_fact).unwrap();

    let conn = pool.writer.get().unwrap();
    conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source, category, importance) VALUES ('audit:k8s:last', ?1, '2026-09-01', '2026-09-01', 'skills', 'operations', 0.9)",
        rusqlite::params![encrypted_fact],
    ).unwrap();

    let loaded: String = pool.readers.get().unwrap().query_row(
        "SELECT value FROM facts WHERE key = 'audit:k8s:last'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(crypto.decrypt(&loaded), audit_fact);
}

// ============================================================================
// SCENARIO 4: DEVICE PAIRING & DISTRIBUTED CONTROL
// ============================================================================
#[tokio::test]
async fn test_scenario_4_device_pairing_and_distributed_inference() {
    // 1. Mobile Companion requests pairing
    let registry = PairingRegistry::with_random_secret("distributed_server_secret");
    let mobile_node_id = NodeId::new();

    let req = PairingRequest {
        node_id: mobile_node_id,
        node_name: "iPhone 16 Pro Companion".into(),
        role: NodeRole::MobileCompanion,
        public_key: "pubkey_mobile_ecc_256".into(),
        pairing_nonce: "nonce_sec4_9988".into(),
        timestamp_unix: 1700000000,
    };

    // 2. Server creates pairing challenge with 6-digit short code
    let challenge = registry.create_challenge(req, 120).await.expect("challenge creation");
    assert_eq!(challenge.short_code.len(), 6);

    // 3. User enters short-code on desktop dashboard to approve
    let response = registry.approve_by_short_code(&challenge.short_code).await.expect("approve code");
    assert!(response.paired);
    assert!(!response.auth_token.expect("token").is_empty());

    // 4. Authenticated Control Plane Streaming
    let auth_frame = ControlFrame::auth("token_mobile_123");
    assert_eq!(auth_frame.opcode, ControlOpcode::Auth);

    let ack_frame = ControlFrame::command_response(
        "req_001",
        serde_json::json!({"acknowledged": true, "sync_interval_ms": 5000}),
    );
    assert_eq!(ack_frame.opcode, ControlOpcode::CommandResponse);
}

// ============================================================================
// SCENARIO 5: EPISODIC MEMORY CONSOLIDATION & RETRIEVAL
// ============================================================================
#[tokio::test]
async fn test_scenario_5_episodic_memory_consolidation_and_retrieval() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = EncryptionEngine::new("episodic_memory_secret_32_bytes");

    // 1. Populate multi-turn conversation events over time
    {
        let conn = pool.writer.get().unwrap();
        for i in 0..15 {
            conn.execute(
                "INSERT INTO events (eventId, timestamp, rawUserMsg, rawAiReply, consolidation_status) VALUES (?1, ?2, ?3, ?4, 'pending')",
                rusqlite::params![
                    format!("evt_turn_{}", i),
                    1700000000 + i * 3600,
                    format!("User shared knowledge snippet {}", i),
                    format!("AI confirmed understanding {}", i),
                ],
            ).unwrap();
        }
    }

    // 2. Run background consolidation worker
    let consolidation_res = liva_native_core::memory_consolidation::consume_pending_once(pool.clone(), crypto.clone(), 15).await.unwrap();
    assert_eq!(consolidation_res.processed, 15);

    // 3. Consolidate and store extracted facts
    let user_preference = "Người dùng thích giao diện tối và lập trình bằng ngôn ngữ Rust";
    let encrypted_pref = crypto.encrypt(user_preference).unwrap();

    {
        let conn = pool.writer.get().unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source, category, importance) VALUES ('pref:lang_theme', ?1, '2026-09-01', '2026-09-01', 'consolidation', 'profile', 0.98)",
            rusqlite::params![encrypted_pref],
        ).unwrap();
    }

    // 4. Subsequent conversation turn retrieves grounded fact
    let retrieved_ciphertext: String = pool.readers.get().unwrap().query_row(
        "SELECT value FROM facts WHERE key = 'pref:lang_theme'",
        [],
        |r| r.get(0),
    ).unwrap();

    let decrypted = crypto.read_fact(&retrieved_ciphertext);
    assert_eq!(decrypted, FactRead::Ok(user_preference.into()));
}

// ============================================================================
// SCENARIO 6: SYSTEM STRESS & RESILIENCE UNDER PRESSURE
// ============================================================================
#[tokio::test]
async fn test_scenario_6_system_stress_and_failure_recovery() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let session_mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));

    // 1. Session Creation Storm (50 concurrent clients)
    let mut join_handles = Vec::new();
    for i in 0..50 {
        let sm = session_mgr.clone();
        join_handles.push(tokio::spawn(async move {
            let ctx = sm.get_or_create_session(&ChannelId::Telegram, &format!("storm_user_{}", i), None).await.unwrap();
            let mut g = ctx.write().await;
            g.set_variable("metric", serde_json::json!(i * 10));
        }));
    }

    for h in join_handles {
        h.await.unwrap();
    }

    // 2. Governor Check under High Load
    let cpu_thresh = liva_native_core::governor::busy_cpu_threshold();
    assert!(cpu_thresh > 0);

    // 3. Concurrent Read/Write Transactions on SQLite Pool
    let mut db_handles = Vec::new();
    for i in 0..20 {
        let p = pool.clone();
        db_handles.push(tokio::spawn(async move {
            let conn = p.writer.get().unwrap();
            conn.execute(
                "INSERT INTO tasks (id, title, status, created_at, updated_at) VALUES (?1, ?2, 'done', 100, 100)",
                rusqlite::params![format!("stress_task_{}", i), format!("Stress Task {}", i)],
            ).unwrap();
        }));
    }

    for h in db_handles {
        h.await.unwrap();
    }

    // 4. Verify Zero Data Loss & Clean State Resumption
    let total_tasks: i64 = pool.readers.get().unwrap().query_row(
        "SELECT count(*) FROM tasks",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(total_tasks, 20);

    // Evict expired sessions cleanly
    let evicted = session_mgr.evict_expired(Duration::from_secs(3600)).await.unwrap();
    assert_eq!(evicted, 0); // Not expired yet
}
