use liva_storage::{register_connection_sqlite_vec, register_sqlite_vec};
use rusqlite::Connection;

#[test]
fn test_sqlite_vec_static_auto_extension_and_vtab() {
    // 1. Auto-extension registration
    register_sqlite_vec().expect("register_sqlite_vec should succeed");

    // 2. Open brand-new in-memory connection without any DLL
    let conn = Connection::open_in_memory().expect("open in-memory connection");

    // 3. Verify vec_version()
    let version: String = conn
        .query_row("SELECT vec_version()", [], |row| row.get(0))
        .expect("query vec_version");
    assert!(
        version.starts_with('v'),
        "vec_version should start with 'v', got: {version}"
    );

    // 4. Create virtual table with int8[384] vector matching LIVA memory schema
    conn.execute(
        "CREATE VIRTUAL TABLE test_vec USING vec0(embedding int8[384])",
        [],
    )
    .expect("create virtual table vec0");

    // 5. Generate dummy 384-dimensional float vector
    let dummy_floats: Vec<f32> = (0..384).map(|i| (i as f32) / 384.0).collect();
    let blob = bytemuck::cast_slice::<f32, u8>(&dummy_floats);

    // 6. Insert quantized vector
    conn.execute(
        "INSERT INTO test_vec(rowid, embedding) VALUES (1, vec_quantize_int8(?, 'unit'))",
        [blob],
    )
    .expect("insert vector into vec0 table");

    // 7. Perform kNN vector search
    let (matched_id, distance): (i64, f32) = conn
        .query_row(
            "SELECT rowid, distance FROM test_vec WHERE embedding MATCH vec_quantize_int8(?, 'unit') AND k = 1",
            [blob],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )
        .expect("kNN vector query should return result");

    assert_eq!(matched_id, 1);
    assert!(
        distance < 0.05,
        "Self-distance of quantized vector should be near zero, got: {distance}"
    );
}

#[test]
fn test_sqlite_vec_explicit_connection_registration() {
    let conn = Connection::open_in_memory().expect("open in-memory connection");
    register_connection_sqlite_vec(&conn).expect("explicit connection registration");

    let version: String = conn
        .query_row("SELECT vec_version()", [], |row| row.get(0))
        .expect("query vec_version");
    assert!(version.starts_with('v'));
}
