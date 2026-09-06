//! Self-echo cancellation via Sonora (pure-Rust WebRTC AEC3, BSD-3-Clause).
//!
//! LIVA's own TTS voice, played on the user's speakers, can leak back into
//! their physical microphone and re-appear in the next `OP_MIC_IN` capture —
//! this is what makes barge-in transcripts noisy. The far-end reference
//! AEC3 needs is exactly the PCM we just sent as `OP_SPEAKER_OUT`, so
//! [`SelfEchoCanceller::push_render`] is fed from the same chunk emission
//! point pipeline.rs already has, resampled down to the AEC's 16kHz
//! operating rate.
//!
//! Includes a bandlimited polyphase sinc resampler for 22050/24000/48000Hz -> 16000Hz
//! downsampling and a 30ms (480-sample) far-end delay compensation FIFO queue.
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;

const SAMPLE_RATE: u32 = 16000;
const FRAME_SIZE: usize = (SAMPLE_RATE / 100) as usize; // 10ms = 160 samples
const DELAY_MS: usize = 30;
const DELAY_SAMPLES: usize = (SAMPLE_RATE as usize * DELAY_MS) / 1000; // 480 samples
const MAX_RENDER_QUEUE_SAMPLES: usize = 1600; // 100ms @ 16kHz

/// Bandlimited Sinc Resampler with Blackman-Harris windowing.
pub struct BandlimitedResampler;

impl BandlimitedResampler {
    /// Resample mono PCM audio from `source_rate` to 16000Hz with anti-aliasing filtering.
    pub fn resample_to_16k(input: &[f32], source_rate: u32) -> Vec<f32> {
        const TARGET_RATE: u32 = 16000;
        if input.is_empty() {
            return Vec::new();
        }
        if source_rate == TARGET_RATE {
            return input.to_vec();
        }

        let ratio = TARGET_RATE as f64 / source_rate as f64;
        let out_len = ((input.len() as f64) * ratio).round() as usize;
        let mut output = Vec::with_capacity(out_len);

        // Anti-aliasing cutoff margin (0.90 of Nyquist for downsampling)
        let cutoff = if source_rate > TARGET_RATE {
            (TARGET_RATE as f64 / source_rate as f64) * 0.90
        } else {
            0.90
        };

        const RADIUS: isize = 16;

        for i in 0..out_len {
            let src_time = i as f64 / ratio;
            let center = src_time.floor() as isize;
            let frac = src_time - center as f64;

            let mut sum = 0.0f64;
            let mut weight_sum = 0.0f64;

            for k in (-RADIUS)..=(RADIUS) {
                let idx = center + k;
                if idx >= 0 && (idx as usize) < input.len() {
                    let t = k as f64 - frac;
                    let x = t * cutoff;
                    let sinc_val = if x.abs() < 1e-7 {
                        1.0
                    } else {
                        (std::f64::consts::PI * x).sin() / (std::f64::consts::PI * x)
                    };

                    let tau = (t / RADIUS as f64).clamp(-1.0, 1.0);
                    let u = (tau + 1.0) * 0.5; // [0.0, 1.0]
                    let w = 0.35875
                        - 0.48829 * (2.0 * std::f64::consts::PI * u).cos()
                        + 0.14128 * (4.0 * std::f64::consts::PI * u).cos()
                        - 0.01168 * (6.0 * std::f64::consts::PI * u).cos();

                    let weight = sinc_val * w;
                    sum += input[idx as usize] as f64 * weight;
                    weight_sum += weight;
                }
            }

            let sample = if weight_sum.abs() > 1e-6 {
                (sum / weight_sum) as f32
            } else {
                0.0f32
            };
            output.push(sample);
        }

        output
    }
}

pub struct SelfEchoCanceller {
    apm: AudioProcessing,
    render_queue: VecDeque<f32>,
    capture_pending: VecDeque<f32>,
    delay_fifo: VecDeque<f32>,
}

impl SelfEchoCanceller {
    pub fn new() -> Self {
        let stream_config = StreamConfig::new(SAMPLE_RATE, 1);
        let config = Config {
            echo_canceller: Some(EchoCanceller::default()),
            ..Default::default()
        };
        let apm = AudioProcessing::builder()
            .config(config)
            .capture_config(stream_config)
            .render_config(stream_config)
            .build();

        Self {
            apm,
            render_queue: VecDeque::new(),
            capture_pending: VecDeque::new(),
            delay_fifo: VecDeque::from(vec![0.0f32; DELAY_SAMPLES]),
        }
    }

    /// Feed audio LIVA is playing into the 30ms delay queue as AEC far-end reference.
    /// Bounds the render queue to 100ms (1600 samples) by dropping oldest unconsumed samples.
    pub fn push_render(&mut self, samples: &[f32], source_rate: u32) {
        if samples.is_empty() {
            return;
        }
        let resampled = BandlimitedResampler::resample_to_16k(samples, source_rate);
        for sample in resampled {
            self.delay_fifo.push_back(sample);
            if self.delay_fifo.len() > DELAY_SAMPLES
                && let Some(delayed_sample) = self.delay_fifo.pop_front()
            {
                self.render_queue.push_back(delayed_sample);
            }
        }

        // Bounded capacity: drop oldest render samples if mic capture is inactive or slower,
        // avoiding desynchronization and unbounded memory growth.
        if self.render_queue.len() > MAX_RENDER_QUEUE_SAMPLES {
            let overflow = self.render_queue.len() - MAX_RENDER_QUEUE_SAMPLES;
            self.render_queue.drain(..overflow);
        }
    }

    /// Cancel self-echo out of a 16kHz mono mic buffer.
    pub fn process_capture(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.capture_pending.extend(samples.iter().copied());
        let mut out = Vec::with_capacity(samples.len());

        while self.capture_pending.len() >= FRAME_SIZE {
            let mic_frame: Vec<f32> = self.capture_pending.drain(0..FRAME_SIZE).collect();

            if self.render_queue.len() >= FRAME_SIZE {
                let render_frame: Vec<f32> = self.render_queue.drain(0..FRAME_SIZE).collect();
                let mut render_out = vec![0.0f32; FRAME_SIZE];
                self.apm
                    .process_render_f32(&[&render_frame], &mut [&mut render_out])
                    .map_err(|e| format!("AEC3 process_render failed: {:?}", e))?;
            }

            let mut capture_out = vec![0.0f32; FRAME_SIZE];
            self.apm
                .process_capture_f32(&[&mic_frame], &mut [&mut capture_out])
                .map_err(|e| format!("AEC3 process_capture failed: {:?}", e))?;
            out.extend_from_slice(&capture_out);
        }

        Ok(out)
    }

    /// Reset internal state on session boundaries.
    pub fn reset(&mut self) {
        self.render_queue.clear();
        self.capture_pending.clear();
        self.delay_fifo.clear();
        self.delay_fifo.extend(std::iter::repeat_n(0.0f32, DELAY_SAMPLES));
    }
}

impl Default for SelfEchoCanceller {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_capture_preserves_length_and_stays_finite() {
        let mut aec = SelfEchoCanceller::new();
        let mic: Vec<f32> = (0..1600)
            .map(|i| 0.3 * (2.0 * std::f32::consts::PI * 300.0 * i as f32 / 16000.0).sin())
            .collect();
        let out = aec.process_capture(&mic).expect("process_capture");
        assert_eq!(out.len(), 1600);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn push_render_resamples_and_does_not_panic_with_mismatched_rates() {
        let mut aec = SelfEchoCanceller::new();
        let tts_chunk: Vec<f32> = vec![0.1; 2205]; // 100ms @ 22050Hz (Piper's rate)
        aec.push_render(&tts_chunk, 22050);
        assert!(!aec.render_queue.is_empty());

        let mic: Vec<f32> = vec![0.05; 1600];
        let out = aec
            .process_capture(&mic)
            .expect("process_capture with render queued");
        assert_eq!(out.len(), 1600);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn push_render_polyphase_resamples_24k_to_16k_cleanly() {
        let input: Vec<f32> = (0..2400)
            .map(|i| {
                let t = i as f32 / 24000.0;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        let resampled = BandlimitedResampler::resample_to_16k(&input, 24000);
        assert_eq!(resampled.len(), 1600);
        assert!(resampled.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn push_render_polyphase_resamples_22050_to_16k_cleanly() {
        let input: Vec<f32> = (0..2205)
            .map(|i| {
                let t = i as f32 / 22050.0;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        let resampled = BandlimitedResampler::resample_to_16k(&input, 22050);
        assert_eq!(resampled.len(), 1600);
        assert!(resampled.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn push_render_polyphase_resamples_48k_to_16k_cleanly() {
        let input: Vec<f32> = (0..4800)
            .map(|i| {
                let t = i as f32 / 48000.0;
                0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
            })
            .collect();
        let resampled = BandlimitedResampler::resample_to_16k(&input, 48000);
        assert_eq!(resampled.len(), 1600);
        assert!(resampled.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn aec_delay_fifo_delays_far_end_reference_by_30ms() {
        let mut aec = SelfEchoCanceller::new();
        assert_eq!(aec.render_queue.len(), 0);

        // Push 480 samples (30ms @ 16kHz) of constant 1.0
        let burst = vec![1.0f32; 480];
        aec.push_render(&burst, 16000);

        // Exactly 480 samples were pushed: because delay FIFO was pre-filled with 480 zeros,
        // the 480 zero samples were pushed out into render_queue while the 1.0 samples reside in delay_fifo.
        assert_eq!(aec.render_queue.len(), 480);
        assert!(
            aec.render_queue.iter().all(|&s| s == 0.0),
            "Initial 480 samples popped from FIFO must be the 30ms pre-delay zeros"
        );

        // Push another 160 samples (10ms) of 0.0
        let next = vec![0.0f32; 160];
        aec.push_render(&next, 16000);

        // Now render_queue has 480 + 160 = 640 samples. The first 160 samples of the 1.0 burst should now be in render_queue[480..640]
        assert_eq!(aec.render_queue.len(), 640);
        for &s in &aec.render_queue.make_contiguous()[480..640] {
            assert_eq!(s, 1.0, "Delayed signal must emerge after 30ms (480 samples)");
        }
    }

    #[test]
    fn leftover_tail_shorter_than_one_frame_is_carried_over() {
        let mut aec = SelfEchoCanceller::new();
        let first = vec![0.0f32; 100]; // < FRAME_SIZE (160)
        let out1 = aec.process_capture(&first).expect("first call");
        assert!(out1.is_empty(), "partial frame should not emit output yet");

        let second = vec![0.0f32; 100];
        let out2 = aec.process_capture(&second).expect("second call");
        assert_eq!(
            out2.len(),
            FRAME_SIZE,
            "combined 200 samples should flush one 160-frame"
        );
    }

    #[test]
    fn push_render_bounds_queue_capacity_to_100ms_dropping_oldest() {
        let mut aec = SelfEchoCanceller::new();
        // Push 4800 samples (300ms @ 16kHz) without capture running
        let large_render: Vec<f32> = (0..4800).map(|i| i as f32).collect();
        aec.push_render(&large_render, 16000);

        assert_eq!(
            aec.render_queue.len(),
            MAX_RENDER_QUEUE_SAMPLES,
            "Render queue must be bounded strictly to 1600 samples (100ms)"
        );

        // Verify that the queue contains the freshest samples:
        // Input had 4800 samples. 480 pre-delay -> delayed samples were 0..4800-480 = 0..4320.
        // Queue retained the last 1600 of those, i.e., 4320 - 1600 = 2720..4320.
        let slice = aec.render_queue.make_contiguous();
        assert_eq!(slice[0], 2720.0);
        assert_eq!(slice[slice.len() - 1], 4319.0);
    }
}
