//! Immutable artifact trust anchor and canonical path verification.

use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::{Component, Path, PathBuf};

const EMBEDDED_MANIFEST: &str = include_str!("../../data/models-manifest.json");

fn embedded_manifest() -> Result<serde_json::Value, String> {
    serde_json::from_str(EMBEDDED_MANIFEST)
        .map_err(|error| format!("models-manifest nhúng trong binary không hợp lệ: {error}"))
}

/// Return the pinned SHA-256 for any file entry in the embedded manifest.
///
/// Unlike [`embedded_model_hash`], this is not restricted to `llm: true` and
/// is therefore suitable for native runtime artifacts such as wake models.
pub fn embedded_file_hash(relative_path: &str) -> Result<String, String> {
    let manifest = embedded_manifest()?;
    manifest["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|file| file["dest"].as_str() == Some(relative_path))
        })
        .and_then(|file| file["sha256"].as_str())
        .filter(|hash| crate::setup::la_hex_sha256(hash))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("artifact '{relative_path}' không có SHA-256 hợp lệ trong trust manifest")
        })
}

pub fn embedded_model_hash(relative_path: &str) -> Result<String, String> {
    let manifest = embedded_manifest()?;
    manifest["files"]
        .as_array()
        .and_then(|files| {
            files.iter().find(|file| {
                file["llm"].as_bool().unwrap_or(false)
                    && file["dest"].as_str() == Some(relative_path)
            })
        })
        .and_then(|file| file["sha256"].as_str())
        .filter(|hash| crate::setup::la_hex_sha256(hash))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("model '{relative_path}' không có SHA-256 hợp lệ trong trust manifest")
        })
}

pub fn embedded_runtime_artifact_hash(name: &str) -> Result<String, String> {
    let manifest = embedded_manifest()?;
    manifest["runtimeArtifacts"][name]["sha256"]
        .as_str()
        .filter(|hash| crate::setup::la_hex_sha256(hash))
        .map(str::to_owned)
        .ok_or_else(|| {
            format!("runtime artifact '{name}' không có SHA-256 hợp lệ trong trust manifest")
        })
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("không mở được {}: {error}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let read = file
            .read(&mut buffer)
            .map_err(|error| format!("không băm được {}: {error}", path.display()))?;
        if read == 0 {
            break;
        }
        digest.update(&buffer[..read]);
    }
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

pub fn verify_trusted_file(
    trust_root: &Path,
    candidate: &Path,
    expected_sha256: &str,
) -> Result<PathBuf, String> {
    if !crate::setup::la_hex_sha256(expected_sha256) {
        return Err("SHA-256 mong đợi không hợp lệ".to_string());
    }
    if candidate
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err("artifact path không được chứa '..'".to_string());
    }

    let canonical_root = trust_root
        .canonicalize()
        .map_err(|error| format!("không canonicalize được trust root: {error}"))?;
    let joined = if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        canonical_root.join(candidate)
    };
    let canonical_file = joined.canonicalize().map_err(|error| {
        format!(
            "không canonicalize được artifact {}: {error}",
            joined.display()
        )
    })?;
    if !canonical_file.starts_with(&canonical_root) {
        return Err(format!(
            "artifact {} thoát khỏi trust root {}",
            canonical_file.display(),
            canonical_root.display()
        ));
    }
    if !canonical_file
        .metadata()
        .map_err(|error| format!("không đọc được metadata artifact: {error}"))?
        .is_file()
    {
        return Err(format!(
            "artifact không phải file: {}",
            canonical_file.display()
        ));
    }

    let actual = sha256_file(&canonical_file)?;
    if !actual.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "SHA-256 artifact không khớp: mong đợi {expected_sha256}, nhận {actual}"
        ));
    }
    Ok(canonical_file)
}

pub fn verify_model_artifact(models_root: &Path, candidate: &Path) -> Result<PathBuf, String> {
    if candidate
        .components()
        .any(|component| component == Component::ParentDir)
    {
        return Err("model_path không được chứa '..'".to_string());
    }
    let extension_ok = candidate
        .extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"));
    if !extension_ok {
        return Err("model_path phải là file .gguf".to_string());
    }

    let canonical_root = models_root
        .canonicalize()
        .map_err(|error| format!("không canonicalize được models root: {error}"))?;
    let canonical_file = if candidate.is_absolute() {
        candidate
            .canonicalize()
            .map_err(|error| format!("không canonicalize được model: {error}"))?
    } else {
        canonical_root
            .join(candidate)
            .canonicalize()
            .map_err(|error| format!("không canonicalize được model: {error}"))?
    };
    let relative = canonical_file
        .strip_prefix(&canonical_root)
        .map_err(|_| "model thoát khỏi canonical models root".to_string())?;
    let manifest_path = relative.to_string_lossy().replace('\\', "/");
    let hash = embedded_model_hash(&manifest_path)?;
    verify_trusted_file(&canonical_root, &canonical_file, &hash)
}
