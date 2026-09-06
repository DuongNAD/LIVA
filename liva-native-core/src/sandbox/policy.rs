//! Unified Sandboxing Policy, Capability Enforcement, Path Canonicalization & SSRF Filter
//!
//! Provides defense-in-depth isolation across Tier-1 Wasm and Tier-2 OS containment:
//! - Capability token authorization (`fs_read`, `fs_write`, `net_outbound`, `os_execute`, `vision_capture`, `audio_record`, `keystore_access`).
//! - Monotonic capability attenuation preventing privilege escalation across agent delegations.
//! - Path canonicalization guard preventing traversal, symlink escapes, and null-byte injections.
//! - Network SSRF filter blocking cloud metadata endpoints, loopback, and RFC 1918 subnets.
//! - AST command sanitizer blocking destructive shell sequences, fork bombs, and disguised traversal args.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{debug, warn};

pub use crate::sandbox::path_validator::CanonicalPathValidator;
pub use crate::sandbox::ssrf_filter::SsrfFilter;

/// Capability tokens defining granular privileges granted to sandboxed execution environments.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CapabilityToken {
    /// Read filesystem access under allowed directories.
    FsRead,
    /// Write filesystem access under allowed directories.
    FsWrite,
    /// Outbound network communication with allowlisted hosts.
    NetOutbound,
    /// Host OS subprocess execution (cargo, python, git, etc.).
    OsExecute,
    /// Screen capture and vision analysis.
    VisionCapture,
    /// Audio input/microphone recording.
    AudioRecord,
    /// Secure keystore and credentials access.
    KeystoreAccess,
}

impl CapabilityToken {
    /// Returns the canonical snake_case string representation of the token.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::FsRead => "fs_read",
            Self::FsWrite => "fs_write",
            Self::NetOutbound => "net_outbound",
            Self::OsExecute => "os_execute",
            Self::VisionCapture => "vision_capture",
            Self::AudioRecord => "audio_record",
            Self::KeystoreAccess => "keystore_access",
        }
    }
}

impl std::fmt::Display for CapabilityToken {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl FromStr for CapabilityToken {
    type Err = SandboxViolation;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let normalized = s.trim().to_lowercase().replace('-', "_");
        match normalized.as_str() {
            "fs_read" | "fsread" => Ok(Self::FsRead),
            "fs_write" | "fswrite" => Ok(Self::FsWrite),
            "net_outbound" | "netoutbound" => Ok(Self::NetOutbound),
            "os_execute" | "osexecute" => Ok(Self::OsExecute),
            "vision_capture" | "visioncapture" => Ok(Self::VisionCapture),
            "audio_record" | "audiorecord" => Ok(Self::AudioRecord),
            "keystore_access" | "keystoreaccess" => Ok(Self::KeystoreAccess),
            _ => Err(SandboxViolation::CapabilityMissing(format!(
                "Unknown capability token: {s}"
            ))),
        }
    }
}

/// Violations and security faults detected by the sandbox engine.
#[derive(Debug, thiserror::Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SandboxViolation {
    #[error("Required capability missing: {0}")]
    CapabilityMissing(String),

    #[error("Target domain blocked by security policy: {0}")]
    BlockedDomain(String),

    #[error("SSRF or Private IP access prohibited: {0}")]
    SsrfAttempt(String),

    #[error("Path traversal or jailbreak detected: {0}")]
    PathJailbreak(String),

    #[error("Read access denied: {0:?}")]
    ReadDenied(PathBuf),

    #[error("Write access denied: {0:?}")]
    WriteDenied(PathBuf),

    #[error("Destructive or forbidden command blocked: {0}")]
    DestructiveCommand(String),

    #[error("Resource limit exceeded: {0}")]
    ResourceExceeded(String),

    #[error("Unauthorized process execution: {0}")]
    UnauthorizedProcess(String),

    #[error("Sandbox execution error: {0}")]
    ExecutionError(String),
}

/// Unified security policy for sandboxed execution (Wasmtime & OS containment).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SandboxPolicy {
    /// Capabilities explicitly granted to this sandbox session.
    pub capabilities: HashSet<CapabilityToken>,
    /// Root workspace path if anchored.
    pub workspace_root: Option<PathBuf>,
    /// Allowed domain wildcards or exact hostnames.
    pub allowed_domains: Vec<String>,
    /// Explicitly blocked domain patterns or hosts.
    pub blocked_domains: Vec<String>,
    /// Paths permitted for read-only access.
    pub allowed_read_paths: Vec<PathBuf>,
    /// Paths permitted for read-write access.
    pub allowed_write_paths: Vec<PathBuf>,
    /// Shell command fragments or regex patterns strictly forbidden.
    pub command_denylist: Vec<String>,
    /// Maximum execution time in seconds.
    pub max_execution_time_secs: u64,
    /// Maximum memory limit in megabytes (Tier 1 Wasm defaults to 64MB hard cap).
    pub max_memory_mb: u64,
    /// Initial fuel budget for Wasm execution.
    pub initial_fuel: u64,
    /// Epoch deadline ticks.
    pub epoch_deadline_ticks: u64,
    /// Whether spawned child processes are permitted in OS sandbox.
    pub allow_child_processes: bool,
}

impl Default for SandboxPolicy {
    fn default() -> Self {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);
        Self {
            capabilities: caps,
            workspace_root: None,
            allowed_domains: vec!["*".to_string()],
            blocked_domains: vec![],
            allowed_read_paths: vec![],
            allowed_write_paths: vec![],
            command_denylist: vec![
                "rm -rf".to_string(),
                "mkfs".to_string(),
                ":(){ :|:& };:".to_string(),
                "dd if=".to_string(),
                "> /dev/sd".to_string(),
                "> /dev/nvme".to_string(),
                "chmod -R 777 /".to_string(),
                "chown -R".to_string(),
                "shutdown".to_string(),
                "reboot".to_string(),
                "init 0".to_string(),
                "/dev/null > /dev/sda".to_string(),
                "nc -e".to_string(),
                "fork()".to_string(),
            ],
            max_execution_time_secs: 30,
            max_memory_mb: 64,
            initial_fuel: 50_000_000,
            epoch_deadline_ticks: 50,
            allow_child_processes: false,
        }
    }
}

impl SandboxPolicy {
    /// Constructs a new `SandboxPolicy` with explicit capability set and workspace root.
    pub fn new(capabilities: HashSet<CapabilityToken>, workspace_root: PathBuf) -> Self {
        Self {
            capabilities,
            workspace_root: Some(workspace_root.clone()),
            allowed_read_paths: vec![workspace_root],
            ..Self::default()
        }
    }

    /// Creates a strict read-only policy anchored to a root workspace directory.
    pub fn strict_readonly(root: &Path) -> Self {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);
        Self {
            capabilities: caps,
            workspace_root: Some(root.to_path_buf()),
            allowed_domains: vec![],
            blocked_domains: vec!["*".to_string()],
            allowed_read_paths: vec![root.to_path_buf()],
            allowed_write_paths: vec![],
            command_denylist: Self::default().command_denylist,
            max_execution_time_secs: 5,
            max_memory_mb: 64,
            initial_fuel: 20_000_000,
            epoch_deadline_ticks: 50,
            allow_child_processes: false,
        }
    }

    /// Checks if a capability is granted in this policy.
    pub fn has_capability(&self, cap: CapabilityToken) -> bool {
        self.capabilities.contains(&cap)
    }

    /// Enforces that a capability is present, returning an error if missing.
    pub fn require_capability(&self, cap: CapabilityToken) -> Result<(), SandboxViolation> {
        if self.has_capability(cap) {
            Ok(())
        } else {
            Err(SandboxViolation::CapabilityMissing(cap.to_string()))
        }
    }

    /// Adds an allowed read path to the policy.
    pub fn allow_read_path(&mut self, path: PathBuf) {
        if !self.allowed_read_paths.contains(&path) {
            self.allowed_read_paths.push(path);
        }
    }

    /// Adds an allowed write path to the policy.
    pub fn allow_write_path(&mut self, path: PathBuf) {
        if !self.allowed_write_paths.contains(&path) {
            self.allowed_write_paths.push(path);
        }
    }

    /// Whether network egress is permitted under this policy.
    pub fn allow_network(&self) -> bool {
        self.has_capability(CapabilityToken::NetOutbound)
            || (!self.allowed_domains.is_empty() && !self.blocked_domains.contains(&"*".to_string()))
    }

    /// Monotonically attenuates permissions to derive a child policy.
    /// Enforces non-escalation: child policy can never exceed parent's capabilities,
    /// paths, or resource limits.
    pub fn attenuate(&self, requested: &SandboxPolicy) -> Result<SandboxPolicy, SandboxViolation> {
        // 1. Verify capability subset
        for cap in &requested.capabilities {
            if !self.capabilities.contains(cap) {
                return Err(SandboxViolation::CapabilityMissing(format!(
                    "Cannot delegate capability '{}' not held by parent",
                    cap
                )));
            }
        }

        // 2. Verify read path containment
        for r_path in &requested.allowed_read_paths {
            let is_contained = self.allowed_read_paths.is_empty()
                || self
                    .allowed_read_paths
                    .iter()
                    .any(|parent_r| r_path.starts_with(parent_r));
            if !is_contained {
                return Err(SandboxViolation::PathJailbreak(format!(
                    "Requested read path {:?} exceeds parent bounds",
                    r_path
                )));
            }
        }

        // 3. Verify write path containment
        for w_path in &requested.allowed_write_paths {
            let is_contained = self.allowed_write_paths.is_empty()
                || self
                    .allowed_write_paths
                    .iter()
                    .any(|parent_w| w_path.starts_with(parent_w));
            if !is_contained {
                return Err(SandboxViolation::PathJailbreak(format!(
                    "Requested write path {:?} exceeds parent bounds",
                    w_path
                )));
            }
        }

        // 4. Bounded resource caps (min of requested and parent)
        let max_time = requested
            .max_execution_time_secs
            .min(self.max_execution_time_secs);
        let max_mem = requested.max_memory_mb.min(self.max_memory_mb);
        let initial_fuel = requested.initial_fuel.min(self.initial_fuel);
        let epoch_deadline_ticks = requested
            .epoch_deadline_ticks
            .min(self.epoch_deadline_ticks);
        let allow_child_processes = self.allow_child_processes && requested.allow_child_processes;

        // 5. Blocked domains union
        let mut blocked_domains = self.blocked_domains.clone();
        for b in &requested.blocked_domains {
            if !blocked_domains.contains(b) {
                blocked_domains.push(b.clone());
            }
        }

        // 6. Command denylist union
        let mut command_denylist = self.command_denylist.clone();
        for d in &requested.command_denylist {
            if !command_denylist.contains(d) {
                command_denylist.push(d.clone());
            }
        }

        Ok(SandboxPolicy {
            capabilities: requested.capabilities.clone(),
            workspace_root: self.workspace_root.clone(),
            allowed_domains: requested.allowed_domains.clone(),
            blocked_domains,
            allowed_read_paths: requested.allowed_read_paths.clone(),
            allowed_write_paths: requested.allowed_write_paths.clone(),
            command_denylist,
            max_execution_time_secs: max_time,
            max_memory_mb: max_mem,
            initial_fuel,
            epoch_deadline_ticks,
            allow_child_processes,
        })
    }
}

/// Evaluates whether a shell command and its arguments contain destructive patterns or path traversal attempts.
pub fn validate_command(command: &str, args: &[String]) -> Result<(), SandboxViolation> {
    let trimmed_cmd = command.trim();
    let lower_cmd = trimmed_cmd.to_lowercase();

    // 1. Destructive executable names
    let destructive_executables = [
        "mkfs", "dd", "shutdown", "reboot", "init", "format", "fdisk",
    ];
    for denied in &destructive_executables {
        if lower_cmd == *denied || lower_cmd.starts_with(&format!("{denied}.")) {
            warn!("Destructive command blocked: {}", command);
            return Err(SandboxViolation::DestructiveCommand(command.to_string()));
        }
    }

    // 2. Destructive command combinations (e.g. `rm -rf /` or `rm -rf target`)
    if lower_cmd == "rm" {
        for arg in args {
            let l_arg = arg.to_lowercase();
            if l_arg.contains("-r") || l_arg.contains("-f") || l_arg.contains("--recursive") {
                warn!("Destructive rm recursive flag blocked: {:?}", args);
                return Err(SandboxViolation::DestructiveCommand(format!("rm {arg}")));
            }
        }
    }

    // 3. Inspect arguments for path traversal escapes (`..` in arguments)
    for arg in args {
        if arg.contains("..") {
            warn!("Path traversal escape detected in CLI argument: {}", arg);
            return Err(SandboxViolation::PathJailbreak(arg.clone()));
        }
    }

    // 4. Heuristic pattern checks
    let full_line = format!("{} {}", command, args.join(" ")).to_lowercase();
    if full_line.contains(":(){ :|:& };:") || full_line.contains("fork()") {
        return Err(SandboxViolation::DestructiveCommand("fork_bomb".to_string()));
    }
    if full_line.contains("> /dev/sd") || full_line.contains("> /dev/nvme") {
        return Err(SandboxViolation::DestructiveCommand("raw_disk_overwrite".to_string()));
    }
    if full_line.contains("rm -rf") {
        return Err(SandboxViolation::DestructiveCommand("rm -rf".to_string()));
    }

    debug!("Command passed sandbox validation: {} {:?}", command, args);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_tokens() {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);
        let policy = SandboxPolicy::new(caps, PathBuf::from("/tmp"));

        assert!(policy.has_capability(CapabilityToken::FsRead));
        assert!(!policy.has_capability(CapabilityToken::OsExecute));

        assert!(policy.require_capability(CapabilityToken::FsRead).is_ok());
        assert!(policy.require_capability(CapabilityToken::OsExecute).is_err());
    }

    #[test]
    fn test_capability_token_from_str() {
        assert_eq!("fs_read".parse::<CapabilityToken>().unwrap(), CapabilityToken::FsRead);
        assert_eq!("FS_WRITE".parse::<CapabilityToken>().unwrap(), CapabilityToken::FsWrite);
        assert_eq!("net-outbound".parse::<CapabilityToken>().unwrap(), CapabilityToken::NetOutbound);
        assert_eq!("os_execute".parse::<CapabilityToken>().unwrap(), CapabilityToken::OsExecute);
        assert_eq!("vision_capture".parse::<CapabilityToken>().unwrap(), CapabilityToken::VisionCapture);
        assert_eq!("audio_record".parse::<CapabilityToken>().unwrap(), CapabilityToken::AudioRecord);
        assert_eq!("keystore_access".parse::<CapabilityToken>().unwrap(), CapabilityToken::KeystoreAccess);
        assert!("invalid_token".parse::<CapabilityToken>().is_err());
    }

    #[test]
    fn test_policy_attenuation() {
        let mut parent_caps = HashSet::new();
        parent_caps.insert(CapabilityToken::FsRead);
        parent_caps.insert(CapabilityToken::NetOutbound);

        let parent_policy = SandboxPolicy {
            capabilities: parent_caps,
            allowed_read_paths: vec![PathBuf::from("/workspace/src")],
            max_execution_time_secs: 30,
            max_memory_mb: 128,
            ..SandboxPolicy::default()
        };

        // Valid child request
        let mut child_caps = HashSet::new();
        child_caps.insert(CapabilityToken::FsRead);
        let requested_child = SandboxPolicy {
            capabilities: child_caps,
            allowed_read_paths: vec![PathBuf::from("/workspace/src/sub")],
            max_execution_time_secs: 60, // should be capped at 30
            max_memory_mb: 64,
            ..SandboxPolicy::default()
        };

        let child = parent_policy.attenuate(&requested_child).unwrap();
        assert_eq!(child.max_execution_time_secs, 30);
        assert_eq!(child.max_memory_mb, 64);
        assert!(child.has_capability(CapabilityToken::FsRead));
        assert!(!child.has_capability(CapabilityToken::NetOutbound));

        // Invalid child request: escalating capability
        let mut bad_caps = HashSet::new();
        bad_caps.insert(CapabilityToken::OsExecute);
        let bad_request = SandboxPolicy {
            capabilities: bad_caps,
            ..SandboxPolicy::default()
        };
        assert!(parent_policy.attenuate(&bad_request).is_err());
    }

    #[test]
    fn test_ssrf_filter_rejection() {
        let filter = SsrfFilter::new();
        assert!(filter.validate_url("https://169.254.169.254/latest/meta-data").is_err());
        assert!(filter.validate_url("http://metadata.google.internal/computeMetadata/v1/").is_err());
        assert!(filter.validate_url("http://127.0.0.1:8080/api").is_err());
        assert!(filter.validate_url("http://localhost:3000").is_err());
        assert!(filter.validate_url("http://192.168.1.1/admin").is_err());
        assert!(filter.validate_url("http://10.0.0.5/secret").is_err());
        assert!(filter.validate_url("https://api.github.com/repos").is_ok());
    }

    #[test]
    fn test_command_validation_ast_sanitizer() {
        assert!(validate_command("cargo", &["check".to_string()]).is_ok());
        assert!(validate_command("git", &["status".to_string()]).is_ok());
        assert!(validate_command("rm", &["-rf".to_string(), "/".to_string()]).is_err());
        assert!(validate_command("mkfs", &["/dev/sda".to_string()]).is_err());
        assert!(validate_command("cargo", &["--manifest-path".to_string(), "../../etc/passwd".to_string()]).is_err());
    }
}
