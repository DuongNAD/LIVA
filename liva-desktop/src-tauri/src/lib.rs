use liva_native_core::{
    authorize_command, handle_command_as, websocket::WebSocketSessionAuthority,
    websocket::WebSocketSessionTicket, AppState, CommandPrincipal,
};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::OnceLock;
use tauri::Emitter;
use tauri::Manager;

struct NativeCoreState(Arc<AppState>);

#[derive(Default)]
struct WebSocketSessionState(OnceLock<WebSocketSessionAuthority>);

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
    window
        .set_ignore_cursor_events(enabled)
        .map_err(|e| format!("Failed to set ghost mode: {}", e))
}

#[tauri::command]
fn set_eco_mode(eco_state: tauri::State<'_, EcoModeState>, enabled: bool) -> Result<(), String> {
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
        dashboard
            .show()
            .map_err(|e| format!("Failed to show dashboard: {}", e))?;
        dashboard
            .set_focus()
            .map_err(|e| format!("Failed to focus dashboard: {}", e))?;
    } else {
        // Recreate the dashboard window dynamically if closed/destroyed
        let _ = tauri::WebviewWindowBuilder::new(
            &handle,
            "dashboard",
            tauri::WebviewUrl::App("dashboard.html".into()),
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

/// Mở cửa sổ chuẩn bị model (`setup.html`). Idempotent: đã mở thì đưa lên trước.
fn mo_cua_so_setup(handle: &tauri::AppHandle) -> Result<(), String> {
    if let Some(w) = handle.get_webview_window("setup") {
        let _ = w.show();
        return w.set_focus().map_err(|e| e.to_string());
    }
    tauri::WebviewWindowBuilder::new(handle, "setup", tauri::WebviewUrl::App("setup.html".into()))
        .title("LIVA — Chuẩn bị lần đầu")
        .inner_size(680.0, 620.0)
        .resizable(true)
        .center()
        .build()
        .map(|_| ())
        .map_err(|e| format!("Không mở được cửa sổ thiết lập: {e}"))
}

#[tauri::command]
fn open_setup(handle: tauri::AppHandle) -> Result<(), String> {
    mo_cua_so_setup(&handle)
}

/// Còn thiếu model **bắt buộc** không? `false` khi không đọc nổi danh sách —
/// một cửa sổ thiết lập bật lên vì lỗi nội bộ còn khó hiểu hơn là không bật.
fn thieu_model_bat_buoc() -> bool {
    match liva_native_core::setup::load_manifest() {
        Ok(m) => {
            let st = liva_native_core::setup::status(
                &m,
                "minimal",
                &liva_native_core::configured_models_dir(),
                &liva_native_core::resource_write_root(),
            );
            st.blocking
        }
        Err(e) => {
            tracing::warn!("Không đọc được danh sách model ({e}) — bỏ qua màn hình thiết lập");
            false
        }
    }
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

/// H5: thoát với DIALOG lỗi thay vì panic im lặng. Boot fail (thiếu vec0.dll,
/// DB hỏng hoặc không mở được khoá thiết bị) trước đây `.expect()` → vỏ Tauri panic mà người
/// dùng chỉ thấy "app không mở". Nay hiện MessageBox có hướng khắc phục
/// (`db_error_hint` dùng chung với gateway) rồi thoát sạch.
fn die_tauri_boot(context: &str, err: impl std::fmt::Display) -> ! {
    let e = err.to_string();
    let hint = liva_native_core::db_error_hint(&e);
    // `BootError::db` đã gắn hint vào context để gateway standalone cũng nhận
    // được. Không lặp lại cùng đoạn trong dialog Tauri.
    let suffix = if hint.is_empty() || context.contains(hint.trim()) {
        ""
    } else {
        hint
    };
    let msg = format!("LIVA không khởi động được.\n\n{context}:\n{e}{}", suffix);
    liva_native_core::keystore::show_message_box("LIVA — lỗi khởi động", &msg);
    eprintln!("{msg}");
    std::process::exit(1);
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
    let (pw, salt, generated) = match liva_native_core::keystore::load_or_create_vault_secret(&dir)
    {
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
    let keys = store
        .keys()
        .map_err(|e| format!("enumerate keys: {:?}", e))?;
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
        let newsh = Stronghold::new(&new_path, new_key.to_vec())
            .map_err(|e| format!("tạo vault mới: {:?}", e))?;
        let nc = newsh
            .create_client("liva_client")
            .map_err(|e| format!("create client mới: {:?}", e))?;
        for (k, v) in &pairs {
            nc.store()
                .insert(k.clone(), v.clone(), None)
                .map_err(|e| format!("insert mới: {:?}", e))?;
        }
        newsh
            .save()
            .map_err(|e| format!("save vault mới: {:?}", e))?;
    }
    // Verify round-trip: mở lại `.new` bằng new_key, đủ số key.
    {
        let check = Stronghold::new(&new_path, new_key.to_vec())
            .map_err(|e| format!("mở lại .new: {:?}", e))?;
        let cc = check
            .load_client("liva_client")
            .map_err(|e| format!("load .new: {:?}", e))?;
        let got = cc
            .store()
            .keys()
            .map_err(|e| format!("keys .new: {:?}", e))?;
        if got.len() != pairs.len() {
            let _ = std::fs::remove_file(&new_path);
            return Err(format!(
                "verify .new lệch số key: {} != {}",
                got.len(),
                pairs.len()
            ));
        }
    }
    // Atomic: giữ bản gốc tới phút chót.
    std::fs::rename(snapshot, snapshot.with_extension("app.legacybak"))
        .map_err(|e| e.to_string())?;
    std::fs::rename(&new_path, snapshot).map_err(|e| e.to_string())?;
    Ok(())
}

const VAULT_SECRET_KEYS: &[&str] = &[
    "ai/cloud_api_key",
    "search/tavily_api_key",
    "weather/api_key",
    "telegram/bot_token",
    "zalo/access_token",
    "zalo/app_secret",
    "email/password",
    "google/client_secret",
];
const MAX_VAULT_SECRET_BYTES: usize = 16 * 1024;

fn validate_vault_secret_key(key: &str) -> Result<(), String> {
    if !VAULT_SECRET_KEYS.contains(&key) {
        return Err("vault secret key is not allowed".to_string());
    }
    Ok(())
}

fn validate_vault_secret_input(key: &str, value: &str) -> Result<(), String> {
    validate_vault_secret_key(key)?;
    if value.is_empty() {
        return Err("vault secret value must not be empty".to_string());
    }
    if value.len() > MAX_VAULT_SECRET_BYTES {
        return Err(format!(
            "vault secret exceeds {MAX_VAULT_SECRET_BYTES} bytes"
        ));
    }
    Ok(())
}

fn read_vault_key(app: &tauri::AppHandle, key: &str) -> Result<Option<String>, String> {
    validate_vault_secret_key(key)?;
    let local_data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot_path = local_data_dir.join("liva_vault.app");

    if !snapshot_path.exists() {
        return Ok(None);
    }

    let vault_key = get_vault_key(app)?;
    let stronghold =
        tauri_plugin_stronghold::stronghold::Stronghold::new(&snapshot_path, vault_key)
            .map_err(|e| format!("Failed to load Stronghold: {:?}", e))?;

    let client_name = "liva_client";
    let client = match stronghold.get_client(client_name) {
        Ok(c) => c,
        Err(_) => match stronghold.load_client(client_name) {
            Ok(c) => c,
            Err(_) => return Ok(None),
        },
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

fn write_vault_key(app: &tauri::AppHandle, key: &str, value: &str) -> Result<(), String> {
    validate_vault_secret_input(key, value)?;
    let local_data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot_path = local_data_dir.join("liva_vault.app");

    // Ensure parent directory exists
    if let Some(parent) = snapshot_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    let vault_key = get_vault_key(app)?;
    let stronghold =
        tauri_plugin_stronghold::stronghold::Stronghold::new(&snapshot_path, vault_key)
            .map_err(|e| format!("Failed to load/create Stronghold: {:?}", e))?;

    let client_name = "liva_client";
    let client = match stronghold.get_client(client_name) {
        Ok(c) => c,
        Err(_) => match stronghold.load_client(client_name) {
            Ok(c) => c,
            Err(_) => stronghold
                .create_client(client_name)
                .map_err(|e| format!("Failed to create client: {:?}", e))?,
        },
    };

    client
        .store()
        .insert(key.as_bytes().to_vec(), value.as_bytes().to_vec(), None)
        .map_err(|e| format!("Store insert failed: {:?}", e))?;

    stronghold
        .save()
        .map_err(|e| format!("Stronghold save failed: {:?}", e))?;

    Ok(())
}

fn delete_vault_key(app: &tauri::AppHandle, key: &str) -> Result<(), String> {
    validate_vault_secret_key(key)?;
    let local_data_dir = app.path().app_local_data_dir().map_err(|e| e.to_string())?;
    let snapshot_path = local_data_dir.join("liva_vault.app");
    if !snapshot_path.exists() {
        return Ok(());
    }

    let vault_key = get_vault_key(app)?;
    let stronghold =
        tauri_plugin_stronghold::stronghold::Stronghold::new(&snapshot_path, vault_key)
            .map_err(|e| format!("Failed to load Stronghold: {:?}", e))?;
    let client = match stronghold.get_client("liva_client") {
        Ok(client) => client,
        Err(_) => match stronghold.load_client("liva_client") {
            Ok(client) => client,
            Err(_) => return Ok(()),
        },
    };

    client
        .store()
        .delete(key.as_bytes())
        .map_err(|e| format!("Store delete failed: {:?}", e))?;
    stronghold
        .save()
        .map_err(|e| format!("Stronghold save failed: {:?}", e))
}

#[tauri::command]
fn vault_secret_present(app: tauri::AppHandle, key: String) -> Result<bool, String> {
    Ok(read_vault_key(&app, &key)?.is_some())
}

#[tauri::command]
fn store_vault_secret(app: tauri::AppHandle, key: String, value: String) -> Result<(), String> {
    write_vault_key(&app, &key, &value)
}

#[tauri::command]
fn delete_vault_secret(app: tauri::AppHandle, key: String) -> Result<(), String> {
    delete_vault_key(&app, &key)
}

fn authorize_tauri_principal(
    window_label: &str,
    command: &str,
) -> Result<CommandPrincipal, String> {
    let principal = match window_label {
        "widget" => CommandPrincipal::TauriWidget,
        "dashboard" => CommandPrincipal::TauriDashboard,
        "setup" => CommandPrincipal::TauriSetup,
        _ => {
            return Err(format!("Cửa sổ Tauri không được cấp quyền: {window_label}"));
        }
    };

    authorize_command(principal, command)?;
    Ok(principal)
}

fn websocket_principal_for_window(window_label: &str) -> Result<CommandPrincipal, String> {
    match window_label {
        "widget" => Ok(CommandPrincipal::WebSocketWidget),
        "dashboard" => Ok(CommandPrincipal::WebSocketDashboard),
        _ => Err(format!(
            "Cửa sổ Tauri không được cấp WebSocket session: {window_label}"
        )),
    }
}

#[tauri::command]
fn issue_websocket_session(
    window: tauri::Window,
    state: tauri::State<'_, WebSocketSessionState>,
) -> Result<WebSocketSessionTicket, String> {
    let principal = websocket_principal_for_window(window.label())?;
    let sessions = state
        .0
        .get()
        .ok_or_else(|| "WebSocket session authority chưa sẵn sàng".to_string())?;
    sessions.issue(principal)
}

#[tauri::command]
async fn native_ipc_call(
    window: tauri::Window,
    state: tauri::State<'_, NativeCoreState>,
    command: String,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let principal = authorize_tauri_principal(window.label(), &command)?;
    handle_command_as(principal, state.0.clone(), &command, payload, None, None).await
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
    let principal = authorize_tauri_principal(window.label(), &command)?;

    let window_clone = window.clone();
    let req_id_clone = req_id.clone();
    tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            if let Ok(resp) = serde_json::from_str::<serde_json::Value>(&msg) {
                let _ = window_clone.emit(&format!("ipc-stream:{}", req_id_clone), resp);
            }
        }
    });

    handle_command_as(
        principal,
        state.0.clone(),
        &command,
        payload,
        Some(tx),
        Some(req_id),
    )
    .await
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Without a subscriber every tracing::info!/error! from liva-native-core
    // (model autoload failures included) is silently dropped.
    //
    // Filter đọc từ `RUST_LOG` bằng chính sách dùng chung với gateway
    // (`liva_native_core::tracing_env_filter`) để hai vỏ không trôi dạt. Trước
    // đây là `.with_max_level(Level::INFO)` cứng ⇒ `RUST_LOG` vô tác dụng và
    // mọi `debug!` không bao giờ hiện. Mặc định vẫn `info`.
    let _ = tracing_subscriber::fmt()
        .with_env_filter(liva_native_core::tracing_env_filter())
        .try_init();

    // Dựng AppState bằng đường DÙNG CHUNG với gateway (`boot::build_app_state`).
    // Trước đây khối này là ~155 dòng chép gần nguyên từ
    // liva-native-core/src/main.rs, và hai bản sao đã trôi lệch — xem bảng ở
    // đầu `liva-native-core/src/boot.rs`.
    let boot = liva_native_core::boot::build_app_state()
        .unwrap_or_else(|e| die_tauri_boot(&e.context, e.detail));
    let liva_native_core::boot::Boot {
        state,
        escrow_hex,
        crypto_source,
        crypto_rekeyed,
        crypto_locked,
        audio_stream,
        llm_n_gpu_layers,
    } = boot;

    if let Some(hex) = &escrow_hex {
        // Vỏ Tauri không có console ⇒ escrow phải qua hộp thoại, nếu không
        // người dùng mất khoá mà không hề biết mình vừa được đưa cho một khoá.
        liva_native_core::keystore::show_message_box(
            "LIVA — SAO LƯU khoá mã hoá",
            &liva_native_core::escrow_message(hex),
        );
    }
    tracing::info!(
        "Khoá mã hoá: nguồn={}, rekey {} fact, {} bản khoá-chết",
        crypto_source,
        crypto_rekeyed,
        crypto_locked
    );

    let native_state = NativeCoreState(state);

    // Giữ OutputStream sống suốt đời tiến trình — drop nó là LIVA câm.
    if let Some(s) = audio_stream {
        std::mem::forget(s);
    }

    tauri::Builder::default()
        .manage(native_state)
        .manage(InteractiveZones::default())
        .manage(EcoModeState::default())
        .manage(StrongholdKey(Mutex::new(None)))
        .manage(WebSocketSessionState::default())
        // Plugin tauri_plugin_stronghold ĐÃ GỠ (H2): closure của nó là literal
        // hardcode salt cuối cùng trên write path, và UI KHÔNG import
        // @tauri-apps/plugin-stronghold (renderer chỉ invoke present/store/delete).
        // Vault vẫn dùng qua tauri_plugin_stronghold::stronghold::Stronghold trực
        // tiếp trong helper vault private; khoá lấy từ get_vault_key
        // (bí mật per-machine DPAPI, không còn hardcode).
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        // `move`: closure setup là `'static`, nên `llm_n_gpu_layers` (lấy từ
        // `Boot`) phải được CHUYỂN vào chứ không mượn. Đây cũng là lý do bản cũ
        // tự đọc lại env trong task thay vì dùng giá trị đã parse lúc boot —
        // nay giá trị đi thẳng từ chỗ dựng LLM, không còn đường lệch.
        .setup(move |app| {
            let handle = app.handle().clone();

            // Dịch vụ nền DÙNG CHUNG với gateway: WebSocket, tự nạp model,
            // phóng chiếu bộ nhớ, hạ lớp GPU khi chơi game, giải phóng TTS lúc
            // rảnh, bot Telegram, governor ưu tiên CPU. Danh sách sống ở
            // `liva_native_core::boot` để thêm dịch vụ mới không còn phải nhớ
            // sửa hai chỗ — trước đây app desktop THIẾU hai dịch vụ cuối.
            //
            // `ipc_tx: None` là khác biệt thật: vỏ Tauri không có stdout IPC để
            // bot Telegram ghi vào (bot vẫn chạy đủ, chỉ mất kênh phụ đó).
            let services_state = app.state::<NativeCoreState>().0.clone();
            let ready_handle = handle.clone();
            let session_handle = handle.clone();
            // `spawn_background_services` gọi `tokio::spawn` bên trong, nhưng
            // closure `.setup()` của Tauri chạy NGOÀI runtime Tokio — gọi trực
            // tiếp sẽ panic ngay lúc khởi động:
            //
            //   memory_consolidation.rs:41: there is no reactor running,
            //   must be called from the context of a Tokio 1.x runtime
            //
            // `main.rs` không gặp vì nó nằm trong `#[tokio::main]`. Đây đúng
            // loại khác biệt giữa hai vỏ mà `boot.rs` sinh ra để xoá, nên vào
            // ngữ cảnh runtime của Tauri thay vì nhân đôi danh sách dịch vụ.
            //
            // Guard phải sống hết closure: mọi `tokio::spawn` bên dưới đều cần
            // nó, kể cả các dịch vụ thêm về sau.
            let _rt_guard = tauri::async_runtime::handle().inner().enter();
            let _services = liva_native_core::boot::spawn_background_services(
                services_state,
                liva_native_core::boot::ServiceOptions {
                    ipc_tx: None,
                    on_gateway_ready: Some(Box::new(move |address| {
                        if let Err(error) = ready_handle.emit(
                            "gateway-ready",
                            serde_json::json!({
                                "port": address.port(),
                                "token": serde_json::Value::Null
                            }),
                        ) {
                            tracing::error!("Không emit được gateway-ready: {error}");
                        }
                    })),
                    on_websocket_sessions_ready: Some(Box::new(move |sessions| {
                        let state = session_handle.state::<WebSocketSessionState>();
                        if state.0.set(sessions).is_err() {
                            tracing::error!(
                                "WebSocket session authority đã được khởi tạo trước đó"
                            );
                        }
                    })),
                    llm_n_gpu_layers,
                },
            );

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
                            sleep_duration =
                                std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
                            continue;
                        }
                    };

                    if !widget_window.is_visible().unwrap_or(false) {
                        sleep_duration =
                            std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
                        continue;
                    }

                    let now = std::time::Instant::now();
                    // Refresh cached properties every 1000ms (or 2000ms in Eco Mode)
                    let cache_ttl_ms = if is_eco { 2000 } else { 1000 };
                    if cached_scale_factor.is_none()
                        || cached_window_pos.is_none()
                        || now.duration_since(last_property_check).as_millis() > cache_ttl_ms
                    {
                        cached_scale_factor = Some(widget_window.scale_factor().unwrap_or(1.0));
                        cached_window_pos = widget_window.inner_position().ok();
                        last_property_check = now;
                    }

                    let scale_factor = cached_scale_factor.unwrap_or(1.0);

                    let cursor_pos = match widget_window.cursor_position() {
                        Ok(pos) => pos,
                        Err(_) => {
                            sleep_duration =
                                std::time::Duration::from_millis(if is_eco { 1000 } else { 500 });
                            continue;
                        }
                    };

                    let window_pos = match cached_window_pos {
                        Some(pos) => pos,
                        None => {
                            sleep_duration =
                                std::time::Duration::from_millis(if is_eco { 1000 } else { 500 });
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
                            sleep_duration =
                                std::time::Duration::from_millis(if is_eco { 2000 } else { 1000 });
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

            // Lần chạy đầu của một bản CÀI: model bị gitignore và nặng ~3,7 GB
            // nên không nằm trong bộ cài. Không có màn hình này thì LIVA mở lên
            // trông như chạy bình thường nhưng không nghe, không nói, không nhớ
            // — và người dùng không có cách nào biết vì sao, vì cách sửa duy
            // nhất trước đây là một script Node trong cây mã nguồn.
            // (`liva-desktop` là edition 2021 — không có let-chains như lõi.)
            if thieu_model_bat_buoc() {
                if let Err(e) = mo_cua_so_setup(&handle) {
                    tracing::error!("{e}");
                }
            }

            println!("✅ [LIVA Tauri] Desktop shell ready. Widget + Dashboard windows active.");
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            toggle_ghost_mode,
            set_eco_mode,
            update_interactive_zones,
            open_dashboard,
            open_setup,
            vault_secret_present,
            store_vault_secret,
            delete_vault_secret,
            issue_websocket_session,
            native_ipc_call,
            native_ipc_call_stream
        ])
        .run(tauri::generate_context!())
        .expect("[LIVA Tauri] Fatal: Failed to start application");
}

#[cfg(test)]
mod h2_migration_tests {
    use super::{
        authorize_tauri_principal, derive_vault_key, legacy_vault_key, migrate_legacy_vault,
        validate_vault_secret_input, websocket_principal_for_window,
    };
    use liva_native_core::CommandPrincipal;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tauri_plugin_stronghold::stronghold::Stronghold;

    fn tmp_dir() -> std::path::PathBuf {
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "liva_vault_test_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// E2E H2: vault CŨ mã bằng cred hardcode legacy có API key → migrate sang
    /// khoá mới → API KEY CÒN NGUYÊN dưới khoá mới, bản gốc giữ ở .legacybak, và
    /// khoá legacy hết mở được. Dùng Stronghold trực tiếp (không cần Tauri runtime).
    #[test]
    fn migrate_legacy_vault_bao_toan_api_key() {
        let dir = tmp_dir();
        let snapshot = dir.join("liva_vault.app");

        // 1. Dựng vault LEGACY (khoá hardcode cũ) có 2 API key.
        {
            let old =
                Stronghold::new(&snapshot, legacy_vault_key().unwrap()).expect("tạo vault legacy");
            let c = old.create_client("liva_client").expect("create client");
            c.store()
                .insert(b"OPENAI_KEY".to_vec(), b"sk-abc123".to_vec(), None)
                .unwrap();
            c.store()
                .insert(b"ZALO_TOKEN".to_vec(), b"zalo-xyz".to_vec(), None)
                .unwrap();
            old.save().expect("save vault legacy");
        }

        // 2. Khoá MỚI (mô phỏng bí mật per-machine).
        let new_key =
            derive_vault_key(b"khoa-moi-per-machine-1234567890", b"salt-moi-16bytes").unwrap();

        // 3. Migrate.
        migrate_legacy_vault(&snapshot, &new_key).expect("migrate phải thành công");

        // 4. Mở bằng khoá MỚI → 2 API key còn nguyên.
        {
            let sh = Stronghold::new(&snapshot, new_key.clone()).expect("mở bằng khoá mới");
            let c = sh.load_client("liva_client").expect("load client mới");
            assert_eq!(
                c.store().get(b"OPENAI_KEY").unwrap(),
                Some(b"sk-abc123".to_vec()),
                "API key phải được bảo toàn qua migrate"
            );
            assert_eq!(
                c.store().get(b"ZALO_TOKEN").unwrap(),
                Some(b"zalo-xyz".to_vec())
            );
        }

        // 5. Bản gốc giữ ở .legacybak; khoá legacy KHÔNG còn mở snapshot mới.
        assert!(
            snapshot.with_extension("app.legacybak").exists(),
            "bản gốc phải được giữ ở .legacybak"
        );
        // Khoá legacy phải hết mở được: Stronghold từ chối khoá sai ngay ở new()
        // (BadFileKey) hoặc muộn nhất ở load_client — bắt cả hai.
        let legacy_fails = match Stronghold::new(&snapshot, legacy_vault_key().unwrap()) {
            Err(_) => true,
            Ok(sh) => sh.load_client("liva_client").is_err(),
        };
        assert!(
            legacy_fails,
            "khoá legacy phải hết mở được sau khi re-encrypt"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Vault legacy RỖNG → migrate NGHI NGỜ, trả Err (không ghi đè vault rỗng).
    #[test]
    fn migrate_vault_legacy_rong_thi_bo_qua() {
        let dir = tmp_dir();
        let snapshot = dir.join("liva_vault.app");
        {
            let old = Stronghold::new(&snapshot, legacy_vault_key().unwrap()).unwrap();
            old.create_client("liva_client").unwrap();
            old.save().unwrap();
        }
        let new_key =
            derive_vault_key(b"khoa-moi-1234567890123456789012", b"salt16byteslong!").unwrap();
        assert!(
            migrate_legacy_vault(&snapshot, &new_key).is_err(),
            "vault rỗng phải bị coi là nghi ngờ, không migrate"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn vault_chi_nhan_namespace_secret_da_phe_duyet() {
        for key in [
            "ai/cloud_api_key",
            "search/tavily_api_key",
            "weather/api_key",
            "telegram/bot_token",
            "zalo/access_token",
            "zalo/app_secret",
            "email/password",
            "google/client_secret",
        ] {
            validate_vault_secret_input(key, "secret").expect(key);
        }

        assert!(validate_vault_secret_input("../escape", "secret").is_err());
        assert!(validate_vault_secret_input("arbitrary/key", "secret").is_err());
        assert!(validate_vault_secret_input("ai/cloud_api_key", "").is_err());
        assert!(validate_vault_secret_input("ai/cloud_api_key", &"x".repeat(16_385)).is_err());
    }

    #[test]
    fn window_label_anh_xa_sang_principal_chinh_xac() {
        assert_eq!(
            authorize_tauri_principal("widget", "ping").unwrap(),
            CommandPrincipal::TauriWidget
        );
        assert_eq!(
            authorize_tauri_principal("dashboard", "update_config").unwrap(),
            CommandPrincipal::TauriDashboard
        );
        assert_eq!(
            authorize_tauri_principal("setup", "setup:fetch").unwrap(),
            CommandPrincipal::TauriSetup
        );
    }

    #[test]
    fn tauri_tu_choi_label_la_va_lenh_vuot_quyen() {
        assert!(authorize_tauri_principal("unknown", "ping").is_err());
        assert!(authorize_tauri_principal("widget", "update_config").is_err());
        assert!(authorize_tauri_principal("setup", "get_memory_data").is_err());
        assert!(authorize_tauri_principal("dashboard", "mcp:call_tool").is_err());
    }

    #[test]
    fn chi_widget_va_dashboard_duoc_cap_websocket_session() {
        assert_eq!(
            websocket_principal_for_window("widget").unwrap(),
            CommandPrincipal::WebSocketWidget
        );
        assert_eq!(
            websocket_principal_for_window("dashboard").unwrap(),
            CommandPrincipal::WebSocketDashboard
        );
        assert!(websocket_principal_for_window("setup").is_err());
        assert!(websocket_principal_for_window("unknown").is_err());
    }

    #[test]
    fn recovery_key_khong_duoc_ghi_ra_stderr() {
        let source = include_str!("lib.rs");
        let forbidden = [
            "eprint!",
            "(\"{}\", liva_native_core::",
            "escrow_message(hex))",
        ]
        .concat();
        assert!(
            !source.contains(&forbidden),
            "recovery key chỉ được giao one-time qua local secure dialog"
        );
    }
}
