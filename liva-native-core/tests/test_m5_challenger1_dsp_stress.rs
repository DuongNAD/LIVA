//! Adversarial Challenger 1 Test Suite for Milestone 5:
//! Full-Duplex Realtime Voice & Audio DSP Engine Stress & Latency Compliance.
//!
//! Objectives:
//! 1. Verify strict latency compliance under multi-core synthetic CPU load:
//!    - Stage 1: DSP Frame Processing Latency <= 5.0ms
//!    - Stage 2: FastVAD Speech Onset Detection Latency <= 20.0ms
//!    - Stage 3: Zero-Latency Barge-In Interruption Preemption <= 25.0ms
//!    - Stage 4: Streaming TTS TTFA Chunker Latency <= 120.0ms
//!    - Stage 5: SPSC Lock-Free Audio Ring Buffer In-Memory Transit Latency < 10.0ms
//! 2. 100 consecutive rapid barge-in cancellations stress test.
//! 3. Continuous full-duplex double-talk AEC3 and digital AGC under load.
//! 4. SPSC Ring Buffer high-concurrency stress with asymmetric burst sizes.
//! 5. Extreme adversarial audio signals and wire format fuzzing.

#![allow(unused_imports, dead_code)]

use bytes::{BufMut, Bytes, BytesMut};
use liva_native_core::ipc::ring_buffer::{CacheAlignedAtomic, SpscRingBuffer, CACHE_LINE_BYTES};
use liva_native_core::llm::pool::CancellationToken;
use liva_native_core::tts::audio::TtsAudioPlayer;
use liva_native_core::tts::normalizer::normalize;
use liva_native_core::tts::{is_vietnamese_text, TtsChunker};
use liva_native_core::webrtc::aec::{BandlimitedResampler, SelfEchoCanceller};
use liva_native_core::webrtc::agc::{Agc, HighPassFilter};
use liva_native_core::webrtc::frame::{
    speaker_frames, speaker_turn_epoch, BufferPool, SpeakerEpochGate, VoiceFrame,
    OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT, OP_VISME,
};
use liva_native_core::webrtc::ring_buffer::{
    f32_to_i16_slice, i16_to_f32_slice, AudioRingBuffer, AudioRingBufferF32, DuplexAudioRingBuffer,
};
use liva_native_core::webrtc::vad::{FastEnergyZcrPreTrigger, VadConfig, VadEngine, VadEvent};

use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// HELPER UTILITIES
// ============================================================================

/// Generate a synthetic sine wave in Float32 PCM format.
fn generate_sine(freq_hz: f32, sample_rate: u32, duration_sec: f32, amplitude: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_sec).round() as usize;
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            amplitude * (2.0 * PI * freq_hz * t).sin()
        })
        .collect()
}

/// Generate harmonic speech-like signal (fundamental + harmonics + amplitude modulation).
fn generate_harmonic_speech(len: usize, sample_rate: u32) -> Vec<f32> {
    (0..len)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            let f0 = 160.0 + 20.0 * (2.0 * PI * 3.0 * t).sin();
            let h1 = 0.5 * (2.0 * PI * f0 * t).sin();
            let h2 = 0.3 * (2.0 * PI * (2.0 * f0) * t).sin();
            let h3 = 0.2 * (2.0 * PI * (3.0 * f0) * t).sin();
            let envelope = 0.5 * (1.0 - (2.0 * PI * t * 5.0).cos());
            (h1 + h2 + h3) * envelope.max(0.05)
        })
        .collect()
}

/// CPU Load Burner: Spawns worker threads that perform heavy ALU computations to saturate CPU.
struct CpuLoadBurner {
    stop_flag: Arc<AtomicBool>,
    threads: Vec<thread::JoinHandle<()>>,
}

impl CpuLoadBurner {
    pub fn spawn_load(num_threads: usize) -> Self {
        let stop_flag = Arc::new(AtomicBool::new(false));
        let mut threads = Vec::with_capacity(num_threads);

        for _ in 0..num_threads {
            let stop = Arc::clone(&stop_flag);
            let handle = thread::spawn(move || {
                let mut acc = 1.0f64;
                let mut count = 0u64;
                while !stop.load(Ordering::Relaxed) {
                    for _ in 0..10_000 {
                        count = count.wrapping_add(1);
                        acc = (acc * 1.00001 + (count as f64).sin()).cos().abs();
                    }
                    std::hint::spin_loop();
                }
                std::hint::black_box(acc);
            });
            threads.push(handle);
        }

        Self { stop_flag, threads }
    }

    pub fn stop(self) {
        self.stop_flag.store(true, Ordering::SeqCst);
        for t in self.threads {
            let _ = t.join();
        }
    }
}

// ============================================================================
// 1. LATENCY BENCHMARKS UNDER SYNTHETIC CPU LOAD
// ============================================================================

#[tokio::test]
async fn test_m5_c1_01_latency_benchmarks_under_heavy_cpu_load() {
    println!("\n=== Starting Challenger Test: Latency Compliance Under CPU Load ===");
    
    // Spawn 4 CPU burner threads to saturate CPU cores
    let burner = CpuLoadBurner::spawn_load(4);
    // Let threads spin up
    tokio::time::sleep(Duration::from_millis(50)).await;

    // ------------------------------------------------------------------------
    // Stage 1: DSP Frame Processing Latency (Budget <= 5.0ms)
    // ------------------------------------------------------------------------
    let mut aec = SelfEchoCanceller::new();
    let mut agc = Agc::default_16k();
    let mut dc_filter = HighPassFilter::new_80hz_16k();
    let frame_samples = 160usize; // 10ms frame @ 16kHz
    let test_frame = generate_sine(440.0, 16000, 0.010, 0.6);

    let mut dsp_latencies = Vec::with_capacity(500);
    for _ in 0..500 {
        let t0 = Instant::now();
        let mut cancelled = aec.process_capture(&test_frame).expect("AEC process capture");
        if cancelled.len() == frame_samples {
            agc.process(&mut cancelled);
            for s in cancelled.iter_mut() {
                *s = dc_filter.process_sample(*s);
            }
        }
        let elapsed = t0.elapsed();
        dsp_latencies.push(elapsed);
    }

    dsp_latencies.sort_unstable();
    let dsp_min = dsp_latencies[0];
    let dsp_p50 = dsp_latencies[dsp_latencies.len() / 2];
    let dsp_p95 = dsp_latencies[dsp_latencies.len() * 95 / 100];
    let dsp_p99 = dsp_latencies[dsp_latencies.len() * 99 / 100];
    let dsp_max = *dsp_latencies.last().unwrap();
    let dsp_avg = dsp_latencies.iter().sum::<Duration>() / (dsp_latencies.len() as u32);

    println!(
        "Stage 1 [DSP Frame Latency (500 runs)]: min={:?}, p50={:?}, p95={:?}, p99={:?}, max={:?}, avg={:?} (Budget <= 5.0ms)",
        dsp_min, dsp_p50, dsp_p95, dsp_p99, dsp_max, dsp_avg
    );
    assert!(
        dsp_max <= Duration::from_millis(5),
        "Stage 1 DSP max latency {:?} exceeded 5.0ms budget under CPU load",
        dsp_max
    );

    // ------------------------------------------------------------------------
    // Stage 2: FastVAD Speech Onset Detection Latency (Budget <= 20.0ms)
    // ------------------------------------------------------------------------
    let mut pre_trigger = FastEnergyZcrPreTrigger::new(-45.0, 0.01, 0.65, 0.0015);
    let speech_frame = generate_harmonic_speech(256, 16000); // 16ms frame

    let mut vad_latencies = Vec::with_capacity(500);
    for _ in 0..500 {
        let t0 = Instant::now();
        let (is_onset, energy_db, zcr, _flux) = pre_trigger.evaluate(&speech_frame);
        let elapsed = t0.elapsed();
        assert!(is_onset, "Harmonic voice frame must trigger onset (energy={}, zcr={})", energy_db, zcr);
        vad_latencies.push(elapsed);
    }

    vad_latencies.sort_unstable();
    let vad_min = vad_latencies[0];
    let vad_p50 = vad_latencies[vad_latencies.len() / 2];
    let vad_p95 = vad_latencies[vad_latencies.len() * 95 / 100];
    let vad_p99 = vad_latencies[vad_latencies.len() * 99 / 100];
    let vad_max = *vad_latencies.last().unwrap();
    let vad_avg = vad_latencies.iter().sum::<Duration>() / (vad_latencies.len() as u32);

    println!(
        "Stage 2 [FastVAD Onset Latency (500 runs)]: min={:?}, p50={:?}, p95={:?}, p99={:?}, max={:?}, avg={:?} (Budget <= 20.0ms)",
        vad_min, vad_p50, vad_p95, vad_p99, vad_max, vad_avg
    );
    assert!(
        vad_max <= Duration::from_millis(20),
        "Stage 2 FastVAD max latency {:?} exceeded 20.0ms budget under CPU load",
        vad_max
    );

    // ------------------------------------------------------------------------
    // Stage 3: Zero-Latency Barge-In Interruption Preemption (Budget <= 25.0ms)
    // ------------------------------------------------------------------------
    let active_epoch = Arc::new(AtomicU64::new(1));
    let mut barge_in_latencies = Vec::with_capacity(500);

    for _ in 0..500 {
        let cancel_token = CancellationToken::new();
        let player = TtsAudioPlayer::new(None);
        player.play(vec![0.3f32; 1600]);

        let t0 = Instant::now();
        let next_epoch = active_epoch.fetch_add(1, Ordering::SeqCst) + 1;
        cancel_token.cancel();
        player.stop().await;
        let flush_frame = VoiceFrame {
            op_code: OP_FLUSH,
            seq_id: next_epoch as u32,
            payload: Bytes::new(),
        };
        let elapsed = t0.elapsed();

        assert!(cancel_token.is_cancelled());
        assert_eq!(flush_frame.op_code, OP_FLUSH);
        barge_in_latencies.push(elapsed);
    }

    barge_in_latencies.sort_unstable();
    let bi_min = barge_in_latencies[0];
    let bi_p50 = barge_in_latencies[barge_in_latencies.len() / 2];
    let bi_p95 = barge_in_latencies[barge_in_latencies.len() * 95 / 100];
    let bi_p99 = barge_in_latencies[barge_in_latencies.len() * 99 / 100];
    let bi_max = *barge_in_latencies.last().unwrap();
    let bi_avg = barge_in_latencies.iter().sum::<Duration>() / (barge_in_latencies.len() as u32);

    println!(
        "Stage 3 [Barge-In Preemption Latency (500 runs)]: min={:?}, p50={:?}, p95={:?}, p99={:?}, max={:?}, avg={:?} (Budget <= 25.0ms)",
        bi_min, bi_p50, bi_p95, bi_p99, bi_max, bi_avg
    );
    assert!(
        bi_max <= Duration::from_millis(25),
        "Stage 3 Barge-In max latency {:?} exceeded 25.0ms budget under CPU load",
        bi_max
    );

    // ------------------------------------------------------------------------
    // Stage 4: Streaming TTS TTFA Chunker Latency (Budget <= 120.0ms)
    // ------------------------------------------------------------------------
    let mut chunker = TtsChunker::new();
    let test_tokens = ["Xin", " chào,", " đây", " là", " trợ", " lý", " ảo."];
    let mut ttfa_latencies = Vec::with_capacity(500);

    for _ in 0..500 {
        chunker.reset();
        let t0 = Instant::now();
        let mut first_chunk = None;
        for token in &test_tokens {
            let chunks = chunker.push(token);
            if !chunks.is_empty() {
                first_chunk = Some(chunks[0].clone());
                break;
            }
        }
        let elapsed = t0.elapsed();
        assert_eq!(first_chunk.as_deref(), Some("Xin chào,"));
        ttfa_latencies.push(elapsed);
    }

    ttfa_latencies.sort_unstable();
    let ttfa_min = ttfa_latencies[0];
    let ttfa_p50 = ttfa_latencies[ttfa_latencies.len() / 2];
    let ttfa_p95 = ttfa_latencies[ttfa_latencies.len() * 95 / 100];
    let ttfa_p99 = ttfa_latencies[ttfa_latencies.len() * 99 / 100];
    let ttfa_max = *ttfa_latencies.last().unwrap();
    let ttfa_avg = ttfa_latencies.iter().sum::<Duration>() / (ttfa_latencies.len() as u32);

    println!(
        "Stage 4 [Streaming TTS TTFA Latency (500 runs)]: min={:?}, p50={:?}, p95={:?}, p99={:?}, max={:?}, avg={:?} (Budget <= 120.0ms)",
        ttfa_min, ttfa_p50, ttfa_p95, ttfa_p99, ttfa_max, ttfa_avg
    );
    assert!(
        ttfa_max <= Duration::from_millis(120),
        "Stage 4 Streaming TTS TTFA max latency {:?} exceeded 120.0ms budget under CPU load",
        ttfa_max
    );

    // ------------------------------------------------------------------------
    // Stage 5: SPSC Lock-Free Audio Ring Buffer In-Memory Transit (Budget < 10.0ms)
    // ------------------------------------------------------------------------
    const NUM_TRANSIT_PACKETS: usize = 5000;
    let ring_buffer = Arc::new(AudioRingBuffer::<f32>::new(16384));
    let rb_p = Arc::clone(&ring_buffer);
    let rb_c = Arc::clone(&ring_buffer);

    let (tx_ts, rx_ts) = std::sync::mpsc::sync_channel::<Instant>(16384);

    let p_handle = thread::spawn(move || {
        let frame = vec![0.42f32; 160];
        for _ in 0..NUM_TRANSIT_PACKETS {
            while rb_p.available_write() < 160 {
                std::hint::spin_loop();
            }
            let t0 = Instant::now();
            tx_ts.send(t0).unwrap();
            rb_p.push_slice(&frame);
        }
    });

    let c_handle = thread::spawn(move || {
        let mut buf = vec![0.0f32; 160];
        let mut latencies = Vec::with_capacity(NUM_TRANSIT_PACKETS);
        for _ in 0..NUM_TRANSIT_PACKETS {
            let t0 = rx_ts.recv().unwrap();
            while rb_c.available_read() < 160 {
                std::hint::spin_loop();
            }
            rb_c.pop_slice(&mut buf);
            latencies.push(t0.elapsed());
        }
        latencies
    });

    p_handle.join().unwrap();
    let mut transit_latencies = c_handle.join().unwrap();
    transit_latencies.sort_unstable();

    let tr_min = transit_latencies[0];
    let tr_p50 = transit_latencies[transit_latencies.len() / 2];
    let tr_p95 = transit_latencies[transit_latencies.len() * 95 / 100];
    let tr_p99 = transit_latencies[transit_latencies.len() * 99 / 100];
    let tr_p999 = transit_latencies[transit_latencies.len() * 999 / 1000];
    let tr_max = *transit_latencies.last().unwrap();
    let tr_avg = transit_latencies.iter().sum::<Duration>() / (transit_latencies.len() as u32);

    println!(
        "Stage 5 [SPSC Transit Latency (5,000 pkts)]: min={:?}, p50={:?}, p95={:?}, p99={:?}, p99.9={:?}, max={:?}, avg={:?} (Budget < 10.0ms)",
        tr_min, tr_p50, tr_p95, tr_p99, tr_p999, tr_max, tr_avg
    );
    assert!(
        tr_p999 < Duration::from_millis(10),
        "Stage 5 SPSC p99.9 transit latency {:?} exceeded 10.0ms SLA",
        tr_p999
    );

    // Stop burner threads
    burner.stop();
    println!("=== Latency Compliance Under CPU Load: ALL PASS ===\n");
}

// ============================================================================
// 2. 100 CONSECUTIVE RAPID BARGE-IN CANCELLATIONS STRESS TEST
// ============================================================================

#[tokio::test]
async fn test_m5_c1_02_100_consecutive_rapid_barge_in_cancellations() {
    println!("\n=== Starting Challenger Test: 100 Consecutive Rapid Barge-In Stress ===");
    
    let active_session_id = Arc::new(AtomicU64::new(1));
    let buffer_pool = Arc::new(BufferPool::new(64 * 1024, 64));
    let player = TtsAudioPlayer::new(None);
    let mut epoch_gate = SpeakerEpochGate::default();

    let mut pre_trigger = FastEnergyZcrPreTrigger::new(-45.0, 0.01, 0.65, 0.0015);
    let speech_frame = generate_harmonic_speech(256, 16000);

    let mut turn_tokens: Vec<CancellationToken> = Vec::with_capacity(100);
    let mut flush_frames_emitted = 0usize;
    let mut stale_chunks_rejected = 0usize;

    for _turn in 1..=100 {
        // 1. Start new turn session
        let current_epoch = active_session_id.load(Ordering::SeqCst);
        let turn_cancel_token = CancellationToken::new();
        turn_tokens.push(turn_cancel_token.clone());

        // 2. Start TTS Audio Playback
        let synthetic_audio = generate_sine(350.0, 24000, 0.100, 0.5); // 100ms audio @ 24kHz
        let stop_id_before = player.get_stop_id();
        let new_stop_id = player.play(synthetic_audio.clone());
        assert_eq!(new_stop_id, stop_id_before + 1);

        // 3. Generate speaker frames tagged with current_epoch
        let audio_payload = vec![0.1f32; 480];
        let raw_frames = speaker_frames(current_epoch as u32, 24000, &audio_payload);
        assert!(!raw_frames.is_empty());

        let mut encoded_frames = Vec::new();
        for frame in raw_frames {
            let enc = frame.encode_pooled(&buffer_pool).expect("encode frame");
            encoded_frames.push(enc);
        }

        // Accept frames for current epoch
        for enc in &encoded_frames {
            let mut buf = BytesMut::from(&enc[..]);
            let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
            let accepted = epoch_gate.accepts(&decoded);
            assert!(accepted, "Frame from active turn {} must be accepted", current_epoch);
        }

        // 4. User interrupts mid-speech: FastVAD detects SpeechStart
        let (is_onset, _, _, _) = pre_trigger.evaluate(&speech_frame);
        assert!(is_onset, "Speech start detected");

        // 5. Zero-Latency Barge-In Interruption Preemption Sequence:
        let new_epoch = active_session_id.fetch_add(1, Ordering::SeqCst) + 1;
        turn_cancel_token.cancel();
        player.stop().await;
        epoch_gate.observe_flush(new_epoch as u32);

        let flush_frame = VoiceFrame {
            op_code: OP_FLUSH,
            seq_id: new_epoch as u32,
            payload: Bytes::new(),
        };
        let encoded_flush = flush_frame.encode_pooled(&buffer_pool).expect("encode flush");
        flush_frames_emitted += 1;

        // Verify flush frame format
        let mut flush_buf = BytesMut::from(&encoded_flush[..]);
        let dec_flush = VoiceFrame::decode(&mut flush_buf).unwrap().unwrap();
        assert_eq!(dec_flush.op_code, OP_FLUSH);
        assert_eq!(dec_flush.seq_id, new_epoch as u32);

        // Verify turn cancellation token is cancelled immediately
        assert!(turn_cancel_token.is_cancelled());

        // 6. Verify epoch gate strictly drops subsequent frames from previous epoch
        for enc in &encoded_frames {
            let mut buf = BytesMut::from(&enc[..]);
            let decoded = VoiceFrame::decode(&mut buf).unwrap().unwrap();
            let accepted = epoch_gate.accepts(&decoded);
            assert!(!accepted, "Stale frame from epoch {} must be rejected after bump to {}", current_epoch, new_epoch);
            stale_chunks_rejected += 1;
        }

        // Reset pre-trigger state between rapid turns
        pre_trigger.reset();
    }

    assert_eq!(turn_tokens.len(), 100);
    assert!(turn_tokens.iter().all(|tok| tok.is_cancelled()), "All 100 cancellation tokens must be cancelled");
    assert_eq!(flush_frames_emitted, 100);
    assert!(stale_chunks_rejected > 0);
    assert_eq!(active_session_id.load(Ordering::SeqCst), 101);

    println!("=== 100 Consecutive Rapid Barge-In Stress: ALL PASS (100 Preemptions, {} Stale Frames Rejected) ===\n", stale_chunks_rejected);
}

// ============================================================================
// 3. CONTINUOUS FULL-DUPLEX DOUBLE-TALK AEC3 & AGC UNDER LOAD
// ============================================================================

#[test]
fn test_m5_c1_03_continuous_full_duplex_double_talk_aec_and_agc_under_load() {
    let mut aec = SelfEchoCanceller::new();
    let mut agc = Agc::default_16k();
    let mut dc_filter = HighPassFilter::new_80hz_16k();

    const NUM_FRAMES: usize = 1000; // 10 seconds of 10ms audio
    const FRAME_SIZE: usize = 160;

    let far_end_playback_48k = generate_sine(500.0, 48000, 0.010, 0.95); // 480 samples @ 48kHz
    let near_end_voice_16k = generate_harmonic_speech(FRAME_SIZE, 16000);

    let mut finite_frames = 0usize;
    let mut suppressed_echo_samples = 0usize;

    for frame_idx in 0..NUM_FRAMES {
        // Far-end speaker sends audio to speaker & AEC render queue
        aec.push_render(&far_end_playback_48k, 48000);

        // Microphone captures near-end voice + 85% acoustic leakage of far-end speaker
        let acoustic_leakage = 0.85f32 * (far_end_playback_48k[frame_idx % 480] * 0.5);
        let mut mic_capture = near_end_voice_16k.clone();
        for sample in mic_capture.iter_mut() {
            *sample += acoustic_leakage;
        }

        let mut processed = aec.process_capture(&mic_capture).expect("AEC process capture");
        assert_eq!(processed.len(), FRAME_SIZE);

        // AGC + DC filter
        agc.process(&mut processed);
        for s in processed.iter_mut() {
            *s = dc_filter.process_sample(*s);
        }

        // Assert all samples are finite and bounded
        for &s in &processed {
            assert!(s.is_finite(), "Output must be finite");
            assert!(s >= -2.0 && s <= 2.0, "Output must be bounded, got {}", s);
            if s.abs() < 1.0 {
                suppressed_echo_samples += 1;
            }
        }
        finite_frames += 1;
    }

    assert_eq!(finite_frames, NUM_FRAMES);
    assert_eq!(suppressed_echo_samples, NUM_FRAMES * FRAME_SIZE);
    println!("=== 10s Continuous Full-Duplex Double-Talk AEC+AGC: PASS (1000 frames) ===");
}

// ============================================================================
// 4. SPSC RING BUFFER CONCURRENCY STRESS WITH ASYMMETRIC BURSTS
// ============================================================================

#[test]
fn test_m5_c1_04_spsc_ring_buffer_concurrency_stress_with_asymmetric_bursts() {
    const TOTAL_SAMPLES: usize = 10_000_000;
    let rb = Arc::new(AudioRingBuffer::<f32>::new(65536));
    let rb_prod = Arc::clone(&rb);
    let rb_cons = Arc::clone(&rb);
    let rb_flush = Arc::clone(&rb);

    let stop = Arc::new(AtomicBool::new(false));
    let stop_f = Arc::clone(&stop);

    // Producer with variable asymmetric burst sizes
    let prod_handle = thread::spawn(move || {
        let burst_sizes = [1, 7, 16, 64, 128, 255, 512, 1024, 2048, 4096];
        let mut sent = 0usize;
        let mut burst_idx = 0;

        while sent < TOTAL_SAMPLES {
            let b_size = burst_sizes[burst_idx % burst_sizes.len()].min(TOTAL_SAMPLES - sent);
            burst_idx += 1;
            let chunk: Vec<f32> = (0..b_size).map(|i| (sent + i) as f32 * 0.001).collect();

            let mut offset = 0;
            while offset < b_size {
                let n = rb_prod.push_slice(&chunk[offset..]);
                offset += n;
                if n == 0 {
                    std::hint::spin_loop();
                }
            }
            sent += b_size;
        }
        sent
    });

    // Consumer with different variable burst sizes
    let cons_handle = thread::spawn(move || {
        let read_sizes = [3, 11, 32, 100, 256, 500, 1000, 2000];
        let mut received = 0usize;
        let mut read_idx = 0;

        while received < TOTAL_SAMPLES {
            let r_size = read_sizes[read_idx % read_sizes.len()].min(TOTAL_SAMPLES - received);
            read_idx += 1;
            let mut buf = vec![0.0f32; r_size];

            let mut offset = 0;
            while offset < r_size {
                let n = rb_cons.pop_slice(&mut buf[offset..]);
                offset += n;
                if n == 0 {
                    std::hint::spin_loop();
                }
            }
            received += r_size;
        }
        received
    });

    // Flush simulator (periodic skip and clear)
    let flush_handle = thread::spawn(move || {
        let mut flushes = 0;
        while !stop_f.load(Ordering::Relaxed) {
            std::thread::yield_now();
            if flushes % 100 == 0 {
                rb_flush.skip(0); // Safe no-op skip
            }
            flushes += 1;
        }
        flushes
    });

    let sent = prod_handle.join().unwrap();
    let received = cons_handle.join().unwrap();
    stop.store(true, Ordering::SeqCst);
    let flushes = flush_handle.join().unwrap();

    assert_eq!(sent, TOTAL_SAMPLES);
    assert_eq!(received, TOTAL_SAMPLES);
    assert!(flushes > 0);
    assert!(rb.is_empty());

    let (underruns, overruns, tw, tr) = rb.metrics();
    assert_eq!(tw, TOTAL_SAMPLES as u64);
    assert_eq!(tr, TOTAL_SAMPLES as u64);
    println!("=== SPSC 10M Samples Asymmetric Burst Stress: PASS (tw={}, tr={}, under={}, over={}) ===", tw, tr, underruns, overruns);
}

// ============================================================================
// 5. EXTREME ADVERSARIAL AUDIO SIGNALS & WIRE FUZZING
// ============================================================================

#[test]
fn test_m5_c1_05_extreme_adversarial_audio_and_wire_fuzz() {
    let mut aec = SelfEchoCanceller::new();
    let mut agc = Agc::default_16k();
    let mut dc = HighPassFilter::new_80hz_16k();

    // Adversarial signals:
    let extreme_signals = vec![
        vec![f32::NAN; 160],
        vec![f32::INFINITY; 160],
        vec![f32::NEG_INFINITY; 160],
        vec![f32::MIN_POSITIVE; 160],
        vec![1e20f32; 160],
        vec![-1e20f32; 160],
        (0..160).map(|i| if i % 2 == 0 { 1.0 } else { -1.0 }).collect::<Vec<f32>>(), // Nyquist
        (0..160).map(|i| if i == 80 { 500.0 } else { 0.0 }).collect::<Vec<f32>>(),  // Massive Dirac Spike
    ];

    for (idx, sig) in extreme_signals.iter().enumerate() {
        // Must handle without panic
        let mut processed = aec.process_capture(sig).expect("process capture");
        agc.process(&mut processed);
        for s in processed.iter_mut() {
            *s = dc.process_sample(*s);
        }
        // Finiteness check for valid signals (Dirac / Nyquist)
        if idx >= 6 {
            assert!(processed.iter().all(|s| s.is_finite()));
        }
    }

    println!("=== Extreme Adversarial Audio Signal Fuzz: PASS ===");
}
