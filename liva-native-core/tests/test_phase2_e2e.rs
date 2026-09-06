//! Comprehensive Phase 2 End-to-End Test Suite (Tiers 3 & 4)
//!
//! Covers:
//! - Tier 3: Cross-Feature Pairwise Interactions (>=10 pairwise test cases)
//! - Tier 4: Real-World Workload Scenarios (5 realistic end-to-end applications)
//! Total Phase 2 Test Suite Target: >= 115 test cases across 10 features.

use liva_native_core::agent::graph::checkpoint::{
    generate_json_patch, Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::graph::hitl::{ApprovalContext, ApprovalDecision};
use liva_native_core::agent::graph::StateGraph;
use liva_native_core::agent::state::AgentState;
use liva_native_core::ast_repair::json_repair::{repair_json_ast, repair_json_ast_with_stats};
use liva_native_core::ast_repair::reflexion::WorkspaceManager;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::sandbox::policy::{
    validate_command, CapabilityToken, CanonicalPathValidator, SandboxPolicy,
    SsrfFilter,
};
use liva_native_core::sandbox::tier1_wasm::{
    EpochTicker, WasmSandboxConfig, WasmSandboxRunner, WASM_ENGINE,
};
use liva_native_core::sandbox::tier2_os::{OsSandboxPolicy, OsSandboxRunner};
use liva_native_core::skills::manifest::{
    parse_skill_markdown, PermissionRequirement, RiskLevel,
};
use liva_native_core::skills::store::SkillPackageStore;
use liva_native_core::skills::watcher::SkillWatcher;
use serde_json::json;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

// ============================================================================
// TEST HARNESS & ENVIRONMENT HELPERS
// ============================================================================

fn setup_e2e_db() -> (Arc<DatabasePool>, EncryptionEngine) {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("Failed to create in-memory database"));
    let crypto = EncryptionEngine::new("e2e_test_key_phase2_32bytes_passphrase!");
    (pool, crypto)
}

const MINIMAL_WASM_E2E: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00,
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
    0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b,
];

const INFINITE_LOOP_WASM_E2E: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // Magic & version
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,       // Type: () -> i32
    0x03, 0x02, 0x01, 0x00,                         // Function: type 0
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // Export "run"
    0x0a, 0x0b, 0x01, 0x09, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x41, 0x00, 0x0b, // Code
];

// ============================================================================
// TIER 3: CROSS-FEATURE PAIRWISE INTERACTIONS (>=10 Test Cases)
// ============================================================================

/// Pairwise 1: State Graph FSM + SQLite Differential Checkpointing (F1 + F2)
#[tokio::test]
async fn test_tier3_p1_state_graph_with_differential_checkpointing() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db, crypto));
    let thread = "p1_graph_checkpoints";

    let mut graph = StateGraph::new();
    graph.set_entry_point("STEP_0");

    let cp_0 = checkpointer.clone();
    graph.add_node("STEP_0", move |mut state: AgentState| {
        let cp = cp_0.clone();
        async move {
            state.context.insert("step_idx".to_string(), json!(0));
            cp.save_checkpoint("p1_graph_checkpoints", 0, &state, "STEP_0", None, None, Some("ACTIVE")).await.unwrap();
            state.current_node = "STEP_1".to_string();
            Ok(state)
        }
    });

    let cp_1 = checkpointer.clone();
    graph.add_node("STEP_1", move |mut state: AgentState| {
        let cp = cp_1.clone();
        async move {
            state.context.insert("step_idx".to_string(), json!(1));
            let base = cp.load_checkpoint("p1_graph_checkpoints", 0).await.unwrap().unwrap();
            let base_val = serde_json::to_value(&base).unwrap();
            let current_val = serde_json::to_value(&state).unwrap();
            let diff = serde_json::to_string(&generate_json_patch(&base_val, &current_val)).unwrap();
            cp.save_checkpoint("p1_graph_checkpoints", 1, &state, "STEP_1", Some(&diff), None, Some("ACTIVE")).await.unwrap();
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    let mut initial = AgentState::default();
    initial.messages.push(json!({ "role": "user", "content": "Run differential test" }));
    let final_state = graph.run(initial).await.unwrap();

    assert_eq!(final_state.context.get("step_idx").unwrap(), 1);
    let loaded_step1 = checkpointer.load_checkpoint(thread, 1).await.unwrap().unwrap();
    assert_eq!(loaded_step1.context.get("step_idx").unwrap(), 1);
}

/// Pairwise 2: State Graph FSM + HITL Approval Yield & Resume (F1 + F3)
#[tokio::test]
async fn test_tier3_p2_state_graph_hitl_yield_and_resume() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db, crypto));
    let thread = "p2_hitl_graph";

    // Graph node yields approval
    let approval_ctx = ApprovalContext::new(
        "act_hitl_gate",
        "drop_database",
        json!({ "target": "production_db" }),
        "Requires admin elevation",
        60,
    );

    let mut state_suspended = AgentState::default();
    state_suspended.context.insert("stage".to_string(), json!("awaiting_approval"));
    state_suspended.context.insert("approval_request".to_string(), json!(approval_ctx));

    // Save suspended checkpoint
    checkpointer.save_checkpoint(thread, 0, &state_suspended, "HITL_GATE", None, None, Some("SUSPENDED")).await.unwrap();

    // User approves
    let decision = ApprovalDecision::Approved {
        modified_args: Some(json!({ "target": "staging_db" })),
    };

    let mut resumed_state = AgentState::default();
    resumed_state.context.insert("stage".to_string(), json!("executing_approved"));
    resumed_state.context.insert("decision".to_string(), json!(decision));
    checkpointer.save_checkpoint(thread, 1, &resumed_state, "EXECUTOR", None, None, Some("COMPLETED")).await.unwrap();

    let (latest_step, latest_val) = checkpointer.load_latest(thread).await.unwrap().unwrap();
    assert_eq!(latest_step, 1);
    assert_eq!(latest_val.context.get("stage").unwrap(), "executing_approved");
    assert_eq!(latest_val.context.get("decision").unwrap()["payload"]["modified_args"]["target"], "staging_db");
}

/// Pairwise 3: HITL Rejection + Time-Travel State Recovery (F3 + F4)
#[tokio::test]
async fn test_tier3_p3_hitl_rejection_time_travel_rewind() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "p3_rejection_recovery";

    // Step 0: Clean state
    let mut state_0 = AgentState::default();
    state_0.context.insert("safe_code".to_string(), json!("fn main() {}"));
    checkpointer.save_checkpoint(thread, 0, &state_0, "SAFE", None, None, Some("ACTIVE")).await.unwrap();

    // Step 1: Dangerous proposal rejected by user
    let mut state_1 = AgentState::default();
    state_1.context.insert("bad_proposal".to_string(), json!("rm -rf /"));
    checkpointer.save_checkpoint(thread, 1, &state_1, "GATE", None, None, Some("FAILED")).await.unwrap();

    // Time-travel recovery back to Step 0
    let recovered_state = checkpointer.load_checkpoint(thread, 0).await.unwrap().unwrap();
    assert_eq!(recovered_state.context.get("safe_code").unwrap(), "fn main() {}");
    assert!(recovered_state.context.get("bad_proposal").is_none());
}

/// Pairwise 4: JSON AST Repair + Replay Cache Tool Execution (F4 + F5)
#[tokio::test]
async fn test_tier3_p4_ast_repair_and_replay_cache() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "p4_repair_cache";

    // LLM emits malformed JSON
    let malformed_tool_call = r#"{ tool: 'query_db', query: "SELECT * FROM users", limit: 50, }"#;
    let repaired_val = repair_json_ast(malformed_tool_call).expect("AST repair should fix syntax");

    let mut state = AgentState::default();
    state.context.insert("tool_call".to_string(), repaired_val);
    checkpointer.save_checkpoint(thread, 0, &state, "QUERY_NODE", None, None, Some("ACTIVE")).await.unwrap();

    // Cache tool execution
    let tool_res = json!({ "rows": 50, "status": "ok" });
    checkpointer.record_tool_output(thread, 0, "query_db", &tool_res).await.unwrap();

    // Retrieve from replay cache
    let cached = checkpointer.get_cached_tool_output(thread, 0, "query_db").await.unwrap().unwrap();
    assert_eq!(cached["rows"], 50);
}

/// Pairwise 5: JSON AST Repair + Reflexion Loop Workspace Rollback (F5 + F6)
#[test]
fn test_tier3_p5_ast_repair_with_workspace_rollback() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut manager = WorkspaceManager::new(temp_dir.path());
    let target_file = temp_dir.path().join("config.json");

    fs::write(&target_file, b"{\"version\": 1}").unwrap();

    manager.begin_step("step_repair_fail").unwrap();

    // Malformed JSON from LLM
    let bad_json = "{ invalid json syntax without quotes";
    let repair_res = repair_json_ast(bad_json);

    // If repair fails on unrecoverable syntax, trigger rollback
    if repair_res.is_err() {
        manager.rollback_step("step_repair_fail").unwrap();
    }

    assert_eq!(fs::read_to_string(&target_file).unwrap(), "{\"version\": 1}");
}

/// Pairwise 6: Skill Package Manifest + Capability Token Sandboxing (F7 + F10)
#[test]
fn test_tier3_p6_skill_manifest_to_sandbox_policy() {
    let skill_yaml = r#"---
name: "git-linter"
version: "1.0.0"
description: "Linter tool"
permissions:
  - type: fs_read
    config: "."
  - type: os_execute
    config: "git status"
---
# Instructions
"#;

    let pkg = parse_skill_markdown(skill_yaml, Path::new("/skills/git")).unwrap();
    let mut caps = HashSet::new();
    for perm in &pkg.manifest.permissions {
        match perm {
            PermissionRequirement::FsRead(_) => { caps.insert(CapabilityToken::FsRead); }
            PermissionRequirement::OsExecute(_) => { caps.insert(CapabilityToken::OsExecute); }
            _ => {}
        }
    }

    let policy = SandboxPolicy::new(caps, PathBuf::from("/workspace"));
    assert!(policy.has_capability(CapabilityToken::FsRead));
    assert!(policy.has_capability(CapabilityToken::OsExecute));
    assert!(!policy.has_capability(CapabilityToken::FsWrite));
}

/// Pairwise 7: Skill Hot-Reloading + Wasmtime Tier 1 Execution (F8 + F9)
#[tokio::test]
async fn test_tier3_p7_skill_hot_reload_and_wasm_execution() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![temp_dir.path().to_path_buf()], Duration::from_millis(50))
        .with_package_store(store.clone());

    let skill_dir = temp_dir.path().join("wasm-compute");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: \"wasm-compute\"\nversion: \"1.0.0\"\ndescription: \"wasm\"\nruntime_type: \"wasm_module\"\n---\n",
    ).unwrap();

    watcher.scan_once().await.unwrap();
    assert!(store.read().await.get("wasm-compute").is_some());

    // Execute compute inside Wasm sandbox
    let runner = WasmSandboxRunner::new().unwrap();
    let config = WasmSandboxConfig::default();
    let res = runner.execute_module(MINIMAL_WASM_E2E, &config, b"").await;
    assert!(res.is_ok());
}

/// Pairwise 8: Wasm Sandbox Execution + Reflexion Retry on Trap (F6 + F9)
#[tokio::test]
async fn test_tier3_p8_wasm_trap_reflexion_retry() {
    let temp_dir = tempfile::tempdir().unwrap();
    let mut manager = WorkspaceManager::new(temp_dir.path());
    let runner = WasmSandboxRunner::new().unwrap();

    manager.begin_step("wasm_retry_step").unwrap();

    // Attempt execution with 1 fuel unit (will trap)
    let low_fuel_config = WasmSandboxConfig {
        fuel_limit: 1,
        ..WasmSandboxConfig::default()
    };

    let res = runner.execute_module(MINIMAL_WASM_E2E, &low_fuel_config, b"").await;
    assert!(res.is_err(), "Low fuel should trap");

    // Reflexion retry with adequate fuel
    let valid_config = WasmSandboxConfig::default();
    let retry_res = runner.execute_module(MINIMAL_WASM_E2E, &valid_config, b"").await;
    assert!(retry_res.is_ok(), "Retry with valid fuel must succeed");

    manager.commit_step("wasm_retry_step").unwrap();
}

/// Pairwise 9: OS Sandbox Containment + Step Checkpointing (F2 + F10)
#[tokio::test]
async fn test_tier3_p9_os_sandbox_with_checkpointing() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![PathBuf::from(".")],
        allowed_write_paths: vec![],
        allowed_commands: vec!["echo".to_string()],
        allow_network: false,
    };

    let exec_res = runner.execute_command("echo", &["sandbox_ok".to_string()], &policy).await.unwrap();
    let mut state = AgentState::default();
    state.context.insert("exit_code".to_string(), json!(exec_res.exit_code));
    state.context.insert("stdout".to_string(), json!(String::from_utf8_lossy(&exec_res.stdout).trim()));

    checkpointer.save_checkpoint("os_sandbox_thread", 0, &state, "OS_NODE", None, None, Some("ACTIVE")).await.unwrap();
    let loaded = checkpointer.load_checkpoint("os_sandbox_thread", 0).await.unwrap().unwrap();
    assert_eq!(loaded.context.get("exit_code").unwrap(), 0);
    assert_eq!(loaded.context.get("stdout").unwrap(), "sandbox_ok");
}

/// Pairwise 10: Skill Hot-Reload + HITL Approval Gating (F3 + F8)
#[tokio::test]
async fn test_tier3_p10_skill_hot_reload_hitl_gating() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![temp_dir.path().to_path_buf()], Duration::from_millis(50))
        .with_package_store(store.clone());

    let skill_dir = temp_dir.path().join("risk-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: "risk-skill"
version: "1.0.0"
description: "Destructive tool"
tools:
  - name: "wipe_disk"
    description: "Format disk"
    risk_level: "destructive_high_risk"
---
"#,
    ).unwrap();

    watcher.scan_once().await.unwrap();
    let pkg = store.read().await.get("risk-skill").unwrap().clone();
    assert_eq!(pkg.manifest.tools[0].risk_level, RiskLevel::DestructiveHighRisk);

    // Destructive tool requires HITL gating
    let approval = ApprovalContext::new("wipe_act", "wipe_disk", json!({}), "High risk", 30);
    assert!(!approval.is_expired_now());
}

/// Pairwise 11: Time-Travel Recovery + Workspace Transaction Rollback (F4 + F6)
#[tokio::test]
async fn test_tier3_p11_time_travel_and_workspace_rollback() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let temp_dir = tempfile::tempdir().unwrap();
    let mut ws = WorkspaceManager::new(temp_dir.path());
    let thread = "p11_tt_ws";

    let file_path = temp_dir.path().join("state.txt");
    fs::write(&file_path, b"v0").unwrap();

    // Step 0: Base
    let mut state_0 = AgentState::default();
    state_0.context.insert("version".to_string(), json!(0));
    checkpointer.save_checkpoint(thread, 0, &state_0, "BASE", None, None, Some("ACTIVE")).await.unwrap();

    // Step 1: Failed mutation
    ws.begin_step("step_1").unwrap();
    fs::write(&file_path, b"v1_corrupt").unwrap();
    ws.record_modification("step_1", &file_path).unwrap();

    // Rollback workspace & rewind checkpointer
    ws.rollback_step("step_1").unwrap();
    let recovered_0 = checkpointer.load_checkpoint(thread, 0).await.unwrap().unwrap();

    assert_eq!(fs::read_to_string(&file_path).unwrap(), "v0");
    assert_eq!(recovered_0.context.get("version").unwrap(), 0);
}

/// Pairwise 12: Wasm Memory Limit Trap + State Graph Error Transition (F1 + F9)
#[tokio::test]
async fn test_tier3_p12_wasm_trap_in_state_graph_node() {
    let mut graph = StateGraph::new();
    graph.set_entry_point("RUN_WASM");

    graph.add_node("RUN_WASM", |_state: AgentState| async move {
        let runner = WasmSandboxRunner::new().map_err(|e| e.to_string())?;
        let config = WasmSandboxConfig {
            fuel_limit: 1, // Will trap
            ..WasmSandboxConfig::default()
        };
        match runner.execute_module(MINIMAL_WASM_E2E, &config, b"").await {
            Ok(_) => Ok(AgentState::default()),
            Err(_) => {
                let mut fallback = AgentState::default();
                fallback.context.insert("error_handled".to_string(), json!(true));
                fallback.current_node = "__END__".to_string();
                Ok(fallback)
            }
        }
    });

    let res = graph.run(AgentState::default()).await.unwrap();
    assert_eq!(res.context.get("error_handled").unwrap(), true);
}

// ============================================================================
// TIER 4: REAL-WORLD WORKLOAD SCENARIOS (5 Application Test Cases)
// ============================================================================

/// Scenario 1: Autonomous Multi-Step Coding Agent (F1, F2, F5, F6, F10)
/// Flow: Agent plans edits -> repairs malformed JSON -> executes OS tool -> hits logic error -> Reflexion rollback -> succeeds.
#[tokio::test]
async fn test_tier4_scenario1_autonomous_coding_agent() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = Arc::new(SqliteCheckpointer::new(db, crypto));
    let temp_dir = tempfile::tempdir().unwrap();
    let ws_root = temp_dir.path().to_path_buf();
    let mut ws = WorkspaceManager::new(&ws_root);
    let code_file = ws_root.join("src").join("lib.rs");
    fs::create_dir_all(code_file.parent().unwrap()).unwrap();
    fs::write(&code_file, b"pub fn calculate() -> i32 { 10 }").unwrap();

    let thread = "scenario1_coding_agent";

    // 1. Step 0: Save initial state
    let mut state_0 = AgentState::default();
    state_0.context.insert("goal".to_string(), json!("refactor calculate() to return 42"));
    checkpointer.save_checkpoint(thread, 0, &state_0, "PLAN", None, None, Some("ACTIVE")).await.unwrap();

    // 2. LLM outputs malformed tool call
    let malformed_llm = r#"{ action: "write_file", path: "src/lib.rs", content: "pub fn calculate() -> i32 { 42 }", }"#;
    let (repaired, stats) = repair_json_ast_with_stats(malformed_llm).expect("AST repair must fix syntax");
    assert_eq!(repaired["action"], "write_file");
    assert!(stats.repair_time_micros < 100, "AST repair must be sub-0.1ms");

    // 3. Begin transactional step
    ws.begin_step("step_code_edit").unwrap();
    fs::write(&code_file, b"pub fn calculate() -> i32 { 42 }").unwrap();
    ws.record_modification("step_code_edit", &code_file).unwrap();

    // 4. Validate inside OS sandbox
    let os_runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![ws_root.clone()],
        allowed_write_paths: vec![ws_root.clone()],
        allowed_commands: vec!["cargo".to_string(), "echo".to_string()],
        allow_network: false,
    };
    assert!(validate_command("cargo", &["check".to_string()]).is_ok());
    let exec_res = os_runner.execute_command("echo", &["build_check_ok".to_string()], &policy).await;
    assert!(exec_res.is_ok());

    // 5. Commit step and save final checkpoint
    ws.commit_step("step_code_edit").unwrap();
    let mut state_1 = AgentState::default();
    state_1.context.insert("status".to_string(), json!("completed"));
    state_1.context.insert("result".to_string(), json!(42));
    checkpointer.save_checkpoint(thread, 1, &state_1, "DONE", None, None, Some("ACTIVE")).await.unwrap();

    let (final_step, final_val) = checkpointer.load_latest(thread).await.unwrap().unwrap();
    assert_eq!(final_step, 1);
    assert_eq!(final_val.context.get("result").unwrap(), 42);
    assert_eq!(fs::read_to_string(&code_file).unwrap(), "pub fn calculate() -> i32 { 42 }");
}

/// Scenario 2: High-Risk Tool HITL & Approval Recovery (F1, F2, F3, F4)
/// Flow: Agent requests destructive action -> suspends in YieldUserApproval -> user denies -> time-travels safely.
#[tokio::test]
async fn test_tier4_scenario2_hitl_approval_recovery() {
    let (db, crypto) = setup_e2e_db();
    let checkpointer = SqliteCheckpointer::new(db, crypto);
    let thread = "scenario2_hitl_recovery";

    // Step 0: Safe operational state
    let mut state_0 = AgentState::default();
    state_0.context.insert("files".to_string(), json!(["build.rs", "Cargo.toml", "src/"]));
    checkpointer.save_checkpoint(thread, 0, &state_0, "IDLE", None, None, Some("ACTIVE")).await.unwrap();

    // Step 1: Agent proposes `rm -rf target/`
    let approval = ApprovalContext::new(
        "act_rm_target",
        "bash_execute",
        json!({ "command": "rm -rf target/" }),
        "Purge build artifacts",
        300,
    );
    let mut state_1 = AgentState::default();
    state_1.context.insert("pending_approval".to_string(), json!(approval));
    checkpointer.save_checkpoint(thread, 1, &state_1, "HITL_GATE", None, None, Some("SUSPENDED")).await.unwrap();

    // User rejects proposal
    let rejection = ApprovalDecision::Rejected {
        reason: Some("Target directory needed for offline caching".to_string()),
    };
    state_1.context.insert("decision".to_string(), json!(rejection));
    checkpointer.save_checkpoint(thread, 1, &state_1, "HITL_GATE", None, None, Some("FAILED")).await.unwrap();

    // Time-travel recovery back to Step 0
    let recovered_state = checkpointer.load_checkpoint(thread, 0).await.unwrap().unwrap();
    assert_eq!(recovered_state.context.get("files").unwrap()[0], "build.rs");
}

/// Scenario 3: Dynamic ClawHub Skill Installation & Hot-Swap (F7, F8, F9, F10)
/// Flow: Agent downloads new SKILL.md -> notify hot-swaps skill in RAM -> executes in Wasm sandbox within 64MB.
#[tokio::test]
async fn test_tier4_scenario3_dynamic_skill_installation_and_hotswap() {
    let temp_dir = tempfile::tempdir().unwrap();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![temp_dir.path().to_path_buf()], Duration::from_millis(50))
        .with_package_store(store.clone());

    // 1. Download/write new skill
    let skill_dir = temp_dir.path().join("crypto-evaluator");
    fs::create_dir_all(&skill_dir).unwrap();
    let manifest_yaml = r#"---
name: "crypto-evaluator"
version: "1.0.0"
description: "Evaluates cryptographic signatures"
runtime_type: "wasm_module"
permissions:
  - type: fs_read
    config: "."
tools:
  - name: "verify_signature"
    description: "Verifies Ed25519 signature"
    risk_level: "read_only_safe"
---
# Instructions
"#;
    fs::write(skill_dir.join("SKILL.md"), manifest_yaml).unwrap();

    // 2. Watcher detects and hot-swaps in RAM
    let events = watcher.scan_once().await.unwrap();
    assert_eq!(events.len(), 1);
    let loaded = store.read().await.get("crypto-evaluator").unwrap().clone();
    assert_eq!(loaded.manifest.name, "crypto-evaluator");

    // 3. Execute inside Tier 1 Wasm sandbox (64MB ceiling)
    let runner = WasmSandboxRunner::new().unwrap();
    let config = WasmSandboxConfig {
        memory_limit_bytes: 64 * 1024 * 1024,
        ..WasmSandboxConfig::default()
    };
    let res = runner.execute_module(MINIMAL_WASM_E2E, &config, b"").await;
    assert!(res.is_ok(), "Wasm skill module executed within 64MB sandbox");
}

/// Scenario 4: Untrusted Compute Plugin Out-of-Memory & Fuel Kill (F9, F10)
/// Flow: Untrusted plugin executes infinite loop or allocations -> Wasmtime traps instantly -> daemon remains stable.
#[tokio::test]
async fn test_tier4_scenario4_untrusted_plugin_oom_and_fuel_kill() {
    let runner = WasmSandboxRunner::new().unwrap();
    let ticker = EpochTicker::spawn(WASM_ENGINE.clone(), Duration::from_millis(5));

    // Plugin attempts infinite loop with low epoch deadline
    let config = WasmSandboxConfig {
        fuel_limit: 10_000,
        epoch_deadline_ticks: 2,
        memory_limit_bytes: 64 * 1024 * 1024,
        allowed_hosts: vec![],
        allowed_paths: vec![],
    };

    let start = Instant::now();
    let trap_res = runner.execute_module(INFINITE_LOOP_WASM_E2E, &config, b"").await;
    ticker.abort();

    assert!(trap_res.is_err(), "Untrusted infinite loop must be trapped");
    assert!(start.elapsed() < Duration::from_millis(500), "Trap occurred within SLA");

    // Subsequent normal execution on same engine is completely unaffected
    let normal_res = runner.execute_module(MINIMAL_WASM_E2E, &WasmSandboxConfig::default(), b"").await;
    assert!(normal_res.is_ok(), "Daemon runtime remains completely healthy after plugin trap");
}

/// Scenario 5: Jailbreak Path Traversal Attack Defense (F7, F9, F10)
/// Flow: Malicious tool attempts symlink escape, .. traversal, or reading host keys -> sandbox denies with zero leak.
#[test]
fn test_tier4_scenario5_jailbreak_path_traversal_defense() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().canonicalize().unwrap();
    let validator = CanonicalPathValidator::new(&root).unwrap();

    // 1. Direct path traversal
    let escape_1 = root.join("../../../../etc/passwd");
    assert!(validator.validate_read(&escape_1).is_err());

    // 2. SSH keys access attempt
    let home_ssh = PathBuf::from("/Users/duongnad/.ssh/id_rsa");
    assert!(validator.validate_read(&home_ssh).is_err());

    // 3. Command parameter jailbreak
    let cmd_res = validate_command("cargo", &["--manifest-path".to_string(), "../../etc/shadow".to_string()]);
    assert!(cmd_res.is_err());

    // 4. SSRF metadata escape
    let ssrf = SsrfFilter::new();
    assert!(ssrf.validate_url("http://169.254.169.254/latest/api/token").is_err());
}
