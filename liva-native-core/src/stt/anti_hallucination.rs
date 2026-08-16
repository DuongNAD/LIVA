//! 5-Layer Native Rust Anti-Hallucination Filter for Vietnamese STT.
//!
//! Provides defense-in-depth against silence hallucinations, acoustic noise bursts,
//! repeated token runaway, and phantom YouTube/subtitle/music artifacts.
//!
//! Architecture:
//! - Layer 1: VAD Speech Probability Gating (`no_speech_prob > 0.60` or `speech_prob < 0.40`).
//! - Layer 2: Words-Per-Second (WPS) envelope (`1.0 <= wps <= 5.5` for sustained speech).
//! - Layer 3: Frame Shannon Entropy Thresholding (`H(p) < 1.85`) to reject silence/noise hallucinations.
//! - Layer 4: Compression Ratio (`CR < 2.2`) via Gzip compression & Trigram Diversity Repetition Penalty.
//! - Layer 5: Regex Blacklist for common phantom subtitle/music/ad artifacts and suspicious word density.

use flate2::Compression;
use flate2::write::GzEncoder;
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use unicode_normalization::UnicodeNormalization;

/// Reason why a candidate transcription was filtered out.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterReason {
    TextTooShort,
    NoSpeechProbTooHigh(f32),
    WpsTooLow {
        wps: f32,
        min: f32,
    },
    WpsTooHigh {
        wps: f32,
        max: f32,
    },
    EntropyTooHigh {
        entropy: f32,
        threshold: f32,
    },
    RepetitionDetected {
        compression_ratio: f32,
        threshold: f32,
    },
    TrigramDiversityTooLow {
        diversity: f32,
        threshold: f32,
    },
    ConsecutiveWordRepeats {
        word: String,
        count: usize,
    },
    BlacklistPattern(String),
    TooManySuspiciousWords {
        ratio: f32,
        threshold: f32,
    },
}

impl std::fmt::Display for FilterReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TextTooShort => write!(f, "Text too short (< 2 chars)"),
            Self::NoSpeechProbTooHigh(prob) => {
                write!(f, "No-speech probability too high ({prob:.2} > threshold)")
            }
            Self::WpsTooLow { wps, min } => {
                write!(f, "Speech rate too slow ({wps:.2} wps < {min:.2} wps)")
            }
            Self::WpsTooHigh { wps, max } => {
                write!(
                    f,
                    "Speech rate too fast / runaway ({wps:.2} wps > {max:.2} wps)"
                )
            }
            Self::EntropyTooHigh { entropy, threshold } => {
                write!(
                    f,
                    "Frame Shannon entropy too high ({entropy:.2} > {threshold:.2})"
                )
            }
            Self::RepetitionDetected {
                compression_ratio,
                threshold,
            } => {
                write!(
                    f,
                    "Repetition detected: compression ratio {compression_ratio:.2} > {threshold:.2}"
                )
            }
            Self::TrigramDiversityTooLow {
                diversity,
                threshold,
            } => {
                write!(
                    f,
                    "Trigram diversity too low ({diversity:.2} < {threshold:.2})"
                )
            }
            Self::ConsecutiveWordRepeats { word, count } => {
                write!(f, "Word '{word}' repeated {count} times consecutively")
            }
            Self::BlacklistPattern(pat) => {
                write!(f, "Matched phantom/hallucination pattern: '{pat}'")
            }
            Self::TooManySuspiciousWords { ratio, threshold } => {
                write!(
                    f,
                    "Suspicious word density too high ({ratio:.2} > {threshold:.2})"
                )
            }
        }
    }
}

/// Filter decision for a candidate STT transcript.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FilterDecision {
    Valid {
        normalized_text: String,
        wps: f32,
        entropy: Option<f32>,
        compression_ratio: f32,
    },
    Filtered {
        reason: FilterReason,
        original_text: String,
    },
}

impl FilterDecision {
    pub fn is_valid(&self) -> bool {
        matches!(self, Self::Valid { .. })
    }

    pub fn text(&self) -> Option<&str> {
        match self {
            Self::Valid {
                normalized_text, ..
            } => Some(normalized_text.as_str()),
            Self::Filtered { .. } => None,
        }
    }
}

/// Configuration parameters for the 5-layer anti-hallucination filter.
#[derive(Debug, Clone)]
pub struct AntiHallucinationConfig {
    /// Layer 1: Maximum tolerable no-speech probability (default: 0.60).
    pub no_speech_threshold: f32,
    /// Layer 2: Minimum words per second for sustained speech (default: 1.0).
    pub wps_min: f32,
    /// Layer 2: Maximum words per second before runaway detection (default: 5.5).
    pub wps_max: f32,
    /// Layer 3: Maximum average Shannon frame entropy on active frames (default: 1.85).
    pub max_frame_entropy: f32,
    /// Layer 4: Maximum tolerable compression ratio (default: 2.2).
    pub max_compression_ratio: f32,
    /// Layer 4: Minimum trigram diversity ratio (default: 0.35).
    pub min_trigram_diversity: f32,
    /// Layer 4: Maximum consecutive identical words allowed (default: 3).
    pub max_consecutive_repeats: usize,
    /// Layer 5: Maximum ratio of suspicious subtitle/ad words (default: 0.20).
    pub max_suspicious_ratio: f32,
}

impl Default for AntiHallucinationConfig {
    fn default() -> Self {
        Self {
            no_speech_threshold: 0.60,
            wps_min: 1.0,
            wps_max: 5.5,
            max_frame_entropy: 1.85,
            max_compression_ratio: 2.2,
            min_trigram_diversity: 0.35,
            max_consecutive_repeats: 3,
            max_suspicious_ratio: 0.20,
        }
    }
}

/// 5-Layer Anti-Hallucination Filter engine.
pub struct AntiHallucinationFilter {
    config: AntiHallucinationConfig,
    blacklist_patterns: Vec<Regex>,
    suspicious_words: HashSet<&'static str>,
}

impl Default for AntiHallucinationFilter {
    fn default() -> Self {
        Self::new(AntiHallucinationConfig::default())
    }
}

impl AntiHallucinationFilter {
    pub fn new(config: AntiHallucinationConfig) -> Self {
        let patterns = vec![
            // YouTube / Subtitle hallucinations
            r"(?i)cảm\s+ơn\s+(các\s+bạn|quý\s+vị|mọi\s+người|bạn)\s+đã\s+(theo\s+dõi|xem|lắng\s+nghe)",
            r"(?i)(nhấn|hãy|đừng\s+quên)?\s*(đăng\s+ký|subscribe)\s*kênh",
            r"(?i)(nhấn|hãy)?\s*(like|thích)\s*và\s*(subscribe|đăng\s+ký)",
            r"(?i)subtitles?\s*by",
            r"(?i)please\s+subscribe",
            r"(?i)thank\s+you\s+for\s+watching",
            r"(?i)like\s+and\s+subscribe",
            r"(?i)hit\s+the\s+like\s+button",
            r"(?i)don'?t\s+forget\s+to\s+(like|subscribe)",
            // Music / Audio / Transcript artifacts
            r"[♪🎵].*?[♪🎵]",
            r"(?i)\[(music|âm\s+nhạc|applause|vỗ\s+tay|laughter|tiếng\s+cười|singing|hát)\]",
            // Technical / Verification artifacts
            r"(?i)(captions?|subtitles?)\s*(by|are)?\s*(not\s+)?(verified|reviewed)",
            r"(?i)this\s+video\s+is\s+(sponsored|brought\s+to\s+you\s+by)",
        ];

        let compiled = patterns
            .into_iter()
            .filter_map(|p| Regex::new(p).ok())
            .collect();

        let suspicious: HashSet<&'static str> = [
            "subtitles",
            "subscribe",
            "follow",
            "like",
            "share",
            "comment",
            "notification",
            "bell",
            "youtube",
            "channel",
            "sponsored",
        ]
        .into_iter()
        .collect();

        Self {
            config,
            blacklist_patterns: compiled,
            suspicious_words: suspicious,
        }
    }

    /// Normalize Vietnamese text to Unicode NFC and trim whitespace.
    pub fn normalize_vietnamese_nfc(text: &str) -> String {
        text.nfc().collect::<String>().trim().to_string()
    }

    /// Compute frame-level Shannon entropy from CTC log probabilities.
    ///
    /// Logprobs shape: `[T, V]`.
    /// For each frame $t$, $p_i = \exp(l_i)$, $H(t) = -\sum_i p_i l_i$.
    /// Computes average entropy over active (non-pure-blank) frames.
    pub fn compute_shannon_entropy(
        logprobs: &[f32],
        vocab_size: usize,
        blank_id: usize,
    ) -> Option<f32> {
        if logprobs.is_empty() || vocab_size == 0 {
            return None;
        }
        let t_frames = logprobs.len() / vocab_size;
        if t_frames == 0 {
            return None;
        }

        let mut total_entropy = 0.0f32;
        let mut active_frames = 0usize;

        for t in 0..t_frames {
            let row = &logprobs[t * vocab_size..(t + 1) * vocab_size];

            // Exclude silence frames where blank dominates (> 0.98 probability -> logprob > -0.02)
            let blank_lp = row.get(blank_id).copied().unwrap_or(-100.0);
            if blank_lp > -0.02 {
                continue;
            }

            let mut frame_entropy = 0.0f32;
            for &lp in row {
                if lp > -25.0 {
                    let p = lp.exp();
                    frame_entropy -= p * lp;
                }
            }

            if frame_entropy.is_finite() && frame_entropy > 0.0 {
                total_entropy += frame_entropy;
                active_frames += 1;
            }
        }

        if active_frames > 0 {
            Some(total_entropy / active_frames as f32)
        } else {
            Some(0.0)
        }
    }

    /// Compute genuine Gzip Compression Ratio ($CR = \text{raw\_len} / \text{gzip\_len}$).
    /// Highly repetitive hallucination text compresses into a fraction of its size, yielding $CR > 2.2$.
    pub fn compute_compression_ratio(text: &str) -> f32 {
        let bytes = text.trim().as_bytes();
        if bytes.is_empty() {
            return 1.0;
        }

        let mut encoder = GzEncoder::new(Vec::new(), Compression::default());
        if encoder.write_all(bytes).is_err() {
            return 1.0;
        }
        let compressed = match encoder.finish() {
            Ok(c) => c,
            Err(_) => return 1.0,
        };

        if compressed.is_empty() {
            return 1.0;
        }

        bytes.len() as f32 / compressed.len() as f32
    }

    /// Check trigram diversity: unique trigrams / total trigrams.
    pub fn compute_trigram_diversity(words: &[&str]) -> f32 {
        if words.len() < 3 {
            return 1.0;
        }
        let total_trigrams = words.len() - 2;
        let mut unique: HashSet<(&str, &str, &str)> = HashSet::new();
        for i in 0..total_trigrams {
            unique.insert((words[i], words[i + 1], words[i + 2]));
        }
        unique.len() as f32 / total_trigrams as f32
    }

    /// Check if any single word repeats consecutively more than `max_repeats` times.
    pub fn check_consecutive_word_repeats<'a>(
        words: &[&'a str],
        max_repeats: usize,
    ) -> Option<(&'a str, usize)> {
        if words.is_empty() {
            return None;
        }
        let mut current_word = words[0];
        let mut count = 1;

        for &w in &words[1..] {
            if w.eq_ignore_ascii_case(current_word) {
                count += 1;
                if count > max_repeats {
                    return Some((current_word, count));
                }
            } else {
                current_word = w;
                count = 1;
            }
        }
        None
    }

    /// Apply the full 5-Layer filter on candidate text and acoustic metadata.
    pub fn filter(
        &self,
        text: &str,
        duration_sec: f32,
        no_speech_prob: Option<f32>,
        avg_entropy: Option<f32>,
    ) -> FilterDecision {
        let normalized = Self::normalize_vietnamese_nfc(text);

        // Pre-check: Minimum length
        if normalized.chars().count() < 2 {
            return FilterDecision::Filtered {
                reason: FilterReason::TextTooShort,
                original_text: text.to_string(),
            };
        }

        // Layer 1: VAD Speech Probability Gating
        if let Some(nsp) = no_speech_prob.filter(|&nsp| nsp > self.config.no_speech_threshold) {
            return FilterDecision::Filtered {
                reason: FilterReason::NoSpeechProbTooHigh(nsp),
                original_text: text.to_string(),
            };
        }

        // Tokenize into words for rate and linguistic checks
        let words: Vec<&str> = normalized.split_whitespace().collect();
        let word_count = words.len();

        // Layer 2: Words-Per-Second (WPS) Envelope
        let wps = if duration_sec > 0.05 {
            word_count as f32 / duration_sec
        } else {
            0.0
        };

        // Runaway hallucination rate check (e.g. > 5.5 words per second)
        if duration_sec >= 0.5 && wps > self.config.wps_max {
            return FilterDecision::Filtered {
                reason: FilterReason::WpsTooHigh {
                    wps,
                    max: self.config.wps_max,
                },
                original_text: text.to_string(),
            };
        } else if duration_sec < 0.5 && word_count > 3 {
            // Short segment (<0.5s) cannot legitimately have > 3 Vietnamese words
            return FilterDecision::Filtered {
                reason: FilterReason::WpsTooHigh {
                    wps,
                    max: self.config.wps_max,
                },
                original_text: text.to_string(),
            };
        }

        // Isolated phantom noise in long silence (> 4.0s with <= 1 word, wps < 0.5)
        if duration_sec >= 4.0 && word_count <= 1 && wps < self.config.wps_min * 0.5 {
            return FilterDecision::Filtered {
                reason: FilterReason::WpsTooLow {
                    wps,
                    min: self.config.wps_min,
                },
                original_text: text.to_string(),
            };
        }

        // Layer 3: Frame Shannon Entropy Thresholding
        if let Some(entropy) = avg_entropy.filter(|&e| e > self.config.max_frame_entropy) {
            return FilterDecision::Filtered {
                reason: FilterReason::EntropyTooHigh {
                    entropy,
                    threshold: self.config.max_frame_entropy,
                },
                original_text: text.to_string(),
            };
        }

        // Layer 4: Compression Ratio & Trigram Diversity Repetition Penalty
        let compression_ratio = Self::compute_compression_ratio(&normalized);
        if word_count >= 8 && compression_ratio > self.config.max_compression_ratio {
            return FilterDecision::Filtered {
                reason: FilterReason::RepetitionDetected {
                    compression_ratio,
                    threshold: self.config.max_compression_ratio,
                },
                original_text: text.to_string(),
            };
        }

        if word_count >= 8 {
            let diversity = Self::compute_trigram_diversity(&words);
            if diversity < self.config.min_trigram_diversity {
                return FilterDecision::Filtered {
                    reason: FilterReason::TrigramDiversityTooLow {
                        diversity,
                        threshold: self.config.min_trigram_diversity,
                    },
                    original_text: text.to_string(),
                };
            }
        }

        if let Some((rep_word, count)) =
            Self::check_consecutive_word_repeats(&words, self.config.max_consecutive_repeats)
        {
            return FilterDecision::Filtered {
                reason: FilterReason::ConsecutiveWordRepeats {
                    word: rep_word.to_string(),
                    count,
                },
                original_text: text.to_string(),
            };
        }

        // Single word dominance in long repetitions (e.g. 1 word > 75% of 6+ words)
        if word_count >= 6 {
            let mut word_freq: HashMap<&str, usize> = HashMap::new();
            for &w in &words {
                *word_freq.entry(w).or_insert(0) += 1;
            }
            if let Some((_, &freq)) = word_freq
                .iter()
                .max_by_key(|&(_, &v)| v)
                .filter(|&(_, &freq)| (freq as f32 / word_count as f32) > 0.75)
            {
                return FilterDecision::Filtered {
                    reason: FilterReason::RepetitionDetected {
                        compression_ratio: freq as f32,
                        threshold: self.config.max_compression_ratio,
                    },
                    original_text: text.to_string(),
                };
            }
        }

        // Layer 5: Regex Blacklist for Phantom Subtitles / Music / Ad Artifacts
        for pattern in &self.blacklist_patterns {
            if pattern.is_match(&normalized) {
                return FilterDecision::Filtered {
                    reason: FilterReason::BlacklistPattern(pattern.as_str().to_string()),
                    original_text: text.to_string(),
                };
            }
        }

        // Check suspicious words density
        let suspicious_count = words
            .iter()
            .filter(|w| self.suspicious_words.contains(w.to_lowercase().as_str()))
            .count();
        if word_count > 0 {
            let suspicious_ratio = suspicious_count as f32 / word_count as f32;
            if suspicious_ratio > self.config.max_suspicious_ratio {
                return FilterDecision::Filtered {
                    reason: FilterReason::TooManySuspiciousWords {
                        ratio: suspicious_ratio,
                        threshold: self.config.max_suspicious_ratio,
                    },
                    original_text: text.to_string(),
                };
            }
        }

        FilterDecision::Valid {
            normalized_text: normalized,
            wps,
            entropy: avg_entropy,
            compression_ratio,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_vietnamese_sentence() {
        let filter = AntiHallucinationFilter::default();
        let text = "Hôm nay tôi muốn xem lịch trình làm việc";
        let decision = filter.filter(text, 2.8, Some(0.05), Some(1.20));

        assert!(decision.is_valid());
        if let FilterDecision::Valid {
            normalized_text, ..
        } = decision
        {
            assert_eq!(normalized_text, "Hôm nay tôi muốn xem lịch trình làm việc");
        }
    }

    #[test]
    fn layer1_rejects_high_no_speech_probability() {
        let filter = AntiHallucinationFilter::default();
        let text = "xin chào liva";
        let decision = filter.filter(text, 1.5, Some(0.85), Some(1.2));

        assert!(!decision.is_valid());
        match decision {
            FilterDecision::Filtered {
                reason: FilterReason::NoSpeechProbTooHigh(p),
                ..
            } => {
                assert_eq!(p, 0.85);
            }
            _ => panic!("Expected NoSpeechProbTooHigh"),
        }
    }

    #[test]
    fn layer2_rejects_wps_out_of_range() {
        let filter = AntiHallucinationFilter::default();

        // Too slow: single isolated word in 8.0 seconds (0.125 wps < 0.5)
        let slow = filter.filter("alo", 8.0, Some(0.0), Some(1.1));
        assert!(!slow.is_valid());
        assert!(matches!(
            slow,
            FilterDecision::Filtered {
                reason: FilterReason::WpsTooLow { .. },
                ..
            }
        ));

        // Too fast: 20 words in 1.0 second (20 wps > 5.5)
        let fast_text =
            "một hai ba bốn năm sáu bảy tám chín mười một hai ba bốn năm sáu bảy tám chín mười";
        let fast = filter.filter(fast_text, 1.0, Some(0.0), Some(1.1));
        assert!(!fast.is_valid());
        assert!(matches!(
            fast,
            FilterDecision::Filtered {
                reason: FilterReason::WpsTooHigh { .. },
                ..
            }
        ));
    }

    #[test]
    fn layer3_rejects_high_shannon_entropy() {
        let filter = AntiHallucinationFilter::default();
        let text = "xin chào buổi sáng";
        // High entropy 2.45 > 1.85 threshold
        let noisy = filter.filter(text, 1.5, Some(0.1), Some(2.45));

        assert!(!noisy.is_valid());
        match noisy {
            FilterDecision::Filtered {
                reason: FilterReason::EntropyTooHigh { entropy, threshold },
                ..
            } => {
                assert_eq!(entropy, 2.45);
                assert_eq!(threshold, 1.85);
            }
            _ => panic!("Expected EntropyTooHigh"),
        }
    }

    #[test]
    fn layer4_rejects_repeated_words_and_low_trigram_diversity() {
        let filter = AntiHallucinationFilter::default();

        // 4 consecutive identical words
        let repeats = filter.filter("tôi tôi tôi tôi muốn đi chơi", 2.0, Some(0.0), Some(1.2));
        assert!(!repeats.is_valid());

        // Low trigram diversity loop
        let loop_text = "đi chợ đi chợ đi chợ đi chợ đi chợ đi chợ đi chợ đi chợ";
        let loop_res = filter.filter(loop_text, 3.0, Some(0.0), Some(1.2));
        assert!(!loop_res.is_valid());
    }

    #[test]
    fn layer4_gzip_compression_ratio_distinguishes_normal_from_repetitive() {
        let normal = "văn hóa và bộ lạc cổ xưa đã bắt đầu giữ những con vật này để dễ lấy sữa tóc thịt và da";
        let cr_normal = AntiHallucinationFilter::compute_compression_ratio(normal);
        assert!(cr_normal < 1.8, "Normal CR {} should be < 1.8", cr_normal);

        let repetitive = "tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi";
        let cr_rep = AntiHallucinationFilter::compute_compression_ratio(repetitive);
        assert!(cr_rep > 2.5, "Repetitive CR {} should be > 2.5", cr_rep);
    }

    #[test]
    fn layer5_rejects_youtube_and_music_blacklist_patterns() {
        let filter = AntiHallucinationFilter::default();

        let yt1 = filter.filter(
            "Cảm ơn các bạn đã theo dõi video hôm nay",
            2.5,
            Some(0.0),
            Some(1.1),
        );
        assert!(!yt1.is_valid());

        let yt2 = filter.filter(
            "Hãy nhấn like và subscribe kênh của mình nhé",
            2.5,
            Some(0.0),
            Some(1.1),
        );
        assert!(!yt2.is_valid());

        let music = filter.filter("♪ Nhạc không lời ♪", 2.0, Some(0.0), Some(1.1));
        assert!(!music.is_valid());
    }

    #[test]
    fn vietnamese_nfc_normalization_preserves_6_tones_and_diacritics() {
        let original = "Thế giới rộng lớn tiếng Việt có sáu thanh điệu";
        let nfd: String = original.nfd().collect();
        assert!(nfd.chars().count() > original.chars().count());

        let normalized = AntiHallucinationFilter::normalize_vietnamese_nfc(&nfd);
        assert_eq!(normalized, original);

        // 6 Tones verification
        let tones = "ma mà má mả mã mạ";
        let norm_tones = AntiHallucinationFilter::normalize_vietnamese_nfc(tones);
        assert_eq!(norm_tones, "ma mà má mả mã mạ");
    }

    #[test]
    fn entropy_computation_handles_uniform_and_peaked_distributions() {
        let vocab_size = 1000;
        let blank_id = 999;

        let mut peaked = vec![-100.0f32; vocab_size];
        peaked[5] = -0.01;
        let ent_peaked =
            AntiHallucinationFilter::compute_shannon_entropy(&peaked, vocab_size, blank_id)
                .unwrap();
        assert!(ent_peaked < 0.20);

        let mut uniform = vec![-100.0f32; vocab_size];
        for item in uniform.iter_mut().take(50) {
            *item = -(50.0f32.ln());
        }
        let ent_uniform =
            AntiHallucinationFilter::compute_shannon_entropy(&uniform, vocab_size, blank_id)
                .unwrap();
        assert!(ent_uniform > 3.0);
    }
}
