use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use liva_native_core::evolution::{Sandbox, SandboxError, SelfCorrectionLoop, CodeAgent};

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> std::io::Result<Self> {
        let mut path = std::env::temp_dir();
        let rand_val: u64 = rand::random();
        path.push(format!("{}_{}", name, rand_val));
        std::fs::create_dir_all(&path)?;
        Ok(Self { path })
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            // Attempt to clean up. In Windows, this might fail if processes are still running.
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

fn create_dummy_project(path: &Path, test_code: &str) -> std::io::Result<()> {
    let cargo_toml = r#"[package]
name = "dummy_project"
version = "0.1.0"
edition = "2021"
"#;
    std::fs::write(path.join("Cargo.toml"), cargo_toml)?;
    std::fs::create_dir_all(path.join("src"))?;
    std::fs::write(path.join("src/lib.rs"), test_code)?;
    Ok(())
}

#[tokio::test]
async fn test_sandbox_timeout_and_reclamation() {
    let temp_dir = TempDirGuard::new("stress_sandbox_timeout").expect("failed to create temp dir");
    
    // Write test code that loops forever to trigger timeout (30 seconds limit)
    let sleeping_test_code = r#"
#[test]
fn test_sleep() {
    println!("Starting loop test...");
    std::fs::write("started.txt", "yes").unwrap();
    loop {
        std::thread::sleep(std::time::Duration::from_millis(500));
        // Append to verify it keeps running
        let _ = std::fs::OpenOptions::new()
            .append(true)
            .open("started.txt")
            .map(|mut f| {
                use std::io::Write;
                let _ = writeln!(f, "still running");
            });
    }
}
"#;
    create_dummy_project(temp_dir.path(), sleeping_test_code).expect("failed to create dummy project");

    println!("Running sandbox on a hanging test...");
    let start = Instant::now();
    let res = Sandbox::run_tests(temp_dir.path()).await;
    let elapsed = start.elapsed();

    println!("Sandbox finished in {:?}. Result: {:?}", elapsed, res);

    // Verify it timed out and took at least 30 seconds
    assert!(res.is_err(), "Expected timeout error, but got Ok");
    match res.unwrap_err() {
        SandboxError::Timeout => {}
        other => panic!("Expected SandboxError::Timeout, got {:?}", other),
    }
    assert!(elapsed >= Duration::from_secs(30), "Sandbox timed out too early in {:?}", elapsed);

    // Check if started.txt was created
    let started_file_path = temp_dir.path().join("started.txt");
    let initial_exists = started_file_path.exists();
    println!("started.txt exists right after sandbox finished: {}", initial_exists);
    
    let mut initial_content = String::new();
    if initial_exists {
        initial_content = std::fs::read_to_string(&started_file_path).unwrap_or_default();
        println!("Initial started.txt length: {}", initial_content.len());
    }

    // Sleep 3 seconds to observe if the orphaned process writes more data
    println!("Sleeping for 3 seconds to check for orphaned process activity...");
    tokio::time::sleep(Duration::from_secs(3)).await;

    let mut final_content = String::new();
    if started_file_path.exists() {
        final_content = std::fs::read_to_string(&started_file_path).unwrap_or_default();
        println!("Final started.txt length: {}", final_content.len());
    }

    let is_orphaned_active = final_content.len() > initial_content.len();
    println!("Orphaned process is still active and writing: {}", is_orphaned_active);

    // We will report whether resource reclamation succeeded or failed.
    // Let's also check if we can list any running processes using tasklist.
    #[cfg(target_os = "windows")]
    {
        let output = std::process::Command::new("tasklist")
            .output()
            .expect("failed to execute tasklist");
        let stdout = String::from_utf8_lossy(&output.stdout);
        println!("=== Tasklist Matching Lines ===");
        for line in stdout.lines() {
            if line.contains("dummy_project") || line.contains("cargo") {
                println!("MATCH: {}", line);
            }
        }
    }

    // Attempt to remove temp directory
    let remove_res = std::fs::remove_dir_all(temp_dir.path());
    println!("Attempted to remove temp directory. Result: {:?}", remove_res);
}

#[tokio::test]
async fn test_sandbox_concurrency() {
    // Run 3 sandbox test processes concurrently to verify they don't collide or deadlock.
    let mut instances = vec![];
    for i in 0..3 {
        let temp_dir = TempDirGuard::new(&format!("stress_sandbox_concur_{}", i)).expect("failed to create temp dir");
        let test_code = r#"
#[test]
fn test_success() {
    assert_eq!(2 + 2, 4);
}
"#;
        create_dummy_project(temp_dir.path(), test_code).expect("failed to create dummy project");
        instances.push(temp_dir);
    }

    let start = Instant::now();
    let mut futures = vec![];
    for inst in &instances {
        futures.push(Sandbox::run_tests(inst.path()));
    }

    let results = futures_util::future::join_all(futures).await;
    let elapsed = start.elapsed();
    println!("Concurrent sandboxes finished in {:?}", elapsed);

    for (i, res) in results.iter().enumerate() {
        assert!(res.is_ok(), "Sandbox {} failed: {:?}", i, res);
        let output = res.as_ref().unwrap();
        assert!(output.success, "Sandbox {} test did not succeed", i);
    }
}

struct MultiAttemptAgent {
    attempts: std::sync::atomic::AtomicUsize,
}

impl CodeAgent for MultiAttemptAgent {
    fn suggest_fix(
        &self,
        _source_content: &str,
        _error_log: &str,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send {
        let attempt = self.attempts.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let res = if attempt == 0 {
            // First fix attempt still has a syntax error
            Ok(r#"
pub fn answer() -> i32 {
    let x = 
}
"#.to_string())
        } else {
            // Second fix attempt is correct
            Ok(r#"
pub fn answer() -> i32 {
    42
}
#[test]
fn test_answer() {
    assert_eq!(answer(), 42);
}
"#.to_string())
        };
        std::future::ready(res)
    }
}

#[tokio::test]
async fn test_self_correction_multiple_attempts() {
    let temp_dir = TempDirGuard::new("stress_self_correction_multi").expect("failed to create temp dir");
    
    // Initial broken code
    let initial_broken_code = r#"
pub fn answer() -> i32 {
    broken
}
"#;
    create_dummy_project(temp_dir.path(), initial_broken_code).expect("failed to create dummy project");
    let source_file_path = temp_dir.path().join("src/lib.rs");

    let agent = MultiAttemptAgent {
        attempts: std::sync::atomic::AtomicUsize::new(0),
    };

    let loop_runner = SelfCorrectionLoop::with_max_retries(agent, 3);
    let run_res = loop_runner.run(temp_dir.path(), &source_file_path).await;

    assert!(run_res.is_ok(), "Self-correction loop failed: {:?}", run_res.err());
    let output = run_res.unwrap();
    assert!(output.success, "Cargo test failed to pass after correction");

    let final_content = std::fs::read_to_string(&source_file_path).expect("failed to read src/lib.rs");
    assert!(final_content.contains("42"));
}
