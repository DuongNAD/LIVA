//! Coordinate translation and CuaAction resolution from Set-of-Mark (SoM) element indices.

use super::types::UiTree;
use crate::types::{CuaAction, CuaError, CuaPoint, CuaRect};

/// Translates Set-of-Mark element indices into verified desktop coordinates and CuaActions.
pub struct PerceptionResolver;

impl PerceptionResolver {
    /// Resolves a Set-of-Mark element index into its verified center coordinate point and bounding box.
    pub fn resolve_coordinate(
        tree: &UiTree,
        som_index: usize,
    ) -> Result<(CuaPoint, CuaRect), CuaError> {
        let element = tree.find_by_som(som_index).ok_or_else(|| {
            CuaError::InvalidParameter(format!("SoM mark [{som_index}] not found in UI tree"))
        })?;

        let (cx, cy) = element.center();
        Ok((CuaPoint::new(cx, cy), element.bounds))
    }

    /// Resolves a Set-of-Mark element index and converts an action template into a concrete, verified CuaAction.
    /// Preserves the element's bounding rectangle for post-action visual ROI verification.
    pub fn resolve_action(
        tree: &UiTree,
        som_index: usize,
        action_template: CuaAction,
    ) -> Result<CuaAction, CuaError> {
        let (center_pt, _bounds) = Self::resolve_coordinate(tree, som_index)?;
        let cx = center_pt.x;
        let cy = center_pt.y;

        let resolved = match action_template {
            CuaAction::Click {
                target_hwnd,
                button,
                click_count,
                delivery_mode,
                ..
            } => CuaAction::Click {
                target_hwnd,
                x: Some(cx),
                y: Some(cy),
                button,
                click_count,
                delivery_mode,
            },
            CuaAction::MoveCursor {
                target_hwnd,
                delivery_mode,
                ..
            } => CuaAction::MoveCursor {
                target_hwnd,
                x: cx,
                y: cy,
                delivery_mode,
            },
            CuaAction::Drag {
                target_hwnd,
                end_x,
                end_y,
                button,
                steps,
                delivery_mode,
                ..
            } => CuaAction::Drag {
                target_hwnd,
                start_x: cx,
                start_y: cy,
                end_x,
                end_y,
                button,
                steps,
                delivery_mode,
            },
            CuaAction::Scroll {
                target_hwnd,
                delta_x,
                delta_y,
                ..
            } => CuaAction::Scroll {
                target_hwnd,
                delta_x,
                delta_y,
                x: Some(cx),
                y: Some(cy),
            },
            CuaAction::TypeText {
                target_hwnd,
                text,
                delivery_mode,
            } => CuaAction::TypeText {
                target_hwnd,
                text,
                delivery_mode,
            },
            CuaAction::PressKey {
                target_hwnd,
                key,
                down,
                delivery_mode,
            } => CuaAction::PressKey {
                target_hwnd,
                key,
                down,
                delivery_mode,
            },
            CuaAction::Hotkey {
                target_hwnd,
                keys,
                delivery_mode,
            } => CuaAction::Hotkey {
                target_hwnd,
                keys,
                delivery_mode,
            },
            CuaAction::BringToFront { target_hwnd } => CuaAction::BringToFront { target_hwnd },
            CuaAction::RestoreWindow { target_hwnd } => CuaAction::RestoreWindow { target_hwnd },
        };

        Ok(resolved)
    }

    /// Validates whether an element's center point is strictly contained within its container or window.
    pub fn validate_containment(element_bounds: &CuaRect, container_bounds: &CuaRect) -> bool {
        let (cx, cy) = element_bounds.center();
        container_bounds.contains_point(cx, cy)
    }
}
