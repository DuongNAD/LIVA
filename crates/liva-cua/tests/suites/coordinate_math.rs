//! Comprehensive tests for coordinate normalization, signed LPARAM packing, and DPI scaling.

use liva_cua::geometry::*;

#[test]
fn test_virtualdesk_normalization_single_monitor() {
    let (vx, vy, vw, vh) = (0, 0, 1920, 1080);
    // Origin maps to (0, 0)
    let (dx, dy) = to_virtualdesk_absolute(0, 0, vx, vy, vw, vh);
    assert_eq!((dx, dy), (0, 0));

    // Bottom-right corner maps to (65535, 65535)
    let (dx, dy) = to_virtualdesk_absolute(1919, 1079, vx, vy, vw, vh);
    assert_eq!((dx, dy), (65535, 65535));

    // Center maps to approx (32767, 32767)
    let (dx, dy) = to_virtualdesk_absolute(960, 540, vx, vy, vw, vh);
    assert!((dx - 32767).abs() <= 50);
    assert!((dy - 32767).abs() <= 50);
}

#[test]
fn test_virtualdesk_multi_monitor_negative_origin() {
    // Secondary monitor to the left: -1920..0, primary monitor: 0..1920
    let (vx, vy, vw, vh) = (-1920, 0, 3840, 1080);

    // Far left of secondary monitor maps to 0
    let (dx, _) = to_virtualdesk_absolute(-1920, 500, vx, vy, vw, vh);
    assert_eq!(dx, 0);

    // Primary monitor origin (0, 0) starts at the seam right half (dx >= 32768)
    let (dx_seam_left, _) = to_virtualdesk_absolute(-1, 500, vx, vy, vw, vh);
    let (dx_seam_right, _) = to_virtualdesk_absolute(0, 500, vx, vy, vw, vh);
    assert!(
        dx_seam_left < 32768,
        "Seam left -1 mapped to dx={}",
        dx_seam_left
    );
    assert!(
        dx_seam_right >= 32768,
        "Seam right 0 mapped to dx={}",
        dx_seam_right
    );
    assert!(
        dx_seam_right > dx_seam_left,
        "Monotonic progression across monitor seam"
    );

    // Far right of primary monitor maps to 65535
    let (dx, _) = to_virtualdesk_absolute(1919, 500, vx, vy, vw, vh);
    assert_eq!(dx, 65535);

    // Points on secondary display stay strictly in the left half (< 32768)
    for sx in [-1920, -1500, -1000, -500, -1] {
        let (dx, _) = to_virtualdesk_absolute(sx, 500, vx, vy, vw, vh);
        assert!(dx < 32768, "sx={} produced dx={}", sx, dx);
    }
}

#[test]
fn test_virtualdesk_round_trip_accuracy() {
    let layouts = [
        ("single-1080p", 0, 0, 1920, 1080),
        ("dual-left-1080p", -1920, 0, 3840, 1080),
        ("dual-above-1440p", 0, -1440, 2560, 2880),
        ("triple-mixed", -1920, -1080, 5760, 2160),
    ];

    for &(label, vx, vy, vw, vh) in &layouts {
        let test_points = [
            (vx, vy),
            (vx + vw / 4, vy + vh / 4),
            (vx + vw / 2, vy + vh / 2),
            (vx + vw - 1, vy + vh - 1),
        ];

        for &(sx, sy) in &test_points {
            let (dx, dy) = to_virtualdesk_absolute(sx, sy, vx, vy, vw, vh);
            let (rx, ry) = from_virtualdesk_absolute(dx, dy, vx, vy, vw, vh);
            assert!(
                (rx - sx).abs() <= 1,
                "{}: X drift |rx - sx| = {} (sx={}, rx={})",
                label,
                (rx - sx).abs(),
                sx,
                rx
            );
            assert!(
                (ry - sy).abs() <= 1,
                "{}: Y drift |ry - sy| = {} (sy={}, ry={})",
                label,
                (ry - sy).abs(),
                sy,
                ry
            );
        }
    }
}

#[test]
fn test_lparam_signed_packing_and_unpacking() {
    let test_coords = [
        (0, 0),
        (100, 200),
        (-1, -1),
        (-500, 300),
        (1920, -1080),
        (i16::MAX as i32, i16::MIN as i32),
        (i16::MIN as i32, i16::MAX as i32),
    ];

    for &(x, y) in &test_coords {
        let lp = pack_lparam(x, y).expect("coordinate fits in i16");
        let (ux, uy) = unpack_xy(lp);
        assert_eq!((ux, uy), (x, y), "Mismatch for coordinate ({}, {})", x, y);
    }
}

#[test]
fn test_lparam_out_of_range_rejection() {
    assert!(matches!(
        pack_lparam(32768, 0),
        Err(LParamPackError::XOutOfRange(32768))
    ));
    assert!(matches!(
        pack_lparam(0, -32769),
        Err(LParamPackError::YOutOfRange(-32769))
    ));
}

#[test]
fn test_lparam_clamping() {
    let lp = pack_lparam_clamped(50000, -60000);
    let (ux, uy) = unpack_xy(lp);
    assert_eq!(ux, i16::MAX as i32);
    assert_eq!(uy, i16::MIN as i32);
}

#[test]
fn test_dpi_scaling_conversions() {
    assert_eq!(dpi_scale_factor(96), 1.0);
    assert_eq!(dpi_scale_factor(144), 1.5);
    assert_eq!(dpi_scale_factor(192), 2.0);

    // 100 logical points at 150% DPI = 150 physical pixels
    let (px, py) = logical_to_physical_pt(100, 200, 144);
    assert_eq!((px, py), (150, 300));

    // Convert back from physical to logical
    let (lx, ly) = physical_to_logical_pt(150, 300, 144);
    assert_eq!((lx, ly), (100, 200));

    // Rescale across different DPI monitors (e.g. 96 to 144)
    let rescaled = rescale_dimension(100, 96, 144);
    assert_eq!(rescaled, 150);
}
