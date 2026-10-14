//! Đầu mối LLM trong [`crate::AppState`].
//!
//! `f041c08` đổi `AppState.llm` từ `Mutex<LlamaRouterManager>` sang
//! [`liva_llm::LlmActorHandle`] nhưng không để lại đường nào nạp model vào
//! engine của actor: mọi đường "nạp" chỉ còn ghi metadata, nên actor trả về
//! `"Simulated response to: <prompt>"` cho mọi câu hỏi (đo lại 10/10/2026 trên
//! build debug). `LlmService` giữ cả hai thứ: actor cho các lệnh sinh văn bản
//! xếp hàng theo ưu tiên, và chính engine đó — dùng chung sau một `Mutex` — cho
//! những việc cần model thật: nạp/tráo model, sinh có ngân sách prompt, hỏi
//! kèm ảnh.

use super::LlamaRouterManager;
use std::sync::Arc;

/// Engine llama.cpp dùng chung giữa actor và các lệnh cần model.
pub type SharedRouter = Arc<tokio::sync::Mutex<LlamaRouterManager>>;

#[derive(Clone)]
pub struct LlmService {
    actor: liva_llm::LlmActorHandle,
    router: Option<SharedRouter>,
}

impl LlmService {
    pub fn new(actor: liva_llm::LlmActorHandle, router: SharedRouter) -> Self {
        Self {
            actor,
            router: Some(router),
        }
    }

    /// Engine thật; `None` với state kiểm thử dựng từ một actor giả.
    pub fn router(&self) -> Option<&SharedRouter> {
        self.router.as_ref()
    }

    /// Như [`Self::router`] nhưng trả lỗi chuẩn "No model loaded" khi không có engine.
    pub fn require_router(&self) -> Result<SharedRouter, String> {
        self.router
            .clone()
            .ok_or_else(|| super::engine::ERR_NO_MODEL.to_string())
    }
}

impl std::ops::Deref for LlmService {
    type Target = liva_llm::LlmActorHandle;

    fn deref(&self) -> &Self::Target {
        &self.actor
    }
}

impl From<liva_llm::LlmActorHandle> for LlmService {
    fn from(actor: liva_llm::LlmActorHandle) -> Self {
        Self {
            actor,
            router: None,
        }
    }
}
