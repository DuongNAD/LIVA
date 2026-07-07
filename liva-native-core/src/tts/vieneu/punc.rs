//! Chuẩn hóa dấu câu cuối ("punc_norm").
//!
//! Vendored from sea-g2p (`pnnbao97/sea-g2p`, Apache-2.0), `src/punc.rs`. Kept
//! verbatim so the phoneme stream terminates exactly as VieNeu-TTS was trained:
//!   - Câu "siêu ngắn" (< 5 từ) luôn kết thúc bằng đúng một dấu `.`, thay mọi
//!     dấu câu cuối đang có.
//!   - Câu dài hơn chỉ thêm `.` nếu chưa kết thúc bằng một trong `, . ! ?`.

/// Câu có số từ <= ngưỡng này được coi là "siêu ngắn" (yêu cầu: dưới 5 từ).
const SHORT_SENTENCE_MAX_WORDS: usize = 4;

/// Dấu câu cuối có thể bị bỏ/thay khi ép câu ngắn về `.`.
/// Bao gồm cả ellipsis một-ký-tự: `…` (U+2026), `‥` (U+2025), `․` (U+2024).
fn is_trailing_punct(c: char) -> bool {
    matches!(
        c,
        ',' | '.' | '!' | '?' | ';' | ':' | '\u{2026}' | '\u{2025}' | '\u{2024}'
    )
}

/// Dấu kết thúc câu được chấp nhận cho câu dài (không cần thêm `.`).
fn is_sentence_end(c: char) -> bool {
    matches!(c, ',' | '.' | '!' | '?')
}

/// Đếm số "từ" thực — chỉ tính token có ít nhất một ký tự chữ/số, để các token
/// dấu câu đứng riêng (vd "Xin chào !") không bị tính nhầm thành một từ.
fn word_count(text: &str) -> usize {
    text.split_whitespace()
        .filter(|w| w.chars().any(|c| c.is_alphanumeric()))
        .count()
}

/// Áp dụng chuẩn hóa dấu câu cuối lên `text`.
pub fn apply_punc_norm(text: &str) -> String {
    let trimmed = text.trim_end();
    if trimmed.is_empty() {
        return trimmed.to_string();
    }

    if word_count(trimmed) <= SHORT_SENTENCE_MAX_WORDS {
        // Câu siêu ngắn: ép dấu cuối về đúng một `.` bất kể đang là dấu gì.
        let stripped =
            trimmed.trim_end_matches(|c: char| is_trailing_punct(c) || c.is_whitespace());
        if stripped.is_empty() {
            // Toàn dấu câu -> trả về một dấu `.`.
            return ".".to_string();
        }
        format!("{}.", stripped)
    } else {
        // Câu dài: chỉ thêm `.` nếu chưa kết thúc bằng , . ! ?
        let last_char = trimmed.chars().next_back().unwrap();
        if is_sentence_end(last_char) {
            trimmed.to_string()
        } else {
            format!("{}.", trimmed)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::apply_punc_norm;

    #[test]
    fn long_sentence_gets_dot_when_missing() {
        assert_eq!(
            apply_punc_norm("tôi đi học mỗi ngày vào buổi sáng"),
            "tôi đi học mỗi ngày vào buổi sáng."
        );
    }

    #[test]
    fn short_sentence_forced_to_dot() {
        assert_eq!(apply_punc_norm("xin chào"), "xin chào.");
        assert_eq!(apply_punc_norm("xin chào!"), "xin chào.");
        assert_eq!(apply_punc_norm("ừ…"), "ừ.");
    }

    #[test]
    fn idempotent() {
        assert_eq!(apply_punc_norm("xin chào."), "xin chào.");
    }
}
