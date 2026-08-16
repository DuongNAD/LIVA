pub mod embed;
pub mod embedder;
pub mod engine;
pub mod output_filter;
pub mod prompt;
pub mod sampler;
pub mod scoped_tool_registry;
pub mod tool_calling;

pub use embed::get_embedding;
pub use engine::{CompletionOutput, LlamaEngine, LlamaRouterManager};
pub use output_filter::{
    CLOSE_TAGS, OPEN_TAGS, ReasoningStreamSplitter, StreamChunk, TauriIpcChunk, VisibleOutputFilter,
};
pub use prompt::persona;
pub use prompt::{
    ChatMessage, DynamicPromptAssembler, PromptAssemblyError, PromptBudget, PromptSlice,
    SkillDefinition, SlicePriority, compile_gemma_prompt, compile_prompt,
};
pub use sampler::{create_greedy_sampler, create_sampler};
pub use scoped_tool_registry::{
    ScopeGuard, ScopedToolRegistry, ToolError, ToolExecError, ToolScope,
};
pub use tool_calling::{CatalogTool, ExecPolicy, Selection, ToolCatalog};
