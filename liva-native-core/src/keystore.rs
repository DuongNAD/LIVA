//! Kho khoá thiết bị (device key): khoá 32 byte tự sinh, niêm phong bằng Windows
//! DPAPI (phạm vi CurrentUser) rồi lưu cạnh DB. Nền của việc BỎ KHOÁ mã hoá MẶC
//! ĐỊNH mà không bắt người dùng tự quản lý khoá.
//!
//! DPAPI cột khoá vào tài khoản Windows hiện tại — mạnh hơn hằng số công khai
//! `"0"×32`, không cần nhập mật khẩu (offline-first, "just works"). Đổi lại DPAPI
//! là **điểm hỏng đơn**: reset/cài lại Windows làm mất master key → khoá không
//! mở lại được. Vì vậy khi SINH khoá mới, boot phải **escrow** (hiện 1 lần cho
//! người dùng sao lưu), và luôn có đường khôi phục qua `LIVA_ENCRYPTION_KEY`.

use std::fmt;
use std::path::{Path, PathBuf};

/// Độ dài khoá thiết bị (byte). 32 byte = khoá đối xứng đầy đủ cho HKDF.
pub const DEVICE_KEY_LEN: usize = 32;

/// Tên file keystore, đặt CẠNH file DB (cùng thư mục dữ liệu).
pub const DEVICE_KEY_FILE: &str = ".device_key";

#[derive(Debug)]
pub enum KeyError {
    /// `.device_key` tồn tại nhưng DPAPI KHÔNG mở được (đổi/tạo lại user Windows,
    /// reset mật khẩu bởi admin, cài lại OS…). Dữ liệu vẫn còn — cần khôi phục
    /// bằng `LIVA_ENCRYPTION_KEY=<khoá đã backup>`.
    Locked(String),
    /// Lỗi I/O đọc/ghi keystore.
    Io(String),
    /// Lỗi DPAPI khi seal/unseal.
    Dpapi(String),
    /// Nền tảng không hỗ trợ kho khoá tự sinh (non-Windows) — phải cấp
    /// `LIVA_ENCRYPTION_KEY` tường minh.
    Unsupported(String),
}

impl fmt::Display for KeyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyError::Locked(m) => write!(f, "khoá thiết bị bị khoá: {m}"),
            KeyError::Io(m) => write!(f, "lỗi I/O keystore: {m}"),
            KeyError::Dpapi(m) => write!(f, "lỗi DPAPI: {m}"),
            KeyError::Unsupported(m) => write!(f, "không hỗ trợ kho khoá tự sinh: {m}"),
        }
    }
}
impl std::error::Error for KeyError {}

/// Đường dẫn file keystore cạnh file DB.
pub fn device_key_path(db_path: &Path) -> PathBuf {
    match db_path.parent() {
        Some(dir) => dir.join(DEVICE_KEY_FILE),
        None => PathBuf::from(DEVICE_KEY_FILE),
    }
}

// ── DPAPI (Windows) ─────────────────────────────────────────────────────────

/// Niêm phong `plain` bằng DPAPI phạm vi CurrentUser (dwFlags = 0).
#[cfg(windows)]
pub fn dpapi_seal(plain: &[u8]) -> Result<Vec<u8>, KeyError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptProtectData};

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: plain.len() as u32,
        pbData: plain.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: `in_blob.pbData` trỏ vào `plain` còn sống suốt lời gọi. `out_blob`
    // do DPAPI cấp phát; ta sao chép ra Vec rồi LocalFree ngay, không giữ con trỏ.
    let ok = unsafe {
        CryptProtectData(
            &in_blob,
            std::ptr::null(),     // szDataDescr
            std::ptr::null_mut(), // pOptionalEntropy
            std::ptr::null_mut(), // pvReserved
            std::ptr::null_mut(), // pPromptStruct
            0,                    // dwFlags: 0 = CurrentUser
            &mut out_blob,
        )
    };
    if ok == 0 {
        return Err(KeyError::Dpapi("CryptProtectData thất bại".into()));
    }
    let sealed =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe { LocalFree(out_blob.pbData as _) };
    Ok(sealed)
}

/// Mở khoá dữ liệu đã niêm phong bằng [`dpapi_seal`]. `Err(Locked)` nghĩa là
/// DPAPI của user hiện tại không mở được (đổi user / cài lại OS).
#[cfg(windows)]
pub fn dpapi_unseal(sealed: &[u8]) -> Result<Vec<u8>, KeyError> {
    use windows_sys::Win32::Foundation::LocalFree;
    use windows_sys::Win32::Security::Cryptography::{CRYPT_INTEGER_BLOB, CryptUnprotectData};

    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: sealed.len() as u32,
        pbData: sealed.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB {
        cbData: 0,
        pbData: std::ptr::null_mut(),
    };
    // SAFETY: như dpapi_seal — con trỏ vào `sealed` sống suốt lời gọi, out_blob
    // sao chép ra Vec rồi LocalFree.
    let ok = unsafe {
        CryptUnprotectData(
            &in_blob,
            std::ptr::null_mut(), // ppszDataDescr
            std::ptr::null_mut(), // pOptionalEntropy
            std::ptr::null_mut(), // pvReserved
            std::ptr::null_mut(), // pPromptStruct
            0,                    // dwFlags
            &mut out_blob,
        )
    };
    if ok == 0 {
        return Err(KeyError::Locked(
            "CryptUnprotectData thất bại — sai user Windows hoặc dữ liệu hỏng".into(),
        ));
    }
    let plain =
        unsafe { std::slice::from_raw_parts(out_blob.pbData, out_blob.cbData as usize) }.to_vec();
    unsafe { LocalFree(out_blob.pbData as _) };
    Ok(plain)
}

/// Đọc khoá thiết bị từ `.device_key` (giải bằng DPAPI), hoặc SINH mới nếu
/// chưa có. Trả `(passphrase_hex_64, vừa_sinh_mới)`:
/// - `passphrase_hex` = 32 byte khoá mã hoá hex (dùng làm passphrase cho
///   `EncryptionEngine::new`); cũng chính là chuỗi khôi phục cho
///   `LIVA_ENCRYPTION_KEY`;
/// - `vừa_sinh_mới == true` ⇒ boot PHẢI escrow (hiện 1 lần cho người dùng lưu).
///
/// Ghi khoá mới ATOMIC bằng `create_new` (CREATE_NEW/O_EXCL): nếu tiến trình
/// khác vừa ghi (race first-boot khi gateway + vỏ Tauri cùng chạy), lời ghi này
/// thất bại `AlreadyExists` → ta VỨT khoá vừa sinh và ĐỌC LẠI khoá đã persist,
/// không bao giờ chạy bằng khoá chưa ghi thành công (tránh split-brain khoá).
///
/// **KHÔNG** ghi đè file đã tồn tại: nếu `.device_key` có nhưng DPAPI không mở
/// được (đổi user / cài lại OS), trả `Err(Locked)` để boot dừng và chỉ dẫn khôi
/// phục — TUYỆT ĐỐI không sinh khoá mới đè lên (sẽ khoá chết dữ liệu cũ).
pub fn load_or_create_device_key(db_path: &Path) -> Result<(String, bool), KeyError> {
    let (raw, generated) = load_or_create_sealed(&device_key_path(db_path), DEVICE_KEY_LEN)?;
    Ok((hex::encode(raw), generated))
}

/// Tên file bí mật vault, đặt CẠNH snapshot `liva_vault.app`.
pub const VAULT_SECRET_FILE: &str = ".vault_secret";
/// Độ dài password vault (byte) đưa vào Argon2id.
pub const VAULT_PASSWORD_LEN: usize = 32;
/// Độ dài salt vault (byte).
pub const VAULT_SALT_LEN: usize = 16;

/// Bí mật cho Stronghold vault: `(password 32B, salt 16B)` per-machine, niêm
/// phong DPAPI, lưu `.vault_secret` cạnh snapshot. Trả thêm `vừa_sinh_mới` để
/// caller biết cần MIGRATE vault cũ (nếu snapshot đã tồn tại).
///
/// Tách khỏi khoá DB (`.device_key`) theo quyết định của người dùng — lộ khoá
/// vault KHÔNG kéo mất khoá DB và ngược lại.
pub fn load_or_create_vault_secret(dir: &Path) -> Result<(Vec<u8>, Vec<u8>, bool), KeyError> {
    let (raw, generated) = load_or_create_sealed(
        &dir.join(VAULT_SECRET_FILE),
        VAULT_PASSWORD_LEN + VAULT_SALT_LEN,
    )?;
    let password = raw[..VAULT_PASSWORD_LEN].to_vec();
    let salt = raw[VAULT_PASSWORD_LEN..].to_vec();
    Ok((password, salt, generated))
}

/// Đọc + giải niêm (DPAPI) một khối bí mật `len` byte từ `path`, hoặc SINH mới
/// (OsRng) + niêm phong + ghi ATOMIC (`create_new`) nếu chưa có. Trả
/// `(bytes, vừa_sinh_mới)`.
///
/// Dùng chung cho khoá thiết bị (DB) lẫn bí mật vault. KHÔNG ghi đè file
/// mở-không-được: trả `Err(Locked)` để caller quyết (fail-fast cho DB, fail-soft
/// reset cho vault). Race first-boot: `create_new` thua → đọc lại bản đã persist.
fn load_or_create_sealed(path: &Path, len: usize) -> Result<(Vec<u8>, bool), KeyError> {
    if path.exists() {
        let sealed = std::fs::read(path).map_err(|e| KeyError::Io(e.to_string()))?;
        let raw = dpapi_unseal(&sealed)?;
        if raw.len() != len {
            return Err(KeyError::Locked(format!(
                "bí mật giải ra {} byte, cần {len}",
                raw.len()
            )));
        }
        return Ok((raw, false));
    }

    let mut raw = vec![0u8; len];
    rand::RngCore::fill_bytes(&mut rand::rngs::OsRng, &mut raw);
    let sealed = dpapi_seal(&raw)?;

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| KeyError::Io(e.to_string()))?;
    }
    match write_new_exclusive(path, &sealed) {
        Ok(()) => Ok((raw, true)),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            let sealed2 = std::fs::read(path).map_err(|e| KeyError::Io(e.to_string()))?;
            let raw2 = dpapi_unseal(&sealed2)?;
            if raw2.len() != len {
                return Err(KeyError::Locked("bí mật (đọc lại) sai độ dài".into()));
            }
            Ok((raw2, false))
        }
        Err(e) => Err(KeyError::Io(e.to_string())),
    }
}

/// Ghi file CHỈ KHI CHƯA TỒN TẠI (create_new). Trả `AlreadyExists` nếu đã có.
fn write_new_exclusive(path: &Path, data: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    let mut f = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    f.write_all(data)?;
    f.sync_all()
}

/// Hiện hộp thoại thông báo modal (Win32 MessageBox), chặn tới khi bấm OK.
/// Dùng cho escrow khoá ở vỏ Tauri (không có console). No-op ngoài Windows.
/// (Win32 MessageBox cho Ctrl+C sao chép toàn bộ nội dung — người dùng copy
/// được khoá.)
#[cfg(windows)]
pub fn show_message_box(title: &str, text: &str) {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MB_ICONWARNING, MB_OK, MessageBoxW};
    let wide = |s: &str| {
        s.encode_utf16()
            .chain(std::iter::once(0))
            .collect::<Vec<u16>>()
    };
    let (t, b) = (wide(title), wide(text));
    // SAFETY: hai chuỗi wide kết thúc NUL còn sống suốt lời gọi; HWND 0 = không parent.
    unsafe {
        MessageBoxW(0, b.as_ptr(), t.as_ptr(), MB_OK | MB_ICONWARNING);
    }
}

#[cfg(not(windows))]
pub fn show_message_box(_title: &str, _text: &str) {}

#[cfg(not(windows))]
pub fn dpapi_seal(_plain: &[u8]) -> Result<Vec<u8>, KeyError> {
    Err(KeyError::Unsupported(
        "DPAPI chỉ có trên Windows; non-Windows phải cấp LIVA_ENCRYPTION_KEY".into(),
    ))
}

#[cfg(not(windows))]
pub fn dpapi_unseal(_sealed: &[u8]) -> Result<Vec<u8>, KeyError> {
    Err(KeyError::Unsupported(
        "DPAPI chỉ có trên Windows; non-Windows phải cấp LIVA_ENCRYPTION_KEY".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Round-trip DPAPI: seal rồi unseal phải ra đúng bản gốc, và ciphertext
    /// KHÁC bản gốc. Chỉ chạy trên Windows (nơi DPAPI tồn tại).
    #[cfg(windows)]
    #[test]
    fn dpapi_seal_unseal_round_trip() {
        let key = [0x42u8; DEVICE_KEY_LEN];
        let sealed = dpapi_seal(&key).expect("seal phải thành công trên Windows");
        assert_ne!(sealed.as_slice(), &key[..], "sealed phải khác bản gốc");
        let opened = dpapi_unseal(&sealed).expect("unseal phải mở được");
        assert_eq!(
            opened.as_slice(),
            &key[..],
            "round-trip phải khôi phục đúng khoá"
        );
    }

    /// Dữ liệu rác không phải sealed-blob → unseal trả Locked (không panic).
    #[cfg(windows)]
    #[test]
    fn dpapi_unseal_rac_tra_locked() {
        let res = dpapi_unseal(b"khong-phai-dpapi-blob-chi-la-rac");
        assert!(
            matches!(res, Err(KeyError::Locked(_))),
            "rác phải cho Locked, không panic"
        );
    }

    #[test]
    fn device_key_path_ke_ben_db() {
        let p = device_key_path(Path::new("data/agents/liva_core/mem.sqlite"));
        assert!(p.ends_with(DEVICE_KEY_FILE));
        assert_eq!(
            p.parent(),
            Path::new("data/agents/liva_core/mem.sqlite").parent()
        );
    }

    #[cfg(windows)]
    fn unique_tmp_dir(tag: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        let d = std::env::temp_dir().join(format!(
            "liva_ks_{}_{}_{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst),
            tag
        ));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    /// Lần đầu SINH khoá (generated=true) + tạo file; lần sau ĐỌC LẠI cùng khoá
    /// (generated=false). Khoá là 64 hex (32 byte). Chỉ Windows (cần DPAPI).
    #[cfg(windows)]
    #[test]
    fn load_or_create_sinh_roi_doc_lai_on_dinh() {
        let dir = unique_tmp_dir("create");
        let db_path = dir.join("mem.sqlite");

        let (k1, gen1) = load_or_create_device_key(&db_path).unwrap();
        assert!(gen1, "lần đầu phải là sinh mới");
        assert_eq!(k1.len(), DEVICE_KEY_LEN * 2, "khoá = 64 ký tự hex");
        assert!(
            device_key_path(&db_path).exists(),
            "file .device_key phải được tạo"
        );
        assert_ne!(k1, "0".repeat(64), "khoá sinh ra không được là 0");

        let (k2, gen2) = load_or_create_device_key(&db_path).unwrap();
        assert!(!gen2, "lần sau là đọc lại, không sinh");
        assert_eq!(k1, k2, "đọc lại phải ra ĐÚNG khoá đã sinh");

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// Bí mật vault: sinh (pw 32B + salt 16B) rồi đọc lại ổn định; tách BIỆT
    /// khỏi khoá thiết bị (khác file, khác giá trị).
    #[cfg(windows)]
    #[test]
    fn load_or_create_vault_secret_sinh_doc_lai_va_tach_khoi_device_key() {
        let dir = unique_tmp_dir("vault");

        let (pw1, salt1, gen1) = load_or_create_vault_secret(&dir).unwrap();
        assert!(gen1, "lần đầu sinh mới");
        assert_eq!(pw1.len(), VAULT_PASSWORD_LEN);
        assert_eq!(salt1.len(), VAULT_SALT_LEN);
        assert!(dir.join(VAULT_SECRET_FILE).exists());

        let (pw2, salt2, gen2) = load_or_create_vault_secret(&dir).unwrap();
        assert!(!gen2, "lần sau đọc lại");
        assert_eq!(
            (pw1.clone(), salt1.clone()),
            (pw2, salt2),
            "đọc lại phải ổn định"
        );

        // Khoá thiết bị ở cùng thư mục PHẢI khác bí mật vault (file + giá trị khác).
        let (dev_hex, _) = load_or_create_device_key(&dir.join("db.sqlite")).unwrap();
        assert_ne!(
            dev_hex,
            hex::encode(&pw1),
            "device key và vault password phải khác nhau"
        );
        assert!(dir.join(".device_key").exists() && dir.join(VAULT_SECRET_FILE).exists());

        let _ = std::fs::remove_dir_all(&dir);
    }
}
