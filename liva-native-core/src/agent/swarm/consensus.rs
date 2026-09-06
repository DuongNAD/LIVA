//! Distributed Consensus & Quorum Voting Engine for Multi-Agent Swarm
//!
//! Manages structured voting proposals across concurrent actors.
//! Enforces Unanimous, Majority, Supermajority, SentinelProtected, and WeightedQuorum rules
//! with deterministic early termination and absolute Sentinel Veto authority.

use super::types::SwarmRole;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use thiserror::Error;

/// Consensus voting rules determining threshold conditions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusRule {
    /// 100% of eligible voters must approve; any single rejection or veto fails immediately.
    Unanimous,
    /// Simple majority (> 50% of cast votes with minimum quorum).
    Majority { min_quorum: usize },
    /// Supermajority ratio (e.g. 2/3 or 3/4) evaluated with exact integer arithmetic.
    Supermajority {
        numerator: u32,
        denominator: u32,
        min_quorum: usize,
    },
    /// Majority approval required, but any Sentinel vote must approve (Sentinel rejection/veto aborts).
    SentinelProtected { min_quorum: usize },
    /// Weighted voting where each role possesses an assigned voting weight.
    WeightedQuorum {
        weights: HashMap<SwarmRole, u32>,
        threshold_weight: u32,
        min_quorum_weight: u32,
    },
}

/// Individual voter decision on a proposal ballot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum VoteDecision {
    Approve,
    Reject { reason: String },
    Abstain { reason: Option<String> },
    Veto { reason: String },
}

/// Submitted vote ballot cast by a swarm actor.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VoteBallot {
    pub voter_id: String,
    pub voter_role: SwarmRole,
    pub decision: VoteDecision,
    pub timestamp_ms: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

impl VoteBallot {
    pub fn new(
        voter_id: impl Into<String>,
        voter_role: SwarmRole,
        decision: VoteDecision,
    ) -> Self {
        Self {
            voter_id: voter_id.into(),
            voter_role,
            decision,
            timestamp_ms: chrono::Utc::now().timestamp_millis() as u64,
            rationale: None,
            signature: None,
        }
    }

    pub fn with_rationale(mut self, rationale: impl Into<String>) -> Self {
        self.rationale = Some(rationale.into());
        self
    }
}

/// Final summary outcome of a consensus proposal session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusOutcome {
    Approved,
    Rejected { reason: String },
    VetoedBySentinel { reason: String },
    TimedOut,
    QuorumNotReached,
}

/// Lifecycle status of a consensus proposal session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", content = "details")]
pub enum ConsensusStatus {
    Pending {
        votes_received: usize,
        total_eligible: usize,
        approvals: usize,
        rejections: usize,
        abstentions: usize,
    },
    Passed {
        approvals: usize,
        total_cast: usize,
        tally_summary: String,
        finalized_at_ms: u64,
    },
    Rejected {
        rejections: usize,
        total_cast: usize,
        reason: String,
        finalized_at_ms: u64,
    },
    VetoedBySentinel {
        sentinel_id: String,
        reason: String,
        finalized_at_ms: u64,
    },
    TimedOut {
        votes_received: usize,
        required_quorum: usize,
        finalized_at_ms: u64,
    },
    QuorumNotReached {
        votes_received: usize,
        required_quorum: usize,
        finalized_at_ms: u64,
    },
}

/// Errors occurring during consensus management and voting.
#[derive(Debug, Error, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConsensusError {
    #[error("Proposal session '{0}' not found")]
    ProposalNotFound(String),

    #[error("Proposal session '{0}' is already finalized")]
    ProposalAlreadyFinalized(String),

    #[error("Agent '{0}' is not in the eligible voters list for proposal '{1}'")]
    IneligibleVoter(String, String),

    #[error("Agent '{0}' has already cast a ballot for proposal '{1}'")]
    DuplicateVote(String, String),

    #[error("Consensus deadline has expired for proposal '{0}'")]
    DeadlineExpired(String),

    #[error("Invalid rule configuration: {0}")]
    InvalidRuleConfiguration(String),

    #[error("Role mismatch: Agent '{agent_id}' claimed role {claimed:?}, expected {expected:?}")]
    RoleMismatch {
        agent_id: String,
        claimed: SwarmRole,
        expected: SwarmRole,
    },
}

/// Active proposal session managing the consensus lifecycle.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProposalSession {
    pub proposal_id: String,
    pub title: String,
    pub description: String,
    pub proposer_id: String,
    pub proposer_role: SwarmRole,
    pub rule: ConsensusRule,
    pub eligible_voters: HashMap<String, SwarmRole>,
    pub ballots: HashMap<String, VoteBallot>,
    pub status: ConsensusStatus,
    pub payload_digest: String,
    pub created_at_ms: u64,
    pub timeout_ms: u64,
}

impl ProposalSession {
    /// Creates a new proposal session with explicit voter eligibility and timeout bounds.
    pub fn new(
        proposal_id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        proposer_id: impl Into<String>,
        proposer_role: SwarmRole,
        rule: ConsensusRule,
        eligible_voters: HashMap<String, SwarmRole>,
        payload_digest: impl Into<String>,
        created_at_ms: u64,
        timeout_duration_ms: u64,
    ) -> Result<Self, ConsensusError> {
        if eligible_voters.is_empty() {
            return Err(ConsensusError::InvalidRuleConfiguration(
                "Eligible voters list cannot be empty".to_string(),
            ));
        }

        let total_eligible = eligible_voters.len();
        let timeout_ms = created_at_ms + timeout_duration_ms;

        Ok(Self {
            proposal_id: proposal_id.into(),
            title: title.into(),
            description: description.into(),
            proposer_id: proposer_id.into(),
            proposer_role,
            rule,
            eligible_voters,
            ballots: HashMap::new(),
            status: ConsensusStatus::Pending {
                votes_received: 0,
                total_eligible,
                approvals: 0,
                rejections: 0,
                abstentions: 0,
            },
            payload_digest: payload_digest.into(),
            created_at_ms,
            timeout_ms,
        })
    }

    /// Casts a ballot and triggers deterministic early evaluation.
    pub fn cast_vote(&mut self, ballot: VoteBallot, now_ms: u64) -> Result<&ConsensusStatus, ConsensusError> {
        if !matches!(self.status, ConsensusStatus::Pending { .. }) {
            return Err(ConsensusError::ProposalAlreadyFinalized(self.proposal_id.clone()));
        }

        if now_ms > self.timeout_ms {
            self.finalize_on_timeout(now_ms);
            return Err(ConsensusError::DeadlineExpired(self.proposal_id.clone()));
        }

        // 1. Verify eligibility
        let expected_role = self
            .eligible_voters
            .get(&ballot.voter_id)
            .ok_or_else(|| ConsensusError::IneligibleVoter(ballot.voter_id.clone(), self.proposal_id.clone()))?;

        if *expected_role != ballot.voter_role {
            return Err(ConsensusError::RoleMismatch {
                agent_id: ballot.voter_id.clone(),
                claimed: ballot.voter_role,
                expected: *expected_role,
            });
        }

        // 2. Reject duplicate votes
        if self.ballots.contains_key(&ballot.voter_id) {
            return Err(ConsensusError::DuplicateVote(ballot.voter_id.clone(), self.proposal_id.clone()));
        }

        // 3. Sentinel Veto immediate check: Sentinel Reject or any explicit Veto decision
        if (ballot.voter_role == SwarmRole::Sentinel
            && matches!(ballot.decision, VoteDecision::Reject { .. } | VoteDecision::Veto { .. }))
            || matches!(ballot.decision, VoteDecision::Veto { .. })
        {
            let reason = match &ballot.decision {
                VoteDecision::Reject { reason } | VoteDecision::Veto { reason } => reason.clone(),
                _ => "Sentinel security veto".to_string(),
            };
            self.ballots.insert(ballot.voter_id.clone(), ballot.clone());
            self.status = ConsensusStatus::VetoedBySentinel {
                sentinel_id: ballot.voter_id,
                reason,
                finalized_at_ms: now_ms,
            };
            return Ok(&self.status);
        }

        // 4. Record ballot
        self.ballots.insert(ballot.voter_id.clone(), ballot);

        // 5. Re-evaluate state
        self.evaluate_tally(now_ms);
        Ok(&self.status)
    }

    /// Evaluates current ballots against rule thresholds.
    pub fn evaluate_tally(&mut self, now_ms: u64) {
        if !matches!(self.status, ConsensusStatus::Pending { .. }) {
            return;
        }

        let total_eligible = self.eligible_voters.len();
        let mut approvals = 0;
        let mut rejections = 0;
        let mut abstentions = 0;

        for ballot in self.ballots.values() {
            match &ballot.decision {
                VoteDecision::Approve => approvals += 1,
                VoteDecision::Reject { .. } => rejections += 1,
                VoteDecision::Abstain { .. } => abstentions += 1,
                VoteDecision::Veto { reason } => {
                    self.status = ConsensusStatus::VetoedBySentinel {
                        sentinel_id: ballot.voter_id.clone(),
                        reason: reason.clone(),
                        finalized_at_ms: now_ms,
                    };
                    return;
                }
            }
        }

        let total_cast = self.ballots.len();
        let remaining_votes = total_eligible.saturating_sub(total_cast);

        match &self.rule {
            ConsensusRule::Unanimous => {
                if rejections > 0 {
                    self.status = ConsensusStatus::Rejected {
                        rejections,
                        total_cast,
                        reason: "Unanimous vote failed: rejection recorded".to_string(),
                        finalized_at_ms: now_ms,
                    };
                } else if approvals == total_eligible {
                    self.status = ConsensusStatus::Passed {
                        approvals,
                        total_cast,
                        tally_summary: format!("{approvals}/{total_eligible} unanimous approval"),
                        finalized_at_ms: now_ms,
                    };
                } else {
                    self.status = ConsensusStatus::Pending {
                        votes_received: total_cast,
                        total_eligible,
                        approvals,
                        rejections,
                        abstentions,
                    };
                }
            }
            ConsensusRule::Majority { min_quorum } => {
                let required_quorum = (*min_quorum).max((total_eligible / 2) + 1);
                let needed_to_win = (total_eligible / 2) + 1;

                if approvals >= needed_to_win {
                    self.status = ConsensusStatus::Passed {
                        approvals,
                        total_cast,
                        tally_summary: format!("{approvals}/{total_cast} majority approval (quorum {required_quorum})"),
                        finalized_at_ms: now_ms,
                    };
                } else if rejections >= needed_to_win
                    || (approvals + remaining_votes < needed_to_win && total_cast >= required_quorum)
                {
                    self.status = ConsensusStatus::Rejected {
                        rejections,
                        total_cast,
                        reason: format!("Majority threshold impossible: {rejections} rejections"),
                        finalized_at_ms: now_ms,
                    };
                } else if total_cast == total_eligible {
                    if total_cast < required_quorum {
                        self.status = ConsensusStatus::QuorumNotReached {
                            votes_received: total_cast,
                            required_quorum,
                            finalized_at_ms: now_ms,
                        };
                    } else if approvals > rejections {
                        self.status = ConsensusStatus::Passed {
                            approvals,
                            total_cast,
                            tally_summary: format!("{approvals}/{total_cast} majority approval"),
                            finalized_at_ms: now_ms,
                        };
                    } else {
                        self.status = ConsensusStatus::Rejected {
                            rejections,
                            total_cast,
                            reason: format!("Approvals ({approvals}) did not exceed rejections ({rejections})"),
                            finalized_at_ms: now_ms,
                        };
                    }
                } else {
                    self.status = ConsensusStatus::Pending {
                        votes_received: total_cast,
                        total_eligible,
                        approvals,
                        rejections,
                        abstentions,
                    };
                }
            }
            ConsensusRule::Supermajority {
                numerator,
                denominator,
                min_quorum,
            } => {
                let num = *numerator as u64;
                let den = *denominator as u64;
                let required_quorum = *min_quorum;

                // Deterministic pass: approvals * den >= total_eligible * num
                if (approvals as u64) * den >= (total_eligible as u64) * num {
                    self.status = ConsensusStatus::Passed {
                        approvals,
                        total_cast,
                        tally_summary: format!("{approvals}/{total_eligible} reached supermajority {num}/{den}"),
                        finalized_at_ms: now_ms,
                    };
                } else if ((approvals + remaining_votes) as u64) * den < (total_eligible as u64) * num
                    && total_cast >= required_quorum
                {
                    self.status = ConsensusStatus::Rejected {
                        rejections,
                        total_cast,
                        reason: format!("Mathematically impossible to achieve {num}/{den} supermajority"),
                        finalized_at_ms: now_ms,
                    };
                } else if total_cast == total_eligible {
                    if total_cast < required_quorum {
                        self.status = ConsensusStatus::QuorumNotReached {
                            votes_received: total_cast,
                            required_quorum,
                            finalized_at_ms: now_ms,
                        };
                    } else if (approvals as u64) * den >= (total_cast as u64) * num {
                        self.status = ConsensusStatus::Passed {
                            approvals,
                            total_cast,
                            tally_summary: format!("{approvals}/{total_cast} achieved supermajority {num}/{den}"),
                            finalized_at_ms: now_ms,
                        };
                    } else {
                        self.status = ConsensusStatus::Rejected {
                            rejections,
                            total_cast,
                            reason: format!("{approvals}/{total_cast} failed to meet {num}/{den} supermajority"),
                            finalized_at_ms: now_ms,
                        };
                    }
                } else {
                    self.status = ConsensusStatus::Pending {
                        votes_received: total_cast,
                        total_eligible,
                        approvals,
                        rejections,
                        abstentions,
                    };
                }
            }
            ConsensusRule::SentinelProtected { min_quorum } => {
                let required_quorum = (*min_quorum).max((total_eligible / 2) + 1);
                let needed_to_win = (total_eligible / 2) + 1;

                // Check Sentinel approval
                let sentinel_approved = self.ballots.values().any(|b| {
                    b.voter_role == SwarmRole::Sentinel && matches!(b.decision, VoteDecision::Approve)
                });

                if sentinel_approved && approvals >= needed_to_win {
                    self.status = ConsensusStatus::Passed {
                        approvals,
                        total_cast,
                        tally_summary: format!("{approvals}/{total_cast} Sentinel-protected approval passed"),
                        finalized_at_ms: now_ms,
                    };
                } else if total_cast == total_eligible {
                    if !sentinel_approved {
                        self.status = ConsensusStatus::Rejected {
                            rejections,
                            total_cast,
                            reason: "Sentinel did not cast approval".to_string(),
                            finalized_at_ms: now_ms,
                        };
                    } else if approvals > rejections && total_cast >= required_quorum {
                        self.status = ConsensusStatus::Passed {
                            approvals,
                            total_cast,
                            tally_summary: format!("{approvals}/{total_cast} Sentinel-protected approval passed"),
                            finalized_at_ms: now_ms,
                        };
                    } else {
                        self.status = ConsensusStatus::Rejected {
                            rejections,
                            total_cast,
                            reason: "Failed to achieve quorum or majority".to_string(),
                            finalized_at_ms: now_ms,
                        };
                    }
                } else {
                    self.status = ConsensusStatus::Pending {
                        votes_received: total_cast,
                        total_eligible,
                        approvals,
                        rejections,
                        abstentions,
                    };
                }
            }
            ConsensusRule::WeightedQuorum {
                weights,
                threshold_weight,
                min_quorum_weight,
            } => {
                let mut current_weight_cast = 0u32;
                let mut approval_weight = 0u32;
                let mut _rejection_weight = 0u32;

                for ballot in self.ballots.values() {
                    let weight = weights.get(&ballot.voter_role).copied().unwrap_or(1);
                    current_weight_cast += weight;
                    match ballot.decision {
                        VoteDecision::Approve => approval_weight += weight,
                        VoteDecision::Reject { .. } => _rejection_weight += weight,
                        _ => {}
                    }
                }

                if approval_weight >= *threshold_weight {
                    self.status = ConsensusStatus::Passed {
                        approvals,
                        total_cast,
                        tally_summary: format!("Approval weight {approval_weight} >= {threshold_weight}"),
                        finalized_at_ms: now_ms,
                    };
                } else if total_cast == total_eligible {
                    if current_weight_cast < *min_quorum_weight {
                        self.status = ConsensusStatus::QuorumNotReached {
                            votes_received: total_cast,
                            required_quorum: *min_quorum_weight as usize,
                            finalized_at_ms: now_ms,
                        };
                    } else {
                        self.status = ConsensusStatus::Rejected {
                            rejections,
                            total_cast,
                            reason: format!("Approval weight {approval_weight} < threshold {threshold_weight}"),
                            finalized_at_ms: now_ms,
                        };
                    }
                } else {
                    self.status = ConsensusStatus::Pending {
                        votes_received: total_cast,
                        total_eligible,
                        approvals,
                        rejections,
                        abstentions,
                    };
                }
            }
        }
    }

    /// Handles session timeout when deadline expires before all votes are received.
    pub fn finalize_on_timeout(&mut self, now_ms: u64) {
        if !matches!(self.status, ConsensusStatus::Pending { .. }) {
            return;
        }

        let total_cast = self.ballots.len();
        let min_quorum = match self.rule {
            ConsensusRule::Majority { min_quorum } => min_quorum,
            ConsensusRule::Supermajority { min_quorum, .. } => min_quorum,
            ConsensusRule::SentinelProtected { min_quorum } => min_quorum,
            ConsensusRule::Unanimous => self.eligible_voters.len(),
            ConsensusRule::WeightedQuorum { min_quorum_weight, .. } => min_quorum_weight as usize,
        };

        if total_cast < min_quorum {
            self.status = ConsensusStatus::TimedOut {
                votes_received: total_cast,
                required_quorum: min_quorum,
                finalized_at_ms: now_ms,
            };
            return;
        }

        self.evaluate_tally(now_ms);

        if matches!(self.status, ConsensusStatus::Pending { .. }) {
            self.status = ConsensusStatus::TimedOut {
                votes_received: total_cast,
                required_quorum: min_quorum,
                finalized_at_ms: now_ms,
            };
        }
    }

    /// Retrieve high-level outcome if session is finalized.
    pub fn outcome(&self) -> Option<ConsensusOutcome> {
        match &self.status {
            ConsensusStatus::Passed { .. } => Some(ConsensusOutcome::Approved),
            ConsensusStatus::Rejected { reason, .. } => Some(ConsensusOutcome::Rejected {
                reason: reason.clone(),
            }),
            ConsensusStatus::VetoedBySentinel { reason, .. } => Some(ConsensusOutcome::VetoedBySentinel {
                reason: reason.clone(),
            }),
            ConsensusStatus::TimedOut { .. } => Some(ConsensusOutcome::TimedOut),
            ConsensusStatus::QuorumNotReached { .. } => Some(ConsensusOutcome::QuorumNotReached),
            ConsensusStatus::Pending { .. } => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn voters_map() -> HashMap<String, SwarmRole> {
        let mut map = HashMap::new();
        map.insert("p1".to_string(), SwarmRole::Planner);
        map.insert("c1".to_string(), SwarmRole::Coder);
        map.insert("r1".to_string(), SwarmRole::Reviewer);
        map.insert("a1".to_string(), SwarmRole::Auditor);
        map.insert("s1".to_string(), SwarmRole::Sentinel);
        map
    }

    #[test]
    fn test_unanimous_pass_and_early_fail() {
        let voters = voters_map();
        let mut session = ProposalSession::new(
            "prop-1",
            "Root Plan",
            "Initial goal",
            "p1",
            SwarmRole::Planner,
            ConsensusRule::Unanimous,
            voters.clone(),
            "sha256-digest",
            1000,
            5000,
        )
        .expect("session");

        // Cast 4 approves
        session.cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100).unwrap();
        session.cast_vote(VoteBallot::new("c1", SwarmRole::Coder, VoteDecision::Approve), 1200).unwrap();
        session.cast_vote(VoteBallot::new("r1", SwarmRole::Reviewer, VoteDecision::Approve), 1300).unwrap();
        session.cast_vote(VoteBallot::new("a1", SwarmRole::Auditor, VoteDecision::Approve), 1400).unwrap();

        assert!(matches!(session.status, ConsensusStatus::Pending { .. }));

        // 5th approve passes unanimous
        session.cast_vote(VoteBallot::new("s1", SwarmRole::Sentinel, VoteDecision::Approve), 1500).unwrap();
        assert!(matches!(session.status, ConsensusStatus::Passed { .. }));
        assert_eq!(session.outcome(), Some(ConsensusOutcome::Approved));

        // Test early fail on 1 reject
        let mut session2 = ProposalSession::new(
            "prop-2",
            "Root Plan",
            "Initial goal",
            "p1",
            SwarmRole::Planner,
            ConsensusRule::Unanimous,
            voters,
            "sha256-digest",
            1000,
            5000,
        )
        .unwrap();

        session2.cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100).unwrap();
        session2.cast_vote(VoteBallot::new("r1", SwarmRole::Reviewer, VoteDecision::Reject { reason: "Flaw".to_string() }), 1200).unwrap();
        assert!(matches!(session2.status, ConsensusStatus::Rejected { .. }));
        assert_eq!(session2.outcome(), Some(ConsensusOutcome::Rejected { reason: "Unanimous vote failed: rejection recorded".to_string() }));
    }

    #[test]
    fn test_sentinel_veto_immediate_override() {
        let voters = voters_map();
        let mut session = ProposalSession::new(
            "prop-veto",
            "High Risk Command",
            "Exec tool",
            "c1",
            SwarmRole::Coder,
            ConsensusRule::Majority { min_quorum: 3 },
            voters,
            "sha256-digest",
            1000,
            5000,
        )
        .unwrap();

        // 2 actors approve (vote pending)
        session.cast_vote(VoteBallot::new("p1", SwarmRole::Planner, VoteDecision::Approve), 1100).unwrap();
        session.cast_vote(VoteBallot::new("c1", SwarmRole::Coder, VoteDecision::Approve), 1200).unwrap();
        assert!(matches!(session.status, ConsensusStatus::Pending { .. }));

        // Sentinel casts Veto, immediately aborting the session
        session.cast_vote(VoteBallot::new("s1", SwarmRole::Sentinel, VoteDecision::Veto { reason: "Dangerous command detected".to_string() }), 1300).unwrap();

        assert!(matches!(session.status, ConsensusStatus::VetoedBySentinel { .. }));
        match session.outcome() {
            Some(ConsensusOutcome::VetoedBySentinel { reason }) => {
                assert_eq!(reason, "Dangerous command detected");
            }
            other => panic!("Expected Sentinel Veto, got {:?}", other),
        }
    }

    #[test]
    fn test_supermajority_quorum() {
        let mut voters = HashMap::new();
        for i in 1..=6 {
            voters.insert(format!("v{i}"), SwarmRole::Reviewer);
        }

        let mut session = ProposalSession::new(
            "prop-super",
            "Refactor",
            "Major change",
            "v1",
            SwarmRole::Reviewer,
            ConsensusRule::Supermajority { numerator: 2, denominator: 3, min_quorum: 4 },
            voters,
            "digest",
            1000,
            5000,
        )
        .unwrap();

        // 4 approvals out of 6 is 2/3 (66.7%) -> passes supermajority
        session.cast_vote(VoteBallot::new("v1", SwarmRole::Reviewer, VoteDecision::Approve), 1100).unwrap();
        session.cast_vote(VoteBallot::new("v2", SwarmRole::Reviewer, VoteDecision::Approve), 1200).unwrap();
        session.cast_vote(VoteBallot::new("v3", SwarmRole::Reviewer, VoteDecision::Approve), 1300).unwrap();
        assert!(matches!(session.status, ConsensusStatus::Pending { .. }));

        session.cast_vote(VoteBallot::new("v4", SwarmRole::Reviewer, VoteDecision::Approve), 1400).unwrap();
        assert!(matches!(session.status, ConsensusStatus::Passed { .. }));
    }
}
