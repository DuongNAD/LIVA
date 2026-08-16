//! Cognitive Runtime Subsystem for LIVA.
//!
//! Provides data contracts, static 4-tier risk policy evaluation, action proposal validation,
//! verified tool observations, cryptographic idempotency tracking, and secret-redacted audit ledgers.

pub mod events;
pub mod idempotency;
pub mod memory;
pub mod observation;
pub mod policy;
pub mod proposal;
pub mod redaction;

pub use events::{
    EventSensitivity, PerceptionEvent, PerceptionPayload, SessionEvent, SessionEventStream,
};
pub use idempotency::{
    IdempotencyCheckResult, IdempotencyManager, IdempotencyRecord, IdempotencyState,
};
pub use memory::{
    CognitiveFact, CognitiveMemoryCoordinator, ConflictResolutionAction, FactDeletionCounts,
    FactHistoryRecord, FactUpsertOutcome, MemoryConflictRecord, MemoryDeleteCoordinator,
    MemoryProvenance,
};
pub use observation::{AuditTrace, ObservationStatus, SideEffectRecord, ToolObservation};
pub use policy::{PolicyDecision, PolicyEngine, RiskTier};
pub use proposal::{ActionProposal, UndoHint};
pub use redaction::{ActionAuditRecord, RedactedAuditLedger, SecretScrubber};
