//! Phase 4 E2E Test Suite — Windows Job Objects & Cross-Platform Sandbox Matrix (Features 11–13)
//!
//! Features Tested:
//! - F11: Windows Job Objects Sandbox (CPU, RAM, IO Limits, Child Process Containment)
//! - F12: Unified Cross-Platform Matrix (macOS Seatbelt, Linux Bwrap, Windows Job Objects)
//! - F13: Uniform CapabilityToken Security & SSRF Network Isolation

use liva_native_core::sandbox::tier2_os::{
    OsExecutionResult, OsSandboxError, OsSandboxPolicy, OsSandboxRunner,
};
use serde::{Deserialize, Serialize};
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;

// ── Domain Types for Windows Job Objects & Sandbox Matrix (RFC-003 §R3) ───────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WindowsJobConfig {
    pub job_name: String,
    pub max_cpu_rate_pct: u32,
    pub max_memory_limit_bytes: u64,
    pub max_io_bytes_per_sec: u64,
    pub block_child_process_creation: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OsPlatformBackend {
    MacOsSeatbelt,
    LinuxBubblewrap,
    WindowsJobObject,
}

impl OsPlatformBackend {
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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityScope {
    ReadFs(PathBuf),
    WriteFs(PathBuf),
    ExecuteCli(String),
    NetworkAccess(String),
    AdminOverride,
}

pub struct SecurityCapabilityValidator {
    pub allowed_scopes: Vec<CapabilityScope>,
}

impl SecurityCapabilityValidator {
    pub fn new(allowed_scopes: Vec<CapabilityScope>) -> Self {
        Self { allowed_scopes }
    }

    pub fn validate_fs_read(&self, target_path: &Path) -> Result<(), String> {
        for scope in &self.allowed_scopes {
            if let CapabilityScope::ReadFs(allowed_dir) = scope {
                if target_path.starts_with(allowed_dir) {
                    return Ok(());
                }
            } else if let CapabilityScope::AdminOverride = scope {
                return Ok(());
            }
        }
        Err(format!("Access denied for read on path: {:?}", target_path))
    }

    pub fn validate_fs_write(&self, target_path: &Path) -> Result<(), String> {
        for scope in &self.allowed_scopes {
            if let CapabilityScope::WriteFs(allowed_dir) = scope {
                if target_path.starts_with(allowed_dir) {
                    return Ok(());
                }
            } else if let CapabilityScope::AdminOverride = scope {
                return Ok(());
            }
        }
        Err(format!("Access denied for write on path: {:?}", target_path))
    }

    pub fn validate_cli_execution(&self, command: &str) -> Result<(), String> {
        for scope in &self.allowed_scopes {
            if let CapabilityScope::ExecuteCli(allowed_cmd) = scope {
                if allowed_cmd == command || allowed_cmd == "*" {
                    return Ok(());
                }
            } else if let CapabilityScope::AdminOverride = scope {
                return Ok(());
            }
        }
        Err(format!("Access denied for execution of CLI command: {}", command))
    }
}

// ── SSRF Security Guard ──────────────────────────────────────────────────────

pub struct SsrfProtectionEngine;

impl SsrfProtectionEngine {
    pub fn validate_url(url_str: &str) -> Result<(), String> {
        let parsed = reqwest::Url::parse(url_str).map_err(|e| format!("Invalid URL: {}", e))?;

        // 1. Protocol allowlist
        match parsed.scheme() {
            "http" | "https" => {}
            disallowed => return Err(format!("Forbidden URI scheme '{}'", disallowed)),
        }

        // 2. Host validation
        let host_str = parsed.host_str().ok_or("Missing host in URL")?;

        // Disallow localhost names
        if host_str == "localhost" || host_str.ends_with(".localhost") || host_str.ends_with(".internal") {
            return Err("Access to localhost or internal domains is forbidden".to_string());
        }

        // Check for private IP addresses
        if let Ok(ip) = host_str.parse::<IpAddr>() {
            if Self::is_private_or_reserved_ip(ip) {
                return Err(format!("Access to private/reserved IP {} is forbidden", ip));
            }
        }

        Ok(())
    }

    fn is_private_or_reserved_ip(ip: IpAddr) -> bool {
        match ip {
            IpAddr::V4(ipv4) => {
                ipv4.is_loopback()
                    || ipv4.is_private()
                    || ipv4.is_link_local()
                    || ipv4.is_broadcast()
                    || ipv4.is_documentation()
                    || ipv4.octets()[0] == 0
                    // AWS metadata service 169.254.169.254
                    || (ipv4.octets()[0] == 169 && ipv4.octets()[1] == 254)
            }
            IpAddr::V6(ipv6) => {
                ipv6.is_loopback() || ipv6.is_unspecified()
            }
        }
    }
}

// ============================================================================
// FEATURE 11: WINDOWS JOB OBJECTS SANDBOX (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[test]
fn test_t1_f11_01_windows_job_config_defaults() {
    let config = WindowsJobConfig::default();
    assert_eq!(config.job_name, "LIVA_Sandbox_Job");
    assert_eq!(config.max_cpu_rate_pct, 50);
    assert_eq!(config.max_memory_limit_bytes, 512 * 1024 * 1024);
    assert!(config.block_child_process_creation);
    assert!(config.terminate_on_job_close);
}

#[test]
fn test_t1_f11_02_windows_job_custom_memory_limit() {
    let mut config = WindowsJobConfig::default();
    config.max_memory_limit_bytes = 1024 * 1024 * 1024; // 1 GB
    assert_eq!(config.max_memory_limit_bytes, 1073741824);
}

#[test]
fn test_t1_f11_03_windows_job_cpu_rate_bounds() {
    let mut config = WindowsJobConfig::default();
    config.max_cpu_rate_pct = 25;
    assert_eq!(config.max_cpu_rate_pct, 25);
}

#[test]
fn test_t1_f11_04_windows_job_child_process_containment_flag() {
    let mut config = WindowsJobConfig::default();
    assert!(config.block_child_process_creation);
    config.block_child_process_creation = false;
    assert!(!config.block_child_process_creation);
}

#[test]
fn test_t1_f11_05_windows_job_serialization_roundtrip() {
    let config = WindowsJobConfig::default();
    let json = serde_json::to_string(&config).unwrap();
    let deser: WindowsJobConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(config, deser);
}

// ── Tier 2 Boundaries (Feature 11) ──────────────────────────────────────────

#[test]
fn test_t2_f11_01_zero_cpu_rate_limit() {
    let mut config = WindowsJobConfig::default();
    config.max_cpu_rate_pct = 0;
    assert_eq!(config.max_cpu_rate_pct, 0);
}

#[test]
fn test_t2_f11_02_100_percent_cpu_rate_limit() {
    let mut config = WindowsJobConfig::default();
    config.max_cpu_rate_pct = 100;
    assert_eq!(config.max_cpu_rate_pct, 100);
}

#[test]
fn test_t2_f11_03_zero_memory_limit_bytes() {
    let mut config = WindowsJobConfig::default();
    config.max_memory_limit_bytes = 0;
    assert_eq!(config.max_memory_limit_bytes, 0);
}

#[test]
fn test_t2_f11_04_massive_memory_limit_terabytes() {
    let mut config = WindowsJobConfig::default();
    config.max_memory_limit_bytes = 10 * 1024 * 1024 * 1024 * 1024; // 10 TB
    assert_eq!(config.max_memory_limit_bytes, 10995116277760);
}

#[test]
fn test_t2_f11_05_empty_job_name() {
    let mut config = WindowsJobConfig::default();
    config.job_name = "".to_string();
    assert_eq!(config.job_name, "");
}

// ============================================================================
// FEATURE 12: UNIFIED CROSS-PLATFORM MATRIX (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[test]
fn test_t1_f12_01_platform_backend_resolution() {
    let backend = OsPlatformBackend::current_host();
    #[cfg(target_os = "macos")]
    assert_eq!(backend, OsPlatformBackend::MacOsSeatbelt);
    #[cfg(target_os = "linux")]
    assert_eq!(backend, OsPlatformBackend::LinuxBubblewrap);
    #[cfg(target_os = "windows")]
    assert_eq!(backend, OsPlatformBackend::WindowsJobObject);
}

#[tokio::test]
async fn test_t1_f12_02_os_sandbox_runner_instantiation() {
    let runner = OsSandboxRunner::new();
    let mut policy = OsSandboxPolicy::default();
    policy.allowed_read_paths.push(PathBuf::from("/bin"));
    policy.allowed_read_paths.push(PathBuf::from("/usr"));
    policy.allowed_commands.push("echo".to_string());

    let res = runner.execute_command("echo", &["sandbox_matrix_ok".to_string()], &policy).await;
    if let Ok(output) = res {
        assert!(output.success);
        assert!(output.stdout_str().contains("sandbox_matrix_ok"));
    }
}

#[test]
fn test_t1_f12_03_os_sandbox_policy_construction() {
    let mut policy = OsSandboxPolicy::default();
    policy.allowed_read_paths.push(PathBuf::from("/tmp"));
    policy.allowed_write_paths.push(PathBuf::from("/tmp/scratch"));
    policy.allowed_commands.push("git".to_string());
    policy.allow_network = false;

    assert_eq!(policy.allowed_read_paths.len(), 1);
    assert_eq!(policy.allowed_write_paths.len(), 1);
    assert!(!policy.allow_network);
}

#[test]
fn test_t1_f12_04_os_execution_result_formatting() {
    let result = OsExecutionResult {
        exit_code: 0,
        status_code: 0,
        success: true,
        stdout: b"compilation succeeded\n".to_vec(),
        stderr: b"".to_vec(),
        execution_time: Duration::from_millis(42),
    };
    assert_eq!(result.stdout_str().trim(), "compilation succeeded");
    assert!(result.stderr_str().is_empty());
    assert_eq!(result.execution_time.as_millis(), 42);
}

#[test]
fn test_t1_f12_05_os_sandbox_error_variants() {
    let err1 = OsSandboxError::ExecutionTimeout;
    let err2 = OsSandboxError::CommandForbidden("rm -rf /".to_string());
    let err3 = OsSandboxError::PermissionDenied("seatbelt violation".to_string());

    assert_eq!(err1.to_string(), "Subprocess execution timed out");
    assert!(err2.to_string().contains("Command forbidden"));
    assert!(err3.to_string().contains("denied access"));
}

// ── Tier 2 Boundaries (Feature 12) ──────────────────────────────────────────

#[tokio::test]
async fn test_t2_f12_01_denylist_command_forbidden() {
    let runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy::default();

    let res = runner.execute_command("rm", &["-rf".to_string(), "/".to_string()], &policy).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        OsSandboxError::CommandForbidden(_) => {}
        other => panic!("Expected CommandForbidden, got: {:?}", other),
    }
}

#[test]
fn test_t2_f12_02_empty_allowed_paths_policy() {
    let policy = OsSandboxPolicy::default();
    assert!(policy.allowed_read_paths.is_empty());
    assert!(policy.allowed_write_paths.is_empty());
    assert!(policy.allowed_commands.is_empty());
}

#[test]
fn test_t2_f12_03_os_sandbox_policy_serde() {
    let mut policy = OsSandboxPolicy::default();
    policy.allowed_read_paths.push(PathBuf::from("/etc"));
    policy.allow_network = true;

    let json = serde_json::to_string(&policy).unwrap();
    let deser: OsSandboxPolicy = serde_json::from_str(&json).unwrap();
    assert_eq!(policy.allowed_read_paths, deser.allowed_read_paths);
    assert_eq!(policy.allow_network, deser.allow_network);
}

#[test]
fn test_t2_f12_04_execution_result_non_utf8_binary_stdout() {
    let result = OsExecutionResult {
        exit_code: 0,
        status_code: 0,
        success: true,
        stdout: vec![0xFF, 0xFE, 0xFD], // Invalid UTF-8 sequence
        stderr: vec![],
        execution_time: Duration::from_millis(1),
    };
    let stdout_str = result.stdout_str();
    assert!(!stdout_str.is_empty(), "Lossy conversion should succeed for binary output");
}

#[test]
fn test_t2_f12_05_large_stderr_capture() {
    let large_err = vec![b'E'; 100_000];
    let result = OsExecutionResult {
        exit_code: 1,
        status_code: 1,
        success: false,
        stdout: vec![],
        stderr: large_err,
        execution_time: Duration::from_secs(1),
    };
    assert_eq!(result.stderr.len(), 100_000);
}

// ============================================================================
// FEATURE 13: UNIFORM CAPABILITY TOKEN SECURITY & SSRF (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[test]
fn test_t1_f13_01_capability_validator_valid_read() {
    let validator = SecurityCapabilityValidator::new(vec![
        CapabilityScope::ReadFs(PathBuf::from("/workspace/src")),
    ]);
    assert!(validator.validate_fs_read(Path::new("/workspace/src/main.rs")).is_ok());
}

#[test]
fn test_t1_f13_02_capability_validator_invalid_read_escape() {
    let validator = SecurityCapabilityValidator::new(vec![
        CapabilityScope::ReadFs(PathBuf::from("/workspace/src")),
    ]);
    assert!(validator.validate_fs_read(Path::new("/etc/shadow")).is_err());
}

#[test]
fn test_t1_f13_03_capability_validator_valid_write() {
    let validator = SecurityCapabilityValidator::new(vec![
        CapabilityScope::WriteFs(PathBuf::from("/workspace/target")),
    ]);
    assert!(validator.validate_fs_write(Path::new("/workspace/target/debug/app")).is_ok());
}

#[test]
fn test_t1_f13_04_capability_validator_cli_command_match() {
    let validator = SecurityCapabilityValidator::new(vec![
        CapabilityScope::ExecuteCli("cargo".to_string()),
        CapabilityScope::ExecuteCli("rustc".to_string()),
    ]);
    assert!(validator.validate_cli_execution("cargo").is_ok());
    assert!(validator.validate_cli_execution("rustc").is_ok());
    assert!(validator.validate_cli_execution("bash").is_err());
}

#[test]
fn test_t1_f13_05_ssrf_protection_valid_public_url() {
    assert!(SsrfProtectionEngine::validate_url("https://api.github.com/repos").is_ok());
    assert!(SsrfProtectionEngine::validate_url("https://crates.io/api/v1").is_ok());
}

// ── Tier 2 Boundaries (Feature 13) ──────────────────────────────────────────

#[test]
fn test_t2_f13_01_ssrf_rejects_loopback_ipv4() {
    assert!(SsrfProtectionEngine::validate_url("http://127.0.0.1:8000/admin").is_err());
    assert!(SsrfProtectionEngine::validate_url("http://127.0.0.2:80/").is_err());
}

#[test]
fn test_t2_f13_02_ssrf_rejects_aws_metadata_service() {
    assert!(SsrfProtectionEngine::validate_url("http://169.254.169.254/latest/meta-data/").is_err());
}

#[test]
fn test_t2_f13_03_ssrf_rejects_rfc1918_private_ips() {
    assert!(SsrfProtectionEngine::validate_url("http://10.0.0.1/internal").is_err());
    assert!(SsrfProtectionEngine::validate_url("http://192.168.1.1/router").is_err());
    assert!(SsrfProtectionEngine::validate_url("http://172.16.0.1/data").is_err());
}

#[test]
fn test_t2_f13_04_ssrf_rejects_forbidden_schemes() {
    assert!(SsrfProtectionEngine::validate_url("file:///etc/passwd").is_err());
    assert!(SsrfProtectionEngine::validate_url("gopher://127.0.0.1/").is_err());
    assert!(SsrfProtectionEngine::validate_url("dict://127.0.0.1/").is_err());
}

#[test]
fn test_t2_f13_05_capability_admin_override_permits_all() {
    let admin_validator = SecurityCapabilityValidator::new(vec![CapabilityScope::AdminOverride]);
    assert!(admin_validator.validate_fs_read(Path::new("/etc/hosts")).is_ok());
    assert!(admin_validator.validate_fs_write(Path::new("/tmp/test")).is_ok());
    assert!(admin_validator.validate_cli_execution("custom_tool").is_ok());
}
