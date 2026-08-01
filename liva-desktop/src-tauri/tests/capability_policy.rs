use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

fn crate_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(
        &fs::read_to_string(path)
            .unwrap_or_else(|error| panic!("không đọc được {}: {error}", path.display())),
    )
    .unwrap_or_else(|error| panic!("JSON không hợp lệ {}: {error}", path.display()))
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
        .unwrap_or_else(|| panic!("thiếu mảng {field}"))
        .iter()
        .map(|item| {
            item.as_str()
                .unwrap_or_else(|| panic!("{field} chỉ được chứa chuỗi"))
                .to_string()
        })
        .collect()
}

#[test]
fn chi_bat_ba_capability_da_kiem_soat() {
    let config = read_json(&crate_root().join("tauri.conf.json"));
    let enabled = string_set(&config["app"]["security"], "capabilities");
    assert_eq!(
        enabled,
        BTreeSet::from([
            "dashboard".to_string(),
            "setup".to_string(),
            "widget".to_string(),
        ])
    );
    assert!(
        !crate_root().join("capabilities/default.json").exists(),
        "capability dùng chung phải bị loại bỏ"
    );
}

#[test]
fn moi_capability_chi_gan_dung_mot_window() {
    for name in ["widget", "dashboard", "setup"] {
        let cap = capability(name);
        assert_eq!(cap["identifier"], name);
        assert_eq!(
            string_set(&cap, "windows"),
            BTreeSet::from([name.to_string()])
        );
    }
}

#[test]
fn widget_khong_co_quyen_vault_setup_dialog_hay_process() {
    let permissions = string_set(&capability("widget"), "permissions");
    for required in [
        "allow-native-ipc-call",
        "allow-native-ipc-call-stream",
        "allow-issue-websocket-session",
        "allow-toggle-ghost-mode",
        "allow-update-interactive-zones",
        "allow-open-dashboard",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
    ] {
        assert!(permissions.contains(required), "widget thiếu {required}");
    }
    for forbidden in [
        "allow-open-setup",
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "dialog:default",
        "dialog:allow-open",
        "process:default",
        "process:allow-exit",
        "process:allow-restart",
    ] {
        assert!(!permissions.contains(forbidden), "widget thừa {forbidden}");
    }
}

#[test]
fn setup_chi_co_quyen_tai_artifact_va_dong_cua_so() {
    let permissions = string_set(&capability("setup"), "permissions");
    for required in [
        "allow-native-ipc-call",
        "allow-native-ipc-call-stream",
        "allow-open-dashboard",
        "core:event:allow-listen",
        "core:event:allow-unlisten",
        "core:window:allow-close",
    ] {
        assert!(permissions.contains(required), "setup thiếu {required}");
    }
    for forbidden in [
        "allow-issue-websocket-session",
        "allow-toggle-ghost-mode",
        "allow-update-interactive-zones",
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "dialog:default",
        "process:default",
    ] {
        assert!(!permissions.contains(forbidden), "setup thừa {forbidden}");
    }
}

#[test]
fn dashboard_co_vault_nhung_khong_dieu_khien_widget() {
    let permissions = string_set(&capability("dashboard"), "permissions");
    for required in [
        "allow-native-ipc-call",
        "allow-native-ipc-call-stream",
        "allow-issue-websocket-session",
        "allow-vault-secret-present",
        "allow-store-vault-secret",
        "allow-delete-vault-secret",
        "dialog:allow-open",
        "process:allow-exit",
        "process:allow-restart",
    ] {
        assert!(permissions.contains(required), "dashboard thiếu {required}");
    }
    for forbidden in [
        "allow-toggle-ghost-mode",
        "allow-set-eco-mode",
        "allow-update-interactive-zones",
        "allow-open-dashboard",
    ] {
        assert!(
            !permissions.contains(forbidden),
            "dashboard thừa {forbidden}"
        );
    }
}
