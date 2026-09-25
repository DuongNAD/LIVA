//! Synthetic pointer (touch/pen) input injection and activation suppression.
//!
//! Routes input through the Windows system input queue using CreateSyntheticPointerDevice
//! and InjectSyntheticPointerInput. Works across Chromium, Electron, WPF, and WinUI
//! without displacing the hardware cursor.
//!
//! Enforces NoActivateGuard (WS_EX_NOACTIVATE) to prevent window focus theft during injection.

#![allow(non_snake_case, dead_code)]

use crate::types::{CuaError, MouseButton};
use std::time::Duration;

#[cfg(windows)]
use windows_sys::Win32::Foundation::{BOOL, HWND, POINT, RECT};
#[cfg(windows)]
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleA, GetProcAddress};
#[cfg(windows)]
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetAncestor, GetWindowLongPtrW, SetWindowLongPtrW, SetWindowPos, GA_ROOT, GWL_EXSTYLE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, WS_EX_NOACTIVATE,
};

// Synthetic Pointer constants and types
pub type HSYNTHETICPOINTERDEVICE = *mut std::ffi::c_void;

pub const PT_TOUCH: u32 = 0x00000002;
pub const PT_PEN: u32 = 0x00000003;

pub const POINTER_FEEDBACK_DEFAULT: u32 = 0x00000001;
pub const POINTER_FEEDBACK_INDIRECT: u32 = 0x00000002;
pub const POINTER_FEEDBACK_NONE: u32 = 0x00000003;

pub const POINTER_FLAG_NONE: u32 = 0x00000000;
pub const POINTER_FLAG_NEW: u32 = 0x00000001;
pub const POINTER_FLAG_INRANGE: u32 = 0x00000002;
pub const POINTER_FLAG_INCONTACT: u32 = 0x00000004;
pub const POINTER_FLAG_FIRSTBUTTON: u32 = 0x00000010;
pub const POINTER_FLAG_SECONDBUTTON: u32 = 0x00000020;
pub const POINTER_FLAG_DOWN: u32 = 0x00010000;
pub const POINTER_FLAG_UPDATE: u32 = 0x00020000;
pub const POINTER_FLAG_UP: u32 = 0x00040000;

pub const TOUCH_MASK_CONTACTAREA: u32 = 0x00000001;
pub const TOUCH_MASK_ORIENTATION: u32 = 0x00000002;
pub const TOUCH_MASK_PRESSURE: u32 = 0x00000004;

pub const TOUCH_FLAG_NONE: u32 = 0x00000000;

pub const PEN_FLAG_NONE: u32 = 0x00000000;
pub const PEN_FLAG_BARREL: u32 = 0x00000001; // Barrel button indicates right click
pub const PEN_FLAG_INVERTED: u32 = 0x00000002;
pub const PEN_FLAG_ERASER: u32 = 0x00000004;

pub const PEN_MASK_PRESSURE: u32 = 0x00000001;
pub const PEN_MASK_ROTATION: u32 = 0x00000002;
pub const PEN_MASK_TILT_X: u32 = 0x00000004;
pub const PEN_MASK_TILT_Y: u32 = 0x00000008;

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct POINTER_INFO {
    pub pointerType: u32,
    pub pointerId: u32,
    pub frameId: u32,
    pub pointerFlags: u32,
    pub sourceDevice: *mut std::ffi::c_void,
    pub hwndTarget: HWND,
    pub ptPixelLocation: POINT,
    pub ptHimetricLocation: POINT,
    pub ptPixelLocationRaw: POINT,
    pub ptHimetricLocationRaw: POINT,
    pub dwTime: u32,
    pub historyCount: u32,
    pub InputData: i32,
    pub KeyStates: u32,
    pub PerformanceCount: u64,
    pub ButtonChangeType: i32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct POINTER_TOUCH_INFO {
    pub pointerInfo: POINTER_INFO,
    pub touchFlags: u32,
    pub touchMask: u32,
    pub rcContact: RECT,
    pub rcContactRaw: RECT,
    pub orientation: u32,
    pub pressure: u32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct POINTER_PEN_INFO {
    pub pointerInfo: POINTER_INFO,
    pub penFlags: u32,
    pub penMask: u32,
    pub pressure: u32,
    pub rotation: u32,
    pub tiltX: i32,
    pub tiltY: i32,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
pub union POINTER_TYPE_INFO_UNION {
    pub touchInfo: POINTER_TOUCH_INFO,
    pub penInfo: POINTER_PEN_INFO,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Copy, Clone)]
pub struct POINTER_TYPE_INFO {
    pub type_: u32,
    pub info: POINTER_TYPE_INFO_UNION,
}

#[cfg(windows)]
type FnCreateSyntheticPointerDevice =
    unsafe extern "system" fn(u32, u32, u32) -> HSYNTHETICPOINTERDEVICE;
#[cfg(windows)]
type FnInjectSyntheticPointerInput =
    unsafe extern "system" fn(HSYNTHETICPOINTERDEVICE, *const POINTER_TYPE_INFO, u32) -> BOOL;
#[cfg(windows)]
type FnDestroySyntheticPointerDevice = unsafe extern "system" fn(HSYNTHETICPOINTERDEVICE);

#[cfg(windows)]
struct SyntheticPointerFns {
    create_device: FnCreateSyntheticPointerDevice,
    inject_input: FnInjectSyntheticPointerInput,
    destroy_device: FnDestroySyntheticPointerDevice,
}

#[cfg(windows)]
#[allow(clippy::missing_transmute_annotations)]
fn load_synthetic_pointer_fns() -> Option<SyntheticPointerFns> {
    unsafe {
        let user32 = GetModuleHandleA(c"user32.dll".as_ptr() as *const u8);
        if user32 == 0 {
            return None;
        }

        let create_ptr = GetProcAddress(
            user32,
            c"CreateSyntheticPointerDevice".as_ptr() as *const u8,
        );
        let inject_ptr =
            GetProcAddress(user32, c"InjectSyntheticPointerInput".as_ptr() as *const u8);
        let destroy_ptr = GetProcAddress(
            user32,
            c"DestroySyntheticPointerDevice".as_ptr() as *const u8,
        );

        if create_ptr.is_none() || inject_ptr.is_none() || destroy_ptr.is_none() {
            return None;
        }

        Some(SyntheticPointerFns {
            create_device: std::mem::transmute::<_, FnCreateSyntheticPointerDevice>(
                create_ptr.unwrap(),
            ),
            inject_input: std::mem::transmute::<_, FnInjectSyntheticPointerInput>(
                inject_ptr.unwrap(),
            ),
            destroy_device: std::mem::transmute::<_, FnDestroySyntheticPointerDevice>(
                destroy_ptr.unwrap(),
            ),
        })
    }
}

/// RAII Guard that asserts WS_EX_NOACTIVATE on the target root window during synthetic tap.
///
/// Prevents Windows from activating or foregrounding the target window when touch input arrives.
#[cfg(windows)]
pub struct NoActivateGuard {
    root_hwnd: HWND,
    original_ex_style: isize,
    is_applied: bool,
}

#[cfg(windows)]
impl NoActivateGuard {
    pub fn new(target: HWND) -> Result<Self, CuaError> {
        unsafe {
            let root = GetAncestor(target, GA_ROOT);
            let effective_hwnd = if root != 0 { root } else { target };

            let ex_style = GetWindowLongPtrW(effective_hwnd, GWL_EXSTYLE);
            let has_noactivate = (ex_style & (WS_EX_NOACTIVATE as isize)) != 0;

            if !has_noactivate {
                let new_style = ex_style | (WS_EX_NOACTIVATE as isize);
                SetWindowLongPtrW(effective_hwnd, GWL_EXSTYLE, new_style);
                SetWindowPos(
                    effective_hwnd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
                );
                Ok(Self {
                    root_hwnd: effective_hwnd,
                    original_ex_style: ex_style,
                    is_applied: true,
                })
            } else {
                Ok(Self {
                    root_hwnd: effective_hwnd,
                    original_ex_style: ex_style,
                    is_applied: false,
                })
            }
        }
    }
}

#[cfg(windows)]
impl Drop for NoActivateGuard {
    fn drop(&mut self) {
        if self.is_applied {
            unsafe {
                SetWindowLongPtrW(self.root_hwnd, GWL_EXSTYLE, self.original_ex_style);
                SetWindowPos(
                    self.root_hwnd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_FRAMECHANGED | SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// Wrapper around a Windows Synthetic Pointer Device handle.
#[cfg(windows)]
pub struct SyntheticPointerDevice {
    handle: HSYNTHETICPOINTERDEVICE,
    device_type: u32,
    fns: SyntheticPointerFns,
}

#[cfg(windows)]
impl SyntheticPointerDevice {
    /// Creates a touch pointer device supporting up to `max_count` simultaneous contacts.
    pub fn new_touch(max_count: u32) -> Result<Self, CuaError> {
        let fns = load_synthetic_pointer_fns().ok_or_else(|| {
            CuaError::Win32Error("Synthetic pointer API not available in user32.dll".into())
        })?;

        unsafe {
            let h = (fns.create_device)(PT_TOUCH, max_count, POINTER_FEEDBACK_INDIRECT);
            if h.is_null() {
                return Err(CuaError::Win32Error(
                    "CreateSyntheticPointerDevice(PT_TOUCH) failed".into(),
                ));
            }
            Ok(Self {
                handle: h,
                device_type: PT_TOUCH,
                fns,
            })
        }
    }

    /// Creates a pen pointer device (used for right clicks with barrel button).
    pub fn new_pen() -> Result<Self, CuaError> {
        let fns = load_synthetic_pointer_fns().ok_or_else(|| {
            CuaError::Win32Error("Synthetic pointer API not available in user32.dll".into())
        })?;

        unsafe {
            let h = (fns.create_device)(PT_PEN, 1, POINTER_FEEDBACK_INDIRECT);
            if h.is_null() {
                return Err(CuaError::Win32Error(
                    "CreateSyntheticPointerDevice(PT_PEN) failed".into(),
                ));
            }
            Ok(Self {
                handle: h,
                device_type: PT_PEN,
                fns,
            })
        }
    }

    /// Injects a touch tap sequence (down -> delay -> up) at physical screen coordinates.
    pub fn inject_touch_tap(&self, screen_x: i32, screen_y: i32) -> Result<(), CuaError> {
        unsafe {
            let pt = POINT {
                x: screen_x,
                y: screen_y,
            };
            let rc_contact = RECT {
                left: screen_x - 2,
                top: screen_y - 2,
                right: screen_x + 2,
                bottom: screen_y + 2,
            };

            // Touch Down
            let mut down_info: POINTER_TYPE_INFO = std::mem::zeroed();
            down_info.type_ = PT_TOUCH;
            down_info.info.touchInfo = POINTER_TOUCH_INFO {
                pointerInfo: POINTER_INFO {
                    pointerType: PT_TOUCH,
                    pointerId: 1,
                    frameId: 0,
                    pointerFlags: POINTER_FLAG_DOWN
                        | POINTER_FLAG_INRANGE
                        | POINTER_FLAG_INCONTACT
                        | POINTER_FLAG_FIRSTBUTTON,
                    sourceDevice: std::ptr::null_mut(),
                    hwndTarget: 0,
                    ptPixelLocation: pt,
                    ptHimetricLocation: POINT { x: 0, y: 0 },
                    ptPixelLocationRaw: pt,
                    ptHimetricLocationRaw: POINT { x: 0, y: 0 },
                    dwTime: 0,
                    historyCount: 0,
                    InputData: 0,
                    KeyStates: 0,
                    PerformanceCount: 0,
                    ButtonChangeType: 0,
                },
                touchFlags: TOUCH_FLAG_NONE,
                touchMask: TOUCH_MASK_CONTACTAREA | TOUCH_MASK_PRESSURE,
                rcContact: rc_contact,
                rcContactRaw: rc_contact,
                orientation: 90,
                pressure: 512,
            };

            let ok = (self.fns.inject_input)(self.handle, &down_info, 1);
            if ok == 0 {
                return Err(CuaError::Win32Error(
                    "InjectSyntheticPointerInput (Touch Down) failed".into(),
                ));
            }

            std::thread::sleep(Duration::from_millis(20));

            // Touch Up
            let mut up_info = down_info;
            up_info.info.touchInfo.pointerInfo.pointerFlags = POINTER_FLAG_UP;
            up_info.info.touchInfo.pressure = 0;

            let ok = (self.fns.inject_input)(self.handle, &up_info, 1);
            if ok == 0 {
                return Err(CuaError::Win32Error(
                    "InjectSyntheticPointerInput (Touch Up) failed".into(),
                ));
            }

            Ok(())
        }
    }

    /// Injects a pen barrel tap (translating natively to right click in Chromium/WPF).
    pub fn inject_pen_barrel_tap(&self, screen_x: i32, screen_y: i32) -> Result<(), CuaError> {
        unsafe {
            let pt = POINT {
                x: screen_x,
                y: screen_y,
            };

            // Pen Down with BARREL flag
            let mut down_info: POINTER_TYPE_INFO = std::mem::zeroed();
            down_info.type_ = PT_PEN;
            down_info.info.penInfo = POINTER_PEN_INFO {
                pointerInfo: POINTER_INFO {
                    pointerType: PT_PEN,
                    pointerId: 1,
                    frameId: 0,
                    pointerFlags: POINTER_FLAG_DOWN
                        | POINTER_FLAG_INRANGE
                        | POINTER_FLAG_INCONTACT
                        | POINTER_FLAG_SECONDBUTTON,
                    sourceDevice: std::ptr::null_mut(),
                    hwndTarget: 0,
                    ptPixelLocation: pt,
                    ptHimetricLocation: POINT { x: 0, y: 0 },
                    ptPixelLocationRaw: pt,
                    ptHimetricLocationRaw: POINT { x: 0, y: 0 },
                    dwTime: 0,
                    historyCount: 0,
                    InputData: 0,
                    KeyStates: 0,
                    PerformanceCount: 0,
                    ButtonChangeType: 0,
                },
                penFlags: PEN_FLAG_BARREL,
                penMask: PEN_MASK_PRESSURE,
                pressure: 512,
                rotation: 0,
                tiltX: 0,
                tiltY: 0,
            };

            let ok = (self.fns.inject_input)(self.handle, &down_info, 1);
            if ok == 0 {
                return Err(CuaError::Win32Error(
                    "InjectSyntheticPointerInput (Pen Down Barrel) failed".into(),
                ));
            }

            std::thread::sleep(Duration::from_millis(20));

            // Pen Up
            let mut up_info = down_info;
            up_info.info.penInfo.pointerInfo.pointerFlags = POINTER_FLAG_UP;
            up_info.info.penInfo.penFlags = PEN_FLAG_NONE;
            up_info.info.penInfo.pressure = 0;

            let ok = (self.fns.inject_input)(self.handle, &up_info, 1);
            if ok == 0 {
                return Err(CuaError::Win32Error(
                    "InjectSyntheticPointerInput (Pen Up Barrel) failed".into(),
                ));
            }

            Ok(())
        }
    }
}

#[cfg(windows)]
impl Drop for SyntheticPointerDevice {
    fn drop(&mut self) {
        unsafe {
            if !self.handle.is_null() {
                (self.fns.destroy_device)(self.handle);
            }
        }
    }
}

/// Executes a background click using synthetic pointer injection guarded by WS_EX_NOACTIVATE.
#[cfg(windows)]
pub fn background_pointer_click(
    target_hwnd: HWND,
    screen_x: i32,
    screen_y: i32,
    button: MouseButton,
) -> Result<(), CuaError> {
    let _guard = NoActivateGuard::new(target_hwnd)?;

    match button {
        MouseButton::Left => {
            let device = SyntheticPointerDevice::new_touch(1)?;
            device.inject_touch_tap(screen_x, screen_y)?;
        }
        MouseButton::Right => {
            let device = SyntheticPointerDevice::new_pen()?;
            device.inject_pen_barrel_tap(screen_x, screen_y)?;
        }
        MouseButton::Middle => {
            return crate::input::win32_msg::post_click(
                target_hwnd,
                screen_x,
                screen_y,
                MouseButton::Middle,
            );
        }
    }

    Ok(())
}

#[cfg(not(windows))]
pub fn background_pointer_click(
    _target_hwnd: u64,
    _screen_x: i32,
    _screen_y: i32,
    _button: MouseButton,
) -> Result<(), CuaError> {
    Ok(())
}
