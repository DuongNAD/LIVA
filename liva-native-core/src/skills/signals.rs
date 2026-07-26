//! Sổ cái chất lượng → **prior trong xếp hạng** (rung G3).
//!
//! §2.1 của tài liệu 04 chỉ ra chỗ này: `skill_ranker.py` của OpenSpace **không**
//! có trọng số chất lượng — tín hiệu của họ chảy vào *tiến hoá*, không vào *truy
//! hồi*. Một skill hỏng đi hỏng lại vẫn được họ xếp hạng y như skill chạy tốt.
//! Đây là chỗ LIVA làm hơn với chi phí gần bằng không: không LLM, không mô hình,
//! chỉ một phép cộng trên thứ tự.
//!
//! ## Ba quyết định đắt nhất ở module này
//!
//! **1. Đếm `merge_key` phân biệt, KHÔNG đếm dòng.** Cột `merge_key` được định
//! nghĩa ở G2 là "hai tín hiệu cùng khoá là *cùng một vấn đề* quan sát nhiều lần".
//! Nên `COUNT(*)` thô — thứ [`super::SkillStore::signal_counts`] trả về — là con số
//! SAI cho mục đích này: một sự cố lặp 20 lần sẽ đọc thành 20 lỗi và dìm chết một
//! skill vốn chỉ có một vấn đề. [`SignalTally`] đếm khoá phân biệt.
//!
//! **2. Tín hiệu bị phản chứng KHÔNG trừ điểm.** `evidence_status = "refuted"`
//! nhân trọng số 0. Nếu không, một lời phàn nàn đã được chứng minh là sai vẫn làm
//! hỏng skill vĩnh viễn, và không có đường hồi phục nào ngoài xoá bản ghi — tức là
//! sổ cái trở thành thứ người ta phải đi dọn thay vì thứ đọc được.
//!
//! **3. Prior cộng trên THỨ HẠNG, không cộng trên điểm.** Điểm ở
//! [`super::rank_skills`] là cosine (dải hẹp 0,77–0,91 đo ở G1) HOẶC BM25 (dải
//! rộng 0…~10) tuỳ có embedder hay không. Một hằng số trừ vào điểm sẽ vô hình ở
//! thang này và áp đảo ở thang kia — cùng một tham số cho hai hành vi khác hẳn.
//! Cộng trên thứ hạng thì tham số có nghĩa đọc được và **có chặn trên**: một skill
//! tệ nhất mức tối đa tụt nhiều nhất [`LAMBDA_HANG`] bậc, không bao giờ lật được
//! một khoảng cách liên quan lớn.

/// Bốn loại tín hiệu ở §2 (`signals/types.py` của OpenSpace).
///
/// Giữ dạng `&str` chứ không enum vì cột DB là `TEXT` và sổ cái phải đọc được cả
/// loại mà bản này chưa biết — một `kind` lạ được **bỏ qua** (trọng số 0) chứ
/// không làm hỏng cả phép xếp hạng.
pub const KIND_TOOL_CALL_FAILED: &str = "tool_call_failed";
pub const KIND_TOOL_FAILURE_AFFECTS_SKILL: &str = "tool_failure_affects_skill";
pub const KIND_SKILL_SELECTION_NOT_INVOKED: &str = "skill_selection_not_invoked";
pub const KIND_TOOL_SEMANTIC_ISSUE: &str = "tool_semantic_issue";

/// Bốn loại tín hiệu **không** nặng như nhau.
///
/// - `tool_failure_affects_skill` (1,0): lỗi đã được quy về skill này. Bằng chứng
///   trực tiếp nhất.
/// - `tool_semantic_issue` (1,0): tool báo thành công nhưng kết quả sai. Nặng
///   ngang lỗi được quy trách — và tệ hơn theo một nghĩa: nó **im lặng**, người
///   dùng không có dấu hiệu nào để mà nghi.
/// - `tool_call_failed` (0,5): có lỗi trong lượt mà skill này đang tham gia, nhưng
///   chưa quy được trách nhiệm. Đáng ghi, chưa đáng kết luận.
/// - `skill_selection_not_invoked` (0,25): được truy hồi rồi không ai dùng. Đây là
///   dấu hiệu *ít hữu ích*, không phải dấu hiệu *lỗi* — nên nhẹ nhất.
fn trong_so_kind(kind: &str) -> f32 {
    match kind {
        KIND_TOOL_FAILURE_AFFECTS_SKILL | KIND_TOOL_SEMANTIC_ISSUE => 1.0,
        KIND_TOOL_CALL_FAILED => 0.5,
        KIND_SKILL_SELECTION_NOT_INVOKED => 0.25,
        // `kind` lạ (bản mới hơn ghi vào, hoặc người dùng tự đặt): không hiểu thì
        // không trừ điểm. Im lặng bỏ qua an toàn hơn là đoán mức nặng.
        _ => 0.0,
    }
}

/// Trọng số theo mức bằng chứng.
///
/// `refuted` = 0 là quyết định có ý — xem ghi chú (2) ở đầu file.
fn trong_so_bang_chung(evidence: Option<&str>) -> f32 {
    match evidence {
        Some("refuted") => 0.0,
        Some("confirmed") => 1.0,
        // Chưa xác minh (hoặc không ghi) vẫn tính, nhưng chỉ một nửa: sổ cái phải
        // dùng được ngay khi chưa ai đi xác minh, mà không được coi tin chưa kiểm
        // ngang tin đã kiểm.
        _ => 0.5,
    }
}

/// Mức bão hoà: tổng trọng số bằng ngần này thì hình phạt đạt **một nửa** mức tối
/// đa.
///
/// 2,0 nghĩa là hai lỗi đã-xác-minh-và-quy-trách là đủ để mất nửa mức phạt. Chọn
/// nhỏ vì tập skill nhỏ và tín hiệu thưa: đặt cao thì prior gần như không bao giờ
/// kích hoạt, tức là code có mà không có tác dụng — dạng chết lặng tệ hơn không
/// làm.
const BAO_HOA: f32 = 2.0;

/// Số bậc tối đa mà một skill tệ nhất mức có thể bị tụt.
///
/// Đây là **chặn trên tường minh**, và là lý do prior cộng trên thứ hạng chứ không
/// trên điểm: 3 nghĩa là prior chỉ đảo được các trường hợp gần ngang nhau, không
/// bao giờ đẩy một skill không liên quan lên trước một skill đúng. Truy hồi vẫn do
/// liên quan quyết định; chất lượng chỉ phá thế cân bằng.
const LAMBDA_HANG: f32 = 3.0;

/// Số tín hiệu **phân biệt theo `merge_key`** của một skill, tách theo `kind` và
/// `evidence_status`.
///
/// Một dòng ở đây = một *vấn đề*, không phải một lần quan sát. Xem ghi chú (1).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SignalTally {
    /// `(kind, evidence_status, số vấn đề phân biệt)`.
    pub theo_loai: Vec<(String, Option<String>, i64)>,
}

impl SignalTally {
    /// Tổng trọng số thô, chưa bão hoà. Public để probe/test in ra được con số
    /// trung gian thay vì chỉ thấy kết quả cuối.
    pub fn tong_trong_so(&self) -> f32 {
        self.theo_loai
            .iter()
            .map(|(kind, ev, n)| {
                trong_so_kind(kind) * trong_so_bang_chung(ev.as_deref()) * (*n as f32)
            })
            .sum()
    }

    /// Hình phạt trong `[0, 1)`, bão hoà kiểu hyperbol: `t / (t + BAO_HOA)`.
    ///
    /// Bão hoà chứ không tuyến tính vì lỗi thứ mười không nói thêm gì so với lỗi
    /// thứ ba — và tuyến tính thì một skill dùng nhiều (nên có nhiều tín hiệu) sẽ
    /// bị phạt nặng hơn một skill không ai dùng, tức là thưởng cho sự vô dụng.
    ///
    /// Không bao giờ đạt đúng 1,0: luôn còn đường hồi phục bằng cách thêm bằng
    /// chứng phản chứng, chứ không có trạng thái "chết hẳn".
    pub fn hinh_phat(&self) -> f32 {
        let t = self.tong_trong_so();
        if t <= 0.0 {
            return 0.0;
        }
        t / (t + BAO_HOA)
    }
}

/// Khoá xếp hạng sau khi hoà prior vào thứ hạng liên quan.
///
/// `hang_lien_quan` là vị trí 0-based **sau** khi đã xếp theo cosine/BM25.
/// Kết quả: `hang + LAMBDA_HANG × hình_phạt`.
///
/// Hoà bằng nhau thì `sort_by` ổn định giữ nguyên thứ tự liên quan — nên một skill
/// bị phạt tối đa chỉ *bị vượt* bởi skill trong vòng [`LAMBDA_HANG`] bậc dưới nó
/// mà có hình phạt thấp hơn hẳn, không bị đẩy xuống cuối danh sách.
pub fn khoa_hoa_tron(hang_lien_quan: usize, hinh_phat: f32) -> f32 {
    hang_lien_quan as f32 + LAMBDA_HANG * hinh_phat.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tally(v: &[(&str, Option<&str>, i64)]) -> SignalTally {
        SignalTally {
            theo_loai: v
                .iter()
                .map(|(k, e, n)| (k.to_string(), e.map(str::to_string), *n))
                .collect(),
        }
    }

    #[test]
    fn khong_tin_hieu_thi_khong_phat() {
        assert_eq!(SignalTally::default().hinh_phat(), 0.0);
        assert_eq!(khoa_hoa_tron(2, 0.0), 2.0, "không phạt ⇒ khoá = đúng thứ hạng");
    }

    /// Quyết định (2): tín hiệu đã bị phản chứng phải KHÔNG trừ điểm. Không có
    /// test này thì một hồi quy làm `refuted` tính như `confirmed` sẽ đi qua lặng
    /// lẽ — và biểu hiện của nó là "skill tự nhiên xếp thấp", rất khó truy.
    #[test]
    fn tin_hieu_bi_phan_chung_khong_phat() {
        let t = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("refuted"), 50)]);
        assert_eq!(t.tong_trong_so(), 0.0, "50 lỗi đã phản chứng vẫn phải là 0");
        assert_eq!(t.hinh_phat(), 0.0);
    }

    #[test]
    fn chua_xac_minh_nhe_hon_da_xac_minh() {
        let da = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 1)]);
        let chua = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, None, 1)]);
        assert!(
            chua.hinh_phat() < da.hinh_phat(),
            "chưa xác minh {} phải nhẹ hơn đã xác minh {}",
            chua.hinh_phat(),
            da.hinh_phat()
        );
        assert!(chua.hinh_phat() > 0.0, "nhưng vẫn phải tính, không bỏ hẳn");
    }

    #[test]
    fn bon_loai_xep_dung_thu_tu_nang_nhe() {
        let p = |k: &str| tally(&[(k, Some("confirmed"), 1)]).hinh_phat();
        let quy_trach = p(KIND_TOOL_FAILURE_AFFECTS_SKILL);
        let ngu_nghia = p(KIND_TOOL_SEMANTIC_ISSUE);
        let that_bai = p(KIND_TOOL_CALL_FAILED);
        let khong_dung = p(KIND_SKILL_SELECTION_NOT_INVOKED);
        assert_eq!(quy_trach, ngu_nghia, "lỗi im lặng nặng ngang lỗi quy trách");
        assert!(that_bai < quy_trach, "chưa quy trách thì nhẹ hơn");
        assert!(khong_dung < that_bai, "không ai dùng ≠ lỗi, phải nhẹ nhất");
        assert!(khong_dung > 0.0);
    }

    /// `kind` lạ không được làm hỏng phép tính — bản sau có thể thêm loại mới, và
    /// DB cũ đọc bằng code mới (hoặc ngược lại) phải chạy được.
    #[test]
    fn kind_la_thi_bo_qua_chu_khong_hong() {
        let t = tally(&[("mot_loai_chua_ton_tai", Some("confirmed"), 99)]);
        assert_eq!(t.tong_trong_so(), 0.0);
        assert_eq!(t.hinh_phat(), 0.0);
    }

    #[test]
    fn hinh_phat_bao_hoa_va_khong_bao_gio_dat_mot() {
        let p1 = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 1)]).hinh_phat();
        let p5 = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 5)]).hinh_phat();
        let p500 = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 500)]).hinh_phat();
        assert!(p1 < p5 && p5 < p500, "phải đơn điệu tăng: {p1} {p5} {p500}");
        assert!(p500 < 1.0, "không bao giờ đạt 1,0 ⇒ luôn còn đường hồi phục");
        // Bão hoà thật: từ 5 lên 500 lỗi thêm rất ít, còn từ 1 lên 5 thêm nhiều.
        assert!(
            p500 - p5 < p5 - p1,
            "phải bão hoà, không tuyến tính: Δ(5→500)={} vs Δ(1→5)={}",
            p500 - p5,
            p5 - p1
        );
    }

    #[test]
    fn phat_o_muc_bao_hoa_dung_bang_mot_nua() {
        // Tổng trọng số = BAO_HOA ⇒ hình phạt = 0,5 theo định nghĩa hyperbol.
        let t = tally(&[(KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 2)]);
        assert_eq!(t.tong_trong_so(), BAO_HOA);
        assert!((t.hinh_phat() - 0.5).abs() < 1e-6, "{}", t.hinh_phat());
    }

    /// Chặn trên là cả lý do prior cộng trên thứ hạng: nó KHÔNG được lật một
    /// khoảng cách liên quan lớn.
    #[test]
    fn prior_khong_lat_duoc_khoang_cach_lien_quan_lon() {
        let phat_toi_da = 1.0;
        let khoa_tot_nhat_nhung_te = khoa_hoa_tron(0, phat_toi_da);
        // Skill xếp thứ 4 mà sạch tín hiệu: vẫn KHÔNG vượt được skill thứ 0.
        let khoa_thu_tu_sach = khoa_hoa_tron(4, 0.0);
        assert!(
            khoa_tot_nhat_nhung_te < khoa_thu_tu_sach,
            "tụt tối đa {LAMBDA_HANG} bậc: {khoa_tot_nhat_nhung_te} vs {khoa_thu_tu_sach}"
        );
        // Nhưng skill ngay sát dưới thì vượt được — nếu không thì prior vô dụng.
        assert!(khoa_hoa_tron(1, 0.0) < khoa_tot_nhat_nhung_te);
    }

    #[test]
    fn nhieu_loai_cong_don() {
        let t = tally(&[
            (KIND_TOOL_FAILURE_AFFECTS_SKILL, Some("confirmed"), 1),
            (KIND_SKILL_SELECTION_NOT_INVOKED, Some("confirmed"), 2),
        ]);
        assert_eq!(t.tong_trong_so(), 1.0 * 1.0 + 0.25 * 1.0 * 2.0);
    }
}
