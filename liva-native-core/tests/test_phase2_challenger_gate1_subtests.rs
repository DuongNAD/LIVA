//! Specialized tests targeting State Graph, Checkpoint Time-Travel, and HITL Gating

use liva_native_core::agent::graph::{
    generate_json_patch, ApprovalContext, ApprovalDecision, Checkpointer, LivaAgentRuntime,
    NodeError, SqliteCheckpointer,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use serde_json::json;
use std::sync::Arc;

fn pool() -> Arc<DatabasePool> {
    Arc::new(DatabasePool::new_in_memory().expect("in-memory db"))
}

fn crypto() -> EncryptionEngine {
    EncryptionEngine::new("gate1-subtests-key-32-bytes-long")
}

#[tokio::test]
async fn test_state_graph_skip_steps_and_time_travel() {
    let db = pool();
    let enc = crypto();
    let cp = SqliteCheckpointer::new(db, enc);
    let thread_id = "thread-skip-step-tt";

    // Save step 0
    let mut s0 = AgentState::default();
    s0.execution_step = 0;
    s0.scratchpad_set("k0", json!("val0"));
    cp.save_checkpoint(thread_id, 0, &s0, "init", None, None, Some("ACTIVE"))
        .await
        .expect("save 0");

    // Save step 5 (skipped 1..4) with full state (no diff)
    let mut s5 = s0.clone();
    s5.execution_step = 5;
    s5.scratchpad_set("k5", json!("val5"));
    cp.save_checkpoint(thread_id, 5, &s5, "step5", None, None, Some("ACTIVE"))
        .await
        .expect("save 5");

    // Save step 6 with diff relative to step 5
    let mut s6 = s5.clone();
    s6.execution_step = 6;
    s6.scratchpad_set("k6", json!("val6"));
    let patch = generate_json_patch(&serde_json::to_value(&s5).unwrap(), &serde_json::to_value(&s6).unwrap());
    let patch_str = serde_json::to_string(&patch).unwrap();
    cp.save_checkpoint(thread_id, 6, &s6, "step6", Some(&patch_str), None, Some("ACTIVE"))
        .await
        .expect("save 6");

    // Restore step 6: nearest base <= 6 is step 5, diff is step 6
    let restored6 = cp.restore_time_travel(thread_id, 6).await.expect("restore 6");
    assert_eq!(restored6.scratchpad.get("k6"), Some(&json!("val6")));
    assert_eq!(restored6.scratchpad.get("k5"), Some(&json!("val5")));

    // Restore step 5
    let restored5 = cp.restore_time_travel(thread_id, 5).await.expect("restore 5");
    assert_eq!(restored5.scratchpad.get("k5"), Some(&json!("val5")));
    assert!(restored5.scratchpad.get("k6").is_none());

    // Restore step 0
    let restored0 = cp.restore_time_travel(thread_id, 0).await.expect("restore 0");
    assert_eq!(restored0.scratchpad.get("k0"), Some(&json!("val0")));
    assert!(restored0.scratchpad.get("k5").is_none());
}

#[tokio::test]
async fn test_hitl_rejection_and_timeout_behavior() {
    let db = pool();
    let enc = crypto();
    let cp = Arc::new(SqliteCheckpointer::new(db, enc));

    let mut runtime = LivaAgentRuntime::new();
    runtime.set_checkpointer(cp.clone());

    runtime.add_node("start", |mut s: AgentState| async move {
        s.messages.push(json!("starting"));
        Ok(s)
    });

    runtime.add_node("dangerous", |s: AgentState| async move {
        let ctx = ApprovalContext::new("act-danger", "format_disk", json!({}), "Dangerous", 10);
        Err(NodeError::YieldUserApproval(s, ctx))
    });

    runtime.add_edge("start", "dangerous");
    runtime.set_entry_point("start");

    let tid = "thread-hitl-reject";
    let res = runtime.run_thread(Some(tid), AgentState::default()).await;
    assert!(matches!(res, Err(NodeError::YieldUserApproval(_, _))));

    // Test rejection
    let rej_res = runtime
        .resume(tid, 1, ApprovalDecision::Rejected { reason: Some("Blocked by Security".to_string()) })
        .await;
    match rej_res {
        Err(NodeError::Fatal(msg)) => {
            assert!(msg.contains("Operation rejected by user: Blocked by Security"));
        }
        _ => panic!("Expected fatal error on rejection, got {:?}", rej_res),
    }

    // Test timeout
    let timeout_res = runtime
        .resume(tid, 1, ApprovalDecision::TimedOut)
        .await;
    match timeout_res {
        Err(NodeError::Timeout(msg)) => {
            assert!(msg.contains("Human approval request timed out"));
        }
        _ => panic!("Expected timeout error, got {:?}", timeout_res),
    }
}
