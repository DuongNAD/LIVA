use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

use liva_llm::{LlmActor, LlmActorHandle, LlmBackendFn, LlmStreamingBackendFn, Priority};

#[tokio::test]
async fn test_llm_actor_queue_and_process() {
    let (tx, rx) = mpsc::channel(16);
    let actor = LlmActor::new(rx);
    tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let res = handle
        .generate_text("Hello LIVA".to_string(), Priority::Normal)
        .await
        .expect("should generate text");

    assert!(res.contains("Simulated response to: Hello LIVA"));
}

#[tokio::test]
async fn test_llm_actor_with_custom_backend() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmBackendFn = Box::new(|prompt| Ok(format!("Echo: {}", prompt)));

    let actor = LlmActor::new(rx).with_backend(backend);
    tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let res = handle
        .generate_text("Custom prompt", Priority::Normal)
        .await
        .expect("should execute custom backend");

    assert_eq!(res, "Echo: Custom prompt");
}

#[tokio::test]
async fn test_llm_actor_priority_preemption() {
    let (tx, rx) = mpsc::channel(16);
    let execution_order = Arc::new(Mutex::new(Vec::<String>::new()));
    let order_clone = execution_order.clone();

    let backend: LlmBackendFn = Box::new(move |prompt| {
        order_clone.lock().unwrap().push(prompt.to_string());
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_backend(backend);
    let handle = LlmActorHandle::new(tx);

    // Enqueue Low priority command first
    let h1 = handle.clone();
    let low_fut = tokio::spawn(async move { h1.generate_text("low_task", Priority::Low).await });

    // Enqueue High priority command second
    let h2 = handle.clone();
    let high_fut = tokio::spawn(async move { h2.generate_text("high_task", Priority::High).await });

    // Enqueue another Low priority command third
    let h3 = handle.clone();
    let low_fut_2 =
        tokio::spawn(async move { h3.generate_text("low_task_2", Priority::Low).await });

    // Small sleep to ensure all three commands are in the channel buffer before actor starts
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Start the actor — it must drain the channel and sort by priority before processing
    let actor_task = tokio::spawn(actor.run());

    let (r_low, r_high, r_low_2) = tokio::join!(low_fut, high_fut, low_fut_2);
    assert_eq!(r_low.unwrap().unwrap(), "Done: low_task");
    assert_eq!(r_high.unwrap().unwrap(), "Done: high_task");
    assert_eq!(r_low_2.unwrap().unwrap(), "Done: low_task_2");

    // Verify execution order: high_task MUST have executed first even though sent after low_task
    let recorded = execution_order.lock().unwrap().clone();
    assert_eq!(recorded, vec!["high_task", "low_task", "low_task_2"]);

    // Clean shutdown
    handle.shutdown().await.unwrap();
    let _ = actor_task.await;
}

#[tokio::test]
async fn test_llm_actor_clean_shutdown() {
    let (tx, rx) = mpsc::channel(16);
    let actor = LlmActor::new(rx);
    let actor_task = tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    // Initial command works
    let res = handle.generate_text("pre-shutdown", Priority::Normal).await;
    assert!(res.is_ok());

    // Clean shutdown
    handle.shutdown().await.expect("shutdown should succeed");

    // Wait for actor task to terminate
    tokio::time::timeout(Duration::from_secs(2), actor_task)
        .await
        .expect("actor should terminate cleanly within timeout")
        .expect("actor task should not panic");

    // Commands sent after shutdown must fail
    let after_res = handle
        .generate_text("post-shutdown", Priority::Normal)
        .await;
    assert!(after_res.is_err(), "command after shutdown must fail");
}

#[tokio::test]
async fn test_llm_actor_token_streaming_default() {
    let (tx, rx) = mpsc::channel(16);
    let actor = LlmActor::new(rx);
    tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let (token_tx, mut token_rx) = mpsc::channel(16);
    let gen_fut = handle.generate_text_stream("Stream test", Priority::Normal, Some(token_tx));

    let mut collected_tokens = Vec::new();
    let collector_fut = tokio::spawn(async move {
        while let Some(tok) = token_rx.recv().await {
            collected_tokens.push(tok);
        }
        collected_tokens
    });

    let res = gen_fut.await.expect("should generate text");
    let tokens = collector_fut.await.expect("collector finished");

    assert_eq!(res, "Simulated response to: Stream test");
    assert!(!tokens.is_empty(), "Tokens should be emitted incrementally");
}

#[tokio::test]
async fn test_llm_actor_with_custom_streaming_backend() {
    let (tx, rx) = mpsc::channel(16);
    let backend: LlmStreamingBackendFn = Box::new(|prompt, token_tx| {
        if let Some(ref tx) = token_tx {
            let _ = tx.try_send("Chunk 1: ".to_string());
            let _ = tx.try_send("Chunk 2: ".to_string());
            let _ = tx.try_send(prompt.to_string());
        }
        Ok(format!("Done: {}", prompt))
    });

    let actor = LlmActor::new(rx).with_streaming_backend(backend);
    tokio::spawn(actor.run());
    let handle = LlmActorHandle::new(tx);

    let (token_tx, mut token_rx) = mpsc::channel(16);
    let gen_fut = handle.generate_text_stream("hello", Priority::Normal, Some(token_tx));

    let mut tokens = Vec::new();
    let collector_fut = tokio::spawn(async move {
        while let Some(tok) = token_rx.recv().await {
            tokens.push(tok);
        }
        tokens
    });

    let res = gen_fut.await.expect("should execute streaming backend");
    let tokens = collector_fut.await.expect("collector finished");

    assert_eq!(res, "Done: hello");
    assert_eq!(tokens, vec!["Chunk 1: ", "Chunk 2: ", "hello"]);
}
