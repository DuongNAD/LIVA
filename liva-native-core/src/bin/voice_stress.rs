
use std::time::Instant;
use liva_native_core::stt::engine::SttEngine;
use liva_native_core::tts::TtsChunker;
use liva_native_core::tts::engine::TtsEngine;
use liva_native_core::tts::g2p::G2p;

/// Kiểm bảng mở rộng viết tắt của `G2p::clean_text`.
///
/// # Vì sao bản trước ĐỎ, và vì sao nó đỏ theo hướng ngược đời
///
/// Bản trước ghim **chuỗi IPA nguyên văn** cho từng viết tắt
/// (`doʊktoʊɹ`, `mɪstɛɹ`, `ɛtsˈɛtəɹə`…). Đo ngày 29/07/2026 thì
/// `phonemize("Hello Dr. Watson.")` trả `həlˈoʊ dˈɑːktɚ wˈɑːtsən` — tức mở rộng
/// **đã chạy đúng** (`dˈɑːktɚ` chính là "doctor"), nhưng assert vẫn nổ.
///
/// Lý do: những chuỗi đó là output của **nhánh dự phòng** trong `g2p.rs`, nhánh
/// chỉ chạy khi `try_espeak_ng` thất bại. Thông điệp lỗi của chính bản cũ còn
/// ghi *"in fallback"*. Nghĩa là bộ assert này **chỉ xanh trên máy THIẾU
/// espeak-ng** — trong khi espeak-ng là điều kiện tiên quyết bắt buộc, ghi rõ ở
/// `CLAUDE.md`. Một phép kiểm chỉ đạt khi môi trường bị hỏng thì nó không kiểm
/// tính năng, nó kiểm sự vắng mặt của một phụ thuộc.
///
/// Không ai bắt được vì `voice_stress` **không nằm trong CI** (CI chạy
/// `cargo test`, không chạy các binary probe) — cùng mô thức đã ghi ở `boot.rs`:
/// *mọi lệch đều rơi đúng vào phía không ai kiểm*.
///
/// # Bản này khẳng định cái gì thay vào đó
///
/// Mở rộng viết tắt xảy ra trong `clean_text`, **trước** khi văn bản tới espeak.
/// Nên tính chất đúng để khẳng định là một **đẳng thức**: viết tắt và dạng viết
/// đầy đủ phải cho ra **cùng một** chuỗi phiên âm. Nó độc lập với phiên bản
/// espeak-ng, với giọng (`-v en-us`), và đúng cả trên nhánh dự phòng — vì cả hai
/// vế đều đi qua đúng một đường. Nó vẫn bắt được đủ các hồi quy đáng lo: bảng
/// mở rộng bị gỡ, ánh xạ sai từ, hoặc regex thôi khớp.
fn test_g2p_accuracy() {
    println!("\n--- Running G2P Accuracy Verification ---");

    // (viết tắt, dạng viết đầy đủ theo bảng trong `tts/g2p.rs::clean_text`)
    let truong_hop = [
        ("Hello Dr. Watson.", "Hello Doctor Watson."),
        ("Hello Mr. Holmes.", "Hello Mister Holmes."),
        ("Hello Ms. Hudson.", "Hello Miss Hudson."),
        ("Hello Mrs. Hudson.", "Hello Misses Hudson."),
        (
            "Apples, oranges, etc. on the table.",
            "Apples, oranges, etcetera on the table.",
        ),
    ];

    for (viet_tat, viet_day_du) in truong_hop {
        let ipa_viet_tat = G2p::phonemize(viet_tat);
        let ipa_day_du = G2p::phonemize(viet_day_du);
        println!("'{viet_tat}' -> '{ipa_viet_tat}'");

        assert!(
            !ipa_viet_tat.trim().is_empty(),
            "'{viet_tat}' cho ra phiên âm RỖNG — espeak-ng lẫn nhánh dự phòng đều hỏng?"
        );
        assert_eq!(
            ipa_viet_tat, ipa_day_du,
            "'{viet_tat}' phải phiên âm y hệt '{viet_day_du}'. \
             Lệch nghĩa là bảng mở rộng viết tắt trong `tts/g2p.rs::clean_text` \
             không còn chạy, hoặc đang ánh xạ sang từ khác."
        );
    }

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

    // Thiếu file giọng là THIẾU TÀI NGUYÊN, không phải lỗi lập trình — nên nó
    // phải hạ cấp mềm y như nhánh STT ngay phía trên, chứ không panic.
    //
    // Trước 29/07/2026 dòng này là `.expect("Failed to read voice bin file")`,
    // và trên máy chưa tải Kokoro nó giết cả chương trình bằng
    // `Os { code: 3, kind: NotFound }` — không nói thiếu file nào, không nói lấy
    // ở đâu, và **chôn luôn ba nhóm kiểm phía sau** vốn không cần TTS. Cùng một
    // hàm mà STT hạ cấp còn TTS panic là lệch do sót, không phải do thiết kế.
    let voice_data_vec: Option<Vec<f32>> = match std::fs::read(&tts_voice) {
        Ok(bytes) => {
            let len_rounded = (bytes.len() / 4) * 4;
            Some(
                bytes[..len_rounded]
                    .chunks_exact(4)
                    .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
                    .collect(),
            )
        }
        Err(e) => {
            println!("Failed to read voice bin file '{tts_voice}': {e}");
            println!(
                "  → Bỏ qua phần kiểm TTS. Lấy file giọng bằng `npm ci` (kokoro-js) \
                 hoặc `npm run setup:models`; `npm run doctor` liệt kê thứ còn thiếu."
            );
            None
        }
    };

    let mut tts_engine = match voice_data_vec.map(|v| TtsEngine::new(tts_model, v)) {
        Some(Ok(eng)) => {
            println!(
                "TTS Engine loaded successfully in {:?}",
                start_tts_load.elapsed()
            );
            Some(eng)
        }
        Some(Err(e)) => {
            println!("Failed to load TTS Engine: {}", e);
            None
        }
        None => None,
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
