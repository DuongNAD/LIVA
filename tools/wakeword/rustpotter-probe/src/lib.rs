use rustpotter::{
    Rustpotter, RustpotterConfig, ScoreMode, VADMode, WakewordRef, WakewordRefBuildFromFiles,
};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub const WAKE_KEYWORD: &str = "HEY LIVA";
const SAMPLE_RATE: u32 = 16_000;
const CHANNELS: u16 = 1;
const MFCC_SIZE: u16 = 16;

#[derive(Clone, Debug)]
pub struct PersonalWakeConfig {
    pub threshold: f32,
    pub avg_threshold: f32,
    pub min_scores: usize,
}

impl Default for PersonalWakeConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            avg_threshold: 0.2,
            min_scores: 5,
        }
    }
}

pub struct PersonalWakeDetector {
    detector: Rustpotter,
}

impl PersonalWakeDetector {
    pub fn enroll(enrollment: &[PathBuf], config: &PersonalWakeConfig) -> Result<Self, String> {
        if !(3..=8).contains(&enrollment.len()) {
            return Err(format!(
                "personal wake enrollment requires 3 to 8 WAV files, got {}",
                enrollment.len()
            ));
        }
        let enrollment = enrollment
            .iter()
            .map(|path| path_string(path))
            .collect::<Result<Vec<_>, _>>()?;
        let wakeword = WakewordRef::new_from_sample_files(
            WAKE_KEYWORD.to_owned(),
            None,
            None,
            enrollment,
            MFCC_SIZE,
        )?;

        let mut native = RustpotterConfig::default();
        native.fmt.sample_rate = SAMPLE_RATE as usize;
        native.fmt.channels = CHANNELS;
        native.detector.threshold = config.threshold;
        native.detector.avg_threshold = config.avg_threshold;
        native.detector.min_scores = config.min_scores;
        native.detector.score_mode = ScoreMode::P50;
        native.detector.vad_mode = Some(VADMode::Medium);
        native.detector.eager = false;

        let mut detector = Rustpotter::new(&native)?;
        detector.add_wakeword_ref("hey_liva", wakeword)?;
        Ok(Self { detector })
    }

    pub fn detect_clip(&mut self, samples: &[f32]) -> bool {
        self.detector.reset();
        let frame_size = self.detector.get_samples_per_frame();
        if frame_size == 0 || samples.is_empty() {
            return false;
        }

        let mut frame = vec![0.0_f32; frame_size];
        for chunk in samples.chunks(frame_size) {
            frame.fill(0.0);
            frame[..chunk.len()].copy_from_slice(chunk);
            if self.detector.process_samples(frame.clone()).is_some() {
                return true;
            }
        }

        // Flush the partial-detection window after a finite clip.
        for _ in 0..(SAMPLE_RATE as usize / frame_size + 1) {
            if self
                .detector
                .process_samples(vec![0.0_f32; frame_size])
                .is_some()
            {
                return true;
            }
        }
        false
    }
}

#[derive(Debug, Serialize)]
pub struct PersonalWakeMetrics {
    pub keyword: &'static str,
    pub enrollment_total: usize,
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

pub fn benchmark(
    detector: &mut PersonalWakeDetector,
    enrollment: &[PathBuf],
    positives: &[PathBuf],
    negatives: &[PathBuf],
) -> Result<PersonalWakeMetrics, String> {
    let started = Instant::now();
    let enrollment_paths = enrollment
        .iter()
        .filter_map(|path| path.canonicalize().ok())
        .collect::<HashSet<_>>();
    let positives = positives
        .iter()
        .filter(|path| {
            path.canonicalize()
                .map(|resolved| !enrollment_paths.contains(&resolved))
                .unwrap_or(true)
        })
        .collect::<Vec<_>>();

    let mut true_positive = 0;
    for path in &positives {
        let clip = read_wav(path)?;
        if detector.detect_clip(&clip.samples) {
            true_positive += 1;
        }
    }

    let mut false_positive = 0;
    let mut negative_audio_seconds = 0.0;
    for path in negatives {
        let clip = read_wav(path)?;
        negative_audio_seconds += clip.samples.len() as f64 / SAMPLE_RATE as f64;
        if detector.detect_clip(&clip.samples) {
            false_positive += 1;
        }
    }

    let positive_total = positives.len();
    let negative_total = negatives.len();
    let negative_audio_hours = negative_audio_seconds / 3600.0;
    Ok(PersonalWakeMetrics {
        keyword: WAKE_KEYWORD,
        enrollment_total: enrollment.len(),
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
        .map_err(|error| format!("cannot read WAV directory {}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .and_then(|value| value.to_str())
                .is_some_and(|value| value.eq_ignore_ascii_case("wav"))
        })
        .collect::<Vec<_>>();
    files.sort();
    Ok(limit.map_or(files.clone(), |limit| even_sample(files, limit)))
}

pub fn even_sample<T>(items: Vec<T>, limit: usize) -> Vec<T> {
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
        .collect::<HashSet<_>>();
    items
        .into_iter()
        .enumerate()
        .filter_map(|(index, item)| selected.contains(&index).then_some(item))
        .collect()
}

struct AudioClip {
    samples: Vec<f32>,
}

fn read_wav(path: &Path) -> Result<AudioClip, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|error| format!("cannot read WAV {}: {error}", path.display()))?;
    let spec = reader.spec();
    if spec.sample_rate != SAMPLE_RATE || spec.channels != CHANNELS {
        return Err(format!(
            "WAV must be 16 kHz mono ({} is {} Hz, {} channels)",
            path.display(),
            spec.sample_rate,
            spec.channels
        ));
    }
    let samples = match spec.sample_format {
        hound::SampleFormat::Float => reader
            .samples::<f32>()
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("invalid float WAV {}: {error}", path.display()))?,
        hound::SampleFormat::Int => {
            let scale = ((1_i64 << (spec.bits_per_sample - 1)) - 1) as f32;
            reader
                .samples::<i32>()
                .map(|sample| sample.map(|value| value as f32 / scale))
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| format!("invalid PCM WAV {}: {error}", path.display()))?
        }
    };
    Ok(AudioClip { samples })
}

fn path_string(path: &Path) -> Result<String, String> {
    path.to_str()
        .map(str::to_owned)
        .ok_or_else(|| format!("path is not valid UTF-8: {}", path.display()))
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
    fn lay_mau_corpus_rai_deu() {
        assert_eq!(even_sample((0..10).collect(), 3), [0, 4, 9]);
        assert_eq!(even_sample((0..3).collect(), 10), [0, 1, 2]);
        assert!(even_sample((0..3).collect(), 0).is_empty());
    }

    #[test]
    fn enrollment_bat_buoc_tu_ba_den_tam_mau() {
        let error = PersonalWakeDetector::enroll(&[], &PersonalWakeConfig::default())
            .err()
            .expect("empty enrollment must fail");
        assert!(error.contains("3 to 8"), "{error}");
    }

    #[test]
    fn cau_goi_duy_nhat_la_hey_liva() {
        assert_eq!(WAKE_KEYWORD, "HEY LIVA");
        assert_eq!(PersonalWakeConfig::default().min_scores, 5);
    }
}
