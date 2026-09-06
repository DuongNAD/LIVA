//! Context Compression & Recursive Summary Tree Subsystem (Phase 3 Milestone 2)
//!
//! Provides:
//! 1. `llmlingua`: LLMLingua-2 Token Entropy & Perplexity Pruning Engine
//!    (3x-5x compression ratio, <1.5% semantic loss, protected masks for XML, Code, JSON, Entities).
//! 2. `summary_tree`: Hierarchical Recursive Summary Tree with dynamic token overflow trigger,
//!    chunk-level abstracts (Level 1), meta-summaries (Level 2+), Merkle integrity hashes,
//!    and SQLite turn ID lineage tracking.

pub mod llmlingua;
pub mod summary_tree;

pub use llmlingua::{
    CompressionMetrics, CompressionResult, LLMLinguaCompressor, LLMLinguaConfig,
    ProtectionReason, TokenMetadata,
};

pub use summary_tree::{
    CondensationReport, DialogueTurn, RecursiveSummaryTree, SummaryNode, SummaryTreeConfig,
};
