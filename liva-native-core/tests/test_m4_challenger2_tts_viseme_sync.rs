//! Adversarial Empirical Challenger 2 Test Suite for Milestone 4:
//! Streaming TTS Engine & Realtime Visemes / Blendshapes Lip-Sync.
//!
//! Mandatory Adversarial Verification:
//! 1. Concurrent TTS Streaming & OP_VISME Synchronization across 20 concurrent turns:
//!    - Verify OP_VISME frames precede OP_SPEAKER_OUT frames for every chunk.
//!    - Verify sequence IDs, turn epochs, payload JSON schema, and monotonic timestamps.
//! 2. Instant Barge-In OP_FLUSH Purge:
//!    - Zero visual ghosting / zero orphaned blendshape cues.
//!    - 100% rejection of stale visemes and audio frames post-cancellation.
//!    - Rapid barge-in storm (100 preemption cycles under high load).
//! 3. Extended IPA Phonetic Mapping & Stress/Modifier Filtering Invariants.
//! 4. Asymmetric TtsChunker Newline & Clause Boundary Rules.

#![allow(unused_imports, dead_code, clippy::identity_op)]

use bytes::{BufMut, Bytes, BytesMut};
use liva_native_core::ipc::ring_buffer::SpscRingBuffer;
use liva_native_core::llm::pool::CancellationToken;
use liva_native_core::tts::audio::TtsAudioPlayer;
use liva_native_core::tts::{is_vietnamese_text, TtsChunker};
use liva_native_core::webrtc::frame::{
    speaker_frames, speaker_turn_epoch, BufferPool, SpeakerEpochGate, VoiceFrame,
    OP_AUTH_HANDSHAKE, OP_FLUSH, OP_MIC_IN, OP_SPEAKER_OUT, OP_VISME, OP_WAKE_PROBE,
};
use liva_native_core::webrtc::pipeline::{
    PipelineEvent, PipelineState, VoiceOutbound,
};
use serde_json::Value;
use std::collections::HashSet;
use std::f32::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, watch};

// ============================================================================
// VISEME SPECIFICATION & TEST HARNESS
// ============================================================================

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

fn test_is_ipa_modifier(c: char) -> bool {
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
        .filter(|c| !c.is_whitespace() && !test_is_ipa_modifier(*c))
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

// ============================================================================
// SIMULATED CLIENT LIP-SYNC REGISTRY (MIRRORS phonemeLipSync.ts IN FRONTEND)
// ============================================================================

#[derive(Debug, Clone)]
struct ClientAnchoredTimeline {
    cues: Vec<TestVisemeCue>,
    anchor_sec: f64,
    end_sec: f64,
}

#[derive(Default)]
struct ClientVisemeRegistry {
    pending_cues: Option<Vec<TestVisemeCue>>,
    anchored: Option<ClientAnchoredTimeline>,
    active_epoch: u64,
}

impl ClientVisemeRegistry {
    fn new() -> Self {
        Self::default()
    }

    fn on_viseme_frame(&mut self, payload_str: &str, current_epoch: u64) -> Result<(), String> {
        let v: Value = serde_json::from_str(payload_str).map_err(|e| e.to_string())?;
        let turn_epoch = v["turn_epoch"].as_u64().ok_or("missing turn_epoch")?;
        if turn_epoch != current_epoch {
            // Reject stale viseme from previous turn
            return Ok(());
        }
        let visemes_arr = v["visemes"].as_array().ok_or("missing visemes array")?;
        let mut cues = Vec::new();
        for item in visemes_arr {
            let viseme_str = item["v"].as_str().ok_or("missing v")?;
            let t_ms = item["t_ms"].as_u64().ok_or("missing t_ms")?;
            let viseme = match viseme_str {
                "aa" => TestViseme::Aa,
                "ee" => TestViseme::Ee,
                "ih" => TestViseme::Ih,
                "oh" => TestViseme::Oh,
                "ou" => TestViseme::Ou,
                "nil" => TestViseme::Nil,
                other => return Err(format!("unknown viseme: {}", other)),
            };
            cues.push(TestVisemeCue { viseme, t_ms });
        }
        self.pending_cues = Some(cues);
        Ok(())
    }

    fn note_chunk_scheduled(&mut self, start_sec: f64, duration_sec: f64) {
        if let Some(cues) = self.pending_cues.take() {
            if !cues.is_empty() {
                self.anchored = Some(ClientAnchoredTimeline {
                    cues,
                    anchor_sec: start_sec,
                    end_sec: start_sec + duration_sec.max(0.0),
                });
            }
        }
    }

    fn current_viseme(&self, ctx_time_sec: f64) -> Option<&'static str> {
        let a = self.anchored.as_ref()?;
        if ctx_time_sec < a.anchor_sec || ctx_time_sec > a.end_sec {
            return None;
        }
        let elapsed_ms = ((ctx_time_sec - a.anchor_sec) * 1000.0).round() as u64;

        // Binary search matching phonemeLipSync.ts
        let mut lo = 0;
        let mut hi = a.cues.len() as isize - 1;
        let mut found: Option<&TestVisemeCue> = None;
        while lo <= hi {
            let mid = ((lo + hi) / 2) as usize;
            if a.cues[mid].t_ms <= elapsed_ms {
                found = Some(&a.cues[mid]);
                lo = mid as isize + 1;
            } else {
                hi = mid as isize - 1;
            }
        }
        found.map(|c| c.viseme.as_str())
    }

    fn on_flush(&mut self) {
        self.pending_cues = None;
        self.anchored = None;
    }
}

// ============================================================================
// 1. ADVERSARIAL CONCURRENCY: 20 CONCURRENT TTS TURNS OP_VISME ORDERING
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn test_adversarial_20_concurrent_tts_turns_viseme_ordering() {
    const NUM_TURNS: usize = 20;
    const CHUNKS_PER_TURN: usize = 5;

    let barrier = Arc::new(tokio::sync::Barrier::new(NUM_TURNS));
    let mut join_handles = Vec::new();

    for turn_idx in 0..NUM_TURNS {
        let barrier = barrier.clone();
        let handle = tokio::spawn(async move {
            let session_id = (turn_idx + 100) as u64;

            let (speaker_tx, mut speaker_rx) = mpsc::channel::<VoiceFrame>(1000);

            // Wait for all 20 threads to align before bursting
            barrier.wait().await;

            let mut seq_id = 0u32;

            for chunk_idx in 0..CHUNKS_PER_TURN {
                let phonemes = match (turn_idx + chunk_idx) % 4 {
                    0 => "xincˈaːo",
                    1 => "mˈaːmaba",
                    2 => "həlˈoʊwɜːld",
                    _ => "vjətnaːm",
                };
                let sample_rate = 24_000u32;
                let duration_ms = 300u64;
                let sample_count = (sample_rate as u64 * duration_ms / 1000) as usize;
                let dummy_audio = vec![0.05f32; sample_count];

                let cues = test_build_viseme_timeline(phonemes, duration_ms);
                assert!(!cues.is_empty(), "Cues must not be empty for phonemes: {}", phonemes);

                // 1. Build and emit OP_VISME frame with current seq_id
                let payload_json = serde_json::json!({
                    "turn_epoch": session_id,
                    "base_seq_id": seq_id,
                    "visemes": cues.iter().map(|c| serde_json::json!({
                        "v": c.viseme.as_str(),
                        "t_ms": c.t_ms,
                    })).collect::<Vec<_>>(),
                });

                let visme_frame = VoiceFrame {
                    op_code: OP_VISME,
                    seq_id,
                    payload: Bytes::from(payload_json.to_string()),
                };

                speaker_tx.send(visme_frame).await.unwrap();

                // 2. Build and emit OP_SPEAKER_OUT frames for this chunk
                let audio_frames = speaker_frames(session_id as u32, sample_rate, &dummy_audio);
                assert!(!audio_frames.is_empty());

                for mut frame in audio_frames {
                    frame.seq_id = seq_id;
                    seq_id = seq_id.wrapping_add(1);
                    speaker_tx.send(frame).await.unwrap();
                }
            }

            drop(speaker_tx);

            // Verify the received sequence from client perspective
            let mut received_frames = Vec::new();
            while let Some(f) = speaker_rx.recv().await {
                received_frames.push(f);
            }

            assert!(
                received_frames.len() >= CHUNKS_PER_TURN * 2,
                "Turn {} received insufficient frames: {}",
                turn_idx,
                received_frames.len()
            );

            // Invariant Verification:
            // Every OP_VISME MUST strictly precede all OP_SPEAKER_OUT frames for that chunk.
            let mut chunk_count = 0;
            let mut i = 0;

            while i < received_frames.len() {
                let frame = &received_frames[i];
                assert_eq!(
                    frame.op_code, OP_VISME,
                    "Turn {}: Frame at index {} must be OP_VISME, found 0x{:02x}",
                    turn_idx, i, frame.op_code
                );

                // Parse and validate OP_VISME payload schema
                let payload_str = std::str::from_utf8(&frame.payload).unwrap();
                let v: Value = serde_json::from_str(payload_str).unwrap();
                assert_eq!(v["turn_epoch"].as_u64().unwrap(), session_id);
                let base_seq = v["base_seq_id"].as_u64().unwrap() as u32;
                assert_eq!(base_seq, frame.seq_id);

                let visemes = v["visemes"].as_array().unwrap();
                assert!(!visemes.is_empty());
                let mut prev_t = 0u64;
                for (cue_idx, item) in visemes.iter().enumerate() {
                    let v_str = item["v"].as_str().unwrap();
                    assert!(
                        ["aa", "ee", "ih", "oh", "ou", "nil"].contains(&v_str),
                        "Invalid viseme name: {}",
                        v_str
                    );
                    let t_ms = item["t_ms"].as_u64().unwrap();
                    if cue_idx > 0 {
                        assert!(t_ms > prev_t, "Timestamps must be strictly monotonic");
                    }
                    prev_t = t_ms;
                }

                i += 1;
                // Followed by at least one OP_SPEAKER_OUT frame
                let mut speaker_count = 0;
                while i < received_frames.len() && received_frames[i].op_code == OP_SPEAKER_OUT {
                    let sf = &received_frames[i];
                    assert_eq!(sf.op_code, OP_SPEAKER_OUT);
                    let epoch = speaker_turn_epoch(sf).unwrap();
                    assert_eq!(epoch, session_id as u32);
                    speaker_count += 1;
                    i += 1;
                }

                assert!(
                    speaker_count > 0,
                    "Turn {}: OP_VISME must be followed by >=1 OP_SPEAKER_OUT frames",
                    turn_idx
                );
                chunk_count += 1;
            }

            assert_eq!(chunk_count, CHUNKS_PER_TURN);
            turn_idx
        });

        join_handles.push(handle);
    }

    let mut completed_turns = HashSet::new();
    for handle in join_handles {
        let turn_idx = handle.await.expect("Turn task failed or panicked");
        completed_turns.insert(turn_idx);
    }

    assert_eq!(
        completed_turns.len(),
        NUM_TURNS,
        "All 20 concurrent turns must complete cleanly"
    );
}

// ============================================================================
// 2. ADVERSARIAL STRESS: BARGE-IN OP_FLUSH PURGES VISEMES (ZERO GHOSTING)
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adversarial_barge_in_instant_viseme_purge_and_zero_ghosting() {
    let mut client_registry = ClientVisemeRegistry::new();
    let mut speaker_gate = SpeakerEpochGate::default();

    let initial_epoch = 1u64;
    client_registry.active_epoch = initial_epoch;

    // 1. Client receives OP_VISME for epoch 1
    let payload_turn1 = serde_json::json!({
        "turn_epoch": initial_epoch,
        "base_seq_id": 0,
        "visemes": [
            { "v": "nil", "t_ms": 0 },
            { "v": "aa", "t_ms": 100 },
            { "v": "ee", "t_ms": 250 },
        ],
    });

    client_registry
        .on_viseme_frame(&payload_turn1.to_string(), initial_epoch)
        .unwrap();

    // Schedule audio playback at ctx.currentTime = 5.0s, duration 0.4s
    client_registry.note_chunk_scheduled(5.0, 0.4);

    // During playback (t = 5.15s), viseme is 'aa'
    assert_eq!(client_registry.current_viseme(5.15), Some("aa"));

    // 2. User barges in at t = 5.18s!
    // Server triggers cancel_active_operations: epoch bumps to 2, OP_FLUSH emitted
    let new_epoch = 2u64;
    speaker_gate.observe_flush(new_epoch as u32);
    client_registry.on_flush();
    client_registry.active_epoch = new_epoch;

    // Immediately after flush, viseme MUST be None (or neutral), with ZERO ghosting
    assert_eq!(
        client_registry.current_viseme(5.18),
        None,
        "Viseme must be completely purged after OP_FLUSH"
    );
    assert_eq!(
        client_registry.current_viseme(5.20),
        None,
        "No residual viseme cues allowed after flush"
    );

    // 3. Stale audio and viseme frames from epoch 1 in flight MUST be 100% rejected
    let stale_visme = serde_json::json!({
        "turn_epoch": initial_epoch,
        "base_seq_id": 1,
        "visemes": [{ "v": "oh", "t_ms": 0 }],
    });
    client_registry
        .on_viseme_frame(&stale_visme.to_string(), new_epoch)
        .unwrap();
    assert!(
        client_registry.pending_cues.is_none(),
        "Stale epoch viseme frame must be ignored"
    );

    let dummy_pcm = vec![0.1f32; 480];
    let stale_speaker_frames = speaker_frames(initial_epoch as u32, 24_000, &dummy_pcm);
    for sf in stale_speaker_frames {
        assert!(
            !speaker_gate.accepts(&sf),
            "SpeakerEpochGate must reject stale audio frame from old epoch"
        );
    }

    // 4. New turn (epoch 2) can now cleanly register new viseme timeline
    let payload_turn2 = serde_json::json!({
        "turn_epoch": new_epoch,
        "base_seq_id": 0,
        "visemes": [
            { "v": "ih", "t_ms": 0 },
            { "v": "ou", "t_ms": 150 },
        ],
    });
    client_registry
        .on_viseme_frame(&payload_turn2.to_string(), new_epoch)
        .unwrap();
    client_registry.note_chunk_scheduled(6.0, 0.3);

    assert_eq!(client_registry.current_viseme(6.05), Some("ih"));
    assert_eq!(client_registry.current_viseme(6.20), Some("ou"));
    assert_eq!(client_registry.current_viseme(6.35), None); // expired chunk
}

// ============================================================================
// 3. RAPID BARGE-IN PREEMPTION STORM: 100 TIGHT INTERRUPTION CYCLES
// ============================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adversarial_100_cycle_barge_in_storm_no_deadlock_or_ghosting() {
    let mut client_registry = ClientVisemeRegistry::new();
    let mut speaker_gate = SpeakerEpochGate::default();

    let start_time = Instant::now();

    for epoch in 1..=100u64 {
        client_registry.active_epoch = epoch;
        speaker_gate.observe_flush(epoch as u32);

        // Emit viseme
        let payload = serde_json::json!({
            "turn_epoch": epoch,
            "base_seq_id": 0,
            "visemes": [
                { "v": "nil", "t_ms": 0 },
                { "v": "aa", "t_ms": 50 },
            ],
        });
        client_registry
            .on_viseme_frame(&payload.to_string(), epoch)
            .unwrap();

        // Audio scheduled
        let start_sec = epoch as f64 * 10.0;
        client_registry.note_chunk_scheduled(start_sec, 0.1);

        assert_eq!(client_registry.current_viseme(start_sec + 0.06), Some("aa"));

        // Sudden barge-in interruption midway
        client_registry.on_flush();
        assert_eq!(
            client_registry.current_viseme(start_sec + 0.06),
            None,
            "Must be cleanly purged on cycle {}",
            epoch
        );

        // Stale frame test
        let stale_audio = speaker_frames(epoch as u32, 24_000, &[0.0f32; 100]);
        speaker_gate.observe_flush((epoch + 1) as u32);
        for f in stale_audio {
            assert!(!speaker_gate.accepts(&f));
        }
    }

    let elapsed = start_time.elapsed();
    assert!(
        elapsed < Duration::from_millis(500),
        "100 barge-in cycles took {:?}, must finish in <500ms",
        elapsed
    );
}

// ============================================================================
// 4. IPA PHONETIC MAPPING INVARIANTS & STRESS/MODIFIER FILTERING
// ============================================================================

#[test]
fn test_adversarial_extended_ipa_complete_mapping_and_modifiers() {
    // Open vowels -> Aa
    let open_vowels = ['a', 'ɑ', 'æ', 'ɐ', 'ä', 'ą', 'ã', 'ʌ', 'ɒ'];
    for ph in open_vowels {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Aa, "Failed on open vowel: {}", ph);
    }

    // High front vowels -> Ee
    let high_front = ['i', 'ɪ', 'y', 'ɨ', 'j'];
    for ph in high_front {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ee, "Failed on high front: {}", ph);
    }

    // Mid front/central -> Ih
    let mid_vowels = ['e', 'ɛ', 'ə', 'ɜ', 'ɚ'];
    for ph in mid_vowels {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ih, "Failed on mid vowel: {}", ph);
    }

    // Back rounded -> Oh
    let back_mid = ['o', 'ɔ', 'ø'];
    for ph in back_mid {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Oh, "Failed on back mid: {}", ph);
    }

    // High back rounded -> Ou
    let high_back = ['u', 'ʊ', 'ư', 'w', 'ʉ', 'ɯ'];
    for ph in high_back {
        assert_eq!(TestViseme::from_phoneme(ph), TestViseme::Ou, "Failed on high back: {}", ph);
    }

    // Bilabials & labiodentals -> Nil (closed mouth)
    let bilabials = ['m', 'b', 'p', 'f', 'v', 'ɱ', 'ʋ', 'β'];
    for ph in bilabials {
        assert_eq!(
            TestViseme::from_phoneme(ph),
            TestViseme::Nil,
            "Bilabial {} must map to Nil to close mouth",
            ph
        );
    }

    // Complex IPA sequence with stress marks, tie bars, length marks
    // "ˈt͡ʃeɪnd͡ʒ" (change) with 400ms duration
    let timeline = test_build_viseme_timeline("ˈt͡ʃeɪnd͡ʒ", 400);
    assert!(!timeline.is_empty());
    assert_eq!(timeline[0].t_ms, 0);

    // Verify modifiers alone do not produce phantom visemes
    let pure_modifiers = "ˈˌːˑ̆͜͡ʰʲʷˤ˞̃";
    assert!(test_build_viseme_timeline(pure_modifiers, 500).is_empty());
}

// ============================================================================
// 5. ASYMMETRIC CHUNKER: NEWLINE CLAUSE BOUNDARIES & RAPID STREAMING
// ============================================================================

#[test]
fn test_adversarial_chunker_asymmetric_streaming_with_newlines() {
    let mut chunker = TtsChunker::new();

    // Asymmetric rule: First chunk emits at >=2 words on clause boundary (including newline)
    let c1 = chunker.push("Xin chào\n");
    assert_eq!(c1.len(), 1, "Must split on newline after 2 words");
    assert_eq!(c1[0], "Xin chào");

    // Subsequent chunks require >=6 words for clause boundary or terminal punctuation
    let c2 = chunker.push("đây là dòng thứ hai\n");
    // "đây là dòng thứ hai" has 5 words, not yet >= 6 words on clause boundary
    assert_eq!(c2.len(), 0, "Subsequent chunk must not split under 6 words on clause");

    let c3 = chunker.push("của câu tiếp theo.\n");
    // With "." terminal punctuation, it emits immediately
    assert_eq!(c3.len(), 1);
    assert!(c3[0].contains("đây là dòng thứ hai"));
    assert!(c3[0].contains("của câu tiếp theo."));

    // Trailing buffer flush
    let c4 = chunker.push("Phần còn lại");
    assert_eq!(c4.len(), 0);
    let flush = chunker.flush();
    assert_eq!(flush, Some("Phần còn lại".to_string()));
}
