//! Per-Monitor V2 DPI awareness scaling and coordinate conversions.
//!
//! Windows 10 Creators Update (1703)+ supports Per-Monitor V2 (`DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2`),
//! ensuring non-client areas, child windows, and dialogs are automatically scaled by Windows
//! while Win32 APIs return physical device pixels.
//!
//! This module provides initialization, DPI querying, and exact integer scaling routines.

/// DPI awareness context constants for Win32 API.
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2: isize = -4;
pub const DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE: isize = -3;
pub const DPI_AWARENESS_CONTEXT_SYSTEM_AWARE: isize = -2;
pub const DPI_AWARENESS_CONTEXT_UNAWARE: isize = -1;

pub const STANDARD_DPI: u32 = 96;

/// Initialize Per-Monitor V2 DPI awareness on the current process.
///
/// Must be invoked early in the process lifecycle before any windows are created.
/// Falls back to Per-Monitor V1 or System-aware if Per-Monitor V2 is unsupported.
#[cfg(windows)]
pub fn init_per_monitor_v2_dpi_awareness() -> bool {
    use windows_sys::Win32::UI::HiDpi::SetProcessDpiAwarenessContext;
    use windows_sys::Win32::UI::WindowsAndMessaging::SetProcessDPIAware;

    unsafe {
        // Attempt Per-Monitor V2
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
            return true;
        }
        // Fallback to Per-Monitor V1
        if SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE) != 0 {
            return true;
        }
        // Fallback to legacy System DPI Aware
        SetProcessDPIAware() != 0
    }
}

#[cfg(not(windows))]
pub fn init_per_monitor_v2_dpi_awareness() -> bool {
    false
}

/// Retrieve the effective DPI for a given window handle.
/// Falls back to system DPI or standard 96 DPI if handle is invalid.
#[cfg(windows)]
pub fn get_dpi_for_window(hwnd: u64) -> u32 {
    use windows_sys::Win32::Foundation::HWND;
    use windows_sys::Win32::UI::HiDpi::{GetDpiForSystem, GetDpiForWindow};

    if hwnd == 0 {
        return get_dpi_for_system();
    }

    unsafe {
        let dpi = GetDpiForWindow(hwnd as HWND);
        if dpi > 0 {
            dpi
        } else {
            let sys_dpi = GetDpiForSystem();
            if sys_dpi > 0 {
                sys_dpi
            } else {
                STANDARD_DPI
            }
        }
    }
}

#[cfg(not(windows))]
pub fn get_dpi_for_window(_hwnd: u64) -> u32 {
    STANDARD_DPI
}

/// Retrieve system-wide baseline DPI.
#[cfg(windows)]
pub fn get_dpi_for_system() -> u32 {
    use windows_sys::Win32::UI::HiDpi::GetDpiForSystem;
    unsafe {
        let dpi = GetDpiForSystem();
        if dpi > 0 {
            dpi
        } else {
            STANDARD_DPI
        }
    }
}

#[cfg(not(windows))]
pub fn get_dpi_for_system() -> u32 {
    STANDARD_DPI
}

/// Calculate float DPI scale factor relative to standard 96 DPI (1.0 = 100%).
#[inline]
pub fn dpi_scale_factor(dpi: u32) -> f64 {
    dpi.max(1) as f64 / STANDARD_DPI as f64
}

/// Convert logical device-independent coordinates (DIPs) to physical device pixels.
/// Uses round-to-nearest integer arithmetic `(val * dpi + 48) / 96`.
#[inline]
pub fn logical_to_physical_pt(lx: i32, ly: i32, dpi: u32) -> (i32, i32) {
    let scale = dpi.max(1) as i64;
    let px = (lx as i64 * scale + 48) / 96;
    let py = (ly as i64 * scale + 48) / 96;
    (px as i32, py as i32)
}

/// Convert physical device pixels to logical coordinates (DIPs).
/// Uses round-to-nearest integer arithmetic `(px * 96 + scale/2) / scale`.
#[inline]
pub fn physical_to_logical_pt(px: i32, py: i32, dpi: u32) -> (i32, i32) {
    let scale = dpi.max(1) as i64;
    let lx = (px as i64 * 96 + scale / 2) / scale;
    let ly = (py as i64 * 96 + scale / 2) / scale;
    (lx as i32, ly as i32)
}

/// Rescale dimensions between different DPI levels (e.g. dragging across monitors).
#[inline]
pub fn rescale_dimension(val: i32, from_dpi: u32, to_dpi: u32) -> i32 {
    let from_scale = from_dpi.max(1) as i64;
    let to_scale = to_dpi.max(1) as i64;
    ((val as i64 * to_scale + from_scale / 2) / from_scale) as i32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpi_scale_factors() {
        assert_eq!(dpi_scale_factor(96), 1.0);
        assert_eq!(dpi_scale_factor(120), 1.25);
        assert_eq!(dpi_scale_factor(144), 1.50);
        assert_eq!(dpi_scale_factor(192), 2.0);
    }

    #[test]
    fn test_logical_to_physical_conversions() {
        // At 150% (144 DPI): 100 logical -> 150 physical
        let (px, py) = logical_to_physical_pt(100, 200, 144);
        assert_eq!((px, py), (150, 300));

        // At 125% (120 DPI): 80 logical -> 100 physical
        let (px, py) = logical_to_physical_pt(80, 160, 120);
        assert_eq!((px, py), (100, 200));
    }

    #[test]
    fn test_round_trip_physical_logical() {
        for dpi in [96, 120, 144, 192, 240] {
            for logical in [0, 10, 50, 100, 640, 1080] {
                let (px, _) = logical_to_physical_pt(logical, 0, dpi);
                let (lx, _) = physical_to_logical_pt(px, 0, dpi);
                assert!(
                    (lx - logical).abs() <= 1,
                    "DPI {} round trip drift for {}",
                    dpi,
                    logical
                );
            }
        }
    }
}
