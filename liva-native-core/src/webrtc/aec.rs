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
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use sonora::config::EchoCanceller;
use sonora::{AudioProcessing, Config, StreamConfig};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

const SAMPLE_RATE: u32 = 16000;
const FRAME_SIZE: usize = (SAMPLE_RATE / 100) as usize; // 10ms, Sonora's required frame size

pub struct SelfEchoCanceller {
    apm: AudioProcessing,
    render_queue: VecDeque<f32>,
    capture_pending: VecDeque<f32>,
    tts_queued: usize,
}

impl SelfEchoCanceller {
    /// Bounded render delay queue ceiling: 2 seconds @ 16kHz (32,000 samples).
    /// Prevents delay drift between render reference and mic capture, and eliminates memory leaks.
    pub const MAX_RENDER_QUEUE_SAMPLES: usize = 16_000 * 2;

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
            tts_queued: 0,
        }
    }

    /// Feed audio LIVA is about to play (any sample rate — TTS voices run at
    /// 22050/24000Hz) as the AEC's far-end reference. Linearly resampled to
    /// 16kHz and queued; a following `process_capture` call registers it.
    pub fn push_render(&mut self, samples: &[f32], source_rate: u32) {
        if samples.is_empty() {
            return;
        }
        let mut pushed = 0usize;
        if source_rate == SAMPLE_RATE {
            for &s in samples {
                let sample = if s.is_finite() {
                    s.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                self.render_queue.push_back(sample);
                pushed += 1;
            }
        } else {
            let ratio = SAMPLE_RATE as f64 / source_rate as f64;
            let out_len = ((samples.len() as f64) * ratio).round() as usize;
            for i in 0..out_len {
                let src_pos = i as f64 / ratio;
                let idx = src_pos.floor() as usize;
                let frac = (src_pos - idx as f64) as f32;
                let a = samples[idx.min(samples.len() - 1)];
                let b = samples[(idx + 1).min(samples.len() - 1)];
                let interp = a + (b - a) * frac;
                let sample = if interp.is_finite() {
                    interp.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                self.render_queue.push_back(sample);
                pushed += 1;
            }
        }
        self.tts_queued = self.tts_queued.saturating_add(pushed);

        if self.render_queue.len() > Self::MAX_RENDER_QUEUE_SAMPLES {
            let overflow = self.render_queue.len() - Self::MAX_RENDER_QUEUE_SAMPLES;
            self.render_queue.drain(0..overflow);
            self.tts_queued = self.tts_queued.saturating_sub(overflow);
        }
    }

    /// Feed desktop loopback audio (e.g. from Windows WASAPI loopback capture)
    /// as an additional far-end reference for AEC3. Mixes sample-by-sample with
    /// any pending TTS render audio in the queue, saturation-clamping to [-1.0, 1.0]
    /// to avoid signal distortion in the echo canceller.
    pub fn push_loopback_render(&mut self, samples: &[f32], source_rate: u32) {
        if samples.is_empty() {
            return;
        }
        let resampled: Vec<f32> = if source_rate == SAMPLE_RATE {
            samples
                .iter()
                .map(|&s| {
                    if s.is_finite() {
                        s.clamp(-1.0, 1.0)
                    } else {
                        0.0
                    }
                })
                .collect()
        } else {
            let ratio = SAMPLE_RATE as f64 / source_rate as f64;
            let out_len = ((samples.len() as f64) * ratio).round() as usize;
            let mut res = Vec::with_capacity(out_len);
            for i in 0..out_len {
                let src_pos = i as f64 / ratio;
                let idx = src_pos.floor() as usize;
                let frac = (src_pos - idx as f64) as f32;
                let a = samples[idx.min(samples.len() - 1)];
                let b = samples[(idx + 1).min(samples.len() - 1)];
                let interp = a + (b - a) * frac;
                let val = if interp.is_finite() {
                    interp.clamp(-1.0, 1.0)
                } else {
                    0.0
                };
                res.push(val);
            }
            res
        };

        // Mix with pending TTS samples that have not yet had loopback mixed in.
        // Once tts_queued is exhausted, append new loopback samples chronologically to the FIFO queue.
        let mix_budget = self.tts_queued.min(resampled.len());
        let queue_len = self.render_queue.len();
        let mix_start = queue_len.saturating_sub(self.tts_queued);

        for (idx, &sample) in resampled.iter().enumerate() {
            if idx < mix_budget && mix_start + idx < queue_len {
                let mixed = (self.render_queue[mix_start + idx] + sample).clamp(-1.0, 1.0);
                self.render_queue[mix_start + idx] = mixed;
            } else {
                self.render_queue.push_back(sample);
            }
        }
        self.tts_queued = self.tts_queued.saturating_sub(mix_budget);

        if self.render_queue.len() > Self::MAX_RENDER_QUEUE_SAMPLES {
            let overflow = self.render_queue.len() - Self::MAX_RENDER_QUEUE_SAMPLES;
            self.render_queue.drain(0..overflow);
            self.tts_queued = self.tts_queued.saturating_sub(overflow);
        }
    }

    /// Clear all queued far-end render reference audio.
    /// Call this on barge-in / playback cancellation so unplayed audio
    /// does not bleed into the echo canceller during user speech.
    pub fn clear_render(&mut self) {
        self.render_queue.clear();
        self.tts_queued = 0;
    }

    /// Reset both render queue and capture pending buffers.
    pub fn reset(&mut self) {
        self.render_queue.clear();
        self.capture_pending.clear();
        self.tts_queued = 0;
    }

    /// Returns the number of far-end render samples currently queued.
    pub fn render_queue_len(&self) -> usize {
        self.render_queue.len()
    }

    /// Cancel LIVA's own voice out of a 16kHz mono mic buffer of arbitrary
    /// length. Internally reblocks to Sonora's fixed 10ms/160-sample frames;
    /// any leftover tail shorter than one frame is carried to the next call.
    pub fn process_capture(&mut self, samples: &[f32]) -> Result<Vec<f32>, String> {
        for &s in samples {
            let sanitized = if s.is_finite() {
                s.clamp(-1.0, 1.0)
            } else {
                0.0
            };
            self.capture_pending.push_back(sanitized);
        }
        let mut out = Vec::with_capacity(samples.len());

        while self.capture_pending.len() >= FRAME_SIZE {
            let mic_frame: Vec<f32> = self.capture_pending.drain(0..FRAME_SIZE).collect();

            // Register whatever far-end audio is queued for this tick before
            // cancelling — if nothing is playing, there's no echo to model,
            // so we simply skip the render call for this frame.
            if self.render_queue.len() >= FRAME_SIZE {
                let render_frame: Vec<f32> = self.render_queue.drain(0..FRAME_SIZE).collect();
                self.tts_queued = self.tts_queued.saturating_sub(FRAME_SIZE);
                let mut render_out = vec![0.0f32; FRAME_SIZE];
                self.apm
                    .process_render_f32(&[&render_frame], &mut [&mut render_out])
                    .map_err(|e| format!("AEC3 process_render failed: {:?}", e))?;
            }

            let mut capture_out = vec![0.0f32; FRAME_SIZE];
            self.apm
                .process_capture_f32(&[&mic_frame], &mut [&mut capture_out])
                .map_err(|e| format!("AEC3 process_capture failed: {:?}", e))?;

            for sample in &mut capture_out {
                if !sample.is_finite() {
                    *sample = 0.0;
                } else {
                    *sample = sample.clamp(-1.0, 1.0);
                }
            }
            out.extend_from_slice(&capture_out);
        }

        Ok(out)
    }
}

/// Downmix interleaved multi-channel f32 samples to mono normalized [-1.0, 1.0].
fn downmix_f32_to_mono(data: &[f32], channels: usize) -> Vec<f32> {
    if channels <= 1 {
        data.iter()
            .map(|&s| {
                if s.is_finite() {
                    s.clamp(-1.0, 1.0)
                } else {
                    0.0
                }
            })
            .collect()
    } else {
        data.chunks_exact(channels)
            .map(|frame| {
                let sum: f32 = frame
                    .iter()
                    .map(|&s| if s.is_finite() { s } else { 0.0 })
                    .sum();
                (sum / channels as f32).clamp(-1.0, 1.0)
            })
            .collect()
    }
}

/// Downmix interleaved multi-channel i16 samples to mono f32 normalized [-1.0, 1.0].
fn downmix_i16_to_mono(data: &[i16], channels: usize) -> Vec<f32> {
    const NORM: f32 = 32768.0;
    if channels <= 1 {
        data.iter()
            .map(|&s| (s as f32 / NORM).clamp(-1.0, 1.0))
            .collect()
    } else {
        data.chunks_exact(channels)
            .map(|frame| {
                let sum: f32 = frame.iter().map(|&s| s as f32 / NORM).sum();
                (sum / channels as f32).clamp(-1.0, 1.0)
            })
            .collect()
    }
}

/// Wrapper around cpal::Stream to allow Send on Windows WASAPI targets where
/// cpal::Stream contains raw pointers (*mut ()). Access is synchronized via Mutex.
#[allow(dead_code)]
struct SendStream(cpal::Stream);
// SAFETY: The cpal stream is owned within a Mutex and only dropped or taken when synchronized.
unsafe impl Send for SendStream {}

struct WasapiLoopbackInner {
    running: AtomicBool,
    stream: Mutex<Option<SendStream>>,
    device_name: String,
    sample_rate: u32,
    channels: u16,
}

impl Drop for WasapiLoopbackInner {
    fn drop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.stream.lock() {
            let _ = guard.take();
        }
    }
}

/// Active WASAPI loopback audio capturer capturing the Windows default output endpoint.
///
/// On Windows, calling `build_input_stream` on an output device configures WASAPI with
/// `AUDCLNT_STREAMFLAGS_LOOPBACK`. The captured audio (music, games, TTS) is streamed
/// directly into the AEC's far-end reference buffer via `push_loopback_render`.
#[derive(Clone)]
pub struct WasapiLoopbackCapturer {
    inner: Arc<WasapiLoopbackInner>,
}

impl WasapiLoopbackCapturer {
    /// Create a stopped / idle capturer handle (useful for testing or fallback).
    pub fn new() -> Self {
        Self {
            inner: Arc::new(WasapiLoopbackInner {
                running: AtomicBool::new(false),
                stream: Mutex::new(None),
                device_name: "MockLoopback".to_string(),
                sample_rate: 16000,
                channels: 1,
            }),
        }
    }

    /// Discover the default output device and start continuous loopback capture into AEC.
    pub fn start(aec_handle: crate::webrtc::session::SessionAec) -> Result<Self, String> {
        let host = cpal::default_host();
        let device = host
            .default_output_device()
            .ok_or_else(|| "No default audio output device available".to_string())?;

        let device_name = device
            .name()
            .unwrap_or_else(|_| "Default Output Device".to_string());

        let supported_config = device.default_output_config().map_err(|e| {
            format!(
                "Failed to query default output config for '{}': {}",
                device_name, e
            )
        })?;

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels() as usize;
        let sample_format = supported_config.sample_format();
        let stream_config = supported_config.config();

        let running = Arc::new(AtomicBool::new(true));
        let running_clone = Arc::clone(&running);
        let aec_target = Arc::clone(&aec_handle);

        let err_fn = move |err: cpal::StreamError| {
            tracing::warn!("WASAPI loopback stream error: {err}");
        };

        let stream = match sample_format {
            cpal::SampleFormat::F32 => {
                let r = Arc::clone(&running_clone);
                let aec = Arc::clone(&aec_target);
                device.build_input_stream(
                    &stream_config,
                    move |data: &[f32], _: &cpal::InputCallbackInfo| {
                        if !r.load(Ordering::Relaxed) {
                            return;
                        }
                        let mono = downmix_f32_to_mono(data, channels);
                        if let Ok(mut guard) = aec.lock()
                            && let Some(ref mut echo) = *guard
                        {
                            echo.push_loopback_render(&mono, sample_rate);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            cpal::SampleFormat::I16 => {
                let r = Arc::clone(&running_clone);
                let aec = Arc::clone(&aec_target);
                device.build_input_stream(
                    &stream_config,
                    move |data: &[i16], _: &cpal::InputCallbackInfo| {
                        if !r.load(Ordering::Relaxed) {
                            return;
                        }
                        let mono = downmix_i16_to_mono(data, channels);
                        if let Ok(mut guard) = aec.lock()
                            && let Some(ref mut echo) = *guard
                        {
                            echo.push_loopback_render(&mono, sample_rate);
                        }
                    },
                    err_fn,
                    None,
                )
            }
            other => {
                return Err(format!(
                    "Unsupported audio output sample format for loopback: {:?}",
                    other
                ));
            }
        }
        .map_err(|e| format!("Failed to build WASAPI loopback input stream: {}", e))?;

        stream
            .play()
            .map_err(|e| format!("Failed to start WASAPI loopback stream: {}", e))?;

        Ok(Self {
            inner: Arc::new(WasapiLoopbackInner {
                running: AtomicBool::new(true),
                stream: Mutex::new(Some(SendStream(stream))),
                device_name,
                sample_rate,
                channels: channels as u16,
            }),
        })
    }

    pub fn is_running(&self) -> bool {
        self.inner.running.load(Ordering::SeqCst)
    }

    pub fn stop(&self) {
        self.inner.running.store(false, Ordering::SeqCst);
        if let Ok(mut guard) = self.inner.stream.lock() {
            let _ = guard.take();
        }
    }

    pub fn device_name(&self) -> &str {
        &self.inner.device_name
    }

    pub fn sample_rate(&self) -> u32 {
        self.inner.sample_rate
    }

    pub fn channels(&self) -> u16 {
        self.inner.channels
    }
}

impl Default for WasapiLoopbackCapturer {
    fn default() -> Self {
        Self::new()
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
    fn push_loopback_render_mixes_cleanly_with_existing_tts_render() {
        let mut aec = SelfEchoCanceller::new();
        // 1. Push TTS reference audio: 320 samples @ 0.4 amplitude
        let tts_chunk = vec![0.4f32; 320];
        aec.push_render(&tts_chunk, 16000);
        assert_eq!(aec.render_queue.len(), 320);

        // 2. Push desktop loopback audio: 320 samples @ 0.5 amplitude
        let loopback_chunk = vec![0.5f32; 320];
        aec.push_loopback_render(&loopback_chunk, 16000);
        // Queue length remains 320 because samples were mixed
        assert_eq!(aec.render_queue.len(), 320);
        // Mixed value should be 0.4 + 0.5 = 0.9
        assert!((aec.render_queue[0] - 0.9).abs() < 1e-4);

        // 3. Push new TTS + extreme loopback audio to test clamp (-1.0 to 1.0)
        let tts_chunk2 = vec![0.5f32; 160];
        aec.push_render(&tts_chunk2, 16000);
        let extreme_chunk = vec![0.8f32; 160];
        aec.push_loopback_render(&extreme_chunk, 16000);
        assert_eq!(aec.render_queue[320], 1.0, "should clamp to 1.0");

        // 4. Process capture successfully with mixed render stream
        let mic = vec![0.1f32; 320];
        let out = aec
            .process_capture(&mic)
            .expect("process_capture with mixed loopback");
        assert_eq!(out.len(), 320);
        assert!(out.iter().all(|s| s.is_finite()));
    }

    #[test]
    fn wasapi_loopback_capturer_lifecycle() {
        let capturer = WasapiLoopbackCapturer::new();
        assert!(!capturer.is_running());
        capturer.stop();
        assert!(!capturer.is_running());
    }

    #[test]
    fn render_queue_caps_at_max_ceiling_and_discards_oldest() {
        let mut aec = SelfEchoCanceller::new();
        // Push 40,000 samples (exceeding MAX_RENDER_QUEUE_SAMPLES = 32,000)
        let large_chunk = vec![0.3f32; 40_000];
        aec.push_render(&large_chunk, 16000);

        assert_eq!(
            aec.render_queue_len(),
            SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES
        );
        assert_eq!(aec.render_queue.len(), 32_000);
    }

    #[test]
    fn clear_render_and_reset_wipe_internal_buffers() {
        let mut aec = SelfEchoCanceller::new();
        aec.push_render(&[0.2f32; 320], 16000);
        let _ = aec.process_capture(&[0.1f32; 100]); // 100 samples < 160 leaves capture_pending
        assert_eq!(aec.render_queue_len(), 320);
        assert_eq!(aec.capture_pending.len(), 100);

        aec.clear_render();
        assert_eq!(aec.render_queue_len(), 0);
        assert_eq!(aec.capture_pending.len(), 100);

        aec.reset();
        assert_eq!(aec.render_queue_len(), 0);
        assert_eq!(aec.capture_pending.len(), 0);
    }

    #[test]
    fn nan_and_inf_audio_samples_are_sanitized_safely() {
        let mut aec = SelfEchoCanceller::new();
        let corrupted_render = vec![f32::NAN, f32::INFINITY, f32::NEG_INFINITY, 2.5, -3.0];
        aec.push_render(&corrupted_render, 16000);

        // Verify all queued render samples are strictly finite and clamped
        for &sample in &aec.render_queue {
            assert!(sample.is_finite());
            assert!((-1.0..=1.0).contains(&sample));
        }

        // Process corrupted mic samples
        let mut corrupted_mic = vec![0.1f32; 160];
        corrupted_mic[10] = f32::NAN;
        corrupted_mic[20] = f32::INFINITY;

        let out = aec
            .process_capture(&corrupted_mic)
            .expect("process_capture");
        assert_eq!(out.len(), 160);
        for &sample in &out {
            assert!(sample.is_finite());
            assert!((-1.0..=1.0).contains(&sample));
        }
    }

    #[test]
    fn push_loopback_render_continuous_streaming_preserves_fifo_order() {
        let mut aec = SelfEchoCanceller::new();
        // Push 5 successive 160-sample chunks of desktop loopback audio without TTS render
        for i in 0..5 {
            let chunk = vec![0.1 * (i as f32 + 1.0); 160];
            aec.push_loopback_render(&chunk, 16000);
        }
        // Total samples must be 5 * 160 = 800 without overwriting chunk 0!
        assert_eq!(aec.render_queue.len(), 800);
        assert!((aec.render_queue[0] - 0.1).abs() < 1e-4);
        assert!((aec.render_queue[160] - 0.2).abs() < 1e-4);
        assert!((aec.render_queue[320] - 0.3).abs() < 1e-4);
        assert!((aec.render_queue[480] - 0.4).abs() < 1e-4);
        assert!((aec.render_queue[640] - 0.5).abs() < 1e-4);
    }
}
