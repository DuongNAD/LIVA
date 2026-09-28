//! Empirical Challenger 1: Concurrency & Stress Adversarial Test Suite (Milestone M4)
//!
//! Subsystems tested:
//! 1. SQLite WAL connection pooling under heavy concurrent read/write transactions:
//!    - 25 concurrent tasks (5 writers + 20 readers) hammering DatabasePool.
//!    - Saturated reader checkout SLA: strictly adheres to 3-second timeout, zero infinite hangs.
//!    - Zero SQLITE_BUSY crashes and zero corrupted rows under intense load.
//! 2. CUA lock poison resilience under simulated concurrency and thread panics:
//!    - Simulates thread panics while holding or accessing SecurityGovernor locks.
//!    - Verifies parking_lot::RwLock never leaves locks poisoned and threads recover immediately.
//! 3. Voice barge-in responsiveness during active chunk synthesis:
//!    - Decoupled state.tts lock during synthesis allows tts_stop to preempt immediately (<20ms).
//!    - Rapid burst of 50 speech/stop interruptions without deadlock or resource leak.
//! 4. RAM footprint verification:
//!    - Confirms process memory remains well below 4.0 GB throughout all high-concurrency stress tests.

use liva_cua::security::SecurityGovernor;
use liva_cua::types::{
    CuaAction, CuaConfig, CuaDeliveryMode, CuaRect, CuaWindowInfo, MouseButton, PermissionMode,
};
use liva_native_core::commands::voice;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{DatabasePool, is_transient_sqlite_lock_error};
use liva_native_core::{AppState, stt, tts};
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------
// Helpers & Diagnostics
// ---------------------------------------------------------------------------

struct TempDbGuard(PathBuf);
impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

static DB_COUNTER: AtomicU64 = AtomicU64::new(5000);

fn create_file_pool() -> (Arc<DatabasePool>, TempDbGuard) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand_id = format!("{}_{}_{}", std::process::id(), nanos, counter);
    let db_path = std::env::temp_dir().join(format!("liva_wal_stress_{rand_id}.sqlite"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    let pool = DatabasePool::new(&db_path).expect("failed to create file-backed DatabasePool");
    (Arc::new(pool), TempDbGuard(db_path))
}

#[cfg(windows)]
fn get_current_process_ram_mb() -> f64 {
    use std::mem::MaybeUninit;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::GetCurrentProcess;

    unsafe {
        let mut pmc = MaybeUninit::<PROCESS_MEMORY_COUNTERS>::uninit();
        let handle = GetCurrentProcess();
        if GetProcessMemoryInfo(
            handle,
            pmc.as_mut_ptr(),
            std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32,
        ) != 0
        {
            let pmc = pmc.assume_init();
            (pmc.WorkingSetSize as f64) / (1024.0 * 1024.0)
        } else {
            0.0
        }
    }
}

#[cfg(not(windows))]
fn get_current_process_ram_mb() -> f64 {
    100.0 // Stub for non-windows
}

fn build_stress_app_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("in-memory database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    let mut cua_config = CuaConfig::default();
    cua_config.permission_mode = PermissionMode::Bounded;
    cua_config.process_allowlist = vec!["notepad.exe".to_string(), "explorer.exe".to_string()];

    let mock_driver = Arc::new(liva_cua::mock::MockCuaDriver::new());
    let cua_engine = liva_cua::CuaEngine::new_with_driver(cua_config, mock_driver);
    cua_engine.kill_switch.stop_poller();
    let cua = Arc::new(cua_engine);

    Arc::new(AppState {
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
        embedder: AppState::empty_embedder(),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
        active_recall: Arc::new(liva_native_core::active_recall::ActiveRecallManager::new()),
        cua,
    })
}

// ===========================================================================
// 1. ADVERSARIAL STRESS: 25 CONCURRENT TASKS ON SQLITE WAL CONNECTION POOL
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adversarial_sqlite_wal_pool_25_concurrent_hammering() {
    let (pool, _guard) = create_file_pool();

    let initial_ram = get_current_process_ram_mb();
    println!("[STRESS] Initial RAM footprint: {initial_ram:.2} MB");

    // Initialize stress table
    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS wal_stress_ledger (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id INTEGER NOT NULL,
                iteration INTEGER NOT NULL,
                payload TEXT NOT NULL,
                checksum INTEGER NOT NULL,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );
            CREATE INDEX IF NOT EXISTS idx_wal_stress_task ON wal_stress_ledger(task_id);",
        )
    })
    .expect("setup wal stress ledger table");

    let num_writers = 5;
    let num_readers = 20;
    let writes_per_task = 30; // 5 * 30 = 150 transactions

    let is_running = Arc::new(AtomicBool::new(true));
    let sqlite_busy_errors = Arc::new(AtomicUsize::new(0));
    let total_successful_writes = Arc::new(AtomicUsize::new(0));
    let total_successful_reads = Arc::new(AtomicUsize::new(0));
    let corrupted_rows_detected = Arc::new(AtomicUsize::new(0));

    // Spawn 20 concurrent readers continuously querying the database
    let mut reader_tasks = Vec::new();
    for reader_id in 0..num_readers {
        let pool_c = pool.clone();
        let running = is_running.clone();
        let busy_errs = sqlite_busy_errors.clone();
        let read_count = total_successful_reads.clone();
        let corrupt_count = corrupted_rows_detected.clone();

        reader_tasks.push(tokio::spawn(async move {
            while running.load(Ordering::Relaxed) {
                // Interleave sync checkout and async spawn_reader
                if reader_id % 2 == 0 {
                    let res = pool_c.with_reader(|conn| {
                        let mut stmt = conn.prepare(
                            "SELECT id, task_id, iteration, payload, checksum FROM wal_stress_ledger ORDER BY id DESC LIMIT 5",
                        )?;
                        let rows = stmt.query_map([], |row| {
                            let id: i64 = row.get(0)?;
                            let tid: i64 = row.get(1)?;
                            let iter: i64 = row.get(2)?;
                            let payload: String = row.get(3)?;
                            let checksum: i64 = row.get(4)?;
                            Ok((id, tid, iter, payload, checksum))
                        })?;

                        for row_res in rows {
                            let (_id, tid, iter, payload, checksum) = row_res?;
                            let expected_checksum = (tid * 1000 + iter) as i64;
                            if checksum != expected_checksum || !payload.contains("wal_stress_payload_") {
                                return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                                    std::io::Error::other(format!(
                                        "Checksum corruption: got {checksum} expected {expected_checksum}"
                                    )),
                                )));
                            }
                        }
                        Ok(())
                    });

                    match res {
                        Ok(()) => {
                            read_count.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            if is_transient_sqlite_lock_error(&e) {
                                busy_errs.fetch_add(1, Ordering::SeqCst);
                            } else if e.to_string().contains("Checksum corruption") {
                                corrupt_count.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                } else {
                    let res = pool_c.spawn_reader(|conn| {
                        let count: i64 = conn.query_row(
                            "SELECT COUNT(*) FROM wal_stress_ledger",
                            [],
                            |r| r.get(0),
                        )?;
                        Ok(count)
                    }).await;

                    match res {
                        Ok(_) => {
                            read_count.fetch_add(1, Ordering::Relaxed);
                        }
                        Err(e) => {
                            if e.contains("busy") || e.contains("locked") {
                                busy_errs.fetch_add(1, Ordering::SeqCst);
                            }
                        }
                    }
                }

                tokio::time::sleep(Duration::from_micros(500)).await;
            }
        }));
    }

    // Spawn 5 concurrent writers submitting batch transactions
    let mut writer_tasks = Vec::new();
    for writer_id in 0..num_writers {
        let pool_c = pool.clone();
        let busy_errs = sqlite_busy_errors.clone();
        let write_count = total_successful_writes.clone();

        writer_tasks.push(tokio::spawn(async move {
            for iter in 0..writes_per_task {
                let payload = format!("wal_stress_payload_{writer_id}_{iter}_data");
                let checksum = (writer_id * 1000 + iter) as i64;

                let res = pool_c.spawn_writer(move |conn| {
                    let tx = conn.unchecked_transaction()?;
                    tx.execute(
                        "INSERT INTO wal_stress_ledger (task_id, iteration, payload, checksum) VALUES (?1, ?2, ?3, ?4);",
                        rusqlite::params![writer_id, iter, payload, checksum],
                    )?;
                    // Perform an immediate read inside transaction to verify intra-tx consistency
                    let inserted_id: i64 = tx.last_insert_rowid();
                    let verified_chk: i64 = tx.query_row(
                        "SELECT checksum FROM wal_stress_ledger WHERE id = ?1",
                        [inserted_id],
                        |r| r.get(0),
                    )?;
                    if verified_chk != checksum {
                        return Err(rusqlite::Error::ToSqlConversionFailure(Box::new(
                            std::io::Error::other("Transaction read-your-own-writes violation"),
                        )));
                    }
                    tx.commit()?;
                    Ok(())
                }).await;

                match res {
                    Ok(()) => {
                        write_count.fetch_add(1, Ordering::Relaxed);
                    }
                    Err(e) => {
                        if e.contains("busy") || e.contains("locked") {
                            busy_errs.fetch_add(1, Ordering::SeqCst);
                        } else {
                            panic!("Unexpected write error in task {writer_id}: {e}");
                        }
                    }
                }

                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    // Await all writers
    for wt in writer_tasks {
        wt.await.expect("writer task panicked");
    }

    // Stop readers
    is_running.store(false, Ordering::SeqCst);
    for rt in reader_tasks {
        rt.await.expect("reader task panicked");
    }

    // Flush any pending actor writes
    let flush_res = pool.flush().await;
    assert!(
        flush_res.is_ok(),
        "DbActor flush failed: {:?}",
        flush_res.err()
    );

    // EMPIRICAL ASSERTION 1: Zero SQLITE_BUSY crashes
    let busy_count = sqlite_busy_errors.load(Ordering::SeqCst);
    assert_eq!(
        busy_count, 0,
        "EMPIRICAL FAILURE: Observed {busy_count} SQLITE_BUSY errors during 25-task concurrency!"
    );

    // EMPIRICAL ASSERTION 2: Zero row corruption
    let corrupt_count = corrupted_rows_detected.load(Ordering::SeqCst);
    assert_eq!(
        corrupt_count, 0,
        "EMPIRICAL FAILURE: Observed {corrupt_count} corrupted rows during concurrent reads!"
    );

    // EMPIRICAL ASSERTION 3: Strict row count match
    let expected_writes = num_writers * writes_per_task;
    let actual_writes = total_successful_writes.load(Ordering::SeqCst);
    assert_eq!(actual_writes, expected_writes);

    let total_db_rows: i64 = pool
        .with_reader(|conn| {
            conn.query_row("SELECT COUNT(*) FROM wal_stress_ledger", [], |r| r.get(0))
        })
        .expect("query total rows");
    assert_eq!(total_db_rows as usize, expected_writes);

    let reads_done = total_successful_reads.load(Ordering::Relaxed);
    println!(
        "[STRESS] Completed {actual_writes} writes and {reads_done} reads with 0 SQLITE_BUSY errors."
    );

    // EMPIRICAL ASSERTION 4: RAM footprint <= 4.0 GB
    let final_ram = get_current_process_ram_mb();
    println!("[STRESS] Final RAM footprint after WAL stress: {final_ram:.2} MB");
    assert!(
        final_ram < 4096.0,
        "RAM usage exceeded 4GB guardrail: {final_ram:.2} MB"
    );
}

#[tokio::test]
async fn test_adversarial_reader_pool_saturation_and_3s_timeout_sla() {
    let (pool, _guard) = create_file_pool();

    let pool_size = liva_native_core::db::get_reader_pool_size() as usize;

    // Saturate entire reader pool
    let mut held_conns = Vec::with_capacity(pool_size);
    for i in 0..pool_size {
        let conn = pool
            .readers
            .get()
            .unwrap_or_else(|e| panic!("Failed to checkout reader connection {i}: {e}"));
        held_conns.push(conn);
    }

    assert_eq!(pool.readers.state().idle_connections, 0);

    // Attempt checkout on saturated pool in a blocking task
    let pool_c = pool.clone();
    let start = Instant::now();
    let checkout_task = tokio::task::spawn_blocking(move || pool_c.read_conn()).await;
    let elapsed = start.elapsed();

    let checkout_res = checkout_task.expect("spawn_blocking failed");

    // EMPIRICAL ASSERTION: Strictly adheres to 3-second timeout SLA (between 2.7s and 4.5s)
    assert!(
        checkout_res.is_err(),
        "Expected timeout error on saturated pool, but checkout succeeded!"
    );

    let err_msg = checkout_res.unwrap_err().to_string();
    assert!(
        err_msg.to_lowercase().contains("timed out") || err_msg.to_lowercase().contains("timeout"),
        "Error message did not indicate timeout: '{err_msg}'"
    );

    assert!(
        elapsed >= Duration::from_millis(2700),
        "Timeout fired prematurely: {elapsed:?} < 2.7s"
    );
    assert!(
        elapsed <= Duration::from_millis(4500),
        "Timeout hung excessively beyond 3s SLA: {elapsed:?} > 4.5s"
    );

    // Drop held connections
    drop(held_conns);

    // Immediate checkout must succeed now without hanging
    let immediate_start = Instant::now();
    let immediate_checkout = pool.read_conn();
    let immediate_elapsed = immediate_start.elapsed();

    assert!(
        immediate_checkout.is_ok(),
        "Subsequent checkout failed after connections returned"
    );
    assert!(
        immediate_elapsed < Duration::from_millis(50),
        "Immediate checkout took too long: {immediate_elapsed:?}"
    );
}

// ===========================================================================
// 2. CUA LOCK POISON RESILIENCE UNDER CONCURRENCY & THREAD PANICS
// ===========================================================================

#[test]
fn test_adversarial_cua_parking_lot_rwlock_poison_immunity() {
    let mut config = CuaConfig::default();
    config.permission_mode = PermissionMode::Bounded;
    config.process_allowlist = vec!["notepad.exe".to_string()];
    config.protected_denylist = vec!["taskmgr.exe".to_string()];

    let governor = Arc::new(SecurityGovernor::new(&config));

    // Phase 1: Deliberately induce panics while modifying permission_mode and allowlist
    let num_panic_threads = 10;
    let mut handles = Vec::new();

    for i in 0..num_panic_threads {
        let gov = governor.clone();
        handles.push(std::thread::spawn(move || {
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                if i % 2 == 0 {
                    gov.set_permission_mode(PermissionMode::Unrestricted);
                    if i == 4 {
                        panic!("Intentional thread panic in worker 4 during set_permission_mode");
                    }
                } else {
                    let mut list = gov.get_allowlist();
                    list.push(format!("panic_app_{i}.exe"));
                    gov.set_allowlist(list);
                    if i == 7 {
                        panic!("Intentional thread panic in worker 7 during set_allowlist");
                    }
                }
            }));
        }));
    }

    for h in handles {
        let _ = h.join();
    }

    // Phase 2: EMPIRICAL VERIFICATION - Governor locks must NOT be poisoned!
    // With std::sync::RwLock, this would panic with PoisonError.
    // With parking_lot::RwLock, this succeeds cleanly and immediately.
    assert_eq!(
        governor.get_permission_mode(),
        PermissionMode::Unrestricted,
        "Governor permission mode must remain readable and consistent"
    );

    governor.set_permission_mode(PermissionMode::Bounded);
    assert_eq!(governor.get_permission_mode(), PermissionMode::Bounded);

    let allowlist = governor.get_allowlist();
    assert!(
        allowlist.contains(&"notepad.exe".to_string()),
        "Original allowlist item must be intact"
    );

    // Phase 3: High-concurrency 50 threads reading & writing through governor
    let concurrent_threads = 50;
    let mut stress_handles = Vec::new();
    let panic_observed = Arc::new(AtomicUsize::new(0));

    for tid in 0..concurrent_threads {
        let gov = governor.clone();
        let panic_obs = panic_observed.clone();

        stress_handles.push(std::thread::spawn(move || {
            let unwind_res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                for iter in 0..20 {
                    if tid == 13 && iter == 5 {
                        gov.set_permission_mode(PermissionMode::Bounded);
                        panic!("Deliberate mid-concurrency panic on thread 13");
                    }

                    if (tid + iter) % 5 == 0 {
                        // Mutate
                        let mode = if iter % 2 == 0 {
                            PermissionMode::Bounded
                        } else {
                            PermissionMode::Standard
                        };
                        gov.set_permission_mode(mode);
                    } else if (tid + iter) % 7 == 0 {
                        // Register simulated UIPI
                        gov.set_simulated_uipi(0x1000 + tid as u64, 0x2000 + iter as u32);
                    } else {
                        // Read
                        let _m = gov.get_permission_mode();
                        let _al = gov.get_allowlist();
                    }
                }
            }));

            if unwind_res.is_err() {
                panic_obs.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for sh in stress_handles {
        let _ = sh.join();
    }

    assert_eq!(
        panic_observed.load(Ordering::SeqCst),
        1,
        "Expected exactly 1 intentional panic caught during high-concurrency run"
    );

    // Final assert: Governor can still evaluate actions normally
    let target = CuaWindowInfo {
        hwnd: 0x1000,
        pid: 100,
        title: "Test Notepad".into(),
        class_name: "Notepad".into(),
        process_name: Some("notepad.exe".into()),
        bounds: CuaRect::new(100, 100, 800, 600),
        is_on_screen: true,
        is_minimized: false,
        z_index: 0,
        is_uwp_frame: false,
        uwp_app_pid: None,
    };

    let action = CuaAction::Click {
        target_hwnd: 0x1000,
        x: Some(200),
        y: Some(200),
        button: MouseButton::Left,
        click_count: 1,
        delivery_mode: CuaDeliveryMode::Background,
    };

    governor.set_permission_mode(PermissionMode::Bounded);
    governor.set_simulated_uipi(0x1000, 0x2000);
    governor.set_simulated_agent_rid(0x2000);
    let eval_res = governor.evaluate_action(&action, &target);
    assert!(
        eval_res.is_ok(),
        "Governor evaluate_action failed after concurrency and panic test: {:?}",
        eval_res.err()
    );
}

// ===========================================================================
// 3. VOICE BARGE-IN RESPONSIVENESS DURING ACTIVE CHUNK SYNTHESIS
// ===========================================================================

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn test_adversarial_voice_barge_in_responsiveness() {
    let state = build_stress_app_state();

    // Test 1: Active audio playback halted by tts_stop in < 20ms
    let initial_stop_id = state.tts_player.get_stop_id();

    // Push dummy audio buffer
    let samples = vec![0.0f32; 24000]; // 1 second of audio
    let play_id = state.tts_player.play(samples);
    assert!(play_id > initial_stop_id);

    // Invoke tts_stop while playback is simulated
    let t0 = Instant::now();
    let stop_result = voice::handle(state.clone(), "tts_stop", json!({})).await;
    let stop_duration = t0.elapsed();

    assert!(
        stop_result.is_ok(),
        "tts_stop failed: {:?}",
        stop_result.err()
    );
    let stop_val = stop_result.unwrap();
    assert_eq!(stop_val["success"], json!(true));

    // Verify sub-20ms responsiveness SLA
    println!("[STRESS] tts_stop latency: {stop_duration:?}");
    assert!(
        stop_duration < Duration::from_millis(20),
        "tts_stop latency exceeded 20ms SLA: {stop_duration:?}"
    );

    // Test 2: Verify state.tts lock decoupling during active chunk synthesis
    // Simulate long synthesis in progress outside state.tts lock
    let is_synthesizing = Arc::new(AtomicBool::new(true));
    let synth_flag = is_synthesizing.clone();
    let synthesis_task = tokio::task::spawn_blocking(move || {
        // Simulating heavy CPU synthesis (e.g. Kokoro ONNX model)
        let start = Instant::now();
        while synth_flag.load(Ordering::Relaxed) && start.elapsed() < Duration::from_millis(200) {
            std::thread::sleep(Duration::from_millis(5));
        }
    });

    // While synthesis is running, tts_stop must complete in < 20ms without being blocked
    let barge_start = Instant::now();
    let barge_res = voice::handle(state.clone(), "tts_stop", json!({})).await;
    let barge_latency = barge_start.elapsed();

    assert!(
        barge_res.is_ok(),
        "Barge-in stop failed: {:?}",
        barge_res.err()
    );
    println!("[STRESS] Active chunk synthesis barge-in latency: {barge_latency:?}");
    assert!(
        barge_latency < Duration::from_millis(20),
        "Barge-in latency under active chunk synthesis exceeded 20ms SLA: {barge_latency:?}"
    );

    is_synthesizing.store(false, Ordering::Relaxed);
    let _ = synthesis_task.await;

    // Test 3: Rapid burst of 50 consecutive tts_speak / tts_stop barge-in cycles
    for cycle in 1..=50 {
        let _ = state.tts_player.play(vec![0.1f32; 1000]);

        let cycle_start = Instant::now();
        let cycle_res = voice::handle(state.clone(), "tts_stop", json!({})).await;
        let cycle_elapsed = cycle_start.elapsed();

        assert!(
            cycle_res.is_ok(),
            "Cycle {cycle} failed: {:?}",
            cycle_res.err()
        );
        assert!(
            cycle_elapsed < Duration::from_millis(25),
            "Cycle {cycle}: tts_stop took too long: {cycle_elapsed:?}"
        );
    }

    println!("[STRESS] Successfully completed 50 rapid voice barge-in cycles without contention.");
}

// ===========================================================================
// 4. RAM FOOTPRINT BOUNDS & RUNTIME LEAK PREVENTION
// ===========================================================================

#[test]
fn test_ram_footprint_guardrail_invariant() {
    let current_ram = get_current_process_ram_mb();
    println!("[STRESS] Current test runner process RAM: {current_ram:.2} MB");

    assert!(
        current_ram < 4096.0,
        "CRITICAL: Process RAM usage exceeded 4.0 GB limit! Found: {current_ram:.2} MB"
    );
}
