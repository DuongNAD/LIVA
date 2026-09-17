//! Adversarial Challenge Test Suite for Milestone 3 (Phase 2 Heuristic Complexity Classifier)
//!
//! Evaluates `phan_loai_do_kho` under extreme adversarial conditions:
//! 1. Empty, whitespace, Unicode zero-width, BOM, null bytes, Zalgo text, emojis, punctuation noise.
//! 2. Massive payloads (100KB - 1MB), ReDoS / runaway loops.
//! 3. Substring vs whole-token keyword boundary precision (e.g. `definition` vs `def `, `classic` vs `class `).
//! 4. Subtle multi-question and numbered list heuristics.
//! 5. Microsecond execution latency and 0-token verification.

use liva_native_core::agent::graph::{DoKho, Intent, phan_loai_do_kho, route_intent};
use std::time::Instant;

// =========================================================================
// 1. ADVERSARIAL EDGE CASES: STRINGS, UNICODE, EMOJIS, SPECIAL CHARACTERS
// =========================================================================

#[test]
fn test_challenge_empty_whitespace_and_special_unicode() {
    let edge_cases_noise = [
        "",
        " ",
        "   \t\r\n   ",
        "\u{0000}",                // Null byte
        "\u{200B}",                // Zero-width space
        "\u{FEFF}",                // Byte order mark (BOM)
        "\u{200E}\u{200F}",        // Directional marks
        "👋🤖✨🎉",                // Emojis only (no alphanumeric tokens)
        "Xin chào 👋 bạn nhé! ✨", // Emojis in casual greeting -> Thuong
        "t̷ừ̸n̷g̷ ̸b̷ư̸ớ̵c̷",               // Zalgo corrupted text
        "!!!???...",               // Pure punctuation noise -> Thuong
        "???",                     // Pure punctuation noise -> Thuong
        "??",                      // Pure punctuation noise -> Thuong
        "?",                       // Single question mark -> Thuong
    ];

    for text in edge_cases_noise {
        let result = phan_loai_do_kho(text);
        assert_eq!(
            result,
            DoKho::Thuong,
            "Input '{text}' should default to Thuong"
        );
    }

    // Meaningful queries with multiple question marks -> must be Kho
    let multi_question_queries = [
        "Bạn là ai? Bạn từ đâu đến?",
        "Hôm nay có mưa không? Nhiệt độ là bao nhiêu?",
        "Why did this fail? How can we fix it?",
    ];

    for query in multi_question_queries {
        assert_eq!(
            phan_loai_do_kho(query),
            DoKho::Kho,
            "Textual multi-question '{query}' must classify as Kho"
        );
    }
}

// =========================================================================
// 2. MASSIVE PAYLOADS & RE-DOS RESILIENCE (STRESS HARNESS)
// =========================================================================

#[test]
fn test_challenge_massive_payload_and_memory_safety() {
    // Test 1: 100 KB repetitive payload with text words
    let large_100kb = "Xin chao LIVA ".repeat(7000);
    assert!(large_100kb.len() >= 98_000);
    let start_100kb = Instant::now();
    let res_100kb = phan_loai_do_kho(&large_100kb);
    let dur_100kb = start_100kb.elapsed();
    assert_eq!(
        res_100kb,
        DoKho::Kho,
        "100KB payload must exceed token/char thresholds and classify as Kho"
    );
    assert!(
        dur_100kb.as_millis() < 50,
        "100KB classification took {:?}, exceeding 50ms bound",
        dur_100kb
    );

    // Test 2: 1 MB repetitive payload with text words
    let large_1mb = "Hôm nay thời tiết đẹp quá bạn ơi. ".repeat(30_000);
    assert!(large_1mb.len() >= 1_000_000);
    let start_1mb = Instant::now();
    let res_1mb = phan_loai_do_kho(&large_1mb);
    let dur_1mb = start_1mb.elapsed();
    assert_eq!(res_1mb, DoKho::Kho, "1MB payload must classify as Kho");
    assert!(
        dur_1mb.as_millis() < 150,
        "1MB classification took {:?}, exceeding 150ms bound",
        dur_1mb
    );

    // Test 3: 5,000 question marks with a text word (token not empty)
    let qmarks_5000_with_word = format!("cau_hoi {}", "?".repeat(5000));
    let start_q = Instant::now();
    let res_q = phan_loai_do_kho(&qmarks_5000_with_word);
    let dur_q = start_q.elapsed();
    assert_eq!(res_q, DoKho::Kho);
    assert!(
        dur_q.as_micros() < 5000,
        "5000 question marks took {:?}, exceeding 5ms bound",
        dur_q
    );

    // Test 4: Deeply nested markdown blocks
    let nested_markdown = "```rust\n```python\n```c\n```sql\nSELECT 1;\n```\n```\n```\n```";
    assert_eq!(phan_loai_do_kho(nested_markdown), DoKho::Kho);
}

// =========================================================================
// 3. SUBSTRING VS TOKEN BOUNDARY FALSE POSITIVE EVASION
// =========================================================================

#[test]
fn test_challenge_substring_vs_word_boundary_false_positives() {
    // Ensure English words with code/math prefixes do not trigger false positives
    let non_code_queries = [
        "What is the definition of happiness?", // "definition" contains "def"
        "I love classic literature and music",  // "classic" contains "class"
        "The infrastructure of this city is great", // "infrastructure" contains "struct"
        "Inflation has increased this year",    // "inflation" contains "fn"
        "She is going to school today",         // normal question
        "We are evaluating options",            // "evaluating" vs "evaluate"
        "This is an analytical approach",       // "analytical" vs "analyze" / "analysis"
    ];

    for query in non_code_queries {
        let result = phan_loai_do_kho(query);
        assert_eq!(
            result,
            DoKho::Thuong,
            "Query '{query}' should NOT falsely trigger Kho due to substring match"
        );
    }
}

// =========================================================================
// 4. CODE SYNTAX & MULTI-QUESTION DETECTION MATRIX
// =========================================================================

#[test]
fn test_challenge_code_syntax_and_multi_question_detection() {
    // Code blocks & statements
    let code_cases = [
        "def calculate_total(items): return sum(items)",
        "fn process_data(buf: &[u8]) -> Result<()> { Ok(()) }",
        "class OrderManager extends BaseService {}",
        "struct AppState { count: usize }",
        "impl Display for Vector2D { fn fmt(&self) {} }",
        "select id, username from accounts where active = 1",
        "SELECT id, name FROM users",
        "```typescript\nconst x: number = 42;\n```",
    ];

    for code in code_cases {
        assert_eq!(
            phan_loai_do_kho(code),
            DoKho::Kho,
            "Code syntax '{code}' must be classified as Kho"
        );
    }

    // Numbered lists with & without question marks
    assert_eq!(
        phan_loai_do_kho("1. Bật đèn phòng khách 2. Tắt quạt phòng ngủ"),
        DoKho::Thuong,
        "Numbered list without question marks or multi-step reasoning remains Thuong"
    );
    assert_eq!(
        phan_loai_do_kho("1. Dự án gồm những module nào? 2. Chi phí bao nhiêu?"),
        DoKho::Kho,
        "Numbered list with question marks must classify as Kho"
    );
    assert_eq!(
        phan_loai_do_kho("1) Giải thích nguyên lý 2) Đưa ra ví dụ?"),
        DoKho::Kho,
        "Numbered list with parenthesis and question marks must classify as Kho"
    );
}

// =========================================================================
// 5. MICROSECOND LATENCY & 0-TOKEN BENCHMARK (100,000 ITERATIONS)
// =========================================================================

#[test]
fn test_challenge_microsecond_latency_benchmark() {
    let test_queries = [
        "Xin chào LIVA",
        "bật đèn phòng khách",
        "Thời tiết Hà Nội hôm nay thế nào?",
        "Viết hàm kiểm tra số nguyên tố bằng Rust",
        "Phân tích ưu nhược điểm của SQLite WAL so với PostgreSQL",
        "1. Kế hoạch ra sao? 2. Rủi ro gồm những gì?",
        "What is Tokio runtime? Explain in detail",
        "SELECT * FROM users WHERE active = 1",
        "nhỏ nhạc lại giúp mình",
        "tắt máy lạnh nhé",
    ];

    let iterations = 100_000;
    let start = Instant::now();

    for i in 0..iterations {
        let query = test_queries[i % test_queries.len()];
        let _ = phan_loai_do_kho(query);
    }

    let elapsed = start.elapsed();
    let avg_micros = elapsed.as_micros() as f64 / iterations as f64;

    println!("\n=== EMPIRICAL COMPLEXITY ROUTER BENCHMARK ===");
    println!("Total iterations: {iterations}");
    println!("Total time: {:?}", elapsed);
    println!("Average latency per call: {:.4} µs", avg_micros);

    // Latency must be strictly under 25 microseconds per call (empirical threshold)
    assert!(
        avg_micros < 25.0,
        "Average latency ({avg_micros:.4} µs) exceeded 25 µs target!"
    );
}

// =========================================================================
// 6. INTENT ROUTER REGRESSION VERIFICATION (~60 CASES MATRIX)
// =========================================================================

#[test]
fn test_challenge_route_intent_zero_regression() {
    let test_matrix = [
        // Smart Home
        (
            "bật đèn",
            Intent::SmartHome {
                device: "light",
                action: "on",
            },
        ),
        (
            "tắt đèn đi",
            Intent::SmartHome {
                device: "light",
                action: "off",
            },
        ),
        (
            "mở quạt",
            Intent::SmartHome {
                device: "fan",
                action: "on",
            },
        ),
        (
            "tắt quạt",
            Intent::SmartHome {
                device: "fan",
                action: "off",
            },
        ),
        (
            "bật điều hoà",
            Intent::SmartHome {
                device: "ac",
                action: "on",
            },
        ),
        (
            "tắt máy lạnh",
            Intent::SmartHome {
                device: "ac",
                action: "off",
            },
        ),
        // Media & Volume
        (
            "bật nhạc lên",
            Intent::OsControl {
                tool: "control_media",
                action: "play_pause",
            },
        ),
        (
            "tắt nhạc",
            Intent::OsControl {
                tool: "control_media",
                action: "play_pause",
            },
        ),
        (
            "chuyển bài khác",
            Intent::OsControl {
                tool: "control_media",
                action: "next",
            },
        ),
        (
            "quay lại bài trước",
            Intent::OsControl {
                tool: "control_media",
                action: "previous",
            },
        ),
        (
            "tăng âm lượng lên",
            Intent::OsControl {
                tool: "control_volume",
                action: "up",
            },
        ),
        (
            "giảm âm lượng xuống",
            Intent::OsControl {
                tool: "control_volume",
                action: "down",
            },
        ),
        (
            "tắt tiếng đi",
            Intent::OsControl {
                tool: "control_volume",
                action: "mute",
            },
        ),
        // Vision
        ("trên màn hình có gì", Intent::Vision),
        ("take a screenshot", Intent::Vision),
        // Chat fallback
        ("Xin chào bạn", Intent::Chat),
        ("Hôm nay là thứ mấy?", Intent::Chat),
        ("Viết code Rust", Intent::Chat),
    ];

    for (input, expected) in test_matrix {
        let actual = route_intent(input);
        assert_eq!(
            actual, expected,
            "Regression detected on route_intent('{input}') -> got {actual:?}, expected {expected:?}"
        );
    }
}
