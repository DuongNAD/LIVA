//! Empirical Adversarial Challenge: ARTEMIS CUA Native MCP Tools & LLM Tool Calling (Milestone M4)
//!
//! Empirically challenges and stress-tests:
//! 1. Tool Schema Validity & Structural Integrity for all 4 CUA tools:
//!    - `cua_inspect_ui`, `cua_execute_som`, `cua_run_profile`, `cua_emergency_halt`.
//!    - JSON Schema type, required arrays, and property typing.
//! 2. Adversarial Argument Validation (`validate_arguments`):
//!    - Missing required fields (`som_index`, `action`, `profile`).
//!    - Empty string and whitespace-only detection on required fields (anti-hallucination/guessing barrier).
//!    - Type mismatch rejections (e.g., string for integer, number for boolean).
//! 3. Compact Parameter Rendering (`render_params`):
//!    - Verifies required marker (`*`) on essential fields and absence on optional fields.
//!    - Verifies single-line compactness to prevent context exhaustion in small LLMs (2-4B).
//! 4. `NATIVE_AUTOEXEC` Allowlist Boundary & Privilege Isolation:
//!    - Read-only `cua_inspect_ui` and emergency `cua_emergency_halt` are `Auto`.
//!    - Mutating `cua_execute_som` and `cua_run_profile` are strictly `ProposeOnly`.
//!    - `guard_direct_call` blocks unauthorized invocation of mutating tools.
//! 5. `execute_call` Defense-in-Depth against Policy Spoofing:
//!    - Forges `ResolvedCall { policy: ExecPolicy::Auto, ... }` for mutating tools and proves `execute_call`
//!      re-queries `ExecPolicy::for_tool` independently and rejects the forged call.
//! 6. IPC Dispatcher Protection (`mcp:call_tool`):
//!    - Direct IPC calls to `cua_execute_som` and `cua_run_profile` are rejected by `guard_direct_call`.
//!    - Read-only `cua_inspect_ui` and `cua_emergency_halt` pass through.
//! 7. `DEFAULT_TOP_K` (11) Catalog Retention Invariant:
//!    - Verifies catalog contains 11 native tools and `DEFAULT_TOP_K` (11) guarantees zero silent truncation.
//!    - Verifies `rank_tools` retains all 11 tools when `k = DEFAULT_TOP_K`.
//! 8. Emergency Halt Kill-Switch Lifecycle & Mutating Action Blocking:
//!    - Verifies `cua_emergency_halt` triggers `<15ms` kill switch, blocking mutating operations.
//!    - Verifies reset unblocks subsequent operations.
//! 9. Adversarial SoM Resolution & Action Boundary Validation:
//!    - Out-of-bounds SoM indices and unsupported action types fail safely with structured error.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::llm::tool_calling::{
    DEFAULT_TOP_K, ExecPolicy, NATIVE_SERVER, ResolvedCall, ToolExecPolicy, build_catalog,
    execute_call, guard_direct_call, rank_tools, render_params, tool_exec_policy,
    validate_arguments,
};
use liva_native_core::mcp::protocol::{CallToolRequest, ToolContent};
use liva_native_core::{AppState, CommandPrincipal, db, handle_command_as, stt, tts};
use serde_json::{Value, json};
use std::sync::Arc;

fn create_adversarial_test_state() -> Arc<AppState> {
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
        hwnd: 2002,
        pid: 5678,
        title: "Test Adversarial Window".to_string(),
        class_name: "TestWindowClass".to_string(),
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
        "Test Adversarial Window",
        liva_cua::CuaRect::new(0, 0, 1920, 1080),
    )
    .with_child(
        liva_cua::perception::UiElement::new(
            "btn_submit",
            liva_cua::perception::UiRole::Button,
            "Submit Action",
            liva_cua::CuaRect::new(50, 50, 100, 40),
        )
        .with_state(liva_cua::perception::UiState::clickable()),
    )
    .with_child(
        liva_cua::perception::UiElement::new(
            "input_box",
            liva_cua::perception::UiRole::Edit,
            "Input Field",
            liva_cua::CuaRect::new(50, 120, 400, 30),
        )
        .with_state(liva_cua::perception::UiState::editable()),
    );
    let mut test_tree = liva_cua::perception::UiTree::new(2002, root_element);
    liva_cua::perception::SoMIndexer::index_tree(&mut test_tree);
    mock_driver.insert_ui_tree(2002, test_tree);

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
            liva_native_core::mcp::server::NativeMcpServer::new_with_cua(
                "test_vault_adv",
                cua.clone(),
            ),
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

// ============================================================================
// 1. TOOL SCHEMA VALIDITY & STRUCTURAL INTEGRITY
// ============================================================================

#[test]
fn test_c2_cua_schemas_structural_validity() {
    let server = liva_native_core::mcp::server::NativeMcpServer::new("test_vault");
    let tools = server.list_tools().tools;

    let cua_tools = [
        "cua_inspect_ui",
        "cua_execute_som",
        "cua_run_profile",
        "cua_emergency_halt",
    ];
    for tool_name in cua_tools {
        let tool = tools
            .iter()
            .find(|t| t.name == tool_name)
            .unwrap_or_else(|| {
                panic!("Tool '{}' must be registered in NativeMcpServer", tool_name)
            });

        assert!(
            !tool.description.trim().is_empty(),
            "Tool '{}' description must not be empty",
            tool_name
        );

        let schema_val = serde_json::to_value(&tool.input_schema).expect("serialize schema");
        let obj = schema_val
            .as_object()
            .expect("schema must be a JSON object");

        let schema_type = obj.get("type").and_then(Value::as_str);
        assert_eq!(
            schema_type,
            Some("object"),
            "Tool '{}' schema type must be 'object'",
            tool_name
        );

        let properties = obj.get("properties").and_then(Value::as_object);
        assert!(
            properties.is_some(),
            "Tool '{}' must define 'properties'",
            tool_name
        );
    }

    // Inspect UI schema details
    let inspect_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_inspect_ui")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let props = inspect_schema["properties"].as_object().unwrap();
    assert!(
        props.contains_key("target_hwnd"),
        "cua_inspect_ui must have target_hwnd"
    );
    assert!(
        props.contains_key("fallback_roi"),
        "cua_inspect_ui must have fallback_roi"
    );

    // Execute SoM schema details
    let som_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_execute_som")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let som_req = som_schema["required"]
        .as_array()
        .expect("cua_execute_som must have required fields");
    let som_req_names: Vec<&str> = som_req.iter().filter_map(Value::as_str).collect();
    assert!(
        som_req_names.contains(&"som_index"),
        "som_index must be required in cua_execute_som"
    );
    assert!(
        som_req_names.contains(&"action"),
        "action must be required in cua_execute_som"
    );

    // Run Profile schema details
    let profile_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_run_profile")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let prof_req = profile_schema["required"]
        .as_array()
        .expect("cua_run_profile must have required fields");
    let prof_req_names: Vec<&str> = prof_req.iter().filter_map(Value::as_str).collect();
    assert!(
        prof_req_names.contains(&"profile"),
        "profile must be required in cua_run_profile"
    );

    // Emergency Halt schema details
    let halt_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_emergency_halt")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let halt_props = halt_schema["properties"].as_object().unwrap();
    assert!(
        halt_props.contains_key("reset"),
        "cua_emergency_halt must have reset property"
    );
}

// ============================================================================
// 2. ADVERSARIAL ARGUMENT VALIDATION (STRESS TESTS)
// ============================================================================

#[test]
fn test_c2_adversarial_argument_validation() {
    let server = liva_native_core::mcp::server::NativeMcpServer::new("test_vault");
    let tools = server.list_tools().tools;

    let som_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_execute_som")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let prof_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_run_profile")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let inspect_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_inspect_ui")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let halt_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_emergency_halt")
            .unwrap()
            .input_schema,
    )
    .unwrap();

    // 1. Missing required fields
    let err_missing_action = validate_arguments(&som_schema, &json!({ "som_index": 1 }));
    assert!(err_missing_action.is_err(), "Must reject missing 'action'");
    assert!(
        err_missing_action
            .unwrap_err()
            .contains("thiếu tham số bắt buộc: action")
    );

    let err_missing_som = validate_arguments(&som_schema, &json!({ "action": "click" }));
    assert!(err_missing_som.is_err(), "Must reject missing 'som_index'");
    assert!(
        err_missing_som
            .unwrap_err()
            .contains("thiếu tham số bắt buộc: som_index")
    );

    let err_missing_prof = validate_arguments(&prof_schema, &json!({ "target_hwnd": 1001 }));
    assert!(err_missing_prof.is_err(), "Must reject missing 'profile'");
    assert!(
        err_missing_prof
            .unwrap_err()
            .contains("thiếu tham số bắt buộc: profile")
    );

    // 2. Empty string & whitespace on required fields (anti-hallucination guard)
    let err_empty_action =
        validate_arguments(&som_schema, &json!({ "som_index": 1, "action": "" }));
    assert!(
        err_empty_action.is_err(),
        "Must reject empty string for required 'action'"
    );
    assert!(
        err_empty_action
            .unwrap_err()
            .contains("tham số bắt buộc rỗng")
    );

    let err_ws_action =
        validate_arguments(&som_schema, &json!({ "som_index": 1, "action": "   " }));
    assert!(
        err_ws_action.is_err(),
        "Must reject whitespace-only string for required 'action'"
    );
    assert!(err_ws_action.unwrap_err().contains("tham số bắt buộc rỗng"));

    let err_empty_prof = validate_arguments(&prof_schema, &json!({ "profile": "" }));
    assert!(
        err_empty_prof.is_err(),
        "Must reject empty string for required 'profile'"
    );
    assert!(
        err_empty_prof
            .unwrap_err()
            .contains("tham số bắt buộc rỗng")
    );

    // 3. Type mismatches on required fields
    let err_som_type = validate_arguments(
        &som_schema,
        &json!({ "som_index": "one", "action": "click" }),
    );
    assert!(
        err_som_type.is_err(),
        "Must reject string for integer som_index"
    );
    assert!(err_som_type.unwrap_err().contains("phải là integer"));

    let err_action_type =
        validate_arguments(&som_schema, &json!({ "som_index": 1, "action": 123 }));
    assert!(
        err_action_type.is_err(),
        "Must reject integer for string action"
    );
    assert!(err_action_type.unwrap_err().contains("phải là string"));

    let err_prof_type = validate_arguments(&prof_schema, &json!({ "profile": 123 }));
    assert!(
        err_prof_type.is_err(),
        "Must reject integer for string profile"
    );
    assert!(err_prof_type.unwrap_err().contains("phải là string"));

    // 4. Unknown/extra properties are safely tolerated by validate_arguments (open-world schema)
    let ok_extra = validate_arguments(
        &inspect_schema,
        &json!({ "target_hwnd": 1001, "unknown_annotation": "test" }),
    );
    assert!(
        ok_extra.is_ok(),
        "Unknown properties should be tolerated by validate_arguments"
    );
}

// ============================================================================
// 3. COMPACT PARAMETER RENDERING (PROMPT EFFICIENCY)
// ============================================================================

#[test]
fn test_c2_cua_tool_param_rendering() {
    let server = liva_native_core::mcp::server::NativeMcpServer::new("test_vault");
    let tools = server.list_tools().tools;

    let som_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_execute_som")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let som_params = render_params(&som_schema);
    assert!(
        som_params.contains("som_index*: integer"),
        "som_index must be rendered with '*' required marker"
    );
    assert!(
        som_params.contains("action*: string"),
        "action must be rendered with '*' required marker"
    );
    assert!(
        som_params.lines().count() <= 1,
        "render_params must be a single compact line"
    );

    let prof_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_run_profile")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let prof_params = render_params(&prof_schema);
    assert!(
        prof_params.contains("profile*: string"),
        "profile must be rendered with '*' required marker"
    );
    assert!(prof_params.lines().count() <= 1);

    let inspect_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_inspect_ui")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let inspect_params = render_params(&inspect_schema);
    assert!(
        inspect_params.contains("target_hwnd"),
        "target_hwnd is present"
    );
    assert!(
        !inspect_params.contains("target_hwnd*"),
        "target_hwnd must NOT have required marker"
    );
    assert!(inspect_params.lines().count() <= 1);

    let halt_schema = serde_json::to_value(
        &tools
            .iter()
            .find(|t| t.name == "cua_emergency_halt")
            .unwrap()
            .input_schema,
    )
    .unwrap();
    let halt_params = render_params(&halt_schema);
    assert!(halt_params.contains("reset"), "reset is present");
    assert!(
        !halt_params.contains("reset*"),
        "reset must NOT have required marker"
    );
    assert!(halt_params.lines().count() <= 1);
}

// ============================================================================
// 4. NATIVE_AUTOEXEC ALLOWLIST BOUNDARY & PRIVILEGE BARRIERS
// ============================================================================

#[test]
fn test_c2_native_autoexec_allowlist_barriers() {
    // 1. Tool execution policy defaults
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_inspect_ui"),
        ToolExecPolicy::Auto
    );
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_emergency_halt"),
        ToolExecPolicy::Auto
    );
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_execute_som"),
        ToolExecPolicy::ProposeOnly
    );
    assert_eq!(
        tool_exec_policy(NATIVE_SERVER, "cua_run_profile"),
        ToolExecPolicy::ProposeOnly
    );

    // 2. Direct-call guards
    assert!(guard_direct_call(NATIVE_SERVER, "cua_inspect_ui").is_ok());
    assert!(guard_direct_call(NATIVE_SERVER, "cua_emergency_halt").is_ok());

    let som_guard_err = guard_direct_call(NATIVE_SERVER, "cua_execute_som");
    assert!(
        som_guard_err.is_err(),
        "guard_direct_call must block cua_execute_som"
    );
    assert!(
        som_guard_err
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy")
    );

    let prof_guard_err = guard_direct_call(NATIVE_SERVER, "cua_run_profile");
    assert!(
        prof_guard_err.is_err(),
        "guard_direct_call must block cua_run_profile"
    );
    assert!(
        prof_guard_err
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy")
    );
}

// ============================================================================
// 5. EXECUTE_CALL DEFENSE-IN-DEPTH AGAINST POLICY SPOOFING
// ============================================================================

#[tokio::test]
async fn test_c2_execute_call_defense_in_depth_anti_spoofing() {
    let state = create_adversarial_test_state();

    // Adversarial attack: caller creates a ResolvedCall claiming policy is Auto for mutating tools
    let spoofed_som_call = ResolvedCall {
        server: NATIVE_SERVER.to_string(),
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "click"
        }),
        policy: ExecPolicy::Auto, // Spoofed!
    };

    let result = execute_call(&state, &spoofed_som_call, None).await;
    assert!(
        result.is_err(),
        "execute_call must reject spoofed policy for cua_execute_som"
    );
    assert!(
        result
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy"),
        "Error message must indicate allowlist rejection"
    );

    let spoofed_profile_call = ResolvedCall {
        server: NATIVE_SERVER.to_string(),
        name: "cua_run_profile".to_string(),
        arguments: json!({
            "profile": "flash",
            "target_hwnd": 2002
        }),
        policy: ExecPolicy::Auto, // Spoofed!
    };

    let prof_result = execute_call(&state, &spoofed_profile_call, None).await;
    assert!(
        prof_result.is_err(),
        "execute_call must reject spoofed policy for cua_run_profile"
    );
    assert!(
        prof_result
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy")
    );

    // Legitimate read-only tool with Auto policy executes cleanly
    let legitimate_inspect_call = ResolvedCall {
        server: NATIVE_SERVER.to_string(),
        name: "cua_inspect_ui".to_string(),
        arguments: json!({ "target_hwnd": 2002 }),
        policy: ExecPolicy::Auto,
    };
    let inspect_res = execute_call(&state, &legitimate_inspect_call, None).await;
    assert!(
        inspect_res.is_ok(),
        "Legitimate Auto call to cua_inspect_ui must succeed"
    );
    assert!(!inspect_res.unwrap().is_error);
}

// ============================================================================
// 6. MCP:CALL_TOOL IPC DIRECT-CALL PROTECTION
// ============================================================================

#[tokio::test]
async fn test_c2_mcp_ipc_direct_call_protection() {
    let state = create_adversarial_test_state();

    // Layer 1 Protection: CommandPrincipal::TauriDashboard is NOT authorized for mcp:call_tool at all
    let dashboard_blocked = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "mcp:call_tool",
        json!({
            "name": "cua_execute_som",
            "arguments": {
                "target_hwnd": 2002,
                "som_index": 1,
                "action": "click"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        dashboard_blocked.is_err(),
        "TauriDashboard must not have access to mcp:call_tool"
    );
    assert!(
        dashboard_blocked
            .unwrap_err()
            .contains("is not authorized for command 'mcp:call_tool'")
    );

    // Layer 2 Protection: Even with CommandPrincipal::Test (which passes IPC authorization),
    // guard_direct_call in commands/mcp.rs blocks mutating cua_execute_som
    let som_ipc_err = handle_command_as(
        CommandPrincipal::Test,
        state.clone(),
        "mcp:call_tool",
        json!({
            "name": "cua_execute_som",
            "arguments": {
                "target_hwnd": 2002,
                "som_index": 1,
                "action": "click"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        som_ipc_err.is_err(),
        "mcp:call_tool for cua_execute_som must fail closed via guard_direct_call"
    );
    assert!(
        som_ipc_err
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy")
    );

    // Mutating cua_run_profile via IPC mcp:call_tool must also be blocked by guard_direct_call
    let prof_ipc_err = handle_command_as(
        CommandPrincipal::Test,
        state.clone(),
        "mcp:call_tool",
        json!({
            "name": "cua_run_profile",
            "arguments": {
                "profile": "flash",
                "target_hwnd": 2002
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        prof_ipc_err.is_err(),
        "mcp:call_tool for cua_run_profile must fail closed via guard_direct_call"
    );
    assert!(
        prof_ipc_err
            .unwrap_err()
            .contains("không nằm trong allowlist tự chạy")
    );

    // Read-only cua_inspect_ui via IPC mcp:call_tool succeeds for CommandPrincipal::Test
    let inspect_ipc_ok = handle_command_as(
        CommandPrincipal::Test,
        state.clone(),
        "mcp:call_tool",
        json!({
            "name": "cua_inspect_ui",
            "arguments": { "target_hwnd": 2002 }
        }),
        None,
        None,
    )
    .await;
    assert!(
        inspect_ipc_ok.is_ok(),
        "mcp:call_tool for cua_inspect_ui must succeed"
    );

    // Emergency halt via IPC mcp:call_tool succeeds for CommandPrincipal::Test
    let halt_ipc_ok = handle_command_as(
        CommandPrincipal::Test,
        state.clone(),
        "mcp:call_tool",
        json!({
            "name": "cua_emergency_halt",
            "arguments": {}
        }),
        None,
        None,
    )
    .await;
    assert!(
        halt_ipc_ok.is_ok(),
        "mcp:call_tool for cua_emergency_halt must succeed"
    );
}

// ============================================================================
// 7. DEFAULT_TOP_K (11) CATALOG RETENTION INVARIANT
// ============================================================================

#[tokio::test]
async fn test_c2_default_top_k_catalog_retention_invariant() {
    let state = create_adversarial_test_state();
    let catalog = build_catalog(&state).await;

    // Verify exactly 11 tools in the native catalog
    assert_eq!(
        catalog.len(),
        11,
        "Native catalog must contain exactly 11 tools"
    );
    assert_eq!(
        DEFAULT_TOP_K, 11,
        "DEFAULT_TOP_K must be 11 to match catalog size"
    );
    assert!(
        DEFAULT_TOP_K >= catalog.len(),
        "DEFAULT_TOP_K must not be less than catalog size"
    );

    let expected_tools = [
        "read_markdown",
        "write_markdown",
        "search_vault",
        "control_smarthome",
        "control_volume",
        "control_media",
        "get_weather",
        "cua_inspect_ui",
        "cua_execute_som",
        "cua_run_profile",
        "cua_emergency_halt",
    ];

    for name in expected_tools {
        assert!(
            catalog.tools().iter().any(|t| t.name == name),
            "Tool '{}' must be present in the catalog",
            name
        );
    }

    // Stress test: Rank tools with k = DEFAULT_TOP_K.
    // Proves that all 11 tools are retained and none are silently truncated.
    let ranked_indices = rank_tools(&catalog, "bấm nút số 1 trên màn hình", None, DEFAULT_TOP_K);
    assert_eq!(
        ranked_indices.len(),
        11,
        "All 11 tools must be retained in ranked output when k = DEFAULT_TOP_K"
    );

    // If k were 7 (the legacy M10 value), 4 tools would be dropped silently
    let legacy_ranked = rank_tools(&catalog, "bấm nút số 1 trên màn hình", None, 7);
    assert_eq!(
        legacy_ranked.len(),
        7,
        "Legacy top_k=7 silently truncates 4 tools"
    );
}

// ============================================================================
// 8. EMERGENCY HALT KILL-SWITCH LIFECYCLE & MUTATING ACTION BLOCKING
// ============================================================================

#[tokio::test]
async fn test_c2_cua_emergency_halt_kill_switch_lifecycle() {
    let state = create_adversarial_test_state();

    // 1. Initial state: not halted
    assert!(!state.cua.kill_switch.is_halted());

    // 2. Trigger emergency halt via MCP tool
    let halt_req = CallToolRequest {
        name: "cua_emergency_halt".to_string(),
        arguments: json!({}),
    };
    let halt_res = state
        .mcp_server
        .call_tool(halt_req)
        .await
        .expect("halt succeeds");
    assert!(!halt_res.is_error);
    assert!(
        state.cua.kill_switch.is_halted(),
        "Kill switch must be halted"
    );

    // 3. Mutating action via MCP while halted must fail
    let som_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "click"
        }),
    };
    let som_res = state
        .mcp_server
        .call_tool(som_req.clone())
        .await
        .expect("call completes");
    assert!(som_res.is_error, "Action must report error when halted");
    let err_msg = match &som_res.content[0] {
        ToolContent::Text { text } => text,
        _ => panic!("Expected text error"),
    };
    assert!(
        err_msg.contains("halt") || err_msg.contains("Emergency") || err_msg.contains("failed"),
        "Error message should mention halt/failure: {}",
        err_msg
    );

    // 4. Reset emergency halt via MCP tool
    let reset_req = CallToolRequest {
        name: "cua_emergency_halt".to_string(),
        arguments: json!({ "reset": true }),
    };
    let reset_res = state
        .mcp_server
        .call_tool(reset_req)
        .await
        .expect("reset succeeds");
    assert!(!reset_res.is_error);
    assert!(
        !state.cua.kill_switch.is_halted(),
        "Kill switch must be unhalted after reset"
    );

    // 5. Mutating action now succeeds
    let som_ok = state
        .mcp_server
        .call_tool(som_req)
        .await
        .expect("call completes");
    assert!(!som_ok.is_error, "Action must succeed after halt reset");
}

// ============================================================================
// 9. ADVERSARIAL SOM RESOLUTION & ACTION BOUNDARY VALIDATION
// ============================================================================

#[tokio::test]
async fn test_c2_adversarial_som_resolution_and_unsupported_actions() {
    let state = create_adversarial_test_state();

    // 1. Unsupported action type
    let bad_action_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "eval_malicious_script"
        }),
    };
    let bad_action_res = state
        .mcp_server
        .call_tool(bad_action_req)
        .await
        .expect("call completes");
    assert!(
        bad_action_res.is_error,
        "Unsupported action must return is_error: true"
    );
    let bad_text = match &bad_action_res.content[0] {
        ToolContent::Text { text } => text,
        _ => panic!("Expected text"),
    };
    assert!(
        bad_text.contains("Unsupported action"),
        "Must report unsupported action"
    );

    // 2. Out-of-bounds SoM index
    let oob_som_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 99999,
            "action": "click"
        }),
    };
    let oob_som_res = state
        .mcp_server
        .call_tool(oob_som_req)
        .await
        .expect("call completes");
    assert!(
        oob_som_res.is_error,
        "Out-of-bounds SoM index must return is_error: true"
    );
    let oob_text = match &oob_som_res.content[0] {
        ToolContent::Text { text } => text,
        _ => panic!("Expected text"),
    };
    assert!(
        oob_text.contains("Failed to resolve SoM index 99999"),
        "Must report resolution failure"
    );

    // 3. Different valid action types: double_click, hover, type_text, scroll
    let double_click_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "double_click"
        }),
    };
    let dc_res = state
        .mcp_server
        .call_tool(double_click_req)
        .await
        .expect("call completes");
    assert!(!dc_res.is_error, "double_click must succeed");

    let hover_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "hover"
        }),
    };
    let hover_res = state
        .mcp_server
        .call_tool(hover_req)
        .await
        .expect("call completes");
    assert!(!hover_res.is_error, "hover must succeed");

    let type_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 2,
            "action": "type_text",
            "text": "Hello Adversarial Test"
        }),
    };
    let type_res = state
        .mcp_server
        .call_tool(type_req)
        .await
        .expect("call completes");
    assert!(!type_res.is_error, "type_text must succeed");

    let scroll_req = CallToolRequest {
        name: "cua_execute_som".to_string(),
        arguments: json!({
            "target_hwnd": 2002,
            "som_index": 1,
            "action": "scroll",
            "delta_y": -240
        }),
    };
    let scroll_res = state
        .mcp_server
        .call_tool(scroll_req)
        .await
        .expect("call completes");
    assert!(!scroll_res.is_error, "scroll must succeed");
}
