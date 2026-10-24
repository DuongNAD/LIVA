//! Compact Prompt Assembler with Fixed Token Budgets and Static-to-Dynamic Prefix Caching (Feature F11).
//!
//! Enforces mathematical token boundaries:
//! - Static System Rules: <= 1000 tokens
//! - Skill Index: <= 300 tokens
//! - Dynamic Tools: <= 1500 tokens
//! - Bitemporal Memory: <= 1500 tokens
//!
//! Layout sequence strictly maximizes llama.cpp / llama-server KV prefix cache hit rate:
//! [Static System Rules] -> [Skill Index] -> [Tool Definitions] -> [Bitemporal Memory] -> [Dynamic Conversation Turns]
//!
//! The first three blocks are 100% stable across conversation turns within the same session/toolset,
//! allowing Slot 0 to instantly reuse prefilled KV cache (TTFT drops to <45ms).

use crate::authorization::{DynamicToolPruner, Principal};
use crate::llm::prompt::ChatMessage;
use crate::llm::prompt::persona::sanitize_untrusted;
use crate::llm::tool_calling::CatalogTool;
use serde::{Deserialize, Serialize};

/// Fixed token budget boundaries for compact prompt assembly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompactPromptBudget {
    /// Upper bound for static persona and safety constraints (default: 1000).
    pub max_system_tokens: usize,
    /// Upper bound for compact skill index (default: 300).
    pub max_skill_index_tokens: usize,
    /// Upper bound for active tool schemas (default: 1500).
    pub max_tools_tokens: usize,
    /// Upper bound for recalled bitemporal memory facts (default: 1500).
    pub max_memory_tokens: usize,
    /// Context window size (e.g. 4096 or 8192).
    pub n_ctx: usize,
    /// Generation reserve tokens (default: 512).
    pub reserve_response_tokens: usize,
}

impl Default for CompactPromptBudget {
    fn default() -> Self {
        Self {
            max_system_tokens: 1000,
            max_skill_index_tokens: 300,
            max_tools_tokens: 1500,
            max_memory_tokens: 1500,
            n_ctx: 8192,
            reserve_response_tokens: 512,
        }
    }
}

impl CompactPromptBudget {
    pub fn for_dialogue_4k() -> Self {
        Self {
            max_system_tokens: 1000,
            max_skill_index_tokens: 300,
            max_tools_tokens: 1200,
            max_memory_tokens: 1000,
            n_ctx: 4096,
            reserve_response_tokens: 512,
        }
    }

    pub fn total_static_prefix_cap(&self) -> usize {
        self.max_system_tokens
            .saturating_add(self.max_skill_index_tokens)
            .saturating_add(self.max_tools_tokens)
            .saturating_add(self.max_memory_tokens)
    }

    pub fn max_dynamic_history_tokens(&self) -> usize {
        self.n_ctx
            .saturating_sub(self.reserve_response_tokens)
            .saturating_sub(self.total_static_prefix_cap())
    }
}

/// Compact entry representing a registered skill in the skill index.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SkillIndexEntry {
    pub skill_id: String,
    pub description: String,
}

/// Compact entry representing a bitemporal recalled memory fact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MemoryFactEntry {
    pub fact_id: String,
    pub subject: String,
    pub predicate: String,
    pub object_value: String,
    pub scope: String,
    pub confidence: f32,
    pub is_active: bool,
}

/// Token estimation helper (1 token ≈ 4 characters for alphanumeric & whitespace).
#[inline]
pub fn estimate_tokens(text: &str) -> usize {
    if text.is_empty() {
        0
    } else {
        text.chars().count().div_ceil(4)
    }
}

/// Output breakdown of the compact assembled prompt.
#[derive(Debug, Clone)]
pub struct AssembledCompactPrompt {
    pub system_turn_content: String,
    pub full_messages: Vec<ChatMessage>,
    pub estimated_system_tokens: usize,
    pub estimated_skill_index_tokens: usize,
    pub estimated_tools_tokens: usize,
    pub estimated_memory_tokens: usize,
    pub estimated_history_tokens: usize,
    pub prefix_cache_tokens: usize,
    pub dropped_history_turns: usize,
}

/// Assembler engine implementing static-to-dynamic prompt layout.
pub struct CompactPromptAssembler;

impl CompactPromptAssembler {
    /// Formats a catalog tool into a dense 1-line schema.
    pub fn format_dense_tool_line(tool: &CatalogTool) -> String {
        let mut params = Vec::new();
        if let Some(props) = tool.input_schema.get("properties").and_then(|p| p.as_object()) {
            let required_list: Vec<&str> = tool
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();

            for (key, spec) in props {
                let is_req = required_list.contains(&key.as_str());
                let marker = if is_req { "*" } else { "" };
                let type_str = spec.get("type").and_then(|t| t.as_str()).unwrap_or("any");
                params.push(format!("{key}{marker}:{type_str}"));
            }
        }
        if params.is_empty() {
            format!("- {}: {}\n", tool.name, tool.description)
        } else {
            format!("- {}: {} ({})\n", tool.name, tool.description, params.join(", "))
        }
    }

    /// Assembles the complete prompt strictly following the static-to-dynamic layout.
    pub fn assemble(
        base_system_rules: &str,
        skills: &[SkillIndexEntry],
        tools: &[CatalogTool],
        memories: &[MemoryFactEntry],
        conversation_turns: &[ChatMessage],
        budget: &CompactPromptBudget,
    ) -> Result<AssembledCompactPrompt, String> {
        Self::assemble_with_principal(
            base_system_rules,
            skills,
            tools,
            memories,
            conversation_turns,
            budget,
            &Principal::Owner,
        )
    }

    /// Assembles the prompt while pruning Owner-only tools if principal is not Owner (Feature F21).
    pub fn assemble_with_principal(
        base_system_rules: &str,
        skills: &[SkillIndexEntry],
        tools: &[CatalogTool],
        memories: &[MemoryFactEntry],
        conversation_turns: &[ChatMessage],
        budget: &CompactPromptBudget,
        principal: &Principal,
    ) -> Result<AssembledCompactPrompt, String> {
        // Dynamic tool pruning: strip tools exceeding the principal's authority tier
        let pruned_tools = DynamicToolPruner::prune_tools(tools, principal);
        let tools = &pruned_tools;

        // --- 1. SECTION 1: STATIC SYSTEM RULES (<= 1000 tokens) ---
        let sys_text = base_system_rules.trim().to_string();
        let sys_tokens = estimate_tokens(&sys_text);
        if sys_tokens > budget.max_system_tokens {
            return Err(format!(
                "Static system rules exceed budget: {} > {}",
                sys_tokens, budget.max_system_tokens
            ));
        }

        // --- 2. SECTION 2: SKILL INDEX (<= 300 tokens) ---
        let mut skill_text = String::new();
        let mut skill_tokens = 0;
        if !skills.is_empty() {
            skill_text.push_str("\n\n## SKILL INDEX\n");
            for skill in skills {
                let line = format!("- {}: {}\n", skill.skill_id, skill.description);
                let line_tok = estimate_tokens(&line);
                if skill_tokens + line_tok <= budget.max_skill_index_tokens {
                    skill_text.push_str(&line);
                    skill_tokens += line_tok;
                }
            }
        }

        // --- 3. SECTION 3: TOOL DEFINITIONS (<= 1500 tokens) ---
        let mut tools_text = String::new();
        let mut tools_tokens = 0;
        if !tools.is_empty() {
            tools_text.push_str("\n## TOOLS CATALOG\n");
            for tool in tools {
                let line = Self::format_dense_tool_line(tool);
                let line_tok = estimate_tokens(&line);
                if tools_tokens + line_tok <= budget.max_tools_tokens {
                    tools_text.push_str(&line);
                    tools_tokens += line_tok;
                }
            }
        }

        // Fixed static prefix length (tokens 0..prefix are 100% stable across conversation turns!)
        let static_prefix_tokens = sys_tokens + skill_tokens + tools_tokens;

        // --- 4. SECTION 4: BITEMPORAL MEMORY CONTEXT (<= 1500 tokens) ---
        let mut memory_text = String::new();
        let mut memory_tokens = 0;
        let mut active_memories: Vec<&MemoryFactEntry> = memories.iter().filter(|m| m.is_active).collect();
        // Sort memories by confidence descending
        active_memories.sort_by(|a, b| b.confidence.partial_cmp(&a.confidence).unwrap_or(std::cmp::Ordering::Equal));

        if !active_memories.is_empty() {
            memory_text.push_str("\n## BITEMPORAL MEMORY CONTEXT\n");
            for fact in active_memories {
                let line = format!(
                    "- [{}] {} -> {} (scope: {}, conf: {:.2})\n",
                    fact.predicate, fact.subject, fact.object_value, fact.scope, fact.confidence
                );
                let line_tok = estimate_tokens(&line);
                if memory_tokens + line_tok <= budget.max_memory_tokens {
                    memory_text.push_str(&line);
                    memory_tokens += line_tok;
                }
            }
        }

        // Combine Sections 1-4 into unified leading system message
        let mut combined_system = String::with_capacity(sys_text.len() + skill_text.len() + tools_text.len() + memory_text.len());
        combined_system.push_str(&sys_text);
        combined_system.push_str(&skill_text);
        combined_system.push_str(&tools_text);
        combined_system.push_str(&memory_text);

        // --- 5. SECTION 5: DYNAMIC CONVERSATION TURNS ---
        let total_prefix_allocated = static_prefix_tokens + memory_tokens;
        let available_for_history = budget
            .n_ctx
            .saturating_sub(budget.reserve_response_tokens)
            .saturating_sub(total_prefix_allocated);

        let mut history_tokens = 0;
        let mut dropped_turns = 0;

        // Ensure latest user turn is mandatory
        let latest_user_idx = conversation_turns.iter().rposition(|m| m.role == "user");

        // Iterate backwards from newest to oldest turns to fit within remaining budget
        let mut turn_accumulator = Vec::new();
        for (i, msg) in conversation_turns.iter().enumerate().rev() {
            let is_mandatory = Some(i) == latest_user_idx;
            let sanitized_content = if msg.role == "tool" || msg.role == "system" {
                sanitize_untrusted(&msg.content)
            } else {
                msg.content.clone()
            };

            let tok = estimate_tokens(&sanitized_content) + 4; // Turn delimiter overhead
            if is_mandatory || (history_tokens + tok <= available_for_history) {
                history_tokens += tok;
                turn_accumulator.push(ChatMessage {
                    role: msg.role.clone(),
                    content: sanitized_content,
                });
            } else {
                dropped_turns += 1;
            }
        }

        // Restore chronological order
        turn_accumulator.reverse();
        let accepted_history = turn_accumulator;

        let mut full_messages = Vec::with_capacity(accepted_history.len() + 1);
        full_messages.push(ChatMessage {
            role: "system".to_string(),
            content: combined_system.clone(),
        });
        full_messages.extend(accepted_history);

        Ok(AssembledCompactPrompt {
            system_turn_content: combined_system,
            full_messages,
            estimated_system_tokens: sys_tokens,
            estimated_skill_index_tokens: skill_tokens,
            estimated_tools_tokens: tools_tokens,
            estimated_memory_tokens: memory_tokens,
            estimated_history_tokens: history_tokens,
            prefix_cache_tokens: static_prefix_tokens,
            dropped_history_turns: dropped_turns,
        })
    }
}
