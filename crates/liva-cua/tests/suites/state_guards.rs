//! State guards tests: MinimizedWindowGuard, ExactPidWindowTargetGuard, and UWP delegation.

use liva_cua::guards::{ExactPidWindowTargetGuard, MinimizedWindowGuard};
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;

#[test]
fn test_minimized_window_guard_direct() {
    let bounds_sentinel = CuaRect::new(-32000, -32000, 0, 0);
    assert!(MinimizedWindowGuard::validate(0x1, false, &bounds_sentinel).is_err());
    assert!(MinimizedWindowGuard::validate(0x1, true, &CuaRect::new(0, 0, 800, 600)).is_err());
    assert!(MinimizedWindowGuard::validate(0x1, false, &CuaRect::new(0, 0, 800, 600)).is_ok());
}

#[tokio::test]
async fn test_minimized_sentinel_refusal() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1234,
        pid: 1000,
        title: "Minimized Notepad".into(),
        class_name: "Notepad".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 0, 0), // Win32 minimized sentinel
        is_on_screen: false,
        is_minimized: true,
        z_index: 5,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    let action = CuaAction::Click {
        target_hwnd: 0x1234,
        x: Some(50),
        y: Some(50),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        CuaError::WindowMinimized(hwnd) => assert_eq!(hwnd, 0x1234),
        other => panic!("Expected WindowMinimized error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_degenerate_bounds_refusal() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5678,
        pid: 2000,
        title: "Zero-size Window".into(),
        class_name: "DummyClass".into(),
        process_name: Some("app.exe".into()),
        bounds: CuaRect::new(100, 100, 0, 0), // Empty degenerate bounds
        is_on_screen: true,
        is_minimized: false,
        z_index: 1,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    let action = CuaAction::Click {
        target_hwnd: 0x5678,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        CuaError::WindowMinimized(hwnd) => assert_eq!(hwnd, 0x5678),
        other => panic!(
            "Expected WindowMinimized error on degenerate bounds, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_exact_pid_guard_rejection() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x9999,
        pid: 4000, // True PID
        title: "Banking Client".into(),
        class_name: "FinanceApp".into(),
        process_name: Some("bank.exe".into()),
        bounds: CuaRect::new(100, 100, 500, 400),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    let action = CuaAction::Click {
        target_hwnd: 0x9999,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // Caller expected PID 1000, but window actually belongs to PID 4000
    let guard = ExactPidWindowTargetGuard::new(1000);
    let result = engine.execute_action_guarded(action, guard).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        CuaError::TargetPidMismatch {
            hwnd,
            expected_pid,
            actual_pid,
        } => {
            assert_eq!(hwnd, 0x9999);
            assert_eq!(expected_pid, 1000);
            assert_eq!(actual_pid, 4000);
        }
        other => panic!("Expected TargetPidMismatch error, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_exact_pid_guard_uwp_child_delegation() {
    let guard = ExactPidWindowTargetGuard::new(5555);

    // Frame PID is 1111 (ApplicationFrameHost.exe), but hosted UWP child PID is 5555
    let res = guard.validate(0xAAAA, 1111, Some("ApplicationFrameHost.exe"), Some(5555));
    assert!(res.is_ok(), "UWP hosted PID 5555 should match expected PID");

    // Frame PID is 1111, hosted child is 9999 (mismatch)
    let res_mismatch = guard.validate(0xAAAA, 1111, Some("ApplicationFrameHost.exe"), Some(9999));
    assert!(matches!(
        res_mismatch,
        Err(CuaError::TargetPidMismatch {
            expected_pid: 5555,
            actual_pid: 9999,
            ..
        })
    ));
}

#[tokio::test]
async fn test_exact_pid_guard_process_name_verification() {
    let guard = ExactPidWindowTargetGuard::new(1234).with_process_name("notepad.exe");

    // Exact match (case insensitive)
    assert!(guard.validate(0x1, 1234, Some("NOTEPAD.EXE"), None).is_ok());

    // Mismatched executable name
    assert!(matches!(
        guard.validate(0x1, 1234, Some("malicious.exe"), None),
        Err(CuaError::TargetPidMismatch { .. })
    ));
}
