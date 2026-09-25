#![cfg(feature = "experimental")]

use liva_native_core::evolution::{
    CodeAgent, LlmCodeAgent, Sandbox, SelfCorrectionError, SelfCorrectionLoop,
    build_code_repair_prompt, extract_code_from_response,
};
use liva_native_core::llm::LlamaRouterManager;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;

struct TempProject {
    root: PathBuf,
}

impl TempProject {
    fn create(name: &str) -> Self {
        let mut root = std::env::temp_dir();
        let rand_suffix: u64 = rand::random();
        root.push(format!("{name}_{rand_suffix}"));
        fs::create_dir_all(&root).expect("create temp project dir");
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn setup(&self, initial_code: &str) -> PathBuf {
        let cargo_toml = r#"[package]
name = "temp_eval_pkg"
version = "0.1.0"
edition = "2021"
"#;
        fs::write(self.root.join("Cargo.toml"), cargo_toml).expect("write Cargo.toml");
        let src_dir = self.root.join("src");
        fs::create_dir_all(&src_dir).expect("create src dir");
        let src_file = src_dir.join("lib.rs");
        fs::write(&src_file, initial_code).expect("write src/lib.rs");
        src_file
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        if self.root.exists() {
            let _ = fs::remove_dir_all(&self.root);
        }
    }
}

#[tokio::test]
async fn test_end_to_end_self_correction_fixes_real_compiler_error() {
    let project = TempProject::create("liva_u15_test");
    let broken_source = r#"pub fn calculate_area(width: u32, height: u32) -> u32 {
    let base: &str = "0";
    base + width * height
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_area() {
        assert_eq!(calculate_area(5, 10), 50);
    }
}
"#;

    let source_file = project.setup(broken_source);

    // Initial check: Sandbox::run_tests MUST fail on the broken code with rustc type error
    let initial_test = Sandbox::run_tests(project.path())
        .await
        .expect("sandbox execution");
    assert!(
        !initial_test.success,
        "Initial code should fail compilation, but passed"
    );
    assert!(
        initial_test.stderr.contains("cannot add")
            || initial_test.stderr.contains("E0277")
            || initial_test.stderr.contains("error"),
        "Expected rustc compiler error in stderr"
    );

    // Construct LlmCodeAgent with intelligent fix provider
    let agent = LlmCodeAgent::new_custom(|prompt| {
        // Assert prompt formatting follows standard contract
        assert!(prompt.contains("### SOURCE CODE:"));
        assert!(prompt.contains("### ERROR LOG:"));
        assert!(prompt.contains("### FIXED SOURCE CODE:"));

        // Simulate LLM output with markdown code fences
        let reply = r#"```rust
pub fn calculate_area(width: u32, height: u32) -> u32 {
    width * height
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_area() {
        assert_eq!(calculate_area(5, 10), 50);
    }
}
```"#;
        std::future::ready(Ok(reply.to_string()))
    });

    let loop_runner = SelfCorrectionLoop::with_max_retries(agent, 2);
    let result = loop_runner
        .run(project.path(), &source_file)
        .await
        .expect("self correction succeeded");

    assert!(result.success, "Cargo test must pass after patch");

    // Verify file content was updated to the fixed version
    let updated_code = fs::read_to_string(&source_file).expect("read source file");
    assert!(
        !updated_code.contains("base: &str"),
        "Broken line must be removed"
    );
    assert!(
        updated_code.contains("width * height"),
        "Fixed multiplication must be present"
    );
}

#[tokio::test]
async fn test_self_correction_exhaustion_restores_original_file() {
    let project = TempProject::create("liva_u15_restore");
    let broken_source = r#"pub fn broken() { let x = ; }"#;
    let source_file = project.setup(broken_source);

    // Agent returns an invalid fix repeatedly
    let failing_agent = LlmCodeAgent::new_custom(|_| {
        std::future::ready(Ok(
            "```rust\npub fn still_broken() { let y = ; }\n```".to_string()
        ))
    });

    let loop_runner = SelfCorrectionLoop::with_max_retries(failing_agent, 1);
    let result = loop_runner.run(project.path(), &source_file).await;

    assert!(result.is_err(), "Loop should fail when retries exhausted");
    match result.unwrap_err() {
        SelfCorrectionError::MaxRetriesExhausted(err) => {
            assert!(!err.is_empty(), "Error log must be recorded");
        }
        other => panic!("Unexpected error variant: {:?}", other),
    }

    // Source code must be restored to original broken_source by BackupGuard
    let restored = fs::read_to_string(&source_file).expect("read source file");
    assert_eq!(restored.trim(), broken_source.trim());
}

#[tokio::test]
async fn test_llm_code_agent_with_local_router_manager() {
    let mgr = LlamaRouterManager::new(2048, 0).expect("create manager");
    let mgr_arc = Arc::new(Mutex::new(mgr));

    let agent = LlmCodeAgent::new_local(mgr_arc).with_sampling(0.1, 0.9);

    // On dev/test environment with vocab-only model, complete should return controlled error
    let fix_res = agent
        .suggest_fix(
            "pub fn test() { let x: i32 = \"bad\"; }",
            "error[E0308]: mismatched types",
        )
        .await;

    // Vocab-only model returns error string safely without panic
    assert!(fix_res.is_err());
    let err = fix_res.unwrap_err();
    assert!(
        err.contains("vocab-only") || err.contains("No model loaded") || err.contains("JoinError")
    );
}

#[test]
fn test_code_fence_extraction_and_prompt_builder() {
    let raw_md = "Here is the repaired code:\n```rust\nfn add(a: i32, b: i32) -> i32 { a + b }\n```\nAll tests will pass.";
    assert_eq!(
        extract_code_from_response(raw_md),
        "fn add(a: i32, b: i32) -> i32 { a + b }"
    );

    let prompt =
        build_code_repair_prompt(Some("Custom instructions"), "source code", "error details");
    assert!(prompt.starts_with("Custom instructions"));
    assert!(prompt.contains("source code"));
    assert!(prompt.contains("error details"));
}
