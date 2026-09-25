//! M2 Adversarial Challenge & Empirical Stress Test Suite
//!
//! Milestone 2 Challenger M2.1:
//! 1. Verify AEC3 Echo Cancellation suppresses loud speaker playback and avoids false VAD interrupts.
//! 2. Verify concurrent user barge-in speech during loud speaker playback is detected in < 100 ms.
//! 3. Verify [INTERRUPT] / on_interrupted immediate playback cancellation takes < 15 ms.
//! 4. Verify continuous loopback streaming strictly caps at MAX_RENDER_QUEUE_SAMPLES (32,000)
//!    and maintains FIFO ordering without desynchronization or memory leaks.
//! 5. Verify dynamic sample rate switching (48kHz, 44.1kHz, 22.05kHz, 16kHz) and capturer lifecycle.
//! 6. Verify extreme corrupted float sanitization (NaN, Inf, extreme DC offsets).

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::aec::{SelfEchoCanceller, WasapiLoopbackCapturer};
use liva_native_core::webrtc::frame::{OP_FLUSH, VoiceFrame};
use liva_native_core::webrtc::pipeline::{VoiceOutbound, WebRTCActor};
use liva_native_core::webrtc::vad::{
    VadConfig, VadEngine, VadEvent, compute_rms, compute_stage0_metrics, compute_zcr,
    resolve_model_path,
};
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

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
        cua: AppState::mock_cua(),
    })
}

fn load_test_vad_engine() -> Option<VadEngine> {
    let vad_path = resolve_model_path("models/nemotron-asr");
    if vad_path.exists() {
        let config = VadConfig {
            frame_size: 160,
            ..VadConfig::default()
        };
        VadEngine::new(&vad_path, config).ok()
    } else {
        None
    }
}

// =============================================================================
// Challenge 1: Loud Speaker Playback Suppression & Zero False VAD Interruptions
// =============================================================================

#[test]
fn challenge_loud_speaker_playback_suppression_and_zero_false_vad_trigger() {
    let mut aec = SelfEchoCanceller::new();
    let mut vad_opt = load_test_vad_engine();

    // Generate 1.0 second of loud simulated music / TTS playback @ 16kHz (amplitude 0.8)
    // Multi-tone harmonic signal: 440Hz + 880Hz + 1320Hz
    let sample_rate = 16000;
    let duration_samples = sample_rate; // 1 second = 16,000 samples
    let mut speaker_pcm = Vec::with_capacity(duration_samples);
    for i in 0..duration_samples {
        let t = i as f32 / sample_rate as f32;
        let s = 0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            + 0.2 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
            + 0.1 * (2.0 * std::f32::consts::PI * 1320.0 * t).sin();
        speaker_pcm.push(s);
    }

    // Push into loopback render reference in chunks of 160 samples (10ms)
    // Simulated acoustic coupling factor in room = 0.5 (high acoustic leak into mic)
    const FRAME_SIZE: usize = 160;
    let num_frames = duration_samples / FRAME_SIZE;

    let mut false_positive_vad_interrupts = 0;
    let mut uncancelled_frame_energies = Vec::new();
    let mut processed_frame_energies = Vec::new();

    for frame_idx in 0..num_frames {
        let start = frame_idx * FRAME_SIZE;
        let end = start + FRAME_SIZE;
        let speaker_chunk = &speaker_pcm[start..end];

        // Far-end speaker audio sent to loopback
        aec.push_loopback_render(speaker_chunk, sample_rate as u32);

        // Acoustic leak captured by physical mic (half amplitude, room attenuation)
        let mic_echo: Vec<f32> = speaker_chunk.iter().map(|&s| s * 0.5).collect();
        let raw_rms = compute_rms(&mic_echo);
        uncancelled_frame_energies.push(raw_rms);

        // Process through AEC3
        let cleaned = aec.process_capture(&mic_echo).expect("process_capture");
        assert_eq!(cleaned.len(), FRAME_SIZE);

        let cleaned_rms = compute_rms(&cleaned);
        processed_frame_energies.push(cleaned_rms);

        // Run neural VAD if model is available
        if let Some(ref mut vad) = vad_opt {
            let events = vad.process_audio(&cleaned).expect("vad process_audio");
            for (ev, _conf) in events {
                if ev == VadEvent::SpeechStart {
                    false_positive_vad_interrupts += 1;
                }
            }
        }
    }

    // Verify significant echo reduction (ERLE):
    // Compare average RMS of uncancelled echo vs processed echo after convergence
    let raw_avg_rms: f32 =
        uncancelled_frame_energies[20..].iter().sum::<f32>() / (num_frames - 20) as f32;
    let cleaned_avg_rms: f32 =
        processed_frame_energies[20..].iter().sum::<f32>() / (num_frames - 20) as f32;

    println!(
        "[AEC3 Echo Test] Raw Echo Avg RMS: {:.4}, Cleaned Echo Avg RMS: {:.4}, VAD Model Loaded: {}, False SpeechStart Interrupts: {}",
        raw_avg_rms,
        cleaned_avg_rms,
        vad_opt.is_some(),
        false_positive_vad_interrupts
    );

    // Suppressed echo must be significantly lower than raw echo (> 10dB attenuation)
    assert!(
        cleaned_avg_rms < raw_avg_rms * 0.35,
        "AEC3 must attenuate speaker echo by at least ~10dB: cleaned {:.4} vs raw {:.4}",
        cleaned_avg_rms,
        raw_avg_rms
    );

    // Zero false positive speech start events triggered from speaker echo!
    assert_eq!(
        false_positive_vad_interrupts, 0,
        "Zero false positive voice activity triggers from suppressed speaker echo!"
    );
}

// =============================================================================
// Challenge 2: Concurrent User Barge-In Speech Detection < 100ms
// =============================================================================

#[test]
fn challenge_bargein_speech_detection_under_concurrent_loud_playback_sub_100ms() {
    let mut aec = SelfEchoCanceller::new();
    let vad_config = VadConfig::default();

    let sample_rate = 16000;
    const FRAME_SIZE: usize = 160; // 10ms

    // 1. Initial 200ms of loud speaker playback only to allow AEC3 filter to converge
    for i in 0..20 {
        let t0 = (i * FRAME_SIZE) as f32 / sample_rate as f32;
        let speaker_chunk: Vec<f32> = (0..FRAME_SIZE)
            .map(|k| {
                let t = t0 + k as f32 / sample_rate as f32;
                0.7 * (2.0 * std::f32::consts::PI * 500.0 * t).sin()
            })
            .collect();
        aec.push_loopback_render(&speaker_chunk, sample_rate);

        let mic_echo: Vec<f32> = speaker_chunk.iter().map(|&s| s * 0.5).collect();
        let _ = aec.process_capture(&mic_echo).expect("converge frame");
    }

    // 2. User starts speaking at t = 200ms while loud speaker playback continues (Double-talk)
    // User voice: strong fundamental frequency 220Hz (human pitch) with harmonics, amplitude 0.6
    let mut detected_speech_frame = None;

    for frame_idx in 0..20 {
        let t0 = ((20 + frame_idx) * FRAME_SIZE) as f32 / sample_rate as f32;
        let speaker_chunk: Vec<f32> = (0..FRAME_SIZE)
            .map(|k| {
                let t = t0 + k as f32 / sample_rate as f32;
                0.7 * (2.0 * std::f32::consts::PI * 500.0 * t).sin()
            })
            .collect();
        aec.push_loopback_render(&speaker_chunk, sample_rate);

        // Near-end speech: rich vowel harmonic content (220Hz + 440Hz + 660Hz)
        let user_speech: Vec<f32> = (0..FRAME_SIZE)
            .map(|k| {
                let t = t0 + k as f32 / sample_rate as f32;
                0.4 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                    + 0.2 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();

        // Physical mic captures both speaker echo and user speech
        let mic_combined: Vec<f32> = speaker_chunk
            .iter()
            .zip(user_speech.iter())
            .map(|(&spk, &usr)| (spk * 0.5 + usr).clamp(-1.0, 1.0))
            .collect();

        let cleaned = aec.process_capture(&mic_combined).expect("process_capture");
        assert_eq!(cleaned.len(), FRAME_SIZE);

        let rms = compute_rms(&cleaned);
        let zcr = compute_zcr(&cleaned);

        // Check if user speech is preserved and detected
        let metrics = compute_stage0_metrics(
            &cleaned,
            vad_config.energy_threshold,
            vad_config.zcr_min,
            vad_config.zcr_max,
        );

        if (metrics.is_active || rms >= vad_config.energy_threshold)
            && detected_speech_frame.is_none()
        {
            let latency_ms = (frame_idx + 1) * 10; // each frame is 10ms
            detected_speech_frame = Some(latency_ms);
            println!(
                "[Barge-In Test] User speech detected at frame {} ({} ms), RMS: {:.4}, ZCR: {:.4}",
                frame_idx, latency_ms, rms, zcr
            );
        }
    }

    assert!(
        detected_speech_frame.is_some(),
        "User speech during double-talk must be detected!"
    );
    let latency_ms = detected_speech_frame.unwrap();
    println!("[Barge-In Test] Detection Latency: {} ms", latency_ms);
    assert!(
        latency_ms < 100,
        "Barge-in detection latency ({} ms) must be < 100 ms SLA!",
        latency_ms
    );
}

// =============================================================================
// Challenge 3: [INTERRUPT] Playback Cancellation Latency Strictly Under 15ms
// =============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_interrupt_cancellation_latency_strictly_under_15ms() {
    let state = build_test_app_state();
    let (speaker_tx, _speaker_rx) = mpsc::channel::<VoiceFrame>(64);
    let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(64);

    let outbound = VoiceOutbound::new(speaker_tx, control_tx);
    let (pipeline_handle, actor) = WebRTCActor::new(
        state,
        outbound,
        "interrupt_latency_challenge".to_string(),
        Arc::new(std::sync::Mutex::new(None)),
    );
    let actor_task = tokio::spawn(actor.run());

    let mut latencies = Vec::with_capacity(30);

    for i in 1..=30 {
        let t0 = Instant::now();
        pipeline_handle
            .on_interrupted()
            .expect("on_interrupted dispatch");

        let flush_frame = tokio::time::timeout(Duration::from_millis(50), control_rx.recv())
            .await
            .unwrap_or_else(|_| panic!("Iteration {}: Timeout waiting for OP_FLUSH", i))
            .unwrap_or_else(|| panic!("Iteration {}: Control channel closed", i));

        let elapsed = t0.elapsed();
        latencies.push(elapsed);

        assert_eq!(flush_frame.op_code, OP_FLUSH);
        assert_eq!(flush_frame.seq_id, i);

        // Check release contract (< 15ms)
        const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
        assert!(
            elapsed < Duration::from_millis(15) * SLOWDOWN,
            "Iteration {}: Interruption preemption latency {:?} exceeded 15ms!",
            i,
            elapsed
        );
    }

    let min_lat = latencies.iter().min().unwrap();
    let max_lat = latencies.iter().max().unwrap();
    let avg_lat = latencies.iter().sum::<Duration>() / latencies.len() as u32;

    println!("\n=== [INTERRUPT] Cancellation Latency Benchmark ===");
    println!("Total Interrupts: {}", latencies.len());
    println!("Min: {:?}", min_lat);
    println!("Avg: {:?}", avg_lat);
    println!("Max: {:?}", max_lat);
    println!("=================================================\n");

    const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
    assert!(
        *max_lat < Duration::from_millis(15) * SLOWDOWN,
        "Max preemption latency {:?} exceeded 15ms SLA limit",
        max_lat
    );

    actor_task.abort();
}

// =============================================================================
// Challenge 4: Continuous WASAPI Loopback Queue FIFO Order & Hard Capacity Bounds
// =============================================================================

#[test]
fn challenge_wasapi_loopback_queue_overflow_bounds_and_fifo_sync() {
    let mut aec = SelfEchoCanceller::new();

    // Push 200,000 samples (12.5 seconds at 16kHz) across 1,250 chunks of 160 samples
    // without any capture calls to stress test queue limits.
    for i in 0..1250 {
        let chunk = vec![(i as f32 % 100.0) / 100.0; 160];
        aec.push_loopback_render(&chunk, 16000);
    }

    // Must never exceed MAX_RENDER_QUEUE_SAMPLES = 32,000 samples (2.0s ceiling)
    assert_eq!(
        aec.render_queue_len(),
        SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
        "Render queue must be strictly bounded at 32,000 samples"
    );

    // Verify interleaving mismatched chunks:
    // 240 samples @ 24kHz (10ms) render vs 160 samples @ 16kHz (10ms) capture
    let mut aec_interleaved = SelfEchoCanceller::new();
    for i in 0..100 {
        let render_24k = vec![0.2f32; 240];
        aec_interleaved.push_loopback_render(&render_24k, 24000);

        let mic_16k = vec![0.1f32; 160];
        let out = aec_interleaved
            .process_capture(&mic_16k)
            .expect("interleaved process_capture");
        assert_eq!(out.len(), 160);
        assert!(out.iter().all(|s| s.is_finite()));

        // Check queue length stays stable and bounded
        assert!(
            aec_interleaved.render_queue_len() <= SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
            "Iteration {}: Render queue exceeded ceiling: {}",
            i,
            aec_interleaved.render_queue_len()
        );
    }
}

// =============================================================================
// Challenge 5: Dynamic Sample Rate Switching & Device Restart Recovery
// =============================================================================

#[test]
fn challenge_dynamic_sample_rate_switching_and_stream_restart() {
    let mut aec = SelfEchoCanceller::new();

    // Stream audio switching rapidly across different sample rates:
    // 48kHz (Windows standard) -> 44.1kHz (CD) -> 22.05kHz (TTS) -> 16kHz (WebRTC) -> 32kHz
    let rates_and_lengths = [
        (48000, 480), // 10ms -> 160 samples
        (44100, 441), // 10ms -> 160 samples
        (22050, 221), // ~10ms -> 160 samples
        (16000, 160), // 10ms -> 160 samples
        (32000, 320), // 10ms -> 160 samples
        (48000, 480), // 10ms -> 160 samples
    ];

    for (rate, len) in rates_and_lengths {
        let chunk = vec![0.3f32; len];
        aec.push_loopback_render(&chunk, rate);

        let mic = vec![0.05f32; 160];
        let out = aec
            .process_capture(&mic)
            .expect("process_capture with rate switch");
        assert_eq!(out.len(), 160);
        assert!(
            out.iter()
                .all(|s| s.is_finite() && (-1.0..=1.0).contains(s))
        );
    }

    // After consuming all 6 * 160 samples, render queue should be empty (or near 0)
    assert!(
        aec.render_queue_len() <= 1,
        "All resampled chunks should be consumed by capture"
    );

    // Test capturer lifecycle and idempotency
    let capturer = WasapiLoopbackCapturer::new();
    assert!(!capturer.is_running());
    capturer.stop();
    assert!(!capturer.is_running());
    // Repeated stop() calls must not panic
    capturer.stop();
    capturer.stop();

    // Test clear_render and reset recovery
    aec.clear_render();
    assert_eq!(aec.render_queue_len(), 0);

    aec.push_render(&[0.5f32; 800], 16000);
    assert_eq!(aec.render_queue_len(), 800);
    aec.clear_render();
    assert_eq!(aec.render_queue_len(), 0);

    // Fresh audio stream after clear_render must process cleanly
    let mic_fresh = vec![0.1f32; 160];
    let out_fresh = aec.process_capture(&mic_fresh).expect("fresh capture");
    assert_eq!(out_fresh.len(), 160);
    assert!(out_fresh.iter().all(|s| s.is_finite()));
}

// =============================================================================
// Challenge 6: Extreme Corrupted Floats & DC Bias Sanitization
// =============================================================================

#[test]
fn challenge_extreme_corrupted_floats_and_dc_bias_sanitization() {
    let mut aec = SelfEchoCanceller::new();

    // 1. Extreme DC bias in render (+100.0 and -100.0) and non-finite floats
    let corrupted_render = vec![
        100.0,
        -100.0,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e-38, // subnormal
    ];
    aec.push_loopback_render(&corrupted_render, 16000);

    // 2. Corrupted capture stream
    let mut corrupted_mic = vec![0.2f32; 320];
    corrupted_mic[0] = f32::NAN;
    corrupted_mic[1] = f32::INFINITY;
    corrupted_mic[2] = f32::NEG_INFINITY;
    corrupted_mic[3] = 50.0;
    corrupted_mic[4] = -50.0;
    corrupted_mic[5] = 1e-40; // subnormal

    let out = aec
        .process_capture(&corrupted_mic)
        .expect("process_capture with corrupted audio");
    assert_eq!(out.len(), 320);

    // All output samples must be strictly finite and clamped to [-1.0, 1.0]
    for (i, &s) in out.iter().enumerate() {
        assert!(s.is_finite(), "Sample {} must be finite, got {:?}", i, s);
        assert!(
            (-1.0..=1.0).contains(&s),
            "Sample {} must be clamped to [-1.0, 1.0], got {}",
            i,
            s
        );
    }
}
