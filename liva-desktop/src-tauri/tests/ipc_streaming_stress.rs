use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::{
    authorize_command, handle_command_as, websocket::WebSocketSessionAuthority, AppState,
    CommandPrincipal,
};
use liva_native_core::{db, llm, stt, tts};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

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

// ─────────────────────────────────────────────────────────────────────────────
// 1. STREAMING CHANNEL BACKPRESSURE & CAPACITY 100 STRESS TESTS
// ─────────────────────────────────────────────────────────────────────────────

#[tokio::test]
async fn test_streaming_channel_bounded_capacity_100_backpressure() {
    let (tx, mut rx) = mpsc::channel::<String>(100);

    // Verify channel capacity is strictly bounded at 100
    for i in 0..100 {
        tx.try_send(format!("{{\"chunk\": {i}}}"))
            .expect("first 100 messages must fit");
    }

    // 101st try_send must fail with Full
    let overflow = tx.try_send("{\"chunk\": 100}".to_string());
    assert!(
        overflow.is_err(),
        "101st message must trigger channel backpressure / full condition"
    );

    // Draining 50 items allows exactly 50 more items to be queued
    for _ in 0..50 {
        let msg = rx.recv().await.expect("must receive item");
        assert!(msg.contains("chunk"));
    }

    for i in 100..150 {
        tx.try_send(format!("{{\"chunk\": {i}}}"))
            .expect("drained slots must accept new items");
    }

    // Full again
    assert!(tx.try_send("{\"chunk\": 150}".to_string()).is_err());
}

#[tokio::test]
async fn test_streaming_producer_consumer_concurrency_flood_5000() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let produced = Arc::new(AtomicUsize::new(0));
    let consumed = Arc::new(AtomicUsize::new(0));

    let produced_clone = produced.clone();
    let producer_handle = tokio::spawn(async move {
        for i in 0..5000 {
            let msg = format!("{{\"seq\": {i}, \"data\": \"sample payload content\"}}");
            tx.send(msg)
                .await
                .expect("send must succeed while receiver is alive");
            produced_clone.fetch_add(1, Ordering::Relaxed);
        }
    });

    let consumed_clone = consumed.clone();
    let consumer_handle = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            // Verify valid JSON parsing as done in tauri forwarder
            if serde_json::value::RawValue::from_string(msg).is_ok() {
                consumed_clone.fetch_add(1, Ordering::Relaxed);
            }
        }
    });

    producer_handle.await.expect("producer finished cleanly");
    consumer_handle.await.expect("consumer finished cleanly");

    assert_eq!(produced.load(Ordering::SeqCst), 5000);
    assert_eq!(consumed.load(Ordering::SeqCst), 5000);
}

#[tokio::test]
async fn test_streaming_early_receiver_drop_does_not_panic_producer() {
    let (tx, rx) = mpsc::channel::<String>(100);

    // Drop receiver immediately
    drop(rx);

    // Producer send should return Err, never panic
    let result = tx.send("{\"test\": 1}".to_string()).await;
    assert!(result.is_err(), "send to dropped receiver must return Err");

    // blocking_send should also return Err, never panic
    let result_blocking =
        tokio::task::spawn_blocking(move || tx.blocking_send("{\"test\": 2}".to_string()))
            .await
            .unwrap();
    assert!(
        result_blocking.is_err(),
        "blocking_send to dropped receiver must return Err"
    );
}

#[tokio::test]
async fn test_streaming_corrupted_non_json_recovery() {
    let (tx, mut rx) = mpsc::channel::<String>(100);
    let valid_count = Arc::new(AtomicUsize::new(0));
    let valid_count_clone = valid_count.clone();

    let forwarder = tokio::spawn(async move {
        while let Some(msg) = rx.recv().await {
            match serde_json::value::RawValue::from_string(msg) {
                Ok(_) => {
                    valid_count_clone.fetch_add(1, Ordering::Relaxed);
                }
                Err(_) => {
                    // Simulates Tauri's graceful non-JSON chunk skip without aborting the loop
                }
            }
        }
    });

    // Send a mixture of valid JSON, malformed strings, binary-like text, and empty chunks
    tx.send("{\"valid\": 1}".to_string()).await.unwrap();
    tx.send("NOT_JSON_RAW_STRING".to_string()).await.unwrap();
    tx.send("{broken json".to_string()).await.unwrap();
    tx.send("{\"valid\": 2}".to_string()).await.unwrap();
    tx.send("\0\0\0binary_corrupt".to_string()).await.unwrap();
    tx.send("{\"valid\": 3}".to_string()).await.unwrap();
    drop(tx);

    forwarder.await.unwrap();

    // Exactly 3 valid JSON chunks must be processed; malformed chunks skipped safely
    assert_eq!(valid_count.load(Ordering::SeqCst), 3);
}

// ─────────────────────────────────────────────────────────────────────────────
// 2. CAPABILITY ISOLATION ACROSS WINDOW PRINCIPALS
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn test_widget_strict_capability_isolation_matrix() {
    let p = CommandPrincipal::TauriWidget;

    // Allowed conversational/query commands
    let allowed = [
        "ping",
        "status",
        "get_config",
        "get_ai_config",
        "get_voice_status",
        "get_voice_profiles",
        "get_system_status",
        "get_user_profile",
        "get_avatar_models",
        "llm:health_check",
        "chat:completion",
        "vision:capture",
        "vision:ask",
        "voice:stt_start",
        "voice:stt_chunk",
        "voice:stt_stop",
        "voice:tts_speak",
        "voice:tts_stop",
        "message:draft",
        "message:confirm",
        "message:cancel",
        "message:pending",
        "messenger:status",
    ];
    for cmd in allowed {
        assert!(
            authorize_command(p, cmd).is_ok(),
            "Widget must allow: {cmd}"
        );
    }

    // Strictly forbidden administrative, setup, memory mutation, and raw tool commands
    let forbidden = [
        "update_config",
        "get_preflight_status",
        "get_skills_list",
        "toggle_skill",
        "toggle_all_skills",
        "update_user_profile",
        "import_avatar_folder",
        "delete_avatar_model",
        "consent:get",
        "consent:grant",
        "consent:revoke",
        "integrations:list",
        "llm:embed",
        "task_plan_chat",
        "get_memory_data",
        "memory:set_fact",
        "memory:get_fact",
        "delete_memory_fact",
        "memory:delete_conversation",
        "memory:delete_subject",
        "memory:sweep_retention",
        "consolidate_memory",
        "reset_memory",
        "memory:search_hybrid",
        "memory:upsert_vector",
        "contacts:list",
        "contacts:upsert",
        "contacts:delete",
        "get_tasks",
        "add_task",
        "delete_task",
        "update_task",
        "vision:add_region",
        "vision:remove_region",
        "vision:get_changed_regions",
        "vision:set_config",
        "voice:set_language",
        "voice:list_vieneu_voices",
        "voice:set_vieneu_voice",
        "mcp:list_tools",
        "mcp:call_tool",
        "mcp_client:list_servers",
        "mcp_client:list_tools",
        "skills:list",
        "skills:search",
        "skills:signals",
        "skills:history",
        "setup:status",
        "setup:paths",
        "setup:fetch",
    ];
    for cmd in forbidden {
        assert!(
            authorize_command(p, cmd).is_err(),
            "Widget must DENY: {cmd}"
        );
    }
}

#[test]
fn test_setup_strict_capability_isolation_matrix() {
    let p = CommandPrincipal::TauriSetup;

    // Allowed setup-only commands
    let allowed = ["setup:status", "setup:paths", "setup:fetch"];
    for cmd in allowed {
        assert!(authorize_command(p, cmd).is_ok(), "Setup must allow: {cmd}");
    }

    // Strictly forbidden all standard application, conversational, memory, and config commands
    let forbidden = [
        "ping",
        "echo",
        "status",
        "get_config",
        "update_config",
        "chat:completion",
        "get_memory_data",
        "memory:set_fact",
        "reset_memory",
        "get_tasks",
        "vision:capture",
        "vision:ask",
        "voice:stt_start",
        "voice:tts_speak",
        "message:draft",
        "mcp:list_tools",
        "mcp:call_tool",
        "skills:list",
    ];
    for cmd in forbidden {
        assert!(authorize_command(p, cmd).is_err(), "Setup must DENY: {cmd}");
    }
}

#[test]
fn test_dashboard_strict_capability_isolation_matrix() {
    let p = CommandPrincipal::TauriDashboard;

    // Allowed dashboard commands
    let allowed = [
        "ping",
        "echo",
        "status",
        "get_config",
        "update_config",
        "get_ai_config",
        "get_voice_status",
        "get_voice_profiles",
        "select_voice_profile",
        "get_system_status",
        "get_preflight_status",
        "get_skills_list",
        "toggle_skill",
        "toggle_all_skills",
        "get_user_profile",
        "update_user_profile",
        "get_avatar_models",
        "import_avatar_folder",
        "delete_avatar_model",
        "consent:get",
        "consent:grant",
        "consent:revoke",
        "integrations:list",
        "llm:embed",
        "llm:health_check",
        "chat:completion",
        "task_plan_chat",
        "get_memory_data",
        "memory:set_fact",
        "memory:get_fact",
        "delete_memory_fact",
        "memory:delete_conversation",
        "memory:delete_subject",
        "memory:sweep_retention",
        "consolidate_memory",
        "reset_memory",
        "memory:search_hybrid",
        "memory:upsert_vector",
        "contacts:list",
        "contacts:upsert",
        "contacts:delete",
        "message:draft",
        "message:confirm",
        "message:cancel",
        "message:pending",
        "messenger:status",
        "get_tasks",
        "add_task",
        "delete_task",
        "update_task",
        "vision:capture",
        "vision:add_region",
        "vision:remove_region",
        "vision:get_changed_regions",
        "vision:set_config",
        "vision:ask",
        "voice:stt_start",
        "voice:stt_chunk",
        "voice:stt_stop",
        "voice:set_language",
        "voice:list_vieneu_voices",
        "voice:set_vieneu_voice",
        "voice:tts_speak",
        "voice:tts_stop",
        "mcp:list_tools",
        "mcp_client:list_servers",
        "mcp_client:list_tools",
        "skills:list",
        "skills:search",
        "skills:signals",
        "skills:history",
    ];
    for cmd in allowed {
        assert!(
            authorize_command(p, cmd).is_ok(),
            "Dashboard must allow: {cmd}"
        );
    }

    // Strictly forbidden setup-fetch, arbitrary tool call, and foreign escape commands
    let forbidden = [
        "setup:fetch",
        "setup:paths",
        "setup:status",
        "mcp:call_tool",
        "mcp_client:call_tool",
        "skills:pin_ids",
        "telegram:send_text",
        "integration:smart_home_control",
    ];
    for cmd in forbidden {
        assert!(
            authorize_command(p, cmd).is_err(),
            "Dashboard must DENY: {cmd}"
        );
    }
}

#[test]
fn test_adversarial_malformed_command_strings_rejected() {
    let principals = [
        CommandPrincipal::TauriWidget,
        CommandPrincipal::TauriDashboard,
        CommandPrincipal::TauriSetup,
        CommandPrincipal::WebSocketWidget,
        CommandPrincipal::WebSocketDashboard,
        CommandPrincipal::WebSocketRemote,
        CommandPrincipal::Telegram,
    ];

    let attack_payloads = [
        "",
        " ",
        "\n",
        "\t",
        "\0",
        "../../../etc/passwd",
        "..\\..\\windows\\system32\\cmd.exe",
        "; DROP TABLE users; --",
        "' OR '1'='1",
        "<script>alert(1)</script>",
        "ping\0inject",
        "chat:completion\nupdate_config",
        "eval(process.exit(1))",
        "__proto__",
        "constructor",
    ];

    for p in principals {
        for attack in attack_payloads {
            assert!(
                authorize_command(p, attack).is_err(),
                "Principal {p:?} must REJECT attack string: {attack:?}"
            );
        }
    }
}

#[tokio::test]
async fn test_in_process_ipc_execution_performance_and_isolation() {
    let state = test_state();

    // 1. Valid in-process ping returns sub-millisecond Result
    let start = std::time::Instant::now();
    let result = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "ping",
        serde_json::json!({}),
        None,
        None,
    )
    .await
    .expect("in-process ping must succeed");
    let elapsed = start.elapsed();
    assert_eq!(result, serde_json::json!({"pong": true}));
    assert!(
        elapsed < Duration::from_millis(10),
        "In-process memory call took {elapsed:?}, expected <10ms"
    );

    // 2. Unauthorized in-process call is rejected before reaching handler
    let err = handle_command_as(
        CommandPrincipal::TauriWidget,
        state.clone(),
        "reset_memory",
        serde_json::json!({}),
        None,
        None,
    )
    .await
    .expect_err("Widget must not be allowed to reset memory");
    assert!(err.contains("TauriWidget"));
    assert!(err.contains("reset_memory"));

    // 3. Setup window in-process call isolation
    let setup_err = handle_command_as(
        CommandPrincipal::TauriSetup,
        state.clone(),
        "chat:completion",
        serde_json::json!({"messages": []}),
        None,
        None,
    )
    .await
    .expect_err("Setup must not be allowed to invoke chat:completion");
    assert!(setup_err.contains("TauriSetup"));
}

#[test]
fn test_websocket_session_authority_principal_issuance() {
    let authority = WebSocketSessionAuthority::new();

    // Widget ticket
    let widget_ticket = authority
        .issue(CommandPrincipal::WebSocketWidget)
        .expect("widget ticket");
    assert_eq!(widget_ticket.token.len(), 64);
    assert!(widget_ticket.expires_in_ms > 0);

    // Dashboard ticket
    let dash_ticket = authority
        .issue(CommandPrincipal::WebSocketDashboard)
        .expect("dashboard ticket");
    assert_eq!(dash_ticket.token.len(), 64);
    assert_ne!(widget_ticket.token, dash_ticket.token);

    // Setup and other unauthorized principals must be rejected for ticket issuance
    assert!(authority.issue(CommandPrincipal::TauriSetup).is_err());
    assert!(authority.issue(CommandPrincipal::WebSocketRemote).is_err());
    assert!(authority.issue(CommandPrincipal::Telegram).is_err());
}
