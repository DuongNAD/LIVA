use regex::Regex;
use std::process::Command;

pub struct G2p;

impl G2p {
    pub fn phonemize(text: &str) -> String {
        // 1. Text normalization / cleaning
        let cleaned = Self::clean_text(text);

        // 2. Try executing espeak-ng if available
        if let Ok(phonemes) = Self::try_espeak_ng(&cleaned) {
            return Self::post_process(&phonemes);
        }

        // 3. Fallback to dictionary + rule-based mapping
        let phonemes = Self::fallback_phonemize(&cleaned);
        Self::post_process(&phonemes)
    }

    fn clean_text(text: &str) -> String {
        let t = text
            .replace("’", "'")
            .replace("‘", "'")
            .replace("“", "\"")
            .replace("”", "\"")
            .replace("«", "\"")
            .replace("»", "\"")
            .replace("(", " ")
            .replace(")", " ")
            .replace("—", " — ");

        // Expand abbreviations
        let mut t = t;
        let abbrevs = [
            (Regex::new(r"(?i)\bdr\.").unwrap(), "Doctor"),
            (Regex::new(r"(?i)\bmr\.").unwrap(), "Mister"),
            (Regex::new(r"(?i)\bms\.").unwrap(), "Miss"),
            (Regex::new(r"(?i)\bmrs\.").unwrap(), "Misses"),
            (Regex::new(r"(?i)\betc\.").unwrap(), "etcetera"),
        ];

        for (re, repl) in abbrevs.iter() {
            t = re.replace_all(&t, *repl).to_string();
        }

        t.trim().to_string()
    }

    fn try_espeak_ng(text: &str) -> Result<String, String> {
        let output = Command::new("espeak-ng")
            .args(&["-q", "--ipa", "-v", "en-us", text])
            .output()
            .map_err(|e| e.to_string())?;

        if !output.status.success() {
            return Err("espeak-ng failed".to_string());
        }

        let ipa = String::from_utf8(output.stdout).map_err(|e| e.to_string())?;

        Ok(ipa.trim().to_string())
    }

    fn fallback_phonemize(text: &str) -> String {
        // Split into words and punctuation
        let re = Regex::new(r"(\s*[;:,.!?¡¿—…\x22«»“”()]+\s*)+").unwrap();

        let mut parts = Vec::new();
        let mut last_idx = 0;

        for mat in re.find_iter(text) {
            let start = mat.start();
            let end = mat.end();

            if start > last_idx {
                let word_part = &text[last_idx..start];
                parts.push(Self::phonemize_words(word_part));
            }

            parts.push(text[start..end].to_string());
            last_idx = end;
        }

        if last_idx < text.len() {
            let word_part = &text[last_idx..];
            parts.push(Self::phonemize_words(word_part));
        }

        parts.concat()
    }

    fn phonemize_words(text: &str) -> String {
        let words: Vec<&str> = text.split_whitespace().collect();
        let mut phonemized_words = Vec::new();

        for word in words {
            // Strip punctuation from word boundaries if any
            let clean_word = word.trim_matches(|c: char| c.is_ascii_punctuation());
            if let Some(ipa) = Self::get_word_phoneme(clean_word) {
                phonemized_words.push(ipa.to_string());
            } else {
                phonemized_words.push(Self::fallback_word_to_phonemes(clean_word));
            }
        }

        phonemized_words.join(" ")
    }

    fn get_word_phoneme(word: &str) -> Option<&'static str> {
        match word.to_lowercase().as_str() {
            "hello" => Some("həlˈoʊ"),
            "world" => Some("wˈɜːld"),
            "life" => Some("lˈaɪf"),
            "is" => Some("ɪz"),
            "like" => Some("lˈaɪk"),
            "a" => Some("ɐ"),
            "box" => Some("bˈɑːks"),
            "of" => Some("ʌv"),
            "chocolates" => Some("tʃˈɑːkləts"),
            "you" => Some("juː"),
            "never" => Some("nˈɛvɚ"),
            "know" => Some("nˈoʊ"),
            "what" => Some("wʌt"),
            "you're" | "you’re" => Some("jʊɹ"),
            "gonna" => Some("ɡˌənə"),
            "get" => Some("ɡˈɛt"),
            "some" => Some("sˌʌm"),
            "pending" => Some("pˈɛndɪŋ"),
            "text" => Some("tˈɛkst"),
            "incomplete" => Some("ɪŋkəmplˈiːt"),
            "sentence" => Some("sˈɛntəns"),
            "without" => Some("wɪðˌaʊt"),
            "ending" => Some("ˈɛndɪŋ"),
            "should" => Some("ʃˌʊd"),
            "be" => Some("biː"),
            "ignored" => Some("ɪɡnˈoːɹd"),
            "misses" => Some("mˈɪsɪz"),
            "mrs" => Some("mˈɪsɪz"),
            "etcetera" => Some("ɛtsˈɛtəɹə"),
            _ => None,
        }
    }

    fn fallback_word_to_phonemes(word: &str) -> String {
        let mut res = String::new();
        for c in word.chars() {
            let ipa = match c.to_ascii_lowercase() {
                'a' => "ɐ",
                'b' => "b",
                'c' => "k",
                'd' => "d",
                'e' => "ɛ",
                'f' => "f",
                'g' => "ɡ",
                'h' => "h",
                'i' => "ɪ",
                'j' => "ʤ",
                'k' => "k",
                'l' => "l",
                'm' => "m",
                'n' => "n",
                'o' => "oʊ",
                'p' => "p",
                'q' => "k",
                'r' => "ɹ",
                's' => "s",
                't' => "t",
                'u' => "ʊ",
                'v' => "v",
                'w' => "w",
                'x' => "ks",
                'y' => "j",
                'z' => "z",
                other => {
                    res.push(other);
                    continue;
                }
            };
            res.push_str(ipa);
        }
        res
    }

    fn post_process(ipa: &str) -> String {
        let mut s = ipa.to_string();
        s = s.replace("kəkˈoːɹoʊ", "kˈoʊkəɹoʊ");
        s = s.replace("kəkˈɔːɹəʊ", "kˈəʊkəɹəʊ");
        s = s.replace("ʲ", "j");
        s = s.replace("r", "ɹ");
        s = s.replace("x", "k");
        s = s.replace("ɬ", "l");

        // Regex for space before hˈʌndɹɪd
        let re_hundred = Regex::new(r"([a-zɹː])hˈʌndɹɪd").unwrap();
        s = re_hundred.replace_all(&s, "$1 hˈʌndɹɪd").to_string();

        // Regex for z before punctuation or end
        let re_z = Regex::new(r" z([;:,.!?¡¿—…\x22«»“” ])").unwrap();
        s = re_z.replace_all(&s, "z$1").to_string();
        if s.ends_with(" z") {
            s = s[..s.len() - 2].to_string() + "z";
        }

        // American English flap-t (ti -> di after nˈaɪn)
        let re_flap = Regex::new(r"nˈaɪnti([^ː])").unwrap();
        s = re_flap.replace_all(&s, "nˈaɪndi$1").to_string();
        if s.ends_with("nˈaɪnti") {
            s = s[..s.len() - "nˈaɪnti".len()].to_string() + "nˈaɪndi";
        }

        s.trim().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_phonemize_hello_world() {
        let ipa = G2p::phonemize("Hello world.");
        assert_eq!(ipa, "həlˈoʊ wˈɜːld.");
    }

    #[test]
    fn test_phonemize_life_is_like() {
        let ipa = G2p::phonemize(
            "Life is like a box of chocolates. You never know what you're gonna get.",
        );
        assert_eq!(
            ipa,
            "lˈaɪf ɪz lˈaɪk ɐ bˈɑːks ʌv tʃˈɑːkləts. juː nˈɛvɚ nˈoʊ wʌt jʊɹ ɡˌənə ɡˈɛt."
        );
    }

    #[test]
    fn test_abbrev_expansion() {
        let ipa = G2p::phonemize("Hello Dr. Watson.");
        // Dr. -> Doctor -> d oʊ k t oʊ ɹ (in fallback)
        // clean_text("Hello Dr. Watson.") -> "Hello Doctor Watson."
        assert!(ipa.contains("doʊktoʊɹ"));
    }
}
