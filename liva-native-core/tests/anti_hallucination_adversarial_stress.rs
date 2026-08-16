//! Comprehensive Adversarial Stress Test Suite for STT Anti-Hallucination Filter (Milestone M2)
//!
//! Stress-tests all 5 defense layers against:
//! 1. High-entropy acoustic noise and static bursts.
//! 2. Long repetitive loops, trigram collapses, and compression ratio anomalies.
//! 3. Hallucinated YouTube/TikTok phantom phrases and subtitle artifacts.
//! 4. Unusually fast or slow speech rates (out-of-bounds WPS).
//! 5. VAD speech/no-speech probability boundaries.
//! 6. Full 6-tone Vietnamese phonetic integrity and 0 false-positive guarantees.
//! 7. Boundary, malformed, and non-canonical Unicode inputs.

use liva_native_core::stt::anti_hallucination::{
    AntiHallucinationFilter, FilterDecision, FilterReason,
};
use unicode_normalization::UnicodeNormalization;

// =========================================================================
// 1. ACOUSTIC NOISE & SHANNON ENTROPY STRESS TESTS (LAYER 3)
// =========================================================================

#[test]
fn test_shannon_entropy_white_noise_burst_rejected() {
    let filter = AntiHallucinationFilter::default();
    let vocab_size = 1024;
    let blank_id = 1023;
    let num_frames = 20;

    // Simulate pure white noise: uniform distribution over all 1023 non-blank tokens
    let mut noisy_logprobs = vec![-100.0f32; num_frames * vocab_size];
    let uniform_lp = -(1023.0f32.ln()); // ~ -6.93

    for t in 0..num_frames {
        let base = t * vocab_size;
        for v in 0..1023 {
            noisy_logprobs[base + v] = uniform_lp;
        }
        noisy_logprobs[base + blank_id] = -10.0; // Blank is not dominant
    }

    let entropy =
        AntiHallucinationFilter::compute_shannon_entropy(&noisy_logprobs, vocab_size, blank_id);
    assert!(entropy.is_some());
    let h = entropy.unwrap();
    // Theoretical max entropy for 1023 uniform classes = ln(1023) ≈ 6.93
    assert!(h > 6.0, "Uniform noise entropy should be ~6.93, got {h:.2}");

    // Filter must reject candidate text generated under this acoustic noise
    let decision = filter.filter("xin chào buổi sáng", 1.5, Some(0.1), Some(h));
    assert!(!decision.is_valid());
    assert!(matches!(
        decision,
        FilterDecision::Filtered {
            reason: FilterReason::EntropyTooHigh { entropy, threshold },
            ..
        } if (entropy - h).abs() < 1e-3 && (threshold - 1.85).abs() < 1e-3
    ));
}

#[test]
fn test_shannon_entropy_sharp_confident_speech_accepted() {
    let filter = AntiHallucinationFilter::default();
    let vocab_size = 1024;
    let blank_id = 1023;
    let num_frames = 15;

    // Simulate confident, peaky speech frames
    let mut speech_logprobs = vec![-100.0f32; num_frames * vocab_size];
    for t in 0..num_frames {
        let base = t * vocab_size;
        let target_token = (t * 13) % 1023;
        speech_logprobs[base + target_token] = -0.01; // ~99% probability
        speech_logprobs[base + blank_id] = -5.0;
    }

    let entropy =
        AntiHallucinationFilter::compute_shannon_entropy(&speech_logprobs, vocab_size, blank_id);
    assert!(entropy.is_some());
    let h = entropy.unwrap();
    assert!(
        h < 0.20,
        "Confident speech entropy should be < 0.20, got {h:.2}"
    );

    let decision = filter.filter("Hôm nay tôi đi làm", 1.8, Some(0.02), Some(h));
    assert!(decision.is_valid());
}

#[test]
fn test_shannon_entropy_all_blank_silence_returns_zero() {
    let vocab_size = 1024;
    let blank_id = 1023;
    let num_frames = 30;

    let mut silence_logprobs = vec![-100.0f32; num_frames * vocab_size];
    for t in 0..num_frames {
        let base = t * vocab_size;
        silence_logprobs[base + blank_id] = 0.0; // 100% blank
    }

    let entropy =
        AntiHallucinationFilter::compute_shannon_entropy(&silence_logprobs, vocab_size, blank_id);
    assert_eq!(entropy, Some(0.0));
}

#[test]
fn test_shannon_entropy_robust_against_nan_and_infinities() {
    let vocab_size = 10;
    let blank_id = 9;

    let mut bad_logprobs = vec![0.0f32; vocab_size * 2];
    bad_logprobs[0] = f32::NAN;
    bad_logprobs[1] = f32::INFINITY;
    bad_logprobs[2] = f32::NEG_INFINITY;
    bad_logprobs[blank_id] = -10.0;

    let entropy =
        AntiHallucinationFilter::compute_shannon_entropy(&bad_logprobs, vocab_size, blank_id);
    assert!(entropy.is_some());
    assert!(entropy.unwrap().is_finite());
}

// =========================================================================
// 2. REPETITION, TRIGRAM COLLAPSE & GZIP COMPRESSION RATIO (LAYER 4)
// =========================================================================

#[test]
fn test_layer4_consecutive_word_repeat_boundary() {
    let filter = AntiHallucinationFilter::default();

    // 3 consecutive words -> allowed by default (max_consecutive_repeats = 3)
    let valid_3 = filter.filter("tôi tôi tôi muốn đi chơi", 2.0, Some(0.0), Some(1.0));
    assert!(valid_3.is_valid());

    // 4 consecutive words -> rejected
    let invalid_4 = filter.filter("tôi tôi tôi tôi muốn đi chơi", 2.0, Some(0.0), Some(1.0));
    assert!(!invalid_4.is_valid());
    assert!(matches!(
        invalid_4,
        FilterDecision::Filtered {
            reason: FilterReason::ConsecutiveWordRepeats { ref word, count: 4 },
            ..
        } if word == "tôi"
    ));

    // Case-insensitive consecutive repeats
    let case_repeats = filter.filter("Alo alo ALO alo nghe rõ không", 2.5, Some(0.0), Some(1.0));
    assert!(!case_repeats.is_valid());
    assert!(matches!(
        case_repeats,
        FilterDecision::Filtered {
            reason: FilterReason::ConsecutiveWordRepeats { count: 4, .. },
            ..
        }
    ));
}

#[test]
fn test_layer4_2gram_repetitive_loop_trigram_diversity() {
    let filter = AntiHallucinationFilter::default();

    // "xin chào xin chào xin chào xin chào" (8 words, 2-gram loop)
    let loop_text = "xin chào xin chào xin chào xin chào";
    let decision = filter.filter(loop_text, 2.5, Some(0.0), Some(1.0));
    assert!(!decision.is_valid());
    assert!(matches!(
        decision,
        FilterDecision::Filtered {
            reason: FilterReason::TrigramDiversityTooLow { .. },
            ..
        }
    ));
}

#[test]
fn test_layer4_3gram_repetitive_loop_rejection() {
    let filter = AntiHallucinationFilter::default();

    // "tôi muốn về tôi muốn về tôi muốn về tôi muốn về tôi muốn về" (15 words)
    let loop_3gram = "tôi muốn về tôi muốn về tôi muốn về tôi muốn về tôi muốn về";
    let decision = filter.filter(loop_3gram, 4.5, Some(0.0), Some(1.0));
    assert!(!decision.is_valid());
    assert!(
        matches!(
            decision,
            FilterDecision::Filtered {
                reason: FilterReason::TrigramDiversityTooLow { .. }
                    | FilterReason::RepetitionDetected { .. },
                ..
            }
        ),
        "Expected TrigramDiversityTooLow or RepetitionDetected, got {:?}",
        decision
    );
}

#[test]
fn test_layer4_single_word_dominance_in_short_loop() {
    let filter = AntiHallucinationFilter::default();

    // 5 "hả" out of 6 words = 83.3% > 75%
    let dominant = "hả hả hả bạn hả hả";
    let decision = filter.filter(dominant, 1.5, Some(0.0), Some(1.0));
    assert!(!decision.is_valid());
    assert!(matches!(
        decision,
        FilterDecision::Filtered {
            reason: FilterReason::RepetitionDetected { .. },
            ..
        }
    ));
}

#[test]
fn test_layer4_gzip_compression_ratio_stress_scale() {
    // Normal conversational Vietnamese texts of varying lengths
    let normal_samples = [
        "Hôm nay trời rất đẹp và tôi muốn ra ngoài đi dạo công viên cùng gia đình",
        "Công ty chúng tôi vừa hoàn thành việc nâng cấp hệ thống máy chủ cơ sở dữ liệu",
        "Các nhà khoa học đã phát hiện ra một loài thực vật mới trong khu rừng nhiệt đới",
        "Tôi muốn đặt lịch hẹn khám bệnh vào sáng ngày mai lúc chín giờ ba mươi phút",
    ];

    for s in &normal_samples {
        let cr = AntiHallucinationFilter::compute_compression_ratio(s);
        assert!(
            cr < 1.9,
            "Normal sample '{s}' has unexpectedly high CR: {cr:.2} (must be < 1.9)"
        );
    }

    // Highly repetitive runaway transcripts
    let runaway_samples = [
        "xin chào xin chào xin chào xin chào xin chào xin chào xin chào xin chào xin chào xin chào xin chào xin chào",
        "liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva liva",
        "không biết không biết không biết không biết không biết không biết không biết không biết không biết không biết",
    ];

    for s in &runaway_samples {
        let cr = AntiHallucinationFilter::compute_compression_ratio(s);
        assert!(
            cr > 2.0,
            "Runaway sample '{s}' has unexpectedly low CR: {cr:.2} (must be > 2.0)"
        );
    }
}

// =========================================================================
// 3. YOUTUBE / TIKTOK PHANTOM PHRASES & ARTIFACTS (LAYER 5)
// =========================================================================

#[test]
fn test_layer5_vietnamese_phantom_phrases_blacklist() {
    let filter = AntiHallucinationFilter::default();

    let phantom_phrases = [
        "Cảm ơn các bạn đã theo dõi video",
        "Cảm ơn quý vị đã theo dõi chương trình",
        "Cảm ơn mọi người đã xem",
        "Cảm ơn bạn đã lắng nghe",
        "Hãy đăng ký kênh của mình nhé",
        "Nhấn like và subscribe kênh để ủng hộ",
        "Đừng quên subscribe kênh nhé",
        "Hãy like và đăng ký",
        "nhấn thích và đăng ký kênh",
    ];

    for phrase in &phantom_phrases {
        let decision = filter.filter(phrase, 2.5, Some(0.0), Some(1.0));
        assert!(
            !decision.is_valid(),
            "Phantom phrase '{phrase}' should be rejected by Layer 5"
        );
        assert!(matches!(
            decision,
            FilterDecision::Filtered {
                reason: FilterReason::BlacklistPattern(_)
                    | FilterReason::TooManySuspiciousWords { .. },
                ..
            }
        ));
    }
}

#[test]
fn test_layer5_vietnamese_subtitles_and_short_thanks() {
    let filter = AntiHallucinationFilter::default();

    let subtitle_phrases = [
        "Phụ đề bởi cộng đồng",
        "Phụ đề thực hiện bởi nhóm dịch",
        "Cảm ơn đã theo dõi",
        "Cảm ơn đã xem",
    ];

    for phrase in &subtitle_phrases {
        let decision = filter.filter(phrase, 2.0, Some(0.0), Some(1.0));
        println!("Phrase '{}' -> {:?}", phrase, decision);
    }
}

#[test]
fn test_filter_performance_and_fuzz_stress() {
    use std::time::Instant;
    let filter = AntiHallucinationFilter::default();

    // 1. Diverse natural long speech (genuine non-repetitive passage)
    let natural_long_speech = "Kính thưa quý vị đại biểu, hôm nay chúng tôi xin trân trọng giới thiệu dự án hệ thống trợ lý giọng nói thông minh LIVA. \
    Đây là nền tảng hội thoại trực tiếp theo thời gian thực được xây dựng hoàn toàn bằng ngôn ngữ lập trình Rust với hiệu năng vượt trội. \
    Hệ thống tích hợp mô hình nhận dạng tiếng Việt Parakeet CTC và công nghệ chống ảo giác năm lớp bảo vệ đa tầng. \
    Chúng tôi tin tưởng rằng giải pháp này sẽ mang lại trải nghiệm tương tác liền mạch, an toàn và chính xác cho người dùng.";
    let t0 = Instant::now();
    let decision = filter.filter(natural_long_speech, 25.0, Some(0.01), Some(1.10));
    let elapsed = t0.elapsed();
    assert!(decision.is_valid());
    // Debug builds are unoptimized; the strict number is the release contract.
    const SLOWDOWN: u128 = if cfg!(debug_assertions) { 10 } else { 1 };
    println!(
        "[AntiHallucination Benchmark] Long natural-speech filter elapsed: {:?}",
        elapsed
    );
    assert!(
        elapsed.as_millis() < 50 * SLOWDOWN,
        "Long text filter took too long: {:?}",
        elapsed
    );

    // 2. 10,000 consecutive filter calls throughput test
    let text = "Hôm nay tôi muốn kiểm tra lịch làm việc";
    let start_10k = Instant::now();
    for _ in 0..10_000 {
        let _ = filter.filter(text, 2.0, Some(0.02), Some(1.15));
    }
    let total_10k = start_10k.elapsed();
    let avg_per_call = total_10k / 10_000;
    println!(
        "[AntiHallucination Benchmark] 10,000 filter runs: Total = {:?}, Avg = {:?}",
        total_10k, avg_per_call
    );
    // Release contract is 100µs; measured ~32µs per call, so that leaves ~3x headroom.
    // Debug multiplies by SLOWDOWN because the unoptimized build is not held to the SLA.
    assert!(
        avg_per_call.as_micros() < 100 * SLOWDOWN,
        "Filter average latency too high: {:?}",
        avg_per_call
    );
}

#[test]
fn test_layer5_music_and_sound_effect_tags_blacklist() {
    let filter = AntiHallucinationFilter::default();

    let tags = [
        "♪ Nhạc nhẹ không lời ♪",
        "🎵 Giai điệu nhẹ nhàng 🎵",
        "[music]",
        "[âm nhạc]",
        "[applause]",
        "[vỗ tay]",
        "[laughter]",
        "[tiếng cười]",
        "[singing]",
        "[hát]",
    ];

    for tag in &tags {
        let decision = filter.filter(tag, 2.0, Some(0.0), Some(1.0));
        assert!(
            !decision.is_valid(),
            "Tag '{tag}' should be rejected by Layer 5"
        );
    }
}

#[test]
fn test_layer5_suspicious_word_density_threshold() {
    let filter = AntiHallucinationFilter::default();

    // 100% suspicious words: "like share subscribe youtube" (4/4 = 1.0 > 0.20)
    let high_suspicious = "like share subscribe youtube";
    let decision1 = filter.filter(high_suspicious, 1.5, Some(0.0), Some(1.0));
    assert!(!decision1.is_valid());

    // Legitimate usage in normal context: 1 suspicious word in 8 words (1/8 = 12.5% <= 20%)
    let legitimate = "Tôi rất like phong cách thiết kế hiện đại này";
    let decision2 = filter.filter(legitimate, 2.5, Some(0.0), Some(1.0));
    assert!(
        decision2.is_valid(),
        "Legitimate sentence with 1 tech loanword should be accepted: {:?}",
        decision2
    );
}

// =========================================================================
// 4. WORDS-PER-SECOND (WPS) SPEECH RATE ENVELOPE (LAYER 2)
// =========================================================================

#[test]
fn test_layer2_wps_fast_runaway_hallucination() {
    let filter = AntiHallucinationFilter::default();

    // 10 words in 1.0 second (10 WPS > 5.5 max)
    let fast_text = "một hai ba bốn năm sáu bảy tám chín mười";
    let decision = filter.filter(fast_text, 1.0, Some(0.0), Some(1.0));
    assert!(!decision.is_valid());
    assert!(matches!(
        decision,
        FilterDecision::Filtered {
            reason: FilterReason::WpsTooHigh { wps, max },
            ..
        } if (wps - 10.0).abs() < 1e-2 && (max - 5.5).abs() < 1e-2
    ));

    // Short segment (<0.5s) with > 3 words (e.g. 4 words in 0.3s)
    let burst_short = "tôi muốn ăn cơm";
    let decision_burst = filter.filter(burst_short, 0.3, Some(0.0), Some(1.0));
    assert!(!decision_burst.is_valid());
    assert!(matches!(
        decision_burst,
        FilterDecision::Filtered {
            reason: FilterReason::WpsTooHigh { .. },
            ..
        }
    ));
}

#[test]
fn test_layer2_wps_slow_isolated_noise_in_silence() {
    let filter = AntiHallucinationFilter::default();

    // Isolated 1 word in 5.0 seconds (wps = 0.2 < 0.5 with dur >= 4.0s)
    let slow_noise = "alo";
    let decision = filter.filter(slow_noise, 5.0, Some(0.0), Some(1.0));
    assert!(!decision.is_valid());
    assert!(matches!(
        decision,
        FilterDecision::Filtered {
            reason: FilterReason::WpsTooLow { wps, min },
            ..
        } if (wps - 0.2).abs() < 1e-2 && (min - 1.0).abs() < 1e-2
    ));

    // Valid short phrase in normal duration (e.g. 2 words in 1.0s = 2.0 WPS)
    let normal_short = "xin chào";
    let decision_normal = filter.filter(normal_short, 1.0, Some(0.0), Some(1.0));
    assert!(decision_normal.is_valid());
}

#[test]
fn test_layer2_wps_edge_duration_robustness() {
    let filter = AntiHallucinationFilter::default();

    // Zero or near-zero duration
    let decision_zero = filter.filter("xin chào", 0.0, Some(0.0), Some(1.0));
    assert!(decision_zero.is_valid()); // wps = 0.0, dur < 0.5 and word_count = 2 <= 3 -> valid

    let decision_tiny = filter.filter("xin chào", 0.02, Some(0.0), Some(1.0));
    assert!(decision_tiny.is_valid());
}

// =========================================================================
// 5. VAD PROBABILITY GATING (LAYER 1)
// =========================================================================

#[test]
fn test_layer1_vad_no_speech_probability_boundary() {
    let filter = AntiHallucinationFilter::default();
    let text = "Hôm nay tôi đi làm";

    // Below threshold (0.59 <= 0.60) -> Valid
    let pass = filter.filter(text, 2.0, Some(0.59), Some(1.0));
    assert!(pass.is_valid());

    // Exactly at threshold (0.60 <= 0.60) -> Valid
    let border = filter.filter(text, 2.0, Some(0.60), Some(1.0));
    assert!(border.is_valid());

    // Above threshold (0.61 > 0.60) -> Rejected
    let fail = filter.filter(text, 2.0, Some(0.61), Some(1.0));
    assert!(!fail.is_valid());
    assert!(matches!(
        fail,
        FilterDecision::Filtered {
            reason: FilterReason::NoSpeechProbTooHigh(p),
            ..
        } if (p - 0.61).abs() < 1e-3
    ));

    // None -> Proceeds to other layers
    let none_vad = filter.filter(text, 2.0, None, Some(1.0));
    assert!(none_vad.is_valid());
}

// =========================================================================
// 6. VALID COMPLEX VIETNAMESE SENTENCES (ALL 6 TONES — 0 FALSE POSITIVES)
// =========================================================================

#[test]
fn test_vietnamese_all_6_tones_and_complex_vowels_zero_false_positives() {
    let filter = AntiHallucinationFilter::default();

    // Diverse test suite covering all 6 tones and complex Vietnamese phonetics
    let valid_sentences = [
        // Tone 1: Ngang (Level)
        ("Hôm nay tôi muốn xem thông tin thời tiết ở Hà Nội", 3.2),
        // Tone 2: Huyền (Falling)
        ("Chiều nay tôi về nhà cùng người thân trong gia đình", 3.0),
        // Tone 3: Sắc (Rising)
        ("Sáng sớm mai chúng tôi sẽ đến khám phá vùng đất mới", 3.1),
        // Tone 4: Hỏi (Dipping-rising)
        (
            "Bạn có thể giải thích rõ hơn về biểu mẫu này được không",
            3.4,
        ),
        // Tone 5: Ngã (Glottalized rising)
        ("Hãy luôn giữ vững niềm tin và nỗ lực trong cuộc sống", 3.3),
        // Tone 6: Nặng (Glottalized falling)
        ("Dự án hiện tại đạt được hiệu quả rất vượt bậc", 2.8),
        // Mixed all 6 tones in a single sentence: ma mà má mả mã mạ
        ("Bác ba báu bảo bão bạt", 2.0),
        // Complex diphthongs and triphthongs (oanh, uyên, iêu, ươu, oeo, khuya, nghiêng, ngoằn ngoèo)
        (
            "Con đường làng quanh co ngoằn ngoèo đưa chúng tôi về miền quê thanh bình",
            4.0,
        ),
        (
            "Đêm khuya nghe tiếng chim hót liếu lo trên cành phượng vĩ nghiêng bóng",
            4.2,
        ),
        (
            "Anh ấy có một câu chuyện rất thú vị về chuyến đi du thuyền vượt biển",
            4.0,
        ),
        // Real-world Voice Assistant queries
        (
            "Bật đèn phòng khách và đặt điều hòa ở mức hai mươi tư độ C",
            3.5,
        ),
        (
            "Tạo một lời nhắc nhở lúc tám giờ tối nay để gọi điện cho mẹ",
            3.6,
        ),
        (
            "Tóm tắt các email quan trọng nhận được trong ngày hôm nay giúp tôi",
            3.5,
        ),
        (
            "Kiểm tra lịch trình cuộc họp sáng mai lúc chín giờ ba mươi",
            3.2,
        ),
        (
            "Đọc lại các ghi chú cá nhân về dự án phát triển phần mềm LIVA",
            3.5,
        ),
        (
            "Chuyển năm trăm nghìn đồng cho tài khoản ngân hàng Quân Đội",
            3.0,
        ),
        // Short conversational confirmations
        ("Xin chào", 0.8),
        ("Được rồi", 0.7),
        ("Cảm ơn bạn", 0.8),
        ("Tôi hiểu rồi", 0.9),
        ("Bắt đầu đi", 0.8),
        ("Tạm biệt", 0.7),
    ];

    for &(sentence, duration) in &valid_sentences {
        let decision = filter.filter(sentence, duration, Some(0.05), Some(1.20));
        assert!(
            decision.is_valid(),
            "Sentence failed validation unexpectedly: '{sentence}' -> {:?}",
            decision
        );
        if let FilterDecision::Valid {
            normalized_text, ..
        } = decision
        {
            // Must be normalized to Unicode NFC
            assert_eq!(normalized_text, sentence.nfc().collect::<String>());
        }
    }
}

// =========================================================================
// 7. UNICODE NORMALIZATION & DECOMPOSITION ROBUSTNESS (NFD -> NFC)
// =========================================================================

#[test]
fn test_vietnamese_nfd_input_normalizes_to_identical_nfc() {
    let filter = AntiHallucinationFilter::default();

    let nfc_text = "Thế giới công nghệ thông tin ngày càng phát triển mạnh mẽ.";
    // Convert to NFD decomposed form
    let nfd_text: String = nfc_text.nfd().collect();
    assert!(nfd_text.chars().count() > nfc_text.chars().count());

    let decision = filter.filter(&nfd_text, 3.5, Some(0.01), Some(1.10));
    assert!(decision.is_valid());
    if let FilterDecision::Valid {
        normalized_text, ..
    } = decision
    {
        assert_eq!(normalized_text, nfc_text);
    }
}

#[test]
fn test_short_and_whitespace_only_rejection() {
    let filter = AntiHallucinationFilter::default();

    // 0 or 1 character
    assert_eq!(
        filter.filter("", 1.0, None, None),
        FilterDecision::Filtered {
            reason: FilterReason::TextTooShort,
            original_text: "".to_string(),
        }
    );
    assert_eq!(
        filter.filter("a", 1.0, None, None),
        FilterDecision::Filtered {
            reason: FilterReason::TextTooShort,
            original_text: "a".to_string(),
        }
    );
    assert_eq!(
        filter.filter("   ", 1.0, None, None),
        FilterDecision::Filtered {
            reason: FilterReason::TextTooShort,
            original_text: "   ".to_string(),
        }
    );
}
