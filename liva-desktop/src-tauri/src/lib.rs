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

/// Nhãn salt cố định (domain-separation) khi dùng env password KHÔNG kèm
/// `LIVA_STRONGHOLD_SALT`. KHÔNG còn là bí mật (password giờ ngẫu nhiên/máy hoặc
/// do người dùng cấp) — chỉ để tách miền khoá.
const VAULT_SALT_LABEL: &[u8] = b"LIVA_STRONGHOLD_PERSISTENT_SALT_KEY";

/// Dẫn xuất khoá vault 32B từ (password, salt) bằng Argon2id.
///
/// ⚠️ KHÔNG đổi tham số Argon2 và KHÔNG bump `rust-argon2` (đang 2.1.0): vault
/// cũ được mã hoá bằng ĐÚNG `Config { variant: Argon2id, hash_length: 32,
/// ..Config::default() }` của phiên bản này. Đổi bất kỳ tham số nào = khoá tính
/// sai = KHÔNG mở lại được vault (mất API key đã lưu + hỏng đường legacy rescue).
fn derive_vault_key(password: &[u8], salt: &[u8]) -> Result<Vec<u8>, String> {
    let config = argon2::Config {
        variant: argon2::Variant::Argon2id,
        hash_length: 32,
        ..argon2::Config::default()
    };
    argon2::hash_raw(password, salt, &config).map_err(|e| format!("Argon2 fail: {}", e))
}

/// Khoá vault CŨ (hằng số hardcode công khai) — CHỈ để ĐỌC vault chưa migrate.
/// KHÔNG bao giờ là khoá GHI (song song `DEFAULT_ENCRYPTION_KEY` làm khoá phụ
/// cứu dữ liệu trong `resolve_and_rekey`). Đây là NƠI DUY NHẤT hai hằng cũ còn
/// tồn tại; xoá được ở release sau khi mọi máy dev đã migrate.
fn legacy_vault_key() -> Result<Vec<u8>, String> {
    derive_vault_key(b"LIVA_DEFAULT_SECURE_PASSWORD", VAULT_SALT_LABEL)
}

/// Fail-soft reset (quyết định người dùng): sao lưu vault + bí mật KHÔNG mở được
/// rồi để hệ thống tạo mới. Vault chứa API key NHẬP LẠI ĐƯỢC nên reset chấp nhận
/// được — KHÁC HẲN facts DB (ký ức không tái tạo → facts fail-closed + escrow).
fn fail_soft_reset_vault(dir: &std::path::Path, snapshot: &std::path::Path) {
    let stamp = std::process::id();
    if snapshot.exists() {
        let _ = std::fs::rename(snapshot, dir.join(format!("liva_vault.app.bak-{stamp}")));
    }
    let secret = dir.join(liva_native_core::keystore::VAULT_SECRET_FILE);
    if secret.exists() {
        let _ = std::fs::rename(&secret, dir.join(format!(".vault_secret.bak-{stamp}")));
    }
    liva_native_core::keystore::show_message_box(
        "LIVA — kho bí mật được đặt lại",
        "Không mở được kho API key cũ (đổi/cài lại Windows?). Kho cũ đã được sao lưu; \
         hãy nhập lại API key trong phần Cài đặt. Ký ức (facts) KHÔNG bị ảnh hưởng.",
    );
}

fn get_vault_key(app: &tauri::AppHandle) -> Result<Vec<u8>, String> {
    let key_state = app.state::<StrongholdKey>();
    let mut cached_key = key_state.0.lock().map_err(|e| e.to_string())?;
    if let Some(ref key) = *cached_key {
        return Ok(key.clone());
    }

    let dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot = dir.join("liva_vault.app");

    // Env override (giữ contract cũ lenient): PASSWORD set → dùng nó; salt từ
    // LIVA_STRONGHOLD_SALT hoặc nhãn cố định. Byte-identical với hành vi cũ nên
    // ai đặt custom password mở vault thẳng, không cần migrate.
    if let Ok(pw) = std::env::var("LIVA_STRONGHOLD_PASSWORD") {
        if !pw.is_empty() {
            let salt = std::env::var("LIVA_STRONGHOLD_SALT")
                .map(|s| s.into_bytes())
                .unwrap_or_else(|_| VAULT_SALT_LABEL.to_vec());
            let key = derive_vault_key(pw.as_bytes(), &salt)?;
            *cached_key = Some(key.clone());
            return Ok(key);
        }
    }

    // Bí mật per-machine niêm phong DPAPI (BỎ hardcode). Mất DPAPI (Locked) →
    // fail-soft reset rồi sinh lại.
    let (pw, salt, generated) =
        match liva_native_core::keystore::load_or_create_vault_secret(&dir) {
            Ok(t) => t,
            Err(liva_native_core::keystore::KeyError::Locked(_)) => {
                fail_soft_reset_vault(&dir, &snapshot);
                liva_native_core::keystore::load_or_create_vault_secret(&dir)
                    .map_err(|e| e.to_string())?
            }
            Err(e) => return Err(e.to_string()),
        };
    let key = derive_vault_key(&pw, &salt)?;

    // Vừa sinh .vault_secret mà snapshot ĐÃ tồn tại → vault cũ (khoá legacy).
    // Thử migrate lossless; lỗi bất kỳ → fail-soft reset (không kẹt boot).
    if generated && snapshot.exists() {
        if let Err(e) = migrate_legacy_vault(&snapshot, &key) {
            tracing::warn!("Migrate vault legacy thất bại ({e}) → reset an toàn");
            fail_soft_reset_vault(&dir, &snapshot);
        }
    }

    *cached_key = Some(key.clone());
    Ok(key)
}

/// Mở vault cũ bằng khoá legacy, chuyển toàn bộ key-value sang khoá mới —
/// crash-safe: ghi `.new` → verify round-trip đủ key → rename atomic, giữ
/// `.legacybak`. Vault legacy rỗng → nghi ngờ, bỏ (không ghi đè rỗng).
fn migrate_legacy_vault(snapshot: &std::path::Path, new_key: &[u8]) -> Result<(), String> {
    use tauri_plugin_stronghold::stronghold::Stronghold;

    let legacy = legacy_vault_key()?;
    let old = Stronghold::new(snapshot, legacy).map_err(|e| format!("mở vault legacy: {:?}", e))?;
    let client = old
        .load_client("liva_client")
        .map_err(|e| format!("load client legacy: {:?}", e))?;
    let store = client.store();
    let keys = store.keys().map_err(|e| format!("enumerate keys: {:?}", e))?;
    if keys.is_empty() {
        return Err("vault legacy rỗng — nghi ngờ, bỏ migrate".into());
    }
    let mut pairs: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    for k in &keys {
        if let Some(v) = store.get(k).map_err(|e| format!("get: {:?}", e))? {
            pairs.push((k.clone(), v));
        }
    }

    let new_path = snapshot.with_extension("app.new");
    let _ = std::fs::remove_file(&new_path);
    {
        let newsh =
            Stronghold::new(&new_path, new_key.to_vec()).map_err(|e| format!("tạo vault mới: {:?}", e))?;
        let nc = newsh
            .create_client("liva_client")
            .map_err(|e| format!("create client mới: {:?}", e))?;
        for (k, v) in &pairs {
            nc.store()
                .insert(k.clone(), v.clone(), None)
                .map_err(|e| format!("insert mới: {:?}", e))?;
        }
        newsh.save().map_err(|e| format!("save vault mới: {:?}", e))?;
    }
    // Verify round-trip: mở lại `.new` bằng new_key, đủ số key.
    {
        let check =
            Stronghold::new(&new_path, new_key.to_vec()).map_err(|e| format!("mở lại .new: {:?}", e))?;
        let cc = check
            .load_client("liva_client")
            .map_err(|e| format!("load .new: {:?}", e))?;
        let got = cc.store().keys().map_err(|e| format!("keys .new: {:?}", e))?;
        if got.len() != pairs.len() {
            let _ = std::fs::remove_file(&new_path);
            return Err(format!("verify .new lệch số key: {} != {}", got.len(), pairs.len()));
        }
    }
    // Atomic: giữ bản gốc tới phút chót.
    std::fs::rename(snapshot, snapshot.with_extension("app.legacybak")).map_err(|e| e.to_string())?;
    std::fs::rename(&new_path, snapshot).map_err(|e| e.to_string())?;
    Ok(())
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
    // Without a subscriber every tracing::info!/error! from liva-native-core
    // (model autoload failures included) is silently dropped.
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();

    let db_path = std::env::var("LIVA_DB_PATH")
        .unwrap_or_else(|_| "data/agents/liva_core/structured_memory.sqlite".to_string());

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    // Xem ghi chú ở liva-native-core/src/main.rs: `.is_ok()` khiến
    // `LIVA_DB_IN_MEMORY=false` bật in-memory và mất sạch dữ liệu người dùng.
    let is_in_memory = liva_native_core::env_flag("LIVA_DB_IN_MEMORY", false);
    let db = if is_in_memory {
        liva_native_core::db::DatabasePool::new_in_memory().expect("Failed to initialize in-memory DB")
    } else {
        liva_native_core::db::DatabasePool::new(&db_path).expect("Failed to initialize DatabasePool")
    };

    // BỎ KHOÁ MẶC ĐỊNH (dùng chung resolve_and_rekey với gateway): khoá thật từ
    // env → khoá thiết bị DPAPI (sinh mới nếu chưa có → escrow qua dialog vì vỏ
    // Tauri không có console); rekey facts về khoá đó (cứu dữ liệu khoá mặc định).
    let boot_crypto = match liva_native_core::resolve_and_rekey(
        &db,
        std::path::Path::new(&db_path),
        is_in_memory,
    ) {
        Ok(bk) => bk,
        Err(e) => {
            let msg = format!(
                "LIVA không thiết lập được khoá mã hoá:\n{e}\n\nNếu Windows vừa bị cài lại/đổi user, \
                 đặt biến môi trường LIVA_ENCRYPTION_KEY = khoá đã sao lưu để khôi phục dữ liệu."
            );
            liva_native_core::keystore::show_message_box("LIVA — lỗi khoá mã hoá", &msg);
            eprintln!("{msg}");
            std::process::exit(1);
        }
    };
    if let Some(hex) = &boot_crypto.escrow_hex {
        liva_native_core::keystore::show_message_box(
            "LIVA — SAO LƯU khoá mã hoá",
            &liva_native_core::escrow_message(hex),
        );
        eprint!("{}", liva_native_core::escrow_message(hex));
    }
    tracing::info!(
        "Khoá mã hoá: nguồn={}, rekey {} fact, {} bản khoá-chết",
        boot_crypto.source, boot_crypto.rekeyed, boot_crypto.locked
    );

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

    // Tauri runs with cwd = liva-desktop/src-tauri, so repo-relative model
    // paths must be resolved against the real project root.
    let stt_model_dir = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_STT_MODEL_DIR")
            .unwrap_or_else(|_| "models/nemotron-asr".to_string()),
    )
    .to_string_lossy()
    .into_owned();
    let tts_model_path = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_TTS_MODEL_PATH")
            .unwrap_or_else(|_| "models/kokoro-v1.0.onnx".to_string()),
    )
    .to_string_lossy()
    .into_owned();
    let tts_voice_path = liva_native_core::resolve_resource_path(
        &std::env::var("LIVA_TTS_VOICE_PATH")
            .unwrap_or_else(|_| "node_modules/kokoro-js/voices/af_heart.bin".to_string()),
    )
    .to_string_lossy()
    .into_owned();

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

    // Vỏ Tauri cũng nạp embedder: khác với VAD/denoise (chỉ đường WebSocket
    // tiêu thụ), bộ nhớ dài hạn đi qua chat:completion nên có tác dụng ở đây.
    let embedder = {
        let dir = liva_native_core::llm::embedder::resolve_model_dir();
        match liva_native_core::llm::embedder::EmbeddingEngine::load(&dir) {
            Ok(e) => Some(e),
            Err(e) => {
                tracing::warn!("Bo nho dai han TAT: {}", e);
                None
            }
        }
    };

    let state = Arc::new(AppState {
        db,
        crypto: boot_crypto.engine,
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: tokio::sync::Mutex::new(embedder),
    });

    // (Rekey mã hoá facts đã chạy trong resolve_and_rekey ở trên.)

    let native_state = NativeCoreState(state);

    if let Some(s) = _stream {
        std::mem::forget(s);
    }

    tauri::Builder::default()
        .manage(native_state)
        .manage(InteractiveZones::default())
        .manage(EcoModeState::default())
        .manage(StrongholdKey(Mutex::new(None)))
        // Plugin tauri_plugin_stronghold ĐÃ GỠ (H2): closure của nó là literal
        // hardcode salt cuối cùng trên write path, và UI KHÔNG import
        // @tauri-apps/plugin-stronghold (chỉ invoke command read/write_vault_key).
        // Vault vẫn dùng qua tauri_plugin_stronghold::stronghold::Stronghold trực
        // tiếp trong read_vault_key/write_vault_key, khoá lấy từ get_vault_key
        // (bí mật per-machine DPAPI, không còn hardcode).
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let handle = app.handle().clone();

            // Autoload the configured router LLM in the background so chat
            // works without a manual llm:swap_model call.
            let llm_state = app.state::<NativeCoreState>().0.clone();
            tauri::async_runtime::spawn(async move {
                liva_native_core::load_configured_router_model(llm_state, false).await;
            });

            // Game-aware GPU downshift: while a foreground game runs, reload the
            // LLM with fewer GPU layers (LIVA_GAME_N_GPU_LAYERS, default 0) to
            // hand VRAM back to the game, then restore LIVA_LLM_N_GPU_LAYERS on
            // exit. This is the desktop shell — the primary runtime while
            // gaming (embedded core + widget overlay). Reads env inside the
            // task so the 'static setup closure captures no outer locals.
            let gpu_state = app.state::<NativeCoreState>().0.clone();
            tauri::async_runtime::spawn(async move {
                let normal_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                let game_layers = std::env::var("LIVA_GAME_N_GPU_LAYERS")
                    .ok()
                    .and_then(|s| s.parse::<u32>().ok())
                    .unwrap_or(0);
                if normal_layers == 0 || game_layers == normal_layers {
                    return; // CPU-only config or no delta — nothing to downshift
                }
                let mut last_active: Option<bool> = None;
                loop {
                    let active = liva_native_core::governor::game_mode_active_now();
                    if last_active != Some(active) {
                        let target = if active { game_layers } else { normal_layers };
                        // Latch only once the model actually reached the target;
                        // if it isn't loaded yet, retry on the next poll.
                        if liva_native_core::reload_llm_gpu_layers(gpu_state.clone(), target).await {
                            last_active = Some(active);
                        }
                    }
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                }
            });

            // Game-aware CPU priority: while a foreground game runs, drop this
            // whole process to BELOW_NORMAL so the game keeps its frame time,
            // and restore NORMAL on exit. Mirrors the gateway (main.rs) exactly
            // by reusing the core Governor — same LIVA_GAME_MODE / LIVA_GAME_PRIORITY
            // switches, same transition-latched SetPriorityClass (fires only on
            // enter/leave, not every poll). Kept as its own std::thread rather
            // than folded into the GPU watcher above: that task early-returns for
            // CPU-only configs and would then skip priority management. The UI is
            // unaffected — Tauri's WebView2 renders in separate child processes
            // and DWM composites the overlay, so only host-side threads (IPC,
            // llama.cpp/STT/TTS) yield, which is exactly game mode's intent.
            let priority_governor =
                std::sync::Arc::new(liva_native_core::governor::Governor::from_env());
            std::thread::spawn(move || loop {
                let _ = priority_governor.game_mode_active();
                std::thread::sleep(std::time::Duration::from_secs(5));
            });

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
