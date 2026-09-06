//! Multi-Agent Swarm Orchestration Subsystem (Milestone 1)
//!
//! Provides distributed actor collaboration with:
//! - Actor model and 5 specialized roles (Planner, Coder, Reviewer, Auditor, Sentinel).
//! - 3-tier priority mailboxes (High, Normal, Low) with biased scheduling.
//! - Distributed Quorum Consensus with unconditional Sentinel Veto authority.
//! - Hierarchical subagent task delegation with recursion limits, token budgets, and capability attenuation.
//! - Causal state tracking via Vector Clocks and 3-Way RFC 6902 JSON Patch state merging.
//! - MVCC transaction coordination with optimistic concurrency control.

pub mod actor;
pub mod conflict;
pub mod consensus;
pub mod delegation;
pub mod mailbox;
pub mod merge;
pub mod mvcc;
pub mod pool;
pub mod roles;
pub mod types;
pub mod vector_clock;

pub use actor::{ActorContext, ActorHandle, SwarmActor};
pub use conflict::{ConflictItem, ConflictResolutionStrategy, ConflictType, MergeResult};
pub use consensus::{
    ConsensusError, ConsensusOutcome, ConsensusRule, ConsensusStatus, ProposalSession,
    VoteBallot, VoteDecision,
};
pub use delegation::{
    DelegationError, DelegationHop, DelegationToken, ResourceBudget, DEFAULT_MAX_DELEGATION_DEPTH,
};
pub use mailbox::{
    create_priority_mailbox, default_priority_mailbox, PriorityMailboxReceiver,
    PriorityMailboxSender, DEFAULT_HIGH_CAPACITY, DEFAULT_LOW_CAPACITY, DEFAULT_NORMAL_CAPACITY,
};
pub use merge::ThreeWayMerger;
pub use mvcc::{MvccCommitResult, MvccError, MvccTransactionCoordinator};
pub use pool::AgentActorPool;
pub use roles::{
    AuditorRole, CoderRole, PlannerRole, ReviewerRole, SentinelRole, SwarmActorRole,
};
pub use types::{
    ActorError, ActorStatus, DiffHunk, HunkStatus, MessagePriority, SwarmMessage, SwarmPayload,
    SwarmRole,
};
pub use vector_clock::{CausalRelation, VectorClock};

use crate::agent::graph::pregel::{LivaAgentRuntime, NodeError};
use crate::agent::state::AgentState;
use crate::sandbox::policy::CapabilityToken;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Central Swarm Orchestrator facade managing multi-agent collaboration.
pub struct SwarmOrchestrator {
    pub pool: Arc<tokio::sync::Mutex<AgentActorPool>>,
    pub dispatcher_tx: mpsc::Sender<SwarmMessage>,
    pub root_clock: Arc<tokio::sync::Mutex<VectorClock>>,
}

impl SwarmOrchestrator {
    /// Creates a SwarmOrchestrator from an existing pool and dispatcher sender.
    pub fn new(pool: AgentActorPool, dispatcher_tx: mpsc::Sender<SwarmMessage>) -> Self {
        Self {
            pool: Arc::new(tokio::sync::Mutex::new(pool)),
            dispatcher_tx,
            root_clock: Arc::new(tokio::sync::Mutex::new(VectorClock::new())),
        }
    }

    /// Bootstraps a standard 5-role swarm with default actors spawned in background tasks.
    pub async fn bootstrap_standard_swarm() -> Self {
        let (pool, dispatcher_tx) = AgentActorPool::new();

        // 1. Planner
        let (p_tx, p_rx) = default_priority_mailbox();
        let planner_actor = SwarmActor::new(
            "actor-planner-1",
            Box::new(PlannerRole::new()),
            p_tx,
            p_rx,
            dispatcher_tx.clone(),
        );
        pool.register(planner_actor.spawn()).await;

        // 2. Coder
        let (c_tx, c_rx) = default_priority_mailbox();
        let coder_actor = SwarmActor::new(
            "actor-coder-1",
            Box::new(CoderRole::new()),
            c_tx,
            c_rx,
            dispatcher_tx.clone(),
        );
        pool.register(coder_actor.spawn()).await;

        // 3. Reviewer
        let (r_tx, r_rx) = default_priority_mailbox();
        let reviewer_actor = SwarmActor::new(
            "actor-reviewer-1",
            Box::new(ReviewerRole::new()),
            r_tx,
            r_rx,
            dispatcher_tx.clone(),
        );
        pool.register(reviewer_actor.spawn()).await;

        // 4. Auditor
        let (a_tx, a_rx) = default_priority_mailbox();
        let auditor_actor = SwarmActor::new(
            "actor-auditor-1",
            Box::new(AuditorRole::new()),
            a_tx,
            a_rx,
            dispatcher_tx.clone(),
        );
        pool.register(auditor_actor.spawn()).await;

        // 5. Sentinel
        let (s_tx, s_rx) = default_priority_mailbox();
        let sentinel_actor = SwarmActor::new(
            "actor-sentinel-1",
            Box::new(SentinelRole::new()),
            s_tx,
            s_rx,
            dispatcher_tx.clone(),
        );
        pool.register(sentinel_actor.spawn()).await;

        Self::new(pool, dispatcher_tx)
    }

    /// Dispatches a structured message to the swarm.
    pub async fn dispatch(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        let pool = self.pool.lock().await;
        pool.dispatch(msg).await
    }

    /// Broadcasts a message to all actors in the swarm.
    pub async fn broadcast(&self, msg: SwarmMessage) -> Result<(), ActorError> {
        let pool = self.pool.lock().await;
        pool.broadcast(msg).await
    }

    /// Proposes a new top-level goal to the swarm with initial task proposal payload.
    pub async fn propose_task(
        &self,
        goal: &str,
        description: &str,
        assigned_to: Option<SwarmRole>,
        budget: u64,
        capabilities: Vec<CapabilityToken>,
    ) -> Result<SwarmMessage, ActorError> {
        let mut clock = self.root_clock.lock().await;
        clock.tick("orchestrator");

        let task_id = format!("task-{}", uuid::Uuid::new_v4());
        let payload = SwarmPayload::TaskProposal {
            task_id,
            goal: goal.to_string(),
            description: description.to_string(),
            required_capabilities: capabilities,
            assigned_to,
            budget_tokens: budget,
        };

        let msg = SwarmMessage::new(
            SwarmRole::Planner,
            assigned_to,
            MessagePriority::Normal,
            payload,
            clock.clone(),
        );

        self.dispatch(msg.clone()).await?;
        Ok(msg)
    }

    /// Creates a new distributed consensus session.
    pub fn create_consensus_session(
        &self,
        proposal_id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        proposer_id: impl Into<String>,
        proposer_role: SwarmRole,
        rule: ConsensusRule,
        eligible_voters: HashMap<String, SwarmRole>,
        payload_digest: impl Into<String>,
        now_ms: u64,
        timeout_duration_ms: u64,
    ) -> Result<ProposalSession, ConsensusError> {
        ProposalSession::new(
            proposal_id,
            title,
            description,
            proposer_id,
            proposer_role,
            rule,
            eligible_voters,
            payload_digest,
            now_ms,
            timeout_duration_ms,
        )
    }

    /// Gracefully shuts down the swarm actor pool.
    pub async fn shutdown(&self) -> Result<(), ActorError> {
        let mut pool = self.pool.lock().await;
        pool.shutdown().await
    }
}

/// Registers swarm orchestration execution nodes into a Pregel `LivaAgentRuntime<AgentState>`.
pub fn register_swarm_graph_nodes(
    runtime: &mut LivaAgentRuntime<AgentState>,
    orchestrator: Arc<SwarmOrchestrator>,
) {
    let orch1 = Arc::clone(&orchestrator);
    runtime.add_node("swarm_planner", move |mut state: AgentState| {
        let orch = Arc::clone(&orch1);
        async move {
            let task_goal = state
                .scratchpad_get("goal")
                .and_then(|v| v.as_str())
                .unwrap_or("Execute swarm orchestration task")
                .to_string();

            let _ = orch
                .propose_task(
                    &task_goal,
                    &task_goal,
                    Some(SwarmRole::Coder),
                    10_000,
                    vec![CapabilityToken::FsRead, CapabilityToken::FsWrite],
                )
                .await
                .map_err(|e| NodeError::Fatal(format!("Swarm planning failed: {}", e)))?;

            state.record_node_visit("swarm_planner");
            state.current_node = "swarm_execute".to_string();
            Ok(state)
        }
    });

    let _orch2 = Arc::clone(&orchestrator);
    runtime.add_node("swarm_execute", move |mut state: AgentState| {
        async move {
            state.record_node_visit("swarm_execute");
            state.current_node = "__END__".to_string();
            Ok(state)
        }
    });

    runtime.set_entry_point("swarm_planner");
    runtime.add_edge("swarm_planner", "swarm_execute");
    runtime.add_edge("swarm_execute", "__END__");
}
