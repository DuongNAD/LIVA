//! Empirical Adversarial Verification Test Suite for Milestone 1 (M1)
//!
//! Areas verified:
//! 1. Buffer pool concurrency, rapid acquire/release churn, capacity limits & zero-leakage
//! 2. SQLite reader concurrency under heavy concurrent WAL checkpoints & writes on disk DB
//! 3. KV cache prefix reuse logic, token matching edge cases, arithmetic overflow & bounds
//! 4. Voice frame zero-copy encoding/decoding & compact streaming JSON format under stress

use bytes::{BufMut, BytesMut};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{
    DatabasePool, MetadataFilter, WalCheckpointMode, search_similar_vectors, upsert_vector,
};
use liva_native_core::llm::engine::{
    LlamaRouterManager, check_prompt_fits, compute_common_prefix_len,
};
use liva_native_core::webrtc::frame::{BufferPool, OP_SPEAKER_OUT, VoiceFrame};
use liva_native_core::websocket::format_ai_stream_chunk;
use llama_cpp_2::token::LlamaToken;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::time::timeout;

fn test_temp_dir() -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "liva_m1_adversarial_{}_{}",
        std::process::id(),
        rand::random::<u64>()
    ));
    std::fs::create_dir_all(&path).expect("create temp dir");
    path
}

// ============================================================================
// SECTION 1: Buffer Pool Stress, Concurrency, and Boundary Verification
// ============================================================================

#[tokio::test]
async fn test_adversarial_buffer_pool_high_concurrency_churn() {
    let pool = Arc::new(BufferPool::new(4096, 16));
    let num_tasks = 64;
    let iterations_per_task = 100;
    let total_operations = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(num_tasks);

    for task_id in 0..num_tasks {
        let pool_clone = Arc::clone(&pool);
        let ops_counter = Arc::clone(&total_operations);

        handles.push(tokio::spawn(async move {
            for iter in 0..iterations_per_task {
                let mut buf = pool_clone.acquire_buffer();

                // Verification 1: Re-acquired buffer must be clean / empty
                assert_eq!(
                    buf.len(),
                    0,
                    "Task {task_id} iter {iter}: Re-acquired buffer must have len 0!"
                );

                // Write varying payload sizes
                let payload_size = ((task_id * 31 + iter * 17) % 8192) + 1;
                let test_byte = ((task_id + iter) % 255) as u8;
                let payload = vec![test_byte; payload_size];
                buf.put_slice(&payload);
                assert_eq!(buf.len(), payload_size);
                assert_eq!(buf[0], test_byte);
                assert_eq!(buf[payload_size - 1], test_byte);

                // Alternate between into_bytes() and normal drop
                if iter % 2 == 0 {
                    let bytes = buf.into_bytes();
                    assert_eq!(bytes.len(), payload_size);
                    assert_eq!(bytes[0], test_byte);
                } // else: dropped and recycled to pool

                ops_counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    let join_all = async {
        for handle in handles {
            handle.await.expect("task must join cleanly");
        }
    };

    let result = timeout(Duration::from_secs(10), join_all).await;
    assert!(
        result.is_ok(),
        "CRITICAL: Buffer pool deadlocked or stalled under concurrent load!"
    );
    assert_eq!(
        total_operations.load(Ordering::Relaxed),
        num_tasks * iterations_per_task
    );

    // Idle count should never exceed max_idle (16)
    assert!(
        pool.idle_count() <= 16,
        "Buffer pool idle count {} exceeded max_idle 16",
        pool.idle_count()
    );
}

#[test]
fn test_adversarial_buffer_pool_oversized_allocation_drop() {
    let target_cap = 1024;
    let pool = BufferPool::new(target_cap, 4);

    // 1. Acquire and expand beyond 4x target capacity (> 4096 bytes)
    {
        let mut buf = pool.acquire_buffer();
        let huge_payload = vec![0xABu8; 10_000];
        buf.put_slice(&huge_payload);
        assert!(buf.capacity() >= 10_000);
        // When dropped, oversized buffer should NOT be returned to the pool
    }

    // Idle count must remain 0 because oversized buffer was discarded
    assert_eq!(
        pool.idle_count(),
        0,
        "Oversized buffer (>4x target capacity) must not be recycled into pool"
    );

    // 2. Normal sized buffer SHOULD be recycled
    {
        let mut normal_buf = pool.acquire_buffer();
        normal_buf.put_slice(&[1, 2, 3, 4]);
    }
    assert_eq!(pool.idle_count(), 1, "Normal buffer must be recycled");
}

#[tokio::test]
async fn test_adversarial_voice_frame_concurrent_streaming_encode_decode() {
    let pool = Arc::new(BufferPool::new(16 * 1024, 32));
    let num_workers = 32;
    let frames_per_worker = 50;

    let mut handles = Vec::new();

    for worker_id in 0..num_workers {
        let pool_clone = Arc::clone(&pool);
        handles.push(tokio::spawn(async move {
            for seq in 0..frames_per_worker {
                let payload_len = (seq * 64) % 4096;
                let payload: Vec<u8> = (0..payload_len).map(|i| (i ^ worker_id) as u8).collect();

                let mut pooled = pool_clone.acquire_buffer();
                VoiceFrame::encode_into(&mut pooled, OP_SPEAKER_OUT, seq as u32, &payload)
                    .expect("encode_into must succeed");

                let wire_bytes = pooled.into_bytes();
                assert_eq!(wire_bytes.len(), 9 + payload_len);

                // Decode and verify
                let mut decode_buf = BytesMut::from(&wire_bytes[..]);
                let decoded = VoiceFrame::decode(&mut decode_buf)
                    .expect("decode must succeed")
                    .expect("frame must be complete");

                assert_eq!(decoded.op_code, OP_SPEAKER_OUT);
                assert_eq!(decoded.seq_id, seq as u32);
                assert_eq!(&decoded.payload[..], &payload[..]);
                assert!(decode_buf.is_empty());
            }
        }));
    }

    for h in handles {
        h.await.expect("worker must complete");
    }
}

#[test]
fn test_adversarial_format_ai_stream_chunk_edge_cases() {
    // 1. Empty string
    let chunk_empty = format_ai_stream_chunk("", false).expect("empty chunk format");
    assert!(chunk_empty.contains("\"event\":\"ai_stream_chunk\""));
    assert!(chunk_empty.contains("\"textChunk\":\"\""));
    assert!(chunk_empty.contains("\"isThought\":false"));

    // 2. Special escaped characters (quotes, backslashes, newlines, control characters)
    let complex_text = "Line 1\nLine 2\t\"quoted text\" with \\ backslash and emoji: 🚀 🤖";
    let chunk_complex = format_ai_stream_chunk(complex_text, true).expect("complex chunk format");
    assert!(chunk_complex.contains("\"isThought\":true"));

    // Verify valid JSON round-trip
    let parsed: serde_json::Value =
        serde_json::from_str(&chunk_complex).expect("must be valid json");
    assert_eq!(parsed["event"], "ai_stream_chunk");
    assert_eq!(parsed["payload"]["textChunk"], complex_text);
    assert_eq!(parsed["payload"]["isThought"], true);

    // 3. Very large token chunk (100KB)
    let large_text = "A".repeat(100_000);
    let chunk_large = format_ai_stream_chunk(&large_text, false).expect("large chunk format");
    let parsed_large: serde_json::Value =
        serde_json::from_str(&chunk_large).expect("must be valid json");
    assert_eq!(
        parsed_large["payload"]["textChunk"].as_str().map(|s| s.len()),
        Some(100_000)
    );
}

// ============================================================================
// SECTION 2: SQLite Concurrency, WAL Checkpoints & Vector Search on Disk DB
// ============================================================================

#[tokio::test]
async fn test_adversarial_sqlite_disk_wal_checkpoint_and_heavy_reader_concurrency() {
    let temp_dir = test_temp_dir();
    let db_path = temp_dir.join("liva_stress_test.sqlite");

    let pool = Arc::new(DatabasePool::new(&db_path).expect("disk db pool initialize"));

    // Initial seed
    {
        let conn = pool.writer.get().expect("writer connection");
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) \
             VALUES ('init_key', 'init_val', '2026-09-01', '2026-09-01', 'system')",
            [],
        )
        .expect("initial insert");
    }

    let num_readers = 32;
    let reads_per_task = 50;
    let mut handles = Vec::new();

    // Group A: 32 Concurrent Readers (using spawn_blocking to properly handle synchronous r2d2 pool acquisitions)
    for reader_id in 0..num_readers {
        let pool_clone = Arc::clone(&pool);
        handles.push(tokio::spawn(async move {
            for iter in 0..reads_per_task {
                let pool_c = Arc::clone(&pool_clone);
                let count = tokio::task::spawn_blocking(move || {
                    let conn = pool_c.readers.get().expect("reader connection");
                    let count: i64 = conn
                        .query_row("SELECT count(*) FROM facts", [], |r| r.get(0))
                        .expect("reader query must succeed without SQLITE_BUSY");
                    count
                })
                .await
                .expect("reader task join");

                assert!(
                    count >= 1,
                    "Reader {reader_id} iter {iter}: count must be >= 1"
                );

                if iter % 10 == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
            }
        }));
    }

    // Group B: Concurrent Writer doing active updates and inserts
    let pool_writer = Arc::clone(&pool);
    let writer_handle = tokio::spawn(async move {
        for w in 0..30 {
            let pool_w = Arc::clone(&pool_writer);
            tokio::task::spawn_blocking(move || {
                let conn = pool_w.writer.get().expect("acquire writer");
                conn.execute(
                    "INSERT OR REPLACE INTO facts (key, value, createdAt, updatedAt, source) \
                     VALUES (?1, ?2, '2026-09-01', '2026-09-01', 'stress')",
                    rusqlite::params![format!("key_{w}"), format!("value_{w}")],
                )
                .expect("insert writer");
            })
            .await
            .expect("writer task join");

            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    });
    handles.push(writer_handle);

    // Group C: Aggressive Background WAL Checkpointer cycling through all modes
    let pool_checkpointer = Arc::clone(&pool);
    let checkpoint_handle = tokio::spawn(async move {
        let modes = [
            WalCheckpointMode::Passive,
            WalCheckpointMode::Full,
            WalCheckpointMode::Restart,
            WalCheckpointMode::Truncate,
        ];
        for (i, mode) in modes.iter().copied().cycle().take(16).enumerate() {
            tokio::time::sleep(Duration::from_millis(5)).await;
            let pool_cp = Arc::clone(&pool_checkpointer);
            let res = tokio::task::spawn_blocking(move || pool_cp.wal_checkpoint(mode))
                .await
                .expect("checkpoint task join");
            assert!(
                res.is_ok(),
                "Checkpoint step {i} with mode {mode:?} failed: {:?}",
                res.err()
            );
        }
    });
    handles.push(checkpoint_handle);

    // Group D: Spawned idle checkpoint background worker
    let bg_worker = pool.spawn_idle_checkpoint_worker(Duration::from_millis(10));

    // Await all tasks with timeout
    let join_all = async {
        for handle in handles {
            handle.await.expect("handle join cleanly");
        }
    };

    let outcome = timeout(Duration::from_secs(15), join_all).await;
    bg_worker.abort();

    assert!(
        outcome.is_ok(),
        "CRITICAL: SQLite concurrency locked or deadlocked during concurrent WAL checkpoints!"
    );

    // Verify final state
    let final_conn = pool.writer.get().expect("writer connection");
    let total_facts: i64 = final_conn
        .query_row("SELECT count(*) FROM facts", [], |r| r.get(0))
        .expect("final count query");
    assert!(
        total_facts >= 31,
        "Total facts should reflect all committed writes"
    );

    // Clean up temporary database files
    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_adversarial_vector_search_boundary_and_performance() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("fedcba9876543210fedcba9876543210");
    let conn = pool.writer.get().expect("writer");

    // 1. Empty database vector search
    let zero_query = vec![0.0f32; 384];
    let empty_results = search_similar_vectors(
        &conn,
        &crypto,
        &zero_query,
        10,
        &MetadataFilter::default(),
    )
    .expect("search on empty db must succeed with empty vec");
    assert!(empty_results.is_empty());

    // 2. Insert 20 vectors across different domains and categories
    for i in 0..20 {
        let mut v = vec![0.0f32; 384];
        v[i % 384] = 1.0;
        let domain = match i % 3 {
            0 => "Personal",
            1 => "Work",
            _ => "Health",
        };
        let category = match i % 2 {
            0 => "Finance",
            _ => "Task",
        };
        upsert_vector(
            &conn,
            &crypto,
            &format!("vec_id_{i}"),
            "fact",
            &format!("Test vector content {i}"),
            &v,
            Some(domain),
            Some(category),
            None,
            None,
            None,
        )
        .expect("upsert vector");
    }

    // 3. Search with limit = 0
    let zero_limit_results =
        search_similar_vectors(&conn, &crypto, &zero_query, 0, &MetadataFilter::default())
            .expect("limit 0");
    assert!(zero_limit_results.is_empty());

    // 4. Search with non-matching domain filter
    let no_match_filter = MetadataFilter {
        domain: Some("NonExistentDomain".to_string()),
        ..Default::default()
    };
    let no_match_results =
        search_similar_vectors(&conn, &crypto, &zero_query, 5, &no_match_filter)
            .expect("no match filter search");
    assert!(no_match_results.is_empty());

    // 5. Search with matching composite filter (domain + category)
    let match_filter = MetadataFilter {
        domain: Some("Personal".to_string()),
        category: Some("Finance".to_string()),
        ..Default::default()
    };
    let matched = search_similar_vectors(&conn, &crypto, &zero_query, 10, &match_filter)
        .expect("composite filter search");
    assert!(!matched.is_empty());
    for item in &matched {
        assert_eq!(item.domain, "Personal");
        assert_eq!(item.category, "Finance");
    }
}

// ============================================================================
// SECTION 3: KV Cache Prefix Reuse, Token Matching & Bounds Stress
// ============================================================================

#[test]
fn test_adversarial_compute_common_prefix_len_exhaustive() {
    let t = |id: i32| LlamaToken(id);

    // Case 1: Empty vs Empty
    assert_eq!(compute_common_prefix_len(&[], &[]), 0);

    // Case 2: Empty vs Non-empty
    assert_eq!(compute_common_prefix_len(&[], &[t(1), t(2), t(3)]), 0);
    assert_eq!(compute_common_prefix_len(&[t(1), t(2), t(3)], &[]), 0);

    // Case 3: Single token match & mismatch
    assert_eq!(compute_common_prefix_len(&[t(10)], &[t(10)]), 1);
    assert_eq!(compute_common_prefix_len(&[t(10)], &[t(20)]), 0);
    assert_eq!(compute_common_prefix_len(&[t(10)], &[t(10), t(20)]), 1);
    assert_eq!(compute_common_prefix_len(&[t(10), t(20)], &[t(10)]), 1);

    // Case 4: Long sequences with mismatch at position N
    let seq_a: Vec<LlamaToken> = (0..500).map(t).collect();
    let mut seq_b = seq_a.clone();
    assert_eq!(compute_common_prefix_len(&seq_a, &seq_b), 500);

    seq_b[250] = t(9999);
    assert_eq!(
        compute_common_prefix_len(&seq_a, &seq_b),
        250,
        "Mismatch at index 250 must halt prefix count at 250"
    );

    // Case 5: Completely disjoint sequences of same length
    let seq_c: Vec<LlamaToken> = (1000..1500).map(t).collect();
    assert_eq!(compute_common_prefix_len(&seq_a, &seq_c), 0);

    // Case 6: Substring match later in sequence but not at start
    let seq_d = vec![t(99), t(0), t(1), t(2)];
    assert_eq!(
        compute_common_prefix_len(&seq_a, &seq_d),
        0,
        "Prefix match must start from index 0"
    );
}

#[test]
fn test_adversarial_check_prompt_fits_overflow_and_boundaries() {
    // Standard context size
    let n_ctx = 2048;

    // Normal fitting prompt: 1000 + 512 = 1512 < 2048 -> OK
    assert!(check_prompt_fits(1000, n_ctx).is_ok());

    // Boundary: 1535 + 512 = 2047 < 2048 -> OK
    assert!(check_prompt_fits(1535, n_ctx).is_ok());

    // Exact boundary: 1536 + 512 = 2048 (NOT < 2048) -> Err
    assert!(check_prompt_fits(1536, n_ctx).is_err());

    // Excessive prompt length
    assert!(check_prompt_fits(2000, n_ctx).is_err());
    assert!(check_prompt_fits(100_000, n_ctx).is_err());

    // Extreme integer boundary: usize::MAX must NOT panic with integer overflow
    assert!(check_prompt_fits(usize::MAX, n_ctx).is_err());
    assert!(check_prompt_fits(usize::MAX - 100, n_ctx).is_err());

    // Tiny / zero n_ctx
    assert!(check_prompt_fits(0, 512).is_err()); // 0 + 512 is not < 512
    assert!(check_prompt_fits(0, 0).is_err());
    assert!(check_prompt_fits(0, 513).is_ok()); // 0 + 512 = 512 < 513 -> OK
}

#[test]
fn test_adversarial_llama_router_manager_kv_prefix_state_transitions() {
    let mut router =
        LlamaRouterManager::new(2048, 0).expect("router manager must initialize without errors");

    assert!(router.last_tokens().is_empty());
    assert_eq!(
        router.prefix_cached_len(&[LlamaToken(1), LlamaToken(2)]),
        0
    );

    // Simulate multi-turn caching:
    // Turn 1 cached tokens: [Sys1, Sys2, User1, Reply1]
    router.last_tokens = vec![
        LlamaToken(10),
        LlamaToken(20),
        LlamaToken(100),
        LlamaToken(200),
    ];
    assert_eq!(router.last_tokens().len(), 4);

    // Turn 2 incoming prompt: [Sys1, Sys2, User1, Reply1, User2]
    let turn2_prompt = vec![
        LlamaToken(10),
        LlamaToken(20),
        LlamaToken(100),
        LlamaToken(200),
        LlamaToken(101),
    ];
    assert_eq!(router.prefix_cached_len(&turn2_prompt), 4);

    // Disjoint session incoming prompt: [SysOther, UserX]
    let disjoint_prompt = vec![LlamaToken(99), LlamaToken(101)];
    assert_eq!(router.prefix_cached_len(&disjoint_prompt), 0);

    // Clear KV cache resets token state
    router.clear_kv_cache();
    assert!(router.last_tokens().is_empty());
    assert_eq!(router.prefix_cached_len(&turn2_prompt), 0);
}
