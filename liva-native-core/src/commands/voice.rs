//! Miền `voice:*` — STT streaming, ngôn ngữ, giọng VieNeu, và phát TTS.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 2). Chín nhánh, chuyển
//! **nguyên văn** — không sửa hành vi nào, để hồi quy (nếu có) quy được về việc
//! dời chứ không phải việc sửa.

use crate::{AppState, config_file_path, tts, update_config_file_at};
use serde_json::{Value, json};
use std::sync::Arc;

/// `verb` là phần sau `voice:` — `"stt_chunk"`, `"tts_speak"`, …
pub async fn handle(state: Arc<AppState>, verb: &str, payload: Value) -> Result<Value, String> {
    match verb {
        "stt_start" | "stt_flush" => {
            state.stt.lock().await.reset_stream();
            Ok(json!({ "success": true }))
        }
        "stt_chunk" => stt_chunk(state, payload).await,
        "stt_stop" => stt_stop(state).await,
        "set_language" => set_language(state, payload).await,
        "list_vieneu_voices" => list_vieneu_voices(state).await,
        "set_vieneu_voice" => set_vieneu_voice(state, payload).await,
        "tts_speak" => tts_speak(state, payload).await,
        "tts_stop" => tts_stop(state).await,
        _ => Err(format!("Unknown command: voice:{verb}")),
    }
}

async fn stt_chunk(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    use base64::Engine;

    let chunk_b64 = payload["chunk"]
        .as_str()
        .ok_or_else(|| "Missing 'chunk'".to_string())?;
    let is_last = payload["isLast"].as_bool().unwrap_or(false);

    let audio_bytes = base64::engine::general_purpose::STANDARD
        .decode(chunk_b64)
        .map_err(|e| format!("Base64 decode failed: {}", e))?;

    let len_rounded = (audio_bytes.len() / 4) * 4;
    let audio_bytes_aligned = &audio_bytes[..len_rounded];
    let audio_samples: Vec<f32> =
        if (audio_bytes_aligned.as_ptr() as usize).is_multiple_of(std::mem::align_of::<f32>()) {
            bytemuck::cast_slice(audio_bytes_aligned).to_vec()
        } else {
            audio_bytes_aligned
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        };

    let text = tokio::task::spawn_blocking(move || {
        let mut stt = state.stt.blocking_lock();
        stt.feed_audio(&audio_samples, is_last)
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(json!({ "text": text }))
}

async fn stt_stop(state: Arc<AppState>) -> Result<Value, String> {
    let text = tokio::task::spawn_blocking(move || {
        let mut stt = state.stt.blocking_lock();
        stt.feed_audio(&[], true)
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(json!({ "text": text }))
}

async fn set_language(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let lang = payload["language"]
        .as_str()
        .ok_or_else(|| "Missing 'language'".to_string())?;

    state.stt.lock().await.set_language(lang)?;
    {
        let mut tts = state.tts.lock().await;
        if let Some(ref mut tts_mgr) = *tts {
            tts_mgr.set_language(lang);
        }
    }
    Ok(json!({ "success": true, "language": lang }))
}

async fn list_vieneu_voices(state: Arc<AppState>) -> Result<Value, String> {
    // Chỉ đọc JSON, không nạp ONNX — nên trả lời được cả khi VieNeu
    // đang TẮT. Đó là điều kiện để màn chọn giọng hiện danh sách trước
    // khi người dùng quyết định bật.
    let voices = tokio::task::spawn_blocking(tts::list_vieneu_voices)
        .await
        .map_err(|error| format!("Voice catalogue task failed: {error}"))??;
    let current = {
        let guard = state.tts.lock().await;
        guard
            .as_ref()
            .and_then(|manager| manager.vieneu_voice_name())
    };
    Ok(json!({
        "enabled": current.is_some(),
        "current": current,
        "voices": voices,
    }))
}

async fn set_vieneu_voice(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let voice = payload["voice"].as_str().map(str::to_string);
    // Vắng `enabled` = "giữ nguyên", khác hẳn `false` = "tắt đi".
    let want_enabled = payload["enabled"].as_bool();
    if voice.is_none() && want_enabled.is_none() {
        return Err("Cần ít nhất 'voice' hoặc 'enabled'".to_string());
    }

    // Kiểm tên giọng TRƯỚC khi ghi cấu hình. Ghi một tên sai xuống
    // liva-config.json thì lần khởi động sau VieNeu nạp thất bại rồi
    // im lặng rơi về Piper — triệu chứng hiện ra rất xa nguyên nhân.
    if let Some(ref name) = voice {
        let known = tokio::task::spawn_blocking(tts::list_vieneu_voices)
            .await
            .map_err(|error| format!("Voice catalogue task failed: {error}"))??;
        if !known.iter().any(|info| &info.name == name) {
            return Err(format!(
                "Giọng '{name}' không có trong danh mục — gọi voice:list_vieneu_voices để xem danh sách"
            ));
        }
    }

    let mut patch = serde_json::Map::new();
    if let Some(ref name) = voice {
        patch.insert("vieneuVoice".to_string(), json!(name));
    }
    if let Some(on) = want_enabled {
        patch.insert("vieneuEnabled".to_string(), json!(on));
    }
    let path = config_file_path();
    let config_patch = json!({ "tts": Value::Object(patch) });
    tokio::task::spawn_blocking(move || update_config_file_at(&path, &config_patch))
        .await
        .map_err(|error| format!("Config writer task failed: {error}"))??;

    // ── áp dụng ngay, không bắt người dùng khởi động lại ───────────
    let loaded = {
        let guard = state.tts.lock().await;
        guard
            .as_ref()
            .and_then(|manager| manager.vieneu_voice_name())
            .is_some()
    };
    let applied = match (want_enabled, loaded) {
        (Some(false), _) => {
            let mut guard = state.tts.lock().await;
            if let Some(manager) = guard.as_mut() {
                manager.set_vieneu_engine(None);
            }
            "đã tắt VieNeu"
        }
        (_, true) => {
            if let Some(ref name) = voice {
                let mut guard = state.tts.lock().await;
                let manager = guard.as_mut().ok_or("TTS engine not initialized")?;
                manager.set_vieneu_voice(name)?;
            }
            "đã đổi giọng ngay"
        }
        (Some(true), false) => {
            // Nạp ~500 MB trọng số: bắt buộc ra khỏi luồng async, nếu
            // không sẽ chẹn cả runtime trong ~2 giây.
            let wanted = voice.clone();
            let engine =
                tokio::task::spawn_blocking(move || tts::load_vieneu_engine(wanted.as_deref()))
                    .await
                    .map_err(|error| format!("VieNeu load task failed: {error}"))??;
            let mut guard = state.tts.lock().await;
            let manager = guard.as_mut().ok_or("TTS engine not initialized")?;
            manager.set_vieneu_engine(Some(engine));
            "đã bật và nạp VieNeu"
        }
        (None, false) => "đã lưu cấu hình; VieNeu đang tắt nên chưa áp dụng",
    };

    let current = {
        let guard = state.tts.lock().await;
        guard
            .as_ref()
            .and_then(|manager| manager.vieneu_voice_name())
    };
    Ok(json!({
        "success": true,
        "applied": applied,
        "enabled": current.is_some(),
        "current": current,
    }))
}

async fn tts_speak(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let text = payload["text"]
        .as_str()
        .ok_or_else(|| "Missing 'text'".to_string())?;

    let flush = payload["flush"].as_bool().unwrap_or(false);

    let mut tts = state.tts.lock().await;
    if let Some(ref mut tts_mgr) = *tts {
        tts_mgr.speak(text).await?;
        if flush {
            tts_mgr.flush().await?;
        }
        Ok(json!({ "success": true }))
    } else {
        Err("TTS engine not initialized".to_string())
    }
}

async fn tts_stop(state: Arc<AppState>) -> Result<Value, String> {
    state.tts_player.stop().await;

    let mut tts = state.tts.lock().await;
    if let Some(ref mut tts_mgr) = *tts {
        tts_mgr.stop().await;
    }

    Ok(json!({ "success": true }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crypto::EncryptionEngine;
    use crate::{db, llm, stt};
    use std::time::Duration;

    fn test_state() -> Arc<AppState> {
        let db = db::DatabasePool::new_in_memory().expect("in-memory database");
        let stt_manager = stt::SttManager::new("non-existent-model");
        let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
        let mock_capturer = Arc::new(crate::vision::capture::MockScreenCapturer::new(
            64,
            64,
            crate::vision::capture::PixelFormat::Rgba,
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
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            embedder: tokio::sync::Mutex::new(None),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                mock_capturer,
                crate::vision::VisionConfig::default(),
            )),
        })
    }

    /// The start signal prevents a scheduler delay from making the timeout
    /// assertion pass without the command ever reaching `tts_stop`.
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn tts_stop_khong_duoc_bao_success_truoc_khi_don_dep_xong() {
        let state = test_state();
        let guard = state.tts.lock().await;

        let command_state = Arc::clone(&state);
        let (started_tx, started_rx) = tokio::sync::oneshot::channel::<()>();
        let mut command = tokio::spawn(async move {
            let _ = started_tx.send(());
            handle(command_state, "tts_stop", json!({})).await
        });
        started_rx.await.expect("command task must start");

        let early = tokio::time::timeout(Duration::from_millis(300), &mut command).await;
        assert!(
            early.is_err(),
            "voice:tts_stop returned while state.tts was still locked: {early:?}"
        );

        drop(guard);

        let result = tokio::time::timeout(Duration::from_secs(5), command)
            .await
            .expect("tts_stop must finish after the lock is released")
            .expect("command task must not panic")
            .expect("tts_stop must succeed");
        assert_eq!(result["success"], json!(true));
    }
}
