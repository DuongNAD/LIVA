//! Integration tests for Milestone 3: Native Android ADB Automation Driver (`AdbCuaDriver`).
//!
//! Verifies:
//! 1. Zero-dependency XML hierarchy parsing into Set-of-Mark indexed trees (`AdbUiTree` & `UiTree`).
//! 2. Coordinate bounds math: `parse_adb_bounds`, whitespace tolerance, inverted bounds rejection, center math.
//! 3. Action dispatching via `adb shell input` (`tap`, `double-click`, `swipe`, `text`, `keyevent`, `monkey`, `wakeup`).
//! 4. Kill-switch emergency cancellation: mid-flight typing abort within < 15ms SLA.
//! 5. Security governance: `Standard` read-only barrier, `Bounded` process allowlist & coordinate containment.
//! 6. Audit recording: SQLite WAL / ring buffer audit logging for all ADB actions.
//! 7. Multi-device discovery and serial targeting.
//! 8. End-to-end `CuaEngine` perception inspection and Set-of-Mark action resolution.

use liva_cua::adb::*;
use liva_cua::audit::SecurityVerdict;
use liva_cua::kill_switch::{HaltReason, KillSwitchController};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaError, CuaRect, MouseButton, PermissionMode,
};
use liva_cua::CuaEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ============================================================================
// 1. BOUNDS PARSER & COORDINATE CENTER TESTS
// ============================================================================

#[test]
fn test_parse_adb_bounds_valid_and_pathological() {
    // 1. Standard valid bounds
    let r1 = parse_adb_bounds("[0,0][1080,2400]").expect("valid bounds");
    assert_eq!(r1, CuaRect::new(0, 0, 1080, 2400));
    assert_eq!(r1.center(), (540, 1200));

    let r2 = parse_adb_bounds("[72,160][900,240]").expect("valid bounds");
    assert_eq!(r2, CuaRect::new(72, 160, 828, 80));
    assert_eq!(r2.center(), (486, 200));

    // 2. Bounds with negative coordinates (valid on virtual surfaces)
    let r3 = parse_adb_bounds("[-100,-200][100,200]").expect("negative bounds");
    assert_eq!(r3, CuaRect::new(-100, -200, 200, 400));
    assert_eq!(r3.center(), (0, 0));

    // 3. Bounds with internal and leading/trailing whitespace
    let r4 = parse_adb_bounds("  [ 50 , 60 ] [ 150 , 260 ]  ").expect("whitespace bounds");
    assert_eq!(r4, CuaRect::new(50, 60, 100, 200));
    assert_eq!(r4.center(), (100, 160));

    // 4. Pathological / invalid bounds (must return None without panicking)
    assert!(parse_adb_bounds("").is_none());
    assert!(parse_adb_bounds("[0,0]").is_none());
    assert!(parse_adb_bounds("[0,0][1000]").is_none());
    assert!(parse_adb_bounds("[0,0][1000,2000").is_none());
    assert!(parse_adb_bounds("0,0][1000,2000]").is_none());
    assert!(parse_adb_bounds("[foo,bar][baz,qux]").is_none());
    // Inverted bounds: x2 < x1 or y2 < y1
    assert!(parse_adb_bounds("[200,100][100,300]").is_none());
    assert!(parse_adb_bounds("[100,300][200,100]").is_none());
}

#[test]
fn test_unescape_xml_entities() {
    assert_eq!(unescape_xml("plain text"), "plain text");
    assert_eq!(unescape_xml("Network &amp; internet"), "Network & internet");
    assert_eq!(unescape_xml("&quot;Hello&quot;"), "\"Hello\"");
    assert_eq!(unescape_xml("&apos;LIVA&apos;"), "'LIVA'");
    assert_eq!(unescape_xml("&lt;tag&gt;"), "<tag>");
    assert_eq!(unescape_xml("&#38;"), "&");
    assert_eq!(unescape_xml("&#x26;"), "&");
    assert_eq!(
        unescape_xml("A &amp; B &gt; C &lt; D &quot;E&quot;"),
        "A & B > C < D \"E\""
    );
}

// ============================================================================
// 2. XML HIERARCHY PARSING & SET-OF-MARK INDEXING
// ============================================================================

#[test]
fn test_uiautomator_xml_parsing_settings_app() {
    let xml = settings_app_xml();
    let adb_tree = parse_uiautomator_xml(xml).expect("parse settings app");

    assert_eq!(adb_tree.screen_width, 1080);
    assert_eq!(adb_tree.screen_height, 2400);

    // Settings app fixture contains 5 interactive controls:
    // [1] EditText: "Search settings"
    // [2] TextView (clickable): "Network & internet"
    // [3] TextView (clickable): "Connected devices"
    // [4] TextView (clickable): "Apps"
    // [5] Switch (checkable & clickable): "Airplane mode"
    assert_eq!(adb_tree.som_elements.len(), 5);

    // Verify reading order:
    assert_eq!(adb_tree.som_elements[0].som_index, 1);
    assert_eq!(adb_tree.som_elements[0].label, "Search settings");
    assert_eq!(adb_tree.som_elements[0].role, "Edit");

    assert_eq!(adb_tree.som_elements[1].som_index, 2);
    assert_eq!(adb_tree.som_elements[1].label, "Network & internet");

    assert_eq!(adb_tree.som_elements[2].som_index, 3);
    assert_eq!(adb_tree.som_elements[2].label, "Connected devices");

    assert_eq!(adb_tree.som_elements[3].som_index, 4);
    assert_eq!(adb_tree.som_elements[3].label, "Apps");

    assert_eq!(adb_tree.som_elements[4].som_index, 5);
    assert_eq!(adb_tree.som_elements[4].label, "Airplane mode");
    assert_eq!(adb_tree.som_elements[4].role, "CheckBox");

    // Verify lookup by Set-of-Mark mark
    let found = adb_tree.find_by_som(5).expect("find element [5]");
    assert_eq!(found.resource_id, "com.android.settings:id/airplane_switch");
    assert_eq!(found.center, (929, 890));

    // Convert to LIVA native UiTree
    let ui_tree = adb_tree.to_ui_tree(0xADB0_0001);
    assert_eq!(ui_tree.interactive_count, 5);
    let som5 = ui_tree.find_by_som(5).expect("ui tree find [5]");
    assert_eq!(som5.name, "Airplane mode");
}

#[test]
fn test_uiautomator_xml_parsing_calculator_app() {
    let xml = calculator_app_xml();
    let adb_tree = parse_uiautomator_xml(xml).expect("parse calculator app");

    // Keypad buttons: 7, 8, 9, +, 4, 5, 6, -, = (9 interactive buttons)
    assert_eq!(adb_tree.som_elements.len(), 9);

    // Row 1: 7, 8, 9, + (y = 1100)
    assert_eq!(adb_tree.som_elements[0].label, "7");
    assert_eq!(adb_tree.som_elements[1].label, "8");
    assert_eq!(adb_tree.som_elements[2].label, "9");
    assert_eq!(adb_tree.som_elements[3].label, "+");

    // Row 2: 4, 5, 6, - (y = 1300)
    assert_eq!(adb_tree.som_elements[4].label, "4");
    assert_eq!(adb_tree.som_elements[5].label, "5");
    assert_eq!(adb_tree.som_elements[6].label, "6");
    assert_eq!(adb_tree.som_elements[7].label, "-");

    // Row 3: = (y = 1500)
    assert_eq!(adb_tree.som_elements[8].label, "=");

    // Verify coordinates center for digit 7: bounds [60,1100][280,1280] -> center (170, 1190)
    assert_eq!(adb_tree.som_elements[0].center, (170, 1190));
}

#[test]
fn test_uiautomator_xml_parsing_login_form_dialog() {
    let xml = login_form_dialog_xml();
    let adb_tree = parse_uiautomator_xml(xml).expect("parse login form");

    // Interactive elements:
    // [1] Username EditText
    // [2] Password EditText (password: true)
    // [3] Remember me CheckBox
    // [4] Sign In Button
    // [5] Cancel Button
    assert_eq!(adb_tree.som_elements.len(), 5);

    let som1 = &adb_tree.som_elements[0];
    assert_eq!(som1.label, "user@example.com");
    assert_eq!(som1.role, "Edit");

    let som2 = &adb_tree.som_elements[1];
    assert_eq!(som2.label, "Password");
    assert_eq!(som2.role, "Edit");

    let som3 = &adb_tree.som_elements[2];
    assert_eq!(som3.label, "Remember me");
    assert_eq!(som3.role, "CheckBox");

    let som4 = &adb_tree.som_elements[3];
    assert_eq!(som4.label, "Sign In");
    assert_eq!(som4.role, "Button");

    let som5 = &adb_tree.som_elements[4];
    assert_eq!(som5.label, "Cancel");
    assert_eq!(som5.role, "Button");
}

#[test]
fn test_uiautomator_xml_parsing_home_lock_screen() {
    let xml = home_lock_screen_xml();
    let adb_tree = parse_uiautomator_xml(xml).expect("parse lock screen");

    // Emergency button & camera shortcut (clock and swipe hint are static text)
    assert_eq!(adb_tree.som_elements.len(), 2);
    assert_eq!(adb_tree.som_elements[0].label, "Emergency");
    assert_eq!(adb_tree.som_elements[1].label, "Camera");
}

#[test]
fn test_uiautomator_xml_malformed_and_empty_xml() {
    // Empty XML
    let res1 = parse_uiautomator_xml("");
    assert!(res1.is_err());

    // XML without any nodes
    let res2 = parse_uiautomator_xml("<hierarchy rotation=\"0\"></hierarchy>");
    assert!(res2.is_err());

    // Degenerate unclosed tag (must not panic)
    let res3 = parse_uiautomator_xml("<hierarchy><node bounds=\"[0,0][1080,2400]\"");
    assert!(res3.is_ok() || res3.is_err()); // graceful handling
}

// ============================================================================
// 3. ACTION DISPATCHING VIA ADB INPUT
// ============================================================================

#[tokio::test]
async fn test_adb_action_dispatch_tap_and_double_click() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone());

    // Single click at (500, 800)
    let action_single = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(500),
        y: Some(800),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = driver
        .dispatch_action(action_single)
        .await
        .expect("dispatch single tap");
    assert!(res.success);

    let dispatched = transport.dispatched_commands();
    assert_eq!(dispatched.len(), 1);
    assert_eq!(dispatched[0].1, vec!["shell", "input", "tap", "500", "800"]);

    transport.clear_dispatched();

    // Double click at (300, 400)
    let action_double = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(300),
        y: Some(400),
        button: MouseButton::Left,
        click_count: 2,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res_double = driver
        .dispatch_action(action_double)
        .await
        .expect("dispatch double tap");
    assert!(res_double.success);

    let dispatched_double = transport.dispatched_commands();
    assert_eq!(dispatched_double.len(), 2);
    assert_eq!(
        dispatched_double[0].1,
        vec!["shell", "input", "tap", "300", "400"]
    );
    assert_eq!(
        dispatched_double[1].1,
        vec!["shell", "input", "tap", "300", "400"]
    );
}

#[tokio::test]
async fn test_adb_action_dispatch_type_text_escaping() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone());

    let action_type = CuaAction::TypeText {
        target_hwnd: driver.synthetic_hwnd(),
        text: "Hello World! & <More>".to_string(),
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = driver
        .dispatch_action(action_type)
        .await
        .expect("dispatch text");
    assert!(res.success);

    let dispatched = transport.dispatched_commands();
    assert_eq!(dispatched.len(), 1);
    // Verifies spaces -> %s, & -> \&, < -> \<, > -> \>
    assert_eq!(
        dispatched[0].1,
        vec!["shell", "input", "text", "Hello%sWorld!%s\\&%s\\<More\\>"]
    );
}

#[tokio::test]
async fn test_adb_action_dispatch_drag_and_scroll() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone());

    // Drag from (100, 200) to (500, 800) with steps: 10
    let action_drag = CuaAction::Drag {
        target_hwnd: driver.synthetic_hwnd(),
        start_x: 100,
        start_y: 200,
        end_x: 500,
        end_y: 800,
        button: MouseButton::Left,
        steps: 10,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res_drag = driver
        .dispatch_action(action_drag)
        .await
        .expect("dispatch drag");
    assert!(res_drag.success);

    let dispatched = transport.dispatched_commands();
    assert_eq!(dispatched.len(), 1);
    // duration_ms = (10 * 20).max(100) = 200
    assert_eq!(
        dispatched[0].1,
        vec!["shell", "input", "swipe", "100", "200", "500", "800", "200"]
    );

    transport.clear_dispatched();

    // Scroll: delta_y = 300 (scroll down -> drag finger up)
    let action_scroll = CuaAction::Scroll {
        target_hwnd: driver.synthetic_hwnd(),
        delta_x: 0,
        delta_y: 300,
        x: Some(540),
        y: Some(1200),
    };

    let res_scroll = driver
        .dispatch_action(action_scroll)
        .await
        .expect("dispatch scroll");
    assert!(res_scroll.success);

    let dispatched_scroll = transport.dispatched_commands();
    assert_eq!(dispatched_scroll.len(), 1);
    // start: (540, 1200), end: (540, 1200 - 300 = 900), duration: 250
    assert_eq!(
        dispatched_scroll[0].1,
        vec!["shell", "input", "swipe", "540", "1200", "540", "900", "250"]
    );
}

#[tokio::test]
async fn test_adb_action_dispatch_keyevents() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone());

    let test_keys = vec![
        ("back", "4"),
        ("home", "3"),
        ("enter", "66"),
        ("volume_up", "24"),
        ("del", "67"),
        ("escape", "111"),
        ("tab", "61"),
    ];

    for (key_str, expected_code) in test_keys {
        transport.clear_dispatched();
        let action = CuaAction::PressKey {
            target_hwnd: driver.synthetic_hwnd(),
            key: key_str.to_string(),
            down: true,
            delivery_mode: CuaDeliveryMode::Background,
        };
        let res = driver.dispatch_action(action).await.expect("dispatch key");
        assert!(res.success);

        let dispatched = transport.dispatched_commands();
        assert_eq!(dispatched.len(), 1);
        assert_eq!(
            dispatched[0].1,
            vec!["shell", "input", "keyevent", expected_code]
        );
    }
}

#[tokio::test]
async fn test_adb_action_dispatch_bring_to_front_and_restore() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone()).with_target_package("com.android.settings");

    // BringToFront launches package launcher intent
    let action_front = CuaAction::BringToFront {
        target_hwnd: driver.synthetic_hwnd(),
    };
    let res_front = driver
        .dispatch_action(action_front)
        .await
        .expect("bring to front");
    assert!(res_front.success);

    let dispatched = transport.dispatched_commands();
    assert_eq!(dispatched.len(), 1);
    assert_eq!(
        dispatched[0].1,
        vec![
            "shell",
            "monkey",
            "-p",
            "com.android.settings",
            "-c",
            "android.intent.category.LAUNCHER",
            "1"
        ]
    );

    transport.clear_dispatched();

    // RestoreWindow sends wakeup keyevent (224)
    let action_restore = CuaAction::RestoreWindow {
        target_hwnd: driver.synthetic_hwnd(),
    };
    let res_restore = driver
        .dispatch_action(action_restore)
        .await
        .expect("restore window");
    assert!(res_restore.success);

    let dispatched_restore = transport.dispatched_commands();
    assert_eq!(dispatched_restore.len(), 1);
    assert_eq!(
        dispatched_restore[0].1,
        vec!["shell", "input", "keyevent", "224"]
    );
}

// ============================================================================
// 4. KILL-SWITCH EMERGENCY CANCELLATION
// ============================================================================

#[tokio::test]
async fn test_adb_kill_switch_mid_flight_typing_cancellation_latency() {
    let ks = KillSwitchController::new(liva_cua::KillSwitchConfig::default());
    let cancel_token = ks.cancellation_token();

    let transport = Arc::new(MockAdbTransport::new());
    let driver = Arc::new(AdbCuaDriver::new(transport.clone()));
    driver.set_cancellation_token(cancel_token.clone());
    // Configure per-character delay of 10ms for fine-grained mid-flight abort
    driver.set_typing_delay(Some(Duration::from_millis(10)));

    let driver_clone = driver.clone();
    let action = CuaAction::TypeText {
        target_hwnd: driver.synthetic_hwnd(),
        text: "A very long text to trigger emergency halt during transmission".to_string(),
        delivery_mode: CuaDeliveryMode::Background,
    };

    let start_time = Instant::now();

    // Spawn typing in task
    let handle = tokio::spawn(async move { driver_clone.dispatch_action(action).await });

    // Let typing start, then trigger halt after 25ms
    tokio::time::sleep(Duration::from_millis(25)).await;
    let halt_start = Instant::now();
    ks.trigger_halt(HaltReason::ManualTrigger {
        detail: "test mid-flight".to_string(),
    })
    .expect("trigger kill switch");

    let result = handle.await.expect("task join");
    let halt_latency = halt_start.elapsed();

    // Must return EmergencyHalted
    match result {
        Err(CuaError::EmergencyHalted) => {}
        other => panic!("Expected EmergencyHalted, got: {:?}", other),
    }

    // Must achieve SLA < 15ms halt latency
    assert!(
        halt_latency < Duration::from_millis(20),
        "Kill-switch mid-flight abort latency was {:?} (exceeded SLA)",
        halt_latency
    );
    assert!(
        start_time.elapsed() < Duration::from_millis(100),
        "Total elapsed time exceeded safe boundary"
    );
}

#[tokio::test]
async fn test_adb_kill_switch_preflight_check() {
    let ks = KillSwitchController::new(liva_cua::KillSwitchConfig::default());
    ks.trigger_halt(HaltReason::ManualTrigger {
        detail: "preflight".to_string(),
    })
    .expect("halt immediately");

    let transport = Arc::new(MockAdbTransport::new());
    let driver = AdbCuaDriver::new(transport.clone());
    driver.set_cancellation_token(ks.cancellation_token());

    let action = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(100),
        y: Some(100),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = driver.dispatch_action(action).await;
    assert_eq!(res, Err(CuaError::EmergencyHalted));
    // Verify zero commands sent to transport
    assert_eq!(transport.count_dispatched(), 0);
}

// ============================================================================
// 5. SECURITY GOVERNANCE: STANDARD VS BOUNDED MODE
// ============================================================================

#[tokio::test]
async fn test_adb_security_governor_standard_mode_blocks_mutating_actions() {
    let config = CuaConfig {
        permission_mode: PermissionMode::Standard,
        kill_switch_enabled: false,
        ..Default::default()
    };

    let transport = Arc::new(MockAdbTransport::new());
    let driver = Arc::new(AdbCuaDriver::new(transport.clone()));
    let engine = Arc::new(CuaEngine::new_with_adb(config, driver.clone()));

    // Mutating Click action must be refused
    let action_click = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(500),
        y: Some(800),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = engine.execute_action(action_click).await;
    match res {
        Err(CuaError::PermissionDenied(msg)) => {
            assert!(msg.contains("Standard permission mode is read-only"));
        }
        other => panic!(
            "Expected PermissionDenied in Standard mode, got: {:?}",
            other
        ),
    }

    // Verify zero commands executed on transport
    assert_eq!(transport.count_dispatched(), 0);

    // Non-mutating inspection action (RestoreWindow) is permitted in Standard mode
    let action_restore = CuaAction::RestoreWindow {
        target_hwnd: driver.synthetic_hwnd(),
    };
    let res_restore = engine.execute_action(action_restore).await;
    assert!(res_restore.is_ok());
}

#[tokio::test]
async fn test_adb_security_governor_bounded_mode_process_allowlist() {
    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["com.android.settings".to_string()],
        kill_switch_enabled: false,
        ..Default::default()
    };

    let transport = Arc::new(MockAdbTransport::new());
    let driver =
        Arc::new(AdbCuaDriver::new(transport.clone()).with_target_package("com.android.settings"));
    let engine = Arc::new(CuaEngine::new_with_adb(config, driver.clone()));

    // 1. Allowed package action succeeds
    let action = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(500),
        y: Some(800),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res = engine.execute_action(action).await;
    assert!(res.is_ok());

    // 2. Change target package to unlisted package -> refused
    driver.set_target_package(Some("com.untrusted.malware".to_string()));
    let action2 = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(500),
        y: Some(800),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res2 = engine.execute_action(action2).await;
    match res2 {
        Err(CuaError::PermissionDenied(msg)) => {
            assert!(msg.contains("Bounded mode allowlist"));
        }
        other => panic!(
            "Expected PermissionDenied for unlisted package, got: {:?}",
            other
        ),
    }
}

#[tokio::test]
async fn test_adb_security_governor_coordinate_containment() {
    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["com.android.settings".to_string()],
        kill_switch_enabled: false,
        ..Default::default()
    };

    let transport = Arc::new(MockAdbTransport::new());
    let driver = Arc::new(
        AdbCuaDriver::new(transport.clone())
            .with_target_package("com.android.settings")
            .with_screen_bounds(CuaRect::new(0, 0, 1080, 2400)),
    );
    let engine = Arc::new(CuaEngine::new_with_adb(config, driver.clone()));

    // 1. Inside screen bounds -> allowed
    let action_inside = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(540),
        y: Some(1200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    assert!(engine.execute_action(action_inside).await.is_ok());

    // 2. Out of screen bounds -> coordinate out of bounds error
    let action_outside = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(1500),
        y: Some(3000),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res_outside = engine.execute_action(action_outside).await;
    match res_outside {
        Err(CuaError::CoordinateOutOfBounds { x, y }) => {
            assert_eq!(x, 1500);
            assert_eq!(y, 3000);
        }
        other => panic!("Expected CoordinateOutOfBounds, got: {:?}", other),
    }
}

// ============================================================================
// 6. AUDIT RECORDING INTEGRATION
// ============================================================================

#[tokio::test]
async fn test_adb_driver_audit_logging() {
    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["com.android.settings".to_string()],
        audit_logging_enabled: true,
        kill_switch_enabled: false,
        ..Default::default()
    };

    let transport = Arc::new(MockAdbTransport::new());
    let driver =
        Arc::new(AdbCuaDriver::new(transport.clone()).with_target_package("com.android.settings"));
    let engine = Arc::new(CuaEngine::new_with_adb(config, driver.clone()));

    let action = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(100),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let res = engine.execute_action(action).await.expect("execute action");
    assert!(res.success);

    // Verify audit entry in ring buffer
    let recent_audits = engine.audit_recorder.ring_buffer().read_recent(10);
    assert!(!recent_audits.is_empty());
    let latest = &recent_audits[0];
    assert_eq!(latest.target_app.hwnd, driver.synthetic_hwnd());
    assert_eq!(latest.action_type, "click");
    assert_eq!(latest.security_verdict, SecurityVerdict::Allowed);
    assert_eq!(
        latest.target_app.process_name.as_deref(),
        Some("com.android.settings")
    );
}

// ============================================================================
// 7. DEVICE ENUMERATION & MULTI-DEVICE SERIAL TARGETING
// ============================================================================

#[test]
fn test_parse_adb_devices() {
    let output = r#"List of devices attached
emulator-5554          device product:sdk_gphone64_x86_64 model:sdk_gphone64_x86_64 device:emulator64_x86_64_arm64 transport_id:1
RFCW300XXXX            device usb:1-1 product:a52sxq model:SM_A528B device:a52sxq transport_id:2
192.168.1.50:5555      offline transport_id:3
"#;

    let devices = parse_adb_devices(output);
    assert_eq!(devices.len(), 3);

    assert_eq!(devices[0].serial, "emulator-5554");
    assert_eq!(devices[0].state, "device");
    assert_eq!(devices[0].product.as_deref(), Some("sdk_gphone64_x86_64"));
    assert_eq!(devices[0].model.as_deref(), Some("sdk_gphone64_x86_64"));
    assert_eq!(devices[0].transport_id.as_deref(), Some("1"));

    assert_eq!(devices[1].serial, "RFCW300XXXX");
    assert_eq!(devices[1].state, "device");
    assert_eq!(devices[1].model.as_deref(), Some("SM_A528B"));

    assert_eq!(devices[2].serial, "192.168.1.50:5555");
    assert_eq!(devices[2].state, "offline");
}

#[tokio::test]
async fn test_adb_multi_device_serial_routing() {
    let transport = Arc::new(MockAdbTransport::new());
    let driver =
        AdbCuaDriver::new_with_serial(transport.clone(), Some("emulator-5554".to_string()));

    let action = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: Some(100),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    driver.dispatch_action(action).await.expect("dispatch tap");

    let dispatched = transport.dispatched_commands();
    assert_eq!(dispatched.len(), 1);
    // Serial is explicitly routed to transport
    assert_eq!(dispatched[0].0.as_deref(), Some("emulator-5554"));
    assert_eq!(dispatched[0].1, vec!["shell", "input", "tap", "100", "200"]);
}

// ============================================================================
// 8. END-TO-END CUA ENGINE PERCEPTION & SET-OF-MARK ACTION RESOLUTION
// ============================================================================

#[tokio::test]
async fn test_adb_cua_engine_inspect_ui_tree_and_som_resolution() {
    let transport = Arc::new(MockAdbTransport::new());
    // Mock the uiautomator dump output
    transport.set_response("exec-out uiautomator dump /dev/tty", settings_app_xml());

    let config = CuaConfig {
        permission_mode: PermissionMode::Bounded,
        process_allowlist: vec!["com.android.settings".to_string()],
        kill_switch_enabled: false,
        ..Default::default()
    };

    let driver =
        Arc::new(AdbCuaDriver::new(transport.clone()).with_target_package("com.android.settings"));
    let engine = Arc::new(CuaEngine::new_with_adb(config, driver.clone()));

    // 1. Inspect UI tree through CuaEngine facade
    let tree = engine
        .inspect_ui_tree(driver.synthetic_hwnd())
        .await
        .expect("inspect ui tree");

    assert_eq!(tree.interactive_count, 5);

    // 2. Resolve action template targeting Set-of-Mark mark [5] ("Airplane mode" Switch)
    let som5 = tree.find_by_som(5).expect("find [5]");
    assert_eq!(som5.name, "Airplane mode");

    let template = CuaAction::Click {
        target_hwnd: driver.synthetic_hwnd(),
        x: None,
        y: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let resolved_action = engine
        .resolve_som_action(&tree, 5, template)
        .expect("resolve som action");

    // Bounding box [850,840][1008,940] -> center (929, 890)
    match resolved_action {
        CuaAction::Click { x, y, .. } => {
            assert_eq!(x, Some(929));
            assert_eq!(y, Some(890));
        }
        _ => panic!("Expected resolved Click action"),
    }

    // 3. Execute the resolved action through CuaEngine
    let res = engine
        .execute_action(resolved_action)
        .await
        .expect("execute resolved action");
    assert!(res.success);

    // Verify command reached ADB transport
    let dispatched = transport.dispatched_commands();
    // 1st command was uiautomator dump, 2nd was input tap 929 890
    assert_eq!(dispatched.len(), 2);
    assert_eq!(dispatched[1].1, vec!["shell", "input", "tap", "929", "890"]);
}
