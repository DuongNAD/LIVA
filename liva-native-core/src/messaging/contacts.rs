//! Sổ danh bạ: tên người → địa chỉ đích trên một nền nhắn tin.
//!
//! Vì sao cần một bảng riêng thay vì để LLM tự nhớ: `telegram:send_text` đòi
//! `chatId` dạng SỐ, còn người nói thì nói "nhắn cho Minh Hiến". Không có lớp
//! ánh xạ này thì không có đường nào từ câu nói tới lệnh gửi — đó chính là lý do
//! LIVA trả lời vòng vo khi được bảo nhắn tin, chứ không phải model hiểu sai.
//!
//! ## Khoá tra cứu
//!
//! `lookup_key` = [`crate::wake::normalize_for_match`] của `display_name`: bỏ
//! dấu, thường hoá, gộp khoảng trắng. Dùng lại đúng hàm của cổng wake-word chứ
//! không viết bản thứ hai — hai bảng chữ cái tiếng Việt lệch nhau một ký tự là
//! đủ để "nhắn cho Hiền" tìm ra người, còn "nhắn cho Hiến" thì không.
//!
//! Điều đó cũng có nghĩa **dấu không phân biệt được người**: "Hiền" và "Hiến"
//! cùng ra `hien`. Đây là đánh đổi có chủ ý — STT đọc tên riêng tiếng Việt sai
//! dấu thường xuyên hơn nhiều so với việc hai người trong danh bạ trùng tên
//! không dấu. Trường hợp trùng thật thì [`resolve`] trả [`Resolution::Ambiguous`]
//! và người dùng chọn, chứ không đoán bừa.

use rusqlite::Connection;
use serde::Serialize;

/// Nền nhắn tin. Không dùng chuỗi trần vì hai nền có ràng buộc `handle` khác
/// hẳn nhau, và gõ nhầm `"telegam"` phải là lỗi biên dịch chứ không phải một
/// hàng danh bạ không bao giờ tìm thấy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Platform {
    Telegram,
    Messenger,
}

impl Platform {
    pub fn as_str(self) -> &'static str {
        match self {
            Platform::Telegram => "telegram",
            Platform::Messenger => "messenger",
        }
    }

    pub fn parse(s: &str) -> Result<Self, String> {
        match s.trim().to_lowercase().as_str() {
            "telegram" => Ok(Platform::Telegram),
            "messenger" | "facebook" | "fb" | "mess" => Ok(Platform::Messenger),
            other => Err(format!(
                "Nền nhắn tin không nhận ra: '{other}'. Chỉ có 'telegram' hoặc 'messenger'."
            )),
        }
    }

    /// `handle` có hợp lệ cho nền này không — kiểm TẠI ĐIỂM GHI, không đợi tới
    /// lúc gửi. Một chat id Telegram sai kiểu mà lọt vào danh bạ thì lỗi chỉ nổ
    /// lúc gửi, tức lúc người dùng đã bấm xác nhận và đang tin là tin đã đi.
    pub fn validate_handle(self, handle: &str) -> Result<(), String> {
        let handle = handle.trim();
        if handle.is_empty() {
            return Err("handle rỗng".to_string());
        }
        match self {
            // Chat id có thể âm (group/supergroup), nên chỉ đòi parse được i64.
            Platform::Telegram => handle.parse::<i64>().map(|_| ()).map_err(|_| {
                format!(
                    "Telegram cần chat id dạng số (có thể âm với group), nhận được '{handle}'. \
                     Đây là con số Telegram cấp, không phải @username."
                )
            }),
            // Messenger: thread id dạng số, hoặc username/vanity trong URL.
            // Không đòi chặt hơn được vì cả hai đều hợp lệ trong URL hội thoại.
            Platform::Messenger => {
                if handle.contains(char::is_whitespace) {
                    Err(format!(
                        "handle Messenger không được chứa khoảng trắng: '{handle}'. \
                         Dùng phần cuối URL hội thoại (t.me/… kiểu id số hoặc username)."
                    ))
                } else {
                    Ok(())
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Contact {
    pub contact_id: String,
    pub display_name: String,
    pub lookup_key: String,
    pub platform: String,
    pub handle: String,
    pub note: String,
    pub created_at: i64,
    pub updated_at: i64,
}

/// Kết quả tra một cái tên. Ba nhánh, và **`Ambiguous` không được tự chọn hộ** —
/// gửi nhầm người là hành động không hoàn tác được.
#[derive(Debug, Clone)]
pub enum Resolution {
    Found(Box<Contact>),
    Ambiguous(Vec<Contact>),
    NotFound,
}

fn bay_gio() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs() as i64
}

/// Khoá tra cứu cho một tên hiển thị.
pub fn lookup_key(display_name: &str) -> String {
    crate::wake::normalize_for_match(display_name)
}

const SELECT_COLS: &str = "contact_id, display_name, lookup_key, platform, handle, note, \
                           created_at, updated_at";

fn row_to_contact(row: &rusqlite::Row<'_>) -> rusqlite::Result<Contact> {
    Ok(Contact {
        contact_id: row.get(0)?,
        display_name: row.get(1)?,
        lookup_key: row.get(2)?,
        platform: row.get(3)?,
        handle: row.get(4)?,
        note: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
    })
}

/// Thêm mới hoặc cập nhật theo `(lookup_key, platform)`.
///
/// Ghi đè `handle` khi tên+nền đã tồn tại là CÓ CHỦ Ý: người dùng sửa số chat id
/// gõ nhầm phải là một thao tác, không phải "xoá rồi thêm lại".
pub fn upsert(
    conn: &Connection,
    display_name: &str,
    platform: Platform,
    handle: &str,
    note: &str,
) -> Result<Contact, String> {
    let display_name = display_name.trim();
    if display_name.is_empty() {
        return Err("Thiếu tên hiển thị".to_string());
    }
    let key = lookup_key(display_name);
    if key.is_empty() {
        return Err(format!(
            "Tên '{display_name}' không còn ký tự nào sau khi chuẩn hoá, không tra cứu được"
        ));
    }
    let handle = handle.trim();
    platform.validate_handle(handle)?;

    let now = bay_gio();
    let contact_id = format!("ct_{}", rand::random::<u64>());

    conn.execute(
        "INSERT INTO contacts (contact_id, display_name, lookup_key, platform, handle, note, \
         created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8) \
         ON CONFLICT(lookup_key, platform) DO UPDATE SET \
             display_name = excluded.display_name, \
             handle       = excluded.handle, \
             note         = excluded.note, \
             updated_at   = excluded.updated_at",
        rusqlite::params![
            contact_id,
            display_name,
            key,
            platform.as_str(),
            handle,
            note,
            now,
            now
        ],
    )
    .map_err(|e| format!("Không ghi được danh bạ: {e}"))?;

    // Đọc lại thay vì dựng từ tham số: khi nhánh UPDATE chạy thì `contact_id`
    // thật là của hàng cũ, không phải cái vừa sinh ở trên.
    match get_by_key(conn, &key, platform)? {
        Some(c) => Ok(c),
        None => Err("Ghi xong nhưng đọc lại không thấy — DB không nhất quán".to_string()),
    }
}

fn get_by_key(
    conn: &Connection,
    key: &str,
    platform: Platform,
) -> Result<Option<Contact>, String> {
    let sql = format!("SELECT {SELECT_COLS} FROM contacts WHERE lookup_key = ?1 AND platform = ?2");
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("Không chuẩn bị được truy vấn danh bạ: {e}"))?;
    let mut rows = stmt
        .query_map(rusqlite::params![key, platform.as_str()], row_to_contact)
        .map_err(|e| format!("Không đọc được danh bạ: {e}"))?;
    match rows.next() {
        Some(r) => r
            .map(Some)
            .map_err(|e| format!("Không đọc được hàng danh bạ: {e}")),
        None => Ok(None),
    }
}

/// Hai nhánh viết thẳng thay vì gộp qua một closure nhận `MappedRows`: kiểu của
/// nó mang lifetime bậc cao, và ép về con trỏ hàm để dùng chung làm trình biên
/// dịch từ chối ("implementation of `FnMut` is not general enough"). Lặp bốn
/// dòng rẻ hơn nhiều so với một trò khéo mà lần sau không ai sửa nổi.
pub fn list(conn: &Connection, platform: Option<Platform>) -> Result<Vec<Contact>, String> {
    let doc_loi = |e: rusqlite::Error| format!("Không đọc được danh bạ: {e}");
    let chuan_bi_loi = |e: rusqlite::Error| format!("Không chuẩn bị được truy vấn danh bạ: {e}");

    match platform {
        Some(p) => {
            let sql = format!(
                "SELECT {SELECT_COLS} FROM contacts WHERE platform = ?1 ORDER BY display_name"
            );
            let mut stmt = conn.prepare(&sql).map_err(chuan_bi_loi)?;
            let rows = stmt
                .query_map(rusqlite::params![p.as_str()], row_to_contact)
                .map_err(doc_loi)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(doc_loi)
        }
        None => {
            let sql = format!("SELECT {SELECT_COLS} FROM contacts ORDER BY display_name");
            let mut stmt = conn.prepare(&sql).map_err(chuan_bi_loi)?;
            let rows = stmt.query_map([], row_to_contact).map_err(doc_loi)?;
            rows.collect::<rusqlite::Result<Vec<_>>>().map_err(doc_loi)
        }
    }
}

pub fn delete(conn: &Connection, contact_id: &str) -> Result<bool, String> {
    let n = conn
        .execute(
            "DELETE FROM contacts WHERE contact_id = ?1",
            rusqlite::params![contact_id],
        )
        .map_err(|e| format!("Không xoá được danh bạ: {e}"))?;
    Ok(n > 0)
}

/// Tra một cái tên người dùng vừa nói ra.
///
/// Hai vòng, cố ý theo thứ tự này:
/// 1. Khớp **đúng** `lookup_key` — "minh hien" ra đúng Minh Hiến.
/// 2. Khớp **một phần theo ranh giới từ** — "hien" ra "minh hien". Dùng
///    `LIKE '% hien %'` trên chuỗi đã bọc khoảng trắng, KHÔNG phải `%hien%`
///    trần: bản trần khiến "an" khớp cả "tuấn" lẫn "ngân", tức mọi câu đều
///    thành nhập nhằng và tính năng thành vô dụng.
///
/// Vòng 2 chỉ chạy khi vòng 1 rỗng. Nếu vòng 2 ra nhiều người thì trả
/// `Ambiguous` — **không** tự chọn người đầu tiên.
pub fn resolve(
    conn: &Connection,
    name_query: &str,
    platform: Option<Platform>,
) -> Result<Resolution, String> {
    let key = lookup_key(name_query);
    if key.is_empty() {
        return Ok(Resolution::NotFound);
    }

    let mut exact: Vec<Contact> = list(conn, platform)?
        .into_iter()
        .filter(|c| c.lookup_key == key)
        .collect();
    if exact.len() == 1 {
        return Ok(Resolution::Found(Box::new(exact.remove(0))));
    }
    if exact.len() > 1 {
        return Ok(Resolution::Ambiguous(exact));
    }

    let needle = format!(" {key} ");
    let mut partial: Vec<Contact> = list(conn, platform)?
        .into_iter()
        .filter(|c| format!(" {} ", c.lookup_key).contains(&needle))
        .collect();
    match partial.len() {
        0 => Ok(Resolution::NotFound),
        1 => Ok(Resolution::Found(Box::new(partial.remove(0)))),
        _ => Ok(Resolution::Ambiguous(partial)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn db() -> Connection {
        let conn = Connection::open_in_memory().expect("mo db trong bo nho");
        conn.execute_batch(
            "CREATE TABLE contacts (
                 contact_id  TEXT PRIMARY KEY,
                 display_name TEXT NOT NULL,
                 lookup_key  TEXT NOT NULL,
                 platform    TEXT NOT NULL,
                 handle      TEXT NOT NULL,
                 note        TEXT NOT NULL DEFAULT '',
                 created_at  INTEGER NOT NULL,
                 updated_at  INTEGER NOT NULL
             );
             CREATE UNIQUE INDEX idx_contacts_lookup ON contacts(lookup_key, platform);",
        )
        .expect("dung schema");
        conn
    }

    #[test]
    fn bo_dau_nen_stt_sai_dau_van_tim_ra_nguoi() {
        let conn = db();
        upsert(&conn, "Minh Hiến", Platform::Telegram, "12345", "").unwrap();

        for cach_goi in ["Minh Hiến", "minh hien", "MINH HIEN", "Minh Hiền"] {
            match resolve(&conn, cach_goi, None).unwrap() {
                Resolution::Found(c) => assert_eq!(c.display_name, "Minh Hiến"),
                other => panic!("'{cach_goi}' phai tim ra nguoi, nhan duoc {other:?}"),
            }
        }
    }

    #[test]
    fn khop_mot_phan_theo_ranh_gioi_tu_khong_phai_chuoi_con() {
        let conn = db();
        upsert(&conn, "Minh Hiến", Platform::Telegram, "1", "").unwrap();
        upsert(&conn, "Tuấn Anh", Platform::Telegram, "2", "").unwrap();

        // "hien" là một TỪ trong "minh hien" → tìm ra.
        assert!(matches!(
            resolve(&conn, "Hiến", None).unwrap(),
            Resolution::Found(_)
        ));
        // "an" là chuỗi con của "tuan anh" nhưng KHÔNG phải một từ trọn vẹn ở
        // "tuan"; chỉ được khớp qua từ "anh"? Không — " an " không nằm trong
        // " tuan anh ", nên phải là NotFound. Đây là chỗ bản `%an%` trần sai.
        assert!(matches!(
            resolve(&conn, "an", None).unwrap(),
            Resolution::NotFound
        ));
    }

    #[test]
    fn trung_ten_tra_ambiguous_chu_khong_doan_bua() {
        let conn = db();
        upsert(&conn, "Hiến", Platform::Telegram, "1", "ban cap 3").unwrap();
        upsert(&conn, "Hiến", Platform::Messenger, "hien.nguyen", "dong nghiep").unwrap();

        match resolve(&conn, "Hiến", None).unwrap() {
            Resolution::Ambiguous(v) => assert_eq!(v.len(), 2),
            other => panic!("phai la Ambiguous, nhan duoc {other:?}"),
        }
        // Nêu rõ nền thì hết nhập nhằng.
        assert!(matches!(
            resolve(&conn, "Hiến", Some(Platform::Telegram)).unwrap(),
            Resolution::Found(_)
        ));
    }

    #[test]
    fn upsert_ghi_de_giu_nguyen_contact_id() {
        let conn = db();
        let a = upsert(&conn, "Nam", Platform::Telegram, "111", "").unwrap();
        let b = upsert(&conn, "Nam", Platform::Telegram, "222", "sua so").unwrap();
        assert_eq!(a.contact_id, b.contact_id, "sua so khong duoc tao nguoi moi");
        assert_eq!(b.handle, "222");
        assert_eq!(list(&conn, None).unwrap().len(), 1);
    }

    #[test]
    fn handle_sai_kieu_bi_chan_ngay_luc_ghi() {
        let conn = db();
        // Telegram đòi chat id dạng số — `@username` là lỗi thường gặp nhất.
        let err = upsert(&conn, "Nam", Platform::Telegram, "@namdev", "").unwrap_err();
        assert!(err.contains("chat id"), "loi phai noi ro can gi: {err}");
        // Chat id âm (group) là hợp lệ.
        assert!(upsert(&conn, "Nhom lop", Platform::Telegram, "-1001234567890", "").is_ok());
        // Messenger không nhận khoảng trắng.
        assert!(upsert(&conn, "Ai do", Platform::Messenger, "co khoang trang", "").is_err());
    }

    #[test]
    fn xoa_tra_ve_co_xoa_duoc_hay_khong() {
        let conn = db();
        let c = upsert(&conn, "Nam", Platform::Telegram, "1", "").unwrap();
        assert!(delete(&conn, &c.contact_id).unwrap());
        assert!(!delete(&conn, &c.contact_id).unwrap(), "xoa lan hai phai la false");
    }
}
