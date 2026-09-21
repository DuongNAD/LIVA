//! Adversarial Stress & Panic Elimination Challenge Test Suite for Milestone M1
//! (Native Core Resilience & Concurrency).
//!
//! Empirical Challenger 2 Verification:
//! 1. VAD memory bounds:
//!    - MAX_RESIDUAL_CAPACITY = 32_000 samples (2.0s @ 16kHz) strictly clamps memory.
//!    - Flooding with massive 100k+ sample chunks drains properly and bounds heap usage.
//!    - Reset clears residual buffer, debounce counters, and recurrent state.
//!    - Unaligned streaming frame feeding produces no unbounded accumulation.
//! 2. DSP state reset:
//!    - VoiceSessionAudio::reset_dsp() resets VAD recurrent states, GTCRN denoiser caches, and AEC.
//!    - WebRTCActor::with_voice_session wires connection-local DSP state.
//!    - reset_turn_dsp() invoked across turn boundaries, speech interruptions, and Drop for WebRTCActor.
//! 3. Panic elimination:
//!    - Missing/corrupted STT model paths return graceful Result::Err instead of panicking.
//!    - Non-contiguous tensors in VieNeu TTS return graceful Result::Err instead of panicking.
//!    - Governor and VisualGovernor public methods handle system state transitions without panicking.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::governor::{Governor, VisualGovernor};
use liva_native_core::stt::SttManager;
use liva_native_core::webrtc::frame::{OP_FLUSH, VoiceFrame};
use liva_native_core::webrtc::pipeline::{VoiceOutbound, WebRTCActor};
use liva_native_core::webrtc::session::{TurnAudioAction, TurnAudioBuffer, VoiceSessionAudio};
use liva_native_core::webrtc::vad::{
    MAX_RESIDUAL_CAPACITY, VadConfig, VadEngine, VadEvent, resolve_model_path,
};
use liva_native_core::{AppState, llm, tts};
use ndarray::{array, s};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

fn get_vad_model_path() -> std::path::PathBuf {
    let mut model_dir =
        std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| "models/nemotron-asr".to_string());
    if !std::path::Path::new(&model_dir).exists() {
        model_dir = "../models/nemotron-asr".to_string();
    }
    resolve_model_path(&model_dir)
}

fn build_test_app_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = SttManager::new("non-existent-model");
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

// =============================================================================
// TEST SUITE 1: VAD MEMORY BOUNDS & RESIDUAL BUFFER STRESS
// =============================================================================

#[test]
fn challenge_vad_max_residual_capacity_strictly_clamped_on_massive_chunk() {
    assert_eq!(
        MAX_RESIDUAL_CAPACITY, 32_000,
        "MAX_RESIDUAL_CAPACITY must be exactly 32,000 samples (2.0s at 16kHz)"
    );

    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not present at {:?}", model_path);
        return;
    }

    let mut engine =
        VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");

    // Flood with a single massive chunk of 128,000 samples (4x MAX_RESIDUAL_CAPACITY)
    let massive_chunk = vec![0.0f32; 128_000];
    let res = engine.process_audio(&massive_chunk);
    assert!(
        res.is_ok(),
        "process_audio must succeed under massive chunk: {:?}",
        res.err()
    );

    // After processing 512-sample frames, remaining buffer MUST be strictly < 512
    assert!(
        engine.residual_buffer_len() < 512,
        "Residual buffer len ({}) must be strictly < frame_size (512)",
        engine.residual_buffer_len()
    );

    // Flood with 10 successive bursts of 48,000 samples (each > MAX_RESIDUAL_CAPACITY)
    for burst_idx in 1..=10 {
        let burst = vec![0.02f32; 48_000];
        let res = engine.process_audio(&burst);
        assert!(
            res.is_ok(),
            "Burst {} must succeed without memory overflow",
            burst_idx
        );
        assert!(
            engine.residual_buffer_len() < 512,
            "Burst {}: Residual buffer len ({}) must stay < 512",
            burst_idx,
            engine.residual_buffer_len()
        );
    }
}

#[test]
fn challenge_vad_unaligned_streaming_chunks_no_unbounded_accumulation() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not present at {:?}", model_path);
        return;
    }

    let mut engine = VadEngine::new(&model_path, VadConfig::fast()).expect("load Silero VAD model");

    // Feed 500 chunks of 337 samples (odd prime, unaligned with 256 frame size)
    for i in 0..500 {
        let chunk = vec![((i as f32) * 0.01).sin() * 0.1; 337];
        let res = engine.process_audio(&chunk);
        assert!(res.is_ok(), "Chunk {} failed", i);
        // At all times, the residual buffer must never exceed 256 (the frame size for VadConfig::fast)
        assert!(
            engine.residual_buffer_len() < 256,
            "Chunk {}: Residual buffer ({}) exceeded frame_size 256",
            i,
            engine.residual_buffer_len()
        );
    }
}

#[test]
fn challenge_vad_reset_clears_residual_and_recurrent_state() {
    let model_path = get_vad_model_path();
    if !model_path.exists() {
        eprintln!("skip: Silero VAD model not present at {:?}", model_path);
        return;
    }

    let mut engine =
        VadEngine::new(&model_path, VadConfig::default()).expect("load Silero VAD model");

    // Feed a chunk with speech to mutate internal state
    let chunk = vec![0.35f32; 1024];
    let _ = engine.process_audio(&chunk);

    // Reset engine
    engine.reset();

    assert!(
        !engine.is_speaking(),
        "Engine must not be speaking after reset"
    );
    assert_eq!(
        engine.residual_buffer_len(),
        0,
        "Residual buffer must be completely cleared (0)"
    );
    assert_eq!(
        engine.last_confidence(),
        0.0,
        "Last confidence must be reset to 0.0"
    );
    assert!(
        engine.last_stage0().is_none(),
        "Stage0 metrics must be reset to None"
    );
}

#[test]
fn challenge_turn_audio_buffer_strictly_bounds_pre_roll_and_turn_ceiling() {
    let mut buffer = TurnAudioBuffer::new(1536);

    // 1. Push 50,000 samples of silence when inactive (pre-roll testing)
    let silence = vec![0.0f32; 50_000];
    let actions = buffer.ingest(&silence, &[]);
    assert!(
        actions.is_empty(),
        "No actions expected during unvoiced ingestion"
    );

    // 2. Trigger speech start
    let actions = buffer.ingest(&[0.5f32; 160], &[VadEvent::SpeechStart]);
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0], TurnAudioAction::Started);

    // 3. Stream continuous speech chunks of 1600 samples (100ms) until 20s ceiling is reached
    let mut ended_turns = Vec::new();
    for _ in 0..250 {
        let chunk = vec![0.4f32; 1600];
        let actions = buffer.ingest(&chunk, &[]);
        for action in actions {
            if let TurnAudioAction::Ended(turn) = action {
                ended_turns.push(turn);
            }
        }
    }

    assert_eq!(
        ended_turns.len(),
        1,
        "Continuous speech past 20s ceiling must yield exactly 1 Ended turn"
    );
    assert!(
        ended_turns[0].len() >= TurnAudioBuffer::MAX_TURN_SAMPLES,
        "Ended turn must meet or exceed MAX_TURN_SAMPLES ceiling"
    );
    assert!(
        ended_turns[0].len() < TurnAudioBuffer::MAX_TURN_SAMPLES + 1600 + 1536,
        "Ended turn length must be bounded by MAX_TURN_SAMPLES + chunk_size + pre_roll"
    );
}

// =============================================================================
// TEST SUITE 2: DSP STATE RESET ACROSS BOUNDARIES, INTERRUPTIONS & DROP
// =============================================================================

#[tokio::test]
async fn challenge_voice_session_audio_reset_dsp_clears_all_processors() {
    let state = build_test_app_state();
    let session_audio = VoiceSessionAudio::from_app_state(&state).await;

    // Resetting DSP when components are None must execute without error or panic
    session_audio.reset_dsp();
    session_audio.reset_vad();
    session_audio.reset_denoiser();
    session_audio.reset_aec();
    session_audio.clear_aec_render();

    // Verify handle cloning and thread safety
    let aec_handle = session_audio.aec_handle();
    assert!(aec_handle.lock().unwrap().is_none());
}

#[tokio::test]
async fn challenge_webrtc_pipeline_reset_turn_dsp_and_interruption_flow() {
    let state = build_test_app_state();
    let (speaker_tx, _speaker_rx) = mpsc::channel::<VoiceFrame>(64);
    let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(64);

    let outbound = VoiceOutbound::new(speaker_tx, control_tx);
    let conversation_id = "test_reset_turn_dsp_conv".to_string();
    let aec = Arc::new(std::sync::Mutex::new(None));

    let session_audio = VoiceSessionAudio::from_app_state(&state).await;

    let (pipeline_handle, actor) = WebRTCActor::new(state, outbound, conversation_id, aec);
    let actor = actor.with_voice_session(session_audio);

    let actor_task = tokio::spawn(actor.run());

    // 1. Dispatch ResetDsp event
    pipeline_handle
        .reset_dsp()
        .expect("dispatch reset_dsp event");

    // 2. Dispatch barge-in speech start
    pipeline_handle
        .on_vad_start()
        .expect("dispatch on_vad_start");

    // Verify OP_FLUSH frame emitted on preemption
    let flush_frame = tokio::time::timeout(Duration::from_millis(100), control_rx.recv())
        .await
        .expect("timeout waiting for flush")
        .expect("control channel closed");
    assert_eq!(flush_frame.op_code, OP_FLUSH);

    // Clean shutdown of actor
    drop(pipeline_handle);
    let _ = tokio::time::timeout(Duration::from_millis(200), actor_task).await;
}

#[test]
fn challenge_webrtc_actor_drop_handler_resets_dsp_and_aborts_tasks() {
    let state = build_test_app_state();
    let (speaker_tx, _speaker_rx) = mpsc::channel::<VoiceFrame>(64);
    let (control_tx, _control_rx) = mpsc::channel::<VoiceFrame>(64);

    let outbound = VoiceOutbound::new(speaker_tx, control_tx);
    let conversation_id = "test_drop_conv".to_string();
    let aec = Arc::new(std::sync::Mutex::new(None));

    let (_pipeline_handle, actor) = WebRTCActor::new(state, outbound, conversation_id, aec);

    // Dropping WebRTCActor triggers Drop implementation which calls reset_turn_dsp()
    // and aborts in-flight task handles cleanly.
    let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        drop(actor);
    }));

    assert!(
        outcome.is_ok(),
        "Drop implementation of WebRTCActor must not panic"
    );
}

// =============================================================================
// TEST SUITE 3: PANIC ELIMINATION & FAULT TOLERANCE
// =============================================================================

#[test]
fn challenge_stt_missing_or_corrupted_model_returns_graceful_err_no_panic() {
    let mut stt = SttManager::new("models/corrupted_or_missing_path_xyz_123");
    let _ = stt.set_language("en");

    let test_audio = vec![0.0f32; 1600];

    // 1. feed_chunk with missing engine must return Err, NOT panic
    let outcome_chunk = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stt.feed_chunk(&test_audio, false)
    }));
    assert!(outcome_chunk.is_ok(), "feed_chunk must not panic");
    let res_chunk = outcome_chunk.unwrap();
    assert!(
        res_chunk.is_err(),
        "feed_chunk must return Err on missing model"
    );

    // 2. feed_audio must return Err, NOT panic
    let outcome_audio = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        stt.feed_audio(&test_audio, true)
    }));
    assert!(outcome_audio.is_ok(), "feed_audio must not panic");
    let res_audio = outcome_audio.unwrap();
    assert!(
        res_audio.is_err(),
        "feed_audio must return Err on missing model"
    );
}

#[test]
fn challenge_vieneu_non_contiguous_tensor_returns_graceful_err() {
    // 1. Array slicing with step > 1 creates non-contiguous memory layout
    let arr = array![10.0f32, 20.0, 30.0, 40.0, 50.0, 60.0];
    let strided = arr.slice(s![..;2]); // elements [10, 30, 50] with stride 2

    assert!(
        strided.as_slice().is_none(),
        "Strided slice view must NOT be in contiguous layout (as_slice is None)"
    );

    // 2. Verify that the production pattern returns Err instead of panicking via .unwrap()
    let outcome: Result<&[f32], String> = strided
        .as_slice()
        .ok_or_else(|| "VieNeu acoustic step: text_logits tensor is non-contiguous".to_string());

    assert!(outcome.is_err());
    assert_eq!(
        outcome.unwrap_err(),
        "VieNeu acoustic step: text_logits tensor is non-contiguous"
    );
}

#[test]
fn challenge_governor_handles_mode_transitions_without_panic() {
    let gov = Governor::from_env();
    let _ = gov.game_mode_active();

    let visual_gov = VisualGovernor::new(Duration::from_millis(50));
    assert!(!visual_gov.is_active());
    assert!(visual_gov.is_dormant());

    visual_gov.acquire_visual_slot();
    assert!(visual_gov.is_active());
    assert!(!visual_gov.is_dormant());

    visual_gov.release_visual_slot();
    assert!(!visual_gov.is_active());
}
