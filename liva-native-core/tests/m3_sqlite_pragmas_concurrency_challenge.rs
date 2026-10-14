//! Adversarial Empirical Challenge Suite: SQLite Pragmas Deduplication & WAL Pool Concurrency
//!
//! Empirical Challenger: challenger_m3_2
//! Target: liva-native-core/src/db.rs & crates/liva-storage/src/pragmas.rs
//!
//! Objectives:
//! 1. Empirically verify that every pooled connection in `liva-native-core` has correct pragmas
//!    applied via `liva_storage::pragmas::configure_connection`:
//!    - Query `PRAGMA journal_mode`, `PRAGMA busy_timeout`, `PRAGMA cache_size`,
//!      `PRAGMA page_size`, `PRAGMA mmap_size`, `PRAGMA foreign_keys`, `PRAGMA synchronous`
//!      across both writer and reader pools.
//! 2. Concurrently check out connections across multiple threads and verify invariant pragma stability.
//! 3. Stress test concurrent SQLite readers and writer pool operations under heavy multi-threaded load.

use liva_native_core::db::{DatabasePool, get_reader_pool_size, is_transient_sqlite_lock_error};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// RAII Temporary Database Guard to ensure clean teardown of disk-backed SQLite files.
struct TempDbGuard(PathBuf);
impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

static DB_COUNTER: AtomicU64 = AtomicU64::new(5000);

fn create_file_pool() -> (Arc<DatabasePool>, TempDbGuard) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand_id = format!("{}_{}_{}", std::process::id(), nanos, counter);
    let db_path = std::env::temp_dir().join(format!("liva_m3_pragma_challenge_{rand_id}.sqlite"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    let pool = DatabasePool::new(&db_path).expect("failed to create file-backed DatabasePool");
    (Arc::new(pool), TempDbGuard(db_path))
}

#[derive(Debug, PartialEq, Eq)]
struct PragmaSnapshot {
    journal_mode: String,
    busy_timeout: i64,
    cache_size: i64,
    page_size: i64,
    mmap_size: i64,
    foreign_keys: i64,
    synchronous: i64,
}

fn query_connection_pragmas(conn: &Connection) -> Result<PragmaSnapshot, rusqlite::Error> {
    let journal_mode: String = conn
        .query_row("PRAGMA journal_mode;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA journal_mode failed: {e}");
            e
        })?;
    let busy_timeout: i64 = conn
        .query_row("PRAGMA busy_timeout;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA busy_timeout failed: {e}");
            e
        })?;
    let cache_size: i64 = conn
        .query_row("PRAGMA cache_size;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA cache_size failed: {e}");
            e
        })?;
    let page_size: i64 = conn
        .query_row("PRAGMA page_size;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA page_size failed: {e}");
            e
        })?;
    let mmap_size: i64 = conn
        .query_row("PRAGMA mmap_size;", [], |row| row.get(0))
        .unwrap_or(0); // In-memory DB might return no row for mmap_size
    let foreign_keys: i64 = conn
        .query_row("PRAGMA foreign_keys;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA foreign_keys failed: {e}");
            e
        })?;
    let synchronous: i64 = conn
        .query_row("PRAGMA synchronous;", [], |row| row.get(0))
        .map_err(|e| {
            eprintln!("PRAGMA synchronous failed: {e}");
            e
        })?;

    Ok(PragmaSnapshot {
        journal_mode: journal_mode.to_lowercase(),
        busy_timeout,
        cache_size,
        page_size,
        mmap_size,
        foreign_keys,
        synchronous,
    })
}

// =========================================================================
// TEST 1: EMPIRICAL VERIFICATION OF PRAGMAS ACROSS WRITER & READER POOLS
// =========================================================================

#[test]
fn test_m3_empirical_sqlite_pragmas_on_disk_pool() {
    let (pool, _guard) = create_file_pool();

    // 1. Verify Writer Pool Connections (max_size = 2)
    let conn_w1 = pool.writer.get().expect("checkout writer 1");
    let pragmas_w1 = query_connection_pragmas(&conn_w1).expect("query writer 1 pragmas");

    assert_eq!(
        pragmas_w1.journal_mode, "wal",
        "Writer journal_mode must be WAL"
    );
    assert_eq!(
        pragmas_w1.busy_timeout, 5000,
        "Writer busy_timeout must be 5000ms"
    );
    assert_eq!(
        pragmas_w1.cache_size, -2000,
        "Writer cache_size must be -2000 KiB (~2MB)"
    );
    assert_eq!(
        pragmas_w1.page_size, 4096,
        "Writer page_size must be 4096 bytes"
    );
    assert_eq!(
        pragmas_w1.mmap_size, 268435456,
        "Writer mmap_size must be 268435456 (256MB)"
    );
    assert_eq!(
        pragmas_w1.foreign_keys, 1,
        "Writer foreign_keys must be ON (1)"
    );
    assert_eq!(
        pragmas_w1.synchronous, 1,
        "Writer synchronous must be NORMAL (1)"
    );

    let conn_w2 = pool.writer.get().expect("checkout writer 2");
    let pragmas_w2 = query_connection_pragmas(&conn_w2).expect("query writer 2 pragmas");
    assert_eq!(
        pragmas_w1, pragmas_w2,
        "All pooled writer connections must have identical pragmas"
    );

    drop(conn_w1);
    drop(conn_w2);

    // 2. Verify Reader Pool Connections (size = get_reader_pool_size())
    let reader_size = get_reader_pool_size() as usize;
    let mut reader_conns = Vec::with_capacity(reader_size);
    for i in 0..reader_size {
        let conn = pool
            .readers
            .get()
            .unwrap_or_else(|e| panic!("checkout reader {i}: {e}"));
        let pragmas = query_connection_pragmas(&conn)
            .unwrap_or_else(|e| panic!("query reader {i} pragmas: {e}"));

        assert_eq!(
            pragmas.journal_mode, "wal",
            "Reader {i} journal_mode must be WAL"
        );
        assert_eq!(
            pragmas.busy_timeout, 5000,
            "Reader {i} busy_timeout must be 5000ms"
        );
        assert_eq!(
            pragmas.cache_size, -2000,
            "Reader {i} cache_size must be -2000 KiB"
        );
        assert_eq!(
            pragmas.page_size, 4096,
            "Reader {i} page_size must be 4096 bytes"
        );
        assert_eq!(
            pragmas.mmap_size, 268435456,
            "Reader {i} mmap_size must be 268435456"
        );
        assert_eq!(
            pragmas.foreign_keys, 1,
            "Reader {i} foreign_keys must be ON (1)"
        );
        assert_eq!(
            pragmas.synchronous, 1,
            "Reader {i} synchronous must be NORMAL (1)"
        );

        reader_conns.push(conn);
    }

    assert_eq!(reader_conns.len(), reader_size);
    drop(reader_conns);
}

#[test]
fn test_m3_empirical_sqlite_pragmas_on_memory_pool() {
    let pool = DatabasePool::new_in_memory().expect("create in-memory db pool");

    // Writer pragmas
    let conn_w = pool.writer.get().expect("checkout in-memory writer");
    let pragmas_w = query_connection_pragmas(&conn_w).expect("query writer pragmas");

    // Note: In-memory database returns "memory" for journal_mode in SQLite
    assert_eq!(
        pragmas_w.journal_mode, "memory",
        "In-memory db journal_mode is memory"
    );
    assert_eq!(
        pragmas_w.busy_timeout, 5000,
        "In-memory busy_timeout must be 5000ms"
    );
    assert_eq!(
        pragmas_w.cache_size, -2000,
        "In-memory cache_size must be -2000"
    );
    assert_eq!(
        pragmas_w.page_size, 4096,
        "In-memory page_size must be 4096"
    );
    assert_eq!(
        pragmas_w.foreign_keys, 1,
        "In-memory foreign_keys must be 1"
    );
    assert_eq!(pragmas_w.synchronous, 1, "In-memory synchronous must be 1");

    // Reader pragmas
    let conn_r = pool.readers.get().expect("checkout in-memory reader");
    let pragmas_r = query_connection_pragmas(&conn_r).expect("query reader pragmas");

    assert_eq!(pragmas_r.busy_timeout, 5000);
    assert_eq!(pragmas_r.cache_size, -2000);
    assert_eq!(pragmas_r.page_size, 4096);
    assert_eq!(pragmas_r.foreign_keys, 1);
    assert_eq!(pragmas_r.synchronous, 1);
}

// =========================================================================
// TEST 2: CONCURRENT PRAGMA INVARIANT STABILITY ACROSS POOL CHECKOUTS
// =========================================================================

#[test]
fn test_m3_concurrent_pragma_stability_across_threads() {
    let (pool, _guard) = create_file_pool();

    let num_threads = 6;
    let iterations_per_thread = 20;
    let mut handles = Vec::new();

    for thread_idx in 0..num_threads {
        let pool_clone = pool.clone();
        handles.push(std::thread::spawn(move || {
            for iter in 0..iterations_per_thread {
                if thread_idx % 2 == 0 {
                    // Test reader checkout
                    let conn = pool_clone.readers.get().expect("reader checkout in thread");
                    let pragmas = query_connection_pragmas(&conn).expect("query reader pragmas");
                    assert_eq!(
                        pragmas.journal_mode, "wal",
                        "Reader iter {iter} journal_mode must be wal"
                    );
                    assert_eq!(
                        pragmas.busy_timeout, 5000,
                        "Reader iter {iter} busy_timeout must be 5000"
                    );
                    assert_eq!(
                        pragmas.cache_size, -2000,
                        "Reader iter {iter} cache_size must be -2000"
                    );
                    assert_eq!(
                        pragmas.page_size, 4096,
                        "Reader iter {iter} page_size must be 4096"
                    );
                    assert_eq!(
                        pragmas.mmap_size, 268435456,
                        "Reader iter {iter} mmap_size must be 268435456"
                    );
                } else {
                    // Test writer checkout
                    let conn = pool_clone.writer.get().expect("writer checkout in thread");
                    let pragmas = query_connection_pragmas(&conn).expect("query writer pragmas");
                    assert_eq!(
                        pragmas.journal_mode, "wal",
                        "Writer iter {iter} journal_mode must be wal"
                    );
                    assert_eq!(
                        pragmas.busy_timeout, 5000,
                        "Writer iter {iter} busy_timeout must be 5000"
                    );
                    assert_eq!(
                        pragmas.cache_size, -2000,
                        "Writer iter {iter} cache_size must be -2000"
                    );
                    assert_eq!(
                        pragmas.page_size, 4096,
                        "Writer iter {iter} page_size must be 4096"
                    );
                    assert_eq!(
                        pragmas.mmap_size, 268435456,
                        "Writer iter {iter} mmap_size must be 268435456"
                    );
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("thread join panicked during pragma query");
    }
}

// =========================================================================
// TEST 3: HEAVY CONCURRENT READERS & WRITER POOL STRESS HARNESS
// =========================================================================

#[tokio::test]
async fn test_m3_stress_concurrent_readers_and_writers() {
    let (pool, _guard) = create_file_pool();

    // Initialize stress table
    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS stress_bench (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                worker_id INTEGER NOT NULL,
                payload TEXT NOT NULL,
                counter INTEGER NOT NULL,
                is_active INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_stress_worker ON stress_bench (worker_id, counter);",
        )
    })
    .expect("setup stress table");

    let num_writers = 4;
    let writes_per_writer = 40;
    let num_readers = 8;

    let stop_signal = Arc::new(AtomicBool::new(false));
    let total_reads = Arc::new(AtomicUsize::new(0));
    let transient_busy_count = Arc::new(AtomicUsize::new(0));
    let uncommitted_anomaly_count = Arc::new(AtomicUsize::new(0));

    // Spawn concurrent readers
    let mut reader_handles = Vec::new();
    for reader_id in 0..num_readers {
        let pool_clone = pool.clone();
        let stop_clone = stop_signal.clone();
        let total_reads_clone = total_reads.clone();
        let busy_clone = transient_busy_count.clone();
        let anomaly_clone = uncommitted_anomaly_count.clone();

        reader_handles.push(tokio::spawn(async move {
            while !stop_clone.load(Ordering::Relaxed) {
                let res = pool_clone.with_reader(|conn| {
                    // Check for anomalous inactive states (should always be committed with is_active = 1)
                    let invalid_count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM stress_bench WHERE is_active = 0;",
                        [],
                        |row| row.get(0),
                    )?;
                    let total_count: i64 =
                        conn.query_row("SELECT COUNT(*) FROM stress_bench;", [], |row| row.get(0))?;
                    Ok((invalid_count, total_count))
                });

                match res {
                    Ok((invalid, _total)) => {
                        if invalid > 0 {
                            anomaly_clone.fetch_add(invalid as usize, Ordering::SeqCst);
                        }
                        total_reads_clone.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        if is_transient_sqlite_lock_error(&e) {
                            busy_clone.fetch_add(1, Ordering::SeqCst);
                        } else {
                            panic!("Reader {reader_id} encountered non-lock error: {e}");
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    // Spawn concurrent writers
    let mut writer_handles = Vec::new();
    for writer_id in 0..num_writers {
        let pool_clone = pool.clone();
        let busy_clone = transient_busy_count.clone();

        writer_handles.push(tokio::spawn(async move {
            for seq in 0..writes_per_writer {
                let payload = format!("payload_from_writer_{writer_id}_seq_{seq}");
                let res = pool_clone.spawn_writer(move |conn| {
                    let tx = conn.unchecked_transaction()?;
                    // Write uncommitted status
                    tx.execute(
                        "INSERT INTO stress_bench (worker_id, payload, counter, is_active) VALUES (?1, ?2, ?3, 0);",
                        rusqlite::params![writer_id, payload, seq],
                    )?;
                    // Finalize to committed status within same transaction
                    tx.execute(
                        "UPDATE stress_bench SET is_active = 1 WHERE worker_id = ?1 AND counter = ?2;",
                        rusqlite::params![writer_id, seq],
                    )?;
                    tx.commit()?;
                    Ok(())
                }).await;

                if let Err(e) = res {
                    if e.contains("busy") || e.contains("locked") {
                        busy_clone.fetch_add(1, Ordering::SeqCst);
                    } else {
                        panic!("Writer {writer_id} failed with error: {e}");
                    }
                }

                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }));
    }

    // Wait for all writers to complete
    for wh in writer_handles {
        wh.await.expect("writer task panicked");
    }

    // Stop readers
    stop_signal.store(true, Ordering::SeqCst);
    for rh in reader_handles {
        rh.await.expect("reader task panicked");
    }

    // Verify isolation: zero dirty rows seen
    assert_eq!(
        uncommitted_anomaly_count.load(Ordering::SeqCst),
        0,
        "ACID isolation anomaly: dirty uncommitted row observed by reader during stress test!"
    );

    // Verify final row count
    let final_count: i64 = pool
        .with_reader(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM stress_bench WHERE is_active = 1;",
                [],
                |row| row.get(0),
            )
        })
        .expect("query final stress rows");

    assert_eq!(
        final_count as usize,
        num_writers * writes_per_writer,
        "All expected writer rows must be committed"
    );

    // Verify readers executed meaningful volume
    let reads_executed = total_reads.load(Ordering::Relaxed);
    assert!(
        reads_executed >= 50,
        "Expected >= 50 reads during concurrent writer stress, got {reads_executed}"
    );

    // Verify transient busy errors are zero or safely absorbed by 5000ms timeout
    let busy_errors = transient_busy_count.load(Ordering::SeqCst);
    assert_eq!(
        busy_errors, 0,
        "SQLite busy/lock error bubbled up during WAL stress test (timeout did not absorb): {busy_errors}"
    );
}
