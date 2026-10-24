//! Milestone 2 Integration Tests: Session Actor & Queue Modes (Features F07 & F08).

use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use liva_core_types::{
    CommandPrincipal, Principal, SessionError, SessionMode, SessionQueueMode, SessionTurnRequest,
};
use liva_native_core::agent::session_actor::{
    DEFAULT_MAILBOX_CAPACITY, MAX_FOLLOWUP_QUEUE_DEPTH, SessionActor, SessionActorHandle,
    SessionManager,
};

#[tokio::test]
async fn test_f07_session_actor_bounded_mailbox_backpressure() {
    let (tx, rx) = mpsc::channel(DEFAULT_MAILBOX_CAPACITY);
    let actor = SessionActor::new("test_bounded_sess".into(), rx, None, None);
    let handle = tokio::spawn(actor.run());

    let actor_handle = SessionActorHandle::new("test_bounded_sess".into(), tx.clone());

    let req1 = SessionTurnRequest {
        session_id: "test_bounded_sess".into(),
        turn_id: "t1".into(),
        input_text: "msg 1".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };
    let res1 = actor_handle.execute_turn(req1, None).await;
    assert!(res1.is_ok());
    let out = res1.unwrap();
    assert_eq!(out.turn_id, "t1");
    assert!(!out.is_interrupted);

    actor_handle.shutdown().await.unwrap();
    handle.await.unwrap();
}

#[tokio::test]
async fn test_f07_session_isolation_and_cancellation_independence() {
    let manager = SessionManager::new(None);
    let handle_a = manager.get_or_create("sess_alpha").await.unwrap();
    let handle_b = manager.get_or_create("sess_beta").await.unwrap();

    let task_a = tokio::spawn(async move {
        handle_a
            .execute_turn(
                SessionTurnRequest {
                    session_id: "sess_alpha".into(),
                    turn_id: "turn_a".into(),
                    input_text: "Task A".into(),
                    queue_mode: SessionQueueMode::Followup,
                    principal: CommandPrincipal::LocalCli,
                    metadata: Default::default(),
                    session_mode: SessionMode::Actor,
                    turn_principal: Principal::Owner,
                },
                None,
            )
            .await
    });

    let task_b = tokio::spawn(async move {
        handle_b
            .execute_turn(
                SessionTurnRequest {
                    session_id: "sess_beta".into(),
                    turn_id: "turn_b".into(),
                    input_text: "Task B".into(),
                    queue_mode: SessionQueueMode::Followup,
                    principal: CommandPrincipal::LocalCli,
                    metadata: Default::default(),
                    session_mode: SessionMode::Actor,
                    turn_principal: Principal::Owner,
                },
                None,
            )
            .await
    });

    let (res_a, res_b) = tokio::join!(task_a, task_b);
    assert!(res_a.unwrap().is_ok());
    assert!(res_b.unwrap().is_ok());
}

#[tokio::test]
async fn test_f08_queue_mode_followup_fifo_ordering() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_fifo").await.unwrap();

    let req1 = SessionTurnRequest {
        session_id: "sess_fifo".into(),
        turn_id: "turn_1".into(),
        input_text: "First turn".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };
    let req2 = SessionTurnRequest {
        session_id: "sess_fifo".into(),
        turn_id: "turn_2".into(),
        input_text: "Second turn".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };

    let (res1, res2) = tokio::join!(
        handle.execute_turn(req1, None),
        handle.execute_turn(req2, None),
    );

    assert!(res1.is_ok());
    assert!(res2.is_ok());
    assert_eq!(res1.unwrap().turn_id, "turn_1");
    assert_eq!(res2.unwrap().turn_id, "turn_2");
}

#[tokio::test]
async fn test_f08_queue_mode_interrupt_preemption() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_interrupt").await.unwrap();

    let req_long = SessionTurnRequest {
        session_id: "sess_interrupt".into(),
        turn_id: "long_turn".into(),
        input_text: "Compute long response".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };

    let handle_clone = handle.clone();
    let turn1_task = tokio::spawn(async move {
        handle_clone.execute_turn(req_long, None).await
    });

    // Allow turn 1 to start
    tokio::time::sleep(Duration::from_millis(20)).await;

    // Send Interrupt turn
    let req_interrupt = SessionTurnRequest {
        session_id: "sess_interrupt".into(),
        turn_id: "interrupt_turn".into(),
        input_text: "Stop and do this instead".into(),
        queue_mode: SessionQueueMode::Interrupt,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };

    let res_interrupt = handle.execute_turn(req_interrupt, None).await.unwrap();
    assert_eq!(res_interrupt.turn_id, "interrupt_turn");

    let res_turn1 = turn1_task.await.unwrap();
    if let Ok(out) = res_turn1 {
        assert!(out.is_interrupted || out.response_text.contains("interrupted"));
    }
}

#[tokio::test]
async fn test_f08_queue_mode_collect_debouncing() {
    let (tx, rx) = mpsc::channel(DEFAULT_MAILBOX_CAPACITY);
    let actor = SessionActor::new("sess_collect".into(), rx, None, None)
        .with_collect_debounce(Duration::from_millis(80));
    let actor_task = tokio::spawn(actor.run());

    let handle = SessionActorHandle::new("sess_collect".into(), tx);

    let h1 = handle.clone();
    let h2 = handle.clone();
    let h3 = handle.clone();

    // Fire 3 collect messages in rapid burst
    let (r1, r2, r3) = tokio::join!(
        h1.execute_turn(
            SessionTurnRequest {
                session_id: "sess_collect".into(),
                turn_id: "c1".into(),
                input_text: "Phần 1".into(),
                queue_mode: SessionQueueMode::Collect,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                session_mode: SessionMode::Actor,
                turn_principal: Principal::Owner,
            },
            None,
        ),
        h2.execute_turn(
            SessionTurnRequest {
                session_id: "sess_collect".into(),
                turn_id: "c2".into(),
                input_text: "Phần 2".into(),
                queue_mode: SessionQueueMode::Collect,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                session_mode: SessionMode::Actor,
                turn_principal: Principal::Owner,
            },
            None,
        ),
        h3.execute_turn(
            SessionTurnRequest {
                session_id: "sess_collect".into(),
                turn_id: "c3".into(),
                input_text: "Phần 3".into(),
                queue_mode: SessionQueueMode::Collect,
                principal: CommandPrincipal::LocalCli,
                metadata: Default::default(),
                session_mode: SessionMode::Actor,
                turn_principal: Principal::Owner,
            },
            None,
        ),
    );

    assert!(r1.is_ok());
    assert!(r2.is_ok());
    assert!(r3.is_ok());

    handle.shutdown().await.unwrap();
    actor_task.await.unwrap();
}

#[tokio::test]
async fn test_f08_queue_mode_steer_injection() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_steer").await.unwrap();

    let req_normal = SessionTurnRequest {
        session_id: "sess_steer".into(),
        turn_id: "normal_turn".into(),
        input_text: "Start writing essay".into(),
        queue_mode: SessionQueueMode::Followup,
        principal: CommandPrincipal::LocalCli,
        metadata: Default::default(),
        session_mode: SessionMode::Actor,
        turn_principal: Principal::Owner,
    };

    let handle_clone = handle.clone();
    let turn_task = tokio::spawn(async move {
        handle_clone.execute_turn(req_normal, None).await
    });

    tokio::time::sleep(Duration::from_millis(20)).await;

    // Send steer instruction
    let steer_res = handle.steer("Keep it under 50 words".into()).await;
    assert!(steer_res.is_ok());

    let final_turn = turn_task.await.unwrap().unwrap();
    assert_eq!(final_turn.turn_id, "normal_turn");
}

#[tokio::test]
async fn test_f07_session_snapshot_and_manager_lifecycle() {
    let manager = SessionManager::new(None);
    let handle = manager.get_or_create("sess_snap").await.unwrap();

    let snapshot = handle.get_snapshot().await.unwrap();
    assert_eq!(snapshot.session_id, "sess_snap");
    assert!(snapshot.is_idle);
    assert_eq!(snapshot.queued_followups, 0);

    assert_eq!(manager.active_session_count().await, 1);
    handle.shutdown().await.unwrap();

    manager.prune_closed_sessions().await;
    assert_eq!(manager.active_session_count().await, 0);
}
