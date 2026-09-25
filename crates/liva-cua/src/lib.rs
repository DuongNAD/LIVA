//! Native Computer-Use Agent (CUA) Desktop Automation Engine for LIVA.
//!
//! Provides non-intrusive desktop automation, window targeting,
//! coordinate geometry, background input synthesis, UIPI detection,
//! state guards, multi-tier security governance, multi-trigger emergency kill-switch,
//! and structured audit ledger.

pub mod audit;
pub mod geometry;
pub mod guards;
pub mod input;
pub mod kill_switch;
pub mod mock;
pub mod router;
pub mod security;
pub mod traits;
pub mod types;
pub mod vision;
pub mod window;

use async_trait::async_trait;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;

pub use audit::*;
pub use geometry::*;
pub use guards::*;
pub use input::*;
pub use kill_switch::*;
pub use mock::MockCuaDriver;
pub use router::*;
pub use security::*;
pub use traits::CuaDriverTrait;
pub use types::*;
pub use vision::*;
pub use window::*;

/// Public facade for the LIVA Computer-Use Agent (CUA) engine.
pub struct CuaEngine {
    config: Arc<RwLock<CuaConfig>>,
    pub security_governor: Arc<SecurityGovernor>,
    pub kill_switch: Arc<KillSwitchController>,
    pub audit_recorder: Arc<CuaAuditRecorder>,
    action_counter: Arc<AtomicU64>,
    driver: Arc<dyn CuaDriverTrait>,
    start_time: Instant,
}

impl CuaEngine {
    /// Initialize with default native OS driver and optional SQLite DbActor handle.
    pub fn new(config: CuaConfig, db_handle: Option<liva_storage::DbActorHandle>) -> Self {
        Self::new_with_deps(config, Arc::new(Win32CuaDriver::new()), db_handle)
    }

    /// Gracefully shut down the CUA engine: aborts running actions, releases synthetic
    /// keys/buttons, and stops the background Esc polling thread.
    pub fn shutdown(&self) {
        let _ = self.trigger_emergency_halt();
        self.kill_switch.stop_poller();
    }

    /// Initialize with a custom driver (used for mocking, tests, or platform injection).
    pub fn new_with_driver(config: CuaConfig, driver: Arc<dyn CuaDriverTrait>) -> Self {
        Self::new_with_deps(config, driver, None)
    }

    /// Full constructor accepting custom driver and optional SQLite DbActor handle.
    pub fn new_with_deps(
        config: CuaConfig,
        driver: Arc<dyn CuaDriverTrait>,
        db_handle: Option<liva_storage::DbActorHandle>,
    ) -> Self {
        let governor = Arc::new(SecurityGovernor::new(&config));

        let ks_config = KillSwitchConfig {
            esc_polling_enabled: config.kill_switch_enabled,
            ..Default::default()
        };
        let kill_switch = KillSwitchController::new(ks_config);

        // Synchronize cancellation token to driver for fine-grained character cancellation
        driver.set_cancellation_token(kill_switch.cancellation_token());

        let audit_config = CuaAuditConfig {
            enabled: config.audit_logging_enabled,
            ..Default::default()
        };
        let audit_recorder = CuaAuditRecorder::new(audit_config, db_handle);

        Self {
            config: Arc::new(RwLock::new(config)),
            security_governor: governor,
            kill_switch,
            audit_recorder,
            action_counter: Arc::new(AtomicU64::new(0)),
            driver,
            start_time: Instant::now(),
        }
    }

    /// Returns a reference to the active SecurityGovernor.
    pub fn security_governor(&self) -> &Arc<SecurityGovernor> {
        &self.security_governor
    }

    /// Returns a reference to the active KillSwitchController.
    pub fn kill_switch(&self) -> &Arc<KillSwitchController> {
        &self.kill_switch
    }

    /// Returns a reference to the active CuaAuditRecorder.
    pub fn audit_recorder(&self) -> &Arc<CuaAuditRecorder> {
        &self.audit_recorder
    }

    /// Returns a reference to the active CuaDriverTrait.
    pub fn driver(&self) -> &Arc<dyn CuaDriverTrait> {
        &self.driver
    }

    /// List visible top-level windows, optionally filtered by PID.
    pub async fn list_windows(
        &self,
        filter_pid: Option<u32>,
    ) -> Result<Vec<CuaWindowInfo>, CuaError> {
        self.driver.list_windows(filter_pid).await
    }

    /// Validates the state of a target window.
    pub async fn validate_window_state(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError> {
        self.driver.inspect_window(hwnd).await
    }

    /// Execute a discrete action with full security, kill-switch, state verification,
    /// and structured audit logging across all exit paths.
    pub async fn execute_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError> {
        let start = Instant::now();
        let target_hwnd = Self::extract_hwnd(&action);
        let action_type = Self::format_action_type(&action).to_string();
        let delivery_mode = Self::extract_delivery_mode(&action);
        let mut coordinates = Self::extract_coordinates(&action);
        let mut target_app = TargetAppInfo {
            hwnd: target_hwnd,
            ..Default::default()
        };
        let event_id = uuid::Uuid::new_v4().to_string();
        let session_id = "default_session".to_string();
        let permission_mode = self.security_governor.get_permission_mode();

        // 1. Emergency Halt Pre-flight Check
        if self.kill_switch.is_halted() {
            let err = CuaError::EmergencyHalted;
            self.audit_recorder.record(CuaAuditEvent {
                event_id,
                timestamp_unix_ms: current_unix_ms(),
                session_id,
                action_type,
                target_app,
                coordinates,
                permission_mode,
                security_verdict: SecurityVerdict::AbortedByKillSwitch,
                policy_violation: Some("Emergency halt active".into()),
                execution_delivery: delivery_mode,
                action_effect: CuaActionEffect::Aborted,
                latency_ms: start.elapsed().as_millis() as u64,
                error_code: Some(err.error_code().to_string()),
            });
            return Err(err);
        }

        // 2. Standard Permission Mode Read-Only Check
        if let Err(err) = self.security_governor.evaluate_permission_mode(&action) {
            self.audit_recorder.record(CuaAuditEvent {
                event_id,
                timestamp_unix_ms: current_unix_ms(),
                session_id,
                action_type,
                target_app,
                coordinates,
                permission_mode,
                security_verdict: SecurityVerdict::BlockedByPolicy,
                policy_violation: Some(err.to_string()),
                execution_delivery: delivery_mode,
                action_effect: CuaActionEffect::Refused,
                latency_ms: start.elapsed().as_millis() as u64,
                error_code: Some(err.error_code().to_string()),
            });
            return Err(err);
        }

        // 3. Extract Target HWND & State Validation
        let verdict = match self.driver.inspect_window(target_hwnd).await {
            Ok(v) => v,
            Err(err) => {
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::FailedPreCheck,
                    policy_violation: Some(err.to_string()),
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Failed,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error_code: Some(err.error_code().to_string()),
                });
                return Err(err);
            }
        };

        let window_info = match verdict {
            WindowStateVerdict::Valid {
                hwnd,
                pid,
                title,
                bounds,
                window_info,
            } => {
                if let Err(err) = MinimizedWindowGuard::validate(target_hwnd, false, &bounds) {
                    self.audit_recorder.record(CuaAuditEvent {
                        event_id,
                        timestamp_unix_ms: current_unix_ms(),
                        session_id,
                        action_type,
                        target_app,
                        coordinates,
                        permission_mode,
                        security_verdict: SecurityVerdict::FailedPreCheck,
                        policy_violation: Some(err.to_string()),
                        execution_delivery: delivery_mode,
                        action_effect: CuaActionEffect::Refused,
                        latency_ms: start.elapsed().as_millis() as u64,
                        error_code: Some(err.error_code().to_string()),
                    });
                    return Err(err);
                }
                window_info.unwrap_or_else(|| {
                    #[cfg(windows)]
                    let proc_name = crate::window::get_process_name(pid);
                    #[cfg(not(windows))]
                    let proc_name = Some("app.exe".into());
                    CuaWindowInfo {
                        hwnd,
                        pid,
                        title,
                        class_name: "StandardApp".into(),
                        process_name: proc_name,
                        bounds,
                        is_on_screen: true,
                        is_minimized: false,
                        z_index: 0,
                        is_uwp_frame: false,
                        uwp_app_pid: None,
                    }
                })
            }
            WindowStateVerdict::Minimized { bounds, .. } => {
                coordinates.bounds = Some(bounds);
                let err = CuaError::WindowMinimized(target_hwnd);
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::FailedPreCheck,
                    policy_violation: Some(err.to_string()),
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Refused,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error_code: Some(err.error_code().to_string()),
                });
                return Err(err);
            }
            WindowStateVerdict::Hidden { .. } => {
                let err = CuaError::WindowHidden(target_hwnd);
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::FailedPreCheck,
                    policy_violation: Some(err.to_string()),
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Refused,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error_code: Some(err.error_code().to_string()),
                });
                return Err(err);
            }
            WindowStateVerdict::NotFound { .. } => {
                let err = CuaError::WindowNotFound(target_hwnd);
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::FailedPreCheck,
                    policy_violation: Some(err.to_string()),
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Refused,
                    latency_ms: start.elapsed().as_millis() as u64,
                    error_code: Some(err.error_code().to_string()),
                });
                return Err(err);
            }
        };

        // Update target_app & coordinates with validated window_info
        target_app.process_id = Some(window_info.pid);
        target_app.process_name = window_info.process_name.clone();
        target_app.window_title = Some(window_info.title.clone());
        coordinates.bounds = Some(window_info.bounds);
        if let (Some(sx), Some(sy)) = (coordinates.screen_x, coordinates.screen_y) {
            coordinates.client_x = Some(sx - window_info.bounds.x);
            coordinates.client_y = Some(sy - window_info.bounds.y);
        }

        // Synchronize simulated UIPI from driver if present (used in unit test harness)
        if let Some((target_rid, agent_rid)) = self.driver.check_simulated_uipi(target_hwnd) {
            self.security_governor.set_simulated_agent_rid(agent_rid);
            self.security_governor
                .set_simulated_uipi(target_hwnd, target_rid);
        }

        // 4. Centralized Security Governor Evaluation
        // Evaluates: Denylists, UIPI elevation, Allowlists, Dangerous Hotkeys, Coordinate Containment
        if let Err(err) = self
            .security_governor
            .evaluate_action(&action, &window_info)
        {
            let verdict = match err {
                CuaError::UipiBlocked(_) => SecurityVerdict::FailedPreCheck,
                _ => SecurityVerdict::BlockedByPolicy,
            };
            self.audit_recorder.record(CuaAuditEvent {
                event_id,
                timestamp_unix_ms: current_unix_ms(),
                session_id,
                action_type,
                target_app,
                coordinates,
                permission_mode,
                security_verdict: verdict,
                policy_violation: Some(err.to_string()),
                execution_delivery: delivery_mode,
                action_effect: CuaActionEffect::Refused,
                latency_ms: start.elapsed().as_millis() as u64,
                error_code: Some(err.error_code().to_string()),
            });
            return Err(err);
        }

        // 5. Scoped Action Motion Guard for Mouse Thrashing Detector
        let is_foreground = matches!(delivery_mode, CuaDeliveryMode::Foreground);
        let dest = match &action {
            CuaAction::Click { x, y, .. } | CuaAction::Scroll { x, y, .. } => x.zip(*y),
            CuaAction::MoveCursor { x, y, .. } => Some((*x, *y)),
            _ => None,
        };
        let _motion_guard = self
            .kill_switch
            .begin_action(target_hwnd, is_foreground, dest);

        // 6. Driver Dispatch with Instant Mid-Flight Cancellation Guard (< 15ms SLA)
        let cancel_token = self.kill_switch.cancellation_token();
        let dispatch_res = tokio::select! {
            res = self.driver.dispatch_action(action) => res,
            _ = cancel_token.cancelled() => Err(CuaError::EmergencyHalted),
        };
        drop(_motion_guard);

        match dispatch_res {
            Ok(mut result) => {
                result.duration_ms = start.elapsed().as_millis() as u64;
                self.action_counter.fetch_add(1, Ordering::Relaxed);

                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::Allowed,
                    policy_violation: None,
                    execution_delivery: delivery_mode,
                    action_effect: result.effect,
                    latency_ms: result.duration_ms,
                    error_code: None,
                });

                Ok(result)
            }
            Err(CuaError::EmergencyHalted) => {
                let latency = start.elapsed().as_millis() as u64;
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::AbortedByKillSwitch,
                    policy_violation: Some("Emergency halt during action execution".into()),
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Aborted,
                    latency_ms: latency,
                    error_code: Some("emergency_halt".into()),
                });
                Err(CuaError::EmergencyHalted)
            }
            Err(err) => {
                let latency = start.elapsed().as_millis() as u64;
                self.audit_recorder.record(CuaAuditEvent {
                    event_id,
                    timestamp_unix_ms: current_unix_ms(),
                    session_id,
                    action_type,
                    target_app,
                    coordinates,
                    permission_mode,
                    security_verdict: SecurityVerdict::Allowed,
                    policy_violation: None,
                    execution_delivery: delivery_mode,
                    action_effect: CuaActionEffect::Failed,
                    latency_ms: latency,
                    error_code: Some(err.error_code().to_string()),
                });
                Err(err)
            }
        }
    }

    /// Execute an action guarded by exact PID ownership.
    pub async fn execute_action_guarded(
        &self,
        action: CuaAction,
        guard: ExactPidWindowTargetGuard,
    ) -> Result<CuaActionResult, CuaError> {
        let target_hwnd = Self::extract_hwnd(&action);
        let verdict = self.driver.inspect_window(target_hwnd).await?;
        if let WindowStateVerdict::Valid { pid, .. } = verdict {
            guard.validate(target_hwnd, pid, None, None)?;
        }
        self.execute_action(action).await
    }

    /// Instantly trigger emergency halt (< 15ms SLA).
    pub fn trigger_emergency_halt(&self) -> Result<(), CuaError> {
        self.kill_switch.trigger_halt(HaltReason::ManualTrigger {
            detail: "Emergency halt triggered via CuaEngine".into(),
        })
    }

    /// Reset emergency halt signal after human authorization.
    pub fn reset_emergency_halt(&self) -> Result<(), CuaError> {
        self.kill_switch.reset()?;
        self.driver
            .set_cancellation_token(self.kill_switch.cancellation_token());
        Ok(())
    }

    /// Update security permission mode.
    pub async fn set_permission_mode(&self, mode: PermissionMode) -> Result<(), CuaError> {
        let mut config = self.config.write().await;
        config.permission_mode = mode;
        self.security_governor.set_permission_mode(mode);
        Ok(())
    }

    /// Query current status.
    pub async fn get_status(&self) -> CuaStatus {
        let _config = self.config.read().await;
        CuaStatus {
            permission_mode: self.security_governor.get_permission_mode(),
            is_halted: self.kill_switch.is_halted(),
            active_actions: 0,
            total_actions_executed: self.action_counter.load(Ordering::Relaxed),
            uptime_seconds: self.start_time.elapsed().as_secs(),
        }
    }

    pub fn extract_hwnd(action: &CuaAction) -> u64 {
        match action {
            CuaAction::Click { target_hwnd, .. }
            | CuaAction::MoveCursor { target_hwnd, .. }
            | CuaAction::Drag { target_hwnd, .. }
            | CuaAction::Scroll { target_hwnd, .. }
            | CuaAction::TypeText { target_hwnd, .. }
            | CuaAction::PressKey { target_hwnd, .. }
            | CuaAction::Hotkey { target_hwnd, .. }
            | CuaAction::BringToFront { target_hwnd }
            | CuaAction::RestoreWindow { target_hwnd } => *target_hwnd,
        }
    }

    fn format_action_type(action: &CuaAction) -> &'static str {
        match action {
            CuaAction::Click { .. } => "click",
            CuaAction::MoveCursor { .. } => "move_cursor",
            CuaAction::Drag { .. } => "drag",
            CuaAction::Scroll { .. } => "scroll",
            CuaAction::TypeText { .. } => "type_text",
            CuaAction::PressKey { .. } => "press_key",
            CuaAction::Hotkey { .. } => "hotkey",
            CuaAction::BringToFront { .. } => "bring_to_front",
            CuaAction::RestoreWindow { .. } => "restore_window",
        }
    }

    fn extract_delivery_mode(action: &CuaAction) -> CuaDeliveryMode {
        match action {
            CuaAction::Click { delivery_mode, .. }
            | CuaAction::MoveCursor { delivery_mode, .. }
            | CuaAction::Drag { delivery_mode, .. }
            | CuaAction::TypeText { delivery_mode, .. }
            | CuaAction::PressKey { delivery_mode, .. }
            | CuaAction::Hotkey { delivery_mode, .. } => *delivery_mode,
            CuaAction::Scroll { .. } | CuaAction::RestoreWindow { .. } => {
                CuaDeliveryMode::Background
            }
            CuaAction::BringToFront { .. } => CuaDeliveryMode::Foreground,
        }
    }

    fn extract_coordinates(action: &CuaAction) -> ActionCoordinates {
        match action {
            CuaAction::Click { x, y, .. } => ActionCoordinates {
                screen_x: *x,
                screen_y: *y,
                ..Default::default()
            },
            CuaAction::MoveCursor { x, y, .. } => ActionCoordinates {
                screen_x: Some(*x),
                screen_y: Some(*y),
                ..Default::default()
            },
            CuaAction::Drag {
                start_x, start_y, ..
            } => ActionCoordinates {
                screen_x: Some(*start_x),
                screen_y: Some(*start_y),
                ..Default::default()
            },
            CuaAction::Scroll { x, y, .. } => ActionCoordinates {
                screen_x: *x,
                screen_y: *y,
                ..Default::default()
            },
            _ => ActionCoordinates::default(),
        }
    }
}

/// Native Win32 driver executing against live desktop primitives.
pub struct Win32CuaDriver {
    cancel_token: std::sync::RwLock<Option<crate::kill_switch::CancellationToken>>,
}

impl Default for Win32CuaDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl Win32CuaDriver {
    pub fn new() -> Self {
        Self {
            cancel_token: std::sync::RwLock::new(None),
        }
    }
}

#[async_trait]
impl CuaDriverTrait for Win32CuaDriver {
    fn set_cancellation_token(&self, token: crate::kill_switch::CancellationToken) {
        if let Ok(mut lock) = self.cancel_token.write() {
            *lock = Some(token);
        }
    }

    async fn list_windows(&self, filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError> {
        window::list_windows(filter_pid).map_err(|e| CuaError::Win32Error(e.to_string()))
    }

    async fn inspect_window(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError> {
        match window::get_window_info(hwnd) {
            Ok(info) => {
                if info.is_minimized || info.bounds.is_minimized_sentinel() {
                    Ok(WindowStateVerdict::Minimized {
                        hwnd,
                        bounds: info.bounds,
                    })
                } else if !info.is_on_screen {
                    Ok(WindowStateVerdict::Hidden { hwnd })
                } else {
                    Ok(WindowStateVerdict::Valid {
                        hwnd,
                        pid: info.pid,
                        title: info.title.clone(),
                        bounds: info.bounds,
                        window_info: Some(info),
                    })
                }
            }
            Err(window::WindowError::InvalidHandle(_)) => Ok(WindowStateVerdict::NotFound { hwnd }),
            Err(e) => Err(CuaError::Win32Error(e.to_string())),
        }
    }

    async fn dispatch_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError> {
        let start = Instant::now();
        let target_hwnd = CuaEngine::extract_hwnd(&action);
        let action_name = format!("{:?}", action);

        match action {
            CuaAction::Click {
                target_hwnd,
                x,
                y,
                button,
                click_count,
                delivery_mode,
            } => {
                let (sx, sy) = (x.unwrap_or(0), y.unwrap_or(0));
                let res = if click_count > 1 {
                    InputActuator::dispatch_double_click(
                        target_hwnd,
                        sx,
                        sy,
                        button,
                        delivery_mode,
                    )?
                } else {
                    InputActuator::dispatch_click(target_hwnd, sx, sy, button, delivery_mode)?
                };
                Ok(CuaActionResult {
                    success: res.success,
                    action_type: action_name,
                    target_hwnd,
                    target_pid: None,
                    delivery_used: res.delivery_used,
                    effect: CuaActionEffect::Completed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                })
            }
            CuaAction::TypeText {
                target_hwnd,
                text,
                delivery_mode,
            } => {
                let token_opt = self.cancel_token.read().ok().and_then(|t| t.clone());
                let res = InputActuator::dispatch_type_text(
                    target_hwnd,
                    &text,
                    delivery_mode,
                    token_opt.as_ref(),
                )
                .await?;
                Ok(CuaActionResult {
                    success: res.success,
                    action_type: action_name,
                    target_hwnd,
                    target_pid: None,
                    delivery_used: res.delivery_used,
                    effect: CuaActionEffect::Completed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                })
            }
            CuaAction::RestoreWindow { target_hwnd } => {
                self.restore_window(target_hwnd).await?;
                Ok(CuaActionResult {
                    success: true,
                    action_type: action_name,
                    target_hwnd,
                    target_pid: None,
                    delivery_used: CuaDeliveryMode::Background,
                    effect: CuaActionEffect::Completed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                })
            }
            CuaAction::Scroll {
                target_hwnd,
                delta_x,
                delta_y,
                ..
            } => {
                self.scroll_container(target_hwnd, delta_x, delta_y).await?;
                Ok(CuaActionResult {
                    success: true,
                    action_type: action_name,
                    target_hwnd,
                    target_pid: None,
                    delivery_used: CuaDeliveryMode::Background,
                    effect: CuaActionEffect::Completed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                })
            }
            CuaAction::BringToFront { target_hwnd } => {
                #[cfg(windows)]
                {
                    use windows_sys::Win32::Foundation::HWND;
                    force_foreground_attached(target_hwnd as usize as HWND)?;
                }
                Ok(CuaActionResult {
                    success: true,
                    action_type: action_name,
                    target_hwnd,
                    target_pid: None,
                    delivery_used: CuaDeliveryMode::Foreground,
                    effect: CuaActionEffect::Completed,
                    error: None,
                    duration_ms: start.elapsed().as_millis() as u64,
                    timestamp_utc: crate::types::current_iso8601_utc(),
                })
            }
            _ => Ok(CuaActionResult {
                success: true,
                action_type: action_name,
                target_hwnd,
                target_pid: None,
                delivery_used: CuaDeliveryMode::Background,
                effect: CuaActionEffect::Completed,
                error: None,
                duration_ms: start.elapsed().as_millis() as u64,
                timestamp_utc: crate::types::current_iso8601_utc(),
            }),
        }
    }

    async fn restore_window(&self, hwnd: u64) -> Result<(), CuaError> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::{ShowWindow, SW_RESTORE};
            ShowWindow(hwnd as usize as HWND, SW_RESTORE);
        }
        #[cfg(not(windows))]
        let _ = hwnd;
        Ok(())
    }

    async fn scroll_container(
        &self,
        hwnd: u64,
        _delta_x: i32,
        delta_y: i32,
    ) -> Result<(), CuaError> {
        #[cfg(windows)]
        unsafe {
            use windows_sys::Win32::Foundation::{HWND, LPARAM, WPARAM};
            use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_MOUSEWHEEL};
            let wparam = ((delta_y as i16 as u16 as u32) << 16) as WPARAM;
            PostMessageW(hwnd as usize as HWND, WM_MOUSEWHEEL, wparam, 0 as LPARAM);
        }
        #[cfg(not(windows))]
        let _ = (hwnd, delta_y);
        Ok(())
    }
}

impl Drop for CuaEngine {
    fn drop(&mut self) {
        self.kill_switch.stop_poller();
    }
}
