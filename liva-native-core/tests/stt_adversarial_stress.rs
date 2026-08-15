use liva_native_core::stt::anti_hallucination::{
    AntiHallucinationConfig, AntiHallucinationFilter, FilterDecision, FilterReason,
};
use liva_native_core::stt::parakeet::ParakeetVi;
use liva_native_core::stt::{ParakeetRecognizer, SttManager, StreamingTranscript};
use rodio::{Decoder, Source};
use std::io::BufReader;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

fn resolve_model_paths() -> (PathBuf, PathBuf) {
    let mut model_path = PathBuf::from("models/parakeet_vi.onnx");
    let mut vocab_path = PathBuf::from("models/parakeet_vi_vocab.json");
    if !model_path.exists() {
        model_path = PathBuf::from("../models/parakeet_vi.onnx");
        vocab_path = PathBuf::from("../models/parakeet_vi_vocab.json");
    }
    (model_path, vocab_path)
}

fn load_audio_wav_16k(path: &Path) -> Result<Vec<f32>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {:?}: {}", path, e))?;
    let dec = Decoder::new(BufReader::new(file)).map_err(|e| format!("decode {:?}: {}", path, e))?;
    let sr = dec.sample_rate();
    let ch = dec.channels() as usize;
    let samples: Vec<f32> = dec.convert_samples::<f32>().collect();

    let mono: Vec<f32> = if ch > 1 {
        samples
            .chunks(ch)
            .map(|c| c.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        samples
    };

    if sr == 16000 {
        return Ok(mono);
    }

    let ratio = 16000.0f64 / sr as f64;
    let out_len = (mono.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let i0 = src.floor() as usize;
        let frac = (src - i0 as f64) as f32;
        let s0 = mono.get(i0).copied().unwrap_or(0.0);
        let s1 = mono.get(i0 + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
    }
    Ok(out)
}

fn find_audio_file(rel: &str) -> PathBuf {
    let p = PathBuf::from(rel);
    if p.exists() {
        p
    } else {
        PathBuf::from("..").join(rel)
    }
}

// ---------------------------------------------------------------------------
// 1. EMPIRICAL FIRST-CHUNK LATENCY TEST (<150ms BUDGET)
// ---------------------------------------------------------------------------
#[test]
fn test_parakeet_first_chunk_latency_budget() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        eprintln!("Skipping test: Parakeet model files not found");
        return;
    }

    let mut pk = ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi");
    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0000.wav");
    let audio = load_audio_wav_16k(&audio_path).expect("Failed to load audio");

    const CHUNK_SIZE: usize = 2560; // 160ms @ 16kHz
    let first_chunk = &audio[0..CHUNK_SIZE.min(audio.len())];

    // Warm-up run
    let _ = pk.feed_chunk(first_chunk, false);
    pk.reset_stream();

    // Benchmark first chunk latency across 10 iterations
    let mut latencies: Vec<Duration> = Vec::new();
    for _ in 0..10 {
        pk.reset_stream();
        let t0 = Instant::now();
        let _ = pk.feed_chunk(first_chunk, false).expect("feed_chunk failed");
        latencies.push(t0.elapsed());
    }

    latencies.sort();
    let min_lat = latencies[0];
    let p50_lat = latencies[latencies.len() / 2];
    let max_lat = latencies[latencies.len() - 1];

    println!(
        "[Challenger 1] First-Chunk Latency (160ms frame): Min={:?}, P50={:?}, Max={:?}",
        min_lat, p50_lat, max_lat
    );

    // The 150ms CPU budget declared in this section's header is the contract, and it is what
    // both assertions below enforce. It replaced a 600ms/550ms pair that sat 8x above the
    // measured value and so could not fail until performance collapsed.
    //
    // Measured over 6 release runs on an idle machine: P50 74.2-90.2ms, max 75.8-97.9ms. That
    // leaves roughly 1.6x headroom - deliberately, because this is an SLA test, not a
    // regression detector. If a run exceeds 150ms the declared budget genuinely was not met.
    // On a slower or heavily loaded CI runner, raise the budget here AND in the header comment
    // together, so the number the test enforces never drifts from the number it claims.
    // Debug builds are unoptimized; the strict number is the release contract.
    const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
    assert!(
        p50_lat < Duration::from_millis(150) * SLOWDOWN,
        "First-chunk latency p50 ({:?}) must be < 150ms on CPU",
        p50_lat
    );
    // Asserted on the MAX, not the min. `min_lat <= p50_lat` always holds, so a min assertion
    // sharing the p50 threshold can never fail on its own - it was dead weight. The tail is the
    // half of the distribution worth guarding.
    assert!(
        max_lat < Duration::from_millis(150) * SLOWDOWN,
        "First-chunk latency max ({:?}) must be < 150ms on CPU",
        max_lat
    );
}

// ---------------------------------------------------------------------------
// 2. PARTIAL TOKEN STREAMING CADENCE & MONOTONICITY
// ---------------------------------------------------------------------------
#[test]
fn test_parakeet_partial_token_streaming_cadence() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        eprintln!("Skipping test: Parakeet model files not found");
        return;
    }

    let mut pk = ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi");
    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0000.wav");
    let audio = load_audio_wav_16k(&audio_path).expect("Failed to load audio");

    const CHUNK_SIZE: usize = 2560; // 160ms @ 16kHz
    let total_chunks = audio.len().div_ceil(CHUNK_SIZE);

    let mut partial_emissions: Vec<(usize, u64, String, f32)> = Vec::new();
    let stream_start = Instant::now();

    for (chunk_idx, chunk_slice) in audio.chunks(CHUNK_SIZE).enumerate() {
        let is_last = chunk_idx + 1 == total_chunks;
        let res = pk
            .feed_chunk(chunk_slice, is_last)
            .expect("feed_chunk failed");

        if let Some(transcript) = res {
            let elapsed_ms = stream_start.elapsed().as_millis() as u64;
            println!(
                "  Chunk {:02}/{} (Audio {:4}ms, Wall {:4}ms) [is_final={:5}, conf={:.2}]: \"{}\"",
                chunk_idx + 1,
                total_chunks,
                (chunk_idx + 1) * 160,
                elapsed_ms,
                transcript.is_final,
                transcript.confidence,
                transcript.partial_text
            );

            partial_emissions.push((
                chunk_idx + 1,
                transcript.latency_ms,
                transcript.partial_text,
                transcript.confidence,
            ));
        }
    }

    println!(
        "[Challenger 1] Total chunks: {}, Partial updates emitted: {}",
        total_chunks,
        partial_emissions.len()
    );

    // Verify that streaming emits intermediate partial updates during speech turns
    assert!(
        partial_emissions.len() >= 5,
        "Streaming STT must emit multiple intermediate partial transcripts during 11s speech (got {})",
        partial_emissions.len()
    );

    // Verify final transcript is non-empty and contains key content
    let (_, _, final_text, _) = partial_emissions.last().unwrap();
    assert!(!final_text.is_empty(), "Final transcript must not be empty");
    assert!(
        final_text.to_lowercase().contains("văn hóa"),
        "Final transcript should contain expected Vietnamese words, got: {}",
        final_text
    );
}

// ---------------------------------------------------------------------------
// 3. CUMULATIVE INFERENCE LATENCY SCALING UNDER CONTINUOUS SPEECH
// ---------------------------------------------------------------------------
#[test]
fn test_cumulative_latency_scaling() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        return;
    }

    let mut pk = ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi");
    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0000.wav");
    let audio = load_audio_wav_16k(&audio_path).expect("Failed to load audio");

    const CHUNK_SIZE: usize = 2560; // 160ms @ 16kHz
    let mut chunk_latencies: Vec<(usize, Duration)> = Vec::new();

    for (chunk_idx, chunk_slice) in audio.chunks(CHUNK_SIZE).enumerate() {
        let is_last = (chunk_idx + 1) * CHUNK_SIZE >= audio.len();
        let t0 = Instant::now();
        let _ = pk.feed_chunk(chunk_slice, is_last);
        let elapsed = t0.elapsed();
        chunk_latencies.push((chunk_idx + 1, elapsed));
    }

    println!("[Challenger 1] Chunk Latency Profile across 11.3s utterance:");
    for &(c_idx, lat) in &[
        chunk_latencies[0],
        chunk_latencies[9],
        chunk_latencies[24],
        chunk_latencies[49],
        *chunk_latencies.last().unwrap(),
    ] {
        println!(
            "  Chunk {:02} ({:.2}s audio): Compute = {:?}",
            c_idx,
            c_idx as f32 * 0.16,
            lat
        );
    }

    // Debug builds are unoptimized; the strict number is the release contract.
    const SLOWDOWN: u32 = if cfg!(debug_assertions) { 10 } else { 1 };
    // Cumulative latency assertion: compute for any single 160ms chunk should remain bounded under 1500ms on CPU
    for (c_idx, lat) in &chunk_latencies {
        assert!(
            *lat < Duration::from_millis(1500) * SLOWDOWN,
            "Chunk {} compute latency ({:?}) exceeded bound (1500ms)",
            c_idx,
            lat
        );
    }
}

// ---------------------------------------------------------------------------
// 4. WORD ERROR RATE & TONAL ACCURACY SPOT CHECKS
// ---------------------------------------------------------------------------
#[test]
fn test_parakeet_vietnamese_wer_and_tonal_accuracy() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        return;
    }

    let mut pk = ParakeetVi::load(&model_path, &vocab_path).expect("Failed to load ParakeetVi");

    let test_cases = [
        (
            "data/benchmarks/fleurs-vi/audio/0000.wav",
            "văn hóa và bộ lạc cổ xưa đã bắt đầu giữ những con vật này để dễ lấy sữa tóc thịt và da",
        ),
        (
            "data/benchmarks/fleurs-vi/audio/0001.wav",
            "đối với springbok trận này đã giúp đội tuyển kết thúc chuỗi thua năm trận liền",
        ),
        (
            "data/benchmarks/fleurs-vi/audio/0002.wav",
            "nó cũng tấn công mọi thứ trong nước ngay cả khủng long khổng lồ như t rex cũng không phải là đối thủ với nó",
        ),
    ];

    let mut total_words = 0;
    let mut total_errors = 0;

    for (audio_rel, ref_text) in test_cases {
        let audio_path = find_audio_file(audio_rel);
        let audio = load_audio_wav_16k(&audio_path).expect("Failed to load audio");
        let transcript = pk.transcribe(&audio).expect("Transcribe failed");

        let norm_hyp: String = transcript
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric() || c.is_whitespace())
            .collect();
        let hyp_words: Vec<&str> = norm_hyp.split_whitespace().collect();
        let ref_words: Vec<&str> = ref_text.split_whitespace().collect();

        // Simple Levenshtein word distance
        let n = ref_words.len();
        let m = hyp_words.len();
        let mut dp = vec![vec![0usize; m + 1]; n + 1];
        for i in 0..=n {
            dp[i][0] = i;
        }
        for j in 0..=m {
            dp[0][j] = j;
        }
        for i in 1..=n {
            for j in 1..=m {
                if ref_words[i - 1] == hyp_words[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1];
                } else {
                    dp[i][j] = 1 + dp[i - 1][j - 1].min(dp[i - 1][j]).min(dp[i][j - 1]);
                }
            }
        }
        let errs = dp[n][m];
        let sample_wer = errs as f32 / n as f32;

        println!(
            "[Challenger 1] Sample {:?}:\n  REF: {}\n  HYP: {}\n  WER: {:.2}% ({} errors / {} words)",
            audio_rel,
            ref_text,
            norm_hyp,
            sample_wer * 100.0,
            errs,
            n
        );

        total_words += n;
        total_errors += errs;
    }

    let overall_wer = total_errors as f32 / total_words as f32;
    println!(
        "[Challenger 1] Spot-check Aggregate WER: {:.2}% ({} errors / {} words)",
        overall_wer * 100.0,
        total_errors,
        total_words
    );

    // Spot-check WER across clean test samples should be <= 12%
    assert!(
        overall_wer <= 0.12,
        "Aggregate spot-check WER must be <= 12%, got {:.2}%",
        overall_wer * 100.0
    );
}

// ---------------------------------------------------------------------------
// 5. 5-LAYER ANTI-HALLUCINATION FILTER ADVERSARIAL STRESS TEST
// ---------------------------------------------------------------------------
#[test]
fn test_anti_hallucination_adversarial_matrix() {
    let filter = AntiHallucinationFilter::default();

    // Adversarial Case 1: Silence noise burst producing runaway tokens
    let runaway = "tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi tôi";
    let d1 = filter.filter(runaway, 3.0, Some(0.1), Some(1.2));
    assert!(!d1.is_valid());
    println!("Adversarial 1 (Runaway): {:?}", d1);

    // Adversarial Case 2: High Shannon entropy from pure background acoustic noise
    let noisy_text = "thời tiết hôm nay khá đẹp";
    let d2 = filter.filter(noisy_text, 2.0, Some(0.1), Some(2.15)); // Entropy 2.15 > 1.85
    assert!(!d2.is_valid());
    assert!(matches!(
        d2,
        FilterDecision::Filtered {
            reason: FilterReason::EntropyTooHigh { .. },
            ..
        }
    ));
    println!("Adversarial 2 (High Entropy): {:?}", d2);

    // Adversarial Case 3: YouTube / Subtitle hallucination pattern
    let yt_hallucination = "Cảm ơn các bạn đã lắng nghe và theo dõi video";
    let d3 = filter.filter(yt_hallucination, 2.5, Some(0.0), Some(1.0));
    assert!(!d3.is_valid());
    assert!(matches!(
        d3,
        FilterDecision::Filtered {
            reason: FilterReason::BlacklistPattern(_),
            ..
        }
    ));
    println!("Adversarial 3 (Blacklist Pattern): {:?}", d3);

    // Adversarial Case 4: Music tags
    let music_hallucination = "[Music] ♪ lalala ♪";
    let d4 = filter.filter(music_hallucination, 2.0, Some(0.0), Some(1.0));
    assert!(!d4.is_valid());
    println!("Adversarial 4 (Music Tag): {:?}", d4);

    // Adversarial Case 5: Extremely fast speech rate (runaway decoder loop)
    let fast_runaway = "một hai ba bốn năm sáu bảy tám chín mười một hai ba bốn năm sáu bảy tám chín mười một hai ba bốn";
    let d5 = filter.filter(fast_runaway, 1.2, Some(0.0), Some(1.0)); // > 15 WPS
    assert!(!d5.is_valid());
    println!("Adversarial 5 (Fast Runaway): {:?}", d5);

    // Positive Control: Valid Vietnamese conversational utterances must PASS
    let valid_sentences = [
        "Bật đèn phòng khách giúp tôi",
        "Hôm nay trời có mưa ở Hà Nội không",
        "Tôi muốn tạo một lịch hẹn lúc chín giờ sáng mai",
    ];

    for s in valid_sentences {
        let dec = filter.filter(s, 2.5, Some(0.02), Some(1.15));
        assert!(dec.is_valid(), "Valid sentence falsely filtered: {}", s);
    }
}

// ---------------------------------------------------------------------------
// 6. STT MANAGER PRODUCTION INTERFACE CONTRACT VERIFICATION
// ---------------------------------------------------------------------------
#[test]
fn test_stt_manager_production_interface() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        return;
    }
    unsafe {
        std::env::set_var("LIVA_PARAKEET_MODEL_PATH", &model_path);
        std::env::set_var("LIVA_PARAKEET_VOCAB_PATH", &vocab_path);
    }
    let mut stt = SttManager::new("models/nemotron-asr");
    stt.set_language("vi-VN").unwrap();

    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0001.wav");
    if !audio_path.exists() {
        return;
    }
    let audio = load_audio_wav_16k(&audio_path).expect("load audio");

    const CHUNK_SIZE: usize = 2560; // 160ms chunks
    let total_chunks = audio.len().div_ceil(CHUNK_SIZE);

    let mut last_transcript: Option<StreamingTranscript> = None;
    for (idx, slice) in audio.chunks(CHUNK_SIZE).enumerate() {
        let is_last = idx + 1 == total_chunks;
        let res = stt.feed_chunk(slice, is_last).expect("feed_chunk failed");
        if let Some(t) = res {
            if is_last {
                assert!(t.is_final, "Last chunk must have is_final = true");
            }
            last_transcript = Some(t);
        }
    }

    assert!(last_transcript.is_some());
    let final_t = last_transcript.unwrap();
    assert!(final_t.is_final);
    assert!(!final_t.partial_text.is_empty());
    println!(
        "[Challenger 1] SttManager production feed_chunk result: \"{}\" (latency: {}ms)",
        final_t.partial_text, final_t.latency_ms
    );
}

// ---------------------------------------------------------------------------
// 7. CONCURRENT MULTI-STREAM THREAD-SAFETY STRESS TEST
// ---------------------------------------------------------------------------
#[test]
fn test_concurrent_parakeet_streaming() {
    let (model_path, vocab_path) = resolve_model_paths();
    if !model_path.exists() || !vocab_path.exists() {
        return;
    }

    let audio_path = find_audio_file("data/benchmarks/fleurs-vi/audio/0002.wav");
    let audio = Arc::new(load_audio_wav_16k(&audio_path).expect("load audio"));

    let model_path = Arc::new(model_path);
    let vocab_path = Arc::new(vocab_path);

    let mut handles = Vec::new();
    const NUM_THREADS: usize = 4;

    for thread_id in 0..NUM_THREADS {
        let m_path = Arc::clone(&model_path);
        let v_path = Arc::clone(&vocab_path);
        let aud = Arc::clone(&audio);

        let h = std::thread::spawn(move || {
            let mut pk = ParakeetVi::load(&m_path, &v_path).expect("load ParakeetVi in thread");
            const CHUNK: usize = 2560;
            let total = aud.len().div_ceil(CHUNK);
            let mut final_res = String::new();

            for (idx, slice) in aud.chunks(CHUNK).enumerate() {
                let is_last = idx + 1 == total;
                if let Ok(Some(t)) = pk.feed_chunk(slice, is_last) {
                    if is_last {
                        final_res = t.partial_text;
                    }
                }
            }
            (thread_id, final_res)
        });
        handles.push(h);
    }

    for h in handles {
        let (id, text) = h.join().expect("thread panicked");
        println!("[Challenger 1] Thread {} completed: \"{}\"", id, text);
        assert!(!text.is_empty(), "Thread {} produced empty text", id);
    }
}
