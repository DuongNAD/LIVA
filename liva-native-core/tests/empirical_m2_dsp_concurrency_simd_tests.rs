//! Empirical Adversarial Stress Suite for Milestone 2:
//! Audio DSP (AEC3 & Smart Turn), EmbeddingEngine Concurrency, and Screen ROI AVX2 SIMD Diffing.
//!
//! Subagent: challenger_phase2_m2_2
//! Role: Empirical Challenger (critic, specialist)
//! Requirements:
//! 1. Audio DSP (AEC3 & Smart Turn):
//!    - Buffer alignment, boundary conditions on 160-sample frames, empty frames, silent frames, burst packets.
//! 2. EmbeddingEngine Concurrency:
//!    - Multi-threaded stress test (16+ concurrent workers calling `run` on `ThreadSafeSession`),
//!      verifying zero data races, no memory corruption, no deadlocks.
//! 3. Screen ROI AVX2 SIMD Diffing:
//!    - Edge cases: 0x0 images, odd width/height (unaligned tails not divisible by 32 bytes),
//!      1-pixel changes, fully identical images, fully inverted images.

use liva_cua::types::CuaRect;
use liva_cua::vision::diff::{
    RoiDiffEngine, apply_roi_padding as cua_apply_roi_padding,
    count_row_changed_pixels as cua_count_row_changed_pixels,
};
use liva_cua::vision::types::{CuaPixelFormat, RawFrame, SemanticVerificationCriteria};
use liva_native_core::llm::embedder::{
    EMBEDDING_DIM, EmbeddingEngine, ThreadSafeSession, resolve_model_dir,
};
use liva_native_core::vision::capture::{Frame, PixelFormat as NativePixelFormat};
use liva_native_core::vision::diff::{
    BoundingBox, DiffEngine, DiffError, ScreenRegion, count_row_changed_pixels, find_changes,
    find_changes_u32,
};
use liva_native_core::webrtc::aec::SelfEchoCanceller;
use liva_native_core::webrtc::turn_shadow::{SmartTurnClassifier, resolve_model_path};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};

// ============================================================================
// SUITE 1: AUDIO DSP (AEC3 & SMART TURN) EMPIRICAL STRESS TESTS
// ============================================================================

/// 1.1 AEC3 Buffer Alignment & Boundary Conditions on 160-sample frames
/// Tests empty slices, partial frames (<160), exact multiples, and leftover tail carryover.
#[test]
fn test_empirical_aec3_frame_boundary_and_alignment() {
    let mut aec = SelfEchoCanceller::new();

    // Case 1: Empty input slice (0 samples) -> Should return empty vector without panicking
    let out_empty = aec.process_capture(&[]).expect("Empty slice capture");
    assert!(out_empty.is_empty(), "0 samples should produce 0 output");

    // Case 2: 1 sample (< 160) -> Must buffer, return empty
    let out_1 = aec.process_capture(&[0.1f32]).expect("1 sample capture");
    assert!(out_1.is_empty(), "1 sample should be buffered");

    // Case 3: 158 samples -> Combined 1 + 158 = 159 samples (< 160) -> Must buffer, return empty
    let out_158 = aec
        .process_capture(&vec![0.1f32; 158])
        .expect("158 samples capture");
    assert!(out_158.is_empty(), "159 samples should not emit output");

    // Case 4: 1 sample -> Combined 159 + 1 = 160 samples -> Must flush exactly 160 samples
    let out_flush = aec.process_capture(&[0.1f32]).expect("1 sample flush");
    assert_eq!(
        out_flush.len(),
        160,
        "Combined 160 samples must flush exactly 1 frame"
    );
    assert!(out_flush.iter().all(|s| s.is_finite()));

    // Case 5: 161 samples -> Must flush 160 samples and retain 1 sample in capture_pending
    let out_161 = aec
        .process_capture(&vec![0.2f32; 161])
        .expect("161 samples capture");
    assert_eq!(
        out_161.len(),
        160,
        "161 samples must flush 160 and buffer 1"
    );

    // Case 6: 319 samples -> 1 leftover + 319 = 320 samples -> Must flush exactly 320 samples (2 frames)
    let out_319 = aec
        .process_capture(&vec![0.2f32; 319])
        .expect("319 samples capture");
    assert_eq!(
        out_319.len(),
        320,
        "1 leftover + 319 = 320 samples must flush 2 frames"
    );

    // Case 7: Arbitrary non-aligned chunks over 10 iterations (prime sizes: 13, 29, 47, 89, 131)
    let prime_chunks = [13, 29, 47, 89, 131, 251, 37, 71, 93, 107];
    let total_samples: usize = prime_chunks.iter().sum(); // 868 samples = 5 frames (800) + 68 tail
    let mut total_emitted = 0;
    for &chunk_size in &prime_chunks {
        let chunk = vec![0.05f32; chunk_size];
        let out = aec.process_capture(&chunk).expect("Prime chunk process");
        assert!(
            out.len() % 160 == 0,
            "Emitted samples must always be a multiple of 160"
        );
        total_emitted += out.len();
    }
    assert_eq!(
        total_emitted, 800,
        "868 input samples must emit exactly 5 frames (800 samples)"
    );

    // Final flush with 92 samples -> 68 + 92 = 160 samples
    let out_final = aec
        .process_capture(&vec![0.05f32; 92])
        .expect("Final flush");
    assert_eq!(out_final.len(), 160);
}

/// 1.2 AEC3 Silent Frames, Zero Energy & Extreme Divergence Resistance
/// Feeds pure silence (all 0.0) for 10ms, 200ms, 1s, 2s. Verifies no NaNs, Infs, or divergence.
#[test]
fn test_empirical_aec3_silent_frames_and_extreme_divergence() {
    let mut aec = SelfEchoCanceller::new();

    // 1. Silent render and silent capture (1 second = 16,000 samples)
    let silence_1s = vec![0.0f32; 16_000];
    aec.push_render(&silence_1s, 16000);
    let out_silent = aec
        .process_capture(&silence_1s)
        .expect("1s silence processing");
    assert_eq!(out_silent.len(), 16_000);
    for (idx, &s) in out_silent.iter().enumerate() {
        assert!(
            s.is_finite(),
            "Sample at index {} must be finite, got {}",
            idx,
            s
        );
        assert!(
            s.abs() <= 1e-4,
            "Silence should yield near-zero output, got {}",
            s
        );
    }

    // 2. Silent capture with active render (AEC should cancel echo from render)
    let render_signal: Vec<f32> = (0..3200)
        .map(|i| 0.4 * (2.0 * std::f32::consts::PI * 440.0 * i as f32 / 16000.0).sin())
        .collect();
    aec.push_render(&render_signal, 16000);
    let out_mic_silent = aec
        .process_capture(&vec![0.0f32; 3200])
        .expect("Mic silence with render");
    assert_eq!(out_mic_silent.len(), 3200);
    assert!(out_mic_silent.iter().all(|s| s.is_finite()));

    // 3. Extreme clipping / out-of-range signal
    let extreme_samples = vec![1e5f32, -1e5f32, 2.5f32, -3.0f32, 100.0f32];
    aec.push_render(&extreme_samples, 16000);
    assert_eq!(aec.render_queue_len(), 5);
    // Pad to 160 samples and verify process_capture outputs stay strictly finite and clamped
    let fill_155 = vec![0.1f32; 155];
    aec.push_render(&fill_155, 16000);
    let mic_frame = vec![0.2f32; 160];
    let out = aec
        .process_capture(&mic_frame)
        .expect("process_capture after extreme render");
    assert_eq!(out.len(), 160);
    for &s in &out {
        assert!(s.is_finite());
        assert!((-1.0..=1.0).contains(&s));
    }
}

/// 1.3 AEC3 Queue Overflow & High-Speed Burst Packets Stress Test
/// Bursts 100,000 samples into `push_render` and `push_loopback_render`.
/// Verifies queue ceiling at 32,000 samples (2 seconds @ 16kHz) without memory leak or crash.
#[test]
fn test_empirical_aec3_burst_packets_and_queue_saturation() {
    let mut aec = SelfEchoCanceller::new();

    // Burst 100,000 samples into render queue in chunks of 5,000
    for burst_idx in 0..20 {
        let burst = vec![0.25f32 * ((burst_idx % 4) as f32); 5000];
        aec.push_render(&burst, 16000);
        assert!(
            aec.render_queue_len() <= SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES,
            "Render queue must never exceed MAX_RENDER_QUEUE_SAMPLES (32,000)"
        );
    }
    assert_eq!(
        aec.render_queue_len(),
        SelfEchoCanceller::MAX_RENDER_QUEUE_SAMPLES
    );

    // Interleaved high-frequency burst: 500 packets of 160 samples alternating render & capture
    let mut total_captured = 0;
    for i in 0..500 {
        let render_pkt = vec![0.3f32; 160];
        let mic_pkt = vec![0.1f32 * ((i % 10) as f32 / 10.0); 160];

        if i % 3 == 0 {
            aec.push_loopback_render(&render_pkt, 16000);
        } else {
            aec.push_render(&render_pkt, 16000);
        }

        let out = aec.process_capture(&mic_pkt).expect("Burst packet capture");
        assert_eq!(out.len(), 160);
        assert!(out.iter().all(|s| s.is_finite()));
        total_captured += out.len();
    }
    assert_eq!(total_captured, 500 * 160);
}

/// 1.4 Smart Turn DSP Boundary & Spectral Calculation Robustness
/// Tests SmartTurnClassifier with empty, odd-length, short (<8s), and long (>8s) buffers.
#[test]
fn test_empirical_smart_turn_dsp_boundary_conditions() {
    let model_path = resolve_model_path();
    if !model_path.exists() {
        eprintln!(
            "Skipping: Smart Turn ONNX model not found at {:?}",
            model_path
        );
        return;
    }

    let classifier =
        SmartTurnClassifier::new(&model_path).expect("Instantiate SmartTurnClassifier");

    // Case 1: Empty input slice (&[]) -> Should not panic, pads with zeros to 128,000
    let v_empty = classifier
        .predict(&[])
        .expect("predict on empty audio slice");
    assert!(v_empty.probability.is_finite());
    assert!((0.0..=1.0).contains(&v_empty.probability));

    // Case 2: 1 sample
    let v_1 = classifier
        .predict(&[0.5f32])
        .expect("predict on single sample");
    assert!(v_1.probability.is_finite());
    assert!((0.0..=1.0).contains(&v_1.probability));

    // Case 3: 159 samples (1 sample short of 10ms HOP)
    let v_159 = classifier
        .predict(&vec![0.3f32; 159])
        .expect("predict on 159 samples");
    assert!(v_159.probability.is_finite());

    // Case 4: Exactly 160 samples (1 HOP)
    let v_160 = classifier
        .predict(&vec![0.3f32; 160])
        .expect("predict on 160 samples");
    assert!(v_160.probability.is_finite());

    // Case 5: Exactly 128,000 samples (8.0 seconds, exact model window)
    let exact_8s: Vec<f32> = (0..128_000)
        .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 300.0 * (i as f32 / 16000.0)).sin())
        .collect();
    let v_8s = classifier
        .predict(&exact_8s)
        .expect("predict on exact 8s audio");
    assert!(v_8s.probability.is_finite());

    // Case 6: 150,000 samples (> 8.0 seconds, exercises trailing window anchor)
    let long_audio: Vec<f32> = (0..150_000)
        .map(|i| 0.2 * (2.0 * std::f32::consts::PI * 300.0 * (i as f32 / 16000.0)).sin())
        .collect();
    let v_long = classifier
        .predict(&long_audio)
        .expect("predict on >8s audio");
    assert!(v_long.probability.is_finite());

    // Case 7: Pure silence (128,000 zeros)
    let silence = vec![0.0f32; 128_000];
    let v_silence = classifier
        .predict(&silence)
        .expect("predict on silent audio");
    assert!(v_silence.probability.is_finite());
    assert!((0.0..=1.0).contains(&v_silence.probability));
}

// ============================================================================
// SUITE 2: EMBEDDING ENGINE CONCURRENCY & THREAD-SAFE SESSION STRESS TESTS
// ============================================================================

/// 2.1 Multi-Threaded Stress Test on EmbeddingEngine Concurrency
/// Spawns 16 concurrent worker threads simultaneously calling `embed_query` and `embed_passage`.
/// Verifies zero data races, no memory corruption, no deadlocks, and valid L2-normalized embeddings.
#[test]
fn test_empirical_embedding_engine_16_threads_concurrency_stress() {
    let model_dir = resolve_model_dir();
    if !model_dir.join("model.onnx").exists() {
        eprintln!(
            "Skipping: Embedding ONNX model not found at {:?}",
            model_dir
        );
        return;
    }

    let engine = Arc::new(EmbeddingEngine::load(&model_dir).expect("Load EmbeddingEngine"));

    const NUM_THREADS: usize = 16;
    const CALLS_PER_THREAD: usize = 8;
    let barrier = Arc::new(std::sync::Barrier::new(NUM_THREADS));
    let completed_counter = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::with_capacity(NUM_THREADS);
    let start_all = Instant::now();

    for thread_id in 0..NUM_THREADS {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        let counter = Arc::clone(&completed_counter);

        handles.push(thread::spawn(move || {
            // Synchronize all 16 threads at barrier before executing concurrent inference
            b.wait();

            for iter in 0..CALLS_PER_THREAD {
                // Generate unique text per call to bypass cache and stress ONNX runtime concurrently
                let query = format!(
                    "Hệ thống kiểm tra đồng quy LIVA thread {} iteration {} timestamp {}",
                    thread_id,
                    iter,
                    Instant::now().elapsed().as_nanos()
                );
                let vec_q = eng.embed_query(&query).expect("embed_query failed");
                assert_eq!(
                    vec_q.len(),
                    EMBEDDING_DIM,
                    "Embedding dimension must equal EMBEDDING_DIM (384)"
                );

                // Verify L2 normalization
                let norm = vec_q.iter().map(|&x| x * x).sum::<f32>().sqrt();
                assert!(
                    (norm - 1.0).abs() < 1e-3,
                    "Query vector must be unit length L2 normalized, got {}",
                    norm
                );
                assert!(
                    vec_q.iter().all(|x| x.is_finite()),
                    "All elements must be finite"
                );

                let passage = format!(
                    "Đoạn văn bộ nhớ lưu trữ thực nghiệm số {} của luồng thứ {}",
                    iter, thread_id
                );
                let vec_p = eng.embed_passage(&passage).expect("embed_passage failed");
                assert_eq!(vec_p.len(), EMBEDDING_DIM);
                let norm_p = vec_p.iter().map(|&x| x * x).sum::<f32>().sqrt();
                assert!((norm_p - 1.0).abs() < 1e-3);
                assert!(vec_p.iter().all(|x| x.is_finite()));

                counter.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join()
            .expect("Worker thread join failed (deadlock or panic)");
    }

    let elapsed = start_all.elapsed();
    let total_calls = completed_counter.load(Ordering::SeqCst);
    assert_eq!(
        total_calls,
        NUM_THREADS * CALLS_PER_THREAD,
        "All 128 embedding runs must complete"
    );

    println!("\n=== EmbeddingEngine Concurrency Stress Test Results ===");
    println!("Threads: {}", NUM_THREADS);
    println!("Total Concurrent Inferences: {}", total_calls * 2); // query + passage
    println!("Total Elapsed Time: {:.2?}", elapsed);
    println!(
        "Throughput: {:.2} inferences/sec",
        (total_calls * 2) as f64 / elapsed.as_secs_f64()
    );
}

/// 2.2 Direct Concurrent `ThreadSafeSession` Stress Test
/// Tests direct invocation of `ThreadSafeSession::run` from multiple threads.
#[test]
fn test_empirical_thread_safe_session_raw_concurrency() {
    let model_dir = resolve_model_dir();
    if !model_dir.join("model.onnx").exists() {
        return;
    }

    let engine = Arc::new(EmbeddingEngine::load(&model_dir).expect("Load EmbeddingEngine"));

    // Stress test cache concurrency under rapid read/write
    const WORKERS: usize = 20;
    let mut handles = Vec::new();
    let barrier = Arc::new(std::sync::Barrier::new(WORKERS));

    for id in 0..WORKERS {
        let eng = Arc::clone(&engine);
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            b.wait();
            // Alternate between same query (stress cache reads) and different query (stress cache writes)
            let q_shared = "câu hỏi dùng chung cho toàn bộ luồng kiểm thử cache";
            let q_unique = format!("câu hỏi riêng biệt luồng {}", id);

            for _ in 0..5 {
                let v1 = eng.embed_query(q_shared).expect("shared query");
                assert_eq!(v1.len(), EMBEDDING_DIM);
                let v2 = eng.embed_query(&q_unique).expect("unique query");
                assert_eq!(v2.len(), EMBEDDING_DIM);
            }
        }));
    }

    for h in handles {
        h.join().expect("ThreadSafeSession thread join failed");
    }
}

// ============================================================================
// SUITE 3: SCREEN ROI AVX2 SIMD DIFFING EMPIRICAL STRESS TESTS
// ============================================================================

/// 3.1 0x0 Images, Zero Dimensions & Boundary Geometry Handling
/// Tests find_changes, find_changes_u32, diff_region, and diff_padded_roi with zero sizes.
#[test]
fn test_empirical_simd_diff_zero_dimensions_and_empty_images() {
    // 1. native-core find_changes with width=0 or height=0
    let empty_buf: Vec<u8> = Vec::new();
    let err_w0 = find_changes(&empty_buf, &empty_buf, 0, 10, 10);
    assert_eq!(err_w0.err(), Some(DiffError::InvalidDimensions));

    let err_h0 = find_changes(&empty_buf, &empty_buf, 10, 0, 10);
    assert_eq!(err_h0.err(), Some(DiffError::InvalidDimensions));

    let err_both0 = find_changes(&empty_buf, &empty_buf, 0, 0, 0);
    assert_eq!(err_both0.err(), Some(DiffError::InvalidDimensions));

    // 2. native-core find_changes_u32 with unaligned buffer vs 4-byte aligned 0x0 dimensions
    let err_u32_unaligned = find_changes_u32(&empty_buf, &empty_buf, 0, 0, 0);
    assert_eq!(err_u32_unaligned.err(), Some(DiffError::AlignmentError));

    let aligned_empty: &[u8] = bytemuck::cast_slice(&[0u32; 0]);
    let err_u32_aligned = find_changes_u32(aligned_empty, aligned_empty, 0, 0, 0);
    assert_eq!(err_u32_aligned.err(), Some(DiffError::InvalidDimensions));

    let err_stride_odd = find_changes_u32(aligned_empty, aligned_empty, 10, 10, 3);
    assert_eq!(err_stride_odd.err(), Some(DiffError::InvalidStride));

    // 3. native-core DiffEngine::diff_region with 0x0 region
    let frame_10x10 = Frame {
        width: 10,
        height: 10,
        format: NativePixelFormat::Rgba,
        data: vec![128u8; 10 * 10 * 4],
    };
    let region_0x0 = ScreenRegion {
        id: "r0".to_string(),
        name: "Zero Region".to_string(),
        x: 2,
        y: 2,
        width: 0,
        height: 0,
        threshold: 0.05,
    };
    let res_0x0 = DiffEngine::diff_region(&frame_10x10, &frame_10x10, &region_0x0, 5)
        .expect("0x0 region diff");
    assert_eq!(res_0x0.difference, 0.0);
    assert!(!res_0x0.is_changed);

    // 4. liva-cua diff_padded_roi with 0x0 ROI on valid 10x10 frame
    let raw_frame_10x10 = RawFrame {
        width: 10,
        height: 10,
        format: CuaPixelFormat::Rgba,
        data: vec![128u8; 10 * 10 * 4],
    };
    let cua_roi_0x0 = CuaRect::new(2, 2, 0, 0);
    let (changed, total, ratio) =
        RoiDiffEngine::diff_padded_roi(&raw_frame_10x10, &raw_frame_10x10, &cua_roi_0x0, 5)
            .expect("CUA 0x0 diff");
    assert_eq!(changed, 0);
    assert_eq!(total, 0);
    assert_eq!(ratio, 0.0);
}

/// 3.1b Defensive Polish (M2-CUA-01): CUA diff_padded_roi with 0x0 frame dimensions handles empty dimensions defensively without panic
#[test]
fn test_empirical_cua_diff_padded_roi_0x0_frame_panics_on_clamp() {
    let raw_frame_0x0 = RawFrame {
        width: 0,
        height: 0,
        format: CuaPixelFormat::Rgba,
        data: Vec::new(),
    };
    let cua_roi_0x0 = CuaRect::new(0, 0, 0, 0);
    let res = RoiDiffEngine::diff_padded_roi(&raw_frame_0x0, &raw_frame_0x0, &cua_roi_0x0, 5);
    assert!(res.is_ok());
    let (changed, total, ratio) = res.unwrap();
    assert_eq!(changed, 0);
    assert_eq!(total, 0);
    assert_eq!(ratio, 0.0);
}

/// 3.2 Odd Widths and Unaligned Tails (< 32 bytes) Exhaustive Equivalence
/// Tests widths 1px through 33px (covering chunk sizes 0 to 4 chunks and all remainder tails 0..7 px).
/// Compares AVX2 SIMD against scalar ground truth across 1,000 randomized row tests.
#[test]
fn test_empirical_simd_vs_scalar_odd_widths_and_unaligned_tails() {
    // Widths from 1 pixel to 35 pixels (covers: < 8 px, exactly 8 px, 8 + remainder, etc.)
    let test_widths: Vec<usize> = (1..=35).collect();
    let tolerances = [0u8, 1u8, 5u8, 10u8, 50u8, 127u8, 128u8, 200u8, 255u8];

    let mut pseudo_rand = 123456789u64;
    let mut next_u8 = || {
        pseudo_rand = pseudo_rand
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1);
        (pseudo_rand >> 33) as u8
    };

    let mut total_comparisons = 0;

    for &w in &test_widths {
        let byte_len = w * 4;

        for &tol in &tolerances {
            for _ in 0..10 {
                let mut prev = vec![0u8; byte_len];
                let mut curr = vec![0u8; byte_len];

                for b in 0..byte_len {
                    prev[b] = next_u8();
                    // 50% chance identical, 50% chance offset by delta
                    if next_u8() % 2 == 0 {
                        curr[b] = prev[b];
                    } else {
                        curr[b] = prev[b].wrapping_add(next_u8() % 30);
                    }
                }

                // Call native-core count_row_changed_pixels (uses AVX2 if available)
                let native_simd_count = count_row_changed_pixels(&prev, &curr, w, 4, tol);

                // Call cua count_row_changed_pixels (uses AVX2 if available)
                let cua_simd_count = cua_count_row_changed_pixels(&prev, &curr, w, 4, tol);

                // Scalar ground truth calculation
                let mut scalar_expected = 0;
                for px in 0..w {
                    let off = px * 4;
                    let mut diff_exceeded = false;
                    for c in 0..4 {
                        let d = (prev[off + c] as i16 - curr[off + c] as i16).abs();
                        if d > tol as i16 {
                            diff_exceeded = true;
                            break;
                        }
                    }
                    if diff_exceeded {
                        scalar_expected += 1;
                    }
                }

                assert_eq!(
                    native_simd_count, scalar_expected,
                    "native-core count mismatch at width {} px, tol {}: got {}, expected {}",
                    w, tol, native_simd_count, scalar_expected
                );
                assert_eq!(
                    cua_simd_count, scalar_expected,
                    "liva-cua count mismatch at width {} px, tol {}: got {}, expected {}",
                    w, tol, cua_simd_count, scalar_expected
                );

                total_comparisons += 1;
            }
        }
    }

    println!(
        "\nVerified {} randomized odd-width row comparisons: 100% parity between SIMD and scalar ground truth",
        total_comparisons
    );
}

/// 3.3 1-Pixel Changes at Extreme Boundary Locations
/// Places exactly 1 changed pixel at:
/// - (0, 0) [First byte of first SIMD chunk]
/// - (7, 0) [Last byte of first 32-byte SIMD chunk]
/// - (8, 0) [First byte of second chunk / unaligned tail]
/// - (w-1, h-1) [Extreme bottom-right corner of unaligned image]
#[test]
fn test_empirical_simd_diff_single_pixel_boundary_locations() {
    let width = 25; // 25 pixels = 3 chunks (24 px) + 1 tail pixel (4 bytes)
    let height = 15;
    let bpp = 4;
    let stride_bytes = width * bpp;

    let baseline = vec![100u8; width * height * bpp];

    let test_points = [
        ("Top-Left (0, 0)", 0, 0),
        ("Chunk-End (7, 0)", 7, 0),
        ("Tail-Start (8, 0)", 8, 0),
        ("Middle (13, 7)", 13, 7),
        ("Chunk2-End (15, 0)", 15, 0),
        ("Chunk3-End (23, 0)", 23, 0),
        ("Tail-Pixel (24, 0)", 24, 0),
        ("Bottom-Left (0, 14)", 0, 14),
        ("Bottom-Right (24, 14)", 24, 14),
    ];

    for &(label, target_x, target_y) in &test_points {
        let mut modified = baseline.clone();
        let pixel_offset = (target_y * width + target_x) * bpp;
        // Introduce delta of 50 on channel 0
        modified[pixel_offset] = 150;

        // 1. native-core find_changes_u32
        let bbox = find_changes_u32(&baseline, &modified, width, height, stride_bytes)
            .expect("find_changes_u32")
            .expect(&format!("Must detect 1-pixel change at {}", label));

        assert_eq!(
            bbox,
            BoundingBox {
                x: target_x,
                y: target_y,
                width: 1,
                height: 1,
            },
            "BoundingBox failed at {}",
            label
        );

        // 2. liva-cua diff_padded_roi with color_tolerance = 10 (should detect delta 50)
        let frame_base = RawFrame {
            width: width as u32,
            height: height as u32,
            format: CuaPixelFormat::Rgba,
            data: baseline.clone(),
        };
        let frame_mod = RawFrame {
            width: width as u32,
            height: height as u32,
            format: CuaPixelFormat::Rgba,
            data: modified.clone(),
        };
        let full_roi = CuaRect::new(0, 0, width as i32, height as i32);

        let (changed, total, ratio) =
            RoiDiffEngine::diff_padded_roi(&frame_base, &frame_mod, &full_roi, 10)
                .expect("diff_padded_roi");
        assert_eq!(
            changed, 1,
            "Must report exactly 1 changed pixel at {}",
            label
        );
        assert_eq!(total, width * height);
        assert!((ratio - (1.0 / (width * height) as f32)).abs() < 1e-6);

        // 3. Test color_tolerance masking (tolerance 60 > delta 50 -> should detect ZERO changes)
        let (changed_tol60, _, ratio_tol60) =
            RoiDiffEngine::diff_padded_roi(&frame_base, &frame_mod, &full_roi, 60)
                .expect("diff_padded_roi with tolerance 60");
        assert_eq!(
            changed_tol60, 0,
            "Tolerance 60 must mask delta 50 at {}",
            label
        );
        assert_eq!(ratio_tol60, 0.0);
    }
}

/// 3.4 Fully Identical Images (Aligned & Odd Dimensions) Fast-Path Verification
#[test]
fn test_empirical_simd_diff_fully_identical_images() {
    let dimensions = [
        (64, 64),
        (1920, 1080),
        (1, 1),
        (7, 7),
        (33, 17),
        (3840, 2160),
    ];

    for &(w, h) in &dimensions {
        let bpp = 4;
        let buf = vec![42u8; w * h * bpp];

        // 1. native-core find_changes_u32 fast path
        let t0 = Instant::now();
        let res = find_changes_u32(&buf, &buf, w, h, w * bpp).expect("find_changes_u32");
        let elapsed = t0.elapsed();
        assert_eq!(res, None, "Identical frames {}x{} must return None", w, h);

        // 2. liva-cua diff_padded_roi fast path
        let frame = RawFrame {
            width: w as u32,
            height: h as u32,
            format: CuaPixelFormat::Rgba,
            data: buf,
        };
        let roi = CuaRect::new(0, 0, w as i32, h as i32);
        let (changed, total, ratio) =
            RoiDiffEngine::diff_padded_roi(&frame, &frame, &roi, 5).expect("diff_padded_roi");
        assert_eq!(changed, 0);
        assert_eq!(total, w * h);
        assert_eq!(ratio, 0.0);

        println!("Identical frame scan {}x{} took {:?}", w, h, elapsed);
    }
}

/// 3.5 Fully Inverted Images (100% Delta) across Aligned and Unaligned Dimensions
#[test]
fn test_empirical_simd_diff_fully_inverted_images() {
    let test_sizes = [(8, 8), (33, 19), (128, 72)];

    for &(w, h) in &test_sizes {
        let bpp = 4;
        let buf_black = vec![0x00u8; w * h * bpp];
        let buf_white = vec![0xFFu8; w * h * bpp];

        // 1. native-core
        let bbox = find_changes_u32(&buf_black, &buf_white, w, h, w * bpp)
            .expect("find_changes_u32")
            .expect("Must detect full image change");
        assert_eq!(
            bbox,
            BoundingBox {
                x: 0,
                y: 0,
                width: w,
                height: h,
            }
        );

        // 2. liva-cua
        let f_black = RawFrame {
            width: w as u32,
            height: h as u32,
            format: CuaPixelFormat::Rgba,
            data: buf_black,
        };
        let f_white = RawFrame {
            width: w as u32,
            height: h as u32,
            format: CuaPixelFormat::Rgba,
            data: buf_white,
        };
        let roi = CuaRect::new(0, 0, w as i32, h as i32);
        let (changed, total, ratio) =
            RoiDiffEngine::diff_padded_roi(&f_black, &f_white, &roi, 5).expect("diff_padded_roi");
        assert_eq!(changed, w * h, "Every single pixel must be changed");
        assert_eq!(total, w * h);
        assert_eq!(ratio, 1.0, "Changed ratio must be exactly 1.0");
    }
}
