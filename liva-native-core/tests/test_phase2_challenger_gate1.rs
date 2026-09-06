//! Phase 2 Challenger Gate 1 Stress & Adversarial Test Suite
//!
//! Empirically challenges:
//! - Cyclic State Graph FSM convergence, cycle threshold interrupts, concurrent branch isolation
//! - SQLite Checkpointer, RFC 6902 Differential JSON Patches & Time-Travel Recovery
//! - Sub-0.1ms Token-Aware JSON AST Self-Healing Engine against extreme adversarial payloads
//! - Reflexion Loop & Transactional Workspace Snapshot/Rollback isolation

use liva_native_core::agent::graph::{
    apply_json_patch, generate_json_patch, Checkpointer, LivaAgentRuntime, NodeError,
    SqliteCheckpointer,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::ast_repair::json_repair::{repair_json_ast, AstRepairError};
use liva_native_core::ast_repair::reflexion::{ReflexionError, WorkspaceManager};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use serde_json::{json, Value};
use std::fs;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

fn pool() -> Arc<DatabasePool> {
    Arc::new(DatabasePool::new_in_memory().expect("in-memory db"))
}

fn crypto() -> EncryptionEngine {
    EncryptionEngine::new("gate1-adversarial-challenger-key-32")
}

// ============================================================================
// PART 1: CYCLIC STATE GRAPH FSM & PREGEL SCHEDULER EMPIRICAL CHALLENGES
// ============================================================================

#[tokio::test]
async fn test_challenge_cyclic_exact_threshold_boundary() {
    // 1. Graph that loops exactly 5 times and terminates on 5th iteration
    let mut runtime = LivaAgentRuntime::new();
    runtime.set_max_cycles_per_node(5);

    runtime.add_node("counter_node", |mut s: AgentState| async move {
        let count = s.scratchpad.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        s.scratchpad_set("count", json!(count + 1));
        Ok(s)
    });

    runtime.add_conditional_edge("counter_node", |s: &AgentState| {
        let count = s.scratchpad.get("count").and_then(|v| v.as_u64()).unwrap_or(0);
        if count >= 5 {
            "__END__".to_string()
        } else {
            "counter_node".to_string()
        }
    });
    runtime.set_entry_point("counter_node");

    let result = runtime.run(AgentState::default()).await;
    assert!(result.is_ok(), "Graph looping exactly 5 times must succeed on boundary");
    let state = result.unwrap();
    assert_eq!(state.scratchpad.get("count"), Some(&json!(5)));

    // 2. Graph that attempts 6th iteration when limit is 5
    let mut runtime_fail = LivaAgentRuntime::new();
    runtime_fail.set_max_cycles_per_node(5);

    runtime_fail.add_node("infinite_loop", |s: AgentState| async move { Ok(s) });
    runtime_fail.add_edge("infinite_loop", "infinite_loop");
    runtime_fail.set_entry_point("infinite_loop");

    let fail_result = runtime_fail.run(AgentState::default()).await;
    match fail_result {
        Err(NodeError::Fatal(msg)) => {
            assert!(
                msg.contains("Dynamic loop detected") && msg.contains("exceeded cycle limit of 5"),
                "Expected dynamic loop detection error message, got: {}",
                msg
            );
        }
        _ => panic!("Expected Fatal loop detection error, got {:?}", fail_result),
    }
}

#[tokio::test]
async fn test_challenge_multi_node_cycle_threshold_and_total_steps() {
    let mut runtime = LivaAgentRuntime::new();
    runtime.set_max_steps(10);
    runtime.set_max_cycles_per_node(20);

    // Multi-node cycle: A -> B -> C -> A
    runtime.add_node("node_a", |mut s: AgentState| async move {
        s.messages.push(json!("A"));
        Ok(s)
    });
    runtime.add_node("node_b", |mut s: AgentState| async move {
        s.messages.push(json!("B"));
        Ok(s)
    });
    runtime.add_node("node_c", |mut s: AgentState| async move {
        s.messages.push(json!("C"));
        Ok(s)
    });

    runtime.add_edge("node_a", "node_b");
    runtime.add_edge("node_b", "node_c");
    runtime.add_edge("node_c", "node_a");
    runtime.set_entry_point("node_a");

    let result = runtime.run(AgentState::default()).await;
    match result {
        Err(NodeError::Timeout(msg)) => {
            assert!(
                msg.contains("maximum allowable steps (10)"),
                "Expected total step timeout error, got: {}",
                msg
            );
        }
        _ => panic!("Expected Timeout error for step limit, got {:?}", result),
    }
}

#[tokio::test]
async fn test_challenge_concurrent_independent_state_graphs() {
    let db = pool();
    let enc = crypto();
    let cp = Arc::new(SqliteCheckpointer::new(db, enc));

    let concurrency = 20;
    let mut handles = Vec::new();

    for i in 0..concurrency {
        let cp_clone = cp.clone();
        let handle = tokio::spawn(async move {
            let thread_id = format!("concurrent-thread-{}", i);
            let mut runtime = LivaAgentRuntime::new();
            runtime.set_checkpointer(cp_clone);
            runtime.set_max_steps(30);

            runtime.add_node("step_a", move |mut s: AgentState| {
                let id = i;
                async move {
                    s.scratchpad_set("thread_id", json!(id));
                    s.scratchpad_set("val_a", json!(id * 10));
                    s.messages.push(json!({"step": "a", "thread": id}));
                    Ok(s)
                }
            });

            runtime.add_node("step_b", move |mut s: AgentState| {
                let id = i;
                async move {
                    s.scratchpad_set("val_b", json!(id * 10 + 5));
                    s.messages.push(json!({"step": "b", "thread": id}));
                    Ok(s)
                }
            });

            runtime.add_edge("step_a", "step_b");
            runtime.add_edge("step_b", "__END__");
            runtime.set_entry_point("step_a");

            let res = runtime
                .run_thread(Some(&thread_id), AgentState::default())
                .await
                .expect("execution failed");

            (thread_id, res, i)
        });
        handles.push(handle);
    }

    for h in handles {
        let (tid, state, i) = h.await.expect("join handle");
        assert_eq!(state.scratchpad.get("thread_id"), Some(&json!(i)));
        assert_eq!(state.scratchpad.get("val_a"), Some(&json!(i * 10)));
        assert_eq!(state.scratchpad.get("val_b"), Some(&json!(i * 10 + 5)));
        assert_eq!(state.messages.len(), 2);

        // Verify checkpoints in SQLite for this thread
        let checkpoints = cp.list_checkpoints(&tid).await.expect("list checkpoints");
        assert!(checkpoints.len() >= 3, "Expected at least 3 checkpoints (0, 1, 2)");
        let latest = cp.load_latest(&tid).await.expect("load latest").expect("exists");
        assert_eq!(latest.1.scratchpad.get("val_b"), Some(&json!(i * 10 + 5)));
    }
}

#[tokio::test]
async fn test_challenge_parallel_branch_isolation_and_error_propagation() {
    let mut runtime = LivaAgentRuntime::new();

    runtime.add_node("fanout", |s: AgentState| async move { Ok(s) });
    runtime.add_node("branch_1", |mut s: AgentState| async move {
        s.scratchpad_set("b1", json!("ok"));
        Ok(s)
    });
    runtime.add_node("branch_2", |mut s: AgentState| async move {
        s.scratchpad_set("b2", json!("ok"));
        Ok(s)
    });
    runtime.add_node("branch_failing", |_s: AgentState| async move {
        Err(NodeError::Fatal("branch execution fatal explosion".to_string()))
    });

    runtime.add_parallel_edge(
        "fanout",
        vec![
            "branch_1".to_string(),
            "branch_2".to_string(),
            "branch_failing".to_string(),
        ],
    );
    runtime.set_entry_point("fanout");

    let res = runtime.run(AgentState::default()).await;
    match res {
        Err(NodeError::Fatal(msg)) => {
            assert!(
                msg.contains("branch execution fatal explosion"),
                "Parallel error propagation failed: {}",
                msg
            );
        }
        _ => panic!("Expected Fatal error from failing parallel branch, got {:?}", res),
    }
}

#[tokio::test]
async fn test_challenge_reflexion_retry_inside_graph_node() {
    let mut runtime = LivaAgentRuntime::new();
    let attempt_counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = attempt_counter.clone();

    runtime.add_node("reflexion_node", move |mut s: AgentState| {
        let c = counter_clone.clone();
        async move {
            let current = c.fetch_add(1, Ordering::SeqCst);
            if current < 2 {
                s.scratchpad_set("retry_note", json!(format!("attempt_{}", current)));
                Err(NodeError::ReflexionRetry(
                    s,
                    format!("Temporary logical defect at attempt {}", current),
                ))
            } else {
                s.scratchpad_set("final_status", json!("success_after_retries"));
                Ok(s)
            }
        }
    });

    runtime.add_edge("reflexion_node", "__END__");
    runtime.set_entry_point("reflexion_node");

    let result = runtime.run(AgentState::default()).await.expect("run reflexion");
    assert_eq!(attempt_counter.load(Ordering::SeqCst), 3);
    assert_eq!(
        result.scratchpad.get("final_status"),
        Some(&json!("success_after_retries"))
    );
}

// ============================================================================
// PART 2: SQLITE CHECKPOINTER, RFC 6902 PATCHES & TIME-TRAVEL RECOVERY
// ============================================================================

#[test]
fn test_challenge_deep_nested_rfc6902_diff_and_patch() {
    let v1 = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "level5": {
                            "items": [1, 2, 3, 4, 5],
                            "metadata": {
                                "author": "Alice",
                                "flags": [true, false, true]
                            }
                        }
                    }
                }
            }
        },
        "root_val": 42
    });

    let v2 = json!({
        "level1": {
            "level2": {
                "level3": {
                    "level4": {
                        "level5": {
                            "items": [1, 20, 3, 4, 5, 6], // replaced item 1, appended 6
                            "metadata": {
                                "author": "Bob", // changed author
                                "flags": [true, true], // removed one, changed one
                                "extra_tag": "v2_tag" // added key
                            }
                        }
                    }
                }
            }
        },
        "root_val": 100, // changed
        "new_root_array": ["a", "b", "c"] // added root array
    });

    let patch = generate_json_patch(&v1, &v2);
    assert!(!patch.is_empty());

    let reconstructed = apply_json_patch(&v1, &patch).expect("apply deep patch");
    assert_eq!(reconstructed, v2, "Reconstructed JSON must match target JSON exactly");
}

#[test]
fn test_challenge_json_pointer_escaped_characters() {
    let v1 = json!({
        "path/with/slashes": 1,
        "key~with~tildes": 2,
        "combo/~0/~1/special": {"nested/key": "old_val"}
    });

    let v2 = json!({
        "path/with/slashes": 100,
        "key~with~tildes": 200,
        "combo/~0/~1/special": {"nested/key": "new_val", "added/slash": true},
        "brand/new~key": [1, 2]
    });

    let patch = generate_json_patch(&v1, &v2);
    let reconstructed = apply_json_patch(&v1, &patch).expect("apply escaped pointer patch");
    assert_eq!(reconstructed, v2);
}

#[tokio::test]
async fn test_challenge_multi_step_time_travel_rewind_random_sampling() {
    let db = pool();
    let enc = crypto();
    let cp = SqliteCheckpointer::new(db, enc);
    let thread_id = "thread-multi-step-tt";

    let total_steps = 30;
    let mut history_states = Vec::with_capacity(total_steps);

    let mut current_state = AgentState::default();
    current_state.current_node = "init".to_string();
    current_state.execution_step = 0;
    current_state.scratchpad_set("step_0", json!("base"));
    history_states.push(current_state.clone());

    // Save step 0 as full base checkpoint
    cp.save_checkpoint(thread_id, 0, &current_state, "init", None, None, Some("ACTIVE"))
        .await
        .expect("save base step 0");

    let mut prev_val = serde_json::to_value(&current_state).unwrap();

    for step in 1..total_steps {
        let mut next_state = current_state.clone();
        next_state.execution_step = step;
        next_state.current_node = format!("node_{}", step);
        next_state.messages.push(json!({"step": step, "text": format!("Message for step {}", step)}));
        next_state.scratchpad_set(&format!("step_{}", step), json!(step * 100));

        let next_val = serde_json::to_value(&next_state).unwrap();
        let patch = generate_json_patch(&prev_val, &next_val);
        let patch_str = serde_json::to_string(&patch).unwrap();

        cp.save_checkpoint(
            thread_id,
            step,
            &next_state,
            &format!("node_{}", step),
            Some(&patch_str),
            None,
            Some("ACTIVE"),
        )
        .await
        .expect("save diff step");

        history_states.push(next_state.clone());
        prev_val = next_val;
        current_state = next_state;
    }

    // Time-travel rewind test at multiple sample steps
    let test_indices = vec![0, 1, 5, 12, 19, 25, 29];
    for idx in test_indices {
        let restored = cp
            .restore_time_travel(thread_id, idx)
            .await
            .unwrap_or_else(|e| panic!("Failed to restore step {}: {}", idx, e));

        let expected = &history_states[idx];
        assert_eq!(
            restored.execution_step, expected.execution_step,
            "Execution step mismatch at index {}",
            idx
        );
        assert_eq!(
            restored.messages.len(),
            expected.messages.len(),
            "Messages length mismatch at index {}",
            idx
        );
        assert_eq!(
            restored.scratchpad.get(&format!("step_{}", idx)),
            expected.scratchpad.get(&format!("step_{}", idx)),
            "Scratchpad key mismatch at index {}",
            idx
        );
    }
}

#[tokio::test]
async fn test_challenge_tool_replay_cache_concurrency_and_overwrites() {
    let db = pool();
    let enc = crypto();
    let cp = SqliteCheckpointer::new(db, enc);
    let thread_id = "thread-replay-cache-stress";

    let mut st = AgentState::default();
    st.execution_step = 1;
    cp.save_checkpoint(thread_id, 1, &st, "tool_node", None, None, Some("ACTIVE"))
        .await
        .expect("save checkpoint");

    // 1. Concurrent writes to distinct tool call IDs on step 1
    let mut handles = Vec::new();
    let cp_arc = Arc::new(cp);

    for i in 0..10 {
        let cp_clone = cp_arc.clone();
        let handle = tokio::spawn(async move {
            let tool_id = format!("call_tool_{}", i);
            let payload = json!({
                "tool_id": tool_id,
                "output": format!("Output from tool {}", i),
                "unicode": "🚀 LIVA \n\t\"quoted\"",
                "number": i * 42
            });
            cp_clone
                .record_tool_output(thread_id, 1, &tool_id, &payload)
                .await
                .expect("record tool output");
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("join handle");
    }

    // 2. Verify all cached outputs are intact
    for i in 0..10 {
        let tool_id = format!("call_tool_{}", i);
        let cached = cp_arc
            .get_cached_tool_output(thread_id, 1, &tool_id)
            .await
            .expect("get cached")
            .expect("should find cached tool output");

        assert_eq!(cached["tool_id"], tool_id);
        assert_eq!(cached["number"], i * 42);
    }

    // 3. Test overwrite of an existing tool output
    let overwrite_val = json!({"status": "overwritten", "new_val": 9999});
    cp_arc
        .record_tool_output(thread_id, 1, "call_tool_0", &overwrite_val)
        .await
        .expect("overwrite tool 0");

    let updated = cp_arc
        .get_cached_tool_output(thread_id, 1, "call_tool_0")
        .await
        .expect("get cached")
        .expect("found");
    assert_eq!(updated["status"], "overwritten");
    assert_eq!(updated["new_val"], 9999);

    // 4. Test cache miss on non-existent tool ID
    let miss = cp_arc
        .get_cached_tool_output(thread_id, 1, "non_existent_call")
        .await
        .expect("get cached");
    assert!(miss.is_none(), "Cache miss must return None");
}

// ============================================================================
// PART 3: SUB-0.1ms TOKEN-AWARE JSON AST REPAIR EXTREME ADVERSARIAL STRESS
// ============================================================================

#[test]
fn test_challenge_json_repair_extreme_deep_nesting_stack_completion() {
    // 50 levels of unclosed nested objects and arrays
    let mut payload = String::new();
    for i in 0..30 {
        payload.push_str(&format!("{{\"l_{}\": [", i));
    }
    payload.push_str("\"deep_value\"");

    let repaired = repair_json_ast(&payload).expect("Should repair 30 levels unclosed stack");
    let mut current = &repaired;
    for i in 0..30 {
        current = &current[format!("l_{}", i)][0];
    }
    assert_eq!(current, "deep_value");
}

#[test]
fn test_challenge_json_repair_adversarial_quotes_and_escapes() {
    // 1. Unescaped quotes inside strings
    let raw1 = r#"{"title": "The "Special" Edition of "Rust" Book", "status": "active"}"#;
    let res1 = repair_json_ast(raw1).expect("Repair unescaped quotes");
    assert_eq!(res1["title"], "The \"Special\" Edition of \"Rust\" Book");
    assert_eq!(res1["status"], "active");

    // 2. Windows file paths with unescaped backslashes
    let raw2 = r#"{"executable": "C:\Program Files\LIVA\core.exe", "log": "C:\temp\run.log"}"#;
    let res2 = repair_json_ast(raw2).expect("Repair windows backslashes");
    assert!(res2["executable"].as_str().unwrap().contains("core.exe"));

    // 3. Single quotes with interior contractions (apostrophes)
    let raw3 = r#"{'query': 'It\'s working, don\'t fail, LIVA\'s power'}"#;
    let res3 = repair_json_ast(raw3).expect("Repair single quotes with apostrophes");
    assert_eq!(res3["query"], "It's working, don't fail, LIVA's power");

    // 4. Mixed single and double quotes
    let raw4 = r#"{'mode': "fast", "options": {'timeout': 30, 'retry': True}}"#;
    let res4 = repair_json_ast(raw4).expect("Repair mixed quotes");
    assert_eq!(res4["mode"], "fast");
    assert_eq!(res4["options"]["timeout"], 30);
    assert_eq!(res4["options"]["retry"], true);
}

#[test]
fn test_challenge_json_repair_unquoted_keys_and_assignment_operators() {
    let raw = r#"
    {
        service-name = "auth-backend",
        max_workers => 16,
        timeout.sec = 120,
        enable_tls: True
        debug-mode: False
    }
    "#;
    let res = repair_json_ast(raw).expect("Repair unquoted keys and assignment operators");
    assert_eq!(res["service-name"], "auth-backend");
    assert_eq!(res["max_workers"], 16);
    assert_eq!(res["timeout.sec"], 120);
    assert_eq!(res["enable_tls"], true);
    assert_eq!(res["debug-mode"], false);
}

#[test]
fn test_challenge_json_repair_pythonic_and_js_literals() {
    let raw = r#"
    {
        "a": True,
        "b": False,
        "c": None,
        "d": undefined,
        "e": NaN,
        "f": null,
        "g": nil,
        "h": Infinity,
        "i": -Infinity
    }
    "#;
    let res = repair_json_ast(raw).expect("Repair pythonic and JS literals");
    assert_eq!(res["a"], true);
    assert_eq!(res["b"], false);
    assert_eq!(res["c"], Value::Null);
    assert_eq!(res["d"], Value::Null);
    assert_eq!(res["e"], Value::Null);
    assert_eq!(res["f"], Value::Null);
    assert_eq!(res["g"], Value::Null);
}

#[test]
fn test_challenge_json_repair_adversarial_trailing_commas_and_redundant_delimiters() {
    let raw = r#"{"a": 1,,,, "b": [1,,,, 2,,,,],,,, "c": {"d": "ok",,},,}"#;
    let res = repair_json_ast(raw).expect("Repair multiple redundant commas");
    assert_eq!(res["a"], 1);
    assert_eq!(res["b"], json!([1, 2]));
    assert_eq!(res["c"]["d"], "ok");
}

#[test]
fn test_challenge_json_repair_noisy_markdown_and_conversational_prose() {
    let raw = r#"
    Here is the requested tool call:
    <think>
    User asked to run cargo test with nocapture.
    </think>
    ```json
    {
        "command": "cargo test",
        "flags": ["--nocapture", "--test", "integration",],
        "timeout": 60,
    }
    ```
    Please confirm if you want to proceed.
    "#;
    let res = repair_json_ast(raw).expect("Repair noisy markdown and conversational text");
    assert_eq!(res["command"], "cargo test");
    assert_eq!(res["flags"], json!(["--nocapture", "--test", "integration"]));
    assert_eq!(res["timeout"], 60);
}

#[test]
fn test_challenge_json_repair_unrecoverable_gibberish_safety() {
    let non_json = "This is simply unstructured natural language prose with zero brackets or colons.";
    let err = repair_json_ast(non_json);
    assert!(err.is_err());
    match err.unwrap_err() {
        AstRepairError::NoJsonFound(_) => {}
        other => panic!("Expected NoJsonFound error, got {:?}", other),
    }

    let empty = "   \n\t  ";
    let empty_err = repair_json_ast(empty);
    assert_eq!(empty_err.unwrap_err(), AstRepairError::EmptyInput);
}

#[test]
fn test_challenge_json_repair_latency_sla_under_100_micros() {
    let payloads = vec![
        r#"{"name": 'LIVA', 'active': True, 'count': 42, 'items': [1, 2, 3,],}"#,
        r#"{cmd = "write_file", path => "/tmp/test.txt", content: 'hello "world" it\'s fine',}"#,
        r#"```json\n{"status": "ok", "tags": ['ai', 'fast', None, undefined],}\n```"#,
        r#"{"a": {"b": {"c": [1, 2, 3"#,
        r#"{"k1": 1 "k2": 2 "k3": [1 2 3]}"#,
    ];

    let iterations = 2000;
    let mut total_micros: u64 = 0;
    let mut max_micros: u64 = 0;

    for i in 0..iterations {
        let payload = payloads[i % payloads.len()];
        let start = Instant::now();
        let val = repair_json_ast(payload).expect("Must repair successfully");
        let elapsed = start.elapsed().as_micros() as u64;
        assert!(val.is_object());

        total_micros += elapsed;
        if elapsed > max_micros {
            max_micros = elapsed;
        }
    }

    let avg_micros = total_micros as f64 / iterations as f64;
    println!(
        "Sub-0.1ms Benchmark Result: iterations={}, avg={:.2} µs, max={} µs",
        iterations, avg_micros, max_micros
    );

    // Assert SLA: Average execution time must be under 100 microseconds (0.1ms)
    assert!(
        avg_micros < 100.0,
        "Average repair latency ({:.2} µs) exceeded 100 µs (0.1ms) SLA target",
        avg_micros
    );
}

// ============================================================================
// PART 4: REFLEXION ENGINE & TRANSACTIONAL WORKSPACE ROLLBACK
// ============================================================================

#[test]
fn test_challenge_reflexion_workspace_nested_directory_and_isolation() {
    let temp_root = std::env::temp_dir().join(format!("liva_challenge_ws_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_root).expect("create workspace root");

    let mut mgr = WorkspaceManager::new(&temp_root);

    // Existing file before step
    let existing_file = temp_root.join("existing.rs");
    fs::write(&existing_file, "fn original() {}").expect("write existing");

    // Begin step 1
    mgr.begin_step("step_1").expect("begin step 1");

    // 1. Record modification of existing file
    mgr.record_modification("step_1", &existing_file).expect("record mod");
    fs::write(&existing_file, "fn corrupted_mutation() {}").expect("write corrupted");

    // 2. Create nested directory and files
    let nested_dir = temp_root.join("deep").join("nested");
    fs::create_dir_all(&nested_dir).expect("create nested dirs");
    let nested_file = nested_dir.join("temp_gen.rs");
    mgr.record_creation("step_1", &nested_file).expect("record creation");
    fs::write(&nested_file, "// temporary generated code").expect("write nested file");

    assert!(nested_file.exists());
    assert_eq!(fs::read_to_string(&existing_file).unwrap(), "fn corrupted_mutation() {}");

    // Rollback step 1
    mgr.rollback_step("step_1").expect("rollback step 1");

    // Verify created file is deleted
    assert!(!nested_file.exists(), "Created file must be deleted on rollback");

    // Verify modified file is restored to pristine original content
    assert_eq!(
        fs::read_to_string(&existing_file).unwrap(),
        "fn original() {}",
        "Modified file must be restored to original bytes"
    );

    let _ = fs::remove_dir_all(&temp_root);
}

#[test]
fn test_challenge_reflexion_path_jailbreak_traversal_attack() {
    let temp_root = std::env::temp_dir().join(format!("liva_jailbreak_ws_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&temp_root).expect("create workspace root");

    let mut mgr = WorkspaceManager::new(&temp_root);
    mgr.begin_step("step_jailbreak").expect("begin step");

    // Attacker attempts to traverse out of workspace root: ../../escape.txt
    let escape_target = temp_root.join("..").join("..").join("etc_shadow_fake.txt");
    let res = mgr.record_mutation_before_write("step_jailbreak", &escape_target);

    assert!(res.is_err(), "Path traversal must be blocked by WorkspaceManager");
    match res.unwrap_err() {
        ReflexionError::PathEscaped(escaped, root) => {
            assert_eq!(root, temp_root);
            assert_eq!(escaped, escape_target);
        }
        other => panic!("Expected PathEscaped error, got {:?}", other),
    }

    let _ = fs::remove_dir_all(&temp_root);
}
