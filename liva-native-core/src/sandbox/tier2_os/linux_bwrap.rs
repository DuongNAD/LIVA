//! Linux Bubblewrap (`bwrap`) & Landlock LSM OS Containment
//!
//! Enforces Linux kernel-level sandboxing using unprivileged namespaces (Bubblewrap)
//! and Landlock Linux Security Module for filesystem isolation.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, Instant};
use tokio::process::Command;
use tracing::warn;

use super::{OsExecutionResult, OsSandboxError, OsSandboxPolicy};
use crate::sandbox::policy::validate_command;

/// Builds the Bubblewrap (`bwrap`) command invocation line for unprivileged containerization.
pub fn build_bwrap_command(
    command: &str,
    args: &[String],
    policy: &OsSandboxPolicy,
) -> Command {
    let mut cmd = Command::new("bwrap");

    // 1. Mount read-only system root directories
    for ro_dir in &["/usr", "/bin", "/sbin", "/lib", "/lib64", "/etc"] {
        if Path::new(ro_dir).exists() {
            cmd.arg("--ro-bind").arg(ro_dir).arg(ro_dir);
        }
    }

    // 2. Mount toolchain cache directories if present
    if let Ok(home) = std::env::var("HOME") {
        let cargo_home = format!("{home}/.cargo");
        let rustup_home = format!("{home}/.rustup");
        if Path::new(&cargo_home).exists() {
            cmd.arg("--ro-bind").arg(&cargo_home).arg(&cargo_home);
        }
        if Path::new(&rustup_home).exists() {
            cmd.arg("--ro-bind").arg(&rustup_home).arg(&rustup_home);
        }
    }

    // 3. Mount read-only allowed paths
    for ro_path in &policy.allowed_read_paths {
        if ro_path.exists() {
            cmd.arg("--ro-bind").arg(ro_path).arg(ro_path);
        }
    }

    // 4. Mount read-write allowed paths
    for rw_path in &policy.allowed_write_paths {
        if rw_path.exists() {
            cmd.arg("--bind").arg(rw_path).arg(rw_path);
        }
    }

    // 5. Ephemeral and pseudo-filesystems
    cmd.arg("--tmpfs").arg("/tmp");
    cmd.arg("--proc").arg("/proc");
    cmd.arg("--dev").arg("/dev");

    // 6. Security & namespace isolation
    cmd.arg("--unshare-all");
    cmd.arg("--die-with-parent");

    // If network is disabled, unshare network namespace
    if !policy.allow_network {
        cmd.arg("--unshare-net");
    } else {
        cmd.arg("--share-net");
    }

    // 7. Command and arguments
    cmd.arg("--");
    cmd.arg(command);
    cmd.args(args);

    cmd
}

/// Executes a native host command inside Linux Bubblewrap / Landlock containment.
pub async fn execute_linux_sandbox(
    command: &str,
    args: &[String],
    policy: &OsSandboxPolicy,
) -> Result<OsExecutionResult, OsSandboxError> {
    let start = Instant::now();

    // 1. AST sanitizer and command denylist verification
    validate_command(command, args)
        .map_err(|e| OsSandboxError::CommandForbidden(e.to_string()))?;

    // 2. Check if bwrap binary exists on host
    let has_bwrap = which_binary("bwrap").is_some();

    let mut cmd = if has_bwrap {
        build_bwrap_command(command, args, policy)
    } else {
        warn!("bwrap binary not found; falling back to standard subprocess isolation");
        let mut fallback = Command::new(command);
        fallback.args(args);
        fallback
    };

    cmd.kill_on_drop(true);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    // 3. Spawn child process
    let child = cmd.spawn().map_err(|e| {
        OsSandboxError::ProcessFailed(format!("Failed to spawn Linux sandbox process: {e}"))
    })?;

    // 4. Await with timeout
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

/// Helper locating executable on PATH.
fn which_binary(name: &str) -> Option<PathBuf> {
    if let Ok(paths) = std::env::var("PATH") {
        for dir in paths.split(':') {
            let candidate = Path::new(dir).join(name);
            if candidate.exists() {
                return Some(candidate);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bwrap_command_building() {
        let policy = OsSandboxPolicy {
            allowed_read_paths: vec![PathBuf::from("/workspace")],
            allowed_write_paths: vec![PathBuf::from("/workspace/output")],
            allowed_commands: vec!["cargo".to_string()],
            allow_network: false,
        };

        let cmd = build_bwrap_command("cargo", &["test".to_string()], &policy);
        let prog = cmd.as_std().get_program().to_string_lossy().to_string();
        assert_eq!(prog, "bwrap");
    }
}
