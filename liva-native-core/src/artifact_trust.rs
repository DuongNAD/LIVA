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

/// Định danh nền hiện tại theo quy ước của các package npm platform-binary
/// (`sqlite-vec-darwin-arm64`, …): `{os}-{arch}`.
///
/// Dùng để chọn đúng hash của artifact runtime: một binary native (dylib/dll/so)
/// có nội dung KHÁC NHAU mỗi nền, nên một hash duy nhất trong manifest chỉ đúng
/// cho đúng một nền — đây chính là lý do vec0 không nạp được trên macOS trước
/// khi có bảng này.
pub fn runtime_artifact_platform_key() -> &'static str {
    #[cfg(target_os = "windows")]
    {
        if cfg!(target_arch = "aarch64") {
            "windows-arm64"
        } else {
            "windows-x64"
        }
    }
    #[cfg(target_os = "macos")]
    {
        if cfg!(target_arch = "aarch64") {
            "darwin-arm64"
        } else {
            "darwin-x64"
        }
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        if cfg!(target_arch = "aarch64") {
            "linux-arm64"
        } else {
            "linux-x64"
        }
    }
}

/// Hash của runtime artifact cho MỘT nền cụ thể.
///
/// Thứ tự tra cứu trong manifest:
/// 1. `runtimeArtifacts[name].platforms[platform].sha256` — chuẩn mới, mỗi nền
///    một hash;
/// 2. nếu entry không có mục `platforms` nào: rơi về `runtimeArtifacts[name].
///    sha256` (chuẩn cũ, chỉ đúng khi artifact giống nhau mọi nền);
/// 3. nếu có `platforms` mà thiếu đúng nền đang hỏi: BÁO LỖI RÕ — đừng lặng lẽ
///    dùng hash của nền khác rồi để `verify_trusted_file` từ chối với thông báo
///    sai nguyên nhân ("SHA-256 không khớp" thay vì "thiếu hash cho nền này").
pub fn embedded_runtime_artifact_hash_for_platform(
    name: &str,
    platform: &str,
) -> Result<String, String> {
    let manifest = embedded_manifest()?;
    let entry = manifest["runtimeArtifacts"][name].clone();
    if !entry.is_object() {
        return Err(format!(
            "runtime artifact '{name}' không tồn tại trong trust manifest"
        ));
    }

    let hop_le = |hash: &str| crate::setup::la_hex_sha256(hash);

    // Mỗi nền chấp nhận HAI dạng giá trị: chuỗi hex trần ("darwin-arm64":
    // "<sha256>") hoặc object {"sha256": "<hex>"} — chọn một cho gọn khi viết.
    fn doc_hash(v: &serde_json::Value) -> Option<&str> {
        let hash = v.as_str().or_else(|| v["sha256"].as_str())?;
        if crate::setup::la_hex_sha256(hash) {
            Some(hash)
        } else {
            None
        }
    }

    if let Some(platforms) = entry["platforms"].as_object() {
        return platforms
            .get(platform)
            .and_then(doc_hash)
            .map(str::to_owned)
            .ok_or_else(|| {
                format!(
                    "runtime artifact '{name}' chưa có SHA-256 hợp lệ cho nền \
                     '{platform}' trong trust manifest — hãy băm binary của nền đó \
                     và thêm vào runtimeArtifacts.{name}.platforms.{platform}"
                )
            });
    }

    // Chuẩn cũ: không có bảng platforms — hash duy nhất dùng cho mọi nền.
    entry["sha256"]
        .as_str()
        .filter(|hash| hop_le(hash))
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
