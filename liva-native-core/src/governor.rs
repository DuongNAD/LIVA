//! Game-mode resource governor: keeps LIVA polite while the user plays.
//!
//! Detection (Windows): the foreground window is borderless-fullscreen on the
//! primary display and belongs to another process (desktop shell excluded).
//! When game mode is active the process priority drops to BELOW_NORMAL so
//! LIVA's inference never steals frame time; STT/VAD/TTS are already
//! CPU-light (2 intra-op threads each). LLM thread count is baked in at model
//! load (`LIVA_LLM_THREADS`, see `llm::engine`) — swapping to a smaller model
//! or fewer GPU layers on mode change requires a model reload and is a
//! follow-up (documented in the overhaul plan).
//!
//! Env:
//! - `LIVA_GAME_MODE`      = auto | on | off   (default auto)
//! - `LIVA_GAME_PRIORITY`  = off to disable the priority drop (default on)

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GovernorMode {
    /// Detect fullscreen apps automatically.
    Auto,
    /// Always behave as if a game is running.
    ForcedOn,
    /// Never throttle.
    Off,
}

impl GovernorMode {
    fn from_env() -> Self {
        match std::env::var("LIVA_GAME_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "on" | "force" | "forced" => Self::ForcedOn,
            "off" | "disable" | "disabled" => Self::Off,
            _ => Self::Auto,
        }
    }
}

pub struct Governor {
    mode: GovernorMode,
    manage_priority: bool,
    active: AtomicBool,
    priority_lowered: AtomicBool,
    last_check: Mutex<Option<Instant>>,
}

const CHECK_INTERVAL: Duration = Duration::from_secs(2);

impl Governor {
    pub fn from_env() -> Self {
        Self {
            mode: GovernorMode::from_env(),
            manage_priority: std::env::var("LIVA_GAME_PRIORITY")
                .map(|v| v.to_lowercase() != "off")
                .unwrap_or(true),
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        }
    }

    pub fn mode(&self) -> GovernorMode {
        self.mode
    }

    /// True when LIVA should minimize its resource footprint. Result is
    /// cached for [`CHECK_INTERVAL`]; cheap to call on hot paths.
    pub fn game_mode_active(&self) -> bool {
        match self.mode {
            GovernorMode::ForcedOn => {
                self.apply_priority(true);
                return true;
            }
            GovernorMode::Off => return false,
            GovernorMode::Auto => {}
        }

        let mut last = self.last_check.lock().unwrap();
        let stale = last.map_or(true, |t| t.elapsed() >= CHECK_INTERVAL);
        if stale {
            let detected = foreground_is_fullscreen();
            self.active.store(detected, Ordering::Relaxed);
            *last = Some(Instant::now());
            self.apply_priority(detected);
        }
        self.active.load(Ordering::Relaxed)
    }

    fn apply_priority(&self, game_active: bool) {
        if !self.manage_priority {
            return;
        }
        let lowered = self.priority_lowered.load(Ordering::Relaxed);
        if game_active == lowered {
            return;
        }
        set_process_below_normal(game_active);
        self.priority_lowered.store(game_active, Ordering::Relaxed);
        tracing::info!(
            "Game mode {} — process priority {}",
            if game_active { "ON" } else { "OFF" },
            if game_active { "below-normal" } else { "normal" }
        );
    }
}

/// Stateless game-mode check (respects `LIVA_GAME_MODE`), for callers without a
/// `Governor` instance — e.g. the vision path deciding whether to use a cheap
/// mouse-guided crop instead of a full-screen capture. Not cached; call only on
/// rare paths (a vision request), not hot loops.
pub fn game_mode_active_now() -> bool {
    match GovernorMode::from_env() {
        GovernorMode::ForcedOn => true,
        GovernorMode::Off => false,
        GovernorMode::Auto => foreground_is_fullscreen(),
    }
}

#[cfg(windows)]
fn foreground_is_fullscreen() -> bool {
    use windows_sys::Win32::Foundation::RECT;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        GetClassNameW, GetForegroundWindow, GetSystemMetrics, GetWindowRect,
        GetWindowThreadProcessId, SM_CXSCREEN, SM_CYSCREEN,
    };

    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd == 0 {
            return false;
        }

        // Never throttle because of our own windows.
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid == std::process::id() {
            return false;
        }

        // The desktop shell hosts fullscreen-sized windows; exclude it.
        let mut class_buf = [0u16; 64];
        let class_len = GetClassNameW(hwnd, class_buf.as_mut_ptr(), class_buf.len() as i32);
        if class_len > 0 {
            let class = String::from_utf16_lossy(&class_buf[..class_len as usize]);
            if class == "Progman" || class == "WorkerW" {
                return false;
            }
        }

        let mut rect = RECT {
            left: 0,
            top: 0,
            right: 0,
            bottom: 0,
        };
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return false;
        }
        let screen_w = GetSystemMetrics(SM_CXSCREEN);
        let screen_h = GetSystemMetrics(SM_CYSCREEN);

        rect.left <= 0
            && rect.top <= 0
            && (rect.right - rect.left) >= screen_w
            && (rect.bottom - rect.top) >= screen_h
    }
}

#[cfg(not(windows))]
fn foreground_is_fullscreen() -> bool {
    false
}

#[cfg(windows)]
fn set_process_below_normal(lower: bool) {
    use windows_sys::Win32::System::Threading::{
        BELOW_NORMAL_PRIORITY_CLASS, GetCurrentProcess, NORMAL_PRIORITY_CLASS, SetPriorityClass,
    };
    unsafe {
        let class = if lower {
            BELOW_NORMAL_PRIORITY_CLASS
        } else {
            NORMAL_PRIORITY_CLASS
        };
        SetPriorityClass(GetCurrentProcess(), class);
    }
}

#[cfg(not(windows))]
fn set_process_below_normal(_lower: bool) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forced_modes_bypass_detection() {
        let gov = Governor {
            mode: GovernorMode::Off,
            manage_priority: false,
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        };
        assert!(!gov.game_mode_active());

        let gov = Governor {
            mode: GovernorMode::ForcedOn,
            manage_priority: false,
            active: AtomicBool::new(false),
            priority_lowered: AtomicBool::new(false),
            last_check: Mutex::new(None),
        };
        assert!(gov.game_mode_active());
    }
}
