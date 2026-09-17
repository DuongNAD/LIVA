//! Piper (VITS) ONNX text-to-speech — the local-first voice path.
//!
//! A Piper voice is an `.onnx` model plus an `.onnx.json` config describing
//! its espeak phonemizer voice, sample rate, phoneme→id map, and inference
//! scales. Synthesis: text → espeak IPA (per the config voice) → one id per
//! phoneme codepoint interspersed with pad, framed by bos/eos → VITS
//! inference → f32 mono samples at the config sample rate.

use super::espeak::espeak_ipa;
use ort::{session::Session, value::Value};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const BOS_ID: i64 = 1; // "^"
const EOS_ID: i64 = 2; // "$"
const PAD_ID: i64 = 0; // "_"

#[derive(Deserialize)]
struct RawConfig {
    audio: RawAudio,
    espeak: RawEspeak,
    #[serde(default)]
    inference: RawInference,
    phoneme_id_map: HashMap<String, Vec<i64>>,
}

#[derive(Deserialize)]
struct RawAudio {
    sample_rate: u32,
}

#[derive(Deserialize)]
struct RawEspeak {
    voice: String,
}

#[derive(Deserialize)]
struct RawInference {
    #[serde(default = "default_noise_scale")]
    noise_scale: f32,
    #[serde(default = "default_length_scale")]
    length_scale: f32,
    #[serde(default = "default_noise_w")]
    noise_w: f32,
}

impl Default for RawInference {
    fn default() -> Self {
        Self {
            noise_scale: default_noise_scale(),
            length_scale: default_length_scale(),
            noise_w: default_noise_w(),
        }
    }
}

fn default_noise_scale() -> f32 {
    0.667
}
fn default_length_scale() -> f32 {
    1.0
}
fn default_noise_w() -> f32 {
    0.8
}

/// espeak marks mid-text language switches as "(en)" / "(vi)" — annotations,
/// not phonemes; they must be stripped before id mapping.
fn lang_switch_re() -> &'static regex::Regex {
    static RE: OnceLock<regex::Regex> = OnceLock::new();
    RE.get_or_init(|| regex::Regex::new(r"\([a-z-]+\)").unwrap())
}

/// Chặn `sample_rate = 0` ngay tại điểm nạp config, kèm tên file để sửa được.
///
/// Giá trị này đến từ `<voice>.onnx.json` — một file **tải về**, và nó đi thẳng
/// tới [`crate::tts::audio::TtsAudioPlayer::play_with_rate`] rồi vào
/// `rodio::buffer::SamplesBuffer::new`, nơi có `assert!(sample_rate != 0)`
/// (rodio-0.17.3 `src/buffer.rs:43`, doc ghi rõ "Panics if the samples rate is
/// zero"). Panic ấy nổ **bên trong** vùng khoá của player, nên hậu quả không chỉ
/// là hỏng một lượt nói mà là poison mutex ⇒ LIVA câm tới khi khởi động lại.
///
/// Kiểm ở đây thay vì ở player: chỗ này biết **tên file** nào khai sai, còn
/// player thì chỉ thấy một con số trần.
fn validated_sample_rate(rate: u32, cfg_path: &Path) -> Result<u32, String> {
    if rate == 0 {
        return Err(format!(
            "piper voice config {:?} khai sample_rate = 0 — không phát được tiếng; \
             sửa trường audio.sample_rate (Piper thường là 22050) hoặc tải lại voice",
            cfg_path
        ));
    }
    Ok(rate)
}

pub struct PiperVoice {
    model_path: PathBuf,
    session: Session,
    sample_rate: u32,
    espeak_voice: String,
    char_ids: HashMap<char, Vec<i64>>,
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
}

impl PiperVoice {
    /// Load a voice from `<name>.onnx`; expects `<name>.onnx.json` beside it.
    pub fn load<P: AsRef<Path>>(onnx_path: P) -> Result<Self, String> {
        let onnx_path = onnx_path.as_ref();
        let config_path = PathBuf::from(format!("{}.json", onnx_path.display()));
        let raw = std::fs::read_to_string(&config_path)
            .map_err(|e| format!("read {:?}: {}", config_path, e))?;
        let cfg: RawConfig =
            serde_json::from_str(&raw).map_err(|e| format!("parse {:?}: {}", config_path, e))?;

        // Kiểm TRƯỚC khi dựng session ONNX: hỏng thì báo ngay, không tốn vài giây
        // nạp model rồi mới từ chối.
        let sample_rate = validated_sample_rate(cfg.audio.sample_rate, &config_path)?;

        let mut char_ids = HashMap::new();
        for (key, ids) in cfg.phoneme_id_map {
            let mut chars = key.chars();
            if let (Some(c), None) = (chars.next(), chars.next()) {
                char_ids.insert(c, ids);
            }
        }

        let session = Session::builder()
            .map_err(|e| e.to_string())?
            .with_intra_threads(2)
            .map_err(|e| e.to_string())?
            .with_inter_threads(1)
            .map_err(|e| e.to_string())?
            .commit_from_file(onnx_path)
            .map_err(|e| format!("load piper model {:?}: {}", onnx_path, e))?;

        Ok(Self {
            model_path: onnx_path.to_path_buf(),
            session,
            sample_rate,
            espeak_voice: cfg.espeak.voice,
            char_ids,
            noise_scale: cfg.inference.noise_scale,
            length_scale: cfg.inference.length_scale,
            noise_w: cfg.inference.noise_w,
        })
    }

    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    pub fn espeak_voice(&self) -> &str {
        &self.espeak_voice
    }

    /// Piper id framing: `[bos, (phoneme ids…, pad)*, eos]`.
    fn phoneme_ids(&self, phonemes: &str) -> Vec<i64> {
        let mut ids = vec![BOS_ID];
        for ch in phonemes.chars() {
            if let Some(v) = self.char_ids.get(&ch) {
                ids.extend_from_slice(v);
                ids.push(PAD_ID);
            }
            // Codepoints absent from the map (rare espeak artifacts) are skipped.
        }
        ids.push(EOS_ID);
        ids
    }

    /// Synthesize UTF-8 text to mono f32 samples at [`Self::sample_rate`].
    ///
    /// Trả kèm chuỗi phoneme IPA đã dùng để tổng hợp — nguyên liệu cho timeline
    /// viseme (VC-8); Piper là backend duy nhất nắm chính xác chuỗi này vì nó tự
    /// phonemize qua espeak voice của mình.
    pub fn synthesize(&mut self, text: &str) -> Result<(Vec<f32>, String), String> {
        let ipa = espeak_ipa(&self.espeak_voice, text)?;
        let cleaned = lang_switch_re().replace_all(&ipa, " ");
        let flat = cleaned.replace(['\r', '\n'], " ");

        let ids = self.phoneme_ids(&flat);
        if ids.len() <= 2 {
            return Err(format!("no phonemes mapped for text: {:?}", text));
        }

        let seq_len = ids.len();
        let inputs = ort::inputs![
            "input" => Value::from_array((vec![1, seq_len], ids)).map_err(|e| e.to_string())?,
            "input_lengths" => Value::from_array((vec![1], vec![seq_len as i64]))
                .map_err(|e| e.to_string())?,
            "scales" => Value::from_array((
                vec![3],
                vec![self.noise_scale, self.length_scale, self.noise_w],
            ))
            .map_err(|e| e.to_string())?,
        ];

        let outputs = self
            .session
            .run(inputs)
            .map_err(|e| format!("piper inference failed ({:?}): {}", self.model_path, e))?;

        let out_val = outputs
            .get("output")
            .ok_or_else(|| "missing 'output' tensor from piper model".to_string())?;
        let (_, samples) = out_val
            .try_extract_tensor::<f32>()
            .map_err(|e| e.to_string())?;

        Ok((samples.to_vec(), flat))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `sample_rate` đi thẳng từ `.onnx.json` (file TẢI VỀ) tới
    /// `rodio::buffer::SamplesBuffer::new`, mà hàm đó `assert!(sample_rate != 0)`
    /// (rodio-0.17.3 `src/buffer.rs:43`). Panic ấy nổ **bên trong** vùng khoá của
    /// `TtsAudioPlayer::play_with_rate`, làm poison mutex và khiến LIVA câm cho
    /// tới khi khởi động lại. Chặn ngay tại điểm nạp config là rẻ nhất.
    #[test]
    fn sample_rate_0_bi_tu_choi_va_neu_ten_file() {
        let p = Path::new("models/piper/vi_VN-vais1000-medium.onnx.json");
        let e = validated_sample_rate(0, p).expect_err("rate 0 phai bi tu choi");
        assert!(
            e.contains("vi_VN-vais1000-medium.onnx.json"),
            "thong bao loi phai neu ro FILE nao hong, nguoi dung moi sua duoc: {e}"
        );
        assert!(
            e.contains("sample_rate"),
            "va phai neu ro TRUONG nao sai: {e}"
        );
    }

    #[test]
    fn sample_rate_hop_le_di_qua_nguyen_ven() {
        let p = Path::new("bat-ky.onnx.json");
        assert_eq!(
            validated_sample_rate(22_050, p).expect("22050 la sample rate Piper hop le"),
            22_050
        );
        assert_eq!(
            validated_sample_rate(24_000, p).expect("24000 hop le"),
            24_000
        );
    }
}
