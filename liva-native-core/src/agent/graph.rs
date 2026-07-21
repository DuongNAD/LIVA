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

            let prompt = crate::llm::compile_prompt(&chat_messages)?;

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
