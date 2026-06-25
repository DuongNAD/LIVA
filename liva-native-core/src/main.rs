#![allow(dead_code, unused_imports, unused_variables)]
use liva_native_core::{
    crypto, db, llm, stt, tts, webrtc, telegram, AppState, handle_command
};

use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{Level, error, info};
use tracing_subscriber::FmtSubscriber;

#[derive(Debug, Deserialize)]
struct IpcRequest {
    id: String,
    command: String,
    payload: serde_json::Value,
}

#[derive(Debug, Serialize)]
struct IpcResponse {
    id: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
}

fn main() {
    let worker_threads = std::env::var("LIVA_TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

    let max_blocking_threads = std::env::var("LIVA_TOKIO_MAX_BLOCKING_THREADS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or(512);

    let rt = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(worker_threads)
        .max_blocking_threads(max_blocking_threads)
        .enable_all()
        .build()
        .expect("Failed to build Tokio runtime");

    rt.block_on(async_main());
}

async fn async_main() {
    // Initialize tracing to stderr so it doesn't pollute stdout (which is used for IPC)
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_writer(std::io::stderr)
        .finish();
    tracing::subscriber::set_global_default(subscriber).expect("setting default subscriber failed");

    info!("LIVA Native Core starting up...");

    let db_path = std::env::var("LIVA_DB_PATH")
        .unwrap_or_else(|_| "data/agents/liva_core/structured_memory.sqlite".to_string());
    let encryption_key = std::env::var("LIVA_ENCRYPTION_KEY")
        .unwrap_or_else(|_| "00000000000000000000000000000000".to_string());

    if let Some(parent) = std::path::Path::new(&db_path).parent() {
        std::fs::create_dir_all(parent).ok();
    }

    let is_in_memory = std::env::var("LIVA_DB_IN_MEMORY").is_ok();
    let db = if is_in_memory {
        db::DatabasePool::new_in_memory().expect("Failed to initialize in-memory DB")
    } else {
        db::DatabasePool::new(&db_path).expect("Failed to initialize DatabasePool")
    };

    let (_stream, handle) = match rodio::OutputStream::try_default() {
        Ok((s, h)) => (Some(s), Some(h)),
        Err(e) => {
            error!("Failed to initialize default audio output stream: {}", e);
            (None, None)
        }
    };
    let sink = handle.as_ref().and_then(|h| match rodio::Sink::try_new(h) {
        Ok(s) => Some(s),
        Err(e) => {
            error!("Failed to create rodio Sink: {}", e);
            None
        }
    });

    let stt_model_dir = std::env::var("LIVA_STT_MODEL_DIR")
        .unwrap_or_else(|_| "models/nemotron-asr".to_string());
    let tts_model_path = std::env::var("LIVA_TTS_MODEL_PATH")
        .unwrap_or_else(|_| "models/kokoro-v1.0.onnx".to_string());
    let tts_voice_path = std::env::var("LIVA_TTS_VOICE_PATH")
        .unwrap_or_else(|_| "node_modules/kokoro-js/voices/af_heart.bin".to_string());

    let stt_manager = stt::SttManager::new(&stt_model_dir);
    let shared_sink = sink.map(Arc::new);
    let tts_player = tts::audio::TtsAudioPlayer::new(shared_sink.clone());
    let tts_manager = match tts::TtsManager::new(&tts_model_path, &tts_voice_path, shared_sink) {
        Ok(m) => Some(m),
        Err(e) => {
            error!(
                "Failed to initialize TtsManager: {}. TTS commands will fail.",
                e
            );
            None
        }
    };

    let llm_n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .unwrap_or_else(|_| "4096".to_string())
        .parse::<usize>()
        .unwrap_or(4096);
    let llm_n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .unwrap_or_else(|_| "0".to_string())
        .parse::<u32>()
        .unwrap_or(0);
    let llm_manager = llm::LlamaRouterManager::new(llm_n_ctx, llm_n_gpu_layers)
        .expect("Failed to initialize LlamaRouterManager");

    // Initialize VAD Engine globally
    let mut stt_model_dir = std::env::var("LIVA_STT_MODEL_DIR")
        .unwrap_or_else(|_| "models/nemotron-asr".to_string());
    let mut vad_model_path = std::path::Path::new(&stt_model_dir).join("silero_vad.onnx");
    if !vad_model_path.exists() {
        stt_model_dir = "../models/nemotron-asr".to_string();
        vad_model_path = std::path::Path::new(&stt_model_dir).join("silero_vad.onnx");
    }
    let vad_engine = if vad_model_path.exists() {
        match webrtc::vad::VadEngine::new(&vad_model_path, webrtc::vad::VadConfig::default()) {
            Ok(e) => Some(e),
            Err(err) => {
                eprintln!("Failed to initialize VadEngine: {}", err);
                None
            }
        }
    } else {
        eprintln!("VAD model not found at {:?}", vad_model_path);
        None
    };

    let vault_path = std::env::var("LIVA_VAULT_PATH")
        .unwrap_or_else(|_| "E:\\Project\\LIVA\\teamwork_projects\\obsidian_llm_wiki\\vault".to_string());
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(&vault_path));

    let state = Arc::new(AppState {
        db,
        crypto: crypto::EncryptionEngine::new(&encryption_key),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(tts_manager),
        tts_player,
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(vad_engine),
        mcp_server,
    });

    // Spawn WebRTC/IPC WebSocket server
    let state_ws = state.clone();
    tokio::spawn(async move {
        if let Err(e) = start_websocket_server(state_ws).await {
            error!("WebSocket server error: {}", e);
        }
    });

    // Spawn background task for idle TTS model unloading
    let state_unload_clone = state.clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
        loop {
            interval.tick().await;
            let tts_opt = state_unload_clone.tts.lock().await;
            if let Some(ref tts_mgr) = *tts_opt {
                tts_mgr.check_idle_unload();
            }
        }
    });

    // Create an mpsc channel to safely serialize and write responses to stdout
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Spawn background Telegram bot service if token is set
    let telegram_token = std::env::var("TELEGRAM_BOT_TOKEN").ok();
    if let Some(token) = telegram_token {
        let allowed_ids_raw = std::env::var("TELEGRAM_ALLOWED_IDS").unwrap_or_default();
        let allowed_ids: std::collections::HashSet<String> = allowed_ids_raw
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        
        let state_tg = state.clone();
        let tx_tg = tx.clone();
        
        tokio::spawn(async move {
            let manager = Arc::new(telegram::TelegramBotManager::new(
                token,
                allowed_ids,
                state_tg,
                Some(tx_tg),
            ));
            manager.start().await;
        });
    }

    // Spawn stdout writer task
    let writer_handle: tokio::task::JoinHandle<()> = tokio::spawn(async move {
        let mut stdout = io::stdout();
        while let Some(msg) = rx.recv().await {
            let mut bytes = msg.into_bytes();
            bytes.push(b'\n');
            if let Err(e) = stdout.write_all(&bytes).await {
                error!("Failed to write IPC response to stdout: {}", e);
            }
            if let Err(e) = stdout.flush().await {
                error!("Failed to flush stdout: {}", e);
            }
        }
    });

    // Read commands from stdin line-by-line using Tokio async io
    let stdin = io::stdin();
    let reader = BufReader::new(stdin);
    let mut lines = reader.lines();

    loop {
        match lines.next_line().await {
            Ok(Some(line)) => {
                let trim_line = line.trim();
                if trim_line.is_empty() {
                    continue;
                }

                // Parse command
                let req: IpcRequest = match serde_json::from_str(trim_line) {
                    Ok(r) => r,
                    Err(e) => {
                        let err_resp = IpcResponse {
                            id: "unknown".to_string(),
                            status: "error".to_string(),
                            data: None,
                            error: Some(format!("Invalid JSON query: {}", e)),
                        };
                        if let Ok(resp_str) = serde_json::to_string(&err_resp) {
                            let _ = tx.send(resp_str).await;
                        }
                        continue;
                    }
                };

                let req_id = req.id.clone();
                info!("Received command: {} (ID: {})", req.command, req_id);

                let tx_clone = tx.clone();
                let state_clone = state.clone();
                let req_id_clone = req_id.clone();
                // Process request asynchronously
                tokio::spawn(async move {
                    let result = handle_command(
                        state_clone,
                        &req.command,
                        req.payload,
                        Some(tx_clone.clone()),
                        Some(req_id_clone),
                    )
                    .await;

                    let response = match result {
                        Ok(data) => IpcResponse {
                            id: req_id,
                            status: "ok".to_string(),
                            data: Some(data),
                            error: None,
                        },
                        Err(err_msg) => IpcResponse {
                            id: req_id,
                            status: "error".to_string(),
                            data: None,
                            error: Some(err_msg),
                        },
                    };

                    if let Ok(resp_str) = serde_json::to_string(&response) {
                        let _ = tx_clone.send(resp_str).await;
                    }
                });
            }
            Ok(None) => {
                break;
            }
            Err(e) => {
                error!("Error reading from stdin: {}", e);
                break;
            }
        }
    }

    // Drop the main sender so rx knows no more messages are coming after all processing tasks finish
    drop(tx);

    // Wait for writer task to finish writing all pending responses
    let _ = writer_handle.await;

    info!("LIVA Native Core shutting down...");
}

// handle_command is now imported from liva_native_core

async fn start_websocket_server(state: Arc<AppState>) -> Result<(), String> {
    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{Request, Response};

    let addr = "127.0.0.1:8002";
    let listener = TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Failed to bind to {}: {}", addr, e))?;
    info!("WebSocket server listening on ws://{}/ws", addr);

    while let Ok((stream, _)) = listener.accept().await {
        let state_clone = state.clone();
        tokio::spawn(async move {
            let mut is_ws_path = false;
            let callback = |req: &Request, response: Response| {
                if req.uri().path() == "/ws" {
                    is_ws_path = true;
                }
                Ok(response)
            };

            let ws_stream = match accept_hdr_async(stream, callback).await {
                Ok(ws) => ws,
                Err(e) => {
                    error!("WebSocket handshake failed: {}", e);
                    return;
                }
            };

            if !is_ws_path {
                error!("WebSocket connection rejected: invalid path");
                return;
            }

            info!("New WebSocket client connected");
            if let Err(e) = handle_ws_connection(ws_stream, state_clone).await {
                error!("WebSocket connection error: {}", e);
            }
            info!("WebSocket client disconnected");
        });
    }

    Ok(())
}

async fn handle_ws_connection(
    ws_stream: tokio_tungstenite::WebSocketStream<tokio::net::TcpStream>,
    state: Arc<AppState>,
) -> Result<(), String> {
    use futures_util::{SinkExt, StreamExt};
    use tokio::sync::mpsc;
    use bytes::BytesMut;
    use crate::webrtc::frame::{VoiceFrame, OP_AUTH_HANDSHAKE, OP_MIC_IN};
    use crate::webrtc::vad::VadEvent;

    let (mut ws_sender, mut ws_receiver) = ws_stream.split();
    let (outgoing_tx, mut outgoing_rx) = mpsc::channel::<VoiceFrame>(128);

    // Spawn pipeline actor
    let (pipeline_handle, actor) = crate::webrtc::pipeline::WebRTCActor::new(state.clone(), outgoing_tx.clone());
    let actor_handle = tokio::spawn(actor.run());



    // Spawn outgoing message forwarder task
    let send_task = tokio::spawn(async move {
        while let Some(frame) = outgoing_rx.recv().await {
            match frame.encode() {
                Ok(bytes) => {
                    if let Err(e) = ws_sender.send(tokio_tungstenite::tungstenite::Message::Binary(bytes.to_vec())).await {
                        error!("Failed to send binary frame to client: {}", e);
                        break;
                    }
                }
                Err(e) => {
                    error!("Failed to encode frame: {}", e);
                }
            }
        }
    });

    let mut accumulating = false;
    let mut audio_buffer = Vec::new();

    while let Some(msg_res) = ws_receiver.next().await {
        let msg = match msg_res {
            Ok(m) => m,
            Err(e) => {
                error!("WebSocket receive error: {}", e);
                break;
            }
        };

        if msg.is_binary() {
            let data = msg.into_data();
            let mut bytes_mut = BytesMut::from(&data[..]);
            
            while bytes_mut.len() >= 9 {
                let frame = match VoiceFrame::decode(&mut bytes_mut) {
                    Ok(Some(f)) => f,
                    Ok(None) => break,
                    Err(e) => {
                        error!("Frame decode error: {}", e);
                        break;
                    }
                };

                match frame.op_code {
                    OP_AUTH_HANDSHAKE => {
                        // Echo handshake back to acknowledge
                        let handshake_frame = VoiceFrame {
                            op_code: OP_AUTH_HANDSHAKE,
                            seq_id: frame.seq_id,
                            payload: frame.payload.clone(),
                        };
                        let _ = outgoing_tx.send(handshake_frame).await;
                    }
                    OP_MIC_IN => {
                        let payload = &frame.payload;
                        let len_rounded = (payload.len() / 4) * 4;
                        let payload_aligned = &payload[..len_rounded];
                        let samples_vec: Vec<f32> = if payload_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
                            bytemuck::cast_slice(payload_aligned).to_vec()
                        } else {
                            payload_aligned
                                .chunks_exact(4)
                                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                                .collect()
                        };
                        let samples_vec_clone = samples_vec.clone();

                        // Run VAD in blocking task with shared engine
                        let state_clone = state.clone();
                        let events_res = tokio::task::spawn_blocking(move || {
                            let mut vad_guard = state_clone.vad.blocking_lock();
                            if let Some(ref mut vad) = *vad_guard {
                                vad.process_audio(&samples_vec_clone)
                            } else {
                                Ok(Vec::new())
                            }
                        })
                        .await
                        .map_err(|e| format!("VAD task panicked: {}", e))?;

                        let events = events_res.map_err(|e| format!("VAD processing failed: {}", e))?;

                        for (event, _) in events {
                            match event {
                                VadEvent::SpeechStart => {
                                    if let Err(e) = pipeline_handle.on_vad_start() {
                                        error!("Failed on_vad_start: {}", e);
                                    }
                                    accumulating = true;
                                    audio_buffer.clear();
                                    
                                    // Pre-populate with recent samples to avoid clipping initial speech onset
                                    let pre_trigger_len = 1536.min(samples_vec.len());
                                    audio_buffer.extend_from_slice(&samples_vec[samples_vec.len() - pre_trigger_len..]);
                                }
                                VadEvent::SpeechEnd => {
                                    accumulating = false;
                                    let speech_audio = std::mem::take(&mut audio_buffer);
                                    if let Err(e) = pipeline_handle.on_vad_end(speech_audio) {
                                        error!("Failed on_vad_end: {}", e);
                                    }
                                }
                                VadEvent::None => {}
                            }
                        }

                        if accumulating {
                            audio_buffer.extend_from_slice(&samples_vec);
                        }
                    }
                    _ => {}
                }
            }
        } else if msg.is_close() {
            break;
        }
    }

    // Clean up
    let _ = pipeline_handle.on_interrupted();
    send_task.abort();
    actor_handle.abort();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state() -> Arc<AppState> {
        unsafe {
            std::env::set_var("LIVA_ENCRYPTION_KEY", "00000000000000000000000000000000");
        }
        let db = db::DatabasePool::new_in_memory().unwrap();
        let stt_manager = stt::SttManager::new("data/models/nemotron-asr");
        let llm_manager = llm::LlamaRouterManager::new(2048, 0).unwrap();
        Arc::new(AppState {
            db,
            crypto: crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(stt_manager),
            tts: tokio::sync::Mutex::new(None),
            tts_player: tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(llm_manager),
            vad: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        })
    }

    #[tokio::test]
    async fn test_ping() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "ping", payload, None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, serde_json::json!({ "pong": true }));
    }

    #[tokio::test]
    async fn test_echo() {
        let state = test_state();
        let payload = serde_json::json!({ "hello": "world" });
        let res = handle_command(state, "echo", payload.clone(), None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val, payload);
    }

    #[tokio::test]
    async fn test_status() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "status", payload, None, None).await;
        assert!(res.is_ok());
        let val = res.unwrap();
        assert_eq!(val["engine"], "LIVA Native Engine");
        assert_eq!(val["status"], "healthy");
        assert_eq!(val["version"], env!("CARGO_PKG_VERSION"));
    }

    #[tokio::test]
    async fn test_unknown_command() {
        let state = test_state();
        let payload = serde_json::json!({});
        let res = handle_command(state, "unknown", payload, None, None).await;
        assert!(res.is_err());
        assert_eq!(res.err().unwrap(), "Unknown command: unknown");
    }

    #[test]
    fn test_tokio_runtime_env_parsing() {
        unsafe {
            std::env::set_var("LIVA_TOKIO_WORKER_THREADS", "8");
            std::env::set_var("LIVA_TOKIO_MAX_BLOCKING_THREADS", "128");
        }

        let worker_threads = std::env::var("LIVA_TOKIO_WORKER_THREADS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));

        let max_blocking_threads = std::env::var("LIVA_TOKIO_MAX_BLOCKING_THREADS")
            .ok()
            .and_then(|val| val.parse::<usize>().ok())
            .unwrap_or(512);

        assert_eq!(worker_threads, 8);
        assert_eq!(max_blocking_threads, 128);

        unsafe {
            std::env::remove_var("LIVA_TOKIO_WORKER_THREADS");
            std::env::remove_var("LIVA_TOKIO_MAX_BLOCKING_THREADS");
        }
    }
}
