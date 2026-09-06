use aes_gcm::aead::consts::U16;
use aes_gcm::{
    aead::{Aead, KeyInit},
    AesGcm, Nonce,
};
use hkdf::Hkdf;
use rand::RngCore;
use sha2::{Digest, Sha256};
use zeroize::Zeroizing;

use crate::crypto::FactRead;

type Aes256Gcm16 = AesGcm<aes_gcm::aes::Aes256, U16>;

pub const ENCLAVE_V2_PREFIX: &str = "v2:";
pub const SALT_BYTES: usize = 16;
pub const NONCE_BYTES: usize = 16;
pub const TAG_BYTES: usize = 16;
pub const HKDF_INFO: &[u8] = b"liva-memory-enclave-v2";

/// Errors that can occur during MemoryEnclave operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum EnclaveError {
    #[error("Argon2 parameter error: {0}")]
    Argon2Params(String),

    #[error("Argon2 key derivation error: {0}")]
    Argon2Derivation(String),

    #[error("Invalid envelope prefix: expected 'v2:'")]
    InvalidPrefix,

    #[error("Malformed envelope: expected 4 colon-separated fields (salt:iv:tag:ciphertext)")]
    MalformedEnvelope,

    #[error("Hex decoding error: {0}")]
    HexDecode(String),

    #[error("Authentication failed: ciphertext corrupted, tampered, or wrong key")]
    AuthenticationFailed,

    #[error("Ciphertext payload is too short")]
    PayloadTooShort,

    #[error("Decrypted payload is not valid UTF-8: {0}")]
    NotUtf8(String),

    #[error("Database error during WAL sanitization: {0}")]
    DatabaseError(String),
}

/// AES-256-GCM v2 Memory Enclave with Argon2id Master Key Derivation (RFC 9106)
/// and per-record HKDF-SHA256 derivation with zeroizing in-memory buffers.
pub struct MemoryEnclave {
    master_key: Zeroizing<[u8; 32]>,
}

impl MemoryEnclave {
    /// Derive a 256-bit Master Key from a passphrase and salt using Argon2id.
    /// Parameters: M=64MB (65536 KiB), T=3 iterations, P=4 parallel lanes.
    pub fn new_with_argon2id(passphrase: &[u8], master_salt: &[u8]) -> Result<Self, EnclaveError> {
        let mut key = [0u8; 32];
        let params = argon2::Params::new(65536, 3, 4, Some(32))
            .map_err(|e| EnclaveError::Argon2Params(e.to_string()))?;
        let argon = argon2::Argon2::new(
            argon2::Algorithm::Argon2id,
            argon2::Version::V0x13,
            params,
        );
        argon
            .hash_password_into(passphrase, master_salt, &mut key)
            .map_err(|e| EnclaveError::Argon2Derivation(e.to_string()))?;

        Ok(Self {
            master_key: Zeroizing::new(key),
        })
    }

    /// Construct an enclave directly from an existing 256-bit master key.
    pub fn new_from_master_key(master_key: [u8; 32]) -> Self {
        Self {
            master_key: Zeroizing::new(master_key),
        }
    }

    /// Return an opaque identifier for the master key (SHA-256 with domain separation).
    /// Used for telemetry, auditing, and key rotation without leaking the master key.
    pub fn key_id(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"liva-memory-enclave-key-id-v2\0");
        hasher.update(&*self.master_key);
        let hash = hasher.finalize();
        hex::encode(&hash[..16])
    }

    /// Derive a per-record 256-bit AES key using HKDF-SHA256 with record salt.
    fn derive_record_key(&self, record_salt: &[u8]) -> Zeroizing<[u8; 32]> {
        let hk = Hkdf::<Sha256>::new(Some(record_salt), &*self.master_key);
        let mut okm = [0u8; 32];
        hk.expand(HKDF_INFO, &mut okm)
            .expect("HKDF-SHA256 32-byte expansion is guaranteed to succeed");
        Zeroizing::new(okm)
    }

    /// Check if a string conforms to the `v2:<salt>:<iv>:<tag>:<cipher>` format.
    pub fn is_valid_envelope(envelope: &str) -> bool {
        if !envelope.starts_with(ENCLAVE_V2_PREFIX) {
            return false;
        }
        let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
        if parts.len() != 4 {
            return false;
        }
        // Validate salt, iv, and tag hex lengths
        let salt_ok = parts[0].len() == SALT_BYTES * 2 && hex::decode(parts[0]).is_ok();
        let iv_ok = parts[1].len() == NONCE_BYTES * 2 && hex::decode(parts[1]).is_ok();
        let tag_ok = parts[2].len() == TAG_BYTES * 2 && hex::decode(parts[2]).is_ok();
        let cipher_ok = parts[3].len() % 2 == 0 && hex::decode(parts[3]).is_ok();
        salt_ok && iv_ok && tag_ok && cipher_ok
    }

    /// Encrypt raw binary bytes into a v2 authenticated envelope format:
    /// `v2:<salt_hex>:<iv_hex>:<tag_hex>:<ciphertext_hex>`
    pub fn encrypt_record(&self, plaintext: &[u8]) -> Result<String, EnclaveError> {
        let mut salt = [0u8; SALT_BYTES];
        let mut iv = [0u8; NONCE_BYTES];
        rand::rngs::OsRng.fill_bytes(&mut salt);
        rand::rngs::OsRng.fill_bytes(&mut iv);

        let record_key = self.derive_record_key(&salt);
        let cipher = Aes256Gcm16::new_from_slice(&*record_key)
            .map_err(|_| EnclaveError::AuthenticationFailed)?;
        let nonce = Nonce::<U16>::from_slice(&iv);

        let ciphertext_with_tag = cipher
            .encrypt(nonce, plaintext)
            .map_err(|_| EnclaveError::AuthenticationFailed)?;

        if ciphertext_with_tag.len() < TAG_BYTES {
            return Err(EnclaveError::PayloadTooShort);
        }

        let split_idx = ciphertext_with_tag.len() - TAG_BYTES;
        let ciphertext = &ciphertext_with_tag[..split_idx];
        let tag = &ciphertext_with_tag[split_idx..];

        Ok(format!(
            "{}{}:{}:{}:{}",
            ENCLAVE_V2_PREFIX,
            hex::encode(salt),
            hex::encode(iv),
            hex::encode(tag),
            hex::encode(ciphertext)
        ))
    }

    /// Encrypt a string into a v2 authenticated envelope format.
    pub fn encrypt_string(&self, plaintext: &str) -> Result<String, EnclaveError> {
        self.encrypt_record(plaintext.as_bytes())
    }

    /// Decrypt a v2 envelope and return the plaintext in a zeroized buffer.
    pub fn decrypt_record(&self, envelope: &str) -> Result<Zeroizing<Vec<u8>>, EnclaveError> {
        if !envelope.starts_with(ENCLAVE_V2_PREFIX) {
            return Err(EnclaveError::InvalidPrefix);
        }
        let rest = &envelope[ENCLAVE_V2_PREFIX.len()..];
        let parts: Vec<&str> = rest.split(':').collect();
        if parts.len() != 4 {
            return Err(EnclaveError::MalformedEnvelope);
        }
        if parts[0].len() != SALT_BYTES * 2
            || parts[1].len() != NONCE_BYTES * 2
            || parts[2].len() != TAG_BYTES * 2
            || parts[3].len() % 2 != 0
        {
            return Err(EnclaveError::MalformedEnvelope);
        }

        let salt = hex::decode(parts[0]).map_err(|e| EnclaveError::HexDecode(e.to_string()))?;
        let iv = hex::decode(parts[1]).map_err(|e| EnclaveError::HexDecode(e.to_string()))?;
        let tag = hex::decode(parts[2]).map_err(|e| EnclaveError::HexDecode(e.to_string()))?;
        let ciphertext = hex::decode(parts[3]).map_err(|e| EnclaveError::HexDecode(e.to_string()))?;

        let record_key = self.derive_record_key(&salt);
        let cipher = Aes256Gcm16::new_from_slice(&*record_key)
            .map_err(|_| EnclaveError::AuthenticationFailed)?;
        let nonce = Nonce::<U16>::from_slice(&iv);

        let mut combined = ciphertext;
        combined.extend_from_slice(&tag);

        let plaintext = cipher
            .decrypt(nonce, combined.as_ref())
            .map_err(|_| EnclaveError::AuthenticationFailed)?;

        Ok(Zeroizing::new(plaintext))
    }

    /// Decrypt a v2 envelope and return the plaintext string in a zeroized wrapper.
    pub fn decrypt_string(&self, envelope: &str) -> Result<Zeroizing<String>, EnclaveError> {
        let bytes = self.decrypt_record(envelope)?;
        let s = String::from_utf8(bytes.to_vec())
            .map_err(|e| EnclaveError::NotUtf8(e.to_string()))?;
        Ok(Zeroizing::new(s))
    }

    /// Fail-closed fact reader that maps errors to opaque `FactRead::Locked`
    /// preventing ciphertext leakage to callers or prompts.
    pub fn read_record(&self, envelope: &str) -> FactRead {
        match self.decrypt_string(envelope) {
            Ok(zeroized_str) => FactRead::Ok((*zeroized_str).clone()),
            Err(EnclaveError::AuthenticationFailed) => FactRead::Locked {
                reason: "auth_failed",
            },
            Err(EnclaveError::NotUtf8(_)) => FactRead::Locked {
                reason: "not_utf8",
            },
            Err(_) => FactRead::Locked {
                reason: "locked",
            },
        }
    }

    /// Rotate encryption of an existing envelope to a new target enclave.
    pub fn rotate_envelope(&self, old_envelope: &str, new_enclave: &MemoryEnclave) -> Result<String, EnclaveError> {
        let plaintext = self.decrypt_record(old_envelope)?;
        new_enclave.encrypt_record(&plaintext)
    }

    /// Execute database pragmas to sanitize SQLite WAL and prevent plaintext leaks.
    pub fn sanitize_wal_checkpoint(conn: &rusqlite::Connection) -> Result<(), EnclaveError> {
        conn.execute_batch(
            "PRAGMA secure_delete = ON;
             PRAGMA temp_store = MEMORY;
             PRAGMA wal_checkpoint(TRUNCATE);"
        ).map_err(|e| EnclaveError::DatabaseError(e.to_string()))?;
        Ok(())
    }
}
