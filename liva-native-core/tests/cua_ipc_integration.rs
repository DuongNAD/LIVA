//! Integration tests for CUA Tauri IPC command execution and authorization barriers.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{AppState, CommandPrincipal, db, handle_command_as, stt, tts};
use serde_json::json;
use std::sync::Arc;

fn test_state_with_cua() -> Arc<AppState> {
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

#[tokio::test]
async fn test_cua_get_status_command() {
    let state = test_state_with_cua();
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:get_status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("get_status succeeds");

    assert_eq!(res["halted"], false);
    assert_eq!(res["active_actions"], 0);
    assert!(res["mode"].is_string());
}

#[tokio::test]
async fn test_cua_set_mode_and_allowlist() {
    let state = test_state_with_cua();

    // 1. Change to Unrestricted
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "Unrestricted" }),
        None,
        None,
    )
    .await
    .expect("set_mode succeeds");

    assert_eq!(res["updated"], true);
    assert_eq!(
        state.cua.security_governor.get_permission_mode(),
        liva_cua::types::PermissionMode::Unrestricted
    );

    // 2. Change to Bounded with allowlist
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({
            "mode": "Bounded",
            "allowlist": ["calc.exe", "mspaint.exe"]
        }),
        None,
        None,
    )
    .await
    .expect("set_mode with allowlist succeeds");

    assert_eq!(res["updated"], true);
    assert_eq!(
        state.cua.security_governor.get_permission_mode(),
        liva_cua::types::PermissionMode::Bounded
    );
    assert_eq!(
        state.cua.security_governor.get_allowlist(),
        vec!["calc.exe".to_string(), "mspaint.exe".to_string()]
    );

    // 3. Invalid mode error
    let err = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:set_mode",
        json!({ "mode": "InvalidModeName" }),
        None,
        None,
    )
    .await;

    assert!(err.is_err());
}

#[tokio::test]
async fn test_cua_list_windows() {
    let state = test_state_with_cua();
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:list_windows",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list_windows succeeds");

    assert!(res["windows"].is_array());
}

#[tokio::test]
async fn test_cua_execute_action_and_emergency_halt() {
    let state = test_state_with_cua();

    // 1. Execute an action
    let action_payload = json!({
        "action": {
            "action": "click",
            "target_hwnd": 1001,
            "x": 100,
            "y": 100,
            "button": "left",
            "click_count": 1,
            "delivery_mode": "background"
        }
    });

    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        action_payload.clone(),
        None,
        None,
    )
    .await
    .expect("execute_action succeeds");

    assert!(res["result"]["action_type"].is_string());

    // 2. Trigger emergency stop
    let halt_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({}),
        None,
        None,
    )
    .await
    .expect("emergency_stop succeeds");

    assert_eq!(halt_res["halted"], true);

    // 3. Subsequent action must be rejected while halted
    let post_halt_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        action_payload,
        None,
        None,
    )
    .await;

    assert!(post_halt_res.is_err(), "Action must be blocked when halted");

    // 4. Reset emergency halt
    let reset_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({ "reset": true }),
        None,
        None,
    )
    .await
    .expect("reset emergency_stop succeeds");

    assert_eq!(reset_res["halted"], false);
    assert_eq!(reset_res["reset"], true);
}

#[tokio::test]
async fn test_cua_query_audit_logs() {
    let state = test_state_with_cua();

    // Query logs
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:query_audit_logs",
        json!({ "limit": 20 }),
        None,
        None,
    )
    .await
    .expect("query_audit_logs succeeds");

    assert!(res["events"].is_array());
    assert_eq!(res["limit"], 20);
    assert!(res["source"].is_string());
}

#[tokio::test]
async fn test_cua_principal_authorization_barriers() {
    let state = test_state_with_cua();

    // Widget cannot query audit logs
    let widget_err = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:query_audit_logs",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        widget_err.is_err(),
        "Widget must be blocked from query_audit_logs"
    );

    // Setup cannot execute any CUA commands
    let setup_err = handle_command_as(
        CommandPrincipal::TauriSetup,
        state.clone(),
        "cua:get_status",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        setup_err.is_err(),
        "Setup must be blocked from cua commands"
    );

    // Telegram cannot execute actions
    let tg_err = handle_command_as(
        CommandPrincipal::Telegram,
        state,
        "cua:execute_action",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        tg_err.is_err(),
        "Telegram must be blocked from CUA execution"
    );
}
