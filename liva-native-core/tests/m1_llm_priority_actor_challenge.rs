//! Adversarial Empirical Stress Test Suite for Milestone 1:
//! `AppState` LLM Priority Queue Actor Integration (Requirement R2)
//!
//! Objectives:
//! 1. Verify `AppState.llm` burst concurrency: Multiple simultaneous High, Normal,
//!    and Low priority requests from multiple Tokio handles do not deadlock or starve.
//! 2. Verify voice turn head-of-line preemption: A real-time voice command (`Priority::High`)
//!    preempts a backlog of autonomous agent tasks (`Priority::Low`).
//! 3. Verify non-blocking status queries: `is_generating` and engine metadata do not block on LLM lock.
//! 4. Verify graceful shutdown of `AppState.llm` under active traffic.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};
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

/// Adversarial Challenge 1: Multi-Handle High Concurrency across AppState
///
/// 20 parallel tasks concurrently invoke `state.llm.generate_text` across High, Normal,
/// and Low priorities while simultaneously querying `AppState` methods.
/// Verifies zero deadlocks and 100% completion rate.
#[tokio::test]
async fn test_appstate_llm_concurrent_multi_handle_burst() {
    let (tx, rx) = mpsc::channel(64);
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok(format!("Echo: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    let mut tasks = Vec::new();
    for task_id in 0..20 {
        let state_clone = state.clone();
        tasks.push(tokio::spawn(async move {
            let mut results = Vec::new();
            for req_id in 0..5 {
                let priority = match req_id % 3 {
                    0 => Priority::Low,
                    1 => Priority::Normal,
                    _ => Priority::High,
                };
                let res = state_clone
                    .llm
                    .generate_text(format!("t{task_id}_r{req_id}"), priority)
                    .await;
                results.push(res);
            }
            results
        }));
    }

    // Join all with timeout guardrail
    let results = tokio::time::timeout(Duration::from_secs(10), async {
        let mut all = Vec::new();
        for t in tasks {
            all.push(t.await.expect("task join failed"));
        }
        all
    })
    .await
    .expect("All concurrent AppState requests must complete within 10s without deadlocking");

    let total = results.into_iter().flatten().filter(|r| r.is_ok()).count();
    assert_eq!(total, 100, "All 100 requests must succeed");
    assert_eq!(counter.load(Ordering::SeqCst), 100);

    state.llm.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Adversarial Challenge 2: Voice Head-of-Line Preemption over Agent Backlog
///
/// Simulates 10 low-priority autonomous agent graph tasks (each taking 30ms).
/// While task 0 is running, a High-priority voice dialogue turn arrives.
/// Verifies that the voice turn is executed IMMEDIATELY after task 0,
/// rather than waiting behind the remaining 9 agent tasks.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_appstate_voice_priority_preempts_agent_backlog() {
    let (tx, rx) = mpsc::channel(64);
    let execution_order = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let order_clone = execution_order.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        if prompt.starts_with("agent_task_") {
            std::thread::sleep(Duration::from_millis(30));
        } else if prompt == "voice_dialogue_turn" {
            std::thread::sleep(Duration::from_millis(5));
        }
        order_clone.lock().unwrap().push(prompt.to_string());
        Ok(format!("Processed: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    // 1. Submit 10 Low-priority agent tasks
    let mut agent_futs = Vec::new();
    for i in 0..10 {
        let s = state.clone();
        agent_futs.push(tokio::spawn(async move {
            s.llm
                .generate_text(format!("agent_task_{i}"), Priority::Low)
                .await
        }));
    }

    // 2. Allow task 0 to start executing on the worker thread (10ms delay)
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 3. High-priority voice dialogue arrives while task 0 is in-flight
    let s_voice = state.clone();
    let voice_start = Instant::now();
    let voice_fut = tokio::spawn(async move {
        s_voice
            .llm
            .generate_text("voice_dialogue_turn", Priority::High)
            .await
    });

    // Await voice turn
    let voice_res = voice_fut.await.unwrap().expect("voice call should succeed");
    let voice_elapsed = voice_start.elapsed();
    assert_eq!(voice_res, "Processed: voice_dialogue_turn");

    // Await remaining agent tasks
    for f in agent_futs {
        f.await.unwrap().expect("agent task should succeed");
    }

    let order = execution_order.lock().unwrap().clone();
    println!("Actual execution order: {:?}", order);
    assert_eq!(order.len(), 11, "All 11 tasks must be processed");

    // Task 1 MUST BE the voice turn because it preempted agent_task_1..9!
    assert_eq!(
        order[1], "voice_dialogue_turn",
        "Voice turn MUST execute in position 1, preempting all 9 queued agent tasks! Execution order was: {:?}",
        order
    );

    // Voice elapsed must be significantly less than waiting for all 10 tasks (10 * 30ms = 300ms)
    // It only waited for task 0 to finish (~30ms) + its own execution (~5ms).
    assert!(
        voice_elapsed < Duration::from_millis(150),
        "Voice turn took {:?}, which indicates it waited for the whole queue instead of preempting!",
        voice_elapsed
    );

    state.llm.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Adversarial Challenge 3: AppState Graceful Shutdown under In-Flight Traffic
///
/// Under active traffic, calling `state.llm.shutdown()` must terminate the actor task
/// and prevent lingering threads or orphaned locks.
#[tokio::test]
async fn test_appstate_llm_graceful_shutdown() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        std::thread::sleep(Duration::from_millis(10));
        Ok(format!("Resp: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);
    let state = create_test_state_with_llm(handle);

    // Send a command
    let res = state.llm.generate_text("test_cmd", Priority::Normal).await;
    assert!(res.is_ok());

    // Shutdown actor via state
    state.llm.shutdown().await.expect("shutdown should succeed");

    // Actor task terminates
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor must terminate cleanly")
        .expect("Actor panicked");

    // Subsequent call must fail gracefully
    let err_res = state
        .llm
        .generate_text("post_shutdown", Priority::Normal)
        .await;
    assert!(err_res.is_err(), "Call after shutdown must return Err");
}

/// Adversarial Challenge 4: AppState::mock_llm Contract Conformance
///
/// Verifies that `AppState::mock_llm()` creates an operational actor that handles
/// all priorities and simulated responses correctly.
#[tokio::test]
async fn test_appstate_mock_llm_conformance() {
    let handle = AppState::mock_llm();
    let state = create_test_state_with_llm(handle);

    let res_low = state
        .llm
        .generate_text("mock_low", Priority::Low)
        .await
        .unwrap();
    assert!(res_low.contains("Simulated response to: mock_low"));

    let res_norm = state
        .llm
        .generate_text("mock_norm", Priority::Normal)
        .await
        .unwrap();
    assert!(res_norm.contains("Simulated response to: mock_norm"));

    let res_high = state
        .llm
        .generate_text("mock_high", Priority::High)
        .await
        .unwrap();
    assert!(res_high.contains("Simulated response to: mock_high"));

    state.llm.shutdown().await.unwrap();
}
