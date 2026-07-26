use bytes::{Bytes, BytesMut};
use futures_util::{SinkExt, StreamExt};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::frame::{OP_AUTH_HANDSHAKE, VoiceFrame};
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;
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
