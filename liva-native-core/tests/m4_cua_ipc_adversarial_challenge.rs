//! Adversarial Challenge Test Suite for Milestone M4 (Tauri IPC & Authorization).
//!
//! Empirical stress tests:
//! 1. Authorization Boundary Stress:
//!    - Unauthorized principals (`TauriSetup`, `Telegram`, forged/unknown principals) attempting CUA operations -> 100% fail-closed rejection.
//!    - `TauriWidget` attempting to query audit logs (`cua:query_audit_logs`) -> strict rejection.
//!    - `TauriWidget` allowed to invoke interactive CUA operations (`cua:execute_action`, `cua:emergency_stop`, `cua:get_status`, `cua:list_windows`, `cua:set_mode`).
//!    - Unknown CUA verbs -> fail-closed rejection.
//! 2. IPC Payload Robustness & Fuzzing:
//!    - Submitting malformed, truncated, null, array, and hostile payloads to all 6 CUA verbs.
//!    - Verifying structured JSON error responses with standard error codes (`invalid_parameter`, `window_not_found`, `forbidden`) and zero panics.
//! 3. Emergency Stop SLA & Action Interruption:
//!    - Verify latency < 50ms SLA across cold and burst invocations.
//!    - Verify immediate rejection of actions while halted, and clean resumption after reset.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, db, handle_command_as, stt, tts,
};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;

fn setup_adversarial_test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    let mut cua_config = liva_cua::types::CuaConfig::default();
    cua_config.permission_mode = liva_cua::types::PermissionMode::Bounded;
    cua_config.process_allowlist = vec!["notepad.exe".to_string(), "mock_app.exe".to_string()];

    let mock_driver = Arc::new(liva_cua::mock::MockCuaDriver::new());
    mock_driver.insert_window(liva_cua::CuaWindowInfo {
        hwnd: 1001,
        pid: 1234,
        title: "Untitled - Notepad".to_string(),
        class_name: "Notepad".to_string(),
        process_name: Some("notepad.exe".to_string()),
        bounds: liva_cua::CuaRect::new(0, 0, 1920, 1080),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    let cua_engine = liva_cua::CuaEngine::new_with_driver(cua_config, mock_driver);
    cua_engine.kill_switch.stop_poller();
    let cua = Arc::new(cua_engine);

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
        embedder: AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// 1. Authorization Boundary Stress Tests
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn challenge_setup_and_telegram_100_percent_fail_closed_on_cua() {
    let all_cua_verbs = [
        "cua:list_windows",
        "cua:execute_action",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:get_status",
        "cua:query_audit_logs",
    ];

    for verb in all_cua_verbs {
        assert!(
            authorize_command(CommandPrincipal::TauriSetup, verb).is_err(),
            "SECURITY VULNERABILITY: Setup was permitted to execute CUA command: {verb}"
        );
        assert!(
            authorize_command(CommandPrincipal::Telegram, verb).is_err(),
            "SECURITY VULNERABILITY: Telegram was permitted to execute CUA command: {verb}"
        );
    }
}

#[tokio::test]
async fn challenge_unauthorized_principals_execution_rejection() {
    let state = setup_adversarial_test_state();
    let all_cua_verbs = [
        "cua:list_windows",
        "cua:execute_action",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:get_status",
        "cua:query_audit_logs",
    ];

    for verb in all_cua_verbs {
        // Attempt execution as Setup
        let setup_res = handle_command_as(
            CommandPrincipal::TauriSetup,
            state.clone(),
            verb,
            json!({}),
            None,
            None,
        )
        .await;
        assert!(setup_res.is_err(), "TauriSetup must be denied for {verb}");
        let err = setup_res.unwrap_err();
        assert!(
            err.contains("not authorized"),
            "Error must state unauthorized: {err}"
        );

        // Attempt execution as Telegram
        let tg_res = handle_command_as(
            CommandPrincipal::Telegram,
            state.clone(),
            verb,
            json!({}),
            None,
            None,
        )
        .await;
        assert!(tg_res.is_err(), "Telegram must be denied for {verb}");
        let err = tg_res.unwrap_err();
        assert!(
            err.contains("not authorized"),
            "Error must state unauthorized: {err}"
        );
    }
}

#[tokio::test]
async fn challenge_tauri_widget_audit_log_barrier_vs_interactive_access() {
    let state = setup_adversarial_test_state();

    // 1. TauriWidget attempting to query audit logs must be STRICTLY REJECTED
    let audit_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:query_audit_logs",
        json!({ "limit": 10 }),
        None,
        None,
    )
    .await;
    assert!(
        audit_res.is_err(),
        "SECURITY VULNERABILITY: TauriWidget was permitted to query audit logs!"
    );
    let err = audit_res.unwrap_err();
    assert!(err.contains("TauriWidget") && err.contains("cua:query_audit_logs"));

    // 2. TauriWidget MUST be allowed to invoke interactive CUA operations
    let status_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:get_status",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        status_res.is_ok(),
        "TauriWidget must be allowed cua:get_status"
    );

    let list_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:list_windows",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        list_res.is_ok(),
        "TauriWidget must be allowed cua:list_windows"
    );

    let stop_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        stop_res.is_ok(),
        "TauriWidget must be allowed cua:emergency_stop"
    );

    // Reset halt
    let _ = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({ "reset": true }),
        None,
        None,
    )
    .await;
}

#[test]
fn challenge_forged_principals_and_unknown_commands_fail_closed() {
    // 1. Serde deserialization of forged / unknown principals must fail
    let forged_principals = [
        "\"Admin\"",
        "\"Root\"",
        "\"System\"",
        "\"SuperUser\"",
        "\"\"",
        "123",
    ];
    for raw in forged_principals {
        let deser = serde_json::from_str::<CommandPrincipal>(raw);
        assert!(
            deser.is_err(),
            "Forged principal '{raw}' was deserialized successfully!"
        );
    }

    // 2. Unknown or forged CUA commands must fail authorization across all untrusted principals
    let forged_cua_verbs = [
        "cua:arbitrary_rce",
        "cua:eval",
        "cua:rm_rf",
        "cua:delete_system32",
        "cua:",
        "cua:execute_action_privileged",
    ];

    for principal in [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::Telegram,
    ] {
        for verb in forged_cua_verbs {
            assert!(
                authorize_command(principal, verb).is_err(),
                "Principal {principal:?} was authorized for forged verb {verb}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. IPC Payload Robustness & Fuzzing Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_malformed_and_hostile_json_payloads_do_not_panic() {
    let state = setup_adversarial_test_state();

    let hostile_payloads = vec![
        Value::Null,
        json!(""),
        json!("string_payload_instead_of_object"),
        json!(12345),
        json!(-999999),
        json!(true),
        json!(false),
        json!([]),
        json!([1, "two", null, {"nested": true}]),
        json!({}),
        json!({ "unexpected_garbage_key": "some_data" }),
        json!({ "nested": { "deep": { "depth": [1, 2, 3] } } }),
    ];

    let test_verbs = [
        "cua:get_status",
        "cua:list_windows",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:execute_action",
        "cua:query_audit_logs",
    ];

    for verb in test_verbs {
        for payload in &hostile_payloads {
            // Must NEVER panic regardless of payload malformation
            let res = handle_command_as(
                CommandPrincipal::TauriDashboard,
                state.clone(),
                verb,
                payload.clone(),
                None,
                None,
            )
            .await;

            // If an error is returned, verify it is either structured JSON or clean descriptive string
            if let Err(err_str) = res {
                assert!(!err_str.is_empty(), "Error message should not be empty");
                // If it claims to be JSON error, parse it to verify structure
                if err_str.starts_with('{') {
                    let parsed: Result<Value, _> = serde_json::from_str(&err_str);
                    assert!(
                        parsed.is_ok(),
                        "Structured error must be valid JSON: {err_str}"
                    );
                    let val = parsed.unwrap();
                    assert!(
                        val.get("error").is_some(),
                        "Structured error must contain 'error' object: {err_str}"
                    );
                }
            }
        }
    }
}

#[tokio::test]
async fn challenge_execute_action_payload_validation_and_structured_errors() {
    let state = setup_adversarial_test_state();

    // 1. Missing action field
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");
    assert_eq!(err["error"]["subsystem"], "cua");

    // 2. Unknown action verb
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({ "action": "nonexistent_action_type", "target_hwnd": 1001 }),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 3. Missing target_hwnd
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({ "action": "click", "x": 10, "y": 10 }),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 4. Invalid data types (negative target_hwnd)
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({ "action": "click", "target_hwnd": -100 }),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 5. Non-existent window target (HWND 999999) -> structured "window_not_found" error
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 999999,
                "x": 50,
                "y": 50,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(
        err["error"]["code"], "window_not_found",
        "Expected window_not_found for unmapped HWND: {err:?}"
    );
    assert_eq!(err["error"]["subsystem"], "cua");
}

#[tokio::test]
async fn challenge_set_mode_payload_validation() {
    let state = setup_adversarial_test_state();

    // 1. Missing mode field
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 2. Invalid mode name
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "SuperAdministratorMode" }),
        None,
        None,
    )
    .await;
    assert!(res.is_err());
    let err: Value = serde_json::from_str(&res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 3. Hostile allowlist contents (mixed types) -> filters cleanly
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({
            "mode": "Bounded",
            "allowlist": ["valid_app.exe", 12345, null, true, "second_app.exe"]
        }),
        None,
        None,
    )
    .await;
    assert!(
        res.is_ok(),
        "Mixed allowlist types should be filtered gracefully"
    );
    let allowlist = state.cua.security_governor.get_allowlist();
    assert_eq!(
        allowlist,
        vec!["valid_app.exe".to_string(), "second_app.exe".to_string()]
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Emergency Stop SLA & Action Interruption Stress Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn challenge_emergency_stop_latency_sla_under_50ms() {
    let state = setup_adversarial_test_state();

    // 1. Cold invocation latency check
    let start = Instant::now();
    let res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({}),
        None,
        None,
    )
    .await
    .expect("emergency stop must succeed");
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 50,
        "Cold emergency stop exceeded 50ms SLA: {:?}",
        elapsed
    );
    assert_eq!(res["halted"], true);
    assert!(res["latency_ms"].as_u64().unwrap_or(999) < 50);

    // 2. Burst latency check (100 rapid cycles)
    for _ in 0..100 {
        let cycle_start = Instant::now();
        let halt_res = handle_command_as(
            CommandPrincipal::TauriWidget,
            state.clone(),
            "cua:emergency_stop",
            json!({}),
            None,
            None,
        )
        .await
        .expect("burst emergency stop");
        let cycle_elapsed = cycle_start.elapsed();

        assert!(
            cycle_elapsed.as_millis() < 50,
            "Burst emergency stop exceeded 50ms SLA: {:?}",
            cycle_elapsed
        );
        assert_eq!(halt_res["halted"], true);
    }

    // 3. Verify actions are immediately refused while halted
    let action_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1001,
                "x": 50,
                "y": 50,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(action_res.is_err(), "Action must be refused while halted");
    let err: Value =
        serde_json::from_str(&action_res.unwrap_err()).expect("must be valid JSON error");
    assert_eq!(err["error"]["code"], "emergency_halt");

    // 4. Reset emergency stop and verify actions can resume
    let reset_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({ "reset": true }),
        None,
        None,
    )
    .await
    .expect("reset emergency stop");
    assert_eq!(reset_res["halted"], false);
    assert_eq!(reset_res["reset"], true);

    let resumed_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1001,
                "x": 50,
                "y": 50,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        resumed_res.is_ok(),
        "Action must succeed after emergency stop reset"
    );
}
