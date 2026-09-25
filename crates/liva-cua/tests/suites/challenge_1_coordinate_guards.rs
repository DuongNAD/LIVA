//! Empirical Challenger 1: Adversarial tests for coordinate geometry and state guards.
//!
//! Scope:
//! 1. Multi-monitor virtual desk mapping under extreme negative origins and large bounding boxes.
//! 2. Boundary mapping: virtual screen corners map to (0, 0) and (65535, 65535), out-of-bounds clamping, degenerate 0/1 metrics.
//! 3. Signed 16-bit LPARAM packing edge cases (-1, -32768, 32767) and Win32 sign-extension invariants.
//! 4. Minimized window sentinel coordinates (-32000, -32000) and zero/negative dimension guards.

use liva_cua::geometry::*;
use liva_cua::guards::MinimizedWindowGuard;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::Arc;

// ============================================================================
// PART 1: Extreme Multi-Monitor Virtual Desktop Normalization
// ============================================================================

#[test]
fn test_adversarial_virtualdesk_extreme_horizontal_3x() {
    // 3 monitors horizontal (e.g. 3x 4K): origin x = -3840, y = -1080, w = 11520, h = 3240
    let (vx, vy, vw, vh) = (-3840, -1080, 11520, 3240);

    // 1. Exact top-left corner must map to (0, 0)
    let (tl_dx, tl_dy) = to_virtualdesk_absolute(vx, vy, vx, vy, vw, vh);
    assert_eq!(
        (tl_dx, tl_dy),
        (0, 0),
        "Extreme 3x horizontal top-left corner must map to (0, 0)"
    );

    // 2. Exact bottom-right corner must map to (65535, 65535)
    let br_sx = vx + vw - 1;
    let br_sy = vy + vh - 1;
    let (br_dx, br_dy) = to_virtualdesk_absolute(br_sx, br_sy, vx, vy, vw, vh);
    assert_eq!(
        (br_dx, br_dy),
        (65535, 65535),
        "Extreme 3x horizontal bottom-right corner must map to (65535, 65535)"
    );

    // 3. Monotonicity across the entire span (-3840 to 7679)
    let mut prev_dx = -1;
    for sx in (-3840..=7679).step_by(128) {
        let (dx, _) = to_virtualdesk_absolute(sx, 0, vx, vy, vw, vh);
        assert!(
            dx >= prev_dx,
            "Monotonicity violated: sx={} gave dx={}, prev_dx={}",
            sx,
            dx,
            prev_dx
        );
        assert!(
            (0..=65535).contains(&dx),
            "dx out of range [0, 65535]: {}",
            dx
        );
        prev_dx = dx;
    }

    // 4. Seam behavior across negative to positive transitions
    // -1 on secondary monitor vs 0 on primary monitor
    let (dx_minus1, _) = to_virtualdesk_absolute(-1, 0, vx, vy, vw, vh);
    let (dx_zero, _) = to_virtualdesk_absolute(0, 0, vx, vy, vw, vh);
    assert!(
        dx_zero > dx_minus1,
        "dx(0) must strictly exceed dx(-1) at seam"
    );
    // Negative pixels (-3840..-1) occupy the first 3840/11520 = 1/3 of virtual desk (approx 0..21845)
    assert!(
        dx_minus1 < 21850,
        "dx(-1) was unexpectedly large: {}",
        dx_minus1
    );
    assert!(
        dx_zero >= 21840,
        "dx(0) was unexpectedly small: {}",
        dx_zero
    );

    // 5. Round trip recovery bounded drift
    for sx in (-3840..=7679).step_by(256) {
        for sy in (-1080..=2159).step_by(256) {
            let (dx, dy) = to_virtualdesk_absolute(sx, sy, vx, vy, vw, vh);
            let (rx, ry) = from_virtualdesk_absolute(dx, dy, vx, vy, vw, vh);
            // With span 11520 and 65535 ticks, step is ~0.176 px/tick, round-trip drift is <= 1 px
            assert!(
                (rx - sx).abs() <= 1,
                "Round trip X drift exceeded: sx={}, rx={}",
                sx,
                rx
            );
            assert!(
                (ry - sy).abs() <= 1,
                "Round trip Y drift exceeded: sy={}, ry={}",
                sy,
                ry
            );
        }
    }
}

#[test]
fn test_adversarial_virtualdesk_extreme_vertical_3x() {
    // 3 monitors vertical: x = 0, y = -4320, w = 3840, h = 6480
    let (vx, vy, vw, vh) = (0, -4320, 3840, 6480);

    let (tl_dx, tl_dy) = to_virtualdesk_absolute(vx, vy, vx, vy, vw, vh);
    assert_eq!((tl_dx, tl_dy), (0, 0));

    let (br_dx, br_dy) = to_virtualdesk_absolute(vx + vw - 1, vy + vh - 1, vx, vy, vw, vh);
    assert_eq!((br_dx, br_dy), (65535, 65535));

    // Vertical seam at y = -1 vs y = 0
    let (_, dy_minus1) = to_virtualdesk_absolute(100, -1, vx, vy, vw, vh);
    let (_, dy_zero) = to_virtualdesk_absolute(100, 0, vx, vy, vw, vh);
    assert!(dy_zero > dy_minus1);

    // Monotonicity along Y
    let mut prev_dy = -1;
    for sy in (-4320..=2159).step_by(128) {
        let (_, dy) = to_virtualdesk_absolute(500, sy, vx, vy, vw, vh);
        assert!(
            dy >= prev_dy,
            "Y Monotonicity violated: sy={}, dy={}",
            sy,
            dy
        );
        prev_dy = dy;
    }
}

#[test]
fn test_adversarial_virtualdesk_l_shape_configuration() {
    // L-shape dual/triple setup: x = -3840, y = -2160, w = 7680, h = 4320
    let (vx, vy, vw, vh) = (-3840, -2160, 7680, 4320);

    assert_eq!(to_virtualdesk_absolute(vx, vy, vx, vy, vw, vh), (0, 0));
    assert_eq!(
        to_virtualdesk_absolute(vx + vw - 1, vy + vh - 1, vx, vy, vw, vh),
        (65535, 65535)
    );
}

#[test]
fn test_adversarial_virtualdesk_huge_bounding_box_overflow_resistance() {
    // Huge 2,000,000 x 2,000,000 pixel extent to stress 64-bit intermediate products
    let (vx, vy, vw, vh) = (-1_000_000, -1_000_000, 2_000_000, 2_000_000);

    let (tl_dx, tl_dy) = to_virtualdesk_absolute(vx, vy, vx, vy, vw, vh);
    assert_eq!((tl_dx, tl_dy), (0, 0));

    let (br_dx, br_dy) = to_virtualdesk_absolute(vx + vw - 1, vy + vh - 1, vx, vy, vw, vh);
    assert_eq!((br_dx, br_dy), (65535, 65535));

    // Midpoint mapping
    let (mid_dx, mid_dy) = to_virtualdesk_absolute(0, 0, vx, vy, vw, vh);
    assert!((mid_dx - 32767).abs() <= 2);
    assert!((mid_dy - 32767).abs() <= 2);
}

// ============================================================================
// PART 2: Boundary Mapping, Clamping & Degenerate Divisors
// ============================================================================

#[test]
fn test_adversarial_virtualdesk_out_of_bounds_clamping() {
    let (vx, vy, vw, vh) = (-1920, -1080, 3840, 2160);

    // Far negative out-of-bounds must clamp to 0
    let (dx_neg, dy_neg) = to_virtualdesk_absolute(-10_000_000, -10_000_000, vx, vy, vw, vh);
    assert_eq!(
        (dx_neg, dy_neg),
        (0, 0),
        "Negative out-of-bounds must clamp to (0, 0)"
    );

    // Far positive out-of-bounds must clamp to 65535
    let (dx_pos, dy_pos) = to_virtualdesk_absolute(10_000_000, 10_000_000, vx, vy, vw, vh);
    assert_eq!(
        (dx_pos, dy_pos),
        (65535, 65535),
        "Positive out-of-bounds must clamp to (65535, 65535)"
    );

    // Extreme i32 boundaries
    let (dx_min, dy_min) = to_virtualdesk_absolute(i32::MIN, i32::MIN, vx, vy, vw, vh);
    assert_eq!(
        (dx_min, dy_min),
        (0, 0),
        "i32::MIN must clamp to 0 without overflow"
    );

    let (dx_max, dy_max) = to_virtualdesk_absolute(i32::MAX, i32::MAX, vx, vy, vw, vh);
    assert_eq!(
        (dx_max, dy_max),
        (65535, 65535),
        "i32::MAX must clamp to 65535 without overflow"
    );

    // Inverse out-of-bounds clamping
    let (sx_low, sy_low) = from_virtualdesk_absolute(-5000, -5000, vx, vy, vw, vh);
    assert_eq!(
        (sx_low, sy_low),
        (vx, vy),
        "Negative dx/dy must clamp to origin"
    );

    let (sx_high, sy_high) = from_virtualdesk_absolute(100_000, 100_000, vx, vy, vw, vh);
    assert_eq!(
        (sx_high, sy_high),
        (vx + vw - 1, vy + vh - 1),
        "Excessive dx/dy must clamp to bottom-right"
    );
}

#[test]
fn test_adversarial_virtualdesk_degenerate_dimensions_zero_division() {
    // 1. Both dimensions zero
    let (dx, dy) = to_virtualdesk_absolute(50, 50, 0, 0, 0, 0);
    assert!((0..=65535).contains(&dx));
    assert!((0..=65535).contains(&dy));

    let (rx, ry) = from_virtualdesk_absolute(32768, 32768, 0, 0, 0, 0);
    assert_eq!((rx, ry), (0, 0));

    // 2. Both dimensions 1
    let (dx1, dy1) = to_virtualdesk_absolute(10, 20, 10, 20, 1, 1);
    assert_eq!((dx1, dy1), (0, 0));
    let (rx1, ry1) = from_virtualdesk_absolute(65535, 65535, 10, 20, 1, 1);
    assert_eq!((rx1, ry1), (10, 20));

    // 3. Asymmetric degenerate (width 0, height normal)
    let (dx_z_w, dy_n_h) = to_virtualdesk_absolute(0, 540, 0, 0, 0, 1080);
    assert!((0..=65535).contains(&dx_z_w));
    assert!((0..=65535).contains(&dy_n_h));

    // 4. Asymmetric degenerate (width normal, height 0)
    let (dx_n_w, dy_z_h) = to_virtualdesk_absolute(960, 0, 0, 0, 1920, 0);
    assert!((0..=65535).contains(&dx_n_w));
    assert!((0..=65535).contains(&dy_z_h));

    // 5. Negative dimensions (degenerate hardware query results)
    let (dx_neg_dim, dy_neg_dim) = to_virtualdesk_absolute(100, 100, 0, 0, -100, -100);
    assert!((0..=65535).contains(&dx_neg_dim));
    assert!((0..=65535).contains(&dy_neg_dim));
}

// ============================================================================
// PART 3: Signed 16-bit LPARAM Packing Sign Invariants
// ============================================================================

#[test]
fn test_adversarial_lparam_exact_edge_cases() {
    let edge_cases = [
        (-1, -1),
        (i16::MIN as i32, i16::MIN as i32), // -32768, -32768
        (i16::MAX as i32, i16::MAX as i32), // 32767, 32767
        (-1, i16::MAX as i32),              // -1, 32767
        (i16::MAX as i32, -1),              // 32767, -1
        (i16::MIN as i32, i16::MAX as i32), // -32768, 32767
        (i16::MAX as i32, i16::MIN as i32), // 32767, -32768
        (0, -1),
        (-1, 0),
        (0, i16::MIN as i32),
        (i16::MIN as i32, 0),
    ];

    for &(x, y) in &edge_cases {
        let lp =
            pack_lparam(x, y).unwrap_or_else(|e| panic!("Failed to pack ({}, {}): {:?}", x, y, e));
        let (rx, ry) = unpack_xy(lp);
        assert_eq!(
            (rx, ry),
            (x, y),
            "Unpack mismatch for exact edge case ({}, {})",
            x,
            y
        );

        // Win32 sign-extension macro check:
        // C macro: (int)(short)LOWORD(lp) == (lparam as usize & 0xFFFF) as u16 as i16 as i32
        let c_loword = ((lp as usize) & 0xFFFF) as u16 as i16 as i32;
        let c_hiword = (((lp as usize) >> 16) & 0xFFFF) as u16 as i16 as i32;
        assert_eq!(c_loword, x, "C macro GET_X_LPARAM mismatch for x={}", x);
        assert_eq!(c_hiword, y, "C macro GET_Y_LPARAM mismatch for y={}", y);
    }
}

#[test]
fn test_adversarial_lparam_bit_patterns_sign_extension() {
    // Specifically test (-1, -1):
    // -1 as i16 is 0xFFFF. Low = 0xFFFF, High = 0xFFFF.
    // Full packed u32 is 0xFFFFFFFF.
    let lp = pack_lparam(-1, -1).expect("Pack (-1, -1)");
    assert_eq!(unpack_x(lp), -1);
    assert_eq!(unpack_y(lp), -1);

    // Verify low word and high word bitwise
    let packed_u32 = pack_xy(-1, -1).expect("pack_xy");
    assert_eq!(packed_u32, 0xFFFFFFFF);
    assert_eq!(packed_u32 & 0xFFFF, 0xFFFF);
    assert_eq!((packed_u32 >> 16) & 0xFFFF, 0xFFFF);

    // Specifically test (-32768, 32767):
    // -32768 is 0x8000. 32767 is 0x7FFF.
    let lp_bounds = pack_lparam(-32768, 32767).expect("Pack (-32768, 32767)");
    let packed_bounds_u32 = pack_xy(-32768, 32767).expect("pack_xy");
    assert_eq!(packed_bounds_u32, 0x7FFF8000);
    assert_eq!(unpack_x(lp_bounds), -32768);
    assert_eq!(unpack_y(lp_bounds), 32767);
}

#[test]
fn test_adversarial_lparam_out_of_range_rejections_and_clamping() {
    // Boundaries: valid is [-32768, 32767]
    assert!(pack_lparam(32768, 0).is_err(), "32768 must be rejected");
    assert!(
        pack_lparam(0, 32768).is_err(),
        "32768 must be rejected for y"
    );
    assert!(pack_lparam(-32769, 0).is_err(), "-32769 must be rejected");
    assert!(
        pack_lparam(0, -32769).is_err(),
        "-32769 must be rejected for y"
    );
    assert!(
        pack_lparam(i32::MAX, 0).is_err(),
        "i32::MAX must be rejected"
    );
    assert!(
        pack_lparam(0, i32::MIN).is_err(),
        "i32::MIN must be rejected"
    );

    // Clamped variant tests
    let lp_clamp_max = pack_lparam_clamped(100_000, i32::MAX);
    assert_eq!(unpack_xy(lp_clamp_max), (32767, 32767));

    let lp_clamp_min = pack_lparam_clamped(-100_000, i32::MIN);
    assert_eq!(unpack_xy(lp_clamp_min), (-32768, -32768));

    let lp_clamp_mixed = pack_lparam_clamped(32768, -32769);
    assert_eq!(unpack_xy(lp_clamp_mixed), (32767, -32768));
}

// ============================================================================
// PART 4: Minimized Window Sentinel Coordinates and State Guards
// ============================================================================

#[test]
fn test_adversarial_minimized_sentinel_variations() {
    // Win32 minimized window sentinel: x = -32000, y = -32000
    // Test 1: Standard zero-sized sentinel
    let b1 = CuaRect::new(-32000, -32000, 0, 0);
    assert!(b1.is_minimized_sentinel());
    assert!(MinimizedWindowGuard::validate(0x10, false, &b1).is_err());

    // Test 2: Sentinel coordinates with non-zero Win32 minimized icon dimensions (160x28)
    let b2 = CuaRect::new(-32000, -32000, 160, 28);
    assert!(b2.is_minimized_sentinel());
    assert!(MinimizedWindowGuard::validate(0x11, false, &b2).is_err());

    // Test 3: Sentinel coordinates with arbitrary restored dimensions (800x600)
    let b3 = CuaRect::new(-32000, -32000, 800, 600);
    assert!(b3.is_minimized_sentinel());
    assert!(MinimizedWindowGuard::validate(0x12, false, &b3).is_err());

    // Test 4: IsIconic true overrides even valid coordinates
    let b_normal = CuaRect::new(100, 100, 1024, 768);
    assert!(!b_normal.is_minimized_sentinel());
    assert!(MinimizedWindowGuard::validate(0x13, true, &b_normal).is_err());

    // Test 5: Truly valid non-iconic window passes
    assert!(MinimizedWindowGuard::validate(0x14, false, &b_normal).is_ok());
}

#[test]
fn test_adversarial_degenerate_window_dimensions() {
    // Zero width
    let b_zero_w = CuaRect::new(200, 200, 0, 600);
    assert!(b_zero_w.is_empty());
    assert!(MinimizedWindowGuard::validate(0x20, false, &b_zero_w).is_err());

    // Zero height
    let b_zero_h = CuaRect::new(200, 200, 800, 0);
    assert!(b_zero_h.is_empty());
    assert!(MinimizedWindowGuard::validate(0x21, false, &b_zero_h).is_err());

    // Negative width/height
    let b_neg = CuaRect::new(200, 200, -50, -50);
    assert!(b_neg.is_empty());
    assert!(MinimizedWindowGuard::validate(0x22, false, &b_neg).is_err());
}

#[tokio::test]
async fn test_adversarial_engine_fail_closed_on_sentinel_and_degenerate() {
    let mock = Arc::new(MockCuaDriver::new());

    // Register 1: Window with iconic size at sentinel (-32000, -32000, 160, 28)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xA1,
        pid: 101,
        title: "Minimized App 1".into(),
        class_name: "AppClass".into(),
        process_name: Some("app.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 160, 28),
        is_on_screen: false,
        is_minimized: true,
        z_index: 10,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // Register 2: Window not marked iconic, but coordinates are sentinel (-32000, -32000, 800, 600)
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xA2,
        pid: 102,
        title: "Sneaky Sentinel App".into(),
        class_name: "AppClass".into(),
        process_name: Some("app2.exe".into()),
        bounds: CuaRect::new(-32000, -32000, 800, 600),
        is_on_screen: false,
        is_minimized: false, // Flag is false, but bounds are sentinel!
        z_index: 11,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    // Register 3: Degenerate width=0
    mock.insert_window(CuaWindowInfo {
        hwnd: 0xA3,
        pid: 103,
        title: "Zero Width App".into(),
        class_name: "AppClass".into(),
        process_name: Some("app3.exe".into()),
        bounds: CuaRect::new(300, 300, 0, 500),
        is_on_screen: true,
        is_minimized: false,
        z_index: 12,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let engine = CuaEngine::new_with_driver(CuaConfig::default(), mock);

    // Attempt Click on Register 1
    let act1 = CuaAction::Click {
        target_hwnd: 0xA1,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res1 = engine.execute_action(act1).await;
    assert!(res1.is_err());
    assert!(matches!(res1.unwrap_err(), CuaError::WindowMinimized(0xA1)));

    // Attempt Click on Register 2 (Sneaky sentinel)
    let act2 = CuaAction::Click {
        target_hwnd: 0xA2,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res2 = engine.execute_action(act2).await;
    assert!(
        res2.is_err(),
        "Action on sentinel coordinates must be rejected fail-closed"
    );
    match res2.unwrap_err() {
        CuaError::WindowMinimized(hwnd) => assert_eq!(hwnd, 0xA2),
        CuaError::WindowHidden(hwnd) => assert_eq!(hwnd, 0xA2),
        other => panic!("Expected WindowMinimized or WindowHidden, got {:?}", other),
    }

    // Attempt Click on Register 3 (Zero width)
    let act3 = CuaAction::Click {
        target_hwnd: 0xA3,
        x: Some(10),
        y: Some(10),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };
    let res3 = engine.execute_action(act3).await;
    assert!(
        res3.is_err(),
        "Action on degenerate bounds must be rejected fail-closed"
    );
    match res3.unwrap_err() {
        CuaError::WindowMinimized(hwnd) => assert_eq!(hwnd, 0xA3),
        CuaError::WindowHidden(hwnd) => assert_eq!(hwnd, 0xA3),
        other => panic!("Expected WindowMinimized or WindowHidden, got {:?}", other),
    }
}
