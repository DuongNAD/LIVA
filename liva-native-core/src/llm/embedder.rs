//! Model embedding chuyên dụng cho bộ nhớ dài hạn — TÁCH KHỎI model chat.
//!
//! # Vì sao không dùng embedding của model chat
//!
//! `llm::embed::get_embedding` lấy embedding từ chính `LlamaContext` đang chạy
//! chat. Cách đó có ba vấn đề:
//!
//! 1. **Chiều phụ thuộc model chat.** Qwen3-VL-2B cho 2048 chiều, gemma cho số
//!    khác. Bảng `vec_idx` khai cứng `int8[384]`, nên ghi vào sẽ lỗi:
//!    `Dimension mismatch ... Expected 384 dimensions but received 2048`
//!    (đã kiểm chứng thực nghiệm, sqlite-vec báo lỗi rõ chứ không ghi sai lặng lẽ).
//! 2. **Đổi model chat là mất bộ nhớ.** Vector sinh bởi model A không so sánh
//!    được với vector sinh bởi model B. Người dùng đổi router model thì toàn bộ
//!    index cũ thành vô nghĩa.
//! 3. **Chất lượng truy hồi kém.** Model sinh (nhất là model thị giác) không
//!    được huấn luyện cho similarity search.
//!
//! Module này dùng một model ONNX riêng, cố định 384 chiều, để bộ nhớ độc lập
//! hoàn toàn với model chat đang nạp.
//!
//! # Hợp đồng model
//!
//! Thư mục model (mặc định `models/embedding`, đổi bằng `LIVA_EMBEDDING_MODEL_DIR`)
//! cần hai file:
//!   - `model.onnx`     — input `input_ids` + `attention_mask` (i64 [1, seq]),
//!                        output tensor cuối cùng dạng [1, seq, 384]
//!   - `tokenizer.json` — tokenizer HuggingFace tương ứng
//!
//! Model khuyến nghị: `intfloat/multilingual-e5-small` (384 chiều, hỗ trợ tiếng
//! Việt). Với họ E5, tiền tố `query: ` / `passage: ` là bắt buộc để đúng cách
//! model được huấn luyện — xem [`EmbeddingEngine::embed_query`] và
//! [`EmbeddingEngine::embed_passage`].
//!
//! Thiếu model **không phải lỗi chí mạng**: [`EmbeddingEngine::load`] trả `Err`
//! có hướng khắc phục, và tầng gọi được phép chạy tiếp mà không có RAG.

use ort::{session::Session, value::Value};
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;

/// Số chiều vector bộ nhớ. Phải khớp `db::MEMORY_VECTOR_DIM` và cột
/// `vec_idx(embedding int8[384])`.
pub const EMBEDDING_DIM: usize = 384;

/// Cắt bớt token đầu vào. Model họ E5 huấn luyện ở 512; dài hơn không giúp gì
/// mà còn chậm.
const MAX_TOKENS: usize = 512;

/// Resolve thư mục model: env override, rồi `models/embedding` với fallback
/// `../` và `../../` (cwd khác nhau tuỳ điểm vào — repo root, liva-native-core,
/// hay liva-desktop/src-tauri).
pub fn resolve_model_dir() -> PathBuf {
    if let Ok(p) = std::env::var("LIVA_EMBEDDING_MODEL_DIR") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    for candidate in [
        PathBuf::from("models/embedding"),
        PathBuf::from("../models/embedding"),
        PathBuf::from("../../models/embedding"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("models/embedding")
}

pub struct EmbeddingEngine {
    session: Session,
    tokenizer: Tokenizer,
    model_dir: PathBuf,
}

impl EmbeddingEngine {
    /// Nạp model. Trả `Err` có hướng khắc phục khi thiếu file — tầng gọi nên
    /// coi đây là "không có RAG" chứ không phải sập.
    pub fn load(model_dir: &Path) -> Result<Self, String> {
        let onnx_path = model_dir.join("model.onnx");
        let tok_path = model_dir.join("tokenizer.json");

        if !onnx_path.exists() || !tok_path.exists() {
            return Err(format!(
                "khong tim thay model embedding trong {:?} (can ca model.onnx va tokenizer.json). \
                 Tai mot model 384 chieu — khuyen nghi intfloat/multilingual-e5-small — \
                 roi dat vao thu muc do, hoac tro LIVA_EMBEDDING_MODEL_DIR sang noi khac. \
                 Thieu model thi bo nho dai han khong ghi/tim duoc, phan con lai van chay.",
                model_dir
            ));
        }

        let threads = std::env::var("LIVA_EMBEDDING_THREADS")
            .ok()
            .and_then(|v| v.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(1);

        let session = Session::builder()
            .map_err(|e| format!("Failed to create SessionBuilder: {}", e))?
            .with_intra_threads(threads)
            .map_err(|e| format!("Failed to configure intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to configure inter threads: {}", e))?
            .commit_from_file(&onnx_path)
            .map_err(|e| format!("Failed to load embedding model {:?}: {}", onnx_path, e))?;

        let tokenizer = Tokenizer::from_file(&tok_path)
            .map_err(|e| format!("Failed to load tokenizer {:?}: {}", tok_path, e))?;

        Ok(Self {
            session,
            tokenizer,
            model_dir: model_dir.to_path_buf(),
        })
    }

    pub fn model_dir(&self) -> &Path {
        &self.model_dir
    }

    /// Embedding cho **câu truy vấn**. Với họ E5 phải có tiền tố `query: `.
    pub fn embed_query(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_raw(&format!("query: {}", text))
    }

    /// Embedding cho **đoạn văn được lưu**. Với họ E5 phải có tiền tố `passage: `.
    ///
    /// Dùng sai cặp query/passage sẽ làm điểm tương đồng lệch một cách khó
    /// nhận ra — kết quả vẫn trả về, chỉ là kém hơn đáng kể.
    pub fn embed_passage(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_raw(&format!("passage: {}", text))
    }

    /// Chạy model trên văn bản đã có tiền tố. Mean-pooling theo attention mask
    /// rồi chuẩn hoá L2 — đúng công thức tham chiếu của họ E5/MiniLM.
    fn embed_raw(&mut self, text: &str) -> Result<Vec<f32>, String> {
        let encoding = self
            .tokenizer
            .encode(text, true)
            .map_err(|e| format!("Tokenization failed: {}", e))?;

        let mut ids: Vec<i64> = encoding.get_ids().iter().map(|&x| x as i64).collect();
        let mut mask: Vec<i64> = encoding
            .get_attention_mask()
            .iter()
            .map(|&x| x as i64)
            .collect();

        if ids.is_empty() {
            return Err("chuoi rong sau khi tokenize".to_string());
        }
        ids.truncate(MAX_TOKENS);
        mask.truncate(MAX_TOKENS);

        let seq = ids.len();
        let ids_tensor = Value::from_array(([1usize, seq], ids))
            .map_err(|e| format!("Failed to build input_ids tensor: {}", e))?;
        let mask_tensor = Value::from_array(([1usize, seq], mask.clone()))
            .map_err(|e| format!("Failed to build attention_mask tensor: {}", e))?;

        let outputs = self
            .session
            .run(ort::inputs![
                "input_ids" => ids_tensor,
                "attention_mask" => mask_tensor,
            ])
            .map_err(|e| format!("Embedding ONNX run failed: {}", e))?;

        // Lấy output đầu tiên: tên khác nhau giữa các bản export
        // (last_hidden_state / token_embeddings / output_0). Giữ `first` sống
        // riêng vì `try_extract_tensor` mượn từ nó.
        let first = outputs
            .iter()
            .next()
            .ok_or("model embedding khong tra ve output nao")?
            .1;
        let (shape, data) = first
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract embedding output: {}", e))?;

        let hidden = *shape.last().ok_or("output embedding khong co chieu nao")? as usize;
        if hidden != EMBEDDING_DIM {
            return Err(format!(
                "model embedding tra ve {} chieu nhung he thong can {}. \
                 Bang vec_idx khai cung int8[{}] — hay dung model 384 chieu \
                 (vi du intfloat/multilingual-e5-small).",
                hidden, EMBEDDING_DIM, EMBEDDING_DIM
            ));
        }

        let pooled = mean_pool(data, &mask, hidden)?;
        Ok(l2_normalize(pooled))
    }
}

/// Mean-pooling theo attention mask: chỉ tính token thật, bỏ padding.
///
/// Tách riêng khỏi `embed_raw` để test được mà không cần nạp model ONNX —
/// đây là phần logic dễ sai nhất và cũng là phần duy nhất kiểm được offline.
pub fn mean_pool(data: &[f32], mask: &[i64], hidden: usize) -> Result<Vec<f32>, String> {
    if hidden == 0 {
        return Err("hidden size = 0".to_string());
    }
    if data.len() % hidden != 0 {
        return Err(format!(
            "kich thuoc output {} khong chia het cho hidden {}",
            data.len(),
            hidden
        ));
    }
    let seq = data.len() / hidden;
    if seq == 0 {
        return Err("output khong co token nao".to_string());
    }

    let mut sum = vec![0.0f32; hidden];
    let mut kept = 0usize;
    for t in 0..seq {
        // mask ngắn hơn output (hoặc thiếu) thì coi như token thật.
        if mask.get(t).copied().unwrap_or(1) == 0 {
            continue;
        }
        kept += 1;
        let base = t * hidden;
        for h in 0..hidden {
            sum[h] += data[base + h];
        }
    }

    // Toàn bộ bị mask thì lấy trung bình tất cả, còn hơn trả vector 0.
    if kept == 0 {
        for t in 0..seq {
            let base = t * hidden;
            for h in 0..hidden {
                sum[h] += data[base + h];
            }
        }
        kept = seq;
    }

    let inv = 1.0 / kept as f32;
    for v in sum.iter_mut() {
        *v *= inv;
    }
    Ok(sum)
}

/// Chuẩn hoá L2. `vec_quantize_int8(?, 'unit')` của sqlite-vec giả định giá trị
/// nằm trong [-1, 1], nên bước này là BẮT BUỘC chứ không phải tuỳ chọn.
pub fn l2_normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        let inv = 1.0 / norm;
        for x in v.iter_mut() {
            *x *= inv;
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mean_pool_bo_qua_token_bi_mask() {
        // 3 token, hidden = 2; token cuối bị mask
        let data = vec![1.0, 1.0, 3.0, 3.0, 100.0, 100.0];
        let mask = vec![1i64, 1, 0];
        let pooled = mean_pool(&data, &mask, 2).unwrap();
        assert_eq!(pooled, vec![2.0, 2.0], "token bi mask khong duoc tinh vao");
    }

    #[test]
    fn mean_pool_khong_co_mask_thi_tinh_het() {
        let data = vec![1.0, 1.0, 3.0, 3.0];
        let pooled = mean_pool(&data, &[], 2).unwrap();
        assert_eq!(pooled, vec![2.0, 2.0], "mask rong = coi tat ca la token that");
    }

    #[test]
    fn mean_pool_toan_bo_bi_mask_thi_khong_tra_vector_0() {
        let data = vec![2.0, 4.0, 6.0, 8.0];
        let pooled = mean_pool(&data, &[0i64, 0], 2).unwrap();
        assert_eq!(pooled, vec![4.0, 6.0], "phai lay trung binh tat ca thay vi vector 0");
    }

    #[test]
    fn mean_pool_bat_loi_kich_thuoc_le() {
        assert!(mean_pool(&[1.0, 2.0, 3.0], &[1], 2).is_err(), "3 khong chia het cho 2");
        assert!(mean_pool(&[], &[], 2).is_err(), "output rong");
        assert!(mean_pool(&[1.0], &[1], 0).is_err(), "hidden = 0");
    }

    #[test]
    fn l2_normalize_cho_ra_do_dai_1() {
        let v = l2_normalize(vec![3.0, 4.0]);
        assert!((v[0] - 0.6).abs() < 1e-6);
        assert!((v[1] - 0.8).abs() < 1e-6);
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-6);
    }

    #[test]
    fn l2_normalize_vector_0_khong_chia_cho_0() {
        let v = l2_normalize(vec![0.0, 0.0, 0.0]);
        assert_eq!(v, vec![0.0, 0.0, 0.0], "vector 0 phai giu nguyen, khong NaN");
        assert!(v.iter().all(|x| x.is_finite()));
    }

    #[test]
    fn l2_normalize_giu_moi_gia_tri_trong_khoang_don_vi() {
        // vec_quantize_int8(_, 'unit') gia dinh [-1, 1]
        let v = l2_normalize(vec![100.0, -250.0, 7.0, -3.0]);
        assert!(v.iter().all(|x| *x >= -1.0 && *x <= 1.0), "phai nam trong [-1,1]");
    }

    #[test]
    fn thieu_model_thi_bao_loi_co_huong_khac_phuc() {
        let err = match EmbeddingEngine::load(Path::new("khong-ton-tai-abc-xyz")) {
            Err(e) => e,
            Ok(_) => panic!("thu muc khong ton tai ma van nap duoc model?"),
        };
        assert!(err.contains("multilingual-e5-small"), "phai goi y model cu the: {}", err);
        assert!(err.contains("LIVA_EMBEDDING_MODEL_DIR"), "phai neu cach doi duong dan: {}", err);
    }

    /// Chỉ chạy khi model thật có trên đĩa; trên máy CI/dev chưa tải thì bỏ qua.
    #[test]
    fn embed_that_khi_co_model() {
        let dir = resolve_model_dir();
        if !dir.join("model.onnx").exists() {
            eprintln!("bo qua: chua co model embedding tai {:?}", dir);
            return;
        }
        let mut eng = EmbeddingEngine::load(&dir).expect("nap model embedding");

        let a = eng.embed_passage("con mèo đang ngủ trên ghế").unwrap();
        assert_eq!(a.len(), EMBEDDING_DIM);
        let norm = a.iter().map(|x| x * x).sum::<f32>().sqrt();
        assert!((norm - 1.0).abs() < 1e-3, "phai duoc chuan hoa L2");

        // Câu gần nghĩa phải giống hơn câu lạc đề.
        let gan = eng.embed_query("mèo nằm trên ghế").unwrap();
        let xa = eng.embed_query("giá cổ phiếu hôm nay").unwrap();
        let cos = |x: &[f32], y: &[f32]| x.iter().zip(y).map(|(p, q)| p * q).sum::<f32>();
        assert!(
            cos(&a, &gan) > cos(&a, &xa),
            "cau gan nghia phai co diem cao hon cau lac de"
        );
    }
}
