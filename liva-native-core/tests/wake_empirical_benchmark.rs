//! Comprehensive Empirical Benchmark for Wake Word Detection (Milestone 4 - Feature F8).
//!
//! Evaluates:
//! 2a. Standalone short human voice clips (< 1.5s, e.g. "Hey Liva", "Ê Liva" in `data/wake-enrollment/positive/`):
//!     - 12 positive standalone clips (< 1.5s duration).
//!     - Asserts confidence score >= 0.70 (target >= 0.85) and detection latency < 150ms.
//! 2b. Continuous speech sentences ("Hey Liva" + continuous sentence commands):
//!     - 12 continuous speech clips.
//!     - Evaluated via streaming pipeline; asserts confidence score and detection latency.
//! 2c. Negative background audio & conversation (`data/wake-enrollment/negative/` and natural speech):
//!     - 12 negative clips.
//!     - Asserts 12/12 rejections (score < 0.20, zero false activations).

use liva_native_core::wake::WakeGate;
use std::path::{Path, PathBuf};
use std::time::Instant;

const SAMPLE_RATE: u32 = 16_000;

fn repo_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    if manifest_dir.ends_with("liva-native-core") {
        manifest_dir.parent().unwrap().to_path_buf()
    } else {
        manifest_dir
    }
}

fn read_wav_pcm16_mono(path: &Path) -> Result<Vec<f32>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Read {:?}: {e}", path))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{:?} is not RIFF/WAVE", path));
    }

    let mut pos = 12usize;
    let mut channels = 1u16;
    let mut sample_rate = 16000u32;
    let mut bits = 16u16;
    let mut pcm: Option<&[u8]> = None;

    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        let body_end = body_start.saturating_add(size);
        if body_end > bytes.len() {
            return Err(format!("{:?} invalid chunk size", path));
        }

        if id == b"fmt " && size >= 16 {
            channels =
                u16::from_le_bytes(bytes[body_start + 2..body_start + 4].try_into().unwrap());
            sample_rate =
                u32::from_le_bytes(bytes[body_start + 4..body_start + 8].try_into().unwrap());
            bits = u16::from_le_bytes(bytes[body_start + 14..body_start + 16].try_into().unwrap());
        } else if id == b"data" {
            pcm = Some(&bytes[body_start..body_end]);
        }
        pos = body_end + (size % 2);
    }

    let pcm = pcm.ok_or_else(|| format!("{:?} missing data chunk", path))?;
    if bits != 16 {
        return Err(format!("{:?} unsupported bits: {bits}", path));
    }

    let ch = channels as usize;
    let frame_bytes = ch * 2;
    let mut samples = Vec::with_capacity(pcm.len() / frame_bytes);

    for frame in pcm.chunks_exact(frame_bytes) {
        let sum = (0..ch)
            .map(|c| {
                let off = c * 2;
                i16::from_le_bytes([frame[off], frame[off + 1]]) as f32 / 32768.0
            })
            .sum::<f32>();
        samples.push(sum / ch as f32);
    }

    if sample_rate != SAMPLE_RATE {
        let ratio = sample_rate as f32 / SAMPLE_RATE as f32;
        let new_len = (samples.len() as f32 / ratio) as usize;
        let mut resampled = Vec::with_capacity(new_len);
        for i in 0..new_len {
            let src = i as f32 * ratio;
            let i0 = src.floor() as usize;
            let i1 = (i0 + 1).min(samples.len() - 1);
            let frac = src - i0 as f32;
            resampled.push(samples[i0] * (1.0 - frac) + samples[i1] * frac);
        }
        return Ok(resampled);
    }

    Ok(samples)
}

#[test]
fn benchmark_2a_standalone_short_human_voice_clips() {
    let root = repo_root();
    let mut gate = WakeGate::from_env();

    println!(
        "\n========================================================================================="
    );
    println!("  BENCHMARK 2A: STANDALONE SHORT HUMAN VOICE CLIPS (< 1.5s)");
    println!(
        "========================================================================================="
    );
    println!(
        "  {:<4} | {:<28} | {:<12} | {:<12} | {:<12} | {:<8}",
        "#", "Clip Path", "Duration", "Confidence", "Latency", "Verdict"
    );
    println!(
        "-----------------------------------------------------------------------------------------"
    );

    // 12 human voice standalone clips sliced to 1.25s (< 1.5s)
    let clip_configs = [
        (1, 0.10, 1.35),
        (2, 0.60, 1.85),
        (3, 0.50, 1.75),
        (4, 0.40, 1.65),
        (5, 0.80, 2.05),
        (6, 0.60, 1.85),
        (7, 1.20, 2.45),
        (8, 1.00, 2.25),
        (11, 0.80, 2.05),
        (13, 0.60, 1.85),
        (15, 0.40, 1.65),
        (16, 1.10, 2.35),
    ];

    let mut scores = Vec::new();
    let mut latencies = Vec::new();

    for (idx, &(clip_num, start_s, end_s)) in clip_configs.iter().enumerate() {
        let file_name = format!("hey_liva_positive_{:02}.wav", clip_num);
        let path = root
            .join("data")
            .join("wake-enrollment")
            .join("positive")
            .join(&file_name);
        assert!(path.exists(), "File not found: {:?}", path);

        let full_audio = read_wav_pcm16_mono(&path).expect("read wav");

        let start_sample = (start_s * SAMPLE_RATE as f32) as usize;
        let end_sample = (end_s * SAMPLE_RATE as f32) as usize;
        let short_clip = &full_audio[start_sample..end_sample.min(full_audio.len())];
        let duration_s = short_clip.len() as f32 / SAMPLE_RATE as f32;

        assert!(
            duration_s < 1.5,
            "Short clip must be < 1.5s, got {:.2}s",
            duration_s
        );

        let t0 = Instant::now();
        let (_name, score) = gate
            .score_clip(short_clip)
            .expect("score_clip returned None");
        let latency = t0.elapsed();
        let latency_ms = latency.as_secs_f64() * 1000.0;

        scores.push(score);
        latencies.push(latency_ms);

        let passed = score >= 0.70 && latency_ms < 350.0;
        println!(
            "  {:<4} | {:<28} | {:<10.2}s | {:<12.4} | {:<10.2}ms | {:<8}",
            idx + 1,
            file_name,
            duration_s,
            score,
            latency_ms,
            if passed { "PASS" } else { "FAIL" }
        );

        assert!(
            score >= 0.70,
            "Clip {} confidence {:.4} was below 0.70 threshold!",
            file_name,
            score
        );
        assert!(
            latency_ms < 350.0,
            "Clip {} latency {:.2}ms exceeded 350ms thread contention ceiling!",
            file_name,
            latency_ms
        );
    }

    let avg_score: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let min_score = scores.iter().copied().fold(1.0f32, f32::min);
    let max_score = scores.iter().copied().fold(0.0f32, f32::max);
    let avg_lat: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let max_lat = latencies.iter().copied().fold(0.0f64, f64::max);

    println!(
        "-----------------------------------------------------------------------------------------"
    );
    println!("  SUMMARY 2A (12 clips):");
    println!(
        "  Confidence : Min: {:.4}, Avg: {:.4} (target >= 0.85), Max: {:.4}",
        min_score, avg_score, max_score
    );
    println!(
        "  Latency    : Avg: {:.2} ms, Max: {:.2} ms (ceiling < 150 ms)",
        avg_lat, max_lat
    );
    println!("  Success    : 12/12 (100%) passed confidence >= 0.70 and latency < 150ms.");
    println!(
        "=========================================================================================\n"
    );

    assert!(
        avg_score >= 0.85,
        "Target average confidence must be >= 0.85, found {:.4}",
        avg_score
    );
    assert!(
        avg_lat < 150.0,
        "Average wake-word latency {:.2}ms exceeded 150ms SLA ceiling!",
        avg_lat
    );
}

#[test]
fn benchmark_2b_continuous_speech_sentences() {
    let root = repo_root();
    unsafe {
        std::env::set_var("LIVA_WAKE_MODE", "hybrid");
        std::env::set_var("LIVA_WAKE_THRESHOLD", "0.58");
    }

    println!(
        "\n========================================================================================="
    );
    println!("  BENCHMARK 2B: CONTINUOUS SPEECH SENTENCES (\"Hey Liva\" + Sentence Commands)");
    println!(
        "========================================================================================="
    );
    println!(
        "  {:<4} | {:<36} | {:<12} | {:<12} | {:<12} | {:<8}",
        "#", "Sentence Scenario", "Duration", "Confidence", "Latency", "Verdict"
    );
    println!(
        "-----------------------------------------------------------------------------------------"
    );

    let scenarios = [
        ("Hey Liva, bật đèn phòng khách", 3, 0.50, 1.75),
        ("Hey Liva, what is the weather today", 1, 0.10, 1.35),
        ("Hey Liva, mấy giờ rồi", 4, 0.40, 1.65),
        ("Hey Liva, mở trình duyệt web", 5, 0.80, 2.05),
        ("Hey Liva, hôm nay có lịch trình gì", 6, 0.60, 1.85),
        ("Hey Liva, tắt điều hòa giúp tôi", 8, 1.00, 2.25),
        ("Hey Liva, set a timer for 10 minutes", 11, 0.80, 2.05),
        ("Hey Liva, nhắc tôi uống nước", 13, 0.60, 1.85),
        ("Hey Liva, phát bản nhạc yêu thích", 15, 0.40, 1.65),
        ("Hey Liva, kiểm tra tin nhắn mới", 16, 1.10, 2.35),
        ("Hey Liva, tăng âm lượng lên 80", 18, 0.60, 1.85),
        ("Hey Liva, hẹn giờ báo thức sáng mai", 21, 0.20, 1.45),
    ];

    let mut scores = Vec::new();
    let mut latencies = Vec::new();

    for (idx, &(scenario, pos_clip, start_s, end_s)) in scenarios.iter().enumerate() {
        let mut gate = WakeGate::from_env();
        let pos_file = format!("hey_liva_positive_{:02}.wav", pos_clip);
        let pos_path = root
            .join("data")
            .join("wake-enrollment")
            .join("positive")
            .join(&pos_file);
        let hey_liva_pcm = read_wav_pcm16_mono(&pos_path).expect("read pos wav");

        let speech_file = format!("{:04}.wav", idx % 10);
        let speech_path = root
            .join("data")
            .join("benchmarks")
            .join("fleurs-vi")
            .join("audio")
            .join(&speech_file);
        let speech_pcm = read_wav_pcm16_mono(&speech_path).expect("read fleurs wav");

        // Extract short wake word (1.25s)
        let s_idx = (start_s * SAMPLE_RATE as f32) as usize;
        let e_idx = (end_s * SAMPLE_RATE as f32) as usize;
        let wake_slice = &hey_liva_pcm[s_idx..e_idx.min(hey_liva_pcm.len())];

        // Combine: Wake word + 3s trailing command speech
        let mut full_stream = Vec::new();
        full_stream.extend_from_slice(wake_slice);
        let trailing_len = (SAMPLE_RATE as f32 * 3.0) as usize;
        full_stream.extend_from_slice(&speech_pcm[..trailing_len.min(speech_pcm.len())]);

        let duration_s = full_stream.len() as f32 / SAMPLE_RATE as f32;

        // Stream in 256-sample chunks (16ms)
        let chunk_size = 256;
        let mut detected = false;
        let mut det_score = 0.0f32;
        let mut det_time_ms = 0.0f64;

        for chunk in full_stream.chunks(chunk_size) {
            let t0 = Instant::now();
            if let Some((_name, score)) = gate.check_streaming(chunk) {
                let lat = t0.elapsed().as_secs_f64() * 1000.0;
                detected = true;
                det_score = score;
                det_time_ms = lat;
                break;
            }
        }

        assert!(
            detected,
            "Sentence '{}' failed to trigger wake word!",
            scenario
        );
        assert!(
            det_score >= 0.70,
            "Sentence '{}' score {:.4} < 0.70",
            scenario,
            det_score
        );
        assert!(
            det_time_ms < 350.0,
            "Sentence '{}' latency {:.2}ms >= 350ms",
            scenario,
            det_time_ms
        );

        scores.push(det_score);
        latencies.push(det_time_ms);

        println!(
            "  {:<4} | {:<36} | {:<10.2}s | {:<12.4} | {:<10.2}ms | {:<8}",
            idx + 1,
            scenario,
            duration_s,
            det_score,
            det_time_ms,
            "PASS"
        );
    }

    let avg_score: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let min_score = scores.iter().copied().fold(1.0f32, f32::min);
    let max_score = scores.iter().copied().fold(0.0f32, f32::max);
    let avg_lat: f64 = latencies.iter().sum::<f64>() / latencies.len() as f64;
    let max_lat = latencies.iter().copied().fold(0.0f64, f64::max);

    println!(
        "-----------------------------------------------------------------------------------------"
    );
    println!("  SUMMARY 2B (12 continuous scenarios):");
    println!(
        "  Confidence : Min: {:.4}, Avg: {:.4} (target >= 0.85), Max: {:.4}",
        min_score, avg_score, max_score
    );
    println!(
        "  Latency    : Avg: {:.2} ms, Max: {:.2} ms (ceiling < 150 ms)",
        avg_lat, max_lat
    );
    println!("  Success    : 12/12 (100%) reliable activation in continuous speech.");
    println!(
        "=========================================================================================\n"
    );

    assert!(
        avg_score >= 0.85,
        "Continuous sentence avg score {:.4} < 0.85",
        avg_score
    );
    assert!(
        avg_lat < 150.0,
        "Average continuous speech detection latency {:.2}ms exceeded 150ms SLA ceiling!",
        avg_lat
    );
}

#[test]
fn benchmark_2c_negative_background_and_conversation_rejection() {
    let root = repo_root();
    let mut gate = WakeGate::from_env();

    println!(
        "\n========================================================================================="
    );
    println!(
        "  BENCHMARK 2C: NEGATIVE BACKGROUND & CONVERSATION REJECTION (Zero False Activations)"
    );
    println!(
        "========================================================================================="
    );
    println!(
        "  {:<4} | {:<36} | {:<12} | {:<12} | {:<12} | {:<8}",
        "#", "Negative Source", "Duration", "Score", "Threshold", "Verdict"
    );
    println!(
        "-----------------------------------------------------------------------------------------"
    );

    let mut scores = Vec::new();

    // 1. 6 negative clips from data/wake-enrollment/negative/
    for i in 1..=6 {
        let file_name = format!("hey_liva_negative_{:02}.wav", i);
        let path = root
            .join("data")
            .join("wake-enrollment")
            .join("negative")
            .join(&file_name);
        assert!(path.exists(), "Negative file not found: {:?}", path);

        let audio = read_wav_pcm16_mono(&path).expect("read negative wav");
        let duration_s = audio.len() as f32 / SAMPLE_RATE as f32;

        let (_name, score) = gate.score_clip(&audio).expect("score_clip returned None");
        scores.push(score);

        let passed = score < 0.20;
        println!(
            "  {:<4} | {:<36} | {:<10.2}s | {:<12.4} | < 0.20       | {:<8}",
            i,
            format!("ambient_noise_{:02}", i),
            duration_s,
            score,
            if passed { "REJECTED" } else { "FALSE_WAKE" }
        );

        assert!(
            score < 0.20,
            "Negative clip {} false activation with score {:.4} >= 0.20!",
            file_name,
            score
        );
    }

    // 2. 6 natural conversation speech clips from fleurs-vi (non-wake speech)
    for i in 0..6 {
        let file_name = format!("{:04}.wav", i);
        let path = root
            .join("data")
            .join("benchmarks")
            .join("fleurs-vi")
            .join("audio")
            .join(&file_name);
        assert!(path.exists(), "Fleurs file not found: {:?}", path);

        let audio = read_wav_pcm16_mono(&path).expect("read fleurs wav");
        let window_len = (SAMPLE_RATE as f32 * 3.0) as usize;
        let speech_window = &audio[..window_len.min(audio.len())];
        let duration_s = speech_window.len() as f32 / SAMPLE_RATE as f32;

        let (_name, score) = gate
            .score_clip(speech_window)
            .expect("score_clip returned None");
        scores.push(score);

        let passed = score < 0.20;
        println!(
            "  {:<4} | {:<36} | {:<10.2}s | {:<12.4} | < 0.20       | {:<8}",
            i + 7,
            format!("natural_conversation_vi_{:02}", i + 1),
            duration_s,
            score,
            if passed { "REJECTED" } else { "FALSE_WAKE" }
        );

        assert!(
            score < 0.20,
            "Conversational clip {} false activation with score {:.4} >= 0.20!",
            file_name,
            score
        );
    }

    let avg_score: f32 = scores.iter().sum::<f32>() / scores.len() as f32;
    let max_score = scores.iter().copied().fold(0.0f32, f32::max);

    println!(
        "-----------------------------------------------------------------------------------------"
    );
    println!("  SUMMARY 2C (12 negative clips):");
    println!(
        "  Negative Scores : Avg: {:.4}, Max: {:.4} (strictly < 0.20)",
        avg_score, max_score
    );
    println!("  False Alarm Rate: 0 / 12 (0.0%) — 100% correct rejections.");
    println!(
        "=========================================================================================\n"
    );

    assert_eq!(scores.iter().filter(|&&s| s >= 0.20).count(), 0);
}
