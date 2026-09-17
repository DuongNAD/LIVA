//! Shared espeak-ng process resolution and IPA phonemization.
//!
//! espeak-ng may not be on PATH (a fresh winget install only updates PATH for
//! new shells), so resolution order is: `LIVA_ESPEAK_PATH` env → PATH → the
//! default Windows install locations. The resolved path is cached per process.

use std::io::Read;
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

fn resolve_espeak() -> PathBuf {
    if let Ok(p) = std::env::var("LIVA_ESPEAK_PATH") {
        let p = PathBuf::from(p);
        if p.exists() {
            return p;
        }
    }

    if Command::new("espeak-ng").arg("--version").output().is_ok() {
        return PathBuf::from("espeak-ng");
    }

    #[cfg(windows)]
    for candidate in [
        r"C:\Program Files\eSpeak NG\espeak-ng.exe",
        r"C:\Program Files (x86)\eSpeak NG\espeak-ng.exe",
    ] {
        let p = PathBuf::from(candidate);
        if p.exists() {
            return p;
        }
    }

    // Last resort: bare name — spawn will fail with a clear message.
    PathBuf::from("espeak-ng")
}

pub(crate) fn espeak_command() -> Command {
    static ESPEAK: OnceLock<PathBuf> = OnceLock::new();
    Command::new(ESPEAK.get_or_init(resolve_espeak))
}

/// Phonemize `text` to espeak IPA using the given espeak voice ("vi", "en-us").
///
/// Chạy tiến trình con đồng bộ có kiểm soát hạn chờ (mặc định 10 giây qua `LIVA_ESPEAK_TIMEOUT_SECS`).
/// Nếu tiến trình bị treo, vòng lặp polling sẽ ngắt, tiêu diệt cây tiến trình bằng taskkill / child.kill
/// và trả Err rõ ràng thay vì để luồng G2P/TTS bị khoá cứng vô hạn.
pub(crate) fn espeak_ipa(voice: &str, text: &str) -> Result<String, String> {
    let timeout_secs = std::env::var("LIVA_ESPEAK_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .unwrap_or(10);

    let mut child = espeak_command()
        .args(["-q", "--ipa", "-v", voice, "--", text])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("espeak-ng spawn failed: {}", e))?;

    let mut stdout_handle = child.stdout.take();
    let mut stderr_handle = child.stderr.take();

    let stdout_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut out) = stdout_handle.take() {
            let _ = out.read_to_end(&mut buf);
        }
        buf
    });
    let stderr_thread = std::thread::spawn(move || {
        let mut buf = Vec::new();
        if let Some(mut err) = stderr_handle.take() {
            let _ = err.read_to_end(&mut buf);
        }
        buf
    });

    let timeout = Duration::from_secs(timeout_secs);
    let start = Instant::now();
    let poll_interval = Duration::from_millis(10);

    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let stdout = stdout_thread.join().unwrap_or_default();
                let stderr = stderr_thread.join().unwrap_or_default();

                if !status.success() {
                    return Err(format!(
                        "espeak-ng exited with {}: {}",
                        status,
                        String::from_utf8_lossy(&stderr)
                    ));
                }

                return Ok(String::from_utf8_lossy(&stdout).trim().to_string());
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    tracing::warn!(
                        "espeak-ng xử lý IPA quá hạn {}s; tiến hành huỷ cây tiến trình",
                        timeout_secs
                    );
                    #[cfg(target_os = "windows")]
                    {
                        let pid = child.id();
                        let _ = Command::new("taskkill")
                            .args(["/F", "/T", "/PID", &pid.to_string()])
                            .output();
                    }
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_thread.join();
                    let _ = stderr_thread.join();
                    return Err(format!(
                        "espeak-ng phonemization timed out after {}s",
                        timeout_secs
                    ));
                }
                std::thread::sleep(poll_interval);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                let _ = stdout_thread.join();
                let _ = stderr_thread.join();
                return Err(format!("espeak-ng wait failed: {}", e));
            }
        }
    }
}
