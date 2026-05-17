use std::process::{Command, Stdio};
use std::io::{BufReader, BufRead};
use std::thread;
use tauri::Emitter;

// Learn more about Tauri commands at https://tauri.app/develop/calling-rust/
#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_stronghold::Builder::new(|password| {
            let mut key = vec![0u8; 32];
            for (i, b) in password.as_bytes().iter().enumerate().take(32) {
                if i < 32 { key[i] = *b; }
            }
            key
        }).build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_handle = app.handle().clone();
            
            // [Phase 5.1] Inversion of Control: Host (Tauri) nắm Master Key
            // Ghi chú: Trong thực tế sẽ lấy từ OS Keyring/Stronghold. Tạm thời mock 32-bytes.
            let master_key = "12345678901234567890123456789012"; 
            
            println!("[Tauri] Đang khởi động Gateway Sidecar...");
            
            // Spawn Gateway Process (using tsx for dev, node for prod)
            #[cfg(debug_assertions)]
            let mut gateway = Command::new("npx.cmd")
                .arg("tsx")
                .arg("src/Gateway.ts")
                .current_dir("../../openclaw-gateway")
                .env("LIVA_ENCRYPTION_KEY", master_key)
                .env("LIVA_VAULT_PATH", "../data/liva_vault.json")
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to spawn Gateway");

            #[cfg(not(debug_assertions))]
            let mut gateway = Command::new("node")
                .arg("Gateway.js")
                .env("LIVA_ENCRYPTION_KEY", master_key)
                .env("LIVA_VAULT_PATH", "../../data/liva_vault.json")
                .stdout(Stdio::piped())
                .spawn()
                .expect("Failed to spawn Gateway");

            // Capture Zero-Trust Handshake từ Stdout
            let stdout = gateway.stdout.take().expect("Failed to open stdout");
            let mut reader = BufReader::new(stdout);
            
            thread::spawn(move || {
                let mut line = String::new();
                if let Ok(bytes) = reader.read_line(&mut line) {
                    if bytes > 0 {
                        // Phân tích cú pháp JSON
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&line) {
                            if json["event"] == "GATEWAY_READY" {
                                let port = json["port"].as_u64().unwrap_or(0) as u16;
                                let token = json["token"].as_str().map(|s| s.to_string());
                                
                                println!("✅ [Zero-Trust Handshake] Nhận tín hiệu thành công! Port: {}, Token: {:?}", port, token);
                                
                                // Bắn Event xuyên suốt cầu nối (Platform Bridge) xuống Vue.js
                                app_handle.emit("gateway-ready", serde_json::json!({
                                    "port": port,
                                    "token": token
                                })).unwrap_or_else(|e| println!("Failed to emit gateway-ready: {}", e));
                            }
                        }
                    }
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
