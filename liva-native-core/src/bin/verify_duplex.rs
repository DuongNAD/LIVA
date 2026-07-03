#![allow(dead_code, unused_imports, unused_variables)]
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use liva_native_core::{
    crypto, db, stt, tts, llm, webrtc, AppState
};
use liva_native_core::webrtc::frame::{VoiceFrame, OP_FLUSH};
use liva_native_core::webrtc::pipeline::{WebRTCActor, PipelineState, PipelineEvent};
use liva_native_core::webrtc::vad::{VadEngine, VadConfig, VadEvent};

#[tokio::main]
async fn main() -> Result<(), String> {
    println!("\n=== Running Duplex Voice Streaming Pipeline Verification ===\n");

    // 1. Test VAD Debounce State Machine Transitions
    println!("--- Testing VAD Debounce Logic Transitions ---");
    let mut model_dir = std::env::var("LIVA_STT_MODEL_DIR")
        .unwrap_or_else(|_| "models/nemotron-asr".to_string());
    let mut vad_model_path = std::path::Path::new(&model_dir).join("silero_vad.onnx");
    if !vad_model_path.exists() {
        model_dir = "../models/nemotron-asr".to_string();
        vad_model_path = std::path::Path::new(&model_dir).join("silero_vad.onnx");
    }
    if !vad_model_path.exists() {
        return Err(format!("VAD model not found at {:?}", vad_model_path));
    }

    let mut vad_engine = VadEngine::new(&vad_model_path, VadConfig::default())?;

    // Check default state
    assert!(!vad_engine.is_speaking(), "VAD engine should start in silent state");

    // Test transition to speech: requires 3 consecutive speech frames
    let event1 = vad_engine.test_update_state_machine(true);
    assert_eq!(event1, None, "1 frame of speech should not trigger SpeechStart");
    let event2 = vad_engine.test_update_state_machine(true);
    assert_eq!(event2, None, "2 frames of speech should not trigger SpeechStart");
    let event3 = vad_engine.test_update_state_machine(true);
    assert_eq!(event3, Some(VadEvent::SpeechStart), "3 consecutive speech frames must trigger SpeechStart");
    assert!(vad_engine.is_speaking(), "VAD engine should be in speaking state");

    // Test transition back to silence: requires 45 consecutive silence frames
    for i in 1..45 {
        let ev = vad_engine.test_update_state_machine(false);
        assert_eq!(ev, None, "Frame {} of silence should not trigger SpeechEnd yet", i);
    }
    let event_end = vad_engine.test_update_state_machine(false);
    assert_eq!(event_end, Some(VadEvent::SpeechEnd), "45 consecutive silence frames must trigger SpeechEnd");
    assert!(!vad_engine.is_speaking(), "VAD engine should be back to silent state");
    println!("✅ VAD Debounce Logic Transitions verified successfully.\n");


    // 2. Test VAD Engine ONNX Model execution and latency
    println!("--- Testing Real VAD ONNX Inference Execution ---");
    let dummy_frame = vec![0.0f32; 512];
    
    // Warmup run
    let _ = vad_engine.process_audio(&dummy_frame)?;
    
    // Measure inference latency
    let start_inf = Instant::now();
    let _ = vad_engine.process_audio(&dummy_frame)?;
    let inf_latency = start_inf.elapsed();
    println!("VAD ONNX CPU Inference latency: {:?}", inf_latency);
    assert!(inf_latency < Duration::from_millis(15), "VAD inference latency must be under 15ms (ideally <8ms)");
    println!("✅ VAD ONNX Inference verified successfully.\n");


    // 3. Test Coordinator State Machine, Interruptions, and Latency
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
        mcp_server: std::sync::Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<VoiceFrame>(128);

    let (pipeline_handle, actor) = WebRTCActor::new(state, outgoing_tx);
    let actor_handle = tokio::spawn(actor.run());

    // Initial state check
    assert_eq!(pipeline_handle.state(), PipelineState::Idle);

    // Call on_vad_start -> transitions to VadStart
    pipeline_handle.on_vad_start()?;
    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);

    // Outgoing channel must receive FLUSH frame
    let frame1 = outgoing_rx.try_recv().map_err(|e| format!("Expected FLUSH frame: {}", e))?;
    assert_eq!(frame1.op_code, OP_FLUSH, "Expected OP_FLUSH opcode");

    pipeline_handle.on_vad_end(vec![0.0f32; 1024])?;
    tokio::time::sleep(Duration::from_millis(5)).await;
    let state = pipeline_handle.state();
    assert!(state == PipelineState::SttProcessing || state == PipelineState::Idle, "State must be SttProcessing or Idle");

    // Test Preemption and Barge-in latency (<10ms)
    println!("Measuring barge-in/interruption preemption latency...");
    let preemption_start = Instant::now();
    pipeline_handle.on_vad_start()?;
    
    // We should receive FLUSH frame immediately
    let flush_frame = tokio::time::timeout(Duration::from_millis(100), outgoing_rx.recv())
        .await
        .map_err(|e| format!("Timeout waiting for FLUSH: {}", e))?
        .ok_or_else(|| "Outgoing channel closed".to_string())?;
    
    let preemption_elapsed = preemption_start.elapsed();
    println!("Interruption preemption latency: {:?}", preemption_elapsed);
    assert_eq!(flush_frame.op_code, OP_FLUSH);
    assert!(preemption_elapsed < Duration::from_millis(10), "Interruption latency must be strictly under 10ms");
    
    // Verify state transitioned to VadStart
    tokio::time::sleep(Duration::from_millis(5)).await;
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart);
    println!("✅ Interruption & Barge-in preemption latency verified successfully.\n");


    // 4. Test Late/Stale Callback Safety (Monotonic Session ID)
    println!("--- Testing Monotonic Session ID Callback Safety ---");
    // Send SttCompleted with a stale session ID (the current session_id has incremented due to interruption)
    let stale_event = PipelineEvent::SttCompleted {
        session_id: 0, // Old session
        result: Ok(Some("Hello".to_string())),
    };
    
    pipeline_handle.event_tx.send(stale_event).await.map_err(|e| e.to_string())?;
    tokio::time::sleep(Duration::from_millis(20)).await;
    
    // State must remain in VadStart, and NOT transition to LlmGenerating/Idle
    assert_eq!(pipeline_handle.state(), PipelineState::VadStart, "State must remain in VadStart and ignore stale STT completed events");
    println!("✅ Monotonic Session ID protection verified successfully.\n");


    // Shutdown actor
    actor_handle.abort();
    let _ = actor_handle.await;

    println!("🎉 All verification checks passed successfully!");
    Ok(())
}
