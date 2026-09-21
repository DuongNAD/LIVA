//! Empirical Adversarial Stress Suite for VoiceCoordinator (Milestone 2 - Requirement R1)
//!
//! Tests include:
//! 1. Rapid burst stream flood (1,000 consecutive chunks) without deadlocks or panic.
//! 2. Concurrent multi-producer flood (10 parallel tasks streaming audio simultaneously).
//! 3. High-frequency interruption & barge-in collision while streaming active audio.
//! 4. Adversarial audio payloads: empty chunks, massive chunks, NaN/Infinity/extreme values.
//! 5. Concurrent subscriber churn (subscribe/unsubscribe storm under active audio stream).
//! 6. Wake probe duration boundary checks and concurrent probe evaluations.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::voice_coordinator::{VoiceCoordinator, VoiceIpcEvent};
use liva_native_core::{AppState, db, stt, tts};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;
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
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. RAPID BURST STREAM FLOOD (1,000 CHUNKS)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_rapid_burst_stream_flood_1000_chunks() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "burst-flood-conv".to_string()).await);

    let event_count = Arc::new(AtomicUsize::new(0));
    let ec = event_count.clone();
    coordinator.subscribe(Arc::new(move |_evt| {
        ec.fetch_add(1, Ordering::Relaxed);
    }));

    // Ingest 1,000 audio chunks (160 samples each = 10ms frame at 16kHz)
    let chunk = vec![0.05_f32; 160];
    let start = std::time::Instant::now();

    for seq in 0..1000 {
        let res = coordinator.ingest_mic_chunk(seq, chunk.clone()).await;
        assert!(res.is_ok(), "chunk {seq} ingestion failed: {:?}", res.err());
    }

    let elapsed = start.elapsed();
    println!("Ingested 1,000 chunks in {elapsed:?}");

    // Also dispatch text events during flood
    for i in 0..50 {
        coordinator.send_text_event("ping_during_flood", serde_json::json!({ "i": i }));
    }

    coordinator.unsubscribe();
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. CONCURRENT MULTI-PRODUCER AUDIO FLOOD (10 TASKS × 100 CHUNKS)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_concurrent_multi_producer_flood() {
    let state = test_state();
    let coordinator =
        Arc::new(VoiceCoordinator::new(state, "multi-producer-conv".to_string()).await);

    let mut handles = Vec::new();
    const NUM_TASKS: usize = 10;
    const CHUNKS_PER_TASK: usize = 100;

    for task_id in 0..NUM_TASKS {
        let coord = coordinator.clone();
        handles.push(tokio::spawn(async move {
            let chunk = vec![0.02_f32; 160];
            for i in 0..CHUNKS_PER_TASK {
                let seq = (task_id * CHUNKS_PER_TASK + i) as u32;
                let res = coord.ingest_mic_chunk(seq, chunk.clone()).await;
                assert!(res.is_ok(), "task {task_id} chunk {i} failed");
            }
        }));
    }

    for h in handles {
        h.await.expect("worker task panicked");
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. HIGH-FREQUENCY INTERRUPTION & BARGE-IN COLLISION UNDER ACTIVE STREAM
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_interruption_barge_in_collision_under_active_stream() {
    let state = test_state();
    let coordinator =
        Arc::new(VoiceCoordinator::new(state, "bargein-collision-conv".to_string()).await);

    let (flush_tx, _flush_rx) = mpsc::channel(128);
    coordinator.subscribe(Arc::new(move |evt| {
        if let VoiceIpcEvent::Flush { seq_id } = evt {
            let _ = flush_tx.try_send(seq_id);
        }
    }));

    let stop_signal = Arc::new(AtomicBool::new(false));

    // Streamer task feeding chunks continuously
    let coord_stream = coordinator.clone();
    let stop_stream = stop_signal.clone();
    let stream_handle = tokio::spawn(async move {
        let chunk = vec![0.3_f32; 160];
        let mut seq = 0u32;
        while !stop_stream.load(Ordering::Relaxed) && seq < 1500 {
            let _ = coord_stream.ingest_mic_chunk(seq, chunk.clone()).await;
            seq += 1;
            tokio::task::yield_now().await;
        }
        seq
    });

    // Interrupter task triggering barge-in cutoffs
    let coord_interrupt = coordinator.clone();
    let stop_interrupt = stop_signal.clone();
    let interrupt_handle = tokio::spawn(async move {
        let mut interrupt_count = 0usize;
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(5)).await;
            if coord_interrupt.interrupt().is_ok() {
                interrupt_count += 1;
            }
        }
        stop_interrupt.store(true, Ordering::Relaxed);
        interrupt_count
    });

    let total_chunks = stream_handle.await.expect("streamer finished");
    let total_interrupts = interrupt_handle.await.expect("interrupter finished");

    println!("Completed stream with {total_chunks} chunks and {total_interrupts} interruptions");
    assert!(total_chunks >= 50, "streamer should have processed chunks");
    assert!(total_interrupts > 0, "interrupter should have run");

    coordinator.unsubscribe();
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. ADVERSARIAL AUDIO PAYLOADS: EMPTY, MASSIVE, NAN, INFINITY, EXTREMES
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_adversarial_audio_payloads_handling() {
    let state = test_state();
    let coordinator =
        Arc::new(VoiceCoordinator::new(state, "adversarial-payloads-conv".to_string()).await);

    // 1. Empty audio chunk: must return Ok(()) without error
    let empty_res = coordinator.ingest_mic_chunk(1, vec![]).await;
    assert!(empty_res.is_ok(), "Empty audio chunk must return Ok(())");

    // 2. Huge audio chunk (64,000 samples = 4 seconds of audio)
    let large_chunk = vec![0.1_f32; 64_000];
    let large_res = coordinator.ingest_mic_chunk(2, large_chunk).await;
    assert!(large_res.is_ok(), "Large chunk must be handled safely");

    // 3. Audio chunk containing NaN and +/-Infinity
    let nan_chunk = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 0.0, -0.5, 0.5];
    let nan_res = coordinator.ingest_mic_chunk(3, nan_chunk).await;
    // Should handle gracefully without crashing/panicking the process
    assert!(
        nan_res.is_ok(),
        "NaN/Inf chunk must be ingested without panic"
    );

    // 4. Extreme amplitude values (+1000.0, -1000.0)
    let extreme_chunk = vec![1000.0_f32; 160];
    let extreme_res = coordinator.ingest_mic_chunk(4, extreme_chunk).await;
    assert!(
        extreme_res.is_ok(),
        "Extreme amplitude must be handled safely"
    );

    // 5. Subnormal numbers
    let subnormal_chunk = vec![f32::MIN_POSITIVE / 2.0; 160];
    let subnormal_res = coordinator.ingest_mic_chunk(5, subnormal_chunk).await;
    assert!(
        subnormal_res.is_ok(),
        "Subnormal audio chunk must be handled safely"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. CONCURRENT SUBSCRIBER CHURN UNDER ACTIVE STREAM
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_subscriber_churn_under_active_stream() {
    let state = test_state();
    let coordinator =
        Arc::new(VoiceCoordinator::new(state, "subscriber-churn-conv".to_string()).await);

    let stop_flag = Arc::new(AtomicBool::new(false));

    // Producer feeding chunks
    let coord_prod = coordinator.clone();
    let stop_prod = stop_flag.clone();
    let producer = tokio::spawn(async move {
        let chunk = vec![0.05_f32; 160];
        let mut i = 0u32;
        while !stop_prod.load(Ordering::Relaxed) && i < 500 {
            let _ = coord_prod.ingest_mic_chunk(i, chunk.clone()).await;
            coord_prod.send_text_event("churn_event", serde_json::json!({ "idx": i }));
            i += 1;
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    });

    // Subscriber churner rapidly subscribing and unsubscribing
    let coord_churn = coordinator.clone();
    let churner = tokio::spawn(async move {
        for _cycle in 0..50 {
            let cycle_count = Arc::new(AtomicUsize::new(0));
            let cc = cycle_count.clone();
            coord_churn.subscribe(Arc::new(move |_| {
                cc.fetch_add(1, Ordering::Relaxed);
            }));
            tokio::time::sleep(Duration::from_micros(500)).await;
            coord_churn.unsubscribe();
        }
    });

    churner.await.expect("churner finished cleanly");
    stop_flag.store(true, Ordering::Relaxed);
    producer.await.expect("producer finished cleanly");
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. WAKE PROBE DURATION BOUNDARIES & CONCURRENT EVALUATIONS
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_wake_probe_boundaries_and_concurrency() {
    let state = test_state();
    let coordinator =
        Arc::new(VoiceCoordinator::new(state, "wake-boundary-conv".to_string()).await);

    // 1. Too short: < 0.3s (< 4800 samples at 16kHz)
    let short_samples = vec![0.0_f32; 2000]; // 0.125s
    let resp = coordinator
        .evaluate_wake_probe(1, short_samples)
        .await
        .expect("probe ok");
    assert!(!resp.matched);
    assert_eq!(resp.tier, "length_rejected");
    assert_eq!(resp.seq_id, 1);

    // 2. Too long: > 4.0s (> 64000 samples at 16kHz)
    let long_samples = vec![0.0_f32; 65000]; // ~4.06s
    let resp_long = coordinator
        .evaluate_wake_probe(2, long_samples)
        .await
        .expect("probe ok");
    assert!(!resp_long.matched);
    assert_eq!(resp_long.tier, "length_rejected");
    assert_eq!(resp_long.seq_id, 2);

    // 3. Exactly minimum duration: 0.3s = 4800 samples
    let min_samples = vec![0.0_f32; 4800];
    let resp_min = coordinator
        .evaluate_wake_probe(3, min_samples)
        .await
        .expect("probe ok");
    // Not length rejected
    assert_ne!(resp_min.tier, "length_rejected");

    // 4. Exactly maximum duration: 4.0s = 64000 samples
    let max_samples = vec![0.0_f32; 64000];
    let resp_max = coordinator
        .evaluate_wake_probe(4, max_samples)
        .await
        .expect("probe ok");
    assert_ne!(resp_max.tier, "length_rejected");

    // 5. Concurrent wake probes while audio is streaming
    let mut probe_handles = Vec::new();
    for i in 10..20 {
        let coord = coordinator.clone();
        probe_handles.push(tokio::spawn(async move {
            let samples = vec![0.0_f32; 8000]; // 0.5s
            coord.evaluate_wake_probe(i, samples).await
        }));
    }

    for h in probe_handles {
        let res = h
            .await
            .expect("probe task panicked")
            .expect("probe returned error");
        assert!(!res.matched);
    }
}
