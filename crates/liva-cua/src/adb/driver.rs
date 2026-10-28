//! Native Android ADB Automation Driver (`AdbCuaDriver`).
//!
//! Implements `CuaDriverTrait` for Android devices and emulators over ADB,
//! translating high-level `CuaAction` desktop intents into low-level `adb shell input`
//! commands with Set-of-Mark semantic hierarchy extraction, coordinate bounds math,
//! security governance integration, and <15ms kill-switch emergency cancellation.

use super::parser::{parse_uiautomator_xml, AdbUiTree};
use super::transport::AdbTransport;
use crate::kill_switch::CancellationToken;
use crate::perception::{SoMIndexer, UiTree};
use crate::traits::CuaDriverTrait;
use crate::types::{
    CuaAction, CuaActionEffect, CuaActionResult, CuaDeliveryMode, CuaError, CuaRect, CuaWindowInfo,
    WindowStateVerdict,
};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Synthetic root window HWND assigned to Android target devices.
pub const ANDROID_SYNTHETIC_HWND_BASE: u64 = 0xADB0_0000;

/// Native Android Computer-Use Agent driver implementing `CuaDriverTrait`.
pub struct AdbCuaDriver {
    transport: Arc<dyn AdbTransport>,
    serial: RwLock<Option<String>>,
    cancel_token: RwLock<Option<CancellationToken>>,
    target_package: RwLock<Option<String>>,
    screen_bounds: RwLock<CuaRect>,
    synthetic_hwnd: u64,
    typing_delay: RwLock<Option<Duration>>,
    cached_ui_tree: RwLock<Option<AdbUiTree>>,
}

impl AdbCuaDriver {
    /// Creates a new `AdbCuaDriver` with the given transport and default screen dimensions (1080x2400).
    pub fn new(transport: Arc<dyn AdbTransport>) -> Self {
        Self::new_with_serial(transport, None)
    }

    /// Creates a new `AdbCuaDriver` targeting a specific device serial.
    pub fn new_with_serial(transport: Arc<dyn AdbTransport>, serial: Option<String>) -> Self {
        let synthetic_hwnd = if let Some(ref s) = serial {
            let hash = s
                .bytes()
                .fold(0u64, |acc, b| acc.wrapping_mul(31).wrapping_add(b as u64));
            ANDROID_SYNTHETIC_HWND_BASE | (hash & 0xFFFF)
        } else {
            ANDROID_SYNTHETIC_HWND_BASE | 0x0001
        };

        Self {
            transport,
            serial: RwLock::new(serial),
            cancel_token: RwLock::new(None),
            target_package: RwLock::new(Some("com.android.settings".to_string())),
            screen_bounds: RwLock::new(CuaRect::new(0, 0, 1080, 2400)),
            synthetic_hwnd,
            typing_delay: RwLock::new(None),
            cached_ui_tree: RwLock::new(None),
        }
    }

    /// Sets the target package name (used for allowlist security checks and activity launches).
    pub fn with_target_package(self, pkg: impl Into<String>) -> Self {
        *self.target_package.write() = Some(pkg.into());
        self
    }

    /// Sets explicit screen bounds (width & height).
    pub fn with_screen_bounds(self, bounds: CuaRect) -> Self {
        *self.screen_bounds.write() = bounds;
        self
    }

    /// Returns the target device serial if configured.
    pub fn serial(&self) -> Option<String> {
        self.serial.read().clone()
    }

    /// Updates the target device serial at runtime.
    pub fn set_serial(&self, serial: Option<String>) {
        *self.serial.write() = serial;
    }

    /// Returns the active Android target package name.
    pub fn target_package(&self) -> Option<String> {
        self.target_package.read().clone()
    }

    /// Sets the active Android target package name at runtime.
    pub fn set_target_package(&self, pkg: Option<String>) {
        *self.target_package.write() = pkg;
    }

    /// Returns the current screen bounds rectangle.
    pub fn screen_bounds(&self) -> CuaRect {
        *self.screen_bounds.read()
    }

    /// Updates the screen bounds rectangle.
    pub fn set_screen_bounds(&self, bounds: CuaRect) {
        *self.screen_bounds.write() = bounds;
    }

    /// Returns the synthetic HWND identifier for this Android device target.
    pub fn synthetic_hwnd(&self) -> u64 {
        self.synthetic_hwnd
    }

    /// Returns a reference to the underlying ADB transport.
    pub fn transport(&self) -> &Arc<dyn AdbTransport> {
        &self.transport
    }

    /// Configures synthetic per-character typing delay for fine-grained cancellation testing.
    pub fn set_typing_delay(&self, delay: Option<Duration>) {
        *self.typing_delay.write() = delay;
    }

    /// Returns the last cached `AdbUiTree` if available.
    pub fn cached_ui_tree(&self) -> Option<AdbUiTree> {
        self.cached_ui_tree.read().clone()
    }

    /// Executes raw ADB command using the active serial.
    pub async fn execute_adb(&self, args: &[&str]) -> Result<String, CuaError> {
        self.check_cancellation()?;
        let ser = self.serial.read().clone();
        self.transport.execute(ser.as_deref(), args).await
    }

    /// Checks whether the emergency kill switch has cancelled execution.
    pub fn check_cancellation(&self) -> Result<(), CuaError> {
        if let Some(ref token) = *self.cancel_token.read() {
            if token.is_cancelled() {
                return Err(CuaError::EmergencyHalted);
            }
        }
        Ok(())
    }
}

#[async_trait]
impl CuaDriverTrait for AdbCuaDriver {
    fn set_cancellation_token(&self, token: CancellationToken) {
        *self.cancel_token.write() = Some(token);
    }

    async fn list_windows(&self, _filter_pid: Option<u32>) -> Result<Vec<CuaWindowInfo>, CuaError> {
        self.check_cancellation()?;

        let bounds = *self.screen_bounds.read();
        let ser_label = self
            .serial
            .read()
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let pkg = self
            .target_package
            .read()
            .clone()
            .unwrap_or_else(|| "com.android.settings".to_string());

        let window_info = CuaWindowInfo {
            hwnd: self.synthetic_hwnd,
            pid: 1000,
            title: format!("Android Device ({})", ser_label),
            class_name: "AndroidWindow".to_string(),
            process_name: Some(pkg),
            bounds,
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        };

        Ok(vec![window_info])
    }

    async fn inspect_window(&self, hwnd: u64) -> Result<WindowStateVerdict, CuaError> {
        self.check_cancellation()?;

        let bounds = *self.screen_bounds.read();
        let ser_label = self
            .serial
            .read()
            .clone()
            .unwrap_or_else(|| "default".to_string());
        let pkg = self
            .target_package
            .read()
            .clone()
            .unwrap_or_else(|| "com.android.settings".to_string());

        let window_info = CuaWindowInfo {
            hwnd: if hwnd == 0 { self.synthetic_hwnd } else { hwnd },
            pid: 1000,
            title: format!("Android Device ({})", ser_label),
            class_name: "AndroidWindow".to_string(),
            process_name: Some(pkg),
            bounds,
            is_on_screen: true,
            is_minimized: false,
            z_index: 0,
            is_uwp_frame: false,
            uwp_app_pid: None,
        };

        Ok(WindowStateVerdict::Valid {
            hwnd: window_info.hwnd,
            pid: 1000,
            title: window_info.title.clone(),
            bounds,
            window_info: Some(window_info),
        })
    }

    async fn restore_window(&self, _hwnd: u64) -> Result<(), CuaError> {
        self.check_cancellation()?;
        // Send KEYCODE_WAKEUP (224) to wake up screen
        self.execute_adb(&["shell", "input", "keyevent", "224"])
            .await?;
        Ok(())
    }

    async fn scroll_container(
        &self,
        _hwnd: u64,
        delta_x: i32,
        delta_y: i32,
    ) -> Result<(), CuaError> {
        self.check_cancellation()?;
        let bounds = *self.screen_bounds.read();
        let (cx, cy) = bounds.center();

        let end_x = (cx - delta_x).clamp(bounds.x, bounds.x + bounds.width.max(1) - 1);
        let end_y = (cy - delta_y).clamp(bounds.y, bounds.y + bounds.height.max(1) - 1);

        let sx_str = cx.to_string();
        let sy_str = cy.to_string();
        let ex_str = end_x.to_string();
        let ey_str = end_y.to_string();

        self.execute_adb(&[
            "shell", "input", "swipe", &sx_str, &sy_str, &ex_str, &ey_str, "250",
        ])
        .await?;
        Ok(())
    }

    async fn inspect_ui_tree(&self, hwnd: u64) -> Result<UiTree, CuaError> {
        self.check_cancellation()?;

        // Primary strategy: exec-out uiautomator dump /dev/tty
        let mut xml_dump = self
            .execute_adb(&["exec-out", "uiautomator", "dump", "/dev/tty"])
            .await
            .unwrap_or_default();

        // Secondary fallback strategy: shell uiautomator dump to file + cat
        if xml_dump.trim().is_empty() || !xml_dump.contains("<node") {
            let _ = self
                .execute_adb(&["shell", "uiautomator", "dump", "/sdcard/window_dump.xml"])
                .await;
            xml_dump = self
                .execute_adb(&["shell", "cat", "/sdcard/window_dump.xml"])
                .await
                .unwrap_or_default();
        }

        if xml_dump.trim().is_empty() {
            return Err(CuaError::Internal(
                "uiautomator dump returned empty hierarchy output".into(),
            ));
        }

        let adb_tree = parse_uiautomator_xml(&xml_dump)?;

        // Update screen bounds from root node if available
        if adb_tree.screen_width > 0 && adb_tree.screen_height > 0 {
            *self.screen_bounds.write() =
                CuaRect::new(0, 0, adb_tree.screen_width, adb_tree.screen_height);
        }

        // Cache parsed tree
        *self.cached_ui_tree.write() = Some(adb_tree.clone());

        let target_hwnd = if hwnd == 0 { self.synthetic_hwnd } else { hwnd };
        let mut ui_tree = adb_tree.to_ui_tree(target_hwnd);
        SoMIndexer::index_tree(&mut ui_tree);

        Ok(ui_tree)
    }

    async fn dispatch_action(&self, action: CuaAction) -> Result<CuaActionResult, CuaError> {
        let start_time = Instant::now();
        self.check_cancellation()?;

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

        let action_name = match action {
            CuaAction::Click { .. } => "click",
            CuaAction::MoveCursor { .. } => "move_cursor",
            CuaAction::Drag { .. } => "drag",
            CuaAction::Scroll { .. } => "scroll",
            CuaAction::TypeText { .. } => "type_text",
            CuaAction::PressKey { .. } => "press_key",
            CuaAction::Hotkey { .. } => "hotkey",
            CuaAction::BringToFront { .. } => "bring_to_front",
            CuaAction::RestoreWindow { .. } => "restore_window",
        };

        match action {
            CuaAction::Click {
                x, y, click_count, ..
            } => {
                let bounds = *self.screen_bounds.read();
                let (cx, cy) = match (x, y) {
                    (Some(px), Some(py)) => (px, py),
                    _ => bounds.center(),
                };

                let sx = cx.to_string();
                let sy = cy.to_string();

                if click_count >= 2 {
                    self.execute_adb(&["shell", "input", "tap", &sx, &sy])
                        .await?;
                    self.check_cancellation()?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    self.check_cancellation()?;
                    self.execute_adb(&["shell", "input", "tap", &sx, &sy])
                        .await?;
                } else {
                    self.execute_adb(&["shell", "input", "tap", &sx, &sy])
                        .await?;
                }
            }

            CuaAction::MoveCursor { .. } => {
                // Touch screens do not track cursor hover; no-op success
            }

            CuaAction::Drag {
                start_x,
                start_y,
                end_x,
                end_y,
                steps,
                ..
            } => {
                let duration_ms = if steps > 0 {
                    (steps * 20).max(100)
                } else {
                    300
                };
                let sx = start_x.to_string();
                let sy = start_y.to_string();
                let ex = end_x.to_string();
                let ey = end_y.to_string();
                let dur = duration_ms.to_string();

                self.execute_adb(&["shell", "input", "swipe", &sx, &sy, &ex, &ey, &dur])
                    .await?;
            }

            CuaAction::Scroll {
                delta_x,
                delta_y,
                x,
                y,
                ..
            } => {
                let bounds = *self.screen_bounds.read();
                let (cx, cy) = match (x, y) {
                    (Some(px), Some(py)) => (px, py),
                    _ => bounds.center(),
                };

                let end_x = (cx - delta_x).clamp(bounds.x, bounds.x + bounds.width.max(1) - 1);
                let end_y = (cy - delta_y).clamp(bounds.y, bounds.y + bounds.height.max(1) - 1);

                let sx = cx.to_string();
                let sy = cy.to_string();
                let ex = end_x.to_string();
                let ey = end_y.to_string();

                self.execute_adb(&["shell", "input", "swipe", &sx, &sy, &ex, &ey, "250"])
                    .await?;
            }

            CuaAction::TypeText { ref text, .. } => {
                let delay_opt = *self.typing_delay.read();
                if let Some(delay) = delay_opt {
                    let token_opt = self.cancel_token.read().clone();
                    for ch in text.chars() {
                        if let Some(ref token) = token_opt {
                            if token.is_cancelled() {
                                return Err(CuaError::EmergencyHalted);
                            }
                        }
                        let escaped = escape_adb_text(&ch.to_string());
                        self.execute_adb(&["shell", "input", "text", &escaped])
                            .await?;

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
                } else {
                    self.check_cancellation()?;
                    let escaped = escape_adb_text(text);
                    self.execute_adb(&["shell", "input", "text", &escaped])
                        .await?;
                }
            }

            CuaAction::PressKey { ref key, down, .. } => {
                if down {
                    let keycode = map_key_to_android_keycode(key).unwrap_or(66);
                    let kc_str = keycode.to_string();
                    self.execute_adb(&["shell", "input", "keyevent", &kc_str])
                        .await?;
                }
            }

            CuaAction::Hotkey { ref keys, .. } => {
                for k in keys {
                    self.check_cancellation()?;
                    let keycode = map_key_to_android_keycode(k).unwrap_or(66);
                    let kc_str = keycode.to_string();
                    self.execute_adb(&["shell", "input", "keyevent", &kc_str])
                        .await?;
                }
            }

            CuaAction::BringToFront { .. } => {
                let pkg_opt = self.target_package.read().clone();
                if let Some(pkg) = pkg_opt {
                    self.execute_adb(&[
                        "shell",
                        "monkey",
                        "-p",
                        &pkg,
                        "-c",
                        "android.intent.category.LAUNCHER",
                        "1",
                    ])
                    .await?;
                } else {
                    self.execute_adb(&["shell", "input", "keyevent", "3"])
                        .await?; // KEYCODE_HOME
                }
            }

            CuaAction::RestoreWindow { .. } => {
                self.restore_window(target_hwnd).await?;
            }
        }

        Ok(CuaActionResult {
            success: true,
            action_type: action_name.to_string(),
            target_hwnd,
            target_pid: Some(1000),
            delivery_used: CuaDeliveryMode::Background,
            effect: CuaActionEffect::Completed,
            error: None,
            duration_ms: start_time.elapsed().as_millis() as u64,
            timestamp_utc: crate::types::current_iso8601_utc(),
        })
    }
}

/// Escapes a text string for safe transmission via `adb shell input text <escaped>`.
/// Spaces are substituted with `%s`, and shell-special characters are escaped with `\`.
pub fn escape_adb_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len() * 2);
    for ch in text.chars() {
        match ch {
            ' ' => escaped.push_str("%s"),
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\'' => escaped.push_str("\\\'"),
            '&' => escaped.push_str("\\&"),
            '<' => escaped.push_str("\\<"),
            '>' => escaped.push_str("\\>"),
            '|' => escaped.push_str("\\|"),
            ';' => escaped.push_str("\\;"),
            '(' => escaped.push_str("\\("),
            ')' => escaped.push_str("\\)"),
            '$' => escaped.push_str("\\$"),
            '`' => escaped.push_str("\\`"),
            '*' => escaped.push_str("\\*"),
            '?' => escaped.push_str("\\?"),
            '[' => escaped.push_str("\\["),
            ']' => escaped.push_str("\\]"),
            '{' => escaped.push_str("\\{"),
            '}' => escaped.push_str("\\}"),
            _ => escaped.push(ch),
        }
    }
    escaped
}

/// Maps common key identifier strings to Android KeyEvent integer keycodes.
pub fn map_key_to_android_keycode(key: &str) -> Option<u32> {
    if let Ok(num) = key.parse::<u32>() {
        return Some(num);
    }

    let lower = key.to_lowercase();
    match lower.as_str() {
        "home" | "keycode_home" => Some(3),
        "back" | "keycode_back" => Some(4),
        "call" => Some(5),
        "endcall" => Some(6),
        "0" => Some(7),
        "1" => Some(8),
        "2" => Some(9),
        "3" => Some(10),
        "4" => Some(11),
        "5" => Some(12),
        "6" => Some(13),
        "7" => Some(14),
        "8" => Some(15),
        "9" => Some(16),
        "star" => Some(17),
        "pound" => Some(18),
        "dpad_up" | "up" => Some(19),
        "dpad_down" | "down" => Some(20),
        "dpad_left" | "left" => Some(21),
        "dpad_right" | "right" => Some(22),
        "dpad_center" => Some(23),
        "volume_up" | "volup" => Some(24),
        "volume_down" | "voldown" => Some(25),
        "power" => Some(26),
        "camera" => Some(27),
        "clear" => Some(28),
        "tab" => Some(61),
        "space" => Some(62),
        "enter" | "return" => Some(66),
        "del" | "backspace" => Some(67),
        "menu" => Some(82),
        "search" => Some(84),
        "escape" | "esc" => Some(111),
        "app_switch" | "recent" | "recents" => Some(187),
        "wakeup" | "wake" => Some(224),
        _ => {
            if lower.len() == 1 {
                let ch = lower.chars().next().unwrap();
                if ch.is_ascii_lowercase() {
                    Some(29 + (ch as u32 - 'a' as u32))
                } else {
                    None
                }
            } else {
                None
            }
        }
    }
}
