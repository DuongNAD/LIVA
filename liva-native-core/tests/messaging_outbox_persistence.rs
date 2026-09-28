use liva_native_core::messaging::{contacts::Platform, outbox};
use liva_native_core::{DatabasePool, EncryptionEngine};

static OUTBOX_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn temp_db_path() -> std::path::PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = OUTBOX_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "liva-outbox-persistence-{}-{}-{}.db",
        std::process::id(),
        nanos,
        counter
    ));
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
    path
}

#[test]
fn encrypted_draft_survives_pool_restart_and_is_consumed_once() {
    let path = temp_db_path();
    let crypto = EncryptionEngine::new("outbox-test-key-32-bytes-long");
    let canary = "noi-dung-rieng-tu-khong-duoc-lo";

    let draft_id = {
        let pool = DatabasePool::new(&path).expect("create outbox database");
        let conn = pool.writer.get().expect("writer connection");
        outbox::stage(
            &conn,
            &crypto,
            Platform::Telegram,
            "Minh Hiến",
            "12345",
            canary,
        )
        .expect("persist draft")
        .draft_id
    };

    let pool = DatabasePool::new(&path).expect("reopen outbox database");
    let conn = pool.writer.get().expect("writer connection after restart");
    let stored: String = conn
        .query_row(
            "SELECT text_ciphertext FROM message_outbox WHERE draft_id = ?1",
            [&draft_id],
            |row| row.get(0),
        )
        .expect("stored ciphertext");
    assert!(
        !stored.contains(canary),
        "outbox must not store message plaintext"
    );

    let pending = outbox::pending(&conn, &crypto).expect("list restarted outbox");
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].text, canary);

    let first = outbox::take(&conn, &crypto, &draft_id).expect("consume draft");
    assert!(matches!(first, outbox::TakeResult::Taken(ref d) if d.text == canary));
    let second = outbox::take(&conn, &crypto, &draft_id).expect("consume twice");
    assert!(matches!(second, outbox::TakeResult::Missing));

    drop(conn);
    drop(pool);
    let _ = std::fs::remove_file(&path);
    let _ = std::fs::remove_file(format!("{}-wal", path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", path.display()));
}
