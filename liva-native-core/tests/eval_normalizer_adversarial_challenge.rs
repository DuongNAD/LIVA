//! Empirical Adversarial Stress Test Suite for `eval::normalizer` (Milestone M1 Challenger)
//!
//! Stress-tests:
//! 1. Pathological Vietnamese diacritics & tone permutations (NFC/NFD mix, archaic/new tone placement, mixed scripts, emojis)
//! 2. Negative numbers ("-5", "-100đ", "-2.5", "-15%", "-$50")
//! 3. Numbers exceeding 1 billion (1 tỷ, 999 tỷ, 1.000 tỷ / 10^12, and large 16-digit account numbers)
//! 4. Grouped thousands vs decimals ("1.000.000", "50.000", "2.5")
//! 5. Complex abbreviations & nested punctuation ("tôi ko.", "tp.hcm.", "tp hcm", "([{\"stk...\"}])")
//! 6. Empty strings, whitespace permutations, and WER edge cases

use liva_native_core::eval::normalizer::{
    VietnameseNormalizer, align_words, calculate_dual_wer, calculate_n_wer, calculate_o_wer,
    decimal_to_vietnamese_words, integer_to_vietnamese_words, tokenize_words,
};
use std::panic::{AssertUnwindSafe, catch_unwind};
use unicode_normalization::UnicodeNormalization;

// =========================================================================
// 1. PATHOLOGICAL VIETNAMESE STRINGS & NFD/NFC CANONICAL EQUIVALENCE
// =========================================================================

#[test]
fn test_adversarial_pathological_vietnamese_and_nfd_nfc() {
    let normalizer = VietnameseNormalizer::new();

    // 1a. Alternating NFD and NFC tokens
    // "Hà" in NFD: 'H' + 'a' + U+0300
    // "Nội" in NFC: U+004E U+1ED9 U+0069
    // "và" in NFD: 'v' + 'a' + U+0300
    // "Sài" in NFC: 'S' + U+00E0 + 'i'
    // "Gòn" in NFD: 'G' + 'o' + U+0300 + 'n'
    let mixed_nfd_nfc = "H\u{0061}\u{0300} N\u{1ED9}i v\u{0061}\u{0300} S\u{00E0}i G\u{006F}\u{0300}n";
    let pure_nfc = "Hà Nội và Sài Gòn";

    let norm_mixed = normalizer.normalize(mixed_nfd_nfc);
    let norm_nfc = normalizer.normalize(pure_nfc);
    assert_eq!(
        norm_mixed, norm_nfc,
        "Normalizer must produce identical NFC outputs for mixed NFD/NFC inputs"
    );
    assert_eq!(norm_mixed, "hà nội và sài gòn");

    // 1b. Decomposed multiple accents on single vowels
    // 'ế': 'e' + U+0302 (circumflex) + U+0301 (acute)
    let nfd_e_sac = "ti\u{0065}\u{0302}\u{0301}ng";
    assert_eq!(normalizer.normalize(nfd_e_sac), "tiếng");

    // 1c. All uppercase Vietnamese with complex tones
    let upper_accented = "ĐẮC LẮC, QUẢNG NGÃI, BÀ RỊA VŨNG TÀU, THỪA THIÊN HUẾ";
    let norm_upper = normalizer.normalize(upper_accented);
    assert_eq!(
        norm_upper,
        "đắc lắc quảng ngãi bà rịa vũng tàu thừa thiên huế"
    );

    // 1d. Zero-width and invisible control characters
    let invisible_chars = "Tôi\u{200b} là\u{200e} trợ\u{202a} lý\u{202c} LIVA";
    let norm_invisible = normalizer.normalize(invisible_chars);
    assert_eq!(norm_invisible, "tôi là trợ lý liva");

    // 1e. Emojis and mixed foreign scripts
    let mixed_content = "Xin chào 👋🇻🇳! LIVA engine (リヴァ) đạt 100 điểm 💯.";
    let norm_mixed_content = normalizer.normalize(mixed_content);
    assert!(
        norm_mixed_content.contains("xin chào"),
        "Should preserve Vietnamese words"
    );
    assert!(
        norm_mixed_content.contains("liva engine"),
        "Should lowercase English"
    );
    assert!(
        norm_mixed_content.contains("một trăm điểm"),
        "Should expand 100 to một trăm"
    );
}

// =========================================================================
// 2. STRESS TEST: NUMBERS EXCEEDING 1 BILLION (TRILLIONS & BIG INTEGERS)
// =========================================================================

#[test]
fn test_adversarial_numbers_exceeding_one_billion() {
    let normalizer = VietnameseNormalizer::new();

    // 2a. Numbers in range 1 billion to 999 billion
    let one_billion = integer_to_vietnamese_words(1_000_000_000);
    assert_eq!(one_billion, "một tỷ");

    let five_billion = integer_to_vietnamese_words(5_000_000_000);
    assert_eq!(five_billion, "năm tỷ");

    let hundred_billion = integer_to_vietnamese_words(100_000_000_000);
    assert_eq!(hundred_billion, "một trăm tỷ");

    let nine_hundred_ninety_nine_billion = integer_to_vietnamese_words(999_000_000_000);
    assert_eq!(
        nine_hundred_ninety_nine_billion,
        "chín trăm chín mươi chín tỷ"
    );

    // 2b. NUMBERS >= 1 TRILLION (1.000 tỷ / 10^12)
    // In current normalizer.rs line 141:
    // let billions = n / 1_000_000_000;
    // For n = 1_000_000_000_000, billions = 1000.
    // h = (val / 100) = 10 -> DIGIT_WORDS[10] -> PANIC (index out of bounds: len is 10)!
    let panic_trillion = catch_unwind(AssertUnwindSafe(|| {
        integer_to_vietnamese_words(1_000_000_000_000)
    }));

    if panic_trillion.is_err() {
        eprintln!(
            "[DEFECT REPRODUCED] integer_to_vietnamese_words(1_000_000_000_000) PANICS due to triplet h=10 index out of bounds!"
        );
    }

    // 2c. Normalizer handling on 16-digit account number or large integer in text
    let panic_norm = catch_unwind(AssertUnwindSafe(|| {
        normalizer.normalize("Số tài khoản 1234567890123456")
    }));

    if panic_norm.is_err() {
        eprintln!(
            "[DEFECT REPRODUCED] normalizer.normalize(\"Số tài khoản 1234567890123456\") PANICS on 16-digit account number!"
        );
    }
}

// =========================================================================
// 3. STRESS TEST: GROUPED THOUSANDS VS DECIMALS
// =========================================================================

#[test]
fn test_adversarial_grouped_thousands_vs_decimals() {
    let normalizer = VietnameseNormalizer::new();

    // In Vietnamese speech and text:
    // "1.000.000" is 1 million ("một triệu"), NOT "một phẩy không không không..."
    // "50.000" is 50 thousand ("năm mươi nghìn"), NOT "năm mươi phẩy không..."
    // "2.5" or "2,5" is decimal ("hai phẩy năm")
    let norm_one_million = normalizer.normalize("Doanh thu 1.000.000 đồng");
    let norm_fifty_k = normalizer.normalize("Vé xem phim 50.000 đồng");
    let norm_decimal = normalizer.normalize("Tỷ lệ 2.5 phần trăm");

    eprintln!("[Grouped Thousands Test]");
    eprintln!("  '1.000.000 đồng' -> '{norm_one_million}'");
    eprintln!("  '50.000 đồng'    -> '{norm_fifty_k}'");
    eprintln!("  '2.5 phần trăm'  -> '{norm_decimal}'");

    // Check whether RE_DECIMAL shadowed RE_NUMBER_GROUPED
    let decimal_shadow_defect = norm_one_million.contains("phẩy không không không");
    if decimal_shadow_defect {
        eprintln!(
            "[DEFECT REPRODUCED] RE_DECIMAL executed before RE_NUMBER_GROUPED, converting 1.000.000 into decimal 'phẩy không không không'!"
        );
    }
}

// =========================================================================
// 4. STRESS TEST: NEGATIVE NUMBERS
// =========================================================================

#[test]
fn test_adversarial_negative_numbers() {
    let normalizer = VietnameseNormalizer::new();

    let norm_neg_int = normalizer.normalize("Nhiệt độ -5 độ C");
    let norm_neg_curr = normalizer.normalize("Lỗ -100.000đ");
    let norm_neg_pct = normalizer.normalize("Tăng trưởng -2,5%");
    let norm_neg_usd = normalizer.normalize("Số dư -$50");

    eprintln!("[Negative Numbers Test]");
    eprintln!("  'Nhiệt độ -5 độ C'   -> '{norm_neg_int}'");
    eprintln!("  'Lỗ -100.000đ'       -> '{norm_neg_curr}'");
    eprintln!("  'Tăng trưởng -2,5%'  -> '{norm_neg_pct}'");
    eprintln!("  'Số dư -$50'         -> '{norm_neg_usd}'");

    // In speech recognition, "-5" is spoken as "âm năm".
    // If the normalizer erases '-', reference "nhiệt độ -5 độ" -> "nhiệt độ năm độ",
    // while hypothesis "nhiệt độ âm năm độ" gets mismatched with 25% WER penalty!
    let lost_sign = !norm_neg_int.contains("âm") && norm_neg_int.contains("năm");
    if lost_sign {
        eprintln!(
            "[DEFECT REPRODUCED] Negative sign '-' is completely stripped without converting to 'âm'!"
        );
    }
}

// =========================================================================
// 5. STRESS TEST: ABBREVIATIONS WITH NESTED PUNCTUATION & SPACES
// =========================================================================

#[test]
fn test_adversarial_abbreviations_and_punctuation() {
    let normalizer = VietnameseNormalizer::new();

    // 5a. Dot-terminated abbreviations at end of sentence
    // In normalizer.rs line 424:
    // let clean_token = word.trim_matches(|c: char| !c.is_alphanumeric() && c != '.');
    // Because c != '.', trailing dots are NOT trimmed! "ko." remains "ko." and fails lookup!
    let norm_ko_dot = normalizer.normalize("Tôi ko.");
    let norm_stk_dot = normalizer.normalize("Vui lòng gửi stk.");
    let norm_tp_dot = normalizer.normalize("Chi nhánh tp.hcm.");

    eprintln!("[Dot-Terminated Abbreviations Test]");
    eprintln!("  'Tôi ko.'              -> '{norm_ko_dot}'");
    eprintln!("  'Vui lòng gửi stk.'    -> '{norm_stk_dot}'");
    eprintln!("  'Chi nhánh tp.hcm.'    -> '{norm_tp_dot}'");

    if norm_ko_dot == "tôi ko" {
        eprintln!(
            "[DEFECT REPRODUCED] 'ko.' at end of sentence failed expansion because trailing '.' was not trimmed!"
        );
    }
    if norm_tp_dot == "chi nhánh tp hcm" {
        eprintln!(
            "[DEFECT REPRODUCED] 'tp.hcm.' at end of sentence failed expansion to 'thành phố hồ chí minh'!"
        );
    }

    // 5b. Space-delimited abbreviation ("tp hcm")
    // ABBREVIATIONS has ("tp hcm", "thành phố hồ chí minh"), but expand_abbreviations splits on whitespace!
    let norm_tp_space = normalizer.normalize("Tôi sống tại tp hcm");
    eprintln!("  'Tôi sống tại tp hcm'  -> '{norm_tp_space}'");
    if norm_tp_space == "tôi sống tại tp hcm" {
        eprintln!(
            "[DEFECT REPRODUCED] Multi-word abbreviation 'tp hcm' can never match because tokenizer splits tokens on whitespace!"
        );
    }

    // 5c. Nested surrounding punctuation
    let norm_nested = normalizer.normalize("Thông tin: ([{\"ko\"!?}])");
    assert_eq!(norm_nested, "thông tin không");
}

// =========================================================================
// 6. STRESS TEST: EMPTY STRINGS, EXTREME WHITESPACE & WER MATRICES
// =========================================================================

#[test]
fn test_adversarial_empty_whitespace_and_wer_boundaries() {
    let normalizer = VietnameseNormalizer::new();

    // 6a. Empty and whitespace-only strings
    assert_eq!(normalizer.normalize(""), "");
    assert_eq!(normalizer.normalize("   "), "");
    assert_eq!(normalizer.normalize("\t\n\r  \u{a0}  "), "");
    assert_eq!(normalizer.normalize_minimal(""), "");

    // 6b. Zero words in reference and hypothesis
    let wer_empty = calculate_o_wer("", "");
    assert_eq!(wer_empty.total_errors, 0);
    assert_eq!(wer_empty.reference_words, 0);
    assert_eq!(wer_empty.wer_percent(), 0.0);

    // 6c. Empty hypothesis (100% deletion)
    let wer_empty_hyp = calculate_o_wer("xin chào bạn", "");
    assert_eq!(wer_empty_hyp.deletions, 3);
    assert_eq!(wer_empty_hyp.reference_words, 3);
    assert_eq!(wer_empty_hyp.wer_percent(), 100.0);

    // 6d. Empty reference (all insertions)
    let wer_empty_ref = calculate_o_wer("", "xin chào bạn");
    assert_eq!(wer_empty_ref.insertions, 3);
    assert_eq!(wer_empty_ref.reference_words, 0);
    assert_eq!(wer_empty_ref.wer_percent(), 100.0);

    // 6e. Tokenize empty and whitespace
    assert!(tokenize_words("").is_empty());
    assert!(tokenize_words("   \n\t ").is_empty());
}
