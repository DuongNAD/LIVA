//! Vietnamese Text Normalizer and Word Error Rate (WER) Evaluator (Features F01, F02).
//!
//! Provides:
//! - 5-Step Normalization Pipeline:
//!   1. Unicode NFC normalization (resolving decomposed NFD accents to precomposed NFC)
//!   2. Unicode lowercasing (canonical lowercase for all Vietnamese vowels with tones)
//!   3. Domain Entity Expansion (Dates, Weekdays, Percentages, Currency, Numbers 0..u64::MAX)
//!   4. Vietnamese Abbreviation & Phonetic Slang Expansion ("ko", "đc", "vs", "stk", "tp.hcm", "tp hcm", ...)
//!   5. Punctuation Stripping & Whitespace Collapsing
//! - Word-level Levenshtein alignment computing Substitutions (S), Deletions (D), Insertions (I).
//! - Dual-metric evaluation:
//!   - Original WER (O-WER): Minimal clean (raw spelling & formatting)
//!   - Normalized WER (N-WER): Standard 5-step normalization for speech recognition benchmarks.

use regex::Regex;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;
use unicode_normalization::UnicodeNormalization;

// ============================================================================
// REGEX PATTERNS (Compiled once with LazyLock)
// ============================================================================

static RE_NEGATIVE_SIGN: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(^|[\s(\[:])[-−–]\s*(\$?\d)").expect("valid regex for negative sign")
});

static RE_DATE_DMY: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)\b(?:ngày\s+)?(\d{1,2})\/(\d{1,2})(?:\/(\d{2,4}))?\b")
        .expect("valid regex for date d/m/y")
});

static RE_WEEKDAY: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\bthứ\s*([2-7])\b").expect("valid regex for weekday"));

static RE_PERCENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(\d+(?:[.,]\d+)?)\s*%").expect("valid regex for percent"));

static RE_CURRENCY_VND: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(\d+(?:[.,]\d+)*)\s*(?:đ|₫|vnd|vnđ)\b").expect("valid regex for currency vnd")
});

static RE_CURRENCY_K: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d+)\s*k\b").expect("valid regex for k unit"));

static RE_CURRENCY_TR: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)\b(\d+)\s*tr(?:iệu)?\b").expect("valid regex for tr unit"));

static RE_CURRENCY_USD: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)(?:\$\s*(\d+(?:[.,]\d+)*)|(\d+(?:[.,]\d+)*)\s*\$)")
        .expect("valid regex for usd")
});

static RE_NUMBER_GROUPED: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"\b(?:\d{1,3}(?:\.\d{3})+|\d{1,3}(?:,\d{3}){2,})\b")
        .expect("valid regex for grouped thousands")
});

static RE_DECIMAL: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b(\d+)[,.](\d+)\b").expect("valid regex for decimal numbers"));

static RE_INTEGER: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\b\d+\b").expect("valid regex for standalone integers"));

static RE_MULTI_WHITESPACE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"\s+").expect("valid regex for whitespace collapsing"));

// ============================================================================
// ABBREVIATIONS & PHONETIC SLANG DICTIONARY
// ============================================================================

/// Canonical mapping from Vietnamese colloquial, shorthand, and acronym terms to spoken words.
const ABBREVIATIONS: &[(&str, &str)] = &[
    // Multi-token phrases first
    ("tp.hcm", "thành phố hồ chí minh"),
    ("tphcm", "thành phố hồ chí minh"),
    ("tp hcm", "thành phố hồ chí minh"),
    ("tt.hcm", "thành phố hồ chí minh"),
    ("stk", "số tài khoản"),
    ("sđt", "số điện thoại"),
    ("sdt", "số điện thoại"),
    ("cccd", "căn cước công dân"),
    ("cmnd", "chứng minh nhân dân"),
    ("cntt", "công nghệ thông tin"),
    // Weekday shorthand
    ("t2", "thứ hai"),
    ("t3", "thứ ba"),
    ("t4", "thứ tư"),
    ("t5", "thứ năm"),
    ("t6", "thứ sáu"),
    ("t7", "thứ bảy"),
    ("cn", "chủ nhật"),
    // Common chat & phonetic slang
    ("ko", "không"),
    ("k", "không"),
    ("kh", "không"),
    ("hong", "không"),
    ("hông", "không"),
    ("đc", "được"),
    ("dc", "được"),
    ("vs", "với"),
    ("hn", "hà nội"),
    ("đn", "đà nẵng"),
    ("hp", "hải phòng"),
    ("ct", "cần thơ"),
    ("tw", "trung ương"),
    ("bs", "bác sĩ"),
    ("ai", "trí tuệ nhân tạo"),
    ("bth", "bình thường"),
    ("bt", "bình thường"),
    ("tks", "cảm ơn"),
    ("tkx", "cảm ơn"),
    ("thanks", "cảm ơn"),
    ("j", "gì"),
    ("ntn", "như thế nào"),
    ("nv", "nhân viên"),
    ("ql", "quản lý"),
    ("pt", "phát triển"),
    ("oto", "ô tô"),
    ("ok", "đồng ý"),
];

// ============================================================================
// VIETNAMESE NUMBER-TO-WORDS CONVERTER
// ============================================================================

const DIGIT_WORDS: [&str; 10] = [
    "không", "một", "hai", "ba", "bốn", "năm", "sáu", "bảy", "tám", "chín",
];

/// Converts an unsigned integer (supporting arbitrary scale up to u64::MAX) into spoken Vietnamese words.
pub fn integer_to_vietnamese_words(n: u64) -> String {
    if n == 0 {
        return "không".to_string();
    }
    format_number_recursive(n, true)
}

fn format_number_recursive(n: u64, is_highest: bool) -> String {
    if n >= 1_000_000_000 {
        let billions = n / 1_000_000_000;
        let remainder = n % 1_000_000_000;
        let billions_words = format_number_recursive(billions, is_highest);
        if remainder == 0 {
            format!("{billions_words} tỷ")
        } else {
            let remainder_words = format_sub_billion(remainder, false);
            if remainder_words.is_empty() {
                format!("{billions_words} tỷ")
            } else {
                format!("{billions_words} tỷ {remainder_words}")
            }
        }
    } else if n == 0 && is_highest {
        "không".to_string()
    } else {
        format_sub_billion(n, is_highest)
    }
}

fn format_sub_billion(n: u64, mut is_highest: bool) -> String {
    let mut words = Vec::new();
    let millions = n / 1_000_000;
    let thousands = (n % 1_000_000) / 1_000;
    let units = n % 1_000;

    let groups = [(millions, "triệu"), (thousands, "nghìn"), (units, "")];

    for &(val, scale) in &groups {
        if val > 0 {
            let h = (val / 100) as u32;
            let t = ((val % 100) / 10) as u32;
            let u = (val % 10) as u32;

            append_triplet_words(&mut words, h, t, u, is_highest);
            if !scale.is_empty() {
                words.push(scale);
            }
            is_highest = false;
        }
    }

    words.join(" ")
}

fn append_triplet_words(words: &mut Vec<&'static str>, h: u32, t: u32, u: u32, is_highest: bool) {
    if h > 0 {
        words.push(DIGIT_WORDS[h as usize]);
        words.push("trăm");
    } else if !is_highest {
        words.push("không");
        words.push("trăm");
    }

    if t == 0 && u > 0 {
        if h > 0 || !is_highest {
            words.push("lẻ");
        }
        words.push(DIGIT_WORDS[u as usize]);
    } else if t == 1 {
        words.push("mười");
        match u {
            0 => {}
            1 => words.push("một"),
            5 => words.push("lăm"),
            _ => words.push(DIGIT_WORDS[u as usize]),
        }
    } else if t > 1 {
        words.push(DIGIT_WORDS[t as usize]);
        words.push("mươi");
        match u {
            0 => {}
            1 => words.push("mốt"),
            4 => words.push("tư"),
            5 => words.push("lăm"),
            _ => words.push(DIGIT_WORDS[u as usize]),
        }
    }
}

/// Converts a sequence of digits into digit-by-digit spoken Vietnamese words.
fn digits_to_vietnamese_words(s: &str) -> String {
    s.chars()
        .filter_map(|c| c.to_digit(10))
        .map(|d| DIGIT_WORDS[d as usize])
        .collect::<Vec<_>>()
        .join(" ")
}

/// Converts a decimal number string (e.g., "2.5" or "10,25") into spoken Vietnamese words.
pub fn decimal_to_vietnamese_words(int_part_str: &str, frac_part_str: &str) -> String {
    let int_val: u64 = int_part_str.parse().unwrap_or(0);
    let int_words = integer_to_vietnamese_words(int_val);

    // Read fractional part
    let frac_words = if frac_part_str.starts_with('0') {
        // Read digit by digit if leading zero: 2.05 -> hai phẩy không năm
        frac_part_str
            .chars()
            .filter_map(|c| c.to_digit(10))
            .map(|d| DIGIT_WORDS[d as usize])
            .collect::<Vec<_>>()
            .join(" ")
    } else if let Ok(frac_val) = frac_part_str.parse::<u64>() {
        integer_to_vietnamese_words(frac_val)
    } else {
        frac_part_str.to_string()
    };

    format!("{int_words} phẩy {frac_words}")
}

// ============================================================================
// VIETNAMESE NORMALIZER ENGINE
// ============================================================================

/// Vietnamese Text Normalizer providing both full 5-step normalization and minimal normalization.
#[derive(Debug, Default, Clone)]
pub struct VietnameseNormalizer;

impl VietnameseNormalizer {
    /// Creates a new normalizer instance.
    pub fn new() -> Self {
        Self
    }

    /// Full 5-step normalization pipeline for Speech-to-Text evaluation (N-WER).
    ///
    /// Pipeline steps:
    /// 1. Unicode NFC Normalization
    /// 2. Unicode Lowercase
    /// 3. Domain Entity Expansion (Numbers, Dates, Currencies, Percentages)
    /// 4. Vietnamese Abbreviations & Phonetic Slang Expansion
    /// 5. Punctuation Stripping & Whitespace Collapsing
    pub fn normalize(&self, text: &str) -> String {
        // Step 1: Unicode NFC Normalization
        let nfc_text: String = text.nfc().collect();

        // Step 2: Unicode Lowercase
        let lower_text = nfc_text.to_lowercase();

        // Step 3: Domain Entity Expansion (Numbers, Dates, Currencies, Percentages)
        let expanded = self.expand_domain_entities(&lower_text);

        // Step 4: Vietnamese Abbreviations & Phonetic Slang Expansion
        let slang_expanded = self.expand_abbreviations(&expanded);

        // Step 5: Punctuation Stripping & Whitespace Collapsing
        self.strip_punctuation_and_collapse(&slang_expanded)
    }

    /// Minimal normalization for baseline Original Word Error Rate (O-WER).
    /// Applies NFC, lowercase, and basic non-alphanumeric punctuation stripping,
    /// without verbalizing numbers or slang.
    pub fn normalize_minimal(&self, text: &str) -> String {
        let nfc_text: String = text.nfc().collect();
        let lower_text = nfc_text.to_lowercase();
        self.strip_punctuation_and_collapse(&lower_text)
    }

    /// Step 3: Expand domain entities (Dates, Weekdays, Percent, Currency, Decimals, Integers).
    fn expand_domain_entities(&self, text: &str) -> String {
        let mut result = text.to_string();

        // 3.0. Negative signs: "-5" -> "âm 5", "-$50" -> "âm $50", "-100.000đ" -> "âm 100.000đ"
        result = RE_NEGATIVE_SIGN
            .replace_all(&result, |caps: &regex::Captures| {
                let prefix = &caps[1];
                let rest = &caps[2];
                format!("{prefix}âm {rest}")
            })
            .to_string();

        // 3a. Weekdays: "thứ 2" -> "thứ hai"
        result = RE_WEEKDAY
            .replace_all(&result, |caps: &regex::Captures| {
                let day = &caps[1];
                let word = match day {
                    "2" => "hai",
                    "3" => "ba",
                    "4" => "tư",
                    "5" => "năm",
                    "6" => "sáu",
                    "7" => "bảy",
                    _ => day,
                };
                format!("thứ {word}")
            })
            .to_string();

        // 3b. Dates: 15/4 or 15/04/2026 -> mười lăm tháng tư năm hai nghìn không trăm hai mươi sáu
        result = RE_DATE_DMY
            .replace_all(&result, |caps: &regex::Captures| {
                let day: u64 = caps[1].parse().unwrap_or(0);
                let month: u64 = caps[2].parse().unwrap_or(0);
                let day_words = integer_to_vietnamese_words(day);
                let month_words = match month {
                    4 => "tư".to_string(),
                    _ => integer_to_vietnamese_words(month),
                };

                let mut out = format!("ngày {day_words} tháng {month_words}");
                if let Some(year_match) = caps.get(3) {
                    let year: u64 = year_match.as_str().parse().unwrap_or(0);
                    let year_words = integer_to_vietnamese_words(year);
                    out.push_str(&format!(" năm {year_words}"));
                }
                out
            })
            .to_string();

        // 3c. Percentages: 5% -> năm phần trăm, 2,5% -> hai phẩy năm phần trăm
        result = RE_PERCENT
            .replace_all(&result, |caps: &regex::Captures| {
                let num_str = &caps[1];
                let num_words = self.parse_and_read_number(num_str);
                format!("{num_words} phần trăm")
            })
            .to_string();

        // 3d. Currency VND: 100.000đ / 100.000 vnđ -> một trăm nghìn đồng
        result = RE_CURRENCY_VND
            .replace_all(&result, |caps: &regex::Captures| {
                let raw_num = &caps[1].replace(['.', ','], "");
                let num: u64 = raw_num.parse().unwrap_or(0);
                let words = integer_to_vietnamese_words(num);
                format!("{words} đồng")
            })
            .to_string();

        // 3e. Currency shorthand: 50k -> năm mươi nghìn
        result = RE_CURRENCY_K
            .replace_all(&result, |caps: &regex::Captures| {
                let num: u64 = caps[1].parse().unwrap_or(0);
                let total = num * 1_000;
                integer_to_vietnamese_words(total)
            })
            .to_string();

        // 3f. Currency shorthand: 10tr -> mười triệu
        result = RE_CURRENCY_TR
            .replace_all(&result, |caps: &regex::Captures| {
                let num: u64 = caps[1].parse().unwrap_or(0);
                let words = integer_to_vietnamese_words(num);
                format!("{words} triệu")
            })
            .to_string();

        // 3g. Currency USD: $50 / 50$ -> năm mươi đô la
        result = RE_CURRENCY_USD
            .replace_all(&result, |caps: &regex::Captures| {
                let raw = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .map_or("", |m| m.as_str());
                let clean = raw.replace(['.', ','], "");
                let num: u64 = clean.parse().unwrap_or(0);
                let words = integer_to_vietnamese_words(num);
                format!("{words} đô la")
            })
            .to_string();

        // 3h. Grouped thousand integers (MUST PRECEDE DECIMALS!): 1.000.000 -> một triệu
        result = RE_NUMBER_GROUPED
            .replace_all(&result, |caps: &regex::Captures| {
                let clean = caps[0].replace(['.', ','], "");
                if let Ok(num) = clean.parse::<u64>() {
                    integer_to_vietnamese_words(num)
                } else {
                    caps[0].to_string()
                }
            })
            .to_string();

        // 3i. Decimals: 2.5 / 2,5 -> hai phẩy năm
        result = RE_DECIMAL
            .replace_all(&result, |caps: &regex::Captures| {
                decimal_to_vietnamese_words(&caps[1], &caps[2])
            })
            .to_string();

        // 3j. Plain integers: 120 -> một trăm hai mươi, phone numbers, large STK
        result = RE_INTEGER
            .replace_all(&result, |caps: &regex::Captures| {
                let s = &caps[0];
                if s.starts_with('0') && s.len() > 1 {
                    digits_to_vietnamese_words(s)
                } else if let Ok(num) = s.parse::<u64>() {
                    integer_to_vietnamese_words(num)
                } else {
                    digits_to_vietnamese_words(s)
                }
            })
            .to_string();

        result
    }

    /// Parses and reads an integer or decimal string into Vietnamese words.
    fn parse_and_read_number(&self, s: &str) -> String {
        if let Some((int_part, frac_part)) = s.split_once(['.', ',']) {
            decimal_to_vietnamese_words(int_part, frac_part)
        } else if let Ok(val) = s.parse::<u64>() {
            integer_to_vietnamese_words(val)
        } else {
            s.to_string()
        }
    }

    /// Step 4: Expand abbreviations and phonetic slang.
    fn expand_abbreviations(&self, text: &str) -> String {
        let raw_tokens: Vec<&str> = text.split_whitespace().collect();
        let mut out: Vec<String> = Vec::with_capacity(raw_tokens.len());
        let mut i = 0;

        while i < raw_tokens.len() {
            // Try 2-token phrase first (e.g. "tp hcm")
            if i + 1 < raw_tokens.len() {
                let t1 = raw_tokens[i].trim_matches(|c: char| !c.is_alphanumeric());
                let t2 = raw_tokens[i + 1].trim_matches(|c: char| !c.is_alphanumeric());
                let phrase = format!("{t1} {t2}");

                let mut matched_expansion = None;
                for &(abbr, expansion) in ABBREVIATIONS {
                    if abbr == phrase {
                        matched_expansion = Some(expansion);
                        break;
                    }
                }

                if let Some(exp) = matched_expansion {
                    out.push(exp.to_string());
                    i += 2;
                    continue;
                }
            }

            // Fallback to single token (e.g. "ko.", "stk.", "tp.hcm.")
            let t1 = raw_tokens[i].trim_matches(|c: char| !c.is_alphanumeric());
            let mut matched_expansion = None;
            for &(abbr, expansion) in ABBREVIATIONS {
                if abbr == t1 {
                    matched_expansion = Some(expansion);
                    break;
                }
            }

            if let Some(exp) = matched_expansion {
                out.push(exp.to_string());
            } else {
                out.push(raw_tokens[i].to_string());
            }
            i += 1;
        }

        out.join(" ")
    }

    /// Step 5: Strip remaining punctuation, keep Vietnamese alphanumeric and whitespace, collapse spaces.
    fn strip_punctuation_and_collapse(&self, text: &str) -> String {
        let filtered: String = text
            .chars()
            .map(|ch| {
                if ch.is_alphanumeric() || ch.is_whitespace() {
                    ch
                } else {
                    ' '
                }
            })
            .collect();

        RE_MULTI_WHITESPACE
            .replace_all(filtered.trim(), " ")
            .to_string()
    }
}

// ============================================================================
// WORD ERROR RATE (WER) CALCULATOR & LEVENSHTEIN ALIGNMENT
// ============================================================================

/// Mode of WER evaluation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WerMode {
    /// Original WER: Minimal normalization (Unicode NFC, lowercase, basic punctuation removal).
    Original,
    /// Normalized WER: Full 5-step Vietnamese normalization pipeline.
    Normalized,
}

/// Detailed Word Error Rate breakdown for an utterance or aggregated dataset.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct WerResult {
    /// Number of substituted words (thay thế)
    pub substitutions: usize,
    /// Number of deleted words (xóa / thiếu từ so với reference)
    pub deletions: usize,
    /// Number of inserted words (chèn / thừa từ so với reference)
    pub insertions: usize,
    /// Total number of words in reference transcript
    pub reference_words: usize,
    /// Total word errors (S + D + I)
    pub total_errors: usize,
}

impl WerResult {
    /// Calculates Word Error Rate as a percentage (0.0 to 100.0+).
    pub fn wer_percent(&self) -> f64 {
        if self.reference_words == 0 {
            if self.total_errors == 0 { 0.0 } else { 100.0 }
        } else {
            (self.total_errors as f64 / self.reference_words as f64) * 100.0
        }
    }

    /// Calculates Word Error Rate as a ratio (0.0 to 1.0+).
    pub fn wer_ratio(&self) -> f64 {
        self.wer_percent() / 100.0
    }

    /// Micro-averages error counts into an aggregate accumulator.
    pub fn add(&mut self, other: &Self) {
        self.substitutions += other.substitutions;
        self.deletions += other.deletions;
        self.insertions += other.insertions;
        self.reference_words += other.reference_words;
        self.total_errors = self.substitutions + self.deletions + self.insertions;
    }
}

/// Dual Word Error Rate report containing both O-WER and N-WER.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DualWerScore {
    pub o_wer: WerResult,
    pub n_wer: WerResult,
}

impl DualWerScore {
    pub fn o_wer_percent(&self) -> f64 {
        self.o_wer.wer_percent()
    }

    pub fn n_wer_percent(&self) -> f64 {
        self.n_wer.wer_percent()
    }

    /// Difference between O-WER and N-WER (formatting / normalization gap).
    pub fn normalization_gap_percent(&self) -> f64 {
        self.o_wer_percent() - self.n_wer_percent()
    }
}

/// Aligns two word sequences using Levenshtein distance dynamic programming.
pub fn align_words(reference: &[String], hypothesis: &[String]) -> WerResult {
    let n = reference.len();
    let m = hypothesis.len();

    if n == 0 {
        return WerResult {
            substitutions: 0,
            deletions: 0,
            insertions: m,
            reference_words: 0,
            total_errors: m,
        };
    }

    let mut cost = vec![vec![0usize; m + 1]; n + 1];

    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    for (j, cell) in cost[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            if reference[i - 1] == hypothesis[j - 1] {
                cost[i][j] = cost[i - 1][j - 1];
            } else {
                cost[i][j] = 1 + cost[i - 1][j - 1].min(cost[i - 1][j]).min(cost[i][j - 1]);
            }
        }
    }

    // Backtrack to count substitutions, deletions, insertions
    let (mut i, mut j) = (n, m);
    let mut substitutions = 0;
    let mut deletions = 0;
    let mut insertions = 0;

    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && reference[i - 1] == hypothesis[j - 1]
            && cost[i][j] == cost[i - 1][j - 1]
        {
            i -= 1;
            j -= 1;
        } else if i > 0 && j > 0 && cost[i][j] == cost[i - 1][j - 1] + 1 {
            substitutions += 1;
            i -= 1;
            j -= 1;
        } else if i > 0 && cost[i][j] == cost[i - 1][j] + 1 {
            deletions += 1;
            i -= 1;
        } else {
            insertions += 1;
            j -= 1;
        }
    }

    WerResult {
        substitutions,
        deletions,
        insertions,
        reference_words: n,
        total_errors: substitutions + deletions + insertions,
    }
}

/// Splits normalized text into whitespace-delimited tokens.
pub fn tokenize_words(text: &str) -> Vec<String> {
    text.split_whitespace().map(ToOwned::to_owned).collect()
}

/// Computes Word Error Rate between reference and hypothesis under the specified mode.
pub fn calculate_wer(reference: &str, hypothesis: &str, mode: WerMode) -> WerResult {
    let normalizer = VietnameseNormalizer::new();
    let (norm_ref, norm_hyp) = match mode {
        WerMode::Original => (
            normalizer.normalize_minimal(reference),
            normalizer.normalize_minimal(hypothesis),
        ),
        WerMode::Normalized => (
            normalizer.normalize(reference),
            normalizer.normalize(hypothesis),
        ),
    };

    let ref_words = tokenize_words(&norm_ref);
    let hyp_words = tokenize_words(&norm_hyp);

    align_words(&ref_words, &hyp_words)
}

/// Computes Original Word Error Rate (O-WER).
pub fn calculate_o_wer(reference: &str, hypothesis: &str) -> WerResult {
    calculate_wer(reference, hypothesis, WerMode::Original)
}

/// Computes Normalized Word Error Rate (N-WER).
pub fn calculate_n_wer(reference: &str, hypothesis: &str) -> WerResult {
    calculate_wer(reference, hypothesis, WerMode::Normalized)
}

/// Computes both O-WER and N-WER simultaneously.
pub fn calculate_dual_wer(reference: &str, hypothesis: &str) -> DualWerScore {
    DualWerScore {
        o_wer: calculate_o_wer(reference, hypothesis),
        n_wer: calculate_n_wer(reference, hypothesis),
    }
}

// ============================================================================
// UNIT TESTS FOR NORMALIZER & WER EVALUATOR
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_step1_unicode_nfc_normalization() {
        let normalizer = VietnameseNormalizer::new();
        // Decomposed NFD 'ế' (e + U+0302 + U+0301) and 'ệ' (e + U+0302 + U+0323) vs Precomposed NFC 'tiếng việt'
        let decomposed = "ti\u{0065}\u{0302}\u{0301}ng vi\u{0065}\u{0302}\u{0323}t";
        let precomposed = "tiếng việt";

        assert_eq!(normalizer.normalize(decomposed), "tiếng việt");
        assert_eq!(normalizer.normalize(precomposed), "tiếng việt");
        assert_eq!(
            normalizer.normalize(decomposed),
            normalizer.normalize(precomposed)
        );
    }

    #[test]
    fn test_step2_lowercase_vietnamese_diacritics() {
        let normalizer = VietnameseNormalizer::new();
        let upper = "HÀ NỘI ĐÀ NẴNG THÀNH PHỐ HỒ CHÍ MINH";
        assert_eq!(
            normalizer.normalize(upper),
            "hà nội đà nẵng thành phố hồ chí minh"
        );
    }

    #[test]
    fn test_step3_punctuation_stripping() {
        let normalizer = VietnameseNormalizer::new();
        let punctuated = "Xin chào, tôi là trợ lý LIVA! Bạn có khỏe không?";
        assert_eq!(
            normalizer.normalize(punctuated),
            "xin chào tôi là trợ lý liva bạn có khỏe không"
        );
    }

    #[test]
    fn test_step4_number_to_words_integers() {
        assert_eq!(integer_to_vietnamese_words(0), "không");
        assert_eq!(integer_to_vietnamese_words(1), "một");
        assert_eq!(integer_to_vietnamese_words(5), "năm");
        assert_eq!(integer_to_vietnamese_words(10), "mười");
        assert_eq!(integer_to_vietnamese_words(11), "mười một");
        assert_eq!(integer_to_vietnamese_words(14), "mười bốn");
        assert_eq!(integer_to_vietnamese_words(15), "mười lăm");
        assert_eq!(integer_to_vietnamese_words(21), "hai mươi mốt");
        assert_eq!(integer_to_vietnamese_words(24), "hai mươi tư");
        assert_eq!(integer_to_vietnamese_words(25), "hai mươi lăm");
        assert_eq!(integer_to_vietnamese_words(100), "một trăm");
        assert_eq!(integer_to_vietnamese_words(101), "một trăm lẻ một");
        assert_eq!(integer_to_vietnamese_words(105), "một trăm lẻ năm");
        assert_eq!(integer_to_vietnamese_words(115), "một trăm mười lăm");
        assert_eq!(integer_to_vietnamese_words(125), "một trăm hai mươi lăm");
        assert_eq!(
            integer_to_vietnamese_words(1005),
            "một nghìn không trăm lẻ năm"
        );
        assert_eq!(integer_to_vietnamese_words(20000), "hai mươi nghìn");
        assert_eq!(
            integer_to_vietnamese_words(1500000),
            "một triệu năm trăm nghìn"
        );
        assert_eq!(
            integer_to_vietnamese_words(999000000),
            "chín trăm chín mươi chín triệu"
        );
        assert_eq!(
            integer_to_vietnamese_words(999999999),
            "chín trăm chín mươi chín triệu chín trăm chín mươi chín nghìn chín trăm chín mươi chín"
        );
        // Numbers >= 10^9 and >= 10^12
        assert_eq!(integer_to_vietnamese_words(1_000_000_000), "một tỷ");
        assert_eq!(
            integer_to_vietnamese_words(1_000_000_000_000),
            "một nghìn tỷ"
        );
    }

    #[test]
    fn test_step4_percent_currency_and_dates() {
        let normalizer = VietnameseNormalizer::new();

        // Percent
        assert_eq!(
            normalizer.normalize("lãi suất 5%"),
            "lãi suất năm phần trăm"
        );
        assert_eq!(
            normalizer.normalize("tăng 2,5%"),
            "tăng hai phẩy năm phần trăm"
        );

        // Currency VND
        assert_eq!(
            normalizer.normalize("giá 100.000đ"),
            "giá một trăm nghìn đồng"
        );
        assert_eq!(
            normalizer.normalize("tiền phòng 50k"),
            "tiền phòng năm mươi nghìn"
        );
        assert_eq!(normalizer.normalize("chi phí 10tr"), "chi phí mười triệu");

        // Currency USD
        assert_eq!(
            normalizer.normalize("học phí $50"),
            "học phí năm mươi đô la"
        );

        // Dates & Weekdays
        assert_eq!(
            normalizer.normalize("họp vào ngày 15/4"),
            "họp vào ngày mười lăm tháng tư"
        );
        assert_eq!(normalizer.normalize("gặp vào thứ 2"), "gặp vào thứ hai");
        assert_eq!(normalizer.normalize("lịch hẹn t6"), "lịch hẹn thứ sáu");
    }

    #[test]
    fn test_step5_abbreviations_and_slang() {
        let normalizer = VietnameseNormalizer::new();
        assert_eq!(
            normalizer.normalize("hôm nay ko đi học đc vs bạn"),
            "hôm nay không đi học được với bạn"
        );
        assert_eq!(
            normalizer.normalize("gửi stk tại tp.hcm"),
            "gửi số tài khoản tại thành phố hồ chí minh"
        );
        assert_eq!(
            normalizer.normalize("trụ sở ở hn và cntt"),
            "trụ sở ở hà nội và công nghệ thông tin"
        );
    }

    #[test]
    fn test_adversarial_remediation_suite() {
        let normalizer = VietnameseNormalizer::new();

        // 1. Numbers >= 10^12 and long STK
        assert_eq!(
            integer_to_vietnamese_words(1_000_000_000_000),
            "một nghìn tỷ"
        );
        assert_eq!(
            integer_to_vietnamese_words(1_500_000_000_000),
            "một nghìn năm trăm tỷ"
        );
        let norm_stk = normalizer.normalize("Số tài khoản 1234567890123456");
        assert!(!norm_stk.is_empty());
        assert!(!norm_stk.contains("1234567890123456"));

        // 2. Grouped thousands vs decimals
        assert_eq!(
            normalizer.normalize("Doanh thu 1.000.000 đồng"),
            "doanh thu một triệu đồng"
        );
        assert_eq!(
            normalizer.normalize("Vé xem phim 50.000 đồng"),
            "vé xem phim năm mươi nghìn đồng"
        );
        assert_eq!(
            normalizer.normalize("Tỷ lệ 2.5 phần trăm"),
            "tỷ lệ hai phẩy năm phần trăm"
        );

        // 3. Negative numbers
        assert_eq!(
            normalizer.normalize("Nhiệt độ -5 độ C"),
            "nhiệt độ âm năm độ c"
        );
        assert_eq!(
            normalizer.normalize("Lỗ -100.000đ"),
            "lỗ âm một trăm nghìn đồng"
        );
        assert_eq!(
            normalizer.normalize("Tăng trưởng -2,5%"),
            "tăng trưởng âm hai phẩy năm phần trăm"
        );
        assert_eq!(
            normalizer.normalize("Số dư -$50"),
            "số dư âm năm mươi đô la"
        );

        // 4. Dot-terminated abbreviations
        assert_eq!(normalizer.normalize("Tôi ko."), "tôi không");
        assert_eq!(
            normalizer.normalize("Vui lòng gửi stk."),
            "vui lòng gửi số tài khoản"
        );
        assert_eq!(
            normalizer.normalize("Chi nhánh tp.hcm."),
            "chi nhánh thành phố hồ chí minh"
        );

        // 5. Multi-word abbreviation
        assert_eq!(
            normalizer.normalize("Tôi sống tại tp hcm"),
            "tôi sống tại thành phố hồ chí minh"
        );
    }

    #[test]
    fn test_wer_levenshtein_alignment() {
        // Exact match
        let res = calculate_o_wer("xin chào trợ lý liva", "xin chào trợ lý liva");
        assert_eq!(res.total_errors, 0);
        assert_eq!(res.wer_percent(), 0.0);

        // Substitution
        let res_sub = calculate_o_wer("xin chào", "xin vĩnh");
        assert_eq!(res_sub.substitutions, 1);
        assert_eq!(res_sub.reference_words, 2);
        assert_eq!(res_sub.wer_percent(), 50.0);

        // Deletion
        let res_del = calculate_o_wer("xin chào bạn", "xin chào");
        assert_eq!(res_del.deletions, 1);
        assert_eq!(res_del.reference_words, 3);
        assert!((res_del.wer_percent() - 33.333).abs() < 0.01);

        // Insertion
        let res_ins = calculate_o_wer("xin chào", "xin chào bạn");
        assert_eq!(res_ins.insertions, 1);
        assert_eq!(res_ins.reference_words, 2);
        assert_eq!(res_ins.wer_percent(), 50.0);

        // Empty hypothesis
        let res_empty = calculate_o_wer("xin chào", "");
        assert_eq!(res_empty.deletions, 2);
        assert_eq!(res_empty.wer_percent(), 100.0);
    }

    #[test]
    fn test_dual_wer_comparison_gap() {
        let reference = "Lãi suất 5% tại TP.HCM vào thứ 2 ko đổi.";
        let hypothesis = "lãi suất năm phần trăm tại thành phố hồ chí minh vào thứ hai không đổi";

        let score = calculate_dual_wer(reference, hypothesis);

        // O-WER is heavily penalized due to surface mismatches
        assert!(score.o_wer.wer_percent() > 50.0);

        // N-WER normalizes both to identical word sequences -> 0% error!
        assert_eq!(score.n_wer.total_errors, 0);
        assert_eq!(score.n_wer.wer_percent(), 0.0);

        // Significant normalization gap demonstrated
        assert!(score.normalization_gap_percent() > 50.0);
    }
}
