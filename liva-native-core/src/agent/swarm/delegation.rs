//! Subagent Task Delegation, Budget Tracking & Capability Attenuation Engine
//!
//! Enforces hierarchical subagent governance:
//! - Recursion depth limit (current_depth < max_depth) to prevent recursive fork bombs.
//! - Capability attenuation (child_caps ⊆ parent_caps) enforcing Principle of Least Privilege.
//! - Bounded token, step, and wall-clock execution budgets with refund mechanisms.
//! - Verifiable ancestry lineage tracking.
//! - Direct synthesis into Tier-1 and Tier-2 `SandboxPolicy`.

use super::types::SwarmRole;
use crate::sandbox::policy::{CapabilityToken, SandboxPolicy};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;

/// Default maximum subagent recursion depth.
pub const DEFAULT_MAX_DELEGATION_DEPTH: usize = 3;

/// Resource budget allocated for a delegated execution subtree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBudget {
    /// Maximum LLM inference tokens allowed (prompt + completion).
    pub allocated_tokens: u64,
    /// Remaining LLM tokens available.
    pub remaining_tokens: u64,
    /// Maximum execution steps / tool invocations allowed.
    pub allocated_steps: usize,
    /// Remaining execution steps available.
    pub remaining_steps: usize,
    /// Maximum execution duration in milliseconds.
    pub max_duration_ms: u64,
    /// Timestamp when budget was created.
    pub created_at_ms: u64,
    /// Hard deadline timestamp in milliseconds.
    pub deadline_ms: u64,
}

impl ResourceBudget {
    pub fn new(tokens: u64, steps: usize, max_duration_ms: u64, now_ms: u64) -> Self {
        Self {
            allocated_tokens: tokens,
            remaining_tokens: tokens,
            allocated_steps: steps,
            remaining_steps: steps,
            max_duration_ms,
            created_at_ms: now_ms,
            deadline_ms: now_ms.saturating_add(max_duration_ms),
        }
    }

    pub fn is_expired(&self, now_ms: u64) -> bool {
        now_ms > self.deadline_ms
    }

    pub fn deduct_tokens(&mut self, amount: u64) -> Result<(), DelegationError> {
        if amount > self.remaining_tokens {
            return Err(DelegationError::TokenBudgetExhausted {
                attempted: amount,
                remaining: self.remaining_tokens,
            });
        }
        self.remaining_tokens -= amount;
        Ok(())
    }

    pub fn deduct_step(&mut self) -> Result<(), DelegationError> {
        if self.remaining_steps == 0 {
            return Err(DelegationError::StepBudgetExhausted {
                attempted: 1,
                remaining: 0,
            });
        }
        self.remaining_steps -= 1;
        Ok(())
    }

    pub fn refund(&mut self, tokens: u64, steps: usize) {
        self.remaining_tokens = (self.remaining_tokens + tokens).min(self.allocated_tokens);
        self.remaining_steps = (self.remaining_steps + steps).min(self.allocated_steps);
    }
}

/// A recorded hop in the delegation lineage tree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationHop {
    pub hop_index: usize,
    pub delegator_id: String,
    pub delegator_role: SwarmRole,
    pub assignee_id: String,
    pub assignee_role: SwarmRole,
    pub task_id: String,
    pub task_summary: String,
    pub timestamp_ms: u64,
}

/// Errors occurring during delegation and budget enforcement.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DelegationError {
    #[error("Maximum delegation recursion depth exceeded: depth {current_depth} >= max {max_depth}")]
    MaxDepthExceeded {
        current_depth: usize,
        max_depth: usize,
    },

    #[error("Privilege escalation attempt: requested capabilities {unauthorized:?} are not granted to parent")]
    PrivilegeEscalationAttempt {
        unauthorized: Vec<CapabilityToken>,
    },

    #[error("Insufficient token budget: requested {requested}, parent only has {available} remaining")]
    InsufficientTokenBudget {
        requested: u64,
        available: u64,
    },

    #[error("Insufficient step budget: requested {requested}, parent only has {available} remaining")]
    InsufficientStepBudget {
        requested: usize,
        available: usize,
    },

    #[error("Token budget exhausted: attempted spend {attempted}, remaining {remaining}")]
    TokenBudgetExhausted {
        attempted: u64,
        remaining: u64,
    },

    #[error("Step budget exhausted: attempted spend {attempted}, remaining {remaining}")]
    StepBudgetExhausted {
        attempted: usize,
        remaining: usize,
    },

    #[error("Execution deadline expired: deadline was {deadline_ms}, current time is {current_ms}")]
    DeadlineExpired {
        deadline_ms: u64,
        current_ms: u64,
    },

    #[error("Delegation token '{0}' has been revoked")]
    TokenRevoked(String),

    #[error("Invalid token lineage: {0}")]
    InvalidLineage(String),

    #[error("Agent ID mismatch: token authorized for '{authorized_to}', presented by '{presented_by}'")]
    AgentMismatch {
        authorized_to: String,
        presented_by: String,
    },
}

/// Hierarchical Delegation Token granting verified authority to an actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationToken {
    pub token_id: String,
    pub root_agent_id: String,
    pub parent_agent_id: String,
    pub delegated_to_agent_id: String,
    pub delegated_to_role: SwarmRole,
    pub task_id: String,
    pub current_depth: usize,
    pub max_depth: usize,
    pub budget: ResourceBudget,
    pub allowed_capabilities: HashSet<CapabilityToken>,
    pub lineage: Vec<DelegationHop>,
    pub is_revoked: bool,
}

impl DelegationToken {
    /// Creates the root delegation token for the swarm master orchestrator.
    pub fn create_root(
        root_agent_id: impl Into<String>,
        root_role: SwarmRole,
        task_id: impl Into<String>,
        max_depth: usize,
        total_tokens: u64,
        total_steps: usize,
        max_duration_ms: u64,
        capabilities: HashSet<CapabilityToken>,
        now_ms: u64,
    ) -> Self {
        let root_id = root_agent_id.into();
        let t_id = task_id.into();
        let token_id = format!("dtk-root-{}", uuid::Uuid::new_v4());

        let hop = DelegationHop {
            hop_index: 0,
            delegator_id: "system".to_string(),
            delegator_role: SwarmRole::Sentinel,
            assignee_id: root_id.clone(),
            assignee_role: root_role,
            task_id: t_id.clone(),
            task_summary: "Root Orchestration Goal".to_string(),
            timestamp_ms: now_ms,
        };

        Self {
            token_id,
            root_agent_id: root_id.clone(),
            parent_agent_id: "system".to_string(),
            delegated_to_agent_id: root_id,
            delegated_to_role: root_role,
            task_id: t_id,
            current_depth: 0,
            max_depth,
            budget: ResourceBudget::new(total_tokens, total_steps, max_duration_ms, now_ms),
            allowed_capabilities: capabilities,
            lineage: vec![hop],
            is_revoked: false,
        }
    }

    /// Spawns a child subagent token with capability attenuation and sub-budget deduction.
    pub fn sub_delegate(
        &mut self,
        to_agent_id: impl Into<String>,
        to_role: SwarmRole,
        sub_task_id: impl Into<String>,
        task_summary: impl Into<String>,
        sub_budget_tokens: u64,
        sub_steps: usize,
        requested_capabilities: HashSet<CapabilityToken>,
        duration_ms: u64,
        now_ms: u64,
    ) -> Result<Self, DelegationError> {
        if self.is_revoked {
            return Err(DelegationError::TokenRevoked(self.token_id.clone()));
        }

        if self.budget.is_expired(now_ms) {
            return Err(DelegationError::DeadlineExpired {
                deadline_ms: self.budget.deadline_ms,
                current_ms: now_ms,
            });
        }

        // 1. Enforce recursion limit
        let next_depth = self.current_depth + 1;
        if next_depth >= self.max_depth {
            return Err(DelegationError::MaxDepthExceeded {
                current_depth: next_depth,
                max_depth: self.max_depth,
            });
        }

        // 2. Capability Attenuation (Principle of Least Privilege)
        let unauthorized: Vec<CapabilityToken> = requested_capabilities
            .iter()
            .filter(|c| !self.allowed_capabilities.contains(c))
            .copied()
            .collect();

        if !unauthorized.is_empty() {
            return Err(DelegationError::PrivilegeEscalationAttempt { unauthorized });
        }

        // 3. Deduct token & step budget from parent
        if sub_budget_tokens > self.budget.remaining_tokens {
            return Err(DelegationError::InsufficientTokenBudget {
                requested: sub_budget_tokens,
                available: self.budget.remaining_tokens,
            });
        }
        if sub_steps > self.budget.remaining_steps {
            return Err(DelegationError::InsufficientStepBudget {
                requested: sub_steps,
                available: self.budget.remaining_steps,
            });
        }

        self.budget.remaining_tokens -= sub_budget_tokens;
        self.budget.remaining_steps -= sub_steps;

        let to_id = to_agent_id.into();
        let sub_t_id = sub_task_id.into();
        let sub_summary = task_summary.into();

        // Effective child duration cannot exceed parent remaining deadline
        let parent_remaining_ms = self.budget.deadline_ms.saturating_sub(now_ms);
        let effective_duration_ms = duration_ms.min(parent_remaining_ms);

        let child_hop = DelegationHop {
            hop_index: self.lineage.len(),
            delegator_id: self.delegated_to_agent_id.clone(),
            delegator_role: self.delegated_to_role,
            assignee_id: to_id.clone(),
            assignee_role: to_role,
            task_id: sub_t_id.clone(),
            task_summary: sub_summary,
            timestamp_ms: now_ms,
        };

        let mut child_lineage = self.lineage.clone();
        child_lineage.push(child_hop);

        Ok(Self {
            token_id: format!("dtk-sub-{}", uuid::Uuid::new_v4()),
            root_agent_id: self.root_agent_id.clone(),
            parent_agent_id: self.delegated_to_agent_id.clone(),
            delegated_to_agent_id: to_id,
            delegated_to_role: to_role,
            task_id: sub_t_id,
            current_depth: next_depth,
            max_depth: self.max_depth,
            budget: ResourceBudget::new(sub_budget_tokens, sub_steps, effective_duration_ms, now_ms),
            allowed_capabilities: requested_capabilities,
            lineage: child_lineage,
            is_revoked: false,
        })
    }

    /// Spends tokens from the current token's remaining budget.
    pub fn spend_tokens(&mut self, amount: u64, now_ms: u64) -> Result<(), DelegationError> {
        if self.is_revoked {
            return Err(DelegationError::TokenRevoked(self.token_id.clone()));
        }
        if self.budget.is_expired(now_ms) {
            return Err(DelegationError::DeadlineExpired {
                deadline_ms: self.budget.deadline_ms,
                current_ms: now_ms,
            });
        }
        self.budget.deduct_tokens(amount)
    }

    /// Spends 1 execution step from the current token's remaining budget.
    pub fn spend_step(&mut self, now_ms: u64) -> Result<(), DelegationError> {
        if self.is_revoked {
            return Err(DelegationError::TokenRevoked(self.token_id.clone()));
        }
        if self.budget.is_expired(now_ms) {
            return Err(DelegationError::DeadlineExpired {
                deadline_ms: self.budget.deadline_ms,
                current_ms: now_ms,
            });
        }
        self.budget.deduct_step()
    }

    /// Refunds unused resources back into this token (e.g. from completed child).
    pub fn refund_unused(&mut self, unused_tokens: u64, unused_steps: usize) {
        self.budget.refund(unused_tokens, unused_steps);
    }

    /// Revokes this token, immediately halting subagent execution.
    pub fn revoke(&mut self) {
        self.is_revoked = true;
    }

    /// Converts delegation token capabilities and parameters into a SandboxPolicy.
    pub fn to_sandbox_policy(&self, workspace_root: PathBuf) -> SandboxPolicy {
        SandboxPolicy {
            capabilities: self.allowed_capabilities.clone(),
            workspace_root: Some(workspace_root.clone()),
            allowed_read_paths: vec![workspace_root.clone()],
            allowed_write_paths: if self.allowed_capabilities.contains(&CapabilityToken::FsWrite) {
                vec![workspace_root]
            } else {
                vec![]
            },
            max_execution_time_secs: (self.budget.max_duration_ms / 1000).max(1),
            ..SandboxPolicy::default()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_root_and_sub_delegation() {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);
        caps.insert(CapabilityToken::FsWrite);
        caps.insert(CapabilityToken::OsExecute);

        let mut root = DelegationToken::create_root(
            "orch_root",
            SwarmRole::Planner,
            "task_0",
            3,
            10_000,
            50,
            60_000,
            caps.clone(),
            1000,
        );

        assert_eq!(root.current_depth, 0);
        assert_eq!(root.budget.remaining_tokens, 10_000);
        assert_eq!(root.budget.remaining_steps, 50);

        // Subdelegate to Coder
        let mut child_caps = HashSet::new();
        child_caps.insert(CapabilityToken::FsRead);
        child_caps.insert(CapabilityToken::FsWrite);

        let mut child = root
            .sub_delegate(
                "coder_1",
                SwarmRole::Coder,
                "task_1",
                "Write feature code",
                3_000,
                15,
                child_caps,
                30_000,
                1050,
            )
            .expect("sub_delegate");

        assert_eq!(root.budget.remaining_tokens, 7_000);
        assert_eq!(root.budget.remaining_steps, 35);
        assert_eq!(child.current_depth, 1);
        assert_eq!(child.budget.remaining_tokens, 3_000);
        assert_eq!(child.lineage.len(), 2);

        // Child spends tokens
        child.spend_tokens(500, 1100).unwrap();
        child.spend_step(1100).unwrap();
        assert_eq!(child.budget.remaining_tokens, 2_500);
        assert_eq!(child.budget.remaining_steps, 14);

        // Child completes and refunds remaining to parent
        root.refund_unused(child.budget.remaining_tokens, child.budget.remaining_steps);
        assert_eq!(root.budget.remaining_tokens, 9_500);
        assert_eq!(root.budget.remaining_steps, 49);
    }

    #[test]
    fn test_privilege_escalation_prevention() {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);

        let mut root = DelegationToken::create_root(
            "orch_root",
            SwarmRole::Planner,
            "task_0",
            3,
            5_000,
            20,
            30_000,
            caps,
            1000,
        );

        let mut evil_caps = HashSet::new();
        evil_caps.insert(CapabilityToken::FsRead);
        evil_caps.insert(CapabilityToken::OsExecute); // Not held by parent!

        let err = root
            .sub_delegate(
                "coder_evil",
                SwarmRole::Coder,
                "task_hack",
                "Try exec",
                1_000,
                5,
                evil_caps,
                10_000,
                1050,
            )
            .unwrap_err();

        match err {
            DelegationError::PrivilegeEscalationAttempt { unauthorized } => {
                assert_eq!(unauthorized, vec![CapabilityToken::OsExecute]);
            }
            _ => panic!("Expected PrivilegeEscalationAttempt"),
        }
    }

    #[test]
    fn test_recursion_depth_limit() {
        let mut caps = HashSet::new();
        caps.insert(CapabilityToken::FsRead);

        let mut root = DelegationToken::create_root(
            "root",
            SwarmRole::Planner,
            "task_0",
            2, // Max depth is 2 (depths 0 and 1 allowed)
            5_000,
            20,
            30_000,
            caps.clone(),
            1000,
        );

        // Depth 1: Ok
        let mut d1 = root
            .sub_delegate("a1", SwarmRole::Coder, "t1", "desc", 1000, 5, caps.clone(), 10000, 1100)
            .unwrap();
        assert_eq!(d1.current_depth, 1);

        // Depth 2: Should fail max depth (since 2 >= max_depth 2)
        let err = d1
            .sub_delegate("a2", SwarmRole::Reviewer, "t2", "desc", 500, 2, caps, 5000, 1200)
            .unwrap_err();

        assert!(matches!(err, DelegationError::MaxDepthExceeded { .. }));
    }
}
