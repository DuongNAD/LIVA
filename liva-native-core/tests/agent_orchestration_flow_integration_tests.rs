//! Agent Orchestration Flow Integration Test Suite (TICKET-15)
//!
//! Validates:
//! 1. Full DAG execution & tool synthesis (`router` -> `tool_exec` -> `synthesizer` -> `__END__`).
//! 2. Per-node WAL intermediate checkpointing & crash resumption (zero step 1 re-execution, AES-256-GCM v2 encrypted).
//! 3. Loop cycle guard & recursion limit enforcement (`max_iterations = 25`).
//! 4. Multi-session concurrency isolation across 10 simultaneous execution threads.

use liva_native_core::agent::graph::StateGraph;
use liva_native_core::agent::memory::SqliteCheckpointer;
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::{EncryptionEngine, FactRead};
use liva_native_core::db::DatabasePool;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};

/// RAII Temporary Database Guard to ensure clean teardown of disk-backed SQLite files.
struct TempDbGuard(PathBuf);
impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

static DB_COUNTER: AtomicU64 = AtomicU64::new(500);

fn create_test_db() -> (Arc<DatabasePool>, EncryptionEngine, TempDbGuard) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand_id = format!("{}_{}_{}", std::process::id(), nanos, counter);
    let db_path = std::env::temp_dir().join(format!("liva_agent_flow_test_{rand_id}.sqlite"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    let db = DatabasePool::new(&db_path).expect("failed to create test SQLite database pool");
    let crypto = EncryptionEngine::new("agent-flow-m3-test-key-32-bytes");
    (Arc::new(db), crypto, TempDbGuard(db_path))
}

// =========================================================================
// TEST 1: MULTI-STEP PIPELINE DAG EXECUTION & TOOL SYNTHESIS
// =========================================================================

#[tokio::test]
async fn test_agent_orchestration_full_dag_execution_and_tool_synthesis() {
    let mut graph = StateGraph::new();

    let router_calls = Arc::new(AtomicUsize::new(0));
    let tool_calls = Arc::new(AtomicUsize::new(0));
    let synth_calls = Arc::new(AtomicUsize::new(0));

    // Node 1: Router Node (Intent parsing and tool dispatch)
    let r_cnt = Arc::clone(&router_calls);
    graph.add_node("router", move |mut state: AgentState| {
        let cnt = Arc::clone(&r_cnt);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let prompt = state
                .messages
                .last()
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .unwrap_or("");

            if prompt.contains("thời tiết") {
                state.context.insert(
                    "tool_call".to_string(),
                    json!({
                        "name": "get_weather",
                        "location": "Đà Nẵng",
                    }),
                );
                state.current_node = "tool_exec".to_string();
            } else {
                state.current_node = "synthesizer".to_string();
            }
            Ok(state)
        }
    });

    // Node 2: Tool Execution Node (Executes resolution and populates observations)
    let t_cnt = Arc::clone(&tool_calls);
    graph.add_node("tool_exec", move |mut state: AgentState| {
        let cnt = Arc::clone(&t_cnt);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let tool_name = state
                .context
                .get("tool_call")
                .and_then(|tc| tc.get("name"))
                .and_then(|n| n.as_str())
                .unwrap_or("");

            assert_eq!(tool_name, "get_weather");

            // Mock authentic tool resolution
            state.context.insert(
                "tool_result".to_string(),
                json!({
                    "location": "Đà Nẵng",
                    "temperature": 27.5,
                    "condition": "Nắng nhẹ",
                    "humidity": 72,
                }),
            );

            state.current_node = "synthesizer".to_string();
            Ok(state)
        }
    });

    // Node 3: Synthesizer Node (Generates cohesive final response)
    let s_cnt = Arc::clone(&synth_calls);
    graph.add_node("synthesizer", move |mut state: AgentState| {
        let cnt = Arc::clone(&s_cnt);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let result_opt = state.context.get("tool_result");
            let response_text = match result_opt {
                Some(res) => {
                    let loc = res.get("location").and_then(|l| l.as_str()).unwrap_or("");
                    let temp = res.get("temperature").and_then(|t| t.as_f64()).unwrap_or(0.0);
                    let cond = res.get("condition").and_then(|c| c.as_str()).unwrap_or("");
                    format!("Thời tiết tại {loc}: {temp}°C, {cond}. Thích hợp cho các hoạt động ngoài trời!")
                }
                None => "Không có kết quả công cụ để tổng hợp.".to_string(),
            };

            state.messages.push(json!({
                "role": "assistant",
                "content": response_text
            }));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("router");

    let initial_state = AgentState {
        messages: vec![json!({
            "role": "user",
            "content": "Kiểm tra thời tiết tại Đà Nẵng và đề xuất hoạt động phù hợp"
        })],
        current_node: "router".to_string(),
        context: Default::default(),
    };

    let final_state = graph
        .run(initial_state)
        .await
        .expect("DAG execution should complete cleanly");

    assert_eq!(final_state.current_node, "__END__");
    assert_eq!(router_calls.load(Ordering::SeqCst), 1);
    assert_eq!(tool_calls.load(Ordering::SeqCst), 1);
    assert_eq!(synth_calls.load(Ordering::SeqCst), 1);

    // Verify synthesized output
    let last_msg = final_state
        .messages
        .last()
        .expect("must have final message");
    let content = last_msg
        .get("content")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(content.contains("Đà Nẵng"));
    assert!(content.contains("27.5°C"));
    assert!(content.contains("Nắng nhẹ"));
    assert!(final_state.context.contains_key("tool_result"));
}

// =========================================================================
// TEST 2: PER-NODE WAL CHECKPOINTING & RECOVERY WITHOUT RE-EXECUTING STEP 1
// =========================================================================

#[tokio::test]
async fn test_agent_orchestration_per_node_wal_checkpoint_and_resume() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));
    let thread_id = "test-thread-wal-checkpoint-recovery-042";

    let step1_router_calls = Arc::new(AtomicUsize::new(0));
    let step2_tool_calls = Arc::new(AtomicUsize::new(0));
    let step3_synth_calls = Arc::new(AtomicUsize::new(0));
    let inject_crash = Arc::new(AtomicBool::new(true));

    let mut graph = StateGraph::new().with_checkpointer(checkpointer.clone(), thread_id);

    // Step 1: Router
    let s1 = Arc::clone(&step1_router_calls);
    graph.add_node("step1_router", move |mut state: AgentState| {
        let cnt = Arc::clone(&s1);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state
                .context
                .insert("goal".to_string(), json!("query_sales_data"));
            state.current_node = "step2_tool".to_string();
            Ok(state)
        }
    });

    // Step 2: Tool Execution (Injected failure on first run)
    let s2 = Arc::clone(&step2_tool_calls);
    let crash_flag = Arc::clone(&inject_crash);
    graph.add_node("step2_tool", move |mut state: AgentState| {
        let cnt = Arc::clone(&s2);
        let crash = Arc::clone(&crash_flag);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            if crash.load(Ordering::SeqCst) {
                return Err("INJECTED_TOOL_EXEC_CRASH_SIMULATION".to_string());
            }
            state
                .context
                .insert("sales_records_found".to_string(), json!(150));
            state.current_node = "step3_synthesizer".to_string();
            Ok(state)
        }
    });

    // Step 3: Synthesizer
    let s3 = Arc::clone(&step3_synth_calls);
    graph.add_node("step3_synthesizer", move |mut state: AgentState| {
        let cnt = Arc::clone(&s3);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            let records = state
                .context
                .get("sales_records_found")
                .and_then(|r| r.as_i64())
                .unwrap_or(0);
            state.messages.push(json!({
                "role": "assistant",
                "content": format!("Đã truy xuất thành công {records} bản ghi doanh số.")
            }));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("step1_router");

    let init_state = AgentState {
        messages: vec![json!({"role": "user", "content": "Báo cáo doanh số tháng này"})],
        current_node: "step1_router".to_string(),
        context: Default::default(),
    };

    // Phase 1: Run graph with crash enabled
    let run_res = graph.run(init_state).await;
    assert!(run_res.is_err(), "Run must fail at step 2");
    assert_eq!(run_res.unwrap_err(), "INJECTED_TOOL_EXEC_CRASH_SIMULATION");

    assert_eq!(step1_router_calls.load(Ordering::SeqCst), 1);
    assert_eq!(step2_tool_calls.load(Ordering::SeqCst), 1);
    assert_eq!(step3_synth_calls.load(Ordering::SeqCst), 0);

    // Verify raw SQLite checkpoint in agent_checkpoints table
    let raw_payload: String = db
        .readers
        .get()
        .expect("reader checkout")
        .query_row(
            "SELECT state_json FROM agent_checkpoints WHERE thread_id = ?1;",
            [thread_id],
            |row| row.get(0),
        )
        .expect("checkpoint row must exist");

    // Must be encrypted with v2 header
    assert!(
        raw_payload.starts_with("v2:"),
        "Checkpoint in SQLite WAL must be AES-256-GCM v2 encrypted"
    );
    assert!(
        !raw_payload.contains("query_sales_data"),
        "Plaintext context must not leak in raw SQLite database!"
    );

    // Verify authenticated decryption
    let decrypted = match crypto.read_fact(&raw_payload) {
        FactRead::Ok(plain) => plain,
        FactRead::Locked { reason } => panic!("Decryption locked: {reason}"),
    };
    let intermediate: AgentState = serde_json::from_str(&decrypted).expect("parse JSON");
    assert_eq!(intermediate.current_node, "step2_tool");
    assert_eq!(
        intermediate.context.get("goal"),
        Some(&json!("query_sales_data"))
    );

    // Phase 2: Resume from checkpoint after resolving failure
    inject_crash.store(false, Ordering::SeqCst);

    let resume_res = graph.resume(thread_id).await.expect("resume succeeds");
    assert!(
        resume_res.is_some(),
        "resume should find pending checkpoint"
    );

    let final_state = resume_res.unwrap();
    assert_eq!(final_state.current_node, "__END__");

    // CORE INVARIANT: Step 1 (router) was NEVER re-executed!
    assert_eq!(
        step1_router_calls.load(Ordering::SeqCst),
        1,
        "Recovery invariant violated: Step 1 was re-executed!"
    );
    assert_eq!(step2_tool_calls.load(Ordering::SeqCst), 2);
    assert_eq!(step3_synth_calls.load(Ordering::SeqCst), 1);

    // Verify final output
    let last_content = final_state
        .messages
        .last()
        .and_then(|m| m.get("content"))
        .and_then(|c| c.as_str())
        .unwrap_or("");
    assert!(last_content.contains("150 bản ghi doanh số"));

    // Subsequent resume returns None
    let final_resume = graph.resume(thread_id).await.expect("second resume ok");
    assert!(final_resume.is_none(), "Completed turn must not resume");
}

// =========================================================================
// TEST 3: LOOP CYCLE GUARD & RECURSION LIMIT (MAX_ITERATIONS = 25)
// =========================================================================

#[tokio::test]
async fn test_agent_orchestration_loop_guard_and_recursion_limit() {
    let mut graph = StateGraph::new();

    let node_a_calls = Arc::new(AtomicUsize::new(0));
    let node_b_calls = Arc::new(AtomicUsize::new(0));

    // Construct intentional infinite loop cycle: node_a -> node_b -> node_a
    let na = Arc::clone(&node_a_calls);
    graph.add_node("node_a", move |mut state: AgentState| {
        let cnt = Arc::clone(&na);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.current_node = "node_b".to_string();
            Ok(state)
        }
    });

    let nb = Arc::clone(&node_b_calls);
    graph.add_node("node_b", move |mut state: AgentState| {
        let cnt = Arc::clone(&nb);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.current_node = "node_a".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("node_a");

    let init_state = AgentState {
        messages: vec![json!({"role": "user", "content": "Trigger cycle"})],
        current_node: "node_a".to_string(),
        context: Default::default(),
    };

    // Run graph with default max_iterations = 25
    let result = graph.run(init_state).await;

    assert!(result.is_err(), "Graph must terminate on cycle limit");
    let err_msg = result.unwrap_err();
    assert!(
        err_msg.contains("recursion limit reached") || err_msg.contains("max iterations"),
        "Error message must report recursion limit: got '{err_msg}'"
    );

    let total_executed = node_a_calls.load(Ordering::SeqCst) + node_b_calls.load(Ordering::SeqCst);
    assert_eq!(
        total_executed, 25,
        "Cycle guard must halt after exactly 25 node executions"
    );

    // Test with configurable max_iterations = 10
    let mut custom_graph = StateGraph::new();
    custom_graph.set_max_iterations(10);

    let custom_calls = Arc::new(AtomicUsize::new(0));
    let cc = Arc::clone(&custom_calls);
    custom_graph.add_node("loop_node", move |mut state: AgentState| {
        let cnt = Arc::clone(&cc);
        async move {
            cnt.fetch_add(1, Ordering::SeqCst);
            state.current_node = "loop_node".to_string();
            Ok(state)
        }
    });
    custom_graph.set_entry_point("loop_node");
    custom_graph.add_edge("loop_node", "loop_node");

    let custom_res = custom_graph
        .run(AgentState {
            messages: vec![],
            current_node: "loop_node".to_string(),
            context: Default::default(),
        })
        .await;

    assert!(custom_res.is_err());
    assert_eq!(
        custom_calls.load(Ordering::SeqCst),
        10,
        "Custom cycle guard must halt after exactly 10 node executions"
    );
}

// =========================================================================
// TEST 4: MULTI-SESSION CONCURRENCY & THREAD ISOLATION ACROSS 10 THREADS
// =========================================================================

#[tokio::test]
async fn test_agent_orchestration_multi_session_concurrency_isolation() {
    let (db, crypto, _guard) = create_test_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db.clone(), crypto.clone()));

    let num_sessions = 10;
    let mut handles = Vec::new();

    for session_idx in 0..num_sessions {
        let cp = checkpointer.clone();
        let thread_id = format!("isolated-session-thread-{session_idx}");

        handles.push(tokio::spawn(async move {
            let mut graph = StateGraph::new().with_checkpointer(cp, &thread_id);

            let s_idx = session_idx;
            graph.add_node("n1", move |mut state: AgentState| async move {
                state.context.insert("session_id".to_string(), json!(s_idx));
                state.context.insert("step".to_string(), json!(1));
                state.current_node = "n2".to_string();
                Ok(state)
            });

            graph.add_node("n2", move |mut state: AgentState| {
                async move {
                    // Assert strictly isolated context
                    let id = state
                        .context
                        .get("session_id")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(999);
                    assert_eq!(id, s_idx as u64, "Thread cross-talk detected at step 2!");
                    state.context.insert("step".to_string(), json!(2));
                    state.current_node = "__END__".to_string();
                    Ok(state)
                }
            });

            graph.set_entry_point("n1");

            let initial_state = AgentState {
                messages: vec![json!({"role": "user", "content": format!("query_{s_idx}")})],
                current_node: "n1".to_string(),
                context: Default::default(),
            };

            let final_state = graph
                .run(initial_state)
                .await
                .expect("session execution succeeds");
            assert_eq!(final_state.current_node, "__END__");
            assert_eq!(
                final_state.context.get("session_id"),
                Some(&json!(s_idx)),
                "Session {s_idx} context corrupted!"
            );
            assert_eq!(final_state.context.get("step"), Some(&json!(2)));
        }));
    }

    for h in handles {
        h.await.expect("concurrent session worker must not panic");
    }
}
