//! Integration tests for Dynamic Prompt Assembly Subsystem (Feature F8).

use liva_native_core::llm::prompt::dynamic_prompt::{
    DynamicPromptAssembler, PromptAssemblyError, PromptBudget, PromptSlice, SkillDefinition,
    SlicePriority,
};
use liva_native_core::llm::{CatalogTool, ChatMessage};
use serde_json::json;

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
