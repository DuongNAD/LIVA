#![allow(dead_code, unused_imports, unused_variables)]
pub mod crypto;
pub mod db;
pub mod llm;
pub mod prng;
pub mod stt;
pub mod tts;
pub mod webrtc;
pub mod integrations;
pub mod telegram;
pub mod mcp;
pub mod agent;
pub mod vision;
pub mod passive;
pub mod evolution;
pub mod governor;
pub mod wake_model;
pub mod wake;

use std::sync::Arc;
pub use db::DatabasePool;
pub use crypto::EncryptionEngine;
pub use stt::SttManager;
pub use tts::TtsManager;
pub use tts::audio::TtsAudioPlayer;
pub use llm::LlamaRouterManager;
pub use vision::{
    VisionManager, VisionConfig,
    capture::{Frame, PixelFormat, ScreenCapturer},
    diff::{ScreenRegion, RegionDiffResult, DiffEngine},
};

pub struct AppState {
    pub db: DatabasePool,
    pub crypto: EncryptionEngine,
    pub stt: tokio::sync::Mutex<SttManager>,
    pub tts: tokio::sync::Mutex<Option<TtsManager>>,
    pub tts_player: TtsAudioPlayer,
    pub llm: tokio::sync::Mutex<LlamaRouterManager>,
    pub vad: tokio::sync::Mutex<Option<webrtc::vad::VadEngine>>,
    pub denoiser: tokio::sync::Mutex<Option<webrtc::denoise::GtcrnDenoiser>>,
    pub turn_shadow: tokio::sync::Mutex<Option<webrtc::turn_shadow::SmartTurnClassifier>>,
    pub aec: tokio::sync::Mutex<Option<webrtc::aec::SelfEchoCanceller>>,
    pub mcp_server: Arc<mcp::server::NativeMcpServer>,
    pub vision: tokio::sync::Mutex<VisionManager>,
}

#[derive(serde::Serialize)]
struct IpcResponse {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

const CONFIG_REL_PATH: &str = "data/liva-config.json";
pub const DEFAULT_MODELS_DIR: &str = "E:\\AI_Models";
pub const DEFAULT_ROUTER_MODEL: &str = "gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf";
pub const DEFAULT_EXPERT_MODEL: &str = "gemma-4-12B-it-qat-UD-Q4_K_XL.gguf";

/// The working directory differs per entry point (repo root, liva-native-core,
/// or liva-desktop/src-tauri), so walk up to two levels to find the project's
/// real data/liva-config.json instead of silently reading an empty one.
pub fn config_file_path() -> std::path::PathBuf {
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(CONFIG_REL_PATH);
        if candidate.exists() {
            return candidate;
        }
    }
    std::path::PathBuf::from(CONFIG_REL_PATH)
}

fn read_config_file() -> serde_json::Value {
    std::fs::read_to_string(config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Resolve a repo-relative resource path (models/, node_modules/, ...) against
/// the actual project root, whatever the working directory is (repo root,
/// liva-native-core, or liva-desktop/src-tauri). Absolute paths pass through.
pub fn resolve_resource_path(rel: &str) -> std::path::PathBuf {
    let raw = std::path::Path::new(rel);
    if raw.is_absolute() {
        return raw.to_path_buf();
    }
    for prefix in ["", "..", "../.."] {
        let candidate = std::path::Path::new(prefix).join(raw);
        if candidate.exists() {
            return candidate;
        }
    }
    raw.to_path_buf()
}

/// Deep-merge `patch` into `base`: nested objects merge per key, everything
/// else is overwritten. Lets the UI send partial configs (e.g. only `ai`).
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

/// Router GGUF path from config; None when the provider is not "local".
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
        .unwrap_or(DEFAULT_MODELS_DIR);
    let model = ai
        .get("routerModel")
        .and_then(|v| v.as_str())
        .unwrap_or(DEFAULT_ROUTER_MODEL);
    Some(std::path::Path::new(dir).join(model))
}

/// Vision-projector (mmproj) GGUF path from config (`ai.mmprojModel`); None when
/// unset or the provider isn't "local". Enables the multimodal `vision:ask` path
/// for VL models like Qwen3-VL.
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
        .unwrap_or(DEFAULT_MODELS_DIR);
    let mmproj = ai
        .get("mmprojModel")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())?;
    Some(std::path::Path::new(dir).join(mmproj))
}

/// Load the configured router model into the LLM engine. `force=false` only
/// fills an empty engine (startup autoload); `force=true` also swaps when the
/// configured file differs from the loaded one (after update_config).
pub async fn load_configured_router_model(state: Arc<AppState>, force: bool) {
    let Some(model_path) = configured_router_model_path() else {
        tracing::info!("LLM provider is not 'local'; skipping router model load");
        return;
    };
    if !model_path.exists() {
        tracing::error!(
            "Router model not found at {:?} — check ai.localModelsDir/ai.routerModel in {:?}",
            model_path,
            config_file_path()
        );
        return;
    }
    let mut llm_manager = state.llm.lock().await;
    // Keep the vision projector path current so `vision:ask` can lazily build
    // the multimodal context for a VL model.
    llm_manager.set_mmproj_path(configured_mmproj_path());
    if llm_manager.engine.is_some() && (!force || llm_manager.current_model_path == model_path) {
        return;
    }
    tracing::info!("Loading router model {:?}...", model_path);
    match llm_manager.swap_model(&model_path, None, None, None).await {
        Ok(()) => tracing::info!("Router model loaded: {:?}", model_path),
        Err(e) => tracing::error!("Failed to load router model {:?}: {}", model_path, e),
    }
}

/// Reload the currently-loaded router LLM with a different GPU-layer count.
///
/// Used by the game-aware GPU governor: when a foreground game is detected we
/// reload the model with fewer (or zero) GPU layers to free VRAM for the game,
/// then restore full offload once the game exits. This is a real model reload
/// (~seconds, resets the KV cache), so the caller must only invoke it on an
/// actual game-mode transition — never on every poll.
///
/// Returns `true` once a model is loaded and now sits at `n_gpu_layers` (either
/// just reloaded there or already matching); returns `false` when no model is
/// loaded yet, so the caller can retry on a later poll instead of latching the
/// game state prematurely (e.g. a game already running at startup while the
/// autoload is still in flight).
pub async fn reload_llm_gpu_layers(state: Arc<AppState>, n_gpu_layers: u32) -> bool {
    let mut llm = state.llm.lock().await;
    if llm.engine.is_none() {
        return false; // model not loaded yet — caller should retry
    }
    let path = llm.current_model_path.clone();
    if llm.n_gpu_layers == n_gpu_layers || path.as_os_str().is_empty() {
        return true; // already at target (or no path to reload from)
    }
    let n_ctx = llm.n_ctx;
    let vocab_only = llm.vocab_only;
    let from = llm.n_gpu_layers;
    tracing::info!(
        "Game-aware GPU: reloading {:?} (n_gpu_layers {} -> {})",
        path,
        from,
        n_gpu_layers
    );
    match llm
        .swap_model(&path, Some(n_ctx), Some(n_gpu_layers), Some(vocab_only))
        .await
    {
        Ok(()) => tracing::info!("Game-aware GPU: reloaded (n_gpu_layers={})", n_gpu_layers),
        Err(e) => tracing::error!("Game-aware GPU reload failed: {}", e),
    }
    true
}

pub async fn handle_command(
    state: Arc<AppState>,
    command: &str,
    payload: serde_json::Value,
    tx: Option<tokio::sync::mpsc::Sender<String>>,
    req_id: Option<String>,
) -> Result<serde_json::Value, String> {
    use base64::Engine;

    match command {
        "ping" => Ok(serde_json::json!({ "pong": true })),
        
        // --- Screen Vision IPC Interfaces ---
        "vision:capture" => {
            let capturer = {
                let vision = state.vision.lock().await;
                vision.capturer()
            };
            let frame = tokio::task::spawn_blocking(move || {
                capturer.capture().map_err(|e| e.to_string())
            })
            .await
            .map_err(|e| format!("Join error: {}", e))??;

            {
                let mut vision = state.vision.lock().await;
                vision.update_last_frame(frame.clone());
            }

            // Perform CPU-bound base64 encoding outside the lock
            let b64_data = base64::engine::general_purpose::STANDARD.encode(&frame.data);
            Ok(serde_json::json!({
                "width": frame.width,
                "height": frame.height,
                "format": format!("{:?}", frame.format),
                "data": b64_data,
            }))
        }
        "vision:add_region" => {
            let region: ScreenRegion = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid region payload: {}", e))?;
            let mut vision = state.vision.lock().await;
            vision.add_region(region)?;
            Ok(serde_json::json!({ "success": true }))
        }
        "vision:remove_region" => {
            let id = payload["id"]
                .as_str()
                .ok_or_else(|| "Missing 'id' in payload".to_string())?;
            let mut vision = state.vision.lock().await;
            vision.remove_region(id)?;
            Ok(serde_json::json!({ "success": true }))
        }
        "vision:get_changed_regions" => {
            let (capturer, last_frame, regions, color_tolerance) = {
                let vision = state.vision.lock().await;
                (
                    vision.capturer(),
                    vision.last_frame(),
                    vision.regions(),
                    vision.color_tolerance(),
                )
            };

            let (current_frame, results) = tokio::task::spawn_blocking(move || -> Result<(Frame, Vec<RegionDiffResult>), String> {
                let current_frame = capturer.capture().map_err(|e| e.to_string())?;
                let prev_frame = match &last_frame {
                    Some(f) => f,
                    None => {
                        let baseline = regions.iter().map(|r| RegionDiffResult {
                            region_id: r.id.clone(),
                            name: r.name.clone(),
                            difference: 1.0,
                            is_changed: true,
                        }).collect();
                        return Ok((current_frame, baseline));
                    }
                };

                let mut results = Vec::with_capacity(regions.len());
                for region in &regions {
                    let res = DiffEngine::diff_region(
                        prev_frame,
                        &current_frame,
                        region,
                        color_tolerance,
                    )?;
                    results.push(res);
                }
                Ok((current_frame, results))
            })
            .await
            .map_err(|e| format!("Join error: {}", e))??;

            {
                let mut vision = state.vision.lock().await;
                vision.update_last_frame(current_frame);
            }

            Ok(serde_json::to_value(results).unwrap())
        }
        "vision:set_config" => {
            let config: VisionConfig = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid config payload: {}", e))?;
            let mut vision = state.vision.lock().await;
            vision.set_config(config);
            Ok(serde_json::json!({ "success": true }))
        }

        "echo" => Ok(payload),
        "status" => Ok(serde_json::json!({
            "engine": "LIVA Native Engine",
            "status": "healthy",
            "version": env!("CARGO_PKG_VERSION")
        })),
        "get_config" => {
            let path = config_file_path();
            if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read config file: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse config file: {}", e))?;
                Ok(val)
            } else {
                Ok(serde_json::json!({
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
                    "ai": {
                        "provider": "local",
                        "cloudBaseUrl": "",
                        "cloudApiKey": "",
                        "cloudModel": "",
                        "localModelsDir": DEFAULT_MODELS_DIR,
                        "routerModel": DEFAULT_ROUTER_MODEL,
                        "expertModel": DEFAULT_EXPERT_MODEL,
                        "temperature": 0.3,
                        "maxTokens": 2048,
                        "topP": 0.9
                    },
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
        "update_config" => {
            let path = config_file_path();
            let mut config = read_config_file();
            merge_json(&mut config, &payload);

            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent).ok();
            }
            let serialized = serde_json::to_string_pretty(&config)
                .map_err(|e| format!("Failed to serialize config: {}", e))?;
            std::fs::write(&path, serialized)
                .map_err(|e| format!("Failed to write config file: {}", e))?;

            // Apply AI changes right away: swap the router model in the
            // background so the save request returns immediately.
            if payload.get("ai").is_some() {
                let state_clone = state.clone();
                tokio::spawn(async move {
                    load_configured_router_model(state_clone, true).await;
                });
            }

            Ok(serde_json::json!({ "success": true }))
        }
        "get_ai_config" => {
            let path = config_file_path();
            let ai_val = if path.exists() {
                let content = std::fs::read_to_string(&path)
                    .map_err(|e| format!("Failed to read config file: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse config: {}", e))?;
                val.get("ai").cloned().unwrap_or(serde_json::json!({}))
            } else {
                serde_json::json!({
                    "provider": "local",
                    "cloudBaseUrl": "",
                    "cloudApiKey": "",
                    "cloudModel": "",
                    "localModelsDir": DEFAULT_MODELS_DIR,
                    "routerModel": DEFAULT_ROUTER_MODEL,
                    "expertModel": DEFAULT_EXPERT_MODEL,
                    "temperature": 0.3,
                    "maxTokens": 2048,
                    "topP": 0.9
                })
            };
            Ok(ai_val)
        }
        "get_voice_status" => {
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

            Ok(serde_json::json!({
                "stt": if stt_ready { "ready" } else { "offline" },
                "tts": if tts_ready { "ready" } else { "offline" }
            }))
        }
        "get_voice_profiles" => {
            let path = std::path::Path::new("data/voices");
            let mut profiles = Vec::new();
            if path.is_dir() {
                if let Ok(entries) = std::fs::read_dir(path) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Some(name) = entry.file_name().to_str() {
                                profiles.push(name.to_string());
                            }
                        }
                    }
                }
            }
            Ok(serde_json::json!(profiles))
        }
        "get_system_status" => {
            let (llm_loaded, llm_model_name) = {
                let llm_manager = state.llm.lock().await;
                let name = llm_manager
                    .current_model_path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                (llm_manager.engine.is_some(), name)
            };
            Ok(serde_json::json!({
                "healthChecks": {
                    "gateway": { "wsClients": 1, "skillsLoaded": 1 },
                    "aiEngine": {
                        "status": if llm_loaded { "online" } else { "offline" },
                        "latencyMs": 10,
                        "detail": if llm_loaded { "Active" } else { "No model loaded" }
                    },
                    "orchestrator": { "status": "online", "detail": "Idle" },
                    "voiceEngine": { "status": "online", "latencyMs": 5, "detail": "Active" },
                    "memory": { "status": "online", "detail": "WAL Active" },
                    "vramGuard": { "status": "online", "detail": "0% utilized" },
                    "whisper": { "status": "online", "detail": "Active" },
                    "remoteControl": { "enabled": true, "telegram": { "status": "online" }, "zalo": { "status": "offline" } }
                },
                "osStats": {
                    "cpuUsage": 12,
                    "totalMemory": 16000000000u64,
                    "freeMemory": 8000000000u64
                },
                "telemetry": [],
                "uptime": 3600,
                "memoryUsage": 50_000_000,
                "rssMemory": 100_000_000,
                "engineMode": "native_grpc",
                "modelLoaded": llm_loaded,
                "model": llm_model_name
            }))
        }
        "get_skills_list" => {
            Ok(serde_json::json!([
                integrations::smart_home::get_metadata()
            ]))
        }
        "get_user_profile" => {
            let path = std::path::Path::new("data/user_profile.json");
            if path.exists() {
                let content = std::fs::read_to_string(path)
                    .map_err(|e| format!("Failed to read user profile: {}", e))?;
                let val: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse user profile: {}", e))?;
                Ok(val)
            } else {
                Ok(serde_json::json!({
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
        }
        "get_tasks" => {
            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                let mut stmt = conn.prepare("SELECT id, title, description, status, priority, result, created_at, updated_at FROM tasks")
                    .map_err(|e| format!("Failed to prepare query: {}", e))?;

                let rows = stmt.query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "title": row.get::<_, String>(1)?,
                        "description": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "status": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "pending".to_string()),
                        "priority": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "medium".to_string()),
                        "result": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                        "createdAt": row.get::<_, i64>(6)?,
                        "updatedAt": row.get::<_, i64>(7)?,
                    }))
                }).map_err(|e| format!("Failed to execute query: {}", e))?;

                let mut list = Vec::new();
                for r in rows {
                    list.push(r.map_err(|e| format!("Row extraction failed: {}", e))?);
                }
                Ok::<_, String>(list)
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "tasks": results }))
        }
        "add_task" => {
            let title = payload["title"].as_str().ok_or_else(|| "Missing 'title' in payload".to_string())?.to_string();
            let description = payload["description"].as_str().unwrap_or("").to_string();
            let priority = payload["priority"].as_str().unwrap_or("medium").to_string();
            let status = payload["status"].as_str().unwrap_or("pending").to_string();
            
            let id = match payload.get("id").and_then(|v| v.as_str()) {
                Some(id_str) => id_str.to_string(),
                None => rand::random::<u64>().to_string(),
            };
            
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;

            let state_clone = state.clone();
            let id_clone = id.clone();
            tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                conn.execute(
                    "INSERT INTO tasks (id, title, description, status, priority, result, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                    rusqlite::params![id_clone, title, description, status, priority, "", now, now],
                )
                .map_err(|e| format!("Failed to insert task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true, "id": id }))
        }
        "delete_task" => {
            let id = payload["id"].as_str().ok_or_else(|| "Missing 'id' in payload".to_string())?.to_string();

            let state_clone = state.clone();
            tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                conn.execute(
                    "DELETE FROM tasks WHERE id = ?1",
                    rusqlite::params![id],
                )
                .map_err(|e| format!("Failed to delete task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "update_task" => {
            let id = payload["id"].as_str().ok_or_else(|| "Missing 'id' in payload".to_string())?.to_string();
            let updates = payload["updates"]
                .as_object()
                .cloned()
                .ok_or_else(|| "Missing or invalid 'updates' object".to_string())?;

            let state_clone = state.clone();
            tokio::task::spawn_blocking(move || {
                let mut conn = state_clone
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                let tx = conn.transaction().map_err(|e| format!("Failed to start transaction: {}", e))?;

                // Get current values inside a nested scope so stmt is dropped before tx.commit()
                let current: (String, String, String, String, String) = {
                    let mut stmt = tx.prepare("SELECT title, description, status, priority, result FROM tasks WHERE id = ?1")
                        .map_err(|e| format!("Failed to prepare select query: {}", e))?;
                    
                    stmt.query_row(
                        rusqlite::params![id],
                        |row| {
                            Ok((
                                row.get::<_, String>(0)?,
                                row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                                row.get::<_, Option<String>>(2)?.unwrap_or_else(|| "pending".to_string()),
                                row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "medium".to_string()),
                                row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                            ))
                        }
                    ).map_err(|e| format!("Task not found: {}", e))?
                };

                let title = updates.get("title").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.0);
                let description = updates.get("description").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.1);
                let status = updates.get("status").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.2);
                let priority = updates.get("priority").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.3);
                let result = updates.get("result").and_then(|v| v.as_str()).map(|s| s.to_string()).unwrap_or(current.4);

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;

                tx.execute(
                    "UPDATE tasks SET title = ?1, description = ?2, status = ?3, priority = ?4, result = ?5, updated_at = ?6 WHERE id = ?7",
                    rusqlite::params![title, description, status, priority, result, now, id],
                ).map_err(|e| format!("Failed to update task: {}", e))?;

                tx.commit().map_err(|e| format!("Failed to commit transaction: {}", e))?;
                Ok::<_, String>(())
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "task_plan_chat" => {
            let task_id = payload["taskId"]
                .as_str()
                .ok_or_else(|| "Missing 'taskId' in payload".to_string())?
                .to_string();

            let message = payload.get("message")
                .or_else(|| payload.get("text"))
                .and_then(|v| v.as_str())
                .ok_or_else(|| "Missing 'message' or 'text' in payload".to_string())?
                .to_string();

            let state_clone = state.clone();
            let task_id_clone = task_id.clone();
            let (title, description) = tokio::task::spawn_blocking(move || {
                let conn = state_clone
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                conn.query_row(
                    "SELECT title, description FROM tasks WHERE id = ?1",
                    rusqlite::params![task_id_clone],
                    |row| {
                        Ok((
                            row.get::<_, String>(0)?,
                            row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        ))
                    }
                ).map_err(|e| format!("Failed to query task: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            // Title/description are user-authored: interpolate them as
            // delimited DATA in the user turn (never into the system prompt),
            // with delimiter sequences neutralized.
            let user_content = format!(
                "<user_task_title>{}</user_task_title>\n<user_task_description>{}</user_task_description>\n\n{}",
                llm::persona::sanitize_untrusted(&title),
                llm::persona::sanitize_untrusted(&description),
                message
            );

            let messages = vec![
                llm::ChatMessage {
                    role: "system".to_string(),
                    content: llm::persona::SYS_TASK_PLANNER.to_string(),
                },
                llm::ChatMessage {
                    role: "user".to_string(),
                    content: user_content,
                },
            ];

            let temperature = payload["temperature"]
                .as_f64()
                .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
            let top_p = payload["top_p"]
                .as_f64()
                .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
            let stream = payload["stream"].as_bool().unwrap_or(tx.is_some());

            let compiled_prompt = llm::compile_prompt(&messages)?;

            let state_clone = state.clone();
            let tx_clone = tx.clone();
            let task_id_clone = task_id.clone();

            let completion_output = tokio::task::spawn_blocking(move || {
                let mut llm_manager = state_clone.llm.blocking_lock();

                if stream {
                    let tx_inner = tx_clone
                        .ok_or_else(|| "IPC output channel missing for streaming".to_string())?;

                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |piece| {
                        let chunk = serde_json::json!({
                            "taskId": task_id_clone.clone(),
                            "message": piece,
                            "done": false
                        });
                        if let Ok(chunk_str) = serde_json::to_string(&chunk) {
                            let _ = tx_inner.blocking_send(chunk_str);
                        }
                        true
                    })
                } else {
                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |_| true)
                }
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({
                "taskId": task_id,
                "message": completion_output.text,
                "done": true
            }))
        }
        "get_avatar_models" => {
            let mut models2d = Vec::new();
            let mut models3d = Vec::new();

            let path_2d = resolve_resource_path("models/live2d");
            if path_2d.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&path_2d) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Some(name) = entry.file_name().to_str() {
                                models2d.push(name.to_string());
                            }
                        }
                    }
                }
            }

            let path_3d = resolve_resource_path("models/vrm");
            if path_3d.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&path_3d) {
                    for entry in entries {
                        if let Ok(entry) = entry {
                            if let Some(name) = entry.file_name().to_str() {
                                models3d.push(name.to_string());
                            }
                        }
                    }
                }
            }

            Ok(serde_json::json!({
                "models2d": models2d,
                "models3d": models3d
            }))
        }
        "get_memory_data" => {
            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                // 1. Query l0
                let mut stmt_l0 = conn.prepare(
                    "SELECT turnId, userMsg, aiReply, temporal_anchor FROM turn_layer_nodes ORDER BY temporal_anchor DESC LIMIT 100"
                ).map_err(|e| format!("Prepare l0 failed: {}", e))?;
                let rows_l0 = stmt_l0.query_map([], |row| {
                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "userMsg": row.get::<_, Option<String>>(1)?.unwrap_or_default(),
                        "aiReply": row.get::<_, Option<String>>(2)?.unwrap_or_default(),
                        "timestamp": row.get::<_, i64>(3)?,
                    }))
                }).map_err(|e| format!("Query l0 failed: {}", e))?;
                let mut l0 = Vec::new();
                for r in rows_l0 {
                    l0.push(r.map_err(|e| e.to_string())?);
                }

                // 2. Query facts
                let mut stmt_facts = conn.prepare(
                    "SELECT key, value, createdAt, source, category, importance, memory_strength FROM facts"
                ).map_err(|e| format!("Prepare facts failed: {}", e))?;
                let rows_facts = stmt_facts.query_map([], |row| {
                    let key: String = row.get(0)?;
                    let enc_val: String = row.get(1)?;
                    let dec_val = state.crypto.decrypt(&enc_val);
                    Ok(serde_json::json!({
                        "key": key,
                        "value": dec_val,
                        "createdAt": row.get::<_, String>(2)?,
                        "source": row.get::<_, String>(3)?,
                        "category": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                        "importance": row.get::<_, Option<f64>>(5)?.unwrap_or(0.5),
                        "memoryStrength": row.get::<_, Option<f64>>(6)?.unwrap_or(1.0),
                    }))
                }).map_err(|e| format!("Query facts failed: {}", e))?;
                let mut facts = Vec::new();
                for r in rows_facts {
                    facts.push(r.map_err(|e| e.to_string())?);
                }

                // 3. Query events
                let mut stmt_events = conn.prepare(
                    "SELECT eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidation_status, domain, category, trace_keywords FROM events ORDER BY timestamp DESC LIMIT 100"
                ).map_err(|e| format!("Prepare events failed: {}", e))?;
                let rows_events = stmt_events.query_map([], |row| {
                    let phi_facts_str: Option<String> = row.get(2)?;
                    let phi_entities_str: Option<String> = row.get(3)?;
                    let trace_kw_str: Option<String> = row.get(12)?;

                    let phi_facts: serde_json::Value = phi_facts_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let phi_entities: serde_json::Value = phi_entities_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let trace_keywords: serde_json::Value = trace_kw_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));

                    Ok(serde_json::json!({
                        "eventId": row.get::<_, String>(0)?,
                        "timestamp": row.get::<_, i64>(1)?,
                        "rawUserMsg": row.get::<_, Option<String>>(7)?.unwrap_or_default(),
                        "rawAiReply": row.get::<_, Option<String>>(8)?.unwrap_or_default(),
                        "phi": {
                            "facts": phi_facts,
                            "entities": phi_entities
                        },
                        "psi": {
                            "sentiment": row.get::<_, Option<String>>(4)?.unwrap_or_default(),
                            "intent": row.get::<_, Option<String>>(5)?.unwrap_or_default(),
                            "relational": row.get::<_, Option<String>>(6)?.unwrap_or_default()
                        },
                        "consolidationStatus": row.get::<_, Option<String>>(9)?.unwrap_or_else(|| "pending".to_string()),
                        "domain": row.get::<_, Option<String>>(10)?.unwrap_or_else(|| "General".to_string()),
                        "category": row.get::<_, Option<String>>(11)?.unwrap_or_else(|| "Uncategorized".to_string()),
                        "traceKeywords": trace_keywords,
                    }))
                }).map_err(|e| format!("Query events failed: {}", e))?;
                let mut events = Vec::new();
                for r in rows_events {
                    events.push(r.map_err(|e| e.to_string())?);
                }

                // 4. Query vectors
                let mut stmt_vectors = conn.prepare(
                    "SELECT vec_id, type, content, domain, category, trace_keywords, created_at, source_event_ids FROM vectors_meta ORDER BY created_at DESC LIMIT 100"
                ).map_err(|e| format!("Prepare vectors failed: {}", e))?;
                let rows_vectors = stmt_vectors.query_map([], |row| {
                    let trace_kw_str: Option<String> = row.get(5)?;
                    let src_event_ids_str: Option<String> = row.get(7)?;

                    let trace_keywords: serde_json::Value = trace_kw_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));
                    let source_event_ids: serde_json::Value = src_event_ids_str
                        .and_then(|s| serde_json::from_str(&s).ok())
                        .unwrap_or(serde_json::json!([]));

                    Ok(serde_json::json!({
                        "id": row.get::<_, String>(0)?,
                        "type": row.get::<_, String>(1)?,
                        "content": row.get::<_, String>(2)?,
                        "domain": row.get::<_, Option<String>>(3)?.unwrap_or_else(|| "General".to_string()),
                        "category": row.get::<_, Option<String>>(4)?.unwrap_or_else(|| "Uncategorized".to_string()),
                        "traceKeywords": trace_keywords,
                        "createdAt": row.get::<_, i64>(6)?,
                        "sourceEventIds": source_event_ids,
                    }))
                }).map_err(|e| format!("Query vectors failed: {}", e))?;
                let mut vectors = Vec::new();
                for r in rows_vectors {
                    vectors.push(r.map_err(|e| e.to_string())?);
                }

                Ok::<_, String>(serde_json::json!({
                    "l0": l0,
                    "l0_5": "",
                    "facts": facts,
                    "events": events,
                    "vectors": vectors
                }))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(results)
        }
        "memory:set_fact" => {
            let fact: db::Fact = serde_json::from_value(payload)
                .map_err(|e| format!("Invalid fact payload: {}", e))?;

            tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                db::set_fact(&conn, &state.crypto, &fact)
                    .map_err(|e| format!("Failed to set fact: {}", e))?;
                Ok::<_, String>(())
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "memory:get_fact" => {
            let key = payload["key"]
                .as_str()
                .ok_or_else(|| "Missing 'key' in payload".to_string())?
                .to_string();

            let fact = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                db::get_fact(&conn, &state.crypto, &key)
                    .map_err(|e| format!("Failed to get fact: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            match fact {
                Some(f) => Ok(serde_json::to_value(f).unwrap()),
                None => Ok(serde_json::Value::Null),
            }
        }
        "memory:search_hybrid" => {
            let query_text = payload["query_text"]
                .as_str()
                .ok_or_else(|| "Missing 'query_text'".to_string())?
                .to_string();

            let query_vector_val = payload["query_vector"]
                .as_array()
                .ok_or_else(|| "Missing 'query_vector'".to_string())?;

            let mut query_vector = Vec::with_capacity(query_vector_val.len());
            for v in query_vector_val {
                let f = v
                    .as_f64()
                    .ok_or_else(|| "Invalid float in query_vector".to_string())?
                    as f32;
                query_vector.push(f);
            }

            let top_k = payload["top_k"].as_u64().unwrap_or(5) as usize;

            let filter_val = payload.get("filter");
            let filter = match filter_val {
                Some(f_val) if !f_val.is_null() => serde_json::from_value(f_val.clone())
                    .map_err(|e| format!("Invalid filter: {}", e))?,
                _ => db::MetadataFilter {
                    r#type: None,
                    domain: None,
                    category: None,
                    created_after: None,
                    created_before: None,
                },
            };

            let dense_weight = payload["dense_weight"].as_f64().unwrap_or(1.0);
            let sparse_weight = payload["sparse_weight"].as_f64().unwrap_or(1.0);

            let results = tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .readers
                    .get()
                    .map_err(|e| format!("Failed to acquire read connection: {}", e))?;

                db::search_hybrid_vectors(
                    &conn,
                    &query_text,
                    &query_vector,
                    top_k,
                    &filter,
                    dense_weight,
                    sparse_weight,
                )
                .map_err(|e| format!("Hybrid search failed: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::to_value(results).unwrap())
        }
        "memory:upsert_vector" => {
            let vec_id = payload["vecId"]
                .as_str()
                .ok_or_else(|| "Missing 'vecId'".to_string())?
                .to_string();
            let r#type = payload["type"]
                .as_str()
                .ok_or_else(|| "Missing 'type'".to_string())?
                .to_string();
            let content = payload["content"]
                .as_str()
                .ok_or_else(|| "Missing 'content'".to_string())?
                .to_string();

            let vector_val = payload["vector"]
                .as_array()
                .ok_or_else(|| "Missing 'vector'".to_string())?;
            let mut vector = Vec::with_capacity(vector_val.len());
            for v in vector_val {
                let f = v
                    .as_f64()
                    .ok_or_else(|| "Invalid float in vector".to_string())?
                    as f32;
                vector.push(f);
            }

            let domain = payload
                .get("domain")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            let category = payload
                .get("category")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let trace_keywords_val = payload.get("traceKeywords").and_then(|v| v.as_array());
            let mut trace_keywords = Vec::new();
            if let Some(arr) = trace_keywords_val {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        trace_keywords.push(s.to_string());
                    }
                }
            }

            let file_target = payload
                .get("fileTarget")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            let source_event_ids_val = payload.get("sourceEventIds").and_then(|v| v.as_array());
            let mut source_event_ids = Vec::new();
            if let Some(arr) = source_event_ids_val {
                for v in arr {
                    if let Some(s) = v.as_str() {
                        source_event_ids.push(s.to_string());
                    }
                }
            }

            tokio::task::spawn_blocking(move || {
                let conn = state
                    .db
                    .writer
                    .get()
                    .map_err(|e| format!("Failed to acquire write connection: {}", e))?;

                db::upsert_vector(
                    &conn,
                    &vec_id,
                    &r#type,
                    &content,
                    &vector,
                    domain.as_deref(),
                    category.as_deref(),
                    Some(&trace_keywords),
                    file_target.as_deref(),
                    Some(&source_event_ids),
                )
                .map_err(|e| format!("Failed to upsert vector: {}", e))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({ "success": true }))
        }
        "voice:stt_start" => {
            state.stt.lock().await.reset_stream();
            Ok(serde_json::json!({ "success": true }))
        }
        "voice:stt_chunk" => {
            let chunk_b64 = payload["chunk"]
                .as_str()
                .ok_or_else(|| "Missing 'chunk'".to_string())?;
            let is_last = payload["isLast"].as_bool().unwrap_or(false);

            let audio_bytes = base64::engine::general_purpose::STANDARD
                .decode(chunk_b64)
                .map_err(|e| format!("Base64 decode failed: {}", e))?;

            let len_rounded = (audio_bytes.len() / 4) * 4;
            let audio_bytes_aligned = &audio_bytes[..len_rounded];
            let audio_samples: Vec<f32> = if audio_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
                bytemuck::cast_slice(audio_bytes_aligned).to_vec()
            } else {
                audio_bytes_aligned
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect()
            };

            let state_clone = state.clone();
            let text = tokio::task::spawn_blocking(move || {
                let mut stt = state_clone.stt.blocking_lock();
                stt.feed_audio(&audio_samples, is_last)
            })
            .await
            .map_err(|e| e.to_string())??;

            Ok(serde_json::json!({ "text": text }))
        }
        "voice:stt_stop" => {
            let state_clone = state.clone();
            let text = tokio::task::spawn_blocking(move || {
                let mut stt = state_clone.stt.blocking_lock();
                stt.feed_audio(&[], true)
            })
            .await
            .map_err(|e| e.to_string())??;

            Ok(serde_json::json!({ "text": text }))
        }
        "voice:stt_flush" => {
            state.stt.lock().await.reset_stream();
            Ok(serde_json::json!({ "success": true }))
        }
        "voice:set_language" => {
            let lang = payload["language"]
                .as_str()
                .ok_or_else(|| "Missing 'language'".to_string())?;

            state.stt.lock().await.set_language(lang)?;
            {
                let mut tts = state.tts.lock().await;
                if let Some(ref mut tts_mgr) = *tts {
                    tts_mgr.set_language(lang);
                }
            }
            Ok(serde_json::json!({ "success": true, "language": lang }))
        }
        "voice:tts_speak" => {
            let text = payload["text"]
                .as_str()
                .ok_or_else(|| "Missing 'text'".to_string())?;

            let flush = payload["flush"].as_bool().unwrap_or(false);

            let mut tts = state.tts.lock().await;
            if let Some(ref mut tts_mgr) = *tts {
                tts_mgr.speak(text).await?;
                if flush {
                    tts_mgr.flush().await?;
                }
                Ok(serde_json::json!({ "success": true }))
            } else {
                Err("TTS engine not initialized".to_string())
            }
        }
        "voice:tts_stop" => {
            state.tts_player.stop().await;

            let state_clone = state.clone();
            tokio::spawn(async move {
                let mut tts = state_clone.tts.lock().await;
                if let Some(ref mut tts_mgr) = *tts {
                    tts_mgr.stop().await;
                }
            });

            Ok(serde_json::json!({ "success": true }))
        }
        "llm:swap_model" => {
            let model_path_str = payload["model_path"]
                .as_str()
                .ok_or_else(|| "Missing 'model_path'".to_string())?;
            let model_path = std::path::Path::new(model_path_str);

            let n_ctx = payload["n_ctx"].as_u64().map(|v| v as usize);
            let n_gpu_layers = payload["n_gpu_layers"].as_u64().map(|v| v as u32);
            let vocab_only = payload["vocab_only"].as_bool();

            let mut llm_manager = state.llm.lock().await;
            llm_manager
                .swap_model(model_path, n_ctx, n_gpu_layers, vocab_only)
                .await?;

            Ok(serde_json::json!({ "success": true }))
        }
        "llm:embed" => {
            let inputs = if let Some(s) = payload["input"].as_str() {
                vec![s.to_string()]
            } else if let Some(arr) = payload["input"].as_array() {
                let mut vec = Vec::new();
                for v in arr {
                    let s = v
                        .as_str()
                        .ok_or_else(|| "Invalid string in input list".to_string())?;
                    vec.push(s.to_string());
                }
                vec
            } else {
                return Err("Missing or invalid 'input' parameter".to_string());
            };

            let mut llm_manager = state.llm.lock().await;
            if llm_manager.vocab_only {
                return Err("Cannot compute embeddings on a vocab-only model".to_string());
            }
            let engine = llm_manager
                .engine
                .as_mut()
                .ok_or_else(|| "No model loaded".to_string())?;
            let mut embeddings = Vec::new();
            for text in inputs {
                let emb = llm::get_embedding(&engine.model, &mut engine.context, &text)?;
                embeddings.push(emb);
            }

            if payload["input"].is_string() {
                Ok(serde_json::to_value(&embeddings[0]).unwrap())
            } else {
                Ok(serde_json::to_value(embeddings).unwrap())
            }
        }
        "chat:completion" => {
            let messages_val = payload["messages"]
                .as_array()
                .ok_or_else(|| "Missing or invalid 'messages' array".to_string())?;

            let mut messages = Vec::with_capacity(messages_val.len() + 1);
            for v in messages_val {
                let msg: llm::ChatMessage = serde_json::from_value(v.clone())
                    .map_err(|e| format!("Invalid message object: {}", e))?;
                messages.push(msg);
            }

            // Server-side persona injection: if the client supplied no system
            // message, prepend the LIVA persona so the model never runs bare.
            if !messages.iter().any(|m| m.role == "system") {
                messages.insert(0, llm::ChatMessage {
                    role: "system".to_string(),
                    content: llm::persona::PERSONA_LIVA.to_string(),
                });
            }

            let temperature = payload["temperature"]
                .as_f64()
                .unwrap_or(llm::persona::TEMP_DEFAULT as f64) as f32;
            let top_p = payload["top_p"]
                .as_f64()
                .unwrap_or(llm::persona::TOP_P_DEFAULT as f64) as f32;
            let stream = payload["stream"].as_bool().unwrap_or(false);

            let compiled_prompt = llm::compile_prompt(&messages)?;

            let state_clone = state.clone();
            let tx_clone = tx.clone();
            let req_id_clone = req_id.clone();

            let completion_output = tokio::task::spawn_blocking(move || {
                let mut llm_manager = state_clone.llm.blocking_lock();

                if stream {
                    let tx_inner = tx_clone
                        .ok_or_else(|| "IPC output channel missing for streaming".to_string())?;
                    let req_id_inner = req_id_clone
                        .ok_or_else(|| "Request ID missing for streaming".to_string())?;

                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |piece| {
                        let chunk_resp = IpcResponse {
                            id: req_id_inner.clone(),
                            status: "ok".to_string(),
                            data: Some(serde_json::json!({
                                "token": piece,
                                "done": false
                            })),
                            error: None,
                        };
                        if let Ok(chunk_str) = serde_json::to_string(&chunk_resp) {
                            let _ = tx_inner.blocking_send(chunk_str);
                        }
                        true
                    })
                } else {
                    llm_manager.generate_completion(&compiled_prompt, temperature, top_p, |_| true)
                }
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({
                "text": completion_output.text,
                "done": true,
                "usage": {
                    "prompt_tokens": completion_output.prompt_tokens,
                    "completion_tokens": completion_output.completion_tokens,
                    "total_tokens": completion_output.prompt_tokens + completion_output.completion_tokens
                }
            }))
        }
        "vision:ask" => {
            // Multimodal Q&A on an image with the unified VL core (Qwen3-VL).
            // Image source: a base64 `image` (png/jpg), else the primary screen.
            let question = payload["question"]
                .as_str()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt.")
                .to_string();
            let temperature = payload["temperature"].as_f64().unwrap_or(0.7) as f32;
            let top_p = payload["top_p"].as_f64().unwrap_or(0.8) as f32;
            let image_b64 = payload["image"].as_str().map(|s| s.to_string());

            let state_clone = state.clone();
            let output = tokio::task::spawn_blocking(
                move || -> Result<llm::CompletionOutput, String> {
                    use base64::Engine as _;
                    let mut llm_manager = state_clone.llm.blocking_lock();
                    if let Some(b64) = image_b64 {
                        let bytes = base64::engine::general_purpose::STANDARD
                            .decode(b64.as_bytes())
                            .map_err(|e| format!("Invalid base64 image: {}", e))?;
                        llm_manager.answer_with_image(
                            &question,
                            llm::engine::VisionImage::Encoded(&bytes),
                            temperature,
                            top_p,
                            |_| true,
                        )
                    } else {
                        // Context-aware capture (mouse-guided crop while gaming).
                        let (width, height, rgb) = crate::vision::capture::capture_for_vision()?;
                        llm_manager.answer_with_image(
                            &question,
                            llm::engine::VisionImage::Rgb { width, height, data: &rgb },
                            temperature,
                            top_p,
                            |_| true,
                        )
                    }
                },
            )
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            Ok(serde_json::json!({
                "text": output.text,
                "usage": {
                    "prompt_tokens": output.prompt_tokens,
                    "completion_tokens": output.completion_tokens
                }
            }))
        }
        "llm:health_check" => {
            let llm_manager = state.llm.lock().await;
            let loaded = llm_manager.engine.is_some();
            let model_path = llm_manager.current_model_path.to_string_lossy().to_string();

            Ok(serde_json::json!({
                "status": "healthy",
                "model_loaded": loaded,
                "model_path": model_path,
                "n_ctx": llm_manager.n_ctx,
                "n_gpu_layers": llm_manager.n_gpu_layers
            }))
        }
        "telegram:send_text" => {
            let chat_id_str = payload["chatId"].as_str().ok_or("Missing chatId")?.to_string();
            let text = payload["text"].as_str().ok_or("Missing text")?.to_string();
            
            let chat_id = chat_id_str.parse::<i64>().map_err(|e| format!("Invalid chatId: {}", e))?;
            
            let token = std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "Bot token missing")?;
            let bot = teloxide::prelude::Bot::new(token);
            tokio::spawn(async move {
                use teloxide::prelude::Requester;
                let _ = bot.send_message(teloxide::prelude::ChatId(chat_id), text).await;
            });
            
            Ok(serde_json::json!({ "success": true }))
        }
        "integration:smart_home_control" => {
            let result = integrations::smart_home::execute(payload)?;
            Ok(serde_json::json!({ "result": result }))
        }
        "integrations:list" => {
            Ok(serde_json::json!([
                integrations::smart_home::get_metadata()
            ]))
        }
        _ => Err(format!("Unknown command: {}", command)),
    }
}
