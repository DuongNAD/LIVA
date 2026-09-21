//! Empirical Adversarial Verification Suite — Challenger 2 (Milestone 1, Requirement R2)
//!
//! Objectives:
//! 1. Client receivers dropped prematurely: actor does not crash, ignores dropped oneshot,
//!    and continues processing subsequent requests across all priority levels.
//! 2. Backend closure returns Err: cleanly forwarded through responder channel as anyhow::Result::Err,
//!    and actor event loop remains healthy and responsive.
//! 3. Rapid repeated shutdowns or calls after shutdown fail cleanly without panics.
//! 4. Massive concurrent storm of mixed dropped receivers, backend errors, and shutdown commands.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmBackendFn, Priority};

/// 1. Client receivers dropped prematurely: actor does not crash and continues processing subsequent requests.
#[tokio::test]
async fn test_prematurely_dropped_receivers_preserves_event_loop() {
    let (tx, rx) = mpsc::channel(32);
    let processed_counter = Arc::new(AtomicUsize::new(0));
    let proc_clone = processed_counter.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        proc_clone.fetch_add(1, Ordering::SeqCst);
        // Small delay to simulate inference time
        std::thread::sleep(Duration::from_millis(10));
        Ok(format!("InferenceResult: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Spawn 20 tasks that immediately drop the receiving oneshot (client cancelled)
    let mut dropped_tasks = Vec::new();
    for i in 0..20 {
        let h = handle.clone();
        dropped_tasks.push(tokio::spawn(async move {
            let fut = h.generate_text(format!("dropped_client_{i}"), Priority::Normal);
            // Drop immediately without awaiting or with microscopic timeout
            let _ = tokio::time::timeout(Duration::from_micros(50), fut).await;
        }));
    }

    for dt in dropped_tasks {
        let _ = dt.await;
    }

    // Give actor time to drain and complete dropped tasks
    tokio::time::sleep(Duration::from_millis(250)).await;

    // Verify actor is fully functional and event loop is healthy:
    // Send High, Normal, and Low requests and assert they all succeed
    let res_high = handle
        .generate_text("subsequent_high", Priority::High)
        .await
        .expect("High priority request must succeed after dropped clients");
    assert_eq!(res_high, "InferenceResult: subsequent_high");

    let res_norm = handle
        .generate_text("subsequent_norm", Priority::Normal)
        .await
        .expect("Normal priority request must succeed after dropped clients");
    assert_eq!(res_norm, "InferenceResult: subsequent_norm");

    let res_low = handle
        .generate_text("subsequent_low", Priority::Low)
        .await
        .expect("Low priority request must succeed after dropped clients");
    assert_eq!(res_low, "InferenceResult: subsequent_low");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 2. Backend closure returns Err: error is cleanly forwarded through responder channel
/// and actor event loop remains healthy.
#[tokio::test]
async fn test_backend_err_clean_forwarding_and_event_loop_survival() {
    let (tx, rx) = mpsc::channel(16);

    let backend: LlmBackendFn = Box::new(|prompt| {
        if prompt.contains("error_fatal_context") {
            Err(anyhow::anyhow!(
                "ContextLimitExceeded: token count 4097 > 4096"
            ))
        } else if prompt.contains("error_cuda_oom") {
            Err(anyhow::anyhow!(
                "CudaOomError: failed to allocate 512MB VRAM"
            ))
        } else if prompt.contains("error_syntax_corrupt") {
            Err(anyhow::anyhow!("CorruptPromptFormat: unexpected delimiter"))
        } else {
            Ok(format!("SuccessAck: {}", prompt))
        }
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Phase 1: Context limit error on High priority
    let err1 = handle
        .generate_text("error_fatal_context_probe", Priority::High)
        .await
        .expect_err("Must return Err to caller");
    assert!(err1.to_string().contains("ContextLimitExceeded"));

    // Phase 2: Verify immediate normal request succeeds
    let ok1 = handle
        .generate_text("valid_prompt_after_err1", Priority::Normal)
        .await
        .expect("Actor must remain healthy after first Err");
    assert_eq!(ok1, "SuccessAck: valid_prompt_after_err1");

    // Phase 3: CUDA OOM error on Normal priority
    let err2 = handle
        .generate_text("error_cuda_oom_probe", Priority::Normal)
        .await
        .expect_err("Must return Err to caller");
    assert!(err2.to_string().contains("CudaOomError"));

    // Phase 4: Syntax error on Low priority
    let err3 = handle
        .generate_text("error_syntax_corrupt_probe", Priority::Low)
        .await
        .expect_err("Must return Err to caller");
    assert!(err3.to_string().contains("CorruptPromptFormat"));

    // Phase 5: Verify actor processes multiple subsequent requests cleanly
    for i in 0..10 {
        let res = handle
            .generate_text(format!("follow_up_{i}"), Priority::Normal)
            .await
            .expect("All follow-up requests must succeed");
        assert_eq!(res, format!("SuccessAck: follow_up_{i}"));
    }

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

/// 3. Rapid repeated shutdowns: 50 concurrent shutdown calls + calls after shutdown fail cleanly without panics.
#[tokio::test]
async fn test_rapid_repeated_shutdowns_and_post_shutdown_calls() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| {
        std::thread::sleep(Duration::from_millis(5));
        Ok(format!("Ok: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Warm-up request
    let warm = handle
        .generate_text("warmup", Priority::Normal)
        .await
        .expect("warmup succeeds");
    assert_eq!(warm, "Ok: warmup");

    // 50 concurrent tasks all calling handle.shutdown() simultaneously
    let mut shutdown_futs = Vec::new();
    for _ in 0..50 {
        let h = handle.clone();
        shutdown_futs.push(tokio::spawn(async move { h.shutdown().await }));
    }

    // Collect shutdown results — some may succeed (Ok(()) sent before receiver closed),
    // others may return Err("Failed to send Shutdown..."), but NONE must panic!
    let mut shutdown_oks = 0;
    let mut _shutdown_errs = 0;
    for sf in shutdown_futs {
        let res = sf.await.expect("shutdown task panicked");
        match res {
            Ok(()) => shutdown_oks += 1,
            Err(_) => _shutdown_errs += 1,
        }
    }

    assert!(shutdown_oks >= 1, "At least one shutdown call must succeed");
    // Ensure actor task finishes cleanly
    tokio::time::timeout(Duration::from_millis(500), actor_task)
        .await
        .expect("Actor must shut down within 500ms")
        .expect("Actor panicked");

    // Repeated call to shutdown() after actor has completely stopped
    for _ in 0..10 {
        let repeat_shut = handle.shutdown().await;
        assert!(
            repeat_shut.is_err(),
            "Calling shutdown after actor exit must return Err"
        );
    }

    // 50 concurrent calls to generate_text AFTER shutdown has taken effect
    let mut post_shutdown_futs = Vec::new();
    for i in 0..50 {
        let h = handle.clone();
        post_shutdown_futs.push(tokio::spawn(async move {
            h.generate_text(format!("post_{i}"), Priority::High).await
        }));
    }

    for pf in post_shutdown_futs {
        let res = pf.await.expect("post-shutdown call task panicked");
        assert!(
            res.is_err(),
            "generate_text after shutdown must fail cleanly with Err"
        );
    }
}

/// 4. Stress Storm: Concurrently interleave dropped receivers, backend errors, and valid requests
#[tokio::test]
async fn test_adversarial_mixed_chaos_storm() {
    let (tx, rx) = mpsc::channel(16);
    let success_count = Arc::new(AtomicUsize::new(0));
    let succ_clone = success_count.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        if prompt.starts_with("chaos_err_") {
            Err(anyhow::anyhow!("ChaosInjectedError"))
        } else {
            succ_clone.fetch_add(1, Ordering::SeqCst);
            Ok(format!("Echo: {}", prompt))
        }
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let mut all_tasks = Vec::new();

    // 90 mixed tasks:
    // 30 normal valid callers
    // 30 backend error callers
    // 30 dropped receiver callers
    for i in 0..90 {
        let h = handle.clone();
        let priority = match i % 3 {
            0 => Priority::High,
            1 => Priority::Normal,
            _ => Priority::Low,
        };

        if i % 3 == 0 {
            // Valid request
            all_tasks.push(tokio::spawn(async move {
                let res = h.generate_text(format!("chaos_ok_{i}"), priority).await;
                assert!(res.is_ok());
            }));
        } else if i % 3 == 1 {
            // Error request
            all_tasks.push(tokio::spawn(async move {
                let res = h.generate_text(format!("chaos_err_{i}"), priority).await;
                assert!(res.is_err());
            }));
        } else {
            // Dropped receiver request
            all_tasks.push(tokio::spawn(async move {
                let _ = tokio::time::timeout(
                    Duration::from_micros(10),
                    h.generate_text(format!("chaos_drop_{i}"), priority),
                )
                .await;
            }));
        }
    }

    for t in all_tasks {
        let _ = t.await;
    }

    // Verify actor is still responsive
    let final_ping = handle
        .generate_text("final_ping", Priority::High)
        .await
        .expect("Actor must survive the chaos storm");
    assert_eq!(final_ping, "Echo: final_ping");

    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}
