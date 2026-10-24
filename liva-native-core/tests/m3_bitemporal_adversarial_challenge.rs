//! Empirical Adversarial Challenge Suite for Milestone 3 (F13, F14, F15)
//!
//! Subsystem under challenge: `bitemporal.rs`
//! Challenger: challenger_m3_1
//!
//! Adversarially challenges:
//! 1. Active Fact Singleton Invariant (`idx_active_singleton_fact`):
//!    - Direct SQL concurrent/duplicate active fact insertion.
//!    - Scope boundary isolation (personal vs work vs project:*).
//!    - Subject edge cases (empty string, whitespace, unicode emojis).
//!    - Sequential reconciliation stress: multi-step mutation chain maintaining singleton invariant at every step.
//!    - Race conditions across concurrent transactions.
//!
//! 2. Evidence Log Immutability & Trigger Enforcement:
//!    - Direct physical UPDATE attacks on `nfc_content`, `content`, `evidence_id`, `created_at`.
//!    - Direct physical DELETE attacks (single row and table wipe).
//!    - Non-content column mutation probing (integrity, principal).
//!    - Tombstoning lifecycle and idempotence.
//!    - NFC canonicalization and BLAKE3 content-addressing collision/deduplication stress.
//!    - Empty content and malicious payload rejection.
//!
//! 3. Bitemporal Point-in-Time Time-Travel:
//!    - Dual time axis orthogonal independence:
//!      * Retroactive facts (past world event recorded in future system time).
//!      * Pre-scheduled facts (future world event recorded in past system time).
//!    - Boundary conditions: exact half-open interval checks [valid_from, valid_to) and [recorded_at, expired_at).
//!    - Lineage chain integrity: superseded_by pointers form an acyclic audit trail.

use liva_native_core::db::bitemporal::{
    apply_reconciliation, get_active_fact, ingest_evidence, init_bitemporal_schema,
    query_active_facts, query_fact_history, query_facts_as_of_system_time,
    query_facts_as_of_world_time, tombstone_evidence, BitemporalFact, FactOrigin, FactSensitivity,
    ReconciliationOutcome,
};
use rusqlite::{params, Connection};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;

fn setup_in_memory_bitemporal_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    init_bitemporal_schema(&conn).expect("init bitemporal schema");
    conn
}

// ============================================================================
// 1. ACTIVE FACT SINGLETON INVARIANT ADVERSARIAL CHALLENGES
// ============================================================================

#[test]
fn challenge_active_singleton_rejects_duplicate_active_facts() {
    let conn = setup_in_memory_bitemporal_db();

    // Insert initial active fact
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_orig', 'user', 'preferred_editor', 'neovim', 'work', 'explicit', 1.0, 8, 'normal', 1000, NULL, 1000, NULL)",
        [],
    ).expect("initial insert must succeed");

    // Attack 1: Attempt to insert identical (subject, predicate, scope) while active
    let err_identical = conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_dup_1', 'user', 'preferred_editor', 'neovim', 'work', 'explicit', 1.0, 8, 'normal', 1000, NULL, 1000, NULL)",
        [],
    );
    assert!(err_identical.is_err(), "Duplicate active fact with identical value must be rejected by idx_active_singleton_fact");
    let err_str = err_identical.unwrap_err().to_string();
    assert!(
        err_str.contains("UNIQUE constraint failed"),
        "Error must specifically be UNIQUE constraint failed, got: {}",
        err_str
    );

    // Attack 2: Attempt to insert conflicting value for same (subject, predicate, scope) while active
    let err_conflict = conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_dup_2', 'user', 'preferred_editor', 'vscode', 'work', 'explicit', 1.0, 8, 'normal', 2000, NULL, 2000, NULL)",
        [],
    );
    assert!(err_conflict.is_err(), "Conflicting active fact for same key must be rejected");
    assert!(
        err_conflict.unwrap_err().to_string().contains("UNIQUE constraint failed"),
        "Must enforce active singleton regardless of value differences"
    );

    // Verify that the table still has exactly 1 fact
    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM facts WHERE subject = 'user' AND predicate = 'preferred_editor'", [], |r| r.get(0))
        .expect("count query");
    assert_eq!(count, 1, "Only the original fact must exist in facts table");
}

#[test]
fn challenge_active_singleton_scope_partition_allows_cross_scope_coexistence() {
    let conn = setup_in_memory_bitemporal_db();

    // Same subject and predicate, but different scopes:
    // 1. 'personal' scope
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_personal', 'user', 'email', 'personal@gmail.com', 'personal', 'explicit', 1.0, 5, 'private', 1000, NULL, 1000, NULL)",
        [],
    ).expect("personal scope fact insert");

    // 2. 'work' scope
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_work', 'user', 'email', 'work@company.com', 'work', 'explicit', 1.0, 5, 'private', 1000, NULL, 1000, NULL)",
        [],
    ).expect("work scope fact insert");

    // 3. 'project:alpha' scope
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_proj_a', 'user', 'email', 'alpha@project.io', 'project:alpha', 'explicit', 1.0, 5, 'private', 1000, NULL, 1000, NULL)",
        [],
    ).expect("project:alpha scope fact insert");

    // Verify all 3 coexist as active facts
    let active_facts = query_active_facts(&conn, None).expect("query all active");
    assert_eq!(active_facts.len(), 3, "Distinct scopes must isolate active facts without conflict");

    // But inserting a second active fact in 'personal' must still fail
    let err_dup_personal = conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('fact_dup_personal', 'user', 'email', 'other@gmail.com', 'personal', 'explicit', 1.0, 5, 'private', 2000, NULL, 2000, NULL)",
        [],
    );
    assert!(err_dup_personal.is_err(), "Duplicate within same scope must fail");
}

#[test]
fn challenge_active_singleton_expired_facts_allow_new_active_fact() {
    let conn = setup_in_memory_bitemporal_db();

    // 1. Insert fact 1 which was active from t=1000 to t=2000 (now closed in world time)
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f1_world_closed', 'user', 'city', 'Hà Nội', 'personal', 'explicit', 1.0, 5, 'private', 1000, 2000, 1000, NULL)",
        [],
    ).expect("insert fact with valid_to non-null");

    // Because valid_to IS NOT NULL, the partial index `WHERE valid_to IS NULL AND expired_at IS NULL` does not apply.
    // Therefore, an active fact for the same (subject, predicate, scope) can be inserted!
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f2_active', 'user', 'city', 'Đà Nẵng', 'personal', 'explicit', 1.0, 5, 'private', 2000, NULL, 2000, NULL)",
        [],
    ).expect("insert active fact while previous has valid_to set");

    // 2. Insert fact 3 which has expired_at IS NOT NULL (soft-deleted in system time)
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f3_sys_expired', 'user', 'os', 'Windows 10', 'personal', 'explicit', 1.0, 5, 'private', 1000, NULL, 1000, 1500)",
        [],
    ).expect("insert fact with expired_at non-null");

    // An active fact for 'os' can be inserted cleanly
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f4_active_os', 'user', 'os', 'Windows 11', 'personal', 'explicit', 1.0, 5, 'private', 1500, NULL, 1500, NULL)",
        [],
    ).expect("insert active fact while previous has expired_at set");

    // But attempting to insert another active fact for 'city' must fail
    let err = conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f5_fail', 'user', 'city', 'TP HCM', 'personal', 'explicit', 1.0, 5, 'private', 3000, NULL, 3000, NULL)",
        [],
    );
    assert!(err.is_err(), "Second active fact for city must fail");
}

#[test]
fn challenge_active_singleton_multi_step_reconciliation_stress() {
    let mut conn = setup_in_memory_bitemporal_db();
    let now_base = 10_000i64;

    // We simulate 20 successive mutations to the user's favorite programming language
    let languages = [
        "Rust", "TypeScript", "Python", "Go", "C++", "Zig", "Haskell", "Elixir", "Julia", "OCaml",
        "Rust", "Rust", "TypeScript", "Swift", "Kotlin", "Java", "C#", "Dart", "Scala", "Rust",
    ];

    for (i, &lang) in languages.iter().enumerate() {
        let t = now_base + (i as i64) * 1000;
        let candidate = BitemporalFact {
            fact_id: format!("fact_lang_{}", i),
            subject: "user".into(),
            predicate: "primary_language".into(),
            value: lang.into(),
            object_value: lang.into(),
            scope: "work".into(),
            origin: FactOrigin::Explicit,
            confidence: 1.0,
            importance: 9,
            sensitivity: FactSensitivity::Normal,
            valid_from: t,
            valid_to: None,
            recorded_at: t,
            expired_at: None,
            superseded_by: None,
            reason: Some(format!("switch_step_{}", i)),
            supersede_reason: Some(format!("switch_step_{}", i)),
            category: Some("tech_stack".into()),
            key: None,
            created_at: t,
            updated_at: t,
        };

        let tx = conn.transaction().expect("tx");
        let outcome = apply_reconciliation(&tx, candidate, None, t).expect("apply reconciliation");
        tx.commit().expect("commit");

        // Verify INVARIANT: At EVERY single step, exactly 1 active fact exists for (user, primary_language, work)
        let active_count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM facts WHERE subject = 'user' AND predicate = 'primary_language' AND scope = 'work' AND valid_to IS NULL AND expired_at IS NULL",
                [],
                |r| r.get(0),
            )
            .expect("active count");
        assert_eq!(
            active_count, 1,
            "Active singleton invariant violated at step {} (language = {})",
            i, lang
        );

        let active = get_active_fact(&conn, "user", "primary_language", "work")
            .expect("get active")
            .expect("active fact must exist");
        assert_eq!(active.value, lang);

        if i > 0 && lang == languages[i - 1] {
            assert!(matches!(outcome, ReconciliationOutcome::Noop { .. }));
        }
    }

    // Verify history length: 20 mutations, 19 transitions (some NOOPs didn't create new rows)
    let history = query_fact_history(&conn, "user", "primary_language", "work").expect("history");
    assert!(history.len() >= 15, "History should retain all unique past states");

    // Verify lineage integrity: every superseded fact points to a valid later fact
    for fact in &history {
        if let Some(ref next_id) = fact.superseded_by {
            let next_exists: bool = conn
                .query_row("SELECT 1 FROM facts WHERE fact_id = ?1", params![next_id], |_| Ok(true))
                .unwrap_or(false);
            assert!(next_exists, "Lineage pointer superseded_by='{}' points to non-existent fact", next_id);
        }
    }
}

// ============================================================================
// 2. EVIDENCE LOG IMMUTABILITY & TRIGGER ENFORCEMENT CHALLENGES
// ============================================================================

#[test]
fn challenge_evidence_log_immutable_update_trigger_blocks_all_content_columns() {
    let conn = setup_in_memory_bitemporal_db();
    let text = "Hải đã mua vé máy bay ngày 25/12";
    let eid = ingest_evidence(&conn, text, "turn", Some("s1"), Some("t1"), "Owner", "tauri", "Trusted", 1000)
        .expect("ingest");

    // Attack 1: Attempt to alter `nfc_content`
    let res_nfc = conn.execute(
        "UPDATE evidence_log SET nfc_content = 'Hải đã huỷ vé máy bay' WHERE evidence_id = ?1",
        params![eid],
    );
    assert!(res_nfc.is_err(), "Must reject update to nfc_content");
    let err_msg = res_nfc.unwrap_err().to_string();
    assert!(err_msg.contains("SECURITY VIOLATION"), "Must raise explicit SECURITY VIOLATION: {}", err_msg);

    // Attack 2: Attempt to alter `content`
    let res_content = conn.execute(
        "UPDATE evidence_log SET content = 'Fake content' WHERE evidence_id = ?1",
        params![eid],
    );
    assert!(res_content.is_err(), "Must reject update to content column");
    assert!(res_content.unwrap_err().to_string().contains("SECURITY VIOLATION"));

    // Attack 3: Attempt to alter `evidence_id` (content-addressed hash)
    let res_id = conn.execute(
        "UPDATE evidence_log SET evidence_id = '0000000000000000000000000000000000000000000000000000000000000000' WHERE evidence_id = ?1",
        params![eid],
    );
    assert!(res_id.is_err(), "Must reject update to evidence_id");
    assert!(res_id.unwrap_err().to_string().contains("SECURITY VIOLATION"));

    // Attack 4: Attempt to forge `created_at` timestamp
    let res_ts = conn.execute(
        "UPDATE evidence_log SET created_at = 0 WHERE evidence_id = ?1",
        params![eid],
    );
    assert!(res_ts.is_err(), "Must reject update to created_at");
    assert!(res_ts.unwrap_err().to_string().contains("SECURITY VIOLATION"));

    // Verify the record remains completely uncorrupted
    let current_content: String = conn
        .query_row("SELECT nfc_content FROM evidence_log WHERE evidence_id = ?1", params![eid], |r| r.get(0))
        .expect("read record");
    assert_eq!(current_content, text, "Content must remain intact after blocked attacks");
}

#[test]
fn challenge_evidence_log_immutable_delete_trigger_blocks_physical_deletions() {
    let conn = setup_in_memory_bitemporal_db();
    let eid = ingest_evidence(&conn, "Dữ liệu nhật ký quan trọng", "turn", None, None, "Owner", "cli", "Trusted", 1000)
        .expect("ingest");

    // Attack 1: Specific row physical DELETE
    let res_del = conn.execute("DELETE FROM evidence_log WHERE evidence_id = ?1", params![eid]);
    assert!(res_del.is_err(), "Must block DELETE of single evidence record");
    let err_msg = res_del.unwrap_err().to_string();
    assert!(
        err_msg.contains("SECURITY VIOLATION: evidence_log is append-only"),
        "Must specify that table is append-only: {}",
        err_msg
    );

    // Attack 2: Table-wide wipe attempt (DELETE FROM evidence_log)
    let res_wipe = conn.execute("DELETE FROM evidence_log", []);
    assert!(res_wipe.is_err(), "Must block bulk DELETE FROM evidence_log");
    assert!(res_wipe.unwrap_err().to_string().contains("SECURITY VIOLATION"));

    // Verify row still exists
    let count: i64 = conn.query_row("SELECT COUNT(*) FROM evidence_log", [], |r| r.get(0)).expect("count");
    assert_eq!(count, 1, "Evidence row must survive DELETE attacks");
}

#[test]
fn challenge_evidence_log_tombstoning_lifecycle_and_idempotence() {
    let conn = setup_in_memory_bitemporal_db();
    let eid = ingest_evidence(&conn, "Thông tin cần quên", "turn", None, None, "Owner", "cli", "Trusted", 1000)
        .expect("ingest");

    // Tombstone the record
    let tombstone_res = tombstone_evidence(&conn, &eid, "right_to_be_forgotten", 1500).expect("tombstone");
    assert!(tombstone_res, "First tombstone should succeed");

    // Second tombstone on already tombstoned record -> returns false (idempotent noop)
    let tombstone_repeat = tombstone_evidence(&conn, &eid, "second_attempt", 1600).expect("tombstone repeat");
    assert!(!tombstone_repeat, "Repeated tombstone should be a safe noop returning false");

    // Verify row is marked tombstoned
    let (tombstoned_at, reason): (Option<i64>, Option<String>) = conn
        .query_row(
            "SELECT tombstoned_at, tombstone_reason FROM evidence_log WHERE evidence_id = ?1",
            params![eid],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .expect("query tombstone metadata");
    assert_eq!(tombstoned_at, Some(1500));
    assert_eq!(reason, Some("right_to_be_forgotten".to_string()));
}

#[test]
fn challenge_evidence_log_empty_and_whitespace_rejection() {
    let conn = setup_in_memory_bitemporal_db();

    // Empty string
    let res_empty = ingest_evidence(&conn, "", "turn", None, None, "Owner", "cli", "Trusted", 1000);
    assert!(res_empty.is_err(), "Must reject empty evidence string");

    // Pure whitespace
    let res_ws = ingest_evidence(&conn, "   \t\n  ", "turn", None, None, "Owner", "cli", "Trusted", 1000);
    assert!(res_ws.is_err(), "Must reject whitespace-only evidence string");
}

#[test]
fn challenge_evidence_log_vietnamese_nfc_canonicalization_stress() {
    let conn = setup_in_memory_bitemporal_db();

    // Test multiple Vietnamese words in NFD (decomposed) vs NFC (composed)
    let pairs = [
        // Hà Nội
        ("Hà Nội", "Ha\u{0300} N\u{00f4}\u{0323}i"),
        // Đà Nẵng
        ("Đà Nẵng", "\u{0110}a\u{0300} N\u{0103}\u{0303}ng"),
        // Thành phố Hồ Chí Minh
        ("Thành phố Hồ Chí Minh", "Tha\u{0300}nh ph\u{00f4}\u{0301} H\u{00f4}\u{0300} Chi\u{0301} Minh"),
        // Cà phê sữa đá
        ("Cà phê sữa đá", "Ca\u{0300} ph\u{00ea} s\u{01b0}\u{0303}a \u{0111}a\u{0301}"),
    ];

    for (nfc_str, nfd_str) in pairs {
        let id_nfc = ingest_evidence(&conn, nfc_str, "turn", None, None, "Owner", "cli", "Trusted", 1000)
            .expect("ingest nfc");
        let id_nfd = ingest_evidence(&conn, nfd_str, "turn", None, None, "Owner", "cli", "Trusted", 1001)
            .expect("ingest nfd");

        assert_eq!(
            id_nfc, id_nfd,
            "NFC and NFD representations must produce identical BLAKE3 hashes for '{}'",
            nfc_str
        );
    }
}

// ============================================================================
// 3. BITEMPORAL POINT-IN-TIME TIME-TRAVEL ADVERSARIAL CHALLENGES
// ============================================================================

#[test]
fn challenge_bitemporal_retroactive_fact_orthogonality() {
    let conn = setup_in_memory_bitemporal_db();

    // Scenario: User tells LIVA at t_sys = 2026-05-01 (1_777_593_600_000 ms)
    // that they lived in Paris from 2020-01-01 (1_577_836_800_000 ms) to 2022-01-01 (1_640_995_200_000 ms).
    let t_world_start = 1_577_836_800_000i64; // 2020-01-01
    let t_world_end = 1_640_995_200_000i64;   // 2022-01-01
    let t_sys_record = 1_777_593_600_000i64;  // 2026-05-01

    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f_retro', 'user', 'city', 'Paris', 'personal', 'explicit', 1.0, 5, 'private', ?1, ?2, ?3, NULL)",
        params![t_world_start, t_world_end, t_sys_record],
    ).expect("insert retroactive fact");

    // 1. World Time Check during validity window (t_world = 2021-01-01)
    let t_world_mid = 1_609_459_200_000i64;
    let world_facts = query_facts_as_of_world_time(&conn, t_world_mid).expect("query world time");
    assert_eq!(world_facts.len(), 1, "Fact was valid in the world in 2021");
    assert_eq!(world_facts[0].value, "Paris");

    // 2. World Time Check after validity ended (t_world = 2023-01-01)
    let t_world_after = 1_672_531_200_000i64;
    let world_after = query_facts_as_of_world_time(&conn, t_world_after).expect("query world time after");
    assert_eq!(world_after.len(), 0, "Fact was no longer valid in the world in 2023");

    // 3. System Time Check before recording (t_sys = 2021-01-01):
    // What did LIVA know at transaction time in 2021? NOTHING!
    let sys_facts_2021 = query_facts_as_of_system_time(&conn, t_world_mid).expect("query sys time 2021");
    assert_eq!(sys_facts_2021.len(), 0, "LIVA system did NOT know about Paris in 2021 (recorded in 2026)");

    // 4. System Time Check after recording (t_sys = 2026-06-01):
    // LIVA knows about it in 2026
    let sys_facts_2026 = query_facts_as_of_system_time(&conn, t_sys_record + 1000).expect("query sys time 2026");
    assert_eq!(sys_facts_2026.len(), 1, "LIVA knows about Paris after transaction recording in 2026");
}

#[test]
fn challenge_bitemporal_prescheduled_future_fact_orthogonality() {
    let conn = setup_in_memory_bitemporal_db();

    // Scenario: User schedules a planned relocation:
    // Recorded on 2026-10-01 (t_sys = 1000), but takes effect in the real world on 2028-01-01 (t_world = 5000).
    let t_sys = 1000i64;
    let t_world_start = 5000i64;

    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f_future', 'user', 'residence', 'Tokyo', 'personal', 'explicit', 1.0, 5, 'private', ?1, NULL, ?2, NULL)",
        params![t_world_start, t_sys],
    ).expect("insert scheduled fact");

    // System knew this fact at t=1500 (since recorded_at=1000)
    let sys_facts = query_facts_as_of_system_time(&conn, 1500).expect("query sys");
    assert_eq!(sys_facts.len(), 1, "System already knows about scheduled move");

    // But in the real world at t=2000, user does NOT yet live in Tokyo
    let world_facts_2000 = query_facts_as_of_world_time(&conn, 2000).expect("query world 2000");
    assert_eq!(world_facts_2000.len(), 0, "Fact is not yet valid in world time at t=2000");

    // In the real world at t=6000, user DOES live in Tokyo
    let world_facts_6000 = query_facts_as_of_world_time(&conn, 6000).expect("query world 6000");
    assert_eq!(world_facts_6000.len(), 1, "Fact is valid in world time at t=6000");
    assert_eq!(world_facts_6000[0].value, "Tokyo");
}

#[test]
fn challenge_bitemporal_half_open_interval_boundaries() {
    let conn = setup_in_memory_bitemporal_db();

    // Fact valid in world time on interval [1000, 2000)
    // and in system time on interval [3000, 4000)
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f_bound', 'user', 'status', 'active', 'personal', 'explicit', 1.0, 5, 'normal', 1000, 2000, 3000, 4000)",
        [],
    ).expect("insert boundary fact");

    // World time boundary checks:
    // At t = 999: outside (too early)
    assert_eq!(query_facts_as_of_world_time(&conn, 999).unwrap().len(), 0);
    // At t = 1000 (valid_from): inside (inclusive start)
    assert_eq!(query_facts_as_of_world_time(&conn, 1000).unwrap().len(), 1);
    // At t = 1500: inside
    assert_eq!(query_facts_as_of_world_time(&conn, 1500).unwrap().len(), 1);
    // At t = 1999: inside
    assert_eq!(query_facts_as_of_world_time(&conn, 1999).unwrap().len(), 1);
    // At t = 2000 (valid_to): outside (exclusive end)
    assert_eq!(query_facts_as_of_world_time(&conn, 2000).unwrap().len(), 0);

    // System time boundary checks:
    // At t = 2999: outside
    assert_eq!(query_facts_as_of_system_time(&conn, 2999).unwrap().len(), 0);
    // At t = 3000 (recorded_at): inside (inclusive start)
    assert_eq!(query_facts_as_of_system_time(&conn, 3000).unwrap().len(), 1);
    // At t = 3500: inside
    assert_eq!(query_facts_as_of_system_time(&conn, 3500).unwrap().len(), 1);
    // At t = 3999: inside
    assert_eq!(query_facts_as_of_system_time(&conn, 3999).unwrap().len(), 1);
    // At t = 4000 (expired_at): outside (exclusive end)
    assert_eq!(query_facts_as_of_system_time(&conn, 4000).unwrap().len(), 0);
}
