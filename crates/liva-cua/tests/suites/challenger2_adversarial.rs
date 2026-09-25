//! Empirical Adversarial Verification Suite — Challenger 2
//! Milestone M1: Native CUA Desktop Automation Engine
//!
//! Areas under challenge:
//! 1. Emergency Halt Latency & State Invariants (< 15ms abort, get_status halted flag, reset mechanics)
//! 2. UIPI Token Integrity Filtering (integrity levels, ordering, cross-integrity refusal, bypass gaps)
//! 3. Synthetic Pointer & NoActivate Safety (NoActivateGuard style application, restoration, bit preservation, cross-process limits)
//! 4. Target PID Mismatch & Minimized Element Refusal (error codes, guarded vs unguarded, sentinel/degenerate bounds)

use liva_cua::guards::{ExactPidWindowTargetGuard, MinimizedWindowGuard};
use liva_cua::input::pointer::NoActivateGuard;
use liva_cua::input::uipi::*;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;
use std::time::Instant;

// ===========================================================================
// AREA 1: Emergency Halt Latency & State Invariants
// ===========================================================================

#[tokio::test]
async fn test_challenger2_emergency_halt_latency_and_state_transition() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1111,
        pid: 100,
        title: "Test Window".into(),
        class_name: "TestClass".into(),
        process_name: Some("test.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    // Initial status: not halted
    let status_before = engine.get_status().await;
    assert!(!status_before.is_halted, "Initial state must not be halted");

    // Measure emergency halt transition latency
    let start_halt = Instant::now();
    engine
        .trigger_emergency_halt()
        .expect("Emergency halt trigger must succeed");
    let halt_duration = start_halt.elapsed();

    // Verify halt transition latency is < 15ms SLA
    assert!(
        halt_duration.as_millis() < 15,
        "Emergency halt transition took {}ms, expected < 15ms SLA",
        halt_duration.as_millis()
    );

    // Query status: must indicate is_halted == true
    let status_after = engine.get_status().await;
    assert!(
        status_after.is_halted,
        "Status after trigger must be is_halted == true"
    );

    // Measure rejection latency for subsequent action
    let action = CuaAction::Click {
        target_hwnd: 0x1111,
        x: Some(100),
        y: Some(100),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let start_action = Instant::now();
    let action_res = engine.execute_action(action).await;
    let action_duration = start_action.elapsed();

    assert!(
        action_duration.as_millis() < 15,
        "Action rejection took {}ms, expected < 15ms SLA",
        action_duration.as_millis()
    );

    // Verify rejection error
    assert!(
        action_res.is_err(),
        "Action must be rejected after emergency halt"
    );
    assert_eq!(
        action_res.unwrap_err(),
        CuaError::EmergencyHalted,
        "Rejection must return CuaError::EmergencyHalted"
    );
}

#[tokio::test]
async fn test_challenger2_emergency_halt_reset_and_restoration() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2222,
        pid: 200,
        title: "Test Window".into(),
        class_name: "TestClass".into(),
        process_name: Some("test.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock.clone());

    // Halt
    engine.trigger_emergency_halt().unwrap();
    assert!(engine.get_status().await.is_halted);

    // Reset halt
    let start_reset = Instant::now();
    engine
        .reset_emergency_halt()
        .expect("Reset emergency halt must succeed");
    let reset_duration = start_reset.elapsed();
    assert!(
        reset_duration.as_millis() < 15,
        "Reset emergency halt took {}ms, expected < 15ms SLA",
        reset_duration.as_millis()
    );

    let status = engine.get_status().await;
    assert!(
        !status.is_halted,
        "Status after reset must be is_halted == false"
    );

    // Action now executes normally
    let action = CuaAction::TypeText {
        target_hwnd: 0x2222,
        text: "resumed text".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res = engine.execute_action(action).await;
    assert!(res.is_ok(), "Action must succeed after halt reset");
    assert_eq!(mock.get_dispatched_actions().len(), 1);
}

#[tokio::test]
async fn test_challenger2_emergency_halt_concurrent_storm() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x3333,
        pid: 300,
        title: "Storm Window".into(),
        class_name: "StormClass".into(),
        process_name: Some("storm.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = Arc::new(CuaEngine::new_with_driver(CuaConfig::default(), mock));

    // Spawn 50 tasks concurrently trying to execute actions
    let mut handles = Vec::new();
    for i in 0..50 {
        let eng = engine.clone();
        handles.push(tokio::spawn(async move {
            let act = CuaAction::Click {
                target_hwnd: 0x3333,
                x: Some(i),
                y: Some(i),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: CuaDeliveryMode::Background,
            };
            eng.execute_action(act).await
        }));
    }

    // Trigger halt concurrently
    engine.trigger_emergency_halt().unwrap();

    let mut halted_count = 0;
    let mut completed_count = 0;
    for h in handles {
        let res = h.await.unwrap();
        match res {
            Ok(_) => completed_count += 1,
            Err(CuaError::EmergencyHalted) => halted_count += 1,
            Err(other) => panic!("Unexpected error during storm: {:?}", other),
        }
    }

    // Verify all subsequent actions after halt are rejected
    assert!(
        halted_count > 0,
        "At least some actions must have been halted"
    );
    let follow_up = engine
        .execute_action(CuaAction::RestoreWindow {
            target_hwnd: 0x3333,
        })
        .await;
    assert_eq!(follow_up.unwrap_err(), CuaError::EmergencyHalted);
    println!(
        "Concurrent storm: {} halted, {} completed",
        halted_count, completed_count
    );
}

// ===========================================================================
// AREA 2: UIPI Token Integrity Filtering
// ===========================================================================

#[test]
fn test_challenger2_uipi_integrity_level_from_rid_and_ordering() {
    // Exact mapping verification
    assert_eq!(IntegrityLevel::from_rid(0x0000), IntegrityLevel::Untrusted);
    assert_eq!(IntegrityLevel::from_rid(0x0500), IntegrityLevel::Untrusted);
    assert_eq!(IntegrityLevel::from_rid(0x1000), IntegrityLevel::Low);
    assert_eq!(IntegrityLevel::from_rid(0x1500), IntegrityLevel::Low);
    assert_eq!(IntegrityLevel::from_rid(0x2000), IntegrityLevel::Medium);
    assert_eq!(IntegrityLevel::from_rid(0x2050), IntegrityLevel::Medium);
    assert_eq!(IntegrityLevel::from_rid(0x2100), IntegrityLevel::MediumPlus);
    assert_eq!(IntegrityLevel::from_rid(0x2500), IntegrityLevel::MediumPlus);
    assert_eq!(IntegrityLevel::from_rid(0x3000), IntegrityLevel::High);
    assert_eq!(IntegrityLevel::from_rid(0x3500), IntegrityLevel::High);
    assert_eq!(IntegrityLevel::from_rid(0x4000), IntegrityLevel::System);
    assert_eq!(IntegrityLevel::from_rid(0x4500), IntegrityLevel::System);
    assert_eq!(IntegrityLevel::from_rid(0x5000), IntegrityLevel::Protected);
    assert_eq!(IntegrityLevel::from_rid(0x6000), IntegrityLevel::Protected);

    // Strict monotonic ordering
    assert!(IntegrityLevel::Untrusted < IntegrityLevel::Low);
    assert!(IntegrityLevel::Low < IntegrityLevel::Medium);
    assert!(IntegrityLevel::Medium < IntegrityLevel::MediumPlus);
    assert!(IntegrityLevel::MediumPlus < IntegrityLevel::High);
    assert!(IntegrityLevel::High < IntegrityLevel::System);
    assert!(IntegrityLevel::System < IntegrityLevel::Protected);

    // Check equal comparison
    assert_eq!(IntegrityLevel::Medium, IntegrityLevel::Medium);
    assert!(IntegrityLevel::Medium <= IntegrityLevel::Medium);
    assert!(IntegrityLevel::Medium >= IntegrityLevel::Medium);
}

#[tokio::test]
async fn test_challenger2_uipi_elevation_matrix() {
    let mock = Arc::new(MockCuaDriver::new());
    *mock.agent_rid.lock().unwrap() = 0x2000; // Medium agent

    // Target A: Low (0x1000) -> Allowed
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xAAAA,
        pid: 101,
        title: "Low Integrity App".into(),
        class_name: "LowClass".into(),
        process_name: Some("low.exe".into()),
        bounds: CuaRect::new(0, 0, 400, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0xAAAA, 0x1000);

    // Target B: Medium (0x2000) -> Allowed
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xBBBB,
        pid: 102,
        title: "Medium Integrity App".into(),
        class_name: "MedClass".into(),
        process_name: Some("med.exe".into()),
        bounds: CuaRect::new(0, 0, 400, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0xBBBB, 0x2000);

    // Target C: High (0x3000) -> Blocked
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xCCCC,
        pid: 103,
        title: "Elevated High App".into(),
        class_name: "HighClass".into(),
        process_name: Some("high.exe".into()),
        bounds: CuaRect::new(0, 0, 400, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0xCCCC, 0x3000);

    // Target D: System (0x4000) -> Blocked
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xDDDD,
        pid: 104,
        title: "System App".into(),
        class_name: "SysClass".into(),
        process_name: Some("sys.exe".into()),
        bounds: CuaRect::new(0, 0, 400, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0xDDDD, 0x4000);

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    // 1. Medium -> Low (allowed)
    let res_low = engine
        .execute_action(CuaAction::Click {
            target_hwnd: 0xAAAA,
            x: Some(100),
            y: Some(100),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_low.is_ok(),
        "Medium agent targeting Low process must succeed"
    );

    // 2. Medium -> Medium (allowed)
    let res_med = engine
        .execute_action(CuaAction::Click {
            target_hwnd: 0xBBBB,
            x: Some(100),
            y: Some(100),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_med.is_ok(),
        "Medium agent targeting Medium process must succeed"
    );

    // 3. Medium -> High (blocked)
    let res_high = engine
        .execute_action(CuaAction::Click {
            target_hwnd: 0xCCCC,
            x: Some(100),
            y: Some(100),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_high.is_err(),
        "Medium agent targeting High process must be blocked by UIPI"
    );
    match res_high.unwrap_err() {
        CuaError::UipiBlocked(msg) => {
            assert!(
                msg.contains("0x3000"),
                "Error message should mention target RID 0x3000: {}",
                msg
            );
        }
        other => panic!("Expected UipiBlocked, got: {:?}", other),
    }

    // 4. Medium -> System (blocked)
    let res_sys = engine
        .execute_action(CuaAction::Click {
            target_hwnd: 0xDDDD,
            x: Some(100),
            y: Some(100),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Background,
        })
        .await;
    assert!(
        res_sys.is_err(),
        "Medium agent targeting System process must be blocked by UIPI"
    );
    assert!(matches!(res_sys.unwrap_err(), CuaError::UipiBlocked(_)));
}

#[test]
fn test_challenger2_uipi_self_process_bypass() {
    #[cfg(windows)]
    {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::GetDesktopWindow;

        // Current process ID
        let self_pid = unsafe { GetCurrentProcessId() };
        let self_integrity = get_process_integrity_level(self_pid);
        assert!(
            self_integrity.is_ok(),
            "Must query own process integrity level"
        );

        // Desktop window
        let desktop_hwnd = unsafe { GetDesktopWindow() } as usize as u64;
        let uipi = check_uipi_restriction(desktop_hwnd);
        assert!(
            uipi.is_ok(),
            "check_uipi_restriction on desktop window must return verdict"
        );
    }
}

// ===========================================================================
// AREA 3: Synthetic Pointer & NoActivate Safety
// ===========================================================================

#[test]
fn test_challenger2_noactivate_guard_local_window_lifecycle() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_POPUP,
        };

        let hinstance =
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());

        // Create a local popup window with known style (without WS_EX_NOACTIVATE)
        let class_name: Vec<u16> = "Static\0".encode_utf16().collect();
        let title: Vec<u16> = "Challenger2TestWin\0".encode_utf16().collect();

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            100,
            100,
            200,
            200,
            0,
            0,
            hinstance,
            std::ptr::null_mut(),
        );

        if hwnd == 0 {
            eprintln!("CreateWindowExW returned 0, skipping live window test (headless)");
            return;
        }

        let initial_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        assert_eq!(
            initial_ex_style & (WS_EX_NOACTIVATE as isize),
            0,
            "Initial style must NOT contain WS_EX_NOACTIVATE"
        );

        // Scope the guard
        {
            let guard = NoActivateGuard::new(hwnd);
            assert!(
                guard.is_ok(),
                "NoActivateGuard::new must succeed on local window"
            );

            let active_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            assert_ne!(
                active_ex_style & (WS_EX_NOACTIVATE as isize),
                0,
                "Window must have WS_EX_NOACTIVATE set while guard is active"
            );
            assert_eq!(
                active_ex_style,
                initial_ex_style | (WS_EX_NOACTIVATE as isize),
                "No other style bits should be modified during guard activation"
            );
        } // guard drops here

        let restored_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        assert_eq!(
            restored_ex_style, initial_ex_style,
            "Window ex_style must be EXACTLY restored after NoActivateGuard drops without leaking bits"
        );
        assert_eq!(
            restored_ex_style & (WS_EX_NOACTIVATE as isize),
            0,
            "WS_EX_NOACTIVATE must be removed after guard drop"
        );

        DestroyWindow(hwnd);
    }
}

#[test]
fn test_challenger2_noactivate_guard_preexisting_bit_preservation() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DestroyWindow, GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
            WS_EX_TOOLWINDOW, WS_POPUP,
        };

        let hinstance =
            windows_sys::Win32::System::LibraryLoader::GetModuleHandleW(std::ptr::null());

        let class_name: Vec<u16> = "Static\0".encode_utf16().collect();
        let title: Vec<u16> = "Challenger2PreexistingWin\0".encode_utf16().collect();

        // Window created WITH WS_EX_NOACTIVATE
        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
            class_name.as_ptr(),
            title.as_ptr(),
            WS_POPUP,
            100,
            100,
            200,
            200,
            0,
            0,
            hinstance,
            std::ptr::null_mut(),
        );

        if hwnd == 0 {
            eprintln!("CreateWindowExW returned 0, skipping live window test");
            return;
        }

        let initial_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        assert_ne!(
            initial_ex_style & (WS_EX_NOACTIVATE as isize),
            0,
            "Window initially has WS_EX_NOACTIVATE"
        );

        {
            let guard = NoActivateGuard::new(hwnd);
            assert!(guard.is_ok());
            // is_applied should be false
        }

        let after_drop_ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        assert_eq!(
            after_drop_ex_style, initial_ex_style,
            "Preexisting WS_EX_NOACTIVATE must NOT be stripped off when guard drops!"
        );

        DestroyWindow(hwnd);
    }
}

#[test]
fn test_challenger2_noactivate_guard_cross_process_limitation() {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::GetCurrentProcessId;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowLongPtrW, GWL_EXSTYLE, WS_EX_NOACTIVATE,
        };

        let my_pid = GetCurrentProcessId();
        let windows = liva_cua::window::list_windows(None).unwrap_or_default();
        let external_win = windows.iter().find(|w| w.pid != my_pid && w.is_on_screen);

        if let Some(w) = external_win {
            let hwnd = w.hwnd as usize as windows_sys::Win32::Foundation::HWND;
            let initial_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

            // Try applying NoActivateGuard to external window
            let guard = NoActivateGuard::new(hwnd);
            assert!(
                guard.is_ok(),
                "NoActivateGuard returns Ok even on external windows"
            );

            let style_during_guard = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);

            let was_applied = (style_during_guard & (WS_EX_NOACTIVATE as isize)) != 0;
            println!(
                "External window '{}' (PID {}, HWND {:#x}) initial_style: {:#x}, during guard: {:#x}, WS_EX_NOACTIVATE applied: {}",
                w.title, w.pid, hwnd, initial_style, style_during_guard, was_applied
            );

            // In Win32, SetWindowLongPtrW cannot modify window styles across process boundaries.
            // If it didn't have it initially, Win32 security prevents setting it cross-process.
            if (initial_style & (WS_EX_NOACTIVATE as isize)) == 0 {
                assert_eq!(
                    style_during_guard, initial_style,
                    "Win32 security prevents SetWindowLongPtrW from modifying styles of cross-process windows!"
                );
            }
        } else {
            println!("No external window found to test cross-process NoActivateGuard");
        }
    }
}

#[test]
fn test_challenger2_noactivate_guard_invalid_hwnd_silent_success() {
    #[cfg(windows)]
    {
        // Pass a completely bogus / non-existent HWND
        let bogus_hwnd = 0xDEAD_BEEF_usize as windows_sys::Win32::Foundation::HWND;
        let guard_res = NoActivateGuard::new(bogus_hwnd);

        // Current implementation returns Ok even for a completely invalid HWND
        // because it ignores SetWindowLongPtrW errors!
        println!(
            "NoActivateGuard on bogus HWND returned: {:?}",
            guard_res.is_ok()
        );
        assert!(
            guard_res.is_ok(),
            "Finding: NoActivateGuard::new returns Ok even on invalid HWND because SetWindowLongPtrW return is unchecked!"
        );
    }
}

// ===========================================================================

// AREA 4: Target PID Mismatch & Minimized Element Refusal
// ===========================================================================

#[test]
fn test_challenger2_exact_pid_guard_mismatch_verification() {
    let guard = ExactPidWindowTargetGuard::new(1000);

    // Mismatched PID
    let result = guard.validate(0x9999, 2000, None, None);
    assert!(result.is_err(), "Target PID mismatch must fail validation");

    let err = result.unwrap_err();
    match err {
        CuaError::TargetPidMismatch {
            hwnd,
            expected_pid,
            actual_pid,
        } => {
            assert_eq!(hwnd, 0x9999);
            assert_eq!(expected_pid, 1000);
            assert_eq!(actual_pid, 2000);
            assert_eq!(
                err.error_code(),
                "target_pid_mismatch",
                "Error code must be target_pid_mismatch"
            );
        }
        other => panic!("Expected TargetPidMismatch, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_challenger2_guarded_vs_unguarded_pid_behavior() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5555,
        pid: 8888, // Actual PID in window info
        title: "Protected Bank".into(),
        class_name: "BankClass".into(),
        process_name: Some("bank.exe".into()),
        bounds: CuaRect::new(0, 0, 500, 400),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    let action = CuaAction::Click {
        target_hwnd: 0x5555,
        x: Some(50),
        y: Some(50),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // 1. Guarded execution with WRONG expected PID (1111 != 8888) -> Must be refused!
    let guard = ExactPidWindowTargetGuard::new(1111);
    let guarded_res = engine.execute_action_guarded(action.clone(), guard).await;
    assert!(
        guarded_res.is_err(),
        "Guarded execution with wrong PID must fail"
    );
    assert!(matches!(
        guarded_res.unwrap_err(),
        CuaError::TargetPidMismatch {
            expected_pid: 1111,
            actual_pid: 8888,
            ..
        }
    ));

    // 2. Guarded execution with CORRECT expected PID (8888 == 8888) -> Must succeed!
    let correct_guard = ExactPidWindowTargetGuard::new(8888);
    let success_res = engine
        .execute_action_guarded(action.clone(), correct_guard)
        .await;
    assert!(
        success_res.is_ok(),
        "Guarded execution with matching PID must succeed"
    );
}

#[test]
fn test_challenger2_minimized_window_guard_comprehensive() {
    // 1. Win32 iconic flag true
    let err_iconic = MinimizedWindowGuard::validate(0x1, true, &CuaRect::new(0, 0, 800, 600));
    assert_eq!(err_iconic.unwrap_err(), CuaError::WindowMinimized(0x1));

    // 2. Win32 sentinel coordinates (-32000, -32000)
    let err_sentinel =
        MinimizedWindowGuard::validate(0x2, false, &CuaRect::new(-32000, -32000, 160, 24));
    assert_eq!(err_sentinel.unwrap_err(), CuaError::WindowMinimized(0x2));

    // 3. Degenerate zero width
    let err_zero_w = MinimizedWindowGuard::validate(0x3, false, &CuaRect::new(100, 100, 0, 600));
    assert_eq!(err_zero_w.unwrap_err(), CuaError::WindowMinimized(0x3));

    // 4. Degenerate zero height
    let err_zero_h = MinimizedWindowGuard::validate(0x4, false, &CuaRect::new(100, 100, 800, 0));
    assert_eq!(err_zero_h.unwrap_err(), CuaError::WindowMinimized(0x4));

    // 5. Degenerate negative width
    let err_neg_w = MinimizedWindowGuard::validate(0x5, false, &CuaRect::new(100, 100, -50, 600));
    assert_eq!(err_neg_w.unwrap_err(), CuaError::WindowMinimized(0x5));

    // 6. Normal valid window -> Ok
    let ok_win = MinimizedWindowGuard::validate(0x6, false, &CuaRect::new(100, 100, 800, 600));
    assert!(ok_win.is_ok());
}
