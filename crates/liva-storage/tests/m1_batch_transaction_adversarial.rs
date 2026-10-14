//! Empirical Adversarial Verification Suite for Milestone M1
//!
//! Validates:
//! 1. `process_transactional_batch` atomicity and channel preservation under failures at any position K.
//! 2. `DbActorHandle` error handling across all API methods (`send`, `flush`, `execute`, `checkpoint_wal`).
//! 3. Fire-and-forget channel interleaving (`result_tx: None`).
//! 4. Multi-threaded concurrency and stress resilience under interleaved failures.
//! 5. SQLite UNIQUE constraint violation atomicity and ledger integrity.

use liva_storage::pragmas::configure_connection;
use liva_storage::{DbActorHandle, DbWriteCommand, StorageError, run_db_actor};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

fn create_temp_db_pool(prefix: &str) -> (Pool<SqliteConnectionManager>, PathBuf) {
    let mut path = std::env::temp_dir();
    let unique_name = format!(
        "liva_test_{}_{}_{}.db",
        prefix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    );
    path.push(unique_name);

    let manager = SqliteConnectionManager::file(&path);
    let pool = Pool::builder()
        .max_size(8)
        .build(manager)
        .expect("build pool");

    {
        let conn = pool.get().expect("get init conn");
        configure_connection(&conn, false).expect("configure pragmas");
    }

    (pool, path)
}

// -----------------------------------------------------------------------------
// TEST 1: Parameterized Matrix of Batch Sizes N and Failure Positions K
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_batch_matrix_position_k_failure() {
    let batch_configs: Vec<(usize, Vec<usize>)> = vec![
        // (N, list of K failure indices to test)
        (1, vec![0]),
        (2, vec![0, 1]),
        (5, vec![0, 2, 4]),
        (10, vec![0, 4, 9]),
        (20, vec![0, 10, 19]),
    ];

    for (n, k_list) in batch_configs {
        for k in k_list {
            let (pool, db_path) = create_temp_db_pool(&format!("matrix_n{}_k{}", n, k));

            // Initialize table
            {
                let conn = pool.get().expect("get conn");
                conn.execute_batch(
                    "CREATE TABLE matrix_ledger (id INTEGER PRIMARY KEY, payload TEXT);",
                )
                .expect("create table");
            }

            let (tx, rx) = mpsc::channel(n + 10);
            let mut receivers = Vec::with_capacity(n);

            for i in 0..n {
                let (res_tx, res_rx) = oneshot::channel();
                receivers.push(res_rx);

                let is_failure_item = i == k;
                let cmd = DbWriteCommand::Execute {
                    op: Box::new(move |conn| {
                        if is_failure_item {
                            Err(format!("synthetic_error_at_k_{k}"))
                        } else {
                            conn.execute(
                                "INSERT INTO matrix_ledger (id, payload) VALUES (?1, ?2)",
                                rusqlite::params![i as i64, format!("payload_{i}")],
                            )
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                        }
                    }),
                    result_tx: Some(res_tx),
                };
                tx.send(cmd).await.expect("queue command into channel");
            }

            // Spawn actor with max_batch_size = N so all N items are ingested in one batch
            let actor_pool = pool.clone();
            let actor_task = tokio::spawn(async move {
                run_db_actor(actor_pool, rx, n, Duration::from_millis(50)).await;
            });

            // Evaluate responses for all N commands
            for (i, res_rx) in receivers.into_iter().enumerate() {
                let recv_res = res_rx.await;
                assert!(
                    recv_res.is_ok(),
                    "Receiver at index {i} (batch size {n}, failure k={k}) MUST NOT be dropped! RecvError: {:?}",
                    recv_res.err()
                );

                let result = recv_res.unwrap();
                if i < k {
                    // Preceding command: rolled back
                    assert!(
                        result.is_err(),
                        "Command at index {i} < {k} must return Err, got Ok"
                    );
                    let err_msg = result.unwrap_err();
                    assert!(
                        err_msg.contains("Transaction rolled back due to error in batch"),
                        "Command at {i} < {k} expected rollback notice, got: {err_msg}"
                    );
                    assert!(
                        err_msg.contains(&format!("synthetic_error_at_k_{k}")),
                        "Rollback notice must include original error at {k}"
                    );
                } else if i == k {
                    // Failing command: direct error
                    assert!(
                        result.is_err(),
                        "Failing command at index {k} must return Err"
                    );
                    let err_msg = result.unwrap_err();
                    assert_eq!(
                        err_msg,
                        format!("synthetic_error_at_k_{k}"),
                        "Failing command at {k} must receive direct error"
                    );
                } else {
                    // Succeeding command: rolled back
                    assert!(
                        result.is_err(),
                        "Command at index {i} > {k} must return Err, got Ok"
                    );
                    let err_msg = result.unwrap_err();
                    assert!(
                        err_msg.contains("Transaction rolled back due to error in batch"),
                        "Command at {i} > {k} expected rollback notice, got: {err_msg}"
                    );
                    assert!(
                        err_msg.contains(&format!("synthetic_error_at_k_{k}")),
                        "Rollback notice must include original error at {k}"
                    );
                }
            }

            // Verify database state: 0 rows committed
            {
                let conn = pool.get().expect("get conn");
                let count: i64 = conn
                    .query_row("SELECT COUNT(*) FROM matrix_ledger", [], |r| r.get(0))
                    .expect("query count");
                assert_eq!(
                    count, 0,
                    "Database ledger must contain 0 committed rows for batch N={n}, K={k}"
                );
            }

            drop(tx);
            let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
            let _ = std::fs::remove_file(&db_path);
        }
    }
}

// -----------------------------------------------------------------------------
// TEST 2: Untracked Commands (result_tx = None) and Flush Interleaving
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_batch_with_untracked_and_flush_interleaving() {
    let (pool, db_path) = create_temp_db_pool("untracked_interleaving");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch("CREATE TABLE mixed_ledger (id INTEGER PRIMARY KEY, note TEXT);")
            .expect("create table");
    }

    let (tx, rx) = mpsc::channel(30);

    // Build 10 commands:
    // 0: Tracked (ok)
    // 1: Untracked (ok)
    // 2: Flush (tracked)
    // 3: Tracked (ok)
    // 4: Failing Tracked (err)
    // 5: Untracked (would be ok)
    // 6: Tracked (would be ok)
    // 7: Flush (tracked)
    // 8: Untracked (would be ok)
    // 9: Tracked (would be ok)

    let (tx0, rx0) = oneshot::channel();
    let (tx2_flush, rx2_flush) = oneshot::channel();
    let (tx3, rx3) = oneshot::channel();
    let (tx4_fail, rx4_fail) = oneshot::channel();
    let (tx6, rx6) = oneshot::channel();
    let (tx7_flush, rx7_flush) = oneshot::channel();
    let (tx9, rx9) = oneshot::channel();

    // 0: Tracked ok
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (0, 'zero')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx0),
    })
    .await
    .unwrap();

    // 1: Untracked ok (result_tx = None)
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (1, 'one')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: None,
    })
    .await
    .unwrap();

    // 2: Flush
    tx.send(DbWriteCommand::Flush {
        result_tx: Some(tx2_flush),
    })
    .await
    .unwrap();

    // 3: Tracked ok
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (3, 'three')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx3),
    })
    .await
    .unwrap();

    // 4: Failing tracked
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|_| Err("fatal_injected_error_at_cmd_4".to_string())),
        result_tx: Some(tx4_fail),
    })
    .await
    .unwrap();

    // 5: Untracked
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (5, 'five')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: None,
    })
    .await
    .unwrap();

    // 6: Tracked
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (6, 'six')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx6),
    })
    .await
    .unwrap();

    // 7: Flush
    tx.send(DbWriteCommand::Flush {
        result_tx: Some(tx7_flush),
    })
    .await
    .unwrap();

    // 8: Untracked
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (8, 'eight')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: None,
    })
    .await
    .unwrap();

    // 9: Tracked
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute("INSERT INTO mixed_ledger VALUES (9, 'nine')", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx9),
    })
    .await
    .unwrap();

    let actor_pool = pool.clone();
    let actor_task = tokio::spawn(async move {
        run_db_actor(actor_pool, rx, 10, Duration::from_millis(50)).await;
    });

    // Check rx0: was part of batch 1 (cmds 0, 1, 2) which committed before cmd 4
    let res0 = rx0.await.expect("rx0 channel open");
    assert!(res0.is_ok(), "rx0 committed in batch 1 before flush");

    // Check rx2_flush: batch 1 committed
    let res2 = rx2_flush.await.expect("rx2_flush channel open");
    assert!(res2.is_ok(), "rx2_flush committed in batch 1");

    // Check rx3: in batch 2, preceded failing cmd 4 -> rolled back
    let res3 = rx3.await.expect("rx3 channel open");
    assert!(
        res3.unwrap_err().contains("Transaction rolled back"),
        "rx3 rolled back in batch 2"
    );

    // Check rx4_fail: direct error
    let res4 = rx4_fail.await.expect("rx4_fail channel open");
    assert_eq!(res4.unwrap_err(), "fatal_injected_error_at_cmd_4");

    // Check rx6: in batch 2, succeeded failing cmd 4 -> rolled back
    let res6 = rx6.await.expect("rx6 channel open");
    assert!(
        res6.unwrap_err().contains("Transaction rolled back"),
        "rx6 rolled back in batch 2"
    );

    // Check rx7_flush: flush in batch 2 -> rolled back
    let res7 = rx7_flush.await.expect("rx7_flush channel open");
    assert!(
        res7.unwrap_err().contains("Transaction rolled back"),
        "rx7_flush rolled back in batch 2"
    );

    // Check rx9: in batch 3 (cmds 8, 9) -> committed
    let res9 = rx9.await.expect("rx9 channel open");
    assert!(res9.is_ok(), "rx9 committed in batch 3");

    // Verify database count = 4 (rows 0, 1 from batch 1 and 6, 9 from batch 3)
    {
        let conn = pool.get().expect("get conn");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM mixed_ledger", [], |r| r.get(0))
            .expect("query count");
        assert_eq!(
            count, 4,
            "Rows 0, 1 from batch 1 and 8, 9 from batch 3 must be committed (count = 4)"
        );
    }

    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    let _ = std::fs::remove_file(&db_path);
}

// -----------------------------------------------------------------------------
// TEST 2B: Single Transactional Batch Containing Flush Under Error
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_adversarial_single_batch_flush_failure_cases() {
    let (pool, db_path) = create_temp_db_pool("single_batch_flush");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch("CREATE TABLE flush_fail_ledger (id INTEGER PRIMARY KEY, msg TEXT);")
            .expect("create table");
    }

    // Case A: [Execute(ok), Execute(fail), Flush]
    // max_batch_size = 3. All 3 are queued into rx before actor starts.
    {
        let (tx, rx) = mpsc::channel(10);
        let (tx0, rx0) = oneshot::channel();
        let (tx1, rx1) = oneshot::channel();
        let (tx_flush, rx_flush) = oneshot::channel();

        tx.send(DbWriteCommand::Execute {
            op: Box::new(|conn| {
                conn.execute("INSERT INTO flush_fail_ledger VALUES (1, 'ok')", [])
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            }),
            result_tx: Some(tx0),
        })
        .await
        .unwrap();

        tx.send(DbWriteCommand::Execute {
            op: Box::new(|_| Err("middle_execute_failed".to_string())),
            result_tx: Some(tx1),
        })
        .await
        .unwrap();

        tx.send(DbWriteCommand::Flush {
            result_tx: Some(tx_flush),
        })
        .await
        .unwrap();

        let actor_pool = pool.clone();
        let actor_task = tokio::spawn(async move {
            run_db_actor(actor_pool, rx, 3, Duration::from_millis(50)).await;
        });

        let r0 = rx0.await.expect("rx0 channel open");
        assert!(
            r0.unwrap_err().contains("Transaction rolled back"),
            "rx0 rolled back"
        );

        let r1 = rx1.await.expect("rx1 channel open");
        assert_eq!(r1.unwrap_err(), "middle_execute_failed");

        let rf = rx_flush.await.expect("rx_flush channel open");
        assert!(
            rf.unwrap_err().contains("Transaction rolled back"),
            "Flush in failing batch must receive rollback error without dropped channel"
        );

        // Verify ledger empty
        {
            let conn = pool.get().expect("get conn");
            let count: i64 = conn
                .query_row("SELECT COUNT(*) FROM flush_fail_ledger", [], |r| r.get(0))
                .expect("query count");
            assert_eq!(count, 0, "Ledger must be empty");
        }

        drop(tx);
        let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    }

    let _ = std::fs::remove_file(&db_path);
}

// -----------------------------------------------------------------------------
// TEST 3: DbActorHandle Closed-Channel Error Handling
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_db_actor_handle_closed_channel_graceful_failures() {
    let (tx, rx) = mpsc::channel(1);
    // Explicitly drop rx to simulate crashed or terminated actor
    drop(rx);

    let handle = DbActorHandle::new(tx);

    // 1. send
    let send_res = handle.send(DbWriteCommand::Flush { result_tx: None }).await;
    assert!(send_res.is_err(), "send on closed channel must return Err");
    match send_res.unwrap_err() {
        StorageError::Actor(msg) => assert_eq!(msg, "Actor channel closed"),
        other => panic!("Expected StorageError::Actor, got {:?}", other),
    }

    // 2. flush
    let flush_res = handle.flush().await;
    assert!(
        flush_res.is_err(),
        "flush on closed channel must return Err"
    );
    match flush_res.unwrap_err() {
        StorageError::Actor(msg) => assert_eq!(msg, "Actor channel closed"),
        other => panic!("Expected StorageError::Actor, got {:?}", other),
    }

    // 3. execute
    let exec_res = handle.execute(|_| Ok(())).await;
    assert!(
        exec_res.is_err(),
        "execute on closed channel must return Err"
    );
    match exec_res.unwrap_err() {
        StorageError::Actor(msg) => assert_eq!(msg, "Actor channel closed"),
        other => panic!("Expected StorageError::Actor, got {:?}", other),
    }

    // 4. checkpoint_wal
    let cp_res = handle.checkpoint_wal().await;
    assert!(
        cp_res.is_err(),
        "checkpoint_wal on closed channel must return Err"
    );
    match cp_res.unwrap_err() {
        StorageError::Actor(msg) => assert_eq!(msg, "Actor channel closed"),
        other => panic!("Expected StorageError::Actor, got {:?}", other),
    }
}

// -----------------------------------------------------------------------------
// TEST 4: DbActorHandle Execute Error Propagation and Recovery
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_db_actor_handle_execute_error_propagation_and_recovery() {
    let (pool, db_path) = create_temp_db_pool("exec_recovery");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch("CREATE TABLE recovery_tbl (id INTEGER PRIMARY KEY, txt TEXT);")
            .expect("create table");
    }

    let (tx, rx) = mpsc::channel(32);
    let handle = DbActorHandle::new(tx.clone());

    let actor_pool = pool.clone();
    let actor_task = tokio::spawn(async move {
        run_db_actor(actor_pool, rx, 4, Duration::from_millis(20)).await;
    });

    // 1. Submit failing SQL execution
    let err_res = handle
        .execute(|conn| {
            conn.execute("INSERT INTO table_that_does_not_exist (id) VALUES (1)", [])
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .await;

    assert!(err_res.is_err(), "Failing SQL must yield error");
    match err_res.unwrap_err() {
        StorageError::Actor(msg) => {
            assert!(
                msg.contains("no such table"),
                "Expected 'no such table' error, got: {msg}"
            );
        }
        other => panic!("Expected StorageError::Actor, got {:?}", other),
    }

    // 2. Submit valid SQL: actor must have recovered and continue accepting commands
    let ok_res = handle
        .execute(|conn| {
            conn.execute(
                "INSERT INTO recovery_tbl (id, txt) VALUES (42, 'success')",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
        .await;

    assert!(ok_res.is_ok(), "Subsequent valid execute must succeed");

    // Verify row was written
    {
        let conn = pool.get().expect("get conn");
        let txt: String = conn
            .query_row("SELECT txt FROM recovery_tbl WHERE id = 42", [], |r| {
                r.get(0)
            })
            .expect("query txt");
        assert_eq!(txt, "success");
    }

    drop(handle);
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    let _ = std::fs::remove_file(&db_path);
}

// -----------------------------------------------------------------------------
// TEST 5: DbActorHandle Flush and CheckpointWal End-to-End
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_db_actor_handle_flush_and_checkpoint_wal_end_to_end() {
    let (pool, db_path) = create_temp_db_pool("flush_cp");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch("CREATE TABLE wal_demo (id INTEGER PRIMARY KEY, val TEXT);")
            .expect("create table");
    }

    let (tx, rx) = mpsc::channel(32);
    let handle = DbActorHandle::new(tx.clone());

    let actor_pool = pool.clone();
    let actor_task = tokio::spawn(async move {
        run_db_actor(actor_pool, rx, 8, Duration::from_millis(50)).await;
    });

    for i in 0..5 {
        handle
            .execute(move |conn| {
                conn.execute(
                    "INSERT INTO wal_demo (id, val) VALUES (?1, ?2)",
                    rusqlite::params![i, format!("val_{i}")],
                )
                .map(|_| ())
                .map_err(|e| e.to_string())
            })
            .await
            .expect("execute write");
    }

    // Test flush
    let flush_res = handle.flush().await;
    assert!(flush_res.is_ok(), "flush must succeed");

    // Test checkpoint_wal
    let cp_res = handle.checkpoint_wal().await;
    assert!(cp_res.is_ok(), "checkpoint_wal must succeed");

    // Verify all 5 rows exist
    {
        let conn = pool.get().expect("get conn");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM wal_demo", [], |r| r.get(0))
            .expect("query count");
        assert_eq!(count, 5, "All 5 rows must exist");
    }

    drop(handle);
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    let _ = std::fs::remove_file(&db_path);
}

// -----------------------------------------------------------------------------
// TEST 6: Multi-Threaded Concurrent Concurrency & Stress Harness
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_db_actor_concurrent_stress_and_thread_safety() {
    let (pool, db_path) = create_temp_db_pool("stress_concurrent");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch(
            "CREATE TABLE stress_ledger (id INTEGER PRIMARY KEY, worker_id INTEGER, seq INTEGER);",
        )
        .expect("create table");
    }

    let (tx, rx) = mpsc::channel(128);
    let handle = DbActorHandle::new(tx.clone());

    let actor_pool = pool.clone();
    let actor_task = tokio::spawn(async move {
        run_db_actor(actor_pool, rx, 16, Duration::from_millis(25)).await;
    });

    let num_workers = 8;
    let ops_per_worker = 25;
    let expected_successful_writes = Arc::new(AtomicUsize::new(0));

    let mut tasks = Vec::new();

    for worker_id in 0..num_workers {
        let h = handle.clone();
        let succ_counter = Arc::clone(&expected_successful_writes);

        let t = tokio::spawn(async move {
            for seq in 0..ops_per_worker {
                let unique_id = (worker_id * 1000 + seq) as i64;
                let is_injected_fail = (worker_id == 2 || worker_id == 5) && seq == 12;

                let res = h
                    .execute(move |conn| {
                        if is_injected_fail {
                            Err("injected_worker_stress_error".to_string())
                        } else {
                            conn.execute(
                                "INSERT INTO stress_ledger (id, worker_id, seq) VALUES (?1, ?2, ?3)",
                                rusqlite::params![unique_id, worker_id as i64, seq as i64],
                            )
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                        }
                    })
                    .await;

                if is_injected_fail {
                    assert!(res.is_err(), "Injected failure must return Err");
                } else if res.is_ok() {
                    succ_counter.fetch_add(1, Ordering::SeqCst);
                } else {
                    // It was rolled back because it was batched with the injected fail!
                    let err_msg = format!("{:?}", res.err());
                    assert!(
                        err_msg.contains("Transaction rolled back"),
                        "Co-batched item must be rolled back: {err_msg}"
                    );
                }

                // Occasionally flush or checkpoint
                if seq % 10 == 0 {
                    let _ = h.flush().await;
                }
            }
        });
        tasks.push(t);
    }

    for t in tasks {
        t.await.expect("worker task completed");
    }

    // Flush and checkpoint
    handle.flush().await.expect("final flush");
    handle.checkpoint_wal().await.expect("final checkpoint");

    let total_successful = expected_successful_writes.load(Ordering::SeqCst);

    // Verify row count in SQLite ledger exactly equals successful writes
    {
        let conn = pool.get().expect("get conn");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM stress_ledger", [], |r| r.get(0))
            .expect("query count");
        assert_eq!(
            count as usize, total_successful,
            "Committed ledger row count must match exactly the number of successful writes!"
        );
        // Ensure at least 80% succeeded despite deliberate failures
        assert!(
            total_successful >= (num_workers * ops_per_worker * 8 / 10),
            "Most writes should succeed: total_successful = {total_successful}"
        );
    }

    drop(handle);
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    let _ = std::fs::remove_file(&db_path);
}

// -----------------------------------------------------------------------------
// TEST 7: SQLite UNIQUE Constraint Violation Atomicity
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_transactional_unique_constraint_rollback_atomicity() {
    let (pool, db_path) = create_temp_db_pool("unique_atomicity");

    {
        let conn = pool.get().expect("get conn");
        conn.execute_batch(
            "CREATE TABLE accounts (id INTEGER PRIMARY KEY, iban TEXT UNIQUE, balance INTEGER);",
        )
        .expect("create table");
    }

    let (tx, rx) = mpsc::channel(32);
    let handle = DbActorHandle::new(tx.clone());

    let actor_pool = pool.clone();
    let actor_task = tokio::spawn(async move {
        run_db_actor(actor_pool, rx, 4, Duration::from_millis(25)).await;
    });

    // Batch 1: Insert valid accounts
    handle
        .execute(|conn| {
            conn.execute(
                "INSERT INTO accounts (id, iban, balance) VALUES (1, 'IBAN-01', 100)",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
        .await
        .expect("insert iban 1");

    handle
        .execute(|conn| {
            conn.execute(
                "INSERT INTO accounts (id, iban, balance) VALUES (2, 'IBAN-02', 200)",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        })
        .await
        .expect("insert iban 2");

    handle.flush().await.expect("flush batch 1");

    // Batch 2: Submit 3 commands in one batch where the middle violates UNIQUE
    let (tx_a, rx_a) = oneshot::channel();
    let (tx_b, rx_b) = oneshot::channel();
    let (tx_c, rx_c) = oneshot::channel();

    // Command A: Valid new account
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute(
                "INSERT INTO accounts (id, iban, balance) VALUES (3, 'IBAN-03', 300)",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx_a),
    })
    .await
    .unwrap();

    // Command B: Violates UNIQUE on 'IBAN-01'
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute(
                "INSERT INTO accounts (id, iban, balance) VALUES (4, 'IBAN-01', 400)",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx_b),
    })
    .await
    .unwrap();

    // Command C: Valid new account
    tx.send(DbWriteCommand::Execute {
        op: Box::new(|conn| {
            conn.execute(
                "INSERT INTO accounts (id, iban, balance) VALUES (5, 'IBAN-05', 500)",
                [],
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        result_tx: Some(tx_c),
    })
    .await
    .unwrap();

    // Evaluate responses
    let res_a = rx_a.await.expect("rx_a not dropped");
    assert!(
        res_a.unwrap_err().contains("Transaction rolled back"),
        "Command A must be rolled back"
    );

    let res_b = rx_b.await.expect("rx_b not dropped");
    let err_b = res_b.unwrap_err();
    assert!(
        err_b.contains("UNIQUE constraint failed"),
        "Command B must receive SQLite UNIQUE constraint error, got: {err_b}"
    );

    let res_c = rx_c.await.expect("rx_c not dropped");
    assert!(
        res_c.unwrap_err().contains("Transaction rolled back"),
        "Command C must be rolled back"
    );

    // Verify physical state: only accounts 1 and 2 exist!
    {
        let conn = pool.get().expect("get conn");
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM accounts", [], |r| r.get(0))
            .expect("query count");
        assert_eq!(count, 2, "Only initial 2 accounts must remain");

        let exists_3: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = 3)",
                [],
                |r| r.get(0),
            )
            .expect("query exists");
        assert!(!exists_3, "Account 3 must NOT exist");

        let exists_5: bool = conn
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM accounts WHERE id = 5)",
                [],
                |r| r.get(0),
            )
            .expect("query exists");
        assert!(!exists_5, "Account 5 must NOT exist");
    }

    drop(handle);
    drop(tx);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
    let _ = std::fs::remove_file(&db_path);
}
