use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::frame::{OP_AUTH_HANDSHAKE, VoiceFrame};
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::{connect_async, tungstenite::Message};

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
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

async fn receive_text_event(
    client: &mut tokio_tungstenite::WebSocketStream<
        tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>,
    >,
    expected_event: &str,
) -> serde_json::Value {
    tokio::time::timeout(std::time::Duration::from_secs(3), async {
        loop {
            let message = client
                .next()
                .await
                .expect("server closed before expected event")
                .expect("receive WebSocket event");
            let Message::Text(text) = message else {
                continue;
            };
            let value: serde_json::Value =
                serde_json::from_str(&text).expect("server text must be JSON");
            if value.get("event").and_then(|event| event.as_str()) == Some(expected_event) {
                return value;
            }
        }
    })
    .await
    .expect("timed out waiting for expected event")
}

#[tokio::test]
async fn reusable_server_binds_and_echoes_voice_handshake() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    assert_ne!(address.port(), 0, "port zero must resolve to a real port");

    let server_task = tokio::spawn(server.run(test_state()));
    let (mut client, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect to reusable server");
    let expected = VoiceFrame {
        op_code: OP_AUTH_HANDSHAKE,
        seq_id: 41,
        payload: Bytes::from_static(b"embedded-tauri"),
    };

    client
        .send(Message::Binary(
            expected.encode().expect("encode handshake").to_vec(),
        ))
        .await
        .expect("send handshake");

    let message = tokio::time::timeout(std::time::Duration::from_secs(2), client.next())
        .await
        .expect("handshake timeout")
        .expect("server closed connection")
        .expect("receive handshake");
    let Message::Binary(data) = message else {
        panic!("handshake response must be binary");
    };
    let actual = VoiceFrame::decode(&mut BytesMut::from(data.as_slice()))
        .expect("decode handshake")
        .expect("complete handshake");

    assert_eq!(actual.op_code, expected.op_code);
    assert_eq!(actual.seq_id, expected.seq_id);
    assert_eq!(actual.payload, expected.payload);

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn voice_message_dialogue_asks_platform_and_remembers_the_request() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let server_task = tokio::spawn(server.run(test_state()));
    let (mut client, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect to reusable server");

    client
        .send(Message::Text(
            serde_json::json!({
                "event": "user_voice_command",
                "payload": {
                    "text": "nhắn tin cho Minh Hiển hỏi nó chiều đi bắt pokemon k"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send initial voice command");
    let prompt = receive_text_event(&mut client, "ai_spoken_response").await;
    assert_eq!(
        prompt
            .pointer("/payload/text")
            .and_then(|text| text.as_str()),
        Some("Bạn muốn nhắn bằng Messenger hay Telegram?")
    );

    client
        .send(Message::Text(
            serde_json::json!({
                "event": "user_voice_command",
                "payload": { "text": "Messenger" }
            })
            .to_string(),
        ))
        .await
        .expect("answer platform prompt");
    let result = receive_text_event(&mut client, "ai_spoken_response").await;
    let response = result
        .pointer("/payload/text")
        .and_then(|text| text.as_str())
        .expect("spoken response text");
    assert!(
        response.contains("Chưa có ai tên Minh Hiển"),
        "the second turn must reuse the original recipient: {response}"
    );

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn voice_message_dialogue_creates_then_cancels_a_draft_without_sending() {
    let state = test_state();
    {
        let connection = state.db.writer.get().expect("writer connection");
        liva_native_core::messaging::contacts::upsert(
            &connection,
            "Minh Hiển",
            liva_native_core::messaging::contacts::Platform::Messenger,
            "123456789",
            "",
        )
        .expect("insert Messenger contact");
    }
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let server_task = tokio::spawn(server.run(Arc::clone(&state)));
    let (mut client, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect to reusable server");

    client
        .send(Message::Text(
            serde_json::json!({
                "event": "user_voice_command",
                "payload": {
                    "text": "nhắn tin cho Minh Hiển bằng Messenger hỏi nó chiều đi bắt pokemon k"
                }
            })
            .to_string(),
        ))
        .await
        .expect("send complete voice command");
    let confirmation = receive_text_event(&mut client, "ai_spoken_response").await;
    let confirmation_text = confirmation
        .pointer("/payload/text")
        .and_then(|text| text.as_str())
        .expect("confirmation text");
    assert!(confirmation_text.contains("Minh Hiển"));
    assert!(confirmation_text.contains("chiều đi bắt pokemon k"));
    assert!(confirmation_text.contains("gửi đi"));
    assert!(confirmation_text.contains("hủy"));

    client
        .send(Message::Text(
            serde_json::json!({
                "event": "user_voice_command",
                "payload": { "text": "hủy đi" }
            })
            .to_string(),
        ))
        .await
        .expect("cancel draft by voice");
    let cancellation = receive_text_event(&mut client, "ai_spoken_response").await;
    assert_eq!(
        cancellation
            .pointer("/payload/text")
            .and_then(|text| text.as_str()),
        Some("Mình đã hủy bản nháp, chưa gửi tin nhắn.")
    );

    let pending_count: i64 = state
        .db
        .writer
        .get()
        .expect("writer connection")
        .query_row("SELECT COUNT(*) FROM message_outbox", [], |row| row.get(0))
        .expect("count pending drafts");
    assert_eq!(pending_count, 0, "cancel must remove the only draft");

    client.close(None).await.expect("close client");
    server_task.abort();
}

#[tokio::test]
async fn aborting_server_closes_active_connections() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let server_task = tokio::spawn(server.run(test_state()));
    let (mut client, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect to reusable server");

    server_task.abort();
    let _ = server_task.await;

    let terminal = tokio::time::timeout(std::time::Duration::from_secs(1), client.next())
        .await
        .expect("active connection survived after its server was aborted");
    assert!(
        !matches!(terminal, Some(Ok(Message::Binary(_) | Message::Text(_)))),
        "aborting the server must terminate active connection tasks"
    );
}

#[tokio::test]
async fn oversized_text_messages_are_rejected_before_json_parsing() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let server_task = tokio::spawn(server.run(test_state()));
    let (mut client, _) = connect_async(format!("ws://{address}/ws"))
        .await
        .expect("connect to reusable server");

    client
        .send(Message::Text("x".repeat(1024 * 1024 + 1)))
        .await
        .expect("client can write oversized frame");

    let terminal = tokio::time::timeout(std::time::Duration::from_secs(1), client.next())
        .await
        .expect("server kept an oversized connection alive");
    assert!(
        matches!(terminal, None | Some(Ok(Message::Close(_))) | Some(Err(_))),
        "oversized text must terminate the connection"
    );

    server_task.abort();
}

#[tokio::test]
async fn loopback_handshake_rejects_self_declared_privileged_principal() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let server_task = tokio::spawn(server.run(test_state()));

    for principal in ["widget", "dashboard"] {
        let denied = connect_async(format!("ws://{address}/ws?principal={principal}"))
            .await
            .expect_err("self-declared privileged principal must fail");
        let tokio_tungstenite::tungstenite::Error::Http(response) = denied else {
            panic!("expected HTTP principal rejection");
        };
        assert_eq!(response.status().as_u16(), 403);
    }

    server_task.abort();
}

#[tokio::test]
async fn loopback_session_ticket_is_single_use_and_derives_principal_server_side() {
    let server = liva_native_core::websocket::WebSocketServer::bind("127.0.0.1:0")
        .await
        .expect("bind reusable WebSocket server");
    let address = server.local_addr();
    let sessions = server.session_authority();
    let ticket = sessions
        .issue(liva_native_core::CommandPrincipal::WebSocketDashboard)
        .expect("issue dashboard ticket");
    let server_task = tokio::spawn(server.run(test_state()));
    let url = format!("ws://{address}/ws?session={}", ticket.token);

    let (mut authorized, _) = connect_async(&url)
        .await
        .expect("fresh ticket must connect");
    authorized.close(None).await.unwrap();

    let replay = connect_async(&url)
        .await
        .expect_err("consumed ticket must reject replay");
    let tokio_tungstenite::tungstenite::Error::Http(response) = replay else {
        panic!("expected HTTP session rejection");
    };
    assert_eq!(response.status().as_u16(), 403);
    server_task.abort();
}

#[tokio::test]
async fn non_loopback_rejects_missing_token_and_accepts_exact_bearer() {
    let token = "0123456789abcdef0123456789abcdef";
    let server = liva_native_core::websocket::WebSocketServer::bind_with_auth(
        "0.0.0.0:0",
        Some(token.to_string()),
    )
    .await
    .expect("bind authenticated non-loopback server");
    let port = server.local_addr().port();
    let server_task = tokio::spawn(server.run(test_state()));
    let url = format!("ws://127.0.0.1:{port}/ws");

    let denied = connect_async(&url)
        .await
        .expect_err("missing token must fail");
    let tokio_tungstenite::tungstenite::Error::Http(response) = denied else {
        panic!("expected HTTP authentication rejection");
    };
    assert_eq!(response.status().as_u16(), 401);

    let mut request = url.into_client_request().unwrap();
    request
        .headers_mut()
        .insert("authorization", format!("Bearer {token}").parse().unwrap());
    let (mut authorized, _) = connect_async(request)
        .await
        .expect("exact bearer token must connect");
    authorized.close(None).await.unwrap();
    server_task.abort();
}
