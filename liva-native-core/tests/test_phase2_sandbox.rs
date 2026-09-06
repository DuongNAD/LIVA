//! Phase 2 Integration Tests: Features 9-10
//! - Feature 9: Tier 1 Wasmtime WASI-0.2 Sandbox (64MB/Fuel/Epoch) (RFC-003 R4)
//! - Feature 10: Tier 2 OS Containment (macOS Seatbelt & Linux Bwrap) & Policy Enforcement (RFC-003 R4)

use liva_native_core::sandbox::policy::{
    validate_command, CapabilityToken, CanonicalPathValidator, SandboxPolicy, SandboxViolation,
    SsrfFilter,
};
use liva_native_core::sandbox::tier1_wasm::{
    EpochTicker, WasmSandboxConfig, WasmSandboxError, WasmSandboxRunner, WASM_ENGINE,
};
use liva_native_core::sandbox::tier2_os::macos_seatbelt::generate_sbpl_profile;
use liva_native_core::sandbox::tier2_os::{OsSandboxPolicy, OsSandboxRunner};
use std::collections::HashSet;
use std::fs;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

// ============================================================================
// FEATURE 9: TIER 1 WASMTIME WASI-0.2 SANDBOX (Tier 1 & Tier 2)
// ============================================================================

// Minimal valid WASM bytecode for: (module (func (export "run") (result i32) i32.const 42))
const MINIMAL_VALID_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // Magic + Version
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,       // Type section: () -> i32
    0x03, 0x02, 0x01, 0x00,                         // Function section
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // Export "run"
    0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b, // Code: i32.const 42, end
];

// WASM bytecode for infinite loop: (module (func (export "run") (result i32) (loop (br 0)) (i32.const 0)))
const INFINITE_LOOP_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, // Magic & version
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,       // Type: () -> i32
    0x03, 0x02, 0x01, 0x00,                         // Function: type 0
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00, // Export "run"
    0x0a, 0x0b, 0x01, 0x09, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x41, 0x00, 0x0b, // Code
];

/// Tier 1.1: Wasm Engine Initialization & Configuration
#[test]
fn test_f9_tier1_wasm_engine_init() {
    let runner = WasmSandboxRunner::new();
    assert!(runner.is_ok(), "WasmSandboxRunner initialization should succeed");
}

/// Tier 1.2: Wasm Sandbox Default Resource Limits (64MB Hard Cap)
#[test]
fn test_f9_tier1_wasm_sandbox_default_limits() {
    let config = WasmSandboxConfig::default();
    assert_eq!(config.memory_limit_bytes, 64 * 1024 * 1024); // 64 MB
    assert_eq!(config.fuel_limit, 100_000_000);             // 100M fuel
    assert_eq!(config.epoch_deadline_ticks, 10);            // 10 ticks
}

/// Tier 1.3: Custom Sandbox Limits Configuration
#[test]
fn test_f9_tier1_custom_sandbox_limits() {
    let config = WasmSandboxConfig {
        memory_limit_bytes: 32 * 1024 * 1024,
        fuel_limit: 50_000_000,
        epoch_deadline_ticks: 5,
        allowed_hosts: vec!["api.anthropic.com".to_string()],
        allowed_paths: vec![PathBuf::from("/workspace/data")],
        ..WasmSandboxConfig::default()
    };

    assert_eq!(config.memory_limit_bytes, 32 * 1024 * 1024);
    assert_eq!(config.fuel_limit, 50_000_000);
    assert_eq!(config.allowed_hosts.len(), 1);
    assert_eq!(config.allowed_paths.len(), 1);
}

/// Tier 1.4: Execute Valid WebAssembly Core Module
#[tokio::test]
async fn test_f9_tier1_execute_valid_wasm_module() {
    let runner = WasmSandboxRunner::new().expect("Runner init failed");
    let config = WasmSandboxConfig::default();

    let res = runner.execute_module(MINIMAL_VALID_WASM, &config, b"").await;
    assert!(res.is_ok(), "Valid WASM module should execute successfully");
}

/// Tier 1.5: Epoch Ticker Background Task Spawning
#[tokio::test]
async fn test_f9_tier1_epoch_ticker_spawn() {
    let engine = WASM_ENGINE.clone();
    let handle = EpochTicker::spawn(engine, Duration::from_millis(10));
    tokio::time::sleep(Duration::from_millis(30)).await;
    handle.abort();
    let _ = handle.await; // Await task cancellation
}

/// Tier 2.1: Invalid Wasm Bytecode Rejection
#[tokio::test]
async fn test_f9_tier2_invalid_wasm_bytecode() {
    let runner = WasmSandboxRunner::new().unwrap();
    let config = WasmSandboxConfig::default();
    let corrupted_wasm = b"NOT_WASM_BYTECODE";

    let res = runner.execute_module(corrupted_wasm, &config, b"").await;
    assert!(res.is_err());
    match res.unwrap_err() {
        WasmSandboxError::Compilation(_) => {}
        other => panic!("Expected Compilation error, got: {:?}", other),
    }
}

/// Tier 2.2: Fuel Limit Throttles & Traps Low-Budget Module
#[tokio::test]
async fn test_f9_tier2_fuel_exhaustion_trap() {
    let runner = WasmSandboxRunner::new().unwrap();
    let config = WasmSandboxConfig {
        fuel_limit: 10_000,
        epoch_deadline_ticks: 1000,
        ..WasmSandboxConfig::default()
    };

    let res = runner.execute_module(INFINITE_LOOP_WASM, &config, b"").await;
    assert!(res.is_err(), "Loop module should exhaust 10000 fuel units and trap");
    match res.unwrap_err() {
        WasmSandboxError::Trap(msg) | WasmSandboxError::Execution(msg) => {
            let lower = msg.to_lowercase();
            assert!(lower.contains("fuel") || lower.contains("trap") || lower.contains("consumed") || lower.contains("exhaust"));
        }
        other => panic!("Expected Fuel Trap error, got: {:?}", other),
    }
}

/// Tier 2.3: Epoch Deadline Interruption on Infinite Loop
#[tokio::test]
async fn test_f9_tier2_epoch_deadline_interruption() {
    let runner = WasmSandboxRunner::new().unwrap();
    let ticker = EpochTicker::spawn(WASM_ENGINE.clone(), Duration::from_millis(5));

    let config = WasmSandboxConfig {
        epoch_deadline_ticks: 2, // 10ms deadline
        fuel_limit: 1_000_000_000, // plenty of fuel so epoch interrupts first
        ..WasmSandboxConfig::default()
    };

    let start = std::time::Instant::now();
    let res = runner.execute_module(INFINITE_LOOP_WASM, &config, b"").await;
    let elapsed = start.elapsed();
    ticker.abort();

    assert!(elapsed < Duration::from_millis(500), "Interruption should happen quickly (<500ms)");
    assert!(res.is_err());
    match res.unwrap_err() {
        WasmSandboxError::Trap(msg) | WasmSandboxError::Execution(msg) => {
            let lower = msg.to_lowercase();
            assert!(lower.contains("epoch") || lower.contains("deadline") || lower.contains("interrupted") || lower.contains("trap"));
        }
        other => panic!("Expected Epoch Trap error, got: {:?}", other),
    }
}

/// Tier 2.4: 64MB Memory Cap Enforcement
#[test]
fn test_f9_tier2_store_limits_64mb_bound() {
    let config = WasmSandboxConfig::default();
    let _store_limits = config.to_store_limits();
    // Memory size bound is 64MB
    assert_eq!(config.memory_limit_bytes, 64 * 1024 * 1024);
}

/// Tier 2.5: Concurrent Multi-Instance Wasm Execution Isolation
#[tokio::test]
async fn test_f9_tier2_concurrent_wasm_isolation() {
    let runner = Arc::new(WasmSandboxRunner::new().unwrap());
    let mut handles = Vec::new();

    for _ in 0..10 {
        let r = runner.clone();
        let handle = tokio::spawn(async move {
            let config = WasmSandboxConfig::default();
            let res = r.execute_module(MINIMAL_VALID_WASM, &config, b"").await;
            assert!(res.is_ok());
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ============================================================================
// FEATURE 10: TIER 2 OS CONTAINMENT & SECURITY POLICY (Tier 1 & Tier 2)
// ============================================================================

/// Tier 1.1: Allowed Commands Validation
#[test]
fn test_f10_tier1_allowed_commands() {
    assert!(validate_command("cargo", &["check".to_string()]).is_ok());
    assert!(validate_command("git", &["status".to_string()]).is_ok());
    assert!(validate_command("python", &["--version".to_string()]).is_ok());
    assert!(validate_command("rustc", &["--version".to_string()]).is_ok());
}

/// Tier 1.2: Destructive / Forbidden Commands Blocked
#[test]
fn test_f10_tier1_destructive_commands_blocked() {
    let res_rm = validate_command("rm", &["-rf".to_string(), "/".to_string()]);
    assert!(res_rm.is_err());
    match res_rm.unwrap_err() {
        SandboxViolation::DestructiveCommand(_) => {}
        other => panic!("Expected DestructiveCommand, got: {:?}", other),
    }

    let res_mkfs = validate_command("mkfs", &["/dev/sda".to_string()]);
    assert!(res_mkfs.is_err());

    let res_dd = validate_command("dd", &["if=/dev/zero".to_string()]);
    assert!(res_dd.is_err());
}

/// Tier 1.3: CanonicalPathValidator Workspace Boundary
#[test]
fn test_f10_tier1_canonical_path_validator() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().canonicalize().unwrap();
    let validator = CanonicalPathValidator::new(&root).unwrap();

    let inside_file = root.join("src").join("main.rs");
    fs::create_dir_all(inside_file.parent().unwrap()).unwrap();
    fs::write(&inside_file, b"content").unwrap();

    // Valid path inside workspace
    assert!(validator.validate_read(&inside_file).is_ok());
    assert!(validator.validate_write(&inside_file).is_ok());

    // Path outside workspace
    let outside_path = PathBuf::from("/etc/passwd");
    assert!(validator.validate_read(&outside_path).is_err());
    assert!(validator.validate_write(&outside_path).is_err());
}

/// Tier 1.4: SSRF Filter Blocks Private IPs & Cloud Metadata
#[test]
fn test_f10_tier1_ssrf_filter() {
    let filter = SsrfFilter::new();

    // Cloud metadata endpoint (AWS / GCP / Azure)
    let aws_meta: IpAddr = "169.254.169.254".parse().unwrap();
    assert!(!filter.is_ip_allowed(aws_meta), "AWS metadata IP must be blocked");

    // Loopback
    let loopback: IpAddr = "127.0.0.1".parse().unwrap();
    assert!(!filter.is_ip_allowed(loopback), "Loopback IP must be blocked");

    // Private subnets (RFC 1918)
    let priv_10: IpAddr = "10.0.0.1".parse().unwrap();
    let priv_172: IpAddr = "172.16.0.1".parse().unwrap();
    let priv_192: IpAddr = "192.168.1.1".parse().unwrap();
    assert!(!filter.is_ip_allowed(priv_10));
    assert!(!filter.is_ip_allowed(priv_172));
    assert!(!filter.is_ip_allowed(priv_192));

    // Public Internet IP allowed
    let public_ip: IpAddr = "8.8.8.8".parse().unwrap();
    assert!(filter.is_ip_allowed(public_ip));
}

/// Tier 1.5: SSRF URL Validation
#[test]
fn test_f10_tier1_ssrf_url_validation() {
    let filter = SsrfFilter::new();

    // Blocked URLs
    assert!(filter.validate_url("http://169.254.169.254/latest/meta-data/").is_err());
    assert!(filter.validate_url("http://localhost:8002/admin").is_err());
    assert!(filter.validate_url("http://127.0.0.1:8080/secret").is_err());
    assert!(filter.validate_url("http://192.168.1.100/router").is_err());

    // Allowed public URLs
    assert!(filter.validate_url("https://api.github.com/repos").is_ok());
    assert!(filter.validate_url("https://crates.io/api/v1").is_ok());
}

/// Tier 2.1: Relative Path Traversal Jailbreak Interception
#[test]
fn test_f10_tier2_relative_path_traversal() {
    let temp_dir = tempfile::tempdir().unwrap();
    let root = temp_dir.path().canonicalize().unwrap();
    let validator = CanonicalPathValidator::new(&root).unwrap();

    let traversal_path = root.join("..").join("..").join("etc").join("shadow");
    let res = validator.validate_read(&traversal_path);
    assert!(res.is_err());
    match res.unwrap_err() {
        SandboxViolation::PathJailbreak(_) => {}
        other => panic!("Expected PathJailbreak, got: {:?}", other),
    }
}

/// Tier 2.2: macOS Seatbelt SBPL Profile Generation
#[test]
fn test_f10_tier2_seatbelt_sbpl_profile_generation() {
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![PathBuf::from("/System/Library"), PathBuf::from("/Users/duongnad/LIVA")],
        allowed_write_paths: vec![PathBuf::from("/Users/duongnad/LIVA/target")],
        allowed_commands: vec!["git".to_string(), "cargo".to_string()],
        allow_network: false,
    };

    let profile = generate_sbpl_profile(&policy, Path::new("/usr/bin/git"));
    assert!(profile.contains("(version 1)"));
    assert!(profile.contains("(deny default)"));
    assert!(profile.contains("(allow file-read*"));
    assert!(profile.contains("(allow file-write*"));
    assert!(profile.contains("(deny network*)"));
    assert!(profile.contains("/Users/duongnad/LIVA/target"));
}

/// Tier 2.3: Disguised CLI Parameter Path Traversal
#[test]
fn test_f10_tier2_cli_parameter_traversal_detection() {
    let res = validate_command("cargo", &[
        "--manifest-path".to_string(),
        "../../../../etc/passwd".to_string(),
    ]);
    assert!(res.is_err());
    match res.unwrap_err() {
        SandboxViolation::PathJailbreak(_) => {}
        other => panic!("Expected PathJailbreak, got: {:?}", other),
    }
}

/// Tier 2.4: Capability Token Set Policy Validation
#[test]
fn test_f10_tier2_capability_token_policy_check() {
    let mut caps = HashSet::new();
    caps.insert(CapabilityToken::FsRead);
    caps.insert(CapabilityToken::NetOutbound);

    let policy = SandboxPolicy::new(caps, PathBuf::from("/workspace"));

    assert!(policy.has_capability(CapabilityToken::FsRead));
    assert!(policy.has_capability(CapabilityToken::NetOutbound));
    assert!(!policy.has_capability(CapabilityToken::FsWrite));
    assert!(!policy.has_capability(CapabilityToken::OsExecute));

    assert!(policy.require_capability(CapabilityToken::FsRead).is_ok());
    assert!(policy.require_capability(CapabilityToken::OsExecute).is_err());
}

/// Tier 2.5: OS Sandbox Runner Safe Command Execution
#[tokio::test]
async fn test_f10_tier2_os_sandbox_runner_execution() {
    let runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![PathBuf::from(".")],
        allowed_write_paths: vec![],
        allowed_commands: vec!["echo".to_string()],
        allow_network: false,
    };

    let res = runner.execute_command("echo", &["LIVA_OS_SANDBOX_OK".to_string()], &policy).await;
    assert!(res.is_ok(), "OS runner execution should return structured OsExecutionResult");
    let output = res.unwrap();
    assert!(output.execution_time < Duration::from_secs(5));
}
