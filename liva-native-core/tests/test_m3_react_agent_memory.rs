//! Milestone 3 (M3) Integration Test Suite: Autonomous ReAct Loop & Hierarchical Memory Engine.
//!
//! Verifies:
//! 1. Autonomous ReAct Planner & Iterative Execution Loop (`Thought -> Action -> Observation`)
//! 2. Self-Healing Error Recovery & Reflection Retry (up to 3 retries, fallback tool selection)
//! 3. Hierarchical Working Memory (`AgentState`, `TaskPlan`, step outputs, bounded scratchpad)
//! 4. Episodic Memory with SQLite WAL + 384-dim INT8 vector indexing + FTS5 RRF hybrid search
//! 5. Semantic Memory Consolidation & Retention (fact extraction, HKDF+AES-GCM encryption, KG nodes/edges)
//! 6. Unified Channel Dispatch Integration (`agent:react_step`, `agent:plan_and_execute`)

use liva_native_core::agent::graph::ConversationMemoryScope;
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::plan::{
    MAX_PLAN_STEPS, MAX_TOOL_RETRIES_PER_STEP, PlanStep, TaskPlan,
};
use liva_native_core::agent::react::{
    AgentError, AgentLoop, ReActPlanner, ReActThought, StepOutcome,
};
use liva_native_core::agent::state::{AgentState, MAX_SCRATCHPAD_ENTRIES};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{DatabasePool, MEMORY_VECTOR_DIM, persist_conversation_event_vector};
use liva_native_core::memory_consolidation::process_pending_batch;
use liva_native_core::skills::dispatcher::UnifiedToolDispatcher;
use liva_native_core::skills::manifest::{RiskLevel, SkillToolDefinition};
use liva_native_core::AppState;
use serde_json::json;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

fn test_crypto() -> EncryptionEngine {
    EncryptionEngine::new("m3-test-secret-key-32-bytes-long")
}

fn create_test_app_state() -> Arc<AppState> {
    let capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    Arc::new(AppState {
        db: DatabasePool::new_in_memory().expect("in-memory db"),
        crypto: test_crypto(),
        stt: tokio::sync::Mutex::new(liva_native_core::stt::SttManager::new("non_existent_dir")),
        tts: tokio::sync::Mutex::new(None),
        tts_player: liva_native_core::tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(
            liva_native_core::llm::LlamaRouterManager::new(512, 0).expect("llm manager"),
        ),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

// ============================================================================
// 1. Autonomous ReAct Planner & Iterative Execution Loop
// ============================================================================

#[test]
fn test_react_goal_decomposition_multi_step() {
    let planner = ReActPlanner::new();
    let state = AgentState::default();

    // Multi-goal smart home command
    let plan = planner
        .plan("bật đèn phòng khách, bật quạt và bật điều hoà", &state)
        .expect("multi-goal decomposition");

    assert_eq!(plan.steps.len(), 3);
    assert_eq!(plan.steps[0].id, "step-light");
    assert_eq!(plan.steps[0].tool_name.as_deref(), Some("control_smarthome"));
    assert_eq!(plan.steps[1].id, "step-fan");
    assert_eq!(plan.steps[1].tool_name.as_deref(), Some("control_smarthome"));
    assert_eq!(plan.steps[2].id, "step-ac");
    assert_eq!(plan.steps[2].tool_name.as_deref(), Some("control_smarthome"));

    // Knowledge search goal
    let search_plan = planner
        .plan("tìm kiếm thông tin kiến trúc LIVA trong vault", &state)
        .expect("search goal decomposition");
    assert!(search_plan.steps.len() >= 2);
    assert_eq!(search_plan.steps[0].id, "step-search");
    assert_eq!(search_plan.steps[0].tool_name.as_deref(), Some("search_vault"));
}

#[test]
fn test_react_plan_bounds_and_token_budget_control() {
    let planner = ReActPlanner::new().with_max_steps(3);
    let mut steps = Vec::new();
    for i in 1..=8 {
        steps.push(PlanStep::new(format!("s{i}"), format!("Step {i}")));
    }
    let plan = TaskPlan::new("Goal with 8 steps", steps);
    assert_eq!(plan.steps.len(), MAX_PLAN_STEPS, "Must clamp to MAX_PLAN_STEPS");

    // Empty goal rejection
    let empty_res = planner.plan("   ", &AgentState::default());
    assert!(empty_res.is_err());
    assert!(matches!(empty_res.unwrap_err(), AgentError::PlanningFailed(_)));

    // Token budget exceeded check
    let strict_planner = ReActPlanner {
        max_steps: 5,
        max_retries: 3,
        token_budget: 10,
    };
    let huge_goal = "a".repeat(100);
    let budget_res = strict_planner.plan(&huge_goal, &AgentState::default());
    assert!(budget_res.is_err());
    assert!(matches!(budget_res.unwrap_err(), AgentError::TokenBudgetExceeded { .. }));
}

#[tokio::test]
async fn test_react_loop_step_progression_and_synthesis() {
    let dispatcher = UnifiedToolDispatcher::new();
    dispatcher
        .register_native_handler(
            SkillToolDefinition {
                name: "control_smarthome".to_string(),
                description: "Smart home controller".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            },
            |args| {
                Box::pin(async move {
                    let dev = args.get("device").and_then(|d| d.as_str()).unwrap_or("unknown");
                    let act = args.get("action").and_then(|a| a.as_str()).unwrap_or("unknown");
                    Ok(json!({ "status": "ok", "device": dev, "action": act }))
                })
            },
        )
        .await;

    let mut state = AgentState {
        messages: vec![json!({"role": "user", "content": "bật đèn và bật quạt"})],
        ..Default::default()
    };

    // Step 1: Execute light control
    let outcome1 = AgentLoop::step(&mut state, &dispatcher).await.expect("step 1");
    assert!(matches!(outcome1, StepOutcome::StepCompleted { step_index: 0, .. }));
    assert_eq!(state.active_step_index, 1);
    assert_eq!(state.step_outputs.len(), 1);

    // Step 2: Execute fan control and reach completion
    let outcome2 = AgentLoop::step(&mut state, &dispatcher).await.expect("step 2");
    assert!(matches!(outcome2, StepOutcome::PlanCompleted { .. }));
    let plan = state.get_plan().expect("plan present");
    assert!(plan.is_finished());
    assert_eq!(plan.successful_steps_count(), 2);
    assert_eq!(plan.progress_percentage(), 100.0);
}

// ============================================================================
// 2. Self-Healing Error Recovery & Reflection Retry
// ============================================================================

#[tokio::test]
async fn test_react_self_healing_vault_fallback_to_search_tool() {
    let dispatcher = Arc::new(UnifiedToolDispatcher::new());
    let vault_attempts = Arc::new(AtomicUsize::new(0));
    let fallback_called = Arc::new(AtomicUsize::new(0));

    let v_att = vault_attempts.clone();
    dispatcher
        .register_native_handler(
            SkillToolDefinition {
                name: "search_vault".to_string(),
                description: "Search notes in vault".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            },
            move |_args| {
                let att = v_att.clone();
                Box::pin(async move {
                    att.fetch_add(1, Ordering::SeqCst);
                    Err("Obsidian Vault lock timeout: resource busy".to_string())
                })
            },
        )
        .await;

    let fb_att = fallback_called.clone();
    dispatcher
        .register_native_handler(
            SkillToolDefinition {
                name: "search_tool".to_string(),
                description: "Web/knowledge search fallback".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            },
            move |_args| {
                let fb = fb_att.clone();
                Box::pin(async move {
                    fb.fetch_add(1, Ordering::SeqCst);
                    Ok(json!({ "results": ["Document match 1", "Document match 2"] }))
                })
            },
        )
        .await;

    let mut state = AgentState {
        messages: vec![json!({"role": "user", "content": "tìm kiếm tài liệu LIVA"})],
        ..Default::default()
    };

    let result = AgentLoop::run(&mut state, &dispatcher, 8).await;
    assert!(result.is_ok(), "Self-healing loop must recover through fallback");
    assert!(vault_attempts.load(Ordering::SeqCst) >= 1, "Vault search was attempted");
    assert!(fallback_called.load(Ordering::SeqCst) >= 1, "Fallback search was executed");

    // Scratchpad must store reflection reasoning
    assert!(state.scratchpad.keys().any(|k| k.starts_with("reflection_")));
    assert!(state.get_plan().unwrap().is_finished());
}

#[tokio::test]
async fn test_react_self_healing_device_argument_normalization() {
    let dispatcher = Arc::new(UnifiedToolDispatcher::new());
    let calls = Arc::new(AtomicUsize::new(0));

    let calls_clone = calls.clone();
    dispatcher
        .register_native_handler(
            SkillToolDefinition {
                name: "control_smarthome".to_string(),
                description: "Smart home control".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            },
            move |args| {
                let c = calls_clone.clone();
                Box::pin(async move {
                    let attempt = c.fetch_add(1, Ordering::SeqCst);
                    let dev = args.get("device").and_then(|d| d.as_str()).unwrap_or("");
                    if attempt == 0 && dev == "bong_den" {
                        Err(format!("Device '{dev}' not recognized in Zigbee registry"))
                    } else if dev == "light" {
                        Ok(json!({ "status": "success", "device": "light", "state": "on" }))
                    } else {
                        Err("Unknown device".to_string())
                    }
                })
            },
        )
        .await;

    let planner = ReActPlanner::new();
    let step = PlanStep::new("step-den", "Bật bóng đèn")
        .with_tool("control_smarthome", json!({ "device": "bong_den", "action": "on" }));

    // Simulate first failure & reflection
    let reflection = planner.reflect_on_tool_failure(&step, "Device 'bong_den' not recognized").unwrap();
    if let ReActThought::ReflectAndRetry { corrected_tool, corrected_arguments, retry_count, .. } = reflection {
        assert_eq!(corrected_tool, "control_smarthome");
        assert_eq!(corrected_arguments.get("device").and_then(|d| d.as_str()), Some("light"));
        assert_eq!(retry_count, 1);
    } else {
        panic!("Expected ReflectAndRetry thought");
    }
}

#[tokio::test]
async fn test_react_max_retries_exhaustion_behavior() {
    let dispatcher = Arc::new(UnifiedToolDispatcher::new());
    dispatcher
        .register_native_handler(
            SkillToolDefinition {
                name: "failing_tool".to_string(),
                description: "Permanently broken tool".to_string(),
                risk_level: RiskLevel::ReadOnlySafe,
                input_schema: json!({}),
            },
            |_args| {
                Box::pin(async move {
                    Err("Permanent hardware failure".to_string())
                })
            },
        )
        .await;

    let mut state = AgentState::default();
    let plan = TaskPlan::new(
        "Failing Goal",
        vec![
            PlanStep::new("s1", "Attempt failing tool")
                .with_tool("failing_tool", json!({})),
        ],
    );
    state.set_plan(plan);

    let result = AgentLoop::run(&mut state, &dispatcher, 10).await;
    assert!(result.is_ok(), "Engine should terminate gracefully even when max retries are exhausted");
    let plan = state.get_plan().unwrap();
    assert!(plan.steps[0].is_failed());
    assert_eq!(plan.steps[0].retries, MAX_TOOL_RETRIES_PER_STEP);
}

// ============================================================================
// 3. Hierarchical Working Memory & Checkpointing
// ============================================================================

#[test]
fn test_working_memory_scratchpad_bounds_and_operations() {
    let mut state = AgentState::default();

    // Insert within capacity
    for i in 0..MAX_SCRATCHPAD_ENTRIES {
        state.scratchpad_set(format!("key_{i}"), json!({ "value": i }));
    }
    assert_eq!(state.scratchpad.len(), MAX_SCRATCHPAD_ENTRIES);

    // Overflowing by 10 entries preserves boundary <= MAX_SCRATCHPAD_ENTRIES
    for i in MAX_SCRATCHPAD_ENTRIES..(MAX_SCRATCHPAD_ENTRIES + 10) {
        state.scratchpad_set(format!("key_{i}"), json!({ "value": i }));
        assert!(state.scratchpad.len() <= MAX_SCRATCHPAD_ENTRIES);
    }

    assert_eq!(state.scratchpad.len(), MAX_SCRATCHPAD_ENTRIES);
    assert!(state.scratchpad_get("key_70").is_some());

    // Removal and clear
    assert!(state.scratchpad_remove("key_70").is_some());
    state.clear_scratchpad();
    assert!(state.scratchpad.is_empty());
}

#[tokio::test]
async fn test_working_memory_encrypted_checkpoint_roundtrip() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let crypto = test_crypto();
    let checkpointer = SqliteCheckpointer::new(Arc::new(pool), crypto.clone());

    let state = AgentState {
        messages: vec![
            json!({"role": "user", "content": "Thiết lập hệ thống thông minh"}),
            json!({"role": "assistant", "content": "Đang lập kế hoạch..."}),
        ],
        current_node: "react_step".to_string(),
        context: std::collections::HashMap::new(),
        active_plan: Some(TaskPlan::new(
            "Smart Setup",
            vec![
                PlanStep::new("s1", "Scan devices"),
                PlanStep::new("s2", "Authorize permissions"),
            ],
        )),
        active_step_index: 1,
        step_outputs: [("s1".to_string(), json!({"scanned": 4}))].into_iter().collect(),
        scratchpad: [("intent".to_string(), json!("home_automation"))].into_iter().collect(),
        ..Default::default()
    };

    let thread_id = "thread_session_m3_test";
    checkpointer.save_checkpoint(thread_id, &state).await.expect("checkpoint save");

    let loaded_state = checkpointer.load_checkpoint(thread_id).await.expect("checkpoint load").expect("state exists");
    assert_eq!(loaded_state.current_node, "react_step");
    assert_eq!(loaded_state.active_step_index, 1);
    assert_eq!(loaded_state.active_plan.unwrap().steps.len(), 2);
    assert_eq!(loaded_state.step_outputs.get("s1"), Some(&json!({"scanned": 4})));
    assert_eq!(loaded_state.scratchpad.get("intent"), Some(&json!("home_automation")));
}

// ============================================================================
// 4. Episodic Memory & Cross-Session Vector/FTS Hybrid Search
// ============================================================================

#[tokio::test]
async fn test_episodic_memory_cross_session_storage_and_scope_isolation() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();
    let crypto = test_crypto();

    let owner_a = ConversationMemoryScope::new("user_alice", "session_1").unwrap();
    let owner_a_sess2 = ConversationMemoryScope::new("user_alice", "session_2").unwrap();
    let owner_b = ConversationMemoryScope::new("user_bob", "session_1").unwrap();

    let vec_query = vec![0.15_f32; MEMORY_VECTOR_DIM];

    // Persist turns for Alice in session 1
    persist_conversation_event_vector(
        &conn,
        &crypto,
        "turn_alice_1",
        "Người dùng: tôi thích uống trà sen và làm việc ban đêm\nLIVA: Đã ghi nhận sở thích của bạn.",
        &vec_query,
        owner_a.storage_domain(),
        owner_a.storage_category(),
    )
    .expect("persist turn alice");

    // Persist turns for Bob in session 1
    persist_conversation_event_vector(
        &conn,
        &crypto,
        "turn_bob_1",
        "Người dùng: tôi thích uống cà phê đen và dậy sớm\nLIVA: Đã ghi nhận sở thích của Bob.",
        &vec_query,
        owner_b.storage_domain(),
        owner_b.storage_category(),
    )
    .expect("persist turn bob");

    // Alice searches in session 2: cross-session retrieval works within same owner
    let hits_alice = liva_native_core::db::search_hybrid_vectors(
        &conn,
        &crypto,
        "trà sen",
        &vec_query,
        5,
        &owner_a_sess2.recall_filter(),
        1.0,
        1.0,
    )
    .expect("hybrid search alice");

    assert!(!hits_alice.is_empty(), "Alice should recall cross-session memories");
    assert!(hits_alice[0].content.contains("trà sen"));
    assert!(!hits_alice.iter().any(|h| h.content.contains("cà phê đen")), "Must not leak Bob's memories to Alice");

    // Bob searches in session 1: only sees his own memories
    let hits_bob = liva_native_core::db::search_hybrid_vectors(
        &conn,
        &crypto,
        "cà phê",
        &vec_query,
        5,
        &owner_b.recall_filter(),
        1.0,
        1.0,
    )
    .expect("hybrid search bob");

    assert!(!hits_bob.is_empty());
    assert!(hits_bob[0].content.contains("cà phê đen"));
    assert!(!hits_bob.iter().any(|h| h.content.contains("trà sen")));
}

// ============================================================================
// 5. Semantic Memory Consolidation & Retention Engine
// ============================================================================

#[tokio::test]
async fn test_semantic_memory_consolidation_full_lifecycle() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let conn = pool.writer.get().unwrap();
    let crypto = test_crypto();

    // 1. Seed conversation events
    persist_conversation_event_vector(
        &conn,
        &crypto,
        "event_conv_101",
        "Người dùng: tôi tên là David, thích nhiệt độ điều hòa 24 độ và đang phát triển dự án LIVA Core\nLIVA: Xin chào David, tôi đã lưu thông tin.",
        &vec![0.1_f32; MEMORY_VECTOR_DIM],
        "memory_owner:david",
        "conversation:session_init",
    )
    .expect("seed event");

    // 2. Run consolidation batch
    let result = process_pending_batch(&conn, &crypto, "consolidation-test-worker", 10)
        .expect("process pending consolidation");

    assert_eq!(result.processed, 1);
    assert_eq!(result.consolidated, 1);
    assert!(result.facts_extracted >= 1, "Must extract semantic facts");
    assert!(result.nodes_created >= 1, "Must create KG nodes");

    // 3. Verify encrypted fact stored in facts table
    let encrypted_val: String = conn
        .query_row("SELECT value FROM facts WHERE key = 'user:name'", [], |r| r.get(0))
        .expect("query fact user:name");
    assert!(encrypted_val.starts_with("v2:"), "Fact must be encrypted with v2 HKDF+AES-GCM");
    let decrypted_name = crypto.read_fact(&encrypted_val).into_value();
    assert_eq!(decrypted_name, "David");

    // 4. Verify knowledge graph nodes and edges
    let node_count: i64 = conn
        .query_row("SELECT count(*) FROM l3_nodes", [], |r| r.get(0))
        .expect("count l3_nodes");
    assert!(node_count >= 1);

    // 5. Verify consolidation checkpoint
    let (last_step, state_json): (i64, String) = conn
        .query_row(
            "SELECT last_step, state_data FROM consolidation_checkpoints WHERE session_id = 'consolidation-test-worker'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query checkpoint");
    assert_eq!(last_step, 1);
    assert!(state_json.contains("event_conv_101"));

    // 6. Test Retention sweep with positive cutoff (1000ms after epoch, so recent event is preserved)
    let sweep_report = liva_native_core::db::sweep_conversation_retention(
        &conn,
        "david",
        1000,
        10,
        false,
    )
    .expect("retention sweep");
    assert_eq!(sweep_report.deletions.len(), 0, "Recent event must not be deleted");
}

// ============================================================================
// 6. Unified Channel Dispatch Integration
// ============================================================================

#[tokio::test]
async fn test_unified_channel_agent_react_commands() {
    let state = create_test_app_state();

    // 1. Test agent:react_step command
    let step_payload = json!({
        "goal": "bật đèn phòng khách"
    });
    let step_res = liva_native_core::commands::llm::handle(
        state.clone(),
        "agent:react_step",
        step_payload,
        None,
        None,
    )
    .await
    .expect("agent:react_step command execution");

    assert!(step_res.get("outcome").is_some());
    assert!(step_res.get("state").is_some());

    // 2. Test agent:plan_and_execute command
    let exec_payload = json!({
        "goal": "kiểm tra thời tiết Hà Nội",
        "maxIterations": 5
    });
    let exec_res = liva_native_core::commands::llm::handle(
        state.clone(),
        "agent:plan_and_execute",
        exec_payload,
        None,
        None,
    )
    .await
    .expect("agent:plan_and_execute command execution");

    assert!(exec_res.get("final_answer").is_some());
    assert!(exec_res.get("plan").is_some());
    let final_ans = exec_res["final_answer"].as_str().unwrap();
    assert!(final_ans.contains("Đã hoàn thành kế hoạch") || final_ans.contains("thời tiết"));
}
