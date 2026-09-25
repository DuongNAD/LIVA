//! Pure-math coordinate normalization for `MOUSEEVENTF_VIRTUALDESK` absolute inputs.
//!
//! Windows `SendInput` with `MOUSEEVENTF_ABSOLUTE | MOUSEEVENTF_VIRTUALDESK` interprets
//! `MOUSEINPUT::dx` and `dy` as normalized coordinates in `0..=65535` spanning the union
//! of all monitors (the virtual desktop).
//!
//! In multi-monitor configurations where a secondary monitor is positioned to the left of
//! or above the primary display, `SM_XVIRTUALSCREEN` and `SM_YVIRTUALSCREEN` are negative.
//! This module provides pure, overflow-safe integer arithmetic that maps any physical
//! screen pixel `(sx, sy)` onto `0..=65535` without sign flips or float rounding drift.

/// Metrics describing the virtual desktop extent across all connected monitors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VirtualDesktopMetrics {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl VirtualDesktopMetrics {
    /// Query current system metrics from Win32 API.
    #[cfg(windows)]
    pub fn query_from_system() -> Self {
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetSystemMetrics, SM_CXSCREEN, SM_CXVIRTUALSCREEN, SM_CYSCREEN, SM_CYVIRTUALSCREEN,
            SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
        };

        unsafe {
            let mut vx = GetSystemMetrics(SM_XVIRTUALSCREEN);
            let mut vy = GetSystemMetrics(SM_YVIRTUALSCREEN);
            let mut vw = GetSystemMetrics(SM_CXVIRTUALSCREEN);
            let mut vh = GetSystemMetrics(SM_CYVIRTUALSCREEN);

            // Fallback to primary monitor if virtual desktop metrics are unpopulated
            if vw <= 0 || vh <= 0 {
                vx = 0;
                vy = 0;
                vw = GetSystemMetrics(SM_CXSCREEN).max(1);
                vh = GetSystemMetrics(SM_CYSCREEN).max(1);
            }

            Self {
                x: vx,
                y: vy,
                width: vw,
                height: vh,
            }
        }
    }

    #[cfg(not(windows))]
    pub fn query_from_system() -> Self {
        Self {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }

    /// Construct a synthetic virtual desktop metric for tests or headless environments.
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }
}

/// Convert a screen-space pixel `(sx, sy)` to normalized absolute coordinates `(dx, dy)` in `0..=65535`.
///
/// Parameters:
/// - `sx`, `sy`: Screen coordinates in virtual desktop space (may be negative).
/// - `virt_x`, `virt_y`: Origin of virtual desktop (`SM_XVIRTUALSCREEN`, `SM_YVIRTUALSCREEN`).
/// - `virt_w`, `virt_h`: Total dimensions of virtual desktop (`SM_CXVIRTUALSCREEN`, `SM_CYVIRTUALSCREEN`).
///
/// Divisor math uses `(virt_w - 1).max(1)` fence-post logic because:
/// `dx = 0` corresponds to the first pixel column `sx = virt_x`.
/// `dx = 65535` corresponds to the last pixel column `sx = virt_x + virt_w - 1`.
#[inline]
pub fn to_virtualdesk_absolute(
    sx: i32,
    sy: i32,
    virt_x: i32,
    virt_y: i32,
    virt_w: i32,
    virt_h: i32,
) -> (i32, i32) {
    let denom_x = (virt_w as i64 - 1).max(1);
    let denom_y = (virt_h as i64 - 1).max(1);

    let nx = ((sx as i64 - virt_x as i64) * 65535) / denom_x;
    let ny = ((sy as i64 - virt_y as i64) * 65535) / denom_y;

    (nx.clamp(0, 65535) as i32, ny.clamp(0, 65535) as i32)
}

/// Inverse translation: Convert normalized absolute coordinate `(dx, dy)` in `0..=65535`
/// back to physical screen space `(sx, sy)`.
///
/// Uses round-to-nearest integer arithmetic to guarantee `±1` pixel recovery accuracy.
#[inline]
pub fn from_virtualdesk_absolute(
    dx: i32,
    dy: i32,
    virt_x: i32,
    virt_y: i32,
    virt_w: i32,
    virt_h: i32,
) -> (i32, i32) {
    let denom = 65535_i64;
    let span_x = (virt_w as i64 - 1).max(0);
    let span_y = (virt_h as i64 - 1).max(0);

    let sx = ((dx.clamp(0, 65535) as i64 * span_x + denom / 2) / denom) + virt_x as i64;
    let sy = ((dy.clamp(0, 65535) as i64 * span_y + denom / 2) / denom) + virt_y as i64;

    (sx as i32, sy as i32)
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Layout {
        label: &'static str,
        vx: i32,
        vy: i32,
        vw: i32,
        vh: i32,
        points: Vec<(i32, i32)>,
    }

    fn test_layouts() -> Vec<Layout> {
        vec![
            Layout {
                label: "single-1080p",
                vx: 0,
                vy: 0,
                vw: 1920,
                vh: 1080,
                points: vec![(0, 0), (960, 540), (1919, 1079), (1, 1)],
            },
            Layout {
                label: "dual-secondary-left-1080p",
                vx: -1920,
                vy: 0,
                vw: 3840,
                vh: 1080,
                points: vec![(-1920, 0), (-1795, 383), (-1, 540), (0, 0), (1919, 1079)],
            },
            Layout {
                label: "dual-secondary-above-1440p",
                vx: 0,
                vy: -1440,
                vw: 2560,
                vh: 2880,
                points: vec![(0, -1440), (1280, -720), (0, 0), (2559, 1439)],
            },
            Layout {
                label: "triple-surround-mixed",
                vx: -1920,
                vy: -1080,
                vw: 5760,
                vh: 2160,
                points: vec![
                    (-1920, -1080),
                    (-960, -540),
                    (0, 0),
                    (1920, 0),
                    (3839, 1079),
                ],
            },
        ]
    }

    #[test]
    fn test_normalized_range_bounds() {
        for layout in test_layouts() {
            for (sx, sy) in layout.points {
                let (dx, dy) =
                    to_virtualdesk_absolute(sx, sy, layout.vx, layout.vy, layout.vw, layout.vh);
                assert!(
                    (0..=65535).contains(&dx),
                    "{}: dx={} out of bounds",
                    layout.label,
                    dx
                );
                assert!(
                    (0..=65535).contains(&dy),
                    "{}: dy={} out of bounds",
                    layout.label,
                    dy
                );
            }
        }
    }

    #[test]
    fn test_extreme_corners_mapping() {
        for layout in test_layouts() {
            let (tl_x, tl_y) = to_virtualdesk_absolute(
                layout.vx, layout.vy, layout.vx, layout.vy, layout.vw, layout.vh,
            );
            assert_eq!(tl_x, 0, "{}: Top-left X must map to 0", layout.label);
            assert_eq!(tl_y, 0, "{}: Top-left Y must map to 0", layout.label);

            let (br_x, br_y) = to_virtualdesk_absolute(
                layout.vx + layout.vw - 1,
                layout.vy + layout.vh - 1,
                layout.vx,
                layout.vy,
                layout.vw,
                layout.vh,
            );
            assert_eq!(
                br_x, 65535,
                "{}: Bottom-right X must map to 65535",
                layout.label
            );
            assert_eq!(
                br_y, 65535,
                "{}: Bottom-right Y must map to 65535",
                layout.label
            );
        }
    }

    #[test]
    fn test_round_trip_accuracy() {
        for layout in test_layouts() {
            for (sx, sy) in layout.points {
                let (dx, dy) =
                    to_virtualdesk_absolute(sx, sy, layout.vx, layout.vy, layout.vw, layout.vh);
                let (rx, ry) =
                    from_virtualdesk_absolute(dx, dy, layout.vx, layout.vy, layout.vw, layout.vh);
                assert!(
                    (rx - sx).abs() <= 1,
                    "{}: round-trip X drift (sx={}, rx={})",
                    layout.label,
                    sx,
                    rx
                );
                assert!(
                    (ry - sy).abs() <= 1,
                    "{}: round-trip Y drift (sy={}, ry={})",
                    layout.label,
                    sy,
                    ry
                );
            }
        }
    }

    #[test]
    fn test_negative_seam_monotonicity() {
        let (vx, vy, vw, vh) = (-1920, 0, 3840, 1080);
        // Ensure that pixels on secondary display stay on left half (< 32768)
        for sx in [-1920, -1000, -100, -1] {
            let (dx, _) = to_virtualdesk_absolute(sx, 500, vx, vy, vw, vh);
            assert!(
                dx < 32768,
                "sx={} on secondary display produced dx={}",
                sx,
                dx
            );
        }
        // Ensure that pixels on primary display stay on right half (>= 32768)
        for sx in [0, 100, 1000, 1919] {
            let (dx, _) = to_virtualdesk_absolute(sx, 500, vx, vy, vw, vh);
            assert!(
                dx >= 32768,
                "sx={} on primary display produced dx={}",
                sx,
                dx
            );
        }
    }
}
