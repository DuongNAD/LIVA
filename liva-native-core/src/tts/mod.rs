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

impl Default for TtsChunker {
    fn default() -> Self {
        Self::new()
    }
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
                if (ch == ',' || ch == ';' || ch == ':' || ch == '—') && word_count >= 6 {
                    split_at = Some((idx + ch.len_utf8(), false));
                    break;
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

/// Một giọng Piper nạp sẵn, chia sẻ được giữa các luồng. `None` = slot ngôn
/// ngữ đó không có voice trên đĩa (không chí mạng — Kokoro là fallback).
pub type SharedPiperVoice = Option<Arc<Mutex<piper::PiperVoice>>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TtsBackend {
    VieNeu,
    Piper,
    Kokoro,
}

impl TtsBackend {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::VieNeu => "vieneu",
            Self::Piper => "piper",
            Self::Kokoro => "kokoro",
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct TtsSynthesisOutcome {
    pub(crate) samples: Vec<f32>,
    pub(crate) sample_rate: u32,
    pub(crate) backend: TtsBackend,
    pub(crate) fallback_count: usize,
}

type TtsSynthesisResult = Result<(Vec<f32>, u32), String>;
type TtsSynthesisFn<'a> = dyn FnMut() -> TtsSynthesisResult + 'a;

struct TtsSynthesisAttempt<'a> {
    backend: TtsBackend,
    synthesize: Box<TtsSynthesisFn<'a>>,
}

impl<'a> TtsSynthesisAttempt<'a> {
    fn new<F>(backend: TtsBackend, synthesize: F) -> Self
    where
        F: FnMut() -> TtsSynthesisResult + 'a,
    {
        Self {
            backend,
            synthesize: Box::new(synthesize),
        }
    }
}

fn run_synthesis_fallback<F>(
    attempts: &mut [TtsSynthesisAttempt<'_>],
    mut is_cancelled: F,
) -> Result<TtsSynthesisOutcome, String>
where
    F: FnMut() -> bool,
{
    let mut errors: Vec<String> = Vec::new();
    for (fallback_count, attempt) in attempts.iter_mut().enumerate() {
        if is_cancelled() {
            return Err("TTS synthesis cancelled".to_string());
        }
        match (attempt.synthesize)() {
            Ok((samples, sample_rate)) => {
                return Ok(TtsSynthesisOutcome {
                    samples,
                    sample_rate,
                    backend: attempt.backend,
                    fallback_count,
                });
            }
            Err(error) => {
                tracing::warn!(
                    backend = attempt.backend.as_str(),
                    error = %error,
                    "TTS backend failed; trying the next candidate"
                );
                errors.push(format!("{}: {error}", attempt.backend.as_str()));
            }
        }
    }

    Err(format!("All TTS backends failed: {}", errors.join("; ")))
}

pub(crate) struct TtsSynthesisPlan {
    text: String,
    vieneu: Option<Arc<Mutex<vieneu::VieNeuVoice>>>,
    piper: Option<Arc<Mutex<piper::PiperVoice>>>,
    kokoro: Arc<Mutex<TtsEngine>>,
}

impl TtsSynthesisPlan {
    pub(crate) fn synthesize<F>(self, is_cancelled: F) -> Result<TtsSynthesisOutcome, String>
    where
        F: FnMut() -> bool,
    {
        let mut attempts = Vec::with_capacity(3);

        if let Some(engine) = self.vieneu {
            let text = self.text.clone();
            attempts.push(TtsSynthesisAttempt::new(TtsBackend::VieNeu, move || {
                let mut engine = engine
                    .lock()
                    .map_err(|_| "VieNeu TTS mutex poisoned".to_string())?;
                let sample_rate = engine.sample_rate();
                engine
                    .synthesize(&text)
                    .map(|samples| (samples, sample_rate))
            }));
        }

        if let Some(voice) = self.piper {
            let text = self.text.clone();
            attempts.push(TtsSynthesisAttempt::new(TtsBackend::Piper, move || {
                let mut voice = voice
                    .lock()
                    .map_err(|_| "Piper TTS mutex poisoned".to_string())?;
                let sample_rate = voice.sample_rate();
                voice
                    .synthesize(&text)
                    .map(|samples| (samples, sample_rate))
            }));
        }

        let text = self.text;
        let engine = self.kokoro;
        attempts.push(TtsSynthesisAttempt::new(TtsBackend::Kokoro, move || {
            let phonemes = G2p::phonemize(&text);
            let token_ids = TtsTokenizer::new().tokenize(&phonemes);
            let (session, voice_data) = {
                let mut engine = engine
                    .lock()
                    .map_err(|_| "Kokoro TTS engine mutex poisoned".to_string())?;
                engine.prepare_inference()?
            };
            let mut session = session
                .lock()
                .map_err(|_| "Kokoro ONNX session mutex poisoned".to_string())?;
            TtsEngine::generate_from_session(&mut session, &voice_data, &token_ids, 1.0)
                .map(|samples| (samples, 24_000))
        }));

        run_synthesis_fallback(&mut attempts, is_cancelled)
    }
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

/// Thư mục model VieNeu (`LIVA_VIENEU_MODEL_DIR`, mặc định `models/vieneu`).
///
/// cwd khác nhau tuỳ điểm vào (gốc repo, `liva-native-core`, hay
/// `liva-desktop/src-tauri`), nên phải dò lên tối đa hai cấp chứ không tin cwd.
pub fn vieneu_model_dir() -> std::path::PathBuf {
    let rel =
        std::env::var("LIVA_VIENEU_MODEL_DIR").unwrap_or_else(|_| "models/vieneu".to_string());
    let raw = std::path::PathBuf::from(&rel);
    if raw.is_absolute() {
        return raw;
    }
    ["", "..", "../.."]
        .iter()
        .map(|p| std::path::Path::new(p).join(&raw))
        .find(|c| c.join("config.json").exists())
        .unwrap_or(raw)
}

/// Mục `tts` trong `data/liva-config.json`, `Null` nếu thiếu hoặc hỏng.
///
/// Thiếu file cấu hình **không phải lỗi**: máy chưa chạy lần nào thì chưa có
/// file, và VieNeu vốn tắt mặc định — trả `Null` rồi rơi về mặc định là đúng.
fn tts_config_section() -> serde_json::Value {
    std::fs::read_to_string(crate::config_file_path())
        .ok()
        .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
        .and_then(|v| v.get("tts").cloned())
        .unwrap_or(serde_json::Value::Null)
}

/// `(bật?, tên giọng)` cho VieNeu — **env thắng cấu hình, cấu hình thắng mặc định**.
///
/// Vì sao env vẫn được ưu tiên: `LIVA_TTS_VIENEU` / `LIVA_VIENEU_VOICE` là
/// đường của người phát triển và của `vieneu_probe`. Nếu một cú bấm nút trên
/// giao diện ghi đè được env, thì mọi phép đo chạy sau đó không tái lập nổi —
/// đúng loại "xanh giả" mà dự án đã dính hai lần ở CI.
fn vieneu_settings() -> (bool, Option<String>) {
    let cfg = tts_config_section();
    let enabled = if std::env::var("LIVA_TTS_VIENEU").is_ok() {
        crate::env_flag("LIVA_TTS_VIENEU", false)
    } else {
        cfg.get("vieneuEnabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false)
    };
    let voice = std::env::var("LIVA_VIENEU_VOICE").ok().or_else(|| {
        cfg.get("vieneuVoice")
            .and_then(|v| v.as_str())
            .filter(|s| !s.trim().is_empty())
            .map(str::to_string)
    });
    (enabled, voice)
}

/// Nạp engine VieNeu. **Nặng** (~500 MB, ~2 s) — gọi trong `spawn_blocking`.
pub fn load_vieneu_engine(
    voice: Option<&str>,
) -> Result<Arc<Mutex<vieneu::VieNeuVoice>>, String> {
    let v = vieneu::VieNeuVoice::load(&vieneu_model_dir(), voice)?;
    tracing::info!(
        "VieNeu-TTS premium tier enabled (voice '{}')",
        v.voice_name()
    );
    Ok(Arc::new(Mutex::new(v)))
}

/// Danh mục giọng preset. Rẻ — chỉ đọc JSON, không đụng ONNX.
pub fn list_vieneu_voices() -> Result<Vec<vieneu::VoiceInfo>, String> {
    vieneu::list_voices(&vieneu_model_dir())
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
        let language = std::env::var("LIVA_TTS_LANGUAGE").unwrap_or_else(|_| "vi".to_string());
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

    /// Load the premium VieNeu-TTS engine when it is switched on.
    /// Heavy (~500 MB, ~2 s) so it stays opt-in; any failure logs and falls back
    /// to the Piper/Kokoro path (returns `None`).
    fn load_vieneu() -> Option<Arc<Mutex<vieneu::VieNeuVoice>>> {
        let (enabled, voice) = vieneu_settings();
        if !enabled {
            return None;
        }
        match load_vieneu_engine(voice.as_deref()) {
            Ok(v) => Some(v),
            Err(e) => {
                tracing::error!("VieNeu-TTS enabled but failed to load ({}); using Piper", e);
                None
            }
        }
    }

    /// Tên giọng VieNeu đang nạp, `None` khi VieNeu đang tắt.
    pub fn vieneu_voice_name(&self) -> Option<String> {
        let engine = self.vieneu.as_ref()?;
        let guard = engine.lock().ok()?;
        Some(guard.voice_name().to_string())
    }

    /// Đổi giọng của engine VieNeu **đang nạp** (rẻ — xem `VieNeuVoice::set_voice`).
    ///
    /// `Err` khi VieNeu đang tắt: người gọi cần phân biệt "đổi được ngay" với
    /// "đã ghi cấu hình, chờ bật" để báo đúng cho người dùng, thay vì im lặng
    /// không làm gì.
    pub fn set_vieneu_voice(&mut self, name: &str) -> Result<(), String> {
        let engine = self
            .vieneu
            .as_ref()
            .ok_or("VieNeu đang tắt — chưa có engine để đổi giọng")?;
        let mut guard = engine
            .lock()
            .map_err(|_| "VieNeu TTS mutex poisoned".to_string())?;
        guard.set_voice(name)
    }

    /// Gắn (hoặc gỡ) engine VieNeu lúc chạy, để bật/tắt được từ giao diện mà
    /// không phải khởi động lại tiến trình.
    pub fn set_vieneu_engine(&mut self, engine: Option<Arc<Mutex<vieneu::VieNeuVoice>>>) {
        self.vieneu = engine;
    }

    /// Scan a directory for Piper voices: first `vi*.onnx` → Vietnamese slot,
    /// first `en*.onnx` → English slot. Missing voices are non-fatal (Kokoro
    /// stays as the English fallback).
    fn load_piper_voices(dir: &str) -> (SharedPiperVoice, SharedPiperVoice) {
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

    /// Chuẩn bị một clause cho chuỗi runtime fallback VieNeu → Piper → Kokoro.
    /// Chỉ clone các handle nhẹ; inference chạy sau khi caller đã nhả khoá
    /// `TtsManager`, nên không giữ tokio mutex trong lúc model chạy.
    pub(crate) fn synthesis_plan(&self, chunk: &str) -> Option<TtsSynthesisPlan> {
        let cleaned = chunk.replace(['[', ']'], "");
        if cleaned.trim().is_empty() {
            return None;
        }

        let language = if is_vietnamese_text(&cleaned) {
            "vi"
        } else {
            self.language.as_str()
        };
        let text = normalizer::normalize(&cleaned, language);

        Some(TtsSynthesisPlan {
            vieneu: self.vieneu_for_chunk(&text),
            piper: self.piper_for_chunk(&text),
            kokoro: Arc::clone(&self.engine),
            text,
        })
    }

    pub fn from_bin<P: AsRef<Path>>(
        model_path: P,
        bin_path: P,
        sink: Option<Arc<rodio::Sink>>,
    ) -> Result<Self, String> {
        // Voice embedding NÀY CHỈ DÙNG CHO KOKORO. Thiếu file thì trước đây cả
        // `from_bin` trả Err, khiến hai điểm vào đặt `TtsManager = None` và mất
        // LUÔN Piper lẫn VieNeu — tức là thiếu một file fallback tiếng Anh làm
        // hỏng toàn bộ giọng nói, kể cả giọng tiếng Việt vốn không cần nó.
        // Giờ đọc được thì dùng, không đọc được thì đưa vector rỗng: Kokoro sẽ
        // tự báo lỗi khi thật sự được gọi, còn Piper/VieNeu chạy bình thường.
        let voice_bytes = match std::fs::read(bin_path.as_ref()) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(
                    "Khong doc duoc voice embedding Kokoro {:?} ({}). Kokoro se khong dung duoc, \
                     nhung Piper/VieNeu van hoat dong binh thuong.",
                    bin_path.as_ref(),
                    e
                );
                Vec::new()
            }
        };
        let len_rounded = (voice_bytes.len() / 4) * 4;
        let voice_bytes_aligned = &voice_bytes[..len_rounded];
        #[allow(clippy::manual_is_multiple_of)]
        let voice_data = if voice_bytes_aligned.as_ptr() as usize % std::mem::align_of::<f32>() == 0
        {
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
        let decoder =
            rodio::Decoder::new(std::io::BufReader::new(file)).map_err(|e| e.to_string())?;

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

    /// Các backend giọng nói đã NẠP ĐƯỢC, theo thứ tự ưu tiên khi phát.
    ///
    /// Cho bảng sức khoẻ: trước đây ô "Voice Engine" luôn báo `"Active"` với
    /// `latencyMs: 5` kể cả khi không một backend nào nạp được. Danh sách rỗng
    /// nghĩa là TtsManager có tồn tại nhưng KHÔNG nói được câu nào.
    ///
    /// Kokoro nạp session lazy nên chỉ kiểm được sự tồn tại của file model —
    /// đó cũng đúng là điều kiện cần để nó không nổ lúc phát.
    pub fn loaded_backends(&self) -> Vec<&'static str> {
        let mut ds = Vec::new();
        if self.vieneu.is_some() {
            ds.push("VieNeu");
        }
        if self.piper_vi.is_some() {
            ds.push("Piper-vi");
        }
        if self.piper_en.is_some() {
            ds.push("Piper-en");
        }
        if self.engine.lock().is_ok_and(|e| e.model_available()) {
            ds.push("Kokoro");
        }
        ds
    }

    pub fn check_idle_unload(&self) {
        if let Ok(mut engine) = self.engine.lock() {
            // Unload if idle for 5 minutes (300 seconds)
            engine.check_idle_unload(std::time::Duration::from_secs(300));
        }
    }

    async fn process_chunk(&self, chunk: &str, initial_stop_id: usize) -> Result<usize, String> {
        let Some(plan) = self.synthesis_plan(chunk) else {
            return Ok(initial_stop_id);
        };

        let player = self.player.clone();
        let cancellation_player = player.clone();
        let outcome = tokio::task::spawn_blocking(move || {
            plan.synthesize(|| cancellation_player.get_stop_id() != initial_stop_id)
        })
        .await
        .map_err(|e| format!("TTS fallback task panicked: {}", e))??;

        if player.get_stop_id() == initial_stop_id {
            tracing::info!(
                backend = outcome.backend.as_str(),
                fallback_count = outcome.fallback_count,
                sample_rate = outcome.sample_rate,
                "TTS clause synthesized"
            );
            Ok(player.play_with_rate(outcome.samples, outcome.sample_rate))
        } else {
            Ok(initial_stop_id)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    #[test]
    fn tts_fallback_chuyen_sang_backend_ke_tiep_khi_primary_loi() {
        let primary_calls = Cell::new(0usize);
        let secondary_calls = Cell::new(0usize);
        let mut attempts = vec![
            TtsSynthesisAttempt::new(TtsBackend::VieNeu, || {
                primary_calls.set(primary_calls.get() + 1);
                Err("vieneu runtime error".to_string())
            }),
            TtsSynthesisAttempt::new(TtsBackend::Piper, || {
                secondary_calls.set(secondary_calls.get() + 1);
                Ok((vec![0.25, -0.25], 22_050))
            }),
        ];

        let outcome = run_synthesis_fallback(&mut attempts, || false)
            .expect("Piper fallback phai thanh cong");

        assert_eq!(outcome.backend, TtsBackend::Piper);
        assert_eq!(outcome.fallback_count, 1);
        assert_eq!(outcome.sample_rate, 22_050);
        assert_eq!(outcome.samples, vec![0.25, -0.25]);
        assert_eq!(primary_calls.get(), 1);
        assert_eq!(secondary_calls.get(), 1);
    }

    #[test]
    fn tts_fallback_dung_truoc_backend_ke_tiep_khi_turn_da_huy() {
        let secondary_calls = Cell::new(0usize);
        let cancellation_checks = Cell::new(0usize);
        let mut attempts = vec![
            TtsSynthesisAttempt::new(TtsBackend::VieNeu, || {
                Err("vieneu runtime error".to_string())
            }),
            TtsSynthesisAttempt::new(TtsBackend::Piper, || {
                secondary_calls.set(secondary_calls.get() + 1);
                Ok((vec![0.5], 22_050))
            }),
        ];

        let result = run_synthesis_fallback(&mut attempts, || {
            let check = cancellation_checks.get();
            cancellation_checks.set(check + 1);
            check > 0
        });

        assert_eq!(result.unwrap_err(), "TTS synthesis cancelled");
        assert_eq!(
            secondary_calls.get(),
            0,
            "khong duoc chay fallback sau khi turn bi huy"
        );
    }

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

#[cfg(test)]
mod tts_manager_tests {
    use super::TtsManager;

    /// HỒI QUY: thiếu voice embedding của Kokoro KHÔNG được làm hỏng toàn bộ TTS.
    ///
    /// Trước đây `from_bin` đọc file này eager và trả `Err` khi thiếu; hai điểm
    /// vào đều biến `Err` đó thành `TtsManager = None`, tức là mất luôn Piper và
    /// VieNeu. Hệ quả thực tế: người dùng có đủ giọng tiếng Việt vẫn bị câm tiếng
    /// chỉ vì thiếu một file fallback tiếng Anh (`af_heart.bin`) — file này lại
    /// đến từ một gói npm, thứ mà bản build Rust thuần không hề có.
    #[test]
    fn thieu_voice_kokoro_van_dung_duoc_tts() {
        let res = TtsManager::from_bin("khong-ton-tai-model.onnx", "khong-ton-tai-voice.bin", None);
        assert!(
            res.is_ok(),
            "thieu voice embedding Kokoro khong duoc lam hong ca TtsManager: {:?}",
            res.err()
        );
    }
}
