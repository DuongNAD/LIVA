//! Adversarial & Stress Verification Challenge Test Suite for Milestone M3
//! (Desktop UI Clean-up & IPC Synchronization).
//!
//! Verifies:
//! 1. Principle Authorization Matrix for `audio_play_started` and `audio_play_finished`:
//!    - TauriWidget / LocalCli / Test: ALLOWED (200 / Ok)
//!    - TauriDashboard / TauriSetup / Telegram: DENIED (fail-closed)
//! 2. In-process IPC Execution for TauriWidget:
//!    - Events execute cleanly as graceful no-ops without error.
//! 3. Fail-Closed Enforcement for Unauthorized Principals:
//!    - Unauthorized principals sending `audio_play_started` receive Err.
//! 4. High-Throughput Stress Burst:
//!    - Burst of 200 alternating audio lifecycle events handled without resource leakage or panic.
//! 5. Robustness to Malformed / Fuzzed Payloads:
//!    - Non-object, null, array, and extraneous payloads in audio events do not panic the dispatcher.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, db, handle_command_as, stt, tts,
};
use std::sync::Arc;

fn create_test_state() -> Arc<AppState> {
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

#[test]
fn test_audio_play_events_authorization_matrix() {
    let audio_events = ["audio_play_started", "audio_play_finished"];

    // 1. Authorized principals: TauriWidget, LocalCli, Test
    for cmd in &audio_events {
        assert!(
            authorize_command(CommandPrincipal::TauriWidget, cmd).is_ok(),
            "TauriWidget must be authorized for {cmd}"
        );
        assert!(
            authorize_command(CommandPrincipal::LocalCli, cmd).is_ok(),
            "LocalCli must be authorized for {cmd}"
        );
        assert!(
            authorize_command(CommandPrincipal::Test, cmd).is_ok(),
            "Test principal must be authorized for {cmd}"
        );
    }

    // 2. Unauthorized principals must be strictly fail-closed
    let unauthorized_principals = [
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::Telegram,
    ];

    for principal in unauthorized_principals {
        for cmd in &audio_events {
            let res = authorize_command(principal, cmd);
            assert!(
                res.is_err(),
                "{principal:?} must NOT be authorized for {cmd}, but got {res:?}"
            );
        }
    }
}

#[tokio::test]
async fn test_tauri_widget_audio_events_clean_execution_and_noop() {
    let state = create_test_state();

    // Send audio_play_started
    let res1 = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "audio_play_started",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res1.is_ok(), "audio_play_started should succeed");

    // Send audio_play_finished
    let res2 = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "audio_play_finished",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res2.is_ok(), "audio_play_finished should succeed");

    // Ping command to ensure channel is responsive
    let res3 = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res3.is_ok(), "ping should succeed");
}

#[tokio::test]
async fn test_unauthorized_audio_events_fail_closed_under_acl() {
    let state = create_test_state();

    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "audio_play_started",
        serde_json::json!({}),
        None,
        None,
    )
    .await;

    assert!(
        res.is_err(),
        "Unauthorized TauriDashboard principal must be rejected"
    );
}

#[tokio::test]
async fn test_tauri_widget_audio_events_stress_burst() {
    let state = create_test_state();

    // Send a burst of 200 alternating audio play events
    for i in 0..100 {
        let res_start = handle_command_as(
            CommandPrincipal::TauriWidget,
            state.clone(),
            "audio_play_started",
            serde_json::json!({ "seq": i }),
            None,
            None,
        )
        .await;
        assert!(res_start.is_ok());

        let res_finish = handle_command_as(
            CommandPrincipal::TauriWidget,
            state.clone(),
            "audio_play_finished",
            serde_json::json!({ "seq": i }),
            None,
            None,
        )
        .await;
        assert!(res_finish.is_ok());
    }

    // Verify system responsiveness after stress burst
    let res_ping = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res_ping.is_ok());
}

#[tokio::test]
async fn test_tauri_widget_audio_events_malformed_fuzz() {
    let state = create_test_state();

    // Various fuzzed payloads
    let fuzzed = vec![
        ("audio_play_started", serde_json::json!(null)),
        ("audio_play_started", serde_json::json!("unexpected_string")),
        ("audio_play_started", serde_json::json!([1, 2, 3, false])),
        ("audio_play_finished", serde_json::json!(-12345)),
        (
            "audio_play_finished",
            serde_json::json!({"deep": {"nested": [null]}}),
        ),
        ("audio_play_started", serde_json::Value::Null),
    ];

    for (cmd, payload) in fuzzed {
        let _ = handle_command_as(
            CommandPrincipal::TauriWidget,
            state.clone(),
            cmd,
            payload,
            None,
            None,
        )
        .await;
    }

    // Follow with ping to ensure stability
    let res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res.is_ok(), "Ping should succeed after fuzzing");
}
