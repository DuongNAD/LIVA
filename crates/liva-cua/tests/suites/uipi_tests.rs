//! UIPI elevation barrier and integrity level tests.

use liva_cua::input::uipi::*;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;

#[test]
fn test_integrity_level_from_rid() {
    assert_eq!(IntegrityLevel::from_rid(0x0000), IntegrityLevel::Untrusted);
    assert_eq!(IntegrityLevel::from_rid(0x1000), IntegrityLevel::Low);
    assert_eq!(IntegrityLevel::from_rid(0x2000), IntegrityLevel::Medium);
    assert_eq!(IntegrityLevel::from_rid(0x2100), IntegrityLevel::MediumPlus);
    assert_eq!(IntegrityLevel::from_rid(0x3000), IntegrityLevel::High);
    assert_eq!(IntegrityLevel::from_rid(0x4000), IntegrityLevel::System);
    assert_eq!(IntegrityLevel::from_rid(0x5000), IntegrityLevel::Protected);
}

#[tokio::test]
async fn test_uipi_elevation_barrier_refusal() {
    let mock = Arc::new(MockCuaDriver::new());
    // Agent process is Medium (0x2000)
    *mock.agent_rid.lock().unwrap() = 0x2000;

    // Window 0x7777 belongs to an elevated High integrity process (0x3000)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x7777,
        pid: 999,
        title: "Administrator Taskmgr".into(),
        class_name: "TaskManagerWindow".into(),
        process_name: Some("taskmgr.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.simulated_uipi_rids
        .lock()
        .unwrap()
        .insert(0x7777, 0x3000);

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    let action = CuaAction::Click {
        target_hwnd: 0x7777,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert!(result.is_err());
    match result.unwrap_err() {
        CuaError::UipiBlocked(msg) => {
            assert!(msg.contains("0x3000"));
        }
        other => panic!("Expected UipiBlocked, got: {:?}", other),
    }
}
