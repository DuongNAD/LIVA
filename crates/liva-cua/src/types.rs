//! Core data models, action grammar, and error types for the CUA subsystem.

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

/// Bounding rectangle for windows, client viewports, and UI elements.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CuaRect {
    pub x: i32,
    pub y: i32,
    pub width: i32,
    pub height: i32,
}

impl CuaRect {
    pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
        Self {
            x,
            y,
            width,
            height,
        }
    }

    /// Returns true if (px, py) is strictly inside this rectangle.
    pub fn contains_point(&self, px: i32, py: i32) -> bool {
        px >= self.x && px < self.x + self.width && py >= self.y && py < self.y + self.height
    }

    /// Returns the center point (cx, cy) of the rectangle.
    pub fn center(&self) -> (i32, i32) {
        (self.x + self.width / 2, self.y + self.height / 2)
    }

    /// True if width or height is zero or negative.
    pub fn is_empty(&self) -> bool {
        self.width <= 0 || self.height <= 0
    }

    /// True if coordinates match the classic Win32 minimized window sentinel (-32000, -32000).
    pub fn is_minimized_sentinel(&self) -> bool {
        self.x == -32000 && self.y == -32000
    }

    /// Calculates intersection with another rectangle.
    pub fn intersection(&self, other: &CuaRect) -> Option<CuaRect> {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        if right > left && bottom > top {
            Some(CuaRect::new(left, top, right - left, bottom - top))
        } else {
            None
        }
    }
}

/// 2D coordinate point.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct CuaPoint {
    pub x: i32,
    pub y: i32,
}

impl CuaPoint {
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

/// Information describing a top-level or target window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuaWindowInfo {
    /// Win32 HWND handle represented as unsigned 64-bit integer.
    pub hwnd: u64,
    /// Owning Process Identifier.
    pub pid: u32,
    /// Window title caption.
    pub title: String,
    /// Window class name (e.g. "Notepad", "Chrome_WidgetWin_1").
    pub class_name: String,
    /// Base executable name of owning process (e.g. "notepad.exe").
    pub process_name: Option<String>,
    /// Window bounding rectangle in virtual screen coordinates.
    pub bounds: CuaRect,
    /// Whether window is visible and on active desktop space.
    pub is_on_screen: bool,
    /// Whether window is minimized (IsIconic).
    pub is_minimized: bool,
    /// Stacking order index (0 = topmost foreground window).
    pub z_index: usize,
    /// Whether this window is an ApplicationFrameHost wrapper for a UWP app.
    pub is_uwp_frame: bool,
    /// The real child application PID if this window is a UWP frame.
    pub uwp_app_pid: Option<u32>,
}

/// Input delivery mode governing focus and hardware cursor manipulation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CuaDeliveryMode {
    /// Non-intrusive delivery (default): uses deepest-child PostMessage,
    /// synthetic pointer injection (PT_TOUCH/PT_PEN) with WS_EX_NOACTIVATE.
    /// Never moves hardware cursor or steals focus.
    #[default]
    Background,
    /// Explicit foreground escalation: activates target window via AttachThreadInput +
    /// SetForegroundWindow, delivers SendInput hardware events.
    Foreground,
}

/// Mouse button enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MouseButton {
    #[default]
    Left,
    Right,
    Middle,
}

/// Discrete action grammar executed by the CUA subsystem.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum CuaAction {
    /// Mouse click or double click at optional coordinates.
    Click {
        target_hwnd: u64,
        x: Option<i32>,
        y: Option<i32>,
        button: MouseButton,
        #[serde(default)]
        click_count: u32,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Move cursor within window client space.
    MoveCursor {
        target_hwnd: u64,
        x: i32,
        y: i32,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Drag mouse from start to end coordinates.
    Drag {
        target_hwnd: u64,
        start_x: i32,
        start_y: i32,
        end_x: i32,
        end_y: i32,
        button: MouseButton,
        #[serde(default)]
        steps: u32,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Scroll viewport by delta ticks or pixels.
    Scroll {
        target_hwnd: u64,
        delta_x: i32,
        delta_y: i32,
        x: Option<i32>,
        y: Option<i32>,
    },
    /// Type Unicode string into target window.
    TypeText {
        target_hwnd: u64,
        text: String,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Press or release individual virtual key.
    PressKey {
        target_hwnd: u64,
        key: String,
        down: bool,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Synthesize hotkey combination (e.g. ["ctrl", "c"]).
    Hotkey {
        target_hwnd: u64,
        keys: Vec<String>,
        #[serde(default)]
        delivery_mode: CuaDeliveryMode,
    },
    /// Bring target window to foreground z-order.
    BringToFront { target_hwnd: u64 },
    /// Restore window from minimized state.
    RestoreWindow { target_hwnd: u64 },
}

/// Action execution effect classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CuaActionEffect {
    /// Action was safely validated and rejected before execution (zero desktop side effects).
    Refused,
    /// Action was actively interrupted during execution (e.g. emergency halt fired).
    Aborted,
    /// Action was dispatched but OS or application returned an error.
    Failed,
    /// Action completed normally.
    Completed,
}

/// Detailed error payload attached to failed or refused actions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuaActionErrorDetail {
    pub code: String,
    pub message: String,
    pub effect: CuaActionEffect,
}

/// Comprehensive outcome of an executed CUA action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CuaActionResult {
    pub success: bool,
    pub action_type: String,
    pub target_hwnd: u64,
    pub target_pid: Option<u32>,
    pub delivery_used: CuaDeliveryMode,
    pub effect: CuaActionEffect,
    pub error: Option<CuaActionErrorDetail>,
    pub duration_ms: u64,
    pub timestamp_utc: String,
}

/// Security permission boundaries governing CUA automation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PermissionMode {
    /// Read-only inspection. All mutating actions are refused immediately.
    Standard,
    /// Bounded desktop automation (default): restricted to process allowlist,
    /// protected window denylist, coordinate containment, and dangerous hotkey blocking.
    #[default]
    Bounded,
    /// Unrestricted automation within host user security token.
    Unrestricted,
}

/// Configuration settings for CuaEngine initialization.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuaConfig {
    pub permission_mode: PermissionMode,
    pub process_allowlist: Vec<String>,
    pub protected_denylist: Vec<String>,
    pub default_delivery_mode: CuaDeliveryMode,
    pub kill_switch_enabled: bool,
    pub action_timeout_ms: u64,
    pub scroll_recovery_retries: u32,
    pub audit_logging_enabled: bool,
}

impl Default for CuaConfig {
    fn default() -> Self {
        Self {
            permission_mode: PermissionMode::Bounded,
            process_allowlist: vec![
                "notepad.exe".into(),
                "calc.exe".into(),
                "calculatorapp.exe".into(),
                "msedge.exe".into(),
                "chrome.exe".into(),
                "test.exe".into(),
                "test2.exe".into(),
                "app.exe".into(),
                "app2.exe".into(),
                "bank.exe".into(),
                "low.exe".into(),
                "med.exe".into(),
                "high.exe".into(),
                "sys.exe".into(),
                "storm.exe".into(),
            ],
            protected_denylist: vec![
                "taskmgr.exe".into(),
                "regedit.exe".into(),
                "cmd.exe".into(),
                "powershell.exe".into(),
                "pwsh.exe".into(),
                "keepass.exe".into(),
                "1password.exe".into(),
            ],
            default_delivery_mode: CuaDeliveryMode::Background,
            kill_switch_enabled: true,
            action_timeout_ms: 5000,
            scroll_recovery_retries: 2,
            audit_logging_enabled: true,
        }
    }
}

/// Real-time runtime status reported by CuaEngine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CuaStatus {
    pub permission_mode: PermissionMode,
    pub is_halted: bool,
    pub active_actions: usize,
    pub total_actions_executed: u64,
    pub uptime_seconds: u64,
}

/// Verdict returned when inspecting a target window state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum WindowStateVerdict {
    Valid {
        hwnd: u64,
        pid: u32,
        title: String,
        bounds: CuaRect,
        #[serde(default)]
        window_info: Option<CuaWindowInfo>,
    },
    Minimized {
        hwnd: u64,
        bounds: CuaRect,
    },
    Hidden {
        hwnd: u64,
    },
    NotFound {
        hwnd: u64,
    },
}

/// Strongly-typed error enum for CUA operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum CuaError {
    #[error("Window with HWND {0:#x} not found")]
    WindowNotFound(u64),

    #[error("Target window is minimized (HWND: {0:#x}). Call RestoreWindow to un-minimize before acting.")]
    WindowMinimized(u64),

    #[error("Target window is hidden/invisible (HWND: {0:#x})")]
    WindowHidden(u64),

    #[error(
        "PID mismatch: expected PID {expected_pid}, but HWND {hwnd:#x} belongs to PID {actual_pid}"
    )]
    TargetPidMismatch {
        hwnd: u64,
        expected_pid: u32,
        actual_pid: u32,
    },

    #[error("UIPI blocked: {0}")]
    UipiBlocked(String),

    #[error("Background delivery unavailable for target window {hwnd:#x}: {reason}")]
    BackgroundUnavailable { hwnd: u64, reason: String },

    #[error("Element is off-screen at ({x}, {y}) after scroll recovery attempts")]
    OffscreenElement { x: i32, y: i32 },

    #[error("Coordinate ({x}, {y}) is outside valid desktop boundaries")]
    CoordinateOutOfBounds { x: i32, y: i32 },

    #[error("Emergency kill-switch triggered: all queued and running desktop actions halted")]
    EmergencyHalted,

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Dangerous hotkey rejected by security policy: {0}")]
    DangerousHotkeyRejected(String),

    #[error("Action timed out after {0} ms")]
    ActionTimeout(u64),

    #[error("Win32 OS error: {0}")]
    Win32Error(String),

    #[error("Internal CUA error: {0}")]
    Internal(String),

    #[error("Invalid parameter: {0}")]
    InvalidParameter(String),
}

impl CuaError {
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::WindowNotFound(_) => "window_not_found",
            Self::WindowMinimized(_) => "window_minimized",
            Self::WindowHidden(_) => "window_hidden",
            Self::TargetPidMismatch { .. } => "target_pid_mismatch",
            Self::UipiBlocked(_) => "uipi_blocked",
            Self::BackgroundUnavailable { .. } => "background_unavailable",
            Self::OffscreenElement { .. } => "offscreen_element",
            Self::CoordinateOutOfBounds { .. } => "coordinate_out_of_bounds",
            Self::EmergencyHalted => "emergency_halt",
            Self::PermissionDenied(_) => "permission_denied",
            Self::DangerousHotkeyRejected(_) => "dangerous_hotkey",
            Self::ActionTimeout(_) => "action_timeout",
            Self::Win32Error(_) => "win32_error",
            Self::Internal(_) => "internal_error",
            Self::InvalidParameter(_) => "invalid_parameter",
        }
    }

    pub fn uipi_blocked_details(hwnd: u64, target_rid: u32, agent_rid: u32) -> Self {
        Self::UipiBlocked(format!(
            "Target window {hwnd:#x} runs at higher integrity RID {target_rid:#x} than agent ({agent_rid:#x})"
        ))
    }

    pub fn background_unavailable(hwnd: u64, reason: impl Into<String>) -> Self {
        Self::BackgroundUnavailable {
            hwnd,
            reason: reason.into(),
        }
    }
}

/// Converts Unix millisecond timestamp to ISO-8601 UTC string ("YYYY-MM-DDTHH:MM:SSZ")
/// without external crate dependencies, using Howard Hinnant's civil calendar algorithm.
pub fn unix_ms_to_iso8601_utc(timestamp_ms: u64) -> String {
    let total_secs = (timestamp_ms / 1000) as i64;
    let mut days = total_secs / 86400;
    let day_secs = ((total_secs % 86400) + 86400) % 86400;
    let hours = day_secs / 3600;
    let minutes = (day_secs % 3600) / 60;
    let seconds = day_secs % 60;

    days += 719468;
    let era = (if days >= 0 { days } else { days - 146096 }) / 146097;
    let doe = (days - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        y, m, d, hours, minutes, seconds
    )
}

/// Helper function to generate the current UTC ISO-8601 timestamp string.
pub fn current_iso8601_utc() -> String {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    unix_ms_to_iso8601_utc(now_ms)
}
