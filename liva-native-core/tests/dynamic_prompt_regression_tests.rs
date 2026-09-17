use liva_native_core::llm::ChatMessage;
use liva_native_core::llm::prompt::dynamic_prompt::{
    DynamicPromptAssembler, PromptAssemblyError, PromptBudget,
};

fn message(role: &str, content: &str) -> ChatMessage {
    ChatMessage {
        role: role.into(),
        content: content.into(),
    }
}

#[test]
fn latest_user_survives_trailing_tool_and_assistant() {
    for suffix in [vec!["tool"], vec!["assistant"], vec!["tool", "assistant"]] {
        let mut messages = vec![
            message("system", "S"),
            message("user", "old"),
            message("assistant", "old reply"),
            message("user", "latest"),
        ];
        for role in suffix {
            messages.push(message(role, "suffix"));
        }
        // Exactly enough for the system (5) and latest user (6).
        let result =
            DynamicPromptAssembler::budget_chat_messages(&messages, &PromptBudget::new(6, 5, 10))
                .unwrap();
        assert_eq!(result.len(), 2);
        assert_eq!(result[0].content, "S");
        assert_eq!(result[1].content, "latest");
    }
}

#[test]
fn oversized_latest_user_is_an_error_not_system_only_success() {
    let messages = [
        message("system", "S"),
        message("user", &"x".repeat(100)),
        message("tool", "result"),
    ];
    assert!(matches!(
        DynamicPromptAssembler::budget_chat_messages(&messages, &PromptBudget::new(6, 5, 10)),
        Err(PromptAssemblyError::BudgetExceeded)
    ));
}

#[test]
fn explicit_context_counts_reserve_and_rejects_overflow() {
    use liva_native_core::llm::prompt::dynamic_prompt::{
        ContextTokenBudget, PromptSlice, SlicePriority,
    };
    for (ctx, reserve) in [(512, 128), (1024, 512)] {
        let context = ContextTokenBudget::new(ctx, reserve).unwrap();
        let budget = PromptBudget::new(ctx, ctx, reserve);
        let slices = [
            PromptSlice::with_tokens(
                "system",
                SlicePriority::P0_SystemCore,
                0,
                message("system", "S"),
                10,
            ),
            PromptSlice::with_tokens(
                "user",
                SlicePriority::P0_SystemCore,
                1,
                message("user", "Q"),
                context.max_prompt_tokens() - 10,
            ),
            PromptSlice::with_tokens(
                "history",
                SlicePriority::P4_DynamicContext,
                2,
                message("assistant", "H"),
                usize::MAX,
            ),
        ];
        let retained =
            DynamicPromptAssembler::assemble_messages_in_context(&slices, &budget, &context)
                .unwrap();
        assert_eq!(retained.len(), 2);
        let mut oversized = slices.clone();
        oversized[1].estimated_tokens += 1;
        assert_eq!(
            DynamicPromptAssembler::assemble_messages_in_context(&oversized, &budget, &context)
                .unwrap_err(),
            PromptAssemblyError::BudgetExceeded
        );
    }
    assert!(ContextTokenBudget::new(512, 512).is_err());
    assert_eq!(
        DynamicPromptAssembler::assemble_messages(&[], &PromptBudget::new(usize::MAX, 1, 512))
            .unwrap_err(),
        PromptAssemblyError::InvalidConfiguration
    );
}

#[test]
fn adaptive_budgets_never_exceed_available_tokens() {
    for ctx in 0usize..=4096 {
        let dialogue = PromptBudget::for_dialogue(ctx);
        assert!(dialogue.total_budget() <= ctx.saturating_sub(513));
        for reserve in [0, 1, 128, 512, 1024, usize::MAX] {
            let budget = PromptBudget::from_context_window(ctx, reserve);
            assert!(budget.total_budget() <= ctx.saturating_sub(reserve).saturating_sub(1));
        }
    }
}

#[test]
fn narrow_context_chat_keeps_system_and_latest_user() {
    use liva_native_core::llm::prompt::dynamic_prompt::ContextTokenBudget;
    for (ctx, reserve) in [(512, 128), (1024, 512)] {
        let context = ContextTokenBudget::new(ctx, reserve).unwrap();
        let budget = PromptBudget::from_context_window(ctx, reserve);
        let messages = [
            message("system", "S"),
            message("user", &"old".repeat(1000)),
            message("assistant", "old reply"),
            message("user", "latest"),
            message("tool", "result"),
            message("assistant", "suffix"),
        ];
        let result =
            DynamicPromptAssembler::budget_chat_messages_in_context(&messages, &budget, &context)
                .unwrap();
        assert!(result.iter().any(|m| m.content == "S"));
        assert!(result.iter().any(|m| m.content == "latest"));
        let tokens: usize = result.iter().map(|m| m.content.len().div_ceil(4) + 4).sum();
        assert!(tokens + reserve < ctx);
    }
}
