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

    /// SentencePiece-correct decode: `▁` marks a word boundary, every other
    /// piece concatenates directly onto the previous one. The generic
    /// `Tokenizer::decode` joined ALL pieces with spaces, which shredded
    /// Vietnamese ("Xin chào" → "X in ch à o") since diacritic-bearing text
    /// splits into many sub-word pieces. Also assembles `<0xNN>` byte-fallback
    /// runs and drops control/locale tokens like `<vi-VN>`.
    pub fn decode(&self, ids: &[u32]) -> Result<String, String> {
        let mut out = String::new();
        let mut byte_buf: Vec<u8> = Vec::new();

        for &id in ids {
            if id == self.blank_id {
                continue;
            }
            let Some(tok) = self.tokenizer.id_to_token(id) else {
                continue;
            };

            // Byte-fallback piece: accumulate, flush as UTF-8 on the next
            // non-byte piece so multi-byte characters reassemble correctly.
            if let Some(hex) = tok.strip_prefix("<0x").and_then(|t| t.strip_suffix('>'))
                && let Ok(b) = u8::from_str_radix(hex, 16) {
                    byte_buf.push(b);
                    continue;
                }
            if !byte_buf.is_empty() {
                out.push_str(&String::from_utf8_lossy(&byte_buf));
                byte_buf.clear();
            }

            // Control / locale tokens (<vi-VN>, <pad>, …) are not text.
            if tok.starts_with('<') && tok.ends_with('>') {
                continue;
            }

            if let Some(rest) = tok.strip_prefix('▁') {
                if !out.is_empty() {
                    out.push(' ');
                }
                out.push_str(rest);
            } else {
                out.push_str(&tok);
            }
        }

        if !byte_buf.is_empty() {
            out.push_str(&String::from_utf8_lossy(&byte_buf));
        }

        Ok(out.trim().to_string())
    }

    #[allow(dead_code)]
    pub fn blank_id(&self) -> u32 {
        self.blank_id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trips Vietnamese text through the real model tokenizer when the
    /// model directory is present (skipped otherwise, e.g. in CI).
    #[test]
    fn decode_reassembles_vietnamese_pieces() {
        let model_dir = std::path::Path::new("models/nemotron-asr");
        let model_dir = if model_dir.join("tokenizer.json").exists() {
            model_dir.to_path_buf()
        } else {
            let alt = std::path::Path::new("../models/nemotron-asr");
            if !alt.join("tokenizer.json").exists() {
                eprintln!("skipping: nemotron tokenizer not on disk");
                return;
            }
            alt.to_path_buf()
        };

        let tk = SttTokenizer::load(&model_dir).expect("load tokenizer");
        let text = "Xin chào, tôi là trợ lý ảo Liva";
        let enc = {
            // Encode with the underlying HF tokenizer to obtain piece ids.
            let raw = Tokenizer::from_file(model_dir.join("tokenizer.json")).unwrap();
            raw.encode(text, false).unwrap().get_ids().to_vec()
        };
        let decoded = tk.decode(&enc).expect("decode");
        assert_eq!(decoded, text);
    }
}
