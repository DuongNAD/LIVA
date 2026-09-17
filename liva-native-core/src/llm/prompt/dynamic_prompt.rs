use crate::llm::engine::RESERVE_FOR_COMPLETION;
use crate::llm::prompt::ChatMessage;
use crate::llm::prompt::compile_prompt;
use crate::llm::tool_calling::CatalogTool;
use serde::{Deserialize, Serialize};

/// Validated context budget: prompt + response reserve must be strictly below n_ctx.
/// Private fields prevent callers from bypassing validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContextTokenBudget {
    n_ctx: usize,
    reserve_response_tokens: usize,
}

impl ContextTokenBudget {
    pub fn new(n_ctx: usize, reserve_response_tokens: usize) -> Result<Self, PromptAssemblyError> {
        let max_prompt = n_ctx
            .checked_sub(reserve_response_tokens)
            .and_then(|remaining| remaining.checked_sub(1));
        if reserve_response_tokens == 0 || !max_prompt.is_some_and(|tokens| tokens > 0) {
            return Err(PromptAssemblyError::InvalidConfiguration);
        }
        Ok(Self {
            n_ctx,
            reserve_response_tokens,
        })
    }

    pub fn max_prompt_tokens(&self) -> usize {
        // The constructor proves both subtractions safe; fields are immutable outside this module.
        self.n_ctx - self.reserve_response_tokens - 1
    }

    pub fn can_fit(&self, prompt_tokens: usize) -> bool {
        prompt_tokens <= self.max_prompt_tokens()
    }
}

/// Compute floor(tokens * percent / 100) without overflowing usize.
/// All callers supply a fixed percentage in 0..=100.
fn token_percentage(tokens: usize, percent: usize) -> usize {
    (tokens / 100) * percent + ((tokens % 100) * percent) / 100
}

/// Configurable estimated ceilings; not an exact-token safety guard.
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
        let available = ContextTokenBudget::new(n_ctx, reserve_response_tokens)
            .map_or(0, |budget| budget.max_prompt_tokens());
        let max_system = token_percentage(available, 40);
        let max_tool = token_percentage(available, 30);
        Self {
            max_system_tokens: max_system,
            max_tool_tokens: max_tool,
            reserve_response_tokens,
        }
    }

    /// Default dialogue budget tuned for runtime multi-turn interaction.
    /// Allocates estimated headroom; exact token measurement is still required.
    pub fn for_dialogue(n_ctx: usize) -> Self {
        let reserve = RESERVE_FOR_COMPLETION;
        // Scale the estimation cushion down on narrow contexts; keep the
        // established 31-token cushion for normal contexts. Never reduce reserve.
        let cushion = token_percentage(n_ctx, 1).min(31);
        let available = ContextTokenBudget::new(n_ctx, reserve).map_or(0, |budget| {
            budget.max_prompt_tokens().saturating_sub(cushion)
        });
        let max_system = token_percentage(available, 55);
        let max_tool = available.saturating_sub(max_system);
        Self {
            max_system_tokens: max_system,
            max_tool_tokens: max_tool,
            reserve_response_tokens: reserve,
        }
    }

    pub fn total_budget(&self) -> usize {
        self.max_system_tokens.saturating_add(self.max_tool_tokens)
    }

    // Compatibility APIs have no n_ctx parameter. Validate their implied
    // ceiling without changing the meaning of the two prompt quotas.
    fn context_budget(&self) -> Result<ContextTokenBudget, PromptAssemblyError> {
        let n_ctx = self
            .max_system_tokens
            .checked_add(self.max_tool_tokens)
            .and_then(|tokens| tokens.checked_add(self.reserve_response_tokens))
            .and_then(|tokens| tokens.checked_add(1))
            .ok_or(PromptAssemblyError::InvalidConfiguration)?;
        ContextTokenBudget::new(n_ctx, self.reserve_response_tokens)
    }
}

impl Default for PromptBudget {
    fn default() -> Self {
        Self::for_dialogue(4096)
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
    InvalidSelection(String),
    TokenizationError(String),
    RequiredContentTooLarge {
        required_tokens: usize,
        max_prompt_tokens: usize,
    },
}

impl std::fmt::Display for PromptAssemblyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BudgetExceeded => write!(f, "Prompt assembly budget exceeded"),
            Self::InvalidConfiguration => write!(f, "Invalid prompt budget configuration"),
            Self::EmptyPrompt => write!(f, "Cannot assemble empty prompt"),
            Self::SlicingError(msg) => write!(f, "Prompt slicing error: {msg}"),
            Self::InvalidSelection(msg) => write!(f, "Invalid prompt selection: {msg}"),
            Self::TokenizationError(msg) => write!(f, "Prompt tokenization error: {msg}"),
            Self::RequiredContentTooLarge {
                required_tokens,
                max_prompt_tokens,
            } => write!(
                f,
                "Required prompt content exceeds budget: {required_tokens} tokens, maximum {max_prompt_tokens}"
            ),
        }
    }
}

impl std::error::Error for PromptAssemblyError {}

/// Compiled prompt measured by the supplied tokenizer, never by slice estimates.
#[derive(Debug, Clone)]
pub struct BudgetedPrompt {
    pub prompt: String,
    pub prompt_tokens: usize,
    pub dropped_slice_ids: Vec<String>,
}

/// Each group is retained or evicted as a unit. Ungrouped slices are singletons.
#[derive(Debug, Clone, Default)]
pub struct PromptSelection {
    pub mandatory_ids: Vec<String>,
    pub atomic_groups: Vec<Vec<String>>,
}

struct PromptGroup {
    indices: Vec<usize>,
    mandatory: bool,
    priority: SlicePriority,
    sequence_order: usize,
}

fn validated_prompt_groups(
    slices: &[PromptSlice],
    selection: &PromptSelection,
) -> Result<Vec<PromptGroup>, PromptAssemblyError> {
    use std::collections::{HashMap, HashSet};
    let invalid = || {
        PromptAssemblyError::InvalidSelection(
            "IDs must be unique and known; groups must be nonempty and disjoint".into(),
        )
    };
    let mut ids = HashMap::with_capacity(slices.len());
    for (idx, slice) in slices.iter().enumerate() {
        if slice.id.is_empty() || ids.insert(slice.id.as_str(), idx).is_some() {
            return Err(invalid());
        }
    }
    let mut mandatory = HashSet::new();
    for id in &selection.mandatory_ids {
        let &idx = ids.get(id.as_str()).ok_or_else(&invalid)?;
        if !mandatory.insert(idx) {
            return Err(invalid());
        }
    }
    let mut assigned = vec![false; slices.len()];
    let mut groups = Vec::new();
    for group_ids in &selection.atomic_groups {
        if group_ids.is_empty() {
            return Err(invalid());
        }
        let mut indices = Vec::with_capacity(group_ids.len());
        for id in group_ids {
            let &idx = ids.get(id.as_str()).ok_or_else(&invalid)?;
            if assigned[idx] {
                return Err(invalid());
            }
            assigned[idx] = true;
            indices.push(idx);
        }
        groups.push(indices);
    }
    for (idx, assigned) in assigned.iter().enumerate() {
        if !assigned {
            groups.push(vec![idx]);
        }
    }
    Ok(groups
        .into_iter()
        .map(|indices| {
            // Each group is nonempty. Mixed groups inherit their most protected priority.
            let mut priority = SlicePriority::P4_DynamicContext;
            let mut sequence_order = usize::MAX;
            let mut required = false;
            for &idx in &indices {
                priority = priority.min(slices[idx].priority);
                sequence_order = sequence_order.min(slices[idx].sequence_order);
                required |= mandatory.contains(&idx)
                    || slices[idx].priority == SlicePriority::P0_SystemCore;
            }
            PromptGroup {
                indices,
                mandatory: required,
                priority,
                sequence_order,
            }
        })
        .collect())
}

/// Dynamic prompt compilation engine supporting token budgeting and priority-ranked slice eviction.
pub struct DynamicPromptAssembler;

impl DynamicPromptAssembler {
    /// Keep every system message and the latest user turn (including its suffix).
    /// History is grouped by user turn. Orphan assistant/tool prefixes are rejected;
    /// ChatMessage has no call IDs with which to reconstruct missing dependencies.
    pub fn select_chat_messages(
        messages: &[ChatMessage],
    ) -> Result<(Vec<PromptSlice>, PromptSelection), PromptAssemblyError> {
        let latest_user = messages
            .iter()
            .rposition(|m| m.role == "user")
            .ok_or(PromptAssemblyError::EmptyPrompt)?;
        let mut slices = Vec::with_capacity(messages.len());
        let mut selection = PromptSelection::default();
        let mut turn = Vec::new();
        for (idx, message) in messages.iter().enumerate() {
            let id = format!("msg_{idx}");
            let system = message.role == "system";
            if !system {
                if message.role == "user" {
                    if !turn.is_empty() {
                        selection.atomic_groups.push(std::mem::take(&mut turn));
                    }
                } else if turn.is_empty() {
                    return Err(PromptAssemblyError::InvalidSelection(
                        "Conversation prefix has no user turn".into(),
                    ));
                }
                turn.push(id.clone());
            }
            if system || idx >= latest_user {
                selection.mandatory_ids.push(id.clone());
            }
            let priority = if system || idx == latest_user {
                SlicePriority::P0_SystemCore
            } else if idx >= latest_user {
                SlicePriority::P1_BaseCapabilities
            } else {
                SlicePriority::P4_DynamicContext
            };
            slices.push(PromptSlice::new(id, priority, idx, message.clone()));
        }
        if !turn.is_empty() {
            selection.atomic_groups.push(turn);
        }
        Ok((slices, selection))
    }

    /// Compile and measure the entire prompt after each atomic eviction.
    /// Production callers must supply the active template compiler and tokenizer.
    /// Estimates are deliberately ignored. Callback errors are sanitized because
    /// upstream diagnostics can contain private prompts or model paths.
    pub fn compile_exact_budgeted_prompt<C, M>(
        slices: &[PromptSlice],
        selection: &PromptSelection,
        budget: &ContextTokenBudget,
        compile: C,
        mut measure: M,
    ) -> Result<BudgetedPrompt, PromptAssemblyError>
    where
        C: Fn(&[ChatMessage]) -> Result<String, String>,
        M: FnMut(&str) -> Result<usize, String>,
    {
        let groups = validated_prompt_groups(slices, selection)?;
        if slices.is_empty() {
            return Err(PromptAssemblyError::EmptyPrompt);
        }
        let mut order: Vec<usize> = (0..slices.len()).collect();
        order.sort_by_key(|&idx| slices[idx].sequence_order);
        let mut optional: Vec<&PromptGroup> = groups.iter().filter(|g| !g.mandatory).collect();
        optional.sort_by(|a, b| {
            b.priority
                .cmp(&a.priority)
                .then(a.sequence_order.cmp(&b.sequence_order))
        });
        let mut evictions = optional.into_iter();
        let mut retained = vec![true; slices.len()];
        loop {
            let messages: Vec<ChatMessage> = order
                .iter()
                .filter(|&&idx| retained[idx])
                .map(|&idx| slices[idx].message.clone())
                .collect();
            if messages.is_empty() {
                return Err(PromptAssemblyError::EmptyPrompt);
            }
            let prompt = compile(&messages).map_err(|_| {
                PromptAssemblyError::SlicingError("Template compilation failed".into())
            })?;
            if prompt.is_empty() {
                return Err(PromptAssemblyError::EmptyPrompt);
            }
            let prompt_tokens = measure(&prompt).map_err(|_| {
                PromptAssemblyError::TokenizationError("Token measurement failed".into())
            })?;
            if budget.can_fit(prompt_tokens) {
                return Ok(BudgetedPrompt {
                    prompt,
                    prompt_tokens,
                    dropped_slice_ids: order
                        .iter()
                        .filter(|&&idx| !retained[idx])
                        .map(|&idx| slices[idx].id.clone())
                        .collect(),
                });
            }
            let Some(group) = evictions.next() else {
                return Err(PromptAssemblyError::RequiredContentTooLarge {
                    required_tokens: prompt_tokens,
                    max_prompt_tokens: budget.max_prompt_tokens(),
                });
            };
            for &idx in &group.indices {
                retained[idx] = false;
            }
        }
    }

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
        Self::assemble_messages_in_context(slices, budget, &budget.context_budget()?)
    }

    /// Enforce an actual context ceiling, including response reserve. Token
    /// counts remain estimates; runtime inference must use the exact compiler.
    pub fn assemble_messages_in_context(
        slices: &[PromptSlice],
        budget: &PromptBudget,
        context: &ContextTokenBudget,
    ) -> Result<Vec<ChatMessage>, PromptAssemblyError> {
        if budget.reserve_response_tokens != context.reserve_response_tokens {
            return Err(PromptAssemblyError::InvalidConfiguration);
        }
        let max_budget = budget.total_budget().min(context.max_prompt_tokens());
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
            .try_fold(0usize, |total, slice| {
                total.checked_add(slice.estimated_tokens)
            })
            .ok_or(PromptAssemblyError::BudgetExceeded)?;

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
            if slice.estimated_tokens <= max_budget - accumulated_tokens {
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

    /// Automatically converts a slice of ChatMessage into prioritized PromptSlices,
    /// applies prompt budgeting with priority-based eviction (P4 conversation history evicted first,
    /// older turns before newer turns, P0 system core preserved), and returns the budgeted messages
    /// sorted chronologically.
    pub fn budget_chat_messages(
        messages: &[ChatMessage],
        budget: &PromptBudget,
    ) -> Result<Vec<ChatMessage>, PromptAssemblyError> {
        Self::budget_chat_messages_in_context(messages, budget, &budget.context_budget()?)
    }

    /// Estimate-based compatibility selection with an explicit context ceiling.
    /// Keeps the first system and latest user intact; unlike the exact runtime
    /// selector, optional history here is not grouped into atomic tool turns.
    pub fn budget_chat_messages_in_context(
        messages: &[ChatMessage],
        budget: &PromptBudget,
        context: &ContextTokenBudget,
    ) -> Result<Vec<ChatMessage>, PromptAssemblyError> {
        let latest_user = messages.iter().rposition(|msg| msg.role == "user");
        let first_system = messages.iter().position(|msg| msg.role == "system");
        let mut slices = Vec::with_capacity(messages.len());
        for (idx, msg) in messages.iter().enumerate() {
            let priority = match msg.role.as_str() {
                "system" if Some(idx) == first_system => SlicePriority::P0_SystemCore,
                "system" => SlicePriority::P3_DomainSkills,
                "user" if Some(idx) == latest_user => SlicePriority::P0_SystemCore,
                _ => SlicePriority::P4_DynamicContext,
            };
            slices.push(PromptSlice::new(
                format!("msg_{idx}"),
                priority,
                idx,
                msg.clone(),
            ));
        }
        Self::assemble_messages_in_context(&slices, budget, context)
    }
}
