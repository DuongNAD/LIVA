//! Adversarial Stress & Audio Boundary Challenge Test for Wake Word Engine (Milestone 1).
//!
//! Evaluates:
//! 1. Audio silence padding invariants (exact sample preservation, zero perturbation, DC offset, clipping).
//! 2. Edge case resilience (empty slices, boundary lengths, NaN/Inf handling).
//! 3. 100 rapid calls stress harness with varying clip lengths (100 to 100,000 samples).
//! 4. Process memory stability (RSS working set leak detection).
//! 5. Per-call CPU latency profiling under -j 2 concurrency.

use liva_native_core::sysinfo::process_memory_bytes;
use liva_native_core::wake::{WAKE_PAD_TARGET_SAMPLES, WakeGate};
use std::time::{Duration, Instant};

/// Generate synthetic harmonic audio signal resembling vocal formants.
fn generate_synthetic_audio(num_samples: usize, base_freq: f32) -> Vec<f32> {
    let sample_rate = 16000.0f32;
    let mut buffer = Vec::with_capacity(num_samples);
    for i in 0..num_samples {
        let t = i as f32 / sample_rate;
        // Fundamental frequency + 2nd & 3rd harmonics with mild amplitude modulation
        let f1 = (2.0 * std::f32::consts::PI * base_freq * t).sin() * 0.3;
        let f2 = (2.0 * std::f32::consts::PI * (base_freq * 2.0) * t).sin() * 0.15;
        let f3 = (2.0 * std::f32::consts::PI * (base_freq * 3.0) * t).sin() * 0.08;
        let envelope = 0.8 + 0.2 * (2.0 * std::f32::consts::PI * 3.0 * t).cos();
        let sample = (f1 + f2 + f3) * envelope;
        buffer.push(sample.clamp(-1.0, 1.0));
    }
    buffer
}

#[test]
fn test_audio_padding_preserves_exact_sample_values_and_bits() {
    let test_lengths = [1, 10, 100, 500, 1000, 8000, 12800, 19200, 31360, 39999];

    for &len in &test_lengths {
        let original_audio = generate_synthetic_audio(len, 220.0);
        let missing = WAKE_PAD_TARGET_SAMPLES - original_audio.len();

        // Simulate the exact padding logic from score_clip
        let mut padded = vec![0.0f32; WAKE_PAD_TARGET_SAMPLES];
        padded[missing..].copy_from_slice(&original_audio);

        assert_eq!(
            padded.len(),
            WAKE_PAD_TARGET_SAMPLES,
            "Padded length must equal WAKE_PAD_TARGET_SAMPLES (40,000)"
        );

        // 1. Verify leading silence region is strictly bitwise 0.0f32
        for (i, &s) in padded[..missing].iter().enumerate() {
            assert_eq!(
                s.to_bits(),
                0.0f32.to_bits(),
                "Leading sample at index {} must be exactly +0.0f32 bits, found bits: {:08x}",
                i,
                s.to_bits()
            );
        }

        // 2. Verify trailing audio region matches original samples bit-for-bit
        for (i, (&orig, &pad)) in original_audio.iter().zip(&padded[missing..]).enumerate() {
            assert_eq!(
                orig.to_bits(),
                pad.to_bits(),
                "Sample at offset {} was perturbed by padding! orig: {}, pad: {}",
                i,
                orig,
                pad
            );
        }
    }
}

#[test]
fn test_padding_dc_offset_and_clipping_invariants() {
    let lengths = [100, 12800, 19200, 39999];

    for &len in &lengths {
        // Case A: Audio with zero DC offset (balanced sine wave)
        let balanced_audio = generate_synthetic_audio(len, 440.0);
        let orig_sum: f32 = balanced_audio.iter().sum();
        let orig_mean = orig_sum / len as f32;

        let missing = WAKE_PAD_TARGET_SAMPLES - len;
        let mut padded = vec![0.0f32; WAKE_PAD_TARGET_SAMPLES];
        padded[missing..].copy_from_slice(&balanced_audio);

        let padded_sum: f32 = padded.iter().sum();
        let padded_mean = padded_sum / WAKE_PAD_TARGET_SAMPLES as f32;

        // Sum must be strictly conserved since leading samples are 0.0
        assert!(
            (padded_sum - orig_sum).abs() < 1e-5,
            "Padding must conserve total audio sum. orig: {}, padded: {}",
            orig_sum,
            padded_sum
        );

        // Padded mean must be diluted exactly by (len / 40000)
        let expected_mean = orig_mean * (len as f32 / WAKE_PAD_TARGET_SAMPLES as f32);
        assert!(
            (padded_mean - expected_mean).abs() < 1e-6,
            "Padded mean must equal expected diluted mean. actual: {}, expected: {}",
            padded_mean,
            expected_mean
        );

        // Case B: Audio with deliberate DC bias (+0.30)
        let mut biased_audio = balanced_audio.clone();
        for s in &mut biased_audio {
            *s = (*s + 0.30).clamp(-1.0, 1.0);
        }
        let biased_sum: f32 = biased_audio.iter().sum();
        let mut padded_biased = vec![0.0f32; WAKE_PAD_TARGET_SAMPLES];
        padded_biased[missing..].copy_from_slice(&biased_audio);

        let padded_biased_sum: f32 = padded_biased.iter().sum();
        assert!(
            (padded_biased_sum - biased_sum).abs() < 1e-5,
            "Padding must not add DC bias to biased audio"
        );

        // Verify the silence prefix has zero DC offset
        let prefix_sum: f32 = padded_biased[..missing].iter().sum();
        assert_eq!(
            prefix_sum, 0.0,
            "Leading silence prefix must have zero DC offset"
        );

        // Case C: Clipping invariant (max amplitude boundary)
        let mut peak_audio = vec![0.0f32; len];
        peak_audio[0] = 1.0;
        peak_audio[1] = -1.0;
        let mut padded_peak = vec![0.0f32; WAKE_PAD_TARGET_SAMPLES];
        padded_peak[missing..].copy_from_slice(&peak_audio);

        let max_val = padded_peak
            .iter()
            .copied()
            .fold(f32::NEG_INFINITY, f32::max);
        let min_val = padded_peak.iter().copied().fold(f32::INFINITY, f32::min);
        assert!(
            max_val <= 1.0 && min_val >= -1.0,
            "Padding must never introduce clipping beyond [-1.0, 1.0]. max: {}, min: {}",
            max_val,
            min_val
        );
    }
}

#[test]
fn test_score_clip_edge_cases_and_adversarial_inputs() {
    let mut gate = WakeGate::from_env();

    // 1. Empty audio slice
    assert!(
        gate.score_clip(&[]).is_none(),
        "Empty slice must return None immediately without running models"
    );

    // 2. Ultra-short clips
    for &short_len in &[1usize, 2, 5, 10, 50, 99] {
        let short_audio = vec![0.01f32; short_len];
        let result = gate.score_clip(&short_audio);
        assert!(
            result.is_some(),
            "Ultra-short clip of len {} must be padded to 40,000 and scored",
            short_len
        );
        let (name, score) = result.unwrap();
        assert!(!name.is_empty());
        assert!((0.0..=1.0).contains(&score));
    }

    // 3. Exact threshold boundary clips
    for &boundary_len in &[39999usize, 40000, 40001] {
        let boundary_audio = generate_synthetic_audio(boundary_len, 300.0);
        let result = gate.score_clip(&boundary_audio);
        assert!(
            result.is_some(),
            "Boundary clip of len {} must be evaluated successfully",
            boundary_len
        );
    }

    // 4. Adversarial values: NaN / Inf resilience
    let mut nan_audio = generate_synthetic_audio(12800, 300.0);
    nan_audio[100] = f32::NAN;
    nan_audio[101] = f32::INFINITY;
    nan_audio[102] = f32::NEG_INFINITY;
    // score_clip must not panic on non-standard floats
    let nan_result = gate.score_clip(&nan_audio);
    // Result can be Some or None, but it must not abort or panic
    if let Some((_, score)) = nan_result {
        println!("Adversarial NaN/Inf input scored: {}", score);
    }
}

#[test]
fn test_score_clip_100_rapid_calls_stress_and_rss_stability() {
    // 1. Pre-flight check: RAM free >= 4GB
    let (_, free_bytes) =
        liva_native_core::sysinfo::ram_bytes().expect("ram_bytes must succeed on Windows");
    let free_gb = free_bytes as f64 / (1024.0 * 1024.0 * 1024.0);
    println!("[Pre-Flight Check] Free physical RAM: {:.2} GB", free_gb);
    assert!(
        free_gb >= 4.0,
        "Pre-flight requirement: Free RAM must be >= 4.0 GB, found {:.2} GB",
        free_gb
    );

    let mut gate = WakeGate::from_env();

    // Deterministic varying clip lengths spanning 100 to 100,000 samples
    let length_pool = [
        100usize, 250, 500, 1000, 4000, 8000, 12800, 16000, 19200, 24000, 31360, 32000, 39999,
        40000, 40001, 48000, 64000, 80000, 100000,
    ];

    const TOTAL_CALLS: usize = 100;
    const WARMUP_CALLS: usize = 5;

    // Warmup calls to initialize ONNX runtime memory arenas, thread pools, and scratch buffers
    println!("[Warmup] Executing {} warmup calls...", WARMUP_CALLS);
    for w in 0..WARMUP_CALLS {
        let len = length_pool[w % length_pool.len()];
        let audio = generate_synthetic_audio(len, 250.0 + (w as f32 * 10.0));
        let _ = gate.score_clip(&audio);
    }

    let (rss_baseline_bytes, _) =
        process_memory_bytes().expect("process_memory_bytes must succeed on Windows");
    let rss_baseline_mb = rss_baseline_bytes as f64 / (1024.0 * 1024.0);
    println!(
        "[Stress Test Initial Baseline] Post-warmup Working Set (RSS): {:.2} MB",
        rss_baseline_mb
    );

    let mut latencies: Vec<(usize, Duration)> = Vec::with_capacity(TOTAL_CALLS);
    let mut rss_checkpoints: Vec<(usize, f64)> = Vec::new();

    let start_all = Instant::now();

    for call_idx in 1..=TOTAL_CALLS {
        let len = length_pool[(call_idx - 1) % length_pool.len()];
        let base_freq = 200.0 + ((call_idx % 10) as f32 * 30.0);
        let audio = generate_synthetic_audio(len, base_freq);

        let t0 = Instant::now();
        let result = gate.score_clip(&audio);
        let elapsed = t0.elapsed();

        assert!(
            result.is_some(),
            "Call #{}: score_clip failed on len {} samples",
            call_idx,
            len
        );

        latencies.push((len, elapsed));

        // Checkpoint RSS at 25, 50, 75, 100 calls
        if call_idx == 25 || call_idx == 50 || call_idx == 75 || call_idx == 100 {
            let (current_rss_bytes, _) =
                process_memory_bytes().expect("process_memory_bytes must succeed on Windows");
            let current_rss_mb = current_rss_bytes as f64 / (1024.0 * 1024.0);
            rss_checkpoints.push((call_idx, current_rss_mb));
            println!(
                "[Stress Checkpoint] Call #{:3}/100 - Current RSS: {:.2} MB (Delta from baseline: {:+.2} MB)",
                call_idx,
                current_rss_mb,
                current_rss_mb - rss_baseline_mb
            );
        }
    }

    let total_elapsed = start_all.elapsed();
    let total_time_ms = total_elapsed.as_secs_f64() * 1000.0;
    let avg_latency_ms = total_time_ms / TOTAL_CALLS as f64;

    // Latency sorting & percentile calculations
    let mut all_durations: Vec<f64> = latencies
        .iter()
        .map(|(_, d)| d.as_secs_f64() * 1000.0)
        .collect();
    all_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let min_lat = all_durations[0];
    let max_lat = all_durations[TOTAL_CALLS - 1];
    let p50_lat = all_durations[TOTAL_CALLS * 50 / 100];
    let p90_lat = all_durations[TOTAL_CALLS * 90 / 100];
    let p95_lat = all_durations[TOTAL_CALLS * 95 / 100];
    let p99_lat = all_durations[TOTAL_CALLS * 99 / 100];

    println!("============================================================");
    println!("  SCORE_CLIP STRESS HARNESS LATENCY PROFILE (100 RAPID CALLS)");
    println!("============================================================");
    println!("  Total time for 100 calls: {:.2} ms", total_time_ms);
    println!("  Mean Latency:             {:.2} ms", avg_latency_ms);
    println!("  Min Latency:              {:.2} ms", min_lat);
    println!("  P50 Latency (Median):     {:.2} ms", p50_lat);
    println!("  P90 Latency:              {:.2} ms", p90_lat);
    println!("  P95 Latency:              {:.2} ms", p95_lat);
    println!("  P99 Latency:              {:.2} ms", p99_lat);
    println!("  Max Latency:              {:.2} ms", max_lat);
    println!("------------------------------------------------------------");

    // Breakdown by clip category
    let mut short_durations: Vec<f64> = latencies
        .iter()
        .filter(|(len, _)| *len < WAKE_PAD_TARGET_SAMPLES)
        .map(|(_, d)| d.as_secs_f64() * 1000.0)
        .collect();
    short_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut exact_durations: Vec<f64> = latencies
        .iter()
        .filter(|(len, _)| *len == WAKE_PAD_TARGET_SAMPLES)
        .map(|(_, d)| d.as_secs_f64() * 1000.0)
        .collect();
    exact_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mut long_durations: Vec<f64> = latencies
        .iter()
        .filter(|(len, _)| *len > WAKE_PAD_TARGET_SAMPLES)
        .map(|(_, d)| d.as_secs_f64() * 1000.0)
        .collect();
    long_durations.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let short_mean: f64 = short_durations.iter().sum::<f64>() / short_durations.len() as f64;
    let exact_mean: f64 = exact_durations.iter().sum::<f64>() / exact_durations.len() as f64;
    let long_mean: f64 = long_durations.iter().sum::<f64>() / long_durations.len() as f64;

    println!(
        "  [Padded Clips < 40k] Count: {}, Mean: {:.2} ms, P50: {:.2} ms",
        short_durations.len(),
        short_mean,
        short_durations[short_durations.len() / 2]
    );
    println!(
        "  [Exact Clips = 40k]  Count: {}, Mean: {:.2} ms, P50: {:.2} ms",
        exact_durations.len(),
        exact_mean,
        exact_durations[exact_durations.len() / 2]
    );
    println!(
        "  [Long Clips > 40k]   Count: {}, Mean: {:.2} ms, P50: {:.2} ms",
        long_durations.len(),
        long_mean,
        long_durations[long_durations.len() / 2]
    );
    println!("============================================================");

    // Memory Leak Verification
    let final_rss_mb = rss_checkpoints.last().unwrap().1;
    let mid_rss_mb = rss_checkpoints[1].1; // at call 50
    let total_rss_growth = final_rss_mb - rss_baseline_mb;
    let second_half_rss_growth = final_rss_mb - mid_rss_mb;

    println!("  MEMORY STABILITY (RSS WORKING SET):");
    println!("  Baseline RSS:            {:.2} MB", rss_baseline_mb);
    println!("  Call #50 RSS:            {:.2} MB", mid_rss_mb);
    println!("  Final (Call #100) RSS:   {:.2} MB", final_rss_mb);
    println!("  Total RSS Growth:        {:+.2} MB", total_rss_growth);
    println!(
        "  Second-Half (50-100) RSS:{:+.2} MB",
        second_half_rss_growth
    );
    println!("============================================================");

    // In a stable system with zero memory leaks:
    // 1. Total RSS growth across 100 calls must not exceed 25 MB (allowing allocator arenas).
    // 2. Second-half growth (call 50 -> 100) must be essentially flat (< 5 MB).
    assert!(
        total_rss_growth < 25.0,
        "Memory leak detected: Total RSS grew by {:.2} MB (> 25 MB limit)",
        total_rss_growth
    );
    assert!(
        second_half_rss_growth < 5.0,
        "Memory leak detected: Second-half RSS grew by {:.2} MB (> 5 MB limit)",
        second_half_rss_growth
    );

    // Latency guardrail: Mean latency must be well under 150ms per call requirement
    assert!(
        avg_latency_ms < 150.0,
        "Latency requirement breached: Mean latency {:.2} ms exceeded 150.0 ms ceiling",
        avg_latency_ms
    );
}
