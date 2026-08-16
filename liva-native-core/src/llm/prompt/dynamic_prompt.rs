use crate::llm::prompt::ChatMessage;
use crate::llm::prompt::compile_prompt;
use crate::llm::tool_calling::CatalogTool;
use serde::{Deserialize, Serialize};

/// Configurable token ceilings for system persona, tools, and response headroom.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptBudget {
    pub max_system_tokens: usize,
    pub max_tool_tokens: usize,
    pub reserve_response_tokens: usize,
}

impl PromptBudget {
    pub fn new(
        max_system_tokens: usize,
        max_tool_tokens: usize,
        reserve_response_tokens: usize,
    ) -> Self {
        Self {
            max_system_tokens,
            max_tool_tokens,
            reserve_response_tokens,
        }
    }

    pub fn from_context_window(n_ctx: usize, reserve_response_tokens: usize) -> Self {
        let available = n_ctx.saturating_sub(reserve_response_tokens);
        let max_system = (available * 40) / 100;
        let max_tool = (available * 30) / 100;
        Self {
            max_system_tokens: max_system.max(256),
            max_tool_tokens: max_tool.max(128),
            reserve_response_tokens,
        }
    }

    pub fn total_budget(&self) -> usize {
        self.max_system_tokens.saturating_add(self.max_tool_tokens)
    }
}

/// Eviction priority for prompt slices (P0 is never evicted; P4 is evicted first).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[allow(non_camel_case_types)]
pub enum SlicePriority {
    /// P0: System Core Persona & Essential Constraints (Never evicted)
    P0_SystemCore = 0,
    /// P1: Base System Capabilities & Instructions
    P1_BaseCapabilities = 1,
    /// P2: Active Tool Schemas
    P2_ActiveTools = 2,
    /// P3: Domain Skills & Recalled Memory Facts
    P3_DomainSkills = 3,
    /// P4: Dynamic Conversation Context & History
    P4_DynamicContext = 4,
}

/// Discrete prompt slice with eviction priority, estimated tokens, and sequence index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PromptSlice {
    pub id: String,
    pub priority: SlicePriority,
    pub sequence_order: usize,
    pub message: ChatMessage,
    pub estimated_tokens: usize,
}

impl PromptSlice {
    pub fn new(
        id: impl Into<String>,
        priority: SlicePriority,
        sequence_order: usize,
        message: ChatMessage,
    ) -> Self {
        let estimated_tokens = message.content.len().div_ceil(4) + 4;
        Self {
            id: id.into(),
            priority,
            sequence_order,
            message,
            estimated_tokens,
        }
    }

    pub fn with_tokens(
        id: impl Into<String>,
        priority: SlicePriority,
        sequence_order: usize,
        message: ChatMessage,
        estimated_tokens: usize,
    ) -> Self {
        Self {
            id: id.into(),
            priority,
            sequence_order,
            message,
            estimated_tokens,
        }
    }
}

/// Skill metadata definition for dynamic prompt injection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillDefinition {
    pub skill_id: String,
    pub name: String,
    pub description: String,
    pub instructions: String,
    pub priority: f32,
    pub estimated_tokens: usize,
}

/// Errors occurring during dynamic prompt assembly or budget enforcement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PromptAssemblyError {
    BudgetExceeded,
    InvalidConfiguration,
    EmptyPrompt,
    SlicingError(String),
}

impl std::fmt::Display for PromptAssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BudgetExceeded => write!(f, "Prompt assembly budget exceeded"),
            Self::InvalidConfiguration => write!(f, "Invalid prompt budget configuration"),
            Self::EmptyPrompt => write!(f, "Cannot assemble empty prompt"),
            Self::SlicingError(msg) => write!(f, "Prompt slicing error: {msg}"),
        }
    }
}

impl std::error::Error for PromptAssemblyError {}

/// Dynamic prompt compilation engine supporting token budgeting and priority-ranked slice eviction.
pub struct DynamicPromptAssembler;

impl DynamicPromptAssembler {
    /// Formats a `CatalogTool` into a compact 1-line schema representation.
    pub fn format_compact_tool_schema(tool: &CatalogTool) -> String {
        let mut params = Vec::new();
        if let Some(props) = tool
            .input_schema
            .get("properties")
            .and_then(|p| p.as_object())
        {
            let required_list: Vec<&str> = tool
                .input_schema
                .get("required")
                .and_then(|r| r.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                .unwrap_or_default();
            for (key, _) in props {
                if required_list.contains(&key.as_str()) {
                    params.push(format!("{key}*"));
                } else {
                    params.push(key.clone());
                }
            }
        }
        if params.is_empty() {
            format!("- {}: {}\n", tool.name, tool.description)
        } else {
            format!(
                "- {}: {} (params: {})\n",
                tool.name,
                tool.description,
                params.join(", ")
            )
        }
    }

    /// Assemble dynamic system prompt from base prompt, active skills, and active tools.
    pub fn assemble_prompt(
        base_prompt: &str,
        active_skills: &[SkillDefinition],
        active_tools: &[CatalogTool],
        budget: &PromptBudget,
    ) -> Result<String, PromptAssemblyError> {
        if budget.max_system_tokens == 0 {
            return Err(PromptAssemblyError::InvalidConfiguration);
        }

        let base_tokens = base_prompt.len().div_ceil(4);
        if base_tokens > budget.max_system_tokens {
            return Err(PromptAssemblyError::BudgetExceeded);
        }

        let mut available_skill_budget = budget.max_system_tokens.saturating_sub(base_tokens);

        // Sort skills by priority descending
        let mut sorted_skills: Vec<&SkillDefinition> = active_skills.iter().collect();
        sorted_skills.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        let mut included_skills = Vec::new();
        for skill in sorted_skills {
            if skill.estimated_tokens <= available_skill_budget {
                available_skill_budget =
                    available_skill_budget.saturating_sub(skill.estimated_tokens);
                included_skills.push(skill);
            }
        }

        let mut output = String::new();
        output.push_str(base_prompt);

        if !included_skills.is_empty() {
            output.push_str("\n\n## ACTIVE SKILLS\n");
            for skill in &included_skills {
                output.push_str(&format!("- **{}**: {}\n", skill.name, skill.instructions));
            }
        }

        if !active_tools.is_empty() && budget.max_tool_tokens > 0 {
            output.push_str("\n## AVAILABLE TOOLS\n");
            let mut tool_tokens = 0;
            for tool in active_tools {
                let line = Self::format_compact_tool_schema(tool);
                let tokens = line.len().div_ceil(4);
                if tool_tokens + tokens <= budget.max_tool_tokens {
                    output.push_str(&line);
                    tool_tokens += tokens;
                }
            }
        }

        Ok(output)
    }

    /// Assemble multi-turn chat messages from prioritized slices under strict budget constraints.
    ///
    /// IMPORTANT: Pruning drops lower-priority slices first (P4 -> P3 -> P2 -> P1),
    /// but the accepted slices are sorted strictly by `sequence_order` to preserve
    /// chronological conversational history integrity!
    pub fn assemble_messages(
        slices: &[PromptSlice],
        budget: &PromptBudget,
    ) -> Result<Vec<ChatMessage>, PromptAssemblyError> {
        let max_budget = budget.total_budget();
        if max_budget == 0 {
            return Err(PromptAssemblyError::InvalidConfiguration);
        }

        if slices.is_empty() {
            return Ok(Vec::new());
        }

        // Check if mandatory P0 core slices exceed total budget
        let p0_tokens: usize = slices
            .iter()
            .filter(|s| s.priority == SlicePriority::P0_SystemCore)
            .map(|s| s.estimated_tokens)
            .sum();

        if p0_tokens > max_budget {
            return Err(PromptAssemblyError::BudgetExceeded);
        }

        // Determine which slices to keep via priority-based eviction
        // Higher priority enum value means earlier eviction (P4 evicted first, P0 never evicted)
        // For identical priority in P4 (conversation history), older turns (smaller sequence_order)
        // are evicted before newer turns.
        let mut slice_indices: Vec<usize> = (0..slices.len()).collect();
        slice_indices.sort_by(|&a, &b| {
            let sa = &slices[a];
            let sb = &slices[b];
            if sa.priority != sb.priority {
                // Slices with lower priority enum (e.g. P0 < P1) are kept first
                sa.priority.cmp(&sb.priority)
            } else if sa.priority == SlicePriority::P4_DynamicContext {
                // In conversation history, newer turns (larger sequence_order) are preferred
                sb.sequence_order.cmp(&sa.sequence_order)
            } else {
                sa.sequence_order.cmp(&sb.sequence_order)
            }
        });

        let mut accumulated_tokens = 0;
        let mut accepted_indices = Vec::new();

        for idx in slice_indices {
            let slice = &slices[idx];
            if accumulated_tokens + slice.estimated_tokens <= max_budget {
                accumulated_tokens += slice.estimated_tokens;
                accepted_indices.push(idx);
            }
        }

        // Re-sort accepted slices by chronological sequence_order to prevent dialogue scrambling!
        accepted_indices.sort_by_key(|&idx| slices[idx].sequence_order);

        let messages: Vec<ChatMessage> = accepted_indices
            .into_iter()
            .map(|idx| slices[idx].message.clone())
            .collect();

        Ok(messages)
    }

    /// Compile dynamic prompt messages into final template string (ChatML or Gemma).
    pub fn compile_budgeted_prompt(
        slices: &[PromptSlice],
        budget: &PromptBudget,
    ) -> Result<String, PromptAssemblyError> {
        let messages = Self::assemble_messages(slices, budget)?;
        if messages.is_empty() {
            return Err(PromptAssemblyError::EmptyPrompt);
        }
        compile_prompt(&messages).map_err(PromptAssemblyError::SlicingError)
    }
}
