//! Diagnostic: run the GTCRN denoiser over a 16kHz mono WAV and report
//! RMS energy before/after, plus write the denoised result for listening.
//!
//! Usage: gtcrn_probe <in.wav> [out.wav]

use liva_native_core::webrtc::denoise::GtcrnDenoiser;

fn read_wav_pcm16_mono(path: &str) -> (u32, Vec<f32>) {
    let bytes = std::fs::read(path).expect("read wav");
    assert_eq!(&bytes[0..4], b"RIFF");
    assert_eq!(&bytes[8..12], b"WAVE");
    let mut pos = 12;
    let mut sample_rate = 0u32;
    let mut data: Vec<f32> = Vec::new();
    while pos + 8 <= bytes.len() {
        let chunk_id = &bytes[pos..pos + 4];
        let chunk_size = u32::from_le_bytes(bytes[pos + 4..pos + 8].try_into().unwrap()) as usize;
        let body_start = pos + 8;
        if chunk_id == b"fmt " {
            sample_rate = u32::from_le_bytes(bytes[body_start + 4..body_start + 8].try_into().unwrap());
        } else if chunk_id == b"data" {
            let body = &bytes[body_start..body_start + chunk_size];
            data = body
                .chunks_exact(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]) as f32 / 32768.0)
                .collect();
        }
        pos = body_start + chunk_size + (chunk_size % 2);
    }
    (sample_rate, data)
}

fn write_wav(path: &str, samples: &[f32], rate: u32) {
    let mut pcm = Vec::with_capacity(samples.len() * 2);
    for &s in samples {
        let v = (s.clamp(-1.0, 1.0) * 32767.0) as i16;
        pcm.extend_from_slice(&v.to_le_bytes());
    }
    let data_len = pcm.len() as u32;
    use std::io::Write;
    let mut f = std::fs::File::create(path).unwrap();
    f.write_all(b"RIFF").unwrap();
    f.write_all(&(36 + data_len).to_le_bytes()).unwrap();
    f.write_all(b"WAVEfmt ").unwrap();
    f.write_all(&16u32.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&1u16.to_le_bytes()).unwrap();
    f.write_all(&rate.to_le_bytes()).unwrap();
    f.write_all(&(rate * 2).to_le_bytes()).unwrap();
    f.write_all(&2u16.to_le_bytes()).unwrap();
    f.write_all(&16u16.to_le_bytes()).unwrap();
    f.write_all(b"data").unwrap();
    f.write_all(&data_len.to_le_bytes()).unwrap();
    f.write_all(&pcm).unwrap();
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("Usage: gtcrn_probe <in.wav> [out.wav]");
        std::process::exit(1);
    }
    let out_path = args.get(2).cloned().unwrap_or_else(|| "gtcrn_probe_out.wav".to_string());

    let (rate, samples) = read_wav_pcm16_mono(&args[1]);
    println!("input: {} samples @ {} Hz, RMS {:.4}", samples.len(), rate, rms(&samples));
    assert_eq!(rate, 16000, "GTCRN model expects 16kHz mono input");

    let model_path = liva_native_core::webrtc::denoise::resolve_model_path();
    let mut denoiser = GtcrnDenoiser::new(&model_path).expect("load GTCRN model");

    let t0 = std::time::Instant::now();
    let out = denoiser.process_audio(&samples).expect("process_audio");
    let dt = t0.elapsed();

    let secs = samples.len() as f32 / rate as f32;
    println!(
        "output: {} samples, RMS {:.4} (in {:.4}) in {:?} (RTF {:.4})",
        out.len(),
        rms(&out),
        rms(&samples),
        dt,
        dt.as_secs_f32() / secs.max(0.001)
    );
    write_wav(&out_path, &out, rate);
    println!("wrote {}", out_path);
}
