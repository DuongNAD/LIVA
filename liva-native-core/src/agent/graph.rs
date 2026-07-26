use super::state::AgentState;
use crate::AppState;
use serde_json::json;
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio::sync::mpsc;

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
            let node_fn = self
                .nodes
                .get(&current)
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
    SmartHome {
        device: &'static str,
        action: &'static str,
    },
    /// Điều khiển chính máy này: âm lượng / phát nhạc (U19) → `mcp_tool_exec`.
    ///
    /// Vì sao đi đường nhanh thay vì để LLM chọn: đo trên Qwen3-VL-2B
    /// (26/07/2026) cho thấy model trượt đúng những câu **đa nghĩa** —
    /// *"bật nhạc lên"* rơi sang chỉnh âm lượng, *"chuyển bài khác"* chọn đúng
    /// tool nhưng sai hướng. Đó là trần của model 2B, không sửa được bằng cách
    /// viết lại prompt. Bảng từ khoá thì không đa nghĩa, không tốn token nào,
    /// và cho cùng một kết quả mọi lần.
    OsControl {
        /// `control_volume` hoặc `control_media`.
        tool: &'static str,
        action: &'static str,
    },
    /// Nhắn tin cho người trong danh bạ → nhánh `message_draft`.
    ///
    /// Mang `String` chứ không `&'static str` như hai nhánh trên: tên người và
    /// nội dung tin lấy ra từ chính câu nói, không thể là hằng.
    ///
    /// `body` được phép RỖNG — "nhắn cho Hiến đi" là câu hợp lệ, chỉ là chưa nói
    /// nội dung. Nhánh thi hành sẽ hỏi lại thay vì gửi một tin trống.
    SendMessage { recipient: String, body: String },
    /// Còn lại → trả lời bằng LLM.
    Chat,
}

/// Tách "nhắn cho X bảo Y" thành `(X, Y)`.
///
/// ## Vì sao không để LLM làm
///
/// Cùng lý do với `OsControl`: model 2B trượt đúng những câu đa nghĩa, và ở đây
/// cái giá của việc trượt cao hơn nhiều — không phải bật nhầm đèn mà là gửi
/// nhầm chữ cho người khác. Bảng từ khoá không đa nghĩa, không tốn token, và ra
/// cùng kết quả mọi lần. Phần *diễn đạt lại cho tự nhiên* mới là việc của LLM,
/// và nó nằm sau bước xác nhận.
///
/// ## Quy tắc
///
/// 1. **Cò:** (`nhắn`|`gửi`) [`tin`] [`nhắn`] `cho`. So khớp trên dạng đã bỏ dấu
///    nên "nhan cho" từ STT vẫn ăn.
/// 2. **Mốc nội dung:** từ đầu tiên trong {`bảo`, `rằng`, `là`, `nói`} hoặc dấu
///    hai chấm. Trước mốc là tên, sau mốc là nội dung.
/// 3. **Bỏ đại từ mở đầu nội dung:** "bảo **nó** ngủ đi" → "ngủ đi".
///
/// Không có mốc thì toàn bộ phần sau cò là tên, nội dung rỗng.
fn tach_nhan_tin(text: &str) -> Option<(String, String)> {
    let goc: Vec<&str> = text.split_whitespace().collect();
    if goc.is_empty() {
        return None;
    }
    // Dạng bỏ dấu của từng token, giữ nguyên chỉ số để cắt lại trên bản gốc.
    let gap: Vec<String> = goc
        .iter()
        .map(|t| crate::wake::normalize_for_match(t))
        .collect();

    // ── 1. Tìm cò ────────────────────────────────────────────────────────────
    let mut sau_co = None;
    for i in 0..gap.len() {
        if gap[i] != "nhan" && gap[i] != "gui" {
            continue;
        }
        let mut j = i + 1;
        // Nuốt "tin", "nhắn" ở giữa: "gửi tin nhắn cho", "nhắn tin cho".
        while j < gap.len() && (gap[j] == "tin" || gap[j] == "nhan") {
            j += 1;
        }
        if j < gap.len() && gap[j] == "cho" {
            sau_co = Some(j + 1);
            break;
        }
    }
    let bat_dau = sau_co?;
    if bat_dau >= goc.len() {
        return None; // "nhắn cho" rồi hết câu — không có người nhận
    }

    // ── 2. Tìm mốc nội dung ──────────────────────────────────────────────────
    //
    // Mốc so khớp theo dấu HAY không tuỳ câu, và đây không phải cầu kỳ vô cớ —
    // nó là bản vá cho một lỗi đo được: "nhắn cho Người **Lạ** Hoắc bảo alo".
    // Bỏ dấu thì `lạ` và `là` cùng ra `la`, nên tên bị cắt còn "Người" và nội
    // dung thành "Hoắc bảo alo". Cùng bẫy đó rình mọi tên có `La/Lá/Lã`, và
    // `Bảo` là tên người rất phổ biến.
    //
    // Quy tắc: câu CÓ dấu thì đòi mốc đúng dấu (`là`, `bảo`, `rằng`, `nói`);
    // câu KHÔNG dấu nào — tức STT trả về trần — mới chấp nhận mốc không dấu.
    // Người gõ có dấu thì gõ có dấu cả câu; người đọc cho STT thì mất dấu cả
    // câu. Trường hợp lẫn lộn hiếm, và nếu trượt thì thẻ xác nhận đỡ.
    const MOC_CO_DAU: [&str; 4] = ["bảo", "rằng", "là", "nói"];
    const MOC_KHONG_DAU: [&str; 4] = ["bao", "rang", "la", "noi"];
    let cau_co_dau = goc
        .iter()
        .any(|t| t.chars().any(|c| crate::wake::normalize_for_match(&c.to_string()) != c.to_lowercase().to_string()));

    let mut moc = None;
    for k in bat_dau..goc.len() {
        // Dấu hai chấm dính cuối token: "Hiến: ngủ đi".
        if goc[k].ends_with(':') {
            moc = Some((k, true));
            break;
        }
        // Token đầu ngay sau "cho" LUÔN thuộc về tên: người nhận không thể
        // rỗng. Không có dòng này thì "nhắn cho **Bảo** rằng mai đi học" ra tên
        // rỗng rồi trả None — tức mất trắng câu, tệ hơn cả tách sai.
        if k == bat_dau {
            continue;
        }
        let la_moc = if cau_co_dau {
            let thuong = goc[k].to_lowercase();
            MOC_CO_DAU.contains(&thuong.trim_matches(|c: char| !c.is_alphanumeric()))
        } else {
            MOC_KHONG_DAU.contains(&gap[k].as_str())
        };
        if la_moc {
            moc = Some((k, false));
            break;
        }
    }

    let (het_ten, dau_noi_dung) = match moc {
        Some((k, dinh_hai_cham)) => {
            if dinh_hai_cham {
                (k + 1, k + 1) // token có dấu ':' vẫn thuộc về tên
            } else {
                (k, k + 1)
            }
        }
        None => (goc.len(), goc.len()),
    };

    let ten = goc[bat_dau..het_ten]
        .join(" ")
        .trim_end_matches(':')
        .trim()
        .to_string();
    if ten.is_empty() {
        return None;
    }

    // ── 3. Bỏ đại từ mở đầu nội dung ─────────────────────────────────────────
    let mut i = dau_noi_dung;
    if i < gap.len() {
        if gap[i] == "no" {
            i += 1;
        } else if matches!(gap[i].as_str(), "anh" | "chi" | "em" | "cau" | "ban" | "ong" | "ba")
            && i + 1 < gap.len()
            && matches!(gap[i + 1].as_str(), "ay" | "ta")
        {
            i += 2;
        }
    }
    let noi_dung = goc.get(i..).unwrap_or(&[]).join(" ").trim().to_string();

    Some((ten, noi_dung))
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

    // Nhắn tin đứng TRƯỚC tất cả, kể cả vision. Vì nội dung tin nhắn là câu của
    // NGƯỜI KHÁC, và nó có thể chứa bất kỳ từ khoá nào của các nhánh dưới:
    // "nhắn cho Nam bật nhạc lên" mà rơi vào OsControl thì LIVA bật nhạc của
    // chính máy này thay vì nhắn — sai thầm lặng, người dùng tưởng đã nhắn.
    // Đặt đầu tiên là cách duy nhất để phần thân tin nhắn không bị nhánh khác
    // cướp. Đổi lại, cái giá phải trả là câu "chụp màn hình gửi cho Nam" sẽ
    // thành nhắn tin — chấp nhận được, vì bản nháp hiện ra để người dùng huỷ.
    if let Some((recipient, body)) = tach_nhan_tin(text) {
        return Intent::SendMessage { recipient, body };
    }

    // Vision ưu tiên cao nhất: hỏi về màn hình thì không thể là lệnh thiết bị.
    if has_phrase(&tokens, &["màn", "hình"])
        || has_word(&tokens, "screen")
        || has_word(&tokens, "screenshot")
        || has_phrase(&tokens, &["trên", "màn"])
    {
        return Intent::Vision;
    }

    // ── Điều khiển máy: âm lượng / phát nhạc (U19) ─────────────────────────
    //
    // CỐ TÌNH chỉ nhận từ vựng TIẾNG VIỆT. Đường nhanh này tồn tại đúng vì
    // tiếng Việt là chỗ model 2B yếu nhất; tiếng Anh nó xử lý tốt nên nhường
    // cho LLM. Thêm danh từ tiếng Anh vào đây là tự rước lại bẫy
    // `"let's get back on track"` — `track` + `back` sẽ thành "quay lại bài
    // trước", đúng loại dương tính giả mà `khong_con_duong_tinh_gia` canh.
    //
    // Đặt TRƯỚC nhánh smart-home nhưng đòi một danh từ âm thanh/nhạc, nên nó
    // không thể cướp `"bật đèn"` / `"tắt quạt"`.
    let danh_tu_am_thanh = has_word(&tokens, "tiếng")
        || has_phrase(&tokens, &["âm", "lượng"])
        || has_word(&tokens, "loa");
    let danh_tu_nhac =
        has_word(&tokens, "nhạc") || has_word(&tokens, "bài") || has_word(&tokens, "hát");

    if danh_tu_am_thanh || danh_tu_nhac {
        // ĐỘ TO thắng ĐANG-PHÁT-GÌ: `"nhỏ nhạc lại"` có cả "nhạc" lẫn "nhỏ",
        // và ý người nói là âm lượng. Cùng ranh giới đã ghi trong mô tả tool.
        let am_luong = if has_word(&tokens, "to")
            || has_word(&tokens, "lớn")
            || has_word(&tokens, "tăng")
        {
            Some("up")
        } else if has_word(&tokens, "nhỏ")
            || has_word(&tokens, "bé")
            || has_word(&tokens, "giảm")
            || has_word(&tokens, "khẽ")
        {
            Some("down")
        } else if has_word(&tokens, "tắt") && danh_tu_am_thanh {
            // "tắt tiếng" = mute. "tắt nhạc" thì KHÁC — đó là dừng phát, nên
            // nhánh này đòi đúng danh từ âm thanh.
            Some("mute")
        } else {
            None
        };
        if let Some(action) = am_luong {
            return Intent::OsControl {
                tool: "control_volume",
                action,
            };
        }

        if danh_tu_nhac {
            let media = if has_word(&tokens, "trước")
                || has_phrase(&tokens, &["quay", "lại"])
                || has_word(&tokens, "lùi")
            {
                Some("previous")
            } else if has_word(&tokens, "khác")
                || has_word(&tokens, "kế")
                || has_word(&tokens, "chuyển")
                || has_phrase(&tokens, &["tiếp", "theo"])
            {
                Some("next")
            } else if has_word(&tokens, "dừng")
                || has_word(&tokens, "phát")
                || has_word(&tokens, "bật")
                || has_word(&tokens, "mở")
                || has_word(&tokens, "tắt")
            {
                Some("play_pause")
            } else {
                None
            };
            if let Some(action) = media {
                return Intent::OsControl {
                    tool: "control_media",
                    action,
                };
            }
        }
    }

    let device =
        if has_word(&tokens, "light") || has_word(&tokens, "lamp") || has_word(&tokens, "đèn") {
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

    let action = if has_word(&tokens, "on") || has_word(&tokens, "bật") || has_word(&tokens, "mở")
    {
        Some("on")
    } else if has_word(&tokens, "off") || has_word(&tokens, "tắt") || has_word(&tokens, "đóng")
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
/// Phạm vi bộ nhớ hội thoại tách ranh giới bảo mật (`owner`) khỏi lineage
/// (`conversation`). RAG dài hạn được phép nhớ xuyên nhiều conversation của cùng
/// owner, nhưng tuyệt đối không được truy vấn global hoặc đọc chéo owner.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversationMemoryScope {
    owner_domain: String,
    conversation_category: String,
    recall_category: Option<String>,
}

impl ConversationMemoryScope {
    pub fn new(owner_id: &str, conversation_id: &str) -> Result<Self, String> {
        Self::build(owner_id, conversation_id, false)
    }

    /// Giới hạn recall vào đúng conversation/audience hiện tại.
    ///
    /// Dùng cho kênh có nhiều người cùng đọc câu trả lời (ví dụ Telegram group)
    /// để ký ức riêng của owner ở DM hoặc group khác không bị đưa vào audience này.
    pub fn new_audience_scoped(owner_id: &str, conversation_id: &str) -> Result<Self, String> {
        Self::build(owner_id, conversation_id, true)
    }

    fn build(
        owner_id: &str,
        conversation_id: &str,
        restrict_recall_to_conversation: bool,
    ) -> Result<Self, String> {
        let owner_id = owner_id.trim();
        let conversation_id = conversation_id.trim();
        if owner_id.is_empty() || conversation_id.is_empty() {
            return Err("owner_id và conversation_id không được để trống".to_string());
        }
        let conversation_category = format!("conversation:{conversation_id}");
        Ok(Self {
            owner_domain: format!("memory_owner:{owner_id}"),
            recall_category: restrict_recall_to_conversation.then(|| conversation_category.clone()),
            conversation_category,
        })
    }

    pub fn recall_filter(&self) -> crate::db::MetadataFilter {
        crate::db::MetadataFilter {
            r#type: Some("conversation_turn".to_string()),
            domain: Some(self.owner_domain.clone()),
            category: self.recall_category.clone(),
            created_after: None,
            created_before: None,
        }
    }

    pub fn storage_domain(&self) -> &str {
        &self.owner_domain
    }

    pub fn storage_category(&self) -> &str {
        &self.conversation_category
    }
}

fn persist_embedded_turn(
    conn: &rusqlite::Connection,
    scope: &ConversationMemoryScope,
    content: &str,
    vector: &[f32],
) -> Result<(), rusqlite::Error> {
    let vec_id = format!("turn_{}", uuid::Uuid::new_v4());
    crate::db::persist_conversation_event_vector(
        conn,
        &vec_id,
        content,
        vector,
        scope.storage_domain(),
        scope.storage_category(),
    )
}

fn recall_embedded_context(
    conn: &rusqlite::Connection,
    scope: &ConversationMemoryScope,
    query: &str,
    vector: &[f32],
    top_k: usize,
) -> Result<Option<String>, rusqlite::Error> {
    let hits = crate::db::search_hybrid_vectors(
        conn,
        query,
        vector,
        top_k,
        &scope.recall_filter(),
        1.0,
        1.0,
    )?;
    if hits.is_empty() {
        return Ok(None);
    }
    Ok(Some(
        hits.iter()
            .map(|hit| format!("- {}", hit.content.trim()))
            .collect::<Vec<_>>()
            .join("\n"),
    ))
}

pub async fn recall_context(state: &Arc<AppState>, query: &str) -> Option<String> {
    let scope = ConversationMemoryScope::new("local", "default")
        .expect("default memory scope must be valid");
    recall_context_scoped(state, query, &scope).await
}

pub async fn recall_context_scoped(
    state: &Arc<AppState>,
    query: &str,
    scope: &ConversationMemoryScope,
) -> Option<String> {
    if query.trim().is_empty() {
        return None;
    }
    let state = Arc::clone(state);
    let query = query.to_string();
    let scope = scope.clone();
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
        match recall_embedded_context(&conn, &scope, &query, &vector, top_k) {
            Ok(memories) => memories,
            Err(e) => {
                tracing::warn!("[RAG] search_hybrid_vectors that bai: {}", e);
                None
            }
        }
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
    let scope = ConversationMemoryScope::new("local", "default")
        .expect("default memory scope must be valid");
    persist_turn_scoped(state, user_text, reply, &scope).await;
}

pub async fn persist_turn_scoped(
    state: &Arc<AppState>,
    user_text: &str,
    reply: &str,
    scope: &ConversationMemoryScope,
) {
    if user_text.trim().is_empty() || reply.trim().is_empty() {
        return;
    }
    let state = Arc::clone(state);
    let scope = scope.clone();
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

        let Ok(conn) = state.db.writer.get() else {
            return;
        };
        if let Err(e) = persist_embedded_turn(&conn, &scope, &content, &vector) {
            tracing::warn!("[RAG] upsert_vector that bai: {}", e);
        }
    })
    .await;
}

const LLM_STREAM_ABORT_PREFIX: &str = "LLM stream aborted";
const LLM_TTS_BACKPRESSURE_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

fn send_llm_chunk_if_current(
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

fn finish_streamed_completion(
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
                Intent::SendMessage { recipient, body } => {
                    state
                        .context
                        .insert("message_to".to_string(), json!(recipient));
                    state.context.insert("message_text".to_string(), json!(body));
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
                    state.current_node = match crate::llm::tool_calling::select_tool(&ss, &text)
                        .await
                    {
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
                        format!("Công cụ {} chạy xong, không trả về văn bản.", call.qualified())
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

            let noi_dung = if text.trim().is_empty() {
                // Không có nội dung thì hỏi lại, KHÔNG dựng bản nháp rỗng.
                format!(
                    "Người dùng muốn nhắn tin cho '{to}' nhưng chưa nói nội dung. \
                     Hãy hỏi lại họ muốn nhắn gì."
                )
            } else {
                let payload = json!({ "to": to, "text": text });
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

#[cfg(test)]
mod tach_nhan_tin_tests {
    use super::{Intent, route_intent, tach_nhan_tin};

    fn tach(s: &str) -> (String, String) {
        tach_nhan_tin(s).unwrap_or_else(|| panic!("phai tach duoc: {s}"))
    }

    #[test]
    fn cau_that_cua_nguoi_dung() {
        // Đúng câu đã gõ vào widget và bị LIVA trả lời vòng vo.
        let (ten, noi_dung) = tach("nhắn tin cho Minh hiến bảo nó ngủ đi");
        assert_eq!(ten, "Minh hiến");
        assert_eq!(noi_dung, "ngủ đi", "dai tu 'no' phai bi bo khoi noi dung");
    }

    #[test]
    fn moi_cach_noi_co_deu_an() {
        for cau in [
            "nhắn cho Nam là mai đi học",
            "nhắn tin cho Nam là mai đi học",
            "gửi tin cho Nam là mai đi học",
            "gửi tin nhắn cho Nam là mai đi học",
        ] {
            let (ten, noi_dung) = tach(cau);
            assert_eq!(ten, "Nam", "sai ten o: {cau}");
            assert_eq!(noi_dung, "mai đi học", "sai noi dung o: {cau}");
        }
    }

    /// STT tiếng Việt bỏ dấu là chuyện thường; cò vẫn phải ăn.
    #[test]
    fn khong_dau_van_an() {
        let (ten, noi_dung) = tach("nhan cho Nam rang toi nay ranh");
        assert_eq!(ten, "Nam");
        assert_eq!(noi_dung, "toi nay ranh");
    }

    /// Lỗi đo được trên app thật 26/07/2026: "Lạ" bỏ dấu ra "la", trùng mốc
    /// "là", nên "Người Lạ Hoắc" bị cắt còn "Người". Tên người Việt đầy chữ
    /// trùng mốc sau khi bỏ dấu — `Bảo`, `Là`, `Nói` đều là tên có thật.
    #[test]
    fn ten_trung_moc_sau_khi_bo_dau_khong_bi_cat() {
        let (ten, noi_dung) = tach("nhắn cho Người Lạ Hoắc bảo alo");
        assert_eq!(ten, "Người Lạ Hoắc", "'Lạ' khong duoc coi la moc 'là'");
        assert_eq!(noi_dung, "alo");

        // `Bảo` là tên người rất phổ biến, và ở đây nó đứng ngay sau "cho" —
        // vị trí không bao giờ là mốc, vì người nhận không thể rỗng.
        let (ten2, noi_dung2) = tach("nhắn cho Bảo rằng mai đi học");
        assert_eq!(ten2, "Bảo", "'Bảo' dung ngay sau 'cho' thi van la ten");
        assert_eq!(noi_dung2, "mai đi học");
    }

    #[test]
    fn dau_hai_cham_cung_la_moc() {
        let (ten, noi_dung) = tach("nhắn cho Hiến: ngủ sớm nhé");
        assert_eq!(ten, "Hiến", "dau ':' phai bi cat khoi ten");
        assert_eq!(noi_dung, "ngủ sớm nhé");
    }

    #[test]
    fn khong_co_moc_thi_noi_dung_rong_chu_khong_doan() {
        let (ten, noi_dung) = tach("nhắn tin cho Minh Hiến");
        assert_eq!(ten, "Minh Hiến");
        assert!(noi_dung.is_empty(), "khong duoc bia noi dung: '{noi_dung}'");
    }

    #[test]
    fn dai_tu_hai_tu_cung_bi_bo() {
        assert_eq!(tach("nhắn cho Nam bảo anh ấy về sớm").1, "về sớm");
        // Nhưng "anh" đứng một mình là một phần nội dung, không phải đại từ.
        assert_eq!(tach("nhắn cho Nam bảo anh về sớm").1, "anh về sớm");
    }

    /// Bất biến quan trọng nhất của thứ tự nhánh: thân tin nhắn KHÔNG được một
    /// nhánh khác cướp mất. Nếu câu này ra `OsControl` thì LIVA bật nhạc của
    /// chính máy mình trong khi người dùng tưởng đã nhắn tin.
    #[test]
    fn than_tin_nhan_khong_bi_nhanh_khac_cuop() {
        match route_intent("nhắn cho Nam bảo bật nhạc lên") {
            Intent::SendMessage { recipient, body } => {
                assert_eq!(recipient, "Nam");
                assert_eq!(body, "bật nhạc lên");
            }
            khac => panic!("phai la SendMessage, nhan duoc {khac:?}"),
        }
        match route_intent("nhắn cho Nam bảo tắt đèn đi") {
            Intent::SendMessage { .. } => {}
            khac => panic!("phai la SendMessage, nhan duoc {khac:?}"),
        }
    }

    #[test]
    fn khong_phai_lenh_nhan_tin_thi_tra_none() {
        for cau in [
            "hôm nay trời thế nào",
            "nhắn tin nhiều quá",   // có "nhắn tin" nhưng không có "cho"
            "cho tôi xem màn hình", // có "cho" nhưng không có cò
            "nhắn cho",             // có cò nhưng không có người nhận
        ] {
            assert!(tach_nhan_tin(cau).is_none(), "khong duoc tach: {cau}");
        }
    }
}

#[cfg(test)]
mod stream_backpressure_tests {
    use super::{finish_streamed_completion, send_llm_chunk_if_current};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc;

    #[test]
    fn llm_chunk_queue_day_dung_sau_deadline() {
        let (tx, mut rx) = mpsc::channel(1);
        tx.try_send("first".to_string()).expect("fill queue");
        let active_session_id = AtomicU64::new(7);
        let started = Instant::now();

        let result = send_llm_chunk_if_current(
            &tx,
            &active_session_id,
            7,
            "second",
            Duration::from_millis(5),
        );

        assert!(result.is_err());
        assert!(
            started.elapsed() < Duration::from_millis(100),
            "LLM must not hold its process-wide mutex indefinitely on TTS backpressure",
        );
        assert_eq!(rx.try_recv().unwrap(), "first");
        assert!(rx.try_recv().is_err());
    }

    #[test]
    fn heartbeat_reasoning_rong_kiem_epoch_nhung_khong_chiem_tts_queue() {
        let (tx, mut rx) = mpsc::channel(1);
        let active_session_id = AtomicU64::new(7);

        send_llm_chunk_if_current(&tx, &active_session_id, 7, "", Duration::from_millis(5))
            .expect("heartbeat cua turn hien tai");

        assert!(
            rx.try_recv().is_err(),
            "heartbeat rong khong duoc vao TTS queue"
        );

        active_session_id.store(8, Ordering::SeqCst);
        assert!(
            send_llm_chunk_if_current(&tx, &active_session_id, 7, "", Duration::from_millis(5),)
                .expect_err("heartbeat epoch cu phai bi huy")
                .contains("cancelled"),
        );
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn barge_in_dung_llm_dang_cho_tts_backpressure() {
        let (tx, _rx) = mpsc::channel(1);
        tx.try_send("first".to_string()).expect("fill queue");
        let active_session_id = Arc::new(AtomicU64::new(7));
        let active_session_for_send = Arc::clone(&active_session_id);

        let send = tokio::task::spawn_blocking(move || {
            send_llm_chunk_if_current(
                &tx,
                &active_session_for_send,
                7,
                "second",
                Duration::from_secs(1),
            )
        });
        tokio::time::sleep(Duration::from_millis(5)).await;
        active_session_id.store(8, Ordering::SeqCst);

        let result = tokio::time::timeout(Duration::from_millis(100), send)
            .await
            .expect("barge-in must interrupt backpressure wait")
            .expect("join blocking sender");
        assert!(
            result
                .expect_err("cancelled session must reject the chunk")
                .contains("cancelled"),
        );
    }

    #[test]
    fn loi_stream_khong_duoc_coi_la_completion_de_persist() {
        let active_session_id = AtomicU64::new(7);

        let result = finish_streamed_completion(
            "partial answer".to_string(),
            Some("LLM stream aborted: TTS receiver closed".to_string()),
            &active_session_id,
            7,
        );

        assert!(result.is_err());
    }

    #[test]
    fn completion_epoch_cu_khong_duoc_persist() {
        let active_session_id = AtomicU64::new(8);

        let result =
            finish_streamed_completion("stale answer".to_string(), None, &active_session_id, 7);

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod router_tests {
    use super::{Intent, route_intent};

    fn smart(device: &'static str, action: &'static str) -> Intent {
        Intent::SmartHome { device, action }
    }

    fn os(tool: &'static str, action: &'static str) -> Intent {
        Intent::OsControl { tool, action }
    }

    /// U19: những câu mà model 2B trượt, đường nhanh phải xử đúng và tất định.
    ///
    /// Hai câu đầu là lý do mục này tồn tại — đo trên Qwen3-VL-2B, *"bật nhạc
    /// lên"* rơi sang chỉnh âm lượng và *"chuyển bài khác"* chọn sai hướng.
    #[test]
    fn dieu_khien_may_di_duong_nhanh() {
        assert_eq!(route_intent("bật nhạc lên"), os("control_media", "play_pause"));
        assert_eq!(route_intent("chuyển bài khác"), os("control_media", "next"));
        assert_eq!(route_intent("tạm dừng nhạc"), os("control_media", "play_pause"));
        assert_eq!(route_intent("quay lại bài trước"), os("control_media", "previous"));
        assert_eq!(route_intent("bài tiếp theo"), os("control_media", "next"));

        assert_eq!(route_intent("nhỏ nhạc lại giúp mình"), os("control_volume", "down"));
        assert_eq!(route_intent("giảm âm lượng xuống"), os("control_volume", "down"));
        assert_eq!(route_intent("tăng âm lượng lên"), os("control_volume", "up"));
        assert_eq!(route_intent("tắt tiếng đi"), os("control_volume", "mute"));
    }

    /// ĐỘ TO thắng ĐANG-PHÁT-GÌ khi câu có cả hai loại từ.
    ///
    /// "nhỏ nhạc lại" chứa cả "nhạc" (danh từ media) lẫn "nhỏ" (từ độ to). Ý
    /// người nói là âm lượng — cùng ranh giới đã ghi trong mô tả hai tool.
    #[test]
    fn do_to_thang_dang_phat_gi() {
        assert_eq!(route_intent("nhỏ nhạc lại"), os("control_volume", "down"));
        assert_eq!(route_intent("to nhạc lên"), os("control_volume", "up"));
        // Ngược lại: "tắt nhạc" là dừng phát, KHÔNG phải mute — nhánh mute đòi
        // đúng danh từ âm thanh ("tiếng", "âm lượng", "loa").
        assert_eq!(route_intent("tắt nhạc"), os("control_media", "play_pause"));
    }

    /// HỒI QUY U19: đường mới không được cướp câu của smart-home hay của
    /// những câu đời thường có chứa "bài".
    #[test]
    fn dieu_khien_may_khong_cuop_cau_khac() {
        // Không có danh từ âm thanh/nhạc ⇒ vẫn là smart-home.
        assert_eq!(route_intent("bật đèn"), smart("light", "on"));
        assert_eq!(route_intent("tắt quạt"), smart("fan", "off"));
        assert_eq!(route_intent("bật điều hoà"), smart("ac", "on"));

        // "bài" là từ rất thông dụng; có danh từ nhưng KHÔNG có động từ điều
        // khiển thì phải rơi về Chat, không được đoán bừa.
        assert_eq!(route_intent("làm bài tập xong chưa"), Intent::Chat);
        // Không có cò "cho" thì không phải lệnh nhắn tin.
        assert_eq!(route_intent("nhắn tin nhiều quá"), Intent::Chat);
        assert_eq!(route_intent("bài viết này hay đấy"), Intent::Chat);

        // Bẫy tiếng Anh: bảng từ khoá CỐ TÌNH chỉ có tiếng Việt, nên "track" và
        // "back" không thể thành "quay lại bài trước".
        assert_eq!(route_intent("let's get back on track"), Intent::Chat);
        assert_eq!(route_intent("play the next song"), Intent::Chat);
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
    use super::{
        ConversationMemoryScope, persist_embedded_turn, persist_turn, persist_turn_scoped,
        rag_top_k, recall_context, recall_context_scoped, recall_embedded_context,
    };
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

    #[test]
    fn memory_scope_cach_ly_owner_nhung_nho_xuyen_conversation() {
        let owner_a_conversation_1 =
            ConversationMemoryScope::new("telegram:100", "chat:1").expect("scope hop le");
        let owner_a_conversation_2 =
            ConversationMemoryScope::new("telegram:100", "chat:2").expect("scope hop le");
        let owner_b = ConversationMemoryScope::new("telegram:200", "chat:1").expect("scope hop le");

        let filter_a1 = owner_a_conversation_1.recall_filter();
        let filter_a2 = owner_a_conversation_2.recall_filter();
        let filter_b = owner_b.recall_filter();

        assert_eq!(filter_a1.r#type.as_deref(), Some("conversation_turn"));
        assert_eq!(
            filter_a1.domain, filter_a2.domain,
            "cung owner phai recall duoc ky uc xuyen conversation"
        );
        assert_ne!(
            filter_a1.domain, filter_b.domain,
            "hai owner khong duoc dung chung namespace RAG"
        );
        assert!(
            filter_a1
                .domain
                .as_deref()
                .is_some_and(|v| !v.trim().is_empty()),
            "owner filter la bat buoc, khong duoc roi ve truy van global"
        );
        assert!(
            filter_a1.category.is_none(),
            "recall dai han loc theo owner, khong khoa vao mot conversation"
        );
        assert_eq!(
            owner_a_conversation_1.storage_category(),
            "conversation:chat:1",
            "conversation id phai duoc luu lam lineage"
        );
    }

    #[test]
    fn memory_scope_tu_choi_dinh_danh_rong() {
        assert!(ConversationMemoryScope::new("", "chat:1").is_err());
        assert!(ConversationMemoryScope::new("   ", "chat:1").is_err());
        assert!(ConversationMemoryScope::new("telegram:100", "").is_err());
        assert!(ConversationMemoryScope::new("telegram:100", "   ").is_err());
    }

    #[test]
    fn persist_embedded_turn_noi_vao_event_ledger() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let scope = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();

        persist_embedded_turn(&conn, &scope, "ma du an ORION-7", &vector).unwrap();

        let (event_id, domain, category, source_event_ids): (String, String, String, String) = conn
            .query_row(
                "SELECT e.eventId, e.domain, e.category, m.source_event_ids \
                 FROM events e \
                 JOIN vectors_meta m ON m.vec_id = e.eventId \
                 WHERE m.content = ?1",
                ["ma du an ORION-7"],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )
            .unwrap();
        assert_eq!(domain, "memory_owner:telegram:100");
        assert_eq!(category, "conversation:chat:1");
        assert_eq!(source_event_ids, format!(r#"["{event_id}"]"#));
    }

    #[test]
    fn rag_khong_recall_cheo_owner_khi_vector_giong_nhau() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let owner_a = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();
        let owner_b = ConversationMemoryScope::new("telegram:200", "chat:2").unwrap();

        persist_embedded_turn(&conn, &owner_a, "shared secret owner A", &vector).unwrap();
        persist_embedded_turn(&conn, &owner_b, "shared secret owner B", &vector).unwrap();

        let recalled_a = recall_embedded_context(&conn, &owner_a, "shared secret", &vector, 10)
            .unwrap()
            .expect("owner A co ky uc");
        let recalled_b = recall_embedded_context(&conn, &owner_b, "shared secret", &vector, 10)
            .unwrap()
            .expect("owner B co ky uc");

        assert!(recalled_a.contains("owner A"));
        assert!(
            !recalled_a.contains("owner B"),
            "owner A doc duoc ky uc owner B"
        );
        assert!(recalled_b.contains("owner B"));
        assert!(
            !recalled_b.contains("owner A"),
            "owner B doc duoc ky uc owner A"
        );
    }

    #[test]
    fn telegram_group_khong_recall_ky_uc_dm_cua_cung_owner() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let dm_scope = ConversationMemoryScope::new("telegram:100", "telegram_chat:100").unwrap();
        let group_scope =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-200")
                .unwrap();

        persist_embedded_turn(&conn, &dm_scope, "wifi DM la Hunter2", &vector).unwrap();
        persist_embedded_turn(&conn, &group_scope, "noi dung rieng cua group", &vector).unwrap();

        let recalled_group = recall_embedded_context(&conn, &group_scope, "wifi", &vector, 10)
            .unwrap()
            .expect("group co ky uc trong cung audience");

        assert!(recalled_group.contains("noi dung rieng cua group"));
        assert!(
            !recalled_group.contains("Hunter2"),
            "group da recall ky uc DM cua cung owner"
        );
    }

    /// Bổ sung khuyến nghị review #2: hai GROUP khác nhau của CÙNG owner phải
    /// cách ly — ký ức group A KHÔNG rò vào audience của group B (mỗi group
    /// audience_scoped theo chat_id riêng → lọc qua `category=conversation`).
    #[test]
    fn hai_group_khac_nhau_cung_owner_khong_ro_cheo() {
        let db = crate::db::DatabasePool::new_in_memory().expect("in-memory db");
        let conn = db.writer.get().expect("writer");
        let vector = vec![0.01_f32; crate::db::MEMORY_VECTOR_DIM];
        let group_a =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-200")
                .unwrap();
        let group_b =
            ConversationMemoryScope::new_audience_scoped("telegram:100", "telegram_chat:-300")
                .unwrap();

        persist_embedded_turn(&conn, &group_a, "bi mat cua group A", &vector).unwrap();
        persist_embedded_turn(&conn, &group_b, "noi dung group B", &vector).unwrap();

        // Recall trong group B: chỉ thấy của B, KHÔNG thấy của A (dù vector giống
        // hệt và FTS 'bi mat' khớp nội dung A — category filter loại A ra).
        let recalled_b = recall_embedded_context(&conn, &group_b, "bi mat", &vector, 10)
            .unwrap()
            .expect("group B co ky uc cua chinh no");
        assert!(recalled_b.contains("noi dung group B"));
        assert!(
            !recalled_b.contains("group A"),
            "group B da recall ky uc cua group A (ro cheo audience)"
        );

        // Chiều ngược lại cũng phải cách ly.
        let recalled_a = recall_embedded_context(&conn, &group_a, "noi dung", &vector, 10)
            .unwrap()
            .expect("group A co ky uc cua chinh no");
        assert!(recalled_a.contains("group A"));
        assert!(
            !recalled_a.contains("group B"),
            "group A da recall ky uc cua group B (ro cheo audience)"
        );
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
        assert_eq!(
            dem_vector(&state),
            0,
            "khong co model thi khong duoc ghi ky uc nao"
        );
    }

    #[tokio::test]
    async fn scoped_rag_khong_co_embedder_van_fail_closed() {
        let state = state_khong_co_embedder();
        let scope = ConversationMemoryScope::new("telegram:100", "chat:1").unwrap();

        assert!(
            recall_context_scoped(&state, "ky uc rieng", &scope)
                .await
                .is_none()
        );
        persist_turn_scoped(&state, "cau hoi", "cau tra loi", &scope).await;
        assert_eq!(dem_vector(&state), 0);
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
