use crate::AppState;
use crate::agent::graph::ConversationMemoryScope;
use crate::messaging::{VoiceMessageAction, VoiceMessageDialogue};
use crate::webrtc::pipeline::WebRTCPipelineHandle;
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tracing::{error, warn};

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

    let compiled_prompt = match crate::llm::compile_prompt(&messages) {
        Ok(prompt) => prompt,
        Err(error) => {
            error!("Failed to compile prompt: {error}");
            send_event(&text_tx, "ai_thinking_end", serde_json::json!({})).await;
            return;
        }
    };

    let state_persist = state.clone();
    let text_tx_inner = text_tx.clone();
    let completion_res = tokio::task::spawn_blocking(move || {
        let mut llm_manager = state.llm.blocking_lock();
        llm_manager.generate_completion(
            &compiled_prompt,
            crate::llm::persona::TEMP_DEFAULT,
            crate::llm::persona::TOP_P_DEFAULT,
            |token| {
                if token.is_empty() {
                    return true;
                }
                let chunk = serde_json::json!({
                    "event": "ai_stream_chunk",
                    "payload": {
                        "textChunk": token,
                        "isThought": false,
                    }
                });
                if let Ok(chunk_str) = serde_json::to_string(&chunk) {
                    let _ = text_tx_inner.blocking_send(chunk_str);
                }
                true
            },
        )
    })
    .await;

    let (final_text, response_ok) = match completion_res {
        Ok(Ok(output)) => (output.text, true),
        Ok(Err(ref error)) => (super::loi_chat_thanh_cau_noi(Some(error)), false),
        Err(_) => (super::loi_chat_thanh_cau_noi(None), false),
    };

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
