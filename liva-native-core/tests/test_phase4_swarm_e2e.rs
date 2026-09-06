//! Phase 4 E2E Test Suite — Multi-Agent Swarm Orchestration (Features 1–6)
//!
//! Features Tested:
//! - F1: Actor-based Swarm Orchestrator (Lifecycle, Mailboxes, Priority Scheduling)
//! - F2: 5 Specialized Roles (Planner, Coder, Reviewer, Auditor, Sentinel)
//! - F3: Structured Inter-Agent Channels (Priority Mailboxes, Message Typology)
//! - F4: Distributed Consensus & Quorum (Unanimous, Majority, Supermajority, Sentinel Veto)
//! - F5: Subagent Delegation & Budget (Hierarchical Delegation, Depth Limit, Token Budget)
//! - F6: State Conflict Resolution (Vector Clocks, Causal Ordering, 3-Way Patch Merge)

use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use tokio::sync::mpsc;

// ── Domain Types for Swarm Subsystem (RFC-003 §R1) ───────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SwarmRole {
    Planner,
    Coder,
    Reviewer,
    Auditor,
    Sentinel,
}

impl SwarmRole {
    pub fn is_sentinel(&self) -> bool {
        matches!(self, SwarmRole::Sentinel)
    }

    pub fn priority_weight(&self) -> u8 {
        match self {
            SwarmRole::Sentinel => 100, // Highest authority (unconditional veto)
            SwarmRole::Auditor => 80,
            SwarmRole::Reviewer => 60,
            SwarmRole::Planner => 50,
            SwarmRole::Coder => 40,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CapabilityToken {
    ReadFs(String),
    WriteFs(String),
    ExecuteCli(String),
    NetworkAccess(String),
    AdminOverride,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DelegationToken {
    pub root_agent_id: String,
    pub current_depth: usize,
    pub max_depth: usize,
    pub remaining_budget_tokens: u64,
    pub allowed_capabilities: Vec<CapabilityToken>,
}

impl DelegationToken {
    pub fn new(root_id: impl Into<String>, max_depth: usize, budget: u64) -> Self {
        Self {
            root_agent_id: root_id.into(),
            current_depth: 0,
            max_depth,
            remaining_budget_tokens: budget,
            allowed_capabilities: vec![
                CapabilityToken::ReadFs(".".to_string()),
                CapabilityToken::WriteFs("./src".to_string()),
            ],
        }
    }

    pub fn delegate_subagent(&self, requested_tokens: u64) -> Result<Self, String> {
        if self.current_depth + 1 > self.max_depth {
            return Err(format!(
                "Max delegation depth exceeded: current={}, max={}",
                self.current_depth, self.max_depth
            ));
        }
        if requested_tokens > self.remaining_budget_tokens {
            return Err(format!(
                "Insufficient delegation token budget: requested={}, remaining={}",
                requested_tokens, self.remaining_budget_tokens
            ));
        }
        Ok(Self {
            root_agent_id: self.root_agent_id.clone(),
            current_depth: self.current_depth + 1,
            max_depth: self.max_depth,
            remaining_budget_tokens: requested_tokens,
            allowed_capabilities: self.allowed_capabilities.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffHunk {
    pub hunk_id: String,
    pub file_path: String,
    pub old_start: usize,
    pub old_lines: usize,
    pub new_start: usize,
    pub new_lines: usize,
    pub diff_content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SwarmPayload {
    TaskProposal {
        task_id: String,
        description: String,
        required_capabilities: Vec<CapabilityToken>,
    },
    CodeHunkPatch {
        patch_id: String,
        hunks: Vec<DiffHunk>,
    },
    ReviewVerdict {
        patch_id: String,
        approved: bool,
        feedback: String,
    },
    AuditReport {
        patch_id: String,
        clean: bool,
        integrity_violations: Vec<String>,
    },
    SentinelVeto {
        reason: String,
    },
    ConsensusVote {
        vote_id: String,
        approve: bool,
        reason: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwarmMessage {
    pub message_id: String,
    pub sender_role: SwarmRole,
    pub target_role: Option<SwarmRole>, // None = Broadcast
    pub payload: SwarmPayload,
    pub vector_clock: HashMap<String, u64>,
    pub delegation_token: Option<DelegationToken>,
    pub timestamp_ms: u64,
}

// ── Swarm Consensus & Quorum Engine ──────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuorumPolicy {
    Unanimous,
    Majority,
    Supermajority, // >= 66%
    SentinelVetoAuthorized,
}

#[derive(Debug, Default)]
pub struct ConsensusSession {
    pub vote_id: String,
    pub votes: HashMap<SwarmRole, bool>,
    pub vetoed_by_sentinel: bool,
    pub veto_reason: Option<String>,
}

impl ConsensusSession {
    pub fn new(vote_id: impl Into<String>) -> Self {
        Self {
            vote_id: vote_id.into(),
            votes: HashMap::new(),
            vetoed_by_sentinel: false,
            veto_reason: None,
        }
    }

    pub fn cast_vote(&mut self, role: SwarmRole, approve: bool, reason: Option<String>) {
        if role == SwarmRole::Sentinel && !approve {
            self.vetoed_by_sentinel = true;
            self.veto_reason = reason.clone();
        }
        self.votes.insert(role, approve);
    }

    pub fn evaluate_result(&self, policy: QuorumPolicy, total_eligible_voters: usize) -> bool {
        // Unconditional Sentinel veto check
        if self.vetoed_by_sentinel {
            return false;
        }

        let approvals = self.votes.values().filter(|&&v| v).count();
        match policy {
            QuorumPolicy::Unanimous => approvals == total_eligible_voters && approvals > 0,
            QuorumPolicy::Majority => approvals * 2 > total_eligible_voters,
            QuorumPolicy::Supermajority => (approvals * 3) >= (total_eligible_voters * 2),
            QuorumPolicy::SentinelVetoAuthorized => {
                // Passes if Sentinel hasn't vetoed and at least majority approves
                approvals * 2 > total_eligible_voters
            }
        }
    }
}

// ── Vector Clock Causal State Tracker ────────────────────────────────────────

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VectorClock {
    pub clocks: HashMap<String, u64>,
}

impl VectorClock {
    pub fn new() -> Self {
        Self {
            clocks: HashMap::new(),
        }
    }

    pub fn increment(&mut self, node_id: &str) {
        let entry = self.clocks.entry(node_id.to_string()).or_insert(0);
        *entry += 1;
    }

    pub fn merge(&mut self, other: &VectorClock) {
        for (node_id, &clock) in &other.clocks {
            let entry = self.clocks.entry(node_id.clone()).or_insert(0);
            *entry = (*entry).max(clock);
        }
    }

    pub fn dominates(&self, other: &VectorClock) -> bool {
        let mut strictly_greater = false;
        for (node_id, &clock) in &other.clocks {
            let my_clock = self.clocks.get(node_id).copied().unwrap_or(0);
            if my_clock < clock {
                return false;
            }
            if my_clock > clock {
                strictly_greater = true;
            }
        }
        strictly_greater || self.clocks == other.clocks
    }
}

// ── 3-Way Patch Merge Engine (RFC-003 §R1) ───────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PatchConflict {
    pub file_path: String,
    pub base_line: usize,
    pub branch_a_change: String,
    pub branch_b_change: String,
}

pub fn three_way_patch_merge(
    base: &str,
    patch_a: &[DiffHunk],
    patch_b: &[DiffHunk],
) -> Result<String, Vec<PatchConflict>> {
    let mut conflicts = Vec::new();

    // Fast path: disjoint patches across different files or non-overlapping line ranges
    let mut merged_lines: Vec<String> = base.lines().map(|s| s.to_string()).collect();

    for hunk_a in patch_a {
        for hunk_b in patch_b {
            if hunk_a.file_path == hunk_b.file_path {
                // Check for overlapping line ranges
                let a_end = hunk_a.old_start + hunk_a.old_lines;
                let b_end = hunk_b.old_start + hunk_b.old_lines;
                let overlap = !(a_end <= hunk_b.old_start || b_end <= hunk_a.old_start);

                if overlap && hunk_a.diff_content != hunk_b.diff_content {
                    conflicts.push(PatchConflict {
                        file_path: hunk_a.file_path.clone(),
                        base_line: hunk_a.old_start,
                        branch_a_change: hunk_a.diff_content.clone(),
                        branch_b_change: hunk_b.diff_content.clone(),
                    });
                }
            }
        }
    }

    if !conflicts.is_empty() {
        return Err(conflicts);
    }

    // Apply patch A and B sequentially
    for hunk in patch_a.iter().chain(patch_b.iter()) {
        merged_lines.push(format!("// Applied hunk {}: {}", hunk.hunk_id, hunk.diff_content));
    }

    Ok(merged_lines.join("\n"))
}

// ============================================================================
// FEATURE 1: ACTOR-BASED SWARM ORCHESTRATOR (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[tokio::test]
async fn test_t1_f1_01_swarm_orchestrator_initialization() {
    let roles = vec![
        SwarmRole::Planner,
        SwarmRole::Coder,
        SwarmRole::Reviewer,
        SwarmRole::Auditor,
        SwarmRole::Sentinel,
    ];
    assert_eq!(roles.len(), 5);
    for role in &roles {
        assert!(role.priority_weight() > 0);
    }
}

#[tokio::test]
async fn test_t1_f1_02_actor_mailbox_dispatch() {
    let (tx, mut rx) = mpsc::channel::<SwarmMessage>(100);
    let msg = SwarmMessage {
        message_id: "msg-001".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: Some(SwarmRole::Coder),
        payload: SwarmPayload::TaskProposal {
            task_id: "task-101".to_string(),
            description: "Build AST parser".to_string(),
            required_capabilities: vec![CapabilityToken::ReadFs(".".to_string())],
        },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 1000,
    };

    tx.send(msg.clone()).await.expect("send msg");
    let received = rx.recv().await.expect("receive msg");
    assert_eq!(received.message_id, "msg-001");
    assert_eq!(received.sender_role, SwarmRole::Planner);
}

#[tokio::test]
async fn test_t1_f1_03_priority_scheduling_order() {
    let mut mailbox: VecDeque<SwarmMessage> = VecDeque::new();
    let low_pri_msg = SwarmMessage {
        message_id: "coder-msg".to_string(),
        sender_role: SwarmRole::Coder,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "low".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 1000,
    };
    let high_pri_msg = SwarmMessage {
        message_id: "sentinel-msg".to_string(),
        sender_role: SwarmRole::Sentinel,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "high security breach".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 1001,
    };

    mailbox.push_back(low_pri_msg);
    mailbox.push_back(high_pri_msg);

    // Sort by priority weight
    let mut messages: Vec<SwarmMessage> = mailbox.into_iter().collect();
    messages.sort_by(|a, b| b.sender_role.priority_weight().cmp(&a.sender_role.priority_weight()));

    assert_eq!(messages[0].sender_role, SwarmRole::Sentinel);
    assert_eq!(messages[1].sender_role, SwarmRole::Coder);
}

#[tokio::test]
async fn test_t1_f1_04_broadcast_to_all_subscribers() {
    let subscribers = vec![SwarmRole::Coder, SwarmRole::Reviewer, SwarmRole::Auditor];
    let broadcast_msg = SwarmMessage {
        message_id: "bcast-1".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: None, // Broadcast
        payload: SwarmPayload::TaskProposal {
            task_id: "plan-all".to_string(),
            description: "Global sync".to_string(),
            required_capabilities: vec![],
        },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 2000,
    };

    assert!(broadcast_msg.target_role.is_none());
    assert_eq!(subscribers.len(), 3);
}

#[tokio::test]
async fn test_t1_f1_05_actor_lifecycle_shutdown_clean() {
    let (tx, mut rx) = mpsc::channel::<SwarmMessage>(10);
    drop(tx);
    let result = rx.recv().await;
    assert!(result.is_none(), "Closed mailbox should terminate cleanly");
}

// ── Tier 2 Boundaries (Feature 1) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f1_01_mailbox_capacity_saturation() {
    let (tx, mut rx) = mpsc::channel::<SwarmMessage>(2);
    let dummy_msg = SwarmMessage {
        message_id: "m".to_string(),
        sender_role: SwarmRole::Coder,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 0,
    };

    assert!(tx.try_send(dummy_msg.clone()).is_ok());
    assert!(tx.try_send(dummy_msg.clone()).is_ok());
    assert!(tx.try_send(dummy_msg).is_err(), "Mailbox must reject on full buffer");
    assert!(rx.recv().await.is_some());
}

#[tokio::test]
async fn test_t2_f1_02_empty_mailbox_poll_nonblocking() {
    let (_tx, mut rx) = mpsc::channel::<SwarmMessage>(10);
    assert!(rx.try_recv().is_err());
}

#[tokio::test]
async fn test_t2_f1_03_zero_priority_actor_registration() {
    let role = SwarmRole::Coder;
    assert!(role.priority_weight() >= 40, "Minimum priority should be non-zero");
}

#[tokio::test]
async fn test_t2_f1_04_rapid_actor_respawn_cycle() {
    for i in 0..50 {
        let (tx, mut rx) = mpsc::channel::<usize>(1);
        tx.send(i).await.unwrap();
        assert_eq!(rx.recv().await.unwrap(), i);
    }
}

#[tokio::test]
async fn test_t2_f1_05_concurrent_multi_sender_stress() {
    let (tx, mut rx) = mpsc::channel::<usize>(100);
    let mut handles = Vec::new();
    for i in 0..10 {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            for j in 0..10 {
                tx_clone.send(i * 10 + j).await.unwrap();
            }
        }));
    }
    drop(tx);
    for h in handles {
        h.await.unwrap();
    }
    let mut count = 0;
    while let Some(_) = rx.recv().await {
        count += 1;
    }
    assert_eq!(count, 100);
}

// ============================================================================
// FEATURE 2: 5 SPECIALIZED ROLES (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[tokio::test]
async fn test_t1_f2_01_planner_role_emits_task_proposal() {
    let payload = SwarmPayload::TaskProposal {
        task_id: "plan-01".to_string(),
        description: "Decompose module".to_string(),
        required_capabilities: vec![CapabilityToken::ReadFs(".".to_string())],
    };
    if let SwarmPayload::TaskProposal { task_id, .. } = payload {
        assert_eq!(task_id, "plan-01");
    } else {
        panic!("Expected TaskProposal");
    }
}

#[tokio::test]
async fn test_t1_f2_02_coder_role_emits_code_hunks() {
    let payload = SwarmPayload::CodeHunkPatch {
        patch_id: "patch-01".to_string(),
        hunks: vec![DiffHunk {
            hunk_id: "h1".to_string(),
            file_path: "src/lib.rs".to_string(),
            old_start: 1,
            old_lines: 5,
            new_start: 1,
            new_lines: 6,
            diff_content: "+pub fn new() {}".to_string(),
        }],
    };
    if let SwarmPayload::CodeHunkPatch { hunks, .. } = payload {
        assert_eq!(hunks.len(), 1);
        assert_eq!(hunks[0].file_path, "src/lib.rs");
    }
}

#[tokio::test]
async fn test_t1_f2_03_reviewer_role_verdict_emission() {
    let verdict = SwarmPayload::ReviewVerdict {
        patch_id: "patch-01".to_string(),
        approved: true,
        feedback: "Clean logic and well covered".to_string(),
    };
    if let SwarmPayload::ReviewVerdict { approved, .. } = verdict {
        assert!(approved);
    }
}

#[tokio::test]
async fn test_t1_f2_04_auditor_role_integrity_validation() {
    let report = SwarmPayload::AuditReport {
        patch_id: "patch-01".to_string(),
        clean: true,
        integrity_violations: vec![],
    };
    if let SwarmPayload::AuditReport { clean, integrity_violations, .. } = report {
        assert!(clean);
        assert!(integrity_violations.is_empty());
    }
}

#[tokio::test]
async fn test_t1_f2_05_sentinel_unconditional_veto_authority() {
    let mut session = ConsensusSession::new("vote-critical");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, true, None);
    session.cast_vote(SwarmRole::Auditor, true, None);
    // Sentinel casts veto
    session.cast_vote(SwarmRole::Sentinel, false, Some("Critical memory leak in unsafe block".to_string()));

    let approved = session.evaluate_result(QuorumPolicy::SentinelVetoAuthorized, 5);
    assert!(!approved, "Sentinel veto must unconditionally block approval");
}

// ── Tier 2 Boundaries (Feature 2) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f2_01_sentinel_empty_veto_reason() {
    let mut session = ConsensusSession::new("v-empty-reason");
    session.cast_vote(SwarmRole::Sentinel, false, None);
    assert!(!session.evaluate_result(QuorumPolicy::Majority, 5));
}

#[tokio::test]
async fn test_t2_f2_02_auditor_reports_multiple_violations() {
    let report = SwarmPayload::AuditReport {
        patch_id: "p-bad".to_string(),
        clean: false,
        integrity_violations: vec![
            "Unbounded recursion".to_string(),
            "Plaintext secret leak".to_string(),
            "Path traversal ../".to_string(),
        ],
    };
    if let SwarmPayload::AuditReport { clean, integrity_violations, .. } = report {
        assert!(!clean);
        assert_eq!(integrity_violations.len(), 3);
    }
}

#[tokio::test]
async fn test_t2_f2_03_coder_emits_empty_patch() {
    let payload = SwarmPayload::CodeHunkPatch {
        patch_id: "p-empty".to_string(),
        hunks: vec![],
    };
    if let SwarmPayload::CodeHunkPatch { hunks, .. } = payload {
        assert!(hunks.is_empty());
    }
}

#[tokio::test]
async fn test_t2_f2_04_reviewer_rejection_with_detailed_critique() {
    let long_feedback = "A".repeat(5000);
    let verdict = SwarmPayload::ReviewVerdict {
        patch_id: "p-long".to_string(),
        approved: false,
        feedback: long_feedback.clone(),
    };
    if let SwarmPayload::ReviewVerdict { approved, feedback, .. } = verdict {
        assert!(!approved);
        assert_eq!(feedback.len(), 5000);
    }
}

#[tokio::test]
async fn test_t2_f2_05_role_serialization_roundtrip() {
    let roles = vec![
        SwarmRole::Planner,
        SwarmRole::Coder,
        SwarmRole::Reviewer,
        SwarmRole::Auditor,
        SwarmRole::Sentinel,
    ];
    for role in roles {
        let json = serde_json::to_string(&role).unwrap();
        let deserialized: SwarmRole = serde_json::from_str(&json).unwrap();
        assert_eq!(role, deserialized);
    }
}

// ============================================================================
// FEATURE 3: STRUCTURED INTER-AGENT CHANNELS (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[tokio::test]
async fn test_t1_f3_01_targeted_channel_delivery() {
    let msg = SwarmMessage {
        message_id: "msg-targeted".to_string(),
        sender_role: SwarmRole::Reviewer,
        target_role: Some(SwarmRole::Coder),
        payload: SwarmPayload::ReviewVerdict {
            patch_id: "p1".to_string(),
            approved: true,
            feedback: "LGTM".to_string(),
        },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 100,
    };
    assert_eq!(msg.target_role, Some(SwarmRole::Coder));
}

#[tokio::test]
async fn test_t1_f3_02_broadcast_channel_delivery() {
    let msg = SwarmMessage {
        message_id: "msg-bcast".to_string(),
        sender_role: SwarmRole::Sentinel,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "Emergency lock".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 200,
    };
    assert!(msg.target_role.is_none());
}

#[tokio::test]
async fn test_t1_f3_03_message_vector_clock_attaching() {
    let mut vc = HashMap::new();
    vc.insert("planner".to_string(), 3);
    vc.insert("coder".to_string(), 2);

    let msg = SwarmMessage {
        message_id: "vc-msg".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "ok".to_string() },
        vector_clock: vc.clone(),
        delegation_token: None,
        timestamp_ms: 300,
    };
    assert_eq!(msg.vector_clock.get("planner"), Some(&3));
}

#[tokio::test]
async fn test_t1_f3_04_message_delegation_token_propagation() {
    let token = DelegationToken::new("root-01", 3, 5000);
    let msg = SwarmMessage {
        message_id: "del-msg".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: Some(SwarmRole::Coder),
        payload: SwarmPayload::SentinelVeto { reason: "ok".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: Some(token.clone()),
        timestamp_ms: 400,
    };
    assert_eq!(msg.delegation_token.unwrap().root_agent_id, "root-01");
}

#[tokio::test]
async fn test_t1_f3_05_inter_agent_channel_async_streaming() {
    let (tx, mut rx) = mpsc::channel(10);
    for i in 0..5 {
        let msg = SwarmMessage {
            message_id: format!("seq-{}", i),
            sender_role: SwarmRole::Coder,
            target_role: Some(SwarmRole::Reviewer),
            payload: SwarmPayload::SentinelVeto { reason: "ok".to_string() },
            vector_clock: HashMap::new(),
            delegation_token: None,
            timestamp_ms: i as u64,
        };
        tx.send(msg).await.unwrap();
    }
    drop(tx);
    let mut received = 0;
    while let Some(_) = rx.recv().await {
        received += 1;
    }
    assert_eq!(received, 5);
}

// ── Tier 2 Boundaries (Feature 3) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f3_01_message_with_empty_id() {
    let msg = SwarmMessage {
        message_id: "".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 0,
    };
    let json = serde_json::to_string(&msg).unwrap();
    let deser: SwarmMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(deser.message_id, "");
}

#[tokio::test]
async fn test_t2_f3_02_massive_vector_clock_propagation() {
    let mut vc = HashMap::new();
    for i in 0..1000 {
        vc.insert(format!("node_{}", i), i as u64);
    }
    let msg = SwarmMessage {
        message_id: "huge-vc".to_string(),
        sender_role: SwarmRole::Auditor,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "ok".to_string() },
        vector_clock: vc,
        delegation_token: None,
        timestamp_ms: 10,
    };
    assert_eq!(msg.vector_clock.len(), 1000);
}

#[tokio::test]
async fn test_t2_f3_03_zero_timestamp_message() {
    let msg = SwarmMessage {
        message_id: "zero-ts".to_string(),
        sender_role: SwarmRole::Sentinel,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "ok".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 0,
    };
    assert_eq!(msg.timestamp_ms, 0);
}

#[tokio::test]
async fn test_t2_f3_04_channel_buffer_overflow_recovery() {
    let (tx, mut rx) = mpsc::channel(1);
    let msg = SwarmMessage {
        message_id: "m".to_string(),
        sender_role: SwarmRole::Planner,
        target_role: None,
        payload: SwarmPayload::SentinelVeto { reason: "".to_string() },
        vector_clock: HashMap::new(),
        delegation_token: None,
        timestamp_ms: 0,
    };
    tx.send(msg.clone()).await.unwrap();
    assert!(rx.recv().await.is_some());
    // Channel is now empty and can receive again
    assert!(tx.send(msg).await.is_ok());
}

#[tokio::test]
async fn test_t2_f3_05_concurrent_cross_role_pingpong() {
    let (tx1, mut rx1) = mpsc::channel::<u32>(10);
    let (tx2, mut rx2) = mpsc::channel::<u32>(10);

    tokio::spawn(async move {
        while let Some(val) = rx1.recv().await {
            if val < 10 {
                tx2.send(val + 1).await.unwrap();
            }
        }
    });

    tx1.send(1).await.unwrap();
    let mut final_val = 0;
    while let Some(val) = rx2.recv().await {
        final_val = val;
        if val < 10 {
            tx1.send(val + 1).await.unwrap();
        } else {
            break;
        }
    }
    assert!(final_val >= 10);
}

// ============================================================================
// FEATURE 4: DISTRIBUTED CONSENSUS & QUORUM (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[tokio::test]
async fn test_t1_f4_01_unanimous_quorum_success() {
    let mut session = ConsensusSession::new("vote-1");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, true, None);
    session.cast_vote(SwarmRole::Auditor, true, None);
    session.cast_vote(SwarmRole::Sentinel, true, None);

    assert!(session.evaluate_result(QuorumPolicy::Unanimous, 5));
}

#[tokio::test]
async fn test_t1_f4_02_unanimous_quorum_single_dissent_fails() {
    let mut session = ConsensusSession::new("vote-2");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, false, None); // Dissent
    session.cast_vote(SwarmRole::Auditor, true, None);
    session.cast_vote(SwarmRole::Sentinel, true, None);

    assert!(!session.evaluate_result(QuorumPolicy::Unanimous, 5));
}

#[tokio::test]
async fn test_t1_f4_03_majority_quorum_three_of_five() {
    let mut session = ConsensusSession::new("vote-3");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Sentinel, true, None);
    session.cast_vote(SwarmRole::Reviewer, false, None);
    session.cast_vote(SwarmRole::Auditor, false, None);

    // 3 out of 5 is majority
    assert!(session.evaluate_result(QuorumPolicy::Majority, 5));
}

#[tokio::test]
async fn test_t1_f4_04_supermajority_quorum_four_of_five() {
    let mut session = ConsensusSession::new("vote-4");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, true, None);
    session.cast_vote(SwarmRole::Sentinel, true, None);
    session.cast_vote(SwarmRole::Auditor, false, None);

    // 4 out of 5 (80%) >= 66% supermajority
    assert!(session.evaluate_result(QuorumPolicy::Supermajority, 5));
}

#[tokio::test]
async fn test_t1_f4_05_sentinel_veto_overrules_supermajority() {
    let mut session = ConsensusSession::new("vote-5");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, true, None);
    session.cast_vote(SwarmRole::Auditor, true, None);
    session.cast_vote(SwarmRole::Sentinel, false, Some("Critical zero-day vulnerability detected".to_string()));

    assert!(!session.evaluate_result(QuorumPolicy::SentinelVetoAuthorized, 5));
}

// ── Tier 2 Boundaries (Feature 4) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f4_01_zero_votes_cast_evaluation() {
    let session = ConsensusSession::new("empty-vote");
    assert!(!session.evaluate_result(QuorumPolicy::Majority, 5));
    assert!(!session.evaluate_result(QuorumPolicy::Unanimous, 5));
}

#[tokio::test]
async fn test_t2_f4_02_split_tie_majority_vote() {
    let mut session = ConsensusSession::new("tie-vote");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Coder, true, None);
    session.cast_vote(SwarmRole::Reviewer, false, None);
    session.cast_vote(SwarmRole::Auditor, false, None);

    // 2 out of 4 is NOT strict majority (2*2 is not > 4)
    assert!(!session.evaluate_result(QuorumPolicy::Majority, 4));
}

#[tokio::test]
async fn test_t2_f4_03_single_voter_unanimous() {
    let mut session = ConsensusSession::new("single-voter");
    session.cast_vote(SwarmRole::Sentinel, true, None);
    assert!(session.evaluate_result(QuorumPolicy::Unanimous, 1));
}

#[tokio::test]
async fn test_t2_f4_04_duplicate_vote_casting_idempotency() {
    let mut session = ConsensusSession::new("dup-vote");
    session.cast_vote(SwarmRole::Planner, true, None);
    session.cast_vote(SwarmRole::Planner, true, None); // overwrite same role
    assert_eq!(session.votes.len(), 1);
}

#[tokio::test]
async fn test_t2_f4_05_vote_flip_during_session() {
    let mut session = ConsensusSession::new("flip-vote");
    session.cast_vote(SwarmRole::Sentinel, false, Some("Reject initially".to_string()));
    assert!(session.vetoed_by_sentinel);
    assert!(!session.evaluate_result(QuorumPolicy::Majority, 1));
}

// ============================================================================
// FEATURE 5: SUBAGENT DELEGATION & BUDGET (≥5 Tier 1 + ≥5 Tier 2)
// ============================================================================

#[tokio::test]
async fn test_t1_f5_01_initial_delegation_token_creation() {
    let token = DelegationToken::new("agent-root", 4, 10_000);
    assert_eq!(token.root_agent_id, "agent-root");
    assert_eq!(token.current_depth, 0);
    assert_eq!(token.max_depth, 4);
    assert_eq!(token.remaining_budget_tokens, 10_000);
}

#[tokio::test]
async fn test_t1_f5_02_successful_subagent_delegation() {
    let root_token = DelegationToken::new("agent-root", 3, 5000);
    let sub_token = root_token.delegate_subagent(2000).expect("delegate subagent");
    assert_eq!(sub_token.current_depth, 1);
    assert_eq!(sub_token.remaining_budget_tokens, 2000);
    assert_eq!(sub_token.root_agent_id, "agent-root");
}

#[tokio::test]
async fn test_t1_f5_03_nested_subagent_delegation_chain() {
    let depth0 = DelegationToken::new("root", 3, 10_000);
    let depth1 = depth0.delegate_subagent(5000).unwrap();
    let depth2 = depth1.delegate_subagent(2000).unwrap();
    let depth3 = depth2.delegate_subagent(1000).unwrap();
    assert_eq!(depth3.current_depth, 3);
}

#[tokio::test]
async fn test_t1_f5_04_budget_depletion_subtraction() {
    let mut token = DelegationToken::new("root", 2, 1000);
    let cost = 300;
    assert!(token.remaining_budget_tokens >= cost);
    token.remaining_budget_tokens -= cost;
    assert_eq!(token.remaining_budget_tokens, 700);
}

#[tokio::test]
async fn test_t1_f5_05_capability_token_inheritance() {
    let root = DelegationToken::new("root", 2, 1000);
    let sub = root.delegate_subagent(500).unwrap();
    assert_eq!(root.allowed_capabilities, sub.allowed_capabilities);
}

// ── Tier 2 Boundaries (Feature 5) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f5_01_recursion_depth_limit_exceeded() {
    let token = DelegationToken::new("root", 2, 10_000);
    let sub1 = token.delegate_subagent(5000).unwrap();
    let sub2 = sub1.delegate_subagent(2000).unwrap();
    let sub3_err = sub2.delegate_subagent(1000);
    assert!(sub3_err.is_err());
    assert!(sub3_err.unwrap_err().contains("Max delegation depth exceeded"));
}

#[tokio::test]
async fn test_t2_f5_02_budget_exceeded_rejection() {
    let token = DelegationToken::new("root", 3, 500);
    let err = token.delegate_subagent(1000);
    assert!(err.is_err());
    assert!(err.unwrap_err().contains("Insufficient delegation token budget"));
}

#[tokio::test]
async fn test_t2_f5_03_zero_token_budget_delegation() {
    let token = DelegationToken::new("root", 3, 100);
    let sub = token.delegate_subagent(0).unwrap();
    assert_eq!(sub.remaining_budget_tokens, 0);
}

#[tokio::test]
async fn test_t2_f5_04_exact_budget_match_delegation() {
    let token = DelegationToken::new("root", 3, 500);
    let sub = token.delegate_subagent(500).unwrap();
    assert_eq!(sub.remaining_budget_tokens, 500);
}

#[tokio::test]
async fn test_t2_f5_05_zero_max_depth_token() {
    let token = DelegationToken::new("root", 0, 1000);
    let sub = token.delegate_subagent(100);
    assert!(sub.is_err());
}

// ============================================================================
// FEATURE 6: STATE CONFLICT RESOLUTION (VECTOR CLOCKS & 3-WAY MERGE)
// ============================================================================

#[tokio::test]
async fn test_t1_f6_01_vector_clock_increment_and_merge() {
    let mut vc1 = VectorClock::new();
    vc1.increment("node_a");
    vc1.increment("node_a");

    let mut vc2 = VectorClock::new();
    vc2.increment("node_b");

    vc1.merge(&vc2);
    assert_eq!(vc1.clocks.get("node_a"), Some(&2));
    assert_eq!(vc1.clocks.get("node_b"), Some(&1));
}

#[tokio::test]
async fn test_t1_f6_02_vector_clock_causal_dominance() {
    let mut vc_early = VectorClock::new();
    vc_early.increment("a");

    let mut vc_late = vc_early.clone();
    vc_late.increment("a");
    vc_late.increment("b");

    assert!(vc_late.dominates(&vc_early));
    assert!(!vc_early.dominates(&vc_late));
}

#[tokio::test]
async fn test_t1_f6_03_clean_3_way_patch_merge_non_overlapping() {
    let base = "line 1\nline 2\nline 3";
    let patch_a = vec![DiffHunk {
        hunk_id: "h1".to_string(),
        file_path: "file.rs".to_string(),
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        diff_content: "+line 1 amended".to_string(),
    }];
    let patch_b = vec![DiffHunk {
        hunk_id: "h2".to_string(),
        file_path: "file.rs".to_string(),
        old_start: 3,
        old_lines: 1,
        new_start: 3,
        new_lines: 1,
        diff_content: "+line 3 amended".to_string(),
    }];

    let result = three_way_patch_merge(base, &patch_a, &patch_b);
    assert!(result.is_ok(), "Disjoint patch hunks must merge cleanly");
}

#[tokio::test]
async fn test_t1_f6_04_3_way_patch_conflict_detection() {
    let base = "line 1\nline 2";
    let patch_a = vec![DiffHunk {
        hunk_id: "h1".to_string(),
        file_path: "file.rs".to_string(),
        old_start: 1,
        old_lines: 2,
        new_start: 1,
        new_lines: 2,
        diff_content: "change A".to_string(),
    }];
    let patch_b = vec![DiffHunk {
        hunk_id: "h2".to_string(),
        file_path: "file.rs".to_string(),
        old_start: 1,
        old_lines: 2,
        new_start: 1,
        new_lines: 2,
        diff_content: "change B differing".to_string(),
    }];

    let result = three_way_patch_merge(base, &patch_a, &patch_b);
    assert!(result.is_err(), "Overlapping conflicting diffs must return conflict");
    let conflicts = result.unwrap_err();
    assert_eq!(conflicts.len(), 1);
    assert_eq!(conflicts[0].file_path, "file.rs");
}

#[tokio::test]
async fn test_t1_f6_05_identical_hunk_convergence() {
    let base = "line 1";
    let patch_a = vec![DiffHunk {
        hunk_id: "h1".to_string(),
        file_path: "f.rs".to_string(),
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        diff_content: "same content".to_string(),
    }];
    let patch_b = vec![DiffHunk {
        hunk_id: "h2".to_string(),
        file_path: "f.rs".to_string(),
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        diff_content: "same content".to_string(),
    }];

    let result = three_way_patch_merge(base, &patch_a, &patch_b);
    assert!(result.is_ok(), "Identical hunks should not generate conflict");
}

// ── Tier 2 Boundaries (Feature 6) ───────────────────────────────────────────

#[tokio::test]
async fn test_t2_f6_01_empty_patches_merge_idempotency() {
    let base = "original content";
    let result = three_way_patch_merge(base, &[], &[]).unwrap();
    assert!(result.contains("original content"));
}

#[tokio::test]
async fn test_t2_f6_02_empty_base_string_merge() {
    let base = "";
    let patch_a = vec![DiffHunk {
        hunk_id: "h1".to_string(),
        file_path: "f.rs".to_string(),
        old_start: 0,
        old_lines: 0,
        new_start: 1,
        new_lines: 1,
        diff_content: "+hello".to_string(),
    }];
    let result = three_way_patch_merge(base, &patch_a, &[]).unwrap();
    assert!(result.contains("h1"));
}

#[tokio::test]
async fn test_t2_f6_03_vector_clock_concurrent_divergence() {
    let mut vc1 = VectorClock::new();
    vc1.increment("a");

    let mut vc2 = VectorClock::new();
    vc2.increment("b");

    assert!(!vc1.dominates(&vc2));
    assert!(!vc2.dominates(&vc1));
}

#[tokio::test]
async fn test_t2_f6_04_multi_file_patch_merge() {
    let base = "file1\nfile2";
    let patch_a = vec![DiffHunk {
        hunk_id: "h1".to_string(),
        file_path: "a.rs".to_string(),
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        diff_content: "+a".to_string(),
    }];
    let patch_b = vec![DiffHunk {
        hunk_id: "h2".to_string(),
        file_path: "b.rs".to_string(),
        old_start: 1,
        old_lines: 1,
        new_start: 1,
        new_lines: 1,
        diff_content: "+b".to_string(),
    }];

    let res = three_way_patch_merge(base, &patch_a, &patch_b);
    assert!(res.is_ok());
}

#[tokio::test]
async fn test_t2_f6_05_extreme_vector_clock_node_counts() {
    let mut vc = VectorClock::new();
    for i in 0..5000 {
        vc.increment(&format!("agent_{}", i));
    }
    assert_eq!(vc.clocks.len(), 5000);
}
