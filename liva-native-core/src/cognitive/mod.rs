//! Cognitive Runtime Subsystem for LIVA.
//!
//! Provides data contracts, static 4-tier risk policy evaluation, action proposal validation,
//! verified tool observations, cryptographic idempotency tracking, and secret-redacted audit ledgers.

pub mod audit_chain;
pub mod deterministic_writer;
pub mod events;
pub mod idempotency;
pub mod markdown_sync;
pub mod memory;
pub mod observation;
pub mod policy;
pub mod proposal;
pub mod redaction;
pub mod triple_extractor;

pub use audit_chain::{AuditBlock, Blake3AuditChain, GENESIS_PREV_HASH};
pub use deterministic_writer::{
    AntiHallucinationResult, AntiHallucinationVerifier, BitemporalReconcileOp, CandidateFact,
    ChannelIntegrity, DeterministicPostTurnWriter, DeterministicReconciliationEngine,
    ExtractedFactCandidate, FactCandidate, FactExtractionRule, MemGhostFilter,
    MemGhostFilterResult, MessageRole, PostTurnProcessingResult, PostTurnWriteResult,
    ReconciliationAction, ReconciliationOutcome,
};
pub use events::{
    EventSensitivity, PerceptionEvent, PerceptionPayload, SessionEvent, SessionEventStream,
};
pub use idempotency::{
    IdempotencyCheckResult, IdempotencyManager, IdempotencyRecord, IdempotencyState,
};
pub use markdown_sync::{MarkdownSyncConfig, MarkdownSyncManager, MarkdownSyncReport, SyncReport};
pub use memory::{
    CognitiveFact, CognitiveMemoryCoordinator, ConflictResolutionAction, FactDeletionCounts,
    FactHistoryRecord, FactUpsertOutcome, MemoryConflictRecord, MemoryDeleteCoordinator,
    MemoryProvenance,
};
pub use observation::{AuditTrace, ObservationStatus, SideEffectRecord, ToolObservation};
pub use policy::{
    PolicyDecision, PolicyEngine, RiskTier, canonicalize_json, compute_canonical_args_hash,
    visualize_invisible_chars,
};
pub use proposal::{ActionProposal, UndoHint};
pub use redaction::{ActionAuditRecord, RedactedAuditLedger, SecretScrubber};
