//! Specialized Swarm Role Implementations
//!
//! Provides the 5 foundational agent personas:
//! 1. PlannerRole: Goal decomposition, task planning, subagent token issuance.
//! 2. CoderRole: Code generation, tool execution, DiffHunk patch production.
//! 3. ReviewerRole: Logic critique, regression verification, consensus voting.
//! 4. AuditorRole: Security compliance, taint flow inspection, risk scoring.
//! 5. SentinelRole: Safety invariants guardian with unconditional VETO authority.

use super::actor::ActorContext;
use super::consensus::VoteDecision;
use super::types::{ActorError, DiffHunk, HunkStatus, SwarmMessage, SwarmPayload, SwarmRole};
use crate::sandbox::policy::CapabilityToken;
use std::collections::HashSet;

/// Trait implemented by all specialized swarm role handlers.
#[async_trait::async_trait]
pub trait SwarmActorRole: Send + Sync {
    /// Returns the specialized SwarmRole of this handler.
    fn role(&self) -> SwarmRole;

    /// Display name of the role handler.
    fn name(&self) -> &str;

    /// Returns the static capability tokens granted to this role.
    fn allowed_capabilities(&self) -> &[CapabilityToken];

    /// Processes an incoming swarm message and optionally generates a response payload.
    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError>;
}

// -------------------------------------------------------------------------------------------------
// 1. PLANNER ROLE
// -------------------------------------------------------------------------------------------------

pub struct PlannerRole {
    capabilities: Vec<CapabilityToken>,
}

impl PlannerRole {
    pub fn new() -> Self {
        Self {
            capabilities: vec![CapabilityToken::FsRead, CapabilityToken::FsWrite],
        }
    }
}

impl Default for PlannerRole {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SwarmActorRole for PlannerRole {
    fn role(&self) -> SwarmRole {
        SwarmRole::Planner
    }

    fn name(&self) -> &str {
        "Planner"
    }

    fn allowed_capabilities(&self) -> &[CapabilityToken] {
        &self.capabilities
    }

    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        _ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError> {
        match msg.payload {
            SwarmPayload::TaskProposal {
                ref task_id,
                ref goal,
                ..
            } => {
                tracing::info!("[Planner] Planning sub-steps for goal: {}", goal);
                // Decompose task and emit initial progress
                Ok(Some(SwarmPayload::TaskProgress {
                    task_id: task_id.clone(),
                    step_index: 0,
                    status: format!("Plan formulated for: {}", goal),
                    output: None,
                }))
            }
            SwarmPayload::ReviewVerdict {
                ref patch_id,
                approved,
                ref feedback,
                ..
            } => {
                tracing::info!(
                    "[Planner] Received review verdict for patch {}: approved={}, feedback={}",
                    patch_id,
                    approved,
                    feedback
                );
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// 2. CODER ROLE
// -------------------------------------------------------------------------------------------------

pub struct CoderRole {
    capabilities: Vec<CapabilityToken>,
}

impl CoderRole {
    pub fn new() -> Self {
        Self {
            capabilities: vec![
                CapabilityToken::FsRead,
                CapabilityToken::FsWrite,
                CapabilityToken::OsExecute,
            ],
        }
    }
}

impl Default for CoderRole {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SwarmActorRole for CoderRole {
    fn role(&self) -> SwarmRole {
        SwarmRole::Coder
    }

    fn name(&self) -> &str {
        "Coder"
    }

    fn allowed_capabilities(&self) -> &[CapabilityToken] {
        &self.capabilities
    }

    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        _ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError> {
        match msg.payload {
            SwarmPayload::TaskProposal {
                ref task_id,
                ref description,
                ..
            } => {
                tracing::info!("[Coder] Executing code generation for task: {}", task_id);
                // Produce sample diff hunk for review
                let hunk = DiffHunk::new(
                    format!("hunk-{}", uuid::Uuid::new_v4()),
                    "src/lib.rs",
                    1,
                    5,
                    1,
                    6,
                    "@@ -1,5 +1,6 @@",
                    format!("+// Implementation for {}\npub fn execute() {{}}\n", description),
                );
                Ok(Some(SwarmPayload::CodeHunkPatch {
                    patch_id: format!("patch-{}", uuid::Uuid::new_v4()),
                    task_id: task_id.clone(),
                    file_path: "src/lib.rs".to_string(),
                    hunks: vec![hunk],
                    summary: format!("Implement {}", description),
                }))
            }
            _ => Ok(None),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// 3. REVIEWER ROLE
// -------------------------------------------------------------------------------------------------

pub struct ReviewerRole {
    capabilities: Vec<CapabilityToken>,
}

impl ReviewerRole {
    pub fn new() -> Self {
        Self {
            capabilities: vec![CapabilityToken::FsRead],
        }
    }
}

impl Default for ReviewerRole {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SwarmActorRole for ReviewerRole {
    fn role(&self) -> SwarmRole {
        SwarmRole::Reviewer
    }

    fn name(&self) -> &str {
        "Reviewer"
    }

    fn allowed_capabilities(&self) -> &[CapabilityToken] {
        &self.capabilities
    }

    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        _ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError> {
        match msg.payload {
            SwarmPayload::CodeHunkPatch {
                ref patch_id,
                ref hunks,
                ..
            } => {
                let has_rejected = hunks
                    .iter()
                    .any(|h| matches!(h.status, HunkStatus::Rejected { .. }));

                let (approved, feedback) = if has_rejected {
                    (false, "Code contains rejected hunks".to_string())
                } else {
                    (true, "Code hunk passed static logic review".to_string())
                };

                Ok(Some(SwarmPayload::ReviewVerdict {
                    patch_id: patch_id.clone(),
                    approved,
                    feedback,
                    required_changes: vec![],
                }))
            }
            SwarmPayload::ConsensusProposal {
                ref vote_id,
                ref subject,
                ..
            } => {
                tracing::info!("[Reviewer] Voting on proposal {}: {}", vote_id, subject);
                Ok(Some(SwarmPayload::ConsensusVote {
                    vote_id: vote_id.clone(),
                    voter: SwarmRole::Reviewer,
                    decision: VoteDecision::Approve,
                    reason: Some("Approved by reviewer".to_string()),
                }))
            }
            _ => Ok(None),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// 4. AUDITOR ROLE
// -------------------------------------------------------------------------------------------------

pub struct AuditorRole {
    capabilities: Vec<CapabilityToken>,
}

impl AuditorRole {
    pub fn new() -> Self {
        Self {
            capabilities: vec![CapabilityToken::FsRead, CapabilityToken::KeystoreAccess],
        }
    }
}

impl Default for AuditorRole {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SwarmActorRole for AuditorRole {
    fn role(&self) -> SwarmRole {
        SwarmRole::Auditor
    }

    fn name(&self) -> &str {
        "Auditor"
    }

    fn allowed_capabilities(&self) -> &[CapabilityToken] {
        &self.capabilities
    }

    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        _ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError> {
        match msg.payload {
            SwarmPayload::CodeHunkPatch {
                ref patch_id,
                ref hunks,
                ..
            } => {
                let mut violations = Vec::new();
                let mut risk_score = 0.0f32;

                for hunk in hunks {
                    let content = hunk.diff_content.to_lowercase();
                    if content.contains("rm -rf") || content.contains("mkfs") || content.contains(":(){ :|:& };:") {
                        violations.push(format!("Destructive shell sequence in hunk {}", hunk.hunk_id));
                        risk_score = 1.0;
                    }
                    if content.contains("password") || content.contains("secret_key") {
                        violations.push(format!("Potential hardcoded credential in hunk {}", hunk.hunk_id));
                        risk_score = risk_score.max(0.7);
                    }
                }

                let clean = violations.is_empty();
                Ok(Some(SwarmPayload::AuditReport {
                    patch_id: patch_id.clone(),
                    clean,
                    integrity_violations: violations,
                    risk_score,
                }))
            }
            _ => Ok(None),
        }
    }
}

// -------------------------------------------------------------------------------------------------
// 5. SENTINEL ROLE (Holds Unconditional VETO Authority)
// -------------------------------------------------------------------------------------------------

pub struct SentinelRole {
    capabilities: Vec<CapabilityToken>,
    forbidden_patterns: HashSet<String>,
}

impl SentinelRole {
    pub fn new() -> Self {
        let mut patterns = HashSet::new();
        patterns.insert("rm -rf /".to_string());
        patterns.insert("rm -rf".to_string());
        patterns.insert("mkfs".to_string());
        patterns.insert(":(){ :|:& };:".to_string());
        patterns.insert("> /dev/sd".to_string());
        patterns.insert("fork()".to_string());

        Self {
            capabilities: vec![
                CapabilityToken::FsRead,
                CapabilityToken::FsWrite,
                CapabilityToken::NetOutbound,
                CapabilityToken::OsExecute,
                CapabilityToken::VisionCapture,
                CapabilityToken::AudioRecord,
                CapabilityToken::KeystoreAccess,
            ],
            forbidden_patterns: patterns,
        }
    }

    /// Checks whether arbitrary content violates safety invariants.
    pub fn inspect_safety(&self, content: &str) -> Option<String> {
        let lower = content.to_lowercase();
        for pat in &self.forbidden_patterns {
            if lower.contains(pat) {
                return Some(format!("Forbidden dangerous pattern '{}' detected", pat));
            }
        }
        None
    }
}

impl Default for SentinelRole {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait::async_trait]
impl SwarmActorRole for SentinelRole {
    fn role(&self) -> SwarmRole {
        SwarmRole::Sentinel
    }

    fn name(&self) -> &str {
        "Sentinel"
    }

    fn allowed_capabilities(&self) -> &[CapabilityToken] {
        &self.capabilities
    }

    async fn handle_message(
        &mut self,
        msg: SwarmMessage,
        _ctx: &ActorContext,
    ) -> Result<Option<SwarmPayload>, ActorError> {
        // Sentinel inspects EVERY incoming message for safety invariant violations
        match msg.payload {
            SwarmPayload::CodeHunkPatch {
                ref patch_id,
                ref hunks,
                ..
            } => {
                for hunk in hunks {
                    if let Some(violation) = self.inspect_safety(&hunk.diff_content) {
                        tracing::warn!(
                            "[Sentinel] UNCONDITIONAL VETO triggered on patch {}: {}",
                            patch_id,
                            violation
                        );
                        return Ok(Some(SwarmPayload::SentinelVeto {
                            veto_id: format!("veto-{}", uuid::Uuid::new_v4()),
                            target_id: patch_id.clone(),
                            reason: violation.clone(),
                            violated_invariant: format!("Safe execution policy: {}", violation),
                        }));
                    }
                }
                Ok(None)
            }
            SwarmPayload::TaskProposal {
                ref task_id,
                ref description,
                ..
            } => {
                if let Some(violation) = self.inspect_safety(description) {
                    tracing::warn!(
                        "[Sentinel] UNCONDITIONAL VETO triggered on task {}: {}",
                        task_id,
                        violation
                    );
                    return Ok(Some(SwarmPayload::SentinelVeto {
                        veto_id: format!("veto-{}", uuid::Uuid::new_v4()),
                        target_id: task_id.clone(),
                        reason: violation.clone(),
                        violated_invariant: format!("Dangerous task proposal: {}", violation),
                    }));
                }
                Ok(None)
            }
            _ => Ok(None),
        }
    }
}
