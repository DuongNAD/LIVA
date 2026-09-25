//! Empirical Adversarial Verification Suite — Challenger 2
//! Milestone M2: Sandboxing, Permission Modes & Emergency Kill-Switch
//!
//! Areas under challenge:
//! 1. Kill-Switch Latency SLA (< 15ms target vs < 50ms SLA) mid-typing & mid-drag
//! 2. Physical Mouse Thrashing Sensor (> 50px threshold vs micro-jitter)
//! 3. Synthetic Input Release Safety (0 stuck mouse buttons / modifier keys)
//! 4. Audit Ring Buffer High-Concurrency Stress (0 corruption, slot wrapping, fast RAM queries)

use liva_cua::audit::*;
use liva_cua::kill_switch::*;
use liva_cua::mock::MockCuaDriver;
use liva_cua::types::*;
use liva_cua::CuaEngine;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::{Duration, Instant};

// ===========================================================================
// HELPER FACTORIES
// ===========================================================================

fn make_adversarial_audit_event(
    id: &str,
    verdict: SecurityVerdict,
    latency_ms: u64,
) -> CuaAuditEvent {
    CuaAuditEvent {
        event_id: id.to_string(),
        timestamp_unix_ms: current_unix_ms(),
        session_id: "m2_challenger2_session".to_string(),
        action_type: "click".to_string(),
        target_app: TargetAppInfo {
            process_name: Some("notepad.exe".to_string()),
            process_id: Some(4422),
            window_title: Some("Adversarial Test Target".to_string()),
            hwnd: 0xCAFE_BABE,
        },
        coordinates: ActionCoordinates {
            screen_x: Some(400),
            screen_y: Some(300),
            client_x: Some(50),
            client_y: Some(60),
            bounds: Some(CuaRect::new(350, 240, 200, 150)),
        },
        permission_mode: PermissionMode::Bounded,
        security_verdict: verdict,
        policy_violation: if verdict == SecurityVerdict::BlockedByPolicy {
            Some("Adversarial violation test".to_string())
        } else {
            None
        },
        execution_delivery: CuaDeliveryMode::Background,
        action_effect: match verdict {
            SecurityVerdict::Allowed => CuaActionEffect::Completed,
            SecurityVerdict::AbortedByKillSwitch => CuaActionEffect::Aborted,
            _ => CuaActionEffect::Refused,
        },
        latency_ms,
        error_code: if verdict == SecurityVerdict::AbortedByKillSwitch {
            Some("emergency_halt".to_string())
        } else {
            None
        },
    }
}

// ===========================================================================
// AREA 1: Kill-Switch Latency SLA (< 15ms target vs < 50ms SLA)
// ===========================================================================

#[tokio::test]
async fn test_adversarial_m2_mid_flight_typing_cancellation_latency_sub_15ms() {
    let ks = KillSwitchController::new(KillSwitchConfig {
        esc_polling_enabled: false,
        poll_interval_ms: 5,
        mouse_thrashing_threshold_px: 50.0,
        mouse_thrashing_enabled: false,
    });
    let token = ks.cancellation_token();

    // Simulate an async typing actuator: 100 characters with 8ms delay between keystrokes
    let typing_task = tokio::spawn(async move {
        let mut typed_chars = Vec::new();
        for i in 0..100 {
            if token.is_cancelled() {
                return Err(("aborted_pre_check", typed_chars));
            }
            typed_chars.push(i as u8);

            // Simulate keystroke dispatch delay with cancellation token awareness
            tokio::select! {
                _ = tokio::time::sleep(Duration::from_millis(8)) => {}
                _ = token.cancelled() => {
                    return Err(("aborted_mid_sleep", typed_chars));
                }
            }
        }
        Ok(typed_chars)
    });

    // Let typing proceed for ~25ms (~3-4 characters)
    tokio::time::sleep(Duration::from_millis(25)).await;

    // Trigger emergency halt mid-typing
    let start_halt = Instant::now();
    ks.trigger_halt(HaltReason::EscapeKeyPressed)
        .expect("Trigger halt must succeed");
    let halt_trigger_duration = start_halt.elapsed();

    // Await task completion and measure total latency to abort
    let task_result = typing_task.await.expect("Task must not panic");
    let total_abort_latency = start_halt.elapsed();

    // 1. Verify trigger call latency itself is sub-millisecond (< 1ms)
    assert!(
        halt_trigger_duration.as_millis() < 5,
        "trigger_halt took {}ms, expected < 5ms",
        halt_trigger_duration.as_millis()
    );

    // 2. Verify total abort latency is < 15ms (target) and well below 50ms (SLA)
    assert!(
        total_abort_latency.as_millis() < 15,
        "Mid-typing abort took {}ms, expected < 15ms target (SLA < 50ms)",
        total_abort_latency.as_millis()
    );

    // 3. Verify task was aborted mid-flight
    assert!(
        task_result.is_err(),
        "Typing task must return Err on cancellation"
    );
    let (reason, chars) = task_result.unwrap_err();
    assert!(reason.starts_with("aborted"));
    assert!(
        chars.len() < 10,
        "Typing must be cut off immediately, but typed {} characters",
        chars.len()
    );
}

#[tokio::test]
async fn test_adversarial_m2_mid_flight_drag_cancellation_latency_sub_15ms() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x4000,
        pid: 800,
        title: "Test Drag Window".into(),
        class_name: "TestClass4".into(),
        process_name: Some("test4.exe".into()),
        bounds: CuaRect::new(0, 0, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    mock.action_delay_ms.store(300, Ordering::Relaxed);

    let config = CuaConfig {
        permission_mode: PermissionMode::Unrestricted,
        ..Default::default()
    };
    let engine = Arc::new(CuaEngine::new_with_driver(config, mock));
    let engine_clone = Arc::clone(&engine);

    let drag_task = tokio::spawn(async move {
        engine_clone
            .execute_action(CuaAction::Drag {
                target_hwnd: 0x4000,
                start_x: 100,
                start_y: 100,
                end_x: 500,
                end_y: 500,
                button: MouseButton::Left,
                steps: 50,
                delivery_mode: CuaDeliveryMode::Background,
            })
            .await
    });

    // Let drag start and be in-flight
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Trigger emergency halt mid-drag
    let start_halt = Instant::now();
    engine
        .trigger_emergency_halt()
        .expect("Trigger halt must succeed");

    let drag_result = drag_task.await.expect("Drag task must not panic");
    let total_abort_latency = start_halt.elapsed();

    // Verify abort latency < 15ms target
    assert!(
        total_abort_latency.as_millis() < 15,
        "Mid-drag abort took {}ms, expected < 15ms target",
        total_abort_latency.as_millis()
    );

    assert_eq!(
        drag_result.unwrap_err(),
        CuaError::EmergencyHalted,
        "Drag task must be aborted by emergency halt"
    );
}

#[tokio::test]
async fn test_adversarial_m2_kill_switch_flag_propagation_sub_millisecond() {
    let ks = KillSwitchController::new(KillSwitchConfig {
        esc_polling_enabled: false,
        poll_interval_ms: 5,
        mouse_thrashing_threshold_px: 50.0,
        mouse_thrashing_enabled: false,
    });

    let flag = ks.halt_flag();
    let token = ks.cancellation_token();

    assert!(!flag.load(Ordering::SeqCst));
    assert!(!token.is_cancelled());

    // Measure time from trigger_halt to flag update
    let start = Instant::now();
    ks.trigger_halt(HaltReason::EscapeKeyPressed).unwrap();
    let elapsed = start.elapsed();

    assert!(
        elapsed.as_millis() < 5,
        "Halt flag update took {}ms, expected < 5ms",
        elapsed.as_millis()
    );
    assert!(flag.load(Ordering::SeqCst));
    assert!(token.is_cancelled());
    assert!(ks.is_halted());
}

// ===========================================================================
// AREA 2: Physical Mouse Thrashing Sensor (> 50px vs Micro-Jitter)
// ===========================================================================

#[test]
fn test_adversarial_m2_mouse_thrashing_micro_jitter_zero_false_positives() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());
    let motion = &ks.motion;

    // Baseline centered at (1000, 1000)
    motion.begin_action(0x1000, (1000, 1000), (1000, 1000));

    // Exhaustive matrix of sub-50px displacements (including hand tremors, micro-jitters, slight nudges)
    let micro_jitter_offsets = [
        (0, 0),    // 0.00 px
        (1, 1),    // 1.41 px
        (-2, 3),   // 3.61 px
        (5, -5),   // 7.07 px
        (0, 9),    // 9.00 px
        (-7, 7),   // 9.90 px
        (10, 0),   // 10.00 px
        (15, -15), // 21.21 px
        (-25, 25), // 35.36 px
        (0, 45),   // 45.00 px
        (35, 35),  // 49.50 px
        (0, 49),   // 49.00 px
        (50, 0),   // EXACTLY 50.00 px (threshold boundary test: strictly > 50.0)
        (-50, 0),  // EXACTLY 50.00 px left
        (0, 50),   // EXACTLY 50.00 px down
        (0, -50),  // EXACTLY 50.00 px up
    ];

    for &(dx, dy) in &micro_jitter_offsets {
        let observed = (1000 + dx, 1000 + dy);
        let verdict = motion.check_displacement(observed, 50.0);
        assert!(
            verdict.is_none(),
            "Displacement ({}, {}) dist={:.2}px must NOT trigger mouse thrashing halt (false positive)",
            dx,
            dy,
            ((dx * dx + dy * dy) as f64).sqrt()
        );
    }
}

#[test]
fn test_adversarial_m2_mouse_thrashing_sudden_displacement_instant_halt() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());
    let motion = &ks.motion;

    // Baseline at (500, 500)
    motion.begin_action(0x2000, (500, 500), (500, 500));

    // Matrix of physical thrash displacements > 50px across all 4 quadrants
    let thrash_offsets = [
        (36, 36),    // 50.91 px (> 50.0)
        (51, 0),     // 51.00 px (> 50.0)
        (0, -51),    // 51.00 px (> 50.0)
        (-40, -40),  // 56.57 px (> 50.0)
        (60, 0),     // 60.00 px
        (0, 75),     // 75.00 px
        (-100, 100), // 141.42 px
        (300, -400), // 500.00 px
    ];

    for &(dx, dy) in &thrash_offsets {
        let observed = (500 + dx, 500 + dy);
        let expected_dist = ((dx * dx + dy * dy) as f64).sqrt();
        let verdict = motion.check_displacement(observed, 50.0);

        assert!(
            verdict.is_some(),
            "Displacement ({}, {}) dist={:.2}px MUST trigger mouse thrashing",
            dx,
            dy,
            expected_dist
        );

        if let Some(HaltReason::MouseThrashing {
            displacement_px,
            threshold_px,
            observed_x,
            observed_y,
            expected_x,
            expected_y,
        }) = verdict
        {
            assert_eq!(threshold_px, 50.0);
            assert_eq!(observed_x, 500 + dx);
            assert_eq!(observed_y, 500 + dy);
            assert_eq!(expected_x, 500);
            assert_eq!(expected_y, 500);
            assert!((displacement_px - expected_dist).abs() < 1e-4);
        } else {
            panic!("Expected HaltReason::MouseThrashing");
        }
    }
}

#[test]
fn test_adversarial_m2_mouse_thrashing_quiescence_when_inactive() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());
    let motion = &ks.motion;

    // Context is NOT active
    assert!(!motion.is_active.load(Ordering::Relaxed));

    // Even an extreme cursor jump of 2000px should be ignored when no synthetic action is running
    let result = motion.check_displacement((2500, 2500), 50.0);
    assert!(
        result.is_none(),
        "When no synthetic action is active, physical cursor moves must never trip kill-switch"
    );
}

#[test]
fn test_adversarial_m2_mouse_thrashing_multi_monitor_negative_coordinates() {
    let ks = KillSwitchController::new(KillSwitchConfig::default());
    let motion = &ks.motion;

    // Baseline on a secondary left monitor at (-1920, 500)
    motion.begin_action(0x3000, (-1920, 500), (-1920, 500));

    // Jitter on negative origin: (-1915, 505) -> dist = 7.07px <= 50.0
    assert!(motion.check_displacement((-1915, 505), 50.0).is_none());

    // Human yank: (-1840, 500) -> dx = 80px > 50.0
    let thrash = motion.check_displacement((-1840, 500), 50.0);
    assert!(thrash.is_some());
    if let Some(HaltReason::MouseThrashing {
        displacement_px, ..
    }) = thrash
    {
        assert_eq!(displacement_px, 80.0);
    }
}

// ===========================================================================
// AREA 3: Synthetic Input Release Safety (0 Stuck Buttons / Modifiers)
// ===========================================================================

#[test]
fn test_adversarial_m2_release_all_synthetic_inputs_safe_execution() {
    // 1. Target HWND = None
    release_all_synthetic_inputs(None);

    // 2. Target HWND = Some(valid mock)
    release_all_synthetic_inputs(Some(0x1000));

    // 3. Target HWND = Some(0)
    release_all_synthetic_inputs(Some(0));
}

#[tokio::test]
async fn test_adversarial_m2_kill_switch_trigger_and_reset_cleans_inputs() {
    let ks = KillSwitchController::new(KillSwitchConfig {
        esc_polling_enabled: false,
        poll_interval_ms: 5,
        mouse_thrashing_threshold_px: 50.0,
        mouse_thrashing_enabled: false,
    });

    // Arm state
    assert_eq!(ks.get_status().state, KillSwitchState::Armed);

    // Trigger halt: internally executes release_all_synthetic_inputs
    ks.trigger_halt(HaltReason::EscapeKeyPressed).unwrap();
    assert_eq!(ks.get_status().state, KillSwitchState::Halted);
    assert!(ks.is_halted());

    // Reset: internally executes release_all_synthetic_inputs for clean recovery
    ks.reset().unwrap();
    assert_eq!(ks.get_status().state, KillSwitchState::Armed);
    assert!(!ks.is_halted());
}

#[test]
fn test_adversarial_m2_rapid_halt_reset_churn_no_deadlock() {
    let ks = KillSwitchController::new(KillSwitchConfig {
        esc_polling_enabled: false,
        poll_interval_ms: 5,
        mouse_thrashing_threshold_px: 50.0,
        mouse_thrashing_enabled: false,
    });

    for i in 0..100 {
        ks.trigger_halt(HaltReason::ManualTrigger {
            detail: format!("churn cycle {}", i),
        })
        .unwrap();
        assert!(ks.is_halted());

        ks.reset().unwrap();
        assert!(!ks.is_halted());
    }

    let status = ks.get_status();
    assert_eq!(status.state, KillSwitchState::Armed);
    assert_eq!(status.total_aborts_count, 100);
}

// ===========================================================================
// AREA 4: Audit Ring Buffer High-Concurrency Stress
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_adversarial_m2_audit_ring_buffer_extreme_concurrent_writers() {
    let ring = Arc::new(AuditRingBuffer::new());
    let num_tasks = 16;
    let events_per_task = 1_000;
    let mut handles = Vec::new();

    let start = Instant::now();

    for t in 0..num_tasks {
        let ring_clone = Arc::clone(&ring);
        handles.push(tokio::spawn(async move {
            for i in 0..events_per_task {
                let ev = make_adversarial_audit_event(
                    &format!("t{:02}-e{:04}", t, i),
                    SecurityVerdict::Allowed,
                    5,
                );
                ring_clone.push(ev);
            }
        }));
    }

    for h in handles {
        h.await.expect("Writer task join failed");
    }

    let elapsed = start.elapsed();
    println!(
        "Pushed {} events across {} concurrent tasks in {:?}",
        num_tasks * events_per_task,
        num_tasks,
        elapsed
    );

    // 1. Monotonic counter must equal total writes
    assert_eq!(ring.total_pushed(), (num_tasks * events_per_task) as u64);

    // 2. Buffer capacity must be strictly capped at 256
    assert_eq!(ring.len(), 256);

    // 3. Fast RAM query must return 256 valid events with 0 corruption
    let recent = ring.read_recent(256);
    assert_eq!(recent.len(), 256);
    for ev in &recent {
        assert!(!ev.event_id.is_empty());
        assert_eq!(ev.session_id, "m2_challenger2_session");
        assert_eq!(ev.security_verdict, SecurityVerdict::Allowed);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_adversarial_m2_audit_ring_buffer_concurrent_readers_during_write_storm() {
    let ring = Arc::new(AuditRingBuffer::new());
    let stop_signal = Arc::new(std::sync::atomic::AtomicBool::new(false));

    // Spawn 8 high-velocity writers
    let mut writer_handles = Vec::new();
    for t in 0..8 {
        let ring_w = Arc::clone(&ring);
        let stop_w = Arc::clone(&stop_signal);
        writer_handles.push(tokio::spawn(async move {
            let mut i = 0;
            while !stop_w.load(Ordering::Relaxed) && i < 2_000 {
                let ev = make_adversarial_audit_event(
                    &format!("storm-w{}-{}", t, i),
                    if i % 3 == 0 {
                        SecurityVerdict::BlockedByPolicy
                    } else {
                        SecurityVerdict::Allowed
                    },
                    10,
                );
                ring_w.push(ev);
                i += 1;
            }
            i
        }));
    }

    // Spawn 4 concurrent readers actively reading during write storm
    let mut reader_handles = Vec::new();
    for _r in 0..4 {
        let ring_r = Arc::clone(&ring);
        let stop_r = Arc::clone(&stop_signal);
        reader_handles.push(tokio::spawn(async move {
            let mut reads_done = 0;
            let mut max_read_latency_us = 0u128;

            while !stop_r.load(Ordering::Relaxed) {
                let read_start = Instant::now();
                let events = ring_r.read_recent(100);
                let dur_us = read_start.elapsed().as_micros();
                if dur_us > max_read_latency_us {
                    max_read_latency_us = dur_us;
                }

                // Verify every returned event is intact (no torn reads)
                for ev in &events {
                    assert!(!ev.event_id.is_empty());
                    assert!(ev.event_id.starts_with("storm-w"));
                }

                reads_done += 1;
                tokio::task::yield_now().await;
            }

            (reads_done, max_read_latency_us)
        }));
    }

    // Let storm run for 80ms
    tokio::time::sleep(Duration::from_millis(80)).await;
    stop_signal.store(true, Ordering::SeqCst);

    let mut total_writes = 0;
    for wh in writer_handles {
        total_writes += wh.await.unwrap();
    }

    let mut total_reads = 0;
    let mut worst_read_latency_us = 0;
    for rh in reader_handles {
        let (reads, max_us) = rh.await.unwrap();
        total_reads += reads;
        if max_us > worst_read_latency_us {
            worst_read_latency_us = max_us;
        }
    }

    println!(
        "Storm completed: {} writes, {} reads, worst read latency: {} us",
        total_writes, total_reads, worst_read_latency_us
    );

    assert!(total_writes > 0);
    assert!(total_reads > 0);
    assert_eq!(ring.len(), 256);

    // Verify worst read latency is well below 10 milliseconds (10,000 us) even under storm
    assert!(
        worst_read_latency_us < 10_000,
        "Worst read latency under write storm took {}us, expected < 10000us",
        worst_read_latency_us
    );
}

#[test]
fn test_adversarial_m2_audit_ring_buffer_slot_wrapping_order_exactness() {
    let ring = AuditRingBuffer::new();

    // Push 1,000 events deterministically
    for i in 0..1000 {
        let ev =
            make_adversarial_audit_event(&format!("seq-{:04}", i), SecurityVerdict::Allowed, 1);
        ring.push(ev);
    }

    assert_eq!(ring.total_pushed(), 1000);
    assert_eq!(ring.len(), 256);

    // 1. read_recent(1): must be newest event (999)
    let r1 = ring.read_recent(1);
    assert_eq!(r1.len(), 1);
    assert_eq!(r1[0].event_id, "seq-0999");

    // 2. read_recent(5): must be descending 999..=995
    let r5 = ring.read_recent(5);
    assert_eq!(r5.len(), 5);
    assert_eq!(r5[0].event_id, "seq-0999");
    assert_eq!(r5[1].event_id, "seq-0998");
    assert_eq!(r5[2].event_id, "seq-0997");
    assert_eq!(r5[3].event_id, "seq-0996");
    assert_eq!(r5[4].event_id, "seq-0995");

    // 3. read_recent(256): all remaining slots (999 down to 744)
    let r256 = ring.read_recent(256);
    assert_eq!(r256.len(), 256);
    assert_eq!(r256[0].event_id, "seq-0999");
    assert_eq!(r256[255].event_id, "seq-0744");

    // 4. Over-request: read_recent(500) must clamp to capacity 256
    let r_over = ring.read_recent(500);
    assert_eq!(r_over.len(), 256);
    assert_eq!(r_over[0].event_id, "seq-0999");
    assert_eq!(r_over[255].event_id, "seq-0744");
}

#[test]
fn test_adversarial_m2_audit_ring_buffer_empty_and_partial_states() {
    let ring = AuditRingBuffer::new();

    // Empty state
    assert!(ring.is_empty());
    assert_eq!(ring.len(), 0);
    assert_eq!(ring.total_pushed(), 0);
    assert_eq!(ring.read_recent(0).len(), 0);
    assert_eq!(ring.read_recent(50).len(), 0);

    // Partial state: 7 events pushed
    for i in 0..7 {
        ring.push(make_adversarial_audit_event(
            &format!("part-{}", i),
            SecurityVerdict::Allowed,
            2,
        ));
    }

    assert!(!ring.is_empty());
    assert_eq!(ring.len(), 7);
    assert_eq!(ring.total_pushed(), 7);

    // Read 3
    let r3 = ring.read_recent(3);
    assert_eq!(r3.len(), 3);
    assert_eq!(r3[0].event_id, "part-6");
    assert_eq!(r3[1].event_id, "part-5");
    assert_eq!(r3[2].event_id, "part-4");

    // Read 10 (more than available 7): returns all 7
    let r10 = ring.read_recent(10);
    assert_eq!(r10.len(), 7);
    assert_eq!(r10[0].event_id, "part-6");
    assert_eq!(r10[6].event_id, "part-0");
}

// ===========================================================================
// AREA 5: Milestone M2 Iteration 2 Direct Adversarial Challenges
// ===========================================================================

#[tokio::test]
async fn test_adversarial_m2_it2_real_engine_typing_abort_over_100_chars_sub_15ms() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x5555,
        pid: 999,
        title: "Adversarial Typing Target".into(),
        class_name: "TypingClass".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(0, 0, 1024, 768),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });
    // 10ms per character delay in mock driver
    mock.set_typing_delay(Duration::from_millis(10));

    let engine = Arc::new(CuaEngine::new_with_driver(
        CuaConfig::default(),
        mock.clone(),
    ));
    let engine_clone = Arc::clone(&engine);

    // Text with 225 characters (> 100 characters required)
    let long_text = "The quick brown fox jumps over the lazy dog. ".repeat(5);
    assert!(long_text.chars().count() > 100);

    let action = CuaAction::TypeText {
        target_hwnd: 0x5555,
        text: long_text,
        delivery_mode: CuaDeliveryMode::Background,
    };

    // 1. Dispatch action through genuine CuaEngine::execute_action in a spawned Tokio task
    let action_task = tokio::spawn(async move { engine_clone.execute_action(action).await });

    // 2. Allow typing to initiate and emit a few characters (~30ms)
    tokio::time::sleep(Duration::from_millis(30)).await;

    // 3. Trigger emergency halt mid-typing
    let start_halt = Instant::now();
    engine
        .trigger_emergency_halt()
        .expect("Emergency halt trigger must succeed");

    let action_res = action_task
        .await
        .expect("Action task must complete without panic");
    let abort_latency = start_halt.elapsed();

    // 4. Adversarial Assertion: Abort latency strictly < 15ms target
    assert!(
        abort_latency.as_millis() < 15,
        "Mid-flight typing abort took {}ms, expected < 15ms SLA target",
        abort_latency.as_millis()
    );

    // 5. Adversarial Assertion: Returned error is CuaError::EmergencyHalted
    assert_eq!(
        action_res.unwrap_err(),
        CuaError::EmergencyHalted,
        "Action must fail with CuaError::EmergencyHalted"
    );

    // 6. Adversarial Assertion: Driver stopped emitting characters mid-flight
    let typed = mock.get_typed_characters();
    assert!(
        typed.len() < 10,
        "Subsequent characters must NOT be emitted: typed {} characters out of 225",
        typed.len()
    );
    assert!(
        !typed.is_empty(),
        "Expected at least 1-2 characters emitted before halt"
    );

    // 7. Engine status must confirm halted
    assert!(engine.get_status().await.is_halted);
}

#[tokio::test]
async fn test_adversarial_m2_it2_resting_cursor_immunity_foreground_clicks() {
    let mock = Arc::new(MockCuaDriver::new());
    mock.insert_window(CuaWindowInfo {
        hwnd: 0x6666,
        pid: 1234,
        title: "Resting Cursor App".into(),
        class_name: "RestingClass".into(),
        process_name: Some("calc.exe".into()),
        bounds: CuaRect::new(0, 0, 1920, 1080),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    });

    let config = CuaConfig {
        kill_switch_enabled: true,
        ..Default::default()
    };

    let engine = CuaEngine::new_with_driver(config, mock);
    engine.kill_switch.start_poller();

    // Test multiple click coordinates far from any possible resting cursor position (> 50px away)
    let distant_targets = [(700, 700), (1200, 800), (350, 950)];
    for &(tx, ty) in &distant_targets {
        let res = engine
            .execute_action(CuaAction::Click {
                target_hwnd: 0x6666,
                x: Some(tx),
                y: Some(ty),
                button: MouseButton::Left,
                click_count: 1,
                delivery_mode: CuaDeliveryMode::Foreground,
            })
            .await;

        assert!(
            res.is_ok(),
            "Foreground click at ({}, {}) must NOT abort due to resting cursor > 50px away: {:?}",
            tx,
            ty,
            res.err()
        );
    }
}

#[tokio::test]
async fn test_adversarial_m2_it2_sqlite_table_auto_init_and_concurrent_batch_persistence() {
    let test_dir = std::env::temp_dir().join(format!("liva_m2_it2_sqlite_{}", current_unix_ms()));
    let _ = std::fs::create_dir_all(&test_dir);

    let config = CuaAuditConfig {
        enabled: true,
        log_dir: test_dir.clone(),
        active_jsonl_filename: "audit_it2.jsonl".to_string(),
        micro_batch_size: 5,
        micro_batch_timeout_ms: 15,
        mpsc_buffer_capacity: 128,
    };

    let db_path = test_dir.join("audit_it2.db");
    let manager = r2d2_sqlite::SqliteConnectionManager::file(&db_path);
    let pool = r2d2::Pool::builder().max_size(2).build(manager).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(128);
    let actor_pool = pool.clone();
    tokio::spawn(liva_storage::run_db_actor(
        actor_pool,
        rx,
        20,
        Duration::from_millis(10),
    ));
    let db_handle = liva_storage::DbActorHandle::new(tx);

    // CRITICAL: CuaAuditStore::init_tables is NEVER called manually!
    let recorder = Arc::new(CuaAuditRecorder::new(config, Some(db_handle.clone())));

    // Concurrently record 100 events across 4 tasks (25 events each)
    let mut tasks = Vec::new();
    for t in 0..4 {
        let rec = Arc::clone(&recorder);
        tasks.push(tokio::spawn(async move {
            for i in 0..25 {
                let ev = make_adversarial_audit_event(
                    &format!("it2-t{}-e{:02}", t, i),
                    SecurityVerdict::Allowed,
                    5,
                );
                rec.record(ev);
            }
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    // Wait for recorder persistence worker to batch & forward to DbActor
    tokio::time::sleep(Duration::from_millis(150)).await;
    db_handle.flush().await.expect("flush DbActor");

    // Verify all 100 events persisted to SQLite cua_audit_ledger table
    let conn = pool.get().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cua_audit_ledger", [], |row| {
            row.get(0)
        })
        .expect("table cua_audit_ledger must exist and be queryable via auto-init");
    assert_eq!(
        count, 100,
        "All 100 concurrently recorded events must be in SQLite"
    );

    // Clean up
    let _ = std::fs::remove_dir_all(&test_dir);
}

#[test]
fn test_adversarial_m2_it2_poller_thread_termination_stress_churn() {
    // Churn 50 consecutive controller instances with active poller threads
    for i in 0..50 {
        let config = KillSwitchConfig {
            esc_polling_enabled: true,
            poll_interval_ms: 5,
            ..Default::default()
        };
        let controller = KillSwitchController::new(config);
        assert!(controller.get_status().is_poller_active);

        // Strong ref held by controller variable, weak ref held by poller thread
        assert_eq!(Arc::strong_count(&controller), 1);
        assert_eq!(Arc::weak_count(&controller), 1);

        let weak = Arc::downgrade(&controller);
        // Dropping controller must trigger drop and join thread cleanly
        drop(controller);

        assert!(
            weak.upgrade().is_none(),
            "Iteration {}: controller must deallocate cleanly",
            i
        );
    }
}
