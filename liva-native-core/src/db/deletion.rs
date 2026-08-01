use rusqlite::Connection;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationDeletionCounts {
    pub events: i64,
    pub vectors_meta: i64,
    pub vec_idx: i64,
    pub vectors_fts: i64,
    pub checkpoints: i64,
    pub dlq: i64,
    pub facts: i64,
    pub fact_backups: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationDeletionReport {
    pub request_id: String,
    pub scope_hash: String,
    pub dry_run: bool,
    pub counts: ConversationDeletionCounts,
    /// `false` ở dry-run hoặc khi SQLite báo còn reader giữ WAL. Dữ liệu logic
    /// đã xóa nhưng caller phải chạy maintenance/retry trước khi tuyên bố xóa
    /// byte-level hoàn tất.
    pub wal_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubjectDeletionCounts {
    pub events: i64,
    pub vectors_meta: i64,
    pub vec_idx: i64,
    pub vectors_fts: i64,
    pub checkpoints: i64,
    pub consolidation_checkpoints: i64,
    pub dlq: i64,
    pub vector_dlq: i64,
    pub facts: i64,
    pub fact_backups: i64,
    pub turns: i64,
    pub l3_edges: i64,
    pub l3_nodes: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubjectDeletionReport {
    pub request_id: String,
    pub scope_hash: String,
    pub dry_run: bool,
    pub counts: SubjectDeletionCounts,
    pub wal_truncated: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetentionSweepReport {
    pub scope_hash: String,
    pub cutoff_ms: i64,
    pub batch_limit: usize,
    pub dry_run: bool,
    pub candidates: Vec<String>,
    pub deletions: Vec<ConversationDeletionReport>,
    pub has_more: bool,
}

fn scope_hash(owner_domain: &str, conversation_category: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"liva-conversation-delete-v1\0");
    digest.update(owner_domain.as_bytes());
    digest.update(b"\0");
    digest.update(conversation_category.as_bytes());
    hex::encode(digest.finalize())
}

fn subject_scope_hash(owner_domain: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut digest = Sha256::new();
    digest.update(b"liva-subject-delete-v1\0");
    digest.update(owner_domain.as_bytes());
    hex::encode(digest.finalize())
}

fn mismatch(projection: &str, expected: i64, actual: usize) -> rusqlite::Error {
    rusqlite::Error::SqliteFailure(
        rusqlite::ffi::Error::new(rusqlite::ffi::SQLITE_CONSTRAINT),
        Some(format!(
            "conversation deletion mismatch for {projection}: expected {expected}, deleted {actual}"
        )),
    )
}

/// Xóa một hội thoại khỏi mọi projection hiện đang được runtime sản xuất.
///
/// Scope là cặp `memory_owner:{owner_id}` + `conversation:{conversation_id}`;
/// không nhận chuỗi SQL/filter từ caller. `dry_run` vẫn ghi audit nhưng không
/// đụng dữ liệu đích. Với owner `local`, checkpoint dùng chính
/// `conversation_id`; các kênh Telegram hiện không tạo checkpoint.
pub fn delete_conversation(
    conn: &Connection,
    owner_id: &str,
    conversation_id: &str,
    dry_run: bool,
) -> Result<ConversationDeletionReport, rusqlite::Error> {
    let owner_id = owner_id.trim();
    let conversation_id = conversation_id.trim();
    if owner_id.is_empty() || conversation_id.is_empty() {
        return Err(rusqlite::Error::InvalidParameterName(
            "owner_id and conversation_id must not be empty".to_string(),
        ));
    }
    let owner_domain = format!("memory_owner:{owner_id}");
    let conversation_category = format!("conversation:{conversation_id}");
    let request_id = format!("del_{}", uuid::Uuid::new_v4());
    let scope_hash = scope_hash(&owner_domain, &conversation_category);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    conn.execute_batch("PRAGMA secure_delete = ON;")?;
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let scope = [&owner_domain, &conversation_category];
    let count =
        |sql: &str| -> Result<i64, rusqlite::Error> { tx.query_row(sql, scope, |row| row.get(0)) };
    let events = count(
        "SELECT COUNT(*) FROM events
         WHERE domain = ?1 AND category = ?2",
    )?;
    let vectors_meta = count(
        "SELECT COUNT(*) FROM vectors_meta
         WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2",
    )?;
    let vec_idx = count(
        "SELECT COUNT(*) FROM vec_idx
         WHERE rowid IN (
             SELECT id FROM vectors_meta
             WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2
         )",
    )?;
    let vectors_fts = count(
        "SELECT COUNT(*) FROM vectors_fts
         WHERE rowid IN (
             SELECT id FROM vectors_meta
             WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2
         )",
    )?;
    let dlq = count(
        "SELECT COUNT(*) FROM dlq_consolidation
         WHERE session_id IN (
             SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
         )",
    )?;
    let facts = count(
        "SELECT COUNT(*) FROM facts
         WHERE sourceTurnId IN (
             SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
         )",
    )?;
    let fact_backups = count(
        "SELECT COUNT(*) FROM facts_locked_backup
         WHERE key IN (
             SELECT key FROM facts
             WHERE sourceTurnId IN (
                 SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
             )
         )",
    )?;
    let checkpoints = if owner_id == "local" {
        tx.query_row(
            "SELECT COUNT(*) FROM agent_checkpoints WHERE thread_id = ?1",
            [conversation_id],
            |row| row.get(0),
        )?
    } else {
        0
    };
    let counts = ConversationDeletionCounts {
        events,
        vectors_meta,
        vec_idx,
        vectors_fts,
        checkpoints,
        dlq,
        facts,
        fact_backups,
    };
    let counts_json = serde_json::to_string(&counts)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    tx.execute(
        "INSERT INTO deletion_audit
         (audit_id, scope_hash, dry_run, counts_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![request_id, scope_hash, i64::from(dry_run), counts_json, now],
    )?;

    if !dry_run {
        let deleted = tx.execute(
            "DELETE FROM facts_locked_backup
             WHERE key IN (
                 SELECT key FROM facts
                 WHERE sourceTurnId IN (
                     SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
                 )
             )",
            scope,
        )?;
        if deleted as i64 != fact_backups {
            return Err(mismatch("facts_locked_backup", fact_backups, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM facts
             WHERE sourceTurnId IN (
                 SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
             )",
            scope,
        )?;
        if deleted as i64 != facts {
            return Err(mismatch("facts", facts, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM dlq_consolidation
             WHERE session_id IN (
                 SELECT eventId FROM events WHERE domain = ?1 AND category = ?2
             )",
            scope,
        )?;
        if deleted as i64 != dlq {
            return Err(mismatch("dlq_consolidation", dlq, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM vectors_fts
             WHERE rowid IN (
                 SELECT id FROM vectors_meta
                 WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2
             )",
            scope,
        )?;
        if deleted as i64 != vectors_fts {
            return Err(mismatch("vectors_fts", vectors_fts, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM vec_idx
             WHERE rowid IN (
                 SELECT id FROM vectors_meta
                 WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2
             )",
            scope,
        )?;
        if deleted as i64 != vec_idx {
            return Err(mismatch("vec_idx", vec_idx, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM vectors_meta
             WHERE type = 'conversation_turn' AND domain = ?1 AND category = ?2",
            scope,
        )?;
        if deleted as i64 != vectors_meta {
            return Err(mismatch("vectors_meta", vectors_meta, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM events WHERE domain = ?1 AND category = ?2",
            scope,
        )?;
        if deleted as i64 != events {
            return Err(mismatch("events", events, deleted));
        }
        if owner_id == "local" {
            let deleted = tx.execute(
                "DELETE FROM agent_checkpoints WHERE thread_id = ?1",
                [conversation_id],
            )?;
            if deleted as i64 != checkpoints {
                return Err(mismatch("agent_checkpoints", checkpoints, deleted));
            }
        }
    }
    tx.commit()?;

    let wal_truncated = if dry_run {
        false
    } else {
        match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        }) {
            Ok((busy, log_frames, checkpointed)) => {
                busy == 0 && (log_frames <= 0 || checkpointed == log_frames)
            }
            Err(error) => {
                tracing::warn!(%error, "conversation deletion could not truncate WAL");
                false
            }
        }
    };

    Ok(ConversationDeletionReport {
        request_id,
        scope_hash,
        dry_run,
        counts,
        wal_truncated,
    })
}

/// Xóa toàn bộ bộ nhớ cá nhân của subject local.
///
/// Hiện một số projection lịch sử (`facts`, `turn_layer_nodes`, `l3_*`,
/// checkpoint) chưa có cột owner. Vì vậy API cố ý từ chối owner khác `local`
/// thay vì xóa chéo tenant. Scope local bao gồm cả
/// `memory_owner:legacy_unowned`, là dữ liệu conversation trước migration v2.
pub fn delete_subject(
    conn: &Connection,
    owner_id: &str,
    dry_run: bool,
) -> Result<SubjectDeletionReport, rusqlite::Error> {
    if owner_id.trim() != "local" {
        return Err(rusqlite::Error::InvalidParameterName(
            "DeleteSubject currently supports only the local owner".to_string(),
        ));
    }

    let owner_domain = "memory_owner:local";
    let legacy_domain = "memory_owner:legacy_unowned";
    let request_id = format!("subdel_{}", uuid::Uuid::new_v4());
    let scope_hash = subject_scope_hash(owner_domain);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as i64;

    conn.execute_batch("PRAGMA secure_delete = ON;")?;
    let tx = rusqlite::Transaction::new_unchecked(conn, rusqlite::TransactionBehavior::Immediate)?;
    let domains = [owner_domain, legacy_domain];
    let count_domains = |sql: &str| -> Result<i64, rusqlite::Error> {
        tx.query_row(sql, domains, |row| row.get(0))
    };
    let events = count_domains("SELECT COUNT(*) FROM events WHERE domain IN (?1, ?2)")?;
    let vectors_meta = count_domains("SELECT COUNT(*) FROM vectors_meta WHERE domain IN (?1, ?2)")?;
    let vec_idx = count_domains(
        "SELECT COUNT(*) FROM vec_idx
         WHERE rowid IN (
             SELECT id FROM vectors_meta WHERE domain IN (?1, ?2)
         )",
    )?;
    let vectors_fts = count_domains(
        "SELECT COUNT(*) FROM vectors_fts
         WHERE rowid IN (
             SELECT id FROM vectors_meta WHERE domain IN (?1, ?2)
         )",
    )?;
    let dlq = count_domains(
        "SELECT COUNT(*) FROM dlq_consolidation
         WHERE session_id IN (
             SELECT eventId FROM events WHERE domain IN (?1, ?2)
         )",
    )?;
    let vector_dlq = count_domains(
        "SELECT COUNT(*) FROM vector_dlq
         WHERE instr(delete_filter, ?1) > 0 OR instr(delete_filter, ?2) > 0",
    )?;
    let scalar_count = |table: &str| -> Result<i64, rusqlite::Error> {
        tx.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
            row.get(0)
        })
    };
    let counts = SubjectDeletionCounts {
        events,
        vectors_meta,
        vec_idx,
        vectors_fts,
        checkpoints: scalar_count("agent_checkpoints")?,
        consolidation_checkpoints: scalar_count("consolidation_checkpoints")?,
        dlq,
        vector_dlq,
        facts: scalar_count("facts")?,
        fact_backups: scalar_count("facts_locked_backup")?,
        turns: scalar_count("turn_layer_nodes")?,
        l3_edges: scalar_count("l3_edges")?,
        l3_nodes: scalar_count("l3_nodes")?,
    };
    let counts_json = serde_json::to_string(&counts)
        .map_err(|error| rusqlite::Error::ToSqlConversionFailure(Box::new(error)))?;
    tx.execute(
        "INSERT INTO deletion_audit
         (audit_id, scope_hash, dry_run, counts_json, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5)",
        rusqlite::params![request_id, scope_hash, i64::from(dry_run), counts_json, now],
    )?;

    if !dry_run {
        let exact_delete =
            |sql: &str, expected: i64, projection: &str| -> Result<(), rusqlite::Error> {
                let deleted = tx.execute(sql, [])?;
                if deleted as i64 != expected {
                    return Err(mismatch(projection, expected, deleted));
                }
                Ok(())
            };
        exact_delete(
            "DELETE FROM facts_locked_backup",
            counts.fact_backups,
            "facts_locked_backup",
        )?;
        exact_delete("DELETE FROM facts", counts.facts, "facts")?;
        let deleted = tx.execute(
            "DELETE FROM dlq_consolidation
             WHERE session_id IN (
                 SELECT eventId FROM events WHERE domain IN (?1, ?2)
             )",
            domains,
        )?;
        if deleted as i64 != counts.dlq {
            return Err(mismatch("dlq_consolidation", counts.dlq, deleted));
        }
        let deleted = tx.execute(
            "DELETE FROM vector_dlq
             WHERE instr(delete_filter, ?1) > 0 OR instr(delete_filter, ?2) > 0",
            domains,
        )?;
        if deleted as i64 != counts.vector_dlq {
            return Err(mismatch("vector_dlq", counts.vector_dlq, deleted));
        }
        for (table, expected) in [
            ("vectors_fts", counts.vectors_fts),
            ("vec_idx", counts.vec_idx),
        ] {
            let deleted = tx.execute(
                &format!(
                    "DELETE FROM {table}
                     WHERE rowid IN (
                         SELECT id FROM vectors_meta WHERE domain IN (?1, ?2)
                     )"
                ),
                domains,
            )?;
            if deleted as i64 != expected {
                return Err(mismatch(table, expected, deleted));
            }
        }
        let deleted = tx.execute("DELETE FROM vectors_meta WHERE domain IN (?1, ?2)", domains)?;
        if deleted as i64 != counts.vectors_meta {
            return Err(mismatch("vectors_meta", counts.vectors_meta, deleted));
        }
        let deleted = tx.execute("DELETE FROM events WHERE domain IN (?1, ?2)", domains)?;
        if deleted as i64 != counts.events {
            return Err(mismatch("events", counts.events, deleted));
        }
        exact_delete(
            "DELETE FROM agent_checkpoints",
            counts.checkpoints,
            "agent_checkpoints",
        )?;
        exact_delete(
            "DELETE FROM consolidation_checkpoints",
            counts.consolidation_checkpoints,
            "consolidation_checkpoints",
        )?;
        exact_delete(
            "DELETE FROM turn_layer_nodes",
            counts.turns,
            "turn_layer_nodes",
        )?;
        exact_delete("DELETE FROM l3_edges", counts.l3_edges, "l3_edges")?;
        exact_delete("DELETE FROM l3_nodes", counts.l3_nodes, "l3_nodes")?;
    }
    tx.commit()?;

    let wal_truncated = if dry_run {
        false
    } else {
        truncate_wal(conn, "subject deletion")
    };
    Ok(SubjectDeletionReport {
        request_id,
        scope_hash,
        dry_run,
        counts,
        wal_truncated,
    })
}

/// Xóa các hội thoại không có hoạt động mới hơn `cutoff_ms`.
///
/// Mỗi batch tối đa 25 hội thoại. Mỗi hội thoại là một transaction/audit độc
/// lập, nên nếu tiến trình dừng giữa batch thì lần chạy sau tiếp tục từ dữ liệu
/// còn lại mà không lặp xóa hay cần cursor bên ngoài.
pub fn sweep_conversation_retention(
    conn: &Connection,
    owner_id: &str,
    cutoff_ms: i64,
    batch_limit: usize,
    dry_run: bool,
) -> Result<RetentionSweepReport, rusqlite::Error> {
    let owner_id = owner_id.trim();
    if owner_id.is_empty() || cutoff_ms <= 0 {
        return Err(rusqlite::Error::InvalidParameterName(
            "owner_id must not be empty and cutoff_ms must be positive".to_string(),
        ));
    }
    let owner_domain = format!("memory_owner:{owner_id}");
    let batch_limit = batch_limit.clamp(1, 25);
    let eligible_cte = "
        WITH activity AS (
            SELECT category, MAX(timestamp) AS last_activity
            FROM events
            WHERE domain = ?1
              AND category LIKE 'conversation:%'
              AND length(category) > length('conversation:')
            GROUP BY category
            UNION ALL
            SELECT category, MAX(created_at) AS last_activity
            FROM vectors_meta
            WHERE domain = ?1
              AND category LIKE 'conversation:%'
              AND length(category) > length('conversation:')
            GROUP BY category
        ),
        eligible AS (
            SELECT category, MAX(last_activity) AS last_activity
            FROM activity
            GROUP BY category
            HAVING MAX(last_activity) < ?2
        )";
    let total: i64 = conn.query_row(
        &format!("{eligible_cte} SELECT COUNT(*) FROM eligible"),
        rusqlite::params![owner_domain, cutoff_ms],
        |row| row.get(0),
    )?;
    let candidates = {
        let mut statement = conn.prepare(&format!(
            "{eligible_cte}
             SELECT substr(category, length('conversation:') + 1)
             FROM eligible
             ORDER BY last_activity, category
             LIMIT ?3"
        ))?;
        statement
            .query_map(
                rusqlite::params![owner_domain, cutoff_ms, batch_limit as i64],
                |row| row.get::<_, String>(0),
            )?
            .collect::<Result<Vec<_>, _>>()?
    };
    let has_more = total > candidates.len() as i64;
    let mut deletions = Vec::with_capacity(candidates.len());
    for conversation_id in &candidates {
        deletions.push(delete_conversation(
            conn,
            owner_id,
            conversation_id,
            dry_run,
        )?);
    }

    Ok(RetentionSweepReport {
        scope_hash: subject_scope_hash(&owner_domain),
        cutoff_ms,
        batch_limit,
        dry_run,
        candidates,
        deletions,
        has_more,
    })
}

fn truncate_wal(conn: &Connection, operation: &str) -> bool {
    match conn.query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |row| {
        Ok((
            row.get::<_, i64>(0)?,
            row.get::<_, i64>(1)?,
            row.get::<_, i64>(2)?,
        ))
    }) {
        Ok((busy, log_frames, checkpointed)) => {
            busy == 0 && (log_frames <= 0 || checkpointed == log_frames)
        }
        Err(error) => {
            tracing::warn!(%error, %operation, "privacy deletion could not truncate WAL");
            false
        }
    }
}
