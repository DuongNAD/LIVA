//! Milestone 2: Desktop Dashboard Tauri v2 & IPC Bridge Comprehensive Tests
//!
//! Validates:
//! 1. Multi-Channel Commands (Telegram, WhatsApp, Discord, Slack)
//! 2. Skill Manager Commands (Manifest, Config, Logs, ClawHub Install)
//! 3. Companion Node Pairing Commands (Challenges, Short-code approval, Revocation)
//! 4. Browser Automation Commands (Status, Screenshot, Navigation, DOM Extraction, Action Logs, Control)
//! 5. Fail-Closed Principal Authorization Security Gate

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    AppState, CommandPrincipal, authorize_command, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::sync::Arc;

fn test_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
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
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

#[tokio::test]
async fn test_m2_channels_management_lifecycle() {
    let state = test_state();

    // 1. List channels
    let list_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:list",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list channels");

    assert!(list_res["count"].as_u64().unwrap() >= 4);
    let channels = list_res["channels"].as_array().unwrap();
    let has_telegram = channels.iter().any(|c| c["id"] == "telegram");
    let has_whatsapp = channels.iter().any(|c| c["id"] == "whatsapp");
    let has_discord = channels.iter().any(|c| c["id"] == "discord");
    let has_slack = channels.iter().any(|c| c["id"] == "slack");
    assert!(has_telegram && has_whatsapp && has_discord && has_slack);

    // 2. Configure Discord channel
    let config_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:configure",
        json!({
            "channelId": "discord",
            "config": {
                "enabled": true,
                "botToken": "MTA5ODc2NTQzMjEw.GhIjKl.MnOpQrStUvWxYz",
                "clientId": "109876543210"
            }
        }),
        None,
        None,
    )
    .await
    .expect("configure discord");

    assert!(config_res["success"].as_bool().unwrap());
    assert_eq!(config_res["channel"]["status"]["status"], "connected");

    // 3. Generate WhatsApp live QR code
    let qr_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:whatsapp_qr",
        json!({}),
        None,
        None,
    )
    .await
    .expect("whatsapp qr");

    assert!(qr_res["qrData"].as_str().unwrap().starts_with("2@LIVA_PAIR_"));
    assert!(qr_res["ttlSeconds"].as_u64().unwrap() > 0);

    // 4. Test channel connection probe
    let test_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:test",
        json!({ "channelId": "telegram" }),
        None,
        None,
    )
    .await
    .expect("test telegram");

    assert!(test_res["success"].as_bool().unwrap());
    assert!(test_res["latencyMs"].as_u64().unwrap() > 0);

    // 5. Start and Stop channel
    let stop_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:stop",
        json!({ "channelId": "discord" }),
        None,
        None,
    )
    .await
    .expect("stop discord");
    assert_eq!(stop_res["channel"]["status"]["status"], "disconnected");

    let start_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "channels:start",
        json!({ "channelId": "discord" }),
        None,
        None,
    )
    .await
    .expect("start discord");
    assert_eq!(start_res["channel"]["status"]["status"], "connected");
}

#[tokio::test]
async fn test_m2_skill_manager_extended_commands() {
    let state = test_state();

    // 1. Get manifest
    let manifest_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:get_manifest",
        json!({ "skillId": "liva-technical-debt-triage" }),
        None,
        None,
    )
    .await
    .expect("get manifest");

    assert_eq!(manifest_res["skillId"], "liva-technical-debt-triage");
    assert!(!manifest_res["markdownInstructions"].as_str().unwrap().is_empty());
    assert!(!manifest_res["contentHash"].as_str().unwrap().is_empty());

    // 2. Get and Save Config
    let config_get = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:get_config",
        json!({ "skillId": "liva-skill-governance" }),
        None,
        None,
    )
    .await
    .expect("get skill config");

    assert!(config_get["schema"]["properties"]["timeoutSeconds"].is_object());

    let config_save = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:save_config",
        json!({
            "skillId": "liva-skill-governance",
            "params": {
                "timeoutSeconds": 60,
                "maxRetries": 5,
                "logVerbosity": "debug"
            }
        }),
        None,
        None,
    )
    .await
    .expect("save skill config");

    assert!(config_save["success"].as_bool().unwrap());
    assert_eq!(config_save["params"]["timeoutSeconds"], 60);

    // 3. Execution logs
    let logs_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:logs",
        json!({ "skillId": "liva-technical-debt-triage", "limit": 10 }),
        None,
        None,
    )
    .await
    .expect("get skill logs");

    assert!(logs_res["count"].as_u64().unwrap() > 0);
    assert!(!logs_res["logs"].as_array().unwrap().is_empty());

    // 4. Install from ClawHub
    let install_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:install_from_hub",
        json!({ "skillId": "clawhub-weather-radar", "name": "clawhub-weather-radar" }),
        None,
        None,
    )
    .await
    .expect("install from clawhub");

    assert!(install_res["success"].as_bool().unwrap());
    assert_eq!(install_res["name"], "clawhub-weather-radar");
}

#[tokio::test]
async fn test_m2_node_pairing_monitor_protocol() {
    let state = test_state();

    // 1. Create a pairing challenge (simulating mobile device)
    let challenge_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:create_challenge",
        json!({
            "nodeName": "iPad Pro M4",
            "role": "mobile_companion",
            "publicKey": "ed25519_test_client_key_12345"
        }),
        None,
        None,
    )
    .await
    .expect("create challenge");

    let short_code = challenge_res["shortCode"].as_str().unwrap().to_string();
    let challenge_id = challenge_res["challengeId"].as_str().unwrap().to_string();
    assert_eq!(short_code.len(), 6);
    assert!(!challenge_id.is_empty());

    // 2. List pending challenges
    let pending_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:list_pending",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list pending");

    let pending_list = pending_res["challenges"].as_array().unwrap();
    assert!(pending_list.iter().any(|c| c["shortCode"] == short_code));

    // 3. Approve via 6-digit short code
    let approve_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:approve",
        json!({ "shortCode": short_code }),
        None,
        None,
    )
    .await
    .expect("approve pairing");

    assert!(approve_res["success"].as_bool().unwrap());
    assert!(approve_res["authToken"].as_str().unwrap().starts_with("v1:"));

    // 4. List approved nodes
    let nodes_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:list_nodes",
        json!({}),
        None,
        None,
    )
    .await
    .expect("list nodes");

    let nodes = nodes_res["nodes"].as_array().unwrap();
    let paired_node = nodes.iter().find(|n| n["nodeName"] == "iPad Pro M4").expect("found paired node");
    let node_id = paired_node["nodeId"].as_str().unwrap().to_string();
    assert_eq!(paired_node["deviceType"], "mobile");

    // 5. Revoke node
    let revoke_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:revoke",
        json!({ "nodeId": node_id }),
        None,
        None,
    )
    .await
    .expect("revoke node");

    assert!(revoke_res["success"].as_bool().unwrap());

    let nodes_after = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "pairing:list_nodes",
        json!({}),
        None,
        None,
    )
    .await
    .unwrap();

    assert!(!nodes_after["nodes"].as_array().unwrap().iter().any(|n| n["nodeId"] == node_id));
}

#[tokio::test]
async fn test_m2_browser_preview_automation_controller() {
    let state = test_state();

    // 1. Browser status
    let status_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("browser status");

    assert!(status_res["isRunning"].as_bool().unwrap());
    assert!(status_res["sandboxActive"].as_bool().unwrap());
    assert!(status_res["ssrfGuard"].as_bool().unwrap());

    // 2. Browser navigate
    let nav_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "https://doc.rust-lang.org/book/" }),
        None,
        None,
    )
    .await
    .expect("browser navigate");

    assert_eq!(nav_res["url"], "https://doc.rust-lang.org/book/");
    assert_eq!(nav_res["httpStatus"], 200);

    // 3. Browser screenshot
    let shot_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:screenshot",
        json!({}),
        None,
        None,
    )
    .await
    .expect("browser screenshot");

    assert!(shot_res["base64Png"].as_str().unwrap().starts_with("data:image/png;base64,"));
    assert_eq!(shot_res["width"], 1280);
    assert_eq!(shot_res["height"], 800);

    // 4. DOM extraction
    let extract_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:extract",
        json!({ "mode": "accessibility" }),
        None,
        None,
    )
    .await
    .expect("browser extract");

    assert_eq!(extract_res["mode"], "accessibility");

    // 5. Action logs
    let logs_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:action_log",
        json!({}),
        None,
        None,
    )
    .await
    .expect("browser action logs");

    assert!(logs_res["count"].as_u64().unwrap() > 0);

    // 6. Session control
    let pause_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "pause" }),
        None,
        None,
    )
    .await
    .expect("browser pause");
    assert_eq!(pause_res["state"], "paused");

    let resume_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "resume" }),
        None,
        None,
    )
    .await
    .expect("browser resume");
    assert_eq!(resume_res["state"], "running");
}

#[test]
fn test_m2_authorization_fail_closed_for_weak_principals() {
    let m2_sensitive_commands = [
        "channels:list",
        "channels:configure",
        "channels:whatsapp_qr",
        "channels:start",
        "channels:stop",
        "skills:get_manifest",
        "skills:save_config",
        "skills:install_from_hub",
        "pairing:approve",
        "pairing:revoke",
        "browser:navigate",
        "browser:screenshot",
        "browser:control",
    ];

    let weak_principals = [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::WebSocketWidget,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::Telegram,
    ];

    for principal in weak_principals {
        for cmd in m2_sensitive_commands {
            assert!(
                authorize_command(principal, cmd).is_err(),
                "Principal {principal:?} MUST NOT be authorized to execute '{cmd}'"
            );
        }
    }

    for cmd in m2_sensitive_commands {
        assert!(
            authorize_command(CommandPrincipal::TauriDashboard, cmd).is_ok(),
            "TauriDashboard MUST be authorized for '{cmd}'"
        );
        assert!(
            authorize_command(CommandPrincipal::WebSocketDashboard, cmd).is_ok(),
            "WebSocketDashboard MUST be authorized for '{cmd}'"
        );
    }
}
