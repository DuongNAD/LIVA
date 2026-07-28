//! Hộp chờ xác nhận: chặng bắt buộc giữa "LIVA hiểu ý" và "tin nhắn rời khỏi máy".
//!
//! ## Vì sao tồn tại
//!
//! `docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md` ghi "bước xác nhận cho hành
//! động vật lý vẫn CHƯA có". Gửi tin nhắn là **không hoàn tác được** — mạnh hơn
//! bật đèn, vì cái sai không nằm ở máy mà nằm ở người khác đã đọc nó.
//!
//! Và cái sai ở đây không hiếm: bộ định tuyến chạy trên model 2B, đầu vào
//! thường là STT tiếng Việt. Nghe "Hiến" thành "Hiền", nghe "bảo nó ngủ đi"
//! thành "bảo nó ngu đi" — cả hai đều là câu hợp lệ, không có tín hiệu nào để
//! máy tự biết mình sai. Người đọc lại một dòng chữ thì biết ngay.
//!
//! Nên module này giữ đúng một bất biến, và mọi thứ khác chỉ là hệ quả:
//!
//! > **Không có đường nào gửi tin mà không đi qua một [`take`] thành công.**
//!
//! Module này KHÔNG tự gửi gì. Nó chỉ giữ chữ. Người gửi là
//! `messaging::send` — và nó chỉ nhận được [`Draft`] từ [`take`].
//!
//! ## Ba tính chất được test khoá lại
//!
//! 1. **Dùng một lần.** [`take`] lấy bản nháp ra khỏi hộp. Bấm xác nhận hai lần
//!    (hoặc UI gửi trùng gói) thì lần thứ hai không có gì để gửi.
//! 2. **Hết hạn.** Bản nháp quá [`TTL_SECS`] không lấy được nữa. Bấm xác nhận
//!    cho một câu nói từ hai mươi phút trước là gần như chắc chắn nhầm ngữ cảnh.
//! 3. **Có trần.** Hộp không giữ quá [`MAX_PENDING`] bản nháp; vượt thì bản cũ
//!    nhất rơi ra. Một vòng lặp lỗi gọi `stage` liên tục không được phép ăn hết
//!    RAM một tiến trình chạy nền cả ngày.

use serde::Serialize;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};

use super::contacts::Platform;

/// Bản nháp sống bao lâu trước khi phải nói lại từ đầu.
pub const TTL_SECS: u64 = 300;

/// Số bản nháp tối đa giữ cùng lúc.
pub const MAX_PENDING: usize = 32;

#[derive(Debug, Clone, Serialize)]
pub struct Draft {
    pub draft_id: String,
    pub platform: Platform,
    /// Tên hiển thị của người nhận — thứ người dùng ĐỌC trên thẻ xác nhận.
    pub display_name: String,
    /// Địa chỉ đích thật — thứ máy DÙNG. Hiện cả hai trên thẻ là có chủ ý: tên
    /// đúng mà số sai vẫn là gửi nhầm người.
    pub handle: String,
    pub text: String,
    pub created_at: u64,
    /// Số thứ tự tăng đơn điệu trong tiến trình — thứ tự tạo, dùng ở CẢ hai chỗ
    /// cần biết cái nào cũ/mới hơn: chọn bản bị đuổi trong [`stage`] và xếp
    /// "mới nhất trước" trong [`pending`].
    ///
    /// Vì sao cần: `created_at` tính bằng **giây**, nên mọi bản nháp tạo trong
    /// cùng một giây có `created_at` bằng nhau. Khi hộp đầy, `min_by_key` khi đó
    /// rơi về so `draft_id` — vốn NGẪU NHIÊN — nên một bản nháp **vừa tạo xong**
    /// có thể bị đuổi trước những bản thật sự cũ hơn. Với `seq`, thứ tự "cũ
    /// nhất" là toàn phần và đúng nghĩa.
    ///
    /// `#[serde(skip)]`: đây là chi tiết nội bộ, không thuộc hợp đồng JSON với
    /// client — thêm nó không đổi hình dạng dữ liệu bên ngoài.
    #[serde(skip)]
    seq: u64,
}

impl Draft {
    fn het_han(&self, bay_gio: u64) -> bool {
        bay_gio.saturating_sub(self.created_at) >= TTL_SECS
    }
}

fn bay_gio() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn hop() -> &'static Mutex<HashMap<String, Draft>> {
    static OUTBOX: OnceLock<Mutex<HashMap<String, Draft>>> = OnceLock::new();
    OUTBOX.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Khoá hộp. `Mutex` bị poison nghĩa là một luồng đã panic khi đang giữ khoá;
/// với một hộp chứa toàn dữ liệu bất biến thì nội dung vẫn đọc được, và từ chối
/// phục vụ ở đây chỉ làm hỏng thêm — nên lấy lại ruột và đi tiếp.
fn khoa() -> std::sync::MutexGuard<'static, HashMap<String, Draft>> {
    hop().lock().unwrap_or_else(|e| e.into_inner())
}

fn don_het_han(map: &mut HashMap<String, Draft>, bay_gio: u64) {
    map.retain(|_, d| !d.het_han(bay_gio));
}

/// Đặt một bản nháp vào hộp, trả về `draft_id` để UI đính vào nút xác nhận.
pub fn stage(platform: Platform, display_name: &str, handle: &str, text: &str) -> Draft {
    let now = bay_gio();
    let draft = Draft {
        draft_id: format!("dr_{}", rand::random::<u64>()),
        platform,
        display_name: display_name.to_string(),
        handle: handle.to_string(),
        text: text.to_string(),
        created_at: now,
        seq: {
            static SEQ: AtomicU64 = AtomicU64::new(0);
            SEQ.fetch_add(1, Ordering::Relaxed)
        },
    };

    let mut map = khoa();
    don_het_han(&mut map, now);

    // Trần: bỏ bản cũ nhất. `>=` vì ta sắp chèn thêm một cái nữa.
    //
    // Xếp theo `(created_at, seq)`, KHÔNG theo `draft_id`. Bản trước dùng
    // `draft_id` làm tie-break, mà id là ngẫu nhiên và `created_at` chỉ có độ
    // phân giải GIÂY — nên khi hộp đầy, bản nháp vừa tạo xong có thể bị đuổi
    // trước những bản thật sự cũ hơn. `seq` làm thứ tự trở thành toàn phần.
    while map.len() >= MAX_PENDING {
        let cu_nhat = map
            .values()
            .min_by_key(|d| (d.created_at, d.seq))
            .map(|d| d.draft_id.clone());
        match cu_nhat {
            Some(id) => {
                map.remove(&id);
            }
            None => break,
        }
    }

    map.insert(draft.draft_id.clone(), draft.clone());
    draft
}

/// Xem một bản nháp mà KHÔNG tiêu nó. Dùng để vẽ lại thẻ xác nhận.
pub fn peek(draft_id: &str) -> Option<Draft> {
    let now = bay_gio();
    let mut map = khoa();
    don_het_han(&mut map, now);
    map.get(draft_id).cloned()
}

/// Lấy bản nháp ra để gửi. **Đây là cửa duy nhất** — sau lời gọi này bản nháp
/// không còn trong hộp, nên không thể gửi lần hai.
pub fn take(draft_id: &str) -> Option<Draft> {
    let now = bay_gio();
    let mut map = khoa();
    don_het_han(&mut map, now);
    map.remove(draft_id)
}

/// Bỏ một bản nháp mà không gửi. `true` nếu nó còn ở đó để mà bỏ.
pub fn cancel(draft_id: &str) -> bool {
    let now = bay_gio();
    let mut map = khoa();
    don_het_han(&mut map, now);
    map.remove(draft_id).is_some()
}

/// Các bản nháp còn chờ, mới nhất trước.
///
/// Tie-break bằng `seq` chứ KHÔNG bằng `draft_id`, cùng lý do đã ghi ở
/// [`Draft::seq`]: `created_at` chỉ có độ phân giải giây, nên trong cùng một
/// giây `draft_id` ngẫu nhiên sẽ xáo thứ tự và biến "mới nhất trước" thành một
/// lời hứa sai. Danh sách này là thẻ xác nhận gửi tin — xếp sai ở đây là mời
/// người dùng bấm nhầm bản nháp.
pub fn pending() -> Vec<Draft> {
    let now = bay_gio();
    let mut map = khoa();
    don_het_han(&mut map, now);
    let mut v: Vec<Draft> = map.values().cloned().collect();
    v.sort_by(|a, b| b.created_at.cmp(&a.created_at).then(b.seq.cmp(&a.seq)));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Hộp là toàn cục còn `cargo test` chạy song song trong CÙNG tiến trình,
    /// nên các test ở đây dùng chung một trạng thái thật.
    ///
    /// Bản đầu chỉ dặn "mỗi test chỉ khẳng định về id của chính nó" và coi thế
    /// là đủ. Không đủ: test trần `MAX_PENDING` vừa dọn sạch hộp vừa nạp 42 bản
    /// nháp, mà cơ chế trần thì **đuổi bản cũ nhất** — đúng lúc đó bản nháp của
    /// test khác biến mất và test kia đỏ. Nó đã đỏ thật, sau vài lần chạy xanh.
    ///
    /// Khoá này nối tiếp hoá chúng. Đây không phải "làm test bớt khó tính" — nó
    /// loại một nguồn nhiễu do chính bộ test tạo ra, để lần sau một test đỏ có
    /// nghĩa là mã hỏng.
    static KHOA_TEST: Mutex<()> = Mutex::new(());

    fn nam_khoa() -> std::sync::MutexGuard<'static, ()> {
        KHOA_TEST.lock().unwrap_or_else(|e| e.into_inner())
    }

    fn moi() -> Draft {
        stage(Platform::Telegram, "Minh Hiến", "12345", "ngủ đi")
    }

    #[test]
    fn stage_roi_take_ra_dung_noi_dung() {
        let _g = nam_khoa();
        let d = moi();
        let lay = take(&d.draft_id).expect("vua stage thi phai lay duoc");
        assert_eq!(lay.text, "ngủ đi");
        assert_eq!(lay.handle, "12345");
        assert_eq!(lay.display_name, "Minh Hiến");
    }

    /// Tính chất 1 — bấm xác nhận hai lần không gửi hai tin.
    #[test]
    fn take_lan_hai_tra_none() {
        let _g = nam_khoa();
        let d = moi();
        assert!(take(&d.draft_id).is_some());
        assert!(take(&d.draft_id).is_none(), "ban nhap phai la dung-mot-lan");
    }

    #[test]
    fn peek_khong_tieu_ban_nhap() {
        let _g = nam_khoa();
        let d = moi();
        assert!(peek(&d.draft_id).is_some());
        assert!(peek(&d.draft_id).is_some(), "peek khong duoc tieu");
        assert!(take(&d.draft_id).is_some(), "sau peek van phai take duoc");
    }

    /// Tính chất 2 — quá hạn thì không lấy được nữa.
    #[test]
    fn ban_nhap_het_han_khong_lay_duoc() {
        let _g = nam_khoa();
        let d = moi();
        // Đẩy `created_at` lùi về quá khứ thay vì ngủ TTL_SECS giây.
        {
            let mut map = khoa();
            if let Some(entry) = map.get_mut(&d.draft_id) {
                entry.created_at = bay_gio().saturating_sub(TTL_SECS + 1);
            }
        }
        assert!(take(&d.draft_id).is_none(), "qua han thi khong duoc gui");
        assert!(peek(&d.draft_id).is_none());
    }

    #[test]
    fn cancel_bo_ban_nhap_va_bao_dung_su_that() {
        let _g = nam_khoa();
        let d = moi();
        assert!(cancel(&d.draft_id));
        assert!(!cancel(&d.draft_id), "huy lan hai phai la false");
        assert!(take(&d.draft_id).is_none(), "da huy thi khong gui duoc");
    }

    #[test]
    fn pending_liet_ke_ban_nhap_cua_chinh_no() {
        let _g = nam_khoa();
        let a = moi();
        let b = moi();
        let ids: Vec<String> = pending().into_iter().map(|d| d.draft_id).collect();
        assert!(ids.contains(&a.draft_id));
        assert!(ids.contains(&b.draft_id));
        take(&a.draft_id);
        take(&b.draft_id);
    }

    /// HỒI QUY — `pending` phải xếp **mới nhất trước** THẬT, kể cả trong cùng
    /// một giây.
    ///
    /// Cùng lớp lỗi với [`ban_nhap_vua_tao_khong_bi_duoi_khi_hop_day`], chỉ khác
    /// chỗ: `stage` đã được vá bằng `seq`, còn `pending` thì chưa — nó vẫn
    /// tie-break bằng `draft_id`, thứ NGẪU NHIÊN. Vì `created_at` chỉ có độ phân
    /// giải GIÂY, mọi bản nháp tạo trong cùng một giây đều hoà ở khoá chính, nên
    /// thứ tự trả ra là ngẫu nhiên trong khi doc-comment hứa "mới nhất trước".
    ///
    /// Vì sao đáng vá chứ không phải chuyện thẩm mỹ: `message:pending` trả đúng
    /// danh sách này cho UI và cho LLM. Đây là **thẻ xác nhận gửi tin** — cả
    /// module tồn tại để chặn gửi nhầm người. Một danh sách tự nhận là mới-nhất-
    /// trước mà thật ra xếp ngẫu nhiên là đúng cách để người dùng bấm xác nhận
    /// nhầm bản nháp. "Nhắn cho Hiến, và nhắn cho Nam luôn" là đủ để dính.
    ///
    /// Tất định hoá giống test anh em ở trên: dùng `MAX_PENDING` bản nháp trong
    /// cùng một giây. Với tie-break ngẫu nhiên, xác suất cả loạt tình cờ ra đúng
    /// thứ tự nghịch đảo là 1/32! ≈ 0.
    #[test]
    fn pending_xep_moi_nhat_truoc_ke_ca_trong_cung_mot_giay() {
        let _g = nam_khoa();
        {
            let mut map = khoa();
            map.clear();
        }

        let theo_thu_tu_tao: Vec<String> = (0..MAX_PENDING)
            .map(|i| stage(Platform::Telegram, "Nam", "1", &format!("tin {i}")).draft_id)
            .collect();

        let tra_ve: Vec<String> = pending().into_iter().map(|d| d.draft_id).collect();
        let mong_doi: Vec<String> = theo_thu_tu_tao.iter().rev().cloned().collect();

        assert_eq!(
            tra_ve, mong_doi,
            "pending() phai la nghich dao thu tu stage — dang tie-break bang draft_id ngau nhien?"
        );
    }

    /// HỒI QUY — bản nháp VỪA TẠO không được bị đuổi khi hộp đầy.
    ///
    /// Đây là lỗi thật đã làm `pending_liet_ke_ban_nhap_cua_chinh_no` đỏ:
    /// `created_at` chỉ có độ phân giải GIÂY, nên mọi bản nháp tạo trong cùng
    /// một giây bằng nhau, và tie-break cũ dùng `draft_id` NGẪU NHIÊN. Hệ quả:
    /// `stage` có thể đuổi đúng bản mà người gọi vừa tạo — người dùng bấm gửi
    /// thì nhận "bản nháp không còn", mà không có gì trong log giải thích.
    ///
    /// ⚠️ Phải stage NHIỀU lần sau khi lấp đầy, không phải một lần. `stage` đuổi
    /// TRƯỚC khi chèn, nên bản vừa tạo không bao giờ là ứng viên bị đuổi bởi
    /// chính lời gọi tạo ra nó — một test chỉ stage một lần sẽ xanh kể cả khi
    /// lỗi còn nguyên (đã thử: nó xanh với tie-break cũ, tức vô dụng).
    ///
    /// Lỗi thật lộ ra ở lời gọi THỨ HAI: lúc đó bản của lời gọi thứ nhất đã nằm
    /// trong hộp và trở thành ứng viên. Dùng `MAX_PENDING` bản mới để phép kiểm
    /// tất định — với tie-break ngẫu nhiên, xác suất cả loạt sống sót là ~0.
    #[test]
    fn ban_nhap_vua_tao_khong_bi_duoi_khi_hop_day() {
        let _g = nam_khoa();
        {
            let mut map = khoa();
            map.clear();
        }
        for i in 0..MAX_PENDING {
            stage(Platform::Telegram, "Nam", "1", &format!("cu {i}"));
        }
        // Toàn bộ ở trên cùng một giây với loạt dưới đây — đúng điều kiện gây lỗi.
        let moi: Vec<String> = (0..MAX_PENDING)
            .map(|i| stage(Platform::Telegram, "Nam", "1", &format!("moi {i}")).draft_id)
            .collect();

        let mat: Vec<&String> = moi.iter().filter(|id| peek(id).is_none()).collect();
        assert!(
            mat.is_empty(),
            "{} ban nhap MOI bi duoi trong khi ban CU van con — tie-break dang dua vao id ngau nhien?",
            mat.len()
        );
    }

    /// Tính chất 3 — hộp có trần. Chạy trên hộp sạch: dọn sạch trước, và không
    /// test nào khác được xen vào giữa (các test khác chỉ khẳng định về id của
    /// riêng chúng, nên chúng chịu được việc bị test này dọn).
    #[test]
    fn hop_khong_phinh_qua_tran() {
        let _g = nam_khoa();
        {
            let mut map = khoa();
            map.clear();
        }
        for i in 0..(MAX_PENDING + 10) {
            stage(Platform::Telegram, "Nam", "1", &format!("tin {i}"));
        }
        let n = khoa().len();
        assert!(
            n <= MAX_PENDING,
            "hop giu {n} ban nhap, vuot tran {MAX_PENDING}"
        );
    }
}
