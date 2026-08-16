pub mod anti_hallucination;
pub mod dsp;
pub mod engine;
pub mod lang;
pub mod parakeet;
pub mod tokenizer;

use dsp::SttDsp;
use engine::SttEngine;
use parakeet::ParakeetVi;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Instant;
use tokenizer::SttTokenizer;

/// Streaming transcript output containing partial or final text and acoustic metrics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StreamingTranscript {
    pub partial_text: String,
    pub is_final: bool,
    pub confidence: f32,
    pub latency_ms: u64,
}

pub(crate) fn prefers_parakeet_vi(configured_engine: Option<&str>) -> bool {
    match configured_engine.map(str::trim) {
        None | Some("") => true,
        Some(engine) => engine.eq_ignore_ascii_case("parakeet"),
    }
}

pub trait ParakeetRecognizer: Send {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String>;
    fn feed_chunk(
        &mut self,
        chunk: &[f32],
        is_last: bool,
    ) -> Result<Option<StreamingTranscript>, String>;
    fn reset_stream(&mut self);
}

impl ParakeetRecognizer for ParakeetVi {
    fn transcribe(&mut self, samples: &[f32]) -> Result<String, String> {
        ParakeetVi::transcribe(self, samples)
    }

    fn feed_chunk(
        &mut self,
        chunk: &[f32],
        is_last: bool,
    ) -> Result<Option<StreamingTranscript>, String> {
        ParakeetVi::feed_chunk(self, chunk, is_last)
    }

    fn reset_stream(&mut self) {
        ParakeetVi::reset_stream(self);
    }
}

#[cfg(test)]
struct LoadedParakeetTestDouble;

#[cfg(test)]
impl ParakeetRecognizer for LoadedParakeetTestDouble {
    fn transcribe(&mut self, _samples: &[f32]) -> Result<String, String> {
        Ok(String::new())
    }

    fn feed_chunk(
        &mut self,
        _chunk: &[f32],
        is_last: bool,
    ) -> Result<Option<StreamingTranscript>, String> {
        if is_last {
            Ok(Some(StreamingTranscript {
                partial_text: String::new(),
                is_final: true,
                confidence: 1.0,
                latency_ms: 0,
            }))
        } else {
            Ok(None)
        }
    }

    fn reset_stream(&mut self) {}
}

pub struct SttManager {
    pub model_dir: PathBuf,
    engine: Option<SttEngine>,
    tokenizer: Option<SttTokenizer>,
    dsp: SttDsp,
    language: String,

    // Default high-accuracy Vietnamese engine (Parakeet-CTC), opt-out
    // via `LIVA_STT_VI_ENGINE=nemotron`. Supports both Overlapping Chunked
    // Streaming and whole-utterance batch transcription.
    parakeet: Option<Box<dyn ParakeetRecognizer>>,
    use_parakeet_vi: bool,
    parakeet_fallback_reason: Option<String>,
    raw_audio_buffer: Vec<f32>,

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
            5.960_464_5e-8, // log_eps
        );

        let configured_vi_engine = std::env::var("LIVA_STT_VI_ENGINE").ok();
        let use_parakeet_vi = prefers_parakeet_vi(configured_vi_engine.as_deref());

        Self {
            model_dir: model_dir.as_ref().to_path_buf(),
            engine: None,
            tokenizer: None,
            dsp,
            language: std::env::var("LIVA_STT_LANGUAGE")
                .unwrap_or_else(|_| lang::DEFAULT_LANGUAGE.to_string()),
            parakeet: None,
            use_parakeet_vi,
            parakeet_fallback_reason: None,
            raw_audio_buffer: Vec::new(),
            residual_samples: Vec::new(),
            prev_sample: 0.0,
            accumulated_tokens: Vec::new(),
            has_run_encoder: false,
            is_streaming: false,
        }
    }

    pub fn init(&mut self) -> Result<(), String> {
        if self.engine.is_none() {
            let mut engine = SttEngine::new(&self.model_dir)?;
            match lang::lang_id_for(&self.language) {
                Some(id) => engine.set_lang_id(id),
                None => {
                    tracing::warn!(
                        "Unsupported STT language '{}', falling back to '{}'",
                        self.language,
                        lang::DEFAULT_LANGUAGE
                    );
                    self.language = lang::DEFAULT_LANGUAGE.to_string();
                }
            }
            let tokenizer = SttTokenizer::load(&self.model_dir)?;
            self.engine = Some(engine);
            self.tokenizer = Some(tokenizer);
        }
        self.reset_stream();
        Ok(())
    }

    /// Configured to prefer Parakeet AND the active language is Vietnamese.
    fn should_use_parakeet(&self) -> bool {
        self.use_parakeet_vi && {
            let norm = self.language.trim().to_lowercase();
            norm == "vi" || norm.starts_with("vi-") || norm.starts_with("vi_")
        }
    }

    /// Lazily load the Parakeet model on first use. Returns whether it is ready.
    fn ensure_parakeet_loaded(&mut self) -> bool {
        if self.parakeet.is_some() {
            return true;
        }
        if !self.use_parakeet_vi {
            return false;
        }
        let model_path = std::env::var("LIVA_PARAKEET_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("models/parakeet_vi.onnx"));
        let vocab_path = std::env::var("LIVA_PARAKEET_VOCAB_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| model_path.with_file_name("parakeet_vi_vocab.json"));
        match ParakeetVi::load(&model_path, &vocab_path) {
            Ok(pk) => {
                tracing::info!("Parakeet-CTC vi STT loaded from {:?}", model_path);
                self.parakeet = Some(Box::new(pk));
                self.parakeet_fallback_reason = None;
                true
            }
            Err(e) => {
                tracing::warn!(
                    "Parakeet-CTC vi disabled ({}); falling back to Nemotron for Vietnamese",
                    e
                );
                self.use_parakeet_vi = false;
                self.parakeet_fallback_reason = Some(e);
                false
            }
        }
    }

    /// Vietnamese engine currently ready.
    pub fn active_vietnamese_engine(&self) -> (&'static str, Option<&str>) {
        if self.parakeet.is_some() {
            ("Parakeet-vi", None)
        } else if self.use_parakeet_vi && self.parakeet_fallback_reason.is_none() {
            ("chưa xác định", None)
        } else {
            ("Nemotron", self.parakeet_fallback_reason.as_deref())
        }
    }

    #[cfg(test)]
    pub(crate) fn record_parakeet_pending_for_test(&mut self) {
        self.use_parakeet_vi = true;
        self.parakeet = None;
        self.parakeet_fallback_reason = None;
    }

    #[cfg(test)]
    pub(crate) fn record_parakeet_fallback_for_test(&mut self, reason: &str) {
        self.use_parakeet_vi = false;
        self.parakeet_fallback_reason = Some(reason.to_string());
    }

    #[cfg(test)]
    pub(crate) fn record_parakeet_loaded_for_test(&mut self) {
        self.parakeet = Some(Box::new(LoadedParakeetTestDouble));
        self.parakeet_fallback_reason = None;
    }

    /// Switch the recognition language ("vi", "en", "vi-VN", …).
    pub fn set_language(&mut self, code: &str) -> Result<(), String> {
        let id =
            lang::lang_id_for(code).ok_or_else(|| format!("Unsupported STT language: {}", code))?;
        self.language = code.to_string();
        if let Some(ref mut eng) = self.engine {
            eng.set_lang_id(id);
        }
        self.reset_stream();
        Ok(())
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Diagnostic-only: force a raw encoder lang_id.
    pub fn set_lang_id_raw(&mut self, id: i64) {
        if let Some(ref mut eng) = self.engine {
            eng.set_lang_id(id);
        }
    }

    pub fn reset_stream(&mut self) {
        if let Some(ref mut eng) = self.engine {
            eng.reset_states();
        }
        if let Some(ref mut pk) = self.parakeet {
            pk.reset_stream();
        }
        self.residual_samples.clear();
        self.raw_audio_buffer.clear();
        self.prev_sample = 0.0;
        self.accumulated_tokens.clear();
        self.has_run_encoder = false;
        self.is_streaming = false;
    }

    /// Process streaming audio frame and return a structured `StreamingTranscript`.
    pub fn feed_chunk(
        &mut self,
        pcm_chunk: &[f32],
        is_last: bool,
    ) -> Result<Option<StreamingTranscript>, String> {
        if self.should_use_parakeet() && self.ensure_parakeet_loaded() {
            return self
                .parakeet
                .as_mut()
                .unwrap()
                .feed_chunk(pcm_chunk, is_last);
        }

        let start = Instant::now();
        let text_opt = self.feed_audio_inner(pcm_chunk, is_last, false)?;
        let latency_ms = start.elapsed().as_millis() as u64;

        Ok(text_opt.map(|partial_text| StreamingTranscript {
            partial_text,
            is_final: is_last,
            confidence: 0.90,
            latency_ms,
        }))
    }

    pub fn feed_audio(&mut self, audio: &[f32], is_last: bool) -> Result<Option<String>, String> {
        self.feed_audio_inner(audio, is_last, true)
    }

    /// Transcribe a full utterance for **wake-word** detection. Always uses the
    /// lightweight streaming Nemotron engine.
    pub fn transcribe_for_wake(&mut self, audio: &[f32]) -> Result<Option<String>, String> {
        self.feed_audio_inner(audio, true, false)
    }

    fn feed_audio_inner(
        &mut self,
        audio: &[f32],
        is_last: bool,
        allow_parakeet: bool,
    ) -> Result<Option<String>, String> {
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

        // Parakeet-CTC path for Vietnamese: streaming chunk or whole-utterance decoding
        if allow_parakeet && self.should_use_parakeet() && self.ensure_parakeet_loaded() {
            let res = self.parakeet.as_mut().unwrap().feed_chunk(audio, is_last)?;
            if is_last {
                self.reset_stream();
            }
            return Ok(res.map(|t| t.partial_text));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_transcript_struct_serialization() {
        let transcript = StreamingTranscript {
            partial_text: "xin chào liva".to_string(),
            is_final: false,
            confidence: 0.94,
            latency_ms: 110,
        };

        let json = serde_json::to_string(&transcript).unwrap();
        let deserialized: StreamingTranscript = serde_json::from_str(&json).unwrap();
        assert_eq!(transcript, deserialized);
        assert!(!deserialized.is_final);
        assert_eq!(deserialized.latency_ms, 110);
    }

    #[test]
    fn stt_manager_feed_chunk_with_loaded_double() {
        let mut stt = SttManager::new("models/nemotron-asr");
        stt.set_language("vi").unwrap();
        stt.record_parakeet_loaded_for_test();

        let chunk = vec![0.0f32; 2560];
        // Intermediate chunk
        let res1 = stt.feed_chunk(&chunk, false).unwrap();
        assert!(res1.is_none());

        // Final chunk
        let res2 = stt.feed_chunk(&chunk, true).unwrap();
        assert!(res2.is_some());
        let transcript = res2.unwrap();
        assert!(transcript.is_final);
    }
}
