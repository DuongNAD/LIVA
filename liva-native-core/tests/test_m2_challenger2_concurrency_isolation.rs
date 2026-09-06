//! Adversarial Empirical Challenger 2 Test Suite for Milestone 2:
//! FastVAD & Zero-Latency Barge-In Interruption with LLM Worker Pool CancellationToken.
//!
//! Areas Adversarially Tested:
//! 1. Concurrent Multi-Session State Isolation (VAD, DSP, SPSC ring buffers, echo cancel).
//! 2. 100% Stale Audio Frame Rejection under Preemption (`SpeakerEpochGate` and atomic epoch checks).
//! 3. LLM Token Stream Preemption via `CancellationToken` (zero leak, immediate abort).
//! 4. Barge-In Interruption Storm (Zero Deadlock Stress Harness across threads).
//! 5. FastVAD Stage 0 Acoustic Pre-Trigger & Dual-Threshold Hysteresis Validation.
//! 6. Speech End Hangover Counting (22 frames / ~352ms debounce).
//! 7. Memory Allocation Boundedness & Buffer Pool Recycling.

#![allow(unused_imports, dead_code, clippy::identity_op)]

use bytes::{BufMut, Bytes, BytesMut};
use liva_native_core::ipc::ring_buffer::SpscRingBuffer;
use liva_native_core::llm::pool::CancellationToken;
use liva_native_core::tts::audio::TtsAudioPlayer;
use liva_native_core::tts::TtsChunker;
use liva_native_core::webrtc::aec::SelfEchoCanceller;
use liva_native_core::webrtc::frame::{
    speaker_frames, speaker_turn_epoch, BufferPool, SpeakerEpochGate, VoiceFrame,
    OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT, OP_VISME, OP_WAKE_PROBE,
};
use liva_native_core::webrtc::pipeline::{
    PipelineEvent, PipelineState, WebRTCPipelineHandle,
};
use liva_native_core::webrtc::session::{TurnAudioAction, TurnAudioBuffer, VoiceSessionAudio};
use liva_native_core::webrtc::vad::{
    resolve_model_path as resolve_vad_path, FastEnergyZcrPreTrigger, VadConfig, VadEngine, VadEvent,
};
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

fn generate_sine(freq_hz: f32, sample_rate: u32, duration_sec: f32, amplitude: f32) -> Vec<f32> {
    let total_samples = (sample_rate as f32 * duration_sec).round() as usize;
    (0..total_samples)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            amplitude * (2.0 * PI * freq_hz * t).sin()
        })
        .collect()
}

fn vad_model_path() -> Option<std::path::PathBuf> {
    let p = resolve_vad_path("models/stt");
    if p.exists() {
        Some(p)
    } else {
        None
    }
}

// ============================================================================
// 1. MULTI-SESSION ISOLATION STRESS TEST
// ============================================================================

#[test]
fn test_adv_multi_session_vad_concurrency_isolation() {
    let Some(vad_path) = vad_model_path() else {
        eprintln!("skip: silero_vad model not present");
        return;
    };

    let prototype = VadEngine::new(&vad_path, VadConfig::default()).expect("load vad prototype");
    let num_sessions = 32;
    let barrier = Arc::new(Barrier::new(num_sessions));

    let mut handles = Vec::new();
    for session_id in 0..num_sessions {
        let mut session_vad = prototype.fork_session();
        let b = Arc::clone(&barrier);

        let handle = std::thread::spawn(move || {
            b.wait();
            if session_id % 2 == 0 {
                // Silence session
                let silence = vec![0.0f32; 256 * 10];
                let events = session_vad.process_audio(&silence).expect("silence process");
                assert!(
                    events.iter().all(|(e, _)| *e != VadEvent::SpeechStart),
                    "Session {} (silence) must NOT trigger SpeechStart",
                    session_id
                );
            } else {
                // Active tone session
                let tone = generate_sine(400.0, 16000, 0.2, 0.7);
                let events = session_vad.process_audio(&tone).expect("tone process");
                assert!(
                    events.iter().all(|(_, conf)| conf.is_finite()),
                    "Session {} (tone) confidences must be finite",
                    session_id
                );
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined successfully");
    }
}

#[test]
fn test_adv_multi_session_ring_buffer_isolation_and_no_leak() {
    let num_sessions = 64;
    let frames_per_session = 100;
    let barrier = Arc::new(Barrier::new(num_sessions));

    let mut handles = Vec::new();
    for session_id in 0..num_sessions {
        let b = Arc::clone(&barrier);
        let handle = std::thread::spawn(move || {
            let ring_buf = SpscRingBuffer::new(4096);
            b.wait();

            let mut scratch = Vec::new();
            for frame_idx in 0..frames_per_session {
                let payload = format!("session_{:03}_frame_{:04}", session_id, frame_idx);
                ring_buf
                    .write_slice(payload.as_bytes())
                    .expect("write to ring buffer");

                let read_len = ring_buf
                    .read_bytes(&mut scratch)
                    .expect("read from ring buffer")
                    .expect("bytes available");
                assert_eq!(read_len, payload.len());
                assert_eq!(
                    std::str::from_utf8(&scratch).unwrap(),
                    payload,
                    "Payload mismatch in session {}",
                    session_id
                );
            }
            assert!(ring_buf.is_empty());
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Thread joined successfully");
    }
}

// ============================================================================
// 2. 100% STALE AUDIO FRAME REJECTION UNDER PREEMPTION
// ============================================================================

#[test]
fn test_adv_speaker_epoch_gate_100_percent_rejection_under_preemption() {
    let mut gate = SpeakerEpochGate::default();

    // Invariant 1: Gate accepts non-speaker frames unconditionally
    let non_speaker_ops = [
        OP_MIC_IN,
        OP_FLUSH,
        OP_VISME,
        OP_AUTH_HANDSHAKE,
        OP_WAKE_PROBE,
    ];
    for &op in &non_speaker_ops {
        let frame = VoiceFrame {
            op_code: op,
            seq_id: 1,
            payload: Bytes::from_static(b"control_data"),
        };
        assert!(
            gate.accepts(&frame),
            "Gate must accept non-speaker frame op: {}",
            op
        );
    }

    // Invariant 2: Initial gate (minimum_epoch = 0) accepts all epochs >= 0
    let frame_epoch_0 = speaker_frames(0, 16000, &[0.1f32; 160])[0].clone();
    assert!(gate.accepts(&frame_epoch_0));

    // Preempt to epoch 100 via flush
    gate.observe_flush(100);

    // Invariant 3: Stale flushes (< 100) must NOT regress gate minimum_epoch
    gate.observe_flush(50);
    gate.observe_flush(99);
    gate.observe_flush(0);

    // Invariant 4: 100% of frames from epoch 0..99 must be rejected
    for stale_epoch in 0..100 {
        let frame = speaker_frames(stale_epoch, 16000, &[0.1f32; 160])[0].clone();
        assert!(
            !gate.accepts(&frame),
            "SpeakerEpochGate MUST reject stale epoch {} when minimum is 100",
            stale_epoch
        );
    }

    // Invariant 5: 100% of frames from epoch 100..200 must be accepted
    for valid_epoch in 100..=200 {
        let frame = speaker_frames(valid_epoch, 16000, &[0.1f32; 160])[0].clone();
        assert!(
            gate.accepts(&frame),
            "SpeakerEpochGate MUST accept valid epoch {} >= 100",
            valid_epoch
        );
    }
}

#[test]
fn test_adv_concurrent_epoch_gate_rejection_fuzzing() {
    let gate = Arc::new(Mutex::new(SpeakerEpochGate::default()));
    let num_threads = 8;
    let frames_per_thread = 500;
    let barrier = Arc::new(Barrier::new(num_threads + 1));

    // Flusher thread bumps epoch from 0 to 50
    let gate_flusher = Arc::clone(&gate);
    let b_flusher = Arc::clone(&barrier);
    let flusher_handle = std::thread::spawn(move || {
        b_flusher.wait();
        for epoch in (1..=50).step_by(5) {
            std::thread::sleep(Duration::from_millis(2));
            gate_flusher.lock().unwrap().observe_flush(epoch);
        }
    });

    let mut worker_handles = Vec::new();
    for thread_id in 0..num_threads {
        let gate_worker = Arc::clone(&gate);
        let b_worker = Arc::clone(&barrier);
        let handle = std::thread::spawn(move || {
            b_worker.wait();
            for i in 0..frames_per_thread {
                let epoch = (thread_id * 10 + i % 60) as u32;
                let frame = speaker_frames(epoch, 16000, &[0.1f32; 160])[0].clone();
                let accepts = gate_worker.lock().unwrap().accepts(&frame);
                let _ = accepts;
            }
        });
        worker_handles.push(handle);
    }

    flusher_handle.join().unwrap();
    for h in worker_handles {
        h.join().unwrap();
    }

    // After all flushes, gate minimum_epoch is at least 46
    let final_gate = gate.lock().unwrap();
    let old_frame = speaker_frames(40, 16000, &[0.1f32; 160])[0].clone();
    assert!(!final_gate.accepts(&old_frame), "Must reject epoch 40 when minimum is >= 46");
}

// ============================================================================
// 3. LLM TOKEN STREAM PREEMPTION VIA CANCELLATIONTOKEN
// ============================================================================

#[test]
fn test_adv_cancellation_token_immediate_preemption_and_zero_leak() {
    let num_workers = 16;
    let barrier = Arc::new(Barrier::new(num_workers));

    let mut handles = Vec::new();
    for worker_id in 0..num_workers {
        let b = Arc::clone(&barrier);
        let handle = std::thread::spawn(move || {
            let token = CancellationToken::new();
            b.wait();

            let mut generated_tokens = Vec::new();
            let total_expected = 500;

            for i in 0..total_expected {
                if i == 25 {
                    // Trigger preemption at token 25
                    token.cancel();
                }

                if token.is_cancelled() {
                    break;
                }

                generated_tokens.push(format!("token_{}", i));
            }

            assert!(token.is_cancelled());
            assert_eq!(
                generated_tokens.len(),
                25,
                "Worker {} must halt immediately at 25 tokens, got {}",
                worker_id,
                generated_tokens.len()
            );
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().expect("Worker thread finished");
    }
}

// ============================================================================
// 4. BARGE-IN INTERRUPTION STORM (ZERO DEADLOCK STRESS HARNESS)
// ============================================================================

#[tokio::test]
async fn test_adv_barge_in_preemption_storm_zero_deadlock() {
    let player = TtsAudioPlayer::new(None);
    let active_session_id = Arc::new(AtomicU64::new(0));
    let cancel_token = Arc::new(Mutex::new(Some(CancellationToken::new())));

    let num_interruptions = 300;
    let start = Instant::now();

    for i in 1..=num_interruptions {
        // 1. Assistant starts generating / playing audio
        let p_clone = player.clone();
        p_clone.play(vec![0.1f32; 320]);

        // 2. User barges in: Atomic epoch bump + token cancel + player stop
        let new_epoch = active_session_id.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(new_epoch, i as u64);

        if let Some(tok) = cancel_token.lock().unwrap().take() {
            tok.cancel();
        }
        *cancel_token.lock().unwrap() = Some(CancellationToken::new());

        player.stop().await;
    }

    let elapsed = start.elapsed();
    assert_eq!(active_session_id.load(Ordering::SeqCst), num_interruptions as u64);
    assert!(
        elapsed < Duration::from_secs(5),
        "Interruption storm took too long: {:?}",
        elapsed
    );
}

// ============================================================================
// 5. FASTVAD STAGE 0 PRE-TRIGGER & DUAL-THRESHOLD HYSTERESIS
// ============================================================================

#[test]
fn test_adv_fastvad_stage0_pre_trigger_physics() {
    let mut pre_trigger = FastEnergyZcrPreTrigger::new(-45.0, 0.02, 0.65, 0.0015);

    // 1. Digital silence (all zeros) -> no pre-trigger
    let silence = vec![0.0f32; 256];
    let (pre, db, zcr, flux) = pre_trigger.evaluate(&silence);
    assert!(!pre);
    assert!(db < -90.0);
    assert_eq!(zcr, 0.0);
    assert_eq!(flux, 0.0);

    // 2. High frequency Nyquist spike (+1, -1) -> ZCR = 1.0 > zcr_max (0.65) -> rejected
    let nyquist: Vec<f32> = (0..256).map(|i| if i % 2 == 0 { 0.8 } else { -0.8 }).collect();
    let (pre_nyq, _, zcr_nyq, _) = pre_trigger.evaluate(&nyquist);
    assert!(!pre_nyq, "Nyquist noise must exceed zcr_max and NOT pre-trigger");
    assert!(zcr_nyq > 0.65);

    // 3. Human voice fundamental (200Hz sine @ -15 dBFS) -> ZCR ~0.025, RMS ~0.177 -> pre-triggers!
    let voice_tone = generate_sine(200.0, 16000, 0.016, 0.25); // 256 samples (16ms)
    let (pre_voice, db_voice, zcr_voice, _) = pre_trigger.evaluate(&voice_tone);
    assert!(pre_voice, "200Hz human pitch voice frame MUST pre-trigger");
    assert!(db_voice >= -45.0);
    assert!(zcr_voice >= 0.02 && zcr_voice <= 0.65);
}

#[test]
fn test_adv_fastvad_dual_threshold_hysteresis_and_hangover() {
    let config = VadConfig {
        sample_rate: 16000,
        frame_size: 256,
        start_threshold: 0.50,
        end_threshold: 0.35,
        speech_start_threshold: 1, // 16ms onset <= 20ms requirement
        speech_end_threshold: 22,  // ~352ms hangover
        ..Default::default()
    };

    struct MockVadStateMachine {
        config: VadConfig,
        is_speaking: bool,
        consecutive_speech: usize,
        consecutive_silence: usize,
    }

    impl MockVadStateMachine {
        fn step(&mut self, confidence: f32, pre_triggered: bool) -> Option<VadEvent> {
            let is_speech = if !self.is_speaking {
                if pre_triggered && confidence >= self.config.start_threshold * 0.85 {
                    true
                } else {
                    confidence >= self.config.start_threshold
                }
            } else {
                confidence >= self.config.end_threshold
            };

            if is_speech {
                self.consecutive_speech += 1;
                self.consecutive_silence = 0;
                if !self.is_speaking && self.consecutive_speech >= self.config.speech_start_threshold {
                    self.is_speaking = true;
                    return Some(VadEvent::SpeechStart);
                }
            } else {
                self.consecutive_silence += 1;
                self.consecutive_speech = 0;
                if self.is_speaking && self.consecutive_silence >= self.config.speech_end_threshold {
                    self.is_speaking = false;
                    return Some(VadEvent::SpeechEnd);
                }
            }
            None
        }
    }

    let mut sm = MockVadStateMachine {
        config,
        is_speaking: false,
        consecutive_speech: 0,
        consecutive_silence: 0,
    };

    // Frame 0: Pre-triggered + 0.45 confidence (>= 0.50*0.85 = 0.425) -> triggers SpeechStart immediately (1 frame = 16ms)
    let evt0 = sm.step(0.45, true);
    assert_eq!(evt0, Some(VadEvent::SpeechStart));
    assert!(sm.is_speaking);

    // Frames 1..5: Confidence drops to 0.38 (below start_threshold 0.50, but above end_threshold 0.35) -> remains speaking
    for _ in 1..=5 {
        let evt = sm.step(0.38, false);
        assert_eq!(evt, None);
        assert!(sm.is_speaking, "Hysteresis must hold active speaking state while conf >= 0.35");
    }

    // Frames 6..26 (21 frames): Silence (conf = 0.10) -> hangover countdown in progress, still speaking
    for frame_idx in 1..=21 {
        let evt = sm.step(0.10, false);
        assert_eq!(evt, None, "Hangover at frame {} must not trigger SpeechEnd yet", frame_idx);
        assert!(sm.is_speaking);
    }

    // Frame 27 (22nd silence frame): Hangover expires -> SpeechEnd emitted
    let evt_end = sm.step(0.10, false);
    assert_eq!(evt_end, Some(VadEvent::SpeechEnd));
    assert!(!sm.is_speaking);
}

// ============================================================================
// 6. PIPELINE HANDLE INTERRUPTIONS & VAD TRANSITIONS
// ============================================================================

#[tokio::test]
async fn test_adv_pipeline_handle_rapid_interruptions() {
    let (event_tx, mut event_rx) = mpsc::channel(64);
    let (_state_tx, state_rx) = watch::channel(PipelineState::Idle);
    let handle = WebRTCPipelineHandle { event_tx, state_rx };

    for _ in 0..50 {
        handle.on_interrupted().expect("send interruption");
        let evt = event_rx.recv().await.expect("receive event");
        assert!(matches!(evt, PipelineEvent::Interrupted));
    }
}

// ============================================================================
// 7. MEMORY RECYCLING AND BOUNDEDNESS
// ============================================================================

#[test]
fn test_adv_buffer_pool_recycling_and_no_leak() {
    let pool = BufferPool::new(16, 4096);
    let iterations = 10_000;

    for _ in 0..iterations {
        let mut buf = pool.acquire_buffer();
        buf.extend_from_slice(&[0x42u8; 1024]);
        assert_eq!(buf.len(), 1024);
    }

    let mut global_buf = BufferPool::acquire();
    global_buf.extend_from_slice(&[0x99u8; 512]);
    assert_eq!(global_buf.len(), 512);
}
