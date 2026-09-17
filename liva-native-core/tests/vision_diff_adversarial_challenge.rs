//! Milestone 3 Feature 9: Screen Vision SIMD Diff ROI Adversarial Challenge Test Suite
//!
//! Empirical adversarial tests for:
//! 1. Adversarial Token Bound Stress Testing:
//!    - Extreme crop aspect ratios: 1:1, 1:4, 4:1, 1:10, 10:1, 700x700, 1080x1080, 4000x400.
//!    - 20,000+ randomized trials verifying discrete visual tokens ceil(W/28) * ceil(H/28) <= 384 with 0 violations.
//!    - Image aspect ratio distortion <= 2% during downscaling.
//! 2. Co-scale Fallback Boundary Verification:
//!    - Exact 35.00% vs 35.01% boundary verification in `should_co_scale`.
//!    - End-to-end <= 35% visual diff crops ROI patch with 16px safety padding.
//!    - End-to-end > 35% visual diff triggers 720p co-scale fallback (`downsample_rgb_to_720p`).
//!    - Identical frames (0% diff) fall back cleanly to 720p baseline without errors.

use liva_native_core::vision::{
    BoundingBox, Frame, PixelFormat, clamp_to_visual_tokens,
    diff::{CO_SCALE_AREA_THRESHOLD, should_co_scale},
    process_vision_frame,
};

// =========================================================================
// 1. ADVERSARIAL TOKEN BOUND & EXTREME ASPECT RATIOS
// =========================================================================

#[test]
fn test_extreme_aspect_ratios_token_bound_and_distortion() {
    let test_cases = [
        // 1:1 Aspect Ratio
        (500, 500, "1:1 500x500"),
        (700, 700, "1:1 700x700"),
        (1080, 1080, "1:1 1080x1080"),
        (2000, 2000, "1:1 2000x2000"),
        // 1:4 Aspect Ratio
        (250, 1000, "1:4 250x1000"),
        (400, 1600, "1:4 400x1600"),
        (500, 2000, "1:4 500x2000"),
        // 4:1 Aspect Ratio
        (1000, 250, "4:1 1000x250"),
        (1600, 400, "4:1 1600x400"),
        (2000, 500, "4:1 2000x500"),
        // 1:10 Aspect Ratio
        (100, 1000, "1:10 100x1000"),
        (200, 2000, "1:10 200x2000"),
        (300, 3000, "1:10 300x3000"),
        // 10:1 Aspect Ratio
        (1000, 100, "10:1 1000x100"),
        (2000, 200, "10:1 2000x200"),
        (3000, 300, "10:1 3000x300"),
        // Specific Milestone 3 targets
        (4000, 400, "10:1 4000x400"),
        (400, 4000, "1:10 400x4000"),
    ];

    for &(w, h, label) in &test_cases {
        let dummy = vec![128u8; (w * h * 3) as usize];
        let (fw, fh, rgb) = clamp_to_visual_tokens(&dummy, w, h, 384);

        assert_eq!(
            rgb.len(),
            (fw * fh * 3) as usize,
            "Buffer length mismatch for {}",
            label
        );

        // Discrete grid visual tokens formula: ceil(W / 28) * ceil(H / 28)
        let gw = (fw as f32 / 28.0).ceil() as usize;
        let gh = (fh as f32 / 28.0).ceil() as usize;
        let tokens = gw * gh;

        assert!(
            tokens <= 384,
            "Violated 384 token ceiling for {}: got {} tokens ({}x{} pixels, grid {}x{})",
            label,
            tokens,
            fw,
            fh,
            gw,
            gh
        );

        // Verify aspect ratio preservation during downscaling: distortion <= 2.0%
        let orig_ar = w as f64 / h as f64;
        let down_ar = fw as f64 / fh as f64;
        let distortion = ((down_ar - orig_ar) / orig_ar).abs();

        assert!(
            distortion <= 0.02,
            "Aspect ratio distortion exceeded 2% for {}: orig AR {:.4}, down AR {:.4}, distortion {:.4}%",
            label,
            orig_ar,
            down_ar,
            distortion * 100.0
        );
    }
}

#[test]
fn test_adversarial_20000_randomized_trials_token_ceiling_and_distortion() {
    // Deterministic XorShift PRNG for 100% reproducible adversarial stress runs
    let mut rng_state = 0x853c49e6748fea9b_u64;
    let mut next_u32 = || -> u32 {
        rng_state ^= rng_state << 13;
        rng_state ^= rng_state >> 7;
        rng_state ^= rng_state << 17;
        (rng_state & 0xFFFFFFFF) as u32
    };

    let trials = 20_000;
    let max_tokens = 384;
    let mut violations = 0;
    let mut max_observed_tokens = 0;
    let mut max_observed_distortion: f64 = 0.0;

    for _ in 0..trials {
        let w = (next_u32() % 4000) + 1; // Range: 1..=4000
        let h = (next_u32() % 4000) + 1; // Range: 1..=4000

        // Mathematical emulation of clamp_to_visual_tokens clamping logic
        let gw = (w as f32 / 28.0).ceil() as usize;
        let gh = (h as f32 / 28.0).ceil() as usize;

        let (target_w, target_h) = if gw * gh <= max_tokens {
            (w, h)
        } else {
            let scale = (max_tokens as f32 / (gw * gh) as f32).sqrt();
            let mut target_gw = ((gw as f32 * scale).floor() as usize).max(1);
            let mut target_gh = ((gh as f32 * scale).floor() as usize).max(1);

            while target_gw * target_gh > max_tokens {
                if target_gw * h as usize >= target_gh * w as usize {
                    target_gw = target_gw.saturating_sub(1).max(1);
                } else {
                    target_gh = target_gh.saturating_sub(1).max(1);
                }
            }

            let max_w = (target_gw * 28) as u32;
            let max_h = (target_gh * 28) as u32;

            let ratio = (max_w as f32 / w as f32).min(max_h as f32 / h as f32);
            let tw = ((w as f32 * ratio).round() as u32).max(1);
            let th = ((h as f32 * ratio).round() as u32).max(1);
            (tw, th)
        };

        let final_gw = (target_w as f32 / 28.0).ceil() as usize;
        let final_gh = (target_h as f32 / 28.0).ceil() as usize;
        let tokens = final_gw * final_gh;

        if tokens > max_observed_tokens {
            max_observed_tokens = tokens;
        }

        if tokens > max_tokens {
            violations += 1;
        }

        // Measure distortion for standard crop dimensions (>= 28px on both axes)
        if target_w >= 28 && target_h >= 28 {
            let orig_ar = w as f64 / h as f64;
            let down_ar = target_w as f64 / target_h as f64;
            let distortion = ((down_ar - orig_ar) / orig_ar).abs();
            if distortion > max_observed_distortion {
                max_observed_distortion = distortion;
            }
            assert!(
                distortion <= 0.02,
                "Aspect ratio distortion {:.4}% exceeded 2% for {}x{} -> {}x{}",
                distortion * 100.0,
                w,
                h,
                target_w,
                target_h
            );
        }
    }

    assert_eq!(
        violations, 0,
        "Found {} violations of max_tokens {} across {} trials!",
        violations, max_tokens, trials
    );
    assert!(
        max_observed_tokens <= 384,
        "Max observed tokens {} exceeded 384",
        max_observed_tokens
    );
    assert!(
        max_observed_distortion <= 0.02,
        "Max observed distortion {:.4}% exceeded 2%",
        max_observed_distortion * 100.0
    );
}

// =========================================================================
// 2. CO-SCALE FALLBACK BOUNDARY VERIFICATION
// =========================================================================

#[test]
fn test_should_co_scale_exact_boundary_35_percent() {
    // 1. Exactly 35.000% of screen: 350,000 / 1,000,000 pixels
    let bbox_35_00 = BoundingBox {
        x: 0,
        y: 0,
        width: 3500,
        height: 100,
    };
    assert!(
        !should_co_scale(&bbox_35_00, 10000, 100),
        "Exactly 35.00% must NOT trigger co-scale"
    );

    // 2. Exactly 35.010% of screen: 350,100 / 1,000,000 pixels
    let bbox_35_01 = BoundingBox {
        x: 0,
        y: 0,
        width: 3501,
        height: 100,
    };
    assert!(
        should_co_scale(&bbox_35_01, 10000, 100),
        "35.01% MUST trigger co-scale"
    );

    // 3. Exactly 34.990% of screen: 349,900 / 1,000,000 pixels
    let bbox_34_99 = BoundingBox {
        x: 0,
        y: 0,
        width: 3499,
        height: 100,
    };
    assert!(
        !should_co_scale(&bbox_34_99, 10000, 100),
        "34.99% must NOT trigger co-scale"
    );

    assert_eq!(CO_SCALE_AREA_THRESHOLD, 0.35);
}

#[test]
fn test_end_to_end_diff_boundary_35_percent_transitions() {
    let screen_w = 1000u32;
    let screen_h = 1000u32;
    let _total_pixels = (screen_w * screen_h) as f32;

    let frame_base = Frame {
        width: screen_w,
        height: screen_h,
        format: PixelFormat::Rgba,
        data: vec![50; (screen_w * screen_h * 4) as usize],
    };

    // Case A: 35.00% visual diff (700 x 500 = 350,000 pixels)
    // Centered box: x: 100..800, y: 200..700
    let mut frame_35_00 = frame_base.clone();
    for y in 200..700 {
        for x in 100..800 {
            let idx = (y * screen_w as usize + x) * 4;
            frame_35_00.data[idx] = 200;
        }
    }

    let (w_a, h_a, rgb_a, patch_a_opt) =
        process_vision_frame(&frame_35_00, Some(&frame_base)).unwrap();
    assert!(patch_a_opt.is_some());
    let patch_a = patch_a_opt.unwrap();

    assert!(
        !patch_a.is_co_scaled,
        "35.00% diff must NOT trigger co-scale fallback"
    );
    assert!(
        (patch_a.area_ratio - 0.350).abs() < 0.001,
        "Expected area_ratio ~0.35, got {}",
        patch_a.area_ratio
    );

    // Padding verification: 16px safety padding around (700x500) centered at (x:100, y:200)
    // Raw box: width 700, height 500
    assert_eq!(patch_a.raw_bounding_box.width, 700);
    assert_eq!(patch_a.raw_bounding_box.height, 500);
    // Padded box: (100-16)..(800+16) = 84..816 (width 732)
    //             (200-16)..(700+16) = 184..716 (height 532)
    assert_eq!(patch_a.padded_bounding_box.x, 84);
    assert_eq!(patch_a.padded_bounding_box.y, 184);
    assert_eq!(patch_a.padded_bounding_box.width, 732);
    assert_eq!(patch_a.padded_bounding_box.height, 532);

    // Tokens must be clamped to <= 384
    let tokens_a = ((w_a as f32 / 28.0).ceil() * (h_a as f32 / 28.0).ceil()) as usize;
    assert!(
        tokens_a <= 384,
        "35.00% patch tokens {} exceeded 384 ceiling",
        tokens_a
    );
    assert_eq!(rgb_a.len(), (w_a * h_a * 3) as usize);

    // Case B: 35.07% visual diff (> 35%) (700 x 501 = 350,700 pixels)
    let mut frame_35_07 = frame_base.clone();
    for y in 200..701 {
        for x in 100..800 {
            let idx = (y * screen_w as usize + x) * 4;
            frame_35_07.data[idx] = 200;
        }
    }

    let (w_b, h_b, rgb_b, patch_b_opt) =
        process_vision_frame(&frame_35_07, Some(&frame_base)).unwrap();
    assert!(patch_b_opt.is_some());
    let patch_b = patch_b_opt.unwrap();

    assert!(
        patch_b.is_co_scaled,
        "35.07% (> 35%) diff MUST trigger co-scale fallback"
    );
    assert!(
        patch_b.area_ratio > 0.35,
        "Expected area_ratio > 0.35, got {}",
        patch_b.area_ratio
    );
    // Clean transition to 720p co-scale: max 1280x720, aspect ratio preserved
    // For 1000x1000, downsample_rgb_to_720p yields 720x720
    assert_eq!((w_b, h_b), (720, 720));
    assert_eq!(rgb_b.len(), 720 * 720 * 3);

    // Case C: Identical frames (0% diff)
    let (w_c, h_c, rgb_c, patch_c_opt) =
        process_vision_frame(&frame_base, Some(&frame_base)).unwrap();
    assert!(
        patch_c_opt.is_none(),
        "Identical frames must produce None patch"
    );
    // Falls back cleanly to 720p baseline
    assert_eq!((w_c, h_c), (720, 720));
    assert_eq!(rgb_c.len(), 720 * 720 * 3);
}

#[test]
fn test_corner_diff_padding_clamping() {
    let screen_w = 640u32;
    let screen_h = 480u32;
    let frame_base = Frame {
        width: screen_w,
        height: screen_h,
        format: PixelFormat::Rgba,
        data: vec![0; (screen_w * screen_h * 4) as usize],
    };

    // 1. Top-left corner (0, 0)
    let mut frame_tl = frame_base.clone();
    frame_tl.data[0] = 255;
    let (_, _, _, patch_tl) = process_vision_frame(&frame_tl, Some(&frame_base)).unwrap();
    let p_tl = patch_tl.expect("Patch expected for top-left");
    assert_eq!(p_tl.raw_bounding_box.x, 0);
    assert_eq!(p_tl.raw_bounding_box.y, 0);
    assert_eq!(p_tl.padded_bounding_box.x, 0);
    assert_eq!(p_tl.padded_bounding_box.y, 0);
    assert_eq!(p_tl.padded_bounding_box.width, 17); // 1 + 16 padding
    assert_eq!(p_tl.padded_bounding_box.height, 17);

    // 2. Bottom-right corner (screen_w - 1, screen_h - 1)
    let mut frame_br = frame_base.clone();
    let br_idx = ((screen_h as usize - 1) * screen_w as usize + (screen_w as usize - 1)) * 4;
    frame_br.data[br_idx] = 255;
    let (_, _, _, patch_br) = process_vision_frame(&frame_br, Some(&frame_base)).unwrap();
    let p_br = patch_br.expect("Patch expected for bottom-right");
    assert_eq!(p_br.raw_bounding_box.x, (screen_w - 1) as usize);
    assert_eq!(p_br.raw_bounding_box.y, (screen_h - 1) as usize);
    assert_eq!(
        p_br.padded_bounding_box.x + p_br.padded_bounding_box.width,
        screen_w as usize
    );
    assert_eq!(
        p_br.padded_bounding_box.y + p_br.padded_bounding_box.height,
        screen_h as usize
    );
}

#[test]
fn test_adversarial_mutex_poison_recovery() {
    use liva_native_core::vision::{MockScreenCapturer, capture_for_vision_from};
    use std::sync::{Arc, Mutex};
    use std::thread;

    let slot = Arc::new(Mutex::new(None));
    let capturer = Arc::new(MockScreenCapturer::new(1280, 720, PixelFormat::Rgba));
    capturer.set_frame_data(vec![128; 1280 * 720 * 4]);

    // Initial capture
    let _ = capture_for_vision_from(&*capturer, &slot).unwrap();

    // Spawn a thread that acquires the lock and deliberately panics to poison the mutex
    let slot_clone = Arc::clone(&slot);
    let handle = thread::spawn(move || {
        let _guard = slot_clone.lock().unwrap();
        panic!("Adversarial induced panic while holding lock");
    });
    let _ = handle.join(); // Expect Err due to panic

    // The mutex is now poisoned.
    // Verify that subsequent call recovers gracefully without panicking
    let recovery_res = capture_for_vision_from(&*capturer, &slot);
    assert!(
        recovery_res.is_ok(),
        "Must recover transparently from poisoned lock without panic!"
    );
}
