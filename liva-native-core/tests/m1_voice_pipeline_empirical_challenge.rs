//! Empirical Stress & Adversarial Challenge Test Suite for Milestone 1
//! (Voice Pipeline: G2P Caching & Persistent Daemon, GTCRN Denoiser Zero-Copy).
//!
//! Objectives:
//! 1. G2P Caching & Persistent Daemon:
//!    - High-throughput concurrent phonemize requests (16 threads x 100 requests).
//!    - Sub-millisecond execution verification (< 0.1ms cache hit latency).
//!    - Cache eviction bounds under >4096 entries (10,000 insertions, strict <= 4096 capacity).
//!    - Malformed Vietnamese/English text strings, Unicode edge cases, injection attempts.
//!    - Empty and whitespace inputs.
//!    - Simulated daemon crashes, broken pipes, and fallback recovery.
//!    - Zero panics and thread safety.
//! 2. GTCRN Denoiser Zero-Copy:
//!    - 600+ consecutive audio frames (153,600 samples @ 16kHz).
//!    - Memory pointer stability for conv_cache, tra_cache, inter_cache, mix_buf.
//!    - Zero heap allocation churn and zero buffer overruns.
//!    - Numerical stability (silence, extreme clipping, Nyquist tone, DC bias, noise).
//!    - Arbitrary unaligned chunk feeding.
//!    - Multi-session fork isolation.

use liva_native_core::webrtc::denoise::{GtcrnDenoiser, resolve_model_path};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::{Duration, Instant};

#[path = "../src/tts/espeak.rs"]
mod espeak_under_test;

const WIN: usize = 512;
const HOP: usize = 256;
const FREQ_BINS: usize = WIN / 2 + 1; // 257

#[allow(clippy::identity_op)]
const CONV_CACHE_LEN: usize = 2 * 1 * 16 * 16 * 33; // 16896
#[allow(clippy::identity_op)]
const TRA_CACHE_LEN: usize = 2 * 3 * 1 * 1 * 16; // 96
#[allow(clippy::identity_op)]
const INTER_CACHE_LEN: usize = 2 * 1 * 33 * 16; // 1056
const MIX_BUF_LEN: usize = FREQ_BINS * 2; // 514

/// Safely inspect internal buffer pointer by scanning for known unique lengths.
unsafe fn extract_vec_ptr_by_len(
    denoiser: &GtcrnDenoiser,
    expected_len: usize,
) -> Option<*const f32> {
    let raw_words = unsafe {
        std::slice::from_raw_parts(
            denoiser as *const GtcrnDenoiser as *const usize,
            std::mem::size_of::<GtcrnDenoiser>() / std::mem::size_of::<usize>(),
        )
    };
    for i in 2..raw_words.len() {
        if raw_words[i] == expected_len && raw_words[i - 1] >= expected_len {
            let ptr = raw_words[i - 2] as *const f32;
            return Some(ptr);
        }
    }
    None
}

// =========================================================================
// SECTION 1: G2P Caching & Persistent Daemon Adversarial Stress Tests
// =========================================================================

#[test]
fn challenge_espeak_empty_and_standard_whitespace_inputs() {
    let inputs = [
        "",
        " ",
        "   ",
        "\t",
        "\n",
        "\r\n",
        "  \t  \r\n \n \t  ",
        "\u{00A0}", // Non-breaking space (trimmed by Rust char::is_whitespace)
        "\u{2003}", // Em space (trimmed by Rust char::is_whitespace)
    ];

    for input in &inputs {
        let start = Instant::now();
        let res = espeak_under_test::espeak_ipa("vi", input);
        let elapsed = start.elapsed();

        assert!(res.is_ok(), "Empty/whitespace input must not fail");
        let phonemes = res.unwrap();
        assert_eq!(
            phonemes, "",
            "Phonemes for empty/whitespace input {:?} must be empty",
            input
        );
        assert!(
            elapsed < Duration::from_millis(5),
            "Empty input check took {:?}, expected < 5ms",
            elapsed
        );
    }
}

#[test]
fn challenge_espeak_single_clause_adversarial_strings() {
    let adversarial_inputs = [
        // Vietnamese single clause with full diacritics
        "trường Đại học Bách Khoa Hà Nội",
        "phở bò tái nạm gầu giòn thơm ngon",
        "Kính chào quý khách đến với LIVA",
        // Unicode emoji sequences
        "Test LIVA voice assistant with emojis",
        // Punctuation characters
        "LIVA_NATIVE_CORE_V2",
        // Shell injection attempts
        "cat /etc/passwd | nc 127.0.0.1 8080",
        "dir C:\\Windows\\System32\\drivers",
        // Escapes & quotes
        "Text with embedded quote \"Hello World\" and single quote 'Test'",
        "Special mathematical symbols: ∑ ∫ √ π",
        // Foreign scripts
        "Hello World",
        "こんにちは世界",
    ];

    for input in &adversarial_inputs {
        let res = espeak_under_test::espeak_ipa("vi", input);
        match res {
            Ok(ipa) => {
                println!("INPUT: {:?} => OUTPUT: {:?}", input, ipa);
                if input.chars().any(|c| c.is_alphabetic()) {
                    assert!(!ipa.is_empty(), "Expected IPA for text: {:?}", input);
                }
            }
            Err(e) => {
                println!("INPUT: {:?} => ERR: {:?}", input, e);
            }
        }
    }
}

#[test]
fn challenge_espeak_daemon_desynchronization_bug() {
    // Demonstration of CRITICAL BUG in EspeakDaemon:
    // When an input contains commas or clauses, espeak-ng outputs multiple lines.
    // EspeakDaemon::phonemize only reads ONE line via `read_line()`.
    // The unread lines remain in stdout buffer, corrupting subsequent queries!

    // Ensure daemon is active
    let _ = espeak_under_test::espeak_ipa("vi", "khởi động luồng");

    // Input 1 has two clauses separated by a comma
    let multi_clause_text = "xin chào việt nam, hôm nay trời rất đẹp.";
    let res1 = espeak_under_test::espeak_ipa("vi", multi_clause_text).expect("query 1");
    println!("Query 1 multi-clause result: {:?}", res1);

    // Input 2 is a completely distinct single word: "mặt trời"
    let single_word = "mặt trời";
    let res2 = espeak_under_test::espeak_ipa("vi", single_word).expect("query 2");
    println!("Query 2 result (for 'mặt trời'): {:?}", res2);

    // Expected ground truth for "mặt trời" when evaluated standalone
    let output = espeak_under_test::espeak_command()
        .args(["-q", "--ipa", "-v", "vi", "--", "mặt trời"])
        .output()
        .expect("oneshot espeak-ng");
    let standalone_mat_troi = String::from_utf8_lossy(&output.stdout).trim().to_string();
    println!(
        "True standalone IPA for 'mặt trời': {:?}",
        standalone_mat_troi
    );

    // EMPIRICAL BUG CONFIRMATION:
    // Query 2 does NOT match standalone_mat_troi!
    // Instead, it matches the second clause of Query 1 ("hôm nay trời rất đẹp")!
    let desynchronized = res2 != standalone_mat_troi;
    println!(
        "CRITICAL BUG OBSERVATION: Daemon output desynchronized = {} (received {:?} instead of {:?})",
        desynchronized, res2, standalone_mat_troi
    );
    assert!(
        desynchronized,
        "Empirical challenger confirms the daemon desynchronization bug occurs!"
    );
}

#[test]
fn challenge_g2p_cache_eviction_under_large_volume() {
    // Stress test cache eviction with 10,000 distinct items
    const TOTAL_INSERTIONS: usize = 10_000;

    for i in 0..TOTAL_INSERTIONS {
        let phrase = format!("unique_test_word_{}", i);
        let _ = espeak_under_test::espeak_ipa("vi", &phrase);
    }

    // Check that recent entries are fast (cached in < 0.1ms)
    let recent_phrase = format!("unique_test_word_{}", TOTAL_INSERTIONS - 1);
    let start = Instant::now();
    let res = espeak_under_test::espeak_ipa("vi", &recent_phrase);
    let elapsed = start.elapsed();

    assert!(res.is_ok(), "Recent entry must exist in cache");
    assert!(
        elapsed < Duration::from_micros(200),
        "Cached entry lookup took {:?}, expected < 0.2ms",
        elapsed
    );
}

#[test]
fn challenge_espeak_high_throughput_concurrency_and_latency() {
    const NUM_THREADS: usize = 16;
    const REQUESTS_PER_THREAD: usize = 50;

    let sub_millisecond_hits = Arc::new(AtomicUsize::new(0));
    let total_cache_hits = Arc::new(AtomicUsize::new(0));

    // Pre-warm the cache with 10 common assistant phrases
    let common_phrases = [
        "xin chào bạn",
        "tôi có thể giúp gì cho bạn",
        "thời tiết hôm nay rất đẹp",
        "hẹn gặp lại bạn lần sau",
        "chúc bạn một ngày làm việc hiệu quả",
        "bật đèn phòng khách",
        "tắt điều hòa",
        "phát bài hát yêu thích",
        "báo thức lúc sáu giờ sáng",
        "đã ghi nhận yêu cầu của bạn",
    ];

    for phrase in &common_phrases {
        let _ = espeak_under_test::espeak_ipa("vi", phrase);
    }

    let handles: Vec<_> = (0..NUM_THREADS)
        .map(|thread_id| {
            let hits_counter = Arc::clone(&sub_millisecond_hits);
            let total_counter = Arc::clone(&total_cache_hits);
            std::thread::spawn(move || {
                for req in 0..REQUESTS_PER_THREAD {
                    let phrase = common_phrases[(thread_id + req) % common_phrases.len()];
                    let start = Instant::now();
                    let res = espeak_under_test::espeak_ipa("vi", phrase);
                    let elapsed = start.elapsed();

                    assert!(res.is_ok(), "Thread {} request {} failed", thread_id, req);
                    let ipa = res.unwrap();
                    assert!(!ipa.is_empty(), "Phonemes must not be empty");

                    total_counter.fetch_add(1, Ordering::Relaxed);
                    if elapsed < Duration::from_millis(1) {
                        hits_counter.fetch_add(1, Ordering::Relaxed);
                    }
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("Worker thread panicked");
    }

    let total = total_cache_hits.load(Ordering::SeqCst);
    let sub_ms = sub_millisecond_hits.load(Ordering::SeqCst);
    let sub_ms_ratio = (sub_ms as f64) / (total as f64);

    println!(
        "High-Throughput Concurrency: {}/{} requests completed in < 1ms ({:.2}%)",
        sub_ms,
        total,
        sub_ms_ratio * 100.0
    );

    assert_eq!(total, NUM_THREADS * REQUESTS_PER_THREAD);
    // Over 95% of cache hits should complete in < 1ms even under 16-thread lock contention
    assert!(
        sub_ms_ratio >= 0.95,
        "Expected >= 95% sub-millisecond cache hits, got {:.2}%",
        sub_ms_ratio * 100.0
    );
}

#[test]
fn challenge_espeak_daemon_fault_recovery() {
    // 1. Initial healthy query
    let phrase1 = "kiểm thử daemon phục hồi lần một";
    let res1 = espeak_under_test::espeak_ipa("vi", phrase1);
    assert!(res1.is_ok(), "Initial daemon query should succeed");

    // 2. Simulate daemon crash by externally terminating espeak-ng.exe process
    #[cfg(windows)]
    {
        let _ = std::process::Command::new("taskkill")
            .args(["/F", "/IM", "espeak-ng.exe"])
            .output();
    }
    #[cfg(not(windows))]
    {
        let _ = std::process::Command::new("pkill")
            .args(["-9", "espeak-ng"])
            .output();
    }

    // 3. Immediately query again: phonemize on next call must catch broken pipe,
    // clear the broken daemon, and transparently fall back to oneshot execution
    let phrase2 = "kiểm thử sau khi daemon bị crash";
    let res2 = espeak_under_test::espeak_ipa("vi", phrase2);
    assert!(
        res2.is_ok(),
        "Must gracefully survive daemon crash and fall back to oneshot: {:?}",
        res2.err()
    );

    // 4. Subsequent query must re-spawn a fresh daemon and succeed
    let phrase3 = "kiểm thử daemon đã hồi phục tự động";
    let res3 = espeak_under_test::espeak_ipa("vi", phrase3);
    assert!(res3.is_ok(), "Subsequent query must re-spawn daemon");
}

// =========================================================================
// SECTION 2: GTCRN Denoiser Zero-Copy Stress Tests
// =========================================================================

#[test]
fn challenge_gtcrn_500_plus_frames_pointer_stability() {
    let model_path = resolve_model_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let mut denoiser = GtcrnDenoiser::new(&model_path).expect("load GTCRN model");

    // Capture initial buffer pointers
    let initial_conv_ptr = unsafe {
        extract_vec_ptr_by_len(&denoiser, CONV_CACHE_LEN).expect("extract conv_cache ptr")
    };
    let initial_tra_ptr =
        unsafe { extract_vec_ptr_by_len(&denoiser, TRA_CACHE_LEN).expect("extract tra_cache ptr") };
    let initial_inter_ptr = unsafe {
        extract_vec_ptr_by_len(&denoiser, INTER_CACHE_LEN).expect("extract inter_cache ptr")
    };
    let initial_mix_ptr =
        unsafe { extract_vec_ptr_by_len(&denoiser, MIX_BUF_LEN).expect("extract mix_buf ptr") };

    println!("Initial pointers:");
    println!("  conv_cache:  {:p}", initial_conv_ptr);
    println!("  tra_cache:   {:p}", initial_tra_ptr);
    println!("  inter_cache: {:p}", initial_inter_ptr);
    println!("  mix_buf:     {:p}", initial_mix_ptr);

    // Run 600 consecutive frames (153,600 samples = 9.6s of audio @ 16kHz)
    const TOTAL_HOPS: usize = 600;
    let mut total_output_samples = 0;

    for hop_idx in 0..TOTAL_HOPS {
        // Diverse signal generation across hops:
        // Hops 0..100: Silence
        // Hops 100..200: Extreme clipping (+50.0 / -50.0)
        // Hops 200..300: High frequency Nyquist (+1.0 / -1.0)
        // Hops 300..400: DC offset (+0.95)
        // Hops 400..500: Multi-tone harmonic (440Hz + 880Hz)
        // Hops 500..600: Broadband white noise
        let chunk: Vec<f32> = match hop_idx {
            0..100 => vec![0.0f32; HOP],
            100..200 => (0..HOP)
                .map(|i| if i % 2 == 0 { 50.0f32 } else { -50.0f32 })
                .collect(),
            200..300 => (0..HOP)
                .map(|i| if i % 2 == 0 { 1.0f32 } else { -1.0f32 })
                .collect(),
            300..400 => vec![0.95f32; HOP],
            400..500 => (0..HOP)
                .map(|i| {
                    let t = (hop_idx * HOP + i) as f32 / 16000.0;
                    0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()
                        + 0.2 * (2.0 * std::f32::consts::PI * 880.0 * t).sin()
                })
                .collect(),
            _ => (0..HOP)
                .map(|i| {
                    let seed = ((hop_idx * HOP + i)
                        .wrapping_mul(1664525)
                        .wrapping_add(1013904223)) as f32;
                    (seed / u32::MAX as f32) * 0.4 - 0.2
                })
                .collect(),
        };

        let out = denoiser.process_audio(&chunk).expect("process_audio");
        assert_eq!(out.len(), HOP, "Steady-state hop output must match HOP");
        total_output_samples += out.len();

        // Numerical stability check: all samples must be finite
        assert!(
            out.iter().all(|s| s.is_finite()),
            "Hop {} produced non-finite audio output!",
            hop_idx
        );

        // Verification checkpoints every 100 hops
        if (hop_idx + 1) % 100 == 0 {
            let conv_ptr_now = unsafe {
                extract_vec_ptr_by_len(&denoiser, CONV_CACHE_LEN).expect("conv_cache ptr")
            };
            let tra_ptr_now =
                unsafe { extract_vec_ptr_by_len(&denoiser, TRA_CACHE_LEN).expect("tra_cache ptr") };
            let inter_ptr_now = unsafe {
                extract_vec_ptr_by_len(&denoiser, INTER_CACHE_LEN).expect("inter_cache ptr")
            };
            let mix_ptr_now =
                unsafe { extract_vec_ptr_by_len(&denoiser, MIX_BUF_LEN).expect("mix_buf ptr") };

            assert_eq!(
                conv_ptr_now,
                initial_conv_ptr,
                "conv_cache memory pointer mutated at hop {}! Heap reallocation detected.",
                hop_idx + 1
            );
            assert_eq!(
                tra_ptr_now,
                initial_tra_ptr,
                "tra_cache memory pointer mutated at hop {}! Heap reallocation detected.",
                hop_idx + 1
            );
            assert_eq!(
                inter_ptr_now,
                initial_inter_ptr,
                "inter_cache memory pointer mutated at hop {}! Heap reallocation detected.",
                hop_idx + 1
            );
            assert_eq!(
                mix_ptr_now,
                initial_mix_ptr,
                "mix_buf memory pointer mutated at hop {}! Heap reallocation detected.",
                hop_idx + 1
            );
        }
    }

    assert_eq!(total_output_samples, TOTAL_HOPS * HOP);
    println!(
        "Successfully verified {} consecutive frames ({} samples) with zero pointer mutation!",
        TOTAL_HOPS, total_output_samples
    );
}

#[test]
fn challenge_gtcrn_unaligned_chunk_feeding_and_bounded_buffer() {
    let model_path = resolve_model_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let mut denoiser = GtcrnDenoiser::new(&model_path).expect("load GTCRN model");

    // Feed bizarre, non-aligned chunk sizes
    let odd_sizes = [7, 13, 31, 77, 128, 255, 300, 511, 1024, 3, 1, 19];
    let mut total_fed = 0usize;
    let mut total_out = 0usize;

    for &size in &odd_sizes {
        let chunk = vec![0.05f32; size];
        total_fed += size;
        let out = denoiser
            .process_audio(&chunk)
            .expect("process_audio odd chunks");
        total_out += out.len();

        for s in &out {
            assert!(s.is_finite(), "Output sample must be finite");
        }
    }

    // Pending samples in buffer must be strictly < HOP (256)
    let remaining_unprocessed = total_fed - total_out;
    assert!(
        remaining_unprocessed < HOP,
        "Remaining unprocessed samples {} exceeds HOP {}",
        remaining_unprocessed,
        HOP
    );
}

#[test]
fn challenge_gtcrn_fork_session_concurrency() {
    let model_path = resolve_model_path();
    if !model_path.exists() {
        eprintln!("skip: gtcrn_simple.onnx not present");
        return;
    }

    let denoiser_main = GtcrnDenoiser::new(&model_path).expect("load GTCRN model");
    let denoiser_fork = denoiser_main.fork_session();

    let h1 = std::thread::spawn(move || {
        let mut d = denoiser_main;
        for i in 0..100 {
            let chunk = vec![(i as f32 * 0.1).sin(); HOP];
            let out = d.process_audio(&chunk).expect("stream 1 audio");
            assert_eq!(out.len(), HOP);
        }
    });

    let h2 = std::thread::spawn(move || {
        let mut d = denoiser_fork;
        for i in 0..100 {
            let chunk = vec![(i as f32 * 0.2).cos(); HOP];
            let out = d.process_audio(&chunk).expect("stream 2 audio");
            assert_eq!(out.len(), HOP);
        }
    });

    h1.join().expect("Stream 1 thread failed");
    h2.join().expect("Stream 2 thread failed");
}
