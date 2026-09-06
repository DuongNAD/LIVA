//! Challenger 2 (Gen 2): Comprehensive Empirical Verification & Stress Test Suite
//!
//! Scope:
//! - R2: Device Keystore multi-platform support (macOS/Linux HKDF-SHA256 + AES-256-GCM),
//!   0600 POSIX permissions, auto-boot without LIVA_ENCRYPTION_KEY, corruption/tampering fail-closed,
//!   concurrency race isolation.
//! - R3: Path traversal defense & default model protection for `import_avatar_folder` & `delete_avatar_model`,
//!   model type classification (Live2D vs 3D/VRM), RBAC authorization via CommandPrincipal.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::keystore::{
    self, KeyError, UNIX_SEAL_MAGIC, VAULT_SECRET_FILE,
};
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, handle_command, handle_command_as,
    resolve_and_rekey, resolve_resource_path,
};
use serde_json::json;

static TEST_ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn unique_test_dir(tag: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let dir = std::env::temp_dir().join(format!(
        "liva_ch2_test_{}_{}_{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::SeqCst),
        tag
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn create_mock_app_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("00000000000000000000000000000000");
    let stt = tokio::sync::Mutex::new(liva_native_core::stt::SttManager::new("non_existent"));
    let tts = tokio::sync::Mutex::new(None);
    let tts_player = liva_native_core::tts::audio::TtsAudioPlayer::new(None);
    let llm = tokio::sync::Mutex::new(
        liva_native_core::llm::LlamaRouterManager::new(2048, 0).expect("llm manager"),
    );
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
        "test_vault",
    ));
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        mock_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    Arc::new(AppState {
        db,
        crypto,
        stt,
        tts,
        tts_player,
        llm,
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server,
        vision: tokio::sync::Mutex::new(vision_manager),
        embedder: tokio::sync::Mutex::new(None),
    })
}

// ============================================================================
// R2 EMPIRICAL TESTS: Keystore Auto-Boot, Permissions, Corruption, Concurrency
// ============================================================================

#[test]
fn test_r2_keystore_auto_boot_without_env_key() {
    let _guard = TEST_ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::remove_var("LIVA_ENCRYPTION_KEY");
        std::env::remove_var("LIVA_ENCRYPTION_KEY_OLD");
    }

    let dir = unique_test_dir("autoboot");
    let db_path = dir.join("mem.sqlite");

    // 1. Initial boot: Generates key, creates .device_key, escrows key
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).expect("Boot must succeed without LIVA_ENCRYPTION_KEY");

        assert_eq!(bk.source, "device-key (mới)");
        assert!(bk.escrow_hex.is_some(), "New key generation must produce escrow hex");
        let escrow = bk.escrow_hex.unwrap();
        assert_eq!(escrow.len(), 64, "Key must be 64-character hex (32 bytes)");
        assert_ne!(escrow, "0".repeat(64), "Key must not be all zeros");

        let key_file = keystore::device_key_path(&db_path);
        assert!(key_file.exists(), ".device_key file must exist");

        // Verify 0600 POSIX permissions
        let mode = fs::metadata(&key_file).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, ".device_key must have mode 0600 (-rw-------)");
    }

    // 2. Second boot: Reads back existing key, no escrow, idempotent
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).expect("Subsequent boot must succeed");

        assert_eq!(bk.source, "device-key");
        assert!(bk.escrow_hex.is_none(), "Subsequent boot must not escrow");
    }

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_r2_keystore_permissions_re_enforcement_on_degradation() {
    let dir = unique_test_dir("perm_degrade");
    let db_path = dir.join("mem.sqlite");

    // Create device key and vault secret
    let (k1, gen1) = keystore::load_or_create_device_key(&db_path).unwrap();
    assert!(gen1);
    let (pw1, salt1, vgen1) = keystore::load_or_create_vault_secret(&dir).unwrap();
    assert!(vgen1);

    let key_file = keystore::device_key_path(&db_path);
    let vault_file = dir.join(VAULT_SECRET_FILE);

    // Intentionally degrade permissions to 0777 (world-readable/writable)
    fs::set_permissions(&key_file, fs::Permissions::from_mode(0o777)).unwrap();
    fs::set_permissions(&vault_file, fs::Permissions::from_mode(0o777)).unwrap();
    assert_eq!(fs::metadata(&key_file).unwrap().permissions().mode() & 0o777, 0o777);
    assert_eq!(fs::metadata(&vault_file).unwrap().permissions().mode() & 0o777, 0o777);

    // Call load_or_create again: It should automatically detect and re-enforce 0600
    let (k2, gen2) = keystore::load_or_create_device_key(&db_path).unwrap();
    assert!(!gen2);
    assert_eq!(k1, k2);

    let (pw2, salt2, vgen2) = keystore::load_or_create_vault_secret(&dir).unwrap();
    assert!(!vgen2);
    assert_eq!((pw1, salt1), (pw2, salt2));

    // Assert permissions are restored to 0600
    let key_mode = fs::metadata(&key_file).unwrap().permissions().mode() & 0o777;
    let vault_mode = fs::metadata(&vault_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(key_mode, 0o600, ".device_key permissions must be re-enforced to 0600");
    assert_eq!(vault_mode, 0o600, ".vault_secret permissions must be re-enforced to 0600");

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_r2_corrupted_keystore_safe_handling_and_no_overwrite() {
    let dir = unique_test_dir("corrupt_test");
    let db_path = dir.join("mem.sqlite");
    let key_file = keystore::device_key_path(&db_path);

    // 1. Truncated / Zero-byte file
    fs::write(&key_file, b"").unwrap();
    let res_empty = keystore::load_or_create_device_key(&db_path);
    assert!(
        matches!(res_empty, Err(KeyError::Locked(_))),
        "Empty .device_key must return KeyError::Locked"
    );
    // Crucial: Must NOT overwrite the file with a new key!
    assert_eq!(fs::read(&key_file).unwrap(), b"", "Corrupted file must not be overwritten");

    // 2. Corrupted Magic bytes
    let valid_secret = [0x99u8; 32];
    let sealed = keystore::unix_seal(&valid_secret).unwrap();
    let mut bad_magic = sealed.clone();
    bad_magic[0..5].copy_from_slice(b"BADMG");
    fs::write(&key_file, &bad_magic).unwrap();

    let res_bad_magic = keystore::load_or_create_device_key(&db_path);
    assert!(
        matches!(res_bad_magic, Err(KeyError::Locked(_))),
        "Bad magic must return KeyError::Locked"
    );
    assert_eq!(fs::read(&key_file).unwrap(), bad_magic, "File must not be overwritten on bad magic");

    // 3. Bit-flipped Ciphertext / Tag (AES-GCM Auth Failure)
    let mut tampered = sealed.clone();
    let last = tampered.len() - 1;
    tampered[last] ^= 0x01; // flip 1 bit in tag/ciphertext
    fs::write(&key_file, &tampered).unwrap();

    let res_tampered = keystore::load_or_create_device_key(&db_path);
    assert!(
        matches!(res_tampered, Err(KeyError::Locked(_))),
        "Tampered ciphertext must return KeyError::Locked"
    );
    assert_eq!(fs::read(&key_file).unwrap(), tampered, "File must not be overwritten on auth failure");

    // 4. Truncated valid blob (e.g. truncated mid-header)
    let truncated = &sealed[..UNIX_SEAL_MAGIC.len() + 8];
    fs::write(&key_file, truncated).unwrap();
    let res_trunc = keystore::load_or_create_device_key(&db_path);
    assert!(
        matches!(res_trunc, Err(KeyError::Locked(_))),
        "Truncated blob must return KeyError::Locked"
    );
    assert_eq!(fs::read(&key_file).unwrap(), truncated);

    let _ = fs::remove_dir_all(&dir);
}

#[test]
fn test_r2_keystore_concurrency_race_no_split_brain() {
    let dir = unique_test_dir("concurrency_race");
    let db_path = dir.join("mem.sqlite");

    let handles: Vec<_> = (0..16)
        .map(|_| {
            let p = db_path.clone();
            std::thread::spawn(move || {
                keystore::load_or_create_device_key(&p)
            })
        })
        .collect();

    let mut keys = Vec::new();
    for h in handles {
        let res = h.join().unwrap();
        assert!(res.is_ok(), "Concurrent load_or_create must not error");
        let (key_hex, _) = res.unwrap();
        keys.push(key_hex);
    }

    // All threads must converge on the exact same 32-byte (64 hex char) key
    let first = &keys[0];
    assert_eq!(first.len(), 64);
    for k in &keys[1..] {
        assert_eq!(k, first, "All concurrent threads must obtain the identical device key");
    }

    let _ = fs::remove_dir_all(&dir);
}

// ============================================================================
// R3 EMPIRICAL TESTS: Avatar Path Traversal & Default Model Protection
// ============================================================================

#[tokio::test]
async fn test_r3_delete_avatar_model_path_traversal_rejection() {
    let state = create_mock_app_state();

    let traversal_payloads = [
        "../secret.txt",
        "../../etc/passwd",
        "..",
        "../../../../../../../../etc/shadow",
        "sub/../model.vrm",
        "models/vrm/../secret.txt",
        "/etc/passwd",
        "/tmp/evil.vrm",
        "/var/log",
        "..\\secret.txt",
        "sub\\..\\model.vrm",
        "C:\\Windows\\System32",
        "",
        "/",
    ];

    for payload in traversal_payloads {
        let res = handle_command(
            state.clone(),
            "delete_avatar_model",
            json!({ "filename": payload }),
            None,
            None,
        )
        .await;

        assert!(
            res.is_err(),
            "delete_avatar_model MUST reject path traversal payload: '{payload}'"
        );
        let err = res.unwrap_err();
        assert!(
            err.contains("không an toàn") || err.contains("Thiếu 'filename'"),
            "Error message should mention unsafe path or missing filename: '{err}' for '{payload}'"
        );
    }
}

#[tokio::test]
async fn test_r3_delete_avatar_model_default_model_protection() {
    let state = create_mock_app_state();

    let default_model_names = [
        "default_avatar",
        "default_avatar.vrm",
        "DEFAULT_AVATAR",
        "Default_Avatar",
        "default_avatar_model",
        "pio",
        "pio/index.json",
        "pio/model3.json",
        "PIO",
        "Pio",
        "tripo_convert",
        "tripo_convert_123.fbx",
        "TRIPO_CONVERT_ABC",
        "Tripo_Convert_test",
    ];

    for name in default_model_names {
        let res = handle_command(
            state.clone(),
            "delete_avatar_model",
            json!({ "filename": name }),
            None,
            None,
        )
        .await;

        assert!(
            res.is_err(),
            "delete_avatar_model MUST protect default system model: '{name}'"
        );
        let err = res.unwrap_err();
        assert!(
            err.contains("Không thể xoá model mặc định"),
            "Error message should protect default model: '{err}' for '{name}'"
        );
    }
}

#[tokio::test]
async fn test_r3_delete_avatar_model_custom_model_lifecycle() {
    let state = create_mock_app_state();

    // 1. Prepare a custom dummy model directory in models/vrm
    let vrm_dir = resolve_resource_path("models/vrm");
    let _ = fs::create_dir_all(&vrm_dir);
    let custom_model_name = "test_custom_challenger2_model";
    let custom_model_path = vrm_dir.join(custom_model_name);
    fs::create_dir_all(&custom_model_path).unwrap();
    fs::write(custom_model_path.join("model.vrm"), b"dummy vrm binary").unwrap();

    assert!(custom_model_path.exists());

    // 2. Delete the custom model via handle_command
    let res = handle_command(
        state.clone(),
        "delete_avatar_model",
        json!({ "filename": custom_model_name }),
        None,
        None,
    )
    .await
    .expect("Deleting custom model should succeed");

    assert_eq!(res["success"], true);
    assert_eq!(res["filename"], custom_model_name);
    assert!(!custom_model_path.exists(), "Custom model directory must be removed");

    // 3. Attempting to delete again must return not found
    let res_again = handle_command(
        state.clone(),
        "delete_avatar_model",
        json!({ "filename": custom_model_name }),
        None,
        None,
    )
    .await;

    assert!(res_again.is_err(), "Deleting non-existent model must return error");
    assert!(res_again.unwrap_err().contains("Không tìm thấy model để xoá"));
}

#[tokio::test]
async fn test_r3_import_avatar_folder_validation_and_classification() {
    let state = create_mock_app_state();
    let temp_base = unique_test_dir("import_test");

    // 1. Missing folderPath payload
    let res_missing = handle_command(state.clone(), "import_avatar_folder", json!({}), None, None).await;
    assert!(res_missing.is_err());
    assert!(res_missing.unwrap_err().contains("Thiếu 'folderPath'"));

    // 2. Non-existent source path
    let res_nonexistent = handle_command(
        state.clone(),
        "import_avatar_folder",
        json!({ "folderPath": temp_base.join("does_not_exist").to_str().unwrap() }),
        None,
        None,
    )
    .await;
    assert!(res_nonexistent.is_err());
    assert!(res_nonexistent.unwrap_err().contains("không tồn tại"));

    // 3. Source path is a regular file, not a directory
    let file_path = temp_base.join("not_a_dir.txt");
    fs::write(&file_path, b"hello").unwrap();
    let res_file = handle_command(
        state.clone(),
        "import_avatar_folder",
        json!({ "folderPath": file_path.to_str().unwrap() }),
        None,
        None,
    )
    .await;
    assert!(res_file.is_err());
    assert!(res_file.unwrap_err().contains("không phải thư mục"));

    // 4. Valid Live2D folder import (contains .model3.json)
    let live2d_src = temp_base.join("my_live2d_character");
    fs::create_dir_all(&live2d_src).unwrap();
    fs::write(live2d_src.join("character.model3.json"), b"{}").unwrap();
    fs::write(live2d_src.join("character.moc3"), b"mock moc3").unwrap();

    let res_live2d = handle_command(
        state.clone(),
        "import_avatar_folder",
        json!({ "folderPath": live2d_src.to_str().unwrap() }),
        None,
        None,
    )
    .await
    .expect("Live2D import should succeed");

    assert_eq!(res_live2d["success"], true);
    assert_eq!(res_live2d["folderName"], "my_live2d_character");
    assert_eq!(res_live2d["modelType"], "2d");

    let live2d_dest = resolve_resource_path("models/live2d/my_live2d_character");
    assert!(live2d_dest.exists());
    assert!(live2d_dest.join("character.model3.json").exists());
    assert!(live2d_dest.join("character.moc3").exists());

    // 5. Valid 3D/VRM folder import
    let vrm_src = temp_base.join("my_vrm_character");
    fs::create_dir_all(&vrm_src).unwrap();
    fs::write(vrm_src.join("character.vrm"), b"mock vrm").unwrap();

    let res_vrm = handle_command(
        state.clone(),
        "import_avatar_folder",
        json!({ "folderPath": vrm_src.to_str().unwrap() }),
        None,
        None,
    )
    .await
    .expect("VRM import should succeed");

    assert_eq!(res_vrm["success"], true);
    assert_eq!(res_vrm["folderName"], "my_vrm_character");
    assert_eq!(res_vrm["modelType"], "3d");

    let vrm_dest = resolve_resource_path("models/vrm/my_vrm_character");
    assert!(vrm_dest.exists());
    assert!(vrm_dest.join("character.vrm").exists());

    // Cleanup destination folders and temp_base
    let _ = fs::remove_dir_all(&live2d_dest);
    let _ = fs::remove_dir_all(&vrm_dest);
    let _ = fs::remove_dir_all(&temp_base);
}

#[tokio::test]
async fn test_r3_command_authorization_rbac() {
    let state = create_mock_app_state();

    // Dashboard principals should be authorized
    for principal in [
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::WebSocketDashboard,
        CommandPrincipal::LocalCli,
        CommandPrincipal::Test,
    ] {
        authorize_command(principal, "import_avatar_folder").unwrap();
        authorize_command(principal, "delete_avatar_model").unwrap();
    }

    // Untrusted principals must be rejected
    for principal in [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::WebSocketWidget,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::Telegram,
    ] {
        assert!(
            authorize_command(principal, "import_avatar_folder").is_err(),
            "{principal:?} must not be authorized to import_avatar_folder"
        );
        assert!(
            authorize_command(principal, "delete_avatar_model").is_err(),
            "{principal:?} must not be authorized to delete_avatar_model"
        );

        // Also test through handle_command_as dispatcher
        let res_import = handle_command_as(
            principal,
            state.clone(),
            "import_avatar_folder",
            json!({ "folderPath": "/tmp/test" }),
            None,
            None,
        )
        .await;
        assert!(res_import.is_err());
        assert!(res_import.unwrap_err().contains("not authorized"));

        let res_delete = handle_command_as(
            principal,
            state.clone(),
            "delete_avatar_model",
            json!({ "filename": "test" }),
            None,
            None,
        )
        .await;
        assert!(res_delete.is_err());
        assert!(res_delete.unwrap_err().contains("not authorized"));
    }
}
