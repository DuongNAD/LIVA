//! Empirical Challenger Adversarial Test Suite for Milestone 2 Preview:
//! 1. Malformed YAML/MD skill manifests, missing metadata, large logs (>10MB), and ClawHub installation edge cases.
//! 2. Browser automation preview with malicious URLs (SSRF attempts like `http://169.254.169.254`, `file:///etc/passwd`), invalid viewports, and rapid screenshot polling.
//! 3. Concurrency, thread safety, and bounded memory retention.

use liva_native_core::automation::sandbox::{SandboxGuard, SandboxPolicy};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::skills::manifest::{
    ClawHubSkillParser, RiskLevel, SkillError, SkillParser, SkillRuntimeType,
};
use liva_native_core::{
    AppState, CommandPrincipal, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::path::Path;
use std::sync::Arc;

fn test_app_state() -> Arc<AppState> {
    let db = db::DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

// =========================================================================
// AREA 1: Skill Manifest Parsing, Permissions & Store Edge Cases
// =========================================================================

#[test]
fn test_adversarial_skill_manifest_syntax_and_missing_metadata() {
    let parser = ClawHubSkillParser::new();

    // 1. Missing leading delimiter ---
    let no_leading = "name: test-skill\nversion: 1.0.0\n---\n# Content";
    let res = parser.parse_skill_markdown(no_leading, Path::new("/tmp/test"));
    assert!(matches!(res, Err(SkillError::ManifestParse(msg)) if msg.contains("Missing leading YAML delimiter")));

    // 2. Missing closing delimiter ---
    let no_closing = "---\nname: test-skill\nversion: 1.0.0\n# Content without closing delimiter";
    let res = parser.parse_skill_markdown(no_closing, Path::new("/tmp/test"));
    assert!(matches!(res, Err(SkillError::ManifestParse(msg)) if msg.contains("Missing closing YAML delimiter")));

    // 3. Missing name field
    let missing_name = "---\nversion: 1.0.0\ndescription: A skill without name\n---\n# Instructions";
    let res = parser.parse_skill_markdown(missing_name, Path::new("/tmp/test"));
    assert!(matches!(res, Err(SkillError::ManifestParse(msg)) if msg.contains("Missing required field: name")));

    // 4. Missing version field
    let missing_version = "---\nname: valid-name\ndescription: No version here\n---\n# Instructions";
    let res = parser.parse_skill_markdown(missing_version, Path::new("/tmp/test"));
    assert!(matches!(res, Err(SkillError::ManifestParse(msg)) if msg.contains("Missing required field: version")));

    // 5. Unknown runtime type
    let invalid_runtime = "---\nname: bad-runtime\nversion: 1.0.0\nruntime_type: quantum_vm\n---\n# Instructions";
    let res = parser.parse_skill_markdown(invalid_runtime, Path::new("/tmp/test"));
    assert!(matches!(res, Err(SkillError::ManifestParse(msg)) if msg.contains("Unknown runtime_type")));

    // 6. Valid manifest with Unicode BOM prefix
    let bom_manifest = "\u{feff}---\nname: bom-skill\nversion: 2.1.0\ndescription: UTF8 with BOM\n---\n# Instructions";
    let res = parser.parse_skill_markdown(bom_manifest, Path::new("/tmp/test"));
    assert!(res.is_ok());
    let pkg = res.unwrap();
    assert_eq!(pkg.manifest.name, "bom-skill");
    assert_eq!(pkg.manifest.version, "2.1.0");
    assert_eq!(pkg.manifest.runtime_type, SkillRuntimeType::NativeRust);

    // 7. Structured tools with diverse risk levels
    let structured_yaml = "---
name: multi-tool-skill
version: 1.5.0
description: Skill exporting multiple tools
tools:
  - name: safe_reader
    description: Reads data safely
    risk_level: safe
  - name: modify_record
    description: Idempotent updates
    risk_level: idempotent_action
  - name: purge_database
    description: Destructive wipe
    risk_level: destructive_high_risk
---
# Instructions
Execute tools according to policies.
";
    let pkg = parser.parse_skill_markdown(structured_yaml, Path::new("/tmp/multi")).unwrap();
    assert_eq!(pkg.manifest.tools.len(), 3);
    assert_eq!(pkg.manifest.tools[0].risk_level, RiskLevel::ReadOnlySafe);
    assert_eq!(pkg.manifest.tools[1].risk_level, RiskLevel::IdempotentAction);
    assert_eq!(pkg.manifest.tools[2].risk_level, RiskLevel::DestructiveHighRisk);
}

#[test]
fn test_adversarial_skill_permissions_security_validation() {
    let parser = ClawHubSkillParser::new();

    // 1. Unsafe path traversal in fs_write
    let unsafe_path_manifest = "---\nname: traversal-skill\nversion: 1.0.0\ndescription: Test traversal\npermissions:\n  - fs_write: ../../etc/shadow\n---\n# Body";
    let res = parser.parse_skill_markdown(unsafe_path_manifest, Path::new("/tmp"));
    assert!(matches!(res, Err(SkillError::SecurityViolation(msg)) if msg.contains("Unsafe path traversal")));

    // 2. Unsafe system root write in fs_write (/etc/passwd)
    let unsafe_etc_manifest = "---\nname: etc-skill\nversion: 1.0.0\ndescription: Test etc\npermissions:\n  - fs_write: /etc/passwd\n---\n# Body";
    let res = parser.parse_skill_markdown(unsafe_etc_manifest, Path::new("/tmp"));
    assert!(matches!(res, Err(SkillError::SecurityViolation(msg)) if msg.contains("Forbidden system directory access")));

    // 3. Destructive command in os_execute (rm -rf /)
    let destructive_cmd_manifest = "---\nname: nuke-skill\nversion: 1.0.0\ndescription: Test nuke\npermissions:\n  - os_execute: rm -rf /var/lib\n---\n# Body";
    let res = parser.parse_skill_markdown(destructive_cmd_manifest, Path::new("/tmp"));
    assert!(matches!(res, Err(SkillError::SecurityViolation(msg)) if msg.contains("Forbidden destructive command")));

    // 4. Fork bomb in os_execute
    let fork_bomb_manifest = "---\nname: fork-skill\nversion: 1.0.0\ndescription: Test fork bomb\npermissions:\n  - os_execute: :(){ :|:& };:\n---\n# Body";
    let res = parser.parse_skill_markdown(fork_bomb_manifest, Path::new("/tmp"));
    assert!(matches!(res, Err(SkillError::SecurityViolation(msg)) if msg.contains("Forbidden destructive command")));

    // 5. Safe permissions (net_outbound, fs_read, keystore_access)
    let safe_manifest = "---\nname: safe-skill\nversion: 1.0.0\ndescription: Test safe\npermissions:\n  - net_outbound: api.anthropic.com\n  - fs_read: ./data/input.json\n  - keystore_access\n---\n# Safe Body";
    let pkg = parser.parse_skill_markdown(safe_manifest, Path::new("/tmp")).unwrap();
    assert!(parser.validate_permissions(&pkg.manifest).is_ok());
}

#[test]
fn test_adversarial_large_skill_manifest_stress_10mb() {
    let parser = ClawHubSkillParser::new();

    // Construct 10MB of markdown instructions
    let frontmatter = "---\nname: massive-doc-skill\nversion: 3.0.0\ndescription: Heavy documentation stress test\n---\n\n";
    let chunk = "LIVA Autonomous Cognitive Engine instructions step line.\n";
    let repeat_count = (10 * 1024 * 1024) / chunk.len();
    let mut large_content = String::with_capacity(frontmatter.len() + repeat_count * chunk.len());
    large_content.push_str(frontmatter);
    for _ in 0..repeat_count {
        large_content.push_str(chunk);
    }

    assert!(large_content.len() >= 10 * 1024 * 1024);

    let start = std::time::Instant::now();
    let pkg = parser
        .parse_skill_markdown(&large_content, Path::new("/skills/massive"))
        .expect("should parse 10MB manifest without crashing");
    let elapsed = start.elapsed();

    assert_eq!(pkg.manifest.name, "massive-doc-skill");
    assert_eq!(pkg.manifest.version, "3.0.0");
    assert_eq!(pkg.content_hash.len(), 64); // Valid SHA-256
    assert!(elapsed.as_millis() < 1000, "10MB parsing took too long: {:?}", elapsed);
}

#[tokio::test]
async fn test_adversarial_skill_store_ipc_edge_cases() {
    let state = test_app_state();

    // 1. skills:get_manifest for non-existent skill returns synthetic fallback with valid hash
    let syn_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:get_manifest",
        json!({ "skillId": "non-existent-synthetic-skill" }),
        None,
        None,
    )
    .await
    .expect("synthetic manifest fallback");

    assert_eq!(syn_res["skillId"], "non-existent-synthetic-skill");
    assert_eq!(syn_res["runtimeType"], "native_rust");
    assert!(!syn_res["contentHash"].as_str().unwrap().is_empty());

    // 2. skills:get_manifest missing skillId & name fails gracefully
    let err_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:get_manifest",
        json!({}),
        None,
        None,
    )
    .await;
    assert!(err_res.is_err());

    // 3. skills:save_config and get_config with huge custom parameter payload
    let mut huge_map = serde_json::Map::new();
    for i in 0..500 {
        huge_map.insert(format!("param_{i}"), json!(format!("value_{i}_configured")));
    }
    huge_map.insert("timeoutSeconds".to_string(), json!(120));
    huge_map.insert("maxRetries".to_string(), json!(10));

    let save_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:save_config",
        json!({
            "skillId": "heavy-config-skill",
            "params": huge_map
        }),
        None,
        None,
    )
    .await
    .expect("save heavy skill config");

    assert!(save_res["success"].as_bool().unwrap());

    let get_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:get_config",
        json!({ "skillId": "heavy-config-skill" }),
        None,
        None,
    )
    .await
    .expect("get heavy skill config");

    assert_eq!(get_res["params"]["timeoutSeconds"], 120);
    assert_eq!(get_res["params"]["maxRetries"], 10);
    assert_eq!(get_res["params"]["param_499"], "value_499_configured");

    // 4. skills:logs with massive limit parameter (1,000,000)
    let logs_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:logs",
        json!({ "skillId": "heavy-config-skill", "limit": 1_000_000 }),
        None,
        None,
    )
    .await
    .expect("logs with large limit");

    let count = logs_res["count"].as_u64().unwrap();
    assert!(count <= 20, "Returned logs count must be bounded");

    // 5. skills:install_from_hub edge case
    let hub_install = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "skills:install_from_hub",
        json!({
            "name": "clawhub-crypto-ticker",
            "repoUrl": "https://hub.openclaw.ai/skills/clawhub-crypto-ticker"
        }),
        None,
        None,
    )
    .await
    .expect("install clawhub crypto ticker");

    assert!(hub_install["success"].as_bool().unwrap());
    assert_eq!(hub_install["skillId"], "clawhub-crypto-ticker");
}

// =========================================================================
// AREA 2: Browser Automation Preview, SSRF & Viewport Edge Cases
// =========================================================================

#[test]
fn test_adversarial_browser_ssrf_and_malicious_urls() {
    let policy = SandboxPolicy::default();
    let guard = SandboxGuard::new(policy);

    // 1. Cloud Instance Metadata Endpoints (AWS, GCP, Azure, OpenStack)
    let cloud_metadata_targets = [
        "http://169.254.169.254/latest/meta-data/",
        "https://169.254.169.254/computeMetadata/v1/",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://169.254.169.254.nip.io/",
        "http://[::ffff:169.254.169.254]/",
    ];
    for url in cloud_metadata_targets {
        let res = guard.validate_url(url);
        assert!(
            res.is_err(),
            "Cloud metadata endpoint MUST be blocked by SSRF Guard: {url}"
        );
    }

    // 2. Loopback and Localhost Endpoints
    let loopback_targets = [
        "http://127.0.0.1:8080/admin",
        "http://127.0.0.1/",
        "http://127.1.2.3/",
        "http://localhost:3000/",
        "http://localhost:5173/",
        "http://0.0.0.0:8000/",
        "http://[::1]:8080/",
        "http://sub.localhost:8080/",
    ];
    for url in loopback_targets {
        let res = guard.validate_url(url);
        assert!(
            res.is_err(),
            "Localhost loopback endpoint MUST be blocked by SSRF Guard: {url}"
        );
    }

    // 3. RFC-1918 Private Subnets
    let private_subnet_targets = [
        "http://10.0.0.1/gateway",
        "http://10.255.255.254/",
        "http://192.168.1.1/router-login",
        "http://192.168.0.100:8080/",
        "http://172.16.0.1/intranet",
        "http://172.24.10.5/",
        "http://172.31.255.254/secret",
    ];
    for url in private_subnet_targets {
        let res = guard.validate_url(url);
        assert!(
            res.is_err(),
            "Private RFC-1918 subnet endpoint MUST be blocked by SSRF Guard: {url}"
        );
    }

    // 4. Internal / Local Hostnames
    let internal_hostnames = [
        "http://router.local/",
        "http://nas.local:5000/",
        "http://corp.internal/db",
        "http://vault.corp/",
    ];
    for url in internal_hostnames {
        let res = guard.validate_url(url);
        assert!(
            res.is_err(),
            "Internal domain suffix MUST be blocked by SSRF Guard: {url}"
        );
    }

    // 5. Valid Public Web URLs
    let valid_public_urls = [
        "https://github.com/rust-lang/rust",
        "https://doc.rust-lang.org/book/",
        "https://crates.io/crates/serde",
        "https://liva.ai/dashboard",
    ];
    for url in valid_public_urls {
        let res = guard.validate_url(url);
        assert!(
            res.is_ok(),
            "Valid public URL must pass SSRF Guard: {url}"
        );
    }
}

#[tokio::test]
async fn test_adversarial_browser_navigation_ssrf_rejection() {
    let state = test_app_state();

    // 1. Attempt SSRF via browser:navigate
    let ssrf_attempts = [
        "http://169.254.169.254/latest/meta-data/",
        "http://127.0.0.1:9090/",
        "http://localhost:3000/",
        "http://192.168.1.1/admin",
        "http://10.0.0.1/",
    ];

    for bad_url in ssrf_attempts {
        let nav_res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "browser:navigate",
            json!({ "url": bad_url }),
            None,
            None,
        )
        .await;

        assert!(
            nav_res.is_err(),
            "browser:navigate MUST reject SSRF target '{bad_url}'"
        );
        let err_msg = nav_res.unwrap_err();
        assert!(
            err_msg.contains("Security policy") || err_msg.contains("SSRF"),
            "Error message must specify security violation for '{bad_url}': {err_msg}"
        );
    }

    // 2. Legitimate URL navigation succeeds
    let ok_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "https://crates.io/crates/tokio" }),
        None,
        None,
    )
    .await
    .expect("legitimate navigation");

    assert_eq!(ok_res["url"], "https://crates.io/crates/tokio");
    assert_eq!(ok_res["httpStatus"], 200);
}

#[tokio::test]
async fn test_adversarial_browser_rapid_polling_and_concurrency_stress() {
    let state = test_app_state();

    // Rapid concurrent screenshot & status polling (100 parallel tasks)
    let mut handles = Vec::new();
    for _ in 0..50 {
        let st = state.clone();
        handles.push(tokio::spawn(async move {
            handle_command_as(
                CommandPrincipal::TauriDashboard,
                st,
                "browser:screenshot",
                json!({}),
                None,
                None,
            )
            .await
        }));

        let st2 = state.clone();
        handles.push(tokio::spawn(async move {
            handle_command_as(
                CommandPrincipal::TauriDashboard,
                st2,
                "browser:status",
                json!({}),
                None,
                None,
            )
            .await
        }));
    }

    for h in handles {
        let res = h.await.expect("task panicked");
        assert!(res.is_ok(), "Concurrent screenshot/status request failed: {:?}", res);
        let val = res.unwrap();
        if val.get("base64Png").is_some() {
            assert!(val["base64Png"].as_str().unwrap().starts_with("data:image/png;base64,"));
        } else {
            assert!(val["isRunning"].as_bool().unwrap());
        }
    }

    // Verify action log ring buffer does not exceed max capacity (50)
    let logs_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:action_log",
        json!({}),
        None,
        None,
    )
    .await
    .expect("browser action logs");

    let count = logs_res["count"].as_u64().unwrap();
    assert!(count <= 50, "Action logs ring buffer must be bounded at 50, got {count}");
}

#[tokio::test]
async fn test_adversarial_browser_session_state_transitions() {
    let state = test_app_state();

    // Rapid pause / resume / stop cycles
    let control_sequence = [
        ("pause", "paused"),
        ("resume", "running"),
        ("pause", "paused"),
        ("resume", "running"),
        ("stop", "stopped"),
    ];

    for (action, expected_state) in control_sequence {
        let res = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "browser:control",
            json!({ "action": action }),
            None,
            None,
        )
        .await
        .expect("control action");

        assert_eq!(res["state"], expected_state);
    }

    // Clear logs
    let clear_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "clear_logs" }),
        None,
        None,
    )
    .await
    .expect("clear logs");

    assert!(clear_res["cleared"].as_bool().unwrap());

    let logs_after = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:action_log",
        json!({}),
        None,
        None,
    )
    .await
    .expect("logs after clear");

    let count = logs_after["count"].as_u64().unwrap();
    assert!(count <= 5, "Logs after clear should be reset (got {count})");
}
