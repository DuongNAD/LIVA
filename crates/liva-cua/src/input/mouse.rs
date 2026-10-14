//! Mouse actuation controller.

use crate::input::{force_foreground_attached, validate_window_and_uipi, ActuationResult};
use crate::types::{CuaDeliveryMode, CuaError, MouseButton};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{HWND, POINT};
#[cfg(windows)]
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_MOUSE, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP,
    MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP, MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP,
    MOUSEINPUT,
};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::SetCursorPos;

#[derive(Debug, Default, Clone)]
pub struct MouseController;

impl MouseController {
    pub fn new() -> Self {
        Self
    }

    /// Moves the cursor within or to target window coordinates.
    pub fn move_cursor(
        &self,
        target_hwnd: u64,
        x: i32,
        y: i32,
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();
        validate_window_and_uipi(target_hwnd)?;

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            match delivery_mode {
                CuaDeliveryMode::Background => {
                    let screen_pt = POINT { x, y };
                    let (leaf_hwnd, client_pt) =
                        crate::input::win32_msg::find_deepest_child_at(win_hwnd, screen_pt);
                    crate::input::win32_msg::post_mouse_move(leaf_hwnd, client_pt.x, client_pt.y)?;
                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    unsafe {
                        SetCursorPos(x, y);
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
            let _ = (target_hwnd, x, y, delivery_mode);
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }

    /// Performs a mouse drag from start coordinates to end coordinates.
    #[allow(clippy::too_many_arguments)]
    pub fn drag(
        &self,
        target_hwnd: u64,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
        button: MouseButton,
        steps: u32,
        delivery_mode: CuaDeliveryMode,
    ) -> Result<ActuationResult, CuaError> {
        let start = std::time::Instant::now();
        validate_window_and_uipi(target_hwnd)?;

        let num_steps = steps.max(1);

        #[cfg(windows)]
        {
            let win_hwnd = target_hwnd as usize as HWND;
            match delivery_mode {
                CuaDeliveryMode::Background => {
                    let start_pt = POINT {
                        x: start_x,
                        y: start_y,
                    };
                    let (leaf_start, client_start) =
                        crate::input::win32_msg::find_deepest_child_at(win_hwnd, start_pt);
                    crate::input::win32_msg::post_mouse_move(
                        leaf_start,
                        client_start.x,
                        client_start.y,
                    )?;
                    crate::input::win32_msg::post_mouse_down(
                        leaf_start,
                        client_start.x,
                        client_start.y,
                        button,
                    )?;
                    std::thread::sleep(Duration::from_millis(10));

                    for s in 1..num_steps {
                        let cur_x = start_x + ((end_x - start_x) * s as i32) / num_steps as i32;
                        let cur_y = start_y + ((end_y - start_y) * s as i32) / num_steps as i32;
                        let cur_pt = POINT { x: cur_x, y: cur_y };
                        let (leaf_cur, client_cur) =
                            crate::input::win32_msg::find_deepest_child_at(win_hwnd, cur_pt);
                        let _ = crate::input::win32_msg::post_mouse_move(
                            leaf_cur,
                            client_cur.x,
                            client_cur.y,
                        );
                        std::thread::sleep(Duration::from_millis(5));
                    }

                    let end_pt = POINT { x: end_x, y: end_y };
                    let (leaf_end, client_end) =
                        crate::input::win32_msg::find_deepest_child_at(win_hwnd, end_pt);
                    crate::input::win32_msg::post_mouse_move(leaf_end, client_end.x, client_end.y)?;
                    std::thread::sleep(Duration::from_millis(10));
                    crate::input::win32_msg::post_mouse_up(
                        leaf_end,
                        client_end.x,
                        client_end.y,
                        button,
                    )?;

                    Ok(ActuationResult {
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        duration_ms: start.elapsed().as_millis() as u64,
                    })
                }
                CuaDeliveryMode::Foreground => {
                    force_foreground_attached(win_hwnd)?;
                    unsafe {
                        SetCursorPos(start_x, start_y);
                    }

                    let (down_flag, up_flag) = match button {
                        MouseButton::Left => (MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP),
                        MouseButton::Right => (MOUSEEVENTF_RIGHTDOWN, MOUSEEVENTF_RIGHTUP),
                        MouseButton::Middle => (MOUSEEVENTF_MIDDLEDOWN, MOUSEEVENTF_MIDDLEUP),
                    };

                    unsafe {
                        let mut down_input = [INPUT {
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
                        }];
                        SendInput(
                            1,
                            down_input.as_mut_ptr(),
                            std::mem::size_of::<INPUT>() as i32,
                        );
                    }
                    std::thread::sleep(Duration::from_millis(10));

                    for s in 1..num_steps {
                        let cur_x = start_x + ((end_x - start_x) * s as i32) / num_steps as i32;
                        let cur_y = start_y + ((end_y - start_y) * s as i32) / num_steps as i32;
                        unsafe {
                            SetCursorPos(cur_x, cur_y);
                        }
                        std::thread::sleep(Duration::from_millis(5));
                    }

                    unsafe {
                        SetCursorPos(end_x, end_y);
                    }
                    std::thread::sleep(Duration::from_millis(10));

                    unsafe {
                        let mut up_input = [INPUT {
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
                        }];
                        SendInput(
                            1,
                            up_input.as_mut_ptr(),
                            std::mem::size_of::<INPUT>() as i32,
                        );
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
            let _ = (
                target_hwnd,
                start_x,
                start_y,
                end_x,
                end_y,
                button,
                steps,
                delivery_mode,
            );
            Ok(ActuationResult {
                success: true,
                delivery_used: delivery_mode,
                duration_ms: start.elapsed().as_millis() as u64,
            })
        }
    }
}
