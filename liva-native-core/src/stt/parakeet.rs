#![allow(dead_code)]
//! NVIDIA Parakeet-CTC-0.6B Vietnamese — offline (whole-utterance) STT.
//!
//! Complements the streaming Nemotron RNN-T path with a much more accurate
//! Vietnamese recognizer (FLEURS-vi WER 5.15 vs Nemotron 14.45). It is a
//! FastConformer **CTC** model, so it decodes a full utterance at once — it is
//! wired only into the "transcribe after VAD-end" path in [`super::SttManager`]
//! and never into the streaming-partial path.
//!
//! Contract (verified with `onnx_probe`, 2026-07-05):
//! - input  `audio_signal` Float32 `[B, 80, T]`  — 80 log-mel features × T frames
//! - input  `length`       Int64   `[B]`         — valid frame count per sample
//! - output `logprobs`     Float32 `[B, T, 1025]` — 1024 BPE + 1 CTC blank (id 1024)
//!
//! Preprocessing differs from Nemotron (see `docs/99-luu-tru/ke-hoach-da-hoan-thanh/parakeet_vi_integration_plan.md`):
//! **80** mels (not 128), `per_feature` normalization, and **no preemphasis**.
//! The Slaney/librosa mel scale matches [`super::dsp::compute_mel_filterbank`]
//! exactly, so we reuse it with `num_mels = 80`.

use ort::{session::Session, value::Value};
use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;
use std::sync::Arc;

use super::dsp::compute_mel_filterbank;

const N_MELS: usize = 80;
const FFT_SIZE: usize = 512;
const WIN_LENGTH: usize = 400;
const HOP_LENGTH: usize = 160;
const SAMPLE_RATE: f64 = 16000.0;
/// NeMo `log_zero_guard_value = 2^-24` (`log_zero_guard_type = "add"`).
const LOG_GUARD: f32 = 5.960_464_5e-8;
/// NeMo `normalize_batch` per-feature epsilon.
const NORM_EPS: f32 = 1e-5;
/// Expected BPE vocabulary size (blank id = this value = last logprob index).
const EXPECTED_VOCAB: usize = 1024;

/// 80-mel `per_feature` front-end for Parakeet — independent of the Nemotron
/// `SttDsp` because that one is hard-wired to 65 frames / 10 640 samples and
/// emits time-major, un-normalized features.
pub struct ParakeetDsp {
    hann: Vec<f32>,
    mel_fb: Vec<Vec<f32>>, // 80 × 257
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl ParakeetDsp {
    pub fn new() -> Self {
        // Periodic Hann (torch.stft default) — denominator = win_length.
        let mut hann = vec![0.0f32; WIN_LENGTH];
        for (i, w) in hann.iter_mut().enumerate() {
            *w = 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / WIN_LENGTH as f32).cos());
        }
        let mel_fb = compute_mel_filterbank(FFT_SIZE, N_MELS, SAMPLE_RATE);
        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);
        Self { hann, mel_fb, fft }
    }

    /// Compute log-mel features with NeMo `per_feature` normalization.
    ///
    /// Returns `(features, T)` where `features` is **feature-major**
    /// (`features[m * T + t]`) to match the ONNX `[1, 80, T]` input layout —
    /// note this is transposed relative to the Nemotron `[1, T, 128]` layout.
    ///
    /// Framing reproduces `torch.stft(center=True)`: reflect-pad the signal by
    /// `n_fft/2`, so `T = 1 + n_samples / hop`, and each frame is centered at
    /// `t * hop`.
    pub fn log_mel_per_feature(&self, samples: &[f32]) -> (Vec<f32>, usize) {
        let n = samples.len();
        if n == 0 {
            return (Vec::new(), 0);
        }
        let t_frames = 1 + n / HOP_LENGTH;
        let num_bins = FFT_SIZE / 2 + 1; // 257
        let offset = (FFT_SIZE - WIN_LENGTH) / 2; // 56 — center the 400-window in the 512 FFT

        let mut feat = vec![0.0f32; N_MELS * t_frames];
        let mut windowed = vec![0.0f32; WIN_LENGTH];
        let mut fft_buf = vec![Complex::new(0.0f32, 0.0); FFT_SIZE];
        let mut power = vec![0.0f32; num_bins];

        for t in 0..t_frames {
            let center = (t * HOP_LENGTH) as isize;
            let start = center - (WIN_LENGTH as isize) / 2;
            for (i, (w, &h)) in windowed
                .iter_mut()
                .zip(&self.hann)
                .enumerate()
                .take(WIN_LENGTH)
            {
                // Reflect padding about [0, n-1] (numpy/torch "reflect": edge not repeated).
                let mut idx = start + i as isize;
                if idx < 0 {
                    idx = -idx;
                }
                if idx >= n as isize {
                    idx = 2 * n as isize - 2 - idx;
                }
                // Clamp for pathologically short signals that need >1 bounce.
                idx = idx.clamp(0, n as isize - 1);
                *w = samples[idx as usize] * h;
            }

            for c in fft_buf.iter_mut() {
                *c = Complex::new(0.0, 0.0);
            }
            for i in 0..WIN_LENGTH {
                fft_buf[offset + i] = Complex::new(windowed[i], 0.0);
            }
            self.fft.process(&mut fft_buf);

            for (k, p) in power.iter_mut().enumerate() {
                *p = fft_buf[k].norm_sqr(); // power = |FFT|^2 (mag_power = 2.0)
            }

            for m in 0..N_MELS {
                let filter = &self.mel_fb[m];
                let mut e = 0.0f32;
                for k in 0..num_bins {
                    e += filter[k] * power[k];
                }
                feat[m * t_frames + t] = (e + LOG_GUARD).ln();
            }
        }

        // per_feature: normalize each mel-bin over T using that bin's own
        // mean/std (unbiased, matching torch `.std()`), then `+ 1e-5`.
        for m in 0..N_MELS {
            let row = &mut feat[m * t_frames..(m + 1) * t_frames];
            let mean = row.iter().sum::<f32>() / t_frames as f32;
            let std = if t_frames > 1 {
                let ss = row.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>();
                (ss / (t_frames as f32 - 1.0)).sqrt()
            } else {
                0.0
            };
            let denom = std + NORM_EPS;
            for v in row.iter_mut() {
                *v = (*v - mean) / denom;
            }
        }

        (feat, t_frames)
    }
}

impl Default for ParakeetDsp {
    fn default() -> Self {
        Self::new()
    }
}

pub struct ParakeetVi {
    session: Session,
    vocab: Vec<String>,
    dsp: ParakeetDsp,
}

impl ParakeetVi {
    /// Load the ONNX graph (ORT auto-loads the sibling `.onnx.data`) and the
    /// BPE vocab list (`index = token id`).
    pub fn load(model_path: &Path, vocab_path: &Path) -> Result<Self, String> {
        if !model_path.exists() {
            return Err(format!("Parakeet model not found: {:?}", model_path));
        }
        if !vocab_path.exists() {
            return Err(format!("Parakeet vocab not found: {:?}", vocab_path));
        }

        let vocab_raw = std::fs::read_to_string(vocab_path)
            .map_err(|e| format!("read parakeet vocab {:?}: {}", vocab_path, e))?;
        let vocab: Vec<String> = serde_json::from_str(&vocab_raw)
            .map_err(|e| format!("parse parakeet vocab json: {}", e))?;
        if vocab.len() != EXPECTED_VOCAB {
            tracing::warn!(
                "Parakeet vocab has {} tokens (expected {}); decoding falls back to the model's logprob width",
                vocab.len(),
                EXPECTED_VOCAB
            );
        }

        // Intra-op threads default to 4 but are tunable so a machine sharing
        // the CPU with a heavy workload (game, render) can throttle the offline
        // model. Under game mode the process priority is already lowered by the
        // governor, so this is a further, explicit knob. GPU (ORT CUDA EP) is a
        // documented follow-up — it needs the `ort` `cuda` feature, which is
        // not enabled here and risks the ORT backend-init pitfall noted in
        // models/README.md.
        let intra_threads: usize = std::env::var("LIVA_PARAKEET_THREADS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .filter(|&n| n >= 1)
            .unwrap_or(4);
        let session = Session::builder()
            .map_err(|e| format!("session builder: {}", e))?
            .with_intra_threads(intra_threads)
            .map_err(|e| format!("set intra threads: {}", e))?
            .with_inter_threads(1)
            .map_err(|e| format!("set inter threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| format!("load parakeet model {:?}: {}", model_path, e))?;

        Ok(Self {
            session,
            vocab,
            dsp: ParakeetDsp::new(),
        })
    }

    /// Transcribe a full mono 16 kHz utterance. Returns the decoded text
    /// (possibly empty for silence).
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        let (feat, t_frames) = self.dsp.log_mel_per_feature(samples);
        if t_frames == 0 {
            return Ok(String::new());
        }

        let outputs = self
            .session
            .run(ort::inputs![
                "audio_signal" => Value::from_array((vec![1, N_MELS, t_frames], feat))
                    .map_err(|e| format!("audio_signal tensor: {}", e))?,
                "length" => Value::from_array((vec![1], vec![t_frames as i64]))
                    .map_err(|e| format!("length tensor: {}", e))?,
            ])
            .map_err(|e| format!("Parakeet run failed: {}", e))?;

        let lp_val = outputs
            .get("logprobs")
            .ok_or_else(|| "Missing logprobs from Parakeet".to_string())?;
        let (shape, logprobs) = lp_val
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("extract logprobs: {}", e))?;
        // shape = [1, T, vocab_size]; trust the model's own dims.
        let out_t = shape[1] as usize;
        let vocab_size = shape[2] as usize;

        Ok(ctc_decode(&self.vocab, logprobs, out_t, vocab_size))
    }

    pub fn vocab_len(&self) -> usize {
        self.vocab.len()
    }
}

/// CTC greedy decode: per-frame argmax, collapse consecutive repeats, drop the
/// blank (id `vocab_size - 1`), then detokenize the surviving BPE ids.
fn ctc_decode(vocab: &[String], logprobs: &[f32], t_frames: usize, vocab_size: usize) -> String {
    let blank = vocab_size - 1;
    let mut ids: Vec<usize> = Vec::new();
    let mut prev: i64 = -1;
    for t in 0..t_frames {
        let base = t * vocab_size;
        let row = &logprobs[base..base + vocab_size];
        let mut best = 0usize;
        let mut best_v = row[0];
        for (k, &v) in row.iter().enumerate() {
            if v > best_v {
                best_v = v;
                best = k;
            }
        }
        if best != blank && best as i64 != prev {
            ids.push(best);
        }
        prev = best as i64;
    }
    detokenize(vocab, &ids)
}

/// SentencePiece detokenize: `▁` opens a new word (→ space), other pieces
/// concatenate; assemble `<0xNN>` byte-fallback runs and drop control tokens
/// (`<unk>`, `<pad>`, …). Mirrors [`super::tokenizer::SttTokenizer::decode`].
fn detokenize(vocab: &[String], ids: &[usize]) -> String {
    let mut out = String::new();
    let mut byte_buf: Vec<u8> = Vec::new();

    for &id in ids {
        let Some(tok) = vocab.get(id) else {
            continue;
        };

        if let Some(hex) = tok.strip_prefix("<0x").and_then(|t| t.strip_suffix('>'))
            && let Ok(b) = u8::from_str_radix(hex, 16) {
                byte_buf.push(b);
                continue;
            }
        if !byte_buf.is_empty() {
            out.push_str(&String::from_utf8_lossy(&byte_buf));
            byte_buf.clear();
        }

        if tok.starts_with('<') && tok.ends_with('>') {
            continue;
        }

        if let Some(rest) = tok.strip_prefix('▁') {
            // `▁` marks a word boundary → a space, EXCEPT for standalone
            // sentence punctuation pieces (`▁.`, `▁,`, `▁?` …) where a leading
            // space reads wrong ("Liva ," → "Liva,").
            let leads_with_punct = rest
                .chars()
                .next()
                .is_some_and(|c| matches!(c, '.' | ',' | '?' | '!' | ':' | ';' | '%' | ')'));
            if !out.is_empty() && !leads_with_punct {
                out.push(' ');
            }
            out.push_str(rest);
        } else {
            out.push_str(tok);
        }
    }

    if !byte_buf.is_empty() {
        out.push_str(&String::from_utf8_lossy(&byte_buf));
    }

    out.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    /// One-hot logprobs (value 1.0 at the chosen id, 0.0 elsewhere) for a frame
    /// sequence of ids, over `vocab_size` classes.
    fn onehot(seq: &[usize], vocab_size: usize) -> Vec<f32> {
        let mut lp = vec![0.0f32; seq.len() * vocab_size];
        for (t, &id) in seq.iter().enumerate() {
            lp[t * vocab_size + id] = 1.0;
        }
        lp
    }

    #[test]
    fn ctc_collapses_repeats_and_drops_blank() {
        let vocab = v(&["a", "▁b"]); // ids 0,1 ; blank = 2 ; vocab_size = 3
        // [a, a, blank, b] → "a b" (repeat collapsed, blank splits nothing here)
        let lp = onehot(&[0, 0, 2, 1], 3);
        assert_eq!(ctc_decode(&vocab, &lp, 4, 3), "a b");
    }

    #[test]
    fn ctc_blank_separates_identical_labels() {
        let vocab = v(&["a", "▁b"]);
        // [a, blank, a] → two distinct "a" (blank breaks the repeat-collapse)
        let lp = onehot(&[0, 2, 0], 3);
        assert_eq!(ctc_decode(&vocab, &lp, 3, 3), "aa");
    }

    #[test]
    fn detokenize_joins_sentencepiece_pieces() {
        let vocab = v(&["<unk>", "▁xin", "▁ch", "ào"]);
        // "▁xin ▁ch ào" → "xin chào"
        assert_eq!(detokenize(&vocab, &[1, 2, 3]), "xin chào");
        // control token dropped
        assert_eq!(detokenize(&vocab, &[0, 1]), "xin");
    }

    #[test]
    fn detokenize_suppresses_space_before_punctuation() {
        let vocab = v(&["▁liva", "▁,", "▁rất", "▁."]);
        // "▁liva ▁, ▁rất ▁." → "liva, rất." (no space before , or .)
        assert_eq!(detokenize(&vocab, &[0, 1, 2, 3]), "liva, rất.");
    }

    #[test]
    fn per_feature_rows_are_zero_mean() {
        let dsp = ParakeetDsp::new();
        // 0.5 s of a 220 Hz tone → excites several mel bins.
        let n = 8000usize;
        let samples: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * 220.0 * i as f32 / 16000.0).sin() * 0.3)
            .collect();
        let (feat, t) = dsp.log_mel_per_feature(&samples);
        assert_eq!(t, 1 + n / HOP_LENGTH);
        assert_eq!(feat.len(), N_MELS * t);
        for m in 0..N_MELS {
            let row = &feat[m * t..(m + 1) * t];
            let mean = row.iter().sum::<f32>() / t as f32;
            assert!(mean.abs() < 1e-3, "mel {} mean {} not ~0", m, mean);
        }
    }
}
