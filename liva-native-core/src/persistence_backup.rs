//! Transactionally safe SQLite backup and offline restore.

use crate::db::DatabasePool;
use rusqlite::{Connection, DatabaseName, OpenFlags};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::error::Error;
use std::fmt::{Display, Formatter};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const MANIFEST_VERSION: u32 = 2;

#[derive(Debug)]
pub struct BackupRestoreError(String);

impl BackupRestoreError {
    fn new(message: impl Into<String>) -> Self {
        Self(message.into())
    }
}

impl Display for BackupRestoreError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Error for BackupRestoreError {}

impl From<std::io::Error> for BackupRestoreError {
    fn from(error: std::io::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<rusqlite::Error> for BackupRestoreError {
    fn from(error: rusqlite::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<serde_json::Error> for BackupRestoreError {
    fn from(error: serde_json::Error) -> Self {
        Self(error.to_string())
    }
}

impl From<r2d2::Error> for BackupRestoreError {
    fn from(error: r2d2::Error) -> Self {
        Self(error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackupMetadata {
    pub manifest_version: u32,
    pub created_unix_ms: u128,
    pub bytes: u64,
    pub sha256: String,
    pub schema_version: i64,
    /// Fingerprint không bí mật của khóa mã hóa cần để đọc dữ liệu trong backup.
    pub key_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RestoreOutcome {
    pub restored_bytes: u64,
    pub rollback_path: Option<PathBuf>,
}

struct RemoveOnDrop {
    path: PathBuf,
    armed: bool,
}

impl RemoveOnDrop {
    fn new(path: PathBuf) -> Self {
        Self { path, armed: true }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

pub fn backup_manifest_path(backup_path: &Path) -> PathBuf {
    let mut name = backup_path.as_os_str().to_os_string();
    name.push(".manifest.json");
    PathBuf::from(name)
}

fn temporary_sibling(path: &Path, label: &str) -> Result<PathBuf, BackupRestoreError> {
    let parent = path
        .parent()
        .ok_or_else(|| BackupRestoreError::new("path has no parent directory"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| BackupRestoreError::new("path has no UTF-8 file name"))?;
    Ok(parent.join(format!(".{file_name}.{label}.{}.tmp", uuid::Uuid::new_v4())))
}

fn sha256_file(path: &Path) -> Result<(u64, String), BackupRestoreError> {
    let mut file = File::open(path)?;
    let mut digest = Sha256::new();
    let mut bytes = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
        bytes += count as u64;
    }
    Ok((bytes, hex::encode(digest.finalize())))
}

fn validate_sqlite(path: &Path) -> Result<i64, BackupRestoreError> {
    // SQLite's FTS5 quick_check invokes its internal `integrity-check` command,
    // which requires a writable handle even though it does not persist changes.
    let connection = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_WRITE)?;
    let quick_check: String = connection.query_row("PRAGMA quick_check", [], |row| row.get(0))?;
    if quick_check != "ok" {
        return Err(BackupRestoreError::new(format!(
            "SQLite quick_check failed: {quick_check}"
        )));
    }

    let mut foreign_key_check = connection.prepare("PRAGMA foreign_key_check")?;
    let mut violations = foreign_key_check.query([])?;
    if let Some(row) = violations.next()? {
        let table: String = row.get(0)?;
        return Err(BackupRestoreError::new(format!(
            "SQLite foreign_key_check failed in table {table}"
        )));
    }
    drop(violations);
    drop(foreign_key_check);

    connection
        .query_row("PRAGMA user_version", [], |row| row.get(0))
        .map_err(Into::into)
}

fn sync_file(path: &Path) -> Result<(), BackupRestoreError> {
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(path)?
        .sync_all()?;
    Ok(())
}

pub fn backup_database(
    pool: &DatabasePool,
    destination: &Path,
    key_id: &str,
) -> Result<BackupMetadata, BackupRestoreError> {
    if key_id.len() != 32 || !key_id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(BackupRestoreError::new(
            "invalid encryption key id; expected 16-byte hex fingerprint",
        ));
    }
    let manifest_path = backup_manifest_path(destination);
    if destination.exists() || manifest_path.exists() {
        return Err(BackupRestoreError::new(
            "backup destination or manifest already exists",
        ));
    }
    let parent = destination
        .parent()
        .ok_or_else(|| BackupRestoreError::new("backup path has no parent"))?;
    std::fs::create_dir_all(parent)?;

    let temporary_database = temporary_sibling(destination, "backup")?;
    let temporary_manifest = temporary_sibling(&manifest_path, "manifest")?;
    let mut database_guard = RemoveOnDrop::new(temporary_database.clone());
    let mut manifest_guard = RemoveOnDrop::new(temporary_manifest.clone());

    {
        let source = pool.writer.get()?;
        source
            .backup(DatabaseName::Main, &temporary_database, None)
            .map_err(|error| BackupRestoreError::new(format!("online backup failed: {error}")))?;
    }
    let schema_version = validate_sqlite(&temporary_database)
        .map_err(|error| BackupRestoreError::new(format!("backup validation failed: {error}")))?;
    sync_file(&temporary_database)
        .map_err(|error| BackupRestoreError::new(format!("backup fsync failed: {error}")))?;
    let (bytes, sha256) = sha256_file(&temporary_database)
        .map_err(|error| BackupRestoreError::new(format!("backup hash failed: {error}")))?;
    let metadata = BackupMetadata {
        manifest_version: MANIFEST_VERSION,
        created_unix_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| BackupRestoreError::new(error.to_string()))?
            .as_millis(),
        bytes,
        sha256,
        schema_version,
        key_id: key_id.to_ascii_lowercase(),
    };

    let manifest_json = serde_json::to_vec_pretty(&metadata)?;
    {
        let mut manifest = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary_manifest)?;
        manifest.write_all(&manifest_json)?;
        manifest.sync_all()?;
    }

    std::fs::rename(&temporary_database, destination).map_err(|error| {
        BackupRestoreError::new(format!("backup atomic rename failed: {error}"))
    })?;
    database_guard.disarm();
    if let Err(error) = std::fs::rename(&temporary_manifest, &manifest_path) {
        let _ = std::fs::remove_file(destination);
        return Err(error.into());
    }
    manifest_guard.disarm();
    Ok(metadata)
}

fn sidecar_path(database: &Path, suffix: &str) -> PathBuf {
    let mut path = database.as_os_str().to_os_string();
    path.push(suffix);
    PathBuf::from(path)
}

pub fn restore_database(
    backup: &Path,
    target: &Path,
    expected_key_id: &str,
) -> Result<RestoreOutcome, BackupRestoreError> {
    let manifest: BackupMetadata =
        serde_json::from_slice(&std::fs::read(backup_manifest_path(backup))?)?;
    if manifest.manifest_version != MANIFEST_VERSION {
        return Err(BackupRestoreError::new(
            "unsupported backup manifest version",
        ));
    }
    if !manifest.key_id.eq_ignore_ascii_case(expected_key_id) {
        return Err(BackupRestoreError::new(
            "encryption key does not match backup manifest",
        ));
    }
    let (bytes, digest) = sha256_file(backup)?;
    if bytes != manifest.bytes || digest != manifest.sha256 {
        return Err(BackupRestoreError::new(
            "backup size or SHA-256 does not match manifest",
        ));
    }
    let schema_version = validate_sqlite(backup)?;
    if schema_version != manifest.schema_version {
        return Err(BackupRestoreError::new(
            "backup schema version does not match manifest",
        ));
    }

    let parent = target
        .parent()
        .ok_or_else(|| BackupRestoreError::new("restore target has no parent"))?;
    std::fs::create_dir_all(parent)?;
    let temporary_target = temporary_sibling(target, "restore")?;
    let mut temporary_guard = RemoveOnDrop::new(temporary_target.clone());
    let source = Connection::open_with_flags(backup, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.backup(DatabaseName::Main, &temporary_target, None)?;
    validate_sqlite(&temporary_target)?;
    sync_file(&temporary_target)?;

    let mut moved_sidecars: Vec<(PathBuf, PathBuf)> = Vec::new();
    let rollback_path = if target.exists() {
        let rollback = temporary_sibling(target, "pre-restore")?.with_extension("sqlite.rollback");
        std::fs::rename(target, &rollback)?;
        for suffix in ["-wal", "-shm"] {
            let source_sidecar = sidecar_path(target, suffix);
            if source_sidecar.exists() {
                let rollback_sidecar = sidecar_path(&rollback, suffix);
                if let Err(error) = std::fs::rename(&source_sidecar, &rollback_sidecar) {
                    for (original, moved) in moved_sidecars.iter().rev() {
                        let _ = std::fs::rename(moved, original);
                    }
                    let _ = std::fs::rename(&rollback, target);
                    return Err(BackupRestoreError::new(format!(
                        "failed to preserve SQLite sidecar {}: {error}",
                        source_sidecar.display()
                    )));
                }
                moved_sidecars.push((source_sidecar, rollback_sidecar));
            }
        }
        Some(rollback)
    } else {
        None
    };

    if let Err(error) = std::fs::rename(&temporary_target, target) {
        for (original, moved) in moved_sidecars.iter().rev() {
            let _ = std::fs::rename(moved, original);
        }
        if let Some(rollback) = &rollback_path {
            let _ = std::fs::rename(rollback, target);
        }
        return Err(error.into());
    }
    temporary_guard.disarm();

    Ok(RestoreOutcome {
        restored_bytes: bytes,
        rollback_path,
    })
}
