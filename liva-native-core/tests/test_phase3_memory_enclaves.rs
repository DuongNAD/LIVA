use liva_native_core::crypto::FactRead;
use liva_native_core::memory::enclave::{
    EnclaveError, MemoryEnclave, ENCLAVE_V2_PREFIX,
};

#[test]
fn test_argon2id_derivation_deterministic() {
    let passphrase = b"SuperSecretMasterPassword123!";
    let salt1 = b"0123456789abcdef";
    let salt2 = b"fedcba9876543210";

    let enc1 = MemoryEnclave::new_with_argon2id(passphrase, salt1).expect("Enclave init failed");
    let enc1_dup = MemoryEnclave::new_with_argon2id(passphrase, salt1).expect("Enclave init failed");
    let enc2 = MemoryEnclave::new_with_argon2id(passphrase, salt2).expect("Enclave init failed");

    // Same passphrase + salt -> identical key_id
    assert_eq!(enc1.key_id(), enc1_dup.key_id());
    // Different salt -> distinct key_id
    assert_ne!(enc1.key_id(), enc2.key_id());
}

#[test]
fn test_encrypt_decrypt_roundtrip_bytes_and_string() {
    let passphrase = b"EnclavePassphrase2026";
    let salt = b"master_salt_1234";
    let enclave = MemoryEnclave::new_with_argon2id(passphrase, salt).unwrap();

    let raw_data = b"Sensitive binary data \x00\x01\x02\xff payload";
    let envelope = enclave.encrypt_record(raw_data).expect("Encryption failed");

    assert!(envelope.starts_with(ENCLAVE_V2_PREFIX));
    assert!(MemoryEnclave::is_valid_envelope(&envelope));

    let decrypted = enclave.decrypt_record(&envelope).expect("Decryption failed");
    assert_eq!(&*decrypted, raw_data);

    let text_data = "Người dùng thích lập trình Rust và AI trên macOS.";
    let text_envelope = enclave.encrypt_string(text_data).expect("Text encryption failed");
    let decrypted_text = enclave.decrypt_string(&text_envelope).expect("Text decryption failed");
    assert_eq!(&*decrypted_text, text_data);
}

#[test]
fn test_random_salts_and_nonces_per_record() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt123456789012").unwrap();
    let text = "Identical Plaintext Message";

    let env1 = enclave.encrypt_string(text).unwrap();
    let env2 = enclave.encrypt_string(text).unwrap();

    // Two encryptions of identical plaintext must yield distinct ciphertexts
    assert_ne!(env1, env2);

    assert_eq!(&*enclave.decrypt_string(&env1).unwrap(), text);
    assert_eq!(&*enclave.decrypt_string(&env2).unwrap(), text);
}

#[test]
fn test_tampered_ciphertext_fails_authentication() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt123456789012").unwrap();
    let env = enclave.encrypt_string("Secret information").unwrap();

    let mut parts: Vec<String> = env[ENCLAVE_V2_PREFIX.len()..].split(':').map(|s| s.to_string()).collect();
    assert_eq!(parts.len(), 4);

    // 1. Tamper with ciphertext payload
    let mut bad_cipher_bytes = hex::decode(&parts[3]).unwrap();
    if let Some(first_byte) = bad_cipher_bytes.first_mut() {
        *first_byte ^= 0x55;
    }
    parts[3] = hex::encode(bad_cipher_bytes);
    let tampered_env = format!("{}{}", ENCLAVE_V2_PREFIX, parts.join(":"));

    let err = enclave.decrypt_string(&tampered_env).unwrap_err();
    assert_eq!(err, EnclaveError::AuthenticationFailed);

    // 2. Tamper with tag
    let mut bad_tag_bytes = hex::decode(&parts[2]).unwrap();
    bad_tag_bytes[0] ^= 0xAA;
    parts[2] = hex::encode(bad_tag_bytes);
    let tampered_tag_env = format!("{}{}", ENCLAVE_V2_PREFIX, parts.join(":"));

    let err_tag = enclave.decrypt_string(&tampered_tag_env).unwrap_err();
    assert_eq!(err_tag, EnclaveError::AuthenticationFailed);
}

#[test]
fn test_fail_closed_fact_read() {
    let enclave = MemoryEnclave::new_with_argon2id(b"correct_pass", b"salt_0000000000").unwrap();
    let wrong_enclave = MemoryEnclave::new_with_argon2id(b"wrong_pass", b"salt_0000000000").unwrap();

    let secret = "User Credit Card or Sensitive Personal Fact";
    let envelope = enclave.encrypt_string(secret).unwrap();

    // Correct key
    let read_ok = enclave.read_record(&envelope);
    assert_eq!(read_ok, FactRead::Ok(secret.to_string()));

    // Wrong key fail-closed
    let read_locked = wrong_enclave.read_record(&envelope);
    assert!(read_locked.is_locked());
    assert_eq!(read_locked, FactRead::Locked { reason: "auth_failed" });
    // Verify ciphertext is NOT leaked
    assert_eq!(read_locked.into_value(), "");

    // Malformed envelope
    let malformed_read = enclave.read_record("v2:invalid:envelope");
    assert!(malformed_read.is_locked());
}

#[test]
fn test_key_rotation() {
    let enclave_v1 = MemoryEnclave::new_with_argon2id(b"old_passphrase", b"old_salt_123456").unwrap();
    let enclave_v2 = MemoryEnclave::new_with_argon2id(b"new_passphrase", b"new_salt_654321").unwrap();

    let text = "Secret note to be migrated across enclave keys.";
    let old_envelope = enclave_v1.encrypt_string(text).unwrap();

    let new_envelope = enclave_v1.rotate_envelope(&old_envelope, &enclave_v2).expect("Rotation failed");

    // Enclave v2 successfully decrypts rotated envelope
    let decrypted = enclave_v2.decrypt_string(&new_envelope).unwrap();
    assert_eq!(&*decrypted, text);

    // Enclave v1 cannot decrypt new envelope
    assert_eq!(
        enclave_v1.decrypt_string(&new_envelope).unwrap_err(),
        EnclaveError::AuthenticationFailed
    );
}

#[test]
fn test_sqlite_wal_sanitization() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    let res = MemoryEnclave::sanitize_wal_checkpoint(&conn);
    assert!(res.is_ok());
}

#[test]
fn test_adversarial_and_corrupt_inputs() {
    let enclave = MemoryEnclave::new_with_argon2id(b"test_pass", b"test_salt_12345").unwrap();

    assert_eq!(enclave.decrypt_string("").unwrap_err(), EnclaveError::InvalidPrefix);
    assert_eq!(enclave.decrypt_string("v1:old:format").unwrap_err(), EnclaveError::InvalidPrefix);
    assert_eq!(enclave.decrypt_string("v2:part1:part2").unwrap_err(), EnclaveError::MalformedEnvelope);
    assert_eq!(enclave.decrypt_string("v2:not_hex:not_hex:not_hex:not_hex").unwrap_err(), EnclaveError::MalformedEnvelope);
    assert!(!MemoryEnclave::is_valid_envelope("random text string"));
}
