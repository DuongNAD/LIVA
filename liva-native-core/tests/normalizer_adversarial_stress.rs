//! Comprehensive Adversarial Stress Test Suite for Vietnamese Text Normalizer (Milestone M2 / Challenger 2)
//!
//! Stress-tests all normalization categories and verifies:
//! 1. Zero panics on arbitrary garbage inputs, control characters, emojis, and 100KB payloads.
//! 2. Thousands-separator correctness (`1.000` -> "một nghìn", not decimal).
//! 3. Sub-50µs execution latency budget across typical conversational turns.
//! 4. Proper rule ordering and handling of mixed alphanumeric, dates, times, currencies, and abbreviations.

use liva_native_core::tts::normalizer::{normalize, normalize_vi};
use std::time::{Duration, Instant};

// =========================================================================
// 1. GARBAGE INPUTS, CONTROL CHARS & BOUNDARY SAFETY (ZERO PANIC)
// =========================================================================

#[test]
fn test_normalizer_garbage_inputs_zero_panic() {
    let pathological_dots = format!("1{}", ".000".repeat(5_000));
    let test_inputs: Vec<(&str, &str)> = vec![
        ("empty_string", ""),
        ("spaces_tabs_newlines", "   \t\n\r\u{a0} \u{200b} "),
        ("pure_emojis", "🙂🙃🎉👍🏽🇻🇳❤️‍🔥👨‍👩‍👧‍👦"),
        (
            "control_chars",
            "\u{0}\u{1}\u{7}\u{8}\u{1b}[31m\u{7f}\u{1f}",
        ),
        ("pure_punctuation", "!!!???...,,,;;;:::---...///()[]{}"),
        ("bidi_override", "\u{202e}gnud iờn \u{202d}thuận"),
        ("unicode_replacement", "\u{fffd}\u{fffd}\u{fffd}\u{fffd}"),
        (
            "decomposed_diacritics",
            "e\u{301}\u{323}\u{300}\u{302} a\u{303} u\u{31b}",
        ),
        (
            "mixed_scripts",
            "Привет мир, مرحبا, こんにちは, สวัสดี, Xin chào 123",
        ),
        ("unclosed_regex_chars", "([{\\^$|?*+.)]}?*+"),
        (
            "long_integer",
            "99999999999999999999999999999999999999999999999999",
        ),
        ("nested_pathological_dots", pathological_dots.as_str()),
        ("malformed_date", "99/99/9999 lúc 99:99:99"),
        (
            "negative_money_and_percent",
            "-1.000.000 đồng, $-50 và -15%",
        ),
        ("stuck_abbreviations", "TP.HCM.TS.PGS.v.v.Q.1P.5"),
        ("broken_phone", "0912345678901234567890"),
        ("math_formula", "f(x) = x^2 + 2x + 1/2 với x = 5/3"),
    ];

    for (name, input) in test_inputs {
        for lang in ["vi", "en", "en-US", "", "  VI  ", "unknown_lang"] {
            let result = normalize(input, lang);
            assert!(
                result.len() <= input.len().saturating_mul(64) + 4096,
                "Case {name} (lang={lang:?}) produced excessive memory expansion: {} bytes from {} bytes",
                result.len(),
                input.len()
            );
        }
    }
}

// =========================================================================
// 2. 100KB PAYLOAD STRESS TEST
// =========================================================================

#[test]
fn test_normalizer_100kb_payload_stress() {
    let chunk = "Xin chào, hôm nay ngày 25/12/2026 lúc 10:30 tại Q.1 TP.HCM, giá 2.500.000 đồng giảm 10%, gọi 0912345678. ";
    let payload_100kb = chunk.repeat(1_000); // ~110 KB
    assert!(payload_100kb.len() > 100_000);

    let t0 = Instant::now();
    let normalized = normalize_vi(&payload_100kb);
    let elapsed = t0.elapsed();

    println!(
        "[Normalizer 100KB Payload] Input size: {} bytes, Output size: {} bytes, Time: {:?}",
        payload_100kb.len(),
        normalized.len(),
        elapsed
    );

    // Assert completed in under 200ms for 100KB.
    //
    // Debug builds are unoptimized; the strict 200ms number is the release contract, measured
    // at ~64ms. The debug factor is 10, not 5: at 5 the debug bound is 1000ms against a
    // measured 838-906ms, which is 10-19% of headroom, and it duly failed once the other test
    // binaries ran alongside this one. 10 puts the debug bound at 2000ms, ~2.2x the measured
    // value, which is what the rest of these suites use.
    const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
    assert!(
        elapsed < Duration::from_millis(200) * SLOWDOWN,
        "100KB normalization took {:?} (>200ms limit)",
        elapsed
    );
    assert!(!normalized.is_empty());
}

// =========================================================================
// 3. THOUSANDS SEPARATOR VS DECIMAL & VERSION STRINGS
// =========================================================================

#[test]
fn test_normalizer_thousands_vs_decimal_correctness() {
    // Vietnamese standard: dot (.) is thousands separator, comma (,) is decimal
    assert_eq!(normalize_vi("1.000"), "một nghìn");
    assert_eq!(normalize_vi("2.500.000"), "hai triệu năm trăm nghìn");
    assert_eq!(normalize_vi("1.000.000"), "một triệu");
    assert_ne!(
        normalize_vi("1.000"),
        "một phẩy không không không",
        "Thousands separator must not be parsed as decimal!"
    );

    // Decimal comma
    assert_eq!(normalize_vi("3,5"), "ba phẩy năm");
    assert_eq!(normalize_vi("3,14"), "ba phẩy một bốn");
    assert_eq!(normalize_vi("0,05"), "không phẩy không năm");
    assert_eq!(
        normalize_vi("1.234,5"),
        "một nghìn hai trăm ba mươi tư phẩy năm"
    );

    // Version numbers with invalid grouping dots
    assert_eq!(normalize_vi("3.14.1"), "ba chấm mười bốn chấm một");
    assert_eq!(
        normalize_vi("phiên bản 1.0.0"),
        "phiên bản một chấm không chấm không"
    );
}

// =========================================================================
// 4. DATES, TIMES, CURRENCIES & PERCENTAGES
// =========================================================================

#[test]
fn test_normalizer_dates_times_currencies() {
    // Dates
    assert_eq!(
        normalize_vi("25/12/2026"),
        "ngày hai mươi lăm tháng mười hai năm hai nghìn không trăm hai mươi sáu"
    );
    assert_eq!(
        normalize_vi("01/01/2000"),
        "ngày một tháng một năm hai nghìn"
    );
    assert_eq!(normalize_vi("5/3"), "ngày năm tháng ba");
    assert_eq!(
        normalize_vi("tháng 12/2026"),
        "tháng mười hai năm hai nghìn không trăm hai mươi sáu"
    );
    assert_eq!(
        normalize_vi("Hôm nay ngày 25/12/2026"),
        "Hôm nay ngày hai mươi lăm tháng mười hai năm hai nghìn không trăm hai mươi sáu"
    );

    // Times
    assert_eq!(normalize_vi("10:30"), "mười giờ ba mươi phút");
    assert_eq!(normalize_vi("7:05"), "bảy giờ không năm phút");
    assert_eq!(normalize_vi("7:00"), "bảy giờ");
    assert_eq!(
        normalize_vi("10:30:45"),
        "mười giờ ba mươi phút bốn mươi lăm giây"
    );

    // Currencies
    assert_eq!(normalize_vi("5.000đ"), "năm nghìn đồng");
    assert_eq!(normalize_vi("5.000₫"), "năm nghìn đồng");
    assert_eq!(normalize_vi("5000 VND"), "năm nghìn đồng");
    assert_eq!(
        normalize_vi("2.500.000 đồng"),
        "hai triệu năm trăm nghìn đồng"
    );
    assert_eq!(normalize_vi("$5"), "năm đô la");
    assert_eq!(normalize_vi("$1.000"), "một nghìn đô la");
    assert_eq!(normalize_vi("100 USD"), "một trăm đô la mỹ");

    // Colloquial 'k' & Units
    assert_eq!(normalize_vi("50k"), "năm mươi nghìn");
    assert_eq!(normalize_vi("100k"), "một trăm nghìn");
    assert_eq!(normalize_vi("5km"), "năm ki lô mét");
    assert_eq!(normalize_vi("70kg"), "bảy mươi ki lô gam");
    assert_eq!(normalize_vi("100mb"), "một trăm mê ga bai");

    // Percent
    assert_eq!(normalize_vi("5%"), "năm phần trăm");
    assert_eq!(normalize_vi("3,5%"), "ba phẩy năm phần trăm");
}

// =========================================================================
// 5. ABBREVIATIONS, ACRONYMS & CASE SENSITIVITY
// =========================================================================

#[test]
fn test_normalizer_abbreviations_and_case_sensitivity() {
    // Dotted abbreviations
    assert_eq!(normalize_vi("TP.HCM"), "thành phố hồ chí minh");
    assert_eq!(normalize_vi("TS. Nam"), "tiến sĩ Nam");
    assert_eq!(normalize_vi("ThS. Hoa"), "thạc sĩ Hoa");
    assert_eq!(normalize_vi("Q.1"), "quận một");
    assert_eq!(normalize_vi("P.5"), "phường năm");

    // Word abbreviations
    assert_eq!(normalize_vi("UBND"), "ủy ban nhân dân");
    assert_eq!(normalize_vi("THPT"), "trung học phổ thông");
    assert_eq!(normalize_vi("học ĐH"), "học đại học");

    // Case-sensitive acronyms: "AI" vs "ai" ("who")
    assert_eq!(normalize_vi("công nghệ AI"), "công nghệ a i");
    assert_eq!(normalize_vi("ngành IT"), "ngành i t");
    assert_eq!(normalize_vi("ai đó đang gọi"), "ai đó đang gọi");
}

// =========================================================================
// 6. ZERO-PREFIXED NUMBERS & PHONE NUMBERS
// =========================================================================

#[test]
fn test_normalizer_zero_prefixed_and_phone_numbers() {
    assert_eq!(normalize_vi("0123"), "không một hai ba");
    assert_eq!(normalize_vi("007"), "không không bảy");
    assert_eq!(
        normalize_vi("0912345678"),
        "không chín một hai ba bốn năm sáu bảy tám"
    );
    assert_eq!(
        normalize_vi("0901 234 567"),
        "không chín không một hai ba bốn năm sáu bảy"
    );
}

// =========================================================================
// 7. MICROBENCHMARK & SUB-50µs LATENCY VERIFICATION
// =========================================================================

#[test]
fn test_normalizer_sub_50us_microbenchmark() {
    let test_cases = [
        ("short", "Giá 50k nhé bạn"),
        (
            "medium",
            "Hẹn bạn lúc 10:30 ngày 25/12/2026 tại Q.1 TP.HCM nhé",
        ),
        (
            "complex",
            "Tổng đơn 2.500.000đ, giảm 5%, thanh toán qua app hoặc chuyển khoản 0912345678",
        ),
    ];

    const ITERATIONS: usize = 10_000;

    for (label, sentence) in test_cases {
        // Warmup
        for _ in 0..100 {
            let _ = normalize_vi(sentence);
        }

        let mut latencies: Vec<Duration> = Vec::with_capacity(ITERATIONS);
        let start_all = Instant::now();

        for _ in 0..ITERATIONS {
            let t0 = Instant::now();
            let _ = normalize_vi(sentence);
            latencies.push(t0.elapsed());
        }

        let total_time = start_all.elapsed();
        let avg_latency = total_time / ITERATIONS as u32;

        latencies.sort();
        let min_lat = latencies[0];
        let p50_lat = latencies[ITERATIONS * 50 / 100];
        let p99_lat = latencies[ITERATIONS * 99 / 100];
        let max_lat = latencies[ITERATIONS - 1];

        println!(
            "[Normalizer Benchmark - {}] Avg = {:?}, Min = {:?}, P50 = {:?}, P99 = {:?}, Max = {:?}",
            label, avg_latency, min_lat, p50_lat, p99_lat, max_lat
        );

        // Verification assertion: P50 latency must be < 50µs across short and medium turns
        if label != "complex" {
            // Debug builds are unoptimized; the strict number is the release contract.
            const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
            assert!(
                p50_lat < Duration::from_micros(50) * SLOWDOWN,
                "Normalizer P50 latency for '{}' must be < 50µs, got {:?}",
                label,
                p50_lat
            );
        }
    }
}
