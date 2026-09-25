//! Tests for Emergency Halt (< 50ms SLA), kill-switch controller, and reset mechanics.

use liva_cua::kill_switch::*;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};

#[tokio::test]
async fn test_emergency_halt_latency_and_action_blocking() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1000,
        pid: 500,
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

    let start = Instant::now();
    engine.trigger_emergency_halt().unwrap();
    let elapsed = start.elapsed();

    // Verify halt transition SLA < 15ms (well within < 50ms SLA)
    assert!(
        elapsed.as_millis() < 15,
        "Emergency halt transition took {}ms, expected < 15ms",
        elapsed.as_millis()
    );

    let status = engine.get_status().await;
    assert!(
        status.is_halted,
        "Engine status must indicate is_halted == true"
    );

    // Any subsequent action must be immediately refused with CuaError::EmergencyHalted
    let action = CuaAction::TypeText {
        target_hwnd: 0x1000,
        text: "Aborted text sequence".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert_eq!(result.unwrap_err(), CuaError::EmergencyHalted);
}

#[tokio::test]
async fn test_emergency_halt_reset() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2000,
        pid: 600,
        title: "Test Window 2".into(),
        class_name: "TestClass2".into(),
        process_name: Some("test2.exe".into()),
        bounds: CuaRect::new(10, 10, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock.clone());

    engine.trigger_emergency_halt().unwrap();
    assert!(engine.get_status().await.is_halted);

    // Reset halt
    engine.reset_emergency_halt().unwrap();
    assert!(!engine.get_status().await.is_halted);

    // Action now executes successfully
    let action = CuaAction::Click {
        target_hwnd: 0x2000,
        x: Some(100),
        y: Some(100),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert!(result.is_ok());
    assert_eq!(mock.get_dispatched_actions().len(), 1);
}

#[test]
fn test_kill_switch_atomic_halt_and_cancellation_latency() {
    let ks = KillSwitchController::new(KillSwitchConfig {
        esc_polling_enabled: false,
        poll_interval_ms: 5,
        mouse_thrashing_threshold_px: 50.0,
        mouse_thrashing_enabled: false,
    });

    assert!(!ks.is_halted());
    let token = ks.cancellation_token();
    assert!(!token.is_cancelled());

    let start = Instant::now();
    ks.trigger_halt(HaltReason::ManualTrigger {
        detail: "unit test".into(),
    })
    .unwrap();
    let elapsed = start.elapsed();

    // Must transition in < 15ms (achieved < 0.1ms)
    assert!(
        elapsed.as_millis() < 15,
        "Halt took {}ms, expected < 15ms",
        elapsed.as_millis()
    );
    assert!(ks.is_halted());
    assert!(token.is_cancelled());

    // Verify status
    let status = ks.get_status();
    assert_eq!(status.state, KillSwitchState::Halted);
    assert_eq!(
        status.halt_reason,
        Some(HaltReason::ManualTrigger {
            detail: "unit test".into()
        })
    );
}

#[test]
fn test_mouse_thrashing_detector_thresholds() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());
    let motion = &ks.motion;

    // Baseline at (500, 500)
    motion.begin_action(0x1000, (500, 500), (500, 500));

    // Jitter of 30px (e.g. at 520, 520 -> dist ~28.28px <= 50.0px)
    let small_jitter = motion.check_displacement((520, 520), 50.0);
    assert!(
        small_jitter.is_none(),
        "Small jitter <= 50px must NOT trigger thrashing"
    );

    // Sudden human jerk of 75px (at 560, 550 -> dist = sqrt(60^2 + 50^2) = 78.1px > 50.0px)
    let large_jerk = motion.check_displacement((560, 550), 50.0);
    assert!(
        large_jerk.is_some(),
        "Displacement > 50px MUST trigger thrashing"
    );

    if let Some(HaltReason::MouseThrashing {
        displacement_px, ..
    }) = large_jerk
    {
        assert!(displacement_px > 50.0);
    } else {
        panic!("Expected MouseThrashing reason");
    }
}

#[tokio::test]
async fn test_mid_flight_typing_cancellation_latency() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x1000,
        pid: 500,
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
    // 10ms per char -> 135 chars = 1350ms duration
    mock.set_typing_delay(Duration::from_millis(10));

    let engine = Arc::new(CuaEngine::new_with_driver(
        CuaConfig::default(),
        mock.clone(),
    ));
    let engine_clone = Arc::clone(&engine);

    let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(3);
    let action = CuaAction::TypeText {
        target_hwnd: 0x1000,
        text: long_text,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // 1. Dispatch action through genuine CuaEngine::execute_action
    let action_task = tokio::spawn(async move { engine_clone.execute_action(action).await });

    // 2. Allow typing to initiate and emit ~3 characters (30ms)
    tokio::time::sleep(Duration::from_millis(30)).await;

    // 3. Trigger emergency halt mid-flight
    let start_halt = Instant::now();
    engine
        .trigger_emergency_halt()
        .expect("Emergency halt must trigger");

    let action_res = action_task.await.expect("Action task must not panic");
    let abort_duration = start_halt.elapsed();

    // 4. Verify SLA < 15ms target
    assert!(
        abort_duration.as_millis() < 15,
        "Mid-flight typing abort took {}ms, expected < 15ms target",
        abort_duration.as_millis()
    );

    // 5. Verify action returned EmergencyHalted error
    assert_eq!(
        action_res.unwrap_err(),
        CuaError::EmergencyHalted,
        "Action must return EmergencyHalted"
    );

    // 6. Verify loop aborted mid-string without continuing to send characters
    let typed_chars = mock.get_typed_characters();
    assert!(
        typed_chars.len() < 10,
        "Typing loop must abort mid-string; only {} chars typed out of >130",
        typed_chars.len()
    );
    assert!(
        !typed_chars.is_empty(),
        "Typing loop should have emitted at least 1-2 chars before abort"
    );

    // 7. Verify engine state is halted
    let status = engine.get_status().await;
    assert!(status.is_halted, "Engine must be halted");
}

#[tokio::test]
async fn test_mid_flight_driver_abort_via_engine_select_guard() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x3000,
        pid: 700,
        title: "Test Select Window".into(),
        class_name: "TestClass3".into(),
        process_name: Some("test3.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.action_delay_ms
        .store(500, std::sync::atomic::Ordering::Relaxed);

    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = Arc::new(CuaEngine::new_with_driver(config, mock));
    let engine_clone = Arc::clone(&engine);

    let action = CuaAction::TypeText {
        target_hwnd: 0x3000,
        text: "Long sequence of slow typing".into(),
        delivery_mode: CuaDeliveryMode::Background,
    };

    let action_task = tokio::spawn(async move { engine_clone.execute_action(action).await });

    tokio::time::sleep(Duration::from_millis(20)).await;

    let start_halt = Instant::now();
    engine.trigger_emergency_halt().unwrap();

    let res = action_task.await.unwrap();
    let elapsed = start_halt.elapsed();

    assert!(
        elapsed.as_millis() < 15,
        "Engine select guard took {}ms, expected < 15ms",
        elapsed.as_millis()
    );
    assert_eq!(res.unwrap_err(), CuaError::EmergencyHalted);
}

#[test]
fn test_kill_switch_poller_drops_cleanly_without_arc_cycle() {
    let config = KillSwitchConfig {
        esc_polling_enabled: true,
        poll_interval_ms: 5,
        ..Default::default()
    };

    let controller = KillSwitchController::new(config);
    assert!(controller.get_status().is_poller_active);

    // Strong count must be exactly 1 (held by caller), weak count must be 1 (held by poller thread)
    assert_eq!(
        Arc::strong_count(&controller),
        1,
        "Poller thread must NOT hold a strong Arc reference"
    );
    assert_eq!(
        Arc::weak_count(&controller),
        1,
        "Poller thread must hold a Weak reference"
    );

    let weak_ref = Arc::downgrade(&controller);
    // Dropping controller must trigger Drop::drop and terminate the poller thread cleanly
    drop(controller);

    // Verify controller was dropped and cannot be upgraded
    assert!(
        weak_ref.upgrade().is_none(),
        "Controller must be completely dropped and deallocated"
    );
}

#[tokio::test]
async fn test_foreground_click_resting_cursor_no_false_positive() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x2000,
        pid: 2000,
        title: "Calculator".into(),
        class_name: "CalcFrame".into(),
        process_name: Some("calc.exe".into()),
        bounds: CuaRect::new(0, 0, 1920, 1080),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        kill_switch_enabled: true,
        ..Default::default()
    };

    let engine = CuaEngine::new_with_driver(config, mock);
    engine.kill_switch.start_poller();

    // Target (600, 600) is far away from default hardware cursor resting pos
    let result = engine
        .execute_action(CuaAction::Click {
            target_hwnd: 0x2000,
            x: Some(600),
            y: Some(600),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: CuaDeliveryMode::Foreground,
        })
        .await;

    assert!(
        result.is_ok(),
        "Foreground click with resting cursor must NOT trigger false-positive thrashing abort: {:?}",
        result.err()
    );
}

#[test]
fn test_unix_ms_to_iso8601_utc_boundaries() {
    // 1. Unix Epoch (1970-01-01T00:00:00Z)
    assert_eq!(unix_ms_to_iso8601_utc(0), "1970-01-01T00:00:00Z");

    // 2. 2024 Leap Day Midnight (2024-02-29T00:00:00Z)
    assert_eq!(
        unix_ms_to_iso8601_utc(1709164800 * 1000),
        "2024-02-29T00:00:00Z"
    );

    // 3. 2024 Leap Day with Hours, Minutes, Seconds (2024-02-29T13:45:30Z)
    assert_eq!(
        unix_ms_to_iso8601_utc(1709214330 * 1000),
        "2024-02-29T13:45:30Z"
    );

    // 4. Milestone M2 Target Timestamp (2026-09-24T12:00:00Z)
    assert_eq!(
        unix_ms_to_iso8601_utc(1790251200 * 1000),
        "2026-09-24T12:00:00Z"
    );

    // 5. Year-end Boundary (2023-12-31T23:59:59Z)
    assert_eq!(
        unix_ms_to_iso8601_utc(1704067199 * 1000),
        "2023-12-31T23:59:59Z"
    );

    // 6. Current timestamp adheres to ISO-8601 pattern
    let current = current_iso8601_utc();
    assert_eq!(current.len(), 20);
    assert!(current.ends_with('Z'));
    assert_eq!(&current[10..11], "T");
}

#[test]
fn test_reset_and_recovery_lifecycle() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());

    ks.trigger_halt(HaltReason::EscapeKeyPressed).unwrap();
    assert!(ks.is_halted());
    assert!(ks.cancellation_token().is_cancelled());

    // Reset
    ks.reset().unwrap();
    assert!(!ks.is_halted());
    assert!(!ks.cancellation_token().is_cancelled());
    assert_eq!(ks.get_status().state, KillSwitchState::Armed);
    assert!(ks.get_status().halt_reason.is_none());
}
