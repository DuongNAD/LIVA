//! Keyboard actuation controller.

use crate::input::{
    force_foreground_attached, validate_window_and_uipi, would_be_silently_dropped,
    ActuationResult, InputEventKind,
};
use crate::types::{CuaDeliveryMode, CuaError};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN, WM_KEYUP};

/// Parses a key descriptor string into (virtual_key_code, is_extended_key).
pub fn parse_virtual_key(key: &str) -> Result<(u16, bool), CuaError> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return Err(CuaError::Win32Error("Empty key string".into()));
    }
    let lower = trimmed.to_ascii_lowercase();
    match lower.as_str() {
        // Modifiers
        "ctrl" | "control" | "lctrl" => Ok((0x11, false)), // VK_CONTROL
        "rctrl" => Ok((0x11, true)),
        "alt" | "menu" | "lalt" => Ok((0x12, false)), // VK_MENU
        "ralt" => Ok((0x12, true)),
        "shift" | "lshift" => Ok((0x10, false)), // VK_SHIFT
        "rshift" => Ok((0x10, true)),
        "win" | "windows" | "super" | "lwin" => Ok((0x5B, false)), // VK_LWIN
        "rwin" => Ok((0x5C, false)),                               // VK_RWIN

        // Common keys
        "enter" | "return" => Ok((0x0D, false)), // VK_RETURN
        "esc" | "escape" => Ok((0x1B, false)),   // VK_ESCAPE
        "tab" => Ok((0x09, false)),              // VK_TAB
        "space" | " " => Ok((0x20, false)),      // VK_SPACE
        "backspace" | "back" => Ok((0x08, false)), // VK_BACK
        "del" | "delete" => Ok((0x2E, true)),    // VK_DELETE
        "ins" | "insert" => Ok((0x2D, true)),    // VK_INSERT
        "home" => Ok((0x24, true)),              // VK_HOME
        "end" => Ok((0x23, true)),               // VK_END
        "pageup" | "prior" => Ok((0x21, true)),  // VK_PRIOR
        "pagedown" | "next" => Ok((0x22, true)), // VK_NEXT
        "left" => Ok((0x25, true)),              // VK_LEFT
        "up" => Ok((0x26, true)),                // VK_UP
        "right" => Ok((0x27, true)),             // VK_RIGHT
        "down" => Ok((0x28, true)),              // VK_DOWN
        "capslock" | "caps" => Ok((0x14, false)), // VK_CAPITAL
        "numlock" => Ok((0x90, true)),           // VK_NUMLOCK
        "scrolllock" => Ok((0x91, false)),       // VK_SCROLL
        "printscreen" | "prtscn" => Ok((0x2C, true)), // VK_SNAPSHOT
        "pause" => Ok((0x13, false)),            // VK_PAUSE

        // Function keys F1 - F24
        "f1" => Ok((0x70, false)),
        "f2" => Ok((0x71, false)),
        "f3" => Ok((0x72, false)),
        "f4" => Ok((0x73, false)),
        "f5" => Ok((0x74, false)),
        "f6" => Ok((0x75, false)),
        "f7" => Ok((0x76, false)),
        "f8" => Ok((0x77, false)),
        "f9" => Ok((0x78, false)),
        "f10" => Ok((0x79, false)),
        "f11" => Ok((0x7A, false)),
        "f12" => Ok((0x7B, false)),
        "f13" => Ok((0x7C, false)),
        "f14" => Ok((0x7D, false)),
        "f15" => Ok((0x7E, false)),
        "f16" => Ok((0x7F, false)),
        "f17" => Ok((0x80, false)),
        "f18" => Ok((0x81, false)),
        "f19" => Ok((0x82, false)),
        "f20" => Ok((0x83, false)),
        "f21" => Ok((0x84, false)),
        "f22" => Ok((0x85, false)),
        "f23" => Ok((0x86, false)),
        "f24" => Ok((0x87, false)),

        other => {
            if other.len() == 1 {
                let ch = other.chars().next().unwrap();
                if ch.is_ascii_alphabetic() {
                    Ok((ch.to_ascii_uppercase() as u16, false))
                } else if ch.is_ascii_digit() {
                    Ok((ch as u16, false))
                } else {
                    #[cfg(windows)]
                    {
                        unsafe {
                            let scan = windows_sys::Win32::UI::Input::KeyboardAndMouse::VkKeyScanW(
                                ch as u16,
                            );
                            if scan != -1 {
                                let vk = (scan & 0xFF) as u16;
                                return Ok((vk, false));
                            }
                        }
                    }
                    Err(CuaError::Win32Error(format!("Unrecognized key: '{key}'")))
                }
            } else {
                Err(CuaError::Win32Error(format!("Unrecognized key: '{key}'")))
            }
        }
    }
}

#[derive(Debug, Default, Clone)]
pub struct KeyboardController;

impl KeyboardController {
    pub fn new() -> Self {
        Self
    }

    /// Presses or releases an individual virtual key.
    pub fn press_key(
        &self,
        target_hwnd: u64,
        key: &str,
        down: bool,
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();
        validate_window_and_uipi(target_hwnd)?;
        let (vk, is_extended) = parse_virtual_key(key)?;

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            match delivery_mode {
                CuaDeliveryMode::Background => {
                    if let Some(reason) =
                        would_be_silently_dropped(win_hwnd, InputEventKind::Hotkey)
                    {
                        return Err(CuaError::background_unavailable(target_hwnd, reason));
                    }
                    let target = crate::input::win32_msg::find_focused_descendant(win_hwnd);
                    let (msg, lparam) = if down {
                        let mut lp: LPARAM = 1;
                        if is_extended {
                            lp |= 0x01000000;
                        }
                        (WM_KEYDOWN, lp)
                    } else {
                        let mut lp: LPARAM = 0xC0000001_u32 as LPARAM;
                        if is_extended {
                            lp |= 0x01000000;
                        }
                        (WM_KEYUP, lp)
                    };
                    let ok = unsafe { PostMessageW(target, msg, vk as WPARAM, lparam) };
                    if ok == 0 {
                        return Err(CuaError::Win32Error("PostMessageW key event failed".into()));
                    }
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    let mut flags = if down { 0u32 } else { KEYEVENTF_KEYUP };
                    if is_extended {
                        flags |= 0x0001; // KEYEVENTF_EXTENDEDKEY
                    }
                    unsafe {
                        let mut input = [INPUT {
                            r#type: INPUT_KEYBOARD,
                            Anonymous: INPUT_0 {
                                ki: KEYBDINPUT {
                                    wVk: vk,
                                    wScan: 0,
                                    dwFlags: flags,
                                    time: 0,
                                    dwExtraInfo: 0,
                                },
                            },
                        }];
                        SendInput(1, input.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
                    }
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Foreground,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (target_hwnd, key, down, delivery_mode, vk, is_extended);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }

    /// Dispatches a chord of hotkeys (press in order, release in reverse).
    pub fn hotkey(
        &self,
        target_hwnd: u64,
        keys: &[String],
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();
        validate_window_and_uipi(target_hwnd)?;

        if keys.is_empty() {
            return Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            });
        }

        let mut parsed = Vec::with_capacity(keys.len());
        for k in keys {
            parsed.push(parse_virtual_key(k)?);
        }

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            match delivery_mode {
                CuaDeliveryMode::Background => {
                    if let Some(reason) =
                        would_be_silently_dropped(win_hwnd, InputEventKind::Hotkey)
                    {
                        return Err(CuaError::background_unavailable(target_hwnd, reason));
                    }
                    let target = crate::input::win32_msg::find_focused_descendant(win_hwnd);
                    for &(vk, is_ext) in &parsed {
                        let mut lp: LPARAM = 1;
                        if is_ext {
                            lp |= 0x01000000;
                        }
                        let ok = unsafe { PostMessageW(target, WM_KEYDOWN, vk as WPARAM, lp) };
                        if ok == 0 {
                            return Err(CuaError::Win32Error(
                                "PostMessageW hotkey down failed".into(),
                            ));
                        }
                    }
                    std::thread::sleep(Duration::from_millis(15));
                    for &(vk, is_ext) in parsed.iter().rev() {
                        let mut lp: LPARAM = 0xC0000001_u32 as LPARAM;
                        if is_ext {
                            lp |= 0x01000000;
                        }
                        let ok = unsafe { PostMessageW(target, WM_KEYUP, vk as WPARAM, lp) };
                        if ok == 0 {
                            return Err(CuaError::Win32Error(
                                "PostMessageW hotkey up failed".into(),
                            ));
                        }
                    }
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    for &(vk, is_ext) in &parsed {
                        let mut flags = 0u32;
                        if is_ext {
                            flags |= 0x0001;
                        }
                        unsafe {
                            let mut input = [INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: vk,
                                        wScan: 0,
                                        dwFlags: flags,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            }];
                            SendInput(1, input.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
                        }
                    }
                    std::thread::sleep(Duration::from_millis(15));
                    for &(vk, is_ext) in parsed.iter().rev() {
                        let mut flags = KEYEVENTF_KEYUP;
                        if is_ext {
                            flags |= 0x0001;
                        }
                        unsafe {
                            let mut input = [INPUT {
                                r#type: INPUT_KEYBOARD,
                                Anonymous: INPUT_0 {
                                    ki: KEYBDINPUT {
                                        wVk: vk,
                                        wScan: 0,
                                        dwFlags: flags,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            }];
                            SendInput(1, input.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
                        }
                    }
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Foreground,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
            }
        }

        #[cfg(not(windows))]
        {
            let _ = (target_hwnd, keys, delivery_mode, parsed);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}
