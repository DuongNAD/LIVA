use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::voice_coordinator::{VoiceCoordinator, VoiceIpcEvent};
use liva_native_core::{AppState, db, stt, tts};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
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

#[tokio::test]
async fn test_voice_coordinator_event_dispatching() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "dispatch-conv".to_string()).await);

    let (tx, mut rx) = mpsc::channel(16);
    coordinator.subscribe(Arc::new(move |evt| {
        let _ = tx.try_send(evt);
    }));

    // Send a text event
    coordinator.send_text_event("test_event", serde_json::json!({"foo": "bar"}));

    let received = tokio::time::timeout(Duration::from_millis(500), rx.recv()).await;
    assert!(received.is_ok(), "timeout waiting for text event");
    let event = received.unwrap().expect("event received");
    match event {
        VoiceIpcEvent::TextEvent { event, payload } => {
            assert_eq!(event, "test_event");
            assert_eq!(payload["foo"], "bar");
        }
        _ => panic!("unexpected event variant"),
    }

    coordinator.unsubscribe();
}

#[tokio::test]
async fn test_voice_coordinator_mic_chunk_ingestion() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "mic-conv".to_string()).await);

    let samples = vec![0.1_f32; 160];
    let res = coordinator.ingest_mic_chunk(1, samples).await;
    assert!(res.is_ok(), "ingest_mic_chunk should succeed");

    let next_samples = vec![0.2_f32; 160];
    let res2 = coordinator.ingest_mic_chunk(2, next_samples).await;
    assert!(res2.is_ok(), "subsequent ingest_mic_chunk should succeed");
}

#[tokio::test]
async fn test_voice_coordinator_wake_probe() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "wake-conv".to_string()).await);

    let probe_samples = vec![0.0_f32; 320];
    let probe_resp = coordinator
        .evaluate_wake_probe(1, probe_samples)
        .await
        .expect("evaluate_wake_probe should succeed");

    assert_eq!(probe_resp.seq_id, 1);
    // Silent samples should not match wake word
    assert!(!probe_resp.matched);
}

#[tokio::test]
async fn test_voice_coordinator_barge_in_interrupt() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "barge-conv".to_string()).await);

    let (tx, mut rx) = mpsc::channel(16);
    coordinator.subscribe(Arc::new(move |evt| {
        let _ = tx.try_send(evt);
    }));

    // Trigger interrupt
    coordinator.interrupt().expect("interrupt should succeed");

    coordinator.unsubscribe();
}

#[tokio::test]
async fn test_voice_coordinator_subscriber_lifecycle() {
    let state = test_state();
    let coordinator = Arc::new(VoiceCoordinator::new(state, "multi-conv".to_string()).await);

    let count_1 = Arc::new(AtomicUsize::new(0));
    let c1 = Arc::clone(&count_1);
    coordinator.subscribe(Arc::new(move |_| {
        c1.fetch_add(1, Ordering::SeqCst);
    }));

    // Dispatch 5 text events
    for i in 0..5 {
        coordinator.send_text_event("multi", serde_json::json!({ "i": i }));
    }

    assert_eq!(count_1.load(Ordering::SeqCst), 5);

    // Unsubscribe
    coordinator.unsubscribe();

    coordinator.send_text_event("multi_after", serde_json::json!({}));

    // Count remains unchanged after unsubscribe
    assert_eq!(count_1.load(Ordering::SeqCst), 5);
}
