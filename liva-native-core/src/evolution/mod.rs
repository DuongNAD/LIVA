pub mod sandbox;

pub use sandbox::{Sandbox, SandboxError, TestOutput};
use std::path::Path;

pub trait CodeAgent: Send + Sync {
    fn suggest_fix(
        &self,
        source_content: &str,
        error_log: &str,
    ) -> impl std::future::Future<Output = Result<String, String>> + Send;
}

pub struct SelfCorrectionLoop<A: CodeAgent> {
    agent: A,
    max_retries: usize,
}

#[derive(Debug)]
pub enum SelfCorrectionError {
    Io(std::io::Error),
    Sandbox(SandboxError),
    Agent(String),
    MaxRetriesExhausted(String),
}

impl std::fmt::Display for SelfCorrectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SelfCorrectionError::Io(e) => write!(f, "IO error: {}", e),
            SelfCorrectionError::Sandbox(e) => write!(f, "Sandbox error: {}", e),
            SelfCorrectionError::Agent(e) => write!(f, "Agent error: {}", e),
            SelfCorrectionError::MaxRetriesExhausted(s) => {
                write!(f, "Max retries exhausted: {}", s)
            }
        }
    }
}

impl std::error::Error for SelfCorrectionError {}

impl From<std::io::Error> for SelfCorrectionError {
    fn from(err: std::io::Error) -> Self {
        SelfCorrectionError::Io(err)
    }
}

impl From<SandboxError> for SelfCorrectionError {
    fn from(err: SandboxError) -> Self {
        SelfCorrectionError::Sandbox(err)
    }
}

struct BackupGuard {
    file_path: std::path::PathBuf,
    backup_content: String,
    active: bool,
}

impl BackupGuard {
    fn new(file_path: std::path::PathBuf, backup_content: String) -> Self {
        Self {
            file_path,
            backup_content,
            active: true,
        }
    }

    fn disarm(&mut self) {
        self.active = false;
    }

    async fn restore(&mut self) -> Result<(), std::io::Error> {
        if self.active {
            tokio::fs::write(&self.file_path, &self.backup_content).await?;
            self.active = false;
        }
        Ok(())
    }
}

impl Drop for BackupGuard {
    fn drop(&mut self) {
        if self.active {
            let path = self.file_path.clone();
            let content = self.backup_content.clone();
            tokio::spawn(async move {
                let _ = tokio::fs::write(path, content).await;
            });
        }
    }
}

impl<A: CodeAgent> SelfCorrectionLoop<A> {
    pub fn new(agent: A) -> Self {
        Self {
            agent,
            max_retries: 3,
        }
    }

    pub fn with_max_retries(agent: A, max_retries: usize) -> Self {
        Self { agent, max_retries }
    }

    pub async fn run(
        &self,
        project_path: &Path,
        source_file_path: &Path,
    ) -> Result<TestOutput, SelfCorrectionError> {
        // 1. Back up the source file
        let backup_content = tokio::fs::read_to_string(source_file_path).await?;
        let mut backup_guard = BackupGuard::new(source_file_path.to_path_buf(), backup_content);

        let mut last_error_log = String::new();

        // Loop: 1 initial attempt + up to `max_retries` corrections
        for attempt in 0..=self.max_retries {
            // Run tests in sandbox
            let test_res = Sandbox::run_tests(project_path).await;

            match test_res {
                Ok(output) => {
                    if output.success {
                        backup_guard.disarm();
                        return Ok(output);
                    } else {
                        last_error_log = Self::extract_error(&output);
                    }
                }
                Err(sandbox_err) => {
                    last_error_log = sandbox_err.to_string();
                }
            }

            if attempt == self.max_retries {
                break;
            }

            // Get suggest fix from agent
            let current_content = match tokio::fs::read_to_string(source_file_path).await {
                Ok(c) => c,
                Err(e) => {
                    backup_guard.restore().await?;
                    return Err(e.into());
                }
            };

            match self
                .agent
                .suggest_fix(&current_content, &last_error_log)
                .await
            {
                Ok(fixed_content) => {
                    if let Err(e) = tokio::fs::write(source_file_path, &fixed_content).await {
                        backup_guard.restore().await?;
                        return Err(e.into());
                    }
                }
                Err(agent_err) => {
                    backup_guard.restore().await?;
                    return Err(SelfCorrectionError::Agent(agent_err));
                }
            }
        }

        backup_guard.restore().await?;
        Err(SelfCorrectionError::MaxRetriesExhausted(last_error_log))
    }

    fn extract_error(output: &TestOutput) -> String {
        let mut error_summary = String::new();
        let mut in_failures = false;

        for line in output.stderr.lines().chain(output.stdout.lines()) {
            if line.contains("error[E") || line.contains("error:") || line.contains("--> ") {
                error_summary.push_str(line);
                error_summary.push('\n');
            } else if line.contains("panicked at") || line.contains("failed") {
                error_summary.push_str(line);
                error_summary.push('\n');
            } else if line.contains("failures:") {
                in_failures = true;
                error_summary.push_str(line);
                error_summary.push('\n');
            } else if in_failures && line.trim().is_empty() {
                in_failures = false;
            } else if in_failures {
                error_summary.push_str(line);
                error_summary.push('\n');
            }
        }

        if error_summary.trim().is_empty() {
            format!("Stderr:\n{}\nStdout:\n{}", output.stderr, output.stdout)
        } else {
            error_summary
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct MockCodeAgent {
        expected_keyword: String,
        correction: String,
    }

    impl CodeAgent for MockCodeAgent {
        fn suggest_fix(
            &self,
            _source_content: &str,
            error_log: &str,
        ) -> impl std::future::Future<Output = Result<String, String>> + Send {
            let matches = error_log.contains(&self.expected_keyword);
            let res = if matches {
                Ok(self.correction.clone())
            } else {
                Err(format!(
                    "Error log did not contain keyword: {}",
                    self.expected_keyword
                ))
            };
            std::future::ready(res)
        }
    }

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
                let _ = std::fs::remove_dir_all(&self.path);
            }
        }
    }

    #[tokio::test]
    async fn test_self_correction_loop_syntax_error() {
        let temp_dir =
            TempDirGuard::new("test_self_correction").expect("Failed to create temp dir");
        let project_path = temp_dir.path();

        // Write Cargo.toml
        let cargo_toml_content = r#"[package]
name = "test_self_correction"
version = "0.1.0"
edition = "2021"
"#;
        tokio::fs::write(project_path.join("Cargo.toml"), cargo_toml_content)
            .await
            .expect("Failed to write Cargo.toml");

        // Create src/
        tokio::fs::create_dir_all(project_path.join("src"))
            .await
            .expect("Failed to create src dir");

        // Write src/lib.rs with syntax error
        let source_file_path = project_path.join("src/lib.rs");
        let initial_broken_code = r#"pub fn answer() -> i32 {
    let x = 
}
"#;
        tokio::fs::write(&source_file_path, initial_broken_code)
            .await
            .expect("Failed to write src/lib.rs");

        // Set up MockAgent that corrects the syntax error
        let corrected_code = r#"pub fn answer() -> i32 {
    42
}
"#;
        let mock_agent = MockCodeAgent {
            expected_keyword: "error".to_string(), // syntax errors emit compile errors containing "error"
            correction: corrected_code.to_string(),
        };

        let correction_loop = SelfCorrectionLoop::new(mock_agent);
        let run_res = correction_loop.run(project_path, &source_file_path).await;

        assert!(
            run_res.is_ok(),
            "Self-correction loop failed: {:?}",
            run_res.err()
        );
        let output = run_res.unwrap();
        assert!(output.success, "Cargo test failed to pass after correction");

        // Read corrected file content and check it matches corrected_code
        let current_content = tokio::fs::read_to_string(&source_file_path)
            .await
            .expect("Failed to read src/lib.rs");
        assert_eq!(current_content.trim(), corrected_code.trim());
    }
}
