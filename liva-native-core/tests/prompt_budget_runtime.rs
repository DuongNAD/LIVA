//! Runtime opt-in suite for exact prompt budgeting on a real tokenizer/model.
//!
//! Opt-in by design (implementation_plan.md, mục [Testing] 3): set
//! `LIVA_TEST_MODEL_PATH` to a local instruct GGUF and run
//! `cargo test -p liva-native-core --release --test prompt_budget_runtime -- --ignored`.
//! A missing or invalid fixture must fail loudly — never skip as a silent pass.
//! Tokenizer-only checks are not reported as inference; this suite runs the
//! real `generate_budgeted_completion` entry end to end.

use liva_native_core::llm::ChatMessage;
use liva_native_core::llm::engine::LlamaRouterManager;

const N_CTX: usize = 4096;

/// Mirrors the ContextTokenBudget strict bound: prompt + reserve must stay
/// below n_ctx, so the largest accepted prompt is n_ctx - reserve - 1.
fn max_prompt_tokens() -> usize {
    N_CTX - liva_native_core::llm::engine::RESERVE_FOR_COMPLETION - 1
}

fn require_model() -> std::path::PathBuf {
    let Some(path) = std::env::var_os("LIVA_TEST_MODEL_PATH") else {
        panic!(
            "LIVA_TEST_MODEL_PATH is not set. Point it at a local instruct GGUF \
             for this opt-in runtime suite; an explicit run must fail with a \
             configuration message, not fake-pass."
        );
    };
    let path = std::path::PathBuf::from(path);
    assert!(
        path.is_file(),
        "LIVA_TEST_MODEL_PATH does not point at a file: {}",
        path.display()
    );
    path
}

fn msg(role: &str, content: impl Into<String>) -> ChatMessage {
    ChatMessage {
        role: role.to_string(),
        content: content.into(),
    }
}

fn long_dialogue() -> Vec<ChatMessage> {
    let mut messages = vec![msg("system", "Bạn là trợ lý LIVA. Trả lời ngắn gọn.")];
    for turn in 0..30u32 {
        messages.push(msg("user", format!("Lượt {turn}: hãy nhớ con số {turn}.")));
        messages.push(msg("assistant", "Đã ghi nhớ."));
    }
    messages.push(msg(
        "user",
        "Trong lượt hỏi gần nhất, tôi nhắc con số nào? Trả lời chỉ bằng con số.",
    ));
    messages
}

fn short_dialogue() -> Vec<ChatMessage> {
    vec![
        msg("system", "Bạn là trợ lý LIVA. Trả lời ngắn gọn."),
        msg("user", "Trả lời đúng một từ: mỗi tuần có mấy ngày?"),
    ]
}

#[tokio::test]
#[ignore = "requires LIVA_TEST_MODEL_PATH (real instruct GGUF); run with --ignored"]
async fn runtime_exact_budget_inference_cancellation_and_recovery() {
    let model_path = require_model();
    let mut manager = LlamaRouterManager::new(N_CTX, 0).expect("manager");
    manager
        .swap_model(&model_path, Some(N_CTX), Some(0), Some(false))
        .await
        .expect("full (non vocab-only) model load");

    // 1) Long dialogue: the exact-budget selector keeps the system constraint
    //    and the current user turn while evicting whole optional history
    //    groups; the reported token count is the compiled-prompt measurement.
    let dialogue = long_dialogue();
    let pieces = std::sync::atomic::AtomicUsize::new(0);
    let output = manager
        .generate_budgeted_completion(&dialogue, 0.7, 0.9, |piece| {
            if !piece.is_empty() {
                pieces.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            }
            true
        })
        .expect("long dialogue must fit and generate");
    assert!(
        !output.text.trim().is_empty(),
        "completion must not be empty"
    );
    assert!(
        output.prompt_tokens > 0 && output.prompt_tokens <= max_prompt_tokens(),
        "prompt_tokens {} must be an exact in-budget measurement (max {})",
        output.prompt_tokens,
        max_prompt_tokens()
    );
    assert!(
        pieces.load(std::sync::atomic::Ordering::SeqCst) > 0,
        "streaming pieces must reach the callback"
    );

    // 2) Mandatory content larger than the budget -> structured rejection,
    //    zero emitted tokens, no silent eviction of the current question.
    let oversized = vec![
        msg("system", "Bạn là trợ lý LIVA."),
        msg("user", "chi tiết ".repeat(6_000)),
    ];
    let emissions = std::sync::atomic::AtomicUsize::new(0);
    let budget_error = manager
        .generate_budgeted_completion(&oversized, 0.7, 0.9, |_| {
            emissions.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            true
        })
        .expect_err("oversized mandatory content must be rejected");
    assert!(
        budget_error.contains("Required prompt content exceeds budget"),
        "unexpected error: {budget_error}"
    );
    assert_eq!(
        emissions.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "a budget-rejected request must not emit any token"
    );

    // 3) Client cancellation at the first visible token: partial output is
    //    returned as Ok; the caller layer turns it into an error envelope.
    let keep_going = std::sync::atomic::AtomicBool::new(true);
    let cancelled = manager
        .generate_budgeted_completion(&short_dialogue(), 0.7, 0.9, |piece| {
            if !piece.is_empty() && keep_going.swap(false, std::sync::atomic::Ordering::SeqCst) {
                return false;
            }
            true
        })
        .expect("cancellation returns the partial output, not an error");
    assert!(
        cancelled.completion_tokens >= 1,
        "at least one token was sampled"
    );

    // 4) The very next request after cancellation must still work.
    let recovered = manager
        .generate_budgeted_completion(&short_dialogue(), 0.7, 0.9, |_| true)
        .expect("request after cancellation must succeed");
    assert!(!recovered.text.trim().is_empty());
}
