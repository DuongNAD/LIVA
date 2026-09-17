//! Adversarial & Stress Verification Challenge Test Suite for Milestone M3
//! (Desktop UI Clean-up & IPC Synchronization).
//!
//! Verifies:
//! 1. Principle Authorization Matrix for `audio_play_started` and `audio_play_finished`:
//!    - WebSocketWidget / TauriWidget: ALLOWED (200 / Ok)
//!    - WebSocketDashboard / TauriDashboard / WebSocketRemote / TauriSetup / Telegram: DENIED (fail-closed)
//! 2. Live WebSocket Execution for WebSocketWidget:
//!    - Events execute cleanly as graceful no-ops without emitting `{event}_error` or breaking the connection.
//!    - Connection remains fully responsive to subsequent command dispatching (e.g. ping/pong).
//! 3. Live WebSocket Fail-Closed Enforcement for Remote Clients:
//!    - Unauthenticated/remote WebSocket clients sending `audio_play_started` receive `{event}_error`.
//! 4. High-Throughput Stress Burst:
//!    - Burst of 200 alternating audio lifecycle events handled without resource leakage or panic.
//! 5. Robustness to Malformed / Fuzzed Payloads:
//!    - Non-object, null, array, and extraneous payloads in audio events do not panic the server.

use futures_util::{SinkExt, StreamExt};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::websocket::WebSocketServer;
use liva_native_core::{AppState, CommandPrincipal, authorize_command, db, llm, stt, tts};
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::{connect_async, tungstenite::Message};

fn create_test_state() -> Arc<AppState> {
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
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
    })
}

#[test]
fn test_audio_play_events_authorization_matrix() {
    let audio_events = ["audio_play_started", "audio_play_finished"];

    // 1. Authorized principals: TauriWidget and WebSocketWidget
    for cmd in &audio_events {
        assert!(
            authorize_command(CommandPrincipal::TauriWidget, cmd).is_ok(),
            "TauriWidget must be authorized for {cmd}"
        );
        assert!(
            authorize_command(CommandPrincipal::WebSocketWidget, cmd).is_ok(),
            "WebSocketWidget must be authorized for {cmd}"
        );
        assert!(
            authorize_command(CommandPrincipal::LocalCli, cmd).is_ok(),
            "LocalCli must be authorized for {cmd}"
        );
        assert!(
            authorize_command(CommandPrincipal::Test, cmd).is_ok(),
            "Test principal must be authorized for {cmd}"
        );
    }

    // 2. Unauthorized principals must be strictly fail-closed
    let unauthorized_principals = [
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::WebSocketDashboard,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::Telegram,
    ];

    for principal in unauthorized_principals {
        for cmd in &audio_events {
            let res = authorize_command(principal, cmd);
            assert!(
                res.is_err(),
                "{principal:?} must NOT be authorized for {cmd}, but got {res:?}"
            );
        }
    }
}

#[tokio::test]
async fn test_websocket_widget_audio_events_clean_execution_and_noop() {
    let server = WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = server.local_addr();
    let sessions = server.session_authority();
    let ticket = sessions
        .issue(CommandPrincipal::WebSocketWidget)
        .expect("issue widget session ticket");

    let server_task = tokio::spawn(server.run(create_test_state()));
    let url = format!("ws://{address}/ws?session={}", ticket.token);

    let (mut client, _) = connect_async(&url)
        .await
        .expect("connect with widget session");

    // Send audio_play_started
    client
        .send(Message::Text(
            serde_json::json!({
                "event": "audio_play_started",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send audio_play_started");

    // Send audio_play_finished
    client
        .send(Message::Text(
            serde_json::json!({
                "event": "audio_play_finished",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send audio_play_finished");

    // Now send a ping command to ensure channel is alive and no error was emitted before it
    client
        .send(Message::Text(
            serde_json::json!({
                "id": "probe_ping_1",
                "command": "ping",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send ping");

    // The first response received MUST be the ping response, NOT an audio error!
    let response = tokio::time::timeout(Duration::from_secs(3), client.next())
        .await
        .expect("timeout waiting for response")
        .expect("stream ended")
        .expect("ws error");

    let Message::Text(text) = response else {
        panic!("expected text response from ping");
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some("probe_ping_1"),
        "First received message should be ping response, not an error message: {text}"
    );
    assert_eq!(
        parsed.get("status").and_then(|v| v.as_str()),
        Some("ok"),
        "Ping should succeed"
    );

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn test_websocket_remote_audio_events_fail_closed_under_acl() {
    let server = WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = server.local_addr();

    let server_task = tokio::spawn(server.run(create_test_state()));
    // Connect WITHOUT session token -> maps to WebSocketRemote
    let url = format!("ws://{address}/ws");

    let (mut client, _) = connect_async(&url).await.expect("connect as remote");

    // Send audio_play_started as remote client
    client
        .send(Message::Text(
            serde_json::json!({
                "event": "audio_play_started",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send audio_play_started");

    // Remote client MUST receive audio_play_started_error
    let response = tokio::time::timeout(Duration::from_secs(3), client.next())
        .await
        .expect("timeout waiting for rejection")
        .expect("stream ended")
        .expect("ws error");

    let Message::Text(text) = response else {
        panic!("expected text response");
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(
        parsed.get("event").and_then(|v| v.as_str()),
        Some("audio_play_started_error"),
        "Unauthorized remote client must receive audio_play_started_error"
    );

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn test_websocket_widget_audio_events_stress_burst() {
    let server = WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = server.local_addr();
    let sessions = server.session_authority();
    let ticket = sessions
        .issue(CommandPrincipal::WebSocketWidget)
        .expect("issue widget session ticket");

    let server_task = tokio::spawn(server.run(create_test_state()));
    let url = format!("ws://{address}/ws?session={}", ticket.token);

    let (mut client, _) = connect_async(&url)
        .await
        .expect("connect with widget session");

    // Send a burst of 200 alternating audio play events
    for i in 0..100 {
        client
            .send(Message::Text(
                serde_json::json!({
                    "event": "audio_play_started",
                    "payload": { "seq": i }
                })
                .to_string(),
            ))
            .await
            .expect("send burst start");

        client
            .send(Message::Text(
                serde_json::json!({
                    "event": "audio_play_finished",
                    "payload": { "seq": i }
                })
                .to_string(),
            ))
            .await
            .expect("send burst finish");
    }

    // Verify system responsiveness after stress burst
    client
        .send(Message::Text(
            serde_json::json!({
                "id": "post_burst_ping",
                "command": "ping",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send ping");

    let response = tokio::time::timeout(Duration::from_secs(5), client.next())
        .await
        .expect("timeout waiting for post-burst ping")
        .expect("stream ended")
        .expect("ws error");

    let Message::Text(text) = response else {
        panic!("expected text response");
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some("post_burst_ping"),
        "Ping response must be received cleanly after burst: {text}"
    );

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn test_websocket_widget_audio_events_malformed_fuzz() {
    let server = WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind test server");
    let address = server.local_addr();
    let sessions = server.session_authority();
    let ticket = sessions
        .issue(CommandPrincipal::WebSocketWidget)
        .expect("issue widget session ticket");

    let server_task = tokio::spawn(server.run(create_test_state()));
    let url = format!("ws://{address}/ws?session={}", ticket.token);

    let (mut client, _) = connect_async(&url)
        .await
        .expect("connect with widget session");

    // Various fuzzed payloads
    let fuzzed = vec![
        serde_json::json!({"event": "audio_play_started", "payload": null}),
        serde_json::json!({"event": "audio_play_started", "payload": "unexpected_string"}),
        serde_json::json!({"event": "audio_play_started", "payload": [1, 2, 3, false]}),
        serde_json::json!({"event": "audio_play_finished", "payload": -12345}),
        serde_json::json!({"event": "audio_play_finished", "payload": {"deep": {"nested": [null]}}}),
        serde_json::json!({"event": "audio_play_started"}), // missing payload
    ];

    for msg in fuzzed {
        client
            .send(Message::Text(msg.to_string()))
            .await
            .expect("send fuzzed msg");
    }

    // Follow with ping to ensure stability
    client
        .send(Message::Text(
            serde_json::json!({
                "id": "post_fuzz_ping",
                "command": "ping",
                "payload": {}
            })
            .to_string(),
        ))
        .await
        .expect("send ping");

    let response = tokio::time::timeout(Duration::from_secs(3), client.next())
        .await
        .expect("timeout waiting for post-fuzz ping")
        .expect("stream ended")
        .expect("ws error");

    let Message::Text(text) = response else {
        panic!("expected text response");
    };

    let parsed: serde_json::Value = serde_json::from_str(&text).expect("valid json");
    assert_eq!(
        parsed.get("id").and_then(|v| v.as_str()),
        Some("post_fuzz_ping"),
        "Ping response must be received cleanly after fuzzing: {text}"
    );

    client.close(None).await.expect("close client");
    server_task.abort();
}
