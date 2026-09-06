//! Comprehensive Integration and Verification Tests for Milestone 4 (M4).
//!
//! Covers:
//! 1. Subsystem Active Diagnostic Probes:
//!    - LLM runtime probe (model paths, context window budget, tokenization sanity).
//!    - Audio I/O loopback probe (microphone, speaker playback buffer, auxiliary DSP modules).
//!    - SQLite pool probe (writer transaction latency, 4-way reader concurrency, WAL checkpoint, vec0).
//!    - Network adapters probe (loopback socket bind, DNS resolution, internet reachability).
//!    - Headless browser binary probe (CDP port, executable discovery, sandbox SSRF security).
//! 2. Telemetry and Latency Profiler:
//!    - TTFT, receive-to-stream, and WebSocket transit latency percentile calculations (p50/p95).
//!    - Real-time CPU and memory telemetry profiler integration in `system:telemetry` and `get_system_status`.
//! 3. Authorization and Fail-Closed Security Matrix for Diagnostic Commands.
//! 4. High-Concurrency Stress Testing on Diagnostics and Telemetry Ring Buffers.

use liva_native_core::{
    AppState, CommandPrincipal, DatabasePool, EncryptionEngine, LlamaRouterManager, SttManager,
    SubsystemReport, SubsystemStatus, SystemDiagnosticReport, TtsAudioPlayer,
    authorize_command, global_telemetry, handle_command,
    probe_audio_io, probe_browser_binary, probe_llm_runtime, probe_network_adapters,
    probe_sqlite_pool, run_system_diagnostic, system_status,
    telemetry::TelemetryProfiler,
};
use std::sync::Arc;
use tokio::sync::Mutex;

/// Build an isolated in-memory test AppState instance.
async fn make_test_app_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("In-memory DatabasePool creation must succeed");
    let crypto = EncryptionEngine::new(liva_native_core::crypto::DEFAULT_ENCRYPTION_KEY);

    let stt = Mutex::new(SttManager::new("non_existent_test_dir"));
    let tts = Mutex::new(None);
    let tts_player = TtsAudioPlayer::new(None);
    let llm = Mutex::new(LlamaRouterManager::new(2048, 0).expect("LlamaRouterManager creation must succeed"));
    let vad = Mutex::new(None);
    let denoiser = Mutex::new(None);
    let turn_shadow = Mutex::new(None);
    let aec = Mutex::new(None);
    let mcp_server = Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("data/vault"));

    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        1920,
        1080,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));
    let vision_manager = liva_native_core::vision::VisionManager::new(
        mock_capturer,
        liva_native_core::vision::VisionConfig::default(),
    );

    let vision = Mutex::new(vision_manager);
    let embedder = Mutex::new(None);

    Arc::new(AppState {
        db,
        crypto,
        stt,
        tts,
        tts_player,
        llm,
        vad,
        denoiser,
        turn_shadow,
        aec,
        mcp_server,
        vision,
        embedder,
    })
}

// ── Test 1: Full Diagnostic Suite Sweep ──────────────────────────────────────

#[tokio::test]
async fn test_m4_system_diagnostic_probe_full_sweep() {
    let state = make_test_app_state().await;
    let report: SystemDiagnosticReport = run_system_diagnostic(state)
        .await
        .expect("Full system diagnostic run must succeed");

    // Verify report top-level fields
    assert!(!report.timestamp.is_empty(), "Timestamp must be populated");
    assert!(
        matches!(
            report.overall_status,
            SubsystemStatus::Healthy | SubsystemStatus::Degraded | SubsystemStatus::Unavailable
        ),
        "Overall status must be a valid SubsystemStatus"
    );

    // Verify LLM Subsystem Report
    assert_eq!(report.llm.name, "llm_runtime");
    assert!(report.llm.latency_ms >= 0.0);
    assert!(!report.llm.detail.is_empty());
    assert!(report.llm.checks.get("n_ctx").is_some());
    assert!(report.llm.checks.get("context_budget_ok").is_some());

    // Verify Audio I/O Subsystem Report
    assert_eq!(report.audio.name, "audio_io");
    assert!(report.audio.latency_ms >= 0.0);
    assert!(!report.audio.detail.is_empty());
    assert!(report.audio.checks.get("tts_player_buffer_empty").is_some());
    assert!(report.audio.checks.get("microphone_detected").is_some());
    assert!(report.audio.checks.get("speaker_detected").is_some());

    // Verify SQLite Pool Subsystem Report
    assert_eq!(report.database.name, "sqlite_pool");
    assert!(report.database.latency_ms >= 0.0);
    assert!(!report.database.detail.is_empty());
    assert_eq!(
        report.database.checks["writer_transaction_passed"],
        true,
        "In-memory writer transaction must succeed"
    );
    assert_eq!(
        report.database.checks["reader_concurrency_acquired"],
        4,
        "All 4 concurrent readers must be acquired"
    );

    // Verify Network Adapters Subsystem Report
    assert_eq!(report.network.name, "network_adapters");
    assert!(report.network.latency_ms >= 0.0);
    assert_eq!(
        report.network.checks["loopback_bound"],
        true,
        "Local loopback interface must bind"
    );

    // Verify Browser Binary Subsystem Report
    assert_eq!(report.browser.name, "headless_browser");
    assert!(report.browser.latency_ms >= 0.0);
    assert_eq!(
        report.browser.checks["sandbox_security_verified"],
        true,
        "Sandbox SSRF security rules must be validated"
    );
    assert_eq!(
        report.browser.checks["sandbox_ssrf_metadata_blocked"],
        true,
        "AWS/Cloud metadata IP must be blocked"
    );
    assert_eq!(
        report.browser.checks["sandbox_loopback_blocked"],
        true,
        "Private loopback navigation must be blocked"
    );
    assert_eq!(
        report.browser.checks["sandbox_file_protocol_blocked"],
        true,
        "file:// URI scheme must be blocked"
    );

    // Verify Telemetry Summary attached
    assert!(report.telemetry_summary.is_some());
}

// ── Test 2: Database Pool Concurrency & WAL Checkpoint Probe ─────────────────

#[tokio::test]
async fn test_m4_database_active_probe_concurrency() {
    let state = make_test_app_state().await;
    let db_report: SubsystemReport = probe_sqlite_pool(&state).await;

    assert_eq!(db_report.name, "sqlite_pool");
    assert_eq!(
        db_report.checks["writer_transaction_passed"], true,
        "Writer connection transaction check must pass"
    );
    assert_eq!(
        db_report.checks["reader_concurrency_acquired"], 4,
        "4 concurrent readers must succeed simultaneously"
    );
    assert!(
        db_report.checks["reader_concurrency_latency_ms"].as_f64().unwrap() >= 0.0,
        "Reader concurrency latency must be measured"
    );
    assert!(
        db_report.checks["writer_transaction_latency_ms"].as_f64().unwrap() >= 0.0,
        "Writer transaction latency must be measured"
    );
}

// ── Test 3: LLM Runtime Active Probe ────────────────────────────────────────

#[tokio::test]
async fn test_m4_llm_runtime_active_probe() {
    let state = make_test_app_state().await;
    let llm_report: SubsystemReport = probe_llm_runtime(&state).await;

    assert_eq!(llm_report.name, "llm_runtime");
    assert_eq!(
        llm_report.checks["n_ctx"], 2048,
        "Context window budget must match initial n_ctx"
    );
    assert_eq!(
        llm_report.checks["context_budget_ok"], true,
        "n_ctx (2048) must exceed RESERVE_FOR_COMPLETION (512)"
    );
    assert!(
        llm_report.checks["tokenization_sanity_passed"].is_boolean(),
        "Tokenization sanity check boolean must be present"
    );
}

// ── Test 4: Audio I/O Active Loopback Probe ──────────────────────────────────

#[tokio::test]
async fn test_m4_audio_io_active_probe() {
    let state = make_test_app_state().await;
    let audio_report: SubsystemReport = probe_audio_io(&state).await;

    assert_eq!(audio_report.name, "audio_io");
    assert_eq!(
        audio_report.checks["tts_player_buffer_empty"], true,
        "Initial audio player buffer must be empty"
    );
    assert!(
        audio_report.checks["microphone_detected"].is_boolean(),
        "Microphone detection field must be a boolean"
    );
    assert!(
        audio_report.checks["speaker_detected"].is_boolean(),
        "Speaker detection field must be a boolean"
    );
}

// ── Test 5: Network Adapters & DNS Active Probe ──────────────────────────────

#[tokio::test]
async fn test_m4_network_adapters_active_probe() {
    let net_report: SubsystemReport = probe_network_adapters().await;

    assert_eq!(net_report.name, "network_adapters");
    assert_eq!(
        net_report.checks["loopback_bound"], true,
        "Loopback TCP socket must bind on 127.0.0.1"
    );
    assert!(
        net_report.checks["loopback_ephemeral_port"].as_u64().unwrap() > 0,
        "Ephemeral port must be > 0"
    );
    assert!(
        net_report.checks["loopback_bind_latency_ms"].as_f64().unwrap() >= 0.0,
        "Loopback bind latency must be recorded"
    );
}

// ── Test 6: Headless Browser Binary & Sandbox Security Probe ─────────────────

#[tokio::test]
async fn test_m4_browser_binary_active_probe() {
    let browser_report: SubsystemReport = probe_browser_binary().await;

    assert_eq!(browser_report.name, "headless_browser");
    assert_eq!(
        browser_report.checks["sandbox_security_verified"], true,
        "Sandbox security guard verification must pass"
    );
    assert_eq!(
        browser_report.checks["sandbox_ssrf_metadata_blocked"], true,
        "SSRF metadata endpoint 169.254.169.254 must be blocked"
    );
    assert_eq!(
        browser_report.checks["sandbox_loopback_blocked"], true,
        "Private loopback endpoint must be blocked"
    );
    assert_eq!(
        browser_report.checks["sandbox_file_protocol_blocked"], true,
        "file:// URI scheme must be blocked"
    );
    assert_eq!(
        browser_report.checks["sandbox_public_https_allowed"], true,
        "Public HTTPS domain must be allowed"
    );
}

// ── Test 7: Telemetry Ring Buffer & Percentile Profiler ──────────────────────

#[tokio::test]
async fn test_m4_telemetry_ring_buffer_percentiles() {
    let profiler = TelemetryProfiler::new();

    // Populate TTFT measurements
    for i in 1..=100 {
        profiler.record_ttft("gemma-4-E4B", i as f64, 32 + i);
    }

    // Populate receive-to-stream measurements
    for i in 1..=50 {
        profiler.record_receive_to_stream("websocket_jsonrpc", i as f64 * 0.5);
    }

    // Populate WebSocket chunk transit measurements
    for i in 1..=20 {
        profiler.record_ws_transit(0x01, i as f64 * 0.2, 512);
    }

    // Populate audio stage measurements
    profiler.record_audio_latency("stt_turnaround", 42.0);
    profiler.record_audio_latency("tts_first_chunk", 18.5);

    let summary = profiler.get_latency_summary();

    // Verify TTFT percentiles
    let ttft = &summary["ttft"];
    assert_eq!(ttft["count"], 100);
    assert_eq!(ttft["min_ms"], 1.0);
    assert_eq!(ttft["max_ms"], 100.0);
    assert_eq!(ttft["latest_ms"], 100.0);
    assert!((ttft["p50_ms"].as_f64().unwrap() - 50.0).abs() <= 1.0);
    assert!((ttft["p95_ms"].as_f64().unwrap() - 95.0).abs() <= 1.0);
    assert!((ttft["avg_ms"].as_f64().unwrap() - 50.5).abs() <= 0.1);

    // Verify receive-to-stream percentiles
    let rts = &summary["receive_to_stream"];
    assert_eq!(rts["count"], 50);
    assert_eq!(rts["min_ms"], 0.5);
    assert_eq!(rts["max_ms"], 25.0);

    // Verify WS transit percentiles
    let ws = &summary["ws_transit"];
    assert_eq!(ws["count"], 20);
    assert_eq!(ws["min_ms"], 0.2);
    assert_eq!(ws["max_ms"], 4.0);

    // Verify Audio latency summary
    let audio = &summary["audio"];
    assert_eq!(audio["count"], 2);

    // Test ring buffer eviction overflow
    for i in 101..=250 {
        profiler.record_ttft("gemma-4-E4B", i as f64, 100);
    }
    let summary_overflow = profiler.get_latency_summary();
    assert_eq!(
        summary_overflow["ttft"]["count"], 128,
        "Ring buffer capacity of 128 must be respected"
    );
}

// ── Test 8: IPC Commands Integration (`system:diagnostics`, `system:telemetry`)

#[tokio::test]
async fn test_m4_ipc_system_diagnostics_and_telemetry_commands() {
    let state = make_test_app_state().await;

    // 1. Dispatch `system:diagnostics` command
    let diag_res = handle_command(state.clone(), "system:diagnostics", serde_json::json!({}), None, None)
        .await
        .expect("handle_command system:diagnostics must succeed");

    assert!(diag_res.get("overall_status").is_some());
    assert!(diag_res.get("llm").is_some());
    assert!(diag_res.get("audio").is_some());
    assert!(diag_res.get("database").is_some());
    assert!(diag_res.get("network").is_some());
    assert!(diag_res.get("browser").is_some());

    // 2. Dispatch `system_diagnostic_probe` alias command
    let probe_res = handle_command(state.clone(), "system_diagnostic_probe", serde_json::json!({}), None, None)
        .await
        .expect("handle_command system_diagnostic_probe must succeed");
    assert!(probe_res.get("overall_status").is_some());

    // 3. Dispatch `system:telemetry` command
    let telemetry_res = handle_command(state.clone(), "system:telemetry", serde_json::json!({}), None, None)
        .await
        .expect("handle_command system:telemetry must succeed");

    assert!(telemetry_res.get("latencies").is_some());
    assert!(telemetry_res.get("recent_events").is_some());
    assert!(telemetry_res.get("resource_history").is_some());

    // 4. Verify `get_system_status` reflects telemetry ring buffer
    global_telemetry().record_event("info", "test", "Diagnostic sweep completed", None);
    let status_res = system_status(state.clone())
        .await
        .expect("system_status must succeed");

    let events = status_res["telemetry"]
        .as_array()
        .expect("telemetry in system_status must be a JSON array");
    assert!(!events.is_empty(), "telemetry array must contain recorded events");
    assert!(
        events
            .iter()
            .any(|e| e["message"].as_str() == Some("Diagnostic sweep completed")),
        "Recorded telemetry event must appear in get_system_status"
    );
}

// ── Test 9: Authorization Matrix & Fail-Closed Security ──────────────────────

#[test]
fn test_m4_authorization_fail_closed_matrix() {
    let diagnostic_commands = [
        "system:diagnostics",
        "system_diagnostic_probe",
        "system_diagnostic",
        "system:telemetry",
    ];

    // Authorized principals: LocalCli, Test, TauriDashboard, WebSocketDashboard
    for cmd in diagnostic_commands {
        assert!(
            authorize_command(CommandPrincipal::LocalCli, cmd).is_ok(),
            "LocalCli must be authorized for '{cmd}'"
        );
        assert!(
            authorize_command(CommandPrincipal::Test, cmd).is_ok(),
            "Test must be authorized for '{cmd}'"
        );
        assert!(
            authorize_command(CommandPrincipal::TauriDashboard, cmd).is_ok(),
            "TauriDashboard must be authorized for '{cmd}'"
        );
        assert!(
            authorize_command(CommandPrincipal::WebSocketDashboard, cmd).is_ok(),
            "WebSocketDashboard must be authorized for '{cmd}'"
        );
    }

    // Fail-Closed Unauthorized principals: TauriWidget, TauriSetup, WebSocketWidget, WebSocketRemote, Telegram
    let weak_principals = [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::WebSocketWidget,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::Telegram,
    ];

    for principal in weak_principals {
        for cmd in diagnostic_commands {
            assert!(
                authorize_command(principal, cmd).is_err(),
                "{principal:?} MUST BE DENIED access to diagnostic command '{cmd}'"
            );
        }
    }
}

// ── Test 10: High-Concurrency Stress Testing on Diagnostics & Telemetry ──────

#[tokio::test]
async fn test_m4_concurrent_stress_diagnostics_and_telemetry() {
    let state = make_test_app_state().await;
    let mut handles = Vec::new();

    // Spawn 20 concurrent diagnostic sweeps and telemetry writes
    for i in 0..20 {
        let state_clone = state.clone();
        handles.push(tokio::spawn(async move {
            // Concurrently record telemetry
            global_telemetry().record_ttft("concurrent_stress_model", 50.0 + (i as f64 * 2.0), 64);
            global_telemetry().record_receive_to_stream("stress_path", 10.0 + (i as f64));
            global_telemetry().record_ws_transit(0x01, 1.5, 1024);
            global_telemetry().record_event("info", "stress", &format!("Stress iteration {}", i), None);

            // Execute full diagnostic sweep
            let report = run_system_diagnostic(state_clone).await;
            assert!(report.is_ok(), "Concurrent diagnostic sweep {} must succeed", i);

            // Execute system:telemetry command
            let telemetry_res = global_telemetry().get_telemetry_snapshot();
            assert!(telemetry_res.get("latencies").is_some());
        }));
    }

    for (idx, handle) in handles.into_iter().enumerate() {
        let res = handle.await;
        assert!(
            res.is_ok(),
            "Concurrent worker {} panicked or failed: {:?}",
            idx,
            res
        );
    }
}
