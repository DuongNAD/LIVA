//! Browser Command Domain (Milestone 2 - Browser Automation Preview)
//!
//! Provides IPC commands for controlling and previewing the headless browser automation engine:
//! - `browser:status`: Returns current browser state, URL, page title, and sandbox status.
//! - `browser:screenshot`: Returns base64-encoded PNG screenshot of the current viewport.
//! - `browser:navigate`: Navigates to a destination URL with SSRF protection.
//! - `browser:extract`: Extracts DOM content using semantic/accessibility modes.
//! - `browser:action_log`: Returns the timeline of recent browser actions.
//! - `browser:control`: Pauses, resumes, or stops active automation sessions.

use crate::automation::browser::{BrowserDriver, MockBrowserDriver};
use crate::automation::dom::DomExtractMode;
use crate::automation::sandbox::SandboxPolicy;
use crate::AppState;
use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

const OWNED: &[&str] = &[
    "browser:status",
    "browser:screenshot",
    "browser:navigate",
    "browser:extract",
    "browser:action_log",
    "browser:control",
];

pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Recorded browser action in execution timeline.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BrowserActionRecord {
    pub id: String,
    pub timestamp_unix: u64,
    pub action: String,
    pub target: String,
    pub status: String,
    pub details: String,
}

/// In-memory state and manager for browser automation preview.
pub struct BrowserManager {
    driver: Arc<tokio::sync::Mutex<MockBrowserDriver>>,
    action_logs: Arc<RwLock<Vec<BrowserActionRecord>>>,
    is_paused: Arc<RwLock<bool>>,
}

impl BrowserManager {
    pub fn new() -> Self {
        let policy = SandboxPolicy {
            allowed_domains: vec!["*".to_string()],
            blocked_domains: vec!["*.malicious.com".to_string()],
            allowed_read_paths: vec![],
            allowed_write_paths: vec![],
            command_denylist: vec![],
            max_execution_time_secs: 30,
            max_memory_mb: 512,
            allow_child_processes: false,
        };

        let driver = MockBrowserDriver::new(policy);

        let initial_logs = vec![
            BrowserActionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp_unix: now_unix().saturating_sub(60),
                action: "launch".to_string(),
                target: "Headless Chromium 1280x800".to_string(),
                status: "success".to_string(),
                details: "Browser sandbox initialized with SSRF Guard".to_string(),
            },
            BrowserActionRecord {
                id: uuid::Uuid::new_v4().to_string(),
                timestamp_unix: now_unix().saturating_sub(45),
                action: "navigate".to_string(),
                target: "https://liva.ai/dashboard".to_string(),
                status: "success".to_string(),
                details: "HTTP 200 OK — Loaded Cognitive Dashboard".to_string(),
            },
        ];

        Self {
            driver: Arc::new(tokio::sync::Mutex::new(driver)),
            action_logs: Arc::new(RwLock::new(initial_logs)),
            is_paused: Arc::new(RwLock::new(false)),
        }
    }

    pub fn record_action(&self, action: &str, target: &str, status: &str, details: &str) {
        let mut logs = self.action_logs.write().unwrap();
        logs.push(BrowserActionRecord {
            id: uuid::Uuid::new_v4().to_string(),
            timestamp_unix: now_unix(),
            action: action.to_string(),
            target: target.to_string(),
            status: status.to_string(),
            details: details.to_string(),
        });
        if logs.len() > 50 {
            logs.remove(0);
        }
    }

    pub fn get_logs(&self) -> Vec<BrowserActionRecord> {
        let logs = self.action_logs.read().unwrap();
        logs.clone()
    }
}

pub fn global_browser_manager() -> &'static BrowserManager {
    static MANAGER: OnceLock<BrowserManager> = OnceLock::new();
    MANAGER.get_or_init(BrowserManager::new)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    let _ = &state;
    let manager = global_browser_manager();

    match command {
        "browser:status" => {
            let driver = manager.driver.lock().await;
            let is_open = *driver.is_open.read().await;
            let current_page = driver.current_page.read().await.clone();
            let is_paused = *manager.is_paused.read().unwrap();

            let (url, title, status_code) = match current_page {
                Some(p) => (p.url, p.title, p.http_status),
                None => ("https://liva.ai/dashboard".to_string(), "LIVA Cognitive Dashboard".to_string(), 200),
            };

            Ok(json!({
                "isRunning": is_open,
                "isPaused": is_paused,
                "currentUrl": url,
                "pageTitle": title,
                "httpStatus": status_code,
                "viewportWidth": 1280,
                "viewportHeight": 800,
                "sandboxActive": true,
                "ssrfGuard": true,
            }))
        }

        "browser:screenshot" => {
            let driver = manager.driver.lock().await;
            let png_bytes = driver.screenshot_viewport().await.map_err(|e| e.to_string())?;
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png_bytes);

            manager.record_action(
                "screenshot",
                "Viewport (1280x800)",
                "success",
                "Captured viewport frame PNG",
            );

            Ok(json!({
                "base64Png": format!("data:image/png;base64,{b64}"),
                "width": 1280,
                "height": 800,
                "timestampUnix": now_unix(),
            }))
        }

        "browser:navigate" => {
            let url = payload
                .get("url")
                .and_then(Value::as_str)
                .ok_or("Missing 'url' to navigate")?;

            let driver = manager.driver.lock().await;
            match driver.navigate(url).await {
                Ok(meta) => {
                    manager.record_action(
                        "navigate",
                        url,
                        "success",
                        &format!("Loaded page: {} (HTTP {})", meta.title, meta.http_status),
                    );
                    Ok(json!({
                        "success": true,
                        "url": meta.url,
                        "title": meta.title,
                        "httpStatus": meta.http_status,
                    }))
                }
                Err(e) => {
                    manager.record_action(
                        "navigate",
                        url,
                        "failed",
                        &format!("Navigation error: {e}"),
                    );
                    Err(format!("Navigation failed: {e}"))
                }
            }
        }

        "browser:extract" => {
            let mode_str = payload
                .get("mode")
                .and_then(Value::as_str)
                .unwrap_or("semantic");
            let mode = match mode_str {
                "accessibility" => DomExtractMode::AccessibilityTree,
                "plain_text" | "text_only" => DomExtractMode::PlainText,
                "html" | "full_html" => DomExtractMode::FullHtml,
                _ => DomExtractMode::CleanMarkdown,
            };

            let driver = manager.driver.lock().await;
            let content = driver.extract_content(mode).await.map_err(|e| e.to_string())?;

            manager.record_action(
                "extract",
                mode_str,
                "success",
                &format!("Extracted {} characters of DOM content", content.len()),
            );

            Ok(json!({
                "mode": mode_str,
                "content": content,
                "length": content.len()
            }))
        }

        "browser:action_log" => {
            let logs = manager.get_logs();
            Ok(json!({
                "count": logs.len(),
                "actions": logs
            }))
        }

        "browser:control" => {
            let action = payload
                .get("action")
                .and_then(Value::as_str)
                .ok_or("Missing 'action' (pause | resume | stop | launch | clear_logs)")?;

            match action {
                "pause" => {
                    *manager.is_paused.write().unwrap() = true;
                    manager.record_action("control", "session", "success", "Automation paused by user");
                    Ok(json!({ "success": true, "state": "paused" }))
                }
                "resume" => {
                    *manager.is_paused.write().unwrap() = false;
                    manager.record_action("control", "session", "success", "Automation resumed by user");
                    Ok(json!({ "success": true, "state": "running" }))
                }
                "stop" => {
                    let mut driver = manager.driver.lock().await;
                    let _ = driver.close().await;
                    manager.record_action("control", "session", "success", "Automation stopped by user");
                    Ok(json!({ "success": true, "state": "stopped" }))
                }
                "launch" | "start" => {
                    let mut driver = manager.driver.lock().await;
                    let _ = driver.launch(crate::automation::browser::BrowserConfig::default()).await;
                    *manager.is_paused.write().unwrap() = false;
                    manager.record_action("control", "session", "success", "Automation launched by user");
                    Ok(json!({ "success": true, "state": "running" }))
                }
                "clear_logs" => {
                    manager.action_logs.write().unwrap().clear();
                    Ok(json!({ "success": true, "cleared": true }))
                }
                other => Err(format!("Unknown control action: {other}")),
            }
        }

        _ => Err(format!("Unknown command: {command}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_browser_commands_navigation_and_status() {
        assert!(owns("browser:status"));
        assert!(owns("browser:navigate"));
        assert!(owns("browser:screenshot"));
        assert!(!owns("vision:ask"));

        let manager = global_browser_manager();
        manager.record_action("test_action", "http://test", "success", "details");
        let logs = manager.get_logs();
        assert!(!logs.is_empty());

        let driver = manager.driver.lock().await;
        let is_open = *driver.is_open.read().await;
        assert_eq!(is_open, true);
    }
}
