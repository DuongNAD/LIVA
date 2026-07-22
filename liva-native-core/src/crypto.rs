use aes_gcm::aead::consts::U16;
use aes_gcm::{
    AesGcm, Nonce,
    aead::{Aead, KeyInit},
};
use rand::RngCore;

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

pub struct EncryptionEngine {
    key: [u8; 32],
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
        let mut key = [0u8; 32];
        let bytes = key_str.as_bytes();
        let len = bytes.len().min(32);
        key[..len].copy_from_slice(&bytes[..len]);
        Self { key }
    }

    pub fn encrypt(&self, text: &str) -> Result<String, String> {
        let mut iv = [0u8; 16];
        rand::rngs::OsRng.fill_bytes(&mut iv);

        let cipher = Aes256Gcm16::new_from_slice(&self.key).map_err(|e| e.to_string())?;

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

        let iv_hex = hex::encode(iv);
        let tag_hex = hex::encode(tag);
        let ciphertext_hex = hex::encode(ciphertext);

        Ok(format!("{}:{}:{}", iv_hex, tag_hex, ciphertext_hex))
    }

    pub fn decrypt(&self, text: &str) -> String {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 3 {
            return text.to_string();
        }

        let iv_bytes = match hex::decode(parts[0]) {
            Ok(b) => b,
            Err(_) => return text.to_string(),
        };
        let tag_bytes = match hex::decode(parts[1]) {
            Ok(b) => b,
            Err(_) => return text.to_string(),
        };
        let cipher_bytes = match hex::decode(parts[2]) {
            Ok(b) => b,
            Err(_) => return text.to_string(),
        };

        if iv_bytes.len() != 16 || tag_bytes.len() != 16 {
            return text.to_string();
        }

        let cipher = match Aes256Gcm16::new_from_slice(&self.key) {
            Ok(c) => c,
            Err(_) => return text.to_string(),
        };

        let nonce = Nonce::<U16>::from_slice(&iv_bytes);

        // Standard rust aes-gcm expects tag appended at the end of the ciphertext
        let mut payload = cipher_bytes;
        payload.extend_from_slice(&tag_bytes);

        match cipher.decrypt(nonce, payload.as_slice()) {
            Ok(plain) => String::from_utf8(plain).unwrap_or_else(|_| text.to_string()),
            Err(_) => text.to_string(),
        }
    }

    /// Giải mã **fail-CLOSED**: trả `Err` khi dữ liệu bị sửa đổi (AuthFailed),
    /// hỏng, hoặc chưa mã hoá — thay vì âm thầm trả lại chuỗi đầu vào như
    /// [`decrypt`](Self::decrypt).
    ///
    /// Thêm 22/07/2026 (lộ trình P2 crypto). PHỤ TRỢ, KHÔNG đổi hành vi hiện
    /// có: `decrypt` giữ nguyên fail-open để không phá đường migration
    /// plaintext đang được dựa vào. Đây là primitive để nơi gọi CHỌN phát hiện
    /// giả mạo khi cần — việc nối nó vào bảng `facts` (và có thêm KDF) là quyết
    /// định có hệ quả migration, để người dùng chọn.
    ///
    /// Điểm mấu chốt: tag xác thực của AES-GCM sinh ra để **phát hiện** sửa
    /// đổi; `decrypt` fail-open đang vứt tín hiệu đó. `try_decrypt` giữ lại.
    pub fn try_decrypt(&self, text: &str) -> Result<String, DecryptError> {
        let parts: Vec<&str> = text.split(':').collect();
        if parts.len() != 3 {
            return Err(DecryptError::NotEncrypted);
        }

        let iv_bytes = hex::decode(parts[0]).map_err(|_| DecryptError::BadFormat)?;
        let tag_bytes = hex::decode(parts[1]).map_err(|_| DecryptError::BadFormat)?;
        let cipher_bytes = hex::decode(parts[2]).map_err(|_| DecryptError::BadFormat)?;

        if iv_bytes.len() != 16 || tag_bytes.len() != 16 {
            return Err(DecryptError::BadFormat);
        }

        let cipher = Aes256Gcm16::new_from_slice(&self.key).map_err(|_| DecryptError::BadFormat)?;
        let nonce = Nonce::<U16>::from_slice(&iv_bytes);

        let mut payload = cipher_bytes;
        payload.extend_from_slice(&tag_bytes);

        // Err ở đây = tag không khớp = ciphertext bị sửa đổi HOẶC sai khoá.
        let plain = cipher
            .decrypt(nonce, payload.as_slice())
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

        let parts: Vec<&str> = encrypted.split(':').collect();
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0].len(), 32); // 16 bytes in hex = 32 chars
        assert_eq!(parts[1].len(), 32); // 16 bytes in hex = 32 chars

        let decrypted = engine.decrypt(&encrypted);
        assert_eq!(plain, decrypted);
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

        // Lật byte cuối của ciphertext (phần thứ 3).
        let mut parts: Vec<String> = enc.split(':').map(|s| s.to_string()).collect();
        let last = parts[2].pop().unwrap();
        parts[2].push(if last == 'a' { 'b' } else { 'a' });
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
