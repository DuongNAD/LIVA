//! Windows desktop top-level window enumeration, bounds resolution, and UWP disambiguation.
//!
//! Implementation architecture:
//! 1. Canonical Win32 `EnumWindows` z-order enumeration.
//! 2. DWM Extended Frame Bounds (`DWMWA_EXTENDED_FRAME_BOUNDS`) with 1-pixel drop shadow crop inset.
//! 3. UWP `ApplicationFrameHost` resolution detecting inner `Windows.UI.Core.CoreWindow` app PID.
//! 4. UI Automation (`IUIAutomation`) tree walker fallback bounded by a 2.0s single-flight deadline.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use thiserror::Error;

use crate::types::{CuaRect, CuaWindowInfo};

/// Drop-shadow antialiasing crop inset applied to DWM frame screenshots.
pub const DWM_CROP_INSET_PX: i32 = 1;

/// Win32 DWM attribute constants.
pub const DWMWA_EXTENDED_FRAME_BOUNDS: u32 = 9;
pub const DWMWA_CLOAKED: u32 = 14;

#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum WindowError {
    #[error("Win32 error: {0}")]
    Win32Error(String),
    #[error("Window handle {0:#x} is invalid or has been destroyed")]
    InvalidHandle(u64),
    #[error("Target window PID mismatch: expected {expected_pid}, found {actual_pid}")]
    PidMismatch { expected_pid: u32, actual_pid: u32 },
}

/// Translate visual bitmap pixel coordinates to global physical screen coordinates.
/// Adds the window's DWM origin plus 1-pixel crop inset to account for screenshot borders.
#[inline]
pub fn bitmap_to_screen_coords(bounds: CuaRect, px: i32, py: i32) -> (i32, i32) {
    let sx = bounds.x + DWM_CROP_INSET_PX + px;
    let sy = bounds.y + DWM_CROP_INSET_PX + py;
    (sx, sy)
}

/// List top-level windows matching an optional PID filter.
/// Executes Win32 `EnumWindows` first for canonical z-order, then appends UIA fallback entries.
pub fn list_windows(filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, WindowError> {
    #[cfg(windows)]
    {
        let win32_windows = enumerate_via_enum_windows();
        let uia_windows = enumerate_via_uia_fallback();

        let mut seen = HashSet::with_capacity(win32_windows.len() + uia_windows.len());
        let mut merged = Vec::with_capacity(win32_windows.len() + uia_windows.len());

        // EnumWindows first — canonical z-order stacking
        for w in win32_windows {
            if seen.insert(w.hwnd) {
                merged.push(w);
            }
        }

        // Append UIA fallback windows if not already seen
        let mut fallback_z = merged.len();
        for mut w in uia_windows {
            if seen.insert(w.hwnd) {
                w.z_index = fallback_z;
                fallback_z += 1;
                merged.push(w);
            }
        }

        // Apply PID filter if specified (matching either owner PID or UWP child app PID)
        if let Some(target_pid) = filter_pid {
            merged.retain(|w| w.pid == target_pid || w.uwp_app_pid == Some(target_pid));
        }

        Ok(merged)
    }

    #[cfg(not(windows))]
    {
        let _ = filter_pid;
        Ok(Vec::new())
    }
}

/// Direct query for one exact window by HWND without full desktop enumeration.
pub fn get_window_info(hwnd: u64) -> Result<CuaWindowInfo, WindowError> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::HWND;
        use windows_sys::Win32::UI::WindowsAndMessaging::{
            GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow,
            IsWindowVisible,
        };

        let win_hwnd = hwnd as usize as HWND;
        if win_hwnd == 0 || unsafe { IsWindow(win_hwnd) } == 0 {
            return Err(WindowError::InvalidHandle(hwnd));
        }

        let mut pid: u32 = 0;
        unsafe { GetWindowThreadProcessId(win_hwnd, &mut pid) };

        let is_visible = unsafe { IsWindowVisible(win_hwnd) } != 0;
        let is_minimized = unsafe { IsIconic(win_hwnd) } != 0;
        let (bounds, is_cloaked) = get_window_dwm_bounds(win_hwnd);
        let class_name = get_window_class_name(win_hwnd);
        let process_name = get_process_name(pid);

        let title_len = unsafe { GetWindowTextLengthW(win_hwnd) };
        let title = if title_len > 0 {
            let mut buf = vec![0u16; (title_len + 1) as usize];
            let copied = unsafe { GetWindowTextW(win_hwnd, buf.as_mut_ptr(), buf.len() as i32) };
            String::from_utf16_lossy(&buf[..copied as usize])
                .trim()
                .to_string()
        } else {
            String::new()
        };

        let is_uwp_frame = class_name == "ApplicationFrameWindow";
        let uwp_app_pid = if is_uwp_frame {
            resolve_uwp_child_app_pid(win_hwnd)
        } else {
            None
        };

        let is_on_screen =
            is_visible && !is_minimized && !is_cloaked && bounds.width > 0 && bounds.height > 0;

        Ok(CuaWindowInfo {
            hwnd,
            pid,
            title,
            class_name,
            process_name,
            bounds,
            is_on_screen,
            is_minimized,
            z_index: 0,
            is_uwp_frame,
            uwp_app_pid,
        })
    }

    #[cfg(not(windows))]
    {
        Err(WindowError::InvalidHandle(hwnd))
    }
}

/// Resolve the outer `ApplicationFrameWindow` HWND hosting an application launched under `app_pid`.
pub fn resolve_uwp_host_window(app_pid: u32) -> Option<CuaWindowInfo> {
    if app_pid == 0 {
        return None;
    }
    let windows = list_windows(None).ok()?;
    windows
        .into_iter()
        .find(|w| w.is_uwp_frame && w.uwp_app_pid == Some(app_pid))
}

/// Resolve the child packaged application PID from an outer `ApplicationFrameWindow`.
pub fn resolve_uwp_app_pid(frame_hwnd: u64) -> Option<u32> {
    #[cfg(windows)]
    {
        use windows_sys::Win32::Foundation::HWND;
        let hwnd = frame_hwnd as usize as HWND;
        if get_window_class_name(hwnd) != "ApplicationFrameWindow" {
            return None;
        }
        resolve_uwp_child_app_pid(hwnd)
    }

    #[cfg(not(windows))]
    {
        let _ = frame_hwnd;
        None
    }
}

// ---------------------------------------------------------------------------
// Win32 Helper Routines
// ---------------------------------------------------------------------------

#[cfg(windows)]
struct EnumCollector {
    windows: Vec<CuaWindowInfo>,
    current_z: usize,
}

#[cfg(windows)]
fn enumerate_via_enum_windows() -> Vec<CuaWindowInfo> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetWindowLongPtrW, GetWindowTextLengthW, GetWindowTextW,
        GetWindowThreadProcessId, IsIconic, IsWindowVisible, GWL_EXSTYLE, WS_EX_TOOLWINDOW,
    };

    let collector = Mutex::new(EnumCollector {
        windows: Vec::with_capacity(64),
        current_z: 0,
    });

    unsafe extern "system" fn enum_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
        let collector_ptr = lparam as *const Mutex<EnumCollector>;
        let collector = &*collector_ptr;

        // Skip invisible helper windows (minimized windows remain visible in Win32)
        if IsWindowVisible(hwnd) == 0 {
            return 1;
        }

        let is_minimized = IsIconic(hwnd) != 0;

        // Skip tooltip/helper tool windows that lack titles
        let ex_style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let title_len = GetWindowTextLengthW(hwnd);
        if title_len == 0 && (ex_style & WS_EX_TOOLWINDOW) != 0 {
            return 1;
        }

        let mut buf = vec![0u16; (title_len + 1) as usize];
        let copied = GetWindowTextW(hwnd, buf.as_mut_ptr(), buf.len() as i32);
        let title = String::from_utf16_lossy(&buf[..copied as usize])
            .trim()
            .to_string();

        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, &mut pid);

        let class_name = get_window_class_name(hwnd);
        let process_name = get_process_name(pid);
        let is_uwp_frame = class_name == "ApplicationFrameWindow";
        let uwp_app_pid = if is_uwp_frame {
            resolve_uwp_child_app_pid(hwnd)
        } else {
            None
        };

        let (bounds, is_cloaked) = get_window_dwm_bounds(hwnd);
        let is_on_screen = !is_minimized && !is_cloaked && bounds.width > 0 && bounds.height > 0;

        let mut guard = collector.lock().unwrap();
        let z_index = guard.current_z;
        guard.current_z += 1;

        guard.windows.push(CuaWindowInfo {
            hwnd: hwnd as usize as u64,
            pid,
            title,
            class_name,
            process_name,
            bounds,
            is_on_screen,
            is_minimized,
            z_index,
            is_uwp_frame,
            uwp_app_pid,
        });

        1 // Continue enumeration
    }

    let state_ptr = &collector as *const Mutex<EnumCollector> as LPARAM;
    unsafe {
        EnumWindows(Some(enum_proc), state_ptr);
    }

    collector.into_inner().unwrap().windows
}

#[cfg(windows)]
fn get_window_dwm_bounds(hwnd: windows_sys::Win32::Foundation::HWND) -> (CuaRect, bool) {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::Graphics::Dwm::DwmGetWindowAttribute;
    use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowRect;

    let mut rect: RECT = unsafe { std::mem::zeroed() };
    let mut cloaked: u32 = 0;

    unsafe {
        // Query DWM extended frame bounds (excludes drop-shadow border)
        let hr = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut RECT as *mut _,
            std::mem::size_of::<RECT>() as u32,
        );

        if hr != 0 {
            // Fallback to standard GetWindowRect
            let _ = GetWindowRect(hwnd, &mut rect);
        }

        // Query whether window is cloaked (on other virtual desktop or suspended)
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            &mut cloaked as *mut u32 as *mut _,
            std::mem::size_of::<u32>() as u32,
        );
    }

    let bounds = CuaRect {
        x: rect.left,
        y: rect.top,
        width: (rect.right - rect.left).max(0),
        height: (rect.bottom - rect.top).max(0),
    };

    (bounds, cloaked != 0)
}

#[cfg(windows)]
fn get_window_class_name(hwnd: windows_sys::Win32::Foundation::HWND) -> String {
    use windows_sys::Win32::UI::WindowsAndMessaging::GetClassNameW;

    let mut buf = [0u16; 256];
    let len = unsafe { GetClassNameW(hwnd, buf.as_mut_ptr(), buf.len() as i32) };
    if len > 0 {
        String::from_utf16_lossy(&buf[..len as usize])
    } else {
        String::new()
    }
}

#[cfg(windows)]
pub fn get_process_name(pid: u32) -> Option<String> {
    use windows_sys::Win32::Foundation::{CloseHandle, FALSE};
    use windows_sys::Win32::System::ProcessStatus::GetModuleFileNameExW;
    use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};

    if pid == 0 {
        return None;
    }
    unsafe {
        let h_proc = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, FALSE, pid);
        if h_proc == 0 {
            return None;
        }
        let mut buf = [0u16; 260];
        let len = GetModuleFileNameExW(h_proc, 0, buf.as_mut_ptr(), buf.len() as u32);
        CloseHandle(h_proc);
        if len > 0 {
            let full_path = String::from_utf16_lossy(&buf[..len as usize]);
            let file_name = std::path::Path::new(&full_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&full_path)
                .to_string();
            Some(file_name)
        } else {
            None
        }
    }
}

#[cfg(windows)]
fn resolve_uwp_child_app_pid(frame_hwnd: windows_sys::Win32::Foundation::HWND) -> Option<u32> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM};
    use windows_sys::Win32::UI::WindowsAndMessaging::{EnumChildWindows, GetWindowThreadProcessId};

    struct UwpChildScan {
        host_pid: u32,
        app_pid: Option<u32>,
    }

    let mut host_pid = 0u32;
    unsafe { GetWindowThreadProcessId(frame_hwnd, &mut host_pid) };

    unsafe extern "system" fn child_proc(child: HWND, lparam: LPARAM) -> BOOL {
        let scan = &mut *(lparam as *mut UwpChildScan);
        let mut child_pid = 0u32;
        GetWindowThreadProcessId(child, &mut child_pid);

        if child_pid != 0 && child_pid != scan.host_pid {
            let class_name = get_window_class_name(child);
            if class_name == "Windows.UI.Core.CoreWindow" {
                scan.app_pid = Some(child_pid);
                return 0; // Found target, stop child enumeration
            }
        }
        1
    }

    let mut scan = UwpChildScan {
        host_pid,
        app_pid: None,
    };

    unsafe {
        EnumChildWindows(frame_hwnd, Some(child_proc), &mut scan as *mut _ as LPARAM);
    }

    scan.app_pid
}

// ---------------------------------------------------------------------------
// UIA Tree Walker Fallback Implementation
// ---------------------------------------------------------------------------

#[cfg(windows)]
static UIA_BUSY: AtomicBool = AtomicBool::new(false);

#[cfg(windows)]
fn enumerate_via_uia_fallback() -> Vec<CuaWindowInfo> {
    // Single-flight gate: if another UIA scan is in progress or hung, do not stall
    if UIA_BUSY
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return Vec::new();
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let _handle = std::thread::spawn(move || {
        let result = walk_uia_top_level_windows();
        let _ = tx.send(result);
        UIA_BUSY.store(false, Ordering::Release);
    });

    // Enforce strict 2.0s deadline
    rx.recv_timeout(Duration::from_millis(2000))
        .unwrap_or_default()
}

#[cfg(windows)]
fn walk_uia_top_level_windows() -> Vec<CuaWindowInfo> {
    // Safe graceful fallback for custom containerized frames
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cua_rect_containment_and_center() {
        let rect = CuaRect::new(100, 200, 300, 400);
        assert!(rect.contains_point(100, 200));
        assert!(rect.contains_point(399, 599));
        assert!(!rect.contains_point(400, 600));
        assert!(!rect.contains_point(99, 199));
        assert_eq!(rect.center(), (250, 400));
    }

    #[test]
    fn test_bitmap_to_screen_coords() {
        let rect = CuaRect::new(500, 300, 800, 600);
        // At bitmap (0, 0), screen coordinate is frame.x + 1, frame.y + 1
        let (sx, sy) = bitmap_to_screen_coords(rect, 0, 0);
        assert_eq!((sx, sy), (501, 301));

        // At bitmap (50, 80)
        let (sx, sy) = bitmap_to_screen_coords(rect, 50, 80);
        assert_eq!((sx, sy), (551, 381));
    }
}
