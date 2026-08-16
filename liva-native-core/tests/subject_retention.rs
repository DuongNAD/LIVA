use liva_native_core::db::{self, Fact};
use liva_native_core::{DatabasePool, EncryptionEngine};
use rusqlite::{Connection, params};

fn temp_db_path(label: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "liva-{label}-{}-{}.db",
        std::process::id(),
        rand::random::<u64>()
    ))
}

fn seed_conversation(
    conn: &Connection,
    crypto: &EncryptionEngine,
    event_id: &str,
    owner: &str,
    conversation: &str,
    timestamp: i64,
) {
    let domain = format!("memory_owner:{owner}");
    let category = format!("conversation:{conversation}");
    conn.execute(
        "INSERT INTO events (
             eventId, timestamp, domain, category, consolidation_status
         ) VALUES (?1, ?2, ?3, ?4, 'consolidated')",
        params![event_id, timestamp, domain, category],
    )
    .unwrap();
    db::upsert_vector(
        conn,
        crypto,
        event_id,
        "conversation_turn",
        &format!("transcript {event_id}"),
        &vec![0.01; db::MEMORY_VECTOR_DIM],
        Some(&domain),
        Some(&category),
        None,
        None,
        Some(&[event_id.to_string()]),
    )
    .unwrap();
    conn.execute(
        "UPDATE vectors_meta SET created_at = ?2 WHERE vec_id = ?1",
        params![event_id, timestamp],
    )
    .unwrap();
}

#[test]
fn delete_subject_dry_run_then_execute_removes_local_memory_and_keeps_other_owner() {
    let path = temp_db_path("subject-delete");
    let pool = DatabasePool::new(&path).expect("create database");
    let crypto = EncryptionEngine::new("subject-delete-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");
    let canary = "LOCAL_SUBJECT_CANARY_48f31a";

    seed_conversation(&conn, &crypto, "local-turn", "local", "local-chat", 1);
    seed_conversation(
        &conn,
        &crypto,
        "telegram-turn",
        "telegram:42",
        "remote-chat",
        1,
    );
    conn.execute(
        "UPDATE events SET rawUserMsg = ?1 WHERE eventId = 'local-turn'",
        [canary],
    )
    .unwrap();

    db::upsert_vector(
        &conn,
        &crypto,
        "local-episodic",
        "episodic",
        canary,
        &vec![0.02; db::MEMORY_VECTOR_DIM],
        Some("memory_owner:local"),
        Some("profile"),
        None,
        None,
        Some(&["local-turn".to_string()]),
    )
    .unwrap();
    let fact = Fact {
        key: "local-private-fact".to_string(),
        value: "private fact".to_string(),
        createdAt: "2026-07-31".to_string(),
        updatedAt: "2026-07-31".to_string(),
        ttlDays: None,
        source: "conversation".to_string(),
        category: Some("profile".to_string()),
        importance: 0.5,
        confidenceScore: 1.0,
        sourceTurnId: Some("local-turn".to_string()),
        memory_strength: 1.0,
        last_accessed_at: 0,
        access_count: 0,
    };
    db::set_fact(&conn, &crypto, &fact).unwrap();
    conn.execute(
        "INSERT INTO facts_locked_backup(key, value, backed_up_at)
         VALUES (?1, ?2, 1)",
        params![fact.key, crypto.encrypt("private historical fact").unwrap()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_checkpoints(thread_id, state_json) VALUES ('local-chat', ?1)",
        [crypto.encrypt(r#"{"messages":["private"]}"#).unwrap()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO turn_layer_nodes(turnId, temporal_anchor, userMsg, aiReply, createdAt)
         VALUES ('legacy-local-turn', 1, ?1, 'reply', '2026-07-31')",
        [canary],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO consolidation_checkpoints(
             session_id, last_step, state_data, created_at, updated_at
         ) VALUES ('event-projection-v1', 1, ?1, 1, 1)",
        [format!(
            r#"{{"last_event_id":"local-turn","canary":"{canary}"}}"#
        )],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO dlq_consolidation(
             session_id, failed_step, error_msg, retry_count, status, created_at
         ) VALUES ('local-turn', 'fixture', ?1, 3, 'pending', 1)",
        [canary],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO vector_dlq(delete_filter, status, retry_count)
         VALUES (?1, 'pending', 0)",
        [format!(
            r#"{{"domain":"memory_owner:local","canary":"{canary}"}}"#
        )],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO l3_nodes(id, label, properties) VALUES ('local-node', 'private', ?1)",
        [format!(r#"{{"canary":"{canary}"}}"#)],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO l3_edges(source, target, relation)
         VALUES ('local-node', 'local-node', 'self')",
        [],
    )
    .unwrap();

    let preview = db::delete_subject(&conn, "local", true).expect("subject dry-run");
    assert!(preview.dry_run);
    assert_eq!(preview.counts.events, 1);
    assert_eq!(preview.counts.vectors_meta, 2);
    assert_eq!(preview.counts.facts, 1);
    assert_eq!(preview.counts.turns, 1);
    assert_eq!(preview.counts.l3_nodes, 1);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE eventId = 'local-turn'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "dry-run must not mutate subject data"
    );

    let deleted = db::delete_subject(&conn, "local", false).expect("subject deletion");
    assert!(!deleted.dry_run);
    assert!(deleted.wal_truncated);
    for table in [
        "facts",
        "facts_locked_backup",
        "agent_checkpoints",
        "turn_layer_nodes",
        "consolidation_checkpoints",
        "dlq_consolidation",
        "vector_dlq",
        "l3_edges",
        "l3_nodes",
    ] {
        let count: i64 = conn
            .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                row.get(0)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} retained local subject data");
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE eventId = 'local-turn'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE domain = 'memory_owner:local'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE eventId = 'telegram-turn'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "another owner must survive DeleteSubject(local)"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM vectors_meta
             WHERE domain = 'memory_owner:telegram:42'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM deletion_audit", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        2
    );
    let tg_deleted =
        db::delete_subject(&conn, "telegram:42", false).expect("DeleteSubject for telegram:42");
    assert_eq!(tg_deleted.counts.events, 1);
    assert_eq!(tg_deleted.counts.vectors_meta, 1);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE eventId = 'telegram-turn'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        0,
        "telegram:42 events must be deleted after DeleteSubject(telegram:42)"
    );

    drop(conn);
    drop(pool);
    let bytes = std::fs::read(&path).expect("read compacted database");
    assert!(
        !bytes
            .windows(canary.len())
            .any(|window| window == canary.as_bytes()),
        "deleted subject plaintext remains in database bytes"
    );
    let wal_path = path.with_extension("db-wal");
    if wal_path.exists() {
        let wal = std::fs::read(&wal_path).unwrap();
        assert!(
            !wal.windows(canary.len())
                .any(|window| window == canary.as_bytes()),
            "deleted subject plaintext remains in WAL"
        );
        let _ = std::fs::remove_file(wal_path);
    }
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    let _ = std::fs::remove_file(path);
}

#[test]
fn retention_uses_last_activity_and_processes_bounded_retryable_batches() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("retention-test-key-32-bytes");
    let conn = pool.writer.get().expect("writer");
    let cutoff = 1_000_000_i64;

    seed_conversation(&conn, &crypto, "old-a", "local", "old-a", 100);
    seed_conversation(&conn, &crypto, "old-b", "local", "old-b", 200);
    seed_conversation(&conn, &crypto, "mixed-old", "local", "active", 100);
    seed_conversation(
        &conn,
        &crypto,
        "mixed-recent",
        "local",
        "active",
        cutoff + 1,
    );
    seed_conversation(
        &conn,
        &crypto,
        "remote-old",
        "telegram:42",
        "remote-old",
        50,
    );

    let preview =
        db::sweep_conversation_retention(&conn, "local", cutoff, 1, true).expect("dry run");
    assert!(preview.dry_run);
    assert_eq!(preview.candidates, vec!["old-a"]);
    assert!(preview.has_more);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE category = 'conversation:old-a'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1
    );

    let first =
        db::sweep_conversation_retention(&conn, "local", cutoff, 1, false).expect("first batch");
    assert_eq!(first.candidates, vec!["old-a"]);
    assert_eq!(first.deletions.len(), 1);
    assert!(first.has_more);

    let second =
        db::sweep_conversation_retention(&conn, "local", cutoff, 25, false).expect("second batch");
    assert_eq!(second.candidates, vec!["old-b"]);
    assert_eq!(second.deletions.len(), 1);
    assert!(!second.has_more);

    for category in ["conversation:old-a", "conversation:old-b"] {
        assert_eq!(
            conn.query_row(
                "SELECT COUNT(*) FROM events WHERE domain = 'memory_owner:local'
                 AND category = ?1",
                [category],
                |row| row.get::<_, i64>(0)
            )
            .unwrap(),
            0
        );
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE category = 'conversation:active'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        2,
        "conversation with recent activity must survive"
    );
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE domain = 'memory_owner:telegram:42'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "retention must stay inside owner scope"
    );
}
