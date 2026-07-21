//! VieNeu-TTS smoke test: load the native Rust engine, synthesize a Vietnamese
//! and a vi/en code-switch line, write 48 kHz WAVs, and report RTF. Lets the
//! user judge quality by ear (Claude can't hear audio).
//!
//! Run:  cargo run --bin vieneu_probe        (after `cargo build`)
//! Env:  LIVA_VIENEU_MODEL_DIR (default models/vieneu), LIVA_VIENEU_VOICE,
//!       LIVA_VIENEU_SEED, LIVA_VIENEU_THREADS.
//!
//! Truyền câu riêng để đo: `vieneu_probe "câu một" "câu hai"` — mỗi tham số là
//! một ca đo, ghi ra `…/rust_out_arg{N}.wav`. Dùng để kiểm tra RTF có tuyến
//! tính theo độ dài câu hay không: KV-cache bị `clone()` mỗi lớp mỗi bước nên
//! khối lượng sao chép về lý thuyết là O(T²) — câu ngắn có thể che mất điều đó.
//!
//! **Đo RTF phải chạy ở build RELEASE.** Ở debug, ONNX Runtime và vòng lặp
//! decode chậm hơn nhiều lần; con số debug không dùng để kết luận được.

use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

use liva_native_core::tts::vieneu::VieNeuVoice;

fn resolve_dir() -> PathBuf {
    let rel = std::env::var("LIVA_VIENEU_MODEL_DIR").unwrap_or_else(|_| "models/vieneu".to_string());
    for prefix in ["", "..", "../.."] {
        let p = PathBuf::from(prefix).join(&rel);
        if p.join("config.json").exists() {
            return p;
        }
    }
    PathBuf::from(rel)
}

fn write_wav(path: &str, samples: &[f32], sample_rate: u32) -> std::io::Result<()> {
    if let Some(parent) = std::path::Path::new(path).parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut f = std::io::BufWriter::new(std::fs::File::create(path)?);
    let n = samples.len() as u32;
    let byte_rate = sample_rate * 2;
    let data_bytes = n * 2;
    f.write_all(b"RIFF")?;
    f.write_all(&(36 + data_bytes).to_le_bytes())?;
    f.write_all(b"WAVE")?;
    f.write_all(b"fmt ")?;
    f.write_all(&16u32.to_le_bytes())?; // PCM chunk size
    f.write_all(&1u16.to_le_bytes())?; // PCM
    f.write_all(&1u16.to_le_bytes())?; // mono
    f.write_all(&sample_rate.to_le_bytes())?;
    f.write_all(&byte_rate.to_le_bytes())?;
    f.write_all(&2u16.to_le_bytes())?; // block align
    f.write_all(&16u16.to_le_bytes())?; // bits per sample
    f.write_all(b"data")?;
    f.write_all(&data_bytes.to_le_bytes())?;
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        f.write_all(&v.to_le_bytes())?;
    }
    f.flush()?;
    Ok(())
}

fn main() {
    if std::env::var("LIVA_VIENEU_SEED").is_err() {
        // Deterministic by default so repeated probe runs are comparable.
        unsafe { std::env::set_var("LIVA_VIENEU_SEED", "42") };
    }

    let dir = resolve_dir();
    println!("Loading VieNeu-TTS from {:?} ...", dir);
    let voice = std::env::var("LIVA_VIENEU_VOICE").ok();
    let t0 = Instant::now();
    let mut engine = match VieNeuVoice::load(&dir, voice.as_deref()) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("FAILED to load VieNeu: {}", e);
            std::process::exit(1);
        }
    };
    println!(
        "Loaded voice '{}' in {:.2}s (sample_rate {} Hz)",
        engine.voice_name(),
        t0.elapsed().as_secs_f32(),
        engine.sample_rate()
    );

    // Không có tham số -> hai ca mặc định để nghe thử chất lượng. Có tham số ->
    // mỗi tham số là một câu cần đo (dùng để khảo sát RTF theo độ dài).
    let args: Vec<String> = std::env::args().skip(1).collect();
    let default_cases = vec![
        (
            "vi".to_string(),
            "docs/99-luu-tru/bao-cao-lich-su/vieneu_poc_samples/rust_out_vi.wav".to_string(),
            "Xin chào, tôi là LIVA, trợ lý ảo của bạn. Hôm nay tôi có thể giúp gì cho bạn?"
                .to_string(),
        ),
        (
            "en_codeswitch".to_string(),
            "docs/99-luu-tru/bao-cao-lich-su/vieneu_poc_samples/rust_out_en_codeswitch.wav"
                .to_string(),
            "Được rồi, tôi sẽ mở file report của bạn và chạy build ngay bây giờ.".to_string(),
        ),
    ];
    let cases: Vec<(String, String, String)> = if args.is_empty() {
        default_cases
    } else {
        args.iter()
            .enumerate()
            .map(|(i, text)| {
                (
                    format!("arg{i}"),
                    format!(
                        "docs/99-luu-tru/bao-cao-lich-su/vieneu_poc_samples/rust_out_arg{i}.wav"
                    ),
                    text.clone(),
                )
            })
            .collect()
    };

    let sr = engine.sample_rate();
    for (tag, out, text) in cases {
        let t = Instant::now();
        match engine.synthesize(&text) {
            Ok(samples) => {
                let dur = samples.len() as f32 / sr as f32;
                let wall = t.elapsed().as_secs_f32();
                let rtf = if dur > 0.0 { wall / dur } else { f32::NAN };
                match write_wav(&out, &samples, sr) {
                    Ok(()) => println!(
                        "[{}] {} ky tu, {} samples, {:.2}s audio, {:.2}s wall, RTF {:.3} -> {}",
                        tag,
                        text.chars().count(),
                        samples.len(),
                        dur,
                        wall,
                        rtf,
                        out
                    ),
                    Err(e) => eprintln!("[{}] write wav failed: {}", tag, e),
                }
            }
            Err(e) => eprintln!("[{}] synth failed: {}", tag, e),
        }
    }
    println!("Done.");
}
