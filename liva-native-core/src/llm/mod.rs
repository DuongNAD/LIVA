pub mod decision;
pub mod embed;
pub mod embedder;
pub mod embedding;
pub mod engine;
pub mod expert_governor;
pub mod output_filter;
pub mod prompt;
pub mod sampler;
pub mod scoped_tool_registry;
pub mod service;
pub mod sidecar;
pub mod tool_calling;
pub mod tool_insurance;
pub mod trust_cache;

pub use decision::{
    DecisionDistribution, DecisionOption, ScoredDecisionOption, compile_decision_prompt,
    compute_decision_distribution, render_decision_turn, resolve_model_option_tokens,
    resolve_option_token_with,
};
pub use embed::get_embedding;
pub use engine::{
    CompletionOutput, LlamaEngine, LlamaRouterManager, nen_sinh_tiep, nen_sinh_tiep_async,
};
pub use expert_governor::{ExpertSwapGovernor, ModelRole, SwapDecision};
pub use output_filter::{
    CLOSE_TAGS, OPEN_TAGS, ReasoningStreamSplitter, StreamChunk, TauriIpcChunk, VisibleOutputFilter,
};
pub use prompt::persona;
pub use prompt::{
    AssembledCompactPrompt, ChatMessage, CompactPromptAssembler, CompactPromptBudget,
    DynamicPromptAssembler, MemoryFactEntry, PromptAssemblyError, PromptBudget, PromptSlice,
    SkillDefinition, SkillIndexEntry, SlicePriority, compile_gemma_prompt, compile_prompt,
    estimate_tokens,
};
pub use sampler::{create_greedy_sampler, create_sampler};
pub use scoped_tool_registry::{
    CallToolResult, CommandPrincipal, NativeScopedToolRegistry, ScopeGuard, ScopedTool,
    ScopedToolRegistry, Tool, ToolContent, ToolError, ToolExecError, ToolExecPolicy,
    ToolExecutionContext, ToolScope, ToolScopeKind, async_trait,
};
pub use service::{LlmService, SharedRouter};
pub use sidecar::{
    FallbackAction, KvPrecision, LlamaHttpClient, LlamaSidecarClient, LlamaSidecarSupervisor,
    SidecarConfig, SidecarError, SidecarSlot, SlotActionResponse, SlotId, VramGovernor,
    VramPressureAssessment, VramPressureTier, VramSupervisor, allocate_ephemeral_port,
    generate_csprng_key,
};
pub use tool_calling::{CatalogTool, ExecPolicy, Selection, ToolCatalog};
pub use tool_insurance::{
    EscalationReason, HmacReceiptVerifier, InsuranceAction, MAX_SCHEMA_RETRIES,
    MAX_TOOL_CHAIN_DEPTH, ToolCallReceipt, ToolInsuranceEngine, canonical_json_hash,
    compute_hmac_sha256, verify_hmac_constant_time,
};
