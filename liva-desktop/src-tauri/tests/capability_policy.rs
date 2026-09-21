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
        "allow-voice-subscribe",
        "allow-voice-mic-chunk",
        "allow-voice-wake-probe",
        "allow-voice-interrupt",
        "allow-toggle-ghost-mode",
        "allow-set-eco-mode",
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
        "allow-voice-subscribe",
        "allow-voice-mic-chunk",
        "allow-voice-wake-probe",
        "allow-voice-interrupt",
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
        "allow-open-setup",
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
        "allow-voice-subscribe",
        "allow-voice-mic-chunk",
        "allow-voice-wake-probe",
        "allow-voice-interrupt",
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

#[test]
fn test_tauri_ipc_channel_commands_registered_in_build_manifest() {
    let build_rs = fs::read_to_string(crate_root().join("build.rs")).expect("build.rs must exist");

    for cmd in [
        "\"voice_subscribe\"",
        "\"voice_mic_chunk\"",
        "\"voice_wake_probe\"",
        "\"voice_interrupt\"",
        "\"native_ipc_call\"",
        "\"native_ipc_call_stream\"",
    ] {
        assert!(
            build_rs.contains(cmd),
            "build.rs AppManifest commands must declare {cmd}"
        );
    }
}

#[test]
fn test_tauri_ipc_channel_authorized_in_capabilities() {
    let widget_perms = string_set(&capability("widget"), "permissions");
    for req in [
        "allow-voice-subscribe",
        "allow-voice-mic-chunk",
        "allow-voice-wake-probe",
        "allow-voice-interrupt",
        "allow-native-ipc-call",
        "allow-native-ipc-call-stream",
    ] {
        assert!(
            widget_perms.contains(req),
            "widget capability must contain {req}"
        );
    }
}

#[test]
fn test_tauri_ipc_channel_event_serialization_contract() {
    use liva_native_core::VoiceIpcEvent;

    // 1. Test Speaker event serialization
    let speaker_event = VoiceIpcEvent::Speaker {
        turn_epoch: 1,
        sample_rate: 24000,
        samples: vec![0.0, 0.5, -0.5],
    };
    let json = serde_json::to_value(&speaker_event).expect("Speaker serialize");
    assert_eq!(json["type"], "Speaker");
    assert_eq!(json["data"]["turn_epoch"], 1);
    assert_eq!(json["data"]["sample_rate"], 24000);

    // 2. Test Viseme event serialization
    let viseme_event = VoiceIpcEvent::Viseme {
        turn_epoch: 1,
        base_seq_id: 10,
        visemes: serde_json::json!([{"time": 0.1, "viseme": "aa"}]),
    };
    let v_json = serde_json::to_value(&viseme_event).expect("Viseme serialize");
    assert_eq!(v_json["type"], "Viseme");
    assert_eq!(v_json["data"]["base_seq_id"], 10);

    // 3. Test Flush event serialization
    let flush_event = VoiceIpcEvent::Flush { seq_id: 42 };
    let f_json = serde_json::to_value(&flush_event).expect("Flush serialize");
    assert_eq!(f_json["type"], "Flush");
    assert_eq!(f_json["data"]["seq_id"], 42);

    // 4. Test TextEvent serialization
    let text_event = VoiceIpcEvent::TextEvent {
        event: "transcription".to_string(),
        payload: serde_json::json!({"text": "test"}),
    };
    let t_json = serde_json::to_value(&text_event).expect("TextEvent serialize");
    assert_eq!(t_json["type"], "TextEvent");
    assert_eq!(t_json["data"]["event"], "transcription");
}
