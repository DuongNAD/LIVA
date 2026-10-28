//! Central multi-step execution orchestrator for Flash and Pro profiles.
//!
//! Implements:
//! - Flash Profile: High-speed reactive loop using Set-of-Mark indices and bounded history.
//! - Pro Profile: Pre/post state verification, anomaly detection, self-healing, and audit logging.

use crate::audit::{
    ActionCoordinates, CuaAuditEvent, CuaAuditRecorder, SecurityVerdict, TargetAppInfo,
};
use crate::execution::anomaly::{AnomalyDetector, ExecutionAnomaly, ProDefectReport};
use crate::execution::healing::{RecoveryActionTaken, SelfHealingEngine};
use crate::execution::history::{ActionHistoryTracker, CompactActionSummary};
use crate::execution::profile::{
    ExecutionProfile, ProfileAction, ProfileConfig, ProfileExecutionResult, ProfilePlan,
    ProfileStepResult,
};
use crate::perception::PerceptionResolver;
use crate::router::RemedyHint;
use crate::security::SecurityGovernor;
use crate::traits::CuaDriverTrait;
use crate::types::{
    CuaAction, CuaActionEffect, CuaDeliveryMode, CuaError, CuaRect, PermissionMode,
    WindowStateVerdict,
};
use crate::vision::traits::VisualVerifierTrait;
use crate::vision::types::SemanticVerificationCriteria;
use crate::CuaEngine;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

/// Central orchestrator executing multi-step action plans under Flash and Pro profiles.
pub struct CuaExecutionOrchestrator {
    engine: Arc<CuaEngine>,
    driver: Arc<dyn CuaDriverTrait>,
    security_governor: Arc<SecurityGovernor>,
    kill_switch: Arc<crate::kill_switch::KillSwitchController>,
    audit_recorder: Arc<CuaAuditRecorder>,
    visual_verifier: Option<Arc<dyn VisualVerifierTrait>>,
    healing_engine: SelfHealingEngine,
    config: ProfileConfig,
    history_tracker: Arc<Mutex<ActionHistoryTracker>>,
}

impl CuaExecutionOrchestrator {
    /// Create a new orchestrator instance.
    pub fn new(
        engine: Arc<CuaEngine>,
        visual_verifier: Option<Arc<dyn VisualVerifierTrait>>,
        config: ProfileConfig,
    ) -> Self {
        let driver = engine.driver().clone();
        let security_governor = engine.security_governor().clone();
        let kill_switch = engine.kill_switch().clone();
        let audit_recorder = engine.audit_recorder().clone();
        let history_capacity = config.max_history_size;

        let healing_engine = SelfHealingEngine::new(
            driver.clone(),
            security_governor.clone(),
            audit_recorder.clone(),
        );

        Self {
            engine,
            driver,
            security_governor,
            kill_switch,
            audit_recorder,
            visual_verifier,
            healing_engine,
            config,
            history_tracker: Arc::new(Mutex::new(ActionHistoryTracker::new(history_capacity))),
        }
    }

    /// Builder: update the configuration.
    pub fn with_config(mut self, config: ProfileConfig) -> Self {
        self.config = config;
        self
    }

    /// Access the underlying action history tracker.
    pub fn history_tracker(&self) -> &Arc<Mutex<ActionHistoryTracker>> {
        &self.history_tracker
    }

    /// Format current bounded action history into a prompt summary string.
    pub async fn get_history_summary(&self) -> String {
        self.history_tracker.lock().await.format_prompt_summary()
    }

    /// Execute a multi-step plan under the plan's configured profile (`Flash` or `Pro`).
    pub async fn execute_plan(
        &self,
        plan: ProfilePlan,
    ) -> Result<ProfileExecutionResult, CuaError> {
        match plan.profile {
            ExecutionProfile::Flash => self.execute_flash_loop(plan).await,
            ExecutionProfile::Pro => self.execute_pro_loop(plan).await,
        }
    }

    // ========================================================================
    // FLASH PROFILE EXECUTION LOOP
    // ========================================================================

    async fn execute_flash_loop(
        &self,
        plan: ProfilePlan,
    ) -> Result<ProfileExecutionResult, CuaError> {
        let start_time = Instant::now();
        let default_hwnd = plan.target_hwnd.unwrap_or(0);
        let mut step_results = Vec::new();
        let total_steps = plan.steps.len();

        // Check emergency halt before starting (< 15ms SLA)
        if self.kill_switch.is_halted() {
            self.record_kill_switch_audit(&plan.session_id, default_hwnd, "flash:plan");
            return Err(CuaError::EmergencyHalted);
        }

        for (step_idx, action) in plan.steps.iter().enumerate() {
            if self.kill_switch.is_halted() {
                let step_hwnd = action.target_hwnd().unwrap_or(default_hwnd);
                self.record_kill_switch_audit(&plan.session_id, step_hwnd, action.action_name());
                return Err(CuaError::EmergencyHalted);
            }

            let step_hwnd = action.target_hwnd().unwrap_or(default_hwnd);
            let res = self
                .execute_flash_step(step_idx, action, step_hwnd, &plan.session_id)
                .await;

            match res {
                Ok(step_res) => {
                    let success = step_res.success;
                    step_results.push(step_res);
                    if !success {
                        let history = self.get_history_summary().await;
                        return Ok(ProfileExecutionResult {
                            plan_id: plan.plan_id,
                            session_id: plan.session_id,
                            profile: ExecutionProfile::Flash,
                            success: false,
                            total_steps,
                            completed_steps: step_results.len() - 1,
                            step_results,
                            history_summary: history,
                            defect_report: None,
                            duration_ms: start_time.elapsed().as_millis() as u64,
                        });
                    }
                }
                Err(CuaError::EmergencyHalted) => {
                    self.record_kill_switch_audit(
                        &plan.session_id,
                        step_hwnd,
                        action.action_name(),
                    );
                    return Err(CuaError::EmergencyHalted);
                }
                Err(e) => {
                    let history = self.get_history_summary().await;
                    step_results.push(ProfileStepResult {
                        step_index: step_idx,
                        action_name: action.action_name().to_string(),
                        target_som_id: action.target_som_id(),
                        target_hwnd: step_hwnd,
                        success: false,
                        delivery_used: action
                            .delivery_mode()
                            .unwrap_or(CuaDeliveryMode::Background),
                        duration_ms: 0,
                        error: Some(e.to_string()),
                        healing_attempted: false,
                        visual_diff_ratio: None,
                    });
                    return Ok(ProfileExecutionResult {
                        plan_id: plan.plan_id,
                        session_id: plan.session_id,
                        profile: ExecutionProfile::Flash,
                        success: false,
                        total_steps,
                        completed_steps: step_results.len() - 1,
                        step_results,
                        history_summary: history,
                        defect_report: None,
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        let history = self.get_history_summary().await;
        Ok(ProfileExecutionResult {
            plan_id: plan.plan_id,
            session_id: plan.session_id,
            profile: ExecutionProfile::Flash,
            success: true,
            total_steps,
            completed_steps: step_results.len(),
            step_results,
            history_summary: history,
            defect_report: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Execute a single step under Flash profile.
    pub async fn execute_flash_step(
        &self,
        step_index: usize,
        action: &ProfileAction,
        target_hwnd: u64,
        session_id: &str,
    ) -> Result<ProfileStepResult, CuaError> {
        let step_start = Instant::now();

        // 1. Check emergency halt (< 15ms SLA)
        if self.kill_switch.is_halted() {
            self.record_kill_switch_audit(session_id, target_hwnd, action.action_name());
            return Err(CuaError::EmergencyHalted);
        }

        // 2. Resolve Set-of-Mark target coordinates if indexed
        let (resolved_pt, target_label) = if let Some(som_id) = action.target_som_id() {
            let tree = self.engine.inspect_ui_tree(target_hwnd).await?;
            let (pt, _) = PerceptionResolver::resolve_coordinate(&tree, som_id)?;
            let label = tree.find_by_som(som_id).map(|e| e.name.clone());
            (Some(pt), label)
        } else {
            (None, None)
        };

        // 3. Convert to discrete CuaAction
        let cua_action = action.to_cua_action(resolved_pt, target_hwnd)?;
        let delivery_mode = action
            .delivery_mode()
            .unwrap_or(CuaDeliveryMode::Background);

        // 4. Dispatch via CuaEngine (with security governor & kill-switch guards)
        let dispatch_result = self.engine.execute_action(cua_action).await;

        let duration_ms = step_start.elapsed().as_millis() as u64;

        match dispatch_result {
            Ok(res) => {
                // Record into bounded action history
                self.history_tracker
                    .lock()
                    .await
                    .append(CompactActionSummary {
                        step_index,
                        action_type: action.action_name().to_string(),
                        target_som_id: action.target_som_id(),
                        target_hwnd,
                        target_label,
                        delivery_mode: res.delivery_used,
                        effect: res.effect,
                        success: res.success,
                        duration_ms,
                        timestamp_ms: crate::audit::current_unix_ms(),
                    });

                Ok(ProfileStepResult {
                    step_index,
                    action_name: action.action_name().to_string(),
                    target_som_id: action.target_som_id(),
                    target_hwnd,
                    success: res.success,
                    delivery_used: res.delivery_used,
                    duration_ms,
                    error: res.error.map(|e| format!("{}: {}", e.code, e.message)),
                    healing_attempted: false,
                    visual_diff_ratio: None,
                })
            }
            Err(CuaError::EmergencyHalted) => Err(CuaError::EmergencyHalted),
            Err(e) => {
                self.history_tracker
                    .lock()
                    .await
                    .append(CompactActionSummary {
                        step_index,
                        action_type: action.action_name().to_string(),
                        target_som_id: action.target_som_id(),
                        target_hwnd,
                        target_label,
                        delivery_mode,
                        effect: CuaActionEffect::Failed,
                        success: false,
                        duration_ms,
                        timestamp_ms: crate::audit::current_unix_ms(),
                    });

                Ok(ProfileStepResult {
                    step_index,
                    action_name: action.action_name().to_string(),
                    target_som_id: action.target_som_id(),
                    target_hwnd,
                    success: false,
                    delivery_used: delivery_mode,
                    duration_ms,
                    error: Some(e.to_string()),
                    healing_attempted: false,
                    visual_diff_ratio: None,
                })
            }
        }
    }

    // ========================================================================
    // PRO PROFILE EXECUTION LOOP
    // ========================================================================

    async fn execute_pro_loop(
        &self,
        plan: ProfilePlan,
    ) -> Result<ProfileExecutionResult, CuaError> {
        let start_time = Instant::now();
        let default_hwnd = plan.target_hwnd.unwrap_or(0);
        let mut step_results = Vec::new();
        let total_steps = plan.steps.len();

        // Check emergency halt upfront (< 15ms SLA)
        if self.kill_switch.is_halted() {
            self.record_kill_switch_audit(&plan.session_id, default_hwnd, "pro:plan");
            return Err(CuaError::EmergencyHalted);
        }

        for (step_idx, action) in plan.steps.iter().enumerate() {
            if self.kill_switch.is_halted() {
                let step_hwnd = action.target_hwnd().unwrap_or(default_hwnd);
                self.record_kill_switch_audit(&plan.session_id, step_hwnd, action.action_name());
                return Err(CuaError::EmergencyHalted);
            }

            let step_hwnd = action.target_hwnd().unwrap_or(default_hwnd);
            let retries = plan.config.max_step_retries;

            match self
                .execute_pro_step(step_idx, action, step_hwnd, &plan.session_id, retries)
                .await
            {
                Ok(step_res) => {
                    step_results.push(step_res);
                }
                Err(defect) => {
                    if defect.error_code == "EMERGENCY_HALTED" {
                        return Err(CuaError::EmergencyHalted);
                    }

                    step_results.push(ProfileStepResult {
                        step_index: step_idx,
                        action_name: action.action_name().to_string(),
                        target_som_id: action.target_som_id(),
                        target_hwnd: step_hwnd,
                        success: false,
                        delivery_used: defect
                            .delivery_modes_attempted
                            .last()
                            .copied()
                            .unwrap_or(CuaDeliveryMode::Background),
                        duration_ms: 0,
                        error: Some(defect.error_message.clone()),
                        healing_attempted: defect.recovery_attempts > 0,
                        visual_diff_ratio: match &defect.anomaly {
                            ExecutionAnomaly::NoOpAction {
                                visual_diff_ratio, ..
                            } => Some(*visual_diff_ratio),
                            _ => None,
                        },
                    });

                    let history = self.get_history_summary().await;
                    return Ok(ProfileExecutionResult {
                        plan_id: plan.plan_id,
                        session_id: plan.session_id,
                        profile: ExecutionProfile::Pro,
                        success: false,
                        total_steps,
                        completed_steps: step_results.len() - 1,
                        step_results,
                        history_summary: history,
                        defect_report: Some(defect),
                        duration_ms: start_time.elapsed().as_millis() as u64,
                    });
                }
            }
        }

        let history = self.get_history_summary().await;
        Ok(ProfileExecutionResult {
            plan_id: plan.plan_id,
            session_id: plan.session_id,
            profile: ExecutionProfile::Pro,
            success: true,
            total_steps,
            completed_steps: step_results.len(),
            step_results,
            history_summary: history,
            defect_report: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
        })
    }

    /// Execute a single step under Pro profile with pre/post verification,
    /// anomaly detection, and automated self-healing.
    pub async fn execute_pro_step(
        &self,
        step_index: usize,
        action: &ProfileAction,
        target_hwnd: u64,
        session_id: &str,
        retries_left: u32,
    ) -> Result<ProfileStepResult, ProDefectReport> {
        let step_start = Instant::now();

        // 1. Check emergency halt
        if self.kill_switch.is_halted() {
            self.record_kill_switch_audit(session_id, target_hwnd, action.action_name());
            return Err(ProDefectReport {
                plan_id: "pro_step".to_string(),
                session_id: session_id.to_string(),
                failed_step_index: step_index,
                failed_action: action.clone(),
                anomaly: ExecutionAnomaly::WindowDegraded {
                    target_hwnd,
                    reason: "Emergency kill-switch is active".to_string(),
                },
                recovery_attempts: 0,
                delivery_modes_attempted: vec![],
                error_code: "EMERGENCY_HALTED".to_string(),
                error_message: "Emergency kill-switch halted execution".to_string(),
                remedy_hint: RemedyHint::UserInterventionNeeded(
                    "Emergency kill-switch halted execution".to_string(),
                ),
                screenshot_recommended: false,
                timestamp_utc: crate::types::current_iso8601_utc(),
            });
        }

        // 2. Pre-Action Window State Verification
        let window_verdict = self
            .driver
            .inspect_window(target_hwnd)
            .await
            .unwrap_or(WindowStateVerdict::NotFound { hwnd: target_hwnd });

        if let Some(anomaly) = AnomalyDetector::detect_window_degraded(&window_verdict) {
            match self
                .healing_engine
                .attempt_recovery(
                    &anomaly,
                    CuaDeliveryMode::Foreground,
                    target_hwnd,
                    &self.config,
                    session_id,
                )
                .await
            {
                Ok(RecoveryActionTaken::RestoredWindow) => {
                    // Window restored, proceed
                }
                _ => {
                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: anomaly.clone(),
                        recovery_attempts: 1,
                        delivery_modes_attempted: vec![],
                        error_code: anomaly.error_code().to_string(),
                        error_message: format!(
                            "Target window state degraded: {:?}",
                            window_verdict
                        ),
                        remedy_hint: RemedyHint::RestoreMinimizedWindow(
                            "Target window is minimized or closed".to_string(),
                        ),
                        screenshot_recommended: false,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }
            }
        }

        // 3. Pre-Action State Snapshots (Windows list, UI Tree, Baseline Visual Frame)
        let pre_windows = self.driver.list_windows(None).await.unwrap_or_default();
        let pre_tree = self.engine.inspect_ui_tree(target_hwnd).await.ok();
        let baseline_frame = match &self.visual_verifier {
            Some(v) => v.capture_baseline().await.ok(),
            None => None,
        };

        // 4. Resolve Target Coordinates and Bounds
        let (resolved_pt, target_bounds, target_label) =
            if let Some(som_id) = action.target_som_id() {
                if let Some(ref tree) = pre_tree {
                    match PerceptionResolver::resolve_coordinate(tree, som_id) {
                        Ok((pt, bounds)) => {
                            let label = tree.find_by_som(som_id).map(|e| e.name.clone());
                            (Some(pt), bounds, label)
                        }
                        Err(_) => {
                            return Err(ProDefectReport {
                                plan_id: "pro_step".to_string(),
                                session_id: session_id.to_string(),
                                failed_step_index: step_index,
                                failed_action: action.clone(),
                                anomaly: ExecutionAnomaly::TargetDisappeared {
                                    target_som_id: som_id,
                                },
                                recovery_attempts: 0,
                                delivery_modes_attempted: vec![],
                                error_code: "TARGET_DISAPPEARED".to_string(),
                                error_message: format!(
                                    "Set-of-Mark index [{som_id}] not found in target window tree"
                                ),
                                remedy_hint: RemedyHint::ReacquireTargetCoordinates(
                                    "Target element vanished from tree".to_string(),
                                ),
                                screenshot_recommended: true,
                                timestamp_utc: crate::types::current_iso8601_utc(),
                            });
                        }
                    }
                } else {
                    (None, CuaRect::default(), None)
                }
            } else {
                (None, CuaRect::default(), None)
            };

        // 5. Execution & Self-Healing Retry Loop
        let mut current_delivery = action
            .delivery_mode()
            .unwrap_or(CuaDeliveryMode::Background);
        let mut delivery_modes_attempted = Vec::new();
        let mut healing_attempted = false;
        let mut attempt = 0;

        loop {
            delivery_modes_attempted.push(current_delivery);

            // Handle WaitVisualChange action specifically
            if let ProfileAction::WaitVisualChange {
                timeout_ms,
                min_changed_ratio,
                ..
            } = action
            {
                if let Some(ref verifier) = self.visual_verifier {
                    let base = match baseline_frame {
                        Some(ref f) => f.clone(),
                        None => verifier
                            .capture_baseline()
                            .await
                            .map_err(|e| ProDefectReport {
                                plan_id: "pro_step".to_string(),
                                session_id: session_id.to_string(),
                                failed_step_index: step_index,
                                failed_action: action.clone(),
                                anomaly: ExecutionAnomaly::WindowDegraded {
                                    target_hwnd,
                                    reason: e.to_string(),
                                },
                                recovery_attempts: attempt,
                                delivery_modes_attempted: delivery_modes_attempted.clone(),
                                error_code: "BASELINE_CAPTURE_FAILED".to_string(),
                                error_message: e.to_string(),
                                remedy_hint: RemedyHint::AdjustVisualThreshold(
                                    "Baseline capture failed".to_string(),
                                ),
                                screenshot_recommended: false,
                                timestamp_utc: crate::types::current_iso8601_utc(),
                            })?,
                    };

                    let criteria = SemanticVerificationCriteria::WaitVisualChange {
                        min_changed_ratio: min_changed_ratio.unwrap_or(0.01),
                        poll_interval_ms: 50,
                        timeout_ms: *timeout_ms,
                    };

                    let diff_res = verifier
                        .wait_visual_change(&target_bounds, &criteria, &base)
                        .await
                        .map_err(|e| ProDefectReport {
                            plan_id: "pro_step".to_string(),
                            session_id: session_id.to_string(),
                            failed_step_index: step_index,
                            failed_action: action.clone(),
                            anomaly: ExecutionAnomaly::NoOpAction {
                                target_som_id: action.target_som_id(),
                                target_bounds,
                                visual_diff_ratio: 0.0,
                                is_identical_tree: true,
                            },
                            recovery_attempts: attempt,
                            delivery_modes_attempted: delivery_modes_attempted.clone(),
                            error_code: "WAIT_VISUAL_CHANGE_TIMEOUT".to_string(),
                            error_message: e.to_string(),
                            remedy_hint: RemedyHint::AdjustVisualThreshold(
                                "Visual change wait timed out".to_string(),
                            ),
                            screenshot_recommended: true,
                            timestamp_utc: crate::types::current_iso8601_utc(),
                        })?;

                    let duration_ms = step_start.elapsed().as_millis() as u64;
                    self.history_tracker
                        .lock()
                        .await
                        .append(CompactActionSummary {
                            step_index,
                            action_type: action.action_name().to_string(),
                            target_som_id: action.target_som_id(),
                            target_hwnd,
                            target_label,
                            delivery_mode: current_delivery,
                            effect: CuaActionEffect::Completed,
                            success: diff_res.diff_detected,
                            duration_ms,
                            timestamp_ms: crate::audit::current_unix_ms(),
                        });

                    return Ok(ProfileStepResult {
                        step_index,
                        action_name: action.action_name().to_string(),
                        target_som_id: action.target_som_id(),
                        target_hwnd,
                        success: diff_res.diff_detected,
                        delivery_used: current_delivery,
                        duration_ms,
                        error: None,
                        healing_attempted,
                        visual_diff_ratio: Some(diff_res.changed_ratio),
                    });
                }
            }

            // Convert ProfileAction to CuaAction with current delivery mode
            let mut concrete_action = match action.to_cua_action(resolved_pt, target_hwnd) {
                Ok(a) => a,
                Err(e) => {
                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: ExecutionAnomaly::WindowDegraded {
                            target_hwnd,
                            reason: e.to_string(),
                        },
                        recovery_attempts: attempt,
                        delivery_modes_attempted,
                        error_code: "ACTION_CONVERSION_ERROR".to_string(),
                        error_message: e.to_string(),
                        remedy_hint: RemedyHint::AdjustVisualThreshold(
                            "Cannot convert action".to_string(),
                        ),
                        screenshot_recommended: false,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }
            };

            // Force current delivery mode
            match &mut concrete_action {
                CuaAction::Click { delivery_mode, .. }
                | CuaAction::MoveCursor { delivery_mode, .. }
                | CuaAction::Drag { delivery_mode, .. }
                | CuaAction::TypeText { delivery_mode, .. }
                | CuaAction::PressKey { delivery_mode, .. }
                | CuaAction::Hotkey { delivery_mode, .. } => {
                    *delivery_mode = current_delivery;
                }
                _ => {}
            }

            // Dispatch via CuaEngine
            let dispatch_res = self.engine.execute_action(concrete_action).await;

            match dispatch_res {
                Ok(action_res) if !action_res.success => {
                    let (err_code, err_msg) = if let Some(ref detail) = action_res.error {
                        (detail.code.to_uppercase(), detail.message.clone())
                    } else {
                        (
                            "ACTION_DISPATCH_FAILED".to_string(),
                            "Action dispatch failed".to_string(),
                        )
                    };

                    self.history_tracker
                        .lock()
                        .await
                        .append(CompactActionSummary {
                            step_index,
                            action_type: action.action_name().to_string(),
                            target_som_id: action.target_som_id(),
                            target_hwnd,
                            target_label: target_label.clone(),
                            delivery_mode: action_res.delivery_used,
                            effect: action_res.effect,
                            success: false,
                            duration_ms: step_start.elapsed().as_millis() as u64,
                            timestamp_ms: crate::audit::current_unix_ms(),
                        });

                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: ExecutionAnomaly::WindowDegraded {
                            target_hwnd,
                            reason: err_msg.clone(),
                        },
                        recovery_attempts: attempt,
                        delivery_modes_attempted,
                        error_code: err_code,
                        error_message: err_msg.clone(),
                        remedy_hint: RemedyHint::UserInterventionNeeded(err_msg),
                        screenshot_recommended: true,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }
                Ok(_) => {
                    // Action dispatch reported success; proceed with verification
                }
                Err(CuaError::EmergencyHalted) => {
                    self.record_kill_switch_audit(session_id, target_hwnd, action.action_name());
                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: ExecutionAnomaly::WindowDegraded {
                            target_hwnd,
                            reason: "Emergency kill-switch is active".to_string(),
                        },
                        recovery_attempts: attempt,
                        delivery_modes_attempted,
                        error_code: "EMERGENCY_HALTED".to_string(),
                        error_message: "Emergency kill-switch halted execution".to_string(),
                        remedy_hint: RemedyHint::UserInterventionNeeded(
                            "Emergency kill-switch halted execution".to_string(),
                        ),
                        screenshot_recommended: false,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }
                Err(e) => {
                    self.history_tracker
                        .lock()
                        .await
                        .append(CompactActionSummary {
                            step_index,
                            action_type: action.action_name().to_string(),
                            target_som_id: action.target_som_id(),
                            target_hwnd,
                            target_label: target_label.clone(),
                            delivery_mode: current_delivery,
                            effect: CuaActionEffect::Failed,
                            success: false,
                            duration_ms: step_start.elapsed().as_millis() as u64,
                            timestamp_ms: crate::audit::current_unix_ms(),
                        });

                    let (error_code, remedy_hint) = match &e {
                        CuaError::PermissionDenied(msg)
                        | CuaError::DangerousHotkeyRejected(msg) => (
                            "PERMISSION_DENIED".to_string(),
                            RemedyHint::SecurityPolicyViolation(msg.clone()),
                        ),
                        CuaError::UipiBlocked(msg) => (
                            "UIPI_BLOCKED".to_string(),
                            RemedyHint::ElevationRequired(msg.clone()),
                        ),
                        CuaError::WindowNotFound(hwnd) => (
                            "WINDOW_NOT_FOUND".to_string(),
                            RemedyHint::WindowDied(format!("Window 0x{:X} not found", hwnd)),
                        ),
                        CuaError::WindowMinimized(hwnd) => (
                            "WINDOW_MINIMIZED".to_string(),
                            RemedyHint::RestoreMinimizedWindow(format!(
                                "Window 0x{:X} is minimized",
                                hwnd
                            )),
                        ),
                        CuaError::WindowHidden(hwnd) => (
                            "WINDOW_HIDDEN".to_string(),
                            RemedyHint::WindowDied(format!("Window 0x{:X} is hidden", hwnd)),
                        ),
                        _ => (
                            e.error_code().to_uppercase(),
                            RemedyHint::UserInterventionNeeded(e.to_string()),
                        ),
                    };

                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: ExecutionAnomaly::WindowDegraded {
                            target_hwnd,
                            reason: e.to_string(),
                        },
                        recovery_attempts: attempt,
                        delivery_modes_attempted,
                        error_code,
                        error_message: e.to_string(),
                        remedy_hint,
                        screenshot_recommended: true,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }
            }

            // DWM redraw micro-tick delay
            if self.config.redraw_micro_tick_ms > 0 {
                tokio::time::sleep(Duration::from_millis(self.config.redraw_micro_tick_ms)).await;
            }

            // Post-Action State Snapshots
            let post_windows = self.driver.list_windows(None).await.unwrap_or_default();
            let post_tree = self.engine.inspect_ui_tree(target_hwnd).await.ok();

            // Post-Action Visual ROI Diffing
            let visual_diff = if let Some(ref verifier) = self.visual_verifier {
                let criteria = match action {
                    ProfileAction::TypeText { .. } => {
                        SemanticVerificationCriteria::type_text_default()
                    }
                    ProfileAction::Scroll { .. } => SemanticVerificationCriteria::scroll_default(),
                    _ => SemanticVerificationCriteria::click_default(),
                };
                verifier
                    .verify_action(&target_bounds, &criteria, baseline_frame.as_ref())
                    .await
                    .ok()
            } else {
                None
            };

            // Anomaly Check 1: Unexpected Modal Dialog
            if let Some(anomaly) = AnomalyDetector::detect_unexpected_modal(
                &pre_windows,
                &post_windows,
                target_hwnd,
                &self.security_governor,
            ) {
                if let ExecutionAnomaly::UnexpectedModal {
                    is_security_barrier: true,
                    ref modal_title,
                    modal_hwnd,
                    ..
                } = anomaly
                {
                    // Security barrier modals (e.g. UAC consent.exe) must NEVER be auto-dismissed!
                    self.healing_engine
                        .attempt_recovery(
                            &anomaly,
                            current_delivery,
                            target_hwnd,
                            &self.config,
                            session_id,
                        )
                        .await
                        .ok();

                    return Err(ProDefectReport {
                        plan_id: "pro_step".to_string(),
                        session_id: session_id.to_string(),
                        failed_step_index: step_index,
                        failed_action: action.clone(),
                        anomaly: anomaly.clone(),
                        recovery_attempts: attempt,
                        delivery_modes_attempted,
                        error_code: "SECURITY_BARRIER_MODAL".to_string(),
                        error_message: format!(
                            "Security elevation barrier modal [0x{:X}] '{}' appeared; automated dismissal prohibited",
                            modal_hwnd, modal_title
                        ),
                        remedy_hint: RemedyHint::SecurityPolicyViolation(format!(
                            "Security barrier modal '{}' requires user confirmation",
                            modal_title
                        )),
                        screenshot_recommended: true,
                        timestamp_utc: crate::types::current_iso8601_utc(),
                    });
                }

                // Dismissible modal popup recovery
                if attempt < retries_left {
                    attempt += 1;
                    if let Ok(RecoveryActionTaken::DismissedModalPopup) = self
                        .healing_engine
                        .attempt_recovery(
                            &anomaly,
                            current_delivery,
                            target_hwnd,
                            &self.config,
                            session_id,
                        )
                        .await
                    {
                        healing_attempted = true;
                        continue;
                    }
                }

                return Err(ProDefectReport {
                    plan_id: "pro_step".to_string(),
                    session_id: session_id.to_string(),
                    failed_step_index: step_index,
                    failed_action: action.clone(),
                    anomaly: anomaly.clone(),
                    recovery_attempts: attempt,
                    delivery_modes_attempted,
                    error_code: anomaly.error_code().to_string(),
                    error_message: "Unexpected modal dialog obstructed execution".to_string(),
                    remedy_hint: RemedyHint::UserInterventionNeeded(
                        "Unexpected modal dialog requires user intervention".to_string(),
                    ),
                    screenshot_recommended: true,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                });
            }

            // Anomaly Check 2: No-Op Action (Zero visual diff AND identical UI tree)
            if let Some(anomaly) = AnomalyDetector::detect_no_op(
                action.target_som_id(),
                target_bounds,
                pre_tree.as_ref(),
                post_tree.as_ref(),
                visual_diff.as_ref(),
            ) {
                // If background delivery yielded no-op, escalate to foreground!
                if current_delivery == CuaDeliveryMode::Background
                    && self.config.allow_foreground_escalation
                    && attempt < retries_left
                {
                    attempt += 1;
                    healing_attempted = true;
                    current_delivery = CuaDeliveryMode::Foreground;

                    let _ = self
                        .healing_engine
                        .attempt_recovery(
                            &anomaly,
                            CuaDeliveryMode::Background,
                            target_hwnd,
                            &self.config,
                            session_id,
                        )
                        .await;

                    continue;
                }

                if attempt < retries_left {
                    attempt += 1;
                    continue;
                }

                return Err(ProDefectReport {
                    plan_id: "pro_step".to_string(),
                    session_id: session_id.to_string(),
                    failed_step_index: step_index,
                    failed_action: action.clone(),
                    anomaly: anomaly.clone(),
                    recovery_attempts: attempt,
                    delivery_modes_attempted,
                    error_code: "NO_OP_ACTION".to_string(),
                    error_message: "Action produced zero semantic UI change and zero visual pixel diff".to_string(),
                    remedy_hint: RemedyHint::AdjustVisualThreshold(
                        "Action completed with zero visual change; element may be disabled or static".to_string(),
                    ),
                    screenshot_recommended: true,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                });
            }

            // Check if window became degraded during action
            let post_verdict = self
                .driver
                .inspect_window(target_hwnd)
                .await
                .unwrap_or(WindowStateVerdict::NotFound { hwnd: target_hwnd });

            if let Some(anomaly) = AnomalyDetector::detect_window_degraded(&post_verdict) {
                if attempt < retries_left {
                    attempt += 1;
                    healing_attempted = true;
                    let _ = self
                        .healing_engine
                        .attempt_recovery(
                            &anomaly,
                            current_delivery,
                            target_hwnd,
                            &self.config,
                            session_id,
                        )
                        .await;
                    continue;
                }

                return Err(ProDefectReport {
                    plan_id: "pro_step".to_string(),
                    session_id: session_id.to_string(),
                    failed_step_index: step_index,
                    failed_action: action.clone(),
                    anomaly: anomaly.clone(),
                    recovery_attempts: attempt,
                    delivery_modes_attempted,
                    error_code: anomaly.error_code().to_string(),
                    error_message: format!(
                        "Target window degraded post-action: {:?}",
                        post_verdict
                    ),
                    remedy_hint: RemedyHint::RestoreMinimizedWindow("Window degraded".to_string()),
                    screenshot_recommended: false,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                });
            }

            // If we reached here, action was verified successfully!
            let duration_ms = step_start.elapsed().as_millis() as u64;

            self.history_tracker
                .lock()
                .await
                .append(CompactActionSummary {
                    step_index,
                    action_type: action.action_name().to_string(),
                    target_som_id: action.target_som_id(),
                    target_hwnd,
                    target_label,
                    delivery_mode: current_delivery,
                    effect: CuaActionEffect::Completed,
                    success: true,
                    duration_ms,
                    timestamp_ms: crate::audit::current_unix_ms(),
                });

            return Ok(ProfileStepResult {
                step_index,
                action_name: action.action_name().to_string(),
                target_som_id: action.target_som_id(),
                target_hwnd,
                success: true,
                delivery_used: current_delivery,
                duration_ms,
                error: None,
                healing_attempted,
                visual_diff_ratio: visual_diff.map(|d| d.changed_ratio),
            });
        }
    }

    fn record_kill_switch_audit(&self, session_id: &str, hwnd: u64, action_type: &str) {
        let now = crate::audit::current_unix_ms();
        self.audit_recorder.record(CuaAuditEvent {
            event_id: format!("halt-{}", now),
            timestamp_unix_ms: now,
            session_id: session_id.to_string(),
            action_type: format!("profile:{}", action_type),
            target_app: TargetAppInfo {
                hwnd,
                ..Default::default()
            },
            coordinates: ActionCoordinates::default(),
            permission_mode: PermissionMode::Bounded,
            security_verdict: SecurityVerdict::AbortedByKillSwitch,
            policy_violation: Some("Emergency kill-switch is active (<15ms SLA)".to_string()),
            execution_delivery: CuaDeliveryMode::Background,
            action_effect: CuaActionEffect::Aborted,
            latency_ms: 1,
            error_code: Some("EMERGENCY_HALTED".to_string()),
        });
    }
}
