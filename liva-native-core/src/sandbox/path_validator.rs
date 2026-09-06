//! Path Canonicalization Guard and Filesystem Jailbreak Defense
//!
//! Provides defense-in-depth filesystem access controls:
//! - Canonical root anchoring and directory containment.
//! - Rejection of directory traversal sequences (`..`, `%2e%2e`).
//! - Null-byte injection prevention (`\0`).
//! - Symlink and junction escape detection via ancestor canonicalization.
//! - Windows DOS reserved device names (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`) and NTFS Alternate Data Streams (`:stream`).

use std::path::{Path, PathBuf};
use tracing::warn;

use crate::sandbox::policy::SandboxViolation;

/// Reserved Windows device names forbidden across all platforms to prevent OS escape.
const FORBIDDEN_DOS_DEVICES: &[&str] = &[
    "con", "prn", "aux", "nul",
    "com1", "com2", "com3", "com4", "com5", "com6", "com7", "com8", "com9",
    "lpt1", "lpt2", "lpt3", "lpt4", "lpt5", "lpt6", "lpt7", "lpt8", "lpt9",
];

/// Path canonicalization guard ensuring all file operations remain strictly inside bounds.
#[derive(Debug, Clone)]
pub struct CanonicalPathValidator {
    root: PathBuf,
}

impl CanonicalPathValidator {
    /// Creates a validator bound to an anchor directory.
    pub fn new(root: &Path) -> Result<Self, SandboxViolation> {
        let canonical_root = root.canonicalize().map_err(|e| {
            SandboxViolation::PathJailbreak(format!("Failed to canonicalize root {:?}: {e}", root))
        })?;
        Ok(Self {
            root: canonical_root,
        })
    }

    /// Returns a reference to the canonicalized anchor root.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Validates read access for a path.
    pub fn validate_read(&self, path: &Path) -> Result<PathBuf, SandboxViolation> {
        Self::sanitize_and_canonicalize(&self.root, path)
    }

    /// Validates write access for a path.
    pub fn validate_write(&self, path: &Path) -> Result<PathBuf, SandboxViolation> {
        Self::sanitize_and_canonicalize(&self.root, path)
    }

    /// Canonicalizes a path and verifies that it resides strictly within `base_root`.
    /// Rejects directory traversal `..`, null-byte injections, DOS device names, and symlink/junction escapes.
    pub fn sanitize_and_canonicalize(
        base_root: &Path,
        rel_or_abs: &Path,
    ) -> Result<PathBuf, SandboxViolation> {
        let path_str = rel_or_abs.to_string_lossy();

        // 1. Null byte detection
        if path_str.contains('\0') {
            warn!("Null byte injection detected in path: {:?}", path_str);
            return Err(SandboxViolation::PathJailbreak(
                "Null byte injection detected in path".to_string(),
            ));
        }

        // 2. Directory traversal sequences (plain and URL-encoded)
        let lower_path = path_str.to_lowercase();
        if path_str.contains("..")
            || lower_path.contains("%2e%2e")
            || lower_path.contains("%252e%252e")
        {
            warn!("Directory traversal detected in path: {:?}", path_str);
            return Err(SandboxViolation::PathJailbreak(path_str.to_string()));
        }

        // 3. Inspect individual path components for DOS devices and Alternate Data Streams
        for component in rel_or_abs.components() {
            let comp_str = component.as_os_str().to_string_lossy();
            let comp_lower = comp_str.to_lowercase();

            // DOS device check (e.g. CON, NUL, COM1.txt)
            let stem = comp_lower.split('.').next().unwrap_or(&comp_lower);
            if FORBIDDEN_DOS_DEVICES.contains(&stem) {
                warn!("Forbidden DOS device name in path: {:?}", comp_str);
                return Err(SandboxViolation::PathJailbreak(format!(
                    "Forbidden device name in path component: {comp_str}"
                )));
            }

            // NTFS Alternate Data Streams check (e.g. "file.txt:hidden.exe")
            // Note: Skip drive letter component on Windows (e.g. "C:")
            if comp_str.contains(':') {
                #[cfg(target_os = "windows")]
                {
                    if let std::path::Component::Prefix(_) = component {
                        // Valid Windows drive prefix, allowed
                        continue;
                    }
                }
                warn!("NTFS Alternate Data Stream detected in path component: {:?}", comp_str);
                return Err(SandboxViolation::PathJailbreak(format!(
                    "Alternate Data Stream forbidden: {comp_str}"
                )));
            }
        }

        // 4. Canonicalize base root
        let canonical_root = base_root.canonicalize().map_err(|e| {
            SandboxViolation::PathJailbreak(format!(
                "Failed to canonicalize sandbox root {:?}: {e}",
                base_root
            ))
        })?;

        // 5. Construct candidate target path
        let target_full = if rel_or_abs.is_absolute() {
            rel_or_abs.to_path_buf()
        } else {
            base_root.join(rel_or_abs)
        };

        // 6. Walk ancestors to resolve canonical directory and verify symlink containment
        let mut ancestor = target_full.as_path();
        let canonical_ancestor = loop {
            if let Ok(c) = ancestor.canonicalize() {
                break c;
            }
            match ancestor.parent() {
                Some(p) if !p.as_os_str().is_empty() => ancestor = p,
                _ => {
                    return Err(SandboxViolation::PathJailbreak(
                        "Cannot resolve directory tree for path".to_string(),
                    ));
                }
            }
        };

        if !canonical_ancestor.starts_with(&canonical_root) {
            warn!(
                "Security violation: path {:?} (ancestor {:?}) escapes sandbox boundary {:?}",
                target_full, canonical_ancestor, canonical_root
            );
            return Err(SandboxViolation::PathJailbreak(
                "Target path escapes sandbox boundary via symlink or parent directory".to_string(),
            ));
        }

        Ok(target_full)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_nested_paths() {
        let temp_dir = std::env::temp_dir().join("liva_test_pv_nested");
        let sub = temp_dir.join("workspace").join("src");
        let _ = std::fs::create_dir_all(&sub);

        let validator = CanonicalPathValidator::new(&temp_dir).unwrap();
        assert!(validator.validate_read(Path::new("workspace/src/lib.rs")).is_ok());
        assert!(validator.validate_write(Path::new("workspace/src/new.txt")).is_ok());
    }

    #[test]
    fn test_dotdot_traversal_rejection() {
        let temp_dir = std::env::temp_dir().join("liva_test_pv_dotdot");
        let _ = std::fs::create_dir_all(&temp_dir);

        let validator = CanonicalPathValidator::new(&temp_dir).unwrap();
        assert!(validator.validate_read(Path::new("../secret.txt")).is_err());
        assert!(validator.validate_read(Path::new("sub/../../escape.txt")).is_err());
        assert!(validator.validate_read(Path::new("%2e%2e/escape.txt")).is_err());
    }

    #[test]
    fn test_null_byte_rejection() {
        let temp_dir = std::env::temp_dir().join("liva_test_pv_null");
        let _ = std::fs::create_dir_all(&temp_dir);

        let validator = CanonicalPathValidator::new(&temp_dir).unwrap();
        assert!(validator.validate_read(Path::new("file.txt\0.png")).is_err());
    }

    #[test]
    fn test_dos_device_rejection() {
        let temp_dir = std::env::temp_dir().join("liva_test_pv_dos");
        let _ = std::fs::create_dir_all(&temp_dir);

        let validator = CanonicalPathValidator::new(&temp_dir).unwrap();
        assert!(validator.validate_read(Path::new("con")).is_err());
        assert!(validator.validate_read(Path::new("NUL.txt")).is_err());
        assert!(validator.validate_read(Path::new("com1")).is_err());
    }
}
