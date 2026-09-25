//! Empirical Challenger 1 Adversarial Test Suite for Milestone M2.
//!
//! Subsystem: Security Governor, Permission Modes, Sandboxing, Coordinate Containment, Hotkeys & UIPI.
//!
//! Scope:
//! 1. Denylist Precedence: Bypass attempts by adding denylisted processes (taskmgr, regedit, 1password, csrss) to allowlist.
//! 2. Dangerous Hotkey Permutations: Alias variations, casing, and multi-key supersets (win+r, ctrl+shift+escape, alt+f4, ctrl+alt+del, win+l, win+x).
//! 3. Coordinate Containment Bypass: (x, 0), (x, 15) title bar, (width - 5, 5) close button [X], and negative coordinates.
//! 4. Standard Mode Read-Only Barrier: Rejecting all mutating actions fail-closed.
//! 5. UIPI Integrity: Simulated and hierarchy-based elevation rejection pre-execution.

use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;

// ============================================================================
// AREA 1: Denylist Precedence & Bypass Resistance
// ============================================================================

#[tokio::test]
async fn test_adversarial_denylist_bypass_attempt_all_catalogs() {
    let mock = Arc::new(MockCuaDriver::new());

    // Denylisted processes from Kernel, Admin Tools, and Password Vault catalogs
    let denylisted_apps = [
        (0x101, 101, "taskmgr.exe", "TaskManagerWindow"),
        (0x102, 102, "regedit.exe", "RegEdit_Class"),
        (0x103, 103, "1password.exe", "OnePassword_Class"),
        (0x104, 104, "csrss.exe", "Csrss_Class"),
        (0x105, 105, "bitwarden.exe", "Chrome_WidgetWin_1"),
        (0x106, 106, "keepass.exe", "WindowsForms10.Window.8.app"),
        (0x107, 107, "mmc.exe", "MMCMainFrame"),
    ];

    for &(hwnd, pid, proc, class) in &denylisted_apps {
        mock.insert_window(CuaWindowInfo {
            hwnd,
            pid,
            title: format!("Window for {}", proc),
            class_name: class.into(),
            process_name: Some(proc.into()),
            bounds: CuaRect::new(100, 100, 800, 600),
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        });
    }

    // Adversarial configuration: Attacker explicitly injects denylisted apps into allowlist
    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec![
            "taskmgr.exe".into(),
            "regedit.exe".into(),
            "1password.exe".into(),
            "csrss.exe".into(),
            "bitwarden.exe".into(),
            "keepass.exe".into(),
            "mmc.exe".into(),
        ],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    for &(hwnd, pid, proc, _) in &denylisted_apps {
        // Attempt Click
        let click = CuaAction::Click {
            target_hwnd: hwnd,
            x: Some(300),
            y: Some(300),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let click_res = engine.execute_action(click).await;
        assert!(
            matches!(click_res, Err(CuaError::PermissionDenied(_))),
            "Click on denylisted process '{}' (PID {}) was NOT rejected!",
            proc,
            pid
        );

        // Attempt TypeText
        let type_text = CuaAction::TypeText {
            target_hwnd: hwnd,
            text: "malicious_payload".into(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let type_res = engine.execute_action(type_text).await;
        assert!(
            matches!(type_res, Err(CuaError::PermissionDenied(_))),
            "TypeText on denylisted process '{}' (PID {}) was NOT rejected!",
            proc,
            pid
        );

        // Attempt MoveCursor
        let move_cursor = CuaAction::MoveCursor {
            target_hwnd: hwnd,
            x: 300,
            y: 300,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let move_res = engine.execute_action(move_cursor).await;
        assert!(
            matches!(move_res, Err(CuaError::PermissionDenied(_))),
            "MoveCursor on denylisted process '{}' (PID {}) was NOT rejected!",
            proc,
            pid
        );

        // Attempt Drag
        let drag = CuaAction::Drag {
            target_hwnd: hwnd,
            start_x: 200,
            start_y: 200,
            end_x: 400,
            end_y: 400,
            button: MouseButton::Left,
            steps: 5,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let drag_res = engine.execute_action(drag).await;
        assert!(
            matches!(drag_res, Err(CuaError::PermissionDenied(_))),
            "Drag on denylisted process '{}' (PID {}) was NOT rejected!",
            proc,
            pid
        );
    }
}

#[tokio::test]
async fn test_adversarial_denylist_casing_and_path_variations() {
    let mock = Arc::new(MockCuaDriver::new());

    let test_cases = [
        (0x201, 201, "TASKMGR.EXE"),
        (0x202, 202, "RegEdit.exe"),
        (0x203, 203, "1Password.EXE"),
        (0x204, 204, "Csrss.EXE"),
        (0x205, 205, r"C:\Windows\System32\taskmgr.exe"),
        (0x206, 206, "C:/Windows/System32/regedit.exe"),
        (0x207, 207, r"C:\Program Files\1Password\app\1password.exe"),
        (0x208, 208, r"\SystemRoot\System32\csrss.exe"),
    ];

    for &(hwnd, pid, proc) in &test_cases {
        mock.insert_window(CuaWindowInfo {
            hwnd,
            pid,
            title: "Test Casing Window".into(),
            class_name: "AppClass".into(),
            process_name: Some(proc.into()),
            bounds: CuaRect::new(100, 100, 800, 600),
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        });
    }

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec![
            "TASKMGR.EXE".into(),
            "regedit.exe".into(),
            "1Password.EXE".into(),
            "csrss.exe".into(),
        ],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    for &(hwnd, pid, proc) in &test_cases {
        let action = CuaAction::Click {
            target_hwnd: hwnd,
            x: Some(300),
            y: Some(300),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::PermissionDenied(_))),
            "Process variation '{}' (PID {}) was not rejected by denylist!",
            proc,
            pid
        );
    }
}

#[tokio::test]
async fn test_adversarial_kernel_denylist_unrestricted_mode_bypass() {
    let mock = Arc::new(MockCuaDriver::new());

    let kernel_targets = [
        (0x301, 0, "System Idle Process (PID 0)"),
        (0x302, 4, "System Kernel (PID 4)"),
        (0x303, 50, "csrss.exe"),
        (0x304, 51, "lsass.exe"),
        (0x305, 52, "services.exe"),
        (0x306, 53, "dwm.exe"),
        (0x307, 54, "smss.exe"),
        (0x308, 55, "liva-native-core.exe"),
    ];

    for &(hwnd, pid, name) in &kernel_targets {
        mock.insert_window(CuaWindowInfo {
            hwnd,
            pid,
            title: name.into(),
            class_name: "KernelClass".into(),
            process_name: Some(name.into()),
            bounds: CuaRect::new(100, 100, 800, 600),
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        });
    }

    // Set permission mode to UNRESTRICTED
    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    for &(hwnd, pid, name) in &kernel_targets {
        let click = CuaAction::Click {
            target_hwnd: hwnd,
            x: Some(200),
            y: Some(200),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(click).await;
        assert!(
            matches!(res, Err(CuaError::PermissionDenied(_))),
            "Kernel process / PID invariant '{}' (PID {}) was NOT blocked in Unrestricted mode!",
            name,
            pid
        );
    }
}

#[tokio::test]
async fn test_adversarial_shell_classes_and_uac_dialog_bypass() {
    let mock = Arc::new(MockCuaDriver::new());

    // Shell classes
    let shell_targets = [
        (0x401, "Shell_TrayWnd", "Primary Taskbar"),
        (0x402, "Shell_SecondaryTrayWnd", "Secondary Taskbar"),
        (0x403, "Progman", "Desktop Program Manager"),
        (0x404, "WorkerW", "Wallpaper Host"),
    ];

    for &(hwnd, class, title) in &shell_targets {
        mock.insert_window(CuaWindowInfo {
            hwnd,
            pid: 1000,
            title: title.into(),
            class_name: class.into(),
            process_name: Some("explorer.exe".into()),
            bounds: CuaRect::new(0, 0, 1920, 1080),
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        });
    }

    // Dialogs with security warning captions
    let security_dialogs = [
        (0x411, "User Account Control", "#32770"),
        (0x412, "Windows Security Alert", "#32770"),
        (0x413, "Important Security Warning", "#32770"),
        (0x414, "Windows Defender SmartScreen", "#32770"),
    ];

    for &(hwnd, title, class) in &security_dialogs {
        mock.insert_window(CuaWindowInfo {
            hwnd,
            pid: 1001,
            title: title.into(),
            class_name: class.into(),
            process_name: Some("consent.exe".into()),
            bounds: CuaRect::new(200, 200, 600, 400),
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        });
    }

    // Allowlist includes explorer.exe and consent.exe
    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["explorer.exe".into(), "consent.exe".into()],
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Verify all shell classes are rejected
    for &(hwnd, class, _) in &shell_targets {
        let action = CuaAction::Click {
            target_hwnd: hwnd,
            x: Some(100),
            y: Some(100),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::PermissionDenied(_))),
            "Shell class '{}' was NOT rejected!",
            class
        );
    }

    // Verify all security dialogs are rejected
    for &(hwnd, title, _) in &security_dialogs {
        let action = CuaAction::Click {
            target_hwnd: hwnd,
            x: Some(300),
            y: Some(300),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::PermissionDenied(_))),
            "Security dialog '{}' was NOT rejected!",
            title
        );
    }
}

#[tokio::test]
async fn test_adversarial_custom_denylist_precedence() {
    let mock = Arc::new(MockCuaDriver::new());

    mock.insert_window(CuaWindowInfo {
        hwnd: 0x501,
        pid: 501,
        title: "Confidential Accounting App".into(),
        class_name: "FinanceClass".into(),
        process_name: Some("finance_app.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["finance_app.exe".into()], // Allowlisted
        protected_denylist: vec!["finance_app.exe".into()], // But also denylisted!
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    let click = CuaAction::Click {
        target_hwnd: 0x501,
        x: Some(300),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res = engine.execute_action(click).await;
    assert!(
        matches!(res, Err(CuaError::PermissionDenied(_))),
        "Custom denylist entry did NOT take precedence over allowlist!"
    );
}

// ============================================================================
// AREA 2: Dangerous Hotkey Permutations, Casing & Aliases
// ============================================================================

#[tokio::test]
async fn test_adversarial_dangerous_hotkeys_exhaustive_alias_matrix() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x601,
        pid: 601,
        title: "Notepad".into(),
        class_name: "Notepad".into(),
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

    // Permutations for all 6 dangerous hotkeys
    let dangerous_matrix = vec![
        // 1. Win+R
        vec!["win", "r"],
        vec!["WIN", "R"],
        vec!["WiN", "r"],
        vec!["windows", "r"],
        vec!["super", "r"],
        vec!["lwin", "r"],
        vec!["rwin", "R"],
        vec!["r", "win"],         // reversed order
        vec!["ctrl", "win", "r"], // superset containing win+r
        // 2. Ctrl+Shift+Escape
        vec!["ctrl", "shift", "esc"],
        vec!["control", "shift", "escape"],
        vec!["CTRL", "SHIFT", "ESCAPE"],
        vec!["lctrl", "lshift", "esc"],
        vec!["rctrl", "rshift", "escape"],
        vec!["escape", "shift", "ctrl"],     // reversed order
        vec!["ctrl", "shift", "esc", "alt"], // superset
        // 3. Alt+F4
        vec!["alt", "f4"],
        vec!["ALT", "F4"],
        vec!["menu", "f4"],
        vec!["lalt", "f4"],
        vec!["ralt", "F4"],
        vec!["f4", "alt"],          // reversed order
        vec!["shift", "alt", "f4"], // superset
        // 4. Ctrl+Alt+Del
        vec!["ctrl", "alt", "del"],
        vec!["control", "menu", "delete"],
        vec!["CTRL", "ALT", "DEL"],
        vec!["lctrl", "lalt", "del"],
        vec!["rctrl", "ralt", "delete"],
        vec!["del", "alt", "ctrl"],        // reversed order
        vec!["ctrl", "alt", "del", "win"], // superset
        // 5. Win+L
        vec!["win", "l"],
        vec!["WIN", "L"],
        vec!["windows", "l"],
        vec!["super", "l"],
        vec!["lwin", "l"],
        vec!["rwin", "l"],
        vec!["l", "win"],         // reversed order
        vec!["ctrl", "win", "l"], // superset
        // 6. Win+X
        vec!["win", "x"],
        vec!["WIN", "X"],
        vec!["windows", "x"],
        vec!["super", "x"],
        vec!["lwin", "x"],
        vec!["rwin", "x"],
        vec!["x", "win"],          // reversed order
        vec!["shift", "win", "x"], // superset
    ];

    for keys in dangerous_matrix {
        let key_repr = keys.join("+");
        let action = CuaAction::Hotkey {
            target_hwnd: 0x601,
            keys: keys.into_iter().map(String::from).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            matches!(res, Err(CuaError::DangerousHotkeyRejected(_))),
            "Dangerous hotkey permutation '{}' was NOT rejected!",
            key_repr
        );
    }
}

#[tokio::test]
async fn test_adversarial_safe_hotkeys_are_not_falsely_rejected() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x701,
        pid: 701,
        title: "Notepad".into(),
        class_name: "Notepad".into(),
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

    let safe_cases = vec![
        vec!["ctrl", "c"],
        vec!["ctrl", "v"],
        vec!["ctrl", "z"],
        vec!["ctrl", "y"],
        vec!["ctrl", "a"],
        vec!["ctrl", "f"],
        vec!["alt", "tab"],
        vec!["win", "d"],
        vec!["win", "e"],
        vec!["ctrl", "shift", "n"],
        vec!["ctrl", "shift", "t"],
        vec!["f5"],
        vec!["enter"],
        vec!["escape"], // Esc alone is safe
    ];

    for keys in safe_cases {
        let key_repr = keys.join("+");
        let action = CuaAction::Hotkey {
            target_hwnd: 0x701,
            keys: keys.into_iter().map(String::from).collect(),
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(action).await;
        assert!(
            res.is_ok(),
            "Safe hotkey '{}' was falsely rejected: {:?}",
            key_repr,
            res.err()
        );
    }
}

// ============================================================================
// AREA 3: Non-Client Coordinate Clicks & Boundary Containment
// ============================================================================

#[tokio::test]
async fn test_adversarial_non_client_coordinate_clicks_and_boundaries() {
    let mock = Arc::new(MockCuaDriver::new());
    // Window bounds: [0, 0, 800, 600].
    // Client rect starts at [8, 30, 784, 562].
    // Caption inset: y < 30.
    // Close button region: x >= 750 && y < 40.
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x801,
        pid: 801,
        title: "Test Window".into(),
        class_name: "AppClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
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

    // 1. (x, 0) - Title bar top border
    let click_x_0 = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(400),
        y: Some(0),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click_x_0).await,
            Err(CuaError::CoordinateOutOfBounds { x: 400, y: 0 })
        ),
        "Click at (x, 0) was NOT rejected!"
    );

    // 2. (x, 15) - Title bar caption
    let click_x_15 = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(400),
        y: Some(15),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click_x_15).await,
            Err(CuaError::CoordinateOutOfBounds { x: 400, y: 15 })
        ),
        "Click at (x, 15) was NOT rejected!"
    );

    // 3. (width - 5, 5) - Close button [X]
    let click_close_btn = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(795),
        y: Some(5),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click_close_btn).await,
            Err(CuaError::CoordinateOutOfBounds { x: 795, y: 5 })
        ),
        "Click at (width - 5, 5) [Close button] was NOT rejected!"
    );

    // 4. (width - 5, 35) - Close button region near client boundary (x >= 750, y < 40)
    let click_close_region = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(795),
        y: Some(35),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click_close_region).await,
            Err(CuaError::CoordinateOutOfBounds { x: 795, y: 35 })
        ),
        "Click at (width - 5, 35) [Close button region] was NOT rejected!"
    );

    // 5. Negative coordinates
    let negative_coords = [(-1, 300), (400, -1), (-10, -10), (-32000, -32000)];
    for (nx, ny) in negative_coords {
        let click_neg = CuaAction::Click {
            target_hwnd: 0x801,
            x: Some(nx),
            y: Some(ny),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        assert!(
            matches!(
                engine.execute_action(click_neg).await,
                Err(CuaError::CoordinateOutOfBounds { x, y }) if x == nx && y == ny
            ),
            "Click at negative coordinates ({}, {}) was NOT rejected!",
            nx,
            ny
        );
    }

    // 6. Outside window bounds (beyond width and height)
    let oob_coords = [
        (800, 300), // x == width is out of range [0, 800)
        (801, 300),
        (400, 600), // y == height is out of range [0, 600)
        (400, 605),
        (1000, 1000),
    ];
    for (ox, oy) in oob_coords {
        let click_oob = CuaAction::Click {
            target_hwnd: 0x801,
            x: Some(ox),
            y: Some(oy),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        assert!(
            matches!(
                engine.execute_action(click_oob).await,
                Err(CuaError::CoordinateOutOfBounds { x, y }) if x == ox && y == oy
            ),
            "Click at out-of-bounds coordinates ({}, {}) was NOT rejected!",
            ox,
            oy
        );
    }

    // 7. Border insets:
    // Left border: x < 8
    let click_left_border = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(7),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click_left_border).await,
            Err(CuaError::CoordinateOutOfBounds { x: 7, y: 300 })
        ),
        "Click on left window border was NOT rejected!"
    );

    // Valid client click: (8, 300) -> Allowed!
    let click_valid_left = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(8),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        engine.execute_action(click_valid_left).await.is_ok(),
        "Valid click at client left edge (8, 300) was falsely rejected!"
    );

    // Valid client center click: (400, 300) -> Allowed!
    let click_valid_center = CuaAction::Click {
        target_hwnd: 0x801,
        x: Some(400),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        engine.execute_action(click_valid_center).await.is_ok(),
        "Valid click at client center (400, 300) was falsely rejected!"
    );
}

#[tokio::test]
async fn test_adversarial_drag_and_move_coordinate_containment() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x901,
        pid: 901,
        title: "Test Window".into(),
        class_name: "AppClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
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

    // Drag ending in title bar (400, 15)
    let drag_to_title = CuaAction::Drag {
        target_hwnd: 0x901,
        start_x: 200,
        start_y: 200,
        end_x: 400,
        end_y: 15,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(drag_to_title).await,
            Err(CuaError::CoordinateOutOfBounds { x: 400, y: 15 })
        ),
        "Drag ending in title bar was NOT rejected!"
    );

    // Drag starting in close button (795, 5)
    let drag_from_close = CuaAction::Drag {
        target_hwnd: 0x901,
        start_x: 795,
        start_y: 5,
        end_x: 200,
        end_y: 200,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(drag_from_close).await,
            Err(CuaError::CoordinateOutOfBounds { x: 795, y: 5 })
        ),
        "Drag starting at close button was NOT rejected!"
    );

    // MoveCursor to negative coordinates
    let move_neg = CuaAction::MoveCursor {
        target_hwnd: 0x901,
        x: -5,
        y: 200,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(move_neg).await,
            Err(CuaError::CoordinateOutOfBounds { x: -5, y: 200 })
        ),
        "MoveCursor to negative coordinates was NOT rejected!"
    );

    // Scroll to title bar
    let scroll_title = CuaAction::Scroll {
        target_hwnd: 0x901,
        delta_x: 0,
        delta_y: 120,
        x: Some(400),
        y: Some(10),
    };
    assert!(
        matches!(
            engine.execute_action(scroll_title).await,
            Err(CuaError::CoordinateOutOfBounds { x: 400, y: 10 })
        ),
        "Scroll targeting title bar was NOT rejected!"
    );
}

// ============================================================================
// AREA 4: Standard Mode Read-Only Barrier Fail-Closed Invariants
// ============================================================================

#[tokio::test]
async fn test_adversarial_standard_mode_blocks_all_mutating_actions_fail_closed() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xa01,
        pid: 1001,
        title: "Test Target".into(),
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

    // 1. Click
    let click = CuaAction::Click {
        target_hwnd: 0xa01,
        x: Some(300),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject Click!"
    );

    // 2. MoveCursor
    let move_cursor = CuaAction::MoveCursor {
        target_hwnd: 0xa01,
        x: 300,
        y: 300,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(move_cursor).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject MoveCursor!"
    );

    // 3. Drag
    let drag = CuaAction::Drag {
        target_hwnd: 0xa01,
        start_x: 200,
        start_y: 200,
        end_x: 400,
        end_y: 400,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(drag).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject Drag!"
    );

    // 4. Scroll
    let scroll = CuaAction::Scroll {
        target_hwnd: 0xa01,
        delta_x: 0,
        delta_y: 120,
        x: Some(300),
        y: Some(300),
    };
    assert!(
        matches!(
            engine.execute_action(scroll).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject Scroll!"
    );

    // 5. TypeText
    let type_text = CuaAction::TypeText {
        target_hwnd: 0xa01,
        text: "hello".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(type_text).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject TypeText!"
    );

    // 6. PressKey
    let press_key = CuaAction::PressKey {
        target_hwnd: 0xa01,
        key: "Return".into(),
        down: true,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(press_key).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject PressKey!"
    );

    // 7. Hotkey
    let hotkey = CuaAction::Hotkey {
        target_hwnd: 0xa01,
        keys: vec!["Ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(hotkey).await,
            Err(CuaError::PermissionDenied(_))
        ),
        "Standard mode did NOT reject Hotkey!"
    );

    // Inspection actions permitted in Standard mode
    let bring_front = CuaAction::BringToFront { target_hwnd: 0xa01 };
    assert!(
        engine.execute_action(bring_front).await.is_ok(),
        "Standard mode falsely rejected BringToFront!"
    );

    let restore = CuaAction::RestoreWindow { target_hwnd: 0xa01 };
    assert!(
        engine.execute_action(restore).await.is_ok(),
        "Standard mode falsely rejected RestoreWindow!"
    );
}

// ============================================================================
// AREA 5: UIPI Integrity & Mandatory Elevation Barriers
// ============================================================================

#[tokio::test]
async fn test_adversarial_uipi_elevation_matrix_across_all_actions() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xb01,
        pid: 1101,
        title: "Administrator Command Prompt".into(),
        class_name: "ConsoleWindowClass".into(),
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

    // Agent Medium integrity (0x2000), Target High integrity (0x3000)
    engine.security_governor.set_simulated_agent_rid(0x2000);
    engine.security_governor.set_simulated_uipi(0xb01, 0x3000);

    // 1. Click
    let click = CuaAction::Click {
        target_hwnd: 0xb01,
        x: Some(300),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(click).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block Click!"
    );

    // 2. TypeText
    let type_text = CuaAction::TypeText {
        target_hwnd: 0xb01,
        text: "whoami".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(type_text).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block TypeText!"
    );

    // 3. Scroll
    let scroll = CuaAction::Scroll {
        target_hwnd: 0xb01,
        delta_x: 0,
        delta_y: 120,
        x: Some(300),
        y: Some(300),
    };
    assert!(
        matches!(
            engine.execute_action(scroll).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block Scroll!"
    );

    // 4. MoveCursor
    let move_cursor = CuaAction::MoveCursor {
        target_hwnd: 0xb01,
        x: 300,
        y: 300,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(move_cursor).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block MoveCursor!"
    );

    // 5. Drag
    let drag = CuaAction::Drag {
        target_hwnd: 0xb01,
        start_x: 200,
        start_y: 200,
        end_x: 400,
        end_y: 400,
        button: MouseButton::Left,
        steps: 5,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(drag).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block Drag!"
    );

    // 6. Hotkey
    let hotkey = CuaAction::Hotkey {
        target_hwnd: 0xb01,
        keys: vec!["Ctrl".into(), "c".into()],
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(hotkey).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block Hotkey!"
    );

    // 7. PressKey
    let press_key = CuaAction::PressKey {
        target_hwnd: 0xb01,
        key: "Return".into(),
        down: true,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(
        matches!(
            engine.execute_action(press_key).await,
            Err(CuaError::UipiBlocked(_))
        ),
        "UIPI elevation did NOT block PressKey!"
    );
}

#[tokio::test]
async fn test_adversarial_uipi_integrity_levels_hierarchy() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xc01,
        pid: 1201,
        title: "Test App".into(),
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

    // (Agent RID, Target RID, Should Block)
    let hierarchy_scenarios = [
        (0x1000, 0x2000, true),  // Low -> Medium: Blocked
        (0x1000, 0x3000, true),  // Low -> High: Blocked
        (0x2000, 0x3000, true),  // Medium -> High: Blocked
        (0x2000, 0x4000, true),  // Medium -> System: Blocked
        (0x3000, 0x3000, false), // High -> High: Allowed
        (0x3000, 0x2000, false), // High -> Medium: Allowed
        (0x3000, 0x1000, false), // High -> Low: Allowed
        (0x4000, 0x3000, false), // System -> High: Allowed
        (0x2000, 0x2000, false), // Medium -> Medium: Allowed
    ];

    for &(agent_rid, target_rid, should_block) in &hierarchy_scenarios {
        engine.security_governor.set_simulated_agent_rid(agent_rid);
        engine
            .security_governor
            .set_simulated_uipi(0xc01, target_rid);

        let click = CuaAction::Click {
            target_hwnd: 0xc01,
            x: Some(300),
            y: Some(300),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = engine.execute_action(click).await;

        if should_block {
            assert!(
                matches!(res, Err(CuaError::UipiBlocked(_))),
                "UIPI failed to block: Agent {:#x} -> Target {:#x}",
                agent_rid,
                target_rid
            );
        } else {
            assert!(
                res.is_ok(),
                "UIPI falsely blocked: Agent {:#x} -> Target {:#x}: {:?}",
                agent_rid,
                target_rid,
                res.err()
            );
        }
    }
}

#[tokio::test]
async fn test_adversarial_uipi_enforcement_in_unrestricted_mode() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xd01,
        pid: 1301,
        title: "Elevated Notepad".into(),
        class_name: "Notepad".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // Unrestricted permission mode
    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = CuaEngine::new_with_driver(config, mock);

    // Agent Medium (0x2000) vs Target High (0x3000)
    engine.security_governor.set_simulated_agent_rid(0x2000);
    engine.security_governor.set_simulated_uipi(0xd01, 0x3000);

    let click = CuaAction::Click {
        target_hwnd: 0xd01,
        x: Some(300),
        y: Some(300),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res = engine.execute_action(click).await;

    // UIPI is an OS mandatory integrity barrier; must block even in Unrestricted mode!
    assert!(
        matches!(res, Err(CuaError::UipiBlocked(_))),
        "UIPI elevation did NOT block action in Unrestricted mode!"
    );
}
