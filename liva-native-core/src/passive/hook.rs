use std::sync::atomic::{AtomicIsize, AtomicU32, Ordering};
use std::sync::mpsc::Sender;

#[derive(Debug, Clone)]
pub enum RawEvent {
    KeyPress {
        key: String,
        vk_code: u32,
        window_title: String,
        process_name: String,
    },
    MouseClick {
        button: String,
        x: i32,
        y: i32,
        window_title: String,
        process_name: String,
    },
}

#[cfg(windows)]
static EVENT_SENDER: std::sync::OnceLock<Sender<RawEvent>> = std::sync::OnceLock::new();

#[cfg(windows)]
static KEYBOARD_HOOK: AtomicIsize = AtomicIsize::new(0);
#[cfg(windows)]
static MOUSE_HOOK: AtomicIsize = AtomicIsize::new(0);
#[cfg(windows)]
static HOOK_THREAD_ID: AtomicU32 = AtomicU32::new(0);

#[cfg(windows)]
unsafe fn vk_to_char(vk: u32, _scan_code: u32) -> Option<char> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;

    let raw_char = unsafe { MapVirtualKeyW(vk, 2) };
    if raw_char == 0 {
        return None;
    }

    let c = (raw_char & 0xFFFF) as u32;
    let mut ch = std::char::from_u32(c)?;

    let is_shift = unsafe { GetKeyState(VK_SHIFT as i32) < 0 };
    let is_caps = unsafe { GetKeyState(VK_CAPITAL as i32) & 1 != 0 };

    if ch.is_ascii_alphabetic() {
        if is_shift ^ is_caps {
            ch = ch.to_ascii_uppercase();
        } else {
            ch = ch.to_ascii_lowercase();
        }
    } else if is_shift {
        ch = match ch {
            '1' => '!',
            '2' => '@',
            '3' => '#',
            '4' => '$',
            '5' => '%',
            '6' => '^',
            '7' => '&',
            '8' => '*',
            '9' => '(',
            '0' => ')',
            '-' => '_',
            '=' => '+',
            '[' => '{',
            ']' => '}',
            '\\' => '|',
            ';' => ':',
            '\'' => '"',
            ',' => '<',
            '.' => '>',
            '/' => '?',
            '`' => '~',
            other => other,
        };
    }

    Some(ch)
}

#[cfg(windows)]
unsafe fn get_active_window_info() -> (String, String) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, QueryFullProcessImageNameW};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowTextW, GetWindowThreadProcessId,
    };

    let hwnd = unsafe { GetForegroundWindow() };
    if hwnd == 0 {
        return ("".to_string(), "".to_string());
    }

    let mut title_buf = [0u16; 512];
    let len = unsafe { GetWindowTextW(hwnd, title_buf.as_mut_ptr(), title_buf.len() as i32) };
    let title = if len > 0 {
        String::from_utf16_lossy(&title_buf[..len as usize])
    } else {
        "".to_string()
    };

    let mut pid: u32 = 0;
    unsafe {
        GetWindowThreadProcessId(hwnd, &mut pid);
    }
    if pid == 0 {
        return (title, "".to_string());
    }

    // PROCESS_QUERY_LIMITED_INFORMATION = 0x1000
    let process_handle = unsafe { OpenProcess(0x1000, 0, pid) };
    if process_handle == 0 {
        return (title, "".to_string());
    }

    let mut path_buf = [0u16; 1024];
    let mut size = path_buf.len() as u32;
    let success =
        unsafe { QueryFullProcessImageNameW(process_handle, 0, path_buf.as_mut_ptr(), &mut size) };
    unsafe {
        CloseHandle(process_handle);
    }

    let process_name = if success != 0 {
        let full_path = String::from_utf16_lossy(&path_buf[..size as usize]);
        std::path::Path::new(&full_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string()
    } else {
        "".to_string()
    };

    (title, process_name)
}

#[cfg(windows)]
fn send_event(event: RawEvent) {
    if let Some(tx) = EVENT_SENDER.get() {
        let _ = tx.send(event);
    }
}

#[cfg(windows)]
unsafe extern "system" fn keyboard_proc(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, KBDLLHOOKSTRUCT, WM_KEYDOWN, WM_SYSKEYDOWN,
    };

    if lparam == 0 {
        return unsafe { CallNextHookEx(0, code, wparam, lparam) };
    }

    if code == HC_ACTION as i32 {
        let msg = wparam as u32;
        if msg == WM_KEYDOWN || msg == WM_SYSKEYDOWN {
            let kb_struct = lparam as *const KBDLLHOOKSTRUCT;
            let vk = unsafe { (*kb_struct).vkCode };
            let scan_code = unsafe { (*kb_struct).scanCode };

            let key_str = unsafe { vk_to_char(vk, scan_code) }
                .map(|c| c.to_string())
                .unwrap_or_default();

            let is_special = vk == 0x08 || vk == 0x0D || vk == 0x09; // Backspace, Enter, Tab
            if !key_str.is_empty() || is_special {
                let (window_title, process_name) = unsafe { get_active_window_info() };
                let event = RawEvent::KeyPress {
                    key: key_str,
                    vk_code: vk,
                    window_title,
                    process_name,
                };
                send_event(event);
            }
        }
    }

    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

#[cfg(windows)]
unsafe extern "system" fn mouse_proc(
    code: i32,
    wparam: windows_sys::Win32::Foundation::WPARAM,
    lparam: windows_sys::Win32::Foundation::LPARAM,
) -> windows_sys::Win32::Foundation::LRESULT {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, HC_ACTION, MSLLHOOKSTRUCT, WM_LBUTTONDOWN, WM_MBUTTONDOWN, WM_RBUTTONDOWN,
    };

    if lparam == 0 {
        return unsafe { CallNextHookEx(0, code, wparam, lparam) };
    }

    if code == HC_ACTION as i32 {
        let msg = wparam as u32;
        let mut button = None;
        if msg == WM_LBUTTONDOWN {
            button = Some("Left".to_string());
        } else if msg == WM_RBUTTONDOWN {
            button = Some("Right".to_string());
        } else if msg == WM_MBUTTONDOWN {
            button = Some("Middle".to_string());
        }

        if let Some(btn) = button {
            let ms = lparam as *const MSLLHOOKSTRUCT;
            let (x, y) = unsafe { ((*ms).pt.x, (*ms).pt.y) };

            let (window_title, process_name) = unsafe { get_active_window_info() };
            let event = RawEvent::MouseClick {
                button: btn,
                x,
                y,
                window_title,
                process_name,
            };
            send_event(event);
        }
    }

    unsafe { CallNextHookEx(0, code, wparam, lparam) }
}

#[cfg(windows)]
pub fn start_os_hook(tx: Sender<RawEvent>) -> Result<(), String> {
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        DispatchMessageW, GetMessageW, MSG, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, WH_KEYBOARD_LL, WH_MOUSE_LL,
    };

    if EVENT_SENDER.set(tx).is_err() {
        return Err("Event sender already initialized".to_string());
    }

    std::thread::spawn(move || unsafe {
        let h_instance = GetModuleHandleW(std::ptr::null());
        let k_hook = SetWindowsHookExW(WH_KEYBOARD_LL, Some(keyboard_proc), h_instance, 0);
        let m_hook = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_proc), h_instance, 0);

        if k_hook == 0 || m_hook == 0 {
            if k_hook != 0 {
                UnhookWindowsHookEx(k_hook);
            }
            if m_hook != 0 {
                UnhookWindowsHookEx(m_hook);
            }
            return;
        }

        KEYBOARD_HOOK.store(k_hook, Ordering::SeqCst);
        MOUSE_HOOK.store(m_hook, Ordering::SeqCst);
        HOOK_THREAD_ID.store(
            windows_sys::Win32::System::Threading::GetCurrentThreadId(),
            Ordering::SeqCst,
        );

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, 0, 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let k_hook_stored = KEYBOARD_HOOK.swap(0, Ordering::SeqCst);
        if k_hook_stored != 0 {
            UnhookWindowsHookEx(k_hook_stored);
        }
        let m_hook_stored = MOUSE_HOOK.swap(0, Ordering::SeqCst);
        if m_hook_stored != 0 {
            UnhookWindowsHookEx(m_hook_stored);
        }
        HOOK_THREAD_ID.store(0, Ordering::SeqCst);
    });

    Ok(())
}

#[cfg(windows)]
pub fn stop_os_hook() -> Result<(), String> {
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        PostThreadMessageW, UnhookWindowsHookEx, WM_QUIT,
    };

    let thread_id = HOOK_THREAD_ID.swap(0, Ordering::SeqCst);
    if thread_id != 0 {
        unsafe {
            PostThreadMessageW(thread_id, WM_QUIT, 0, 0);
        }
    }

    let k_hook = KEYBOARD_HOOK.swap(0, Ordering::SeqCst);
    if k_hook != 0 {
        unsafe {
            UnhookWindowsHookEx(k_hook);
        }
    }

    let m_hook = MOUSE_HOOK.swap(0, Ordering::SeqCst);
    if m_hook != 0 {
        unsafe {
            UnhookWindowsHookEx(m_hook);
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn start_os_hook(_tx: Sender<RawEvent>) -> Result<(), String> {
    Ok(())
}

#[cfg(not(windows))]
pub fn stop_os_hook() -> Result<(), String> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_stop_os_hook_does_not_panic() {
        let res = stop_os_hook();
        assert!(res.is_ok());
    }

    #[cfg(windows)]
    #[test]
    fn test_vk_to_char_mapping() {
        unsafe {
            let ch = vk_to_char(0x41, 0);
            if let Some(c) = ch {
                assert!(c == 'a' || c == 'A');
            }
        }
        unsafe {
            let ch = vk_to_char(0x31, 0);
            if let Some(c) = ch {
                assert!(c == '1' || c == '!');
            }
        }
    }
}
