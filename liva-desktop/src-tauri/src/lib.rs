use tauri::Emitter;
use tauri::Manager;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use liva_native_core::{AppState, handle_command};

struct NativeCoreState(Arc<AppState>);


/// [Phase 5.1] LIVA Tauri Host — Multi-Window Desktop Shell (Optimized)
/// =========================================================
/// Architecture: Tauri (Rust) → WebView (liva-ui Vue.js)
/// 
/// Windows:
///   - widget:    Transparent overlay (3D avatar, chat bubble)
///   - dashboard: Full management UI (AI settings, avatar gallery, etc.)
///
/// Gateway: Replaced by Unified Native Engine (liva-native-core) running in-process.
///          UI communicates directly via Tauri IPC commands.

#[derive(Default)]
struct InteractiveZones {
    zones: Mutex<Vec<Rect>>,
}

#[derive(Default)]
struct EcoModeState {
    enabled: AtomicBool,
}

struct StrongholdKey(Mutex<Option<Vec<u8>>>);

#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

fn check_cursor_in_zones(rx: f64, ry: f64, zones: &[Rect]) -> (bool, f64) {
    let mut is_inside = false;
    let mut min_distance = f64::MAX;

    for rect in zones {
        let dx = if rx < rect.x {
            rect.x - rx
        } else if rx > rect.x + rect.width {
            rx - (rect.x + rect.width)
        } else {
            0.0
        };

        let dy = if ry < rect.y {
            rect.y - ry
        } else if ry > rect.y + rect.height {
            ry - (rect.y + rect.height)
        } else {
            0.0
        };

        let dist = (dx * dx + dy * dy).sqrt();
        if dist < min_distance {
            min_distance = dist;
        }
        if dx == 0.0 && dy == 0.0 {
            is_inside = true;
        }
    }

    (is_inside, min_distance)
}

#[tauri::command]
fn toggle_ghost_mode(window: tauri::Window, enabled: bool) -> Result<(), String> {
    window.set_ignore_cursor_events(enabled)
        .map_err(|e| format!("Failed to set ghost mode: {}", e))
}

#[tauri::command]
fn set_eco_mode(
    eco_state: tauri::State<'_, EcoModeState>,
    enabled: bool,
) -> Result<(), String> {
    eco_state.enabled.store(enabled, Ordering::Relaxed);
    println!("[LIVA Tauri] Eco Mode state synchronized: {}", enabled);
    Ok(())
}

#[tauri::command]
fn update_interactive_zones(
    zones_state: tauri::State<'_, InteractiveZones>,
    zones: Vec<Rect>,
) -> Result<(), String> {
    let mut current_zones = zones_state.zones.lock().map_err(|e| e.to_string())?;
    *current_zones = zones;
    Ok(())
}

#[tauri::command]
fn open_dashboard(handle: tauri::AppHandle) -> Result<(), String> {
    if let Some(dashboard) = handle.get_webview_window("dashboard") {
        dashboard.show().map_err(|e| format!("Failed to show dashboard: {}", e))?;
        dashboard.set_focus().map_err(|e| format!("Failed to focus dashboard: {}", e))?;
    } else {
        // Recreate the dashboard window dynamically if closed/destroyed
        let _ = tauri::WebviewWindowBuilder::new(
            &handle,
            "dashboard",
            tauri::WebviewUrl::App("dashboard.html".into())
        )
        .title("LIVA Dashboard")
        .inner_size(1200.0, 800.0)
        .resizable(true)
        .center()
        .build()
        .map_err(|e| format!("Failed to create dashboard window: {}", e))?;
    }
    Ok(())
}

fn get_stronghold_credentials() -> (String, Vec<u8>) {
    let password = std::env::var("LIVA_STRONGHOLD_PASSWORD")
        .unwrap_or_else(|_| "LIVA_DEFAULT_SECURE_PASSWORD".to_string());
    let salt_str = std::env::var("LIVA_STRONGHOLD_SALT")
        .unwrap_or_else(|_| "LIVA_STRONGHOLD_PERSISTENT_SALT_KEY".to_string());
    (password, salt_str.into_bytes())
}

fn get_vault_key(app: &tauri::AppHandle) -> Result<Vec<u8>, String> {
    let key_state = app.state::<StrongholdKey>();
    let mut cached_key = key_state.0.lock().map_err(|e| e.to_string())?;
    
    if let Some(ref key) = *cached_key {
        return Ok(key.clone());
    }
    
    let (password, salt) = get_stronghold_credentials();
    let mut config = argon2::Config::default();
    config.variant = argon2::Variant::Argon2id;
    config.hash_length = 32;
    
    let derived_key = argon2::hash_raw(password.as_bytes(), &salt, &config)
        .map_err(|e| format!("Failed to derive key: {}", e))?;
        
    *cached_key = Some(derived_key.clone());
    Ok(derived_key)
}

#[tauri::command]
fn read_vault_key(
    app: tauri::AppHandle,
    key: String,
) -> Result<Option<String>, String> {
    let local_data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot_path = local_data_dir.join("liva_vault.app");
    
    if !snapshot_path.exists() {
        return Ok(None);
    }
    
    let vault_key = get_vault_key(&app)?;
    let stronghold = tauri_plugin_stronghold::stronghold::Stronghold::new(&snapshot_path, vault_key)
        .map_err(|e| format!("Failed to load Stronghold: {:?}", e))?;
        
    let client_name = "liva_client";
    let client = match stronghold.get_client(client_name) {
        Ok(c) => c,
        Err(_) => {
            match stronghold.load_client(client_name) {
                Ok(c) => c,
                Err(_) => return Ok(None),
            }
        }
    };
    
    match client.store().get(key.as_bytes()) {
        Ok(Some(value_bytes)) => {
            let value_str = String::from_utf8(value_bytes).map_err(|e| e.to_string())?;
            Ok(Some(value_str))
        }
        Ok(None) => Ok(None),
        Err(e) => Err(format!("Store get failed: {:?}", e)),
    }
}

#[tauri::command]
fn write_vault_key(
    app: tauri::AppHandle,
    key: String,
    value: String,
) -> Result<(), String> {
    let local_data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot_path = local_data_dir.join("liva_vault.app");
    
    // Ensure parent directory exists
    if let Some(parent) = snapshot_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    
    let vault_key = get_vault_key(&app)?;
    let stronghold = tauri_plugin_stronghold::stronghold::Stronghold::new(&snapshot_path, vault_key)
        .map_err(|e| format!("Failed to load/create Stronghold: {:?}", e))?;
        
    let client_name = "liva_client";
    let client = match stronghold.get_client(client_name) {
        Ok(c) => c,
        Err(_) => {
            match stronghold.load_client(client_name) {
                Ok(c) => c,
                Err(_) => {
                    stronghold.create_client(client_name)
                        .map_err(|e| format!("Failed to create client: {:?}", e))?
                }
            }
        }
    };
    
    client.store().insert(key.as_bytes().to_vec(), value.as_bytes().to_vec(), None)
        .map_err(|e| format!("Store insert failed: {:?}", e))?;
        
    stronghold.save().map_err(|e| format!("Stronghold save failed: {:?}", e))?;
    
    Ok(())
}

#[tauri::command]
async fn native_ipc_call(
    state: tauri::State<'_, NativeCoreState>,
    command: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    handle_command(state.0.clone(), &command, payload, None, None).await
}

#[tauri::command]
async fn native_ipc_call_stream(
    window: tauri::Window,
    state: tauri::State<'_, NativeCoreState>,
    command: String,
    payload: serde_json::Value,
    req_id: String,
) -> Result<serde_json::Value, String> {
    let (tx, mut rx) = tokio::sync::mpsc::channel::<String>(100);
    
    let window_clone = window.clone();
    let req_id_clone = req_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&msg) {
                let _ = window_clone.emit(&format!("ipc-stream:{}", req_id_clone), resp);
            }
        }
    });

    handle_command(state.0.clone(), &command, payload, Some(tx), Some(req_id)).await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let db_path = std::env::var("LIVA_DB_PATH")
        .unwrap_or_else(|_| "data/agents/liva_core/structured_memory.sqlite".to_string());
    let encryption_key = std::env::var("LIVA_ENCRYPTION_KEY")
        .unwrap_or_else(|_| "00000000000000000000000000000000".to_string());

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let is_in_memory = std::env::var("LIVA_DB_IN_MEMORY").is_ok();
    let db = if is_in_memory {
        liva_native_core::db::DatabasePool::new_in_memory().expect("Failed to initialize in-memory DB")
    } else {
        liva_native_core::db::DatabasePool::new(&db_path).expect("Failed to initialize DatabasePool")
    };

    let (_stream, audio_handle) = match rodio::OutputStream::try_default() {
        Ok((s, h)) => (Some(s), Some(h)),
        Err(e) => {
            eprintln!("Failed to initialize default audio output stream: {}", e);
            (None, None)
        }
    };
    let sink = audio_handle.as_ref().and_then(|h| match rodio::Sink::try_new(h) {
        Ok(s) => Some(s),
        Err(e) => {
            eprintln!("Failed to create rodio Sink: {}", e);
            None
        }
    });

    let stt_model_dir = std::env::var("LIVA_STT_MODEL_DIR")
        .unwrap_or_else(|_| "models/nemotron-asr".to_string());
    let tts_model_path = std::env::var("LIVA_TTS_MODEL_PATH")
        .unwrap_or_else(|_| "models/kokoro-v1.0.onnx".to_string());
    let tts_voice_path = std::env::var("LIVA_TTS_VOICE_PATH")
        .unwrap_or_else(|_| "node_modules/kokoro-js/voices/af_heart.bin".to_string());

    let stt_manager = liva_native_core::stt::SttManager::new(&stt_model_dir);
    let shared_sink = sink.map(Arc::new);
    let tts_player = liva_native_core::tts::audio::TtsAudioPlayer::new(shared_sink.clone());
    let tts_manager = match liva_native_core::tts::TtsManager::from_bin(&tts_model_path, &tts_voice_path, shared_sink) {
        Ok(m) => Some(m),
        Err(e) => {
            eprintln!(
                "Failed to initialize TtsManager: {}. TTS commands will fail.",
                e
            );
            None
        }
    };

    let llm_n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .unwrap_or_else(|_| "4096".to_string())
        .parse::<usize>()
        .unwrap_or(4096);
    let llm_n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<u32>()
        .unwrap_or(0);
    let llm_manager = liva_native_core::llm::LlamaRouterManager::new(llm_n_ctx, llm_n_gpu_layers)
        .expect("Failed to initialize LlamaRouterManager");

    let vault_path = std::env::var("LIVA_VAULT_PATH")
        .unwrap_or_else(|_| "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault".to_string());
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(&vault_path));

    let native_capturer = Arc::new(liva_native_core::vision::capture::NativeScreenCapturer::new(0));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        native_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    let state = Arc::new(AppState {
        db,
        crypto: liva_native_core::crypto::EncryptionEngine::new(&encryption_key),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
    });

    let native_state = NativeCoreState(state);

    if let Some(s) = _stream {
        std::mem::forget(s);
    }

    tauri::Builder::default()
        .manage(native_state)
        .manage(InteractiveZones::default())
        .manage(EcoModeState::default())
        .manage(StrongholdKey(Mutex::new(None)))
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            // Derive 32-byte key from password for Stronghold vault using Argon2id
            // Persistent static salt is required so the vault can be decrypted again.
            let salt_str = std::env::var("LIVA_STRONGHOLD_SALT")
                .unwrap_or_else(|_| "LIVA_STRONGHOLD_PERSISTENT_SALT_KEY".to_string());
            let salt = salt_str.as_bytes();
            let mut config = argon2::Config::default();
            config.variant = argon2::Variant::Argon2id;
            config.hash_length = 32;
            
            argon2::hash_raw(password.as_bytes(), salt, &config)
                .expect("Failed to derive Stronghold key via Argon2id")
        }).build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Emit gateway connection info to all windows
            // Gateway is already running on port 8002 (started by start_all.ps1)
            handle.emit("gateway-ready", serde_json::json!({
                "port": 8002,
                "token": serde_json::Value::Null
            })).unwrap_or_else(|e| eprintln!("[Tauri] Failed to emit gateway-ready: {}", e));

            // Start global cursor hit-test thread for widget window
            let handle_clone = handle.clone();
            std::thread::spawn(move || {
                let mut sleep_duration = std::time::Duration::from_millis(30);
                let mut last_ignore: Option<bool> = None;
                
                // Cache scale factor and window position to prevent querying OS APIs 33 times/sec
                let mut cached_scale_factor: Option<f64> = None;
                let mut cached_window_pos: Option<tauri::PhysicalPosition<i32>> = None;
                let mut last_property_check = std::time::Instant::now();
                
                loop {
                    std::thread::sleep(sleep_duration);
                    
                    let eco_state = handle_clone.state::<EcoModeState>();
                    let is_eco = eco_state.enabled.load(Ordering::Relaxed);

                    let widget_window = match handle_clone.get_webview_window("widget") {
                        Some(w) => w,
                        None => {
                            sleep_duration = std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
                            continue;
                        }
                    };

                    if !widget_window.is_visible().unwrap_or(false) {
                        sleep_duration = std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
                        continue;
                    }

                    let now = std::time::Instant::now();
                    // Refresh cached properties every 1000ms (or 2000ms in Eco Mode)
                    let cache_ttl_ms = if is_eco { 2000 } else { 1000 };
                    if cached_scale_factor.is_none() || cached_window_pos.is_none() || now.duration_since(last_property_check).as_millis() > cache_ttl_ms {
                        cached_scale_factor = Some(widget_window.scale_factor().unwrap_or(1.0));
                        cached_window_pos = widget_window.inner_position().ok();
                        last_property_check = now;
                    }

                    let scale_factor = cached_scale_factor.unwrap_or(1.0);
                    
                    let cursor_pos = match widget_window.cursor_position() {
                        Ok(pos) => pos,
                        Err(_) => {
                            sleep_duration = std::time::Duration::from_millis(if is_eco { 1000 } else { 500 });
                            continue;
                        }
                    };
                    
                    let window_pos = match cached_window_pos {
                        Some(pos) => pos,
                        None => {
                            sleep_duration = std::time::Duration::from_millis(if is_eco { 1000 } else { 500 });
                            continue;
                        }
                    };
                    
                    let rx = (cursor_pos.x - window_pos.x as f64) / scale_factor;
                    let ry = (cursor_pos.y - window_pos.y as f64) / scale_factor;
                    
                    let zones_state = handle_clone.state::<InteractiveZones>();
                    let mut is_inside = false;
                    let mut min_distance = f64::MAX;
                    
                    if let Ok(zones) = zones_state.zones.lock() {
                        if zones.is_empty() {
                            let ignore = true;
                            if last_ignore != Some(ignore) {
                                let _ = widget_window.set_ignore_cursor_events(ignore);
                                last_ignore = Some(ignore);
                            }
                            sleep_duration = std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
                            continue;
                        }
                        let (inside, dist) = check_cursor_in_zones(rx, ry, &zones);
                        is_inside = inside;
                        min_distance = dist;
                    }
                    
                    let ignore = !is_inside;
                    if last_ignore != Some(ignore) {
                        let _ = widget_window.set_ignore_cursor_events(ignore);
                        last_ignore = Some(ignore);
                    }
                    
                    // Adjust polling interval dynamically (scaled in Eco Mode)
                    sleep_duration = if is_inside || min_distance < 50.0 {
                        std::time::Duration::from_millis(if is_eco { 100 } else { 30 })
                    } else if min_distance < 200.0 {
                        std::time::Duration::from_millis(if is_eco { 300 } else { 100 })
                    } else {
                        std::time::Duration::from_millis(if is_eco { 1000 } else { 500 })
                    };
                }
            });

            println!("✅ [LIVA Tauri] Desktop shell ready. Widget + Dashboard windows active.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            toggle_ghost_mode,
            set_eco_mode,
            update_interactive_zones,
            open_dashboard,
            read_vault_key,
            write_vault_key,
            native_ipc_call,
            native_ipc_call_stream
        ])
        .run(tauri::generate_context!())
        .expect("[LIVA Tauri] Fatal: Failed to start application");
}
