//! `wer_bench` measures Vietnamese ASR quality through LIVA's production STT
//! manager, not through a model-specific ONNX shortcut.
//!
//! Expected input is a JSONL manifest produced by `scripts/prepare-fleurs-vi.py`:
//! `{"audio":"audio/0000.wav","transcript":"..."}`.

use liva_native_core::stt::SttManager;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

const DEFAULT_MIN_SAMPLES: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Engine {
    Nemotron,
    Parakeet,
}

impl Engine {
    fn name(self) -> &'static str {
        match self {
            Self::Nemotron => "nemotron",
            Self::Parakeet => "parakeet",
        }
    }
}

#[derive(Debug)]
struct Args {
    manifest: PathBuf,
    model_dir: PathBuf,
    engines: Vec<Engine>,
    limit: usize,
    min_samples: usize,
    output: Option<PathBuf>,
}

#[derive(Debug, Deserialize)]
struct ManifestRow {
    audio: String,
    #[serde(alias = "transcription")]
    transcript: String,
}

#[derive(Debug)]
struct Sample {
    audio: PathBuf,
    transcript: String,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize)]
struct WordErrors {
    substitutions: usize,
    deletions: usize,
    insertions: usize,
    reference_words: usize,
}

impl WordErrors {
    fn add(&mut self, other: Self) {
        self.substitutions += other.substitutions;
        self.deletions += other.deletions;
        self.insertions += other.insertions;
        self.reference_words += other.reference_words;
    }

    fn total(self) -> usize {
        self.substitutions + self.deletions + self.insertions
    }
}

#[derive(Debug, Serialize)]
struct EngineResult {
    engine: &'static str,
    active_engine: &'static str,
    samples: usize,
    substitutions: usize,
    deletions: usize,
    insertions: usize,
    reference_words: usize,
    wer_percent: f64,
    audio_seconds: f64,
    elapsed_seconds: f64,
    real_time_factor: f64,
    end_of_turn_latency_p50_ms: f64,
    end_of_turn_latency_p95_ms: f64,
}

#[derive(Debug, Serialize)]
struct BenchmarkReport {
    dataset: &'static str,
    manifest: String,
    production_api: &'static str,
    parakeet_model_sha256: Option<String>,
    parakeet_model_revision: Option<String>,
    results: Vec<EngineResult>,
}

fn normalize_words(text: &str) -> Vec<String> {
    let normalized: String = text
        .to_lowercase()
        .chars()
        .map(|ch| {
            if ch.is_alphanumeric() || ch.is_whitespace() {
                ch
            } else {
                ' '
            }
        })
        .collect();
    normalized
        .split_whitespace()
        .map(ToOwned::to_owned)
        .collect()
}

fn align_words(reference: &[String], hypothesis: &[String]) -> WordErrors {
    let n = reference.len();
    let m = hypothesis.len();
    let mut cost = vec![vec![0usize; m + 1]; n + 1];
    for (i, row) in cost.iter_mut().enumerate() {
        row[0] = i;
    }
    // `cost[0]` dài đúng `m + 1`, nên `enumerate()` chạy j = 0..=m — tương
    // đương vòng chỉ số cũ, và cùng dạng với vòng đặt cột 0 ngay trên.
    for (j, cell) in cost[0].iter_mut().enumerate() {
        *cell = j;
    }

    for i in 1..=n {
        for j in 1..=m {
            if reference[i - 1] == hypothesis[j - 1] {
                cost[i][j] = cost[i - 1][j - 1];
            } else {
                cost[i][j] = 1 + cost[i - 1][j - 1].min(cost[i - 1][j]).min(cost[i][j - 1]);
            }
        }
    }

    let (mut i, mut j) = (n, m);
    let mut errors = WordErrors {
        reference_words: n,
        ..WordErrors::default()
    };
    while i > 0 || j > 0 {
        if i > 0
            && j > 0
            && reference[i - 1] == hypothesis[j - 1]
            && cost[i][j] == cost[i - 1][j - 1]
        {
            i -= 1;
            j -= 1;
        } else if i > 0 && j > 0 && cost[i][j] == cost[i - 1][j - 1] + 1 {
            errors.substitutions += 1;
            i -= 1;
            j -= 1;
        } else if i > 0 && cost[i][j] == cost[i - 1][j] + 1 {
            errors.deletions += 1;
            i -= 1;
        } else {
            errors.insertions += 1;
            j -= 1;
        }
    }
    errors
}

fn read_manifest(path: &Path, limit: usize, min_samples: usize) -> Result<Vec<Sample>, String> {
    let raw = std::fs::read_to_string(path)
        .map_err(|error| format!("không đọc được manifest {:?}: {error}", path))?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    let mut samples = Vec::new();
    for (index, line) in raw.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let row: ManifestRow = serde_json::from_str(line)
            .map_err(|error| format!("manifest dòng {} không hợp lệ: {error}", index + 1))?;
        if row.transcript.trim().is_empty() {
            return Err(format!("manifest dòng {} có transcript rỗng", index + 1));
        }
        let audio = base.join(row.audio);
        if !audio.is_file() {
            return Err(format!(
                "manifest dòng {} thiếu audio {:?}",
                index + 1,
                audio
            ));
        }
        samples.push(Sample {
            audio,
            transcript: row.transcript,
        });
        if samples.len() == limit {
            break;
        }
    }
    if samples.len() < min_samples {
        return Err(format!(
            "cần ít nhất {min_samples} câu, manifest/limit chỉ cho {} câu",
            samples.len()
        ));
    }
    Ok(samples)
}

fn read_wav_pcm16_mono(path: &Path) -> Result<(u32, Vec<f32>), String> {
    let bytes =
        std::fs::read(path).map_err(|error| format!("không đọc được {:?}: {error}", path))?;
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WAVE" {
        return Err(format!("{:?} không phải RIFF/WAVE", path));
    }

    let mut cursor = 12usize;
    let mut format = None;
    let mut data = None;
    while cursor + 8 <= bytes.len() {
        let id = &bytes[cursor..cursor + 4];
        let len = u32::from_le_bytes(bytes[cursor + 4..cursor + 8].try_into().unwrap()) as usize;
        let start = cursor + 8;
        let end = start
            .checked_add(len)
            .filter(|end| *end <= bytes.len())
            .ok_or_else(|| format!("{:?} có WAV chunk vượt kích thước file", path))?;
        if id == b"fmt " {
            if len < 16 {
                return Err(format!("{:?} có fmt chunk quá ngắn", path));
            }
            format = Some((
                u16::from_le_bytes(bytes[start..start + 2].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 2..start + 4].try_into().unwrap()),
                u32::from_le_bytes(bytes[start + 4..start + 8].try_into().unwrap()),
                u16::from_le_bytes(bytes[start + 14..start + 16].try_into().unwrap()),
            ));
        } else if id == b"data" {
            data = Some((start, end));
        }
        cursor = end + (len % 2);
    }

    let (audio_format, channels, sample_rate, bits) =
        format.ok_or_else(|| format!("{:?} thiếu fmt chunk", path))?;
    if audio_format != 1 || channels != 1 || bits != 16 {
        return Err(format!(
            "{:?} phải là PCM16 mono; format={audio_format}, channels={channels}, bits={bits}",
            path
        ));
    }
    if sample_rate != 16_000 {
        return Err(format!("{:?} phải là 16 kHz; nhận {sample_rate} Hz", path));
    }
    let (start, end) = data.ok_or_else(|| format!("{:?} thiếu data chunk", path))?;
    if (end - start) % 2 != 0 {
        return Err(format!("{:?} có data PCM16 lẻ byte", path));
    }
    let samples = bytes[start..end]
        .chunks_exact(2)
        .map(|pair| i16::from_le_bytes([pair[0], pair[1]]) as f32 / 32768.0)
        .collect();
    Ok((sample_rate, samples))
}

fn configure_engine(engine: Engine) {
    // SAFETY: wer_bench is single-threaded and sets this process-level selector
    // before constructing SttManager. No other thread reads or writes it here.
    unsafe {
        std::env::set_var("LIVA_STT_LANGUAGE", "vi-VN");
        std::env::set_var("LIVA_STT_VI_ENGINE", engine.name());
    }
}

fn validate_engine_assets(engine: Engine, model_dir: &Path) -> Result<(), String> {
    for name in [
        "encoder.onnx",
        "decoder.onnx",
        "joint.onnx",
        "tokenizer.json",
    ] {
        let path = model_dir.join(name);
        if !path.is_file() {
            return Err(format!("thiếu model Nemotron {:?}", path));
        }
    }
    if engine == Engine::Parakeet {
        let model = std::env::var_os("LIVA_PARAKEET_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("models/parakeet_vi.onnx"));
        let vocab = std::env::var_os("LIVA_PARAKEET_VOCAB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| model.with_file_name("parakeet_vi_vocab.json"));
        if !model.is_file() || !vocab.is_file() {
            return Err(format!(
                "thiếu model/vocab Parakeet: {:?}, {:?}",
                model, vocab
            ));
        }
    }
    Ok(())
}

fn percentile(samples: &[Duration], percentile: f64) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    let rank = ((percentile / 100.0) * sorted.len() as f64).ceil().max(1.0) as usize;
    sorted[(rank - 1).min(sorted.len() - 1)]
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("đọc model {:?}: {e}", path))?;
    Ok(Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[derive(Debug, PartialEq, Eq)]
struct ParakeetProvenance {
    sha256: String,
    revision: String,
}

fn parakeet_provenance(
    model_path: &Path,
    manifest_path: &Path,
) -> Result<ParakeetProvenance, String> {
    let manifest_raw = std::fs::read_to_string(manifest_path)
        .map_err(|error| format!("đọc manifest model {:?}: {error}", manifest_path))?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_raw)
        .map_err(|error| format!("parse manifest model {:?}: {error}", manifest_path))?;
    let entry = manifest["files"]
        .as_array()
        .and_then(|files| {
            files
                .iter()
                .find(|entry| entry["dest"] == "models/parakeet_vi.onnx")
        })
        .ok_or_else(|| "manifest thiếu entry models/parakeet_vi.onnx".to_string())?;
    let url = entry["url"]
        .as_str()
        .ok_or_else(|| "entry Parakeet thiếu url".to_string())?;
    let revision = url
        .split_once("/resolve/")
        .and_then(|(_, tail)| tail.split('/').next())
        .filter(|revision| {
            revision.len() == 40 && revision.chars().all(|ch| ch.is_ascii_hexdigit())
        })
        .ok_or_else(|| format!("URL Parakeet không chứa revision cố định: {url}"))?;
    let expected_sha256 = entry["sha256"]
        .as_str()
        .ok_or_else(|| "entry Parakeet thiếu sha256".to_string())?;
    let sha256 = sha256_file(model_path)?;
    if !sha256.eq_ignore_ascii_case(expected_sha256) {
        return Err(format!(
            "SHA-256 Parakeet không khớp manifest: thực tế {sha256}, mong đợi {expected_sha256}"
        ));
    }

    Ok(ParakeetProvenance {
        sha256,
        revision: revision.to_string(),
    })
}

fn validate_active_engine(
    requested: Engine,
    active: &str,
    fallback_reason: Option<&str>,
) -> Result<(), String> {
    if requested != Engine::Parakeet || active == "Parakeet-vi" {
        return Ok(());
    }
    Err(format!(
        "yêu cầu benchmark Parakeet nhưng runtime đang dùng {active}{}",
        fallback_reason
            .map(|reason| format!(" · đã lùi engine: {reason}"))
            .unwrap_or_default()
    ))
}

fn run_engine(
    engine: Engine,
    model_dir: &Path,
    samples: &[Sample],
) -> Result<EngineResult, String> {
    validate_engine_assets(engine, model_dir)?;
    configure_engine(engine);
    let mut stt = SttManager::new(model_dir);
    stt.set_language("vi-VN")?;

    let started = Instant::now();
    let mut aggregate = WordErrors::default();
    let mut audio_seconds = 0.0f64;
    let mut end_of_turn_latencies = Vec::with_capacity(samples.len());
    for (index, sample) in samples.iter().enumerate() {
        let (sample_rate, audio) = read_wav_pcm16_mono(&sample.audio)?;
        audio_seconds += audio.len() as f64 / sample_rate as f64;
        let vad_ended = Instant::now();
        let hypothesis = stt
            .feed_audio(&audio, true)?
            .ok_or_else(|| format!("STT không trả final transcript cho {:?}", sample.audio))?;
        if index == 0 {
            let (active, fallback_reason) = stt.active_vietnamese_engine();
            validate_active_engine(engine, active, fallback_reason)?;
        }
        end_of_turn_latencies.push(vad_ended.elapsed());
        aggregate.add(align_words(
            &normalize_words(&sample.transcript),
            &normalize_words(&hypothesis),
        ));
        if (index + 1) % 10 == 0 || index + 1 == samples.len() {
            eprintln!("{}: {}/{} câu", engine.name(), index + 1, samples.len());
        }
    }
    let elapsed_seconds = started.elapsed().as_secs_f64();
    let wer_percent = if aggregate.reference_words == 0 {
        0.0
    } else {
        100.0 * aggregate.total() as f64 / aggregate.reference_words as f64
    };
    let (active_engine, fallback_reason) = stt.active_vietnamese_engine();
    validate_active_engine(engine, active_engine, fallback_reason)?;
    Ok(EngineResult {
        engine: engine.name(),
        active_engine,
        samples: samples.len(),
        substitutions: aggregate.substitutions,
        deletions: aggregate.deletions,
        insertions: aggregate.insertions,
        reference_words: aggregate.reference_words,
        wer_percent,
        audio_seconds,
        elapsed_seconds,
        real_time_factor: elapsed_seconds / audio_seconds,
        end_of_turn_latency_p50_ms: percentile(&end_of_turn_latencies, 50.0).as_secs_f64()
            * 1_000.0,
        end_of_turn_latency_p95_ms: percentile(&end_of_turn_latencies, 95.0).as_secs_f64()
            * 1_000.0,
    })
}

fn parse_args(raw: impl IntoIterator<Item = String>) -> Result<Args, String> {
    let mut args = raw.into_iter();
    let _program = args.next();
    let mut manifest = None;
    let mut model_dir = PathBuf::from("models/nemotron-asr");
    let mut engines = vec![Engine::Nemotron, Engine::Parakeet];
    let mut limit = DEFAULT_MIN_SAMPLES;
    let mut min_samples = DEFAULT_MIN_SAMPLES;
    let mut output = None;

    while let Some(arg) = args.next() {
        let value = |args: &mut dyn Iterator<Item = String>, flag: &str| {
            args.next().ok_or_else(|| format!("{flag} thiếu giá trị"))
        };
        match arg.as_str() {
            "--manifest" => manifest = Some(PathBuf::from(value(&mut args, "--manifest")?)),
            "--model-dir" => model_dir = PathBuf::from(value(&mut args, "--model-dir")?),
            "--engine" => {
                engines = match value(&mut args, "--engine")?.as_str() {
                    "nemotron" => vec![Engine::Nemotron],
                    "parakeet" => vec![Engine::Parakeet],
                    "both" => vec![Engine::Nemotron, Engine::Parakeet],
                    other => return Err(format!("engine không hợp lệ: {other}")),
                }
            }
            "--limit" => {
                limit = value(&mut args, "--limit")?
                    .parse()
                    .map_err(|_| "--limit phải là số nguyên dương".to_string())?
            }
            "--min-samples" => {
                min_samples = value(&mut args, "--min-samples")?
                    .parse()
                    .map_err(|_| "--min-samples phải là số nguyên dương".to_string())?
            }
            "--output" => output = Some(PathBuf::from(value(&mut args, "--output")?)),
            "-h" | "--help" => return Err(usage().to_string()),
            other => return Err(format!("tham số không biết: {other}\n{}", usage())),
        }
    }
    if limit == 0 || min_samples == 0 {
        return Err("--limit và --min-samples phải > 0".to_string());
    }
    Ok(Args {
        manifest: manifest.ok_or_else(|| format!("thiếu --manifest\n{}", usage()))?,
        model_dir,
        engines,
        limit,
        min_samples,
        output,
    })
}

fn usage() -> &'static str {
    "Dùng: wer_bench --manifest <fleurs-vi.jsonl> [--engine both|nemotron|parakeet] \
     [--limit 100] [--min-samples 100] [--model-dir models/nemotron-asr] \
     [--output docs/05-chat-luong/wer-fleurs-vi.json]"
}

fn run() -> Result<(), String> {
    let args = parse_args(std::env::args())?;
    let samples = read_manifest(&args.manifest, args.limit, args.min_samples)?;
    let parakeet_provenance = if args.engines.contains(&Engine::Parakeet) {
        let model = std::env::var_os("LIVA_PARAKEET_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("models/parakeet_vi.onnx"));
        Some(parakeet_provenance(
            &model,
            Path::new("data/models-manifest.json"),
        )?)
    } else {
        None
    };
    let mut results = Vec::new();
    for engine in args.engines {
        results.push(run_engine(engine, &args.model_dir, &samples)?);
    }
    let report = BenchmarkReport {
        dataset: "google/fleurs vi_vn test",
        manifest: args.manifest.display().to_string(),
        production_api: "liva_native_core::stt::SttManager::feed_audio(audio, true)",
        parakeet_model_sha256: parakeet_provenance
            .as_ref()
            .map(|provenance| provenance.sha256.clone()),
        parakeet_model_revision: parakeet_provenance.map(|provenance| provenance.revision),
        results,
    };
    let json = serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?;
    println!("{json}");
    if let Some(output) = args.output {
        if let Some(parent) = output.parent() {
            std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        std::fs::write(&output, format!("{json}\n"))
            .map_err(|error| format!("không ghi được {:?}: {error}", output))?;
    }
    Ok(())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("wer_bench: {error}");
        std::process::exit(2);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn words(items: &[&str]) -> Vec<String> {
        items.iter().map(|item| (*item).to_string()).collect()
    }

    #[test]
    fn normalization_keeps_vietnamese_letters_and_removes_punctuation() {
        assert_eq!(
            normalize_words("  Xin chào, LIVA! Tôi  ở đây. "),
            words(&["xin", "chào", "liva", "tôi", "ở", "đây"])
        );
    }

    #[test]
    fn alignment_counts_substitution_deletion_and_insertion() {
        let errors = align_words(
            &words(&["mốc-a", "xóa", "mốc-b", "đổi", "mốc-c"]),
            &words(&["mốc-a", "mốc-b", "khác", "mốc-c", "thêm"]),
        );
        assert_eq!(
            errors,
            WordErrors {
                substitutions: 1,
                deletions: 1,
                insertions: 1,
                reference_words: 5,
            }
        );
    }

    #[test]
    fn alignment_handles_empty_hypothesis() {
        assert_eq!(
            align_words(&words(&["xin", "chào"]), &[]),
            WordErrors {
                substitutions: 0,
                deletions: 2,
                insertions: 0,
                reference_words: 2,
            }
        );
    }

    #[test]
    fn cli_defaults_to_both_engines_and_one_hundred_samples() {
        let args = parse_args([
            "wer_bench".to_string(),
            "--manifest".to_string(),
            "fleurs.jsonl".to_string(),
        ])
        .unwrap();
        assert_eq!(args.engines, vec![Engine::Nemotron, Engine::Parakeet]);
        assert_eq!(args.limit, 100);
        assert_eq!(args.min_samples, 100);
        let samples: Vec<Duration> = (1..=100).map(Duration::from_millis).collect();

        assert_eq!(percentile(&samples, 50.0), Duration::from_millis(50));
        assert_eq!(percentile(&samples, 95.0), Duration::from_millis(95));
        assert_eq!(percentile(&[], 95.0), Duration::ZERO);
    }

    #[test]
    fn report_serializes_parakeet_sha256_and_manifest_revision() {
        let expected = "aa5658c3499fc991780e44ad5ccd9d4393d1266727a281cb3e4ca39be42334c4";
        let revision = "240d82cc243f7cf47d100b293c7dff96e65a04c2";
        let report = BenchmarkReport {
            dataset: "test",
            manifest: "test.jsonl".to_string(),
            production_api: "SttManager::feed_audio",
            parakeet_model_sha256: Some(expected.to_string()),
            parakeet_model_revision: Some(revision.to_string()),
            results: Vec::new(),
        };
        let value = serde_json::to_value(report).unwrap();
        assert_eq!(value["parakeet_model_sha256"], expected);
        assert_eq!(value["parakeet_model_revision"], revision);
    }

    #[test]
    fn hashes_the_exact_model_file_used_by_the_benchmark() {
        let path = std::env::temp_dir().join(format!("liva-wer-hash-{}.onnx", std::process::id()));
        std::fs::write(&path, b"abc").unwrap();
        let digest = sha256_file(&path).unwrap();
        std::fs::remove_file(path).unwrap();
        assert_eq!(
            digest,
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }

    #[test]
    fn parakeet_benchmark_rejects_a_silent_fallback() {
        let error = validate_active_engine(
            Engine::Parakeet,
            "Nemotron",
            Some("missing external weights"),
        )
        .unwrap_err();
        assert!(error.contains("Nemotron"));
        assert!(error.contains("missing external weights"));
        assert!(validate_active_engine(Engine::Parakeet, "Parakeet-vi", None).is_ok());
    }

    #[test]
    fn parakeet_provenance_reads_revision_from_the_matching_manifest_entry() {
        let root = std::env::temp_dir().join(format!("liva-wer-provenance-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let model = root.join("parakeet_vi.onnx");
        let manifest = root.join("models-manifest.json");
        std::fs::write(&model, b"abc").unwrap();
        std::fs::write(
            &manifest,
            r#"{"files":[{"dest":"models/parakeet_vi.onnx","url":"https://huggingface.co/OpenVoiceOS/model/resolve/240d82cc243f7cf47d100b293c7dff96e65a04c2/model.onnx","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]}"#,
        )
        .unwrap();

        let provenance = parakeet_provenance(&model, &manifest).unwrap();

        assert_eq!(
            provenance.revision,
            "240d82cc243f7cf47d100b293c7dff96e65a04c2"
        );
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn parakeet_provenance_rejects_a_model_that_does_not_match_the_manifest() {
        let root = std::env::temp_dir().join(format!(
            "liva-wer-provenance-mismatch-{}",
            std::process::id()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let model = root.join("parakeet_vi.onnx");
        let manifest = root.join("models-manifest.json");
        std::fs::write(&model, b"not the published graph").unwrap();
        std::fs::write(
            &manifest,
            r#"{"files":[{"dest":"models/parakeet_vi.onnx","url":"https://huggingface.co/OpenVoiceOS/model/resolve/240d82cc243f7cf47d100b293c7dff96e65a04c2/model.onnx","sha256":"ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"}]}"#,
        )
        .unwrap();

        let error = parakeet_provenance(&model, &manifest).unwrap_err();

        assert!(error.contains("SHA-256"));
        let _ = std::fs::remove_dir_all(root);
    }
}
