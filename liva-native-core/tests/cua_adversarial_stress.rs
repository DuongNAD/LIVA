//! Adversarial Stress & Fuzzing Harness for CUA IPC Commands (Milestone M4)
//!
//! Empirically challenges and stress-tests:
//! 1. Malformed Payloads & Missing Fields Fuzzing:
//!    - Missing `som_index` / `target_som_id`, null, boolean, array, object values
//!    - Unsupported and malformed action strings & action templates
//!    - Missing and invalid fields in `set_mode`, `query_audit_logs`, `run_profile`
//! 2. Negative & Out-of-Bounds Set-of-Mark (SoM) Indices:
//!    - Negative indices (-1, -999999)
//!    - Zero index (0 - invalid 1-based index)
//!    - Out-of-bounds indices (999999, u64::MAX)
//!    - Handling in `execute_som_action` and `run_profile` (Flash & Pro)
//! 3. Emergency Stop Responsiveness & Concurrency Stress:
//!    - Sub-15ms latency SLA validation across burst executions
//!    - Immediate halting of subsequent actions with `emergency_halt` error code
//!    - Concurrency race conditions: simultaneous action execution and emergency halt
//!    - Clean reset and resumption
//! 4. PermissionMode::Standard (Read-Only) Enforcement:
//!    - Permitting `list_windows`, `inspect_ui_tree`, `get_status`, `query_audit_logs`
//!    - Blocking mutating actions: `execute_action`, `execute_som_action`, `run_profile`
//!    - Multi-mode transitions: Standard -> Bounded (allowlist check) -> Unrestricted

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{AppState, CommandPrincipal, db, handle_command_as, stt, tts};
use serde_json::{Value, json};
use std::sync::Arc;
use std::time::Instant;

fn setup_adversarial_cua_state() -> Arc<AppState> {
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

    // Window 1001: notepad.exe (allowlisted)
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

    // Window 1002: calc.exe (NOT allowlisted in Bounded mode)
    mock_driver.insert_window(liva_cua::CuaWindowInfo {
        hwnd: 1002,
        pid: 5678,
        title: "Calculator".to_string(),
        class_name: "CalcFrame".to_string(),
        process_name: Some("calc.exe".to_string()),
        bounds: liva_cua::CuaRect::new(100, 100, 400, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 1,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // UI tree for Window 1001 with 2 interactive elements:
    // [1] Button "Save" (bounds: 100, 100, 80, 30 -> center: 140, 115)
    // [2] Edit "Editor Area" (bounds: 100, 200, 600, 400 -> center: 400, 400)
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

// ─────────────────────────────────────────────────────────────────────────────
// 1. Malformed Payloads & Missing Fields Fuzzing
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_fuzz_execute_som_action_missing_and_malformed_index() {
    let state = setup_adversarial_cua_state();

    let invalid_payloads = vec![
        ("empty_object", json!({})),
        ("null_som_index", json!({ "som_index": null })),
        ("string_som_index", json!({ "som_index": "one" })),
        ("boolean_som_index", json!({ "som_index": true })),
        ("array_som_index", json!({ "som_index": [1] })),
        ("object_som_index", json!({ "som_index": { "val": 1 } })),
        ("float_som_index", json!({ "som_index": 1.5 })),
        ("empty_target_som_id", json!({ "target_som_id": null })),
        ("string_target_som_id", json!({ "target_som_id": "abc" })),
    ];

    for (desc, payload) in invalid_payloads {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:execute_som_action",
            payload,
            None,
            None,
        )
        .await;

        assert!(
            res.is_err(),
            "Payload '{desc}' must fail fail-closed on cua:execute_som_action"
        );
        let err_str = res.unwrap_err();
        let parsed: Value = serde_json::from_str(&err_str)
            .unwrap_or_else(|_| panic!("Error must be valid JSON for '{desc}', got: {err_str}"));
        assert_eq!(
            parsed["error"]["code"], "invalid_parameter",
            "Error code must be 'invalid_parameter' for '{desc}'"
        );
        assert_eq!(
            parsed["error"]["subsystem"], "cua",
            "Subsystem must be 'cua' for '{desc}'"
        );
    }
}

#[tokio::test]
async fn test_fuzz_execute_som_action_unsupported_action_strings() {
    let state = setup_adversarial_cua_state();

    let hostile_actions = [
        "fly_to_moon",
        "rm -rf /",
        "format_c",
        "inject_script",
        "__proto__",
        "DROP TABLE",
        "",
        "   ",
    ];

    for act in hostile_actions {
        let payload = json!({
            "target_hwnd": 1001,
            "som_index": 1,
            "action": act
        });

        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:execute_som_action",
            payload,
            None,
            None,
        )
        .await;

        assert!(
            res.is_err(),
            "Hostile action '{act}' must fail fail-closed on cua:execute_som_action"
        );
        let err_str = res.unwrap_err();
        let parsed: Value = serde_json::from_str(&err_str)
            .unwrap_or_else(|_| panic!("Error must be valid JSON for '{act}', got: {err_str}"));
        assert_eq!(parsed["error"]["code"], "invalid_parameter");
        assert!(
            parsed["error"]["message"]
                .as_str()
                .unwrap()
                .contains("Unsupported action type")
        );
    }
}

#[tokio::test]
async fn test_fuzz_execute_som_action_valid_action_case_insensitivity() {
    let state = setup_adversarial_cua_state();

    let variations = [
        (
            "CLICK",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "CLICK" }),
        ),
        (
            "Click",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "Click" }),
        ),
        (
            "double_click",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "DOUBLE_CLICK" }),
        ),
        (
            "HOVER",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "HOVER" }),
        ),
        (
            "Move_Cursor",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "MOVE_CURSOR" }),
        ),
        (
            "TYPE_TEXT",
            json!({ "target_hwnd": 1001, "som_index": 2, "action": "TYPE_TEXT", "text": "hello" }),
        ),
        (
            "SCROLL",
            json!({ "target_hwnd": 1001, "som_index": 1, "action": "SCROLL", "delta_y": -100 }),
        ),
    ];

    for (desc, payload) in variations {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:execute_som_action",
            payload,
            None,
            None,
        )
        .await;

        assert!(
            res.is_ok(),
            "Case variation '{desc}' should be accepted: {:?}",
            res.err()
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. Negative & Out-of-Bounds SoM Indices Stress Tests
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_som_index_negative_and_zero_and_out_of_bounds() {
    let state = setup_adversarial_cua_state();

    // 1. Negative SoM index (-1)
    let neg_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": -1,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(neg_res.is_err(), "Negative SoM index must be rejected");
    let err: Value = serde_json::from_str(&neg_res.unwrap_err()).unwrap();
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 2. Negative target_som_id (-9999)
    let neg_id_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "target_som_id": -9999,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(
        neg_id_res.is_err(),
        "Negative target_som_id must be rejected"
    );

    // 3. Zero SoM index (0 is invalid: SoM indices are 1-based [1..N])
    let zero_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 0,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(zero_res.is_err(), "Zero SoM index must be rejected");
    let zero_err: Value = serde_json::from_str(&zero_res.unwrap_err()).unwrap();
    assert_eq!(zero_err["error"]["code"], "invalid_parameter");
    assert!(
        zero_err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("SoM mark [0] not found")
    );

    // 4. Out-of-bounds SoM index (999999)
    let oob_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": 999999,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(oob_res.is_err(), "Out-of-bounds SoM index must be rejected");
    let oob_err: Value = serde_json::from_str(&oob_res.unwrap_err()).unwrap();
    assert_eq!(oob_err["error"]["code"], "invalid_parameter");
    assert!(
        oob_err["error"]["message"]
            .as_str()
            .unwrap()
            .contains("SoM mark [999999] not found")
    );

    // 5. u64::MAX boundary
    let max_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_som_action",
        json!({
            "target_hwnd": 1001,
            "som_index": u64::MAX,
            "action": "click"
        }),
        None,
        None,
    )
    .await;
    assert!(
        max_res.is_err(),
        "u64::MAX SoM index must not panic and must be rejected"
    );
}

#[tokio::test]
async fn test_som_index_bounds_in_run_profile_flash_and_pro() {
    let state = setup_adversarial_cua_state();

    // 1. Flash profile with out-of-bounds SoM index (99999)
    let flash_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "flash",
            "plan_id": "oob_flash_plan",
            "target_hwnd": 1001,
            "steps": [
                { "action": "click", "target_som_id": 99999 }
            ]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile itself returns execution result");

    assert_eq!(
        flash_res["result"]["success"], false,
        "Plan must fail on OOB step"
    );
    assert_eq!(flash_res["result"]["completed_steps"], 0);
    let step_err = flash_res["result"]["step_results"][0]["error"]
        .as_str()
        .expect("Error message present");
    assert!(
        step_err.contains("SoM mark [99999] not found"),
        "Step error must reflect mark not found: {step_err}"
    );

    // 2. Pro profile with zero SoM index (0)
    let pro_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "pro",
            "plan_id": "zero_pro_plan",
            "target_hwnd": 1001,
            "steps": [
                { "action": "click", "target_som_id": 0 }
            ]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile returns execution result");

    assert_eq!(
        pro_res["result"]["success"], false,
        "Plan must fail on index 0"
    );
    let pro_step_err = pro_res["result"]["step_results"][0]["error"]
        .as_str()
        .expect("Error message present");
    assert!(
        pro_step_err.contains("[0] not found"),
        "Step error must reflect mark [0] not found: {pro_step_err}"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. Emergency Stop Responsiveness & Concurrency Stress
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_emergency_stop_sla_under_15ms() {
    let state = setup_adversarial_cua_state();

    for i in 0..50 {
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
        .expect("emergency_stop must succeed");

        let elapsed = start.elapsed();
        assert_eq!(res["halted"], true);
        let reported_latency = res["latency_ms"].as_u64().unwrap();

        assert!(
            reported_latency <= 15,
            "Iteration {i}: Emergency stop reported latency {reported_latency}ms exceeds 15ms SLA"
        );
        assert!(
            elapsed.as_millis() <= 50,
            "Iteration {i}: Total roundtrip {}ms exceeds bounded threshold",
            elapsed.as_millis()
        );

        // Reset for next iteration
        let reset_res = handle_command_as(
            CommandPrincipal::TauriWidget,
            state.clone(),
            "cua:emergency_stop",
            json!({ "reset": true }),
            None,
            None,
        )
        .await
        .expect("reset must succeed");
        assert_eq!(reset_res["halted"], false);
    }
}

#[tokio::test]
async fn test_emergency_stop_halts_all_subsequent_cua_operations() {
    let state = setup_adversarial_cua_state();

    // 1. Trigger halt
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

    // 2. cua:execute_action MUST fail with emergency_halt
    let act_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1001,
                "x": 100,
                "y": 100,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        act_res.is_err(),
        "execute_action must be blocked while halted"
    );
    let act_err: Value = serde_json::from_str(&act_res.unwrap_err()).unwrap();
    assert_eq!(act_err["error"]["code"], "emergency_halt");

    // 3. cua:execute_som_action MUST fail with emergency_halt
    let som_res = handle_command_as(
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
        som_res.is_err(),
        "execute_som_action must be blocked while halted"
    );
    let som_err: Value = serde_json::from_str(&som_res.unwrap_err()).unwrap();
    assert_eq!(som_err["error"]["code"], "emergency_halt");

    // 4. cua:run_profile (Flash) MUST fail with emergency_halt
    let flash_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "flash",
            "target_hwnd": 1001,
            "steps": [{ "action": "click", "target_som_id": 1 }]
        }),
        None,
        None,
    )
    .await;
    assert!(
        flash_res.is_err(),
        "run_profile Flash must be blocked while halted"
    );
    let flash_err: Value = serde_json::from_str(&flash_res.unwrap_err()).unwrap();
    assert_eq!(flash_err["error"]["code"], "emergency_halt");

    // 5. cua:run_profile (Pro) MUST fail with emergency_halt
    let pro_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "pro",
            "target_hwnd": 1001,
            "steps": [{ "action": "click", "target_som_id": 1 }]
        }),
        None,
        None,
    )
    .await;
    assert!(
        pro_res.is_err(),
        "run_profile Pro must be blocked while halted"
    );
    let pro_err: Value = serde_json::from_str(&pro_res.unwrap_err()).unwrap();
    assert_eq!(pro_err["error"]["code"], "emergency_halt");

    // 6. Reset emergency stop
    let reset_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({ "reset": true }),
        None,
        None,
    )
    .await
    .expect("reset succeeds");
    assert_eq!(reset_res["halted"], false);

    // 7. Verify operations now succeed
    let resume_res = handle_command_as(
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
        resume_res.is_ok(),
        "Operation must succeed after reset: {:?}",
        resume_res.err()
    );
}

#[tokio::test]
async fn test_emergency_stop_concurrent_race_resilience() {
    let state = setup_adversarial_cua_state();

    let mut join_handles = Vec::new();

    // Spawn 10 concurrent worker tasks attempting rapid SoM actions
    for task_id in 0..10 {
        let state_clone = state.clone();
        join_handles.push(tokio::spawn(async move {
            for _ in 0..15 {
                let _ = handle_command_as(
                    CommandPrincipal::TauriDashboard,
                    state_clone.clone(),
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
                tokio::time::sleep(std::time::Duration::from_millis(2)).await;
            }
            task_id
        }));
    }

    // Simultaneously trigger emergency stop mid-flight
    tokio::time::sleep(std::time::Duration::from_millis(5)).await;
    let halt_res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "cua:emergency_stop",
        json!({}),
        None,
        None,
    )
    .await
    .expect("emergency stop trigger");
    assert_eq!(halt_res["halted"], true);

    // Wait for all concurrent tasks to finish without panic
    for h in join_handles {
        let res = h.await;
        assert!(
            res.is_ok(),
            "Worker task must not panic during concurrent emergency halt"
        );
    }

    // Clean up: reset halt
    let _ = handle_command_as(
        CommandPrincipal::TauriWidget,
        state,
        "cua:emergency_stop",
        json!({ "reset": true }),
        None,
        None,
    )
    .await;
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. PermissionMode::Standard (Read-Only) Enforcement
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_permission_mode_standard_allows_read_only_and_blocks_all_mutations() {
    let state = setup_adversarial_cua_state();

    // 1. Set mode to Standard (Read-Only)
    let mode_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "Standard" }),
        None,
        None,
    )
    .await
    .expect("set_mode Standard succeeds");
    assert_eq!(mode_res["mode"], "standard");

    // 2. Read-only commands MUST succeed:
    // a) list_windows
    let win_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:list_windows",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        win_res.is_ok(),
        "list_windows must be allowed in Standard mode"
    );

    // b) inspect_ui_tree
    let tree_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:inspect_ui_tree",
        json!({ "target_hwnd": 1001 }),
        None,
        None,
    )
    .await;
    assert!(
        tree_res.is_ok(),
        "inspect_ui_tree must be allowed in Standard mode"
    );

    // c) get_status
    let status_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:get_status",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        status_res.is_ok(),
        "get_status must be allowed in Standard mode"
    );

    // d) query_audit_logs
    let logs_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:query_audit_logs",
        json!({ "limit": 10 }),
        None,
        None,
    )
    .await;
    assert!(
        logs_res.is_ok(),
        "query_audit_logs must be allowed in Standard mode"
    );

    // 3. Mutating commands MUST be blocked with permission_denied:
    // a) cua:execute_action (Click)
    let act_click_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1001,
                "x": 100,
                "y": 100,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        act_click_res.is_err(),
        "execute_action Click must be blocked in Standard mode"
    );
    let click_err: Value = serde_json::from_str(&act_click_res.unwrap_err()).unwrap();
    assert_eq!(click_err["error"]["code"], "permission_denied");

    // b) cua:execute_action (TypeText)
    let act_type_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "type_text",
                "target_hwnd": 1001,
                "text": "test"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        act_type_res.is_err(),
        "execute_action TypeText must be blocked in Standard mode"
    );
    let type_err: Value = serde_json::from_str(&act_type_res.unwrap_err()).unwrap();
    assert_eq!(type_err["error"]["code"], "permission_denied");

    // c) cua:execute_som_action (Click)
    let som_act_res = handle_command_as(
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
        som_act_res.is_err(),
        "execute_som_action must be blocked in Standard mode"
    );
    let som_act_err: Value = serde_json::from_str(&som_act_res.unwrap_err()).unwrap();
    assert_eq!(som_act_err["error"]["code"], "permission_denied");

    // d) cua:run_profile (Flash)
    let flash_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "flash",
            "target_hwnd": 1001,
            "steps": [{ "action": "click", "target_som_id": 1 }]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile returns execution report");
    assert_eq!(
        flash_res["result"]["success"], false,
        "Flash plan must fail in Standard mode"
    );
    let flash_step_err = flash_res["result"]["step_results"][0]["error"]
        .as_str()
        .unwrap();
    assert!(flash_step_err.contains("Permission denied"));

    // e) cua:run_profile (Pro)
    let pro_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({
            "profile": "pro",
            "target_hwnd": 1001,
            "steps": [{ "action": "click", "target_som_id": 1 }]
        }),
        None,
        None,
    )
    .await
    .expect("run_profile returns execution report");
    assert_eq!(
        pro_res["result"]["success"], false,
        "Pro plan must fail in Standard mode"
    );
    let pro_step_err = pro_res["result"]["step_results"][0]["error"]
        .as_str()
        .unwrap();
    assert!(pro_step_err.contains("Permission denied"));
}

#[tokio::test]
async fn test_permission_mode_transitions_and_bounded_allowlist_filtering() {
    let state = setup_adversarial_cua_state();

    // 1. Set mode to Bounded with allowlist = ["notepad.exe"]
    handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({
            "mode": "Bounded",
            "allowlist": ["notepad.exe"]
        }),
        None,
        None,
    )
    .await
    .expect("set_mode Bounded succeeds");

    // 2. Action targeting allowlisted notepad.exe (HWND 1001) MUST SUCCEED
    let allowlisted_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1001,
                "x": 100,
                "y": 100,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        allowlisted_res.is_ok(),
        "Allowlisted app action must succeed in Bounded mode"
    );

    // 3. Action targeting NON-allowlisted calc.exe (HWND 1002) MUST BE BLOCKED
    let non_allowlisted_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1002,
                "x": 100,
                "y": 100,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        non_allowlisted_res.is_err(),
        "Non-allowlisted app action must be blocked in Bounded mode"
    );
    let block_err: Value = serde_json::from_str(&non_allowlisted_res.unwrap_err()).unwrap();
    assert_eq!(block_err["error"]["code"], "permission_denied");

    // 4. Transition to Unrestricted mode
    handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({ "mode": "Unrestricted" }),
        None,
        None,
    )
    .await
    .expect("set_mode Unrestricted succeeds");

    // 5. Action targeting calc.exe (HWND 1002) now SUCCEEDS
    let unrestricted_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:execute_action",
        json!({
            "action": {
                "action": "click",
                "target_hwnd": 1002,
                "x": 100,
                "y": 100,
                "button": "left"
            }
        }),
        None,
        None,
    )
    .await;
    assert!(
        unrestricted_res.is_ok(),
        "Non-allowlisted app action must succeed in Unrestricted mode: {:?}",
        unrestricted_res.err()
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. Hostile & Fuzzed Payloads on Aux Verbs (set_mode, query_audit_logs, run_profile)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_fuzz_aux_cua_verbs() {
    let state = setup_adversarial_cua_state();

    // 1. cua:set_mode missing mode
    let missing_mode = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:set_mode",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(missing_mode.is_err());
    let err: Value = serde_json::from_str(&missing_mode.unwrap_err()).unwrap();
    assert_eq!(err["error"]["code"], "invalid_parameter");

    // 2. cua:set_mode invalid mode name
    let invalid_modes = ["Root", "Admin", "SuperUser", "123", "", "true"];
    for inv in invalid_modes {
        let inv_res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:set_mode",
            json!({ "mode": inv }),
            None,
            None,
        )
        .await;
        assert!(inv_res.is_err(), "Mode '{inv}' must be rejected");
        let err: Value = serde_json::from_str(&inv_res.unwrap_err()).unwrap();
        assert_eq!(err["error"]["code"], "invalid_parameter");
    }

    // 3. cua:query_audit_logs invalid verdict
    let inv_verdict = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:query_audit_logs",
        json!({ "security_verdict": "MALICIOUS_INJECTION" }),
        None,
        None,
    )
    .await;
    assert!(
        inv_verdict.is_err(),
        "Invalid security verdict must be rejected"
    );
    let v_err: Value = serde_json::from_str(&inv_verdict.unwrap_err()).unwrap();
    assert_eq!(v_err["error"]["code"], "invalid_parameter");

    // 4. cua:run_profile empty payload gracefully handled (defaults to Flash with 0 steps)
    let empty_run = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:run_profile",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(
        empty_run.is_ok(),
        "Empty run_profile should default safely without panicking"
    );
    let empty_val = empty_run.unwrap();
    assert_eq!(empty_val["result"]["completed_steps"], 0);
    assert_eq!(empty_val["result"]["success"], true);

    // 5. cua:run_profile with malformed plan JSON object
    let malformed_plan = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:run_profile",
        json!({ "plan": "not_an_object_string" }),
        None,
        None,
    )
    .await;
    assert!(malformed_plan.is_err(), "Malformed plan must be rejected");
    let p_err: Value = serde_json::from_str(&malformed_plan.unwrap_err()).unwrap();
    assert_eq!(p_err["error"]["code"], "invalid_parameter");
}

// ─────────────────────────────────────────────────────────────────────────────
// 6. Empirical Defect Demonstration: Flattened CuaAction Payload Deserialization Bug
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_defect_cua_execute_action_flattened_payload_bug() {
    let state = setup_adversarial_cua_state();

    // Verify that flattened action object (standard Tauri IPC / tool format)
    // where "action" is the enum tag string succeeds:
    let flattened_payload = json!({
        "action": "click",
        "target_hwnd": 1001,
        "x": 100,
        "y": 100,
        "button": "left"
    });

    let res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "cua:execute_action",
        flattened_payload,
        None,
        None,
    )
    .await;

    assert!(
        res.is_ok(),
        "Flattened CuaAction payload parses and succeeds: {:?}",
        res
    );

    // Conversely, verify wrapped format { "action": { ... } } also succeeds:
    let wrapped_payload = json!({
        "action": {
            "action": "click",
            "target_hwnd": 1001,
            "x": 100,
            "y": 100,
            "button": "left"
        }
    });

    let wrapped_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state,
        "cua:execute_action",
        wrapped_payload,
        None,
        None,
    )
    .await;
    assert!(
        wrapped_res.is_ok(),
        "Wrapped CuaAction payload parses and succeeds: {:?}",
        wrapped_res
    );
}
