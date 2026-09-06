//! Challenger 1 Adversarial & Empirical Test Suite
//! Covers:
//! - RUST-02: commands::browser status `isRunning` dynamic boolean verification (elimination of `is_open || true` tautology)
//! - RUST-03: agent::graph::checkpoint `restore_time_travel` boundary, non-existent, intermediate step replay

use liva_native_core::agent::graph::checkpoint::{
    generate_json_patch, Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::{commands, AppState, db, llm, stt, tts};
use serde_json::{json, Value};
use std::sync::Arc;

fn test_app_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
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
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

// =========================================================================
// RUST-02: Browser Command Status Tautology Elimination
// =========================================================================

#[tokio::test]
async fn test_rust_02_browser_status_dynamic_is_running_boolean() {
    let state = test_app_state();

    // 1. Stopped / Closed State: driver is_open == false -> isRunning MUST BE false (NOT true!)
    let res_stop = commands::browser::handle(state.clone(), "browser:control", json!({"action": "stop"})).await;
    assert!(res_stop.is_ok(), "Stop action should succeed");

    let status1 = commands::browser::handle(state.clone(), "browser:status", json!({})).await
        .expect("browser:status should succeed");
    
    // Crucial check: with the previous bug (`is_open || true`), this would return `true` even when closed.
    // Now with the fix (`"isRunning": is_open`), this returns `false`.
    let is_running1 = status1.get("isRunning").and_then(Value::as_bool).expect("isRunning must be a boolean");
    assert!(!is_running1, "isRunning MUST be false when browser driver is closed. If true, tautology regression has occurred!");

    // 2. Verify all other status fields are properly shaped
    assert_eq!(status1.get("isPaused"), Some(&json!(false)));
    assert_eq!(status1.get("sandboxActive"), Some(&json!(true)));
    assert_eq!(status1.get("ssrfGuard"), Some(&json!(true)));
    assert_eq!(status1.get("viewportWidth"), Some(&json!(1280)));
    assert_eq!(status1.get("viewportHeight"), Some(&json!(800)));

    // 3. Pause / Resume cycle
    let res_pause = commands::browser::handle(state.clone(), "browser:control", json!({"action": "pause"})).await
        .expect("pause should succeed");
    assert_eq!(res_pause.get("state"), Some(&json!("paused")));

    let status_paused = commands::browser::handle(state.clone(), "browser:status", json!({})).await
        .expect("browser:status should succeed");
    assert_eq!(status_paused.get("isPaused"), Some(&json!(true)));
    assert_eq!(status_paused.get("isRunning"), Some(&json!(false)));

    let res_resume = commands::browser::handle(state.clone(), "browser:control", json!({"action": "resume"})).await
        .expect("resume should succeed");
    assert_eq!(res_resume.get("state"), Some(&json!("running")));

    let status_resumed = commands::browser::handle(state.clone(), "browser:status", json!({})).await
        .expect("browser:status should succeed");
    assert_eq!(status_resumed.get("isPaused"), Some(&json!(false)));
    assert_eq!(status_resumed.get("isRunning"), Some(&json!(false)));
}

// =========================================================================
// RUST-03: State Graph Checkpoint & Time Travel Replay
// =========================================================================

#[tokio::test]
async fn test_rust_03_restore_time_travel_boundary_and_diff_reconstruction() {
    let pool = Arc::new(DatabasePool::new_in_memory().expect("in-memory db"));
    let crypto = EncryptionEngine::new("test-key-32-bytes-long-for-test");
    let checkpointer = SqliteCheckpointer::new(pool, crypto);

    let tid = "thread_challenger_time_travel";

    // A. Error case: Restoring on a non-existent thread returns Err
    let err_res = checkpointer.restore_time_travel("thread_does_not_exist", 0).await;
    assert!(err_res.is_err(), "Restoring non-existent thread must fail");
    let err_msg = err_res.unwrap_err();
    assert!(err_msg.contains("No base checkpoint found"), "Error must mention missing base checkpoint: {}", err_msg);

    // B. Build a multi-step checkpoint history with diffs
    // Step 0 (Base Snapshot)
    let mut state0 = AgentState::default();
    state0.execution_step = 0;
    state0.current_node = "init".to_string();
    state0.scratchpad_set("score", json!(100));
    state0.messages.push(json!({"role": "user", "content": "Step 0 Msg"}));
    checkpointer.save_checkpoint(tid, 0, &state0, "init", None, None, Some("ACTIVE")).await
        .expect("Save step 0 base");

    // Step 1 (Diff from Step 0)
    let mut state1 = state0.clone();
    state1.execution_step = 1;
    state1.current_node = "process".to_string();
    state1.scratchpad_set("score", json!(200));
    state1.messages.push(json!({"role": "assistant", "content": "Step 1 Msg"}));
    let patch1 = generate_json_patch(&serde_json::to_value(&state0).unwrap(), &serde_json::to_value(&state1).unwrap());
    let patch1_str = serde_json::to_string(&patch1).unwrap();
    checkpointer.save_checkpoint(tid, 1, &state1, "process", Some(&patch1_str), None, Some("ACTIVE")).await
        .expect("Save step 1 diff");

    // Step 2 (Diff from Step 1)
    let mut state2 = state1.clone();
    state2.execution_step = 2;
    state2.current_node = "evaluate".to_string();
    state2.scratchpad_set("score", json!(300));
    state2.scratchpad_set("flag", json!(true));
    state2.messages.push(json!({"role": "user", "content": "Step 2 Msg"}));
    let patch2 = generate_json_patch(&serde_json::to_value(&state1).unwrap(), &serde_json::to_value(&state2).unwrap());
    let patch2_str = serde_json::to_string(&patch2).unwrap();
    checkpointer.save_checkpoint(tid, 2, &state2, "evaluate", Some(&patch2_str), None, Some("ACTIVE")).await
        .expect("Save step 2 diff");

    // Step 3 (Another Base Snapshot at step 3)
    let mut state3 = state2.clone();
    state3.execution_step = 3;
    state3.current_node = "final".to_string();
    state3.scratchpad_set("score", json!(400));
    state3.messages.push(json!({"role": "assistant", "content": "Step 3 Final"}));
    checkpointer.save_checkpoint(tid, 3, &state3, "final", None, None, Some("COMPLETED")).await
        .expect("Save step 3 base");

    // Step 4 (Diff from Step 3)
    let mut state4 = state3.clone();
    state4.execution_step = 4;
    state4.current_node = "post_finish".to_string();
    state4.scratchpad_set("score", json!(500));
    let patch4 = generate_json_patch(&serde_json::to_value(&state3).unwrap(), &serde_json::to_value(&state4).unwrap());
    let patch4_str = serde_json::to_string(&patch4).unwrap();
    checkpointer.save_checkpoint(tid, 4, &state4, "post_finish", Some(&patch4_str), None, Some("COMPLETED")).await
        .expect("Save step 4 diff");

    // C. Verify Boundary Replays:
    // 1. Time travel to Step 0 (Boundary: step 0 base)
    let restored0 = checkpointer.restore_time_travel(tid, 0).await.expect("Restore step 0");
    assert_eq!(restored0.execution_step, 0);
    assert_eq!(restored0.current_node, "init");
    assert_eq!(restored0.scratchpad_get("score"), Some(&json!(100)));
    assert_eq!(restored0.messages.len(), 1);

    // 2. Time travel to Step 1 (Intermediate diff applied over step 0)
    let restored1 = checkpointer.restore_time_travel(tid, 1).await.expect("Restore step 1");
    assert_eq!(restored1.execution_step, 1);
    assert_eq!(restored1.current_node, "process");
    assert_eq!(restored1.scratchpad_get("score"), Some(&json!(200)));
    assert_eq!(restored1.messages.len(), 2);

    // 3. Time travel to Step 2 (Second intermediate diff applied)
    let restored2 = checkpointer.restore_time_travel(tid, 2).await.expect("Restore step 2");
    assert_eq!(restored2.execution_step, 2);
    assert_eq!(restored2.current_node, "evaluate");
    assert_eq!(restored2.scratchpad_get("score"), Some(&json!(300)));
    assert_eq!(restored2.scratchpad_get("flag"), Some(&json!(true)));
    assert_eq!(restored2.messages.len(), 3);

    // 4. Time travel to Step 3 (Second base snapshot)
    let restored3 = checkpointer.restore_time_travel(tid, 3).await.expect("Restore step 3");
    assert_eq!(restored3.execution_step, 3);
    assert_eq!(restored3.current_node, "final");
    assert_eq!(restored3.scratchpad_get("score"), Some(&json!(400)));

    // 5. Time travel to Step 4 (Diff over second base snapshot)
    let restored4 = checkpointer.restore_time_travel(tid, 4).await.expect("Restore step 4");
    assert_eq!(restored4.execution_step, 4);
    assert_eq!(restored4.current_node, "post_finish");
    assert_eq!(restored4.scratchpad_get("score"), Some(&json!(500)));

    // 6. Time travel to step beyond highest (e.g. 99) -> uses highest base (step 3) and diffs up to 4
    let restored99 = checkpointer.restore_time_travel(tid, 99).await.expect("Restore step 99");
    assert_eq!(restored99.execution_step, 4);
    assert_eq!(restored99.scratchpad_get("score"), Some(&json!(500)));
}
