//! Comprehensive Security Policy & Sandboxing Test Suite.
//! Milestone M2: Sandboxing, Permission Modes & Emergency Kill-Switch.

use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;

// ===========================================================================
// 1. Permission Mode Boundaries (Standard, Bounded, Unrestricted)
// ===========================================================================

#[tokio::test]
async fn test_standard_mode_read_only_enforcement() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x3000,
        pid: 700,
        title: "Target App".into(),
        class_name: "AppClass".into(),
        process_name: Some("app.exe".into()),
        bounds: CuaRect::new(0, 0, 600, 400),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Standard,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Click must be denied in Standard (read-only) mode
    let action = CuaAction::Click {
        target_hwnd: 0x3000,
        x: Some(100),
        y: Some(100),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let result = engine.execute_action(action).await;
    assert!(matches!(result, Err(CuaError::PermissionDenied(_))));

    // Type text must be denied in Standard mode
    let action_text = CuaAction::TypeText {
        target_hwnd: 0x3000,
        text: "Forbidden".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    let result_text = engine.execute_action(action_text).await;
    assert!(matches!(result_text, Err(CuaError::PermissionDenied(_))));
}

#[tokio::test]
async fn test_standard_mode_blocks_all_mutating_inputs() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1000,
        pid: 100,
        title: "Target".into(),
        class_name: "AppClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Standard,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // 1. Click -> Blocked
    let click = CuaAction::Click {
        target_hwnd: 0x1000,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(click).await,
        Err(CuaError::PermissionDenied(_))
    ));

    // 2. TypeText -> Blocked
    let type_text = CuaAction::TypeText {
        target_hwnd: 0x1000,
        text: "hello".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(type_text).await,
        Err(CuaError::PermissionDenied(_))
    ));

    // 3. Scroll -> Blocked
    let scroll = CuaAction::Scroll {
        target_hwnd: 0x1000,
        delta_x: 0,
        delta_y: 120,
        x: Some(200),
        y: Some(200),
    };
    assert!(matches!(
        engine.execute_action(scroll).await,
        Err(CuaError::PermissionDenied(_))
    ));

    // 4. Hotkey -> Blocked
    let hotkey = CuaAction::Hotkey {
        target_hwnd: 0x1000,
        keys: vec!["Ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(hotkey).await,
        Err(CuaError::PermissionDenied(_))
    ));

    // 5. RestoreWindow -> Permitted (non-mutating inspection preparation)
    let restore = CuaAction::RestoreWindow {
        target_hwnd: 0x1000,
    };
    assert!(engine.execute_action(restore).await.is_ok());
}

// ===========================================================================
// 2. Process Allowlist Enforcement
// ===========================================================================

#[tokio::test]
async fn test_bounded_mode_process_allowlist_matching() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2001,
        pid: 201,
        title: "Notepad".into(),
        class_name: "Notepad".into(),
        process_name: Some("NOTEPAD.EXE".into()), // Test uppercase normalization
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2002,
        pid: 202,
        title: "Malicious App".into(),
        class_name: "HackerClass".into(),
        process_name: Some("trojan.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["notepad.exe".into(), "calc.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Notepad is allowed
    let allowed_click = CuaAction::Click {
        target_hwnd: 0x2001,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(engine.execute_action(allowed_click).await.is_ok());

    // Trojan is blocked
    let denied_click = CuaAction::Click {
        target_hwnd: 0x2002,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res = engine.execute_action(denied_click).await;
    assert!(matches!(res, Err(CuaError::PermissionDenied(_))));
}

// ===========================================================================
// 3. Protected Window Denylist Enforcement
// ===========================================================================

#[tokio::test]
async fn test_denylist_takes_precedence_over_allowlist() {
    let mock = Arc::new(MockCuaDriver::new());
    // Inadvertently allowlisted Taskmgr
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x3001,
        pid: 301,
        title: "Task Manager".into(),
        class_name: "TaskManagerWindow".into(),
        process_name: Some("taskmgr.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["taskmgr.exe".into()], // Accidental allowlisting!
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    let action = CuaAction::Click {
        target_hwnd: 0x3001,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // Denylist MUST take precedence and block it!
    let res = engine.execute_action(action).await;
    assert!(matches!(res, Err(CuaError::PermissionDenied(_))));
}

#[tokio::test]
async fn test_protected_window_classes_denylist() {
    let mock = Arc::new(MockCuaDriver::new());
    // Windows Taskbar
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4001,
        pid: 50,
        title: "".into(),
        class_name: "Shell_TrayWnd".into(),
        process_name: Some("explorer.exe".into()),
        bounds: CuaRect::new(0, 1040, 1920, 40),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["explorer.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    let action = CuaAction::Click {
        target_hwnd: 0x4001,
        x: Some(10),
        y: Some(1050),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = engine.execute_action(action).await;
    assert!(matches!(res, Err(CuaError::PermissionDenied(_))));
}

// ===========================================================================
// 4. Dangerous Hotkey Filtering
// ===========================================================================

#[tokio::test]
async fn test_bounded_mode_dangerous_hotkeys_filter() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4000,
        pid: 800,
        title: "Target App 2".into(),
        class_name: "AppClass2".into(),
        process_name: Some("app2.exe".into()),
        bounds: CuaRect::new(0, 0, 600, 400),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Alt+F4 dangerous hotkey
    let alt_f4 = CuaAction::Hotkey {
        target_hwnd: 0x4000,
        keys: vec!["Alt".into(), "F4".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(alt_f4).await,
        Err(CuaError::DangerousHotkeyRejected(_))
    ));

    // Ctrl+Alt+Delete dangerous hotkey
    let cad = CuaAction::Hotkey {
        target_hwnd: 0x4000,
        keys: vec!["Ctrl".into(), "Alt".into(), "Delete".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(cad).await,
        Err(CuaError::DangerousHotkeyRejected(_))
    ));

    // Safe hotkey (Ctrl+C) permitted
    let ctrl_c = CuaAction::Hotkey {
        target_hwnd: 0x4000,
        keys: vec!["Ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(engine.execute_action(ctrl_c).await.is_ok());
}

#[tokio::test]
async fn test_dangerous_hotkeys_exhaustive_matrix() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5001,
        pid: 501,
        title: "Target".into(),
        class_name: "AppClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["notepad.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    let dangerous_cases = vec![
        vec!["Win", "L"],
        vec!["windows", "l"],
        vec!["Ctrl", "Alt", "Del"],
        vec!["control", "menu", "delete"],
        vec!["Win", "R"],
        vec!["WIN", "r"],
        vec!["Win", "X"],
        vec!["Ctrl", "Shift", "Esc"],
        vec!["ctrl", "shift", "escape"],
        vec!["Alt", "F4"],
    ];

    for keys in dangerous_cases {
        let action = CuaAction::Hotkey {
            target_hwnd: 0x5001,
            keys: keys.into_iter().map(String::from).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::DangerousHotkeyRejected(_))),
            "Failed to reject dangerous hotkey"
        );
    }

    // Legitimate hotkeys must succeed
    let safe_cases = vec![
        vec!["Ctrl", "c"],
        vec!["Ctrl", "v"],
        vec!["Ctrl", "z"],
        vec!["Ctrl", "a"],
        vec!["Alt", "Tab"],
    ];

    for keys in safe_cases {
        let action = CuaAction::Hotkey {
            target_hwnd: 0x5001,
            keys: keys.into_iter().map(String::from).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        assert!(engine.execute_action(action).await.is_ok());
    }
}

// ===========================================================================
// 5. Client Coordinate Containment
// ===========================================================================

#[tokio::test]
async fn test_client_coordinate_containment_and_title_bar_exclusion() {
    let mock = Arc::new(MockCuaDriver::new());
    // Window bounds: [100, 100, 800, 600].
    // Client area starts at [108, 130, 784, 562].
    // Title bar region: y in [100..130).
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x6001,
        pid: 601,
        title: "Target Window".into(),
        class_name: "AppClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["notepad.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // 1. Inside client area: (200, 200) -> Allowed
    let inside_client = CuaAction::Click {
        target_hwnd: 0x6001,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(engine.execute_action(inside_client).await.is_ok());

    // 2. Title bar click: (200, 115) -> Rejected
    let title_bar_click = CuaAction::Click {
        target_hwnd: 0x6001,
        x: Some(200),
        y: Some(115),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(title_bar_click).await,
        Err(CuaError::CoordinateOutOfBounds { x: 200, y: 115 })
    ));

    // 3. Close button click: (885, 110) -> Rejected
    let close_button_click = CuaAction::Click {
        target_hwnd: 0x6001,
        x: Some(885),
        y: Some(110),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(close_button_click).await,
        Err(CuaError::CoordinateOutOfBounds { x: 885, y: 110 })
    ));

    // 4. Outside window bounds: (50, 50) -> Rejected
    let outside_bounds = CuaAction::Click {
        target_hwnd: 0x6001,
        x: Some(50),
        y: Some(50),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(matches!(
        engine.execute_action(outside_bounds).await,
        Err(CuaError::CoordinateOutOfBounds { x: 50, y: 50 })
    ));
}

// ===========================================================================
// 6. Engine-Level UIPI Enforcement Across All Actions
// ===========================================================================

#[tokio::test]
async fn test_engine_level_uipi_blocks_scroll_and_hotkey() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x7001,
        pid: 701,
        title: "Elevated Admin Console".into(),
        class_name: "AdminClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["notepad.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Set target window to High integrity RID (0x3000), agent is Medium (0x2000)
    engine.security_governor.set_simulated_agent_rid(0x2000);
    engine.security_governor.set_simulated_uipi(0x7001, 0x3000);

    // 1. Scroll must be blocked by UIPI
    let scroll = CuaAction::Scroll {
        target_hwnd: 0x7001,
        delta_x: 0,
        delta_y: 120,
        x: Some(200),
        y: Some(200),
    };
    let scroll_res = engine.execute_action(scroll).await;
    assert!(matches!(scroll_res, Err(CuaError::UipiBlocked(_))));

    // 2. Hotkey must be blocked by UIPI
    let hotkey = CuaAction::Hotkey {
        target_hwnd: 0x7001,
        keys: vec!["Ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    let hotkey_res = engine.execute_action(hotkey).await;
    assert!(matches!(hotkey_res, Err(CuaError::UipiBlocked(_))));

    // 3. MoveCursor must be blocked by UIPI
    let move_cursor = CuaAction::MoveCursor {
        target_hwnd: 0x7001,
        x: 200,
        y: 200,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let move_res = engine.execute_action(move_cursor).await;
    assert!(matches!(move_res, Err(CuaError::UipiBlocked(_))));
}
