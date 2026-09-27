use rusqlite::ffi;
use std::ffi::CStr;
use std::os::raw::c_char;
use std::sync::Once;

unsafe extern "C" {
    /// C entry point defined in sqlite-vec.c
    pub fn sqlite3_vec_init(
        db: *mut ffi::sqlite3,
        pzErrMsg: *mut *const c_char,
        pThunk: *const ffi::sqlite3_api_routines,
    ) -> std::os::raw::c_int;
}

static INIT_ONCE: Once = Once::new();

/// Registers sqlite-vec as an auto-extension for all newly opened SQLite connections.
/// Safe to call multiple times; the registration runs exactly once.
pub fn register_sqlite_vec() -> Result<(), rusqlite::Error> {
    let mut result = Ok(());
    INIT_ONCE.call_once(|| {
        let rc = unsafe { ffi::sqlite3_auto_extension(Some(sqlite3_vec_init)) };
        if rc != ffi::SQLITE_OK {
            result = Err(rusqlite::Error::SqliteFailure(
                ffi::Error::new(rc),
                Some("Failed to register sqlite-vec via sqlite3_auto_extension".to_string()),
            ));
        }
    });
    result
}

/// Explicitly initializes sqlite-vec on an existing SQLite connection handle.
pub fn register_connection_sqlite_vec(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
    let raw_db = unsafe { conn.handle() };
    let mut err_msg: *mut c_char = std::ptr::null_mut();
    let rc = unsafe {
        sqlite3_vec_init(
            raw_db,
            &mut err_msg as *mut *mut c_char as *mut *const c_char,
            std::ptr::null(),
        )
    };
    if rc != ffi::SQLITE_OK {
        let msg = if !err_msg.is_null() {
            let s = unsafe { CStr::from_ptr(err_msg) }
                .to_string_lossy()
                .into_owned();
            unsafe { ffi::sqlite3_free(err_msg as *mut _) };
            Some(s)
        } else {
            None
        };
        return Err(rusqlite::Error::SqliteFailure(ffi::Error::new(rc), msg));
    }
    Ok(())
}
