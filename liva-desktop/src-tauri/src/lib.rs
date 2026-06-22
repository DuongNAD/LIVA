use tauri::Emitter;
use tauri::Manager;
use std::sync::Mutex;

/// [Phase 5.1] LIVA Tauri Host — Multi-Window Desktop Shell
/// =========================================================
/// Architecture: Tauri (Rust) → WebView (liva-ui Vue.js)
/// 
/// Windows:
///   - widget:    Transparent overlay (3D avatar, chat bubble)
///   - dashboard: Full management UI (AI settings, avatar gallery, etc.)
///
/// Gateway: Launched externally by start_all.ps1/bat. 
///          UI connects via WebSocket (port 8082) through useGateway.ts.

#[derive(Default)]
struct InteractiveZones {
    zones: Mutex<Vec<Rect>>,
}



#[derive(serde::Deserialize, serde::Serialize, Clone, Debug)]
struct Rect {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[tauri::command]
fn toggle_ghost_mode(window: tauri::Window, enabled: bool) -> Result<(), String> {
    window.set_ignore_cursor_events(enabled)
        .map_err(|e| format!("Failed to set ghost mode: {}", e))
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

fn get_vault_key() -> Result<Vec<u8>, String> {
    let password = "LIVA_DEFAULT_SECURE_PASSWORD";
    let salt = b"LIVA_STRONGHOLD_PERSISTENT_SALT_KEY";
    let mut config = argon2::Config::default();
    config.variant = argon2::Variant::Argon2id;
    config.hash_length = 32;
    
    argon2::hash_raw(password.as_bytes(), salt, &config)
        .map_err(|e| format!("Failed to derive key: {}", e))
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
    
    let vault_key = get_vault_key()?;
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
    
    let vault_key = get_vault_key()?;
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(InteractiveZones::default())
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            // Derive 32-byte key from password for Stronghold vault using Argon2id
            // Persistent static salt is required so the vault can be decrypted again.
            let salt = b"LIVA_STRONGHOLD_PERSISTENT_SALT_KEY";
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
            // Gateway is already running on port 8082 (started by start_all.ps1)
            handle.emit("gateway-ready", serde_json::json!({
                "port": 8082,
                "token": serde_json::Value::Null
            })).unwrap_or_else(|e| eprintln!("[Tauri] Failed to emit gateway-ready: {}", e));

            // Start global cursor hit-test thread for widget window
            let handle_clone = handle.clone();
            std::thread::spawn(move || {
                let mut sleep_duration = std::time::Duration::from_millis(30);
                loop {
                    std::thread::sleep(sleep_duration);
                    
                    if let Some(widget_window) = handle_clone.get_webview_window("widget") {
                        if let Ok(true) = widget_window.is_visible() {
                            let scale_factor = widget_window.scale_factor().unwrap_or(1.0);
                            
                            let cursor_pos = match widget_window.cursor_position() {
                                Ok(pos) => pos,
                                Err(_) => {
                                    sleep_duration = std::time::Duration::from_millis(500);
                                    continue;
                                }
                            };
                            
                            let window_pos = match widget_window.inner_position() {
                                Ok(pos) => pos,
                                Err(_) => {
                                    sleep_duration = std::time::Duration::from_millis(500);
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
                                    let _ = widget_window.set_ignore_cursor_events(true);
                                    sleep_duration = std::time::Duration::from_millis(1000);
                                    continue;
                                }
                                for rect in zones.iter() {
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
                                    
                                    let dist = (dx*dx + dy*dy).sqrt();
                                    if dist < min_distance {
                                        min_distance = dist;
                                    }
                                    if dx == 0.0 && dy == 0.0 {
                                        is_inside = true;
                                    }
                                }
                            }
                            
                            let _ = widget_window.set_ignore_cursor_events(!is_inside);
                            
                            // Adjust polling interval dynamically
                            sleep_duration = if is_inside || min_distance < 50.0 {
                                std::time::Duration::from_millis(30)
                            } else if min_distance < 200.0 {
                                std::time::Duration::from_millis(100)
                            } else {
                                std::time::Duration::from_millis(500)
                            };
                        } else {
                            sleep_duration = std::time::Duration::from_millis(1000);
                        }
                    } else {
                        sleep_duration = std::time::Duration::from_millis(1000);
                    }
                }
            });

            println!("✅ [LIVA Tauri] Desktop shell ready. Widget + Dashboard windows active.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            toggle_ghost_mode,
            update_interactive_zones,
            open_dashboard,
            read_vault_key,
            write_vault_key
        ])
        .run(tauri::generate_context!())
        .expect("[LIVA Tauri] Fatal: Failed to start application");
}
