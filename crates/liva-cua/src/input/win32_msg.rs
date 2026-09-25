//! Deepest-child Win32 window message routing and focused-descendant keyboard input.
//!
//! Provides non-intrusive background input dispatch using Win32 PostMessageW.
//! Recursively hit-tests down to leaf controls to avoid triggering top-level frame
//! activation via DefWindowProcW, and discovers focused child windows for keyboard routing.

use crate::types::{CuaError, MouseButton};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, POINT, TRUE, WPARAM};
#[cfg(windows)]
use windows_sys::Win32::Graphics::Gdi::ScreenToClient;
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::VK_RETURN;
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    ChildWindowFromPointEx, EnumChildWindows, GetClassLongPtrW, GetClassNameW, GetGUIThreadInfo,
    GetWindowThreadProcessId, IsChild, IsWindow, IsWindowVisible, PostMessageW, CS_DBLCLKS,
    CWP_SKIPDISABLED, CWP_SKIPINVISIBLE, CWP_SKIPTRANSPARENT, GCL_STYLE, GUITHREADINFO, WM_CHAR,
    WM_KEYDOWN, WM_KEYUP, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDBLCLK,
    WM_MBUTTONDOWN, WM_MBUTTONUP, WM_MOUSEMOVE, WM_RBUTTONDBLCLK, WM_RBUTTONDOWN, WM_RBUTTONUP,
};

pub const MK_LBUTTON: usize = 0x0001;
pub const MK_RBUTTON: usize = 0x0002;
pub const MK_MBUTTON: usize = 0x0010;

/// Maximum recursion depth for child window traversal to prevent cyclic loops.
pub const MAX_CHILD_RECURSION_DEPTH: usize = 16;

use crate::geometry::pack_lparam_clamped as pack_lparam;

#[cfg(windows)]
pub fn find_deepest_child_at(root: HWND, screen_pt: POINT) -> (HWND, POINT) {
    unsafe {
        if IsWindow(root) == 0 || IsWindowVisible(root) == 0 {
            return (root, screen_pt);
        }

        let mut current = root;
        let mut depth = 0;

        while depth < MAX_CHILD_RECURSION_DEPTH {
            depth += 1;

            let mut client_pt = screen_pt;
            if ScreenToClient(current, &mut client_pt) == 0 {
                break;
            }

            let child = ChildWindowFromPointEx(
                current,
                client_pt,
                CWP_SKIPINVISIBLE | CWP_SKIPDISABLED | CWP_SKIPTRANSPARENT,
            );

            // If no child found, or child is self, we have reached the leaf control
            if child == 0 || child == current {
                return (current, client_pt);
            }

            // Verify child is a valid visible window before descending
            if IsWindow(child) == 0 || IsWindowVisible(child) == 0 {
                return (current, client_pt);
            }

            current = child;
        }

        let mut final_client = screen_pt;
        ScreenToClient(current, &mut final_client);
        (current, final_client)
    }
}

#[cfg(windows)]
pub fn post_mouse_move(target_hwnd: HWND, client_x: i32, client_y: i32) -> Result<(), CuaError> {
    unsafe {
        let lparam = pack_lparam(client_x, client_y);
        let ok = PostMessageW(target_hwnd, WM_MOUSEMOVE, 0, lparam);
        if ok == 0 {
            return Err(CuaError::Win32Error(
                "PostMessageW WM_MOUSEMOVE failed".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn post_mouse_down(
    target_hwnd: HWND,
    client_x: i32,
    client_y: i32,
    button: MouseButton,
) -> Result<(), CuaError> {
    unsafe {
        let lparam = pack_lparam(client_x, client_y);
        let (msg, wparam) = match button {
            MouseButton::Left => (WM_LBUTTONDOWN, MK_LBUTTON as WPARAM),
            MouseButton::Right => (WM_RBUTTONDOWN, MK_RBUTTON as WPARAM),
            MouseButton::Middle => (WM_MBUTTONDOWN, MK_MBUTTON as WPARAM),
        };

        let ok = PostMessageW(target_hwnd, msg, wparam, lparam);
        if ok == 0 {
            return Err(CuaError::Win32Error(
                "PostMessageW button down failed".into(),
            ));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn post_mouse_up(
    target_hwnd: HWND,
    client_x: i32,
    client_y: i32,
    button: MouseButton,
) -> Result<(), CuaError> {
    unsafe {
        let lparam = pack_lparam(client_x, client_y);
        let msg = match button {
            MouseButton::Left => WM_LBUTTONUP,
            MouseButton::Right => WM_RBUTTONUP,
            MouseButton::Middle => WM_MBUTTONUP,
        };

        let ok = PostMessageW(target_hwnd, msg, 0, lparam);
        if ok == 0 {
            return Err(CuaError::Win32Error("PostMessageW button up failed".into()));
        }
        Ok(())
    }
}

#[cfg(windows)]
pub fn post_click(
    root_hwnd: HWND,
    screen_x: i32,
    screen_y: i32,
    button: MouseButton,
) -> Result<(), CuaError> {
    let screen_pt = POINT {
        x: screen_x,
        y: screen_y,
    };
    let (leaf_hwnd, client_pt) = find_deepest_child_at(root_hwnd, screen_pt);

    post_mouse_move(leaf_hwnd, client_pt.x, client_pt.y)?;
    post_mouse_down(leaf_hwnd, client_pt.x, client_pt.y, button)?;
    std::thread::sleep(Duration::from_millis(15));
    post_mouse_up(leaf_hwnd, client_pt.x, client_pt.y, button)?;

    Ok(())
}

#[cfg(windows)]
pub fn post_double_click(
    root_hwnd: HWND,
    screen_x: i32,
    screen_y: i32,
    button: MouseButton,
) -> Result<(), CuaError> {
    unsafe {
        let screen_pt = POINT {
            x: screen_x,
            y: screen_y,
        };
        let (leaf_hwnd, client_pt) = find_deepest_child_at(root_hwnd, screen_pt);

        let class_style = GetClassLongPtrW(leaf_hwnd, GCL_STYLE);
        let has_dblclks = (class_style & (CS_DBLCLKS as usize)) != 0;

        let lparam = pack_lparam(client_pt.x, client_pt.y);

        // First click
        post_mouse_move(leaf_hwnd, client_pt.x, client_pt.y)?;
        post_mouse_down(leaf_hwnd, client_pt.x, client_pt.y, button)?;
        std::thread::sleep(Duration::from_millis(10));
        post_mouse_up(leaf_hwnd, client_pt.x, client_pt.y, button)?;

        std::thread::sleep(Duration::from_millis(20));

        if has_dblclks {
            // Send explicit DBLCLK message
            let (dbl_msg, wparam) = match button {
                MouseButton::Left => (WM_LBUTTONDBLCLK, MK_LBUTTON as WPARAM),
                MouseButton::Right => (WM_RBUTTONDBLCLK, MK_RBUTTON as WPARAM),
                MouseButton::Middle => (WM_MBUTTONDBLCLK, MK_MBUTTON as WPARAM),
            };
            PostMessageW(leaf_hwnd, dbl_msg, wparam, lparam);
            std::thread::sleep(Duration::from_millis(10));
            post_mouse_up(leaf_hwnd, client_pt.x, client_pt.y, button)?;
        } else {
            // Fallback: rapid second click
            post_mouse_down(leaf_hwnd, client_pt.x, client_pt.y, button)?;
            std::thread::sleep(Duration::from_millis(10));
            post_mouse_up(leaf_hwnd, client_pt.x, client_pt.y, button)?;
        }

        Ok(())
    }
}

#[cfg(windows)]
pub fn find_focused_descendant(root_hwnd: HWND) -> HWND {
    unsafe {
        let mut pid = 0u32;
        let root_tid = GetWindowThreadProcessId(root_hwnd, &mut pid);

        let mut gui_info: GUITHREADINFO = std::mem::zeroed();
        gui_info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;

        if GetGUIThreadInfo(root_tid, &mut gui_info) != 0 && gui_info.hwndFocus != 0 {
            if gui_info.hwndFocus == root_hwnd || IsChild(root_hwnd, gui_info.hwndFocus) != 0 {
                return gui_info.hwndFocus;
            }
        }

        struct ThreadSearchContext {
            threads: Vec<u32>,
        }

        unsafe extern "system" fn enum_child_proc(hwnd: HWND, lparam: LPARAM) -> BOOL {
            let ctx = &mut *(lparam as *mut ThreadSearchContext);
            let mut pid = 0u32;
            let tid = GetWindowThreadProcessId(hwnd, &mut pid);
            if !ctx.threads.contains(&tid) {
                ctx.threads.push(tid);
            }
            TRUE
        }

        let mut ctx = ThreadSearchContext {
            threads: vec![root_tid],
        };

        EnumChildWindows(
            root_hwnd,
            Some(enum_child_proc),
            &mut ctx as *mut _ as LPARAM,
        );

        for tid in ctx.threads {
            let mut info: GUITHREADINFO = std::mem::zeroed();
            info.cbSize = std::mem::size_of::<GUITHREADINFO>() as u32;
            if GetGUIThreadInfo(tid, &mut info) != 0 && info.hwndFocus != 0 {
                if info.hwndFocus == root_hwnd || IsChild(root_hwnd, info.hwndFocus) != 0 {
                    return info.hwndFocus;
                }
            }
        }

        root_hwnd
    }
}

#[cfg(windows)]
pub fn is_xaml_host_hwnd(hwnd: HWND) -> bool {
    unsafe {
        let mut class_buf = [0u16; 256];
        let len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), 256);
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
            if class_name == "ApplicationFrameWindow"
                || class_name == "WinUIDesktopWin32WindowClass"
                || class_name == "Windows.UI.Core.CoreWindow"
                || class_name == "XamlExplorerHostIslandWindow"
            {
                return true;
            }
        }
        false
    }
}

#[cfg(windows)]
pub fn post_char(target_hwnd: HWND, ch: char) -> Result<(), CuaError> {
    unsafe {
        if ch == '\n' || ch == '\r' {
            let lparam_down: LPARAM = 0x001C0001; // repeat 1, scancode 0x1C
            let lparam_up: LPARAM = 0xC01C0001_u32 as LPARAM; // transition flag, repeat 1, scancode 0x1C
            PostMessageW(target_hwnd, WM_KEYDOWN, VK_RETURN as WPARAM, lparam_down);
            PostMessageW(target_hwnd, WM_KEYUP, VK_RETURN as WPARAM, lparam_up);
            return Ok(());
        }

        let mut utf16_buf = [0u16; 2];
        let encoded = ch.encode_utf16(&mut utf16_buf);
        for &code_unit in encoded.iter() {
            let ok = PostMessageW(target_hwnd, WM_CHAR, code_unit as WPARAM, 1);
            if ok == 0 {
                return Err(CuaError::Win32Error("PostMessageW WM_CHAR failed".into()));
            }
        }

        Ok(())
    }
}

#[cfg(windows)]
pub async fn post_text(
    root_hwnd: HWND,
    text: &str,
    cancel_token: Option<&crate::kill_switch::CancellationToken>,
) -> Result<(), CuaError> {
    if is_xaml_host_hwnd(root_hwnd) {
        return Err(CuaError::background_unavailable(
            root_hwnd as usize as u64,
            "Target window is a modern XAML island/UWP host which ignores background WM_CHAR. Re-run with delivery_mode: 'foreground'.",
        ));
    }

    let target = find_focused_descendant(root_hwnd);

    for ch in text.chars() {
        if let Some(token) = cancel_token {
            if token.is_cancelled() {
                return Err(CuaError::EmergencyHalted);
            }
        }

        post_char(target, ch)?;

        if let Some(token) = cancel_token {
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(10)) => {}
                _ = token.cancelled() => {
                    return Err(CuaError::EmergencyHalted);
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    }

    Ok(())
}

#[cfg(windows)]
pub fn post_virtual_key(target_hwnd: HWND, vk: u16, is_extended: bool) -> Result<(), CuaError> {
    unsafe {
        let mut lparam_down: LPARAM = 1;
        let mut lparam_up: LPARAM = 0xC0000001_u32 as LPARAM;

        if is_extended {
            lparam_down |= 0x01000000;
            lparam_up |= 0x01000000;
        }

        PostMessageW(target_hwnd, WM_KEYDOWN, vk as WPARAM, lparam_down);
        std::thread::sleep(Duration::from_millis(15));
        PostMessageW(target_hwnd, WM_KEYUP, vk as WPARAM, lparam_up);

        Ok(())
    }
}

// Non-windows stubs
#[cfg(not(windows))]
pub fn post_click(
    _root_hwnd: u64,
    _screen_x: i32,
    _screen_y: i32,
    _button: MouseButton,
) -> Result<(), CuaError> {
    Ok(())
}
#[cfg(not(windows))]
pub fn post_double_click(
    _root_hwnd: u64,
    _screen_x: i32,
    _screen_y: i32,
    _button: MouseButton,
) -> Result<(), CuaError> {
    Ok(())
}
#[cfg(not(windows))]
pub async fn post_text(
    _root_hwnd: u64,
    _text: &str,
    _cancel_token: Option<&crate::kill_switch::CancellationToken>,
) -> Result<(), CuaError> {
    Ok(())
}
#[cfg(not(windows))]
pub fn post_virtual_key(_target_hwnd: u64, _vk: u16, _is_extended: bool) -> Result<(), CuaError> {
    Ok(())
}
