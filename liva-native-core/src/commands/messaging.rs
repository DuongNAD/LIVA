//! Miền nhắn tin ra ngoài: danh bạ + hộp chờ xác nhận + gửi.
//!
//! ## Hình dạng của API, và vì sao nó tách làm hai nhịp
//!
//! `message:draft` **không gửi gì**. Nó tra danh bạ, dựng bản nháp, trả về cho
//! UI vẽ thẻ xác nhận. Chỉ `message:confirm` mới gửi, và nó chỉ nhận `draftId`
//! — không nhận nội dung. Nghĩa là không có cách nào gọi một lệnh duy nhất để
//! đẩy chữ tuỳ ý ra ngoài; muốn gửi thì phải có một bản nháp đã tồn tại, mà bản
//! nháp thì người dùng đã đọc.
//!
//! Điều này quan trọng hơn bình thường ở đây vì WebSocket 8002 **chưa có xác
//! thực** (`docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md`, mục C1). Trước bản
//! này, miền "đi ra khỏi máy" duy nhất là `telegram:send_text` — một lệnh gửi
//! thẳng, không nhịp nào chặn. Bản này không sửa được lỗ hổng C1, nhưng nó cố ý
//! **không nới rộng** bề mặt đó: kênh mới nào cũng phải qua hai nhịp.
//!
//! ## Quan hệ với `commands::integrations`
//!
//! `telegram:send_text` cũ vẫn còn, vẫn fire-and-forget — không đụng tới trong
//! bản này để khỏi phá thứ đang dùng. Đường mới (`message:*`) chờ kết quả thật.
//! Hai đường cùng tồn tại là có chủ ý và tạm thời; ghi ra đây để lần sau không
//! ai tưởng đó là trùng lặp vô tình.

use crate::AppState;
use crate::messaging::{contacts, outbox};
use serde_json::{Value, json};
use std::sync::Arc;

const OWNED: &[&str] = &[
    "contacts:list",
    "contacts:upsert",
    "contacts:delete",
    "message:draft",
    "message:confirm",
    "message:cancel",
    "message:pending",
    "messenger:status",
];

pub fn owns(command: &str) -> bool {
    OWNED.contains(&command)
}

pub async fn handle(state: Arc<AppState>, command: &str, payload: Value) -> Result<Value, String> {
    match command {
        "contacts:list" => list(state, payload).await,
        "contacts:upsert" => upsert(state, payload).await,
        "contacts:delete" => delete(state, payload).await,
        "message:draft" => draft(state, payload).await,
        "message:confirm" => confirm(payload).await,
        "message:cancel" => Ok(json!({
            "cancelled": outbox::cancel(chuoi_bat_buoc(&payload, "draftId")?.as_str())
        })),
        "message:pending" => Ok(json!({ "drafts": outbox::pending() })),
        "messenger:status" => crate::integrations::messenger::status().await,
        _ => Err(format!("Unknown command: {command}")),
    }
}

fn chuoi_bat_buoc(payload: &Value, key: &str) -> Result<String, String> {
    payload[key]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("Thiếu '{key}' trong payload"))
}

/// `platform` tuỳ chọn: thiếu thì tra cả hai nền.
fn nen_tuy_chon(payload: &Value) -> Result<Option<contacts::Platform>, String> {
    match payload.get("platform").and_then(Value::as_str) {
        None => Ok(None),
        Some(s) if s.trim().is_empty() => Ok(None),
        Some(s) => contacts::Platform::parse(s).map(Some),
    }
}

async fn list(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let nen = nen_tuy_chon(&payload)?;
    let ds = tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .readers
            .get()
            .map_err(|e| format!("Không lấy được kết nối đọc: {e}"))?;
        contacts::list(&conn, nen)
    })
    .await
    .map_err(|e| format!("Tác vụ chặn panic: {e}"))??;

    Ok(json!({ "contacts": ds }))
}

async fn upsert(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let ten = chuoi_bat_buoc(&payload, "name")?;
    let nen = contacts::Platform::parse(&chuoi_bat_buoc(&payload, "platform")?)?;
    let handle = chuoi_bat_buoc(&payload, "handle")?;
    let note = payload
        .get("note")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let c = tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .writer
            .get()
            .map_err(|e| format!("Không lấy được kết nối ghi: {e}"))?;
        contacts::upsert(&conn, &ten, nen, &handle, &note)
    })
    .await
    .map_err(|e| format!("Tác vụ chặn panic: {e}"))??;

    Ok(json!({ "success": true, "contact": c }))
}

async fn delete(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let id = chuoi_bat_buoc(&payload, "contactId")?;
    let xoa_duoc = tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .writer
            .get()
            .map_err(|e| format!("Không lấy được kết nối ghi: {e}"))?;
        contacts::delete(&conn, &id)
    })
    .await
    .map_err(|e| format!("Tác vụ chặn panic: {e}"))??;

    Ok(json!({ "deleted": xoa_duoc }))
}

/// Dựng bản nháp. **Không gửi.**
///
/// Ba kết quả có thể, và cả ba đều là `Ok` — vì "không tìm ra người" là một câu
/// trả lời hợp lệ cho UI hiển thị, không phải lỗi hệ thống:
/// - `needsConfirm: true` + `draft` — tìm ra đúng một người.
/// - `ambiguous` — nhiều người khớp; UI phải hỏi lại, LIVA **không tự chọn**.
/// - `notFound` — chưa có trong danh bạ.
async fn draft(state: Arc<AppState>, payload: Value) -> Result<Value, String> {
    let to = chuoi_bat_buoc(&payload, "to")?;
    let text = chuoi_bat_buoc(&payload, "text")?;
    if text.trim().is_empty() {
        return Err("Nội dung tin nhắn rỗng".to_string());
    }
    let nen = nen_tuy_chon(&payload)?;

    let to_cho_truy_van = to.clone();
    let kq = tokio::task::spawn_blocking(move || {
        let conn = state
            .db
            .readers
            .get()
            .map_err(|e| format!("Không lấy được kết nối đọc: {e}"))?;
        contacts::resolve(&conn, &to_cho_truy_van, nen)
    })
    .await
    .map_err(|e| format!("Tác vụ chặn panic: {e}"))??;

    match kq {
        contacts::Resolution::NotFound => Ok(json!({
            "notFound": true,
            "query": to,
            "hint": "Chưa có ai tên này trong danh bạ. Thêm bằng contacts:upsert \
                     {name, platform, handle}.",
        })),
        contacts::Resolution::Ambiguous(ds) => Ok(json!({
            "ambiguous": true,
            "query": to,
            "candidates": ds,
            "hint": "Nhiều người khớp — nêu rõ nền hoặc tên đầy đủ. Không tự chọn hộ.",
        })),
        contacts::Resolution::Found(c) => {
            let nen = contacts::Platform::parse(&c.platform)?;
            let d = outbox::stage(nen, &c.display_name, &c.handle, &text);
            Ok(json!({
                "needsConfirm": true,
                "draft": d,
                "ttlSecs": outbox::TTL_SECS,
            }))
        }
    }
}

/// Xác nhận và gửi. Đây là **lệnh duy nhất** đẩy chữ ra khỏi máy theo đường này.
async fn confirm(payload: Value) -> Result<Value, String> {
    let id = chuoi_bat_buoc(&payload, "draftId")?;

    // `take` tiêu bản nháp. Nếu nó trả None thì hoặc đã gửi rồi, hoặc đã huỷ,
    // hoặc hết hạn — cả ba đều KHÔNG được gửi, và thông điệp phải phân biệt
    // được với "gửi thất bại" để người dùng không bấm lại vô ích.
    let d = outbox::take(&id).ok_or_else(|| {
        format!(
            "Bản nháp '{id}' không còn: đã gửi, đã huỷ, hoặc quá hạn {} giây. \
             Nói lại yêu cầu để tạo bản mới.",
            outbox::TTL_SECS
        )
    })?;

    let mo_ta = crate::messaging::send(d).await?;
    Ok(json!({ "sent": true, "detail": mo_ta }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owns_dung_tam_lenh() {
        assert_eq!(OWNED.len(), 8);
        for name in OWNED {
            assert!(owns(name), "{name}");
        }
        assert!(!owns("message:send"), "khong duoc co lenh gui mot nhip");
        assert!(!owns("contacts:upser"));
    }

    /// Bất biến của cả miền: **không có lệnh nào gửi trong một nhịp**. Nếu ai đó
    /// thêm `message:send` thì test này đỏ, và đó đúng là điều cần xảy ra.
    #[test]
    fn khong_co_duong_gui_mot_nhip() {
        for name in OWNED {
            let la_gui = name.starts_with("message:") && !name.contains("confirm");
            if la_gui {
                assert!(
                    ["message:draft", "message:cancel", "message:pending"].contains(name),
                    "lenh '{name}' co ve gui truc tiep — moi duong gui phai qua message:confirm"
                );
            }
        }
    }

    #[tokio::test]
    async fn confirm_voi_draft_id_khong_ton_tai_bao_ro_ly_do() {
        let e = confirm(json!({ "draftId": "dr_khong_co_that" }))
            .await
            .unwrap_err();
        assert!(e.contains("không còn"), "{e}");
        assert!(e.contains("quá hạn"), "phai neu ca kha nang het han: {e}");
    }

    #[tokio::test]
    async fn confirm_thieu_tham_so_khong_panic() {
        assert!(confirm(json!({})).await.is_err());
    }

    #[test]
    fn nen_tuy_chon_chap_nhan_bi_danh_cua_messenger() {
        for s in ["messenger", "facebook", "fb", "FB", " mess "] {
            assert!(
                matches!(
                    nen_tuy_chon(&json!({ "platform": s })),
                    Ok(Some(contacts::Platform::Messenger))
                ),
                "'{s}' phai ra Messenger"
            );
        }
        assert!(matches!(nen_tuy_chon(&json!({})), Ok(None)));
        assert!(nen_tuy_chon(&json!({ "platform": "sms" })).is_err());
    }
}
