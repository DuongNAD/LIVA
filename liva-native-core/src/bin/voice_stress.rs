#![allow(dead_code, unused_imports, unused_variables)]

use std::time::Instant;
use liva_native_core::stt::engine::SttEngine;
use liva_native_core::tts::TtsChunker;
use liva_native_core::tts::engine::TtsEngine;
use liva_native_core::tts::g2p::G2p;

fn test_g2p_accuracy() {
    println!("\n--- Running G2P Accuracy Verification ---");

    // Test Dr. -> Doctor
    let ipa_dr = G2p::phonemize("Hello Dr. Watson.");
    println!("'Hello Dr. Watson.' -> '{}'", ipa_dr);
    assert!(
        ipa_dr.contains("doʊktoʊɹ"),
        "Dr. should expand to Doctor and contain 'doʊktoʊɹ'"
    );

    // Test Mr. -> Mister
    let ipa_mr = G2p::phonemize("Hello Mr. Holmes.");
    println!("'Hello Mr. Holmes.' -> '{}'", ipa_mr);
    assert!(
        ipa_mr.contains("mɪstɛɹ"),
        "Mr. should expand to Mister and contain 'mɪstɛɹ' in fallback"
    );

    // Test Ms. -> Miss
    let ipa_ms = G2p::phonemize("Hello Ms. Hudson.");
    println!("'Hello Ms. Hudson.' -> '{}'", ipa_ms);
    assert!(
        ipa_ms.contains("mɪs"),
        "Ms. should expand to Miss and contain 'mɪs'"
    );

    // Test Mrs. -> Mrs
    let ipa_mrs = G2p::phonemize("Hello Mrs. Hudson.");
    println!("'Hello Mrs. Hudson.' -> '{}'", ipa_mrs);
    assert!(
        ipa_mrs.contains("mˈɪsɪz"),
        "Mrs. should expand to Misses and contain 'mˈɪsɪz'"
    );

    // Test etc. -> etcetera
    let ipa_etc = G2p::phonemize("Apples, oranges, etc. on the table.");
    println!("'Apples, oranges, etc. on the table.' -> '{}'", ipa_etc);
    assert!(
        ipa_etc.contains("ɛtsˈɛtəɹə"),
        "etc. should expand to etcetera and contain 'ɛtsˈɛtəɹə'"
    );

    println!("G2P accuracy checks passed successfully!");
}

fn test_g2p_speed() {
    println!("\n--- Running G2P Speed Benchmark ---");
    let sample_text = "The quick brown fox jumps over the lazy dog. Dr. Watson and Mr. Holmes discussed the case at 221B Baker Street, etc. Mrs. Hudson served tea, and they spent hours analyzing the clues.";

    let iterations = 1000;
    let start = Instant::now();
    for _ in 0..iterations {
        let _res = G2p::phonemize(sample_text);
    }
    let duration = start.elapsed();
    let per_iter = duration / iterations;
    let characters = sample_text.len();
    let chars_per_sec = (characters as f64 * iterations as f64) / duration.as_secs_f64();

    println!("G2P Benchmark Results:");
    println!("- Total time for {} iterations: {:?}", iterations, duration);
    println!("- Time per iteration: {:?}", per_iter);
    println!("- Characters processed per second: {:.2}", chars_per_sec);
}

fn test_chunking_bounds() {
    println!("\n--- Running Sub-Sentence Chunking Verification ---");

    // 1. Sentence splitting bounds
    let mut chunker = TtsChunker::new();
    let chunks = chunker.push("Hello world. How are you today? Perfect.");
    println!("Sentence split test: {:?}", chunks);
    assert_eq!(
        chunks,
        vec!["Hello world.", "How are you today?", "Perfect."]
    );

    // 2. Comma boundary split: 6-word limit
    let mut chunker_comma = TtsChunker::new();
    // 5 words before comma -> should NOT split on comma.
    let chunks_5w = chunker_comma.push("This has five words here, but we don't split");
    println!("5-word comma test: {:?}", chunks_5w);
    assert_eq!(
        chunks_5w,
        Vec::<String>::new(),
        "Should not return chunks yet, since no terminal punctuation and comma words < 6"
    );
    let rem_5w = chunker_comma.flush();
    assert_eq!(
        rem_5w,
        Some("This has five words here, but we don't split".to_string())
    );

    let mut chunker_comma6 = TtsChunker::new();
    // 6 words before comma -> SHOULD split.
    let chunks_6w = chunker_comma6.push("One two three four five six, seven eight.");
    println!("6-word comma test: {:?}", chunks_6w);
    assert_eq!(
        chunks_6w,
        vec!["One two three four five six,", "seven eight."]
    );

    // 3. 25-word maximum limit
    let mut chunker_max = TtsChunker::new();
    let long_sentence = "one two three four five six seven eight nine ten eleven twelve thirteen fourteen fifteen sixteen seventeen eighteen nineteen twenty twentyone twentytwo twentythree twentyfour twentyfive twentysix twentyseven twentyeight twentynine thirty";
    let chunks_max = chunker_max.push(long_sentence);
    println!("Max 25-word split test: {:?}", chunks_max);
    assert_eq!(chunks_max.len(), 1);
    let words: Vec<&str> = chunks_max[0].split_whitespace().collect();
    assert_eq!(words.len(), 25, "Chunk should contain exactly 25 words");

    let rem_max = chunker_max.flush();
    assert!(rem_max.is_some());
    let rem_str = rem_max.unwrap();
    let rem_words: Vec<&str> = rem_str.split_whitespace().collect();
    assert_eq!(
        rem_words.len(),
        5,
        "Remaining chunk should contain the leftover 5 words"
    );

    println!("Sub-sentence chunking checks passed successfully!");
}

fn test_continuous_execution() {
    println!("\n--- Running ASR/TTS Continuous Execution Benchmark ---");

    let stt_model_dir = std::env::var("LIVA_STT_MODEL_DIR").unwrap_or_else(|_| {
        let paths = [
            "models/nemotron-asr",
            "../models/nemotron-asr",
            "../../models/nemotron-asr",
            "models\\nemotron-asr",
            "..\\models\\nemotron-asr",
            "..\\..\\models\\nemotron-asr",
        ];
        for p in &paths {
            if std::path::Path::new(p).exists() {
                return p.to_string();
            }
        }
        "models/nemotron-asr".to_string()
    });
    let stt_dir = &stt_model_dir;
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

    println!("Loading STT Engine from {}...", stt_dir);
    let start_stt_load = Instant::now();
    let mut stt_engine = match SttEngine::new(stt_dir) {
        Ok(eng) => {
            println!(
                "STT Engine loaded successfully in {:?}",
                start_stt_load.elapsed()
            );
            Some(eng)
        }
        Err(e) => {
            println!("Failed to load STT Engine: {}", e);
            None
        }
    };

    println!("Loading TTS Engine from {}...", tts_model);
    let start_tts_load = Instant::now();
    let voice_bytes = std::fs::read(&tts_voice).expect("Failed to read voice bin file");
    let len_rounded = (voice_bytes.len() / 4) * 4;
    let voice_bytes_aligned = &voice_bytes[..len_rounded];
    let voice_data_vec: Vec<f32> = voice_bytes_aligned.chunks_exact(4).map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]])).collect();

    let mut tts_engine = match TtsEngine::new(tts_model, voice_data_vec) {
        Ok(eng) => {
            println!(
                "TTS Engine loaded successfully in {:?}",
                start_tts_load.elapsed()
            );
            Some(eng)
        }
        Err(e) => {
            println!("Failed to load TTS Engine: {}", e);
            None
        }
    };

    // Benchmark ASR
    if let Some(ref mut stt) = stt_engine {
        println!("\nRunning ASR continuous execution benchmark...");
        // 65 frames of 128 coefficients = 8320 floats
        let dummy_mel = vec![-5.0f32; 65 * 128];
        let iterations = 10;
        let start_stt = Instant::now();
        for i in 0..iterations {
            let chunk_start = Instant::now();
            let res = stt.run_chunk(&dummy_mel, 65);
            match res {
                Ok(tokens) => {
                    println!(
                        "  Iteration {} completed in {:?}, emitted {} tokens",
                        i,
                        chunk_start.elapsed(),
                        tokens.len()
                    );
                }
                Err(e) => {
                    println!("  Iteration {} FAILED: {}", i, e);
                }
            }
        }
        let duration = start_stt.elapsed();
        println!(
            "ASR Benchmark complete. Average iteration time: {:?}",
            duration / iterations
        );
    }

    // Benchmark TTS
    if let Some(ref mut tts) = tts_engine {
        println!("\nRunning TTS continuous execution benchmark...");
        // Hello world tokenizer output character IDs: [0, 43, 16, 44, 16, 45, 0] etc.
        let token_ids = vec![0, 50, 83, 54, 54, 57, 16, 65, 57, 60, 54, 46, 4, 0]; // hello world phonemes
        let iterations = 10;
        let start_tts = Instant::now();
        for i in 0..iterations {
            let chunk_start = Instant::now();
            let res = tts.generate(&token_ids, 1.0);
            match res {
                Ok(waveform) => {
                    println!(
                        "  Iteration {} completed in {:?}, generated {} samples",
                        i,
                        chunk_start.elapsed(),
                        waveform.len()
                    );
                }
                Err(e) => {
                    println!("  Iteration {} FAILED: {}", i, e);
                }
            }
        }
        let duration = start_tts.elapsed();
        println!(
            "TTS Benchmark complete. Average iteration time: {:?}",
            duration / iterations
        );
    }
}

fn main() {
    println!("=== LIVA Voice Modules Performance & Stress Verification ===");
    test_g2p_accuracy();
    test_g2p_speed();
    test_chunking_bounds();
    test_continuous_execution();
    println!("\n=== Verification Completed ===");
}
