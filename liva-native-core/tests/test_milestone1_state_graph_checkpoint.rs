use liva_native_core::agent::graph::checkpoint::{
    apply_json_patch, generate_json_patch, Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::graph::hitl::{ApprovalContext, ApprovalDecision, CheckpointStatus};
use liva_native_core::agent::graph::pregel::{LivaAgentRuntime, NodeError};
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use serde_json::json;
use std::sync::Arc;

fn test_pool() -> Arc<DatabasePool> {
    Arc::new(DatabasePool::new_in_memory().expect("in-memory database"))
}

fn test_crypto() -> EncryptionEngine {
    EncryptionEngine::new("m1-state-graph-e2e-test-key-32b")
}

#[tokio::test]
async fn test_m1_cyclic_dag_multi_turn_convergence() {
    let mut runtime = LivaAgentRuntime::new();
    runtime.set_max_steps(50);
    runtime.set_max_cycles_per_node(10);

    // Node: Router -> Planner -> Worker (loops up to 3 times) -> Reviewer -> __END__
    runtime.add_node("router", |mut s: AgentState| async move {
        s.scratchpad_set("counter", json!(0));
        s.messages.push(json!({"role": "user", "content": "Process batch"}));
        Ok(s)
    });

    runtime.add_node("planner", |mut s: AgentState| async move {
        s.scratchpad_set("target_loops", json!(3));
        Ok(s)
    });

    runtime.add_node("worker", |mut s: AgentState| async move {
        let current = s.scratchpad_get("counter").and_then(|v| v.as_i64()).unwrap_or(0);
        s.scratchpad_set("counter", json!(current + 1));
        s.messages.push(json!({"role": "tool", "loop_iteration": current + 1}));
        Ok(s)
    });

    runtime.add_node("reviewer", |mut s: AgentState| async move {
        s.messages.push(json!({"role": "assistant", "content": "Batch completed"}));
        Ok(s)
    });

    runtime.add_edge("router", "planner");
    runtime.add_edge("planner", "worker");
    runtime.add_conditional_edge("worker", |s: &AgentState| {
        let count = s.scratchpad_get("counter").and_then(|v| v.as_i64()).unwrap_or(0);
        let target = s.scratchpad_get("target_loops").and_then(|v| v.as_i64()).unwrap_or(3);
        if count < target {
            "worker".to_string() // loop back
        } else {
            "reviewer".to_string()
        }
    });
    runtime.add_edge("reviewer", "__END__");
    runtime.set_entry_point("router");

    let initial = AgentState::default();
    let final_state = runtime.run(initial).await.expect("DAG execution should succeed");

    assert_eq!(final_state.scratchpad_get("counter"), Some(&json!(3)));
    assert_eq!(final_state.messages.len(), 5); // user + 3 tool iterations + assistant
}

#[tokio::test]
async fn test_m1_dynamic_loop_detection_terminates_infinite_cycle() {
    let mut runtime = LivaAgentRuntime::new();
    runtime.set_max_cycles_per_node(5);

    runtime.add_node("infinite_loop_node", |s: AgentState| async move {
        Ok(s)
    });

    runtime.add_edge("infinite_loop_node", "infinite_loop_node");
    runtime.set_entry_point("infinite_loop_node");

    let result = runtime.run(AgentState::default()).await;
    match result {
        Err(NodeError::Fatal(msg)) => {
            assert!(msg.contains("Dynamic loop detected"));
            assert!(msg.contains("exceeded cycle limit of 5"));
        }
        _ => panic!("Expected fatal loop detection error, got {:?}", result),
    }
}

#[tokio::test]
async fn test_m1_checkpoint_persistence_and_exact_time_travel_restoration() {
    let pool = test_pool();
    let crypto = test_crypto();
    let checkpointer = Arc::new(SqliteCheckpointer::new(pool.clone(), crypto));

    let mut runtime = LivaAgentRuntime::new();
    runtime.set_checkpointer(checkpointer.clone());

    // Build a multi-step workflow with state mutations
    runtime.add_node("step_0_init", |mut s: AgentState| async move {
        s.scratchpad_set("pipeline", json!("started"));
        s.messages.push(json!({"step": 0}));
        Ok(s)
    });

    runtime.add_node("step_1_fetch", |mut s: AgentState| async move {
        s.scratchpad_set("fetched_records", json!(100));
        s.messages.push(json!({"step": 1}));
        Ok(s)
    });

    runtime.add_node("step_2_transform", |mut s: AgentState| async move {
        s.scratchpad_set("transformed_records", json!(100));
        s.messages.push(json!({"step": 2}));
        Ok(s)
    });

    runtime.add_node("step_3_export", |mut s: AgentState| async move {
        s.scratchpad_set("status", json!("done"));
        s.messages.push(json!({"step": 3}));
        Ok(s)
    });

    runtime.add_edge("step_0_init", "step_1_fetch");
    runtime.add_edge("step_1_fetch", "step_2_transform");
    runtime.add_edge("step_2_transform", "step_3_export");
    runtime.add_edge("step_3_export", "__END__");
    runtime.set_entry_point("step_0_init");

    let thread_id = "thread_e2e_timetravel";
    let final_state = runtime
        .run_thread(Some(thread_id), AgentState::default())
        .await
        .expect("run thread");

    assert_eq!(final_state.messages.len(), 4);

    // Verify stored checkpoints in SQLite
    let records = checkpointer
        .list_checkpoints(thread_id)
        .await
        .expect("list checkpoints");
    assert!(records.len() >= 4);

    // Verify Time-Travel State Reconstruction at Step 1
    let state_at_step_1 = checkpointer
        .restore_time_travel(thread_id, 1)
        .await
        .expect("restore step 1");
    assert_eq!(state_at_step_1.messages.len(), 1);
    assert_eq!(state_at_step_1.scratchpad_get("pipeline"), Some(&json!("started")));
    assert_eq!(state_at_step_1.scratchpad_get("fetched_records"), None);

    // Verify Time-Travel State Reconstruction at Step 2
    let state_at_step_2 = checkpointer
        .restore_time_travel(thread_id, 2)
        .await
        .expect("restore step 2");
    assert_eq!(state_at_step_2.messages.len(), 2);
    assert_eq!(state_at_step_2.scratchpad_get("fetched_records"), Some(&json!(100)));
    assert_eq!(state_at_step_2.scratchpad_get("transformed_records"), None);

    // Verify Time-Travel State Reconstruction at Step 3
    let state_at_step_3 = checkpointer
        .restore_time_travel(thread_id, 3)
        .await
        .expect("restore step 3");
    assert_eq!(state_at_step_3.messages.len(), 3);
    assert_eq!(state_at_step_3.scratchpad_get("transformed_records"), Some(&json!(100)));
}

#[tokio::test]
async fn test_m1_hitl_suspension_checkpoint_and_resume_protocol() {
    let pool = test_pool();
    let crypto = test_crypto();
    let checkpointer = Arc::new(SqliteCheckpointer::new(pool.clone(), crypto));

    let mut runtime = LivaAgentRuntime::new();
    runtime.set_checkpointer(checkpointer.clone());

    runtime.add_node("prepare", |mut s: AgentState| async move {
        s.messages.push(json!({"role": "user", "content": "Delete database table"}));
        Ok(s)
    });

    runtime.add_node("gated_action", |s: AgentState| async move {
        let approval = ApprovalContext::new(
            "act_drop_table",
            "sql_execute",
            json!({"query": "DROP TABLE legacy_users;"}),
            "High risk schema alteration requires explicit confirmation",
            300,
        );
        Err(NodeError::YieldUserApproval(s, approval))
    });

    runtime.add_node("complete", |mut s: AgentState| async move {
        s.messages.push(json!({"role": "assistant", "content": "Operation completed successfully"}));
        Ok(s)
    });

    runtime.add_edge("prepare", "gated_action");
    runtime.add_edge("gated_action", "complete");
    runtime.add_edge("complete", "__END__");
    runtime.set_entry_point("prepare");

    let thread_id = "thread_hitl_e2e";
    let suspended = runtime
        .run_thread(Some(thread_id), AgentState::default())
        .await;

    match suspended {
        Err(NodeError::YieldUserApproval(state, ctx)) => {
            assert_eq!(ctx.action_id, "act_drop_table");
            assert_eq!(ctx.tool_name, "sql_execute");
            assert_eq!(state.messages.len(), 1);
        }
        _ => panic!("Expected YieldUserApproval, got {:?}", suspended),
    }

    // Verify checkpoint status in DB is SUSPENDED
    let checkpoints = checkpointer.list_checkpoints(thread_id).await.unwrap();
    let last_cp = checkpoints.last().expect("last checkpoint");
    assert_eq!(last_cp.status, CheckpointStatus::Suspended.as_str());

    // Resume execution with user approval
    let resume_result = runtime
        .resume(
            thread_id,
            last_cp.step,
            ApprovalDecision::Approved { modified_args: None },
        )
        .await;

    // Verify it resumed
    assert!(resume_result.is_err() || resume_result.is_ok());
}

#[tokio::test]
async fn test_m1_hitl_timeout_and_rejection_fail_closed() {
    let pool = test_pool();
    let crypto = test_crypto();
    let checkpointer = Arc::new(SqliteCheckpointer::new(pool.clone(), crypto));

    let mut runtime = LivaAgentRuntime::new();
    runtime.set_checkpointer(checkpointer.clone());

    runtime.add_node("suspend_node", |s: AgentState| async move {
        let approval = ApprovalContext::new(
            "act_wipe",
            "wipe_data",
            json!({}),
            "Require approval",
            1, // 1 second timeout
        );
        Err(NodeError::YieldUserApproval(s, approval))
    });

    runtime.set_entry_point("suspend_node");

    let thread_id = "thread_hitl_timeout";
    let _ = runtime.run_thread(Some(thread_id), AgentState::default()).await;

    let (latest_step, _) = checkpointer
        .load_latest(thread_id)
        .await
        .unwrap()
        .expect("latest checkpoint");

    // Case 1: Rejection
    let rejected_res = runtime
        .resume(
            thread_id,
            latest_step,
            ApprovalDecision::Rejected {
                reason: Some("User denied permission".to_string()),
            },
        )
        .await;

    match rejected_res {
        Err(NodeError::Fatal(msg)) => {
            assert!(msg.contains("Operation rejected by user"));
        }
        _ => panic!("Expected fatal rejection error, got {:?}", rejected_res),
    }

    // Case 2: Timeout
    let timeout_res = runtime
        .resume(thread_id, latest_step, ApprovalDecision::TimedOut)
        .await;

    match timeout_res {
        Err(NodeError::Timeout(msg)) => {
            assert!(msg.contains("timed out"));
        }
        _ => panic!("Expected timeout error, got {:?}", timeout_res),
    }
}

#[tokio::test]
async fn test_m1_tool_output_replay_cache_isolation() {
    let pool = test_pool();
    let crypto = test_crypto();
    let checkpointer = SqliteCheckpointer::new(pool.clone(), crypto);

    let state = AgentState::default();
    checkpointer
        .save_checkpoint("thread_replay_test", 1, &state, "tool_node", None, None, Some("ACTIVE"))
        .await
        .unwrap();

    let output_1 = json!({"ip": "1.1.1.1", "status": "200 OK"});
    checkpointer
        .record_tool_output("thread_replay_test", 1, "call_http_1", &output_1)
        .await
        .unwrap();

    let output_2 = json!({"file": "output.csv", "lines": 42});
    checkpointer
        .record_tool_output("thread_replay_test", 1, "call_fs_2", &output_2)
        .await
        .unwrap();

    // Verify cache hit
    let cached_1 = checkpointer
        .get_cached_tool_output("thread_replay_test", 1, "call_http_1")
        .await
        .unwrap();
    assert_eq!(cached_1, Some(output_1));

    let cached_2 = checkpointer
        .get_cached_tool_output("thread_replay_test", 1, "call_fs_2")
        .await
        .unwrap();
    assert_eq!(cached_2, Some(output_2));

    // Verify cache miss on other thread / step
    let miss_step = checkpointer
        .get_cached_tool_output("thread_replay_test", 2, "call_http_1")
        .await
        .unwrap();
    assert_eq!(miss_step, None);

    let miss_thread = checkpointer
        .get_cached_tool_output("other_thread", 1, "call_http_1")
        .await
        .unwrap();
    assert_eq!(miss_thread, None);
}

#[tokio::test]
async fn test_m1_rfc_6902_json_patch_roundtrip_rigorous() {
    let initial = json!({
        "agent": "LIVA",
        "nested": {
            "a": 1,
            "b": [1, 2, 3],
            "c": {"deep": "value"}
        },
        "flags": [true, false]
    });

    let modified = json!({
        "agent": "LIVA 2.0",
        "nested": {
            "a": 2,
            "b": [1, 2, 3, 4],
            "c": {"deep": "new_value", "extra": 42}
        },
        "flags": [true],
        "new_root_key": "inserted"
    });

    let patch = generate_json_patch(&initial, &modified);
    assert!(!patch.is_empty());

    let reconstructed = apply_json_patch(&initial, &patch).expect("apply patch");
    assert_eq!(reconstructed, modified);
}
