
use std::time::{Duration, Instant};
use liva_native_core::stt::engine::SttEngine;
use liva_native_core::tts::engine::TtsEngine;
use liva_native_core::tts::g2p::G2p;

fn verify_g2p_normalization() {
    println!("\n==============================================");
    println!("G2P Normalization & Abbreviation Casing Robustness");
    println!("==============================================");

    // List of abbreviations and their expected expanded forms (partially phonetic)
    // "Dr." -> "Doctor" -> falls back to "doʊktoʊɹ"
    // "Mr." -> "Mister" -> falls back to "mɪstɛɹ"
    // "Ms." -> "Miss" -> falls back to "mɪss"
    // "Mrs." -> "Misses" -> matches dict "misses" -> "mˈɪsɪz" or falls back to "mˈɪsɪz"
    // "etc." -> "etcetera" -> matches dict "etcetera" -> "ɛtsˈɛtəɹə"
    let test_cases = vec![
        // Casing variations for Dr.
        ("Dr.", "doʊktoʊɹ"),
        ("dr.", "doʊktoʊɹ"),
        ("DR.", "doʊktoʊɹ"),
        ("dR.", "doʊktoʊɹ"),
        // Casing variations for Mr.
        ("Mr.", "mɪstɛɹ"),
        ("mr.", "mɪstɛɹ"),
        ("MR.", "mɪstɛɹ"),
        // Casing variations for Ms.
        ("Ms.", "mɪss"),
        ("ms.", "mɪss"),
        ("MS.", "mɪss"),
        // Casing variations for Mrs.
        ("Mrs.", "mˈɪsɪz"),
        ("mrs.", "mˈɪsɪz"),
        ("MRS.", "mˈɪsɪz"),
        // Casing variations for etc.
        ("etc.", "ɛtsˈɛtəɹə"),
        ("Etc.", "ɛtsˈɛtəɹə"),
        ("ETC.", "ɛtsˈɛtəɹə"),
        // Combinations and edge cases
        ("Dr. Watson and Mr. Holmes", "doʊktoʊɹ"),
        ("Dr. Mr. Mrs. Ms. etc.", "doʊktoʊɹ"),
        ("Mr. Holmes, etc.", "mɪstɛɹ"),
    ];

    let mut failed = 0;
    for (input, expected_substring) in test_cases.iter() {
        let ipa = G2p::phonemize(input);
        let contains = ipa.contains(expected_substring);
        println!(
            "Input: {:<25} -> Output: {:<35} | Expected Substring: {:<12} -> {}",
            input,
            ipa,
            expected_substring,
            if contains { "PASS" } else { "FAIL" }
        );
        if !contains {
            failed += 1;
        }
    }

    if failed == 0 {
        println!("✅ Casing robustness and normalization tests passed!");
    } else {
        println!("❌ G2P normalization checks failed for {} cases!", failed);
    }
}

fn stress_test_g2p_dictionary() {
    println!("\n==============================================");
    println!("G2P Dictionary Mapping Stress Testing");
    println!("==============================================");

    // Testing dictionary boundary inputs: empty, special chars, emojis, non-ascii, extremely long
    let binding = "A".repeat(1000);
    let edge_cases = vec![
        ("", ""),
        ("   ", ""),
        ("!!!???...", "!!!???..."),
        ("Hello, world!!!", "həlˈoʊ, wˈɜːld!!!"),
        ("life-is-like", "lˈaɪf-ɪz-lˈaɪk"), // hyphenated words
        ("emoji 😀 test", "ɛmoʊʤɪ 😀 tɛst"),
        ("Cyrillic привет test", "kɪɹɪllɪk pɹɪvɛt tɛst"), // cyrillic letters fallback
        ("12345 numbers", "12345 nʊmbɛɹs"),
        (binding.as_str(), "ɐ"), // large input
    ];

    for (input, _expected) in edge_cases.iter() {
        let ipa = G2p::phonemize(input);
        println!(
            "Input: {:<20} -> Output: {}",
            if input.len() > 30 {
                &input[..30]
            } else {
                input
            },
            ipa
        );
        if input.starts_with("A") {
            assert_eq!(ipa.chars().count(), 1000); // fallback mapped 'a' -> 'ɐ' 1000 times
        }
    }
    println!("✅ G2P dictionary stress test completed without panic!");
}

fn benchmark_g2p_speed() {
    println!("\n==============================================");
    println!("G2P Speed Benchmark");
    println!("==============================================");

    let sentences = ["The quick brown fox jumps over the lazy dog.",
        "Mr. Sherlock Holmes and Dr. John Watson lived at 221B Baker Street.",
        "Mrs. Hudson served tea, while Ms. Mary Morstan explained her case.",
        "We need apples, pears, peaches, oranges, etc. from the grocery store.",
        "Hello world! Life is like a box of chocolates, you never know what you're gonna get."];

    let start = Instant::now();
    let iterations: u32 = 10000; // 10k runs to get stable average
    let mut total_chars = 0;

    for i in 0..iterations {
        let text = sentences[(i as usize) % sentences.len()];
        if i == 0 {
            total_chars = text.len();
        } else {
            total_chars += text.len();
        }
        let _ = G2p::phonemize(text);
    }

    let elapsed = start.elapsed();
    println!(
        "Processed {} iterations ({} total characters) in {:?}",
        iterations, total_chars, elapsed
    );
    println!("Average time per phonemization: {:?}", elapsed / iterations);
    println!(
        "Throughput: {:.2} characters/sec",
        (total_chars as f64) / elapsed.as_secs_f64()
    );
}

#[tokio::main]
async fn main() {
    let pid = std::process::id();
    println!("==============================================");
    println!("LIVA VOICE SYSTEM PROFILER & STRESS RUNNER");
    println!("Process ID (PID): {}", pid);
    println!("==============================================");

    // 1. Run G2P verification
    verify_g2p_normalization();
    stress_test_g2p_dictionary();
    benchmark_g2p_speed();

    let stt_dir = {
        let paths = [
            "models/nemotron-asr",
            "../models/nemotron-asr",
            "../../models/nemotron-asr",
            "models\\nemotron-asr",
            "..\\models\\nemotron-asr",
            "..\\..\\models\\nemotron-asr",
        ];
        let mut resolved = paths[0].to_string();
        for p in &paths {
            if std::path::Path::new(p).exists() {
                resolved = p.to_string();
                break;
            }
        }
        resolved
    };
    let tts_model = {
        let paths = [
            "node_modules/kokoro-js/node_modules/@huggingface/transformers/.cache/onnx-community/Kokoro-82M-v1.0-ONNX/onnx/model.onnx",
            "../node_modules/kokoro-js/node_modules/@huggingface/transformers/.cache/onnx-community/Kokoro-82M-v1.0-ONNX/onnx/model.onnx",
            "../../node_modules/kokoro-js/node_modules/@huggingface/transformers/.cache/onnx-community/Kokoro-82M-v1.0-ONNX/onnx/model.onnx",
            "node_modules\\kokoro-js\\node_modules\\@huggingface\\transformers\\.cache\\onnx-community\\Kokoro-82M-v1.0-ONNX\\onnx\\model.onnx",
            "..\\node_modules\\kokoro-js\\node_modules\\@huggingface\\transformers\\.cache\\onnx-community\\Kokoro-82M-v1.0-ONNX\\onnx\\model.onnx",
        ];
        let mut resolved = paths[0].to_string();
        for p in &paths {
            if std::path::Path::new(p).exists() {
                resolved = p.to_string();
                break;
            }
        }
        resolved
    };
    let tts_voice = {
        let paths = [
            "node_modules/kokoro-js/voices/af_heart.bin",
            "../node_modules/kokoro-js/voices/af_heart.bin",
            "../../node_modules/kokoro-js/voices/af_heart.bin",
            "node_modules\\kokoro-js\\voices\\af_heart.bin",
            "..\\node_modules\\kokoro-js\\voices\\af_heart.bin",
        ];
        let mut resolved = paths[0].to_string();
        for p in &paths {
            if std::path::Path::new(p).exists() {
                resolved = p.to_string();
                break;
            }
        }
        resolved
    };
    println!("\nLoading ASR Engine...");
    let mut stt_engine = SttEngine::new(&stt_dir).expect("Failed to load STT");
    println!("ASR Engine loaded.");

    println!("Loading TTS Engine...");
    let voice_bytes = std::fs::read(&tts_voice).expect("Failed to read voice bin file");
    let len_rounded = (voice_bytes.len() / 4) * 4;
    let voice_bytes_aligned = &voice_bytes[..len_rounded];
    let voice_data_vec: Vec<f32> = voice_bytes_aligned.chunks_exact(4).map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])).collect();

    let mut tts_engine = TtsEngine::new(&tts_model, voice_data_vec).expect("Failed to load TTS");
    println!("TTS Engine initialized.");
    // Force session initialization so thread pools are spawned
    tts_engine
        .ensure_session()
        .expect("Failed to initialize TTS session");
    println!("TTS Engine loaded and session initialized.");

    // Run stress load loop to profile threads/memory
    println!("\n==============================================");
    println!("Running ASR & TTS load loops for 30 seconds...");
    println!("Please check memory and thread counts now!");
    println!("==============================================");

    // Prepare inputs
    let dummy_mel = vec![-5.0f32; 65 * 128];
    let token_ids = vec![0, 50, 83, 54, 54, 57, 16, 65, 57, 60, 54, 46, 4, 0];

    let run_start = Instant::now();
    let stress_duration = Duration::from_secs(30);
    let mut loop_count = 0;

    while run_start.elapsed() < stress_duration {
        // Run ASR chunk
        let _ = stt_engine
            .run_chunk(&dummy_mel, 65)
            .expect("ASR chunk failed");

        // Run TTS generate
        let _ = tts_engine
            .generate(&token_ids, 1.0)
            .expect("TTS generation failed");

        loop_count += 1;
        if loop_count % 5 == 0 {
            println!(
                "  [Profiler Progress] Completed {} execution cycles. Elapsed: {:.1}s",
                loop_count,
                run_start.elapsed().as_secs_f32()
            );
        }

        // yield to executor briefly
        tokio::time::sleep(Duration::from_millis(50)).await;
    }

    println!("\n==============================================");
    println!("Stress Profiling Run Completed ({} cycles)!", loop_count);
    println!("==============================================");
}
