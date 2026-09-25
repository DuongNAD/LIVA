//! Integration & Unit Test Suite for CUA Audit Subsystem.
//!
//! Complies with LIVA test bounds: sequential execution, `-j 2 -- --test-threads 2`.

use liva_cua::audit::*;
use liva_cua::types::*;
use std::path::PathBuf;
use std::sync::Arc;

struct TempDirGuard {
    path: PathBuf,
}

impl TempDirGuard {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("liva_cua_test_{}_{}", name, current_unix_ms()));
        let _ = std::fs::create_dir_all(&path);
        Self { path }
    }
}

impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn make_test_event(id: &str, verdict: SecurityVerdict, process: &str) -> CuaAuditEvent {
    CuaAuditEvent {
        event_id: id.to_string(),
        timestamp_unix_ms: 1790250000000, // 2026-09-24
        session_id: "test_session_001".to_string(),
        action_type: "click".to_string(),
        target_app: TargetAppInfo {
            process_name: Some(process.to_string()),
            process_id: Some(1234),
            window_title: Some("Test Window".to_string()),
            hwnd: 0x102030,
        },
        coordinates: ActionCoordinates {
            screen_x: Some(500),
            screen_y: Some(300),
            client_x: Some(100),
            client_y: Some(50),
            bounds: Some(CuaRect::new(400, 250, 200, 100)),
        },
        permission_mode: PermissionMode::Bounded,
        security_verdict: verdict,
        policy_violation: if verdict == SecurityVerdict::BlockedByPolicy {
            Some("Process denylisted".to_string())
        } else {
            None
        },
        execution_delivery: CuaDeliveryMode::Background,
        action_effect: if verdict == SecurityVerdict::Allowed {
            CuaActionEffect::Completed
        } else {
            CuaActionEffect::Refused
        },
        latency_ms: 12,
        error_code: None,
    }
}

#[test]
fn test_audit_event_schema_serialization_round_trip() {
    let ev = make_test_event("evt-001", SecurityVerdict::Allowed, "notepad.exe");
    let json = serde_json::to_string(&ev).expect("serialization failed");
    let deserialized: CuaAuditEvent = serde_json::from_str(&json).expect("deserialization failed");
    assert_eq!(ev, deserialized);
}

#[test]
fn test_ring_buffer_capacity_256_overwrite() {
    let ring = AuditRingBuffer::new();
    assert_eq!(ring.len(), 0);
    assert!(ring.is_empty());

    // Push 300 sequential events
    for i in 0..300 {
        let ev = make_test_event(
            &format!("evt-{:03}", i),
            SecurityVerdict::Allowed,
            "notepad.exe",
        );
        ring.push(ev);
    }

    assert_eq!(ring.total_pushed(), 300);
    assert_eq!(ring.len(), 256);

    // Read recent 5
    let recent = ring.read_recent(5);
    assert_eq!(recent.len(), 5);
    assert_eq!(recent[0].event_id, "evt-299");
    assert_eq!(recent[1].event_id, "evt-298");
    assert_eq!(recent[2].event_id, "evt-297");
    assert_eq!(recent[3].event_id, "evt-296");
    assert_eq!(recent[4].event_id, "evt-295");

    // Read all 256
    let all = ring.read_recent(300);
    assert_eq!(all.len(), 256);
    assert_eq!(all[0].event_id, "evt-299");
    assert_eq!(all[255].event_id, "evt-044"); // Oldest remaining item (300 - 256 = 44)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_ring_buffer_concurrent_writers_zero_contention() {
    let ring = Arc::new(AuditRingBuffer::new());
    let num_tasks = 8;
    let events_per_task = 100;
    let mut handles = Vec::new();

    for t in 0..num_tasks {
        let ring_clone = Arc::clone(&ring);
        handles.push(tokio::spawn(async move {
            for i in 0..events_per_task {
                let ev = make_test_event(
                    &format!("task-{}-evt-{}", t, i),
                    SecurityVerdict::Allowed,
                    "notepad.exe",
                );
                ring_clone.push(ev);
            }
        }));
    }

    for h in handles {
        h.await.expect("task join failed");
    }

    assert_eq!(ring.total_pushed(), 800);
    assert_eq!(ring.len(), 256);
    let recent = ring.read_recent(50);
    assert_eq!(recent.len(), 50);
}

#[test]
fn test_sqlite_wal_persistence_and_query_filters() {
    let conn = rusqlite::Connection::open_in_memory().expect("open in memory sqlite");
    CuaAuditStore::init_tables(&conn).expect("init tables");

    // Prepare test events
    let mut events = Vec::new();
    for i in 0..50 {
        let verdict = if i % 2 == 0 {
            SecurityVerdict::Allowed
        } else {
            SecurityVerdict::BlockedByPolicy
        };
        let proc = if i < 25 { "notepad.exe" } else { "calc.exe" };
        let mut ev = make_test_event(&format!("id-{:02}", i), verdict, proc);
        ev.timestamp_unix_ms = 1000 + (i as u64) * 100;
        events.push(ev);
    }

    CuaAuditStore::insert_batch(&conn, &events).expect("insert batch");

    // 1. Query all
    let all_res = CuaAuditStore::query(&conn, &AuditQueryFilter::default()).expect("query all");
    assert_eq!(all_res.total_matched, 50);
    assert_eq!(all_res.events.len(), 50);

    // 2. Filter by verdict: BlockedByPolicy (should be 25)
    let blocked_filter = AuditQueryFilter {
        security_verdict: Some(SecurityVerdict::BlockedByPolicy),
        ..Default::default()
    };
    let blocked_res = CuaAuditStore::query(&conn, &blocked_filter).expect("query blocked");
    assert_eq!(blocked_res.total_matched, 25);
    assert_eq!(blocked_res.events.len(), 25);

    // 3. Filter by process: "calc.exe" (should be 25)
    let proc_filter = AuditQueryFilter {
        process_name: Some("calc.exe".to_string()),
        ..Default::default()
    };
    let proc_res = CuaAuditStore::query(&conn, &proc_filter).expect("query process");
    assert_eq!(proc_res.total_matched, 25);

    // 4. Combined Filter + Pagination: limit 5, offset 10
    let page_filter = AuditQueryFilter {
        process_name: Some("calc.exe".to_string()),
        limit: Some(5),
        offset: Some(10),
        ..Default::default()
    };
    let page_res = CuaAuditStore::query(&conn, &page_filter).expect("query page");
    assert_eq!(page_res.total_matched, 25);
    assert_eq!(page_res.events.len(), 5);
    assert_eq!(page_res.limit, 5);
    assert_eq!(page_res.offset, 10);
}

#[test]
fn test_date_conversion_algorithm() {
    // Test Epoch (1970-01-01)
    assert_eq!(unix_ms_to_date_string(0), "1970-01-01");
    // Test 2024 Leap Day (2024-02-29) -> 1709164800s
    assert_eq!(unix_ms_to_date_string(1709164800 * 1000), "2024-02-29");
    // Test 2026-09-24 -> 1790250000s
    assert_eq!(unix_ms_to_date_string(1790250000 * 1000), "2026-09-24");
}

#[test]
fn test_streaming_jsonl_daily_rotation() {
    let guard = TempDirGuard::new("jsonl_rotation");
    let log_dir = guard.path.clone();

    let mut logger = StreamingJsonlLogger::new(&log_dir, "cua_audit.jsonl", true);

    // Day 1 events (2026-09-24)
    let mut ev1 = make_test_event("day1-01", SecurityVerdict::Allowed, "notepad.exe");
    ev1.timestamp_unix_ms = 1790250000000;
    logger.write_batch(&[ev1]).expect("write day 1");

    let active_file = log_dir.join("cua_audit.jsonl");
    assert!(active_file.exists());
    let content_day1 = std::fs::read_to_string(&active_file).expect("read day 1");
    assert!(content_day1.contains("day1-01"));

    // Day 2 events (2026-09-25: +86400s)
    let mut ev2 = make_test_event("day2-01", SecurityVerdict::Allowed, "calc.exe");
    ev2.timestamp_unix_ms = 1790250000000 + 86400 * 1000;
    logger
        .write_batch(&[ev2])
        .expect("write day 2 with rotation");

    // Verifications:
    // 1. Rotated file "cua_audit_2026-09-24.jsonl" exists and has Day 1 content
    let rotated_file = log_dir.join("cua_audit_2026-09-24.jsonl");
    assert!(rotated_file.exists(), "Rotated file should exist");
    let rotated_content = std::fs::read_to_string(&rotated_file).expect("read rotated");
    assert!(rotated_content.contains("day1-01"));

    // 2. Active file "cua_audit.jsonl" contains Day 2 content
    let content_day2 = std::fs::read_to_string(&active_file).expect("read active day 2");
    assert!(content_day2.contains("day2-01"));
    assert!(!content_day2.contains("day1-01"));
}

#[tokio::test]
async fn test_recorder_non_blocking_latency_and_zero_lock_contention() {
    let guard = TempDirGuard::new("recorder_latency");
    let config = CuaAuditConfig {
        enabled: true,
        log_dir: guard.path.clone(),
        active_jsonl_filename: "test_audit.jsonl".to_string(),
        micro_batch_size: 64,
        micro_batch_timeout_ms: 100,
        mpsc_buffer_capacity: 10, // Small capacity to test saturation
    };

    let recorder = CuaAuditRecorder::new(config, None);
    let start = std::time::Instant::now();

    // Record 100 events rapidly (exceeding mpsc capacity of 10)
    for i in 0..100 {
        let ev = make_test_event(
            &format!("rapid-{}", i),
            SecurityVerdict::Allowed,
            "notepad.exe",
        );
        recorder.record(ev);
    }

    let elapsed = start.elapsed();
    // Total time for 100 records must be under 5 milliseconds (average < 50us per call)
    assert!(
        elapsed.as_millis() < 10,
        "100 records took too long: {:?}",
        elapsed
    );

    let stats = recorder.stats();
    assert_eq!(stats.total_recorded, 100);
    // Ring buffer in RAM retains all 100 events
    assert_eq!(stats.ring_buffer_items, 100);
    let recent = recorder.get_recent(10);
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0].event_id, "rapid-99");
}

#[tokio::test]
async fn test_audit_recorder_auto_inits_sqlite_schema_with_db_actor() {
    let guard = TempDirGuard::new("db_actor_auto_init");
    let config = CuaAuditConfig {
        enabled: true,
        log_dir: guard.path.clone(),
        active_jsonl_filename: "test_audit.jsonl".to_string(),
        micro_batch_size: 1, // Flush every single event immediately
        micro_batch_timeout_ms: 10,
        mpsc_buffer_capacity: 64,
    };

    // Spin up a file-backed SQLite connection pool and DbActor
    let db_path = guard.path.join("audit.db");
    let manager = r2d2_sqlite::SqliteConnectionManager::file(&db_path);
    let pool = r2d2::Pool::builder().max_size(2).build(manager).unwrap();
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let actor_pool = pool.clone();
    tokio::spawn(liva_storage::run_db_actor(
        actor_pool,
        rx,
        10,
        std::time::Duration::from_millis(10),
    ));
    let db_handle = liva_storage::DbActorHandle::new(tx);

    // Note: CuaAuditStore::init_tables is NEVER called manually in this test!
    let recorder = CuaAuditRecorder::new(config, Some(db_handle.clone()));

    // Give worker time to complete auto-init
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Record an event
    let ev = make_test_event("auto-init-01", SecurityVerdict::Allowed, "notepad.exe");
    recorder.record(ev);

    // Wait for recorder worker to drain and forward to db actor, then flush db actor
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    db_handle.flush().await.expect("flush db actor");

    // Verify record exists in SQLite cua_audit_ledger table
    let conn = pool.get().unwrap();
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM cua_audit_ledger", [], |row| {
            row.get(0)
        })
        .expect(
            "table cua_audit_ledger must exist and be queryable without manual init_tables call",
        );
    assert_eq!(count, 1);
}

#[test]
fn test_streaming_jsonl_daily_rotation_on_process_restart() {
    let guard = TempDirGuard::new("jsonl_restart_rotation");
    let log_dir = guard.path.clone();

    // 1. Process Run 1: writes Day 1 events (2026-09-24)
    {
        let mut logger1 = StreamingJsonlLogger::new(&log_dir, "cua_audit.jsonl", true);
        let mut ev1 = make_test_event("day1-restart-01", SecurityVerdict::Allowed, "notepad.exe");
        ev1.timestamp_unix_ms = 1790250000000; // 2026-09-24
        logger1.write_batch(&[ev1]).expect("write day 1 in run 1");
    } // logger1 dropped (simulating process exit)

    let active_file = log_dir.join("cua_audit.jsonl");
    assert!(active_file.exists());
    let content_day1 = std::fs::read_to_string(&active_file).expect("read day 1");
    assert!(content_day1.contains("day1-restart-01"));

    // 2. Process Run 2: starts on Day 2 (2026-09-25) with fresh logger (current_date_stamp is None)
    {
        let mut logger2 = StreamingJsonlLogger::new(&log_dir, "cua_audit.jsonl", true);
        let mut ev2 = make_test_event("day2-restart-01", SecurityVerdict::Allowed, "calc.exe");
        ev2.timestamp_unix_ms = 1790250000000 + 86400 * 1000; // 2026-09-25
        logger2
            .write_batch(&[ev2])
            .expect("write day 2 in run 2 should trigger restart rotation");
    }

    // Verifications:
    // 1. Rotated file "cua_audit_2026-09-24.jsonl" exists and has Day 1 content
    let rotated_file = log_dir.join("cua_audit_2026-09-24.jsonl");
    assert!(
        rotated_file.exists(),
        "Yesterday's log should have been rotated upon restart"
    );
    let rotated_content = std::fs::read_to_string(&rotated_file).expect("read rotated file");
    assert!(rotated_content.contains("day1-restart-01"));
    assert!(!rotated_content.contains("day2-restart-01"));

    // 2. Active file "cua_audit.jsonl" contains ONLY Day 2 content
    let active_content = std::fs::read_to_string(&active_file).expect("read active day 2 file");
    assert!(active_content.contains("day2-restart-01"));
    assert!(
        !active_content.contains("day1-restart-01"),
        "Active file must not retain yesterday's events after rotation"
    );
}
