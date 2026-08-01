//! Miền tích hợp ngoài — Telegram và nhà thông minh.
//!
//! Tách khỏi `handle_command` 26/07/2026 (B1 bước 7). Ba nhánh, miền nhỏ nhất —
//! nhưng cũng là miền duy nhất **đi ra khỏi máy**.
//!
//! ## Một chỗ đáng nhìn kỹ khi đọc miền này
//!
//! `telegram:send_text` là **fire-and-forget**: nó `tokio::spawn` lời gọi HTTP
//! rồi trả `{"success": true}` NGAY, không chờ kết quả. Nghĩa là "success" ở
//! đây chỉ có nghĩa *"đã nhận lệnh và token có tồn tại"*, không có nghĩa
//! *"Telegram đã nhận tin"*.
//!
//! Đó là hành vi cũ, giữ nguyên trong bước dời này. Nhưng nó đáng ghi ra vì
//! `tests/verify_commands.rs` từng assert `{"success": true}` cho lệnh này và
//! coi đó là bằng chứng gửi được — một assertion vô nghĩa, và tệ hơn: nó khiến
//! `cargo test` trong CI phát sinh một request mạng THẬT ra `api.telegram.org`
//! (lỗi bị nuốt vì fire-and-forget). Xem `docs/02-van-hanh/04-kiem-thu-va-ci.md`.

use crate::{AppState, integrations};
use serde_json::{Value, json};
use std::sync::Arc;

const OWNED: &[&str] = &[
    "telegram:send_text",
    "integration:smart_home_control",
    "integrations:list",
];

/// Lệnh này có thuộc miền tích hợp ngoài không.
///
/// Ba tiền tố khác nhau cho ba lệnh (`telegram:`, `integration:`,
/// `integrations:` — hai cái sau chỉ khác một chữ `s`), nên `strip_prefix` vừa
/// không gom được vừa dễ gõ nhầm. Danh sách tường minh là cách duy nhất đọc ra
/// được rằng `integration:` và `integrations:` là HAI tiền tố có thật, không
/// phải một lỗi chính tả.
pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    let _ = &state; // miền này chưa cần AppState; giữ chữ ký đồng nhất với các miền khác
    match command {
        "telegram:send_text" => send_text(payload),
        "integration:smart_home_control" => {
            let result = integrations::smart_home::execute(payload)?;
            Ok(json!({ "result": result }))
        }
        "integrations:list" => Ok(json!([integrations::smart_home::get_metadata()])),
        _ => Err(format!("Unknown command: {command}")),
    }
}

/// Gửi tin Telegram. **Fire-and-forget** — xem ghi chú đầu module.
fn send_text(payload: Value) -> Result<Value, String> {
    let chat_id_str = payload["chatId"]
        .as_str()
        .ok_or("Missing chatId")?
        .to_string();
    let text = payload["text"].as_str().ok_or("Missing text")?.to_string();

    let chat_id = chat_id_str
        .parse::<i64>()
        .map_err(|e| format!("Invalid chatId: {}", e))?;

    let token = std::env::var("TELEGRAM_BOT_TOKEN").map_err(|_| "Bot token missing")?;
    let bot = teloxide::prelude::Bot::new(token);
    tokio::spawn(async move {
        use teloxide::prelude::Requester;
        let _ = bot
            .send_message(teloxide::prelude::ChatId(chat_id), text)
            .await;
    });

    Ok(json!({ "success": true }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_dung_ba_lenh_va_phan_biet_hai_tien_to_gan_giong() {
        assert_eq!(OWNED.len(), 3);
        for name in OWNED {
            assert!(owns(name));
        }
        // `integration:` (số ít) và `integrations:` (số nhiều) là HAI tiền tố
        // có thật; đổi chỗ chúng là lệnh không tồn tại.
        assert!(owns("integration:smart_home_control"));
        assert!(owns("integrations:list"));
        assert!(!owns("integrations:smart_home_control"));
        assert!(!owns("integration:list"));
    }

    /// Thiếu tham số phải trả `Err` gọn, không panic — lệnh này nhận payload từ
    /// WebSocket, tức là dữ liệu không tin cậy.
    #[test]
    fn send_text_thieu_tham_so_tra_err_khong_panic() {
        assert!(send_text(json!({})).is_err());
        assert!(send_text(json!({ "chatId": "123" })).is_err());
        // chatId không phải số → Err có nêu lý do
        let err = send_text(json!({ "chatId": "abc", "text": "hi" })).unwrap_err();
        assert!(
            err.contains("chatId"),
            "thông điệp phải nêu trường sai: {err}"
        );
    }
}
