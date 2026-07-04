//! Diagnostic: run the from-scratch mel→embedding→classify wake-word
//! pipeline (wake_model.rs) against a classifier + WAV clip and print the
//! score. Used to validate the reimplementation against livekit-wakeword's
//! own reference fixtures (hey_livekit.onnx + positive/negative.wav).
//!
//! Usage: wakeword_probe <classifier.onnx> <clip.wav>

use liva_native_core::wake_model::TrainedWakeDetector;

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

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 3 {
        eprintln!("Usage: wakeword_probe <classifier.onnx> <clip.wav>");
        std::process::exit(1);
    }

    let (rate, mut samples) = read_wav_pcm16_mono(&args[2]);
    println!("clip: {} samples @ {} Hz ({:.2}s)", samples.len(), rate, samples.len() as f32 / rate as f32);
    assert_eq!(rate, 16000, "wake-word pipeline expects 16kHz mono input");

    // Isolated-word clips (e.g. straight from the training data generator)
    // are usually well under the model's ~2s minimum window — pad with
    // leading silence so short clips can still be probed, matching how the
    // phrase would sit inside a real rolling mic buffer.
    const MIN_SAMPLES: usize = 16000 * 2;
    if samples.len() < MIN_SAMPLES {
        let pad = MIN_SAMPLES - samples.len();
        let mut padded = vec![0.0f32; pad];
        padded.extend_from_slice(&samples);
        println!("padded with {} leading silence samples to reach the ~2s minimum", pad);
        samples = padded;
    }

    let t_load = std::time::Instant::now();
    let mut detector = TrainedWakeDetector::new(&[&args[1]], 0.5).expect("load wake-word pipeline");
    println!("model load time (one-off, at LIVA startup): {:?}", t_load.elapsed());

    let scores = detector.predict_raw(&samples).expect("predict_raw");
    if scores.is_empty() {
        println!("no scores (clip shorter than the ~2s minimum, or classifier failed to load)");
    }
    for (name, score) in &scores {
        println!("{}: {:.4}", name, score);
    }

    // Steady-state per-check latency: this is what runs every ~200ms while
    // the wake gate is listening (see PREDICT_INTERVAL_SAMPLES).
    const RUNS: u32 = 50;
    let t0 = std::time::Instant::now();
    for _ in 0..RUNS {
        let _ = detector.predict_raw(&samples).expect("predict_raw");
    }
    let avg = t0.elapsed() / RUNS;
    println!("avg inference latency over {} runs: {:?}", RUNS, avg);
}
