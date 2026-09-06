//! Challenger 2 Empirical Stress and Boundary Verification Suite
//!
//! Tests:
//! 1. RUST-02: Browser Command isRunning Dynamic Status Validation
//! 2. RUST-03: Checkpoint Restoration & Single Row Retrieval
//!

use liva_native_core::agent::graph::checkpoint::{
    Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::{commands, AppState, db, llm, stt, tts};
use serde_json::json;
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
// RUST-02: Browser Command isRunning Status Validation
// =========================================================================

#[tokio::test]
async fn test_rust_02_browser_status_command_contract() {
    let state = test_app_state();

    // Explicitly stop the preview session to verify the closed driver contract (RUST-02)
    let res_stop = commands::browser::handle(state.clone(), "browser:control", json!({"action": "stop"})).await;
    assert!(res_stop.is_ok(), "Stop action should succeed");

    // 1. Closed State: isRunning must be false when closed
    let res = commands::browser::handle(state.clone(), "browser:status", json!({})).await;
    assert!(res.is_ok(), "browser:status should succeed");

    let val = res.unwrap();
    assert_eq!(val.get("isRunning"), Some(&json!(false)));
    assert_eq!(val.get("isPaused"), Some(&json!(false)));
    assert_eq!(val.get("sandboxActive"), Some(&json!(true)));
    assert_eq!(val.get("ssrfGuard"), Some(&json!(true)));

    // 2. Pause & Resume actions while closed
    let pause_res = commands::browser::handle(state.clone(), "browser:control", json!({"action": "pause"})).await;
    assert!(pause_res.is_ok());

    let status_paused = commands::browser::handle(state.clone(), "browser:status", json!({})).await.unwrap();
    assert_eq!(status_paused.get("isPaused"), Some(&json!(true)));
    assert_eq!(status_paused.get("isRunning"), Some(&json!(false)));

    let resume_res = commands::browser::handle(state.clone(), "browser:control", json!({"action": "resume"})).await;
    assert!(resume_res.is_ok());

    let status_resumed = commands::browser::handle(state.clone(), "browser:status", json!({})).await.unwrap();
    assert_eq!(status_resumed.get("isPaused"), Some(&json!(false)));
    assert_eq!(status_resumed.get("isRunning"), Some(&json!(false)));

    // Teardown: relaunch to restore pristine running preview state
    let _ = commands::browser::handle(state.clone(), "browser:control", json!({"action": "launch"})).await;
}

// =========================================================================
// RUST-03: Checkpointer Single Record Retrieval Validation
// =========================================================================

#[tokio::test]
async fn test_rust_03_checkpointer_record_retrieval() {
    let pool = Arc::new(DatabasePool::new_in_memory().unwrap());
    let crypto = EncryptionEngine::new("00000000000000000000000000000000");
    let checkpointer = SqliteCheckpointer::new(pool, crypto);

    let tid = "challenger2_thread_1";

    // 1. Non-existent thread should return error
    let restore_err = checkpointer.restore_time_travel(tid, 1).await;
    assert!(restore_err.is_err(), "Non-existent thread restore should fail gracefully");

    // 2. Save step 0 base checkpoint
    let mut state0 = AgentState::default();
    state0.execution_step = 0;
    state0.current_node = "init".to_string();
    state0.scratchpad_set("key1", json!("val1"));
    let save0 = checkpointer
        .save_checkpoint(tid, 0, &state0, "init", None, None, Some("ACTIVE"))
        .await;
    assert!(save0.is_ok());

    // 3. Restore time travel to step 0
    let restored0 = checkpointer.restore_time_travel(tid, 0).await.expect("Restore step 0");
    assert_eq!(restored0.execution_step, 0);
    assert_eq!(restored0.scratchpad_get("key1"), Some(&json!("val1")));
}
