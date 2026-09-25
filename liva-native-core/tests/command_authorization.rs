use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, db, handle_command_as, llm, stt, tts,
};
use std::sync::Arc;

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
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua: AppState::mock_cua(),
    })
}

fn duoc(principal: CommandPrincipal, command: &str) {
    authorize_command(principal, command)
        .unwrap_or_else(|error| panic!("{principal:?} phải được gọi {command}: {error}"));
}

fn bi_chan(principal: CommandPrincipal, command: &str) {
    assert!(
        authorize_command(principal, command).is_err(),
        "{principal:?} không được phép gọi {command}"
    );
}

#[test]
fn setup_chi_duoc_dung_mien_setup() {
    for command in ["setup:status", "setup:paths", "setup:fetch"] {
        duoc(CommandPrincipal::TauriSetup, command);
    }

    for command in [
        "get_config",
        "update_config",
        "get_memory_data",
        "llm:swap_model",
        "mcp:call_tool",
    ] {
        bi_chan(CommandPrincipal::TauriSetup, command);
    }
}

#[test]
fn widget_duoc_hoi_thoai_nhung_khong_duoc_quan_tri() {
    for command in [
        "ping",
        "get_config",
        "get_system_status",
        "get_user_profile",
        "get_avatar_models",
        "chat:completion",
        "vision:ask",
        "voice:tts_speak",
        "message:pending",
        "message:confirm",
        "message:cancel",
        "telemetry:summary",
    ] {
        duoc(CommandPrincipal::TauriWidget, command);
    }

    for command in [
        "update_config",
        "get_memory_data",
        "delete_memory_fact",
        "memory:delete_subject",
        "memory:sweep_retention",
        "add_task",
        "skills:pin_ids",
        "llm:swap_model",
        "setup:fetch",
        "mcp:call_tool",
        "telegram:send_text",
    ] {
        bi_chan(CommandPrincipal::TauriWidget, command);
    }
}

#[test]
fn dashboard_duoc_quan_tri_ui_nhung_khong_co_cua_thoat_native_tho() {
    for command in [
        "get_config",
        "update_config",
        "get_memory_data",
        "delete_memory_fact",
        "memory:delete_conversation",
        "memory:delete_subject",
        "memory:sweep_retention",
        "consolidate_memory",
        "get_tasks",
        "add_task",
        "skills:list",
        "skills:search",
        "consent:grant",
        "vision:set_config",
        "voice:set_vieneu_voice",
        "telemetry:summary",
    ] {
        duoc(CommandPrincipal::TauriDashboard, command);
    }

    for command in [
        "setup:fetch",
        "llm:swap_model",
        "mcp:call_tool",
        "mcp_client:call_tool",
        "skills:pin_ids",
        "telegram:send_text",
        "integration:smart_home_control",
    ] {
        bi_chan(CommandPrincipal::TauriDashboard, command);
    }
}

#[test]
fn principal_khong_tin_cay_mac_dinh_tu_choi_lenh_moi() {
    for principal in [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::Telegram,
    ] {
        bi_chan(principal, "future:dangerous_command");
    }
}

#[test]
fn cli_noi_bo_va_test_la_hai_principal_tin_cay_ro_rang() {
    for principal in [CommandPrincipal::LocalCli, CommandPrincipal::Test] {
        duoc(principal, "mcp:call_tool");
        duoc(principal, "llm:swap_model");
        duoc(principal, "future:diagnostic_command");
    }
}

#[tokio::test]
async fn dispatcher_chan_principal_truoc_khi_chay_handler() {
    let error = handle_command_as(
        CommandPrincipal::TauriWidget,
        test_state(),
        "update_config",
        serde_json::json!({"system": {"language": "en"}}),
        None,
        None,
    )
    .await
    .expect_err("widget không được ghi config");

    assert!(error.contains("TauriWidget"), "{error}");
    assert!(error.contains("update_config"), "{error}");
}

#[tokio::test]
async fn dispatcher_van_chay_lenh_duoc_cap_quyen() {
    let response = handle_command_as(
        CommandPrincipal::TauriWidget,
        test_state(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await
    .expect("widget được ping");

    assert_eq!(response, serde_json::json!({"pong": true}));
}

#[tokio::test]
async fn dashboard_co_the_chay_projection_batch_thu_cong() {
    let response = handle_command_as(
        CommandPrincipal::TauriDashboard,
        test_state(),
        "consolidate_memory",
        serde_json::json!({"batchSize": 25}),
        None,
        None,
    )
    .await
    .expect("dashboard được chạy projection batch");

    assert_eq!(response["processed"], 0);
    assert_eq!(response["consolidated"], 0);
}

#[test]
fn dashboard_duoc_toan_quyen_cua() {
    for cmd in [
        "cua:list_windows",
        "cua:execute_action",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:get_status",
        "cua:query_audit_logs",
    ] {
        duoc(CommandPrincipal::TauriDashboard, cmd);
    }
}

#[test]
fn widget_duoc_dieu_khien_cua_nhung_khong_duoc_truy_van_audit_ledger() {
    for cmd in [
        "cua:list_windows",
        "cua:execute_action",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:get_status",
    ] {
        duoc(CommandPrincipal::TauriWidget, cmd);
    }

    // Fail-closed barrier: Widget cannot perform heavy historical ledger audits
    bi_chan(CommandPrincipal::TauriWidget, "cua:query_audit_logs");
}

#[test]
fn setup_va_telegram_bi_chan_toan_bo_cua() {
    for cmd in [
        "cua:list_windows",
        "cua:execute_action",
        "cua:emergency_stop",
        "cua:set_mode",
        "cua:get_status",
        "cua:query_audit_logs",
    ] {
        bi_chan(CommandPrincipal::TauriSetup, cmd);
        bi_chan(CommandPrincipal::Telegram, cmd);
    }
}
