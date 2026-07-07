//! Streaming speech-enhancement pre-stage using GTCRN (ICASSP 2024), an
//! ultra-lightweight (23.7K params / 33 MMACs) causal denoiser distributed
//! as an ONNX graph by the sherpa-onnx project (MIT/Apache upstream).
//!
//! Model: `models/gtcrn_simple.onnx` — STFT-domain, single-frame streaming
//! contract verified empirically via `onnx_probe` against the upstream
//! export script (github.com/Xiaobin-Rong/gtcrn, `stream/gtcrn_stream.py`):
//! n_fft=512, hop=256, sqrt-Hann analysis+synthesis window, mono 16kHz.
//! Recurrent state (`conv_cache`/`tra_cache`/`inter_cache`) threads across
//! hops exactly like `VadEngine`'s `state`/`stateN`.
use ort::{session::Session, value::Value};
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::path::Path;
use std::sync::Arc;

const WIN: usize = 512;
const HOP: usize = 256;
const FREQ_BINS: usize = WIN / 2 + 1; // 257

const CONV_CACHE_LEN: usize = 2 * 1 * 16 * 16 * 33;
const TRA_CACHE_LEN: usize = 2 * 3 * 1 * 1 * 16;
const INTER_CACHE_LEN: usize = 2 * 1 * 33 * 16;

/// Resolve the GTCRN model path: env override, else `models/gtcrn_simple.onnx`
/// (with `../` fallback for binaries run from `liva-native-core/`).
pub fn resolve_model_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("LIVA_DENOISE_MODEL_PATH") {
        if !p.trim().is_empty() {
            return PathBuf::from(p);
        }
    }
    for candidate in [
        PathBuf::from("models/gtcrn_simple.onnx"),
        PathBuf::from("../models/gtcrn_simple.onnx"),
        // liva-desktop/src-tauri is two levels below the repo root
        PathBuf::from("../../models/gtcrn_simple.onnx"),
    ] {
        if candidate.exists() {
            return candidate;
        }
    }
    PathBuf::from("models/gtcrn_simple.onnx")
}

pub struct GtcrnDenoiser {
    session: Session,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: Vec<f32>, // sqrt-Hann, len WIN

    analysis_hist: Vec<f32>, // last WIN raw input samples (shifts by HOP)
    ola_buf: Vec<f32>,       // overlap-add synthesis accumulator, len WIN
    pending_in: Vec<f32>,    // raw samples not yet consumed into a hop
    primed: bool,            // whether analysis_hist has seen real audio

    conv_cache: Vec<f32>,
    tra_cache: Vec<f32>,
    inter_cache: Vec<f32>,
}

impl GtcrnDenoiser {
    pub fn new<P: AsRef<Path>>(model_path: P) -> Result<Self, String> {
        let session = Session::builder()
            .map_err(|e| format!("Failed to create SessionBuilder: {}", e))?
            .with_intra_threads(1)
            .map_err(|e| format!("Failed to configure intra threads: {}", e))?
            .commit_from_file(model_path)
            .map_err(|e| format!("Failed to load GTCRN ONNX model: {}", e))?;

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(WIN);
        let ifft = planner.plan_fft_inverse(WIN);

        // sqrt-Hann: satisfies COLA at 50% overlap (hop = WIN/2) when applied
        // on both analysis and synthesis sides, matching the reference export
        // script's `hann_window(512).pow(0.5)`.
        let window: Vec<f32> = (0..WIN)
            .map(|i| {
                let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WIN as f32).cos();
                hann.sqrt()
            })
            .collect();

        Ok(Self {
            session,
            fft,
            ifft,
            window,
            analysis_hist: vec![0.0; WIN],
            ola_buf: vec![0.0; WIN],
            pending_in: Vec::with_capacity(HOP * 2),
            primed: false,
            conv_cache: vec![0.0; CONV_CACHE_LEN],
            tra_cache: vec![0.0; TRA_CACHE_LEN],
            inter_cache: vec![0.0; INTER_CACHE_LEN],
        })
    }

    /// Reset all recurrent/streaming state (call on session/utterance boundaries).
    pub fn reset(&mut self) {
        self.analysis_hist.fill(0.0);
        self.ola_buf.fill(0.0);
        self.pending_in.clear();
        self.primed = false;
        self.conv_cache.fill(0.0);
        self.tra_cache.fill(0.0);
        self.inter_cache.fill(0.0);
    }

    /// Push raw 16kHz mono PCM and get back the denoised samples produced so
    /// far (may be shorter than the input while the ~32ms analysis window
    /// primes, then tracks 1:1 in steady state).
    pub fn process_audio(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.pending_in.extend_from_slice(samples);
        let mut out = Vec::new();

        while self.pending_in.len() >= HOP {
            let new_hop: Vec<f32> = self.pending_in.drain(0..HOP).collect();
            self.primed = true;

            // Slide the analysis window: drop oldest HOP, append newest HOP.
            self.analysis_hist.copy_within(HOP..WIN, 0);
            self.analysis_hist[WIN - HOP..].copy_from_slice(&new_hop);

            let enh_bins = self.run_frame()?;

            // ISTFT: rebuild the full conjugate-symmetric spectrum, inverse
            // FFT, re-window (sqrt-Hann synthesis side), overlap-add.
            let mut full = vec![Complex::new(0.0f32, 0.0f32); WIN];
            for k in 0..FREQ_BINS {
                full[k] = enh_bins[k];
            }
            for k in 1..(WIN / 2) {
                full[WIN - k] = enh_bins[k].conj();
            }
            self.ifft.process(&mut full);
            let inv_scale = 1.0 / WIN as f32;

            for i in 0..WIN {
                self.ola_buf[i] += full[i].re * inv_scale * self.window[i];
            }

            out.extend_from_slice(&self.ola_buf[0..HOP]);
            self.ola_buf.copy_within(HOP..WIN, 0);
            self.ola_buf[WIN - HOP..].fill(0.0);
        }

        Ok(out)
    }

    fn run_frame(&mut self) -> Result<Vec<Complex<f32>>, String> {
        // Forward STFT of the current WIN-sample analysis window.
        let mut buf: Vec<Complex<f32>> = self
            .analysis_hist
            .iter()
            .zip(self.window.iter())
            .map(|(&s, &w)| Complex::new(s * w, 0.0))
            .collect();
        self.fft.process(&mut buf);

        // Arrange as ONNX tensor layout (1, 257, 1, 2): [freq][re, im].
        let mut mix_data = Vec::with_capacity(FREQ_BINS * 2);
        for k in 0..FREQ_BINS {
            mix_data.push(buf[k].re);
            mix_data.push(buf[k].im);
        }

        let inputs = ort::inputs![
            "mix" => Value::from_array((vec![1usize, FREQ_BINS, 1, 2], mix_data)).map_err(|e| e.to_string())?,
            "conv_cache" => Value::from_array((vec![2usize, 1, 16, 16, 33], self.conv_cache.clone())).map_err(|e| e.to_string())?,
            "tra_cache" => Value::from_array((vec![2usize, 3, 1, 1, 16], self.tra_cache.clone())).map_err(|e| e.to_string())?,
            "inter_cache" => Value::from_array((vec![2usize, 1, 33, 16], self.inter_cache.clone())).map_err(|e| e.to_string())?,
        ];

        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| format!("GTCRN ONNX run failed: {}", e))?;

        let (_, enh_data) = outputs
            .get("enh")
            .ok_or_else(|| "Missing 'enh' output".to_string())?
            .try_extract_tensor::<f32>()
            .map_err(|e| format!("Failed to extract 'enh': {}", e))?;

        let (_, conv_out) = outputs
            .get("conv_cache_out")
            .ok_or_else(|| "Missing 'conv_cache_out' output".to_string())?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let (_, tra_out) = outputs
            .get("tra_cache_out")
            .ok_or_else(|| "Missing 'tra_cache_out' output".to_string())?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;
        let (_, inter_out) = outputs
            .get("inter_cache_out")
            .ok_or_else(|| "Missing 'inter_cache_out' output".to_string())?
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;

        self.conv_cache.copy_from_slice(conv_out);
        self.tra_cache.copy_from_slice(tra_out);
        self.inter_cache.copy_from_slice(inter_out);

        let mut enh_bins = Vec::with_capacity(FREQ_BINS);
        for k in 0..FREQ_BINS {
            enh_bins.push(Complex::new(enh_data[k * 2], enh_data[k * 2 + 1]));
        }
        Ok(enh_bins)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn model_path_for_test() -> Option<std::path::PathBuf> {
        let p = resolve_model_path();
        if p.exists() {
            Some(p)
        } else {
            None
        }
    }

    #[test]
    fn denoiser_preserves_length_and_produces_finite_output() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: gtcrn_simple.onnx not present");
            return;
        };
        let mut denoiser = GtcrnDenoiser::new(&path).expect("load GTCRN model");

        // 0.5s of synthetic "noisy" audio: a tone plus broadband noise.
        let n: usize = 8000;
        let samples: Vec<f32> = (0..n as u32)
            .map(|i| {
                let t = i as f32 / 16000.0;
                let tone = 0.3 * (2.0 * std::f32::consts::PI * 220.0 * t).sin();
                let noise = ((i.wrapping_mul(2654435761u32)) % 1000) as f32 / 1000.0 - 0.5;
                tone + 0.2 * noise
            })
            .collect();

        let mut total_out = 0usize;
        for chunk in samples.chunks(320) {
            let out = denoiser.process_audio(chunk).expect("process_audio");
            assert!(out.iter().all(|s| s.is_finite()), "non-finite sample in output");
            total_out += out.len();
        }

        // Steady-state output tracks input 1:1 hop-for-hop; only the initial
        // ~1 window (512 samples) of algorithmic latency is missing.
        assert!(
            total_out + WIN >= n,
            "expected output length close to input length ({} vs {})",
            total_out,
            n
        );
    }

    #[test]
    fn reset_clears_all_streaming_state() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: gtcrn_simple.onnx not present");
            return;
        };
        let mut denoiser = GtcrnDenoiser::new(&path).expect("load GTCRN model");
        let samples = vec![0.1f32; 4000];
        let _ = denoiser.process_audio(&samples).expect("process_audio");
        denoiser.reset();
        assert!(denoiser.conv_cache.iter().all(|&v| v == 0.0));
        assert!(denoiser.tra_cache.iter().all(|&v| v == 0.0));
        assert!(denoiser.inter_cache.iter().all(|&v| v == 0.0));
        assert_eq!(denoiser.analysis_hist, vec![0.0; WIN]);
        assert!(!denoiser.primed);
    }
}
