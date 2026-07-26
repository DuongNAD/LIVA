use std::path::Path;
use std::process::Stdio;
use tokio::io::AsyncReadExt;
use tokio::process::Command;
use tokio::time::{Duration, timeout};

#[derive(Debug)]
pub enum SandboxError {
    Io(std::io::Error),
    Timeout,
    SpawnFailed(String),
}

impl std::fmt::Display for SandboxError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SandboxError::Io(e) => write!(f, "IO error: {}", e),
            SandboxError::Timeout => write!(f, "Sandbox execution timed out after 30 seconds"),
            SandboxError::SpawnFailed(s) => write!(f, "Spawn failed: {}", s),
        }
    }
}

impl std::error::Error for SandboxError {}

impl From<std::io::Error> for SandboxError {
    fn from(err: std::io::Error) -> Self {
        SandboxError::Io(err)
    }
}

#[derive(Debug, Clone)]
pub struct TestOutput {
    pub success: bool,
    pub stdout: String,
    pub stderr: String,
}

pub struct Sandbox;

impl Sandbox {
    pub async fn run_tests(project_path: &Path) -> Result<TestOutput, SandboxError> {
        let target_dir = project_path.join("target_sandbox");

        let mut cmd = Command::new("cargo");
        cmd.arg("test")
            .current_dir(project_path)
            .env("CARGO_TARGET_DIR", &target_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                // On Windows, try running with cmd /c in case cargo is not directly executable
                #[cfg(target_os = "windows")]
                {
                    let mut cmd_win = Command::new("cmd");
                    cmd_win
                        .args(["/C", "cargo", "test"])
                        .current_dir(project_path)
                        .env("CARGO_TARGET_DIR", &target_dir)
                        .stdout(Stdio::piped())
                        .stderr(Stdio::piped());
                    match cmd_win.spawn() {
                        Ok(child) => child,
                        Err(win_err) => {
                            return Err(SandboxError::SpawnFailed(format!(
                                "Failed to spawn cargo ({}) or cmd /C cargo ({})",
                                e, win_err
                            )));
                        }
                    }
                }
                #[cfg(not(target_os = "windows"))]
                {
                    return Err(SandboxError::SpawnFailed(format!(
                        "Failed to spawn cargo: {}",
                        e
                    )));
                }
            }
        };

        // Take stdout and stderr handles
        let mut stdout = child.stdout.take().ok_or_else(|| {
            SandboxError::SpawnFailed("Failed to capture stdout stream".to_string())
        })?;
        let mut stderr = child.stderr.take().ok_or_else(|| {
            SandboxError::SpawnFailed("Failed to capture stderr stream".to_string())
        })?;

        let mut stdout_bytes = Vec::new();
        let mut stderr_bytes = Vec::new();

        // Build futures to read to end and wait for child
        let read_stdout = stdout.read_to_end(&mut stdout_bytes);
        let read_stderr = stderr.read_to_end(&mut stderr_bytes);
        let wait_child = child.wait();

        let run_future = async {
            let (status_res, stdout_res, stderr_res) =
                tokio::join!(wait_child, read_stdout, read_stderr);
            let status = status_res?;
            stdout_res?;
            stderr_res?;
            Ok::<_, std::io::Error>(status)
        };

        match timeout(Duration::from_secs(30), run_future).await {
            Ok(Ok(status)) => {
                let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
                let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();
                let success = status.success();
                Ok(TestOutput {
                    success,
                    stdout,
                    stderr,
                })
            }
            Ok(Err(e)) => Err(SandboxError::Io(e)),
            Err(_) => {
                // If it times out, kill the child process and its grandchildren
                #[cfg(target_os = "windows")]
                {
                    if let Some(pid) = child.id() {
                        let _ = Command::new("taskkill")
                            .args(["/F", "/T", "/PID", &pid.to_string()])
                            .status()
                            .await;
                    }
                }
                let _ = child.kill().await;
                Err(SandboxError::Timeout)
            }
        }
    }
}
