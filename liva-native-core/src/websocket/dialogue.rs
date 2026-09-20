use crate::AppState;
use crate::agent::graph::ConversationMemoryScope;
use crate::messaging::{VoiceMessageAction, VoiceMessageDialogue};
use crate::webrtc::pipeline::WebRTCPipelineHandle;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::warn;

async fn send_event(text_tx: &mpsc::Sender<String>, event: &str, payload: serde_json::Value) {
    let _ = text_tx
        .send(
            serde_json::json!({
                "event": event,
                "payload": payload,
            })
            .to_string(),
        )
        .await;
}

/// Xử lý một lượt hội thoại nhắn tin bằng giọng nói.
///
/// `None` nghĩa là câu nói không thuộc luồng nhắn tin và không có hội thoại
/// nhắn tin nào đang chờ. Mọi đường gửi thật đều đi qua `message:confirm`;
/// `Draft` chỉ ghi outbox và đọc lại cho người dùng xác nhận.
async fn handle_voice_message_turn(
    state: Arc<AppState>,
    dialogue: &mut VoiceMessageDialogue,
    user_text: &str,
) -> Option<String> {
    use crate::messaging::contacts::Platform;

    let action = match crate::agent::graph::route_intent(user_text) {
        crate::agent::graph::Intent::SendMessage {
            recipient,
            body,
            platform,
        } => {
            // Một lệnh nhắn tin đầy đủ mới thay thế hội thoại dở trước đó.
            dialogue.clear();
            let platform = platform.and_then(|value| Platform::parse(&value).ok());
            Some(dialogue.begin(recipient, body, platform))
        }
        _ if dialogue.is_pending() => dialogue.follow_up(user_text),
        _ => None,
    }?;

    let response = match action {
        VoiceMessageAction::AskPlatform => "Bạn muốn nhắn bằng Messenger hay Telegram?".to_string(),
        VoiceMessageAction::AskBody => "Bạn muốn nhắn nội dung gì?".to_string(),
        VoiceMessageAction::RepeatConfirmation => {
            "Bạn nói “gửi đi” để xác nhận, hoặc nói “hủy” để bỏ bản nháp.".to_string()
        }
        VoiceMessageAction::Draft {
            recipient,
            body,
            platform,
        } => {
            let result = crate::commands::messaging::handle(
                state,
                "message:draft",
                serde_json::json!({
                    "to": recipient,
                    "text": body,
                    "platform": platform.as_str(),
                }),
            )
            .await;

            match result {
                Ok(value) if value.get("needsConfirm").and_then(|v| v.as_bool()) == Some(true) => {
                    let Some(draft_id) = value
                        .pointer("/draft/draft_id")
                        .and_then(|v| v.as_str())
                        .map(str::to_string)
                    else {
                        dialogue.clear();
                        return Some(
                            "Mình đã tạo bản nháp nhưng không đọc được mã xác nhận, nên chưa gửi."
                                .to_string(),
                        );
                    };
                    dialogue.await_confirmation(draft_id);
                    let display_name = value
                        .pointer("/draft/display_name")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&recipient);
                    let draft_text = value
                        .pointer("/draft/text")
                        .and_then(|v| v.as_str())
                        .unwrap_or(&body);
                    let platform_name = match platform {
                        Platform::Messenger => "Messenger",
                        Platform::Telegram => "Telegram",
                    };
                    format!(
                        "Mình sẽ gửi cho {display_name} qua {platform_name}: “{draft_text}”. Bạn nói “gửi đi” để xác nhận hoặc “hủy”."
                    )
                }
                Ok(value) if value.get("ambiguous").and_then(|v| v.as_bool()) == Some(true) => {
                    dialogue.clear();
                    format!(
                        "Có nhiều người tên {recipient} trên nền tảng này. Bạn hãy nói rõ tên người nhận hơn."
                    )
                }
                Ok(_) => {
                    dialogue.clear();
                    format!(
                        "Chưa có ai tên {recipient} trên nền tảng này trong danh bạ, nên mình chưa gửi."
                    )
                }
                Err(error) => {
                    dialogue.clear();
                    format!("Mình không tạo được bản nháp cho {recipient}: {error}")
                }
            }
        }
        VoiceMessageAction::Confirm { draft_id } => {
            match crate::commands::messaging::handle(
                state,
                "message:confirm",
                serde_json::json!({ "draftId": draft_id }),
            )
            .await
            {
                Ok(value) if value.get("sent").and_then(|v| v.as_bool()) == Some(true) => value
                    .get("detail")
                    .and_then(|v| v.as_str())
                    .map(|detail| format!("{detail}."))
                    .unwrap_or_else(|| "Tin nhắn đã được gửi.".to_string()),
                Ok(_) => "Hệ thống chưa xác nhận được việc gửi tin nhắn.".to_string(),
                Err(error) => format!("Mình chưa gửi được tin nhắn: {error}"),
            }
        }
        VoiceMessageAction::Cancel { draft_id } => {
            match crate::commands::messaging::handle(
                state,
                "message:cancel",
                serde_json::json!({ "draftId": draft_id }),
            )
            .await
            {
                Ok(value) if value.get("cancelled").and_then(|v| v.as_bool()) == Some(true) => {
                    "Mình đã hủy bản nháp, chưa gửi tin nhắn.".to_string()
                }
                Ok(_) => "Bản nháp không còn tồn tại; mình không gửi gì thêm.".to_string(),
                Err(error) => format!("Mình chưa hủy được bản nháp: {error}"),
            }
        }
    };

    Some(response)
}

/// Hoàn tất một lượt lệnh thoại sau khi nhánh vision đã được loại trừ.
///
/// State machine nhắn tin được ưu tiên trước hội thoại LLM thông thường để một
/// câu xác nhận/hủy không lọt vào prompt và không thể gửi ngoài `message:confirm`.
pub(super) async fn handle_user_voice_text(
    state: Arc<AppState>,
    voice_message_dialogue: Arc<Mutex<VoiceMessageDialogue>>,
    user_text: String,
    memory_scope: ConversationMemoryScope,
    text_tx: mpsc::Sender<String>,
    pipeline_handle: WebRTCPipelineHandle,
) {
    let message_response = {
        let mut dialogue = voice_message_dialogue.lock().await;
        handle_voice_message_turn(state.clone(), &mut dialogue, &user_text).await
    };
    if let Some(response) = message_response {
        send_event(
            &text_tx,
            "ai_spoken_response",
            serde_json::json!({ "text": response }),
        )
        .await;
        if let Err(error) = pipeline_handle.speak_text(response) {
            warn!("Không xếp được câu trả lời TTS: {error}");
        }
        send_event(&text_tx, "ai_thinking_end", serde_json::json!({})).await;
        return;
    }

    // U22: Active Recall (Spaced Retrieval) can thiệp trước khi gọi LLM (0 token cost).
    let session_id = format!(
        "{}:{}",
        memory_scope.storage_domain(),
        memory_scope.storage_category()
    );
    if let Some(recall_reply) =
        state
            .active_recall
            .try_intercept_turn(&user_text, &session_id, &state.db, &state.crypto)
    {
        send_event(
            &text_tx,
            "ai_spoken_response",
            serde_json::json!({ "text": &recall_reply }),
        )
        .await;
        if let Err(error) = pipeline_handle.speak_text(recall_reply) {
            warn!("Không xếp được câu trả lời TTS cho active recall: {error}");
        }
        send_event(&text_tx, "ai_thinking_end", serde_json::json!({})).await;
        return;
    }

    let do_kho = crate::agent::graph::phan_loai_do_kho(&user_text);
    let co_expert = crate::paths::configured_expert_model_path().is_some_and(|p| p.exists());
    let goi_y_expert = matches!(do_kho, crate::agent::graph::DoKho::Kho) && co_expert;

    if goi_y_expert {
        send_event(
            &text_tx,
            "ai_expert_suggestion",
            serde_json::json!({
                "goi_y_expert": true,
                "do_kho": "kho",
                "user_text": &user_text,
            }),
        )
        .await;
    }

    // Giữ bộ nhớ của đường thoại đồng nhất với chat chữ.
    let mut messages = vec![crate::llm::ChatMessage {
        role: "system".to_string(),
        content: crate::llm::persona::PERSONA_LIVA.to_string(),
    }];
    if let Some(memories) =
        crate::agent::graph::recall_context_scoped(&state, &user_text, &memory_scope).await
    {
        messages.push(crate::llm::ChatMessage {
            role: "system".to_string(),
            content: crate::agent::graph::memory_system_message(&memories),
        });
    }
    messages.push(crate::llm::ChatMessage {
        role: "user".to_string(),
        content: user_text.clone(),
    });

    let start_instant = std::time::Instant::now();
    let (model_tx, model_rx) = tokio::sync::oneshot::channel();

    let state_persist = state.clone();
    let text_tx_inner = text_tx.clone();
    let completion_res = tokio::task::spawn_blocking(move || {
        let mut llm_manager = state.llm.blocking_lock();
        // U14: Tự động tráo đổi router <-> expert model theo do_kho và chính sách chống dao động
        // Thực hiện NGAY trong khi đang giữ lock, loại bỏ hoàn toàn race condition check-then-act.
        let _ = llm_manager.maybe_auto_swap_blocking(do_kho);
        let _ = model_tx.send(llm_manager.current_model_path.to_string_lossy().to_string());
        let mut stream_state = crate::llm::engine::CompletionStream::new(&text_tx_inner);
        let completion = llm_manager.generate_budgeted_completion(
            &messages,
            crate::llm::persona::TEMP_DEFAULT,
            crate::llm::persona::TOP_P_DEFAULT,
            |token| {
                let chunk = serde_json::json!({
                    "event": "ai_stream_chunk",
                    "payload": {
                        "textChunk": token,
                        "isThought": false,
                    }
                });
                stream_state.forward(token, &chunk)
            },
        );
        stream_state.finish(completion)
    })
    .await;

    let model_id = model_rx.await.unwrap_or_default();
    let latency_ms = start_instant.elapsed().as_millis() as i64;
    let (final_text, response_ok, prompt_tokens, completion_tokens, err_kind) = match completion_res
    {
        Ok(Ok(ref output)) => (
            output.text.clone(),
            true,
            output.prompt_tokens as i64,
            output.completion_tokens as i64,
            None,
        ),
        Ok(Err(ref error)) => (
            super::loi_chat_thanh_cau_noi(Some(error)),
            false,
            0,
            0,
            Some("llm_error".to_string()),
        ),
        Err(ref e) => (
            super::loi_chat_thanh_cau_noi(None),
            false,
            0,
            0,
            Some(format!("panic: {e}")),
        ),
    };

    let now_ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64;
    let record = crate::db::TurnTelemetryRecord {
        id: None,
        event_id: None,
        ts: now_ts,
        entry_path: "voice".to_string(),
        model_id,
        prompt_tokens,
        completion_tokens,
        latency_ms,
        outcome: if response_ok {
            "ok".to_string()
        } else {
            "err".to_string()
        },
        err_kind,
    };
    let db = state_persist.db.clone();
    tokio::spawn(async move {
        if let Err(e) = db
            .spawn_writer(move |conn| crate::db::record_turn_telemetry(conn, &record))
            .await
        {
            tracing::warn!("Failed to record voice turn telemetry: {e}");
        }
    });

    if response_ok {
        crate::agent::graph::persist_turn_scoped(
            &state_persist,
            &user_text,
            &final_text,
            &memory_scope,
        )
        .await;
    }

    send_event(
        &text_tx,
        "ai_spoken_response",
        serde_json::json!({ "text": final_text }),
    )
    .await;
    send_event(&text_tx, "ai_thinking_end", serde_json::json!({})).await;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent::graph::{ConversationMemoryScope, state_khong_co_embedder};
    use crate::messaging::VoiceMessageDialogue;
    use crate::webrtc::pipeline::{PipelineState, WebRTCPipelineHandle};
    use std::sync::Arc;
    use tokio::sync::{Mutex, mpsc, watch};

    fn make_test_pipeline_handle() -> WebRTCPipelineHandle {
        let (event_tx, _event_rx) = mpsc::channel(32);
        let (_state_tx, state_rx) = watch::channel(PipelineState::Idle);
        WebRTCPipelineHandle { event_tx, state_rx }
    }

    #[tokio::test]
    async fn test_handle_user_voice_text_regular_prompt_no_expert_suggestion() {
        let state = state_khong_co_embedder();
        let dialogue = Arc::new(Mutex::new(VoiceMessageDialogue::default()));
        let scope = ConversationMemoryScope::new("test_owner", "test_chat").unwrap();
        let (text_tx, mut text_rx) = mpsc::channel(32);
        let pipeline_handle = make_test_pipeline_handle();

        let prompt = "Xin chào LIVA, hôm nay thế nào?".to_string();

        handle_user_voice_text(state, dialogue, prompt, scope, text_tx, pipeline_handle).await;

        // Drain text_rx and verify no ai_expert_suggestion event was emitted
        let mut events = Vec::new();
        while let Ok(msg) = text_rx.try_recv() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg)
                && let Some(evt) = v.get("event").and_then(|e| e.as_str())
            {
                events.push(evt.to_string());
            }
        }

        assert!(
            !events.contains(&"ai_expert_suggestion".to_string()),
            "Câu chào hỏi thông thường không được phát event ai_expert_suggestion"
        );
    }

    #[tokio::test]
    async fn test_handle_user_voice_text_voice_message_intent_no_expert_suggestion() {
        let state = state_khong_co_embedder();
        let dialogue = Arc::new(Mutex::new(VoiceMessageDialogue::default()));
        let scope = ConversationMemoryScope::new("test_owner", "test_chat").unwrap();
        let (text_tx, mut text_rx) = mpsc::channel(32);
        let pipeline_handle = make_test_pipeline_handle();

        let prompt = "Nhắn tin cho Minh bảo tối nay đi ăn nhé".to_string();

        handle_user_voice_text(state, dialogue, prompt, scope, text_tx, pipeline_handle).await;

        let mut events = Vec::new();
        while let Ok(msg) = text_rx.try_recv() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg)
                && let Some(evt) = v.get("event").and_then(|e| e.as_str())
            {
                events.push(evt.to_string());
            }
        }

        assert!(
            !events.contains(&"ai_expert_suggestion".to_string()),
            "Voice message turn không được phát event ai_expert_suggestion"
        );
        assert!(
            events.contains(&"ai_spoken_response".to_string()),
            "Voice message turn phải phát ai_spoken_response"
        );
    }

    #[tokio::test]
    async fn test_handle_user_voice_text_complex_prompt_suggestion_matches_expert_presence() {
        let state = state_khong_co_embedder();
        let dialogue = Arc::new(Mutex::new(VoiceMessageDialogue::default()));
        let scope = ConversationMemoryScope::new("test_owner", "test_chat").unwrap();
        let (text_tx, mut text_rx) = mpsc::channel(32);
        let pipeline_handle = make_test_pipeline_handle();

        let prompt =
            "Viết hàm quicksort bằng Rust và giải thích chi tiết thuật toán từng bước?".to_string();

        handle_user_voice_text(
            state,
            dialogue,
            prompt.clone(),
            scope,
            text_tx,
            pipeline_handle,
        )
        .await;

        let mut expert_suggestion_payload = None;
        while let Ok(msg) = text_rx.try_recv() {
            if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg)
                && v.get("event").and_then(|e| e.as_str()) == Some("ai_expert_suggestion")
            {
                expert_suggestion_payload = v.get("payload").cloned();
            }
        }

        let co_expert = crate::paths::configured_expert_model_path().is_some_and(|p| p.exists());
        if co_expert {
            assert!(
                expert_suggestion_payload.is_some(),
                "Khi có expert model trên máy, câu hỏi phức tạp phải phát ai_expert_suggestion"
            );
            let payload = expert_suggestion_payload.unwrap();
            assert_eq!(
                payload.get("goi_y_expert").and_then(|v| v.as_bool()),
                Some(true)
            );
            assert_eq!(payload.get("do_kho").and_then(|v| v.as_str()), Some("kho"));
            assert_eq!(
                payload.get("user_text").and_then(|v| v.as_str()),
                Some(prompt.as_str())
            );
        } else {
            assert!(
                expert_suggestion_payload.is_none(),
                "Khi không có expert model trên máy, không được phát ai_expert_suggestion"
            );
        }
    }
}
