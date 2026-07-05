//! Diagnostic: transcribe Vietnamese audio with Parakeet-CTC vi and print the
//! text. Use it to verify the DSP/CTC path against known-good NeMo output
//! (plan step B4) and to spot-check accuracy vs Nemotron.
//!
//! Usage:
//!   parakeet_probe <audio.wav> [more_audio.wav ...]
//!   parakeet_probe --model <path.onnx> --vocab <vocab.json> <audio.wav> ...
//!
//! Defaults: models/parakeet_vi.onnx + models/parakeet_vi_vocab.json.

use liva_native_core::stt::parakeet::ParakeetVi;
use rodio::{Decoder, Source};
use std::io::BufReader;
use std::path::PathBuf;
use std::time::Instant;

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
    let mut model = PathBuf::from("models/parakeet_vi.onnx");
    let mut vocab = PathBuf::from("models/parakeet_vi_vocab.json");
    let mut audio_files: Vec<String> = Vec::new();

    let mut args = std::env::args().skip(1);
    while let Some(a) = args.next() {
        match a.as_str() {
            "--model" => model = PathBuf::from(args.next().unwrap_or_default()),
            "--vocab" => vocab = PathBuf::from(args.next().unwrap_or_default()),
            _ => audio_files.push(a),
        }
    }

    if audio_files.is_empty() {
        eprintln!("Usage: parakeet_probe [--model p.onnx] [--vocab v.json] <audio.wav> ...");
        std::process::exit(1);
    }

    println!("Loading Parakeet-CTC vi: {:?}", model);
    let load_start = Instant::now();
    let mut pk = match ParakeetVi::load(&model, &vocab) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("Load failed: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "  loaded in {:.2}s (vocab {} tokens)\n",
        load_start.elapsed().as_secs_f32(),
        pk.vocab_len()
    );

    for path in &audio_files {
        let audio = match load_audio_16k(path) {
            Ok(a) => a,
            Err(e) => {
                eprintln!("{}: audio load failed: {}", path, e);
                continue;
            }
        };
        let dur = audio.len() as f32 / 16000.0;
        let t0 = Instant::now();
        match pk.transcribe(&audio) {
            Ok(text) => {
                let rtf = t0.elapsed().as_secs_f32() / dur.max(1e-6);
                println!("{} ({:.2}s, RTF {:.2})", path, dur, rtf);
                println!("  → {}\n", text);
            }
            Err(e) => eprintln!("{}: transcribe failed: {}", path, e),
        }
    }
}
