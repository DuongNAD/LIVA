//! Adversarial Challenge Test Suite — Challenger 2 (Milestone M3: System-1 GUI Loop).
//!
//! Evaluates:
//! 1. Visual diff engine under noise/ClearType sub-pixel jitter (diff <= color_tolerance)
//!    and genuine pixel shifts above threshold.
//! 2. Out-of-bounds ROI cropping and padding without panics or memory corruption.
//! 3. Delivery fallback escalation protocol (Background -> Foreground on 0% diff)
//!    and exhaustion on persistent 0% diff with RemedyHint::AdjustVisualThreshold.
//! 4. Emergency Kill-Switch mid-plan abort latency (< 15ms SLA) and downstream step cancellation.
//! 5. Structured System-2 defect report integrity and JSON serialization roundtrips.

use liva_cua::mock::MockCuaDriver;
use liva_cua::router::{
    DefectCategory, MacroPlan, RemedyHint, System1Action, System1ActionRouter, System1DefectReport,
    System1RouterConfig, TargetCoordinate, TargetDescriptor, WindowSelector,
};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaError, CuaRect, CuaWindowInfo, MouseButton,
    PermissionMode,
};
use liva_cua::vision::diff::{
    apply_roi_padding, crop_raw_frame, DefaultVisualVerifier, RoiDiffEngine,
};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::{
    CuaPixelFormat, RawFrame, RoiDiffConfig, SemanticVerificationCriteria, VisualDiffResult,
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
// 1. Visual Diffing Engine Under Noise, ClearType Jitter & Edge Cases
// =========================================================================

#[test]
fn test_adversarial_cleartype_subpixel_noise_no_false_positive() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    let mut f2 = capturer.create_uniform_frame(128);

    let tol = 5u8;
    let roi = CuaRect::new(50, 50, 100, 100); // 10,000 pixels

    // Inject simulated ClearType sub-pixel noise across the entire 100x100 ROI
    // where per-channel differences are strictly <= tol (perturbations: -5..=+5)
    let bpp = f2.format.bytes_per_pixel();
    let stride = f2.width as usize * bpp;
    for r in 0..100 {
        for c in 0..100 {
            let offset = (50 + r) * stride + (50 + c) * bpp;
            // Perturb R, G, B channels within tolerance <= 5
            f2.data[offset] = (128i16 + ((c % 5) as i16 + 1)).clamp(0, 255) as u8; // +1 to +5
            f2.data[offset + 1] = (128i16 - (((r + c) % 5) as i16 + 1)).clamp(0, 255) as u8; // -1 to -5
            f2.data[offset + 2] =
                (128i16 + if (r + c) % 2 == 0 { 5 } else { -5 }).clamp(0, 255) as u8; // +/- 5
            f2.data[offset + 3] = 128; // Keep alpha channel identical (128 in f1)
        }
    }

    // Verify Click criteria: min 0.5% or 8px
    let criteria_click = SemanticVerificationCriteria::click_default();
    let res_click = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, tol, &criteria_click)
        .expect("diff_roi must succeed");
    assert!(
        !res_click.diff_detected,
        "ClearType noise <= tol must NOT trigger diff for Click (changed_pixels: {})",
        res_click.changed_pixels
    );
    assert_eq!(res_click.changed_pixels, 0);
    assert_eq!(res_click.changed_ratio, 0.0);

    // Verify TypeText criteria: min 6px
    let criteria_type = SemanticVerificationCriteria::type_text_default();
    let res_type = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, tol, &criteria_type)
        .expect("diff_roi must succeed");
    assert!(
        !res_type.diff_detected,
        "ClearType noise <= tol must NOT trigger diff for TypeText"
    );
    assert_eq!(res_type.changed_pixels, 0);

    // Verify Scroll criteria: min 3.0% or 50px
    let criteria_scroll = SemanticVerificationCriteria::scroll_default();
    let res_scroll = RoiDiffEngine::diff_roi(&f1, &f2, &roi, &roi, tol, &criteria_scroll)
        .expect("diff_roi must succeed");
    assert!(
        !res_scroll.diff_detected,
        "ClearType noise <= tol must NOT trigger diff for Scroll"
    );
    assert_eq!(res_scroll.changed_pixels, 0);
}

#[test]
fn test_adversarial_pixel_shifts_exceeding_tolerance_thresholds() {
    let capturer = MockScreenCapturer::new(800, 600, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    let tol = 5u8;
    let roi = CuaRect::new(50, 50, 100, 100); // 10,000 pixels

    // Test 1: Exactly 7 pixels shifted by diff = 6 (tol + 1) -> Below Click threshold (needs 8)
    let mut f_sub_click = capturer.create_uniform_frame(128);
    let bpp = f_sub_click.format.bytes_per_pixel();
    let stride = f_sub_click.width as usize * bpp;
    for i in 0..7 {
        let offset = 50 * stride + (50 + i) * bpp;
        f_sub_click.data[offset] = 128 + tol + 1; // 134, delta = 6 > 5
    }
    let res_sub_click = RoiDiffEngine::diff_roi(
        &f1,
        &f_sub_click,
        &roi,
        &roi,
        tol,
        &SemanticVerificationCriteria::click_default(),
    )
    .expect("diff_roi must succeed");
    assert_eq!(res_sub_click.changed_pixels, 7);
    assert!(
        !res_sub_click.diff_detected,
        "7 changed pixels must NOT satisfy Click criteria (requires >= 8)"
    );

    // Test 2: Exactly 8 pixels shifted by diff = 6 (tol + 1) -> Meets Click threshold (>= 8)
    let mut f_click = capturer.create_uniform_frame(128);
    for i in 0..8 {
        let offset = 50 * stride + (50 + i) * bpp;
        f_click.data[offset] = 128 + tol + 1;
    }
    let res_click = RoiDiffEngine::diff_roi(
        &f1,
        &f_click,
        &roi,
        &roi,
        tol,
        &SemanticVerificationCriteria::click_default(),
    )
    .expect("diff_roi must succeed");
    assert_eq!(res_click.changed_pixels, 8);
    assert!(
        res_click.diff_detected,
        "8 changed pixels with delta > tol MUST satisfy Click criteria"
    );

    // Test 3: TypeText threshold: 5 pixels (fails) vs 6 pixels (passes)
    let mut f_type_fail = capturer.create_uniform_frame(128);
    for i in 0..5 {
        let offset = 60 * stride + (50 + i) * bpp;
        f_type_fail.data[offset] = 128 + 20; // delta = 20
    }
    let res_type_fail = RoiDiffEngine::diff_roi(
        &f1,
        &f_type_fail,
        &roi,
        &roi,
        tol,
        &SemanticVerificationCriteria::type_text_default(),
    )
    .expect("diff_roi must succeed");
    assert_eq!(res_type_fail.changed_pixels, 5);
    assert!(
        !res_type_fail.diff_detected,
        "5 pixels must not satisfy TypeText"
    );

    let mut f_type_pass = capturer.create_uniform_frame(128);
    for i in 0..6 {
        let offset = 60 * stride + (50 + i) * bpp;
        f_type_pass.data[offset] = 128 + 20;
    }
    let res_type_pass = RoiDiffEngine::diff_roi(
        &f1,
        &f_type_pass,
        &roi,
        &roi,
        tol,
        &SemanticVerificationCriteria::type_text_default(),
    )
    .expect("diff_roi must succeed");
    assert_eq!(res_type_pass.changed_pixels, 6);
    assert!(
        res_type_pass.diff_detected,
        "6 pixels MUST satisfy TypeText"
    );

    // Test 4: Channel diff exactly equal to tolerance (diff == tol) must NOT count as changed
    let mut f_exact_tol = capturer.create_uniform_frame(128);
    for i in 0..50 {
        let offset = 70 * stride + (50 + i) * bpp;
        f_exact_tol.data[offset] = 128 + tol; // 133, delta = 5 == tol
    }
    let res_exact_tol = RoiDiffEngine::diff_roi(
        &f1,
        &f_exact_tol,
        &roi,
        &roi,
        tol,
        &SemanticVerificationCriteria::click_default(),
    )
    .expect("diff_roi must succeed");
    assert_eq!(
        res_exact_tol.changed_pixels, 0,
        "Pixel difference equal to tolerance must NOT be counted as changed"
    );
    assert!(!res_exact_tol.diff_detected);
}

#[test]
fn test_adversarial_out_of_bounds_roi_cropping_and_diffing_no_panic() {
    let capturer = MockScreenCapturer::new(1920, 1080, CuaPixelFormat::Rgba, 128);
    let f1 = capturer.create_uniform_frame(128);
    let f2 = capturer.create_uniform_frame(200);

    let screen_w = 1920;
    let screen_h = 1080;

    // Test extreme bounding boxes:
    let test_rects = [
        CuaRect::new(-500, -500, 100, 100),   // Negative coordinates
        CuaRect::new(2500, 1500, 200, 200),   // Beyond screen dimensions
        CuaRect::new(1900, 1060, 500, 500),   // Partially straddling bottom-right
        CuaRect::new(-50, 100, 200, 200),     // Partially straddling left
        CuaRect::new(100, -50, 200, 200),     // Partially straddling top
        CuaRect::new(0, 0, 100_000, 100_000), // Massive dimensions
        CuaRect::new(100, 100, 1, 1),         // 1x1 minimum
    ];

    for (idx, raw_rect) in test_rects.iter().enumerate() {
        // 1. apply_roi_padding must clamp without panic
        let padded = apply_roi_padding(raw_rect, 16, screen_w, screen_h);
        assert!(
            padded.width >= 1 && padded.height >= 1,
            "Padded ROI [#{}] must have dimensions >= 1",
            idx
        );

        // 2. diff_padded_roi must execute cleanly without out-of-bounds index panics
        let diff_res = RoiDiffEngine::diff_padded_roi(&f1, &f2, &padded, 5);
        assert!(
            diff_res.is_ok(),
            "diff_padded_roi [#{}] must not panic or error: {:?}",
            idx,
            diff_res
        );

        // 3. diff_roi must execute cleanly
        let full_res = RoiDiffEngine::diff_roi(
            &f1,
            &f2,
            &padded,
            raw_rect,
            5,
            &SemanticVerificationCriteria::click_default(),
        );
        assert!(
            full_res.is_ok(),
            "diff_roi [#{}] must succeed: {:?}",
            idx,
            full_res
        );
    }

    // 4. crop_raw_frame out-of-bounds handling
    // Negative coordinates return clean CoordinateOutOfBounds error without panicking
    let neg_rect = CuaRect::new(-10, -10, 50, 50);
    let neg_crop = crop_raw_frame(&f1, &neg_rect);
    assert!(
        matches!(
            neg_crop,
            Err(liva_cua::types::CuaError::CoordinateOutOfBounds { .. })
        ),
        "crop_raw_frame with negative coordinates must return CoordinateOutOfBounds"
    );

    // Straddling right-bottom edge clamps cleanly and produces valid cropped frame
    let straddle_rect = CuaRect::new(1900, 1060, 100, 100);
    let straddle_crop = crop_raw_frame(&f1, &straddle_rect).expect("Straddling crop must succeed");
    assert_eq!(straddle_crop.width, 20); // 1920 - 1900
    assert_eq!(straddle_crop.height, 20); // 1080 - 1060
    assert_eq!(straddle_crop.data.len(), 20 * 20 * 4);
}

#[test]
fn test_adversarial_corrupted_truncated_frame_buffer() {
    // Malicious or corrupted frame where data is truncated
    let f1 = liva_cua::vision::types::RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![0; 50], // Truncated: expected 40,000 bytes
    };
    let f2 = f1.clone();
    let roi = CuaRect::new(0, 0, 50, 50);

    let res = std::panic::catch_unwind(|| {
        let _ = RoiDiffEngine::diff_padded_roi(&f1, &f2, &roi, 5);
    });

    if res.is_err() {
        println!("FINDING: diff_padded_roi panics on truncated RawFrame buffer without length validation");
    }
}

// =========================================================================
// 2. Delivery Fallback Escalation Protocol Under 0% Diff
// =========================================================================

#[tokio::test]
async fn test_adversarial_delivery_fallback_escalation_background_to_foreground() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Setup: Canvas drops background clicks (Attempt 0 = 0% diff).
    // On foreground escalation (Attempt 1 = retry 1), the button visually updates (diff > 0%).
    let base = capturer.create_uniform_frame(128);
    let changed = capturer.create_frame_with_rect(128, 240, 140, 140, 40, 40);

    capturer.enqueue_frame(base.clone()); // Pre-background baseline
    capturer.enqueue_frame(base.clone()); // Post-background capture (0% diff -> triggers escalation)
    capturer.enqueue_frame(base); // Pre-foreground baseline
    capturer.enqueue_frame(changed); // Post-foreground capture (diff detected!)

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 45, y: 45 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let step_res = router
        .execute_step_with_escalation("sess_adv", None, 0, 1, 0, action, 2, true)
        .await
        .expect("Action must succeed after escalating to Foreground");

    assert!(step_res.success);
    assert_eq!(
        step_res.delivery_used,
        CuaDeliveryMode::Foreground,
        "Must use Foreground delivery after escalation"
    );
    assert!(
        step_res.escalated_from_background,
        "Must flag escalated_from_background == true"
    );
    assert_eq!(
        step_res.retries_taken, 1,
        "Must consume exactly 1 retry for foreground escalation"
    );

    // Verify driver received: Click(Background) -> BringToFront -> Click(Foreground)
    let dispatched = driver.get_dispatched_actions();
    assert!(
        dispatched.len() >= 3,
        "Must dispatch Background, BringToFront, and Foreground"
    );

    let has_bg_click = dispatched.iter().any(|a| {
        matches!(
            a,
            liva_cua::types::CuaAction::Click {
                delivery_mode: CuaDeliveryMode::Background,
                ..
            }
        )
    });
    let has_bring_to_front = dispatched
        .iter()
        .any(|a| matches!(a, liva_cua::types::CuaAction::BringToFront { .. }));
    let has_fg_click = dispatched.iter().any(|a| {
        matches!(
            a,
            liva_cua::types::CuaAction::Click {
                delivery_mode: CuaDeliveryMode::Foreground,
                ..
            }
        )
    });

    assert!(has_bg_click, "Must attempt Background click first");
    assert!(
        has_bring_to_front,
        "Must bring window to front before foreground click"
    );
    assert!(has_fg_click, "Must execute Foreground click");
}

#[tokio::test]
async fn test_adversarial_persistent_zero_diff_halts_after_max_retries_with_remedy() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Setup: Element is completely inert or disabled.
    // Background yields 0% diff -> Escalates to Foreground -> Foreground yields 0% diff -> Retried -> 0% diff.
    let base = capturer.create_uniform_frame(128);
    // Enqueue 6 identical frames to satisfy baseline and post-checks across all attempts
    for _ in 0..8 {
        capturer.enqueue_frame(base.clone());
    }

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let defect = router
        .execute_step_with_escalation("sess_zero", Some("plan_zero"), 0, 1, 0, action, 2, true)
        .await
        .expect_err("Must halt and return defect report when 0% diff persists");

    assert_eq!(
        defect.failure_category,
        DefectCategory::VisualVerificationZeroDiff,
        "Failure category must be VisualVerificationZeroDiff"
    );
    assert_eq!(defect.error_code, "zero_visual_diff");
    assert_eq!(defect.retry_count, 2, "Must exhaust 2 retries");
    assert_eq!(
        defect.delivery_modes_attempted,
        vec![
            CuaDeliveryMode::Background,
            CuaDeliveryMode::Foreground,
            CuaDeliveryMode::Foreground,
        ],
        "Delivery modes must trace Background -> Foreground -> Foreground"
    );
    assert!(
        matches!(defect.remedy_hint, RemedyHint::AdjustVisualThreshold(_)),
        "Remedy hint must be AdjustVisualThreshold: {:?}",
        defect.remedy_hint
    );
    assert!(
        defect.screenshot_recommended,
        "Screenshot must be recommended"
    );
    assert!(
        defect.post_diff_result.is_some(),
        "Must include post_diff_result"
    );

    let diff_info = defect.post_diff_result.unwrap();
    assert!(!diff_info.diff_detected);
    assert_eq!(diff_info.changed_pixels, 0);

    // Verify driver dispatches stopped after max retries
    assert!(driver.get_dispatched_actions().len() >= 3);
}

#[tokio::test]
async fn test_adversarial_foreground_escalation_blocked_in_standard_mode() {
    let (router, _, _capturer, _) = create_test_router(PermissionMode::Standard, true);

    // In Standard mode, all mutating actions are refused during pre-checks.
    // Even if pre-checks somehow passed, foreground escalation is strictly forbidden.
    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let defect = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, action, 2, true)
        .await
        .expect_err("Standard mode must reject mutating click during pre-check");

    assert_eq!(defect.failure_category, DefectCategory::SecurityRefused);
    assert!(matches!(
        defect.remedy_hint,
        RemedyHint::SecurityPolicyViolation(_)
    ));
}

// =========================================================================
// 3. Emergency Kill-Switch Mid-Plan Abort Latency (< 15ms SLA)
// =========================================================================

#[tokio::test]
async fn test_adversarial_kill_switch_mid_plan_abort_latency_and_cancellation() {
    let (router, driver, capturer, engine) = create_test_router(PermissionMode::Bounded, true);

    // Enqueue frame pairs so Steps 0 and 1 succeed normally
    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 220, 150, 150, 40, 40);

    // Step 0 frames (Click)
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    // Step 1 frames (TypeText)
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    // Construct a 6-step MacroPlan
    let plan = MacroPlan {
        plan_id: "plan_kill_stress".into(),
        session_id: "sess_kill".into(),
        goal_description: "Adversarial kill-switch test plan".into(),
        steps: vec![
            // Step 0
            System1Action::Click {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 40, y: 40 },
                ),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            // Step 1
            System1Action::TypeText {
                target: None,
                text: "Hello World".into(),
                clear_before: false,
                press_enter: false,
                delivery_mode: None,
            },
            // Step 2: Kill switch will be active at/before this step
            System1Action::Click {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 60, y: 60 },
                ),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            // Step 3: Should NEVER execute
            System1Action::Scroll {
                target: None,
                delta_x: 0,
                delta_y: -120,
            },
            // Step 4: Should NEVER execute
            System1Action::Click {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 80, y: 80 },
                ),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            // Step 5: Should NEVER execute
            System1Action::Hover {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 100, y: 100 },
                ),
                duration_ms: None,
            },
        ],
        max_step_retries: 2,
        abort_on_error: true,
        allow_foreground_escalation: true,
    };

    // Spawn concurrent trigger: wait 15ms (after Step 0 & 1 execute) and trigger kill-switch
    let engine_clone = engine.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(15)).await;
        let _ = engine_clone.trigger_emergency_halt();
    });

    let macro_start = Instant::now();
    let result = router.execute_macro_plan(plan).await;
    let _macro_duration = macro_start.elapsed();

    // Verify plan aborted
    assert!(
        !result.success,
        "Macro plan must not succeed when kill-switch triggers"
    );
    assert!(
        result.defect_report.is_some(),
        "Must produce a defect report"
    );

    let defect = result.defect_report.unwrap();
    assert_eq!(
        defect.failure_category,
        DefectCategory::KillSwitchAborted,
        "Defect category must be KillSwitchAborted"
    );
    assert_eq!(defect.error_code, "emergency_halt");
    assert!(
        matches!(defect.remedy_hint, RemedyHint::UserInterventionNeeded(_)),
        "Remedy hint must indicate UserInterventionNeeded"
    );

    // Verify cancellation of all downstream steps
    assert!(
        result.completed_steps < 6,
        "Completed steps must be less than total (observed: {})",
        result.completed_steps
    );
    assert_eq!(result.step_results.len(), result.completed_steps);

    // Verify driver dispatches match completed steps (no dangling downstream steps executed)
    let dispatched_count = driver.get_dispatched_actions().len();
    assert!(
        dispatched_count <= result.completed_steps + 1,
        "Downstream steps must not be dispatched"
    );

    // Verify abort latency: once kill-switch is active, loop termination takes < 15ms
    // Now trigger an immediate second macro to measure pure halt detection latency
    let plan2 = MacroPlan {
        plan_id: "plan_pure_latency".into(),
        session_id: "sess_pure".into(),
        goal_description: "Pure halt latency check".into(),
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
            System1Action::Hover {
                target: TargetDescriptor::new(
                    WindowSelector::Hwnd(0x1234),
                    TargetCoordinate::ClientPixel { x: 60, y: 60 },
                ),
                duration_ms: None,
            },
        ],
        ..Default::default()
    };

    let start_instant = Instant::now();
    let res2 = router.execute_macro_plan(plan2).await;
    let pure_abort_latency_us = start_instant.elapsed().as_micros();
    let pure_abort_latency_ms = pure_abort_latency_us as f64 / 1000.0;

    println!(
        "EMPIRICAL METRIC: Macro Kill-Switch abort latency = {:.3}ms ({:.1}us) [SLA < 15.0ms]",
        pure_abort_latency_ms, pure_abort_latency_us as f64
    );

    assert!(!res2.success);
    assert_eq!(res2.completed_steps, 0);
    assert!(
        pure_abort_latency_ms < 15.0,
        "Kill-Switch abort check must take < 15ms SLA (observed: {:.3}ms)",
        pure_abort_latency_ms
    );
}

// =========================================================================
// 4. Structured Defect Report Integrity & JSON Serialization
// =========================================================================

#[test]
fn test_adversarial_defect_report_json_roundtrip_all_categories_and_remedies() {
    let dummy_action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientNormalized { x: 0.5, y: 0.5 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: Some(CuaDeliveryMode::Background),
    };

    let categories = vec![
        DefectCategory::PreCheckFailed,
        DefectCategory::OutOfBounds,
        DefectCategory::TargetOccluded,
        DefectCategory::SecurityRefused,
        DefectCategory::KillSwitchAborted,
        DefectCategory::VisualVerificationZeroDiff,
        DefectCategory::EscalationBlockedByPolicy,
        DefectCategory::DriverError,
        DefectCategory::Timeout,
    ];

    let remedies = vec![
        RemedyHint::ReacquireTargetCoordinates("Reacquire".into()),
        RemedyHint::RestoreMinimizedWindow("Restore".into()),
        RemedyHint::BringWindowToForeground("Foreground".into()),
        RemedyHint::AdjustVisualThreshold("Threshold".into()),
        RemedyHint::ElevationRequired("Elevation".into()),
        RemedyHint::SecurityPolicyViolation("Security".into()),
        RemedyHint::UserInterventionNeeded("User".into()),
        RemedyHint::WindowDied("Died".into()),
    ];

    let dummy_window = CuaWindowInfo {
        hwnd: 0x9999,
        pid: 1234,
        title: "Test App".into(),
        class_name: "AppClass".into(),
        process_name: Some("test.exe".into()),
        bounds: CuaRect::new(10, 10, 400, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };

    let dummy_diff = VisualDiffResult {
        diff_detected: false,
        changed_ratio: 0.0,
        changed_pixels: 0,
        total_roi_pixels: 5000,
        roi_rect: CuaRect::new(20, 20, 100, 50),
        raw_rect: CuaRect::new(20, 20, 100, 50),
        latency_ms: 42,
        scan_duration_us: 120,
    };

    for (cat, rem) in categories.into_iter().zip(remedies) {
        let report = System1DefectReport {
            defect_id: uuid::Uuid::new_v4().to_string(),
            session_id: "session_json_test".into(),
            plan_id: Some("plan_json_test".into()),
            failed_step_index: 3,
            total_steps: 10,
            completed_steps: 3,
            failed_action: dummy_action.clone(),
            failure_category: cat,
            pre_check_error: Some("Precheck failed".into()),
            post_diff_result: Some(dummy_diff.clone()),
            retry_count: 2,
            delivery_modes_attempted: vec![
                CuaDeliveryMode::Background,
                CuaDeliveryMode::Foreground,
            ],
            error_code: "test_error_code".into(),
            error_message: "Human readable diagnostic message".into(),
            remedy_hint: rem,
            last_known_window: Some(dummy_window.clone()),
            screenshot_recommended: true,
            timestamp_utc: "2026-09-24T14:20:00Z".into(),
        };

        // 1. Serialize to JSON string
        let json_str = serde_json::to_string_pretty(&report)
            .expect("System1DefectReport serialization must not fail");

        // 2. Deserialize back
        let deserialized: System1DefectReport = serde_json::from_str(&json_str)
            .expect("System1DefectReport deserialization must succeed");

        // 3. Strict equality check
        assert_eq!(
            report, deserialized,
            "Defect report must preserve 100% fidelity through JSON roundtrip"
        );
    }
}

#[tokio::test]
async fn test_adversarial_defect_report_diagnostics_completeness() {
    let (router, driver, _, _) = create_test_router(PermissionMode::Bounded, true);

    // Insert an occluding modal dialog
    let modal = CuaWindowInfo {
        hwnd: 0x9999,
        pid: 2000,
        title: "Modal Block".into(),
        class_name: "ModalClass".into(),
        process_name: Some("modal.exe".into()),
        bounds: CuaRect::new(100, 100, 300, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    // Demote target window z_index
    let target = driver.inspect_window(0x1234).await.unwrap();
    if let liva_cua::types::WindowStateVerdict::Valid {
        window_info: Some(mut info),
        ..
    } = target
    {
        info.z_index = 1;
        driver.insert_window(info);
    }
    driver.insert_window(modal);

    let action = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ScreenPixel { x: 200, y: 200 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let defect = router
        .execute_step_with_escalation(
            "sess_diag",
            Some("plan_diag"),
            1,
            5,
            1,
            action.clone(),
            2,
            true,
        )
        .await
        .expect_err("Must generate pre-check defect report for occlusion");

    // Diagnostic fields inspection
    assert_eq!(defect.failed_step_index, 1);
    assert_eq!(defect.total_steps, 5);
    assert_eq!(defect.completed_steps, 1);
    assert_eq!(defect.failed_action, action);
    assert_eq!(defect.failure_category, DefectCategory::TargetOccluded);
    assert!(defect.pre_check_error.is_some());
    assert!(
        defect.post_diff_result.is_none(),
        "Pre-check errors must not have post_diff_result"
    );
    assert_eq!(
        defect.retry_count, 0,
        "Pre-check errors must not waste retries"
    );
    assert!(defect.screenshot_recommended);
    assert!(matches!(
        defect.remedy_hint,
        RemedyHint::BringWindowToForeground(_)
    ));
}

// =========================================================================
// 6. Milestone M3 Iteration 2: Remediation Adversarial Challenge Tests
// =========================================================================

#[tokio::test]
async fn test_adversarial_m3_it2_wait_visual_change_multi_frame_resolution() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Initial state: verify target window z_index
    let win_before = driver.inspect_window(0x1234).await.unwrap();
    let initial_z = match win_before {
        liva_cua::types::WindowStateVerdict::Valid {
            window_info: Some(ref info),
            ..
        } => info.z_index,
        _ => panic!("Expected valid window"),
    };

    // Frame 0: Baseline uniform 128
    let base = capturer.create_uniform_frame(128);
    // Frames 1..3: Identical uniform 128 (no diff detected)
    let frame1 = capturer.create_uniform_frame(128);
    let frame2 = capturer.create_uniform_frame(128);
    let frame3 = capturer.create_uniform_frame(128);
    // Frame 4: Significant visual change in the ROI (150, 150, 50, 50)
    let frame4 = capturer.create_frame_with_rect(128, 240, 150, 150, 50, 50);

    capturer.enqueue_frame(base);
    capturer.enqueue_frame(frame1);
    capturer.enqueue_frame(frame2);
    capturer.enqueue_frame(frame3);
    capturer.enqueue_frame(frame4);

    let action = System1Action::WaitVisualChange {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        )
        .with_roi(CuaRect::new(150, 150, 100, 100)),
        timeout_ms: 2000,
        min_changed_ratio: Some(0.01),
    };

    let result = router
        .execute_step_with_escalation("sess_wait_multiframe", None, 0, 1, 0, action, 1, true)
        .await;

    // 1. Must succeed after observing the multi-frame visual transition
    assert!(
        result.is_ok(),
        "WaitVisualChange multi-frame must succeed: {:?}",
        result.err()
    );
    let step_res = result.unwrap();
    assert!(step_res.success);
    assert!(step_res.diff_result.is_some());
    assert!(step_res.diff_result.as_ref().unwrap().diff_detected);

    // 2. Must NOT dispatch any low-level OS input or BringToFront actuation
    assert!(
        driver.get_dispatched_actions().is_empty(),
        "WaitVisualChange must not dispatch OS actuation, got: {:?}",
        driver.get_dispatched_actions()
    );

    // 3. Must NOT mutate window z-order
    let win_after = driver.inspect_window(0x1234).await.unwrap();
    let final_z = match win_after {
        liva_cua::types::WindowStateVerdict::Valid {
            window_info: Some(ref info),
            ..
        } => info.z_index,
        _ => panic!("Expected valid window"),
    };
    assert_eq!(
        initial_z, final_z,
        "WaitVisualChange must not mutate window z-order"
    );
}

#[tokio::test]
async fn test_adversarial_m3_it2_wait_visual_change_clean_timeout_and_diagnostics() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Enqueue multiple identical frames: visual change never occurs
    for _ in 0..10 {
        capturer.enqueue_frame(capturer.create_uniform_frame(128));
    }

    let timeout_ms = 150u64;
    let action = System1Action::WaitVisualChange {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        )
        .with_roi(CuaRect::new(150, 150, 100, 100)),
        timeout_ms,
        min_changed_ratio: Some(0.05),
    };

    let start_t = Instant::now();
    let result = router
        .execute_step_with_escalation("sess_wait_timeout", None, 0, 1, 0, action.clone(), 1, true)
        .await;
    let elapsed = start_t.elapsed();

    // 1. Must fail cleanly with defect report, without hanging or panic
    assert!(
        result.is_err(),
        "WaitVisualChange must return error when visual change never occurs"
    );
    assert!(
        elapsed >= Duration::from_millis(100),
        "Timeout must wait approximately the requested duration, elapsed: {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(800),
        "Timeout must not hang indefinitely, elapsed: {:?}",
        elapsed
    );

    let defect = result.unwrap_err();
    assert_eq!(
        defect.failure_category,
        DefectCategory::VisualVerificationZeroDiff
    );
    assert_eq!(defect.error_code, "visual_wait_timeout");
    assert!(
        defect
            .error_message
            .contains("expired without detecting expected visual change"),
        "Diagnostic message must indicate timeout: {}",
        defect.error_message
    );
    assert!(defect.post_diff_result.is_some());
    assert!(!defect.post_diff_result.as_ref().unwrap().diff_detected);
    assert!(matches!(
        defect.remedy_hint,
        RemedyHint::AdjustVisualThreshold(_)
    ));

    // 2. Must NOT dispatch low-level OS actuation
    assert!(
        driver.get_dispatched_actions().is_empty(),
        "WaitVisualChange timeout must not dispatch OS inputs"
    );
}

#[tokio::test]
async fn test_adversarial_m3_it2_wait_visual_change_kill_switch_immediate_abort() {
    let (router, _driver, capturer, engine) = create_test_router(PermissionMode::Bounded, true);

    capturer.enqueue_frame(capturer.create_uniform_frame(128));

    // Trigger emergency kill switch prior to WaitVisualChange
    engine
        .trigger_emergency_halt()
        .expect("Kill switch trigger must succeed");

    let action = System1Action::WaitVisualChange {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        )
        .with_roi(CuaRect::new(150, 150, 100, 100)),
        timeout_ms: 3000,
        min_changed_ratio: Some(0.01),
    };

    let start_t = Instant::now();
    let result = router
        .execute_step_with_escalation("sess_wait_abort", None, 0, 1, 0, action, 1, true)
        .await;
    let elapsed = start_t.elapsed();

    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_millis(50),
        "Pre-halted kill-switch must abort WaitVisualChange immediately (< 50ms), elapsed: {:?}",
        elapsed
    );

    let defect = result.unwrap_err();
    assert_eq!(defect.failure_category, DefectCategory::KillSwitchAborted);
    assert_eq!(defect.error_code, "emergency_halt");
}

#[tokio::test]
async fn test_adversarial_m3_it2_hover_dwell_duration_accuracy() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    // Request hover with 1000ms dwell duration
    let dwell_ms = 1000u32;
    let action = System1Action::Hover {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        duration_ms: Some(dwell_ms),
    };

    let start_t = Instant::now();
    let result = router
        .execute_step_with_escalation("sess_hover_1000", None, 0, 1, 0, action, 1, true)
        .await;
    let elapsed = start_t.elapsed();

    assert!(
        result.is_ok(),
        "Hover with 1000ms dwell must succeed: {:?}",
        result.err()
    );
    assert!(
        elapsed >= Duration::from_millis(980),
        "Hover must dwell for requested duration (>= 980ms), elapsed: {:?}",
        elapsed
    );
    assert!(
        elapsed < Duration::from_millis(2000),
        "Hover dwell should not overrun significantly, elapsed: {:?}",
        elapsed
    );

    let dispatched = driver.get_dispatched_actions();
    assert_eq!(dispatched.len(), 1);
    match &dispatched[0] {
        CuaAction::MoveCursor { x, y, .. } => {
            // Client drawing area starts at (108, 130) due to 8px border and 30px caption
            assert_eq!(*x, 158); // 108 + 50
            assert_eq!(*y, 180); // 130 + 50
        }
        other => panic!("Expected MoveCursor action for Hover, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adversarial_m3_it2_hover_dwell_kill_switch_abort_latency_under_50ms_sla() {
    let (router, _driver, capturer, engine) = create_test_router(PermissionMode::Bounded, true);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    let halt_engine = engine.clone();
    let (trigger_tx, trigger_rx) = tokio::sync::oneshot::channel::<Instant>();

    tokio::spawn(async move {
        // Sleep 40ms to ensure the hover action is actively dwelling in its while loop
        tokio::time::sleep(Duration::from_millis(40)).await;
        let halt_time = Instant::now();
        let _ = halt_engine.trigger_emergency_halt();
        let _ = trigger_tx.send(halt_time);
    });

    let action = System1Action::Hover {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        duration_ms: Some(3000), // 3000ms dwell
    };

    let result = router
        .execute_step_with_escalation("sess_hover_abort_sla", None, 0, 1, 0, action, 1, true)
        .await;

    let halt_instant = trigger_rx
        .await
        .expect("Halt signal must have been recorded");
    let abort_latency = halt_instant.elapsed();

    assert!(result.is_err(), "Emergency halt must abort hover dwell");
    assert!(
        abort_latency < Duration::from_millis(50),
        "Emergency halt abort latency during hover dwell MUST be < 50ms SLA (measured: {:?})",
        abort_latency
    );

    let defect = result.unwrap_err();
    assert_eq!(defect.failure_category, DefectCategory::KillSwitchAborted);
    assert_eq!(defect.error_code, "emergency_halt");
    assert!(defect
        .error_message
        .contains("Kill-switch triggered during hover dwell"));
}

#[tokio::test]
async fn test_adversarial_m3_it2_type_text_clear_before_and_press_enter_hotkey_matrix() {
    let (router, driver, capturer, _) = create_test_router(PermissionMode::Bounded, true);

    // Matrix Case A: clear_before: true, press_enter: true
    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    let action_a = System1Action::TypeText {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        )),
        text: "Alpha".into(),
        clear_before: true,
        press_enter: true,
        delivery_mode: None,
    };

    let res_a = router
        .execute_step_with_escalation("sess_type_a", None, 0, 1, 0, action_a, 1, true)
        .await;
    assert!(res_a.is_ok());

    let dispatched_a = driver.get_dispatched_actions();
    assert_eq!(dispatched_a.len(), 4);
    // 1. Hotkey Ctrl+A
    match &dispatched_a[0] {
        CuaAction::Hotkey { keys, .. } => {
            assert!(keys.contains(&"ctrl".to_string()) || keys.contains(&"Control".to_string()));
            assert!(keys.contains(&"a".to_string()));
        }
        other => panic!("Expected Hotkey(Ctrl+A), got: {:?}", other),
    }
    // 2. PressKey Backspace down
    match &dispatched_a[1] {
        CuaAction::PressKey { key, down, .. } => {
            assert_eq!(key, "backspace");
            assert!(*down);
        }
        other => panic!("Expected PressKey down, got: {:?}", other),
    }
    // 3. PressKey Backspace up
    match &dispatched_a[2] {
        CuaAction::PressKey { key, down, .. } => {
            assert_eq!(key, "backspace");
            assert!(!*down);
        }
        other => panic!("Expected PressKey up, got: {:?}", other),
    }
    // 4. TypeText "Alpha\n"
    match &dispatched_a[3] {
        CuaAction::TypeText { text, .. } => {
            assert_eq!(
                text, "Alpha\n",
                "press_enter must append newline to typed text"
            );
        }
        other => panic!("Expected TypeText, got: {:?}", other),
    }

    // Matrix Case B: clear_before: false, press_enter: true with existing newline
    driver.dispatched_actions.lock().unwrap().clear();
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    let action_b = System1Action::TypeText {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        )),
        text: "Beta\n".into(),
        clear_before: false,
        press_enter: true,
        delivery_mode: None,
    };
    let res_b = router
        .execute_step_with_escalation("sess_type_b", None, 0, 1, 0, action_b, 1, true)
        .await;
    assert!(res_b.is_ok());

    let dispatched_b = driver.get_dispatched_actions();
    assert_eq!(
        dispatched_b.len(),
        1,
        "clear_before: false must not dispatch hotkey or backspace"
    );
    match &dispatched_b[0] {
        CuaAction::TypeText { text, .. } => {
            assert_eq!(
                text, "Beta\n",
                "Should not append double newline if text already ends with \\n"
            );
        }
        other => panic!("Expected TypeText, got: {:?}", other),
    }

    // Matrix Case C: clear_before: false, press_enter: true with carriage return
    driver.dispatched_actions.lock().unwrap().clear();
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    let action_c = System1Action::TypeText {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        )),
        text: "Gamma\r".into(),
        clear_before: false,
        press_enter: true,
        delivery_mode: None,
    };
    let res_c = router
        .execute_step_with_escalation("sess_type_c", None, 0, 1, 0, action_c, 1, true)
        .await;
    assert!(res_c.is_ok());

    let dispatched_c = driver.get_dispatched_actions();
    assert_eq!(dispatched_c.len(), 1);
    match &dispatched_c[0] {
        CuaAction::TypeText { text, .. } => {
            assert_eq!(
                text, "Gamma\r",
                "Should not append \\n if text already ends with \\r"
            );
        }
        other => panic!("Expected TypeText, got: {:?}", other),
    }

    // Matrix Case D: clear_before: true, press_enter: false
    driver.dispatched_actions.lock().unwrap().clear();
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    let action_d = System1Action::TypeText {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        )),
        text: "Delta".into(),
        clear_before: true,
        press_enter: false,
        delivery_mode: None,
    };
    let res_d = router
        .execute_step_with_escalation("sess_type_d", None, 0, 1, 0, action_d, 1, true)
        .await;
    assert!(res_d.is_ok());

    let dispatched_d = driver.get_dispatched_actions();
    assert_eq!(dispatched_d.len(), 4);
    match &dispatched_d[3] {
        CuaAction::TypeText { text, .. } => {
            assert_eq!(text, "Delta", "press_enter: false must not append newline");
        }
        other => panic!("Expected TypeText, got: {:?}", other),
    }
}

#[test]
fn test_adversarial_m3_it2_diff_padded_roi_truncated_buffer_bounds_checking() {
    let roi = CuaRect::new(10, 10, 50, 50);

    // 1. Zero-byte buffer for frame_before (100x100 Rgba expects 40,000 bytes)
    let empty_before = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![],
    };
    let valid_frame = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![128u8; 40000],
    };

    let err1 = RoiDiffEngine::diff_padded_roi(&empty_before, &valid_frame, &roi, 5)
        .expect_err("Empty frame_before must fail bounds check");
    match err1 {
        CuaError::InvalidParameter(msg) => {
            assert!(msg.contains("frame_before buffer truncated"));
        }
        other => panic!("Expected CuaError::InvalidParameter, got: {:?}", other),
    }

    // 2. Zero-byte buffer for frame_after
    let err2 = RoiDiffEngine::diff_padded_roi(&valid_frame, &empty_before, &roi, 5)
        .expect_err("Empty frame_after must fail bounds check");
    match err2 {
        CuaError::InvalidParameter(msg) => {
            assert!(msg.contains("frame_after buffer truncated"));
        }
        other => panic!("Expected CuaError::InvalidParameter, got: {:?}", other),
    }

    // 3. Off-by-one truncated buffer (39,999 bytes)
    let off_by_one = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![128u8; 39999],
    };
    let err3 = RoiDiffEngine::diff_padded_roi(&off_by_one, &valid_frame, &roi, 5)
        .expect_err("Off-by-one truncated buffer must fail bounds check");
    assert!(matches!(err3, CuaError::InvalidParameter(_)));

    // 4. Integer overflow dimensions (u32::MAX)
    let overflow_frame = RawFrame {
        width: u32::MAX,
        height: u32::MAX,
        format: CuaPixelFormat::Rgba,
        data: vec![0u8; 100],
    };
    let err4 = RoiDiffEngine::diff_padded_roi(&overflow_frame, &overflow_frame, &roi, 5)
        .expect_err("Overflowing dimensions must return InvalidParameter");
    match err4 {
        CuaError::InvalidParameter(msg) => {
            assert!(msg.contains("overflow"));
        }
        other => panic!(
            "Expected CuaError::InvalidParameter for dimension overflow, got: {:?}",
            other
        ),
    }

    // 5. Extreme out-of-bounds ROI on valid frames: must clamp and not panic
    let oob_roi = CuaRect::new(-50, -50, 300, 300);
    let oob_res = RoiDiffEngine::diff_padded_roi(&valid_frame, &valid_frame, &oob_roi, 5);
    assert!(
        oob_res.is_ok(),
        "Out-of-bounds ROI must be safely clamped without panic: {:?}",
        oob_res
    );
    let (changed, total, ratio) = oob_res.unwrap();
    assert_eq!(changed, 0);
    assert!(total > 0);
    assert_eq!(ratio, 0.0);

    // 6. Zero dimension ROI: returns (0, 0, 0.0) cleanly
    let zero_roi = CuaRect::new(10, 10, 0, 0);
    let zero_res = RoiDiffEngine::diff_padded_roi(&valid_frame, &valid_frame, &zero_roi, 5);
    assert!(zero_res.is_ok());
    assert_eq!(zero_res.unwrap(), (0, 0, 0.0));

    // 7. Full diff_roi pipeline with truncated buffer returns Err(CuaError::InvalidParameter)
    let diff_roi_err = RoiDiffEngine::diff_roi(
        &empty_before,
        &valid_frame,
        &roi,
        &roi,
        5,
        &SemanticVerificationCriteria::click_default(),
    );
    assert!(
        diff_roi_err.is_err(),
        "diff_roi with truncated buffer must fail gracefully"
    );
    assert!(matches!(
        diff_roi_err.unwrap_err(),
        CuaError::InvalidParameter(_)
    ));
}
