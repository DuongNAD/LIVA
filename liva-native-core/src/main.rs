use liva_native_core::{
    boot::{self, ServiceOptions},
    handle_command,
};

use serde::{Deserialize, Serialize};
use tokio::io::{self, AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{error, info};
use tracing_subscriber::FmtSubscriber;

/// Tải model từ dòng lệnh (`--setup-models`). Cùng lý do đặt ở binary như
/// trước đây của `preflight`; phần logic dùng chung với vỏ Tauri nằm ở
/// `liva_native_core::setup`.
mod setup_cli;

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
    // `--preflight` chạy TRƯỚC mọi khởi tạo: không runtime Tokio, không DB,
    // không nạp model. Đó là cả điểm của nó — phải trả lời được "máy này thiếu
    // gì" trên đúng cái máy chưa boot nổi. Nạp model ở đây là tự thua.
    if std::env::args().skip(1).any(|a| a == "--preflight") {
        std::process::exit(liva_native_core::preflight::chay());
    }

    // `--setup-models` cũng chạy TRƯỚC mọi khởi tạo, và vì cùng một lý do: nó
    // tồn tại để dùng trên đúng cái máy chưa có model, tức là cái máy mà đường
    // khởi động bình thường chưa chạy được gì. Runtime một luồng là đủ — công
    // việc ở đây là I/O mạng, không phải tính toán.
    if std::env::args().skip(1).any(|a| a == "--setup-models") {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("không dựng được runtime cho --setup-models");
        std::process::exit(rt.block_on(setup_cli::chay()));
    }

    let worker_threads = std::env::var("LIVA_TOKIO_WORKER_THREADS")
        .ok()
        .and_then(|val| val.parse::<usize>().ok())
        .unwrap_or_else(|| {
            std::thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(4)
        });

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

/// Thoát sạch với một chẩn đoán rõ ràng thay vì `panic!`.
///
/// Vì sao (lộ trình 0.6): các `.expect()` lúc boot dựng backtrace Rust — nhiễu,
/// và với người dùng thường thì hoàn toàn không gợi ý được cách khắc phục. Ở
/// đây in một dòng lỗi có hành động cụ thể ra **stderr** (stdout dành cho IPC)
/// rồi `exit(1)`. Vỏ Tauri hiện lỗi này lên dialog là việc follow-up (cần
/// quyết định UI); binary standalone thì stderr + mã thoát ≠ 0 chính là "UI".
fn die(context: &str, err: impl std::fmt::Display) -> ! {
    tracing::error!("KHỞI ĐỘNG THẤT BẠI — {context}: {err}");
    eprintln!("\n❌ LIVA không khởi động được.\n   {context}:\n   {err}\n");
    std::process::exit(1);
}

async fn async_main() {
    // Initialize tracing to stderr so it doesn't pollute stdout (which is used for IPC)
    //
    // Filter đến từ `RUST_LOG` qua `tracing_env_filter()` (chính sách dùng chung
    // với vỏ Tauri). Trước đây chỗ này là `.with_max_level(Level::INFO)` cứng,
    // nên `RUST_LOG` bị bỏ qua và mọi `debug!` trong crate là code chết. Mặc
    // định vẫn là `info` — không đổi hành vi của ai đang chạy.
    let subscriber = FmtSubscriber::builder()
        .with_env_filter(liva_native_core::tracing_env_filter())
        .with_writer(std::io::stderr)
        .finish();
    // Chưa có logger ở đây nên không dùng `die`; nếu cái này hỏng thì đằng nào
    // cũng không log được gì — panic là hợp lý duy nhất còn lại.
    tracing::subscriber::set_global_default(subscriber)
        .expect("không đặt được tracing subscriber (chỉ xảy ra khi đã có subscriber khác)");

    info!("LIVA Native Core starting up...");

    // Dựng AppState bằng đường DÙNG CHUNG với vỏ Tauri (`boot::build_app_state`).
    // Trước đây khối này là ~155 dòng chép gần nguyên sang liva-desktop, và hai
    // bản sao đã trôi lệch — xem bảng ở đầu `boot.rs`.
    let boot = boot::build_app_state().unwrap_or_else(|e| die(&e.context, e.detail));
    let liva_native_core::boot::Boot {
        state,
        escrow_hex,
        crypto_source,
        crypto_rekeyed,
        crypto_locked,
        audio_stream,
        llm_n_gpu_layers,
    } = boot;

    // Giữ OutputStream sống suốt đời tiến trình — drop nó là LIVA câm.
    let _stream = audio_stream;

    if let Some(hex) = &escrow_hex {
        // Gateway: escrow ra stderr (stdout dành cho IPC). Vỏ Tauri hiện dialog.
        liva_native_core::keystore::show_message_box(
            "LIVA - BACK UP ENCRYPTION KEY",
            &liva_native_core::escrow_message(hex),
        );
    }
    info!(
        "Khoá mã hoá: nguồn={}, rekey {} fact, {} bản khoá-chết (không mất, đọc lại được khi đúng khoá)",
        crypto_source, crypto_rekeyed, crypto_locked
    );

    // Kênh ghi stdout phải dựng TRƯỚC dịch vụ nền: bot Telegram đẩy vài thông
    // điệp qua đây.
    let (tx, mut rx) = mpsc::channel::<String>(100);

    let background_tasks = boot::spawn_background_services(
        state.clone(),
        ServiceOptions {
            ipc_tx: Some(tx.clone()),
            // Gateway không có cửa sổ để báo "đã sẵn sàng".
            on_gateway_ready: None,
            // Gateway độc lập không sở hữu WebView Tauri tin cậy.
            on_websocket_sessions_ready: None,
            llm_n_gpu_layers,
        },
    );

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

    // Stop every process-owned service before closing stdout. Telegram owns a
    // sender for its whole polling lifetime; leaving it detached would keep
    // `rx` open forever after EOF. The other handles are drained here as well
    // so model, WebSocket and projection resources do not rely on runtime drop.
    boot::stop_background_services(background_tasks).await;

    // Drop the main sender so rx knows no more messages are coming after all processing tasks finish
    drop(tx);

    // Wait for writer task to finish writing all pending responses
    let _ = writer_handle.await;

    info!("LIVA Native Core shutting down...");
}

#[cfg(test)]
mod main_tests;
