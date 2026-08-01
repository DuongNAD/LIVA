use super::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// H5: vec0 candidates phải phủ CẢ đóng gói (cạnh exe + resources/) chứ không
    /// chỉ dev (node_modules quanh cwd) — nếu không app Tauri cài đặt sẽ sập DB.
    #[test]
    fn vec0_candidates_chi_gom_trust_root_dev_va_dong_goi() {
        let exe_dir = std::path::Path::new(if cfg!(windows) {
            r"C:\App\bin"
        } else {
            "/app/bin"
        });
        let ext = if cfg!(target_os = "windows") {
            ".dll"
        } else if cfg!(target_os = "macos") {
            ".dylib"
        } else {
            ".so"
        };
        let vec_name = format!("vec0{ext}");
        let c = vec0_candidate_paths(Some(exe_dir));

        assert!(
            c.iter()
                .any(|p| p.contains("node_modules") && p.ends_with(&vec_name)),
            "phải có candidate node_modules (dev)"
        );
        assert!(
            c.iter().any(
                |p| std::path::Path::new(p).parent() == Some(exe_dir) && p.ends_with(&vec_name)
            ),
            "phải có candidate cạnh executable (đóng gói)"
        );
        assert!(
            c.iter()
                .any(|p| p.contains("resources") && p.ends_with(&vec_name)),
            "phải có candidate trong resources/ (Tauri bundle)"
        );
        assert!(
            c.iter()
                .all(|path| std::path::Path::new(path).is_absolute()),
            "không được nạp vec0 từ cwd hay search path của hệ điều hành"
        );
        assert!(!c.contains(&"vec0".to_string()));
        assert!(!c.contains(&vec_name));

        // Không có exe_dir (không xác định được) → ít candidate hơn, không có resources.
        let c_none = vec0_candidate_paths(None);
        assert!(c_none.len() < c.len(), "thiếu exe_dir thì ít candidate hơn");
        assert!(!c_none.iter().any(|p| p.contains("resources")));
    }
    use crate::crypto::EncryptionEngine;

    #[test]
    fn pending_consolidation_query_uses_ordered_partial_index() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");
        let conn = pool.writer.get().expect("get writer");
        let mut statement = conn
            .prepare(
                "EXPLAIN QUERY PLAN \
                 SELECT eventId FROM events \
                 WHERE consolidation_status = 'pending' \
                 ORDER BY timestamp, eventId LIMIT 20",
            )
            .expect("prepare query plan");
        let details = statement
            .query_map([], |row| row.get::<_, String>(3))
            .expect("query plan")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect query plan");

        assert!(
            details
                .iter()
                .any(|detail| detail.contains("idx_events_pending_ts")),
            "ordered partial index must serve the consolidation queue: {details:?}",
        );
        assert!(
            details
                .iter()
                .all(|detail| !detail.contains("USE TEMP B-TREE")),
            "consolidation query must not sort into a temporary B-tree: {details:?}",
        );
    }

    /// Lộ trình 0.2: DB mới phải được đóng dấu SCHEMA_VERSION, và mở lại một DB
    /// cũ (user_version=0 nhưng đủ bảng) phải nâng lên mà KHÔNG mất dữ liệu.
    #[test]
    fn migration_dong_dau_va_khong_mat_du_lieu() {
        use rusqlite::Connection;
        let path = std::env::temp_dir().join(format!(
            "liva_schema_migration_{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        // Lần tạo đầu: schema mới phải ở đúng SCHEMA_VERSION.
        {
            let pool = DatabasePool::new(&path).expect("tao db");
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "db moi phai duoc dong dau");
            conn.execute(
                "INSERT INTO facts (key, value, createdAt, updatedAt, source) \
                 VALUES ('ten', 'Bun', '2026-07-22', '2026-07-22', 'test')",
                [],
            )
            .unwrap();
        }

        // Giả lập DB "cũ": hạ user_version về 0 như thể được tạo trước khi có
        // đánh số. Dữ liệu vẫn còn.
        {
            let c = Connection::open(&path).unwrap();
            c.execute_batch("PRAGMA user_version = 0;").unwrap();
        }

        // Mở lại: phải nâng lên SCHEMA_VERSION, dữ liệu cũ còn nguyên.
        {
            let pool = DatabasePool::new(&path).expect("mo lai db cu");
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "db cu phai duoc nang len");
            let val: String = conn
                .query_row("SELECT value FROM facts WHERE key='ten'", [], |r| r.get(0))
                .unwrap();
            assert_eq!(val, "Bun", "du lieu cu KHONG duoc mat khi migrate");
        }

        for candidate in [
            path.clone(),
            std::path::PathBuf::from(format!("{}-wal", path.display())),
            std::path::PathBuf::from(format!("{}-shm", path.display())),
        ] {
            match std::fs::remove_file(&candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("khong the don test DB {}: {error}", candidate.display()),
            }
        }
    }

    #[test]
    fn migration_cach_ly_conversation_turn_legacy_khong_owner() {
        let path = std::env::temp_dir().join(format!(
            "liva_legacy_unowned_migration_{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        {
            let pool = DatabasePool::new(&path).expect("tao legacy db");
            let conn = pool.writer.get().unwrap();
            for (vec_id, memory_type, domain) in [
                ("legacy-turn", "conversation_turn", "General"),
                ("semantic-general", "semantic_fact", "General"),
                (
                    "already-scoped",
                    "conversation_turn",
                    "memory_owner:telegram:100",
                ),
            ] {
                conn.execute(
                    "INSERT INTO vectors_meta \
                     (vec_id, type, content, domain, category, created_at) \
                     VALUES (?1, ?2, ?3, ?4, 'legacy', 1)",
                    rusqlite::params![vec_id, memory_type, vec_id, domain],
                )
                .unwrap();
            }
            conn.execute_batch("PRAGMA user_version = 1;").unwrap();
        }

        {
            let pool = DatabasePool::new(&path).expect("migrate legacy db");
            let conn = pool.writer.get().unwrap();
            let domain_of = |vec_id: &str| -> String {
                conn.query_row(
                    "SELECT domain FROM vectors_meta WHERE vec_id = ?1",
                    [vec_id],
                    |row| row.get(0),
                )
                .unwrap()
            };

            assert_eq!(domain_of("legacy-turn"), "memory_owner:legacy_unowned");
            assert_eq!(domain_of("semantic-general"), "General");
            assert_eq!(domain_of("already-scoped"), "memory_owner:telegram:100");
        }

        let _ = std::fs::remove_file(path);
    }

    /// Bổ sung khuyến nghị review #3: migration là MỘT LẦN (idempotent qua
    /// `user_version`). Sau khi đã ở v2, chèn một hàng `General` conversation_turn
    /// rồi mở lại DB — migration KHÔNG chạy lại nên hàng đó KHÔNG bị backfill.
    #[test]
    fn migration_idempotent_khong_chay_lai_sau_v2() {
        let path = std::env::temp_dir().join(format!(
            "liva_idem_migration_{}.sqlite",
            uuid::Uuid::new_v4()
        ));

        // Lần 1: tạo + migrate lên SCHEMA_VERSION.
        {
            let pool = DatabasePool::new(&path).expect("tao db");
            let v: i64 = pool
                .writer
                .get()
                .unwrap()
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "lần đầu phải migrate lên SCHEMA_VERSION");
        }

        // Chèn một 'General' conversation_turn SAU khi DB đã ở v2.
        {
            let pool = DatabasePool::new(&path).unwrap();
            pool.writer
                .get()
                .unwrap()
                .execute(
                    "INSERT INTO vectors_meta (vec_id, type, content, domain, category, created_at) \
                     VALUES ('late', 'conversation_turn', 'x', 'General', 'c', 1)",
                    [],
                )
                .unwrap();
        }

        // Mở lại: migration KHÔNG chạy lại (đã v2) → 'late' vẫn 'General'.
        {
            let pool = DatabasePool::new(&path).unwrap();
            let conn = pool.writer.get().unwrap();
            let v: i64 = conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION, "mở lại không được đổi version");
            let domain: String = conn
                .query_row(
                    "SELECT domain FROM vectors_meta WHERE vec_id='late'",
                    [],
                    |r| r.get(0),
                )
                .unwrap();
            assert_eq!(
                domain, "General",
                "migration đã chạy 1 lần → mở lại KHÔNG backfill hàng mới (idempotent)"
            );
        }

        let _ = std::fs::remove_file(path);
    }

    /// DB do bản LIVA mới hơn tạo (version tương lai) phải bị TỪ CHỐI rõ ràng,
    /// không âm thầm chạy trên schema mình không hiểu.
    #[test]
    fn tu_choi_db_tu_tuong_lai() {
        use rusqlite::Connection;
        let path = std::env::temp_dir().join(format!(
            "liva_future_schema_{}.sqlite",
            uuid::Uuid::new_v4()
        ));
        {
            let _ = DatabasePool::new(&path).expect("tao db");
            let c = Connection::open(&path).unwrap();
            c.execute_batch(&format!("PRAGMA user_version = {};", SCHEMA_VERSION + 5))
                .unwrap();
        }
        let res = DatabasePool::new(&path);
        assert!(res.is_err(), "db tu tuong lai phai bi tu choi");
        drop(res);
        for candidate in [
            path.clone(),
            std::path::PathBuf::from(format!("{}-wal", path.display())),
            std::path::PathBuf::from(format!("{}-shm", path.display())),
        ] {
            xoa_file_test(&candidate);
        }
    }

    /// Xoá file tạm của test, chịu được việc handle đóng TRỄ.
    ///
    /// **Vì sao cần retry thay vì `remove_file` một phát.** `DatabasePool::new`
    /// trả `Err`, nhưng r2d2 giữ pool sau một `Arc` dùng chung với thread bảo
    /// trì của nó — kết nối SQLite không nhất thiết đóng xong ngay tại thời điểm
    /// hàm trả về. Trên Windows, xoá một file còn handle mở là `os error 32`
    /// ("being used by another process"), nên bản một-phát ăn may theo tải máy:
    /// xanh trên máy dev, **đỏ trên CI** (`bc20eb1`, bước 19) trong khi cùng mã
    /// Rust đó vừa xanh ở `e6391eb` — cùng lớp "test nhấp nháy" đã cắn hai lần
    /// trước ở `speaker_queue_day_fail_fast` và `system_status_tests`.
    ///
    /// Vẫn **panic** nếu hết hạn: một handle bị rò VĨNH VIỄN là lỗi thật và
    /// test này là chỗ duy nhất bắt được nó. Retry chỉ nuốt độ trễ đóng, không
    /// nuốt rò rỉ.
    fn xoa_file_test(candidate: &std::path::Path) {
        const SO_LAN: u32 = 40;
        const NGHI: std::time::Duration = std::time::Duration::from_millis(50);
        for lan in 0..SO_LAN {
            match std::fs::remove_file(candidate) {
                Ok(()) => return,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => return,
                Err(error) if lan + 1 == SO_LAN => panic!(
                    "khong the don test DB {} sau {} lan trong {:?}: {error}",
                    candidate.display(),
                    SO_LAN,
                    NGHI * SO_LAN
                ),
                Err(_) => std::thread::sleep(NGHI),
            }
        }
    }

    #[test]
    fn test_database_pooling_and_wal() {
        let db_path =
            std::env::temp_dir().join(format!("liva_pooling_{}.sqlite", uuid::Uuid::new_v4()));

        {
            let pool = DatabasePool::new(&db_path).unwrap();
            let writer_conn = pool.writer.get().unwrap();
            let reader_conn = pool.readers.get().unwrap();

            // Check WAL mode
            let journal_mode: String = writer_conn
                .query_row("PRAGMA journal_mode", [], |row| row.get(0))
                .unwrap();
            assert_eq!(journal_mode.to_uppercase(), "WAL");

            // Check synchronous setting
            let synchronous: i64 = writer_conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            assert_eq!(synchronous, 1); // 1 = NORMAL

            // Check readers also have normal sync
            let reader_sync: i64 = reader_conn
                .query_row("PRAGMA synchronous", [], |row| row.get(0))
                .unwrap();
            assert_eq!(reader_sync, 1);
        }

        for candidate in [
            db_path.clone(),
            std::path::PathBuf::from(format!("{}-wal", db_path.display())),
            std::path::PathBuf::from(format!("{}-shm", db_path.display())),
        ] {
            match std::fs::remove_file(&candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => panic!("khong the don test DB {}: {error}", candidate.display()),
            }
        }
    }

    #[test]
    fn cau_hinh_connection_bat_lai_foreign_keys_va_cascade() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch("PRAGMA foreign_keys=OFF").unwrap();

        configure_connection(&conn, false).unwrap();

        let foreign_keys: i64 = conn
            .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
            .unwrap();
        assert_eq!(
            foreign_keys, 1,
            "contract không được phụ thuộc compile option"
        );

        conn.execute_batch(
            "
            CREATE TABLE skills (
                skill_id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                dir_path TEXT NOT NULL,
                updated_at INTEGER NOT NULL
            );
            CREATE TABLE skill_versions (
                version_id TEXT PRIMARY KEY,
                skill_id TEXT NOT NULL REFERENCES skills(skill_id) ON DELETE CASCADE,
                body TEXT NOT NULL,
                body_sha TEXT NOT NULL,
                created_at INTEGER NOT NULL
            );
            ",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO skills(skill_id, name, dir_path, updated_at)
             VALUES ('skill-fk', 'FK test', 'skills/fk', 1)",
            [],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO skill_versions(version_id, skill_id, body, body_sha, created_at)
             VALUES ('version-fk', 'skill-fk', 'body', 'sha', 1)",
            [],
        )
        .unwrap();
        conn.execute("DELETE FROM skills WHERE skill_id='skill-fk'", [])
            .unwrap();

        let versions: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM skill_versions WHERE skill_id='skill-fk'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(versions, 0, "ON DELETE CASCADE phải xóa version");
    }

    #[test]
    fn foreign_key_check_tu_choi_database_co_ban_ghi_mo_coi() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "
            PRAGMA foreign_keys=OFF;
            CREATE TABLE parent(id TEXT PRIMARY KEY);
            CREATE TABLE child(
                id TEXT PRIMARY KEY,
                parent_id TEXT NOT NULL REFERENCES parent(id)
            );
            INSERT INTO child(id, parent_id) VALUES ('orphan', 'missing');
            ",
        )
        .unwrap();

        let error = ensure_foreign_key_integrity(&conn).unwrap_err();
        assert!(
            error.to_string().contains("child"),
            "lỗi phải chỉ ra bảng vi phạm: {error}"
        );
    }

    #[test]
    fn test_facts_crud_and_encryption() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let conn = pool.writer.get().unwrap();

        let fact = Fact {
            key: "user_name".to_string(),
            value: "Alice".to_string(),
            createdAt: "2026-06-25T00:00:00Z".to_string(),
            updatedAt: "2026-06-25T00:00:00Z".to_string(),
            ttlDays: Some(30),
            source: "user_input".to_string(),
            category: Some("profile".to_string()),
            importance: 0.8,
            confidenceScore: 0.95,
            sourceTurnId: Some("turn_1".to_string()),
            memory_strength: 1.0,
            last_accessed_at: 0,
            access_count: 0,
        };

        // Save fact
        set_fact(&conn, &engine, &fact).unwrap();

        // Query directly to check that it is encrypted
        let raw_val: String = conn
            .query_row(
                "SELECT value FROM facts WHERE key = 'user_name'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_ne!(raw_val, "Alice");
        // set_fact nay mã hoá định dạng v2: "v2:salt:iv:tag:cipher" (5 phần).
        assert!(
            raw_val.starts_with("v2:"),
            "value trên đĩa phải là ciphertext v2"
        );
        assert_eq!(raw_val.split(':').count(), 5);

        // Retrieve using get_fact, should be decrypted
        let retrieved = get_fact(&conn, &engine, "user_name").unwrap().unwrap();
        assert_eq!(retrieved.value, "Alice");
        assert_eq!(retrieved.source, "user_input");
        assert_eq!(retrieved.importance, 0.8);
    }

    #[test]
    fn conversation_turn_tao_ledger_va_vector_cung_lineage_scope() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("conversation-ledger-key-32-bytes");
        let vector = vec![0.25; MEMORY_VECTOR_DIM];
        let turn_id = "turn_ledger_1";
        let domain = "memory_owner:telegram:100";
        let category = "conversation:telegram_chat:-200";
        let content = "Người dùng: nhớ mã ORION-7\nLIVA: Tôi đã ghi nhớ.";

        persist_conversation_event_vector(
            &conn, &engine, turn_id, content, &vector, domain, category,
        )
        .unwrap();

        let event: (String, String, String, String, bool, bool) = conn
            .query_row(
                "SELECT eventId, consolidation_status, domain, category, \
                        rawUserMsg IS NULL, rawAiReply IS NULL \
                 FROM events WHERE eventId = ?1",
                [turn_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                        row.get(5)?,
                    ))
                },
            )
            .unwrap();
        let memory: (String, String, String, String, String) = conn
            .query_row(
                "SELECT vec_id, content, domain, category, source_event_ids \
                 FROM vectors_meta WHERE vec_id = ?1",
                [turn_id],
                |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                },
            )
            .unwrap();

        assert_eq!(
            event,
            (
                turn_id.to_string(),
                "pending".to_string(),
                domain.to_string(),
                category.to_string(),
                true,
                true,
            )
        );
        assert_eq!(memory.0, turn_id);
        assert_ne!(memory.1, content);
        assert!(memory.1.starts_with("v2:"));
        assert_eq!(engine.decrypt(&memory.1), content);
        assert_eq!(memory.2, domain);
        assert_eq!(memory.3, category);
        assert_eq!(memory.4, r#"["turn_ledger_1"]"#);
    }

    #[test]
    fn conversation_turn_ma_hoa_content_bo_fts_nhung_dense_recall_van_doc_duoc() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("conversation-memory-key-32-bytes");
        let vector = vec![0.25; MEMORY_VECTOR_DIM];
        let canary = "LIVA-CONVERSATION-CANARY-8842";

        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_encrypted_1",
            canary,
            &vector,
            "memory_owner:local",
            "conversation:default",
        )
        .unwrap();

        let raw: String = conn
            .query_row(
                "SELECT content FROM vectors_meta WHERE vec_id = 'turn_encrypted_1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(!raw.contains(canary));
        assert!(raw.starts_with("v2:"));
        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vectors_fts WHERE rowid = (
                    SELECT id FROM vectors_meta WHERE vec_id = 'turn_encrypted_1'
                )",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0, "conversation plaintext không được đi vào FTS");

        let filter = MetadataFilter {
            r#type: Some("conversation_turn".to_string()),
            domain: Some("memory_owner:local".to_string()),
            category: None,
            created_after: None,
            created_before: None,
        };
        let hits = search_similar_vectors(&conn, &engine, &vector, 5, &filter).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].content, canary);
    }

    #[test]
    fn conversation_turn_rollback_toan_bo_khi_vector_khong_ghi_duoc() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("conversation-rollback-key-32");
        let invalid_vector = vec![0.25; MEMORY_VECTOR_DIM - 1];

        let result = persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_atomic_failure",
            "Người dùng: dữ liệu không được ghi nửa vời\nLIVA: đã rõ.",
            &invalid_vector,
            "memory_owner:local",
            "conversation:default",
        );
        assert!(
            result.is_err(),
            "vector sai chiều phải làm lượt ghi thất bại"
        );

        let counts = (
            conn.query_row("SELECT count(*) FROM events", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vectors_meta", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vectors_fts", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
            conn.query_row("SELECT count(*) FROM vec_idx", [], |row| {
                row.get::<_, i64>(0)
            })
            .unwrap(),
        );
        assert_eq!(
            counts,
            (0, 0, 0, 0),
            "event, metadata, FTS và vec0 phải rollback cùng nhau"
        );
    }

    #[test]
    fn test_vector_and_fts_and_hybrid_search() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("vector-search-test-key-32-bytes");

        // 1. Insert vector meta and vectors
        let vec1 = vec![1.0; 384];
        let vec2 = vec![-1.0; 384];

        upsert_vector(
            &conn,
            &engine,
            "v1",
            "fact",
            "the quick brown fox jumps over the lazy dog",
            &vec1,
            Some("test_domain"),
            Some("test_category"),
            Some(&["fox".to_string()]),
            None,
            None,
        )
        .unwrap();

        upsert_vector(
            &conn,
            &engine,
            "v2",
            "fact",
            "the quiet white cat sleeps on the red rug",
            &vec2,
            Some("test_domain"),
            Some("test_category"),
            Some(&["cat".to_string()]),
            None,
            None,
        )
        .unwrap();

        let filter = MetadataFilter {
            r#type: None,
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };

        // 2. Test FTS Search
        let fts_results = search_fts_vectors(&conn, "fox", 5, &filter).unwrap();
        assert_eq!(fts_results.len(), 1);
        assert_eq!(fts_results[0].vec_id, "v1");

        // 3. Test Vector Search
        let vector_results = search_similar_vectors(&conn, &engine, &vec1, 5, &filter).unwrap();
        assert_eq!(vector_results.len(), 2);
        assert_eq!(vector_results[0].vec_id, "v1");
        assert!(vector_results[0].score > vector_results[1].score);

        // 4. Test Hybrid Search
        let hybrid_results =
            search_hybrid_vectors(&conn, &engine, "white cat", &vec2, 5, &filter, 1.0, 1.0)
                .unwrap();
        assert_eq!(hybrid_results.len(), 2);
        // "white cat" and vec2 are closest to v2
        assert_eq!(hybrid_results[0].vec_id, "v2");

        // 5. Test FTS and Hybrid search with non-empty MetadataFilter
        let filter_non_empty = MetadataFilter {
            r#type: Some("fact".to_string()),
            domain: Some("test_domain".to_string()),
            category: Some("test_category".to_string()),
            created_after: None,
            created_before: None,
        };

        let fts_filtered = search_fts_vectors(&conn, "fox", 5, &filter_non_empty).unwrap();
        assert_eq!(fts_filtered.len(), 1);
        assert_eq!(fts_filtered[0].vec_id, "v1");

        let hybrid_filtered = search_hybrid_vectors(
            &conn,
            &engine,
            "white cat",
            &vec2,
            5,
            &filter_non_empty,
            1.0,
            1.0,
        )
        .unwrap();
        assert_eq!(hybrid_filtered.len(), 2);
        assert_eq!(hybrid_filtered[0].vec_id, "v2");

        // Test with mismatching filter to verify it filters out results
        let filter_mismatch = MetadataFilter {
            r#type: Some("not_matching".to_string()),
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };
        let fts_mismatched = search_fts_vectors(&conn, "fox", 5, &filter_mismatch).unwrap();
        assert_eq!(fts_mismatched.len(), 0);
    }

    #[tokio::test]
    async fn test_sqlite_wal_concurrency_stress() {
        use std::sync::Arc;

        let db_path = "temp_test_concurrency.sqlite";
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));

        let pool = Arc::new(DatabasePool::new(db_path).unwrap());
        let engine = Arc::new(EncryptionEngine::new("00000000000000000000000000000000"));

        // Pre-populate some facts to read
        {
            let conn = pool.writer.get().unwrap();
            for i in 0..50 {
                let fact = Fact {
                    key: format!("key_{}", i),
                    value: format!("value_{}", i),
                    createdAt: "2026-06-25T00:00:00Z".to_string(),
                    updatedAt: "2026-06-25T00:00:00Z".to_string(),
                    ttlDays: Some(30),
                    source: "stress_test".to_string(),
                    category: Some("test".to_string()),
                    importance: 0.5,
                    confidenceScore: 1.0,
                    sourceTurnId: None,
                    memory_strength: 1.0,
                    last_accessed_at: 0,
                    access_count: 0,
                };
                set_fact(&conn, &engine, &fact).unwrap();
            }
        }

        // Spawn 100 concurrent reads and 10 writes
        let mut handles = vec![];

        // 10 concurrent writes
        for w in 0..10 {
            let pool_clone = pool.clone();
            let engine_clone = engine.clone();
            let handle = tokio::spawn(async move {
                for i in 0..5 {
                    let fact = Fact {
                        key: format!("write_{}_{}", w, i),
                        value: format!("new_value_{}_{}", w, i),
                        createdAt: "2026-06-25T00:00:00Z".to_string(),
                        updatedAt: "2026-06-25T00:00:00Z".to_string(),
                        ttlDays: Some(30),
                        source: "stress_test".to_string(),
                        category: Some("test".to_string()),
                        importance: 0.5,
                        confidenceScore: 1.0,
                        sourceTurnId: None,
                        memory_strength: 1.0,
                        last_accessed_at: 0,
                        access_count: 0,
                    };
                    let pool_c = pool_clone.clone();
                    let engine_c = engine_clone.clone();
                    let fact_c = fact.clone();
                    tokio::task::spawn_blocking(move || {
                        let conn = pool_c.writer.get().expect("Failed to get write connection");
                        set_fact(&conn, &engine_c, &fact_c).expect("Failed to set fact");
                    })
                    .await
                    .expect("spawn_blocking failed");
                    tokio::time::sleep(tokio::time::Duration::from_millis(5)).await;
                }
            });
            handles.push(handle);
        }

        // 100 concurrent reads
        for r in 0..100 {
            let pool_clone = pool.clone();
            let engine_clone = engine.clone();
            let handle = tokio::spawn(async move {
                for i in 0..10 {
                    let key = format!("key_{}", (r + i) % 50);
                    let pool_c = pool_clone.clone();
                    let engine_c = engine_clone.clone();
                    let fact = tokio::task::spawn_blocking(move || {
                        let conn = pool_c.readers.get().expect("Failed to get read connection");
                        get_fact(&conn, &engine_c, &key).expect("Failed to get fact")
                    })
                    .await
                    .expect("spawn_blocking failed");
                    assert!(fact.is_some());
                    tokio::time::sleep(tokio::time::Duration::from_millis(2)).await;
                }
            });
            handles.push(handle);
        }

        // Wait for all tasks to complete
        for handle in handles {
            handle.await.expect("Task failed");
        }

        // Verify database contents
        {
            let conn = pool.writer.get().unwrap();
            let retrieved = get_fact(&conn, &engine, "write_9_4").unwrap().unwrap();
            assert_eq!(retrieved.value, "new_value_9_4");
        }

        // Clean up
        drop(pool);
        let _ = std::fs::remove_file(db_path);
        let _ = std::fs::remove_file(format!("{}-wal", db_path));
        let _ = std::fs::remove_file(format!("{}-shm", db_path));
    }

    /// Guard chiều vector: dùng embedding của model chat (2048 chiều với
    /// Qwen3-VL-2B) thay vì model embedding chuyên dụng phải bị chặn NGAY tại
    /// hàm gọi, kèm thông báo chỉ ra nguyên nhân — không để lỗi nổ ở tận câu
    /// SQL với thông báo không nói được vector sai từ đâu ra.
    #[test]
    fn guard_chan_vector_sai_chieu_va_chi_ra_nguyen_nhan() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("vector-dimension-test-key-32");
        let filter = MetadataFilter {
            r#type: None,
            domain: None,
            category: None,
            created_after: None,
            created_before: None,
        };

        // n_embd cua Qwen3-VL-2B
        let sai = vec![0.01f32; 2048];
        let e = upsert_vector(
            &conn, &engine, "v1", "fact", "noi dung", &sai, None, None, None, None, None,
        )
        .unwrap_err()
        .to_string();
        assert!(e.contains("2048"), "phai neu so chieu that: {e}");
        assert!(e.contains("384"), "phai neu so chieu can: {e}");
        assert!(
            e.contains("EmbeddingEngine"),
            "phai chi ra cach dung dung: {e}"
        );

        let e2 = search_similar_vectors(&conn, &engine, &sai, 5, &filter)
            .unwrap_err()
            .to_string();
        assert!(
            e2.contains("search_similar_vectors"),
            "phai neu ten ham: {e2}"
        );

        // search_hybrid_vectors di qua search_similar_vectors nen cung bi chan
        assert!(search_hybrid_vectors(&conn, &engine, "q", &sai, 5, &filter, 1.0, 1.0).is_err());

        // Vector rong va vector thieu 1 chieu deu phai bi chan
        assert!(
            upsert_vector(
                &conn,
                &engine,
                "v2",
                "fact",
                "x",
                &[],
                None,
                None,
                None,
                None,
                None
            )
            .is_err()
        );
        let thieu = vec![0.01f32; MEMORY_VECTOR_DIM - 1];
        assert!(
            upsert_vector(
                &conn, &engine, "v3", "fact", "x", &thieu, None, None, None, None, None
            )
            .is_err()
        );

        // Dung chieu thi qua
        let dung = vec![0.01f32; MEMORY_VECTOR_DIM];
        assert!(
            upsert_vector(
                &conn, &engine, "v4", "fact", "x", &dung, None, None, None, None, None
            )
            .is_ok()
        );
    }
}
