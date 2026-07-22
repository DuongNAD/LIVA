#![allow(dead_code)]

use rustfft::{FftPlanner, num_complex::Complex};
use std::sync::Arc;

fn hz_to_mel(hz: f64) -> f64 {
    let f_min = 0.0;
    let f_sp = 200.0 / 3.0;
    if hz < 1000.0 {
        (hz - f_min) / f_sp
    } else {
        let min_log_hz = 1000.0;
        let min_log_mel = (min_log_hz - f_min) / f_sp;
        let logstep = 6.4f64.ln() / 27.0;
        min_log_mel + (hz / min_log_hz).ln() / logstep
    }
}

fn mel_to_hz(mel: f64) -> f64 {
    let f_min = 0.0;
    let f_sp = 200.0 / 3.0;
    let min_log_hz = 1000.0;
    let min_log_mel = (min_log_hz - f_min) / f_sp;
    if mel < min_log_mel {
        f_min + f_sp * mel
    } else {
        let logstep = 6.4f64.ln() / 27.0;
        min_log_hz * (logstep * (mel - min_log_mel)).exp()
    }
}

pub fn compute_mel_filterbank(fft_size: usize, num_mels: usize, sample_rate: f64) -> Vec<Vec<f32>> {
    let num_bins = fft_size / 2 + 1;
    let min_mel = hz_to_mel(0.0);
    let max_mel = hz_to_mel(sample_rate / 2.0);

    let mel_f: Vec<f64> = (0..num_mels + 2)
        .map(|i| mel_to_hz(min_mel + (i as f64 * (max_mel - min_mel)) / (num_mels + 1) as f64))
        .collect();

    let fft_freqs: Vec<f64> = (0..num_bins)
        .map(|k| (k as f64 * sample_rate) / fft_size as f64)
        .collect();

    let mut filters = Vec::with_capacity(num_mels);
    for i in 0..num_mels {
        let mut filter = vec![0.0; num_bins];
        let fdiff_lower = mel_f[i + 1] - mel_f[i];
        let fdiff_upper = mel_f[i + 2] - mel_f[i + 1];
        let enorm = 2.0 / (mel_f[i + 2] - mel_f[i]);

        for k in 0..num_bins {
            let lower = (fft_freqs[k] - mel_f[i]) / fdiff_lower;
            let upper = (mel_f[i + 2] - fft_freqs[k]) / fdiff_upper;
            let weight = 0.0f64.max(lower.min(upper));
            filter[k] = (weight * enorm) as f32;
        }
        filters.push(filter);
    }
    filters
}

pub struct SttDsp {
    fft_size: usize,
    win_length: usize,
    hop_length: usize,
    num_mels: usize,
    log_eps: f32,
    hann_window: Vec<f32>,
    mel_filterbank: Vec<Vec<f32>>,
    fft: Arc<dyn rustfft::Fft<f32>>,
}

impl SttDsp {
    pub fn new(
        fft_size: usize,
        win_length: usize,
        hop_length: usize,
        num_mels: usize,
        sample_rate: f64,
        log_eps: f32,
    ) -> Self {
        let hann_window: Vec<f32> = (0..win_length)
            .map(|i| 0.5 * (1.0 - (2.0 * std::f32::consts::PI * i as f32 / win_length as f32).cos()))
            .collect();

        let mel_filterbank = compute_mel_filterbank(fft_size, num_mels, sample_rate);

        let mut planner = FftPlanner::new();
        let fft = planner.plan_fft_forward(fft_size);

        Self {
            fft_size,
            win_length,
            hop_length,
            num_mels,
            log_eps,
            hann_window,
            mel_filterbank,
            fft,
        }
    }

    pub fn compute_log_mel_spectrogram(&self, samples: &[f32]) -> Result<Vec<f32>, String> {
        if samples.len() != 10640 {
            return Err(format!(
                "compute_log_mel_spectrogram expects exactly 10640 samples, got {}",
                samples.len()
            ));
        }

        let num_frames = 65;
        let mut features = vec![0.0; num_frames * self.num_mels];
        let mut windowed_frame = vec![0.0; self.win_length];

        for f in 0..num_frames {
            let frame_center = f * self.hop_length;
            let start_idx = frame_center as isize - (self.win_length / 2) as isize;

            for (i, (wf, &h)) in windowed_frame
                .iter_mut()
                .zip(&self.hann_window)
                .enumerate()
                .take(self.win_length)
            {
                let mut idx = start_idx + i as isize;
                if idx < 0 {
                    idx = -idx;
                }
                if idx >= samples.len() as isize {
                    idx = 2 * samples.len() as isize - 2 - idx;
                }
                *wf = samples[idx as usize] * h;
            }

            // FFT center-padded to fft_size (512)
            let mut fft_buffer = vec![Complex::new(0.0, 0.0); self.fft_size];
            let offset = (self.fft_size - self.win_length) / 2; // (512 - 400) / 2 = 56
            for i in 0..self.win_length {
                if offset + i < self.fft_size {
                    fft_buffer[offset + i] = Complex::new(windowed_frame[i], 0.0);
                }
            }

            self.fft.process(&mut fft_buffer);

            // Compute power spectrum (magnitude squared)
            let num_bins = self.fft_size / 2 + 1;
            let mut power_spec = vec![0.0; num_bins];
            for i in 0..num_bins {
                power_spec[i] = fft_buffer[i].norm_sqr();
            }

            // Mel filterbank + log
            for m in 0..self.num_mels {
                let mut mel_energy = 0.0;
                let filter = &self.mel_filterbank[m];
                for k in 0..num_bins {
                    mel_energy += filter[k] * power_spec[k];
                }
                features[f * self.num_mels + m] = (mel_energy + self.log_eps).ln();
            }
        }

        Ok(features)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mel_filterbank_dimensions() {
        let filters = compute_mel_filterbank(512, 128, 16000.0);
        assert_eq!(filters.len(), 128);
        for filter in filters {
            assert_eq!(filter.len(), 257); // 512 / 2 + 1
        }
    }

    #[test]
    fn test_compute_spectrogram_wrong_length() {
        let dsp = SttDsp::new(512, 400, 160, 128, 16000.0, 1e-5);
        let samples = vec![0.0; 1000];
        let res = dsp.compute_log_mel_spectrogram(&samples);
        assert!(res.is_err());
        assert!(res.err().unwrap().contains("expects exactly 10640 samples"));
    }

    #[test]
    fn test_compute_spectrogram_zeros() {
        let dsp = SttDsp::new(512, 400, 160, 128, 16000.0, 1e-5);
        let samples = vec![0.0; 10640];
        let res = dsp.compute_log_mel_spectrogram(&samples);
        assert!(res.is_ok());
        let features = res.unwrap();
        assert_eq!(features.len(), 65 * 128);
        for &val in features.iter() {
            assert!((val - 1e-5f32.ln()).abs() < 1e-4);
        }
    }
}
