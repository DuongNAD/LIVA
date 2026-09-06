//! Nhắn tin ra ngoài: danh bạ → bản nháp → **xác nhận** → gửi.
//!
//! Bốn chặng, và chặng thứ ba không bỏ qua được. Xem [`outbox`] để biết vì sao.
//!
//! ## Bản đồ
//!
//! - [`contacts`] — tên người → địa chỉ đích. Ánh xạ này là mảnh còn thiếu khiến
//!   "nhắn cho Minh Hiến" trước đây không có đường nào tới lệnh gửi.
//! - [`outbox`] — hộp chờ xác nhận. Giữ chữ, không gửi.
//! - [`send`] — cửa ra duy nhất, và nó chỉ nhận [`outbox::Draft`], thứ chỉ lấy
//!   được bằng [`outbox::take`].
//!
//! ## Vì sao `send` nhận `Draft` chứ không nhận `(handle, text)`
//!
//! Để bất biến "không gửi nếu chưa xác nhận" được **kiểu dữ liệu** giữ hộ, chứ
//! không phải trông vào kỷ luật của người gọi. Muốn gọi `send` thì phải có
//! `Draft`; muốn có `Draft` thì phải `take` từ hộp; `take` chỉ thành công một
//! lần và chỉ khi chưa hết hạn. Người viết lệnh mới sau này không cần đọc tài
//! liệu này mới làm đúng.

pub mod contacts;
pub mod normalized;
pub mod outbox;
pub mod session;
mod voice_dialogue;

pub use normalized::{
    Attachment, AttachmentSource, ChannelId, ContentPayload, DeliveryReceipt, DeliveryState,
    DeliveryUrgency, IncomingMessage, MessageId, MessageRecipient, MessageSender, OutgoingContent,
    OutgoingMessage, SenderRole, TextEntity, TextEntityType,
};
pub use session::{
    InMemorySessionManager, MemoryScope, SessionContext, SessionError, SessionId, SessionManager,
    SessionState,
};
pub use voice_dialogue::{VoiceMessageAction, VoiceMessageDialogue};

use contacts::Platform;
use outbox::Draft;

/// Gửi một bản nháp **đã được xác nhận**.
///
/// Trả về câu mô tả việc đã làm, để lớp lệnh trả thẳng cho UI.
pub async fn send(draft: Draft) -> Result<String, String> {
    match draft.platform {
        Platform::Telegram => gui_telegram(&draft).await,
        Platform::Messenger => crate::integrations::messenger::send(&draft.handle, &draft.text)
            .await
            .map(|_| format!("Đã gửi Messenger cho {}", draft.display_name)),
    }
}

/// Gửi qua Telegram Bot API, **chờ kết quả**.
///
/// Khác chỗ cũ ở đúng một điểm, và điểm đó là cả vấn đề: `commands/integrations.rs`
/// `tokio::spawn` lời gọi rồi trả `{"success": true}` ngay, nên "thành công" chỉ
/// có nghĩa *"token có tồn tại"*. Ở đây `await` để `Ok` có nghĩa **Telegram đã
/// nhận tin**. Với một hành động không hoàn tác được thì báo sai chiều đó —
/// nói đã gửi trong khi chưa — là kiểu sai tệ nhất: người dùng tưởng bạn mình
/// đã đọc rồi.
async fn gui_telegram(draft: &Draft) -> Result<String, String> {
    use teloxide::prelude::Requester;

    let chat_id: i64 = draft.handle.parse().map_err(|_| {
        format!(
            "chat id '{}' không phải số — danh bạ hỏng, sửa lại bằng contacts:upsert",
            draft.handle
        )
    })?;

    let token = std::env::var("TELEGRAM_BOT_TOKEN")
        .map_err(|_| "Thiếu TELEGRAM_BOT_TOKEN — chưa cấu hình bot Telegram".to_string())?;

    teloxide::prelude::Bot::new(token)
        .send_message(teloxide::prelude::ChatId(chat_id), &draft.text)
        .await
        .map_err(|e| format!("Telegram từ chối: {e}"))?;

    Ok(format!("Đã gửi Telegram cho {}", draft.display_name))
}
