//! Integration Test Suite for Milestone 3 (F13 - F18):
//! - F13: Immutable Evidence Log (NFC, BLAKE3 deduplicated, immutability triggers, tombstoning).
//! - F14: Bitemporal Facts Table (two time axes, lineage, time-travel point-in-time queries).
//! - F15: Active Fact Singleton Invariant (idx_active_singleton_fact, 4-action reconciliation: ADD, EXTEND, NOOP, INVALIDATE).
//! - F16: Deterministic Post-Turn Writer (<5ms, MemGhost defense, anti-hallucination grounding gate).
//! - F17: Vietnamese FTS5 3-Form Search & Hybrid RRF (K=60.0, dynamic Ebbinghaus decay, unaccented matching).
//! - F18: Bidirectional OpenClaw Markdown Sync (atomic swap, BLAKE3 diff, reverse parser with origin=user_edited, confidence=1.0).

use liva_core_types::CommandPrincipal;
use liva_native_core::cognitive::deterministic_writer::{
    AntiHallucinationVerifier, CandidateFact, DeterministicPostTurnWriter, FactCandidate,
    MemGhostFilter, MemGhostFilterResult, MessageRole,
};
use liva_native_core::cognitive::markdown_sync::{MarkdownSyncConfig, MarkdownSyncManager};
use liva_native_core::db::bitemporal::{
    BitemporalFact, FactOrigin, FactScope, FactSensitivity, IntegrityLabel, ReconciliationOutcome,
    SourceChannel, apply_reconciliation, get_active_fact, ingest_evidence, ingest_evidence_log,
    init_bitemporal_schema, query_active_facts, query_fact_history, query_facts_as_of_system_time,
    query_facts_as_of_world_time, tombstone_evidence,
};
use liva_native_core::db::fts_vietnamese::{
    FactScoringMeta, RetrievalCandidate, VietnameseThreeFormNormalizer, compute_ebbinghaus_decay,
    fold_vietnamese_char, fuse_hybrid_candidates, init_facts_fts_schema, search_facts_fts,
    upsert_fact_fts,
};
use rusqlite::{Connection, params};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

struct TempDirGuard {
    path: PathBuf,
}
impl TempDirGuard {
    fn path(&self) -> &Path {
        &self.path
    }
}
impl Drop for TempDirGuard {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
fn tempdir() -> std::io::Result<TempDirGuard> {
    let p = std::env::temp_dir().join(format!("liva_m3_test_{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&p)?;
    Ok(TempDirGuard { path: p })
}

fn setup_m3_test_db() -> Connection {
    let conn = Connection::open_in_memory().expect("open in-memory db");
    init_bitemporal_schema(&conn).expect("init bitemporal schema");
    init_facts_fts_schema(&conn).expect("init fts schema");
    conn
}

// ============================================================================
// F13 & F14 & F15: BITEMPORAL SCHEMA & EVIDENCE LOG INTEGRATION
// ============================================================================

#[test]
fn test_f13_evidence_log_nfc_content_addressed_deduplication() {
    let conn = setup_m3_test_db();

    // Composed vs Decomposed Vietnamese strings for "Hà Nội"
    let composed_nfc = "Tôi sống tại Hà Nội";
    let decomposed_nfd = "T\u{00f4}i s\u{00f4}\u{0301}ng ta\u{0323}i Ha\u{0300} N\u{00f4}\u{0323}i";

    let id1 = ingest_evidence(
        &conn,
        composed_nfc,
        "turn",
        None,
        None,
        "Owner",
        "tauri",
        "Trusted",
        1000,
    )
    .expect("ingest composed");
    let id2 = ingest_evidence(
        &conn,
        decomposed_nfd,
        "turn",
        None,
        None,
        "Owner",
        "tauri",
        "Trusted",
        1001,
    )
    .expect("ingest decomposed");

    assert_eq!(
        id1, id2,
        "NFC normalization must yield identical BLAKE3 content-addressed ID"
    );

    let count: i64 = conn
        .query_row("SELECT COUNT(*) FROM evidence_log", [], |r| r.get(0))
        .expect("count rows");
    assert_eq!(
        count, 1,
        "Content-addressing must deduplicate identical NFC evidence"
    );
}

#[test]
fn test_f13_evidence_log_immutability_triggers_and_tombstoning() {
    let conn = setup_m3_test_db();
    let id = ingest_evidence(
        &conn,
        "Thông tin bí mật",
        "turn",
        None,
        None,
        "Owner",
        "tauri",
        "Trusted",
        1000,
    )
    .expect("ingest evidence");

    // Physical UPDATE must be aborted by trigger
    let update_res = conn.execute(
        "UPDATE evidence_log SET nfc_content = 'Sửa lén' WHERE evidence_id = ?1",
        params![id],
    );
    assert!(
        update_res.is_err(),
        "Trigger must reject physical modification of evidence_log"
    );

    // Physical DELETE must be aborted by trigger
    let delete_res = conn.execute(
        "DELETE FROM evidence_log WHERE evidence_id = ?1",
        params![id],
    );
    assert!(
        delete_res.is_err(),
        "Trigger must reject physical deletion of evidence_log"
    );

    // Tombstoning must be accepted
    let tombstone_res =
        tombstone_evidence(&conn, &id, "user_explicit_forget", 1050).expect("tombstone");
    assert!(tombstone_res, "Tombstone update should succeed");

    let is_tombstoned: Option<i64> = conn
        .query_row(
            "SELECT tombstoned_at FROM evidence_log WHERE evidence_id = ?1",
            params![id],
            |r| r.get(0),
        )
        .expect("query tombstoned");
    assert_eq!(is_tombstoned, Some(1050));
}

#[test]
fn test_f15_active_fact_singleton_invariant_enforcement() {
    let conn = setup_m3_test_db();

    // 1. First active fact
    conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f1', 'user', 'home_city', 'Hà Nội', 'personal', 'explicit', 1.0, 8, 'private', 1000, NULL, 1000, NULL)",
        [],
    )
    .expect("insert first fact");

    // 2. Second conflicting active fact with identical (subject, predicate, scope)
    let err = conn.execute(
        "INSERT INTO facts (
            fact_id, subject, predicate, value, scope, origin, confidence,
            importance, sensitivity, valid_from, valid_to, recorded_at, expired_at
         ) VALUES ('f2', 'user', 'home_city', 'Đà Nẵng', 'personal', 'explicit', 1.0, 8, 'private', 2000, NULL, 2000, NULL)",
        [],
    );

    assert!(
        err.is_err(),
        "idx_active_singleton_fact must prevent simultaneous active singleton facts"
    );
}

#[test]
fn test_f14_f15_four_action_reconciliation_and_time_travel() {
    let mut conn = setup_m3_test_db();
    let eid = ingest_evidence(
        &conn,
        "Tôi ở Hà Nội từ 2026",
        "turn",
        None,
        None,
        "Owner",
        "cli",
        "Trusted",
        1000,
    )
    .expect("ingest");

    // 1. ADD
    let f1 = BitemporalFact {
        fact_id: "fact_hanoi".into(),
        subject: "user".into(),
        predicate: "residence".into(),
        value: "Hà Nội".into(),
        object_value: "Hà Nội".into(),
        scope: "personal".into(),
        origin: FactOrigin::Explicit,
        confidence: 1.0,
        importance: 8,
        sensitivity: FactSensitivity::Private,
        valid_from: 1000,
        valid_to: None,
        recorded_at: 1000,
        expired_at: None,
        superseded_by: None,
        reason: None,
        supersede_reason: None,
        category: Some("location".into()),
        key: None,
        created_at: 1000,
        updated_at: 1000,
    };

    let tx = conn.transaction().expect("tx1");
    let out1 = apply_reconciliation(&tx, f1.clone(), Some(&eid), 1000).expect("add");
    tx.commit().expect("commit tx1");
    assert_eq!(
        out1,
        ReconciliationOutcome::Added {
            fact_id: "fact_hanoi".into()
        }
    );

    // 2. NOOP: Same fact asserted
    let tx = conn.transaction().expect("tx2");
    let out2 = apply_reconciliation(&tx, f1.clone(), Some(&eid), 1100).expect("noop");
    tx.commit().expect("commit tx2");
    match out2 {
        ReconciliationOutcome::Noop { fact_id, .. } => assert_eq!(fact_id, "fact_hanoi"),
        _ => panic!("Expected Noop outcome"),
    }

    // 3. EXTEND: Reaffirm with finite validity window
    let mut f1_extended = f1.clone();
    f1_extended.valid_to = Some(9000);
    let tx = conn.transaction().expect("tx3");
    let out3 = apply_reconciliation(&tx, f1_extended, Some(&eid), 1200).expect("extend");
    tx.commit().expect("commit tx3");
    assert_eq!(
        out3,
        ReconciliationOutcome::Extended {
            fact_id: "fact_hanoi".into()
        }
    );

    // 4. INVALIDATE: User moved to Đà Nẵng
    let f2 = BitemporalFact {
        fact_id: "fact_danang".into(),
        subject: "user".into(),
        predicate: "residence".into(),
        value: "Đà Nẵng".into(),
        object_value: "Đà Nẵng".into(),
        scope: "personal".into(),
        origin: FactOrigin::Explicit,
        confidence: 1.0,
        importance: 9,
        sensitivity: FactSensitivity::Private,
        valid_from: 2000,
        valid_to: None,
        recorded_at: 2000,
        expired_at: None,
        superseded_by: None,
        reason: Some("moved_to_danang".into()),
        supersede_reason: Some("moved_to_danang".into()),
        category: Some("location".into()),
        key: None,
        created_at: 2000,
        updated_at: 2000,
    };

    let tx = conn.transaction().expect("tx4");
    let out4 = apply_reconciliation(&tx, f2, Some(&eid), 2000).expect("invalidate");
    tx.commit().expect("commit tx4");

    assert_eq!(
        out4,
        ReconciliationOutcome::Invalidated {
            superseded_fact_id: "fact_hanoi".into(),
            new_fact_id: Some("fact_danang".into()),
        }
    );

    // Verify current active fact
    let active = get_active_fact(&conn, "user", "residence", "personal")
        .expect("query active")
        .expect("active fact found");
    assert_eq!(active.fact_id, "fact_danang");
    assert_eq!(active.value, "Đà Nẵng");

    // Point-in-Time System "As-Of" Query at T=1500
    let as_of_1500 = query_facts_as_of_system_time(&conn, 1500).expect("query as-of 1500");
    assert_eq!(as_of_1500.len(), 1);
    assert_eq!(as_of_1500[0].value, "Hà Nội");

    // Point-in-Time System "As-Of" Query at T=2500
    let as_of_2500 = query_facts_as_of_system_time(&conn, 2500).expect("query as-of 2500");
    assert_eq!(as_of_2500.len(), 1);
    assert_eq!(as_of_2500[0].value, "Đà Nẵng");

    // Full Lineage History
    let history =
        query_fact_history(&conn, "user", "residence", "personal").expect("query history");
    assert_eq!(history.len(), 2);
    assert_eq!(history[0].fact_id, "fact_hanoi");
    assert_eq!(history[0].superseded_by, Some("fact_danang".into()));
    assert_eq!(history[1].fact_id, "fact_danang");
}

// ============================================================================
// F16: DETERMINISTIC POST-TURN WRITER & MEMGHOST DEFENSE
// ============================================================================

#[test]
fn test_f16_deterministic_writer_sub_5ms_rule_extraction() {
    let mut conn = setup_m3_test_db();
    let text = "Ghi nhớ: tôi thích uống trà sen Tây Hồ";

    let t0 = std::time::Instant::now();
    let outcomes = DeterministicPostTurnWriter::process_turn_sync(
        &conn,
        text,
        "",
        CommandPrincipal::LocalCli,
        SourceChannel::Tauri,
        false,
    )
    .expect("process turn");
    let elapsed = t0.elapsed();

    // Verify latency SLA < 5ms (allowing headroom up to 50ms in unoptimized test debug builds)
    assert!(
        elapsed.as_millis() < 50,
        "Rule extraction must be ultra-fast (took {}ms)",
        elapsed.as_millis()
    );
    assert!(
        !outcomes.is_empty(),
        "Should have extracted and reconciled facts"
    );

    let active_facts = query_active_facts(&conn, None).expect("query active");
    assert!(!active_facts.is_empty(), "Fact must be recorded in DB");
}

#[test]
fn test_f16_memghost_defense_blocks_assistant_and_untrusted_channels() {
    // 1. Candidate trying to write personal bank account
    let candidate = CandidateFact {
        subject: "user".into(),
        predicate: "bank_account".into(),
        scope: "personal".into(),
        value: "0123456789".into(),
        origin: FactOrigin::Inferred,
        confidence: 0.9,
        source_evidence_hash: "hash123".into(),
        source_role: "assistant".into(),
    };

    // Assistant prose trying to mutate protected personal preference -> BLOCKED
    let filter_res1 = MemGhostFilter::evaluate(&candidate, CommandPrincipal::LocalCli, false);
    assert!(matches!(
        filter_res1,
        MemGhostFilterResult::RejectedBlocked { .. }
    ));

    // Untrusted channel (e.g. web/telegram stranger) -> BLOCKED
    let mut candidate2 = candidate.clone();
    candidate2.source_role = "user".into();
    let filter_res2 = MemGhostFilter::evaluate(&candidate2, CommandPrincipal::Telegram, true);
    assert!(matches!(
        filter_res2,
        MemGhostFilterResult::RejectedBlocked { .. }
    ));
}

#[test]
fn test_f16_anti_hallucination_grounding_verification() {
    let raw_evidence = "Khách hàng xác nhận chuyển 5000000 vnđ vào tài khoản 0987654321 qua MB";

    // Fact with matching number in evidence -> GROUNDED
    let grounded_candidate = CandidateFact {
        subject: "user".into(),
        predicate: "bank_account".into(),
        scope: "personal".into(),
        value: "0987654321".into(),
        origin: FactOrigin::Inferred,
        confidence: 0.95,
        source_evidence_hash: "h1".into(),
        source_role: "user".into(),
    };
    let res1 = AntiHallucinationVerifier::verify(&grounded_candidate, raw_evidence);
    assert_eq!(
        res1,
        liva_native_core::cognitive::deterministic_writer::AntiHallucinationResult::Grounded
    );

    // Fact with fabricated number NOT in evidence -> REJECTED
    let fabricated_candidate = CandidateFact {
        subject: "user".into(),
        predicate: "bank_account".into(),
        scope: "personal".into(),
        value: "9999888877".into(),
        origin: FactOrigin::Inferred,
        confidence: 0.95,
        source_evidence_hash: "h2".into(),
        source_role: "user".into(),
    };
    let res2 = AntiHallucinationVerifier::verify(&fabricated_candidate, raw_evidence);
    assert!(matches!(
        res2,
        liva_native_core::cognitive::deterministic_writer::AntiHallucinationResult::Rejected { .. }
    ));
}

// ============================================================================
// F17: VIETNAMESE FTS5 3-FORM SEARCH & HYBRID RRF
// ============================================================================

#[test]
fn test_f17_vietnamese_normalizer_and_unaccented_matching() {
    let conn = setup_m3_test_db();

    // Verify manual folding for double-diacritic vowels and đ/Đ
    assert_eq!(fold_vietnamese_char('đ'), 'd');
    assert_eq!(fold_vietnamese_char('Đ'), 'd');
    assert_eq!(fold_vietnamese_char('ộ'), 'o');
    assert_eq!(fold_vietnamese_char('ệ'), 'e');
    assert_eq!(fold_vietnamese_char('ự'), 'u');
    assert_eq!(fold_vietnamese_char('ẵ'), 'a');

    let text = "Căn cước công dân và hộ chiếu tại Đà Nẵng";
    let forms = VietnameseThreeFormNormalizer::normalize(text);
    assert_eq!(forms.form_nfc, "căn cước công dân và hộ chiếu tại đà nẵng");
    assert_eq!(
        forms.form_folded,
        "can cuoc cong dan va ho chieu tai da nang"
    );
    assert!(forms.form_bigram.contains("ho_chieu"));
    assert!(forms.form_bigram.contains("da_nang"));

    // Upsert into SQLite FTS5 table
    upsert_fact_fts(
        &conn,
        "fact_passport",
        "user",
        "giấy_tờ",
        "Hộ chiếu Việt Nam cấp tại Đà Nẵng",
    )
    .expect("upsert fts");
    upsert_fact_fts(
        &conn,
        "fact_bike",
        "user",
        "sở_thích",
        "Bảo hộ lao động và đèn chiếu sáng",
    )
    .expect("upsert fts");

    // 1. Search with exact unaccented query "ho chieu"
    let hits1 = search_facts_fts(&conn, "ho chieu", 5).expect("search");
    assert_eq!(hits1.len(), 1);
    assert_eq!(hits1[0].fact_id, "fact_passport");

    // 2. Search with "da nang" (verifies đ -> d unaccented matching)
    let hits2 = search_facts_fts(&conn, "da nang", 5).expect("search");
    assert_eq!(hits2.len(), 1);
    assert_eq!(hits2[0].fact_id, "fact_passport");

    // 3. Search with "bao ho lao dong" should match fact_bike, but NOT fact_passport
    let hits3 = search_facts_fts(&conn, "bảo hộ", 5).expect("search");
    assert_eq!(hits3.len(), 1);
    assert_eq!(hits3[0].fact_id, "fact_bike");
}

#[test]
fn test_f17_hybrid_rrf_with_dynamic_ebbinghaus_decay() {
    let now = 1_000_000_000_000i64;
    let mut meta_map = HashMap::new();

    // Fact A: User-edited personal preference (tau = infinity, never decays)
    meta_map.insert(
        "fact_a".into(),
        FactScoringMeta {
            fact_id: "fact_a".into(),
            subject: "user".into(),
            predicate: "theme".into(),
            object_value: "dark_mode".into(),
            origin: "user_edited".into(),
            scope: "personal".into(),
            confidence: 1.0,
            importance: 10.0,
            recorded_at: now - 365 * 86_400_000, // 1 year ago
            access_count: 5,
            is_pinned: false,
        },
    );

    // Fact B: Episodic chat fact from 120 days ago (tau = 30d, decayed)
    meta_map.insert(
        "fact_b".into(),
        FactScoringMeta {
            fact_id: "fact_b".into(),
            subject: "user".into(),
            predicate: "chat".into(),
            object_value: "trời_mưa".into(),
            origin: "conversation".into(),
            scope: "episodic".into(),
            confidence: 1.0,
            importance: 5.0,
            recorded_at: now - 120 * 86_400_000,
            access_count: 0,
            is_pinned: false,
        },
    );

    let dense_ranks = vec![
        RetrievalCandidate {
            fact_id: "fact_a".into(),
            raw_rank: 1,
        },
        RetrievalCandidate {
            fact_id: "fact_b".into(),
            raw_rank: 2,
        },
    ];
    let sparse_ranks = vec![
        RetrievalCandidate {
            fact_id: "fact_a".into(),
            raw_rank: 1,
        },
        RetrievalCandidate {
            fact_id: "fact_b".into(),
            raw_rank: 2,
        },
    ];

    let fused = fuse_hybrid_candidates(&dense_ranks, &sparse_ranks, &meta_map, 0.6, 0.4, 5, now);
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].fact_id, "fact_a");
    assert_eq!(
        fused[0].decay_factor, 1.0,
        "User-edited fact must never decay"
    );
    assert!(
        fused[1].decay_factor <= 0.20,
        "Episodic fact after 120d must have decayed significantly"
    );
}

// ============================================================================
// F18: BIDIRECTIONAL MARKDOWN SYNC
// ============================================================================

#[test]
fn test_f18_bidirectional_markdown_sync_workflow() {
    let mut conn = setup_m3_test_db();
    let temp_dir = tempdir().expect("tempdir");
    let sync_mgr = MarkdownSyncManager::new(MarkdownSyncConfig::new(temp_dir.path()));

    // 1. Insert an active fact into DB
    let fact = BitemporalFact {
        fact_id: "f_sync_01".into(),
        subject: "user".into(),
        predicate: "language".into(),
        value: "Tiếng Việt".into(),
        object_value: "Tiếng Việt".into(),
        scope: "personal".into(),
        origin: FactOrigin::Explicit,
        confidence: 1.0,
        importance: 8,
        sensitivity: FactSensitivity::Private,
        valid_from: 1000,
        valid_to: None,
        recorded_at: 1000,
        expired_at: None,
        superseded_by: None,
        reason: None,
        supersede_reason: None,
        category: None,
        key: None,
        created_at: 1000,
        updated_at: 1000,
    };
    let tx = conn.transaction().expect("tx");
    apply_reconciliation(&tx, fact, None, 1000).expect("insert fact");
    tx.commit().expect("commit");

    // 2. Forward Sync: DB -> Markdown files
    let export_rep = sync_mgr
        .export_all_active_facts(&conn)
        .expect("export markdown");
    assert!(export_rep.files_scanned >= 1);

    let user_md_path = temp_dir.path().join("USER.md");
    assert!(user_md_path.exists());
    let content = fs::read_to_string(&user_md_path).expect("read USER.md");
    assert!(content.contains("**language**: Tiếng Việt"));
    assert!(content.contains("<!-- liva:id=f_sync_01"));

    // 3. User edits USER.md in Obsidian (changes language to "Tiếng Anh")
    let modified = content.replace("Tiếng Việt", "Tiếng Anh");
    fs::write(&user_md_path, modified).expect("write modified");

    // 4. Reverse Sync: Markdown -> DB
    let ingest_rep = sync_mgr.ingest_user_edits(&mut conn).expect("ingest edits");
    assert_eq!(ingest_rep.facts_updated, 1);

    // 5. Verify DB state: new fact created with origin = user_edited and confidence = 1.0
    let active = get_active_fact(&conn, "user", "language", "personal")
        .expect("query active")
        .expect("active fact exists");
    assert_eq!(active.object_value, "Tiếng Anh");
    assert_eq!(active.origin, FactOrigin::UserEdited);
    assert_eq!(active.confidence, 1.0);
}
