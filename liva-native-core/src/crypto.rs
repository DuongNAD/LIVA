use aes_gcm::aead::consts::U16;
use aes_gcm::{
    AesGcm, Nonce,
    aead::{Aead, KeyInit},
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::Sha256;

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

/// Tiền tố phân biệt định dạng mới (KDF, có salt) với định dạng cũ.
const V2_PREFIX: &str = "v2:";
/// `info` cố định cho HKDF-expand — ràng khoá dẫn xuất vào đúng mục đích này.
const HKDF_INFO: &[u8] = b"liva-facts-encryption-v2";
const SALT_LEN: usize = 16;

/// Mã hoá AES-256-GCM cho dữ liệu nhạy cảm (hiện: `facts.value`).
///
/// # Hai định dạng ciphertext cùng tồn tại
///
/// - **v1** (cũ): `iv:tag:cipher` (hex), khoá = bytes của passphrase pad `0x00`
///   tới 32 byte. Entropy thấp, không salt. **Chỉ còn ĐỌC** để không mất dữ
///   liệu cũ; không sinh mới.
/// - **v2** (22/07/2026): `v2:salt:iv:tag:cipher` (hex), khoá =
///   HKDF-SHA256(passphrase, salt ngẫu nhiên mỗi bản ghi). Salt riêng từng
///   ciphertext ⇒ hai plaintext giống nhau ra ciphertext khác nhau, và khoá
///   được trải đều 32 byte thay vì pad `0x00`.
///
/// `encrypt` luôn sinh **v2**. `decrypt`/`try_decrypt` đọc được cả hai. Dữ liệu
/// v1 cũ được nâng cấp lên v2 bởi [`crate::db::migrate_facts_encryption`] khi
/// khởi động (giải mã bằng khoá v1 rồi mã hoá lại bằng KDF v2 — không mất gì).
pub struct EncryptionEngine {
    /// Khoá v1: bytes passphrase pad `0x00` (chỉ để giải mã dữ liệu cũ).
    legacy_key: [u8; 32],
    /// Passphrase gốc, để dẫn xuất khoá v2 qua HKDF với salt mỗi bản ghi.
    passphrase: Vec<u8>,
}

/// Vì sao decrypt thất bại — để tầng gọi phân biệt "chưa mã hoá" (hợp lệ,
/// migration) với "bị sửa đổi" (tấn công) thay vì gộp tất cả thành passthrough.
#[derive(Debug, PartialEq, Eq)]
pub enum DecryptError {
    /// Không phải định dạng `iv:tag:cipher` — nhiều khả năng là plaintext cũ
    /// chưa từng mã hoá. KHÔNG phải lỗi bảo mật.
    NotEncrypted,
    /// Đúng 3 phần nhưng hex/độ dài sai — dữ liệu hỏng.
    BadFormat,
    /// Tag xác thực AES-GCM KHÔNG khớp: ciphertext đã bị sửa đổi hoặc sai khoá.
    /// Đây là ca mà `decrypt` fail-open đang nuốt im lặng.
    AuthFailed,
    /// Giải mã thành công nhưng bytes không phải UTF-8 hợp lệ.
    NotUtf8,
}

impl EncryptionEngine {
    pub fn new(key_str: &str) -> Self {
        let mut legacy_key = [0u8; 32];
        let bytes = key_str.as_bytes();
        let len = bytes.len().min(32);
        legacy_key[..len].copy_from_slice(&bytes[..len]);
        Self {
            legacy_key,
            passphrase: bytes.to_vec(),
        }
    }

    /// Dẫn xuất khoá 32 byte từ passphrase + salt qua HKDF-SHA256. Không bao
    /// giờ fail với salt/ikm hợp lệ (HKDF-expand chỉ fail khi độ dài ra > 255*32).
    fn derive_key(&self, salt: &[u8]) -> [u8; 32] {
        let hk = Hkdf::<Sha256>::new(Some(salt), &self.passphrase);
        let mut okm = [0u8; 32];
        hk.expand(HKDF_INFO, &mut okm)
            .expect("HKDF-expand 32 byte không thể fail");
        okm
    }

    /// Mã hoá thành định dạng **v2** (KDF + salt mỗi bản ghi). Đường sinh mới
    /// duy nhất; v1 chỉ còn để đọc.
    pub fn encrypt(&self, text: &str) -> Result<String, String> {
        let mut salt = [0u8; SALT_LEN];
        let mut iv = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        rand::rngs::OsRng.fill_bytes(&mut iv);

        let key = self.derive_key(&salt);
        let cipher = Aes256Gcm16::new_from_slice(&key).map_err(|e| e.to_string())?;
        let nonce = Nonce::<U16>::from_slice(&iv);

        let ciphertext_with_tag = cipher
            .encrypt(nonce, text.as_bytes())
            .map_err(|e| e.to_string())?;
        if ciphertext_with_tag.len() < 16 {
            return Err("Ciphertext is too short".to_string());
        }
        let split_idx = ciphertext_with_tag.len() - 16;
        let ciphertext = &ciphertext_with_tag[..split_idx];
        let tag = &ciphertext_with_tag[split_idx..];

        Ok(format!(
            "{}{}:{}:{}:{}",
            V2_PREFIX,
            hex::encode(salt),
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(ciphertext)
        ))
    }

    /// Giải mã fail-OPEN: mọi lỗi trả lại chuỗi đầu vào. Giữ nguyên hợp đồng cũ
    /// để không phá đường migration plaintext của caller. Đọc được CẢ v1 lẫn v2.
    pub fn decrypt(&self, text: &str) -> String {
        self.try_decrypt(text).unwrap_or_else(|_| text.to_string())
    }

    /// Giải mã **fail-CLOSED**: `Err` khi bị sửa đổi (AuthFailed), hỏng, hoặc
    /// chưa mã hoá. Đọc được cả hai định dạng — tự nhận theo tiền tố `v2:`.
    pub fn try_decrypt(&self, text: &str) -> Result<String, DecryptError> {
        match text.strip_prefix(V2_PREFIX) {
            Some(rest) => self.decrypt_v2(rest),
            None => self.decrypt_v1(text),
        }
    }

    /// v2: `salt:iv:tag:cipher`, khoá = HKDF(passphrase, salt).
    fn decrypt_v2(&self, rest: &str) -> Result<String, DecryptError> {
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 4 {
            return Err(DecryptError::BadFormat);
        }
        let salt = hex::decode(parts[0]).map_err(|_| DecryptError::BadFormat)?;
        let iv = hex::decode(parts[1]).map_err(|_| DecryptError::BadFormat)?;
        let tag = hex::decode(parts[2]).map_err(|_| DecryptError::BadFormat)?;
        let cipher_bytes = hex::decode(parts[3]).map_err(|_| DecryptError::BadFormat)?;
        if salt.len() != SALT_LEN || iv.len() != 16 || tag.len() != 16 {
            return Err(DecryptError::BadFormat);
        }
        let key = self.derive_key(&salt);
        self.open(&key, &iv, &tag, cipher_bytes)
    }

    /// v1 (cũ): `iv:tag:cipher`, khoá = passphrase pad `0x00`. Chỉ để đọc.
    fn decrypt_v1(&self, text: &str) -> Result<String, DecryptError> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 3 {
            return Err(DecryptError::NotEncrypted);
        }
        let iv = hex::decode(parts[0]).map_err(|_| DecryptError::BadFormat)?;
        let tag = hex::decode(parts[1]).map_err(|_| DecryptError::BadFormat)?;
        let cipher_bytes = hex::decode(parts[2]).map_err(|_| DecryptError::BadFormat)?;
        if iv.len() != 16 || tag.len() != 16 {
            return Err(DecryptError::BadFormat);
        }
        self.open(&self.legacy_key, &iv, &tag, cipher_bytes)
    }

    /// Bước AES-GCM chung: dựng cipher, ghép tag vào cuối, giải mã. `Err` từ
    /// `decrypt` = tag không khớp = bị sửa đổi hoặc sai khoá.
    fn open(&self, key: &[u8], iv: &[u8], tag: &[u8], mut cipher_bytes: Vec<u8>) -> Result<String, DecryptError> {
        let cipher = Aes256Gcm16::new_from_slice(key).map_err(|_| DecryptError::BadFormat)?;
        let nonce = Nonce::<U16>::from_slice(iv);
        cipher_bytes.extend_from_slice(tag);
        let plain = cipher
            .decrypt(nonce, cipher_bytes.as_slice())
            .map_err(|_| DecryptError::AuthFailed)?;
        String::from_utf8(plain).map_err(|_| DecryptError::NotUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encryption_decryption_roundtrip() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);
        let plain = "hello world, this is a secret message!";

        let encrypted = engine.encrypt(plain).unwrap();
        assert_ne!(plain, encrypted);

        // Định dạng mới v2: "v2:salt:iv:tag:cipher".
        assert!(encrypted.starts_with("v2:"), "encrypt phải sinh v2");
        let parts: Vec<&str> = encrypted.split(':').collect();
        assert_eq!(parts.len(), 5, "v2 = v2:salt:iv:tag:cipher");
        assert_eq!(parts[1].len(), 32, "salt 16 byte = 32 hex");
        assert_eq!(parts[2].len(), 32, "iv 16 byte = 32 hex");
        assert_eq!(parts[3].len(), 32, "tag 16 byte = 32 hex");

        let decrypted = engine.decrypt(&encrypted);
        assert_eq!(plain, decrypted);
    }

    /// Hai plaintext GIỐNG NHAU phải ra ciphertext KHÁC NHAU (salt riêng mỗi
    /// bản ghi) — điều mà kiểu cũ không đảm bảo tốt.
    #[test]
    fn v2_salt_lam_ciphertext_khac_nhau() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let a = engine.encrypt("cùng một câu").unwrap();
        let b = engine.encrypt("cùng một câu").unwrap();
        assert_ne!(a, b, "salt ngẫu nhiên -> ciphertext phải khác");
        assert_eq!(engine.decrypt(&a), "cùng một câu");
        assert_eq!(engine.decrypt(&b), "cùng một câu");
    }

    /// BACKWARD-COMPAT: dữ liệu v1 (khoá pad 0x00) do bản CŨ ghi phải vẫn giải
    /// mã được — đây là điều kiện để migration không mất dữ liệu. Dựng một
    /// ciphertext v1 thật bằng chính thuật toán cũ.
    #[test]
    fn doc_duoc_du_lieu_v1_cu() {
        use aes_gcm::aead::consts::U16;
        use aes_gcm::{AesGcm, Nonce, aead::{Aead, KeyInit}};
        type G = AesGcm<aes_gcm::aes::Aes256, U16>;

        let key_str = "khoa-cu-cua-toi";
        // Khoá v1: bytes pad 0x00 tới 32.
        let mut raw = [0u8; 32];
        raw[..key_str.len()].copy_from_slice(key_str.as_bytes());
        let iv = [7u8; 16];
        let ct = G::new_from_slice(&raw).unwrap()
            .encrypt(Nonce::<U16>::from_slice(&iv), b"du lieu cu".as_ref()).unwrap();
        let (c, tag) = ct.split_at(ct.len() - 16);
        let v1 = format!("{}:{}:{}", hex::encode(iv), hex::encode(tag), hex::encode(c));

        let engine = EncryptionEngine::new(key_str);
        assert_eq!(engine.try_decrypt(&v1).unwrap(), "du lieu cu", "v1 phải đọc được");
        // Và engine mới sinh v2 khác hẳn.
        assert!(engine.encrypt("x").unwrap().starts_with("v2:"));
    }

    #[test]
    fn test_decrypt_plain_fallback() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);

        let plain = "unencrypted plaintext";
        let decrypted = engine.decrypt(plain);
        assert_eq!(plain, decrypted);
    }

    #[test]
    fn test_decrypt_corrupted_fallback() {
        let key = "00000000000000000000000000000000";
        let engine = EncryptionEngine::new(key);

        let encrypted = engine.encrypt("secret").unwrap();
        let corrupted = format!("{}a", encrypted);
        let decrypted = engine.decrypt(&corrupted);
        assert_eq!(corrupted, decrypted);
    }

    // ── try_decrypt: fail-closed, PHÁT HIỆN sửa đổi ──────────────────────────

    #[test]
    fn try_decrypt_roundtrip() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let plain = "bí mật của Dương — mèo tên Bún";
        let enc = engine.encrypt(plain).unwrap();
        assert_eq!(engine.try_decrypt(&enc).unwrap(), plain);
    }

    /// Đây là điều `decrypt` fail-open đang nuốt: ciphertext bị sửa MỘT byte
    /// phải bị BẮT (AuthFailed), không được coi như plaintext.
    #[test]
    fn try_decrypt_bat_sua_doi() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        let enc = engine.encrypt("chuyển 1000 cho A").unwrap();

        // v2 = "v2:salt:iv:tag:cipher" → lật byte cuối của CIPHER (phần thứ 5).
        let mut parts: Vec<String> = enc.split(':').map(|s| s.to_string()).collect();
        assert_eq!(parts.len(), 5);
        let last = parts[4].pop().unwrap();
        parts[4].push(if last == 'a' { 'b' } else { 'a' });
        let bi_sua = parts.join(":");

        assert_eq!(engine.try_decrypt(&bi_sua), Err(DecryptError::AuthFailed),
            "ciphertext bi sua doi PHAI bi bat, khong duoc coi la plaintext");
        // Đối chiếu: decrypt cũ nuốt im lặng, trả lại chuỗi bị sửa.
        assert_eq!(engine.decrypt(&bi_sua), bi_sua, "decrypt cu van fail-open (giu nguyen de migration)");
    }

    #[test]
    fn try_decrypt_phan_biet_plaintext_va_hong() {
        let engine = EncryptionEngine::new("00000000000000000000000000000000");
        assert_eq!(engine.try_decrypt("chua tung ma hoa"), Err(DecryptError::NotEncrypted));
        assert_eq!(engine.try_decrypt("khong-phai-hex:zz:yy"), Err(DecryptError::BadFormat));
        assert_eq!(engine.try_decrypt("aa:bb:cc"), Err(DecryptError::BadFormat), "iv/tag sai do dai");
    }

    /// Sai khoá cũng cho AuthFailed — không giải mã nhầm bằng khoá khác.
    #[test]
    fn try_decrypt_sai_khoa() {
        let a = EncryptionEngine::new("00000000000000000000000000000000");
        let b = EncryptionEngine::new("11111111111111111111111111111111");
        let enc = a.encrypt("x").unwrap();
        assert_eq!(b.try_decrypt(&enc), Err(DecryptError::AuthFailed));
    }
}
