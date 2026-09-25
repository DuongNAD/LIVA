//! Abstract driver traits decoupling CuaEngine from Win32 interop for testing.

use crate::kill_switch::CancellationToken;
use crate::types::{CuaAction, CuaActionResult, CuaError, CuaWindowInfo, WindowStateVerdict};
use async_trait::async_trait;

#[async_trait]
pub trait CuaDriverTrait: Send + Sync {
    /// Enumerate all top-level desktop windows.
    async fn list_windows(&self, filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError>;

    /// Inspect single window state and bounds.
    async fn inspect_window(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError>;

    /// Dispatch discrete action.
    async fn dispatch_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError>;

    /// Un-minimize / restore window to active desktop.
    async fn restore_window(&self, hwnd: u64) -> Result<(), CuaError>;

    /// Scroll window container by (delta_x, delta_y).
    async fn scroll_container(&self, hwnd: u64, delta_x: i32, delta_y: i32)
        -> Result<(), CuaError>;

    /// Optional hook to query simulated UIPI for test harness.
    fn check_simulated_uipi(&self, _hwnd: u64) -> Option<(u32, u32)> {
        None
    }

    /// Optional hook to receive active Kill-Switch CancellationToken for loop cancellation.
    fn set_cancellation_token(&self, _token: CancellationToken) {}
}
