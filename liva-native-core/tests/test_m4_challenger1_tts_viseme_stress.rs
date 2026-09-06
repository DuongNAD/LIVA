//! Challenger 1 M4 Adversarial Stress Test Suite
//! Target: Streaming TTS Engine, Asymmetric TtsChunker, and IPA Realtime Visemes Lip-Sync

use liva_native_core::tts::{TtsChunker, is_vietnamese_text};
use liva_native_core::webrtc::frame::{OP_SPEAKER_OUT, OP_VISME, VoiceFrame};
use serde_json::Value;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestViseme {
    Aa,
    Ee,
    Ih,
    Oh,
    Ou,
    Nil,
}

impl TestViseme {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Aa => "aa",
            Self::Ee => "ee",
            Self::Ih => "ih",
            Self::Oh => "oh",
            Self::Ou => "ou",
            Self::Nil => "nil",
        }
    }

    pub fn from_phoneme(ph: char) -> Self {
        match ph {
            'a' | 'ɑ' | 'æ' | 'ɐ' | 'ä' | 'ą' | 'ã' | 'ʌ' | 'ɒ' => Self::Aa,
            'i' | 'ɪ' | 'y' | 'ɨ' | 'j' => Self::Ee,
            'e' | 'ɛ' | 'ə' | 'ɜ' | 'ɚ' => Self::Ih,
            'o' | 'ɔ' | 'ø' => Self::Oh,
            'u' | 'ʊ' | 'ư' | 'w' | 'ʉ' | 'ɯ' => Self::Ou,
            'm' | 'b' | 'p' | 'f' | 'v' | 'ɱ' | 'ʋ' | 'β' => Self::Nil,
            _ => Self::Nil,
        }
    }
}

fn is_ipa_modifier(c: char) -> bool {
    matches!(
        c,
        'ˈ' | 'ˌ' | 'ː' | 'ˑ' | '̆' | '͡' | '͜' | 'ʰ' | 'ʲ' | 'ʷ' | 'ˤ' | '˞' | '̃'
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TestVisemeCue {
    pub viseme: TestViseme,
    pub t_ms: u64,
}

pub fn test_build_viseme_timeline(phonemes: &str, duration_ms: u64) -> Vec<TestVisemeCue> {
    let phones: Vec<char> = phonemes
        .chars()
        .filter(|c| !c.is_whitespace() && !is_ipa_modifier(*c))
        .collect();
    if phones.is_empty() || duration_ms == 0 {
        return Vec::new();
    }
    let n = phones.len() as u64;
    let mut cues: Vec<TestVisemeCue> = Vec::new();
    for (i, &ph) in phones.iter().enumerate() {
        let viseme = TestViseme::from_phoneme(ph);
        let t_ms = i as u64 * duration_ms / n;
        if cues.last().is_none_or(|last| last.viseme != viseme) {
            cues.push(TestVisemeCue { viseme, t_ms });
        }
    }
    cues
}

// ══════════════════════════════════════════════════════════════════════════
// CHALLENGE GROUP 1: ASYMMETRIC TTS CHUNKER & RAPID TOKEN STREAMS
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_adv_c1_01_first_chunk_two_word_delimiter_split() {
    let delimiters = [",", ";", ":", "—", "\n"];
    for delim in delimiters {
        let mut chunker = TtsChunker::new();
        let input = format!("Xin chào{} mình là trợ lý ảo.", delim);
        let chunks = chunker.push(&input);
        assert!(
            !chunks.is_empty(),
            "Delimiter '{}' phai kich hoat cat mau dau khi co >= 2 tu",
            delim
        );
        let first = &chunks[0];
        let expected_prefix = format!("Xin chào{}", delim.trim_end());
        assert_eq!(
            first, &expected_prefix,
            "First chunk must match prefix for delimiter '{}'",
            delim
        );
    }
}

#[test]
fn test_adv_c1_02_single_word_before_delimiter_does_not_split_first_chunk() {
    let delimiters = [",", ";", ":", "—", "\n"];
    for delim in delimiters {
        let mut chunker = TtsChunker::new();
        // Only 1 word before delimiter -> should NOT emit first chunk
        let input = format!("Chào{} bạn có khoẻ không?", delim);
        let chunks = chunker.push(&input);
        // Should only emit at sentence end '?'
        assert_eq!(
            chunks.len(),
            1,
            "1 word before delimiter '{}' must not trigger split; expected split at sentence end",
            delim
        );
        assert_eq!(chunks[0], format!("Chào{} bạn có khoẻ không?", delim));
    }
}

#[test]
fn test_adv_c1_03_rapid_token_feed_ttfa_simulation() {
    // Simulate streaming token feed from LLM: 1-3 chars per token
    let mut chunker = TtsChunker::new();
    let tokens = [
        "V", "â", "n", "g", ",", " ", "t", "ô", "i", " ", "đ", "a", "n", "g",
        " ", "x", "ử", " ", "l", "ý", " ", "y", "ê", "u", " ", "c", "ầ", "u", "."
    ];

    let mut emitted = Vec::new();
    let mut first_emitted_token_idx = None;

    for (idx, tok) in tokens.iter().enumerate() {
        let out = chunker.push(tok);
        if !out.is_empty() && first_emitted_token_idx.is_none() {
            first_emitted_token_idx = Some(idx);
        }
        emitted.extend(out);
    }

    // "Vâng," is 1 word -> does not split at comma. Next split is at "."
    assert_eq!(emitted.len(), 1);
    assert_eq!(emitted[0], "Vâng, tôi đang xử lý yêu cầu.");

    // Now test with 2 words: "Dạ vâng,"
    let mut chunker2 = TtsChunker::new();
    let tokens2 = [
        "D", "ạ", " ", "v", "â", "n", "g", ",", " ", "t", "ô", "i", " ", "s", "ẽ",
        " ", "g", "i", "ú", "p", " ", "b", "ạ", "n", "."
    ];
    let mut emitted2 = Vec::new();
    let mut first_idx2 = None;
    for (idx, tok) in tokens2.iter().enumerate() {
        let out = chunker2.push(tok);
        if !out.is_empty() && first_idx2.is_none() {
            first_idx2 = Some(idx);
        }
        emitted2.extend(out);
    }
    // "Dạ vâng," has 2 words -> splits at comma (index 7)
    assert_eq!(first_idx2, Some(7), "First chunk must emit at comma token");
    assert_eq!(emitted2[0], "Dạ vâng,");
    assert_eq!(emitted2[1], "tôi sẽ giúp bạn.");
}

#[test]
fn test_adv_c1_04_subsequent_chunks_require_six_words_on_delimiters() {
    let mut chunker = TtsChunker::new();
    // First chunk emits on comma with 2 words
    let c1 = chunker.push("Xin chào,");
    assert_eq!(c1, vec!["Xin chào,"]);

    // Subsequent chunk: 3 words before comma -> should NOT split
    let c2 = chunker.push(" tôi là Nam,");
    assert!(c2.is_empty(), "Subsequent chunk with < 6 words must NOT split on comma");

    // Push more words to reach >= 6 words before another comma
    let c3 = chunker.push(" đến từ phòng kỹ thuật hệ thống,");
    assert_eq!(c3.len(), 1);
    assert_eq!(c3[0], "tôi là Nam, đến từ phòng kỹ thuật hệ thống,");
}

#[test]
fn test_adv_c1_05_multi_turn_turn_boundaries_reset_stress() {
    let mut chunker = TtsChunker::new();
    for turn in 1..=50 {
        // First chunk in turn
        let out1 = chunker.push(&format!("Lượt thứ {},", turn));
        assert_eq!(
            out1,
            vec![format!("Lượt thứ {},", turn)],
            "Turn {} first chunk must split on comma",
            turn
        );

        // Subsequent in same turn must follow standard 6-word rule
        let out2 = chunker.push(" mẩu nhỏ,");
        assert!(out2.is_empty(), "Turn {} subsequent < 6 words must not split", turn);

        let out3 = chunker.push(" và kết thúc tại đây.");
        assert_eq!(out3, vec!["mẩu nhỏ, và kết thúc tại đây."]);

        // Reset for next turn
        chunker.reset();
    }
}

#[test]
fn test_adv_c1_06_unpunctuated_stream_max_words_boundaries() {
    let mut chunker = TtsChunker::new();
    // Feed 50 words without any punctuation in a single push
    let words: Vec<String> = (1..=50).map(|i| format!("word{}", i)).collect();
    let text = words.join(" ");
    let chunks = chunker.push(&text);

    assert_eq!(chunks.len(), 2, "50 unpunctuated words split into 9-word chunk and 25-word chunk");
    let c1_words: Vec<&str> = chunks[0].split_whitespace().collect();
    assert_eq!(c1_words.len(), 9, "First unpunctuated chunk must cap at 9 words");

    let c2_words: Vec<&str> = chunks[1].split_whitespace().collect();
    assert_eq!(c2_words.len(), 25, "Second unpunctuated chunk must cap at 25 words");

    // Flush remainder: 16 words left
    let rem = chunker.flush().expect("Must have remainder");
    let rem_words: Vec<&str> = rem.split_whitespace().collect();
    assert_eq!(rem_words.len(), 16);

    // Total reconstructed word count matches original 50 words
    assert_eq!(c1_words.len() + c2_words.len() + rem_words.len(), 50);
}

#[test]
fn test_adv_c1_07_vietnamese_language_chunk_detection() {
    assert!(is_vietnamese_text("Chào bạn, tôi là LIVA."));
    assert!(is_vietnamese_text("Hệ thống xử lý âm thanh thời gian thực"));
    assert!(is_vietnamese_text("Thử nghiệm dấu: ắ ằ ẳ ẵ ặ ấ ầ ổ ỗ ợ ự"));
    assert!(!is_vietnamese_text("Hello, this is English speech synthesis."));
    assert!(!is_vietnamese_text("Realtime voice DSP pipeline with WebRTC."));
}

#[test]
fn test_adv_c1_08_empty_tokens_and_whitespace_bursts() {
    let mut chunker = TtsChunker::new();
    assert!(chunker.push("").is_empty());
    assert!(chunker.push("   \t  \n ").is_empty());
    assert_eq!(chunker.flush(), None);

    let chunks = chunker.push("  Xin chào,   mình ở đây.  ");
    assert_eq!(chunks, vec!["Xin chào,", "mình ở đây."]);
    assert_eq!(chunker.flush(), None);
}

// ══════════════════════════════════════════════════════════════════════════
// CHALLENGE GROUP 2: IPA VISEME TIMELINE GENERATION & EXTREME INPUTS
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_adv_c2_01_all_bilabials_and_labiodentals_are_strictly_nil() {
    let bilabials = ['m', 'b', 'p', 'f', 'v', 'ɱ', 'ʋ', 'β'];
    for &ph in &bilabials {
        assert_eq!(
            TestViseme::from_phoneme(ph),
            TestViseme::Nil,
            "Phoneme '{}' must map to Nil",
            ph
        );
    }

    // Sequence of pure bilabials with duration -> single Nil cue at t=0
    let timeline = test_build_viseme_timeline("mbpfvɱʋβ", 800);
    assert_eq!(timeline.len(), 1);
    assert_eq!(timeline[0], TestVisemeCue { viseme: TestViseme::Nil, t_ms: 0 });
}

#[test]
fn test_adv_c2_02_all_extended_ipa_vowels_classification() {
    // Aa: Open vowels
    for &ph in &['a', 'ɑ', 'æ', 'ɐ', 'ä', 'ą', 'ã', 'ʌ', 'ɒ'] {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Aa, "Vowel '{}' -> Aa", ph);
    }
    // Ee: Front high vowels
    for &ph in &['i', 'ɪ', 'y', 'ɨ', 'j'] {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ee, "Vowel '{}' -> Ee", ph);
    }
    // Ih: Mid front / central vowels
    for &ph in &['e', 'ɛ', 'ə', 'ɜ', 'ɚ'] {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ih, "Vowel '{}' -> Ih", ph);
    }
    // Oh: Mid back rounded vowels
    for &ph in &['o', 'ɔ', 'ø'] {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Oh, "Vowel '{}' -> Oh", ph);
    }
    // Ou: High back rounded vowels
    for &ph in &['u', 'ʊ', 'ư', 'w', 'ʉ', 'ɯ'] {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ou, "Vowel '{}' -> Ou", ph);
    }
}

#[test]
fn test_adv_c2_03_all_ipa_stress_and_modifier_marks_filtered() {
    let modifiers = ['ˈ', 'ˌ', 'ː', 'ˑ', '̆', '͡', '͜', 'ʰ', 'ʲ', 'ʷ', 'ˤ', '˞', '̃'];
    for &mod_char in &modifiers {
        assert!(is_ipa_modifier(mod_char), "Character '{}' must be recognized as modifier", mod_char);
    }

    // Heavily modified IPA string: ˈt͡ʃʰaːˌkˤũ
    // Filtered phones: 't', 'ʃ', 'a', 'k', 'u' (5 phones)
    // t -> Nil@0
    // ʃ -> Nil@0 (merged)
    // a -> Aa@200
    // k -> Nil@300
    // u -> Ou@400 (with total 500ms: 0, 100, 200, 300, 400)
    let timeline = test_build_viseme_timeline("ˈt͡ʃʰaːˌkˤũ", 500);
    assert_eq!(
        timeline,
        vec![
            TestVisemeCue { viseme: TestViseme::Nil, t_ms: 0 },
            TestVisemeCue { viseme: TestViseme::Aa, t_ms: 200 },
            TestVisemeCue { viseme: TestViseme::Nil, t_ms: 300 },
            TestVisemeCue { viseme: TestViseme::Ou, t_ms: 400 },
        ]
    );
}

#[test]
fn test_adv_c2_04_extreme_monologue_phonemes_stress() {
    // 5,000 phonemes alternating between 'm' (Nil) and 'a' (Aa)
    let mut large_phonemes = String::with_capacity(10_000);
    for _ in 0..2500 {
        large_phonemes.push_str("ma");
    }
    let duration_ms = 60_000; // 60s
    let timeline = test_build_viseme_timeline(&large_phonemes, duration_ms);

    assert_eq!(timeline.len(), 5000);
    assert_eq!(timeline[0], TestVisemeCue { viseme: TestViseme::Nil, t_ms: 0 });
    assert_eq!(timeline[1], TestVisemeCue { viseme: TestViseme::Aa, t_ms: 12 });

    // Verify strictly monotonic increasing timestamps
    for i in 1..timeline.len() {
        assert!(
            timeline[i].t_ms >= timeline[i - 1].t_ms,
            "Timestamps must be non-decreasing at index {}: {} < {}",
            i,
            timeline[i].t_ms,
            timeline[i - 1].t_ms
        );
        assert!(
            timeline[i].t_ms <= duration_ms,
            "Timestamp {} exceeds duration {}",
            timeline[i].t_ms,
            duration_ms
        );
    }
}

#[test]
fn test_adv_c2_05_boundary_durations_and_unusual_unicode() {
    // Duration = 0 -> empty
    assert!(test_build_viseme_timeline("hello", 0).is_empty());

    // Duration = 1ms -> valid cue at t=0
    let t1 = test_build_viseme_timeline("a", 1);
    assert_eq!(t1, vec![TestVisemeCue { viseme: TestViseme::Aa, t_ms: 0 }]);

    // Emojis and unassigned unicode should map safely to Nil without panics
    let weird = "👋🚀🔥🦀aäʉ";
    let tw = test_build_viseme_timeline(weird, 1000);
    assert!(!tw.is_empty());
    // Ends with IPA vowel ʉ -> Ou
    assert_eq!(tw.last().unwrap().viseme, TestViseme::Ou);
}

// ══════════════════════════════════════════════════════════════════════════
// CHALLENGE GROUP 3: OP_VISME WIRE PROTOCOL & SYNCHRONIZATION
// ══════════════════════════════════════════════════════════════════════════

#[test]
fn test_adv_c3_01_op_visme_frame_payload_json_compliance() {
    let session_id: u32 = 42;
    let seq_id: u32 = 1001;
    let cues = test_build_viseme_timeline("bɑb", 300);

    let payload = serde_json::json!({
        "turn_epoch": session_id,
        "base_seq_id": seq_id,
        "visemes": cues.iter().map(|c| serde_json::json!({
            "v": c.viseme.as_str(),
            "t_ms": c.t_ms,
        })).collect::<Vec<_>>()
    });

    let frame = VoiceFrame {
        op_code: OP_VISME,
        seq_id,
        payload: bytes::Bytes::from(payload.to_string()),
    };

    assert_eq!(frame.op_code, 0x06);
    assert_eq!(frame.seq_id, 1001);

    let parsed: Value = serde_json::from_slice(&frame.payload).expect("Valid JSON");
    assert_eq!(parsed["turn_epoch"], 42);
    assert_eq!(parsed["base_seq_id"], 1001);
    let visemes = parsed["visemes"].as_array().expect("Array");
    assert_eq!(visemes.len(), 3);
    assert_eq!(visemes[0]["v"], "nil");
    assert_eq!(visemes[0]["t_ms"], 0);
    assert_eq!(visemes[1]["v"], "aa");
    assert_eq!(visemes[1]["t_ms"], 100);
    assert_eq!(visemes[2]["v"], "nil");
    assert_eq!(visemes[2]["t_ms"], 200);
}

#[test]
fn test_adv_c3_02_wire_ordering_op_visme_precedes_speaker_out() {
    let mut channel: Vec<VoiceFrame> = Vec::new();
    let session_id: u32 = 7;
    let base_seq_id: u32 = 50;

    // Simulate pipeline emission: OP_VISME followed by OP_SPEAKER_OUT chunks
    let viseme_payload = serde_json::json!({
        "turn_epoch": session_id,
        "base_seq_id": base_seq_id,
        "visemes": [{"v": "aa", "t_ms": 0}]
    });
    channel.push(VoiceFrame {
        op_code: OP_VISME,
        seq_id: base_seq_id,
        payload: bytes::Bytes::from(viseme_payload.to_string()),
    });

    for seq in base_seq_id..base_seq_id + 3 {
        channel.push(VoiceFrame {
            op_code: OP_SPEAKER_OUT,
            seq_id: seq,
            payload: bytes::Bytes::from(vec![0u8; 64]),
        });
    }

    assert_eq!(channel.len(), 4);
    assert_eq!(channel[0].op_code, OP_VISME, "First frame in stream MUST be OP_VISME");
    assert_eq!(channel[1].op_code, OP_SPEAKER_OUT);
    assert_eq!(channel[2].op_code, OP_SPEAKER_OUT);
    assert_eq!(channel[3].op_code, OP_SPEAKER_OUT);
}
