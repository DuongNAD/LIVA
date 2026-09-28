//! Storage WAL Connection Pool Integration Test Suite (TICKET-13)
//!
//! Validates:
//! 1. High-concurrency 20 readers + 5 writers with zero SQLITE_BUSY and Read Committed isolation.
//! 2. Reader pool saturation, exhaustion, and graceful 3-second timeout without panic.
//! 3. Micro-batch transaction rollback atomicity with zero graph mutations leaked to CsrGraph.
//! 4. Active WAL checkpoint interleaving during concurrent reads and writes.

use arc_swap::ArcSwap;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::csr_graph::CsrGraph;
use liva_native_core::db::{DatabasePool, is_transient_sqlite_lock_error};
use liva_native_core::db_actor::DbWriteCommand;
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

static DB_COUNTER: AtomicU64 = AtomicU64::new(100);

fn create_file_pool() -> (Arc<DatabasePool>, TempDbGuard) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand_id = format!("{}_{}_{}", std::process::id(), nanos, counter);
    let db_path = std::env::temp_dir().join(format!("liva_wal_pool_test_{rand_id}.sqlite"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    let pool = DatabasePool::new(&db_path).expect("failed to create file-backed DatabasePool");
    (Arc::new(pool), TempDbGuard(db_path))
}

// =========================================================================
// TEST 1: 20 CONCURRENT READERS + 5 CONCURRENT WRITERS (ZERO SQLITE_BUSY)
// =========================================================================

#[tokio::test]
async fn test_wal_pool_concurrency_and_read_committed_isolation() {
    let (pool, _guard) = create_file_pool();

    // Prepare test table
    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS concurrency_probe (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                writer_id INTEGER NOT NULL,
                seq INTEGER NOT NULL,
                committed INTEGER NOT NULL
            );",
        )
    })
    .expect("setup probe table");

    let num_writers = 5;
    let num_readers = 20;
    let writes_per_writer = 25;

    let stop_signal = Arc::new(AtomicBool::new(false));
    let total_uncommitted_reads = Arc::new(AtomicUsize::new(0));
    let total_busy_errors = Arc::new(AtomicUsize::new(0));
    let successful_reads = Arc::new(AtomicUsize::new(0));

    // Spawn 20 concurrent readers
    let mut reader_handles = Vec::new();
    for reader_id in 0..num_readers {
        let pool_clone = pool.clone();
        let stop_clone = stop_signal.clone();
        let uncommitted_clone = total_uncommitted_reads.clone();
        let busy_clone = total_busy_errors.clone();
        let success_clone = successful_reads.clone();

        reader_handles.push(tokio::spawn(async move {
            while !stop_clone.load(Ordering::Relaxed) {
                let res = pool_clone.with_reader(|conn| {
                    // Check for uncommitted rows (committed == 0)
                    let uncommitted_count: i64 = conn.query_row(
                        "SELECT COUNT(*) FROM concurrency_probe WHERE committed = 0;",
                        [],
                        |row| row.get(0),
                    )?;
                    let total_rows: i64 =
                        conn.query_row("SELECT COUNT(*) FROM concurrency_probe;", [], |row| {
                            row.get(0)
                        })?;
                    Ok((uncommitted_count, total_rows))
                });

                match res {
                    Ok((uncommitted, _total)) => {
                        if uncommitted > 0 {
                            uncommitted_clone.fetch_add(uncommitted as usize, Ordering::SeqCst);
                        }
                        success_clone.fetch_add(1, Ordering::Relaxed);
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

    // Spawn 5 concurrent writers
    let mut writer_handles = Vec::new();
    for writer_id in 0..num_writers {
        let pool_clone = pool.clone();
        let busy_clone = total_busy_errors.clone();

        writer_handles.push(tokio::spawn(async move {
            for seq in 0..writes_per_writer {
                let res = pool_clone.spawn_writer(move |conn| {
                    // Use an unchecked transaction that writes uncommitted status then finalizes committed = 1
                    let tx = conn.unchecked_transaction()?;
                    tx.execute(
                        "INSERT INTO concurrency_probe (writer_id, seq, committed) VALUES (?1, ?2, 0);",
                        rusqlite::params![writer_id, seq],
                    )?;
                    // Update to committed
                    tx.execute(
                        "UPDATE concurrency_probe SET committed = 1 WHERE writer_id = ?1 AND seq = ?2;",
                        rusqlite::params![writer_id, seq],
                    )?;
                    tx.commit()?;
                    Ok(())
                }).await;

                if let Err(e) = res {
                    if e.contains("busy") || e.contains("locked") {
                        busy_clone.fetch_add(1, Ordering::SeqCst);
                    } else {
                        panic!("Writer {writer_id} failed: {e}");
                    }
                }

                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        }));
    }

    // Await all writers to complete
    for wh in writer_handles {
        wh.await.expect("writer task panicked");
    }

    // Stop readers and await
    stop_signal.store(true, Ordering::SeqCst);
    for rh in reader_handles {
        rh.await.expect("reader task panicked");
    }

    // Assert zero uncommitted reads (strict Read Committed ACID guarantee)
    assert_eq!(
        total_uncommitted_reads.load(Ordering::SeqCst),
        0,
        "Read Committed isolation violated: dirty uncommitted rows observed by reader!"
    );

    // Assert zero SQLITE_BUSY errors
    assert_eq!(
        total_busy_errors.load(Ordering::SeqCst),
        0,
        "Concurrency failure: SQLITE_BUSY detected during WAL pool operation!"
    );

    // Assert all rows were written successfully
    let final_row_count: i64 = pool
        .with_reader(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM concurrency_probe WHERE committed = 1;",
                [],
                |row| row.get(0),
            )
        })
        .expect("query final row count");

    assert_eq!(
        final_row_count as usize,
        num_writers * writes_per_writer,
        "All writer records must be committed and visible"
    );

    assert!(
        successful_reads.load(Ordering::Relaxed) > 50,
        "Readers must have executed substantial reads during write operations"
    );
}

// =========================================================================
// TEST 2: READER POOL EXHAUSTION & GRACEFUL 3S TIMEOUT
// =========================================================================

#[test]
fn test_wal_pool_reader_exhaustion_and_graceful_timeout() {
    let (pool, _guard) = create_file_pool();

    let pool_size = liva_native_core::db::get_reader_pool_size() as usize;

    // Checkout all reader connections to saturate the pool completely
    let mut held_conns = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let conn = pool
            .readers
            .get()
            .unwrap_or_else(|e| panic!("Failed to checkout reader connection {i}: {e}"));
        held_conns.push(conn);
    }

    assert_eq!(
        pool.readers.state().idle_connections,
        0,
        "All pooled reader connections must be checked out"
    );

    // Attempt checkout on a saturated pool.
    // The pool is configured with connection_timeout = 3 seconds.
    let start = Instant::now();
    let checkout_result = pool.read_conn();
    let elapsed = start.elapsed();

    // Verify graceful timeout: must return error and not panic
    assert!(
        checkout_result.is_err(),
        "Expected timeout error when reader pool is exhausted"
    );

    let err_msg = checkout_result.unwrap_err().to_string();
    let lower_err = err_msg.to_lowercase();
    assert!(
        lower_err.contains("timed out") || lower_err.contains("timeout"),
        "Error message should clearly denote timeout: got '{err_msg}'"
    );

    // Verify timeout duration is ~3 seconds (allow modest timing slack on CI/Windows)
    assert!(
        elapsed >= Duration::from_millis(2700),
        "Timeout fired too early: {elapsed:?} < 2.7s"
    );
    assert!(
        elapsed <= Duration::from_millis(5000),
        "Timeout took excessively long: {elapsed:?} > 5.0s"
    );

    // Release held connections back to the pool
    drop(held_conns);

    // Subsequent checkout must now succeed cleanly and immediately
    let fresh_checkout = pool.read_conn();
    assert!(
        fresh_checkout.is_ok(),
        "Reader connection checkout must immediately succeed after connections returned"
    );
}

// =========================================================================
// TEST 3: MICRO-BATCH ROLLBACK ATOMICITY & ZERO CSRGRAPH LEAKAGE
// =========================================================================

#[tokio::test]
async fn test_wal_pool_micro_batch_rollback_atomicity() {
    let (pool, _guard) = create_file_pool();

    // Setup a table with a unique constraint to trigger intentional conflict
    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS atomicity_probe (
                id INTEGER PRIMARY KEY,
                key TEXT UNIQUE NOT NULL
            );
            INSERT INTO atomicity_probe (key) VALUES ('existing_key');",
        )
    })
    .expect("setup atomicity probe");

    let initial_node_count = pool.get_compiled_csr_graph().node_count();

    // Verify executing a failed transaction on DbActor rolls back cleanly
    let tx_res = pool.with_writer(|conn| {
        let tx = conn.unchecked_transaction()?;
        tx.execute(
            "INSERT INTO atomicity_probe (key) VALUES ('canary_valid_1');",
            [],
        )?;
        // Intentionally trigger UNIQUE constraint violation
        tx.execute(
            "INSERT INTO atomicity_probe (key) VALUES ('existing_key');",
            [],
        )?;
        tx.commit()?;
        Ok(())
    });

    assert!(
        tx_res.is_err(),
        "Transaction must fail due to UNIQUE constraint"
    );

    // Verify database state: canary_valid_1 must NOT exist
    let canary_exists: bool = pool
        .with_reader(|conn| {
            let count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM atomicity_probe WHERE key = 'canary_valid_1';",
                [],
                |row| row.get(0),
            )?;
            Ok(count > 0)
        })
        .expect("query canary");

    assert!(
        !canary_exists,
        "Rollback failed: partial uncommitted write persisted into SQLite!"
    );

    // Now test CsrGraph protection: insert an L3 triple followed by an intentional error
    // Ensure CsrGraph does NOT leak nodes/edges from aborted operations
    let (result_tx, result_rx) = tokio::sync::oneshot::channel();
    let _ = pool.writer_actor.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            // Check that aborted transactions leave no trace
            conn.execute_batch(
                "BEGIN IMMEDIATE;
                 INSERT INTO l3_nodes (id, label, properties) VALUES ('phantom_node_1', 'phantom', '{}');
                 -- Force failure
                 INSERT INTO atomicity_probe (key) VALUES ('existing_key');
                 COMMIT;",
            ).map_err(|e| e.to_string())
        }),
        result_tx: Some(result_tx),
    }).await;

    let res = result_rx.await.expect("receive reply");
    assert!(res.is_err(), "Batch should fail and rollback");

    // Force flush
    let _ = pool.writer_actor.flush().await;

    // Verify CsrGraph snapshot is clean and unpolluted
    let current_graph = pool.get_compiled_csr_graph();
    assert_eq!(
        current_graph.node_count(),
        initial_node_count,
        "CsrGraph leaked mutated nodes during rolled-back transaction!"
    );
    assert!(
        current_graph.get_node_index("phantom_node_1").is_none(),
        "Phantom node must not exist in CsrGraph"
    );
}

// =========================================================================
// TEST 4: ACTIVE WAL CHECKPOINT INTERLEAVING UNDER CONCURRENT WORKLOAD
// =========================================================================

#[tokio::test]
async fn test_wal_pool_active_checkpoint_interleaving() {
    let (pool, _guard) = create_file_pool();

    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS checkpoint_probe (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                payload TEXT NOT NULL
            );",
        )
    })
    .expect("setup checkpoint probe table");

    let is_running = Arc::new(AtomicBool::new(true));
    let write_count = Arc::new(AtomicUsize::new(0));
    let read_count = Arc::new(AtomicUsize::new(0));

    // Active writers generating WAL activity
    let mut writer_tasks = Vec::new();
    for _ in 0..2 {
        let pool_c = pool.clone();
        let running = is_running.clone();
        let writes = write_count.clone();

        writer_tasks.push(tokio::spawn(async move {
            let mut i = 0;
            while running.load(Ordering::Relaxed) {
                i += 1;
                let payload = format!("checkpoint_payload_{i}_{}", "x".repeat(256));
                let _ = pool_c
                    .spawn_writer(move |conn| {
                        conn.execute(
                            "INSERT INTO checkpoint_probe (payload) VALUES (?1);",
                            rusqlite::params![payload],
                        )?;
                        Ok(())
                    })
                    .await;
                writes.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    // Active readers querying the table
    let mut reader_tasks = Vec::new();
    for _ in 0..4 {
        let pool_c = pool.clone();
        let running = is_running.clone();
        let reads = read_count.clone();

        reader_tasks.push(tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                let _ = pool_c.with_reader(|conn| {
                    let count: i64 =
                        conn.query_row("SELECT COUNT(*) FROM checkpoint_probe;", [], |row| {
                            row.get(0)
                        })?;
                    Ok(count)
                });
                reads.fetch_add(1, Ordering::Relaxed);
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    // Allow initial writes to populate WAL
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Interleave WAL checkpoints repeatedly while writers and readers are running
    for checkpoint_round in 0..5 {
        tokio::time::sleep(Duration::from_millis(20)).await;

        let ckpt_res = pool.writer_actor.checkpoint_wal().await;
        assert!(
            ckpt_res.is_ok(),
            "WAL checkpoint round {checkpoint_round} failed: {:?}",
            ckpt_res.err()
        );
    }

    // Stop background load
    is_running.store(false, Ordering::SeqCst);
    for wt in writer_tasks {
        wt.await.expect("writer task finished");
    }
    for rt in reader_tasks {
        rt.await.expect("reader task finished");
    }

    assert!(
        write_count.load(Ordering::Relaxed) > 20,
        "Must have executed writes during test"
    );
    assert!(
        read_count.load(Ordering::Relaxed) > 20,
        "Must have executed reads during test"
    );

    // Final verification: database is completely consistent and queryable
    let total_records: i64 = pool
        .with_reader(|conn| {
            conn.query_row("SELECT COUNT(*) FROM checkpoint_probe;", [], |row| {
                row.get(0)
            })
        })
        .expect("query total records");

    assert_eq!(total_records as usize, write_count.load(Ordering::Relaxed));
}
