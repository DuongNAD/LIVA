//! Model Trust & Artifact Verification Fast Caching.
//!
//! Stores computed SHA-256 digests indexed by canonical file path, file size,
//! and last modified timestamp (mtime). Eliminates multi-gigabyte disk re-hashing
//! (3–6 seconds freeze) when waking models from idle, reloading GPU layers, or
//! verifying models across restarts.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct TrustCacheEntry {
    pub file_size: u64,
    pub mtime_nanos: i64,
    pub sha256: String,
    pub verified_at_unix_ts: i64,
}

static MEMORY_CACHE: OnceLock<RwLock<HashMap<PathBuf, TrustCacheEntry>>> = OnceLock::new();

fn get_cache() -> &'static RwLock<HashMap<PathBuf, TrustCacheEntry>> {
    MEMORY_CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Extract metadata (file_size, mtime_nanos) for a file.
pub fn file_metadata(path: &Path) -> Result<(u64, i64), String> {
    let metadata = fs::metadata(path).map_err(|error| {
        format!(
            "không đọc được metadata artifact {}: {error}",
            path.display()
        )
    })?;
    let file_size = metadata.len();
    let mtime_nanos = metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_nanos() as i64)
        .unwrap_or(0);
    Ok((file_size, mtime_nanos))
}

/// Lookup in-memory cache for an artifact matching exact path, file size, and mtime.
pub fn lookup_memory_cache(
    canonical_path: &Path,
    file_size: u64,
    mtime_nanos: i64,
) -> Option<String> {
    let cache = get_cache().read().ok()?;
    let entry = cache.get(canonical_path)?;
    if entry.file_size == file_size && entry.mtime_nanos == mtime_nanos {
        Some(entry.sha256.clone())
    } else {
        None
    }
}

/// Insert or update an artifact entry in the in-memory cache.
pub fn insert_memory_cache(
    canonical_path: PathBuf,
    file_size: u64,
    mtime_nanos: i64,
    sha256: String,
) {
    if let Ok(mut cache) = get_cache().write() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;
        cache.insert(
            canonical_path,
            TrustCacheEntry {
                file_size,
                mtime_nanos,
                sha256,
                verified_at_unix_ts: now,
            },
        );
    }
}

/// Clear in-memory trust cache (primarily for tests).
pub fn clear_memory_cache() {
    if let Ok(mut cache) = get_cache().write() {
        cache.clear();
    }
}

/// Count entries in in-memory trust cache.
pub fn memory_cache_count() -> usize {
    get_cache().read().map(|c| c.len()).unwrap_or(0)
}

/// Fast artifact trust verification with multi-tier caching (in-memory + SQLite fallback).
///
/// If an artifact's file size and modification time match a previously computed SHA-256
/// digest, the cached hash is returned immediately in microseconds (< 0.1ms) instead of
/// re-reading gigabytes of model data from disk.
pub fn check_or_verify_artifact<F>(
    canonical_file: &Path,
    expected_sha256: &str,
    compute_hash: F,
) -> Result<String, String>
where
    F: FnOnce(&Path) -> Result<String, String>,
{
    let (file_size, mtime_nanos) = file_metadata(canonical_file)?;

    // 1. Tier 1: Fast in-memory cache check (< 100ns)
    if let Some(cached_sha) = lookup_memory_cache(canonical_file, file_size, mtime_nanos) {
        if cached_sha.eq_ignore_ascii_case(expected_sha256) {
            tracing::debug!(
                artifact = %canonical_file.display(),
                "Model trust cache hit (memory): skipped re-hashing"
            );
            return Ok(cached_sha);
        } else {
            return Err(format!(
                "SHA-256 artifact không khớp: mong đợi {expected_sha256}, nhận {cached_sha}"
            ));
        }
    }

    // 2. Tier 2: SQLite database cache check across restarts
    if let Some(db_path) = find_existing_db_path() {
        if let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            let path_str = canonical_file.to_string_lossy();
            if let Ok(Some(cached_sha)) =
                crate::db::get_artifact_trust_cache(&conn, &path_str, file_size, mtime_nanos)
            {
                if cached_sha.eq_ignore_ascii_case(expected_sha256) {
                    tracing::debug!(
                        artifact = %canonical_file.display(),
                        "Model trust cache hit (SQLite): skipped re-hashing"
                    );
                    insert_memory_cache(
                        canonical_file.to_path_buf(),
                        file_size,
                        mtime_nanos,
                        cached_sha.clone(),
                    );
                    return Ok(cached_sha);
                } else {
                    return Err(format!(
                        "SHA-256 artifact không khớp: mong đợi {expected_sha256}, nhận {cached_sha}"
                    ));
                }
            }
        }
    }

    // 3. Cache miss: Compute actual SHA-256 digest
    tracing::info!(
        artifact = %canonical_file.display(),
        size_bytes = file_size,
        "Computing artifact SHA-256 digest (cold cache)..."
    );
    let actual_sha = compute_hash(canonical_file)?;

    if !actual_sha.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "SHA-256 artifact không khớp: mong đợi {expected_sha256}, nhận {actual_sha}"
        ));
    }

    // Populate in-memory cache
    insert_memory_cache(
        canonical_file.to_path_buf(),
        file_size,
        mtime_nanos,
        actual_sha.clone(),
    );

    // Populate SQLite database cache if DB is reachable
    if let Some(db_path) = find_existing_db_path() {
        if let Ok(conn) = rusqlite::Connection::open_with_flags(
            &db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            let path_str = canonical_file.to_string_lossy();
            let _ = crate::db::set_artifact_trust_cache(
                &conn,
                &path_str,
                file_size,
                mtime_nanos,
                &actual_sha,
            );
        }
    }

    Ok(actual_sha)
}

fn find_existing_db_path() -> Option<PathBuf> {
    // Check candidate DB paths
    let candidate = crate::paths::data_dir().join("db.sqlite");
    if candidate.exists() {
        return Some(candidate);
    }
    let local_candidate = PathBuf::from("data/db.sqlite");
    if local_candidate.exists() {
        return Some(local_candidate);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_cache_hit_and_invalidation() {
        clear_memory_cache();
        let path = PathBuf::from("mock/model.gguf");
        let hash = "abcdef1234567890".to_string();

        insert_memory_cache(path.clone(), 1000, 500, hash.clone());
        assert_eq!(memory_cache_count(), 1);

        // Exact match
        assert_eq!(lookup_memory_cache(&path, 1000, 500), Some(hash.clone()));

        // Size changed -> invalidation
        assert_eq!(lookup_memory_cache(&path, 1001, 500), None);

        // Mtime changed -> invalidation
        assert_eq!(lookup_memory_cache(&path, 1000, 501), None);

        // Path changed -> invalidation
        assert_eq!(
            lookup_memory_cache(&PathBuf::from("other.gguf"), 1000, 500),
            None
        );
    }

    #[test]
    fn test_sqlite_artifact_trust_cache_roundtrip() {
        let conn = rusqlite::Connection::open_in_memory().expect("in-memory db");
        conn.execute(
            "CREATE TABLE IF NOT EXISTS artifact_trust_cache (
                canonical_path TEXT PRIMARY KEY,
                file_size INTEGER NOT NULL,
                mtime_nanos INTEGER NOT NULL,
                sha256 TEXT NOT NULL,
                verified_at INTEGER NOT NULL
            )",
            [],
        )
        .expect("create table");

        let path = "test/model.gguf";
        let sha = "1234567890abcdef1234567890abcdef1234567890abcdef1234567890abcdef";

        // Initial lookup is empty
        assert_eq!(
            crate::db::get_artifact_trust_cache(&conn, path, 1000, 500).unwrap(),
            None
        );

        // Store entry
        crate::db::set_artifact_trust_cache(&conn, path, 1000, 500, sha).unwrap();

        // Exact match hit
        assert_eq!(
            crate::db::get_artifact_trust_cache(&conn, path, 1000, 500).unwrap(),
            Some(sha.to_string())
        );

        // Size mismatch -> miss
        assert_eq!(
            crate::db::get_artifact_trust_cache(&conn, path, 1001, 500).unwrap(),
            None
        );

        // Mtime mismatch -> miss
        assert_eq!(
            crate::db::get_artifact_trust_cache(&conn, path, 1000, 501).unwrap(),
            None
        );
    }

    fn compute_file_hash(path: &Path) -> Result<String, String> {
        use sha2::{Digest, Sha256};
        let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        Ok(hasher
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }

    #[test]
    fn test_check_or_verify_artifact_file_lifecycle() {
        use std::io::Write;
        clear_memory_cache();

        let temp_dir = std::env::temp_dir().join(format!(
            "liva_test_trust_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&temp_dir).expect("create temp dir");
        let file_path = temp_dir.join("model.bin");
        {
            let mut file = std::fs::File::create(&file_path).expect("create file");
            file.write_all(b"model test weights data")
                .expect("write file");
        }

        let sha = compute_file_hash(&file_path).expect("compute initial hash");

        // 1. Cold cache verification
        let verified_sha = check_or_verify_artifact(&file_path, &sha, compute_file_hash)
            .expect("verify cold cache");
        assert_eq!(verified_sha, sha);
        assert!(memory_cache_count() >= 1);

        // 2. Warm cache verification (fast path, does not invoke closure)
        let warm_sha = check_or_verify_artifact(&file_path, &sha, |_| {
            panic!("Should not compute hash on warm cache hit!");
        })
        .expect("verify warm cache");
        assert_eq!(warm_sha, sha);

        // 3. Modifying file invalidates cache and detects mismatch
        {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .append(true)
                .open(&file_path)
                .expect("open file for append");
            file.write_all(b"tampered data").expect("write append");
        }

        // Cache lookup should now miss and invoke compute_file_hash, detecting mismatch with expected sha
        let result = check_or_verify_artifact(&file_path, &sha, compute_file_hash);
        assert!(result.is_err(), "Modified file must fail expected sha");

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
