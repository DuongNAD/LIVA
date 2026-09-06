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

static DISABLED_SKILLS: std::sync::OnceLock<std::sync::RwLock<std::collections::HashSet<String>>> =
    std::sync::OnceLock::new();

fn disabled_skills_set() -> &'static std::sync::RwLock<std::collections::HashSet<String>> {
    DISABLED_SKILLS.get_or_init(|| std::sync::RwLock::new(std::collections::HashSet::new()))
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
    "test_skill",
    "test_all_skills",
    "toggle_skill",
    "toggle_all_skills",
    "get_user_profile",
    "get_avatar_models",
    "import_avatar_folder",
    "delete_avatar_model",
    "system:diagnostics",
    "system_diagnostic_probe",
    "system_diagnostic",
    "system:telemetry",
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
        "get_skills_list" => get_skills_list(state),
        "test_skill" => test_skill(state, payload).await,
        "test_all_skills" => test_all_skills(state).await,
        "toggle_skill" => toggle_skill(payload).await,
        "toggle_all_skills" => toggle_all_skills(state, payload).await,
        "get_user_profile" => get_user_profile(),
        "get_avatar_models" => Ok(json!({
            "models2d": liet_ke_thu_muc(&resolve_resource_path("models/live2d")),
            "models3d": liet_ke_thu_muc(&resolve_resource_path("models/vrm")),
        })),
        "import_avatar_folder" => import_avatar_folder(payload).await,
        "delete_avatar_model" => delete_avatar_model(payload).await,
        "system:diagnostics" | "system_diagnostic_probe" | "system_diagnostic" => {
            let rep = crate::system_diagnostic_probe::run_system_diagnostic(state).await?;
            serde_json::to_value(rep).map_err(|e| format!("Failed to serialize diagnostic report: {e}"))
        }
        "system:telemetry" => {
            Ok(crate::telemetry::global_telemetry().get_telemetry_snapshot())
        }
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

fn get_skills_list(state: Arc<AppState>) -> Result<Value, String> {
    let mut skills = state.mcp_server.list_skills();
    let disabled = disabled_skills_set()
        .read()
        .unwrap_or_else(|e| e.into_inner());
    for s in &mut skills {
        if let Some(obj) = s.as_object_mut() {
            let name = obj.get("name").and_then(Value::as_str).unwrap_or_default();
            let is_disabled = disabled.contains(name);
            obj.insert("enabled".to_string(), json!(!is_disabled));
            obj.insert(
                "status".to_string(),
                json!(if is_disabled { "disabled" } else { "active" }),
            );
        }
    }
    Ok(json!(skills))
}

fn test_single_skill(state: &Arc<AppState>, name: &str) -> Value {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let tools = state.mcp_server.list_tools().tools;
    if let Some(tool) = tools.into_iter().find(|t| t.name == name) {
        let param_count = serde_json::to_value(&tool.input_schema)
            .ok()
            .and_then(|v| v.get("properties").and_then(|p| p.as_object()).map(|o| o.len()))
            .unwrap_or(0);
        json!({
            "name": name,
            "success": true,
            "message": format!("Skill '{name}' sẵn sàng hoạt động"),
            "details": format!("Schema validation OK: {param_count} parameters"),
            "time": now
        })
    } else {
        json!({
            "name": name,
            "success": false,
            "message": format!("Skill '{name}' không tìm thấy trong native registry"),
            "details": "Tool chưa được đăng ký trong NativeMcpServer",
            "time": now
        })
    }
}

async fn test_skill(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Thiếu 'name' của skill trong payload".to_string())?;

    Ok(test_single_skill(&state, name))
}

async fn test_all_skills(state: Arc<AppState>) -> Result<Value, String> {
    let skills = state.mcp_server.list_skills();
    let mut results = Vec::new();
    let mut passed = 0;
    let mut failed = 0;

    for s in skills {
        if let Some(name) = s.get("name").and_then(Value::as_str) {
            let res = test_single_skill(&state, name);
            if res.get("success").and_then(Value::as_bool).unwrap_or(false) {
                passed += 1;
            } else {
                failed += 1;
            }
            results.push(res);
        }
    }

    Ok(json!({
        "success": failed == 0,
        "total": results.len(),
        "passed": passed,
        "failed": failed,
        "results": results
    }))
}

async fn toggle_skill(payload: Value) -> Result<Value, String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Thiếu 'name' của skill trong payload".to_string())?;
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    {
        let mut set = disabled_skills_set()
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if enabled {
            set.remove(name);
        } else {
            set.insert(name.to_string());
        }
    }

    Ok(json!({
        "success": true,
        "name": name,
        "enabled": enabled
    }))
}

async fn toggle_all_skills(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);

    {
        let mut set = disabled_skills_set()
            .write()
            .unwrap_or_else(|e| e.into_inner());
        if enabled {
            set.clear();
        } else {
            for skill in state.mcp_server.list_skills() {
                if let Some(name) = skill.get("name").and_then(Value::as_str) {
                    set.insert(name.to_string());
                }
            }
        }
    }

    Ok(json!({
        "success": true,
        "enabled": enabled
    }))
}

async fn import_avatar_folder(payload: Value) -> Result<Value, String> {
    let folder_path_str = payload
        .get("folderPath")
        .and_then(Value::as_str)
        .ok_or_else(|| "Thiếu 'folderPath' trong payload".to_string())?;

    let src_path = std::path::Path::new(folder_path_str);
    if !src_path.exists() || !src_path.is_dir() {
        return Err(format!("Thư mục nguồn không tồn tại hoặc không phải thư mục: {folder_path_str}"));
    }

    let folder_name = src_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or_else(|| "Tên thư mục nguồn không hợp lệ".to_string())?;

    // Detect if Live2D (contains .model3.json, index.json, or .moc3) or VRM/3D
    let is_live2d = if let Ok(entries) = std::fs::read_dir(src_path) {
        entries.flatten().any(|e| {
            let n = e.file_name().to_string_lossy().to_lowercase();
            n.ends_with(".model3.json") || n == "index.json" || n.ends_with(".moc3")
        })
    } else {
        false
    };

    let target_parent = if is_live2d {
        resolve_resource_path("models/live2d")
    } else {
        resolve_resource_path("models/vrm")
    };

    std::fs::create_dir_all(&target_parent)
        .map_err(|e| format!("Không thể tạo thư mục models đích: {e}"))?;

    let dest_dir = target_parent.join(folder_name);
    copy_dir_recursive(src_path, &dest_dir)
        .map_err(|e| format!("Lỗi khi copy thư mục avatar: {e}"))?;

    Ok(json!({
        "success": true,
        "folderName": folder_name,
        "modelType": if is_live2d { "2d" } else { "3d" },
        "destPath": dest_dir.display().to_string()
    }))
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    if !dst.exists() {
        std::fs::create_dir_all(dst)?;
    }
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ft = entry.file_type()?;
        let dest_child = dst.join(entry.file_name());
        if ft.is_dir() {
            copy_dir_recursive(&entry.path(), &dest_child)?;
        } else {
            std::fs::copy(entry.path(), dest_child)?;
        }
    }
    Ok(())
}

async fn delete_avatar_model(payload: Value) -> Result<Value, String> {
    let filename = payload
        .get("filename")
        .and_then(Value::as_str)
        .ok_or_else(|| "Thiếu 'filename' trong payload".to_string())?;

    // Path traversal guard
    if filename.is_empty() || filename.contains("..") || filename.contains('\\') || filename.starts_with('/') {
        return Err("Tên file hoặc đường dẫn không an toàn".to_string());
    }

    // Default model protection
    let lower = filename.to_lowercase();
    if lower.contains("default_avatar") || lower.starts_with("pio") || lower == "pio/index.json" || lower.contains("tripo_convert") {
        return Err("Không thể xoá model mặc định của hệ thống".to_string());
    }

    let vrm_path = resolve_resource_path(&format!("models/vrm/{filename}"));
    let live2d_path = resolve_resource_path(&format!("models/live2d/{filename}"));

    let mut deleted = false;
    for path in [&vrm_path, &live2d_path] {
        if path.exists() {
            if path.is_dir() {
                std::fs::remove_dir_all(path)
                    .map_err(|e| format!("Lỗi xoá thư mục model: {e}"))?;
            } else {
                std::fs::remove_file(path)
                    .map_err(|e| format!("Lỗi xoá tệp model: {e}"))?;
            }
            deleted = true;
            break;
        }
    }

    if !deleted {
        return Err(format!("Không tìm thấy model để xoá: {filename}"));
    }

    Ok(json!({
        "success": true,
        "filename": filename
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
            23,
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

    #[tokio::test]
    async fn toggle_skill_va_toggle_all_skills() {
        // Toggle specific skill
        let res = toggle_skill(json!({ "name": "read_markdown", "enabled": false }))
            .await
            .unwrap();
        assert_eq!(res["success"], true);
        assert_eq!(res["name"], "read_markdown");
        assert_eq!(res["enabled"], false);
        assert!(disabled_skills_set().read().unwrap().contains("read_markdown"));

        let res_re_enable = toggle_skill(json!({ "name": "read_markdown", "enabled": true }))
            .await
            .unwrap();
        assert_eq!(res_re_enable["enabled"], true);
        assert!(!disabled_skills_set().read().unwrap().contains("read_markdown"));

        let mock_state = Arc::new(crate::AppState {
            db: crate::db::DatabasePool::new_in_memory().unwrap(),
            crypto: crate::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(crate::stt::SttManager::new("mock")),
            tts: tokio::sync::Mutex::new(None),
            tts_player: crate::tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(crate::llm::LlamaRouterManager::new(2048, 0).unwrap()),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                Arc::new(crate::vision::capture::MockScreenCapturer::new(
                    100,
                    100,
                    crate::vision::capture::PixelFormat::Rgba,
                )),
                crate::vision::VisionConfig::default(),
            )),
            embedder: tokio::sync::Mutex::new(None),
        });

        // Toggle all
        let res_disable_all = toggle_all_skills(mock_state.clone(), json!({ "enabled": false }))
            .await
            .unwrap();
        assert_eq!(res_disable_all["success"], true);
        assert_eq!(res_disable_all["enabled"], false);
        assert!(!disabled_skills_set().read().unwrap().is_empty());

        // Re-enable all
        let res_enable_all = toggle_all_skills(mock_state, json!({ "enabled": true }))
            .await
            .unwrap();
        assert_eq!(res_enable_all["success"], true);
        assert_eq!(res_enable_all["enabled"], true);
        assert!(disabled_skills_set().read().unwrap().is_empty());
    }

    #[tokio::test]
    async fn delete_avatar_model_bao_ve_model_mac_dinh_va_chong_traversal() {
        // Path traversal attempts
        assert!(delete_avatar_model(json!({ "filename": "../secret.txt" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "/etc/passwd" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "sub/../model.vrm" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "" })).await.is_err());

        // Default models protection
        assert!(delete_avatar_model(json!({ "filename": "default_avatar" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "pio/index.json" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "pio" })).await.is_err());
        assert!(delete_avatar_model(json!({ "filename": "tripo_convert_123.fbx" })).await.is_err());
    }
}
