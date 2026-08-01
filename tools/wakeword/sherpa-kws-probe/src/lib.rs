use serde::Serialize;
use sherpa_onnx::{KeywordSpotter, KeywordSpotterConfig, Wave};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const WAKE_KEYWORD: &str = "HEY LIVA";

#[derive(Clone, Debug)]
pub struct SherpaWakeConfig {
    pub model_dir: PathBuf,
    pub encoder: PathBuf,
    pub decoder: PathBuf,
    pub joiner: PathBuf,
    pub tokens: PathBuf,
    pub keywords: PathBuf,
    pub keyword: &'static str,
    pub num_threads: i32,
    pub keywords_score: f32,
    pub keywords_threshold: f32,
}

impl SherpaWakeConfig {
    pub fn for_model_dir(model_dir: PathBuf) -> Self {
        Self {
            encoder: model_dir.join("encoder.int8.onnx"),
            decoder: model_dir.join("decoder.onnx"),
            joiner: model_dir.join("joiner.int8.onnx"),
            tokens: model_dir.join("tokens.txt"),
            keywords: model_dir.join("keywords.txt"),
            model_dir,
            keyword: WAKE_KEYWORD,
            num_threads: 1,
            keywords_score: 1.0,
            keywords_threshold: 0.25,
        }
    }

    pub fn validate_artifacts(&self) -> Result<(), String> {
        let root = self
            .model_dir
            .canonicalize()
            .map_err(|error| format!("model directory is unavailable: {error}"))?;

        for candidate in [
            &self.encoder,
            &self.decoder,
            &self.joiner,
            &self.tokens,
            &self.keywords,
        ] {
            let resolved = candidate.canonicalize().map_err(|error| {
                format!(
                    "artifact is missing or unreadable ({}): {error}",
                    candidate.display()
                )
            })?;
            if !resolved.starts_with(&root) {
                return Err(format!(
                    "artifact escapes the model directory: {}",
                    candidate.display()
                ));
            }
            if !resolved.is_file() {
                return Err(format!(
                    "artifact is not a regular file: {}",
                    candidate.display()
                ));
            }
        }
        Ok(())
    }
}

pub struct SherpaWakeDetector {
    spotter: KeywordSpotter,
    keyword: &'static str,
}

impl SherpaWakeDetector {
    pub fn new(config: SherpaWakeConfig) -> Result<Self, String> {
        config.validate_artifacts()?;

        let mut native = KeywordSpotterConfig::default();
        native.model_config.transducer.encoder = Some(path_string(&config.encoder)?);
        native.model_config.transducer.decoder = Some(path_string(&config.decoder)?);
        native.model_config.transducer.joiner = Some(path_string(&config.joiner)?);
        native.model_config.tokens = Some(path_string(&config.tokens)?);
        native.model_config.provider = Some("cpu".to_owned());
        native.model_config.num_threads = config.num_threads;
        native.keywords_file = Some(path_string(&config.keywords)?);
        native.keywords_score = config.keywords_score;
        native.keywords_threshold = config.keywords_threshold;

        let spotter = KeywordSpotter::create(&native)
            .ok_or_else(|| "failed to initialize sherpa keyword spotter".to_owned())?;
        Ok(Self {
            spotter,
            keyword: config.keyword,
        })
    }

    pub fn detect(&self, samples: &[f32], sample_rate: i32) -> Option<String> {
        if samples.is_empty() || sample_rate <= 0 {
            return None;
        }

        let stream = self.spotter.create_stream();
        stream.accept_waveform(sample_rate, samples);
        stream.accept_waveform(sample_rate, &vec![0.0_f32; sample_rate as usize]);
        stream.input_finished();

        while self.spotter.is_ready(&stream) {
            self.spotter.decode(&stream);
            if let Some(result) = self.spotter.get_result(&stream) {
                let detected = normalize_keyword(&result.keyword);
                if detected == self.keyword {
                    return Some(detected);
                }
            }
        }
        None
    }
}

#[derive(Debug, Serialize)]
pub struct CorpusMetrics {
    pub keyword: &'static str,
    pub positive_total: usize,
    pub true_positive: usize,
    pub false_negative: usize,
    pub recall: f64,
    pub negative_total: usize,
    pub false_positive: usize,
    pub true_negative: usize,
    pub negative_audio_hours: f64,
    pub false_positives_per_hour: f64,
    pub elapsed_seconds: f64,
}

pub fn benchmark_corpus(
    detector: &SherpaWakeDetector,
    positives: &[PathBuf],
    negatives: &[PathBuf],
) -> Result<CorpusMetrics, String> {
    let started = Instant::now();
    let mut true_positive = 0;
    let mut negative_audio_seconds = 0.0;
    let mut false_positive = 0;

    for path in positives {
        let wave = read_wave(path)?;
        if detector
            .detect(wave.samples(), wave.sample_rate())
            .is_some()
        {
            true_positive += 1;
        }
    }
    for path in negatives {
        let wave = read_wave(path)?;
        negative_audio_seconds += wave.samples().len() as f64 / wave.sample_rate() as f64;
        if detector
            .detect(wave.samples(), wave.sample_rate())
            .is_some()
        {
            false_positive += 1;
        }
    }

    let positive_total = positives.len();
    let negative_total = negatives.len();
    let negative_audio_hours = negative_audio_seconds / 3600.0;
    Ok(CorpusMetrics {
        keyword: WAKE_KEYWORD,
        positive_total,
        true_positive,
        false_negative: positive_total - true_positive,
        recall: ratio(true_positive, positive_total),
        negative_total,
        false_positive,
        true_negative: negative_total - false_positive,
        negative_audio_hours,
        false_positives_per_hour: if negative_audio_hours > 0.0 {
            false_positive as f64 / negative_audio_hours
        } else {
            0.0
        },
        elapsed_seconds: started.elapsed().as_secs_f64(),
    })
}

pub fn wav_files(directory: &Path, limit: Option<usize>) -> Result<Vec<PathBuf>, String> {
    let mut files = std::fs::read_dir(directory)
        .map_err(|error| format!("cannot read corpus {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    if let Some(limit) = limit {
        files = evenly_sample(files, limit);
    }
    Ok(files)
}

fn evenly_sample<T>(items: Vec<T>, limit: usize) -> Vec<T> {
    if limit == 0 {
        return Vec::new();
    }
    if items.len() <= limit {
        return items;
    }
    if limit == 1 {
        return items.into_iter().take(1).collect();
    }

    let last = items.len() - 1;
    let selected = (0..limit)
        .map(|position| position * last / (limit - 1))
        .collect::<std::collections::HashSet<_>>();
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| selected.contains(&index).then_some(item))
        .collect()
}

fn read_wave(path: &Path) -> Result<Wave, String> {
    let path = path_string(path)?;
    Wave::read(&path).ok_or_else(|| format!("cannot read WAV: {path}"))
}

fn path_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
}

fn normalize_keyword(keyword: &str) -> String {
    keyword
        .replace('_', " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_uppercase()
}

fn ratio(numerator: usize, denominator: usize) -> f64 {
    if denominator == 0 {
        0.0
    } else {
        numerator as f64 / denominator as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_chi_nhan_hey_liva_va_model_int8_cpu_mot_luong() {
        let config = SherpaWakeConfig::for_model_dir("model".into());

        assert_eq!(config.keyword, "HEY LIVA");
        assert_eq!(config.num_threads, 1);
        assert_eq!(config.encoder.file_name().unwrap(), "encoder.int8.onnx");
        assert_eq!(config.decoder.file_name().unwrap(), "decoder.onnx");
        assert_eq!(config.joiner.file_name().unwrap(), "joiner.int8.onnx");
    }

    #[test]
    fn config_fail_closed_khi_thieu_artifact() {
        let root = std::env::temp_dir().join(format!(
            "liva_sherpa_missing_{}_{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let error = SherpaWakeConfig::for_model_dir(root.clone())
            .validate_artifacts()
            .unwrap_err();
        assert!(error.contains("encoder.int8.onnx"), "{error}");
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detect_tu_choi_audio_rong_hoac_sample_rate_sai() {
        assert_eq!(normalize_keyword("  hey_liva "), "HEY LIVA");
        assert_eq!(ratio(9, 10), 0.9);
        assert_eq!(ratio(0, 0), 0.0);
    }

    #[test]
    fn corpus_limit_lay_mau_deu_thay_vi_chi_lay_dau_thu_muc() {
        assert_eq!(evenly_sample((0..10).collect::<Vec<_>>(), 3), [0, 4, 9]);
        assert_eq!(evenly_sample((0..3).collect::<Vec<_>>(), 10), [0, 1, 2]);
        assert!(evenly_sample((0..3).collect::<Vec<_>>(), 0).is_empty());
    }
}
