//! Phase 2 Challenger Gate 2 Empirical Verification & Adversarial Stress Suite
//!
//! Thoroughly stress-tests:
//! 1. Tier 1 Wasmtime Sandbox: 64MB memory cap (`StoreLimitsBuilder`), fuel exhaustion traps, epoch deadline interruption, and engine stability.
//! 2. Tier 2 OS Containment: Path traversal, symlink escapes, CLI parameter jailbreaks, destructive command blocking, and SSRF filters.
//! 3. Skill System & Live Hot-Reloading: Notify watcher, debouncing, SHA-256 fingerprint diffs, and store synchronization.

use liva_native_core::sandbox::policy::{
    validate_command, CanonicalPathValidator, SandboxViolation, SsrfFilter,
};
use liva_native_core::sandbox::tier1_wasm::{
    EpochTicker, WasmSandboxConfig, WasmSandboxError, WasmSandboxRunner, WASM_ENGINE,
};
use liva_native_core::sandbox::tier2_os::macos_seatbelt::generate_sbpl_profile;
use liva_native_core::sandbox::tier2_os::OsSandboxPolicy;
use liva_native_core::skills::store::SkillPackageStore;
use liva_native_core::skills::watcher::{SkillChangeEvent, SkillWatcher};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use tokio::sync::RwLock;

// ============================================================================
// 1. TIER 1 WASMTIME SANDBOX EMPIRICAL CHALLENGES
// ============================================================================

const MINIMAL_VALID_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00,
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
    0x0a, 0x06, 0x01, 0x04, 0x00, 0x41, 0x2a, 0x0b,
];

const INFINITE_LOOP_WASM: &[u8] = &[
    0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00,
    0x01, 0x05, 0x01, 0x60, 0x00, 0x01, 0x7f,
    0x03, 0x02, 0x01, 0x00,
    0x07, 0x07, 0x01, 0x03, 0x72, 0x75, 0x6e, 0x00, 0x00,
    0x0a, 0x0b, 0x01, 0x09, 0x00, 0x03, 0x40, 0x0c, 0x00, 0x0b, 0x41, 0x00, 0x0b,
];

#[tokio::test]
async fn challenge_wasm_memory_limit_64mb_trap() {
    let runner = WasmSandboxRunner::new().expect("Runner init failed");
    
    // WAT module attempting to grow memory beyond 64MB (1024 pages of 64KB = 64MB)
    let wat = r#"
        (module
            (memory (export "memory") 1)
            (func (export "run") (result i32)
                ;; Try to allocate 1025 pages (64MB + 64KB), which exceeds the 64MB ceiling
                (memory.grow (i32.const 1025))
            )
        )
    "#;
    let module = runner.compile_module(wat.as_bytes()).expect("Compile WAT");

    let mut config = WasmSandboxConfig::default();
    config.memory_limit_bytes = 64 * 1024 * 1024; // 64MB

    let res = runner.execute_core_module(&module, &config, "run").await;
    assert!(res.is_ok(), "Module execution should complete without crashing the host");
    assert_eq!(res.unwrap(), -1, "memory.grow beyond 64MB must return -1 (allocation failure)");
    
    // Attempt extreme growth (10,000 pages = 640MB)
    let wat_extreme = r#"
        (module
            (memory (export "memory") 1)
            (func (export "run") (result i32)
                (memory.grow (i32.const 10000))
            )
        )
    "#;
    let module_extreme = runner.compile_module(wat_extreme.as_bytes()).expect("Compile WAT");
    let res_extreme = runner.execute_core_module(&module_extreme, &config, "run").await;
    assert!(res_extreme.is_ok());
    assert_eq!(res_extreme.unwrap(), -1, "Extreme memory growth must safely return -1");
}

#[tokio::test]
async fn challenge_wasm_fuel_exhaustion_trap_deterministic() {
    let runner = WasmSandboxRunner::new().expect("Runner init");
    let module = runner.compile_module(INFINITE_LOOP_WASM).expect("Compile loop");

    // Tight fuel budget: 100 instructions
    let config = WasmSandboxConfig {
        fuel_limit: 100,
        epoch_deadline_ticks: 10000,
        ..WasmSandboxConfig::default()
    };

    let res = runner.execute_core_module(&module, &config, "run").await;
    assert!(res.is_err(), "Infinite loop must exhaust fuel and trap");
    match res.unwrap_err() {
        WasmSandboxError::Trap(msg) | WasmSandboxError::Execution(msg) => {
            let lower = msg.to_lowercase();
            assert!(lower.contains("fuel") || lower.contains("consumed") || lower.contains("trap") || lower.contains("exhaust"));
        }
        other => panic!("Expected fuel trap error, got: {:?}", other),
    }
}

#[tokio::test]
async fn challenge_wasm_epoch_interruption_infinite_loop() {
    let runner = WasmSandboxRunner::new().expect("Runner init");
    let ticker = EpochTicker::spawn(WASM_ENGINE.clone(), Duration::from_millis(5));

    let config = WasmSandboxConfig {
        epoch_deadline_ticks: 2, // ~10ms deadline
        fuel_limit: 1_000_000_000, // Abundant fuel so epoch interrupts first
        ..WasmSandboxConfig::default()
    };

    let start = Instant::now();
    let res = runner.execute_module(INFINITE_LOOP_WASM, &config, b"").await;
    let elapsed = start.elapsed();
    ticker.abort();

    assert!(elapsed < Duration::from_millis(500), "Interruption must trigger promptly (<500ms)");
    assert!(res.is_err(), "Epoch deadline must abort infinite loop");
    match res.unwrap_err() {
        WasmSandboxError::Trap(msg) | WasmSandboxError::Execution(msg) => {
            let lower = msg.to_lowercase();
            assert!(lower.contains("epoch") || lower.contains("deadline") || lower.contains("interrupted") || lower.contains("trap"));
        }
        other => panic!("Expected epoch trap, got: {:?}", other),
    }

    // Verify engine reusability after trap
    let normal_res = runner.execute_module(MINIMAL_VALID_WASM, &WasmSandboxConfig::default(), b"").await;
    assert!(normal_res.is_ok(), "Engine must remain completely operational after epoch trap");
}

#[tokio::test]
async fn challenge_wasm_concurrent_stress_multi_instance() {
    let runner = Arc::new(WasmSandboxRunner::new().expect("Runner init"));
    let mut handles = Vec::new();

    // Spawn 20 parallel tasks executing varied modules
    for i in 0..20 {
        let r = runner.clone();
        let handle = tokio::spawn(async move {
            if i % 3 == 0 {
                // Valid module
                let res = r.execute_module(MINIMAL_VALID_WASM, &WasmSandboxConfig::default(), b"").await;
                assert!(res.is_ok());
            } else if i % 3 == 1 {
                // Fuel trapped module
                let cfg = WasmSandboxConfig {
                    fuel_limit: 50,
                    epoch_deadline_ticks: 1000,
                    ..WasmSandboxConfig::default()
                };
                let res = r.execute_module(INFINITE_LOOP_WASM, &cfg, b"").await;
                assert!(res.is_err());
            } else {
                // Memory bound module
                let wat = r#"
                    (module
                        (memory (export "memory") 1)
                        (func (export "run") (result i32)
                            (memory.grow (i32.const 1500))
                        )
                    )
                "#;
                let mod_mem = r.compile_module(wat.as_bytes()).unwrap();
                let cfg = WasmSandboxConfig::default();
                let res = r.execute_core_module(&mod_mem, &cfg, "run").await;
                assert_eq!(res.unwrap(), -1);
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.unwrap();
    }
}

// ============================================================================
// 2. TIER 2 OS CONTAINMENT EMPIRICAL CHALLENGES
// ============================================================================

#[test]
fn challenge_os_path_traversal_and_symlinks() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().canonicalize().unwrap();
    let validator = CanonicalPathValidator::new(&root).unwrap();

    // 1. Direct parent directory traversal
    let escape_rel = Path::new("../../etc/passwd");
    assert!(validator.validate_read(escape_rel).is_err());

    // 2. Deep traversal
    let deep_escape = root.join("a/b/c/../../../../etc/shadow");
    assert!(validator.validate_read(&deep_escape).is_err());

    // 3. Null byte injection
    let null_byte_path = Path::new("sub\0dir/file.txt");
    assert!(validator.validate_read(null_byte_path).is_err());

    // 4. Symlink escape attack (symlink inside root pointing outside)
    let outside_target = tempfile::tempdir().unwrap();
    let symlink_path = root.join("escaped_symlink");
    
    #[cfg(unix)]
    {
        use std::os::unix::fs::symlink;
        if symlink(outside_target.path(), &symlink_path).is_ok() {
            let secret_file = symlink_path.join("secret.txt");
            let res = validator.validate_read(&secret_file);
            assert!(res.is_err(), "Symlink pointing outside sandbox root must be rejected");
            match res.unwrap_err() {
                SandboxViolation::PathJailbreak(_) => {}
                other => panic!("Expected PathJailbreak for symlink escape, got: {:?}", other),
            }
        }
    }
}

#[test]
fn challenge_os_destructive_command_blocking() {
    // Destructive rm commands
    assert!(validate_command("rm", &["-rf".to_string(), "/".to_string()]).is_err());
    assert!(validate_command("rm", &["-rf".to_string(), "target".to_string()]).is_err());
    assert!(validate_command("rm", &["-r".to_string(), "/tmp".to_string()]).is_err());
    assert!(validate_command("rm", &["--recursive".to_string(), "data".to_string()]).is_err());

    // System-level destruction commands
    assert!(validate_command("mkfs", &["/dev/sda1".to_string()]).is_err());
    assert!(validate_command("dd", &["if=/dev/zero".to_string(), "of=/dev/nvme0n1".to_string()]).is_err());
    assert!(validate_command("shutdown", &["-h".to_string(), "now".to_string()]).is_err());
    assert!(validate_command("reboot", &[]).is_err());
    assert!(validate_command("init", &["0".to_string()]).is_err());

    // Parameter traversal disguised in safe commands
    assert!(validate_command("cargo", &["--manifest-path".to_string(), "../../etc/passwd".to_string()]).is_err());
    assert!(validate_command("git", &["diff".to_string(), "../../../etc/shadow".to_string()]).is_err());

    // Allowed commands
    assert!(validate_command("cargo", &["check".to_string()]).is_ok());
    assert!(validate_command("git", &["status".to_string()]).is_ok());
    assert!(validate_command("python3", &["--version".to_string()]).is_ok());
}

#[test]
fn challenge_os_ssrf_and_private_ip_filter() {
    let filter = SsrfFilter::new();

    // 1. Cloud Instance Metadata Endpoints
    assert!(filter.validate_url("http://169.254.169.254/latest/meta-data").is_err());
    assert!(filter.validate_url("http://169.254.169.254/computeMetadata/v1").is_err());
    assert!(filter.validate_url("http://169.254.169.250/meta").is_err());
    assert!(filter.validate_url("http://metadata.google.internal/computeMetadata/v1").is_err());
    assert!(filter.validate_url("http://metadata.google/computeMetadata/v1").is_err());

    // 2. Loopback & Localhost
    assert!(filter.validate_url("http://127.0.0.1:8080/admin").is_err());
    assert!(filter.validate_url("http://localhost:3000/metrics").is_err());
    assert!(filter.validate_url("http://0.0.0.0:8000/").is_err());
    assert!(filter.validate_url("http://[::1]:9090/").is_err());

    // 3. RFC 1918 Private Subnets
    assert!(filter.validate_url("http://10.0.0.1/secret").is_err());
    assert!(filter.validate_url("http://172.16.0.1/internal").is_err());
    assert!(filter.validate_url("http://172.31.255.255/db").is_err());
    assert!(filter.validate_url("http://192.168.1.1/admin").is_err());

    // 4. Disallowed Schemes
    assert!(filter.validate_url("file:///etc/passwd").is_err());
    assert!(filter.validate_url("javascript:alert(1)").is_err());
    assert!(filter.validate_url("ftp://ftp.local/").is_err());
    assert!(filter.validate_url("gopher://127.0.0.1:70").is_err());

    // 5. Valid Public Endpoints
    assert!(filter.validate_url("https://api.github.com/repos").is_ok());
    assert!(filter.validate_url("https://crates.io/api/v1/crates").is_ok());
}

#[test]
fn challenge_os_seatbelt_sbpl_generation() {
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![PathBuf::from("/System/Library"), PathBuf::from("/workspace")],
        allowed_write_paths: vec![PathBuf::from("/workspace/target")],
        allowed_commands: vec!["cargo".to_string(), "git".to_string()],
        allow_network: false,
    };

    let profile = generate_sbpl_profile(&policy, Path::new("/usr/bin/cargo"));
    assert!(profile.contains("(deny default)"));
    assert!(profile.contains("(deny network*)"));
    assert!(profile.contains("(allow file-read*"));
    assert!(profile.contains("(allow file-write*"));
    assert!(profile.contains("/workspace/target"));
}

// ============================================================================
// 3. SKILL.MD LIVE HOT-RELOAD & SHA-256 FINGERPRINT CHALLENGES
// ============================================================================

#[tokio::test]
async fn challenge_skill_hot_reload_lifecycle() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![root.clone()], Duration::from_millis(50))
        .with_package_store(store.clone());

    // 1. Dynamic Skill Creation
    let skill_dir = root.join("test-agent-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_v1 = r#"---
name: "test-agent-skill"
version: "1.0.0"
description: "Initial skill version"
runtime_type: "native_rust"
---
# Instructions v1
"#;
    let skill_file = skill_dir.join("SKILL.md");
    fs::write(&skill_file, skill_v1).unwrap();

    let events1 = watcher.scan_once().await.unwrap();
    assert_eq!(events1.len(), 1);
    match &events1[0] {
        SkillChangeEvent::Added(pkg) => {
            assert_eq!(pkg.manifest.name, "test-agent-skill");
            assert_eq!(pkg.manifest.version, "1.0.0");
            assert_eq!(pkg.content_hash.len(), 64);
        }
        other => panic!("Expected Added event, got: {:?}", other),
    }
    assert!(store.read().await.get("test-agent-skill").is_some());

    // 2. Dynamic Skill Modification & SHA-256 update
    let skill_v2 = r#"---
name: "test-agent-skill"
version: "2.0.0"
description: "Updated skill version with modifications"
runtime_type: "native_rust"
---
# Instructions v2 updated
"#;
    fs::write(&skill_file, skill_v2).unwrap();

    let events2 = watcher.scan_once().await.unwrap();
    assert_eq!(events2.len(), 1);
    match &events2[0] {
        SkillChangeEvent::Modified { old_hash, new_package } => {
            assert_eq!(new_package.manifest.version, "2.0.0");
            assert_ne!(old_hash, &new_package.content_hash);
        }
        other => panic!("Expected Modified event, got: {:?}", other),
    }
    assert_eq!(store.read().await.get("test-agent-skill").unwrap().manifest.version, "2.0.0");

    // 3. Dynamic Skill Deletion & Cache Eviction
    fs::remove_file(&skill_file).unwrap();
    fs::remove_dir_all(&skill_dir).unwrap();

    let events3 = watcher.scan_once().await.unwrap();
    assert_eq!(events3.len(), 1);
    match &events3[0] {
        SkillChangeEvent::Removed { skill_name, .. } => {
            assert_eq!(skill_name, "test-agent-skill");
        }
        other => panic!("Expected Removed event, got: {:?}", other),
    }
    assert!(store.read().await.get("test-agent-skill").is_none());
}

#[tokio::test]
async fn challenge_skill_corrupt_manifest_resilience() {
    let temp_dir = TempDir::new().unwrap();
    let root = temp_dir.path().to_path_buf();
    let store = Arc::new(RwLock::new(SkillPackageStore::new()));
    let watcher = SkillWatcher::new(vec![root.clone()], Duration::from_millis(50))
        .with_package_store(store.clone());

    let skill_dir = root.join("stable-skill");
    fs::create_dir_all(&skill_dir).unwrap();
    let skill_file = skill_dir.join("SKILL.md");

    // Valid v1
    fs::write(&skill_file, "---\nname: \"stable-skill\"\nversion: \"1.0.0\"\ndescription: \"stable\"\n---\n").unwrap();
    watcher.scan_once().await.unwrap();
    assert!(store.read().await.get("stable-skill").is_some());

    // Corrupt edit
    fs::write(&skill_file, "MALFORMED GARBAGE NO DELIMITERS").unwrap();
    let events = watcher.scan_once().await.unwrap();
    assert!(events.is_empty(), "Corrupt file should not produce valid event");

    // Memory store must retain original valid package
    assert_eq!(store.read().await.get("stable-skill").unwrap().manifest.version, "1.0.0");
}
