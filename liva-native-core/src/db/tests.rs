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

    #[test]
    fn test_database_pool_with_reader_and_writer() {
        let pool = DatabasePool::new_in_memory().unwrap();

        pool.with_writer(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS pool_test (id INTEGER PRIMARY KEY, val TEXT)",
                [],
            )?;
            conn.execute("INSERT INTO pool_test (val) VALUES ('sync_writer')", [])?;
            Ok(())
        })
        .unwrap();

        let val: String = pool
            .with_reader(|conn| {
                conn.query_row("SELECT val FROM pool_test WHERE id = 1", [], |r| r.get(0))
            })
            .unwrap();

        assert_eq!(val, "sync_writer");
    }

    #[tokio::test]
    async fn test_database_pool_spawn_reader_and_writer() {
        let pool = DatabasePool::new_in_memory().unwrap();

        pool.spawn_writer(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS pool_async_test (id INTEGER PRIMARY KEY, val TEXT)",
                [],
            )?;
            conn.execute(
                "INSERT INTO pool_async_test (val) VALUES ('async_writer')",
                [],
            )?;
            Ok(())
        })
        .await
        .unwrap();

        let val: String = pool
            .spawn_reader(|conn| {
                conn.query_row("SELECT val FROM pool_async_test WHERE id = 1", [], |r| {
                    r.get(0)
                })
            })
            .await
            .unwrap();

        assert_eq!(val, "async_writer");
    }

    #[test]
    fn test_temp_store_pragma_is_memory() {
        let pool = DatabasePool::new_in_memory().unwrap();
        let temp_store: i64 = pool
            .with_reader(|conn| conn.query_row("PRAGMA temp_store", [], |row| row.get(0)))
            .unwrap();
        // 2 corresponds to MEMORY in SQLite
        assert_eq!(temp_store, 2, "temp_store should be set to MEMORY (2)");
    }

    #[test]
    fn test_reader_pool_size_boundaries() {
        assert_eq!(get_reader_pool_size(), 4);
    }

    #[tokio::test]
    async fn test_database_pool_concurrent_readers() {
        let pool = DatabasePool::new_in_memory().unwrap();

        pool.spawn_writer(|conn| {
            conn.execute(
                "CREATE TABLE IF NOT EXISTS concurrent_test (id INTEGER PRIMARY KEY, count INTEGER)",
                [],
            )?;
            conn.execute("INSERT INTO concurrent_test (id, count) VALUES (1, 42)", [])?;
            Ok(())
        })
        .await
        .unwrap();

        let mut handles = Vec::new();
        for _ in 0..8 {
            let pool_clone = pool.clone();
            handles.push(tokio::spawn(async move {
                pool_clone
                    .spawn_reader(|conn| {
                        conn.query_row("SELECT count FROM concurrent_test WHERE id = 1", [], |r| {
                            r.get::<_, i64>(0)
                        })
                    })
                    .await
            }));
        }

        for handle in handles {
            let res = handle.await.unwrap();
            assert_eq!(res.unwrap(), 42);
        }
    }

    #[test]
    fn test_ebbinghaus_decay_fresh_vs_old_vs_reinforced() {
        use crate::crypto::EncryptionEngine;
        use crate::db::{
            DatabasePool, MEMORY_VECTOR_DIM, MetadataFilter, persist_conversation_event_vector,
            reinforce_memory_access, search_similar_vectors,
        };

        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("test-ebbinghaus-key-32-bytes!!");

        let dummy_vector = vec![0.1f32; MEMORY_VECTOR_DIM];
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let thirty_days_ms = 30 * 86_400_000i64;

        // 1. Fresh memory (created now)
        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_fresh",
            "Nội dung mới hôm nay",
            &dummy_vector,
            "local",
            "default",
        )
        .unwrap();

        // 2. Old memory from 30 days ago (access_count = 0)
        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_old",
            "Nội dung từ 30 ngày trước",
            &dummy_vector,
            "local",
            "default",
        )
        .unwrap();
        conn.execute(
            "UPDATE vectors_meta SET created_at = ?1, last_accessed_at = 0, access_count = 0 WHERE vec_id = 'turn_old'",
            rusqlite::params![now_ms - thirty_days_ms],
        )
        .unwrap();

        // 3. Old memory from 30 days ago but reinforced 10 times (access_count = 10)
        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_reinforced",
            "Nội dung quan trọng được củng cố nhiều lần",
            &dummy_vector,
            "local",
            "default",
        )
        .unwrap();
        conn.execute(
            "UPDATE vectors_meta SET created_at = ?1, last_accessed_at = 0, access_count = 10 WHERE vec_id = 'turn_reinforced'",
            rusqlite::params![now_ms - thirty_days_ms],
        )
        .unwrap();

        let filter = MetadataFilter::default();
        let hits = search_similar_vectors(&conn, &engine, &dummy_vector, 5, &filter).unwrap();
        assert_eq!(hits.len(), 3);

        let fresh_hit = hits.iter().find(|h| h.vec_id == "turn_fresh").unwrap();
        let old_hit = hits.iter().find(|h| h.vec_id == "turn_old").unwrap();
        let reinforced_hit = hits.iter().find(|h| h.vec_id == "turn_reinforced").unwrap();

        // Fresh memory score should be higher than 30-day-old memory score
        assert!(fresh_hit.score > old_hit.score);

        // Ratio of old memory to fresh memory score should be close to exp(-1.0) ~= 0.368
        let decay_ratio = old_hit.score / fresh_hit.score;
        assert!(
            (decay_ratio - 0.368).abs() < 0.05,
            "Decay ratio {} should be close to 0.368",
            decay_ratio
        );

        // Reinforced memory should decay slower: reinforced score > unreinforced old score
        assert!(
            reinforced_hit.score > old_hit.score,
            "Reinforced memory score ({}) should be higher than old memory score ({})",
            reinforced_hit.score,
            old_hit.score
        );

        // Test dynamic reinforcement update
        let updated = reinforce_memory_access(&conn, &["turn_old"], now_ms).unwrap();
        assert_eq!(updated, 1);

        let (last_acc, acc_cnt): (i64, i64) = conn
            .query_row(
                "SELECT last_accessed_at, access_count FROM vectors_meta WHERE vec_id = 'turn_old'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .unwrap();
        assert_eq!(last_acc, now_ms);
        assert_eq!(acc_cnt, 1);
    }

    #[test]
    fn test_l3_csr_graph_database_sync_and_ppr() {
        use crate::db::{DatabasePool, insert_l3_edge_sync, insert_l3_node_sync};

        let pool = DatabasePool::new_in_memory().unwrap();

        // Insert nodes and edges via synchronized functions
        insert_l3_node_sync(
            &pool,
            "project_liva",
            "Project LIVA",
            "{\"type\":\"system\"}",
        )
        .unwrap();

        insert_l3_node_sync(
            &pool,
            "hipporag_engine",
            "HippoRAG Memory Engine",
            "{\"layer\":\"L3\"}",
        )
        .unwrap();

        insert_l3_edge_sync(
            &pool,
            "project_liva",
            "hipporag_engine",
            "incorporates",
            1.0,
        )
        .unwrap();

        // Verify SQLite table contents
        let conn = pool.writer.get().unwrap();
        let node_count: i64 = conn
            .query_row("SELECT count(*) FROM l3_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(node_count, 2);

        let edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM l3_edges WHERE obsolete = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 1);

        // Verify in-memory CSR graph cache was updated synchronously
        let graph = pool.csr_graph.read().unwrap();
        assert_eq!(graph.node_count(), 2);
        assert_eq!(graph.edge_count(), 2); // Bidirectional in CSR

        let seeds = [("project_liva", 1.0f32)];
        let ppr = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 2);
        assert_eq!(ppr.len(), 2);

        let node_ids: Vec<String> = ppr.into_iter().map(|(id, _)| id).collect();
        assert!(node_ids.contains(&"project_liva".to_string()));
        assert!(node_ids.contains(&"hipporag_engine".to_string()));
    }

    #[tokio::test]
    async fn test_insert_l3_triples_batch_and_wal_pragmas() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");
        let conn = pool.readers.get().expect("get reader connection");

        // Verify cache_size PRAGMA is tuned (-2000) and page_size is 4096
        let cache_size: i64 = conn
            .query_row("PRAGMA cache_size", [], |r| r.get(0))
            .expect("query cache_size");
        assert_eq!(cache_size, -2000, "cache_size should be tuned to -2000");

        let page_size: i64 = conn
            .query_row("PRAGMA page_size", [], |r| r.get(0))
            .expect("query page_size");
        assert_eq!(page_size, 4096, "page_size should be tuned to 4096");

        let triples = vec![
            (
                "alice".to_string(),
                "knows".to_string(),
                "bob".to_string(),
                0.9f32,
            ),
            (
                "bob".to_string(),
                "works_at".to_string(),
                "company".to_string(),
                0.8f32,
            ),
            (
                "alice".to_string(),
                "founded".to_string(),
                "company".to_string(),
                1.0f32,
            ),
        ];

        pool.insert_l3_triples_batch(&triples)
            .await
            .expect("insert_l3_triples_batch should succeed");

        let node_count: i64 = conn
            .query_row("SELECT count(*) FROM l3_nodes", [], |r| r.get(0))
            .unwrap();
        assert_eq!(node_count, 3);

        let edge_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM l3_edges WHERE obsolete = 0",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(edge_count, 3);

        let graph = pool.csr_graph.read().unwrap();
        assert_eq!(graph.node_count(), 3);
        assert_eq!(graph.edge_count(), 6); // Bidirectional in CSR
    }

    #[test]
    fn test_turn_telemetry_schema_and_migration_10() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");
        let conn = pool.writer.get().expect("get connection");

        // 1. Kiểm tra PRAGMA user_version >= 10
        let version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .expect("query PRAGMA user_version");
        assert!(
            version >= 10,
            "PRAGMA user_version phải ít nhất là 10 sau khi bổ sung turn_telemetry"
        );

        // 2. Kiểm tra bảng turn_telemetry tồn tại
        let table_exists: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='table' AND name='turn_telemetry'",
                [],
                |r| r.get(0),
            )
            .expect("check table existence");
        assert_eq!(
            table_exists, 1,
            "Bảng turn_telemetry phải được tạo thành công"
        );

        // 3. Kiểm tra các index hỗ trợ tối ưu truy vấn
        let index_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_master WHERE type='index' AND name IN ('idx_telemetry_ts', 'idx_telemetry_model')",
                [],
                |r| r.get(0),
            )
            .expect("check index existence");
        assert_eq!(
            index_count, 2,
            "Cả 2 index idx_telemetry_ts và idx_telemetry_model phải tồn tại"
        );

        // 4. Test ghi và đọc record trực tiếp
        let record = TurnTelemetryRecord {
            id: None,
            event_id: Some("test-m10".to_string()),
            ts: 123456,
            entry_path: "chat".to_string(),
            model_id: "qwen-test".to_string(),
            prompt_tokens: 10,
            completion_tokens: 20,
            latency_ms: 150,
            outcome: "ok".to_string(),
            err_kind: None,
        };
        let row_id = record_turn_telemetry(&conn, &record).expect("insert record");
        assert!(row_id > 0);

        let summary = get_telemetry_summary(&conn, None).expect("get summary");
        assert_eq!(summary.total_turns, 1);
        assert_eq!(summary.avg_latency_ms, 150.0);
    }

    #[test]
    fn test_ebbinghaus_decay_ancient_memory_floor_clamp() {
        use crate::crypto::EncryptionEngine;
        use crate::db::{
            DatabasePool, MEMORY_VECTOR_DIM, MetadataFilter, persist_conversation_event_vector,
            search_similar_vectors,
        };

        let pool = DatabasePool::new_in_memory().unwrap();
        let conn = pool.writer.get().unwrap();
        let engine = EncryptionEngine::new("test-ebbinghaus-floor-clamp-32b!");

        let dummy_vector = vec![0.1f32; MEMORY_VECTOR_DIM];
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;
        let one_year_ms = 365 * 86_400_000i64;

        // 1. Fresh memory (created now)
        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_fresh",
            "Nội dung mới hôm nay",
            &dummy_vector,
            "local",
            "default",
        )
        .unwrap();

        // 2. Ancient memory from 365 days ago (access_count = 0)
        persist_conversation_event_vector(
            &conn,
            &engine,
            "turn_ancient",
            "Nội dung từ 1 năm trước",
            &dummy_vector,
            "local",
            "default",
        )
        .unwrap();
        conn.execute(
            "UPDATE vectors_meta SET created_at = ?1, last_accessed_at = 0, access_count = 0 WHERE vec_id = 'turn_ancient'",
            rusqlite::params![now_ms - one_year_ms],
        )
        .unwrap();

        let filter = MetadataFilter::default();
        let hits = search_similar_vectors(&conn, &engine, &dummy_vector, 5, &filter).unwrap();
        assert_eq!(hits.len(), 2);

        let fresh_hit = hits.iter().find(|h| h.vec_id == "turn_fresh").unwrap();
        let ancient_hit = hits.iter().find(|h| h.vec_id == "turn_ancient").unwrap();

        // Without clamp(0.05, 1.0):
        // delta_days = 365, tau = 30 => exp(-365/30) = exp(-12.1667) ~= 0.00000517
        // With clamp(0.05, 1.0):
        // decay_factor = 0.05 exactly.
        let decay_ratio = ancient_hit.score / fresh_hit.score;
        assert!(
            (decay_ratio - 0.05).abs() < 0.001,
            "Ancient memory decay ratio {} must be clamped to lower floor 0.05",
            decay_ratio
        );
        assert!(
            ancient_hit.score >= 0.049,
            "Ancient memory score {} must not vanish below the 0.05 floor",
            ancient_hit.score
        );
    }

    #[test]
    fn test_calibrated_schema_secondary_indexes() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");
        let conn = pool.readers.get().expect("get reader connection");

        let check_index = |table: &str, index_name: &str| -> bool {
            let mut stmt = conn
                .prepare(&format!("PRAGMA index_list('{}')", table))
                .expect("prepare index_list");
            let rows = stmt
                .query_map([], |row| row.get::<_, String>(1))
                .expect("query index_list");
            for idx in rows.flatten() {
                if idx == index_name {
                    return true;
                }
            }
            false
        };

        assert!(
            check_index("events", "idx_events_timestamp"),
            "idx_events_timestamp must exist"
        );
        assert!(
            check_index("facts", "idx_facts_last_accessed"),
            "idx_facts_last_accessed must exist"
        );
        assert!(
            check_index("tasks", "idx_tasks_status_created"),
            "idx_tasks_status_created must exist"
        );
        assert!(
            check_index("l3_edges", "idx_l3_edges_subject"),
            "idx_l3_edges_subject must exist"
        );
        assert!(
            check_index("l3_edges", "idx_l3_edges_object"),
            "idx_l3_edges_object must exist"
        );
    }

    #[tokio::test]
    async fn test_db_actor_micro_batch_coalesce_and_atomic_rollback() {
        let pool = DatabasePool::new_in_memory().expect("create in-memory db");

        let mut handles = Vec::new();
        for i in 0..10 {
            let pool_clone = pool.writer_actor.clone();
            let h = tokio::spawn(async move {
                pool_clone
                    .execute(move |conn| {
                        conn.execute(
                            "INSERT INTO tasks (id, title, created_at, updated_at) VALUES (?1, ?2, 1000, 1000)",
                            rusqlite::params![format!("task_{}", i), format!("Title {}", i)],
                        )
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                    })
                    .await
            });
            handles.push(h);
        }

        for h in handles {
            let res = h.await.expect("join handle");
            assert!(res.is_ok(), "task write should succeed: {:?}", res);
        }

        pool.writer_actor.flush().await.expect("flush actor");

        {
            let conn = pool.readers.get().expect("reader conn");
            let count: i64 = conn
                .query_row("SELECT count(*) FROM tasks", [], |r| r.get(0))
                .expect("count tasks");
            assert_eq!(count, 10, "all 10 batched tasks should be persisted");
        }

        let fail_res = pool
            .writer_actor
            .execute(|conn| {
                conn.execute("INSERT INTO non_existent_table_xyz VALUES (1)", [])
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })
            .await;

        assert!(
            fail_res.is_err(),
            "operation on non-existent table must fail and trigger rollback"
        );
    }
}
