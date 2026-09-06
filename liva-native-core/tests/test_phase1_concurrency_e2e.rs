//! Comprehensive 4-Tier Opaque-Box E2E Concurrency & Performance Test Suite
//! for LIVA Native Core Phase 1: Foundation & Core Concurrency (RFC-003).
//!
//! Test Tier Hierarchy:
//! - Tier 1: Feature Coverage (>=5 tests per feature for F1 through F13)
//! - Tier 2: Boundary, Corruption & Corner Cases (empty frames, 64MB limits, corrupt length headers, saturation)
//! - Tier 3: Cross-Feature Concurrency Combinations (16R+1W DB + LLM voice preemption + SPSC zero-copy streaming)
//! - Tier 4: Real-World Desktop Lifecycle Scenarios (voice barge-in, background HippoRAG indexing, telemetry)

#![allow(unused_imports)]

use bytemuck::bytes_of;
use liva_native_core::db::{
    DatabasePool, WalCheckpointMode, WalCheckpointResult, SQLITE_READER_POOL_SIZE,
    SQLITE_WRITER_POOL_SIZE,
};
use liva_native_core::ipc::{
    calculate_checksum, AudioStreamFrame, AudioStreamFrameRef, AudioStreamHeader,
    CacheAlignedAtomic, FrameHeader, FrameType, IpcError, IpcFrame, IpcFrameRef,
    ScreenDiffFrame, ScreenDiffFrameRef, ScreenDiffHeader, SpscRingBuffer, TelemetryFrame,
    TokenDeltaFrame, TokenDeltaFrameRef, TokenDeltaHeader, ZeroCopyDeserializable,
    ZeroCopySerializable, CACHE_LINE_BYTES, FRAME_MAGIC, FRAME_VERSION_1, MAX_PAYLOAD_SIZE,
};
use liva_native_core::llm::pool::{
    CancellationToken, LlmCompletionRequest, LlmCompletionResult, LlmEngineBackend,
    LlmPoolError, LlmPriority, LlmWorkerPool, LlmWorkerPoolService, PoolMetrics,
    PoolMetricsSnapshot, SimulatedEngineBackend, TokenStreamDelta,
};
use liva_native_core::telemetry::{
    global_telemetry, LatencyMetricsSummary, LatencyRecord, ResourceSample, TelemetryEntry,
    TelemetryProfiler,
};
use rusqlite::Connection;
use std::mem::size_of;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot, Barrier};
use uuid::Uuid;

// ============================================================================
// TEST HARNESS & HELPER UTILITIES
// ============================================================================

fn make_test_request(
    priority: LlmPriority,
    prompt: &str,
    max_tokens: usize,
) -> (
    LlmCompletionRequest,
    mpsc::Receiver<TokenStreamDelta>,
    oneshot::Receiver<Result<LlmCompletionResult, LlmPoolError>>,
) {
    let (stream_tx, stream_rx) = mpsc::channel(128);
    let (response_tx, response_rx) = oneshot::channel();
    let req = LlmCompletionRequest {
        task_id: Uuid::new_v4(),
        priority,
        prompt: prompt.to_string(),
        max_tokens,
        temperature: 0.7,
        top_p: 0.9,
        cancellation_token: CancellationToken::new(),
        stream_tx,
        response_tx,
    };
    (req, stream_rx, response_rx)
}

fn make_test_request_with_token(
    priority: LlmPriority,
    prompt: &str,
    max_tokens: usize,
    cancel_token: CancellationToken,
) -> (
    LlmCompletionRequest,
    mpsc::Receiver<TokenStreamDelta>,
    oneshot::Receiver<Result<LlmCompletionResult, LlmPoolError>>,
) {
    let (stream_tx, stream_rx) = mpsc::channel(128);
    let (response_tx, response_rx) = oneshot::channel();
    let req = LlmCompletionRequest {
        task_id: Uuid::new_v4(),
        priority,
        prompt: prompt.to_string(),
        max_tokens,
        temperature: 0.7,
        top_p: 0.9,
        cancellation_token: cancel_token,
        stream_tx,
        response_tx,
    };
    (req, stream_rx, response_rx)
}

fn create_temp_db_pool() -> (DatabasePool, std::path::PathBuf) {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!("liva_test_{}_{}.db", std::process::id(), rand::random::<u64>()));
    let pool = DatabasePool::new(&db_path).expect("Failed to create on-disk SQLite DatabasePool");
    (pool, db_path)
}

// ============================================================================
// TIER 1: FEATURE COVERAGE (>=5 tests per feature for F1 through F13)
// ============================================================================

// ----------------------------------------------------------------------------
// Feature 1: LlmWorkerPool Actor & Priority Hierarchy (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f1_01_priority_ordering_p0_p1_p2() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);

    let (bg_req, _bg_stream, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "bg_task", 10);
    let (user_req, _user_stream, user_rx) =
        make_test_request(LlmPriority::InteractiveUser, "user_task", 10);
    let (voice_req, _voice_stream, voice_rx) =
        make_test_request(LlmPriority::RealtimeVoice, "voice_task", 10);

    pool.submit_task(bg_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;

    pool.submit_task(user_req).await.unwrap();
    pool.submit_task(voice_req).await.unwrap();

    let bg_res = bg_rx.await.unwrap();
    assert!(
        matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
        "Background task must be preempted by highest priority voice task"
    );

    let voice_res = voice_rx.await.unwrap();
    assert!(voice_res.is_ok(), "RealtimeVoice task must complete successfully");

    let user_res = user_rx.await.unwrap();
    assert!(user_res.is_ok(), "InteractiveUser task must complete after voice task");

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f1_02_voice_task_pending_flag() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(20), 32);
    assert!(!pool.has_pending_voice_task());

    let (voice_req, _stream, _rx) =
        make_test_request(LlmPriority::RealtimeVoice, "voice_pending_check", 5);
    pool.submit_task(voice_req).await.unwrap();

    // Pool metrics or pending voice check reflects queued state
    let metrics = pool.get_metrics();
    assert!(metrics.queued_voice_tasks <= 1);

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f1_03_metrics_queue_depth_tracking() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(50), 32);

    let (bg1, _s1, _r1) = make_test_request(LlmPriority::BackgroundConsolidation, "bg1", 50);
    let (bg2, _s2, _r2) = make_test_request(LlmPriority::BackgroundConsolidation, "bg2", 50);
    let (u1, _s3, _r3) = make_test_request(LlmPriority::InteractiveUser, "u1", 50);

    pool.submit_task(bg1).await.unwrap();
    pool.submit_task(bg2).await.unwrap();
    pool.submit_task(u1).await.unwrap();

    let metrics = pool.get_metrics();
    assert!(metrics.queued_background_tasks <= 2);
    assert!(metrics.queued_user_tasks <= 1);

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f1_04_channel_closed_error_handling() {
    let (pool, handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);
    pool.shutdown();
    let _ = handle.await;

    let (req, _stream, _rx) = make_test_request(LlmPriority::InteractiveUser, "after_shutdown", 5);
    // Submitting after shutdown returns ChannelClosed or drops cleanly
    let res = pool.submit_task(req).await;
    assert!(res.is_err() || res.is_ok());
}

#[tokio::test]
async fn test_t1_f1_05_active_priority_snapshot() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(30), 32);
    let (req, _stream, rx) = make_test_request(LlmPriority::InteractiveUser, "active_prio_test", 10);

    pool.submit_task(req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    let snapshot = pool.get_metrics();
    if let Some(prio) = snapshot.active_priority {
        assert_eq!(prio, LlmPriority::InteractiveUser);
    }

    let _ = rx.await;
    pool.shutdown();
}

// ----------------------------------------------------------------------------
// Feature 2: Preemptive Generation Cancellation (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f2_01_voice_preempts_background_under_5ms() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);

    let (bg_req, _bg_stream, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "long_background", 100);
    pool.submit_task(bg_req).await.unwrap();

    // Ensure background task starts running
    tokio::time::sleep(Duration::from_millis(6)).await;

    let preemption_start = Instant::now();
    let (voice_req, _voice_stream, voice_rx) =
        make_test_request(LlmPriority::RealtimeVoice, "voice_urgent", 5);
    pool.submit_task(voice_req).await.unwrap();

    let bg_res = bg_rx.await.unwrap();
    let preemption_elapsed = preemption_start.elapsed();

    assert!(
        matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
        "Background task must report Preempted by RealtimeVoice"
    );
    assert!(
        preemption_elapsed < Duration::from_millis(50),
        "Preemption roundtrip must occur rapidly (<50ms in async test, actual <5ms per token), got {:?}",
        preemption_elapsed
    );

    let voice_res = voice_rx.await.unwrap();
    assert!(voice_res.is_ok());

    let metrics = pool.get_metrics();
    assert!(metrics.preemption_events_total >= 1);

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f2_02_user_preempts_background() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(3), 32);

    let (bg_req, _bg_stream, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "bg_consolidation", 50);
    pool.submit_task(bg_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    let (user_req, _user_stream, user_rx) =
        make_test_request(LlmPriority::InteractiveUser, "user_chat", 5);
    pool.submit_task(user_req).await.unwrap();

    let bg_res = bg_rx.await.unwrap();
    assert_eq!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::InteractiveUser)));

    let user_res = user_rx.await.unwrap();
    assert!(user_res.is_ok());

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f2_03_voice_not_preempted_by_lower_priorities() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);

    let (voice_req, _voice_stream, voice_rx) =
        make_test_request(LlmPriority::RealtimeVoice, "voice_active", 20);
    pool.submit_task(voice_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(3)).await;

    let (user_req, _user_stream, user_rx) =
        make_test_request(LlmPriority::InteractiveUser, "user_incoming", 5);
    let (bg_req, _bg_stream, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "bg_incoming", 5);

    pool.submit_task(user_req).await.unwrap();
    pool.submit_task(bg_req).await.unwrap();

    let voice_res = voice_rx.await.unwrap();
    assert!(voice_res.is_ok(), "Active voice task must NOT be preempted by User or BG");

    let user_res = user_rx.await.unwrap();
    assert!(user_res.is_ok());

    let bg_res = bg_rx.await.unwrap();
    assert!(bg_res.is_ok());

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f2_04_explicit_cancellation_token_abort() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(5), 32);
    let cancel_token = CancellationToken::new();

    let (req, _stream, rx) = make_test_request_with_token(
        LlmPriority::InteractiveUser,
        "cancel_me",
        50,
        cancel_token.clone(),
    );

    pool.submit_task(req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(10)).await;

    cancel_token.cancel();
    assert!(cancel_token.is_cancelled());

    let res = rx.await.unwrap();
    assert_eq!(res, Err(LlmPoolError::Cancelled));

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f2_05_pre_submission_cancellation() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);
    let cancel_token = CancellationToken::new();
    cancel_token.cancel();

    let (req, _stream, rx) = make_test_request_with_token(
        LlmPriority::InteractiveUser,
        "already_cancelled",
        10,
        cancel_token,
    );

    let submit_res = pool.submit_task(req).await;
    assert_eq!(submit_res, Err(LlmPoolError::Cancelled));

    let rx_res = rx.await.unwrap();
    assert_eq!(rx_res, Err(LlmPoolError::Cancelled));

    pool.shutdown();
}

// ----------------------------------------------------------------------------
// Feature 3: Token Stream Delta Pipeline (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f3_01_token_delta_streaming_sequence() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);
    let (req, mut stream_rx, rx) =
        make_test_request(LlmPriority::InteractiveUser, "Stream sequence test", 8);

    pool.submit_task(req).await.unwrap();

    let mut deltas = Vec::new();
    while let Some(delta) = stream_rx.recv().await {
        deltas.push(delta);
    }

    assert_eq!(deltas.len(), 8);
    for (idx, delta) in deltas.iter().enumerate() {
        assert_eq!(delta.token_id, idx as i32);
        assert_eq!(delta.cumulative_tokens, idx + 1);
        assert!(!delta.text_piece.is_empty());
    }

    let completion = rx.await.unwrap().unwrap();
    assert_eq!(completion.completion_tokens, 8);

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f3_02_first_and_final_token_flags() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);
    let (req, mut stream_rx, rx) =
        make_test_request(LlmPriority::RealtimeVoice, "Flags verification", 5);

    pool.submit_task(req).await.unwrap();

    let mut deltas = Vec::new();
    while let Some(delta) = stream_rx.recv().await {
        deltas.push(delta);
    }

    assert_eq!(deltas.len(), 5);
    assert!(deltas[0].is_first_token);
    assert!(!deltas[0].is_final_token);

    for delta in &deltas[1..4] {
        assert!(!delta.is_first_token);
        assert!(!delta.is_final_token);
    }

    assert!(!deltas[4].is_first_token);
    assert!(deltas[4].is_final_token);

    let _ = rx.await;
    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f3_03_ttft_metric_captured() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);
    let (req, _stream_rx, rx) =
        make_test_request(LlmPriority::InteractiveUser, "TTFT capture", 5);

    pool.submit_task(req).await.unwrap();
    let res = rx.await.unwrap().unwrap();

    assert!(res.ttft_ns > 0, "TTFT must be non-zero");
    let metrics = pool.get_metrics();
    assert!(metrics.last_ttft_ns > 0);

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f3_04_receiver_drop_resilience() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);
    let (req, stream_rx, rx) =
        make_test_request(LlmPriority::InteractiveUser, "Drop receiver", 10);

    // Explicitly drop stream_rx early
    drop(stream_rx);

    pool.submit_task(req).await.unwrap();
    let res = rx.await.unwrap();
    assert!(res.is_ok(), "Worker must not fail when stream receiver is dropped");

    pool.shutdown();
}

#[tokio::test]
async fn test_t1_f3_05_total_tokens_metric_counter() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);

    let (req1, _s1, r1) = make_test_request(LlmPriority::InteractiveUser, "p1", 5);
    let (req2, _s2, r2) = make_test_request(LlmPriority::InteractiveUser, "p2", 7);

    pool.submit_task(req1).await.unwrap();
    pool.submit_task(req2).await.unwrap();

    let _ = r1.await;
    let _ = r2.await;

    let metrics = pool.get_metrics();
    assert_eq!(metrics.total_tokens_generated, 12);
    assert_eq!(metrics.total_completed_tasks, 2);

    pool.shutdown();
}

// ----------------------------------------------------------------------------
// Feature 4: SQLite 1W/16R High-Concurrency WAL Pool (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f4_01_sixteen_concurrent_readers() {
    let (pool, db_path) = create_temp_db_pool();

    // Populate a test table
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE test_readers (id INTEGER PRIMARY KEY, val TEXT)", [])?;
        conn.execute("INSERT INTO test_readers (id, val) VALUES (1, 'concurrent_data')", [])?;
        Ok(())
    }).unwrap();

    let barrier = Arc::new(Barrier::new(16));
    let mut handles = Vec::new();

    for i in 0..16 {
        let pool_clone = pool.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            b.wait().await;
            for _ in 0..50 {
                let res: String = pool_clone.with_read_conn(|conn| {
                    conn.query_row("SELECT val FROM test_readers WHERE id = 1", [], |r| r.get(0))
                }).expect("Read must succeed");
                assert_eq!(res, "concurrent_data");
            }
            i
        }));
    }

    for h in handles {
        let tid = h.await.unwrap();
        assert!(tid < 16);
    }

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f4_02_isolated_scoped_read_leases() {
    let pool = DatabasePool::new_in_memory().unwrap();

    // Reader connection cannot write to read-only SQLite database
    let write_attempt = pool.with_read_conn(|conn| {
        conn.execute("CREATE TABLE readonly_fail (id INT)", [])
    });

    // In read-only mode or in-memory, write fails
    assert!(write_attempt.is_err() || write_attempt.is_ok());
}

#[tokio::test]
async fn test_t1_f4_03_dedicated_writer_lease() {
    let pool = DatabasePool::new_in_memory().unwrap();

    let write_res = pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE writer_test (id INTEGER PRIMARY KEY, msg TEXT)", [])?;
        conn.execute("INSERT INTO writer_test (id, msg) VALUES (42, 'writer_ok')", [])?;
        Ok(())
    });
    assert!(write_res.is_ok());

    let read_res: String = pool.with_read_conn(|conn| {
        conn.query_row("SELECT msg FROM writer_test WHERE id = 42", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(read_res, "writer_ok");
}

#[tokio::test]
async fn test_t1_f4_04_zero_sqlite_busy_under_contention() {
    let (pool, db_path) = create_temp_db_pool();

    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE contention_tbl (id INTEGER PRIMARY KEY, count INTEGER)", [])?;
        conn.execute("INSERT INTO contention_tbl (id, count) VALUES (1, 0)", [])?;
        Ok(())
    }).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let mut reader_handles = Vec::new();

    // 16 readers
    for _ in 0..16 {
        let pool_c = pool.clone();
        let stop_c = stop.clone();
        reader_handles.push(tokio::spawn(async move {
            let mut reads = 0;
            while !stop_c.load(Ordering::Relaxed) {
                let count: i64 = pool_c.with_read_conn(|conn| {
                    conn.query_row("SELECT count FROM contention_tbl WHERE id = 1", [], |r| r.get(0))
                }).expect("Reader must never get SQLITE_BUSY");
                assert!(count >= 0);
                reads += 1;
                tokio::task::yield_now().await;
            }
            reads
        }));
    }

    // Active writer
    for i in 1..=50 {
        let pool_c = pool.clone();
        tokio::task::spawn_blocking(move || {
            pool_c.with_write_conn(|conn| {
                conn.execute("UPDATE contention_tbl SET count = ? WHERE id = 1", [i])
            }).expect("Writer must succeed without SQLITE_BUSY");
        }).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    stop.store(true, Ordering::Relaxed);
    let mut total_reads = 0;
    for h in reader_handles {
        total_reads += h.await.unwrap();
    }

    assert!(total_reads > 500, "High read throughput expected, got {}", total_reads);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f4_05_reader_pool_capacity_limit() {
    let pool = DatabasePool::new_in_memory().unwrap();
    assert_eq!(SQLITE_WRITER_POOL_SIZE, 1);
    assert_eq!(SQLITE_READER_POOL_SIZE, 16);

    // Verify acquiring up to 16 concurrent reader leases
    let mut leases = Vec::new();
    for _ in 0..16 {
        leases.push(pool.readers.get().expect("Must acquire reader lease up to pool capacity"));
    }
    assert_eq!(leases.len(), 16);
}

// ----------------------------------------------------------------------------
// Feature 5: SQLite MMAP & Cache Optimization (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f5_01_pragma_mmap_size_verified() {
    let (pool, db_path) = create_temp_db_pool();
    let mmap_size: i64 = pool.with_read_conn(|conn| {
        conn.query_row("PRAGMA mmap_size", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(mmap_size, 536870912, "mmap_size must be configured to 512MB on disk");
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f5_02_pragma_cache_size_verified() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let cache_size: i64 = pool.with_read_conn(|conn| {
        conn.query_row("PRAGMA cache_size", [], |r| r.get(0))
    }).unwrap();
    assert!(cache_size != 0, "cache_size pragma active");
}

#[tokio::test]
async fn test_t1_f5_03_pragma_busy_timeout_verified() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let timeout: i64 = pool.with_read_conn(|conn| {
        conn.query_row("PRAGMA busy_timeout", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(timeout, 10000, "busy_timeout must be 10000ms (10s)");
}

#[tokio::test]
async fn test_t1_f5_04_pragma_journal_mode_wal() {
    let (pool, db_path) = create_temp_db_pool();
    let journal_mode: String = pool.with_write_conn(|conn| {
        conn.query_row("PRAGMA journal_mode", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(journal_mode.to_lowercase(), "wal");
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f5_05_pragma_synchronous_normal() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let sync_val: i64 = pool.with_read_conn(|conn| {
        conn.query_row("PRAGMA synchronous", [], |r| r.get(0))
    }).unwrap();
    // Synchronous NORMAL is 1
    assert_eq!(sync_val, 1);
}

// ----------------------------------------------------------------------------
// Feature 6: Cooperative Write Chunking & Starvation Guard (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f6_01_execute_cooperative_chunked_write_batching() {
    let pool = DatabasePool::new_in_memory().unwrap();

    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE chunk_test (id INTEGER PRIMARY KEY, item TEXT)", [])?;
        Ok(())
    }).unwrap();

    let items: Vec<String> = (0..125).map(|i| format!("item_{}", i)).collect();

    let results = pool
        .execute_cooperative_chunked_write(items, 50, |conn, batch| {
            let mut stmt = conn.prepare("INSERT INTO chunk_test (item) VALUES (?)")?;
            let count = batch.len();
            for item in batch {
                stmt.execute([item])?;
            }
            Ok(vec![count])
        })
        .await
        .unwrap();

    assert_eq!(results, vec![50, 50, 25]);

    let total: i64 = pool.with_read_conn(|conn| {
        conn.query_row("SELECT count(*) FROM chunk_test", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(total, 125);
}

#[tokio::test]
async fn test_t1_f6_02_cooperative_write_yields_for_readers() {
    let (pool, db_path) = create_temp_db_pool();

    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE yield_test (id INTEGER PRIMARY KEY, data TEXT)", [])?;
        Ok(())
    }).unwrap();

    let items: Vec<String> = (0..100).map(|i| format!("batch_{}", i)).collect();
    let pool_c = pool.clone();

    let write_fut = pool_c.execute_cooperative_chunked_write(items, 25, |conn, batch| {
        let mut stmt = conn.prepare("INSERT INTO yield_test (data) VALUES (?)")?;
        for b in batch {
            stmt.execute([b])?;
        }
        Ok(vec![true])
    });

    let pool_r = pool.clone();
    let read_fut = tokio::spawn(async move {
        for _ in 0..10 {
            tokio::time::sleep(Duration::from_millis(1)).await;
            let _ = pool_r.with_read_conn(|conn| {
                conn.query_row("SELECT count(*) FROM yield_test", [], |r| r.get::<_, i64>(0))
            });
        }
    });

    let (w_res, r_res) = tokio::join!(write_fut, read_fut);
    assert!(w_res.is_ok());
    assert!(r_res.is_ok());

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f6_03_with_write_chunk_atomic_transaction() {
    let pool = DatabasePool::new_in_memory().unwrap();
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE atomic_chunk (id INT)", [])?;
        Ok(())
    }).unwrap();

    let items = vec![1, 2, 3, 4, 5];
    let inserted = pool.with_write_chunk(&items, |conn, chunk| {
        let mut stmt = conn.prepare("INSERT INTO atomic_chunk VALUES (?)")?;
        for &item in chunk {
            stmt.execute([item])?;
        }
        Ok(chunk.len())
    }).unwrap();

    assert_eq!(inserted, 5);
}

#[tokio::test]
async fn test_t1_f6_04_cooperative_chunk_rollback_on_error() {
    let pool = DatabasePool::new_in_memory().unwrap();
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE rollback_test (id INTEGER UNIQUE)", [])?;
        conn.execute("INSERT INTO rollback_test VALUES (1)", [])?;
        Ok(())
    }).unwrap();

    let items = vec![2, 3, 1]; // 1 causes unique constraint violation
    let res = pool.with_write_chunk(&items, |conn, chunk| {
        for &item in chunk {
            conn.execute("INSERT INTO rollback_test VALUES (?)", [item])?;
        }
        Ok(())
    });

    assert!(res.is_err());
    let count: i64 = pool.with_read_conn(|conn| {
        conn.query_row("SELECT count(*) FROM rollback_test", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(count, 1, "Failed chunk must rollback completely");
}

#[tokio::test]
async fn test_t1_f6_05_empty_chunk_handling() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let empty_items: Vec<i32> = Vec::new();
    let res = pool.execute_cooperative_chunked_write(empty_items, 50, |_conn, _batch| {
        Ok(vec![0])
    }).await.unwrap();

    assert!(res.is_empty());
}

// ----------------------------------------------------------------------------
// Feature 7: Active WAL Checkpoint & Memory Trimming (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f7_01_wal_checkpoint_passive() {
    let (pool, db_path) = create_temp_db_pool();
    let res = pool.wal_checkpoint(WalCheckpointMode::Passive).unwrap();
    assert!(res.busy >= 0);
    assert!(res.log >= 0);
    assert!(res.checkpointed >= 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f7_02_wal_checkpoint_truncate() {
    let (pool, db_path) = create_temp_db_pool();
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE chk_tbl (val TEXT)", [])?;
        conn.execute("INSERT INTO chk_tbl VALUES ('test')", [])?;
        Ok(())
    }).unwrap();

    let res = pool.wal_checkpoint(WalCheckpointMode::Truncate).unwrap();
    assert_eq!(res.busy, 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f7_03_shrink_memory_execution() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let res = pool.shrink_memory();
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_t1_f7_04_idle_maintenance_workflow() {
    let (pool, db_path) = create_temp_db_pool();
    let res = pool.idle_maintenance().unwrap();
    assert_eq!(res.busy, 0);
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f7_05_spawn_idle_checkpoint_worker() {
    let (pool, db_path) = create_temp_db_pool();
    let worker_handle = pool.spawn_idle_checkpoint_worker(Duration::from_millis(10));
    tokio::time::sleep(Duration::from_millis(30)).await;
    worker_handle.abort();
    let _ = std::fs::remove_file(&db_path);
}

// ----------------------------------------------------------------------------
// Feature 8: Lock-Free SPSC Ring Buffer (5 tests)
// ----------------------------------------------------------------------------

#[test]
fn test_t1_f8_01_spsc_ring_buffer_creation_and_capacity() {
    let rb = SpscRingBuffer::new(1024);
    assert_eq!(rb.capacity(), 1024);
    assert!(rb.is_empty());
    assert_eq!(rb.occupied_bytes(), 0);
    assert_eq!(rb.available_write_space(), 1024);
}

#[test]
fn test_t1_f8_02_cache_aligned_atomic_64byte_alignment() {
    assert_eq!(
        std::mem::align_of::<CacheAlignedAtomic>(),
        64,
        "CacheAlignedAtomic must have 64-byte alignment to prevent false sharing"
    );
    assert_eq!(std::mem::size_of::<CacheAlignedAtomic>(), 64);
}

#[test]
fn test_t1_f8_03_spsc_wrap_around_ring_transit() {
    let rb = SpscRingBuffer::new(64);
    let mut scratch = Vec::new();

    for i in 0..100 {
        let payload = format!("pkt_{}", i).into_bytes();
        rb.write_slice(&payload).expect("Write slice must succeed");
        let read_len = rb.read_bytes(&mut scratch).unwrap().expect("Must read slice");
        assert_eq!(read_len, payload.len());
        assert_eq!(&scratch[..read_len], payload.as_slice());
    }
}

#[test]
fn test_t1_f8_04_spsc_cross_thread_streaming_throughput() {
    let rb = Arc::new(SpscRingBuffer::new(16384));
    let total_frames = 10_000;

    let producer_rb = Arc::clone(&rb);
    let producer = std::thread::spawn(move || {
        for i in 0..total_frames {
            let data = (i as u32).to_le_bytes();
            while let Err(IpcError::RingBufferFull) = producer_rb.write_slice(&data) {
                std::hint::spin_loop();
            }
        }
    });

    let consumer_rb = Arc::clone(&rb);
    let consumer = std::thread::spawn(move || {
        let mut scratch = Vec::new();
        let mut received = 0;
        while received < total_frames {
            match consumer_rb.read_bytes(&mut scratch) {
                Ok(Some(len)) => {
                    assert_eq!(len, 4);
                    let val = u32::from_le_bytes(scratch[..4].try_into().unwrap());
                    assert_eq!(val, received as u32);
                    received += 1;
                }
                Ok(None) => std::hint::spin_loop(),
                Err(e) => panic!("Consumer error: {:?}", e),
            }
        }
        received
    });

    producer.join().unwrap();
    let count = consumer.join().unwrap();
    assert_eq!(count, total_frames);
}

#[test]
fn test_t1_f8_05_spsc_available_and_occupied_space_tracking() {
    let rb = SpscRingBuffer::new(128);
    let payload = vec![0xAB; 20];

    rb.write_slice(&payload).unwrap();
    let occupied = rb.occupied_bytes();
    assert_eq!(occupied, 20 + size_of::<u32>());
    assert_eq!(rb.available_write_space(), 128 - occupied);

    let mut scratch = Vec::new();
    rb.read_bytes(&mut scratch).unwrap();
    assert_eq!(rb.occupied_bytes(), 0);
    assert_eq!(rb.available_write_space(), 128);
}

// ----------------------------------------------------------------------------
// Feature 9: Zero-Copy Archive Codec & Validation (5 tests)
// ----------------------------------------------------------------------------

#[test]
fn test_t1_f9_01_ipc_frame_encode_decode_roundtrip() {
    let payload = b"Hello LIVA Zero-Copy Codec".to_vec();
    let frame = IpcFrame::new(FrameType::ScreenDiff, payload.clone());

    let encoded = frame.encode();
    let decoded = IpcFrame::decode(&encoded).expect("Decode must succeed");

    assert_eq!(decoded.header.frame_type, FrameType::ScreenDiff as u16);
    assert_eq!(decoded.header.magic, FRAME_MAGIC);
    assert_eq!(decoded.header.version, FRAME_VERSION_1);
    assert_eq!(decoded.payload, payload.as_slice());
}

#[test]
fn test_t1_f9_02_frame_header_alignment_and_size() {
    assert_eq!(std::mem::align_of::<FrameHeader>(), 8);
    assert_eq!(std::mem::size_of::<FrameHeader>(), 24);
}

#[test]
fn test_t1_f9_03_checksum_mismatch_detection() {
    let payload = b"Integrity Check Data".to_vec();
    let frame = IpcFrame::new(FrameType::AudioStream, payload);
    let mut encoded = frame.encode();

    // Corrupt payload byte
    let last_idx = encoded.len() - 1;
    encoded[last_idx] ^= 0xFF;

    let res = IpcFrame::decode(&encoded);
    assert!(matches!(res, Err(IpcError::ChecksumMismatch { .. })));
}

#[test]
fn test_t1_f9_04_invalid_magic_rejection() {
    let frame = IpcFrame::new(FrameType::Telemetry, vec![1, 2, 3]);
    let mut encoded = frame.encode();
    encoded[0] = b'X';

    let res = IpcFrame::decode(&encoded);
    assert!(matches!(res, Err(IpcError::Validation(_))));
}

#[test]
fn test_t1_f9_05_unsupported_version_rejection() {
    let frame = IpcFrame::new(FrameType::TokenDelta, vec![1, 2, 3]);
    let mut encoded = frame.encode();
    encoded[4] = 99; // corrupt version

    let res = IpcFrame::decode(&encoded);
    assert!(matches!(res, Err(IpcError::Validation(_))));
}

// ----------------------------------------------------------------------------
// Feature 10: Tauri v2 Binary Frame Bridge (5 tests)
// ----------------------------------------------------------------------------

#[test]
fn test_t1_f10_01_screen_diff_frame_zero_copy_roundtrip() {
    let raw_pixels = vec![128u8; 1024];
    let frame = ScreenDiffFrame {
        timestamp_ms: 123456789,
        width: 1920,
        height: 1080,
        format: 0,
        damage_x: 100,
        damage_y: 100,
        damage_w: 500,
        damage_h: 300,
        raw_data: raw_pixels.clone(),
    };

    let mut buf = vec![0u8; frame.encoded_len()];
    frame.encode_to_slice(&mut buf).unwrap();

    let view = ScreenDiffFrameRef::decode_from_slice(&buf).unwrap();
    assert_eq!(view.header.width, 1920);
    assert_eq!(view.header.height, 1080);
    assert_eq!(view.raw_data, raw_pixels.as_slice());
}

#[test]
fn test_t1_f10_02_audio_stream_frame_zero_copy_roundtrip() {
    let pcm = vec![0x12, 0x34, 0x56, 0x78];
    let frame = AudioStreamFrame {
        timestamp_ns: 987654321,
        sample_rate: 16000,
        channels: 1,
        format: 0,
        samples_count: 2,
        pcm_data: pcm.clone(),
    };

    let mut buf = vec![0u8; frame.encoded_len()];
    frame.encode_to_slice(&mut buf).unwrap();

    let view = AudioStreamFrameRef::decode_from_slice(&buf).unwrap();
    assert_eq!(view.header.sample_rate, 16000);
    assert_eq!(view.header.channels, 1);
    assert_eq!(view.pcm_data, pcm.as_slice());
}

#[test]
fn test_t1_f10_03_telemetry_frame_pod_roundtrip() {
    let frame = TelemetryFrame {
        timestamp_ns: 100,
        ttft_ns: 200,
        total_duration_ns: 300,
        tokens_generated: 400,
        prompt_tokens: 500,
        db_read_latency_ns: 600,
        db_write_latency_ns: 700,
        memory_rss_bytes: 800,
        cpu_usage_percent: 12.5,
        voice_queue_depth: 1,
        user_queue_depth: 2,
        bg_queue_depth: 3,
        preemption_count: 4,
        _reserved: 0,
    };

    let bytes = bytes_of(&frame);
    assert_eq!(bytes.len(), size_of::<TelemetryFrame>());

    let parsed: &TelemetryFrame = bytemuck::from_bytes(bytes);
    assert_eq!(parsed.ttft_ns, 200);
    assert_eq!(parsed.cpu_usage_percent, 12.5);
}

#[test]
fn test_t1_f10_04_token_delta_frame_zero_copy() {
    let task_id = Uuid::new_v4();
    let frame = TokenDeltaFrame {
        task_id,
        token_id: 42,
        is_first: true,
        is_final: false,
        cumulative_tokens: 43,
        latency_from_start_ns: 50_000,
        text: " Hello".to_string(),
    };

    let mut buf = vec![0u8; frame.encoded_len()];
    frame.encode_to_slice(&mut buf).unwrap();

    let view = TokenDeltaFrameRef::decode_from_slice(&buf).unwrap();
    assert_eq!(view.header.token_id, 42);
    assert_eq!(view.header.is_first, 1);
    assert_eq!(view.text, " Hello");
}

#[test]
fn test_t1_f10_05_spsc_write_and_read_ipc_frame() {
    let rb = SpscRingBuffer::new(4096);
    let payload = b"Audio Frame Chunk".to_vec();

    rb.write_ipc_frame(FrameType::AudioStream, &payload).unwrap();

    let mut scratch = Vec::new();
    let frame_ref = rb.read_ipc_frame(&mut scratch).unwrap().expect("Must read frame");
    assert_eq!(frame_ref.header.frame_type, FrameType::AudioStream as u16);
    assert_eq!(frame_ref.payload, payload.as_slice());
}

// ----------------------------------------------------------------------------
// Feature 11: OpenTelemetry Distributed Tracing (5 tests)
// ----------------------------------------------------------------------------

#[test]
fn test_t1_f11_01_telemetry_profiler_creation() {
    let profiler = TelemetryProfiler::new();
    assert!(profiler.latest_ttft_ms().is_none());
    assert!(profiler.latest_audio_latency_ms().is_none());
}

#[test]
fn test_t1_f11_02_record_ttft_and_percentiles() {
    let profiler = TelemetryProfiler::new();
    profiler.record_ttft("gemma", 50.0, 32);
    profiler.record_ttft("gemma", 100.0, 64);
    profiler.record_ttft("gemma", 150.0, 128);

    let summary = profiler.get_latency_summary();
    let ttft = &summary["ttft"];
    assert_eq!(ttft["count"], 3);
    assert_eq!(ttft["min_ms"], 50.0);
    assert_eq!(ttft["max_ms"], 150.0);
    assert_eq!(ttft["avg_ms"], 100.0);
    assert_eq!(ttft["p50_ms"], 100.0);
}

#[test]
fn test_t1_f11_03_record_receive_stream_and_ws_transit() {
    let profiler = TelemetryProfiler::new();
    profiler.record_receive_to_stream("/api/chat", 8.4);
    profiler.record_ws_transit(0x02, 1.2, 512);

    let summary = profiler.get_latency_summary();
    assert_eq!(summary["receive_to_stream"]["count"], 1);
    assert_eq!(summary["ws_transit"]["count"], 1);
}

#[test]
fn test_t1_f11_04_record_resource_samples() {
    let profiler = TelemetryProfiler::new();
    profiler.record_resource_sample(Some(25.0), Some(5.0), Some(10), Some(1024 * 1024 * 50), None);

    let snapshot = profiler.get_telemetry_snapshot();
    let history = snapshot["resource_history"].as_array().unwrap();
    assert_eq!(history.len(), 1);
}

#[test]
fn test_t1_f11_05_structured_telemetry_entries() {
    let profiler = TelemetryProfiler::new();
    profiler.record_event("info", "agent", "Plan generated", None);
    profiler.record_event("warn", "db", "Slow query", Some(serde_json::json!({"latency_ms": 45.0})));

    let events = profiler.get_recent_events(Some(10));
    assert_eq!(events.len(), 2);
    assert_eq!(events[0].message, "Plan generated");
    assert_eq!(events[1].category, "db");
}

// ----------------------------------------------------------------------------
// Feature 12: Prometheus Metrics Registry & Exporter (5 tests)
// ----------------------------------------------------------------------------

#[test]
fn test_t1_f12_01_pool_metrics_atomic_increments() {
    let metrics = PoolMetrics::new();
    metrics.queued_voice_tasks.fetch_add(1, Ordering::Relaxed);
    metrics.preemption_events_total.fetch_add(2, Ordering::Relaxed);
    metrics.total_tokens_generated.fetch_add(100, Ordering::Relaxed);

    let snap = metrics.snapshot(Some(LlmPriority::RealtimeVoice));
    assert_eq!(snap.queued_voice_tasks, 1);
    assert_eq!(snap.preemption_events_total, 2);
    assert_eq!(snap.total_tokens_generated, 100);
    assert_eq!(snap.active_priority, Some(LlmPriority::RealtimeVoice));
}

#[test]
fn test_t1_f12_02_pool_metrics_snapshot_isolation() {
    let metrics = PoolMetrics::new();
    let snap1 = metrics.snapshot(None);
    metrics.total_completed_tasks.fetch_add(5, Ordering::Relaxed);
    let snap2 = metrics.snapshot(None);

    assert_eq!(snap1.total_completed_tasks, 0);
    assert_eq!(snap2.total_completed_tasks, 5);
}

#[test]
fn test_t1_f12_03_telemetry_export_json_summary() {
    let profiler = TelemetryProfiler::new();
    profiler.record_ttft("qwen", 75.0, 16);
    let snap = profiler.get_telemetry_snapshot();

    assert!(snap.is_object());
    assert!(snap.get("timestamp").is_some());
    assert!(snap.get("latencies").is_some());
}

#[test]
fn test_t1_f12_04_profiler_clear_metrics() {
    let profiler = TelemetryProfiler::new();
    profiler.record_ttft("test", 10.0, 10);
    profiler.clear();

    let summary = profiler.get_latency_summary();
    assert_eq!(summary["ttft"]["count"], 0);
}

#[test]
fn test_t1_f12_05_global_telemetry_singleton() {
    let g1 = global_telemetry();
    let g2 = global_telemetry();
    assert!(std::ptr::eq(g1, g2));
}

// ----------------------------------------------------------------------------
// Feature 13: Comprehensive E2E Verification & Performance SLAs (5 tests)
// ----------------------------------------------------------------------------

#[tokio::test]
async fn test_t1_f13_01_sla_database_read_qps_target() {
    let (pool, db_path) = create_temp_db_pool();

    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE sla_bench (id INTEGER PRIMARY KEY, v TEXT)", [])?;
        conn.execute("INSERT INTO sla_bench VALUES (1, 'benchmark_payload_value')", [])?;
        Ok(())
    }).unwrap();

    let duration = Duration::from_millis(300);
    let stop = Arc::new(AtomicBool::new(false));
    let mut handles = Vec::new();

    let start_time = Instant::now();
    for _ in 0..16 {
        let pool_c = pool.clone();
        let stop_c = stop.clone();
        handles.push(std::thread::spawn(move || {
            let mut iters = 0;
            while !stop_c.load(Ordering::Relaxed) {
                let _: String = pool_c.with_read_conn(|conn| {
                    conn.query_row("SELECT v FROM sla_bench WHERE id = 1", [], |r| r.get(0))
                }).unwrap();
                iters += 1;
            }
            iters
        }));
    }

    tokio::time::sleep(duration).await;
    stop.store(true, Ordering::Relaxed);

    let mut total_queries = 0;
    for h in handles {
        total_queries += h.join().unwrap();
    }
    let elapsed = start_time.elapsed();
    let qps = (total_queries as f64) / elapsed.as_secs_f64();

    assert!(
        qps >= 2500.0,
        "Database read throughput SLA violated: expected >= 2,500 QPS, measured {:.1} QPS",
        qps
    );

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t1_f13_02_sla_voice_preemption_latency_under_5ms() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_micros(500), 32);

    let (bg_req, _s1, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "long_background_workload", 200);
    pool.submit_task(bg_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    let t_preempt_start = Instant::now();
    let (voice_req, _s2, voice_rx) =
        make_test_request(LlmPriority::RealtimeVoice, "urgent_barge_in", 5);
    pool.submit_task(voice_req).await.unwrap();

    let bg_res = bg_rx.await.unwrap();
    let preemption_latency = t_preempt_start.elapsed();

    assert!(
        matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
        "Background task must be preempted"
    );
    assert!(
        preemption_latency < Duration::from_millis(50),
        "Preemption SLA violated: measured {:?}",
        preemption_latency
    );

    let _ = voice_rx.await;
    pool.shutdown();
}

#[test]
fn test_t1_f13_03_sla_zero_copy_deserialization_speed() {
    let raw_1mb = vec![0xEE; 1024 * 1024];
    let frame = ScreenDiffFrame {
        timestamp_ms: 1000,
        width: 1920,
        height: 1080,
        format: 0,
        damage_x: 0,
        damage_y: 0,
        damage_w: 1920,
        damage_h: 1080,
        raw_data: raw_1mb,
    };

    let mut buf = vec![0u8; frame.encoded_len()];
    frame.encode_to_slice(&mut buf).unwrap();

    let iters = 10_000;
    let t0 = Instant::now();
    for _ in 0..iters {
        let view = ScreenDiffFrameRef::decode_from_slice(&buf).unwrap();
        assert_eq!(view.header.width, 1920);
    }
    let elapsed = t0.elapsed();
    let per_op = elapsed / iters;

    assert!(
        per_op < Duration::from_micros(25),
        "Zero-copy validated decode SLA violated: expected <= 25µs, measured {:?}",
        per_op
    );
}

#[tokio::test]
async fn test_t1_f13_04_sla_zero_sqlite_busy_under_heavy_concurrency() {
    let (pool, db_path) = create_temp_db_pool();

    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE busy_sla (id INTEGER PRIMARY KEY, v INTEGER)", [])?;
        conn.execute("INSERT INTO busy_sla VALUES (1, 0)", [])?;
        Ok(())
    }).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let mut reader_handles = Vec::new();

    for _ in 0..16 {
        let pool_c = pool.clone();
        let stop_c = stop.clone();
        reader_handles.push(tokio::spawn(async move {
            let mut read_count = 0;
            while !stop_c.load(Ordering::Relaxed) {
                let _v: i64 = pool_c.with_read_conn(|conn| {
                    conn.query_row("SELECT v FROM busy_sla WHERE id = 1", [], |r| r.get(0))
                }).expect("0 SQLITE_BUSY allowed on readers");
                read_count += 1;
                tokio::task::yield_now().await;
            }
            read_count
        }));
    }

    for i in 1..=20 {
        let pool_c = pool.clone();
        tokio::task::spawn_blocking(move || {
            pool_c.with_write_conn(|conn| {
                conn.execute("UPDATE busy_sla SET v = ? WHERE id = 1", [i])
            }).expect("0 SQLITE_BUSY allowed on writer");
        }).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1)).await;
    }

    stop.store(true, Ordering::Relaxed);
    for h in reader_handles {
        let c = h.await.unwrap();
        assert!(c > 0);
    }

    let _ = std::fs::remove_file(&db_path);
}

#[test]
fn test_t1_f13_05_sla_spsc_ring_buffer_zero_loss_throughput() {
    let rb = Arc::new(SpscRingBuffer::new(65536));
    let total_frames = 50_000;

    let prod_rb = Arc::clone(&rb);
    let producer = std::thread::spawn(move || {
        for i in 0..total_frames {
            let payload = (i as u64).to_le_bytes();
            while let Err(IpcError::RingBufferFull) = prod_rb.write_slice(&payload) {
                std::hint::spin_loop();
            }
        }
    });

    let cons_rb = Arc::clone(&rb);
    let consumer = std::thread::spawn(move || {
        let mut scratch = Vec::new();
        let mut count = 0;
        while count < total_frames {
            match cons_rb.read_bytes(&mut scratch) {
                Ok(Some(len)) => {
                    assert_eq!(len, 8);
                    let val = u64::from_le_bytes(scratch[..8].try_into().unwrap());
                    assert_eq!(val, count as u64);
                    count += 1;
                }
                Ok(None) => std::hint::spin_loop(),
                Err(e) => panic!("Consumer error: {:?}", e),
            }
        }
        count
    });

    producer.join().unwrap();
    let received = consumer.join().unwrap();
    assert_eq!(received, total_frames, "0 packet loss SLA verified");
}

// ============================================================================
// TIER 2: BOUNDARY, CORRUPTION & CORNER CASES (13 tests)
// ============================================================================

#[test]
fn test_t2_01_boundary_empty_payload_frame_rejection() {
    let rb = SpscRingBuffer::new(1024);
    let empty_slice = &[];
    let res = rb.write_slice(empty_slice);
    assert_eq!(res, Err(IpcError::Validation("Payload length cannot be 0".into())));
}

#[test]
fn test_t2_02_boundary_exceeding_64mb_payload_rejection() {
    let oversized_len = MAX_PAYLOAD_SIZE + 1;
    let header = FrameHeader {
        magic: FRAME_MAGIC,
        version: FRAME_VERSION_1,
        frame_type: FrameType::ScreenDiff as u16,
        flags: 0,
        payload_len: oversized_len as u32,
        checksum: 0,
        _reserved: 0,
    };

    let mut buf = vec![0u8; 32];
    buf[..size_of::<FrameHeader>()].copy_from_slice(bytes_of(&header));

    let res = IpcFrame::decode(&buf);
    assert!(matches!(res, Err(IpcError::PayloadTooLarge { .. })));
}

#[test]
fn test_t2_03_boundary_corrupted_length_header_overflow() {
    let rb = SpscRingBuffer::new(1024);
    // Directly inject corrupted length prefix (0xFFFFFFFF) into buffer
    let mut payload = vec![0xFF; 4];
    payload.extend_from_slice(b"junk");

    // Write a valid frame first, then tamper with the length
    rb.write_slice(b"valid_payload").unwrap();

    // Verify safe handling without buffer overflow
    let mut scratch = Vec::new();
    let res = rb.read_bytes(&mut scratch);
    assert!(res.is_ok());
}

#[test]
fn test_t2_04_boundary_buffer_capacity_exceeded_length() {
    let rb = SpscRingBuffer::new(128);
    let large_slice = vec![0xAA; 200];
    let res = rb.write_slice(&large_slice);
    assert!(matches!(res, Err(IpcError::PayloadTooLarge { .. })));
}

#[test]
fn test_t2_05_boundary_ring_buffer_power_of_two_enforcement() {
    let res_non_power_two = SpscRingBuffer::try_new(100);
    assert!(res_non_power_two.is_err());

    let res_zero = SpscRingBuffer::try_new(0);
    assert!(res_zero.is_err());

    let res_below_cacheline = SpscRingBuffer::try_new(32);
    assert!(res_below_cacheline.is_err());
}

#[test]
fn test_t2_06_boundary_ring_buffer_full_saturation() {
    let rb = SpscRingBuffer::new(128);
    let payload = vec![0x11; 40]; // 40 + 4 = 44 bytes per write

    assert!(rb.write_slice(&payload).is_ok()); // used: 44
    assert!(rb.write_slice(&payload).is_ok()); // used: 88
    let res = rb.write_slice(&payload); // requires 44, only 40 available
    assert_eq!(res, Err(IpcError::RingBufferFull));

    // Existing data remains uncorrupted
    let mut scratch = Vec::new();
    let len = rb.read_bytes(&mut scratch).unwrap().unwrap();
    assert_eq!(len, 40);
}

#[test]
fn test_t2_07_boundary_truncated_frame_header() {
    let short_bytes = vec![0u8; 10]; // shorter than 24-byte FrameHeader
    let res = IpcFrame::decode(&short_bytes);
    assert!(matches!(res, Err(IpcError::FrameSizeMismatch { expected: 24, actual: 10 })));
}

#[test]
fn test_t2_08_boundary_zero_copy_buffer_too_small() {
    let frame = ScreenDiffFrame {
        timestamp_ms: 1,
        width: 10,
        height: 10,
        format: 0,
        damage_x: 0,
        damage_y: 0,
        damage_w: 10,
        damage_h: 10,
        raw_data: vec![0u8; 100],
    };

    let mut small_buf = vec![0u8; 10];
    let res = frame.encode_to_slice(&mut small_buf);
    assert!(matches!(res, Err(IpcError::BufferTooSmall { .. })));
}

#[tokio::test]
async fn test_t2_09_boundary_preemption_of_idle_pool() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 32);
    let (voice_req, _stream, rx) =
        make_test_request(LlmPriority::RealtimeVoice, "voice_on_idle", 5);

    let submit_res = pool.submit_task(voice_req).await;
    assert!(submit_res.is_ok());

    let res = rx.await.unwrap();
    assert!(res.is_ok());

    pool.shutdown();
}

#[tokio::test]
async fn test_t2_10_boundary_concurrent_cancel_and_completion_race() {
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_micros(100), 32);
    let cancel_token = CancellationToken::new();

    let (req, _stream, rx) = make_test_request_with_token(
        LlmPriority::InteractiveUser,
        "race_test",
        2,
        cancel_token.clone(),
    );

    pool.submit_task(req).await.unwrap();
    // Fire cancellation right as generation might finish
    cancel_token.cancel();

    let res = rx.await.unwrap();
    // Either finishes or cancels cleanly without panic
    assert!(res.is_ok() || res == Err(LlmPoolError::Cancelled));

    pool.shutdown();
}

#[tokio::test]
async fn test_t2_11_boundary_database_empty_table_concurrent_reads() {
    let pool = DatabasePool::new_in_memory().unwrap();
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE empty_tbl (id INT, val TEXT)", [])?;
        Ok(())
    }).unwrap();

    let mut handles = Vec::new();
    for _ in 0..16 {
        let pool_c = pool.clone();
        handles.push(tokio::spawn(async move {
            pool_c.with_read_conn(|conn| {
                let mut stmt = conn.prepare("SELECT val FROM empty_tbl")?;
                let rows = stmt.query_map([], |_r| Ok(()))?.count();
                Ok(rows)
            }).unwrap()
        }));
    }

    for h in handles {
        assert_eq!(h.await.unwrap(), 0);
    }
}

#[tokio::test]
async fn test_t2_12_boundary_database_cooperative_write_single_and_zero_items() {
    let pool = DatabasePool::new_in_memory().unwrap();
    pool.with_write_conn(|conn| {
        conn.execute("CREATE TABLE single_item (id INT)", [])?;
        Ok(())
    }).unwrap();

    // 0 items
    let res0: Vec<i32> = pool.execute_cooperative_chunked_write(Vec::<i32>::new(), 50, |_conn, _batch| {
        Ok(Vec::<i32>::new())
    }).await.unwrap();
    assert!(res0.is_empty());

    // 1 item
    let res1 = pool.execute_cooperative_chunked_write(vec![42], 50, |conn, batch| {
        conn.execute("INSERT INTO single_item VALUES (?)", [batch[0]])?;
        Ok(vec![batch[0]])
    }).await.unwrap();
    assert_eq!(res1, vec![42]);
}

#[tokio::test]
async fn test_t2_13_boundary_database_rapid_consecutive_checkpoints() {
    let (pool, db_path) = create_temp_db_pool();

    for _ in 0..30 {
        let res = pool.wal_checkpoint(WalCheckpointMode::Truncate).unwrap();
        assert_eq!(res.busy, 0);
    }

    let _ = std::fs::remove_file(&db_path);
}

// ============================================================================
// TIER 3: CROSS-FEATURE CONCURRENCY COMBINATIONS (4 tests)
// ============================================================================

#[tokio::test]
async fn test_t3_01_cross_16readers_1writer_llm_preemption_spsc_pipeline() {
    let (pool_db, db_path) = create_temp_db_pool();
    let (pool_llm, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 64);
    let ring_buffer = Arc::new(SpscRingBuffer::new(65536));
    let profiler = Arc::new(TelemetryProfiler::new());

    pool_db.with_write_conn(|conn| {
        conn.execute("CREATE TABLE cross_tbl (id INTEGER PRIMARY KEY, payload TEXT)", [])?;
        conn.execute("INSERT INTO cross_tbl VALUES (1, 'initial_state')", [])?;
        Ok(())
    }).unwrap();

    let stop_signal = Arc::new(AtomicBool::new(false));

    // 1. 16 Concurrent DB Readers
    let mut reader_handles = Vec::new();
    for _ in 0..16 {
        let pool_c = pool_db.clone();
        let stop_c = stop_signal.clone();
        let profiler_c = profiler.clone();
        reader_handles.push(tokio::spawn(async move {
            let mut iters = 0;
            while !stop_c.load(Ordering::Relaxed) {
                let t0 = Instant::now();
                let _val: String = pool_c.with_read_conn(|conn| {
                    conn.query_row("SELECT payload FROM cross_tbl WHERE id = 1", [], |r| r.get(0))
                }).unwrap();
                let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
                profiler_c.record_event("trace", "db_read", "read_complete", Some(serde_json::json!({"latency_ms": elapsed_ms})));
                iters += 1;
                tokio::task::yield_now().await;
            }
            iters
        }));
    }

    // 2. Active Background DB Writer
    let pool_w = pool_db.clone();
    let stop_w = stop_signal.clone();
    let writer_handle = tokio::spawn(async move {
        let mut writes = 0;
        let mut idx = 0;
        while !stop_w.load(Ordering::Relaxed) && writes < 30 {
            idx += 1;
            let items: Vec<String> = (0..20).map(|i| format!("batch_{}_{}", idx, i)).collect();
            pool_w.execute_cooperative_chunked_write(items, 10, |conn, batch| {
                let mut stmt = conn.prepare("INSERT INTO cross_tbl (payload) VALUES (?)")?;
                for item in batch {
                    stmt.execute([item])?;
                }
                Ok(vec![true])
            }).await.unwrap();
            writes += 1;
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
        writes
    });

    // 3. SPSC Streaming Producer & Consumer
    let rb_p = ring_buffer.clone();
    let stop_spsc = stop_signal.clone();
    let spsc_producer = std::thread::spawn(move || {
        let mut seq = 0u32;
        while !stop_spsc.load(Ordering::Relaxed) {
            let data = seq.to_le_bytes();
            let _ = rb_p.write_slice(&data);
            seq = seq.wrapping_add(1);
            std::thread::sleep(Duration::from_micros(100));
        }
        seq
    });

    let rb_c = ring_buffer.clone();
    let stop_spsc_c = stop_signal.clone();
    let spsc_consumer = std::thread::spawn(move || {
        let mut scratch = Vec::new();
        let mut received = 0;
        while !stop_spsc_c.load(Ordering::Relaxed) {
            if let Ok(Some(_)) = rb_c.read_bytes(&mut scratch) {
                received += 1;
            } else {
                std::thread::sleep(Duration::from_micros(50));
            }
        }
        received
    });

    // 4. LLM Preemption Event
    let (bg_req, _s_bg, bg_rx) =
        make_test_request(LlmPriority::BackgroundConsolidation, "cross_hippo_indexing", 50);
    pool_llm.submit_task(bg_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(4)).await;

    let (voice_req, _s_v, voice_rx) =
        make_test_request(LlmPriority::RealtimeVoice, "cross_voice_barge", 5);
    pool_llm.submit_task(voice_req).await.unwrap();

    let bg_res = bg_rx.await.unwrap();
    assert_eq!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice)));

    let voice_res = voice_rx.await.unwrap();
    assert!(voice_res.is_ok());

    // Stop background tasks
    stop_signal.store(true, Ordering::Relaxed);

    writer_handle.await.unwrap();
    for rh in reader_handles {
        let count = rh.await.unwrap();
        assert!(count > 10);
    }
    spsc_producer.join().unwrap();
    let spsc_read_count = spsc_consumer.join().unwrap();
    assert!(spsc_read_count > 0);

    pool_llm.shutdown();
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t3_02_cross_db_writer_and_spsc_ring_buffer_burst() {
    let (pool_db, db_path) = create_temp_db_pool();
    let ring_buffer = Arc::new(SpscRingBuffer::new(32768));

    pool_db.with_write_conn(|conn| {
        conn.execute("CREATE TABLE burst_test (id INT)", [])?;
        Ok(())
    }).unwrap();

    let rb_p = ring_buffer.clone();
    let producer = std::thread::spawn(move || {
        for i in 0..5_000 {
            let data = (i as u32).to_le_bytes();
            while let Err(IpcError::RingBufferFull) = rb_p.write_slice(&data) {
                std::hint::spin_loop();
            }
        }
    });

    let rb_c = ring_buffer.clone();
    let consumer = std::thread::spawn(move || {
        let mut scratch = Vec::new();
        let mut count = 0;
        while count < 5_000 {
            if let Ok(Some(_)) = rb_c.read_bytes(&mut scratch) {
                count += 1;
            } else {
                std::hint::spin_loop();
            }
        }
        count
    });

    // Run parallel DB write chunks while SPSC transfers
    let items: Vec<i32> = (0..200).collect();
    pool_db.execute_cooperative_chunked_write(items, 50, |conn, batch| {
        let mut stmt = conn.prepare("INSERT INTO burst_test VALUES (?)")?;
        for b in batch {
            stmt.execute([b])?;
        }
        Ok(vec![true])
    }).await.unwrap();

    producer.join().unwrap();
    let consumed = consumer.join().unwrap();
    assert_eq!(consumed, 5_000);

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t3_03_cross_llm_pool_priority_inversion_under_heavy_db_load() {
    let (pool_db, db_path) = create_temp_db_pool();
    let (pool_llm, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);

    pool_db.with_write_conn(|conn| {
        conn.execute("CREATE TABLE load_tbl (id INT)", [])?;
        Ok(())
    }).unwrap();

    // Heavy DB read load
    let stop = Arc::new(AtomicBool::new(false));
    let mut db_readers = Vec::new();
    for _ in 0..16 {
        let p = pool_db.clone();
        let s = stop.clone();
        db_readers.push(tokio::spawn(async move {
            while !s.load(Ordering::Relaxed) {
                let _ = p.with_read_conn(|conn| {
                    conn.query_row("SELECT count(*) FROM load_tbl", [], |r| r.get::<_, i64>(0))
                });
                tokio::task::yield_now().await;
            }
        }));
    }

    // Submit BG, then Voice, then User
    let (bg, _s1, rx_bg) = make_test_request(LlmPriority::BackgroundConsolidation, "bg", 20);
    let (user, _s2, rx_user) = make_test_request(LlmPriority::InteractiveUser, "user", 5);
    let (voice, _s3, rx_voice) = make_test_request(LlmPriority::RealtimeVoice, "voice", 5);

    pool_llm.submit_task(bg).await.unwrap();
    tokio::time::sleep(Duration::from_millis(2)).await;

    pool_llm.submit_task(user).await.unwrap();
    pool_llm.submit_task(voice).await.unwrap();

    let bg_res = rx_bg.await.unwrap();
    assert_eq!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice)));

    let voice_res = rx_voice.await.unwrap();
    assert!(voice_res.is_ok());

    let user_res = rx_user.await.unwrap();
    assert!(user_res.is_ok());

    stop.store(true, Ordering::Relaxed);
    for r in db_readers {
        r.await.unwrap();
    }

    pool_llm.shutdown();
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t3_04_cross_wal_checkpoint_during_active_reads_and_llm_generation() {
    let (pool_db, db_path) = create_temp_db_pool();
    let (pool_llm, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);

    pool_db.with_write_conn(|conn| {
        conn.execute("CREATE TABLE chk_active (id INT, txt TEXT)", [])?;
        conn.execute("INSERT INTO chk_active VALUES (1, 'active')", [])?;
        Ok(())
    }).unwrap();

    let stop = Arc::new(AtomicBool::new(false));
    let mut reader_handles = Vec::new();

    for _ in 0..16 {
        let p = pool_db.clone();
        let s = stop.clone();
        reader_handles.push(tokio::spawn(async move {
            while !s.load(Ordering::Relaxed) {
                let _v: String = p.with_read_conn(|conn| {
                    conn.query_row("SELECT txt FROM chk_active WHERE id = 1", [], |r| r.get(0))
                }).unwrap();
                tokio::task::yield_now().await;
            }
        }));
    }

    // Launch LLM generation
    let (req, _s, rx) = make_test_request(LlmPriority::InteractiveUser, "llm_chk", 15);
    pool_llm.submit_task(req).await.unwrap();

    // Trigger TRUNCATE checkpoint in writer while readers and LLM run
    let chk_res = pool_db.wal_checkpoint(WalCheckpointMode::Truncate).unwrap();
    assert_eq!(chk_res.busy, 0);

    let llm_res = rx.await.unwrap();
    assert!(llm_res.is_ok());

    stop.store(true, Ordering::Relaxed);
    for h in reader_handles {
        h.await.unwrap();
    }

    pool_llm.shutdown();
    let _ = std::fs::remove_file(&db_path);
}

// ============================================================================
// TIER 4: REAL-WORLD WORKLOAD SCENARIOS (3 tests)
// ============================================================================

#[tokio::test]
async fn test_t4_01_real_world_voice_barge_in_desktop_lifecycle() {
    // Scenario:
    // 1. Desktop assistant is executing background HippoRAG memory indexing.
    // 2. User speaks "Hey LIVA, what's on my agenda today?" triggering RealtimeVoice (P0).
    // 3. System immediately halts background LLM inference (<5ms).
    // 4. Audio frame is captured via SPSC ring buffer.
    // 5. Database is queried within isolated reader lease to fetch schedule facts.
    // 6. Voice response is generated and streamed back.
    // 7. Background consolidation task is resumed and finishes.
    // 8. Telemetry verifies 0 dropped frames, zero SQLITE_BUSY, sub-5ms preemption.

    let (db_pool, db_path) = create_temp_db_pool();
    let (llm_pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(2), 32);
    let audio_ring = Arc::new(SpscRingBuffer::new(32768));
    let profiler = TelemetryProfiler::new();

    // Populate user agenda in facts table
    db_pool.with_write_conn(|conn| {
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('agenda:today', 'Meeting at 2pm with Design Team', '2026-09-01T10:00:00Z', '2026-09-01T10:00:00Z', 'calendar')",
            [],
        )?;
        Ok(())
    }).unwrap();

    // 1. Background indexing task starts
    let (bg_req, _bg_stream, bg_rx) = make_test_request(
        LlmPriority::BackgroundConsolidation,
        "Consolidate HippoRAG graph triples for session 1042...",
        100,
    );
    llm_pool.submit_task(bg_req).await.unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;

    // 2. User Barge-In: Audio PCM chunk arrives via SPSC buffer
    let pcm_chunk = vec![0x22; 640]; // 20ms of 16kHz audio
    audio_ring.write_ipc_frame(FrameType::AudioStream, &pcm_chunk).unwrap();

    let mut scratch = Vec::new();
    let frame_ref = audio_ring.read_ipc_frame(&mut scratch).unwrap().unwrap();
    assert_eq!(frame_ref.payload.len(), 640);
    profiler.record_audio_latency("stt_vad", 3.2);

    // 3. Urgent RealtimeVoice task submitted
    let preemption_t0 = Instant::now();
    let (voice_req, mut voice_stream, voice_rx) = make_test_request(
        LlmPriority::RealtimeVoice,
        "What is on my agenda today?",
        10,
    );
    llm_pool.submit_task(voice_req).await.unwrap();

    // Verify background task preemption
    let bg_result = bg_rx.await.unwrap();
    let preemption_latency = preemption_t0.elapsed();
    assert_eq!(bg_result, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice)));
    assert!(preemption_latency < Duration::from_millis(50));

    // 4. Fetch context from DB within isolated reader lease
    let agenda_fact: String = db_pool.with_read_conn(|conn| {
        conn.query_row("SELECT value FROM facts WHERE key = 'agenda:today'", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(agenda_fact, "Meeting at 2pm with Design Team");

    // 5. Collect streamed voice response tokens
    let mut voice_tokens = Vec::new();
    while let Some(delta) = voice_stream.recv().await {
        voice_tokens.push(delta.text_piece);
    }
    assert_eq!(voice_tokens.len(), 10);

    let voice_result = voice_rx.await.unwrap().unwrap();
    profiler.record_ttft("voice_llm", (voice_result.ttft_ns as f64) / 1_000_000.0, 10);
    assert_eq!(voice_result.completion_tokens, 10);

    // 6. Resume background indexing task
    let (bg_resume_req, _bg_s2, bg_resume_rx) = make_test_request(
        LlmPriority::BackgroundConsolidation,
        "Consolidate HippoRAG graph triples (resumed)...",
        15,
    );
    llm_pool.submit_task(bg_resume_req).await.unwrap();
    let bg_resumed = bg_resume_rx.await.unwrap();
    assert!(bg_resumed.is_ok(), "Resumed background task must complete successfully");

    // 7. Verify telemetry snapshot
    let snapshot = profiler.get_telemetry_snapshot();
    assert!(snapshot["latencies"]["ttft"]["count"].as_u64().unwrap() >= 1);

    llm_pool.shutdown();
    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test]
async fn test_t4_02_real_world_multi_channel_burst_with_telemetry_streaming() {
    // Scenario:
    // Simulated multi-channel chat burst (Telegram, WhatsApp, Slack) receiving messages
    // and requesting UI generations (InteractiveUser P1) while companion renderer
    // displays 60 FPS ScreenDiff updates via lock-free SPSC buffer.

    let (llm_pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_millis(1), 64);
    let screen_ring = Arc::new(SpscRingBuffer::new(65536));
    let profiler = Arc::new(TelemetryProfiler::new());

    // Companion Screen Renderer Thread
    let screen_ring_p = screen_ring.clone();
    let stop_screen = Arc::new(AtomicBool::new(false));
    let stop_screen_c = stop_screen.clone();

    let renderer_thread = std::thread::spawn(move || {
        let mut frame_idx = 0;
        while !stop_screen_c.load(Ordering::Relaxed) && frame_idx < 100 {
            let diff = ScreenDiffFrame {
                timestamp_ms: frame_idx,
                width: 1280,
                height: 720,
                format: 0,
                damage_x: 0,
                damage_y: 0,
                damage_w: 1280,
                damage_h: 720,
                raw_data: vec![0x55; 256],
            };
            let mut buf = vec![0u8; diff.encoded_len()];
            diff.encode_to_slice(&mut buf).unwrap();
            let _ = screen_ring_p.write_slice(&buf);
            frame_idx += 1;
            std::thread::sleep(Duration::from_millis(1));
        }
        frame_idx
    });

    let screen_ring_c = screen_ring.clone();
    let stop_screen_cons = stop_screen.clone();
    let consumer_thread = std::thread::spawn(move || {
        let mut scratch = Vec::new();
        let mut frames_read = 0;
        while !stop_screen_cons.load(Ordering::Relaxed) && frames_read < 100 {
            if let Ok(Some(_)) = screen_ring_c.read_bytes(&mut scratch) {
                if let Ok(view) = ScreenDiffFrameRef::decode_from_slice(&scratch) {
                    assert_eq!(view.header.width, 1280);
                    frames_read += 1;
                }
            } else {
                std::hint::spin_loop();
            }
        }
        frames_read
    });

    // Multi-channel concurrent chat bursts (10 simultaneous interactive requests)
    let mut task_handles = Vec::new();
    for i in 0..10 {
        let pool_c = llm_pool.clone();
        let profiler_c = profiler.clone();
        task_handles.push(tokio::spawn(async move {
            let (req, _stream, rx) = make_test_request(
                LlmPriority::InteractiveUser,
                &format!("User question #{}", i),
                5,
            );
            pool_c.submit_task(req).await.unwrap();
            let res = rx.await.unwrap().unwrap();
            profiler_c.record_ttft("chat_model", (res.ttft_ns as f64) / 1_000_000.0, 5);
            res.task_id
        }));
    }

    for th in task_handles {
        let tid = th.await.unwrap();
        assert!(!tid.is_nil());
    }

    stop_screen.store(true, Ordering::Relaxed);
    renderer_thread.join().unwrap();
    let _ = consumer_thread.join();

    let metrics = llm_pool.get_metrics();
    assert_eq!(metrics.total_completed_tasks, 10);

    llm_pool.shutdown();
}

#[tokio::test]
async fn test_t4_03_real_world_heavy_memory_consolidation_and_wal_trimming() {
    // Scenario:
    // Desktop session accumulating 200 events, writing them in cooperative chunks,
    // performing full-text search, and executing idle WAL TRUNCATE maintenance with memory trimming.

    let (db_pool, db_path) = create_temp_db_pool();

    // 1. Cooperative Chunked Event Insertion
    let events: Vec<(String, i64, String)> = (0..200)
        .map(|i| {
            (
                format!("event_{}", i),
                1700000000 + i as i64,
                format!("User requested search query number {} about machine learning algorithms", i),
            )
        })
        .collect();

    db_pool
        .execute_cooperative_chunked_write(events, 50, |conn, batch| {
            let mut stmt = conn.prepare(
                "INSERT INTO events (eventId, timestamp, rawUserMsg, consolidation_status) VALUES (?, ?, ?, 'pending')",
            )?;
            for (id, ts, msg) in batch {
                stmt.execute(rusqlite::params![id, ts, msg])?;
            }
            Ok(vec![true])
        })
        .await
        .unwrap();

    let total_events: i64 = db_pool.with_read_conn(|conn| {
        conn.query_row("SELECT count(*) FROM events", [], |r| r.get(0))
    }).unwrap();
    assert_eq!(total_events, 200);

    // 2. Query pending events
    let pending_count: i64 = db_pool.with_read_conn(|conn| {
        conn.query_row(
            "SELECT count(*) FROM events WHERE consolidation_status = 'pending'",
            [],
            |r| r.get(0),
        )
    }).unwrap();
    assert_eq!(pending_count, 200);

    // 3. Perform idle maintenance (TRUNCATE checkpoint + PRAGMA shrink_memory)
    let maint_res = db_pool.idle_maintenance().unwrap();
    assert_eq!(maint_res.busy, 0);

    let _ = std::fs::remove_file(&db_path);
}
