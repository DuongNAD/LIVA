use super::state::AgentState;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;
use serde_json::json;
use crate::AppState;

pub type NodeFuture = Pin<Box<dyn Future<Output = Result<AgentState, String>> + Send>>;
pub type NodeFn = Box<dyn Fn(AgentState) -> NodeFuture + Send + Sync>;

pub struct StateGraph {
    nodes: HashMap<String, NodeFn>,
    edges: HashMap<String, String>,
    entry_point: String,
}

impl Default for StateGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl StateGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_point: "START".to_string(),
        }
    }

    pub fn add_node<F, Fut>(&mut self, name: &str, node: F)
    where
        F: Fn(AgentState) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<AgentState, String>> + Send + 'static,
    {
        let wrapped = move |state| {
            let fut = node(state);
            Box::pin(fut) as NodeFuture
        };
        self.nodes.insert(name.to_string(), Box::new(wrapped));
    }

    pub fn add_edge(&mut self, from: &str, to: &str) {
        self.edges.insert(from.to_string(), to.to_string());
    }

    pub fn set_entry_point(&mut self, node: &str) {
        self.entry_point = node.to_string();
    }

    pub async fn run(&self, initial_state: AgentState) -> Result<AgentState, String> {
        let mut state = initial_state;
        if state.current_node.is_empty() || state.current_node == "START" {
            state.current_node = self.entry_point.clone();
        }

        while state.current_node != "__END__" {
            let current = state.current_node.clone();
            let node_fn = self.nodes.get(&current)
                .ok_or_else(|| format!("Node '{}' not found", current))?;
            
            state = node_fn(state.clone()).await?;

            if state.current_node == current {
                if let Some(next) = self.edges.get(&current) {
                    state.current_node = next.clone();
                } else {
                    state.current_node = "__END__".to_string();
                }
            }
        }

        Ok(state)
    }
}


/// Ý định mà node `router` suy ra từ câu của người dùng.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Intent {
    /// Hỏi về nội dung màn hình → nhánh vision.
    Vision,
    /// Điều khiển thiết bị → nhánh tool_exec.
    SmartHome { device: &'static str, action: &'static str },
    /// Còn lại → trả lời bằng LLM.
    Chat,
}

/// Tách câu thành các "từ" theo ranh giới ký tự chữ-số Unicode.
///
/// Dùng `is_alphanumeric` chứ không phải `is_ascii_alphanumeric` để giữ nguyên
/// chữ tiếng Việt có dấu — `đèn`, `bật`, `tắt` phải là một token trọn vẹn.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect()
}

/// Câu có chứa cụm từ (dãy token liên tiếp) này không?
fn has_phrase(tokens: &[String], phrase: &[&str]) -> bool {
    if phrase.is_empty() || tokens.len() < phrase.len() {
        return false;
    }
    tokens
        .windows(phrase.len())
        .any(|w| w.iter().zip(phrase).all(|(a, b)| a == b))
}

/// Câu có chứa **nguyên** từ này không (không phải chuỗi con).
fn has_word(tokens: &[String], word: &str) -> bool {
    tokens.iter().any(|t| t == word)
}

/// Suy ý định từ câu của người dùng.
///
/// # Vì sao không dùng `contains()`
///
/// Bản trước khớp chuỗi con nên sai cả hai chiều:
/// - **Dương tính giả:** `contains("ac")` khớp "b**ac**k", "pl**ac**e";
///   `contains("on")` khớp "m**on**ey", "c**on**versation";
///   `contains("off")` khớp "c**off**ee", "**off**ice".
///   "back on track" từng bị hiểu thành lệnh bật điều hoà.
/// - **Âm tính giả:** không có một từ khoá tiếng Việt nào, nên "bật đèn giúp
///   mình" không khớp gì cả — đúng thứ người dùng Việt sẽ nói đầu tiên.
///
/// Giờ khớp theo **token trọn vẹn** và có cả từ khoá tiếng Việt. Đây vẫn là
/// định tuyến theo từ khoá, chưa phải tool-calling có schema do LLM sinh —
/// bước đó nằm ở lộ trình.
pub fn route_intent(text: &str) -> Intent {
    let tokens = tokenize(text);

    // Vision ưu tiên cao nhất: hỏi về màn hình thì không thể là lệnh thiết bị.
    if has_phrase(&tokens, &["màn", "hình"])
        || has_word(&tokens, "screen")
        || has_word(&tokens, "screenshot")
        || has_phrase(&tokens, &["trên", "màn"])
    {
        return Intent::Vision;
    }

    let device = if has_word(&tokens, "light")
        || has_word(&tokens, "lamp")
        || has_word(&tokens, "đèn")
    {
        Some("light")
    } else if has_word(&tokens, "ac")
        || has_phrase(&tokens, &["điều", "hoà"])
        || has_phrase(&tokens, &["điều", "hòa"])
        || has_phrase(&tokens, &["máy", "lạnh"])
    {
        Some("ac")
    } else if has_word(&tokens, "fan") || has_word(&tokens, "quạt") {
        Some("fan")
    } else {
        None
    };

    let action = if has_word(&tokens, "on")
        || has_word(&tokens, "bật")
        || has_word(&tokens, "mở")
    {
        Some("on")
    } else if has_word(&tokens, "off")
        || has_word(&tokens, "tắt")
        || has_word(&tokens, "đóng")
    {
        Some("off")
    } else {
        None
    };

    match (device, action) {
        (Some(device), Some(action)) => Intent::SmartHome { device, action },
        _ => Intent::Chat,
    }
}


/// Số ký ức lấy ra mỗi lượt. Đặt qua `LIVA_RAG_TOP_K` (mặc định 3).
///
/// Giữ nhỏ có chủ ý: mỗi ký ức chèn thêm token vào prompt, mà `n_ctx` mặc định
/// chỉ 4096 và người dùng beta chạy model 2–4B.
fn rag_top_k() -> usize {
    std::env::var("LIVA_RAG_TOP_K")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0 && *n <= 20)
        .unwrap_or(3)
}

/// Tìm ký ức liên quan tới câu hỏi. Trả `None` khi chưa có model embedding,
/// khi không tìm được gì, hoặc khi có lỗi — mọi trường hợp đều để hội thoại
/// chạy tiếp **đúng như khi chưa có RAG**, không bao giờ làm hỏng lượt nói.
///
/// `pub` từ 22/07/2026: bộ nhớ ban đầu chỉ nối vào graph (đường THOẠI), nên
/// LIVA nhớ khi nói mà quên khi gõ — UI (`user_voice_command`, main.rs) và
/// Telegram (`chat:completion`, lib.rs) dựng prompt thẳng không qua graph.
/// Cả ba đường nay dùng chung đúng cặp hàm này để hành xử y hệt nhau.
pub async fn recall_context(state: &Arc<AppState>, query: &str) -> Option<String> {
    if query.trim().is_empty() {
        return None;
    }
    let state = Arc::clone(state);
    let query = query.to_string();
    let top_k = rag_top_k();

    tokio::task::spawn_blocking(move || {
        let mut guard = state.embedder.blocking_lock();
        let engine = guard.as_mut()?;
        let vector = match engine.embed_query(&query) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("[RAG] embed_query that bai: {}", e);
                return None;
            }
        };
        drop(guard);

        let conn = state.db.readers.get().ok()?;
        let filter = crate::db::MetadataFilter {
            r#type: None,
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };
        let hits = match crate::db::search_hybrid_vectors(&conn, &query, &vector, top_k, &filter, 1.0, 1.0) {
            Ok(h) => h,
            Err(e) => {
                tracing::warn!("[RAG] search_hybrid_vectors that bai: {}", e);
                return None;
            }
        };
        if hits.is_empty() {
            return None;
        }
        let joined = hits
            .iter()
            .map(|h| format!("- {}", h.content.trim()))
            .collect::<Vec<_>>()
            .join("\n");
        tracing::info!("[RAG] recall {} ky uc", hits.len());
        Some(joined)
    })
    .await
    .ok()
    .flatten()
}

/// Lưu lượt hội thoại vừa xong thành một ký ức.
///
/// Lỗi ở đây **không được** làm hỏng câu trả lời đã sinh — người dùng đã nhận
/// được nội dung rồi, mất một bản ghi nhớ là chấp nhận được, còn ném lỗi ngược
/// lên thì không.
///
/// Câu chèn ký ức vào prompt — dùng chung cho CẢ BA đường (graph thoại, UI gõ
/// chữ, Telegram) để cách LIVA "đọc ghi chú" không trôi lệch giữa các cửa vào.
pub fn memory_system_message(memories: &str) -> String {
    format!(
        "Ký ức liên quan từ các cuộc trò chuyện trước (dùng nếu hữu ích, \
         bỏ qua nếu không liên quan; đừng nhắc là bạn đang đọc ghi chú):\n{}",
        memories
    )
}

/// `pub` cùng lý do với [`recall_context`] — xem doc ở đó.
pub async fn persist_turn(state: &Arc<AppState>, user_text: &str, reply: &str) {
    if user_text.trim().is_empty() || reply.trim().is_empty() {
        return;
    }
    let state = Arc::clone(state);
    let content = format!("Người dùng: {}\nLIVA: {}", user_text.trim(), reply.trim());

    let _ = tokio::task::spawn_blocking(move || {
        let mut guard = state.embedder.blocking_lock();
        let Some(engine) = guard.as_mut() else { return };
        let vector = match engine.embed_passage(&content) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("[RAG] embed_passage that bai: {}", e);
                return;
            }
        };
        drop(guard);

        let Ok(conn) = state.db.writer.get() else { return };
        let vec_id = format!("turn_{}", uuid::Uuid::new_v4());
        if let Err(e) = crate::db::upsert_vector(
            &conn,
            &vec_id,
            "conversation_turn",
            &content,
            &vector,
            None,
            None,
            None,
            None,
            None,
        ) {
            tracing::warn!("[RAG] upsert_vector that bai: {}", e);
        }
    })
    .await;
}

pub fn build_pipeline_graph(
    state_shared: Arc<AppState>,
    llm_chunk_tx: mpsc::Sender<String>,
    session_id: u64,
    active_session_id: Arc<std::sync::atomic::AtomicU64>,
) -> StateGraph {
    let mut graph = StateGraph::new();

    let ss1 = Arc::clone(&state_shared);
    let tx1 = llm_chunk_tx.clone();
    let as1 = Arc::clone(&active_session_id);
    graph.add_node("router", move |mut state: AgentState| {
        let _ss = Arc::clone(&ss1);
        let _tx = tx1.clone();
        let _as = Arc::clone(&as1);
        async move {
            let last_msg = state.messages.last().ok_or("No messages in state")?;
            let text = last_msg.get("content")
                .and_then(|c| c.as_str())
                .unwrap_or("");

            match route_intent(text) {
                Intent::Vision => {
                    state.current_node = "vision".to_string();
                }
                Intent::SmartHome { device, action } => {
                    state.context.insert("device".to_string(), json!(device));
                    state.context.insert("action".to_string(), json!(action));
                    state.current_node = "tool_exec".to_string();
                }
                Intent::Chat => {
                    state.current_node = "chat_completion".to_string();
                }
            }

            Ok(state)
        }
    });

    graph.add_node("tool_exec", move |mut state: AgentState| {
        async move {
            let device_val = state.context.get("device").ok_or("device missing in context")?;
            let action_val = state.context.get("action").ok_or("action missing in context")?;
            let payload = json!({
                "device": device_val,
                "action": action_val
            });
            let result = crate::integrations::smart_home::execute(payload);
            let res_msg = match result {
                Ok(msg) => json!({"role": "tool", "content": msg}),
                Err(err) => json!({"role": "tool", "content": format!("Tool execution failed: {}", err)}),
            };
            state.messages.push(res_msg);
            state.current_node = "chat_completion".to_string();
            Ok(state)
        }
    });

    let ss3 = Arc::clone(&state_shared);
    let tx3 = llm_chunk_tx.clone();
    let as3 = Arc::clone(&active_session_id);
    graph.add_node("chat_completion", move |mut state: AgentState| {
        let ss = Arc::clone(&ss3);
        let tx = tx3.clone();
        let as_val = Arc::clone(&as3);
        async move {
            // Lớp 1 của F2: cắt cửa sổ TRƯỚC khi dựng prompt. compile_prompt
            // nhét toàn bộ messages vào, còn prune_kv_cache chỉ chạy khi sinh
            // token chứ không chạy lúc prefill — không cắt ở đây thì decode()
            // hỏng ngay khi lịch sử dài hơn n_ctx.
            state.trim_history();

            let mut chat_messages = Vec::new();
            for msg in &state.messages {
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("user").to_string();
                let content = msg.get("content").and_then(|c| c.as_str()).unwrap_or("").to_string();
                chat_messages.push(crate::llm::ChatMessage { role, content });
            }

            // Fallback persona injection: sessions seeded before persona
            // support (e.g. legacy checkpoints) carry no system message.
            if !chat_messages.iter().any(|m| m.role == "system") {
                chat_messages.insert(0, crate::llm::ChatMessage {
                    role: "system".to_string(),
                    content: crate::llm::persona::PERSONA_LIVA.to_string(),
                });
            }

            // RAG — chèn ký ức liên quan ngay sau persona. Không có model
            // embedding thì bỏ qua và hành xử đúng như trước khi có RAG.
            let user_text = chat_messages
                .iter()
                .rev()
                .find(|m| m.role == "user")
                .map(|m| m.content.clone())
                .unwrap_or_default();
            if let Some(memories) = recall_context(&ss, &user_text).await {
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
                let completion = llm.generate_completion(
                    &prompt,
                    crate::llm::persona::TEMP_DEFAULT,
                    crate::llm::persona::TOP_P_DEFAULT,
                    |token| {
                        if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                            return false;
                        }
                        let _ = tx.blocking_send(token.to_string());
                        true
                    },
                )?;
                Ok(completion.text)
            })
            .await
            .map_err(|e| format!("LLM task panicked: {}", e))
            .and_then(|r| r)?;

            // Lưu lượt này thành ký ức trước khi cắt lịch sử — nếu không, nội
            // dung bị cắt khỏi cửa sổ ngữ cảnh sẽ mất hẳn.
            persist_turn(&ss_persist, &user_text, &res).await;

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
                    return Err("Vision cancelled before capture".to_string());
                }
                // Context-aware: mouse-guided crop while a game is foreground.
                let (vw, vh, rgb) = crate::vision::capture::capture_for_vision()?;
                let mut llm = ss.llm.blocking_lock();
                if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                    return Err("Vision cancelled post-lock".to_string());
                }
                if llm.engine.is_none() {
                    return Err("LLM engine not loaded".to_string());
                }
                let out = llm.answer_with_image(
                    &question,
                    crate::llm::engine::VisionImage::Rgb {
                        width: vw,
                        height: vh,
                        data: &rgb,
                    },
                    crate::llm::persona::TEMP_DEFAULT,
                    crate::llm::persona::TOP_P_DEFAULT,
                    |token| {
                        if as_val.load(std::sync::atomic::Ordering::SeqCst) != session_id {
                            return false;
                        }
                        let _ = tx.blocking_send(token.to_string());
                        true
                    },
                )?;
                Ok(out.text)
            })
            .await
            .map_err(|e| format!("Vision task panicked: {}", e))
            .and_then(|r| r);

            let text = match res {
                Ok(t) => t,
                Err(e) => {
                    tracing::warn!("[vision] {}", e);
                    let fallback = "Xin lỗi, hiện mình chưa xem được màn hình.";
                    let _ = tx_fb.send(fallback.to_string()).await;
                    fallback.to_string()
                }
            };

            state.messages.push(json!({ "role": "assistant", "content": text }));
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    graph.set_entry_point("router");
    graph
}

#[cfg(test)]
mod router_tests {
    use super::{route_intent, Intent};

    fn smart(device: &'static str, action: &'static str) -> Intent {
        Intent::SmartHome { device, action }
    }

    /// HỒI QUY: đây là các câu bản cũ hiểu SAI vì dùng contains() khớp chuỗi con.
    #[test]
    fn khong_con_duong_tinh_gia() {
        // "back on track": "ac" trong "b-ac-k" + "on" => bản cũ ra bật điều hoà
        assert_eq!(route_intent("let's get back on track"), Intent::Chat);
        // "coffee": "off" trong "c-off-ee"
        assert_eq!(route_intent("I want coffee and a fan"), Intent::Chat);
        // "money"/"conversation": "on" là chuỗi con
        assert_eq!(route_intent("how much money for the lamp"), Intent::Chat);
        // "office": "off" là chuỗi con
        assert_eq!(route_intent("the office light"), Intent::Chat);
        // "place": "ac" là chuỗi con
        assert_eq!(route_intent("place it on the table"), Intent::Chat);
    }

    /// HỒI QUY: bản cũ không có từ khoá tiếng Việt nào.
    #[test]
    fn hieu_duoc_tieng_viet() {
        assert_eq!(route_intent("bật đèn giúp mình"), smart("light", "on"));
        assert_eq!(route_intent("tắt quạt đi"), smart("fan", "off"));
        assert_eq!(route_intent("mở điều hoà"), smart("ac", "on"));
        assert_eq!(route_intent("tắt điều hòa nhé"), smart("ac", "off"));
        assert_eq!(route_intent("bật máy lạnh lên"), smart("ac", "on"));
    }

    #[test]
    fn van_hieu_tieng_anh() {
        assert_eq!(route_intent("turn on the light"), smart("light", "on"));
        assert_eq!(route_intent("turn off the fan"), smart("fan", "off"));
        assert_eq!(route_intent("ac on"), smart("ac", "on"));
    }

    #[test]
    fn nhan_dien_y_dinh_vision() {
        assert_eq!(route_intent("trên màn hình có gì"), Intent::Vision);
        assert_eq!(route_intent("what's on my screen"), Intent::Vision);
        assert_eq!(route_intent("take a screenshot"), Intent::Vision);
        // Vision phải thắng cả khi câu có từ khoá thiết bị
        assert_eq!(route_intent("bật đèn trên màn hình"), Intent::Vision);
    }

    #[test]
    fn thieu_mot_ve_thi_khong_goi_tool() {
        // có thiết bị nhưng không có hành động
        assert_eq!(route_intent("cái đèn"), Intent::Chat);
        // có hành động nhưng không có thiết bị
        assert_eq!(route_intent("bật lên"), Intent::Chat);
    }

    #[test]
    fn chuoi_rong_va_ky_tu_la() {
        assert_eq!(route_intent(""), Intent::Chat);
        assert_eq!(route_intent("   "), Intent::Chat);
        assert_eq!(route_intent("!!!???"), Intent::Chat);
    }

    #[test]
    fn dau_cau_khong_lam_vo_token() {
        assert_eq!(route_intent("bật đèn, nhanh!"), smart("light", "on"));
        assert_eq!(route_intent("turn on—the light."), smart("light", "on"));
    }

    #[test]
    fn khong_phan_biet_hoa_thuong() {
        assert_eq!(route_intent("BẬT ĐÈN"), smart("light", "on"));
        assert_eq!(route_intent("Turn ON The LIGHT"), smart("light", "on"));
    }
}

#[cfg(test)]
mod rag_tests {
    use super::{persist_turn, rag_top_k, recall_context};
    use crate::AppState;
    use std::sync::Arc;

    /// AppState tối thiểu, **không có model embedding** — đúng tình huống của
    /// người dùng chưa tải model về.
    fn state_khong_co_embedder() -> Arc<AppState> {
        let capturer = Arc::new(crate::vision::capture::MockScreenCapturer::new(
            64,
            64,
            crate::vision::capture::PixelFormat::Rgba,
        ));
        Arc::new(AppState {
            db: crate::db::DatabasePool::new_in_memory().expect("in-memory db"),
            crypto: crate::crypto::EncryptionEngine::new("00000000000000000000000000000000"),
            stt: tokio::sync::Mutex::new(crate::stt::SttManager::new("non_existent_dir")),
            tts: tokio::sync::Mutex::new(None),
            tts_player: crate::tts::audio::TtsAudioPlayer::new(None),
            llm: tokio::sync::Mutex::new(
                crate::llm::LlamaRouterManager::new(512, 0).expect("llm manager"),
            ),
            vad: tokio::sync::Mutex::new(None),
            denoiser: tokio::sync::Mutex::new(None),
            turn_shadow: tokio::sync::Mutex::new(None),
            aec: tokio::sync::Mutex::new(None),
            mcp_server: Arc::new(crate::mcp::server::NativeMcpServer::new("test_vault")),
            embedder: tokio::sync::Mutex::new(None),
            vision: tokio::sync::Mutex::new(crate::vision::VisionManager::new(
                capturer,
                crate::vision::VisionConfig::default(),
            )),
        })
    }

    fn dem_vector(state: &Arc<AppState>) -> i64 {
        let conn = state.db.readers.get().unwrap();
        conn.query_row("SELECT count(*) FROM vectors_meta", [], |r| r.get(0))
            .unwrap()
    }

    /// HỢP ĐỒNG QUAN TRỌNG NHẤT của 2.2: chưa có model embedding thì RAG phải
    /// im lặng tắt, KHÔNG lỗi, KHÔNG ghi gì — hệ thống hành xử y như trước.
    #[tokio::test]
    async fn khong_co_model_thi_rag_im_lang_tat() {
        let state = state_khong_co_embedder();

        assert!(
            recall_context(&state, "hôm qua tôi nói gì").await.is_none(),
            "khong co model thi recall phai tra None"
        );

        // persist không được panic và không được ghi gì
        persist_turn(&state, "câu hỏi", "câu trả lời").await;
        assert_eq!(dem_vector(&state), 0, "khong co model thi khong duoc ghi ky uc nao");
    }

    #[tokio::test]
    async fn cau_rong_khong_kich_hoat_rag() {
        let state = state_khong_co_embedder();
        assert!(recall_context(&state, "").await.is_none());
        assert!(recall_context(&state, "   ").await.is_none());

        persist_turn(&state, "", "tra loi").await;
        persist_turn(&state, "hoi", "").await;
        assert_eq!(dem_vector(&state), 0, "ve rong thi khong duoc ghi");
    }

    #[test]
    fn top_k_co_gioi_han_hop_ly() {
        // Không đặt biến -> mặc định 3
        unsafe { std::env::remove_var("LIVA_RAG_TOP_K") };
        assert_eq!(rag_top_k(), 3);

        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "5") };
        assert_eq!(rag_top_k(), 5);

        // 0 và giá trị vô lý phải rơi về mặc định: 0 nghĩa là tắt RAG một cách
        // khó hiểu, còn số quá lớn sẽ làm prompt phình vượt n_ctx.
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "0") };
        assert_eq!(rag_top_k(), 3);
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "9999") };
        assert_eq!(rag_top_k(), 3);
        unsafe { std::env::set_var("LIVA_RAG_TOP_K", "abc") };
        assert_eq!(rag_top_k(), 3);

        unsafe { std::env::remove_var("LIVA_RAG_TOP_K") };
    }
}
