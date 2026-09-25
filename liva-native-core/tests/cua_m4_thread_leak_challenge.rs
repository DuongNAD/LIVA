//! Empirical Adversarial Challenge Suite — Milestone M4 (Challenger 2)
//!
//! Evaluates:
//! 1. Thread Leak & Poller Lifecycle:
//!    - Verify `AppState::mock_cua()` creates a safe engine with Esc poller stopped.
//!    - Verify repeated creation & destruction of `AppState::mock_cua()` does not leak OS threads.
//!    - Verify `CuaEngine::shutdown()` triggers emergency halt and cleanly joins/stops the physical Esc poller thread.
//!    - Verify shutdown idempotency and thread-safety under concurrent callers.
//!    - Verify RAII `Drop` implementation on `CuaEngine` and `KillSwitchController` safely cleans up background threads.
//! 2. Subsystem Non-Regression:
//!    - Full `AppState` lifecycle stress across 50 consecutive instantiations with CUA IPC execution.

use liva_cua::CuaEngine;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaError, CuaRect, CuaWindowInfo, MouseButton,
    PermissionMode,
};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{AppState, CommandPrincipal, handle_command_as, stt, tts};
use serde_json::json;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helper: Construct an isolated full AppState for lifecycle stress testing
// ---------------------------------------------------------------------------
fn build_stress_app_state() -> Arc<AppState> {
    let db = liva_native_core::db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

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
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

// ---------------------------------------------------------------------------
// 1. Thread Leak & Poller Lifecycle Tests
// ---------------------------------------------------------------------------

#[test]
fn test_mock_cua_poller_immediately_inactive() {
    let engine = AppState::mock_cua();
    let status = engine.kill_switch.get_status();

    assert!(
        !status.is_poller_active,
        "AppState::mock_cua() MUST have poller stopped immediately to avoid thread leaks in tests"
    );
    assert!(
        !status.is_halted,
        "AppState::mock_cua() must start in unhalted Armed state"
    );
}

#[test]
fn test_mock_cua_sync_repeated_instantiation_no_thread_leak() {
    const CYCLES: usize = 100;

    for i in 0..CYCLES {
        let engine = AppState::mock_cua();
        assert!(
            !engine.kill_switch.get_status().is_poller_active,
            "Cycle {i}: Poller must be inactive"
        );
        drop(engine);
    }

    // Allow any trailing async channels to flush
    std::thread::sleep(Duration::from_millis(50));
}

#[tokio::test]
async fn test_mock_cua_async_repeated_instantiation_no_leak() {
    const CYCLES: usize = 100;

    for i in 0..CYCLES {
        let engine = AppState::mock_cua();
        assert!(
            !engine.kill_switch.get_status().is_poller_active,
            "Async cycle {i}: Poller must be inactive"
        );
        let status = engine.get_status().await;
        assert_eq!(status.active_actions, 0);
        drop(engine);
    }
}

#[test]
fn test_cua_engine_shutdown_stops_esc_poller_cleanly() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4000,
        pid: 1234,
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

    let config = CuaConfig {
        kill_switch_enabled: true,
        ..Default::default()
    };

    let engine = CuaEngine::new_with_driver(config, mock);

    // Verify poller thread is running
    assert!(
        engine.kill_switch.get_status().is_poller_active,
        "Esc poller must be active when kill_switch_enabled is true"
    );

    // Measure shutdown SLA (< 50ms)
    let start = Instant::now();
    engine.shutdown();
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_millis(50),
        "CuaEngine::shutdown() took {:?}, must be under 50ms SLA",
        elapsed
    );

    // Verify poller thread is stopped and joined
    let status = engine.kill_switch.get_status();
    assert!(
        !status.is_poller_active,
        "CuaEngine::shutdown() MUST set is_poller_active to false"
    );
    assert!(
        status.is_halted,
        "CuaEngine::shutdown() MUST activate emergency halt"
    );
}

#[tokio::test]
async fn test_cua_engine_shutdown_blocks_subsequent_actions() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5000,
        pid: 5555,
        title: "Target".into(),
        class_name: "TargetClass".into(),
        process_name: Some("test.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
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

    // Shutdown the engine
    engine.shutdown();

    // Verify all subsequent actions fail with EmergencyHalted
    let action = CuaAction::Click {
        target_hwnd: 0x5000,
        x: Some(100),
        y: Some(100),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    let result = engine.execute_action(action).await;
    assert_eq!(
        result.unwrap_err(),
        CuaError::EmergencyHalted,
        "All actions after shutdown() must fail with EmergencyHalted"
    );
}

#[test]
fn test_cua_engine_shutdown_idempotency_and_concurrency() {
    let mock = Arc::new(MockCuaDriver::new());
    let config = CuaConfig {
        kill_switch_enabled: true,
        ..Default::default()
    };
    let engine = Arc::new(CuaEngine::new_with_driver(config, mock));

    // Sequential duplicate shutdowns must not panic or error
    for _ in 0..5 {
        engine.shutdown();
        assert!(!engine.kill_switch.get_status().is_poller_active);
        assert!(engine.kill_switch.is_halted());
    }

    // Concurrent shutdown from 10 threads
    let mut handles = Vec::new();
    for _ in 0..10 {
        let e = Arc::clone(&engine);
        handles.push(std::thread::spawn(move || {
            e.shutdown();
        }));
    }

    for h in handles {
        h.join().expect("Concurrent shutdown thread must not panic");
    }

    assert!(!engine.kill_switch.get_status().is_poller_active);
    assert!(engine.kill_switch.is_halted());
}

#[test]
fn test_repeated_cua_engine_start_and_shutdown_stress() {
    const CYCLES: usize = 50;

    for i in 0..CYCLES {
        let mock = Arc::new(MockCuaDriver::new());
        let config = CuaConfig {
            kill_switch_enabled: true,
            ..Default::default()
        };

        let engine = CuaEngine::new_with_driver(config, mock);
        assert!(
            engine.kill_switch.get_status().is_poller_active,
            "Cycle {i}: Poller must start active"
        );

        engine.shutdown();
        assert!(
            !engine.kill_switch.get_status().is_poller_active,
            "Cycle {i}: Poller must stop after shutdown"
        );

        drop(engine);
    }
}

#[test]
fn test_cua_engine_drop_without_explicit_shutdown_cleans_up_poller() {
    const CYCLES: usize = 30;

    for _ in 0..CYCLES {
        let mock = Arc::new(MockCuaDriver::new());
        let config = CuaConfig {
            kill_switch_enabled: true,
            ..Default::default()
        };
        let engine = CuaEngine::new_with_driver(config, mock);
        let weak_ks = Arc::downgrade(&engine.kill_switch);

        assert!(engine.kill_switch.get_status().is_poller_active);

        // Drop engine without calling shutdown()
        drop(engine);

        // Sleep briefly to confirm thread joined and controller deallocated
        std::thread::sleep(Duration::from_millis(15));
        assert!(
            weak_ks.upgrade().is_none(),
            "KillSwitchController must be deallocated; poller thread exited cleanly"
        );
    }
}

// ---------------------------------------------------------------------------
// 2. Subsystem Non-Regression: AppState Stress Across 50 Cycles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn test_full_appstate_50_cycles_non_regression() {
    const CYCLES: usize = 50;

    for i in 0..CYCLES {
        let state = build_stress_app_state();

        // 1. Verify CUA status through IPC command
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:get_status",
            json!({}),
            None,
            None,
        )
        .await
        .expect("cua:get_status succeeded");

        assert_eq!(res["halted"], false);
        assert_eq!(res["mode"], "bounded");

        // 2. Verify emergency halt & reset
        let halt_res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:emergency_stop",
            json!({}),
            None,
            None,
        )
        .await
        .expect("cua:emergency_stop succeeded");
        assert_eq!(halt_res["halted"], true);

        let reset_res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "cua:emergency_stop",
            json!({ "reset": true }),
            None,
            None,
        )
        .await
        .expect("reset emergency_stop succeeded");
        assert_eq!(reset_res["halted"], false);

        // 3. Verify poller remains inactive across cycles
        assert!(
            !state.cua.kill_switch.get_status().is_poller_active,
            "Cycle {i}: Poller must remain inactive"
        );

        drop(state);
    }
}
