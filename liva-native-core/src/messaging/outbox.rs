//! Hộp chờ xác nhận bền vững giữa “LIVA hiểu ý” và “tin nhắn rời khỏi máy”.
//!
//! Bản nháp nằm trong SQLite nên không mất khi tiến trình khởi động lại. Nội
//! dung tin được mã hóa bằng cùng khóa dữ liệu của LIVA; metadata người nhận
//! giữ nguyên để khớp với bảng `contacts`. Mọi thao tác thay đổi trạng thái dùng
//! transaction `IMMEDIATE`, vì `take` phải là cửa tiêu thụ đúng một lần ngay cả
//! khi hai yêu cầu xác nhận đến đồng thời.

use crate::crypto::{EncryptionEngine, FactRead};
use rusqlite::{Connection, OptionalExtension, Transaction, TransactionBehavior, params};
use serde::Serialize;

use super::contacts::Platform;

/// Bản nháp sống bao lâu trước khi phải nói lại từ đầu.
pub const TTL_SECS: u64 = 300;

/// Số bản nháp tối đa giữ cùng lúc.
pub const MAX_PENDING: usize = 32;

#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub draft_id: String,
    pub platform: Platform,
    /// Tên hiển thị mà người dùng đọc trên thẻ xác nhận.
    pub display_name: String,
    /// Địa chỉ đích thật mà adapter gửi tin sử dụng.
    pub handle: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Debug, Clone)]
struct StoredDraft {
    draft_id: String,
    platform: String,
    display_name: String,
    handle: String,
    text_ciphertext: String,
    created_at: i64,
}

/// Kết quả tiêu thụ bản nháp, tách rõ lý do để UI không nói dối rằng restart
/// làm mất dữ liệu hoặc khuyến khích người dùng bấm lại một hành động đã gửi.
#[derive(Debug, Clone)]
pub enum TakeResult {
    Taken(Draft),
    Expired,
    Missing,
    /// Có hàng nhưng khóa hiện tại không giải mã được. Hàng được giữ nguyên để
    /// có thể phục hồi bằng đúng khóa; tuyệt đối không gửi ciphertext.
    Locked,
}

fn bay_gio() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

fn het_han(created_at: i64, now: i64) -> bool {
    now.saturating_sub(created_at) >= TTL_SECS as i64
}

fn row_to_stored(row: &rusqlite::Row<'_>) -> rusqlite::Result<StoredDraft> {
    Ok(StoredDraft {
        draft_id: row.get(0)?,
        platform: row.get(1)?,
        display_name: row.get(2)?,
        handle: row.get(3)?,
        text_ciphertext: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn decrypt_stored(stored: StoredDraft, crypto: &EncryptionEngine) -> Result<Option<Draft>, String> {
    let text = match crypto.read_fact(&stored.text_ciphertext) {
        FactRead::Ok(text) => text,
        FactRead::Locked { .. } => return Ok(None),
    };
    let platform = Platform::parse(&stored.platform)?;
    Ok(Some(Draft {
        draft_id: stored.draft_id,
        platform,
        display_name: stored.display_name,
        handle: stored.handle,
        text,
        created_at: stored.created_at.max(0) as u64,
    }))
}

fn begin_immediate(conn: &Connection) -> Result<Transaction<'_>, String> {
    Transaction::new_unchecked(conn, TransactionBehavior::Immediate)
        .map_err(|e| format!("Không khóa được outbox để cập nhật: {e}"))
}

fn delete_expired(tx: &Transaction<'_>, now: i64) -> Result<usize, String> {
    tx.execute(
        "DELETE FROM message_outbox WHERE created_at <= ?1",
        [now.saturating_sub(TTL_SECS as i64)],
    )
    .map_err(|e| format!("Không dọn được bản nháp hết hạn: {e}"))
}

/// Ghi một bản nháp mã hóa vào SQLite và trả dữ liệu rõ để UI xác nhận.
pub fn stage(
    conn: &Connection,
    crypto: &EncryptionEngine,
    platform: Platform,
    display_name: &str,
    handle: &str,
    text: &str,
) -> Result<Draft, String> {
    let now = bay_gio();
    let draft_id = format!("dr_{}", rand::random::<u64>());
    let text_ciphertext = crypto
        .encrypt(text)
        .map_err(|e| format!("Không mã hóa được bản nháp: {e}"))?;
    let tx = begin_immediate(conn)?;
    delete_expired(&tx, now)?;

    let count: i64 = tx
        .query_row("SELECT COUNT(*) FROM message_outbox", [], |row| row.get(0))
        .map_err(|e| format!("Không đếm được outbox: {e}"))?;
    if count >= MAX_PENDING as i64 {
        let excess = count - MAX_PENDING as i64 + 1;
        tx.execute(
            "DELETE FROM message_outbox
             WHERE seq IN (
                 SELECT seq FROM message_outbox
                 ORDER BY created_at ASC, seq ASC
                 LIMIT ?1
             )",
            [excess],
        )
        .map_err(|e| format!("Không giới hạn được outbox: {e}"))?;
    }

    tx.execute(
        "INSERT INTO message_outbox
         (draft_id, platform, display_name, handle, text_ciphertext, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        params![
            draft_id,
            platform.as_str(),
            display_name,
            handle,
            text_ciphertext,
            now
        ],
    )
    .map_err(|e| format!("Không ghi được bản nháp: {e}"))?;
    tx.commit()
        .map_err(|e| format!("Không commit được bản nháp: {e}"))?;

    Ok(Draft {
        draft_id,
        platform,
        display_name: display_name.to_string(),
        handle: handle.to_string(),
        text: text.to_string(),
        created_at: now as u64,
    })
}

/// Xem một bản nháp mà không tiêu nó.
pub fn peek(
    conn: &Connection,
    crypto: &EncryptionEngine,
    draft_id: &str,
) -> Result<Option<Draft>, String> {
    let now = bay_gio();
    let tx = begin_immediate(conn)?;
    delete_expired(&tx, now)?;
    let stored = tx
        .query_row(
            "SELECT draft_id, platform, display_name, handle, text_ciphertext, created_at
             FROM message_outbox WHERE draft_id = ?1",
            [draft_id],
            row_to_stored,
        )
        .optional()
        .map_err(|e| format!("Không đọc được bản nháp: {e}"))?;
    tx.commit()
        .map_err(|e| format!("Không kết thúc được lượt đọc outbox: {e}"))?;
    stored
        .map(|row| decrypt_stored(row, crypto))
        .transpose()
        .map(Option::flatten)
}

/// Tiêu thụ bản nháp đúng một lần. Hàng được xóa trong transaction trước khi
/// adapter gửi tin chạy; retry sau lỗi mạng vì thế không thể gửi trùng.
pub fn take(
    conn: &Connection,
    crypto: &EncryptionEngine,
    draft_id: &str,
) -> Result<TakeResult, String> {
    let now = bay_gio();
    let tx = begin_immediate(conn)?;
    let stored = tx
        .query_row(
            "SELECT draft_id, platform, display_name, handle, text_ciphertext, created_at
             FROM message_outbox WHERE draft_id = ?1",
            [draft_id],
            row_to_stored,
        )
        .optional()
        .map_err(|e| format!("Không đọc được bản nháp để xác nhận: {e}"))?;

    let Some(stored) = stored else {
        tx.commit()
            .map_err(|e| format!("Không kết thúc được lượt xác nhận: {e}"))?;
        return Ok(TakeResult::Missing);
    };
    if het_han(stored.created_at, now) {
        tx.execute("DELETE FROM message_outbox WHERE draft_id = ?1", [draft_id])
            .map_err(|e| format!("Không dọn được bản nháp hết hạn: {e}"))?;
        tx.commit()
            .map_err(|e| format!("Không commit được việc dọn outbox: {e}"))?;
        return Ok(TakeResult::Expired);
    }

    let Some(draft) = decrypt_stored(stored, crypto)? else {
        tx.commit()
            .map_err(|e| format!("Không kết thúc được lượt đọc outbox bị khóa: {e}"))?;
        return Ok(TakeResult::Locked);
    };
    let deleted = tx
        .execute("DELETE FROM message_outbox WHERE draft_id = ?1", [draft_id])
        .map_err(|e| format!("Không tiêu được bản nháp: {e}"))?;
    if deleted != 1 {
        return Err("Outbox thay đổi ngoài transaction; từ chối gửi để tránh trùng".to_string());
    }
    tx.commit()
        .map_err(|e| format!("Không commit được xác nhận bản nháp: {e}"))?;
    Ok(TakeResult::Taken(draft))
}

/// Hủy một bản nháp mà không gửi.
pub fn cancel(conn: &Connection, draft_id: &str) -> Result<bool, String> {
    let now = bay_gio();
    let tx = begin_immediate(conn)?;
    delete_expired(&tx, now)?;
    let deleted = tx
        .execute("DELETE FROM message_outbox WHERE draft_id = ?1", [draft_id])
        .map_err(|e| format!("Không hủy được bản nháp: {e}"))?;
    tx.commit()
        .map_err(|e| format!("Không commit được việc hủy bản nháp: {e}"))?;
    Ok(deleted == 1)
}

/// Liệt kê bản nháp còn chờ, mới nhất trước. Nếu có hàng không giải mã được,
/// trả lỗi fail-closed thay vì đưa ciphertext lên UI hoặc âm thầm làm mất hàng.
pub fn pending(conn: &Connection, crypto: &EncryptionEngine) -> Result<Vec<Draft>, String> {
    let now = bay_gio();
    let tx = begin_immediate(conn)?;
    delete_expired(&tx, now)?;
    let stored = {
        let mut statement = tx
            .prepare(
                "SELECT draft_id, platform, display_name, handle, text_ciphertext, created_at
                 FROM message_outbox
                 ORDER BY created_at DESC, seq DESC",
            )
            .map_err(|e| format!("Không chuẩn bị được truy vấn outbox: {e}"))?;
        let rows = statement
            .query_map([], row_to_stored)
            .map_err(|e| format!("Không đọc được outbox: {e}"))?;
        rows.collect::<rusqlite::Result<Vec<_>>>()
            .map_err(|e| format!("Không đọc được hàng outbox: {e}"))?
    };
    tx.commit()
        .map_err(|e| format!("Không kết thúc được lượt liệt kê outbox: {e}"))?;

    stored
        .into_iter()
        .map(|row| {
            decrypt_stored(row, crypto)?.ok_or_else(|| {
                "Outbox có bản nháp bị khóa bởi khóa mã hóa khác; từ chối trả ciphertext"
                    .to_string()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DatabasePool;

    fn fixture() -> (DatabasePool, EncryptionEngine) {
        (
            DatabasePool::new_in_memory().expect("create test database"),
            EncryptionEngine::new("outbox-unit-test-key-32-bytes"),
        )
    }

    fn moi(conn: &Connection, crypto: &EncryptionEngine) -> Draft {
        stage(
            conn,
            crypto,
            Platform::Telegram,
            "Minh Hiến",
            "12345",
            "ngủ đi",
        )
        .expect("stage draft")
    }

    #[test]
    fn stage_roi_take_ra_dung_noi_dung_va_dung_mot_lan() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let d = moi(&conn, &crypto);
        let first = take(&conn, &crypto, &d.draft_id).unwrap();
        assert!(matches!(
            first,
            TakeResult::Taken(ref value)
                if value.text == "ngủ đi"
                    && value.handle == "12345"
                    && value.display_name == "Minh Hiến"
        ));
        assert!(matches!(
            take(&conn, &crypto, &d.draft_id).unwrap(),
            TakeResult::Missing
        ));
    }

    #[test]
    fn peek_khong_tieu_ban_nhap() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let d = moi(&conn, &crypto);
        assert!(peek(&conn, &crypto, &d.draft_id).unwrap().is_some());
        assert!(peek(&conn, &crypto, &d.draft_id).unwrap().is_some());
        assert!(matches!(
            take(&conn, &crypto, &d.draft_id).unwrap(),
            TakeResult::Taken(_)
        ));
    }

    #[test]
    fn ban_nhap_het_han_duoc_phan_loai_ro() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let d = moi(&conn, &crypto);
        conn.execute(
            "UPDATE message_outbox SET created_at = ?1 WHERE draft_id = ?2",
            params![bay_gio() - TTL_SECS as i64 - 1, d.draft_id],
        )
        .unwrap();
        assert!(matches!(
            take(&conn, &crypto, &d.draft_id).unwrap(),
            TakeResult::Expired
        ));
    }

    #[test]
    fn cancel_bo_ban_nhap_va_bao_dung_su_that() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let d = moi(&conn, &crypto);
        assert!(cancel(&conn, &d.draft_id).unwrap());
        assert!(!cancel(&conn, &d.draft_id).unwrap());
    }

    #[test]
    fn pending_xep_moi_nhat_truoc_ke_ca_trong_cung_mot_giay() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let ids: Vec<String> = (0..MAX_PENDING)
            .map(|i| {
                stage(
                    &conn,
                    &crypto,
                    Platform::Telegram,
                    "Nam",
                    "1",
                    &format!("tin {i}"),
                )
                .unwrap()
                .draft_id
            })
            .collect();
        let actual: Vec<String> = pending(&conn, &crypto)
            .unwrap()
            .into_iter()
            .map(|draft| draft.draft_id)
            .collect();
        let expected: Vec<String> = ids.into_iter().rev().collect();
        assert_eq!(actual, expected);
    }

    #[test]
    fn hop_khong_phinh_qua_tran_va_giu_lo_moi_nhat() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let latest: Vec<String> = (0..(MAX_PENDING * 2))
            .map(|i| {
                stage(
                    &conn,
                    &crypto,
                    Platform::Telegram,
                    "Nam",
                    "1",
                    &format!("tin {i}"),
                )
                .unwrap()
                .draft_id
            })
            .skip(MAX_PENDING)
            .collect();
        let actual: Vec<String> = pending(&conn, &crypto)
            .unwrap()
            .into_iter()
            .map(|draft| draft.draft_id)
            .collect();
        assert_eq!(actual.len(), MAX_PENDING);
        assert!(latest.iter().all(|id| actual.contains(id)));
    }

    #[test]
    fn sai_khoa_khong_lam_lo_ciphertext_va_khong_tieu_hang() {
        let (pool, crypto) = fixture();
        let conn = pool.writer.get().unwrap();
        let d = moi(&conn, &crypto);
        let wrong = EncryptionEngine::new("wrong-outbox-key-32-bytes-long");
        assert!(matches!(
            take(&conn, &wrong, &d.draft_id).unwrap(),
            TakeResult::Locked
        ));
        assert!(matches!(
            take(&conn, &crypto, &d.draft_id).unwrap(),
            TakeResult::Taken(_)
        ));
    }
}
