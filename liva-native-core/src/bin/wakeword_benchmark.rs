//! Corpus benchmark for a wake-word classifier.
//!
//! The release gate is intentionally stricter than a handful of positive
//! clips: recall, false positives per hour, negative-corpus duration and the
//! model SHA-256 are reported together.

use liva_native_core::wake_model::TrainedWakeDetector;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};

const SAMPLE_RATE: u32 = 16_000;
const SCORE_INTERVAL_SAMPLES: usize = 3_200;
const DEFAULT_THRESHOLD: f32 = 0.68;
const DEFAULT_DEBOUNCE_SECONDS: f64 = 2.0;
const DEFAULT_MIN_RECALL: f64 = 0.90;
const DEFAULT_MAX_FPPH: f64 = 1.0;
const DEFAULT_MIN_NEGATIVE_HOURS: f64 = 1.0;

#[derive(Clone, Debug, Serialize)]
struct ClipScores {
    path: String,
    duration_seconds: f64,
    score_interval_seconds: f64,
    scores: Vec<f32>,
}

impl ClipScores {
    #[cfg(test)]
    fn new(path: &str, duration_seconds: f64, scores: Vec<f32>) -> Self {
        Self::with_score_interval(path, duration_seconds, 1.0, scores)
    }

    fn with_score_interval(
        path: &str,
        duration_seconds: f64,
        score_interval_seconds: f64,
        scores: Vec<f32>,
    ) -> Self {
        Self {
            path: path.to_string(),
            duration_seconds,
            score_interval_seconds,
            scores,
        }
    }

    fn best_score(&self) -> f32 {
        self.scores.iter().copied().fold(0.0f32, f32::max)
    }
}

#[derive(Clone, Debug, Serialize)]
struct AcceptanceGates {
    min_recall: f64,
    max_false_positives_per_hour: f64,
    min_negative_hours: f64,
    debounce_seconds: f64,
}

impl Default for AcceptanceGates {
    fn default() -> Self {
        Self {
            min_recall: DEFAULT_MIN_RECALL,
            max_false_positives_per_hour: DEFAULT_MAX_FPPH,
            min_negative_hours: DEFAULT_MIN_NEGATIVE_HOURS,
            debounce_seconds: DEFAULT_DEBOUNCE_SECONDS,
        }
    }
}

#[derive(Clone, Debug, Serialize)]
struct Metrics {
    threshold: f32,
    true_positives: usize,
    false_negatives: usize,
    false_positives: usize,
    recall: f64,
    negative_hours: f64,
    false_positives_per_hour: f64,
    accepted: bool,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    model_path: String,
    model_sha256: String,
    gates: AcceptanceGates,
    metrics: Metrics,
    positive_clips: Vec<ClipScores>,
    negative_clips: Vec<ClipScores>,
}

#[cfg(test)]
fn evaluate_threshold(
    positives: &[ClipScores],
    negatives: &[ClipScores],
    threshold: f32,
    debounce_seconds: f64,
) -> Metrics {
    let gates = AcceptanceGates {
        debounce_seconds,
        ..AcceptanceGates::default()
    };
    evaluate_with_gates(positives, negatives, threshold, &gates)
}

fn evaluate_with_gates(
    positives: &[ClipScores],
    negatives: &[ClipScores],
    threshold: f32,
    gates: &AcceptanceGates,
) -> Metrics {
    let true_positives = positives
        .iter()
        .filter(|clip| clip.best_score() >= threshold)
        .count();
    let false_negatives = positives.len().saturating_sub(true_positives);
    let recall = if positives.is_empty() {
        0.0
    } else {
        true_positives as f64 / positives.len() as f64
    };

    let negative_seconds = negatives
        .iter()
        .map(|clip| clip.duration_seconds)
        .sum::<f64>();
    let negative_hours = negative_seconds / 3_600.0;
    let false_positives = negatives
        .iter()
        .map(|clip| count_debounced_hits(clip, threshold, gates.debounce_seconds))
        .sum();
    let false_positives_per_hour = if negative_hours > 0.0 {
        false_positives as f64 / negative_hours
    } else {
        f64::INFINITY
    };
    let accepted = recall >= gates.min_recall
        && negative_hours >= gates.min_negative_hours
        && false_positives_per_hour <= gates.max_false_positives_per_hour;

    Metrics {
        threshold,
        true_positives,
        false_negatives,
        false_positives,
        recall,
        negative_hours,
        false_positives_per_hour,
        accepted,
    }
}

fn count_debounced_hits(clip: &ClipScores, threshold: f32, debounce_seconds: f64) -> usize {
    let mut hits = 0usize;
    let mut last_hit_seconds = f64::NEG_INFINITY;
    for (index, score) in clip.scores.iter().enumerate() {
        let at_seconds = index as f64 * clip.score_interval_seconds;
        if *score >= threshold && at_seconds - last_hit_seconds >= debounce_seconds {
            hits += 1;
            last_hit_seconds = at_seconds;
        }
    }
    hits
}

#[derive(Debug)]
struct Args {
    model: PathBuf,
    positives: Vec<PathBuf>,
    negatives: Vec<PathBuf>,
    threshold: f32,
    gates: AcceptanceGates,
    report: Option<PathBuf>,
}

fn parse_args() -> Result<Args, String> {
    let mut model = None;
    let mut positives = Vec::new();
    let mut negatives = Vec::new();
    let mut threshold = DEFAULT_THRESHOLD;
    let mut gates = AcceptanceGates::default();
    let mut report = None;
    let mut args = std::env::args().skip(1);

    while let Some(arg) = args.next() {
        let value = |args: &mut std::iter::Skip<std::env::Args>, flag: &str| {
            args.next()
                .ok_or_else(|| format!("thiếu giá trị cho {flag}"))
        };
        match arg.as_str() {
            "--model" => model = Some(PathBuf::from(value(&mut args, "--model")?)),
            "--positive" => positives.push(PathBuf::from(value(&mut args, "--positive")?)),
            "--negative" => negatives.push(PathBuf::from(value(&mut args, "--negative")?)),
            "--threshold" => {
                threshold = value(&mut args, "--threshold")?
                    .parse()
                    .map_err(|_| "--threshold phải là số".to_string())?
            }
            "--min-recall" => {
                gates.min_recall = value(&mut args, "--min-recall")?
                    .parse()
                    .map_err(|_| "--min-recall phải là số".to_string())?
            }
            "--max-fpph" => {
                gates.max_false_positives_per_hour = value(&mut args, "--max-fpph")?
                    .parse()
                    .map_err(|_| "--max-fpph phải là số".to_string())?
            }
            "--min-negative-hours" => {
                gates.min_negative_hours = value(&mut args, "--min-negative-hours")?
                    .parse()
                    .map_err(|_| "--min-negative-hours phải là số".to_string())?
            }
            "--debounce-seconds" => {
                gates.debounce_seconds = value(&mut args, "--debounce-seconds")?
                    .parse()
                    .map_err(|_| "--debounce-seconds phải là số".to_string())?
            }
            "--report" => report = Some(PathBuf::from(value(&mut args, "--report")?)),
            "-h" | "--help" => {
                println!(
                    "wakeword_benchmark --model MODEL.onnx \\\n+  --positive FILE_OR_DIR [--positive ...] \\\n+  --negative FILE_OR_DIR [--negative ...] \\\n+  [--threshold 0.68] [--min-recall 0.90] [--max-fpph 1] \\\n+  [--min-negative-hours 1] [--debounce-seconds 2] [--report report.json]"
                );
                std::process::exit(0);
            }
            other => return Err(format!("tham số không hỗ trợ: {other}")),
        }
    }

    let model = model.ok_or_else(|| "bắt buộc có --model".to_string())?;
    if positives.is_empty() || negatives.is_empty() {
        return Err("bắt buộc có ít nhất một --positive và một --negative".to_string());
    }
    if !(0.0..=1.0).contains(&threshold) {
        return Err("--threshold phải nằm trong [0, 1]".to_string());
    }
    Ok(Args {
        model,
        positives,
        negatives,
        threshold,
        gates,
        report,
    })
}

fn collect_wavs(inputs: &[PathBuf]) -> Result<Vec<PathBuf>, String> {
    let mut wavs = Vec::new();
    for input in inputs {
        collect_wavs_at(input, &mut wavs)?;
    }
    wavs.sort();
    wavs.dedup();
    if wavs.is_empty() {
        return Err("không tìm thấy file .wav nào".to_string());
    }
    Ok(wavs)
}

fn collect_wavs_at(path: &Path, wavs: &mut Vec<PathBuf>) -> Result<(), String> {
    if path.is_dir() {
        for entry in std::fs::read_dir(path).map_err(|e| format!("đọc {:?}: {e}", path))? {
            let entry = entry.map_err(|e| format!("đọc entry {:?}: {e}", path))?;
            collect_wavs_at(&entry.path(), wavs)?;
        }
    } else if path
        .extension()
        .and_then(|value| value.to_str())
        .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
    {
        wavs.push(path.to_path_buf());
    } else if !path.exists() {
        return Err(format!("không tồn tại: {:?}", path));
    }
    Ok(())
}

fn read_wav_pcm16(path: &Path) -> Result<(u32, Vec<f32>), String> {
    let bytes = std::fs::read(path).map_err(|e| format!("đọc {:?}: {e}", path))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{:?} không phải RIFF/WAVE", path));
    }

    let mut pos = 12usize;
    let mut format = None;
    let mut channels = None;
    let mut sample_rate = None;
    let mut bits = None;
    let mut pcm = None;
    while pos + 8 <= bytes.len() {
        let id = &bytes[pos..pos + 4];
        let size = u32::from_le_bytes(
            bytes[pos + 4..pos + 8]
                .try_into()
                .map_err(|_| "WAV chunk size hỏng".to_string())?,
        ) as usize;
        let body_start = pos + 8;
        let body_end = body_start.saturating_add(size);
        if body_end > bytes.len() {
            return Err(format!("{:?} có WAV chunk vượt kích thước file", path));
        }
        if id == b"fmt " && size >= 16 {
            format = Some(u16::from_le_bytes(
                bytes[body_start..body_start + 2].try_into().unwrap(),
            ));
            channels = Some(u16::from_le_bytes(
                bytes[body_start + 2..body_start + 4].try_into().unwrap(),
            ));
            sample_rate = Some(u32::from_le_bytes(
                bytes[body_start + 4..body_start + 8].try_into().unwrap(),
            ));
            bits = Some(u16::from_le_bytes(
                bytes[body_start + 14..body_start + 16].try_into().unwrap(),
            ));
        } else if id == b"data" {
            pcm = Some(&bytes[body_start..body_end]);
        }
        pos = body_end + (size % 2);
    }

    let format = format.ok_or_else(|| format!("{:?} thiếu fmt chunk", path))?;
    let channels = channels.ok_or_else(|| format!("{:?} thiếu channel count", path))?;
    let sample_rate = sample_rate.ok_or_else(|| format!("{:?} thiếu sample rate", path))?;
    let bits = bits.ok_or_else(|| format!("{:?} thiếu bit depth", path))?;
    let pcm = pcm.ok_or_else(|| format!("{:?} thiếu data chunk", path))?;
    if format != 1 || bits != 16 || channels == 0 {
        return Err(format!(
            "{:?} phải là PCM16; nhận format={format}, bits={bits}, channels={channels}",
            path
        ));
    }
    if sample_rate != SAMPLE_RATE {
        return Err(format!(
            "{:?} phải là 16 kHz; nhận {sample_rate} Hz. Dùng ffmpeg -i input.wav -ar 16000 -ac 1 output.wav",
            path
        ));
    }

    let channel_count = channels as usize;
    let frame_bytes = channel_count * 2;
    let mut samples = Vec::with_capacity(pcm.len() / frame_bytes);
    for frame in pcm.chunks_exact(frame_bytes) {
        let sum = (0..channel_count)
            .map(|channel| {
                let offset = channel * 2;
                i16::from_le_bytes([frame[offset], frame[offset + 1]]) as f32 / 32_768.0
            })
            .sum::<f32>();
        samples.push(sum / channel_count as f32);
    }
    Ok((sample_rate, samples))
}

fn pad_positive(samples: &[f32]) -> Vec<f32> {
    let target = SAMPLE_RATE as usize * 5 / 2;
    if samples.len() >= target {
        return samples.to_vec();
    }
    let missing = target - samples.len();
    let leading = missing / 2;
    let mut padded = vec![0.0; leading];
    padded.extend_from_slice(samples);
    padded.resize(target, 0.0);
    padded
}

fn score_positives(model: &Path, paths: &[PathBuf]) -> Result<Vec<ClipScores>, String> {
    let mut detector = TrainedWakeDetector::new(&[model], 0.0)?;
    paths
        .iter()
        .map(|path| {
            let (_, samples) = read_wav_pcm16(path)?;
            let duration_seconds = samples.len() as f64 / SAMPLE_RATE as f64;
            let padded = pad_positive(&samples);
            let scores = detector.predict_raw(&padded)?;
            Ok(ClipScores::with_score_interval(
                &path.to_string_lossy(),
                duration_seconds,
                DEFAULT_DEBOUNCE_SECONDS,
                vec![scores.values().copied().fold(0.0f32, f32::max)],
            ))
        })
        .collect()
}

fn score_negatives(model: &Path, paths: &[PathBuf]) -> Result<Vec<ClipScores>, String> {
    paths
        .iter()
        .map(|path| {
            let (_, samples) = read_wav_pcm16(path)?;
            let duration_seconds = samples.len() as f64 / SAMPLE_RATE as f64;
            let mut detector = TrainedWakeDetector::new(&[model], 0.0)?;
            let mut raw_scores = Vec::new();
            for frame in samples.chunks(SCORE_INTERVAL_SAMPLES) {
                if let Some(scores) = detector.push_scores(frame)? {
                    raw_scores.push(scores.values().copied().fold(0.0f32, f32::max));
                }
            }
            Ok(ClipScores::with_score_interval(
                &path.to_string_lossy(),
                duration_seconds,
                SCORE_INTERVAL_SAMPLES as f64 / SAMPLE_RATE as f64,
                raw_scores,
            ))
        })
        .collect()
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("đọc model {:?}: {e}", path))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

fn run() -> Result<bool, String> {
    let args = parse_args()?;
    let positive_paths = collect_wavs(&args.positives)?;
    let negative_paths = collect_wavs(&args.negatives)?;
    let positive_clips = score_positives(&args.model, &positive_paths)?;
    let negative_clips = score_negatives(&args.model, &negative_paths)?;
    let metrics = evaluate_with_gates(
        &positive_clips,
        &negative_clips,
        args.threshold,
        &args.gates,
    );
    let accepted = metrics.accepted;
    let report = BenchmarkReport {
        model_path: args.model.to_string_lossy().into_owned(),
        model_sha256: sha256_file(&args.model)?,
        gates: args.gates,
        metrics,
        positive_clips,
        negative_clips,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?;
    println!("{json}");
    if let Some(path) = args.report {
        if let Some(parent) = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
        {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("tạo thư mục report {:?}: {e}", parent))?;
        }
        std::fs::write(&path, format!("{json}\n"))
            .map_err(|e| format!("ghi report {:?}: {e}", path))?;
    }
    Ok(accepted)
}

fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(2),
        Err(error) => {
            eprintln!("wakeword_benchmark: {error}");
            std::process::exit(1);
        }
    }
}

// `mod tests` phải là item CUỐI file: clippy `items_after_test_module` là gate
// cứng của dự án (`-D warnings`), và `fn main` nằm sau khối test sẽ bật nó.
#[cfg(test)]
mod tests {
    use super::{ClipScores, evaluate_threshold};

    #[test]
    fn acceptance_requires_recall_fpph_and_negative_duration_together() {
        let positives = vec![
            ClipScores::new("p1.wav", 2.0, vec![0.92]),
            ClipScores::new("p2.wav", 2.0, vec![0.81]),
            ClipScores::new("p3.wav", 2.0, vec![0.20]),
        ];
        let negatives = vec![ClipScores::new(
            "room.wav",
            3600.0,
            vec![0.10, 0.72, 0.69, 0.11],
        )];

        let metrics = evaluate_threshold(&positives, &negatives, 0.68, 2.0);

        assert_eq!(metrics.true_positives, 2);
        assert_eq!(metrics.false_negatives, 1);
        assert_eq!(metrics.false_positives, 1);
        assert_eq!(metrics.recall, 2.0 / 3.0);
        assert_eq!(metrics.false_positives_per_hour, 1.0);
        assert!(!metrics.accepted);
    }

    #[test]
    fn nearby_negative_hits_inside_debounce_count_once() {
        let positives = vec![ClipScores::new("p.wav", 2.0, vec![0.90])];
        let negatives = vec![ClipScores::with_score_interval(
            "tv.wav",
            3600.0,
            0.2,
            vec![0.80, 0.91, 0.95, 0.10],
        )];

        let metrics = evaluate_threshold(&positives, &negatives, 0.68, 2.0);

        assert_eq!(metrics.false_positives, 1);
        assert_eq!(metrics.false_positives_per_hour, 1.0);
        assert!(metrics.accepted);
    }
}
