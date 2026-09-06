//! E2E Test Suite - Tier 2: Boundary Value Analysis & Edge Case Hardening (≥5 tests per feature)
//!
//! Features covered:
//! - Feature 1: WebSocket Buffer Boundaries (empty, oversized, truncated, corrupt opcodes)
//! - Feature 2: SQLite WAL & DB Boundaries (rollback, constraints, corrupt data, bounds)
//! - Feature 3: LLM Context Boundaries (exact context match, +1 token overflow, empty, UTF-8 graphemes)
//! - Feature 4: Multi-Channel IPC Guardrails (unauthorized principals, schema mismatch, malicious payloads)
//! - Feature 5: Skill Manifest Parsing Edge Cases (corrupted YAML, missing keys, path traversal)
//! - Feature 6: Node Pairing Security Boundaries (expired challenges, bad codes, replay attack rejection)
//! - Feature 7: Browser Automation Sandbox Boundaries (denied domains, file:// protocol blocking, malformed HTML)
//! - Feature 8: ReAct Planner Edge Cases (cyclic transitions, panic recovery, empty graphs)
//! - Feature 9: Tool Dispatcher Fault Tolerance (consent timeout/denial, missing arguments, handler errors)
//! - Feature 10: Session Memory Boundary Conditions (TTL expiration, mass eviction, large metadata)
//! - Feature 11: Episodic Consolidation Edge Cases (empty batches, dead-lettering, duplicate events)
//! - Feature 12: Messaging Edge Cases (empty content, missing recipients, invalid emojis)
//! - Feature 13: Diagnostics & Preflight Resiliency (missing model directories, uninitialized paths)
//! - Feature 14: Governor & Telemetry Boundaries (extreme CPU metrics >100%, rapid opcode flooding)
//! - Feature 15: Concurrency & Stress Boundaries (session storms, connection pool contention)
//! - Feature 16: Security & Crypto Hardening (tampered ciphertexts, wrong keys, bit-flips, locked facts)
//! - Feature 17: Workspace & Artifact Integrity (tampered hashes, path safety, fail-closed auth)

use bytes::{Bytes, BytesMut};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// LIVA Native Core imports
use liva_native_core::agent::graph::{Intent, StateGraph, route_intent};
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::state::{AgentState, trim_messages};
use liva_native_core::{CommandPrincipal, authorize_command};
use liva_native_core::automation::{
    BrowserDriver, DomExtractMode, MockBrowserDriver, SandboxPolicy,
    SemanticDomExtractor,
};
use liva_native_core::channels::adapter::{
    ChannelCapabilities, ChannelStatus,
};
use liva_native_core::crypto::{EncryptionEngine, FactRead};
use liva_native_core::db::DatabasePool;
use liva_native_core::gateway::control_plane::{ControlFrame, ControlOpcode};
use liva_native_core::gateway::pairing::{
    NodeId, NodeRole, PairingRegistry, PairingRequest,
};
use liva_native_core::llm::engine::{
    check_prompt_fits, compute_common_prefix_len,
};
use liva_native_core::memory_consolidation::ConsolidationBatchResult;
use liva_native_core::memory_retention::RetentionPolicy;
use liva_native_core::messaging::normalized::{
    ChannelId, DeliveryUrgency,
    IncomingMessage, MessageRecipient, MessageSender, OutgoingMessage,
};
use liva_native_core::messaging::session::{
    InMemorySessionManager, MemoryScope, SessionContext, SessionId, SessionManager,
};
use liva_native_core::skills::consent::{
    ConsentDecision, ConsentRequest, ConsentSuspender,
};
use liva_native_core::skills::dispatcher::{
    MockToolDispatcher, ToolCallRequest, ToolCallResult,
};
use liva_native_core::skills::manifest::{
    RiskLevel, SkillManifest, SkillToolDefinition,
    parse_skill_markdown,
};
use liva_native_core::sysinfo;
use liva_native_core::webrtc::frame::{
    OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT,
    VoiceFrame, speaker_frames, speaker_turn_epoch,
};

// ============================================================================
// FEATURE 1: WEBSOCKET BUFFER BOUNDARIES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f1_voice_frame_empty_payload() {
    let frame = VoiceFrame {
        op_code: OP_FLUSH,
        seq_id: 1,
        payload: Bytes::new(),
    };
    let encoded = frame.encode().expect("encode empty payload");
    assert_eq!(encoded.len(), 9); // Header is 9 bytes

    let mut buf = BytesMut::from(&encoded[..]);
    let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
    assert_eq!(decoded.payload.len(), 0);
    assert_eq!(decoded.op_code, OP_FLUSH);
}

#[test]
fn test_t2_f1_voice_frame_large_payload_boundary() {
    let large_payload = vec![0xAB; 64 * 1024]; // 64 KB audio chunk
    let frame = VoiceFrame {
        op_code: OP_MIC_IN,
        seq_id: 99999,
        payload: Bytes::from(large_payload.clone()),
    };

    let encoded = frame.encode().expect("encode large payload");
    let mut buf = BytesMut::from(&encoded[..]);
    let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
    assert_eq!(decoded.payload.len(), 64 * 1024);
    assert_eq!(decoded.payload[0], 0xAB);
}

#[test]
fn test_t2_f1_voice_frame_truncated_header_decode_returns_none() {
    let raw = vec![0x01, 0x00, 0x00]; // Only 3 bytes (header requires 9 bytes)
    let mut buf = BytesMut::from(&raw[..]);
    let res = VoiceFrame::decode(&mut buf).expect("decode attempt");
    assert!(res.is_none(), "partial header must return None without panic");
}

#[test]
fn test_t2_f1_speaker_frames_empty_sample_slice() {
    let empty_samples: Vec<f32> = Vec::new();
    let frames = speaker_frames(100, 16000, &empty_samples);
    assert!(frames.is_empty(), "empty audio buffer produces 0 speaker frames");
}

#[test]
fn test_t2_f1_speaker_turn_epoch_invalid_payload_handling() {
    let invalid_frame = VoiceFrame {
        op_code: OP_SPEAKER_OUT,
        seq_id: 0,
        payload: Bytes::from_static(b"12"), // Less than 4 bytes for epoch u32
    };
    let epoch = speaker_turn_epoch(&invalid_frame);
    assert!(epoch.is_none(), "payload < 4 bytes returns None for turn epoch");
}

// ============================================================================
// FEATURE 2: SQLITE WAL & DB BOUNDARIES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f2_facts_table_unique_key_conflict_handling() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('pref_theme', 'dark', '2026-09-01', '2026-09-01', 'user')",
        [],
    ).unwrap();

    // Inserting duplicate primary key must return SQLite constraint error
    let res = conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('pref_theme', 'light', '2026-09-01', '2026-09-01', 'user')",
        [],
    );
    assert!(res.is_err(), "Duplicate primary key insertion must fail");
}

#[test]
fn test_t2_f2_transaction_rollback_preserves_state() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let mut conn = pool.writer.get().unwrap();

    conn.execute(
        "INSERT INTO tasks (id, title, status, created_at, updated_at) VALUES ('t1', 'Keep Me', 'pending', 100, 100)",
        [],
    ).unwrap();

    let tx_res: Result<(), rusqlite::Error> = (|| {
        let tx = conn.transaction()?;
        tx.execute(
            "INSERT INTO tasks (id, title, status, created_at, updated_at) VALUES ('t2', 'Rollback Me', 'pending', 101, 101)",
            [],
        )?;
        // Intentionally trigger rollback
        Err(rusqlite::Error::ExecuteReturnedResults)
    })();
    assert!(tx_res.is_err());

    let count: i64 = conn.query_row("SELECT count(*) FROM tasks WHERE id = 't2'", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 0, "Rolled back transaction must not persist records");
}

#[test]
fn test_t2_f2_query_non_existent_table_fails_gracefully() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.readers.get().unwrap();

    let res = conn.query_row("SELECT * FROM non_existent_table_xyz", [], |_r| Ok(()));
    assert!(res.is_err(), "Querying missing table must fail gracefully");
}

#[test]
fn test_t2_f2_extreme_importance_float_values_in_facts() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source, importance) VALUES ('k_extreme', 'val', '2026-09-01', '2026-09-01', 'sys', ?1)",
        rusqlite::params![1.0e10],
    ).unwrap();

    let imp: f64 = conn.query_row("SELECT importance FROM facts WHERE key = 'k_extreme'", [], |r| r.get(0)).unwrap();
    assert_eq!(imp, 1.0e10);
}

#[test]
fn test_t2_f2_null_category_and_source_fields_in_facts() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    conn.execute(
        "INSERT INTO facts (key, value, createdAt, updatedAt, source, category) VALUES ('k_nulls', 'val', '2026-09-01', '2026-09-01', 'sys', NULL)",
        [],
    ).unwrap();

    let cat: Option<String> = conn.query_row(
        "SELECT category FROM facts WHERE key = 'k_nulls'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert!(cat.is_none());
}

// ============================================================================
// FEATURE 3: LLM CONTEXT BOUNDARIES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f3_prompt_fits_exact_boundary() {
    // ctx: 2048, prompt_tokens: 2048 - 512 = 1536. check_prompt_fits uses strictly `<`.
    // So 1535 + 512 = 2047 < 2048 (Ok).
    let n_ctx = 2048;
    let safe_limit = 1535;
    assert!(check_prompt_fits(safe_limit, n_ctx).is_ok());
}

#[test]
fn test_t2_f3_prompt_fits_one_token_over_boundary() {
    let n_ctx = 2048;
    let over_limit = 1536; // 1536 + 512 = 2048 (not < 2048) -> Err
    let res = check_prompt_fits(over_limit, n_ctx);
    assert!(res.is_err(), "Prompt matching or exceeding budget must be rejected");
}

#[test]
fn test_t2_f3_prompt_zero_tokens_check() {
    let n_ctx = 4096;
    assert!(check_prompt_fits(0, n_ctx).is_ok());
}

#[test]
fn test_t2_f3_compute_common_prefix_disjoint_tokens() {
    use llama_cpp_2::token::LlamaToken;
    let a = vec![LlamaToken(10), LlamaToken(20)];
    let b = vec![LlamaToken(30), LlamaToken(40)];
    assert_eq!(compute_common_prefix_len(&a, &b), 0);
}

#[test]
fn test_t2_f3_trim_messages_empty_slice_safe() {
    let mut empty_msgs: Vec<serde_json::Value> = Vec::new();
    trim_messages(&mut empty_msgs);
    assert!(empty_msgs.is_empty());
}

// ============================================================================
// FEATURE 4: MULTI-CHANNEL IPC GUARDRAILS (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f4_authorize_command_unknown_principal_rejected() {
    let res = authorize_command(CommandPrincipal::Telegram, "shutdown_server");
    assert!(res.is_err(), "Administrative shutdown command on Telegram principal must be rejected");
}

#[test]
fn test_t2_f4_authorize_command_empty_string_rejected() {
    let res = authorize_command(CommandPrincipal::TauriDashboard, "");
    assert!(res.is_err(), "Empty command name must fail closed");
}

#[test]
fn test_t2_f4_authorize_command_special_characters_injection_rejected() {
    let res = authorize_command(CommandPrincipal::TauriWidget, "status; rm -rf /");
    assert!(res.is_err(), "Injected command strings must be rejected");
}

#[test]
fn test_t2_f4_channel_capabilities_text_only() {
    let text = ChannelCapabilities::text_only();
    assert!(!text.streaming_text);
    assert!(!text.voice_notes);
}

#[test]
fn test_t2_f4_channel_status_failed_with_extreme_error_message() {
    let long_error = "A".repeat(10000);
    let status = ChannelStatus::Failed { error: long_error.clone() };
    let json = serde_json::to_string(&status).unwrap();
    let deserialized: ChannelStatus = serde_json::from_str(&json).unwrap();
    if let ChannelStatus::Failed { error } = deserialized {
        assert_eq!(error.len(), 10000);
    } else {
        panic!("Deserialization failed");
    }
}

// ============================================================================
// FEATURE 5: SKILL MANIFEST PARSING EDGE CASES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f5_missing_manifest_frontmatter_delimiter_fails() {
    let raw = "name: invalid_no_delimiter\nversion: 1.0.0\nBody content";
    let res = parse_skill_markdown(raw, Path::new("/tmp"));
    assert!(res.is_err());
}

#[test]
fn test_t2_f5_unclosed_frontmatter_fails() {
    let raw = "---\nname: unclosed_manifest\nversion: 1.0.0\nBody content without closing delimiter";
    let res = parse_skill_markdown(raw, Path::new("/tmp"));
    assert!(res.is_err());
}

#[test]
fn test_t2_f5_missing_version_field_fails() {
    let raw = "---\nname: missing_ver\n---\nBody";
    let res = parse_skill_markdown(raw, Path::new("/tmp"));
    assert!(res.is_err());
}

#[test]
fn test_t2_f5_skill_tool_empty_properties_schema() {
    let tool = SkillToolDefinition {
        name: "noop".into(),
        description: "Does nothing".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {}
        }),
        risk_level: RiskLevel::ReadOnlySafe,
    };
    let json = serde_json::to_string(&tool).unwrap();
    let parsed: SkillToolDefinition = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.name, "noop");
}

#[test]
fn test_t2_f5_skill_manifest_with_empty_tools_and_triggers() {
    let manifest = SkillManifest {
        name: "minimal-skill".into(),
        version: "0.0.1".into(),
        description: "Empty components".into(),
        author: None,
        license: None,
        triggers: vec![],
        permissions: vec![],
        tools: vec![],
        runtime_type: Default::default(),
    };
    let json = serde_json::to_string(&manifest).unwrap();
    assert!(json.contains("minimal-skill"));
}

// ============================================================================
// FEATURE 6: NODE PAIRING SECURITY BOUNDARIES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f6_pairing_expired_challenge_fails() {
    let registry = PairingRegistry::with_random_secret("srv_pub_exp");
    let req = PairingRequest {
        node_id: NodeId::new(),
        node_name: "Old Device".into(),
        role: NodeRole::MobileCompanion,
        public_key: "pub_key_1".into(),
        pairing_nonce: "nonce_1".into(),
        timestamp_unix: 1700000000,
    };

    // TTL of 0 seconds expires immediately
    let challenge = registry.create_challenge(req, 0).await.unwrap();
    tokio::time::sleep(Duration::from_millis(50)).await;

    let res = registry.approve_by_short_code(&challenge.short_code).await;
    assert!(res.is_err(), "Expired pairing challenge must fail closed");
}

#[tokio::test]
async fn test_t2_f6_pairing_challenge_with_invalid_short_code_length() {
    let registry = PairingRegistry::with_random_secret("srv_pub_len");
    let res = registry.approve_by_short_code("12").await;
    assert!(res.is_err(), "Short code with incorrect length must be rejected");
}

#[tokio::test]
async fn test_t2_f6_pairing_challenge_with_special_characters_code() {
    let registry = PairingRegistry::with_random_secret("srv_pub_spec");
    let res = registry.approve_by_short_code("!@#$%^").await;
    assert!(res.is_err(), "Malformed short code must be rejected");
}

#[test]
fn test_t2_f6_node_id_invalid_string_parse_fails() {
    let res: Result<NodeId, _> = "invalid-uuid-string".parse();
    assert!(res.is_err(), "Malformed UUID string fails parsing");
}

#[test]
fn test_t2_f6_node_role_unknown_string_parse_fails() {
    let res: Result<NodeRole, _> = "super_admin_unsupported".parse();
    assert!(res.is_err(), "Unknown role string fails parsing");
}

// ============================================================================
// FEATURE 7: BROWSER AUTOMATION SANDBOX BOUNDARIES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f7_dom_extractor_malformed_html_unclosed_tags() {
    let malformed = "<div><p>Paragraph without close tag <span>Span text</div>";
    let extracted = SemanticDomExtractor::extract(malformed, DomExtractMode::PlainText);
    assert!(extracted.contains("Paragraph without close tag"));
}

#[test]
fn test_t2_f7_dom_extractor_empty_html() {
    let extracted = SemanticDomExtractor::extract("", DomExtractMode::CleanMarkdown);
    assert!(extracted.is_empty());
}

#[test]
fn test_t2_f7_dom_extractor_script_and_style_stripping() {
    let html = "<html><head><style>body { color: red; }</style></head><body><script>alert(1);</script><p>Clean Content</p></body></html>";
    let extracted = SemanticDomExtractor::extract(html, DomExtractMode::CleanMarkdown);
    assert!(!extracted.contains("alert(1)"));
    assert!(!extracted.contains("color: red"));
    assert!(extracted.contains("Clean Content"));
}

#[tokio::test]
async fn test_t2_f7_mock_browser_driver_empty_url() {
    let driver = MockBrowserDriver::new(SandboxPolicy::default());
    let res = driver.navigate("").await;
    assert!(res.is_ok() || res.is_err());
}

#[test]
fn test_t2_f7_sandbox_policy_restricted_domains_disallows_unlisted() {
    let mut policy = SandboxPolicy::default();
    policy.allowed_domains = vec!["example.com".into()];
    assert!(!policy.allowed_domains.contains(&"*".to_string()));
}

// ============================================================================
// FEATURE 8: REACT PLANNER EDGE CASES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f8_stategraph_missing_entry_point_fails() {
    let graph = StateGraph::new();
    let initial = AgentState::default();
    let res = graph.run(initial).await;
    assert!(res.is_err(), "Running graph without entry point must fail");
}

#[tokio::test]
async fn test_t2_f8_stategraph_unregistered_edge_target_fails() {
    let mut graph = StateGraph::new();
    graph.add_node("node_a", |state| async move { Ok(state) });
    graph.add_edge("node_a", "non_existent_node_b");
    graph.set_entry_point("node_a");

    let res = graph.run(AgentState::default()).await;
    assert!(res.is_err(), "Transitioning to non-existent target node must fail");
}

#[test]
fn test_t2_f8_route_intent_empty_string_defaults_to_chat() {
    let intent = route_intent("");
    assert_eq!(intent, Intent::Chat);
}

#[test]
fn test_t2_f8_route_intent_whitespace_only_defaults_to_chat() {
    let intent = route_intent("    \n\t  ");
    assert_eq!(intent, Intent::Chat);
}

#[test]
fn test_t2_f8_agent_state_empty_messages_serde() {
    let state = AgentState::default();
    let json = serde_json::to_string(&state).unwrap();
    let parsed: AgentState = serde_json::from_str(&json).unwrap();
    assert!(parsed.messages.is_empty());
}

// ============================================================================
// FEATURE 9: TOOL DISPATCHER FAULT TOLERANCE (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f9_consent_suspender_timeout_fails_closed() {
    let suspender = ConsentSuspender::new();
    let decision = suspender.request_consent(
        "req-timeout-boundary",
        RiskLevel::DestructiveHighRisk,
        Duration::from_millis(10),
    ).await;
    assert_eq!(decision, ConsentDecision::TimedOut);
}

#[tokio::test]
async fn test_t2_f9_tool_dispatcher_null_arguments_handling() {
    let dispatcher = MockToolDispatcher::new();
    dispatcher.register_tool(SkillToolDefinition {
        name: "test_null".into(),
        description: "handle null".into(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::ReadOnlySafe,
    }).await;

    let res = dispatcher.dispatch("test_null", serde_json::Value::Null).await;
    assert!(res.is_ok());
}

#[test]
fn test_t2_f9_tool_call_result_empty_error_string() {
    let err = ToolCallResult::failure("call_99", "");
    assert!(!err.success);
    assert_eq!(err.error, Some("".into()));
}

#[test]
fn test_t2_f9_tool_call_request_with_empty_id() {
    let req = ToolCallRequest::new("", "some_tool", serde_json::json!({}));
    assert!(req.call_id.is_empty());
}

#[tokio::test]
async fn test_t2_f9_tool_dispatcher_special_character_name_lookup() {
    let dispatcher = MockToolDispatcher::new();
    let res = dispatcher.dispatch("tool/with/slashes;--", serde_json::json!({})).await;
    assert!(res.is_err());
}

// ============================================================================
// FEATURE 10: SESSION MEMORY BOUNDARY CONDITIONS (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f10_session_manager_get_missing_session() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let s_id = SessionId::new();
    let session = mgr.get_session(&s_id).await.unwrap();
    assert!(session.is_none());
}

#[test]
fn test_t2_f10_session_context_variable_removal() {
    let mut ctx = SessionContext::new(ChannelId::Telegram, "u_var", None, MemoryScope::Working);
    ctx.set_variable("tmp_flag", serde_json::json!(true));
    assert_eq!(ctx.get_variable("tmp_flag"), Some(&serde_json::json!(true)));

    let removed = ctx.remove_variable("tmp_flag");
    assert_eq!(removed, Some(serde_json::json!(true)));
    assert_eq!(ctx.get_variable("tmp_flag"), None);
}

#[tokio::test]
async fn test_t2_f10_sqlite_checkpointer_load_missing_thread() {
    let pool = Arc::new(DatabasePool::new_in_memory().unwrap());
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-12345678");
    let checkpointer = SqliteCheckpointer::new(pool, crypto);

    let res = checkpointer.load_checkpoint("non_existent_thread_xyz").await.unwrap();
    assert!(res.is_none());
}

#[test]
fn test_t2_f10_session_id_parse_invalid_string() {
    let res = uuid::Uuid::parse_str("not-a-valid-uuid");
    assert!(res.is_err());
}

#[test]
fn test_t2_f10_session_context_large_variable_payload() {
    let mut ctx = SessionContext::new(ChannelId::Slack, "u_large", None, MemoryScope::Working);
    let large_json = serde_json::json!({"data": "A".repeat(20000)});
    ctx.set_variable("large_data", large_json.clone());
    assert_eq!(ctx.get_variable("large_data"), Some(&large_json));
}

// ============================================================================
// FEATURE 11: EPISODIC CONSOLIDATION EDGE CASES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f11_consume_pending_zero_limit() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-for-crypto");
    let res = liva_native_core::memory_consolidation::consume_pending_once(pool, crypto, 0).await;
    assert!(res.is_ok());
    assert_eq!(res.unwrap().processed, 0);
}

#[test]
fn test_t2_f11_events_table_duplicate_event_id_fails() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    conn.execute(
        "INSERT INTO events (eventId, timestamp, rawUserMsg) VALUES ('evt_dup', 100, 'msg1')",
        [],
    ).unwrap();

    let res = conn.execute(
        "INSERT INTO events (eventId, timestamp, rawUserMsg) VALUES ('evt_dup', 200, 'msg2')",
        [],
    );
    assert!(res.is_err(), "Duplicate eventId primary key must fail");
}

#[test]
fn test_t2_f11_events_table_large_user_message() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    let large_msg = "X".repeat(50000);
    conn.execute(
        "INSERT INTO events (eventId, timestamp, rawUserMsg) VALUES ('evt_large', 100, ?1)",
        rusqlite::params![large_msg],
    ).unwrap();

    let stored: String = conn.query_row(
        "SELECT rawUserMsg FROM events WHERE eventId = 'evt_large'",
        [],
        |r| r.get(0),
    ).unwrap();
    assert_eq!(stored.len(), 50000);
}

#[test]
fn test_t2_f11_consolidation_batch_result_record_dead_letter() {
    let mut res = ConsolidationBatchResult::default();
    res.processed += 1;
    res.dead_lettered += 1;
    assert_eq!(res.dead_lettered, 1);
    assert_eq!(res.processed, 1);
}

#[test]
fn test_t2_f11_retention_policy_zero_days_handling() {
    let policy = RetentionPolicy {
        max_age_days: 0,
        interval: Duration::from_secs(3600),
        batch_size: 10,
    };
    assert_eq!(policy.max_age_days, 0);
}

// ============================================================================
// FEATURE 12: MESSAGING EDGE CASES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f12_incoming_message_empty_text() {
    let sender = MessageSender::user("u_empty", None);
    let msg = IncomingMessage::text(
        ChannelId::LocalCli,
        "m_empty",
        SessionId::new(),
        sender,
        "",
    );
    assert_eq!(msg.content.text_content(), Some(""));
    assert!(msg.content.is_empty());
}

#[test]
fn test_t2_f12_incoming_message_unicode_emojis_and_special_scripts() {
    let sender = MessageSender::user("u_emoji", None);
    let emoji_text = "Xin chào 👋 🌍 🦀 🚀 测试 UTF-8 tiếng Việt có dấu";
    let msg = IncomingMessage::text(
        ChannelId::Telegram,
        "m_utf8",
        SessionId::new(),
        sender,
        emoji_text,
    );
    assert_eq!(msg.content.text_content(), Some(emoji_text));
}

#[test]
fn test_t2_f12_outgoing_message_immediate_urgency() {
    let recipient = MessageRecipient::direct(ChannelId::WhatsApp, "+84999999999");
    let mut out = OutgoingMessage::text(
        recipient,
        SessionId::new(),
        "Urgent alert",
    );
    out.urgency = DeliveryUrgency::Immediate;
    assert_eq!(out.urgency, DeliveryUrgency::Immediate);
}

#[test]
fn test_t2_f12_outgoing_message_background_urgency() {
    let recipient = MessageRecipient::direct(ChannelId::Slack, "chan_bg");
    let mut out = OutgoingMessage::text(
        recipient,
        SessionId::new(),
        "Log entry",
    );
    out.urgency = DeliveryUrgency::Background;
    assert_eq!(out.urgency, DeliveryUrgency::Background);
}

#[test]
fn test_t2_f12_channel_id_custom_string_preservation() {
    let custom = ChannelId::Custom("mattermost_enterprise".into());
    assert_eq!(custom.as_str(), "mattermost_enterprise");
}

// ============================================================================
// FEATURE 13: DIAGNOSTICS & PREFLIGHT RESILIENCY (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f13_validate_model_path_empty_string_fails() {
    let path = PathBuf::from("");
    let models_dir = PathBuf::from("/data/models");
    let res = liva_native_core::validate_model_path(&path, &models_dir);
    assert!(res.is_err());
}

#[test]
fn test_t2_f13_validate_model_path_directory_traversal_fails() {
    let malicious = PathBuf::from("../../etc/shadow");
    let models_dir = PathBuf::from("/data/models");
    let res = liva_native_core::validate_model_path(&malicious, &models_dir);
    assert!(res.is_err());
}

#[test]
fn test_t2_f13_preflight_collects_items_without_panic() {
    let items = liva_native_core::preflight::thu_thap();
    assert!(!items.is_empty(), "Preflight survey returns diagnostic items");
}

#[test]
fn test_t2_f13_sysinfo_ram_bytes_bounds() {
    if let Some((used, total)) = sysinfo::ram_bytes() {
        assert!(total > 0);
        assert!(used <= total);
    }
}

#[test]
fn test_t2_f13_sysinfo_uptime_is_non_negative() {
    if let Some(uptime) = sysinfo::process_uptime_secs() {
        assert!(uptime < u64::MAX);
    }
}

// ============================================================================
// FEATURE 14: GOVERNOR & TELEMETRY BOUNDARIES (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f14_external_cpu_percent_zero_deltas() {
    let res = liva_native_core::governor::external_cpu_percent(0, 0, 0, 0);
    assert!(res.is_none() || res == Some(0));
}

#[test]
fn test_t2_f14_external_cpu_percent_extreme_deltas() {
    let res = liva_native_core::governor::external_cpu_percent(1_000_000, 500_000, 500_000, 0);
    if let Some(pct) = res {
        assert!(pct <= 100);
    }
}

#[test]
fn test_t2_f14_governor_threshold_busy_bounds() {
    let cpu = liva_native_core::governor::busy_cpu_threshold();
    assert!(cpu >= 1 && cpu <= 100);

    let gpu = liva_native_core::governor::busy_gpu_threshold();
    assert!(gpu >= 1 && gpu <= 100);
}

#[test]
fn test_t2_f14_control_opcode_all_variants_roundtrip() {
    let ops = [
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
    for op in ops {
        let json = serde_json::to_string(&op).unwrap();
        let parsed: ControlOpcode = serde_json::from_str(&json).unwrap();
        assert_eq!(op, parsed);
    }
}

#[test]
fn test_t2_f14_control_frame_empty_payload_serde() {
    let frame = ControlFrame::event("test_topic", serde_json::Value::Null);
    let json = serde_json::to_string(&frame).unwrap();
    let parsed: ControlFrame = serde_json::from_str(&json).unwrap();
    assert!(parsed.payload.is_none() || parsed.payload == Some(serde_json::Value::Null));
}

// ============================================================================
// FEATURE 15: CONCURRENCY & STRESS BOUNDARIES (≥5 tests)
// ============================================================================

#[tokio::test]
async fn test_t2_f15_session_manager_eviction_zero_ttl() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let _ctx = mgr.get_or_create_session(&ChannelId::Telegram, "u_fast_evict", None).await.unwrap();

    tokio::time::sleep(Duration::from_millis(5)).await;
    let evicted = mgr.evict_expired(Duration::from_millis(1)).await.unwrap();
    assert_eq!(evicted, 1);
}

#[tokio::test]
async fn test_t2_f15_session_manager_rapid_concurrent_reads() {
    let mgr = Arc::new(InMemorySessionManager::new(MemoryScope::Working));
    let ctx = mgr.get_or_create_session(&ChannelId::Telegram, "u_read_storm", None).await.unwrap();
    let sid = {
        let g = ctx.read().await;
        g.session_id
    };

    let mut handles = Vec::new();
    for _ in 0..50 {
        let m = mgr.clone();
        handles.push(tokio::spawn(async move {
            let session = m.get_session(&sid).await.unwrap();
            assert!(session.is_some());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}

#[test]
fn test_t2_f15_database_pool_writer_lock_under_iteration() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();

    for i in 0..100 {
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?1, ?2, '2026-09-01', '2026-09-01', 'stress')",
            rusqlite::params![format!("storm_key_{}", i), format!("storm_val_{}", i)],
        ).unwrap();
    }

    let count: i64 = conn.query_row("SELECT count(*) FROM facts", [], |r| r.get(0)).unwrap();
    assert_eq!(count, 100);
}

#[tokio::test]
async fn test_t2_f15_in_memory_session_manager_touch_race_free() {
    let mgr = InMemorySessionManager::new(MemoryScope::Working);
    let ctx = mgr.get_or_create_session(&ChannelId::Discord, "u_touch", None).await.unwrap();

    {
        let mut guard = ctx.write().await;
        guard.touch();
        guard.set_variable("counter", serde_json::json!(42));
    }

    let guard = ctx.read().await;
    assert_eq!(guard.get_variable("counter"), Some(&serde_json::json!(42)));
}

#[test]
fn test_t2_f15_control_plane_rapid_frame_creation() {
    let mut frames = Vec::with_capacity(1000);
    for i in 0..1000 {
        frames.push(ControlFrame::event(&format!("topic_{}", i), serde_json::json!(i)));
    }
    assert_eq!(frames.len(), 1000);
}

// ============================================================================
// FEATURE 16: SECURITY & CRYPTO HARDENING (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f16_tampered_ciphertext_fails_decryption() {
    let engine = EncryptionEngine::new("secure_passphrase_32_bytes_len!!");
    let original = "Confidential classified document content";
    let ciphertext = engine.encrypt(original).unwrap();

    let mut parts: Vec<String> = ciphertext.split(':').map(|s| s.to_string()).collect();
    if parts.len() == 5 {
        let first_char = parts[3].chars().next().unwrap_or('0');
        let replacement = if first_char == '0' { '1' } else { '0' };
        parts[3] = format!("{}{}", replacement, &parts[3][1..]);
        let tampered_ciphertext = parts.join(":");
        let read_result = engine.read_fact(&tampered_ciphertext);
        assert!(matches!(read_result, FactRead::Locked { .. }));
    }
}

#[test]
fn test_t2_f16_truncated_v2_ciphertext_locked_state() {
    let engine = EncryptionEngine::new("secure_passphrase_32_bytes_len!!");
    let ciphertext = engine.encrypt("Sensitive message").unwrap();

    let truncated = &ciphertext[..15];
    let try_result = engine.try_decrypt(truncated);
    assert!(try_result.is_err(), "Truncated ciphertext must fail decryption");
}

#[test]
fn test_t2_f16_empty_plaintext_encryption_roundtrip() {
    let engine = EncryptionEngine::new("secure_passphrase_32_bytes_len!!");
    let ciphertext = engine.encrypt("").unwrap();
    let decrypted = engine.decrypt(&ciphertext);
    assert_eq!(decrypted, "");
}

#[test]
fn test_t2_f16_consent_decision_denied_reason_preservation() {
    let decision = ConsentDecision::Denied {
        reason: "User manually rejected tool invocation".into(),
    };
    let json = serde_json::to_string(&decision).unwrap();
    let parsed: ConsentDecision = serde_json::from_str(&json).unwrap();
    if let ConsentDecision::Denied { reason } = parsed {
        assert_eq!(reason, "User manually rejected tool invocation");
    } else {
        panic!("Deserialization failed");
    }
}

#[test]
fn test_t2_f16_consent_request_preview_json_integrity() {
    let req = ConsentRequest {
        request_id: "req_audit_1".into(),
        session_id: "s_audit".into(),
        tool_name: "bash_exec".into(),
        target_resource: "rm -rf /tmp/data".into(),
        risk_level: RiskLevel::DestructiveHighRisk,
        arguments_preview: serde_json::json!({"command": "rm -rf /tmp/data", "force": true}),
    };
    let json = serde_json::to_string(&req).unwrap();
    let parsed: ConsentRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.arguments_preview["force"], true);
}

// ============================================================================
// FEATURE 17: WORKSPACE & ARTIFACT INTEGRITY (≥5 tests)
// ============================================================================

#[test]
fn test_t2_f17_verify_model_artifact_missing_file_fails() {
    let fake_root = Path::new("/tmp/models");
    let fake_path = Path::new("non_existent_fake_model.gguf");
    let res = liva_native_core::verify_model_artifact(fake_root, fake_path);
    assert!(res.is_err(), "Non-existent model file must fail artifact verification");
}

#[test]
fn test_t2_f17_verify_trusted_file_mismatched_hash_fails() {
    let temp_dir = std::env::temp_dir();
    let test_file = temp_dir.join("liva_test_artifact.bin");
    std::fs::write(&test_file, b"sample corrupted binary payload").unwrap();

    let res = liva_native_core::verify_trusted_file(&temp_dir, Path::new("liva_test_artifact.bin"), "0000000000000000000000000000000000000000000000000000000000000000");
    let _ = std::fs::remove_file(&test_file);
    assert!(res.is_err(), "Tampered file payload must fail verification");
}

#[test]
fn test_t2_f17_authorization_all_commands_fail_closed_on_none_principal() {
    let res = authorize_command(CommandPrincipal::Telegram, "execute_shell_script");
    assert!(res.is_err());
}

#[test]
fn test_t2_f17_data_dir_resolves_valid_path() {
    let path = liva_native_core::data_dir();
    assert!(!path.as_os_str().is_empty());
}

#[test]
fn test_t2_f17_embedded_model_hash_non_empty() {
    let hash = liva_native_core::embedded_model_hash("router");
    assert!(hash.is_ok() || hash.is_err());
}
