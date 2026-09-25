//! Comprehensive Integration Test Suite for Vision-Aligned Fast Action Router (System-1 GUI Loop).
//!
//! Verifies:
//! 1. Target coordinate translation across all 7 coordinate spaces.
//! 2. Deterministic pre-checks (out-of-bounds, close button, minimized window, occlusion, security).
//! 3. Localized visual ROI diffing detecting genuine changes within < 80ms SLA.
//! 4. Delivery fallback escalation (Background -> Foreground upon 0% diff).
//! 5. Policy-bounded foreground escalation rejections.
//! 6. Immediate abort upon Emergency Kill-Switch signal (< 15ms).
//! 7. Structured System-2 defect reporting with appropriate remedy hints.
//! 8. Multi-step macro plan execution with 0 token overhead.

use liva_cua::mock::MockCuaDriver;
use liva_cua::router::{
    DefectCategory, MacroPlan, RemedyHint, System1Action, System1ActionRouter, System1RouterConfig,
    TargetCoordinate, TargetDescriptor, TargetResolver, WindowSelector,
};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaPoint, CuaRect, CuaWindowInfo, MouseButton,
    PermissionMode,
};
use liva_cua::vision::diff::{apply_roi_padding, DefaultVisualVerifier, RoiDiffEngine};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::traits::VisualVerifierTrait;
use liva_cua::vision::types::{
    CuaPixelFormat, RawFrame, RoiDiffConfig, SemanticVerificationCriteria,
};
use liva_cua::CuaEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Test harness helper constructing an isolated System1ActionRouter environment.
fn create_test_router(
    perm_mode: PermissionMode,
    allow_fg_escalation: bool,
) -> (
    Arc<System1ActionRouter>,
    Arc<MockCuaDriver>,
    Arc<MockScreenCapturer>,
    Arc<CuaEngine>,
) {
    let cua_config = CuaConfig {
        permission_mode: perm_mode,
        process_allowlist: vec!["notepad.exe".into(), "app.exe".into()],
        ..Default::default()
    };

    let driver = Arc::new(MockCuaDriver::new());
    let engine = Arc::new(CuaEngine::new_with_driver(cua_config, driver.clone()));

    // Seed mock window (100, 100, 800, 600)
    let window = CuaWindowInfo {
        hwnd: 0x1234,
        pid: 1000,
        title: "Test Application".into(),
        class_name: "TestClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window);

    // Mock screen capturer (1920x1080)
    let capturer = Arc::new(MockScreenCapturer::new(
        1920,
        1080,
        CuaPixelFormat::Rgba,
        128,
    ));
    let verifier = Arc::new(DefaultVisualVerifier::new(
        capturer.clone(),
        RoiDiffConfig {
            redraw_micro_tick_ms: 1, // Fast micro-tick for deterministic unit tests
            ..Default::default()
        },
    ));

    let router_config = System1RouterConfig {
        max_step_retries: 2,
        allow_foreground_escalation: allow_fg_escalation,
        redraw_micro_tick_ms: 1,
        step_timeout_ms: 1000,
    };

    let router = Arc::new(System1ActionRouter::new(
        engine.clone(),
        driver.clone(),
        verifier,
        router_config,
    ));

    (router, driver, capturer, engine)
}

// =========================================================================
// 1. Target Coordinate Translation across 7 Coordinate Spaces
// =========================================================================

#[test]
fn test_coordinate_translation_across_all_7_spaces() {
    let window_bounds = CuaRect::new(100, 100, 800, 600);
    let client_bounds = CuaRect::new(108, 138, 784, 554); // standard insets
    let roi = CuaRect::new(200, 200, 100, 50);

    // 1. ClientNormalized: (0.5, 0.5) maps to center of client bounds
    let coord = TargetCoordinate::ClientNormalized { x: 0.5, y: 0.5 };
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("ClientNormalized translation must succeed");
    assert_eq!(c_pt.x, 392);
    assert_eq!(c_pt.y, 277);
    assert_eq!(s_pt.x, 108 + 392);
    assert_eq!(s_pt.y, 138 + 277);

    // 2. WindowNormalized: (0.5, 0.5) maps to center of window bounds
    let coord = TargetCoordinate::WindowNormalized { x: 0.5, y: 0.5 };
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("WindowNormalized translation must succeed");
    assert_eq!(s_pt.x, 500);
    assert_eq!(s_pt.y, 400);
    assert_eq!(c_pt.x, 500 - client_bounds.x);
    assert_eq!(c_pt.y, 400 - client_bounds.y);

    // 3. ScreenNormalized: (0.5, 0.5) maps across virtual desktop
    let coord = TargetCoordinate::ScreenNormalized { x: 0.5, y: 0.5 };
    let (s_pt, _) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("ScreenNormalized translation must succeed");
    let virt = liva_cua::geometry::VirtualDesktopMetrics::query_from_system();
    assert_eq!(s_pt.x, virt.x + (virt.width as f32 * 0.5).round() as i32);
    assert_eq!(s_pt.y, virt.y + (virt.height as f32 * 0.5).round() as i32);

    // 4. AbsoluteClient: relative to client (10, 20)
    let coord = TargetCoordinate::AbsoluteClient(CuaPoint::new(10, 20));
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("AbsoluteClient translation must succeed");
    assert_eq!(c_pt.x, 10);
    assert_eq!(c_pt.y, 20);
    assert_eq!(s_pt.x, client_bounds.x + 10);
    assert_eq!(s_pt.y, client_bounds.y + 20);

    // 5. AbsoluteScreen: screen point (300, 400)
    let coord = TargetCoordinate::AbsoluteScreen(CuaPoint::new(300, 400));
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("AbsoluteScreen translation must succeed");
    assert_eq!(s_pt.x, 300);
    assert_eq!(s_pt.y, 400);
    assert_eq!(c_pt.x, 300 - client_bounds.x);
    assert_eq!(c_pt.y, 400 - client_bounds.y);

    // 6. RoiNormalized: (0.5, 0.5) inside ROI (200, 200, 100, 50)
    let coord = TargetCoordinate::RoiNormalized {
        roi,
        norm_x: 0.5,
        norm_y: 0.5,
    };
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("RoiNormalized translation must succeed");
    assert_eq!(s_pt.x, 250);
    assert_eq!(s_pt.y, 225);
    assert_eq!(c_pt.x, 250 - client_bounds.x);
    assert_eq!(c_pt.y, 225 - client_bounds.y);

    // 7. WindowClientCenter: with expected_roi uses ROI center, without uses client center
    let coord = TargetCoordinate::WindowClientCenter;
    let (s_pt_roi, _) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, Some(&roi))
            .expect("WindowClientCenter with ROI must succeed");
    assert_eq!(s_pt_roi.x, 250);
    assert_eq!(s_pt_roi.y, 225);

    let (s_pt_no_roi, c_pt_no_roi) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("WindowClientCenter without ROI must succeed");
    assert_eq!(c_pt_no_roi.x, client_bounds.width / 2);
    assert_eq!(c_pt_no_roi.y, client_bounds.height / 2);
    assert_eq!(s_pt_no_roi.x, client_bounds.x + client_bounds.width / 2);
    assert_eq!(s_pt_no_roi.y, client_bounds.y + client_bounds.height / 2);
}

// =========================================================================
// 2. Deterministic Pre-Execution Checks
// =========================================================================

#[tokio::test]
async fn test_precheck_out_of_bounds_rejection() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Point outside client drawing surface (x = -5, y = 50)
    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: -5, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Must reject out-of-bounds click");

    assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err.error_code, "coordinate_out_of_bounds");
    assert!(matches!(
        err.remedy_hint,
        RemedyHint::ReacquireTargetCoordinates(_)
    ));
}

#[tokio::test]
async fn test_precheck_close_button_protection() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Target inside the top-right 50px window bounds (bounds: 100, 100, 800, 600 -> x >= 850, y < 140)
    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ScreenPixel { x: 880, y: 110 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Must reject click in close button protection zone");

    assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
    assert!(err.error_message.contains("CloseButtonProtected"));
}

#[tokio::test]
async fn test_precheck_minimized_window_refusal() {
    let (router, driver, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Register minimized window
    let min_window = CuaWindowInfo {
        hwnd: 0x5555,
        pid: 2000,
        title: "Minimized App".into(),
        class_name: "MinClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 160, 24),
        is_on_screen: false,
        is_minimized: true,
        z_index: 3,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(min_window);

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5555),
            TargetCoordinate::ClientPixel { x: 10, y: 10 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Must fail-closed on minimized window without wasting retries");

    assert_eq!(err.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err.error_code, "window_minimized");
    assert_eq!(
        err.retry_count, 0,
        "Pre-check errors must fail closed with 0 retries"
    );
    assert!(matches!(
        err.remedy_hint,
        RemedyHint::RestoreMinimizedWindow(_)
    ));
}

#[tokio::test]
async fn test_precheck_occlusion_detection() {
    let (router, driver, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Insert an overlapping modal window on top of target (z_index: 0, covering screen 150..350, 150..350)
    let modal_window = CuaWindowInfo {
        hwnd: 0x7777,
        pid: 3000,
        title: "Confirmation Dialog".into(),
        class_name: "ModalClass".into(),
        process_name: Some("popup.exe".into()),
        bounds: CuaRect::new(150, 150, 200, 200),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0, // Topmost!
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    // Demote target window to z_index: 1
    let target = driver.inspect_window(0x1234).await.unwrap();
    if let liva_cua::types::WindowStateVerdict::Valid {
        window_info: Some(mut info),
        ..
    } = target
    {
        info.z_index = 1;
        driver.insert_window(info);
    }
    driver.insert_window(modal_window);

    // Target a coordinate covered by the modal (screen 200, 200 -> client x: 92, y: 62)
    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ScreenPixel { x: 200, y: 200 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Must detect z-order occlusion");

    assert_eq!(err.failure_category, DefectCategory::TargetOccluded);
    assert_eq!(err.error_code, "target_occluded");
    assert!(matches!(
        err.remedy_hint,
        RemedyHint::BringWindowToForeground(_)
    ));
}

#[tokio::test]
async fn test_precheck_security_policy_refusal_in_standard_mode() {
    // Create router in Standard (read-only) mode
    let (router, _, _, _) = create_test_router(PermissionMode::Standard, true);

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Standard mode must reject mutating click");

    assert_eq!(err.failure_category, DefectCategory::SecurityRefused);
    assert!(matches!(
        err.remedy_hint,
        RemedyHint::SecurityPolicyViolation(_)
    ));
}

// =========================================================================
// 3. Localized Visual ROI Diffing & Semantic Criteria (< 80ms SLA)
// =========================================================================

#[test]
fn test_roi_padding_bounds_containment() {
    let screen_w = 1920;
    let screen_h = 1080;

    // Normal element
    let rect = CuaRect::new(100, 100, 50, 30);
    let padded = apply_roi_padding(&rect, 16, screen_w, screen_h);
    assert_eq!(padded.x, 84);
    assert_eq!(padded.y, 84);
    assert_eq!(padded.width, 50 + 32);
    assert_eq!(padded.height, 30 + 32);

    // Element near top-left edge: must clamp to 0 without underflow
    let edge_rect = CuaRect::new(5, 5, 20, 20);
    let edge_padded = apply_roi_padding(&edge_rect, 16, screen_w, screen_h);
    assert_eq!(edge_padded.x, 0);
    assert_eq!(edge_padded.y, 0);

    // Element near bottom-right edge: must clamp to max screen dimensions
    let br_rect = CuaRect::new(1910, 1070, 10, 10);
    let br_padded = apply_roi_padding(&br_rect, 16, screen_w, screen_h);
    assert!(br_padded.x + br_padded.width <= 1920);
    assert!(br_padded.y + br_padded.height <= 1080);
}

#[test]
fn test_diff_roi_zero_diff_on_identical_frames() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    let f2 = capturer.create_uniform_frame(128);

    let roi = CuaRect::new(50, 50, 100, 100);
    let criteria = SemanticVerificationCriteria::click_default();

    let result = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, 5, &criteria)
        .expect("Diff scan must succeed");

    assert!(
        !result.diff_detected,
        "Identical frames must produce 0% diff"
    );
    assert_eq!(result.changed_pixels, 0);
    assert_eq!(result.changed_ratio, 0.0);
    assert!(
        result.scan_duration_us < 2000,
        "Scan must complete in < 2ms"
    );
}

#[test]
fn test_diff_roi_click_press_detection() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    // Button state changes from 128 to 220 over a 40x20 area
    let f2 = capturer.create_frame_with_rect(128, 220, 50, 50, 40, 20);

    let roi = CuaRect::new(40, 40, 60, 40); // 2400 pixels total
    let criteria = SemanticVerificationCriteria::click_default(); // min 0.5% or 8px

    let result = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, 5, &criteria)
        .expect("Diff scan must succeed");

    assert!(result.diff_detected, "Button press must be detected");
    assert!(result.changed_pixels >= 800);
    assert!(result.changed_ratio > 0.3);
}

#[test]
fn test_diff_roi_type_text_glyph_detection() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 255);
    let f1 = capturer.create_uniform_frame(255);
    // Caret or glyph adds 8 black pixels inside input box
    let f2 = capturer.create_frame_with_rect(255, 0, 100, 100, 2, 4); // 8 pixels

    let roi = CuaRect::new(90, 90, 150, 30);
    let criteria = SemanticVerificationCriteria::type_text_default(); // min 6px

    let result = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, 5, &criteria)
        .expect("Diff scan must succeed");

    assert!(
        result.diff_detected,
        "Glyph appearance must satisfy TypeText criteria"
    );
    assert_eq!(result.changed_pixels, 8);
}

#[test]
fn test_diff_roi_scroll_displacement_detection() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    // Large container content displacement (100x100 area)
    let f2 = capturer.create_frame_with_rect(128, 180, 200, 200, 100, 100);

    let roi = CuaRect::new(150, 150, 200, 200); // 40,000 pixels
    let criteria = SemanticVerificationCriteria::scroll_default(); // min 3.0% or 50px

    let result = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, 5, &criteria)
        .expect("Diff scan must succeed");

    assert!(result.diff_detected, "Scroll displacement must be detected");
    assert!(result.changed_pixels >= 10000);
}

#[tokio::test]
async fn test_wait_visual_change_early_exit() {
    let capturer = Arc::new(MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128));
    let base = capturer.create_uniform_frame(128);
    let changed = capturer.create_frame_with_rect(128, 220, 50, 50, 40, 40);

    // Frame 1: baseline (128)
    // Frame 2: changed (diff!)
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(changed);

    let verifier = DefaultVisualVerifier::new(
        capturer,
        RoiDiffConfig {
            redraw_micro_tick_ms: 1,
            ..Default::default()
        },
    );

    let roi = CuaRect::new(40, 40, 60, 60);
    let criteria = SemanticVerificationCriteria::WaitVisualChange {
        min_changed_ratio: 0.01,
        poll_interval_ms: 10,
        timeout_ms: 2000,
    };

    let start = Instant::now();
    let result = verifier
        .wait_visual_change(&roi, &criteria, &base)
        .await
        .expect("wait_visual_change must succeed");

    let elapsed = start.elapsed();
    assert!(result.diff_detected, "Visual change must be detected");
    assert!(
        elapsed < Duration::from_millis(500),
        "Must exit immediately on change, not wait full timeout"
    );
}

// =========================================================================
// 4. Delivery Fallback Escalation Protocol (Background -> Foreground)
// =========================================================================

#[tokio::test]
async fn test_delivery_fallback_escalation_on_zero_diff() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Simulate Chromium / Electron canvas:
    // Frame 1: Pre-background baseline
    // Frame 2: Post-background capture -> identical to baseline (0% diff)
    // Frame 3: Pre-foreground baseline
    // Frame 4: Post-foreground capture -> modified rectangle (diff > 0%!)
    let base = capturer.create_uniform_frame(128);
    let changed = capturer.create_frame_with_rect(128, 240, 140, 140, 40, 40);

    capturer.enqueue_frame(base.clone()); // Pre-background
    capturer.enqueue_frame(base.clone()); // Post-background (0% diff)
    capturer.enqueue_frame(base); // Pre-foreground
    capturer.enqueue_frame(changed); // Post-foreground (diff detected!)

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 45, y: 45 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let res = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action, 2, true)
        .await
        .expect("Step must succeed after foreground escalation");

    assert!(res.success);
    assert_eq!(res.delivery_used, CuaDeliveryMode::Foreground);
    assert!(
        res.escalated_from_background,
        "Must record escalation from background"
    );
    assert_eq!(
        res.retries_taken, 1,
        "Must have used 1 retry for foreground escalation"
    );

    // Verify driver received both background and foreground actions
    let dispatched = driver.get_dispatched_actions();
    assert!(dispatched.len() >= 2);
}

#[tokio::test]
async fn test_foreground_escalation_blocked_when_disabled() {
    // Router configured with allow_foreground_escalation = false
    let (router, _, capturer, _) = create_test_router(PermissionMode::Bounded, false);

    // Background produces 0% diff
    let base = capturer.create_uniform_frame(128);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(base);

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 45, y: 45 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action, 2, false)
        .await
        .expect_err("Must generate defect report when foreground escalation is disallowed");

    assert_eq!(
        err.failure_category,
        DefectCategory::VisualVerificationZeroDiff
    );
    assert!(err.screenshot_recommended);
    assert!(matches!(
        err.remedy_hint,
        RemedyHint::AdjustVisualThreshold(_)
    ));
}

// =========================================================================
// 5. Emergency Kill-Switch Instant Abort (< 15ms SLA)
// =========================================================================

#[tokio::test]
async fn test_kill_switch_instant_abort_in_macro() {
    let (router, _, _, engine) = create_test_router(PermissionMode::Bounded, true);

    // Trigger Emergency Halt before plan execution
    engine.trigger_emergency_halt().expect("Halt must trigger");

    let plan = MacroPlan {
        plan_id: "plan_abort".into(),
        session_id: "sess_abort".into(),
        goal_description: "Aborted plan".into(),
        steps: vec![System1Action::Click {
            target: TargetDescriptor::new(
                WindowSelector::Hwnd(0x1234),
                TargetCoordinate::ClientPixel { x: 50, y: 50 },
            ),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
        }],
        ..Default::default()
    };

    let start = Instant::now();
    let res = router.execute_macro_plan(plan).await;
    let elapsed_ms = start.elapsed().as_millis();

    assert!(!res.success, "Plan must fail on active kill-switch");
    assert_eq!(res.completed_steps, 0);
    assert!(
        elapsed_ms < 15,
        "Halt check must resolve in < 15ms (observed: {}ms)",
        elapsed_ms
    );

    let defect = res.defect_report.expect("Must have defect report");
    assert_eq!(defect.failure_category, DefectCategory::KillSwitchAborted);
    assert_eq!(defect.error_code, "emergency_halt");
    assert!(matches!(
        defect.remedy_hint,
        RemedyHint::UserInterventionNeeded(_)
    ));
}

// =========================================================================
// 6. Structured System-2 Defect Reporting with JSON Schema Validation
// =========================================================================

#[tokio::test]
async fn test_system2_defect_report_serialization_and_fields() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Coordinate strictly outside client bounds (width = 800, x = 1200)
    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 1200, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess_defect", Some("plan_007"), 2, 5, 2, action, 2, true)
        .await
        .expect_err("Must produce defect report");

    assert_eq!(err.failed_step_index, 2);
    assert_eq!(err.total_steps, 5);
    assert_eq!(err.completed_steps, 2);
    assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
    assert!(err.screenshot_recommended);

    // Validate JSON serialization
    let json_val = serde_json::to_value(&err).expect("Defect report must serialize to Value");
    assert_eq!(json_val["failure_category"], "out_of_bounds");
    assert_eq!(json_val["error_code"], "coordinate_out_of_bounds");
    assert!(json_val["remedy_hint"]["remedy_type"].is_string());
}

// =========================================================================
// 7. Multi-Step Macro Plan Execution with 0 Intermediate Tokens
// =========================================================================

#[tokio::test]
async fn test_zero_token_multi_step_macro_plan_execution() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Enqueue (baseline, post-action) frame pairs so each step's visual diff passes
    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);

    // Step 0: Click
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    // Step 1: TypeText
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    // Step 2: Click
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    let plan = MacroPlan {
        plan_id: "plan_zero_token".into(),
        session_id: "sess_macro".into(),
        goal_description: "Multi-step form completion".into(),
        steps: vec![
            System1Action::Click {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 50, y: 50 },
                ),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            System1Action::TypeText {
                target: None,
                text: "Agent Action".into(),
                clear_before: false,
                press_enter: true,
                delivery_mode: None,
            },
            System1Action::Click {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 100, y: 100 },
                ),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
        ],
        max_step_retries: 2,
        abort_on_error: true,
        allow_foreground_escalation: true,
    };

    let result = router.execute_macro_plan(plan).await;

    assert!(result.success, "Macro plan must succeed");
    assert_eq!(result.completed_steps, 3);
    assert_eq!(result.total_steps, 3);
    assert_eq!(
        result.tokens_consumed, 0,
        "System-1 execution consumes exactly 0 intermediate tokens"
    );
    assert!(result.defect_report.is_none());
    assert_eq!(driver.get_dispatched_actions().len(), 3);
}

// =========================================================================
// 8. Milestone M3 Remediation Tests (Findings 1 - 7)
// =========================================================================

#[tokio::test]
async fn test_drag_action_resolves_and_dispatches_distinct_end_coordinates() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    // Window bounds are (100, 100, 800, 600) -> client rect is (108, 130, 784, 562)
    // Start at client (50, 50) -> screen (158, 180)
    // End at client (200, 250) -> screen (308, 380)
    let action = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 200, y: 250 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let result = router
        .execute_step_with_escalation("sess_drag", None, 0, 1, 0, action, 1, true)
        .await;

    assert!(result.is_ok(), "Drag step must succeed: {:?}", result.err());
    let dispatched = driver.get_dispatched_actions();
    assert_eq!(
        dispatched.len(),
        1,
        "Exactly one drag action should be dispatched"
    );

    match &dispatched[0] {
        CuaAction::Drag {
            target_hwnd,
            start_x,
            start_y,
            end_x,
            end_y,
            ..
        } => {
            assert_eq!(*target_hwnd, 0x1234);
            assert_eq!(*start_x, 158);
            assert_eq!(*start_y, 180);
            assert_eq!(*end_x, 308);
            assert_eq!(*end_y, 380);
            // Verify end coordinate is genuinely distinct from the old hardcoded start_x + 50
            assert_ne!(*end_x, start_x + 50);
        }
        other => panic!("Expected CuaAction::Drag, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_drag_action_fails_when_end_target_out_of_bounds() {
    let (router, _driver, _capturer, _) = create_test_router(PermissionMode::Bounded, true);

    let action = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 2000, y: 2000 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err = router
        .execute_step_with_escalation("sess_drag_oob", None, 0, 1, 0, action, 1, true)
        .await
        .expect_err("Drag with out-of-bounds end target must be rejected by precheck");

    assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
}

#[tokio::test]
async fn test_wait_visual_change_polling_loop_direct_execution() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let changed = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(changed);

    let action = System1Action::WaitVisualChange {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        )
        .with_roi(CuaRect::new(150, 150, 100, 100)),
        timeout_ms: 1000,
        min_changed_ratio: Some(0.01),
    };

    let result = router
        .execute_step_with_escalation("sess_wait", None, 0, 1, 0, action, 1, true)
        .await;

    assert!(
        result.is_ok(),
        "WaitVisualChange should succeed without error: {:?}",
        result.err()
    );
    assert!(
        driver.get_dispatched_actions().is_empty(),
        "WaitVisualChange must not dispatch low-level OS inputs or BringToFront"
    );
}

#[tokio::test]
async fn test_type_text_clear_before_and_press_enter_dispatches() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    let action = System1Action::TypeText {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        )),
        text: "Automated Input".into(),
        clear_before: true,
        press_enter: true,
        delivery_mode: None,
    };

    let result = router
        .execute_step_with_escalation("sess_type", None, 0, 1, 0, action, 1, true)
        .await;

    assert!(
        result.is_ok(),
        "TypeText with clear_before and press_enter must succeed"
    );
    let dispatched = driver.get_dispatched_actions();
    // Expected sequence:
    // 1. Hotkey Ctrl+A
    // 2. PressKey Backspace (down)
    // 3. PressKey Backspace (up)
    // 4. TypeText "Automated Input\n"
    assert_eq!(
        dispatched.len(),
        4,
        "Expected Hotkey(Ctrl+A), PressKey down/up(Backspace), TypeText with newline"
    );

    match &dispatched[0] {
        CuaAction::Hotkey { keys, .. } => {
            assert!(keys.contains(&"ctrl".to_string()) || keys.contains(&"Control".to_string()));
            assert!(keys.contains(&"a".to_string()));
        }
        other => panic!("Expected Hotkey for Ctrl+A, got {:?}", other),
    }

    match &dispatched[1] {
        CuaAction::PressKey { key, down, .. } => {
            assert_eq!(key, "backspace");
            assert!(*down);
        }
        other => panic!("Expected PressKey down for Backspace, got {:?}", other),
    }

    match &dispatched[2] {
        CuaAction::PressKey { key, down, .. } => {
            assert_eq!(key, "backspace");
            assert!(!*down);
        }
        other => panic!("Expected PressKey up for Backspace, got {:?}", other),
    }

    match &dispatched[3] {
        CuaAction::TypeText { text, .. } => {
            assert_eq!(
                text, "Automated Input\n",
                "press_enter must append newline to typed text"
            );
        }
        other => panic!("Expected TypeText, got {:?}", other),
    }
}

#[tokio::test]
async fn test_hover_dwell_duration_and_emergency_halt() {
    let (router, _driver, capturer, engine) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    // 1. Normal dwell of 60ms
    let start_time = Instant::now();
    let action_normal = System1Action::Hover {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        duration_ms: Some(60),
    };
    let res = router
        .execute_step_with_escalation("sess_hover", None, 0, 1, 0, action_normal, 1, true)
        .await;
    assert!(res.is_ok());
    let elapsed = start_time.elapsed();
    assert!(
        elapsed >= Duration::from_millis(50),
        "Hover dwell must observe requested duration (elapsed: {:?})",
        elapsed
    );

    // 2. Emergency halt aborts dwell rapidly (< 250ms)
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);
    let halt_engine = engine.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(15)).await;
        let _ = halt_engine.trigger_emergency_halt();
    });

    let action_halt = System1Action::Hover {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        duration_ms: Some(2000),
    };
    let halt_start = Instant::now();
    let halt_res = router
        .execute_step_with_escalation("sess_hover_halt", None, 0, 1, 0, action_halt, 1, true)
        .await;
    let halt_elapsed = halt_start.elapsed();
    assert!(
        halt_res.is_err(),
        "Hover must return error upon emergency halt"
    );
    assert!(
        halt_elapsed < Duration::from_millis(250),
        "Emergency halt must abort hover dwell rapidly (elapsed: {:?})",
        halt_elapsed
    );
    assert_eq!(
        halt_res.unwrap_err().failure_category,
        DefectCategory::KillSwitchAborted
    );
}

#[tokio::test]
async fn test_diff_padded_roi_truncated_buffer_defensive_validation() {
    let roi = CuaRect::new(0, 0, 100, 100);

    let truncated_frame = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![0u8; 100], // Expected 100 * 100 * 4 = 40,000 bytes
    };
    let normal_frame = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![0u8; 40000],
    };

    let res1 = RoiDiffEngine::diff_padded_roi(&truncated_frame, &normal_frame, &roi, 10);
    assert!(res1.is_err(), "Truncated pre-frame must return error");
    match res1.err().unwrap() {
        liva_cua::types::CuaError::InvalidParameter(msg) => {
            assert!(
                msg.contains("truncated"),
                "Error should explain buffer truncation: {}",
                msg
            );
        }
        other => panic!("Expected CuaError::InvalidParameter, got: {:?}", other),
    }

    let res2 = RoiDiffEngine::diff_padded_roi(&normal_frame, &truncated_frame, &roi, 10);
    assert!(res2.is_err(), "Truncated post-frame must return error");
    match res2.err().unwrap() {
        liva_cua::types::CuaError::InvalidParameter(msg) => {
            assert!(
                msg.contains("truncated"),
                "Error should explain buffer truncation: {}",
                msg
            );
        }
        other => panic!("Expected CuaError::InvalidParameter, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_apply_roi_padding_adversarial_containment() {
    let max_w: i32 = 1920;
    let max_h: i32 = 1080;

    let r1 = CuaRect::new(-1000, -1000, 50, 50);
    let p1 = apply_roi_padding(&r1, 16, max_w as u32, max_h as u32);
    assert!(p1.x >= 0 && p1.y >= 0);
    assert!(p1.x + p1.width <= max_w);
    assert!(p1.y + p1.height <= max_h);
    assert!(p1.width >= 1 && p1.height >= 1);

    let r2 = CuaRect::new(5000, 5000, 100, 100);
    let p2 = apply_roi_padding(&r2, 16, max_w as u32, max_h as u32);
    assert!(p2.x >= 0 && p2.y >= 0);
    assert!(p2.x + p2.width <= max_w);
    assert!(p2.y + p2.height <= max_h);
    assert!(p2.width >= 1 && p2.height >= 1);

    let r3 = CuaRect::new(-500, -500, 100_000, 100_000);
    let p3 = apply_roi_padding(&r3, 16, max_w as u32, max_h as u32);
    assert_eq!(p3.x, 0);
    assert_eq!(p3.y, 0);
    assert_eq!(p3.width, max_w);
    assert_eq!(p3.height, max_h);
}

#[tokio::test]
async fn test_normalized_coordinate_boundary_clamping() {
    let window_bounds = CuaRect::new(100, 100, 800, 600);
    let client_bounds = CuaRect::new(108, 130, 784, 562);

    let coord = TargetCoordinate::ClientNormalized { x: 1.0, y: 1.0 };
    let (s_pt, c_pt) =
        TargetResolver::translate_coordinates(&coord, &window_bounds, &client_bounds, None)
            .expect("Translation must succeed");
    assert_eq!(c_pt.x, 783); // 784 - 1
    assert_eq!(c_pt.y, 561); // 562 - 1
    assert_eq!(s_pt.x, 108 + 783);
    assert_eq!(s_pt.y, 130 + 561);
}
