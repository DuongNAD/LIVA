//! Adaptive Digital Automatic Gain Control (AGC) and Peak Limiter.
//!
//! Provides signal normalization for speech input pipelines:
//! - 80Hz 2nd-order Butterworth high-pass filter (removes DC offset & rumble)
//! - Adaptive short-term RMS energy follower
//! - Configurable target level (default: -18.0 dBFS)
//! - Smooth asymmetrical attack (10ms) and release (100ms)
//! - Noise floor gate (-50 dBFS) preventing noise pumping during pauses
//! - C1 continuous soft-knee peak limiter (-3.0 dBFS ceiling)

const SAMPLE_RATE: f32 = 16000.0;

/// 2nd-order Butterworth High-Pass Biquad Filter (80Hz cutoff @ 16kHz).
#[derive(Clone, Debug)]
pub struct HighPassFilter {
    b0: f32,
    b1: f32,
    b2: f32,
    a1: f32,
    a2: f32,
    x1: f32,
    x2: f32,
    y1: f32,
    y2: f32,
}

impl HighPassFilter {
    pub fn new_80hz_16k() -> Self {
        let fc = 80.0f32;
        let omega = 2.0 * std::f32::consts::PI * fc / SAMPLE_RATE;
        let sn = omega.sin();
        let cs = omega.cos();
        let alpha = sn / (2.0 * std::f32::consts::FRAC_1_SQRT_2);

        let a0 = 1.0 + alpha;
        let b0 = ((1.0 + cs) / 2.0) / a0;
        let b1 = (-(1.0 + cs)) / a0;
        let b2 = ((1.0 + cs) / 2.0) / a0;
        let a1 = (-2.0 * cs) / a0;
        let a2 = (1.0 - alpha) / a0;

        Self {
            b0,
            b1,
            b2,
            a1,
            a2,
            x1: 0.0,
            x2: 0.0,
            y1: 0.0,
            y2: 0.0,
        }
    }

    #[inline(always)]
    pub fn process_sample(&mut self, x: f32) -> f32 {
        let x_safe = if x.is_finite() { x } else { 0.0 };
        let y = self.b0 * x_safe + self.b1 * self.x1 + self.b2 * self.x2
            - self.a1 * self.y1
            - self.a2 * self.y2;
        let y_safe = if y.is_finite() { y } else { 0.0 };
        self.x2 = self.x1;
        self.x1 = x_safe;
        self.y2 = self.y1;
        self.y1 = y_safe;
        y_safe
    }

    pub fn reset(&mut self) {
        self.x1 = 0.0;
        self.x2 = 0.0;
        self.y1 = 0.0;
        self.y2 = 0.0;
    }
}

/// Digital AGC with soft-knee peak limiter.
#[derive(Clone, Debug)]
pub struct Agc {
    target_rms: f32,
    max_gain_linear: f32,
    min_gain_linear: f32,
    current_gain: f32,
    energy_ema: f32,
    alpha_energy: f32,
    alpha_attack: f32,
    alpha_release: f32,
    noise_floor_rms: f32,
    limiter_limit: f32,
    limiter_knee_start: f32,
    high_pass: HighPassFilter,
}

impl Agc {
    /// Create a new AGC instance with explicit target dBFS and max gain dB.
    pub fn new(target_dbfs: f32, max_gain_db: f32) -> Self {
        let target_rms = 10.0f32.powf(target_dbfs / 20.0);
        let max_gain_linear = 10.0f32.powf(max_gain_db.abs() / 20.0);
        let min_gain_linear = 10.0f32.powf(-12.0 / 20.0); // -12 dB attenuation limit

        // 10ms energy time constant
        let alpha_energy = (-1.0 / (0.010 * SAMPLE_RATE)).exp();
        // 10ms attack time constant (faster reduction when loud)
        let alpha_attack = (-1.0 / (0.010 * SAMPLE_RATE)).exp();
        // 100ms release time constant (slower recovery when quiet)
        let alpha_release = (-1.0 / (0.100 * SAMPLE_RATE)).exp();

        // -50 dBFS noise gate floor
        let noise_floor_rms = 10.0f32.powf(-50.0 / 20.0);
        // -3.0 dBFS limiter ceiling
        let limiter_limit = 10.0f32.powf(-3.0 / 20.0);
        // -6.0 dBFS limiter knee start
        let limiter_knee_start = 10.0f32.powf(-6.0 / 20.0);

        Self {
            target_rms,
            max_gain_linear,
            min_gain_linear,
            current_gain: 1.0,
            energy_ema: target_rms * target_rms,
            alpha_energy,
            alpha_attack,
            alpha_release,
            noise_floor_rms,
            limiter_limit,
            limiter_knee_start,
            high_pass: HighPassFilter::new_80hz_16k(),
        }
    }

    /// Standard configuration for 16kHz audio: target -18 dBFS, max boost +20 dB.
    pub fn default_16k() -> Self {
        Self::new(-18.0, 20.0)
    }

    /// Process a block of 16kHz mono audio samples in-place.
    /// Sanitizes non-finite inputs (NaN, +/-Inf) to 0.0 and guarantees finite, bounded outputs.
    pub fn process(&mut self, samples: &mut [f32]) {
        for sample in samples.iter_mut() {
            // Sanitize non-finite inputs: replace NaN / Inf with 0.0
            let in_sample = if sample.is_finite() { *sample } else { 0.0 };

            // 1. 80Hz High-pass filtering
            let hp_x = self.high_pass.process_sample(in_sample);

            // 2. Update energy follower
            let x2 = hp_x * hp_x;
            self.energy_ema = self.alpha_energy * self.energy_ema + (1.0 - self.alpha_energy) * x2;
            if !self.energy_ema.is_finite() || self.energy_ema < 0.0 {
                self.energy_ema = self.target_rms * self.target_rms;
            }
            let current_rms = self.energy_ema.sqrt();

            // 3. Compute target gain with noise floor protection
            let target_gain = if current_rms < self.noise_floor_rms {
                1.0 // Decay towards unity gain during silence
            } else {
                let desired = self.target_rms / (current_rms + 1e-6);
                desired.clamp(self.min_gain_linear, self.max_gain_linear)
            };

            // 4. Smooth gain adaptation (Attack vs Release)
            if target_gain < self.current_gain {
                self.current_gain = self.alpha_attack * self.current_gain
                    + (1.0 - self.alpha_attack) * target_gain;
            } else {
                self.current_gain = self.alpha_release * self.current_gain
                    + (1.0 - self.alpha_release) * target_gain;
            }
            if !self.current_gain.is_finite() {
                self.current_gain = 1.0;
            }

            // 5. Apply AGC gain
            let gained = hp_x * self.current_gain;

            // 6. Soft-knee peak limiter (-3 dBFS limit)
            let out = self.apply_soft_knee_limiter(gained);
            *sample = if out.is_finite() { out } else { 0.0 };
        }
    }

    /// Smooth soft-knee saturation curve strictly bounding peak output to -3 dBFS.
    #[inline(always)]
    fn apply_soft_knee_limiter(&self, x: f32) -> f32 {
        if !x.is_finite() {
            return 0.0;
        }
        let sign = if x < 0.0 { -1.0 } else { 1.0 };
        let u = x.abs();
        let k = self.limiter_knee_start; // ~0.50119 (-6 dBFS)
        let l = self.limiter_limit;      // ~0.70795 (-3 dBFS)

        if u <= k {
            x
        } else if u <= l {
            let delta = u - k;
            let span = l - k;
            let compressed = k + delta - (delta * delta) / (4.0 * span);
            sign * compressed
        } else {
            let v_l = k + 0.75 * (l - k);
            let span_rem = l - v_l;
            let compressed = v_l + span_rem * (1.0 - (-0.5 * (u - l) / span_rem).exp());
            sign * compressed
        }
    }

    /// Reset AGC tracking state (call on turn/utterance boundaries).
    pub fn reset(&mut self) {
        self.current_gain = 1.0;
        self.energy_ema = self.target_rms * self.target_rms;
        self.high_pass.reset();
    }

    /// Current applied gain in dB.
    pub fn current_gain_db(&self) -> f32 {
        20.0 * self.current_gain.log10()
    }
}

impl Default for Agc {
    fn default() -> Self {
        Self::default_16k()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calculate_rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        let sum_sq: f32 = samples.iter().map(|&s| s * s).sum();
        (sum_sq / samples.len() as f32).sqrt()
    }

    #[test]
    fn agc_boosts_quiet_audio_towards_target_dbfs() {
        let mut agc = Agc::default_16k();
        // 1.0s of 400Hz sine wave at -35 dBFS (RMS approx 0.0177)
        let freq = 400.0f32;
        let target_rms_in = 10.0f32.powf(-35.0 / 20.0);
        let amplitude = target_rms_in * std::f32::consts::SQRT_2;
        let mut samples: Vec<f32> = (0..16000)
            .map(|i| {
                let t = i as f32 / 16000.0;
                amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let initial_rms = calculate_rms(&samples);
        assert!((initial_rms - target_rms_in).abs() < 0.005);

        agc.process(&mut samples);

        // Check RMS of the second half (after convergence)
        let converged_rms = calculate_rms(&samples[8000..]);
        let expected_target_rms = 10.0f32.powf(-18.0 / 20.0); // ~0.12589
        assert!(
            (converged_rms - expected_target_rms).abs() < 0.035,
            "Quiet audio must be boosted towards -18 dBFS (got RMS {}, expected ~{})",
            converged_rms,
            expected_target_rms
        );
        assert!(
            converged_rms > initial_rms * 3.0,
            "Output RMS must be significantly higher than quiet input"
        );
    }

    #[test]
    fn agc_attenuates_loud_audio_towards_target_dbfs() {
        let mut agc = Agc::default_16k();
        // 1.0s of 400Hz sine wave at -6 dBFS (RMS approx 0.501)
        let freq = 400.0f32;
        let target_rms_in = 10.0f32.powf(-6.0 / 20.0);
        let amplitude = target_rms_in * std::f32::consts::SQRT_2;
        let mut samples: Vec<f32> = (0..16000)
            .map(|i| {
                let t = i as f32 / 16000.0;
                amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        let initial_rms = calculate_rms(&samples);
        agc.process(&mut samples);

        let converged_rms = calculate_rms(&samples[8000..]);
        let expected_target_rms = 10.0f32.powf(-18.0 / 20.0); // ~0.12589
        assert!(
            converged_rms < initial_rms,
            "Loud audio must be attenuated (initial {}, converged {})",
            initial_rms,
            converged_rms
        );
        assert!(
            (converged_rms - expected_target_rms).abs() < 0.05,
            "Loud audio must settle near -18 dBFS (got RMS {}, expected ~{})",
            converged_rms,
            expected_target_rms
        );
    }

    #[test]
    fn agc_soft_knee_limiter_strictly_bounds_peaks() {
        let mut agc = Agc::default_16k();
        // Overdriven signal with peaks up to 4.0 (+12 dBFS)
        let mut samples: Vec<f32> = (0..3200)
            .map(|i| {
                let t = i as f32 / 16000.0;
                4.0 * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect();

        agc.process(&mut samples);

        let max_peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
        let limiter_ceiling = 10.0f32.powf(-3.0 / 20.0); // ~0.70795 (-3 dBFS)

        assert!(
            max_peak <= limiter_ceiling + 1e-4,
            "Peak amplitude ({}) must not exceed -3.0 dBFS ceiling ({})",
            max_peak,
            limiter_ceiling
        );
        assert!(samples.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn agc_noise_gate_preserves_silence_without_noise_pumping() {
        let mut agc = Agc::default_16k();
        // 1.0s of very low-level background noise (-65 dBFS, amplitude ~0.0005)
        let mut samples: Vec<f32> = (0..16000u32)
            .map(|i| {
                let pseudo_rand = ((i.wrapping_mul(1664525).wrapping_add(1013904223)) % 1000) as f32 / 1000.0 - 0.5;
                pseudo_rand * 0.001
            })
            .collect();

        let initial_rms = calculate_rms(&samples);
        agc.process(&mut samples);

        let final_gain_db = agc.current_gain_db();
        // Noise floor gate should freeze target gain to 1.0 (0 dB), preventing +20 dB boost
        assert!(
            final_gain_db < 3.0,
            "Noise gate must prevent gain expansion on silence (gain was {} dB, expected < 3 dB)",
            final_gain_db
        );
        let final_rms = calculate_rms(&samples);
        assert!(
            final_rms < initial_rms * 2.0,
            "Noise must not be boosted heavily (initial {}, final {})",
            initial_rms,
            final_rms
        );
    }

    #[test]
    fn high_pass_filter_attenuates_dc_and_sub_80hz() {
        let mut hpf = HighPassFilter::new_80hz_16k();
        // Signal: DC offset 0.5 + 20Hz sine wave (well below 80Hz cutoff)
        let n = 16000;
        let mut filtered = Vec::with_capacity(n);
        for i in 0..n {
            let t = i as f32 / 16000.0;
            let x = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 20.0 * t).sin();
            filtered.push(hpf.process_sample(x));
        }

        // Check tail (after filter warmup)
        let tail = &filtered[8000..];
        let mean_dc: f32 = tail.iter().sum::<f32>() / tail.len() as f32;
        assert!(
            mean_dc.abs() < 0.01,
            "DC offset must be removed by 80Hz HPF (got mean {})",
            mean_dc
        );

        let tail_rms = calculate_rms(tail);
        // 20Hz is 2 octaves below 80Hz cutoff -> 2nd order Butterworth gives ~24dB attenuation
        assert!(
            tail_rms < 0.1,
            "20Hz sub-rumble must be attenuated significantly (got RMS {})",
            tail_rms
        );
    }

    #[test]
    fn agc_reset_clears_tracking_state() {
        let mut agc = Agc::default_16k();
        let mut loud: Vec<f32> = (0..1600)
            .map(|i| {
                let t = i as f32 / 16000.0;
                0.8 * (2.0 * std::f32::consts::PI * 400.0 * t).sin()
            })
            .collect();
        agc.process(&mut loud);
        assert!(
            agc.current_gain < 0.9,
            "Loud AC signal must reduce AGC gain (got {})",
            agc.current_gain
        );

        agc.reset();
        assert_eq!(agc.current_gain, 1.0);
        assert_eq!(agc.high_pass.x1, 0.0);
        assert_eq!(agc.high_pass.y1, 0.0);
    }

    #[test]
    fn agc_sanitizes_non_finite_inputs_cleanly() {
        let mut agc = Agc::default_16k();
        let mut corrupt = vec![
            f32::NAN,
            f32::INFINITY,
            f32::NEG_INFINITY,
            0.1,
            f32::NAN,
            -0.2,
            f32::INFINITY,
        ];
        agc.process(&mut corrupt);
        assert!(
            corrupt.iter().all(|s| s.is_finite()),
            "All output samples must be finite even with NaN/Inf inputs"
        );
        let limiter_ceiling = 10.0f32.powf(-3.0 / 20.0);
        assert!(
            corrupt.iter().all(|s| s.abs() <= limiter_ceiling + 1e-4),
            "Output must respect limiter ceiling"
        );

        // Verify normal operation resumes afterwards
        let mut clean: Vec<f32> = (0..1600)
            .map(|i| 0.1 * (2.0 * std::f32::consts::PI * 400.0 * i as f32 / 16000.0).sin())
            .collect();
        agc.process(&mut clean);
        assert!(clean.iter().all(|s| s.is_finite()));
        let rms = calculate_rms(&clean);
        assert!(rms > 0.05, "AGC should process clean audio normally after corruption");
    }
}
