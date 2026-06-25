#![allow(dead_code)]

pub mod audio;
pub mod engine;
pub mod g2p;
pub mod tokenizer;

use audio::TtsAudioPlayer;
use engine::TtsEngine;
use g2p::G2p;
use std::path::Path;
use std::sync::{Arc, Mutex};
use tokenizer::TtsTokenizer;

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

pub struct TtsManager {
    pub engine: Arc<Mutex<TtsEngine>>,
    pub tokenizer: TtsTokenizer,
    pub player: TtsAudioPlayer,
    pub chunker: TtsChunker,
}

impl TtsManager {
    pub fn new<P: AsRef<Path>>(
        model_path: P,
        voice_path: P,
        sink: Option<Arc<rodio::Sink>>,
    ) -> Result<Self, String> {
        let engine = TtsEngine::new(model_path, voice_path)?;
        let tokenizer = TtsTokenizer::new();
        let player = TtsAudioPlayer::new(sink);
        let chunker = TtsChunker::new();

        Ok(Self {
            engine: Arc::new(Mutex::new(engine)),
            tokenizer,
            player,
            chunker,
        })
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

        let phonemes = G2p::phonemize(&cleaned_chunk);
        let token_ids = self.tokenizer.tokenize(&phonemes);

        let engine = self.engine.clone();
        let audio_samples = tokio::task::spawn_blocking(move || {
            let mut eng = engine.lock().unwrap();
            eng.generate(&token_ids, 1.0)
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
