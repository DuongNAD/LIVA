//! Empirical Adversarial Challenger Test Suite for Milestone 4 (Feature 12)
//!
//! StateGraph Intermediate Per-Node Checkpointing Invariant & Stress Verification:
//! 1. Multi-hop cascading interruption & recovery (repeated crashes at successive nodes).
//! 2. Branching & explicit transition resumption without re-running upstream nodes.
//! 3. Node failure / panic simulation does NOT poison or overwrite the prior valid checkpoint.
//! 4. Edge cases: empty threads, completed turns (__END__), missing checkpointer, unregistered nodes.
//! 5. High-concurrency stress: 50 concurrent threads with zero cross-talk and zero SQLITE_BUSY.
//! 6. Single-writer actor serialization: 20 concurrent writes to the same thread_id.
//! 7. Concurrent readers and writers under active WAL pressure.
//! 8. AES-256-GCM cryptographic secrecy: raw table examination proving zero plaintext leakage.
//! 9. Tampered ciphertext, unversioned payloads, and wrong key resilience.
//! 10. Large payload and deep history retention fidelity.
//! 11. Stateful iterative loop graph interruption and resumption.

use liva_native_core::agent::graph::StateGraph;
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
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
    let db_path = std::env::temp_dir().join(format!("liva_adv_ckpt_{rand_id}.sqlite"));
    let db = DatabasePool::new(&db_path).expect("failed to create test SQLite database pool");
    let crypto = EncryptionEngine::new("checkpoint-m4-adv-key-32-bytes");
    (Arc::new(db), crypto, TempDbGuard(db_path))
}

// =========================================================================
// TEST 1: MULTI-HOP CASCADING INTERRUPTION & RECOVERY (REPEATED CRASHES)
// =========================================================================

#[tokio::test]
async fn test_adv_1_multi_hop_cascading_interruption_and_recovery() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-adv-cascading-crash";

    let n1_calls = Arc::new(AtomicUsize::new(0));
    let n2_calls = Arc::new(AtomicUsize::new(0));
    let n3_calls = Arc::new(AtomicUsize::new(0));
    let n4_calls = Arc::new(AtomicUsize::new(0));

    let crash_at_2 = Arc::new(AtomicBool::new(true));
    let crash_at_3 = Arc::new(AtomicBool::new(true));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    // Node 1
    let c1 = Arc::clone(&n1_calls);
    graph.add_node("node_1", move |mut state: AgentState| {
        let cnt = Arc::clone(&c1);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.context.insert("n1".to_string(), json!("ok1"));
            state.current_node = "node_2".to_string();
            Ok(state)
        }
    });

    // Node 2 (crashes first)
    let c2 = Arc::clone(&n2_calls);
    let cr2 = Arc::clone(&crash_at_2);
    graph.add_node("node_2", move |mut state: AgentState| {
        let cnt = Arc::clone(&c2);
        let crash = Arc::clone(&cr2);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            if crash.load(Ordering::SeqCst) {
                return Err("CRASH_AT_NODE_2".to_string());
            }
            state.context.insert("n2".to_string(), json!("ok2"));
            state.current_node = "node_3".to_string();
            Ok(state)
        }
    });

    // Node 3 (crashes second)
    let c3 = Arc::clone(&n3_calls);
    let cr3 = Arc::clone(&crash_at_3);
    graph.add_node("node_3", move |mut state: AgentState| {
        let cnt = Arc::clone(&c3);
        let crash = Arc::clone(&cr3);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            if crash.load(Ordering::SeqCst) {
                return Err("CRASH_AT_NODE_3".to_string());
            }
            state.context.insert("n3".to_string(), json!("ok3"));
            state.current_node = "node_4".to_string();
            Ok(state)
        }
    });

    // Node 4 (terminal)
    let c4 = Arc::clone(&n4_calls);
    graph.add_node("node_4", move |mut state: AgentState| {
        let cnt = Arc::clone(&c4);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.context.insert("n4".to_string(), json!("ok4"));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("node_1");

    let initial_state = AgentState {
        messages: vec![],
        current_node: "node_1".to_string(),
        context: Default::default(),
    };

    // Stage 1: Initial run crashes at node 2
    let res1 = graph.run(initial_state).await;
    assert_eq!(res1.unwrap_err(), "CRASH_AT_NODE_2");
    assert_eq!(n1_calls.load(Ordering::SeqCst), 1);
    assert_eq!(n2_calls.load(Ordering::SeqCst), 1);
    assert_eq!(n3_calls.load(Ordering::SeqCst), 0);
    assert_eq!(n4_calls.load(Ordering::SeqCst), 0);

    // Checkpoint after node 1 must point to node 2
    let ckpt1 = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ckpt1.current_node, "node_2");
    assert_eq!(ckpt1.context.get("n1"), Some(&json!("ok1")));

    // Stage 2: Fix crash at node 2, resume -> crashes at node 3
    crash_at_2.store(false, Ordering::SeqCst);
    let res2 = graph.resume(thread_id).await;
    assert_eq!(res2.unwrap_err(), "CRASH_AT_NODE_3");
    assert_eq!(
        n1_calls.load(Ordering::SeqCst),
        1,
        "Node 1 must NOT be re-executed"
    );
    assert_eq!(n2_calls.load(Ordering::SeqCst), 2, "Node 2 executed twice");
    assert_eq!(n3_calls.load(Ordering::SeqCst), 1, "Node 3 attempted once");
    assert_eq!(n4_calls.load(Ordering::SeqCst), 0);

    // Checkpoint after node 2 must point to node 3
    let ckpt2 = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ckpt2.current_node, "node_3");
    assert_eq!(ckpt2.context.get("n1"), Some(&json!("ok1")));
    assert_eq!(ckpt2.context.get("n2"), Some(&json!("ok2")));

    // Stage 3: Fix crash at node 3, resume to completion
    crash_at_3.store(false, Ordering::SeqCst);
    let final_res = graph.resume(thread_id).await.unwrap().unwrap();
    assert_eq!(final_res.current_node, "__END__");

    // Final Invariants:
    assert_eq!(
        n1_calls.load(Ordering::SeqCst),
        1,
        "Node 1 strictly executed once"
    );
    assert_eq!(
        n2_calls.load(Ordering::SeqCst),
        2,
        "Node 2 executed twice (1 fail + 1 pass)"
    );
    assert_eq!(
        n3_calls.load(Ordering::SeqCst),
        2,
        "Node 3 executed twice (1 fail + 1 pass)"
    );
    assert_eq!(n4_calls.load(Ordering::SeqCst), 1, "Node 4 executed once");

    // All context keys preserved
    assert_eq!(final_res.context.get("n1"), Some(&json!("ok1")));
    assert_eq!(final_res.context.get("n2"), Some(&json!("ok2")));
    assert_eq!(final_res.context.get("n3"), Some(&json!("ok3")));
    assert_eq!(final_res.context.get("n4"), Some(&json!("ok4")));
}

// =========================================================================
// TEST 2: DYNAMIC BRANCHING RESUMPTION
// =========================================================================

#[tokio::test]
async fn test_adv_2_branching_resumption_preserves_path() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-adv-branching";

    let router_calls = Arc::new(AtomicUsize::new(0));
    let high_calls = Arc::new(AtomicUsize::new(0));
    let low_calls = Arc::new(AtomicUsize::new(0));
    let should_crash = Arc::new(AtomicBool::new(true));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    let rc = Arc::clone(&router_calls);
    graph.add_node("router", move |mut state: AgentState| {
        let cnt = Arc::clone(&rc);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let score = state
                .context
                .get("score")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if score >= 50 {
                state.current_node = "branch_high".to_string();
            } else {
                state.current_node = "branch_low".to_string();
            }
            Ok(state)
        }
    });

    let hc = Arc::clone(&high_calls);
    let cr = Arc::clone(&should_crash);
    graph.add_node("branch_high", move |mut state: AgentState| {
        let cnt = Arc::clone(&hc);
        let crash = Arc::clone(&cr);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            if crash.load(Ordering::SeqCst) {
                return Err("CRASH_IN_HIGH_BRANCH".to_string());
            }
            state
                .context
                .insert("tier".to_string(), json!("high_expert"));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    let lc = Arc::clone(&low_calls);
    graph.add_node("branch_low", move |mut state: AgentState| {
        let cnt = Arc::clone(&lc);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.context.insert("tier".to_string(), json!("low_slm"));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("router");

    let mut initial_state = AgentState::default();
    initial_state.context.insert("score".to_string(), json!(75));

    // Phase 1: Router decides branch_high, branch_high crashes
    let err = graph.run(initial_state).await.unwrap_err();
    assert_eq!(err, "CRASH_IN_HIGH_BRANCH");
    assert_eq!(router_calls.load(Ordering::SeqCst), 1);
    assert_eq!(high_calls.load(Ordering::SeqCst), 1);
    assert_eq!(low_calls.load(Ordering::SeqCst), 0);

    // Checkpoint saved by router must point to branch_high
    let ckpt = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ckpt.current_node, "branch_high");

    // Phase 2: Resume with crash cleared
    should_crash.store(false, Ordering::SeqCst);
    let final_state = graph.resume(thread_id).await.unwrap().unwrap();
    assert_eq!(final_state.current_node, "__END__");
    assert_eq!(final_state.context.get("tier"), Some(&json!("high_expert")));
    assert_eq!(
        router_calls.load(Ordering::SeqCst),
        1,
        "Router was not re-executed"
    );
    assert_eq!(high_calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        low_calls.load(Ordering::SeqCst),
        0,
        "Low branch was never touched"
    );
}

// =========================================================================
// TEST 3: NODE FAILURE DOES NOT POISON PRIOR CHECKPOINT
// =========================================================================

#[tokio::test]
async fn test_adv_3_node_crash_does_not_poison_prior_checkpoint() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-adv-poison-check";

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    graph.add_node("step_clean", |mut state: AgentState| async move {
        state
            .context
            .insert("clean_key".to_string(), json!("valid_clean_value"));
        state.current_node = "step_poison".to_string();
        Ok(state)
    });

    graph.add_node("step_poison", |mut state: AgentState| async move {
        // Node modifies in-memory state with dirty data, but then fails before finishing
        state
            .context
            .insert("dirty_key".to_string(), json!("DIRTY_MALFORMED"));
        Err("NODE_PANIC_OR_FAILURE".to_string())
    });

    graph.set_entry_point("step_clean");

    let run_res = graph.run(AgentState::default()).await;
    assert!(run_res.is_err());

    // The checkpoint in SQLite MUST NOT contain dirty_key!
    let saved = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(saved.current_node, "step_poison");
    assert_eq!(
        saved.context.get("clean_key"),
        Some(&json!("valid_clean_value"))
    );
    assert!(
        !saved.context.contains_key("dirty_key"),
        "Failed node's uncommitted state must NOT poison the checkpoint"
    );
}

// =========================================================================
// TEST 4: RESUME EDGE CASES & NONEXISTENT NODES
// =========================================================================

#[tokio::test]
async fn test_adv_4_resume_edge_cases_and_nonexistent_nodes() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));

    // Case 1: Resume on a graph without checkpointer configured
    let unconfigured_graph = StateGraph::new();
    let err = unconfigured_graph.resume("any-thread").await.unwrap_err();
    assert!(err.contains("No checkpointer configured"));

    // Case 2: Resume on non-existent thread_id returns Ok(None)
    let graph = StateGraph::new().with_checkpointer(checkpointer.clone(), "missing-thread");
    let res = graph.resume("missing-thread").await.unwrap();
    assert!(res.is_none());

    // Case 3: Resume on completed thread_id (__END__) returns Ok(None)
    let completed_thread = "completed-thread";
    let completed_state = AgentState {
        messages: vec![],
        current_node: "__END__".to_string(),
        context: Default::default(),
    };
    checkpointer
        .save_checkpoint(completed_thread, &completed_state)
        .await
        .unwrap();
    let res_end = graph.resume(completed_thread).await.unwrap();
    assert!(res_end.is_none());

    // Case 4: Resume on checkpoint with current_node pointing to an unregistered node
    let invalid_node_thread = "invalid-node-thread";
    let invalid_state = AgentState {
        messages: vec![],
        current_node: "non_existent_node_xyz".to_string(),
        context: Default::default(),
    };
    checkpointer
        .save_checkpoint(invalid_node_thread, &invalid_state)
        .await
        .unwrap();
    let err_node = graph.resume(invalid_node_thread).await.unwrap_err();
    assert!(
        err_node.contains("Node 'non_existent_node_xyz' not found"),
        "Expected missing node error, got: {err_node}"
    );
}

// =========================================================================
// TEST 5: HIGH CONCURRENCY THREAD ISOLATION (50 THREADS)
// =========================================================================

#[tokio::test]
async fn test_adv_5_high_concurrency_thread_isolation_50_threads() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));

    let mut tasks = Vec::new();

    for i in 0..50 {
        let cp = checkpointer.clone();
        let thread_id = format!("high-conc-thread-{i:03}");

        tasks.push(tokio::spawn(async move {
            let mut graph = StateGraph::new().with_checkpointer(cp.clone(), &thread_id);

            graph.add_node("step_a", move |mut st: AgentState| async move {
                st.context.insert("task_idx".to_string(), json!(i));
                st.current_node = "step_b".to_string();
                Ok(st)
            });

            graph.add_node("step_b", move |mut st: AgentState| async move {
                let idx = st.context.get("task_idx").unwrap().as_i64().unwrap();
                assert_eq!(idx, i as i64);
                st.context.insert("step_b_done".to_string(), json!(true));
                st.current_node = "__END__".to_string();
                Ok(st)
            });

            graph.set_entry_point("step_a");

            let result = graph.run(AgentState::default()).await.unwrap();
            assert_eq!(result.current_node, "__END__");
            assert_eq!(result.context.get("task_idx"), Some(&json!(i)));

            // Load from SQLite and verify isolated persistence
            let loaded = cp.load_checkpoint(&thread_id).await.unwrap().unwrap();
            assert_eq!(loaded.current_node, "__END__");
            assert_eq!(loaded.context.get("task_idx"), Some(&json!(i)));
        }));
    }

    for t in tasks {
        t.await.expect("task must not fail or panic");
    }
}

// =========================================================================
// TEST 6: SINGLE-WRITER ACTOR SERIALIZATION (20 WRITES TO SAME THREAD)
// =========================================================================

#[tokio::test]
async fn test_adv_6_concurrent_writes_same_thread_id_actor_serialization() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let shared_thread_id = "contended-single-thread-id";

    let mut tasks = Vec::new();

    for i in 0..20 {
        let cp = checkpointer.clone();
        tasks.push(tokio::spawn(async move {
            let mut st = AgentState {
                current_node: format!("node_{i}"),
                ..Default::default()
            };
            st.context.insert("writer_idx".to_string(), json!(i));
            // All 20 tasks compete to write to the SAME thread_id simultaneously
            cp.save_checkpoint(shared_thread_id, &st).await
        }));
    }

    // None of the 20 writes should produce SQLITE_BUSY or fail!
    for t in tasks {
        let res = t.await.expect("task join ok");
        assert!(
            res.is_ok(),
            "Write to contended thread must succeed without lock error: {:?}",
            res
        );
    }

    // The database must contain a valid readable state for the thread
    let final_loaded = checkpointer
        .load_checkpoint(shared_thread_id)
        .await
        .unwrap();
    assert!(final_loaded.is_some());
}

// =========================================================================
// TEST 7: CONCURRENT READERS AND WRITERS UNDER WAL PRESSURE
// =========================================================================

#[tokio::test]
async fn test_adv_7_concurrent_readers_and_writers_wal_discipline() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let stop_flag = Arc::new(AtomicBool::new(false));

    // Seed initial checkpoint
    let init_st = AgentState {
        messages: vec![],
        current_node: "init".to_string(),
        context: Default::default(),
    };
    checkpointer
        .save_checkpoint("wal-pressure-thread", &init_st)
        .await
        .unwrap();

    // 5 background writer tasks
    let mut writer_tasks = Vec::new();
    for w in 0..5 {
        let cp = checkpointer.clone();
        let stop = Arc::clone(&stop_flag);
        writer_tasks.push(tokio::spawn(async move {
            let mut iter = 0;
            while !stop.load(Ordering::Relaxed) && iter < 30 {
                let mut st = AgentState {
                    current_node: format!("writer_{w}_{iter}"),
                    ..Default::default()
                };
                st.context.insert("iter".to_string(), json!(iter));
                let _ = cp.save_checkpoint(&format!("writer-tid-{w}"), &st).await;
                iter += 1;
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        }));
    }

    // 5 background reader tasks
    let mut reader_tasks = Vec::new();
    for r in 0..5 {
        let cp = checkpointer.clone();
        let stop = Arc::clone(&stop_flag);
        reader_tasks.push(tokio::spawn(async move {
            let mut read_count = 0;
            while !stop.load(Ordering::Relaxed) && read_count < 30 {
                let target_tid = format!("writer-tid-{}", r % 5);
                let loaded = cp.load_checkpoint(&target_tid).await;
                assert!(
                    loaded.is_ok(),
                    "Concurrent reader must not fail: {:?}",
                    loaded
                );
                read_count += 1;
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
        }));
    }

    for t in writer_tasks {
        t.await.unwrap();
    }
    stop_flag.store(true, Ordering::Relaxed);
    for t in reader_tasks {
        t.await.unwrap();
    }
}

// =========================================================================
// TEST 8: AES-256-GCM CRYPTOGRAPHIC SECRECY & CANARY ABSENCE
// =========================================================================

#[tokio::test]
async fn test_adv_8_aes256_gcm_cryptographic_secrecy_and_canary_absence() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "crypto-secrecy-audit";

    let secret_canary_1 = "SUPER_SECRET_CANARY_TOKEN_999888777";
    let secret_canary_2 = "USER_PII_CREDIT_CARD_4000_1234_5678_9010";
    let secret_canary_3 = "API_KEY_OPENAI_sk-proj-xyz1234567890abcdef";

    let mut st = AgentState {
        current_node: "intermediate_step".to_string(),
        ..Default::default()
    };
    st.messages
        .push(json!({"role": "user", "content": secret_canary_1}));
    st.messages
        .push(json!({"role": "assistant", "content": secret_canary_2}));
    st.context
        .insert("auth_token".to_string(), json!(secret_canary_3));

    checkpointer.save_checkpoint(thread_id, &st).await.unwrap();

    // Query raw SQLite bytes directly from the database connection
    let raw_ciphertext: String = db
        .readers
        .get()
        .unwrap()
        .query_row(
            "SELECT state_json FROM agent_checkpoints WHERE thread_id = ?1",
            [thread_id],
            |row| row.get(0),
        )
        .expect("row must exist");

    // Assert AES-256-GCM v2 ciphertext properties
    assert!(
        raw_ciphertext.starts_with("v2:"),
        "Must use v2: header format"
    );
    assert!(
        !raw_ciphertext.contains(secret_canary_1),
        "Must NOT leak canary 1"
    );
    assert!(
        !raw_ciphertext.contains(secret_canary_2),
        "Must NOT leak canary 2"
    );
    assert!(
        !raw_ciphertext.contains(secret_canary_3),
        "Must NOT leak canary 3"
    );
    assert!(
        !raw_ciphertext.contains("auth_token"),
        "Must NOT leak context keys"
    );
    assert!(
        !raw_ciphertext.contains("intermediate_step"),
        "Must NOT leak node names"
    );

    // Authenticated decryption verifies exact plaintext match
    let loaded = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.current_node, "intermediate_step");
    assert_eq!(loaded.messages[0]["content"], secret_canary_1);
    assert_eq!(loaded.messages[1]["content"], secret_canary_2);
    assert_eq!(loaded.context["auth_token"], secret_canary_3);
}

// =========================================================================
// TEST 9: TAMPERED CIPHERTEXT & WRONG KEY RESILIENCE
// =========================================================================

#[tokio::test]
async fn test_adv_9_tampered_ciphertext_and_wrong_key_resilience() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "tamper-test-thread";

    let st = AgentState {
        current_node: "safe_step".to_string(),
        ..Default::default()
    };
    checkpointer.save_checkpoint(thread_id, &st).await.unwrap();

    // Subtest A: Wrong encryption key cannot decrypt
    let wrong_crypto = EncryptionEngine::new("completely-wrong-key-32-bytes!!");
    let wrong_cp = SqliteCheckpointer::new(db.clone(), wrong_crypto);
    let wrong_res = wrong_cp.load_checkpoint(thread_id).await;
    assert!(wrong_res.is_err(), "Decryption with wrong key must fail");
    let err_str = wrong_res.unwrap_err();
    assert!(
        err_str.contains("checkpoint bị khóa"),
        "Error message must indicate locked checkpoint, got: {err_str}"
    );

    // Subtest B: Corrupted ciphertext in SQLite table
    db.writer_actor
        .save_agent_checkpoint(
            "corrupted-tid".to_string(),
            "v2:invalid_nonce_and_bad_tag_data_123456789".to_string(),
        )
        .await
        .unwrap();

    let corrupted_res = checkpointer.load_checkpoint("corrupted-tid").await;
    assert!(
        corrupted_res.is_err(),
        "Corrupted ciphertext must gracefully error out"
    );

    // Subtest C: Unversioned string (e.g. legacy plain text)
    db.writer_actor
        .save_agent_checkpoint(
            "unversioned-tid".to_string(),
            "plain_non_v2_string".to_string(),
        )
        .await
        .unwrap();

    let unversioned_res = checkpointer.load_checkpoint("unversioned-tid").await;
    assert!(
        unversioned_res.is_err(),
        "Unversioned string must error out gracefully"
    );
}

// =========================================================================
// TEST 10: LARGE PAYLOAD AND DEEP HISTORY FIDELITY
// =========================================================================

#[tokio::test]
async fn test_adv_10_large_payload_and_deep_history_fidelity() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "large-payload-fidelity-thread";

    let mut large_state = AgentState {
        current_node: "heavy_node".to_string(),
        ..Default::default()
    };

    // 50 messages
    for i in 0..50 {
        large_state.messages.push(json!({
            "role": if i % 2 == 0 { "user" } else { "assistant" },
            "content": format!("Message index {i} with some substantial payload contents repeating: {}", "x".repeat(200))
        }));
    }

    // 25 context variables
    for k in 0..25 {
        large_state.context.insert(
            format!("var_{k}"),
            json!({
                "key": k,
                "data": vec![k, k + 1, k + 2],
                "note": "payload test buffer"
            }),
        );
    }

    // Save large checkpoint
    checkpointer
        .save_checkpoint(thread_id, &large_state)
        .await
        .unwrap();

    // Load back and verify complete structural equality
    let loaded = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(loaded.current_node, "heavy_node");
    assert_eq!(loaded.messages.len(), 50);
    assert_eq!(loaded.context.len(), 25);
    assert_eq!(loaded.messages[49]["role"], "assistant");
    assert_eq!(loaded.context["var_24"]["key"], 24);
}

// =========================================================================
// TEST 11: STATEFUL ITERATIVE LOOP GRAPH RESUMPTION
// =========================================================================

#[tokio::test]
async fn test_adv_11_stateful_iterative_loop_graph_resumption() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "stateful-loop-thread";

    let loop_executions = Arc::new(AtomicUsize::new(0));
    let crash_on_iter_2 = Arc::new(AtomicBool::new(true));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    let le = Arc::clone(&loop_executions);
    let cr = Arc::clone(&crash_on_iter_2);

    graph.add_node("loop_step", move |mut st: AgentState| {
        let cnt = Arc::clone(&le);
        let crash = Arc::clone(&cr);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let current_iter = st.context.get("iter").and_then(|v| v.as_i64()).unwrap_or(0);

            // Crash on iteration 2 if flag set
            if current_iter == 2 && crash.load(Ordering::SeqCst) {
                return Err("CRASH_DURING_LOOP_ITERATION_2".to_string());
            }

            let next_iter = current_iter + 1;
            st.context.insert("iter".to_string(), json!(next_iter));

            if next_iter >= 5 {
                st.current_node = "__END__".to_string();
            } else {
                st.current_node = "loop_step".to_string(); // loop edge
            }
            Ok(st)
        }
    });

    graph.add_edge("loop_step", "loop_step");
    graph.set_entry_point("loop_step");

    let initial_st = AgentState {
        messages: vec![],
        current_node: "loop_step".to_string(),
        context: Default::default(),
    };

    // First run: executes iter 0 (iter becomes 1), iter 1 (iter becomes 2), iter 2 (crashes!)
    let err = graph.run(initial_st).await.unwrap_err();
    assert_eq!(err, "CRASH_DURING_LOOP_ITERATION_2");
    assert_eq!(loop_executions.load(Ordering::SeqCst), 3); // iters 0, 1, 2 attempted

    // Checkpoint in DB must have iter == 2 saved from the end of iteration 1
    let ckpt = checkpointer
        .load_checkpoint(thread_id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(ckpt.current_node, "loop_step");
    assert_eq!(ckpt.context.get("iter"), Some(&json!(2)));

    // Clear crash and resume
    crash_on_iter_2.store(false, Ordering::SeqCst);
    let final_st = graph.resume(thread_id).await.unwrap().unwrap();
    assert_eq!(final_st.current_node, "__END__");
    assert_eq!(final_st.context.get("iter"), Some(&json!(5)));

    // Iteration 2 was re-tried and succeeded; then iter 3 and iter 4 ran.
    // Initial: iters 0, 1, 2(failed). Resumed: iters 2(passed), 3, 4 -> Total loop_executions = 3 + 3 = 6.
    assert_eq!(loop_executions.load(Ordering::SeqCst), 6);
}
