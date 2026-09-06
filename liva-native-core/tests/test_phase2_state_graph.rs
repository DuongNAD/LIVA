//! Phase 2 Integration Tests: Features 1-4
//! - Feature 1: Cyclic State Graph FSM Engine (RFC-003 R1)
//! - Feature 2: Immutable Step Checkpoints with RFC 6902 JSON Patches (RFC-003 R1)
//! - Feature 3: Human-In-The-Loop (HITL) Gating & Suspension (RFC-003 R1)
//! - Feature 4: Time-Travel State & Branch Recovery & Replay Cache (RFC-003 R1)

use liva_native_core::agent::graph::checkpoint::{
    apply_json_patch, generate_json_patch, Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::graph::hitl::{ApprovalContext, ApprovalDecision, CheckpointStatus};
use liva_native_core::agent::graph::StateGraph;
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use serde_json::json;
use std::sync::Arc;

// ============================================================================
// HELPER UTILITIES
// ============================================================================

fn setup_test_db() -> (Arc<DatabasePool>, EncryptionEngine) {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("Failed to create in-memory database"));
    let crypto = EncryptionEngine::new("test_secret_passphrase_for_agent_checkpoints_32b!");
    (pool, crypto)
}

fn create_agent_state(user_text: &str, thread_id: &str) -> AgentState {
    let mut state = AgentState::default();
    state.messages.push(json!({
        "role": "user",
        "content": user_text
    }));
    state.context.insert("thread_id".to_string(), json!(thread_id));
    state
}

// ============================================================================
// FEATURE 1: CYCLIC STATE GRAPH FSM ENGINE (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Standard 3-Node Cyclic Execution Loop with Loop Counter Termination
#[tokio::test]
async fn test_f1_tier1_cyclic_graph_loop_convergence() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("PLAN");

    graph.add_node("PLAN", |mut state: AgentState| async move {
        let count = state.context.get("cycle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) + 1;
        state.context.insert("cycle_count".to_string(), json!(count));
        state.current_node = "EXECUTE".to_string();
        Ok(state)
    });

    graph.add_node("EXECUTE", |mut state: AgentState| async move {
        state.current_node = "EVALUATE".to_string();
        Ok(state)
    });

    graph.add_node("EVALUATE", |mut state: AgentState| async move {
        let count = state.context.get("cycle_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if count >= 3 {
            state.current_node = "__END__".to_string();
        } else {
            state.current_node = "PLAN".to_string(); // Cycle back
        }
        Ok(state)
    });

    let initial_state = create_agent_state("Solve complex problem", "thread_f1_1");
    let result = graph.run(initial_state).await.expect("Graph run failed");

    assert_eq!(result.current_node, "__END__");
    assert_eq!(result.context.get("cycle_count").unwrap(), 3);
}

/// Tier 1.2: Dynamic Conditional Branching
#[tokio::test]
async fn test_f1_tier1_conditional_branching() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("ROUTER");

    graph.add_node("ROUTER", |mut state: AgentState| async move {
        let query = state.messages.first()
            .and_then(|m| m.get("content"))
            .and_then(|c| c.as_str())
            .unwrap_or("");
        
        if query.contains("calculate") {
            state.current_node = "MATH_TOOL".to_string();
        } else {
            state.current_node = "TEXT_CHAT".to_string();
        }
        Ok(state)
    });

    graph.add_node("MATH_TOOL", |mut state: AgentState| async move {
        state.context.insert("routed_to".to_string(), json!("math"));
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    graph.add_node("TEXT_CHAT", |mut state: AgentState| async move {
        state.context.insert("routed_to".to_string(), json!("chat"));
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    let state_math = create_agent_state("Please calculate 42 * 10", "thread_f1_2a");
    let res_math = graph.run(state_math).await.unwrap();
    assert_eq!(res_math.context.get("routed_to").unwrap(), "math");

    let state_chat = create_agent_state("Hello, how are you today?", "thread_f1_2b");
    let res_chat = graph.run(state_chat).await.unwrap();
    assert_eq!(res_chat.context.get("routed_to").unwrap(), "chat");
}

/// Tier 1.3: Static Edge Traversal (Fallback transitions)
#[tokio::test]
async fn test_f1_tier1_static_edge_transitions() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("A");
    graph.add_edge("A", "B");
    graph.add_edge("B", "C");

    graph.add_node("A", |mut state: AgentState| async move {
        state.context.insert("visited_a".to_string(), json!(true));
        Ok(state)
    });
    graph.add_node("B", |mut state: AgentState| async move {
        state.context.insert("visited_b".to_string(), json!(true));
        Ok(state)
    });
    graph.add_node("C", |mut state: AgentState| async move {
        state.context.insert("visited_c".to_string(), json!(true));
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    let res = graph.run(create_agent_state("Test traversal", "thread_f1_3")).await.unwrap();
    assert_eq!(res.context.get("visited_a").unwrap(), true);
    assert_eq!(res.context.get("visited_b").unwrap(), true);
    assert_eq!(res.context.get("visited_c").unwrap(), true);
}

/// Tier 1.4: State Accumulator & Message Append across nodes
#[tokio::test]
async fn test_f1_tier1_message_accumulator() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("STEP_1");
    graph.add_edge("STEP_1", "STEP_2");

    graph.add_node("STEP_1", |mut state: AgentState| async move {
        state.messages.push(json!({ "role": "assistant", "content": "Thought: Step 1 executed" }));
        Ok(state)
    });
    graph.add_node("STEP_2", |mut state: AgentState| async move {
        state.messages.push(json!({ "role": "assistant", "content": "Observation: Step 2 finished" }));
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    let res = graph.run(create_agent_state("Run pipeline", "thread_f1_4")).await.unwrap();
    assert_eq!(res.messages.len(), 3);
    assert_eq!(res.messages[1]["content"], "Thought: Step 1 executed");
    assert_eq!(res.messages[2]["content"], "Observation: Step 2 finished");
}

/// Tier 1.5: Direct __END__ short-circuit
#[tokio::test]
async fn test_f1_tier1_direct_end_short_circuit() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("FAST_EXIT");

    graph.add_node("FAST_EXIT", |mut state: AgentState| async move {
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    let res = graph.run(create_agent_state("Exit immediately", "thread_f1_5")).await.unwrap();
    assert_eq!(res.current_node, "__END__");
}

/// Tier 2.1: Missing Node Error Detection
#[tokio::test]
async fn test_f1_tier2_missing_node_error() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("NON_EXISTENT_NODE");

    let res = graph.run(create_agent_state("Error test", "thread_f1_err1")).await;
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("Node 'NON_EXISTENT_NODE' not found"));
}

/// Tier 2.2: Node Error Propagation
#[tokio::test]
async fn test_f1_tier2_node_error_propagation() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("FAULTY_NODE");

    graph.add_node("FAULTY_NODE", |_state: AgentState| async move {
        Err("Simulated hardware or timeout fault in graph node".to_string())
    });

    let res = graph.run(create_agent_state("Fail test", "thread_f1_err2")).await;
    assert!(res.is_err());
    assert_eq!(res.unwrap_err(), "Simulated hardware or timeout fault in graph node");
}

/// Tier 2.3: Self-Loop State Evolution
#[tokio::test]
async fn test_f1_tier2_self_loop_state_evolution() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("LOOP");
    graph.add_edge("LOOP", "LOOP");

    graph.add_node("LOOP", |mut state: AgentState| async move {
        let counter = state.context.get("iter")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) + 1;
        state.context.insert("iter".to_string(), json!(counter));
        if counter >= 5 {
            state.current_node = "__END__".to_string();
        }
        Ok(state)
    });

    let res = graph.run(create_agent_state("Loop 5 times", "thread_f1_loop")).await.unwrap();
    assert_eq!(res.context.get("iter").unwrap(), 5);
}

/// Tier 2.4: Concurrent Graph Execution on Independent States
#[tokio::test]
async fn test_f1_tier2_concurrent_graphs_isolated() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("WORKER");

    graph.add_node("WORKER", |mut state: AgentState| async move {
        let id = state.context.get("thread_id").and_then(|v| v.as_str()).unwrap_or_default().to_string();
        state.context.insert("processed_by".to_string(), json!(format!("worker_{}", id)));
        state.current_node = "__END__".to_string();
        Ok(state)
    });

    let graph_arc = Arc::new(graph);
    let mut handles = Vec::new();

    for i in 0..10 {
        let g = graph_arc.clone();
        let handle = tokio::spawn(async move {
            let thread_id = format!("thread_concurrent_{}", i);
            let state = create_agent_state(&format!("Task {}", i), &thread_id);
            let res = g.run(state).await.expect("Concurrent graph execution failed");
            assert_eq!(res.context.get("processed_by").unwrap(), &format!("worker_{}", thread_id));
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

/// Tier 2.5: Large Payload Cyclic Transformation
#[tokio::test]
async fn test_f1_tier2_large_payload_cyclic() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("TRANSFORM");
    graph.add_edge("TRANSFORM", "TRANSFORM");

    graph.add_node("TRANSFORM", |mut state: AgentState| async move {
        let step = state.context.get("step")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) + 1;
        state.context.insert("step".to_string(), json!(step));
        
        // Append a 1KB block per cycle
        let chunk = "x".repeat(1024);
        state.context.insert(format!("payload_chunk_{}", step), json!(chunk));

        if step >= 10 {
            state.current_node = "__END__".to_string();
        }
        Ok(state)
    });

    let res = graph.run(create_agent_state("Large payload test", "thread_f1_large")).await.unwrap();
    assert_eq!(res.context.get("step").unwrap(), 10);
    assert!(res.context.contains_key("payload_chunk_10"));
}

// ============================================================================
// FEATURE 2: IMMUTABLE STEP CHECKPOINTS (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Save Base Checkpoint and Retrieve State
#[tokio::test]
async fn test_f2_tier1_save_and_load_base_checkpoint() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);

    let state_0 = create_agent_state("User message step 0", "thread_f2_1");

    checkpointer.save_checkpoint(
        "thread_f2_1",
        0,
        &state_0,
        "START_NODE",
        None,
        None,
        Some("ACTIVE"),
    ).await.expect("Save base checkpoint failed");

    let loaded = checkpointer.load_checkpoint("thread_f2_1", 0)
        .await
        .expect("Load checkpoint failed")
        .expect("Checkpoint not found");

    assert_eq!(loaded.context.get("thread_id").unwrap(), "thread_f2_1");
    assert_eq!(loaded.messages[0]["content"], "User message step 0");
}

/// Tier 1.2: RFC 6902 JSON Patch Diff Generation & Application
#[tokio::test]
async fn test_f2_tier1_rfc6902_diff_and_patch() {
    let base = json!({
        "messages": [{ "role": "user", "content": "Hello" }],
        "context": { "key1": "val1" },
        "current_node": "START"
    });

    let updated = json!({
        "messages": [
            { "role": "user", "content": "Hello" },
            { "role": "assistant", "content": "Hi there!" }
        ],
        "context": { "key1": "val1", "key2": "val2" },
        "current_node": "THINK"
    });

    let patch = generate_json_patch(&base, &updated);
    assert!(!patch.is_empty(), "Patch should contain diff operations");

    let reconstructed = apply_json_patch(&base, &patch).expect("Apply patch failed");
    assert_eq!(reconstructed, updated);
}

/// Tier 1.3: Sequential Differential Checkpoints Reconstruction
#[tokio::test]
async fn test_f2_tier1_multi_step_differential_reconstruction() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f2_diff_seq";

    let mut state_0 = AgentState::default();
    state_0.context.insert("step".to_string(), json!(0));
    state_0.context.insert("data".to_string(), json!(["initial"]));

    // Step 0: Base
    checkpointer.save_checkpoint(thread, 0, &state_0, "NODE_0", None, None, Some("ACTIVE")).await.unwrap();

    // Step 1: Add item
    let mut state_1 = state_0.clone();
    state_1.context.insert("step".to_string(), json!(1));
    state_1.context.insert("data".to_string(), json!(["initial", "second"]));
    let base_val = serde_json::to_value(&state_0).unwrap();
    let s1_val = serde_json::to_value(&state_1).unwrap();
    let patch_1 = generate_json_patch(&base_val, &s1_val);
    let diff_1 = serde_json::to_string(&patch_1).unwrap();
    checkpointer.save_checkpoint(thread, 1, &state_1, "NODE_1", Some(&diff_1), None, Some("ACTIVE")).await.unwrap();

    // Step 2: Add third item
    let mut state_2 = state_1.clone();
    state_2.context.insert("step".to_string(), json!(2));
    state_2.context.insert("data".to_string(), json!(["initial", "second", "third"]));
    let s2_val = serde_json::to_value(&state_2).unwrap();
    let patch_2 = generate_json_patch(&s1_val, &s2_val);
    let diff_2 = serde_json::to_string(&patch_2).unwrap();
    checkpointer.save_checkpoint(thread, 2, &state_2, "NODE_2", Some(&diff_2), None, Some("ACTIVE")).await.unwrap();

    // Load step 2
    let loaded_2 = checkpointer.load_checkpoint(thread, 2).await.unwrap().unwrap();
    assert_eq!(loaded_2.context.get("step").unwrap(), 2);
    assert_eq!(loaded_2.context.get("data").unwrap(), &json!(["initial", "second", "third"]));
}

/// Tier 1.4: Load Latest Checkpoint
#[tokio::test]
async fn test_f2_tier1_load_latest_checkpoint() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f2_latest";

    for step in 0..=5 {
        let mut state = AgentState::default();
        state.context.insert("counter".to_string(), json!(step));
        checkpointer.save_checkpoint(thread, step, &state, &format!("NODE_{}", step), None, None, Some("ACTIVE")).await.unwrap();
    }

    let (latest_step, latest_state) = checkpointer.load_latest(thread).await.unwrap().unwrap();
    assert_eq!(latest_step, 5);
    assert_eq!(latest_state.context.get("counter").unwrap(), 5);
}

/// Tier 1.5: Checkpoint History Metadata Listing
#[tokio::test]
async fn test_f2_tier1_list_checkpoint_history() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f2_hist";

    for step in 0..3 {
        let mut state = AgentState::default();
        state.context.insert("s".to_string(), json!(step));
        checkpointer.save_checkpoint(thread, step, &state, &format!("NODE_{}", step), None, None, Some("ACTIVE")).await.unwrap();
    }

    let history = checkpointer.list_checkpoints(thread).await.unwrap();
    assert_eq!(history.len(), 3);
    assert_eq!(history[0].step, 0);
    assert_eq!(history[1].step, 1);
    assert_eq!(history[2].step, 2);
}

/// Tier 2.1: Non-Existent Checkpoint Returns None
#[tokio::test]
async fn test_f2_tier2_non_existent_returns_none() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);

    let res = checkpointer.load_checkpoint("unknown_thread_id", 0).await.unwrap();
    assert!(res.is_none());

    let latest = checkpointer.load_latest("unknown_thread_id").await.unwrap();
    assert!(latest.is_none());
}

/// Tier 2.2: Patch with Array Removal & Key Deletion
#[tokio::test]
async fn test_f2_tier2_patch_array_removal_and_deletion() {
    let from = json!({
        "items": [1, 2, 3, 4, 5],
        "obsolete_key": "to_be_deleted",
        "nested": { "a": 10, "b": 20 }
    });
    let to = json!({
        "items": [1, 2, 3],
        "nested": { "a": 10 }
    });

    let patch = generate_json_patch(&from, &to);
    let result = apply_json_patch(&from, &patch).unwrap();
    assert_eq!(result, to);
}

/// Tier 2.3: Complex Nested Structure Diffing
#[tokio::test]
async fn test_f2_tier2_deeply_nested_diff() {
    let from = json!({
        "l1": { "l2": { "l3": { "val": "old", "arr": [1, 2] } } }
    });
    let to = json!({
        "l1": { "l2": { "l3": { "val": "new", "arr": [1, 2, 3], "extra": true } } }
    });

    let patch = generate_json_patch(&from, &to);
    let reconstructed = apply_json_patch(&from, &patch).unwrap();
    assert_eq!(reconstructed, to);
}

/// Tier 2.4: Checkpoint Status Updates (Suspended / Completed / Failed)
#[tokio::test]
async fn test_f2_tier2_status_updates() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f2_status";

    let mut state = AgentState::default();
    state.context.insert("status_test".to_string(), json!(true));
    checkpointer.save_checkpoint(thread, 0, &state, "NODE", None, None, Some("SUSPENDED")).await.unwrap();

    let history = checkpointer.list_checkpoints(thread).await.unwrap();
    assert_eq!(history[0].status, "SUSPENDED");

    checkpointer.save_checkpoint(thread, 0, &state, "NODE", None, None, Some("COMPLETED")).await.unwrap();
    let history2 = checkpointer.list_checkpoints(thread).await.unwrap();
    assert_eq!(history2[0].status, "COMPLETED");
}

/// Tier 2.5: Concurrent Thread Checkpointing Isolation
#[tokio::test]
async fn test_f2_tier2_concurrent_checkpoint_isolation() {
    let mut handles = Vec::new();

    for i in 0..10 {
        let handle = tokio::spawn(async move {
            let (db, crypto) = setup_test_db();
            let cp = SqliteCheckpointer::new(db, crypto);
            let thread = format!("thread_iso_{}", i);
            for step in 0..5 {
                let mut state = AgentState::default();
                state.context.insert("thread".to_string(), json!(i));
                state.context.insert("step".to_string(), json!(step));
                cp.save_checkpoint(&thread, step, &state, "NODE", None, None, Some("ACTIVE")).await.unwrap();
            }
            let (last_step, last_state) = cp.load_latest(&thread).await.unwrap().unwrap();
            assert_eq!(last_step, 4);
            assert_eq!(last_state.context.get("thread").unwrap(), i);
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ============================================================================
// FEATURE 3: HUMAN-IN-THE-LOOP (HITL) GATING (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Create Approval Context and Verify Non-Expired
#[test]
fn test_f3_tier1_approval_context_creation() {
    let ctx = ApprovalContext::new(
        "act_rm_rf",
        "bash_execute",
        json!({ "command": "rm -rf target/debug" }),
        "Destructive file removal requires confirmation",
        300,
    );

    assert_eq!(ctx.action_id, "act_rm_rf");
    assert_eq!(ctx.tool_name, "bash_execute");
    assert_eq!(ctx.timeout_secs, 300);
    assert!(!ctx.is_expired_now());
}

/// Tier 1.2: Approval Context Expiration Calculation
#[test]
fn test_f3_tier1_approval_context_expired() {
    let ctx = ApprovalContext::new(
        "act_timeout",
        "drop_table",
        json!({ "table": "users" }),
        "Dangerous drop table",
        10, // 10 seconds
    );

    // Current time + 11 seconds is expired
    let expired_time_ms = ctx.created_at + 11_000;
    assert!(ctx.is_expired(expired_time_ms));

    // Current time + 5 seconds is NOT expired
    let valid_time_ms = ctx.created_at + 5_000;
    assert!(!ctx.is_expired(valid_time_ms));
}

/// Tier 1.3: Approval Decision Variants Serialization & Deserialization
#[test]
fn test_f3_tier1_approval_decisions_serde() {
    let approved = ApprovalDecision::Approved { modified_args: None };
    let json_approved = serde_json::to_string(&approved).unwrap();
    let de_approved: ApprovalDecision = serde_json::from_str(&json_approved).unwrap();
    assert_eq!(de_approved, approved);

    let approved_mod = ApprovalDecision::Approved {
        modified_args: Some(json!({ "force": false })),
    };
    let json_mod = serde_json::to_string(&approved_mod).unwrap();
    let de_mod: ApprovalDecision = serde_json::from_str(&json_mod).unwrap();
    assert_eq!(de_mod, approved_mod);

    let rejected = ApprovalDecision::Rejected {
        reason: Some("Operation too risky".to_string()),
    };
    let json_rej = serde_json::to_string(&rejected).unwrap();
    let de_rej: ApprovalDecision = serde_json::from_str(&json_rej).unwrap();
    assert_eq!(de_rej, rejected);

    let timed_out = ApprovalDecision::TimedOut;
    let json_to = serde_json::to_string(&timed_out).unwrap();
    let de_to: ApprovalDecision = serde_json::from_str(&json_to).unwrap();
    assert_eq!(de_to, timed_out);
}

/// Tier 1.4: Checkpoint Status String Conversions
#[test]
fn test_f3_tier1_checkpoint_status_conversions() {
    assert_eq!(CheckpointStatus::Active.as_str(), "ACTIVE");
    assert_eq!(CheckpointStatus::Suspended.as_str(), "SUSPENDED");
    assert_eq!(CheckpointStatus::Completed.as_str(), "COMPLETED");
    assert_eq!(CheckpointStatus::Failed.as_str(), "FAILED");

    assert_eq!(CheckpointStatus::from_str("ACTIVE"), CheckpointStatus::Active);
    assert_eq!(CheckpointStatus::from_str("suspended"), CheckpointStatus::Suspended);
    assert_eq!(CheckpointStatus::from_str("Completed"), CheckpointStatus::Completed);
    assert_eq!(CheckpointStatus::from_str("failed"), CheckpointStatus::Failed);
    assert_eq!(CheckpointStatus::from_str("unknown"), CheckpointStatus::Active);
}

/// Tier 1.5: HITL Yield Flow in Checkpoint Database
#[tokio::test]
async fn test_f3_tier1_hitl_checkpoint_suspension_flow() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_hitl_flow";

    // Step 0: Active thought
    let mut state_0 = AgentState::default();
    state_0.context.insert("plan".to_string(), json!("Delete directory"));
    checkpointer.save_checkpoint(thread, 0, &state_0, "PLAN_NODE", None, None, Some("ACTIVE")).await.unwrap();

    // Step 1: Suspended awaiting approval
    let ctx = ApprovalContext::new("act_1", "fs_delete", json!({ "path": "/data" }), "High risk", 60);
    let mut state_1 = AgentState::default();
    state_1.context.insert("pending_approval".to_string(), json!(ctx));
    checkpointer.save_checkpoint(thread, 1, &state_1, "HITL_GATE", None, None, Some("SUSPENDED")).await.unwrap();

    let history = checkpointer.list_checkpoints(thread).await.unwrap();
    assert_eq!(history[1].status, "SUSPENDED");

    // Resume after approval
    checkpointer.save_checkpoint(thread, 1, &state_1, "HITL_GATE", None, None, Some("COMPLETED")).await.unwrap();
    let history_resumed = checkpointer.list_checkpoints(thread).await.unwrap();
    assert_eq!(history_resumed[1].status, "COMPLETED");
}

/// Tier 2.1: Zero Timeout Context Immediately Expired
#[test]
fn test_f3_tier2_zero_timeout_immediate_expiry() {
    let ctx = ApprovalContext::new("act_0", "test", json!({}), "Zero timeout", 0);
    let next_ms = ctx.created_at + 1;
    assert!(ctx.is_expired(next_ms));
}

/// Tier 2.2: Millisecond Boundary Expiration Precision
#[test]
fn test_f3_tier2_exact_boundary_expiry() {
    let ctx = ApprovalContext::new("act_bound", "test", json!({}), "Boundary test", 1);
    let deadline_ms = ctx.created_at + 1000;
    assert!(!ctx.is_expired(deadline_ms)); // exactly at deadline is valid
    assert!(ctx.is_expired(deadline_ms + 1)); // 1ms after deadline is expired
}

/// Tier 2.3: Multiple Parallel Approval Requests Isolation
#[tokio::test]
async fn test_f3_tier2_parallel_approval_requests() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);

    for i in 0..5 {
        let thread = format!("thread_hitl_parallel_{}", i);
        let ctx = ApprovalContext::new(format!("act_{}", i), "tool", json!({ "id": i }), "reason", 100);
        let mut state = AgentState::default();
        state.context.insert("approval".to_string(), json!(ctx));
        checkpointer.save_checkpoint(&thread, 0, &state, "GATE", None, None, Some("SUSPENDED")).await.unwrap();
    }

    for i in 0..5 {
        let thread = format!("thread_hitl_parallel_{}", i);
        let hist = checkpointer.list_checkpoints(&thread).await.unwrap();
        assert_eq!(hist[0].status, "SUSPENDED");
    }
}

/// Tier 2.4: Approval Decision Payload Modification Parsing
#[test]
fn test_f3_tier2_approval_decision_payload_modifications() {
    let raw_approved = json!({
        "type": "Approved",
        "payload": {
            "modified_args": {
                "safe_mode": true,
                "max_items": 10
            }
        }
    });

    let decision: ApprovalDecision = serde_json::from_value(raw_approved).unwrap();
    match decision {
        ApprovalDecision::Approved { modified_args } => {
            let args = modified_args.expect("Modified args should be present");
            assert_eq!(args["safe_mode"], true);
            assert_eq!(args["max_items"], 10);
        }
        _ => panic!("Expected Approved decision"),
    }
}

/// Tier 2.5: Rejection with Error Reason Serialization
#[test]
fn test_f3_tier2_rejection_with_reason() {
    let raw_rejected = json!({
        "type": "Rejected",
        "payload": {
            "reason": "User denied access to sensitive directory"
        }
    });

    let decision: ApprovalDecision = serde_json::from_value(raw_rejected).unwrap();
    match decision {
        ApprovalDecision::Rejected { reason } => {
            assert_eq!(reason.unwrap(), "User denied access to sensitive directory");
        }
        _ => panic!("Expected Rejected decision"),
    }
}

// ============================================================================
// FEATURE 4: TIME-TRAVEL RECOVERY & REPLAY CACHE (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Time-Travel State Rewind to Arbitrary Historical Step
#[tokio::test]
async fn test_f4_tier1_time_travel_rewind_step() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_time_travel_1";

    for step in 0..=4 {
        let mut state = AgentState::default();
        state.context.insert("stage".to_string(), json!(format!("stage_{}", step)));
        state.context.insert("step_num".to_string(), json!(step));
        checkpointer.save_checkpoint(thread, step, &state, &format!("NODE_{}", step), None, None, Some("ACTIVE")).await.unwrap();
    }

    // Rewind to step 2
    let step_2_state = checkpointer.load_checkpoint(thread, 2).await.unwrap().expect("Step 2 not found");
    assert_eq!(step_2_state.context.get("stage").unwrap(), "stage_2");
    assert_eq!(step_2_state.context.get("step_num").unwrap(), 2);

    // Rewind to step 0
    let step_0_state = checkpointer.load_checkpoint(thread, 0).await.unwrap().expect("Step 0 not found");
    assert_eq!(step_0_state.context.get("stage").unwrap(), "stage_0");
    assert_eq!(step_0_state.context.get("step_num").unwrap(), 0);
}

/// Tier 1.2: Tool Output Replay Cache Storage and Retrieval
#[tokio::test]
async fn test_f4_tier1_tool_output_cache_store_and_retrieve() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_replay_cache_1";

    let state = AgentState::default();
    checkpointer.save_checkpoint(thread, 1, &state, "TOOL_NODE", None, None, Some("ACTIVE")).await.unwrap();

    let tool_output = json!({
        "status": "success",
        "result_data": [10, 20, 30]
    });

    checkpointer.record_tool_output(thread, 1, "fetch_weather", &tool_output).await.unwrap();

    let cached = checkpointer.get_cached_tool_output(thread, 1, "fetch_weather")
        .await
        .unwrap()
        .expect("Cached output should exist");

    assert_eq!(cached["status"], "success");
    assert_eq!(cached["result_data"], json!([10, 20, 30]));
}

/// Tier 1.3: Cache Hit Bypasses Tool Re-execution
#[tokio::test]
async fn test_f4_tier1_replay_cache_hit_bypasses_tool() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_cache_hit";

    let state = AgentState::default();
    checkpointer.save_checkpoint(thread, 1, &state, "NODE", None, None, Some("ACTIVE")).await.unwrap();

    let tool_output = json!({ "computed_val": 999 });
    checkpointer.record_tool_output(thread, 1, "expensive_compute", &tool_output).await.unwrap();

    // Check if output is in cache
    let cached = checkpointer.get_cached_tool_output(thread, 1, "expensive_compute").await.unwrap();
    assert!(cached.is_some());
    assert_eq!(cached.unwrap()["computed_val"], 999);

    // Non-cached tool returns None
    let not_cached = checkpointer.get_cached_tool_output(thread, 1, "other_tool").await.unwrap();
    assert!(not_cached.is_none());
}

/// Tier 1.4: Multiple Tool Outputs Cached for Same Step
#[tokio::test]
async fn test_f4_tier1_multiple_tools_same_step() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_multi_tools_step";

    let state = AgentState::default();
    checkpointer.save_checkpoint(thread, 2, &state, "NODE", None, None, Some("ACTIVE")).await.unwrap();

    checkpointer.record_tool_output(thread, 2, "tool_a", &json!({ "a": 1 })).await.unwrap();
    checkpointer.record_tool_output(thread, 2, "tool_b", &json!({ "b": 2 })).await.unwrap();

    let out_a = checkpointer.get_cached_tool_output(thread, 2, "tool_a").await.unwrap().unwrap();
    let out_b = checkpointer.get_cached_tool_output(thread, 2, "tool_b").await.unwrap().unwrap();

    assert_eq!(out_a["a"], 1);
    assert_eq!(out_b["b"], 2);
}

/// Tier 1.5: Branching / Divergence after Time-Travel Rewind
#[tokio::test]
async fn test_f4_tier1_time_travel_branch_divergence() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);

    let original_thread = "thread_main_timeline";
    for step in 0..=3 {
        let mut state = AgentState::default();
        state.context.insert("val".to_string(), json!(format!("original_step_{}", step)));
        checkpointer.save_checkpoint(original_thread, step, &state, "NODE", None, None, Some("ACTIVE")).await.unwrap();
    }

    // Rewind to step 1 and fork into new thread
    let step_1 = checkpointer.load_checkpoint(original_thread, 1).await.unwrap().unwrap();
    let forked_thread = "thread_forked_timeline";

    checkpointer.save_checkpoint(forked_thread, 1, &step_1, "FORK_NODE", None, None, Some("ACTIVE")).await.unwrap();
    let mut forked_step_2 = AgentState::default();
    forked_step_2.context.insert("val".to_string(), json!("alternate_future_step_2"));
    checkpointer.save_checkpoint(forked_thread, 2, &forked_step_2, "FORK_NODE", None, None, Some("ACTIVE")).await.unwrap();

    // Original timeline step 2 is unchanged
    let orig_2 = checkpointer.load_checkpoint(original_thread, 2).await.unwrap().unwrap();
    assert_eq!(orig_2.context.get("val").unwrap(), "original_step_2");

    // Forked timeline step 2 has alternate value
    let fork_2 = checkpointer.load_checkpoint(forked_thread, 2).await.unwrap().unwrap();
    assert_eq!(fork_2.context.get("val").unwrap(), "alternate_future_step_2");
}

/// Tier 2.1: Rewind Beyond Available Step History
#[tokio::test]
async fn test_f4_tier2_rewind_out_of_bounds() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f4_oob";

    checkpointer.save_checkpoint(thread, 0, &AgentState::default(), "NODE", None, None, Some("ACTIVE")).await.unwrap();
    checkpointer.save_checkpoint(thread, 1, &AgentState::default(), "NODE", None, None, Some("ACTIVE")).await.unwrap();

    let res = checkpointer.load_checkpoint(thread, 999).await.unwrap();
    assert!(res.is_none());
}

/// Tier 2.2: Tool Output Cache for Non-Existent Step Returns None
#[tokio::test]
async fn test_f4_tier2_tool_output_non_existent_step() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f4_no_step";

    let res = checkpointer.get_cached_tool_output(thread, 50, "tool").await.unwrap();
    assert!(res.is_none());
}

/// Tier 2.3: Overwriting Existing Tool Output in Replay Cache
#[tokio::test]
async fn test_f4_tier2_overwrite_tool_output_cache() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f4_overwrite";

    checkpointer.save_checkpoint(thread, 0, &AgentState::default(), "NODE", None, None, Some("ACTIVE")).await.unwrap();
    checkpointer.record_tool_output(thread, 0, "calc", &json!({ "res": 1 })).await.unwrap();
    checkpointer.record_tool_output(thread, 0, "calc", &json!({ "res": 2 })).await.unwrap();

    let cached = checkpointer.get_cached_tool_output(thread, 0, "calc").await.unwrap().unwrap();
    assert_eq!(cached["res"], 2);
}

/// Tier 2.4: High-Frequency Sequential Rewinds (Stress)
#[tokio::test]
async fn test_f4_tier2_high_frequency_rewinds() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f4_stress_rewind";

    for s in 0..20 {
        let mut state = AgentState::default();
        state.context.insert("s".to_string(), json!(s));
        checkpointer.save_checkpoint(thread, s, &state, "NODE", None, None, Some("ACTIVE")).await.unwrap();
    }

    for _ in 0..50 {
        let target_step = 10;
        let loaded = checkpointer.load_checkpoint(thread, target_step).await.unwrap().unwrap();
        assert_eq!(loaded.context.get("s").unwrap(), target_step);
    }
}

/// Tier 2.5: Complex Tool Output Payload Caching & Verification
#[tokio::test]
async fn test_f4_tier2_complex_tool_payload_cache() {
    let (db, crypto) = setup_test_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "thread_f4_complex_tool";

    checkpointer.save_checkpoint(thread, 0, &AgentState::default(), "NODE", None, None, Some("ACTIVE")).await.unwrap();

    let complex_output = json!({
        "nested": {
            "array": [1, 2, { "inner": "val" }],
            "unicode": "Tiếng Việt có dấu và ký tự đặc biệt: 🔥🚀",
            "escaped_quote": "This contains \"quotes\" and \n newlines"
        }
    });

    checkpointer.record_tool_output(thread, 0, "complex_tool", &complex_output).await.unwrap();
    let retrieved = checkpointer.get_cached_tool_output(thread, 0, "complex_tool").await.unwrap().unwrap();

    assert_eq!(retrieved, complex_output);
}
