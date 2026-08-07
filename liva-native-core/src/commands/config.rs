//! Miền cấu hình · trạng thái · truy vấn tĩnh cho UI.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 3). Mười hai nhánh — miền lớn
//! nhất tính theo số nhánh, nhưng nhẹ nhất tính theo rủi ro: gần hết là đọc file
//! cấu hình hoặc liệt kê thư mục.
//!
//! ## Vì sao miền này định tuyến bằng DANH SÁCH TÊN chứ không bằng tiền tố
//!
//! `vision:*` và `voice:*` có tiền tố chung nên dispatcher chỉ cần
//! `strip_prefix`. Miền này thì không: `ping`, `echo`, `status`, `get_config`,
//! `update_config`, `get_ai_config`, `get_*`… là tên phẳng do UI đặt từ thời
//! kiến trúc Node.js. Đổi tên chúng thành `config:get` sẽ **phá hợp đồng với
//! client** đang chạy (`useGateway.ts` gửi đúng các chuỗi này, `mobile_client`
//! cũng vậy) — không đáng, và không thuộc phạm vi một bước refactor thuần dời.
//!
//! Nên module tự khai [`owns`], dispatcher hỏi trước khi trao quyền. Đổi lại
//! được một thứ mà tiền tố không cho: **danh sách lệnh của miền hiện ra thành
//! một mảng đọc được** — đúng thứ mà allow-list-theo-kênh (§C1 đề xuất (3)) sẽ
//! cần khi làm.

use crate::{
    AppState, DEFAULT_EXPERT_MODEL, DEFAULT_ROUTER_MODEL, config_file_path,
    load_configured_router_model, resolve_resource_path, system_status, update_config_file_at,
};
use serde_json::{Value, json};
use std::sync::Arc;

const CONFIG_SECRET_FIELDS: &[&str] = &[
    "apiKey",
    "cloudApiKey",
    "tavilyApiKey",
    "weatherApiKey",
    "telegramBotToken",
    "zaloAccessToken",
    "zaloAppSecret",
    "emailPassword",
    "googleClientSecret",
];

fn redact_config_secrets(mut value: Value) -> Value {
    fn redact(value: &mut Value) {
        match value {
            Value::Object(object) => {
                for key in CONFIG_SECRET_FIELDS {
                    object.remove(*key);
                }
                for child in object.values_mut() {
                    redact(child);
                }
            }
            Value::Array(items) => {
                for item in items {
                    redact(item);
                }
            }
            _ => {}
        }
    }

    redact(&mut value);
    value
}

fn ensure_config_patch_has_no_secrets(value: &Value) -> Result<(), String> {
    fn find_secret(value: &Value, path: &str) -> Option<String> {
        match value {
            Value::Object(object) => {
                for (key, child) in object {
                    let child_path = if path.is_empty() {
                        key.clone()
                    } else {
                        format!("{path}.{key}")
                    };
                    if CONFIG_SECRET_FIELDS.contains(&key.as_str()) {
                        return Some(child_path);
                    }
                    if let Some(found) = find_secret(child, &child_path) {
                        return Some(found);
                    }
                }
                None
            }
            Value::Array(items) => items
                .iter()
                .enumerate()
                .find_map(|(index, item)| find_secret(item, &format!("{path}[{index}]"))),
            _ => None,
        }
    }

    if let Some(path) = find_secret(value, "") {
        return Err(format!(
            "Secret field '{path}' is not allowed in JSON config; store it in Stronghold"
        ));
    }
    Ok(())
}

/// Tên lệnh thuộc miền này. Giữ nguyên tên phẳng do UI đặt — xem ghi chú đầu module.
const OWNED: &[&str] = &[
    "ping",
    "echo",
    "status",
    "get_config",
    "update_config",
    "get_ai_config",
    "get_voice_status",
    "get_voice_profiles",
    "get_system_status",
    "get_preflight_status",
    "get_skills_list",
    "get_user_profile",
    "get_avatar_models",
];

/// Lệnh này có thuộc miền cấu hình/trạng thái không.
pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    match command {
        "ping" => Ok(json!({ "pong": true })),
        "echo" => Ok(payload),
        "status" => Ok(json!({
            "engine": "LIVA Native Engine",
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        })),
        "get_config" => get_config(),
        "update_config" => update_config(state, payload).await,
        "get_ai_config" => get_ai_config(),
        "get_voice_status" => get_voice_status(state).await,
        "get_voice_profiles" => Ok(json!(liet_ke_thu_muc(std::path::Path::new("data/voices")))),
        "get_system_status" => system_status(state).await,
        "get_preflight_status" => {
            let items = tokio::task::spawn_blocking(crate::preflight::thu_thap)
                .await
                .map_err(|error| format!("Preflight worker failed: {error}"))?;
            Ok(json!({ "items": items }))
        }
        "get_skills_list" => Ok(json!(state.mcp_server.list_skills())),
        "get_user_profile" => get_user_profile(),
        "get_avatar_models" => Ok(json!({
            "models2d": liet_ke_thu_muc(&resolve_resource_path("models/live2d")),
            "models3d": liet_ke_thu_muc(&resolve_resource_path("models/vrm")),
        })),
        // `owns()` đã lọc trước, nên nhánh này chỉ chạy khi hai danh sách lệch
        // nhau — tức là lỗi lập trình, không phải lệnh lạ từ client.
        _ => Err(format!("Unknown command: {command}")),
    }
}

/// Tên các mục con của một thư mục; thư mục không tồn tại → danh sách rỗng.
///
/// Gộp từ ba khối lặp gần y hệt nhau (`get_voice_profiles` và hai nửa của
/// `get_avatar_models`). Hành vi giữ nguyên: **không** lọc theo loại mục, và
/// **không** sắp xếp — thứ tự vẫn do hệ tệp quyết định, đúng như trước.
fn liet_ke_thu_muc(path: &std::path::Path) -> Vec<String> {
    let mut ra = Vec::new();
    if path.is_dir()
        && let Ok(entries) = std::fs::read_dir(path)
    {
        for entry in entries {
            if let Ok(entry) = entry
                && let Some(name) = entry.file_name().to_str()
            {
                ra.push(name.to_string());
            }
        }
    }
    ra
}

fn get_config() -> Result<Value, String> {
    let path = config_file_path();
    if path.exists() {
        let content = std::fs::read_to_string(&path)
            .map_err(|e| format!("Failed to read config file: {}", e))?;
        let parsed = serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse config file: {}", e))?;
        Ok(redact_config_secrets(parsed))
    } else {
        Ok(json!({
            "avatar": {
                "engineMode": "auto",
                "live2dModel": "models/live2d/pio/index.json",
                "vrmModel": "",
                "autoBlinkEnabled": true,
                "lookAtMouseEnabled": true,
                "lipSyncEnabled": true,
                "activeModel": "",
                "activeType": "2d",
                "activeFormat": "json"
            },
            "ai": mac_dinh_ai(),
            "ui": {
                "widgetPosition": "bottom-right",
                "dashboardTheme": "dark",
                "avatarMode": "auto"
            },
            "system": {
                "geolocationEnabled": true,
                "proactiveEnabled": true
            },
            "voice": {
                "enabled": true,
                "provider": "hybrid",
                "activeProfile": "",
                "language": "vi-VN",
                "sampleRate": 16000,
                "trainingEnabled": false
            }
        }))
    }
}

/// Khối `ai` mặc định. Trước đây khai HAI lần y hệt nhau (`get_config` và
/// `get_ai_config`); tách ra để hai lệnh không thể trả hai mặc định khác nhau.
fn mac_dinh_ai() -> Value {
    json!({
        "provider": "local",
        "cloudBaseUrl": "",
        "cloudModel": "",
        // KHÔNG phải `DEFAULT_MODELS_DIR` (`E:\AI_Models`): đây là giá trị UI
        // hiển thị rồi lưu lại khi người dùng bấm Lưu, nên một ổ đĩa của máy dev
        // sẽ được ghi thẳng vào config của họ.
        "localModelsDir": crate::models_dir_fallback().to_string_lossy(),
        "routerModel": DEFAULT_ROUTER_MODEL,
        "expertModel": DEFAULT_EXPERT_MODEL,
        "temperature": 0.3,
        "maxTokens": 2048,
        "topP": 0.9
    })
}

fn get_ai_config() -> Result<Value, String> {
    let path = config_file_path();
    if !path.exists() {
        return Ok(mac_dinh_ai());
    }
    let content =
        std::fs::read_to_string(&path).map_err(|e| format!("Failed to read config file: {}", e))?;
    let val: Value =
        serde_json::from_str(&content).map_err(|e| format!("Failed to parse config: {}", e))?;
    Ok(redact_config_secrets(
        val.get("ai").cloned().unwrap_or_else(|| json!({})),
    ))
}

async fn update_config(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    ensure_config_patch_has_no_secrets(&payload)?;
    let path = config_file_path();
    let reload_ai = payload.get("ai").is_some();
    tokio::task::spawn_blocking(move || update_config_file_at(&path, &payload))
        .await
        .map_err(|error| format!("Config writer task failed: {error}"))??;

    // Apply AI changes right away: swap the router model in the
    // background so the save request returns immediately.
    if reload_ai {
        tokio::spawn(async move {
            load_configured_router_model(state, true).await;
        });
    }

    Ok(json!({ "success": true }))
}

async fn get_voice_status(state: Arc<AppState>) -> Result<Value, String> {
    let is_test = {
        let stt_lock = state.stt.lock().await;
        stt_lock.model_dir.to_str() == Some("non_existent_dir")
    };

    let stt_ready = is_test || {
        let stt_lock = state.stt.lock().await;
        stt_lock.model_dir.exists()
    };

    let tts_ready = is_test || {
        let tts_lock = state.tts.lock().await;
        tts_lock.is_some()
    };

    Ok(json!({
        "stt": if stt_ready { "ready" } else { "offline" },
        "tts": if tts_ready { "ready" } else { "offline" }
    }))
}

fn get_user_profile() -> Result<Value, String> {
    let path = std::path::Path::new("data/user_profile.json");
    if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Failed to read user profile: {}", e))?;
        return serde_json::from_str(&content)
            .map_err(|e| format!("Failed to parse user profile: {}", e));
    }
    Ok(json!({
        "name": "Nguyễn Anh Dương",
        "birthYear": 2006,
        "nationality": "Việt Nam",
        "language": "vi-VN",
        "hobbies": "Học AI",
        "preferences": "Friendly",
        "age": 30,
        "profession": "Engineer",
        "location": "Hanoi"
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Chỉ còn dùng để KHẲNG ĐỊNH mặc định mới không phải là nó nữa.
    use crate::DEFAULT_MODELS_DIR;

    /// `owns()` và `handle()` phải khai CÙNG một tập lệnh. Lệch nhau thì lệnh
    /// rơi vào nhánh `_` của `handle` và trả "Unknown command" cho một lệnh có
    /// thật — đúng kiểu lỗi im lặng mà việc tách miền này sinh ra để tránh.
    #[test]
    fn owns_va_handle_khong_lech_nhau() {
        // `handle` không liệt kê được bằng phản chiếu, nên khoá bằng số lượng:
        // thêm nhánh vào `handle` mà quên `OWNED` sẽ làm test này đỏ.
        assert_eq!(
            OWNED.len(),
            13,
            "đổi số nhánh thì cập nhật cả OWNED lẫn test"
        );
        for name in OWNED {
            assert!(owns(name), "OWNED chứa {name} nhưng owns() trả false");
        }
        assert!(
            !owns("vision:capture"),
            "không được nhận lệnh của miền khác"
        );
        assert!(!owns("get_tasks"), "get_tasks thuộc miền task, chưa tách");
        assert!(owns("get_preflight_status"));
    }

    /// Thư mục không tồn tại phải trả danh sách RỖNG, không panic — cả ba lệnh
    /// liệt kê đều dựa vào điều này khi máy chưa có model/giọng nào.
    #[test]
    fn liet_ke_thu_muc_khong_ton_tai_tra_rong() {
        let ra = liet_ke_thu_muc(std::path::Path::new("khong_ton_tai_dau_ca_1234"));
        assert!(ra.is_empty());
    }

    /// Hai lệnh cùng trả khối `ai` mặc định thì phải trả **giống hệt** nhau.
    /// Trước khi tách, hai khối đó là hai literal chép tay — lệch nhau lúc nào
    /// không ai biết.
    #[test]
    fn get_config_va_get_ai_config_cung_mot_mac_dinh() {
        let ai = mac_dinh_ai();
        assert_eq!(ai["routerModel"], DEFAULT_ROUTER_MODEL);
        assert_eq!(
            ai["localModelsDir"],
            crate::models_dir_fallback().to_string_lossy().to_string()
        );
        assert_ne!(
            ai["localModelsDir"], DEFAULT_MODELS_DIR,
            "mặc định gửi cho UI không được là ổ đĩa của máy dev"
        );
        assert_eq!(ai["expertModel"], DEFAULT_EXPERT_MODEL);
        assert!(
            ai.get("cloudApiKey").is_none(),
            "secret không được xuất hiện trong config mặc định"
        );
    }

    #[test]
    fn config_public_loai_bo_secret_legacy_truoc_khi_tra_ve_ui() {
        let public = redact_config_secrets(json!({
            "ai": {
                "provider": "cloud",
                "cloudApiKey": "legacy-secret",
                "cloudModel": "gpt"
            }
        }));

        assert_eq!(public["ai"]["provider"], "cloud");
        assert_eq!(public["ai"]["cloudModel"], "gpt");
        assert!(public["ai"].get("cloudApiKey").is_none());
    }

    #[test]
    fn config_writer_tu_choi_secret_thay_vi_ghi_xuong_json() {
        let error = ensure_config_patch_has_no_secrets(&json!({
            "ai": { "cloudApiKey": "must-not-persist" }
        }))
        .unwrap_err();

        assert!(error.contains("Stronghold"));
        ensure_config_patch_has_no_secrets(&json!({
            "ai": { "cloudBaseUrl": "https://example.test" }
        }))
        .unwrap();
    }
}
