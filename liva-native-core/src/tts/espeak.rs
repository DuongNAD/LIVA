//! Shared espeak-ng process resolution and IPA phonemization.
//!
//! espeak-ng may not be on PATH (a fresh winget install only updates PATH for
//! new shells), so resolution order is: `LIVA_ESPEAK_PATH` env → PATH → the
//! default Windows install locations. The resolved path is cached per process.

use std::path::PathBuf;
use std::process::Command;
use std::sync::OnceLock;

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
pub(crate) fn espeak_ipa(voice: &str, text: &str) -> Result<String, String> {
    let output = espeak_command()
        .args(["-q", "--ipa", "-v", voice, "--", text])
        .output()
        .map_err(|e| format!("espeak-ng spawn failed: {}", e))?;

    if !output.status.success() {
        return Err(format!(
            "espeak-ng exited with {}: {}",
            output.status,
            String::from_utf8_lossy(&output.stderr)
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}
