use liva_native_core::llm::ChatMessage;
use liva_native_core::llm::engine::LlamaRouterManager;

#[test]
fn budgeted_completion_without_model_never_emits_tokens() {
    let mut manager = LlamaRouterManager::new(4096, 0).unwrap();
    let messages = vec![ChatMessage {
        role: "user".into(),
        content: "private request".into(),
    }];
    let mut callbacks = 0;
    let result = manager.generate_budgeted_completion(&messages, 0.7, 0.9, |_| {
        callbacks += 1;
        true
    });
    let error = result.unwrap_err();
    assert!(!error.contains("private request"));
    assert_eq!(callbacks, 0);
    assert!(manager.last_tokens.is_empty());
}

#[test]
fn budgeted_completion_vocab_only_preserves_existing_contract() {
    let mut manager = LlamaRouterManager::new(4096, 0).unwrap();
    manager.vocab_only = true;
    let result = manager.generate_budgeted_completion(&[], 0.7, 0.9, |_| {
        panic!("vocab-only generation must not emit tokens")
    });
    assert_eq!(
        result.unwrap_err(),
        "Cannot generate completions on a vocab-only model"
    );
    assert!(manager.last_tokens.is_empty());
}
