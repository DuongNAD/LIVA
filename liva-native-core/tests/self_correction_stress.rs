//! Chỉ biên dịch với `--features experimental` — các module này chưa nối dây
//! vào đường chạy nào (xem docs mục 3.2). Hai file sandbox/self_correction còn
//! spawn `cargo test` lồng nhau nên rất chậm; tách khỏi build mặc định giúp CI
//! nhanh lên đáng kể.
#![cfg(feature = "experimental")]

use liva_native_core::evolution::{CodeAgent, Sandbox, SandboxError, SelfCorrectionLoop};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let rand_val: u64 = rand::random();
        path.push(format!("{}_{}", name, rand_val));
        fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

fn setup_test_project(project_path: &Path, name: &str, code: &str) {
    let cargo_toml_content = format!(
        r#"[package]
name = "{}"
version = "0.1.0"
edition = "2021"
"#,
        name
    );
    fs::write(project_path.join("Cargo.toml"), cargo_toml_content).unwrap();
    let src_dir = project_path.join("src");
    fs::create_dir_all(&src_dir).unwrap();
    fs::write(src_dir.join("lib.rs"), code).unwrap();
}

struct IterativeMockAgent {
    attempts: Arc<AtomicUsize>,
    fixes: Vec<String>,
}

impl CodeAgent for IterativeMockAgent {
    fn suggest_fix(
        &self,
        _source_content: &str,
        _error_log: &str,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send {
        let idx = self.attempts.fetch_add(1, Ordering::SeqCst);
        let res = if idx < self.fixes.len() {
            Ok(self.fixes[idx].clone())
        } else {
            Err("No more fixes available".to_string())
        };
        std::future::ready(res)
    }
}

fn count_running_test_processes(name: &str) -> usize {
    let output = std::process::Command::new("tasklist")
        .output()
        .expect("failed to run tasklist");
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout.lines().filter(|line| line.contains(name)).count()
}

#[tokio::test]
async fn test_self_correction_multiple_attempts() {
    let temp_dir =
        TempDirGuard::new("test_self_correction_iter").expect("Failed to create temp dir");
    let project_path = temp_dir.path();
    let source_file_path = project_path.join("src/lib.rs");

    // Initial broken code
    let initial_broken_code = r#"pub fn add(a: i32, b: i32) -> i32 {
    let x = 
}
"#;
    setup_test_project(project_path, "test_project_multiple", initial_broken_code);

    // Iterative fixes:
    // Fix 0: still syntax error
    // Fix 1: correct code
    let fixes = vec![
        r#"pub fn add(a: i32, b: i32) -> i32 {
    let x = a + 
}
"#
        .to_string(),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    a + b
}
"#
        .to_string(),
    ];

    let attempts = Arc::new(AtomicUsize::new(0));
    let mock_agent = IterativeMockAgent {
        attempts: attempts.clone(),
        fixes,
    };

    let correction_loop = SelfCorrectionLoop::with_max_retries(mock_agent, 2);
    let run_res = correction_loop.run(project_path, &source_file_path).await;

    assert!(
        run_res.is_ok(),
        "Self-correction loop failed: {:?}",
        run_res.err()
    );
    let output = run_res.unwrap();
    assert!(
        output.success,
        "Cargo test failed to pass after multiple corrections"
    );
    assert_eq!(
        attempts.load(Ordering::SeqCst),
        2,
        "Expected exactly 2 suggest_fix calls"
    );

    let current_content = fs::read_to_string(&source_file_path).expect("Failed to read src/lib.rs");
    assert!(current_content.contains("a + b"));
}

#[tokio::test]
async fn test_self_correction_max_retries_exhausted() {
    let temp_dir =
        TempDirGuard::new("test_self_correction_exhaust").expect("Failed to create temp dir");
    let project_path = temp_dir.path();
    let source_file_path = project_path.join("src/lib.rs");

    let initial_broken_code = r#"pub fn add(a: i32, b: i32) -> i32 {
    let x = 
}
"#;
    setup_test_project(project_path, "test_project_exhaust", initial_broken_code);

    // Fixes are always broken
    let fixes = vec![
        r#"pub fn add(a: i32, b: i32) -> i32 {
    let x = a + 
}
"#
        .to_string(),
        r#"pub fn add(a: i32, b: i32) -> i32 {
    let y = b + 
}
"#
        .to_string(),
    ];

    let attempts = Arc::new(AtomicUsize::new(0));
    let mock_agent = IterativeMockAgent {
        attempts: attempts.clone(),
        fixes,
    };

    let correction_loop = SelfCorrectionLoop::with_max_retries(mock_agent, 1);
    let run_res = correction_loop.run(project_path, &source_file_path).await;

    assert!(run_res.is_err(), "Expected SelfCorrectionLoop to fail");
    let err = run_res.err().unwrap();
    assert!(
        matches!(
            err,
            liva_native_core::evolution::SelfCorrectionError::MaxRetriesExhausted(_)
        ),
        "Expected MaxRetriesExhausted, got {:?}",
        err
    );

    // Verify backup was restored
    let current_content = fs::read_to_string(&source_file_path).expect("Failed to read src/lib.rs");
    assert_eq!(
        current_content, initial_broken_code,
        "Expected backup to be restored"
    );
}

#[tokio::test]
async fn test_sandbox_timeout_and_resource_reclamation() {
    let temp_dir = TempDirGuard::new("test_sandbox_timeout").expect("Failed to create temp dir");
    let project_path = temp_dir.path();

    // Setup a project with a test that sleeps for 60 seconds
    let broken_code_with_infinite_test = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_hangs() {
        println!("Test is starting...");
        std::thread::sleep(std::time::Duration::from_secs(60));
        println!("Test finished (should not print due to timeout)");
    }
}
"#;
    setup_test_project(
        project_path,
        "test_project_timeout",
        broken_code_with_infinite_test,
    );

    // Count running processes before the test
    let proc_count_before = count_running_test_processes("test_project_timeout");

    println!("Starting sandbox with hanging test. Timeout is 30 seconds...");
    let start_time = Instant::now();
    let run_res = Sandbox::run_tests(project_path).await;
    let elapsed = start_time.elapsed();

    println!("Sandbox run completed in {:?}", elapsed);

    assert!(
        run_res.is_err(),
        "Expected Sandbox run to fail with timeout"
    );
    let err = run_res.err().unwrap();
    assert!(
        matches!(err, SandboxError::Timeout),
        "Expected SandboxError::Timeout, got {:?}",
        err
    );

    // Verify execution time was around 30 seconds
    assert!(
        elapsed >= Duration::from_secs(29) && elapsed < Duration::from_secs(45),
        "Expected timeout to trigger at ~30s, actual elapsed was {:?}",
        elapsed
    );

    // Wait for OS to clean up processes (up to 10 seconds)
    let mut proc_count_after = 0;
    for _ in 0..10 {
        tokio::time::sleep(Duration::from_secs(1)).await;
        proc_count_after = count_running_test_processes("test_project_timeout");
        if proc_count_after <= proc_count_before {
            break;
        }
    }
    println!(
        "Process count before: {}, after: {}",
        proc_count_before, proc_count_after
    );

    // Check if any test processes are orphaned
    assert!(
        proc_count_after <= proc_count_before,
        "Orphaned test processes detected! Before: {}, After: {}",
        proc_count_before,
        proc_count_after
    );
}

#[tokio::test]
async fn test_concurrent_sandbox_runs() {
    let mut handles = vec![];
    let concurrency_limit = 5;

    for i in 0..concurrency_limit {
        let handle = tokio::spawn(async move {
            let temp_dir = TempDirGuard::new(&format!("test_concurrent_{}", i))
                .expect("Failed to create temp dir");
            let project_path = temp_dir.path();

            let passing_code = r#"
pub fn add(a: i32, b: i32) -> i32 {
    a + b
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_ok() {
        assert_eq!(2 + 2, 4);
    }
}
"#;
            setup_test_project(
                project_path,
                &format!("test_project_concurrent_{}", i),
                passing_code,
            );

            let res = Sandbox::run_tests(project_path).await;
            assert!(
                res.is_ok(),
                "Sandbox run failed for instance {}: {:?}",
                i,
                res.err()
            );
            let output = res.unwrap();
            assert!(
                output.success,
                "Test should have succeeded for instance {}",
                i
            );
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.await.expect("Concurrent sandbox task panicked");
    }
}
