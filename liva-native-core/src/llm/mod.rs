pub mod embed;
pub mod embedder;
pub mod engine;
pub(crate) mod output_filter;
pub mod pool;
pub mod prompt;
pub mod sampler;
pub mod tool_calling;

pub use embed::get_embedding;
pub use engine::{CompletionOutput, LlamaEngine, LlamaRouterManager};
pub use pool::{
    CancellationToken, LlamaRouterBackend, LlmCompletionRequest, LlmCompletionResult,
    LlmEngineBackend, LlmPoolError, LlmPriority, LlmWorkerPool, LlmWorkerPoolService, PoolMetrics,
    PoolMetricsSnapshot, SimulatedEngineBackend, TokenStreamDelta,
};
pub use prompt::persona;
pub use prompt::{ChatMessage, compile_gemma_prompt, compile_prompt};
pub use sampler::{create_greedy_sampler, create_sampler};
pub use tool_calling::{CatalogTool, ExecPolicy, Selection, ToolCatalog};

