#![allow(dead_code)]

pub mod audio;
pub mod engine;
pub mod espeak;
pub mod g2p;
pub mod normalizer;
pub mod piper;
pub mod tokenizer;
pub mod vieneu;

use audio::TtsAudioPlayer;
use engine::TtsEngine;
use g2p::G2p;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizer::TtsTokenizer;

pub mod style_vector;

pub struct TtsChunker {
    buffer: String,
}

impl TtsChunker {
    pub fn new() -> Self {
        Self {
            buffer: String::new(),
        }
    }

    pub fn push(&mut self, text: &str) -> Vec<String> {
        self.buffer.push_str(text);
        let mut chunks = Vec::new();

        while !self.buffer.is_empty() {
            let mut split_at = None;
            let mut word_count = 0;
            let mut word_start = false;

            for (idx, ch) in self.buffer.char_indices() {
                if ch.is_whitespace() {
                    word_start = false;
                } else if !word_start {
                    word_start = true;
                    word_count += 1;
                }

                // Terminal punctuation always splits
                if ch == '.' || ch == '!' || ch == '?' {
                    split_at = Some((idx + ch.len_utf8(), true));
                    break;
                }

                // Comma-like punctuation splits only if we have >= 6 words
                if ch == ',' || ch == ';' || ch == ':' || ch == '—' {
                    if word_count >= 6 {
                        split_at = Some((idx + ch.len_utf8(), false));
                        break;
                    }
                }

                // 25-word maximum limit
                if word_count > 25 {
                    split_at = Some((idx, true));
                    break;
                }
            }

            if let Some((split_idx, _)) = split_at {
                let chunk: String = self.buffer.drain(0..split_idx).collect();
                let trimmed = chunk.trim().to_string();
                if !trimmed.is_empty() {
                    chunks.push(trimmed);
                }
            } else {
                break;
            }
        }

        chunks
    }

    pub fn flush(&mut self) -> Option<String> {
        let trimmed = self.buffer.trim().to_string();
        self.buffer.clear();
        if !trimmed.is_empty() {
            Some(trimmed)
        } else {
            None
        }
    }

    pub fn reset(&mut self) {
        self.buffer.clear();
    }
}

/// True when the text contains Vietnamese-specific letters — used to route a
/// chunk to the Vietnamese voice even mid-session (LLM replies can mix).
pub fn is_vietnamese_text(text: &str) -> bool {
    const VI_CHARS: &str = "ăâđêôơưàảãáạằẳẵắặầẩẫấậèẻẽéẹềểễếệìỉĩíịòỏõóọồổỗốộờởỡớợùủũúụừửữứựỳỷỹýỵ";
    text.chars()
        .any(|c| c.to_lowercase().any(|lc| VI_CHARS.contains(lc)))
}

pub struct TtsManager {
    pub engine: Arc<Mutex<TtsEngine>>,
    pub tokenizer: TtsTokenizer,
    pub player: TtsAudioPlayer,
    pub chunker: TtsChunker,
    /// Session TTS language ("vi" | "en", default from LIVA_TTS_LANGUAGE).
    language: String,
    piper_vi: Option<Arc<Mutex<piper::PiperVoice>>>,
    piper_en: Option<Arc<Mutex<piper::PiperVoice>>>,
    /// Optional premium tier: the bilingual VieNeu-TTS engine. Opt-in via
    /// `LIVA_TTS_VIENEU=1`; `None` (default) keeps the Piper/Kokoro path.
    vieneu: Option<Arc<Mutex<vieneu::VieNeuVoice>>>,
}

impl TtsManager {
    pub fn new<P: AsRef<Path>>(
        model_path: P,
        voice_data: Vec<f32>,
        sink: Option<Arc<rodio::Sink>>,
    ) -> Result<Self, String> {
        let engine = TtsEngine::new(model_path, voice_data)?;
        let tokenizer = TtsTokenizer::new();
        let player = TtsAudioPlayer::new(sink);
        let chunker = TtsChunker::new();

        let piper_dir =
            std::env::var("LIVA_TTS_PIPER_DIR").unwrap_or_else(|_| "models/piper".to_string());
        let (piper_vi, piper_en) = Self::load_piper_voices(&piper_dir);
        let language =
            std::env::var("LIVA_TTS_LANGUAGE").unwrap_or_else(|_| "vi".to_string());
        let vieneu = Self::load_vieneu();

        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            tokenizer,
            player,
            chunker,
            language,
            piper_vi,
            piper_en,
            vieneu,
        })
    }

    /// Load the premium VieNeu-TTS engine when `LIVA_TTS_VIENEU` is truthy.
    /// Heavy (~500 MB, ~2 s) so it's opt-in; any failure logs and falls back to
    /// the Piper/Kokoro path (returns `None`). Model dir from
    /// `LIVA_VIENEU_MODEL_DIR` (default `models/vieneu`), voice from
    /// `LIVA_VIENEU_VOICE` (default: the file's `default_voice`).
    fn load_vieneu() -> Option<Arc<Mutex<vieneu::VieNeuVoice>>> {
        // KHÔNG dùng `crate::env_flag` ở đây: file này được 3 bin
        // (verify_round2, voice_profile, voice_stress) include qua `#[path]`
        // nên `crate::` trỏ về bin chứ không phải lib, và mọi tham chiếu
        // `crate::` sẽ làm chúng không biên dịch được. Thống nhất được với
        // `env_flag` chỉ sau khi các bin đó chuyển sang `use liva_native_core::`.
        let enabled = std::env::var("LIVA_TTS_VIENEU")
            .map(|v| {
                matches!(
                    v.trim().to_ascii_lowercase().as_str(),
                    "1" | "true" | "yes" | "on"
                )
            })
            .unwrap_or(false);
        if !enabled {
            return None;
        }
        let rel = std::env::var("LIVA_VIENEU_MODEL_DIR")
            .unwrap_or_else(|_| "models/vieneu".to_string());
        // Resolve the repo-relative model dir against the real project root
        // (cwd differs per entry point). Kept self-contained so this compiles
        // both in the lib and in bins that include this module via `#[path]`.
        let raw = std::path::PathBuf::from(&rel);
        let dir = if raw.is_absolute() {
            raw
        } else {
            ["", "..", "../.."]
                .iter()
                .map(|p| std::path::Path::new(p).join(&raw))
                .find(|c| c.join("config.json").exists())
                .unwrap_or(raw)
        };
        let voice = std::env::var("LIVA_VIENEU_VOICE").ok();
        match vieneu::VieNeuVoice::load(&dir, voice.as_deref()) {
            Ok(v) => {
                tracing::info!("VieNeu-TTS premium tier enabled (voice '{}')", v.voice_name());
                Some(Arc::new(Mutex::new(v)))
            }
            Err(e) => {
                tracing::error!("VieNeu-TTS enabled but failed to load ({}); using Piper", e);
                None
            }
        }
    }

    /// Scan a directory for Piper voices: first `vi*.onnx` → Vietnamese slot,
    /// first `en*.onnx` → English slot. Missing voices are non-fatal (Kokoro
    /// stays as the English fallback).
    fn load_piper_voices(
        dir: &str,
    ) -> (
        Option<Arc<Mutex<piper::PiperVoice>>>,
        Option<Arc<Mutex<piper::PiperVoice>>>,
    ) {
        let mut dir_path = std::path::PathBuf::from(dir);
        if !dir_path.exists() {
            let alt = std::path::Path::new("..").join(dir);
            if alt.exists() {
                dir_path = alt;
            }
        }
        let Ok(entries) = std::fs::read_dir(&dir_path) else {
            tracing::warn!(
                "Piper voice dir {:?} not found — TTS falls back to Kokoro (EN only)",
                dir_path
            );
            return (None, None);
        };

        let mut files: Vec<_> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|x| x == "onnx"))
            .collect();
        files.sort();

        let mut vi = None;
        let mut en = None;
        for f in files {
            let name = f
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let slot = if name.starts_with("vi") {
                &mut vi
            } else if name.starts_with("en") {
                &mut en
            } else {
                continue;
            };
            if slot.is_some() {
                continue;
            }
            match piper::PiperVoice::load(&f) {
                Ok(v) => {
                    tracing::info!("Loaded Piper voice {:?} ({} Hz)", f, v.sample_rate());
                    *slot = Some(Arc::new(Mutex::new(v)));
                }
                Err(e) => tracing::warn!("Failed to load Piper voice {:?}: {}", f, e),
            }
        }
        (vi, en)
    }

    /// Switch the session TTS language ("vi" | "en").
    pub fn set_language(&mut self, code: &str) {
        self.language = code.trim().to_lowercase();
    }

    pub fn language(&self) -> &str {
        &self.language
    }

    /// Pick the Piper voice for a text chunk: Vietnamese letters force the vi
    /// voice, otherwise the session language decides, with cross-language
    /// fallback. `None` → caller should use the Kokoro (EN) path. Shared by
    /// local playback and the duplex streaming pipeline.
    pub fn piper_for_chunk(&self, chunk: &str) -> Option<Arc<Mutex<piper::PiperVoice>>> {
        let lang = if is_vietnamese_text(chunk) {
            "vi"
        } else {
            self.language.as_str()
        };
        if lang.starts_with("vi") {
            self.piper_vi.clone().or_else(|| self.piper_en.clone())
        } else {
            self.piper_en.clone().or_else(|| self.piper_vi.clone())
        }
    }

    /// The premium VieNeu engine for a chunk when enabled, else `None`. VieNeu
    /// is bilingual (one model handles vi+en via its own phonemizer), so the
    /// chunk text isn't used for selection — it's the preferred engine for every
    /// chunk when loaded. Callers fall back to [`Self::piper_for_chunk`].
    pub fn vieneu_for_chunk(&self, _chunk: &str) -> Option<Arc<Mutex<vieneu::VieNeuVoice>>> {
        self.vieneu.clone()
    }

    pub fn from_bin<P: AsRef<Path>>(
        model_path: P,
        bin_path: P,
        sink: Option<Arc<rodio::Sink>>,
    ) -> Result<Self, String> {
        let voice_bytes = std::fs::read(bin_path.as_ref()).map_err(|e| e.to_string())?;
        let len_rounded = (voice_bytes.len() / 4) * 4;
        let voice_bytes_aligned = &voice_bytes[..len_rounded];
        #[allow(clippy::manual_is_multiple_of)]
        let voice_data = if voice_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0 {
            bytemuck::cast_slice(voice_bytes_aligned).to_vec()
        } else {
            voice_bytes_aligned
                .chunks_exact(4)
                .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                .collect()
        };
        Self::new(model_path, voice_data, sink)
    }

    pub fn from_wav<P: AsRef<Path>>(
        model_path: P,
        reference_wav: P,
        sink: Option<Arc<rodio::Sink>>,
    ) -> Result<Self, String> {
        let file = std::fs::File::open(reference_wav.as_ref()).map_err(|e| e.to_string())?;
        let decoder = rodio::Decoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;
        
        let mut audio_data = Vec::new();
        for sample in decoder {
            audio_data.push(sample as f32 / 32768.0);
        }
        
        let style = style_vector::extract_style_vector(&audio_data);
        Self::new(model_path, style, sink)
    }

    pub async fn speak(&mut self, text: &str) -> Result<(), String> {
        let chunks = self.chunker.push(text);
        let mut current_stop_id = self.player.get_stop_id();
        for chunk in chunks {
            if self.player.get_stop_id() != current_stop_id {
                break;
            }
            current_stop_id = self.process_chunk(&chunk, current_stop_id).await?;
        }
        Ok(())
    }

    pub async fn flush(&mut self) -> Result<(), String> {
        if let Some(remainder) = self.chunker.flush() {
            let current_stop_id = self.player.get_stop_id();
            self.process_chunk(&remainder, current_stop_id).await?;
        }
        Ok(())
    }

    pub async fn stop(&mut self) {
        self.player.stop().await;
        self.chunker.reset();
    }

    pub fn check_idle_unload(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            // Unload if idle for 5 minutes (300 seconds)
            engine.check_idle_unload(std::time::Duration::from_secs(300));
        }
    }

    async fn process_chunk(&self, chunk: &str, initial_stop_id: usize) -> Result<usize, String> {
        let cleaned_chunk = chunk.replace(|c: char| c == '[' || c == ']', "");
        if cleaned_chunk.trim().is_empty() {
            return Ok(initial_stop_id);
        }

        // Normalize digits/dates/currency into words for the chunk language
        // before any synthesis (Vietnamese TTS reads "5.000đ" correctly).
        let norm_lang = if is_vietnamese_text(&cleaned_chunk) {
            "vi"
        } else {
            self.language.as_str()
        };
        let cleaned_chunk = normalizer::normalize(&cleaned_chunk, norm_lang);

        // Premium tier: VieNeu-TTS (bilingual) takes priority when enabled.
        if let Some(engine) = self.vieneu_for_chunk(&cleaned_chunk) {
            let text = cleaned_chunk.clone();
            let (audio_samples, rate) = tokio::task::spawn_blocking(move || {
                let mut e = engine.lock().unwrap();
                let rate = e.sample_rate();
                e.synthesize(&text).map(|s| (s, rate))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            if self.player.get_stop_id() == initial_stop_id {
                return Ok(self.player.play_with_rate(audio_samples, rate));
            }
            return Ok(initial_stop_id);
        }

        // Local-first routing: Piper voices per language; Kokoro remains the
        // English-only fallback when no Piper voice is available.
        if let Some(voice) = self.piper_for_chunk(&cleaned_chunk) {
            let text = cleaned_chunk.clone();
            let (audio_samples, rate) = tokio::task::spawn_blocking(move || {
                let mut v = voice.lock().unwrap();
                let rate = v.sample_rate();
                v.synthesize(&text).map(|s| (s, rate))
            })
            .await
            .map_err(|e| format!("Blocking task panicked: {}", e))??;

            if self.player.get_stop_id() == initial_stop_id {
                return Ok(self.player.play_with_rate(audio_samples, rate));
            }
            return Ok(initial_stop_id);
        }

        let phonemes = G2p::phonemize(&cleaned_chunk);
        let token_ids = self.tokenizer.tokenize(&phonemes);

        let engine = self.engine.clone();
        let audio_samples = tokio::task::spawn_blocking(move || {
            let (session_arc, voice_data) = {
                let mut eng = engine.lock().unwrap();
                eng.prepare_inference()?
            }; // lock is dropped here!
            
            let mut session = session_arc.lock().unwrap();
            TtsEngine::generate_from_session(&mut session, &voice_data, &token_ids, 1.0)
        })
        .await
        .map_err(|e| format!("Blocking task panicked: {}", e))??;

        if self.player.get_stop_id() == initial_stop_id {
            let new_stop_id = self.player.play(audio_samples);
            Ok(new_stop_id)
        } else {
            Ok(initial_stop_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_chunker_sentence_boundary() {
        let mut chunker = TtsChunker::new();
        let chunks = chunker.push("Hello world. How are you today?");
        assert_eq!(chunks, vec!["Hello world.", "How are you today?"]);
    }

    #[test]
    fn test_chunker_comma_minimum() {
        let mut chunker = TtsChunker::new();

        // 5 words before first comma -> should NOT split on first comma.
        // Second comma makes 10 words -> should split at second comma.
        let chunks = chunker.push("Hello, my name is LIVA, I am your voice assistant.");
        assert_eq!(
            chunks,
            vec!["Hello, my name is LIVA, I am your voice assistant."]
        );

        let mut chunker2 = TtsChunker::new();
        // 7 words before comma -> should split.
        let chunks2 =
            chunker2.push("This is a very long clause right here, and then another clause");
        assert_eq!(chunks2, vec!["This is a very long clause right here,"]);
        let rem = chunker2.flush();
        assert_eq!(rem, Some("and then another clause".to_string()));
    }

    #[test]
    fn test_chunker_maximum_words() {
        let mut chunker = TtsChunker::new();
        // A sentence with 30 words, should split at or before 25th word
        let sentence = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twentyone twentytwo twentythree twentyfour twentyfive twentysix twentyseven twentyeight twentynine thirty";
        let chunks = chunker.push(sentence);
        assert_eq!(chunks.len(), 1);
        let first_chunk_words: Vec<&str> = chunks[0].split_whitespace().collect();
        assert_eq!(first_chunk_words.len(), 25);
    }
}
