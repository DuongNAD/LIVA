//! Integration and Fault-Recovery Test Suite for Milestone 4 (Feature 12)
//!
//! Verifies:
//! 1. Multi-node StateGraph execution with per-node intermediate checkpointing.
//! 2. Checkpoint persistence in SQLite WAL `agent_checkpoints` table.
//! 3. AES-256-GCM encryption verification (starts with `v2:`, zero plaintext leakage).
//! 4. Crash injection at intermediate node (`node_b`) and graceful failure.
//! 5. Resumption from checkpoint: `node_b -> node_c -> __END__` executes without re-running `node_a`.
//! 6. Context preservation and multi-thread isolation under concurrent execution.

use liva_native_core::agent::graph::StateGraph;
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::{EncryptionEngine, FactRead};
use liva_native_core::db::DatabasePool;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

/// RAII Temporary Database Guard to ensure clean teardown of disk-backed SQLite files.
struct TempDbGuard(PathBuf);
impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

/// Helper to create a file-backed SQLite database operating in full WAL mode.
fn create_test_db() -> (Arc<DatabasePool>, EncryptionEngine, TempDbGuard) {
    let rand_id = uuid::Uuid::new_v4();
    let db_path = std::env::temp_dir().join(format!("liva_test_ckpt_{rand_id}.sqlite"));
    let db = DatabasePool::new(&db_path).expect("failed to create test SQLite database pool");
    let crypto = EncryptionEngine::new("checkpoint-m4-test-key-32-bytes");
    (Arc::new(db), crypto, TempDbGuard(db_path))
}

// =========================================================================
// TEST 1: MULTI-NODE INTERMEDIATE CHECKPOINT PERSISTENCE & AES ENCRYPTION
// =========================================================================

#[tokio::test]
async fn test_stategraph_multi_node_intermediate_checkpoint_persistence() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-thread-intermediate-001";

    let node_a_calls = Arc::new(AtomicUsize::new(0));
    let node_b_calls = Arc::new(AtomicUsize::new(0));
    let node_c_calls = Arc::new(AtomicUsize::new(0));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    // Node A: Enrichment
    let a_counter = Arc::clone(&node_a_calls);
    graph.add_node("node_a", move |mut state: AgentState| {
        let counter = Arc::clone(&a_counter);
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            state.messages.push(json!({
                "role": "assistant",
                "content": "CANARY_NODE_A_COMPLETED_PAYLOAD"
            }));
            state.context.insert("step_a".to_string(), json!("done"));
            state.current_node = "node_b".to_string();
            Ok(state)
        }
    });

    // Node B: Intermediate inspection & transformation
    let b_counter = Arc::clone(&node_b_calls);
    let cp_check = checkpointer.clone();
    let tid_check = thread_id.to_string();
    graph.add_node("node_b", move |mut state: AgentState| {
        let counter = Arc::clone(&b_counter);
        let cp = cp_check.clone();
        let tid = tid_check.clone();
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            // Verify that node_a checkpoint was persisted to disk BEFORE node_b body executes!
            let intermediate = cp
                .load_checkpoint(&tid)
                .await
                .expect("load ok")
                .expect("checkpoint exists");
            assert_eq!(intermediate.current_node, "node_b");
            assert_eq!(intermediate.context.get("step_a"), Some(&json!("done")));

            state.messages.push(json!({
                "role": "assistant",
                "content": "CANARY_NODE_B_COMPLETED_PAYLOAD"
            }));
            state.context.insert("step_b".to_string(), json!("done"));
            state.current_node = "node_c".to_string();
            Ok(state)
        }
    });

    // Node C: Terminal node
    let c_counter = Arc::clone(&node_c_calls);
    graph.add_node("node_c", move |mut state: AgentState| {
        let counter = Arc::clone(&c_counter);
        async move {
            counter.fetch_add(1, Ordering::SeqCst);
            state.messages.push(json!({
                "role": "assistant",
                "content": "CANARY_NODE_C_COMPLETED_PAYLOAD"
            }));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("node_a");

    let initial_state = AgentState {
        messages: vec![json!({"role": "user", "content": "Execute workflow"})],
        current_node: "node_a".to_string(),
        context: Default::default(),
    };

    let final_state = graph
        .run(initial_state)
        .await
        .expect("full graph run should succeed");
    assert_eq!(final_state.current_node, "__END__");
    assert_eq!(node_a_calls.load(Ordering::SeqCst), 1);
    assert_eq!(node_b_calls.load(Ordering::SeqCst), 1);
    assert_eq!(node_c_calls.load(Ordering::SeqCst), 1);

    // Inspect SQLite WAL table directly
    let raw_payload: String = db
        .readers
        .get()
        .expect("reader connection")
        .query_row(
            "SELECT state_json FROM agent_checkpoints WHERE thread_id = ?1",
            [thread_id],
            |row| row.get(0),
        )
        .expect("raw checkpoint row must exist");

    // Verify AES-256-GCM encryption properties
    assert!(
        raw_payload.starts_with("v2:"),
        "checkpoint payload must be encrypted with v2 header"
    );
    assert!(
        !raw_payload.contains("CANARY_NODE_A_COMPLETED_PAYLOAD"),
        "raw SQLite WAL must not leak plaintext canary A"
    );
    assert!(
        !raw_payload.contains("CANARY_NODE_B_COMPLETED_PAYLOAD"),
        "raw SQLite WAL must not leak plaintext canary B"
    );
    assert!(
        !raw_payload.contains("CANARY_NODE_C_COMPLETED_PAYLOAD"),
        "raw SQLite WAL must not leak plaintext canary C"
    );
    assert!(
        !raw_payload.contains("step_a"),
        "raw SQLite WAL must not leak plaintext context keys"
    );

    // Verify authenticated decryption
    let decrypted = match crypto.read_fact(&raw_payload) {
        FactRead::Ok(plain) => plain,
        FactRead::Locked { reason } => panic!("Decryption locked: {reason}"),
    };
    let recovered_state: AgentState =
        serde_json::from_str(&decrypted).expect("parse decrypted JSON");
    assert_eq!(recovered_state.current_node, "__END__");
    assert_eq!(recovered_state.context.get("step_a"), Some(&json!("done")));
    assert_eq!(recovered_state.context.get("step_b"), Some(&json!("done")));
}

// =========================================================================
// TEST 2: CRASH INJECTION AT NODE_B AND RESUMPTION WITHOUT RE-RUNNING NODE_A
// =========================================================================

#[tokio::test]
async fn test_stategraph_crash_interruption_at_node_b_and_recovery() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-thread-crash-recovery-002";

    let node_a_calls = Arc::new(AtomicUsize::new(0));
    let node_b_calls = Arc::new(AtomicUsize::new(0));
    let node_c_calls = Arc::new(AtomicUsize::new(0));
    let should_crash = Arc::new(AtomicBool::new(true));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    let a_cnt = Arc::clone(&node_a_calls);
    graph.add_node("node_a", move |mut state: AgentState| {
        let cnt = Arc::clone(&a_cnt);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state
                .messages
                .push(json!({"role": "assistant", "content": "STEP_A_DONE"}));
            state.context.insert("val_a".to_string(), json!(42));
            state.current_node = "node_b".to_string();
            Ok(state)
        }
    });

    let b_cnt = Arc::clone(&node_b_calls);
    let crash_flag = Arc::clone(&should_crash);
    graph.add_node("node_b", move |mut state: AgentState| {
        let cnt = Arc::clone(&b_cnt);
        let crash = Arc::clone(&crash_flag);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            if crash.load(Ordering::SeqCst) {
                // Injected simulated crash / panic / worker abort
                return Err("SIMULATED_WORKER_CRASH_AT_NODE_B".to_string());
            }
            state
                .messages
                .push(json!({"role": "assistant", "content": "STEP_B_DONE"}));
            state.context.insert("val_b".to_string(), json!(100));
            state.current_node = "node_c".to_string();
            Ok(state)
        }
    });

    let c_cnt = Arc::clone(&node_c_calls);
    graph.add_node("node_c", move |mut state: AgentState| {
        let cnt = Arc::clone(&c_cnt);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state
                .messages
                .push(json!({"role": "assistant", "content": "STEP_C_DONE"}));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("node_a");

    let init_state = AgentState {
        messages: vec![json!({"role": "user", "content": "Start DAG"})],
        current_node: "node_a".to_string(),
        context: Default::default(),
    };

    // Phase 1: Run graph. Node A succeeds and persists checkpoint, Node B fails and halts
    let run_res = graph.run(init_state).await;
    assert!(run_res.is_err());
    assert_eq!(run_res.unwrap_err(), "SIMULATED_WORKER_CRASH_AT_NODE_B");

    // Check counters immediately after crash
    assert_eq!(node_a_calls.load(Ordering::SeqCst), 1);
    assert_eq!(node_b_calls.load(Ordering::SeqCst), 1);
    assert_eq!(node_c_calls.load(Ordering::SeqCst), 0);

    // Phase 2: Simulate process recovery / reboot
    // Load last successful checkpoint from SQLite WAL
    let loaded = checkpointer.load_checkpoint(thread_id).await.unwrap();
    assert!(loaded.is_some(), "Checkpoint must exist in database");
    let recovered_state = loaded.unwrap();

    // Invariant: checkpoint must resume at node_b with node_a context intact
    assert_eq!(recovered_state.current_node, "node_b");
    assert_eq!(recovered_state.context.get("val_a"), Some(&json!(42)));

    // Clear crash condition (simulating service restart / fix)
    should_crash.store(false, Ordering::SeqCst);

    // Run resuming from checkpoint to completion via resume()
    let resume_res = graph
        .resume(thread_id)
        .await
        .expect("resume call should succeed");
    assert!(
        resume_res.is_some(),
        "resume should have found pending work"
    );
    let final_state = resume_res.unwrap();
    assert_eq!(final_state.current_node, "__END__");

    // CRITICAL INVARIANT: node_a was NEVER re-executed!
    assert_eq!(
        node_a_calls.load(Ordering::SeqCst),
        1,
        "Recovery invariant violated: node_a was re-executed!"
    );
    // node_b executed twice (1 failed attempt + 1 successful recovery execution)
    assert_eq!(node_b_calls.load(Ordering::SeqCst), 2);
    // node_c executed once
    assert_eq!(node_c_calls.load(Ordering::SeqCst), 1);

    // Verify final state messages and context
    let contents: Vec<&str> = final_state
        .messages
        .iter()
        .filter_map(|m| m.get("content").and_then(|c| c.as_str()))
        .collect();
    assert!(contents.contains(&"STEP_A_DONE"));
    assert!(contents.contains(&"STEP_B_DONE"));
    assert!(contents.contains(&"STEP_C_DONE"));
    assert_eq!(final_state.context.get("val_a"), Some(&json!(42)));
    assert_eq!(final_state.context.get("val_b"), Some(&json!(100)));

    // Second resume returns None because current_node is now __END__
    let second_resume = graph.resume(thread_id).await.unwrap();
    assert!(
        second_resume.is_none(),
        "already completed turn cannot be resumed"
    );
}

// =========================================================================
// TEST 3: CONCURRENT THREAD ISOLATION (ZERO CROSS-TALK & NO SQLITE_BUSY)
// =========================================================================

#[tokio::test]
async fn test_stategraph_thread_isolation_and_concurrent_checkpoints() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));

    let mut handles = Vec::new();

    for i in 0..10 {
        let cp = checkpointer.clone();
        let thread_id = format!("concurrent-thread-{i}");

        handles.push(tokio::spawn(async move {
            let mut state = AgentState {
                messages: vec![json!({"role": "user", "content": format!("msg_{i}")})],
                current_node: "node_b".to_string(),
                context: Default::default(),
            };
            state.context.insert("thread_idx".to_string(), json!(i));

            // Save checkpoint concurrently
            cp.save_checkpoint(&thread_id, &state).await.unwrap();

            // Read checkpoint back
            let loaded = cp.load_checkpoint(&thread_id).await.unwrap().unwrap();
            assert_eq!(loaded.context.get("thread_idx"), Some(&json!(i)));
            assert_eq!(loaded.current_node, "node_b");
        }));
    }

    for h in handles {
        h.await.expect("concurrent thread worker must not panic");
    }
}
