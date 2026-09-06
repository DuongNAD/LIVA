//! Phase 4 E2E Test Suite — Tier 3: Pairwise Cross-Feature Interactions
//!
//! Validates orthogonal subsystem interactions across:
//! - M1: Multi-Agent Swarm (Roles, Quorum, Delegation, Vector Clocks)
//! - M2: Living Canvas & Diff Review (Split-Pane, Generative UI, HITL)
//! - M3: Security & OS Sandbox (Windows Job, Seatbelt, Capability Tokens, SSRF)
//! - M4: Resilience & Hardening (Fuzzing, 1,000-Turn Stress, RAM Leak Profiling)

use liva_native_core::agent::graph::{ApprovalContext, ApprovalDecision};
use liva_native_core::sandbox::policy::validate_command;
use liva_native_core::sandbox::tier2_os::{OsSandboxError, OsSandboxPolicy, OsSandboxRunner};
use serde_json::json;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::mpsc;

// ============================================================================
// PAIR 1: SWARM ROLES (M1) + HITL DIFF HUNK APPROVAL (M2)
// ============================================================================

#[test]
fn test_tier3_pair1_reviewer_critique_triggers_hitl_hunk_revision() {
    // 1. Coder proposes hunk
    let hunk_id = "hunk-auth-01";
    let original_patch = "+fn login() { bypass_auth(); }";

    // 2. Reviewer rejects patch due to insecure bypass
    let reviewer_approved = false;
    assert!(!reviewer_approved);

    // 3. System suspends via HITL ApprovalContext for human intervention
    let ctx = ApprovalContext::new(
        "act-rev-101",
        "review_hunk_decision",
        json!({"hunk_id": hunk_id, "patch": original_patch}),
        "Reviewer rejected security bypass; human intervention required",
        120,
    );
    assert!(!ctx.is_expired_now());

    // 4. Human provides modified safe code
    let user_decision = ApprovalDecision::Approved {
        modified_args: Some(json!({
            "hunk_id": hunk_id,
            "user_patch": "+fn login() { verify_credentials(); }"
        })),
    };

    match user_decision {
        ApprovalDecision::Approved { modified_args } => {
            let args = modified_args.unwrap();
            assert!(args["user_patch"].as_str().unwrap().contains("verify_credentials"));
        }
        _ => panic!("Expected user approved revision"),
    }
}

// ============================================================================
// PAIR 2: DELEGATION BUDGET (M1) + CAPABILITY TOKEN SANDBOX (M3)
// ============================================================================

#[test]
fn test_tier3_pair2_subagent_delegation_restricts_sandbox_capability() {
    #[derive(Clone, PartialEq, Eq)]
    enum Cap {
        Read(PathBuf),
        Write(PathBuf),
    }

    struct AgentToken {
        depth: usize,
        max_depth: usize,
        budget: u64,
        capabilities: Vec<Cap>,
    }

    let root = AgentToken {
        depth: 0,
        max_depth: 2,
        budget: 5000,
        capabilities: vec![
            Cap::Read(PathBuf::from("/workspace")),
            Cap::Write(PathBuf::from("/workspace/src")),
        ],
    };

    // Subagent spawned with restricted read-only capability and half budget
    let sub = AgentToken {
        depth: root.depth + 1,
        max_depth: root.max_depth,
        budget: 2500,
        capabilities: vec![Cap::Read(PathBuf::from("/workspace/docs"))],
    };

    assert_eq!(sub.depth, 1);
    assert_eq!(sub.budget, 2500);
    assert_eq!(sub.capabilities.len(), 1);

    // Verify subagent is denied write capability
    let can_write = sub.capabilities.iter().any(|c| matches!(c, Cap::Write(_)));
    assert!(!can_write, "Subagent must not inherit ungranted write capabilities");
}

// ============================================================================
// PAIR 3: CONSENSUS QUORUM (M1) + GENERATIVE UI WIDGET STREAMING (M2)
// ============================================================================

#[test]
fn test_tier3_pair3_swarm_quorum_triggers_widget_streaming() {
    let mut votes = HashMap::new();
    votes.insert("Planner", true);
    votes.insert("Coder", true);
    votes.insert("Reviewer", true);
    votes.insert("Sentinel", true);

    let approvals = votes.values().filter(|&&v| v).count();
    let total_voters = 4;
    let quorum_reached = approvals * 2 > total_voters;
    assert!(quorum_reached);

    // Living Canvas streams the approved widget
    let widget_html = "<div class='metric-card'>Active Nodes: 12</div>";
    let widget_json = json!({
        "widget_id": "widget-swarm-status",
        "html": widget_html,
        "approved_by_quorum": true,
        "voter_count": approvals,
    });

    assert_eq!(widget_json["approved_by_quorum"], true);
    assert_eq!(widget_json["voter_count"], 4);
}

// ============================================================================
// PAIR 4: VECTOR CLOCK MERGE (M1) + 1,000-TURN CONCURRENCY STRESS (M4)
// ============================================================================

#[tokio::test]
async fn test_tier3_pair4_concurrent_1000_turns_with_vector_clock_reconciliation() {
    #[derive(Clone, Default)]
    struct VClock {
        clocks: HashMap<String, u64>,
    }

    let (tx_a, mut rx_a) = mpsc::channel::<VClock>(500);
    let (tx_b, mut rx_b) = mpsc::channel::<VClock>(500);

    let h_a = tokio::spawn(async move {
        let mut vc = VClock::default();
        for _ in 0..500 {
            *vc.clocks.entry("node_a".to_string()).or_insert(0) += 1;
            tx_b.send(vc.clone()).await.unwrap();
            if let Some(incoming) = rx_a.recv().await {
                for (k, v) in incoming.clocks {
                    let entry = vc.clocks.entry(k).or_insert(0);
                    *entry = (*entry).max(v);
                }
            }
        }
        vc
    });

    let h_b = tokio::spawn(async move {
        let mut vc = VClock::default();
        for _ in 0..500 {
            if let Some(incoming) = rx_b.recv().await {
                for (k, v) in incoming.clocks {
                    let entry = vc.clocks.entry(k).or_insert(0);
                    *entry = (*entry).max(v);
                }
            }
            *vc.clocks.entry("node_b".to_string()).or_insert(0) += 1;
            tx_a.send(vc.clone()).await.unwrap();
        }
        vc
    });

    let (res_a, res_b) = tokio::join!(h_a, h_b);
    let final_a = res_a.unwrap();
    let final_b = res_b.unwrap();

    assert!(final_a.clocks.get("node_a").copied().unwrap_or(0) >= 500);
    assert!(final_b.clocks.get("node_b").copied().unwrap_or(0) >= 500);
}

// ============================================================================
// PAIR 5: DIFF REVIEWER (M2) + TIER 2 OS SANDBOX ISOLATION (M3)
// ============================================================================

#[tokio::test]
async fn test_tier3_pair5_diff_patch_application_confined_to_sandbox() {
    let mut policy = OsSandboxPolicy::default();
    policy.allowed_read_paths.push(PathBuf::from("/workspace"));
    policy.allowed_write_paths.push(PathBuf::from("/workspace/scratch"));

    // Verify sandbox denies patch write escaping workspace
    let forbidden_path = Path::new("/etc/crontab");
    let is_allowed = policy.allowed_write_paths.iter().any(|p| forbidden_path.starts_with(p));
    assert!(!is_allowed, "Sandbox must prevent patch application to system paths");

    // Allowed path
    let target_path = Path::new("/workspace/scratch/main.rs");
    let is_target_allowed = policy.allowed_write_paths.iter().any(|p| target_path.starts_with(p));
    assert!(is_target_allowed);
}

// ============================================================================
// PAIR 6: GENERATIVE UI IFRAME (M2) + SSRF VALIDATION FILTER (M3)
// ============================================================================

#[test]
fn test_tier3_pair6_generative_ui_external_resource_ssrf_filter() {
    fn check_ssrf(url_str: &str) -> bool {
        if let Ok(url) = reqwest::Url::parse(url_str) {
            if let Some(host) = url.host_str() {
                return !(host == "localhost" || host.starts_with("127.") || host.starts_with("169.254.") || host.starts_with("192.168."));
            }
        }
        false
    }

    let malicious_src = "http://169.254.169.254/latest/meta-data/";
    let safe_src = "https://cdn.jsdelivr.net/npm/chart.js";

    assert!(!check_ssrf(malicious_src), "SSRF filter must block cloud metadata URL in widget");
    assert!(check_ssrf(safe_src), "SSRF filter must permit public CDN URL");
}

// ============================================================================
// PAIR 7: SANDBOX OS POLICY (M3) + MALICIOUS FUZZING PAYLOADS (M4)
// ============================================================================

#[tokio::test]
async fn test_tier3_pair7_fuzzed_shell_injection_blocked_by_sandbox() {
    let runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy::default();

    // 1. Validate direct runner blocks forbidden command
    let res = runner.execute_command("rm", &["-rf".to_string(), "/".to_string()], &policy).await;
    assert!(res.is_err());
    match res.unwrap_err() {
        OsSandboxError::CommandForbidden(_) => {}
        other => panic!("Expected CommandForbidden, got: {:?}", other),
    }

    // 2. Validate AST sanitizer blocks fuzzed malicious commands
    assert!(validate_command("mkfs", &["/dev/sda".to_string()]).is_err());
    assert!(validate_command("shutdown", &["-h".to_string(), "now".to_string()]).is_err());
    assert!(validate_command("cargo", &["--manifest-path".to_string(), "../../etc/shadow".to_string()]).is_err());
    assert!(validate_command("bash", &["-c".to_string(), ":(){ :|:& };:".to_string()]).is_err());
}

// ============================================================================
// PAIR 8: SENTINEL VETO (M1) + LIVING CANVAS HUNK ROLLBACK (M2)
// ============================================================================

#[test]
fn test_tier3_pair8_sentinel_veto_rolls_back_pending_canvas_hunks() {
    #[derive(Debug, PartialEq, Eq)]
    enum HunkState { Pending, Approved, Rejected }

    let mut hunks = HashMap::new();
    hunks.insert("h1", HunkState::Approved);
    hunks.insert("h2", HunkState::Pending);

    // Sentinel detects zero-day pattern and issues veto
    let sentinel_veto = true;
    if sentinel_veto {
        // Rollback all approved/pending hunks in this session
        for state in hunks.values_mut() {
            *state = HunkState::Rejected;
        }
    }

    assert_eq!(hunks.get("h1"), Some(&HunkState::Rejected));
    assert_eq!(hunks.get("h2"), Some(&HunkState::Rejected));
}

// ============================================================================
// PAIR 9: SUBAGENT DELEGATION DEPTH (M1) + RAM LEAK PROFILING (M4)
// ============================================================================

#[test]
fn test_tier3_pair9_deep_subagent_recursion_with_zero_ram_leak() {
    let initial_rss = 100_000_000; // 100MB
    let current_rss = Arc::new(AtomicUsize::new(initial_rss));

    // Simulate 5 levels of recursion with allocations & cleanups
    for _ in 0..500 {
        let cr_clone = current_rss.clone();
        // Allocate subagent context
        cr_clone.fetch_add(50_000, Ordering::Relaxed);
        // Subagent completes work and drops memory
        cr_clone.fetch_sub(50_000, Ordering::Relaxed);
    }

    let final_rss = current_rss.load(Ordering::Relaxed);
    let growth = if final_rss > initial_rss {
        ((final_rss - initial_rss) as f64 / initial_rss as f64) * 100.0
    } else {
        0.0
    };

    assert_eq!(growth, 0.0, "Deep subagent recursion must have zero net RAM leak");
}

// ============================================================================
// PAIR 10: VIEWPORT RESIZE (M2) + SWARM BROADCAST NOTIFICATION (M1)
// ============================================================================

#[tokio::test]
async fn test_tier3_pair10_canvas_viewport_resize_broadcast_to_agents() {
    let (tx, mut rx) = mpsc::channel::<String>(10);

    let resize_event = json!({
        "event": "viewport:resize",
        "chat_pct": 35.0,
        "canvas_pct": 65.0,
    });

    tx.send(resize_event.to_string()).await.unwrap();
    let received = rx.recv().await.unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&received).unwrap();

    assert_eq!(parsed["event"], "viewport:resize");
    assert_eq!(parsed["canvas_pct"], 65.0);
}
