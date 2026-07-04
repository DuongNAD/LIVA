//! Wake-word gate: hands-free "LIVA" activation for always-on listening.
//!
//! v1 strategy (`asr_prefix`): the mic stream is always VAD-gated (Silero,
//! ~negligible CPU). When speech ends while the assistant is *asleep*, the
//! utterance is transcribed once and checked for a wake phrase ("liva",
//! "hey liva", …, diacritic-insensitive). On a match the gate opens for
//! `LIVA_WAKE_WINDOW_SECS` (extended on every interaction) and the SAME
//! utterance is forwarded to the pipeline — so "Liva, nhắn tin cho Nam" works
//! in one breath. Non-matching speech (game chat, phone calls) is dropped
//! without ever reaching the LLM. A dedicated openWakeWord-style detector can
//! later replace the transcription check behind this same gate API.
//!
//! Env:
//! - `LIVA_WAKE_MODE`         = off | asr_prefix | trained_model   (default off)
//! - `LIVA_WAKE_PHRASES`      = CSV, default "liva,hey liva,ê liva,này liva"
//! - `LIVA_WAKE_WINDOW_SECS`  = seconds the gate stays open (default 45)
//! - `LIVA_WAKE_MODEL_PATHS`  = CSV of classifier .onnx paths (trained_model mode)
//! - `LIVA_WAKE_THRESHOLD`    = per-classifier confidence cutoff (default 0.68)

use crate::wake_model::TrainedWakeDetector;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WakeMode {
    /// Gate disabled: every utterance reaches the pipeline (push-to-talk UX).
    Off,
    /// Transcribe-and-match gate as described in the module docs.
    AsrPrefix,
    /// Continuous streaming classifier trained with the Python
    /// livekit-wakeword toolkit (see `wake_model.rs`) — scans ambient audio
    /// directly, independent of VAD/STT.
    TrainedModel,
}

pub struct WakeGate {
    mode: WakeMode,
    phrases: Vec<String>, // normalized, no spaces
    window: Duration,
    awake_until: Option<Instant>,
    trained_detector: Option<TrainedWakeDetector>,
}

impl WakeGate {
    pub fn from_env() -> Self {
        let mode = match std::env::var("LIVA_WAKE_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "asr_prefix" | "asr" | "on" => WakeMode::AsrPrefix,
            "trained_model" | "trained" | "model" => WakeMode::TrainedModel,
            _ => WakeMode::Off,
        };
        let phrases_raw = std::env::var("LIVA_WAKE_PHRASES")
            .unwrap_or_else(|_| "liva,hey liva,ê liva,này liva".to_string());
        let phrases = phrases_raw
            .split(',')
            .map(|p| normalize_for_match(p).replace(' ', ""))
            .filter(|p| !p.is_empty())
            .collect();
        let window_secs = std::env::var("LIVA_WAKE_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(45u64);

        let trained_detector = if mode == WakeMode::TrainedModel {
            let paths_raw = std::env::var("LIVA_WAKE_MODEL_PATHS").unwrap_or_default();
            let paths: Vec<String> = paths_raw
                .split(',')
                .map(|p| p.trim().to_string())
                .filter(|p| !p.is_empty())
                .collect();
            let threshold = std::env::var("LIVA_WAKE_THRESHOLD")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(0.68f32);
            if paths.is_empty() {
                tracing::error!(
                    "LIVA_WAKE_MODE=trained_model but LIVA_WAKE_MODEL_PATHS is empty"
                );
                None
            } else {
                match TrainedWakeDetector::new(&paths, threshold) {
                    Ok(d) => Some(d),
                    Err(e) => {
                        tracing::error!("Failed to initialize trained wake-word detector: {}", e);
                        None
                    }
                }
            }
        } else {
            None
        };

        Self {
            mode,
            phrases,
            window: Duration::from_secs(window_secs),
            awake_until: None,
            trained_detector,
        }
    }

    /// Feed ambient 16kHz mono audio to the trained classifier (no-op in any
    /// other mode). On a hit the gate opens exactly like `try_wake` and the
    /// classifier name + score is returned for logging.
    pub fn check_streaming(&mut self, samples: &[f32]) -> Option<(String, f32)> {
        let hit = self.trained_detector.as_mut()?.push_and_check(samples)?;
        self.note_activity();
        Some(hit)
    }

    pub fn mode(&self) -> WakeMode {
        self.mode
    }

    pub fn enabled(&self) -> bool {
        self.mode != WakeMode::Off
    }

    /// With the gate disabled this is always true.
    pub fn is_awake(&self) -> bool {
        match self.mode {
            WakeMode::Off => true,
            WakeMode::AsrPrefix | WakeMode::TrainedModel => {
                self.awake_until.is_some_and(|t| Instant::now() < t)
            }
        }
    }

    /// Extend the awake window (call on any user/assistant interaction).
    pub fn note_activity(&mut self) {
        if self.enabled() {
            self.awake_until = Some(Instant::now() + self.window);
        }
    }

    pub fn sleep(&mut self) {
        self.awake_until = None;
    }

    /// Check a transcript for a wake phrase within its opening words. On a
    /// match the gate opens and `true` is returned (the caller should forward
    /// the same utterance onward).
    pub fn try_wake(&mut self, transcript: &str) -> bool {
        let normalized = normalize_for_match(transcript);
        let head: String = normalized
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join("");
        let hit = self.phrases.iter().any(|p| head.contains(p.as_str()));
        if hit {
            self.note_activity();
        }
        hit
    }
}

/// Lowercase, fold Vietnamese diacritics to ASCII, drop everything but
/// letters/digits/spaces, collapse whitespace. ASR variants like "Lì Va" or
/// "li-va" all normalize to a form containing "liva".
pub fn normalize_for_match(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        for lc in c.to_lowercase() {
            let folded = fold_vietnamese_char(lc);
            match folded {
                'a'..='z' | '0'..='9' => out.push(folded),
                _ if folded.is_alphanumeric() => out.push(folded),
                _ => out.push(' '),
            }
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn fold_vietnamese_char(c: char) -> char {
    const TABLES: [(&str, char); 7] = [
        ("àáảãạăằắẳẵặâầấẩẫậ", 'a'),
        ("èéẻẽẹêềếểễệ", 'e'),
        ("ìíỉĩị", 'i'),
        ("òóỏõọôồốổỗộơờớởỡợ", 'o'),
        ("ùúủũụưừứửữự", 'u'),
        ("ỳýỷỹỵ", 'y'),
        ("đ", 'd'),
    ];
    for (set, base) in TABLES {
        if set.contains(c) {
            return base;
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    fn gate(phrases: &str) -> WakeGate {
        WakeGate {
            mode: WakeMode::AsrPrefix,
            phrases: phrases
                .split(',')
                .map(|p| normalize_for_match(p).replace(' ', ""))
                .collect(),
            window: Duration::from_secs(45),
            awake_until: None,
            trained_detector: None,
        }
    }

    #[test]
    fn wakes_on_variants() {
        let mut g = gate("liva,hey liva");
        assert!(g.try_wake("Liva ơi, nhắn tin cho Nam"));
        assert!(g.is_awake());

        let mut g = gate("liva");
        assert!(g.try_wake("Lì Va bật nhạc lên"));
        let mut g = gate("liva");
        assert!(g.try_wake("hey liva what's the weather"));
    }

    #[test]
    fn ignores_unrelated_speech() {
        let mut g = gate("liva");
        assert!(!g.try_wake("cái va li của tôi đâu rồi"));
        assert!(!g.try_wake("đi ăn cơm không"));
        assert!(!g.is_awake());
    }

    #[test]
    fn window_expires() {
        let mut g = gate("liva");
        g.window = Duration::from_millis(1);
        assert!(g.try_wake("liva này"));
        std::thread::sleep(Duration::from_millis(5));
        assert!(!g.is_awake());
    }

    #[test]
    fn off_mode_is_always_awake() {
        let g = WakeGate {
            mode: WakeMode::Off,
            phrases: vec![],
            window: Duration::from_secs(45),
            awake_until: None,
            trained_detector: None,
        };
        assert!(g.is_awake());
        assert!(!g.enabled());
    }
}
