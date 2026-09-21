//! Independent Adversarial Edge Case Suite by Challenger 2
//!
//! Verifies:
//! 1. Massive dropped oneshot responders during draining and execution.
//! 2. Rapid channel saturation with tiny channel buffer (backpressure and deadlock freedom).
//! 3. Backend error propagation: varied error types and sustained actor health.
//! 4. Actor termination lifecycle when all sender handles are dropped while commands are pending.
//! 5. Preemptive shutdown: Shutdown command (High priority) immediately stops the actor
//!    ahead of queued Normal/Low requests.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmBackendFn, Priority};

/// 1. Massive dropped oneshot responders during draining and execution
#[tokio::test]
async fn test_massive_dropped_responders_during_drain() {
    let (tx, rx) = mpsc::channel(32);
    let execution_count = Arc::new(AtomicUsize::new(0));
    let count_clone = execution_count.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        count_clone.fetch_add(1, Ordering::SeqCst);
        std::thread::sleep(Duration::from_millis(2));
        Ok(format!("Echo: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Spawn 50 tasks that drop their receiver future immediately
    let mut dropped_handles = Vec::new();
    for i in 0..50 {
        let h = handle.clone();
        let priority = match i % 3 {
            0 => Priority::Low,
            1 => Priority::Normal,
            _ => Priority::High,
        };
        dropped_handles.push(tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_micros(10),
                h.generate_text(format!("drop_{i}"), priority),
            )
            .await;
        }));
    }

    for dh in dropped_handles {
        let _ = dh.await;
    }

    // Give actor time to process the batch of dropped responders
    tokio::time::sleep(Duration::from_millis(150)).await;

    // Verify actor is fully functional for valid subsequent requests
    for i in 0..5 {
        let res = handle
            .generate_text(format!("valid_{i}"), Priority::High)
            .await
            .expect("Actor must remain alive and responsive after dropped responders");
        assert_eq!(res, format!("Echo: valid_{i}"));
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 2. Rapid channel saturation with tiny buffer (8 slots) under 30 concurrent callers
#[tokio::test]
async fn test_rapid_saturation_backpressure_and_bounded_memory() {
    let (tx, rx) = mpsc::channel(8); // Extremely tight channel capacity
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let backend: LlmBackendFn = Box::new(move |_prompt| {
        counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok("ack".to_string())
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // 30 concurrent workers sending 15 requests each (450 total requests)
    let mut workers = Vec::new();
    for w in 0..30 {
        let h = handle.clone();
        workers.push(tokio::spawn(async move {
            let mut results = Vec::new();
            for r in 0..15 {
                let priority = match (w + r) % 3 {
                    0 => Priority::Low,
                    1 => Priority::Normal,
                    _ => Priority::High,
                };
                let res = h.generate_text(format!("w{w}_r{r}"), priority).await;
                results.push(res);
            }
            results
        }));
    }

    let all_results = tokio::time::timeout(Duration::from_secs(10), async {
        let mut all = Vec::new();
        for w in workers {
            all.push(w.await.expect("worker task panicked"));
        }
        all
    })
    .await
    .expect("All 450 requests must complete within 10s without deadlock under heavy backpressure");

    let total_ok = all_results
        .into_iter()
        .flatten()
        .filter(|r| r.is_ok())
        .count();
    assert_eq!(
        total_ok, 450,
        "All 450 requests must succeed under backpressure"
    );
    assert_eq!(counter.load(Ordering::SeqCst), 450);

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 3. Backend error propagation: varied error types and sustained actor health
#[tokio::test]
async fn test_backend_error_propagation_preserves_actor_health() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmBackendFn = Box::new(|prompt| {
        if prompt.starts_with("fail_oom") {
            Err(anyhow::anyhow!("CUDA Out of Memory"))
        } else if prompt.starts_with("fail_context") {
            Err(anyhow::anyhow!("Context window exceeded 4096 tokens"))
        } else {
            Ok(format!("Generated: {}", prompt))
        }
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Alternating failure and success requests
    for i in 0..20 {
        if i % 3 == 0 {
            let err = handle
                .generate_text(format!("fail_oom_{i}"), Priority::High)
                .await
                .expect_err("Should fail with OOM");
            assert!(err.to_string().contains("CUDA Out of Memory"));
        } else if i % 3 == 1 {
            let err = handle
                .generate_text(format!("fail_context_{i}"), Priority::Normal)
                .await
                .expect_err("Should fail with context limit");
            assert!(err.to_string().contains("Context window exceeded"));
        } else {
            let ok = handle
                .generate_text(format!("prompt_{i}"), Priority::Low)
                .await
                .expect("Should succeed");
            assert_eq!(ok, format!("Generated: prompt_{i}"));
        }
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 4. Actor terminates cleanly when all sender handles are dropped while commands are pending
#[tokio::test]
async fn test_actor_termination_on_dropped_handles_with_pending_queue() {
    let (tx, rx) = mpsc::channel(32);
    let processed_count = Arc::new(AtomicUsize::new(0));
    let proc_clone = processed_count.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        if prompt == "blocker" {
            std::thread::sleep(Duration::from_millis(30));
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
        proc_clone.fetch_add(1, Ordering::SeqCst);
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Dispatch blocker
    let h0 = handle.clone();
    let blocker_fut = tokio::spawn(async move { h0.generate_text("blocker", Priority::Low).await });

    // Let blocker start
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Dispatch 5 queued tasks
    let mut queued_futs = Vec::new();
    for i in 0..5 {
        let h = handle.clone();
        queued_futs.push(tokio::spawn(async move {
            h.generate_text(format!("queued_{i}"), Priority::Normal)
                .await
        }));
    }

    // Now drop ALL handle clones (including original `handle`)
    drop(handle);

    // Wait for blocker and all queued tasks
    let blocker_res = blocker_fut.await.unwrap().unwrap();
    assert_eq!(blocker_res, "Done: blocker");

    for q in queued_futs {
        let res = q.await.unwrap().unwrap();
        assert!(res.starts_with("Done: queued_"));
    }

    // The actor task MUST terminate on its own because the channel is closed and queue is empty
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor task must exit cleanly when all senders are dropped and queue is empty")
        .expect("Actor task panicked");

    assert_eq!(
        processed_count.load(Ordering::SeqCst),
        6,
        "All 6 tasks (blocker + 5 queued) must have finished"
    );
}

/// 5. Shutdown preemption: High-priority Shutdown skips pending Normal and Low tasks
#[tokio::test]
async fn test_shutdown_priority_preemption_over_lower_tiers() {
    let (tx, rx) = mpsc::channel(32);
    let executed_tasks = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let exec_clone = executed_tasks.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        if prompt == "in_flight_blocker" {
            std::thread::sleep(Duration::from_millis(40));
        }
        exec_clone.lock().unwrap().push(prompt.to_string());
        Ok(format!("Result: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // 1. Submit blocker
    let h0 = handle.clone();
    let blocker_fut =
        tokio::spawn(async move { h0.generate_text("in_flight_blocker", Priority::Low).await });

    // Allow blocker to enter execution
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 2. Submit 8 Low tasks and 4 Normal tasks
    let mut low_futs = Vec::new();
    for i in 0..8 {
        let h = handle.clone();
        low_futs.push(tokio::spawn(async move {
            h.generate_text(format!("low_task_{i}"), Priority::Low)
                .await
        }));
    }
    let mut norm_futs = Vec::new();
    for i in 0..4 {
        let h = handle.clone();
        norm_futs.push(tokio::spawn(async move {
            h.generate_text(format!("norm_task_{i}"), Priority::Normal)
                .await
        }));
    }

    // 3. Dispatch Shutdown (Priority::High) while blocker is running
    handle.shutdown().await.expect("shutdown call succeeds");

    // Blocker completes
    blocker_fut.await.unwrap().unwrap();

    // Actor must terminate promptly
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor must terminate after shutdown")
        .expect("Actor panicked");

    // Check what actually executed
    let executed = executed_tasks.lock().unwrap().clone();
    assert_eq!(
        executed,
        vec!["in_flight_blocker"],
        "Because Shutdown has High priority, it must preempt all Normal and Low tasks! Executed was: {:?}",
        executed
    );

    // Callers of preempted tasks should receive an error (dropped oneshot responder)
    for lf in low_futs {
        let res = lf.await.unwrap();
        assert!(
            res.is_err(),
            "Preempted low task must return Err due to actor shutdown"
        );
    }
    for nf in norm_futs {
        let res = nf.await.unwrap();
        assert!(
            res.is_err(),
            "Preempted normal task must return Err due to actor shutdown"
        );
    }
}
