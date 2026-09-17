//! Runtime opt-in suite for the vision path (mtmd) per implementation_plan.md.
//!
//! Opt-in by design: `LIVA_TEST_MODEL_PATH` (VL GGUF) and `LIVA_TEST_MMPROJ_PATH`
//! must point at a matching local model/projector pair. An explicit `--ignored`
//! run with missing fixtures FAILS with a configuration message — never a
//! silent pass. On Windows the engine intentionally blocks mtmd in debug
//! builds (CRT assert in the mmproj loader), so acceptance runs use `--release`.

use liva_native_core::llm::engine::{LlamaRouterManager, VisionImage};

const N_CTX: usize = 4096;

fn max_prompt_tokens() -> usize {
    N_CTX - liva_native_core::llm::engine::RESERVE_FOR_COMPLETION - 1
}

fn require_fixture(var: &str) -> std::path::PathBuf {
    let Some(path) = std::env::var_os(var) else {
        panic!(
            "{var} is not set. Point it at the local VL GGUF/mmproj fixture; \
             an explicit --ignored run must fail loudly instead of faking a pass."
        )
    };
    let path = std::path::PathBuf::from(path);
    assert!(
        path.is_file(),
        "{var} does not point at a file: {}",
        path.display()
    );
    path
}

/// Fixture pixels only — a solid-color block, no screen content, no personal data.
fn solid_rgb(width: u32, height: u32, r: u8, g: u8, b: u8) -> Vec<u8> {
    let mut data = Vec::with_capacity((width * height * 3) as usize);
    for _ in 0..(width * height) {
        data.extend_from_slice(&[r, g, b]);
    }
    data
}

#[tokio::test]
#[ignore = "requires LIVA_TEST_MODEL_PATH + LIVA_TEST_MMPROJ_PATH (VL model + projector); run with --ignored"]
async fn runtime_vision_budget_rejects_oversized_and_recovers() {
    if cfg!(all(windows, debug_assertions)) {
        panic!(
            "Vision runtime acceptance requires a release build on Windows: the \
             engine blocks debug mtmd on purpose (CRT assert in the mmproj loader). \
             Re-run with: cargo test --release --test vision_budget_runtime -- --ignored"
        );
    }
    let model_path = require_fixture("LIVA_TEST_MODEL_PATH");
    let mmproj_path = require_fixture("LIVA_TEST_MMPROJ_PATH");

    let mut manager = LlamaRouterManager::new(N_CTX, 0).expect("manager");
    manager
        .swap_model(&model_path, Some(N_CTX), Some(0), Some(false))
        .await
        .expect("full VL model load");
    manager.mmproj_path = Some(mmproj_path);

    // 1) Persona + image marker + current question are all mandatory: an
    //    oversized question must be rejected by the real mtmd measurement
    //    (chunks.total_tokens()), never by a text-only estimate, and must
    //    not emit a single token.
    let rgb = solid_rgb(2, 2, 200, 40, 40);
    let emissions = std::sync::atomic::AtomicUsize::new(0);
    let budget_error = manager
        .answer_with_image(
            &format!("mô tả {}trong ảnh này", "chi tiết ".repeat(6_000)),
            VisionImage::Rgb {
                width: 2,
                height: 2,
                data: &rgb,
            },
            0.7,
            0.9,
            |_| {
                emissions.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                true
            },
        )
        .expect_err("oversized mandatory vision prompt must be rejected");
    assert!(
        budget_error.contains("Required prompt content exceeds budget"),
        "unexpected error: {budget_error}"
    );
    assert_eq!(
        emissions.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a budget-rejected vision request must not emit any token"
    );

    // 2) Corrupted encoded bytes hit the real stb_image decoder through the
    //    FFI binding and must come back as a clean error, not an abort.
    let decode_error = manager
        .answer_with_image(
            "Ảnh này là gì?",
            VisionImage::Encoded(b"this is not a png at all"),
            0.7,
            0.9,
            |_| true,
        )
        .expect_err("corrupted image bytes must be rejected by the decoder");
    assert!(!decode_error.is_empty());

    // 3) A valid request right after both failures still runs end to end and
    //    reports the measured multimodal total inside the strict budget.
    let recovered = manager
        .answer_with_image(
            "Màu chủ đạo của ảnh này là gì? Trả lời một từ.",
            VisionImage::Rgb {
                width: 2,
                height: 2,
                data: &rgb,
            },
            0.7,
            0.9,
            |_| true,
        )
        .expect("valid vision request after errors must succeed");
    assert!(
        recovered.prompt_tokens > 0,
        "mtmd total_tokens must be measured"
    );
    assert!(
        recovered.prompt_tokens <= max_prompt_tokens(),
        "prompt_tokens {} exceeds the strict budget {}",
        recovered.prompt_tokens,
        max_prompt_tokens()
    );
    assert!(!recovered.text.trim().is_empty());
}
