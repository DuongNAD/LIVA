//! Smart Turn v3.2 (pipecat-ai/smart-turn, BSD-2-Clause) semantic end-of-turn
//! classifier — **SHADOW MODE ONLY**. Runs alongside the existing
//! frame-count VAD end-of-turn logic purely to log its verdict for
//! comparison; nothing reads its output yet. Vietnamese is this model's
//! weakest supported language (81.27% vs 94.31% English per the v3.0 launch
//! benchmark), so real production logs are needed before trusting it enough
//! to gate anything.
//!
//! Model: `models/smart_turn_v3.2_cpu.onnx`. There are no feature-extraction
//! ops in the ONNX graph itself (verified by inspecting the graph directly)
//! — the caller must supply an 80x800 Whisper-style log-mel spectrogram.
//! Feature recipe ported from the reference implementation
//! (`pipecat_ai/pipecat` `local_smart_turn_v3.py` +
//! `_whisper_features.py`, both BSD-2-Clause):
//!   1. Trailing-anchor to exactly 8s/128000 samples @16kHz — keep the tail
//!      if longer, zero-pad at the *start* if shorter.
//!   2. Zero-mean/unit-variance-normalize the *entire* padded buffer
//!      (`(x - mean) / sqrt(var + 1e-7)`), padding included.
//!   3. Center-STFT: reflect-pad by n_fft/2 each side, periodic Hann,
//!      n_fft=400, hop=160 → 801 frames, drop the trailing one → 800.
//!   4. Mel-project (80 mels, Slaney scale — the exact same math as
//!      `stt::dsp::compute_mel_filterbank`, just with n_fft=400/mels=80
//!      instead of the STT encoder's params) → power → log10 (floor 1e-10)
//!      → floor at (max-8) → (x+4)/4.
//!
//! Input tensor `input_features` [1, 80, 800] f32; output `logits` [1, 1] is
//! already sigmoid-applied (despite the name) — >0.5 means turn complete.
use crate::stt::dsp::compute_mel_filterbank;
use ort::{session::Session, value::Value};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::path::Path;
use std::sync::Arc;

const SAMPLE_RATE: usize = 16000;
const N_SAMPLES: usize = SAMPLE_RATE * 8; // 128_000, the model's fixed window
const N_FFT: usize = 400;
const HOP: usize = 160;
const N_MELS: usize = 80;
const N_FRAMES: usize = N_SAMPLES / HOP; // 800, after dropping the trailing frame

/// Resolve the Smart Turn model path: env override, else
/// `models/smart_turn_v3.2_cpu.onnx` (with `../` fallback).
pub fn resolve_model_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("LIVA_TURN_MODEL_PATH")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p);
    }
    for candidate in [
        PathBuf::from("models/smart_turn_v3.2_cpu.onnx"),
        PathBuf::from("../models/smart_turn_v3.2_cpu.onnx"),
        // liva-desktop/src-tauri is two levels below the repo root
        PathBuf::from("../../models/smart_turn_v3.2_cpu.onnx"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("models/smart_turn_v3.2_cpu.onnx")
}

pub struct SmartTurnClassifier {
    session: Session,
    fft: Arc<dyn Fft<f32>>,
    window: Vec<f32>,           // periodic Hann, len N_FFT
    mel_filters: Vec<Vec<f32>>, // N_MELS x (N_FFT/2+1)
}

/// Shadow-mode verdict: logged for comparison, never acted on.
#[derive(Debug, Clone, Copy)]
pub struct TurnVerdict {
    pub probability: f32,
    pub complete: bool,
}

impl SmartTurnClassifier {
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("Failed to create SessionBuilder: {}", e))?
            .with_intra_threads(1)
            .map_err(|e| format!("Failed to configure intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("Failed to configure inter threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| format!("Failed to load Smart Turn ONNX model: {}", e))?;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(N_FFT);

        // Periodic Hann == np.hanning(N_FFT+1)[:-1], matching the reference.
        let window: Vec<f32> = (0..N_FFT)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / N_FFT as f32).cos())
            .collect();

        let mel_filters = compute_mel_filterbank(N_FFT, N_MELS, SAMPLE_RATE as f64);

        Ok(Self {
            session,
            fft,
            window,
            mel_filters,
        })
    }

    /// Classify up to the last 8s of 16kHz mono audio. Shorter clips are
    /// zero-padded at the start; longer clips keep only their trailing 8s.
    pub fn predict(&mut self, samples: &[f32]) -> Result<TurnVerdict, String> {
        let features = self.log_mel_features(samples);

        let inputs = ort::inputs![
            "input_features" => Value::from_array((vec![1usize, N_MELS, N_FRAMES], features)).map_err(|e| e.to_string())?,
        ];
        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| format!("Smart Turn ONNX run failed: {}", e))?;

        let (_, logits) = outputs
            .get("logits")
            .ok_or_else(|| "Missing 'logits' output".to_string())?
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract 'logits': {}", e))?;

        let probability = *logits.first().ok_or("Empty 'logits' output")?;
        Ok(TurnVerdict {
            probability,
            complete: probability > 0.5,
        })
    }

    fn log_mel_features(&self, samples: &[f32]) -> Vec<f32> {
        // 1. Trailing-anchor to exactly N_SAMPLES.
        let mut windowed = vec![0.0f32; N_SAMPLES];
        if samples.len() >= N_SAMPLES {
            let start = samples.len() - N_SAMPLES;
            windowed.copy_from_slice(&samples[start..]);
        } else {
            let lead_zeros = N_SAMPLES - samples.len();
            windowed[lead_zeros..].copy_from_slice(samples);
        }

        // 2. Zero-mean/unit-variance over the FULL padded buffer (padding
        //    included), matching transformers' WhisperFeatureExtractor.
        let mean = windowed.iter().sum::<f32>() / N_SAMPLES as f32;
        let var = windowed.iter().map(|&s| (s - mean).powi(2)).sum::<f32>() / N_SAMPLES as f32;
        let inv_std = 1.0 / (var + 1e-7).sqrt();
        for s in windowed.iter_mut() {
            *s = (*s - mean) * inv_std;
        }

        // 3. Center STFT: reflect-pad by N_FFT/2 each side. numpy `reflect`
        //    semantics (edge sample itself excluded): padded[pad-k] =
        //    windowed[k], padded[pad+L-1+k] = windowed[L-1-k], for k in 1..=pad.
        let pad = N_FFT / 2;
        let l = windowed.len();
        let mut padded = vec![0.0f32; l + 2 * pad];
        padded[pad..pad + l].copy_from_slice(&windowed);
        for k in 1..=pad {
            padded[pad - k] = windowed[k];
            padded[pad + l - 1 + k] = windowed[l - 1 - k];
        }

        // Frame at hop=160; computing only N_FRAMES (not N_FRAMES+1) drops
        // the trailing frame exactly like the reference's `[:, :-1]`.
        let mut mel_spec = vec![0.0f32; N_MELS * N_FRAMES];
        let mut power = vec![0.0f32; N_FFT / 2 + 1];
        for frame_idx in 0..N_FRAMES {
            let start = frame_idx * HOP;
            let mut buf: Vec<Complex<f32>> = (0..N_FFT)
                .map(|i| Complex::new(padded[start + i] * self.window[i], 0.0))
                .collect();
            self.fft.process(&mut buf);
            for (k, p) in power.iter_mut().enumerate() {
                *p = buf[k].re * buf[k].re + buf[k].im * buf[k].im;
            }

            // 4. Mel-project this frame into every mel bin's column.
            for (mel_idx, filter) in self.mel_filters.iter().enumerate() {
                let energy: f32 = filter.iter().zip(power.iter()).map(|(&w, &p)| w * p).sum();
                mel_spec[mel_idx * N_FRAMES + frame_idx] = energy.max(1e-10).log10();
            }
        }

        let max_log = mel_spec.iter().cloned().fold(f32::MIN, f32::max);
        let floor = max_log - 8.0;
        for v in mel_spec.iter_mut() {
            *v = (v.max(floor) + 4.0) / 4.0;
        }
        mel_spec
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_path_for_test() -> Option<std::path::PathBuf> {
        let p = resolve_model_path();
        if p.exists() { Some(p) } else { None }
    }

    #[test]
    fn predict_returns_finite_probability_for_short_and_long_clips() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: smart_turn_v3.2_cpu.onnx not present");
            return;
        };
        let mut classifier = SmartTurnClassifier::new(&path).expect("load Smart Turn model");

        // Short clip (< 8s): exercises the start-zero-padding path.
        let short: Vec<f32> = (0..8000)
            .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 150.0 * i as f32 / 16000.0).sin())
            .collect();
        let v1 = classifier.predict(&short).expect("predict short clip");
        assert!(v1.probability.is_finite());
        assert!((0.0..=1.0).contains(&v1.probability));

        // Long clip (> 8s): exercises the trailing-crop path.
        let long: Vec<f32> = (0..200_000)
            .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 150.0 * i as f32 / 16000.0).sin())
            .collect();
        let v2 = classifier.predict(&long).expect("predict long clip");
        assert!(v2.probability.is_finite());
        assert!((0.0..=1.0).contains(&v2.probability));
    }
}
