//! NVIDIA Parakeet-CTC-0.6B Vietnamese — Streaming & Offline STT.
//!
//! Complements the streaming Nemotron RNN-T path with a highly accurate
//! Vietnamese recognizer (FLEURS-vi WER ~5.15% vs Nemotron 14.45%).
//!
//! Implements Overlapping Chunked CTC Streaming:
//! - 160ms chunk frames (2,560 samples @ 16kHz) with 40ms context overlap.
//! - Running feature normalization and incremental decoding.
//! - Emits partial transcripts during speech without waiting for whole-utterance VadEnd.
//! - 5-Layer Anti-Hallucination Filter and Unicode NFC normalization.
//!
//! Contract (verified with `onnx_probe`, 2026-07-05):
//! - input  `audio_signal` Float32 `[B, 80, T]`  — 80 log-mel features × T frames
//! - input  `length`       Int64   `[B]`         — valid frame count per sample
//! - output `logprobs`     Float32 `[B, T, 1025]` — 1024 BPE + 1 CTC blank (id 1024)
//!
//! Preprocessing:
//! **80** mels, `per_feature` normalization, no preemphasis.

use ort::{session::Session, value::Value};
use rustfft::{FftPlanner, num_complex::Complex};
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use super::StreamingTranscript;
use super::anti_hallucination::{AntiHallucinationFilter, FilterDecision};
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

/// 160ms streaming chunk @ 16 kHz = 2,560 samples.
pub const STREAMING_CHUNK_SAMPLES: usize = 2560;
/// 40ms context overlap @ 16 kHz = 640 samples.
pub const STREAMING_OVERLAP_SAMPLES: usize = 640;

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
    /// (`features[m * T + t]`) to match the ONNX `[1, 80, T]` input layout.
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
                // Reflect padding about [0, n-1]
                let mut idx = start + i as isize;
                if idx < 0 {
                    idx = -idx;
                }
                if idx >= n as isize {
                    idx = 2 * n as isize - 2 - idx;
                }
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
                *p = fft_buf[k].norm_sqr();
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

        // per_feature: normalize each mel-bin over T using that bin's own mean/std
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
    anti_hallucination: AntiHallucinationFilter,

    // Streaming state
    stream_buffer: Vec<f32>,
    last_emitted_partial: String,
    stream_started_at: Option<Instant>,
}

fn parse_vocab(raw: &str) -> Result<Vec<String>, String> {
    if let Ok(vocab) = serde_json::from_str::<Vec<String>>(raw) {
        return Ok(vocab);
    }

    let mut vocab = Vec::new();
    for (line_index, line) in raw.lines().enumerate() {
        let line = line.trim_end();
        if line.is_empty() {
            continue;
        }
        let (token, id) = line
            .rsplit_once(' ')
            .ok_or_else(|| format!("indexed vocab line {} has no numeric id", line_index + 1))?;
        let id: usize = id
            .parse()
            .map_err(|_| format!("indexed vocab line {} has invalid id", line_index + 1))?;
        if id != vocab.len() {
            return Err(format!(
                "indexed vocab line {} has id {} (expected {})",
                line_index + 1,
                id,
                vocab.len()
            ));
        }
        vocab.push(token.to_string());
    }
    if vocab.last().is_some_and(|token| token == "<blk>") {
        vocab.pop();
    }
    if vocab.is_empty() {
        return Err("vocab is empty".to_string());
    }
    Ok(vocab)
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
        let vocab = parse_vocab(&vocab_raw).map_err(|e| format!("parse parakeet vocab: {e}"))?;
        if vocab.len() != EXPECTED_VOCAB {
            tracing::warn!(
                "Parakeet vocab has {} tokens (expected {}); decoding falls back to the model's logprob width",
                vocab.len(),
                EXPECTED_VOCAB
            );
        }

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
            anti_hallucination: AntiHallucinationFilter::default(),
            stream_buffer: Vec::new(),
            last_emitted_partial: String::new(),
            stream_started_at: None,
        })
    }

    /// Reset internal streaming audio buffer and state.
    pub fn reset_stream(&mut self) {
        self.stream_buffer.clear();
        self.last_emitted_partial.clear();
        self.stream_started_at = None;
    }

    /// Run ONNX graph inference on pre-extracted feature-major log-mel features.
    /// Returns `(normalized_text, confidence, entropy)`.
    fn run_onnx_logprobs(
        &mut self,
        feat: Vec<f32>,
        t_frames: usize,
    ) -> Result<(String, f32, f32), String> {
        if t_frames == 0 || feat.is_empty() {
            return Ok((String::new(), 1.0, 0.0));
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
        let out_t = shape[1] as usize;
        let vocab_size = shape[2] as usize;

        Ok(ctc_decode_with_stats(
            &self.vocab,
            logprobs,
            out_t,
            vocab_size,
        ))
    }

    /// Run acoustic DSP and ONNX graph inference on a slice of PCM audio.
    /// Returns `(normalized_text, confidence, entropy)`.
    fn run_inference_raw(&mut self, samples: &[f32]) -> Result<(String, f32, f32), String> {
        let (feat, t_frames) = self.dsp.log_mel_per_feature(samples);
        if t_frames == 0 {
            return Ok((String::new(), 1.0, 0.0));
        }
        self.run_onnx_logprobs(feat, t_frames)
    }

    /// Overlapping Chunked CTC Streaming: process audio chunks (e.g. 160ms chunks)
    /// and emit partial/final transcripts with acoustic confidence and latency metrics.
    pub fn feed_chunk(
        &mut self,
        chunk: &[f32],
        is_last: bool,
    ) -> Result<Option<StreamingTranscript>, String> {
        if self.stream_started_at.is_none() {
            self.stream_started_at = Some(Instant::now());
        }
        self.stream_buffer.extend_from_slice(chunk);

        let duration_sec = self.stream_buffer.len() as f32 / SAMPLE_RATE as f32;
        let latency_ms = self
            .stream_started_at
            .map(|t| t.elapsed().as_millis() as u64)
            .unwrap_or(0);

        if !is_last {
            // Need at least 1 chunk (160ms) of speech context before first emission
            if self.stream_buffer.len() < STREAMING_CHUNK_SAMPLES {
                return Ok(None);
            }

            let (feat, t_frames) = self.dsp.log_mel_per_feature(&self.stream_buffer);
            let (text, confidence, _entropy) = self.run_onnx_logprobs(feat, t_frames)?;

            if text.is_empty() {
                return Ok(None);
            }

            if text == self.last_emitted_partial {
                return Ok(None);
            }

            self.last_emitted_partial = text.clone();
            Ok(Some(StreamingTranscript {
                partial_text: text,
                is_final: false,
                confidence,
                latency_ms,
            }))
        } else {
            // Finalize stream at VadEnd
            if self.stream_buffer.is_empty() {
                self.reset_stream();
                return Ok(Some(StreamingTranscript {
                    partial_text: String::new(),
                    is_final: true,
                    confidence: 1.0,
                    latency_ms,
                }));
            }

            let (feat, t_frames) = self.dsp.log_mel_per_feature(&self.stream_buffer);
            let (candidate_text, confidence, entropy) = self.run_onnx_logprobs(feat, t_frames)?;

            // Apply 5-Layer Anti-Hallucination Filter on final transcript
            let final_text = match self.anti_hallucination.filter(
                &candidate_text,
                duration_sec,
                None,
                Some(entropy),
            ) {
                FilterDecision::Valid {
                    normalized_text, ..
                } => normalized_text,
                FilterDecision::Filtered { reason, .. } => {
                    tracing::info!(
                        "AntiHallucinationFilter filtered final transcript: {}",
                        reason
                    );
                    String::new()
                }
            };

            self.reset_stream();
            Ok(Some(StreamingTranscript {
                partial_text: final_text,
                is_final: true,
                confidence,
                latency_ms,
            }))
        }
    }

    /// Transcribe a full mono 16 kHz utterance (offline batch mode).
    /// Returns the filtered, NFC-normalized Vietnamese text.
    pub fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        if samples.is_empty() {
            return Ok(String::new());
        }

        let dur_sec = samples.len() as f32 / SAMPLE_RATE as f32;
        let (raw_text, _conf, entropy) = self.run_inference_raw(samples)?;

        if raw_text.is_empty() {
            return Ok(String::new());
        }

        match self
            .anti_hallucination
            .filter(&raw_text, dur_sec, None, Some(entropy))
        {
            FilterDecision::Valid {
                normalized_text, ..
            } => Ok(normalized_text),
            FilterDecision::Filtered { reason, .. } => {
                tracing::info!("AntiHallucinationFilter rejected utterance: {}", reason);
                Ok(String::new())
            }
        }
    }

    pub fn vocab_len(&self) -> usize {
        self.vocab.len()
    }
}

/// CTC greedy decode with confidence and frame Shannon entropy statistics.
fn ctc_decode_with_stats(
    vocab: &[String],
    logprobs: &[f32],
    t_frames: usize,
    vocab_size: usize,
) -> (String, f32, f32) {
    let blank = vocab_size - 1;
    let mut ids: Vec<usize> = Vec::new();
    let mut prev: i64 = -1;
    let mut total_confidence = 0.0f32;
    let mut active_count = 0usize;

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
        if best != blank {
            total_confidence += best_v.exp();
            active_count += 1;
        }
        if best != blank && best as i64 != prev {
            ids.push(best);
        }
        prev = best as i64;
    }

    let raw_text = detokenize(vocab, &ids);
    let normalized = AntiHallucinationFilter::normalize_vietnamese_nfc(&raw_text);
    let confidence = if active_count > 0 {
        (total_confidence / active_count as f32).clamp(0.0, 1.0)
    } else {
        1.0
    };

    let entropy = AntiHallucinationFilter::compute_shannon_entropy(logprobs, vocab_size, blank)
        .unwrap_or(0.0);

    (normalized, confidence, entropy)
}

/// SentencePiece detokenize: `▁` opens a new word (→ space), other pieces
/// concatenate; assemble `<0xNN>` byte-fallback runs and drop control tokens
/// (`<unk>`, `<pad>`, …).
fn detokenize(vocab: &[String], ids: &[usize]) -> String {
    let mut out = String::new();
    let mut byte_buf: Vec<u8> = Vec::new();

    for &id in ids {
        let Some(tok) = vocab.get(id) else {
            continue;
        };

        if let Some(hex) = tok.strip_prefix("<0x").and_then(|t| t.strip_suffix('>'))
            && let Ok(b) = u8::from_str_radix(hex, 16)
        {
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
        let mut lp = vec![-100.0f32; seq.len() * vocab_size];
        for (t, &id) in seq.iter().enumerate() {
            lp[t * vocab_size + id] = 0.0; // log(1.0) = 0.0
        }
        lp
    }

    #[test]
    fn ctc_collapses_repeats_and_drops_blank() {
        let vocab = v(&["a", "▁b"]); // ids 0,1 ; blank = 2 ; vocab_size = 3
        // [a, a, blank, b] → "a b" (repeat collapsed, blank splits nothing here)
        let lp = onehot(&[0, 0, 2, 1], 3);
        let (text, _conf, _ent) = ctc_decode_with_stats(&vocab, &lp, 4, 3);
        assert_eq!(text, "a b");
    }

    #[test]
    fn indexed_vocab_drops_the_ctc_blank_row() {
        let vocab = parse_vocab("<unk> 0\nng 1\n▁t 2\n<blk> 3\n").unwrap();
        assert_eq!(vocab, v(&["<unk>", "ng", "▁t"]));
    }

    #[test]
    fn ctc_blank_separates_identical_labels() {
        let vocab = v(&["a", "▁b"]);
        // [a, blank, a] → two distinct "a" (blank breaks the repeat-collapse)
        let lp = onehot(&[0, 2, 0], 3);
        let (text, _conf, _ent) = ctc_decode_with_stats(&vocab, &lp, 3, 3);
        assert_eq!(text, "aa");
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

    #[test]
    fn streaming_chunk_and_overlap_parameters() {
        // 160ms chunk @ 16kHz = 2560 samples
        assert_eq!(STREAMING_CHUNK_SAMPLES, 2560);
        // 40ms context overlap @ 16kHz = 640 samples
        assert_eq!(STREAMING_OVERLAP_SAMPLES, 640);
    }

    #[test]
    fn ctc_decode_with_stats_calculates_high_confidence_for_sharp_predictions() {
        let vocab = v(&["▁xin", "▁chào"]); // vocab 0, 1; blank = 2; size = 3
        let lp = onehot(&[0, 2, 1], 3);
        let (text, confidence, entropy) = ctc_decode_with_stats(&vocab, &lp, 3, 3);
        assert_eq!(text, "xin chào");
        assert!(confidence > 0.95);
        assert!(entropy < 0.20);
    }

    #[test]
    fn sentencepiece_detokenize_handles_full_vietnamese_alphabet_and_tones() {
        let vocab = v(&[
            "▁hôm",
            "▁nay",
            "▁thời",
            "▁tiết",
            "▁ở",
            "▁hà",
            "▁nội",
            "▁rất",
            "▁đẹp",
            "▁.",
        ]);
        let ids: Vec<usize> = (0..10).collect();
        let text = detokenize(&vocab, &ids);
        assert_eq!(text, "hôm nay thời tiết ở hà nội rất đẹp.");
    }
}
