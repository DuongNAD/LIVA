//! In-memory mock driver for deterministic, headless testing.

use crate::kill_switch::CancellationToken;
use crate::traits::CuaDriverTrait;
use crate::types::*;
use async_trait::async_trait;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, RwLock};
use std::time::Duration;

pub struct MockCuaDriver {
    pub windows: Mutex<HashMap<u64, CuaWindowInfo>>,
    pub dispatched_actions: Mutex<Vec<CuaAction>>,
    pub simulated_uipi_rids: Mutex<HashMap<u64, u32>>,
    pub agent_rid: Mutex<u32>,
    pub cancel_token: RwLock<Option<CancellationToken>>,
    pub typing_delay_per_char: RwLock<Option<Duration>>,
    pub typed_characters: Mutex<Vec<char>>,
    pub action_delay_ms: AtomicU64,
}

impl Default for MockCuaDriver {
    fn default() -> Self {
        Self::new()
    }
}

impl MockCuaDriver {
    pub fn new() -> Self {
        Self {
            windows: Mutex::new(HashMap::new()),
            dispatched_actions: Mutex::new(Vec::new()),
            simulated_uipi_rids: Mutex::new(HashMap::new()),
            agent_rid: Mutex::new(0x2000), // Medium integrity (0x2000)
            cancel_token: RwLock::new(None),
            typing_delay_per_char: RwLock::new(None),
            typed_characters: Mutex::new(Vec::new()),
            action_delay_ms: AtomicU64::new(0),
        }
    }

    pub fn insert_window(&self, info: CuaWindowInfo) {
        self.windows.lock().unwrap().insert(info.hwnd, info);
    }

    pub fn get_dispatched_actions(&self) -> Vec<CuaAction> {
        self.dispatched_actions.lock().unwrap().clone()
    }

    pub fn set_typing_delay(&self, delay: Duration) {
        *self.typing_delay_per_char.write().unwrap() = Some(delay);
    }

    pub fn get_typed_characters(&self) -> Vec<char> {
        self.typed_characters.lock().unwrap().clone()
    }
}

#[async_trait]
impl CuaDriverTrait for MockCuaDriver {
    fn set_cancellation_token(&self, token: CancellationToken) {
        *self.cancel_token.write().unwrap() = Some(token);
    }

    async fn list_windows(&self, filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError> {
        let windows = self.windows.lock().unwrap();
        let mut list: Vec<CuaWindowInfo> = windows.values().cloned().collect();
        if let Some(pid) = filter_pid {
            list.retain(|w| w.pid == pid || w.uwp_app_pid == Some(pid));
        }
        list.sort_by_key(|w| w.z_index);
        Ok(list)
    }

    async fn inspect_window(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError> {
        let windows = self.windows.lock().unwrap();
        if let Some(w) = windows.get(&hwnd) {
            if w.is_minimized || w.bounds.is_minimized_sentinel() {
                Ok(WindowStateVerdict::Minimized {
                    hwnd,
                    bounds: w.bounds,
                })
            } else if !w.is_on_screen {
                Ok(WindowStateVerdict::Hidden { hwnd })
            } else {
                Ok(WindowStateVerdict::Valid {
                    hwnd,
                    pid: w.pid,
                    title: w.title.clone(),
                    bounds: w.bounds,
                    window_info: Some(w.clone()),
                })
            }
        } else {
            Ok(WindowStateVerdict::NotFound { hwnd })
        }
    }

    async fn dispatch_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError> {
        self.dispatched_actions.lock().unwrap().push(action.clone());

        let target_hwnd = match action {
            CuaAction::Click { target_hwnd, .. }
            | CuaAction::MoveCursor { target_hwnd, .. }
            | CuaAction::Drag { target_hwnd, .. }
            | CuaAction::Scroll { target_hwnd, .. }
            | CuaAction::TypeText { target_hwnd, .. }
            | CuaAction::PressKey { target_hwnd, .. }
            | CuaAction::Hotkey { target_hwnd, .. }
            | CuaAction::BringToFront { target_hwnd }
            | CuaAction::RestoreWindow { target_hwnd } => target_hwnd,
        };

        // Check simulated UIPI
        let agent_rid = *self.agent_rid.lock().unwrap();
        if let Some(&target_rid) = self.simulated_uipi_rids.lock().unwrap().get(&target_hwnd) {
            if target_rid > agent_rid {
                return Err(CuaError::uipi_blocked_details(
                    target_hwnd,
                    target_rid,
                    agent_rid,
                ));
            }
        }

        let delay_ms = self.action_delay_ms.load(Ordering::Relaxed);
        if delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(delay_ms)).await;
        }

        // Simulate fine-grained typing with cancellation
        if let CuaAction::TypeText { ref text, .. } = action {
            let delay_opt = *self.typing_delay_per_char.read().unwrap();
            if let Some(delay) = delay_opt {
                let token_opt = self.cancel_token.read().unwrap().clone();
                for ch in text.chars() {
                    if let Some(ref token) = token_opt {
                        if token.is_cancelled() {
                            return Err(CuaError::EmergencyHalted);
                        }
                    }
                    self.typed_characters.lock().unwrap().push(ch);
                    if let Some(ref token) = token_opt {
                        tokio::select! {
                            _ = tokio::time::sleep(delay) => {}
                            _ = token.cancelled() => {
                                return Err(CuaError::EmergencyHalted);
                            }
                        }
                    } else {
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }

        Ok(CuaActionResult {
            success: true,
            action_type: format!("{:?}", action),
            target_hwnd,
            target_pid: Some(1000),
            delivery_used: CuaDeliveryMode::Background,
            effect: CuaActionEffect::Completed,
            error: None,
            duration_ms: 5,
            timestamp_utc: crate::types::current_iso8601_utc(),
        })
    }

    async fn restore_window(&self, hwnd: u64) -> Result<(), CuaError> {
        let mut windows = self.windows.lock().unwrap();
        if let Some(w) = windows.get_mut(&hwnd) {
            w.is_minimized = false;
            w.bounds = CuaRect::new(100, 100, 800, 600);
            w.is_on_screen = true;
            Ok(())
        } else {
            Err(CuaError::WindowNotFound(hwnd))
        }
    }

    async fn scroll_container(
        &self,
        _hwnd: u64,
        _delta_x: i32,
        _delta_y: i32,
    ) -> Result<(), CuaError> {
        Ok(())
    }

    fn check_simulated_uipi(&self, hwnd: u64) -> Option<(u32, u32)> {
        let rids = self.simulated_uipi_rids.lock().unwrap();
        if let Some(&target_rid) = rids.get(&hwnd) {
            let agent_rid = *self.agent_rid.lock().unwrap();
            Some((target_rid, agent_rid))
        } else {
            None
        }
    }
}
