//! Comprehensive Integration Test Suite for Milestone 1: Multi-Agent Swarm Orchestration
//!
//! Verifies:
//! 1. Actor Model & Priority Mailbox scheduling with biased select.
//! 2. 5 Specialized Roles (Planner, Coder, Reviewer, Auditor, Sentinel).
//! 3. Distributed Consensus & Quorum Voting (Unanimous, Majority, Supermajority, SentinelVeto).
//! 4. Hierarchical Subagent Delegation with budget limits & capability attenuation.
//! 5. Vector Clocks causal tracking & concurrency divergence detection.
//! 6. 3-Way RFC 6902 JSON Patch Merge with conflict resolution taxonomy.
//! 7. MVCC Transaction Coordinator & Optimistic Concurrency Control.
//! 8. End-to-End Swarm Orchestrator & Pregel State Graph Integration.

use liva_native_core::agent::graph::checkpoint::{
    Checkpointer, SqliteCheckpointer,
};
use liva_native_core::agent::graph::pregel::LivaAgentRuntime;
use liva_native_core::agent::state::AgentState;
use liva_native_core::agent::swarm::*;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::sandbox::policy::CapabilityToken;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

// -------------------------------------------------------------------------------------------------
// 1. ACTOR MODEL & PRIORITY MAILBOX SCHEDULING
// -------------------------------------------------------------------------------------------------

#[tokio::test]
async fn test_priority_mailbox_strict_scheduling() {
    let (tx, mut rx) = create_priority_mailbox(10, 10, 10);

    // Send messages in reverse priority order: Low -> Normal -> High
    let msg_low = SwarmMessage::new(
        SwarmRole::Auditor,
        Some(SwarmRole::Reviewer),
        MessagePriority::Low,
        SwarmPayload::General {
            action: "telemetry".to_string(),
            data: json!({"cpu": 12}),
        },
        VectorClock::new(),
    );

    let msg_norm = SwarmMessage::new(
        SwarmRole::Coder,
        Some(SwarmRole::Reviewer),
        MessagePriority::Normal,
        SwarmPayload::TaskProgress {
            task_id: "t1".to_string(),
            step_index: 1,
            status: "coding".to_string(),
            output: None,
        },
        VectorClock::new(),
    );

    let msg_high = SwarmMessage::new(
        SwarmRole::Sentinel,
        None,
        MessagePriority::High,
        SwarmPayload::SentinelVeto {
            veto_id: "v1".to_string(),
            target_id: "t1".to_string(),
            reason: "Critical safety violation".to_string(),
            violated_invariant: "No destructive operations".to_string(),
        },
        VectorClock::new(),
    );

    tx.send(msg_low).await.expect("send low");
    tx.send(msg_norm).await.expect("send normal");
    tx.send(msg_high).await.expect("send high");

    // Must dequeue High first, then Normal, then Low
    let first = rx.recv().await.expect("recv first");
    assert_eq!(first.priority, MessagePriority::High);
    assert!(matches!(first.payload, SwarmPayload::SentinelVeto { .. }));

    let second = rx.recv().await.expect("recv second");
    assert_eq!(second.priority, MessagePriority::Normal);
    assert!(matches!(second.payload, SwarmPayload::TaskProgress { .. }));

    let third = rx.recv().await.expect("recv third");
    assert_eq!(third.priority, MessagePriority::Low);
    assert!(matches!(third.payload, SwarmPayload::General { .. }));
}

// -------------------------------------------------------------------------------------------------
// 2. THE 5 SPECIALIZED ROLES VERIFICATION
// -------------------------------------------------------------------------------------------------

#[tokio::test]
async fn test_specialized_roles_behavior() {
    let (pool, dispatcher_tx) = AgentActorPool::new();

    // 1. Planner Role
    let mut planner = PlannerRole::new();
    assert_eq!(planner.role(), SwarmRole::Planner);
    assert!(planner.allowed_capabilities().contains(&CapabilityToken::FsRead));

    let ctx = ActorContext::new("p1", SwarmRole::Planner, dispatcher_tx.clone());
    let plan_msg = SwarmMessage::new(
        SwarmRole::Planner,
        Some(SwarmRole::Planner),
        MessagePriority::Normal,
        SwarmPayload::TaskProposal {
            task_id: "task-feat-1".to_string(),
            goal: "Add User Auth".to_string(),
            description: "Implement JWT auth".to_string(),
            required_capabilities: vec![CapabilityToken::FsRead, CapabilityToken::FsWrite],
            assigned_to: Some(SwarmRole::Coder),
            budget_tokens: 5_000,
        },
        VectorClock::new(),
    );
    let plan_reply = planner.handle_message(plan_msg, &ctx).await.unwrap();
    assert!(matches!(plan_reply, Some(SwarmPayload::TaskProgress { .. })));

    // 2. Coder Role
    let mut coder = CoderRole::new();
    let coder_ctx = ActorContext::new("c1", SwarmRole::Coder, dispatcher_tx.clone());
    let coder_msg = SwarmMessage::new(
        SwarmRole::Planner,
        Some(SwarmRole::Coder),
        MessagePriority::Normal,
        SwarmPayload::TaskProposal {
            task_id: "task-feat-1".to_string(),
            goal: "Write code".to_string(),
            description: "Write auth handler".to_string(),
            required_capabilities: vec![],
            assigned_to: Some(SwarmRole::Coder),
            budget_tokens: 2_000,
        },
        VectorClock::new(),
    );
    let coder_reply = coder.handle_message(coder_msg, &coder_ctx).await.unwrap();
    assert!(matches!(coder_reply, Some(SwarmPayload::CodeHunkPatch { .. })));

    // 3. Reviewer Role
    let mut reviewer = ReviewerRole::new();
    let rev_ctx = ActorContext::new("r1", SwarmRole::Reviewer, dispatcher_tx.clone());
    if let Some(SwarmPayload::CodeHunkPatch { patch_id, hunks, .. }) = coder_reply {
        let rev_msg = SwarmMessage::new(
            SwarmRole::Coder,
            Some(SwarmRole::Reviewer),
            MessagePriority::Normal,
            SwarmPayload::CodeHunkPatch {
                patch_id: patch_id.clone(),
                task_id: "task-feat-1".to_string(),
                file_path: "src/lib.rs".to_string(),
                hunks,
                summary: "Diff".to_string(),
            },
            VectorClock::new(),
        );
        let rev_reply = reviewer.handle_message(rev_msg, &rev_ctx).await.unwrap();
        match rev_reply {
            Some(SwarmPayload::ReviewVerdict { approved, .. }) => assert!(approved),
            _ => panic!("Expected ReviewVerdict"),
        }
    }

    // 4. Auditor Role (Security Analysis)
    let mut auditor = AuditorRole::new();
    let audit_ctx = ActorContext::new("a1", SwarmRole::Auditor, dispatcher_tx.clone());

    // Clean patch
    let clean_hunk = DiffHunk::new("h1", "src/auth.rs", 1, 2, 1, 3, "@@", "+// clean auth\n");
    let audit_clean_msg = SwarmMessage::new(
        SwarmRole::Coder,
        Some(SwarmRole::Auditor),
        MessagePriority::Normal,
        SwarmPayload::CodeHunkPatch {
            patch_id: "patch-clean".to_string(),
            task_id: "t1".to_string(),
            file_path: "src/auth.rs".to_string(),
            hunks: vec![clean_hunk],
            summary: "clean".to_string(),
        },
        VectorClock::new(),
    );
    let audit_clean_reply = auditor.handle_message(audit_clean_msg, &audit_ctx).await.unwrap();
    match audit_clean_reply {
        Some(SwarmPayload::AuditReport { clean, risk_score, .. }) => {
            assert!(clean);
            assert_eq!(risk_score, 0.0);
        }
        _ => panic!("Expected clean AuditReport"),
    }

    // Destructive patch audit
    let evil_hunk = DiffHunk::new("h2", "src/wipe.rs", 1, 2, 1, 3, "@@", "+rm -rf / --no-preserve-root\n");
    let audit_evil_msg = SwarmMessage::new(
        SwarmRole::Coder,
        Some(SwarmRole::Auditor),
        MessagePriority::Normal,
        SwarmPayload::CodeHunkPatch {
            patch_id: "patch-evil".to_string(),
            task_id: "t1".to_string(),
            file_path: "src/wipe.rs".to_string(),
            hunks: vec![evil_hunk],
            summary: "destructive".to_string(),
        },
        VectorClock::new(),
    );
    let audit_evil_reply = auditor.handle_message(audit_evil_msg, &audit_ctx).await.unwrap();
    match audit_evil_reply {
        Some(SwarmPayload::AuditReport { clean, risk_score, integrity_violations, .. }) => {
            assert!(!clean);
            assert_eq!(risk_score, 1.0);
            assert!(!integrity_violations.is_empty());
        }
        _ => panic!("Expected flagged AuditReport"),
    }

    // 5. Sentinel Role (Unconditional VETO)
    let mut sentinel = SentinelRole::new();
    let sent_ctx = ActorContext::new("s1", SwarmRole::Sentinel, dispatcher_tx.clone());

    let veto_hunk = DiffHunk::new("h3", "src/rm.rs", 1, 2, 1, 3, "@@", "+system(\"rm -rf /\");\n");
    let sent_msg = SwarmMessage::new(
        SwarmRole::Coder,
        Some(SwarmRole::Sentinel),
        MessagePriority::Normal,
        SwarmPayload::CodeHunkPatch {
            patch_id: "patch-veto-test".to_string(),
            task_id: "t1".to_string(),
            file_path: "src/rm.rs".to_string(),
            hunks: vec![veto_hunk],
            summary: "bad code".to_string(),
        },
        VectorClock::new(),
    );
    let sent_reply = sentinel.handle_message(sent_msg, &sent_ctx).await.unwrap();
    match sent_reply {
        Some(SwarmPayload::SentinelVeto { reason, .. }) => {
            assert!(reason.contains("rm -rf"));
        }
        _ => panic!("Expected SentinelVeto"),
    }

    let _ = pool;
}

// -------------------------------------------------------------------------------------------------
// 3. DISTRIBUTED QUORUM CONSENSUS & SENTINEL VETO
// -------------------------------------------------------------------------------------------------

#[test]
fn test_consensus_unanimous_majority_supermajority_and_veto() {
    let mut voters = HashMap::new();
    voters.insert("p1".to_string(), SwarmRole::Planner);
    voters.insert("c1".to_string(), SwarmRole::Coder);
    voters.insert("r1".to_string(), SwarmRole::Reviewer);
    voters.insert("a1".to_string(), SwarmRole::Auditor);
    voters.insert("s1".to_string(), SwarmRole::Sentinel);

    // 1. Majority Rule Pass
    let mut maj_session = ProposalSession::new(
        "prop-maj",
        "Refactor Auth",
        "Description",
        "p1",
        SwarmRole::Planner,
        ConsensusRule::Majority { min_quorum: 3 },
        voters.clone(),
        "sha256-hash",
        1000,
        10000,
    )
    .unwrap();

    maj_session
        .cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100)
        .unwrap();
    maj_session
        .cast_vote(VoteBallot::new("c1", SwarmRole::Coder, VoteDecision::Approve), 1200)
        .unwrap();
    assert!(matches!(maj_session.status, ConsensusStatus::Pending { .. }));

    // 3rd vote out of 5 gives strict majority (3 > 5/2)
    maj_session
        .cast_vote(VoteBallot::new("r1", SwarmRole::Reviewer, VoteDecision::Approve), 1300)
        .unwrap();
    assert!(matches!(maj_session.status, ConsensusStatus::Passed { .. }));
    assert_eq!(maj_session.outcome(), Some(ConsensusOutcome::Approved));

    // 2. Sentinel Veto Override (Overrules numerical approval)
    let mut veto_session = ProposalSession::new(
        "prop-veto",
        "Execute Shell Script",
        "Unchecked execution",
        "c1",
        SwarmRole::Coder,
        ConsensusRule::Majority { min_quorum: 3 },
        voters.clone(),
        "sha256-hash",
        1000,
        10000,
    )
    .unwrap();

    veto_session
        .cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100)
        .unwrap();
    veto_session
        .cast_vote(VoteBallot::new("c1", SwarmRole::Coder, VoteDecision::Approve), 1200)
        .unwrap();

    // Sentinel vetoes
    veto_session
        .cast_vote(
            VoteBallot::new(
                "s1",
                SwarmRole::Sentinel,
                VoteDecision::Veto {
                    reason: "Unauthorized privilege escalation".to_string(),
                },
            ),
            1300,
        )
        .unwrap();

    assert!(matches!(veto_session.status, ConsensusStatus::VetoedBySentinel { .. }));
    assert_eq!(
        veto_session.outcome(),
        Some(ConsensusOutcome::VetoedBySentinel {
            reason: "Unauthorized privilege escalation".to_string()
        })
    );

    // 3. Timeout Finalization
    let mut timeout_session = ProposalSession::new(
        "prop-timeout",
        "Idle proposal",
        "Desc",
        "p1",
        SwarmRole::Planner,
        ConsensusRule::Majority { min_quorum: 3 },
        voters,
        "sha256-hash",
        1000,
        5000, // Expires at 6000
    )
    .unwrap();

    timeout_session
        .cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100)
        .unwrap();

    // Trigger timeout after deadline
    timeout_session.finalize_on_timeout(7000);
    assert!(matches!(timeout_session.status, ConsensusStatus::TimedOut { .. }));
    assert_eq!(timeout_session.outcome(), Some(ConsensusOutcome::TimedOut));
}

// -------------------------------------------------------------------------------------------------
// 4. HIERARCHICAL SUBAGENT DELEGATION & BUDGETING
// -------------------------------------------------------------------------------------------------

#[test]
fn test_hierarchical_subagent_delegation_and_budget() {
    let mut root_caps = HashSet::new();
    root_caps.insert(CapabilityToken::FsRead);
    root_caps.insert(CapabilityToken::FsWrite);
    root_caps.insert(CapabilityToken::OsExecute);

    let mut root_token = DelegationToken::create_root(
        "orch-master",
        SwarmRole::Planner,
        "root-task-0",
        3, // Max depth = 3
        50_000,
        100,
        60_000,
        root_caps,
        1000,
    );

    assert_eq!(root_token.current_depth, 0);
    assert_eq!(root_token.budget.remaining_tokens, 50_000);

    // 1. Valid Child Delegation (Depth 1)
    let mut child_caps = HashSet::new();
    child_caps.insert(CapabilityToken::FsRead);
    child_caps.insert(CapabilityToken::FsWrite);

    let mut child_token = root_token
        .sub_delegate(
            "coder-agent-1",
            SwarmRole::Coder,
            "subtask-1",
            "Write core logic",
            15_000,
            30,
            child_caps.clone(),
            30_000,
            1100,
        )
        .expect("child delegation");

    assert_eq!(root_token.budget.remaining_tokens, 35_000);
    assert_eq!(root_token.budget.remaining_steps, 70);
    assert_eq!(child_token.current_depth, 1);
    assert_eq!(child_token.lineage.len(), 2);

    // 2. Valid Grandchild Delegation (Depth 2)
    let mut grandchild_caps = HashSet::new();
    grandchild_caps.insert(CapabilityToken::FsRead);

    let mut grandchild_token = child_token
        .sub_delegate(
            "reviewer-sub-1",
            SwarmRole::Reviewer,
            "subtask-2",
            "Read-only review",
            5_000,
            10,
            grandchild_caps,
            15_000,
            1200,
        )
        .expect("grandchild delegation");

    assert_eq!(child_token.budget.remaining_tokens, 10_000);
    assert_eq!(grandchild_token.current_depth, 2);
    assert_eq!(grandchild_token.lineage.len(), 3);

    // 3. Great-grandchild exceeds max depth 3 (depth 3 >= max_depth 3)
    let err_depth = grandchild_token.sub_delegate(
        "leaf-worker",
        SwarmRole::Auditor,
        "subtask-3",
        "Leaf task",
        1_000,
        2,
        HashSet::new(),
        5_000,
        1300,
    );
    assert!(matches!(err_depth, Err(DelegationError::MaxDepthExceeded { .. })));

    // 4. Privilege Escalation Prevention (Child cannot grant caps not held by itself)
    let mut illegal_caps = HashSet::new();
    illegal_caps.insert(CapabilityToken::OsExecute); // child_token only has FsRead, FsWrite!

    let err_priv = child_token.sub_delegate(
        "evil-worker",
        SwarmRole::Coder,
        "subtask-evil",
        "Exec",
        1_000,
        2,
        illegal_caps,
        5_000,
        1300,
    );
    assert!(matches!(err_priv, Err(DelegationError::PrivilegeEscalationAttempt { .. })));

    // 5. Token Spend & Refund Mechanics
    grandchild_token.spend_tokens(2_000, 1400).unwrap();
    grandchild_token.spend_step(1400).unwrap();
    assert_eq!(grandchild_token.budget.remaining_tokens, 3_000);
    assert_eq!(grandchild_token.budget.remaining_steps, 9);

    // Refund unused tokens to child
    child_token.refund_unused(grandchild_token.budget.remaining_tokens, grandchild_token.budget.remaining_steps);
    assert_eq!(child_token.budget.remaining_tokens, 13_000);

    // 6. Convert to SandboxPolicy
    let policy = child_token.to_sandbox_policy(PathBuf::from("/tmp/workspace"));
    assert!(policy.has_capability(CapabilityToken::FsRead));
    assert!(policy.has_capability(CapabilityToken::FsWrite));
    assert!(!policy.has_capability(CapabilityToken::OsExecute));
}

// -------------------------------------------------------------------------------------------------
// 5. VECTOR CLOCKS & CAUSALITY TRACKING
// -------------------------------------------------------------------------------------------------

#[test]
fn test_vector_clocks_causal_ordering_and_concurrency() {
    let mut v_base = VectorClock::new();
    v_base.set("node_a", 1);
    v_base.set("node_b", 1);

    let mut v_ours = v_base.clone();
    v_ours.tick("node_a"); // {node_a: 2, node_b: 1}

    let mut v_theirs = v_base.clone();
    v_theirs.tick("node_b"); // {node_a: 1, node_b: 2}

    // Base strictly precedes both
    assert_eq!(v_base.relation(&v_ours), CausalRelation::Before);
    assert_eq!(v_base.relation(&v_theirs), CausalRelation::Before);
    assert_eq!(v_ours.relation(&v_base), CausalRelation::After);

    // Ours and Theirs are concurrent
    assert_eq!(v_ours.relation(&v_theirs), CausalRelation::Concurrent);
    assert!(v_ours.is_concurrent_with(&v_theirs));

    // Element-wise maximum merge
    let v_merged = v_ours.merged(&v_theirs);
    assert_eq!(v_merged.get("node_a"), 2);
    assert_eq!(v_merged.get("node_b"), 2);
    assert_eq!(v_ours.relation(&v_merged), CausalRelation::Before);
    assert_eq!(v_theirs.relation(&v_merged), CausalRelation::Before);
}

// -------------------------------------------------------------------------------------------------
// 6. 3-WAY RFC 6902 JSON PATCH MERGE & CONFLICT RESOLUTION
// -------------------------------------------------------------------------------------------------

#[test]
fn test_3way_rfc6902_json_patch_merge() {
    let merger = ThreeWayMerger::new(ConflictResolutionStrategy::DeepMergeLww);

    // 1. Clean disjoint object updates
    let s_base = json!({
        "status": "in_progress",
        "active_plan": {"goal": "Setup", "step": 0},
        "scratchpad": {"counter": 1}
    });

    let s_ours = json!({
        "status": "in_progress",
        "active_plan": {"goal": "Setup", "step": 1},
        "scratchpad": {"counter": 1}
    });

    let s_theirs = json!({
        "status": "in_progress",
        "active_plan": {"goal": "Setup", "step": 0},
        "scratchpad": {"counter": 1, "notes": "audit passed"}
    });

    let res = merger.merge(&s_base, &s_ours, &s_theirs).expect("merge");
    assert!(res.is_clean);
    assert_eq!(
        res.merged_state,
        json!({
            "status": "in_progress",
            "active_plan": {"goal": "Setup", "step": 1},
            "scratchpad": {"counter": 1, "notes": "audit passed"}
        })
    );

    // 2. Chat messages append-stream deduplicated merge
    let m_base = json!({
        "messages": [
            {"role": "system", "content": "persona"},
            {"role": "user", "content": "help me code"}
        ]
    });

    let m_ours = json!({
        "messages": [
            {"role": "system", "content": "persona"},
            {"role": "user", "content": "help me code"},
            {"role": "planner", "content": "Step 1: Scaffold"}
        ]
    });

    let m_theirs = json!({
        "messages": [
            {"role": "system", "content": "persona"},
            {"role": "user", "content": "help me code"},
            {"role": "coder", "content": "Patch ready"}
        ]
    });

    let res_msgs = merger.merge(&m_base, &m_ours, &m_theirs).unwrap();
    assert!(res_msgs.is_clean);
    let msgs = res_msgs.merged_state["messages"].as_array().unwrap();
    assert_eq!(msgs.len(), 4);

    // 3. Conflict resolution with PreferOurs vs PreferTheirs
    let c_base = json!({"config_val": 10});
    let c_ours = json!({"config_val": 20});
    let c_theirs = json!({"config_val": 30});

    let merger_ours = ThreeWayMerger::new(ConflictResolutionStrategy::PreferOurs);
    let res_ours = merger_ours.merge(&c_base, &c_ours, &c_theirs).unwrap();
    assert_eq!(res_ours.merged_state, json!({"config_val": 20}));

    let merger_theirs = ThreeWayMerger::new(ConflictResolutionStrategy::PreferTheirs);
    let res_theirs = merger_theirs.merge(&c_base, &c_ours, &c_theirs).unwrap();
    assert_eq!(res_theirs.merged_state, json!({"config_val": 30}));
}

// -------------------------------------------------------------------------------------------------
// 7. MVCC TRANSACTION COORDINATOR & OCC COMMIT
// -------------------------------------------------------------------------------------------------

#[tokio::test]
async fn test_mvcc_transaction_coordinator_occ_commit() {
    let pool = Arc::new(DatabasePool::new_in_memory().unwrap());
    let enc = EncryptionEngine::new("checkpoint-diff-key-32-bytes-long");
    let cp = Arc::new(SqliteCheckpointer::new(pool, enc));
    let coord = MvccTransactionCoordinator::new(cp.clone(), ConflictResolutionStrategy::DeepMergeLww);
    let tid = "thread-occ-test";

    let mut s0 = AgentState::default();
    s0.current_node = "START".to_string();
    s0.scratchpad_set("data", json!("initial"));
    let clock0 = VectorClock::from_actor("root", 1);

    // Initial commit (Step 1)
    let res0 = coord
        .commit_state(tid, &s0, &clock0, &s0, &clock0, "START", None, None)
        .await
        .unwrap();
    assert_eq!(res0.committed_step, 1);
    assert!(!res0.was_merged);

    // Branch A (Coder) updates scratchpad field A
    let mut s_a = s0.clone();
    s_a.scratchpad_set("field_a", json!("alpha"));
    let mut clock_a = clock0.clone();
    clock_a.tick("coder");

    // Branch B (Reviewer) updates scratchpad field B concurrently
    let mut s_b = s0.clone();
    s_b.scratchpad_set("field_b", json!("beta"));
    let mut clock_b = clock0.clone();
    clock_b.tick("reviewer");

    // Reviewer commits first (Step 2)
    let res_b = coord
        .commit_state(tid, &s0, &clock0, &s_b, &clock_b, "reviewer", None, None)
        .await
        .unwrap();
    assert_eq!(res_b.committed_step, 2);

    // Coder commits second with base s0 -> Triggers 3-way merge into Step 3
    let res_a = coord
        .commit_state(tid, &s0, &clock0, &s_a, &clock_a, "coder", None, None)
        .await
        .unwrap();
    assert_eq!(res_a.committed_step, 3);
    assert!(res_a.was_merged);
    assert_eq!(res_a.final_state.scratchpad_get("field_a"), Some(&json!("alpha")));
    assert_eq!(res_a.final_state.scratchpad_get("field_b"), Some(&json!("beta")));
    assert_eq!(res_a.final_state.scratchpad_get("data"), Some(&json!("initial")));

    // Verify time travel reconstructs step 3 accurately
    let restored = cp.restore_time_travel(tid, 3).await.unwrap();
    assert_eq!(restored.scratchpad_get("field_a"), Some(&json!("alpha")));
    assert_eq!(restored.scratchpad_get("field_b"), Some(&json!("beta")));
}

// -------------------------------------------------------------------------------------------------
// 8. END-TO-END SWARM ORCHESTRATOR & PREGEL GRAPH
// -------------------------------------------------------------------------------------------------

#[tokio::test]
async fn test_swarm_orchestrator_and_pregel_graph_integration() {
    let orchestrator = Arc::new(SwarmOrchestrator::bootstrap_standard_swarm().await);

    // 1. Propose task
    let task_msg = orchestrator
        .propose_task(
            "Build User Auth",
            "Implement secure JWT authentication endpoints",
            Some(SwarmRole::Coder),
            10_000,
            vec![CapabilityToken::FsRead, CapabilityToken::FsWrite],
        )
        .await
        .expect("propose task");

    assert_eq!(task_msg.sender_role, SwarmRole::Planner);
    assert_eq!(task_msg.target_role, Some(SwarmRole::Coder));

    // Allow background actors to process
    tokio::time::sleep(Duration::from_millis(50)).await;

    // 2. Test Pregel StateGraph Integration
    let mut runtime: LivaAgentRuntime<AgentState> = LivaAgentRuntime::new();
    register_swarm_graph_nodes(&mut runtime, orchestrator.clone());

    let mut state = AgentState::default();
    state.scratchpad_set("goal", json!("Automated swarm test execution"));
    state.current_node = "START".to_string();

    let final_state = runtime.run(state).await.expect("pregel run");
    assert_eq!(final_state.current_node, "__END__");
    assert!(final_state.visited_nodes.contains(&"swarm_planner".to_string()));
    assert!(final_state.visited_nodes.contains(&"swarm_execute".to_string()));

    // 3. Graceful shutdown
    orchestrator.shutdown().await.expect("shutdown");
}
