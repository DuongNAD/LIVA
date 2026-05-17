use tauri::Emitter;

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

#[tauri::command]
fn toggle_ghost_mode(window: tauri::Window, enabled: bool) -> Result<(), String> {
    window.set_ignore_cursor_events(enabled)
        .map_err(|e| format!("Failed to set ghost mode: {}", e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
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

            println!("✅ [LIVA Tauri] Desktop shell ready. Widget + Dashboard windows active.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![toggle_ghost_mode])
        .run(tauri::generate_context!())
        .expect("[LIVA Tauri] Fatal: Failed to start application");
}
