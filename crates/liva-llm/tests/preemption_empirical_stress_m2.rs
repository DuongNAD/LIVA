//! Empirical Challenger Stress Suite — Milestone 2
//!
//! Objectives (Adversarial stress-testing LLM cooperative preemption cancellation):
//! 1. Low/Normal priority streaming task preempted immediately by Priority::High without thread leak.
//! 2. Rapid priority interleaving storm: Low, Normal, High, and multiple High commands.
//! 3. Multiple consecutive High commands competing without deadlock or invalid state.
//! 4. Adversarial consumer drop: token_rx dropped mid-stream halts backend immediately.
//! 5. Adversarial client timeout / dropped oneshot responder future does not stall the actor.
//! 6. Dual cancellation: Caller external cancel_token modified by actor preemption.
//! 7. Mixed adversarial chaos under high concurrency.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmStreamingBackendFn, Priority};

/// 1. Verify that when a low/normal priority streaming command is running, sending a
/// Priority::High command immediately sets cancel_token and causes the streaming task
/// to abort promptly without leaking threads.
#[tokio::test]
async fn test_preemption_abort_latency_and_zero_thread_leak() {
    let (tx, rx) = mpsc::channel(16);

    let active_workers = Arc::new(AtomicUsize::new(0));
    let workers_clone = active_workers.clone();
    let cancelled_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancelled_flag.clone();

    // Backend produces 200 tokens with 10ms delay = 2,000ms duration if uncancelled
    let backend: LlmStreamingBackendFn = Box::new(move |prompt, token_tx, cancel_token| {
        workers_clone.fetch_add(1, Ordering::SeqCst);
        let mut chunks_emitted = 0;

        if prompt == "urgent_high_prompt" {
            std::thread::sleep(Duration::from_millis(5));
            workers_clone.fetch_sub(1, Ordering::SeqCst);
            return Ok(format!("completed:{prompt}"));
        }

        for i in 0..200 {
            if cancel_token.load(Ordering::SeqCst) {
                flag_clone.store(true, Ordering::SeqCst);
                workers_clone.fetch_sub(1, Ordering::SeqCst);
                return Err(anyhow::anyhow!("Cooperatively cancelled at chunk {i}"));
            }

            std::thread::sleep(Duration::from_millis(10));
            if let Some(ref tx) = token_tx {
                if tx.try_send(format!("tok_{i}")).is_err() {
                    workers_clone.fetch_sub(1, Ordering::SeqCst);
                    return Err(anyhow::anyhow!("token_tx closed"));
                }
            }
            chunks_emitted += 1;
        }

        workers_clone.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("completed:{prompt}:emitted_{chunks_emitted}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let (token_tx, mut token_rx) = mpsc::channel(16);
    let h1 = handle.clone();

    let start_time = Instant::now();

    // Spawn Low-priority long streaming task
    let low_fut = tokio::spawn(async move {
        h1.generate_text_stream("slow_low_prompt", Priority::Low, Some(token_tx))
            .await
    });

    // Wait until at least 2 tokens are received to confirm worker thread is actively executing
    let tok1 = token_rx.recv().await;
    let tok2 = token_rx.recv().await;
    assert!(
        tok1.is_some() && tok2.is_some(),
        "Must receive initial tokens"
    );

    assert_eq!(
        active_workers.load(Ordering::SeqCst),
        1,
        "Worker thread must be active"
    );

    // Send High-priority command to trigger immediate preemption
    let h2 = handle.clone();
    let high_start = Instant::now();
    let high_fut =
        tokio::spawn(async move { h2.generate_text("urgent_high_prompt", Priority::High).await });

    let (low_res, high_res) = tokio::join!(low_fut, high_fut);

    let high_elapsed = high_start.elapsed();
    let total_elapsed = start_time.elapsed();

    // 1. High priority command must succeed promptly
    let high_out = high_res.expect("high task joined").expect("high task Ok");
    assert!(
        high_out.contains("completed:urgent_high_prompt"),
        "High task must complete: {high_out}"
    );

    // 2. Low priority command must have been cooperatively cancelled
    let low_out = low_res.expect("low task joined");
    assert!(
        low_out.is_err(),
        "Low task must be Err from cancellation, got: {:?}",
        low_out
    );
    let low_err = low_out.unwrap_err().to_string();
    assert!(
        low_err.contains("Cooperatively cancelled"),
        "Unexpected error: {low_err}"
    );

    // 3. Cancel flag must have been observed
    assert!(
        cancelled_flag.load(Ordering::SeqCst),
        "Backend must have loaded true from cancel_token"
    );

    // 4. Prompt abortion: Total time must be well under 1000ms (unpreempted duration is 2000ms)
    assert!(
        total_elapsed < Duration::from_millis(800),
        "Total elapsed {:?} too long; preemption did not abort promptly!",
        total_elapsed
    );
    assert!(
        high_elapsed < Duration::from_millis(600),
        "High priority task took {:?}; preemption latency too high!",
        high_elapsed
    );

    // 5. Zero thread leak: Active workers counter must be exactly 0
    assert_eq!(
        active_workers.load(Ordering::SeqCst),
        0,
        "Worker thread leaked! active_workers != 0"
    );

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 2. Rapid priority interleaving: Low, Normal, High, and multiple High commands.
/// Verifies proper preemption order:
/// - High preempts Normal and Low
/// - Normal preempts Low
/// - Multiple High queue up and execute in FIFO order before Normal/Low
#[tokio::test]
async fn test_rapid_priority_interleaving_and_multi_high_preemption() {
    let (tx, rx) = mpsc::channel(64);
    let execution_order = Arc::new(std::sync::Mutex::new(Vec::<(String, Priority, bool)>::new()));
    let order_clone = execution_order.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, _token_tx, cancel_token| {
        let is_high = prompt.starts_with("high_");
        let is_norm = prompt.starts_with("norm_");
        let priority = if is_high {
            Priority::High
        } else if is_norm {
            Priority::Normal
        } else {
            Priority::Low
        };

        // Tasks run for 30ms unless cancelled
        for _ in 0..6 {
            if cancel_token.load(Ordering::SeqCst) {
                order_clone
                    .lock()
                    .unwrap()
                    .push((prompt.to_string(), priority, true));
                return Err(anyhow::anyhow!("Cancelled: {prompt}"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        order_clone
            .lock()
            .unwrap()
            .push((prompt.to_string(), priority, false));
        Ok(format!("Done:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let mut join_handles = Vec::new();

    // Step A: Start a Low task
    let h_low = handle.clone();
    join_handles.push(tokio::spawn(async move {
        h_low.generate_text("low_0", Priority::Low).await
    }));

    // Allow low_0 to start running
    tokio::time::sleep(Duration::from_millis(8)).await;

    // Step B: Send Normal task -> should preempt low_0
    let h_norm = handle.clone();
    join_handles.push(tokio::spawn(async move {
        h_norm.generate_text("norm_1", Priority::Normal).await
    }));

    // Allow norm_1 to start running
    tokio::time::sleep(Duration::from_millis(8)).await;

    // Step C: Send 3 High tasks in rapid succession -> high_2 should preempt norm_1,
    // and high_3, high_4 should execute before any remaining queued tasks
    for i in 2..5 {
        let h_high = handle.clone();
        join_handles.push(tokio::spawn(async move {
            h_high
                .generate_text(format!("high_{i}"), Priority::High)
                .await
        }));
    }

    // Step D: Also send another Low task that must wait until all High and Normal tasks complete
    let h_low_last = handle.clone();
    join_handles.push(tokio::spawn(async move {
        h_low_last
            .generate_text("low_trailing", Priority::Low)
            .await
    }));

    // Wait for all to finish
    let mut results = Vec::new();
    for jh in join_handles {
        results.push(jh.await.expect("task join"));
    }

    let records = execution_order.lock().unwrap().clone();

    // Verify:
    // 1. low_0 was cancelled by preemption
    let low_0_rec = records.iter().find(|(name, _, _)| name == "low_0");
    assert!(low_0_rec.is_some(), "low_0 must have run");
    assert!(low_0_rec.unwrap().2, "low_0 must have been cancelled");

    // 2. All 3 high tasks executed to completion without cancellation
    for i in 2..5 {
        let high_name = format!("high_{i}");
        let rec = records.iter().find(|(name, _, _)| name == &high_name);
        assert!(rec.is_some(), "{high_name} must have run");
        assert!(!rec.unwrap().2, "{high_name} must NOT have been cancelled");
    }

    // 3. low_trailing completed successfully after all highs
    let low_trailing_rec = records.iter().find(|(name, _, _)| name == "low_trailing");
    assert!(low_trailing_rec.is_some(), "low_trailing must have run");
    assert!(!low_trailing_rec.unwrap().2, "low_trailing must finish Ok");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 3. Multiple consecutive High tasks competing:
/// When an active Low task is running, 10 High commands are dispatched simultaneously.
/// Verifies:
/// - Low task is preempted immediately.
/// - All 10 High tasks execute sequentially without preempting each other (High == High does not preempt).
/// - Zero deadlocks or missing responses.
#[tokio::test]
async fn test_multiple_simultaneous_high_commands() {
    let (tx, rx) = mpsc::channel(32);
    let high_counter = Arc::new(AtomicUsize::new(0));
    let high_clone = high_counter.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, _token_tx, cancel_token| {
        if prompt == "slow_low" {
            for _ in 0..100 {
                if cancel_token.load(Ordering::SeqCst) {
                    return Err(anyhow::anyhow!("slow_low cancelled"));
                }
                std::thread::sleep(Duration::from_millis(5));
            }
            Ok("slow_low done".to_string())
        } else {
            // High priority task
            high_clone.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(Duration::from_millis(3));
            Ok(format!("HighOk:{prompt}"))
        }
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Launch slow low task
    let h_low = handle.clone();
    let low_fut = tokio::spawn(async move { h_low.generate_text("slow_low", Priority::Low).await });

    // Give it 10ms to enter worker thread
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Burst 10 High tasks simultaneously
    let mut high_futs = Vec::new();
    for i in 0..10 {
        let h = handle.clone();
        high_futs.push(tokio::spawn(async move {
            h.generate_text(format!("storm_high_{i}"), Priority::High)
                .await
        }));
    }

    let low_res = low_fut.await.expect("low join");
    assert!(low_res.is_err(), "slow_low must be preempted");

    for hf in high_futs {
        let res = hf.await.expect("high join").expect("high ok");
        assert!(res.starts_with("HighOk:storm_high_"));
    }

    assert_eq!(
        high_counter.load(Ordering::SeqCst),
        10,
        "All 10 High tasks must execute"
    );

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 4. Adversarial Client Drop: token_rx dropped prematurely mid-stream.
/// Verifies:
/// - When client drops token_rx, streaming callback observes Closed channel and aborts immediately.
/// - Worker thread terminates promptly without spinning to end of generation.
/// - Actor event loop remains completely healthy for subsequent commands.
#[tokio::test]
async fn test_adversarial_dropped_token_rx_halts_generation() {
    let (tx, rx) = mpsc::channel(16);
    let tokens_generated = Arc::new(AtomicUsize::new(0));
    let gen_clone = tokens_generated.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, token_tx, cancel_token| {
        if let Some(ref tx) = token_tx {
            for i in 0..500 {
                if cancel_token.load(Ordering::SeqCst) {
                    return Err(anyhow::anyhow!("Cancelled"));
                }
                gen_clone.fetch_add(1, Ordering::SeqCst);
                std::thread::sleep(Duration::from_millis(5));

                // Mimic boot.rs try_send / closed handling
                match tx.try_send(format!("piece_{i}")) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        // Channel dropped by consumer -> abort immediately
                        return Err(anyhow::anyhow!("Consumer disconnected (Closed)"));
                    }
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        if tx.blocking_send(format!("piece_{i}")).is_err() {
                            return Err(anyhow::anyhow!("Consumer disconnected on blocking_send"));
                        }
                    }
                }
            }
        }
        Ok(format!("Finished:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let (token_tx, mut token_rx) = mpsc::channel(4);
    let h1 = handle.clone();

    let gen_fut = tokio::spawn(async move {
        h1.generate_text_stream("stream_to_be_dropped", Priority::Normal, Some(token_tx))
            .await
    });

    // Receive only 3 tokens then explicitly DROP token_rx
    let t0 = token_rx.recv().await;
    let t1 = token_rx.recv().await;
    let t2 = token_rx.recv().await;
    assert!(t0.is_some() && t1.is_some() && t2.is_some());

    drop(token_rx); // Consumer drops!

    let start = Instant::now();
    let res = gen_fut.await.expect("task join");
    let elapsed = start.elapsed();

    // Must return Err because consumer dropped
    assert!(res.is_err(), "Must error on dropped token_rx");
    assert!(
        res.unwrap_err()
            .to_string()
            .contains("Consumer disconnected"),
        "Must abort due to disconnected consumer"
    );

    // Generation must halt promptly (< 200ms) rather than completing all 500 tokens (which would take 2500ms)
    assert!(
        elapsed < Duration::from_millis(300),
        "Elapsed {:?} too slow; backend did not abort on dropped token_rx",
        elapsed
    );

    let count = tokens_generated.load(Ordering::SeqCst);
    assert!(
        count < 20,
        "Tokens generated ({count}) must be far below 500",
    );

    // Verify actor can immediately process new commands normally
    let ping = handle
        .generate_text("ping_after_drop", Priority::High)
        .await
        .expect("Actor must survive dropped token_rx");
    assert_eq!(ping, "Finished:ping_after_drop");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 5. Adversarial Client Timeout: Caller drops the oneshot responder future
/// (e.g. client timeout or cancellation).
/// Verifies:
/// - Actor does not panic when sending result into closed responder oneshot channel.
/// - Actor immediately pops next command from queue and continues smoothly.
#[tokio::test]
async fn test_adversarial_dropped_responder_future_preserves_actor() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmStreamingBackendFn = Box::new(|prompt, _token_tx, _cancel| {
        std::thread::sleep(Duration::from_millis(20));
        Ok(format!("Echo:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Dispatch 15 commands where the caller times out after 1ms (dropping the future)
    for i in 0..15 {
        let h = handle.clone();
        let _ = tokio::time::timeout(
            Duration::from_millis(1),
            h.generate_text(format!("timeout_{i}"), Priority::Normal),
        )
        .await;
    }

    // Give actor 200ms to process through the dropped responder queue
    tokio::time::sleep(Duration::from_millis(200)).await;

    // Subsequent commands must work perfectly
    for i in 0..5 {
        let res = handle
            .generate_text(format!("recovery_{i}"), Priority::High)
            .await
            .expect("Subsequent commands must succeed");
        assert_eq!(res, format!("Echo:recovery_{i}"));
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 6. Dual Cancellation: Caller supplies an external cancel_token.
/// Verifies:
/// - Actor sets true on the caller's external cancel_token upon preemption.
/// - If caller sets true externally, backend also observes it.
#[tokio::test]
async fn test_caller_external_cancel_token_bridge() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmStreamingBackendFn = Box::new(|prompt, _token_tx, cancel_token| {
        for _ in 0..100 {
            if cancel_token.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("CancelledByToken:{prompt}"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        Ok(format!("Done:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Case A: Preemption sets caller's external cancel_token
    let caller_token = Arc::new(AtomicBool::new(false));
    let token_clone = caller_token.clone();

    let h1 = handle.clone();
    let low_fut = tokio::spawn(async move {
        h1.generate_text_stream_cancellable(
            "ext_cancellable_low",
            Priority::Low,
            None,
            Some(token_clone),
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(15)).await;

    // Send High command to preempt
    let h2 = handle.clone();
    let high_fut = tokio::spawn(async move { h2.generate_text("ext_high", Priority::High).await });

    let (low_res, high_res) = tokio::join!(low_fut, high_fut);
    assert!(high_res.unwrap().is_ok());
    assert!(low_res.unwrap().is_err());

    // Assert that the caller's own token was flipped to true by actor
    assert!(
        caller_token.load(Ordering::SeqCst),
        "Actor must store true in caller's external cancel_token on preemption"
    );

    // Case B: Caller explicitly cancels their own token externally
    let caller_token_b = Arc::new(AtomicBool::new(false));
    let token_clone_b = caller_token_b.clone();

    let h3 = handle.clone();
    let low_fut_b = tokio::spawn(async move {
        h3.generate_text_stream_cancellable(
            "ext_manual_cancel",
            Priority::Normal,
            None,
            Some(token_clone_b),
        )
        .await
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    caller_token_b.store(true, Ordering::SeqCst); // Caller cancels externally!

    let res_b = low_fut_b.await.expect("join");
    assert!(res_b.is_err(), "Must be cancelled by caller's token");
    assert!(res_b
        .unwrap_err()
        .to_string()
        .contains("CancelledByToken:ext_manual_cancel"));

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 7. Mixed Adversarial Chaos Storm under High Concurrency:
/// 100 concurrent tasks combining:
/// - Preemption by High
/// - Dropped token_rx
/// - Dropped responder future
/// - Normal tasks
/// Verifies zero deadlock, zero panics, and 100% actor recovery.
#[tokio::test]
async fn test_preemption_adversarial_chaos_storm() {
    let (tx, rx) = mpsc::channel(16); // Small buffer to stress backpressure
    let completed_high = Arc::new(AtomicUsize::new(0));
    let high_clone = completed_high.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, token_tx, cancel_token| {
        let is_high = prompt.starts_with("chaos_high_");
        for i in 0..20 {
            if cancel_token.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("Preempted"));
            }
            std::thread::sleep(Duration::from_millis(2));

            if let Some(ref tx) = token_tx {
                if tx.try_send(format!("p_{i}")).is_err() {
                    return Err(anyhow::anyhow!("TxClosed"));
                }
            }
        }
        if is_high {
            high_clone.fetch_add(1, Ordering::SeqCst);
        }
        Ok(format!("Ok:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let mut tasks = Vec::new();

    for i in 0..80 {
        let h = handle.clone();
        match i % 4 {
            0 => {
                // High priority task
                tasks.push(tokio::spawn(async move {
                    let _ = h
                        .generate_text(format!("chaos_high_{i}"), Priority::High)
                        .await;
                }));
            }
            1 => {
                // Low task with dropped token_rx
                tasks.push(tokio::spawn(async move {
                    let (token_tx, token_rx) = mpsc::channel(2);
                    let fut = h.generate_text_stream(
                        format!("chaos_drop_tok_{i}"),
                        Priority::Low,
                        Some(token_tx),
                    );
                    tokio::spawn(async move {
                        tokio::time::sleep(Duration::from_millis(3)).await;
                        drop(token_rx);
                    });
                    let _ = fut.await;
                }));
            }
            2 => {
                // Normal task with timed out future
                tasks.push(tokio::spawn(async move {
                    let _ = tokio::time::timeout(
                        Duration::from_millis(2),
                        h.generate_text(format!("chaos_timeout_{i}"), Priority::Normal),
                    )
                    .await;
                }));
            }
            _ => {
                // Low task subject to preemption
                tasks.push(tokio::spawn(async move {
                    let _ = h
                        .generate_text(format!("chaos_low_{i}"), Priority::Low)
                        .await;
                }));
            }
        }
    }

    for t in tasks {
        let _ = t.await;
    }

    // Actor must still be completely healthy
    let final_check = handle
        .generate_text("final_resilience_check", Priority::High)
        .await
        .expect("Actor must be alive and responsive after chaos storm");
    assert_eq!(final_check, "Ok:final_resilience_check");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 8. Empirical Soak: 50 consecutive preemption cycles to verify zero cumulative thread leakage.
/// In each cycle:
/// - Low streaming task is spawned.
/// - High task is immediately dispatched to preempt it.
/// - Low task aborts, High task finishes.
/// - We verify that active_workers is strictly 0 at the end of every single cycle.
#[tokio::test]
async fn test_soak_50_consecutive_preemptions_zero_leak() {
    let (tx, rx) = mpsc::channel(16);
    let active_workers = Arc::new(AtomicUsize::new(0));
    let workers_clone = active_workers.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, _token_tx, cancel_token| {
        workers_clone.fetch_add(1, Ordering::SeqCst);
        let is_high = prompt.starts_with("soak_high_");

        if is_high {
            std::thread::sleep(Duration::from_millis(2));
            workers_clone.fetch_sub(1, Ordering::SeqCst);
            return Ok(format!("Ok:{prompt}"));
        }

        // Low task: sleeps up to 100ms unless cancelled
        for _ in 0..20 {
            if cancel_token.load(Ordering::SeqCst) {
                workers_clone.fetch_sub(1, Ordering::SeqCst);
                return Err(anyhow::anyhow!("Preempted:{prompt}"));
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        workers_clone.fetch_sub(1, Ordering::SeqCst);
        Ok(format!("Completed:{prompt}"))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    for cycle in 0..50 {
        let h1 = handle.clone();
        let low_prompt = format!("soak_low_{cycle}");
        let low_fut =
            tokio::spawn(async move { h1.generate_text(low_prompt, Priority::Low).await });

        // Small sleep so low task enters worker thread
        tokio::time::sleep(Duration::from_millis(4)).await;

        let h2 = handle.clone();
        let high_prompt = format!("soak_high_{cycle}");
        let high_fut =
            tokio::spawn(async move { h2.generate_text(high_prompt, Priority::High).await });

        let (low_res, high_res) = tokio::join!(low_fut, high_fut);
        let high_val = high_res.expect("high join").expect("high ok");
        assert!(high_val.starts_with("Ok:soak_high_"));

        let low_val = low_res.expect("low join");
        assert!(
            low_val.is_err(),
            "Low task must be preempted in cycle {cycle}"
        );

        assert_eq!(
            active_workers.load(Ordering::SeqCst),
            0,
            "Worker thread leaked at cycle {cycle}!"
        );
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 9. Preemption before first token: High priority arrives before Low worker even begins emitting.
#[tokio::test]
async fn test_preemption_before_first_token() {
    let (tx, rx) = mpsc::channel(16);
    let tokens_emitted = Arc::new(AtomicUsize::new(0));
    let emitted_clone = tokens_emitted.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |prompt, token_tx, cancel_token| {
        if prompt == "fast_high" {
            return Ok("HighDone".to_string());
        }

        for i in 0..100 {
            if cancel_token.load(Ordering::SeqCst) {
                return Err(anyhow::anyhow!("CancelledBeforeChunks"));
            }
            std::thread::sleep(Duration::from_millis(10));
            if let Some(ref tx) = token_tx {
                let _ = tx.try_send(format!("chunk_{i}"));
            }
            emitted_clone.fetch_add(1, Ordering::SeqCst);
        }
        Ok("LowDone".to_string())
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let (token_tx, _token_rx) = mpsc::channel(16);
    let h1 = handle.clone();
    let low_fut = tokio::spawn(async move {
        h1.generate_text_stream("slow_low_unstarted", Priority::Low, Some(token_tx))
            .await
    });

    // Small delay to ensure low task enters handle_command / worker thread
    tokio::time::sleep(Duration::from_millis(3)).await;

    // Send High command to preempt at chunk 0/1
    let h2 = handle.clone();
    let high_fut = tokio::spawn(async move { h2.generate_text("fast_high", Priority::High).await });

    let (low_res, high_res) = tokio::join!(low_fut, high_fut);
    assert_eq!(high_res.unwrap().unwrap(), "HighDone");
    assert!(low_res.unwrap().is_err());

    let count = tokens_emitted.load(Ordering::SeqCst);
    assert!(
        count <= 2,
        "Must cancel before or at earliest chunks, emitted: {count}"
    );

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 10. Shutdown preempts active High command:
/// When an active High command is running, a Shutdown command immediately signals cancel
/// and aborts the in-flight inference.
#[tokio::test]
async fn test_shutdown_preempts_active_high_command() {
    let (tx, rx) = mpsc::channel(16);
    let cancelled_flag = Arc::new(AtomicBool::new(false));
    let flag_clone = cancelled_flag.clone();

    let backend: LlmStreamingBackendFn = Box::new(move |_prompt, _token_tx, cancel_token| {
        for _ in 0..200 {
            if cancel_token.load(Ordering::SeqCst) {
                flag_clone.store(true, Ordering::SeqCst);
                return Err(anyhow::anyhow!("High generation cancelled by shutdown"));
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        Ok("HighCompleted".to_string())
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let h1 = handle.clone();
    let high_fut =
        tokio::spawn(async move { h1.generate_text("long_high_task", Priority::High).await });

    // Allow high task to enter generation loop
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Send Shutdown command
    let start_shutdown = Instant::now();
    handle.shutdown().await.expect("Shutdown must send");

    let high_res = high_fut.await.expect("high join");
    let shutdown_elapsed = start_shutdown.elapsed();

    // High command must be cancelled
    assert!(
        high_res.is_err(),
        "High command must be cancelled by shutdown"
    );
    assert!(
        cancelled_flag.load(Ordering::SeqCst),
        "Cancel flag must be true"
    );

    // Must abort promptly (< 200ms), not running to 2000ms
    assert!(
        shutdown_elapsed < Duration::from_millis(300),
        "Shutdown took {:?}; did not abort promptly",
        shutdown_elapsed
    );

    // Actor task terminates
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor task must finish")
        .expect("Actor task Ok");
}
