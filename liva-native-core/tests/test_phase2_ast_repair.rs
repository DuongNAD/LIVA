//! Phase 2 Integration Tests: Features 5-6
//! - Feature 5: Token-Aware JSON AST Self-Healing Engine (<0.1ms) (RFC-003 R2)
//! - Feature 6: Reflexion Loop & Transactional Workspace Rollback (RFC-003 R2)

use liva_native_core::ast_repair::json_repair::{repair_json_ast, AstRepairError};
use liva_native_core::ast_repair::reflexion::{
    ReflexionError, WorkspaceManager, MAX_REFLEXION_RETRIES,
};
use serde_json::{json, Value};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;

// ============================================================================
// FEATURE 5: TOKEN-AWARE JSON AST REPAIR (<0.1ms) (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Trailing Commas in Objects and Arrays
#[test]
fn test_f5_tier1_trailing_commas() {
    let malformed = r#"{ "action": "search", "filters": ["rust", "wasm",], "count": 10, }"#;
    let repaired = repair_json_ast(malformed).expect("Repair failed");

    assert_eq!(repaired["action"], "search");
    assert_eq!(repaired["filters"], json!(["rust", "wasm"]));
    assert_eq!(repaired["count"], 10);
}

/// Tier 1.2: Unquoted Object Keys
#[test]
fn test_f5_tier1_unquoted_keys() {
    let malformed = r#"{ command: "cargo_test", timeout_ms: 5000, force: true }"#;
    let repaired = repair_json_ast(malformed).expect("Repair failed");

    assert_eq!(repaired["command"], "cargo_test");
    assert_eq!(repaired["timeout_ms"], 5000);
    assert_eq!(repaired["force"], true);
}

/// Tier 1.3: Single Quotes Normalization
#[test]
fn test_f5_tier1_single_quotes_normalization() {
    let malformed = r#"{ 'name': 'git_commit_helper', 'version': '1.0.0' }"#;
    let repaired = repair_json_ast(malformed).expect("Repair failed");

    assert_eq!(repaired["name"], "git_commit_helper");
    assert_eq!(repaired["version"], "1.0.0");
}

/// Tier 1.4: Stack Auto-Completion for Truncated JSON
#[test]
fn test_f5_tier1_truncated_json_auto_completion() {
    let truncated = r#"{"task": "build", "options": {"release": true, "features": ["simd", "gpu""#;
    let repaired = repair_json_ast(truncated).expect("Repair failed");

    assert_eq!(repaired["task"], "build");
    assert_eq!(repaired["options"]["release"], true);
    assert_eq!(repaired["options"]["features"], json!(["simd", "gpu"]));
}

/// Tier 1.5: Pythonic & JS Literals (None, True, False, undefined)
#[test]
fn test_f5_tier1_python_js_literals() {
    let malformed = r#"{ "debug": True, "active": False, "extra": None, "pending": undefined }"#;
    let repaired = repair_json_ast(malformed).expect("Repair failed");

    assert_eq!(repaired["debug"], true);
    assert_eq!(repaired["active"], false);
    assert_eq!(repaired["extra"], Value::Null);
    assert_eq!(repaired["pending"], Value::Null);
}

/// Tier 2.1: Sub-0.1ms Latency Verification (<100 microseconds)
#[test]
fn test_f5_tier2_sub_0_1ms_performance() {
    let malformed = r#"{ action: 'run', target: "x86_64", flags: ['-O3', 'debug',], timeout: None, }"#;

    // Warm-up
    for _ in 0..50 {
        let _ = repair_json_ast(malformed);
    }

    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        let res = repair_json_ast(malformed).unwrap();
        assert_eq!(res["action"], "run");
    }
    let total_elapsed = start.elapsed();
    let avg_micros = total_elapsed.as_micros() as f64 / iterations as f64;

    // Must be well under 100 microseconds (0.1ms)
    assert!(avg_micros < 100.0, "Average repair latency was {}µs (>100µs SLA)", avg_micros);
}

/// Tier 2.2: Unescaped Internal Quotes in Strings
#[test]
fn test_f5_tier2_unescaped_internal_quotes() {
    let malformed = r#"{"quote": "He said "Hello world" to everyone", "author": "Alice"}"#;
    let repaired = repair_json_ast(malformed).expect("Repair failed");

    assert_eq!(repaired["quote"], "He said \"Hello world\" to everyone");
    assert_eq!(repaired["author"], "Alice");
}

/// Tier 2.3: Deeply Nested Truncated JSON (Depth > 15)
#[test]
fn test_f5_tier2_deeply_nested_truncated() {
    let truncated = String::from("{\"l1\": {\"l2\": {\"l3\": {\"l4\": {\"l5\": {\"l6\": {\"l7\": {\"l8\": {\"l9\": {\"l10\": [1, 2, [3, [4, [5");
    let repaired = repair_json_ast(&truncated).expect("Deeply nested repair failed");

    assert_eq!(repaired["l1"]["l2"]["l3"]["l4"]["l5"]["l6"]["l7"]["l8"]["l9"]["l10"][0], 1);
    assert_eq!(repaired["l1"]["l2"]["l3"]["l4"]["l5"]["l6"]["l7"]["l8"]["l9"]["l10"][2][1][0], 4);
}

/// Tier 2.4: Markdown Code Blocks & Preamble Prose Stripping
#[test]
fn test_f5_tier2_markdown_and_prose_stripping() {
    let response = r#"
Here is the tool invocation you requested:
```json
{
    "tool": "file_writer",
    "parameters": {
        "path": "src/main.rs",
        "content": "fn main() {}",
    }
}
```
Let me know if you need any adjustments.
"#;
    let repaired = repair_json_ast(response).expect("Markdown stripping failed");
    assert_eq!(repaired["tool"], "file_writer");
    assert_eq!(repaired["parameters"]["path"], "src/main.rs");
    assert_eq!(repaired["parameters"]["content"], "fn main() {}");
}

/// Tier 2.5: Missing Commas and Colon Substitutions (= or =>)
#[test]
fn test_f5_tier2_missing_commas_and_equals_colons() {
    let malformed = r#"{"a": 100 "b": 200 "c": [1 2 3]}"#;
    let repaired = repair_json_ast(malformed).expect("Missing comma repair failed");
    assert_eq!(repaired["a"], 100);
    assert_eq!(repaired["b"], 200);
    assert_eq!(repaired["c"], json!([1, 2, 3]));
}

/// Tier 2.6: Comments (Line and Block) Removal
#[test]
fn test_f5_tier2_comments_removal() {
    let malformed = r#"{
        // This is a single-line comment
        "endpoint": "/api/v1/status",
        /* Block comment
           describing the payload */
        "active": true,
    }"#;
    let repaired = repair_json_ast(malformed).expect("Comment stripping failed");
    assert_eq!(repaired["endpoint"], "/api/v1/status");
    assert_eq!(repaired["active"], true);
}

/// Tier 2.7: Error Paths (Empty Input & Gibberish)
#[test]
fn test_f5_tier2_error_paths() {
    assert_eq!(repair_json_ast("").unwrap_err(), AstRepairError::EmptyInput);
    assert_eq!(repair_json_ast("   \n\t  ").unwrap_err(), AstRepairError::EmptyInput);

    let gibberish = "There is no json anywhere in this pure text sentence.";
    let res = repair_json_ast(gibberish);
    assert!(res.is_err());
    match res.unwrap_err() {
        AstRepairError::NoJsonFound(_) => {}
        other => panic!("Expected NoJsonFound, got: {:?}", other),
    }
}

// ============================================================================
// FEATURE 6: REFLEXION LOOP & WORKSPACE ROLLBACK (Tier 1 & Tier 2)
// ============================================================================

fn setup_temp_workspace() -> (tempfile::TempDir, WorkspaceManager) {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let manager = WorkspaceManager::new(temp_dir.path());
    (temp_dir, manager)
}

/// Tier 1.1: Snapshot Modification and Rollback of Existing Files
#[test]
fn test_f6_tier1_modify_and_rollback_file() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let file_path = temp_dir.path().join("code.rs");

    // Initial content
    fs::write(&file_path, b"fn original() {}").unwrap();

    // Begin step 1
    manager.begin_step("step_1").unwrap();

    // Mutate file
    fs::write(&file_path, b"fn modified_with_bug() {}").unwrap();

    // Rollback step 1
    manager.rollback_step("step_1").expect("Rollback failed");

    // Content should be restored to original
    let restored = fs::read_to_string(&file_path).unwrap();
    assert_eq!(restored, "fn original() {}");
}

/// Tier 1.2: Rollback of Newly Created Files
#[test]
fn test_f6_tier1_created_files_deleted_on_rollback() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let new_file_path = temp_dir.path().join("temp_artifact.tmp");

    manager.begin_step("step_create").unwrap();

    // Create file
    fs::write(&new_file_path, b"temporary data").unwrap();
    manager.record_creation("step_create", &new_file_path).unwrap();
    assert!(new_file_path.exists());

    // Rollback step
    manager.rollback_step("step_create").expect("Rollback failed");

    // File must be deleted
    assert!(!new_file_path.exists(), "Created file should be removed on rollback");
}

/// Tier 1.3: Rollback of Deleted Files
#[test]
fn test_f6_tier1_deleted_files_restored_on_rollback() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let file_path = temp_dir.path().join("important_config.toml");
    fs::write(&file_path, b"[config]\nmode = 'prod'").unwrap();

    manager.begin_step("step_del").unwrap();

    // Delete file
    manager.record_deletion("step_del", &file_path).unwrap();
    assert!(!file_path.exists());

    // Rollback step
    manager.rollback_step("step_del").expect("Rollback failed");

    // File must be restored
    assert!(file_path.exists());
    let restored = fs::read_to_string(&file_path).unwrap();
    assert_eq!(restored, "[config]\nmode = 'prod'");
}

/// Tier 1.4: Commit Step Makes Changes Permanent
#[test]
fn test_f6_tier1_commit_step_preserves_changes() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let file_path = temp_dir.path().join("committed.txt");
    fs::write(&file_path, b"initial").unwrap();

    manager.begin_step("step_commit").unwrap();
    fs::write(&file_path, b"committed permanent change").unwrap();

    manager.commit_step("step_commit").expect("Commit failed");

    assert_eq!(manager.committed_steps(), vec!["step_commit"]);

    let content = fs::read_to_string(&file_path).unwrap();
    assert_eq!(content, "committed permanent change");
}

/// Tier 1.5: Reflexion Retry Formulation
#[test]
fn test_f6_tier1_reflexion_retry_feedback() {
    let original_prompt = "cargo build --target wasm32-wasip1";
    let error_observation = "error[E0432]: unresolved import `std::os::unix`";

    let feedback = WorkspaceManager::formulate_reflexion_retry_prompt(
        original_prompt,
        error_observation,
        1,
    );

    assert!(feedback.contains("Execution attempt 1 failed"));
    assert!(feedback.contains("error[E0432]"));
    assert!(feedback.contains("cargo build"));
}

/// Tier 2.1: Path Jailbreak Outside Workspace Root Blocked
#[test]
fn test_f6_tier2_path_jailbreak_blocked() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let escaped_path = PathBuf::from("/etc/passwd");

    manager.begin_step("step_escape").unwrap();

    let res = manager.record_modification("step_escape", &escaped_path);
    assert!(res.is_err());
    match res.unwrap_err() {
        ReflexionError::PathEscaped(escaped, root) => {
            assert_eq!(escaped, escaped_path);
            assert_eq!(root, temp_dir.path());
        }
        other => panic!("Expected PathEscaped error, got: {:?}", other),
    }
}

/// Tier 2.2: Rollback Non-Existent Step Error
#[test]
fn test_f6_tier2_non_existent_step_error() {
    let (_temp_dir, mut manager) = setup_temp_workspace();

    let res = manager.rollback_step("step_unknown");
    assert!(res.is_err());
    match res.unwrap_err() {
        ReflexionError::SnapshotNotFound(id) => {
            assert_eq!(id, "step_unknown");
        }
        other => panic!("Expected SnapshotNotFound, got: {:?}", other),
    }
}

/// Tier 2.3: Max Retries Exceeded Enforcement (Constant = 3)
#[test]
fn test_f6_tier2_max_retries_exceeded() {
    assert_eq!(MAX_REFLEXION_RETRIES, 3);

    let (_temp_dir, manager) = setup_temp_workspace();
    let check_1 = manager.check_retry_limit("step_retry", 1);
    assert!(check_1.is_ok());

    let check_3 = manager.check_retry_limit("step_retry", 3);
    assert!(check_3.is_ok());

    let check_4 = manager.check_retry_limit("step_retry", 4);
    assert!(check_4.is_err());
    match check_4.unwrap_err() {
        ReflexionError::MaxRetriesExceeded(retries, step) => {
            assert_eq!(retries, 4);
            assert_eq!(step, "step_retry");
        }
        other => panic!("Expected MaxRetriesExceeded, got: {:?}", other),
    }
}

/// Tier 2.4: Concurrent Steps on Isolated Paths
#[test]
fn test_f6_tier2_concurrent_isolated_steps() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let file_a = temp_dir.path().join("a.txt");
    let file_b = temp_dir.path().join("b.txt");

    fs::write(&file_a, b"orig_a").unwrap();
    fs::write(&file_b, b"orig_b").unwrap();

    // Step A: Modifies file_a and rolls back
    manager.begin_step("step_a").unwrap();
    fs::write(&file_a, b"mut_a").unwrap();
    manager.rollback_step("step_a").unwrap();
    assert_eq!(fs::read_to_string(&file_a).unwrap(), "orig_a");

    // Step B: Modifies file_b and commits
    manager.begin_step("step_b").unwrap();
    fs::write(&file_b, b"mut_b").unwrap();
    manager.commit_step("step_b").unwrap();
    assert_eq!(fs::read_to_string(&file_b).unwrap(), "mut_b");
}

/// Tier 2.5: Nested Subdirectory Cleanup on Rollback
#[test]
fn test_f6_tier2_nested_directory_cleanup() {
    let (temp_dir, mut manager) = setup_temp_workspace();
    let sub_dir = temp_dir.path().join("nested").join("deep");
    let nested_file = sub_dir.join("artifact.bin");

    manager.begin_step("step_nested").unwrap();

    fs::create_dir_all(&sub_dir).unwrap();
    fs::write(&nested_file, b"binary_data_0x1234").unwrap();
    manager.record_creation("step_nested", &nested_file).unwrap();

    assert!(nested_file.exists());

    // Rollback deletes file
    manager.rollback_step("step_nested").unwrap();
    assert!(!nested_file.exists());
}
