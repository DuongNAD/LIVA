//! Milestone 2 Integration Tests: Compact Prompt Assembler (Feature F11).

use liva_native_core::llm::prompt::ChatMessage;
use liva_native_core::llm::prompt::compact_assembler::{
    AssembledCompactPrompt, CompactPromptAssembler, CompactPromptBudget, MemoryFactEntry,
    SkillIndexEntry, estimate_tokens,
};
use liva_native_core::llm::tool_calling::CatalogTool;
use serde_json::json;

fn create_sample_tools() -> Vec<CatalogTool> {
    vec![
        CatalogTool {
            server: "system".into(),
            name: "read_markdown".into(),
            description: "Read markdown file from vault".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "path": { "type": "string" },
                    "max_lines": { "type": "integer" }
                },
                "required": ["path"]
            }),
            embed_extra: "".into(),
        },
        CatalogTool {
            server: "system".into(),
            name: "search_vault".into(),
            description: "Hybrid vector FTS search across notes".into(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": { "type": "string" }
                },
                "required": ["query"]
            }),
            embed_extra: "".into(),
        },
    ]
}

fn create_sample_skills() -> Vec<SkillIndexEntry> {
    vec![
        SkillIndexEntry {
            skill_id: "obsidian_pkm".into(),
            description: "Curate personal notes and knowledge graphs".into(),
        },
        SkillIndexEntry {
            skill_id: "system_automation".into(),
            description: "Inspect running processes and control volume".into(),
        },
    ]
}

fn create_sample_memories() -> Vec<MemoryFactEntry> {
    vec![
        MemoryFactEntry {
            fact_id: "fact_1".into(),
            subject: "user".into(),
            predicate: "language_preference".into(),
            object_value: "Vietnamese".into(),
            scope: "global".into(),
            confidence: 0.98,
            is_active: true,
        },
        MemoryFactEntry {
            fact_id: "fact_2".into(),
            subject: "user".into(),
            predicate: "work_laptop".into(),
            object_value: "ThinkPad".into(),
            scope: "device".into(),
            confidence: 0.85,
            is_active: true,
        },
        MemoryFactEntry {
            fact_id: "fact_inactive".into(),
            subject: "user".into(),
            predicate: "old_location".into(),
            object_value: "Da Nang".into(),
            scope: "location".into(),
            confidence: 0.99,
            is_active: false, // Inactive fact must be omitted
        },
    ]
}

#[test]
fn test_f11_compact_prompt_budget_boundaries() {
    let budget = CompactPromptBudget::default();
    assert_eq!(budget.max_system_tokens, 1000);
    assert_eq!(budget.max_skill_index_tokens, 300);
    assert_eq!(budget.max_tools_tokens, 1500);
    assert_eq!(budget.max_memory_tokens, 1500);
    assert_eq!(budget.total_static_prefix_cap(), 4300);
    assert_eq!(budget.max_dynamic_history_tokens(), 8192 - 512 - 4300);

    let tools = create_sample_tools();
    let skills = create_sample_skills();
    let memories = create_sample_memories();
    let system_rules = "You are LIVA, a private local AI assistant. Keep responses concise.";
    let turns = vec![ChatMessage {
        role: "user".into(),
        content: "Hôm nay thời tiết thế nào?".into(),
    }];

    let assembled =
        CompactPromptAssembler::assemble(system_rules, &skills, &tools, &memories, &turns, &budget)
            .expect("assemble prompt");

    assert!(assembled.estimated_system_tokens <= budget.max_system_tokens);
    assert!(assembled.estimated_skill_index_tokens <= budget.max_skill_index_tokens);
    assert!(assembled.estimated_tools_tokens <= budget.max_tools_tokens);
    assert!(assembled.estimated_memory_tokens <= budget.max_memory_tokens);
    assert_eq!(
        assembled.prefix_cache_tokens,
        assembled.estimated_system_tokens
            + assembled.estimated_skill_index_tokens
            + assembled.estimated_tools_tokens
    );
}

#[test]
fn test_f11_static_prefix_determinism_and_cache_invariance() {
    let budget = CompactPromptBudget::default();
    let tools = create_sample_tools();
    let skills = create_sample_skills();
    let memories = create_sample_memories();
    let system_rules = "You are LIVA, a private local AI assistant.";

    // Turn 1
    let turns_1 = vec![ChatMessage {
        role: "user".into(),
        content: "Chào bạn!".into(),
    }];
    let assembled_1 = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &memories,
        &turns_1,
        &budget,
    )
    .unwrap();

    // Turn 2: completely different conversation history
    let turns_2 = vec![
        ChatMessage {
            role: "user".into(),
            content: "Chào bạn!".into(),
        },
        ChatMessage {
            role: "assistant".into(),
            content: "Chào bạn, tôi có thể giúp gì cho bạn?".into(),
        },
        ChatMessage {
            role: "user".into(),
            content: "Hãy đọc ghi chú dự án trong vault.".into(),
        },
    ];
    let assembled_2 = CompactPromptAssembler::assemble(
        system_rules,
        &skills,
        &tools,
        &memories,
        &turns_2,
        &budget,
    )
    .unwrap();

    // The leading system turn content (static rules + skill index + tools + memory)
    // MUST BE 100% IDENTICAL! This guarantees llama-server 100% prefix cache hits.
    assert_eq!(
        assembled_1.system_turn_content, assembled_2.system_turn_content,
        "Leading system prompt must be 100% invariant across conversation turns"
    );
    assert_eq!(
        assembled_1.prefix_cache_tokens, assembled_2.prefix_cache_tokens,
        "Prefix cache token length must remain strictly identical"
    );
}

#[test]
fn test_f11_bitemporal_memory_sorting_and_filtering() {
    let budget = CompactPromptBudget::default();
    let tools = create_sample_tools();
    let skills = create_sample_skills();
    let memories = create_sample_memories();
    let system_rules = "You are LIVA.";
    let turns = vec![ChatMessage {
        role: "user".into(),
        content: "Test".into(),
    }];

    let assembled =
        CompactPromptAssembler::assemble(system_rules, &skills, &tools, &memories, &turns, &budget)
            .unwrap();

    let sys = &assembled.system_turn_content;
    assert!(sys.contains("[language_preference] user -> Vietnamese"));
    assert!(sys.contains("[work_laptop] user -> ThinkPad"));
    assert!(
        !sys.contains("old_location"),
        "Inactive memory facts must be strictly excluded"
    );

    // Higher confidence fact (0.98) must appear before lower confidence fact (0.85)
    let pos_lang = sys.find("language_preference").unwrap();
    let pos_laptop = sys.find("work_laptop").unwrap();
    assert!(
        pos_lang < pos_laptop,
        "Memories must be ordered by confidence descending"
    );
}

#[test]
fn test_f11_dense_tool_line_formatting() {
    let tool = CatalogTool {
        server: "system".into(),
        name: "test_tool".into(),
        description: "A test tool description".into(),
        input_schema: json!({
            "type": "object",
            "properties": {
                "file_path": { "type": "string" },
                "verbose": { "type": "boolean" }
            },
            "required": ["file_path"]
        }),
        embed_extra: "".into(),
    };

    let line = CompactPromptAssembler::format_dense_tool_line(&tool);
    assert!(line.contains("- test_tool: A test tool description"));
    assert!(
        line.contains("file_path*:string"),
        "Required parameters must be marked with *"
    );
    assert!(
        line.contains("verbose:boolean"),
        "Optional parameters must NOT have *"
    );
}
