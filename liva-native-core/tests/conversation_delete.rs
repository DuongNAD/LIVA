use liva_native_core::db::{self, Fact};
use liva_native_core::{DatabasePool, EncryptionEngine};
use rusqlite::params;

fn temp_db_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "liva-conversation-delete-{}-{}.db",
        std::process::id(),
        rand::random::<u64>()
    ))
}

#[test]
fn dry_run_then_delete_removes_every_scoped_projection_and_keeps_other_scope() {
    let path = temp_db_path();
    let pool = DatabasePool::new(&path).expect("create database");
    let crypto = EncryptionEngine::new("conversation-delete-test-key-32");
    let conn = pool.writer.get().expect("writer");
    let vector = vec![0.01_f32; db::MEMORY_VECTOR_DIM];
    let owner = "local";
    let conversation = "conv-delete";
    let domain = "memory_owner:local";
    let category = "conversation:conv-delete";
    let event_id = "turn-delete-1";
    let legacy_canary = "LEGACY_DELETE_CANARY_71f8f6";

    conn.execute(
        "INSERT INTO events (
             eventId, timestamp, rawUserMsg, rawAiReply, domain, category,
             consolidation_status
         ) VALUES (?1, 1, ?2, 'legacy reply', ?3, ?4, 'dlq')",
        params![event_id, legacy_canary, domain, category],
    )
    .unwrap();
    db::upsert_vector(
        &conn,
        &crypto,
        event_id,
        "conversation_turn",
        "private encrypted transcript",
        &vector,
        Some(domain),
        Some(category),
        None,
        None,
        Some(&[event_id.to_string()]),
    )
    .unwrap();
    let row_id: i64 = conn
        .query_row(
            "SELECT id FROM vectors_meta WHERE vec_id = ?1",
            [event_id],
            |row| row.get(0),
        )
        .unwrap();
    conn.execute(
        "INSERT OR REPLACE INTO vectors_fts(rowid, content) VALUES (?1, ?2)",
        params![row_id, legacy_canary],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO dlq_consolidation (
             session_id, failed_step, error_msg, retry_count, status, created_at
         ) VALUES (?1, 'fixture', 'fixture', 3, 'pending', 1)",
        [event_id],
    )
    .unwrap();

    let fact = Fact {
        key: "fact-from-deleted-turn".to_string(),
        value: "private derived fact".to_string(),
        createdAt: "2026-07-31".to_string(),
        updatedAt: "2026-07-31".to_string(),
        ttlDays: None,
        source: "conversation".to_string(),
        category: Some("fixture".to_string()),
        importance: 0.5,
        confidenceScore: 1.0,
        sourceTurnId: Some(event_id.to_string()),
        memory_strength: 1.0,
        last_accessed_at: 0,
        access_count: 0,
    };
    db::set_fact(&conn, &crypto, &fact).unwrap();
    conn.execute(
        "INSERT INTO facts_locked_backup(key, value, backed_up_at)
         VALUES (?1, ?2, 1)",
        params![fact.key, crypto.encrypt("historical fact").unwrap()],
    )
    .unwrap();
    conn.execute(
        "INSERT INTO agent_checkpoints(thread_id, state_json) VALUES (?1, ?2)",
        params![
            conversation,
            crypto
                .encrypt(r#"{"messages":["private checkpoint"]}"#)
                .unwrap()
        ],
    )
    .unwrap();

    // Scope khác phải còn nguyên.
    conn.execute(
        "INSERT INTO events (
             eventId, timestamp, domain, category, consolidation_status
         ) VALUES ('turn-keep-1', 2, ?1, 'conversation:conv-keep', 'pending')",
        [domain],
    )
    .unwrap();
    db::upsert_vector(
        &conn,
        &crypto,
        "turn-keep-1",
        "conversation_turn",
        "keep this conversation",
        &vector,
        Some(domain),
        Some("conversation:conv-keep"),
        None,
        None,
        Some(&["turn-keep-1".to_string()]),
    )
    .unwrap();

    let preview =
        db::delete_conversation(&conn, owner, conversation, true).expect("dry-run deletion");
    assert!(preview.dry_run);
    assert_eq!(preview.counts.events, 1);
    assert_eq!(preview.counts.vectors_meta, 1);
    assert_eq!(preview.counts.vec_idx, 1);
    assert_eq!(preview.counts.vectors_fts, 1);
    assert_eq!(preview.counts.checkpoints, 1);
    assert_eq!(preview.counts.dlq, 1);
    assert_eq!(preview.counts.facts, 1);
    assert_eq!(preview.counts.fact_backups, 1);
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM events WHERE eventId = ?1",
            [event_id],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "dry-run must not mutate data"
    );

    let deleted =
        db::delete_conversation(&conn, owner, conversation, false).expect("execute deletion");
    assert!(!deleted.dry_run);
    assert!(deleted.wal_truncated, "privacy deletion must truncate WAL");
    for (table, predicate) in [
        ("events", "eventId = 'turn-delete-1'"),
        ("vectors_meta", "vec_id = 'turn-delete-1'"),
        ("agent_checkpoints", "thread_id = 'conv-delete'"),
        ("dlq_consolidation", "session_id = 'turn-delete-1'"),
        ("facts", "key = 'fact-from-deleted-turn'"),
        ("facts_locked_backup", "key = 'fact-from-deleted-turn'"),
    ] {
        let sql = format!("SELECT COUNT(*) FROM {table} WHERE {predicate}");
        let count: i64 = conn.query_row(&sql, [], |row| row.get(0)).unwrap();
        assert_eq!(count, 0, "{table} still has scoped data");
    }
    assert_eq!(
        conn.query_row(
            "SELECT COUNT(*) FROM vectors_meta WHERE vec_id = 'turn-keep-1'",
            [],
            |row| row.get::<_, i64>(0)
        )
        .unwrap(),
        1,
        "other conversation must survive"
    );
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM deletion_audit", [], |row| {
            row.get::<_, i64>(0)
        })
        .unwrap(),
        2,
        "dry-run and execution both need an audit record"
    );

    drop(conn);
    drop(pool);
    let bytes = std::fs::read(&path).expect("read compacted database");
    assert!(
        !bytes
            .windows(legacy_canary.len())
            .any(|window| window == legacy_canary.as_bytes()),
        "deleted legacy plaintext remains in database bytes"
    );
    let wal_path = path.with_extension("db-wal");
    if wal_path.exists() {
        let wal = std::fs::read(&wal_path).unwrap();
        assert!(
            !wal.windows(legacy_canary.len())
                .any(|window| window == legacy_canary.as_bytes()),
            "deleted legacy plaintext remains in WAL"
        );
        let _ = std::fs::remove_file(wal_path);
    }
    let _ = std::fs::remove_file(path.with_extension("db-shm"));
    let _ = std::fs::remove_file(path);
}
