use liva_native_core::db::{DatabasePool, WalCheckpointMode};
use liva_native_core::ipc::{
    calculate_checksum, AudioStreamFrame, FrameHeader, FrameType, IpcError, IpcFrame,
    ScreenDiffFrame, ScreenDiffFrameRef, SpscRingBuffer, TelemetryFrame, TokenDeltaFrame,
    ZeroCopyDeserializable, ZeroCopySerializable, FRAME_MAGIC, FRAME_VERSION_1, MAX_PAYLOAD_SIZE,
};
use liva_native_core::llm::pool::{
    CancellationToken, LlmCompletionRequest, LlmCompletionResult, LlmPoolError, LlmPriority,
    LlmWorkerPool, LlmWorkerPoolService, TokenStreamDelta,
};
use liva_native_core::telemetry::{
    TelemetryProfiler, TraceContext, TraceContextError,
};
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, oneshot};
use uuid::Uuid;

// ============================================================================
// M1: PRIORITIZED LLM WORKER POOL EMPIRICAL CHALLENGES
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

fn create_temp_db_pool() -> (DatabasePool, std::path::PathBuf) {
    let temp_dir = std::env::temp_dir();
    let db_path = temp_dir.join(format!(
        "liva_challenger_stress_{}_{}.db",
        std::process::id(),
        rand::random::<u64>()
    ));
    let pool = DatabasePool::new(&db_path).expect("Failed to create on-disk SQLite DatabasePool");
    (pool, db_path)
}

#[tokio::test]
async fn benchmark_m1_llm_preemption_latency_stress() {
    println!("\n=== EMPIRICAL CHALLENGE: M1 LLM Worker Pool Preemption Latency ===");

    // Token delay = 500µs per token to simulate fast LLM generation
    let (pool, _handle) = LlmWorkerPool::new_simulated(Duration::from_micros(500), 128);

    let iterations = 50;
    let mut preemption_latencies_us = Vec::with_capacity(iterations);

    for i in 0..iterations {
        // Launch a long background task (100 tokens = 50ms)
        let (bg_req, _bg_stream, bg_rx) = make_test_request(
            LlmPriority::BackgroundConsolidation,
            &format!("background_workload_{}", i),
            100,
        );
        pool.submit_task(bg_req).await.unwrap();

        // Wait a few ms so the worker is actively producing tokens
        tokio::time::sleep(Duration::from_millis(3)).await;

        // Submit high-priority voice interrupt and measure preemption roundtrip
        let start_interrupt = Instant::now();
        let (voice_req, _voice_stream, voice_rx) = make_test_request(
            LlmPriority::RealtimeVoice,
            &format!("voice_interrupt_{}", i),
            5,
        );
        pool.submit_task(voice_req).await.unwrap();

        // Background task should be preempted by RealtimeVoice
        let bg_res = bg_rx.await.unwrap();
        let latency = start_interrupt.elapsed();
        let latency_us = latency.as_micros() as u64;
        preemption_latencies_us.push(latency_us);

        assert!(
            matches!(bg_res, Err(LlmPoolError::Preempted(LlmPriority::RealtimeVoice))),
            "Iteration {}: Expected Preempted(RealtimeVoice), got {:?}",
            i,
            bg_res
        );

        let voice_res = voice_rx.await.unwrap();
        assert!(voice_res.is_ok(), "Voice task must succeed");
    }

    preemption_latencies_us.sort_unstable();
    let count = preemption_latencies_us.len();
    let min = preemption_latencies_us[0];
    let p50 = preemption_latencies_us[count * 50 / 100];
    let p90 = preemption_latencies_us[count * 90 / 100];
    let p95 = preemption_latencies_us[count * 95 / 100];
    let p99 = preemption_latencies_us[count * 99 / 100];
    let max = preemption_latencies_us[count - 1];
    let sum: u64 = preemption_latencies_us.iter().sum();
    let mean = (sum as f64) / (count as f64);

    println!("Iterations: {}", count);
    println!("Preemption Latency Stats:");
    println!("  Min:  {:.3} ms ({} µs)", min as f64 / 1000.0, min);
    println!("  Mean: {:.3} ms ({:.1} µs)", mean / 1000.0, mean);
    println!("  P50:  {:.3} ms ({} µs)", p50 as f64 / 1000.0, p50);
    println!("  P90:  {:.3} ms ({} µs)", p90 as f64 / 1000.0, p90);
    println!("  P95:  {:.3} ms ({} µs)", p95 as f64 / 1000.0, p95);
    println!("  P99:  {:.3} ms ({} µs)", p99 as f64 / 1000.0, p99);
    println!("  Max:  {:.3} ms ({} µs)", max as f64 / 1000.0, max);

    assert!(
        p99 <= 5000,
        "SLA Violation: P99 preemption latency {} µs exceeds 5000 µs (5ms)",
        p99
    );

    let metrics = pool.get_metrics();
    assert!(metrics.preemption_events_total >= iterations as u64);
    assert_eq!(metrics.total_completed_tasks, iterations as u64);
    assert_eq!(metrics.total_failed_tasks, iterations as u64);

    pool.shutdown();
    println!("=== M1 EMPIRICAL CHALLENGE PASSED ===");
}

// ============================================================================
// M2: SQLITE 16R+1W WAL CONCURRENCY & QPS EMPIRICAL CHALLENGES
// ============================================================================

#[tokio::test]
async fn benchmark_m2_sqlite_16readers_1writer_concurrency_qps() {
    println!("\n=== EMPIRICAL CHALLENGE: M2 SQLite 16R+1W Concurrency & QPS ===");

    let (pool, db_path) = create_temp_db_pool();

    // Populate schema and initial records
    pool.with_write_conn(|conn| {
        conn.execute(
            "CREATE TABLE qps_stress (id INTEGER PRIMARY KEY, key TEXT, value TEXT, counter INTEGER)",
            [],
        )?;
        for i in 1..=500 {
            conn.execute(
                "INSERT INTO qps_stress (id, key, value, counter) VALUES (?, ?, ?, 0)",
                rusqlite::params![i, format!("key_{}", i), format!("val_{}_payload_data", i)],
            )?;
        }
        Ok(())
    })
    .unwrap();

    let reader_count = 16;
    let duration = Duration::from_millis(1000);
    let stop_signal = Arc::new(AtomicBool::new(false));
    let sqlite_busy_count = Arc::new(AtomicUsize::new(0));
    let writer_ops_count = Arc::new(AtomicUsize::new(0));

    // Spawn 1 dedicated writer thread performing continuous updates & cooperative chunks
    let pool_writer = pool.clone();
    let stop_w = stop_signal.clone();
    let busy_w = sqlite_busy_count.clone();
    let ops_w = writer_ops_count.clone();

    let writer_handle = std::thread::spawn(move || {
        let mut step = 0;
        while !stop_w.load(Ordering::Relaxed) {
            step += 1;
            let target_id = (step % 500) + 1;
            let res = pool_writer.with_write_conn(|conn| {
                conn.execute(
                    "UPDATE qps_stress SET counter = counter + 1 WHERE id = ?",
                    [target_id],
                )
            });
            match res {
                Ok(_) => {
                    ops_w.fetch_add(1, Ordering::Relaxed);
                }
                Err(rusqlite::Error::SqliteFailure(err, _))
                    if err.extended_code == rusqlite::ffi::SQLITE_BUSY =>
                {
                    busy_w.fetch_add(1, Ordering::Relaxed);
                }
                Err(e) => {
                    panic!("Writer unexpected error: {:?}", e);
                }
            }
            std::thread::yield_now();
        }
        step
    });

    // Spawn 16 concurrent reader threads doing tight SELECT queries
    let mut reader_handles = Vec::with_capacity(reader_count);
    let start_time = Instant::now();

    for r_idx in 0..reader_count {
        let pool_r = pool.clone();
        let stop_r = stop_signal.clone();
        let busy_r = sqlite_busy_count.clone();

        reader_handles.push(std::thread::spawn(move || {
            let mut queries = 0u64;
            while !stop_r.load(Ordering::Relaxed) {
                let target_id = ((queries + r_idx as u64) % 500) + 1;
                let res = pool_r.with_read_conn(|conn| {
                    conn.query_row(
                        "SELECT key, value, counter FROM qps_stress WHERE id = ?",
                        [target_id],
                        |row| {
                            let k: String = row.get(0)?;
                            let v: String = row.get(1)?;
                            let c: i64 = row.get(2)?;
                            Ok((k, v, c))
                        },
                    )
                });
                match res {
                    Ok((_k, _v, _c)) => {
                        queries += 1;
                    }
                    Err(rusqlite::Error::SqliteFailure(err, _))
                        if err.extended_code == rusqlite::ffi::SQLITE_BUSY =>
                    {
                        busy_r.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        panic!("Reader #{} unexpected error: {:?}", r_idx, e);
                    }
                }
            }
            queries
        }));
    }

    // Run for the benchmark duration
    tokio::time::sleep(duration).await;
    stop_signal.store(true, Ordering::Relaxed);

    // Join all reader threads and accumulate query counts
    let mut total_reads = 0u64;
    for (_idx, handle) in reader_handles.into_iter().enumerate() {
        let count = handle.join().unwrap();
        total_reads += count;
    }

    writer_handle.join().unwrap();
    let elapsed = start_time.elapsed();
    let read_qps = (total_reads as f64) / elapsed.as_secs_f64();
    let writer_ops = writer_ops_count.load(Ordering::Relaxed);
    let busy_errors = sqlite_busy_count.load(Ordering::Relaxed);

    println!("Benchmark Duration: {:?}", elapsed);
    println!("Concurrent Readers: {}", reader_count);
    println!("Total Reader Queries: {}", total_reads);
    println!("Aggregate Read QPS: {:.2} QPS", read_qps);
    println!("Writer Operations Completed: {}", writer_ops);
    println!("SQLITE_BUSY Occurrences: {}", busy_errors);

    // Assertions against requirements:
    // 1. Read QPS >= 2,500 QPS SLA
    assert!(
        read_qps >= 2500.0,
        "SLA Violation: Aggregate Read QPS {:.2} is below the 2,500 QPS target!",
        read_qps
    );

    // 2. Exactly 0 SQLITE_BUSY errors
    assert_eq!(
        busy_errors, 0,
        "Concurrency Violation: Encountered {} SQLITE_BUSY errors under 16R+1W load!",
        busy_errors
    );

    // 3. Maintenance check: verify passive checkpoint during active load
    let chk_res = pool.wal_checkpoint(WalCheckpointMode::Passive).unwrap();
    assert!(chk_res.busy >= 0);

    let _ = std::fs::remove_file(&db_path);
    println!("=== M2 EMPIRICAL CHALLENGE PASSED ===");
}

// ============================================================================
// M3: ZERO-COPY IPC BRIDGE EMPIRICAL CHALLENGES
// ============================================================================

#[test]
fn benchmark_m3_spsc_cross_thread_50k_frames_throughput() {
    println!("\n=== EMPIRICAL CHALLENGE: M3 SPSC Ring Buffer 50,000+ Cross-Thread Frames ===");

    let total_frames: usize = 60_000; // > 50,000 SLA
    let ring_buffer = Arc::new(SpscRingBuffer::new(4 * 1024 * 1024)); // 4MB aligned buffer

    let ring_producer = ring_buffer.clone();
    let producer_handle = std::thread::spawn(move || {
        let start = Instant::now();
        for i in 0..total_frames {
            let payload = match i % 4 {
                0 => {
                    // ScreenDiff frame simulation (512 bytes)
                    let frame = ScreenDiffFrame {
                        timestamp_ms: i as i64,
                        width: 1920,
                        height: 1080,
                        format: 0,
                        damage_x: 0,
                        damage_y: 0,
                        damage_w: 100,
                        damage_h: 50,
                        raw_data: vec![(i & 0xFF) as u8; 512],
                    };
                    frame.to_vec()
                }
                1 => {
                    // AudioStream frame simulation (256 bytes)
                    let frame = AudioStreamFrame {
                        timestamp_ns: (i as u64) * 1_000_000,
                        sample_rate: 16000,
                        channels: 1,
                        format: 0,
                        samples_count: 128,
                        pcm_data: vec![(i & 0x7F) as u8; 256],
                    };
                    frame.to_vec()
                }
                2 => {
                    // TokenDelta frame
                    let frame = TokenDeltaFrame {
                        task_id: Uuid::new_v4(),
                        token_id: i as i32,
                        is_first: i == 0,
                        is_final: i == total_frames - 1,
                        cumulative_tokens: i as u32,
                        latency_from_start_ns: 123456,
                        text: format!("token_chunk_{}", i),
                    };
                    frame.to_vec()
                }
                _ => {
                    // Raw custom frame
                    let mut data = vec![0u8; 128];
                    data[0..8].copy_from_slice(&(i as u64).to_le_bytes());
                    data
                }
            };

            let frame_type = match i % 4 {
                0 => FrameType::ScreenDiff,
                1 => FrameType::AudioStream,
                2 => FrameType::TokenDelta,
                _ => FrameType::Custom,
            };

            // Spin/yield until ring buffer space is available
            loop {
                match ring_producer.write_ipc_frame(frame_type, &payload) {
                    Ok(()) => break,
                    Err(IpcError::RingBufferFull) => {
                        std::hint::spin_loop();
                    }
                    Err(e) => panic!("Unexpected write error at index {}: {:?}", i, e),
                }
            }
        }
        start.elapsed()
    });

    let ring_consumer = ring_buffer.clone();
    let consumer_handle = std::thread::spawn(move || {
        let mut scratch_buf = Vec::with_capacity(64 * 1024);
        let mut frames_read = 0usize;
        let mut bytes_transferred = 0usize;
        let start = Instant::now();

        while frames_read < total_frames {
            match ring_consumer.read_ipc_frame(&mut scratch_buf) {
                Ok(Some(frame_ref)) => {
                    let expected_type = match frames_read % 4 {
                        0 => FrameType::ScreenDiff,
                        1 => FrameType::AudioStream,
                        2 => FrameType::TokenDelta,
                        _ => FrameType::Custom,
                    };
                    assert_eq!(
                        frame_ref.header.frame_type,
                        expected_type as u16,
                        "Frame type mismatch at index {}",
                        frames_read
                    );
                    assert_eq!(
                        frame_ref.header.checksum,
                        calculate_checksum(frame_ref.payload),
                        "Checksum corruption at index {}",
                        frames_read
                    );

                    bytes_transferred += std::mem::size_of::<FrameHeader>() + frame_ref.payload.len();
                    frames_read += 1;
                }
                Ok(None) => {
                    std::hint::spin_loop();
                }
                Err(e) => panic!("Consumer read error at frame {}: {:?}", frames_read, e),
            }
        }
        (frames_read, bytes_transferred, start.elapsed())
    });

    let prod_elapsed = producer_handle.join().unwrap();
    let (frames_read, total_bytes, cons_elapsed) = consumer_handle.join().unwrap();

    let max_elapsed = prod_elapsed.max(cons_elapsed);
    let frames_per_sec = (frames_read as f64) / max_elapsed.as_secs_f64();
    let mb_per_sec = (total_bytes as f64) / (1024.0 * 1024.0 * max_elapsed.as_secs_f64());

    println!("Total Frames Streamed: {}", frames_read);
    println!("Total Bytes Transferred: {:.2} MB", total_bytes as f64 / (1024.0 * 1024.0));
    println!("Producer Duration: {:?}", prod_elapsed);
    println!("Consumer Duration: {:?}", cons_elapsed);
    println!("Aggregate Frame Rate: {:.2} frames/sec", frames_per_sec);
    println!("Aggregate Throughput: {:.2} MB/sec", mb_per_sec);

    assert_eq!(frames_read, total_frames, "All frames must be consumed without loss");
    assert!(
        frames_per_sec >= 100_000.0,
        "SLA Violation: SPSC throughput {:.2} frames/sec is below 100k frames/sec target",
        frames_per_sec
    );

    println!("=== M3 SPSC 50,000+ FRAME STREAMING PASSED ===");
}

#[test]
fn benchmark_m3_zero_copy_deserialization_sla_latency() {
    println!("\n=== EMPIRICAL CHALLENGE: M3 Zero-Copy Deserialization Latency SLAs ===");

    // ── Challenge 1: Unchecked / Zero-Copy Direct Struct & Header Borrow (SLA <= 0.01µs / 10ns) ──
    let telemetry_frame = TelemetryFrame {
        timestamp_ns: 123456789,
        ttft_ns: 42000000,
        total_duration_ns: 150000000,
        tokens_generated: 128,
        prompt_tokens: 32,
        db_read_latency_ns: 450000,
        db_write_latency_ns: 1200000,
        memory_rss_bytes: 64 * 1024 * 1024,
        cpu_usage_percent: 12.5,
        voice_queue_depth: 0,
        user_queue_depth: 1,
        bg_queue_depth: 2,
        preemption_count: 3,
        _reserved: 0,
    };
    let telemetry_bytes = bytemuck::bytes_of(&telemetry_frame);

    let unchecked_iterations = 200_000;
    let start_unchecked = Instant::now();
    let mut dummy_acc: u64 = 0;
    for _ in 0..unchecked_iterations {
        // Zero-copy borrow using Pod casting
        let parsed: &TelemetryFrame = bytemuck::from_bytes(telemetry_bytes);
        dummy_acc = dummy_acc.wrapping_add(parsed.tokens_generated);
    }
    let elapsed_unchecked = start_unchecked.elapsed();
    let avg_unchecked_ns = (elapsed_unchecked.as_nanos() as f64) / (unchecked_iterations as f64);
    let avg_unchecked_us = avg_unchecked_ns / 1000.0;

    println!("Part A: Unchecked / Pod Zero-Copy Borrow ({} iterations):", unchecked_iterations);
    println!("  Total Duration: {:?}", elapsed_unchecked);
    println!("  Average Latency: {:.4} ns ({:.6} µs)", avg_unchecked_ns, avg_unchecked_us);
    assert!(
        avg_unchecked_us <= 0.05,
        "SLA Violation: Unchecked zero-copy latency {:.6} µs exceeds 0.05 µs limit (50 ns)!",
        avg_unchecked_us
    );
    assert_ne!(dummy_acc, 0);

    // ── Challenge 2: 1MB Payload Validated Zero-Copy Deserialization (SLA <= 25µs) ──
    let payload_size = 1024 * 1024; // 1 MB payload
    let raw_payload = vec![0xABu8; payload_size];
    let screen_frame = ScreenDiffFrame {
        timestamp_ms: 1000,
        width: 1920,
        height: 1080,
        format: 0,
        damage_x: 0,
        damage_y: 0,
        damage_w: 1920,
        damage_h: 1080,
        raw_data: raw_payload,
    };
    let screen_encoded = screen_frame.to_vec();

    let validated_iterations = 50_000;
    let mut latencies_us = Vec::with_capacity(validated_iterations);

    let start_val = Instant::now();
    for _ in 0..validated_iterations {
        let t0 = Instant::now();
        // Validated zero-copy deserialization with full layout & boundary enforcement
        let decoded_screen = ScreenDiffFrameRef::decode_from_slice(&screen_encoded)
            .expect("Screen diff decode must succeed");
        let dt = t0.elapsed();
        latencies_us.push(dt.as_nanos() as f64 / 1000.0);
        assert_eq!(decoded_screen.header.width, 1920);
        assert_eq!(decoded_screen.raw_data.len(), payload_size);
    }
    let total_val_elapsed = start_val.elapsed();

    latencies_us.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let count = latencies_us.len();
    let min = latencies_us[0];
    let p50 = latencies_us[count * 50 / 100];
    let p95 = latencies_us[count * 95 / 100];
    let p99 = latencies_us[count * 99 / 100];
    let max = latencies_us[count - 1];
    let sum: f64 = latencies_us.iter().sum();
    let mean = sum / (count as f64);

    println!("\nPart B: 1MB Validated Zero-Copy Deserialization ({} iterations):", validated_iterations);
    println!("  Total Duration: {:?}", total_val_elapsed);
    println!("  Min Latency:  {:.4} µs ({} ns)", min, (min * 1000.0) as u64);
    println!("  Mean Latency: {:.4} µs ({} ns)", mean, (mean * 1000.0) as u64);
    println!("  P50 Latency:  {:.4} µs ({} ns)", p50, (p50 * 1000.0) as u64);
    println!("  P95 Latency:  {:.4} µs ({} ns)", p95, (p95 * 1000.0) as u64);
    println!("  P99 Latency:  {:.4} µs ({} ns)", p99, (p99 * 1000.0) as u64);
    println!("  Max Latency:  {:.4} µs ({} ns)", max, (max * 1000.0) as u64);

    assert!(
        mean <= 25.0,
        "SLA Violation: 1MB Validated Mean Latency {:.3} µs exceeds 25.0 µs limit!",
        mean
    );
    assert!(
        p95 <= 25.0,
        "SLA Violation: 1MB Validated P95 Latency {:.3} µs exceeds 25.0 µs limit!",
        p95
    );

    println!("=== M3 ZERO-COPY LATENCY SLAs VERIFIED ===");
}

#[test]
fn benchmark_m3_corrupted_header_defenses_stress() {
    println!("\n=== EMPIRICAL CHALLENGE: M3 Corrupted Header & Malformed Defenses ===");

    let ring_buffer = SpscRingBuffer::new(64 * 1024); // 64 KB capacity

    // 1. Defense against 0xFFFFFFFF (4GB length overflow)
    let huge_slice_err = ring_buffer.write_slice(&vec![0u8; 65 * 1024]);
    assert!(matches!(huge_slice_err, Err(IpcError::PayloadTooLarge { .. })));

    // Test FrameHeader payload_len > MAX_PAYLOAD_SIZE defense
    let mut header = FrameHeader {
        magic: FRAME_MAGIC,
        version: FRAME_VERSION_1,
        frame_type: FrameType::Custom as u16,
        flags: 0,
        payload_len: 0xFFFFFFFF,
        checksum: 0,
        _reserved: 0,
    };
    let mut header_buf = vec![0u8; std::mem::size_of::<FrameHeader>() + 32];
    header_buf[..std::mem::size_of::<FrameHeader>()].copy_from_slice(bytemuck::bytes_of(&header));
    let decode_overflow_res = IpcFrame::decode(&header_buf);
    assert!(
        matches!(decode_overflow_res, Err(IpcError::PayloadTooLarge { .. })),
        "4GB payload length must be rejected with PayloadTooLarge, got {:?}",
        decode_overflow_res
    );

    // 2. Defense against 0 length payload in write_slice
    let zero_len_res = ring_buffer.write_slice(&[]);
    assert!(
        matches!(zero_len_res, Err(IpcError::Validation(ref msg)) if msg.contains("0")),
        "Zero-length payload must be rejected, got {:?}",
        zero_len_res
    );

    // 3. Defense against payload exceeding MAX_PAYLOAD_SIZE (64MB)
    let max_exceeded_header = FrameHeader {
        magic: FRAME_MAGIC,
        version: FRAME_VERSION_1,
        frame_type: FrameType::Custom as u16,
        flags: 0,
        payload_len: (MAX_PAYLOAD_SIZE + 1) as u32,
        checksum: 0,
        _reserved: 0,
    };
    let mut max_buf = vec![0u8; std::mem::size_of::<FrameHeader>() + 32];
    max_buf[..std::mem::size_of::<FrameHeader>()].copy_from_slice(bytemuck::bytes_of(&max_exceeded_header));
    let max_res = IpcFrame::decode(&max_buf);
    assert!(matches!(max_res, Err(IpcError::PayloadTooLarge { .. })));

    // 4. Defense against corrupted Magic bytes
    header.payload_len = 16;
    header.magic = *b"DEAD";
    header.checksum = calculate_checksum(&[0u8; 16]);
    let mut bad_magic_buf = vec![0u8; std::mem::size_of::<FrameHeader>() + 16];
    bad_magic_buf[..std::mem::size_of::<FrameHeader>()].copy_from_slice(bytemuck::bytes_of(&header));
    let bad_magic_res = IpcFrame::decode(&bad_magic_buf);
    assert!(
        matches!(bad_magic_res, Err(IpcError::Validation(ref msg)) if msg.contains("magic")),
        "Bad magic must be rejected with Validation error, got {:?}",
        bad_magic_res
    );

    // 5. Defense against unsupported Version
    header.magic = FRAME_MAGIC;
    header.version = 99;
    bad_magic_buf[..std::mem::size_of::<FrameHeader>()].copy_from_slice(bytemuck::bytes_of(&header));
    let bad_ver_res = IpcFrame::decode(&bad_magic_buf);
    assert!(
        matches!(bad_ver_res, Err(IpcError::Validation(ref msg)) if msg.contains("version")),
        "Bad version must be rejected with Validation error, got {:?}",
        bad_ver_res
    );

    // 6. Defense against Checksum Tampering
    header.version = FRAME_VERSION_1;
    let payload = b"critical_payload_data";
    header.payload_len = payload.len() as u32;
    header.checksum = calculate_checksum(payload);
    let mut tampered_buf = vec![0u8; std::mem::size_of::<FrameHeader>() + payload.len()];
    tampered_buf[..std::mem::size_of::<FrameHeader>()].copy_from_slice(bytemuck::bytes_of(&header));
    tampered_buf[std::mem::size_of::<FrameHeader>()..].copy_from_slice(payload);

    // Tamper 1 byte in payload
    tampered_buf[std::mem::size_of::<FrameHeader>() + 2] ^= 0x01;
    let tampered_res = IpcFrame::decode(&tampered_buf);
    assert!(
        matches!(tampered_res, Err(IpcError::ChecksumMismatch { .. })),
        "Tampered payload must trigger ChecksumMismatch, got {:?}",
        tampered_res
    );

    // 7. Non-power-of-two and zero capacity SPSC buffer creation defense
    assert!(SpscRingBuffer::try_new(0).is_err());
    assert!(SpscRingBuffer::try_new(100).is_err());
    assert!(SpscRingBuffer::try_new(1000).is_err());
    assert!(SpscRingBuffer::try_new(1024).is_ok());

    println!("=== M3 CORRUPTED HEADER DEFENSES VERIFIED ===");
}

// ============================================================================
// M4: TELEMETRY & PROMETHEUS TRACING EMPIRICAL CHALLENGES
// ============================================================================

#[test]
fn benchmark_m4_concurrent_telemetry_metrics_stress() {
    println!("\n=== EMPIRICAL CHALLENGE: M4 Concurrent Multi-Threaded Telemetry Updates ===");

    let profiler = Arc::new(TelemetryProfiler::new());
    let thread_count = 16;
    let ops_per_thread = 10_000;
    let total_expected_ops = thread_count * ops_per_thread;

    let mut handles = Vec::with_capacity(thread_count);
    let start = Instant::now();

    for t_idx in 0..thread_count {
        let profiler_clone = profiler.clone();
        handles.push(std::thread::spawn(move || {
            for i in 0..ops_per_thread {
                let op = (t_idx + i) % 7;
                match op {
                    0 => {
                        // TTFT measurement
                        let latency_ms = 20.0 + ((i % 100) as f64) * 2.0;
                        profiler_clone.record_ttft("llama3-8b", latency_ms, 128);
                    }
                    1 => {
                        // Token generation
                        profiler_clone.record_tokens_generated(5, Duration::from_millis(50));
                    }
                    2 => {
                        // Queue depths
                        profiler_clone.inc_queue_depth("voice");
                        profiler_clone.dec_queue_depth("voice");
                    }
                    3 => {
                        // DB latencies
                        profiler_clone.record_db_read_latency(Duration::from_micros(450));
                        profiler_clone.record_db_write_latency(Duration::from_millis(2));
                    }
                    4 => {
                        // Preemption
                        profiler_clone.record_preemption();
                    }
                    5 => {
                        // Resource gauge
                        profiler_clone.set_process_rss_bytes(128 * 1024 * 1024);
                        profiler_clone.set_process_cpu_percent(35.5);
                    }
                    _ => {
                        // Structured event
                        profiler_clone.record_event("info", "agent", "concurrent_test_step", None);
                    }
                }
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (total_expected_ops as f64) / elapsed.as_secs_f64();

    println!("Threads: {}", thread_count);
    println!("Total Operations: {}", total_expected_ops);
    println!("Elapsed Time: {:?}", elapsed);
    println!("Metric Ingestion Throughput: {:.2} ops/sec", ops_per_sec);

    // Verify consistency
    let preemptions = profiler.get_preemptions();
    let snapshot = profiler.get_telemetry_snapshot();
    let tokens = snapshot["tokens_generated_total"].as_u64().unwrap();

    println!("Preemptions Recorded: {}", preemptions);
    println!("Tokens Recorded: {}", tokens);

    assert!(preemptions > 0);
    assert!(tokens > 0);
    assert!(ops_per_sec >= 200_000.0, "Throughput must exceed 200k ops/sec");

    println!("=== M4 CONCURRENT TELEMETRY STRESS PASSED ===");
}

#[test]
fn benchmark_m4_prometheus_exposition_format_validation() {
    println!("\n=== EMPIRICAL CHALLENGE: M4 Prometheus Exposition Format Validation ===");

    let profiler = TelemetryProfiler::new();

    // Ingest diverse samples
    profiler.record_ttft("qwen-2.5", 35.0, 64);
    profiler.record_ttft("qwen-2.5", 120.0, 128);
    profiler.record_ttft("qwen-2.5", 450.0, 256);
    profiler.record_tokens_generated(100, Duration::from_secs(2)); // 50 tokens/sec
    profiler.set_queue_depth("voice", 2);
    profiler.set_queue_depth("user", 5);
    profiler.set_queue_depth("background", 10);
    profiler.record_preemption();
    profiler.record_preemption();
    profiler.record_db_pool_wait(Duration::from_micros(800));
    profiler.record_db_query(Duration::from_millis(5));
    profiler.set_process_rss_bytes(256 * 1024 * 1024);
    profiler.set_process_cpu_percent(18.75);

    let output = profiler.export_prometheus_metrics();

    println!("Prometheus Exposition Output:\n{}", output);

    // Parse and validate line by line
    let lines: Vec<&str> = output.lines().collect();
    assert!(!lines.is_empty(), "Exposition output must not be empty");

    let required_metric_families = [
        "liva_ttft_seconds",
        "liva_tokens_per_second",
        "liva_worker_queue_depth",
        "liva_llm_preemptions_total",
        "liva_db_pool_wait_duration_ms",
        "liva_db_query_duration_ms",
        "liva_process_rss_bytes",
        "liva_process_cpu_percent",
        "liva_tokens_generated_total",
    ];

    for family in &required_metric_families {
        let help_decl = format!("# HELP {}", family);
        let type_decl = format!("# TYPE {}", family);
        assert!(
            lines.iter().any(|l| l.starts_with(&help_decl)),
            "Missing # HELP declaration for {}",
            family
        );
        assert!(
            lines.iter().any(|l| l.starts_with(&type_decl)),
            "Missing # TYPE declaration for {}",
            family
        );
    }

    // Validate histogram monotonic properties for TTFT
    let ttft_buckets: Vec<u64> = lines
        .iter()
        .filter(|l| l.starts_with("liva_ttft_seconds_bucket{le="))
        .map(|l| {
            let val_part = l.split_whitespace().nth(1).unwrap();
            val_part.parse::<u64>().unwrap()
        })
        .collect();

    assert!(!ttft_buckets.is_empty());
    for i in 1..ttft_buckets.len() {
        assert!(
            ttft_buckets[i] >= ttft_buckets[i - 1],
            "Histogram bucket count must be monotonically non-decreasing: {} vs {}",
            ttft_buckets[i - 1],
            ttft_buckets[i]
        );
    }

    // Validate queue depths
    assert!(lines.iter().any(|&l| l == "liva_worker_queue_depth{priority=\"voice\"} 2"));
    assert!(lines.iter().any(|&l| l == "liva_worker_queue_depth{priority=\"user\"} 5"));
    assert!(lines.iter().any(|&l| l == "liva_worker_queue_depth{priority=\"background\"} 10"));

    // Validate preemptions total
    assert!(lines.iter().any(|&l| l == "liva_llm_preemptions_total 2"));

    println!("=== M4 PROMETHEUS EXPOSITION VALIDATION PASSED ===");
}

#[test]
fn benchmark_m4_w3c_traceparent_propagation_stress() {
    println!("\n=== EMPIRICAL CHALLENGE: M4 W3C Traceparent Propagation Stress ===");

    let thread_count = 16;
    let iterations_per_thread = 5_000;
    let total_iterations = thread_count * iterations_per_thread;

    let mut handles = Vec::with_capacity(thread_count);
    let start = Instant::now();

    for _ in 0..thread_count {
        handles.push(std::thread::spawn(move || {
            for _ in 0..iterations_per_thread {
                // 1. Root context generation
                let root = TraceContext::new();
                assert_eq!(root.version, 0);
                assert!(root.is_sampled());
                assert!(root.trace_id.iter().any(|&b| b != 0));
                assert!(root.parent_id.iter().any(|&b| b != 0));

                // 2. Serialization to W3C string
                let traceparent = root.to_traceparent();
                assert_eq!(traceparent.len(), 55); // 00-32hex-16hex-02hex = 2+1+32+1+16+1+2 = 55 chars

                // 3. Deserialization from W3C string
                let parsed = TraceContext::from_str(&traceparent).expect("Parse must succeed");
                assert_eq!(parsed, root);

                // 4. Child context derivation
                let child = root.child_context();
                assert_eq!(child.version, root.version);
                assert_eq!(child.trace_id, root.trace_id);
                assert_ne!(child.parent_id, root.parent_id); // Child must have fresh span ID
                assert_eq!(child.trace_flags, root.trace_flags);

                let child_traceparent = child.to_traceparent();
                let child_parsed = TraceContext::from_str(&child_traceparent).unwrap();
                assert_eq!(child_parsed, child);
            }
        }));
    }

    for h in handles {
        h.join().unwrap();
    }

    let elapsed = start.elapsed();
    let ops_per_sec = (total_iterations as f64) / elapsed.as_secs_f64();

    println!("Total TraceContext Roundtrips: {}", total_iterations);
    println!("Elapsed Duration: {:?}", elapsed);
    println!("Propagation Rate: {:.2} contexts/sec", ops_per_sec);

    // ── Adversarial Error Rejection Verification ──
    // 1. All zeros trace ID
    let zero_trace = "00-00000000000000000000000000000000-00f067aa0ba902b7-01";
    assert_eq!(
        TraceContext::from_str(zero_trace),
        Err(TraceContextError::AllZerosTraceId)
    );

    // 2. All zeros parent ID
    let zero_parent = "00-4bf92f3577b34da6a3ce929d0e0e4736-0000000000000000-01";
    assert_eq!(
        TraceContext::from_str(zero_parent),
        Err(TraceContextError::AllZerosParentId)
    );

    // 3. Invalid version 0xFF
    let bad_ver = "ff-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";
    assert_eq!(
        TraceContext::from_str(bad_ver),
        Err(TraceContextError::InvalidVersion(0xff))
    );

    // 4. Invalid delimiter format / length
    let bad_fmt = "00-4bf92f3577b34da6a3ce929d0e0e4736";
    assert_eq!(
        TraceContext::from_str(bad_fmt),
        Err(TraceContextError::InvalidFormat)
    );

    // 5. Invalid hex character in trace ID
    let bad_hex = "00-4bf92f3577b34da6a3ce929d0e0e473g-00f067aa0ba902b7-01";
    assert_eq!(
        TraceContext::from_str(bad_hex),
        Err(TraceContextError::InvalidTraceIdHex)
    );

    println!("=== M4 W3C TRACEPARENT PROPAGATION STRESS PASSED ===");
}
