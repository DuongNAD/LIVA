use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

// `JsonSchema` thêm 26/07/2026 (G1) để schema của MCP tool `control_smarthome`
// mang được TỪ VỰNG hợp lệ, không chỉ "là một chuỗi". Đo trên gemma-4-E4B: khi
// schema chỉ nói `device: string`, model sinh `"air conditioner"` và `"turn on"`
// — hợp lý với thông tin nó có, sai với thứ `execute` nhận.
//
// Chú ý dùng `//` chứ KHÔNG `///`: schemars nhét doc comment vào schema thành
// `description`, nên một đoạn giải thích dài sẽ đi thẳng ra `mcp:list_tools` và
// phình prompt của mọi caller.
#[derive(Debug, Deserialize, Serialize, PartialEq, schemars::JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SmartHomeDevice {
    Light,
    Ac,
    Fan,
}

#[derive(Debug, Deserialize, Serialize, PartialEq, schemars::JsonSchema)]
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
    let args: SmartHomeArgs =
        serde_json::from_value(raw_args).map_err(|e| format!("Validation error: {}", e))?;

    let device_str = match args.device {
        SmartHomeDevice::Light => "light",
        SmartHomeDevice::Ac => "ac",
        SmartHomeDevice::Fan => "fan",
    };
    let action_str = match args.action {
        SmartHomeAction::On => "on",
        SmartHomeAction::Off => "off",
    };

    // TRUNG THỰC (nguyên tắc "không bịa"): skill này CHƯA có I/O phần cứng thật.
    // Trước đây trả "successfully turned" vô điều kiện — sau khi router 2.8 khớp
    // cả tiếng Việt ("bật đèn"), người dùng nhận báo thành công GIẢ dù không có
    // gì xảy ra. Báo đúng trạng thái; nối phần cứng thật (Home Assistant/MQTT…)
    // vào ĐÚNG chỗ này khi có tích hợp.
    tracing::info!(
        "[SmartHomeSkill] Nhận lệnh device='{}', action='{}' — CHƯA có tích hợp thiết bị thật",
        device_str,
        action_str
    );
    Ok(format!(
        "Chưa điều khiển được thiết bị thật: LIVA đã hiểu lệnh '{}' cho '{}', nhưng hiện CHƯA \
         kết nối tích hợp nhà thông minh nào nên không có thiết bị nào được thay đổi.",
        action_str, device_str
    ))
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
    fn test_execute_bao_trung_thuc_khong_thanh_cong_gia() {
        // Skill chưa có phần cứng → PHẢI báo trung thực, KHÔNG claim thành công.
        let res = execute(json!({ "device": "light", "action": "on" })).unwrap();
        assert!(
            res.contains("CHƯA") && res.contains("light") && res.contains("on"),
            "phải nêu rõ chưa kết nối + đúng device/action"
        );
        assert!(
            !res.to_lowercase().contains("successfully turned") && !res.contains("thành công"),
            "KHÔNG được báo thành công giả (vi phạm nguyên tắc không bịa)"
        );

        let res_ac = execute(json!({ "device": "ac", "action": "off" })).unwrap();
        assert!(res_ac.contains("ac") && res_ac.contains("off") && res_ac.contains("CHƯA"));
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
