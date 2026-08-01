//! Chỉ biên dịch với `--features experimental` — các module này chưa nối dây
//! vào đường chạy nào (xem docs mục 3.2). Hai file sandbox/self_correction còn
//! spawn `cargo test` lồng nhau nên rất chậm; tách khỏi build mặc định giúp CI
//! nhanh lên đáng kể.
#![cfg(feature = "experimental")]

use liva_native_core::agent::dispatcher::{AgentDispatcher, AgentMessage, AgentRole, SwarmAgent};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{mpsc, oneshot};

#[tokio::test]
async fn test_swarm_stress_shared_dispatcher() {
    let dispatcher = AgentDispatcher::new();

    // Create channels for Orchestrator, Research, and Code agents
    // Set large buffer capacities to prevent channel-full blockages
    let (orch_tx, mut orch_rx) = mpsc::channel(1000);
    let (res_tx, res_rx) = mpsc::channel(1000);
    let (code_tx, code_rx) = mpsc::channel(1000);

    // Register with dispatcher
    dispatcher
        .register_agent(AgentRole::Orchestrator, orch_tx)
        .await;
    dispatcher.register_agent(AgentRole::Research, res_tx).await;
    dispatcher.register_agent(AgentRole::Code, code_tx).await;

    // Start agents
    let research_agent = SwarmAgent::new(AgentRole::Research, dispatcher.clone(), res_rx);
    let code_agent = SwarmAgent::new(AgentRole::Code, dispatcher.clone(), code_rx);

    let research_handle = research_agent.start();
    let code_handle = code_agent.start();

    // Coordinator for matching Orchestrator's incoming replies to oneshot senders
    let pending_replies = Arc::new(Mutex::new(
        HashMap::<String, oneshot::Sender<AgentMessage>>::new(),
    ));
    let pending_replies_clone = Arc::clone(&pending_replies);

    // Spawn a worker to read from orch_rx and forward to oneshot senders
    let coordinator_handle = tokio::spawn(async move {
        while let Some(msg) = orch_rx.recv().await {
            if let Some(ref corr_id) = msg.correlation_id {
                let mut pending = pending_replies_clone.lock().unwrap();
                if let Some(tx) = pending.remove(corr_id) {
                    let _ = tx.send(msg);
                }
            }
        }
    });

    let num_requests = 100;
    let mut join_handles = vec![];

    for i in 0..num_requests {
        let dispatcher_clone = dispatcher.clone();
        let pending_replies_clone = Arc::clone(&pending_replies);

        let handle = tokio::spawn(async move {
            let trace_id = format!("trace-shared-{}", i);
            let request_id = format!("req-shared-{}", i);
            let (tx, rx) = oneshot::channel();

            // Register oneshot channel before dispatching to prevent race condition
            {
                let mut pending = pending_replies_clone.lock().unwrap();
                pending.insert(request_id.clone(), tx);
            }

            let msg = AgentMessage {
                message_id: request_id.clone(),
                trace_id,
                from: AgentRole::Orchestrator,
                to: AgentRole::Research,
                content: format!("Please investigate how to implement a safe queue {}.", i),
                correlation_id: None,
            };

            dispatcher_clone.dispatch(msg).await.unwrap();

            // Wait for response with timeout
            let response = tokio::time::timeout(Duration::from_secs(10), rx)
                .await
                .expect("Shared dispatcher stress test timed out waiting for reply")
                .expect("Oneshot sender dropped");

            assert_eq!(response.correlation_id, Some(request_id));
            assert!(response.content.contains("Research results:"));
            assert!(response.content.contains("// Auto-generated Rust Code"));
        });

        join_handles.push(handle);
    }

    // Await all requests to complete successfully
    for handle in join_handles {
        handle.await.expect("Request worker task failed");
    }

    // Cleanup
    research_handle.abort();
    code_handle.abort();
    coordinator_handle.abort();
}

#[tokio::test]
async fn test_swarm_stress_multiple_independent_dispatchers() {
    let num_instances = 60;
    let mut join_handles = vec![];

    for i in 0..num_instances {
        let handle = tokio::spawn(async move {
            let dispatcher = AgentDispatcher::new();
            let (orch_tx, mut orch_rx) = mpsc::channel(100);
            let (res_tx, res_rx) = mpsc::channel(100);
            let (code_tx, code_rx) = mpsc::channel(100);

            dispatcher
                .register_agent(AgentRole::Orchestrator, orch_tx)
                .await;
            dispatcher.register_agent(AgentRole::Research, res_tx).await;
            dispatcher.register_agent(AgentRole::Code, code_tx).await;

            let research_agent = SwarmAgent::new(AgentRole::Research, dispatcher.clone(), res_rx);
            let code_agent = SwarmAgent::new(AgentRole::Code, dispatcher.clone(), code_rx);

            let research_handle = research_agent.start();
            let code_handle = code_agent.start();

            let trace_id = format!("trace-indep-{}", i);
            let request_id = format!("req-indep-{}", i);
            let msg = AgentMessage {
                message_id: request_id.clone(),
                trace_id,
                from: AgentRole::Orchestrator,
                to: AgentRole::Research,
                content: format!("Please investigate how to implement a safe queue {}.", i),
                correlation_id: None,
            };

            dispatcher.dispatch(msg).await.unwrap();

            // Wait for response with timeout
            let response = tokio::time::timeout(Duration::from_secs(10), orch_rx.recv())
                .await
                .expect("Independent dispatcher stress test timed out waiting for reply")
                .expect("No response received");

            assert_eq!(response.correlation_id, Some(request_id));
            assert!(response.content.contains("Research results:"));
            assert!(response.content.contains("// Auto-generated Rust Code"));

            research_handle.abort();
            code_handle.abort();
        });

        join_handles.push(handle);
    }

    // Await all requests to complete successfully
    for handle in join_handles {
        handle.await.expect("Independent dispatcher task failed");
    }
}
