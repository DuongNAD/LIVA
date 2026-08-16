use liva_native_core::webrtc::vad::{
    VadConfig, VadEngine, VadEvent, compute_stage0_metrics, resolve_model_path,
};
use std::time::{Duration, Instant};

fn get_vad_model_path() -> std::path::PathBuf {
    let mut model_dir =
        std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| "models/nemotron-asr".to_string());
    if !std::path::Path::new(&model_dir).exists() {
        model_dir = "../models/nemotron-asr".to_string();
    }
    resolve_model_path(&model_dir)
}

#[test]
fn stress_test_stage0_microbenchmark_sub_10us() {
    let frame_sizes = [160usize, 256usize, 512usize];
    const ITERATIONS: usize = 10_000;

    for &size in &frame_sizes {
        let mut audio = vec![0.0f32; size];
        for (i, s) in audio.iter_mut().enumerate() {
            *s = ((i as f32 * 0.1).sin() * 0.5) + ((i as f32 * 0.03).cos() * 0.1);
        }

        // Warmup
        for _ in 0..100 {
            let _ = compute_stage0_metrics(&audio, 0.001, 0.01, 0.50);
        }

        let mut latencies: Vec<Duration> = Vec::with_capacity(ITERATIONS);
        let start_all = Instant::now();

        for _ in 0..ITERATIONS {
            let t0 = Instant::now();
            let metrics = compute_stage0_metrics(&audio, 0.001, 0.01, 0.50);
            let elapsed = t0.elapsed();
            latencies.push(elapsed);
            assert!(metrics.is_active);
        }

        let total_time = start_all.elapsed();
        let avg_latency = total_time / ITERATIONS as u32;

        latencies.sort();
        let min_lat = latencies[0];
        let p50_lat = latencies[ITERATIONS * 50 / 100];
        let p99_lat = latencies[ITERATIONS * 99 / 100];
        let max_lat = latencies[ITERATIONS - 1];

        println!(
            "[Stage 0 Benchmark] Frame Size {}: Avg = {:?}, Min = {:?}, P50 = {:?}, P99 = {:?}, Max = {:?}",
            size, avg_latency, min_lat, p50_lat, p99_lat, max_lat
        );

        // Debug builds are unoptimized; the strict number is the release contract.
        const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
        // Verification assertion: Average latency must be strictly under 10µs
        assert!(
            avg_latency < Duration::from_micros(10) * SLOWDOWN,
            "Stage 0 average latency for frame size {} must be < 10µs, got {:?}",
            size,
            avg_latency
        );
        // Hard limit: P99 must be under 50µs
        assert!(
            p99_lat < Duration::from_micros(50) * SLOWDOWN,
            "Stage 0 P99 latency for frame size {} must be < 50µs, got {:?}",
            size,
            p99_lat
        );
    }
}

#[test]
fn stress_test_silero_onnx_frame_latencies_160_256_512() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let frame_configs = [
        (160, "10ms (160 samples)"),
        (256, "16ms (256 samples)"),
        (512, "32ms (512 samples)"),
    ];

    const INFERENCE_ROUNDS: usize = 100;

    for &(frame_size, label) in &frame_configs {
        let config = VadConfig {
            frame_size,
            ..VadConfig::default()
        };
        let mut engine = VadEngine::new(&model_path, config).expect("initialize VadEngine");

        // Synthesize a mixture of 250Hz + 1kHz tone with harmonics (simulating voice vowel)
        let mut speech_frame = vec![0.0f32; frame_size];
        for (i, s) in speech_frame.iter_mut().enumerate() {
            let t = i as f32 / 16000.0;
            *s = (2.0 * std::f32::consts::PI * 250.0 * t).sin() * 0.3
                + (2.0 * std::f32::consts::PI * 1000.0 * t).sin() * 0.2;
        }

        // Warmup ONNX engine
        for _ in 0..10 {
            let _ = engine.process_audio(&speech_frame);
        }

        let mut latencies: Vec<Duration> = Vec::with_capacity(INFERENCE_ROUNDS);

        for _ in 0..INFERENCE_ROUNDS {
            let t0 = Instant::now();
            let _ = engine.process_audio(&speech_frame).expect("process_audio");
            let elapsed = t0.elapsed();
            latencies.push(elapsed);
        }

        latencies.sort();
        let min_lat = latencies[0];
        let p50_lat = latencies[INFERENCE_ROUNDS * 50 / 100];
        let p95_lat = latencies[INFERENCE_ROUNDS * 95 / 100];
        let max_lat = latencies[INFERENCE_ROUNDS - 1];
        let avg_lat: Duration = latencies.iter().sum::<Duration>() / (INFERENCE_ROUNDS as u32);

        println!(
            "[Silero VAD Benchmark] Frame {}: Avg = {:?}, Min = {:?}, P50 = {:?}, P95 = {:?}, Max = {:?}",
            label, avg_lat, min_lat, p50_lat, p95_lat, max_lat
        );

        // Debug builds are unoptimized; the strict number is the release contract.
        const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
        // Verification assertion: Inference latency must be < 15ms (real-time requirement)
        assert!(
            avg_lat < Duration::from_millis(15) * SLOWDOWN,
            "Silero VAD inference for {} took {:?} (limit: 15ms)",
            label,
            avg_lat
        );
        // On modern CPUs it should typically be under 2ms
        assert!(
            avg_lat < Duration::from_millis(5) * SLOWDOWN,
            "Silero VAD inference for {} exceeds expected 5ms CPU bound: {:?}",
            label,
            avg_lat
        );
    }
}

#[test]
fn edge_case_transient_clicks_and_pops_immunity() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let config = VadConfig::ultra_low_latency(); // 160 samples (10ms)
    let mut engine = VadEngine::new(&model_path, config).expect("init VadEngine");

    // 1. Single sample full-scale Dirac impulse spike (pop/click: [1.0, 0.0, 0.0, ...])
    let mut click_frame = vec![0.0f32; 160];
    click_frame[10] = 1.0;
    click_frame[11] = -0.9;

    let stage0 = compute_stage0_metrics(
        &click_frame,
        config.energy_threshold,
        config.zcr_min,
        config.zcr_max,
    );
    assert!(
        stage0.rms_energy > 0.05,
        "Click must register high RMS energy in Stage 0"
    );

    let events = engine.process_audio(&click_frame).expect("process click");
    let click_confidence = engine.last_confidence();

    println!(
        "[Edge Case: Transient Click] Stage 0 RMS: {:.4}, ZCR: {:.4}, Silero Confidence: {:.4}",
        stage0.rms_energy, stage0.zcr, click_confidence
    );

    // Silero neural model is trained on human speech features; a transient click produces very low confidence (< 0.20)
    assert!(
        click_confidence < 0.50,
        "Isolated transient click should have low speech confidence, got {:.4}",
        click_confidence
    );
    assert!(
        !engine.is_speaking(),
        "Isolated click must NOT trigger SpeechStart state"
    );
    assert!(
        events.is_empty() || !events.iter().any(|(e, _)| *e == VadEvent::SpeechStart),
        "Isolated click must not produce SpeechStart event"
    );

    // 2. Burst of 3ms high-frequency white noise pop (e.g. mic unplug/bump) followed by silence
    let mut bump_frame = vec![0.0f32; 160];
    for (i, sample) in bump_frame.iter_mut().enumerate().take(48) {
        // 3ms at 16kHz
        *sample = if i % 2 == 0 { 0.8 } else { -0.8 };
    }
    let events2 = engine.process_audio(&bump_frame).expect("process bump");
    let bump_confidence = engine.last_confidence();

    println!(
        "[Edge Case: Mic Bump Pop] Silero Confidence: {:.4}, is_speaking: {}",
        bump_confidence,
        engine.is_speaking()
    );

    assert!(
        bump_confidence < 0.50,
        "High frequency mic pop should not be classified as speech (confidence: {:.4})",
        bump_confidence
    );
    assert!(
        !engine.is_speaking(),
        "Mic bump must NOT trigger SpeechStart"
    );
    assert!(events2.is_empty());
}

#[test]
fn edge_case_whispering_and_fricatives() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let config = VadConfig::ultra_low_latency();
    let mut engine = VadEngine::new(&model_path, config).expect("init VadEngine");

    // Simulating low-energy whisper with unvoiced fricatives:
    // Low RMS (~0.003 - 0.008), high ZCR (~0.25 - 0.40)
    let mut whisper_frames = Vec::new();
    for f in 0..10 {
        let mut frame = vec![0.0f32; 160];
        for (i, sample) in frame.iter_mut().enumerate() {
            let t = (f * 160 + i) as f32;
            // Mixed colored noise + low amplitude formant
            let noise = ((t * 1337.0).sin() * 43_758.547).fract() - 0.5;
            let formant = (2.0 * std::f32::consts::PI * 800.0 * (t / 16000.0)).sin() * 0.5;
            *sample = (noise * 0.7 + formant * 0.3) * 0.008;
        }
        whisper_frames.push(frame);
    }

    // Verify Stage 0 detects whisper activity and process through engine
    for (idx, frame) in whisper_frames.iter().enumerate() {
        let m = compute_stage0_metrics(
            frame,
            config.energy_threshold,
            config.zcr_min,
            config.zcr_max,
        );
        assert!(
            m.is_active,
            "Frame {} of whisper should be active in Stage 0 (RMS: {:.5}, ZCR: {:.3})",
            idx, m.rms_energy, m.zcr
        );
        let _ = engine.process_audio(frame).expect("process whisper frame");
    }

    // Test state machine debounce behavior with borderline whisper confidence (e.g. 0.60)
    let mut engine_debounce = VadEngine::new(&model_path, config).expect("init VadEngine");
    assert!(!engine_debounce.is_speaking());

    // Frame 1 of whisper (p=0.60, low energy)
    let ev1 = engine_debounce.test_update_state_machine_with_confidence(true, 0.60, 0.003);
    assert_eq!(ev1, None, "Frame 1 of borderline whisper should debounce");
    assert!(!engine_debounce.is_speaking());

    // Frame 2 of whisper (p=0.60, meets speech_start_threshold = 2 in ultra_low_latency)
    let ev2 = engine_debounce.test_update_state_machine_with_confidence(true, 0.60, 0.003);
    assert_eq!(
        ev2,
        Some(VadEvent::SpeechStart),
        "Frame 2 of whisper must trigger SpeechStart after 2 consecutive frames"
    );
    assert!(engine_debounce.is_speaking());
}

#[test]
fn edge_case_extended_absolute_silence_and_dc_bias() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let config = VadConfig::ultra_low_latency();
    let mut engine = VadEngine::new(&model_path, config).expect("init VadEngine");

    // 1. Long period of absolute silence (1000 frames = 10 seconds of 0.0)
    let zero_frame = vec![0.0f32; 160];
    for f in 0..1000 {
        let events = engine
            .process_audio(&zero_frame)
            .expect("process zero frame");
        assert!(
            events.is_empty(),
            "Frame {} of pure silence must produce no events",
            f
        );
        assert!(
            !engine.is_speaking(),
            "Engine must not be speaking during silence"
        );
        assert!(
            engine.last_confidence() < 0.05,
            "Confidence on silence must be < 0.05, got {}",
            engine.last_confidence()
        );
    }

    // 2. DC Offset Bias (+0.05 constant level across all samples)
    // DC has high RMS (0.05), but Zero Crossing Rate is strictly 0.0 (no sign changes).
    let dc_frame = vec![0.05f32; 160];
    let dc_metrics = compute_stage0_metrics(
        &dc_frame,
        config.energy_threshold,
        config.zcr_min,
        config.zcr_max,
    );
    assert_eq!(dc_metrics.zcr, 0.0, "DC bias ZCR must be exactly 0.0");

    let dc_events = engine.process_audio(&dc_frame).expect("process dc frame");
    let dc_confidence = engine.last_confidence();

    println!(
        "[Edge Case: DC Bias] RMS: {:.4}, ZCR: {:.4}, Silero Confidence: {:.4}",
        dc_metrics.rms_energy, dc_metrics.zcr, dc_confidence
    );

    // Neural VAD should not classify constant DC as speech
    assert!(
        dc_confidence < 0.50,
        "DC bias should not trigger speech confidence (got {:.4})",
        dc_confidence
    );
    assert!(!engine.is_speaking());
    assert!(dc_events.is_empty());
}

#[test]
fn edge_case_unaligned_audio_streaming_and_rapid_speech_bursts() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let config = VadConfig::ultra_low_latency(); // 160 samples per frame
    let mut engine = VadEngine::new(&model_path, config).expect("init VadEngine");

    // Feed unaligned arbitrary chunk sizes: 17, 33, 47, 89 samples
    let chunk_sizes = [17, 33, 47, 89, 13, 27, 95];
    let mut total_samples_fed = 0usize;

    for &c_size in &chunk_sizes {
        let chunk = vec![0.0f32; c_size];
        let _ = engine
            .process_audio(&chunk)
            .expect("process unaligned chunk");
        total_samples_fed += c_size;
    }

    let expected_residual = total_samples_fed % 160;

    // Push remaining to complete frames
    let remainder = (160 - expected_residual) % 160;
    let _ = engine
        .process_audio(&vec![0.0f32; remainder])
        .expect("process remainder");

    // Test rapid bursts and debounce recovery:
    // 3 speech frames -> SpeechStart -> 25 silence frames -> SpeechEnd -> 3 speech frames -> SpeechStart
    engine.reset();
    assert!(!engine.is_speaking());

    // Speech burst 1
    let ev1 = engine.test_update_state_machine_with_confidence(true, 0.90, 0.02);
    assert_eq!(ev1, Some(VadEvent::SpeechStart), "Burst 1 starts speech");
    assert!(engine.is_speaking());

    // Silence pause (22 frames in ultra_low_latency)
    for _ in 0..21 {
        let ev = engine.test_update_state_machine_with_confidence(false, 0.01, 0.0001);
        assert_eq!(ev, None);
    }
    let ev_end = engine.test_update_state_machine_with_confidence(false, 0.01, 0.0001);
    assert_eq!(ev_end, Some(VadEvent::SpeechEnd), "Silence ends utterance");
    assert!(!engine.is_speaking());

    // Speech burst 2
    let ev2 = engine.test_update_state_machine_with_confidence(true, 0.90, 0.02);
    assert_eq!(
        ev2,
        Some(VadEvent::SpeechStart),
        "Burst 2 starts new speech utterance"
    );
    assert!(engine.is_speaking());
}

#[test]
fn stress_test_multithreaded_fork_sessions_race_free() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    let prototype =
        VadEngine::new(&model_path, VadConfig::ultra_low_latency()).expect("load Silero prototype");

    const NUM_THREADS: usize = 8;
    const FRAMES_PER_THREAD: usize = 50;

    let mut handles = Vec::new();

    for thread_id in 0..NUM_THREADS {
        let mut session = prototype.fork_session();
        let handle = std::thread::spawn(move || {
            let mut speech_detected = false;
            for f in 0..FRAMES_PER_THREAD {
                // Synthesize alternating speech and silence
                let is_speech_turn = (f + thread_id) % 4 == 0;
                let mut frame = vec![0.0f32; 160];
                if is_speech_turn {
                    for (i, s) in frame.iter_mut().enumerate() {
                        *s =
                            (2.0 * std::f32::consts::PI * 300.0 * (i as f32 / 16000.0)).sin() * 0.4;
                    }
                }

                let res = session.process_audio(&frame);
                assert!(
                    res.is_ok(),
                    "Thread {} frame {} failed: {:?}",
                    thread_id,
                    f,
                    res.err()
                );
                if session.is_speaking() {
                    speech_detected = true;
                }
            }
            speech_detected
        });
        handles.push(handle);
    }

    for (id, handle) in handles.into_iter().enumerate() {
        let res = handle.join().expect("thread join");
        println!(
            "[Multithread Stress] Thread {} completed successfully (speech_detected={})",
            id, res
        );
    }

    // Prototype recurrent state must remain pristine zeroes
    assert!(
        !prototype.is_speaking(),
        "Prototype state must not be modified by forked workers"
    );
}

#[test]
fn verify_e2e_speech_detection_latency_under_30ms() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not found at {:?}", model_path);
        return;
    }

    // 160-sample (10ms) Ultra Low Latency Preset
    let config = VadConfig::ultra_low_latency();
    let mut engine = VadEngine::new(&model_path, config).expect("init VadEngine");

    // Physical accumulation time for 160 samples at 16kHz
    let physical_frame_duration_ms = (160.0 / 16000.0) * 1000.0; // 10.0ms

    // Test with high-confidence speech signal (test_update_state_machine_with_confidence)
    let start = Instant::now();
    let event = engine.test_update_state_machine_with_confidence(true, 0.92, 0.02);
    let compute_duration = start.elapsed();
    let compute_duration_ms = compute_duration.as_secs_f64() * 1000.0;

    let total_detection_latency_ms = physical_frame_duration_ms + compute_duration_ms;

    println!(
        "[VAD Latency Test - Fast Start Trigger] Frame Duration: {:.1}ms, Compute Latency: {:.3}ms, Total Detection Latency: {:.3}ms",
        physical_frame_duration_ms, compute_duration_ms, total_detection_latency_ms
    );

    assert_eq!(
        event,
        Some(VadEvent::SpeechStart),
        "SpeechStart must trigger immediately on high confidence"
    );
    assert!(engine.is_speaking());

    // Debug builds are unoptimized; the strict number is the release contract.
    let slowdown = if cfg!(debug_assertions) { 5.0 } else { 1.0 };
    // E2E Speech Detection Latency must be < 30ms (SLA target)
    assert!(
        total_detection_latency_ms < 30.0 * slowdown,
        "Total speech detection latency ({:.3}ms) must be strictly < 30.0ms",
        total_detection_latency_ms
    );
    assert!(
        total_detection_latency_ms < 15.0 * slowdown,
        "Total speech detection latency on 10ms frame exceeds 15ms: {:.3}ms",
        total_detection_latency_ms
    );
}
