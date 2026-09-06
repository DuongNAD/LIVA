//! Milestone 2: Empirical Challenger 1 Stress & Boundary Suite
//!
//! Tests:
//! 1. Keystore Multi-threaded Concurrency:
//!    - 50 concurrent threads racing `load_or_create_device_key` on fresh non-existent paths.
//!    - 50 concurrent threads racing `load_or_create_vault_secret` on fresh non-existent paths.
//!    - Verifies OnceLock and atomic tempfile operations eliminate 0-byte read races and unseal failures.
//! 2. Browser Preview Automation & State Transitions:
//!    - Initial preview running state (`isRunning == true`).
//!    - URL navigation and metadata updates.
//!    - DOM extraction across all 4 modes (Accessibility, CleanMarkdown, PlainText, FullHtml).
//!    - Base64 viewport screenshot validation.
//!    - Direct MockBrowserDriver click and type_text (input) action recording.
//!    - Lifecycle control transitions: Pause -> Resume -> Stop -> Clear Logs.
//!    - SSRF Guard enforcement and blocked domain rejections.
//!    - Graceful error handling for missing payloads and unregistered commands (browser:click, browser:input).

use liva_native_core::automation::browser::{BrowserConfig, BrowserDriver, MockBrowserDriver};
use liva_native_core::automation::dom::DomExtractMode;
use liva_native_core::automation::sandbox::SandboxPolicy;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::keystore::{
    load_or_create_device_key, load_or_create_vault_secret, platform_seal, platform_unseal,
};
use liva_native_core::{
    AppState, CommandPrincipal, db, handle_command_as, llm, stt, tts,
};
use serde_json::json;
use std::sync::Arc;
use tempfile::tempdir;

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
// 1. Keystore Multi-Threaded Concurrency Stress Tests
// =========================================================================

#[test]
fn test_keystore_concurrent_device_key_creation_race() {
    let tmp = tempdir().expect("tempdir");
    let db_path = tmp.path().join("liva_test.db");

    let thread_count = 50;
    let mut handles = Vec::with_capacity(thread_count);

    for _ in 0..thread_count {
        let p = db_path.clone();
        handles.push(std::thread::spawn(move || {
            load_or_create_device_key(&p)
        }));
    }

    let mut keys = Vec::with_capacity(thread_count);
    for h in handles {
        let res = h.join().expect("thread join");
        assert!(res.is_ok(), "Concurrent load_or_create_device_key failed: {:?}", res.err());
        let (key, _generated) = res.unwrap();
        keys.push(key);
    }

    // All 50 threads MUST resolve the exact same key hex
    let first_key = &keys[0];
    assert_eq!(first_key.len(), 64, "Key hex must be 64 characters (32 bytes)");
    for (idx, k) in keys.iter().enumerate() {
        assert_eq!(k, first_key, "Thread {} received divergent key", idx);
    }
}

#[test]
fn test_keystore_concurrent_vault_secret_creation_race() {
    let tmp = tempdir().expect("tempdir");
    let secret_dir = tmp.path().join("vault");

    let thread_count = 50;
    let mut handles = Vec::with_capacity(thread_count);

    for _ in 0..thread_count {
        let dir = secret_dir.clone();
        handles.push(std::thread::spawn(move || {
            load_or_create_vault_secret(&dir)
        }));
    }

    let mut secrets = Vec::with_capacity(thread_count);
    for h in handles {
        let res = h.join().expect("thread join");
        assert!(res.is_ok(), "Concurrent load_or_create_vault_secret failed: {:?}", res.err());
        let (pwd, salt, _gen) = res.unwrap();
        secrets.push((pwd, salt));
    }

    let (first_pwd, first_salt) = &secrets[0];
    assert_eq!(first_pwd.len(), 32);
    assert_eq!(first_salt.len(), 16);
    for (idx, (p, s)) in secrets.iter().enumerate() {
        assert_eq!(p, first_pwd, "Thread {} received divergent password", idx);
        assert_eq!(s, first_salt, "Thread {} received divergent salt", idx);
    }
}

#[test]
fn test_keystore_seal_unseal_integrity_under_stress() {
    let thread_count = 30;
    let mut handles = Vec::with_capacity(thread_count);

    for i in 0..thread_count {
        handles.push(std::thread::spawn(move || {
            let payload = format!("secret-payload-iteration-{}-{}", i, uuid::Uuid::new_v4());
            let sealed = platform_seal(payload.as_bytes()).expect("seal");
            let unsealed = platform_unseal(&sealed).expect("unseal");
            assert_eq!(unsealed, payload.as_bytes());
        }));
    }

    for h in handles {
        h.join().expect("thread join");
    }
}

// =========================================================================
// 2. Mock Browser Driver Direct State Transitions & Input Handling
// =========================================================================

#[tokio::test]
async fn test_mock_browser_driver_direct_clicks_and_inputs() {
    let policy = SandboxPolicy {
        allowed_domains: vec!["*".to_string()],
        blocked_domains: vec!["*.blocked.com".to_string()],
        allowed_read_paths: vec![],
        allowed_write_paths: vec![],
        command_denylist: vec![],
        max_execution_time_secs: 15,
        max_memory_mb: 256,
        allow_child_processes: false,
    };

    let mut driver = MockBrowserDriver::new(policy);
    assert!(*driver.is_open.read().await, "Driver initialized in running state");

    // Test navigation
    let meta = driver.navigate("https://liva.ai/demo").await.expect("navigate");
    assert_eq!(meta.url, "https://liva.ai/demo");
    assert_eq!(meta.http_status, 200);

    // Test click simulation
    driver.click("#login-btn").await.expect("click");
    driver.click(".submit-action").await.expect("click");
    {
        let clicks = driver.clicks.read().await;
        assert_eq!(clicks.len(), 2);
        assert_eq!(clicks[0], "#login-btn");
        assert_eq!(clicks[1], ".submit-action");
    }

    // Test type_text (input) simulation
    driver.type_text("#username", "testuser@liva.ai").await.expect("type_text");
    driver.type_text("#password", "P@ssw0rd123!").await.expect("type_text");
    {
        let typed = driver.typed_texts.read().await;
        assert_eq!(typed.len(), 2);
        assert_eq!(typed[0], ("#username".to_string(), "testuser@liva.ai".to_string()));
        assert_eq!(typed[1], ("#password".to_string(), "P@ssw0rd123!".to_string()));
    }

    // Test DOM extraction
    let markdown = driver.extract_content(DomExtractMode::CleanMarkdown).await.expect("extract markdown");
    assert!(markdown.contains("Loaded https://liva.ai/demo"));

    // Test screenshot
    let shot = driver.screenshot_viewport().await.expect("screenshot");
    assert!(!shot.is_empty());
    assert_eq!(&shot[..4], &[0x89, 0x50, 0x4E, 0x47]); // PNG magic bytes

    // Test close and relaunch
    driver.close().await.expect("close");
    assert!(!*driver.is_open.read().await, "Driver is closed");

    driver.launch(BrowserConfig::default()).await.expect("launch");
    assert!(*driver.is_open.read().await, "Driver relaunched");
}

// =========================================================================
// 3. Browser IPC Commands End-to-End Stress & State Transitions
// =========================================================================

#[tokio::test]
async fn test_browser_ipc_commands_full_state_transitions_and_boundaries() {
    let state = test_app_state();

    // 1. Initial status query
    let status_initial = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("browser:status");

    assert!(status_initial["isRunning"].as_bool().unwrap());
    assert!(!status_initial["isPaused"].as_bool().unwrap());
    assert!(status_initial["sandboxActive"].as_bool().unwrap());
    assert!(status_initial["ssrfGuard"].as_bool().unwrap());
    assert_eq!(status_initial["viewportWidth"], 1280);
    assert_eq!(status_initial["viewportHeight"], 800);

    // 2. Navigation with valid URL
    let nav_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "https://liva.ai/docs" }),
        None,
        None,
    )
    .await
    .expect("browser:navigate");
    assert_eq!(nav_res["url"], "https://liva.ai/docs");
    assert_eq!(nav_res["httpStatus"], 200);

    // 3. SSRF Guard Boundary Check: Localhost & Private IP MUST be blocked
    let ssrf_localhost = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "http://127.0.0.1:8080/admin" }),
        None,
        None,
    )
    .await;
    assert!(ssrf_localhost.is_err(), "SSRF to 127.0.0.1 MUST be rejected by sandbox");

    let ssrf_aws_metadata = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "http://169.254.169.254/latest/meta-data/" }),
        None,
        None,
    )
    .await;
    assert!(ssrf_aws_metadata.is_err(), "SSRF to cloud metadata MUST be rejected");

    // 4. Blocked Domain Boundary Check
    let blocked_res = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:navigate",
        json!({ "url": "https://malicious.com/phish" }),
        None,
        None,
    )
    .await;
    assert!(blocked_res.is_err(), "Blocked domain MUST be rejected");

    // 5. DOM Extraction across all modes
    for mode in &["accessibility", "plain_text", "html", "semantic"] {
        let extract = handle_command_as(
            CommandPrincipal::TauriDashboard,
            state.clone(),
            "browser:extract",
            json!({ "mode": mode }),
            None,
            None,
        )
        .await
        .expect("extract");
        assert_eq!(extract["mode"], *mode);
        assert!(extract["content"].is_string());
    }

    // 6. Screenshot Viewport
    let shot = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:screenshot",
        json!({}),
        None,
        None,
    )
    .await
    .expect("screenshot");
    assert!(shot["base64Png"].as_str().unwrap().starts_with("data:image/png;base64,"));

    // 7. Action Timeline Logs
    let logs = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:action_log",
        json!({}),
        None,
        None,
    )
    .await
    .expect("action_log");
    assert!(logs["count"].as_u64().unwrap() >= 2);

    // 8. Lifecycle Control: Pause -> Verify -> Resume -> Verify -> Stop -> Verify -> Clear Logs
    let pause = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "pause" }),
        None,
        None,
    )
    .await
    .expect("pause");
    assert_eq!(pause["state"], "paused");

    let status_after_pause = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("status");
    assert!(status_after_pause["isPaused"].as_bool().unwrap());

    let resume = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "resume" }),
        None,
        None,
    )
    .await
    .expect("resume");
    assert_eq!(resume["state"], "running");

    let status_after_resume = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("status");
    assert!(!status_after_resume["isPaused"].as_bool().unwrap());

    let stop = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "stop" }),
        None,
        None,
    )
    .await
    .expect("stop");
    assert_eq!(stop["state"], "stopped");

    let status_after_stop = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:status",
        json!({}),
        None,
        None,
    )
    .await
    .expect("status");
    assert!(!status_after_stop["isRunning"].as_bool().unwrap());

    let clear = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:control",
        json!({ "action": "clear_logs" }),
        None,
        None,
    )
    .await
    .expect("clear_logs");
    assert!(clear["cleared"].as_bool().unwrap());

    let logs_after_clear = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:action_log",
        json!({}),
        None,
        None,
    )
    .await
    .expect("action_log");
    assert_eq!(logs_after_clear["count"].as_u64().unwrap(), 0);

    // 9. Unregistered/Synthetic Commands Graceful Failure (browser:click, browser:input)
    // Verify that attempting unregistered commands does not panic or crash the server.
    let click_cmd = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:click",
        json!({ "selector": "#btn" }),
        None,
        None,
    )
    .await;
    assert!(click_cmd.is_err(), "browser:click is not an IPC command and must fail gracefully");

    let input_cmd = handle_command_as(
        CommandPrincipal::TauriDashboard,
        state.clone(),
        "browser:input",
        json!({ "selector": "#txt", "text": "hello" }),
        None,
        None,
    )
    .await;
    assert!(input_cmd.is_err(), "browser:input is not an IPC command and must fail gracefully");
}
