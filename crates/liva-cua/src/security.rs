//! Security Governor & Permission Policy Engine for the CUA subsystem.
//!
//! Enforces:
//! - Multi-tier Permission Modes: Standard (read-only), Bounded (safe enterprise), Unrestricted.
//! - Bounded Mode Policy Rules:
//!   - Configured process allowlist matching with UWP child delegation.
//!   - Protected window denylist (kernel, UAC dialogs, taskbar, desktop, taskmgr, password vaults).
//!   - Dangerous hotkey filter (blocking Win+L, Ctrl+Alt+Del, Win+R, Win+X, Ctrl+Shift+Esc, Alt+F4).
//!   - Client coordinate containment (clamping to client rect, preventing title-bar/close-button clicks).
//!   - Engine-level UIPI enforcement across all actions.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::RwLock;

use tracing::warn;

#[cfg(windows)]
use crate::input::uipi::check_uipi_restriction;
use crate::types::{CuaAction, CuaConfig, CuaError, CuaRect, CuaWindowInfo, PermissionMode};

// ---------------------------------------------------------------------------
// Constants & Catalogs
// ---------------------------------------------------------------------------

/// Core OS kernel processes and critical daemons (protected across ALL modes).
pub const KERNEL_PROCESS_DENYLIST: &[&str] = &[
    "csrss.exe",
    "lsass.exe",
    "services.exe",
    "wininit.exe",
    "dwm.exe",
    "smss.exe",
    "liva-native-core.exe",
];

/// Windows Security & UAC elevation dialog executables.
pub const SECURITY_ELEVATION_DENYLIST: &[&str] =
    &["consent.exe", "credentialdialoghost.exe", "sechealthui.exe"];

/// Protected Windows shell window class names.
pub const PROTECTED_SHELL_CLASSES: &[&str] = &[
    "Shell_TrayWnd",          // Primary Taskbar
    "Shell_SecondaryTrayWnd", // Secondary Multi-monitor Taskbar
    "Progman",                // Desktop Window Manager
    "WorkerW",                // Desktop Wallpaper / Icon Host
];

/// Administrative, system configuration, and registry tools.
pub const ADMIN_TOOLS_DENYLIST: &[&str] = &["taskmgr.exe", "regedit.exe", "mmc.exe"];

/// Popular password managers and credential vaults.
pub const PASSWORD_VAULT_DENYLIST: &[&str] = &[
    "bitwarden.exe",
    "1password.exe",
    "keepass.exe",
    "keepassxc.exe",
];

/// Default caption / title-bar height inset in physical pixels for client bounds fallback.
pub const DEFAULT_CAPTION_INSET_PX: i32 = 30;

/// Default window frame border inset in physical pixels.
pub const DEFAULT_BORDER_INSET_PX: i32 = 8;

// ---------------------------------------------------------------------------
// Security Governor Implementation
// ---------------------------------------------------------------------------

/// Central security authority enforcing permission boundaries and sandboxing.
pub struct SecurityGovernor {
    permission_mode: RwLock<PermissionMode>,
    process_allowlist: RwLock<Vec<String>>,
    protected_denylist: RwLock<Vec<String>>,
    simulated_uipi_rids: RwLock<HashMap<u64, u32>>,
    simulated_agent_rid: AtomicU32,
}

impl SecurityGovernor {
    /// Initialize with configuration defaults.
    pub fn new(config: &CuaConfig) -> Self {
        Self {
            permission_mode: RwLock::new(config.permission_mode),
            process_allowlist: RwLock::new(config.process_allowlist.clone()),
            protected_denylist: RwLock::new(config.protected_denylist.clone()),
            simulated_uipi_rids: RwLock::new(HashMap::new()),
            simulated_agent_rid: AtomicU32::new(0x2000), // Medium integrity (0x2000)
        }
    }

    /// Set permission mode at runtime.
    pub fn set_permission_mode(&self, mode: PermissionMode) {
        let mut lock = self.permission_mode.write().unwrap();
        *lock = mode;
    }

    /// Get current permission mode.
    pub fn get_permission_mode(&self) -> PermissionMode {
        *self.permission_mode.read().unwrap()
    }

    /// Update process allowlist.
    pub fn set_allowlist(&self, allowlist: Vec<String>) {
        let mut lock = self.process_allowlist.write().unwrap();
        *lock = allowlist;
    }

    /// Get current process allowlist.
    pub fn get_allowlist(&self) -> Vec<String> {
        self.process_allowlist.read().unwrap().clone()
    }

    /// Update custom protected denylist.
    pub fn set_denylist(&self, denylist: Vec<String>) {
        let mut lock = self.protected_denylist.write().unwrap();
        *lock = denylist;
    }

    /// Register simulated UIPI RID for a window (used in unit tests and mock harness).
    pub fn set_simulated_uipi(&self, hwnd: u64, target_rid: u32) {
        self.simulated_uipi_rids
            .write()
            .unwrap()
            .insert(hwnd, target_rid);
    }

    /// Set simulated agent RID (used in unit tests).
    pub fn set_simulated_agent_rid(&self, agent_rid: u32) {
        self.simulated_agent_rid.store(agent_rid, Ordering::SeqCst);
    }

    // -----------------------------------------------------------------------
    // Core Action Evaluation Pipeline
    // -----------------------------------------------------------------------

    /// Comprehensive pre-execution evaluation for discrete CUA actions.
    ///
    /// Evaluates:
    /// 1. Standard mode read-only barrier.
    /// 2. Protected window denylist (kernel, UAC, taskmgr, password vaults).
    /// 3. Engine-level UIPI elevation check across all action types.
    /// 4. Bounded mode process allowlist matching.
    /// 5. Bounded mode dangerous hotkey filtering.
    /// 6. Bounded mode client coordinate containment.
    pub fn evaluate_action(
        &self,
        action: &CuaAction,
        target: &CuaWindowInfo,
    ) -> Result<(), CuaError> {
        // Step 1: Standard Mode read-only enforcement
        self.evaluate_permission_mode(action)?;

        // Step 2: Engine-level UIPI verification across ALL actions (OS mandatory integrity barrier)
        self.evaluate_uipi(target.hwnd, target.pid)?;

        // Step 3: Mode-specific policy rules
        let mode = self.get_permission_mode();
        match mode {
            PermissionMode::Standard => Ok(()),
            PermissionMode::Bounded => {
                // Dangerous hotkeys checked first so dangerous keys are rejected across all apps
                self.evaluate_dangerous_hotkeys(action)?;
                // Protected window denylist takes precedence over allowlist
                self.evaluate_protected_denylist(target)?;
                // Process allowlist containment
                self.evaluate_process_allowlist(target)?;
                // Client coordinate containment
                self.evaluate_coordinate_containment(action, target)?;
                Ok(())
            }
            PermissionMode::Unrestricted => {
                // Denylist kernel protection remains active
                self.evaluate_protected_denylist(target)?;
                Ok(())
            }
        }
    }

    // -----------------------------------------------------------------------
    // Sub-Evaluators
    // -----------------------------------------------------------------------

    /// Evaluates whether the action is permitted under the active PermissionMode.
    pub fn evaluate_permission_mode(&self, action: &CuaAction) -> Result<(), CuaError> {
        let mode = self.get_permission_mode();
        if mode == PermissionMode::Standard {
            match action {
                CuaAction::BringToFront { .. } | CuaAction::RestoreWindow { .. } => {
                    // Window focus / un-minimize permitted in Standard mode for inspection
                    Ok(())
                }
                _ => {
                    warn!(action = ?action, "Desktop mutating action blocked by Standard permission mode");
                    Err(CuaError::PermissionDenied(
                        "Standard permission mode is read-only. Desktop mutating actions are disallowed.".into(),
                    ))
                }
            }
        } else {
            Ok(())
        }
    }

    /// Evaluates target window against the protected window denylist.
    pub fn evaluate_protected_denylist(&self, target: &CuaWindowInfo) -> Result<(), CuaError> {
        // 1. PID Invariants: PID 0 (System Idle) & PID 4 (System Kernel)
        if target.pid == 0 || target.pid == 4 {
            return Err(CuaError::PermissionDenied(format!(
                "Target HWND {:#x} belongs to core OS kernel process (PID {}). Access forbidden across all modes.",
                target.hwnd, target.pid
            )));
        }

        // 2. Kernel & core daemons (Blocked across all modes, including Unrestricted)
        if let Some(ref proc_name) = target.process_name {
            let base = extract_base_filename(proc_name).to_ascii_lowercase();
            if KERNEL_PROCESS_DENYLIST.contains(&base.as_str()) {
                return Err(CuaError::PermissionDenied(format!(
                    "Target process '{}' (PID {}) is a protected system kernel process.",
                    base, target.pid
                )));
            }
        }

        // If Unrestricted mode, permit general user-space applications
        let mode = self.get_permission_mode();
        if mode == PermissionMode::Unrestricted {
            return Ok(());
        }

        // 3. UAC elevation prompts & security hosts
        if let Some(ref proc_name) = target.process_name {
            let base = extract_base_filename(proc_name).to_ascii_lowercase();
            if SECURITY_ELEVATION_DENYLIST.contains(&base.as_str()) {
                return Err(CuaError::PermissionDenied(format!(
                    "Target process '{}' (PID {}) is a protected security elevation prompt.",
                    base, target.pid
                )));
            }
        }

        // 4. Shell components (Taskbar, Desktop icons)
        if PROTECTED_SHELL_CLASSES.contains(&target.class_name.as_str()) {
            return Err(CuaError::PermissionDenied(format!(
                "Target window belongs to protected system shell class '{}'.",
                target.class_name
            )));
        }

        // 5. Windows Dialogs with security warning captions
        if target.class_name == "#32770" {
            let title_lower = target.title.to_ascii_lowercase();
            if title_lower.contains("user account control")
                || title_lower.contains("windows security")
                || title_lower.contains("security warning")
                || title_lower.contains("smartscreen")
            {
                return Err(CuaError::PermissionDenied(format!(
                    "Target dialog is a protected security prompt ('{}').",
                    target.title
                )));
            }
        }

        // 6. Administrative Tools & Task Manager
        if let Some(ref proc_name) = target.process_name {
            let base = extract_base_filename(proc_name).to_ascii_lowercase();
            if ADMIN_TOOLS_DENYLIST.contains(&base.as_str()) {
                return Err(CuaError::PermissionDenied(format!(
                    "Target process '{}' is a protected administrative tool.",
                    base
                )));
            }
        }

        // 7. Password Vaults & Credential Managers
        if let Some(ref proc_name) = target.process_name {
            let base = extract_base_filename(proc_name).to_ascii_lowercase();
            if PASSWORD_VAULT_DENYLIST.contains(&base.as_str()) {
                return Err(CuaError::PermissionDenied(format!(
                    "Target process '{}' is a protected credential/password vault.",
                    base
                )));
            }
        }

        // 8. Custom configured denylist
        let custom_denylist = self.protected_denylist.read().unwrap();
        if let Some(ref proc_name) = target.process_name {
            let base = extract_base_filename(proc_name).to_ascii_lowercase();
            if custom_denylist
                .iter()
                .any(|d| extract_base_filename(d).to_ascii_lowercase() == base)
            {
                return Err(CuaError::PermissionDenied(format!(
                    "Target process '{}' is on the user-configured denylist.",
                    base
                )));
            }
        }

        Ok(())
    }

    /// Evaluates target process against the configured Bounded mode allowlist.
    pub fn evaluate_process_allowlist(&self, target: &CuaWindowInfo) -> Result<(), CuaError> {
        let allowlist = self.process_allowlist.read().unwrap();
        if allowlist.is_empty() {
            return Err(CuaError::PermissionDenied(
                "Bounded permission mode requires a non-empty process allowlist.".into(),
            ));
        }

        let target_base = target
            .process_name
            .as_deref()
            .map(|p| extract_base_filename(p).to_ascii_lowercase());

        // Check outer process name against allowlist
        let mut is_allowed = target_base
            .as_ref()
            .map(|tb| {
                allowlist
                    .iter()
                    .any(|a| extract_base_filename(a).to_ascii_lowercase() == *tb)
            })
            .unwrap_or(false);

        // If not matched directly and window is a UWP ApplicationFrameWindow, check child app PID
        if !is_allowed && target.is_uwp_frame {
            if let Some(child_pid) = target.uwp_app_pid {
                #[cfg(windows)]
                {
                    if let Some(child_name) = crate::window::get_process_name(child_pid) {
                        let child_base = extract_base_filename(&child_name).to_ascii_lowercase();
                        is_allowed = allowlist
                            .iter()
                            .any(|a| extract_base_filename(a).to_ascii_lowercase() == child_base);
                    }
                }
                #[cfg(not(windows))]
                let _ = child_pid;
            }
        }

        if !is_allowed {
            let proc_str = target.process_name.as_deref().unwrap_or("unknown");
            warn!(
                hwnd = format!("{:#x}", target.hwnd),
                process = proc_str,
                pid = target.pid,
                "Action rejected: process not in Bounded mode allowlist"
            );
            return Err(CuaError::PermissionDenied(format!(
                "Process '{}' (PID {}) is not in the authorized Bounded mode allowlist.",
                proc_str, target.pid
            )));
        }

        Ok(())
    }

    /// Evaluates hotkey combinations against dangerous system hotkey rules.
    pub fn evaluate_dangerous_hotkeys(&self, action: &CuaAction) -> Result<(), CuaError> {
        if let CuaAction::Hotkey { keys, .. } = action {
            let canonical_keys: HashSet<String> =
                keys.iter().map(|k| canonicalize_key_token(k)).collect();

            // List of forbidden dangerous key combinations
            let dangerous_combinations: &[&[&str]] = &[
                &["win", "l"],             // Lock Workstation
                &["ctrl", "alt", "del"],   // Windows Security Screen
                &["win", "r"],             // Run Dialog (Process Execution)
                &["win", "x"],             // Power User / Quick Link Menu
                &["ctrl", "shift", "esc"], // Task Manager Direct Shortcut
                &["alt", "f4"],            // Close Active Window
            ];

            for dangerous_set in dangerous_combinations {
                if dangerous_set.iter().all(|dk| canonical_keys.contains(*dk)) {
                    warn!(
                        hotkey = keys.join("+"),
                        "Dangerous hotkey rejected by Bounded policy"
                    );
                    return Err(CuaError::DangerousHotkeyRejected(keys.join("+")));
                }
            }
        }

        Ok(())
    }

    /// Evaluates action coordinates against window client rectangle containment.
    pub fn evaluate_coordinate_containment(
        &self,
        action: &CuaAction,
        target: &CuaWindowInfo,
    ) -> Result<(), CuaError> {
        let (x, y) = match action {
            CuaAction::Click {
                x: Some(cx),
                y: Some(cy),
                ..
            } => (*cx, *cy),
            CuaAction::MoveCursor { x, y, .. } => (*x, *y),
            CuaAction::Scroll {
                x: Some(sx),
                y: Some(sy),
                ..
            } => (*sx, *sy),
            CuaAction::Drag {
                start_x,
                start_y,
                end_x,
                end_y,
                ..
            } => {
                // Check both start and end coordinates
                self.verify_point_contained(*start_x, *start_y, target)?;
                self.verify_point_contained(*end_x, *end_y, target)?;
                return Ok(());
            }
            _ => return Ok(()),
        };

        self.verify_point_contained(x, y, target)
    }

    /// Validates that a single screen point (x, y) resides strictly within the window's client bounds.
    fn verify_point_contained(
        &self,
        x: i32,
        y: i32,
        target: &CuaWindowInfo,
    ) -> Result<(), CuaError> {
        let bounds = target.bounds;

        // 1. Point must be within overall window bounding rectangle
        if !bounds.contains_point(x, y) {
            warn!(
                x = x,
                y = y,
                bounds = ?bounds,
                "Point is strictly outside target window bounds"
            );
            return Err(CuaError::CoordinateOutOfBounds { x, y });
        }

        // 2. Resolve client area rectangle
        let client_rect = compute_client_rect(&target.bounds);

        // 3. Point must be inside the client drawing surface
        if !client_rect.contains_point(x, y) {
            if y < client_rect.y {
                warn!(
                    x = x,
                    y = y,
                    client_rect = ?client_rect,
                    "Target coordinate hits non-client title bar / caption"
                );
            }
            return Err(CuaError::CoordinateOutOfBounds { x, y });
        }

        // 4. Close button protection (top-right corner region)
        if x >= bounds.x + bounds.width - 50 && y < client_rect.y + 10 {
            warn!(x = x, y = y, "Target coordinate hits close button region");
            return Err(CuaError::CoordinateOutOfBounds { x, y });
        }

        // 5. Live Win32 Hit-Testing check (if running on Windows and handle is valid)
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::{HWND, WPARAM};
            use windows_sys::Win32::UI::WindowsAndMessaging::{
                IsWindow, SendMessageW, HTCLIENT, WM_NCHITTEST,
            };

            let win_hwnd = target.hwnd as usize as HWND;
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } != 0 {
                let lparam = crate::geometry::pack_lparam_clamped(x, y);
                let hit_code = unsafe { SendMessageW(win_hwnd, WM_NCHITTEST, 0 as WPARAM, lparam) };
                // If the hit test explicitly returned a non-client element (HTCAPTION, HTCLOSE, etc.)
                if hit_code != 0 && hit_code != (HTCLIENT as isize) {
                    warn!(
                        hwnd = format!("{:#x}", target.hwnd),
                        hit_code = hit_code,
                        "WM_NCHITTEST rejected non-client target point"
                    );
                    return Err(CuaError::CoordinateOutOfBounds { x, y });
                }
            }
        }

        Ok(())
    }

    /// Evaluates whether synthetic input is permitted under Windows UIPI rules.
    pub fn evaluate_uipi(&self, target_hwnd: u64, _target_pid: u32) -> Result<(), CuaError> {
        // 1. Check simulated overrides (for test harness)
        if let Some(&target_rid) = self.simulated_uipi_rids.read().unwrap().get(&target_hwnd) {
            let agent_rid = self.simulated_agent_rid.load(Ordering::SeqCst);
            if target_rid > agent_rid {
                warn!(
                    hwnd = format!("{:#x}", target_hwnd),
                    target_rid = format!("{:#x}", target_rid),
                    agent_rid = format!("{:#x}", agent_rid),
                    "Simulated UIPI elevation blocked"
                );
                return Err(CuaError::uipi_blocked_details(
                    target_hwnd,
                    target_rid,
                    agent_rid,
                ));
            }
            return Ok(());
        }

        // 2. Live Win32 process token integrity query
        #[cfg(windows)]
        {
            use windows_sys::Win32::Foundation::HWND;
            use windows_sys::Win32::UI::WindowsAndMessaging::IsWindow;

            let win_hwnd = target_hwnd as usize as HWND;
            if win_hwnd != 0 && unsafe { IsWindow(win_hwnd) } != 0 {
                let verdict = check_uipi_restriction(target_hwnd)?;
                if verdict.is_blocked {
                    warn!(
                        hwnd = format!("{:#x}", target_hwnd),
                        verdict = ?verdict,
                        "Live UIPI restriction blocked"
                    );
                    return Err(CuaError::UipiBlocked(
                        verdict
                            .refusal_message
                            .unwrap_or_else(|| "UIPI elevation restriction".into()),
                    ));
                }
            }
        }

        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Helper Functions
// ---------------------------------------------------------------------------

/// Normalizes key names to canonical tokens for order-independent subset matching.
pub fn canonicalize_key_token(token: &str) -> String {
    let lower = token.trim().to_ascii_lowercase();
    match lower.as_str() {
        "ctrl" | "control" | "lctrl" | "rctrl" => "ctrl".into(),
        "alt" | "menu" | "lalt" | "ralt" => "alt".into(),
        "win" | "windows" | "super" | "lwin" | "rwin" => "win".into(),
        "del" | "delete" => "del".into(),
        "esc" | "escape" => "esc".into(),
        "shift" | "lshift" | "rshift" => "shift".into(),
        "return" | "enter" => "enter".into(),
        other => other.into(),
    }
}

/// Extracts the bare file name from an executable path (e.g. "C:\\Windows\\notepad.exe" -> "notepad.exe").
pub fn extract_base_filename(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or(path)
        .to_string()
}

/// Computes the client rectangle inside a window bounding box using standard OS insets.
pub fn compute_client_rect(bounds: &CuaRect) -> CuaRect {
    let cx = bounds.x + DEFAULT_BORDER_INSET_PX;
    let cy = bounds.y + DEFAULT_CAPTION_INSET_PX;
    let cw = (bounds.width - 2 * DEFAULT_BORDER_INSET_PX).max(0);
    let ch = (bounds.height - DEFAULT_CAPTION_INSET_PX - DEFAULT_BORDER_INSET_PX).max(0);
    CuaRect::new(cx, cy, cw, ch)
}
