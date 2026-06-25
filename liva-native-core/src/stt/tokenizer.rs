#![allow(dead_code)]

use std::path::Path;
use tokenizers::Tokenizer;

pub struct SttTokenizer {
    tokenizer: Tokenizer,
    blank_id: u32,
}

impl SttTokenizer {
    pub fn load<P: AsRef<Path>>(model_dir: P) -> Result<Self, String> {
        let tokenizer_path = model_dir.as_ref().join("tokenizer.json");
        if !tokenizer_path.exists() {
            return Err(format!(
                "tokenizer.json not found in {:?}",
                model_dir.as_ref()
            ));
        }

        let tokenizer = Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| format!("Failed to load tokenizer: {}", e))?;

        // Standard blank ID for Nemotron is 13087
        let blank_id = 13087;

        Ok(Self {
            tokenizer,
            blank_id,
        })
    }

    pub fn decode(&self, ids: &[u32]) -> Result<String, String> {
        // Filter out blank token
        let filtered_ids: Vec<u32> = ids
            .iter()
            .cloned()
            .filter(|&id| id != self.blank_id)
            .collect();

        if filtered_ids.is_empty() {
            return Ok(String::new());
        }

        // Decode using tokenizers crate
        let decoded = self
            .tokenizer
            .decode(&filtered_ids, true)
            .map_err(|e| format!("Failed to decode tokens: {}", e))?;

        // Replaces SentencePiece space symbol if present (e.g. U+2581 or custom ▁)
        let decoded = decoded.replace("▁", " ").replace(" ", " ");

        // Normalize spaces
        let normalized = decoded.split_whitespace().collect::<Vec<&str>>().join(" ");

        Ok(normalized)
    }

    #[allow(dead_code)]
    pub fn blank_id(&self) -> u32 {
        self.blank_id
    }
}
