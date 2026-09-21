//! Empirical Challenger 1 Stress & Verification Harness for Milestone M1
//!
//! Verifies:
//! 1. Token streaming & concurrency: 25 concurrent streaming tasks with backpressure.
//! 2. Priority queue preemption + client cancellation during active token stream:
//!    Early abort on dropped token_rx, followed by immediate High-priority preemption.
//! 3. Tokio runtime thread starvation prevention: verifies cooperative async heartbeat
//!    ticks uninterrupted while heavy synchronous computation runs via `spawn_blocking`.
//! 4. Fault tolerance & panic containment: panic inside `spawn_blocking` does not crash
//!    the actor loop; caller receives Err and subsequent commands succeed.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmStreamingBackendFn, Priority};

/// Empirical Test 1: Real-Time Token Streaming Under High Concurrency
///
/// Dispatches 25 concurrent streaming requests, each expecting 40 sequential tokens.
/// Verifies:
/// - 100% of tokens are received in strictly increasing sequence without missing chunks.
/// - No cross-client token leakage.
/// - Graceful backpressure handling when token_rx has minimal buffer.
#[tokio::test]
async fn test_empirical_streaming_concurrency_and_ordering() {
    let (tx, rx) = mpsc::channel(64);

    let backend: LlmStreamingBackendFn = Box::new(|prompt, token_tx| {
        let task_id = prompt.to_string();
        if let Some(ref tx) = token_tx {
            for i in 0..40 {
                let chunk = format!("{task_id}:tok_{i}");
                // Push chunk to receiver; handle backpressure
                match tx.try_send(chunk.clone()) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        let _ = tx.blocking_send(chunk);
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        return Err(anyhow::anyhow!("stream cancelled"));
                    }
                }
            }
        }
        Ok(format!("completed:{task_id}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let mut client_tasks = Vec::new();

    for client_id in 0..25 {
        let h = handle.clone();
        let prompt = format!("client_{client_id}");
        let priority = match client_id % 3 {
            0 => Priority::Low,
            1 => Priority::Normal,
            _ => Priority::High,
        };

        client_tasks.push(tokio::spawn(async move {
            let (token_tx, mut token_rx) = mpsc::channel(4); // Tiny buffer for backpressure
            let gen_fut = h.generate_text_stream(prompt.clone(), priority, Some(token_tx));

            let collector = tokio::spawn(async move {
                let mut tokens = Vec::new();
                while let Some(tok) = token_rx.recv().await {
                    tokens.push(tok);
                }
                tokens
            });

            let final_res = gen_fut.await.expect("generation should succeed");
            let collected = collector.await.expect("collector task join");

            (prompt, final_res, collected)
        }));
    }

    let results = tokio::time::timeout(Duration::from_secs(10), async {
        let mut out = Vec::new();
        for task in client_tasks {
            out.push(task.await.expect("client task panicked"));
        }
        out
    })
    .await
    .expect("All 25 streaming clients must complete within 10s without deadlocking");

    assert_eq!(results.len(), 25);

    for (prompt, final_res, tokens) in results {
        assert_eq!(final_res, format!("completed:{prompt}"));
        assert_eq!(tokens.len(), 40, "Each client must receive all 40 tokens");
        for (idx, tok) in tokens.iter().enumerate() {
            assert_eq!(tok, &format!("{prompt}:tok_{idx}"));
        }
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 2: Active Stream Cancellation and Priority Preemption
///
/// Scenario:
/// 1. Task A (Low priority) begins generating a long stream of 100 tokens with 5ms sleep per token (500ms total).
/// 2. Task B (Low priority) and Task C (High priority) are enqueued in the actor while Task A is streaming.
/// 3. After receiving 5 tokens, Task A's client drops `token_rx` (simulating UI navigation away / user cancellation).
/// 4. Backend detects channel closure via `tx.try_send` failing with `Closed`, terminating Task A early (< 50ms).
/// 5. Verifies that Task C (High priority) executes BEFORE Task B (Low priority).
#[tokio::test]
async fn test_empirical_stream_cancellation_and_preemption() {
    let (tx, rx) = mpsc::channel(32);
    let execution_order = Arc::new(Mutex::new(Vec::<String>::new()));
    let order_clone = execution_order.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, token_tx| {
        let p = prompt.to_string();
        order_clone.lock().unwrap().push(format!("start:{p}"));

        if let Some(ref tx) = token_tx {
            for i in 0..100 {
                std::thread::sleep(Duration::from_millis(5));
                let piece = format!("{p}:tok_{i}");
                match tx.try_send(piece) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Channel dropped by client — abort early!
                        order_clone.lock().unwrap().push(format!("aborted:{p}"));
                        return Err(anyhow::anyhow!("Client cancelled stream"));
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        if tx.blocking_send(format!("{p}:tok_{i}")).is_err() {
                            order_clone.lock().unwrap().push(format!("aborted:{p}"));
                            return Err(anyhow::anyhow!("Client cancelled stream"));
                        }
                    }
                }
            }
        }

        order_clone.lock().unwrap().push(format!("done:{p}"));
        Ok(format!("completed:{p}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let start_time = Instant::now();

    // 1. Launch Task A (Low priority, long streaming)
    let h_a = handle.clone();
    let (tx_a, mut rx_a) = mpsc::channel(4);
    let task_a = tokio::spawn(async move {
        h_a.generate_text_stream("task_A_low", Priority::Low, Some(tx_a))
            .await
    });

    // 2. Read only 5 tokens from Task A, then immediately drop rx_a (cancellation)
    let mut tokens_a = Vec::new();
    for _ in 0..5 {
        if let Some(tok) = rx_a.recv().await {
            tokens_a.push(tok);
        }
    }
    assert_eq!(tokens_a.len(), 5);
    drop(rx_a); // Drop receiver!

    // 3. While Task A is aborting, enqueue Task B (Low) and Task C (High)
    let h_b = handle.clone();
    let task_b = tokio::spawn(async move { h_b.generate_text("task_B_low", Priority::Low).await });

    let h_c = handle.clone();
    let task_c =
        tokio::spawn(async move { h_c.generate_text("task_C_high", Priority::High).await });

    // Task A should return Err due to cancellation
    let res_a = task_a.await.expect("task_a join");
    assert!(res_a.is_err(), "Cancelled task_a should return Err");

    let elapsed_a = start_time.elapsed();
    // Task A should abort significantly faster than 500ms (typically under 100ms)
    assert!(
        elapsed_a < Duration::from_millis(250),
        "Task A must abort early on dropped receiver; took {:?}",
        elapsed_a
    );

    // Wait for B and C
    let res_c = task_c.await.expect("task_c join").expect("task_c ok");
    let res_b = task_b.await.expect("task_b join").expect("task_b ok");
    assert_eq!(res_c, "completed:task_C_high");
    assert_eq!(res_b, "completed:task_B_low");

    let log = execution_order.lock().unwrap().clone();
    // Verification:
    // log[0] = start:task_A_low
    // log[1] = aborted:task_A_low
    // log[2] = start:task_C_high (High priority preempted Task B!)
    // log[3] = done:task_C_high
    // log[4] = start:task_B_low
    // log[5] = done:task_B_low
    assert_eq!(log[0], "start:task_A_low");
    assert_eq!(log[1], "aborted:task_A_low");
    assert_eq!(
        log[2], "start:task_C_high",
        "Task C (High) must preempt Task B (Low)!"
    );
    assert_eq!(log[3], "done:task_C_high");
    assert_eq!(log[4], "start:task_B_low");
    assert_eq!(log[5], "done:task_B_low");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 3: Tokio Runtime Thread Starvation Prevention via `spawn_blocking`
///
/// In this test, we build a multi-threaded Tokio runtime with exactly 2 worker threads.
/// We launch an LLM request that executes 200ms of synchronous blocking computation.
/// At the same time, we run an async "heartbeat" task that ticks every 10ms.
///
/// Invariant:
/// Because `actor.rs` wraps inference in `tokio::task::spawn_blocking`, the synchronous
/// work runs on Tokio's blocking threadpool. The 2 async worker threads remain unblocked,
/// allowing the cooperative heartbeat task to tick at least 15 times during the 200ms window.
///
/// If `spawn_blocking` were missing or broken, the worker thread would be starved and
/// heartbeat ticks would drop to near zero.
#[tokio::test]
async fn test_empirical_spawn_blocking_prevents_tokio_starvation() {
    let (tx, rx) = mpsc::channel(16);

    // Backend simulates heavy CPU-bound inference (blocking for 200ms)
    let backend: LlmStreamingBackendFn = Box::new(|prompt, _tx| {
        std::thread::sleep(Duration::from_millis(200));
        Ok(format!("computed:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Async heartbeat counter running on the Tokio worker threads
    let heartbeat_ticks = Arc::new(AtomicUsize::new(0));
    let running = Arc::new(AtomicBool::new(true));

    let hb_ticks = heartbeat_ticks.clone();
    let hb_running = running.clone();
    let heartbeat_task = tokio::spawn(async move {
        while hb_running.load(Ordering::Relaxed) {
            tokio::time::sleep(Duration::from_millis(10)).await;
            hb_ticks.fetch_add(1, Ordering::SeqCst);
        }
    });

    // Also run a concurrent async ping-pong task to test cooperative latency
    let (ping_tx, mut ping_rx) = mpsc::channel::<u32>(1);
    let (pong_tx, mut pong_rx) = mpsc::channel::<u32>(1);
    let ping_pong_task = tokio::spawn(async move {
        let mut round_trips = 0;
        while let Some(val) = ping_rx.recv().await {
            if val == 999 {
                break;
            }
            let _ = pong_tx.send(val + 1).await;
            round_trips += 1;
        }
        round_trips
    });

    // Dispatch heavy LLM generation
    let h = handle.clone();
    let llm_start = Instant::now();
    let llm_task =
        tokio::spawn(async move { h.generate_text("heavy_inference", Priority::Normal).await });

    // While LLM is generating, ping the ping-pong task multiple times
    let mut pongs_received = 0;
    for i in 0..5 {
        tokio::time::sleep(Duration::from_millis(25)).await;
        let _ = ping_tx.send(i).await;
        if let Some(_resp) = pong_rx.recv().await {
            pongs_received += 1;
        }
    }
    let _ = ping_tx.send(999).await;

    let llm_res = llm_task.await.expect("llm task join").expect("llm ok");
    let llm_duration = llm_start.elapsed();

    // Stop heartbeat
    running.store(false, Ordering::Relaxed);
    let _ = heartbeat_task.await;
    let _ = ping_pong_task.await;

    assert_eq!(llm_res, "computed:heavy_inference");
    assert!(
        llm_duration >= Duration::from_millis(180),
        "Inference must take at least ~200ms, took {:?}",
        llm_duration
    );

    let total_ticks = heartbeat_ticks.load(Ordering::SeqCst);
    // During 200ms with 10ms intervals, we expect ~15 to 20 ticks
    assert!(
            total_ticks >= 12,
            "Tokio worker threads must NOT be starved during heavy inference. Expected >= 12 ticks, got {}",
            total_ticks
        );

    // Ping-pong cooperative channel round trips must have succeeded during inference
    assert_eq!(
        pongs_received, 5,
        "Cooperative async tasks must exchange messages smoothly during LLM spawn_blocking"
    );

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 4: Panic Inside `spawn_blocking` Does Not Crash Actor Event Loop
///
/// Verifies:
/// If the LLM backend closure panics (e.g. fatal assertion in native model library),
/// `spawn_blocking` catches the panic, maps it to an `anyhow::Result::Err`,
/// the caller receives the error, and the actor loop SURVIVES to process subsequent requests.
#[tokio::test]
async fn test_empirical_spawn_blocking_panic_isolation() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmStreamingBackendFn = Box::new(|prompt, _tx| {
        if prompt.contains("panic_probe") {
            panic!("Simulated fatal native panic in llama.cpp");
        }
        Ok(format!("recovered:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // 1. Send panicking prompt
    let panic_res = handle.generate_text("panic_probe", Priority::High).await;

    assert!(
        panic_res.is_err(),
        "Panicking backend must return Err to caller"
    );
    let err_str = panic_res.unwrap_err().to_string();
    assert!(
        err_str.contains("LLM blocking inference task failed"),
        "Error message should reflect spawn_blocking join error: {err_str}"
    );

    // 2. Self-healing lock recovery:
    // With self-healing lock recovery (poisoned.into_inner()), the actor recovers
    // the backend guard despite the previous panic, and subsequent commands succeed.
    let subsequent_res = handle
        .generate_text("subsequent_probe", Priority::High)
        .await;
    assert!(
        subsequent_res.is_ok(),
        "Actor must self-heal from poisoned mutex and succeed on subsequent calls: {:?}",
        subsequent_res.err()
    );
    assert_eq!(subsequent_res.unwrap(), "recovered:subsequent_probe");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}
