use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::persistence_backup::{
    backup_database, backup_manifest_path, restore_database,
};
use rusqlite::Connection;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(1);

fn test_key_id() -> String {
    EncryptionEngine::new("backup-test-key-32-bytes-long-val").key_id()
}

fn temp_dir() -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "liva-backup-test-{}-{}",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn online_backup_va_restore_atomic_giu_ban_rollback() {
    let dir = temp_dir();
    let database = dir.join("liva.db");
    let backup = dir.join("liva.backup.db");

    let pool = DatabasePool::new(&database).unwrap();
    pool.writer
        .get()
        .unwrap()
        .execute_batch(
            "
            CREATE TABLE backup_probe(value TEXT NOT NULL);
            INSERT INTO backup_probe(value) VALUES ('before-backup');
            ",
        )
        .unwrap();

    let metadata = backup_database(&pool, &backup, &test_key_id()).unwrap();
    assert!(backup.exists());
    assert!(backup_manifest_path(&backup).exists());
    assert_eq!(metadata.bytes, std::fs::metadata(&backup).unwrap().len());

    pool.writer
        .get()
        .unwrap()
        .execute("UPDATE backup_probe SET value='after-backup'", [])
        .unwrap();
    drop(pool);

    let restored = restore_database(&backup, &database, &test_key_id()).unwrap();
    let rollback = restored.rollback_path.expect("phải giữ bản trước restore");

    let restored_value: String = Connection::open(&database)
        .unwrap()
        .query_row("SELECT value FROM backup_probe", [], |row| row.get(0))
        .unwrap();
    let rollback_value: String = Connection::open(rollback)
        .unwrap()
        .query_row("SELECT value FROM backup_probe", [], |row| row.get(0))
        .unwrap();
    assert_eq!(restored_value, "before-backup");
    assert_eq!(rollback_value, "after-backup");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restore_tu_choi_backup_bi_sua_va_khong_dung_database_hien_tai() {
    let dir = temp_dir();
    let database = dir.join("liva.db");
    let backup = dir.join("liva.backup.db");
    let pool = DatabasePool::new(&database).unwrap();
    pool.writer
        .get()
        .unwrap()
        .execute_batch(
            "
            CREATE TABLE backup_probe(value TEXT NOT NULL);
            INSERT INTO backup_probe(value) VALUES ('current');
            ",
        )
        .unwrap();
    backup_database(&pool, &backup, &test_key_id()).unwrap();
    drop(pool);

    std::fs::OpenOptions::new()
        .append(true)
        .open(&backup)
        .unwrap()
        .write_all(b"tampered")
        .unwrap();

    assert!(restore_database(&backup, &database, &test_key_id()).is_err());
    let current: String = Connection::open(&database)
        .unwrap()
        .query_row("SELECT value FROM backup_probe", [], |row| row.get(0))
        .unwrap();
    assert_eq!(current, "current");

    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn restore_tu_choi_sai_khoa_truoc_khi_dung_target_va_khoi_phuc_dung_khoa() {
    let dir = temp_dir();
    let source = dir.join("source.db");
    let target = dir.join("target.db");
    let backup = dir.join("encrypted.backup.db");
    let key_a = EncryptionEngine::new("recovery-key-a-32-bytes-long-value");
    let key_b = EncryptionEngine::new("recovery-key-b-32-bytes-long-value");
    let canary = "BACKUP-RECOVERY-CANARY-20260731";

    let source_pool = DatabasePool::new(&source).unwrap();
    source_pool
        .writer
        .get()
        .unwrap()
        .execute(
            "INSERT INTO agent_checkpoints (thread_id, state_json) VALUES ('recovery', ?1)",
            [key_a.encrypt(canary).unwrap()],
        )
        .unwrap();
    let metadata = backup_database(&source_pool, &backup, &key_a.key_id()).unwrap();
    assert_eq!(metadata.key_id, key_a.key_id());
    drop(source_pool);

    let target_pool = DatabasePool::new(&target).unwrap();
    target_pool
        .writer
        .get()
        .unwrap()
        .execute(
            "INSERT INTO agent_checkpoints (thread_id, state_json) VALUES ('current', 'untouched')",
            [],
        )
        .unwrap();
    drop(target_pool);

    let mismatch = restore_database(&backup, &target, &key_b.key_id()).unwrap_err();
    assert!(mismatch.to_string().contains("encryption key"));
    let current: String = Connection::open(&target)
        .unwrap()
        .query_row(
            "SELECT state_json FROM agent_checkpoints WHERE thread_id = 'current'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(current, "untouched");

    restore_database(&backup, &target, &key_a.key_id()).unwrap();
    let restored_raw: String = Connection::open(&target)
        .unwrap()
        .query_row(
            "SELECT state_json FROM agent_checkpoints WHERE thread_id = 'recovery'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert_eq!(key_a.try_decrypt(&restored_raw).unwrap(), canary);
    let backup_bytes = std::fs::read(&backup).unwrap();
    assert!(
        !backup_bytes
            .windows(canary.len())
            .any(|window| window == canary.as_bytes())
    );

    let _ = std::fs::remove_dir_all(dir);
}

use std::io::Write;
