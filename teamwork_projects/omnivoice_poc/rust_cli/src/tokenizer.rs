use std::collections::HashMap;

pub struct TtsTokenizer {
    vocab: HashMap<char, i64>,
}

impl TtsTokenizer {
    pub fn new() -> Self {
        let mut vocab = HashMap::new();
        // Load the exact Kokoro character vocab
        let vocab_entries = [
            ('$', 0),
            (';', 1),
            (':', 2),
            (',', 3),
            ('.', 4),
            ('!', 5),
            ('?', 6),
            ('—', 9),
            ('…', 10),
            ('"', 11),
            ('(', 12),
            (')', 13),
            ('“', 14),
            ('”', 15),
            (' ', 16),
            ('̃', 17),
            ('ʣ', 18),
            ('ʥ', 19),
            ('ʦ', 20),
            ('ʨ', 21),
            ('ᵝ', 22),
            ('ꭧ', 23),
            ('A', 24),
            ('I', 25),
            ('O', 31),
            ('Q', 33),
            ('S', 35),
            ('T', 36),
            ('W', 39),
            ('Y', 41),
            ('ᵊ', 42),
            ('a', 43),
            ('b', 44),
            ('c', 45),
            ('d', 46),
            ('e', 47),
            ('f', 48),
            ('h', 50),
            ('i', 51),
            ('j', 52),
            ('k', 53),
            ('l', 54),
            ('m', 55),
            ('n', 56),
            ('o', 57),
            ('p', 58),
            ('q', 59),
            ('r', 60),
            ('s', 61),
            ('t', 62),
            ('u', 63),
            ('v', 64),
            ('w', 65),
            ('x', 66),
            ('y', 67),
            ('z', 68),
            ('ɑ', 69),
            ('ɐ', 70),
            ('ɒ', 71),
            ('æ', 72),
            ('β', 75),
            ('ɔ', 76),
            ('ɕ', 77),
            ('ç', 78),
            ('ɖ', 80),
            ('ð', 81),
            ('ʤ', 82),
            ('ə', 83),
            ('ɚ', 85),
            ('ɛ', 86),
            ('ɜ', 87),
            ('ɟ', 90),
            ('ɡ', 92),
            ('ɥ', 99),
            ('ɨ', 101),
            ('ɪ', 102),
            ('ʝ', 103),
            ('ɯ', 110),
            ('ɰ', 111),
            ('ŋ', 112),
            ('ɳ', 113),
            ('ɲ', 114),
            ('ɴ', 115),
            ('ø', 116),
            ('ɸ', 118),
            ('θ', 119),
            ('œ', 120),
            ('ɹ', 123),
            ('ɾ', 125),
            ('ɻ', 126),
            ('ʁ', 128),
            ('ɽ', 129),
            ('ʂ', 130),
            ('ʃ', 131),
            ('ʈ', 132),
            ('ʧ', 133),
            ('ʊ', 135),
            ('ʋ', 136),
            ('ʌ', 138),
            ('ɣ', 139),
            ('ɤ', 140),
            ('χ', 142),
            ('ʎ', 143),
            ('ʒ', 147),
            ('ʔ', 148),
            ('ˈ', 156),
            ('ˌ', 157),
            ('ː', 158),
            ('ʰ', 162),
            ('ʲ', 164),
            ('↓', 169),
            ('→', 171),
            ('↗', 172),
            ('↘', 173),
            ('ᵻ', 177),
        ];

        for &(ch, id) in vocab_entries.iter() {
            vocab.insert(ch, id);
        }

        Self { vocab }
    }

    pub fn tokenize(&self, phonemes: &str) -> Vec<i64> {
        let mut ids = Vec::new();
        // BOS token is $ (0)
        ids.push(0);

        for ch in phonemes.chars() {
            if let Some(&id) = self.vocab.get(&ch) {
                ids.push(id);
            }
            // Skip characters not in vocab
        }

        // EOS token is $ (0)
        ids.push(0);
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tokenizer_sample() {
        let tok = TtsTokenizer::new();
        // "a b c" should yield [0, 43, 16, 44, 16, 45, 0]
        let ids = tok.tokenize("a b c");
        assert_eq!(ids, vec![0, 43, 16, 44, 16, 45, 0]);
    }

    #[test]
    fn test_tokenizer_skip_unknown() {
        let tok = TtsTokenizer::new();
        // '@' is not in vocab, should be skipped
        let ids = tok.tokenize("a@b");
        assert_eq!(ids, vec![0, 43, 44, 0]);
    }
}
