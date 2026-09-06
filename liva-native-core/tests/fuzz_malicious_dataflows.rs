//! Milestone 4 Fuzzing Suite: Adversarial & Malicious Dataflow Fuzzing
//!
//! Comprehensive property-based, mutation, and adversarial fuzz testing for:
//! 1. IPC frame decoders, JSON AST repair, and JSON-RPC dispatching.
//! 2. Malformed / corrupted Unified Diffs, truncated headers, out-of-bounds coordinates, and path traversals.
//! 3. SSRF bypass payloads, cloud metadata evasion, IP parser confusion, and capability token attenuation.
//! 4. Cyclic state graphs, loop detection, and infinite recursion guards in Pregel runtime.
//! 5. Adversarial Swarm payloads, bloated vector clocks, circular delegations, and MVCC conflict matrices.
//!
//! Verifies zero panics, clean error propagation, and memory safety.

use liva_native_core::agent::graph::checkpoint::SqliteCheckpointer;
use liva_native_core::agent::graph::pregel::{LivaAgentRuntime, NodeError};
use liva_native_core::agent::graph::{
    DiffReviewRegistry, DiffReviewSession, HunkStatus, parse_unified_diff,
    reconstruct_approved_patch,
};
use liva_native_core::agent::state::AgentState;
use liva_native_core::agent::swarm::{
    CausalRelation, ConflictResolutionStrategy, DelegationError, DelegationToken,
    MvccTransactionCoordinator, SwarmRole, ThreeWayMerger, VectorClock,
};
use liva_native_core::ast_repair::json_repair::{
    repair_json_ast, repair_json_ast_with_stats, repair_json_string,
};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::sandbox::policy::{
    CapabilityToken, SandboxPolicy, validate_command,
};
use liva_native_core::sandbox::ssrf_filter::SsrfFilter;
use liva_native_core::{
    AppState, CommandPrincipal, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::collections::HashSet;
use std::net::IpAddr;
use std::sync::Arc;

fn test_state() -> Arc<AppState> {
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
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

// ============================================================================
// 1. IPC FRAME DECODER & JSON AST REPAIR FUZZING
// ============================================================================

#[test]
fn test_fuzz_json_ast_repair_and_decoders() {
    let malformed_json_corpus = [
        // 1. Trailing commas
        r#"{"a": 1, "b": [2, 3,], "c": {"d": 4,},}"#,
        r#"{"arr": [1, 2, 3, , ,], "obj": {"k": "v",,}}"#,
        r#"[1, 2, 3,]"#,

        // 2. Single quotes & unescaped quotes
        r#"{'cmd': 'write_file', 'path': 'doc.txt', 'text': 'it\'s working'}"#,
        r#"{"title": "The "Great" Gatsby", "status": "ok"}"#,
        r#"['item1', 'item2', 'item3']"#,

        // 3. Unquoted keys
        r#"{device: "light", state: true, brightness_level: 80, step-id: "s1"}"#,
        r#"{user_name: "alice", role: "admin", is_active: true}"#,

        // 4. Equal signs instead of colons
        r#"{"key" = "value", "count" => 42}"#,
        r#"{action = "run", timeout = 30}"#,

        // 5. Pythonic and JS literals
        r#"{"is_active": True, "timeout": None, "flag": False, "undefined_val": undefined, "nan_val": NaN}"#,
        r#"{"inf": Infinity, "+inf": +Infinity, "-inf": -Infinity, "nil_val": nil}"#,

        // 6. Markdown fences & leading/trailing prose
        r#"Here is the response: ```json {"result": "success", "code": 200} ``` Hope this helps!"#,
        r#"``` {"model": "llama", "tokens": 1024} ```"#,

        // 7. Comments
        "{\n  // User configuration\n  \"user\": \"admin\",\n  /* Multi-line\n     setting */\n  \"port\": 8080\n}",

        // 8. Truncated containers at EOF
        r#"{"a": 1, "b": [2, 3, {"c": 4"#,
        r#"[{"item": 1}, {"item": 2"#,
        r#"{"key": "value", "nested": {"deep": [1, 2"#,
        r#"{"unclosed_str": "hello world"#,
        r#"{ a: 1, b: 2"#,
        r#"device: light, state: on"#,

        // 9. Boundary numbers and control characters
        r#"{"big_int": 999999999999999999, "neg": -12345, "float": 3.14159, "exp": 1e10}"#,
        "{\"multiline\": \"line1\\nline2\\ttabbed\\rcarriage\"}",

        // 10. Bare key-value pairs
        r#"task_id: 123, status: "pending", priority: "high""#,
    ];

    for (idx, sample) in malformed_json_corpus.iter().enumerate() {
        let res = repair_json_ast(sample);
        assert!(
            res.is_ok(),
            "Malformed JSON sample {} ('{}') should repair successfully: {:?}",
            idx,
            sample,
            res
        );

        let (val, stats) = repair_json_ast_with_stats(sample).expect("stats");
        assert!(val.is_object() || val.is_array() || val.is_string() || val.is_number() || val.is_boolean() || val.is_null());
        assert!(stats.repaired_len > 0);
        let _ = repair_json_string(sample);
    }

    // Deeply nested container fuzzing
    let mut deep_obj = String::new();
    for _ in 0..50 {
        deep_obj.push_str("{\"k\":[");
    }
    deep_obj.push_str("42");
    for _ in 0..50 {
        deep_obj.push_str("]}");
    }
    let deep_res = repair_json_ast(&deep_obj);
    assert!(deep_res.is_ok(), "Deeply nested JSON should parse cleanly");

    // Invalid non-JSON inputs must fail cleanly without panic
    let non_json_inputs = [
        "",
        "   ",
        "Just arbitrary English text without any key or structure",
        "Hello World!",
    ];

    for input in &non_json_inputs {
        let res = repair_json_ast(input);
        assert!(res.is_err(), "Non-JSON input '{}' should return Err", input);
    }
}

#[tokio::test]
async fn test_fuzz_ipc_command_payloads_and_principals() {
    let state = test_state();

    let fuzz_payloads = vec![
        json!(null),
        json!("string_instead_of_object"),
        json!(12345),
        json!(-99999),
        json!(3.1415926535),
        json!([]),
        json!([1, "two", null, {}]),
        json!({}),
        json!({"session_id": null, "hunk_id": null}),
        json!({"session_id": "", "hunk_id": ""}),
        json!({"session_id": "non_existent_session_9999", "hunk_id": "hunk-0"}),
        json!({"session_id": "sess_1", "batch": "unknown_batch_action"}),
        json!({"widget_id": "../../../../etc/passwd", "html": "<script>alert(1)</script>"}),
        json!({"split_ratio": -999.0, "active_mode": "\0\0\0"}),
        json!({"split_ratio": 999999.9, "active_mode": "invalid"}),
        json!({"thread_id": "t1", "patch_id": "p1", "hunks": []}),
        json!({"channel": "unknown_channel", "token": "evil_token"}),
        json!({"query": "\0\0\0", "limit": -100}),
        json!({"query": "A".repeat(10_000), "limit": 1_000_000}),
        json!({"message_id": "m1", "role": "invalid_role", "text": null}),
    ];

    let commands = [
        "diff:get_pending_hunks",
        "diff:get_session",
        "diff:parse_raw_diff",
        "agent:submit_hunk_decision",
        "canvas:stream_widget",
        "canvas:get_canvas_state",
        "canvas:update_widget_state",
        "canvas:close_widget",
        "canvas:set_layout",
        "channels:status",
        "config:get",
        "pairing:status",
        "invalid_module:invalid_verb",
        "",
        "::::",
        "unknown",
    ];

    let principals = [
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::TauriWidget,
        CommandPrincipal::WebSocketDashboard,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::Telegram,
        CommandPrincipal::Test,
    ];

    for cmd in &commands {
        for payload in &fuzz_payloads {
            for principal in &principals {
                // Must safely return Ok or Err without panicking
                let result = handle_command_as(
                    *principal,
                    state.clone(),
                    cmd,
                    payload.clone(),
                    None,
                    None,
                )
                .await;

                match result {
                    Ok(v) => assert!(v.is_object() || v.is_array() || v.is_null() || v.is_string() || v.is_boolean() || v.is_number()),
                    Err(err) => assert!(!err.is_empty(), "Error message should not be empty for command: {}", cmd),
                }
            }
        }
    }
}

// ============================================================================
// 2. MALFORMED UNIFIED DIFF & TRUNCATED HEADER FUZZING
// ============================================================================

#[test]
fn test_fuzz_malformed_unified_diffs_and_headers() {
    let long_a = "A".repeat(10_000);
    let long_b = "B".repeat(10_000);
    let long_c = "C".repeat(50_000);
    let long_diff = format!("--- a/{}\n+++ b/{}\n@@ -1,1 +1,1 @@\n+{}", long_a, long_b, long_c);
    let repeated_hunks = "@@ -1,1 +1,1 @@\n+".repeat(500);

    let malicious_corpus = vec![
        "",
        "   ",
        "\0\0\0\0",
        "\r\n\r\n\r\n",
        "@@ -0,0 +0,0 @@",
        "@@ -1,999999999999999999999999999999999 +1,1 @@",
        "@@ -18446744073709551615,18446744073709551615 +18446744073709551615,18446744073709551615 @@",
        "@@ -invalid,invalid +invalid,invalid @@",
        "@@ --10,-20 +-30,-40 @@",
        "@@ @@",
        "@@ -1,5 @@",
        "@@ +1,5 @@",
        "@@ -1,1 +1,1",
        "--- a/file\n+++ b/file\n@@ -1,1 +1,1 @@\n+line\n-line\n\\ No newline",
        "--- \0/etc/shadow\n+++ \0/etc/shadow\n@@ -1,1 +1,1 @@",
        "--- a/../../../../../../etc/passwd\n+++ b/../../../../../../etc/passwd\n@@ -1,1 +1,1 @@\n+evil",
        "--- a/test\n+++ b/test\n@@ -1,1 +1,1 @@\n+line1\n--- a/test2\n+++ b/test2\n@@ -invalid",
        "diff --git a/f b/f\n--- a/f\n+++ b/f\n@@ -1,2 +1,2 @@\n\n\n\n\n",
        &long_diff,
        &repeated_hunks,
        "diff --git a/file1 b/file1\n--- a/file1\n+++ /dev/null\n@@ -1,3 +0,0 @@\n-a\n-b\n-c\n",
        "diff --git a/file2 b/file2\n--- /dev/null\n+++ b/file2\n@@ -0,0 +1,2 @@\n+x\n+y\n",
    ];

    for (idx, sample) in malicious_corpus.iter().enumerate() {
        let result = parse_unified_diff(sample);
        match result {
            Ok(files) => {
                // If parsed, reconstructor must never panic under any hunk status combination
                let _ = reconstruct_approved_patch(&files);

                // Mutate hunk statuses
                let mut mutated_files = files.clone();
                for f in &mut mutated_files {
                    for (h_idx, h) in f.hunks.iter_mut().enumerate() {
                        match h_idx % 4 {
                            0 => h.status = HunkStatus::Approved,
                            1 => h.status = HunkStatus::Rejected { reason: Some("Fuzz reject".to_string()) },
                            2 => h.status = HunkStatus::Modified { user_override: "let custom = true;\n".to_string() },
                            _ => h.status = HunkStatus::Pending,
                        }
                    }
                }
                let reconstructed = reconstruct_approved_patch(&mutated_files);
                assert!(reconstructed.is_ok(), "Reconstruction must succeed on parsed files (sample {})", idx);
            }
            Err(e) => {
                assert!(!e.is_empty(), "Fuzz sample {} error string should not be empty", idx);
            }
        }
    }
}

#[test]
fn test_fuzz_diff_registry_and_sessions() {
    let registry = DiffReviewRegistry::new();

    // 1. Create and manage 200 sessions with boundary IDs
    for i in 0..200 {
        let session_id = format!("sess_{}_\0_id", i);
        let sample_diff = format!(
            "--- a/src/mod_{}.rs\n+++ b/src/mod_{}.rs\n@@ -1,2 +1,3 @@\n-old\n+new_{}\n+extra\n",
            i, i, i
        );
        let files = parse_unified_diff(&sample_diff).expect("valid diff");
        let session = DiffReviewSession::new(&session_id, format!("th_{}", i), format!("act_{}", i), files);
        registry.create_session(session);

        // Submit decisions
        let current = registry.get_session(&session_id).unwrap();
        let hunk_id = &current.files[0].hunks[0].hunk_id;

        if i % 3 == 0 {
            let updated = registry.submit_decision(&session_id, hunk_id, HunkStatus::Approved).unwrap();
            assert!(updated.is_fully_decided());
        } else if i % 3 == 1 {
            let updated = registry.submit_decision(
                &session_id,
                hunk_id,
                HunkStatus::Rejected { reason: Some("Rejected".to_string()) },
            ).unwrap();
            assert!(updated.is_fully_decided());
        } else {
            let updated = registry.submit_batch_decisions(&session_id, "approve_all").unwrap();
            assert!(updated.is_fully_decided());
        }

        // Cleanup
        registry.remove_session(&session_id);
    }

    assert_eq!(registry.list_sessions().len(), 0);
}

// ============================================================================
// 3. SSRF BYPASS PAYLOADS & CAPABILITY TOKEN FUZZING
// ============================================================================

#[test]
fn test_fuzz_ssrf_bypass_vectors() {
    let filter = SsrfFilter::new();

    let malicious_ssrf_urls = [
        // AWS & Cloud Metadata
        "http://169.254.169.254/latest/meta-data/",
        "http://169.254.169.254/latest/user-data",
        "http://169.254.169.250/metadata",
        "http://100.100.100.200/latest/meta-data/",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://metadata.google/computeMetadata/v1/instance/",
        "https://169.254.169.254.nip.io/",

        // IPv4 Loopback and Private subnets
        "http://127.0.0.1:8080/admin",
        "http://127.0.0.2:3000",
        "http://127.127.127.127",
        "http://0.0.0.0:80",
        "http://localhost:8080",
        "http://10.0.0.1/internal",
        "http://10.255.255.255/secret",
        "http://172.16.0.1:9090",
        "http://172.31.255.255",
        "http://192.168.0.1/router",
        "http://192.168.1.254",

        // CGNAT (RFC 6598)
        "http://100.64.0.1",
        "http://100.127.255.254",

        // IPv6 Loopback, Link-Local, Mapped
        "http://[::1]:8080/metrics",
        "http://[::]:3000",
        "http://[fe80::1]/",
        "http://[fc00::1]/",
        "http://[fd00::1]/",
        "http://[::ffff:127.0.0.1]/",
        "http://[::ffff:169.254.169.254]/",

        // Disallowed Schemes
        "file:///etc/passwd",
        "file:///C:/Windows/System32/drivers/etc/hosts",
        "gopher://127.0.0.1:6379/_FLUSHALL",
        "ftp://attacker.com/file",
        "data:text/html,<script>alert(1)</script>",
        "javascript:alert(document.cookie)",
        "dict://127.0.0.1:11211/stat",
        "ldap://127.0.0.1:389/o=anonymous",

        // Userinfo and obfuscation attempts
        "http://victim.com@127.0.0.1/",
        "http://169.254.169.254@legitimate.com/",
        "http://user:pass@localhost:8080",
        "hTTp://127.0.0.1:80",
        "HTTP://METADATA.GOOGLE.INTERNAL/",
    ];

    for url in &malicious_ssrf_urls {
        let result = filter.validate_url(url);
        assert!(
            result.is_err(),
            "Malicious SSRF vector '{}' MUST be rejected by SsrfFilter",
            url
        );
    }

    // Allowed legitimate public domains MUST pass
    let legitimate_urls = [
        "https://api.github.com/repos/rust-lang/rust",
        "https://crates.io/api/v1/crates",
        "https://raw.githubusercontent.com/user/repo/main/README.md",
        "https://api.openai.com/v1/models",
    ];

    for url in &legitimate_urls {
        assert!(
            filter.validate_url(url).is_ok(),
            "Legitimate URL '{}' should be allowed",
            url
        );
    }

    // Direct IP filtering verification
    assert!(!filter.is_ip_allowed("127.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("169.254.169.254".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("10.0.0.1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("172.20.0.1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("192.168.1.1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("100.64.0.1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("::1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("fe80::1".parse::<IpAddr>().unwrap()));
    assert!(!filter.is_ip_allowed("fc00::1".parse::<IpAddr>().unwrap()));
    assert!(filter.is_ip_allowed("1.1.1.1".parse::<IpAddr>().unwrap()));
    assert!(filter.is_ip_allowed("8.8.8.8".parse::<IpAddr>().unwrap()));
    assert!(filter.is_ip_allowed("2606:4700:4700::1111".parse::<IpAddr>().unwrap()));
}

#[test]
fn test_fuzz_capability_token_attenuation_and_commands() {
    // 1. CapabilityToken parsing from arbitrary strings
    let token_strings = [
        ("fs_read", Ok(CapabilityToken::FsRead)),
        ("fs-read", Ok(CapabilityToken::FsRead)),
        ("FS_WRITE", Ok(CapabilityToken::FsWrite)),
        ("net_outbound", Ok(CapabilityToken::NetOutbound)),
        ("os_execute", Ok(CapabilityToken::OsExecute)),
        ("vision_capture", Ok(CapabilityToken::VisionCapture)),
        ("audio_record", Ok(CapabilityToken::AudioRecord)),
        ("keystore_access", Ok(CapabilityToken::KeystoreAccess)),
        ("invalid_cap", Err(())),
        ("root_superuser", Err(())),
        ("", Err(())),
        ("\0", Err(())),
    ];

    for (input, expected) in &token_strings {
        let parsed = input.parse::<CapabilityToken>();
        match expected {
            Ok(token) => assert_eq!(parsed.unwrap(), *token),
            Err(_) => assert!(parsed.is_err()),
        }
    }

    // 2. Policy attenuation non-escalation invariant
    let mut parent_caps = HashSet::new();
    parent_caps.insert(CapabilityToken::FsRead);
    parent_caps.insert(CapabilityToken::FsWrite);

    let parent_policy = SandboxPolicy {
        capabilities: parent_caps,
        max_execution_time_secs: 30,
        max_memory_mb: 64,
        ..SandboxPolicy::default()
    };

    // Child attempts to escalate privileges
    let mut bad_caps = HashSet::new();
    bad_caps.insert(CapabilityToken::OsExecute);
    let escalation_request = SandboxPolicy {
        capabilities: bad_caps,
        ..SandboxPolicy::default()
    };
    assert!(parent_policy.attenuate(&escalation_request).is_err());

    // 3. Command AST sanitizer fuzzing
    let dangerous_commands = [
        ("rm", vec!["-rf".to_string(), "/".to_string()]),
        ("rm", vec!["--recursive".to_string(), "target".to_string()]),
        ("mkfs.ext4", vec!["/dev/sda1".to_string()]),
        ("dd", vec!["if=/dev/zero".to_string(), "of=/dev/sda".to_string()]),
        ("shutdown", vec!["-h".to_string(), "now".to_string()]),
        ("reboot", vec![]),
        ("cargo", vec!["check".to_string(), "--manifest-path".to_string(), "../../etc/shadow".to_string()]),
        ("sh", vec!["-c".to_string(), ":(){ :|:& };:".to_string()]),
    ];

    for (cmd, args) in &dangerous_commands {
        assert!(
            validate_command(cmd, args).is_err(),
            "Dangerous command '{} {:?}' MUST be blocked",
            cmd,
            args
        );
    }
}

// ============================================================================
// 4. CYCLIC STATE GRAPHS & PREGEL RUNTIME FUZZING
// ============================================================================

#[tokio::test]
async fn test_fuzz_cyclic_state_graphs_and_loop_detection() {
    // 1. 2-Node cyclic loop: A -> B -> A
    let mut runtime_2node = LivaAgentRuntime::new();
    runtime_2node.set_max_cycles_per_node(4);
    runtime_2node.set_max_steps(20);

    runtime_2node.add_node("node_a", |mut s: AgentState| async move {
        s.increment_step();
        Ok(s)
    });
    runtime_2node.add_node("node_b", |mut s: AgentState| async move {
        s.increment_step();
        Ok(s)
    });

    runtime_2node.add_edge("node_a", "node_b");
    runtime_2node.add_edge("node_b", "node_a");
    runtime_2node.set_entry_point("node_a");

    let result_2node = runtime_2node.run(AgentState::default()).await;
    match result_2node {
        Err(NodeError::Fatal(msg)) => {
            assert!(msg.contains("Dynamic loop detected"), "Should detect dynamic loop: {}", msg);
        }
        _ => panic!("Expected dynamic loop detection error, got {:?}", result_2node),
    }

    // 2. 3-Node cycle: A -> B -> C -> A
    let mut runtime_3node = LivaAgentRuntime::new();
    runtime_3node.set_max_cycles_per_node(3);
    runtime_3node.set_max_steps(30);

    runtime_3node.add_node("step_a", |mut s: AgentState| async move { s.increment_step(); Ok(s) });
    runtime_3node.add_node("step_b", |mut s: AgentState| async move { s.increment_step(); Ok(s) });
    runtime_3node.add_node("step_c", |mut s: AgentState| async move { s.increment_step(); Ok(s) });

    runtime_3node.add_edge("step_a", "step_b");
    runtime_3node.add_edge("step_b", "step_c");
    runtime_3node.add_edge("step_c", "step_a");
    runtime_3node.set_entry_point("step_a");

    let result_3node = runtime_3node.run(AgentState::default()).await;
    assert!(matches!(result_3node, Err(NodeError::Fatal(_))));

    // 3. Max total steps exhaustion guard
    let mut runtime_max_steps = LivaAgentRuntime::new();
    runtime_max_steps.set_max_steps(10);
    runtime_max_steps.set_max_cycles_per_node(100); // cycle limit high, step limit low

    runtime_max_steps.add_node("loop_node", |mut s: AgentState| async move { s.increment_step(); Ok(s) });
    runtime_max_steps.add_edge("loop_node", "loop_node");
    runtime_max_steps.set_entry_point("loop_node");

    let result_steps = runtime_max_steps.run(AgentState::default()).await;
    match result_steps {
        Err(NodeError::Timeout(msg)) => {
            assert!(msg.contains("exceeded maximum allowable steps"));
        }
        _ => panic!("Expected step timeout error, got {:?}", result_steps),
    }

    // 4. Parallel Superstep branching with state merging under stress
    let mut parallel_runtime = LivaAgentRuntime::new();
    parallel_runtime.add_node("start", |s: AgentState| async move { Ok(s) });

    for i in 0..10 {
        let node_id = format!("worker_{}", i);
        parallel_runtime.add_node(&node_id, move |mut s: AgentState| async move {
            s.scratchpad_set(format!("key_{}", i), json!(i * 100));
            Ok(s)
        });
    }

    let workers: Vec<String> = (0..10).map(|i| format!("worker_{}", i)).collect();
    parallel_runtime.add_parallel_edge("start", workers);
    parallel_runtime.set_merge_fn(|mut base: AgentState, branches: Vec<AgentState>| {
        for b in branches {
            for (k, v) in b.scratchpad {
                base.scratchpad.insert(k, v);
            }
        }
        base
    });
    parallel_runtime.set_entry_point("start");

    let parallel_res = parallel_runtime.run(AgentState::default()).await.expect("parallel run");
    assert_eq!(parallel_res.scratchpad.len(), 10);
    assert_eq!(parallel_res.scratchpad.get("key_5"), Some(&json!(500)));
}

// ============================================================================
// 5. ADVERSARIAL SWARM PAYLOADS & VECTOR CLOCK FUZZING
// ============================================================================

#[tokio::test]
async fn test_fuzz_adversarial_swarm_and_vector_clocks() {
    // 1. Bloated VectorClock (10,000 actors)
    let mut large_clock = VectorClock::new();
    for i in 0..10_000 {
        large_clock.set(&format!("actor_{}", i), (i % 1000) as u64);
    }
    assert_eq!(large_clock.get("actor_500"), 500);

    let mut concurrent_clock = VectorClock::new();
    concurrent_clock.set("actor_0", 999);
    assert_eq!(large_clock.relation(&concurrent_clock), CausalRelation::Concurrent);

    // Merge vector clocks under contention
    large_clock.merge(&concurrent_clock);
    assert_eq!(large_clock.get("actor_0"), 999);

    // 2. Cyclical subagent delegation depth fuzzing
    let mut root_token = DelegationToken::create_root(
        "planner_root",
        SwarmRole::Planner,
        "root_task",
        3, // Max depth = 3
        100_000,
        100,
        60_000,
        HashSet::new(),
        1000,
    );

    let mut hop1 = root_token
        .sub_delegate("coder_1", SwarmRole::Coder, "t1", "desc", 20_000, 20, HashSet::new(), 30_000, 1050)
        .expect("hop 1");
    assert_eq!(hop1.current_depth, 1);

    let mut hop2 = hop1
        .sub_delegate("reviewer_1", SwarmRole::Reviewer, "t2", "desc", 10_000, 10, HashSet::new(), 15_000, 1100)
        .expect("hop 2");
    assert_eq!(hop2.current_depth, 2);

    // Exceeding max depth MUST return DelegationError::MaxDepthExceeded
    let hop_overflow = hop2.sub_delegate("auditor_1", SwarmRole::Auditor, "t3", "desc", 5_000, 5, HashSet::new(), 5_000, 1150);
    assert!(matches!(hop_overflow, Err(DelegationError::MaxDepthExceeded { .. })));

    // Budget token exhaustion MUST fail closed
    let budget_overflow = hop1.sub_delegate("sub_greedy", SwarmRole::Coder, "t_greedy", "desc", 500_000, 50, HashSet::new(), 5_000, 1100);
    assert!(matches!(budget_overflow, Err(DelegationError::InsufficientTokenBudget { .. })));

    // 3. ThreeWayMerger with adversarial JSON conflict structures
    let merger = ThreeWayMerger::new(ConflictResolutionStrategy::DeepMergeLww);

    // Array conflicting modifications
    let base_arr = json!({"list": [1, 2, 3]});
    let ours_arr = json!({"list": [1, 2, 3, 4]});
    let theirs_arr = json!({"list": [1, 2, 3, 5]});
    let arr_merge = merger.merge(&base_arr, &ours_arr, &theirs_arr).expect("merge array");
    assert!(arr_merge.merged_state.get("list").is_some());

    // Type conflict (object vs array vs primitive)
    let base_type = json!({"field": "string"});
    let ours_type = json!({"field": [1, 2, 3]});
    let theirs_type = json!({"field": {"k": "v"}});
    let type_merge = merger.merge(&base_type, &ours_type, &theirs_type).expect("merge type conflict");
    assert!(type_merge.merged_state.get("field").is_some());

    // 4. MVCC Transaction Coordinator under concurrent commits
    let pool = Arc::new(db::DatabasePool::new_in_memory().unwrap());
    let enc = EncryptionEngine::new("checkpoint-fuzz-key-32-bytes-long");
    let cp = Arc::new(SqliteCheckpointer::new(pool, enc));
    let mvcc = MvccTransactionCoordinator::new(cp.clone(), ConflictResolutionStrategy::DeepMergeLww);

    let tid = "thread_fuzz_mvcc";
    let mut base_st = AgentState::default();
    base_st.scratchpad_set("data", json!("init"));
    let clock_base = VectorClock::from_actor("root", 1);

    let init_commit = mvcc.commit_state(
        tid,
        &base_st,
        &clock_base,
        &base_st,
        &clock_base,
        "init_node",
        None,
        None,
    ).await;
    assert!(init_commit.is_ok());

    let mut coder_st = base_st.clone();
    coder_st.scratchpad_set("coder_key", json!("c_val"));
    let mut coder_clk = clock_base.clone();
    coder_clk.tick("coder");

    let mut rev_st = base_st.clone();
    rev_st.scratchpad_set("rev_key", json!("r_val"));
    let mut rev_clk = clock_base.clone();
    rev_clk.tick("reviewer");

    // Reviewer commits
    let rev_commit = mvcc.commit_state(
        tid,
        &base_st,
        &clock_base,
        &rev_st,
        &rev_clk,
        "rev_node",
        None,
        None,
    ).await;
    assert!(rev_commit.is_ok());

    // Coder commits concurrently with base -> triggers 3-way merge
    let coder_commit = mvcc.commit_state(
        tid,
        &base_st,
        &clock_base,
        &coder_st,
        &coder_clk,
        "coder_node",
        None,
        None,
    ).await;
    assert!(coder_commit.is_ok());
    let res = coder_commit.unwrap();
    assert!(res.was_merged);
    assert_eq!(res.final_state.scratchpad_get("coder_key"), Some(&json!("c_val")));
    assert_eq!(res.final_state.scratchpad_get("rev_key"), Some(&json!("r_val")));
}
