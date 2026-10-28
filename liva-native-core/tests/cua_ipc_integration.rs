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

    let cua_config = liva_cua::types::CuaConfig {
        permission_mode: liva_cua::types::PermissionMode::Bounded,
        process_allowlist: vec!["notepad.exe".to_string(), "mock_app.exe".to_string()],
        ..liva_cua::types::CuaConfig::default()
    };

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

    let root_element = liva_cua::perception::UiElement::new(
        "root_window",
        liva_cua::perception::UiRole::Window,
        "Untitled - Notepad",
        liva_cua::CuaRect::new(0, 0, 1920, 1080),
    )
    .with_child(
        liva_cua::perception::UiElement::new(
            "btn_save",
            liva_cua::perception::UiRole::Button,
            "Save",
            liva_cua::CuaRect::new(100, 100, 80, 30),
        )
        .with_state(liva_cua::perception::UiState::clickable()),
    )
    .with_child(
        liva_cua::perception::UiElement::new(
            "edit_content",
            liva_cua::perception::UiRole::Edit,
            "Editor Area",
            liva_cua::CuaRect::new(100, 200, 600, 400),
        )
        .with_state(liva_cua::perception::UiState::editable()),
    );
    let mut test_tree = liva_cua::perception::UiTree::new(1001, root_element);
    liva_cua::perception::SoMIndexer::index_tree(&mut test_tree);
    mock_driver.insert_ui_tree(1001, test_tree);

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
        mcp_server: Arc::new(
            liva_native_core::mcp::server::NativeMcpServer::new_with_cua("test_vault", cua.clone()),
        ),
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

#[tokio::test]
async fn test_cua_inspect_ui_tree_command() {
    let state = test_state_with_cua();

    // 1. Inspect UI tree for target window 1001
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:inspect_ui_tree",
        json!({ "target_hwnd": 1001 }),
        None,
        None,
    )
    .await
    .expect("inspect_ui_tree succeeds");

    assert_eq!(res["interactive_count"], 2);
    assert_eq!(res["total_nodes"], 3);
    assert!(res["som_elements"].is_array());
    assert_eq!(res["som_elements"].as_array().unwrap().len(), 2);

    let prompt = res["som_prompt"].as_str().expect("som_prompt is string");
    assert!(prompt.contains("[1] Button \"Save\""));
    assert!(prompt.contains("[2] Edit \"Editor Area\""));

    // 2. Inspect with fallback_roi flag
    let fallback_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:inspect_ui_tree",
        json!({ "target_hwnd": 1001, "fallback_roi": true }),
        None,
        None,
    )
    .await
    .expect("inspect_ui_tree with fallback_roi succeeds");

    assert!(fallback_res["perception"].is_object());
}

#[tokio::test]
async fn test_cua_execute_som_action_command() {
    let state = test_state_with_cua();

    // Target SoM element [1] (Save button at x: 100, y: 100, w: 80, h: 30 -> center 140, 115)
    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 1,
            "action": "click",
            "button": "left"
        }),
        None,
        None,
    )
    .await
    .expect("execute_som_action succeeds");

    assert_eq!(res["som_index"], 1);
    assert_eq!(res["resolved_action"]["action"], "click");
    assert_eq!(res["resolved_action"]["x"], 140);
    assert_eq!(res["resolved_action"]["y"], 115);
    assert!(res["result"]["action_type"].is_string());

    // Target SoM element [2] (Editor Area) with text typing
    let type_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 2,
            "action": "type_text",
            "text": "Hello Set-of-Mark!"
        }),
        None,
        None,
    )
    .await
    .expect("execute_som_action type_text succeeds");

    assert_eq!(type_res["som_index"], 2);
    assert_eq!(type_res["resolved_action"]["action"], "type_text");
}

#[tokio::test]
async fn test_cua_run_profile_flash_and_pro_commands() {
    let state = test_state_with_cua();

    // 1. Run Flash profile plan
    let flash_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "flash",
            "plan_id": "test_flash_plan",
            "target_hwnd": 1001,
            "steps": [
                {
                    "action": "click",
                    "target_som_id": 1
                }
            ]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile flash succeeds");

    assert_eq!(flash_res["result"]["profile"], "flash");
    assert_eq!(flash_res["result"]["success"], true);
    assert_eq!(flash_res["result"]["completed_steps"], 1);

    // 2. Run Pro profile plan
    let pro_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:run_profile",
        json!({
            "profile": "pro",
            "plan_id": "test_pro_plan",
            "target_hwnd": 1001,
            "steps": [
                {
                    "action": "click",
                    "target_som_id": 1
                }
            ]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile pro succeeds");

    assert_eq!(pro_res["result"]["profile"], "pro");
    assert_eq!(pro_res["result"]["completed_steps"], 1);
}

#[tokio::test]
async fn test_cua_list_devices_command() {
    let state = test_state_with_cua();

    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:list_devices",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list_devices succeeds");

    assert!(res["windows"].is_array());
    assert!(res["android_devices"].is_array());
    assert!(res["count"]["windows"].as_u64().unwrap() >= 1);
}

#[tokio::test]
async fn test_cua_permission_mode_enforcement() {
    let state = test_state_with_cua();

    // 1. Set mode to Standard (Read-Only)
    handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "Standard" }),
        None,
        None,
    )
    .await
    .expect("set_mode Standard succeeds");

    // 2. Read-only inspect_ui_tree must succeed
    let inspect_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:inspect_ui_tree",
        json!({ "target_hwnd": 1001 }),
        None,
        None,
    )
    .await;
    assert!(
        inspect_res.is_ok(),
        "inspect_ui_tree must be allowed in Standard mode"
    );

    // 3. Mutating action via execute_som_action must be blocked
    let som_err = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 1,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(
        som_err.is_err(),
        "Mutating actions must be blocked in Standard mode"
    );

    // 4. Mutating action via execute_action must be blocked
    let act_err = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": "click",
            "target_hwnd": 1001,
            "x": 100,
            "y": 100
        }),
        None,
        None,
    )
    .await;
    assert!(
        act_err.is_err(),
        "execute_action must be blocked in Standard mode"
    );

    // 5. Restore to Bounded mode
    handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "Bounded", "allowlist": ["notepad.exe"] }),
        None,
        None,
    )
    .await
    .expect("set_mode Bounded succeeds");

    // 6. Action now succeeds
    let act_ok = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 1,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(
        act_ok.is_ok(),
        "Action should succeed in Bounded mode for allowlisted app"
    );
}

#[tokio::test]
async fn test_cua_mcp_native_tools_and_execution() {
    let state = test_state_with_cua();

    // 1. Verify CUA tools appear in list_tools
    let tool_list = state.mcp_server.list_tools();
    let tool_names: Vec<String> = tool_list.tools.iter().map(|t| t.name.clone()).collect();
    assert!(tool_names.contains(&"cua_inspect_ui".to_string()));
    assert!(tool_names.contains(&"cua_execute_som".to_string()));
    assert!(tool_names.contains(&"cua_run_profile".to_string()));
    assert!(tool_names.contains(&"cua_emergency_halt".to_string()));

    // 2. Call cua_inspect_ui via MCP
    let inspect_call = state
        .mcp_server
        .call_tool(liva_native_core::mcp::protocol::CallToolRequest {
            name: "cua_inspect_ui".to_string(),
            arguments: json!({ "target_hwnd": 1001 }),
        })
        .await
        .expect("cua_inspect_ui call succeeds");

    assert!(!inspect_call.is_error);
    let inspect_text = match &inspect_call.content[0] {
        liva_native_core::mcp::protocol::ToolContent::Text { text } => text,
        _ => panic!("Expected text content from cua_inspect_ui"),
    };
    assert!(inspect_text.contains("Save"));

    // 3. Call cua_execute_som via MCP
    let som_call = state
        .mcp_server
        .call_tool(liva_native_core::mcp::protocol::CallToolRequest {
            name: "cua_execute_som".to_string(),
            arguments: json!({
                "target_hwnd": 1001,
                "som_index": 1,
                "action": "click"
            }),
        })
        .await
        .expect("cua_execute_som call succeeds");

    assert!(!som_call.is_error);

    // 4. Call cua_run_profile via MCP
    let profile_call = state
        .mcp_server
        .call_tool(liva_native_core::mcp::protocol::CallToolRequest {
            name: "cua_run_profile".to_string(),
            arguments: json!({
                "profile": "flash",
                "target_hwnd": 1001,
                "steps": [
                    { "action": "click", "target_som_id": 1 }
                ]
            }),
        })
        .await
        .expect("cua_run_profile call succeeds");

    assert!(!profile_call.is_error);

    // 5. Call cua_emergency_halt via MCP
    let halt_call = state
        .mcp_server
        .call_tool(liva_native_core::mcp::protocol::CallToolRequest {
            name: "cua_emergency_halt".to_string(),
            arguments: json!({}),
        })
        .await
        .expect("cua_emergency_halt call succeeds");

    assert!(!halt_call.is_error);
    assert!(state.cua.kill_switch.is_halted());

    // Reset emergency halt via MCP
    let reset_call = state
        .mcp_server
        .call_tool(liva_native_core::mcp::protocol::CallToolRequest {
            name: "cua_emergency_halt".to_string(),
            arguments: json!({ "reset": true }),
        })
        .await
        .expect("cua_emergency_halt reset succeeds");

    assert!(!reset_call.is_error);
    assert!(!state.cua.kill_switch.is_halted());
}

#[tokio::test]
async fn test_cua_tool_calling_exec_policy_guard() {
    use liva_native_core::llm::tool_calling::{
        ExecPolicy, NATIVE_SERVER, ToolExecPolicy, guard_direct_call, tool_exec_policy,
    };

    // Auto policy tools
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_inspect_ui"),
        ToolExecPolicy::Auto
    );
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_emergency_halt"),
        ToolExecPolicy::Auto
    );
    assert!(guard_direct_call(NATIVE_SERVER, "cua_inspect_ui").is_ok());
    assert!(guard_direct_call(NATIVE_SERVER, "cua_emergency_halt").is_ok());

    // ProposeOnly policy tools (mutating actions require user confirmation fail-closed)
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_execute_som"),
        ToolExecPolicy::ProposeOnly
    );
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_run_profile"),
        ToolExecPolicy::ProposeOnly
    );
    assert!(guard_direct_call(NATIVE_SERVER, "cua_execute_som").is_err());
    assert!(guard_direct_call(NATIVE_SERVER, "cua_run_profile").is_err());

    // Test conversion
    let auto_policy: ExecPolicy = ToolExecPolicy::Auto.into();
    assert_eq!(auto_policy, ExecPolicy::Auto);
    let propose_policy: ToolExecPolicy = ExecPolicy::ProposeOnly.into();
    assert_eq!(propose_policy, ToolExecPolicy::ProposeOnly);
}

#[tokio::test]
async fn test_cua_untrusted_principals_fail_closed_for_all_new_verbs() {
    let state = test_state_with_cua();

    let new_commands = [
        "cua:inspect_ui_tree",
        "cua:execute_som_action",
        "cua:run_profile",
        "cua:list_devices",
    ];

    for cmd in new_commands {
        let setup_err = handle_command_as(
            CommandPrincipal::TauriSetup,
            state.clone(),
            cmd,
            json!({}),
            None,
            None,
        )
        .await;
        assert!(
            setup_err.is_err(),
            "Setup principal must be blocked from {cmd}"
        );

        let tg_err = handle_command_as(
            CommandPrincipal::Telegram,
            state.clone(),
            cmd,
            json!({}),
            None,
            None,
        )
        .await;
        assert!(
            tg_err.is_err(),
            "Telegram principal must be blocked from {cmd}"
        );
    }
}
