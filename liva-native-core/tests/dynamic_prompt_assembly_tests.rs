//! Integration tests for Dynamic Prompt Assembly Subsystem (Feature F8).

use liva_native_core::llm::prompt::dynamic_prompt::ContextTokenBudget;
use liva_native_core::llm::prompt::dynamic_prompt::{
    DynamicPromptAssembler, PromptAssemblyError, PromptBudget, PromptSlice, SkillDefinition,
    SlicePriority,
};
use liva_native_core::llm::{CatalogTool, ChatMessage};
use serde_json::json;

#[test]
fn context_budget_minimum_and_invalid_configuration() {
    for (ctx, reserve) in [(0, 0), (4096, 0), (0, 512), (512, 512), (513, 512)] {
        assert_eq!(
            ContextTokenBudget::new(ctx, reserve),
            Err(PromptAssemblyError::InvalidConfiguration)
        );
    }
    let budget = ContextTokenBudget::new(514, 512).unwrap();
    assert_eq!(budget.max_prompt_tokens(), 1);
    assert!(budget.can_fit(0));
    assert!(budget.can_fit(1));
    assert!(!budget.can_fit(2));
}

#[test]
fn context_budget_strict_boundary() {
    let budget = ContextTokenBudget::new(4096, 512).unwrap();
    assert_eq!(budget.max_prompt_tokens(), 3583);
    assert!(budget.can_fit(3583));
    assert!(!budget.can_fit(3584));
}

#[test]
fn context_budget_overflow_boundaries() {
    let budget = ContextTokenBudget::new(usize::MAX, 512).unwrap();
    assert!(budget.can_fit(usize::MAX - 513));
    assert!(!budget.can_fit(usize::MAX - 512));
    assert!(!budget.can_fit(usize::MAX));
    assert!(ContextTokenBudget::new(usize::MAX, usize::MAX).is_err());
    assert!(ContextTokenBudget::new(usize::MAX, usize::MAX - 1).is_err());
    assert!(ContextTokenBudget::new(1, usize::MAX).is_err());
}

#[test]
fn context_budget_guard_preserves_legacy_arithmetic_for_all_boundaries() {
    use liva_native_core::llm::engine::{RESERVE_FOR_COMPLETION, check_prompt_fits};
    let counts = [
        0,
        1,
        16,
        511,
        512,
        513,
        514,
        3583,
        3584,
        4096,
        usize::MAX - 513,
        usize::MAX - 512,
        usize::MAX,
    ];
    for ctx in counts {
        for prompt in counts {
            let expected = prompt.saturating_add(RESERVE_FOR_COMPLETION) < ctx;
            assert_eq!(
                check_prompt_fits(prompt, ctx).is_ok(),
                expected,
                "prompt={prompt}, ctx={ctx}"
            );
        }
    }
}

#[test]
fn estimated_budget_constructors_never_exceed_available_context_or_overflow() {
    for ctx in [0, 1, 128, 512, 513, 514, 544, 545, 1024, 4096, usize::MAX] {
        let dialogue = PromptBudget::for_dialogue(ctx);
        assert!(dialogue.total_budget() <= ctx.saturating_sub(513));
        if ctx <= 513 {
            assert_eq!(dialogue.total_budget(), 0);
        }
        for reserve in [0, 1, 512, usize::MAX - 1, usize::MAX] {
            let estimated = PromptBudget::from_context_window(ctx, reserve);
            match ContextTokenBudget::new(ctx, reserve) {
                Ok(exact) => assert!(estimated.total_budget() <= exact.max_prompt_tokens()),
                Err(_) => assert_eq!(estimated.total_budget(), 0),
            }
        }
    }
    // Preserve the existing normal-context allocation; only floors/overflow change.
    assert_eq!(
        PromptBudget::for_dialogue(4096).total_budget(),
        4096 - 512 - 32
    );
}

// These callbacks deliberately model token costs; they are NOT a real tokenizer
// or an inference/transport E2E test. Runtime model fixtures belong to later steps.
mod exact_budget {
    use super::*;
    use liva_native_core::llm::prompt::dynamic_prompt::{BudgetedPrompt, PromptSelection};
    use std::cell::Cell;

    fn message(role: &str, content: &str) -> ChatMessage {
        ChatMessage {
            role: role.into(),
            content: content.into(),
        }
    }

    fn compile(messages: &[ChatMessage]) -> Result<String, String> {
        Ok(messages
            .iter()
            .map(|m| m.content.as_str())
            .collect::<Vec<_>>()
            .join("|"))
    }

    fn assemble(
        slices: &[PromptSlice],
        selection: &PromptSelection,
        limit: usize,
    ) -> Result<BudgetedPrompt, PromptAssemblyError> {
        DynamicPromptAssembler::compile_exact_budgeted_prompt(
            slices,
            selection,
            &ContextTokenBudget::new(limit + 2, 1).unwrap(),
            compile,
            |prompt| Ok(prompt.chars().count()),
        )
    }

    #[test]
    fn validation_rejects_duplicate_unknown_empty_and_overlapping_ids_before_callbacks() {
        let valid = vec![
            slice("a", SlicePriority::P0_SystemCore, 0),
            slice("b", SlicePriority::P4_DynamicContext, 1),
        ];
        let cases = vec![
            PromptSelection {
                mandatory_ids: vec!["missing".into()],
                atomic_groups: vec![],
            },
            PromptSelection {
                mandatory_ids: vec!["a".into(), "a".into()],
                atomic_groups: vec![],
            },
            PromptSelection {
                mandatory_ids: vec![],
                atomic_groups: vec![vec![]],
            },
            PromptSelection {
                mandatory_ids: vec![],
                atomic_groups: vec![vec!["missing".into()]],
            },
            PromptSelection {
                mandatory_ids: vec![],
                atomic_groups: vec![vec!["a".into(), "a".into()]],
            },
            PromptSelection {
                mandatory_ids: vec![],
                atomic_groups: vec![vec!["a".into()], vec!["a".into(), "b".into()]],
            },
        ];
        for selection in cases {
            let result = DynamicPromptAssembler::compile_exact_budgeted_prompt(
                &valid,
                &selection,
                &ContextTokenBudget::new(100, 1).unwrap(),
                |_| panic!("invalid selection reached compiler"),
                |_| panic!("invalid selection reached tokenizer"),
            );
            assert!(matches!(
                result,
                Err(PromptAssemblyError::InvalidSelection(_))
            ));
        }
        for bad in [
            vec![valid[0].clone(), valid[0].clone()],
            vec![slice("", SlicePriority::P0_SystemCore, 0)],
        ] {
            assert!(matches!(
                assemble(&bad, &PromptSelection::default(), 20),
                Err(PromptAssemblyError::InvalidSelection(_))
            ));
        }
    }

    #[test]
    fn mandatory_and_p0_members_protect_the_whole_group() {
        let slices = vec![
            slice("a", SlicePriority::P4_DynamicContext, 0),
            slice("b", SlicePriority::P4_DynamicContext, 1),
        ];
        let selection = PromptSelection {
            mandatory_ids: vec!["a".into()],
            atomic_groups: vec![vec!["a".into(), "b".into()]],
        };
        assert!(matches!(
            assemble(&slices, &selection, 1),
            Err(PromptAssemblyError::RequiredContentTooLarge {
                required_tokens: 3,
                ..
            })
        ));
        let mut slices = slices;
        slices[0].priority = SlicePriority::P0_SystemCore;
        let selection = PromptSelection {
            mandatory_ids: vec![],
            ..selection
        };
        assert!(matches!(
            assemble(&slices, &selection, 1),
            Err(PromptAssemblyError::RequiredContentTooLarge {
                required_tokens: 3,
                ..
            })
        ));
    }

    #[test]
    fn mixed_priority_groups_and_chronological_ties_are_deterministic() {
        let slices = vec![
            slice("c", SlicePriority::P0_SystemCore, 0),
            slice("a", SlicePriority::P1_BaseCapabilities, 1),
            slice("b", SlicePriority::P4_DynamicContext, 2),
            slice("d", SlicePriority::P3_DomainSkills, 3),
            slice("e", SlicePriority::P4_DynamicContext, 4),
            slice("f", SlicePriority::P4_DynamicContext, 5),
        ];
        let selection = PromptSelection {
            mandatory_ids: vec![],
            atomic_groups: vec![vec!["a".into(), "b".into()]],
        };
        let out = assemble(&slices, &selection, 7).unwrap();
        assert_eq!(out.prompt, "c|a|b|d");
        assert_eq!(out.dropped_slice_ids, ["e", "f"]);
        let out = assemble(&slices, &selection, 5).unwrap();
        assert_eq!(out.prompt, "c|a|b");
        assert_eq!(out.dropped_slice_ids, ["d", "e", "f"]);
        let tied = vec![
            slice("x", SlicePriority::P4_DynamicContext, 2),
            slice("y", SlicePriority::P4_DynamicContext, 2),
            slice("c", SlicePriority::P0_SystemCore, 0),
        ];
        assert_eq!(
            assemble(&tied, &PromptSelection::default(), 3)
                .unwrap()
                .prompt,
            "c|y"
        );
    }

    #[test]
    fn nonmonotonic_measurement_is_repeated_and_bounded() {
        let slices = vec![
            slice("c", SlicePriority::P0_SystemCore, 0),
            slice("a", SlicePriority::P4_DynamicContext, 1),
            slice("b", SlicePriority::P4_DynamicContext, 2),
        ];
        for costs in [[100, 200, 1], [100, 200, usize::MAX]] {
            let mut calls = 0;
            let result = DynamicPromptAssembler::compile_exact_budgeted_prompt(
                &slices,
                &PromptSelection::default(),
                &ContextTokenBudget::new(10, 1).unwrap(),
                compile,
                |_| {
                    let n = costs[calls];
                    calls += 1;
                    Ok(n)
                },
            );
            assert_eq!(calls, 3);
            if costs[2] == 1 {
                let out = result.unwrap();
                assert_eq!(out.prompt, "c");
                assert_eq!(out.prompt_tokens, 1);
            } else {
                assert!(matches!(
                    result,
                    Err(PromptAssemblyError::RequiredContentTooLarge {
                        required_tokens: usize::MAX,
                        ..
                    })
                ));
            }
        }
    }

    #[test]
    fn callback_errors_are_sanitized_and_empty_prompts_never_reach_measurement() {
        let slices = vec![slice("a", SlicePriority::P0_SystemCore, 0)];
        let selection = PromptSelection::default();
        let budget = ContextTokenBudget::new(10, 1).unwrap();
        let err = DynamicPromptAssembler::compile_exact_budgeted_prompt(
            &slices,
            &selection,
            &budget,
            |_| Err("SECRET prompt/path".into()),
            |_| panic!("compiler failed"),
        )
        .unwrap_err();
        assert!(matches!(err, PromptAssemblyError::SlicingError(_)));
        assert!(!err.to_string().contains("SECRET"));
        let err = DynamicPromptAssembler::compile_exact_budgeted_prompt(
            &slices,
            &selection,
            &budget,
            compile,
            |_| Err("SECRET prompt/path".into()),
        )
        .unwrap_err();
        assert!(matches!(err, PromptAssemblyError::TokenizationError(_)));
        assert!(!err.to_string().contains("SECRET"));
        let result = DynamicPromptAssembler::compile_exact_budgeted_prompt(
            &slices,
            &selection,
            &budget,
            |_| Ok(String::new()),
            |_| panic!("empty prompt"),
        );
        assert!(matches!(result, Err(PromptAssemblyError::EmptyPrompt)));
        assert!(matches!(
            assemble(&[], &selection, 10),
            Err(PromptAssemblyError::EmptyPrompt)
        ));
        let optional = vec![slice("long", SlicePriority::P4_DynamicContext, 0)];
        assert!(matches!(
            assemble(&optional, &selection, 1),
            Err(PromptAssemblyError::EmptyPrompt)
        ));
    }

    #[test]
    fn priority_order_covers_every_optional_tier() {
        let slices = vec![
            slice("c", SlicePriority::P0_SystemCore, 0),
            slice("a", SlicePriority::P1_BaseCapabilities, 1),
            slice("b", SlicePriority::P2_ActiveTools, 2),
            slice("d", SlicePriority::P3_DomainSkills, 3),
            slice("e", SlicePriority::P4_DynamicContext, 4),
        ];
        for (limit, expected) in [(7, "c|a|b|d"), (5, "c|a|b"), (3, "c|a"), (1, "c")] {
            assert_eq!(
                assemble(&slices, &PromptSelection::default(), limit)
                    .unwrap()
                    .prompt,
                expected
            );
        }
    }

    fn slice(id: &str, priority: SlicePriority, seq: usize) -> PromptSlice {
        PromptSlice::with_tokens(id, priority, seq, message("user", id), usize::MAX)
    }

    #[test]
    fn selector_preserves_systems_current_user_suffix_and_atomic_history() {
        let messages = vec![
            message("system", "S"),
            message("user", "old"),
            message("assistant", "call"),
            message("system", "T"),
            message("tool", "result"),
            message("user", "Q"),
            message("assistant", "A"),
            message("tool", "R"),
        ];
        let (slices, selection) = DynamicPromptAssembler::select_chat_messages(&messages).unwrap();
        assert_eq!(
            selection.mandatory_ids,
            ["msg_0", "msg_3", "msg_5", "msg_6", "msg_7"]
        );
        assert_eq!(
            selection.atomic_groups,
            vec![
                vec!["msg_1", "msg_2", "msg_4"],
                vec!["msg_5", "msg_6", "msg_7"]
            ]
        );
        let out = assemble(&slices, &selection, 9).unwrap();
        assert_eq!(out.prompt, "S|T|Q|A|R");
        assert_eq!(out.prompt_tokens, 9);
        assert_eq!(out.dropped_slice_ids, ["msg_1", "msg_2", "msg_4"]);
    }

    #[test]
    fn selector_rejects_missing_user_and_orphan_prefix() {
        for messages in [vec![], vec![message("system", "S")]] {
            assert!(matches!(
                DynamicPromptAssembler::select_chat_messages(&messages),
                Err(PromptAssemblyError::EmptyPrompt)
            ));
        }
        for role in ["assistant", "tool"] {
            assert!(matches!(
                DynamicPromptAssembler::select_chat_messages(&[
                    message(role, "orphan"),
                    message("user", "Q")
                ]),
                Err(PromptAssemblyError::InvalidSelection(_))
            ));
        }
    }

    #[test]
    fn oversized_current_question_fails_instead_of_becoming_system_only() {
        let (slices, selection) = DynamicPromptAssembler::select_chat_messages(&[
            message("system", "S"),
            message("user", "too long"),
        ])
        .unwrap();
        assert!(matches!(
            assemble(&slices, &selection, 2),
            Err(PromptAssemblyError::RequiredContentTooLarge {
                required_tokens: 10,
                max_prompt_tokens: 2
            })
        ));
    }

    #[test]
    fn exact_measurement_includes_unicode_and_template_overhead_not_estimates() {
        let (mut slices, selection) = DynamicPromptAssembler::select_chat_messages(&[
            message("system", "S"),
            message("user", "cũ"),
            message("assistant", "🙂"),
            message("user", "mới"),
        ])
        .unwrap();
        for slice in &mut slices {
            slice.estimated_tokens = 0;
        }
        let calls = Cell::new(0);
        let out = DynamicPromptAssembler::compile_exact_budgeted_prompt(
            &slices,
            &selection,
            &ContextTokenBudget::new(10, 1).unwrap(),
            |messages| Ok(format!("<{}>", compile(messages)?)),
            |prompt| {
                calls.set(calls.get() + 1);
                Ok(prompt.chars().count())
            },
        )
        .unwrap();
        assert_eq!(out.prompt, "<S|mới>");
        assert_eq!(out.prompt_tokens, 7);
        assert_eq!(calls.get(), 2);
        assert_eq!(out.dropped_slice_ids, ["msg_1", "msg_2"]);
    }
}

// ---------------------------------------------------------------------------
// TIER 1: FEATURE COVERAGE (F8)
// ---------------------------------------------------------------------------

#[test]
fn test_system_prompt_compilation() {
    let budget = PromptBudget {
        max_system_tokens: 500,
        max_tool_tokens: 200,
        reserve_response_tokens: 100,
    };

    let base = "You are LIVA AI Assistant.";
    let skills = vec![SkillDefinition {
        skill_id: "s1".into(),
        name: "smarthome".into(),
        description: "Control smart lights".into(),
        instructions: "Turn on/off IoT devices".into(),
        priority: 0.9,
        estimated_tokens: 20,
    }];

    let tools = vec![CatalogTool {
        server: "native".into(),
        name: "control_smarthome".into(),
        description: "Control home devices".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "device": { "type": "string" },
                "action": { "type": "string" }
            },
            "required": ["device", "action"]
        }),
        embed_extra: "".into(),
    }];

    let assembled =
        DynamicPromptAssembler::assemble_prompt(base, &skills, &tools, &budget).expect("assembled");
    assert!(assembled.contains("You are LIVA AI Assistant."));
    assert!(assembled.contains("ACTIVE SKILLS"));
    assert!(assembled.contains("smarthome"));
    assert!(assembled.contains("control_smarthome"));
    assert!(assembled.contains("device*"));
}

#[test]
fn test_token_budget_bound_enforcement() {
    let budget = PromptBudget {
        max_system_tokens: 60,
        max_tool_tokens: 50,
        reserve_response_tokens: 20,
    };

    let base = "Base persona."; // ~4 tokens
    let mut skills = Vec::new();
    for i in 0..10 {
        skills.push(SkillDefinition {
            skill_id: format!("s_{i}"),
            name: format!("skill_{i}"),
            description: "desc".into(),
            instructions: "Do action step".into(),
            priority: 0.5,
            estimated_tokens: 20,
        });
    }

    let assembled =
        DynamicPromptAssembler::assemble_prompt(base, &skills, &[], &budget).expect("assembled");
    let estimated_output_tokens = assembled.len().div_ceil(4);
    assert!(
        estimated_output_tokens <= budget.max_system_tokens + 15,
        "Estimated tokens {estimated_output_tokens} must be within budget bounds"
    );
}

#[test]
fn test_priority_skill_pruning() {
    let budget = PromptBudget {
        max_system_tokens: 40,
        max_tool_tokens: 50,
        reserve_response_tokens: 10,
    };

    let base = "Base rule."; // ~3 tokens
    let skills = vec![
        SkillDefinition {
            skill_id: "s_low".into(),
            name: "low_priority".into(),
            description: "Low".into(),
            instructions: "Do secondary tasks".into(),
            priority: 0.1,
            estimated_tokens: 25,
        },
        SkillDefinition {
            skill_id: "s_high".into(),
            name: "high_priority".into(),
            description: "High".into(),
            instructions: "Critical safety rules".into(),
            priority: 0.99,
            estimated_tokens: 25,
        },
    ];

    let assembled =
        DynamicPromptAssembler::assemble_prompt(base, &skills, &[], &budget).expect("assembled");
    assert!(
        assembled.contains("high_priority"),
        "High priority skill must be included"
    );
    assert!(
        !assembled.contains("low_priority"),
        "Low priority skill must be pruned due to budget"
    );
}

#[test]
fn test_compact_tool_schema_formatting() {
    let tool = CatalogTool {
        server: "native".into(),
        name: "search_vault".into(),
        description: "Search notes in Obsidian vault".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "query": { "type": "string" },
                "top_k": { "type": "integer" }
            },
            "required": ["query"]
        }),
        embed_extra: "".into(),
    };

    let formatted = DynamicPromptAssembler::format_compact_tool_schema(&tool);
    assert!(formatted.contains("search_vault: Search notes in Obsidian vault"));
    assert!(formatted.contains("query*"));
    assert!(formatted.contains("top_k"));
}

#[test]
fn test_prompt_assembly_budget_error() {
    let budget = PromptBudget {
        max_system_tokens: 5,
        max_tool_tokens: 50,
        reserve_response_tokens: 10,
    };

    let massive_base =
        "This base prompt is excessively large and clearly exceeds the five token ceiling."
            .repeat(10);
    let res = DynamicPromptAssembler::assemble_prompt(&massive_base, &[], &[], &budget);
    assert_eq!(res.unwrap_err(), PromptAssemblyError::BudgetExceeded);
}

// ---------------------------------------------------------------------------
// TIER 2: BOUNDARY CASES & CHRONOLOGICAL INVARIANTS (F8)
// ---------------------------------------------------------------------------

#[test]
fn test_zero_token_budget() {
    let budget = PromptBudget {
        max_system_tokens: 0,
        max_tool_tokens: 50,
        reserve_response_tokens: 10,
    };

    let res = DynamicPromptAssembler::assemble_prompt("Base", &[], &[], &budget);
    assert_eq!(res.unwrap_err(), PromptAssemblyError::InvalidConfiguration);
}

#[test]
fn test_budget_exceeded_base_prompt() {
    let budget = PromptBudget {
        max_system_tokens: 5,
        max_tool_tokens: 50,
        reserve_response_tokens: 10,
    };

    let massive_base = "A".repeat(500);
    let res = DynamicPromptAssembler::assemble_prompt(&massive_base, &[], &[], &budget);
    assert_eq!(res.unwrap_err(), PromptAssemblyError::BudgetExceeded);
}

#[test]
fn test_chronological_conversation_history_preservation_under_eviction() {
    // Total budget = 120 tokens
    let budget = PromptBudget {
        max_system_tokens: 60,
        max_tool_tokens: 60,
        reserve_response_tokens: 20,
    };

    // Construct multi-turn conversation slices:
    // P0: System Core (sequence 0, 15 tokens)
    // P1: Tool Schemas (sequence 1, 20 tokens)
    // P4: History Turn 1 (sequence 2, 30 tokens) [OLDEST -> should be evicted first]
    // P4: History Turn 2 (sequence 3, 30 tokens) [OLDER -> should be evicted second]
    // P4: History Turn 3 (sequence 4, 30 tokens) [RECENT -> kept]
    // P2: Current User Turn (sequence 5, 20 tokens) [IMMEDIATE -> kept]
    let slices = vec![
        PromptSlice::with_tokens(
            "p0_core",
            SlicePriority::P0_SystemCore,
            0,
            ChatMessage {
                role: "system".into(),
                content: "You are LIVA core system.".into(),
            },
            15,
        ),
        PromptSlice::with_tokens(
            "p1_tools",
            SlicePriority::P1_BaseCapabilities,
            1,
            ChatMessage {
                role: "system".into(),
                content: "Tools: [search_vault]".into(),
            },
            20,
        ),
        PromptSlice::with_tokens(
            "p4_turn1",
            SlicePriority::P4_DynamicContext,
            2,
            ChatMessage {
                role: "user".into(),
                content: "Turn 1 question: Who are you?".into(),
            },
            30,
        ),
        PromptSlice::with_tokens(
            "p4_turn2",
            SlicePriority::P4_DynamicContext,
            3,
            ChatMessage {
                role: "assistant".into(),
                content: "Turn 2 answer: I am LIVA.".into(),
            },
            30,
        ),
        PromptSlice::with_tokens(
            "p4_turn3",
            SlicePriority::P4_DynamicContext,
            4,
            ChatMessage {
                role: "user".into(),
                content: "Turn 3 question: What can you do?".into(),
            },
            30,
        ),
        PromptSlice::with_tokens(
            "p2_current",
            SlicePriority::P2_ActiveTools,
            5,
            ChatMessage {
                role: "user".into(),
                content: "Turn 4 latest question: Search my notes for meetings.".into(),
            },
            20,
        ),
    ];

    // Total tokens of all slices = 15 + 20 + 30 + 30 + 30 + 20 = 145 tokens (exceeds 120 budget)
    // Eviction order should prune Turn 1 (30 tokens), leaving 145 - 30 = 115 tokens (fits within 120)
    let assembled_messages =
        DynamicPromptAssembler::assemble_messages(&slices, &budget).expect("assembled");

    // Verify message contents
    let contents: Vec<&str> = assembled_messages
        .iter()
        .map(|m| m.content.as_str())
        .collect();

    // Turn 1 should have been evicted
    assert!(
        !contents.contains(&"Turn 1 question: Who are you?"),
        "Oldest turn must be pruned"
    );

    // Remaining messages MUST be in strict chronological sequence (0 -> 1 -> 3 -> 4 -> 5)
    assert_eq!(contents[0], "You are LIVA core system.");
    assert_eq!(contents[1], "Tools: [search_vault]");
    assert_eq!(contents[2], "Turn 2 answer: I am LIVA.");
    assert_eq!(contents[3], "Turn 3 question: What can you do?");
    assert_eq!(
        contents[4],
        "Turn 4 latest question: Search my notes for meetings."
    );
}

#[test]
fn test_multi_slice_priority_eviction_order() {
    let budget = PromptBudget {
        max_system_tokens: 50,
        max_tool_tokens: 50,
        reserve_response_tokens: 10,
    };

    let slices = vec![
        PromptSlice::with_tokens(
            "p0",
            SlicePriority::P0_SystemCore,
            0,
            ChatMessage {
                role: "system".into(),
                content: "Core Persona".into(),
            },
            40,
        ),
        PromptSlice::with_tokens(
            "p3_memory",
            SlicePriority::P3_DomainSkills,
            1,
            ChatMessage {
                role: "system".into(),
                content: "Recalled memory".into(),
            },
            40,
        ),
        PromptSlice::with_tokens(
            "p4_history",
            SlicePriority::P4_DynamicContext,
            2,
            ChatMessage {
                role: "user".into(),
                content: "Old chat".into(),
            },
            40,
        ),
    ];

    // Total = 120 tokens, budget = 100 tokens.
    // P4 (40 tokens) should be dropped first -> remaining P0 (40) + P3 (40) = 80 tokens (fits in 100).
    let messages = DynamicPromptAssembler::assemble_messages(&slices, &budget).expect("assembled");
    let contents: Vec<&str> = messages.iter().map(|m| m.content.as_str()).collect();

    assert_eq!(contents.len(), 2);
    assert_eq!(contents[0], "Core Persona");
    assert_eq!(contents[1], "Recalled memory");
}

#[test]
fn test_compile_budgeted_prompt_end_to_end() {
    let budget = PromptBudget {
        max_system_tokens: 200,
        max_tool_tokens: 200,
        reserve_response_tokens: 50,
    };

    let slices = vec![
        PromptSlice::new(
            "sys",
            SlicePriority::P0_SystemCore,
            0,
            ChatMessage {
                role: "system".into(),
                content: "You are LIVA.".into(),
            },
        ),
        PromptSlice::new(
            "user",
            SlicePriority::P2_ActiveTools,
            1,
            ChatMessage {
                role: "user".into(),
                content: "Hello!".into(),
            },
        ),
    ];

    let compiled =
        DynamicPromptAssembler::compile_budgeted_prompt(&slices, &budget).expect("compiled");
    assert!(compiled.contains("You are LIVA."));
    assert!(compiled.contains("Hello!"));
}

#[test]
fn test_budget_chat_messages_preserves_order_and_bounds() {
    let budget = PromptBudget {
        max_system_tokens: 100,
        max_tool_tokens: 80,
        reserve_response_tokens: 20,
    };

    let mut messages = vec![
        ChatMessage {
            role: "system".into(),
            content: "You are LIVA AI Assistant.".into(),
        },
        ChatMessage {
            role: "system".into(),
            content: "Recalled Memory: User likes Rust programming.".into(),
        },
    ];

    // Add 10 conversation turns
    for i in 1..=10 {
        messages.push(ChatMessage {
            role: "user".into(),
            content: format!("User turn message number {i}"),
        });
        messages.push(ChatMessage {
            role: "assistant".into(),
            content: format!("Assistant response number {i}"),
        });
    }

    // Latest user question
    messages.push(ChatMessage {
        role: "user".into(),
        content: "What is my favorite programming language?".into(),
    });

    let budgeted = DynamicPromptAssembler::budget_chat_messages(&messages, &budget)
        .expect("budgeted messages");

    // System core must be preserved
    assert_eq!(budgeted[0].role, "system");
    assert!(budgeted[0].content.contains("You are LIVA AI Assistant."));

    // Latest user prompt must be preserved
    let last = budgeted.last().unwrap();
    assert_eq!(last.role, "user");
    assert!(last.content.contains("favorite programming language"));

    // Chronological order must be maintained for any remaining history
    let mut last_turn = 0;
    for msg in &budgeted {
        if let Some(pos) = msg.content.find("message number ") {
            let num: usize = msg.content[pos + "message number ".len()..]
                .parse()
                .unwrap_or(0);
            assert!(num >= last_turn, "Turns must appear in chronological order");
            last_turn = num;
        }
    }
}
