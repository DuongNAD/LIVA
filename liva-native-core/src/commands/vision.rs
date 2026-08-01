//! Miền `vision:*` — chụp màn hình, vùng theo dõi, và hỏi-đáp đa phương thức.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1, miền đầu tiên). Sáu nhánh, chuyển
//! **nguyên văn** — không sửa hành vi nào trong đợt này, để nếu có hồi quy thì
//! biết chắc nó đến từ việc dời chứ không từ việc sửa.

use crate::{AppState, DiffEngine, Frame, RegionDiffResult, ScreenRegion, VisionConfig, llm};
use serde_json::{Value, json};
use std::sync::Arc;

/// `verb` là phần sau `vision:` — `"capture"`, `"ask"`, …
pub async fn handle(state: Arc<AppState>, verb: &str, payload: Value) -> Result<Value, String> {
    match verb {
        "capture" => capture(state).await,
        "add_region" => add_region(state, payload).await,
        "remove_region" => remove_region(state, payload).await,
        "get_changed_regions" => get_changed_regions(state).await,
        "set_config" => set_config(state, payload).await,
        "ask" => ask(state, payload).await,
        _ => Err(format!("Unknown command: vision:{verb}")),
    }
}

async fn capture(state: Arc<AppState>) -> Result<Value, String> {
    use base64::Engine;

    let capturer = {
        let vision = state.vision.lock().await;
        vision.capturer()
    };
    let frame = tokio::task::spawn_blocking(move || capturer.capture().map_err(|e| e.to_string()))
        .await
        .map_err(|e| format!("Join error: {}", e))??;

    {
        let mut vision = state.vision.lock().await;
        vision.update_last_frame(frame.clone());
    }

    // Nén PNG thay vì base64 pixel thô.
    //
    // Bản trước base64 thẳng `frame.data`: ở 1920x1080 BGRA đó là
    // 8,3 MB thô -> **~11 MB base64** nhét trong MỘT thông điệp JSON.
    // Đủ để làm nghẽn socket và ngốn bộ nhớ cả hai đầu.
    //
    // PNG không tốn thêm dependency nào: `image` đã nằm sẵn trong cây
    // phụ thuộc qua `xcap` (thư viện chụp màn hình), và nó vốn đã kéo
    // theo codec `png`.
    //
    // CẢ BA bước đều nặng CPU và đều phải nằm trong `spawn_blocking`:
    // đổi định dạng pixel (~8 MB), nén PNG, rồi base64. Để bất kỳ bước
    // nào chạy thẳng trên luồng async là chặn cả runtime — nghĩa là mọi
    // phiên thoại đang chạy đứng hình trong lúc xử lý một khung full-HD.
    let (width, height) = (frame.width, frame.height);
    let raw_len = frame.data.len();
    let (png_len, b64_data) =
        tokio::task::spawn_blocking(move || -> Result<(usize, String), String> {
            let (w, h, rgb) = crate::vision::capture::frame_to_rgb(&frame);
            let buf = image::RgbImage::from_raw(w, h, rgb)
                .ok_or_else(|| format!("Kich thuoc RGB khong khop {}x{}", w, h))?;
            let mut out = std::io::Cursor::new(Vec::new());
            buf.write_to(&mut out, image::ImageFormat::Png)
                .map_err(|e| format!("Ma hoa PNG that bai: {}", e))?;
            let png = out.into_inner();
            let b64 = base64::engine::general_purpose::STANDARD.encode(&png);
            Ok((png.len(), b64))
        })
        .await
        .map_err(|e| format!("Join error: {}", e))??;

    Ok(json!({
        "width": width,
        "height": height,
        // "png" — KHÔNG còn là tên biến thể PixelFormat như bản trước.
        // Client cũ đọc trường này để biết cách bóc pixel; nay `data`
        // là một file PNG hoàn chỉnh, giải bằng bộ giải ảnh thông thường.
        "format": "png",
        "data": b64_data,
        // Để đo được mức lợi mà không phải đoán.
        "raw_bytes": raw_len,
        "png_bytes": png_len,
    }))
}

async fn add_region(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let region: ScreenRegion =
        serde_json::from_value(payload).map_err(|e| format!("Invalid region payload: {}", e))?;
    let mut vision = state.vision.lock().await;
    vision.add_region(region)?;
    Ok(json!({ "success": true }))
}

async fn remove_region(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let id = payload["id"]
        .as_str()
        .ok_or_else(|| "Missing 'id' in payload".to_string())?;
    let mut vision = state.vision.lock().await;
    vision.remove_region(id)?;
    Ok(json!({ "success": true }))
}

async fn get_changed_regions(state: Arc<AppState>) -> Result<Value, String> {
    let (capturer, last_frame, regions, color_tolerance) = {
        let vision = state.vision.lock().await;
        (
            vision.capturer(),
            vision.last_frame(),
            vision.regions(),
            vision.color_tolerance(),
        )
    };

    let (current_frame, results) =
        tokio::task::spawn_blocking(move || -> Result<(Frame, Vec<RegionDiffResult>), String> {
            let current_frame = capturer.capture().map_err(|e| e.to_string())?;
            let prev_frame = match &last_frame {
                Some(f) => f,
                None => {
                    let baseline = regions
                        .iter()
                        .map(|r| RegionDiffResult {
                            region_id: r.id.clone(),
                            name: r.name.clone(),
                            difference: 1.0,
                            is_changed: true,
                        })
                        .collect();
                    return Ok((current_frame, baseline));
                }
            };

            let mut results = Vec::with_capacity(regions.len());
            for region in &regions {
                let res =
                    DiffEngine::diff_region(prev_frame, &current_frame, region, color_tolerance)?;
                results.push(res);
            }
            Ok((current_frame, results))
        })
        .await
        .map_err(|e| format!("Join error: {}", e))??;

    {
        let mut vision = state.vision.lock().await;
        vision.update_last_frame(current_frame);
    }

    Ok(serde_json::to_value(results).unwrap())
}

async fn set_config(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let config: VisionConfig =
        serde_json::from_value(payload).map_err(|e| format!("Invalid config payload: {}", e))?;
    let mut vision = state.vision.lock().await;
    vision.set_config(config);
    Ok(json!({ "success": true }))
}

/// Hỏi-đáp đa phương thức trên một ảnh, bằng lõi VL hợp nhất (Qwen3-VL).
/// Nguồn ảnh: `image` base64 (png/jpg) nếu có, không thì chụp màn hình chính.
async fn ask(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let question = payload["question"]
        .as_str()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt.")
        .to_string();
    let temperature = payload["temperature"].as_f64().unwrap_or(0.7) as f32;
    let top_p = payload["top_p"].as_f64().unwrap_or(0.8) as f32;
    let image_b64 = payload["image"].as_str().map(|s| s.to_string());

    let output = tokio::task::spawn_blocking(move || -> Result<llm::CompletionOutput, String> {
        use base64::Engine as _;
        let mut llm_manager = state.llm.blocking_lock();
        if let Some(b64) = image_b64 {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(b64.as_bytes())
                .map_err(|e| format!("Invalid base64 image: {}", e))?;
            llm_manager.answer_with_image(
                &question,
                llm::engine::VisionImage::Encoded(&bytes),
                temperature,
                top_p,
                |_| true,
            )
        } else {
            // Context-aware capture (mouse-guided crop while gaming).
            let (width, height, rgb) = crate::vision::capture::capture_for_vision()?;
            llm_manager.answer_with_image(
                &question,
                llm::engine::VisionImage::Rgb {
                    width,
                    height,
                    data: &rgb,
                },
                temperature,
                top_p,
                |_| true,
            )
        }
    })
    .await
    .map_err(|e| format!("Blocking task panicked: {}", e))??;

    Ok(json!({
        "text": output.text,
        "usage": {
            "prompt_tokens": output.prompt_tokens,
            "completion_tokens": output.completion_tokens
        }
    }))
}
