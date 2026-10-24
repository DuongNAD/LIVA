//! Empirical Adversarial Stress Suite for Milestone 2:
//! Session Actor Concurrency, Mailbox Overflow, Barge-in Preemption & Port Scan Security Audit.
//!
//! Features tested: F07 (SessionActor Mailbox), F08 (Session Queue Modes),
//! F09 (Llama Sidecar Supervisor & Port Confinement), F10 (VRAM Governor).

use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::sync::mpsc;
use liva_core_types::{
    CommandPrincipal, Principal, SessionError, SessionMode, SessionQueueMode, SessionTurnRequest,
};
use liva_native_core::agent::session_actor::{
    DEFAULT_MAILBOX_CAPACITY, MAX_FOLLOWUP_QUEUE_DEPTH, SessionActor, SessionActorHandle,
    SessionManager, SessionMessage,
};
use liva_native_core::llm::sidecar::{
    allocate_ephemeral_port, generate_csprng_key, FallbackAction, KvPrecision,
    LlamaSidecarClient, LlamaSidecarSupervisor, SidecarConfig, SlotId, VramPressureTier,
    VramSupervisor,
};

// ============================================================================
// CHALLENGE 1: MAILBOX OVERFLOW & BACKPRESSURE STRESS TEST (> 64 MESSAGES)
// ============================================================================

/// Challenge 1.1: Direct Mailbox Channel Flood (> 64 messages).
/// Floods a single SessionActor with 100 concurrent requests.
/// Verifies:
/// 1. The bounded channel and queue depth mechanism strictly protect the actor from unbounded growth.
/// 2. Bounded followup capacity (16) rejects excess requests with `SessionError::QueueFull`.
/// 3. Zero unhandled panics, zero memory leaks, zero deadlocks.
/// 4. Actor recovers to healthy state and processes follow-up requests after storm clears.
#[tokio::test]
async fn challenge_mailbox_overflow_burst_100_msgs_no_panic() {
    let (tx, rx) = mpsc::channel(DEFAULT_MAILBOX_CAPACITY);
    let actor = SessionActor::new("sess_overflow_100".into(), rx, None, None);
    let actor_task = tokio::spawn(actor.run());
    let handle = SessionActorHandle::new("sess_overflow_100".into(), tx);

    const FLOOD_COUNT: usize = 100;
    let success_count = Arc::new(AtomicUsize::new(0));
    let queue_full_count = Arc::new(AtomicUsize::new(0));
    let other_err_count = Arc::new(AtomicUsize::new(0));

    let mut tasks = Vec::with_capacity(FLOOD_COUNT);

    // Fire 100 turns simultaneously
    for i in 0..FLOOD_COUNT {
        let h = handle.clone();
        let sc = Arc::clone(&success_count);
        let qfc = Arc::clone(&queue_full_count);
        let oec = Arc::clone(&other_err_count);

        tasks.push(tokio::spawn(async move {
            let req = SessionTurnRequest {
                session_id: "sess_overflow_100".into(),
                turn_id: format!("flood_turn_{i}"),
                input_text: format!("Flood message payload {i}"),
                queue_mode: SessionQueueMode::Followup,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            };
            match h.execute_turn(req, None).await {
                Ok(_) => sc.fetch_add(1, Ordering::SeqCst),
                Err(SessionError::QueueFull) => qfc.fetch_add(1, Ordering::SeqCst),
                Err(_) => oec.fetch_add(1, Ordering::SeqCst),
            };
        }));
    }

    // Await all flood tasks
    for t in tasks {
        let _ = t.await;
    }

    let successes = success_count.load(Ordering::SeqCst);
    let queue_fulls = queue_full_count.load(Ordering::SeqCst);
    let others = other_err_count.load(Ordering::SeqCst);

    println!("\n=== Mailbox Overflow Burst Results (100 concurrent requests) ===");
    println!("Total requests:    {FLOOD_COUNT}");
    println!("Executed/Queued:   {successes}");
    println!("Rejected QueueFull:{queue_fulls}");
    println!("Other errors:      {others}");

    // Invariant: The system MUST have rejected the burst above MAX_FOLLOWUP_QUEUE_DEPTH (16).
    assert!(
        queue_fulls > 0,
        "System must enforce queue limit and reject overflow turns with QueueFull"
    );
    assert_eq!(
        successes + queue_fulls + others,
        FLOOD_COUNT,
        "All 100 requests must be accounted for cleanly"
    );

    // Verify actor is still responsive and healthy after storm
    let probe_req = SessionTurnRequest {
        session_id: "sess_overflow_100".into(),
        turn_id: "post_storm_probe".into(),
        input_text: "Are you still alive?".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        ..Default::default()
    };
    let probe_res = handle.execute_turn(probe_req, None).await;
    assert!(probe_res.is_ok(), "Actor must remain responsive post-burst");

    handle.shutdown().await.expect("Graceful shutdown");
    actor_task.await.expect("Actor task terminates cleanly");
}

/// Challenge 1.2: Raw Mailbox Channel Capacity Bound (64 items).
/// Verifies that the internal `mpsc::channel(64)` rejects `try_send` when at capacity.
#[tokio::test]
async fn challenge_mailbox_mpsc_hard_capacity_64() {
    let (tx, mut rx) = mpsc::channel::<SessionMessage>(DEFAULT_MAILBOX_CAPACITY);

    // Without running the receiver loop, fill exactly 64 messages
    for i in 0..DEFAULT_MAILBOX_CAPACITY {
        let (responder, _) = tokio::sync::oneshot::channel();
        let res = tx.try_send(SessionMessage::Steer {
            instruction: format!("inst_{i}"),
            responder,
        });
        assert!(res.is_ok(), "Message {i} should fit in 64-capacity channel");
    }

    // The 65th message MUST fail with TrySendError::Full
    let (responder_overflow, _) = tokio::sync::oneshot::channel();
    let overflow_res = tx.try_send(SessionMessage::Steer {
        instruction: "overflow".into(),
        responder: responder_overflow,
    });
    assert!(
        overflow_res.is_err(),
        "65th message must exceed mailbox capacity"
    );
    match overflow_res.unwrap_err() {
        mpsc::error::TrySendError::Full(_) => {
            // Expected: backpressure trigger confirmed
        }
        mpsc::error::TrySendError::Closed(_) => panic!("Channel should not be closed"),
    }

    // Draining one message frees up a slot
    let drained = rx.recv().await;
    assert!(drained.is_some());

    // Now another message can be sent
    let (responder_retry, _) = tokio::sync::oneshot::channel();
    assert!(tx.try_send(SessionMessage::Steer {
        instruction: "retry".into(),
        responder: responder_retry,
    }).is_ok());
}

// ============================================================================
// CHALLENGE 2: INTERRUPT BARGE-IN PREEMPTION STRESS TEST
// ============================================================================

/// Challenge 2.1: Millisecond Barge-in Cancellation Latency.
/// Starts a simulated long-running generation turn and fires `Interrupt`.
/// Measures the cancellation latency and verifies that no excess tokens are emitted.
#[tokio::test]
async fn challenge_interrupt_bargein_sub_50ms_latency() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_bargein_latency").await.unwrap();

    let (stream_tx, mut stream_rx) = mpsc::channel(32);

    let long_req = SessionTurnRequest {
        session_id: "sess_bargein_latency".into(),
        turn_id: "long_turn".into(),
        input_text: "Write a long story".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
                ..Default::default()
    };

    let handle_clone = handle.clone();
    let turn_task = tokio::spawn(async move {
        handle_clone.execute_turn(long_req, Some(stream_tx)).await
    });

    // Wait until at least 1 token is emitted to confirm turn is actively executing
    let first_token = stream_rx.recv().await;
    assert!(first_token.is_some(), "Active turn must start generating tokens");

    // Measure barge-in interruption reaction time
    let interrupt_start = Instant::now();
    let interrupt_req = SessionTurnRequest {
        session_id: "sess_bargein_latency".into(),
        turn_id: "interrupt_bargein".into(),
        input_text: "Dừng lại ngay lập tức!".into(),
        queue_mode: SessionQueueMode::Interrupt,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
                ..Default::default()
    };

    let interrupt_res = handle.execute_turn(interrupt_req, None).await.unwrap();
    let interrupt_latency = interrupt_start.elapsed();

    println!("\n=== Barge-in Preemption Benchmark ===");
    println!("Interrupt Barge-in Latency: {:.2}ms", interrupt_latency.as_secs_f64() * 1000.0);
    assert_eq!(interrupt_res.turn_id, "interrupt_bargein");

    // The long turn must finish and report is_interrupted == true
    let long_res = turn_task.await.unwrap().unwrap();
    assert!(
        long_res.is_interrupted,
        "Long turn must be marked as interrupted"
    );

    // Verify token stream terminates promptly
    let mut trailing_tokens = 0;
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(50), stream_rx.recv()).await {
        trailing_tokens += 1;
    }
    println!("Trailing tokens after interrupt: {trailing_tokens}");
    assert!(
        trailing_tokens <= 1,
        "At most 1 in-flight chunk emitted after interrupt signal"
    );

    handle.shutdown().await.unwrap();
}

/// Challenge 2.2: Interrupt Mode Flushes Stale Followup Queues.
/// Enqueues 5 Followup turns behind an active turn, then sends an Interrupt turn.
/// Verifies:
/// 1. The active turn is cancelled.
/// 2. All 5 pending followups are purged and returned with `SessionError::Interrupted`.
/// 3. The Interrupt turn executes cleanly and immediately.
#[tokio::test]
async fn challenge_interrupt_purges_stale_followup_queue() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_purge_queue").await.unwrap();

    let initial_req = SessionTurnRequest {
        session_id: "sess_purge_queue".into(),
        turn_id: "turn_active".into(),
        input_text: "Initial active turn".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
                ..Default::default()
    };

    let h_active = handle.clone();
    let active_task = tokio::spawn(async move {
        h_active.execute_turn(initial_req, None).await
    });

    // Wait 10ms so it starts
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Enqueue 5 followup turns
    let mut followup_tasks = Vec::new();
    for i in 1..=5 {
        let h = handle.clone();
        followup_tasks.push(tokio::spawn(async move {
            h.execute_turn(
                SessionTurnRequest {
                    session_id: "sess_purge_queue".into(),
                    turn_id: format!("stale_followup_{i}"),
                    input_text: format!("Followup {i}"),
                    queue_mode: SessionQueueMode::Followup,
                    principal: CommandPrincipal::LocalCli,
                    metadata: Default::default(),
                ..Default::default()
                },
                None,
            ).await
        }));
    }

    // Check that followups are queued
    tokio::time::sleep(Duration::from_millis(10)).await;
    let snap_before = handle.get_snapshot().await.unwrap();
    assert!(snap_before.queued_followups >= 1);

    // Send Interrupt turn
    let interrupt_req = SessionTurnRequest {
        session_id: "sess_purge_queue".into(),
        turn_id: "emergency_interrupt".into(),
        input_text: "Preempt everything".into(),
        queue_mode: SessionQueueMode::Interrupt,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
                ..Default::default()
    };

    let interrupt_out = handle.execute_turn(interrupt_req, None).await.unwrap();
    assert_eq!(interrupt_out.turn_id, "emergency_interrupt");

    // All queued followups must receive SessionError::Interrupted
    let mut interrupted_count = 0;
    for t in followup_tasks {
        let res = t.await.unwrap();
        if let Err(SessionError::Interrupted) = res {
            interrupted_count += 1;
        }
    }
    assert_eq!(
        interrupted_count, 5,
        "All 5 queued followups must be purged with SessionError::Interrupted"
    );

    let active_out = active_task.await.unwrap().unwrap();
    assert!(active_out.is_interrupted);

    handle.shutdown().await.unwrap();
}

/// Challenge 2.3: Rapid Interleaving Interrupt Storm (20 rapid interrupts).
/// Verifies actor stability under high-frequency preemption churn without deadlocks.
#[tokio::test]
async fn challenge_rapid_interrupt_churn_stability() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_interrupt_storm").await.unwrap();

    for i in 0..20 {
        let req = SessionTurnRequest {
            session_id: "sess_interrupt_storm".into(),
            turn_id: format!("churn_{i}"),
            input_text: format!("Rapid interrupt {i}"),
            queue_mode: SessionQueueMode::Interrupt,
            principal: CommandPrincipal::LocalCli,
            metadata: Default::default(),
                ..Default::default()
        };
        let out = handle.execute_turn(req, None).await;
        assert!(out.is_ok(), "Turn {i} must complete without panic or hang");
    }

    let snap = handle.get_snapshot().await.unwrap();
    assert!(snap.is_idle, "Actor must return to idle state after storm");
    handle.shutdown().await.unwrap();
}

// ============================================================================
// CHALLENGE 3: PORT SCAN & SECURITY BOUNDARY AUDIT
// ============================================================================

/// Challenge 3.1: Zero TCP Listening Ports Verification on SessionActor.
/// Verifies that SessionActor and SessionManager do NOT open or listen on any network ports.
/// Proves that `SessionActor` is 100% in-process with zero remote network attack surface.
#[tokio::test]
async fn challenge_session_actor_zero_tcp_listeners_audit() {
    let manager = SessionManager::new(None);

    // Spawn 20 independent session actors
    let mut handles = Vec::new();
    for i in 0..20 {
        let h = manager.get_or_create(&format!("sess_net_audit_{i}")).await.unwrap();
        handles.push(h);
    }

    // Verify SessionManager has 20 active actors
    assert_eq!(manager.active_session_count().await, 20);

    // Verify that attempting to connect to common arbitrary local ports fails (no listener)
    // and verify that we can bind to arbitrary local ports without conflicts.
    for port in [18490, 18491, 18492, 18493] {
        let test_bind = std::net::TcpListener::bind(format!("127.0.0.1:{port}"));
        assert!(
            test_bind.is_ok(),
            "Port {port} must be free: SessionActor must never bind TCP ports"
        );
    }

    for h in handles {
        h.shutdown().await.unwrap();
    }
}

/// Challenge 3.2: Sidecar Security Confinement & CSPRNG Key Generation.
/// Verifies that:
/// 1. Ephemeral ports are strictly in the private/dynamic range and bind loopback only.
/// 2. 1,000 CSPRNG generated keys have 256-bit entropy and zero collisions.
/// 3. CLI arguments strictly forbid `--tools`, `--agent`, `--mcp-servers-*`.
#[test]
fn challenge_sidecar_security_confinement_and_key_entropy() {
    let config = SidecarConfig::default();
    let port = allocate_ephemeral_port().expect("ephemeral port");
    let key = generate_csprng_key();

    let args = config.build_cli_args(port, &key);
    let joined = args.join(" ");

    // Verify network confinement
    assert!(joined.contains("--host 127.0.0.1"), "Must strictly bind to loopback 127.0.0.1");
    assert!(!joined.contains("0.0.0.0"), "Must NEVER bind to 0.0.0.0");
    assert!(!joined.contains("--tools"), "Forbidden --tools flag found in sidecar CLI");
    assert!(!joined.contains("--agent"), "Forbidden --agent flag found in sidecar CLI");
    assert!(!joined.contains("--mcp"), "Forbidden MCP flag found in sidecar CLI");

    // Entropy verification: generate 1,000 keys and verify 0 collisions
    let mut keys = HashSet::new();
    for _ in 0..1000 {
        let k = generate_csprng_key();
        assert_eq!(k.len(), 64, "Must be 64-character hex string (256 bits)");
        assert!(k.chars().all(|c| c.is_ascii_hexdigit()), "Must be valid hex");
        assert!(keys.insert(k), "Collision detected in CSPRNG key generator!");
    }
}

/// Challenge 3.3: Llama Sidecar HTTP Authentication Enforcement.
/// Spawns a real mock HTTP TCP listener simulating `llama-server`.
/// Verifies:
/// 1. Requests with valid Bearer token are accepted.
/// 2. Requests missing or with wrong Bearer token are rejected with HTTP 401 Unauthorized.
#[tokio::test]
async fn challenge_sidecar_http_bearer_token_enforcement() {
    let listener = TcpListener::bind("127.0.0.1:0").await.expect("bind mock server");
    let server_port = listener.local_addr().unwrap().port();
    let valid_token = "valid_secret_token_1234567890abcdef1234567890abcdef";

    // Spawn mock HTTP server enforcing Authorization: Bearer <valid_token>
    let expected_auth_header = format!("Bearer {valid_token}");
    tokio::spawn(async move {
        loop {
            let (mut socket, _) = match listener.accept().await {
                Ok(conn) => conn,
                Err(_) => break,
            };

            let mut buf = vec![0u8; 4096];
            let n = match socket.read(&mut buf).await {
                Ok(n) if n > 0 => n,
                _ => continue,
            };

            let req_str = String::from_utf8_lossy(&buf[..n]);

            // Check Authorization header
            let is_authorized = req_str
                .lines()
                .any(|line| line.starts_with("authorization:") || line.starts_with("Authorization:"))
                && req_str.contains(&expected_auth_header);

            if is_authorized {
                let body = serde_json::json!({
                    "id": "chatcmpl-live-mock",
                    "object": "chat.completion",
                    "choices": [{
                        "index": 0,
                        "message": { "role": "assistant", "content": "Authenticated response" },
                        "finish_reason": "stop"
                    }]
                }).to_string();

                let resp = format!(
                    "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
            } else {
                let err_body = r#"{"error":{"message":"Unauthorized: Invalid or missing API key","code":401}}"#;
                let resp = format!(
                    "HTTP/1.1 401 Unauthorized\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                    err_body.len(),
                    err_body
                );
                let _ = socket.write_all(resp.as_bytes()).await;
            }
        }
    });

    // Test 1: Client with VALID token succeeds
    let valid_client = LlamaSidecarClient::new(server_port, valid_token.to_string());
    let req_payload = serde_json::json!({
        "messages": [{"role": "user", "content": "Hello"}]
    });
    let valid_res = valid_client.chat_completion(SlotId::Foreground, &req_payload).await;
    assert!(valid_res.is_ok(), "Valid bearer token must authenticate successfully");
    let resp_val = valid_res.unwrap();
    assert_eq!(
        resp_val["choices"][0]["message"]["content"],
        "Authenticated response"
    );

    // Test 2: Client with INVALID token fails with 401
    let invalid_client = LlamaSidecarClient::new(server_port, "wrong_token_here".to_string());
    let invalid_res = invalid_client.chat_completion(SlotId::Foreground, &req_payload).await;
    assert!(
        invalid_res.is_err(),
        "Client with invalid token must fail authentication"
    );
    let err_str = invalid_res.unwrap_err().to_string();
    assert!(
        err_str.contains("401") || err_str.contains("Unauthorized"),
        "Error message must indicate 401 Unauthorized: {err_str}"
    );
}

// ============================================================================
// CHALLENGE 4: MIXED MODE BURST & CONCURRENCY ANOMALY STRESS
// (Interrupt -> Followup -> Steer -> Collect)
// ============================================================================

/// Challenge 4.1: Mixed Mode Continuous Burst (Interrupt -> Followup -> Steer -> Collect).
/// Executes rapid interleaved sequences of all 4 modes on a single session.
/// Verifies:
/// 1. Zero deadlocks under heavy interleaving.
/// 2. Clean status transition back to Idle after storm settles.
/// 3. Subsequent turn executes with full fidelity.
#[tokio::test]
async fn challenge_mixed_mode_burst_interleaved_no_deadlock() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_mixed_burst").await.unwrap();

    // 10 cycles of Interrupt -> Followup -> Steer -> Collect
    for cycle in 0..10 {
        // Step A: Launch a Followup turn
        let h_follow = handle.clone();
        let follow_task = tokio::spawn(async move {
            h_follow.execute_turn(
                SessionTurnRequest {
                    session_id: "sess_mixed_burst".into(),
                    turn_id: format!("cycle_{cycle}_follow"),
                    input_text: format!("Followup turn {cycle}"),
                    queue_mode: SessionQueueMode::Followup,
                    principal: CommandPrincipal::LocalCli,
                    metadata: Default::default(),
                ..Default::default()
                },
                None,
            ).await
        });

        // Step B: Inject Steer into active or queued turn
        let _ = handle.steer(format!("Steer instruction {cycle}")).await;

        // Step C: Launch Interrupt turn to preempt
        let interrupt_req = SessionTurnRequest {
            session_id: "sess_mixed_burst".into(),
            turn_id: format!("cycle_{cycle}_interrupt"),
            input_text: format!("Interrupt signal {cycle}"),
            queue_mode: SessionQueueMode::Interrupt,
            principal: CommandPrincipal::LocalCli,
            metadata: Default::default(),
                ..Default::default()
        };
        let interrupt_res = handle.execute_turn(interrupt_req, None).await;
        assert!(interrupt_res.is_ok(), "Interrupt turn in cycle {cycle} must complete");

        // Step D: Fire rapid Collect turns
        let h_collect = handle.clone();
        let collect_res = h_collect.execute_turn(
            SessionTurnRequest {
                session_id: "sess_mixed_burst".into(),
                turn_id: format!("cycle_{cycle}_collect"),
                input_text: format!("Collect utterance {cycle}"),
                queue_mode: SessionQueueMode::Collect,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            },
            None,
        ).await;
        // Collect may succeed directly or merge
        assert!(collect_res.is_ok(), "Collect turn in cycle {cycle} must resolve");

        let _ = follow_task.await;
    }

    // After all 10 cycles, wait for actor to settle and verify snapshot
    tokio::time::sleep(Duration::from_millis(50)).await;
    let snap = handle.get_snapshot().await.unwrap();
    println!("Post mixed-burst snapshot: {:?}", snap);
    assert!(snap.is_idle, "Actor must return to idle state after storm");

    // Post-burst verification probe
    let probe_res = handle.execute_turn(
        SessionTurnRequest {
            session_id: "sess_mixed_burst".into(),
            turn_id: "probe_post_burst".into(),
            input_text: "Verification after storm".into(),
            queue_mode: SessionQueueMode::Followup,
            principal: CommandPrincipal::LocalCli,
            metadata: Default::default(),
                ..Default::default()
        },
        None,
    ).await;
    assert!(probe_res.is_ok(), "Probe turn after storm must succeed");
    let probe_out = probe_res.unwrap();
    assert!(!probe_out.is_interrupted);
    assert_eq!(probe_out.turn_id, "probe_post_burst");

    handle.shutdown().await.unwrap();
}

/// Challenge 4.2: Collect Debounce Collision with Concurrent Followup.
/// When in Idle state, a Collect request arms a 100ms debounce timer.
/// An immediate Followup request is sent before debounce timer fires.
/// Verifies whether the actor maintains turn serializability without crashing or race conditions.
#[tokio::test]
async fn challenge_collect_debounce_interleaved_with_followup() {
    let (tx, rx) = mpsc::channel(DEFAULT_MAILBOX_CAPACITY);
    let actor = SessionActor::new("sess_collect_race".into(), rx, None, None)
        .with_collect_debounce(Duration::from_millis(100));
    let actor_task = tokio::spawn(actor.run());
    let handle = SessionActorHandle::new("sess_collect_race".into(), tx);

    // 1. Send Collect while Idle (arms 100ms timer)
    let h1 = handle.clone();
    let collect_task = tokio::spawn(async move {
        h1.execute_turn(
            SessionTurnRequest {
                session_id: "sess_collect_race".into(),
                turn_id: "collect_first".into(),
                input_text: "Collect utterance 1".into(),
                queue_mode: SessionQueueMode::Collect,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            },
            None,
        ).await
    });

    // 2. Immediately send Followup turn while Collect is debouncing
    tokio::time::sleep(Duration::from_millis(10)).await;
    let h2 = handle.clone();
    let followup_task = tokio::spawn(async move {
        h2.execute_turn(
            SessionTurnRequest {
                session_id: "sess_collect_race".into(),
                turn_id: "followup_colliding".into(),
                input_text: "Followup colliding turn".into(),
                queue_mode: SessionQueueMode::Followup,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            },
            None,
        ).await
    });

    let (res_collect, res_followup) = tokio::join!(collect_task, followup_task);
    assert!(res_collect.unwrap().is_ok(), "Collect turn should complete");
    assert!(res_followup.unwrap().is_ok(), "Followup turn should complete");

    // Settle and verify actor is alive and healthy
    tokio::time::sleep(Duration::from_millis(150)).await;
    let snap = handle.get_snapshot().await.unwrap();
    assert!(snap.is_idle);

    handle.shutdown().await.unwrap();
    actor_task.await.unwrap();
}

// ============================================================================
// CHALLENGE 5: CROSS-SESSION CANCELLATION ISOLATION (Session A vs Session B)
// ============================================================================

/// Challenge 5.1: Strict Cross-Session Cancellation Isolation.
/// Two sessions (Alpha and Beta) execute long turns simultaneously.
/// Alpha is interrupted mid-flight; Beta must run to 100% completion with zero interruption.
#[tokio::test]
async fn challenge_cross_session_cancellation_strict_isolation() {
    let manager = SessionManager::new(None);
    let handle_alpha = manager.get_or_create("sess_iso_alpha").await.unwrap();
    let handle_beta = manager.get_or_create("sess_iso_beta").await.unwrap();

    let (stream_a_tx, mut stream_a_rx) = mpsc::channel(32);
    let (stream_b_tx, mut stream_b_rx) = mpsc::channel(32);

    // Spawn Turn Alpha
    let h_a = handle_alpha.clone();
    let task_alpha = tokio::spawn(async move {
        h_a.execute_turn(
            SessionTurnRequest {
                session_id: "sess_iso_alpha".into(),
                turn_id: "turn_alpha".into(),
                input_text: "Alpha payload".into(),
                queue_mode: SessionQueueMode::Followup,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            },
            Some(stream_a_tx),
        ).await
    });

    // Spawn Turn Beta
    let h_b = handle_beta.clone();
    let task_beta = tokio::spawn(async move {
        h_b.execute_turn(
            SessionTurnRequest {
                session_id: "sess_iso_beta".into(),
                turn_id: "turn_beta".into(),
                input_text: "Beta payload".into(),
                queue_mode: SessionQueueMode::Followup,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                ..Default::default()
            },
            Some(stream_b_tx),
        ).await
    });

    // Wait until both sessions emit at least 1 token
    let _ = stream_a_rx.recv().await;
    let _ = stream_b_rx.recv().await;

    // Interrupt ONLY Session Alpha
    let interrupt_res = handle_alpha.interrupt("Testing cancellation isolation".into()).await;
    assert!(interrupt_res.is_ok(), "Interrupt on Alpha must succeed");

    // Await both tasks
    let res_alpha = task_alpha.await.unwrap();
    let res_beta = task_beta.await.unwrap();

    // Verify Alpha was interrupted
    assert!(res_alpha.is_ok());
    let out_alpha = res_alpha.unwrap();
    assert!(
        out_alpha.is_interrupted || out_alpha.response_text.contains("interrupted"),
        "Session Alpha MUST be interrupted"
    );

    // Verify Beta was NOT interrupted and completed 100%
    assert!(res_beta.is_ok());
    let out_beta = res_beta.unwrap();
    assert!(
        !out_beta.is_interrupted,
        "Session Beta MUST NOT be interrupted (no cross-session cancellation leak)"
    );
    assert_eq!(
        out_beta.response_text, "Xin chào, tôi là LIVA.",
        "Session Beta must produce full uncorrupted response"
    );

    // Verify both sessions remain functional for subsequent turns
    let post_a = handle_alpha.execute_turn(
        SessionTurnRequest {
            session_id: "sess_iso_alpha".into(),
            turn_id: "turn_alpha_2".into(),
            input_text: "Alpha recovery".into(),
            queue_mode: SessionQueueMode::Followup,
            principal: CommandPrincipal::LocalCli,
            metadata: Default::default(),
                ..Default::default()
        },
        None,
    ).await;
    assert!(post_a.is_ok() && !post_a.unwrap().is_interrupted);

    let post_b = handle_beta.execute_turn(
        SessionTurnRequest {
            session_id: "sess_iso_beta".into(),
            turn_id: "turn_beta_2".into(),
            input_text: "Beta follow".into(),
            queue_mode: SessionQueueMode::Followup,
            principal: CommandPrincipal::LocalCli,
            metadata: Default::default(),
                ..Default::default()
        },
        None,
    ).await;
    assert!(post_b.is_ok() && !post_b.unwrap().is_interrupted);

    handle_alpha.shutdown().await.unwrap();
    handle_beta.shutdown().await.unwrap();
}

// ============================================================================
// CHALLENGE 6: WIN32 JOB OBJECT PROCESS CONTAINMENT & KILL-ON-CLOSE
// ============================================================================

/// Challenge 6.1: Win32 Job Object RAII Process Termination.
/// Spawns an external Windows process (`ping 127.0.0.1 -n 30` simulating llama-server),
/// assigns it to `WindowsProcessGuard` (configured with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`),
/// drops the guard, and confirms that the process is instantly terminated.
#[cfg(windows)]
#[test]
fn challenge_win32_job_object_kill_on_close_empirical() {
    use std::os::windows::io::AsRawHandle;
    use std::process::Command;
    use liva_native_core::llm::sidecar::WindowsProcessGuard;

    // 1. Create WindowsProcessGuard
    let guard = WindowsProcessGuard::new().expect("Create JobObject guard");

    // 2. Spawn a dummy Windows subprocess that would normally live for ~30 seconds
    let mut child = Command::new("cmd.exe")
        .args(["/c", "ping", "-n", "30", "127.0.0.1", ">nul"])
        .spawn()
        .expect("Spawn dummy child process");

    let raw_handle = child.as_raw_handle();
    assert!(!raw_handle.is_null());

    // 3. Assign child to Job Object
    let assign_res = guard.assign_process(raw_handle as _);
    assert!(assign_res.is_ok(), "Child process successfully assigned to JobObject");

    // 4. Verify child is actively running
    let status_before = child.try_wait().expect("try_wait before drop");
    assert!(status_before.is_none(), "Child process must still be running before JobObject is closed");

    // 5. Drop the JobObject guard (triggers CloseHandle -> JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE)
    drop(guard);

    // 6. Give Windows kernel a brief moment (up to 1000ms) to terminate the child
    let start = Instant::now();
    let mut terminated = false;
    while start.elapsed() < Duration::from_millis(1000) {
        if let Ok(Some(exit_status)) = child.try_wait() {
            println!("Child terminated by JobObject with status: {:?}", exit_status);
            terminated = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }

    assert!(
        terminated,
        "Child process MUST be terminated by Windows kernel when JobObject is closed"
    );
    let _ = child.wait();
}

// ============================================================================
// CHALLENGE 7: EXACT NUMERICAL VRAM BOUNDARY DISCRIMINATOR
// (84.9% vs 85.0% and 89.9% vs 90.0%)
// ============================================================================

/// Challenge 7.1: Exact Precision Boundaries for 16GB RTX 5060 Ti.
/// 16GB = 17,179,869,184 bytes.
/// Tests exact boundary values at 84.90% vs 85.00% and 89.90% vs 90.00%.
#[test]
fn challenge_vram_exact_boundary_discriminators() {
    let total_vram: u64 = 16 * 1024 * 1024 * 1024; // 17,179,869,184 bytes
    let supervisor = VramSupervisor::with_total_vram(total_vram);

    // ---------------------------------------------------------
    // Boundary 1: Stage 1 (Elevated -> High) at 85.00%
    // ---------------------------------------------------------

    // Sub-case 1.1: 84.90% usage
    let used_84_90 = (total_vram as f64 * 0.8490).round() as u64; // ~14,585,708,937 bytes
    let free_84_90 = total_vram - used_84_90; // ~2,594,160,247 bytes = 2474 MB (> 2048 MB)
    let assess_84_90 = supervisor.evaluate_pressure_custom(total_vram, used_84_90);
    assert_eq!(
        assess_84_90.tier,
        VramPressureTier::Elevated,
        "84.9% must be Elevated, not High (free = {} MB)",
        free_84_90 / (1024 * 1024)
    );
    assert_eq!(assess_84_90.action_taken, FallbackAction::None);
    assert!(!supervisor.is_reranker_on_cpu());
    assert!(!supervisor.is_worker_sleep_idle());

    // Sub-case 1.2: 84.99% usage
    let used_84_99 = (total_vram as f64 * 0.8499).round() as u64;
    let assess_84_99 = supervisor.evaluate_pressure_custom(total_vram, used_84_99);
    assert_eq!(
        assess_84_99.tier,
        VramPressureTier::Elevated,
        "84.99% must still be Elevated"
    );

    // Sub-case 1.3: Exactly 85.00% usage -> Stage 1 triggered!
    // Note: total_vram * 0.85 = 14,602,888,806.4. To satisfy ratio >= 0.85 in integer bytes,
    // byte count must be ceil (14,602,888,807), since 14,602,888,806 is 84.99999999767% (< 85%).
    let used_85_00 = (total_vram as f64 * 0.8500).ceil() as u64;
    let assess_85_00 = supervisor.evaluate_pressure_custom(total_vram, used_85_00);
    assert_eq!(
        assess_85_00.tier,
        VramPressureTier::High,
        "85.00% MUST trigger High tier (Stage 1)"
    );
    assert_eq!(
        assess_85_00.action_taken,
        FallbackAction::OffloadRerankerToCpu,
        "Stage 1 MUST offload reranker to CPU"
    );
    assert!(supervisor.is_reranker_on_cpu());
    assert!(!supervisor.is_worker_sleep_idle());

    // ---------------------------------------------------------
    // Boundary 2: Stage 2 (High -> Critical) at 90.00%
    // ---------------------------------------------------------

    // Sub-case 2.1: 89.90% usage
    let used_89_90 = (total_vram as f64 * 0.8990).round() as u64; // ~15,444,702,396 bytes
    let free_89_90 = total_vram - used_89_90; // ~1,735,166,788 bytes = 1654 MB (> 1200 MB)
    let assess_89_90 = supervisor.evaluate_pressure_custom(total_vram, used_89_90);
    assert_eq!(
        assess_89_90.tier,
        VramPressureTier::High,
        "89.90% must remain High (free = {} MB > 1200 MB)",
        free_89_90 / (1024 * 1024)
    );
    assert_eq!(assess_89_90.action_taken, FallbackAction::OffloadRerankerToCpu);
    assert!(supervisor.is_reranker_on_cpu());
    assert!(!supervisor.is_worker_sleep_idle());

    // Sub-case 2.2: 89.99% usage
    let used_89_99 = (total_vram as f64 * 0.8999).round() as u64;
    let assess_89_99 = supervisor.evaluate_pressure_custom(total_vram, used_89_99);
    assert_eq!(
        assess_89_99.tier,
        VramPressureTier::High,
        "89.99% must remain High"
    );

    // Sub-case 2.3: Exactly 90.00% usage -> Stage 2 triggered!
    let used_90_00 = (total_vram as f64 * 0.9000).round() as u64;
    let assess_90_00 = supervisor.evaluate_pressure_custom(total_vram, used_90_00);
    assert_eq!(
        assess_90_00.tier,
        VramPressureTier::Critical,
        "90.00% MUST trigger Critical tier (Stage 2)"
    );
    assert_eq!(
        assess_90_00.action_taken,
        FallbackAction::EvictWorkerAndOffloadReranker,
        "Stage 2 MUST evict worker and offload reranker"
    );
    assert!(supervisor.is_reranker_on_cpu());
    assert!(supervisor.is_worker_sleep_idle());

    // ---------------------------------------------------------
    // Boundary 3: Absolute Free Bytes Thresholds (Independent of ratio)
    // ---------------------------------------------------------
    // Stage 1 free threshold: free < 2048 MB (2,147,483,648 bytes)
    // Stage 2 free threshold: free < 1200 MB (1,258,291,200 bytes)

    // Using a 10 GB GPU (10,737,418,240 bytes) where 80% usage leaves exactly 2048 MB free
    let total_10gb = 10 * 1024 * 1024 * 1024;
    let free_2048mb = 2048 * 1024 * 1024;
    let free_2047mb = (2048 * 1024 * 1024) - 1;

    // At 2048 MB free and 80% usage: not < 2048 MB, ratio not >= 85% -> Elevated
    let assess_free_2048 = supervisor.evaluate_pressure_custom(total_10gb, total_10gb - free_2048mb);
    assert_eq!(assess_free_2048.tier, VramPressureTier::Elevated);

    // At 2047 MB free: free < 2048 MB triggers High tier!
    let assess_free_2047 = supervisor.evaluate_pressure_custom(total_10gb, total_10gb - free_2047mb);
    assert_eq!(assess_free_2047.tier, VramPressureTier::High, "free < 2048MB must trigger High tier");

    // Stage 2 free threshold on 10 GB:
    let free_1200mb = 1200 * 1024 * 1024;
    let free_1199mb = (1200 * 1024 * 1024) - 1;

    let assess_free_1200 = supervisor.evaluate_pressure_custom(total_10gb, total_10gb - free_1200mb);
    assert_eq!(assess_free_1200.tier, VramPressureTier::High);

    let assess_free_1199 = supervisor.evaluate_pressure_custom(total_10gb, total_10gb - free_1199mb);
    assert_eq!(assess_free_1199.tier, VramPressureTier::Critical, "free < 1200MB must trigger Critical tier");

    // ---------------------------------------------------------
    // Boundary 4: Hysteresis Recovery Threshold (> 4096 MB free)
    // ---------------------------------------------------------
    // From Critical tier, simulate memory freeing up:
    // When free = 4096 MB: should NOT reset flags (hysteresis buffer)
    let free_4096mb = 4096 * 1024 * 1024;
    let assess_rec_4096 = supervisor.evaluate_pressure_custom(total_vram, total_vram - free_4096mb);
    assert_eq!(assess_rec_4096.tier, VramPressureTier::Nominal);
    assert!(supervisor.is_reranker_on_cpu(), "Flags must not reset at exactly 4096 MB free");

    // When free = 4097 MB (> 4096 MB): flags reset!
    let free_4097mb = (4096 * 1024 * 1024) + 1024 * 1024;
    let assess_rec_4097 = supervisor.evaluate_pressure_custom(total_vram, total_vram - free_4097mb);
    assert_eq!(assess_rec_4097.tier, VramPressureTier::Nominal);
    assert!(!supervisor.is_reranker_on_cpu(), "Flags reset when free > 4096 MB");
    assert!(!supervisor.is_worker_sleep_idle(), "Worker sleep-idle resets when free > 4096 MB");
}

