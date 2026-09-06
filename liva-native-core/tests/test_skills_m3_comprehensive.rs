//! Comprehensive Milestone 3 Integration & Security Verification Suite.
//! Tests ClawHub SKILL.md YAML Parsing, Capability Tokens, Security Guardrails, Hot-Reload Watcher, and Unified Tool Dispatcher.

use liva_native_core::skills::{
    ClawHubSkillParser, RiskLevel, SkillChangeEvent, SkillError, SkillPackageStore, SkillParser,
    SkillRuntimeType, SkillWatcher, ToolCallRequest, ToolDispatcher, UnifiedToolDispatcher,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;

#[tokio::test]
async fn test_m3_manifest_full_capability_tokens() {
    let yaml = r#"---
name: "full-capability-agent"
version: "3.5.0"
description: "Demonstrates all capability tokens and structured schema configuration."
author: "LIVA Core Team"
license: "MIT"
runtime_type: "native_rust"

triggers:
  - type: intent
    config: "analyze_codebase"
  - type: keyword
    config: ["analyze", "inspect", "audit"]
  - type: regex
    config: "^analyze\\s+.*"
  - type: cron
    config: "0 0 * * *"
  - type: event
    config: "git:push"

permissions:
  - type: fs_read
    config: "./src"
  - type: fs_write
    config: "./target/output"
  - type: net_outbound
    config: "*.github.com"
  - type: os_execute
    config: "cargo check"
  - vision_capture
  - audio_record
  - keystore_access

tools:
  - name: "analyze_syntax"
    description: "Parses Rust AST"
    risk_level: "read_only_safe"
    input_schema:
      type: object
      properties:
        target_path:
          type: string
      required: ["target_path"]
  - name: "mutate_ast"
    description: "Applies automated AST refactoring"
    risk_level: "destructive_high_risk"
---
# Full Capability Agent Instructions
Execute deep analysis and safe refactoring.
"#;

    let dir = PathBuf::from("/tmp/full-capability-agent");
    let parser = ClawHubSkillParser::new();
    let pkg = parser.parse_skill_markdown(yaml, &dir).expect("Manifest should parse cleanly");

    assert_eq!(pkg.manifest.name, "full-capability-agent");
    assert_eq!(pkg.manifest.version, "3.5.0");
    assert_eq!(pkg.manifest.runtime_type, SkillRuntimeType::NativeRust);
    assert_eq!(pkg.manifest.triggers.len(), 5);
    assert_eq!(pkg.manifest.permissions.len(), 7);
    assert_eq!(pkg.manifest.tools.len(), 2);
    assert_eq!(pkg.content_hash.len(), 64);
    assert!(parser.validate_permissions(&pkg.manifest).is_ok());
}

#[tokio::test]
async fn test_m3_security_rejections_and_guardrails() {
    let parser = ClawHubSkillParser::new();

    // 1. Path traversal via `..`
    let traversal_yaml = r#"---
name: "jailbreak-skill"
version: "1.0.0"
description: "Tries directory traversal"
permissions:
  - type: fs_read
    config: "../../../etc/shadow"
---
Instructions
"#;
    let res1 = parser.parse_skill_markdown(traversal_yaml, Path::new("/tmp"));
    assert!(matches!(res1, Err(SkillError::SecurityViolation(_))));

    // 2. Sensitive root access (/etc)
    let root_yaml = r#"---
name: "jailbreak-etc"
version: "1.0.0"
description: "Tries /etc write"
permissions:
  - type: fs_write
    config: "/etc/passwd"
---
Instructions
"#;
    let res2 = parser.parse_skill_markdown(root_yaml, Path::new("/tmp"));
    assert!(matches!(res2, Err(SkillError::SecurityViolation(_))));

    // 3. Destructive command (rm -rf /)
    let rm_yaml = r#"---
name: "wipe-skill"
version: "1.0.0"
description: "Tries rm -rf"
permissions:
  - type: os_execute
    config: "rm -rf /"
---
Instructions
"#;
    let res3 = parser.parse_skill_markdown(rm_yaml, Path::new("/tmp"));
    assert!(matches!(res3, Err(SkillError::SecurityViolation(_))));

    // 4. SSRF Cloud Metadata (169.254.169.254)
    let ssrf_yaml = r#"---
name: "ssrf-skill"
version: "1.0.0"
description: "Tries cloud metadata theft"
permissions:
  - type: net_outbound
    config: "169.254.169.254"
---
Instructions
"#;
    let res4 = parser.parse_skill_markdown(ssrf_yaml, Path::new("/tmp"));
    assert!(matches!(res4, Err(SkillError::SecurityViolation(_))));

    // 5. Invalid skill name with path separator
    let invalid_name_yaml = r#"---
name: "bad/name/escape"
version: "1.0.0"
description: "Invalid name"
---
Instructions
"#;
    let res5 = parser.parse_skill_markdown(invalid_name_yaml, Path::new("/tmp"));
    assert!(matches!(res5, Err(SkillError::SecurityViolation(_))));
}

#[tokio::test]
async fn test_m3_live_hot_reloading_with_dispatcher_sync() {
    let temp_dir = std::env::temp_dir().join(format!("liva_m3_hotreload_test_{}", rand::random::<u32>()));
    tokio::fs::create_dir_all(&temp_dir).await.unwrap();

    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = Arc::new(
        SkillWatcher::with_default_debounce(vec![temp_dir.clone()])
            .with_package_store(Arc::clone(&store)),
    );

    let dispatcher = Arc::new(UnifiedToolDispatcher::new());
    let rx = watcher.subscribe();
    let _stream_handle = Arc::clone(&dispatcher).attach_watcher_stream(rx);

    // Initial state: 0 tools
    assert_eq!(dispatcher.list_tools().await.unwrap().len(), 0);

    // 1. Write skill package on disk
    let skill_dir = temp_dir.join("analytics-skill");
    tokio::fs::create_dir_all(&skill_dir).await.unwrap();
    let skill_file = skill_dir.join("SKILL.md");

    let skill_v1 = r#"---
name: "analytics-skill"
version: "1.0.0"
description: "Analytics tool"
runtime_type: "native_rust"
tools:
  - name: "query_metrics"
    description: "Queries system metrics"
    risk_level: "read_only_safe"
---
# Analytics
"#;
    tokio::fs::write(&skill_file, skill_v1).await.unwrap();

    // Trigger scan pass
    let events = watcher.scan_once().await.unwrap();
    assert_eq!(events.len(), 1);
    assert!(matches!(events[0], SkillChangeEvent::Added(_)));

    // Let the async stream handler process the event
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify dispatcher has auto-registered the tool!
    assert!(dispatcher.has_tool("query_metrics").await);
    let tool_def = dispatcher.get_tool("query_metrics").await.unwrap();
    assert_eq!(tool_def.risk_level, RiskLevel::ReadOnlySafe);

    // Execute tool through dispatcher
    let call_req = ToolCallRequest::new("call-101", "query_metrics", serde_json::json!({"metric": "cpu"}));
    let res = dispatcher.dispatch(call_req).await.unwrap();
    assert!(res.success);

    // 2. Modify skill to v2 with an extra tool
    let skill_v2 = r#"---
name: "analytics-skill"
version: "2.0.0"
description: "Upgraded analytics tool"
runtime_type: "native_rust"
tools:
  - name: "query_metrics"
    description: "Queries system metrics v2"
    risk_level: "read_only_safe"
  - name: "export_report"
    description: "Exports PDF report"
    risk_level: "idempotent_action"
---
# Analytics v2
"#;
    tokio::fs::write(&skill_file, skill_v2).await.unwrap();

    let events2 = watcher.scan_once().await.unwrap();
    assert_eq!(events2.len(), 1);
    assert!(matches!(events2[0], SkillChangeEvent::Modified { .. }));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify both tools are present now
    assert!(dispatcher.has_tool("query_metrics").await);
    assert!(dispatcher.has_tool("export_report").await);
    assert_eq!(dispatcher.list_tools().await.unwrap().len(), 2);

    // 3. Remove skill
    tokio::fs::remove_file(&skill_file).await.unwrap();
    tokio::fs::remove_dir_all(&skill_dir).await.unwrap();

    let events3 = watcher.scan_once().await.unwrap();
    assert_eq!(events3.len(), 1);
    assert!(matches!(events3[0], SkillChangeEvent::Removed { .. }));

    tokio::time::sleep(Duration::from_millis(50)).await;

    // Verify tools were auto-unregistered from dispatcher
    assert!(!dispatcher.has_tool("query_metrics").await);
    assert!(!dispatcher.has_tool("export_report").await);
    assert_eq!(dispatcher.list_tools().await.unwrap().len(), 0);

    let _ = tokio::fs::remove_dir_all(&temp_dir).await;
}
