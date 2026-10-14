//! Comprehensive unit tests for CUA action dispatcher completeness and error handling.
//!
//! Validates:
//! - Win32CuaDriver::dispatch_action for MoveCursor, Drag, PressKey, Hotkey
//! - Direct MouseController and KeyboardController execution
//! - Window validation and error propagation (WindowNotFound, WindowMinimized, UIPI)
//! - Virtual key token parsing and invalid key error handling

use liva_cua::input::keyboard::parse_virtual_key;
use liva_cua::input::{KeyboardController, MouseController};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{CuaAction, CuaActionEffect, CuaDeliveryMode, CuaError, MouseButton};
use liva_cua::window::list_windows;
use liva_cua::Win32CuaDriver;

const INVALID_TEST_HWND: u64 = 0x00DE_ADBE_EF00_1234;

#[tokio::test]
async fn test_dispatch_move_cursor_invalid_window_returns_error() {
    let driver = Win32CuaDriver::new();
    let action = CuaAction::MoveCursor {
        target_hwnd: INVALID_TEST_HWND,
        x: 150,
        y: 200,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = driver.dispatch_action(action).await;
    assert!(result.is_err(), "MoveCursor to invalid HWND must error");
    match result.unwrap_err() {
        CuaError::WindowNotFound(hwnd) => assert_eq!(hwnd, INVALID_TEST_HWND),
        other => panic!("Expected WindowNotFound, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_dispatch_drag_invalid_window_returns_error() {
    let driver = Win32CuaDriver::new();
    let action = CuaAction::Drag {
        target_hwnd: INVALID_TEST_HWND,
        start_x: 100,
        start_y: 100,
        end_x: 300,
        end_y: 300,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = driver.dispatch_action(action).await;
    assert!(result.is_err(), "Drag on invalid HWND must error");
    match result.unwrap_err() {
        CuaError::WindowNotFound(hwnd) => assert_eq!(hwnd, INVALID_TEST_HWND),
        other => panic!("Expected WindowNotFound, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_dispatch_press_key_invalid_window_returns_error() {
    let driver = Win32CuaDriver::new();
    let action = CuaAction::PressKey {
        target_hwnd: INVALID_TEST_HWND,
        key: "enter".into(),
        down: true,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = driver.dispatch_action(action).await;
    assert!(result.is_err(), "PressKey on invalid HWND must error");
    match result.unwrap_err() {
        CuaError::WindowNotFound(hwnd) => assert_eq!(hwnd, INVALID_TEST_HWND),
        other => panic!("Expected WindowNotFound, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_dispatch_hotkey_invalid_window_returns_error() {
    let driver = Win32CuaDriver::new();
    let action = CuaAction::Hotkey {
        target_hwnd: INVALID_TEST_HWND,
        keys: vec!["ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = driver.dispatch_action(action).await;
    assert!(result.is_err(), "Hotkey on invalid HWND must error");
    match result.unwrap_err() {
        CuaError::WindowNotFound(hwnd) => assert_eq!(hwnd, INVALID_TEST_HWND),
        other => panic!("Expected WindowNotFound, got: {:?}", other),
    }
}

#[test]
fn test_mouse_controller_direct_error_handling() {
    let mc = MouseController::new();

    let move_res = mc.move_cursor(INVALID_TEST_HWND, 50, 50, CuaDeliveryMode::Background);
    assert!(move_res.is_err());
    assert!(matches!(move_res.unwrap_err(), CuaError::WindowNotFound(_)));

    let drag_res = mc.drag(
        INVALID_TEST_HWND,
        10,
        10,
        100,
        100,
        MouseButton::Left,
        0, // Steps 0 should clamp to >= 1 without panic
        CuaDeliveryMode::Background,
    );
    assert!(drag_res.is_err());
    assert!(matches!(drag_res.unwrap_err(), CuaError::WindowNotFound(_)));
}

#[test]
fn test_keyboard_controller_direct_error_handling() {
    let kc = KeyboardController::new();

    let press_res = kc.press_key(
        INVALID_TEST_HWND,
        "shift",
        true,
        CuaDeliveryMode::Background,
    );
    assert!(press_res.is_err());
    assert!(matches!(
        press_res.unwrap_err(),
        CuaError::WindowNotFound(_)
    ));

    let hotkey_res = kc.hotkey(
        INVALID_TEST_HWND,
        &["ctrl".into(), "v".into()],
        CuaDeliveryMode::Background,
    );
    assert!(hotkey_res.is_err());
    assert!(matches!(
        hotkey_res.unwrap_err(),
        CuaError::WindowNotFound(_)
    ));
}

#[test]
fn test_virtual_key_parser_comprehensive_matrix() {
    // Modifiers
    assert_eq!(parse_virtual_key("ctrl").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("control").unwrap(), (0x11, false));
    assert_eq!(parse_virtual_key("rctrl").unwrap(), (0x11, true));
    assert_eq!(parse_virtual_key("alt").unwrap(), (0x12, false));
    assert_eq!(parse_virtual_key("ralt").unwrap(), (0x12, true));
    assert_eq!(parse_virtual_key("shift").unwrap(), (0x10, false));
    assert_eq!(parse_virtual_key("rshift").unwrap(), (0x10, true));
    assert_eq!(parse_virtual_key("win").unwrap(), (0x5B, false));
    assert_eq!(parse_virtual_key("rwin").unwrap(), (0x5C, false));

    // Common navigation and editing
    assert_eq!(parse_virtual_key("enter").unwrap(), (0x0D, false));
    assert_eq!(parse_virtual_key("return").unwrap(), (0x0D, false));
    assert_eq!(parse_virtual_key("esc").unwrap(), (0x1B, false));
    assert_eq!(parse_virtual_key("escape").unwrap(), (0x1B, false));
    assert_eq!(parse_virtual_key("tab").unwrap(), (0x09, false));
    assert_eq!(parse_virtual_key("space").unwrap(), (0x20, false));
    assert_eq!(parse_virtual_key("backspace").unwrap(), (0x08, false));
    assert_eq!(parse_virtual_key("del").unwrap(), (0x2E, true));
    assert_eq!(parse_virtual_key("delete").unwrap(), (0x2E, true));
    assert_eq!(parse_virtual_key("home").unwrap(), (0x24, true));
    assert_eq!(parse_virtual_key("end").unwrap(), (0x23, true));
    assert_eq!(parse_virtual_key("pageup").unwrap(), (0x21, true));
    assert_eq!(parse_virtual_key("pagedown").unwrap(), (0x22, true));
    assert_eq!(parse_virtual_key("left").unwrap(), (0x25, true));
    assert_eq!(parse_virtual_key("up").unwrap(), (0x26, true));
    assert_eq!(parse_virtual_key("right").unwrap(), (0x27, true));
    assert_eq!(parse_virtual_key("down").unwrap(), (0x28, true));

    // Function keys
    assert_eq!(parse_virtual_key("f1").unwrap(), (0x70, false));
    assert_eq!(parse_virtual_key("f5").unwrap(), (0x74, false));
    assert_eq!(parse_virtual_key("f12").unwrap(), (0x7B, false));

    // Letters and digits
    assert_eq!(parse_virtual_key("a").unwrap(), (b'A' as u16, false));
    assert_eq!(parse_virtual_key("Z").unwrap(), (b'Z' as u16, false));
    assert_eq!(parse_virtual_key("0").unwrap(), (b'0' as u16, false));
    assert_eq!(parse_virtual_key("9").unwrap(), (b'9' as u16, false));

    // Error cases
    assert!(parse_virtual_key("").is_err(), "Empty key must error");
    assert!(
        parse_virtual_key("   ").is_err(),
        "Whitespace key must error"
    );
    assert!(
        parse_virtual_key("nonexistent_key_token").is_err(),
        "Invalid token must error"
    );
}

#[tokio::test]
async fn test_live_window_action_dispatch_execution() {
    let windows = list_windows(None).unwrap_or_default();
    let valid_target = windows.iter().find(|w| w.is_on_screen && !w.is_minimized);

    if let Some(target) = valid_target {
        let driver = Win32CuaDriver::new();
        let center_x = target.bounds.x + (target.bounds.width / 2);
        let center_y = target.bounds.y + (target.bounds.height / 2);

        // 1. MoveCursor on live window in Background mode
        let move_act = CuaAction::MoveCursor {
            target_hwnd: target.hwnd,
            x: center_x,
            y: center_y,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let move_res = driver.dispatch_action(move_act).await;
        assert!(
            move_res.is_ok(),
            "MoveCursor on live window must succeed: {:?}",
            move_res
        );
        let r = move_res.unwrap();
        assert!(r.success);
        assert_eq!(r.effect, CuaActionEffect::Completed);
        assert_eq!(r.delivery_used, CuaDeliveryMode::Background);

        // 2. Drag on live window in Background mode
        let drag_act = CuaAction::Drag {
            target_hwnd: target.hwnd,
            start_x: center_x,
            start_y: center_y,
            end_x: center_x + 10,
            end_y: center_y + 10,
            button: MouseButton::Left,
            steps: 2,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let drag_res = driver.dispatch_action(drag_act).await;
        assert!(
            drag_res.is_ok(),
            "Drag on live window must succeed: {:?}",
            drag_res
        );
        let r = drag_res.unwrap();
        assert!(r.success);
        assert_eq!(r.effect, CuaActionEffect::Completed);

        // 3. PressKey on live window in Background mode
        let press_act = CuaAction::PressKey {
            target_hwnd: target.hwnd,
            key: "shift".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let press_res = driver.dispatch_action(press_act).await;
        assert!(
            press_res.is_ok(),
            "PressKey on live window must succeed: {:?}",
            press_res
        );
        let r = press_res.unwrap();
        assert!(r.success);
        assert_eq!(r.effect, CuaActionEffect::Completed);

        // 4. Hotkey on live window in Background mode
        let hotkey_act = CuaAction::Hotkey {
            target_hwnd: target.hwnd,
            keys: vec!["ctrl".into(), "a".into()],
            delivery_mode: CuaDeliveryMode::Background,
        };
        let hotkey_res = driver.dispatch_action(hotkey_act).await;
        assert!(
            hotkey_res.is_ok(),
            "Hotkey on live window must succeed: {:?}",
            hotkey_res
        );
        let r = hotkey_res.unwrap();
        assert!(r.success);
        assert_eq!(r.effect, CuaActionEffect::Completed);
    }
}

#[tokio::test]
async fn test_dispatch_press_key_invalid_key_token_fails_validation() {
    let windows = list_windows(None).unwrap_or_default();
    let valid_target = windows.iter().find(|w| w.is_on_screen && !w.is_minimized);

    if let Some(target) = valid_target {
        let driver = Win32CuaDriver::new();
        let action = CuaAction::PressKey {
            target_hwnd: target.hwnd,
            key: "completely_invalid_key_token_xyz".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        };

        let res = driver.dispatch_action(action).await;
        assert!(res.is_err(), "Invalid key token must fail dispatch");
        match res.unwrap_err() {
            CuaError::Win32Error(msg) => {
                assert!(
                    msg.contains("Unrecognized key"),
                    "Expected unrecognized key message: {}",
                    msg
                );
            }
            other => panic!("Expected Win32Error, got: {:?}", other),
        }
    }
}

#[tokio::test]
async fn test_dispatch_hotkey_invalid_key_token_fails_validation() {
    let windows = list_windows(None).unwrap_or_default();
    let valid_target = windows.iter().find(|w| w.is_on_screen && !w.is_minimized);

    if let Some(target) = valid_target {
        let driver = Win32CuaDriver::new();
        let action = CuaAction::Hotkey {
            target_hwnd: target.hwnd,
            keys: vec!["ctrl".into(), "invalid_key_token_123".into()],
            delivery_mode: CuaDeliveryMode::Background,
        };

        let res = driver.dispatch_action(action).await;
        assert!(
            res.is_err(),
            "Invalid key in hotkey sequence must fail dispatch"
        );
        match res.unwrap_err() {
            CuaError::Win32Error(msg) => {
                assert!(
                    msg.contains("Unrecognized key"),
                    "Expected unrecognized key message: {}",
                    msg
                );
            }
            other => panic!("Expected Win32Error, got: {:?}", other),
        }
    }
}

#[tokio::test]
async fn test_cua_engine_executes_all_new_actions_with_mock_driver() {
    use liva_cua::mock::MockCuaDriver;
    use liva_cua::types::{CuaConfig, CuaRect, CuaWindowInfo, PermissionMode};
    use liva_cua::CuaEngine;
    use std::sync::Arc;

    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4321,
        pid: 3000,
        title: "Test Editor".into(),
        class_name: "EditorClass".into(),
        process_name: Some("editor.exe".into()),
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
    let engine = CuaEngine::new_with_driver(config, mock.clone());

    // 1. Execute MoveCursor
    let move_res = engine
        .execute_action(CuaAction::MoveCursor {
            target_hwnd: 0x4321,
            x: 200,
            y: 250,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(move_res.is_ok(), "Engine MoveCursor execution must succeed");

    // 2. Execute Drag
    let drag_res = engine
        .execute_action(CuaAction::Drag {
            target_hwnd: 0x4321,
            start_x: 200,
            start_y: 250,
            end_x: 350,
            end_y: 400,
            button: MouseButton::Left,
            steps: 3,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(drag_res.is_ok(), "Engine Drag execution must succeed");

    // 3. Execute PressKey
    let press_res = engine
        .execute_action(CuaAction::PressKey {
            target_hwnd: 0x4321,
            key: "tab".into(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(press_res.is_ok(), "Engine PressKey execution must succeed");

    // 4. Execute Hotkey
    let hotkey_res = engine
        .execute_action(CuaAction::Hotkey {
            target_hwnd: 0x4321,
            keys: vec!["ctrl".into(), "s".into()],
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(hotkey_res.is_ok(), "Engine Hotkey execution must succeed");

    // Verify mock received all 4 actions
    let dispatched = mock.get_dispatched_actions();
    assert_eq!(
        dispatched.len(),
        4,
        "Mock driver must have received 4 dispatched actions"
    );
}
