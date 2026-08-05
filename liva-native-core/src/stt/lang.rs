//! Language-locale mapping for the Nemotron cache-aware multilingual ASR.
//!
//! The encoder takes a `lang_id` input that conditions transcription on a
//! locale. The model ships NO explicit id table, and the id space follows
//! neither the tokenizer's locale-token order nor alphabetical order — the
//! mapping below was determined **empirically** with the `stt_lang_probe`
//! bin (2026-07-03 sweep of all 39 ids against known-language samples):
//!
//! - Vietnamese sample: id 33 produced the only complete, correct transcript
//!   ("Xin chào, tôi là trợ lý ảo Liva …") and the model emitted `<vi-VN>`.
//! - English sample: id 0 produced a perfect transcript and emitted `<en-US>`.
//! - Neighbouring ids partially work (the conditioning is soft), distant ids
//!   return empty or transliterated output.
//!
//! To add a language: get a short sample of it, run
//! `stt_lang_probe models/nemotron-asr <sample> 0,1,…,38`, pick the id with
//! the best transcript, and add it to [`VERIFIED_LANG_IDS`].

/// Empirically verified (locale, encoder lang_id) pairs.
pub const VERIFIED_LANG_IDS: [(&str, i64); 2] = [
    ("vi-VN", 33), // verified 2026-07-03, sample_vi-VN-HoaiMyNeural.mp3
    ("en-US", 0),  // verified 2026-07-03, sample_en-US-AriaNeural.mp3
];

/// Product default: Vietnamese (bilingual vi/en product, vi primary).
pub const DEFAULT_LANGUAGE: &str = "vi";

/// Encoder lang_id for [`DEFAULT_LANGUAGE`].
pub const DEFAULT_LANG_ID: i64 = 33;

/// Resolve a language code ("vi", "vi-VN", "en", "en_us", …) to a verified
/// encoder lang_id. Unverified locales return `None` — probe them first
/// (see module docs) rather than guessing an id.
pub fn lang_id_for(code: &str) -> Option<i64> {
    let norm = code.trim().to_lowercase().replace('_', "-");
    if norm.is_empty() {
        return None;
    }
    let bare = norm.split('-').next().unwrap_or(&norm);

    for (locale, id) in VERIFIED_LANG_IDS {
        let locale_lower = locale.to_lowercase();
        if norm == locale_lower || bare == locale_lower.split('-').next().unwrap_or("") {
            return Some(id);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_vietnamese_and_english() {
        assert_eq!(lang_id_for("vi"), Some(33));
        assert_eq!(lang_id_for("vi-VN"), Some(33));
        assert_eq!(lang_id_for("vi_vn"), Some(33));
        assert_eq!(lang_id_for("en"), Some(0));
        assert_eq!(lang_id_for("en-US"), Some(0));
        assert_eq!(lang_id_for("EN-GB"), Some(0)); // falls back to bare "en"
        assert!(super::super::prefers_parakeet_vi(None));
        assert!(super::super::prefers_parakeet_vi(Some("")));
        assert!(super::super::prefers_parakeet_vi(Some("   ")));
        assert!(super::super::prefers_parakeet_vi(Some("parakeet")));
        assert!(!super::super::prefers_parakeet_vi(Some("nemotron")));
        assert!(!super::super::prefers_parakeet_vi(Some(" NEMOTRON ")));
    }

    #[test]
    fn rejects_unverified() {
        assert_eq!(lang_id_for("ja"), None);
        assert_eq!(lang_id_for("xx"), None);
        assert_eq!(lang_id_for(""), None);
    }
}
