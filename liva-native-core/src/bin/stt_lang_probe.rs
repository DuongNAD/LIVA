//! Diagnostic: empirically determine the Nemotron encoder `lang_id` mapping.
//!
//! Transcribes a known-language audio sample under several candidate ids and
//! prints each transcript — the id that yields correct text (right language,
//! proper diacritics) is the real mapping. Used to verify `stt::lang::LOCALES`.
//!
//! Usage:
//!   stt_lang_probe <model_dir> <audio_file> [id,id,...]
//!   (default candidates: 37,38,33,7,24,6)

use liva_native_core::stt::SttManager;
use rodio::{Decoder, Source};
use std::io::BufReader;

fn load_audio_16k(path: &str) -> Result<Vec<f32>, String> {
    let file = std::fs::File::open(path).map_err(|e| format!("open {}: {}", path, e))?;
    let dec = Decoder::new(BufReader::new(file)).map_err(|e| format!("decode {}: {}", path, e))?;
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

    // Linear resample to 16 kHz (adequate for a diagnostic transcription).
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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: stt_lang_probe <model_dir> <audio_file> [id,id,...]");
        std::process::exit(1);
    }
    let model_dir = &args[1];
    let audio_path = &args[2];
    let candidates: Vec<i64> = args
        .get(3)
        .map(|s| {
            s.split(',')
                .filter_map(|x| x.trim().parse::<i64>().ok())
                .collect()
        })
        .unwrap_or_else(|| vec![37, 38, 33, 7, 24, 6]);

    println!("Loading audio: {}", audio_path);
    let audio = match load_audio_16k(audio_path) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("Audio load failed: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "  {} samples @16kHz ({:.2}s)",
        audio.len(),
        audio.len() as f32 / 16000.0
    );

    let mut mgr = SttManager::new(model_dir);
    if let Err(e) = mgr.init() {
        eprintln!("STT init failed: {}", e);
        std::process::exit(1);
    }

    for id in candidates {
        mgr.set_lang_id_raw(id);
        match mgr.feed_audio(&audio, true) {
            Ok(Some(text)) => println!("lang_id={:>3} → {}", id, text),
            Ok(None) => println!("lang_id={:>3} → <empty>", id),
            Err(e) => println!("lang_id={:>3} → ERROR: {}", id, e),
        }
    }
}
