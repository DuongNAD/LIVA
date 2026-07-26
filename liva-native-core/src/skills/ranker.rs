//! Truy hồi skill: BM25 tiền lọc → embedder rerank.
//!
//! Đúng thiết kế mà §2 tài liệu 04 quyết định **lấy** từ `skill_ranker.py` của
//! OpenSpace: BM25 lấy recall, embedding xếp lại, giữ `top_k × 3` ứng viên.
//!
//! ## Một bài học từ G1 được áp ở đây
//!
//! Đo được ở G1 (26/07/2026): xếp hạng theo **trùng token** trên mô tả tiếng Anh
//! là **MÙ hoàn toàn** với câu tiếng Việt — 0 điểm cho mọi câu. Skill trong repo
//! này cũng mô tả bằng tiếng Anh, còn người dùng nói tiếng Việt. Nên nếu để BM25
//! quyết định danh sách ứng viên một cách cứng nhắc, bộ rerank sẽ **bị bỏ đói**:
//! nó không bao giờ thấy skill đúng.
//!
//! Vì vậy [`rank_skills`] có một quy tắc tường minh: BM25 ra **quá ít** ứng viên
//! thì lấy **toàn bộ** skill làm ứng viên rồi để embedder xếp. BM25 ở đây là
//! *recall booster*, không phải cửa chặn.

use super::LoadedSkill;
use crate::llm::tool_calling::ToolEmbedder;

/// Tham số BM25 tiêu chuẩn.
const K1: f32 = 1.2;
const B: f32 = 0.75;

/// Số ứng viên tối thiểu đưa sang bộ rerank.
///
/// Lấy 10 theo đúng "ngưỡng tiền lọc 10" ghi ở §2. Ý nghĩa ở đây: **số ứng viên
/// tối thiểu**, không phải ngưỡng điểm — điểm tuyệt đối của E5 nằm trong dải hẹp
/// nên ngưỡng điểm là ý tồi (đo ở G1).
const PREFILTER_MIN: usize = 10;

/// Kết quả xếp hạng cho một skill.
#[derive(Debug, Clone, PartialEq)]
pub struct RankedSkill {
    /// Chỉ số trong slice đầu vào.
    pub index: usize,
    pub bm25: f32,
    /// `None` khi không có embedder (hoặc embedder lỗi).
    pub cosine: Option<f32>,
    /// Điểm dùng để sắp: cosine nếu có, không thì BM25.
    pub score: f32,
}

/// Tách token theo ranh giới ký tự chữ-số Unicode.
///
/// `is_alphanumeric` chứ không `is_ascii_alphanumeric`, để `đèn`/`bật`/`ghi` là
/// token trọn vẹn — cùng quy tắc với `llm::tool_calling` và `agent::graph`.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Điểm BM25 của từng skill với một câu truy vấn.
///
/// Tính trên chỗ: tập skill nhỏ (hàng chục), nên không cần index nghịch đảo bền.
/// Đổi lại là code không có trạng thái cần đồng bộ với DB.
pub fn bm25_scores(skills: &[LoadedSkill], query: &str) -> Vec<f32> {
    let n = skills.len();
    if n == 0 {
        return Vec::new();
    }
    let docs: Vec<Vec<String>> = skills.iter().map(|s| tokenize(&s.search_text())).collect();
    let do_dai: Vec<f32> = docs.iter().map(|d| d.len() as f32).collect();
    let tb: f32 = do_dai.iter().sum::<f32>() / n as f32;
    let tb = if tb > 0.0 { tb } else { 1.0 };

    let q = tokenize(query);
    let mut diem = vec![0.0f32; n];

    for tu in &q {
        // df: số tài liệu chứa từ này.
        let df = docs.iter().filter(|d| d.contains(tu)).count() as f32;
        if df == 0.0 {
            continue;
        }
        // IDF dạng Robertson–Sparck Jones, cộng 1 để không âm.
        let idf = ((n as f32 - df + 0.5) / (df + 0.5) + 1.0).ln();
        for i in 0..n {
            let tf = docs[i].iter().filter(|w| *w == tu).count() as f32;
            if tf == 0.0 {
                continue;
            }
            let chuan = K1 * (1.0 - B + B * do_dai[i] / tb);
            diem[i] += idf * (tf * (K1 + 1.0)) / (tf + chuan);
        }
    }
    diem
}

/// Xếp hạng skill cho một câu truy vấn.
///
/// Trình tự:
/// 1. BM25 trên toàn bộ skill.
/// 2. Ứng viên = `top_k × 3` skill có BM25 cao nhất, nhưng **tối thiểu**
///    [`PREFILTER_MIN`]. Nếu số skill có BM25 > 0 ít hơn ngần đó, lấy **tất cả** —
///    xem ghi chú tiếng Việt/tiếng Anh ở đầu file.
/// 3. Có embedder → rerank ứng viên theo cosine. Không có → giữ thứ tự BM25.
/// 4. Cắt còn `top_k`.
///
/// Hoà điểm giữ thứ tự đầu vào (`sort_by` ổn định) ⇒ kết quả tất định, test được.
pub fn rank_skills(
    skills: &[LoadedSkill],
    query: &str,
    embedder: Option<&mut dyn ToolEmbedder>,
    top_k: usize,
) -> Vec<RankedSkill> {
    if skills.is_empty() || top_k == 0 {
        return Vec::new();
    }
    let bm25 = bm25_scores(skills, query);

    let mut idx: Vec<usize> = (0..skills.len()).collect();
    idx.sort_by(|&a, &b| {
        bm25[b]
            .partial_cmp(&bm25[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let co_diem = bm25.iter().filter(|d| **d > 0.0).count();
    let muon = top_k.saturating_mul(3).max(PREFILTER_MIN);
    let so_ung_vien = if co_diem < muon {
        // BM25 không đủ recall (ca tiếng Việt vs mô tả tiếng Anh) ⇒ đừng bỏ đói
        // bộ rerank.
        skills.len()
    } else {
        muon.min(skills.len())
    };
    idx.truncate(so_ung_vien);

    let mut ra: Vec<RankedSkill> = idx
        .iter()
        .map(|&i| RankedSkill {
            index: i,
            bm25: bm25[i],
            cosine: None,
            score: bm25[i],
        })
        .collect();

    if let Some(e) = embedder {
        match cham_diem_cosine(skills, query, &mut ra, e) {
            Ok(()) => ra.sort_by(|a, b| {
                b.score
                    .partial_cmp(&a.score)
                    .unwrap_or(std::cmp::Ordering::Equal)
            }),
            Err(err) => {
                tracing::warn!("rerank skill bằng embedding thất bại ({err}); giữ thứ tự BM25")
            }
        }
    }

    ra.truncate(top_k);
    ra
}

/// Vector từ `EmbeddingEngine` đã chuẩn hoá L2 nên tích vô hướng **chính là**
/// cosine — xem `llm::embedder::embed_raw`.
fn cham_diem_cosine(
    skills: &[LoadedSkill],
    query: &str,
    ung_vien: &mut [RankedSkill],
    e: &mut dyn ToolEmbedder,
) -> Result<(), String> {
    let q = e.embed_query_vec(query)?;
    for r in ung_vien.iter_mut() {
        let v = e.embed_passage_vec(&skills[r.index].search_text())?;
        if v.len() != q.len() {
            return Err(format!("số chiều lệch: {} vs {}", q.len(), v.len()));
        }
        let cos: f32 = q.iter().zip(&v).map(|(a, b)| a * b).sum();
        r.cosine = Some(cos);
        r.score = cos;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn sk(name: &str, desc: &str, body: &str) -> LoadedSkill {
        LoadedSkill {
            skill_id: format!("id-{name}"),
            name: name.to_string(),
            description: desc.to_string(),
            body: body.to_string(),
            body_sha: String::new(),
            dir_path: PathBuf::from(name),
        }
    }

    fn bo() -> Vec<LoadedSkill> {
        vec![
            sk("git-review", "Review a pull request diff", "look at the diff, comment"),
            sk("db-migrate", "Add a SQLite migration safely", "PRAGMA user_version, transaction"),
            sk("voice-debug", "Debug the voice pipeline", "STT, TTS, VAD, barge-in"),
        ]
    }

    #[test]
    fn bm25_xep_dung_khi_cau_dung_tu_cua_skill() {
        let s = bo();
        let d = bm25_scores(&s, "sqlite migration");
        let tot_nhat = d
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .unwrap()
            .0;
        assert_eq!(s[tot_nhat].name, "db-migrate", "điểm: {d:?}");
    }

    #[test]
    fn bm25_bang_khong_khi_khong_chia_tu_nao() {
        let s = bo();
        let d = bm25_scores(&s, "hoàn toàn không liên quan gì");
        assert!(d.iter().all(|x| *x == 0.0), "{d:?}");
    }

    #[test]
    fn corpus_rong_hoac_top_k_khong_thi_tra_rong() {
        assert!(bm25_scores(&[], "x").is_empty());
        assert!(rank_skills(&[], "x", None, 3).is_empty());
        assert!(rank_skills(&bo(), "x", None, 0).is_empty());
    }

    #[test]
    fn khong_co_embedder_thi_giu_thu_tu_bm25() {
        let s = bo();
        let r = rank_skills(&s, "sqlite migration", None, 2);
        assert_eq!(r.len(), 2);
        assert_eq!(s[r[0].index].name, "db-migrate");
        assert!(r.iter().all(|x| x.cosine.is_none()));
    }

    /// Đây là ca mà cả rung G2 phải chịu được: câu TIẾNG VIỆT, mô tả skill TIẾNG
    /// ANH ⇒ BM25 = 0 hết. Nếu tiền lọc là cửa chặn thì embedder không bao giờ
    /// thấy skill đúng.
    #[test]
    fn bm25_mu_thi_van_dua_du_ung_vien_cho_rerank() {
        let s = bo();
        let r = rank_skills(&s, "giúp mình xem lại thay đổi mã", None, 2);
        assert_eq!(
            r.len(),
            2,
            "BM25 = 0 hết vẫn phải trả về ứng viên, không được rỗng"
        );
        // Và toàn bộ skill phải từng được coi là ứng viên: kiểm bằng embedder giả
        // ở test dưới.
    }

    /// Embedder giả, tất định, không nạp model: khớp "diff/review" theo trục 0 và
    /// "migration" theo trục 1. Câu tiếng Việt được cắm để khớp trục 0.
    struct EmbGia;
    impl ToolEmbedder for EmbGia {
        fn embed_query_vec(&mut self, t: &str) -> Result<Vec<f32>, String> {
            Ok(vec![
                if t.contains("thay đổi mã") { 1.0 } else { 0.0 },
                if t.contains("chuyển đổi dữ liệu") { 1.0 } else { 0.0 },
            ])
        }
        fn embed_passage_vec(&mut self, t: &str) -> Result<Vec<f32>, String> {
            Ok(vec![
                if t.contains("diff") { 1.0 } else { 0.0 },
                if t.contains("migration") { 1.0 } else { 0.0 },
            ])
        }
    }

    #[test]
    fn embedder_cuu_duoc_ca_bm25_mu() {
        let s = bo();
        let r = rank_skills(&s, "giúp mình xem lại thay đổi mã", Some(&mut EmbGia), 1);
        assert_eq!(
            s[r[0].index].name, "git-review",
            "embedder phải nối câu tiếng Việt với skill tiếng Anh — đây là cả lý do có bước rerank"
        );
        assert_eq!(r[0].bm25, 0.0, "và BM25 thật sự đã mù ở ca này");
        assert!(r[0].cosine.is_some());
    }

    struct EmbHong;
    impl ToolEmbedder for EmbHong {
        fn embed_query_vec(&mut self, _: &str) -> Result<Vec<f32>, String> {
            Err("model hỏng".into())
        }
        fn embed_passage_vec(&mut self, _: &str) -> Result<Vec<f32>, String> {
            Err("model hỏng".into())
        }
    }

    #[test]
    fn embedder_loi_thi_roi_ve_bm25_chu_khong_hong() {
        let s = bo();
        let r = rank_skills(&s, "sqlite migration", Some(&mut EmbHong), 1);
        assert_eq!(s[r[0].index].name, "db-migrate");
        assert!(r[0].cosine.is_none(), "lỗi embedder ⇒ không có điểm cosine");
    }

    #[test]
    fn top_k_cat_dung_so_luong() {
        let s = bo();
        assert_eq!(rank_skills(&s, "diff", None, 1).len(), 1);
        assert_eq!(rank_skills(&s, "diff", None, 99).len(), 3, "không vượt số skill");
    }
}
