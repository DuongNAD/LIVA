use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SmartHomeDevice {
    Light,
    Ac,
    Fan,
}

#[derive(Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum SmartHomeAction {
    On,
    Off,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SmartHomeArgs {
    pub device: SmartHomeDevice,
    pub action: SmartHomeAction,
}

pub fn get_metadata() -> Value {
    json!({
        "name": "smart_home_control",
        "category": "core",
        "short_desc": "Control smart home devices like lights, AC, or fans.",
        "description": "Control smart home devices by specifying device and action.",
        "parameters": {
            "type": "object",
            "properties": {
                "device": {
                    "type": "string",
                    "enum": ["light", "ac", "fan"],
                    "description": "The device to control (light, ac, fan)."
                },
                "action": {
                    "type": "string",
                    "enum": ["on", "off"],
                    "description": "The action to perform on the device (on, off)."
                }
            },
            "required": ["device", "action"]
        }
    })
}

pub fn execute(raw_args: Value) -> Result<String, String> {
    let args: SmartHomeArgs = serde_json::from_value(raw_args)
        .map_err(|e| format!("Validation error: {}", e))?;

    let device_str = match args.device {
        SmartHomeDevice::Light => "light",
        SmartHomeDevice::Ac => "ac",
        SmartHomeDevice::Fan => "fan",
    };
    let action_str = match args.action {
        SmartHomeAction::On => "on",
        SmartHomeAction::Off => "off",
    };

    tracing::info!("[SmartHomeSkill] Executing: device='{}', action='{}'", device_str, action_str);
    Ok(format!("Device '{}' successfully turned '{}'.", device_str, action_str))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metadata() {
        let meta = get_metadata();
        assert_eq!(meta["name"], "smart_home_control");
        assert_eq!(meta["category"], "core");
    }

    #[test]
    fn test_execute_success() {
        let payload = json!({ "device": "light", "action": "on" });
        let res = execute(payload).unwrap();
        assert_eq!(res, "Device 'light' successfully turned 'on'.");

        let payload_ac = json!({ "device": "ac", "action": "off" });
        let res_ac = execute(payload_ac).unwrap();
        assert_eq!(res_ac, "Device 'ac' successfully turned 'off'.");
    }

    #[test]
    fn test_execute_invalid_device() {
        let payload = json!({ "device": "tv", "action": "on" });
        let res = execute(payload);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("Validation error"));
    }

    #[test]
    fn test_execute_strict_mode() {
        let payload = json!({ "device": "light", "action": "on", "extra": 123 });
        let res = execute(payload);
        assert!(res.is_err());
        assert!(res.unwrap_err().contains("unknown field `extra`"));
    }
}
