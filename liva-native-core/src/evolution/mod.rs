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

pub type CustomCodeFixFuture =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, String>> + Send>>;
pub type CustomCodeFixFn = dyn Fn(&str) -> CustomCodeFixFuture + Send + Sync;

/// Trình xử lý backend cho `LlmCodeAgent`.
pub enum CodeAgentBackend {
    /// LLM engine cục bộ dựa trên `LlamaRouterManager`
    LocalRouter(std::sync::Arc<tokio::sync::Mutex<crate::llm::LlamaRouterManager>>),
    /// Trình xử lý tuỳ biến (cho mock engine, external provider, hoặc test harness)
    Custom(std::sync::Arc<CustomCodeFixFn>),
}

/// Adapter kết nối vòng lặp tự sửa lỗi `CodeAgent` vào LLM engine thật hoặc provider tùy biến.
pub struct LlmCodeAgent {
    backend: CodeAgentBackend,
    pub system_instruction: Option<String>,
    pub temperature: f32,
    pub top_p: f32,
}

impl LlmCodeAgent {
    /// Khởi tạo `LlmCodeAgent` với `LlamaRouterManager` cục bộ
    pub fn new_local(
        llm: std::sync::Arc<tokio::sync::Mutex<crate::llm::LlamaRouterManager>>,
    ) -> Self {
        Self {
            backend: CodeAgentBackend::LocalRouter(llm),
            system_instruction: None,
            temperature: 0.2,
            top_p: 0.95,
        }
    }

    /// Khởi tạo `LlmCodeAgent` với một async handler tùy biến
    pub fn new_custom<F, Fut>(handler: F) -> Self
    where
        F: Fn(&str) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = Result<String, String>> + Send + 'static,
    {
        Self {
            backend: CodeAgentBackend::Custom(std::sync::Arc::new(move |prompt| {
                Box::pin(handler(prompt))
            })),
            system_instruction: None,
            temperature: 0.2,
            top_p: 0.95,
        }
    }

    pub fn with_system_instruction(mut self, instruction: impl Into<String>) -> Self {
        self.system_instruction = Some(instruction.into());
        self
    }

    pub fn with_sampling(mut self, temperature: f32, top_p: f32) -> Self {
        self.temperature = temperature;
        self.top_p = top_p;
        self
    }
}

/// Dựng prompt yêu cầu LLM sửa mã nguồn dựa trên log lỗi
pub fn build_code_repair_prompt(
    system_instruction: Option<&str>,
    source_content: &str,
    error_log: &str,
) -> String {
    let sys = system_instruction.unwrap_or(
        "You are an expert software engineer specializing in Rust.\n\
         Given the source code and error log, provide the exact fixed source code.\n\
         Output ONLY the replacement code inside a single ```rust ... ``` code block without any conversational filler.",
    );

    format!(
        "{sys}\n\n\
         ### SOURCE CODE:\n\
         ```rust\n\
         {source_content}\n\
         ```\n\n\
         ### ERROR LOG:\n\
         {error_log}\n\n\
         ### FIXED SOURCE CODE:\n"
    )
}

/// Bóc tách mã nguồn thuần từ phản hồi của LLM (gỡ bỏ code fences nếu có)
pub fn extract_code_from_response(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(start_idx) = trimmed.find("```") {
        let after_opening = &trimmed[start_idx + 3..];
        // Bỏ qua tag ngôn ngữ (vd: `rust\n` hoặc `\n`)
        let code_start = if let Some(newline_idx) = after_opening.find('\n') {
            &after_opening[newline_idx + 1..]
        } else {
            after_opening
        };

        if let Some(end_idx) = code_start.rfind("```") {
            return code_start[..end_idx].trim().to_string();
        }
    }
    trimmed.to_string()
}

impl CodeAgent for LlmCodeAgent {
    async fn suggest_fix(&self, source_content: &str, error_log: &str) -> Result<String, String> {
        let prompt = build_code_repair_prompt(
            self.system_instruction.as_deref(),
            source_content,
            error_log,
        );

        let raw_reply = match &self.backend {
            CodeAgentBackend::Custom(handler) => handler(&prompt).await?,
            CodeAgentBackend::LocalRouter(llm_lock) => {
                let llm_clone = llm_lock.clone();
                let temp = self.temperature;
                let top_p = self.top_p;
                let prompt_clone = prompt.clone();

                tokio::task::spawn_blocking(move || {
                    let mut mgr = llm_clone.blocking_lock();
                    mgr.generate_completion(&prompt_clone, temp, top_p, |_| true)
                        .map(|out| out.text)
                })
                .await
                .map_err(|e| format!("JoinError: {e}"))??
            }
        };

        let fixed_code = extract_code_from_response(&raw_reply);
        if fixed_code.is_empty() {
            return Err("LLM returned empty code fix".to_string());
        }
        Ok(fixed_code)
    }
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
            if line.contains("error[E")
                || line.contains("error:")
                || line.contains("--> ")
                || line.contains("panicked at")
                || line.contains("failed")
            {
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

    #[test]
    fn test_extract_code_from_response_variants() {
        // Variant 1: ```rust ... ```
        let raw1 = "Here is the fix:\n```rust\npub fn hello() -> &'static str {\n    \"world\"\n}\n```\nHope this helps!";
        assert_eq!(
            extract_code_from_response(raw1),
            "pub fn hello() -> &'static str {\n    \"world\"\n}"
        );

        // Variant 2: ``` ... ``` without language tag
        let raw2 = "```\nlet a = 10;\nlet b = 20;\n```";
        assert_eq!(extract_code_from_response(raw2), "let a = 10;\nlet b = 20;");

        // Variant 3: Raw code without fences
        let raw3 = "fn solve() -> bool { true }";
        assert_eq!(
            extract_code_from_response(raw3),
            "fn solve() -> bool { true }"
        );
    }

    #[test]
    fn test_build_code_repair_prompt_contains_essentials() {
        let prompt = build_code_repair_prompt(
            None,
            "pub fn broken() {",
            "error[E0601]: `main` function not found in crate",
        );
        assert!(prompt.contains("### SOURCE CODE:"));
        assert!(prompt.contains("pub fn broken() {"));
        assert!(prompt.contains("### ERROR LOG:"));
        assert!(prompt.contains("error[E0601]"));
        assert!(prompt.contains("### FIXED SOURCE CODE:"));
    }

    #[tokio::test]
    async fn test_llm_code_agent_custom_handler_flow() {
        let agent = LlmCodeAgent::new_custom(|prompt| {
            assert!(prompt.contains("fn bug()"));
            assert!(prompt.contains("mismatched types"));
            std::future::ready(Ok("```rust\nfn bug() -> i32 { 100 }\n```".to_string()))
        });

        let fix = agent
            .suggest_fix(
                "fn bug() -> i32 { \"hello\" }",
                "error[E0308]: mismatched types",
            )
            .await
            .expect("fix generated");

        assert_eq!(fix, "fn bug() -> i32 { 100 }");
    }
}
