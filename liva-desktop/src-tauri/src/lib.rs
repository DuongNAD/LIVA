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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(InteractiveZones::default())
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            // Derive 32-byte key from password for Stronghold vault
            let mut key = vec![0u8; 32];
            for (i, b) in password.as_bytes().iter().enumerate().take(32) {
                key[i] = *b;
            }
            key
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
                loop {
                    std::thread::sleep(std::time::Duration::from_millis(30));
                    
                    if let Some(widget_window) = handle_clone.get_webview_window("widget") {
                        if let Ok(true) = widget_window.is_visible() {
                            let scale_factor = match widget_window.scale_factor() {
                                Ok(sf) => sf,
                                Err(_) => 1.0,
                            };
                            
                            let cursor_pos = match widget_window.cursor_position() {
                                Ok(pos) => pos,
                                Err(_) => continue,
                            };
                            
                            let window_pos = match widget_window.inner_position() {
                                Ok(pos) => pos,
                                Err(_) => continue,
                            };
                            
                            let rx = (cursor_pos.x - window_pos.x as f64) / scale_factor;
                            let ry = (cursor_pos.y - window_pos.y as f64) / scale_factor;
                            
                            let zones_state = handle_clone.state::<InteractiveZones>();
                            let is_inside = if let Ok(zones) = zones_state.zones.lock() {
                                zones.iter().any(|rect| {
                                    rx >= rect.x
                                        && rx <= rect.x + rect.width
                                        && ry >= rect.y
                                        && ry <= rect.y + rect.height
                                })
                            } else {
                                false
                            };
                            
                            let _ = widget_window.set_ignore_cursor_events(!is_inside);
                        }
                    }
                }
            });

            println!("✅ [LIVA Tauri] Desktop shell ready. Widget + Dashboard windows active.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![toggle_ghost_mode, update_interactive_zones, open_dashboard])
        .run(tauri::generate_context!())
        .expect("[LIVA Tauri] Fatal: Failed to start application");
}
