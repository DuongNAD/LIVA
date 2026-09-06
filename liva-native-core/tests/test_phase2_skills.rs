//! Phase 2 Integration Tests: Features 7-8
//! - Feature 7: Standardized `SKILL.md` Manifest Parser & Capability Permissions (RFC-003 R3)
//! - Feature 8: Live Hot-Reload Watcher (`notify`) & Dynamic Store Swapping (RFC-003 R3)

use liva_native_core::skills::manifest::{
    parse_skill_markdown, ClawHubSkillParser, PermissionRequirement, RiskLevel, SkillError,
    SkillParser, SkillRuntimeType, SkillTrigger,
};
use liva_native_core::skills::store::SkillPackageStore;
use liva_native_core::skills::watcher::{SkillChangeEvent, SkillWatcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::sync::RwLock;

// ============================================================================
// FEATURE 7: STANDARDIZED SKILL.MD MANIFEST (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Parse Complete Valid SKILL.md Package
#[test]
fn test_f7_tier1_parse_valid_skill_md() {
    let skill_text = r#"---
name: "git-commit-helper"
version: "1.0.0"
description: "Formats and validates conventional Git commit messages and runs repository linters."
author: "LIVA Team"
license: "MIT"
runtime_type: "native_rust"

triggers:
  - type: intent
    config: "git_commit"
  - type: keyword
    config: ["commit", "git staged", "tạo commit"]

permissions:
  - type: fs_read
    config: "."
  - type: fs_write
    config: "./target"
  - type: os_execute
    config: "git status"
  - type: net_outbound
    config: "*.github.com"

tools:
  - name: "format_commit_message"
    description: "Validates and structures commit message into conventional commits format."
    risk_level: "read_only_safe"
    input_schema:
      type: object
      properties:
        type:
          type: string
        summary:
          type: string
      required: ["type", "summary"]
---

# Instructions
Always format git commits according to Conventional Commits standard.
"#;

    let dir = PathBuf::from("/skills/git-commit-helper");
    let pkg = parse_skill_markdown(skill_text, &dir).expect("Failed to parse valid SKILL.md");

    assert_eq!(pkg.manifest.name, "git-commit-helper");
    assert_eq!(pkg.manifest.version, "1.0.0");
    assert_eq!(pkg.manifest.description, "Formats and validates conventional Git commit messages and runs repository linters.");
    assert_eq!(pkg.manifest.author, Some("LIVA Team".to_string()));
    assert_eq!(pkg.manifest.license, Some("MIT".to_string()));
    assert_eq!(pkg.manifest.runtime_type, SkillRuntimeType::NativeRust);

    // Triggers
    assert_eq!(pkg.manifest.triggers.len(), 2);
    assert_eq!(pkg.manifest.triggers[0], SkillTrigger::Intent("git_commit".to_string()));
    assert_eq!(pkg.manifest.triggers[1], SkillTrigger::Keyword(vec!["commit".to_string(), "git staged".to_string(), "tạo commit".to_string()]));

    // Permissions
    assert_eq!(pkg.manifest.permissions.len(), 4);
    assert_eq!(pkg.manifest.permissions[0], PermissionRequirement::FsRead(PathBuf::from(".")));
    assert_eq!(pkg.manifest.permissions[1], PermissionRequirement::FsWrite(PathBuf::from("./target")));
    assert_eq!(pkg.manifest.permissions[2], PermissionRequirement::OsExecute("git status".to_string()));
    assert_eq!(pkg.manifest.permissions[3], PermissionRequirement::NetOutbound("*.github.com".to_string()));

    // Tools
    assert_eq!(pkg.manifest.tools.len(), 1);
    assert_eq!(pkg.manifest.tools[0].name, "format_commit_message");
    assert_eq!(pkg.manifest.tools[0].risk_level, RiskLevel::ReadOnlySafe);

    // Body
    assert!(pkg.markdown_instructions.contains("Always format git commits"));
    assert!(!pkg.content_hash.is_empty());
}

/// Tier 1.2: Capability Token Permissions Extraction
#[test]
fn test_f7_tier1_capability_tokens_extraction() {
    let skill_text = r#"---
name: "media-controller"
version: "2.1.0"
description: "Controls screen capture and audio"
permissions:
  - type: vision_capture
  - type: audio_record
  - type: keystore_access
---
# Media instructions
"#;

    let dir = PathBuf::from("/skills/media");
    let pkg = parse_skill_markdown(skill_text, &dir).expect("Failed to parse media skill");

    assert!(pkg.manifest.permissions.contains(&PermissionRequirement::VisionCapture));
    assert!(pkg.manifest.permissions.contains(&PermissionRequirement::AudioRecord));
    assert!(pkg.manifest.permissions.contains(&PermissionRequirement::KeystoreAccess));
}

/// Tier 1.3: Risk Level Hierarchy Parsing
#[test]
fn test_f7_tier1_risk_levels() {
    let skill_text = r#"---
name: "database-tools"
version: "1.0.0"
description: "DB manager"
tools:
  - name: "read_query"
    description: "Read only SQL"
    risk_level: "read_only_safe"
  - name: "update_row"
    description: "Update single row"
    risk_level: "idempotent_action"
  - name: "drop_database"
    description: "Drop full schema"
    risk_level: "destructive_high_risk"
---
# SQL guidelines
"#;

    let dir = PathBuf::from("/skills/db");
    let pkg = parse_skill_markdown(skill_text, &dir).expect("Failed to parse DB tools");

    assert_eq!(pkg.manifest.tools[0].risk_level, RiskLevel::ReadOnlySafe);
    assert_eq!(pkg.manifest.tools[1].risk_level, RiskLevel::IdempotentAction);
    assert_eq!(pkg.manifest.tools[2].risk_level, RiskLevel::DestructiveHighRisk);
}

/// Tier 1.4: Tool JSON Schema Validation
#[test]
fn test_f7_tier1_tool_json_schema() {
    let skill_yaml = r#"---
name: "schema-tool"
version: "1.0.0"
description: "Tool with explicit input schema"
tools:
  - name: "calculate"
    description: "Evaluates mathematical expression"
    risk_level: "read_only_safe"
    input_schema: {"type": "object", "properties": {"expression": {"type": "string"}}}
---
# Instructions
"#;

    let pkg = parse_skill_markdown(skill_yaml, Path::new("/skills/calc")).unwrap();
    let tool = &pkg.manifest.tools[0];

    assert_eq!(tool.input_schema["type"], "object");
    assert_eq!(tool.input_schema["properties"]["expression"]["type"], "string");
}

/// Tier 1.5: Package Content Hash Reproducibility
#[test]
fn test_f7_tier1_package_hash_reproducibility() {
    let skill_text = r#"---
name: "weather-forecast"
version: "1.0.0"
description: "Fetches weather"
---
# Forecast
"#;

    let dir = PathBuf::from("/skills/weather");
    let pkg1 = parse_skill_markdown(skill_text, &dir).unwrap();
    let pkg2 = parse_skill_markdown(skill_text, &dir).unwrap();

    assert_eq!(pkg1.content_hash, pkg2.content_hash);
    assert_eq!(pkg1.content_hash.len(), 64); // SHA-256 hex string length
}

/// Tier 2.1: Missing Frontmatter Delimiters Rejection
#[test]
fn test_f7_tier2_missing_frontmatter_delimiters() {
    let no_opening = r#"name: "bad"
version: "1.0.0"
description: "bad"
---
# Body
"#;

    let dir = PathBuf::from("/skills/invalid");
    let res_no_open = parse_skill_markdown(no_opening, &dir);
    assert!(res_no_open.is_err());
    match res_no_open.unwrap_err() {
        SkillError::ManifestParse(msg) => {
            assert!(msg.contains("YAML delimiter") || msg.contains("---"));
        }
        _ => panic!("Expected ManifestParse error for missing opening delimiter"),
    }

    let no_closing = "---
name: \"bad\"
version: \"1.0.0\"
description: \"bad\"
# No closing delimiter";

    let res_no_close = parse_skill_markdown(no_closing, &dir);
    assert!(res_no_close.is_err());
    match res_no_close.unwrap_err() {
        SkillError::ManifestParse(msg) => {
            assert!(msg.contains("YAML delimiter") || msg.contains("---"));
        }
        _ => panic!("Expected ManifestParse error for missing closing delimiter"),
    }
}

/// Tier 2.2: Path Traversal Permission Rejection
#[test]
fn test_f7_tier2_path_traversal_permission_rejection() {
    let traversal_skill = r#"---
name: "malicious-skill"
version: "1.0.0"
description: "Attempts escape"
permissions:
  - type: fs_write
    config: "../../etc/shadow"
---
# Exploit
"#;

    let parser = ClawHubSkillParser::new();
    let res = parser.parse_skill_markdown(traversal_skill, Path::new("/skills/bad"));
    assert!(res.is_err());
    match res.unwrap_err() {
        SkillError::SecurityViolation(msg) => {
            assert!(msg.contains("traversal") || msg.contains("outside") || msg.contains(".."));
        }
        other => panic!("Expected SecurityViolation error, got: {:?}", other),
    }
}

/// Tier 2.3: Runtime Types Mapping
#[test]
fn test_f7_tier2_runtime_types_mapping() {
    for (yaml_rt, expected_enum) in [
        ("native_rust", SkillRuntimeType::NativeRust),
        ("script_process", SkillRuntimeType::ScriptProcess),
        ("mcp_server", SkillRuntimeType::McpServer),
        ("wasm_module", SkillRuntimeType::WasmModule),
    ] {
        let skill = format!(
            "---\nname: \"test-rt\"\nversion: \"1.0.0\"\ndescription: \"rt\"\nruntime_type: \"{}\"\n---\n# rt",
            yaml_rt
        );
        let pkg = parse_skill_markdown(&skill, Path::new("/skills/rt")).unwrap();
        assert_eq!(pkg.manifest.runtime_type, expected_enum);
    }
}

/// Tier 2.4: Large Markdown Instructions (100KB)
#[test]
fn test_f7_tier2_large_markdown_body() {
    let large_body = "Line of instruction content.\n".repeat(3500); // ~100KB
    let skill_text = format!(
        "---\nname: \"large-skill\"\nversion: \"1.0.0\"\ndescription: \"Large doc skill\"\n---\n{}",
        large_body
    );

    let pkg = parse_skill_markdown(&skill_text, Path::new("/skills/large")).unwrap();
    assert_eq!(pkg.markdown_instructions.len(), large_body.trim().len());
}

/// Tier 2.5: Empty Optional Fields Handling
#[test]
fn test_f7_tier2_empty_optional_fields() {
    let minimal_skill = r#"---
name: "minimal"
version: "0.1.0"
description: "Minimal valid manifest"
---
"#;

    let pkg = parse_skill_markdown(minimal_skill, Path::new("/skills/min")).unwrap();
    assert_eq!(pkg.manifest.name, "minimal");
    assert_eq!(pkg.manifest.author, None);
    assert_eq!(pkg.manifest.license, None);
    assert!(!pkg.manifest.triggers.is_empty(), "Default triggers populated");
}

// ============================================================================
// FEATURE 8: LIVE HOT-RELOAD WATCHER (Tier 1 & Tier 2)
// ============================================================================

fn setup_skill_watcher_env() -> (TempDir, PathBuf, Arc<RwLock<SkillPackageStore>>) {
    let temp_dir = tempfile::tempdir().expect("Failed to create tempdir");
    let skills_root = temp_dir.path().to_path_buf();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    (temp_dir, skills_root, store)
}

/// Tier 1.1: Watcher Discovers Newly Added SKILL.md
#[tokio::test]
async fn test_f8_tier1_watcher_discovers_added_skill() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let skill_dir = skills_root.join("my-new-skill");
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_content = r#"---
name: "my-new-skill"
version: "1.0.0"
description: "Discovered skill"
---
# Instructions
"#;
    fs::write(skill_dir.join("SKILL.md"), skill_content).unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    let events = watcher.scan_once().await.expect("Scan failed");
    assert_eq!(events.len(), 1);

    match &events[0] {
        SkillChangeEvent::Added(pkg) => {
            assert_eq!(pkg.manifest.name, "my-new-skill");
        }
        other => panic!("Expected Added event, got: {:?}", other),
    }

    // Check store in RAM was populated
    let store_guard = store.read().await;
    assert!(store_guard.get("my-new-skill").is_some());
}

/// Tier 1.2: Watcher Emits Modified Event on File Edit
#[tokio::test]
async fn test_f8_tier1_watcher_emits_modified_event() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let skill_dir = skills_root.join("editable-skill");
    fs::create_dir_all(&skill_dir).unwrap();

    let skill_v1 = r#"---
name: "editable-skill"
version: "1.0.0"
description: "Version 1"
---
# V1
"#;
    let skill_file = skill_dir.join("SKILL.md");
    fs::write(&skill_file, skill_v1).unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    // Initial scan
    let events_1 = watcher.scan_once().await.unwrap();
    assert_eq!(events_1.len(), 1);

    // Modify file
    let skill_v2 = r#"---
name: "editable-skill"
version: "2.0.0"
description: "Version 2 modified"
---
# V2 updated
"#;
    fs::write(&skill_file, skill_v2).unwrap();

    // Second scan
    let events_2 = watcher.scan_once().await.unwrap();
    assert_eq!(events_2.len(), 1);

    match &events_2[0] {
        SkillChangeEvent::Modified { old_hash, new_package } => {
            assert_eq!(new_package.manifest.name, "editable-skill");
            assert_eq!(new_package.manifest.version, "2.0.0");
            assert_ne!(old_hash, &new_package.content_hash);
        }
        other => panic!("Expected Modified event, got: {:?}", other),
    }

    // Verify store in RAM has v2
    let store_guard = store.read().await;
    let pkg = store_guard.get("editable-skill").unwrap();
    assert_eq!(pkg.manifest.version, "2.0.0");
}

/// Tier 1.3: Watcher Emits Removed Event on File Deletion
#[tokio::test]
async fn test_f8_tier1_watcher_emits_removed_event() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let skill_dir = skills_root.join("ephemeral-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");

    fs::write(&skill_file, "---\nname: \"ephemeral-skill\"\nversion: \"1.0.0\"\ndescription: \"temp\"\n---\n").unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    // Scan 1
    watcher.scan_once().await.unwrap();
    assert!(store.read().await.get("ephemeral-skill").is_some());

    // Delete file
    fs::remove_file(&skill_file).unwrap();

    // Scan 2
    let events_2 = watcher.scan_once().await.unwrap();
    assert_eq!(events_2.len(), 1);

    match &events_2[0] {
        SkillChangeEvent::Removed { skill_name, .. } => {
            assert_eq!(skill_name, "ephemeral-skill");
        }
        other => panic!("Expected Removed event, got: {:?}", other),
    }

    // Verify removed from store in RAM
    assert!(store.read().await.get("ephemeral-skill").is_none());
}

/// Tier 1.4: Broadcast Channel Subscription
#[tokio::test]
async fn test_f8_tier1_broadcast_channel_subscription() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let watcher = SkillWatcher::new(vec![skills_root.clone()], Duration::from_millis(50))
        .with_package_store(store);

    let mut rx = watcher.subscribe();

    let skill_dir = skills_root.join("sub-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(skill_dir.join("SKILL.md"), "---\nname: \"sub-skill\"\nversion: \"1.0.0\"\ndescription: \"sub\"\n---\n").unwrap();

    watcher.scan_once().await.unwrap();

    let received = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(received.is_ok());
    match received.unwrap().unwrap() {
        SkillChangeEvent::Added(pkg) => {
            assert_eq!(pkg.manifest.name, "sub-skill");
        }
        other => panic!("Expected Added event, got: {:?}", other),
    }
}

/// Tier 1.5: Multi-Skill Directory Scanning
#[tokio::test]
async fn test_f8_tier1_multiple_skills_scan() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();

    for i in 0..5 {
        let dir = skills_root.join(format!("skill-{}", i));
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("SKILL.md"),
            format!("---\nname: \"skill-{}\"\nversion: \"1.0.0\"\ndescription: \"test\"\n---\n", i),
        ).unwrap();
    }

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    let events = watcher.scan_once().await.unwrap();
    assert_eq!(events.len(), 5);
    assert_eq!(store.read().await.list().len(), 5);
}

/// Tier 2.1: Non-SKILL.md Files Ignored
#[tokio::test]
async fn test_f8_tier2_non_skill_files_ignored() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    fs::write(skills_root.join("README.md"), "# Not a skill").unwrap();
    fs::write(skills_root.join("notes.txt"), "some notes").unwrap();
    fs::write(skills_root.join("data.json"), "{}").unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store);

    let events = watcher.scan_once().await.unwrap();
    assert!(events.is_empty(), "Unrelated files should not trigger skill events");
}

/// Tier 2.2: Corrupted File Edit Retains Previous Store Package
#[tokio::test]
async fn test_f8_tier2_corrupt_file_retains_previous_in_store() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let skill_dir = skills_root.join("resilient-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let file = skill_dir.join("SKILL.md");

    // Valid v1
    fs::write(&file, "---\nname: \"resilient-skill\"\nversion: \"1.0.0\"\ndescription: \"valid\"\n---\n").unwrap();
    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    watcher.scan_once().await.unwrap();
    assert!(store.read().await.get("resilient-skill").is_some());

    // Corrupt the file syntax
    fs::write(&file, "MALFORMED CONTENT WITHOUT FRONTMATTER").unwrap();
    let events = watcher.scan_once().await.unwrap();
    assert!(events.is_empty(), "Corrupt file should not produce valid change event");

    // Previous valid package remains in store
    let store_guard = store.read().await;
    let existing = store_guard.get("resilient-skill");
    assert!(existing.is_some(), "Store should retain previous valid package on corrupt edit");
}

/// Tier 2.3: Non-Existent Directory Handling
#[tokio::test]
async fn test_f8_tier2_non_existent_watch_directory() {
    let non_existent = PathBuf::from("/non/existent/path/to/skills");
    let watcher = SkillWatcher::new(vec![non_existent], Duration::from_millis(50));

    let events = watcher.scan_once().await;
    assert!(events.is_ok());
    assert!(events.unwrap().is_empty());
}

/// Tier 2.4: Deeply Nested Directory Scan Depth Limit
#[tokio::test]
async fn test_f8_tier2_deeply_nested_scan() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let deep_dir = skills_root.join("level1").join("level2").join("level3");
    fs::create_dir_all(&deep_dir).unwrap();
    fs::write(
        deep_dir.join("SKILL.md"),
        "---\nname: \"deep-skill\"\nversion: \"1.0.0\"\ndescription: \"deep\"\n---\n",
    ).unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store.clone());

    let events = watcher.scan_once().await.unwrap();
    assert_eq!(events.len(), 1);
    assert_eq!(events[0], SkillChangeEvent::Added(store.read().await.get("deep-skill").unwrap().clone()));
}

/// Tier 2.5: No-Op Scan When Files Unchanged
#[tokio::test]
async fn test_f8_tier2_noop_scan_when_unchanged() {
    let (_temp, skills_root, store) = setup_skill_watcher_env();
    let skill_dir = skills_root.join("static-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    fs::write(
        skill_dir.join("SKILL.md"),
        "---\nname: \"static-skill\"\nversion: \"1.0.0\"\ndescription: \"static\"\n---\n",
    ).unwrap();

    let watcher = SkillWatcher::new(vec![skills_root], Duration::from_millis(50))
        .with_package_store(store);

    let events_1 = watcher.scan_once().await.unwrap();
    assert_eq!(events_1.len(), 1);

    // Second scan with no disk modifications produces 0 events
    let events_2 = watcher.scan_once().await.unwrap();
    assert!(events_2.is_empty(), "Subsequent scan without changes should produce 0 events");
}
