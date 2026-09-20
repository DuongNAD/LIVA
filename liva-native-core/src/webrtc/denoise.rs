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
use ort::{session::Session, value::TensorRef};
use rustfft::{Fft, FftPlanner, num_complex::Complex};
use std::path::Path;
use std::sync::{Arc, Mutex};

const WIN: usize = 512;
const HOP: usize = 256;
const FREQ_BINS: usize = WIN / 2 + 1; // 257

// Các thừa số viết đầy đủ theo ĐÚNG shape tensor cache của GTCRN để đối chiếu
// với model — các số `1` là chiều thật (batch/nhóm), không phải phép nhân thừa.
// Vì vậy tắt `identity_op` ở đây: xoá số `1` cho "gọn" là làm mất tài liệu shape.
#[allow(clippy::identity_op)]
const CONV_CACHE_LEN: usize = 2 * 1 * 16 * 16 * 33; // [2,1,16,16,33]
#[allow(clippy::identity_op)]
const TRA_CACHE_LEN: usize = 2 * 3 * 1 * 1 * 16; // [2,3,1,1,16]
#[allow(clippy::identity_op)]
const INTER_CACHE_LEN: usize = 2 * 1 * 33 * 16; // [2,1,33,16]

/// Resolve the GTCRN model path: env override, else `models/gtcrn_simple.onnx`
/// resolved via `crate::resolve_resource_path` across repo, user data dir, and exe resources.
pub fn resolve_model_path() -> std::path::PathBuf {
    use std::path::PathBuf;
    if let Ok(p) = std::env::var("LIVA_DENOISE_MODEL_PATH")
        && !p.trim().is_empty()
    {
        return PathBuf::from(p);
    }
    crate::resolve_resource_path("models/gtcrn_simple.onnx")
}

pub struct GtcrnDenoiser {
    /// Shared model runner; recurrent/STFT buffers below belong to one audio
    /// stream. ONNX inference is serialized, matching the old global mutex.
    session: Arc<Mutex<Session>>,
    fft: Arc<dyn Fft<f32>>,
    ifft: Arc<dyn Fft<f32>>,
    window: Arc<[f32]>, // sqrt-Hann, len WIN

    analysis_hist: Vec<f32>, // last WIN raw input samples (shifts by HOP)
    ola_buf: Vec<f32>,       // overlap-add synthesis accumulator, len WIN
    pending_in: Vec<f32>,    // raw samples not yet consumed into a hop
    primed: bool,            // whether analysis_hist has seen real audio

    conv_cache: Vec<f32>,
    tra_cache: Vec<f32>,
    inter_cache: Vec<f32>,
    mix_buf: Vec<f32>,
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
        let window: Arc<[f32]> = (0..WIN)
            .map(|i| {
                let hann = 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / WIN as f32).cos();
                hann.sqrt()
            })
            .collect::<Vec<_>>()
            .into();

        Ok(Self {
            session: Arc::new(Mutex::new(session)),
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
            mix_buf: vec![0.0; FREQ_BINS * 2],
        })
    }

    /// Create fresh streaming buffers while sharing the loaded model, FFT plans,
    /// and immutable window with another WebSocket session.
    pub fn fork_session(&self) -> Self {
        Self {
            session: Arc::clone(&self.session),
            fft: Arc::clone(&self.fft),
            ifft: Arc::clone(&self.ifft),
            window: Arc::clone(&self.window),
            analysis_hist: vec![0.0; WIN],
            ola_buf: vec![0.0; WIN],
            pending_in: Vec::with_capacity(HOP * 2),
            primed: false,
            conv_cache: vec![0.0; CONV_CACHE_LEN],
            tra_cache: vec![0.0; TRA_CACHE_LEN],
            inter_cache: vec![0.0; INTER_CACHE_LEN],
            mix_buf: vec![0.0; FREQ_BINS * 2],
        }
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
            self.primed = true;

            // Slide the analysis window: drop oldest HOP, append newest HOP.
            self.analysis_hist.copy_within(HOP..WIN, 0);
            self.analysis_hist[WIN - HOP..].copy_from_slice(&self.pending_in[..HOP]);
            self.pending_in.drain(..HOP);

            let enh_bins = self.run_frame()?;

            if enh_bins.len() < FREQ_BINS {
                return Err(format!(
                    "GTCRN run_frame returned insufficient frequency bins: expected at least {}, got {}",
                    FREQ_BINS,
                    enh_bins.len()
                ));
            }

            // ISTFT: rebuild the full conjugate-symmetric spectrum, inverse
            // FFT, re-window (sqrt-Hann synthesis side), overlap-add.
            let mut full = vec![Complex::new(0.0f32, 0.0f32); WIN];
            full[..FREQ_BINS].copy_from_slice(&enh_bins[..FREQ_BINS]);
            for k in 1..(WIN / 2) {
                full[WIN - k] = enh_bins[k].conj();
            }
            self.ifft.process(&mut full);
            let inv_scale = 1.0 / WIN as f32;

            for ((o, f), &w) in self.ola_buf.iter_mut().zip(&full).zip(self.window.iter()) {
                *o += f.re * inv_scale * w;
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

        if buf.len() < FREQ_BINS {
            return Err(format!(
                "GTCRN STFT buffer underflow: expected at least {} bins, got {}",
                FREQ_BINS,
                buf.len()
            ));
        }
        if self.mix_buf.len() < FREQ_BINS * 2 {
            return Err(format!(
                "GTCRN mix_buf underflow: expected at least {} floats, got {}",
                FREQ_BINS * 2,
                self.mix_buf.len()
            ));
        }

        // Arrange as ONNX tensor layout (1, 257, 1, 2): [freq][re, im].
        // Reuses pre-allocated mix_buf to prevent real-time heap allocation.
        for (k, c) in buf[..FREQ_BINS].iter().enumerate() {
            self.mix_buf[k * 2] = c.re;
            self.mix_buf[k * 2 + 1] = c.im;
        }

        // Verify cache dimensions before creating TensorRef views
        if self.conv_cache.len() != CONV_CACHE_LEN {
            return Err(format!(
                "GTCRN conv_cache length invalid: expected {}, got {}",
                CONV_CACHE_LEN,
                self.conv_cache.len()
            ));
        }
        if self.tra_cache.len() != TRA_CACHE_LEN {
            return Err(format!(
                "GTCRN tra_cache length invalid: expected {}, got {}",
                TRA_CACHE_LEN,
                self.tra_cache.len()
            ));
        }
        if self.inter_cache.len() != INTER_CACHE_LEN {
            return Err(format!(
                "GTCRN inter_cache length invalid: expected {}, got {}",
                INTER_CACHE_LEN,
                self.inter_cache.len()
            ));
        }

        // Borrow recurrent state and spectrum tensors directly via TensorRef views,
        // eliminating 67.5KB heap allocation churn (self.conv_cache.clone()) per hop.
        let mix_ref =
            TensorRef::from_array_view(([1usize, FREQ_BINS, 1, 2], self.mix_buf.as_slice()))
                .map_err(|e| e.to_string())?;
        let conv_ref =
            TensorRef::from_array_view(([2usize, 1, 16, 16, 33], self.conv_cache.as_slice()))
                .map_err(|e| e.to_string())?;
        let tra_ref =
            TensorRef::from_array_view(([2usize, 3, 1, 1, 16], self.tra_cache.as_slice()))
                .map_err(|e| e.to_string())?;
        let inter_ref =
            TensorRef::from_array_view(([2usize, 1, 33, 16], self.inter_cache.as_slice()))
                .map_err(|e| e.to_string())?;

        let inputs = ort::inputs![
            "mix" => mix_ref,
            "conv_cache" => conv_ref,
            "tra_cache" => tra_ref,
            "inter_cache" => inter_ref,
        ];

        let mut session = self
            .session
            .lock()
            .map_err(|_| "GTCRN ONNX session mutex poisoned".to_string())?;
        let outputs = session
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

        // Zero-panic bounds validation on ONNX tensor output slices before copying
        if conv_out.len() != self.conv_cache.len() {
            return Err(format!(
                "GTCRN conv_cache_out slice length mismatch: expected {}, got {}",
                self.conv_cache.len(),
                conv_out.len()
            ));
        }
        if tra_out.len() != self.tra_cache.len() {
            return Err(format!(
                "GTCRN tra_cache_out slice length mismatch: expected {}, got {}",
                self.tra_cache.len(),
                tra_out.len()
            ));
        }
        if inter_out.len() != self.inter_cache.len() {
            return Err(format!(
                "GTCRN inter_cache_out slice length mismatch: expected {}, got {}",
                self.inter_cache.len(),
                inter_out.len()
            ));
        }
        if enh_data.len() < FREQ_BINS * 2 {
            return Err(format!(
                "GTCRN enh output slice length insufficient: expected at least {}, got {}",
                FREQ_BINS * 2,
                enh_data.len()
            ));
        }

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
        if p.exists() { Some(p) } else { None }
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
            assert!(
                out.iter().all(|s| s.is_finite()),
                "non-finite sample in output"
            );
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

    #[test]
    fn fork_session_starts_with_independent_stream_state() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: gtcrn_simple.onnx not present");
            return;
        };
        let mut source = GtcrnDenoiser::new(&path).expect("load GTCRN model");
        let _ = source
            .process_audio(&vec![0.1f32; HOP * 2 + 17])
            .expect("prime source session");
        assert!(source.primed);
        assert!(!source.pending_in.is_empty());

        let fork = source.fork_session();

        assert!(
            Arc::ptr_eq(&source.session, &fork.session),
            "forks should reuse the loaded ONNX model"
        );
        assert!(source.primed, "fork must not reset the source session");
        assert!(!fork.primed, "fork must start before the first audio hop");
        assert!(fork.pending_in.is_empty());
        assert!(fork.conv_cache.iter().all(|&value| value == 0.0));
        assert!(fork.tra_cache.iter().all(|&value| value == 0.0));
        assert!(fork.inter_cache.iter().all(|&value| value == 0.0));
    }

    #[test]
    fn denoiser_zero_copy_cache_evolution() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: gtcrn_simple.onnx not present");
            return;
        };
        let mut denoiser = GtcrnDenoiser::new(&path).expect("load GTCRN model");

        // Verify initial cache capacities and zero-filled states
        assert_eq!(denoiser.conv_cache.len(), CONV_CACHE_LEN);
        assert_eq!(denoiser.tra_cache.len(), TRA_CACHE_LEN);
        assert_eq!(denoiser.inter_cache.len(), INTER_CACHE_LEN);
        assert_eq!(denoiser.mix_buf.len(), FREQ_BINS * 2);

        let initial_conv_ptr = denoiser.conv_cache.as_ptr();
        let initial_tra_ptr = denoiser.tra_cache.as_ptr();
        let initial_inter_ptr = denoiser.inter_cache.as_ptr();
        let initial_mix_ptr = denoiser.mix_buf.as_ptr();

        // Feed multiple hops of non-zero signal
        let signal: Vec<f32> = (0..HOP * 4)
            .map(|i| (i as f32 * 0.05).sin() * 0.4)
            .collect();
        let out = denoiser.process_audio(&signal).expect("process_audio");
        assert!(!out.is_empty());

        // Assert memory buffer pointers remained stable (zero re-allocations)
        assert_eq!(
            denoiser.conv_cache.as_ptr(),
            initial_conv_ptr,
            "conv_cache must not reallocate"
        );
        assert_eq!(
            denoiser.tra_cache.as_ptr(),
            initial_tra_ptr,
            "tra_cache must not reallocate"
        );
        assert_eq!(
            denoiser.inter_cache.as_ptr(),
            initial_inter_ptr,
            "inter_cache must not reallocate"
        );
        assert_eq!(
            denoiser.mix_buf.as_ptr(),
            initial_mix_ptr,
            "mix_buf must not reallocate"
        );

        // Verify recurrent states evolved and contain non-zero weights from audio
        let conv_has_signal = denoiser.conv_cache.iter().any(|&v| v != 0.0);
        let tra_has_signal = denoiser.tra_cache.iter().any(|&v| v != 0.0);
        assert!(
            conv_has_signal,
            "conv_cache should accumulate recurrent state"
        );
        assert!(
            tra_has_signal,
            "tra_cache should accumulate recurrent state"
        );
    }

    #[test]
    fn denoiser_handles_abnormal_and_adversarial_inputs_zero_panic() {
        let Some(path) = model_path_for_test() else {
            eprintln!("skip: gtcrn_simple.onnx not present");
            return;
        };
        let mut denoiser = GtcrnDenoiser::new(&path).expect("load GTCRN model");

        // 1. Empty input slice
        let out_empty = denoiser.process_audio(&[]).expect("empty input");
        assert!(out_empty.is_empty());

        // 2. Partial input slice (< HOP)
        let out_partial = denoiser
            .process_audio(&[0.1f32; 100])
            .expect("partial input");
        assert!(out_partial.is_empty());

        // 3. Sub-hop chunks streaming incrementally
        for _ in 0..50 {
            let res = denoiser.process_audio(&[0.05f32; 7]);
            assert!(res.is_ok());
        }

        // 4. Large burst chunk (e.g. 16000 samples = 1s)
        let burst = vec![0.02f32; 16000];
        let res_burst = denoiser.process_audio(&burst);
        assert!(res_burst.is_ok());

        // 5. Corrupted floats (NaN, Inf, -Inf)
        let mut weird_samples = vec![0.0f32; HOP * 2];
        weird_samples[10] = f32::NAN;
        weird_samples[20] = f32::INFINITY;
        weird_samples[30] = f32::NEG_INFINITY;
        weird_samples[40] = f32::MIN_POSITIVE;
        let res_weird = denoiser.process_audio(&weird_samples);
        assert!(
            res_weird.is_ok(),
            "Denoiser must not panic on NaN or Inf samples"
        );
    }
}
