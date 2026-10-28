//! Self-healing recovery engine for Pro execution profile.
//!
//! Provides deterministic recovery actions:
//! 1. Foreground escalation when background input produces no-op zero diff.
//! 2. Automatic window restore and activation when window is minimized or occluded.
//! 3. Dismissal of unexpected popups via Escape key, strictly respecting security barriers.

use crate::audit::{
    ActionCoordinates, CuaAuditEvent, CuaAuditRecorder, SecurityVerdict, TargetAppInfo,
};
use crate::execution::anomaly::ExecutionAnomaly;
use crate::execution::profile::ProfileConfig;
use crate::router::RemedyHint;
use crate::security::SecurityGovernor;
use crate::traits::CuaDriverTrait;
use crate::types::{CuaAction, CuaActionEffect, CuaDeliveryMode, PermissionMode};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Classification of automated recovery action taken.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryActionTaken {
    /// Background input delivery was escalated to foreground delivery with window activation.
    EscalatedToForeground,
    /// Minimized or hidden target window was restored and brought to front.
    RestoredWindow,
    /// Unexpected modal dialog was dismissed via synthetic Escape key.
    DismissedModalPopup,
}

/// Engine executing deterministic self-healing recovery strategies.
pub struct SelfHealingEngine {
    driver: Arc<dyn CuaDriverTrait>,
    security_governor: Arc<SecurityGovernor>,
    audit_recorder: Arc<CuaAuditRecorder>,
}

impl SelfHealingEngine {
    /// Create a new `SelfHealingEngine` bound to the driver, security governor, and audit recorder.
    pub fn new(
        driver: Arc<dyn CuaDriverTrait>,
        security_governor: Arc<SecurityGovernor>,
        audit_recorder: Arc<CuaAuditRecorder>,
    ) -> Self {
        Self {
            driver,
            security_governor,
            audit_recorder,
        }
    }

    /// Access the underlying SecurityGovernor.
    pub fn security_governor(&self) -> &Arc<SecurityGovernor> {
        &self.security_governor
    }

    /// Attempt an automated self-healing recovery step based on detected anomaly.
    pub async fn attempt_recovery(
        &self,
        anomaly: &ExecutionAnomaly,
        current_delivery: CuaDeliveryMode,
        target_hwnd: u64,
        config: &ProfileConfig,
        session_id: &str,
    ) -> Result<RecoveryActionTaken, RemedyHint> {
        match anomaly {
            ExecutionAnomaly::NoOpAction { .. } => {
                if current_delivery == CuaDeliveryMode::Background
                    && config.allow_foreground_escalation
                {
                    let _ = self
                        .driver
                        .dispatch_action(CuaAction::BringToFront { target_hwnd })
                        .await;

                    self.record_healing_audit(
                        session_id,
                        target_hwnd,
                        "pro:self_healing:foreground_escalation",
                        CuaDeliveryMode::Foreground,
                        "Escalated background delivery to foreground after zero diff",
                    );

                    Ok(RecoveryActionTaken::EscalatedToForeground)
                } else {
                    Err(RemedyHint::AdjustVisualThreshold(
                        "Zero visual and semantic diff observed after retries; element may be disabled or static"
                            .to_string(),
                    ))
                }
            }
            ExecutionAnomaly::WindowDegraded {
                target_hwnd: hwnd,
                reason,
            } => {
                let _ = self.driver.restore_window(*hwnd).await;
                let _ = self
                    .driver
                    .dispatch_action(CuaAction::BringToFront { target_hwnd: *hwnd })
                    .await;

                self.record_healing_audit(
                    session_id,
                    *hwnd,
                    "pro:self_healing:restore_window",
                    CuaDeliveryMode::Foreground,
                    &format!("Restored and brought window to front: {}", reason),
                );

                Ok(RecoveryActionTaken::RestoredWindow)
            }
            ExecutionAnomaly::UnexpectedModal {
                modal_hwnd,
                modal_title,
                is_security_barrier,
                is_dismissible,
            } => {
                if *is_security_barrier {
                    self.record_security_barrier_audit(session_id, *modal_hwnd, modal_title);

                    Err(RemedyHint::SecurityPolicyViolation(format!(
                        "Security elevation barrier detected on modal [0x{:X}]: '{}'. Automated dismissal forbidden.",
                        modal_hwnd, modal_title
                    )))
                } else if *is_dismissible && config.auto_dismiss_popups {
                    let dismiss_down = CuaAction::PressKey {
                        target_hwnd: *modal_hwnd,
                        key: "Escape".to_string(),
                        down: true,
                        delivery_mode: CuaDeliveryMode::Foreground,
                    };
                    let _ = self.driver.dispatch_action(dismiss_down).await;

                    let dismiss_up = CuaAction::PressKey {
                        target_hwnd: *modal_hwnd,
                        key: "Escape".to_string(),
                        down: false,
                        delivery_mode: CuaDeliveryMode::Foreground,
                    };
                    let _ = self.driver.dispatch_action(dismiss_up).await;

                    self.record_healing_audit(
                        session_id,
                        *modal_hwnd,
                        "pro:self_healing:dismiss_popup",
                        CuaDeliveryMode::Foreground,
                        &format!(
                            "Auto-dismissed popup [0x{:X}] '{}' via Escape key",
                            modal_hwnd, modal_title
                        ),
                    );

                    Ok(RecoveryActionTaken::DismissedModalPopup)
                } else {
                    Err(RemedyHint::UserInterventionNeeded(format!(
                        "Unexpected modal popup [0x{:X}] '{}' appeared; auto-dismissal disabled.",
                        modal_hwnd, modal_title
                    )))
                }
            }
            ExecutionAnomaly::TargetDisappeared { target_som_id } => {
                Err(RemedyHint::ReacquireTargetCoordinates(format!(
                    "Target Set-of-Mark element [{}] disappeared from UI tree.",
                    target_som_id
                )))
            }
        }
    }

    fn record_healing_audit(
        &self,
        session_id: &str,
        hwnd: u64,
        action_type: &str,
        delivery: CuaDeliveryMode,
        details: &str,
    ) {
        let now = crate::audit::current_unix_ms();
        self.audit_recorder.record(CuaAuditEvent {
            event_id: format!("heal-{}", now),
            timestamp_unix_ms: now,
            session_id: session_id.to_string(),
            action_type: action_type.to_string(),
            target_app: TargetAppInfo {
                hwnd,
                ..Default::default()
            },
            coordinates: ActionCoordinates::default(),
            permission_mode: PermissionMode::Bounded,
            security_verdict: SecurityVerdict::Allowed,
            policy_violation: Some(details.to_string()),
            execution_delivery: delivery,
            action_effect: CuaActionEffect::Completed,
            latency_ms: 5,
            error_code: None,
        });
    }

    fn record_security_barrier_audit(&self, session_id: &str, hwnd: u64, title: &str) {
        let now = crate::audit::current_unix_ms();
        self.audit_recorder.record(CuaAuditEvent {
            event_id: format!("sec-barrier-{}", now),
            timestamp_unix_ms: now,
            session_id: session_id.to_string(),
            action_type: "pro:security_barrier_blocked".to_string(),
            target_app: TargetAppInfo {
                hwnd,
                window_title: Some(title.to_string()),
                ..Default::default()
            },
            coordinates: ActionCoordinates::default(),
            permission_mode: PermissionMode::Bounded,
            security_verdict: SecurityVerdict::BlockedByPolicy,
            policy_violation: Some(format!(
                "Security barrier modal '{}' (hwnd 0x{:X}) detected; execution halted",
                title, hwnd
            )),
            execution_delivery: CuaDeliveryMode::Foreground,
            action_effect: CuaActionEffect::Refused,
            latency_ms: 1,
            error_code: Some("SECURITY_BARRIER_MODAL".to_string()),
        });
    }
}
