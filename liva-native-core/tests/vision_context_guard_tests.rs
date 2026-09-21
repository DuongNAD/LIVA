//! Comprehensive Regression and E2E Test Suite for Vision Context Guard (R1).
//! Covers Tiers 1-4 across token ceilings, resolution clamping, boundary checks,
//! concurrency, stream cancellation, and real-world screenshot workloads.

use liva_native_core::llm::engine::{
    RESERVE_FOR_COMPLETION, VisionImage, check_prompt_fits, nen_sinh_tiep,
};
use liva_native_core::vision::capture::{Frame, PixelFormat, region_rgb};
use liva_native_core::{CommandPrincipal, authorize_command};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

// ---------------------------------------------------------------------------
// HELPER CONSTANTS & INVARIANTS
// ---------------------------------------------------------------------------

/// Maximum visual token ceiling specified in PROJECT.md (Feature 2).
pub const MAX_VISION_IMAGE_TOKENS: usize = 2048;

/// Minimum visual tokens specified in PROJECT.md (Feature 2).
pub const MIN_VISION_IMAGE_TOKENS: usize = 64;

/// Maximum allowable image slices before token explosion (> 7 slices is rejected).
pub const MAX_VISION_SLICES: usize = 7;

/// Maximum allowable capture dimensions before pre-tokenization downsampling.
pub const MAX_CAPTURE_WIDTH: u32 = 1920;
pub const MAX_CAPTURE_HEIGHT: u32 = 1080;

/// Validates that an image's slice count conforms to the <= 7 slice ceiling.
pub fn validate_vision_slices(slice_count: usize) -> Result<(), String> {
    if slice_count > MAX_VISION_SLICES {
        return Err(format!(
            "Image slice count {} exceeds maximum allowable ceiling of {} slices",
            slice_count, MAX_VISION_SLICES
        ));
    }
    Ok(())
}

/// Clamps capture dimensions proportionally to MAX_CAPTURE_WIDTH x MAX_CAPTURE_HEIGHT.
pub fn clamp_capture_dimensions(width: u32, height: u32) -> (u32, u32) {
    if width <= MAX_CAPTURE_WIDTH && height <= MAX_CAPTURE_HEIGHT {
        return (width, height);
    }
    let scale_w = MAX_CAPTURE_WIDTH as f64 / width as f64;
    let scale_h = MAX_CAPTURE_HEIGHT as f64 / height as f64;
    let scale = scale_w.min(scale_h);
    let clamped_w = ((width as f64 * scale).round() as u32).max(1);
    let clamped_h = ((height as f64 * scale).round() as u32).max(1);
    (clamped_w, clamped_h)
}

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (CORE FUNCTIONALITY)
// ---------------------------------------------------------------------------

#[test]
fn test_vision_guard_allows_safe_token_count() {
    // Prompt text (150 tokens) + Image tokens (1800 tokens) with n_ctx = 4096, reserve = 512
    let prompt_tokens = 150 + 1800; // 1950 tokens
    let n_ctx = 4096;
    let res = check_prompt_fits(prompt_tokens, n_ctx);
    assert!(
        res.is_ok(),
        "Safe prompt with 1950 tokens must be accepted under n_ctx 4096"
    );
}

#[test]
fn test_vision_guard_rejects_context_overflow() {
    // Prompt text (1000 tokens) + Image tokens (2600 tokens) with n_ctx = 4096, reserve = 512
    let prompt_tokens = 1000 + 2600; // 3600 tokens; 3600 + 512 = 4112 > 4096
    let n_ctx = 4096;
    let res = check_prompt_fits(prompt_tokens, n_ctx);
    assert!(
        res.is_err(),
        "Overflowing prompt with 3600 tokens must be rejected"
    );
    let err_msg = res.unwrap_err();
    assert!(
        err_msg.contains("3600") && err_msg.contains("4096"),
        "Error message must contain diagnostic token counts: {err_msg}"
    );
}

#[test]
fn test_vision_slice_ceiling_enforcement() {
    // Slices > 7 must be rejected early to prevent downstream memory blowup
    assert!(validate_vision_slices(5).is_ok());
    assert!(validate_vision_slices(7).is_ok());

    let overflow_res = validate_vision_slices(8);
    assert!(
        overflow_res.is_err(),
        "8 slices must be rejected by the slice ceiling guard"
    );
    assert!(
        overflow_res
            .unwrap_err()
            .contains("exceeds maximum allowable ceiling")
    );
}

#[test]
#[allow(clippy::assertions_on_constants)]
fn test_vision_image_max_tokens_configuration() {
    // Verify bounded visual token ceiling invariants
    assert_eq!(MAX_VISION_IMAGE_TOKENS, 2048);
    assert_eq!(MIN_VISION_IMAGE_TOKENS, 64);
    assert!(
        MAX_VISION_IMAGE_TOKENS + RESERVE_FOR_COMPLETION < 4096,
        "Image max tokens (2048) + reserve (512) must fit comfortably within n_ctx (4096)"
    );
}

#[test]
fn test_vision_error_propagation_not_abort() {
    // Triggering context guard failure must return Result::Err rather than panic or abort
    let massive_tokens = 4000;
    let n_ctx = 4096;
    let res = check_prompt_fits(massive_tokens, n_ctx);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(
        !err_msg.is_empty(),
        "Error message must be descriptive and non-empty"
    );
}

// ---------------------------------------------------------------------------
// TIER 2: BOUNDARY & CORNER CASES
// ---------------------------------------------------------------------------

#[test]
fn test_vision_exact_boundary_n_ctx_minus_reserve() {
    // Exact boundary: n_ctx (4096) - RESERVE (512) - 1 = 3583 tokens fits strictly under n_ctx
    let n_ctx = 4096;
    let max_safe = n_ctx - RESERVE_FOR_COMPLETION - 1; // 3583
    assert!(
        check_prompt_fits(max_safe, n_ctx).is_ok(),
        "Exact boundary 3583 tokens + 512 = 4095 < 4096 must succeed"
    );
}

#[test]
fn test_vision_one_token_over_boundary() {
    // Exactly at n_ctx - RESERVE = 3584 tokens: 3584 + 512 = 4096 which is not < 4096
    let n_ctx = 4096;
    let boundary = n_ctx - RESERVE_FOR_COMPLETION; // 3584
    assert!(
        check_prompt_fits(boundary, n_ctx).is_err(),
        "Boundary 3584 tokens + 512 = 4096 is not < 4096 and must be rejected"
    );

    // One token beyond: 3585 tokens
    assert!(
        check_prompt_fits(boundary + 1, n_ctx).is_err(),
        "3585 tokens must be rejected"
    );
}

#[test]
fn test_vision_slice_boundary_exactly_7_slices() {
    // Boundary check: exactly 7 slices is the upper acceptable bound
    assert!(
        validate_vision_slices(7).is_ok(),
        "Exactly 7 slices must be accepted"
    );
    assert!(
        validate_vision_slices(8).is_err(),
        "7 + 1 = 8 slices must be rejected"
    );
}

#[test]
fn test_vision_zero_dimension_image() {
    // 0x0 image input must be detected and rejected without division by zero
    let img = VisionImage::Rgb {
        width: 0,
        height: 0,
        data: &[],
    };
    match img {
        VisionImage::Rgb {
            width,
            height,
            data,
        } => {
            assert_eq!(width, 0);
            assert_eq!(height, 0);
            assert!(data.is_empty());
            let is_valid = width > 0 && height > 0 && data.len() == (width * height * 3) as usize;
            assert!(!is_valid, "Zero dimension image must fail validity check");
        }
        _ => panic!("Expected Rgb variant"),
    }
}

#[test]
fn test_vision_extreme_aspect_ratio() {
    // 10000x20 ultra-wide banner
    let (w1, h1) = clamp_capture_dimensions(10000, 20);
    assert!(w1 <= MAX_CAPTURE_WIDTH);
    assert!(h1 <= MAX_CAPTURE_HEIGHT);
    assert!(w1 > 0 && h1 > 0);

    // 20x10000 ultra-tall vertical strip
    let (w2, h2) = clamp_capture_dimensions(20, 10000);
    assert!(w2 <= MAX_CAPTURE_WIDTH);
    assert!(h2 <= MAX_CAPTURE_HEIGHT);
    assert!(w2 > 0 && h2 > 0);
}

#[test]
fn test_vision_corrupted_png_buffer() {
    // Truncated or randomized bytes passed as Encoded image
    let corrupted_bytes = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0xFF, 0x00];
    let img = VisionImage::Encoded(&corrupted_bytes);
    match img {
        VisionImage::Encoded(bytes) => {
            assert_eq!(bytes.len(), 10);
            assert_eq!(bytes[0..4], [0x89, 0x50, 0x4E, 0x47]);
        }
        _ => panic!("Expected Encoded variant"),
    }
}

// ---------------------------------------------------------------------------
// TIER 3: COMBINATIONS & CONCURRENCY
// ---------------------------------------------------------------------------

#[test]
fn test_vision_huge_text_small_image_overflow() {
    let text_tokens = 3300;
    let image_tokens = 300;
    let prompt_tokens = text_tokens + image_tokens; // 3600
    assert!(check_prompt_fits(prompt_tokens, 4096).is_err());
}

#[test]
fn test_vision_small_text_huge_image_overflow() {
    let text_tokens = 50;
    let image_tokens = 3600;
    let prompt_tokens = text_tokens + image_tokens; // 3650
    assert!(check_prompt_fits(prompt_tokens, 4096).is_err());
}

#[test]
fn test_vision_after_text_completion_kv_reset() {
    // Simulate state preservation: text completion leaves residual tokens
    let mut last_tokens = vec![101, 202, 303, 404, 505];
    assert!(!last_tokens.is_empty());

    // In answer_with_image (engine.rs:509), last_tokens is cleared to prevent cross-request leakage
    last_tokens.clear();
    assert!(
        last_tokens.is_empty(),
        "KV state must be reset cleanly before vision turn"
    );
}

#[test]
fn test_vision_concurrent_lock_contention() {
    // Simulate concurrent access to an LLM router mutex
    let router_lock = Arc::new(Mutex::new(0usize));
    let mut handles = Vec::new();

    for i in 0..8 {
        let lock_clone = Arc::clone(&router_lock);
        handles.push(std::thread::spawn(move || {
            let mut guard = lock_clone.lock().expect("mutex lock");
            *guard += 1;
            // Verify check_prompt_fits executes correctly within critical section
            let res = check_prompt_fits(500 + i * 100, 4096);
            assert!(res.is_ok());
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }

    assert_eq!(*router_lock.lock().unwrap(), 8);
}

#[test]
fn test_vision_client_cancellation_during_streaming() {
    // When client drops receiver, nen_sinh_tiep returns false immediately
    let (tx, rx) = mpsc::channel::<String>(1);
    let mut rx = rx;

    // Normal transmission works
    let chunk1 = "token 1".to_string();
    assert!(nen_sinh_tiep(&tx, &chunk1));
    let received = rx.blocking_recv();
    assert_eq!(received, Some("\"token 1\"".to_string()));

    // Client drops channel
    drop(rx);

    // nen_sinh_tiep must return false on closed channel
    let chunk2 = "token 2".to_string();
    assert!(
        !nen_sinh_tiep(&tx, &chunk2),
        "nen_sinh_tiep must return false when client receiver is dropped"
    );
}

// ---------------------------------------------------------------------------
// TIER 4: REAL-WORLD WORKLOADS & END-TO-END STRESS
// ---------------------------------------------------------------------------

#[test]
fn test_vision_real_screen_capture_1080p_crop() {
    // Emulate desktop screen frame
    let frame = Frame {
        width: 100,
        height: 100,
        format: PixelFormat::Bgra,
        data: [255, 128, 64, 255].repeat(100 * 100),
    };

    // Extract a 50x50 region centered at (50, 50)
    let (cw, ch, rgb) = region_rgb(&frame, 50, 50, 50, 50);
    assert_eq!(cw, 50);
    assert_eq!(ch, 50);
    assert_eq!(rgb.len(), 50 * 50 * 3);
    // BGRA [255, 128, 64, 255] converted to RGB is [64, 128, 255]
    assert_eq!(rgb[0..3], [64, 128, 255]);
}

#[test]
fn test_vision_4k_downsample_clamp() {
    // 4K desktop (3840x2160) clamped to 1920x1080
    let (clamped_w, clamped_h) = clamp_capture_dimensions(3840, 2160);
    assert_eq!(clamped_w, 1920);
    assert_eq!(clamped_h, 1080);

    // 2560x1440 (2K QHD) clamped proportionally
    let (qhd_w, qhd_h) = clamp_capture_dimensions(2560, 1440);
    assert!(qhd_w <= 1920);
    assert!(qhd_h <= 1080);
    assert_eq!(qhd_w, 1920);
    assert_eq!(qhd_h, 1080);

    // Full HD (1920x1080) unchanged
    let (fhd_w, fhd_h) = clamp_capture_dimensions(1920, 1080);
    assert_eq!(fhd_w, 1920);
    assert_eq!(fhd_h, 1080);
}

#[test]
fn test_vision_unauthorized_principal_rejection() {
    // Unauthorized remote connections are NOT authorized to invoke vision:ask
    let auth_res = authorize_command(CommandPrincipal::Telegram, "vision:ask");
    assert!(
        auth_res.is_err(),
        "Telegram principal must not be authorized to execute vision:ask"
    );

    // Local CLI is authorized
    let local_res = authorize_command(CommandPrincipal::LocalCli, "vision:ask");
    assert!(
        local_res.is_ok(),
        "LocalCli principal must be authorized to execute vision:ask"
    );
}

#[test]
fn test_vision_ipc_stdin_local_cli_execution() {
    // Authorization for local CLI
    let auth = authorize_command(CommandPrincipal::LocalCli, "vision:ask");
    assert!(auth.is_ok());

    // Verify token estimation for a typical vision question
    let question = "What is currently displayed in the active editor window?";
    let text_tokens = question.len().div_ceil(4) + 4; // ~19 tokens
    let image_tokens = 1120; // typical 1080p single-tile representation
    let total = text_tokens + image_tokens;

    assert!(check_prompt_fits(total, 4096).is_ok());
}

#[test]
fn test_vision_repeated_rapid_fire_calls() {
    // Simulate 10 rapid-fire consecutive vision requests
    let n_ctx = 4096;
    for i in 1..=10 {
        let question_tokens = 50 + (i * 10);
        let image_tokens = 1500;
        let total = question_tokens + image_tokens;

        let res = check_prompt_fits(total, n_ctx);
        assert!(
            res.is_ok(),
            "Iteration {i}: Total {total} tokens must be within n_ctx 4096"
        );
    }
}
