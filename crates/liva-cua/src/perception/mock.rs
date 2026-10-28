//! Headless mock fixtures for perception, SoM indexing, and visual fallback testing.

use super::indexer::SoMIndexer;
use super::types::{UiElement, UiRole, UiState, UiTree};
use crate::types::CuaRect;

/// Headless fixtures generating synthetic UI trees for perception testing.
pub struct PerceptionMockFixtures;

impl PerceptionMockFixtures {
    /// Creates a standard dialog window containing a title, a prompt message,
    /// a text edit field, and Cancel/OK buttons.
    pub fn standard_dialog(hwnd: u64, bounds: CuaRect) -> UiTree {
        let x = bounds.x;
        let y = bounds.y;

        let title_bar = UiElement::new(
            "title_bar",
            UiRole::Text,
            "Confirm Action",
            CuaRect::new(x + 10, y + 10, 200, 20),
        )
        .with_state(UiState::static_display());

        let prompt_text = UiElement::new(
            "prompt_msg",
            UiRole::Text,
            "Are you sure you want to proceed with this operation?",
            CuaRect::new(x + 20, y + 40, 360, 40),
        )
        .with_state(UiState::static_display());

        let edit_input = UiElement::new(
            "input_notes",
            UiRole::Edit,
            "Optional Notes",
            CuaRect::new(x + 20, y + 90, 360, 30),
        )
        .with_state(UiState::editable())
        .with_value("Pre-filled notes");

        let chk_remember = UiElement::new(
            "chk_remember",
            UiRole::CheckBox,
            "Do not ask again",
            CuaRect::new(x + 20, y + 130, 160, 24),
        )
        .with_state(UiState::checkable(false));

        let btn_cancel = UiElement::new(
            "btn_cancel",
            UiRole::Button,
            "Cancel",
            CuaRect::new(x + 210, y + 180, 80, 32),
        )
        .with_state(UiState::clickable());

        let btn_ok = UiElement::new(
            "btn_ok",
            UiRole::Button,
            "OK",
            CuaRect::new(x + 300, y + 180, 80, 32),
        )
        .with_state(UiState::clickable());

        let root = UiElement::new("dlg_root", UiRole::Dialog, "Confirm Dialog", bounds)
            .with_children(vec![
                title_bar,
                prompt_text,
                edit_input,
                chk_remember,
                btn_cancel,
                btn_ok,
            ]);

        let mut tree = UiTree::new(hwnd, root);
        SoMIndexer::index_tree(&mut tree);
        tree
    }

    /// Creates an empty canvas or custom OpenGL/DirectX window exposing no interactive semantic controls.
    pub fn empty_canvas(hwnd: u64, bounds: CuaRect) -> UiTree {
        let root = UiElement::new("canvas_root", UiRole::Custom, "DirectX Surface", bounds)
            .with_state(UiState::static_display());

        let mut tree = UiTree::new(hwnd, root);
        SoMIndexer::index_tree(&mut tree);
        tree
    }

    /// Creates a noisy layout tree with deeply nested containers, zero-dimension controls,
    /// invisible elements, disabled controls, offscreen items, and 3 valid interactive buttons.
    pub fn noisy_layout(hwnd: u64, bounds: CuaRect) -> UiTree {
        let x = bounds.x;
        let y = bounds.y;

        // Container wrappers (non-interactive)
        let outer_pane = UiElement::new("pane_outer", UiRole::Pane, "Outer Pane", bounds)
            .with_state(UiState::static_display());
        let inner_group = UiElement::new("group_inner", UiRole::Group, "Controls Group", bounds)
            .with_state(UiState::static_display());

        // Zero-dimension degenerate control
        let zero_dim = UiElement::new(
            "zero_dim_elem",
            UiRole::Button,
            "Invisible Zero",
            CuaRect::new(x + 50, y + 50, 0, 0),
        )
        .with_state(UiState::clickable());

        // Invisible control
        let invisible = UiElement::new(
            "invisible_btn",
            UiRole::Button,
            "Ghost Button",
            CuaRect::new(x + 50, y + 50, 80, 30),
        )
        .with_state(UiState::hidden());

        // Disabled control
        let disabled = UiElement::new(
            "disabled_btn",
            UiRole::Button,
            "Disabled Button",
            CuaRect::new(x + 50, y + 90, 80, 30),
        )
        .with_state(UiState::disabled());

        // Offscreen control sentinel
        let offscreen = UiElement::new(
            "offscreen_btn",
            UiRole::Button,
            "Offscreen Button",
            CuaRect::new(-32000, -32000, 80, 30),
        )
        .with_state(UiState::clickable());

        // Static label (non-interactive)
        let label = UiElement::new(
            "static_lbl",
            UiRole::Text,
            "Status: Ready",
            CuaRect::new(x + 20, y + 10, 100, 20),
        )
        .with_state(UiState::static_display());

        // 3 Valid interactive buttons
        let btn1 = UiElement::new(
            "btn_submit",
            UiRole::Button,
            "Submit",
            CuaRect::new(x + 20, y + 150, 80, 30),
        )
        .with_state(UiState::clickable());

        let btn2 = UiElement::new(
            "btn_reset",
            UiRole::Button,
            "Reset",
            CuaRect::new(x + 110, y + 150, 80, 30),
        )
        .with_state(UiState::clickable());

        let btn3 = UiElement::new(
            "btn_help",
            UiRole::Button,
            "Help",
            CuaRect::new(x + 200, y + 150, 80, 30),
        )
        .with_state(UiState::clickable());

        let group = inner_group.with_children(vec![
            zero_dim, invisible, disabled, offscreen, label, btn1, btn2, btn3,
        ]);
        let root = outer_pane.with_child(group);

        let mut tree = UiTree::new(hwnd, root);
        SoMIndexer::index_tree(&mut tree);
        tree
    }

    /// Creates a multi-row form with controls containing slight vertical jitter (<= 4px)
    /// to verify reading-order sorting and row band clustering.
    pub fn multi_row_form(hwnd: u64, bounds: CuaRect) -> UiTree {
        let x = bounds.x;
        let y = bounds.y;

        // Row 1: First Name and Last Name inputs (y = y + 20 and y + 22)
        let row1_input_first = UiElement::new(
            "in_first_name",
            UiRole::Edit,
            "First Name",
            CuaRect::new(x + 20, y + 20, 150, 28),
        )
        .with_state(UiState::editable());

        let row1_input_last = UiElement::new(
            "in_last_name",
            UiRole::Edit,
            "Last Name",
            CuaRect::new(x + 190, y + 22, 150, 28),
        )
        .with_state(UiState::editable());

        // Row 2: Email input (y = y + 70)
        let row2_input_email = UiElement::new(
            "in_email",
            UiRole::Edit,
            "Email Address",
            CuaRect::new(x + 20, y + 70, 320, 28),
        )
        .with_state(UiState::editable());

        // Row 3: Back and Next buttons (y = y + 130 and y + 128)
        let row3_btn_back = UiElement::new(
            "btn_back",
            UiRole::Button,
            "Back",
            CuaRect::new(x + 170, y + 130, 75, 30),
        )
        .with_state(UiState::clickable());

        let row3_btn_next = UiElement::new(
            "btn_next",
            UiRole::Button,
            "Next",
            CuaRect::new(x + 260, y + 128, 75, 30),
        )
        .with_state(UiState::clickable());

        let root = UiElement::new("form_root", UiRole::Window, "Multi-Row Form", bounds)
            .with_children(vec![
                row1_input_first,
                row1_input_last,
                row2_input_email,
                row3_btn_back,
                row3_btn_next,
            ]);

        let mut tree = UiTree::new(hwnd, root);
        SoMIndexer::index_tree(&mut tree);
        tree
    }
}
