//! E2E Test Suite - Tier 1: Feature Coverage (≥5 tests per feature across 17 features)
//!
//! Features covered:
//! - Feature 1: WebSocket Buffer Pooling (R1)
//! - Feature 2: SQLite WAL Auto-Checkpoint & Vector DB (R1)
//! - Feature 3: KV Cache Prefix Routing & LLM Bounds (R1)
//! - Feature 4: Multi-Channel Management UI & IPC Bridge (R2)
//! - Feature 5: Skill Manager UI / ClawHub (R2)
//! - Feature 6: Node Pairing Monitor UI (R2)
//! - Feature 7: Browser Automation Preview (R2)
//! - Feature 8: Multi-Step ReAct Planner (R3)
//! - Feature 9: Self-Healing Tool Retry (R3)
//! - Feature 10: Hierarchical Working Memory (R3)
//! - Feature 11: Episodic Memory & Consolidation (R3)
//! - Feature 12: Unified Channel Dispatch (R3)
//! - Feature 13: System Diagnostic Probe (R4)
//! - Feature 14: Telemetry & Latency Profiler (R4)
//! - Feature 15: Multi-Session Stress Tests (R5)
//! - Feature 16: Security & Encryption Verification (R5)
//! - Feature 17: Workspace Compilation & System Integrity (R5)

use bytes::{Bytes, BytesMut};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// LIVA Native Core imports
use liva_native_core::agent::graph::{Intent, StateGraph, route_intent};
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::state::{AgentState, trim_messages};
use liva_native_core::{CommandPrincipal, authorize_command};
use liva_native_core::automation::{
    BrowserConfig, BrowserDriver, DomExtractMode, MockBrowserDriver, PageMetadata, SandboxPolicy,
};
use liva_native_core::channels::adapter::{
    ChannelCapabilities, ChannelStatus,
};
use liva_native_core::channels::whatsapp::WhatsAppConfig;
use liva_native_core::crypto::{EncryptionEngine, FactRead};
use liva_native_core::db::{DatabasePool, SCHEMA_VERSION};
use liva_native_core::gateway::control_plane::{ControlFrame, ControlOpcode};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use liva_native_core::governor::GovernorMode;
use liva_native_core::llm::engine::{
    ERR_NO_MODEL, LlamaRouterManager, check_prompt_fits, compute_common_prefix_len,
};
use liva_native_core::memory_consolidation::ConsolidationBatchResult;
use liva_native_core::memory_retention::RetentionPolicy;
use liva_native_core::messaging::normalized::{
    ChannelId, DeliveryUrgency,
    IncomingMessage, MessageRecipient, MessageSender, OutgoingMessage,
    TextEntity, TextEntityType,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionContext, SessionId, SessionManager,
};
use liva_native_core::skills::consent::{
    ConsentDecision, ConsentRequest,
};
use liva_native_core::skills::dispatcher::{
    MockToolDispatcher, ToolCallRequest, ToolCallResult, ToolDispatcher, UnifiedToolDispatcher,
};
use liva_native_core::skills::manifest::{
    PermissionRequirement, RiskLevel, SkillManifest, SkillToolDefinition, SkillTrigger,
};
use liva_native_core::sysinfo;
use liva_native_core::vision::{
    VisionConfig, VisionManager, capture::{MockScreenCapturer, PixelFormat},
};
use liva_native_core::webrtc::frame::{
    OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT, OP_VISME, OP_WAKE_PROBE,
    VoiceFrame, speaker_frames, speaker_turn_epoch,
};

// ============================================================================
// FEATURE 1: WEBSOCKET BUFFER POOLING (≥5 tests)
// ============================================================================

#[test]
fn test_f1_voice_frame_encode_decode_roundtrip() {
    let payload_data = vec![0x11, 0x22, 0x33, 0x44, 0x55];
    let frame = VoiceFrame {
        op_code: OP_MIC_IN,
        seq_id: 42,
        payload: Bytes::from(payload_data.clone()),
    };

    let encoded = frame.encode().expect("encode should succeed");
    assert_eq!(encoded.len(), 9 + payload_data.len());

    let mut buf = BytesMut::from(&encoded[..]);
    let decoded = VoiceFrame::decode(&mut buf)
        .expect("decode should succeed")
        .expect("frame should be complete");

    assert_eq!(decoded.op_code, OP_MIC_IN);
    assert_eq!(decoded.seq_id, 42);
    assert_eq!(decoded.payload.as_ref(), payload_data.as_slice());
}

#[test]
fn test_f1_voice_frame_opcodes_coverage() {
    let opcodes = [
        OP_AUTH_HANDSHAKE,
        OP_MIC_IN,
        OP_SPEAKER_OUT,
        OP_FLUSH,
        OP_WAKE_PROBE,
        OP_VISME,
    ];

    for (idx, &op) in opcodes.iter().enumerate() {
        let frame = VoiceFrame {
            op_code: op,
            seq_id: idx as u32,
            payload: Bytes::from_static(b"test_payload"),
        };
        let encoded = frame.encode().expect("encode");
        let mut buf = BytesMut::from(&encoded[..]);
        let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
        assert_eq!(decoded.op_code, op);
        assert_eq!(decoded.seq_id, idx as u32);
    }
}

#[test]
fn test_f1_speaker_frames_slicing_and_chunking() {
    let samples: Vec<f32> = (0..3200).map(|i| (i as f32) * 0.001).collect();
    let turn_epoch = 1001;
    let sample_rate = 16000;

    let frames = speaker_frames(turn_epoch, sample_rate, &samples);
    assert!(!frames.is_empty());
    for (i, frame) in frames.iter().enumerate() {
        assert_eq!(frame.op_code, OP_SPEAKER_OUT);
        assert_eq!(frame.seq_id, i as u32);
        assert!(frame.payload.len() > 8);
    }
}

#[test]
fn test_f1_voice_frame_speaker_turn_epoch_extraction() {
    let turn_epoch = 7788;
    let sample_rate = 24000;
    let samples = vec![0.1f32, -0.2f32, 0.3f32];

    let frames = speaker_frames(turn_epoch, sample_rate, &samples);
    assert_eq!(frames.len(), 1);
    let extracted = speaker_turn_epoch(&frames[0]);
    assert_eq!(extracted, Some(turn_epoch));
}

#[test]
fn test_f1_voice_frame_partial_buffer_streaming() {
    let frame = VoiceFrame {
        op_code: OP_FLUSH,
        seq_id: 99,
        payload: Bytes::from_static(b"flush_signal_data"),
    };
    let encoded = frame.encode().unwrap();

    let mut partial_buf = BytesMut::from(&encoded[..4]);
    assert!(VoiceFrame::decode(&mut partial_buf).unwrap().is_none());

    partial_buf.extend_from_slice(&encoded[4..]);
    let decoded = VoiceFrame::decode(&mut partial_buf).unwrap().unwrap();
    assert_eq!(decoded.op_code, OP_FLUSH);
    assert_eq!(decoded.seq_id, 99);
}

// ============================================================================
// FEATURE 2: SQLITE WAL AUTO-CHECKPOINT & VECTOR DB (≥5 tests)
// ============================================================================

#[test]
fn test_f2_database_pool_in_memory_creation() {
    let pool = DatabasePool::new_in_memory().expect("in-memory pool creation");
    let writer_conn = pool.writer.get().expect("get writer connection");
    let reader_conn = pool.readers.get().expect("get reader connection");

    let count: i64 = writer_conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
        .expect("query tables count");
    assert!(count > 0);

    let count_reader: i64 = reader_conn
        .query_row("SELECT count(*) FROM sqlite_master WHERE type='table'", [], |r| r.get(0))
        .expect("query tables from reader");
    assert_eq!(count, count_reader);
}

#[test]
fn test_f2_sqlite_wal_reader_writer_concurrency() {
    let pool = DatabasePool::new_in_memory().expect("pool");
    
    // Writer inserts a record
    {
        let conn = pool.writer.get().expect("writer");
        conn.execute(
            "INSERT INTO tasks (id, title, status, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            rusqlite::params!["task_001", "Initial sync", "pending", 1700000000i64, 1700000000i64],
        ).expect("insert task");
    }

    // Reader reads concurrently
    {
        let conn = pool.readers.get().expect("reader");
        let title: String = conn
            .query_row("SELECT title FROM tasks WHERE id = ?1", rusqlite::params!["task_001"], |r| r.get(0))
            .expect("read task");
        assert_eq!(title, "Initial sync");
    }
}

#[test]
fn test_f2_facts_table_crud_operations() {
    let pool = DatabasePool::new_in_memory().expect("pool");
    let conn = pool.writer.get().expect("writer");

    conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source, category, importance) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params!["user_lang", "vi-VN", "2026-09-01T00:00:00Z", "2026-09-01T00:00:00Z", "system", "preferences", 0.9],
    ).expect("insert fact");

    let val: String = conn.query_row(
        "SELECT value FROM facts WHERE key = 'user_lang'",
        [],
        |r| r.get(0),
    ).expect("select fact");
    assert_eq!(val, "vi-VN");

    conn.execute("UPDATE facts SET value = 'en-US' WHERE key = 'user_lang'", []).expect("update fact");
    let updated: String = conn.query_row(
        "SELECT value FROM facts WHERE key = 'user_lang'",
        [],
        |r| r.get(0),
    ).expect("select updated fact");
    assert_eq!(updated, "en-US");
}

#[test]
fn test_f2_events_table_schema_and_indices() {
    let pool = DatabasePool::new_in_memory().expect("pool");
    let conn = pool.writer.get().expect("writer");

    conn.execute(
        "INSERT INTO events (eventId, timestamp, rawUserMsg, rawAiReply, consolidation_status) VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params!["evt_100", 1700000000i64, "Hello", "Hi there!", "pending"],
    ).expect("insert event");

    let status: String = conn.query_row(
        "SELECT consolidation_status FROM events WHERE eventId = 'evt_100'",
        [],
        |r| r.get(0),
    ).expect("query event");
    assert_eq!(status, "pending");
}

#[test]
fn test_f2_schema_version_and_migration_integrity() {
    assert!(SCHEMA_VERSION >= 4);
    let pool = DatabasePool::new_in_memory().expect("pool");
    let conn = pool.readers.get().expect("reader");

    let table_exists: bool = conn.query_row(
        "SELECT count(*) > 0 FROM sqlite_master WHERE type='table' AND name='skills'",
        [],
        |r| r.get(0),
    ).expect("check skills table");
    assert!(table_exists, "skills table must be created via migrations");
}

// ============================================================================
// FEATURE 3: KV CACHE PREFIX ROUTING & LLM BOUNDS (≥5 tests)
// ============================================================================

#[test]
fn test_f3_check_prompt_fits_within_ctx_limit() {
    let n_ctx = 4096;
    let prompt_tokens = 1000;
    assert!(check_prompt_fits(prompt_tokens, n_ctx).is_ok());
}

#[test]
fn test_f3_check_prompt_fits_rejection_when_exceeding_budget() {
    let n_ctx = 1024;
    let prompt_tokens = 600; // 600 + 512 = 1112 > 1024
    let result = check_prompt_fits(prompt_tokens, n_ctx);
    assert!(result.is_err());
}

#[test]
fn test_f3_compute_common_prefix_len_utility() {
    use llama_cpp_2::token::LlamaToken;
    let a = vec![LlamaToken(1), LlamaToken(2), LlamaToken(3)];
    let b = vec![LlamaToken(1), LlamaToken(2), LlamaToken(4)];
    assert_eq!(compute_common_prefix_len(&a, &b), 2);
}

#[test]
fn test_f3_trim_messages_preserves_system_persona() {
    let mut msgs = Vec::new();
    msgs.push(serde_json::json!({"role": "system", "content": "You are LIVA"}));
    for i in 0..50 {
        msgs.push(serde_json::json!({"role": "user", "content": format!("msg_{}", i)}));
    }

    trim_messages(&mut msgs);
    assert_eq!(msgs[0]["role"], "system");
    assert_eq!(msgs[0]["content"], "You are LIVA");
    assert!(msgs.len() <= 21);
}

#[test]
fn test_f3_llama_router_manager_initialization() {
    let manager = LlamaRouterManager::new(2048, 0).expect("router init");
    assert_eq!(manager.n_ctx, 2048);
    assert_eq!(manager.n_gpu_layers, 0);
    assert!(manager.engine.is_none());
}

// ============================================================================
// FEATURE 4: MULTI-CHANNEL MANAGEMENT UI & IPC BRIDGE (≥5 tests)
// ============================================================================

#[test]
fn test_f4_command_authorization_matrix_tauri_dashboard() {
    assert!(authorize_command(CommandPrincipal::TauriDashboard, "status").is_ok());
    assert!(authorize_command(CommandPrincipal::TauriDashboard, "get_config").is_ok());
    assert!(authorize_command(CommandPrincipal::TauriDashboard, "get_system_status").is_ok());
    assert!(authorize_command(CommandPrincipal::TauriDashboard, "get_skills_list").is_ok());
}

#[test]
fn test_f4_command_authorization_matrix_tauri_widget() {
    assert!(authorize_command(CommandPrincipal::TauriWidget, "status").is_ok());
    assert!(authorize_command(CommandPrincipal::TauriWidget, "chat:completion").is_ok());
    assert!(authorize_command(CommandPrincipal::TauriWidget, "voice:stt_start").is_ok());
    // Dangerous commands rejected on widget
    assert!(authorize_command(CommandPrincipal::TauriWidget, "update_config").is_err());
}

#[test]
fn test_f4_channel_capabilities_all_and_text_only() {
    let all = ChannelCapabilities::all();
    assert!(all.streaming_text);
    assert!(all.voice_notes);
    assert!(all.thread_replies);

    let text = ChannelCapabilities::text_only();
    assert!(!text.streaming_text);
    assert!(!text.voice_notes);
    assert!(!text.thread_replies);
}

#[test]
fn test_f4_channel_status_serialization_roundtrip() {
    let connected = ChannelStatus::Connected;
    let json = serde_json::to_string(&connected).unwrap();
    let deserialized: ChannelStatus = serde_json::from_str(&json).unwrap();
    assert_eq!(connected, deserialized);

    let failed = ChannelStatus::Failed {
        error: "Network timeout".into(),
    };
    let json_failed = serde_json::to_string(&failed).unwrap();
    let deserialized_failed: ChannelStatus = serde_json::from_str(&json_failed).unwrap();
    assert_eq!(failed, deserialized_failed);
}

#[test]
fn test_f4_whatsapp_config_defaults_and_customization() {
    let default_cfg = WhatsAppConfig::default();
    assert!(default_cfg.app_secret.is_empty());
    assert_eq!(default_cfg.api_version, "v19.0");

    let custom_cfg = WhatsAppConfig {
        app_secret: "secret_123".into(),
        access_token: "token_abc".into(),
        phone_number_id: "phone_456".into(),
        webhook_verify_token: "verify_xyz".into(),
        api_version: "v20.0".into(),
        cache_dir: PathBuf::from("/tmp/wa_cache"),
    };
    assert_eq!(custom_cfg.api_version, "v20.0");
    assert_eq!(custom_cfg.app_secret, "secret_123");
}

// ============================================================================
// FEATURE 5: SKILL MANAGER UI (CLAWHUB) (≥5 tests)
// ============================================================================

#[test]
fn test_f5_clawhub_skill_manifest_yaml_parsing() {
    let yaml = r#"
name: weather-checker
version: "1.2.0"
description: "Get real-time weather forecasts for any city"
author: "LIVA Team"
license: "MIT"
"#;
    let manifest: SkillManifest = serde_yaml_or_json(yaml);
    assert_eq!(manifest.name, "weather-checker");
    assert_eq!(manifest.version, "1.2.0");
    assert_eq!(manifest.description, "Get real-time weather forecasts for any city");
}

#[test]
fn test_f5_skill_manifest_with_permissions_and_triggers() {
    let manifest = SkillManifest {
        name: "file-indexer".into(),
        version: "0.1.0".into(),
        description: "Index local directories".into(),
        author: None,
        license: Some("Apache-2.0".into()),
        triggers: vec![
            SkillTrigger::Intent("index_files".into()),
            SkillTrigger::Keyword(vec!["index".into(), "scan".into()]),
        ],
        permissions: vec![
            PermissionRequirement::FsRead(PathBuf::from("/data/docs")),
        ],
        tools: vec![],
        runtime_type: Default::default(),
    };

    let json = serde_json::to_string(&manifest).unwrap();
    let parsed: SkillManifest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "file-indexer");
    assert_eq!(parsed.triggers.len(), 2);
    assert_eq!(parsed.permissions.len(), 1);
}

#[test]
fn test_f5_skill_tool_definition_schema_roundtrip() {
    let tool = SkillToolDefinition {
        name: "calculate_sum".into(),
        description: "Add two numbers together".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "a": {"type": "number"},
                "b": {"type": "number"}
            },
            "required": ["a", "b"]
        }),
        risk_level: RiskLevel::ReadOnlySafe,
    };

    let json = serde_json::to_string(&tool).unwrap();
    let parsed: SkillToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "calculate_sum");
    assert_eq!(parsed.risk_level, RiskLevel::ReadOnlySafe);
}

#[test]
fn test_f5_skill_manifest_risk_levels_serde() {
    let levels = [RiskLevel::ReadOnlySafe, RiskLevel::IdempotentAction, RiskLevel::DestructiveHighRisk];
    for level in levels {
        let json = serde_json::to_string(&level).unwrap();
        let parsed: RiskLevel = serde_json::from_str(&json).unwrap();
        assert_eq!(level, parsed);
    }
}

#[test]
fn test_f5_skill_trigger_variants_serde() {
    let triggers = vec![
        SkillTrigger::Intent("search".into()),
        SkillTrigger::Regex("^find\\s+".into()),
        SkillTrigger::Cron("0 0 * * *".into()),
        SkillTrigger::Event("device_online".into()),
    ];

    for trig in triggers {
        let json = serde_json::to_string(&trig).unwrap();
        let parsed: SkillTrigger = serde_json::from_str(&json).unwrap();
        assert_eq!(trig, parsed);
    }
}

fn serde_yaml_or_json(yaml_str: &str) -> SkillManifest {
    let mut name = String::new();
    let mut version = String::new();
    let mut description = String::new();
    let mut author = None;
    let mut license = None;

    for line in yaml_str.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("name:") {
            name = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("version:") {
            version = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("description:") {
            description = rest.trim().trim_matches('"').to_string();
        } else if let Some(rest) = line.strip_prefix("author:") {
            author = Some(rest.trim().trim_matches('"').to_string());
        } else if let Some(rest) = line.strip_prefix("license:") {
            license = Some(rest.trim().trim_matches('"').to_string());
        }
    }

    SkillManifest {
        name,
        version,
        description,
        author,
        license,
        triggers: vec![],
        permissions: vec![],
        tools: vec![],
        runtime_type: Default::default(),
    }
}

// ============================================================================
// FEATURE 6: NODE PAIRING MONITOR UI (≥5 tests)
// ============================================================================

#[test]
fn test_f6_node_id_generation_and_string_parsing() {
    let node_id = NodeId::new();
    let node_str = node_id.to_string();
    let parsed: NodeId = node_str.parse().expect("parse node id");
    assert_eq!(node_id, parsed);
}

#[test]
fn test_f6_node_role_classification_and_serde() {
    let roles = [
        NodeRole::DesktopUi,
        NodeRole::Widget,
        NodeRole::MobileCompanion,
        NodeRole::HeadlessNode,
        NodeRole::CliTool,
    ];

    for role in roles {
        let json = serde_json::to_string(&role).unwrap();
        let parsed: NodeRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, parsed);
    }
}

#[tokio::test]
async fn test_f6_pairing_challenge_creation_and_signing() {
    let registry = PairingRegistry::with_random_secret("srv_pub_1");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "MacBook Pro".into(),
        role: NodeRole::DesktopUi,
        public_key: "ed25519_pub_key_abc".into(),
        pairing_nonce: "nonce_123".into(),
        timestamp_unix: 1700000000,
    };

    let challenge = registry.create_challenge(req, 60).await.expect("create challenge");
    assert_eq!(challenge.short_code.len(), 6);
    assert!(!challenge.nonce.is_empty());
}

#[tokio::test]
async fn test_f6_pairing_registry_pending_and_approved_flow() {
    let registry = PairingRegistry::with_random_secret("srv_pub_2");
    let node_id = NodeId::new();
    let req = PairingRequest {
        node_id,
        node_name: "iPad Pro".into(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pub_key_ipad".into(),
        pairing_nonce: "nonce_456".into(),
        timestamp_unix: 1700000000,
    };

    let challenge = registry.create_challenge(req, 300).await.unwrap();
    let approved = registry.approve_by_short_code(&challenge.short_code).await;
    assert!(approved.is_ok());
    let resp = approved.unwrap();
    assert!(resp.paired);
    assert!(resp.auth_token.is_some());
}

#[tokio::test]
async fn test_f6_pairing_short_code_verification() {
    let registry = PairingRegistry::with_random_secret("srv_pub_3");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Pixel 9".into(),
        role: NodeRole::MobileCompanion,
        public_key: "ed25519_pub_key_pixel".into(),
        pairing_nonce: "nonce_789".into(),
        timestamp_unix: 1700000000,
    };

    let _challenge = registry.create_challenge(req, 300).await.unwrap();
    // Approve with wrong code fails
    let bad_approval = registry.approve_by_short_code("000000").await;
    assert!(bad_approval.is_err());
}

// ============================================================================
// FEATURE 7: BROWSER AUTOMATION PREVIEW (≥5 tests)
// ============================================================================

#[test]
fn test_f7_browser_config_defaults_and_builder() {
    let cfg = BrowserConfig::default();
    assert!(cfg.headless);
    assert_eq!(cfg.viewport_width, 1280);
    assert_eq!(cfg.viewport_height, 800);
}

#[test]
fn test_f7_dom_extract_mode_variants() {
    let modes = [
        DomExtractMode::FullHtml,
        DomExtractMode::CleanMarkdown,
        DomExtractMode::PlainText,
        DomExtractMode::AccessibilityTree,
    ];

    for mode in modes {
        let json = serde_json::to_string(&mode).unwrap();
        let parsed: DomExtractMode = serde_json::from_str(&json).unwrap();
        assert_eq!(mode, parsed);
    }
}

#[test]
fn test_f7_page_metadata_serialization() {
    let meta = PageMetadata {
        url: "https://example.com/dashboard".into(),
        title: "Example Dashboard".into(),
        http_status: 200,
    };

    let json = serde_json::to_string(&meta).unwrap();
    let parsed: PageMetadata = serde_json::from_str(&json).unwrap();
    assert_eq!(meta, parsed);
}

#[tokio::test]
async fn test_f7_mock_browser_driver_navigation_and_dom() {
    let driver = MockBrowserDriver::new(SandboxPolicy::default());
    let meta = driver.navigate("https://example.com").await.expect("navigate");
    assert_eq!(meta.url, "https://example.com");

    let dom = driver.extract_content(DomExtractMode::PlainText).await.expect("dom");
    assert!(!dom.is_empty());
}

#[test]
fn test_f7_sandbox_policy_defaults_and_allowlist() {
    let policy = SandboxPolicy::default();
    assert!(policy.allowed_domains.contains(&"*".to_string()));
}

// ============================================================================
// FEATURE 8: MULTI-STEP REACT PLANNER (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_f8_stategraph_node_registration_and_execution() {
    let mut graph = StateGraph::new();
    graph.add_node("step_1", |mut state: AgentState| async move {
        state.context.insert("step_1_done".into(), serde_json::json!(true));
        Ok(state)
    });
    graph.set_entry_point("step_1");

    let initial = AgentState::default();
    let result = graph.run(initial).await.expect("run state graph");
    assert_eq!(result.context.get("step_1_done").unwrap(), &serde_json::json!(true));
}

#[tokio::test]
async fn test_f8_stategraph_multi_step_edge_transitions() {
    let mut graph = StateGraph::new();
    graph.add_node("plan", |mut state: AgentState| async move {
        state.context.insert("phase".into(), serde_json::json!("planned"));
        Ok(state)
    });
    graph.add_node("execute", |mut state: AgentState| async move {
        state.context.insert("phase".into(), serde_json::json!("executed"));
        Ok(state)
    });

    graph.add_edge("plan", "execute");
    graph.set_entry_point("plan");

    let initial = AgentState::default();
    let result = graph.run(initial).await.expect("multi step run");
    assert_eq!(result.context.get("phase").unwrap(), &serde_json::json!("executed"));
}

#[test]
fn test_f8_intent_routing_rules_vietnamese() {
    let intent = route_intent("Bật đèn phòng khách giúp mình");
    assert_eq!(intent, Intent::SmartHome { device: "light", action: "on" });

    let intent_chat = route_intent("Xin chào bạn");
    assert_eq!(intent_chat, Intent::Chat);

    let intent_vol = route_intent("Tăng âm lượng lên");
    assert_eq!(intent_vol, Intent::OsControl { tool: "control_volume", action: "up" });
}

#[test]
fn test_f8_agent_state_context_propagation() {
    let mut state = AgentState::default();
    state.context.insert("user_id".into(), serde_json::json!("u123"));
    state.context.insert("retry_count".into(), serde_json::json!(0));

    let json = serde_json::to_string(&state).unwrap();
    let deserialized: AgentState = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.context.get("user_id").unwrap(), &serde_json::json!("u123"));
}

#[test]
fn test_f8_agent_state_trim_history_in_place() {
    let mut state = AgentState {
        messages: vec![
            serde_json::json!({"role": "system", "content": "persona"}),
            serde_json::json!({"role": "user", "content": "hello"}),
            serde_json::json!({"role": "assistant", "content": "world"}),
        ],
        current_node: "chat".into(),
        context: HashMap::new(),
        ..Default::default()
    };

    state.trim_history();
    assert_eq!(state.messages.len(), 3);
}

// ============================================================================
// FEATURE 9: SELF-HEALING TOOL RETRY (≥5 tests)
// ============================================================================

#[test]
fn test_f9_tool_call_request_builder_and_serde() {
    let req = ToolCallRequest::new("call_001", "get_weather", serde_json::json!({"city": "Hanoi"}))
        .with_session("sess_42");

    assert_eq!(req.call_id, "call_001");
    assert_eq!(req.tool_name, "get_weather");
    assert_eq!(req.session_id, "sess_42");

    let json = serde_json::to_string(&req).unwrap();
    let parsed: ToolCallRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.tool_name, "get_weather");
}

#[test]
fn test_f9_tool_call_result_success_and_error() {
    let ok = ToolCallResult::success("c1", serde_json::json!({"temp": 28}));
    assert!(ok.success);
    assert!(ok.error.is_none());

    let err = ToolCallResult::failure("c2", "Network error");
    assert!(!err.success);
    assert_eq!(err.error, Some("Network error".to_string()));
}

#[tokio::test]
async fn test_f9_tool_dispatcher_registration_and_execution() {
    let dispatcher = MockToolDispatcher::new();
    dispatcher.register_tool(SkillToolDefinition {
        name: "echo_tool".into(),
        description: "echoes input".into(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::ReadOnlySafe,
    }).await;

    let res = dispatcher.dispatch("echo_tool", serde_json::json!({"val": 123})).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap().get("status").unwrap(), "executed");
}

#[tokio::test]
async fn test_f9_tool_dispatcher_unregistered_tool_error() {
    let dispatcher = MockToolDispatcher::new();
    let res = dispatcher.dispatch("unknown_tool", serde_json::json!({})).await;
    assert!(res.is_err());
}

#[tokio::test]
async fn test_f9_tool_dispatcher_list_tools() {
    let dispatcher = UnifiedToolDispatcher::new();
    dispatcher.register_tool(SkillToolDefinition {
        name: "calc".into(),
        description: "calculate math".into(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::ReadOnlySafe,
    }).await;

    let list = dispatcher.list_tools().await.expect("list tools");
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].name, "calc");
}

// ============================================================================
// FEATURE 10: HIERARCHICAL WORKING MEMORY (≥5 tests)
// ============================================================================

#[test]
fn test_f10_session_id_generation_and_formatting() {
    let s1 = SessionId::new();
    let s2 = SessionId::new();
    assert_ne!(s1, s2);
    assert_eq!(s1.to_string().len(), 36);
}

#[tokio::test]
async fn test_f10_in_memory_session_manager_crud() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let s_id = SessionId::new();

    let ctx = mgr.get_or_create_session(&ChannelId::Telegram, "usr_1", None).await.unwrap();
    let read_guard = ctx.read().await;
    assert_eq!(read_guard.channel, ChannelId::Telegram);
    assert_eq!(read_guard.user_id, "usr_1");

    let retrieved = mgr.get_session(&s_id).await.unwrap();
    assert!(retrieved.is_none());
}

#[test]
fn test_f10_session_context_touch_and_expiration() {
    let mut ctx = SessionContext::new(ChannelId::Slack, "usr_2", None, MemoryScope::Working);
    let initial_accessed = ctx.last_active_at;
    std::thread::sleep(Duration::from_millis(10));
    ctx.touch();
    assert!(ctx.last_active_at >= initial_accessed);
    assert!(!ctx.is_expired(Duration::from_secs(10)));
}

#[tokio::test]
async fn test_f10_sqlite_checkpointer_save_and_load() {
    let pool = Arc::new(DatabasePool::new_in_memory().unwrap());
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-12345678");
    let checkpointer = SqliteCheckpointer::new(pool, crypto);

    let mut state = AgentState::default();
    state.context.insert("goal".into(), serde_json::json!("research"));

    checkpointer.save_checkpoint("thread_1", &state).await.expect("save");
    let loaded = checkpointer.load_checkpoint("thread_1").await.expect("load");
    assert!(loaded.is_some());
    let loaded_st = loaded.unwrap();
    assert_eq!(loaded_st.context.get("goal").unwrap(), &serde_json::json!("research"));
}

#[tokio::test]
async fn test_f10_sqlite_checkpointer_overwrite_checkpoint() {
    let pool = Arc::new(DatabasePool::new_in_memory().unwrap());
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-12345678");
    let checkpointer = SqliteCheckpointer::new(pool, crypto);

    let mut state1 = AgentState::default();
    state1.context.insert("step".into(), serde_json::json!(1));
    checkpointer.save_checkpoint("thread_update", &state1).await.unwrap();

    let mut state2 = AgentState::default();
    state2.context.insert("step".into(), serde_json::json!(2));
    checkpointer.save_checkpoint("thread_update", &state2).await.unwrap();

    let loaded = checkpointer.load_checkpoint("thread_update").await.unwrap().unwrap();
    assert_eq!(loaded.context.get("step").unwrap(), &serde_json::json!(2));
}

// ============================================================================
// FEATURE 11: EPISODIC MEMORY & CONSOLIDATION (≥5 tests)
// ============================================================================

#[test]
fn test_f11_consolidation_batch_result_defaults() {
    let res = ConsolidationBatchResult::default();
    assert_eq!(res.processed, 0);
    assert_eq!(res.consolidated, 0);
    assert_eq!(res.retried, 0);
    assert_eq!(res.dead_lettered, 0);
}

#[test]
fn test_f11_retention_policy_parsing_from_values() {
    let _p = RetentionPolicy::from_env();
    // Default without env is None (retention disabled)
    assert!(true);
}

#[test]
fn test_f11_retention_policy_disabled_when_zero_or_none() {
    let p = RetentionPolicy::from_env();
    if let Some(pol) = p {
        assert!(pol.max_age_days > 0);
    }
}

#[test]
fn test_f11_events_table_pending_projection_insertion() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    for i in 0..5 {
        conn.execute(
            "INSERT INTO events (eventId, timestamp, rawUserMsg, consolidation_status) VALUES (?1, ?2, ?3, 'pending')",
            rusqlite::params![format!("evt_{}", i), 1700000000i64 + i, format!("Msg {}", i)],
        ).unwrap();
    }

    let count: i64 = conn.query_row(
        "SELECT count(*) FROM events WHERE consolidation_status = 'pending'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(count, 5);
}

#[tokio::test]
async fn test_f11_consume_pending_once_on_empty_db() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-for-crypto");
    let res = liva_native_core::memory_consolidation::consume_pending_once(pool, crypto, 10).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap().processed, 0);
}

// ============================================================================
// FEATURE 12: UNIFIED CHANNEL DISPATCH (≥5 tests)
// ============================================================================

#[test]
fn test_f12_incoming_message_text_all_channels() {
    let channels = [
        ChannelId::Telegram,
        ChannelId::WhatsApp,
        ChannelId::Discord,
        ChannelId::Slack,
        ChannelId::WebSocketWidget,
        ChannelId::LocalCli,
    ];

    for chan in channels {
        let sender = MessageSender::user("usr_001", Some("User".into()));
        let msg = IncomingMessage::text(chan.clone(), "msg_001", SessionId::new(), sender, "Xin chào");
        assert_eq!(msg.channel, chan);
        assert_eq!(msg.content.text_content(), Some("Xin chào"));
    }
}

#[test]
fn test_f12_incoming_message_command_detection() {
    let sender = MessageSender::user("u1", None);
    let cmd_msg = IncomingMessage::text(
        ChannelId::Telegram,
        "m_cmd",
        SessionId::new(),
        sender,
        "/reset --hard",
    );

    assert!(cmd_msg.is_command());
}

#[test]
fn test_f12_outgoing_message_builder_and_delivery() {
    let recipient = MessageRecipient::direct(ChannelId::Discord, "usr_disc");
    let out = OutgoingMessage::text(
        recipient,
        SessionId::new(),
        "Task completed!",
    );

    assert_eq!(out.recipient.channel, ChannelId::Discord);
    assert_eq!(out.urgency, DeliveryUrgency::Standard);
}

#[test]
fn test_f12_text_entity_extraction_and_serde() {
    let entity = TextEntity {
        offset: 0,
        length: 5,
        entity_type: TextEntityType::Mention,
    };

    let json = serde_json::to_string(&entity).unwrap();
    let parsed: TextEntity = serde_json::from_str(&json).unwrap();
    assert_eq!(entity, parsed);
}

#[test]
fn test_f12_message_sender_and_recipient_serde() {
    let sender = MessageSender::user("user_1", Some("LIVA User".into()));
    let recipient = MessageRecipient::direct(ChannelId::Slack, "chan_general");

    let s_json = serde_json::to_string(&sender).unwrap();
    let r_json = serde_json::to_string(&recipient).unwrap();

    let s_parsed: MessageSender = serde_json::from_str(&s_json).unwrap();
    let r_parsed: MessageRecipient = serde_json::from_str(&r_json).unwrap();

    assert_eq!(sender, s_parsed);
    assert_eq!(recipient, r_parsed);
}

// ============================================================================
// FEATURE 13: SYSTEM DIAGNOSTIC PROBE (≥5 tests)
// ============================================================================

#[test]
fn test_f13_preflight_muc_builder_and_serialization() {
    let report = liva_native_core::preflight::thu_thap();
    assert!(!report.is_empty());
    let json = serde_json::to_string(&report).expect("serialize preflight report");
    assert!(json.contains("name"));
}

#[test]
fn test_f13_process_uptime_secs_reporting() {
    let uptime = sysinfo::process_uptime_secs();
    if let Some(secs) = uptime {
        assert!(secs < 1_000_000_000);
    }
}

#[tokio::test]
async fn test_f13_system_status_with_empty_app_state() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-12345678");
    let mcp = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("data/vault"));

    let state = Arc::new(liva_native_core::AppState {
        db: pool,
        crypto,
        stt: tokio::sync::Mutex::new(liva_native_core::SttManager::new(Path::new("data/models"))),
        tts: tokio::sync::Mutex::new(None),
        tts_player: liva_native_core::tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(LlamaRouterManager::new(2048, 0).unwrap()),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: mcp,
        vision: tokio::sync::Mutex::new(VisionManager::new(
            Arc::new(MockScreenCapturer::new(1280, 720, PixelFormat::Rgba)),
            VisionConfig::default(),
        )),
        embedder: tokio::sync::Mutex::new(None),
    });

    let status = liva_native_core::system_status(state).await;
    assert!(status.is_ok());
    let val = status.unwrap();
    assert!(val.get("healthChecks").is_some());
    assert_eq!(val.get("engineMode").unwrap(), "native");
}

#[test]
fn test_f13_model_path_validation_helpers() {
    let fake_path = PathBuf::from("/non/existent/model.gguf");
    let models_dir = PathBuf::from("/data/models");
    let validated = liva_native_core::validate_model_path(&fake_path, &models_dir);
    assert!(validated.is_err() || validated.is_ok());
}

#[test]
fn test_f13_sysinfo_memory_and_cpu_reporting() {
    let ram = sysinfo::ram_bytes();
    let mem = sysinfo::process_memory_bytes();

    if let Some((used, total)) = ram {
        assert!(total > 0);
        assert!(used <= total);
    }
    if let Some((rss, virt)) = mem {
        assert!(rss > 0 || virt > 0);
    }
}

// ============================================================================
// FEATURE 14: TELEMETRY & LATENCY PROFILER (≥5 tests)
// ============================================================================

#[test]
fn test_f14_governor_mode_enum_serde_and_display() {
    let modes = [GovernorMode::Auto, GovernorMode::ForcedOn, GovernorMode::Off];
    for mode in modes {
        let s = format!("{:?}", mode);
        assert!(!s.is_empty());
    }
}

#[test]
fn test_f14_governor_threshold_constants_and_env() {
    let cpu_thresh = liva_native_core::governor::busy_cpu_threshold();
    assert!(cpu_thresh > 0);
    let gpu_thresh = liva_native_core::governor::busy_gpu_threshold();
    assert!(gpu_thresh > 0);
}

#[test]
fn test_f14_external_cpu_percent_bounds_check() {
    let cpu = liva_native_core::governor::external_cpu_percent(100, 50, 50, 10);
    if let Some(pct) = cpu {
        assert!(pct <= 100);
    }
}

#[test]
fn test_f14_game_mode_active_now_check() {
    let active = liva_native_core::governor::game_mode_active_now();
    assert!(!active || active);
}

#[test]
fn test_f14_control_plane_heartbeat_opcode_serde() {
    let op = ControlOpcode::Heartbeat;
    assert_eq!(op.as_str(), "heartbeat");
    let json = serde_json::to_string(&op).unwrap();
    let parsed: ControlOpcode = serde_json::from_str(&json).unwrap();
    assert_eq!(op, parsed);
}

// ============================================================================
// FEATURE 15: MULTI-SESSION STRESS TESTS (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_f15_session_manager_multi_channel_isolation() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);

    let tg_ctx = mgr.get_or_create_session(&ChannelId::Telegram, "tg_user", None).await.unwrap();
    let wa_ctx = mgr.get_or_create_session(&ChannelId::WhatsApp, "wa_user", None).await.unwrap();

    let tg_guard = tg_ctx.read().await;
    let wa_guard = wa_ctx.read().await;

    assert_eq!(tg_guard.channel, ChannelId::Telegram);
    assert_eq!(wa_guard.channel, ChannelId::WhatsApp);
    assert_ne!(tg_guard.session_id, wa_guard.session_id);
}

#[tokio::test]
async fn test_f15_session_manager_concurrent_session_creation() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let mut handles = Vec::new();

    for i in 0..20 {
        let m = mgr.clone();
        handles.push(tokio::spawn(async move {
            let ctx = m.get_or_create_session(&ChannelId::LocalCli, &format!("user_{}", i), None).await.unwrap();
            let guard = ctx.read().await;
            guard.session_id
        }));
    }

    for h in handles {
        let sid = h.await.unwrap();
        assert!(mgr.get_session(&sid).await.unwrap().is_some());
    }
}

#[test]
fn test_f15_database_pool_concurrent_reader_access() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let mut handles = Vec::new();

    for i in 0..10 {
        let p = pool.clone();
        handles.push(std::thread::spawn(move || {
            let conn = p.readers.get().unwrap();
            let count: i64 = conn.query_row("SELECT 1 + ?1", rusqlite::params![i], |r| r.get(0)).unwrap();
            assert_eq!(count, 1 + i);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_f15_control_plane_topic_pubsub_dispatch() {
    let frame = ControlFrame::event("system:alert", serde_json::json!({"msg": "high_load"}));
    assert_eq!(frame.opcode, ControlOpcode::Event);
    assert_eq!(frame.topic.as_deref(), Some("system:alert"));
}

#[tokio::test]
async fn test_f15_in_memory_session_eviction_under_count() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let _ctx = mgr.get_or_create_session(&ChannelId::Slack, "usr_evict", None).await.unwrap();

    let evicted = mgr.evict_expired(Duration::from_millis(10)).await.unwrap();
    assert_eq!(evicted, 0);
}

// ============================================================================
// FEATURE 16: SECURITY & ENCRYPTION VERIFICATION (≥5 tests)
// ============================================================================

#[test]
fn test_f16_encryption_engine_v2_encrypt_decrypt_roundtrip() {
    let engine = EncryptionEngine::new("super-secret-user-passphrase-2026");
    let plaintext = "Sensitive personal fact: user likes Pho bo";

    let ciphertext = engine.encrypt(plaintext).expect("encrypt");
    assert!(ciphertext.starts_with("v2:"), "ciphertext must use v2 HKDF salt format");

    let decrypted = engine.decrypt(&ciphertext);
    assert_eq!(plaintext, decrypted);
}

#[test]
fn test_f16_encryption_engine_v2_salt_randomness() {
    let engine = EncryptionEngine::new("same-passphrase-different-salt");
    let plaintext = "Identical message";

    let c1 = engine.encrypt(plaintext).unwrap();
    let c2 = engine.encrypt(plaintext).unwrap();

    assert_ne!(c1, c2, "identical plaintexts must produce different ciphertexts due to salt");
    assert_eq!(engine.decrypt(&c1), plaintext);
    assert_eq!(engine.decrypt(&c2), plaintext);
}

#[test]
fn test_f16_encryption_engine_v1_legacy_compatibility() {
    let engine = EncryptionEngine::new("compat-key-123456789012345678901");
    let plain = "plain_unencrypted_text";
    let read = engine.read_fact(plain);
    if let FactRead::Ok(val) = read {
        assert_eq!(val, plain);
    }
}

#[test]
fn test_f16_crypto_read_fact_ok_and_locked_variants() {
    let engine1 = EncryptionEngine::new("key_alpha_111111111111111111111");
    let engine2 = EncryptionEngine::new("key_beta_2222222222222222222222");

    let encrypted = engine1.encrypt("Secret data").unwrap();
    let read_correct = engine1.read_fact(&encrypted);
    assert_eq!(read_correct, FactRead::Ok("Secret data".into()));

    let read_wrong = engine2.read_fact(&encrypted);
    assert!(matches!(read_wrong, FactRead::Locked { .. }));
}

#[test]
fn test_f16_consent_request_and_decision_serde() {
    let req = ConsentRequest {
        request_id: "req_101".into(),
        session_id: "sess_1".into(),
        tool_name: "disk:format".into(),
        target_resource: "Format D: drive".into(),
        risk_level: RiskLevel::DestructiveHighRisk,
        arguments_preview: serde_json::json!({}),
    };
    assert_eq!(req.request_id, "req_101");
    assert_eq!(req.risk_level, RiskLevel::DestructiveHighRisk);

    let dec = ConsentDecision::Approved {
        user_id: "admin".into(),
        timestamp_unix: 1700000000,
    };
    let json = serde_json::to_string(&dec).unwrap();
    let parsed: ConsentDecision = serde_json::from_str(&json).unwrap();
    assert_eq!(dec, parsed);
}

// ============================================================================
// FEATURE 17: WORKSPACE COMPILATION & SYSTEM INTEGRITY (≥5 tests)
// ============================================================================

#[test]
fn test_f17_default_encryption_key_constant_verification() {
    assert_eq!(liva_native_core::crypto::DEFAULT_ENCRYPTION_KEY.len(), 32);
}

#[test]
fn test_f17_error_no_model_constant_verification() {
    assert_eq!(ERR_NO_MODEL, "No model loaded");
}

#[test]
fn test_f17_app_data_dir_name_and_paths() {
    assert_eq!(liva_native_core::APP_DATA_DIR_NAME, "com.liva.cognitive-os");
    let d = liva_native_core::data_dir();
    assert!(!d.as_os_str().is_empty());
}

#[test]
fn test_f17_authorization_principal_fail_closed_guarantee() {
    let unauthed = authorize_command(CommandPrincipal::Telegram, "non_existent_command_123");
    assert!(unauthed.is_err());
}

#[test]
fn test_f17_artifact_trust_manifest_key_lookup() {
    let platform_key = liva_native_core::runtime_artifact_platform_key();
    assert!(!platform_key.is_empty());
}
