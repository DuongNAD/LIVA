//! Vision-Aligned Fast Action Router (System-1 GUI Loop).
//!
//! Provides target descriptor translation across 7 coordinate spaces,
//! deterministic 6-stage pre-execution checks (< 10ms SLA), delivery fallback escalation
//! (Background -> Foreground on 0% diff), deterministic retries, zero-token multi-step macro
//! execution (> 95% reduction), and structured System-2 defect reporting.

use crate::audit::CuaAuditRecorder;
use crate::kill_switch::KillSwitchController;
use crate::security::SecurityGovernor;
use crate::traits::CuaDriverTrait;
use crate::types::{
    current_iso8601_utc, CuaAction, CuaDeliveryMode, CuaError, CuaPoint, CuaRect, CuaWindowInfo,
    MouseButton, PermissionMode, WindowStateVerdict,
};
use crate::vision::traits::VisualVerifierTrait;
use crate::vision::types::{SemanticVerificationCriteria, VisualDiffResult};
use crate::CuaEngine;

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tracing::{info, warn};

/// Strategy for resolving the target top-level window.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "selector_type", content = "value", rename_all = "snake_case")]
pub enum WindowSelector {
    /// Explicit Win32 HWND handle.
    Hwnd(u64),
    /// Currently focused active foreground window.
    Active,
    /// Owning Process Identifier (PID).
    Pid(u32),
    /// Base executable image name (case-insensitive, e.g. "notepad.exe", "msedge.exe").
    ProcessName(String),
    /// Window title caption matching a pattern or regex.
    TitleRegex(String),
    /// Window title caption containing a literal substring (case-insensitive).
    TitleContains(String),
    /// Hit-test from a physical virtual desktop coordinate point.
    ScreenPoint(CuaPoint),

    // Test and ergonomic aliases
    TitlePattern(String),
    ActiveForeground,
}

/// Representation of coordinate space for target interaction points across 7 spaces.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(tag = "space", rename_all = "snake_case")]
pub enum TargetCoordinate {
    /// Normalized coordinates in window client viewport (0.0 <= x, y <= 1.0).
    ClientNormalized { x: f32, y: f32 },
    /// Normalized coordinates across entire outer window bounding box (0.0 <= x, y <= 1.0).
    WindowNormalized { x: f32, y: f32 },
    /// Normalized coordinates across entire multi-monitor virtual desktop (0.0 <= x, y <= 1.0).
    ScreenNormalized { x: f32, y: f32 },
    /// Absolute pixel coordinates relative to the window client rect top-left (0, 0).
    AbsoluteClient(CuaPoint),
    /// Absolute physical pixel coordinates in the multi-monitor virtual desktop space.
    AbsoluteScreen(CuaPoint),
    /// Normalized coordinates relative to an expected visual ROI bounding box.
    RoiNormalized {
        roi: CuaRect,
        norm_x: f32,
        norm_y: f32,
    },
    /// Center point of target window client area.
    #[default]
    WindowClientCenter,

    // Aliases for System-2 / test compatibility
    #[serde(rename = "normalized")]
    Normalized { x: f32, y: f32 },
    #[serde(rename = "client_pixel")]
    ClientPixel { x: i32, y: i32 },
    #[serde(rename = "screen_pixel")]
    ScreenPixel { x: i32, y: i32 },
    #[serde(rename = "roi_center")]
    RoiCenter,
}

impl TargetCoordinate {
    pub fn client_normalized(x: f32, y: f32) -> Self {
        Self::ClientNormalized { x, y }
    }

    pub fn window_normalized(x: f32, y: f32) -> Self {
        Self::WindowNormalized { x, y }
    }

    pub fn screen_normalized(x: f32, y: f32) -> Self {
        Self::ScreenNormalized { x, y }
    }

    pub fn absolute_client(x: i32, y: i32) -> Self {
        Self::AbsoluteClient(CuaPoint::new(x, y))
    }

    pub fn absolute_screen(x: i32, y: i32) -> Self {
        Self::AbsoluteScreen(CuaPoint::new(x, y))
    }

    pub fn roi_normalized(roi: CuaRect, norm_x: f32, norm_y: f32) -> Self {
        Self::RoiNormalized {
            roi,
            norm_x,
            norm_y,
        }
    }

    pub fn window_client_center() -> Self {
        Self::WindowClientCenter
    }
}

/// Complete descriptor specifying an interaction target and its visual metadata.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TargetDescriptor {
    /// Strategy to locate the target window.
    pub window_selector: WindowSelector,
    /// Coordinate specification for the interaction point.
    #[serde(default)]
    pub coordinate: TargetCoordinate,
    /// Expected bounding box in screen coordinates for localized post-diff verification (< 80ms).
    pub expected_roi: Option<CuaRect>,
    /// Semantic element label (e.g. "submit_button", "search_bar").
    pub semantic_label: Option<String>,
    /// Visual confidence score from VLM or object detector (0.0..=1.0).
    pub confidence: Option<f32>,
}

impl TargetDescriptor {
    pub fn new(window_selector: WindowSelector, coordinate: TargetCoordinate) -> Self {
        Self {
            window_selector,
            coordinate,
            expected_roi: None,
            semantic_label: None,
            confidence: None,
        }
    }

    pub fn with_roi(mut self, roi: CuaRect) -> Self {
        self.expected_roi = Some(roi);
        self
    }

    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.semantic_label = Some(label.into());
        self
    }

    pub fn with_confidence(mut self, confidence: f32) -> Self {
        self.confidence = Some(confidence);
        self
    }
}

/// Discrete atomic action grammar executed by the System-1 Action Router.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum System1Action {
    /// Click at visual target point.
    Click {
        target: TargetDescriptor,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        click_count: u32,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    /// Double click at visual target point.
    DoubleClick { target: TargetDescriptor },
    /// Move cursor or hover over visual target point.
    Hover {
        target: TargetDescriptor,
        #[serde(default)]
        duration_ms: Option<u32>,
    },
    /// Drag from start visual target to end visual target.
    Drag {
        start: TargetDescriptor,
        end: TargetDescriptor,
        #[serde(default)]
        button: Option<MouseButton>,
        #[serde(default = "default_drag_steps")]
        steps: u32,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    /// Scroll container at visual target point.
    Scroll {
        target: Option<TargetDescriptor>,
        delta_x: i32,
        delta_y: i32,
    },
    /// Type text into visual target.
    TypeText {
        target: Option<TargetDescriptor>,
        text: String,
        #[serde(default)]
        clear_before: bool,
        #[serde(default)]
        press_enter: bool,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    /// Synthesize hotkey combination.
    Hotkey {
        target: Option<TargetDescriptor>,
        keys: Vec<String>,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    /// Non-mutating visual synchronizer: wait for visual change in target ROI.
    WaitVisualChange {
        target: TargetDescriptor,
        #[serde(default = "default_wait_timeout_ms")]
        timeout_ms: u64,
        #[serde(default)]
        min_changed_ratio: Option<f32>,
    },
    /// Bring target window to foreground z-order.
    BringToFront { target: TargetDescriptor },
    /// Restore target window from minimized state.
    RestoreWindow { target: TargetDescriptor },
}

fn default_click_count() -> u32 {
    1
}

fn default_drag_steps() -> u32 {
    10
}

fn default_wait_timeout_ms() -> u64 {
    3000
}

/// Detailed reason for out-of-bounds coordinate refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OutOfBoundsReason {
    OutsideWindowRect,
    OutsideClientDrawingSurface,
    TitleBarCaptionProtected,
    CloseButtonProtected,
    NonClientHitTest,
}

/// Policy rule violated during security governance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SecurityPolicyRule {
    StandardModeReadOnly,
    ProcessNotAllowlisted,
    ProtectedProcessDenylist,
    ProtectedShellWindow,
    UacElevationDialog,
    UipiIntegrityElevation,
    DangerousHotkeyBlocked,
}

/// Strongly-typed error classification for pre-execution failures.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "error_code", rename_all = "snake_case")]
pub enum PreCheckError {
    /// Window handle does not exist, closed, or destroyed.
    WindowNotFound { target_hwnd: u64, detail: String },
    /// Target window is minimized to taskbar (IsIconic or sentinel coordinates).
    WindowMinimized {
        target_hwnd: u64,
        can_auto_restore: bool,
    },
    /// Target window is hidden / invisible (WS_VISIBLE == 0).
    WindowHidden { target_hwnd: u64 },
    /// Target window is cloaked (on other virtual desktop or suspended state).
    WindowCloaked { target_hwnd: u64 },
    /// Window bounds are empty or degenerate (width <= 0 || height <= 0).
    WindowEmptyBounds { target_hwnd: u64, bounds: CuaRect },
    /// Target coordinates are occluded by an unauthorized foreground window or modal.
    TargetOccluded {
        target_hwnd: u64,
        target_point: CuaPoint,
        occluded_by_hwnd: u64,
        occluded_by_process: Option<String>,
        occluded_by_title: Option<String>,
    },
    /// Target coordinates fall outside client drawing area or hit window chrome.
    OutOfBounds {
        client_x: i32,
        client_y: i32,
        client_bounds: CuaRect,
        reason: OutOfBoundsReason,
    },
    /// Action refused by SecurityGovernor policy or UIPI elevation boundary.
    SecurityRefused {
        reason: String,
        policy_rule: SecurityPolicyRule,
    },
    /// Emergency halt signal active.
    EmergencyHalted,
    /// Win32 OS internal call error.
    Win32Error(String),
}

impl std::fmt::Display for PreCheckError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::WindowNotFound {
                target_hwnd,
                detail,
            } => {
                write!(f, "Window 0x{:X} not found: {}", target_hwnd, detail)
            }
            Self::WindowMinimized { target_hwnd, .. } => {
                write!(f, "Window 0x{:X} is minimized", target_hwnd)
            }
            Self::WindowHidden { target_hwnd } => {
                write!(f, "Window 0x{:X} is hidden/invisible", target_hwnd)
            }
            Self::WindowCloaked { target_hwnd } => {
                write!(f, "Window 0x{:X} is cloaked", target_hwnd)
            }
            Self::WindowEmptyBounds {
                target_hwnd,
                bounds,
            } => {
                write!(
                    f,
                    "Window 0x{:X} has empty bounds {:?}",
                    target_hwnd, bounds
                )
            }
            Self::TargetOccluded {
                target_hwnd,
                occluded_by_hwnd,
                ..
            } => {
                write!(
                    f,
                    "Target window 0x{:X} occluded by window 0x{:X}",
                    target_hwnd, occluded_by_hwnd
                )
            }
            Self::OutOfBounds {
                client_x,
                client_y,
                reason,
                ..
            } => {
                write!(
                    f,
                    "Point ({}, {}) is out of bounds: {:?}",
                    client_x, client_y, reason
                )
            }
            Self::SecurityRefused { reason, .. } => {
                write!(f, "Security policy refusal: {}", reason)
            }
            Self::EmergencyHalted => write!(f, "Execution halted by emergency kill-switch"),
            Self::Win32Error(err) => write!(f, "Win32 internal error: {}", err),
        }
    }
}

impl std::error::Error for PreCheckError {}

impl PreCheckError {
    /// Classifies whether this error can be autonomously recovered by System-1.
    pub fn is_recoverable(&self) -> bool {
        matches!(
            self,
            Self::WindowMinimized {
                can_auto_restore: true,
                ..
            } | Self::TargetOccluded { .. }
                | Self::OutOfBounds { .. }
        )
    }

    /// Machine-readable error code.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::WindowNotFound { .. } => "window_not_found",
            Self::WindowMinimized { .. } => "window_minimized",
            Self::WindowHidden { .. } => "window_hidden",
            Self::WindowCloaked { .. } => "window_cloaked",
            Self::WindowEmptyBounds { .. } => "window_empty_bounds",
            Self::TargetOccluded { .. } => "target_occluded",
            Self::OutOfBounds { .. } => "coordinate_out_of_bounds",
            Self::SecurityRefused { .. } => "security_policy_refused",
            Self::EmergencyHalted => "emergency_halt",
            Self::Win32Error(_) => "win32_error",
        }
    }

    /// Returns recommended recovery action hint for System-1 or System-2 planner.
    pub fn recovery_hint(&self) -> &'static str {
        match self {
            Self::WindowMinimized { .. } => "Restore window via RestoreWindow/ShowWindowAsync",
            Self::TargetOccluded { .. } => "Bring target window to front via BringToFront",
            Self::OutOfBounds {
                reason: OutOfBoundsReason::OutsideClientDrawingSurface,
                ..
            } => "Scroll viewport to bring target into client view",
            Self::OutOfBounds {
                reason: OutOfBoundsReason::CloseButtonProtected,
                ..
            } => "Target point hit close button - refactor coordinates",
            Self::SecurityRefused { .. } => {
                "Policy violation: requires human permission or allowlist update"
            }
            Self::EmergencyHalted => "Halted by emergency kill-switch: requires manual reset",
            Self::WindowNotFound { .. } => "Target window closed: regenerate task plan",
            Self::WindowCloaked { .. } => "Switch virtual desktop or activate target workspace",
            _ => "Unrecoverable defect",
        }
    }
}

/// High-level failure category classifying the root cause of the defect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DefectCategory {
    /// Pre-execution state check failed (window minimized, hidden, not found, PID mismatch).
    PreCheckFailed,
    /// Coordinate out of bounds or outside client rect.
    OutOfBounds,
    /// Target coordinates occluded by another window or modal dialog.
    TargetOccluded,
    /// Action rejected by SecurityGovernor (allowlist, denylist, dangerous hotkey, Standard mode).
    SecurityRefused,
    /// Action actively aborted by emergency kill-switch (< 15ms SLA).
    KillSwitchAborted,
    /// Background delivery yielded 0% diff, and foreground escalation also yielded 0% diff.
    VisualVerificationZeroDiff,
    /// Foreground escalation was required but blocked by security policy.
    EscalationBlockedByPolicy,
    /// Win32 OS driver or hardware API returned an unrecoverable error.
    DriverError,
    /// Operation timed out waiting for visual change.
    Timeout,
}

/// Actionable diagnostic remedy hint guiding System-2 VLM re-planning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "remedy_type", content = "advice", rename_all = "snake_case")]
pub enum RemedyHint {
    /// The target coordinates were occluded or changed; take a fresh screenshot and re-identify target.
    ReacquireTargetCoordinates(String),
    /// The window is minimized; issue RestoreWindow or ask user to un-minimize.
    RestoreMinimizedWindow(String),
    /// Background delivery was ignored; foreground escalation required user focus grant.
    BringWindowToForeground(String),
    /// Zero visual diff observed after foreground retry; element may be disabled or static.
    AdjustVisualThreshold(String),
    /// Target process runs at higher integrity level (UIPI restriction); elevated privileges required.
    ElevationRequired(String),
    /// Target window or hotkey is blocked by security policy; add to allowlist or change approach.
    SecurityPolicyViolation(String),
    /// User physically intervened or Esc kill-switch halted execution; await user instruction.
    UserInterventionNeeded(String),
    /// Window closed unexpectedly during plan execution.
    WindowDied(String),
}

/// Comprehensive defect report returned to System-2 VLM planner.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct System1DefectReport {
    /// Unique identifier of the defect report.
    pub defect_id: String,
    /// Calling agent session ID.
    pub session_id: String,
    /// ID of the macro plan that encountered the failure.
    pub plan_id: Option<String>,
    /// 0-indexed position of the step in the macro plan that failed.
    pub failed_step_index: usize,
    /// Total number of steps in the plan.
    pub total_steps: usize,
    /// Number of steps successfully completed before this failure.
    pub completed_steps: usize,
    /// The exact System1Action that failed.
    pub failed_action: System1Action,
    /// Primary root-cause classification.
    pub failure_category: DefectCategory,
    /// Description of the pre-check verdict if failed prior to dispatch.
    pub pre_check_error: Option<String>,
    /// Post-action visual diff evaluation result if diff was evaluated.
    pub post_diff_result: Option<VisualDiffResult>,
    /// Number of retries attempted on this step (0, 1, or 2).
    pub retry_count: u32,
    /// Sequential list of delivery modes attempted (e.g. [Background, Foreground]).
    pub delivery_modes_attempted: Vec<CuaDeliveryMode>,
    /// Machine-readable error code.
    pub error_code: String,
    /// Human-readable diagnostic explanation.
    pub error_message: String,
    /// Concrete remedy recommendation for the System-2 VLM planner.
    pub remedy_hint: RemedyHint,
    /// Last known window information for the target.
    pub last_known_window: Option<CuaWindowInfo>,
    /// True if System-2 must capture a brand new screenshot before re-planning.
    pub screenshot_recommended: bool,
    /// UTC ISO-8601 timestamp of failure.
    pub timestamp_utc: String,
}

/// Multi-step macro plan executed deterministically by System-1 in Rust.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroPlan {
    pub plan_id: String,
    pub session_id: String,
    pub goal_description: String,
    pub steps: Vec<System1Action>,
    pub max_step_retries: u32,
    pub abort_on_error: bool,
    pub allow_foreground_escalation: bool,
}

impl Default for MacroPlan {
    fn default() -> Self {
        Self {
            plan_id: uuid::Uuid::new_v4().to_string(),
            session_id: "default_session".into(),
            goal_description: "System-1 Macro Execution".into(),
            steps: Vec::new(),
            max_step_retries: 2,
            abort_on_error: true,
            allow_foreground_escalation: true,
        }
    }
}

impl MacroPlan {
    pub fn from_actions(steps: Vec<System1Action>) -> Self {
        Self {
            steps,
            ..Default::default()
        }
    }
}

/// Result of an individual step execution within a macro plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct System1StepResult {
    pub step_index: usize,
    pub action: System1Action,
    pub success: bool,
    pub delivery_used: CuaDeliveryMode,
    pub escalated_from_background: bool,
    pub retries_taken: u32,
    pub diff_result: Option<VisualDiffResult>,
    pub duration_ms: u64,
}

/// Overall outcome of a multi-step macro plan execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MacroPlanResult {
    pub plan_id: String,
    pub session_id: String,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub success: bool,
    pub step_results: Vec<System1StepResult>,
    pub defect_report: Option<System1DefectReport>,
    pub total_duration_ms: u64,
    /// Tokens consumed during execution (always 0 in System-1).
    pub tokens_consumed: usize,
}

pub type System1PlanResult = MacroPlanResult;

/// Configuration parameters for System1ActionRouter execution.
#[derive(Debug, Clone)]
pub struct System1RouterConfig {
    /// Maximum retries per atomic action step (default: 2).
    pub max_step_retries: u32,
    /// Whether background -> foreground escalation upon 0% diff is enabled.
    pub allow_foreground_escalation: bool,
    /// Default redraw micro-tick delay in milliseconds (default: 35ms).
    pub redraw_micro_tick_ms: u64,
    /// Step timeout in milliseconds (default: 5000ms).
    pub step_timeout_ms: u64,
}

impl Default for System1RouterConfig {
    fn default() -> Self {
        Self {
            max_step_retries: 2,
            allow_foreground_escalation: true,
            redraw_micro_tick_ms: 35,
            step_timeout_ms: 5000,
        }
    }
}

/// Context of a fully resolved visual interaction target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTarget {
    pub hwnd: u64,
    pub pid: u32,
    pub process_name: Option<String>,
    pub title: String,
    pub window_bounds: CuaRect,
    pub client_bounds: CuaRect,
    pub screen_point: CuaPoint,
    pub client_point: CuaPoint,
    pub hit_control_hwnd: u64,
    pub expected_roi: Option<CuaRect>,
    pub semantic_label: Option<String>,
    pub window_info: CuaWindowInfo,
}

/// Container holding resolved interaction targets for an action step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTargets {
    /// Primary target (interaction start point, or single target).
    pub primary: ResolvedTarget,
    /// Optional secondary target (interaction destination, e.g. Drag end point).
    pub secondary: Option<ResolvedTarget>,
}

/// Target translation engine converting TargetDescriptor into OS handles and coordinates.
pub struct TargetResolver;

impl TargetResolver {
    /// Resolves WindowSelector and TargetCoordinate into concrete ResolvedTarget.
    pub async fn resolve(
        descriptor: &TargetDescriptor,
        driver: &dyn CuaDriverTrait,
    ) -> Result<ResolvedTarget, PreCheckError> {
        // 1. Resolve target window
        let window_info = Self::resolve_window(&descriptor.window_selector, driver).await?;

        // 2. Resolve client bounds in screen coordinates
        let client_bounds =
            Self::get_client_bounds_in_screen(window_info.hwnd, &window_info.bounds);

        // 3. Translate TargetCoordinate across 7 spaces
        let (screen_point, client_point) = Self::translate_coordinates(
            &descriptor.coordinate,
            &window_info.bounds,
            &client_bounds,
            descriptor.expected_roi.as_ref(),
        )?;

        // 4. Hit-test child control
        let hit_control_hwnd =
            Self::hit_test_point(screen_point.x, screen_point.y, window_info.hwnd);

        Ok(ResolvedTarget {
            hwnd: window_info.hwnd,
            pid: window_info.pid,
            process_name: window_info.process_name.clone(),
            title: window_info.title.clone(),
            window_bounds: window_info.bounds,
            client_bounds,
            screen_point,
            client_point,
            hit_control_hwnd,
            expected_roi: descriptor.expected_roi,
            semantic_label: descriptor.semantic_label.clone(),
            window_info,
        })
    }

    /// Locates the target window applying z-order and on-screen disambiguation.
    pub async fn resolve_window(
        selector: &WindowSelector,
        driver: &dyn CuaDriverTrait,
    ) -> Result<CuaWindowInfo, PreCheckError> {
        match selector {
            WindowSelector::Hwnd(hwnd) => match driver.inspect_window(*hwnd).await {
                Ok(WindowStateVerdict::Valid {
                    window_info: Some(info),
                    ..
                }) => Ok(info),
                Ok(WindowStateVerdict::Valid {
                    hwnd,
                    pid,
                    title,
                    bounds,
                    ..
                }) => Ok(CuaWindowInfo {
                    hwnd,
                    pid,
                    title,
                    class_name: "StandardApp".into(),
                    process_name: None,
                    bounds,
                    is_on_screen: true,
                    is_minimized: false,
                    z_index: 0,
                    is_uwp_frame: false,
                    uwp_app_pid: None,
                }),
                Ok(WindowStateVerdict::Minimized { hwnd, .. }) => {
                    Err(PreCheckError::WindowMinimized {
                        target_hwnd: hwnd,
                        can_auto_restore: true,
                    })
                }
                Ok(WindowStateVerdict::Hidden { hwnd }) => {
                    Err(PreCheckError::WindowHidden { target_hwnd: hwnd })
                }
                Ok(WindowStateVerdict::NotFound { hwnd }) => Err(PreCheckError::WindowNotFound {
                    target_hwnd: hwnd,
                    detail: "Handle not found".into(),
                }),
                Err(e) => Err(PreCheckError::WindowNotFound {
                    target_hwnd: *hwnd,
                    detail: e.to_string(),
                }),
            },
            WindowSelector::Active | WindowSelector::ActiveForeground => {
                #[cfg(windows)]
                {
                    use windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
                    let fg = unsafe { GetForegroundWindow() };
                    if fg != 0 {
                        if let Ok(info) = Box::pin(Self::resolve_window(
                            &WindowSelector::Hwnd(fg as usize as u64),
                            driver,
                        ))
                        .await
                        {
                            return Ok(info);
                        }
                    }
                }
                let windows = driver
                    .list_windows(None)
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                Self::disambiguate_windows(windows, |_| true)
            }
            WindowSelector::Pid(target_pid) => {
                let windows = driver
                    .list_windows(Some(*target_pid))
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                Self::disambiguate_windows(windows, |w| {
                    w.pid == *target_pid || w.uwp_app_pid == Some(*target_pid)
                })
            }
            WindowSelector::ProcessName(name) => {
                let windows = driver
                    .list_windows(None)
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                let target_name = name.to_ascii_lowercase();
                Self::disambiguate_windows(windows, |w| {
                    w.process_name
                        .as_deref()
                        .map(|pn| pn.to_ascii_lowercase() == target_name)
                        .unwrap_or(false)
                })
            }
            WindowSelector::TitleContains(substr) => {
                let sub_lower = substr.to_ascii_lowercase();
                let windows = driver
                    .list_windows(None)
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                Self::disambiguate_windows(windows, |w| {
                    w.title.to_ascii_lowercase().contains(&sub_lower)
                })
            }
            WindowSelector::TitleRegex(pattern) | WindowSelector::TitlePattern(pattern) => {
                let pat_lower = pattern.to_ascii_lowercase();
                let windows = driver
                    .list_windows(None)
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                Self::disambiguate_windows(windows, |w| {
                    w.title.to_ascii_lowercase().contains(&pat_lower)
                })
            }
            WindowSelector::ScreenPoint(pt) => {
                #[cfg(windows)]
                {
                    use windows_sys::Win32::Foundation::POINT;
                    use windows_sys::Win32::UI::WindowsAndMessaging::{
                        GetAncestor, WindowFromPoint, GA_ROOT,
                    };
                    let hit = unsafe { WindowFromPoint(POINT { x: pt.x, y: pt.y }) };
                    if hit != 0 {
                        let root = unsafe { GetAncestor(hit, GA_ROOT) };
                        let hwnd = if root != 0 { root } else { hit };
                        if let Ok(info) = Box::pin(Self::resolve_window(
                            &WindowSelector::Hwnd(hwnd as usize as u64),
                            driver,
                        ))
                        .await
                        {
                            return Ok(info);
                        }
                    }
                }
                let windows = driver
                    .list_windows(None)
                    .await
                    .map_err(|e| PreCheckError::Win32Error(e.to_string()))?;
                Self::disambiguate_windows(windows, |w| w.bounds.contains_point(pt.x, pt.y))
            }
        }
    }

    /// Selects the topmost on-screen window from a candidate set.
    fn disambiguate_windows<F>(
        mut candidates: Vec<CuaWindowInfo>,
        filter: F,
    ) -> Result<CuaWindowInfo, PreCheckError>
    where
        F: Fn(&CuaWindowInfo) -> bool,
    {
        candidates.retain(|w| filter(w));
        if candidates.is_empty() {
            return Err(PreCheckError::WindowNotFound {
                target_hwnd: 0,
                detail: "No matching window found for selector".into(),
            });
        }

        // Sort by: 1. on-screen vs iconic, 2. canonical z-order index ascending (0 = topmost)
        candidates.sort_by_key(|w| (!w.is_on_screen, w.z_index));

        let best = candidates.remove(0);
        if best.is_minimized || best.bounds.is_minimized_sentinel() {
            return Err(PreCheckError::WindowMinimized {
                target_hwnd: best.hwnd,
                can_auto_restore: true,
            });
        }
        if !best.is_on_screen {
            return Err(PreCheckError::WindowHidden {
                target_hwnd: best.hwnd,
            });
        }

        Ok(best)
    }

    /// Translates any TargetCoordinate enum variant into (screen_point, client_point).
    pub fn translate_coordinates(
        coord: &TargetCoordinate,
        window_bounds: &CuaRect,
        client_bounds: &CuaRect,
        expected_roi: Option<&CuaRect>,
    ) -> Result<(CuaPoint, CuaPoint), PreCheckError> {
        let (sx, sy, cx, cy) = match coord {
            TargetCoordinate::ClientNormalized { x, y } | TargetCoordinate::Normalized { x, y } => {
                let cu = x.clamp(0.0, 1.0);
                let cv = y.clamp(0.0, 1.0);
                let max_cx = (client_bounds.width - 1).max(0);
                let max_cy = (client_bounds.height - 1).max(0);
                let cx = ((client_bounds.width as f32 * cu).round() as i32).clamp(0, max_cx);
                let cy = ((client_bounds.height as f32 * cv).round() as i32).clamp(0, max_cy);
                let sx = client_bounds.x + cx;
                let sy = client_bounds.y + cy;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::WindowNormalized { x, y } => {
                let wu = x.clamp(0.0, 1.0);
                let wv = y.clamp(0.0, 1.0);
                let max_wx = (window_bounds.width - 1).max(0);
                let max_wy = (window_bounds.height - 1).max(0);
                let wx = ((window_bounds.width as f32 * wu).round() as i32).clamp(0, max_wx);
                let wy = ((window_bounds.height as f32 * wv).round() as i32).clamp(0, max_wy);
                let sx = window_bounds.x + wx;
                let sy = window_bounds.y + wy;
                let cx = sx - client_bounds.x;
                let cy = sy - client_bounds.y;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::ScreenNormalized { x, y } => {
                let virt = crate::geometry::VirtualDesktopMetrics::query_from_system();
                let su = x.clamp(0.0, 1.0);
                let sv = y.clamp(0.0, 1.0);
                let max_sx = (virt.width - 1).max(0);
                let max_sy = (virt.height - 1).max(0);
                let sx = virt.x + ((virt.width as f32 * su).round() as i32).clamp(0, max_sx);
                let sy = virt.y + ((virt.height as f32 * sv).round() as i32).clamp(0, max_sy);
                let cx = sx - client_bounds.x;
                let cy = sy - client_bounds.y;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::AbsoluteClient(pt) => {
                let cx = pt.x;
                let cy = pt.y;
                let sx = client_bounds.x + cx;
                let sy = client_bounds.y + cy;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::ClientPixel { x, y } => {
                let cx = *x;
                let cy = *y;
                let sx = client_bounds.x + cx;
                let sy = client_bounds.y + cy;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::AbsoluteScreen(pt) => {
                let sx = pt.x;
                let sy = pt.y;
                let cx = sx - client_bounds.x;
                let cy = sy - client_bounds.y;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::ScreenPixel { x, y } => {
                let sx = *x;
                let sy = *y;
                let cx = sx - client_bounds.x;
                let cy = sy - client_bounds.y;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::RoiNormalized {
                roi,
                norm_x,
                norm_y,
            } => {
                let ru = norm_x.clamp(0.0, 1.0);
                let rv = norm_y.clamp(0.0, 1.0);
                let max_rx = (roi.width - 1).max(0);
                let max_ry = (roi.height - 1).max(0);
                let sx = roi.x + ((roi.width as f32 * ru).round() as i32).clamp(0, max_rx);
                let sy = roi.y + ((roi.height as f32 * rv).round() as i32).clamp(0, max_ry);
                let cx = sx - client_bounds.x;
                let cy = sy - client_bounds.y;
                (sx, sy, cx, cy)
            }
            TargetCoordinate::WindowClientCenter | TargetCoordinate::RoiCenter => {
                if let Some(roi) = expected_roi {
                    let (rcx, rcy) = roi.center();
                    let cx = rcx - client_bounds.x;
                    let cy = rcy - client_bounds.y;
                    (rcx, rcy, cx, cy)
                } else {
                    let cx = client_bounds.width / 2;
                    let cy = client_bounds.height / 2;
                    let sx = client_bounds.x + cx;
                    let sy = client_bounds.y + cy;
                    (sx, sy, cx, cy)
                }
            }
        };

        Ok((CuaPoint::new(sx, sy), CuaPoint::new(cx, cy)))
    }

    /// Queries client drawing area in physical screen coordinates.
    pub fn get_client_bounds_in_screen(hwnd: u64, window_bounds: &CuaRect) -> CuaRect {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{HWND, POINT, RECT};
            use windows_sys::Win32::Graphics::Gdi::ClientToScreen;
            use windows_sys::Win32::UI::WindowsAndMessaging::GetClientRect;

            let win_hwnd = hwnd as usize as HWND;
            if win_hwnd != 0 {
                let mut pt = POINT { x: 0, y: 0 };
                let mut rc: RECT = unsafe { std::mem::zeroed() };
                unsafe {
                    if GetClientRect(win_hwnd, &mut rc) != 0
                        && ClientToScreen(win_hwnd, &mut pt) != 0
                    {
                        let w = rc.right - rc.left;
                        let h = rc.bottom - rc.top;
                        if w > 0 && h > 0 {
                            return CuaRect::new(pt.x, pt.y, w, h);
                        }
                    }
                }
            }
        }
        // Arithmetic fallback based on caption and standard frame insets
        crate::security::compute_client_rect(window_bounds)
    }

    /// Hit-tests a point to find deepest child control HWND.
    fn hit_test_point(sx: i32, sy: i32, fallback_hwnd: u64) -> u64 {
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::POINT;
            use windows_sys::Win32::UI::WindowsAndMessaging::WindowFromPoint;
            let hit = unsafe { WindowFromPoint(POINT { x: sx, y: sy }) };
            if hit != 0 {
                return hit as usize as u64;
            }
        }
        let _ = (sx, sy);
        fallback_hwnd
    }
}

/// Deterministic 6-stage pre-execution validation gatekeeper (< 10ms SLA).
pub struct PreCheckPipeline;

impl PreCheckPipeline {
    /// Validates HWND liveness, window state, z-order occlusion, and client coordinate containment.
    pub async fn validate_target_geometry(
        target: &ResolvedTarget,
        driver: &dyn CuaDriverTrait,
    ) -> Result<(), PreCheckError> {
        Self::check_hwnd_liveness(target.hwnd, driver).await?;
        Self::check_window_state(target.hwnd, &target.window_bounds, &target.window_info)?;
        Self::check_zorder_occlusion(
            target.hwnd,
            target.screen_point.x,
            target.screen_point.y,
            driver,
        )
        .await?;
        Self::check_client_containment(
            target.hwnd,
            target.screen_point.x,
            target.screen_point.y,
            target.client_point.x,
            target.client_point.y,
            &target.client_bounds,
            &target.window_bounds,
        )?;
        Ok(())
    }

    /// Executes the full 6-stage deterministic pre-execution validation suite in < 2.8ms.
    pub async fn validate(
        action: &System1Action,
        target: &ResolvedTarget,
        driver: &dyn CuaDriverTrait,
        security_governor: &SecurityGovernor,
        kill_switch: &KillSwitchController,
    ) -> Result<(), PreCheckError> {
        Self::validate_with_secondary(action, target, None, driver, security_governor, kill_switch)
            .await
    }

    /// Validates an action with both primary and optional secondary targets.
    pub async fn validate_with_secondary(
        action: &System1Action,
        target: &ResolvedTarget,
        secondary_target: Option<&ResolvedTarget>,
        driver: &dyn CuaDriverTrait,
        security_governor: &SecurityGovernor,
        kill_switch: &KillSwitchController,
    ) -> Result<(), PreCheckError> {
        // Stage 1: Emergency Kill-Switch Check (< 0.01ms)
        if kill_switch.is_halted() {
            return Err(PreCheckError::EmergencyHalted);
        }

        // Stages 2-5: Validate primary target
        Self::validate_target_geometry(target, driver).await?;

        // Stages 2-5: Validate secondary target (if present)
        if let Some(sec) = secondary_target {
            Self::validate_target_geometry(sec, driver).await?;
        }

        // Stage 6: SecurityGovernor Multi-Tier Evaluation (< 0.50ms)
        Self::check_security_governor(action, target, secondary_target, security_governor)?;

        Ok(())
    }

    async fn check_hwnd_liveness(
        hwnd: u64,
        driver: &dyn CuaDriverTrait,
    ) -> Result<(), PreCheckError> {
        if hwnd == 0 {
            return Err(PreCheckError::WindowNotFound {
                target_hwnd: 0,
                detail: "Window handle is null".into(),
            });
        }

        // Query driver window inspection first
        if let Ok(verdict) = driver.inspect_window(hwnd).await {
            if matches!(verdict, WindowStateVerdict::NotFound { .. }) {
                return Err(PreCheckError::WindowNotFound {
                    target_hwnd: hwnd,
                    detail: "Window handle not found in driver".into(),
                });
            }
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;
            let win_hwnd = hwnd as usize as HWND;
            // If it is suspected to be a live OS window handle (non-zero), verify
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } == 0 {
                // If it is also not in driver's mock list, fail
                if let Ok(verdict) = driver.inspect_window(hwnd).await {
                    if matches!(verdict, WindowStateVerdict::NotFound { .. }) {
                        return Err(PreCheckError::WindowNotFound {
                            target_hwnd: hwnd,
                            detail: "Win32 handle is invalid or window was destroyed".into(),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    fn check_window_state(
        hwnd: u64,
        bounds: &CuaRect,
        window_info: &CuaWindowInfo,
    ) -> Result<(), PreCheckError> {
        if bounds.is_minimized_sentinel() || window_info.is_minimized {
            return Err(PreCheckError::WindowMinimized {
                target_hwnd: hwnd,
                can_auto_restore: true,
            });
        }
        if bounds.is_empty() {
            return Err(PreCheckError::WindowEmptyBounds {
                target_hwnd: hwnd,
                bounds: *bounds,
            });
        }
        if !window_info.is_on_screen {
            return Err(PreCheckError::WindowHidden { target_hwnd: hwnd });
        }

        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsIconic, IsWindow, IsWindowVisible,
            };

            let win_hwnd = hwnd as usize as HWND;
            // Only query Win32 OS APIs if this is a genuine live OS window
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } != 0 {
                unsafe {
                    if IsIconic(win_hwnd) != 0 {
                        return Err(PreCheckError::WindowMinimized {
                            target_hwnd: hwnd,
                            can_auto_restore: true,
                        });
                    }
                    if IsWindowVisible(win_hwnd) == 0 {
                        return Err(PreCheckError::WindowHidden { target_hwnd: hwnd });
                    }

                    let mut cloaked: u32 = 0;
                    let hr = DwmGetWindowAttribute(
                        win_hwnd,
                        DWMWA_CLOAKED as u32,
                        &mut cloaked as *mut u32 as *mut _,
                        std::mem::size_of::<u32>() as u32,
                    );
                    if hr == 0 && cloaked != 0 {
                        return Err(PreCheckError::WindowCloaked { target_hwnd: hwnd });
                    }
                }
            }
        }
        Ok(())
    }

    async fn check_zorder_occlusion(
        target_hwnd: u64,
        sx: i32,
        sy: i32,
        driver: &dyn CuaDriverTrait,
    ) -> Result<(), PreCheckError> {
        // 1. Check driver's window list for overlapping windows with lower z-index (higher in z-order)
        if let Ok(windows) = driver.list_windows(None).await {
            let target_z = windows
                .iter()
                .find(|w| w.hwnd == target_hwnd)
                .map(|w| w.z_index)
                .unwrap_or(usize::MAX);

            for w in windows {
                if w.hwnd != target_hwnd
                    && w.is_on_screen
                    && !w.is_minimized
                    && w.z_index < target_z
                {
                    if w.bounds.contains_point(sx, sy) {
                        return Err(PreCheckError::TargetOccluded {
                            target_hwnd,
                            target_point: CuaPoint::new(sx, sy),
                            occluded_by_hwnd: w.hwnd,
                            occluded_by_process: w.process_name,
                            occluded_by_title: Some(w.title),
                        });
                    }
                }
            }
        }

        // 2. On live Windows, verify via WindowFromPoint and GetAncestor if target is a live window
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{HWND, POINT};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                GetAncestor, GetWindowTextLengthW, GetWindowTextW, GetWindowThreadProcessId,
                IsWindow, WindowFromPoint, GA_ROOT,
            };

            let win_hwnd = target_hwnd as usize as HWND;
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } != 0 {
                let hit = unsafe { WindowFromPoint(POINT { x: sx, y: sy }) };
                if hit != 0 {
                    let root_hit = unsafe { GetAncestor(hit, GA_ROOT) };
                    let effective_hit = if root_hit != 0 { root_hit } else { hit };

                    if (effective_hit as usize as u64) != target_hwnd
                        && (hit as usize as u64) != target_hwnd
                    {
                        let mut occluder_pid = 0u32;
                        unsafe { GetWindowThreadProcessId(effective_hit, &mut occluder_pid) };
                        let proc_name = crate::window::get_process_name(occluder_pid);

                        let title_len = unsafe { GetWindowTextLengthW(effective_hit) };
                        let title = if title_len > 0 {
                            let mut buf = vec![0u16; (title_len + 1) as usize];
                            let copied = unsafe {
                                GetWindowTextW(effective_hit, buf.as_mut_ptr(), buf.len() as i32)
                            };
                            String::from_utf16_lossy(&buf[..copied as usize])
                                .trim()
                                .to_string()
                        } else {
                            String::new()
                        };

                        return Err(PreCheckError::TargetOccluded {
                            target_hwnd,
                            target_point: CuaPoint::new(sx, sy),
                            occluded_by_hwnd: effective_hit as usize as u64,
                            occluded_by_process: proc_name,
                            occluded_by_title: Some(title),
                        });
                    }
                }
            }
        }

        Ok(())
    }

    fn check_client_containment(
        hwnd: u64,
        sx: i32,
        sy: i32,
        cx: i32,
        cy: i32,
        client_bounds: &CuaRect,
        window_bounds: &CuaRect,
    ) -> Result<(), PreCheckError> {
        // 1. Close Button protection zone: top-right corner of window
        if sx >= window_bounds.x + window_bounds.width - 50 && sy < window_bounds.y + 40 {
            return Err(PreCheckError::OutOfBounds {
                client_x: cx,
                client_y: cy,
                client_bounds: *client_bounds,
                reason: OutOfBoundsReason::CloseButtonProtected,
            });
        }

        // 2. Must be inside client drawing area
        if cx < 0 || cy < 0 || cx >= client_bounds.width || cy >= client_bounds.height {
            let reason = if cy < 0 {
                OutOfBoundsReason::TitleBarCaptionProtected
            } else {
                OutOfBoundsReason::OutsideClientDrawingSurface
            };
            return Err(PreCheckError::OutOfBounds {
                client_x: cx,
                client_y: cy,
                client_bounds: *client_bounds,
                reason,
            });
        }

        // 3. Live Win32 WM_NCHITTEST check if window is genuine live window
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{HWND, WPARAM};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindow, SendMessageW, HTCLIENT, WM_NCHITTEST,
            };

            let win_hwnd = hwnd as usize as HWND;
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } != 0 {
                let lparam = crate::geometry::pack_lparam_clamped(sx, sy);
                let hit_code = unsafe { SendMessageW(win_hwnd, WM_NCHITTEST, 0 as WPARAM, lparam) };
                if hit_code != 0 && hit_code != (HTCLIENT as isize) {
                    return Err(PreCheckError::OutOfBounds {
                        client_x: cx,
                        client_y: cy,
                        client_bounds: *client_bounds,
                        reason: OutOfBoundsReason::NonClientHitTest,
                    });
                }
            }
        }
        let _ = hwnd;

        Ok(())
    }

    fn check_security_governor(
        action: &System1Action,
        target: &ResolvedTarget,
        secondary_target: Option<&ResolvedTarget>,
        governor: &SecurityGovernor,
    ) -> Result<(), PreCheckError> {
        let cua_action = match action {
            System1Action::Click {
                button,
                click_count,
                delivery_mode,
                ..
            } => CuaAction::Click {
                target_hwnd: target.hwnd,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
                button: *button,
                click_count: *click_count,
                delivery_mode: delivery_mode.unwrap_or(CuaDeliveryMode::Background),
            },
            System1Action::DoubleClick { .. } => CuaAction::Click {
                target_hwnd: target.hwnd,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
                button: MouseButton::Left,
                click_count: 2,
                delivery_mode: CuaDeliveryMode::Background,
            },
            System1Action::Hover { .. } => CuaAction::MoveCursor {
                target_hwnd: target.hwnd,
                x: target.screen_point.x,
                y: target.screen_point.y,
                delivery_mode: CuaDeliveryMode::Background,
            },
            System1Action::Drag {
                button,
                steps,
                delivery_mode,
                ..
            } => {
                let (end_x, end_y) = if let Some(sec) = secondary_target {
                    (sec.screen_point.x, sec.screen_point.y)
                } else {
                    (target.screen_point.x, target.screen_point.y)
                };
                CuaAction::Drag {
                    target_hwnd: target.hwnd,
                    start_x: target.screen_point.x,
                    start_y: target.screen_point.y,
                    end_x,
                    end_y,
                    button: button.unwrap_or(MouseButton::Left),
                    steps: *steps,
                    delivery_mode: delivery_mode.unwrap_or(CuaDeliveryMode::Background),
                }
            }
            System1Action::Scroll {
                delta_x, delta_y, ..
            } => CuaAction::Scroll {
                target_hwnd: target.hwnd,
                delta_x: *delta_x,
                delta_y: *delta_y,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
            },
            System1Action::TypeText {
                text,
                press_enter,
                delivery_mode,
                ..
            } => {
                let mut final_text = text.clone();
                if *press_enter && !final_text.ends_with('\n') && !final_text.ends_with('\r') {
                    final_text.push('\n');
                }
                CuaAction::TypeText {
                    target_hwnd: target.hwnd,
                    text: final_text,
                    delivery_mode: delivery_mode.unwrap_or(CuaDeliveryMode::Background),
                }
            }
            System1Action::Hotkey {
                keys,
                delivery_mode,
                ..
            } => CuaAction::Hotkey {
                target_hwnd: target.hwnd,
                keys: keys.clone(),
                delivery_mode: delivery_mode.unwrap_or(CuaDeliveryMode::Background),
            },
            System1Action::BringToFront { .. } => CuaAction::BringToFront {
                target_hwnd: target.hwnd,
            },
            System1Action::RestoreWindow { .. } => CuaAction::RestoreWindow {
                target_hwnd: target.hwnd,
            },
            System1Action::WaitVisualChange { .. } => return Ok(()),
        };

        governor
            .evaluate_action(&cua_action, &target.window_info)
            .map_err(|err| match err {
                CuaError::PermissionDenied(reason) => PreCheckError::SecurityRefused {
                    reason,
                    policy_rule: SecurityPolicyRule::ProcessNotAllowlisted,
                },
                CuaError::DangerousHotkeyRejected(keys) => PreCheckError::SecurityRefused {
                    reason: format!("Dangerous hotkey rejected: {keys}"),
                    policy_rule: SecurityPolicyRule::DangerousHotkeyBlocked,
                },
                CuaError::UipiBlocked(reason) => PreCheckError::SecurityRefused {
                    reason,
                    policy_rule: SecurityPolicyRule::UipiIntegrityElevation,
                },
                CuaError::CoordinateOutOfBounds { x, y } => PreCheckError::OutOfBounds {
                    client_x: x,
                    client_y: y,
                    client_bounds: target.client_bounds,
                    reason: OutOfBoundsReason::OutsideClientDrawingSurface,
                },
                _ => PreCheckError::SecurityRefused {
                    reason: err.to_string(),
                    policy_rule: SecurityPolicyRule::StandardModeReadOnly,
                },
            })?;

        // If secondary target belongs to a different window, validate destination window against governor
        if let Some(sec) = secondary_target {
            if sec.hwnd != target.hwnd {
                governor
                    .evaluate_action(&cua_action, &sec.window_info)
                    .map_err(|err| match err {
                        CuaError::PermissionDenied(reason) => PreCheckError::SecurityRefused {
                            reason,
                            policy_rule: SecurityPolicyRule::ProcessNotAllowlisted,
                        },
                        CuaError::CoordinateOutOfBounds { x, y } => PreCheckError::OutOfBounds {
                            client_x: x,
                            client_y: y,
                            client_bounds: sec.client_bounds,
                            reason: OutOfBoundsReason::OutsideClientDrawingSurface,
                        },
                        _ => PreCheckError::SecurityRefused {
                            reason: err.to_string(),
                            policy_rule: SecurityPolicyRule::StandardModeReadOnly,
                        },
                    })?;
            }
        }

        Ok(())
    }
}

/// System-1 Fast-Loop Action Router coordinating target translation,
/// deterministic pre-execution checks, delivery fallback escalation,
/// and structured System-2 defect reporting.
pub struct System1ActionRouter {
    engine: Arc<CuaEngine>,
    driver: Arc<dyn CuaDriverTrait>,
    security_governor: Arc<SecurityGovernor>,
    kill_switch: Arc<KillSwitchController>,
    audit_recorder: Arc<CuaAuditRecorder>,
    visual_verifier: Arc<dyn VisualVerifierTrait>,
    config: System1RouterConfig,
}

impl System1ActionRouter {
    /// Initialize with dependencies.
    pub fn new(
        engine: Arc<CuaEngine>,
        driver: Arc<dyn CuaDriverTrait>,
        visual_verifier: Arc<dyn VisualVerifierTrait>,
        config: System1RouterConfig,
    ) -> Self {
        let security_governor = engine.security_governor().clone();
        let kill_switch = engine.kill_switch().clone();
        let audit_recorder = engine.audit_recorder().clone();

        Self {
            engine,
            driver,
            security_governor,
            kill_switch,
            audit_recorder,
            visual_verifier,
            config,
        }
    }

    /// Access the underlying CuaEngine.
    pub fn engine(&self) -> &Arc<CuaEngine> {
        &self.engine
    }

    /// Access the underlying CuaDriverTrait.
    pub fn driver(&self) -> &Arc<dyn CuaDriverTrait> {
        &self.driver
    }

    /// Access the underlying SecurityGovernor.
    pub fn security_governor(&self) -> &Arc<SecurityGovernor> {
        &self.security_governor
    }

    /// Access the underlying KillSwitchController.
    pub fn kill_switch(&self) -> &Arc<KillSwitchController> {
        &self.kill_switch
    }

    /// Access the underlying CuaAuditRecorder.
    pub fn audit_recorder(&self) -> &Arc<CuaAuditRecorder> {
        &self.audit_recorder
    }

    /// Access the underlying VisualVerifierTrait.
    pub fn visual_verifier(&self) -> &Arc<dyn VisualVerifierTrait> {
        &self.visual_verifier
    }

    /// Access the router configuration.
    pub fn config(&self) -> &System1RouterConfig {
        &self.config
    }

    /// Execute a simple sequence of System1Action envelopes returning MacroPlanResult or System1DefectReport.
    pub async fn execute_plan(
        &self,
        actions: Vec<System1Action>,
    ) -> Result<System1PlanResult, System1DefectReport> {
        let plan = MacroPlan::from_actions(actions);
        let result = self.execute_macro_plan(plan).await;
        if result.success {
            Ok(result)
        } else {
            Err(result.defect_report.unwrap_or_else(|| {
                self.build_defect_report(
                    &result.session_id,
                    Some(&result.plan_id),
                    result.completed_steps,
                    result.total_steps,
                    result.completed_steps,
                    System1Action::Hover {
                        target: TargetDescriptor::new(
                            WindowSelector::Hwnd(0),
                            TargetCoordinate::default(),
                        ),
                        duration_ms: None,
                    },
                    DefectCategory::DriverError,
                    None,
                    None,
                    0,
                    vec![],
                    "plan_failed",
                    "Macro plan execution failed without specific defect report",
                    RemedyHint::ReacquireTargetCoordinates("Plan failed".into()),
                    None,
                    true,
                )
            }))
        }
    }

    /// Execute a multi-step macro plan deterministically in Rust with 0 intermediate LLM tokens.
    pub async fn execute_macro_plan(&self, plan: MacroPlan) -> MacroPlanResult {
        let total_start = Instant::now();
        let total_steps = plan.steps.len();
        let mut completed_steps = 0;
        let mut step_results = Vec::with_capacity(total_steps);

        for (idx, action) in plan.steps.into_iter().enumerate() {
            // Check Emergency Halt at start of each step (< 15ms SLA)
            if self.kill_switch.is_halted() {
                let defect = self.build_defect_report(
                    &plan.session_id,
                    Some(&plan.plan_id),
                    idx,
                    total_steps,
                    completed_steps,
                    action,
                    DefectCategory::KillSwitchAborted,
                    None,
                    None,
                    0,
                    vec![],
                    "emergency_halt",
                    "Emergency kill-switch active; macro plan aborted",
                    RemedyHint::UserInterventionNeeded(
                        "Kill-switch triggered by user or hardware thrash".into(),
                    ),
                    None,
                    false,
                );
                return MacroPlanResult {
                    plan_id: plan.plan_id,
                    session_id: plan.session_id,
                    total_steps,
                    completed_steps,
                    success: false,
                    step_results,
                    defect_report: Some(defect),
                    total_duration_ms: total_start.elapsed().as_millis() as u64,
                    tokens_consumed: 0,
                };
            }

            // Execute single step with delivery fallback and deterministic retries
            match self
                .execute_step_with_escalation(
                    &plan.session_id,
                    Some(&plan.plan_id),
                    idx,
                    total_steps,
                    completed_steps,
                    action.clone(),
                    plan.max_step_retries.min(self.config.max_step_retries),
                    plan.allow_foreground_escalation && self.config.allow_foreground_escalation,
                )
                .await
            {
                Ok(step_res) => {
                    completed_steps += 1;
                    step_results.push(step_res);
                }
                Err(defect_report) => {
                    return MacroPlanResult {
                        plan_id: plan.plan_id,
                        session_id: plan.session_id,
                        total_steps,
                        completed_steps,
                        success: false,
                        step_results,
                        defect_report: Some(defect_report),
                        total_duration_ms: total_start.elapsed().as_millis() as u64,
                        tokens_consumed: 0,
                    };
                }
            }
        }

        MacroPlanResult {
            plan_id: plan.plan_id,
            session_id: plan.session_id,
            total_steps,
            completed_steps,
            success: true,
            step_results,
            defect_report: None,
            total_duration_ms: total_start.elapsed().as_millis() as u64,
            tokens_consumed: 0,
        }
    }

    /// Execute a single atomic action step with fallback escalation and retries.
    #[allow(clippy::too_many_arguments)]
    pub async fn execute_step_with_escalation(
        &self,
        session_id: &str,
        plan_id: Option<&str>,
        step_index: usize,
        total_steps: usize,
        completed_steps: usize,
        action: System1Action,
        max_retries: u32,
        allow_foreground: bool,
    ) -> Result<System1StepResult, System1DefectReport> {
        let step_start = Instant::now();
        let mut retries_taken = 0;
        let mut delivery_modes_attempted = Vec::new();
        let mut current_delivery = CuaDeliveryMode::Background;
        let mut escalated_from_background = false;

        // 1. Resolve Target & Deterministic Pre-Execution Checks (< 2.8ms)
        let targets = match self.resolve_and_precheck(&action).await {
            Ok(t) => t,
            Err(precheck_err) => {
                let (category, remedy) = self.classify_precheck_error(&precheck_err);
                return Err(self.build_defect_report(
                    session_id,
                    plan_id,
                    step_index,
                    total_steps,
                    completed_steps,
                    action,
                    category,
                    Some(precheck_err.to_string()),
                    None,
                    0,
                    vec![],
                    precheck_err.error_code(),
                    &precheck_err.to_string(),
                    remedy,
                    None,
                    true,
                ));
            }
        };

        let resolved = targets.primary;
        let secondary = targets.secondary;
        let target_roi = resolved.expected_roi.unwrap_or(resolved.window_bounds);

        // 2. Specialized handling for non-actuating visual synchronization: WaitVisualChange
        if let System1Action::WaitVisualChange { .. } = &action {
            if self.kill_switch.is_halted() {
                return Err(self.build_defect_report(
                    session_id,
                    plan_id,
                    step_index,
                    total_steps,
                    completed_steps,
                    action,
                    DefectCategory::KillSwitchAborted,
                    None,
                    None,
                    0,
                    vec![],
                    "emergency_halt",
                    "Kill-switch triggered before visual wait",
                    RemedyHint::UserInterventionNeeded(
                        "Halted by Esc or physical mouse displacement".into(),
                    ),
                    Some(resolved.window_info.clone()),
                    false,
                ));
            }

            let baseline_frame = match self.visual_verifier.capture_baseline().await {
                Ok(frame) => frame,
                Err(e) => {
                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::DriverError,
                        Some(e.to_string()),
                        None,
                        0,
                        vec![],
                        "baseline_capture_failed",
                        "Failed to capture baseline frame for WaitVisualChange",
                        RemedyHint::AdjustVisualThreshold(
                            "Baseline capture failed before polling".into(),
                        ),
                        Some(resolved.window_info.clone()),
                        true,
                    ));
                }
            };

            let criteria = self.determine_criteria(&action);

            // Execute asynchronous polling loop directly without low-level OS input dispatch
            let diff_res = self
                .visual_verifier
                .wait_visual_change(&target_roi, &criteria, &baseline_frame)
                .await;

            match diff_res {
                Ok(diff) if diff.diff_detected => {
                    return Ok(System1StepResult {
                        step_index,
                        action,
                        success: true,
                        delivery_used: CuaDeliveryMode::Background,
                        escalated_from_background: false,
                        retries_taken: 0,
                        diff_result: Some(diff),
                        duration_ms: step_start.elapsed().as_millis() as u64,
                    });
                }
                Ok(diff) => {
                    // Polling timeout expired without detecting expected visual change
                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::VisualVerificationZeroDiff,
                        None,
                        Some(diff),
                        0,
                        vec![CuaDeliveryMode::Background],
                        "visual_wait_timeout",
                        "WaitVisualChange polling loop expired without detecting expected visual change",
                        RemedyHint::AdjustVisualThreshold(
                            "Visual change wait timed out without detecting expected UI transition. Verify timeout or lower min_changed_ratio.".into(),
                        ),
                        Some(resolved.window_info.clone()),
                        true,
                    ));
                }
                Err(err) => {
                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::DriverError,
                        Some(err.to_string()),
                        None,
                        0,
                        vec![CuaDeliveryMode::Background],
                        err.error_code(),
                        &err.to_string(),
                        RemedyHint::AdjustVisualThreshold(
                            "Visual verifier encountered an internal error during polling".into(),
                        ),
                        Some(resolved.window_info.clone()),
                        true,
                    ));
                }
            }
        }

        // 3. Retry loop: Attempt 0 = Background, Attempt 1 = Foreground escalation (if 0% diff)
        while retries_taken <= max_retries {
            // Check Emergency Halt before every dispatch (< 15ms SLA)
            if self.kill_switch.is_halted() {
                return Err(self.build_defect_report(
                    session_id,
                    plan_id,
                    step_index,
                    total_steps,
                    completed_steps,
                    action,
                    DefectCategory::KillSwitchAborted,
                    None,
                    None,
                    retries_taken,
                    delivery_modes_attempted,
                    "emergency_halt",
                    "Kill-switch triggered mid-execution",
                    RemedyHint::UserInterventionNeeded(
                        "Halted by Esc or physical mouse displacement".into(),
                    ),
                    Some(resolved.window_info.clone()),
                    false,
                ));
            }

            delivery_modes_attempted.push(current_delivery);

            // If escalated to Foreground, ensure target window is brought to front
            if current_delivery == CuaDeliveryMode::Foreground {
                let bring_action = CuaAction::BringToFront {
                    target_hwnd: resolved.hwnd,
                };
                let _ = self.driver.dispatch_action(bring_action).await;
                tokio::time::sleep(Duration::from_millis(25)).await;
            }

            // 4. If TypeText specifies clear_before: true, dispatch Select-All (Ctrl+A) and Backspace
            if let System1Action::TypeText {
                clear_before: true, ..
            } = &action
            {
                let select_all = CuaAction::Hotkey {
                    target_hwnd: resolved.hwnd,
                    keys: vec!["ctrl".to_string(), "a".to_string()],
                    delivery_mode: current_delivery,
                };
                let _ = self.engine.execute_action(select_all).await;
                tokio::time::sleep(Duration::from_millis(15)).await;

                let backspace = CuaAction::PressKey {
                    target_hwnd: resolved.hwnd,
                    key: "backspace".to_string(),
                    down: true,
                    delivery_mode: current_delivery,
                };
                let _ = self.engine.execute_action(backspace).await;

                let backspace_up = CuaAction::PressKey {
                    target_hwnd: resolved.hwnd,
                    key: "backspace".to_string(),
                    down: false,
                    delivery_mode: current_delivery,
                };
                let _ = self.engine.execute_action(backspace_up).await;
                tokio::time::sleep(Duration::from_millis(15)).await;
            }

            // 5. Translate to executable low-level CuaAction
            let cua_action = match self.translate_to_cua_action(
                &action,
                &resolved,
                secondary.as_ref(),
                current_delivery,
            ) {
                Some(act) => act,
                None => {
                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::DriverError,
                        None,
                        None,
                        retries_taken,
                        delivery_modes_attempted,
                        "invalid_action_translation",
                        "Action has no corresponding OS actuation mapping",
                        RemedyHint::ReacquireTargetCoordinates(
                            "Unsupported action translation".into(),
                        ),
                        Some(resolved.window_info.clone()),
                        true,
                    ));
                }
            };

            // 6. Capture baseline frame prior to dispatch
            let baseline_frame = self.visual_verifier.capture_baseline().await.ok();

            // 7. Dispatch action via CuaEngine
            let dispatch_res = self.engine.execute_action(cua_action).await;

            match dispatch_res {
                Ok(_) => {
                    // If Hover specifies duration_ms, observe dwell with emergency halt polling
                    if let System1Action::Hover {
                        duration_ms: Some(dwell),
                        ..
                    } = &action
                    {
                        if *dwell > 0 {
                            let dwell_total = *dwell as u64;
                            let mut elapsed = 0u64;
                            while elapsed < dwell_total {
                                if self.kill_switch.is_halted() {
                                    return Err(self.build_defect_report(
                                        session_id,
                                        plan_id,
                                        step_index,
                                        total_steps,
                                        completed_steps,
                                        action,
                                        DefectCategory::KillSwitchAborted,
                                        None,
                                        None,
                                        retries_taken,
                                        delivery_modes_attempted,
                                        "emergency_halt",
                                        "Kill-switch triggered during hover dwell",
                                        RemedyHint::UserInterventionNeeded(
                                            "Halted by Esc or physical mouse displacement during hover dwell".into(),
                                        ),
                                        Some(resolved.window_info.clone()),
                                        false,
                                    ));
                                }
                                let step_ms = (dwell_total - elapsed).min(20);
                                tokio::time::sleep(Duration::from_millis(step_ms)).await;
                                elapsed += step_ms;
                            }
                        }
                    }

                    // 8. Evaluate Post-Execution Visual Verification (< 45ms total)
                    let criteria = self.determine_criteria(&action);
                    let diff_res = self
                        .visual_verifier
                        .verify_action(&target_roi, &criteria, baseline_frame.as_ref())
                        .await;

                    match diff_res {
                        Ok(diff) => {
                            if diff.diff_detected {
                                // Visual change confirmed! Step succeeds.
                                return Ok(System1StepResult {
                                    step_index,
                                    action,
                                    success: true,
                                    delivery_used: current_delivery,
                                    escalated_from_background,
                                    retries_taken,
                                    diff_result: Some(diff),
                                    duration_ms: step_start.elapsed().as_millis() as u64,
                                });
                            }

                            // 0% Visual Diff Detected: Evaluate Delivery Fallback Escalation
                            if current_delivery == CuaDeliveryMode::Background && allow_foreground {
                                let perm_mode = self.security_governor.get_permission_mode();
                                if perm_mode == PermissionMode::Standard {
                                    return Err(self.build_defect_report(
                                        session_id,
                                        plan_id,
                                        step_index,
                                        total_steps,
                                        completed_steps,
                                        action,
                                        DefectCategory::SecurityRefused,
                                        None,
                                        Some(diff),
                                        retries_taken,
                                        delivery_modes_attempted,
                                        "standard_mode_read_only",
                                        "Foreground escalation forbidden in Standard read-only mode",
                                        RemedyHint::SecurityPolicyViolation(
                                            "Standard mode prohibits desktop actuation".into(),
                                        ),
                                        Some(resolved.window_info.clone()),
                                        false,
                                    ));
                                }

                                info!(
                                    "System-1: 0% diff on background delivery for step {}. Escalating to Foreground (retry {}).",
                                    step_index,
                                    retries_taken + 1
                                );
                                current_delivery = CuaDeliveryMode::Foreground;
                                escalated_from_background = true;
                                retries_taken += 1;
                                continue;
                            }

                            // If already Foreground or escalation not allowed, retry if budget remains
                            if retries_taken < max_retries {
                                retries_taken += 1;
                                tokio::time::sleep(Duration::from_millis(30)).await;
                                continue;
                            }

                            // Retries exhausted with 0% diff: generate Defect Report
                            return Err(self.build_defect_report(
                                session_id,
                                plan_id,
                                step_index,
                                total_steps,
                                completed_steps,
                                action,
                                DefectCategory::VisualVerificationZeroDiff,
                                None,
                                Some(diff),
                                retries_taken,
                                delivery_modes_attempted,
                                "zero_visual_diff",
                                "Zero visual diff detected after background and foreground attempts",
                                RemedyHint::AdjustVisualThreshold(
                                    "Target element showed no visual feedback. Verify coordinates or element active state.".into(),
                                ),
                                Some(resolved.window_info.clone()),
                                true,
                            ));
                        }
                        Err(diff_err) => {
                            warn!(
                                "Visual verification error on step {}: {}",
                                step_index, diff_err
                            );
                            // Fall back to dispatch success if verifier itself failed
                            return Ok(System1StepResult {
                                step_index,
                                action,
                                success: true,
                                delivery_used: current_delivery,
                                escalated_from_background,
                                retries_taken,
                                diff_result: None,
                                duration_ms: step_start.elapsed().as_millis() as u64,
                            });
                        }
                    }
                }
                Err(CuaError::EmergencyHalted) => {
                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::KillSwitchAborted,
                        None,
                        None,
                        retries_taken,
                        delivery_modes_attempted,
                        "emergency_halt",
                        "Execution aborted by emergency kill-switch",
                        RemedyHint::UserInterventionNeeded("Kill-switch halted execution".into()),
                        Some(resolved.window_info.clone()),
                        false,
                    ));
                }
                Err(err) => {
                    // Check if unrecoverable error
                    if self.is_unrecoverable(&err) {
                        let (cat, rem) = self.classify_cua_error(&err);
                        return Err(self.build_defect_report(
                            session_id,
                            plan_id,
                            step_index,
                            total_steps,
                            completed_steps,
                            action,
                            cat,
                            Some(err.to_string()),
                            None,
                            retries_taken,
                            delivery_modes_attempted,
                            err.error_code(),
                            &err.to_string(),
                            rem,
                            Some(resolved.window_info.clone()),
                            true,
                        ));
                    }

                    // Retryable driver error
                    if retries_taken < max_retries {
                        retries_taken += 1;
                        tokio::time::sleep(Duration::from_millis(50)).await;
                        continue;
                    }

                    return Err(self.build_defect_report(
                        session_id,
                        plan_id,
                        step_index,
                        total_steps,
                        completed_steps,
                        action,
                        DefectCategory::DriverError,
                        Some(err.to_string()),
                        None,
                        retries_taken,
                        delivery_modes_attempted,
                        err.error_code(),
                        &err.to_string(),
                        RemedyHint::ReacquireTargetCoordinates(
                            "Driver error during input dispatch".into(),
                        ),
                        Some(resolved.window_info.clone()),
                        true,
                    ));
                }
            }
        }

        Err(self.build_defect_report(
            session_id,
            plan_id,
            step_index,
            total_steps,
            completed_steps,
            action,
            DefectCategory::DriverError,
            None,
            None,
            retries_taken,
            delivery_modes_attempted,
            "retries_exhausted",
            "Maximum retries exhausted without visual change",
            RemedyHint::ReacquireTargetCoordinates("Retries exhausted".into()),
            Some(resolved.window_info.clone()),
            true,
        ))
    }

    /// Resolve TargetDescriptor and execute deterministic pre-execution checks (< 2.8ms).
    async fn resolve_and_precheck(
        &self,
        action: &System1Action,
    ) -> Result<ResolvedTargets, PreCheckError> {
        match action {
            System1Action::Drag { start, end, .. } => {
                let start_res = TargetResolver::resolve(start, self.driver.as_ref()).await?;
                let end_res = TargetResolver::resolve(end, self.driver.as_ref()).await?;

                PreCheckPipeline::validate_with_secondary(
                    action,
                    &start_res,
                    Some(&end_res),
                    self.driver.as_ref(),
                    &self.security_governor,
                    &self.kill_switch,
                )
                .await?;

                Ok(ResolvedTargets {
                    primary: start_res,
                    secondary: Some(end_res),
                })
            }
            _ => {
                let descriptor_opt = match action {
                    System1Action::Click { target, .. }
                    | System1Action::DoubleClick { target }
                    | System1Action::Hover { target, .. }
                    | System1Action::WaitVisualChange { target, .. }
                    | System1Action::BringToFront { target }
                    | System1Action::RestoreWindow { target } => Some(target),
                    System1Action::Scroll { target, .. }
                    | System1Action::TypeText { target, .. }
                    | System1Action::Hotkey { target, .. } => target.as_ref(),
                    System1Action::Drag { .. } => unreachable!(),
                };

                let resolved = if let Some(desc) = descriptor_opt {
                    TargetResolver::resolve(desc, self.driver.as_ref()).await?
                } else {
                    // Target is omitted; resolve active foreground window
                    let info = TargetResolver::resolve_window(
                        &WindowSelector::Active,
                        self.driver.as_ref(),
                    )
                    .await?;
                    let client_bounds =
                        TargetResolver::get_client_bounds_in_screen(info.hwnd, &info.bounds);
                    let (sx, sy) = info.bounds.center();
                    let cx = sx - client_bounds.x;
                    let cy = sy - client_bounds.y;
                    ResolvedTarget {
                        hwnd: info.hwnd,
                        pid: info.pid,
                        process_name: info.process_name.clone(),
                        title: info.title.clone(),
                        window_bounds: info.bounds,
                        client_bounds,
                        screen_point: CuaPoint::new(sx, sy),
                        client_point: CuaPoint::new(cx, cy),
                        hit_control_hwnd: info.hwnd,
                        expected_roi: None,
                        semantic_label: None,
                        window_info: info,
                    }
                };

                // Execute full 6-stage deterministic pre-execution checks
                PreCheckPipeline::validate_with_secondary(
                    action,
                    &resolved,
                    None,
                    self.driver.as_ref(),
                    &self.security_governor,
                    &self.kill_switch,
                )
                .await?;

                Ok(ResolvedTargets {
                    primary: resolved,
                    secondary: None,
                })
            }
        }
    }

    /// Translate System1Action to low-level CuaAction with specified delivery mode.
    fn translate_to_cua_action(
        &self,
        action: &System1Action,
        target: &ResolvedTarget,
        secondary: Option<&ResolvedTarget>,
        delivery_mode: CuaDeliveryMode,
    ) -> Option<CuaAction> {
        match action {
            System1Action::Click {
                button,
                click_count,
                ..
            } => Some(CuaAction::Click {
                target_hwnd: target.hwnd,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
                button: *button,
                click_count: *click_count,
                delivery_mode,
            }),
            System1Action::DoubleClick { .. } => Some(CuaAction::Click {
                target_hwnd: target.hwnd,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
                button: MouseButton::Left,
                click_count: 2,
                delivery_mode,
            }),
            System1Action::Hover { .. } => Some(CuaAction::MoveCursor {
                target_hwnd: target.hwnd,
                x: target.screen_point.x,
                y: target.screen_point.y,
                delivery_mode,
            }),
            System1Action::Scroll {
                delta_x, delta_y, ..
            } => Some(CuaAction::Scroll {
                target_hwnd: target.hwnd,
                delta_x: *delta_x,
                delta_y: *delta_y,
                x: Some(target.screen_point.x),
                y: Some(target.screen_point.y),
            }),
            System1Action::TypeText {
                text, press_enter, ..
            } => {
                let mut final_text = text.clone();
                if *press_enter && !final_text.ends_with('\n') && !final_text.ends_with('\r') {
                    final_text.push('\n');
                }
                Some(CuaAction::TypeText {
                    target_hwnd: target.hwnd,
                    text: final_text,
                    delivery_mode,
                })
            }
            System1Action::Hotkey { keys, .. } => Some(CuaAction::Hotkey {
                target_hwnd: target.hwnd,
                keys: keys.clone(),
                delivery_mode,
            }),
            System1Action::Drag { steps, button, .. } => {
                let (end_x, end_y) = if let Some(sec) = secondary {
                    (sec.screen_point.x, sec.screen_point.y)
                } else {
                    (target.screen_point.x, target.screen_point.y)
                };
                Some(CuaAction::Drag {
                    target_hwnd: target.hwnd,
                    start_x: target.screen_point.x,
                    start_y: target.screen_point.y,
                    end_x,
                    end_y,
                    button: button.unwrap_or(MouseButton::Left),
                    steps: *steps,
                    delivery_mode,
                })
            }
            System1Action::BringToFront { .. } => Some(CuaAction::BringToFront {
                target_hwnd: target.hwnd,
            }),
            System1Action::RestoreWindow { .. } => Some(CuaAction::RestoreWindow {
                target_hwnd: target.hwnd,
            }),
            System1Action::WaitVisualChange { .. } => None,
        }
    }

    /// Select appropriate SemanticVerificationCriteria based on action type.
    fn determine_criteria(&self, action: &System1Action) -> SemanticVerificationCriteria {
        match action {
            System1Action::Click { .. } | System1Action::DoubleClick { .. } => {
                SemanticVerificationCriteria::click_default()
            }
            System1Action::TypeText { .. } => SemanticVerificationCriteria::type_text_default(),
            System1Action::Scroll { .. } => SemanticVerificationCriteria::scroll_default(),
            System1Action::WaitVisualChange {
                timeout_ms,
                min_changed_ratio,
                ..
            } => SemanticVerificationCriteria::WaitVisualChange {
                min_changed_ratio: min_changed_ratio.unwrap_or(0.01),
                poll_interval_ms: 50,
                timeout_ms: *timeout_ms,
            },
            _ => SemanticVerificationCriteria::default(),
        }
    }

    /// Returns true if an error is deterministic and should fail-closed without retries.
    fn is_unrecoverable(&self, err: &CuaError) -> bool {
        matches!(
            err,
            CuaError::WindowNotFound(_)
                | CuaError::WindowMinimized(_)
                | CuaError::WindowHidden(_)
                | CuaError::UipiBlocked(_)
                | CuaError::PermissionDenied(_)
                | CuaError::DangerousHotkeyRejected(_)
                | CuaError::CoordinateOutOfBounds { .. }
                | CuaError::EmergencyHalted
        )
    }

    /// Classify pre-check error into DefectCategory and RemedyHint.
    fn classify_precheck_error(&self, err: &PreCheckError) -> (DefectCategory, RemedyHint) {
        match err {
            PreCheckError::WindowNotFound { .. } => (
                DefectCategory::PreCheckFailed,
                RemedyHint::WindowDied("Target application closed or invalid handle".into()),
            ),
            PreCheckError::WindowMinimized { .. } => (
                DefectCategory::PreCheckFailed,
                RemedyHint::RestoreMinimizedWindow("Target window is minimized".into()),
            ),
            PreCheckError::WindowHidden { .. } | PreCheckError::WindowCloaked { .. } => (
                DefectCategory::PreCheckFailed,
                RemedyHint::BringWindowToForeground("Target window is hidden/cloaked".into()),
            ),
            PreCheckError::WindowEmptyBounds { .. } => (
                DefectCategory::PreCheckFailed,
                RemedyHint::ReacquireTargetCoordinates(
                    "Window bounds are empty or degenerate".into(),
                ),
            ),
            PreCheckError::TargetOccluded { .. } => (
                DefectCategory::TargetOccluded,
                RemedyHint::BringWindowToForeground(
                    "Target coordinates occluded by another window".into(),
                ),
            ),
            PreCheckError::OutOfBounds { .. } => (
                DefectCategory::OutOfBounds,
                RemedyHint::ReacquireTargetCoordinates(
                    "Coordinates lie outside client rect".into(),
                ),
            ),
            PreCheckError::SecurityRefused {
                reason,
                policy_rule,
            } => match policy_rule {
                SecurityPolicyRule::UipiIntegrityElevation => (
                    DefectCategory::SecurityRefused,
                    RemedyHint::ElevationRequired(
                        "Target window requires higher privilege token".into(),
                    ),
                ),
                _ => (
                    DefectCategory::SecurityRefused,
                    RemedyHint::SecurityPolicyViolation(reason.clone()),
                ),
            },
            PreCheckError::EmergencyHalted => (
                DefectCategory::KillSwitchAborted,
                RemedyHint::UserInterventionNeeded("Kill-switch active".into()),
            ),
            PreCheckError::Win32Error(msg) => (
                DefectCategory::DriverError,
                RemedyHint::ReacquireTargetCoordinates(msg.clone()),
            ),
        }
    }

    /// Classify runtime CuaError into DefectCategory and RemedyHint.
    fn classify_cua_error(&self, err: &CuaError) -> (DefectCategory, RemedyHint) {
        match err {
            CuaError::WindowNotFound(_) => (
                DefectCategory::PreCheckFailed,
                RemedyHint::WindowDied("Target application closed or invalid handle".into()),
            ),
            CuaError::WindowMinimized(_) => (
                DefectCategory::PreCheckFailed,
                RemedyHint::RestoreMinimizedWindow("Target window is minimized".into()),
            ),
            CuaError::WindowHidden(_) => (
                DefectCategory::PreCheckFailed,
                RemedyHint::BringWindowToForeground("Target window is hidden/invisible".into()),
            ),
            CuaError::CoordinateOutOfBounds { .. } => (
                DefectCategory::OutOfBounds,
                RemedyHint::ReacquireTargetCoordinates(
                    "Coordinates lie outside client rect".into(),
                ),
            ),
            CuaError::UipiBlocked(_) => (
                DefectCategory::PreCheckFailed,
                RemedyHint::ElevationRequired(
                    "Target window requires higher privilege token".into(),
                ),
            ),
            CuaError::PermissionDenied(_) | CuaError::DangerousHotkeyRejected(_) => (
                DefectCategory::SecurityRefused,
                RemedyHint::SecurityPolicyViolation("Action rejected by Security Governor".into()),
            ),
            CuaError::EmergencyHalted => (
                DefectCategory::KillSwitchAborted,
                RemedyHint::UserInterventionNeeded("Kill-switch active".into()),
            ),
            CuaError::InvalidParameter(msg) => (
                DefectCategory::DriverError,
                RemedyHint::ReacquireTargetCoordinates(format!("Invalid parameter: {msg}")),
            ),
            _ => (
                DefectCategory::DriverError,
                RemedyHint::ReacquireTargetCoordinates(err.to_string()),
            ),
        }
    }

    /// Helper to assemble a complete System1DefectReport.
    #[allow(clippy::too_many_arguments)]
    fn build_defect_report(
        &self,
        session_id: &str,
        plan_id: Option<&str>,
        failed_step_index: usize,
        total_steps: usize,
        completed_steps: usize,
        failed_action: System1Action,
        failure_category: DefectCategory,
        pre_check_error: Option<String>,
        post_diff_result: Option<VisualDiffResult>,
        retry_count: u32,
        delivery_modes_attempted: Vec<CuaDeliveryMode>,
        error_code: &str,
        error_message: &str,
        remedy_hint: RemedyHint,
        last_known_window: Option<CuaWindowInfo>,
        screenshot_recommended: bool,
    ) -> System1DefectReport {
        System1DefectReport {
            defect_id: uuid::Uuid::new_v4().to_string(),
            session_id: session_id.to_string(),
            plan_id: plan_id.map(|s| s.to_string()),
            failed_step_index,
            total_steps,
            completed_steps,
            failed_action,
            failure_category,
            pre_check_error,
            post_diff_result,
            retry_count,
            delivery_modes_attempted,
            error_code: error_code.to_string(),
            error_message: error_message.to_string(),
            remedy_hint,
            last_known_window,
            screenshot_recommended,
            timestamp_utc: current_iso8601_utc(),
        }
    }
}
