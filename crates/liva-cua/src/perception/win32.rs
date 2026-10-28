//! Native Win32 child control traversal using EnumChildWindows.

use super::indexer::SoMIndexer;
use super::types::{UiElement, UiRole, UiState, UiTree};
use crate::types::{CuaError, CuaRect};

#[cfg(windows)]
pub fn extract_win32_ui_tree(root_hwnd: u64) -> Result<UiTree, CuaError> {
    use windows_sys::Win32::Foundation::{BOOL, HWND, LPARAM, RECT};
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        EnumChildWindows, GetClassNameW, GetWindowLongW, GetWindowRect, GetWindowTextW,
        IsWindowVisible, GWL_STYLE, WS_DISABLED,
    };

    let hwnd = root_hwnd as usize as HWND;
    if hwnd == 0 {
        return Ok(UiTree::empty(root_hwnd));
    }

    let mut root_rect = RECT {
        left: 0,
        top: 0,
        right: 0,
        bottom: 0,
    };
    let bounds = unsafe {
        if GetWindowRect(hwnd, &mut root_rect) != 0 {
            CuaRect::new(
                root_rect.left,
                root_rect.top,
                root_rect.right - root_rect.left,
                root_rect.bottom - root_rect.top,
            )
        } else {
            CuaRect::default()
        }
    };

    struct EnumContext {
        elements: Vec<UiElement>,
    }

    let mut ctx = EnumContext {
        elements: Vec::new(),
    };

    unsafe extern "system" fn enum_child_proc(child: HWND, lparam: LPARAM) -> BOOL {
        let ctx = &mut *(lparam as *mut EnumContext);

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(child, &mut rect) == 0 {
            return 1; // Continue enumeration
        }

        let w = rect.right - rect.left;
        let h = rect.bottom - rect.top;
        if w <= 0 || h <= 0 {
            return 1;
        }

        let style = GetWindowLongW(child, GWL_STYLE) as u32;
        let is_visible = IsWindowVisible(child) != 0;
        let is_enabled = (style & WS_DISABLED) == 0;

        let mut class_buf = [0u16; 256];
        let class_len = GetClassNameW(child, class_buf.as_mut_ptr(), class_buf.len() as i32);
        let class_name = if class_len > 0 {
            String::from_utf16_lossy(&class_buf[..class_len as usize])
        } else {
            String::new()
        };

        let mut text_buf = [0u16; 512];
        let text_len = GetWindowTextW(child, text_buf.as_mut_ptr(), text_buf.len() as i32);
        let text = if text_len > 0 {
            String::from_utf16_lossy(&text_buf[..text_len as usize])
        } else {
            String::new()
        };

        let role = match class_name.to_lowercase().as_str() {
            "button" => {
                const BS_CHECKBOX: u32 = 0x00000002;
                const BS_AUTOCHECKBOX: u32 = 0x00000003;
                const BS_RADIOBUTTON: u32 = 0x00000004;
                const BS_AUTORADIOBUTTON: u32 = 0x00000009;
                const BS_GROUPBOX: u32 = 0x00000007;

                let btn_type = style & 0x0000000F;
                if btn_type == BS_CHECKBOX || btn_type == BS_AUTOCHECKBOX {
                    UiRole::CheckBox
                } else if btn_type == BS_RADIOBUTTON || btn_type == BS_AUTORADIOBUTTON {
                    UiRole::RadioButton
                } else if btn_type == BS_GROUPBOX {
                    UiRole::Group
                } else {
                    UiRole::Button
                }
            }
            "edit" => UiRole::Edit,
            "static" => UiRole::Text,
            "combobox" => UiRole::ComboBox,
            "listbox" => UiRole::ListBox,
            "scrollbar" => UiRole::ScrollBar,
            "msctls_trackbar32" => UiRole::Slider,
            "msctls_progress32" => UiRole::ProgressBar,
            "systabcontrol32" => UiRole::TabItem,
            "syslistview32" => UiRole::ListBox,
            _ => UiRole::Custom,
        };

        let bounds = CuaRect::new(rect.left, rect.top, w, h);
        let id = format!("win32_0x{:X}", child as usize);

        let elem = UiElement::new(id, role, text, bounds).with_state(UiState {
            is_visible,
            is_enabled,
            is_clickable: role.is_interactive_by_default(),
            ..Default::default()
        });

        ctx.elements.push(elem);
        1 // Continue enumeration
    }

    unsafe {
        EnumChildWindows(
            hwnd,
            Some(enum_child_proc),
            &mut ctx as *mut EnumContext as LPARAM,
        );
    }

    let root_elem = UiElement::new(
        format!("win32_root_0x{:X}", root_hwnd),
        UiRole::Window,
        "Native Win32 Window",
        bounds,
    )
    .with_children(ctx.elements);

    let mut tree = UiTree::new(root_hwnd, root_elem);
    SoMIndexer::index_tree(&mut tree);
    Ok(tree)
}

#[cfg(not(windows))]
pub fn extract_win32_ui_tree(root_hwnd: u64) -> Result<UiTree, CuaError> {
    Ok(UiTree::empty(root_hwnd))
}
