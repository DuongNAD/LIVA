//! Diagnostic: synthesize text with a Piper voice and write a WAV file.
//!
//! Usage: tts_piper_probe <model.onnx> <text> [out.wav]

use liva_native_core::tts::piper::PiperVoice;

fn write_wav(path: &str, samples: &[f32], rate: u32) -> std::io::Result<()> {
    use std::io::Write;

    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }

    let data_len = pcm.len() as u32;
    let mut f = std::fs::File::create(path)?;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_len).to_le_bytes())?;
    f.write_all(b"WAVEfmt ")?;
    f.write_all(&16u32.to_le_bytes())?;
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&rate.to_le_bytes())?;
    f.write_all(&(rate * 2).to_le_bytes())?; // byte rate
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    f.write_all(b"data")?;
    f.write_all(&data_len.to_le_bytes())?;
    f.write_all(&pcm)?;
    Ok(())
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: tts_piper_probe <model.onnx> <text> [out.wav]");
        std::process::exit(1);
    }
    let out = args
        .get(3)
        .cloned()
        .unwrap_or_else(|| "piper_probe.wav".to_string());

    let mut voice = match PiperVoice::load(&args[1]) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("load voice failed: {}", e);
            std::process::exit(1);
        }
    };

    // Run the production text normalizer so the probe matches real output
    // (digits/dates/currency become words before synthesis).
    let norm_lang = if liva_native_core::tts::is_vietnamese_text(&args[2]) {
        "vi"
    } else {
        "en"
    };
    let text = liva_native_core::tts::normalizer::normalize(&args[2], norm_lang);
    if text != args[2] {
        println!("normalized: {}", text);
    }

    let t0 = std::time::Instant::now();
    let samples = match voice.synthesize(&text) {
        Ok((s, _phonemes)) => s,
        Err(e) => {
            eprintln!("synthesize failed: {}", e);
            std::process::exit(1);
        }
    };
    let dt = t0.elapsed();

    let secs = samples.len() as f32 / voice.sample_rate() as f32;
    println!(
        "synthesized {:.2}s of audio in {:?} (RTF {:.3}) @ {} Hz, espeak voice '{}'",
        secs,
        dt,
        dt.as_secs_f32() / secs.max(0.001),
        voice.sample_rate(),
        voice.espeak_voice()
    );

    if let Err(e) = write_wav(&out, &samples, voice.sample_rate()) {
        eprintln!("write wav failed: {}", e);
        std::process::exit(1);
    }
    println!("wrote {}", out);
}
