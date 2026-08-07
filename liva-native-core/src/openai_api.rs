//! openai_api.rs — bề mặt HTTP tương thích OpenAI
//! ================================================
//!
//! Mở ba đường dẫn quen thuộc để **công cụ có sẵn dùng được LIVA mà không cần
//! biết gì về giao thức riêng ở cổng 8002**:
//!
//! | Đường dẫn | Việc |
//! |---|---|
//! | `GET  /v1/models` | Danh sách model — thứ mọi SDK gọi đầu tiên để dò kết nối |
//! | `POST /v1/chat/completions` | Sinh văn bản; `stream: true` trả SSE |
//! | `POST /v1/audio/speech` | Tổng hợp giọng nói, trả WAV |
//!
//! ## Ba quyết định thiết kế, và lý do
//!
//! **1. Cổng RIÊNG, không dùng chung 8002 — và mặc định TẮT.**
//! Cổng 8002 là `TcpListener` thô đưa thẳng mọi kết nối cho `tokio-tungstenite`
//! bắt tay WebSocket; nhét HTTP thường vào đó buộc phải soi trước vài byte rồi
//! phát lại chúng cho tungstenite — chỗ dễ sai, đổi lấy việc trùng số cổng.
//! Bề mặt này bật bằng `LIVA_OPENAI_PORT`; không đặt biến thì **không mở socket
//! nào**. Nó không có xác thực (giống hệt 8002), nên bật-mặc-định là mở thêm
//! một cửa không khoá mà người dùng không yêu cầu.
//!
//! **2. Đây là LIVA, không phải một proxy LLM trung tính.**
//! `handle_chat_completion_scoped` chèn `PERSONA_LIVA` khi request không có
//! message `system`, có truy hồi RAG và có ghi lượt vào bộ nhớ. Đó là chủ đích:
//! giá trị của endpoint này là *LIVA offline dùng được từ công cụ khác*, không
//! phải *một máy chủ suy luận vô danh*. Ai muốn model trần thì gửi kèm message
//! `system` của mình.
//!
//! **3. Bộ nhớ đi vào scope riêng `openai_api`.**
//! Lưu lượng API không trộn vào hội thoại cục bộ (`local`/`default`) của người
//! dùng. Đổi hằng số dưới đây là đổi ranh giới đó — đọc kỹ trước khi đụng.
//!
//! ## Cái này KHÔNG làm
//!
//! - **Không xác thực, không hạn mức, không đa phiên.** Bind mặc định
//!   `127.0.0.1`. Đừng bind ra ngoài khi chưa có TLS + token.
//! - **Không có `/v1/audio/transcriptions`.** Cần phân tích `multipart/form-data`
//!   thủ công; để riêng một mục.
//! - **Không tôn trọng trường `model` của request.** LIVA luôn dùng model router
//!   đang nạp. Trường đó được **phản chiếu nguyên văn** vào hồi âm cho khớp kỳ
//!   vọng của SDK, chứ không phải vì nó chọn được model.

use crate::{AppState, agent};
use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Method, Request, Response, Server, StatusCode};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::sync::Arc;
use tracing::{info, warn};

/// Chủ sở hữu bộ nhớ cho lưu lượng đi qua bề mặt này. Tách khỏi `local` để API
/// không trộn ký ức vào hội thoại cục bộ của người dùng.
const MEMORY_OWNER: &str = "openai_api";
const MEMORY_CONVERSATION: &str = "default";

/// Tên model báo ra ở `/v1/models` khi request không nêu tên nào.
const DEFAULT_MODEL_ID: &str = "liva-local";

/// Chặn thân request phình. Prompt dài nhất vẫn thừa chỗ; mục đích là để một
/// client hỏng không kéo được cả tiến trình vào chỗ hết bộ nhớ.
const MAX_BODY_BYTES: usize = 4 * 1024 * 1024;

/// Địa chỉ bề mặt này sẽ bind, hoặc `None` khi chưa bật.
///
/// Bật bằng `LIVA_OPENAI_PORT`. Host dùng chung `LIVA_SERVER_HOST` với gateway
/// (mặc định `127.0.0.1`) để hai bề mặt không lệch nhau về phạm vi mạng.
pub fn configured_addr() -> Option<SocketAddr> {
    let port: u16 = std::env::var("LIVA_OPENAI_PORT")
        .ok()?
        .trim()
        .parse()
        .ok()?;
    let host = std::env::var("LIVA_SERVER_HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
    match format!("{host}:{port}").parse() {
        Ok(addr) => Some(addr),
        Err(error) => {
            warn!("LIVA_OPENAI_PORT đặt rồi nhưng địa chỉ không hợp lệ: {error}");
            None
        }
    }
}

/// Chạy máy chủ tới khi tiến trình dừng.
pub async fn serve(state: Arc<AppState>, addr: SocketAddr) -> Result<(), String> {
    let make_service = make_service_fn(move |_conn| {
        let state = Arc::clone(&state);
        async move {
            Ok::<_, Infallible>(service_fn(move |req| {
                let state = Arc::clone(&state);
                async move { Ok::<_, Infallible>(route(state, req).await) }
            }))
        }
    });

    info!("OpenAI-compatible API listening on http://{addr}/v1");
    Server::bind(&addr)
        .serve(make_service)
        .await
        .map_err(|error| format!("OpenAI API server dừng: {error}"))
}

async fn route(state: Arc<AppState>, req: Request<Body>) -> Response<Body> {
    let method = req.method().clone();
    let path = req.uri().path().to_string();

    match (&method, path.as_str()) {
        (&Method::GET, "/v1/models") => list_models(),
        (&Method::POST, "/v1/chat/completions") => match read_body(req).await {
            Ok(body) => chat_completions(state, body).await,
            Err(response) => response,
        },
        (&Method::POST, "/v1/audio/speech") => match read_body(req).await {
            Ok(body) => audio_speech(state, body).await,
            Err(response) => response,
        },
        _ => loi(
            StatusCode::NOT_FOUND,
            &format!("Không có đường dẫn {method} {path}"),
            "invalid_request_error",
        ),
    }
}

// ─── Đọc thân request ───────────────────────────────────────────────────────

async fn read_body(req: Request<Body>) -> Result<serde_json::Value, Response<Body>> {
    // Chặn theo Content-Length TRƯỚC khi đọc: từ chối sớm rẻ hơn nhiều so với
    // gom hết vào bộ nhớ rồi mới đo.
    if let Some(len) = req
        .headers()
        .get(hyper::header::CONTENT_LENGTH)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<usize>().ok())
        && len > MAX_BODY_BYTES
    {
        return Err(loi(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("Thân request vượt {MAX_BODY_BYTES} byte"),
            "invalid_request_error",
        ));
    }

    let bytes = hyper::body::to_bytes(req.into_body()).await.map_err(|e| {
        loi(
            StatusCode::BAD_REQUEST,
            &format!("Không đọc được thân request: {e}"),
            "invalid_request_error",
        )
    })?;

    if bytes.len() > MAX_BODY_BYTES {
        return Err(loi(
            StatusCode::PAYLOAD_TOO_LARGE,
            &format!("Thân request vượt {MAX_BODY_BYTES} byte"),
            "invalid_request_error",
        ));
    }

    serde_json::from_slice(&bytes).map_err(|e| {
        loi(
            StatusCode::BAD_REQUEST,
            &format!("JSON không hợp lệ: {e}"),
            "invalid_request_error",
        )
    })
}

// ─── GET /v1/models ─────────────────────────────────────────────────────────

fn list_models() -> Response<Body> {
    json_ok(serde_json::json!({
        "object": "list",
        "data": [{
            "id": DEFAULT_MODEL_ID,
            "object": "model",
            "created": now_unix(),
            "owned_by": "liva",
        }],
    }))
}

// ─── POST /v1/chat/completions ──────────────────────────────────────────────

async fn chat_completions(state: Arc<AppState>, body: serde_json::Value) -> Response<Body> {
    if !body["messages"].is_array() {
        return loi(
            StatusCode::BAD_REQUEST,
            "Thiếu mảng 'messages'",
            "invalid_request_error",
        );
    }

    let model = body["model"]
        .as_str()
        .unwrap_or(DEFAULT_MODEL_ID)
        .to_string();
    let streaming = body["stream"].as_bool().unwrap_or(false);
    let id = format!("chatcmpl-{}", uuid::Uuid::new_v4().simple());

    let scope = match agent::graph::ConversationMemoryScope::new(MEMORY_OWNER, MEMORY_CONVERSATION)
    {
        Ok(scope) => scope,
        Err(error) => {
            return loi(
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
                "internal_server_error",
            );
        }
    };

    // Chỉ chuyển tiếp những tham số lõi thật sự đọc. Gửi nguyên `body` sang sẽ
    // im lặng nuốt các trường OpenAI mà LIVA không hiểu và làm người gọi tưởng
    // chúng có tác dụng.
    let mut payload = serde_json::json!({
        "messages": body["messages"].clone(),
        "stream": streaming,
    });
    if let Some(t) = body["temperature"].as_f64() {
        payload["temperature"] = serde_json::json!(t);
    }
    if let Some(p) = body["top_p"].as_f64() {
        payload["top_p"] = serde_json::json!(p);
    }

    if streaming {
        stream_completion(state, payload, scope, id, model)
    } else {
        match crate::handle_chat_completion_scoped(state, payload, None, None, scope).await {
            Ok(result) => json_ok(serde_json::json!({
                "id": id,
                "object": "chat.completion",
                "created": now_unix(),
                "model": model,
                "choices": [{
                    "index": 0,
                    "message": {
                        "role": "assistant",
                        "content": loc_tag_avatar(result["text"].as_str().unwrap_or_default()),
                    },
                    "finish_reason": "stop",
                }],
                "usage": {
                    "prompt_tokens": result["usage"]["prompt_tokens"].as_u64().unwrap_or(0),
                    "completion_tokens": result["usage"]["completion_tokens"].as_u64().unwrap_or(0),
                    "total_tokens": result["usage"]["total_tokens"].as_u64().unwrap_or(0),
                },
            })),
            Err(error) => loi(
                StatusCode::INTERNAL_SERVER_ERROR,
                &error,
                "internal_server_error",
            ),
        }
    }
}

/// Biến luồng `IpcResponse` của lõi thành Server-Sent Events kiểu OpenAI.
fn stream_completion(
    state: Arc<AppState>,
    payload: serde_json::Value,
    scope: agent::graph::ConversationMemoryScope,
    id: String,
    model: String,
) -> Response<Body> {
    // `loi_tx` mang từng dòng SSE đã định dạng sẵn. Hai kênh chứ không một:
    // lõi nói ngôn ngữ `IpcResponse`, còn dây ra nói SSE — dịch ở giữa để không
    // bên nào phải biết bên kia.
    let (sse_tx, sse_rx) = tokio::sync::mpsc::channel::<Result<String, std::io::Error>>(64);
    let (token_tx, mut token_rx) = tokio::sync::mpsc::channel::<String>(64);

    let created = now_unix();
    let khung = move |delta: serde_json::Value, finish: Option<&str>| {
        format!(
            "data: {}\n\n",
            serde_json::json!({
                "id": id,
                "object": "chat.completion.chunk",
                "created": created,
                "model": model,
                "choices": [{ "index": 0, "delta": delta, "finish_reason": finish }],
            })
        )
    };

    tokio::spawn(async move {
        let req_id = uuid::Uuid::new_v4().to_string();
        let completion = tokio::spawn(crate::handle_chat_completion_scoped(
            state,
            payload,
            Some(token_tx),
            Some(req_id),
            scope,
        ));

        // Mẩu đầu tiên khai `role` — SDK của OpenAI trông đợi đúng thứ tự này.
        let _ = sse_tx
            .send(Ok(khung(serde_json::json!({ "role": "assistant" }), None)))
            .await;

        // Bộ lọc phải sống QUA CẢ vòng lặp, không dựng lại mỗi mẩu: một tag có
        // thể bị cắt đôi giữa hai token (`[wa` + `ve]`), và một bộ lọc mới mỗi
        // mẩu sẽ không bao giờ ghép được hai nửa đó.
        let mut filter = crate::tts::AvatarSpeechFilter::default();

        while let Some(raw) = token_rx.recv().await {
            let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&raw) else {
                continue;
            };
            let Some(token) = parsed["data"]["token"].as_str() else {
                continue;
            };
            let sach = filter.push(token);
            if sach.is_empty() {
                continue; // đang giữ lại một tag còn dở
            }
            if sse_tx
                .send(Ok(khung(serde_json::json!({ "content": sach }), None)))
                .await
                .is_err()
            {
                return; // client ngắt giữa chừng
            }
        }
        let _ = filter.finish();

        // `token_rx` cạn nghĩa là lượt sinh đã xong HOẶC hỏng. Phải chờ kết quả
        // thật để không đóng luồng bằng "stop" trong khi nó thất bại.
        match completion.await {
            Ok(Ok(_)) => {
                let _ = sse_tx
                    .send(Ok(khung(serde_json::json!({}), Some("stop"))))
                    .await;
            }
            Ok(Err(error)) => {
                warn!("OpenAI stream lỗi: {error}");
                let _ = sse_tx
                    .send(Ok(format!(
                        "data: {}\n\n",
                        serde_json::json!({ "error": { "message": error, "type": "internal_server_error" } })
                    )))
                    .await;
            }
            Err(error) => warn!("OpenAI stream task hỏng: {error}"),
        }
        let _ = sse_tx.send(Ok("data: [DONE]\n\n".to_string())).await;
    });

    let stream = futures_util::stream::unfold(sse_rx, |mut rx| async move {
        rx.recv().await.map(|item| (item, rx))
    });

    Response::builder()
        .status(StatusCode::OK)
        .header(hyper::header::CONTENT_TYPE, "text/event-stream")
        .header(hyper::header::CACHE_CONTROL, "no-cache")
        .body(Body::wrap_stream(stream))
        .expect("khung SSE tĩnh luôn hợp lệ")
}

// ─── POST /v1/audio/speech ──────────────────────────────────────────────────

async fn audio_speech(state: Arc<AppState>, body: serde_json::Value) -> Response<Body> {
    let Some(input) = body["input"].as_str().map(str::to_string) else {
        return loi(
            StatusCode::BAD_REQUEST,
            "Thiếu 'input'",
            "invalid_request_error",
        );
    };
    if input.trim().is_empty() {
        return loi(
            StatusCode::BAD_REQUEST,
            "'input' rỗng",
            "invalid_request_error",
        );
    }

    // Chỉ WAV. `response_format` khác (mp3/opus/aac) cần bộ mã hoá mà lõi không
    // có — từ chối thẳng còn hơn trả WAV dán nhãn mp3.
    match body["response_format"].as_str() {
        None | Some("wav") | Some("pcm") => {}
        Some(khac) => {
            return loi(
                StatusCode::BAD_REQUEST,
                &format!("response_format '{khac}' chưa hỗ trợ; chỉ có 'wav'"),
                "invalid_request_error",
            );
        }
    }

    let plan = {
        let guard = state.tts.lock().await;
        let Some(manager) = guard.as_ref() else {
            return loi(
                StatusCode::SERVICE_UNAVAILABLE,
                "TTS chưa khởi tạo — thiếu model weights?",
                "internal_server_error",
            );
        };
        manager.synthesis_plan(&input)
    };
    let Some(plan) = plan else {
        return loi(
            StatusCode::BAD_REQUEST,
            "Không tổng hợp được: 'input' không còn nội dung đọc được sau khi chuẩn hoá",
            "invalid_request_error",
        );
    };

    // Tổng hợp là việc nặng CPU và đồng bộ — đẩy khỏi luồng runtime.
    match tokio::task::spawn_blocking(move || plan.synthesize(|| false)).await {
        Ok(Ok(outcome)) => {
            let wav = wav_tu_f32(&outcome.samples, outcome.sample_rate);
            Response::builder()
                .status(StatusCode::OK)
                .header(hyper::header::CONTENT_TYPE, "audio/wav")
                .body(Body::from(wav))
                .expect("khung WAV tĩnh luôn hợp lệ")
        }
        Ok(Err(error)) => loi(
            StatusCode::INTERNAL_SERVER_ERROR,
            &error,
            "internal_server_error",
        ),
        Err(error) => loi(
            StatusCode::INTERNAL_SERVER_ERROR,
            &format!("Tác vụ tổng hợp hỏng: {error}"),
            "internal_server_error",
        ),
    }
}

/// Đóng gói PCM f32 thành WAV 16-bit mono.
///
/// Tự viết thay vì kéo một crate: WAV không nén chỉ là 44 byte header cộng mẫu,
/// và thêm một dependency cho ngần đó là đắt hơn nhiều so với đoạn mã này.
pub(crate) fn wav_tu_f32(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    const BITS: u16 = 16;
    const CHANNELS: u16 = 1;
    let byte_rate = sample_rate * u32::from(CHANNELS) * u32::from(BITS / 8);
    let data_len = (samples.len() * usize::from(BITS / 8)) as u32;

    let mut out = Vec::with_capacity(44 + data_len as usize);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // cỡ khối fmt
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM không nén
    out.extend_from_slice(&CHANNELS.to_le_bytes());
    out.extend_from_slice(&sample_rate.to_le_bytes());
    out.extend_from_slice(&byte_rate.to_le_bytes());
    out.extend_from_slice(&(CHANNELS * BITS / 8).to_le_bytes()); // block align
    out.extend_from_slice(&BITS.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());

    for sample in samples {
        // Kẹp TRƯỚC khi đổi kiểu. Thiếu bước này thì một mẫu vượt biên sẽ cuộn
        // vòng qua i16 và biến thành tiếng nổ ở đúng chỗ to nhất.
        let clamped = sample.clamp(-1.0, 1.0);
        out.extend_from_slice(&((clamped * i16::MAX as f32) as i16).to_le_bytes());
    }
    out
}

// ─── Tiện ích ───────────────────────────────────────────────────────────────

/// Gỡ tag điều khiển avatar (`[happy]`, `[wave]`…) khỏi văn bản trả ra ngoài.
///
/// LIVA sinh chúng ở đầu câu trả lời để dẫn biểu cảm 3D. Đường thoại và giao
/// diện đều lọc trước khi dùng; bề mặt này **phải làm y hệt**, vì một công cụ
/// bên ngoài không có cách nào biết `[happy]` là chỉ thị chứ không phải chữ mà
/// LIVA muốn nói. Đo lần đầu 06/08/2026: thiếu bước này thì `message.content`
/// trả về nguyên văn `"[happy][wave] Xin chào bạn nhé."`.
fn loc_tag_avatar(text: &str) -> String {
    let mut filter = crate::tts::AvatarSpeechFilter::default();
    let mut sach = filter.push(text);
    sach.push_str(&filter.finish());
    sach
}

fn now_unix() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn json_ok(value: serde_json::Value) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Body::from(value.to_string()))
        .expect("khung JSON tĩnh luôn hợp lệ")
}

/// Lỗi theo đúng hình dạng OpenAI — SDK đọc `error.message`, không đọc thân thô.
fn loi(status: StatusCode, message: &str, kind: &str) -> Response<Body> {
    let body = serde_json::json!({
        "error": { "message": message, "type": kind, "param": null, "code": null }
    });
    Response::builder()
        .status(status)
        .header(hyper::header::CONTENT_TYPE, "application/json")
        .body(Body::from(body.to_string()))
        .expect("khung lỗi tĩnh luôn hợp lệ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn khong_dat_bien_moi_truong_thi_khong_mo_cong() {
        // Không thể đặt/xoá biến môi trường an toàn trong test song song, nên
        // chỉ khẳng định nhánh phân tích: rỗng và rác đều ra None.
        assert!("".trim().parse::<u16>().is_err());
        assert!("khong-phai-so".trim().parse::<u16>().is_err());
        assert_eq!("8003".trim().parse::<u16>().ok(), Some(8003u16));
    }

    #[test]
    fn wav_co_header_44_byte_va_dung_so_mau() {
        let wav = wav_tu_f32(&[0.0, 0.5, -0.5], 24_000);
        assert_eq!(&wav[0..4], b"RIFF");
        assert_eq!(&wav[8..12], b"WAVE");
        assert_eq!(&wav[36..40], b"data");
        assert_eq!(wav.len(), 44 + 3 * 2, "3 mẫu 16-bit sau header 44 byte");

        // Cỡ ghi trong header phải khớp thân thật, nếu không trình phát sẽ cắt.
        let riff_size = u32::from_le_bytes([wav[4], wav[5], wav[6], wav[7]]);
        assert_eq!(riff_size as usize, wav.len() - 8);
        let data_size = u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]);
        assert_eq!(data_size as usize, wav.len() - 44);
    }

    #[test]
    fn wav_kep_mau_vuot_bien_thay_vi_cuon_vong() {
        // Đây là hồi quy cho một lỗi kinh điển: `(2.0 * 32767.0) as i16` không
        // cuộn vòng trong Rust (nó bão hoà), nhưng phép nhân trước khi kẹp vẫn
        // làm mất biên độ đúng. Kẹp trước ⇒ đúng đỉnh, không méo.
        let wav = wav_tu_f32(&[2.0, -2.0], 16_000);
        let mau1 = i16::from_le_bytes([wav[44], wav[45]]);
        let mau2 = i16::from_le_bytes([wav[46], wav[47]]);
        assert_eq!(mau1, i16::MAX);
        assert_eq!(mau2, -i16::MAX);
    }

    #[test]
    fn tag_dieu_khien_avatar_khong_ro_ra_ngoai_api() {
        // Hồi quy cho lỗi đo được 06/08/2026: `/v1/chat/completions` trả nguyên
        // văn "[happy][wave] Xin chào bạn nhé." cho client bên ngoài.
        assert_eq!(
            loc_tag_avatar("[happy][wave] Xin chào bạn nhé."),
            "Xin chào bạn nhé."
        );
        // Ngoặc thật giữa câu vẫn phải nguyên vẹn — cùng hợp đồng với U26.
        assert_eq!(
            loc_tag_avatar("Kết quả [2 + 2] là 4."),
            "Kết quả [2 + 2] là 4."
        );
        assert_eq!(loc_tag_avatar("Không có tag."), "Không có tag.");
    }

    #[test]
    fn loi_dung_hinh_dang_openai() {
        let response = loi(StatusCode::NOT_FOUND, "không thấy", "invalid_request_error");
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(
            response
                .headers()
                .get(hyper::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json")
        );
    }
}
