use liva_native_core::cognitive::{
    CognitiveFact, ConflictResolutionAction, FactUpsertOutcome, MemoryConflictRecord,
    MemoryDeleteCoordinator, MemoryProvenance,
};
use liva_native_core::db;
use liva_native_core::{DatabasePool, EncryptionEngine};
use rusqlite::params;

#[test]
fn test_memory_provenance_and_cognitive_fact_contracts() {
    let prov = MemoryProvenance::new("conversation", 0.95)
        .with_source_event("evt_percept_101")
        .with_model("qwen3-4b-instruct")
        .with_excerpt("User stated: 'I prefer dark mode in IDE and terminal'")
        .with_verified(true)
        .with_owner_domain("memory_owner:local");

    let fact = CognitiveFact::new(
        "ui_theme_preference",
        "user",
        "prefers_theme",
        "dark",
        prov.clone(),
    )
    .with_category("preference")
    .with_importance(0.8)
    .with_effective_interval(1000, Some(5000));

    // 1. Serialization roundtrip
    let json_str = serde_json::to_string(&fact).expect("serialize fact");
    let deserialized: CognitiveFact = serde_json::from_str(&json_str).expect("deserialize fact");
    assert_eq!(fact, deserialized);
    assert_eq!(deserialized.provenance.confidence_score, 0.95);
    assert!(deserialized.provenance.verified_by_user);
    assert_eq!(deserialized.provenance.agent_id, "liva_core");
    assert_eq!(
        deserialized.provenance.model_id.as_deref(),
        Some("qwen3-4b-instruct")
    );

    // 2. Temporal validity checks
    assert!(!fact.is_temporally_valid(500), "Before effective_from_ms");
    assert!(fact.is_temporally_valid(1000), "At effective_from_ms");
    assert!(fact.is_temporally_valid(3000), "Within valid interval");
    assert!(!fact.is_temporally_valid(5000), "At effective_until_ms");
    assert!(!fact.is_temporally_valid(6000), "After effective_until_ms");
}

#[test]
fn test_conflict_staging_and_manual_queue_operations() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("cognitive-memory-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");

    let conflict = MemoryConflictRecord {
        conflict_id: "conf_001".to_string(),
        fact_key: "work_location".to_string(),
        domain: "memory_owner:local".to_string(),
        existing_value: "Hanoi Office".to_string(),
        proposed_value: "Da Nang Remote".to_string(),
        source_event_id: Some("evt_9988".to_string()),
        conflict_type: "contradiction".to_string(),
        resolution_status: "pending".to_string(),
        created_at_ms: 1723789000000,
        resolved_at_ms: None,
    };

    // 1. Stage conflict to SQLite
    db::stage_memory_conflict(&conn, &crypto, &conflict).expect("stage conflict");

    // 2. Query pending conflicts
    let pending = db::get_pending_conflicts(&conn, &crypto, "memory_owner:local")
        .expect("get pending conflicts");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].conflict_id, "conf_001");
    assert_eq!(pending[0].existing_value, "Hanoi Office");
    assert_eq!(pending[0].proposed_value, "Da Nang Remote");
    assert_eq!(pending[0].resolution_status, "pending");

    // 3. Resolve conflict by accepting proposed value
    let resolved = db::resolve_memory_conflict(
        &conn,
        &crypto,
        "conf_001",
        ConflictResolutionAction::AcceptProposed,
    )
    .expect("resolve conflict");
    assert!(resolved);

    // 4. Verify conflict status is now resolved_superseded
    let pending_after =
        db::get_pending_conflicts(&conn, &crypto, "memory_owner:local").expect("get pending after");
    assert_eq!(pending_after.len(), 0);

    // 5. Verify facts table updated with proposed value
    let active_fact = db::get_fact(&conn, &crypto, "work_location")
        .expect("get fact")
        .expect("fact exists");
    assert_eq!(active_fact.value, "Da Nang Remote");

    // 6. Verify previous value archived in facts_history
    let history = db::get_fact_history(&conn, &crypto, "work_location", "memory_owner:local")
        .expect("get history");
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].old_value, "Hanoi Office");
    assert_eq!(history[0].reason, "conflict_resolved_accepted");
}

#[test]
fn test_conflict_resolution_keep_existing_and_custom_merge() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("cognitive-memory-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");

    // Case A: Keep Existing
    let conflict_a = MemoryConflictRecord {
        conflict_id: "conf_keep_01".to_string(),
        fact_key: "primary_ide".to_string(),
        domain: "memory_owner:local".to_string(),
        existing_value: "VSCode".to_string(),
        proposed_value: "Sublime Text".to_string(),
        source_event_id: None,
        conflict_type: "contradiction".to_string(),
        resolution_status: "pending".to_string(),
        created_at_ms: 1000,
        resolved_at_ms: None,
    };
    db::stage_memory_conflict(&conn, &crypto, &conflict_a).unwrap();

    let resolved_a = db::resolve_memory_conflict(
        &conn,
        &crypto,
        "conf_keep_01",
        ConflictResolutionAction::KeepExisting,
    )
    .unwrap();
    assert!(resolved_a);

    let status_a: String = conn
        .query_row(
            "SELECT resolution_status FROM memory_conflict_queue WHERE conflict_id = 'conf_keep_01'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(status_a, "resolved_rejected");

    // Case B: Custom Merge
    let conflict_b = MemoryConflictRecord {
        conflict_id: "conf_merge_02".to_string(),
        fact_key: "spoken_languages".to_string(),
        domain: "memory_owner:local".to_string(),
        existing_value: "Vietnamese, English".to_string(),
        proposed_value: "Japanese".to_string(),
        source_event_id: None,
        conflict_type: "semantic_divergence".to_string(),
        resolution_status: "pending".to_string(),
        created_at_ms: 2000,
        resolved_at_ms: None,
    };
    db::stage_memory_conflict(&conn, &crypto, &conflict_b).unwrap();

    let merged_val = "Vietnamese, English, Japanese".to_string();
    let resolved_b = db::resolve_memory_conflict(
        &conn,
        &crypto,
        "conf_merge_02",
        ConflictResolutionAction::MergeCustom(merged_val.clone()),
    )
    .unwrap();
    assert!(resolved_b);

    let active_merged = db::get_fact(&conn, &crypto, "spoken_languages")
        .unwrap()
        .unwrap();
    assert_eq!(active_merged.value, merged_val);

    let history_b =
        db::get_fact_history(&conn, &crypto, "spoken_languages", "memory_owner:local").unwrap();
    assert_eq!(history_b.len(), 1);
    assert_eq!(history_b[0].old_value, "Vietnamese, English");
    assert_eq!(history_b[0].reason, "conflict_resolved_merged");
}

#[test]
fn test_intelligent_fact_upsert_auto_archive_and_conflict_staging() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("cognitive-memory-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");

    let prov1 = MemoryProvenance::new("conversation", 0.9);
    let fact1 = CognitiveFact::new("favorite_food", "user", "likes", "Pho Bo", prov1);

    // 1. Fresh insert
    let outcome1 = db::upsert_cognitive_fact(&conn, &crypto, &fact1, true).unwrap();
    assert_eq!(outcome1, FactUpsertOutcome::Created);

    let saved1 = db::get_fact(&conn, &crypto, "favorite_food")
        .unwrap()
        .unwrap();
    assert_eq!(saved1.value, "Pho Bo");

    // 2. Conflict Staging when auto_archive = false
    let prov2 = MemoryProvenance::new("conversation", 0.8);
    let fact2 = CognitiveFact::new("favorite_food", "user", "likes", "Bun Cha", prov2);

    let outcome2 = db::upsert_cognitive_fact(&conn, &crypto, &fact2, false).unwrap();
    match outcome2 {
        FactUpsertOutcome::ConflictStaged { conflict_id } => {
            assert!(conflict_id.starts_with("conf_"));
            let pending = db::get_pending_conflicts(&conn, &crypto, "memory_owner:local").unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(pending[0].existing_value, "Pho Bo");
            assert_eq!(pending[0].proposed_value, "Bun Cha");
        }
        other => panic!("Expected ConflictStaged, got {:?}", other),
    }

    // Active fact must remain Pho Bo
    let current_fact = db::get_fact(&conn, &crypto, "favorite_food")
        .unwrap()
        .unwrap();
    assert_eq!(current_fact.value, "Pho Bo");

    // 3. Auto-archive superseding when auto_archive = true
    let prov3 = MemoryProvenance::new("direct_statement", 1.0);
    let fact3 = CognitiveFact::new("favorite_food", "user", "likes", "Banh Mi", prov3);

    let outcome3 = db::upsert_cognitive_fact(&conn, &crypto, &fact3, true).unwrap();
    match outcome3 {
        FactUpsertOutcome::Superseded { history_id } => {
            assert!(history_id.starts_with("hist_"));
        }
        other => panic!("Expected Superseded, got {:?}", other),
    }

    // Active fact must now be Banh Mi
    let current_fact2 = db::get_fact(&conn, &crypto, "favorite_food")
        .unwrap()
        .unwrap();
    assert_eq!(current_fact2.value, "Banh Mi");

    // History must record Pho Bo
    let history =
        db::get_fact_history(&conn, &crypto, "favorite_food", "memory_owner:local").unwrap();
    assert_eq!(history.len(), 1);
    assert_eq!(history[0].old_value, "Pho Bo");
    assert_eq!(history[0].reason, "superseded");
}

#[test]
fn test_cascading_fact_deletion_across_all_projections() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("cognitive-memory-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");

    let prov = MemoryProvenance::new("conversation", 1.0);
    let fact = CognitiveFact::new(
        "secret_project_codename",
        "project",
        "is",
        "Project LIVA",
        prov,
    );
    db::upsert_cognitive_fact(&conn, &crypto, &fact, true).unwrap();

    // Add facts_history record
    let fact_update = CognitiveFact::new(
        "secret_project_codename",
        "project",
        "is",
        "Project Phoenix",
        MemoryProvenance::new("conversation", 1.0),
    );
    db::upsert_cognitive_fact(&conn, &crypto, &fact_update, true).unwrap();

    // Add vector projections
    db::upsert_vector(
        &conn,
        &crypto,
        "vec_fact_secret_project_codename",
        "fact",
        "Fact: secret_project_codename Project Phoenix",
        &vec![0.05; db::MEMORY_VECTOR_DIM],
        Some("memory_owner:local"),
        Some("project"),
        None,
        None,
        None,
    )
    .unwrap();

    // Add L3 Knowledge Graph
    conn.execute(
        "INSERT INTO l3_nodes (id, label, properties) VALUES ('secret_project_codename', 'Fact', '{}')",
        [],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO l3_edges (source, target, relation) VALUES ('secret_project_codename', 'secret_project_codename', 'self')",
        [],
    )
    .unwrap();

    // Execute Cascading Fact Deletion
    let del_report =
        db::delete_fact_cascade(&conn, "secret_project_codename", "memory_owner:local")
            .expect("delete fact cascade");

    assert_eq!(del_report.facts_deleted, 1);
    assert_eq!(del_report.history_deleted, 1);
    assert_eq!(del_report.vectors_meta_deleted, 1);
    assert_eq!(del_report.vec_idx_deleted, 1);
    assert_eq!(del_report.vectors_fts_deleted, 1);
    assert_eq!(del_report.l3_nodes_deleted, 1);
    assert_eq!(del_report.l3_edges_deleted, 1);
    assert!(del_report.wal_truncated);

    // Verify all records completely gone
    assert!(
        db::get_fact(&conn, &crypto, "secret_project_codename")
            .unwrap()
            .is_none()
    );
    assert_eq!(
        db::get_fact_history(
            &conn,
            &crypto,
            "secret_project_codename",
            "memory_owner:local"
        )
        .unwrap()
        .len(),
        0
    );
    let vec_count: i64 = conn
        .query_row(
            "SELECT count(*) FROM vectors_meta WHERE vec_id = 'vec_fact_secret_project_codename'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(vec_count, 0);

    let audit_count: i64 = conn
        .query_row("SELECT count(*) FROM deletion_audit", [], |r| r.get(0))
        .unwrap();
    assert!(audit_count >= 1);
}

#[test]
fn test_cascading_subject_deletion_multi_tenant_and_audit() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("cognitive-memory-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");

    // Seed data for telegram:user_999
    let tg_owner = "telegram:user_999";
    let tg_domain = format!("memory_owner:{}", tg_owner);

    conn.execute(
        "INSERT INTO events (eventId, timestamp, domain, category, consolidation_status)
         VALUES ('evt_tg_1', 1000, ?1, 'chat', 'consolidated')",
        params![tg_domain],
    )
    .unwrap();

    db::upsert_vector(
        &conn,
        &crypto,
        "vec_tg_1",
        "episodic",
        "Telegram message content",
        &vec![0.03; db::MEMORY_VECTOR_DIM],
        Some(&tg_domain),
        Some("chat"),
        None,
        None,
        None,
    )
    .unwrap();

    // 1. Dry run deletion
    let dry_run = MemoryDeleteCoordinator::delete_subject_cascade(&conn, tg_owner, true).unwrap();
    assert!(dry_run.dry_run);
    assert_eq!(dry_run.counts.events, 1);
    assert_eq!(dry_run.counts.vectors_meta, 1);

    let event_still_there: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE eventId = 'evt_tg_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(event_still_there, 1, "Dry run must not delete records");

    // 2. Real deletion execution
    let report = MemoryDeleteCoordinator::delete_subject_cascade(&conn, tg_owner, false).unwrap();
    assert!(!report.dry_run);
    assert_eq!(report.counts.events, 1);
    assert_eq!(report.counts.vectors_meta, 1);
    assert_eq!(report.counts.vec_idx, 1);
    assert_eq!(report.counts.vectors_fts, 1);

    let event_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM events WHERE eventId = 'evt_tg_1'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(event_count_after, 0);

    let vec_count_after: i64 = conn
        .query_row(
            "SELECT count(*) FROM vectors_meta WHERE domain = ?1",
            params![tg_domain],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(vec_count_after, 0);
}
