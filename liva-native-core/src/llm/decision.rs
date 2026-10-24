//! Single-Pass Logit Decision Routing (Granite Switch 4.1 pattern).
//!
//! Thay vì để LLM sinh chuỗi văn bản nhiều token (JSON/YAML) khi cần phân loại
//! ý định, đánh giá độ khó (`DoKho`), hoặc kiểm duyệt an toàn (Guardrail),
//! module này thực hiện **đúng 1 lượt forward pass (prefill)** và trích xuất
//! trực tiếp **logits** của các nhãn lựa chọn đơn token (`"A"`, `"B"`, `"C"`, …)
//! tại vị trí cuối cùng của prompt để tính phân phối xác suất **Softmax** đã hiệu chuẩn.
//!
//! ## Hai nguyên lý cốt lõi (học từ IBM Granite Switch 4.1)
//!
//! 1. **Zero Autoregressive Steps (`max_completion_tokens = 1`):**
//!    Ngay sau khi `LlamaContext::decode()` nạp xong `tail_tokens` của prompt,
//!    hàng logits cuối cùng đã chứa toàn bộ điểm số cho token đầu tiên của
//!    assistant. Ta đọc trực tiếp `logits[token_id]` của từng nhãn lựa chọn và
//!    áp dụng Softmax ổn định số học mà không cần chạy vòng lặp sinh token.
//!
//! 2. **Prefix KV Cache Reuse qua cấu trúc 2 lượt (`State` + `### DECISION`):**
//!    Lượt 1 (`user`) chỉ chứa `State:\n<ngữ cảnh>`. Lượt 2 (`user`) chứa câu hỏi
//!    quyết định và danh sách nhãn. Nhờ vậy, khi chạy liên tiếp nhiều quyết định
//!    (ví dụ: kiểm duyệt an toàn → phân loại độ khó → chọn công cụ) hoặc chuyển
//!    sang sinh câu trả lời trên cùng một `State`, `LlamaRouterManager` tự động
//!    tái sử dụng KV cache của Lượt 1 (`last_tokens` common prefix) mà không phải
//!    prefill lại ngữ cảnh.

use llama_cpp_2::model::{AddBos, LlamaModel};
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};

/// Một phương án lựa chọn trong bài toán quyết định 1-step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DecisionOption {
    /// Nhãn đơn token (ví dụ: `"A"`, `"B"`, `"C"` hoặc `"1"`, `"2"`).
    pub label: String,
    /// Mô tả ngữ nghĩa của phương án.
    pub description: String,
}

impl DecisionOption {
    pub fn new(label: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            description: description.into(),
        }
    }
}

/// Xác suất và logit thô của một phương án sau khi chạy Softmax.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ScoredDecisionOption {
    pub label: String,
    pub token_id: i32,
    pub logit: f32,
    pub probability: f32,
}

/// Kết quả phân phối quyết định từ một lượt forward pass duy nhất.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DecisionDistribution {
    /// Danh sách các phương án giữ nguyên thứ tự khai báo ban đầu kèm xác suất Softmax.
    pub options: Vec<ScoredDecisionOption>,
    /// Chỉ số (0-based) của phương án có xác suất cao nhất.
    pub best_index: usize,
    /// Nhãn của phương án có xác suất cao nhất.
    pub best_label: String,
    /// Xác suất Softmax của phương án đứng đầu (`[0.0, 1.0]`).
    pub best_probability: f32,
    /// Độ chênh lệch xác suất giữa phương án top-1 và top-2 (`p_1 - p_2`).
    /// Nếu chỉ có 1 phương án, `margin = 1.0`.
    pub margin: f32,
    /// Entropy Shannon chuẩn hóa trong `[0.0, 1.0]` (`H / ln(N)`).
    /// Giá trị gần `0.0` nghĩa là mô hình rất chắc chắn; gần `1.0` nghĩa là phân vân đều.
    pub normalized_entropy: f32,
    /// Tổng số token của prompt quyết định.
    pub prompt_tokens: usize,
    /// Số token tiền tố đã được tái sử dụng trực tiếp từ KV cache (`last_tokens`).
    pub reused_prefix_tokens: usize,
}

impl DecisionDistribution {
    /// Kiểm tra xem phương án tốt nhất có vượt ngưỡng tin cậy tối thiểu không.
    #[inline]
    pub fn is_confident(&self, min_probability: f32, min_margin: f32) -> bool {
        self.best_probability >= min_probability && self.margin >= min_margin
    }

    /// Tra cứu xác suất của một nhãn cụ thể.
    pub fn probability_of(&self, label: &str) -> Option<f32> {
        self.options
            .iter()
            .find(|o| o.label == label)
            .map(|o| o.probability)
    }
}

/// Tính phân phối xác suất Softmax ổn định số học (numerically stable softmax)
/// từ danh sách `(label, token_id, logit)`.
///
/// Trừ `max_logit` trước khi lấy `exp()` để chống tràn số (`overflow` / `underflow`).
pub fn compute_decision_distribution(
    raw_options: Vec<(String, i32, f32)>,
    prompt_tokens: usize,
    reused_prefix_tokens: usize,
) -> Result<DecisionDistribution, String> {
    if raw_options.is_empty() {
        return Err("Decision options cannot be empty".to_string());
    }

    for (label, _, logit) in &raw_options {
        if !logit.is_finite() {
            return Err(format!(
                "Non-finite logit ({logit}) encountered for option label '{label}'"
            ));
        }
    }

    let max_logit = raw_options
        .iter()
        .map(|(_, _, l)| *l)
        .fold(f32::NEG_INFINITY, f32::max);

    let exp_vals: Vec<f32> = raw_options
        .iter()
        .map(|(_, _, l)| (*l - max_logit).exp())
        .collect();
    let sum_exp: f32 = exp_vals.iter().sum();

    if !sum_exp.is_finite() || sum_exp <= 0.0 {
        return Err("Softmax normalization denominator is non-positive or non-finite".to_string());
    }

    let mut options = Vec::with_capacity(raw_options.len());
    let mut best_index = 0usize;
    let mut best_prob = -1.0f32;
    let mut second_best_prob = 0.0f32;

    for (idx, ((label, token_id, logit), exp_v)) in
        raw_options.into_iter().zip(exp_vals).enumerate()
    {
        let prob = exp_v / sum_exp;
        if prob > best_prob {
            second_best_prob = best_prob.max(0.0);
            best_prob = prob;
            best_index = idx;
        } else if prob > second_best_prob {
            second_best_prob = prob;
        }
        options.push(ScoredDecisionOption {
            label,
            token_id,
            logit,
            probability: prob,
        });
    }

    let n = options.len();
    let margin = if n <= 1 {
        1.0
    } else {
        (best_prob - second_best_prob).clamp(0.0, 1.0)
    };

    let normalized_entropy = if n <= 1 {
        0.0
    } else {
        let entropy: f32 = options
            .iter()
            .filter(|o| o.probability > 0.0)
            .map(|o| -o.probability * o.probability.ln())
            .sum();
        let max_entropy = (n as f32).ln();
        if max_entropy > 0.0 {
            (entropy / max_entropy).clamp(0.0, 1.0)
        } else {
            0.0
        }
    };

    let best_label = options[best_index].label.clone();

    Ok(DecisionDistribution {
        options,
        best_index,
        best_label,
        best_probability: best_prob,
        margin,
        normalized_entropy,
        prompt_tokens,
        reused_prefix_tokens,
    })
}

/// Phân giải chuỗi nhãn lựa chọn (ví dụ `"A"`, `"B"`, `"1"`) thành `LlamaToken` đơn lẻ
/// thông qua một hàm tokenize trừu tượng (giúp test độc lập không cần nạp GGUF nặng).
///
/// Quy tắc:
/// - Nhãn không được rỗng sau khi `trim()`.
/// - Gọi `tokenize_no_bos(trimmed)`.
/// - Nếu trả về đúng 1 token: dùng luôn token đó.
/// - Nếu trả về 2 token (trường hợp SentencePiece tự chèn token khoảng trắng `▁` đứng đầu):
///   lấy token thứ hai mang nội dung ký tự nhãn.
/// - Nếu nhiều hơn 2 token hoặc rỗng: từ chối với lỗi rõ ràng để tránh đọc sai vị trí logit.
pub fn resolve_option_token_with<F>(
    label: &str,
    mut tokenize_no_bos: F,
) -> Result<LlamaToken, String>
where
    F: FnMut(&str) -> Result<Vec<LlamaToken>, String>,
{
    let trimmed = label.trim();
    if trimmed.is_empty() {
        return Err("Option label cannot be empty or whitespace".to_string());
    }

    let tokens = tokenize_no_bos(trimmed)?;
    match tokens.as_slice() {
        [single] => Ok(*single),
        [_leading_space, label_tok] => Ok(*label_tok),
        [] => Err(format!("Option label '{trimmed}' tokenized into 0 tokens")),
        many => Err(format!(
            "Option label '{trimmed}' must be a single token, but tokenized into {} tokens",
            many.len()
        )),
    }
}

/// Phân giải danh sách nhãn lựa chọn trên `LlamaModel` thật, đảm bảo không có hai nhãn
/// nào ánh xạ trùng vào cùng một `LlamaToken`.
pub fn resolve_model_option_tokens(
    model: &LlamaModel,
    option_labels: &[&str],
) -> Result<Vec<(String, LlamaToken)>, String> {
    if option_labels.is_empty() {
        return Err("Option labels list cannot be empty".to_string());
    }

    let mut resolved = Vec::with_capacity(option_labels.len());
    for &raw_label in option_labels {
        let trimmed = raw_label.trim();
        let token = resolve_option_token_with(trimmed, |text| {
            model
                .str_to_token(text, AddBos::Never)
                .map_err(|e| format!("Failed to tokenize option label '{text}': {e:?}"))
        })?;

        if let Some((existing_label, _)) = resolved.iter().find(|(_, t)| *t == token) {
            return Err(format!(
                "Duplicate option token ID ({}) between labels '{}' and '{}'",
                token.0, existing_label, trimmed
            ));
        }

        resolved.push((trimmed.to_string(), token));
    }

    Ok(resolved)
}

/// Dựng nội dung lượt `### DECISION` theo chuẩn Granite Switch 4.1.
pub fn render_decision_turn(question: &str, options: &[DecisionOption]) -> Result<String, String> {
    let q = question.trim();
    if q.is_empty() {
        return Err("Decision question cannot be empty".to_string());
    }
    if options.is_empty() {
        return Err("Decision options cannot be empty".to_string());
    }

    let mut out = String::with_capacity(256);
    out.push_str("### DECISION\nQuestion:\n");
    out.push_str(q);
    out.push_str("\n\nOptions:\n");

    for opt in options {
        let label = opt.label.trim();
        let desc = opt.description.trim();
        if label.is_empty() {
            return Err("Decision option label cannot be empty".to_string());
        }
        if desc.is_empty() {
            out.push_str(&format!("{label}\n"));
        } else {
            out.push_str(&format!("{label}: {desc}\n"));
        }
    }

    out.push_str("\nReply with exactly one option label.");
    Ok(out)
}

/// Biên dịch prompt quyết định 2 lượt (`State` turn + `### DECISION` turn) qua
/// [`super::prompt::compile_prompt`] để tương thích mọi họ model (Gemma 4, Gemma, ChatML)
/// và tối ưu hóa tái sử dụng Prefix KV Cache cho lượt `State`.
pub fn compile_decision_prompt(
    state_text: &str,
    question: &str,
    options: &[DecisionOption],
) -> Result<String, String> {
    let state_trimmed = state_text.trim();
    if state_trimmed.is_empty() {
        return Err("Decision state_text cannot be empty".to_string());
    }
    let decision_turn = render_decision_turn(question, options)?;

    let messages = [
        super::ChatMessage {
            role: "user".to_string(),
            content: format!("State:\n{state_trimmed}"),
        },
        super::ChatMessage {
            role: "user".to_string(),
            content: decision_turn,
        },
    ];

    super::prompt::compile_prompt(&messages)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn softmax_on_calibrated_logits_matches_expected_distribution() {
        // Logits: A = 4.0, B = 0.0, C = 0.0
        let raw = vec![
            ("A".to_string(), 10, 4.0f32),
            ("B".to_string(), 11, 0.0f32),
            ("C".to_string(), 12, 0.0f32),
        ];
        let dist = compute_decision_distribution(raw, 42, 30).expect("valid distribution");

        assert_eq!(dist.best_index, 0);
        assert_eq!(dist.best_label, "A");
        assert_eq!(dist.prompt_tokens, 42);
        assert_eq!(dist.reused_prefix_tokens, 30);

        let sum_probs: f32 = dist.options.iter().map(|o| o.probability).sum();
        assert!((sum_probs - 1.0).abs() < 1e-5, "sum={sum_probs}");
        assert!(dist.best_probability > 0.96);
        assert!(dist.margin > 0.94);
        assert!(dist.normalized_entropy < 0.20);
        assert!(dist.is_confident(0.80, 0.50));
    }

    #[test]
    fn softmax_is_numerically_stable_under_extreme_logits() {
        // Nếu không trừ max_logit, exp(1000.0) sẽ thành +inf -> NaN.
        let raw = vec![
            ("A".to_string(), 1, 1000.0f32),
            ("B".to_string(), 2, 999.0f32),
        ];
        let dist = compute_decision_distribution(raw, 10, 0).expect("stable softmax");
        let p_a = dist.probability_of("A").unwrap();
        let p_b = dist.probability_of("B").unwrap();
        assert!((p_a + p_b - 1.0).abs() < 1e-6);
        // e^1 / (e^1 + 1) ≈ 0.7310586
        assert!((p_a - 0.7310586).abs() < 1e-4, "p_a={p_a}");
    }

    #[test]
    fn uniform_logits_yield_maximum_entropy_and_zero_margin() {
        let raw = vec![
            ("1".to_string(), 1, 2.5f32),
            ("2".to_string(), 2, 2.5f32),
            ("3".to_string(), 3, 2.5f32),
        ];
        let dist = compute_decision_distribution(raw, 16, 8).expect("uniform distribution");
        assert!(dist.margin.abs() < 1e-6);
        assert!((dist.normalized_entropy - 1.0).abs() < 1e-5);
        assert!(!dist.is_confident(0.5, 0.1));
    }

    #[test]
    fn rejects_empty_or_non_finite_logits() {
        assert!(compute_decision_distribution(vec![], 10, 0).is_err());
        assert!(
            compute_decision_distribution(vec![("A".to_string(), 1, f32::NAN)], 10, 0).is_err()
        );
        assert!(
            compute_decision_distribution(vec![("A".to_string(), 1, f32::INFINITY)], 10, 0)
                .is_err()
        );
    }

    #[test]
    fn resolve_option_token_handles_single_and_sentencepiece_prefixed_tokens() {
        // Trường hợp 1 token trực tiếp
        let tok = resolve_option_token_with("A", |_| Ok(vec![LlamaToken(330)])).unwrap();
        assert_eq!(tok, LlamaToken(330));

        // Trường hợp SentencePiece sinh [space_token, label_token]
        let tok_sp =
            resolve_option_token_with("B", |_| Ok(vec![LlamaToken(29871), LlamaToken(365)]))
                .unwrap();
        assert_eq!(tok_sp, LlamaToken(365));

        // Trường hợp nhãn nhiều token bị từ chối
        let err = resolve_option_token_with("OPTION_LONG", |_| {
            Ok(vec![LlamaToken(1), LlamaToken(2), LlamaToken(3)])
        });
        assert!(err.is_err());

        // Trường hợp nhãn rỗng bị từ chối
        assert!(resolve_option_token_with("   ", |_| Ok(vec![LlamaToken(1)])).is_err());
    }

    #[test]
    fn compile_decision_prompt_shares_identical_state_prefix_across_questions() {
        let state = "Bật đèn phòng khách và kiểm tra nhiệt độ hiện tại.";
        let opts_a = vec![
            DecisionOption::new("A", "An toàn"),
            DecisionOption::new("B", "Rủi ro"),
        ];
        let opts_b = vec![
            DecisionOption::new("A", "Thường (Router)"),
            DecisionOption::new("B", "Khó (Expert)"),
        ];

        let prompt_1 =
            compile_decision_prompt(state, "Yêu cầu này có an toàn không?", &opts_a).unwrap();
        let prompt_2 =
            compile_decision_prompt(state, "Độ khó của yêu cầu này là gì?", &opts_b).unwrap();

        // Phần tiền tố chứa toàn bộ lượt `State:` phải giống hệt nhau từng byte
        // để KV cache trong LlamaRouterManager tái sử dụng 100% lượt State.
        let marker = "### DECISION";
        let idx_1 = prompt_1.find(marker).expect("marker in prompt_1");
        let idx_2 = prompt_2.find(marker).expect("marker in prompt_2");
        assert_eq!(idx_1, idx_2);
        assert_eq!(&prompt_1[..idx_1], &prompt_2[..idx_2]);
        assert!(prompt_1[..idx_1].contains(state));
    }

    #[tokio::test]
    async fn resolve_model_option_tokens_with_vocab_model_and_vocab_only_guard() {
        let model_path = std::path::Path::new("../models/ggml-vocab-llama-spm.gguf");
        if !model_path.exists() {
            return;
        }

        let mut manager = super::super::LlamaRouterManager::new(128, 0).unwrap();
        manager
            .swap_model(model_path, Some(128), Some(0), Some(true))
            .await
            .expect("swap vocab-only model");

        let engine = manager.engine.as_ref().expect("engine loaded");
        let resolved =
            resolve_model_option_tokens(&engine.model, &["A", "B", "C"]).expect("resolve A, B, C");
        assert_eq!(resolved.len(), 3);
        assert_ne!(resolved[0].1, resolved[1].1);
        assert_ne!(resolved[1].1, resolved[2].1);

        // Nhãn trùng lặp phải bị chặn ngay
        let dup_err = resolve_model_option_tokens(&engine.model, &["A", "A"]);
        assert!(dup_err.is_err());

        // Guard chặn chạy decide_single_pass trên model vocab-only
        let err = manager
            .decide_single_pass("Hello", &["A", "B"])
            .expect_err("vocab-only must reject decide_single_pass");
        assert!(err.contains("vocab-only"));
    }
}
