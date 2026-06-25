#![allow(dead_code)]

pub mod dsp;
pub mod engine;
pub mod tokenizer;

use dsp::SttDsp;
use engine::SttEngine;
use std::path::{Path, PathBuf};
use tokenizer::SttTokenizer;

pub struct SttManager {
    pub model_dir: PathBuf,
    engine: Option<SttEngine>,
    tokenizer: Option<SttTokenizer>,
    dsp: SttDsp,

    // Audio stream state
    residual_samples: Vec<f32>,
    prev_sample: f32,
    accumulated_tokens: Vec<u32>,
    has_run_encoder: bool,
    is_streaming: bool,
}

impl SttManager {
    pub fn new<P: AsRef<Path>>(model_dir: P) -> Self {
        let dsp = SttDsp::new(
            512,            // fft_size
            400,            // win_length
            160,            // hop_length
            128,            // num_mels
            16000.0,        // sample_rate
            5.96046448e-08, // log_eps
        );

        Self {
            model_dir: model_dir.as_ref().to_path_buf(),
            engine: None,
            tokenizer: None,
            dsp,
            residual_samples: Vec::new(),
            prev_sample: 0.0,
            accumulated_tokens: Vec::new(),
            has_run_encoder: false,
            is_streaming: false,
        }
    }

    pub fn init(&mut self) -> Result<(), String> {
        if self.engine.is_none() {
            let engine = SttEngine::new(&self.model_dir)?;
            let tokenizer = SttTokenizer::load(&self.model_dir)?;
            self.engine = Some(engine);
            self.tokenizer = Some(tokenizer);
        }
        self.reset_stream();
        Ok(())
    }

    pub fn reset_stream(&mut self) {
        if let Some(ref mut eng) = self.engine {
            eng.reset_states();
        }
        self.residual_samples.clear();
        self.prev_sample = 0.0;
        self.accumulated_tokens.clear();
        self.has_run_encoder = false;
        self.is_streaming = false;
    }

    pub fn feed_audio(&mut self, audio: &[f32], is_last: bool) -> Result<Option<String>, String> {
        if is_last {
            if !self.is_streaming {
                self.reset_stream();
            }
        } else {
            self.is_streaming = true;
        }

        if self.engine.is_none() {
            self.init()?;
        }

        let engine = self.engine.as_mut().unwrap();
        let tokenizer = self.tokenizer.as_ref().unwrap();

        // 1. Pre-emphasis filter in-place or copied
        let mut preemphed = vec![0.0; audio.len()];
        for i in 0..audio.len() {
            preemphed[i] = audio[i] - 0.97 * self.prev_sample;
            self.prev_sample = audio[i];
        }

        // 2. Append to residual buffer
        self.residual_samples.extend_from_slice(&preemphed);

        let mut decoded_any = false;

        // 3. Process sliding window (10,640 samples, hop 8,960)
        while self.residual_samples.len() >= 10640 {
            let slice = &self.residual_samples[0..10640];
            let log_mel = self.dsp.compute_log_mel_spectrogram(slice)?;

            // Run encoder/joint ASR step
            let new_tokens = engine.run_chunk(&log_mel, 65)?;
            self.has_run_encoder = true;

            if !new_tokens.is_empty() {
                self.accumulated_tokens.extend(new_tokens);
                decoded_any = true;
            }

            // Shift buffer by step size (8,960 samples)
            self.residual_samples.drain(0..8960);
        }

        // 4. Handle end of stream (is_last)
        if is_last {
            // If there's enough leftover samples (overlap 1680) or if we haven't run encoder at all yet
            if self.residual_samples.len() > 1680
                || (!self.has_run_encoder && !self.residual_samples.is_empty())
            {
                let mut padded = vec![0.0; 10640];
                let len = self.residual_samples.len().min(10640);
                padded[..len].copy_from_slice(&self.residual_samples[..len]);

                let log_mel = self.dsp.compute_log_mel_spectrogram(&padded)?;
                let new_tokens = engine.run_chunk(&log_mel, 65)?;
                self.accumulated_tokens.extend(new_tokens);
            }

            // Decode the final sequence
            let final_text = tokenizer.decode(&self.accumulated_tokens)?;
            self.reset_stream();
            return Ok(Some(final_text));
        }

        if decoded_any {
            let partial_text = tokenizer.decode(&self.accumulated_tokens)?;
            Ok(Some(partial_text))
        } else {
            Ok(None)
        }
    }
}
