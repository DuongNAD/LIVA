use rusqlite::Connection;

/// Configures the SQLite connection with pragmas optimized for performance and WAL mode.
pub fn configure_connection(conn: &Connection, read_only: bool) -> Result<(), rusqlite::Error> {
    conn.execute_batch(
        "
        PRAGMA foreign_keys = ON;
        PRAGMA busy_timeout = 5000;
        PRAGMA cache_size = -2000;
        PRAGMA page_size = 4096;
        PRAGMA mmap_size = 268435456;
        PRAGMA temp_store = MEMORY;
    ",
    )?;

    if read_only {
        conn.execute("PRAGMA synchronous = NORMAL", [])?;
    } else {
        conn.execute_batch(
            "
            PRAGMA journal_mode = WAL;
            PRAGMA synchronous = NORMAL;
            PRAGMA journal_size_limit = 67108864;
            PRAGMA wal_autocheckpoint = 1000;
        ",
        )?;
    }
    Ok(())
}
