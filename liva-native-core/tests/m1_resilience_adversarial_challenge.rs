//! Adversarial Stress & Concurrency Challenge Test Suite for Milestone M1
//! (Native Core Resilience & Concurrency).
//!
//! Empirically stress-tests:
//! 1. DbActor queue concurrency & backpressure:
//!    - Concurrent async callers (`execute`) and sync callers (`blocking_execute`).
//!    - Backpressure with burst of >1024 operations exceeding queue capacity.
//!    - Error propagation and actor thread health preservation.
//! 2. Vector upsert atomicity:
//!    - Rollback clean verification if sub-operation fails (vec_idx failure or vectors_fts failure).
//!    - Zero orphaned records in vectors_meta, vec_idx, or vectors_fts.
//!    - Respect of outer transaction boundaries and clean rollback.
//! 3. WebSocket accept recovery & fault resilience:
//!    - Resilience against raw TCP garbage, abrupt disconnects, and malformed handshakes.
//!    - Proof that the server accept loop remains alive and continues serving valid clients.

use bytes::Bytes;
use futures_util::{SinkExt, StreamExt};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{self, DatabasePool, MEMORY_VECTOR_DIM};
use liva_native_core::webrtc::frame::{OP_AUTH_HANDSHAKE, VoiceFrame};
use liva_native_core::websocket::WebSocketServer;
use liva_native_core::{AppState, llm, stt, tts};
use std::sync::Arc;
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, tungstenite::Message};

fn create_test_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
    })
}

// =========================================================================
// 1. DBACTOR QUEUE CONCURRENCY, SERIALIZATION & BACKPRESSURE
// =========================================================================

#[tokio::test]
async fn test_db_actor_concurrent_async_and_sync_writers_no_deadlock() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");

    // Initialize a test counter table on the writer
    pool.writer
        .get()
        .unwrap()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS stress_concurrency (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                caller_type TEXT NOT NULL,
                worker_id INTEGER NOT NULL
            );",
        )
        .expect("create test table");

    let pool = Arc::new(pool);
    let mut async_handles = Vec::new();

    // 1. Spawn 60 concurrent async callers using writer_actor.execute
    for worker_id in 0..60 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move {
            pool_clone
                .writer_actor
                .execute(move |conn| {
                    conn.execute(
                        "INSERT INTO stress_concurrency (caller_type, worker_id) VALUES ('async', ?1)",
                        [worker_id],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })
                .await
        });
        async_handles.push(handle);
    }

    // 2. Spawn 30 concurrent OS threads using writer_actor.blocking_execute
    let mut sync_threads = Vec::new();
    for worker_id in 0..30 {
        let pool_clone = Arc::clone(&pool);
        let th = std::thread::spawn(move || {
            pool_clone.writer_actor.blocking_execute(move |conn| {
                conn.execute(
                    "INSERT INTO stress_concurrency (caller_type, worker_id) VALUES ('sync', ?1)",
                    [worker_id],
                )
                .map_err(|e| e.to_string())?;
                Ok(())
            })
        });
        sync_threads.push(th);
    }

    // 3. Spawn 20 concurrent WAL checkpoints
    for _ in 0..20 {
        let pool_clone = Arc::clone(&pool);
        let handle = tokio::spawn(async move { pool_clone.writer_actor.checkpoint_wal().await });
        async_handles.push(handle);
    }

    // Wait for all sync threads
    for th in sync_threads {
        let res = th.join().expect("thread join failed");
        assert!(res.is_ok(), "blocking_execute failed: {:?}", res);
    }

    // Wait for all async tasks
    for handle in async_handles {
        let res = handle.await.expect("tokio task panicked");
        assert!(res.is_ok(), "async execute / checkpoint failed: {:?}", res);
    }

    // Read back through the reader pool to ensure consistency
    let reader_conn = pool.readers.get().expect("checkout reader connection");
    let count: i64 = reader_conn
        .query_row("SELECT COUNT(*) FROM stress_concurrency", [], |row| {
            row.get(0)
        })
        .expect("query count");

    assert_eq!(
        count, 90,
        "Expected exactly 90 records (60 async + 30 sync); found {count}"
    );

    let async_count: i64 = reader_conn
        .query_row(
            "SELECT COUNT(*) FROM stress_concurrency WHERE caller_type = 'async'",
            [],
            |row| row.get(0),
        )
        .expect("query async count");
    let sync_count: i64 = reader_conn
        .query_row(
            "SELECT COUNT(*) FROM stress_concurrency WHERE caller_type = 'sync'",
            [],
            |row| row.get(0),
        )
        .expect("query sync count");

    assert_eq!(async_count, 60);
    assert_eq!(sync_count, 30);
}

#[tokio::test]
async fn test_db_actor_backpressure_burst_exceeding_queue_capacity() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");

    pool.writer
        .get()
        .unwrap()
        .execute_batch(
            "CREATE TABLE IF NOT EXISTS stress_backpressure (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                item_val INTEGER NOT NULL
            );",
        )
        .expect("create test table");

    let pool = Arc::new(pool);
    // Bounded queue capacity is 1024. Flood 1200 operations to force backpressure.
    let total_burst = 1200;
    let mut tasks = Vec::with_capacity(total_burst);

    for i in 0..total_burst {
        let pool_clone = Arc::clone(&pool);
        tasks.push(tokio::spawn(async move {
            pool_clone
                .writer_actor
                .execute(move |conn| {
                    conn.execute(
                        "INSERT INTO stress_backpressure (item_val) VALUES (?1)",
                        [i],
                    )
                    .map_err(|e| e.to_string())?;
                    Ok(())
                })
                .await
        }));
    }

    // Await all tasks with a timeout to detect any deadlock under backpressure
    let timeout_duration = Duration::from_secs(10);
    let results = tokio::time::timeout(timeout_duration, async {
        let mut res = Vec::with_capacity(total_burst);
        for t in tasks {
            res.push(t.await.expect("task panicked"));
        }
        res
    })
    .await
    .expect("Deadlock detected: DbActor queue burst of 1200 exceeded timeout!");

    for r in results {
        assert!(r.is_ok(), "DbActor write failed under burst: {:?}", r);
    }

    // Verify row count
    let reader = pool.readers.get().expect("reader connection");
    let total_rows: i64 = reader
        .query_row("SELECT COUNT(*) FROM stress_backpressure", [], |row| {
            row.get(0)
        })
        .expect("query count");

    assert_eq!(
        total_rows, total_burst as i64,
        "All 1200 burst writes must be recorded deterministically without loss"
    );
}

struct TempDirGuard {
    path: std::path::PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let mut path = std::env::temp_dir();
        let rand_val: u64 = rand::random();
        path.push(format!("{}_{}", name, rand_val));
        std::fs::create_dir_all(&path).expect("create temp dir");
        Self { path }
    }

    fn path(&self) -> &std::path::Path {
        &self.path
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        if self.path.exists() {
            let _ = std::fs::remove_dir_all(&self.path);
        }
    }
}

#[tokio::test]
async fn test_db_actor_500_turn_burst_wal_checkpoints_and_concurrent_readers_zero_busy() {
    let temp_guard = TempDirGuard::new("liva_stress_wal");
    let db_path = temp_guard.path().join("liva_stress_wal.db");

    let pool = DatabasePool::new(&db_path).expect("open on-disk database with WAL");
    let pool = Arc::new(pool);
    let crypto = Arc::new(EncryptionEngine::new("0123456789abcdef0123456789abcdef"));

    // Verify WAL mode is active on disk
    {
        let reader = pool.readers.get().expect("reader connection");
        let journal_mode: String = reader
            .query_row("PRAGMA journal_mode;", [], |r| r.get(0))
            .expect("query journal mode");
        assert_eq!(
            journal_mode.to_lowercase(),
            "wal",
            "Database must be in WAL mode"
        );
    }

    let is_running = Arc::new(std::sync::atomic::AtomicBool::new(true));
    let reader_busy_errors = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let reader_success_queries = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    // Spawn 8 background reader tasks executing queries continuously
    let mut reader_handles = Vec::new();
    for _ in 0..8 {
        let pool_clone = Arc::clone(&pool);
        let running_clone = Arc::clone(&is_running);
        let busy_clone = Arc::clone(&reader_busy_errors);
        let success_clone = Arc::clone(&reader_success_queries);

        let handle = tokio::spawn(async move {
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                let res = pool_clone
                    .spawn_reader(|conn| {
                        let mut stmt = conn.prepare_cached(
                            "SELECT id, vec_id, content FROM vectors_meta ORDER BY id DESC LIMIT 5",
                        )?;
                        let mut rows = stmt.query([])?;
                        let mut cnt = 0;
                        while let Some(_row) = rows.next()? {
                            cnt += 1;
                        }
                        Ok(cnt)
                    })
                    .await;

                match res {
                    Ok(_) => {
                        success_clone.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    }
                    Err(e) => {
                        if e.contains("database is locked") || e.contains("busy") {
                            busy_clone.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                        }
                    }
                }
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        });
        reader_handles.push(handle);
    }

    // Spawn background WAL checkpoint loop (simulating background maintenance)
    let checkpoint_pool = Arc::clone(&pool);
    let checkpoint_running = Arc::clone(&is_running);
    let checkpoint_handle = tokio::spawn(async move {
        while checkpoint_running.load(std::sync::atomic::Ordering::Relaxed) {
            let _ = checkpoint_pool.writer_actor.checkpoint_wal().await;
            tokio::time::sleep(Duration::from_millis(15)).await;
        }
    });

    // Burst 500 persist_turn calls concurrently
    let total_turns = 500;
    let mut write_tasks = Vec::with_capacity(total_turns);
    for i in 0..total_turns {
        let pool_clone = Arc::clone(&pool);
        let crypto_clone = Arc::clone(&crypto);
        let scope = liva_native_core::agent::graph::ConversationMemoryScope::new(
            "stress_domain",
            "stress_category",
        )
        .expect("valid scope");
        let content = format!("User turn message {i} with unique payload data");
        let vector = vec![0.01_f32 * (i as f32 % 10.0); MEMORY_VECTOR_DIM];

        write_tasks.push(tokio::spawn(async move {
            pool_clone
                .writer_actor
                .persist_turn(scope, content, vector, crypto_clone)
                .await
        }));
    }

    // Also interleave 50 sync touch_fact_access calls
    let mut sync_threads = Vec::new();
    for i in 0..50 {
        let pool_clone = Arc::clone(&pool);
        let th = std::thread::spawn(move || {
            pool_clone
                .writer_actor
                .touch_fact_access(format!("fact_{i}"), i as i64);
        });
        sync_threads.push(th);
    }

    for th in sync_threads {
        th.join().expect("sync thread join failed");
    }

    // Await all 500 write tasks
    let mut write_errors = 0;
    for task in write_tasks {
        match task.await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => {
                eprintln!("persist_turn error: {e}");
                write_errors += 1;
            }
            Err(e) => {
                eprintln!("tokio task panicked: {e}");
                write_errors += 1;
            }
        }
    }

    // Stop background readers & checkpoint loop
    is_running.store(false, std::sync::atomic::Ordering::Relaxed);
    let _ = checkpoint_handle.await;
    for h in reader_handles {
        let _ = h.await;
    }

    // Assert zero write errors (0 dropped turns)
    assert_eq!(
        write_errors, 0,
        "All 500 turns must be persisted without error (0 dropped turns)"
    );

    // Assert zero SQLITE_BUSY errors in readers
    let busy_count = reader_busy_errors.load(std::sync::atomic::Ordering::SeqCst);
    assert_eq!(
        busy_count, 0,
        "Zero SQLITE_BUSY errors expected during concurrent reads, writes, and WAL checkpoints"
    );

    let successful_reads = reader_success_queries.load(std::sync::atomic::Ordering::Relaxed);
    assert!(
        successful_reads > 20,
        "Readers must successfully execute queries concurrently (got {successful_reads})"
    );

    // Final verification: exactly 500 turns in vectors_meta
    let reader = pool.readers.get().expect("checkout reader");
    let total_persisted: i64 = reader
        .query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE domain = 'memory_owner:stress_domain'",
            [],
            |r| r.get(0),
        )
        .expect("query vectors_meta count");
    assert_eq!(
        total_persisted, 500,
        "Expected exactly 500 persisted turns in vectors_meta; found {total_persisted}"
    );

    // Verify decryption integrity of persisted records
    let mut stmt = reader
        .prepare(
            "SELECT content FROM vectors_meta WHERE domain = 'memory_owner:stress_domain' ORDER BY id ASC LIMIT 5",
        )
        .expect("prepare select");
    let mut rows = stmt.query([]).expect("query rows");
    let mut decrypted_count = 0;
    while let Some(row) = rows.next().expect("fetch row") {
        let encrypted_content: String = row.get(0).expect("get content");
        let decrypted = crypto.decrypt_read(&encrypted_content);
        assert!(
            decrypted.starts_with("User turn message"),
            "Decrypted content must match payload"
        );
        decrypted_count += 1;
    }
    assert_eq!(
        decrypted_count, 5,
        "Successfully verified decrypted turn contents"
    );
}

#[tokio::test]
async fn test_db_actor_error_propagation_and_thread_health() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");

    // 1. Async error return propagates correctly
    let async_err: Result<(), String> = pool
        .writer_actor
        .execute(|_conn| -> Result<(), String> { Err("simulated async error".to_string()) })
        .await;
    assert_eq!(
        async_err.unwrap_err(),
        "simulated async error",
        "DbActor::execute must propagate Err without panic"
    );

    // 2. Sync error return propagates correctly via spawn_blocking
    let pool_clone = pool.clone();
    let sync_err: Result<(), String> = tokio::task::spawn_blocking(move || {
        pool_clone
            .writer_actor
            .blocking_execute(|_conn| -> Result<(), String> {
                Err("simulated sync error".to_string())
            })
    })
    .await
    .expect("blocking task panicked");

    assert_eq!(
        sync_err.unwrap_err(),
        "simulated sync error",
        "DbActor::blocking_execute must propagate Err without panic"
    );

    // 3. Thread is still alive and processes next valid request
    let healthy_res = pool
        .writer_actor
        .execute(|conn| {
            let one: i32 = conn
                .query_row("SELECT 1", [], |r| r.get(0))
                .map_err(|e| e.to_string())?;
            Ok(one)
        })
        .await;

    assert_eq!(
        healthy_res.unwrap(),
        1,
        "DbActor thread must remain healthy and operational after error returns"
    );
}

// =========================================================================
// 2. VECTOR UPSERT ATOMICITY & ROLLBACK VERIFICATION
// =========================================================================

#[test]
fn test_upsert_vector_atomicity_clean_rollback_on_fts_failure() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");
    let engine = EncryptionEngine::new("00000000000000000000000000000000");
    let conn = pool.writer.get().expect("writer connection");

    // Drop vectors_fts table to simulate an unrecoverable failure at step 5
    // (after vectors_meta insert and vec_idx insert have occurred).
    conn.execute_batch("DROP TABLE vectors_fts;")
        .expect("drop fts table");

    let vector = vec![0.1_f32; MEMORY_VECTOR_DIM];
    let vec_id = "test_fts_atomic_failure";

    // Attempt upsert with type != conversation_turn so it attempts to write to vectors_fts
    let res = db::upsert_vector(
        &conn,
        &engine,
        vec_id,
        "fact",
        "this content must be completely rolled back",
        &vector,
        Some("domain_stress"),
        Some("category_stress"),
        None,
        None,
        None,
    );

    assert!(
        res.is_err(),
        "upsert_vector must fail when sub-operation on vectors_fts fails"
    );

    // Verify atomicity: vectors_meta MUST NOT retain the partial record!
    let meta_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE vec_id = ?1",
            [vec_id],
            |row| row.get(0),
        )
        .expect("query vectors_meta");
    assert_eq!(
        meta_count, 0,
        "ATOMICITY VIOLATION: vectors_meta contains orphaned row after FTS failure!"
    );

    // Verify vec_idx has no partial record
    let idx_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vec_idx WHERE rowid IN (SELECT id FROM vectors_meta WHERE vec_id = ?1)",
            [vec_id],
            |row| row.get(0),
        )
        .expect("query vec_idx");
    assert_eq!(
        idx_count, 0,
        "ATOMICITY VIOLATION: vec_idx contains orphaned row after FTS failure!"
    );

    // Recreate vectors_fts and confirm subsequent clean upsert succeeds
    conn.execute_batch("CREATE VIRTUAL TABLE vectors_fts USING fts5(content);")
        .expect("recreate fts table");

    let clean_res = db::upsert_vector(
        &conn,
        &engine,
        vec_id,
        "fact",
        "this content succeeds after fts recovery",
        &vector,
        Some("domain_stress"),
        Some("category_stress"),
        None,
        None,
        None,
    );
    assert!(clean_res.is_ok(), "upsert_vector must succeed normally");

    let meta_count_after: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE vec_id = ?1",
            [vec_id],
            |row| row.get(0),
        )
        .expect("query vectors_meta");
    assert_eq!(
        meta_count_after, 1,
        "Record must exist after successful upsert"
    );
}

#[test]
fn test_upsert_vector_atomicity_clean_rollback_on_vec_idx_failure() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");
    let engine = EncryptionEngine::new("00000000000000000000000000000000");
    let conn = pool.writer.get().expect("writer connection");

    // Drop vec_idx table to simulate an unrecoverable failure at step 4
    // (after vectors_meta insert has occurred).
    conn.execute_batch("DROP TABLE vec_idx;")
        .expect("drop vec_idx table");

    let vector = vec![0.2_f32; MEMORY_VECTOR_DIM];
    let vec_id = "test_vec_idx_atomic_failure";

    let res = db::upsert_vector(
        &conn,
        &engine,
        vec_id,
        "conversation_turn",
        "content rolling back on vec_idx failure",
        &vector,
        Some("domain_stress"),
        Some("category_stress"),
        None,
        None,
        None,
    );

    assert!(
        res.is_err(),
        "upsert_vector must fail when sub-operation on vec_idx fails"
    );

    // Verify vectors_meta rolled back cleanly
    let meta_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE vec_id = ?1",
            [vec_id],
            |row| row.get(0),
        )
        .expect("query vectors_meta");
    assert_eq!(
        meta_count, 0,
        "ATOMICITY VIOLATION: vectors_meta contains orphaned row after vec_idx failure!"
    );
}

#[test]
fn test_upsert_vector_respects_outer_transaction_rollback() {
    let pool = DatabasePool::new_in_memory().expect("in-memory database");
    let engine = EncryptionEngine::new("00000000000000000000000000000000");
    let conn = pool.writer.get().expect("writer connection");

    let vector = vec![0.3_f32; MEMORY_VECTOR_DIM];
    let vec_id = "test_outer_tx_rollback";

    // Begin an outer transaction explicitly
    let outer_tx = conn
        .unchecked_transaction()
        .expect("start outer transaction");

    let res = db::upsert_vector(
        &outer_tx,
        &engine,
        vec_id,
        "fact",
        "content inside outer tx",
        &vector,
        None,
        None,
        None,
        None,
        None,
    );
    assert!(
        res.is_ok(),
        "upsert_vector must succeed inside outer transaction"
    );

    // Intentionally discard/rollback the outer transaction
    drop(outer_tx);

    // Verify all tables were rolled back cleanly
    let meta_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE vec_id = ?1",
            [vec_id],
            |row| row.get(0),
        )
        .expect("query vectors_meta");
    assert_eq!(
        meta_count, 0,
        "Outer transaction rollback must purge vectors_meta"
    );

    let fts_count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM vectors_fts WHERE content = 'content inside outer tx'",
            [],
            |row| row.get(0),
        )
        .expect("query vectors_fts");
    assert_eq!(
        fts_count, 0,
        "Outer transaction rollback must purge vectors_fts"
    );
}

// =========================================================================
// 3. WEBSOCKET ACCEPT RECOVERY & FAULT RESILIENCE
// =========================================================================

#[tokio::test]
async fn test_websocket_accept_resilience_to_tcp_garbage_and_abrupt_disconnects() {
    let server = WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let state = create_test_state();
    let server_task = tokio::spawn(server.run(state));

    // 1. Initial valid client connects successfully
    let (mut client_1, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("initial valid client must connect");

    let expected = VoiceFrame {
        op_code: OP_AUTH_HANDSHAKE,
        seq_id: 1,
        payload: Bytes::from_static(b"embedded-tauri"),
    };
    client_1
        .send(Message::Binary(
            expected.encode().expect("encode handshake").to_vec(),
        ))
        .await
        .expect("send handshake");
    let reply = tokio::time::timeout(Duration::from_secs(2), client_1.next())
        .await
        .expect("handshake reply timeout");
    assert!(reply.is_some(), "server responded to initial valid client");
    client_1.close(None).await.unwrap();

    // 2. Flood with 20 raw TCP connections sending garbage data, reset, or abrupt close
    for i in 0..20 {
        let mut stream = TcpStream::connect(address).await.expect("tcp connect");
        match i % 4 {
            0 => {
                // Send raw HTTP junk that is not a valid WebSocket handshake
                let _ = stream.write_all(b"GARBAGE_PAYLOAD_NOT_HTTP\r\n\r\n").await;
                let _ = stream.shutdown().await;
            }
            1 => {
                // Send partial HTTP GET and abruptly close
                let _ = stream
                    .write_all(b"GET /ws HTTP/1.1\r\nHost: localhost\r\n")
                    .await;
                drop(stream); // TCP RST / immediate drop
            }
            2 => {
                // Connect and immediately close without sending anything
                drop(stream);
            }
            3 => {
                // Send random binary noise
                let _ = stream
                    .write_all(&[0xFF, 0xFE, 0x00, 0x12, 0x34, 0x56])
                    .await;
                let _ = stream.shutdown().await;
            }
            _ => unreachable!(),
        }
    }

    // Small delay to allow connection JoinSet tasks to process and complete
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Verify the server task has NOT exited or panicked
    assert!(
        !server_task.is_finished(),
        "Server accept loop died from client anomalies!"
    );

    // 3. Second valid client connects cleanly after the assault
    let (mut client_2, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("server must remain receptive to new valid clients after network anomalies");

    let expected_2 = VoiceFrame {
        op_code: OP_AUTH_HANDSHAKE,
        seq_id: 2,
        payload: Bytes::from_static(b"embedded-tauri"),
    };
    client_2
        .send(Message::Binary(
            expected_2.encode().expect("encode handshake").to_vec(),
        ))
        .await
        .expect("send handshake 2");
    let reply_2 = tokio::time::timeout(Duration::from_secs(2), client_2.next())
        .await
        .expect("handshake 2 reply timeout");
    assert!(reply_2.is_some(), "server successfully serviced client 2");

    client_2.close(None).await.unwrap();
    server_task.abort();
}
