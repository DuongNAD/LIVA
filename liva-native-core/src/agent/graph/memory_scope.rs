use crate::AppState;
use std::sync::Arc;

/// Số ký ức lấy ra mỗi lượt. Đặt qua `LIVA_RAG_TOP_K` (mặc định 3).
///
/// Giữ nhỏ có chủ ý: mỗi ký ức chèn thêm token vào prompt, mà `n_ctx` mặc định
/// chỉ 4096 và người dùng beta chạy model 2–4B.
pub(super) fn rag_top_k() -> usize {
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

pub(super) fn persist_embedded_turn(
    conn: &rusqlite::Connection,
    engine: &crate::crypto::EncryptionEngine,
    scope: &ConversationMemoryScope,
    content: &str,
    vector: &[f32],
) -> Result<(), rusqlite::Error> {
    let vec_id = format!("turn_{}", uuid::Uuid::new_v4());
    crate::db::persist_conversation_event_vector(
        conn,
        engine,
        &vec_id,
        content,
        vector,
        scope.storage_domain(),
        scope.storage_category(),
    )
}

pub(super) fn recall_embedded_context(
    conn: &rusqlite::Connection,
    engine: &crate::crypto::EncryptionEngine,
    scope: &ConversationMemoryScope,
    query: &str,
    vector: &[f32],
    top_k: usize,
) -> Result<Option<String>, rusqlite::Error> {
    let hits = crate::db::search_hybrid_vectors(
        conn,
        engine,
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
        match recall_embedded_context(&conn, &state.crypto, &scope, &query, &vector, top_k) {
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
        if let Err(e) = persist_embedded_turn(&conn, &state.crypto, &scope, &content, &vector) {
            tracing::warn!("[RAG] upsert_vector that bai: {}", e);
        }
    })
    .await;
}
