//! Native Android ADB Automation Driver and Hierarchy Parser for LIVA CUA.
//!
//! Provides:
//! - `AdbTransport` trait, `MockAdbTransport`, and `ProcessAdbTransport`.
//! - Zero-dependency XML parser for Android `uiautomator dump` hierarchies into Set-of-Mark indexed trees.
//! - `parse_adb_bounds` coordinate center calculation.
//! - `AdbCuaDriver` implementing `CuaDriverTrait` with security governance and kill switch.
//! - Representative XML mock fixtures (`settings_app_xml`, `calculator_app_xml`, etc.).

pub mod driver;
pub mod mock;
pub mod parser;
pub mod transport;

pub use driver::{
    escape_adb_text, map_key_to_android_keycode, AdbCuaDriver, ANDROID_SYNTHETIC_HWND_BASE,
};
pub use mock::{calculator_app_xml, home_lock_screen_xml, login_form_dialog_xml, settings_app_xml};
pub use parser::{
    classify_android_role, parse_adb_bounds, parse_uiautomator_to_ui_tree, parse_uiautomator_xml,
    unescape_xml, AdbSomElement, AdbUiNode, AdbUiTree,
};
pub use transport::{AdbTransport, MockAdbTransport, ProcessAdbTransport};

use serde::{Deserialize, Serialize};

/// Android device information descriptor parsed from `adb devices -l`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdbDevice {
    pub serial: String,
    pub state: String,
    pub product: Option<String>,
    pub model: Option<String>,
    pub device: Option<String>,
    pub transport_id: Option<String>,
}

/// Parses the output of `adb devices -l` into a list of `AdbDevice` descriptors.
pub fn parse_adb_devices(output: &str) -> Vec<AdbDevice> {
    let mut devices = Vec::new();
    for line in output.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("List of devices") || trimmed.starts_with('*')
        {
            continue;
        }

        let tokens: Vec<&str> = trimmed.split_whitespace().collect();
        if tokens.len() < 2 {
            continue;
        }

        let serial = tokens[0].to_string();
        let state = tokens[1].to_string();

        let mut product = None;
        let mut model = None;
        let mut device = None;
        let mut transport_id = None;

        for token in &tokens[2..] {
            if let Some(val) = token.strip_prefix("product:") {
                product = Some(val.to_string());
            } else if let Some(val) = token.strip_prefix("model:") {
                model = Some(val.to_string());
            } else if let Some(val) = token.strip_prefix("device:") {
                device = Some(val.to_string());
            } else if let Some(val) = token.strip_prefix("transport_id:") {
                transport_id = Some(val.to_string());
            }
        }

        devices.push(AdbDevice {
            serial,
            state,
            product,
            model,
            device,
            transport_id,
        });
    }
    devices
}

/// Configuration settings for the Android ADB subsystem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdbConfig {
    pub serial: Option<String>,
    pub adb_path: String,
    pub timeout_ms: u64,
    pub default_screen_width: i32,
    pub default_screen_height: i32,
    pub default_package: Option<String>,
}

impl Default for AdbConfig {
    fn default() -> Self {
        Self {
            serial: None,
            adb_path: "adb".to_string(),
            timeout_ms: 5000,
            default_screen_width: 1080,
            default_screen_height: 2400,
            default_package: Some("com.android.settings".to_string()),
        }
    }
}
