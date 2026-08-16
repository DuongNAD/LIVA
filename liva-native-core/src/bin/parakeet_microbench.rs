use liva_native_core::stt::parakeet::ParakeetDsp;
use ort::{session::Session, value::Value};
use std::path::PathBuf;
use std::time::Instant;

fn main() {
    let model_path = PathBuf::from("models/parakeet_vi.onnx");

    if !model_path.exists() {
        eprintln!("Model path not found: {:?}", model_path);
        return;
    }

    println!("=== EMPIRICAL CHALLENGER 1: PARAKEET-CTC 0.6B PROBE ===");
    println!("Model: {:?}", model_path);

    let dsp = ParakeetDsp::new();

    // 1. Benchmark DSP for various audio lengths
    println!("\n--- 1. DSP MEL SPECTROGRAM LATENCY ---");
    for &ms in &[160, 320, 640, 1000, 3000, 5000, 10000] {
        let samples = vec![0.0f32; (ms * 16) as usize];
        let t0 = Instant::now();
        for _ in 0..50 {
            let _ = dsp.log_mel_per_feature(&samples);
        }
        let avg = t0.elapsed() / 50;
        println!(
            "  Audio {:5}ms ({} samples): DSP = {:?}",
            ms,
            samples.len(),
            avg
        );
    }

    // 2. Benchmark ONNX Raw Forward Pass for various T frames
    println!("\n--- 2. ONNX FORWARD PASS LATENCY VS T FRAMES ---");
    for threads in [1, 2, 4, 8] {
        println!("\n>>> Testing with intra_threads = {} <<<", threads);
        let mut session = Session::builder()
            .unwrap()
            .with_intra_threads(threads)
            .unwrap()
            .with_inter_threads(1)
            .unwrap()
            .commit_from_file(&model_path)
            .unwrap();

        // Warmup
        let feat = vec![0.0f32; 80 * 16];
        let _ = session.run(ort::inputs![
            "audio_signal" => Value::from_array((vec![1, 80, 16], feat.clone())).unwrap(),
            "length" => Value::from_array((vec![1], vec![16i64])).unwrap(),
        ]);

        for &t_frames in &[16, 32, 64, 100, 200, 400, 800, 1100] {
            let feat = vec![0.0f32; 80 * t_frames];
            let mut lats = Vec::new();
            for _ in 0..5 {
                let t0 = Instant::now();
                let _ = session.run(ort::inputs![
                    "audio_signal" => Value::from_array((vec![1, 80, t_frames], feat.clone())).unwrap(),
                    "length" => Value::from_array((vec![1], vec![t_frames as i64])).unwrap(),
                ]).unwrap();
                lats.push(t0.elapsed());
            }
            lats.sort();
            let p50 = lats[lats.len() / 2];
            let audio_dur_ms = (t_frames * 10) as f32; // ~10ms per frame
            let rtf = (p50.as_secs_f32() * 1000.0) / audio_dur_ms;
            println!(
                "  T={:4} frames (~{:5.0}ms audio): Forward = {:?} (RTF: {:.3})",
                t_frames, audio_dur_ms, p50, rtf
            );
        }
    }
}
