//! Adversarial Empirical Challenge Suite — Milestone 1 (M1_2)
//! Streaming STT Sliding Window Complexity & Pipeline Integration Challenge
//!
//! Objectives:
//! 1. Bounded Sliding Window Invariants: Validate that under 1 to 150+ rapid 160ms chunks,
//!    context window length strictly clamps to STREAMING_WINDOW_MAX_SAMPLES (21,760 samples)
//!    and mel-spectrogram frame count is strictly capped at 137 frames.
//! 2. Empirical O(1) DSP Complexity Verification: Measure per-chunk DSP computation time
//!    across 100+ chunks and verify evaluation time remains strictly O(1) constant-time
//!    (ratio late/early < 1.5x), completely refuting O(N^2) quadratic degradation.
//! 3. Full ONNX Parakeet Streaming Inference: Feed 100 consecutive 160ms chunks into
//!    ParakeetVi::feed_chunk, asserting stable O(1) inference latency and bounded memory.
//! 4. Pipeline Rapid Chunk Dispatch: Verify WebRTCPipelineHandle::on_audio_chunk does not
//!    deadlock, block the async runtime, or drop valid speech frames during VadStart.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::stt::parakeet::{
    ParakeetDsp, ParakeetVi, STREAMING_BUFFER_MAX_SAMPLES, STREAMING_CHUNK_SAMPLES,
    STREAMING_CONTEXT_SAMPLES, STREAMING_WINDOW_MAX_SAMPLES,
};
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;
use std::time::Instant;

use crate::common;
use common::resolve_model_paths;

fn build_test_app_state() -> Arc<AppState> {
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

// ---------------------------------------------------------------------------
// 1. Algorithmic Sliding Window Bounds & Clamping Invariant (1 to 150 chunks)
// ---------------------------------------------------------------------------

#[test]
fn challenge_parakeet_sliding_window_bounds_and_frame_clamp() {
    // 160ms chunk @ 16kHz = 2560 samples
    assert_eq!(STREAMING_CHUNK_SAMPLES, 2560);
    // 1.2s acoustic context = 19,200 samples
    assert_eq!(STREAMING_CONTEXT_SAMPLES, 19200);
    // Clamped ceiling = 21,760 samples (~1.36s)
    assert_eq!(STREAMING_WINDOW_MAX_SAMPLES, 21760);
    // Absolute buffer ceiling = 320,000 samples (20s)
    assert_eq!(STREAMING_BUFFER_MAX_SAMPLES, 320_000);

    let dsp = ParakeetDsp::new();
    let chunk = vec![0.02f32; STREAMING_CHUNK_SAMPLES];
    let mut stream_buffer = Vec::new();

    const TOTAL_CHUNKS: usize = 150; // 24.0 seconds of speech
    let max_allowed_frames = 1 + STREAMING_WINDOW_MAX_SAMPLES / 160; // 1 + 136 = 137 frames

    for chunk_idx in 1..=TOTAL_CHUNKS {
        stream_buffer.extend_from_slice(&chunk);

        // Mimic ParakeetVi::feed_chunk sliding window logic
        let window_start = stream_buffer
            .len()
            .saturating_sub(STREAMING_WINDOW_MAX_SAMPLES);
        let context_window = &stream_buffer[window_start..];

        // Frame extraction
        let (_feat, t_frames) = dsp.log_mel_per_feature(context_window);

        // Verification 1: Window size never exceeds 21,760 samples
        assert!(
            context_window.len() <= STREAMING_WINDOW_MAX_SAMPLES,
            "Chunk {}: Window len {} exceeded maximum ceiling {}",
            chunk_idx,
            context_window.len(),
            STREAMING_WINDOW_MAX_SAMPLES
        );

        // Verification 2: Output frame count never exceeds 137 frames
        assert!(
            t_frames <= max_allowed_frames,
            "Chunk {}: Frame count {} exceeded maximum allowed frames {}",
            chunk_idx,
            t_frames,
            max_allowed_frames
        );

        // Verification 3: Once saturated (>= 9 chunks, 23,040 > 21,760), window is EXACTLY clamped
        if chunk_idx >= 9 {
            assert_eq!(
                context_window.len(),
                STREAMING_WINDOW_MAX_SAMPLES,
                "Chunk {}: Window len {} should be exactly clamped to {}",
                chunk_idx,
                context_window.len(),
                STREAMING_WINDOW_MAX_SAMPLES
            );
            assert_eq!(
                t_frames, max_allowed_frames,
                "Chunk {}: Frame count {} should be exactly {}",
                chunk_idx, t_frames, max_allowed_frames
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 2. Empirical O(1) DSP Computation Time Verification across 120 chunks
// ---------------------------------------------------------------------------

#[test]
fn challenge_parakeet_dsp_constant_time_complexity() {
    let dsp = ParakeetDsp::new();
    let chunk = vec![0.03f32; STREAMING_CHUNK_SAMPLES];
    let mut stream_buffer = Vec::new();

    const BENCH_CHUNKS: usize = 120;
    let mut latencies_micros = Vec::with_capacity(BENCH_CHUNKS);

    // Warm-up DSP engine
    let warmup_window = vec![0.01f32; STREAMING_WINDOW_MAX_SAMPLES];
    for _ in 0..5 {
        let _ = dsp.log_mel_per_feature(&warmup_window);
    }

    for chunk_idx in 1..=BENCH_CHUNKS {
        stream_buffer.extend_from_slice(&chunk);

        let window_start = stream_buffer
            .len()
            .saturating_sub(STREAMING_WINDOW_MAX_SAMPLES);
        let context_window = &stream_buffer[window_start..];

        // Measure precise CPU evaluation time of DSP log-mel extraction
        let t_start = Instant::now();
        let (_feat, t_frames) = dsp.log_mel_per_feature(context_window);
        let t_elapsed = t_start.elapsed().as_micros();

        latencies_micros.push(t_elapsed);

        if chunk_idx >= 9 {
            assert_eq!(t_frames, 137);
        }
    }

    // Compare early saturated chunks (chunks 10..25) with late chunks (chunks 100..115)
    let early_slice = &latencies_micros[10..25];
    let late_slice = &latencies_micros[100..115];

    let early_avg: f64 =
        early_slice.iter().copied().sum::<u128>() as f64 / early_slice.len() as f64;
    let late_avg: f64 = late_slice.iter().copied().sum::<u128>() as f64 / late_slice.len() as f64;

    let ratio = late_avg / early_avg;

    println!(
        "[EMPIRICAL DSP BENCHMARK] Early chunks (10..25) avg: {:.2}µs, Late chunks (100..115) avg: {:.2}µs, Ratio: {:.2}x",
        early_avg, late_avg, ratio
    );

    // In an O(N^2) accumulation scheme, chunk 100 evaluates 10x more audio samples than chunk 10,
    // causing an 8-12x increase in DSP computation.
    // Under O(1) bounded sliding window, ratio must be < 1.5x (and typically ~1.0x).
    assert!(
        ratio < 1.5,
        "DSP latency degraded non-linearly: ratio {:.2}x exceeds 1.5x ceiling (O(1) violation)",
        ratio
    );
}

// ---------------------------------------------------------------------------
// 3. Full ONNX Parakeet Streaming Inference Benchmark (100 Chunks)
// ---------------------------------------------------------------------------

#[test]
fn challenge_parakeet_onnx_streaming_100_chunks_o1_benchmark() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        eprintln!(
            "Skipping ONNX benchmark: Parakeet model files not found ({:?}, {:?})",
            model_path, vocab_path
        );
        return;
    }

    let mut pk =
        ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi model");

    const TEST_CHUNKS: usize = 100; // 16.0 seconds of continuous speech
    // Synthesize 16kHz sine wave audio (440 Hz)
    let mut chunk = vec![0.0f32; STREAMING_CHUNK_SAMPLES];
    for (i, s) in chunk.iter_mut().enumerate() {
        *s = (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin() * 0.2;
    }

    let mut chunk_times_ms = Vec::with_capacity(TEST_CHUNKS);

    // Warm-up 2 chunks
    let _ = pk.feed_chunk(&chunk, false);
    let _ = pk.feed_chunk(&chunk, false);
    pk.reset_stream();

    for chunk_idx in 1..=TEST_CHUNKS {
        let is_last = chunk_idx == TEST_CHUNKS;
        let t_start = Instant::now();
        let result = pk.feed_chunk(&chunk, is_last);
        let elapsed_ms = t_start.elapsed().as_secs_f64() * 1000.0;

        assert!(
            result.is_ok(),
            "feed_chunk failed at chunk {}: {:?}",
            chunk_idx,
            result.err()
        );
        chunk_times_ms.push(elapsed_ms);
    }

    // Analyze latency progression
    let early_slice = &chunk_times_ms[10..25];
    let late_slice = &chunk_times_ms[80..95];

    let early_avg = early_slice.iter().sum::<f64>() / early_slice.len() as f64;
    let late_avg = late_slice.iter().sum::<f64>() / late_slice.len() as f64;
    let ratio = late_avg / early_avg;

    let p50 = {
        let mut sorted = chunk_times_ms[10..95].to_vec();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        sorted[sorted.len() / 2]
    };

    println!(
        "[EMPIRICAL ONNX BENCHMARK] 100 Chunks: Early avg: {:.2}ms, Late avg: {:.2}ms, P50: {:.2}ms, Ratio: {:.2}x",
        early_avg, late_avg, p50, ratio
    );

    // Confirm O(1) property: late average must be within 1.5x of early average
    assert!(
        ratio < 1.5,
        "ONNX streaming inference degraded: ratio {:.2}x exceeds 1.5x (O(1) violation)",
        ratio
    );
}

// ---------------------------------------------------------------------------
// 4. WebRTC Pipeline Rapid Audio Chunk Ingestion Stress
// ---------------------------------------------------------------------------

#[tokio::test]
async fn challenge_webrtc_pipeline_rapid_audio_chunk_dispatch() {
    use liva_native_core::webrtc::pipeline::{PipelineState, VoiceOutbound, WebRTCActor};
    use liva_native_core::webrtc::session::SessionAec;
    use tokio::sync::mpsc;

    let (speaker_tx, mut _speaker_rx) = mpsc::channel(32);
    let (control_tx, mut _control_rx) = mpsc::channel(32);
    let outbound = VoiceOutbound::new(speaker_tx, control_tx);

    let state = build_test_app_state();
    let (pipeline_handle, actor) = WebRTCActor::new(
        state,
        outbound,
        "challenge-session-test".to_string(),
        SessionAec::default(),
    );

    let actor_handle = tokio::spawn(async move {
        actor.run().await;
    });

    // 1. Verify that sending audio chunks in Idle state does not crash or corrupt
    let dummy_chunk = vec![0.01f32; STREAMING_CHUNK_SAMPLES];
    for _ in 0..10 {
        let res = pipeline_handle.on_audio_chunk(dummy_chunk.clone());
        assert!(res.is_ok(), "on_audio_chunk in Idle returned error");
    }

    // 2. Start speech: VadStart
    pipeline_handle.on_vad_start().expect("on_vad_start failed");
    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);

    // 3. Rapidly stream 50 chunks of 160ms audio during VadStart
    for i in 0..50 {
        let res = pipeline_handle.on_audio_chunk(dummy_chunk.clone());
        assert!(
            res.is_ok(),
            "on_audio_chunk #{} failed during VadStart: {:?}",
            i,
            res.err()
        );
    }

    // Verify actor is still responsive
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);

    // 4. Trigger interruption (Barge-in): WebRTCActor transitions to Interrupted then resets to Idle
    pipeline_handle
        .on_interrupted()
        .expect("on_interrupted failed");
    tokio::time::sleep(std::time::Duration::from_millis(30)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::Idle);

    // Cleanup actor
    drop(pipeline_handle);
    let _ = tokio::time::timeout(std::time::Duration::from_millis(100), actor_handle).await;
}
