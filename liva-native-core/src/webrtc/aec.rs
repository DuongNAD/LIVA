//! Self-echo cancellation via Sonora (pure-Rust WebRTC AEC3, BSD-3-Clause).
//!
//! LIVA's own TTS voice, played on the user's speakers, can leak back into
//! their physical microphone and re-appear in the next `OP_MIC_IN` capture —
//! this is what makes barge-in transcripts noisy. The far-end reference
//! AEC3 needs is exactly the PCM we just sent as `OP_SPEAKER_OUT`, so
//! [`SelfEchoCanceller::push_render`] is fed from the same chunk emission
//! point pipeline.rs already has, resampled down to the AEC's 16kHz
//! operating rate (matching the mic/VAD/STT rate — no resampling needed on
//! the capture side). This only cancels *LIVA's own voice*; game audio
//! playing through the OS mixer isn't visible to this process and would
//! need a WASAPI loopback capture to address (out of scope here).
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;

const SAMPLE_RATE: u32 = 16000;
const FRAME_SIZE: usize = (SAMPLE_RATE / 100) as usize; // 10ms, Sonora's required frame size

pub struct SelfEchoCanceller {
    apm: AudioProcessing,
    render_queue: VecDeque<f32>,
    capture_pending: VecDeque<f32>,
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
        }
    }

    /// Feed audio LIVA is about to play (any sample rate — TTS voices run at
    /// 22050/24000Hz) as the AEC's far-end reference. Linearly resampled to
    /// 16kHz and queued; a following `process_capture` call registers it.
    pub fn push_render(&mut self, samples: &[f32], source_rate: u32) {
        if samples.is_empty() {
            return;
        }
        if source_rate == SAMPLE_RATE {
            self.render_queue.extend(samples.iter().copied());
            return;
        }
        let ratio = SAMPLE_RATE as f64 / source_rate as f64;
        let out_len = ((samples.len() as f64) * ratio).round() as usize;
        for i in 0..out_len {
            let src_pos = i as f64 / ratio;
            let idx = src_pos.floor() as usize;
            let frac = (src_pos - idx as f64) as f32;
            let a = samples[idx.min(samples.len() - 1)];
            let b = samples[(idx + 1).min(samples.len() - 1)];
            self.render_queue.push_back(a + (b - a) * frac);
        }
    }

    /// Cancel LIVA's own voice out of a 16kHz mono mic buffer of arbitrary
    /// length. Internally reblocks to Sonora's fixed 10ms/160-sample frames;
    /// any leftover tail shorter than one frame is carried to the next call.
    pub fn process_capture(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        self.capture_pending.extend(samples.iter().copied());
        let mut out = Vec::with_capacity(samples.len());

        while self.capture_pending.len() >= FRAME_SIZE {
            let mic_frame: Vec<f32> = self.capture_pending.drain(0..FRAME_SIZE).collect();

            // Register whatever far-end audio is queued for this tick before
            // cancelling — if nothing is playing, there's no echo to model,
            // so we simply skip the render call for this frame.
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
        let out = aec.process_capture(&mic).expect("process_capture with render queued");
        assert_eq!(out.len(), 1600);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn leftover_tail_shorter_than_one_frame_is_carried_over() {
        let mut aec = SelfEchoCanceller::new();
        let first = vec![0.0f32; 100]; // < FRAME_SIZE (160)
        let out1 = aec.process_capture(&first).expect("first call");
        assert!(out1.is_empty(), "partial frame should not emit output yet");

        let second = vec![0.0f32; 100];
        let out2 = aec.process_capture(&second).expect("second call");
        assert_eq!(out2.len(), FRAME_SIZE, "combined 200 samples should flush one 160-frame");
    }
}
