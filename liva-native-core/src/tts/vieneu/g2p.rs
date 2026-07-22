//! Bilingual (vi/en) grapheme-to-phoneme engine.
//!
//! Vendored from sea-g2p (`pnnbao97/sea-g2p`, Apache-2.0), `src/g2p/mod.rs`.
//! This is the exact phonemizer VieNeu-TTS was trained with, so its 419-token
//! phoneme vocabulary lines up with the model's `tokenizer.json` — espeak-ng
//! (used by Piper) would not. Two changes from upstream, both behaviour-
//! preserving: the `.bin` dictionary is read into an owned `Vec<u8>` instead of
//! memory-mapped (drops the `memmap2` dep), and `once_cell::Lazy` becomes
//! `std::sync::LazyLock` (edition 2024). The lookup/segmentation logic is kept
//! byte-for-byte.

use regex::Regex;
use std::collections::HashMap;
use std::io;
use std::sync::{LazyLock, RwLock};

/// Binary dictionary reader over the `sea_g2p.bin` blob.
///
/// Layout (little-endian): magic `SEAP` + version at [0..8], then three counts
/// at [8..20] and three section offsets at [20..32]. Strings are a packed,
/// NUL-terminated pool addressed by a `u32` offset table; `merged` and `common`
/// are sorted `(word_id, …)` records queried by binary search.
pub struct PhonemeDict {
    data: Vec<u8>,
    string_count: u32,
    merged_count: u32,
    common_count: u32,
    string_offsets_pos: usize,
    merged_pos: usize,
    common_pos: usize,
}

impl PhonemeDict {
    pub fn new(path: &str) -> io::Result<Self> {
        let data = std::fs::read(path)?;

        if data.len() < 32 || &data[0..4] != b"SEAP" {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "Invalid dictionary format",
            ));
        }

        let string_count = u32::from_le_bytes(data[8..12].try_into().unwrap());
        let merged_count = u32::from_le_bytes(data[12..16].try_into().unwrap());
        let common_count = u32::from_le_bytes(data[16..20].try_into().unwrap());

        let string_offsets_pos = u32::from_le_bytes(data[20..24].try_into().unwrap()) as usize;
        let merged_pos = u32::from_le_bytes(data[24..28].try_into().unwrap()) as usize;
        let common_pos = u32::from_le_bytes(data[28..32].try_into().unwrap()) as usize;

        Ok(Self {
            data,
            string_count,
            merged_count,
            common_count,
            string_offsets_pos,
            merged_pos,
            common_pos,
        })
    }

    fn get_string(&self, id: u32) -> &str {
        if id >= self.string_count {
            return "";
        }
        let off_ptr = self.string_offsets_pos + (id as usize * 4);
        let offset =
            u32::from_le_bytes(self.data[off_ptr..off_ptr + 4].try_into().unwrap()) as usize;

        let start = 32 + offset;
        let mut end = start;
        while end < self.data.len() && self.data[end] != 0 {
            end += 1;
        }
        std::str::from_utf8(&self.data[start..end]).unwrap_or("")
    }

    pub fn lookup_merged(&self, word: &str) -> Option<&str> {
        let mut low = 0;
        let mut high = self.merged_count as i32 - 1;

        while low <= high {
            let mid = (low + high) / 2;
            let ptr = self.merged_pos + (mid as usize * 8);
            let w_id = u32::from_le_bytes(self.data[ptr..ptr + 4].try_into().unwrap());
            let current_word = self.get_string(w_id);

            if current_word == word {
                let p_id = u32::from_le_bytes(self.data[ptr + 4..ptr + 8].try_into().unwrap());
                return Some(self.get_string(p_id));
            } else if current_word < word {
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        None
    }

    pub fn lookup_common(&self, word: &str) -> Option<(&str, &str)> {
        let mut low = 0;
        let mut high = self.common_count as i32 - 1;

        while low <= high {
            let mid = (low + high) / 2;
            let ptr = self.common_pos + (mid as usize * 12);
            let w_id = u32::from_le_bytes(self.data[ptr..ptr + 4].try_into().unwrap());
            let current_word = self.get_string(w_id);

            if current_word == word {
                let vi_id = u32::from_le_bytes(self.data[ptr + 4..ptr + 8].try_into().unwrap());
                let en_id = u32::from_le_bytes(self.data[ptr + 8..ptr + 12].try_into().unwrap());
                return Some((self.get_string(vi_id), self.get_string(en_id)));
            } else if current_word < word {
                low = mid + 1;
            } else {
                high = mid - 1;
            }
        }
        None
    }
}

static RE_TOKEN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(?i)(<en>.*?</en>)|(\w+(?:['’]\w+)*)|([^\w\s])").unwrap());

static RE_TAG_CONTENT: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"(\w+(?:['’]\w+)*)|([^\w\s])").unwrap());

static RE_TAG_STRIP: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"(?i)</?en>").unwrap());

static VI_ACCENTS: &str = "àáảãạăằắẳẵặâầấẩẫậèéẻẽẹêềếểễệìíỉĩịòóỏõọôồốổỗộơờớởỡợùúủũụưừứửữựỳýỷỹỵđ";

// Nguyên âm tiếng Anh + tiếng Việt (lowercase, đã include dấu)
static VOWELS: &str = "aeiouyàáảãạăằắẳẵặâầấẩẫậèéẻẽẹêềếểễệìíỉĩịòóỏõọôồốổỗộơờớởỡợùúủũụưừứửữựỳýỷỹỵ";

/// Kiểm tra segment có cả nguyên âm lẫn phụ âm không.
fn has_vowel_and_consonant(s: &str) -> bool {
    let mut has_v = false;
    let mut has_c = false;
    for c in s.chars() {
        let lc = c.to_lowercase().next().unwrap_or(c);
        if VOWELS.contains(lc) {
            has_v = true;
        } else if lc.is_alphabetic() {
            has_c = true;
        }
        if has_v && has_c {
            return true;
        }
    }
    false
}

/// Ánh xạ một token dấu câu về dạng GIỮ trong chuỗi phoneme, đồng bộ với quy tắc
/// của Normalizer: `, . ! ?` giữ nguyên; `; :` -> `,`; ellipsis -> `.`; còn lại bỏ.
fn map_punct(s: &str) -> Option<&'static str> {
    let mut it = s.chars();
    let c = match (it.next(), it.next()) {
        (Some(c), None) => c,
        _ => return None,
    };
    match c {
        ',' => Some(","),
        '.' => Some("."),
        '!' => Some("!"),
        '?' => Some("?"),
        ';' | ':' => Some(","),
        '\u{2026}' | '\u{2025}' | '\u{2024}' => Some("."),
        _ => None,
    }
}

#[derive(Clone)]
struct Token {
    lang: String,
    content: String,
    phone: Option<String>,
    is_explicit_en: bool,
}

pub struct G2PEngine {
    dict: PhonemeDict,
    merged_cache: RwLock<HashMap<String, String>>,
    common_cache: RwLock<HashMap<String, (String, String)>>,
    missing_merged: RwLock<std::collections::HashSet<String>>,
    missing_common: RwLock<std::collections::HashSet<String>>,
    /// Cache kết quả segment_oov. Key = "{word}_{lang}", value = None nếu không segment được.
    segmentation_cache: RwLock<HashMap<String, Option<String>>>,
}

impl G2PEngine {
    pub fn new(dict_path: &str) -> io::Result<Self> {
        Ok(Self {
            dict: PhonemeDict::new(dict_path)?,
            merged_cache: RwLock::new(HashMap::with_capacity(2048)),
            common_cache: RwLock::new(HashMap::with_capacity(1024)),
            missing_merged: RwLock::new(std::collections::HashSet::new()),
            missing_common: RwLock::new(std::collections::HashSet::new()),
            segmentation_cache: RwLock::new(HashMap::with_capacity(512)),
        })
    }

    fn cached_lookup_merged(&self, word: &str) -> Option<String> {
        {
            let r = self.merged_cache.read().unwrap();
            if let Some(v) = r.get(word) {
                return Some(v.clone());
            }
        }
        {
            let m = self.missing_merged.read().unwrap();
            if m.contains(word) {
                return None;
            }
        }
        match self.dict.lookup_merged(word) {
            Some(s) => {
                let val = s.to_string();
                let mut w = self.merged_cache.write().unwrap();
                if w.len() >= 10_000 {
                    w.clear();
                }
                w.insert(word.to_string(), val.clone());
                Some(val)
            }
            None => {
                let mut m = self.missing_merged.write().unwrap();
                if m.len() < 50_000 {
                    m.insert(word.to_string());
                }
                None
            }
        }
    }

    fn cached_lookup_common(&self, word: &str) -> Option<(String, String)> {
        {
            let r = self.common_cache.read().unwrap();
            if let Some(v) = r.get(word) {
                return Some(v.clone());
            }
        }
        {
            let m = self.missing_common.read().unwrap();
            if m.contains(word) {
                return None;
            }
        }
        match self.dict.lookup_common(word) {
            Some((v, e)) => {
                let val = (v.to_string(), e.to_string());
                let mut w = self.common_cache.write().unwrap();
                if w.len() >= 5_000 {
                    w.clear();
                }
                w.insert(word.to_string(), val.clone());
                Some(val)
            }
            None => {
                let mut m = self.missing_common.write().unwrap();
                if m.len() < 50_000 {
                    m.insert(word.to_string());
                }
                None
            }
        }
    }

    /// Resolve phoneme cho một segment đơn từ dict, theo ngữ cảnh lang.
    fn resolve_segment_phone(&self, segment: &str, lang: &str) -> Option<String> {
        let lw = segment.to_lowercase();

        if let Some(p) = self.cached_lookup_merged(&lw) {
            return Some(p.replace("<en>", "").trim().to_string());
        }

        if let Some((vi, en)) = self.cached_lookup_common(&lw) {
            let phone = if lang == "en" && !en.is_empty() {
                en.replace("<en>", "").trim().to_string()
            } else if !vi.is_empty() {
                vi.trim().to_string()
            } else {
                en.replace("<en>", "").trim().to_string()
            };
            return Some(phone);
        }

        None
    }

    /// DP segmentation cho OOV word: segment dài nhất được ưu tiên; mỗi segment
    /// phải có trong dict và có cả nguyên âm lẫn phụ âm.
    fn segment_oov(&self, word: &str, lang: &str) -> Option<String> {
        let cache_key = format!("{}_{}", word, lang);
        {
            let r = self.segmentation_cache.read().unwrap();
            if let Some(cached) = r.get(&cache_key) {
                return cached.clone();
            }
        }

        let chars: Vec<char> = word.chars().collect();
        let n = chars.len();

        let mut dp: Vec<Option<String>> = vec![None; n + 1];
        dp[0] = Some(String::new());

        for i in 0..n {
            if dp[i].is_none() {
                continue;
            }

            for j in (i + 1..=n).rev() {
                let segment: String = chars[i..j].iter().collect();

                if !has_vowel_and_consonant(&segment) {
                    continue;
                }

                if let Some(phone) = self.resolve_segment_phone(&segment, lang) {
                    let prev = dp[i].as_ref().unwrap();
                    let new_phone = if prev.is_empty() {
                        phone
                    } else {
                        format!("{} {}", prev, phone)
                    };
                    dp[j] = dp[j].take().or(Some(new_phone));
                }
            }
        }

        let result = dp[n].clone();

        {
            let mut w = self.segmentation_cache.write().unwrap();
            if w.len() >= 5_000 {
                w.clear();
            }
            w.insert(cache_key, result.clone());
        }

        result
    }

    /// Char-by-char fallback — last resort khi segment_oov cũng thất bại.
    fn char_fallback(&self, content: &str, lang: &str) -> String {
        content
            .chars()
            .map(|c| {
                let cl = c.to_lowercase().to_string();
                if let Some(cp) = self.cached_lookup_merged(&cl) {
                    cp.replace("<en>", "").trim().to_string()
                } else if let Some((v, e)) = self.cached_lookup_common(&cl) {
                    let p = if lang == "en" && !e.is_empty() {
                        e
                    } else if !v.is_empty() {
                        v
                    } else {
                        e
                    };
                    p.replace("<en>", "").trim().to_string()
                } else {
                    cl
                }
            })
            .collect::<Vec<String>>()
            .join("")
    }

    pub fn phonemize(&self, text: &str) -> String {
        let mut tokens = Vec::new();

        for cap in RE_TOKEN.captures_iter(text) {
            if let Some(en_tag) = cap.get(1) {
                let content = RE_TAG_STRIP.replace_all(en_tag.as_str(), "").trim().to_string();
                for scall in RE_TAG_CONTENT.captures_iter(&content) {
                    if let Some(sw) = scall.get(1) {
                        let word = sw.as_str().to_string();
                        let lw = word.to_lowercase();
                        let mut phone_val = None;

                        if let Some(p) = self.cached_lookup_merged(&lw) {
                            phone_val = Some(p.replace("<en>", "").trim().to_string());
                        } else if let Some((_, en)) = self.cached_lookup_common(&lw)
                            && !en.is_empty() {
                                phone_val = Some(en.replace("<en>", "").trim().to_string());
                            }

                        tokens.push(Token {
                            lang: "en".to_string(),
                            content: word,
                            phone: phone_val,
                            is_explicit_en: true,
                        });
                    } else if let Some(sp) = scall.get(2) {
                        tokens.push(Token {
                            lang: "punct".to_string(),
                            content: sp.as_str().to_string(),
                            phone: Some(sp.as_str().to_string()),
                            is_explicit_en: true,
                        });
                    }
                }
            } else if let Some(word) = cap.get(2) {
                let lw = word.as_str().to_lowercase();
                if let Some(p) = self.cached_lookup_merged(&lw) {
                    let lang = if p.contains("<en>") { "en" } else { "vi" };
                    tokens.push(Token {
                        lang: lang.to_string(),
                        content: word.as_str().to_string(),
                        phone: Some(p.replace("<en>", "").trim().to_string()),
                        is_explicit_en: false,
                    });
                } else if let Some((vi, en)) = self.cached_lookup_common(&lw) {
                    tokens.push(Token {
                        lang: "common".to_string(),
                        content: word.as_str().to_string(),
                        phone: Some(format!(
                            "\x1F{}\x1F{}\x1F",
                            vi.trim(),
                            en.replace("<en>", "").trim()
                        )),
                        is_explicit_en: false,
                    });
                } else {
                    let has_vi_accent = lw.chars().any(|c| VI_ACCENTS.contains(c));
                    tokens.push(Token {
                        lang: if has_vi_accent { "vi".to_string() } else { "en".to_string() },
                        content: word.as_str().to_string(),
                        phone: None,
                        is_explicit_en: false,
                    });
                }
            } else if let Some(punct) = cap.get(3) {
                tokens.push(Token {
                    lang: "punct".to_string(),
                    content: punct.as_str().to_string(),
                    phone: Some(punct.as_str().to_string()),
                    is_explicit_en: false,
                });
            }
        }

        self.propagate_language(&mut tokens);

        let mut result = Vec::new();
        for t in tokens {
            if t.lang == "punct" {
                if let Some(p) = map_punct(&t.content) {
                    result.push(p.to_string());
                }
            } else {
                let phone = if let Some(p) = t.phone {
                    if p.starts_with('\x1F') && p.ends_with('\x1F') {
                        let inner = &p[1..p.len() - 1];
                        let sep = inner.find('\x1F').unwrap_or(inner.len());
                        if t.lang == "en" {
                            let mut p_val = if sep < inner.len() {
                                inner[sep + 1..].to_string()
                            } else {
                                String::new()
                            };
                            if t.content.to_lowercase() == "a" && !t.is_explicit_en {
                                p_val = "ɐ".to_string();
                            }
                            p_val
                        } else {
                            inner[..sep].to_string()
                        }
                    } else {
                        let mut p_val = p;
                        if t.lang == "en" && t.content.to_lowercase() == "a" && !t.is_explicit_en {
                            p_val = "ɐ".to_string();
                        }
                        p_val
                    }
                } else {
                    let lw = t.content.to_lowercase();
                    self.segment_oov(&lw, &t.lang)
                        .unwrap_or_else(|| self.char_fallback(&t.content, &t.lang))
                };
                result.push(phone.trim().to_string());
            }
        }

        let mut joined = result
            .join(" ")
            .replace(" .", ".")
            .replace(" ,", ",")
            .replace(" !", "!")
            .replace(" ?", "?")
            .replace(" ;", ";")
            .replace(" :", ":");
        while joined.contains("..") {
            joined = joined.replace("..", ".");
        }
        while joined.contains(",,") {
            joined = joined.replace(",,", ",");
        }
        joined
    }

    fn propagate_language(&self, tokens: &mut [Token]) {
        let n = tokens.len();
        let mut i = 0;
        while i < n {
            if tokens[i].lang == "common" {
                let start = i;
                while i < n && tokens[i].lang == "common" {
                    i += 1;
                }
                let end = i - 1;

                let is_stop_punct = |t: &Token| -> bool {
                    t.content
                        .chars()
                        .next()
                        .map(|c| t.content.len() == c.len_utf8() && ".!?;:()[]{}".contains(c))
                        .unwrap_or(false)
                };

                let mut left_anchor = None;
                let mut left_dist = 999;
                for l in (0..start).rev() {
                    if is_stop_punct(&tokens[l]) {
                        break;
                    }
                    if tokens[l].lang == "vi" || tokens[l].lang == "en" {
                        left_anchor = Some(tokens[l].lang.clone());
                        left_dist = start - l;
                        break;
                    }
                }

                let mut right_anchor = None;
                let mut right_dist = 999;
                for r in (end + 1)..n {
                    if is_stop_punct(&tokens[r]) {
                        break;
                    }
                    if tokens[r].lang == "vi" || tokens[r].lang == "en" {
                        right_anchor = Some(tokens[r].lang.clone());
                        right_dist = r - end;
                        break;
                    }
                }

                let final_lang = if let (Some(l), Some(r)) = (left_anchor.as_ref(), right_anchor.as_ref())
                {
                    if right_dist <= left_dist {
                        r.clone()
                    } else {
                        l.clone()
                    }
                } else if let Some(l) = left_anchor {
                    l
                } else if let Some(r) = right_anchor {
                    r
                } else {
                    "vi".to_string()
                };

                for idx in start..=end {
                    tokens[idx].lang = final_lang.clone();
                }
            } else {
                i += 1;
            }
        }
    }
}
