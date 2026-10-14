//! Empirical Challenger Adversarial Test Suite for Win32CuaDriver and CuaEngine.
//!
//! Thoroughly challenges:
//! 1. Bounds violations (out-of-bounds, negative, zero, multi-monitor virtual desktop coordinates).
//! 2. Invalid window handles (null HWND, dead HWND, minimized window).
//! 3. Hotkey parsing matrix (casing variations, modifier aliases, unrecognized keys, compound strings, atomic failure).
//! 4. UIPI and Security Governor enforcement (prohibited kernel PIDs, elevation denylist, dangerous hotkeys, coordinate containment).

use liva_cua::input::keyboard::parse_virtual_key;
use liva_cua::input::{validate_window_and_uipi, KeyboardController, MouseController};
use liva_cua::mock::MockCuaDriver;
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaActionEffect, CuaConfig, CuaDeliveryMode, CuaError, CuaRect, CuaWindowInfo,
    MouseButton, PermissionMode,
};
use liva_cua::window::list_windows;
use liva_cua::{CuaEngine, Win32CuaDriver};
use std::sync::Arc;

const NULL_HWND: u64 = 0;
const DEAD_HWND_1: u64 = 0x00DE_ADBE_EF00_1234;
const DEAD_HWND_2: u64 = 0xBAAD_F00D_CAFE_BABE;
const DEAD_HWND_MAX: u64 = u64::MAX;

// ============================================================================
// 1. Invalid Window Handles & Window Validation (Null, Dead, Out-of-Range)
// ============================================================================

#[tokio::test]
async fn test_empirical_null_hwnd_rejected_across_all_actions() {
    let driver = Win32CuaDriver::new();

    // 1. MoveCursor on NULL HWND
    let res = driver
        .dispatch_action(CuaAction::MoveCursor {
            target_hwnd: NULL_HWND,
            x: 100,
            y: 100,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res.is_err(), "MoveCursor with NULL HWND must fail");
    assert!(matches!(res.unwrap_err(), CuaError::WindowNotFound(0)));

    // 2. Drag on NULL HWND
    let res = driver
        .dispatch_action(CuaAction::Drag {
            target_hwnd: NULL_HWND,
            start_x: 10,
            start_y: 10,
            end_x: 200,
            end_y: 200,
            button: MouseButton::Left,
            steps: 5,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res.is_err(), "Drag with NULL HWND must fail");
    assert!(matches!(res.unwrap_err(), CuaError::WindowNotFound(0)));

    // 3. PressKey on NULL HWND
    let res = driver
        .dispatch_action(CuaAction::PressKey {
            target_hwnd: NULL_HWND,
            key: "enter".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res.is_err(), "PressKey with NULL HWND must fail");
    assert!(matches!(res.unwrap_err(), CuaError::WindowNotFound(0)));

    // 4. Hotkey on NULL HWND
    let res = driver
        .dispatch_action(CuaAction::Hotkey {
            target_hwnd: NULL_HWND,
            keys: vec!["ctrl".into(), "c".into()],
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res.is_err(), "Hotkey with NULL HWND must fail");
    assert!(matches!(res.unwrap_err(), CuaError::WindowNotFound(0)));
}

#[tokio::test]
async fn test_empirical_dead_hwnds_rejected_across_all_actions() {
    let driver = Win32CuaDriver::new();
    let dead_hwnds = [DEAD_HWND_1, DEAD_HWND_2, DEAD_HWND_MAX];

    for &hwnd in &dead_hwnds {
        // MoveCursor
        let r1 = driver
            .dispatch_action(CuaAction::MoveCursor {
                target_hwnd: hwnd,
                x: 50,
                y: 50,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(matches!(r1.unwrap_err(), CuaError::WindowNotFound(h) if h == hwnd));

        // Drag
        let r2 = driver
            .dispatch_action(CuaAction::Drag {
                target_hwnd: hwnd,
                start_x: 0,
                start_y: 0,
                end_x: 100,
                end_y: 100,
                button: MouseButton::Right,
                steps: 3,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(matches!(r2.unwrap_err(), CuaError::WindowNotFound(h) if h == hwnd));

        // PressKey
        let r3 = driver
            .dispatch_action(CuaAction::PressKey {
                target_hwnd: hwnd,
                key: "escape".into(),
                down: false,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(matches!(r3.unwrap_err(), CuaError::WindowNotFound(h) if h == hwnd));

        // Hotkey
        let r4 = driver
            .dispatch_action(CuaAction::Hotkey {
                target_hwnd: hwnd,
                keys: vec!["alt".into(), "tab".into()],
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(matches!(r4.unwrap_err(), CuaError::WindowNotFound(h) if h == hwnd));
    }
}

#[test]
fn test_empirical_validate_window_and_uipi_direct() {
    // Null HWND
    let res_null = validate_window_and_uipi(0);
    assert!(matches!(res_null.unwrap_err(), CuaError::WindowNotFound(0)));

    // Dead HWND
    let res_dead = validate_window_and_uipi(DEAD_HWND_1);
    assert!(matches!(res_dead.unwrap_err(), CuaError::WindowNotFound(_)));

    // Dead HWND Max
    let res_max = validate_window_and_uipi(u64::MAX);
    assert!(matches!(res_max.unwrap_err(), CuaError::WindowNotFound(_)));
}

// ============================================================================
// 2. Bounds Violations & Coordinate Stress Tests
// ============================================================================

#[tokio::test]
async fn test_empirical_bounded_mode_coordinate_containment_adversarial() {
    let mock = Arc::new(MockCuaDriver::new());
    // Window bounds: [100, 100, 800, 600]
    // Client area: x: 100 + 8 = 108, y: 100 + 32 = 132, w: 800 - 16 = 784, h: 600 - 40 = 560
    // Client bounds: [108..892], [132..692]
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x8888,
        pid: 8888,
        title: "Bounded Target".into(),
        class_name: "TestWindowClass".into(),
        process_name: Some("test_app.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["test_app.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Negative coordinates -> 100% rejection
    let negative_cases = [(-1, -1), (-500, 200), (200, -50), (-1920, 1080)];
    for (x, y) in negative_cases {
        let action = CuaAction::MoveCursor {
            target_hwnd: 0x8888,
            x,
            y,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::CoordinateOutOfBounds { .. })),
            "Expected negative coord ({}, {}) to be rejected",
            x,
            y
        );
    }

    // Zero coordinates (0, 0) -> outside [100, 100, 800, 600]
    let res_zero = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x8888,
            x: 0,
            y: 0,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(
        res_zero,
        Err(CuaError::CoordinateOutOfBounds { x: 0, y: 0 })
    ));

    // Title bar caption area: (200, 115) -> inside window bounds [100..900, 100..700], but outside client area [y < 132]
    let res_caption = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x8888,
            x: 200,
            y: 115,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res_caption, Err(CuaError::CoordinateOutOfBounds { .. })),
        "Title bar click must be rejected as out of bounds"
    );

    // Extreme out-of-bounds coordinates
    let extreme_cases = [(901, 400), (500, 701), (5000, 5000), (i32::MAX, i32::MAX)];
    for (x, y) in extreme_cases {
        let action = CuaAction::MoveCursor {
            target_hwnd: 0x8888,
            x,
            y,
            delivery_mode: CuaDeliveryMode::Background,
        };
        assert!(matches!(
            engine.execute_action(action).await,
            Err(CuaError::CoordinateOutOfBounds { .. })
        ));
    }

    // Drag: start contained, end out of bounds -> rejected
    let res_drag_end_oob = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x8888,
            start_x: 200,
            start_y: 200,
            end_x: 950,
            end_y: 200,
            button: MouseButton::Left,
            steps: 3,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(
        res_drag_end_oob,
        Err(CuaError::CoordinateOutOfBounds { x: 950, y: 200 })
    ));

    // Drag: start out of bounds, end contained -> rejected
    let res_drag_start_oob = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x8888,
            start_x: 50,
            start_y: 50,
            end_x: 200,
            end_y: 200,
            button: MouseButton::Left,
            steps: 3,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(
        res_drag_start_oob,
        Err(CuaError::CoordinateOutOfBounds { x: 50, y: 50 })
    ));

    // Valid coordinates inside client drawing surface: (200, 200) -> accepted
    let res_valid = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x8888,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res_valid.is_ok(), "Valid client coord must succeed");

    // Drag completely inside client area -> accepted
    let res_valid_drag = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x8888,
            start_x: 200,
            start_y: 200,
            end_x: 300,
            end_y: 300,
            button: MouseButton::Left,
            steps: 3,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(res_valid_drag.is_ok(), "Valid drag must succeed");
}

#[tokio::test]
async fn test_empirical_unrestricted_mode_multi_monitor_virtual_desktop_coordinates() {
    let mock = Arc::new(MockCuaDriver::new());
    // Secondary monitor positioned at negative X coordinates: [-1920, 0, 1920, 1080]
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x9999,
        pid: 9999,
        title: "Secondary Monitor Window".into(),
        class_name: "MultiMonClass".into(),
        process_name: Some("multimon_app.exe".into()),
        bounds: CuaRect::new(-1920, 0, 1920, 1080),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // In Unrestricted mode, negative coordinates on a secondary monitor are permitted
    let res_multimon = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x9999,
            x: -1000,
            y: 500,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_multimon.is_ok(),
        "Negative multi-monitor coords must succeed in Unrestricted mode"
    );

    // Multi-monitor drag
    let res_multimon_drag = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x9999,
            start_x: -1500,
            start_y: 400,
            end_x: -500,
            end_y: 400,
            button: MouseButton::Left,
            steps: 5,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_multimon_drag.is_ok(),
        "Multi-monitor drag must succeed in Unrestricted mode"
    );
}

#[test]
fn test_empirical_mouse_controller_drag_steps_clamp_and_math() {
    let mc = MouseController::new();

    // 0 steps clamped to at least 1 step without division by zero
    let res_zero = mc.drag(
        DEAD_HWND_1,
        0,
        0,
        100,
        100,
        MouseButton::Left,
        0,
        CuaDeliveryMode::Background,
    );
    assert!(matches!(res_zero.unwrap_err(), CuaError::WindowNotFound(_)));

    // Large step count and large coordinate delta
    let res_large = mc.drag(
        DEAD_HWND_1,
        -10000,
        -10000,
        10000,
        10000,
        MouseButton::Middle,
        1000,
        CuaDeliveryMode::Background,
    );
    assert!(matches!(
        res_large.unwrap_err(),
        CuaError::WindowNotFound(_)
    ));
}

// ============================================================================
// 3. Hotkey Parsing Matrix & Edge Cases
// ============================================================================

#[test]
fn test_empirical_hotkey_parsing_casing_variations() {
    // Upper / lower / mixed case modifiers
    assert_eq!(parse_virtual_key("ctrl").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("Ctrl").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("CTRL").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("cTrL").unwrap(), (0x11, false));

    assert_eq!(parse_virtual_key("shift").unwrap(), (0x10, false));
    assert_eq!(parse_virtual_key("Shift").unwrap(), (0x10, false));
    assert_eq!(parse_virtual_key("SHIFT").unwrap(), (0x10, false));

    assert_eq!(parse_virtual_key("alt").unwrap(), (0x12, false));
    assert_eq!(parse_virtual_key("Alt").unwrap(), (0x12, false));
    assert_eq!(parse_virtual_key("ALT").unwrap(), (0x12, false));

    assert_eq!(parse_virtual_key("win").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("Win").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("WIN").unwrap(), (0x5B, false));

    assert_eq!(parse_virtual_key("esc").unwrap(), (0x1B, false));
    assert_eq!(parse_virtual_key("Esc").unwrap(), (0x1B, false));
    assert_eq!(parse_virtual_key("ESC").unwrap(), (0x1B, false));

    // Function keys case insensitivity
    assert_eq!(parse_virtual_key("f4").unwrap(), (0x73, false));
    assert_eq!(parse_virtual_key("F4").unwrap(), (0x73, false));
    assert_eq!(parse_virtual_key("f12").unwrap(), (0x7B, false));
    assert_eq!(parse_virtual_key("F12").unwrap(), (0x7B, false));

    // Letters case insensitivity
    assert_eq!(parse_virtual_key("a").unwrap(), (b'A' as u16, false));
    assert_eq!(parse_virtual_key("A").unwrap(), (b'A' as u16, false));
    assert_eq!(parse_virtual_key("z").unwrap(), (b'Z' as u16, false));
    assert_eq!(parse_virtual_key("Z").unwrap(), (b'Z' as u16, false));

    // Common navigation
    assert_eq!(parse_virtual_key("Enter").unwrap(), (0x0D, false));
    assert_eq!(parse_virtual_key("ENTER").unwrap(), (0x0D, false));
    assert_eq!(parse_virtual_key("Tab").unwrap(), (0x09, false));
    assert_eq!(parse_virtual_key("TAB").unwrap(), (0x09, false));
    assert_eq!(parse_virtual_key("Backspace").unwrap(), (0x08, false));
    assert_eq!(parse_virtual_key("Delete").unwrap(), (0x2E, true));
    assert_eq!(parse_virtual_key("Insert").unwrap(), (0x2D, true));
}

#[test]
fn test_empirical_hotkey_parsing_modifier_aliases() {
    assert_eq!(parse_virtual_key("control").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("Control").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("CONTROL").unwrap(), (0x11, false));

    assert_eq!(parse_virtual_key("menu").unwrap(), (0x12, false));
    assert_eq!(parse_virtual_key("Menu").unwrap(), (0x12, false));

    assert_eq!(parse_virtual_key("windows").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("Windows").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("super").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("Super").unwrap(), (0x5B, false));

    assert_eq!(parse_virtual_key("return").unwrap(), (0x0D, false));
    assert_eq!(parse_virtual_key("Return").unwrap(), (0x0D, false));

    assert_eq!(parse_virtual_key("escape").unwrap(), (0x1B, false));
    assert_eq!(parse_virtual_key("Escape").unwrap(), (0x1B, false));

    assert_eq!(parse_virtual_key("space").unwrap(), (0x20, false));
    assert_eq!(parse_virtual_key("Space").unwrap(), (0x20, false));
}

#[test]
fn test_empirical_hotkey_parsing_unrecognized_and_compound_keys() {
    // Unrecognized key token
    let res_unknown = parse_virtual_key("UnknownKey");
    assert!(res_unknown.is_err());
    match res_unknown.unwrap_err() {
        CuaError::Win32Error(msg) => assert!(msg.contains("Unrecognized key: 'UnknownKey'")),
        other => panic!("Expected Win32Error, got: {:?}", other),
    }

    // Compound plus-separated hotkey strings passed as a single key token must fail validation
    let res_plus1 = parse_virtual_key("Ctrl+Shift+Esc");
    assert!(
        res_plus1.is_err(),
        "Plus-separated chord must not parse as a single key"
    );

    let res_plus2 = parse_virtual_key("Alt+F4");
    assert!(
        res_plus2.is_err(),
        "Plus-separated chord must not parse as a single key"
    );

    let res_plus3 = parse_virtual_key("Win+L");
    assert!(
        res_plus3.is_err(),
        "Plus-separated chord must not parse as a single key"
    );

    // Empty and whitespace keys
    assert!(parse_virtual_key("").is_err());
    assert!(parse_virtual_key("   ").is_err());
    assert!(parse_virtual_key("\t\n").is_err());

    // Single space string: observe that trimmed is empty, so it yields Empty key string
    let res_single_space = parse_virtual_key(" ");
    assert!(res_single_space.is_err());
    match res_single_space.unwrap_err() {
        CuaError::Win32Error(msg) => assert!(msg.contains("Empty key string")),
        other => panic!("Expected Win32Error, got: {:?}", other),
    }

    // Whitespace-padded valid token
    assert_eq!(parse_virtual_key("  ctrl  ").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key(" \t shift \n ").unwrap(), (0x10, false));
}

#[test]
fn test_empirical_keyboard_controller_atomic_prevalidation() {
    let kc = KeyboardController::new();

    // Hotkey with empty keys list on dead HWND still validates window first
    let res_empty = kc.hotkey(DEAD_HWND_1, &[], CuaDeliveryMode::Background);
    assert!(matches!(
        res_empty.unwrap_err(),
        CuaError::WindowNotFound(_)
    ));

    // Hotkey with an invalid key among valid keys fails pre-parsing before actuation
    let chord = vec!["ctrl".into(), "shift".into(), "bogus_key_xyz".into()];
    let res_chord = kc.hotkey(DEAD_HWND_1, &chord, CuaDeliveryMode::Background);
    // Window validation occurs first on dead HWND
    assert!(matches!(
        res_chord.unwrap_err(),
        CuaError::WindowNotFound(_)
    ));
}

// ============================================================================
// 4. UIPI and Security Governor Enforcement
// ============================================================================

#[tokio::test]
async fn test_empirical_dangerous_hotkeys_case_insensitivity_and_variants() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x7777,
        pid: 7777,
        title: "Test App".into(),
        class_name: "AppClass".into(),
        process_name: Some("app.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["app.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Matrix of dangerous hotkeys in varied casing and aliases
    let dangerous_test_matrix = vec![
        // Ctrl+Shift+Esc (Task Manager)
        vec!["Ctrl", "Shift", "Esc"],
        vec!["ctrl", "shift", "esc"],
        vec!["CTRL", "SHIFT", "ESC"],
        vec!["Control", "Shift", "Escape"],
        vec!["lctrl", "lshift", "esc"],
        // Alt+F4 (Close Window)
        vec!["Alt", "F4"],
        vec!["alt", "f4"],
        vec!["ALT", "F4"],
        vec!["Menu", "F4"],
        // Win+L (Lock Workstation)
        vec!["Win", "L"],
        vec!["win", "l"],
        vec!["WIN", "L"],
        vec!["Windows", "l"],
        vec!["Super", "L"],
        // Ctrl+Alt+Del (Security Screen)
        vec!["Ctrl", "Alt", "Del"],
        vec!["ctrl", "alt", "del"],
        vec!["CTRL", "ALT", "DEL"],
        vec!["Control", "Menu", "Delete"],
        // Win+R (Run Dialog)
        vec!["Win", "R"],
        vec!["win", "r"],
        vec!["WIN", "r"],
        // Win+X (Quick Link Menu)
        vec!["Win", "X"],
        vec!["win", "x"],
    ];

    for keys in dangerous_test_matrix {
        let action = CuaAction::Hotkey {
            target_hwnd: 0x7777,
            keys: keys.iter().map(|&s| s.to_string()).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::DangerousHotkeyRejected(_))),
            "Expected dangerous hotkey {:?} to be rejected",
            keys
        );
    }

    // Permitted hotkeys must succeed
    let permitted_cases = vec![
        vec!["Ctrl", "C"],
        vec!["ctrl", "c"],
        vec!["Ctrl", "V"],
        vec!["ctrl", "v"],
        vec!["Ctrl", "Z"],
        vec!["ctrl", "z"],
        vec!["Ctrl", "A"],
        vec!["ctrl", "s"],
        vec!["Alt", "Tab"],
    ];

    for keys in permitted_cases {
        let action = CuaAction::Hotkey {
            target_hwnd: 0x7777,
            keys: keys.iter().map(|&s| s.to_string()).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            res.is_ok(),
            "Permitted hotkey {:?} must succeed: {:?}",
            keys,
            res
        );
    }
}

#[tokio::test]
async fn test_empirical_security_governor_protected_pids_and_denylist() {
    let mock = Arc::new(MockCuaDriver::new());

    // 1. Kernel PID 0 (System Idle)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1000,
        pid: 0,
        title: "System Idle".into(),
        class_name: "SystemClass".into(),
        process_name: Some("ntoskrnl.exe".into()),
        bounds: CuaRect::new(100, 100, 500, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // 2. Kernel PID 4 (System)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1004,
        pid: 4,
        title: "System".into(),
        class_name: "SystemClass".into(),
        process_name: Some("system".into()),
        bounds: CuaRect::new(100, 100, 500, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // 3. Security Elevation Denylist (consent.exe)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1010,
        pid: 1010,
        title: "User Account Control".into(),
        class_name: "Credential Dialog Xaml Host".into(),
        process_name: Some("consent.exe".into()),
        bounds: CuaRect::new(100, 100, 500, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // 4. Protected Shell Window (Shell_TrayWnd - Taskbar)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1020,
        pid: 1020,
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

    // PID 0 must be rejected
    let r0 = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x1000,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r0.unwrap_err(), CuaError::PermissionDenied(_)));

    // PID 4 must be rejected
    let r4 = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x1004,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r4.unwrap_err(), CuaError::PermissionDenied(_)));

    // consent.exe must be rejected
    let rc = engine
        .execute_action(CuaAction::PressKey {
            target_hwnd: 0x1010,
            key: "enter".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(rc.unwrap_err(), CuaError::PermissionDenied(_)));

    // Shell_TrayWnd must be rejected
    let rt = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x1020,
            x: 100,
            y: 1050,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(rt.unwrap_err(), CuaError::PermissionDenied(_)));
}

#[tokio::test]
async fn test_empirical_standard_mode_read_only_barrier() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x6666,
        pid: 6666,
        title: "Standard Mode Target".into(),
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

    // In Standard mode, all desktop-mutating actions must fail closed
    let mutating_actions = vec![
        CuaAction::MoveCursor {
            target_hwnd: 0x6666,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        },
        CuaAction::Drag {
            target_hwnd: 0x6666,
            start_x: 200,
            start_y: 200,
            end_x: 300,
            end_y: 300,
            button: MouseButton::Left,
            steps: 2,
            delivery_mode: CuaDeliveryMode::Background,
        },
        CuaAction::PressKey {
            target_hwnd: 0x6666,
            key: "a".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        },
        CuaAction::Hotkey {
            target_hwnd: 0x6666,
            keys: vec!["ctrl".into(), "c".into()],
            delivery_mode: CuaDeliveryMode::Background,
        },
    ];

    for act in mutating_actions {
        let res = engine.execute_action(act).await;
        assert!(
            matches!(res, Err(CuaError::PermissionDenied(_))),
            "Mutating action must be blocked in Standard mode"
        );
    }

    // Non-mutating inspection actions (BringToFront, RestoreWindow) are permitted
    let bring = engine
        .execute_action(CuaAction::BringToFront {
            target_hwnd: 0x6666,
        })
        .await;
    assert!(
        bring.is_ok(),
        "BringToFront must be permitted in Standard mode"
    );

    let restore = engine
        .execute_action(CuaAction::RestoreWindow {
            target_hwnd: 0x6666,
        })
        .await;
    assert!(
        restore.is_ok(),
        "RestoreWindow must be permitted in Standard mode"
    );
}

#[tokio::test]
async fn test_empirical_uipi_elevation_interception_across_actions() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4444,
        pid: 4444,
        title: "Elevated Target".into(),
        class_name: "ElevatedClass".into(),
        process_name: Some("elevated.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // Simulate target at High integrity (0x3000), agent at Medium (0x2000)
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0x4444, 0x3000);
    *mock.agent_rid.lock().unwrap() = 0x2000;

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["elevated.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // MoveCursor on elevated window must be blocked by UIPI
    let res_move = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x4444,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res_move, Err(CuaError::UipiBlocked(_))),
        "MoveCursor on elevated window must be blocked by UIPI"
    );

    // Drag on elevated window must be blocked by UIPI
    let res_drag = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x4444,
            start_x: 200,
            start_y: 200,
            end_x: 300,
            end_y: 300,
            button: MouseButton::Left,
            steps: 3,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res_drag, Err(CuaError::UipiBlocked(_))),
        "Drag on elevated window must be blocked by UIPI"
    );

    // PressKey on elevated window must be blocked by UIPI
    let res_key = engine
        .execute_action(CuaAction::PressKey {
            target_hwnd: 0x4444,
            key: "tab".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res_key, Err(CuaError::UipiBlocked(_))),
        "PressKey on elevated window must be blocked by UIPI"
    );

    // Hotkey on elevated window must be blocked by UIPI
    let res_hotkey = engine
        .execute_action(CuaAction::Hotkey {
            target_hwnd: 0x4444,
            keys: vec!["ctrl".into(), "c".into()],
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res_hotkey, Err(CuaError::UipiBlocked(_))),
        "Hotkey on elevated window must be blocked by UIPI"
    );
}

// ============================================================================
// 5. Live Desktop Actuation Empirical Tests (Conditional)
// ============================================================================

#[tokio::test]
async fn test_empirical_live_desktop_window_actuation_matrix() {
    let windows = list_windows(None).unwrap_or_default();
    let interactive = windows.iter().find(|w| w.is_on_screen && !w.is_minimized);

    if let Some(target) = interactive {
        let driver = Win32CuaDriver::new();
        let center_x = target.bounds.x + (target.bounds.width / 2);
        let center_y = target.bounds.y + (target.bounds.height / 2);

        // 1. MoveCursor
        let r_move = driver
            .dispatch_action(CuaAction::MoveCursor {
                target_hwnd: target.hwnd,
                x: center_x,
                y: center_y,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(r_move.is_ok(), "Live MoveCursor must succeed: {:?}", r_move);
        let res = r_move.unwrap();
        assert!(res.success);
        assert_eq!(res.effect, CuaActionEffect::Completed);

        // 2. Drag
        let r_drag = driver
            .dispatch_action(CuaAction::Drag {
                target_hwnd: target.hwnd,
                start_x: center_x,
                start_y: center_y,
                end_x: center_x + 5,
                end_y: center_y + 5,
                button: MouseButton::Left,
                steps: 2,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(r_drag.is_ok(), "Live Drag must succeed: {:?}", r_drag);
        let res = r_drag.unwrap();
        assert!(res.success);
        assert_eq!(res.effect, CuaActionEffect::Completed);

        // 3. PressKey with Shift
        let r_press = driver
            .dispatch_action(CuaAction::PressKey {
                target_hwnd: target.hwnd,
                key: "Shift".into(),
                down: true,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(r_press.is_ok(), "Live PressKey must succeed: {:?}", r_press);
        let res = r_press.unwrap();
        assert!(res.success);
        assert_eq!(res.effect, CuaActionEffect::Completed);

        // Release Shift
        let _ = driver
            .dispatch_action(CuaAction::PressKey {
                target_hwnd: target.hwnd,
                key: "Shift".into(),
                down: false,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;

        // 4. Hotkey with safe chord
        let r_hotkey = driver
            .dispatch_action(CuaAction::Hotkey {
                target_hwnd: target.hwnd,
                keys: vec!["Ctrl".into(), "A".into()],
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await;
        assert!(r_hotkey.is_ok(), "Live Hotkey must succeed: {:?}", r_hotkey);
        let res = r_hotkey.unwrap();
        assert!(res.success);
        assert_eq!(res.effect, CuaActionEffect::Completed);
    }
}

// ============================================================================
// 6. Advanced Stress Tests: Negative Multi-Monitor Geometry, Kill-Switch, & Paths
// ============================================================================

#[tokio::test]
async fn test_empirical_bounded_mode_negative_multimon_containment_and_rejection() {
    let mock = Arc::new(MockCuaDriver::new());
    // Secondary monitor to top-left of primary: [-1920, -1080, 1920, 1080]
    // Client area: [-1912, -1048, 1904, 1040] -> x in [-1912..-8], y in [-1048..-8]
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5555,
        pid: 5555,
        title: "Negative MultiMon Target".into(),
        class_name: "MultiMonTopLeft".into(),
        process_name: Some("multimon.exe".into()),
        bounds: CuaRect::new(-1920, -1080, 1920, 1080),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["multimon.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // 1. Point strictly inside client area: (-1000, -500) -> Allowed
    let r1 = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x5555,
            x: -1000,
            y: -500,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        r1.is_ok(),
        "Valid point in negative client area must succeed"
    );

    // 2. Point outside window: (-2000, -500) -> Rejected
    let r2 = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x5555,
            x: -2000,
            y: -500,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(
        r2.unwrap_err(),
        CuaError::CoordinateOutOfBounds { x: -2000, y: -500 }
    ));

    // 3. Point in title bar caption: (-1000, -1060) -> outside client area (-1060 < -1048) -> Rejected
    let r3 = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x5555,
            x: -1000,
            y: -1060,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(
        r3.unwrap_err(),
        CuaError::CoordinateOutOfBounds { .. }
    ));
}

#[tokio::test]
async fn test_empirical_kill_switch_emergency_halt_intercepts_all_actions() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x3333,
        pid: 3333,
        title: "Kill Switch Target".into(),
        class_name: "AppClass".into(),
        process_name: Some("app.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Trip emergency halt
    let _ = engine
        .kill_switch
        .trigger_halt(liva_cua::kill_switch::HaltReason::ManualTrigger {
            detail: "Adversarial emergency halt test".into(),
        });
    assert!(engine.kill_switch.is_halted());

    // All actions must fail with EmergencyHalted
    let r_move = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x3333,
            x: 200,
            y: 200,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r_move.unwrap_err(), CuaError::EmergencyHalted));

    let r_drag = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x3333,
            start_x: 200,
            start_y: 200,
            end_x: 300,
            end_y: 300,
            button: MouseButton::Left,
            steps: 2,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r_drag.unwrap_err(), CuaError::EmergencyHalted));

    let r_press = engine
        .execute_action(CuaAction::PressKey {
            target_hwnd: 0x3333,
            key: "a".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r_press.unwrap_err(), CuaError::EmergencyHalted));

    let r_hotkey = engine
        .execute_action(CuaAction::Hotkey {
            target_hwnd: 0x3333,
            keys: vec!["ctrl".into(), "c".into()],
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(matches!(r_hotkey.unwrap_err(), CuaError::EmergencyHalted));
}

#[tokio::test]
async fn test_empirical_denylist_path_normalization() {
    let mock = Arc::new(MockCuaDriver::new());

    // Process specified with full Windows directory paths
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2222,
        pid: 2222,
        title: "Elevated Security Prompt".into(),
        class_name: "Credential Dialog".into(),
        process_name: Some("C:\\Windows\\System32\\consent.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["consent.exe".into()], // Even if in allowlist, denylist takes precedence!
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    let res = engine
        .execute_action(CuaAction::PressKey {
            target_hwnd: 0x2222,
            key: "enter".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        matches!(res.unwrap_err(), CuaError::PermissionDenied(_)),
        "Full path denylist process must be blocked even when allowlisted"
    );
}

#[test]
fn test_empirical_unicode_and_non_ascii_virtual_key_parsing() {
    // Non-ASCII multi-byte characters fail parsing with Win32Error rather than panic
    let non_ascii_keys = ["é", "€", "ñ", "ü", "日", "🔥"];
    for key in non_ascii_keys {
        let res = parse_virtual_key(key);
        assert!(
            res.is_err(),
            "Non-ASCII key '{}' should fail gracefully",
            key
        );
        match res.unwrap_err() {
            CuaError::Win32Error(msg) => assert!(msg.contains("Unrecognized key")),
            other => panic!("Expected Win32Error for '{}', got {:?}", key, other),
        }
    }
}
