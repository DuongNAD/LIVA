//! Helpers shared by the STT integration tests and the latency benchmark.
//!
//! Files directly under `tests/` are each compiled as their own test binary; a subdirectory
//! module like this one is not, so it can be shared without becoming a test target of its own.

use rodio::{Decoder, Source};
use std::io::BufReader;
use std::path::{Path, PathBuf};

/// Resolve the Parakeet model and vocab paths, whether the test runs from the crate directory
/// or from the workspace root.
pub fn resolve_model_paths() -> (PathBuf, PathBuf) {
    let mut model_path = PathBuf::from("models/parakeet_vi.onnx");
    let mut vocab_path = PathBuf::from("models/parakeet_vi_vocab.json");
    if !model_path.exists() {
        model_path = PathBuf::from("../models/parakeet_vi.onnx");
        vocab_path = PathBuf::from("../models/parakeet_vi_vocab.json");
    }
    (model_path, vocab_path)
}

/// Decode a WAV file to mono f32 at 16 kHz, resampling linearly when the source rate differs.
pub fn load_audio_wav_16k(path: &Path) -> Result<Vec<f32>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {:?}: {}", path, e))?;
    let dec =
        Decoder::new(BufReader::new(file)).map_err(|e| format!("decode {:?}: {}", path, e))?;
    let sr = dec.sample_rate();
    let ch = dec.channels() as usize;
    let samples: Vec<f32> = dec.convert_samples::<f32>().collect();

    let mono: Vec<f32> = if ch > 1 {
        samples
            .chunks(ch)
            .map(|c| c.iter().sum::<f32>() / ch as f32)
            .collect()
    } else {
        samples
    };

    if sr == 16000 {
        return Ok(mono);
    }

    let ratio = 16000.0f64 / sr as f64;
    let out_len = (mono.len() as f64 * ratio) as usize;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src = i as f64 / ratio;
        let i0 = src.floor() as usize;
        let frac = (src - i0 as f64) as f32;
        let s0 = mono.get(i0).copied().unwrap_or(0.0);
        let s1 = mono.get(i0 + 1).copied().unwrap_or(s0);
        out.push(s0 + (s1 - s0) * frac);
    }
    Ok(out)
}

/// Locate a data file relative to either the crate directory or the workspace root.
pub fn find_audio_file(rel: &str) -> PathBuf {
    let p = PathBuf::from(rel);
    if p.exists() {
        p
    } else {
        PathBuf::from("..").join(rel)
    }
}
