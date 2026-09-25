//! Empirical Adversarial Integration Suite — Challenger 2 (Milestone 1, Requirement R2)
//!
//! Verifies LlmActor integration within AppState:
//! 1. Client receivers dropped prematurely: actor does not crash, AppState.llm continues servicing subsequent requests.
//! 2. Backend closure returns Err: cleanly forwarded through responder channel, AppState.llm event loop remains healthy.
//! 3. Rapid repeated shutdowns: 50 concurrent shutdown calls and post-shutdown calls fail cleanly without panics.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmBackendFn, Priority};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::{AppState, stt, tts};

fn create_test_state_with_llm(handle: LlmActorHandle) -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
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
        llm: handle,
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

/// 1. Client receivers dropped prematurely: actor does not crash, AppState.llm continues servicing subsequent requests.
#[tokio::test]
async fn test_appstate_prematurely_dropped_receivers_preserves_actor_health() {
    let (tx, rx) = mpsc::channel(32);
    let proc_counter = Arc::new(AtomicUsize::new(0));
    let proc_clone = proc_counter.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        proc_clone.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(5));
        Ok(format!("AppStateEcho: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    // Launch 30 tasks that drop their receivers prematurely
    let mut dropped_tasks = Vec::new();
    for i in 0..30 {
        let s = state.clone();
        dropped_tasks.push(tokio::spawn(async move {
            let fut = s
                .llm
                .generate_text(format!("dropped_{i}"), Priority::Normal);
            let _ = tokio::time::timeout(Duration::from_micros(50), fut).await;
        }));
    }

    for dt in dropped_tasks {
        let _ = dt.await;
    }

    // Wait for in-flight/queued dropped items to finish
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Verify AppState.llm is fully functional for all priority levels
    let res_high = state
        .llm
        .generate_text("voice_turn_after_drops", Priority::High)
        .await
        .expect("High priority voice request must succeed");
    assert_eq!(res_high, "AppStateEcho: voice_turn_after_drops");

    let res_norm = state
        .llm
        .generate_text("chat_turn_after_drops", Priority::Normal)
        .await
        .expect("Normal priority chat request must succeed");
    assert_eq!(res_norm, "AppStateEcho: chat_turn_after_drops");

    let res_low = state
        .llm
        .generate_text("agent_turn_after_drops", Priority::Low)
        .await
        .expect("Low priority agent request must succeed");
    assert_eq!(res_low, "AppStateEcho: agent_turn_after_drops");

    state.llm.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 2. Backend closure returns Err: cleanly forwarded through responder channel, AppState.llm event loop remains healthy.
#[tokio::test]
async fn test_appstate_backend_err_propagation_and_event_loop_survival() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmBackendFn = Box::new(|prompt| {
        if prompt.contains("trigger_oom") {
            Err(anyhow::anyhow!(
                "CudaOomError: 4096MB VRAM allocation failed"
            ))
        } else if prompt.contains("trigger_ctx_overflow") {
            Err(anyhow::anyhow!(
                "ContextOverflowError: n_tokens exceeded 4096"
            ))
        } else {
            Ok(format!("AppStateResp: {}", prompt))
        }
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    // Trigger OOM error on High priority (e.g. voice)
    let oom_err = state
        .llm
        .generate_text("voice_trigger_oom", Priority::High)
        .await
        .expect_err("Must return Err on OOM");
    assert!(oom_err.to_string().contains("CudaOomError"));

    // Verify AppState.llm immediately handles a valid Normal request
    let valid1 = state
        .llm
        .generate_text("normal_request_1", Priority::Normal)
        .await
        .expect("Normal request after OOM error must succeed");
    assert_eq!(valid1, "AppStateResp: normal_request_1");

    // Trigger Context Overflow on Low priority (e.g. agent graph)
    let ctx_err = state
        .llm
        .generate_text("agent_trigger_ctx_overflow", Priority::Low)
        .await
        .expect_err("Must return Err on Context Overflow");
    assert!(ctx_err.to_string().contains("ContextOverflowError"));

    // Verify AppState.llm continues servicing subsequent requests
    for i in 0..10 {
        let res = state
            .llm
            .generate_text(format!("subsequent_{i}"), Priority::Normal)
            .await
            .expect("Subsequent request must succeed");
        assert_eq!(res, format!("AppStateResp: subsequent_{i}"));
    }

    state.llm.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 3. Rapid repeated shutdowns: 50 concurrent shutdown calls and post-shutdown calls fail cleanly without panics.
#[tokio::test]
async fn test_appstate_rapid_repeated_shutdowns_and_post_shutdown_safety() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        std::thread::sleep(Duration::from_millis(5));
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    // Warm-up call
    let warm = state
        .llm
        .generate_text("warmup", Priority::Normal)
        .await
        .expect("warmup succeeds");
    assert_eq!(warm, "Done: warmup");

    // 50 concurrent tasks calling state.llm.shutdown() simultaneously
    let mut shutdown_futs = Vec::new();
    for _ in 0..50 {
        let s = state.clone();
        shutdown_futs.push(tokio::spawn(async move { s.llm.shutdown().await }));
    }

    let mut oks = 0;
    let mut _errs = 0;
    for sf in shutdown_futs {
        let res = sf.await.expect("shutdown task panicked");
        match res {
            Ok(()) => oks += 1,
            Err(_) => _errs += 1,
        }
    }
    assert!(oks >= 1, "At least one shutdown call must succeed");

    // Wait for actor termination
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor must terminate after shutdown")
        .expect("Actor panicked");

    // Repeated calls to shutdown() after actor termination
    for _ in 0..10 {
        let res = state.llm.shutdown().await;
        assert!(res.is_err(), "shutdown after actor exit must return Err");
    }

    // 50 concurrent calls to generate_text AFTER shutdown has taken effect
    let mut post_shutdown_futs = Vec::new();
    for i in 0..50 {
        let s = state.clone();
        post_shutdown_futs.push(tokio::spawn(async move {
            s.llm
                .generate_text(format!("post_{i}"), Priority::High)
                .await
        }));
    }

    for pf in post_shutdown_futs {
        let res = pf.await.expect("post-shutdown task panicked");
        assert!(res.is_err(), "generate_text after shutdown must return Err");
    }
}
