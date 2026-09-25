//! Adversarial stress test suite challenging Trailing Debounce Residue,
//! Preemption Hazards, and Empty Audio Input invariants.
//!
//! Milestone 2 Iteration 2 Challenger 2 Verification:
//! 1. Trailing Debounce scenario:
//!    - Feed 20.05 seconds of audio where the last 0.05s (800 samples) is followed by `VadEvent::SpeechEnd`.
//!    - Verify Turn 1 (20s / 320,000 samples) is emitted as `TurnAudioAction::Ended`.
//!    - Verify the 800 samples of trailing residue are NOT emitted as a separate `Ended` action
//!      (recycled to pre-roll because 800 < 1600 samples).
//!    - Verify that the trailing residue does NOT trigger `WebRTCActor::cancel_active_operations()`
//!      or abort Turn 1's transcription.
//! 2. Empty audio input:
//!    - Push empty slice `&[]` with `&[VadEvent::SpeechEnd]`.
//!    - Verify zero actions emitted and zero panics across idle and active states.
//! 3. Boundary & sweep stress:
//!    - Verify exact threshold behavior at 1599 vs 1600 samples boundary.
//!    - Adversarially sweep residue lengths from 1 to 1599 samples.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::frame::{OP_FLUSH, VoiceFrame};
use liva_native_core::webrtc::pipeline::{PipelineState, VoiceOutbound, WebRTCActor};
use liva_native_core::webrtc::session::{TurnAudioAction, TurnAudioBuffer};
use liva_native_core::webrtc::vad::VadEvent;
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;
use std::time::Duration;
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

// =============================================================================
// Test Suite 1: Trailing Debounce Scenario (20.05s Audio, 800 Samples Residue)
// =============================================================================

#[test]
fn challenge_trailing_debounce_20_05s_800_samples_residue_recycling() {
    // 20.05s of audio @ 16 kHz = 320,800 samples total.
    // Turn 1 = exactly 20.0s = 320,000 samples (MAX_TURN_SAMPLES).
    // Trailing residue = 0.05s = 800 samples (< MIN_TURN_SAMPLES = 1600).
    const SAMPLE_RATE: usize = 16_000;
    const TURN1_SAMPLES: usize = SAMPLE_RATE * 20; // 320,000 samples
    const RESIDUE_SAMPLES: usize = 800; // 0.05s
    const CHUNK_SIZE: usize = 1600; // 100ms
    const TURN1_CHUNKS: usize = TURN1_SAMPLES / CHUNK_SIZE; // 200 chunks

    let mut buffer = TurnAudioBuffer::new(1536);

    // Generate unique sequential audio samples to verify bit-exact preservation
    let mut turn1_audio = Vec::with_capacity(TURN1_SAMPLES);
    for i in 0..TURN1_SAMPLES {
        turn1_audio.push((i as f32) + 0.1);
    }
    let residue_audio: Vec<f32> = (0..RESIDUE_SAMPLES)
        .map(|i| (TURN1_SAMPLES + i) as f32 + 0.1)
        .collect();

    let mut started_actions = Vec::new();
    let mut ended_actions = Vec::new();

    // 1. Feed Turn 1: 200 chunks of 1600 samples (20.0s)
    for chunk_idx in 0..TURN1_CHUNKS {
        let chunk_slice = &turn1_audio[chunk_idx * CHUNK_SIZE..(chunk_idx + 1) * CHUNK_SIZE];
        let events = if chunk_idx == 0 {
            vec![VadEvent::SpeechStart]
        } else {
            vec![]
        };

        let actions = buffer.ingest(chunk_slice, &events);
        for action in actions {
            match action {
                TurnAudioAction::Started => started_actions.push(action),
                TurnAudioAction::Ended(audio) => ended_actions.push(audio),
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    // Assert Turn 1 emissions:
    assert_eq!(
        started_actions.len(),
        1,
        "Must emit exactly 1 Started action on initial SpeechStart"
    );
    assert_eq!(
        ended_actions.len(),
        1,
        "Turn 1 must be emitted as exactly 1 Ended action upon reaching 20s ceiling"
    );
    let turn1_emitted = &ended_actions[0];
    assert_eq!(
        turn1_emitted.len(),
        TURN1_SAMPLES,
        "Turn 1 emitted audio must be exactly 320,000 samples (20.0s)"
    );
    assert_eq!(
        turn1_emitted, &turn1_audio,
        "Turn 1 audio samples must be bit-exact match with original input"
    );

    // 2. Feed the trailing residue: 800 samples (0.05s) followed by VadEvent::SpeechEnd
    let residue_actions = buffer.ingest(&residue_audio, &[VadEvent::SpeechEnd]);

    // Assert trailing residue behavior:
    assert!(
        residue_actions.is_empty(),
        "Trailing residue (800 samples < 1600 MIN_TURN_SAMPLES) must NOT emit Ended action! Got: {:?}",
        residue_actions
    );

    // 3. Verify that the 800 samples of residue were recycled into pre-roll:
    // If a new speech utterance begins immediately, the pre-roll must contain the 800 residue samples.
    let next_chunk = vec![999.0f32; 1600];
    let next_actions = buffer.ingest(&next_chunk, &[VadEvent::SpeechStart]);
    assert_eq!(
        next_actions,
        vec![TurnAudioAction::Started],
        "Subsequent SpeechStart must emit Started"
    );

    // Now end the second utterance with SpeechEnd
    let end_actions = buffer.ingest(&next_chunk, &[VadEvent::SpeechEnd]);
    assert_eq!(end_actions.len(), 1);
    let TurnAudioAction::Ended(turn2_audio) = &end_actions[0] else {
        panic!("Expected Ended action for turn 2");
    };

    // Turn 2 must contain the 800 recycled pre-roll samples + next_chunk (1600) + next_chunk (1600) = 4000 samples
    assert_eq!(
        turn2_audio.len(),
        RESIDUE_SAMPLES + 1600 + 1600,
        "Turn 2 must include the 800 recycled pre-roll samples"
    );
    assert_eq!(
        &turn2_audio[..RESIDUE_SAMPLES],
        &residue_audio[..],
        "Recycled pre-roll samples in Turn 2 must match residue audio bit-for-bit"
    );
}

// =============================================================================
// Test Suite 2: Trailing Debounce WebRTCActor Preemption & STT Abort Prevention
// =============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn challenge_trailing_debounce_does_not_trigger_cancel_or_abort_stt() {
    let state = build_test_app_state();
    let (speaker_tx, _speaker_rx) = mpsc::channel::<VoiceFrame>(128);
    let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(128);

    let outbound = VoiceOutbound::new(speaker_tx, control_tx);
    let conversation_id = "debounce_preemption_stress".to_string();
    let aec = Arc::new(std::sync::Mutex::new(None));

    let (pipeline_handle, actor) = WebRTCActor::new(state.clone(), outbound, conversation_id, aec);
    let _actor_task = tokio::spawn(actor.run());

    let mut turn_audio_buffer = TurnAudioBuffer::new(1536);

    // Step 1: Ingest 20.0s audio to trigger Turn 1 completion
    let chunk = vec![0.1f32; 1600];
    let mut turn1_completed_audio = None;

    for chunk_idx in 0..200 {
        let events = if chunk_idx == 0 {
            vec![VadEvent::SpeechStart]
        } else {
            vec![]
        };

        for action in turn_audio_buffer.ingest(&chunk, &events) {
            match action {
                TurnAudioAction::Started => {
                    pipeline_handle.on_vad_start().expect("on_vad_start");
                }
                TurnAudioAction::Ended(audio) => {
                    turn1_completed_audio = Some(audio);
                }
                TurnAudioAction::SilenceProbe { .. } => {}
            }
        }
    }

    assert!(turn1_completed_audio.is_some(), "Turn 1 must end at 20s");
    let turn1_audio = turn1_completed_audio.unwrap();
    assert_eq!(turn1_audio.len(), 320_000);

    // Initial on_vad_start generated OP_FLUSH for Turn 1 start (epoch = 1)
    let flush1 = tokio::time::timeout(Duration::from_millis(100), control_rx.recv())
        .await
        .expect("Timeout waiting for Turn 1 start flush")
        .expect("Control channel closed");
    assert_eq!(flush1.op_code, OP_FLUSH);
    assert_eq!(flush1.seq_id, 1);

    // Step 2: Acquire STT lock to hold STT in an active, in-flight processing state
    let stt_lock = state.stt.lock().await;

    // Dispatch Turn 1 Ended to pipeline_handle (starts in-flight STT processing for Turn 1)
    pipeline_handle
        .on_vad_end(turn1_audio)
        .expect("on_vad_end for Turn 1");

    // on_vad_end generates OP_FLUSH (epoch = 2) and enters SttProcessing
    let flush2 = tokio::time::timeout(Duration::from_millis(100), control_rx.recv())
        .await
        .expect("Timeout waiting for Turn 1 end flush")
        .expect("Control channel closed");
    assert_eq!(flush2.op_code, OP_FLUSH);
    assert_eq!(flush2.seq_id, 2);

    // Wait a brief moment to ensure WebRTCActor has transitioned to SttProcessing
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        pipeline_handle.state(),
        PipelineState::SttProcessing,
        "Pipeline must be actively in SttProcessing state"
    );

    // Step 3: Simulate 800 samples of trailing debounce residue followed by SpeechEnd
    let residue = vec![0.05f32; 800];
    let residue_actions = turn_audio_buffer.ingest(&residue, &[VadEvent::SpeechEnd]);

    // Verify TurnAudioBuffer emitted ZERO actions
    assert!(
        residue_actions.is_empty(),
        "Trailing debounce must emit ZERO actions"
    );

    // Since residue_actions is empty, the WebSocket loop does NOT call pipeline_handle.on_vad_end or on_vad_start.
    // Therefore, NO cancel_active_operations() is triggered on WebRTCActor.
    // Verify that NO third OP_FLUSH is emitted on control_rx:
    let no_flush = tokio::time::timeout(Duration::from_millis(50), control_rx.recv()).await;
    assert!(
        no_flush.is_err(),
        "CRITICAL PREEMPTION HAZARD: Trailing debounce residue triggered unexpected OP_FLUSH or cancellation!"
    );

    // Verify pipeline state remains in SttProcessing, NOT preempted or reset to VadEnd/Idle
    assert_eq!(
        pipeline_handle.state(),
        PipelineState::SttProcessing,
        "Pipeline must remain in SttProcessing state while trailing debounce residue is processed"
    );

    // Step 4: Release the STT lock and allow STT task to complete cleanly
    drop(stt_lock);
}

// =============================================================================
// Test Suite 3: Empty Audio Input (`&[]` with `&[VadEvent::SpeechEnd]`)
// =============================================================================

#[test]
fn challenge_empty_audio_input_with_speech_end_zero_actions_zero_panics() {
    let mut buffer = TurnAudioBuffer::new(1536);

    // 1. Idle state: push empty slice &[] with &[VadEvent::SpeechEnd]
    let actions1 = buffer.ingest(&[], &[VadEvent::SpeechEnd]);
    assert!(
        actions1.is_empty(),
        "Empty slice with SpeechEnd on idle buffer must emit 0 actions. Got: {:?}",
        actions1
    );

    // 2. Push empty slice &[] with &[VadEvent::SpeechStart]
    let actions2 = buffer.ingest(&[], &[VadEvent::SpeechStart]);
    assert_eq!(
        actions2,
        vec![TurnAudioAction::Started],
        "Empty slice with SpeechStart must emit Started"
    );

    // 3. Active state with 0 samples accumulated: push empty slice &[] with &[VadEvent::SpeechEnd]
    let actions3 = buffer.ingest(&[], &[VadEvent::SpeechEnd]);
    assert!(
        actions3.is_empty(),
        "Empty slice with SpeechEnd on active empty buffer must emit 0 actions (0 < MIN_TURN_SAMPLES). Got: {:?}",
        actions3
    );

    // 4. Repeated empty slices with redundant SpeechEnd floods
    for _ in 0..100 {
        let actions = buffer.ingest(&[], &[VadEvent::SpeechEnd; 10]);
        assert!(
            actions.is_empty(),
            "Flood of SpeechEnd with empty audio must emit 0 actions"
        );
    }

    // 5. Active state with sub-minimum samples (e.g. 500 samples), then push &[] with SpeechEnd
    let sub_chunk = vec![0.5f32; 500];
    let _ = buffer.ingest(&sub_chunk, &[VadEvent::SpeechStart]);
    let actions_end = buffer.ingest(&[], &[VadEvent::SpeechEnd]);
    assert!(
        actions_end.is_empty(),
        "500 samples (< 1600 MIN_TURN_SAMPLES) followed by empty &[] with SpeechEnd must NOT emit Ended"
    );
}

// =============================================================================
// Test Suite 4: Boundary & Residue Sweep Stress (1 to 1600 Samples)
// =============================================================================

#[test]
fn challenge_exact_sample_floor_boundary_1599_vs_1600() {
    // Exact boundary check:
    // 1599 samples (< MIN_TURN_SAMPLES): must NOT emit Ended, must recycle to pre-roll
    // 1600 samples (== MIN_TURN_SAMPLES): MUST emit Ended

    // Sub-test A: 1599 samples
    let mut buffer_a = TurnAudioBuffer::new(2048);
    let chunk_1599 = vec![0.7f32; 1599];
    let _ = buffer_a.ingest(&chunk_1599, &[VadEvent::SpeechStart]);
    let actions_a = buffer_a.ingest(&[], &[VadEvent::SpeechEnd]);
    assert!(
        actions_a.is_empty(),
        "1599 samples (< 1600) must NOT emit Ended action"
    );

    // Sub-test B: 1600 samples
    let mut buffer_b = TurnAudioBuffer::new(2048);
    let chunk_1600 = vec![0.8f32; 1600];
    let _ = buffer_b.ingest(&chunk_1600, &[VadEvent::SpeechStart]);
    let actions_b = buffer_b.ingest(&[], &[VadEvent::SpeechEnd]);
    assert_eq!(
        actions_b.len(),
        1,
        "1600 samples (== MIN_TURN_SAMPLES) MUST emit Ended action"
    );
    let TurnAudioAction::Ended(turn_b) = &actions_b[0] else {
        panic!("Expected Ended action");
    };
    assert_eq!(turn_b.len(), 1600);
}

#[test]
fn challenge_sub_minimum_residue_sweep_1_to_1599_samples() {
    // Adversarially test various residue lengths across the 1..1600 range
    let test_lengths = [1, 2, 8, 16, 64, 128, 256, 512, 800, 1024, 1500, 1598, 1599];

    for &len in &test_lengths {
        let mut buffer = TurnAudioBuffer::new(2048);
        let audio = vec![0.33f32; len];

        let _ = buffer.ingest(&audio, &[VadEvent::SpeechStart]);
        let actions = buffer.ingest(&[], &[VadEvent::SpeechEnd]);

        assert!(
            actions.is_empty(),
            "Length {} (< 1600) with SpeechEnd must NOT emit Ended action",
            len
        );
    }
}
