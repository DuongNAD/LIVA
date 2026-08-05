//! Đường dẫn ghi được, tài nguyên chỉ-đọc và cấu hình model.
//!
//! Mọi entry point (repo root, crate Rust, Tauri và bản cài) dùng chung module
//! này để tránh tạo nhiều database/config chỉ vì working directory khác nhau.

const CONFIG_REL_PATH: &str = "data/liva-config.json";
const CONFIG_FILE_NAME: &str = "liva-config.json";

pub const DEFAULT_MODELS_DIR: &str = "E:\\AI_Models";

/// Model router mặc định khi chưa có config. Tên này phải khớp manifest tải model.
pub const DEFAULT_ROUTER_MODEL: &str = "gemma-4-E4B-it-qat-GGUF/gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf";
pub const DEFAULT_EXPERT_MODEL: &str = "gemma-4-12B-it-qat-UD-Q4_K_XL.gguf";

/// Thư mục dữ liệu người dùng của bản cài, cùng quy ước bundle id của Tauri.
pub const APP_DATA_DIR_NAME: &str = "com.liva.cognitive-os";

/// Neo dữ liệu của các bản trước 28/07/2026.
const LEGACY_DATA_DIR_NAME: &str = "LIVA";

/// Config và database luôn dùng chung một neo ghi được.
pub fn config_file_path() -> std::path::PathBuf {
    data_dir().join(CONFIG_FILE_NAME)
}

/// Thư mục dữ liệu ghi được, không phụ thuộc working directory.
///
/// Trong cây nguồn, ưu tiên thư mục chứa `data/liva-config.json`. Bản đóng gói
/// dùng `%LOCALAPPDATA%\com.liva.cognitive-os\data`, trừ khi `LIVA_HOME` đặt
/// một neo rõ ràng hoặc dữ liệu thật vẫn còn ở neo cũ.
pub fn data_dir() -> std::path::PathBuf {
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(CONFIG_REL_PATH);
        if candidate.exists()
            && let Some(parent) = candidate.parent()
        {
            return parent.to_path_buf();
        }
    }
    if let Some(home) = user_home_dir() {
        return home.join("data");
    }
    std::path::PathBuf::from("data")
}

/// Có dấu vết dữ liệu thật của người dùng dưới `home` không?
///
/// Không dùng riêng sự tồn tại của `data/`: NSIS cũng tạo thư mục đó để chứa
/// manifest, nhưng manifest không chứng minh đây là một bản nâng cấp.
fn co_du_lieu_nguoi_dung(home: &std::path::Path) -> bool {
    let data = home.join("data");
    if data.join(CONFIG_FILE_NAME).exists() {
        return true;
    }
    if data
        .join("agents")
        .join("liva_core")
        .join("structured_memory.sqlite")
        .exists()
    {
        return true;
    }
    std::fs::read_dir(home.join("models")).is_ok_and(|mut d| d.next().is_some())
}

/// Gốc dữ liệu người dùng: `LIVA_HOME` → neo mới → neo cũ → neo mới trống.
pub fn user_home_dir() -> Option<std::path::PathBuf> {
    if let Some(h) = std::env::var_os("LIVA_HOME")
        && !h.is_empty()
    {
        return Some(std::path::PathBuf::from(h));
    }
    let local = std::path::PathBuf::from(std::env::var_os("LOCALAPPDATA")?);
    let moi = local.join(APP_DATA_DIR_NAME);
    if co_du_lieu_nguoi_dung(&moi) {
        return Some(moi);
    }
    let cu = local.join(LEGACY_DATA_DIR_NAME);
    if co_du_lieu_nguoi_dung(&cu) {
        return Some(cu);
    }
    Some(moi)
}

/// Gốc để ghi tài nguyên lớn và dò tài nguyên của bản cài.
pub fn resource_write_root() -> std::path::PathBuf {
    let d = data_dir();
    match d.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.to_path_buf(),
        _ => std::path::PathBuf::from("."),
    }
}

/// Những database có thể đã được tạo nhầm do working directory.
///
/// Hàm chỉ báo cáo, không tự động gộp hoặc di trú dữ liệu.
pub fn stray_database_paths(dang_dung: &std::path::Path) -> Vec<std::path::PathBuf> {
    const REL: &str = "data/agents/liva_core/structured_memory.sqlite";
    let dang_dung = dang_dung.canonicalize().ok();
    [
        "",
        "..",
        "../..",
        "liva-native-core",
        "liva-desktop/src-tauri",
    ]
    .iter()
    .map(|p| std::path::Path::new(p).join(REL))
    .filter(|p| p.exists())
    .filter(|p| p.canonicalize().ok() != dang_dung)
    .collect()
}

fn read_config_file() -> serde_json::Value {
    std::fs::read_to_string(config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Thư mục chứa executable đang chạy. `None` khi hệ điều hành không cung cấp.
pub fn exe_dir() -> Option<std::path::PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(std::path::Path::to_path_buf))
}

/// Các ứng viên tài nguyên chỉ-đọc theo thứ tự ưu tiên.
///
/// Thứ tự là một phần của contract: cây dev → dữ liệu người dùng → cạnh binary
/// → `resources/` cạnh binary. Không đổi thứ tự nếu chưa cập nhật test bản cài.
pub fn resource_candidate_paths(
    rel: &str,
    user_root: Option<&std::path::Path>,
    exe_dir: Option<&std::path::Path>,
) -> Vec<std::path::PathBuf> {
    let raw = std::path::Path::new(rel);
    let mut ds: Vec<std::path::PathBuf> = ["", "..", "../.."]
        .iter()
        .map(|p| std::path::Path::new(p).join(raw))
        .collect();
    if let Some(root) = user_root {
        ds.push(root.join(raw));
    }
    if let Some(dir) = exe_dir {
        ds.push(dir.join(raw));
        ds.push(dir.join("resources").join(raw));
    }
    ds
}

/// Resolve đường dẫn tương đối của repo từ mọi entry point và bản cài.
/// Đường dẫn tuyệt đối đi qua nguyên vẹn.
pub fn resolve_resource_path(rel: &str) -> std::path::PathBuf {
    let raw = std::path::Path::new(rel);
    if raw.is_absolute() {
        return raw.to_path_buf();
    }
    let user_root = resource_write_root();
    let exe = exe_dir();
    for candidate in resource_candidate_paths(rel, Some(&user_root), exe.as_deref()) {
        if candidate.exists() {
            return candidate;
        }
    }
    raw.to_path_buf()
}

/// Thư mục vault mặc định khi `LIVA_VAULT_PATH` không được đặt.
pub fn default_vault_path() -> std::path::PathBuf {
    let trong_repo = resolve_resource_path("teamwork_projects/obsidian_llm_wiki/vault");
    if trong_repo.exists() {
        return trong_repo;
    }
    resource_write_root().join("vault")
}

/// Thư mục model LLM mặc định, cùng gốc với dữ liệu người dùng.
pub fn models_dir_fallback() -> std::path::PathBuf {
    resource_write_root().join("models").join("llm")
}

fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
    match (base, patch) {
        (serde_json::Value::Object(base_map), serde_json::Value::Object(patch_map)) => {
            for (key, value) in patch_map {
                merge_json(
                    base_map
                        .entry(key.clone())
                        .or_insert(serde_json::Value::Null),
                    value,
                );
            }
        }
        (base_slot, patch_value) => *base_slot = patch_value.clone(),
    }
}

static CONFIG_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
static CONFIG_TEMP_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[cfg(windows)]
fn replace_file_atomically(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };

    let source_wide: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination_wide: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    // SAFETY: hai buffer NUL-terminated còn sống trong toàn bộ lời gọi.
    let replaced = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    std::fs::rename(source, destination)
}

pub(crate) fn update_config_file_at(
    path: &std::path::Path,
    payload: &serde_json::Value,
) -> Result<(), String> {
    use std::io::Write;
    use std::sync::atomic::Ordering;

    if !payload.is_object() {
        return Err("Config patch must be a JSON object".to_string());
    }

    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .map_err(|_| "Config write lock is poisoned".to_string())?;

    let mut config = if path.exists() {
        let content = std::fs::read_to_string(path)
            .map_err(|error| format!("Failed to read config file: {error}"))?;
        serde_json::from_str(&content)
            .map_err(|error| format!("Failed to parse existing config file: {error}"))?
    } else {
        serde_json::json!({})
    };
    if !config.is_object() {
        return Err("Existing config root must be a JSON object".to_string());
    }
    merge_json(&mut config, payload);
    let serialized = serde_json::to_vec_pretty(&config)
        .map_err(|error| format!("Failed to serialize config: {error}"))?;

    let parent = path
        .parent()
        .ok_or_else(|| "Config path has no parent directory".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|error| format!("Failed to create config directory: {error}"))?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "Config path has no valid file name".to_string())?;
    let sequence = CONFIG_TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temporary_path = parent.join(format!(
        ".{file_name}.{}-{sequence}.tmp",
        std::process::id()
    ));

    let result = (|| -> Result<(), String> {
        let mut temporary = std::fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temporary_path)
            .map_err(|error| format!("Failed to create temporary config file: {error}"))?;
        temporary
            .write_all(&serialized)
            .map_err(|error| format!("Failed to write temporary config file: {error}"))?;
        temporary
            .sync_all()
            .map_err(|error| format!("Failed to flush temporary config file: {error}"))?;
        drop(temporary);
        replace_file_atomically(&temporary_path, path)
            .map_err(|error| format!("Failed to replace config file atomically: {error}"))
    })();

    if result.is_err() {
        let _ = std::fs::remove_file(&temporary_path);
    }
    result
}

/// Router GGUF từ config; `None` khi provider không phải local.
pub fn configured_router_model_path() -> Option<std::path::PathBuf> {
    let config = read_config_file();
    let ai = config.get("ai").cloned().unwrap_or(serde_json::Value::Null);
    let provider = ai
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local");
    if provider != "local" {
        return None;
    }
    let dir = ai
        .get("localModelsDir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback);
    let model = ai
        .get("routerModel")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_ROUTER_MODEL);
    Some(dir.join(model))
}

/// Vision projector GGUF từ config; `None` khi chưa cấu hình hoặc không local.
pub fn configured_mmproj_path() -> Option<std::path::PathBuf> {
    let config = read_config_file();
    let ai = config.get("ai").cloned().unwrap_or(serde_json::Value::Null);
    if ai
        .get("provider")
        .and_then(|v| v.as_str())
        .unwrap_or("local")
        != "local"
    {
        return None;
    }
    let dir = ai
        .get("localModelsDir")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback);
    let mmproj = ai
        .get("mmprojModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(dir.join(mmproj))
}

/// Thư mục model cấu hình, fallback về vùng dữ liệu người dùng.
pub fn configured_models_dir() -> std::path::PathBuf {
    let config = read_config_file();
    config
        .get("ai")
        .and_then(|ai| ai.get("localModelsDir"))
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(std::path::PathBuf::from)
        .unwrap_or_else(models_dir_fallback)
}

/// Chỉ cho phép file `.gguf` nằm dưới thư mục model cấu hình.
pub fn validate_model_path(
    model_path: &std::path::Path,
    models_dir: &std::path::Path,
) -> Result<(), String> {
    if model_path
        .components()
        .any(|c| c == std::path::Component::ParentDir)
    {
        return Err("model_path không được chứa '..'".to_string());
    }
    let ext_ok = model_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("gguf"));
    if !ext_ok {
        return Err("model_path phải là file .gguf".to_string());
    }
    let full = models_dir.join(model_path);
    if !full.starts_with(models_dir) {
        return Err(format!(
            "model_path phải nằm trong thư mục model đã cấu hình ({})",
            models_dir.display()
        ));
    }
    Ok(())
}
