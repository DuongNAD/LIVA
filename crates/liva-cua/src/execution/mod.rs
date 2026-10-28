//! Dual-Profile Execution Loop Subsystem (`Flash` & `Pro` modes) for LIVA CUA.
//!
//! Inspired by Google ARTEMIS:
//! - `Flash`: High-speed reactive observe-and-act execution using Set-of-Mark indices
//!   and bounded action history summaries.
//! - `Pro`: Pre- and post-action state verification combining semantic UI tree diffing,
//!   visual ROI diffing, anomaly detection (unexpected modals, no-ops), and automated self-healing.

pub mod anomaly;
pub mod healing;
pub mod history;
pub mod orchestrator;
pub mod profile;

pub use anomaly::{AnomalyDetector, ExecutionAnomaly, ProDefectReport};
pub use healing::{RecoveryActionTaken, SelfHealingEngine};
pub use history::{ActionHistoryTracker, CompactActionSummary};
pub use orchestrator::CuaExecutionOrchestrator;
pub use profile::{
    ExecutionProfile, ProfileAction, ProfileConfig, ProfileExecutionConfig, ProfileExecutionResult,
    ProfilePlan, ProfileStepResult,
};
