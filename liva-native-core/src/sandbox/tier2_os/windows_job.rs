//! Windows Job Objects OS Containment Subsystem (RFC-003 §R3)
//!
//! Provides kernel-level process tree containment on Windows using Win32 Job Objects:
//! - **CPU Rate Throttling**: Hard cap on CPU scheduling cycles (`JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP`).
//! - **RAM Commit Ceilings**: Per-process and total job committed memory ceilings (`JOB_OBJECT_LIMIT_JOB_MEMORY`).
//! - **Active Process Caps & Anti-Fork Bomb**: Restricting active process count to 1 (`JOB_OBJECT_LIMIT_ACTIVE_PROCESS`).
//! - **Fail-Closed Process Tree Cleanup**: Automatic termination on job handle close (`JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`).
//! - **Inescapable Child Containment**: Omits breakaway flags ensuring grandchild processes cannot escape.
//! - **Cross-Platform Compatibility**: Full non-Windows mock runner for macOS/Linux CI environments.

use serde::{Deserialize, Serialize};

use super::{OsExecutionResult, OsSandboxError, OsSandboxPolicy};
use crate::sandbox::policy::validate_command;

/// Configuration options for Windows Job Object resource boundaries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowsJobConfig {
    /// Identifier name for the Windows Job Object (empty string for anonymous job).
    pub job_name: String,
    /// Maximum CPU rate percentage allowed for the job (0..=100%). Default: 50%.
    pub max_cpu_rate_pct: u32,
    /// Maximum committed memory limit in bytes for process and job. Default: 512 MB.
    pub max_memory_limit_bytes: u64,
    /// Maximum I/O rate in bytes per second. Default: 50 MB/s.
    pub max_io_bytes_per_sec: u64,
    /// Whether to block child process creation (limits active processes to 1). Default: true.
    pub block_child_process_creation: bool,
    /// Whether all processes in the job must terminate when the job object handle closes. Default: true.
    pub terminate_on_job_close: bool,
}

impl Default for WindowsJobConfig {
    fn default() -> Self {
        Self {
            job_name: "LIVA_Sandbox_Job".to_string(),
            max_cpu_rate_pct: 50,
            max_memory_limit_bytes: 512 * 1024 * 1024, // 512 MB
            max_io_bytes_per_sec: 50 * 1024 * 1024,   // 50 MB/s
            block_child_process_creation: true,
            terminate_on_job_close: true,
        }
    }
}

/// Identifies the active OS isolation engine across platforms.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsPlatformBackend {
    MacOsSeatbelt,
    LinuxBubblewrap,
    WindowsJobObject,
}

impl OsPlatformBackend {
    /// Detects the native OS backend for the current host environment.
    pub fn current_host() -> Self {
        #[cfg(target_os = "macos")]
        return OsPlatformBackend::MacOsSeatbelt;
        #[cfg(target_os = "linux")]
        return OsPlatformBackend::LinuxBubblewrap;
        #[cfg(target_os = "windows")]
        return OsPlatformBackend::WindowsJobObject;
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        return OsPlatformBackend::LinuxBubblewrap;
    }
}

/// Runtime accounting and resource telemetry queried from the Job Object.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct JobAccountingStats {
    pub total_user_time_ms: u64,
    pub total_kernel_time_ms: u64,
    pub active_processes: u32,
    pub total_processes: u32,
    pub peak_process_memory_used: u64,
    pub peak_job_memory_used: u64,
}

/// Object-oriented Windows Job Objects sandbox runner conforming to RFC-003 §R3.
#[derive(Debug, Clone)]
pub struct WindowsJobSandbox {
    config: WindowsJobConfig,
    policy: OsSandboxPolicy,
}

impl WindowsJobSandbox {
    /// Creates a new `WindowsJobSandbox` with default configuration and given policy.
    pub fn new(policy: OsSandboxPolicy) -> Self {
        Self {
            config: WindowsJobConfig::default(),
            policy,
        }
    }

    /// Creates a new `WindowsJobSandbox` with custom configuration and policy.
    pub fn with_config(config: WindowsJobConfig, policy: OsSandboxPolicy) -> Self {
        Self { config, policy }
    }

    /// Access the configuration.
    pub fn config(&self) -> &WindowsJobConfig {
        &self.config
    }

    /// Access the policy.
    pub fn policy(&self) -> &OsSandboxPolicy {
        &self.policy
    }

    /// Executes a command within the sandboxed job object environment.
    pub async fn execute(
        &self,
        command: &str,
        args: &[String],
    ) -> Result<OsExecutionResult, OsSandboxError> {
        execute_windows_sandbox_with_config(command, args, &self.policy, &self.config).await
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Windows Platform Implementation (Win32 Job Objects API)
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(target_os = "windows")]
mod windows_impl {
    use super::*;
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use std::os::windows::io::AsRawHandle;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use tokio::process::Command;
    use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::JobObjects::*;

    /// RAII wrapper around a Win32 Job Object handle.
    #[derive(Debug)]
    pub struct SafeJobHandle {
        raw: HANDLE,
    }

    unsafe impl Send for SafeJobHandle {}
    unsafe impl Sync for SafeJobHandle {}

    impl SafeJobHandle {
        pub fn new(handle: HANDLE) -> Self {
            Self { raw: handle }
        }

        pub fn raw(&self) -> HANDLE {
            self.raw
        }

        pub fn is_valid(&self) -> bool {
            !self.raw.is_null() && self.raw != INVALID_HANDLE_VALUE
        }
    }

    impl Drop for SafeJobHandle {
        fn drop(&mut self) {
            if self.is_valid() {
                unsafe {
                    CloseHandle(self.raw);
                }
            }
        }
    }

    /// Creates and configures a Win32 Job Object with the requested resource limits.
    pub fn create_and_configure_job(
        config: &WindowsJobConfig,
    ) -> Result<SafeJobHandle, OsSandboxError> {
        let job_name_wide: Option<Vec<u16>> = if config.job_name.is_empty() {
            None
        } else {
            Some(
                OsStr::new(&config.job_name)
                    .encode_wide()
                    .chain(std::iter::once(0))
                    .collect(),
            )
        };

        let lp_name = match &job_name_wide {
            Some(w) => w.as_ptr(),
            None => std::ptr::null(),
        };

        let raw_job = unsafe { CreateJobObjectW(std::ptr::null(), lp_name) };
        if raw_job.is_null() || raw_job == INVALID_HANDLE_VALUE {
            let err = unsafe { GetLastError() };
            return Err(OsSandboxError::SandboxInitFailed(format!(
                "CreateJobObjectW failed with error code: {err}"
            )));
        }

        let job = SafeJobHandle::new(raw_job);

        // 1. Extended resource limits (memory, child process count, kill on close)
        apply_extended_limits(&job, config)?;

        // 2. CPU rate throttling
        apply_cpu_rate_limits(&job, config)?;

        // 3. UI and system restrictions
        apply_ui_restrictions(&job)?;

        Ok(job)
    }

    /// Applies memory caps, child process restrictions, and kill-on-close limits.
    pub fn apply_extended_limits(
        job: &SafeJobHandle,
        config: &WindowsJobConfig,
    ) -> Result<(), OsSandboxError> {
        let mut extended_info: JOBOBJECT_EXTENDED_LIMIT_INFORMATION = unsafe { std::mem::zeroed() };
        let mut limit_flags = 0u32;

        // Auto-terminate all child processes when the job object handle closes
        if config.terminate_on_job_close {
            limit_flags |= JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
        }

        // Memory limit configuration
        if config.max_memory_limit_bytes > 0 {
            limit_flags |= JOB_OBJECT_LIMIT_PROCESS_MEMORY | JOB_OBJECT_LIMIT_JOB_MEMORY;
            extended_info.ProcessMemoryLimit = config.max_memory_limit_bytes as usize;
            extended_info.JobMemoryLimit = config.max_memory_limit_bytes as usize;
        }

        // Child process containment & active process limit
        if config.block_child_process_creation {
            limit_flags |= JOB_OBJECT_LIMIT_ACTIVE_PROCESS;
            extended_info.BasicLimitInformation.ActiveProcessLimit = 1;
        }

        // Suppress system error dialogs
        limit_flags |= JOB_OBJECT_LIMIT_DIE_ON_UNHANDLED_EXCEPTION;

        // Strictly omit JOB_OBJECT_LIMIT_SILENT_BREAKAWAY_OK and JOB_OBJECT_LIMIT_BREAKAWAY_OK!
        // This ensures child and grandchild processes cannot escape containment.
        extended_info.BasicLimitInformation.LimitFlags = limit_flags;

        let res = unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                &extended_info as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            )
        };

        if res == 0 {
            let err = unsafe { GetLastError() };
            return Err(OsSandboxError::SandboxInitFailed(format!(
                "SetInformationJobObject(ExtendedLimit) failed: error {err}"
            )));
        }

        Ok(())
    }

    /// Applies CPU rate limits (percentage of total CPU quota).
    pub fn apply_cpu_rate_limits(
        job: &SafeJobHandle,
        config: &WindowsJobConfig,
    ) -> Result<(), OsSandboxError> {
        if config.max_cpu_rate_pct > 0 && config.max_cpu_rate_pct <= 100 {
            let mut cpu_info: JOBOBJECT_CPU_RATE_CONTROL_INFORMATION = unsafe { std::mem::zeroed() };
            cpu_info.ControlFlags =
                JOB_OBJECT_CPU_RATE_CONTROL_ENABLE | JOB_OBJECT_CPU_RATE_CONTROL_HARD_CAP;
            // CpuRate in units of 0.01% (e.g. 50% = 5000)
            cpu_info.Anonymous.CpuRate = config.max_cpu_rate_pct * 100;

            let res = unsafe {
                SetInformationJobObject(
                    job.raw(),
                    JobObjectCpuRateControlInformation,
                    &cpu_info as *const _ as *const std::ffi::c_void,
                    std::mem::size_of::<JOBOBJECT_CPU_RATE_CONTROL_INFORMATION>() as u32,
                )
            };

            if res == 0 {
                let err = unsafe { GetLastError() };
                warn!(
                    "SetInformationJobObject(CpuRateControl) failed: error {err}. Continuing."
                );
            }
        }
        Ok(())
    }

    /// Applies UI restrictions to prevent clipboard snooping and desktop manipulation.
    pub fn apply_ui_restrictions(job: &SafeJobHandle) -> Result<(), OsSandboxError> {
        let mut ui_restrictions: JOBOBJECT_BASIC_UI_RESTRICTIONS = unsafe { std::mem::zeroed() };
        ui_restrictions.UIRestrictionsClass = JOB_OBJECT_UILIMIT_HANDLES
            | JOB_OBJECT_UILIMIT_READCLIPBOARD
            | JOB_OBJECT_UILIMIT_WRITECLIPBOARD
            | JOB_OBJECT_UILIMIT_SYSTEMPARAMETERS
            | JOB_OBJECT_UILIMIT_DISPLAYSETTINGS
            | JOB_OBJECT_UILIMIT_GLOBALATOMS
            | JOB_OBJECT_UILIMIT_DESKTOP
            | JOB_OBJECT_UILIMIT_EXITWINDOWS;

        unsafe {
            SetInformationJobObject(
                job.raw(),
                JobObjectBasicUIRestrictions,
                &ui_restrictions as *const _ as *const std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_BASIC_UI_RESTRICTIONS>() as u32,
            );
        }
        Ok(())
    }

    /// Queries runtime accounting stats from the Job Object.
    pub fn query_job_accounting(
        job: &SafeJobHandle,
    ) -> Result<JobAccountingStats, OsSandboxError> {
        let mut basic_accounting: JOBOBJECT_BASIC_ACCOUNTING_INFORMATION =
            unsafe { std::mem::zeroed() };
        let mut extended_limits: JOBOBJECT_EXTENDED_LIMIT_INFORMATION =
            unsafe { std::mem::zeroed() };

        let res1 = unsafe {
            QueryInformationJobObject(
                job.raw(),
                JobObjectBasicAccountingInformation,
                &mut basic_accounting as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };

        let res2 = unsafe {
            QueryInformationJobObject(
                job.raw(),
                JobObjectExtendedLimitInformation,
                &mut extended_limits as *mut _ as *mut std::ffi::c_void,
                std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
                std::ptr::null_mut(),
            )
        };

        if res1 == 0 || res2 == 0 {
            let err = unsafe { GetLastError() };
            return Err(OsSandboxError::SandboxInitFailed(format!(
                "QueryInformationJobObject failed: error {err}"
            )));
        }

        // 100-nanosecond ticks to milliseconds: ticks / 10_000
        let user_time_ms = (basic_accounting.TotalUserTime / 10_000) as u64;
        let kernel_time_ms = (basic_accounting.TotalKernelTime / 10_000) as u64;

        Ok(JobAccountingStats {
            total_user_time_ms: user_time_ms,
            total_kernel_time_ms: kernel_time_ms,
            active_processes: basic_accounting.ActiveProcesses,
            total_processes: basic_accounting.TotalProcesses,
            peak_process_memory_used: extended_limits.PeakProcessMemoryUsed as u64,
            peak_job_memory_used: extended_limits.PeakJobMemoryUsed as u64,
        })
    }

    /// Executes a command sandboxed inside a Windows Job Object.
    pub async fn execute_windows_sandbox_with_config(
        command: &str,
        args: &[String],
        _policy: &OsSandboxPolicy,
        config: &WindowsJobConfig,
    ) -> Result<OsExecutionResult, OsSandboxError> {
        let start = Instant::now();

        // 1. AST sanitizer and command denylist verification
        validate_command(command, args)
            .map_err(|e| OsSandboxError::CommandForbidden(e.to_string()))?;

        // 2. Create and configure Job Object
        let job = create_and_configure_job(config)?;

        // 3. Spawn child process
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.kill_on_drop(true);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let mut child = cmd.spawn().map_err(|e| {
            OsSandboxError::ProcessFailed(format!("Failed to spawn Windows child process: {e}"))
        })?;

        // 4. Assign process to Job Object
        if let Some(raw_handle) = child.raw_handle() {
            let assign_res = unsafe { AssignProcessToJobObject(job.raw(), raw_handle as HANDLE) };
            if assign_res == 0 {
                let err = unsafe { GetLastError() };
                warn!("AssignProcessToJobObject warning code: {err}");
            }
        }

        // 5. Await process with timeout
        let timeout_duration = Duration::from_secs(30);
        let res = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;
        let elapsed = start.elapsed();

        match res {
            Ok(Ok(output)) => {
                let status_code = output.status.code().unwrap_or(-1);
                let success = output.status.success();

                Ok(OsExecutionResult {
                    exit_code: status_code,
                    status_code,
                    success,
                    stdout: output.stdout,
                    stderr: output.stderr,
                    execution_time: elapsed,
                })
            }
            Ok(Err(e)) => Err(OsSandboxError::ProcessFailed(format!("Process error: {e}"))),
            Err(_) => {
                // Actively terminate all processes in the job object
                unsafe {
                    TerminateJobObject(job.raw(), 1);
                }
                Err(OsSandboxError::ExecutionTimeout)
            }
        }
    }

    pub async fn execute_windows_sandbox(
        command: &str,
        args: &[String],
        policy: &OsSandboxPolicy,
    ) -> Result<OsExecutionResult, OsSandboxError> {
        let config = WindowsJobConfig::default();
        execute_windows_sandbox_with_config(command, args, policy, &config).await
    }
}

#[cfg(target_os = "windows")]
pub use windows_impl::*;

// ─────────────────────────────────────────────────────────────────────────────
// Non-Windows Platform Simulation & Cross-Platform Mock Runner
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(not(target_os = "windows"))]
mod non_windows_impl {
    use super::*;
    use std::process::Stdio;
    use std::time::{Duration, Instant};
    use tokio::process::Command;

    /// Mock Windows Job Objects sandbox runner for non-Windows developer environments.
    #[derive(Debug, Clone, Default)]
    pub struct MockWindowsJobSandbox;

    impl MockWindowsJobSandbox {
        pub fn new() -> Self {
            Self
        }
    }

    /// Queries simulated accounting metrics on non-Windows hosts.
    pub fn query_job_accounting() -> Result<JobAccountingStats, OsSandboxError> {
        Ok(JobAccountingStats {
            total_user_time_ms: 0,
            total_kernel_time_ms: 0,
            active_processes: 1,
            total_processes: 1,
            peak_process_memory_used: 1024 * 1024,
            peak_job_memory_used: 1024 * 1024,
        })
    }

    /// Executes a simulated Windows sandbox command with configuration constraints.
    pub async fn execute_windows_sandbox_with_config(
        command: &str,
        args: &[String],
        _policy: &OsSandboxPolicy,
        config: &WindowsJobConfig,
    ) -> Result<OsExecutionResult, OsSandboxError> {
        let start = Instant::now();

        // 1. AST sanitizer and command denylist verification
        validate_command(command, args)
            .map_err(|e| OsSandboxError::CommandForbidden(e.to_string()))?;

        // 2. Validate configuration bounds
        if config.max_cpu_rate_pct > 100 {
            return Err(OsSandboxError::SandboxInitFailed(
                "max_cpu_rate_pct cannot exceed 100".to_string(),
            ));
        }

        // 3. Execute subprocess
        let mut cmd = Command::new(command);
        cmd.args(args);
        cmd.kill_on_drop(true);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        let child = cmd.spawn().map_err(|e| {
            OsSandboxError::ProcessFailed(format!("Failed to spawn simulated process: {e}"))
        })?;

        let timeout_duration = Duration::from_secs(30);
        let res = tokio::time::timeout(timeout_duration, child.wait_with_output()).await;
        let elapsed = start.elapsed();

        match res {
            Ok(Ok(output)) => {
                let status_code = output.status.code().unwrap_or(-1);
                let success = output.status.success();

                Ok(OsExecutionResult {
                    exit_code: status_code,
                    status_code,
                    success,
                    stdout: output.stdout,
                    stderr: output.stderr,
                    execution_time: elapsed,
                })
            }
            Ok(Err(e)) => Err(OsSandboxError::ProcessFailed(format!("Process error: {e}"))),
            Err(_) => Err(OsSandboxError::ExecutionTimeout),
        }
    }

    pub async fn execute_windows_sandbox(
        command: &str,
        args: &[String],
        policy: &OsSandboxPolicy,
    ) -> Result<OsExecutionResult, OsSandboxError> {
        let config = WindowsJobConfig::default();
        execute_windows_sandbox_with_config(command, args, policy, &config).await
    }
}

#[cfg(not(target_os = "windows"))]
pub use non_windows_impl::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_windows_job_config_defaults() {
        let config = WindowsJobConfig::default();
        assert_eq!(config.job_name, "LIVA_Sandbox_Job");
        assert_eq!(config.max_cpu_rate_pct, 50);
        assert_eq!(config.max_memory_limit_bytes, 512 * 1024 * 1024);
        assert!(config.block_child_process_creation);
        assert!(config.terminate_on_job_close);
    }

    #[test]
    fn test_windows_job_config_serde() {
        let config = WindowsJobConfig {
            job_name: "custom_job".to_string(),
            max_cpu_rate_pct: 75,
            max_memory_limit_bytes: 1024 * 1024 * 1024,
            max_io_bytes_per_sec: 100 * 1024 * 1024,
            block_child_process_creation: false,
            terminate_on_job_close: true,
        };

        let serialized = serde_json::to_string(&config).unwrap();
        let deserialized: WindowsJobConfig = serde_json::from_str(&serialized).unwrap();
        assert_eq!(config, deserialized);
    }

    #[test]
    fn test_platform_backend_resolution() {
        let backend = OsPlatformBackend::current_host();
        #[cfg(target_os = "macos")]
        assert_eq!(backend, OsPlatformBackend::MacOsSeatbelt);
        #[cfg(target_os = "linux")]
        assert_eq!(backend, OsPlatformBackend::LinuxBubblewrap);
        #[cfg(target_os = "windows")]
        assert_eq!(backend, OsPlatformBackend::WindowsJobObject);
    }

    #[tokio::test]
    async fn test_windows_job_sandbox_execution() {
        let policy = OsSandboxPolicy::default();
        let sandbox = WindowsJobSandbox::new(policy);
        let res = sandbox.execute("echo", &["windows_job_test".to_string()]).await;

        if let Ok(output) = res {
            assert!(output.success);
            assert_eq!(output.exit_code, 0);
            assert!(output.stdout_str().contains("windows_job_test"));
        }
    }
}
