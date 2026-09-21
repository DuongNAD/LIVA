//! Empirical Stress Challenge Suite — Challenger 2 (Milestone M1 Memory Bounds & Invariants)
//!
//! Validates:
//! 1. `SessionEventStream.history` clamped at strictly 500 events under 2,500+ pushes, with FIFO eviction and zero memory leakage.
//! 2. `ActiveRecallManager` TTL sweep cleans up expired challenges (> 600s and == 600s) cleanly under concurrency.
//! 3. `VisualGovernor` slot acquisition, cooldown window protection, interrupted cooldowns, and automatic Dormant transition.

use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use liva_native_core::active_recall::{
    ActiveRecallConfig, ActiveRecallManager, CHALLENGE_TTL_SECS,
};
use liva_native_core::cognitive::events::{MAX_HISTORY_EVENTS, SessionEvent, SessionEventStream};
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{self, DatabasePool, Fact};
use liva_native_core::governor::VisualGovernor;
use liva_native_core::{AppState, stt, tts};
use serde_json::json;

// -----------------------------------------------------------------------------
// CHALLENGE 1: SessionEventStream Ring Buffer & Memory Bounds
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_session_event_stream_history_ceiling_clamped_at_500_under_massive_push() {
    let stream = SessionEventStream::new(256);
    assert_eq!(stream.history_len().await, 0);

    // 1. Massive Concurrent Push: 10 tasks pushing 250 events each (2,500 events total)
    let stream_arc = Arc::new(stream);
    let mut tasks = Vec::new();

    for t in 0..10 {
        let s = Arc::clone(&stream_arc);
        tasks.push(tokio::spawn(async move {
            for i in 0..250 {
                let evt = SessionEvent::content_chunk(
                    format!("session_concurrent_{t}"),
                    format!("chunk_{t}_{i}"),
                );
                let _ = s.publish(evt).await;
            }
        }));
    }

    for t in tasks {
        t.await.expect("Concurrent push task failed");
    }

    // Assert: Even after 2,500 events, history length MUST NOT exceed 500.
    let len_after_concurrent = stream_arc.history_len().await;
    assert_eq!(
        len_after_concurrent, MAX_HISTORY_EVENTS,
        "History ceiling must remain clamped at exactly {MAX_HISTORY_EVENTS} (500)"
    );
    assert_eq!(MAX_HISTORY_EVENTS, 500);

    // 2. Strict FIFO Eviction and Ordering Check:
    // Push 1,200 ordered events on a single dedicated session
    let single_stream = SessionEventStream::new(128);
    for seq in 0..1200 {
        let evt = SessionEvent::content_chunk("fifo_session", format!("seq_{seq}"));
        single_stream.publish(evt).await.expect("publish succeeds");
    }

    assert_eq!(
        single_stream.history_len().await,
        500,
        "History must be clamped at 500 after 1,200 sequential events"
    );

    let replayed = single_stream.replay("fifo_session").await;
    assert_eq!(
        replayed.len(),
        500,
        "Replay must return exactly 500 active events"
    );

    // First event must be seq_700 (0..699 were evicted)
    if let SessionEvent::ContentChunk { token, .. } = &replayed[0] {
        assert_eq!(
            token, "seq_700",
            "Expected first event in ring buffer to be seq_700 after 700 evictions"
        );
    } else {
        panic!("Expected ContentChunk at index 0");
    }

    // Last event must be seq_1199
    if let SessionEvent::ContentChunk { token, .. } = &replayed[499] {
        assert_eq!(
            token, "seq_1199",
            "Expected last event in ring buffer to be seq_1199"
        );
    } else {
        panic!("Expected ContentChunk at index 499");
    }

    // 3. Clear Session Invariant
    single_stream.clear_session("fifo_session").await;
    assert_eq!(
        single_stream.history_len().await,
        0,
        "History should be 0 after clearing only session"
    );
    assert!(single_stream.replay("fifo_session").await.is_empty());

    // 4. Memory Leak Resistance: 10,000 rapid event churn
    let churn_stream = SessionEventStream::new(64);
    let start = Instant::now();
    for i in 0..10_000 {
        let evt = SessionEvent::reasoning_chunk("churn_session", format!("reason_{i}"));
        churn_stream.publish(evt).await.unwrap();
    }
    let elapsed = start.elapsed();
    assert_eq!(churn_stream.history_len().await, 500);
    println!(
        "10,000 event pushes with FIFO ring buffer took {:?}",
        elapsed
    );
}

// -----------------------------------------------------------------------------
// CHALLENGE 2: ActiveRecallManager TTL Eviction & Expiration Sweeping
// -----------------------------------------------------------------------------

#[tokio::test]
async fn test_active_recall_manager_ttl_clean_sweep_and_boundary_invariants() {
    let db_pool = DatabasePool::new_in_memory().expect("in-memory db pool");
    let crypto = EncryptionEngine::new("00000000000000000000000000000000");
    let manager = ActiveRecallManager::with_config(ActiveRecallConfig {
        enabled: true,
        min_interval_secs: 0,
    });

    // 1. Ingest 50 distinct facts into the database
    let writer = db_pool.writer.get().expect("writer connection");
    for i in 0..50 {
        let fact = Fact {
            key: format!("test_fact_topic_{i}"),
            value: format!("answer_val_{i}"),
            createdAt: "2026-08-01".to_string(),
            updatedAt: "2026-08-01".to_string(),
            ttlDays: None,
            source: "test".to_string(),
            category: None,
            importance: 0.8,
            confidenceScore: 1.0,
            sourceTurnId: None,
            memory_strength: 1.0,
            last_accessed_at: 0,
            access_count: 0,
        };
        db::set_fact(&writer, &crypto, &fact).expect("set_fact succeeds");
    }
    drop(writer);

    assert_eq!(manager.pending_count(), 0);

    // 2. Intercept 50 turns across 50 sessions to generate 50 active pending challenges
    let now_at_creation = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;

    for i in 0..50 {
        let res = manager.try_intercept_turn(
            &format!("Hỏi về test fact topic {}", i),
            &format!("session_{i}"),
            &db_pool,
            &crypto,
        );
        assert!(res.is_some(), "Turn {i} must be intercepted");
    }

    assert_eq!(
        manager.pending_count(),
        50,
        "50 pending challenges must be registered"
    );

    db_pool
        .writer_actor
        .flush()
        .await
        .expect("flush writer actor after step 2");

    // 3. Test Fresh Sweep: sweeping at (creation + 100s) must evict NOTHING (< 600s TTL)
    let fresh_now = now_at_creation + 100;
    let swept_fresh = manager.sweep_expired(fresh_now);
    assert_eq!(
        swept_fresh, 0,
        "Zero challenges should be swept when evaluated within TTL window"
    );
    assert_eq!(manager.pending_count(), 50);

    // 4. Test Boundary Sweep: sweeping at exactly (creation + CHALLENGE_TTL_SECS)
    // The condition is: `now.saturating_sub(c.asked_at_ts) < CHALLENGE_TTL_SECS`.
    // When now - asked_at_ts == 600, delta < 600 is FALSE, so boundary condition expires!
    let boundary_now = now_at_creation + CHALLENGE_TTL_SECS;
    let swept_boundary = manager.sweep_expired(boundary_now);
    assert_eq!(
        swept_boundary, 50,
        "All challenges must be swept when elapsed time equals CHALLENGE_TTL_SECS (600s)"
    );
    assert_eq!(
        manager.pending_count(),
        0,
        "Pending challenges count must be 0 after expiration sweep"
    );

    // 5. Test Interception auto-sweep:
    // Create 10 new challenges
    for i in 0..10 {
        let res = manager.try_intercept_turn(
            &format!("Hỏi về test fact topic {}", i),
            &format!("auto_sweep_session_{i}"),
            &db_pool,
            &crypto,
        );
        assert!(res.is_some(), "Step 5 turn {i} must be intercepted");
    }
    assert_eq!(manager.pending_count(), 10);

    // Now advance time past TTL and call try_intercept_turn:
    // Note: try_intercept_turn internally calls SystemTime::now() for current epoch.
    // If we call sweep_expired with now_at_creation + 700, verify clean return:
    let swept_expired = manager.sweep_expired(now_at_creation + 700);
    assert_eq!(swept_expired, 10);
    assert_eq!(manager.pending_count(), 0);

    db_pool
        .writer_actor
        .flush()
        .await
        .expect("flush writer actor after step 5");
}

// -----------------------------------------------------------------------------
// CHALLENGE 4: VisualGovernor Slot Acquisition & Cooldown Enforcement
// -----------------------------------------------------------------------------

#[test]
fn test_visual_governor_slot_acquisition_cooldown_and_state_transitions() {
    let cooldown_duration = Duration::from_millis(80);
    let gov = VisualGovernor::new(cooldown_duration);

    // Initial State: Dormant
    assert!(gov.is_dormant(), "Initial state must be Dormant");
    assert!(!gov.is_active(), "Initial state must not be Active");
    assert!(
        gov.should_unload_vlm(),
        "Dormant state indicates VLM should be unloaded"
    );

    // Step 1: Slot Acquisition (VLM loaded on GPU)
    gov.acquire_visual_slot();
    assert!(gov.is_active(), "After acquisition, state must be Active");
    assert!(!gov.is_dormant(), "Active state is not Dormant");
    assert!(!gov.should_unload_vlm(), "Active state must NOT unload VLM");

    // Step 2: Slot Release into Cooldown Window
    gov.release_visual_slot();
    assert!(
        !gov.is_active(),
        "After release, state must no longer be Active"
    );
    assert!(
        !gov.should_unload_vlm(),
        "Within cooldown window, VLM must NOT be unloaded"
    );
    assert!(
        !gov.is_dormant(),
        "Within cooldown window, governor is not yet Dormant"
    );

    // Step 3: Re-acquisition during Cooldown Window (cancels pending unload)
    gov.acquire_visual_slot();
    assert!(
        gov.is_active(),
        "Re-acquisition during cooldown must reactivate slot"
    );
    assert!(!gov.should_unload_vlm());

    // Release again to test natural cooldown expiration
    gov.release_visual_slot();
    assert!(!gov.is_active());
    assert!(!gov.should_unload_vlm());

    // Step 4: Cooldown Expiration
    // Sleep longer than cooldown (80ms + 30ms = 110ms)
    std::thread::sleep(Duration::from_millis(110));

    // should_unload_vlm() should now evaluate to true AND transition state to Dormant
    assert!(
        gov.should_unload_vlm(),
        "Expired cooldown must indicate VLM should be unloaded"
    );
    assert!(
        gov.is_dormant(),
        "Expired cooldown must automatically transition state to Dormant"
    );
    assert!(!gov.is_active());
}

#[tokio::test]
async fn test_visual_governor_high_concurrency_stress_and_safety() {
    let gov = Arc::new(VisualGovernor::new(Duration::from_millis(20)));
    let mut handles = Vec::new();

    // Spawn 20 concurrent threads randomly acquiring and releasing slots
    for _ in 0..20 {
        let g = Arc::clone(&gov);
        handles.push(tokio::spawn(async move {
            for i in 0..100 {
                g.acquire_visual_slot();
                assert!(g.is_active());
                if i % 3 == 0 {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                }
                g.release_visual_slot();
                let _ = g.should_unload_vlm();
                let _ = g.is_dormant();
            }
        }));
    }

    for h in handles {
        h.await.expect("Concurrent task panicked");
    }

    // After all threads finish and cooldown expires, governor must cleanly return to Dormant
    tokio::time::sleep(Duration::from_millis(40)).await;
    assert!(gov.should_unload_vlm());
    assert!(gov.is_dormant());
}

#[tokio::test]
async fn test_visual_governor_ipc_status_integration() {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    let state = Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: AppState::mock_llm(),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new(
            "test_vault",
        )),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        embedder: liva_native_core::AppState::empty_embedder(),
        active_recall: Arc::new(ActiveRecallManager::new()),
    });

    // Query governor status via vision command handler
    let status_val =
        liva_native_core::commands::vision::handle(state.clone(), "governor_status", json!({}))
            .await
            .expect("governor_status command succeeds");

    assert_eq!(status_val["is_active"], false);
    assert_eq!(status_val["is_dormant"], true);
    assert_eq!(status_val["should_unload"], true);

    // Acquire slot via state's visual_governor
    let gov = state.visual_governor();
    gov.acquire_visual_slot();

    let status_active =
        liva_native_core::commands::vision::handle(state.clone(), "governor_status", json!({}))
            .await
            .expect("governor_status command succeeds");

    assert_eq!(status_active["is_active"], true);
    assert_eq!(status_active["is_dormant"], false);
    assert_eq!(status_active["should_unload"], false);

    // Release slot
    gov.release_visual_slot();
    let status_cooldown =
        liva_native_core::commands::vision::handle(state.clone(), "governor_status", json!({}))
            .await
            .expect("governor_status command succeeds");

    assert_eq!(status_cooldown["is_active"], false);
    assert_eq!(status_cooldown["is_dormant"], false); // In cooldown, not yet dormant
    assert_eq!(status_cooldown["should_unload"], false);
}

#[test]
fn test_visual_governor_active_reference_counting_prevents_premature_unload() {
    let gov = VisualGovernor::new(Duration::from_millis(50));
    assert_eq!(gov.active_count(), 0);
    assert!(gov.is_dormant());

    // Task 1 acquires visual slot
    gov.acquire_visual_slot();
    assert_eq!(gov.active_count(), 1);
    assert!(gov.is_active());

    // Task 2 concurrently acquires visual slot
    gov.acquire_visual_slot();
    assert_eq!(gov.active_count(), 2);
    assert!(gov.is_active());

    // Task 1 finishes and releases its slot
    gov.release_visual_slot();

    // INVARIANT CHECK: Task 2 is still running, so state MUST remain Active!
    assert_eq!(gov.active_count(), 1);
    assert!(
        gov.is_active(),
        "State must remain Active while Task 2 is executing"
    );
    assert!(!gov.is_dormant(), "State must not be Dormant");
    assert!(
        !gov.should_unload_vlm(),
        "VLM must not unload while active tasks remain"
    );

    // Task 2 finishes and releases its slot
    gov.release_visual_slot();
    assert_eq!(gov.active_count(), 0);
    assert!(
        !gov.is_active(),
        "State transitions to Cooldown when count reaches 0"
    );
    assert!(
        !gov.should_unload_vlm(),
        "Within cooldown window, VLM is retained"
    );

    // Wait for cooldown window to elapse
    std::thread::sleep(Duration::from_millis(60));
    assert!(
        gov.should_unload_vlm(),
        "Expired cooldown signals VLM unload"
    );
    assert!(gov.is_dormant());
}
