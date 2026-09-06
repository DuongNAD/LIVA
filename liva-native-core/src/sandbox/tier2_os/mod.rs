//! Tier-2 OS Containment Subsystem (macOS Seatbelt, Linux Bubblewrap/Landlock, Windows Job Objects)
//!
//! Provides native process containment for host CLI commands (`cargo`, `python`, `git`)
//! enforcing kernel-level sandboxing, strict filesystem boundaries, CPU/RAM resource limits,
//! and network isolation across all supported desktop platforms.

pub mod linux_bwrap;
pub mod macos_seatbelt;
pub mod windows_job;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;
use tracing::info;

pub use windows_job::{
    execute_windows_sandbox, JobAccountingStats, OsPlatformBackend, WindowsJobConfig,
    WindowsJobSandbox,
};

#[cfg(not(target_os = "windows"))]
pub use windows_job::MockWindowsJobSandbox;
#[cfg(target_os = "windows")]
pub use windows_job::SafeJobHandle;

use crate::sandbox::policy::validate_command;

/// Security policy configuration for Tier-2 OS Subprocess execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OsSandboxPolicy {
    /// Allowed filesystem paths for read operations.
    pub allowed_read_paths: Vec<PathBuf>,
    /// Allowed filesystem paths for write operations.
    pub allowed_write_paths: Vec<PathBuf>,
    /// Explicitly allowed executable commands.
    pub allowed_commands: Vec<String>,
    /// Whether outbound network sockets and DNS resolution are permitted.
    pub allow_network: bool,
}

/// Structured outcome of a sandboxed OS command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OsExecutionResult {
    /// Process exit code.
    pub exit_code: i32,
    /// Process status code alias.
    pub status_code: i32,
    /// Whether the process exited successfully (code 0).
    pub success: bool,
    /// Standard output raw bytes captured from the process.
    pub stdout: Vec<u8>,
    /// Standard error raw bytes captured from the process.
    pub stderr: Vec<u8>,
    /// Total wall-clock time elapsed during command execution.
    pub execution_time: Duration,
}

impl OsExecutionResult {
    pub fn stdout_str(&self) -> String {
        String::from_utf8_lossy(&self.stdout).to_string()
    }

    pub fn stderr_str(&self) -> String {
        String::from_utf8_lossy(&self.stderr).to_string()
    }
}

/// Errors raised by the OS sandbox containment engine.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsSandboxError {
    #[error("Subprocess execution timed out")]
    ExecutionTimeout,

    #[error("OS security containment denied access: {0}")]
    PermissionDenied(String),

    #[error("Command forbidden by security policy or AST sanitizer: {0}")]
    CommandForbidden(String),

    #[error("Subprocess spawn or execution failed: {0}")]
    ProcessFailed(String),

    #[error("Path validation or canonicalization failed: {0}")]
    PathValidationFailed(String),

    #[error("Sandbox initialization error: {0}")]
    SandboxInitFailed(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceLimitExceeded(String),

    #[error("Subprocess terminated by Job Object limit")]
    JobLimitTerminated,

    #[error("Platform backend '{0}' is unavailable on this host")]
    BackendUnavailable(String),
}

/// Contract for platform-specific OS containment engines.
#[async_trait]
pub trait OsSandboxEngine: Send + Sync {
    /// Human-readable identifier of the sandbox backend engine.
    fn platform_name(&self) -> &'static str;

    /// Checks if this backend is fully supported and operational on the current host.
    fn is_supported(&self) -> bool;

    /// Executes a command subject to containment rules.
    async fn execute(
        &self,
        command: &str,
        args: &[String],
        policy: &OsSandboxPolicy,
    ) -> Result<OsExecutionResult, OsSandboxError>;
}

/// Multi-platform unified runner for Tier 2 OS-level sandboxing containment.
#[derive(Debug, Default, Clone)]
pub struct OsSandboxRunner;

impl OsSandboxRunner {
    /// Creates a new `OsSandboxRunner`.
    pub fn new() -> Self {
        Self
    }

    /// Returns the active platform backend identifier.
    pub fn backend(&self) -> OsPlatformBackend {
        OsPlatformBackend::current_host()
    }

    /// Returns the human-readable platform name.
    pub fn backend_name(&self) -> &'static str {
        match self.backend() {
            OsPlatformBackend::MacOsSeatbelt => "macos-seatbelt",
            OsPlatformBackend::LinuxBubblewrap => "linux-bubblewrap",
            OsPlatformBackend::WindowsJobObject => "windows-job-objects",
        }
    }

    /// Executes a command inside the platform-appropriate OS sandbox container.
    pub async fn execute_command(
        &self,
        command: &str,
        args: &[String],
        policy: &OsSandboxPolicy,
    ) -> Result<OsExecutionResult, OsSandboxError> {
        info!(
            "Executing OS sandboxed command: {} {:?} [backend: {}]",
            command,
            args,
            self.backend_name()
        );

        // 1. Upstream AST Command Sanitizer & Denylist check
        validate_command(command, args)
            .map_err(|e| OsSandboxError::CommandForbidden(e.to_string()))?;

        // 2. Upstream argument path traversal check
        for arg in args {
            if arg.contains("..") || arg.contains('\0') {
                return Err(OsSandboxError::PathValidationFailed(format!(
                    "Path traversal escape in CLI argument: {arg}"
                )));
            }
        }

        // 3. Platform dispatch
        #[cfg(target_os = "macos")]
        {
            macos_seatbelt::execute_seatbelt(command, args, policy).await
        }

        #[cfg(target_os = "linux")]
        {
            linux_bwrap::execute_linux_sandbox(command, args, policy).await
        }

        #[cfg(target_os = "windows")]
        {
            windows_job::execute_windows_sandbox(command, args, policy).await
        }

        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        {
            windows_job::execute_windows_sandbox(command, args, policy).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_os_sandbox_runner_basic() {
        let runner = OsSandboxRunner::new();
        let mut policy = OsSandboxPolicy::default();
        policy.allowed_read_paths.push(PathBuf::from("/bin"));
        policy.allowed_read_paths.push(PathBuf::from("/usr"));

        let res = runner
            .execute_command("echo", &["sandbox_test_success".to_string()], &policy)
            .await;

        if let Ok(output) = res {
            assert!(output.success);
            assert_eq!(output.exit_code, 0);
            assert!(output.stdout_str().contains("sandbox_test_success"));
        }
    }

    #[tokio::test]
    async fn test_os_sandbox_denylist_rejection() {
        let runner = OsSandboxRunner::new();
        let policy = OsSandboxPolicy::default();

        let res = runner
            .execute_command("rm", &["-rf".to_string(), "/tmp/test".to_string()], &policy)
            .await;

        assert!(res.is_err());
        let err = res.unwrap_err();
        match err {
            OsSandboxError::CommandForbidden(_) => {}
            other => panic!("Expected CommandForbidden error, got: {:?}", other),
        }
    }

    #[test]
    fn test_platform_backend_names() {
        let runner = OsSandboxRunner::new();
        let name = runner.backend_name();
        assert!(!name.is_empty());
    }
}
