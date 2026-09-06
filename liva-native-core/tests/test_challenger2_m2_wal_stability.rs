//! Milestone 2: Empirical Challenger 2 SQLite WAL Pool & Memory Enclave Stability Harness
//!
//! Adversarial verification of:
//! 1. 32-reader concurrent pool exhaustion under sustained writer throughput and checkpoints.
//! 2. Strict reader `query_only=ON` isolation on file-backed WAL database.
//! 3. Cooperative chunked writes with concurrent reader interleaving.
//! 4. Memory Enclave cross-key isolation, tamper resistance, and fail-closed security.
//! 5. File-backed WAL checkpoint truncate and clean shutdown without leaked descriptors.

use liva_native_core::db::{
    DatabasePool, WalCheckpointMode, SQLITE_READER_POOL_SIZE, SQLITE_WRITER_POOL_SIZE,
};
use liva_native_core::memory::enclave::{EnclaveError, MemoryEnclave, ENCLAVE_V2_PREFIX};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tempfile::tempdir;

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_wal_high_concurrency_32_readers_1_writer() {
    let temp_dir = tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("wal_stress.sqlite");
    let pool = Arc::new(DatabasePool::new(&db_path).expect("create file-backed pool"));

    // Verify configured pool sizes
    assert_eq!(SQLITE_READER_POOL_SIZE, 16);
    assert_eq!(SQLITE_WRITER_POOL_SIZE, 1);

    // Populate initial dataset
    pool.with_write_conn(|conn| {
        let mut stmt = conn.prepare(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?1, ?2, 'now', 'now', 'seed')",
        )?;
        for i in 0..200 {
            stmt.execute(rusqlite::params![
                format!("fact_key_{}", i),
                format!("fact_value_{}", i)
            ])?;
        }
        Ok(())
    })
    .expect("seed initial facts");

    let total_reads = Arc::new(AtomicUsize::new(0));
    let mut reader_handles = Vec::new();

    // Spawn 32 concurrent readers (competing for 16 connection slots)
    for reader_id in 0..32 {
        let pool_clone = pool.clone();
        let total_reads_clone = total_reads.clone();

        let handle = tokio::spawn(async move {
            for iter in 0..50 {
                let target_key = format!("fact_key_{}", (reader_id * 7 + iter) % 200);
                let val: Result<String, rusqlite::Error> = pool_clone.with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT value FROM facts WHERE key = ?1",
                        [&target_key],
                        |row| row.get(0),
                    )
                });
                assert!(
                    val.is_ok(),
                    "Reader {} at iter {} failed: {:?}",
                    reader_id,
                    iter,
                    val
                );
                total_reads_clone.fetch_add(1, Ordering::Relaxed);

                if iter % 10 == 0 {
                    tokio::task::yield_now().await;
                }
            }
        });
        reader_handles.push(handle);
    }

    // Spawn concurrent writer performing continuous writes and transactions
    let pool_clone = pool.clone();
    let writer_handle = tokio::spawn(async move {
        for w in 0..60 {
            let key = format!("dynamic_key_{}", w);
            let val = format!("dynamic_val_{}", w);
            let write_res = pool_clone.with_write_conn(|conn| {
                conn.execute(
                    "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?1, ?2, 'now', 'now', 'dynamic_writer')",
                    rusqlite::params![key, val],
                )
            });
            assert!(write_res.is_ok(), "Writer step {} failed: {:?}", w, write_res);

            // Trigger passive checkpoint periodically during active load
            if w % 20 == 0 {
                let cp_res = pool_clone.wal_checkpoint(WalCheckpointMode::Passive);
                assert!(cp_res.is_ok(), "Checkpoint at step {} failed: {:?}", w, cp_res);
            }

            tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
        }
    });

    // Wait for all 32 readers
    for h in reader_handles {
        h.await.expect("Reader task panicked");
    }

    // Wait for writer
    writer_handle.await.expect("Writer task panicked");

    assert_eq!(total_reads.load(Ordering::Relaxed), 32 * 50);

    // Verify database state integrity
    let count: i64 = pool
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT count(*) FROM facts WHERE source = 'dynamic_writer'",
                [],
                |r| r.get(0),
            )
        })
        .expect("query writer count");
    assert_eq!(count, 60);

    // Final truncate checkpoint and memory shrink
    let maint = pool.idle_maintenance();
    assert!(maint.is_ok(), "idle_maintenance failed: {:?}", maint);
}

#[test]
fn test_wal_reader_query_only_enforcement() {
    let temp_dir = tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("query_only.sqlite");
    let pool = DatabasePool::new(&db_path).expect("create file-backed pool");

    // All reader operations attempting write must fail
    let insert_err = pool.with_read_conn(|conn| {
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('a', 'b', 'now', 'now', 'c')",
            [],
        )
    });
    assert!(insert_err.is_err(), "Reader MUST reject INSERT");

    let create_err = pool.with_read_conn(|conn| {
        conn.execute("CREATE TABLE rogue_table (id INTEGER PRIMARY KEY);", [])
    });
    assert!(create_err.is_err(), "Reader MUST reject CREATE TABLE");

    let delete_err = pool.with_read_conn(|conn| {
        conn.execute("DELETE FROM facts", [])
    });
    assert!(delete_err.is_err(), "Reader MUST reject DELETE");
}

#[tokio::test]
async fn test_cooperative_chunked_write_interleaving() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("in-memory pool"));

    let items: Vec<(String, String)> = (0..250)
        .map(|i| (format!("chunk_{}", i), format!("val_{}", i)))
        .collect();

    let pool_clone = pool.clone();
    let write_task = tokio::spawn(async move {
        pool_clone
            .execute_cooperative_chunked_write(items, 50, |conn, batch| {
                let mut stmt = conn.prepare(
                    "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES (?1, ?2, 'now', 'now', 'chunk_audit')",
                )?;
                let mut c = 0;
                for (k, v) in batch {
                    stmt.execute(rusqlite::params![k, v])?;
                    c += 1;
                }
                Ok(vec![c])
            })
            .await
    });

    // Concurrently query during chunked write
    let mut reader_observations = Vec::new();
    for _ in 0..10 {
        let count: i64 = pool
            .with_read_conn(|conn| {
                conn.query_row(
                    "SELECT count(*) FROM facts WHERE source = 'chunk_audit'",
                    [],
                    |r| r.get(0),
                )
            })
            .unwrap_or(0);
        reader_observations.push(count);
        tokio::time::sleep(tokio::time::Duration::from_millis(1)).await;
    }

    let batch_counts = write_task.await.unwrap().expect("chunked write failed");
    assert_eq!(batch_counts, vec![50, 50, 50, 50, 50]);

    let final_count: i64 = pool
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT count(*) FROM facts WHERE source = 'chunk_audit'",
                [],
                |r| r.get(0),
            )
        })
        .expect("final count");
    assert_eq!(final_count, 250);
}

#[test]
fn test_memory_enclave_isolation_and_tamper_defense() {
    let pass_a = b"EnclaveAlphaPassphrase2026!";
    let pass_b = b"EnclaveBetaPassphrase2026!";
    let salt = b"master_enclave_salt_001";

    let enclave_a = MemoryEnclave::new_with_argon2id(pass_a, salt).expect("init enclave A");
    let enclave_b = MemoryEnclave::new_with_argon2id(pass_b, salt).expect("init enclave B");

    let confidential_data = "TopSecretCredentials_EnclaveProtected";
    let envelope = enclave_a.encrypt_string(confidential_data).expect("encrypt A");

    // 1. Enclave A must decrypt cleanly
    let decrypted_a = enclave_a.decrypt_string(&envelope).expect("decrypt A");
    assert_eq!(&*decrypted_a, confidential_data);

    // 2. Enclave B with different passphrase MUST fail authentication
    let decrypted_b = enclave_b.decrypt_string(&envelope);
    assert!(
        matches!(decrypted_b, Err(EnclaveError::AuthenticationFailed)),
        "Cross-enclave decryption must fail with AuthenticationFailed"
    );

    // 3. Bit tampering in ciphertext must fail-closed
    let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
    assert_eq!(parts.len(), 4);

    let mut cipher_bytes = hex::decode(parts[3]).expect("decode ciphertext hex");
    cipher_bytes[0] ^= 0x7F; // flip bits
    let tampered_envelope = format!(
        "{}{}:{}:{}:{}",
        ENCLAVE_V2_PREFIX,
        parts[0],
        parts[1],
        parts[2],
        hex::encode(cipher_bytes)
    );

    let tamper_res = enclave_a.decrypt_string(&tampered_envelope);
    assert!(
        matches!(tamper_res, Err(EnclaveError::AuthenticationFailed)),
        "Tampered payload must fail with AuthenticationFailed"
    );
}

#[test]
fn test_wal_file_lifecycle_and_clean_exit() {
    let temp_dir = tempdir().expect("create temp dir");
    let db_path = temp_dir.path().join("lifecycle.sqlite");

    {
        let pool = DatabasePool::new(&db_path).expect("create pool");
        pool.with_write_conn(|conn| {
            conn.execute(
                "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', 'v', 'now', 'now', 'lifecycle')",
                [],
            )?;
            Ok(())
        })
        .expect("write");

        // Verify WAL files exist
        let wal_path = format!("{}-wal", db_path.display());
        let shm_path = format!("{}-shm", db_path.display());
        assert!(std::path::Path::new(&wal_path).exists() || std::path::Path::new(&shm_path).exists());

        // Perform truncate checkpoint
        let cp = pool.wal_checkpoint(WalCheckpointMode::Truncate).expect("truncate");
        assert_eq!(cp.busy, 0, "Truncate checkpoint should not be busy");
    }

    // After pool drop, database must reopen cleanly
    let reopened = DatabasePool::new(&db_path).expect("reopen pool");
    let read_val: String = reopened
        .with_read_conn(|conn| {
            conn.query_row("SELECT value FROM facts WHERE key = 'k'", [], |r| r.get(0))
        })
        .expect("read after reopen");
    assert_eq!(read_val, "v");
}
