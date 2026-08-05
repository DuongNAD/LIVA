use super::StateGraph;
use super::intent::{Intent, route_intent};
use super::memory_scope::{
    ConversationMemoryScope, memory_system_message, persist_turn_scoped, recall_context_scoped,
};
use crate::AppState;
use crate::agent::state::AgentState;
use serde_json::json;
use std::sync::Arc;
use tokio::sync::mpsc;

const LLM_STREAM_ABORT_PREFIX: &str = "LLM stream aborted";
const LLM_TTS_BACKPRESSURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

pub(super) fn send_llm_chunk_if_current(
    tx: &mpsc::Sender<String>,
    active_session_id: &std::sync::atomic::AtomicU64,
    session_id: u64,
    chunk: &str,
    timeout: std::time::Duration,
) -> Result<(), String> {
    if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
        return Err(format!("{LLM_STREAM_ABORT_PREFIX}: session cancelled"));
    }
    if chunk.is_empty() {
        return Ok(());
    }

    let deadline = std::time::Instant::now() + timeout;

    loop {
        if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
            return Err(format!("{LLM_STREAM_ABORT_PREFIX}: session cancelled"));
        }
        match tx.try_reserve() {
            Ok(permit) => {
                if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err(format!("{LLM_STREAM_ABORT_PREFIX}: session cancelled"));
                }
                permit.send(chunk.to_string());
                return Ok(());
            }
            Err(mpsc::error::TrySendError::Full(_)) => {
                let now = std::time::Instant::now();
                if now >= deadline {
                    return Err(format!(
                        "{LLM_STREAM_ABORT_PREFIX}: TTS chunk queue remained full for {} ms",
                        timeout.as_millis(),
                    ));
                }
                std::thread::sleep(
                    deadline
                        .saturating_duration_since(now)
                        .min(std::time::Duration::from_millis(1)),
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                return Err(format!(
                    "{LLM_STREAM_ABORT_PREFIX}: TTS chunk receiver closed",
                ));
            }
        }
    }
}

pub(super) fn finish_streamed_completion(
    text: String,
    stream_error: Option<String>,
    active_session_id: &std::sync::atomic::AtomicU64,
    session_id: u64,
) -> Result<String, String> {
    if let Some(error) = stream_error {
        return Err(error);
    }
    if active_session_id.load(std::sync::atomic::Ordering::SeqCst) != session_id {
        return Err(format!("{LLM_STREAM_ABORT_PREFIX}: session cancelled"));
    }
    Ok(text)
}

pub fn build_pipeline_graph(
    state_shared: Arc<AppState>,
    memory_scope: ConversationMemoryScope,
    llm_chunk_tx: mpsc::Sender<String>,
    session_id: u64,
    active_session_id: Arc<std::sync::atomic::AtomicU64>,
) -> StateGraph {
    let mut graph = StateGraph::new();

    let ss1 = Arc::clone(&state_shared);
    let tx1 = llm_chunk_tx.clone();
    let as1 = Arc::clone(&active_session_id);
    graph.add_node("router", move |mut state: AgentState| {
        let ss = Arc::clone(&ss1);
        let _tx = tx1.clone();
        let _as = Arc::clone(&as1);
        async move {
            let last_msg = state.messages.last().ok_or("No messages in state")?;
            let text = last_msg
                .get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("")
                .to_string();

            match route_intent(&text) {
                Intent::Vision => {
                    state.current_node = "vision".to_string();
                }
                Intent::SmartHome { device, action } => {
                    state.context.insert("device".to_string(), json!(device));
                    state.context.insert("action".to_string(), json!(action));
                    state.current_node = "tool_exec".to_string();
                }
                Intent::SendMessage {
                    recipient,
                    body,
                    platform,
                } => {
                    state
                        .context
                        .insert("message_to".to_string(), json!(recipient));
                    state
                        .context
                        .insert("message_text".to_string(), json!(body));
                    if let Some(platform) = platform {
                        state
                            .context
                            .insert("message_platform".to_string(), json!(platform));
                    } else {
                        state.context.remove("message_platform");
                    }
                    state.current_node = "message_draft".to_string();
                }
                // Đi qua ĐÚNG đường `mcp_call` mà nhánh LLM dùng, thay vì dựng
                // một nhánh thực thi riêng: hai đường tới cùng một tool phải cho
                // cùng một câu trả lời cho người dùng. Đây là bài học đã ghi ở
                // `control_smarthome` — nếu tách đường, "hai đường khớp nhau"
                // chỉ còn đúng ở tên tool mà sai ở thứ người dùng nghe được.
                Intent::OsControl { tool, action } => {
                    state.context.insert(
                        "mcp_call".to_string(),
                        json!({
                            "server": crate::llm::tool_calling::NATIVE_SERVER,
                            "name": tool,
                            "arguments": { "action": action },
                        }),
                    );
                    state.current_node = "mcp_tool_exec".to_string();
                }
                // G1 — đường nhanh không nhận ra gì, thử để LLM chọn tool từ
                // schema thật. `route_intent` đi TRƯỚC là cố ý: nó không tốn
                // token nào và đã xử lý đúng cách nói tiếng Việt ("bật đèn giúp
                // mình"), nên nó cũng chính là fallback khi vòng LLM không đọc
                // được output. Tắt theo mặc định — xem `tool_calling::enabled`.
                Intent::Chat => {
                    state.current_node =
                        match crate::llm::tool_calling::select_tool(&ss, &text).await {
                            Some(call) => match call.policy {
                                crate::llm::ExecPolicy::Auto => {
                                    state.context.insert(
                                        "mcp_call".to_string(),
                                        json!({
                                            "server": call.server,
                                            "name": call.name,
                                            "arguments": call.arguments,
                                        }),
                                    );
                                    "mcp_tool_exec".to_string()
                                }
                                // Chỉ được đề xuất: đưa đề xuất vào hội thoại rồi để
                                // LLM nói lại cho người dùng. KHÔNG chạy.
                                crate::llm::ExecPolicy::ProposeOnly => {
                                    state.messages.push(json!({
                                        "role": "tool",
                                        "content": call.proposal_text(),
                                    }));
                                    "chat_completion".to_string()
                                }
                            },
                            None => "chat_completion".to_string(),
                        };
                }
            }

            Ok(state)
        }
    });

    // G1 — chạy tool MCP mà LLM đã chọn. Tách khỏi `tool_exec` (đường
    // smart-home theo từ khoá) vì hai nhánh có ranh giới tin cậy khác nhau:
    // nhánh này nhận tên tool + tham số do LLM sinh, nên `execute_call` kiểm lại
    // allowlist một lần nữa ngay trước khi chạy.
    let ss_mcp = Arc::clone(&state_shared);
    graph.add_node("mcp_tool_exec", move |mut state: AgentState| {
        let ss = Arc::clone(&ss_mcp);
        async move {
            let goi = state
                .context
                .get("mcp_call")
                .cloned()
                .ok_or("mcp_call missing in context")?;
            let call = crate::llm::tool_calling::ResolvedCall {
                server: goi["server"].as_str().unwrap_or_default().to_string(),
                name: goi["name"].as_str().unwrap_or_default().to_string(),
                arguments: goi["arguments"].clone(),
                policy: crate::llm::ExecPolicy::Auto,
            };

            let noi_dung = match crate::llm::tool_calling::execute_call(&ss, &call).await {
                Ok(res) => {
                    let text = res
                        .content
                        .iter()
                        .filter_map(|c| match c {
                            crate::mcp::protocol::ToolContent::Text { text } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect::<Vec<_>>()
                        .join("\n");
                    if res.is_error {
                        format!("Công cụ {} báo lỗi: {text}", call.qualified())
                    } else if text.is_empty() {
                        format!(
                            "Công cụ {} chạy xong, không trả về văn bản.",
                            call.qualified()
                        )
                    } else {
                        text
                    }
                }
                // Báo trung thực là KHÔNG chạy được, không im lặng bỏ qua.
                Err(e) => format!("Không chạy được công cụ {}: {e}", call.qualified()),
            };

            state
                .messages
                .push(json!({ "role": "tool", "content": noi_dung }));
            state.current_node = "chat_completion".to_string();
            Ok(state)
        }
    });

    // Nhắn tin: nút này **không gửi gì**. Nó gọi đúng lệnh `message:draft` mà UI
    // gọi — cùng một đường, nên hai lối vào không thể lệch nhau — rồi đẩy kết
    // quả vào hội thoại dưới vai `tool` để `chat_completion` nói lại cho người
    // dùng nghe. Việc gửi chỉ xảy ra khi người dùng bấm xác nhận, tức một lệnh
    // `message:confirm` riêng do UI phát.
    let ss_msg = Arc::clone(&state_shared);
    graph.add_node("message_draft", move |mut state: AgentState| {
        let ss = Arc::clone(&ss_msg);
        async move {
            let to = state
                .context
                .get("message_to")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let text = state
                .context
                .get("message_text")
                .and_then(|v| v.as_str())
                .unwrap_or_default()
                .to_string();
            let platform = state
                .context
                .get("message_platform")
                .and_then(|v| v.as_str())
                .map(str::to_string);

            let noi_dung = if text.trim().is_empty() {
                // Không có nội dung thì hỏi lại, KHÔNG dựng bản nháp rỗng.
                format!(
                    "Người dùng muốn nhắn tin cho '{to}' nhưng chưa nói nội dung. \
                     Hãy hỏi lại họ muốn nhắn gì."
                )
            } else {
                let payload = match platform {
                    Some(platform) => json!({ "to": to, "text": text, "platform": platform }),
                    None => json!({ "to": to, "text": text }),
                };
                match crate::commands::messaging::handle(ss, "message:draft", payload).await {
                    Ok(v) if v.get("needsConfirm").and_then(|b| b.as_bool()) == Some(true) => {
                        format!(
                            "Đã soạn tin cho {} — nội dung: \"{}\". \
                             CHƯA gửi. Hãy báo người dùng đọc lại và bấm xác nhận.",
                            v.pointer("/draft/display_name")
                                .and_then(|s| s.as_str())
                                .unwrap_or(&to),
                            v.pointer("/draft/text")
                                .and_then(|s| s.as_str())
                                .unwrap_or(&text)
                        )
                    }
                    Ok(v) if v.get("ambiguous").and_then(|b| b.as_bool()) == Some(true) => format!(
                        "Có nhiều người tên '{to}' trong danh bạ. Hãy hỏi người dùng \
                         muốn nhắn cho ai."
                    ),
                    Ok(_) => format!(
                        "Chưa có ai tên '{to}' trong danh bạ, nên chưa nhắn được. \
                         Hãy báo người dùng thêm liên hệ này trước."
                    ),
                    Err(e) => format!("Không soạn được tin cho '{to}': {e}"),
                }
            };

            state
                .messages
                .push(json!({ "role": "tool", "content": noi_dung }));
            state.current_node = "chat_completion".to_string();
            Ok(state)
        }
    });

    graph.add_node("tool_exec", move |mut state: AgentState| async move {
        let device_val = state
            .context
            .get("device")
            .ok_or("device missing in context")?;
        let action_val = state
            .context
            .get("action")
            .ok_or("action missing in context")?;
        let payload = json!({
            "device": device_val,
            "action": action_val
        });
        let result = crate::integrations::smart_home::execute(payload);
        let res_msg = match result {
            Ok(msg) => json!({"role": "tool", "content": msg}),
            Err(err) => {
                json!({"role": "tool", "content": format!("Tool execution failed: {}", err)})
            }
        };
        state.messages.push(res_msg);
        state.current_node = "chat_completion".to_string();
        Ok(state)
    });

    let ss3 = Arc::clone(&state_shared);
    let tx3 = llm_chunk_tx.clone();
    let as3 = Arc::clone(&active_session_id);
    let memory_scope_chat = memory_scope.clone();
    graph.add_node("chat_completion", move |mut state: AgentState| {
        let ss = Arc::clone(&ss3);
        let tx = tx3.clone();
        let as_val = Arc::clone(&as3);
        let memory_scope = memory_scope_chat.clone();
        async move {
            // Lớp 1 của F2: cắt cửa sổ TRƯỚC khi dựng prompt. compile_prompt
            // nhét toàn bộ messages vào, còn prune_kv_cache chỉ chạy khi sinh
            // token chứ không chạy lúc prefill — không cắt ở đây thì decode()
            // hỏng ngay khi lịch sử dài hơn n_ctx.
            state.trim_history();

            let mut chat_messages = Vec::new();
            for msg in &state.messages {
                let role = msg
                    .get("role")
                    .and_then(|r| r.as_str())
                    .unwrap_or("user")
                    .to_string();
                let content = msg
                    .get("content")
                    .and_then(|c| c.as_str())
                    .unwrap_or("")
                    .to_string();
                chat_messages.push(crate::llm::ChatMessage { role, content });
            }

            // Fallback persona injection: sessions seeded before persona
            // support (e.g. legacy checkpoints) carry no system message.
            if !chat_messages.iter().any(|m| m.role == "system") {
                chat_messages.insert(
                    0,
                    crate::llm::ChatMessage {
                        role: "system".to_string(),
                        content: crate::llm::persona::PERSONA_LIVA.to_string(),
                    },
                );
            }

            // RAG — chèn ký ức liên quan ngay sau persona. Không có model
            // embedding thì bỏ qua và hành xử đúng như trước khi có RAG.
            let user_text = chat_messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default();
            if let Some(memories) = recall_context_scoped(&ss, &user_text, &memory_scope).await {
                chat_messages.insert(
                    1,
                    crate::llm::ChatMessage {
                        role: "system".to_string(),
                        content: memory_system_message(&memories),
                    },
                );
            }

            let prompt = crate::llm::compile_prompt(&chat_messages)?;

            // Giữ một handle riêng cho persist_turn: closure spawn_blocking bên
            // dưới move mất `ss`.
            let ss_persist = Arc::clone(&ss);
            let res = tokio::task::spawn_blocking(move || {
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("LLM cancelled before lock".to_string());
                }
                let mut llm = ss.llm.blocking_lock();
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("LLM cancelled post-lock".to_string());
                }
                if llm.engine.is_none() {
                    return Err("LLM engine not loaded".to_string());
                }
                let mut stream_error = None;
                let completion = llm.generate_completion(
                    &prompt,
                    crate::llm::persona::TEMP_DEFAULT,
                    crate::llm::persona::TOP_P_DEFAULT,
                    |token| match send_llm_chunk_if_current(
                        &tx,
                        as_val.as_ref(),
                        session_id,
                        token,
                        LLM_TTS_BACKPRESSURE_TIMEOUT,
                    ) {
                        Ok(()) => true,
                        Err(error) => {
                            tracing::warn!("[LLM] {}", error);
                            stream_error = Some(error);
                            false
                        }
                    },
                )?;
                finish_streamed_completion(
                    completion.text,
                    stream_error,
                    as_val.as_ref(),
                    session_id,
                )
            })
            .await
            .map_err(|e| format!("LLM task panicked: {}", e))
            .and_then(|r| r)?;

            // Lưu lượt này thành ký ức trước khi cắt lịch sử — nếu không, nội
            // dung bị cắt khỏi cửa sổ ngữ cảnh sẽ mất hẳn.
            persist_turn_scoped(&ss_persist, &user_text, &res, &memory_scope).await;

            state.messages.push(json!({
                "role": "assistant",
                "content": res
            }));
            // Cắt lại sau khi thêm câu trả lời: state này được checkpoint
            // xuống `agent_checkpoints`, không cắt thì bảng phình vô hạn.
            state.trim_history();
            state.current_node = "__END__".to_string();

            Ok(state)
        }
    });

    // Vision node: capture the screen and answer the user's spoken question
    // about it with the multimodal core (Qwen3-VL), streaming tokens to TTS just
    // like chat_completion. Requires a VL model + configured mmproj (and, on
    // Windows, a release build). Failures fall back to a short spoken apology.
    let ssv = Arc::clone(&state_shared);
    let txv = llm_chunk_tx.clone();
    let asv = Arc::clone(&active_session_id);
    graph.add_node("vision", move |mut state: AgentState| {
        let ss = Arc::clone(&ssv);
        let tx = txv.clone();
        let tx_fb = txv.clone();
        let as_val = Arc::clone(&asv);
        let as_fallback = Arc::clone(&as_val);
        async move {
            let question = state
                .messages
                .last()
                .and_then(|m| m.get("content"))
                .and_then(|c| c.as_str())
                .filter(|s| !s.trim().is_empty())
                .unwrap_or("Trên màn hình đang hiển thị gì?")
                .to_string();

            let res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err(format!(
                        "{LLM_STREAM_ABORT_PREFIX}: session cancelled before vision capture",
                    ));
                }
                // Context-aware: mouse-guided crop while a game is foreground.
                let (vw, vh, rgb) = crate::vision::capture::capture_for_vision()?;
                let mut llm = ss.llm.blocking_lock();
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err(format!(
                        "{LLM_STREAM_ABORT_PREFIX}: session cancelled after vision lock",
                    ));
                }
                if llm.engine.is_none() {
                    return Err("LLM engine not loaded".to_string());
                }
                let mut stream_error = None;
                let out = llm.answer_with_image(
                    &question,
                    crate::llm::engine::VisionImage::Rgb {
                        width: vw,
                        height: vh,
                        data: &rgb,
                    },
                    crate::llm::persona::TEMP_DEFAULT,
                    crate::llm::persona::TOP_P_DEFAULT,
                    |token| match send_llm_chunk_if_current(
                        &tx,
                        as_val.as_ref(),
                        session_id,
                        token,
                        LLM_TTS_BACKPRESSURE_TIMEOUT,
                    ) {
                        Ok(()) => true,
                        Err(error) => {
                            tracing::warn!("[vision] {}", error);
                            stream_error = Some(error);
                            false
                        }
                    },
                )?;
                finish_streamed_completion(out.text, stream_error, as_val.as_ref(), session_id)
            })
            .await
            .map_err(|e| format!("Vision task panicked: {}", e))
            .and_then(|r| r);

            let text = match res {
                Ok(t) => t,
                Err(e) if e.starts_with(LLM_STREAM_ABORT_PREFIX) => {
                    return Err(e);
                }
                Err(e) => {
                    tracing::warn!("[vision] {}", e);
                    let fallback = "Xin lỗi, hiện mình chưa xem được màn hình.";
                    send_llm_chunk_if_current(
                        &tx_fb,
                        as_fallback.as_ref(),
                        session_id,
                        fallback,
                        std::time::Duration::ZERO,
                    )?;
                    fallback.to_string()
                }
            };

            state
                .messages
                .push(json!({ "role": "assistant", "content": text }));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("router");
    graph
}
