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
    "toggle_skill",
    "toggle_all_skills",
    "get_user_profile",
    "get_avatar_models",
    "import_avatar_folder",
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
        "get_skills_list" => get_skills_list(&state),
        "toggle_skill" => toggle_skill(state, payload).await,
        "toggle_all_skills" => toggle_all_skills(state, payload).await,
        "get_user_profile" => get_user_profile(),
        "get_avatar_models" => Ok(json!({
            "models2d": liet_ke_thu_muc(&resolve_resource_path("models/live2d")),
            "models3d": liet_ke_thu_muc(&resolve_resource_path("models/vrm")),
        })),
        "import_avatar_folder" => import_avatar_folder(&payload),
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

/// Đọc danh sách kỹ năng bị vô hiệu hoá từ file cấu hình.
/// Nếu không đọc được file hoặc JSON sai cú pháp, rơi về tập rỗng (không tắt kỹ năng nào)
/// và ghi log `tracing::warn!`.
pub fn load_disabled_skills() -> std::collections::HashSet<String> {
    load_disabled_skills_from(&config_file_path())
}

pub fn load_disabled_skills_from(path: &std::path::Path) -> std::collections::HashSet<String> {
    if !path.exists() {
        return std::collections::HashSet::new();
    }
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!("Failed to read config file for disabled skills: {e}");
            return std::collections::HashSet::new();
        }
    };
    let val: Value = match serde_json::from_str(&content) {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("Failed to parse config file for disabled skills: {e}");
            return std::collections::HashSet::new();
        }
    };
    if let Some(arr) = val
        .get("skills")
        .and_then(|s| s.get("disabled"))
        .and_then(Value::as_array)
    {
        arr.iter()
            .filter_map(Value::as_str)
            .map(str::to_string)
            .collect()
    } else {
        std::collections::HashSet::new()
    }
}

pub fn get_skills_list(state: &AppState) -> Result<Value, String> {
    get_skills_list_from(state, &config_file_path())
}

pub fn get_skills_list_from(
    state: &AppState,
    config_path: &std::path::Path,
) -> Result<Value, String> {
    let mut skills = state.mcp_server.list_skills();
    let disabled = load_disabled_skills_from(config_path);
    for skill in &mut skills {
        if let Some(name) = skill.get("name").and_then(Value::as_str) {
            let is_enabled = !disabled.contains(name);
            if let Some(obj) = skill.as_object_mut() {
                obj.insert("enabled".to_string(), json!(is_enabled));
            }
        }
    }
    Ok(json!(skills))
}

pub fn toggle_skill_at(
    state: &AppState,
    payload: &Value,
    config_path: &std::path::Path,
) -> Result<Value, String> {
    let name = payload
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing or invalid 'name' in payload".to_string())?;
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| "Missing or invalid 'enabled' in payload".to_string())?;

    let tools = state.mcp_server.list_tools();
    if !tools.tools.iter().any(|t| t.name == name) {
        return Err(format!("Unknown skill or tool: {name}"));
    }

    let mut disabled = load_disabled_skills_from(config_path);
    if enabled {
        disabled.remove(name);
    } else {
        disabled.insert(name.to_string());
    }

    let mut disabled_vec: Vec<String> = disabled.into_iter().collect();
    disabled_vec.sort();

    let patch = json!({
        "skills": {
            "disabled": disabled_vec
        }
    });

    update_config_file_at(config_path, &patch)?;

    Ok(json!({ "success": true }))
}

async fn toggle_skill(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let path = config_file_path();
    tokio::task::spawn_blocking(move || toggle_skill_at(&state, &payload, &path))
        .await
        .map_err(|error| format!("Config writer task failed: {error}"))?
}

pub fn toggle_all_skills_at(
    state: &AppState,
    payload: &Value,
    config_path: &std::path::Path,
) -> Result<Value, String> {
    let enabled = payload
        .get("enabled")
        .and_then(Value::as_bool)
        .ok_or_else(|| "Missing or invalid 'enabled' in payload".to_string())?;

    let disabled_vec: Vec<String> = if enabled {
        Vec::new()
    } else {
        let mut all: Vec<String> = state
            .mcp_server
            .list_tools()
            .tools
            .into_iter()
            .map(|t| t.name)
            .collect();
        all.sort();
        all
    };

    let patch = json!({
        "skills": {
            "disabled": disabled_vec
        }
    });

    update_config_file_at(config_path, &patch)?;

    Ok(json!({ "success": true }))
}

async fn toggle_all_skills(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let path = config_file_path();
    tokio::task::spawn_blocking(move || toggle_all_skills_at(&state, &payload, &path))
        .await
        .map_err(|error| format!("Config writer task failed: {error}"))?
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

pub fn import_avatar_folder(payload: &Value) -> Result<Value, String> {
    let dest_dir = resolve_resource_path("models/vrm");
    import_avatar_folder_to(payload, &dest_dir)
}

fn import_avatar_folder_to(payload: &Value, dest_dir: &std::path::Path) -> Result<Value, String> {
    let folder_path_str = payload
        .get("folderPath")
        .and_then(Value::as_str)
        .ok_or_else(|| "Missing or invalid 'folderPath' field in payload".to_string())?;

    if folder_path_str.trim().is_empty() {
        return Err("Field 'folderPath' cannot be empty".to_string());
    }

    let src_path = std::path::Path::new(folder_path_str);
    let canonical_src = src_path.canonicalize().map_err(|e| {
        format!(
            "Folder '{}' does not exist or is inaccessible: {}",
            folder_path_str, e
        )
    })?;

    if !canonical_src.is_dir() {
        return Err(format!("Path '{}' is not a directory", folder_path_str));
    }

    if !dest_dir.exists() {
        std::fs::create_dir_all(dest_dir).map_err(|e| {
            format!(
                "Failed to create destination directory '{}': {}",
                dest_dir.display(),
                e
            )
        })?;
    }

    let entries = std::fs::read_dir(&canonical_src).map_err(|e| {
        format!(
            "Failed to read directory '{}': {}",
            canonical_src.display(),
            e
        )
    })?;

    let mut imported: Vec<String> = Vec::new();
    let mut skipped: Vec<Value> = Vec::new();
    const MAX_FILES: usize = 200;
    let mut stopped_early = false;

    for (inspected_count, entry_res) in entries.enumerate() {
        if inspected_count >= MAX_FILES {
            stopped_early = true;
            break;
        }

        let entry = match entry_res {
            Ok(e) => e,
            Err(e) => {
                skipped.push(json!({
                    "name": "<unknown>",
                    "reason": format!("Failed to read entry: {}", e)
                }));
                continue;
            }
        };

        let file_name = entry.file_name();
        let name_str = file_name.to_string_lossy().to_string();

        let file_type = match entry.file_type() {
            Ok(ft) => ft,
            Err(e) => {
                skipped.push(json!({
                    "name": name_str,
                    "reason": format!("Failed to get file type: {}", e)
                }));
                continue;
            }
        };

        if file_type.is_dir() {
            skipped.push(json!({
                "name": name_str,
                "reason": "Subdirectories are skipped"
            }));
            continue;
        }

        if !file_type.is_file() {
            skipped.push(json!({
                "name": name_str,
                "reason": "Not a regular file"
            }));
            continue;
        }

        let entry_path = entry.path();
        let ext = entry_path
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        if !ext.eq_ignore_ascii_case("vrm") {
            let reason = if ext.eq_ignore_ascii_case("fbx") {
                "FBX format is currently not supported for 3D avatars; only .vrm files are imported"
            } else {
                "Unsupported file extension (only .vrm files are imported)"
            };
            skipped.push(json!({
                "name": name_str,
                "reason": reason
            }));
            continue;
        }

        let dest_file_path = dest_dir.join(&file_name);

        if dest_file_path.exists() {
            skipped.push(json!({
                "name": name_str,
                "reason": "File already exists in destination"
            }));
            continue;
        }

        match std::fs::copy(&entry_path, &dest_file_path) {
            Ok(_) => {
                imported.push(name_str);
            }
            Err(e) => {
                skipped.push(json!({
                    "name": name_str,
                    "reason": format!("Failed to copy file: {}", e)
                }));
            }
        }
    }

    if stopped_early {
        skipped.push(json!({
            "name": "<limit_reached>",
            "reason": format!("Import stopped: reached maximum limit of {} files", MAX_FILES)
        }));
    }

    Ok(json!({
        "imported": imported,
        "skipped": skipped,
        "destination": dest_dir.display().to_string(),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    /// Chỉ còn dùng để KHẲNG ĐỊNH mặc định mới không phải là nó nữa.
    use crate::DEFAULT_MODELS_DIR;

    struct TempDirGuard(std::path::PathBuf);
    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn tao_thu_muc_tam(prefix: &str) -> (TempDirGuard, std::path::PathBuf) {
        let unique = format!(
            "liva_test_{}_{}_{}",
            prefix,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        (TempDirGuard(path.clone()), path)
    }

    /// `owns()` và `handle()` phải khai CÙNG một tập lệnh. Lệch nhau thì lệnh
    /// rơi vào nhánh `_` của `handle` và trả "Unknown command" cho một lệnh có
    /// thật — đúng kiểu lỗi im lặng mà việc tách miền này sinh ra để tránh.
    #[test]
    fn owns_va_handle_khong_lech_nhau() {
        // `handle` không liệt kê được bằng phản chiếu, nên khoá bằng số lượng:
        // thêm nhánh vào `handle` mà quên `OWNED` sẽ làm test này đỏ.
        assert_eq!(
            OWNED.len(),
            16,
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
        assert!(owns("import_avatar_folder"));
        assert!(owns("toggle_skill"));
        assert!(owns("toggle_all_skills"));
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

    #[test]
    fn import_avatar_folder_thanh_cong_vrm() {
        let (_src_guard, src_dir) = tao_thu_muc_tam("import_vrm_src");
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_vrm_dst");

        let file1 = src_dir.join("avatar1.vrm");
        let file2 = src_dir.join("avatar2.VRM");
        std::fs::write(&file1, b"VRM_DATA_1").unwrap();
        std::fs::write(&file2, b"VRM_DATA_2").unwrap();

        let payload = json!({
            "folderPath": src_dir.to_str().unwrap()
        });

        let res = import_avatar_folder_to(&payload, &dst_dir).unwrap();
        let imported = res["imported"].as_array().unwrap();
        assert_eq!(imported.len(), 2);
        assert!(imported.iter().any(|v| v == "avatar1.vrm"));
        assert!(imported.iter().any(|v| v == "avatar2.VRM"));

        assert_eq!(
            std::fs::read(dst_dir.join("avatar1.vrm")).unwrap(),
            b"VRM_DATA_1"
        );
        assert_eq!(
            std::fs::read(dst_dir.join("avatar2.VRM")).unwrap(),
            b"VRM_DATA_2"
        );
    }

    #[test]
    fn import_avatar_folder_bo_qua_fbx_va_txt() {
        let (_src_guard, src_dir) = tao_thu_muc_tam("import_fbx_src");
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_fbx_dst");

        std::fs::write(src_dir.join("model.fbx"), b"FBX").unwrap();
        std::fs::write(src_dir.join("readme.txt"), b"TXT").unwrap();
        std::fs::create_dir(src_dir.join("subfolder")).unwrap();

        let payload = json!({
            "folderPath": src_dir.to_str().unwrap()
        });

        let res = import_avatar_folder_to(&payload, &dst_dir).unwrap();
        let imported = res["imported"].as_array().unwrap();
        let skipped = res["skipped"].as_array().unwrap();

        assert!(imported.is_empty());
        assert_eq!(skipped.len(), 3);

        let skipped_names: Vec<&str> = skipped
            .iter()
            .map(|s| s["name"].as_str().unwrap())
            .collect();
        assert!(skipped_names.contains(&"model.fbx"));
        assert!(skipped_names.contains(&"readme.txt"));
        assert!(skipped_names.contains(&"subfolder"));

        let fbx_skipped = skipped.iter().find(|s| s["name"] == "model.fbx").unwrap();
        assert!(
            fbx_skipped["reason"]
                .as_str()
                .unwrap()
                .contains("FBX format is currently not supported")
        );

        assert!(!dst_dir.join("model.fbx").exists());
        assert!(!dst_dir.join("readme.txt").exists());
        assert!(!dst_dir.join("subfolder").exists());
    }

    #[test]
    fn import_avatar_folder_duong_dan_khong_hop_le_tra_ve_err() {
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_err_dst");

        let non_existent = std::env::temp_dir().join("khong_ton_tai_liva_123456789");
        let err = import_avatar_folder_to(
            &json!({ "folderPath": non_existent.to_str().unwrap() }),
            &dst_dir,
        )
        .unwrap_err();
        assert!(
            err.contains("folderPath")
                || err.contains("does not exist")
                || err.contains("inaccessible")
        );

        let err_missing = import_avatar_folder_to(&json!({}), &dst_dir).unwrap_err();
        assert!(err_missing.contains("folderPath"));

        let err_empty =
            import_avatar_folder_to(&json!({ "folderPath": "   " }), &dst_dir).unwrap_err();
        assert!(err_empty.contains("folderPath"));

        let (_file_guard, file_dir) = tao_thu_muc_tam("import_file_src");
        let a_file = file_dir.join("file.txt");
        std::fs::write(&a_file, b"content").unwrap();
        let err_file =
            import_avatar_folder_to(&json!({ "folderPath": a_file.to_str().unwrap() }), &dst_dir)
                .unwrap_err();
        assert!(err_file.contains("not a directory"));
    }

    #[test]
    fn import_avatar_folder_khong_cho_thoat_thu_muc_dich() {
        let (_src_guard, src_dir) = tao_thu_muc_tam("import_escape_src");
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_escape_dst");

        std::fs::write(src_dir.join("safe.vrm"), b"SAFE").unwrap();

        let payload = json!({
            "folderPath": src_dir.to_str().unwrap()
        });

        let res = import_avatar_folder_to(&payload, &dst_dir).unwrap();
        let imported = res["imported"].as_array().unwrap();
        assert_eq!(imported.len(), 1);

        let dest_file = dst_dir.join("safe.vrm");
        assert!(dest_file.exists());
        assert_eq!(dest_file.parent(), Some(dst_dir.as_path()));
    }

    #[test]
    fn import_avatar_folder_khong_ghi_de_file_da_co() {
        let (_src_guard, src_dir) = tao_thu_muc_tam("import_no_overwrite_src");
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_no_overwrite_dst");

        std::fs::write(dst_dir.join("existing.vrm"), b"ORIGINAL_DATA").unwrap();
        std::fs::write(src_dir.join("existing.vrm"), b"NEW_DATA").unwrap();

        let payload = json!({
            "folderPath": src_dir.to_str().unwrap()
        });

        let res = import_avatar_folder_to(&payload, &dst_dir).unwrap();
        let imported = res["imported"].as_array().unwrap();
        let skipped = res["skipped"].as_array().unwrap();

        assert!(imported.is_empty());
        assert_eq!(skipped.len(), 1);
        assert_eq!(skipped[0]["name"], "existing.vrm");
        assert!(
            skipped[0]["reason"]
                .as_str()
                .unwrap()
                .contains("already exists")
        );

        assert_eq!(
            std::fs::read(dst_dir.join("existing.vrm")).unwrap(),
            b"ORIGINAL_DATA"
        );
    }

    #[test]
    fn import_avatar_folder_gioi_han_200_files() {
        let (_src_guard, src_dir) = tao_thu_muc_tam("import_limit_src");
        let (_dst_guard, dst_dir) = tao_thu_muc_tam("import_limit_dst");

        for i in 0..205 {
            std::fs::write(src_dir.join(format!("avatar_{i:03}.vrm")), b"VRM").unwrap();
        }

        let payload = json!({
            "folderPath": src_dir.to_str().unwrap()
        });

        let res = import_avatar_folder_to(&payload, &dst_dir).unwrap();
        let imported = res["imported"].as_array().unwrap();
        let skipped = res["skipped"].as_array().unwrap();

        assert_eq!(imported.len(), 200);
        let limit_item = skipped.iter().find(|s| s["name"] == "<limit_reached>");
        assert!(limit_item.is_some());
    }

    fn test_state() -> Arc<AppState> {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let stt = tokio::sync::Mutex::new(crate::stt::SttManager::new("non-existent-model"));
        let llm = tokio::sync::Mutex::new(
            crate::llm::LlamaRouterManager::new(2048, 0).expect("LLM manager"),
        );
        let mock_capturer = Arc::new(crate::vision::capture::MockScreenCapturer::new(
            64,
            64,
            crate::vision::capture::PixelFormat::Rgba,
        ));
        Arc::new(AppState {
            db,
            crypto: crate::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt,
            tts: tokio::sync::Mutex::new(None),
            tts_player: crate::tts::audio::TtsAudioPlayer::new(None),
            llm,
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                mock_capturer,
                crate::vision::VisionConfig::default(),
            )),
            embedder: tokio::sync::Mutex::new(None),
        })
    }

    #[test]
    fn toggle_skill_va_get_skills_list_chinh_xac() {
        let (_guard, temp_dir) = tao_thu_muc_tam("toggle_skill");
        let config_file = temp_dir.join("liva-config.json");
        let state = test_state();

        // Ban đầu: không có file config → mọi skill đều enabled = true
        let list = get_skills_list_from(&state, &config_file).unwrap();
        let arr = list.as_array().unwrap();
        assert!(!arr.is_empty());
        for skill in arr {
            assert_eq!(skill["enabled"], true);
        }

        // Tắt 'control_volume'
        let toggle_off = toggle_skill_at(
            &state,
            &json!({ "name": "control_volume", "enabled": false }),
            &config_file,
        )
        .unwrap();
        assert_eq!(toggle_off["success"], true);

        // Kiểm tra get_skills_list: control_volume false, các tool khác true
        let list = get_skills_list_from(&state, &config_file).unwrap();
        let arr = list.as_array().unwrap();
        let vol = arr
            .iter()
            .find(|s| s["name"] == "control_volume")
            .expect("control_volume exists");
        assert_eq!(vol["enabled"], false);

        let other = arr
            .iter()
            .find(|s| s["name"] == "read_markdown")
            .expect("read_markdown exists");
        assert_eq!(other["enabled"], true);

        // Bật lại 'control_volume'
        let toggle_on = toggle_skill_at(
            &state,
            &json!({ "name": "control_volume", "enabled": true }),
            &config_file,
        )
        .unwrap();
        assert_eq!(toggle_on["success"], true);

        let list = get_skills_list_from(&state, &config_file).unwrap();
        let arr = list.as_array().unwrap();
        let vol = arr
            .iter()
            .find(|s| s["name"] == "control_volume")
            .expect("control_volume exists");
        assert_eq!(vol["enabled"], true);
    }

    #[test]
    fn toggle_skill_ten_khong_hop_le_tra_ve_err() {
        let (_guard, temp_dir) = tao_thu_muc_tam("toggle_invalid");
        let config_file = temp_dir.join("liva-config.json");
        let state = test_state();

        let err = toggle_skill_at(
            &state,
            &json!({ "name": "unknown_tool_12345", "enabled": false }),
            &config_file,
        )
        .unwrap_err();

        assert!(
            err.contains("unknown_tool_12345"),
            "Error must name the unknown tool: {err}"
        );
    }

    #[test]
    fn toggle_all_skills_round_trip() {
        let (_guard, temp_dir) = tao_thu_muc_tam("toggle_all");
        let config_file = temp_dir.join("liva-config.json");
        let state = test_state();

        // Tắt tất cả
        let res_off =
            toggle_all_skills_at(&state, &json!({ "enabled": false }), &config_file).unwrap();
        assert_eq!(res_off["success"], true);

        let list = get_skills_list_from(&state, &config_file).unwrap();
        let arr = list.as_array().unwrap();
        assert!(!arr.is_empty());
        for skill in arr {
            assert_eq!(
                skill["enabled"], false,
                "tool {} should be disabled",
                skill["name"]
            );
        }

        // Bật lại tất cả
        let res_on =
            toggle_all_skills_at(&state, &json!({ "enabled": true }), &config_file).unwrap();
        assert_eq!(res_on["success"], true);

        let list = get_skills_list_from(&state, &config_file).unwrap();
        let arr = list.as_array().unwrap();
        assert!(!arr.is_empty());
        for skill in arr {
            assert_eq!(
                skill["enabled"], true,
                "tool {} should be enabled",
                skill["name"]
            );
        }
    }
}
