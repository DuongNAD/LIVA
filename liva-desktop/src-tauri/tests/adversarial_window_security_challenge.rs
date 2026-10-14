use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{authorize_command, handle_command_as, AppState, CommandPrincipal};
use liva_native_core::{db, stt, tts};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("Cannot read {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("Invalid JSON {}: {error}", path.display()))
}

fn capability(name: &str) -> Value {
    read_json(
        &crate_root()
            .join("capabilities")
            .join(format!("{name}.json")),
    )
}

fn string_set(value: &Value, field: &str) -> BTreeSet<String> {
    value[field]
        .as_array()
        .unwrap_or_else(|| panic!("Missing array field {field}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{field} item must be a string"))
                .to_string()
        })
        .collect()
}

fn test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: AppState::mock_llm(),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

/// Helper replicating the exact window-to-principal authorization logic in `src/lib.rs`
fn simulate_authorize_tauri_principal(
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

// ─────────────────────────────────────────────────────────────────────────────
// 1. PRIVILEGE ESCALATION ATTEMPTS FROM WIDGET
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_empirical_widget_privilege_escalation_denied() {
    let widget_cap = capability("widget");
    let permissions = string_set(&widget_cap, "permissions");

    // 1.1 Vault Permissions: Widget must NEVER have any vault permissions
    let vault_permissions = [
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "vault:default",
        "stronghold:default",
    ];
    for perm in vault_permissions {
        assert!(
            !permissions.contains(perm),
            "CRITICAL: Widget capability MUST NOT grant vault permission: {perm}"
        );
    }

    // 1.2 Setup Permissions: Widget must NEVER have setup controls
    let setup_permissions = [
        "allow-open-setup",
        "setup:status",
        "setup:paths",
        "setup:fetch",
    ];
    for perm in setup_permissions {
        assert!(
            !permissions.contains(perm),
            "CRITICAL: Widget capability MUST NOT grant setup permission: {perm}"
        );
    }

    // 1.3 Process Lifecycle & Dialog Permissions: Widget must NEVER control app lifecycle or OS dialogs
    let dangerous_os_permissions = [
        "process:allow-exit",
        "process:allow-restart",
        "process:default",
        "dialog:allow-open",
        "dialog:allow-save",
        "dialog:default",
    ];
    for perm in dangerous_os_permissions {
        assert!(
            !permissions.contains(perm),
            "CRITICAL: Widget capability MUST NOT grant dangerous OS permission: {perm}"
        );
    }

    // 1.4 Native Core Authorization Check: TauriWidget principal must reject all administrative commands
    let escalated_commands = [
        "store_vault_secret",
        "delete_vault_secret",
        "vault_secret_present",
        "setup:fetch",
        "setup:status",
        "setup:paths",
        "update_config",
        "update_user_profile",
        "select_voice_profile",
        "import_avatar_folder",
        "delete_avatar_model",
        "toggle_skill",
        "toggle_all_skills",
        "get_memory_data",
        "memory:set_fact",
        "memory:get_fact",
        "delete_memory_fact",
        "memory:delete_conversation",
        "memory:delete_subject",
        "memory:sweep_retention",
        "consolidate_memory",
        "reset_memory",
        "memory:search_hybrid",
        "memory:upsert_vector",
        "contacts:upsert",
        "contacts:delete",
        "add_task",
        "delete_task",
        "update_task",
        "mcp:call_tool",
        "mcp_client:call_tool",
    ];

    for cmd in escalated_commands {
        let auth_res = authorize_command(CommandPrincipal::TauriWidget, cmd);
        assert!(
            auth_res.is_err(),
            "CRITICAL: TauriWidget principal MUST NOT be authorized for command '{cmd}'"
        );
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. INVALID, FORGED, AND UNMAPPED WINDOW LABELS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_empirical_forged_unmapped_window_labels_fail_closed() {
    let config = read_json(&crate_root().join("tauri.conf.json"));
    let legitimate_windows = config["app"]["windows"]
        .as_array()
        .expect("app.windows array")
        .iter()
        .map(|w| w["label"].as_str().expect("label string"))
        .collect::<BTreeSet<_>>();

    let attack_labels = [
        "hacker_window",
        "",
        " ",
        "unknown",
        "admin",
        "root",
        "system",
        "widget\0injected",
        "widget/subwindow",
        "../widget",
        "WIDGET",
        "DASHBOARD",
        "SETUP",
        "setup ",
        " dashboard",
        "widget.exe",
        "null",
        "undefined",
        "[object Object]",
        "' OR '1'='1",
        "<script>",
        "eval",
        "default",
        "__proto__",
        "constructor",
    ];

    // 2.1 Verify none of these attack labels exist in configuration
    for label in attack_labels {
        assert!(
            !legitimate_windows.contains(label),
            "Attack label '{label}' must NOT be in legitimate windows"
        );
    }

    // 2.2 Verify none of these attack labels match any capability file
    for cap_name in ["widget", "dashboard", "setup"] {
        let cap = capability(cap_name);
        let cap_windows = string_set(&cap, "windows");
        for label in attack_labels {
            assert!(
                !cap_windows.contains(label),
                "Capability '{cap_name}' must NOT attach to attack label '{label}'"
            );
        }
    }

    // 2.3 Verify authorization dispatcher fails closed immediately on unmapped labels
    let test_commands = [
        "ping",
        "status",
        "chat:completion",
        "store_vault_secret",
        "setup:fetch",
    ];
    for label in attack_labels {
        for cmd in test_commands {
            let res = simulate_authorize_tauri_principal(label, cmd);
            assert!(
                res.is_err(),
                "Dispatcher MUST fail closed for unmapped window label '{label}' with command '{cmd}'"
            );
            let err_msg = res.unwrap_err();
            assert!(
                err_msg.contains("Cửa sổ Tauri không được cấp quyền") || err_msg.contains(label),
                "Error message must clearly identify unauthorized window label: {err_msg}"
            );
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// 3. CAPABILITY POLICY INVARIANTS & INTEGRITY
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_empirical_capability_policy_invariants() {
    let cap_dir = crate_root().join("capabilities");
    let entries = fs::read_dir(&cap_dir).expect("read capabilities directory");

    let mut found_caps = BTreeMap::new();
    for entry in entries {
        let entry = entry.expect("directory entry");
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            let stem = path.file_stem().unwrap().to_str().unwrap().to_string();
            let json = read_json(&path);
            found_caps.insert(stem, json);
        }
    }

    // Invariant 1: Exactly 3 capabilities exist, no default.json
    assert_eq!(
        found_caps.keys().cloned().collect::<Vec<_>>(),
        vec!["dashboard", "setup", "widget"],
        "Only controlled capabilities (dashboard, setup, widget) may exist"
    );
    assert!(
        !cap_dir.join("default.json").exists(),
        "Shared/default capability must be permanently removed"
    );

    // Invariant 2: Each capability attached strictly 1-to-1 to its intended window
    let mut claimed_windows = BTreeSet::new();
    for (name, cap_json) in &found_caps {
        let identifier = cap_json["identifier"].as_str().expect("identifier");
        assert_eq!(identifier, name, "Identifier must match file stem");

        let windows = string_set(cap_json, "windows");
        assert_eq!(
            windows.len(),
            1,
            "Capability '{name}' must bind to exactly ONE window"
        );
        let bound_window = windows.iter().next().unwrap();
        assert_eq!(
            bound_window, name,
            "Capability '{name}' must bind strictly to window '{name}'"
        );

        assert!(
            claimed_windows.insert(bound_window.clone()),
            "Window '{bound_window}' must not be claimed by multiple capabilities"
        );
    }

    // Invariant 3: Zero wildcard permissions in all capabilities
    for (name, cap_json) in &found_caps {
        let perms = string_set(cap_json, "permissions");
        for perm in perms {
            assert!(
                !perm.contains('*'),
                "Capability '{name}' contains forbidden wildcard permission '{perm}'"
            );
        }
    }

    // Invariant 4: tauri.conf.json security.capabilities strictly matches
    let config = read_json(&crate_root().join("tauri.conf.json"));
    let enabled_caps = string_set(&config["app"]["security"], "capabilities");
    assert_eq!(
        enabled_caps,
        BTreeSet::from([
            "dashboard".to_string(),
            "setup".to_string(),
            "widget".to_string()
        ]),
        "tauri.conf.json security.capabilities must enable exactly dashboard, setup, widget"
    );
}

// ─────────────────────────────────────────────────────────────────────────────
// 4. CROSS-WINDOW COMMAND ISOLATION MATRIX
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_empirical_cross_window_command_isolation_matrix() {
    // 4.1 Widget-exclusive voice streaming and overlay controls
    let widget_voice_and_overlay_exclusive = [
        "allow-voice-subscribe",
        "allow-voice-mic-chunk",
        "allow-voice-wake-probe",
        "allow-voice-interrupt",
        "allow-toggle-ghost-mode",
        "allow-set-eco-mode",
        "allow-update-interactive-zones",
    ];
    let dashboard_perms = string_set(&capability("dashboard"), "permissions");
    let setup_perms = string_set(&capability("setup"), "permissions");

    for perm in widget_voice_and_overlay_exclusive {
        assert!(
            !dashboard_perms.contains(perm),
            "Dashboard MUST NOT have widget-exclusive permission: {perm}"
        );
        assert!(
            !setup_perms.contains(perm),
            "Setup MUST NOT have widget-exclusive permission: {perm}"
        );
    }

    // 4.2 Dashboard-exclusive administrative and vault controls
    let dashboard_exclusive = [
        "allow-open-setup",
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "dialog:allow-open",
        "process:allow-exit",
        "process:allow-restart",
    ];
    let widget_perms = string_set(&capability("widget"), "permissions");
    for perm in dashboard_exclusive {
        assert!(
            !widget_perms.contains(perm),
            "Widget MUST NOT have dashboard-exclusive permission: {perm}"
        );
        assert!(
            !setup_perms.contains(perm),
            "Setup MUST NOT have dashboard-exclusive permission: {perm}"
        );
    }

    // 4.3 Setup-exclusive window close control
    assert!(setup_perms.contains("core:window:allow-close"));
    assert!(!widget_perms.contains("core:window:allow-close"));
    assert!(!dashboard_perms.contains("core:window:allow-close"));

    // 4.4 Dispatcher simulation cross-invocations
    assert!(simulate_authorize_tauri_principal("widget", "setup:fetch").is_err());
    assert!(simulate_authorize_tauri_principal("widget", "reset_memory").is_err());
    assert!(simulate_authorize_tauri_principal("dashboard", "setup:fetch").is_err());
    assert!(simulate_authorize_tauri_principal("dashboard", "setup:status").is_err());
    assert!(simulate_authorize_tauri_principal("setup", "chat:completion").is_err());
    assert!(simulate_authorize_tauri_principal("setup", "update_config").is_err());
    assert!(simulate_authorize_tauri_principal("setup", "ping").is_err());
}

// ─────────────────────────────────────────────────────────────────────────────
// 5. STATEFUL REJECTION & RESILIENCE (ZERO SIDE-EFFECTS)
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_empirical_stateful_rejection_no_side_effects() {
    let state = test_state();

    // 1. Attempt unauthorized memory reset from Widget
    let res = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "reset_memory",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(res.is_err(), "Widget memory reset must be rejected");
    let err = res.unwrap_err();
    assert!(
        err.contains("not authorized") || err.contains("TauriWidget"),
        "Error message must indicate lack of authorization: {err}"
    );

    // 2. Attempt unauthorized setup fetch from Widget
    let res_setup = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "setup:fetch",
        serde_json::json!({"model_id": "test"}),
        None,
        None,
    )
    .await;
    assert!(res_setup.is_err(), "Widget setup:fetch must be rejected");

    // 3. Attempt unauthorized chat from Setup
    let res_chat = handle_command_as(
        CommandPrincipal::TauriSetup,
        state.clone(),
        "chat:completion",
        serde_json::json!({"messages": [{"role": "user", "content": "hi"}]}),
        None,
        None,
    )
    .await;
    assert!(res_chat.is_err(), "Setup chat:completion must be rejected");

    // 4. Legitimate commands succeed for their respective principals
    let ping_widget = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(ping_widget.is_ok(), "Widget ping must succeed");

    let ping_dashboard = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await;
    assert!(ping_dashboard.is_ok(), "Dashboard ping must succeed");
}
