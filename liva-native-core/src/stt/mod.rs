pub mod dsp;
pub mod engine;
pub mod lang;
pub mod parakeet;
pub mod tokenizer;

use dsp::SttDsp;
use engine::SttEngine;
use parakeet::ParakeetVi;
use std::path::{Path, PathBuf};
use tokenizer::SttTokenizer;

pub struct SttManager {
    pub model_dir: PathBuf,
    engine: Option<SttEngine>,
    tokenizer: Option<SttTokenizer>,
    dsp: SttDsp,
    language: String,

    // Optional high-accuracy Vietnamese offline engine (Parakeet-CTC), opt-in
    // via `LIVA_STT_VI_ENGINE=parakeet`. Used only for whole-utterance
    // transcription when the active language is Vietnamese; the Nemotron
    // streaming path below is left entirely untouched.
    parakeet: Option<ParakeetVi>,
    use_parakeet_vi: bool,
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

        let use_parakeet_vi = std::env::var("LIVA_STT_VI_ENGINE")
            .map(|v| v.trim().eq_ignore_ascii_case("parakeet"))
            .unwrap_or(false);

        Self {
            model_dir: model_dir.as_ref().to_path_buf(),
            engine: None,
            tokenizer: None,
            dsp,
            language: std::env::var("LIVA_STT_LANGUAGE")
                .unwrap_or_else(|_| lang::DEFAULT_LANGUAGE.to_string()),
            parakeet: None,
            use_parakeet_vi,
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
        // Parakeet is loaded lazily on the first Vietnamese utterance
        // (`ensure_parakeet_loaded`) so the 2.4GB model never sits in RAM for
        // English-only or Nemotron-only sessions.
        self.reset_stream();
        Ok(())
    }

    /// Configured to prefer Parakeet AND the active language is Vietnamese.
    /// This does not imply the model is loaded yet — see [`Self::ensure_parakeet_loaded`].
    fn should_use_parakeet(&self) -> bool {
        self.use_parakeet_vi && {
            let norm = self.language.trim().to_lowercase();
            norm == "vi" || norm.starts_with("vi-") || norm.starts_with("vi_")
        }
    }

    /// Lazily load the Parakeet model on first use. Returns whether it is ready.
    /// A hard failure (missing weights) disables the engine for the rest of the
    /// process so we don't re-attempt a doomed 2.4GB load on every utterance.
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
                self.parakeet = Some(pk);
                true
            }
            Err(e) => {
                tracing::warn!(
                    "Parakeet-CTC vi disabled ({}); falling back to Nemotron for Vietnamese",
                    e
                );
                self.use_parakeet_vi = false;
                false
            }
        }
    }

    /// Switch the recognition language ("vi", "en", "vi-VN", …).
    /// Resets any in-flight stream; takes effect immediately.
    pub fn set_language(&mut self, code: &str) -> Result<(), String> {
        let id = lang::lang_id_for(code)
            .ok_or_else(|| format!("Unsupported STT language: {}", code))?;
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

    /// Diagnostic-only: force a raw encoder lang_id (used by `stt_lang_probe`).
    pub fn set_lang_id_raw(&mut self, id: i64) {
        if let Some(ref mut eng) = self.engine {
            eng.set_lang_id(id);
        }
    }

    pub fn reset_stream(&mut self) {
        if let Some(ref mut eng) = self.engine {
            eng.reset_states();
        }
        self.residual_samples.clear();
        self.raw_audio_buffer.clear();
        self.prev_sample = 0.0;
        self.accumulated_tokens.clear();
        self.has_run_encoder = false;
        self.is_streaming = false;
    }

    pub fn feed_audio(&mut self, audio: &[f32], is_last: bool) -> Result<Option<String>, String> {
        self.feed_audio_inner(audio, is_last, true)
    }

    /// Transcribe a full utterance for **wake-word** detection. Always uses the
    /// lightweight streaming Nemotron engine even when Parakeet is the
    /// configured command engine, so the always-listening asleep state never
    /// pays the 2.4GB offline model's load/inference cost on every speech
    /// segment. Accuracy for a single "liva" prefix is ample from Nemotron;
    /// Parakeet is reserved for the actual command after wake.
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

        // Offline Parakeet-CTC path for Vietnamese: accumulate the raw (never
        // pre-emphasized) utterance and transcribe it whole at VAD-end. CTC is
        // not causal, so we emit no streaming partials in this mode — callers
        // that pass the full utterance in one `is_last` call (Telegram, WebRTC
        // on_vad_end, post-wake command) get the full high-accuracy transcript.
        if allow_parakeet && self.should_use_parakeet() && self.ensure_parakeet_loaded() {
            self.raw_audio_buffer.extend_from_slice(audio);
            if !is_last {
                return Ok(None);
            }
            let buffer = std::mem::take(&mut self.raw_audio_buffer);
            let text = self.parakeet.as_mut().unwrap().transcribe(&buffer)?;
            self.reset_stream();
            return Ok(Some(text));
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
