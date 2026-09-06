//! E2E Test Suite: ClawHub Skills Runtime, Hot-Reload Watcher, Consent Engine & Tool Dispatcher
//! Covers Feature 10 (SKILL.md Parser), Feature 11 (Hot-Reload Watcher), Feature 12 (Consent Engine),
//! and Feature 13 (MCP Tool Dispatcher Bridge)
//! Tiers 1, 2, 3 & 4 Test Suite

use liva_native_core::skills::{
    ClawHubSkillParser, ConsentAuthority, ConsentDecision, ConsentSuspender, InMemoryConsentManager,
    MockToolDispatcher, RiskLevel, SkillChangeEvent, SkillError, SkillPackageStore, SkillParser,
    SkillRuntimeType, SkillToolDefinition, SkillWatcher, ToolCallRequest, ToolDispatcher,
    UnifiedToolDispatcher, parse_skill_markdown,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

// ============================================================================
// Tier 1: Feature Coverage Tests
// ============================================================================

#[tokio::test]
async fn test_tier1_clawhub_skill_manifest_parser() {
    let raw_skill = r#"---
name: "web-researcher"
version: "1.2.0"
description: "Autonomous web search and document extraction skill"
author: "LIVA Team"
license: "Apache-2.0"
runtime_type: "native_rust"
---

# Instructions
1. Run search query
2. Synthesize results
"#;

    let dir = PathBuf::from("/data/skills/web-researcher");
    let loaded = parse_skill_markdown(raw_skill, &dir).expect("parsing skill must succeed");

    assert_eq!(loaded.manifest.name, "web-researcher");
    assert_eq!(loaded.manifest.version, "1.2.0");
    assert_eq!(loaded.manifest.runtime_type, SkillRuntimeType::NativeRust);
    assert_eq!(loaded.manifest.author.as_deref(), Some("LIVA Team"));
    assert!(loaded.markdown_instructions.contains("# Instructions"));
    assert!(!loaded.content_hash.is_empty());
}

#[tokio::test]
async fn test_tier1_skill_sha256_fingerprinting() {
    let content_v1 = "---\nname: \"test-skill\"\nversion: \"1.0.0\"\n---\nPrompt v1";
    let content_v2 = "---\nname: \"test-skill\"\nversion: \"1.0.1\"\n---\nPrompt v2";

    let pkg1 = parse_skill_markdown(content_v1, Path::new("/tmp")).unwrap();
    let pkg2 = parse_skill_markdown(content_v2, Path::new("/tmp")).unwrap();

    assert_ne!(pkg1.content_hash, pkg2.content_hash, "Changed content must yield distinct SHA-256 fingerprint");
    assert_eq!(pkg1.content_hash.len(), 64, "SHA-256 hex digest must be 64 characters");
}

#[tokio::test]
async fn test_tier1_consent_suspense_and_approval() {
    let suspender = Arc::new(ConsentSuspender::new());
    let req_id = "req-consent-001";

    // Read-only tool should auto-allow
    let decision_safe = suspender.request_consent(req_id, RiskLevel::ReadOnlySafe, Duration::from_secs(1)).await;
    assert!(matches!(decision_safe, ConsentDecision::Approved { .. }));

    // Destructive high-risk tool awaits explicit user approval
    let susp_clone = Arc::clone(&suspender);
    let approval_task = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        let resolved = susp_clone.resolve_consent("req-consent-002", ConsentDecision::Approved {
            user_id: "admin_user".to_string(),
            timestamp_unix: 1725180000,
        }).await;
        assert!(resolved);
    });

    let decision_high_risk = suspender.request_consent("req-consent-002", RiskLevel::DestructiveHighRisk, Duration::from_secs(2)).await;
    approval_task.await.unwrap();

    if let ConsentDecision::Approved { user_id, .. } = decision_high_risk {
        assert_eq!(user_id, "admin_user");
    } else {
        panic!("Expected consent approval");
    }
}

#[tokio::test]
async fn test_tier1_tool_dispatcher_routing() {
    let dispatcher = MockToolDispatcher::new();
    dispatcher.register_tool(SkillToolDefinition {
        name: "search_tool".to_string(),
        description: "Search web".to_string(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::ReadOnlySafe,
    }).await;

    let res = dispatcher.dispatch("search_tool", serde_json::json!({"query": "Rust Lang"})).await.unwrap();
    assert_eq!(res.get("query").unwrap().as_str(), Some("Rust Lang"));
}

// ============================================================================
// Tier 2: Boundary Value Analysis & Guardrail Corner Cases
// ============================================================================

#[tokio::test]
async fn test_tier2_corrupted_yaml_frontmatter_fails_closed() {
    // 1. Missing leading delimiter
    let missing_lead = "name: \"invalid\"\nversion: \"1.0.0\"\n---\nBody";
    let res1 = parse_skill_markdown(missing_lead, Path::new("/tmp"));
    assert!(matches!(res1, Err(SkillError::ManifestParse(_))));

    // 2. Missing closing delimiter
    let missing_close = "---\nname: \"invalid\"\nversion: \"1.0.0\"\nBody";
    let res2 = parse_skill_markdown(missing_close, Path::new("/tmp"));
    assert!(matches!(res2, Err(SkillError::ManifestParse(_))));

    // 3. Missing mandatory field 'version'
    let missing_version = "---\nname: \"no_version\"\n---\nBody";
    let res3 = parse_skill_markdown(missing_version, Path::new("/tmp"));
    assert!(matches!(res3, Err(SkillError::ManifestParse(_))));
}

#[tokio::test]
async fn test_tier2_consent_suspense_timeout_fail_closed() {
    let suspender = ConsentSuspender::new();
    // User does not respond within 50ms timeout window -> TimedOut
    let decision = suspender.request_consent(
        "req-timeout-test",
        RiskLevel::DestructiveHighRisk,
        Duration::from_millis(50),
    ).await;

    assert_eq!(decision, ConsentDecision::TimedOut, "Unanswered consent request must time out and fail closed");
}

#[tokio::test]
async fn test_tier2_unregistered_tool_dispatch_error() {
    let dispatcher = MockToolDispatcher::new();
    let res = dispatcher.dispatch("non_existent_tool_xyz", serde_json::json!({})).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("not found"));
}

#[tokio::test]
async fn test_tier2_consent_explicit_denial() {
    let suspender = Arc::new(ConsentSuspender::new());
    let susp_clone = Arc::clone(&suspender);

    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        susp_clone.resolve_consent("req-deny-test", ConsentDecision::Denied {
            reason: "User cancelled high risk action".to_string(),
        }).await;
    });

    let decision = suspender.request_consent(
        "req-deny-test",
        RiskLevel::DestructiveHighRisk,
        Duration::from_secs(1),
    ).await;

    assert_eq!(decision, ConsentDecision::Denied {
        reason: "User cancelled high risk action".to_string(),
    });
}

// ============================================================================
// Tier 3 & Tier 4: Live Hot-Reload, Consent Authority & Dispatcher Pipeline
// ============================================================================

#[tokio::test]
async fn test_tier3_live_skill_watcher_hot_reload() {
    let temp_dir = std::env::temp_dir().join(format!("liva_test_watcher_{}", rand::random::<u32>()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![temp_dir.clone()], Duration::from_millis(20))
        .with_package_store(Arc::clone(&store));
    let mut rx = watcher.subscribe();

    // 1. Create skill v1
    let skill_sub = temp_dir.join("calc-skill");
    tokio::fs::create_dir_all(&skill_sub).await.unwrap();
    let skill_file = skill_sub.join("SKILL.md");

    let skill_v1 = r#"---
name: "calc-skill"
version: "1.0.0"
description: "Safe calculator"
runtime_type: "native_rust"
---
# Calculator v1
"#;
    tokio::fs::write(&skill_file, skill_v1).await.unwrap();

    // Scan pass 1
    let events = watcher.scan_once().await.unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], SkillChangeEvent::Added(_)));
    assert_eq!(store.read().await.count(), 1);

    // Verify broadcast event received
    let broadcast_event = rx.recv().await.unwrap();
    assert!(matches!(broadcast_event, SkillChangeEvent::Added(_)));

    // 2. Modify skill to v2
    let skill_v2 = r#"---
name: "calc-skill"
version: "2.0.0"
description: "Upgraded calculator with trigonometry"
runtime_type: "native_rust"
---
# Calculator v2 with sin/cos
"#;
    tokio::fs::write(&skill_file, skill_v2).await.unwrap();

    // Scan pass 2
    let events2 = watcher.scan_once().await.unwrap();
    assert_eq!(events2.len(), 1);
    match &events2[0] {
        SkillChangeEvent::Modified { new_package, .. } => {
            assert_eq!(new_package.manifest.version, "2.0.0");
        }
        _ => panic!("Expected modified event"),
    }

    let pkg = store.read().await.get("calc-skill").cloned().unwrap();
    assert_eq!(pkg.manifest.version, "2.0.0");

    // Clean up
    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}

#[tokio::test]
async fn test_tier3_unified_dispatcher_with_consent_gating() {
    let consent_mgr = Arc::new(InMemoryConsentManager::new().with_default_timeout(Duration::from_millis(500)));
    let dispatcher = UnifiedToolDispatcher::new()
        .with_consent_authority(Arc::clone(&consent_mgr) as Arc<dyn ConsentAuthority>);

    // Safe tool
    dispatcher.register_tool(SkillToolDefinition {
        name: "get_time".to_string(),
        description: "Returns system time".to_string(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::ReadOnlySafe,
    }).await;

    // High risk tool
    dispatcher.register_tool(SkillToolDefinition {
        name: "format_disk".to_string(),
        description: "Format disk storage".to_string(),
        input_schema: serde_json::json!({}),
        risk_level: RiskLevel::DestructiveHighRisk,
    }).await;

    // Safe tool runs without pause
    let call_safe = ToolCallRequest::new("call-1", "get_time", serde_json::json!({}));
    let res_safe = dispatcher.dispatch(call_safe).await.unwrap();
    assert!(res_safe.success);

    // High risk tool times out if unapproved
    let call_risky = ToolCallRequest::new("call-2", "format_disk", serde_json::json!({}));
    let res_risky = dispatcher.dispatch(call_risky).await.unwrap();
    assert!(!res_risky.success);
    assert!(res_risky.error.unwrap().contains("timed out"));
}

#[tokio::test]
async fn test_tier4_clawhub_skill_parser_trait_validation() {
    let parser = ClawHubSkillParser::new();
    let valid_manifest = r#"---
name: "system-auditor"
version: "1.0.0"
description: "Security audit tool"
runtime_type: "native_rust"
---
# Security Auditor
"#;

    let temp_dir = std::env::temp_dir().join(format!("liva_parser_test_{}", rand::random::<u32>()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();
    let skill_file = temp_dir.join("SKILL.md");
    tokio::fs::write(&skill_file, valid_manifest).await.unwrap();

    let pkg = parser.parse_skill_directory(&temp_dir).unwrap();
    assert_eq!(pkg.manifest.name, "system-auditor");
    assert_eq!(pkg.manifest.version, "1.0.0");
    assert!(parser.validate_permissions(&pkg.manifest).is_ok());

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}
