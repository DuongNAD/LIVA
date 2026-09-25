//! Empirical Challenger 1 Adversarial Test Suite for Milestone M3.
//!
//! Subsystems: Coordinate Translation, Multi-Space Geometry, Client Containment,
//! Z-Order Occlusion, Window State Invariants, and Security Governor Pre-Checks.
//!
//! Scope:
//! 1. Coordinate Normalization Edge Cases (< 0.0, > 1.0, NaN, Inf, negative virtual desk origins, degenerate 0-sized boxes)
//! 2. Client Coordinate Containment & Lockout Zones (title bar caption, close button [X], negative client coordinates -> 100% rejection)
//! 3. Z-Order Occlusion & Window State Invariants (occluded -> TargetOccluded, minimized -> WindowMinimized, invalid HWND -> WindowNotFound)
//! 4. Security Governor Integration (Standard mode blocks mutating actions, Bounded mode enforces allowlist/denylist/hotkeys/UIPI)

use liva_cua::mock::MockCuaDriver;
use liva_cua::router::{
    DefectCategory, PreCheckPipeline, RemedyHint, System1Action, System1ActionRouter,
    System1RouterConfig, TargetCoordinate, TargetDescriptor, TargetResolver, WindowSelector,
};
use liva_cua::traits::CuaDriverTrait;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaPoint, CuaRect, CuaWindowInfo, MouseButton, PermissionMode,
};
use liva_cua::vision::diff::{apply_roi_padding, crop_raw_frame, DefaultVisualVerifier};
use liva_cua::vision::mock::MockScreenCapturer;
use liva_cua::vision::types::{CuaPixelFormat, RawFrame, RoiDiffConfig};
use liva_cua::CuaEngine;
use std::sync::Arc;

/// Helper constructing an isolated System1ActionRouter and CuaEngine test environment.
fn create_test_router(
    perm_mode: PermissionMode,
    allowlist: Vec<String>,
) -> (
    Arc<System1ActionRouter>,
    Arc<MockCuaDriver>,
    Arc<MockScreenCapturer>,
    Arc<CuaEngine>,
) {
    let cua_config = CuaConfig {
        permission_mode: perm_mode,
        process_allowlist: allowlist,
        ..Default::default()
    };

    let driver = Arc::new(MockCuaDriver::new());
    let engine = Arc::new(CuaEngine::new_with_driver(cua_config, driver.clone()));

    // Seed default target mock window (100, 100, 800, 600)
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

    let capturer = Arc::new(MockScreenCapturer::new(
        1920,
        1080,
        CuaPixelFormat::Rgba,
        128,
    ));
    let verifier = Arc::new(DefaultVisualVerifier::new(
        capturer.clone(),
        RoiDiffConfig {
            redraw_micro_tick_ms: 1,
            ..Default::default()
        },
    ));

    let router_config = System1RouterConfig {
        max_step_retries: 2,
        allow_foreground_escalation: true,
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

// ============================================================================
// AREA 1: Coordinate Normalization Edge Cases
// ============================================================================

#[tokio::test]
async fn test_adversarial_coordinate_normalization_out_of_range_and_special_floats() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);
    let window_bounds = CuaRect::new(100, 100, 800, 600);
    let client_bounds = CuaRect::new(108, 130, 784, 562);

    // 1. ClientNormalized with x, y >= 1.0 (e.g. 1.0 and 1.5): clamps to (width - 1, height - 1)
    // Resolves to the valid bottom-right pixel of the client drawing surface and succeeds pre-checks!
    let coord_overflow = TargetCoordinate::ClientNormalized { x: 1.5, y: 1.5 };
    let (s_pt, c_pt) = TargetResolver::translate_coordinates(
        &coord_overflow,
        &window_bounds,
        &client_bounds,
        None,
    )
    .expect("Clamped translation must compute without panic");
    assert_eq!(c_pt.x, client_bounds.width - 1);
    assert_eq!(c_pt.y, client_bounds.height - 1);
    assert_eq!(s_pt.x, client_bounds.x + client_bounds.width - 1);
    assert_eq!(s_pt.y, client_bounds.y + client_bounds.height - 1);

    // Verify genuinely out-of-bounds explicit pixel coordinates remain strictly rejected:
    let action_oob = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel {
                x: client_bounds.width + 10,
                y: client_bounds.height + 10,
            },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_oob = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_oob, 2, true)
        .await
        .expect_err("Point outside client drawing surface must be rejected by pre-check");
    assert_eq!(err_oob.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err_oob.error_code, "coordinate_out_of_bounds");

    // 2. WindowNormalized with x, y > 1.0: window outer corner (900, 700) -> outside client rect
    let coord_win_overflow = TargetCoordinate::WindowNormalized { x: 1.5, y: 1.5 };
    let action_win_overflow = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_win_overflow),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_win = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_win_overflow, 2, true)
        .await
        .expect_err("WindowNormalized overflow must be rejected by pre-check");
    assert_eq!(err_win.failure_category, DefectCategory::OutOfBounds);

    // 4. WindowNormalized with x, y < 0.0: clamps to (0.0, 0.0) -> top-left of window (100, 100)
    // Client area starts at (108, 130), so cx = -8, cy = -30. Must be rejected as TitleBarCaptionProtected!
    let coord_win_underflow = TargetCoordinate::WindowNormalized { x: -0.5, y: -0.5 };
    let action_win_underflow = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_win_underflow),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_win_under = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_win_underflow, 2, true)
        .await
        .expect_err("WindowNormalized underflow into title bar must be rejected");
    assert_eq!(err_win_under.failure_category, DefectCategory::OutOfBounds);
    assert!(err_win_under
        .error_message
        .contains("TitleBarCaptionProtected"));

    // 5. WindowNormalized hitting close button [X] zone: (x: 0.98, y: 0.02) -> sx = 884, sy = 112
    let coord_close = TargetCoordinate::WindowNormalized { x: 0.98, y: 0.02 };
    let action_close = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_close),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_close = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_close, 2, true)
        .await
        .expect_err("Click in close button zone via WindowNormalized must be rejected");
    assert_eq!(err_close.failure_category, DefectCategory::OutOfBounds);
    assert!(err_close.error_message.contains("CloseButtonProtected"));

    // 6. RoiNormalized with out-of-range floats: clamps to [0.0, 1.0]
    let roi = CuaRect::new(200, 200, 100, 50);
    let coord_roi_overflow = TargetCoordinate::RoiNormalized {
        roi,
        norm_x: 2.0,
        norm_y: -1.0,
    };
    let (s_roi, c_roi) = TargetResolver::translate_coordinates(
        &coord_roi_overflow,
        &window_bounds,
        &client_bounds,
        None,
    )
    .expect("RoiNormalized out of range must clamp without panic");
    assert_eq!(s_roi.x, 299); // 200 + (100 - 1) * 1.0
    assert_eq!(s_roi.y, 200); // 200 + (50 - 1) * 0.0
    assert_eq!(c_roi.x, 299 - client_bounds.x);
    assert_eq!(c_roi.y, 200 - client_bounds.y);
}

#[tokio::test]
async fn test_adversarial_nan_coordinate_handling_across_spaces() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);
    let window_bounds = CuaRect::new(100, 100, 800, 600);
    let client_bounds = CuaRect::new(108, 130, 784, 562);

    // 1. WindowNormalized with NaN: Rust float saturation produces 0 offset -> (sx, sy) = (window_bounds.x, window_bounds.y)
    // cy = 100 - 130 = -30 -> Pre-check MUST reject with TitleBarCaptionProtected
    let coord_win_nan = TargetCoordinate::WindowNormalized {
        x: f32::NAN,
        y: f32::NAN,
    };
    let (s_win_nan, c_win_nan) =
        TargetResolver::translate_coordinates(&coord_win_nan, &window_bounds, &client_bounds, None)
            .expect("WindowNormalized NaN translation must not panic");
    assert_eq!(s_win_nan.x, 100);
    assert_eq!(s_win_nan.y, 100);
    assert_eq!(c_win_nan.x, -8);
    assert_eq!(c_win_nan.y, -30);

    let action_win_nan = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_win_nan),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_win_nan = router
        .execute_step_with_escalation("sess_nan", None, 0, 1, 0, action_win_nan, 2, true)
        .await
        .expect_err("WindowNormalized with NaN must be rejected as OutOfBounds");
    assert_eq!(err_win_nan.failure_category, DefectCategory::OutOfBounds);
    assert!(err_win_nan
        .error_message
        .contains("TitleBarCaptionProtected"));

    // 2. ScreenNormalized with NaN: maps to virtual desktop origin (virt.x, virt.y)
    // Outside target window (100, 100, 800, 600) client area -> Pre-check MUST reject
    let coord_screen_nan = TargetCoordinate::ScreenNormalized {
        x: f32::NAN,
        y: f32::NAN,
    };
    let action_screen_nan = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_screen_nan),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_screen_nan = router
        .execute_step_with_escalation("sess_nan", None, 0, 1, 0, action_screen_nan, 2, true)
        .await
        .expect_err("ScreenNormalized with NaN must be rejected as OutOfBounds");
    assert_eq!(err_screen_nan.failure_category, DefectCategory::OutOfBounds);

    // 3. ClientNormalized with NaN: Rust float saturation produces 0 -> (cx = 0, cy = 0)
    let coord_client_nan = TargetCoordinate::ClientNormalized {
        x: f32::NAN,
        y: f32::NAN,
    };
    let (s_client_nan, c_client_nan) = TargetResolver::translate_coordinates(
        &coord_client_nan,
        &window_bounds,
        &client_bounds,
        None,
    )
    .expect("ClientNormalized NaN translation must not panic");
    assert_eq!(c_client_nan.x, 0);
    assert_eq!(c_client_nan.y, 0);
    assert_eq!(s_client_nan.x, client_bounds.x);
    assert_eq!(s_client_nan.y, client_bounds.y);
}

#[tokio::test]
async fn test_adversarial_negative_virtual_desktop_origins_multi_monitor() {
    let (router, driver, _, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // Setup a secondary monitor positioned to the left of the primary monitor:
    // Monitor 2 bounds: (-1920, 0, 1920, 1080)
    // Place target window on Monitor 2: (-1600, 100, 800, 600)
    let neg_window = CuaWindowInfo {
        hwnd: 0x9999,
        pid: 2000,
        title: "Secondary Monitor App".into(),
        class_name: "SecClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(-1600, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(neg_window);

    // 1. Valid click inside negative monitor client bounds
    let _action_valid = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x9999),
            TargetCoordinate::ClientPixel { x: 200, y: 150 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };

    let resolved = TargetResolver::resolve(
        &TargetDescriptor::new(
            WindowSelector::Hwnd(0x9999),
            TargetCoordinate::ClientPixel { x: 200, y: 150 },
        ),
        driver.as_ref(),
    )
    .await
    .expect("Resolving window on negative coordinate monitor must succeed");

    assert_eq!(resolved.screen_point.x, -1600 + 8 + 200); // -1392
    assert_eq!(resolved.screen_point.y, 100 + 30 + 150); // 280
    assert_eq!(resolved.client_point.x, 200);
    assert_eq!(resolved.client_point.y, 150);

    // 2. Cross-monitor click: Window is on negative monitor, but AbsoluteScreen coordinate
    // points to primary monitor (x = 500, y = 300) -> outside window client area
    let action_cross = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x9999),
            TargetCoordinate::AbsoluteScreen(CuaPoint::new(500, 300)),
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_cross = router
        .execute_step_with_escalation("sess_neg", None, 0, 1, 0, action_cross, 2, true)
        .await
        .expect_err("Cross-monitor click outside window client surface must be rejected");
    assert_eq!(err_cross.failure_category, DefectCategory::OutOfBounds);

    // 3. Mathematical round-trip check across negative virtual desktop boundaries
    let virt_layouts = [
        (-1920, 0, 3840, 1080),     // secondary left
        (0, -1080, 1920, 2160),     // secondary top
        (-1920, -1080, 3840, 2160), // dual top-left
    ];

    for (vx, vy, vw, vh) in virt_layouts {
        let test_points = [
            (vx, vy),
            (vx + 500, vy + 200),
            (0, 0),
            (vw / 2, vh / 2),
            (vx + vw - 1, vy + vh - 1),
        ];
        for (sx, sy) in test_points {
            let (dx, dy) = liva_cua::geometry::to_virtualdesk_absolute(sx, sy, vx, vy, vw, vh);
            assert!((0..=65535).contains(&dx), "dx out of range: {}", dx);
            assert!((0..=65535).contains(&dy), "dy out of range: {}", dy);

            let (rec_sx, rec_sy) =
                liva_cua::geometry::from_virtualdesk_absolute(dx, dy, vx, vy, vw, vh);
            assert!(
                (rec_sx - sx).abs() <= 1,
                "Virtual desk X recovery drifted: expected {}, got {}",
                sx,
                rec_sx
            );
            assert!(
                (rec_sy - sy).abs() <= 1,
                "Virtual desk Y recovery drifted: expected {}, got {}",
                sy,
                rec_sy
            );
        }
    }
}

#[tokio::test]
async fn test_adversarial_degenerate_bounding_boxes_zero_and_negative_dimensions() {
    let (router, driver, _, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // 1. Zero-sized window (width = 0, height = 0)
    let zero_window = CuaWindowInfo {
        hwnd: 0x2222,
        pid: 3000,
        title: "Zero Sized App".into(),
        class_name: "ZeroClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(200, 200, 0, 0),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(zero_window);

    let action_zero = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x2222), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_zero = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_zero, 2, true)
        .await
        .expect_err("Zero-sized window must fail pre-check");
    assert_eq!(err_zero.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_zero.error_code, "window_empty_bounds");

    // 2. Negative-sized window (width = -100, height = -50)
    let neg_window = CuaWindowInfo {
        hwnd: 0x3333,
        pid: 3001,
        title: "Negative Bounds App".into(),
        class_name: "NegClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(200, 200, -100, -50),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(neg_window);

    let action_neg = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x3333), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_neg = router
        .execute_step_with_escalation("sess_01", None, 0, 1, 0, action_neg, 2, true)
        .await
        .expect_err("Negative bounds window must fail pre-check");
    assert_eq!(err_neg.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_neg.error_code, "window_empty_bounds");

    // 3. Degenerate frame crop operations: verify crop_raw_frame rejects 0 and negative sizes
    let frame = RawFrame {
        width: 100,
        height: 100,
        format: CuaPixelFormat::Rgba,
        data: vec![0u8; 100 * 100 * 4],
    };
    assert!(crop_raw_frame(&frame, &CuaRect::new(0, 0, 0, 0)).is_err());
    assert!(crop_raw_frame(&frame, &CuaRect::new(10, 10, -5, 20)).is_err());

    // 4. Degenerate ROI padding: apply_roi_padding guarantees >= 1px dimension
    let padded = apply_roi_padding(&CuaRect::new(50, 50, 0, 0), 16, 1920, 1080);
    assert!(padded.width >= 1);
    assert!(padded.height >= 1);
}

// ============================================================================
// AREA 2: Client Coordinate Containment & Lockout Zones
// ============================================================================

#[tokio::test]
async fn test_adversarial_title_bar_caption_lockout_exhaustive_points() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // Target window: bounds (100, 100, 800, 600), client area (108, 130, 784, 562)
    // Any point where cy < 0 corresponds to the title bar caption (sy in [100..129]).
    // Sweep points across different caption coordinates:
    let caption_test_points = [
        (50, -1),   // 1px above client area
        (50, -15),  // Middle of caption bar
        (50, -29),  // Top of caption bar
        (200, -10), // Center x, middle caption
        (400, -20), // Center x, upper caption
        (700, -5),  // Right side x, near caption
    ];

    for (cx, cy) in caption_test_points {
        let action = System1Action::Click {
            target: TargetDescriptor::new(
                WindowSelector::Hwnd(0x1234),
                TargetCoordinate::ClientPixel { x: cx, y: cy },
            ),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
        };

        let err = router
            .execute_step_with_escalation("sess_title", None, 0, 1, 0, action, 2, true)
            .await
            .expect_err("Must reject caption click");

        assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
        assert_eq!(err.error_code, "coordinate_out_of_bounds");
        assert!(
            err.error_message.contains("TitleBarCaptionProtected"),
            "Error for ({}, {}) must cite TitleBarCaptionProtected, got: {}",
            cx,
            cy,
            err.error_message
        );
    }
}

#[tokio::test]
async fn test_adversarial_close_button_lockout_zone_exhaustive() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // Target window bounds: (100, 100, 800, 600)
    // Close button protected zone: sx >= 100 + 800 - 50 = 850 && sy < 100 + 40 = 140
    let close_zone_points = [
        (850, 100), // Top-left of close zone
        (899, 100), // Top-right of close zone
        (850, 139), // Bottom-left of close zone
        (899, 139), // Bottom-right of close zone
        (875, 120), // Exact center of [X] button
        (890, 110), // Upper right quadrant of [X]
    ];

    for (sx, sy) in close_zone_points {
        let action = System1Action::Click {
            target: TargetDescriptor::new(
                WindowSelector::Hwnd(0x1234),
                TargetCoordinate::ScreenPixel { x: sx, y: sy },
            ),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
        };

        let err = router
            .execute_step_with_escalation("sess_close", None, 0, 1, 0, action, 2, true)
            .await
            .expect_err("Must reject close button click");

        assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
        assert!(
            err.error_message.contains("CloseButtonProtected"),
            "Error for ({}, {}) must cite CloseButtonProtected, got: {}",
            sx,
            sy,
            err.error_message
        );
    }

    // Border condition: 1px below close zone (sx: 875, sy: 140) -> falls in client area, NOT close button
    let client_y = 140 - 130; // cy = 10
    let client_x = 875 - 108; // cx = 767 (< 784)
    let action_safe = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel {
                x: client_x,
                y: client_y,
            },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    // Pre-check should pass for valid client coordinate
    let resolved = TargetResolver::resolve(
        &TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel {
                x: client_x,
                y: client_y,
            },
        ),
        router.driver().as_ref(),
    )
    .await
    .unwrap();
    let precheck_res = PreCheckPipeline::validate(
        &action_safe,
        &resolved,
        router.driver().as_ref(),
        router.security_governor().as_ref(),
        router.kill_switch().as_ref(),
    )
    .await;
    assert!(
        precheck_res.is_ok(),
        "Point below close zone must pass pre-check"
    );
}

#[tokio::test]
async fn test_adversarial_negative_and_overflowing_client_coordinates_100_percent_rejection() {
    let (router, _, _, _) = create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);
    // Client bounds: 784 x 562

    let out_of_bounds_points = [
        (-1, 50),     // Negative x
        (-100, 50),   // Extreme negative x
        (50, -1),     // Negative y (title bar)
        (50, -100),   // Extreme negative y
        (-10, -10),   // Negative x and y
        (784, 50),    // Exactly equal to width
        (1000, 50),   // Far right overflow
        (50, 562),    // Exactly equal to height
        (50, 1000),   // Far bottom overflow
        (784, 562),   // Overflow both x and y
        (2000, 2000), // Extreme positive overflow
    ];

    for (cx, cy) in out_of_bounds_points {
        let action = System1Action::Click {
            target: TargetDescriptor::new(
                WindowSelector::Hwnd(0x1234),
                TargetCoordinate::ClientPixel { x: cx, y: cy },
            ),
            button: MouseButton::Left,
            click_count: 1,
            delivery_mode: None,
        };

        let err = router
            .execute_step_with_escalation("sess_oob", None, 0, 1, 0, action, 2, true)
            .await
            .expect_err("Must reject out-of-bounds client coordinate");

        assert_eq!(err.failure_category, DefectCategory::OutOfBounds);
        assert_eq!(err.error_code, "coordinate_out_of_bounds");
    }
}

// ============================================================================
// AREA 3: Z-Order Occlusion & Window State Invariants
// ============================================================================

#[tokio::test]
async fn test_adversarial_zorder_occlusion_and_modal_dialogs() {
    let (router, driver, _, _) = create_test_router(
        PermissionMode::Bounded,
        vec!["notepad.exe".into(), "dialog.exe".into()],
    );

    // Demote target window 0x1234 to z_index = 1
    let target = driver.inspect_window(0x1234).await.unwrap();
    if let liva_cua::types::WindowStateVerdict::Valid {
        window_info: Some(mut info),
        ..
    } = target
    {
        info.z_index = 1;
        driver.insert_window(info);
    }

    // Modal dialog at z_index = 0 covering screen (150, 150, 200, 200)
    let modal = CuaWindowInfo {
        hwnd: 0x8888,
        pid: 4000,
        title: "Security Alert Modal".into(),
        class_name: "ModalClass".into(),
        process_name: Some("dialog.exe".into()),
        bounds: CuaRect::new(150, 150, 200, 200),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0, // Topmost!
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(modal);

    // 1. Point (200, 200) is occluded by the modal dialog
    let action_occluded = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ScreenPixel { x: 200, y: 200 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_occ = router
        .execute_step_with_escalation("sess_occ", None, 0, 1, 0, action_occluded.clone(), 2, true)
        .await
        .expect_err("Point inside overlapping modal must be rejected as TargetOccluded");

    assert_eq!(err_occ.failure_category, DefectCategory::TargetOccluded);
    assert_eq!(err_occ.error_code, "target_occluded");
    assert!(matches!(
        err_occ.remedy_hint,
        RemedyHint::BringWindowToForeground(_)
    ));

    // 2. Point (500, 400) is OUTSIDE the modal dialog -> not occluded
    let client_x = 500 - 108;
    let client_y = 400 - 130;
    let action_clear = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel {
                x: client_x,
                y: client_y,
            },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let resolved_clear = TargetResolver::resolve(
        &TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel {
                x: client_x,
                y: client_y,
            },
        ),
        driver.as_ref(),
    )
    .await
    .unwrap();
    let precheck_clear = PreCheckPipeline::validate(
        &action_clear,
        &resolved_clear,
        driver.as_ref(),
        router.security_governor().as_ref(),
        router.kill_switch().as_ref(),
    )
    .await;
    assert!(
        precheck_clear.is_ok(),
        "Point outside modal dialog must NOT be occluded"
    );

    // 3. Window BEHIND target at z_index = 2 covering (200, 200) -> MUST NOT occlude target
    let behind = CuaWindowInfo {
        hwnd: 0x7777,
        pid: 5000,
        title: "Background Window".into(),
        class_name: "BgClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 2, // Behind target (target has z_index = 1)
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(behind);
    // Remove the modal to test behind window isolation
    let mut w_lock = driver.windows.lock().unwrap();
    w_lock.remove(&0x8888);
    drop(w_lock);

    let resolved_behind_test = TargetResolver::resolve(
        &TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ScreenPixel { x: 200, y: 200 },
        ),
        driver.as_ref(),
    )
    .await
    .unwrap();
    let precheck_behind = PreCheckPipeline::validate(
        &action_occluded,
        &resolved_behind_test,
        driver.as_ref(),
        router.security_governor().as_ref(),
        router.kill_switch().as_ref(),
    )
    .await;
    assert!(
        precheck_behind.is_ok(),
        "Window with higher z-index (behind) must NOT occlude target"
    );

    // 4. Minimized window at z_index = 0 covering (200, 200) -> MUST NOT occlude target
    let min_modal = CuaWindowInfo {
        hwnd: 0x8889,
        pid: 4001,
        title: "Minimized Dialog".into(),
        class_name: "ModalClass".into(),
        process_name: Some("dialog.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 200, 200),
        is_on_screen: false,
        is_minimized: true,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(min_modal);

    let precheck_min = PreCheckPipeline::validate(
        &action_occluded,
        &resolved_behind_test,
        driver.as_ref(),
        router.security_governor().as_ref(),
        router.kill_switch().as_ref(),
    )
    .await;
    assert!(
        precheck_min.is_ok(),
        "Minimized window must NOT cause occlusion"
    );
}

#[tokio::test]
async fn test_adversarial_window_state_invariants_minimized_and_hidden() {
    let (router, driver, _, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // 1. Explicitly minimized window (is_minimized: true)
    let min_window = CuaWindowInfo {
        hwnd: 0x4444,
        pid: 6000,
        title: "Minimized App".into(),
        class_name: "MinClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: false,
        is_minimized: true,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(min_window);

    let action_min = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x4444), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_min = router
        .execute_step_with_escalation("sess_min", None, 0, 1, 0, action_min, 2, true)
        .await
        .expect_err("Minimized window must be refused");
    assert_eq!(err_min.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_min.error_code, "window_minimized");
    assert_eq!(
        err_min.retry_count, 0,
        "Pre-check errors must fail closed with 0 retries"
    );
    assert!(matches!(
        err_min.remedy_hint,
        RemedyHint::RestoreMinimizedWindow(_)
    ));

    // 2. Classic Win32 minimized sentinel (-32000, -32000)
    let sentinel_window = CuaWindowInfo {
        hwnd: 0x4445,
        pid: 6001,
        title: "Sentinel Minimized App".into(),
        class_name: "SentinelClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 160, 24),
        is_on_screen: false,
        is_minimized: false, // flag false, but sentinel coordinates true!
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(sentinel_window);

    let action_sentinel = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x4445), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_sentinel = router
        .execute_step_with_escalation("sess_sentinel", None, 0, 1, 0, action_sentinel, 2, true)
        .await
        .expect_err("Sentinel minimized window must be refused");
    assert_eq!(
        err_sentinel.failure_category,
        DefectCategory::PreCheckFailed
    );
    assert_eq!(err_sentinel.error_code, "window_minimized");

    // 3. Hidden window (is_on_screen: false)
    let hidden_window = CuaWindowInfo {
        hwnd: 0x4446,
        pid: 6002,
        title: "Hidden App".into(),
        class_name: "HiddenClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: false,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(hidden_window);

    let action_hidden = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x4446), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_hidden = router
        .execute_step_with_escalation("sess_hidden", None, 0, 1, 0, action_hidden, 2, true)
        .await
        .expect_err("Hidden window must be refused");
    assert_eq!(err_hidden.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_hidden.error_code, "window_hidden");
}

#[tokio::test]
async fn test_adversarial_invalid_and_dead_hwnd_detection() {
    let (router, driver, _, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // 1. HWND 0 (null handle)
    let action_null = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_null = router
        .execute_step_with_escalation("sess_null", None, 0, 1, 0, action_null, 2, true)
        .await
        .expect_err("Null HWND must be rejected");
    assert_eq!(err_null.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_null.error_code, "window_not_found");
    assert!(matches!(err_null.remedy_hint, RemedyHint::WindowDied(_)));

    // 2. Non-existent HWND (0xDEAD_BEEF)
    let action_dead = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0xDEAD_BEEF),
            TargetCoordinate::default(),
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_dead = router
        .execute_step_with_escalation("sess_dead", None, 0, 1, 0, action_dead, 2, true)
        .await
        .expect_err("Dead HWND must be rejected");
    assert_eq!(err_dead.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_dead.error_code, "window_not_found");

    // 3. Window deleted from driver mid-task
    let mut w_lock = driver.windows.lock().unwrap();
    w_lock.remove(&0x1234);
    drop(w_lock);

    let action_removed = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), TargetCoordinate::default()),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_removed = router
        .execute_step_with_escalation("sess_rem", None, 0, 1, 0, action_removed, 2, true)
        .await
        .expect_err("Destroyed window must be rejected");
    assert_eq!(err_removed.failure_category, DefectCategory::PreCheckFailed);
    assert_eq!(err_removed.error_code, "window_not_found");
}

// ============================================================================
// AREA 4: Security Governor Integration in Pre-Checks
// ============================================================================

#[tokio::test]
async fn test_adversarial_standard_mode_blocks_all_mutating_actions_fail_closed() {
    let (router, driver, capturer, _) =
        create_test_router(PermissionMode::Standard, vec!["notepad.exe".into()]);

    let target = TargetDescriptor::new(
        WindowSelector::Hwnd(0x1234),
        TargetCoordinate::ClientPixel { x: 50, y: 50 },
    );

    // 1. Click
    let click = System1Action::Click {
        target: target.clone(),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_click = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, click, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_click.failure_category, DefectCategory::SecurityRefused);

    // 2. DoubleClick
    let dclick = System1Action::DoubleClick {
        target: target.clone(),
    };
    let err_dclick = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, dclick, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_dclick.failure_category, DefectCategory::SecurityRefused);

    // 3. Hover (MoveCursor)
    let hover = System1Action::Hover {
        target: target.clone(),
        duration_ms: None,
    };
    let err_hover = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, hover, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_hover.failure_category, DefectCategory::SecurityRefused);

    // 4. Drag
    let drag = System1Action::Drag {
        start: target.clone(),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        ),
        button: None,
        steps: 5,
        delivery_mode: None,
    };
    let err_drag = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, drag, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_drag.failure_category, DefectCategory::SecurityRefused);

    // 5. Scroll
    let scroll = System1Action::Scroll {
        target: Some(target.clone()),
        delta_x: 0,
        delta_y: 120,
    };
    let err_scroll = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, scroll, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_scroll.failure_category, DefectCategory::SecurityRefused);

    // 6. TypeText
    let type_text = System1Action::TypeText {
        target: Some(target.clone()),
        text: "hello".into(),
        clear_before: false,
        press_enter: false,
        delivery_mode: None,
    };
    let err_type = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, type_text, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_type.failure_category, DefectCategory::SecurityRefused);

    // 7. Hotkey
    let hotkey = System1Action::Hotkey {
        target: Some(target.clone()),
        keys: vec!["Ctrl".into(), "C".into()],
        delivery_mode: None,
    };
    let err_hotkey = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, hotkey, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_hotkey.failure_category, DefectCategory::SecurityRefused);

    // Invariant: driver must have received ZERO dispatched mutating actions!
    assert!(
        driver.get_dispatched_actions().is_empty(),
        "Standard mode must reject mutating actions in pre-check without dispatching to driver"
    );

    // Non-mutating actions permitted in Standard mode: BringToFront & RestoreWindow
    let btf = System1Action::BringToFront {
        target: target.clone(),
    };
    let resolved = TargetResolver::resolve(&target, driver.as_ref())
        .await
        .unwrap();
    assert!(
        PreCheckPipeline::validate(
            &btf,
            &resolved,
            driver.as_ref(),
            router.security_governor().as_ref(),
            router.kill_switch().as_ref()
        )
        .await
        .is_ok(),
        "BringToFront must pass pre-check in Standard mode"
    );

    let restore = System1Action::RestoreWindow {
        target: target.clone(),
    };
    assert!(
        PreCheckPipeline::validate(
            &restore,
            &resolved,
            driver.as_ref(),
            router.security_governor().as_ref(),
            router.kill_switch().as_ref()
        )
        .await
        .is_ok(),
        "RestoreWindow must pass pre-check in Standard mode"
    );

    // When visual change is confirmed, execution succeeds in Standard mode
    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);
    let res_btf = router
        .execute_step_with_escalation("sess_std", None, 0, 1, 0, btf, 2, true)
        .await;
    assert!(
        res_btf.is_ok(),
        "BringToFront must succeed when visual change is confirmed"
    );
}

#[tokio::test]
async fn test_adversarial_bounded_mode_process_governance_allowlist_denylist_hotkeys_uipi() {
    let (router, driver, _, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // 1. Process not in allowlist (calc.exe)
    let calc_window = CuaWindowInfo {
        hwnd: 0x5001,
        pid: 7001,
        title: "Calculator".into(),
        class_name: "CalcClass".into(),
        process_name: Some("calc.exe".into()),
        bounds: CuaRect::new(100, 100, 400, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(calc_window);

    let action_calc = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5001),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_calc = router
        .execute_step_with_escalation("sess_gov", None, 0, 1, 0, action_calc, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_calc.failure_category, DefectCategory::SecurityRefused);
    assert_eq!(err_calc.error_code, "security_policy_refused");
    assert!(matches!(
        err_calc.remedy_hint,
        RemedyHint::SecurityPolicyViolation(_)
    ));

    // 2. Denylisted process (taskmgr.exe) - denylist takes precedence over allowlist
    let taskmgr_window = CuaWindowInfo {
        hwnd: 0x5002,
        pid: 7002,
        title: "Task Manager".into(),
        class_name: "TaskManagerWindow".into(),
        process_name: Some("taskmgr.exe".into()),
        bounds: CuaRect::new(100, 100, 600, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(taskmgr_window);

    // Adversarially add taskmgr.exe to allowlist
    router
        .security_governor()
        .set_allowlist(vec!["notepad.exe".into(), "taskmgr.exe".into()]);

    let action_tm = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5002),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_tm = router
        .execute_step_with_escalation("sess_gov", None, 0, 1, 0, action_tm, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_tm.failure_category, DefectCategory::SecurityRefused);
    assert_eq!(err_tm.error_code, "security_policy_refused");
    assert!(matches!(
        err_tm.remedy_hint,
        RemedyHint::SecurityPolicyViolation(_)
    ));

    // 3. Dangerous hotkeys: Alt+F4, Ctrl+Alt+Del, Win+L, Win+R, Ctrl+Shift+Esc
    let dangerous_combos = [
        vec!["Alt".into(), "F4".into()],
        vec!["Ctrl".into(), "Alt".into(), "Delete".into()],
        vec!["Win".into(), "L".into()],
        vec!["Win".into(), "R".into()],
        vec!["Ctrl".into(), "Shift".into(), "Escape".into()],
    ];

    for keys in dangerous_combos {
        let hotkey_action = System1Action::Hotkey {
            target: Some(TargetDescriptor::new(
                WindowSelector::Hwnd(0x1234),
                TargetCoordinate::default(),
            )),
            keys: keys.clone(),
            delivery_mode: None,
        };
        let err_hotkey = router
            .execute_step_with_escalation("sess_gov", None, 0, 1, 0, hotkey_action, 2, true)
            .await
            .unwrap_err();
        assert_eq!(err_hotkey.failure_category, DefectCategory::SecurityRefused);
        assert_eq!(err_hotkey.error_code, "security_policy_refused");
        assert!(err_hotkey
            .error_message
            .contains("Dangerous hotkey rejected"));
    }

    // Safe hotkey (Ctrl+C) on allowlisted notepad.exe should pass pre-check
    let safe_hotkey = System1Action::Hotkey {
        target: Some(TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::default(),
        )),
        keys: vec!["Ctrl".into(), "C".into()],
        delivery_mode: None,
    };
    let resolved_notepad = TargetResolver::resolve(
        &TargetDescriptor::new(WindowSelector::Hwnd(0x1234), TargetCoordinate::default()),
        driver.as_ref(),
    )
    .await
    .unwrap();
    let precheck_safe = PreCheckPipeline::validate(
        &safe_hotkey,
        &resolved_notepad,
        driver.as_ref(),
        router.security_governor().as_ref(),
        router.kill_switch().as_ref(),
    )
    .await;
    assert!(
        precheck_safe.is_ok(),
        "Safe hotkey Ctrl+C must pass pre-check"
    );

    // 4. UIPI Elevation Barrier
    // Agent is at Medium integrity (0x2000). Set target window 0x1234 to High integrity (0x3000).
    router.security_governor().set_simulated_agent_rid(0x2000);
    router
        .security_governor()
        .set_simulated_uipi(0x1234, 0x3000);

    let action_uipi = System1Action::Click {
        target: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_uipi = router
        .execute_step_with_escalation("sess_gov", None, 0, 1, 0, action_uipi, 2, true)
        .await
        .unwrap_err();
    assert_eq!(err_uipi.failure_category, DefectCategory::SecurityRefused);
    assert_eq!(err_uipi.error_code, "security_policy_refused");
    assert!(matches!(
        err_uipi.remedy_hint,
        RemedyHint::ElevationRequired(_)
    ));
}

// ============================================================================
// AREA 5: M3 It2 Remediation — Drag Dual-Target & Coordinate Boundary Stress
// ============================================================================

#[tokio::test]
async fn test_adversarial_drag_arbitrary_distinct_coordinates_and_spaces() {
    let (router, driver, capturer, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    // Window bounds: (100, 100, 800, 600) -> client rect: (108, 130, 784, 562)
    // 1. Forward drag across mixed spaces: ClientNormalized to ClientPixel
    // Start: ClientNormalized { x: 0.15, y: 0.25 }
    //   cx = round(784 * 0.15) = 118, cy = round(562 * 0.25) = 141
    //   sx = 108 + 118 = 226, sy = 130 + 141 = 271
    // End: ClientPixel { x: 650, y: 450 }
    //   cx = 650, cy = 450
    //   sx = 108 + 650 = 758, sy = 130 + 450 = 580
    let action_forward = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientNormalized { x: 0.15, y: 0.25 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 650, y: 450 },
        ),
        button: Some(MouseButton::Right),
        steps: 15,
        delivery_mode: None,
    };

    let res1 = router
        .execute_step_with_escalation("sess_drag_distinct", None, 0, 1, 0, action_forward, 1, true)
        .await;
    assert!(
        res1.is_ok(),
        "Forward drag step must succeed: {:?}",
        res1.err()
    );

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
            button,
            steps,
            ..
        } => {
            assert_eq!(*target_hwnd, 0x1234);
            assert_eq!(*start_x, 226);
            assert_eq!(*start_y, 271);
            assert_eq!(*end_x, 758);
            assert_eq!(*end_y, 580);
            assert_ne!(
                *end_x,
                start_x + 50,
                "End x coordinate must NOT be start_x + 50"
            );
            assert_ne!(*end_y, *start_y, "End y coordinate must NOT be start_y");
            assert_eq!(*button, MouseButton::Right);
            assert_eq!(*steps, 15);
        }
        other => panic!("Expected CuaAction::Drag, got: {:?}", other),
    }

    // 2. Reverse drag across mixed spaces: RoiNormalized to ClientNormalized
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);
    // Start: RoiNormalized in ROI (200, 200, 300, 200) at (0.8, 0.7)
    //   sx = 200 + round(300 * 0.8) = 200 + 240 = 440
    //   sy = 200 + round(200 * 0.7) = 200 + 140 = 340
    // End: ClientNormalized { x: 0.05, y: 0.08 }
    //   cx = round(784 * 0.05) = 39, cy = round(562 * 0.08) = 45
    //   sx = 108 + 39 = 147, sy = 130 + 45 = 175
    let action_reverse = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::RoiNormalized {
                roi: CuaRect::new(200, 200, 300, 200),
                norm_x: 0.8,
                norm_y: 0.7,
            },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientNormalized { x: 0.05, y: 0.08 },
        ),
        button: Some(MouseButton::Left),
        steps: 8,
        delivery_mode: None,
    };

    let res2 = router
        .execute_step_with_escalation("sess_drag_rev", None, 0, 1, 0, action_reverse, 1, true)
        .await;
    assert!(
        res2.is_ok(),
        "Reverse drag step must succeed: {:?}",
        res2.err()
    );

    let dispatched2 = driver.get_dispatched_actions();
    assert_eq!(
        dispatched2.len(),
        2,
        "Second drag action should be recorded"
    );
    match &dispatched2[1] {
        CuaAction::Drag {
            target_hwnd,
            start_x,
            start_y,
            end_x,
            end_y,
            button,
            steps,
            ..
        } => {
            assert_eq!(*target_hwnd, 0x1234);
            assert_eq!(*start_x, 440);
            assert_eq!(*start_y, 340);
            assert_eq!(*end_x, 147);
            assert_eq!(*end_y, 175);
            assert_ne!(*end_x, start_x + 50);
            assert_eq!(*button, MouseButton::Left);
            assert_eq!(*steps, 8);
        }
        other => panic!("Expected CuaAction::Drag, got: {:?}", other),
    }
}

#[tokio::test]
async fn test_adversarial_drag_cross_window_dual_target_governor_enforcement() {
    let (router, driver, capturer, _) = create_test_router(
        PermissionMode::Bounded,
        vec!["notepad.exe".into(), "code.exe".into()],
    );

    // Insert Window B (code.exe)
    let window_b = CuaWindowInfo {
        hwnd: 0x5678,
        pid: 2000,
        title: "Visual Studio Code".into(),
        class_name: "CodeWindowClass".into(),
        process_name: Some("code.exe".into()),
        bounds: CuaRect::new(1000, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_b);

    // Insert Window C (cmd.exe - denylisted shell)
    let window_c = CuaWindowInfo {
        hwnd: 0x6666,
        pid: 3000,
        title: "Command Prompt".into(),
        class_name: "ConsoleWindowClass".into(),
        process_name: Some("cmd.exe".into()),
        bounds: CuaRect::new(1000, 700, 800, 400),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(window_c);

    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base);
    capturer.enqueue_frame(patch);

    // Case 1: Cross-window drag where BOTH windows are allowlisted (notepad.exe -> code.exe)
    let action_cross_allowed = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5678),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        ),
        button: Some(MouseButton::Left),
        steps: 12,
        delivery_mode: None,
    };

    let res_allowed = router
        .execute_step_with_escalation("sess_cross", None, 0, 1, 0, action_cross_allowed, 1, true)
        .await;

    // EMPIRICAL CHALLENGE FINDING (Finding C1-1):
    // TargetResolver successfully resolves both Window A (0x1234) and Window B (0x5678).
    // PreCheckPipeline::validate_target_geometry validates both windows individually.
    // However, PreCheckPipeline::check_security_governor passes cua_action (with end_x=1108 in Window B)
    // to governor.evaluate_action(&cua_action, &target.window_info).
    // In security.rs lines 411-422, evaluate_coordinate_containment checks both start_x/y AND end_x/y
    // against target (Window A). Because (1108, 230) is outside Window A (100..900, 100..700),
    // it fails with CuaError::CoordinateOutOfBounds, rejecting cross-window Drag as OutOfBounds!
    let err_cross = res_allowed.expect_err("Cross-window drag is currently rejected by SecurityGovernor because end point is outside source window bounds");
    assert_eq!(err_cross.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err_cross.error_code, "coordinate_out_of_bounds");
    assert!(err_cross
        .error_message
        .contains("Point (1108, 230) is out of bounds"));

    // Case 2: Destination window B is removed from allowlist
    // Note: Due to Finding C1-1, evaluate_coordinate_containment on Window A fails first with OutOfBounds,
    // shadowing the destination allowlist check.
    router
        .security_governor()
        .set_allowlist(vec!["notepad.exe".into()]);
    let action_dest_disallowed = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5678),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_dest_disallowed = router
        .execute_step_with_escalation(
            "sess_dest_denied",
            None,
            0,
            1,
            0,
            action_dest_disallowed,
            1,
            true,
        )
        .await
        .expect_err("Drag to non-allowlisted destination window must fail pre-check");
    // Empirically fails with OutOfBounds because Window A coordinate containment check shadows secondary governor check
    assert_eq!(
        err_dest_disallowed.failure_category,
        DefectCategory::OutOfBounds
    );
    assert_eq!(err_dest_disallowed.error_code, "coordinate_out_of_bounds");

    // Case 3: Destination window C is explicitly denylisted shell process (cmd.exe)
    router
        .security_governor()
        .set_allowlist(vec!["notepad.exe".into(), "cmd.exe".into()]);
    let action_dest_denylisted = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x6666),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_dest_denylisted = router
        .execute_step_with_escalation(
            "sess_dest_denylist",
            None,
            0,
            1,
            0,
            action_dest_denylisted,
            1,
            true,
        )
        .await
        .expect_err("Drag to denylisted destination process must fail pre-check");
    assert_eq!(
        err_dest_denylisted.failure_category,
        DefectCategory::OutOfBounds
    );
    assert_eq!(err_dest_denylisted.error_code, "coordinate_out_of_bounds");

    // Case 4: Source window is not allowlisted
    // Because evaluate_process_allowlist runs before evaluate_coordinate_containment,
    // non-allowlisted source window is cleanly rejected with SecurityRefused!
    router
        .security_governor()
        .set_allowlist(vec!["code.exe".into()]);
    let action_src_disallowed = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x5678),
            TargetCoordinate::ClientPixel { x: 100, y: 100 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_src_disallowed = router
        .execute_step_with_escalation(
            "sess_src_denied",
            None,
            0,
            1,
            0,
            action_src_disallowed,
            1,
            true,
        )
        .await
        .expect_err("Drag from non-allowlisted source window must fail pre-check");
    assert_eq!(
        err_src_disallowed.failure_category,
        DefectCategory::SecurityRefused
    );
    assert_eq!(err_src_disallowed.error_code, "security_policy_refused");
}

#[tokio::test]
async fn test_adversarial_drag_dual_target_occlusion_and_containment_refusals() {
    let (router, driver, _capturer, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    // Insert modal dialog overlapping center of window: (300, 300, 300, 300) with z_index: 0
    let modal = CuaWindowInfo {
        hwnd: 0x9999,
        pid: 1000,
        title: "Security Alert Modal".into(),
        class_name: "ModalDialog".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(300, 300, 300, 300),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(modal);

    // Target window 0x1234 is at z_index: 1 (behind modal)
    let base_window = CuaWindowInfo {
        hwnd: 0x1234,
        pid: 1000,
        title: "Test Application".into(),
        class_name: "TestClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 1,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };
    driver.insert_window(base_window);

    // 1. Drag where END point is occluded by modal
    // Start at client (50, 50) -> screen (158, 180) (uncovered)
    // End at client (250, 250) -> screen (358, 380) (inside modal bounds 300..600, 300..600)
    let action_end_occluded = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 250, y: 250 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_end_occ = router
        .execute_step_with_escalation("sess_occ_end", None, 0, 1, 0, action_end_occluded, 1, true)
        .await
        .expect_err("Drag with occluded end target must fail pre-check");
    assert_eq!(err_end_occ.failure_category, DefectCategory::TargetOccluded);
    assert_eq!(err_end_occ.error_code, "target_occluded");

    // 2. Drag where START point is occluded by modal
    let action_start_occluded = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 250, y: 250 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_start_occ = router
        .execute_step_with_escalation(
            "sess_occ_start",
            None,
            0,
            1,
            0,
            action_start_occluded,
            1,
            true,
        )
        .await
        .expect_err("Drag with occluded start target must fail pre-check");
    assert_eq!(
        err_start_occ.failure_category,
        DefectCategory::TargetOccluded
    );
    assert_eq!(err_start_occ.error_code, "target_occluded");

    // 3. Drag where END point hits Title Bar Caption
    let action_end_titlebar = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: -15 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_title = router
        .execute_step_with_escalation("sess_title", None, 0, 1, 0, action_end_titlebar, 1, true)
        .await
        .expect_err("Drag ending in title bar must fail pre-check");
    assert_eq!(err_title.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err_title.error_code, "coordinate_out_of_bounds");

    // 4. Drag where END point hits Close Button protected zone
    let action_end_close = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 760, y: 5 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_close = router
        .execute_step_with_escalation("sess_close", None, 0, 1, 0, action_end_close, 1, true)
        .await
        .expect_err("Drag ending in close button zone must fail pre-check");
    assert_eq!(err_close.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err_close.error_code, "coordinate_out_of_bounds");

    // 5. Drag where END point exceeds right client edge: cx = 784 (valid range: 0..=783)
    let action_end_x_oob = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 784, y: 200 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_x_oob = router
        .execute_step_with_escalation("sess_x_oob", None, 0, 1, 0, action_end_x_oob, 1, true)
        .await
        .expect_err("Drag ending at cx = width must fail pre-check");
    assert_eq!(err_x_oob.failure_category, DefectCategory::OutOfBounds);

    // 6. Drag where END point exceeds bottom client edge: cy = 562 (valid range: 0..=561)
    let action_end_y_oob = System1Action::Drag {
        start: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 50, y: 50 },
        ),
        end: TargetDescriptor::new(
            WindowSelector::Hwnd(0x1234),
            TargetCoordinate::ClientPixel { x: 200, y: 562 },
        ),
        button: Some(MouseButton::Left),
        steps: 10,
        delivery_mode: None,
    };

    let err_y_oob = router
        .execute_step_with_escalation("sess_y_oob", None, 0, 1, 0, action_end_y_oob, 1, true)
        .await
        .expect_err("Drag ending at cy = height must fail pre-check");
    assert_eq!(err_y_oob.failure_category, DefectCategory::OutOfBounds);

    // Verify invariant: Zero actions reached the driver during any failed pre-check
    assert!(
        driver.get_dispatched_actions().is_empty(),
        "All pre-check failures must fail-closed with zero dispatched actions"
    );
}

#[tokio::test]
async fn test_adversarial_normalized_boundary_1_0_across_all_coordinate_spaces() {
    let (router, _driver, capturer, _) =
        create_test_router(PermissionMode::Bounded, vec!["notepad.exe".into()]);

    let window_bounds = CuaRect::new(100, 100, 800, 600);
    let client_bounds = CuaRect::new(108, 130, 784, 562);

    // 1. ClientNormalized at (1.0, 1.0) and alias Normalized at (1.0, 1.0)
    let coord_cn = TargetCoordinate::ClientNormalized { x: 1.0, y: 1.0 };
    let (s_cn, c_cn) =
        TargetResolver::translate_coordinates(&coord_cn, &window_bounds, &client_bounds, None)
            .expect("ClientNormalized (1.0, 1.0) translation must succeed");
    assert_eq!(c_cn.x, 783, "Client x must resolve to width - 1");
    assert_eq!(c_cn.y, 561, "Client y must resolve to height - 1");
    assert_eq!(s_cn.x, 108 + 783);
    assert_eq!(s_cn.y, 130 + 561);

    let coord_norm = TargetCoordinate::Normalized { x: 1.0, y: 1.0 };
    let (s_norm, c_norm) =
        TargetResolver::translate_coordinates(&coord_norm, &window_bounds, &client_bounds, None)
            .expect("Normalized alias (1.0, 1.0) translation must succeed");
    assert_eq!(c_norm.x, 783);
    assert_eq!(c_norm.y, 561);
    assert_eq!(s_norm.x, s_cn.x);
    assert_eq!(s_norm.y, s_cn.y);

    // Verify it passes full pre-check and step execution cleanly
    let base = capturer.create_uniform_frame(128);
    let patch = capturer.create_frame_with_rect(128, 200, 150, 150, 50, 50);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());

    let action_cn = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_cn),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let res_cn = router
        .execute_step_with_escalation("sess_cn_1", None, 0, 1, 0, action_cn, 1, true)
        .await;
    assert!(
        res_cn.is_ok(),
        "ClientNormalized (1.0, 1.0) must pass pre-check and succeed: {:?}",
        res_cn.err()
    );

    // 2. RoiNormalized at (1.0, 1.0)
    // 2a. Sub-ROI inside client bounds: (200, 200, 150, 100)
    let sub_roi = CuaRect::new(200, 200, 150, 100);
    let coord_roi = TargetCoordinate::RoiNormalized {
        roi: sub_roi,
        norm_x: 1.0,
        norm_y: 1.0,
    };
    let (s_roi, c_roi) =
        TargetResolver::translate_coordinates(&coord_roi, &window_bounds, &client_bounds, None)
            .expect("RoiNormalized (1.0, 1.0) translation must succeed");
    assert_eq!(s_roi.x, 200 + 150 - 1, "sx must be roi.x + roi.width - 1");
    assert_eq!(s_roi.y, 200 + 100 - 1, "sy must be roi.y + roi.height - 1");
    assert_eq!(c_roi.x, s_roi.x - 108);
    assert_eq!(c_roi.y, s_roi.y - 130);
    assert!(c_roi.x < client_bounds.width && c_roi.y < client_bounds.height);

    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    let action_roi = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_roi),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let res_roi = router
        .execute_step_with_escalation("sess_roi_1", None, 0, 1, 0, action_roi, 1, true)
        .await;
    assert!(
        res_roi.is_ok(),
        "RoiNormalized (1.0, 1.0) inside client area must pass pre-check cleanly"
    );

    // 2b. Full-Client ROI: roi == client_bounds
    let coord_roi_full = TargetCoordinate::RoiNormalized {
        roi: client_bounds,
        norm_x: 1.0,
        norm_y: 1.0,
    };
    let (s_rf, c_rf) = TargetResolver::translate_coordinates(
        &coord_roi_full,
        &window_bounds,
        &client_bounds,
        None,
    )
    .expect("Full-client RoiNormalized translation must succeed");
    assert_eq!(c_rf.x, 783);
    assert_eq!(c_rf.y, 561);
    assert_eq!(s_rf.x, 108 + 783);
    assert_eq!(s_rf.y, 130 + 561);

    // 3. WindowNormalized at (1.0, 1.0)
    // 3a. Borderless window (client_bounds == window_bounds)
    let borderless_bounds = CuaRect::new(100, 100, 800, 600);
    let (s_wb, c_wb) = TargetResolver::translate_coordinates(
        &TargetCoordinate::WindowNormalized { x: 1.0, y: 1.0 },
        &borderless_bounds,
        &borderless_bounds,
        None,
    )
    .expect("WindowNormalized on borderless window must succeed");
    assert_eq!(c_wb.x, 799, "cx on borderless window must be width - 1");
    assert_eq!(c_wb.y, 599, "cy on borderless window must be height - 1");
    assert_eq!(s_wb.x, 100 + 799);
    assert_eq!(s_wb.y, 100 + 599);

    // 3b. Standard window with borders: window outer corner (899, 699) -> cx = 791 >= 784
    let coord_wn = TargetCoordinate::WindowNormalized { x: 1.0, y: 1.0 };
    let action_wn = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_wn),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_wn = router
        .execute_step_with_escalation("sess_wn_framed", None, 0, 1, 0, action_wn, 1, true)
        .await
        .expect_err("WindowNormalized (1.0, 1.0) on framed window must be rejected by pre-check");
    assert_eq!(err_wn.failure_category, DefectCategory::OutOfBounds);
    assert_eq!(err_wn.error_code, "coordinate_out_of_bounds");

    // 4. ScreenNormalized at (1.0, 1.0)
    let coord_sn = TargetCoordinate::ScreenNormalized { x: 1.0, y: 1.0 };
    let action_sn = System1Action::Click {
        target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_sn),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: None,
    };
    let err_sn = router
        .execute_step_with_escalation("sess_sn", None, 0, 1, 0, action_sn, 1, true)
        .await
        .expect_err("ScreenNormalized (1.0, 1.0) on non-fullscreen window must be rejected");
    assert_eq!(err_sn.failure_category, DefectCategory::OutOfBounds);

    // 5. Corner Points Matrix:
    // 5a. Top-Left ClientNormalized (0.0, 0.0) -> cx = 0, cy = 0 (passes cleanly)
    let coord_tl = TargetCoordinate::ClientNormalized { x: 0.0, y: 0.0 };
    let (s_tl, c_tl) =
        TargetResolver::translate_coordinates(&coord_tl, &window_bounds, &client_bounds, None)
            .unwrap();
    assert_eq!(c_tl.x, 0);
    assert_eq!(c_tl.y, 0);
    assert_eq!(s_tl.x, 108);
    assert_eq!(s_tl.y, 130);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    let res_tl = router
        .execute_step_with_escalation(
            "sess_tl",
            None,
            0,
            1,
            0,
            System1Action::Click {
                target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_tl),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            1,
            true,
        )
        .await;
    assert!(
        res_tl.is_ok(),
        "Top-left ClientNormalized (0.0, 0.0) must pass pre-check cleanly"
    );

    // 5b. Bottom-Left ClientNormalized (0.0, 1.0) -> cx = 0, cy = 561 (passes cleanly)
    let coord_bl = TargetCoordinate::ClientNormalized { x: 0.0, y: 1.0 };
    let (_, c_bl) =
        TargetResolver::translate_coordinates(&coord_bl, &window_bounds, &client_bounds, None)
            .unwrap();
    assert_eq!(c_bl.x, 0);
    assert_eq!(c_bl.y, 561);
    capturer.enqueue_frame(base.clone());
    capturer.enqueue_frame(patch.clone());
    let res_bl = router
        .execute_step_with_escalation(
            "sess_bl",
            None,
            0,
            1,
            0,
            System1Action::Click {
                target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_bl),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            1,
            true,
        )
        .await;
    assert!(
        res_bl.is_ok(),
        "Bottom-left ClientNormalized (0.0, 1.0) must pass pre-check cleanly"
    );

    // 5c. Top-Left WindowNormalized (0.0, 0.0) -> cx = -8, cy = -30 (rejected as title bar)
    let coord_wn_tl = TargetCoordinate::WindowNormalized { x: 0.0, y: 0.0 };
    let err_wn_tl = router
        .execute_step_with_escalation(
            "sess_wn_tl",
            None,
            0,
            1,
            0,
            System1Action::Click {
                target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_wn_tl),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            1,
            true,
        )
        .await
        .expect_err("Top-left WindowNormalized (0.0, 0.0) must be rejected");
    assert_eq!(err_wn_tl.failure_category, DefectCategory::OutOfBounds);

    // 5d. Top-Right WindowNormalized (1.0, 0.0) -> rejected as close button
    let coord_wn_tr = TargetCoordinate::WindowNormalized { x: 1.0, y: 0.0 };
    let err_wn_tr = router
        .execute_step_with_escalation(
            "sess_wn_tr",
            None,
            0,
            1,
            0,
            System1Action::Click {
                target: TargetDescriptor::new(WindowSelector::Hwnd(0x1234), coord_wn_tr),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: None,
            },
            1,
            true,
        )
        .await
        .expect_err("Top-right WindowNormalized (1.0, 0.0) must be rejected");
    assert_eq!(err_wn_tr.failure_category, DefectCategory::OutOfBounds);

    // 6. Out-of-range clamping:
    // ClientNormalized (1.5, 1.5) clamps to (width - 1, height - 1)
    let coord_clamp_hi = TargetCoordinate::ClientNormalized { x: 1.5, y: 1.5 };
    let (_, c_hi) = TargetResolver::translate_coordinates(
        &coord_clamp_hi,
        &window_bounds,
        &client_bounds,
        None,
    )
    .unwrap();
    assert_eq!(c_hi.x, 783);
    assert_eq!(c_hi.y, 561);

    // ClientNormalized (-0.5, -0.5) clamps to (0, 0)
    let coord_clamp_lo = TargetCoordinate::ClientNormalized { x: -0.5, y: -0.5 };
    let (_, c_lo) = TargetResolver::translate_coordinates(
        &coord_clamp_lo,
        &window_bounds,
        &client_bounds,
        None,
    )
    .unwrap();
    assert_eq!(c_lo.x, 0);
    assert_eq!(c_lo.y, 0);
}
