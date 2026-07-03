//! Vietnamese text normalizer for TTS — runs on every TTS path *before*
//! synthesis so digits, dates, times, currency, units, phone numbers,
//! abbreviations and common foreign words are expanded into plain Vietnamese
//! words the Piper/espeak voices can pronounce.
//!
//! Native port of `liva-voice/src/vietnamese_normalizer.py` that deliberately
//! **fixes** its known bugs instead of replicating them:
//!
//! * **Thousands-separator bug** — Vietnamese groups thousands with `.` and
//!   uses `,` as the decimal mark: `1.000` → "một nghìn", `2.500.000` →
//!   "hai triệu năm trăm nghìn", `3,5` → "ba phẩy năm". The Python
//!   normalizer read `1.000` as a *decimal* ("một phẩy không không không").
//! * **No date/time/currency handling** in Python — added here
//!   (`25/12/2026`, `10:30`, `5.000đ`, `$5`, `5000 VND`, …).
//! * **Abbreviation boundaries** — the Python regexes required a space on
//!   *both* sides, missing abbreviations at string boundaries or next to
//!   punctuation. This port matches at Unicode word boundaries.
//! * Integer reading (mốt/tư/lăm/linh, nghìn/triệu/tỷ) is implemented
//!   natively — no `num2words` dependency, no I/O, infallible.
//!
//! Unlike the Python version, input case is preserved for text that passes
//! through untouched (lowercasing everything is unnecessary for TTS and
//! destroys information); pattern matching is case-insensitive where it
//! matters. Rule order is significant and chosen so no span is processed
//! twice: dotted abbreviations → phone → date → time → currency → percent →
//! number+unit → composite numbers → bare integers → word abbreviations →
//! foreign words → whitespace cleanup. Every replacement emits words without
//! digits, so later numeric rules cannot re-match earlier output.

use regex::Regex;
use std::sync::OnceLock;

// ---------------------------------------------------------------------------
// Number reading core
// ---------------------------------------------------------------------------

/// Largest value read with scale words (999 tỷ). Anything larger is read
/// digit-by-digit, which is always intelligible.
const MAX_SCALED: u64 = 999_999_999_999;

fn digit_name(d: u8) -> &'static str {
    match d {
        0 => "không",
        1 => "một",
        2 => "hai",
        3 => "ba",
        4 => "bốn",
        5 => "năm",
        6 => "sáu",
        7 => "bảy",
        8 => "tám",
        9 => "chín",
        _ => "",
    }
}

/// Read `0..=999` as words. `full` selects the "không trăm" / "linh" style
/// required for a group that follows a higher non-zero group, e.g.
/// 2026 → "hai nghìn **không trăm** hai mươi sáu".
fn read_group3(n: u16, full: bool) -> String {
    let h = (n / 100) as u8;
    let t = ((n / 10) % 10) as u8;
    let u = (n % 10) as u8;
    let mut parts: Vec<&'static str> = Vec::new();

    if h > 0 {
        parts.push(digit_name(h));
        parts.push("trăm");
    } else if full && (t > 0 || u > 0) {
        parts.push("không");
        parts.push("trăm");
    }
    let after_hundred = h > 0 || full;

    match t {
        0 => {
            if u > 0 {
                if after_hundred {
                    parts.push("linh"); // 105 → "một trăm linh năm"
                }
                parts.push(digit_name(u));
            }
        }
        1 => {
            parts.push("mười");
            match u {
                0 => {}
                5 => parts.push("lăm"), // 15 → "mười lăm"
                _ => parts.push(digit_name(u)),
            }
        }
        _ => {
            parts.push(digit_name(t));
            parts.push("mươi");
            match u {
                0 => {}
                1 => parts.push("mốt"), // 21 → "hai mươi mốt"
                4 => parts.push("tư"),  // 24 → "hai mươi tư"
                5 => parts.push("lăm"), // 25 → "hai mươi lăm"
                _ => parts.push(digit_name(u)),
            }
        }
    }
    parts.join(" ")
}

/// Read an integer as Vietnamese words with nghìn/triệu/tỷ scales.
fn read_u64(n: u64) -> String {
    if n == 0 {
        return "không".to_string();
    }
    if n > MAX_SCALED {
        return read_digits(&n.to_string());
    }
    let groups = [
        (((n / 1_000_000_000) % 1_000) as u16, "tỷ"),
        (((n / 1_000_000) % 1_000) as u16, "triệu"),
        (((n / 1_000) % 1_000) as u16, "nghìn"),
        ((n % 1_000) as u16, ""),
    ];
    let mut out: Vec<String> = Vec::new();
    let mut seen_higher = false;
    for (value, scale) in groups {
        if value == 0 {
            continue; // zero groups are silent (1.000.000 → "một triệu")
        }
        let mut words = read_group3(value, seen_higher);
        if !scale.is_empty() {
            words.push(' ');
            words.push_str(scale);
        }
        out.push(words);
        seen_higher = true;
    }
    out.join(" ")
}

/// Read every ASCII digit in `s` individually ("09" → "không chín").
fn read_digits(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_ascii_digit())
        .map(|c| digit_name(c as u8 - b'0'))
        .collect::<Vec<_>>()
        .join(" ")
}

/// Read a plain digit string. Leading-zero strings (codes, phone fragments,
/// minutes like "05") and strings beyond 999 tỷ are read digit-by-digit.
fn read_integer_str(s: &str) -> String {
    if s.is_empty() {
        return String::new();
    }
    if s.len() > 1 && s.starts_with('0') {
        return read_digits(s);
    }
    match s.parse::<u64>() {
        Ok(n) if n <= MAX_SCALED => read_u64(n),
        _ => read_digits(s),
    }
}

/// Read a number that may carry Vietnamese thousands separators (`.`) and a
/// decimal comma: "2.500.000" → "hai triệu năm trăm nghìn", "3,5" →
/// "ba phẩy năm" (fraction digits are read one by one, so "3,14" →
/// "ba phẩy một bốn"). Dot sequences that are not valid thousands grouping
/// (e.g. version strings like "3.14.1") are read as "chấm"-separated numbers.
fn read_number_string(s: &str) -> String {
    let (int_part, frac_part) = match s.find(',') {
        Some(i) => (&s[..i], Some(&s[i + 1..])),
        None => (s, None),
    };

    let int_words = if int_part.contains('.') {
        let segs: Vec<&str> = int_part.split('.').collect();
        let valid_grouping = (1..=3).contains(&segs[0].len())
            && segs[1..].iter().all(|g| g.len() == 3)
            && segs
                .iter()
                .all(|g| g.bytes().all(|b| b.is_ascii_digit()));
        if valid_grouping {
            read_integer_str(&segs.concat())
        } else {
            segs.iter()
                .map(|g| read_integer_str(g))
                .collect::<Vec<_>>()
                .join(" chấm ")
        }
    } else {
        read_integer_str(int_part)
    };

    match frac_part {
        Some(f) if !f.is_empty() => format!("{} phẩy {}", int_words, read_digits(f)),
        _ => int_words,
    }
}

// ---------------------------------------------------------------------------
// Compiled patterns (OnceLock statics, crate style — see tts/piper.rs)
// ---------------------------------------------------------------------------
// `[0-9]` is used instead of `\d` on purpose: `\d` in the regex crate matches
// all Unicode digits, which the ASCII-only readers above must never receive.

/// Common sub-pattern: an integer with optional `.`-thousands groups and an
/// optional decimal comma ("5", "5.000", "1.234,5", "3,5").
const NUM: &str = r"[0-9]+(?:\.[0-9]{3})*(?:,[0-9]+)?";

fn re_phone() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // VN mobile prefixes 09/03/05/07/08 + 8-9 more digits, optionally
    // separated by spaces, dots or dashes ("0912345678", "0901 234 567").
    RE.get_or_init(|| Regex::new(r"\b0[35789](?:[\s.\-]?[0-9]){8,9}\b").unwrap())
}

// Both date patterns capture an optional preceding literal "ngày" (group 1).
// The regex crate has no lookbehind, so the replacement re-emits the captured
// word instead of adding its own — "ngày 25/12/2026" must not become
// "ngày ngày hai mươi lăm …".

fn re_date_dmy() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:((?i:ngày))\s+)?([0-9]{1,2})/([0-9]{1,2})/([0-9]{4})\b").unwrap()
    })
}

fn re_date_dm() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"\b(?:((?i:ngày))\s+)?([0-9]{1,2})/([0-9]{1,2})\b").unwrap()
    })
}

fn re_month_year() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // "tháng 12/2026" → "tháng … năm …". M/YYYY is only a date when the
    // literal word "tháng" precedes it, so the word is required (captured and
    // re-emitted with its original case).
    RE.get_or_init(|| Regex::new(r"\b((?i:tháng))\s+([0-9]{1,2})/([0-9]{4})\b").unwrap())
}

fn re_time() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b([0-9]{1,2}):([0-9]{2})(?::([0-9]{2}))?\b").unwrap())
}

fn re_dong() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(&format!(r"\b({NUM})\s*(?:(?i:vnđ|vnd|đồng|đ)\b|₫)")).unwrap()
    })
}

fn re_dollar() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"\$\s?({NUM})")).unwrap())
}

fn re_percent() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(&format!(r"\b({NUM})\s*%")).unwrap())
}

fn re_number_unit() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Longest alternatives first so "km" wins over "k"/"m".
    RE.get_or_init(|| {
        Regex::new(&format!(
            r"\b({NUM})\s*((?i:km|kg|kb|gb|mb|ml|mm|cm|m|l|g|k))\b"
        ))
        .unwrap()
    })
}

fn re_composite_number() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Any digit run containing separators — semantics decided in
    // `read_number_string` (thousands vs decimal vs version-style).
    RE.get_or_init(|| Regex::new(r"\b[0-9]+(?:[.,][0-9]+)+\b").unwrap())
}

fn re_integer() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\b[0-9]+\b").unwrap())
}

fn re_dotted_abbr() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // "TP.HCM" before "TP." so the longer form wins at the same position.
    RE.get_or_init(|| {
        Regex::new(r"(?i)\b(tp\.hcm|tp\.|ths\.|ts\.|pgs\.|gs\.|bs\.|ks\.|kts\.|v\.v\.?)")
            .unwrap()
    })
}

fn re_quan_phuong() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // "Q.1" / "P.5" — only with a trailing number, so a stray "q."/"p." in
    // running text is left alone. Runs before number expansion.
    RE.get_or_init(|| Regex::new(r"(?i)\b([qp])\.\s?([0-9]{1,2})\b").unwrap())
}

fn re_word_abbr() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(tphcm|ubnd|thpt|thcs|hdmi|hcm|hn|đh|vn|bt|vs|vip|web|app|cpu|gpu|ram|hdd|ssd|usb|vga|dv|pt|km|kg|cm|mm|ml|kb|mb|gb)\b",
        )
        .unwrap()
    })
}

fn re_upper_abbr() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    // Case-SENSITIVE: lowercase "ai" ("who") and "it" are real Vietnamese
    // words; only the fully-uppercase acronyms are expanded.
    RE.get_or_init(|| Regex::new(r"\b(AI|IT)\b").unwrap())
}

fn re_foreign() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(
            r"(?i)\b(live stream|livestream|instagram|messenger|bluetooth|facebook|download|chatgpt|youtube|twitter|windows|android|offline|tiktok|google|upload|online|wi-fi|macos|linux|wifi|zalo|zoom|gpt|usd|eur|gbp|jpy|cny|ceo|cfo|cto|ios|no1|ok)\b",
        )
        .unwrap()
    })
}

fn re_multi_space() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+").unwrap())
}

fn re_space_before_punct() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"\s+([.,!?;:])").unwrap())
}

// ---------------------------------------------------------------------------
// Replacement passes (each consumes its digits, so later passes cannot
// re-process the same span)
// ---------------------------------------------------------------------------

/// "TP.HCM", "TS.", "v.v.", "Q.1" — must run first: `Q.`/`P.` need the raw
/// digit after the dot, and expansions never contain digits.
fn expand_dotted_abbreviations(text: &str) -> String {
    let t = re_dotted_abbr().replace_all(text, |c: &regex::Captures| {
        match c[1].to_lowercase().as_str() {
            "tp.hcm" => "thành phố hồ chí minh",
            "tp." => "thành phố",
            "ths." => "thạc sĩ",
            "ts." => "tiến sĩ",
            "pgs." => "phó giáo sư",
            "gs." => "giáo sư",
            "bs." => "bác sĩ",
            "ks." => "kỹ sư",
            "kts." => "kiến trúc sư",
            "v.v." | "v.v" => "vân vân",
            _ => return c[0].to_string(),
        }
        .to_string()
    });
    re_quan_phuong()
        .replace_all(&t, |c: &regex::Captures| {
            let word = if c[1].to_lowercase() == "q" {
                "quận"
            } else {
                "phường"
            };
            format!("{} {}", word, &c[2])
        })
        .into_owned()
}

/// Phone numbers are read digit-by-digit. Must run before any number rule so
/// "0912345678" is never parsed as the quantity 912.345.678.
fn expand_phone(text: &str) -> String {
    re_phone()
        .replace_all(text, |c: &regex::Captures| read_digits(&c[0]))
        .into_owned()
}

/// `25/12/2026` → "ngày … tháng … năm …"; `5/3` → "ngày năm tháng ba" only
/// when both parts are in calendar range (else it is left for the number
/// rules, e.g. a fraction like `15/30`). A literal "ngày" already in front of
/// the date is reused, never duplicated; "tháng M/YYYY" is expanded first as a
/// month/year date ("tháng 12/2026" → "tháng mười hai năm …").
fn expand_dates(text: &str) -> String {
    // Month/year first: "12/2026" alone never validates as d/m, so without
    // this pass it would fall through to the bare number rules.
    let t = re_month_year().replace_all(text, |c: &regex::Captures| {
        let m: u64 = c[2].parse().unwrap_or(0);
        let y: u64 = c[3].parse().unwrap_or(0);
        if (1..=12).contains(&m) {
            format!("{} {} năm {}", &c[1], read_u64(m), read_u64(y))
        } else {
            c[0].to_string()
        }
    });
    let t = re_date_dmy().replace_all(&t, |c: &regex::Captures| {
        let d: u64 = c[2].parse().unwrap_or(0);
        let m: u64 = c[3].parse().unwrap_or(0);
        let y: u64 = c[4].parse().unwrap_or(0);
        if (1..=31).contains(&d) && (1..=12).contains(&m) {
            let ngay = c.get(1).map_or("ngày", |p| p.as_str());
            format!(
                "{} {} tháng {} năm {}",
                ngay,
                read_u64(d),
                read_u64(m),
                read_u64(y)
            )
        } else {
            c[0].to_string()
        }
    });
    re_date_dm()
        .replace_all(&t, |c: &regex::Captures| {
            let d: u64 = c[2].parse().unwrap_or(0);
            let m: u64 = c[3].parse().unwrap_or(0);
            if (1..=31).contains(&d) && (1..=12).contains(&m) {
                let ngay = c.get(1).map_or("ngày", |p| p.as_str());
                format!("{} {} tháng {}", ngay, read_u64(d), read_u64(m))
            } else {
                c[0].to_string()
            }
        })
        .into_owned()
}

/// `10:30` → "mười giờ ba mươi phút"; minutes keep their leading zero read
/// digit-wise ("7:05" → "bảy giờ không năm phút"); ":00" is silent; an
/// optional seconds field becomes "… giây". Out-of-range values pass through.
fn expand_times(text: &str) -> String {
    re_time()
        .replace_all(text, |c: &regex::Captures| {
            let h: u32 = c[1].parse().unwrap_or(99);
            let m_str = c.get(2).map_or("", |m| m.as_str());
            let m: u32 = m_str.parse().unwrap_or(99);
            let s_str = c.get(3).map(|s| s.as_str());
            let s_ok = s_str.is_none_or(|s| s.parse::<u32>().unwrap_or(99) <= 59);
            if h > 23 || m > 59 || !s_ok {
                return c[0].to_string();
            }
            let mut out = format!("{} giờ", read_u64(h as u64));
            if m > 0 {
                out.push_str(&format!(" {} phút", read_integer_str(m_str)));
            }
            if let Some(ss) = s_str {
                if ss.parse::<u32>().unwrap_or(0) > 0 {
                    out.push_str(&format!(" {} giây", read_integer_str(ss)));
                }
            }
            out
        })
        .into_owned()
}

/// `5.000đ` / `5.000₫` / `5000 VND` / `2.500.000 đồng` → "… đồng";
/// `$5` → "năm đô la". Runs before the generic number rules so the amount and
/// its currency mark are consumed together.
fn expand_currency(text: &str) -> String {
    let t = re_dong().replace_all(text, |c: &regex::Captures| {
        format!("{} đồng", read_number_string(&c[1]))
    });
    re_dollar()
        .replace_all(&t, |c: &regex::Captures| {
            format!("{} đô la", read_number_string(&c[1]))
        })
        .into_owned()
}

/// `5%` → "năm phần trăm", `3,5%` → "ba phẩy năm phần trăm".
fn expand_percent(text: &str) -> String {
    re_percent()
        .replace_all(text, |c: &regex::Captures| {
            format!("{} phần trăm", read_number_string(&c[1]))
        })
        .into_owned()
}

fn unit_word(unit: &str) -> Option<&'static str> {
    Some(match unit {
        "km" => "ki lô mét",
        "kg" => "ki lô gam",
        "kb" => "ki lô bai",
        "gb" => "gi ga bai",
        "mb" => "mê ga bai",
        "ml" => "mi li lít",
        "mm" => "mi li mét",
        "cm" => "xăng ti mét",
        "m" => "mét",
        "l" => "lít",
        "g" => "gam",
        "k" => "nghìn", // "5k" → "năm nghìn"
        _ => return None,
    })
}

/// A number directly followed by a measurement unit: "5km", "70 kg", "100mb",
/// "5k". Single-letter units (m/l/g/k) are only expanded here, *with* a
/// number — standalone they are ordinary Vietnamese letters/words.
fn expand_number_units(text: &str) -> String {
    re_number_unit()
        .replace_all(text, |c: &regex::Captures| {
            match unit_word(c[2].to_lowercase().as_str()) {
                Some(u) => format!("{} {}", read_number_string(&c[1]), u),
                None => c[0].to_string(),
            }
        })
        .into_owned()
}

/// Remaining numbers: first separator composites ("1.000", "3,5", "3.14.1"),
/// then bare integers. Leading-zero integers are read digit-by-digit (they
/// are codes, never quantities).
fn expand_numbers(text: &str) -> String {
    let t = re_composite_number()
        .replace_all(text, |c: &regex::Captures| read_number_string(&c[0]));
    re_integer()
        .replace_all(&t, |c: &regex::Captures| read_integer_str(&c[0]))
        .into_owned()
}

fn word_abbr_expansion(key: &str) -> Option<&'static str> {
    Some(match key {
        "vn" => "việt nam",
        "bt" => "bình thường",
        "vs" => "với",
        "vip" => "víp",
        "web" => "wép",
        "app" => "áp",
        "cpu" => "si pi iu",
        "gpu" => "gi pi iu",
        "ram" => "ram",
        "hdd" => "hạch đi",
        "ssd" => "ét sờ đi",
        "usb" => "i u ét bi",
        "hdmi" => "hạch điêm ai",
        "vga" => "vi gi ai",
        "dv" => "dịch vụ",
        "pt" => "phút",
        // Multi-letter units also occur without a number ("vài kg gạo").
        "km" => "ki lô mét",
        "kg" => "ki lô gam",
        "cm" => "xăng ti mét",
        "mm" => "mi li mét",
        "ml" => "mi li lít",
        "kb" => "ki lô bai",
        "mb" => "mê ga bai",
        "gb" => "gi ga bai",
        // Common Vietnamese abbreviations (not in the Python dict).
        "tphcm" => "thành phố hồ chí minh",
        "hcm" => "hồ chí minh",
        "hn" => "hà nội",
        "ubnd" => "ủy ban nhân dân",
        "thpt" => "trung học phổ thông",
        "thcs" => "trung học cơ sở",
        "đh" => "đại học",
        _ => return None,
    })
}

/// Whole-word abbreviations at Unicode word boundaries (works at string
/// boundaries and next to punctuation — the Python version required spaces).
fn expand_word_abbreviations(text: &str) -> String {
    let t = re_word_abbr().replace_all(text, |c: &regex::Captures| {
        match word_abbr_expansion(c[1].to_lowercase().as_str()) {
            Some(full) => full.to_string(),
            None => c[0].to_string(),
        }
    });
    re_upper_abbr()
        .replace_all(&t, |c: &regex::Captures| {
            match c.get(1).map_or("", |m| m.as_str()) {
                "AI" => "a i".to_string(),
                "IT" => "i t".to_string(),
                _ => c[0].to_string(),
            }
        })
        .into_owned()
}

fn foreign_expansion(key: &str) -> Option<&'static str> {
    Some(match key {
        "livestream" | "live stream" => "lai sờ trim",
        "online" => "on lai",
        "offline" => "óp lai",
        "download" => "đao un lô",
        "upload" => "áp lô",
        "wifi" | "wi-fi" => "wai fai",
        "bluetooth" => "blu tút",
        "chatgpt" => "chát gí pí ti",
        "gpt" => "gí pí ti",
        "youtube" => "iu túp",
        "google" => "gù gồ",
        "facebook" => "phây búc",
        "twitter" => "tuít tơ",
        "instagram" => "in ét ta gram",
        "tiktok" => "tích tốc",
        "zalo" => "za lô",
        "messenger" => "mê xen gơ",
        "zoom" => "zum",
        "windows" => "win đớp",
        "macos" => "mác ô ét",
        "linux" => "li nắc",
        "android" => "an đờ rôi",
        "ios" => "ai ô ét",
        "usd" => "đô la mỹ",
        "eur" => "ê u rô",
        "gbp" => "bảng anh",
        "jpy" => "yên nhật",
        "cny" => "nhân dân tệ",
        "ok" => "ô kê",
        "ceo" => "si i ô",
        "cfo" => "si ét ô",
        "cto" => "si ti ô",
        "no1" => "năm bơ oăn",
        _ => return None,
    })
}

/// Vietnamized pronunciation of common foreign words. Unlike the Python
/// version this matches whole words only — its unbounded substring replace
/// turned "book" into "b<ô kê>…".
fn expand_foreign_words(text: &str) -> String {
    re_foreign()
        .replace_all(text, |c: &regex::Captures| {
            match foreign_expansion(c[1].to_lowercase().as_str()) {
                Some(p) => p.to_string(),
                None => c[0].to_string(),
            }
        })
        .into_owned()
}

/// Collapse whitespace runs and drop spaces left dangling before punctuation.
fn cleanup_whitespace(text: &str) -> String {
    let t = re_multi_space().replace_all(text, " ");
    let t = re_space_before_punct().replace_all(&t, "$1");
    t.trim().to_string()
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Normalize `text` for TTS in the given language.
///
/// * `"vi"` (and anything that is not English) → full Vietnamese
///   normalization via [`normalize_vi`]. Vietnamese is the default because it
///   is LIVA's primary voice.
/// * `"en"` / `"en-US"` … → light-touch passthrough (whitespace tidy only).
///
/// Infallible and pure: no I/O, unknown constructs pass through unchanged.
pub fn normalize(text: &str, lang: &str) -> String {
    if lang.trim().to_ascii_lowercase().starts_with("en") {
        normalize_en(text)
    } else {
        normalize_vi(text)
    }
}

/// Full Vietnamese normalization: numbers, dates, times, currency, percent,
/// units, phone numbers, abbreviations and foreign words. See module docs for
/// the rule order.
pub fn normalize_vi(text: &str) -> String {
    if text.trim().is_empty() {
        return String::new();
    }
    let t = expand_dotted_abbreviations(text);
    let t = expand_phone(&t);
    let t = expand_dates(&t);
    let t = expand_times(&t);
    let t = expand_currency(&t);
    let t = expand_percent(&t);
    let t = expand_number_units(&t);
    let t = expand_numbers(&t);
    let t = expand_word_abbreviations(&t);
    let t = expand_foreign_words(&t);
    cleanup_whitespace(&t)
}

/// English is a deliberate light-touch passthrough: the English Piper voice
/// already reads bare digits acceptably, so only repeated whitespace is
/// collapsed. Full English number/date normalization is a follow-up.
fn normalize_en(text: &str) -> String {
    re_multi_space().replace_all(text, " ").trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- integer reading core ----

    #[test]
    fn test_digits_and_teens() {
        assert_eq!(read_u64(0), "không");
        assert_eq!(read_u64(5), "năm");
        assert_eq!(read_u64(10), "mười");
        assert_eq!(read_u64(11), "mười một");
        assert_eq!(read_u64(14), "mười bốn");
        assert_eq!(read_u64(15), "mười lăm");
    }

    #[test]
    fn test_mot_tu_lam_rules() {
        assert_eq!(read_u64(21), "hai mươi mốt");
        assert_eq!(read_u64(24), "hai mươi tư");
        assert_eq!(read_u64(25), "hai mươi lăm");
        assert_eq!(read_u64(91), "chín mươi mốt");
        assert_eq!(read_u64(44), "bốn mươi tư");
        assert_eq!(read_u64(55), "năm mươi lăm");
    }

    #[test]
    fn test_linh_rule() {
        assert_eq!(read_u64(105), "một trăm linh năm");
        assert_eq!(read_u64(101), "một trăm linh một");
        assert_eq!(read_u64(110), "một trăm mười");
        assert_eq!(read_u64(115), "một trăm mười lăm");
        assert_eq!(read_u64(555), "năm trăm năm mươi lăm");
    }

    #[test]
    fn test_scales() {
        assert_eq!(read_u64(1_000), "một nghìn");
        assert_eq!(read_u64(1_985), "một nghìn chín trăm tám mươi lăm");
        assert_eq!(read_u64(2_026), "hai nghìn không trăm hai mươi sáu");
        assert_eq!(read_u64(100_000), "một trăm nghìn");
        assert_eq!(read_u64(1_000_000), "một triệu");
        assert_eq!(read_u64(2_500_000), "hai triệu năm trăm nghìn");
        assert_eq!(read_u64(1_000_005), "một triệu không trăm linh năm");
        assert_eq!(
            read_u64(1_002_003),
            "một triệu không trăm linh hai nghìn không trăm linh ba"
        );
        assert_eq!(
            read_u64(999_999_999_999),
            "chín trăm chín mươi chín tỷ chín trăm chín mươi chín triệu \
             chín trăm chín mươi chín nghìn chín trăm chín mươi chín"
        );
        // Beyond 999 tỷ → digit-by-digit.
        assert_eq!(
            read_u64(1_000_000_000_000),
            "một không không không không không không không không không không không không"
        );
    }

    // ---- thousands separator (the Python bug this port fixes) ----

    #[test]
    fn test_thousands_separator_not_decimal() {
        assert_eq!(normalize_vi("1.000"), "một nghìn");
        assert_eq!(normalize_vi("2.500.000"), "hai triệu năm trăm nghìn");
        assert_eq!(normalize_vi("1.000.000"), "một triệu");
        // Regression guard against the Python decimal reading.
        assert_ne!(normalize_vi("1.000"), "một phẩy không không không");
    }

    #[test]
    fn test_decimal_comma() {
        assert_eq!(normalize_vi("3,5"), "ba phẩy năm");
        assert_eq!(normalize_vi("3,14"), "ba phẩy một bốn");
        assert_eq!(normalize_vi("0,05"), "không phẩy không năm");
        assert_eq!(
            normalize_vi("1.234,5"),
            "một nghìn hai trăm ba mươi tư phẩy năm"
        );
    }

    #[test]
    fn test_version_like_dots() {
        // Invalid thousands grouping → "chấm"-separated (version numbers).
        assert_eq!(normalize_vi("3.14.1"), "ba chấm mười bốn chấm một");
    }

    #[test]
    fn test_leading_zero_integer_is_spelled() {
        assert_eq!(normalize_vi("0123"), "không một hai ba");
    }

    // ---- percent / currency ----

    #[test]
    fn test_percent() {
        assert_eq!(normalize_vi("5%"), "năm phần trăm");
        assert_eq!(normalize_vi("3,5%"), "ba phẩy năm phần trăm");
        assert_eq!(
            normalize_vi("Độ ẩm 75% hôm nay"),
            "Độ ẩm bảy mươi lăm phần trăm hôm nay"
        );
    }

    #[test]
    fn test_currency_dong() {
        assert_eq!(normalize_vi("5.000đ"), "năm nghìn đồng");
        assert_eq!(normalize_vi("5.000₫"), "năm nghìn đồng");
        assert_eq!(normalize_vi("5000 VND"), "năm nghìn đồng");
        assert_eq!(normalize_vi("2.500.000 đồng"), "hai triệu năm trăm nghìn đồng");
    }

    #[test]
    fn test_currency_dollar() {
        assert_eq!(normalize_vi("$5"), "năm đô la");
        assert_eq!(normalize_vi("$1.000"), "một nghìn đô la");
    }

    #[test]
    fn test_dong_letter_boundary() {
        // "đ" must be a whole suffix, not the start of a word.
        assert_eq!(normalize_vi("5 đen"), "năm đen");
    }

    // ---- time / date ----

    #[test]
    fn test_time() {
        assert_eq!(normalize_vi("10:30"), "mười giờ ba mươi phút");
        assert_eq!(normalize_vi("7:05"), "bảy giờ không năm phút");
        assert_eq!(normalize_vi("7:00"), "bảy giờ");
        assert_eq!(
            normalize_vi("10:30:45"),
            "mười giờ ba mươi phút bốn mươi lăm giây"
        );
        // Out of range → not a time; numbers read individually.
        assert_eq!(normalize_vi("99:99"), "chín mươi chín:chín mươi chín");
    }

    #[test]
    fn test_date_full() {
        assert_eq!(
            normalize_vi("25/12/2026"),
            "ngày hai mươi lăm tháng mười hai năm hai nghìn không trăm hai mươi sáu"
        );
        assert_eq!(
            normalize_vi("01/01/2000"),
            "ngày một tháng một năm hai nghìn"
        );
    }

    #[test]
    fn test_date_short() {
        assert_eq!(normalize_vi("5/3"), "ngày năm tháng ba");
        // Month out of range → treated as plain numbers, not a date.
        assert_eq!(normalize_vi("15/30"), "mười lăm/ba mươi");
    }

    #[test]
    fn test_date_preceded_by_ngay_not_duplicated() {
        assert_eq!(
            normalize_vi("Bây giờ là 10:30 ngày 25/12/2026"),
            "Bây giờ là mười giờ ba mươi phút ngày hai mươi lăm tháng mười hai \
             năm hai nghìn không trăm hai mươi sáu"
        );
        // The preceding word is reused with its original case, once.
        assert_eq!(normalize_vi("Ngày 5/3"), "Ngày năm tháng ba");
        // Without a preceding "ngày", the replacement still emits its own.
        assert_eq!(
            normalize_vi("25/12/2026"),
            "ngày hai mươi lăm tháng mười hai năm hai nghìn không trăm hai mươi sáu"
        );
    }

    #[test]
    fn test_month_year() {
        assert_eq!(
            normalize_vi("tháng 12/2026"),
            "tháng mười hai năm hai nghìn không trăm hai mươi sáu"
        );
        assert_eq!(
            normalize_vi("Tháng 5/2026"),
            "Tháng năm năm hai nghìn không trăm hai mươi sáu"
        );
        // Month out of range → not a month/year date; numbers read plainly.
        assert_eq!(
            normalize_vi("tháng 13/2026"),
            "tháng mười ba/hai nghìn không trăm hai mươi sáu"
        );
    }

    // ---- units / phone ----

    #[test]
    fn test_units_attached() {
        assert_eq!(normalize_vi("5km"), "năm ki lô mét");
        assert_eq!(normalize_vi("70kg"), "bảy mươi ki lô gam");
        assert_eq!(normalize_vi("100mb"), "một trăm mê ga bai");
        assert_eq!(normalize_vi("50ml"), "năm mươi mi li lít");
        assert_eq!(normalize_vi("1.000 km"), "một nghìn ki lô mét");
        assert_eq!(normalize_vi("2l"), "hai lít");
    }

    #[test]
    fn test_unit_k_means_thousand() {
        assert_eq!(normalize_vi("5k"), "năm nghìn");
        assert_eq!(
            normalize_vi("Tôi có 100k trong tài khoản"),
            "Tôi có một trăm nghìn trong tài khoản"
        );
    }

    #[test]
    fn test_standalone_multiletter_unit() {
        assert_eq!(normalize_vi("vài kg gạo"), "vài ki lô gam gạo");
    }

    #[test]
    fn test_phone_numbers() {
        assert_eq!(
            normalize_vi("0912345678"),
            "không chín một hai ba bốn năm sáu bảy tám"
        );
        assert_eq!(
            normalize_vi("Gọi 0901 234 567 nhé"),
            "Gọi không chín không một hai ba bốn năm sáu bảy nhé"
        );
    }

    // ---- abbreviations / foreign words ----

    #[test]
    fn test_dotted_abbreviations_and_boundaries() {
        assert_eq!(normalize_vi("TP.HCM"), "thành phố hồ chí minh");
        assert_eq!(normalize_vi("(TP.HCM)"), "(thành phố hồ chí minh)");
        assert_eq!(normalize_vi("TS. Nam"), "tiến sĩ Nam");
        assert_eq!(normalize_vi("ThS. Hoa"), "thạc sĩ Hoa");
        assert_eq!(normalize_vi("sách, vở v.v."), "sách, vở vân vân");
        assert_eq!(normalize_vi("Q.1"), "quận một");
    }

    #[test]
    fn test_word_abbreviations() {
        assert_eq!(normalize_vi("vn"), "việt nam"); // string boundary, no spaces
        assert_eq!(normalize_vi("học ĐH xong"), "học đại học xong");
        assert_eq!(normalize_vi("cắm usb,"), "cắm i u ét bi,");
    }

    #[test]
    fn test_uppercase_only_acronyms() {
        assert_eq!(normalize_vi("công nghệ AI"), "công nghệ a i");
        assert_eq!(normalize_vi("ngành IT"), "ngành i t");
        // Lowercase "ai" is the Vietnamese word "who" — must pass through.
        assert_eq!(normalize_vi("ai đó gọi"), "ai đó gọi");
    }

    #[test]
    fn test_foreign_words() {
        assert_eq!(normalize_vi("livestream"), "lai sờ trim");
        assert_eq!(normalize_vi("xem YouTube"), "xem iu túp");
        assert_eq!(normalize_vi("OK, tôi đồng ý"), "ô kê, tôi đồng ý");
        assert_eq!(normalize_vi("100 USD"), "một trăm đô la mỹ");
        // Whole-word only — the Python substring replace corrupted "book".
        assert_eq!(normalize_vi("book"), "book");
    }

    // ---- passthrough / pipeline / dispatch ----

    #[test]
    fn test_passthrough_and_whitespace() {
        assert_eq!(normalize_vi("xin chào bạn"), "xin chào bạn");
        assert_eq!(normalize_vi(""), "");
        assert_eq!(normalize_vi("   "), "");
        assert_eq!(normalize_vi("a  b   c"), "a b c");
    }

    #[test]
    fn test_mixed_sentence_rule_order() {
        assert_eq!(
            normalize_vi("25/12/2026 lúc 10:30, giá 2.500.000đ giảm 5%"),
            "ngày hai mươi lăm tháng mười hai năm hai nghìn không trăm hai mươi sáu \
             lúc mười giờ ba mươi phút, giá hai triệu năm trăm nghìn đồng \
             giảm năm phần trăm"
        );
    }

    #[test]
    fn test_normalize_dispatch() {
        assert_eq!(normalize("1.000", "vi"), "một nghìn");
        assert_eq!(normalize("1.000", ""), "một nghìn"); // vi is the default
        assert_eq!(normalize("I have  1000 dollars", "en"), "I have 1000 dollars");
        assert_eq!(normalize("1.000", "en-US"), "1.000");
    }
}
