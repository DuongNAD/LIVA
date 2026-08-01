use super::*;

#[cfg(test)]
mod encryption_migration_tests {
    use super::*;
    use crate::crypto::EncryptionEngine;

    /// Dựng một fact v1 (định dạng cũ) thật, chạy migration, kiểm: (1) nội dung
    /// giữ nguyên, (2) trên đĩa nay là v2, (3) idempotent.
    #[test]
    fn migrate_v1_len_v2_khong_mat_du_lieu() {
        use aes_gcm::aead::consts::U16;
        use aes_gcm::{
            AesGcm, Nonce,
            aead::{Aead, KeyInit},
        };
        type G = AesGcm<aes_gcm::aes::Aes256, U16>;

        let key_str = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key_str);
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        let mut raw = [0u8; 32];
        let kb = key_str.as_bytes();
        raw[..kb.len().min(32)].copy_from_slice(&kb[..kb.len().min(32)]);
        let iv = [3u8; 16];
        let ct = G::new_from_slice(&raw)
            .unwrap()
            .encrypt(Nonce::<U16>::from_slice(&iv), b"meo ten Bun".as_ref())
            .unwrap();
        let (c, tag) = ct.split_at(ct.len() - 16);
        let v1 = format!(
            "{}:{}:{}",
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(c)
        );
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd', 'd', 't')",
            [&v1],
        ).unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('p', 'plaintext-cu', 'd', 'd', 't')",
            [],
        ).unwrap();

        let (nang, khong) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang, 1);
        assert_eq!(khong, 0);

        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(on_disk.starts_with("v2:"));
        assert_eq!(
            get_fact(&conn, &engine, "k").unwrap().unwrap().value,
            "meo ten Bun"
        );
        let p: String = conn
            .query_row("SELECT value FROM facts WHERE key='p'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(p, "plaintext-cu");

        let (nang2, _) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang2, 0);
    }

    /// Guard chống lost-update: UPDATE có điều kiện `value = bản đã đọc` phải
    /// khớp 0 dòng khi value đã bị đổi (tiến trình khác ghi giữa chừng), tức
    /// KHÔNG đè mất bản mới. Kiểm chính cơ chế SQL mà migration dựa vào.
    #[test]
    fn guard_khong_de_mat_ban_moi() {
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', 'V_MOI', 'd', 'd', 't')",
            [],
        ).unwrap();

        // Migration đã đọc 'V_CU' rồi mới ghi — nhưng trên đĩa nay là 'V_MOI'.
        // UPDATE với guard value='V_CU' phải khớp 0 dòng.
        let n = conn
            .execute(
                "UPDATE facts SET value=?1 WHERE key='k' AND value=?2",
                ("V2_CU", "V_CU"),
            )
            .unwrap();
        assert_eq!(n, 0, "value đã đổi -> guard phải chặn, không đè");
        let con_nguyen: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(con_nguyen, "V_MOI", "bản mới phải được GIỮ");

        // Khi value còn đúng bản đã đọc -> UPDATE khớp 1 dòng.
        let n2 = conn
            .execute(
                "UPDATE facts SET value=?1 WHERE key='k' AND value=?2",
                ("V2_MOI", "V_MOI"),
            )
            .unwrap();
        assert_eq!(n2, 1);
    }

    /// Dữ liệu hỏng/sai khoá KHÔNG được đụng (mã hoá lại rác = mất bản gốc).
    #[test]
    fn migrate_khong_dung_du_lieu_hong() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let rac = format!(
            "{}:{}:{}",
            hex::encode([1u8; 16]),
            hex::encode([2u8; 16]),
            hex::encode([3u8; 16])
        );
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('x', ?1, 'd', 'd', 't')",
            [&rac],
        ).unwrap();

        let (nang, khong) = migrate_facts_encryption(&conn, &engine).unwrap();
        assert_eq!(nang, 0);
        assert_eq!(khong, 1);
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='x'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on_disk, rac);
    }

    /// KỊCH BẢN CỐT LÕI của "bỏ khoá mặc định": fact đang mã hoá bằng khoá MẶC
    /// ĐỊNH ("0"×32, giống máy dev) phải được rekey sang khoá THẬT tại chỗ, đọc
    /// lại nguyên vẹn, và sau đó khoá mặc định KHÔNG mở được nữa. Idempotent.
    #[test]
    fn rekey_chuyen_fact_tu_khoa_mac_dinh_sang_khoa_that() {
        let default_engine = EncryptionEngine::new(crate::crypto::DEFAULT_ENCRYPTION_KEY);
        let live = EncryptionEngine::new("khoa-that-su-bi-mat-cua-Duong-99");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        let enc_default = default_engine.encrypt("mèo tên Bún").unwrap();
        assert!(enc_default.starts_with("v2:"), "khoá mặc định cũng sinh v2");
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc_default],
        ).unwrap();
        // Trước rekey: khoá thật KHÔNG mở được (đây là ca 'v2 nhưng live sai khoá').
        assert!(live.read_fact(&enc_default).is_locked());

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!((so, khong), (1, 0), "1 fact chuyển sang khoá thật, 0 mất");

        // Đọc được bằng khoá THẬT; khoá mặc định KHÔNG còn mở được.
        assert_eq!(
            get_fact(&conn, &live, "k").unwrap().unwrap().value,
            "mèo tên Bún"
        );
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(
            default_engine.read_fact(&on_disk).is_locked(),
            "sau rekey, khoá mặc định phải hết mở được"
        );

        // Idempotent: đã v2 + live giải được -> lần hai không rekey gì.
        let (so2, _) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!(so2, 0, "chạy lại không rekey (idempotent)");
    }

    /// Fact đã ở khoá live rồi thì KHÔNG đụng (không đổi salt vô ích) — tiêu chí
    /// idempotent là 'v2 + live giải được', không phải chỉ tiền tố v2.
    #[test]
    fn rekey_de_nguyen_fact_da_o_khoa_live() {
        let live = EncryptionEngine::new("khoa-that-su-bi-mat-cua-Duong-99");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let enc = live.encrypt("đã ở khoá live").unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc],
        ).unwrap();
        let truoc: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[]).unwrap();
        assert_eq!((so, khong), (0, 0));
        let sau: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(truoc, sau, "đã ở khoá live -> giữ nguyên byte");
    }

    /// Không khoá nào (live + phụ) mở được -> KHÔNG đụng bản gốc + đếm. Chống
    /// mã hoá lại rác làm mất dữ liệu người dùng khoá thật khác/ dữ liệu hỏng.
    #[test]
    fn rekey_de_nguyen_fact_khong_khoa_nao_mo_duoc() {
        let live = EncryptionEngine::new("khoa-live-aaaaaaaaaaaaaaaaaaaaaa");
        let unknown = EncryptionEngine::new("khoa-la-khong-ai-biet-bbbbbbbbbb");
        let default_engine = EncryptionEngine::new(crate::crypto::DEFAULT_ENCRYPTION_KEY);
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let enc_unknown = unknown.encrypt("mã bằng khoá lạ").unwrap();
        conn.execute(
            "INSERT INTO facts (key, value, createdAt, updatedAt, source) VALUES ('k', ?1, 'd','d','t')",
            [&enc_unknown],
        ).unwrap();

        let (so, khong) = rekey_facts_encryption(&conn, &live, &[&default_engine]).unwrap();
        assert_eq!(
            (so, khong),
            (0, 1),
            "không khoá nào mở được -> đếm 1, không rekey"
        );
        let on_disk: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert_eq!(on_disk, enc_unknown, "bản gốc GIỮ NGUYÊN, không mã lại rác");
    }

    #[test]
    fn rekey_du_lieu_ca_nhan_ma_hoa_plaintext_va_khoa_cu_bo_fts() {
        let live = EncryptionEngine::new("personal-data-live-key-32-bytes");
        let old = EncryptionEngine::new("personal-data-old-key-32-bytes-");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        let checkpoint_plain = r#"{"messages":[{"role":"user","content":"CHECKPOINT-CANARY"}]}"#;
        let conversation_plain = "CONVERSATION-CANARY-OLD-KEY";
        let conversation_old = old.encrypt(conversation_plain).unwrap();

        conn.execute(
            "INSERT INTO agent_checkpoints (thread_id, state_json) VALUES ('thread-1', ?1)",
            [checkpoint_plain],
        )
        .unwrap();
        conn.execute(
            "INSERT INTO vectors_meta (
                vec_id, type, content, domain, category, created_at
             ) VALUES ('turn-1', 'conversation_turn', ?1, 'memory_owner:local',
                       'conversation:default', 1)",
            [&conversation_old],
        )
        .unwrap();
        let vector_rowid: i64 = conn
            .query_row(
                "SELECT id FROM vectors_meta WHERE vec_id = 'turn-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        conn.execute(
            "INSERT INTO vectors_fts (rowid, content) VALUES (?1, ?2)",
            (vector_rowid, conversation_plain),
        )
        .unwrap();

        let report = rekey_personal_data_encryption(&conn, &live, &[&old]).unwrap();
        assert_eq!(report.rekeyed, 2);
        assert_eq!(report.locked, 0);
        assert_eq!(report.fts_removed, 1);

        let checkpoint_raw: String = conn
            .query_row(
                "SELECT state_json FROM agent_checkpoints WHERE thread_id = 'thread-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let conversation_raw: String = conn
            .query_row(
                "SELECT content FROM vectors_meta WHERE vec_id = 'turn-1'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert!(checkpoint_raw.starts_with("v2:"));
        assert!(conversation_raw.starts_with("v2:"));
        assert_eq!(live.try_decrypt(&checkpoint_raw).unwrap(), checkpoint_plain);
        assert_eq!(
            live.try_decrypt(&conversation_raw).unwrap(),
            conversation_plain
        );
        assert!(old.read_fact(&conversation_raw).is_locked());
        let fts_count: i64 = conn
            .query_row(
                "SELECT count(*) FROM vectors_fts WHERE rowid = ?1",
                [vector_rowid],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_count, 0);

        let second = rekey_personal_data_encryption(&conn, &live, &[&old]).unwrap();
        assert_eq!(second, PersonalDataRekeyReport::default());
    }

    fn mk_fact(key: &str, value: &str) -> Fact {
        Fact {
            key: key.into(),
            value: value.into(),
            createdAt: "d".into(),
            updatedAt: "d".into(),
            ttlDays: None,
            source: "t".into(),
            category: None,
            importance: 0.5,
            confidenceScore: 1.0,
            sourceTurnId: None,
            memory_strength: 1.0,
            last_accessed_at: 0,
            access_count: 0,
        }
    }

    /// set_fact BACKUP-BEFORE-OVERWRITE: đè một value ĐANG locked (sai khoá) phải
    /// SAO LƯU ciphertext gốc vào facts_locked_backup TRƯỚC — không mất bản gốc,
    /// khôi phục được khi có đúng khoá. Đây là chốt chống-mất ở tầng GHI.
    #[test]
    fn set_fact_sao_luu_value_locked_truoc_khi_de() {
        let a = EncryptionEngine::new("khoa-A-that-su-1234567890abcdef");
        let b = EncryptionEngine::new("khoa-B-khac-han-fedcba0987654321");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();

        set_fact(&conn, &a, &mk_fact("k", "bí mật gốc")).unwrap();
        let old_cipher: String = conn
            .query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0))
            .unwrap();
        assert!(
            b.read_fact(&old_cipher).is_locked(),
            "dưới khoá B, value cũ là locked"
        );

        // Ghi đè bằng khoá B (value cũ locked) → phải sao lưu bản gốc.
        set_fact(&conn, &b, &mk_fact("k", "giá trị mới")).unwrap();

        let backup: String = conn
            .query_row(
                "SELECT value FROM facts_locked_backup WHERE key='k'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(
            backup, old_cipher,
            "ciphertext gốc phải được sao lưu nguyên vẹn"
        );
        assert_eq!(
            a.read_fact(&backup).into_value(),
            "bí mật gốc",
            "khôi phục được bằng khoá A"
        );
        assert_eq!(
            get_fact(&conn, &b, "k").unwrap().unwrap().value,
            "giá trị mới"
        );
    }

    /// Ghi đè value ĐỌC ĐƯỢC (khoá đúng) thì KHÔNG sao lưu — tránh bloat backup
    /// cho mọi lần ghi bình thường.
    #[test]
    fn set_fact_khong_sao_luu_khi_value_doc_duoc() {
        let a = EncryptionEngine::new("khoa-A-that-su-1234567890abcdef");
        let db = DatabasePool::new_in_memory().unwrap();
        let conn = db.writer.get().unwrap();
        set_fact(&conn, &a, &mk_fact("k", "v1")).unwrap();
        set_fact(&conn, &a, &mk_fact("k", "v2")).unwrap();

        let n: i64 = conn
            .query_row("SELECT COUNT(*) FROM facts_locked_backup", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "value đọc được -> không sao lưu");
        assert_eq!(get_fact(&conn, &a, "k").unwrap().unwrap().value, "v2");
    }
}
