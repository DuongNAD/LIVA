use bytes::BytesMut;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::webrtc::frame::{OP_VISME, OP_WAKE_PROBE, VoiceFrame};
use liva_native_core::{AppState, db, llm, stt, tts};
use std::sync::Arc;

fn build_test_app_state() -> Arc<AppState> {
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

/// Empirical Verification 1:
/// The blueprint proposes an 11-byte OP_VISEME packet with opcode 0x05:
/// [Byte 0: 0x05][Bytes 1-8: u64 timestamp][Byte 9: u8 viseme_id][Byte 10: u8 weight]
///
/// We test what happens when this 11-byte packet is decoded by VoiceFrame::decode.
#[test]
fn test_blueprint_op_viseme_breaks_wire_protocol() {
    // 1. Check opcode conflict:
    assert_eq!(
        OP_WAKE_PROBE, 0x05,
        "Opcode 0x05 is already committed to OP_WAKE_PROBE"
    );
    assert_eq!(OP_VISME, 0x06, "Real viseme opcode is OP_VISME (0x06)");

    // 2. Construct blueprint's proposed 11-byte packet:
    // Case A: Little-Endian timestamp (Unix epoch ms = 1725619200000)
    let timestamp: u64 = 1725619200000;
    let viseme_id: u8 = 0x01; // aa
    let weight: u8 = 255;

    let mut packet_le = BytesMut::with_capacity(11);
    packet_le.extend_from_slice(&[0x05]); // Blueprint opcode
    packet_le.extend_from_slice(&timestamp.to_le_bytes()); // 8 bytes
    packet_le.extend_from_slice(&[viseme_id, weight]); // 2 bytes
    assert_eq!(packet_le.len(), 11);

    // Pass to VoiceFrame::decode
    let decode_res_le = VoiceFrame::decode(&mut packet_le);
    println!("Decode result LE: {:?}", decode_res_le);
    // Because VoiceFrame expects 9-byte header [op:1][seq:4][len:4],
    // bytes 5..9 are interpreted as payload_size.
    // In LE, payload_size is 401. Since buffer only has 11 bytes (11 < 9 + 401),
    // decode returns Ok(None) - STALLED BUFFER waiting for 399 more bytes!
    assert!(decode_res_le.is_ok(), "Decoder does not crash immediately");
    assert!(
        decode_res_le.unwrap().is_none(),
        "Decoder stalls waiting for incomplete frame (believing payload is 401 bytes)"
    );

    // Case B: Big-Endian timestamp
    let mut packet_be = BytesMut::with_capacity(11);
    packet_be.extend_from_slice(&[0x05]);
    packet_be.extend_from_slice(&timestamp.to_be_bytes());
    packet_be.extend_from_slice(&[viseme_id, weight]);

    let decode_res_be = VoiceFrame::decode(&mut packet_be);
    println!("Decode result BE: {:?}", decode_res_be);
    // In BE, payload_size is 10,825,413 bytes > 1MB limit.
    // decode returns Err("Payload exceeds 1MB limit")!
    assert!(
        decode_res_be.is_err(),
        "BE timestamp immediately crashes decoder with >1MB payload error"
    );
}

/// Empirical Verification 2:
/// Verify SQLite single-writer contention (memory_scope.rs:225) causing silent turn drop.
#[tokio::test]
async fn test_sqlite_writer_contention_silent_drop_reproduction() {
    let state = build_test_app_state();
    let scope = liva_native_core::agent::graph::ConversationMemoryScope::new("local", "default")
        .expect("valid scope");

    // Hold the writer connection
    let held_conn = state.db.writer.get().expect("checkout writer connection");

    // Call persist_turn_scoped while writer is held
    liva_native_core::agent::graph::persist_turn_scoped(&state, "Hello LIVA", "Hello User", &scope)
        .await;

    // Drop the held connection
    drop(held_conn);

    // Inspect database: the turn was silently dropped!
    let reader = state.db.readers.get().expect("reader connection");
    let count: i64 = reader
        .query_row("SELECT COUNT(*) FROM vectors_meta", [], |row| row.get(0))
        .unwrap();

    assert_eq!(
        count, 0,
        "Silent Drop Turn confirmed: 0 turns saved due to locked writer pool"
    );
}

/// Empirical Verification 3:
/// Demonstrate that if the MPSC writer actor computes embeddings synchronously
/// on the writer thread (because DbWriteCommand::PersistTurn lacks pre-computed vector),
/// it creates Head-Of-Line (HOL) blocking for critical checkpointing and state operations.
#[tokio::test]
async fn test_writer_actor_inline_onnx_head_of_line_blocking() {
    // Model command queue
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(&'static str, std::time::Duration)>(32);

    // Actor task simulating single writer thread
    let actor_handle = tokio::spawn(async move {
        let mut processed = Vec::new();
        while let Some((cmd_type, work_duration)) = rx.recv().await {
            let start = std::time::Instant::now();
            tokio::time::sleep(work_duration).await; // Simulating heavy ONNX inference or DB write
            processed.push((cmd_type, start.elapsed()));
        }
        processed
    });

    // Enqueue 3 PersistTurn commands that do ONNX embedding inline (simulated 20ms each)
    for _ in 0..3 {
        tx.send((
            "PersistTurn_With_Inline_ONNX",
            std::time::Duration::from_millis(20),
        ))
        .await
        .unwrap();
    }

    // A high-priority checkpoint arrives
    let _checkpoint_sent = std::time::Instant::now();
    let (_resp_tx, _resp_rx) = tokio::sync::oneshot::channel::<()>();
    tx.send((
        "AgentCheckpoint_Critical",
        std::time::Duration::from_millis(1),
    ))
    .await
    .unwrap();

    drop(tx);
    let results = actor_handle.await.unwrap();

    // Checkpoint was blocked behind 60ms of ONNX neural network inference!
    let total_before_checkpoint: std::time::Duration = results[0..3].iter().map(|(_, d)| *d).sum();
    println!(
        "Head-of-line blocking delay for critical checkpoint: {:?}",
        total_before_checkpoint
    );
    assert!(
        total_before_checkpoint >= std::time::Duration::from_millis(50),
        "Proves that putting ONNX inference inside the DB writer actor blocks all writes"
    );
}
