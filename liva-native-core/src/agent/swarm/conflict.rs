//! Conflict Taxonomy & Resolution Strategy Definitions for State Merging
//!
//! Categorizes all possible 3-way RFC 6902 JSON Patch collisions and provides
//! automated, role-aware, and manual HITL conflict resolution policies.

use crate::agent::graph::checkpoint::JsonPatchOp;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// Type of state conflict detected during 3-way merge.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConflictType {
    /// Both branches modified the same scalar value to different targets.
    ValueMismatch,
    /// Local branch deleted path while remote branch modified it.
    DeleteModify,
    /// Local branch modified path while remote branch deleted it.
    ModifyDelete,
    /// Parent container was deleted/replaced while child property was modified.
    StructuralOverwrite,
    /// Conflicting mutations on array indices or items.
    ArrayCollision,
}

/// Detailed descriptor for an individual conflicted path.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ConflictItem {
    pub path: String,
    pub conflict_type: ConflictType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub our_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub their_value: Option<Value>,
    pub our_op: JsonPatchOp,
    pub their_op: JsonPatchOp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolved_value: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resolution_applied: Option<String>,
}

/// Configurable automated and manual conflict resolution strategies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum ConflictResolutionStrategy {
    /// Last-Writer-Wins based on physical wall clock or incoming remote update.
    LastWriterWins,
    /// Local (Ours) branch unconditionally takes precedence.
    PreferOurs,
    /// Remote (Theirs) branch unconditionally takes precedence.
    PreferTheirs,
    /// Deep recursive merge on objects; LWW on leaf scalar conflicts.
    DeepMergeLww,
    /// Sentinel Role Authority: if Sentinel modified the value, Sentinel wins unconditionally.
    SentinelAuthority { sentinel_role_name: String },
    /// Custom path-scoped strategy routing (e.g. "/messages" -> DeepMergeLww, "/active_plan" -> PreferTheirs).
    PathScoped(HashMap<String, Box<ConflictResolutionStrategy>>),
    /// Human-In-The-Loop: marks conflict as unresolvable, yielding for user or Sentinel approval.
    ManualHitl,
    /// Fail immediately on any conflict, triggering transactional rollback.
    FailFast,
}

impl Default for ConflictResolutionStrategy {
    fn default() -> Self {
        Self::DeepMergeLww
    }
}

/// Complete result of a 3-way merge execution.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MergeResult {
    /// Fully resolved JSON state.
    pub merged_state: Value,
    /// Detailed list of all conflicts encountered during merge.
    pub conflicts: Vec<ConflictItem>,
    /// Whether the merge succeeded cleanly without unresolvable conflicts.
    pub is_clean: bool,
    /// Number of automatically resolved conflicts.
    pub auto_resolved_count: usize,
    /// Number of unresolvable conflicts requiring manual decision / rollback.
    pub unresolvable_count: usize,
}
