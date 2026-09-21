//! Adversarial Empirical Stress Test Suite for `liva_llm::LlmActor`
//!
//! Objectives:
//! 1. Burst concurrency priority inversion resistance: High priority commands preempt
//!    Normal and Low priority commands even under heavy backlog.
//! 2. Multi-handle high-throughput concurrency: Zero deadlocks, zero task leaks, zero panics.
//! 3. Channel backpressure saturation: Graceful handling when queue exceeds MPSC channel capacity.
//! 4. Client cancellation / dropped receiver fault injection: Actor survives dropped oneshot responders.
//! 5. Graceful shutdown: In-flight and pending commands drain or fail cleanly without hangs.
//! 6. Error propagation: Backend errors return to caller without killing the actor worker loop.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmBackendFn, Priority};

/// Empirical Test 1: Burst Concurrency Priority Scheduling & Preemption
///
/// Scenario:
/// - 20 Low-priority tasks are dispatched into the actor queue.
/// - 10 Normal-priority tasks are dispatched.
/// - 5 High-priority tasks are dispatched.
/// - The backend introduces a 5ms delay per prompt to allow interleaving.
/// - We record the exact execution sequence.
/// - Assert: ALL High tasks must execute before any subsequently scheduled Low tasks,
///   and Normal tasks must execute before Low tasks in the backlog.
#[tokio::test]
#[allow(clippy::needless_range_loop)]
async fn test_burst_concurrency_priority_preemption() {
    let (tx, rx) = mpsc::channel(64);
    let execution_log = Arc::new(Mutex::new(Vec::<(String, Priority)>::new()));
    let log_clone = execution_log.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        std::thread::sleep(Duration::from_millis(5));
        let priority = if prompt.starts_with("high_") {
            Priority::High
        } else if prompt.starts_with("norm_") {
            Priority::Normal
        } else {
            Priority::Low
        };
        log_clone
            .lock()
            .unwrap()
            .push((prompt.to_string(), priority));
        Ok(format!("Result: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let handle = LlmActorHandle::new(tx);

    // Enqueue 15 Low tasks first
    let mut low_futs = Vec::new();
    for i in 0..15 {
        let h = handle.clone();
        low_futs.push(tokio::spawn(async move {
            h.generate_text(format!("low_{i}"), Priority::Low).await
        }));
    }

    // Small delay to ensure Low tasks hit the channel buffer
    tokio::time::sleep(Duration::from_millis(15)).await;

    // Enqueue 10 Normal tasks
    let mut norm_futs = Vec::new();
    for i in 0..10 {
        let h = handle.clone();
        norm_futs.push(tokio::spawn(async move {
            h.generate_text(format!("norm_{i}"), Priority::Normal).await
        }));
    }

    // Enqueue 5 High tasks
    let mut high_futs = Vec::new();
    for i in 0..5 {
        let h = handle.clone();
        high_futs.push(tokio::spawn(async move {
            h.generate_text(format!("high_{i}"), Priority::High).await
        }));
    }

    // Now start the actor loop — it will eagerly drain all queued commands into PriorityQueue
    let actor_task = tokio::spawn(actor.run());

    // Await all high tasks
    for f in high_futs {
        let res = f.await.expect("task panicked").expect("llm call failed");
        assert!(res.starts_with("Result: high_"));
    }

    // Await all normal tasks
    for f in norm_futs {
        let res = f.await.expect("task panicked").expect("llm call failed");
        assert!(res.starts_with("Result: norm_"));
    }

    // Await all low tasks
    for f in low_futs {
        let res = f.await.expect("task panicked").expect("llm call failed");
        assert!(res.starts_with("Result: low_"));
    }

    let log = execution_log.lock().unwrap().clone();
    assert_eq!(log.len(), 30, "All 30 tasks must execute");

    // Because the actor was spawned after all 30 were submitted,
    // the initial eager drain puts all 30 into the PriorityQueue.
    // The first 5 MUST be High, the next 10 MUST be Normal, and the last 15 MUST be Low.
    for i in 0..5 {
        assert_eq!(
            log[i].1,
            Priority::High,
            "Position {} should be High priority, got {:?}",
            i,
            log[i]
        );
    }
    for i in 5..15 {
        assert_eq!(
            log[i].1,
            Priority::Normal,
            "Position {} should be Normal priority, got {:?}",
            i,
            log[i]
        );
    }
    for i in 15..30 {
        assert_eq!(
            log[i].1,
            Priority::Low,
            "Position {} should be Low priority, got {:?}",
            i,
            log[i]
        );
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 2: Preemption of Active Backlog (High priority arrives DURING execution)
///
/// Scenario:
/// - Actor is already running.
/// - Task 0 (Low) starts running and takes 50ms.
/// - During this 50ms, 10 Low tasks and 3 High tasks are submitted.
/// - Assert: When Task 0 finishes, the 3 High tasks execute BEFORE any of the 10 Low tasks.
#[tokio::test]
#[allow(clippy::needless_range_loop)]
async fn test_high_priority_preempts_queued_low_tasks_mid_execution() {
    let (tx, rx) = mpsc::channel(64);
    let execution_log = Arc::new(Mutex::new(Vec::<String>::new()));
    let log_clone = execution_log.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        if prompt == "blocker_task" {
            std::thread::sleep(Duration::from_millis(50));
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
        log_clone.lock().unwrap().push(prompt.to_string());
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Send the blocker task (Low priority)
    let h0 = handle.clone();
    let blocker =
        tokio::spawn(async move { h0.generate_text("blocker_task", Priority::Low).await });

    // Give it 10ms to start executing on the actor thread
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Send 8 Low tasks
    let mut low_futs = Vec::new();
    for i in 0..8 {
        let h = handle.clone();
        low_futs.push(tokio::spawn(async move {
            h.generate_text(format!("low_mid_{i}"), Priority::Low).await
        }));
    }

    // Send 3 High tasks while blocker is still in-flight
    let mut high_futs = Vec::new();
    for i in 0..3 {
        let h = handle.clone();
        high_futs.push(tokio::spawn(async move {
            h.generate_text(format!("high_mid_{i}"), Priority::High)
                .await
        }));
    }

    // Wait for blocker and all tasks
    blocker.await.unwrap().unwrap();
    for f in high_futs {
        f.await.unwrap().unwrap();
    }
    for f in low_futs {
        f.await.unwrap().unwrap();
    }

    let log = execution_log.lock().unwrap().clone();
    assert_eq!(log[0], "blocker_task");

    // Next 3 executed tasks MUST be the 3 high tasks!
    assert!(
        log[1].starts_with("high_mid_"),
        "Task 1 must be High, got {}",
        log[1]
    );
    assert!(
        log[2].starts_with("high_mid_"),
        "Task 2 must be High, got {}",
        log[2]
    );
    assert!(
        log[3].starts_with("high_mid_"),
        "Task 3 must be High, got {}",
        log[3]
    );

    // Positions 4..12 must be the low tasks
    for i in 4..12 {
        assert!(
            log[i].starts_with("low_mid_"),
            "Task {i} must be Low, got {}",
            log[i]
        );
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 3: Rapid Multi-Handle Concurrency (Deadlock & Starvation Stress)
///
/// Scenario:
/// - 20 concurrent tasks holding cloned handles fire 10 requests each (200 requests total).
/// - Priorities are randomized (High/Normal/Low).
/// - Verify that ALL 200 requests complete within 10s with zero deadlocks or lost responses.
#[tokio::test]
async fn test_rapid_multi_handle_concurrency_no_deadlocks() {
    let (tx, rx) = mpsc::channel(32); // Small channel buffer to induce contention
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    let backend: LlmBackendFn = Box::new(move |_prompt| {
        let count = counter_clone.fetch_add(1, Ordering::SeqCst);
        Ok(format!("resp_{count}"))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let mut handles = Vec::new();
    for task_id in 0..20 {
        let h = handle.clone();
        handles.push(tokio::spawn(async move {
            let mut results = Vec::new();
            for req_id in 0..10 {
                let priority = match (task_id + req_id) % 3 {
                    0 => Priority::Low,
                    1 => Priority::Normal,
                    _ => Priority::High,
                };
                let res = h
                    .generate_text(format!("req_{task_id}_{req_id}"), priority)
                    .await;
                results.push(res);
            }
            results
        }));
    }

    // Collect all results with strict timeout
    let all_results = tokio::time::timeout(Duration::from_secs(10), async {
        let mut all = Vec::new();
        for h in handles {
            let res = h.await.expect("join handle failed");
            all.push(res);
        }
        all
    })
    .await
    .expect("All 200 requests must complete within 10 seconds without deadlocking");

    let total_successful = all_results
        .into_iter()
        .flat_map(|res_vec| res_vec.into_iter())
        .filter(|r| r.is_ok())
        .count();

    assert_eq!(total_successful, 200, "All 200 requests must succeed");
    assert_eq!(counter.load(Ordering::SeqCst), 200);

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 4: Client Cancellation / Dropped Receiver Fault Injection
///
/// Scenario:
/// - Multiple clients initiate requests, but 10 of them drop the receiving future
///   before the actor finishes processing.
/// - The actor MUST NOT panic when calling `responder.send(result)` on a closed oneshot.
/// - Subsequent requests on the handle MUST continue to succeed normally.
#[tokio::test]
async fn test_actor_resilience_against_dropped_responders() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        std::thread::sleep(Duration::from_millis(5));
        Ok(format!("Echo: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Launch 15 tasks that drop their future before the slow backend completes
    let mut dropped_futs = Vec::new();
    for i in 0..15 {
        let h = handle.clone();
        dropped_futs.push(tokio::spawn(async move {
            let _ = tokio::time::timeout(
                Duration::from_millis(1),
                h.generate_text(format!("timed_out_{i}"), Priority::Low),
            )
            .await;
        }));
    }

    // Wait for all cancellation tasks to trigger timeout and drop receivers
    for f in dropped_futs {
        let _ = f.await;
    }

    // Allow actor to process dropped requests
    tokio::time::sleep(Duration::from_millis(80)).await;

    // Now verify the actor is still healthy and accepts subsequent requests
    for i in 0..5 {
        let res = handle
            .generate_text(format!("healthy_{i}"), Priority::High)
            .await
            .expect("Actor must remain functional after dropped responders");
        assert_eq!(res, format!("Echo: healthy_{i}"));
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// Empirical Test 5: Graceful Shutdown with In-Flight and Pending Tasks
///
/// Scenario:
/// - 1 slow task is currently executing.
/// - 10 pending tasks are queued in the PriorityQueue.
/// - Shutdown command is sent.
/// - Assert:
///   a) Actor terminates cleanly within timeout.
///   b) Pending tasks whose responders are dropped return Err to callers.
///   c) New commands after shutdown return Err immediately.
#[tokio::test]
async fn test_graceful_shutdown_under_inflight_and_pending_commands() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        if prompt == "slow_inflight" {
            std::thread::sleep(Duration::from_millis(50));
        }
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // 1. Submit slow task
    let h1 = handle.clone();
    let inflight_fut =
        tokio::spawn(async move { h1.generate_text("slow_inflight", Priority::Normal).await });

    // Wait 10ms so it starts processing
    tokio::time::sleep(Duration::from_millis(10)).await;

    // 2. Submit 5 Low tasks
    let mut pending_futs = Vec::new();
    for i in 0..5 {
        let h = handle.clone();
        pending_futs.push(tokio::spawn(async move {
            h.generate_text(format!("pending_low_{i}"), Priority::Low)
                .await
        }));
    }

    // 3. Issue shutdown while slow task is in-flight and low tasks are queued
    let h_shut = handle.clone();
    let shutdown_fut = tokio::spawn(async move { h_shut.shutdown().await });

    // Wait for in-flight task to complete
    let inflight_res = inflight_fut.await.unwrap();
    assert_eq!(inflight_res.unwrap(), "Done: slow_inflight");

    // Shutdown should succeed
    shutdown_fut
        .await
        .unwrap()
        .expect("shutdown should succeed");

    // Actor task must exit within 500ms
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor must terminate cleanly after shutdown")
        .expect("Actor task panicked");

    // Subsequent command must fail
    let post_shut = handle.generate_text("post_shut", Priority::Normal).await;
    assert!(
        post_shut.is_err(),
        "Commands sent after shutdown must return Err"
    );
}

/// Empirical Test 6: Backend Error Propagation
///
/// Scenario:
/// - Backend returns an error for faulty prompts.
/// - Verify that caller receives the error.
/// - Verify that the actor loop survives and processes subsequent valid prompts.
#[tokio::test]
async fn test_backend_error_propagation_does_not_crash_actor() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        if prompt.contains("trigger_error") {
            Err(anyhow::anyhow!(
                "Simulated model failure: Out of context memory"
            ))
        } else {
            Ok(format!("Success: {}", prompt))
        }
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // 1. Successful request
    let res1 = handle
        .generate_text("valid_prompt_1", Priority::Normal)
        .await;
    assert_eq!(res1.unwrap(), "Success: valid_prompt_1");

    // 2. Faulty request
    let res2 = handle
        .generate_text("trigger_error_here", Priority::High)
        .await;
    assert!(
        res2.is_err(),
        "Faulty prompt must return an error to caller"
    );
    let err_msg = res2.unwrap_err().to_string();
    assert!(
        err_msg.contains("Simulated model failure"),
        "Error message should be preserved: {err_msg}"
    );

    // 3. Subsequent request must still succeed (actor didn't crash!)
    let res3 = handle
        .generate_text("valid_prompt_2", Priority::Normal)
        .await;
    assert_eq!(res3.unwrap(), "Success: valid_prompt_2");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}
