//! Input Actuation Subsystem & Delivery Escalation Router.
//!
//! Provides a unified input dispatch facade supporting:
//! - DeliveryMode::Background (Default, non-intrusive, zero focus/cursor disturbance)
//! - DeliveryMode::Foreground (Explicit escalation using AttachThreadInput + SendInput)

pub mod pointer;
pub mod uipi;
pub mod win32_msg;

use crate::types::{CuaDeliveryMode, CuaError, MouseButton};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{FALSE, HWND, TRUE};
#[cfg(windows)]
use windows_sys::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, INPUT_MOUSE, KEYBDINPUT, KEYEVENTF_KEYUP,
    MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP,
    MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP, MOUSEINPUT,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId, IsIconic,
    IsWindow, SetCursorPos, SetForegroundWindow, ShowWindow, SW_RESTORE,
};

use crate::kill_switch::CancellationToken;

pub use pointer::*;
pub use uipi::*;
pub use win32_msg::*;

/// Classification of input events for silent-drop detection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InputEventKind {
    Click,
    DoubleClick,
    Text,
    Hotkey,
    Scroll,
}

impl InputEventKind {
    pub fn is_keyboard(&self) -> bool {
        matches!(self, Self::Text | Self::Hotkey)
    }
}

/// Result returned from an input actuation dispatch.
#[derive(Debug, Clone)]
pub struct ActuationResult {
    pub success: bool,
    pub delivery_used: CuaDeliveryMode,
    pub duration_ms: u64,
}

/// Identifies if a window class is known to silently drop background events.
#[cfg(windows)]
pub fn would_be_silently_dropped(target_hwnd: HWND, event: InputEventKind) -> Option<&'static str> {
    unsafe {
        let mut class_buf = [0u16; 256];
        let len = GetClassNameW(target_hwnd, class_buf.as_mut_ptr(), 256);
        if len > 0 {
            let class_name = String::from_utf16_lossy(&class_buf[..len as usize]);
            match class_name.as_str() {
                "Chrome_RenderWidgetHostHWND" if event.is_keyboard() => {
                    Some("Chromium web content drops background WM_CHAR / WM_KEYDOWN")
                }
                "gdkWindowToplevel" => {
                    Some("GTK top-level windows ignore Win32 PostMessage client events")
                }
                "DirectUIHWND" if event.is_keyboard() => {
                    Some("DirectUI dialogs drop raw WM_CHAR without UI Automation")
                }
                _ => None,
            }
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub fn would_be_silently_dropped(
    _target_hwnd: u64,
    _event: InputEventKind,
) -> Option<&'static str> {
    None
}

/// Brings the target window to the foreground using AttachThreadInput to bypass focus stealing limits.
#[cfg(windows)]
pub fn force_foreground_attached(target: HWND) -> Result<(), CuaError> {
    unsafe {
        let fg = GetForegroundWindow();
        if fg == target {
            return Ok(());
        }

        let cur_tid = GetCurrentThreadId();
        let mut target_pid = 0u32;
        let target_tid = GetWindowThreadProcessId(target, &mut target_pid);

        let mut fg_pid = 0u32;
        let fg_tid = if fg != 0 {
            GetWindowThreadProcessId(fg, &mut fg_pid)
        } else {
            0
        };

        if fg_tid != 0 && fg_tid != cur_tid {
            AttachThreadInput(cur_tid, fg_tid, TRUE);
        }
        if target_tid != 0 && target_tid != cur_tid {
            AttachThreadInput(cur_tid, target_tid, TRUE);
        }

        ShowWindow(target, SW_RESTORE);
        BringWindowToTop(target);
        SetForegroundWindow(target);

        if fg_tid != 0 && fg_tid != cur_tid {
            AttachThreadInput(cur_tid, fg_tid, FALSE);
        }
        if target_tid != 0 && target_tid != cur_tid {
            AttachThreadInput(cur_tid, target_tid, FALSE);
        }

        std::thread::sleep(Duration::from_millis(50));
        Ok(())
    }
}

#[cfg(not(windows))]
pub fn force_foreground_attached(_target: u64) -> Result<(), CuaError> {
    Ok(())
}

/// Unified Input Actuator Router.
pub struct InputActuator;

impl InputActuator {
    /// Dispatches a mouse click event using the selected delivery mode.
    pub fn dispatch_click(
        target_hwnd: u64,
        screen_x: i32,
        screen_y: i32,
        button: MouseButton,
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            unsafe {
                if IsWindow(win_hwnd) == 0 {
                    return Err(CuaError::WindowNotFound(target_hwnd));
                }
                if IsIconic(win_hwnd) != 0 {
                    return Err(CuaError::WindowMinimized(target_hwnd));
                }
            }

            // Check UIPI restrictions
            let uipi = check_uipi_restriction(target_hwnd)?;
            if uipi.is_blocked {
                return Err(CuaError::UipiBlocked(
                    uipi.refusal_message
                        .unwrap_or_else(|| "UIPI restricted".into()),
                ));
            }

            match delivery_mode {
                CuaDeliveryMode::Background => {
                    // Tier 2: Universal Synthetic Pointer Injection (guarded by WS_EX_NOACTIVATE)
                    let res = background_pointer_click(win_hwnd, screen_x, screen_y, button);
                    if res.is_ok() {
                        return Ok(ActuationResult {
                            success: true,
                            delivery_used: CuaDeliveryMode::Background,
                            duration_ms: start.elapsed().as_millis() as u64,
                        });
                    }

                    // Tier 3: Deepest-child PostMessage fallback
                    post_click(win_hwnd, screen_x, screen_y, button)?;

                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;

                    unsafe {
                        SetCursorPos(screen_x, screen_y);
                    }

                    let (down_flag, up_flag) = match button {
                        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                    };

                    unsafe {
                        let mut inputs = [
                            INPUT {
                                r#type: INPUT_MOUSE,
                                Anonymous: INPUT_0 {
                                    mi: MOUSEINPUT {
                                        dx: 0,
                                        dy: 0,
                                        mouseData: 0,
                                        dwFlags: down_flag,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            },
                            INPUT {
                                r#type: INPUT_MOUSE,
                                Anonymous: INPUT_0 {
                                    mi: MOUSEINPUT {
                                        dx: 0,
                                        dy: 0,
                                        mouseData: 0,
                                        dwFlags: up_flag,
                                        time: 0,
                                        dwExtraInfo: 0,
                                    },
                                },
                            },
                        ];
                        SendInput(2, inputs.as_mut_ptr(), std::mem::size_of::<INPUT>() as i32);
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
            let _ = (target_hwnd, screen_x, screen_y, button, delivery_mode);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }

    /// Dispatches a double-click event using the selected delivery mode.
    pub fn dispatch_double_click(
        target_hwnd: u64,
        screen_x: i32,
        screen_y: i32,
        button: MouseButton,
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            unsafe {
                if IsWindow(win_hwnd) == 0 {
                    return Err(CuaError::WindowNotFound(target_hwnd));
                }
                if IsIconic(win_hwnd) != 0 {
                    return Err(CuaError::WindowMinimized(target_hwnd));
                }
            }

            let uipi = check_uipi_restriction(target_hwnd)?;
            if uipi.is_blocked {
                return Err(CuaError::UipiBlocked(
                    uipi.refusal_message
                        .unwrap_or_else(|| "UIPI restricted".into()),
                ));
            }

            match delivery_mode {
                CuaDeliveryMode::Background => {
                    post_double_click(win_hwnd, screen_x, screen_y, button)?;
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    Self::dispatch_click(
                        target_hwnd,
                        screen_x,
                        screen_y,
                        button,
                        CuaDeliveryMode::Foreground,
                    )?;
                    std::thread::sleep(Duration::from_millis(30));
                    Self::dispatch_click(
                        target_hwnd,
                        screen_x,
                        screen_y,
                        button,
                        CuaDeliveryMode::Foreground,
                    )?;
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
            let _ = (target_hwnd, screen_x, screen_y, button, delivery_mode);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }

    /// Dispatches text entry using the selected delivery mode with mid-flight cancellation.
    pub async fn dispatch_type_text(
        target_hwnd: u64,
        text: &str,
        delivery_mode: CuaDeliveryMode,
        cancel_token: Option<&CancellationToken>,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            unsafe {
                if IsWindow(win_hwnd) == 0 {
                    return Err(CuaError::WindowNotFound(target_hwnd));
                }
                if IsIconic(win_hwnd) != 0 {
                    return Err(CuaError::WindowMinimized(target_hwnd));
                }
            }

            let uipi = check_uipi_restriction(target_hwnd)?;
            if uipi.is_blocked {
                return Err(CuaError::UipiBlocked(
                    uipi.refusal_message
                        .unwrap_or_else(|| "UIPI restricted".into()),
                ));
            }

            match delivery_mode {
                CuaDeliveryMode::Background => {
                    if let Some(reason) = would_be_silently_dropped(win_hwnd, InputEventKind::Text)
                    {
                        return Err(CuaError::background_unavailable(target_hwnd, reason));
                    }
                    post_text(win_hwnd, text, cancel_token).await?;
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    for ch in text.chars() {
                        // 1. Immediate cancellation check prior to keystroke emission
                        if let Some(token) = cancel_token {
                            if token.is_cancelled() {
                                return Err(CuaError::EmergencyHalted);
                            }
                        }

                        let mut utf16_buf = [0u16; 2];
                        for &code_unit in ch.encode_utf16(&mut utf16_buf).iter() {
                            unsafe {
                                let (vk, scan, flags) = if ch == '\n' || ch == '\r' {
                                    (0x0D, 0x1C, 0u32) // VK_RETURN standard scancode
                                } else {
                                    (0, code_unit, 0x0004u32) // KEYEVENTF_UNICODE
                                };
                                let mut inputs = [
                                    INPUT {
                                        r#type: INPUT_KEYBOARD,
                                        Anonymous: INPUT_0 {
                                            ki: KEYBDINPUT {
                                                wVk: vk,
                                                wScan: scan,
                                                dwFlags: flags,
                                                time: 0,
                                                dwExtraInfo: 0,
                                            },
                                        },
                                    },
                                    INPUT {
                                        r#type: INPUT_KEYBOARD,
                                        Anonymous: INPUT_0 {
                                            ki: KEYBDINPUT {
                                                wVk: vk,
                                                wScan: scan,
                                                dwFlags: flags | KEYEVENTF_KEYUP,
                                                time: 0,
                                                dwExtraInfo: 0,
                                            },
                                        },
                                    },
                                ];
                                SendInput(
                                    2,
                                    inputs.as_mut_ptr(),
                                    std::mem::size_of::<INPUT>() as i32,
                                );
                            }
                        }

                        // 2. Cooperative async sleep with instant cancellation wake-up (< 1ms)
                        if let Some(token) = cancel_token {
                            tokio::select! {
                                _ = tokio::time::sleep(Duration::from_millis(5)) => {}
                                _ = token.cancelled() => {
                                    return Err(CuaError::EmergencyHalted);
                                }
                            }
                        } else {
                            tokio::time::sleep(Duration::from_millis(5)).await;
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
            let _ = (target_hwnd, text, delivery_mode, cancel_token);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}
