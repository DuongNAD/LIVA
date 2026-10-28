//! Anomaly detection models and structured defect reporting for Pro execution profile.
//!
//! Detects unexpected modal dialogs, security elevation barriers, no-op input actions
//! (zero semantic and visual diff), window state degradations, and target disappearances.

use crate::execution::profile::ProfileAction;
use crate::perception::UiTree;
use crate::router::RemedyHint;
use crate::security::{SecurityGovernor, PROTECTED_SHELL_CLASSES};
use crate::types::{CuaDeliveryMode, CuaRect, CuaWindowInfo, WindowStateVerdict};
use crate::vision::types::VisualDiffResult;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Specific anomaly category detected during pre- or post-action verification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "anomaly_type", rename_all = "snake_case")]
pub enum ExecutionAnomaly {
    /// An unexpected modal dialog or popup window appeared after the action.
    UnexpectedModal {
        modal_hwnd: u64,
        modal_title: String,
        is_security_barrier: bool,
        is_dismissible: bool,
    },
    /// The action produced zero semantic UI change AND zero visual pixel difference.
    NoOpAction {
        target_som_id: Option<usize>,
        target_bounds: CuaRect,
        visual_diff_ratio: f32,
        is_identical_tree: bool,
    },
    /// Target window became minimized, hidden, destroyed, or cloaked.
    WindowDegraded { target_hwnd: u64, reason: String },
    /// Target element vanished from semantic tree during verification.
    TargetDisappeared { target_som_id: usize },
}

impl ExecutionAnomaly {
    /// Machine-readable error code corresponding to this anomaly.
    pub fn error_code(&self) -> &'static str {
        match self {
            Self::UnexpectedModal {
                is_security_barrier: true,
                ..
            } => "SECURITY_BARRIER_MODAL",
            Self::UnexpectedModal { .. } => "UNEXPECTED_MODAL",
            Self::NoOpAction { .. } => "NO_OP_ACTION",
            Self::WindowDegraded { .. } => "WINDOW_DEGRADED",
            Self::TargetDisappeared { .. } => "TARGET_DISAPPEARED",
        }
    }
}

/// Comprehensive, structured defect report returned when an action exhausts self-healing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProDefectReport {
    /// Unique identifier of the executed plan.
    pub plan_id: String,
    /// Calling agent session identifier.
    pub session_id: String,
    /// 0-indexed position of the failed step.
    pub failed_step_index: usize,
    /// The action that triggered or encountered the failure.
    pub failed_action: ProfileAction,
    /// Detailed anomaly classification.
    pub anomaly: ExecutionAnomaly,
    /// Number of recovery attempts executed before halting.
    pub recovery_attempts: u32,
    /// Input delivery modes attempted during execution and recovery.
    pub delivery_modes_attempted: Vec<CuaDeliveryMode>,
    /// Machine-readable error code.
    pub error_code: String,
    /// Detailed diagnostic message.
    pub error_message: String,
    /// Actionable remedy hint guiding agent or user resolution.
    pub remedy_hint: RemedyHint,
    /// Whether capturing a full desktop screenshot is recommended.
    pub screenshot_recommended: bool,
    /// UTC timestamp formatted in ISO-8601.
    pub timestamp_utc: String,
}

/// Evaluator detecting runtime execution anomalies.
pub struct AnomalyDetector;

impl AnomalyDetector {
    /// Inspect whether target window is degraded (minimized, hidden, or missing).
    pub fn detect_window_degraded(verdict: &WindowStateVerdict) -> Option<ExecutionAnomaly> {
        match verdict {
            WindowStateVerdict::Minimized { hwnd, .. } => Some(ExecutionAnomaly::WindowDegraded {
                target_hwnd: *hwnd,
                reason: "Window is minimized".to_string(),
            }),
            WindowStateVerdict::Hidden { hwnd } => Some(ExecutionAnomaly::WindowDegraded {
                target_hwnd: *hwnd,
                reason: "Window is hidden or off-screen".to_string(),
            }),
            WindowStateVerdict::NotFound { hwnd } => Some(ExecutionAnomaly::WindowDegraded {
                target_hwnd: *hwnd,
                reason: "Window handle not found or destroyed".to_string(),
            }),
            WindowStateVerdict::Valid { .. } => None,
        }
    }

    /// Detect whether an unexpected modal window or security barrier appeared or is obstructing execution.
    pub fn detect_unexpected_modal(
        pre_windows: &[CuaWindowInfo],
        post_windows: &[CuaWindowInfo],
        target_hwnd: u64,
        security_governor: &SecurityGovernor,
    ) -> Option<ExecutionAnomaly> {
        let pre_hwnds: HashSet<u64> = pre_windows.iter().map(|w| w.hwnd).collect();

        // Pass 1: Prioritize critical security elevation barriers across all windows
        for w in post_windows {
            if w.hwnd != target_hwnd && Self::is_security_barrier_window(w, security_governor) {
                return Some(ExecutionAnomaly::UnexpectedModal {
                    modal_hwnd: w.hwnd,
                    modal_title: w.title.clone(),
                    is_security_barrier: true,
                    is_dismissible: false,
                });
            }
        }

        // Pass 2: Check for dismissible dialogs or unexpected modal popups
        for w in post_windows {
            if w.hwnd != target_hwnd {
                let title_lower = w.title.to_lowercase();
                let class_lower = w.class_name.to_lowercase();

                let is_dialog_class =
                    w.class_name.contains("#32770") || class_lower.contains("dialog");
                let is_dialog_title = title_lower.contains("warning")
                    || title_lower.contains("error")
                    || title_lower.contains("confirm")
                    || title_lower.contains("alert")
                    || title_lower.contains("save as")
                    || title_lower.contains("open")
                    || title_lower.contains("prompt")
                    || title_lower.contains("message");

                let is_new = !pre_hwnds.contains(&w.hwnd);

                // Unexpected dialog or modal popup obstructing execution
                if is_dialog_class || is_dialog_title || is_new {
                    return Some(ExecutionAnomaly::UnexpectedModal {
                        modal_hwnd: w.hwnd,
                        modal_title: w.title.clone(),
                        is_security_barrier: false,
                        is_dismissible: true,
                    });
                }
            }
        }

        None
    }

    /// Evaluates whether a window represents a critical security elevation barrier
    /// (e.g. UAC consent.exe, credential UI broker, or protected security denylist).
    pub fn is_security_barrier_window(
        w: &CuaWindowInfo,
        security_governor: &SecurityGovernor,
    ) -> bool {
        let title_lower = w.title.to_lowercase();
        let proc_lower = w.process_name.as_deref().unwrap_or("").to_lowercase();

        proc_lower.contains("consent.exe")
            || proc_lower.contains("credentialuibroker.exe")
            || proc_lower.contains("credentialdialoghost.exe")
            || proc_lower.contains("sechealthui.exe")
            || title_lower.contains("user account control")
            || title_lower.contains("administrator")
            || title_lower.contains("windows security")
            || PROTECTED_SHELL_CLASSES.contains(&w.class_name.as_str())
            || security_governor.evaluate_protected_denylist(w).is_err()
    }

    /// Detect whether an action produced zero semantic UI change AND zero visual diff.
    pub fn detect_no_op(
        target_som_id: Option<usize>,
        target_bounds: CuaRect,
        pre_tree: Option<&UiTree>,
        post_tree: Option<&UiTree>,
        visual_diff: Option<&VisualDiffResult>,
    ) -> Option<ExecutionAnomaly> {
        // No-Op detection requires a visual verification diff indicating zero change
        let diff = visual_diff?;

        if diff.diff_detected && diff.changed_ratio >= 0.001 {
            return None;
        }

        let is_identical_tree = match (pre_tree, post_tree) {
            (Some(pre), Some(post)) => {
                let count_same = pre.total_nodes == post.total_nodes
                    && pre.interactive_count == post.interactive_count;

                if !count_same {
                    false
                } else if let Some(som) = target_som_id {
                    let pre_elem = pre.find_by_som(som);
                    let post_elem = post.find_by_som(som);
                    match (pre_elem, post_elem) {
                        (Some(pe), Some(poe)) => pe.value == poe.value && pe.state == poe.state,
                        _ => true,
                    }
                } else {
                    true
                }
            }
            _ => true,
        };

        if !diff.diff_detected && is_identical_tree {
            Some(ExecutionAnomaly::NoOpAction {
                target_som_id,
                target_bounds,
                visual_diff_ratio: diff.changed_ratio,
                is_identical_tree,
            })
        } else {
            None
        }
    }
}
