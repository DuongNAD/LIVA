//! Cổng đồng ý cho quan sát thụ động (U20, bước 1).
//!
//! ## Vì sao module này ra đời TRƯỚC phần thu thập
//!
//! Nghiệm thu U20 (`docs/03-danh-gia/05-nang-cap-toan-dien.md`) đặt một thứ tự
//! **bắt buộc**: *"Cổng đồng ý và công tắc tắt phải tồn tại và hoạt động trước
//! khi viết dòng code thu thập đầu tiên."* Đây là module đó. **Chưa có một dòng
//! thu thập nào** trong repo, và cổng này tồn tại để khi có, nó không thể chạy
//! nếu người dùng chưa bật.
//!
//! Bối cảnh: "bộ nhớ thị giác" là ý tưởng đã bị công chúng ném đá khi một hãng
//! lớn làm — lý do là dữ liệu rời khỏi máy. LIVA offline trả lời được lời chê
//! đó, nhưng chỉ khi quyền kiểm soát nằm trong tay người dùng một cách **kiểm
//! chứng được**, không phải một lời hứa trong tài liệu.
//!
//! ## Hai nguyên tắc, cả hai đều có test khoá
//!
//! 1. **Fail-closed.** Thiếu file, file hỏng, JSON sai khuôn, thiếu trường —
//!    MỌI trường hợp không chắc chắn đều quy về **CHƯA đồng ý**. Cùng tinh thần
//!    với allow-list Telegram (`telegram.rs`): khi không rõ, từ chối. Một cổng
//!    riêng tư "mở khi nghi ngờ" thì không phải là cổng.
//! 2. **Thu hồi tức thì.** Nguồn sự thật là file trên đĩa, đọc lại mỗi lần hỏi.
//!    Không cache ẩn ⇒ người dùng bấm "tắt" là có hiệu lực ngay, không chờ khởi
//!    động lại. Khi có collector chạy ở hot path và cần cache, việc **vô hiệu
//!    cache khi thu hồi** là hợp đồng bắt buộc của nó — ghi lại ở đây để không ai
//!    quên.
//!
//! ## Cố ý KHÔNG dùng lại `passive/hook.rs`
//!
//! `passive/` là keylogger toàn hệ thống (WH_KEYBOARD_LL), nằm sau
//! `#[cfg(feature = "experimental")]` và **không được bật lại** — anti-cheat coi
//! hook bàn phím là gian lận và ban phần cứng. Cổng này áp cho collector TƯƠNG
//! LAI qua OS Accessibility / UIAutomation (đọc tên cửa sổ / tiến trình / cấu
//! trúc text UI, **không** chặn phím). Hai thứ độc lập hoàn toàn.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const CONSENT_REL_PATH: &str = "data/consent.json";

/// Trạng thái đồng ý, đúng như ghi trên đĩa.
///
/// `updated_at_unix` giữ thời điểm QUYẾT ĐỊNH gần nhất (dù bật hay tắt) để về
/// sau audit được và để có thể yêu cầu tái xác nhận sau một khoảng thời gian.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ObservationConsent {
    /// Người dùng đã cho phép quan sát thụ động chưa.
    ///
    /// `#[derive(Default)]` cho `bool` là `false` — và đó chính là **điểm
    /// fail-closed gốc**: kể cả khi file hoàn toàn vắng mặt, `load_from` trả
    /// `Default` nên hệ thống bắt đầu ở trạng thái KHÔNG được ghi. Nếu có ngày
    /// đổi trường này sang một kiểu mà `Default` không phải "chưa đồng ý", phải
    /// viết `impl Default` tay lại cho đúng — mặc định an toàn là bất biến của
    /// module này.
    pub granted: bool,
    /// Epoch giây của lần đổi trạng thái gần nhất; `None` khi chưa từng quyết định.
    #[serde(default)]
    pub updated_at_unix: Option<u64>,
}

impl ObservationConsent {
    /// Collector TƯƠNG LAI phải hỏi đúng hàm này trước mỗi lần ghi.
    ///
    /// Đặt tên dài, tường minh có chủ đích: một `if consent.granted` trần dễ bị
    /// đọc lướt qua, còn `is_capture_allowed()` buộc người đọc thấy rõ đây là
    /// một cổng quyền riêng tư.
    pub fn is_capture_allowed(&self) -> bool {
        self.granted
    }
}

/// Đường dẫn `data/consent.json`, dò lên tối đa hai cấp.
///
/// cwd khác nhau tuỳ điểm vào (gốc repo, `liva-native-core`, hay
/// `liva-desktop/src-tauri`) — cùng lý do và cùng cách với `config_file_path`.
/// Khi chưa có file ở đâu cả thì trả đường dẫn gốc-repo để `save` tạo mới.
pub fn consent_file_path() -> PathBuf {
    for prefix in ["", "..", "../.."] {
        let candidate = Path::new(prefix).join(CONSENT_REL_PATH);
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(CONSENT_REL_PATH)
}

fn now_unix() -> Option<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
}

/// Đọc trạng thái từ một file cụ thể. **Fail-closed**: mọi lỗi → `Default` (chưa
/// đồng ý). Tách khỏi [`load`] để test được mà không đụng đường dẫn thật.
pub fn load_from(path: &Path) -> ObservationConsent {
    let Ok(text) = std::fs::read_to_string(path) else {
        return ObservationConsent::default();
    };
    // JSON hỏng, thiếu trường, sai kiểu — đều quy về chưa đồng ý. KHÔNG được
    // "granted một phần" hay lấy giá trị đoán được: nghi ngờ thì đóng.
    serde_json::from_str(&text).unwrap_or_default()
}

/// Ghi trạng thái ra một file cụ thể, tạo thư mục cha nếu cần.
pub fn save_to(path: &Path, state: &ObservationConsent) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("không tạo được thư mục {}: {e}", parent.display()))?;
    }
    let text = serde_json::to_string_pretty(state)
        .map_err(|e| format!("không tuần tự hoá được trạng thái đồng ý: {e}"))?;
    std::fs::write(path, text).map_err(|e| format!("không ghi được {}: {e}", path.display()))
}

/// Đọc trạng thái đồng ý hiện tại (fail-closed).
pub fn load() -> ObservationConsent {
    load_from(&consent_file_path())
}

/// Bật quan sát thụ động và ghi bền vững. Trả trạng thái mới.
pub fn grant() -> Result<ObservationConsent, String> {
    let state = ObservationConsent {
        granted: true,
        updated_at_unix: now_unix(),
    };
    save_to(&consent_file_path(), &state)?;
    tracing::info!("[Consent] quan sát thụ động ĐƯỢC BẬT bởi người dùng");
    Ok(state)
}

/// Thu hồi đồng ý và ghi bền vững. Trả trạng thái mới.
///
/// Giữ `updated_at_unix` = thời điểm thu hồi (không xoá về `None`): một quyết
/// định "tắt" cũng là một quyết định cần audit được.
pub fn revoke() -> Result<ObservationConsent, String> {
    let state = ObservationConsent {
        granted: false,
        updated_at_unix: now_unix(),
    };
    save_to(&consent_file_path(), &state)?;
    tracing::info!("[Consent] quan sát thụ động BỊ TẮT bởi người dùng");
    Ok(state)
}

/// Collector có đang THẬT SỰ chạy không.
///
/// **Luôn `false`** ở thời điểm này: chưa có một dòng code thu thập nào trong
/// repo (đúng thứ tự bắt buộc của U20). Hàm tồn tại để giao diện phân biệt được
/// hai khái niệm khác nhau — "đã cho phép" (`is_capture_allowed`) và "đang ghi"
/// — ngay từ bây giờ, để khi collector ra đời, chỉ báo "đang ghi" đã có chỗ nối
/// và không ai phải nhớ thêm nó vào sau.
pub fn is_capture_active() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp(ten: &str) -> PathBuf {
        std::env::temp_dir().join(format!("liva_consent_{ten}.json"))
    }

    #[test]
    fn mac_dinh_la_chua_dong_y() {
        // Điểm fail-closed gốc: không có gì được cấu hình = không được ghi.
        assert!(!ObservationConsent::default().is_capture_allowed());
    }

    #[test]
    fn thieu_file_thi_fail_closed() {
        let p = tmp("khong_ton_tai");
        let _ = std::fs::remove_file(&p);
        assert!(
            !load_from(&p).is_capture_allowed(),
            "thiếu file phải quy về CHƯA đồng ý, không được mở cổng"
        );
    }

    #[test]
    fn json_hong_thi_fail_closed() {
        let p = tmp("json_hong");
        std::fs::write(&p, "{ granted: đây không phải json").unwrap();
        assert!(
            !load_from(&p).is_capture_allowed(),
            "JSON hỏng phải quy về CHƯA đồng ý"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn json_thieu_truong_updated_van_doc_duoc() {
        // Chỉ có `granted`, thiếu `updated_at_unix` — không được coi là hỏng.
        let p = tmp("thieu_truong");
        std::fs::write(&p, r#"{"granted":true}"#).unwrap();
        let s = load_from(&p);
        assert!(s.is_capture_allowed());
        assert_eq!(s.updated_at_unix, None);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn ghi_roi_doc_lai_giu_nguyen_trang_thai() {
        let p = tmp("roundtrip");
        let goc = ObservationConsent {
            granted: true,
            updated_at_unix: Some(1_700_000_000),
        };
        save_to(&p, &goc).unwrap();
        assert_eq!(load_from(&p), goc);
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn thu_hoi_ghi_de_thanh_chua_dong_y() {
        // Bật rồi tắt: file phải phản ánh trạng thái TẮT ngay, không giữ giá trị cũ.
        let p = tmp("thu_hoi");
        save_to(
            &p,
            &ObservationConsent {
                granted: true,
                updated_at_unix: Some(1),
            },
        )
        .unwrap();
        assert!(load_from(&p).is_capture_allowed());

        save_to(
            &p,
            &ObservationConsent {
                granted: false,
                updated_at_unix: Some(2),
            },
        )
        .unwrap();
        assert!(
            !load_from(&p).is_capture_allowed(),
            "thu hồi phải có hiệu lực ngay ở lần đọc kế tiếp"
        );
        let _ = std::fs::remove_file(&p);
    }

    #[test]
    fn chua_co_collector_nen_khong_bao_gio_dang_ghi() {
        // Trung thực: đã cho phép KHÁC với đang ghi. Chưa có collector thì
        // "đang ghi" phải là false, kể cả khi đã bật cổng.
        assert!(!is_capture_active());
    }
}
