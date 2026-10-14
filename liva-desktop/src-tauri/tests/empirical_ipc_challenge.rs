//! Empirical Stress & Adversarial Challenge Harness for Tauri v2 IPC Streaming & Backpressure
//!
//! Validates:
//! 1. Rapid Producer Flood (10,000+ events across multiple producers) & Bounded Capacity 100 Backpressure.
//! 2. Early Receiver Drop (immediate and mid-stream) without panics, deadlocks, or task leaks.
//! 3. Corrupted / Malformed non-JSON payloads recovery (fuzzing, null bytes, deep nesting, giant chunks).
//! 4. Voice IPC Event Sink concurrent subscribe/drop churn under heavy continuous dispatch.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::llm::engine::CompletionStream;
use liva_native_core::{db, stt, tts, AppState, VoiceIpcEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::mpsc;

fn test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
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
        llm: AppState::mock_llm(),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. PRODUCER FLOOD & BOUNDED CAPACITY 100 BACKPRESSURE
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_bounded_buffer_100_strict_saturation_and_drain() {
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Exactly 100 items fit into the bounded buffer
    for i in 0..100 {
        assert!(
            tx.try_send(format!("{{\"index\": {i}}}")).is_ok(),
            "Buffer item {i} must fit into 100-capacity channel"
        );
    }

    // 101st try_send must fail with Full backpressure error
    let err = tx
        .try_send("{\"index\": 100}".to_string())
        .expect_err("Buffer must reject 101st item when capacity is 100");
    assert!(
        matches!(err, mpsc::error::TrySendError::Full(_)),
        "Error kind must be Full backpressure"
    );

    // Draining 25 elements allows exactly 25 more elements
    for i in 0..25 {
        let msg = rx.recv().await.expect("Must receive element");
        assert!(msg.contains(&format!("\"index\": {i}")));
    }

    for i in 100..125 {
        assert!(
            tx.try_send(format!("{{\"index\": {i}}}")).is_ok(),
            "Freed slot must accept item {i}"
        );
    }

    // Full again at 100 elements
    assert!(tx.try_send("{\"overflow\": true}".to_string()).is_err());
}

#[tokio::test]
async fn challenge_rapid_producer_flood_10000_multi_producer_concurrency() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let total_produced = Arc::new(AtomicUsize::new(0));
    let total_consumed = Arc::new(AtomicUsize::new(0));
    let start_time = Instant::now();

    // 5 concurrent producer tasks, each sending 2,000 JSON messages (total 10,000)
    let num_producers = 5;
    let items_per_producer = 2000;
    let mut producer_handles = Vec::new();

    for p_id in 0..num_producers {
        let tx_clone = tx.clone();
        let produced_counter = total_produced.clone();
        producer_handles.push(tokio::spawn(async move {
            for seq in 0..items_per_producer {
                let payload = format!(
                    "{{\"producer\": {p_id}, \"seq\": {seq}, \"timestamp\": \"2026-10-14T00:00:00Z\", \"data\": \"high-speed stream packet payload content\"}}"
                );
                tx_clone
                    .send(payload)
                    .await
                    .expect("Send must succeed while consumer is active");
                produced_counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }
    // Drop original sender so channel closes when all 5 producers finish
    drop(tx);

    let consumed_counter = total_consumed.clone();
    let consumer_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Validate that incoming message parses as valid RawValue JSON
            let parsed = serde_json::value::RawValue::from_string(msg);
            if parsed.is_ok() {
                consumed_counter.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    for h in producer_handles {
        h.await.expect("Producer task completed cleanly");
    }
    consumer_handle
        .await
        .expect("Consumer task completed cleanly");

    let elapsed = start_time.elapsed();
    let produced = total_produced.load(Ordering::SeqCst);
    let consumed = total_consumed.load(Ordering::SeqCst);

    assert_eq!(produced, 10_000, "Must produce exactly 10,000 items");
    assert_eq!(consumed, 10_000, "Must consume exactly 10,000 items");
    println!("Flood test: 10,000 items streamed through 100-bounded channel in {elapsed:?}");
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. EARLY RECEIVER DROP & STREAM CANCELLATION
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_early_receiver_drop_abrupt_mid_stream_cancellation() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let send_success_count = Arc::new(AtomicUsize::new(0));
    let send_failure_count = Arc::new(AtomicUsize::new(0));

    let success_clone = send_success_count.clone();
    let failure_clone = send_failure_count.clone();

    // Producer attempts to send 5,000 items
    let producer = tokio::spawn(async move {
        for i in 0..5000 {
            let msg = format!("{{\"seq\": {i}}}");
            match tx.send(msg).await {
                Ok(_) => {
                    success_clone.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    // Receiver dropped! Must immediately abort further generation
                    failure_clone.fetch_add(1, Ordering::Relaxed);
                    break;
                }
            }
        }
    });

    // Consumer reads only 150 items then drops the receiver immediately
    for _ in 0..150 {
        let _ = rx.recv().await;
    }
    drop(rx); // Abrupt client disconnect / window close

    producer.await.expect("Producer must terminate cleanly");

    let successes = send_success_count.load(Ordering::SeqCst);
    let failures = send_failure_count.load(Ordering::SeqCst);

    // Producer must have sent 150 (read) + at most 100 (channel buffer capacity) + 1 (triggering send error)
    assert!(
        successes <= 251,
        "Producer must not flood past channel capacity after receiver dropped (successes: {successes})"
    );
    assert_eq!(
        failures, 1,
        "Producer must encounter exactly 1 failure before breaking cleanly"
    );
}

#[tokio::test]
async fn challenge_completion_stream_lifecycle_and_cancellation() {
    let (tx, rx) = mpsc::channel::<String>(100);

    // CompletionStream is designed for synchronous token-generation callbacks inside spawn_blocking
    let res = tokio::task::spawn_blocking(move || {
        let mut stream = CompletionStream::new(&tx);

        // Initial forward succeeds
        let ok = stream.forward("test piece", &serde_json::json!({"token": "hello"}));
        assert!(ok, "Initial forward must succeed");

        // Drop the receiver
        drop(rx);

        // Subsequent forward must detect closed channel and return false
        let stopped = stream.forward("next piece", &serde_json::json!({"token": "world"}));
        assert!(!stopped, "Forward to closed channel must return false");

        // finish() must return an Err indicating cancellation
        stream.finish(Ok(liva_native_core::llm::engine::CompletionOutput {
            text: "partial".to_string(),
            prompt_tokens: 5,
            completion_tokens: 2,
        }))
    })
    .await
    .unwrap();

    assert!(
        res.is_err(),
        "CompletionStream finish must return Err when stream is cancelled"
    );
}

#[tokio::test]
async fn challenge_voice_ipc_sink_high_frequency_drop_churn() {
    // Stress test VoiceIpcEvent dispatching to dropped sink concurrently
    let (tx, rx) = tokio::sync::mpsc::channel::<VoiceIpcEvent>(32);
    drop(rx); // Immediate drop simulating closed webview

    let sink = Arc::new(move |evt: VoiceIpcEvent| {
        if let Err(err) = tx.try_send(evt) {
            // Emulates liva-desktop tracing::debug!
            let _ = err;
        }
    });

    let num_tasks = 4;
    let events_per_task = 2500;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let sink_clone = sink.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..events_per_task {
                let evt = match i % 4 {
                    0 => VoiceIpcEvent::Flush {
                        seq_id: (t * 10000 + i) as u32,
                    },
                    1 => VoiceIpcEvent::Speaker {
                        turn_epoch: 1,
                        sample_rate: 24000,
                        samples: vec![0.02; 240], // 10ms frame
                    },
                    2 => VoiceIpcEvent::Viseme {
                        turn_epoch: 1,
                        base_seq_id: i as u32,
                        visemes: serde_json::json!([{"time": 0.01, "viseme": "sil"}]),
                    },
                    _ => VoiceIpcEvent::TextEvent {
                        event: "voice:transcription".to_string(),
                        payload: serde_json::json!({"text": "stress event"}),
                    },
                };
                sink_clone(evt);
            }
        }));
    }

    for h in handles {
        h.await.expect("Task must terminate without panic");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. CORRUPTED & MALFORMED NON-JSON PAYLOAD FUZZING
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_corrupted_non_json_adversarial_fuzzing() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let valid_count = Arc::new(AtomicUsize::new(0));
    let malformed_count = Arc::new(AtomicUsize::new(0));

    let valid_clone = valid_count.clone();
    let malformed_clone = malformed_count.clone();

    // Simulates the exact forwarding logic in native_ipc_call_stream in lib.rs
    let forwarder = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::value::RawValue::from_string(msg) {
                Ok(_valid_raw) => {
                    valid_clone.fetch_add(1, Ordering::Relaxed);
                }
                Err(_parse_err) => {
                    malformed_clone.fetch_add(1, Ordering::Relaxed);
                }
            }
        }
    });

    let test_vectors = vec![
        // Valid JSON chunks
        (true, "{\"token\": \"valid_1\"}"),
        (true, "{\"delta\": {\"content\": \"hello\"}}"),
        (true, "[1, 2, 3]"),
        (true, "true"),
        (true, "12345"),
        (true, "\"simple json string\""),
        // Corrupted / Malformed non-JSON chunks
        (false, ""),                           // Empty string
        (false, "   "),                        // Whitespace only
        (false, "RAW_UNQUOTED_TEXT_NOT_JSON"), // Plain text
        (false, "{\"unclosed_brace\": 1"),     // Truncated JSON
        (false, "{broken: json key without quotes}"),
        (false, "{\"trailing_comma\": 123,}"),
        (false, "\0\0\0\0binary_null_bytes"), // Binary nulls
        (false, "\x1b[31mANSI_ESCAPE_CODE\x1b[0m"), // ANSI sequences
        (false, "<xml>not_json</xml>"),       // XML/HTML
        (false, "{'single_quotes': 'invalid'}"), // Single quotes
        (false, "NaN"),                       // Invalid JSON token
        (false, "undefined"),                 // JS undefined
        (false, "{\"nested\": {\"deep\": "),  // Cutoff mid-stream
    ];

    let mut expected_valid = 0;
    let mut expected_malformed = 0;

    for (is_valid, payload) in test_vectors {
        if is_valid {
            expected_valid += 1;
        } else {
            expected_malformed += 1;
        }
        tx.send(payload.to_string()).await.unwrap();
    }

    // Interleave 100 valid chunks to ensure parser recovery remains healthy
    for i in 0..100 {
        expected_valid += 1;
        tx.send(format!("{{\"recovery_index\": {i}}}"))
            .await
            .unwrap();
    }

    // Interleave large 1MB non-JSON garbage chunk
    let giant_garbage = "A".repeat(1024 * 1024);
    expected_malformed += 1;
    tx.send(giant_garbage).await.unwrap();

    // Final valid chunk
    expected_valid += 1;
    tx.send("{\"stream_finished\": true}".to_string())
        .await
        .unwrap();

    drop(tx);
    forwarder
        .await
        .expect("Forwarder must not panic on malformed input");

    assert_eq!(valid_count.load(Ordering::SeqCst), expected_valid);
    assert_eq!(malformed_count.load(Ordering::SeqCst), expected_malformed);
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. IN-PROCESS IPC PERFORMANCE & RAPID INVOCATION STRESS
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_in_process_ipc_burst_1000_pings() {
    let state = test_state();
    let start = Instant::now();

    for _ in 0..1000 {
        let res = liva_native_core::handle_command_as(
            liva_native_core::CommandPrincipal::TauriWidget,
            state.clone(),
            "ping",
            serde_json::json!({}),
            None,
            None,
        )
        .await
        .expect("Ping must succeed");
        assert_eq!(res, serde_json::json!({"pong": true}));
    }

    let elapsed = start.elapsed();
    let per_call_micros = elapsed.as_micros() / 1000;
    println!("1,000 in-process IPC pings completed in {elapsed:?} (~{per_call_micros} µs/call)");
    assert!(
        per_call_micros < 500,
        "In-process ping must be sub-millisecond (got {per_call_micros} µs/call)"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. DEEP ADVERSARIAL RECURSION, BOUNDED BACKPRESSURE LEAK DEFENSE & CHURN
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_deeply_nested_json_recursion_and_boundary_fuzzing() {
    // 1. Test 1,000-level nested JSON object
    let mut nested_obj = String::with_capacity(4000);
    for _ in 0..1000 {
        nested_obj.push_str("{\"n\":");
    }
    nested_obj.push('1');
    for _ in 0..1000 {
        nested_obj.push('}');
    }

    // Must handle or reject gracefully without stack overflow or panic
    let res_obj = serde_json::value::RawValue::from_string(nested_obj);
    let _ = res_obj; // Ok or Err is fine; no panic or SIGSEGV is the requirement

    // 2. Test 10,000-level nested array
    let mut nested_arr = String::with_capacity(20000);
    for _ in 0..10000 {
        nested_arr.push('[');
    }
    nested_arr.push('0');
    for _ in 0..10000 {
        nested_arr.push(']');
    }
    let res_arr = serde_json::value::RawValue::from_string(nested_arr);
    let _ = res_arr;

    // 3. Test 100,000-char single token string
    let huge_token = format!("\"{}\"", "X".repeat(100_000));
    let res_huge = serde_json::value::RawValue::from_string(huge_token);
    assert!(
        res_huge.is_ok(),
        "RawValue must safely accept 100KB single JSON string token"
    );
}

#[tokio::test]
async fn challenge_backpressure_strict_buffer_bounds_during_slow_consumer() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let total_items = 3000;
    let produced = Arc::new(AtomicUsize::new(0));
    let consumed = Arc::new(AtomicUsize::new(0));

    let p_counter = produced.clone();
    let tx_clone = tx.clone();
    let producer_handle = tokio::spawn(async move {
        for i in 0..total_items {
            let msg = format!("{{\"seq\": {i}}}");
            tx_clone.send(msg).await.expect("Channel open");
            p_counter.fetch_add(1, Ordering::Relaxed);
        }
    });

    let c_counter = consumed.clone();
    let consumer_handle = tokio::spawn(async move {
        while let Some(_msg) = rx.recv().await {
            c_counter.fetch_add(1, Ordering::Relaxed);
            // Artificial micro-pause to simulate slow webview rendering
            tokio::task::yield_now().await;
        }
    });

    // Monitor that at no point does unconsumed backlog exceed bounded capacity (100) + 1
    let p_mon = produced.clone();
    let c_mon = consumed.clone();
    let monitor_handle = tokio::spawn(async move {
        while c_mon.load(Ordering::Relaxed) < total_items {
            let p = p_mon.load(Ordering::Relaxed);
            let c = c_mon.load(Ordering::Relaxed);
            let backlog = p.saturating_sub(c);
            assert!(
                backlog <= 101,
                "Backlog exceeded bounded capacity: {backlog} > 101 (p={p}, c={c})"
            );
            tokio::task::yield_now().await;
        }
    });

    producer_handle.await.expect("Producer done");
    drop(tx);
    consumer_handle.await.expect("Consumer done");
    monitor_handle.await.expect("Monitor done");

    assert_eq!(produced.load(Ordering::SeqCst), total_items);
    assert_eq!(consumed.load(Ordering::SeqCst), total_items);
}

#[tokio::test]
async fn challenge_voice_coordinator_concurrent_subscribe_unsubscribe_storm() {
    let state = test_state();
    let coordinator = Arc::new(
        liva_native_core::webrtc::voice_coordinator::VoiceCoordinator::new(
            state,
            "challenge-session".to_string(),
        )
        .await,
    );

    let iterations = 200;
    let mut handles = Vec::new();

    // 4 tasks rapidly subscribing and unsubscribing
    for _ in 0..4 {
        let coord = coordinator.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..iterations {
                let (tx, _rx) = mpsc::channel::<VoiceIpcEvent>(16);
                let sink = Arc::new(move |evt: VoiceIpcEvent| {
                    let _ = tx.try_send(evt);
                });
                coord.subscribe(sink);
                if i % 2 == 0 {
                    coord.unsubscribe();
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    // 4 tasks rapidly dispatching text events and mic chunks
    for t in 0..4 {
        let coord = coordinator.clone();
        handles.push(tokio::spawn(async move {
            for i in 0..iterations {
                coord.send_text_event(
                    "challenge_evt",
                    serde_json::json!({"thread": t, "iteration": i}),
                );
                let _ = coord.ingest_mic_chunk(i as u32, vec![0.01; 160]).await;
                tokio::task::yield_now().await;
            }
        }));
    }

    for h in handles {
        h.await.expect(
            "Concurrent subscribe/dispatch storm must finish cleanly without panic or deadlock",
        );
    }
}
