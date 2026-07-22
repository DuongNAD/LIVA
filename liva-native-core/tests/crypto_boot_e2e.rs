//! End-to-end: boot `resolve_and_rekey` trên DB ĐĨA THẬT với facts mã bằng khoá
//! MẶC ĐỊNH — kiểm đường "bỏ khoá mặc định" thực sự chạy trên một vòng boot:
//! sinh khoá thiết bị DPAPI, rekey facts cũ sang khoá thật (không mất), escrow,
//! idempotent khi reboot, và khôi phục qua `LIVA_ENCRYPTION_KEY`.
//!
//! Cả file chỉ chạy trên Windows (cần DPAPI). CI là windows-latest nên phủ được.
#![cfg(windows)]

use liva_native_core::crypto::{self, EncryptionEngine};
use liva_native_core::db::{self, DatabasePool, Fact};
use liva_native_core::resolve_and_rekey;
use std::sync::atomic::{AtomicU64, Ordering};

/// Hai test cùng file dùng chung `std::env` (khoá mã hoá) — phải chạy TUẦN TỰ,
/// nếu không giá trị env của test này rò sang test kia.
static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn tmp_dir(tag: &str) -> std::path::PathBuf {
    static N: AtomicU64 = AtomicU64::new(0);
    let d = std::env::temp_dir().join(format!(
        "liva_e2e_{}_{}_{}",
        std::process::id(),
        N.fetch_add(1, Ordering::SeqCst),
        tag
    ));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn mk_fact(key: &str, value: &str) -> Fact {
    Fact {
        key: key.into(),
        value: value.into(),
        createdAt: "d".into(),
        updatedAt: "d".into(),
        ttlDays: None,
        source: "e2e".into(),
        category: None,
        importance: 0.5,
        confidenceScore: 1.0,
        sourceTurnId: None,
        memory_strength: 1.0,
        last_accessed_at: 0,
        access_count: 0,
    }
}

/// Một vòng đời đầy đủ: máy dev đang chạy khoá mặc định → nâng cấp → reboot →
/// khôi phục sau khi "mất DPAPI".
#[test]
fn boot_bo_khoa_mac_dinh_end_to_end() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    // Bắt đầu sạch: không env khoá (đi đường sinh khoá thiết bị).
    unsafe {
        std::env::remove_var("LIVA_ENCRYPTION_KEY");
        std::env::remove_var("LIVA_ENCRYPTION_KEY_OLD");
    }

    let dir = tmp_dir("boot");
    let db_path = dir.join("mem.sqlite");
    let default_engine = EncryptionEngine::new(crypto::DEFAULT_ENCRYPTION_KEY);

    // 1. Giả lập máy dev: ghi 2 fact mã bằng khoá MẶC ĐỊNH "0"×32.
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let conn = db.writer.get().unwrap();
        db::set_fact(&conn, &default_engine, &mk_fact("mèo", "tên Bún")).unwrap();
        db::set_fact(&conn, &default_engine, &mk_fact("màu", "xanh dương")).unwrap();
    }

    // 2. BOOT LẦN 1 — không env → sinh khoá thiết bị DPAPI + rekey facts cũ.
    let escrow_key;
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).unwrap();

        assert!(bk.escrow_hex.is_some(), "lần đầu sinh khoá thiết bị → phải escrow");
        assert_eq!(bk.rekeyed, 2, "2 fact khoá-mặc-định phải được chuyển sang khoá thật");
        assert_eq!(bk.locked, 0, "không có bản khoá-chết");
        assert!(
            db_path.parent().unwrap().join(".device_key").exists(),
            "file .device_key phải được tạo"
        );

        let conn = db.readers.get().unwrap();
        // Đọc được bằng khoá THẬT.
        assert_eq!(db::get_fact(&conn, &bk.engine, "mèo").unwrap().unwrap().value, "tên Bún");
        assert_eq!(db::get_fact(&conn, &bk.engine, "màu").unwrap().unwrap().value, "xanh dương");
        // Khoá MẶC ĐỊNH KHÔNG còn mở được (đã rekey thật sự trên đĩa).
        let raw: String =
            conn.query_row("SELECT value FROM facts WHERE key='mèo'", [], |r| r.get(0)).unwrap();
        assert!(default_engine.read_fact(&raw).is_locked(), "khoá mặc định phải hết mở được");

        escrow_key = bk.escrow_hex.clone().unwrap();
    }

    // 3. BOOT LẦN 2 (reboot) — đọc lại khoá thiết bị, KHÔNG escrow, idempotent.
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).unwrap();
        assert!(bk.escrow_hex.is_none(), "reboot không sinh lại khoá → không escrow");
        assert_eq!(bk.rekeyed, 0, "đã ở khoá thật → không rekey (idempotent)");
        assert_eq!(bk.source, "device-key");
        let conn = db.readers.get().unwrap();
        assert_eq!(db::get_fact(&conn, &bk.engine, "mèo").unwrap().unwrap().value, "tên Bún");
    }

    // 4. KHÔI PHỤC sau "mất DPAPI": đặt LIVA_ENCRYPTION_KEY = khoá đã escrow.
    //    (env ưu tiên hơn keystore; xoá .device_key mô phỏng cài lại Windows.)
    std::fs::remove_file(db_path.parent().unwrap().join(".device_key")).ok();
    unsafe {
        std::env::set_var("LIVA_ENCRYPTION_KEY", &escrow_key);
    }
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).unwrap();
        assert_eq!(bk.source, "env", "đặt env → dùng khoá env, không sinh khoá thiết bị");
        let conn = db.readers.get().unwrap();
        assert_eq!(
            db::get_fact(&conn, &bk.engine, "mèo").unwrap().unwrap().value,
            "tên Bún",
            "khôi phục dữ liệu qua LIVA_ENCRYPTION_KEY = khoá đã backup"
        );
    }

    unsafe {
        std::env::remove_var("LIVA_ENCRYPTION_KEY");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

/// XOAY KHOÁ THẬT (không phải mặc định): facts mã bằng khoá A; boot với
/// `LIVA_ENCRYPTION_KEY=B` + `LIVA_ENCRYPTION_KEY_OLD=A` → rekey A→B tại chỗ,
/// đọc được bằng B, khoá cũ A hết mở được. Đây là đường khôi phục khi đổi khoá.
#[test]
fn boot_xoay_khoa_qua_key_old_end_to_end() {
    let _guard = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    unsafe {
        std::env::remove_var("LIVA_ENCRYPTION_KEY");
        std::env::remove_var("LIVA_ENCRYPTION_KEY_OLD");
    }

    let dir = tmp_dir("rotate");
    let db_path = dir.join("mem.sqlite");
    let key_a = "khoa-cu-A-1234567890abcdefghij";
    let key_b = "khoa-moi-B-zyxwvutsrqponmlkji9";
    let engine_a = EncryptionEngine::new(key_a);

    // Facts mã bằng khoá A.
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let conn = db.writer.get().unwrap();
        db::set_fact(&conn, &engine_a, &mk_fact("k", "giá trị dưới khoá A")).unwrap();
    }

    // Boot với KEY=B, KEY_OLD=A → rekey A→B.
    unsafe {
        std::env::set_var("LIVA_ENCRYPTION_KEY", key_b);
        std::env::set_var("LIVA_ENCRYPTION_KEY_OLD", key_a);
    }
    {
        let db = DatabasePool::new(&db_path).unwrap();
        let bk = resolve_and_rekey(&db, &db_path, false).unwrap();
        assert_eq!(bk.source, "env");
        assert_eq!(bk.rekeyed, 1, "fact khoá A phải được rekey sang B");
        let conn = db.readers.get().unwrap();
        assert_eq!(db::get_fact(&conn, &bk.engine, "k").unwrap().unwrap().value, "giá trị dưới khoá A");
        let raw: String =
            conn.query_row("SELECT value FROM facts WHERE key='k'", [], |r| r.get(0)).unwrap();
        assert!(engine_a.read_fact(&raw).is_locked(), "sau xoay, khoá cũ A phải hết mở được");
    }

    unsafe {
        std::env::remove_var("LIVA_ENCRYPTION_KEY");
        std::env::remove_var("LIVA_ENCRYPTION_KEY_OLD");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
