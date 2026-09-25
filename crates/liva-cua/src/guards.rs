//! Defensive state guards and recovery engines.

use crate::types::{CuaError, CuaRect};
use tracing::{debug, warn};

/// Guard 1: Detects minimized windows via IsIconic sentinel or (-32000, -32000) coordinates.
pub struct MinimizedWindowGuard;

impl MinimizedWindowGuard {
    /// Validates that a window is on-screen and non-iconic.
    pub fn validate(hwnd: u64, is_iconic: bool, bounds: &CuaRect) -> Result<(), CuaError> {
        if is_iconic {
            warn!(
                hwnd = format!("{:#x}", hwnd),
                "Target window is iconic (minimized)"
            );
            return Err(CuaError::WindowMinimized(hwnd));
        }

        if bounds.is_minimized_sentinel() {
            warn!(
                hwnd = format!("{:#x}", hwnd),
                "Target window coordinates match sentinel (-32000, -32000)"
            );
            return Err(CuaError::WindowMinimized(hwnd));
        }

        if bounds.is_empty() {
            warn!(
                hwnd = format!("{:#x}", hwnd),
                "Target window bounds are degenerate (width <= 0 or height <= 0)"
            );
            return Err(CuaError::WindowMinimized(hwnd));
        }

        Ok(())
    }
}

/// Guard 2: Enforces that target HWND actually belongs to the caller's authorized PID.
#[derive(Debug, Clone)]
pub struct ExactPidWindowTargetGuard {
    pub expected_pid: u32,
    pub expected_process_name: Option<String>,
}

impl ExactPidWindowTargetGuard {
    pub fn new(expected_pid: u32) -> Self {
        Self {
            expected_pid,
            expected_process_name: None,
        }
    }

    pub fn with_process_name(mut self, process_name: impl Into<String>) -> Self {
        self.expected_process_name = Some(process_name.into());
        self
    }

    /// Validates ownership against actual PID and process name.
    /// Handles UWP ApplicationFrameHost child delegation if actual_pid belongs to frame host.
    pub fn validate(
        &self,
        hwnd: u64,
        actual_pid: u32,
        actual_process_name: Option<&str>,
        uwp_hosted_pid: Option<u32>,
    ) -> Result<(), CuaError> {
        let effective_pid = if let Some(hosted_pid) = uwp_hosted_pid {
            debug!(
                frame_pid = actual_pid,
                hosted_pid = hosted_pid,
                "Resolved UWP child PID"
            );
            hosted_pid
        } else {
            actual_pid
        };

        if effective_pid != self.expected_pid {
            warn!(
                hwnd = format!("{:#x}", hwnd),
                expected_pid = self.expected_pid,
                actual_pid = effective_pid,
                "Target PID mismatch rejected by ExactPidWindowTargetGuard"
            );
            return Err(CuaError::TargetPidMismatch {
                hwnd,
                expected_pid: self.expected_pid,
                actual_pid: effective_pid,
            });
        }

        if let Some(ref expected_name) = self.expected_process_name {
            if let Some(actual_name) = actual_process_name {
                if !actual_name.eq_ignore_ascii_case(expected_name) {
                    warn!(
                        hwnd = format!("{:#x}", hwnd),
                        expected = expected_name,
                        actual = actual_name,
                        "Process image name mismatch rejected"
                    );
                    return Err(CuaError::TargetPidMismatch {
                        hwnd,
                        expected_pid: self.expected_pid,
                        actual_pid: effective_pid,
                    });
                }
            }
        }

        Ok(())
    }
}

/// Guard 3: Off-screen element detection and scrolling recovery controller.
pub struct OffscreenScrollRecovery {
    pub max_retries: u32,
}

impl Default for OffscreenScrollRecovery {
    fn default() -> Self {
        Self { max_retries: 2 }
    }
}

impl OffscreenScrollRecovery {
    pub fn new(max_retries: u32) -> Self {
        Self { max_retries }
    }

    /// Evaluates if an element bounding box is outside the client viewport.
    pub fn is_offscreen(&self, element: &CuaRect, viewport: &CuaRect) -> bool {
        let (cx, cy) = element.center();
        !viewport.contains_point(cx, cy)
    }

    /// Computes the required vertical and horizontal scroll delta to center the element.
    pub fn compute_scroll_delta(&self, element: &CuaRect, viewport: &CuaRect) -> (i32, i32) {
        let (ecx, ecy) = element.center();
        let (vcx, vcy) = viewport.center();
        (ecx - vcx, ecy - vcy)
    }
}
