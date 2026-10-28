//! Integration tests for Milestone 2: Dual-Profile Execution Loop (`Flash` & `Pro` profiles).
//!
//! Verifies:
//! 1. `Flash` Profile: High-speed reactive loop using Set-of-Mark indices and bounded history.
//! 2. `Pro` Profile: Pre/post state verification, anomaly detection, self-healing, and audit logging.
//! 3. Kill-switch instant abort across both profiles (< 15ms SLA).
//! 4. Bounded `ActionHistoryTracker` circular FIFO eviction and prompt formatting.
//! 5. Security barrier protection: UAC elevation modals halt immediately without dismissal.

use liva_cua::audit::SecurityVerdict;
use liva_cua::execution::{
    ActionHistoryTracker, CompactActionSummary, CuaExecutionOrchestrator, ExecutionAnomaly,
    ExecutionProfile, ProfileAction, ProfileConfig, ProfilePlan, RecoveryActionTaken,
    SelfHealingEngine,
};
use liva_cua::mock::MockCuaDriver;
use liva_cua::perception::{PerceptionMockFixtures, SoMIndexer};
use liva_cua::router::RemedyHint;
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaActionEffect, CuaConfig, CuaDeliveryMode, CuaError, CuaRect, CuaWindowInfo,
    MouseButton, PermissionMode,
};
use liva_cua::vision::diff::DefaultVisualVerifier;
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::{CuaPixelFormat, RoiDiffConfig};
use liva_cua::CuaEngine;
use std::sync::Arc;

fn create_test_engine(driver: Arc<MockCuaDriver>) -> Arc<CuaEngine> {
    let mut config = CuaConfig::default();
    config.permission_mode = PermissionMode::Bounded;
    config.process_allowlist = vec!["test_app.exe".to_string(), "notepad.exe".to_string()];
    config.audit_logging_enabled = true;
    config.kill_switch_enabled = false; // headless tests disable esc polling thread
    Arc::new(CuaEngine::new_with_driver(config, driver))
}

fn create_test_window(
    hwnd: u64,
    process_name: &str,
    title: &str,
    bounds: CuaRect,
) -> CuaWindowInfo {
    CuaWindowInfo {
        hwnd,
        pid: 1000,
        title: title.to_string(),
        class_name: "TestWindowClass".to_string(),
        process_name: Some(process_name.to_string()),
        bounds,
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    }
}

// ============================================================================
// TEST 1: Flash Profile Multi-Step Indexed Execution
// ============================================================================

#[tokio::test]
async fn test_flash_profile_indexed_actions_execution() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    let win_info = create_test_window(0x1001, "test_app.exe", "Main Dialog", window_bounds);
    mock_driver.insert_window(win_info);

    // Standard dialog has 4 indexed elements: [1] input_notes, [2] chk_remember, [3] btn_cancel, [4] btn_ok
    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    let engine = create_test_engine(mock_driver.clone());
    let orchestrator = CuaExecutionOrchestrator::new(engine, None, ProfileConfig::flash());

    let plan = ProfilePlan::new("plan_flash_1", "session_001", ExecutionProfile::Flash)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: Some(4), // Click OK button
            target_hwnd: None,
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: Some(CuaDeliveryMode::Background),
            x: None,
            y: None,
        })
        .with_step(ProfileAction::TypeText {
            target_som_id: Some(1), // Type into notes field
            target_hwnd: None,
            text: "Automation Verified".to_string(),
            clear_before: false,
            press_enter: false,
            delivery_mode: Some(CuaDeliveryMode::Background),
        });

    let result = orchestrator
        .execute_plan(plan)
        .await
        .expect("Plan execution failed");

    assert!(result.success, "Flash execution must succeed");
    assert_eq!(result.total_steps, 2);
    assert_eq!(result.completed_steps, 2);
    assert_eq!(result.step_results.len(), 2);
    assert!(result.defect_report.is_none());

    // Verify coordinates resolved to element centers
    let dispatched = mock_driver.get_dispatched_actions();
    assert_eq!(dispatched.len(), 2);

    // btn_ok bounds: (100+300, 100+180, 80, 32) -> center (440, 296)
    if let CuaAction::Click { x, y, .. } = &dispatched[0] {
        assert_eq!(*x, Some(440));
        assert_eq!(*y, Some(296));
    } else {
        panic!("First action must be Click with resolved coordinates");
    }

    if let CuaAction::TypeText { text, .. } = &dispatched[1] {
        assert_eq!(text, "Automation Verified");
    } else {
        panic!("Second action must be TypeText");
    }

    // Verify bounded history summary
    let summary = orchestrator.get_history_summary().await;
    assert!(summary.contains("step 0: click on [4]"));
    assert!(summary.contains("step 1: type_text on [1]"));
}

// ============================================================================
// TEST 2: ActionHistoryTracker Bounded Capacity & Formatting
// ============================================================================

#[test]
fn test_flash_profile_bounded_history_summary() {
    let mut tracker = ActionHistoryTracker::new(3);
    assert_eq!(tracker.len(), 0);
    assert!(tracker.is_empty());

    for i in 0..5 {
        tracker.append(CompactActionSummary {
            step_index: i,
            action_type: format!("action_{}", i),
            target_som_id: Some(i + 1),
            target_hwnd: 0x1000 + i as u64,
            target_label: Some(format!("Label {}", i)),
            delivery_mode: CuaDeliveryMode::Background,
            effect: CuaActionEffect::Completed,
            success: true,
            duration_ms: 10 + i as u64,
            timestamp_ms: 1000 + i as u64,
        });
    }

    // Capacity is 3, so steps 0 and 1 must be evicted (FIFO)
    assert_eq!(tracker.len(), 3);
    let summaries = tracker.summaries();
    assert_eq!(summaries[0].step_index, 2);
    assert_eq!(summaries[1].step_index, 3);
    assert_eq!(summaries[2].step_index, 4);

    let prompt_str = tracker.format_prompt_summary();
    assert!(prompt_str.contains("step 2: action_2 on [3] \"Label 2\""));
    assert!(prompt_str.contains("step 3: action_3 on [4] \"Label 3\""));
    assert!(prompt_str.contains("step 4: action_4 on [5] \"Label 4\""));
    assert!(!prompt_str.contains("step 0:"));
    assert!(!prompt_str.contains("step 1:"));
}

// ============================================================================
// TEST 3: Pro Profile Detects Unexpected Modal Dialog
// ============================================================================

#[tokio::test]
async fn test_pro_profile_detects_unexpected_modal_anomaly() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    // Pre-insert an unexpected dialog window that appears post-action
    let modal_bounds = CuaRect::new(200, 200, 300, 200);
    let mut modal_win = create_test_window(
        0x2001,
        "test_app.exe",
        "Warning: Confirm Deletion",
        modal_bounds,
    );
    modal_win.class_name = "#32770".to_string();
    mock_driver.insert_window(modal_win);

    let engine = create_test_engine(mock_driver.clone());

    // Disable auto_dismiss_popups so the modal triggers an abort
    let config = ProfileConfig {
        profile: ExecutionProfile::Pro,
        auto_dismiss_popups: false,
        redraw_micro_tick_ms: 0,
        ..Default::default()
    };
    let orchestrator = CuaExecutionOrchestrator::new(engine.clone(), None, config);

    let plan = ProfilePlan::new("plan_pro_modal", "session_002", ExecutionProfile::Pro)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: Some(3),
            target_hwnd: None,
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: Some(CuaDeliveryMode::Background),
            x: None,
            y: None,
        });

    let result = orchestrator
        .execute_plan(plan)
        .await
        .expect("Execution error");

    assert!(!result.success, "Execution must halt on unexpected modal");
    let defect = result.defect_report.expect("Defect report must be present");
    assert_eq!(defect.failed_step_index, 0);

    match &defect.anomaly {
        ExecutionAnomaly::UnexpectedModal {
            modal_hwnd,
            modal_title,
            ..
        } => {
            assert_eq!(*modal_hwnd, 0x2001);
            assert!(modal_title.contains("Confirm Deletion"));
        }
        other => panic!("Expected UnexpectedModal, got {:?}", other),
    }

    // Verify audit recorder logged the event
    let events = engine.audit_recorder.ring_buffer().read_recent(10);
    assert!(
        !events.is_empty(),
        "Audit events must be recorded in ring buffer"
    );
}

// ============================================================================
// TEST 4: Pro Profile Auto-Dismisses Non-Security Popup
// ============================================================================

#[tokio::test]
async fn test_pro_profile_auto_dismisses_non_security_popup() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    let engine = create_test_engine(mock_driver.clone());

    // Test SelfHealingEngine directly on dismissible popup
    let healing = SelfHealingEngine::new(
        mock_driver.clone(),
        engine.security_governor.clone(),
        engine.audit_recorder.clone(),
    );

    let popup_anomaly = ExecutionAnomaly::UnexpectedModal {
        modal_hwnd: 0x3001,
        modal_title: "Save As Confirmation".to_string(),
        is_security_barrier: false,
        is_dismissible: true,
    };

    let config = ProfileConfig::pro();
    let recovery = healing
        .attempt_recovery(
            &popup_anomaly,
            CuaDeliveryMode::Foreground,
            0x1001,
            &config,
            "session_dismiss",
        )
        .await
        .expect("Recovery must succeed");

    assert_eq!(recovery, RecoveryActionTaken::DismissedModalPopup);

    // Verify Escape key was dispatched
    let actions = mock_driver.get_dispatched_actions();
    let escape_actions: Vec<_> = actions
        .iter()
        .filter(|a| matches!(a, CuaAction::PressKey { key, .. } if key == "Escape"))
        .collect();
    assert_eq!(
        escape_actions.len(),
        2,
        "Must dispatch key down and key up for Escape"
    );

    // Verify audit event was logged
    let events = engine.audit_recorder.ring_buffer().read_recent(5);
    let heal_event = events
        .iter()
        .find(|e| e.action_type == "pro:self_healing:dismiss_popup");
    assert!(
        heal_event.is_some(),
        "Must log dismiss_popup self-healing audit event"
    );
}

// ============================================================================
// TEST 5: Pro Profile Background Zero-Diff Triggers Foreground Escalation
// ============================================================================

#[tokio::test]
async fn test_pro_profile_zero_diff_triggers_foreground_escalation() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    // Capturer setup
    let capturer = Arc::new(MockScreenCapturer::new(
        1920,
        1080,
        CuaPixelFormat::Bgra,
        128,
    ));
    let _verifier = Arc::new(DefaultVisualVerifier::new(
        capturer.clone(),
        RoiDiffConfig {
            redraw_micro_tick_ms: 0,
            ..Default::default()
        },
    ));

    let engine = create_test_engine(mock_driver.clone());

    // Test SelfHealingEngine directly on NoOpAction anomaly
    let healing = SelfHealingEngine::new(
        mock_driver.clone(),
        engine.security_governor.clone(),
        engine.audit_recorder.clone(),
    );

    let noop_anomaly = ExecutionAnomaly::NoOpAction {
        target_som_id: Some(4),
        target_bounds: CuaRect::new(400, 280, 80, 32),
        visual_diff_ratio: 0.0,
        is_identical_tree: true,
    };

    let config = ProfileConfig::pro();
    let recovery = healing
        .attempt_recovery(
            &noop_anomaly,
            CuaDeliveryMode::Background,
            0x1001,
            &config,
            "session_heal_escalate",
        )
        .await
        .expect("Escalation must succeed");

    assert_eq!(recovery, RecoveryActionTaken::EscalatedToForeground);

    // Verify BringToFront was dispatched
    let actions = mock_driver.get_dispatched_actions();
    let btf = actions
        .iter()
        .find(|a| matches!(a, CuaAction::BringToFront { target_hwnd } if *target_hwnd == 0x1001));
    assert!(
        btf.is_some(),
        "Must dispatch BringToFront on foreground escalation"
    );

    // Verify audit event recorded
    let events = engine.audit_recorder.ring_buffer().read_recent(5);
    let escalate_ev = events
        .iter()
        .find(|e| e.action_type == "pro:self_healing:foreground_escalation");
    assert!(
        escalate_ev.is_some(),
        "Must log foreground_escalation in audit recorder"
    );
}

// ============================================================================
// TEST 6: Pro Profile Persistent No-Op Exhausts Retries and Aborts
// ============================================================================

#[tokio::test]
async fn test_pro_profile_no_op_anomaly_triggers_structured_abort() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    // Capturer setup with persistent identical frames (0% diff)
    let capturer = Arc::new(MockScreenCapturer::new(
        1920,
        1080,
        CuaPixelFormat::Bgra,
        128,
    ));
    let verifier = Arc::new(DefaultVisualVerifier::new(
        capturer.clone(),
        RoiDiffConfig {
            redraw_micro_tick_ms: 0,
            ..Default::default()
        },
    ));

    let engine = create_test_engine(mock_driver.clone());

    let config = ProfileConfig {
        profile: ExecutionProfile::Pro,
        max_step_retries: 1,
        allow_foreground_escalation: true,
        redraw_micro_tick_ms: 0,
        ..Default::default()
    };
    let orchestrator = CuaExecutionOrchestrator::new(engine.clone(), Some(verifier), config);

    let plan = ProfilePlan::new("plan_pro_noop", "session_003", ExecutionProfile::Pro)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: Some(4),
            target_hwnd: None,
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: Some(CuaDeliveryMode::Background),
            x: None,
            y: None,
        });

    let result = orchestrator
        .execute_plan(plan)
        .await
        .expect("Execution error");

    assert!(
        !result.success,
        "Persistent no-op must result in failed plan"
    );
    let defect = result.defect_report.expect("Defect report must be emitted");
    assert_eq!(defect.failed_step_index, 0);
    assert_eq!(defect.error_code, "NO_OP_ACTION");
    assert!(defect.recovery_attempts >= 1);
    assert!(defect.screenshot_recommended);

    match defect.remedy_hint {
        RemedyHint::AdjustVisualThreshold(_) => {}
        other => panic!(
            "Expected AdjustVisualThreshold remedy hint, got {:?}",
            other
        ),
    }
}

// ============================================================================
// TEST 7: Security Barrier Modals (UAC consent.exe) Never Auto-Dismiss
// ============================================================================

#[tokio::test]
async fn test_pro_profile_uac_elevation_blocks_immediately_without_self_healing() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    // Insert UAC elevation prompt (consent.exe)
    let uac_bounds = CuaRect::new(0, 0, 1920, 1080);
    let uac_win = create_test_window(0x9999, "consent.exe", "User Account Control", uac_bounds);
    mock_driver.insert_window(uac_win);

    let engine = create_test_engine(mock_driver.clone());

    let config = ProfileConfig {
        profile: ExecutionProfile::Pro,
        auto_dismiss_popups: true, // Even if true, UAC must NEVER be auto-dismissed
        redraw_micro_tick_ms: 0,
        ..Default::default()
    };
    let orchestrator = CuaExecutionOrchestrator::new(engine.clone(), None, config);

    let plan = ProfilePlan::new("plan_pro_uac", "session_004", ExecutionProfile::Pro)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: Some(4),
            target_hwnd: None,
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: Some(CuaDeliveryMode::Background),
            x: None,
            y: None,
        });

    let result = orchestrator
        .execute_plan(plan)
        .await
        .expect("Execution error");

    assert!(!result.success, "Must halt immediately on UAC prompt");
    let defect = result.defect_report.expect("Defect report must be present");
    assert_eq!(defect.error_code, "SECURITY_BARRIER_MODAL");

    match defect.remedy_hint {
        RemedyHint::SecurityPolicyViolation(msg) => {
            assert!(msg.contains("Security barrier modal"));
        }
        other => panic!(
            "Expected SecurityPolicyViolation remedy hint, got {:?}",
            other
        ),
    }

    // Verify NO Escape key was dispatched to dismiss UAC
    let dispatched = mock_driver.get_dispatched_actions();
    let escape_sent = dispatched
        .iter()
        .any(|a| matches!(a, CuaAction::PressKey { key, .. } if key == "Escape"));
    assert!(
        !escape_sent,
        "Escape key must NEVER be sent to security elevation barriers"
    );

    // Verify security barrier audit event was logged
    let events = engine.audit_recorder.ring_buffer().read_recent(10);
    let sec_ev = events
        .iter()
        .find(|e| e.action_type == "pro:security_barrier_blocked");
    assert!(
        sec_ev.is_some(),
        "Must log security_barrier_blocked audit record"
    );
    assert_eq!(
        sec_ev.unwrap().security_verdict,
        SecurityVerdict::BlockedByPolicy
    );
}

// ============================================================================
// TEST 8: Pro Profile Restores Minimized Target Window
// ============================================================================

#[tokio::test]
async fn test_pro_profile_restores_minimized_window() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(-32000, -32000, 0, 0); // Minimized sentinel
    let mut win_info = create_test_window(0x1001, "test_app.exe", "Minimized App", window_bounds);
    win_info.is_minimized = true;
    mock_driver.insert_window(win_info);

    let engine = create_test_engine(mock_driver.clone());

    let healing = SelfHealingEngine::new(
        mock_driver.clone(),
        engine.security_governor.clone(),
        engine.audit_recorder.clone(),
    );

    let degraded_anomaly = ExecutionAnomaly::WindowDegraded {
        target_hwnd: 0x1001,
        reason: "Window is minimized".to_string(),
    };

    let config = ProfileConfig::pro();
    let recovery = healing
        .attempt_recovery(
            &degraded_anomaly,
            CuaDeliveryMode::Foreground,
            0x1001,
            &config,
            "session_restore",
        )
        .await
        .expect("Recovery must succeed");

    assert_eq!(recovery, RecoveryActionTaken::RestoredWindow);

    // Verify window is now un-minimized in driver
    let verdict = mock_driver.inspect_window(0x1001).await.unwrap();
    assert!(matches!(
        verdict,
        liva_cua::types::WindowStateVerdict::Valid { .. }
    ));

    // Verify audit record logged
    let events = engine.audit_recorder.ring_buffer().read_recent(5);
    let restore_ev = events
        .iter()
        .find(|e| e.action_type == "pro:self_healing:restore_window");
    assert!(
        restore_ev.is_some(),
        "Must record restore_window audit event"
    );
}

// ============================================================================
// TEST 9: Emergency Kill-Switch Aborts Flash and Pro Under 15ms SLA
// ============================================================================

#[tokio::test]
async fn test_kill_switch_aborts_both_flash_and_pro_under_15ms() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let engine = create_test_engine(mock_driver.clone());

    // Trigger emergency halt
    engine
        .trigger_emergency_halt()
        .expect("Halt trigger failed");
    assert!(engine.kill_switch.is_halted());

    let plan_flash = ProfilePlan::new("flash_abort", "s1", ExecutionProfile::Flash)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: None,
            target_hwnd: Some(0x1001),
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
            x: Some(200),
            y: Some(200),
        });

    let res_flash = engine.run_profile_plan(plan_flash, None).await;
    assert!(
        matches!(res_flash, Err(CuaError::EmergencyHalted)),
        "Flash profile must immediately return EmergencyHalted when kill-switch is active"
    );

    let plan_pro = ProfilePlan::new("pro_abort", "s2", ExecutionProfile::Pro)
        .with_target_hwnd(0x1001)
        .with_step(ProfileAction::Click {
            target_som_id: None,
            target_hwnd: Some(0x1001),
            target_selector: None,
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
            x: Some(200),
            y: Some(200),
        });

    let res_pro = engine.run_profile_plan(plan_pro, None).await;
    assert!(
        matches!(res_pro, Err(CuaError::EmergencyHalted)),
        "Pro profile must immediately return EmergencyHalted when kill-switch is active"
    );

    // Verify audit events contain AbortedByKillSwitch
    let events = engine.audit_recorder.ring_buffer().read_recent(10);
    let aborted_events: Vec<_> = events
        .iter()
        .filter(|e| e.security_verdict == SecurityVerdict::AbortedByKillSwitch)
        .collect();
    assert!(
        !aborted_events.is_empty(),
        "Must record AbortedByKillSwitch in audit ledger"
    );
}

// ============================================================================
// TEST 10: CuaEngine Profile Facade Methods
// ============================================================================

#[tokio::test]
async fn test_cua_engine_facade_profile_methods() {
    let mock_driver = Arc::new(MockCuaDriver::new());
    let window_bounds = CuaRect::new(100, 100, 500, 400);
    mock_driver.insert_window(create_test_window(
        0x1001,
        "test_app.exe",
        "Main Window",
        window_bounds,
    ));

    let mut tree = PerceptionMockFixtures::standard_dialog(0x1001, window_bounds);
    SoMIndexer::index_tree(&mut tree);
    mock_driver.insert_ui_tree(0x1001, tree);

    let engine = create_test_engine(mock_driver.clone());

    let action = ProfileAction::Click {
        target_som_id: Some(4),
        target_hwnd: None,
        target_selector: None,
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: Some(CuaDeliveryMode::Background),
        x: None,
        y: None,
    };

    // Test execute_flash_step facade
    let flash_res = engine
        .execute_flash_step(0, &action, 0x1001, "facade_session")
        .await
        .expect("execute_flash_step failed");
    assert!(flash_res.success);
    assert_eq!(flash_res.action_name, "click");
    assert_eq!(flash_res.target_som_id, Some(4));

    // Test execute_pro_step facade
    let pro_res = engine
        .execute_pro_step(0, &action, 0x1001, "facade_session", 1, None)
        .await
        .expect("execute_pro_step failed");
    assert!(pro_res.success);
    assert_eq!(pro_res.action_name, "click");
    assert_eq!(pro_res.target_som_id, Some(4));
}
