use liva_native_core::webrtc::frame::{OP_FLUSH, VoiceFrame};
use liva_native_core::webrtc::pipeline::{
    PipelineEvent, PipelineState, VoiceOutbound, WebRTCActor,
};
use liva_native_core::webrtc::vad::{VadConfig, VadEngine, VadEvent, compute_stage0_metrics};
use liva_native_core::{AppState, crypto, db, llm, stt, tts};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("\n=== Running Duplex Voice Streaming Pipeline Verification ===\n");

    // 1. Test Stage 0 Instantaneous Energy & ZCR Computation (<1ms)
    println!("--- Testing Stage 0 Instantaneous Energy & ZCR Detector ---");
    let silence_frame = vec![0.0f32; 160];
    let stage0_silence = compute_stage0_metrics(&silence_frame, 0.001, 0.01, 0.50);
    assert_eq!(stage0_silence.rms_energy, 0.0);
    assert_eq!(stage0_silence.zcr, 0.0);
    assert!(!stage0_silence.is_active, "Silence must be inactive");

    // Generate 1kHz test sine tone at 16kHz
    let mut speech_tone = Vec::with_capacity(160);
    for i in 0..160 {
        let sample = (2.0 * std::f32::consts::PI * i as f32 / 16.0).sin() * 0.4;
        speech_tone.push(sample);
    }
    let stage0_start = Instant::now();
    let stage0_speech = compute_stage0_metrics(&speech_tone, 0.001, 0.01, 0.50);
    let stage0_latency = stage0_start.elapsed();
    println!("Stage 0 Energy/ZCR compute latency: {:?}", stage0_latency);
    assert!(
        stage0_latency < Duration::from_millis(1),
        "Stage 0 compute must be strictly under 1ms (<10µs typical)"
    );
    assert!(stage0_speech.is_active, "Speech tone must be active");
    assert!(
        stage0_speech.rms_energy > 0.25,
        "Speech tone RMS energy should exceed threshold"
    );
    println!("✅ Stage 0 Energy & ZCR detector verified successfully.\n");

    // 2. Test VAD Debounce State Machine Transitions & Fast Single-Frame Trigger
    println!("--- Testing VAD Debounce Logic & Fast-Start Trigger Transitions ---");
    let mut model_dir =
        std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| "models/nemotron-asr".to_string());
    if !std::path::Path::new(&model_dir).exists() {
        model_dir = "../models/nemotron-asr".to_string();
    }
    let vad_model_path = liva_native_core::webrtc::vad::resolve_model_path(&model_dir);
    if !vad_model_path.exists() {
        return Err(format!("VAD model not found at {:?}", vad_model_path));
    }
    println!("VAD model: {:?}", vad_model_path);

    let mut vad_engine = VadEngine::new(&vad_model_path, VadConfig::default())?;

    // Check default state
    assert!(
        !vad_engine.is_speaking(),
        "VAD engine should start in silent state"
    );

    // Standard debounce (low confidence): requires 3 consecutive speech frames
    let event1 = vad_engine.test_update_state_machine(true);
    assert_eq!(
        event1, None,
        "1 frame of low-confidence speech should not trigger SpeechStart"
    );
    let event2 = vad_engine.test_update_state_machine(true);
    assert_eq!(
        event2, None,
        "2 frames of low-confidence speech should not trigger SpeechStart"
    );
    let event3 = vad_engine.test_update_state_machine(true);
    assert_eq!(
        event3,
        Some(VadEvent::SpeechStart),
        "3 consecutive speech frames must trigger SpeechStart"
    );
    assert!(
        vad_engine.is_speaking(),
        "VAD engine should be in speaking state"
    );

    // Test transition back to silence: requires 45 consecutive silence frames
    for i in 1..45 {
        let ev = vad_engine.test_update_state_machine(false);
        assert_eq!(
            ev, None,
            "Frame {} of silence should not trigger SpeechEnd yet",
            i
        );
    }
    let event_end = vad_engine.test_update_state_machine(false);
    assert_eq!(
        event_end,
        Some(VadEvent::SpeechEnd),
        "45 consecutive silence frames must trigger SpeechEnd"
    );
    assert!(
        !vad_engine.is_speaking(),
        "VAD engine should be back to silent state"
    );

    // Test Fast Single-Frame Trigger (p >= 0.85)
    let fast_event = vad_engine.test_update_state_machine_with_confidence(true, 0.92, 0.02);
    assert_eq!(
        fast_event,
        Some(VadEvent::SpeechStart),
        "Single high-confidence frame must trigger SpeechStart immediately (<=20-25ms target)"
    );
    assert!(vad_engine.is_speaking());
    vad_engine.reset();
    println!("✅ VAD Debounce & Fast-Start Logic verified successfully.\n");

    // 3. Test Real VAD ONNX Inference Execution across Frame Sizes (160, 256, 512)
    println!("--- Testing Real VAD ONNX Multi-Frame Inference (160, 256, 512 samples) ---");
    for &frame_size in &[160usize, 256usize, 512usize] {
        let config = VadConfig {
            frame_size,
            ..VadConfig::default()
        };
        let mut multi_engine = VadEngine::new(&vad_model_path, config)?;
        let test_frame = vec![0.0f32; frame_size];

        // Warmup
        let _ = multi_engine.process_audio(&test_frame)?;

        // Measure inference latency
        let start_inf = Instant::now();
        let _ = multi_engine.process_audio(&test_frame)?;
        let inf_latency = start_inf.elapsed();
        let duration_ms = (frame_size as f32 / 16000.0) * 1000.0;
        println!(
            "VAD frame size {} samples ({:.1}ms) - ONNX Inference latency: {:?}",
            frame_size, duration_ms, inf_latency
        );
        assert!(
            inf_latency < Duration::from_millis(15),
            "VAD inference latency must be under 15ms"
        );
    }
    println!("✅ Multi-frame VAD ONNX Inference verified successfully.\n");

    // 4. Test Coordinator State Machine, Interruptions, and Preemption Latency
    println!("--- Testing Actor-Coordinator Pipeline Flow ---");
    let db = db::DatabasePool::new_in_memory().map_err(|e| e.to_string())?;
    let crypto = crypto::EncryptionEngine::new("00000000000000000000000000000000");
    let stt = tokio::sync::Mutex::new(stt::SttManager::new("non_existent_dir"));
    let tts = tokio::sync::Mutex::new(None);
    let tts_player = tts::audio::TtsAudioPlayer::new(None);
    let llm = tokio::sync::Mutex::new(llm::LlamaRouterManager::new(2048, 0)?);

    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        1920,
        1080,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        mock_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    let state = Arc::new(AppState {
        db,
        crypto,
        stt,
        tts,
        tts_player,
        llm,
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: std::sync::Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    let (speaker_tx, _speaker_rx) = mpsc::channel::<VoiceFrame>(128);
    let (control_tx, mut control_rx) = mpsc::channel::<VoiceFrame>(16);

    let (pipeline_handle, actor) = WebRTCActor::new(
        state,
        VoiceOutbound::new(speaker_tx, control_tx),
        "verify_duplex".to_string(),
        Arc::new(std::sync::Mutex::new(None)),
    );
    let actor_handle = tokio::spawn(actor.run());

    // Initial state check
    assert_eq!(pipeline_handle.state(), PipelineState::Idle);

    // Call on_vad_start -> transitions to VadStart
    pipeline_handle.on_vad_start()?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);

    // Outgoing channel must receive FLUSH frame
    let frame1 = control_rx
        .try_recv()
        .map_err(|e| format!("Expected FLUSH frame: {}", e))?;
    assert_eq!(frame1.op_code, OP_FLUSH, "Expected OP_FLUSH opcode");

    pipeline_handle.on_vad_end(vec![0.0f32; 1024])?;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let state = pipeline_handle.state();
    assert!(
        state == PipelineState::SttProcessing || state == PipelineState::Idle,
        "State must be SttProcessing or Idle"
    );

    // Test Preemption and Barge-in latency (<10ms)
    println!("Measuring barge-in/interruption preemption latency...");
    let preemption_start = Instant::now();
    pipeline_handle.on_vad_start()?;

    // We should receive FLUSH frame immediately
    let flush_frame = tokio::time::timeout(Duration::from_millis(100), control_rx.recv())
        .await
        .map_err(|e| format!("Timeout waiting for FLUSH: {}", e))?
        .ok_or_else(|| "Outgoing channel closed".to_string())?;

    let preemption_elapsed = preemption_start.elapsed();
    println!("Interruption preemption latency: {:?}", preemption_elapsed);
    assert_eq!(flush_frame.op_code, OP_FLUSH);
    assert!(
        preemption_elapsed < Duration::from_millis(10),
        "Interruption latency must be strictly under 10ms"
    );

    // Verify state transitioned to VadStart
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);
    println!("✅ Interruption & Barge-in preemption latency verified successfully.\n");

    // 5. Test Late/Stale Callback Safety (Monotonic Session ID)
    println!("--- Testing Monotonic Session ID Callback Safety ---");
    // Send SttCompleted with a stale session ID (the current session_id has incremented due to interruption)
    let stale_event = PipelineEvent::SttCompleted {
        session_id: 0, // Old session
        result: Ok(Some("Hello".to_string())),
    };

    pipeline_handle
        .event_tx
        .send(stale_event)
        .await
        .map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(20)).await;

    // State must remain in VadStart, and NOT transition to LlmGenerating/Idle
    assert_eq!(
        pipeline_handle.state(),
        PipelineState::VadStart,
        "State must remain in VadStart and ignore stale STT completed events"
    );
    println!("✅ Monotonic Session ID protection verified successfully.\n");

    // Shutdown actor
    actor_handle.abort();
    let _ = actor_handle.await;

    println!("🎉 All duplex and VAD verification checks passed successfully!");
    Ok(())
}
