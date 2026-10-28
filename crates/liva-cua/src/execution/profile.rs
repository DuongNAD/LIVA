//! Execution Profile definitions, action grammar, and execution plan models.
//!
//! Provides `ExecutionProfile` (Flash vs. Pro), `ProfileConfig`, `ProfileAction`
//! supporting direct Set-of-Mark element targeting, `ProfilePlan`, and execution result models.

use crate::types::{CuaAction, CuaDeliveryMode, CuaError, CuaPoint, MouseButton};
use serde::{Deserialize, Serialize};

fn default_click_count() -> u32 {
    1
}

/// Execution profile mode governing speed vs. verification rigor.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionProfile {
    /// Flash: Reactive observe-and-act execution using Set-of-Mark indices.
    /// Fast (~10-25ms per step), compact history summary, minimal verification overhead.
    #[default]
    Flash,
    /// Pro: Pre- and post-action verification combining semantic UI tree diffing
    /// and visual ROI diffing, anomaly detection (unexpected modals, no-ops),
    /// and automated self-healing recovery paths.
    Pro,
}

/// Configuration governing execution profile behavior, retries, and timeouts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileConfig {
    /// Active execution profile (Flash or Pro).
    pub profile: ExecutionProfile,
    /// Maximum number of self-healing / retry attempts per step in Pro mode.
    pub max_step_retries: u32,
    /// Whether background actions that yield 0% diff may escalate to foreground delivery.
    pub allow_foreground_escalation: bool,
    /// Whether non-security unexpected modal popups may be auto-dismissed via Escape.
    pub auto_dismiss_popups: bool,
    /// DWM / desktop message loop redraw micro-tick delay in milliseconds (default 35ms).
    pub redraw_micro_tick_ms: u64,
    /// Maximum number of recent step summaries retained in bounded history (default 20).
    pub max_history_size: usize,
    /// Timeout per step in milliseconds (default 5000ms).
    pub step_timeout_ms: u64,
}

pub type ProfileExecutionConfig = ProfileConfig;

impl Default for ProfileConfig {
    fn default() -> Self {
        Self {
            profile: ExecutionProfile::Flash,
            max_step_retries: 2,
            allow_foreground_escalation: true,
            auto_dismiss_popups: true,
            redraw_micro_tick_ms: 35,
            max_history_size: 20,
            step_timeout_ms: 5000,
        }
    }
}

impl ProfileConfig {
    /// Create a configuration tailored for high-speed Flash profile execution.
    pub fn flash() -> Self {
        Self {
            profile: ExecutionProfile::Flash,
            max_step_retries: 0,
            allow_foreground_escalation: false,
            auto_dismiss_popups: false,
            redraw_micro_tick_ms: 0,
            max_history_size: 20,
            step_timeout_ms: 3000,
        }
    }

    /// Create a configuration tailored for high-reliability Pro profile execution.
    pub fn pro() -> Self {
        Self {
            profile: ExecutionProfile::Pro,
            max_step_retries: 2,
            allow_foreground_escalation: true,
            auto_dismiss_popups: true,
            redraw_micro_tick_ms: 35,
            max_history_size: 20,
            step_timeout_ms: 5000,
        }
    }
}

/// Action grammar supporting both direct Set-of-Mark element targeting and raw HWND/coordinates.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum ProfileAction {
    Click {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        #[serde(default)]
        target_selector: Option<String>,
        #[serde(default)]
        button: MouseButton,
        #[serde(default = "default_click_count")]
        click_count: u32,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
        #[serde(default)]
        x: Option<i32>,
        #[serde(default)]
        y: Option<i32>,
    },
    DoubleClick {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
        #[serde(default)]
        x: Option<i32>,
        #[serde(default)]
        y: Option<i32>,
    },
    Hover {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        #[serde(default)]
        duration_ms: Option<u32>,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
        #[serde(default)]
        x: Option<i32>,
        #[serde(default)]
        y: Option<i32>,
    },
    TypeText {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        text: String,
        #[serde(default)]
        clear_before: bool,
        #[serde(default)]
        press_enter: bool,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    Drag {
        #[serde(default)]
        start_som_id: Option<usize>,
        #[serde(default)]
        end_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        #[serde(default)]
        start_point: Option<CuaPoint>,
        #[serde(default)]
        end_point: Option<CuaPoint>,
        #[serde(default)]
        steps: u32,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    Scroll {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        delta_x: i32,
        delta_y: i32,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    PressKey {
        #[serde(default)]
        target_hwnd: Option<u64>,
        key: String,
        down: bool,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    Hotkey {
        #[serde(default)]
        target_hwnd: Option<u64>,
        keys: Vec<String>,
        #[serde(default)]
        delivery_mode: Option<CuaDeliveryMode>,
    },
    WaitVisualChange {
        #[serde(default)]
        target_som_id: Option<usize>,
        #[serde(default)]
        target_hwnd: Option<u64>,
        timeout_ms: u64,
        #[serde(default)]
        min_changed_ratio: Option<f32>,
    },
    BringToFront {
        target_hwnd: u64,
    },
    RestoreWindow {
        target_hwnd: u64,
    },
}

impl ProfileAction {
    /// Return the target Set-of-Mark index if this action targets an indexed UI element.
    pub fn target_som_id(&self) -> Option<usize> {
        match self {
            Self::Click { target_som_id, .. }
            | Self::DoubleClick { target_som_id, .. }
            | Self::Hover { target_som_id, .. }
            | Self::TypeText { target_som_id, .. }
            | Self::Scroll { target_som_id, .. }
            | Self::WaitVisualChange { target_som_id, .. } => *target_som_id,
            Self::Drag { start_som_id, .. } => *start_som_id,
            Self::PressKey { .. }
            | Self::Hotkey { .. }
            | Self::BringToFront { .. }
            | Self::RestoreWindow { .. } => None,
        }
    }

    /// Return the target HWND if explicitly specified on this action.
    pub fn target_hwnd(&self) -> Option<u64> {
        match self {
            Self::Click { target_hwnd, .. }
            | Self::DoubleClick { target_hwnd, .. }
            | Self::Hover { target_hwnd, .. }
            | Self::TypeText { target_hwnd, .. }
            | Self::Drag { target_hwnd, .. }
            | Self::Scroll { target_hwnd, .. }
            | Self::PressKey { target_hwnd, .. }
            | Self::Hotkey { target_hwnd, .. }
            | Self::WaitVisualChange { target_hwnd, .. } => *target_hwnd,
            Self::BringToFront { target_hwnd } | Self::RestoreWindow { target_hwnd } => {
                Some(*target_hwnd)
            }
        }
    }

    /// Return a human-readable action name identifier.
    pub fn action_name(&self) -> &'static str {
        match self {
            Self::Click { .. } => "click",
            Self::DoubleClick { .. } => "double_click",
            Self::Hover { .. } => "hover",
            Self::TypeText { .. } => "type_text",
            Self::Drag { .. } => "drag",
            Self::Scroll { .. } => "scroll",
            Self::PressKey { .. } => "press_key",
            Self::Hotkey { .. } => "hotkey",
            Self::WaitVisualChange { .. } => "wait_visual_change",
            Self::BringToFront { .. } => "bring_to_front",
            Self::RestoreWindow { .. } => "restore_window",
        }
    }

    /// Return the requested input delivery mode if specified.
    pub fn delivery_mode(&self) -> Option<CuaDeliveryMode> {
        match self {
            Self::Click { delivery_mode, .. }
            | Self::DoubleClick { delivery_mode, .. }
            | Self::Hover { delivery_mode, .. }
            | Self::TypeText { delivery_mode, .. }
            | Self::Drag { delivery_mode, .. }
            | Self::Scroll { delivery_mode, .. }
            | Self::PressKey { delivery_mode, .. }
            | Self::Hotkey { delivery_mode, .. } => *delivery_mode,
            _ => None,
        }
    }

    /// Return a new ProfileAction with overridden delivery mode.
    pub fn with_delivery_mode(&self, mode: CuaDeliveryMode) -> Self {
        let mut cloned = self.clone();
        match &mut cloned {
            Self::Click { delivery_mode, .. }
            | Self::DoubleClick { delivery_mode, .. }
            | Self::Hover { delivery_mode, .. }
            | Self::TypeText { delivery_mode, .. }
            | Self::Drag { delivery_mode, .. }
            | Self::Scroll { delivery_mode, .. }
            | Self::PressKey { delivery_mode, .. }
            | Self::Hotkey { delivery_mode, .. } => {
                *delivery_mode = Some(mode);
            }
            _ => {}
        }
        cloned
    }

    /// Convert into low-level `CuaAction` using optionally resolved target coordinates.
    pub fn to_cua_action(
        &self,
        resolved_pt: Option<CuaPoint>,
        default_hwnd: u64,
    ) -> Result<CuaAction, CuaError> {
        let target_hwnd = self.target_hwnd().unwrap_or(default_hwnd);
        let delivery = self.delivery_mode().unwrap_or(CuaDeliveryMode::Background);

        match self {
            Self::Click {
                button,
                click_count,
                x,
                y,
                ..
            } => {
                let final_x = resolved_pt.map(|p| p.x).or(*x);
                let final_y = resolved_pt.map(|p| p.y).or(*y);
                Ok(CuaAction::Click {
                    target_hwnd,
                    x: final_x,
                    y: final_y,
                    button: *button,
                    click_count: *click_count,
                    delivery_mode: delivery,
                })
            }
            Self::DoubleClick { x, y, .. } => {
                let final_x = resolved_pt.map(|p| p.x).or(*x);
                let final_y = resolved_pt.map(|p| p.y).or(*y);
                Ok(CuaAction::Click {
                    target_hwnd,
                    x: final_x,
                    y: final_y,
                    button: MouseButton::Left,
                    click_count: 2,
                    delivery_mode: delivery,
                })
            }
            Self::Hover { x, y, .. } => {
                let final_x = resolved_pt.map(|p| p.x).or(*x).unwrap_or(0);
                let final_y = resolved_pt.map(|p| p.y).or(*y).unwrap_or(0);
                Ok(CuaAction::MoveCursor {
                    target_hwnd,
                    x: final_x,
                    y: final_y,
                    delivery_mode: delivery,
                })
            }
            Self::TypeText { text, .. } => Ok(CuaAction::TypeText {
                target_hwnd,
                text: text.clone(),
                delivery_mode: delivery,
            }),
            Self::Drag {
                start_point,
                end_point,
                steps,
                ..
            } => {
                let sp = resolved_pt.or(*start_point).unwrap_or(CuaPoint::new(0, 0));
                let ep = end_point.unwrap_or(sp);
                Ok(CuaAction::Drag {
                    target_hwnd,
                    start_x: sp.x,
                    start_y: sp.y,
                    end_x: ep.x,
                    end_y: ep.y,
                    button: MouseButton::Left,
                    steps: *steps,
                    delivery_mode: delivery,
                })
            }
            Self::Scroll {
                delta_x, delta_y, ..
            } => {
                let (sx, sy) = match resolved_pt {
                    Some(pt) => (Some(pt.x), Some(pt.y)),
                    None => (None, None),
                };
                Ok(CuaAction::Scroll {
                    target_hwnd,
                    delta_x: *delta_x,
                    delta_y: *delta_y,
                    x: sx,
                    y: sy,
                })
            }
            Self::PressKey { key, down, .. } => Ok(CuaAction::PressKey {
                target_hwnd,
                key: key.clone(),
                down: *down,
                delivery_mode: delivery,
            }),
            Self::Hotkey { keys, .. } => Ok(CuaAction::Hotkey {
                target_hwnd,
                keys: keys.clone(),
                delivery_mode: delivery,
            }),
            Self::BringToFront { target_hwnd } => Ok(CuaAction::BringToFront {
                target_hwnd: *target_hwnd,
            }),
            Self::RestoreWindow { target_hwnd } => Ok(CuaAction::RestoreWindow {
                target_hwnd: *target_hwnd,
            }),
            Self::WaitVisualChange { .. } => {
                // WaitVisualChange is handled directly in orchestrator verification loop
                Err(CuaError::InvalidParameter(
                    "WaitVisualChange cannot be converted directly into discrete CuaAction"
                        .to_string(),
                ))
            }
        }
    }
}

/// A structured multi-step plan targeting a window under an execution profile.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfilePlan {
    pub plan_id: String,
    pub session_id: String,
    pub profile: ExecutionProfile,
    pub target_hwnd: Option<u64>,
    pub steps: Vec<ProfileAction>,
    pub config: ProfileConfig,
}

impl ProfilePlan {
    /// Create a new ProfilePlan with default configuration for the selected profile.
    pub fn new(
        plan_id: impl Into<String>,
        session_id: impl Into<String>,
        profile: ExecutionProfile,
    ) -> Self {
        let p = profile;
        Self {
            plan_id: plan_id.into(),
            session_id: session_id.into(),
            profile: p,
            target_hwnd: None,
            steps: Vec::new(),
            config: ProfileConfig {
                profile: p,
                ..Default::default()
            },
        }
    }

    /// Set the default target HWND for steps in this plan.
    pub fn with_target_hwnd(mut self, hwnd: u64) -> Self {
        self.target_hwnd = Some(hwnd);
        self
    }

    /// Set the plan steps.
    pub fn with_steps(mut self, steps: Vec<ProfileAction>) -> Self {
        self.steps = steps;
        self
    }

    /// Append a single step to the plan.
    pub fn with_step(mut self, step: ProfileAction) -> Self {
        self.steps.push(step);
        self
    }

    /// Set the profile configuration.
    pub fn with_config(mut self, config: ProfileConfig) -> Self {
        self.config = config;
        self
    }
}

/// Execution outcome for a single step in a plan.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileStepResult {
    pub step_index: usize,
    pub action_name: String,
    pub target_som_id: Option<usize>,
    pub target_hwnd: u64,
    pub success: bool,
    pub delivery_used: CuaDeliveryMode,
    pub duration_ms: u64,
    pub error: Option<String>,
    pub healing_attempted: bool,
    pub visual_diff_ratio: Option<f32>,
}

/// Aggregated outcome of a multi-step plan execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileExecutionResult {
    pub plan_id: String,
    pub session_id: String,
    pub profile: ExecutionProfile,
    pub success: bool,
    pub total_steps: usize,
    pub completed_steps: usize,
    pub step_results: Vec<ProfileStepResult>,
    pub history_summary: String,
    pub defect_report: Option<crate::execution::anomaly::ProDefectReport>,
    pub duration_ms: u64,
}
