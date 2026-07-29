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
//! - `LIVA_WAKE_MODE`         = off | asr_prefix | trained_model | hybrid (default off)
//! - `LIVA_WAKE_PHRASES`      = CSV, default catches "liva" + common STT mis-hearings
//! - `LIVA_WAKE_WINDOW_SECS`  = seconds the gate stays open (default 45)
//! - `LIVA_WAKE_MODEL_PATHS`  = CSV of classifier .onnx paths (trained_model/hybrid)
//! - `LIVA_WAKE_THRESHOLD`    = per-classifier confidence cutoff (default 0.68)
//!
//! ## Hybrid (recommended for bilingual vi+en)
//! `hybrid` combines both tiers with OR logic: the trained classifier scans
//! continuously (tier 1 — fast, strong on English/clearly-pronounced "liva"),
//! AND when it *misses*, the end-of-utterance transcript is still checked for
//! "liva" (tier 2 — STT, more reliable for Vietnamese, which the
//! English-centric classifier handles poorly). Whichever tier catches it opens
//! the gate, so each covers the other's weak spot. False positives stay low
//! because the classifier's precision is high and the transcript must actually
//! contain "liva".

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
    /// Two-tier: trained classifier (tier 1) OR transcript match (tier 2).
    /// See the module docs — best for bilingual vi+en.
    Hybrid,
}

pub struct WakeGate {
    mode: WakeMode,
    phrases: Vec<String>, // normalized, no spaces
    window: Duration,
    awake_until: Option<Instant>,
    trained_detector: Option<TrainedWakeDetector>,
    /// Ngưỡng confidence cho classifier; giữ lại để `score_clip` nạp lười dùng
    /// đúng con số mà `from_env` đã quyết.
    model_threshold: f32,
    /// Đã thử nạp classifier chưa — chặn việc thử lại mỗi lần probe khi thiếu file.
    detector_load_attempted: bool,
}

/// Classifier mặc định cho đường probe khi `LIVA_WAKE_MODEL_PATHS` để trống.
/// CHỈ bản `en`: `models/README.md` đo `wake_liva_vi.onnx` ở FPPH 19,4 — bật
/// mặc định là chuốc lấy đúng cái lỗi "tự nhảy" đang đi sửa. Ai cần thì thêm
/// bằng env.
const DEFAULT_PROBE_MODEL: &str = "wake_liva_en.onnx";

impl WakeGate {
    pub fn from_env() -> Self {
        let mode = match std::env::var("LIVA_WAKE_MODE")
            .unwrap_or_default()
            .to_lowercase()
            .as_str()
        {
            "asr_prefix" | "asr" | "on" => WakeMode::AsrPrefix,
            "trained_model" | "trained" | "model" => WakeMode::TrainedModel,
            "hybrid" | "both" => WakeMode::Hybrid,
            _ => WakeMode::Off,
        };
        // Default phrases include how Vietnamese STT commonly mis-hears the
        // foreign name "liva" (all diacritic-folded + de-spaced before match,
        // so e.g. "li vào" → "livao" already contains "liva"). Extend via
        // LIVA_WAKE_PHRASES if your voice trips a different spelling.
        // `li vơ` thêm 2026-07-27: đo qua đường probe thật, "Này Liva ơi, bật
        // nhạc lên giúp tôi" được Nemotron nghe thành "Này Li Vơ oi …" ⇒ chuẩn
        // hoá ra `livo`, không chứa `liva`, nên câu đó bị vứt. Bằng chứng mới ở
        // mức giọng Piper tổng hợp; giọng người thật có thể lệch kiểu khác —
        // xem transcript trong sự kiện `wake_probe_rejected` rồi bổ sung qua
        // LIVA_WAKE_PHRASES.
        let phrases_raw = std::env::var("LIVA_WAKE_PHRASES").unwrap_or_else(|_| {
            "liva,hey liva,ê liva,này liva,liva ơi,laiva,leva,lyva,li goa,li vơ".to_string()
        });
        let phrases = phrases_raw
            .split(',')
            .map(|p| normalize_for_match(p).replace(' ', ""))
            .filter(|p| !p.is_empty())
            .collect();
        let window_secs = std::env::var("LIVA_WAKE_WINDOW_SECS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(45u64);

        let trained_detector = if matches!(mode, WakeMode::TrainedModel | WakeMode::Hybrid) {
            Self::load_trained_detector(mode)
        } else {
            // Mode Off/AsrPrefix: đường probe của widget vẫn có thể cần
            // classifier, nhưng nạp lười trong `score_clip` — kết nối không bao
            // giờ probe thì không phải trả tiền tải model.
            None
        };

        Self {
            mode,
            phrases,
            window: Duration::from_secs(window_secs),
            awake_until: None,
            trained_detector,
            model_threshold: Self::model_threshold_from_env(),
            detector_load_attempted: false,
        }
    }

    /// Ngưỡng confidence của classifier (`LIVA_WAKE_THRESHOLD`, mặc định 0,68).
    pub fn model_threshold(&self) -> f32 {
        self.model_threshold
    }

    fn model_threshold_from_env() -> f32 {
        std::env::var("LIVA_WAKE_THRESHOLD")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0.68f32)
    }

    /// Nạp classifier theo `LIVA_WAKE_MODEL_PATHS`; để trống thì thử
    /// [`DEFAULT_PROBE_MODEL`] ở các vị trí `models/` quen thuộc.
    fn load_trained_detector(mode: WakeMode) -> Option<TrainedWakeDetector> {
        let paths_raw = std::env::var("LIVA_WAKE_MODEL_PATHS").unwrap_or_default();
        let mut paths: Vec<String> = paths_raw
            .split(',')
            .map(|p| p.trim().to_string())
            .filter(|p| !p.is_empty())
            .collect();

        if paths.is_empty() {
            // BẮT BUỘC qua `resolve_resource_path`: `tauri dev` chạy core với cwd
            // `liva-desktop/src-tauri`, không phải gốc repo, nên `models/...`
            // trần sẽ trượt. Đúng lỗi đã làm Piper biến mất khỏi danh sách TTS
            // hôm 27/07/2026 — triệu chứng đọc như thiếu model, thực ra là cwd.
            let candidate =
                crate::resolve_resource_path(&format!("models/{}", DEFAULT_PROBE_MODEL));
            if candidate.exists() {
                paths.push(candidate.to_string_lossy().into_owned());
            }
        }

        if paths.is_empty() {
            match mode {
                // In pure trained_model an empty list means the gate can never open.
                WakeMode::TrainedModel => tracing::error!(
                    "LIVA_WAKE_MODE=trained_model nhưng không tìm được classifier nào (LIVA_WAKE_MODEL_PATHS trống và không thấy {})",
                    DEFAULT_PROBE_MODEL
                ),
                // In hybrid this is fine — tier 2 (STT) still gates.
                WakeMode::Hybrid => tracing::warn!(
                    "LIVA_WAKE_MODE=hybrid không có classifier — chỉ chạy STT (tầng 2)"
                ),
                _ => tracing::warn!(
                    "Không tìm thấy {} — cổng đánh thức của widget chỉ còn dựa vào STT",
                    DEFAULT_PROBE_MODEL
                ),
            }
            return None;
        }

        match TrainedWakeDetector::new(&paths, Self::model_threshold_from_env()) {
            Ok(d) => {
                tracing::info!("Wake classifier đã nạp: {}", paths.join(", "));
                Some(d)
            }
            Err(e) => {
                tracing::error!("Failed to initialize trained wake-word detector: {}", e);
                None
            }
        }
    }

    /// Feed ambient 16kHz mono audio to the trained classifier (no-op in any
    /// other mode). On a hit the gate opens exactly like `try_wake` and the
    /// classifier name + score is returned for logging.
    pub fn check_streaming(&mut self, samples: &[f32]) -> Option<(String, f32)> {
        // `uses_model()` là bắt buộc: từ khi `score_clip` nạp detector theo kiểu
        // lười cho đường probe của widget, `trained_detector` có thể tồn tại
        // ngay cả ở mode Off. Thiếu chốt này thì quét streaming sẽ tự mở gate
        // trong đúng cái mode mà hợp đồng nói là "gate trong suốt".
        if !self.uses_model() {
            return None;
        }
        let hit = self.trained_detector.as_mut()?.push_and_check(samples)?;
        self.note_activity();
        Some(hit)
    }

    /// Chấm điểm MỘT clip bằng classifier đã train — một phát, không đụng vòng
    /// đệm streaming, không mở gate. Dành cho `OP_WAKE_PROBE`.
    ///
    /// Vì sao tồn tại song song với so-cụm-từ bằng STT: hai tầng hỏng độc lập
    /// nhau. Đường STT (`transcribe_for_wake`) **nhạy với cách clip rơi vào
    /// biên chunk** — đo 2026-07-27 trên cùng một nội dung dịch đầu 0/60/120/
    /// 200/300 ms cho ra chữ / rỗng / rỗng / rỗng / chữ, tái lập y hệt qua các
    /// lần chạy. Classifier chạy trên cửa sổ mel 2,5 s nên không dính kiểu hỏng
    /// đó. Một trong hai bắt được là đủ.
    ///
    /// Nạp lười: mô hình chỉ tải ở lần probe đầu, nên kết nối không bao giờ
    /// probe (Telegram, e2e) không phải trả ~70 ms + vài MB.
    pub fn score_clip(&mut self, audio: &[f32]) -> Option<(String, f32)> {
        if self.trained_detector.is_none() && !self.detector_load_attempted {
            // Chỉ thử MỘT lần. Thiếu file thì mọi probe sau đó sẽ lặp lại đúng
            // một lần mở file hỏng và một dòng log — mỗi câu nói một lần.
            self.detector_load_attempted = true;
            self.trained_detector = Self::load_trained_detector(self.mode);
        }
        let detector = self.trained_detector.as_mut()?;
        match detector.predict_raw(audio) {
            // Trả điểm CAO NHẤT bất kể ngưỡng; nơi gọi tự so với
            // `model_threshold()`. Lọc ngay tại đây thì một lần trượt sát
            // (0,64 so với ngưỡng 0,68) và một lần trượt xa (0,02) đều ra
            // `None` y hệt nhau — mà đó lại đúng là con số cần để biết nên
            // chỉnh ngưỡng hay phải đổi cách khác.
            Ok(scores) => scores.into_iter().max_by(|a, b| a.1.total_cmp(&b.1)),
            Err(e) => {
                tracing::error!("Wake probe classifier failed: {}", e);
                None
            }
        }
    }

    pub fn mode(&self) -> WakeMode {
        self.mode
    }

    /// Whether this mode confirms wakes via the end-of-utterance transcript
    /// (tier 2). True for AsrPrefix and Hybrid — the caller should run STT on
    /// a while-asleep utterance and pass the text to `try_wake`.
    pub fn uses_stt_confirm(&self) -> bool {
        matches!(self.mode, WakeMode::AsrPrefix | WakeMode::Hybrid)
    }

    /// Whether this mode runs the streaming classifier (tier 1). True for
    /// TrainedModel and Hybrid.
    pub fn uses_model(&self) -> bool {
        matches!(self.mode, WakeMode::TrainedModel | WakeMode::Hybrid)
    }

    pub fn enabled(&self) -> bool {
        self.mode != WakeMode::Off
    }

    /// With the gate disabled this is always true.
    pub fn is_awake(&self) -> bool {
        match self.mode {
            WakeMode::Off => true,
            WakeMode::AsrPrefix | WakeMode::TrainedModel | WakeMode::Hybrid => {
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
        let hit = self.matches_phrase(transcript);
        if hit {
            self.note_activity();
        }
        hit
    }

    /// So cụm từ thuần tuý — không mở gate, không phụ thuộc `mode`.
    ///
    /// Tách khỏi [`Self::try_wake`] cho đường `OP_WAKE_PROBE`: widget trình duyệt
    /// tự giữ trạng thái thức/ngủ của nó, chỉ hỏi core đúng một câu "câu này có
    /// chứa cụm đánh thức không?". Nếu dùng `try_wake` ở đó thì một lần widget
    /// đánh thức sẽ mở luôn gate phía server 45 giây — biến mọi tiếng nói kế tiếp
    /// trong phòng thành lượt hội thoại thật, đúng cái lỗi đang đi sửa.
    ///
    /// Mode `Off` vẫn so được: `phrases` luôn được nạp trong `from_env`, không
    /// phụ thuộc mode.
    pub fn matches_phrase(&self, transcript: &str) -> bool {
        let normalized = normalize_for_match(transcript);
        let head: String = normalized
            .split_whitespace()
            .take(8)
            .collect::<Vec<_>>()
            .join("");
        self.phrases.iter().any(|p| head.contains(p.as_str()))
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
            model_threshold: 0.68,
            detector_load_attempted: false,
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
            model_threshold: 0.68,
            detector_load_attempted: false,
        };
        assert!(g.is_awake());
        assert!(!g.enabled());
    }

    fn gate_mode(mode: WakeMode) -> WakeGate {
        WakeGate {
            mode,
            phrases: vec!["liva".to_string()],
            window: Duration::from_secs(45),
            awake_until: None,
            trained_detector: None,
            model_threshold: 0.68,
            detector_load_attempted: false,
        }
    }

    #[test]
    fn hybrid_uses_both_tiers() {
        let g = gate_mode(WakeMode::Hybrid);
        assert!(g.uses_model(), "hybrid must run the classifier (tier 1)");
        assert!(g.uses_stt_confirm(), "hybrid must run STT confirm (tier 2)");
        assert!(g.enabled());
    }

    #[test]
    fn tier_selection_per_mode() {
        // asr_prefix: STT only
        let a = gate_mode(WakeMode::AsrPrefix);
        assert!(a.uses_stt_confirm() && !a.uses_model());
        // trained_model: classifier only
        let t = gate_mode(WakeMode::TrainedModel);
        assert!(t.uses_model() && !t.uses_stt_confirm());
    }

    /// `matches_phrase` là đường của `OP_WAKE_PROBE`: trả lời đúng/sai mà TUYỆT
    /// ĐỐI không mở gate. Nếu nó mở, mỗi lần widget đánh thức sẽ kéo theo cửa sổ
    /// awake 45 s phía server và mọi câu nói sau đó vào thẳng LLM.
    #[test]
    fn matches_phrase_khong_mo_gate() {
        let g = gate("liva,hey liva");
        assert!(g.matches_phrase("Hey Liva bật nhạc lên"));
        assert!(!g.matches_phrase("cái va li của tôi đâu rồi"));
        assert!(!g.is_awake(), "so cum tu KHONG duoc mo gate");
    }

    /// Widget probe phải chạy được cả khi gate server tắt (mặc định) — phrases
    /// nạp trong `from_env` không phụ thuộc mode.
    #[test]
    fn matches_phrase_van_chay_khi_mode_off() {
        let g = WakeGate {
            mode: WakeMode::Off,
            phrases: vec!["liva".to_string()],
            window: Duration::from_secs(45),
            awake_until: None,
            trained_detector: None,
            model_threshold: 0.68,
            detector_load_attempted: false,
        };
        assert!(g.matches_phrase("liva oi"));
        assert!(!g.matches_phrase("di an com khong"));
    }

    #[test]
    fn hybrid_tier2_catches_vietnamese_mishearing() {
        // Tier-1 classifier missed (no detector here); tier-2 STT transcript
        // "li vào" is how Vietnamese STT mis-hears "liva" — must still wake.
        let mut g = gate_mode(WakeMode::Hybrid);
        assert!(g.try_wake("li vào hôm nay thời tiết thế nào"));
        assert!(g.is_awake());
    }
}
