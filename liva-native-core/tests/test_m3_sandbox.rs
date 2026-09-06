//! Milestone 3 Integration Test Suite: Windows Job Objects Sandbox & Cross-Platform Security Matrix
//!
//! Validates:
//! 1. All 7 CapabilityToken variants, FromStr, and monotonic capability attenuation semantics.
//! 2. Path canonicalization, symlink resolution, DOS device name rejection, and jailbreak prevention.
//! 3. SSRF network filter across cloud metadata, private IP ranges, CGNAT, IPv6 special ranges, and URL evasion tactics.
//! 4. AST command sanitizer blocking destructive shell sequences and argument escapes.
//! 5. Windows Job Objects configuration, mock/native runner, and unified cross-platform sandbox matrix.

use liva_native_core::sandbox::{
    execute_windows_sandbox, validate_command, CapabilityToken, CanonicalPathValidator,
    JobAccountingStats, OsPlatformBackend, OsSandboxPolicy, OsSandboxRunner, SandboxPolicy,
    SandboxViolation, SandboxingEngine, SsrfFilter, WindowsJobConfig, WindowsJobSandbox,
};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

// ============================================================================
// 1. CapabilityToken Security Matrix & Monotonic Attenuation
// ============================================================================

#[tokio::test]
async fn test_m3_capability_token_all_variants() {
    let all_caps = [
        CapabilityToken::FsRead,
        CapabilityToken::FsWrite,
        CapabilityToken::NetOutbound,
        CapabilityToken::OsExecute,
        CapabilityToken::VisionCapture,
        CapabilityToken::AudioRecord,
        CapabilityToken::KeystoreAccess,
    ];

    let mut set = HashSet::new();
    for cap in &all_caps {
        set.insert(*cap);
    }
    assert_eq!(set.len(), 7, "All 7 CapabilityTokens must be unique");

    let policy = SandboxPolicy::new(set, PathBuf::from("/workspace"));
    for cap in &all_caps {
        assert!(policy.has_capability(*cap), "Policy must have {:?}", cap);
        assert!(policy.require_capability(*cap).is_ok());
    }

    // Test missing capability behavior
    let empty_policy = SandboxPolicy {
        capabilities: HashSet::new(),
        ..SandboxPolicy::default()
    };
    for cap in &all_caps {
        assert!(!empty_policy.has_capability(*cap));
        let err = empty_policy.require_capability(*cap);
        assert!(err.is_err());
        match err.unwrap_err() {
            SandboxViolation::CapabilityMissing(name) => {
                assert_eq!(name, cap.to_string());
            }
            other => panic!("Expected CapabilityMissing, got: {:?}", other),
        }
    }
}

#[tokio::test]
async fn test_m3_capability_token_display_and_serde() {
    let tokens = vec![
        (CapabilityToken::FsRead, "fs_read"),
        (CapabilityToken::FsWrite, "fs_write"),
        (CapabilityToken::NetOutbound, "net_outbound"),
        (CapabilityToken::OsExecute, "os_execute"),
        (CapabilityToken::VisionCapture, "vision_capture"),
        (CapabilityToken::AudioRecord, "audio_record"),
        (CapabilityToken::KeystoreAccess, "keystore_access"),
    ];

    for (token, expected_str) in tokens {
        assert_eq!(token.to_string(), expected_str);
        assert_eq!(token.as_str(), expected_str);

        // String parsing via FromStr
        let parsed: CapabilityToken = expected_str.parse().unwrap();
        assert_eq!(token, parsed);

        // Serde roundtrip
        let serialized = serde_json::to_string(&token).unwrap();
        let deserialized: CapabilityToken = serde_json::from_str(&serialized).unwrap();
        assert_eq!(token, deserialized);
    }
}

#[tokio::test]
async fn test_m3_capability_attenuation_non_escalation() {
    let mut parent_caps = HashSet::new();
    parent_caps.insert(CapabilityToken::FsRead);
    parent_caps.insert(CapabilityToken::NetOutbound);

    let parent_policy = SandboxPolicy {
        capabilities: parent_caps,
        allowed_read_paths: vec![PathBuf::from("/workspace/src")],
        allowed_write_paths: vec![],
        max_execution_time_secs: 30,
        max_memory_mb: 64,
        initial_fuel: 50_000_000,
        epoch_deadline_ticks: 50,
        allow_child_processes: false,
        ..SandboxPolicy::default()
    };

    // Valid child: strictly subset
    let mut child_caps = HashSet::new();
    child_caps.insert(CapabilityToken::FsRead);
    let requested_child = SandboxPolicy {
        capabilities: child_caps,
        allowed_read_paths: vec![PathBuf::from("/workspace/src/sub")],
        max_execution_time_secs: 60, // should be attenuated down to 30
        max_memory_mb: 32,
        initial_fuel: 20_000_000,
        epoch_deadline_ticks: 25,
        allow_child_processes: true, // should be attenuated down to false
        ..SandboxPolicy::default()
    };

    let attenuated = parent_policy.attenuate(&requested_child).expect("Valid attenuation");
    assert_eq!(attenuated.max_execution_time_secs, 30);
    assert_eq!(attenuated.max_memory_mb, 32);
    assert_eq!(attenuated.initial_fuel, 20_000_000);
    assert_eq!(attenuated.epoch_deadline_ticks, 25);
    assert!(!attenuated.allow_child_processes);
    assert!(attenuated.has_capability(CapabilityToken::FsRead));
    assert!(!attenuated.has_capability(CapabilityToken::NetOutbound));

    // Escalating child: asks for FsWrite not held by parent
    let mut bad_caps = HashSet::new();
    bad_caps.insert(CapabilityToken::FsRead);
    bad_caps.insert(CapabilityToken::FsWrite);
    let bad_request = SandboxPolicy {
        capabilities: bad_caps,
        ..SandboxPolicy::default()
    };
    let err = parent_policy.attenuate(&bad_request);
    assert!(err.is_err(), "Child cannot escalate permissions");
}

#[tokio::test]
async fn test_m3_strict_readonly_policy_contract() {
    let temp_root = std::env::temp_dir().join("liva_test_readonly_root");
    let _ = std::fs::create_dir_all(&temp_root);

    let policy = SandboxPolicy::strict_readonly(&temp_root);
    assert!(policy.has_capability(CapabilityToken::FsRead));
    assert!(!policy.has_capability(CapabilityToken::FsWrite));
    assert!(!policy.has_capability(CapabilityToken::OsExecute));
    assert!(!policy.has_capability(CapabilityToken::NetOutbound));
    assert_eq!(policy.allowed_write_paths.len(), 0);
    assert_eq!(policy.blocked_domains, vec!["*".to_string()]);
    assert_eq!(policy.max_memory_mb, 64);
    assert_eq!(policy.max_execution_time_secs, 5);
}

// ============================================================================
// 2. Path Canonicalization & Jailbreak Defense
// ============================================================================

#[tokio::test]
async fn test_m3_path_canonicalization_nested_valid() {
    let temp_root = std::env::temp_dir().join("liva_test_m3_path_valid");
    let nested = temp_root.join("sub").join("inner");
    let _ = std::fs::create_dir_all(&nested);

    let validator = CanonicalPathValidator::new(&temp_root).unwrap();

    let res_rel = validator.validate_read(Path::new("sub/inner/file.rs"));
    assert!(res_rel.is_ok());

    let res_abs = validator.validate_read(&nested.join("file.rs"));
    assert!(res_abs.is_ok());

    let res_write = validator.validate_write(Path::new("sub/inner/new_file.txt"));
    assert!(res_write.is_ok());
}

#[tokio::test]
async fn test_m3_path_traversal_dotdot_blocking() {
    let temp_root = std::env::temp_dir().join("liva_test_m3_dotdot");
    let _ = std::fs::create_dir_all(&temp_root);

    let validator = CanonicalPathValidator::new(&temp_root).unwrap();

    let attacks = [
        "../secret.key",
        "../../etc/passwd",
        "sub/../../outside.txt",
        "sub/./../../outside.txt",
        "..\\windows\\system32",
        "%2e%2e/escaped.txt",
    ];

    for attack in &attacks {
        let res = validator.validate_read(Path::new(attack));
        assert!(res.is_err(), "Attack {:?} must be blocked", attack);
        match res.unwrap_err() {
            SandboxViolation::PathJailbreak(_) => {}
            other => panic!("Expected PathJailbreak for {:?}, got {:?}", attack, other),
        }
    }
}

#[tokio::test]
async fn test_m3_path_null_byte_rejection() {
    let temp_root = std::env::temp_dir().join("liva_test_m3_nullbyte");
    let _ = std::fs::create_dir_all(&temp_root);

    let validator = CanonicalPathValidator::new(&temp_root).unwrap();

    let attacks = [
        "file.txt\0.png",
        "folder\0/sub/file.txt",
        "\0/etc/shadow",
    ];

    for attack in &attacks {
        let res = validator.validate_read(Path::new(attack));
        assert!(res.is_err(), "Null byte injection {:?} must fail", attack);
    }
}

#[tokio::test]
async fn test_m3_path_symlink_jailbreak_rejection() {
    let temp_root = std::env::temp_dir().join("liva_test_m3_symlink_escape");
    let outside_dir = std::env::temp_dir().join("liva_test_m3_outside");
    let _ = std::fs::create_dir_all(&temp_root);
    let _ = std::fs::create_dir_all(&outside_dir);

    let target_file = outside_dir.join("secret_data.txt");
    let _ = std::fs::write(&target_file, "classified");

    #[cfg(unix)]
    {
        let symlink_path = temp_root.join("symlink_outside");
        let _ = std::fs::remove_file(&symlink_path);
        if std::os::unix::fs::symlink(&outside_dir, &symlink_path).is_ok() {
            let validator = CanonicalPathValidator::new(&temp_root).unwrap();
            let res = validator.validate_read(&symlink_path.join("secret_data.txt"));
            assert!(res.is_err(), "Symlink escape to outside directory must be rejected");
        }
    }
}

#[tokio::test]
async fn test_m3_path_windows_and_device_edge_cases() {
    let temp_root = std::env::temp_dir().join("liva_test_m3_devices");
    let _ = std::fs::create_dir_all(&temp_root);

    let validator = CanonicalPathValidator::new(&temp_root).unwrap();

    // Absolute system path escape
    #[cfg(target_os = "windows")]
    let sys_escape = Path::new("C:\\Windows\\System32\\cmd.exe");
    #[cfg(not(target_os = "windows"))]
    let sys_escape = Path::new("/etc/passwd");

    assert!(validator.validate_read(sys_escape).is_err());

    // DOS device names check
    assert!(validator.validate_read(Path::new("CON")).is_err());
    assert!(validator.validate_read(Path::new("NUL")).is_err());
    assert!(validator.validate_read(Path::new("COM1")).is_err());
    assert!(validator.validate_read(Path::new("LPT1")).is_err());
}

// ============================================================================
// 3. SSRF Network Filter Defense Matrix
// ============================================================================

#[tokio::test]
async fn test_m3_ssrf_cloud_metadata_endpoints() {
    let filter = SsrfFilter::new();

    let cloud_endpoints = [
        "http://169.254.169.254/latest/meta-data/",
        "http://169.254.169.254/computeMetadata/v1",
        "http://169.254.169.250/metadata",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://metadata.google/computeMetadata",
    ];

    for ep in &cloud_endpoints {
        assert!(filter.validate_url(ep).is_err(), "Cloud endpoint {} must be blocked", ep);
    }
}

#[tokio::test]
async fn test_m3_ssrf_rfc1918_and_cgnat_subnets() {
    let filter = SsrfFilter::new();

    let private_urls = [
        "http://127.0.0.1:8080/api",
        "http://127.0.1.1/",
        "http://localhost:3000",
        "http://0.0.0.0:8000",
        "http://10.0.0.1/admin",
        "http://10.255.255.254/status",
        "http://172.16.0.1/db",
        "http://172.31.255.254/cluster",
        "http://192.168.0.1/router",
        "http://192.168.1.254/gateway",
    ];

    for url in &private_urls {
        assert!(filter.validate_url(url).is_err(), "Private IP {} must be blocked", url);
    }

    // Direct IP check
    assert!(!filter.is_ip_allowed("127.0.0.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("10.0.0.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("172.20.0.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("192.168.1.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("169.254.1.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("100.64.0.1".parse().unwrap())); // CGNAT
    assert!(!filter.is_ip_allowed("100.127.255.254".parse().unwrap())); // CGNAT
    assert!(filter.is_ip_allowed("8.8.8.8".parse().unwrap()));
    assert!(filter.is_ip_allowed("1.1.1.1".parse().unwrap()));
}

#[tokio::test]
async fn test_m3_ssrf_ipv6_special_and_mapped_ipv4() {
    let filter = SsrfFilter::new();

    // Loopback IPv6
    assert!(!filter.is_ip_allowed("::1".parse().unwrap()));
    assert!(filter.validate_url("http://[::1]:8080/").is_err());

    // Link-local IPv6 (fe80::/10)
    assert!(!filter.is_ip_allowed("fe80::1".parse().unwrap()));

    // Unique-local IPv6 (fc00::/7)
    assert!(!filter.is_ip_allowed("fc00::1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("fd00::1".parse().unwrap()));

    // IPv4-mapped IPv6
    assert!(!filter.is_ip_allowed("::ffff:127.0.0.1".parse().unwrap()));
    assert!(!filter.is_ip_allowed("::ffff:169.254.169.254".parse().unwrap()));

    // Valid public IPv6
    assert!(filter.is_ip_allowed("2606:4700:4700::1111".parse().unwrap()));
}

#[tokio::test]
async fn test_m3_ssrf_url_userinfo_and_bracket_parsing() {
    let filter = SsrfFilter::new();

    assert!(filter.validate_url("http://evil.com@127.0.0.1/").is_err());
    assert!(filter.validate_url("http://127.0.0.1@google.com/").is_err());
    assert!(filter.validate_url("http://admin:pass@169.254.169.254/").is_err());
}

#[tokio::test]
async fn test_m3_ssrf_forbidden_schemes_and_domain_rules() {
    let filter = SsrfFilter::new();

    let forbidden_schemes = [
        "file:///etc/passwd",
        "javascript:alert(1)",
        "data:text/html,payload",
        "gopher://127.0.0.1:70",
        "ftp://ftp.example.com/",
        "ldap://localhost:389/",
        "dict://localhost:2628/",
    ];

    for scheme_url in &forbidden_schemes {
        assert!(filter.validate_url(scheme_url).is_err(), "Scheme in {} must be forbidden", scheme_url);
    }

    // Policy with allowed and blocked domains
    let mut policy = SandboxPolicy::default();
    policy.allowed_domains = vec!["api.github.com".to_string(), "*.crates.io".to_string()];
    policy.blocked_domains = vec!["evil.crates.io".to_string()];

    assert!(SsrfFilter::validate_url_with_policy("https://api.github.com/repos", &policy).is_ok());
    assert!(SsrfFilter::validate_url_with_policy("https://static.crates.io/summary", &policy).is_ok());
    assert!(SsrfFilter::validate_url_with_policy("https://evil.crates.io/malware", &policy).is_err());
    assert!(SsrfFilter::validate_url_with_policy("https://unapproved.org/data", &policy).is_err());
}

// ============================================================================
// 4. AST Command Sanitizer Defense
// ============================================================================

#[tokio::test]
async fn test_m3_command_sanitizer_destructive_blocks() {
    assert!(validate_command("cargo", &["test".to_string()]).is_ok());
    assert!(validate_command("git", &["status".to_string()]).is_ok());
    assert!(validate_command("python3", &["-c".to_string(), "print(1)".to_string()]).is_ok());

    assert!(validate_command("rm", &["-rf".to_string(), "/".to_string()]).is_err());
    assert!(validate_command("rm", &["-r".to_string(), "-f".to_string(), "src".to_string()]).is_err());
    assert!(validate_command("mkfs", &["/dev/sda".to_string()]).is_err());
    assert!(validate_command("dd", &["if=/dev/zero".to_string(), "of=/dev/sda".to_string()]).is_err());
    assert!(validate_command("shutdown", &["-h".to_string(), "now".to_string()]).is_err());
    assert!(validate_command("reboot", &[]).is_err());
}

#[tokio::test]
async fn test_m3_command_sanitizer_argument_traversal() {
    assert!(validate_command("cargo", &["--manifest-path".to_string(), "../Cargo.toml".to_string()]).is_err());
    assert!(validate_command("cat", &["../../etc/shadow".to_string()]).is_err());
    assert!(validate_command("ls", &["..".to_string()]).is_err());
}

// ============================================================================
// 5. Windows Job Objects Sandbox & Cross-Platform Matrix Integration
// ============================================================================

#[tokio::test]
async fn test_m3_sandboxing_engine_lifecycle() {
    let engine = SandboxingEngine::new();
    assert!(engine.is_ok(), "Sandboxing engine must initialize successfully");
    let engine = engine.unwrap();
    assert!(engine.wasm().is_ready());
}

#[tokio::test]
async fn test_m3_policy_enforcement_on_os_execution() {
    let runner = OsSandboxRunner::new();
    let policy = OsSandboxPolicy {
        allowed_read_paths: vec![PathBuf::from(".")],
        allowed_write_paths: vec![],
        allowed_commands: vec!["echo".to_string()],
        allow_network: false,
    };

    let res = runner.execute_command("echo", &["m3_verified".to_string()], &policy).await;
    assert!(res.is_ok());
    let output = res.unwrap();
    assert!(output.success);
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout_str().contains("m3_verified"));
}

#[test]
fn test_m3_windows_job_config_builder() {
    let config = WindowsJobConfig {
        job_name: "LIVA_M3_Test_Job".to_string(),
        max_cpu_rate_pct: 40,
        max_memory_limit_bytes: 256 * 1024 * 1024,
        max_io_bytes_per_sec: 25 * 1024 * 1024,
        block_child_process_creation: true,
        terminate_on_job_close: true,
    };

    assert_eq!(config.job_name, "LIVA_M3_Test_Job");
    assert_eq!(config.max_cpu_rate_pct, 40);
    assert_eq!(config.max_memory_limit_bytes, 268435456);
    assert!(config.block_child_process_creation);
}

#[tokio::test]
async fn test_m3_windows_job_sandbox_execution() {
    let policy = OsSandboxPolicy::default();
    let sandbox = WindowsJobSandbox::new(policy);
    let res = sandbox.execute("echo", &["m3_windows_job_ok".to_string()]).await;

    assert!(res.is_ok(), "Windows job sandbox execution must succeed");
    let output = res.unwrap();
    assert!(output.success);
    assert_eq!(output.exit_code, 0);
    assert!(output.stdout_str().contains("m3_windows_job_ok"));
}

#[test]
fn test_m3_os_platform_backend_current_host() {
    let backend = OsPlatformBackend::current_host();
    let runner = OsSandboxRunner::new();
    assert_eq!(runner.backend(), backend);
    assert!(!runner.backend_name().is_empty());
}

#[tokio::test]
async fn test_m3_direct_execute_windows_sandbox() {
    let policy = OsSandboxPolicy::default();
    let res = execute_windows_sandbox("echo", &["direct_win_job_ok".to_string()], &policy).await;
    assert!(res.is_ok());
    let output = res.unwrap();
    assert!(output.success);
    assert!(output.stdout_str().contains("direct_win_job_ok"));
}

#[test]
fn test_m3_job_accounting_stats_structure() {
    let stats = JobAccountingStats {
        total_user_time_ms: 150,
        total_kernel_time_ms: 50,
        active_processes: 1,
        total_processes: 1,
        peak_process_memory_used: 10 * 1024 * 1024,
        peak_job_memory_used: 12 * 1024 * 1024,
    };
    let json = serde_json::to_string(&stats).unwrap();
    let deserialized: JobAccountingStats = serde_json::from_str(&json).unwrap();
    assert_eq!(stats, deserialized);
}
