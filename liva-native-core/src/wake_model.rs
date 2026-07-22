//! Trained wake-word classifier — the real detector `wake.rs`'s module docs
//! call "a dedicated openWakeWord-style detector" that can replace the
//! transcription-based `asr_prefix` heuristic.
//!
//! This re-implements the (Apache-2.0) `livekit-wakeword` Rust crate's
//! inference pipeline in-house against LIVA's own `ort` (rc.9, native
//! onnxruntime backend) rather than depending on that crate directly: on
//! non-aarch64-Windows targets it requires `ort`'s `alternative-backend`
//! feature + a pure-Rust `ort-tract` backend, and Cargo unifies features
//! across the whole dependency graph — pulling it in silently switched
//! *every* other `ort::Session` in this process (VAD/GTCRN/Smart Turn/STT/
//! TTS) onto an uninitialized alternative backend, crashing them all with
//! "attempted to use `ort` APIs before initializing a backend". Confirmed
//! by the actual test failures, not a hypothetical.
//!
//! Two small bundled models (Apache-2.0, from `livekit/rust-sdks`
//! `livekit-wakeword/onnx/`) plus one trained classifier per accent/language
//! (from the Python `livekit-wakeword` training toolkit — see
//! `docs/99-luu-tru/bao-cao-lich-su/LIVA_OSS_Research_2026-07.md`):
//!   1. `models/wakeword_melspec.onnx` — raw f32 PCM [1, num_samples] →
//!      mel spectrogram [1, 1, time_frames, 32]; post-processed `x/10 + 2`
//!      (matches openWakeWord's `melspec_transform`).
//!   2. `models/wakeword_embedding.onnx` — one 76-frame×32-mel window
//!      [1, 76, 32, 1] → a 96-dim embedding [1, 1, 1, 96].
//!   3. The trained classifier — a stack of the last 16 embeddings
//!      [1, 16, 96] → a single confidence score.
//! Exact tensor names/shapes verified against the actual `.onnx` graphs via
//! the `onnx_probe` diagnostic bin, not just the reference source's doc
//! comments.
//!
//! [`TrainedWakeDetector::push_and_check`] buffers incoming 16kHz mono f32
//! audio and re-runs the full mel→embedding→classify pipeline every
//! [`PREDICT_INTERVAL_SAMPLES`] (~200ms) over a rolling ~2.5s window —
//! there's no cross-call state to exploit (mirroring the reference crate,
//! whose `predict()` is itself stateless per call).
use ort::{session::Session, value::Value};
use std::collections::{HashMap, VecDeque};
use std::path::Path;

const SAMPLE_RATE: usize = 16000;
const RING_SECONDS: f32 = 2.5; // just above the model's ~2s minimum
const RING_LEN: usize = (SAMPLE_RATE as f32 * RING_SECONDS) as usize;
const PREDICT_INTERVAL_SAMPLES: usize = SAMPLE_RATE / 5; // ~200ms

const MEL_BINS: usize = 32;
const EMBEDDING_WINDOW: usize = 76; // mel frames per embedding
const EMBEDDING_STRIDE: usize = 8; // mel frames between embeddings
const EMBEDDING_DIM: usize = 96;
const MIN_EMBEDDINGS: usize = 16; // classifier input length

fn resolve_bundled_model(env_var: &str, default_name: &str) -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var(env_var)
        && !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    for candidate in [
        PathBuf::from(format!("models/{}", default_name)),
        PathBuf::from(format!("../models/{}", default_name)),
        // liva-desktop/src-tauri is two levels below the repo root
        PathBuf::from(format!("../../models/{}", default_name)),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from(format!("models/{}", default_name))
}

struct MelspectrogramModel {
    session: Session,
}

impl MelspectrogramModel {
    fn new() -> Result<Self, String> {
        let path = resolve_bundled_model("LIVA_WAKE_MELSPEC_PATH", "wakeword_melspec.onnx");
        let session = Session::builder()
            .map_err(|e| format!("SessionBuilder: {}", e))?
            .commit_from_file(&path)
            .map_err(|e| format!("load melspectrogram model at {:?}: {}", path, e))?;
        Ok(Self { session })
    }

    /// samples: f32 PCM in [-1.0, 1.0]. Returns mel features flattened
    /// row-major as (time_frames, MEL_BINS), plus the frame count.
    fn detect(&mut self, samples: &[f32]) -> Result<(Vec<f32>, usize), String> {
        let inputs = ort::inputs![
            "input" => Value::from_array((vec![1usize, samples.len()], samples.to_vec())).map_err(|e| e.to_string())?,
        ];
        let outputs = self.session.run(inputs).map_err(|e| format!("melspec run: {}", e))?;
        let (shape, data) = outputs
            .get("output")
            .ok_or("melspec: missing 'output'")?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let time_frames = shape[2] as usize;
        let mel_bins = shape[3] as usize;
        if mel_bins != MEL_BINS {
            return Err(format!("melspec: expected {} mel bins, model gave {}", MEL_BINS, mel_bins));
        }
        // Post-process to match openWakeWord's melspec_transform: x/10 + 2.
        let mel: Vec<f32> = data.iter().map(|&x| x / 10.0 + 2.0).collect();
        Ok((mel, time_frames))
    }
}

struct EmbeddingModel {
    session: Session,
}

impl EmbeddingModel {
    fn new() -> Result<Self, String> {
        let path = resolve_bundled_model("LIVA_WAKE_EMBEDDING_PATH", "wakeword_embedding.onnx");
        let session = Session::builder()
            .map_err(|e| format!("SessionBuilder: {}", e))?
            .commit_from_file(&path)
            .map_err(|e| format!("load embedding model at {:?}: {}", path, e))?;
        Ok(Self { session })
    }

    /// mel_window: exactly EMBEDDING_WINDOW*MEL_BINS (76*32) f32 values,
    /// row-major (time, mel). Returns a 96-dim embedding.
    fn detect(&mut self, mel_window: &[f32]) -> Result<Vec<f32>, String> {
        let inputs = ort::inputs![
            "input_1" => Value::from_array((vec![1usize, EMBEDDING_WINDOW, MEL_BINS, 1], mel_window.to_vec())).map_err(|e| e.to_string())?,
        ];
        let outputs = self.session.run(inputs).map_err(|e| format!("embedding run: {}", e))?;
        let (_, data) = outputs
            .get("conv2d_19")
            .ok_or("embedding: missing 'conv2d_19'")?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        if data.len() != EMBEDDING_DIM {
            return Err(format!("embedding: expected {} dims, got {}", EMBEDDING_DIM, data.len()));
        }
        Ok(data.to_vec())
    }
}

pub struct TrainedWakeDetector {
    mel_model: MelspectrogramModel,
    emb_model: EmbeddingModel,
    classifiers: HashMap<String, Session>,
    ring: VecDeque<f32>,
    since_last_predict: usize,
    threshold: f32,
}

impl TrainedWakeDetector {
    /// Load one classifier per path (e.g. one per language/accent — a hit on
    /// *any* of them counts as a wake). `threshold` is the per-classifier
    /// confidence cutoff (the livekit-wakeword project's own benchmark found
    /// ~0.68 optimal for its conv-attention head; tune per exported model
    /// via the training toolkit's `eval` step). Expects 16kHz mono input —
    /// LIVA's mic pipeline is already 16kHz, so no resampling is needed here.
    pub fn new(model_paths: &[impl AsRef<Path>], threshold: f32) -> Result<Self, String> {
        let mut classifiers = HashMap::new();
        for path in model_paths {
            let path = path.as_ref();
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("wakeword")
                .to_string();
            let session = Session::builder()
                .map_err(|e| format!("SessionBuilder: {}", e))?
                .commit_from_file(path)
                .map_err(|e| format!("load classifier {:?}: {}", path, e))?;
            classifiers.insert(name, session);
        }
        Ok(Self {
            mel_model: MelspectrogramModel::new()?,
            emb_model: EmbeddingModel::new()?,
            classifiers,
            ring: VecDeque::with_capacity(RING_LEN),
            since_last_predict: 0,
            threshold,
        })
    }

    /// Push new 16kHz mono f32 samples ([-1.0, 1.0]). Returns
    /// `Some((name, score))` the moment any loaded classifier crosses
    /// `threshold` on a periodic check (not necessarily every call — see
    /// module docs).
    pub fn push_and_check(&mut self, samples: &[f32]) -> Option<(String, f32)> {
        for &s in samples {
            self.ring.push_back(s.clamp(-1.0, 1.0));
            if self.ring.len() > RING_LEN {
                self.ring.pop_front();
            }
        }
        self.since_last_predict += samples.len();
        if self.since_last_predict < PREDICT_INTERVAL_SAMPLES || self.classifiers.is_empty() {
            return None;
        }
        self.since_last_predict = 0;

        match self.predict() {
            Ok(scores) => scores
                .into_iter()
                .filter(|(_, score)| *score > self.threshold)
                .max_by(|a, b| a.1.total_cmp(&b.1)),
            Err(e) => {
                tracing::error!("Wake-word predict failed: {}", e);
                None
            }
        }
    }

    fn predict(&mut self) -> Result<HashMap<String, f32>, String> {
        let audio: Vec<f32> = self.ring.iter().copied().collect();
        self.predict_raw(&audio)
    }

    /// Run the full mel→embedding→classify pipeline once on an arbitrary
    /// audio clip, bypassing the ring buffer/throttle — for one-shot
    /// evaluation (tests, the `wakeword_probe` diagnostic bin) rather than
    /// live streaming.
    pub fn predict_raw(&mut self, audio: &[f32]) -> Result<HashMap<String, f32>, String> {
        let (mel, num_frames) = self.mel_model.detect(audio)?;
        if num_frames < EMBEDDING_WINDOW {
            return Ok(HashMap::new());
        }

        let mut embeddings: Vec<Vec<f32>> = Vec::new();
        let mut start = 0;
        while start + EMBEDDING_WINDOW <= num_frames {
            let window = &mel[start * MEL_BINS..(start + EMBEDDING_WINDOW) * MEL_BINS];
            embeddings.push(self.emb_model.detect(window)?);
            start += EMBEDDING_STRIDE;
        }
        if embeddings.len() < MIN_EMBEDDINGS {
            return Ok(HashMap::new());
        }

        let last = &embeddings[embeddings.len() - MIN_EMBEDDINGS..];
        let mut flat = Vec::with_capacity(MIN_EMBEDDINGS * EMBEDDING_DIM);
        for e in last {
            flat.extend_from_slice(e);
        }

        let mut predictions = HashMap::new();
        for (name, session) in &mut self.classifiers {
            let inputs = ort::inputs![
                "embeddings" => Value::from_array((vec![1usize, MIN_EMBEDDINGS, EMBEDDING_DIM], flat.clone())).map_err(|e| e.to_string())?,
            ];
            let outputs = session.run(inputs).map_err(|e| format!("classifier '{}' run: {}", name, e))?;
            let (_, data) = outputs
                .get("score")
                .ok_or_else(|| format!("classifier '{}': missing 'score' output", name))?
                .try_extract_tensor::<f32>()
                .map_err(|e| e.to_string())?;
            predictions.insert(name.clone(), data.first().copied().unwrap_or(0.0));
        }
        Ok(predictions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixtures_dir() -> Option<std::path::PathBuf> {
        for candidate in ["models/wake_fixtures", "../models/wake_fixtures"] {
            let p = std::path::PathBuf::from(candidate);
            if p.exists() {
                return Some(p);
            }
        }
        None
    }

    fn read_wav_pcm16_mono(path: &std::path::Path) -> Vec<f32> {
        let bytes = std::fs::read(path).expect("read wav");
        let mut pos = 12;
        let mut data = Vec::new();
        while pos + 8 <= bytes.len() {
            let chunk_id = &bytes[pos..pos + 4];
            let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
            let body_start = pos + 8;
            if chunk_id == b"data" {
                data = bytes[body_start..body_start + chunk_size]
                    .chunks_exact(2)
                    .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                    .collect();
            }
            pos = body_start + chunk_size + (chunk_size % 2);
        }
        data
    }

    /// End-to-end validation against livekit-wakeword's own reference
    /// fixtures (Apache-2.0, `livekit/rust-sdks` `livekit-wakeword/tests/
    /// fixtures/`): if this from-scratch mel→embedding→classify port is
    /// correct, it must reproduce the reference crate's own pass/fail split
    /// on the SAME classifier + WAV clips (reference asserts score>=0.5 for
    /// positive.wav, score<0.5 for negative.wav).
    #[test]
    fn matches_reference_positive_and_negative_fixtures() {
        let Some(dir) = fixtures_dir() else {
            eprintln!("skip: models/wake_fixtures not present");
            return;
        };
        let classifier = dir.join("hey_livekit.onnx");
        let mut detector =
            TrainedWakeDetector::new(&[&classifier], 0.5).expect("load hey_livekit classifier");

        let positive = read_wav_pcm16_mono(&dir.join("positive.wav"));
        let pos_scores = detector.predict_raw(&positive).expect("predict positive.wav");
        let pos_score = pos_scores["hey_livekit"];
        assert!(pos_score >= 0.5, "positive.wav score {} should be >= 0.5", pos_score);

        let negative = read_wav_pcm16_mono(&dir.join("negative.wav"));
        let neg_scores = detector.predict_raw(&negative).expect("predict negative.wav");
        let neg_score = neg_scores["hey_livekit"];
        assert!(neg_score < 0.5, "negative.wav score {} should be < 0.5", neg_score);
    }

    #[test]
    fn short_audio_returns_no_scores() {
        let Some(dir) = fixtures_dir() else {
            eprintln!("skip: models/wake_fixtures not present");
            return;
        };
        let classifier = dir.join("hey_livekit.onnx");
        let mut detector =
            TrainedWakeDetector::new(&[&classifier], 0.5).expect("load hey_livekit classifier");

        let short = vec![0.0f32; 1600]; // 0.1s, far under the ~2s minimum
        let scores = detector.predict_raw(&short).expect("predict short clip");
        assert!(scores.is_empty(), "sub-minimum-duration audio should yield no scores");
    }
}
