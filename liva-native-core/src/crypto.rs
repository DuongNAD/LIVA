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
}
