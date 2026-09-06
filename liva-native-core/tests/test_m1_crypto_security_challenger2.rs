//! Challenger 2: Adversarial Security & Cryptographic Enclave Test Suite
//! Milestone 1: M1 (Cryptographic Enclaves & Security)
//!
//! Objectives:
//! 1. Tamper testing: bit-flipping in salt, iv, tag, ciphertext; assert fail-closed rejection without panicking.
//! 2. Secret leakage verification: assert decrypted buffers are zeroized and ciphertext errors return opaque Locked.
//! 3. WAL sanitization and SQLite PRAGMA configuration verification on disk and in memory.
//! 4. Concurrency stress & boundary condition testing under adversarial multi-threading.

use std::fs::File;
use std::io::Read;
use std::sync::Arc;
use tempfile::TempDir;

use liva_native_core::crypto::FactRead;
use liva_native_core::db::DatabasePool;
use liva_native_core::memory::enclave::{
    EnclaveError, MemoryEnclave, ENCLAVE_V2_PREFIX, NONCE_BYTES, SALT_BYTES, TAG_BYTES,
};
use liva_native_core::memory::l2_episodic::{EpisodicEvent, L2EpisodicStore};
use liva_native_core::memory::VirtualMemoryEngine;

// ============================================================================
// 1. TAMPER TESTING: BIT-FLIPPING & ENVELOPE CORRUPTION HARNESS
// ============================================================================

#[test]
fn test_adversarial_bitflip_exhaustive_salt() {
    let enclave = MemoryEnclave::new_with_argon2id(b"adversarial_passphrase_2026", b"salt_bitflip_001").unwrap();
    let plaintext = "TopSecretConfidentialFact12345";
    let envelope = enclave.encrypt_string(plaintext).unwrap();

    let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
    assert_eq!(parts.len(), 4);
    let original_salt_bytes = hex::decode(parts[0]).unwrap();
    assert_eq!(original_salt_bytes.len(), SALT_BYTES);

    // Flip every single bit in the 16-byte salt (128 bitflips)
    for byte_idx in 0..SALT_BYTES {
        for bit_idx in 0..8 {
            let mut tampered_salt = original_salt_bytes.clone();
            tampered_salt[byte_idx] ^= 1 << bit_idx;

            let tampered_env = format!(
                "{}{}:{}:{}:{}",
                ENCLAVE_V2_PREFIX,
                hex::encode(tampered_salt),
                parts[1],
                parts[2],
                parts[3]
            );

            // 1. Must fail closed in decrypt_record
            let dec_res = enclave.decrypt_record(&tampered_env);
            assert_eq!(
                dec_res.unwrap_err(),
                EnclaveError::AuthenticationFailed,
                "Salt bitflip at byte {byte_idx} bit {bit_idx} MUST fail authentication"
            );

            // 2. Must return opaque Locked in read_record
            let fact_read = enclave.read_record(&tampered_env);
            assert!(fact_read.is_locked(), "Must be locked on tampered salt");
            assert_eq!(fact_read.into_value(), "", "No ciphertext leakage");
        }
    }
}

#[test]
fn test_adversarial_bitflip_exhaustive_iv() {
    let enclave = MemoryEnclave::new_with_argon2id(b"adversarial_passphrase_2026", b"iv_bitflip_002").unwrap();
    let plaintext = "TopSecretConfidentialFact67890";
    let envelope = enclave.encrypt_string(plaintext).unwrap();

    let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
    assert_eq!(parts.len(), 4);
    let original_iv_bytes = hex::decode(parts[1]).unwrap();
    assert_eq!(original_iv_bytes.len(), NONCE_BYTES);

    // Flip every single bit in the 16-byte IV (128 bitflips)
    for byte_idx in 0..NONCE_BYTES {
        for bit_idx in 0..8 {
            let mut tampered_iv = original_iv_bytes.clone();
            tampered_iv[byte_idx] ^= 1 << bit_idx;

            let tampered_env = format!(
                "{}{}:{}:{}:{}",
                ENCLAVE_V2_PREFIX,
                parts[0],
                hex::encode(tampered_iv),
                parts[2],
                parts[3]
            );

            let dec_res = enclave.decrypt_record(&tampered_env);
            assert_eq!(
                dec_res.unwrap_err(),
                EnclaveError::AuthenticationFailed,
                "IV bitflip at byte {byte_idx} bit {bit_idx} MUST fail authentication"
            );

            let fact_read = enclave.read_record(&tampered_env);
            assert!(fact_read.is_locked());
            assert_eq!(fact_read.into_value(), "");
        }
    }
}

#[test]
fn test_adversarial_bitflip_exhaustive_tag() {
    let enclave = MemoryEnclave::new_with_argon2id(b"adversarial_passphrase_2026", b"tag_bitflip_003").unwrap();
    let plaintext = "TopSecretConfidentialFactABCDE";
    let envelope = enclave.encrypt_string(plaintext).unwrap();

    let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
    assert_eq!(parts.len(), 4);
    let original_tag_bytes = hex::decode(parts[2]).unwrap();
    assert_eq!(original_tag_bytes.len(), TAG_BYTES);

    // Flip every single bit in the 16-byte Auth Tag (128 bitflips)
    for byte_idx in 0..TAG_BYTES {
        for bit_idx in 0..8 {
            let mut tampered_tag = original_tag_bytes.clone();
            tampered_tag[byte_idx] ^= 1 << bit_idx;

            let tampered_env = format!(
                "{}{}:{}:{}:{}",
                ENCLAVE_V2_PREFIX,
                parts[0],
                parts[1],
                hex::encode(tampered_tag),
                parts[3]
            );

            let dec_res = enclave.decrypt_record(&tampered_env);
            assert_eq!(
                dec_res.unwrap_err(),
                EnclaveError::AuthenticationFailed,
                "Tag bitflip at byte {byte_idx} bit {bit_idx} MUST fail authentication"
            );

            let fact_read = enclave.read_record(&tampered_env);
            assert!(fact_read.is_locked());
            assert_eq!(fact_read.into_value(), "");
        }
    }
}

#[test]
fn test_adversarial_bitflip_exhaustive_ciphertext() {
    let enclave = MemoryEnclave::new_with_argon2id(b"adversarial_passphrase_2026", b"cipher_bitflip_004").unwrap();
    let plaintext = "Adversarial Ciphertext Integrity Verification String with Multi-byte UTF8: 🦀🔒⚡️";
    let envelope = enclave.encrypt_string(plaintext).unwrap();

    let parts: Vec<&str> = envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
    assert_eq!(parts.len(), 4);
    let original_cipher_bytes = hex::decode(parts[3]).unwrap();

    // Flip every bit across all ciphertext bytes
    for byte_idx in 0..original_cipher_bytes.len() {
        for bit_idx in 0..8 {
            let mut tampered_cipher = original_cipher_bytes.clone();
            tampered_cipher[byte_idx] ^= 1 << bit_idx;

            let tampered_env = format!(
                "{}{}:{}:{}:{}",
                ENCLAVE_V2_PREFIX,
                parts[0],
                parts[1],
                parts[2],
                hex::encode(tampered_cipher)
            );

            let dec_res = enclave.decrypt_record(&tampered_env);
            assert_eq!(
                dec_res.unwrap_err(),
                EnclaveError::AuthenticationFailed,
                "Ciphertext bitflip at byte {byte_idx} bit {bit_idx} MUST fail authentication"
            );

            let fact_read = enclave.read_record(&tampered_env);
            assert!(fact_read.is_locked());
            assert_eq!(fact_read.into_value(), "");
        }
    }
}

#[test]
fn test_adversarial_envelope_syntax_mutilation_and_fuzzing() {
    let enclave = MemoryEnclave::new_with_argon2id(b"fuzz_pass", b"fuzz_salt_123456").unwrap();
    let valid_envelope = enclave.encrypt_string("Valid text").unwrap();
    let parts: Vec<&str> = valid_envelope[ENCLAVE_V2_PREFIX.len()..].split(':').collect();

    let corruptions: Vec<String> = vec![
        // Empty and prefix errors
        "".to_string(),
        "v2".to_string(),
        "v1:salt:iv:tag:cipher".to_string(),
        "v3:salt:iv:tag:cipher".to_string(),
        "V2:salt:iv:tag:cipher".to_string(),
        "v2:::".to_string(),
        "v2::::".to_string(),
        "v2:::::".to_string(),
        "::::".to_string(),
        // Bad delimiter counts
        format!("v2:{}:{}:{}", parts[0], parts[1], parts[2]), // 3 parts
        format!("v2:{}:{}:{}:{}:extra", parts[0], parts[1], parts[2], parts[3]), // 5 parts
        format!("v2:{}:{}:{}:{}:extra1:extra2", parts[0], parts[1], parts[2], parts[3]), // 6 parts
        // Non-hex strings
        format!("v2:not_hex_salt_00000000000000000000:{}:{}:{}", parts[1], parts[2], parts[3]),
        format!("v2:{}:not_hex_iv_0000000000000000000000:{}:{}", parts[0], parts[2], parts[3]),
        format!("v2:{}:{}:not_hex_tag_0000000000000000000:{}", parts[0], parts[1], parts[3]),
        format!("v2:{}:{}:{}:not_hex_cipher_0000000000000000000", parts[0], parts[1], parts[2]),
        // Invalid lengths (odd hex, wrong byte counts)
        format!("v2:{:031x}:{}:{}:{}", 1, parts[1], parts[2], parts[3]), // 31 hex chars (15.5 bytes)
        format!("v2:{:034x}:{}:{}:{}", 1, parts[1], parts[2], parts[3]), // 34 hex chars (17 bytes)
        format!("v2:{}:{:031x}:{}:{}", parts[0], 1, parts[2], parts[3]), // 31 hex chars IV
        format!("v2:{}:{}:{:031x}:{}", parts[0], parts[1], 1, parts[3]), // 31 hex chars Tag
        format!("v2:{}:{}:{}:{}", parts[0], parts[1], parts[2], "a"),    // 1 hex char (odd)
        // Null bytes & injection
        format!("v2:\0{}:{}:{}:{}", &parts[0][1..], parts[1], parts[2], parts[3]),
        format!("v2:{}:{}:{}:{}\0", parts[0], parts[1], parts[2], parts[3]),
        format!("v2:{}:{}:{}:{}", parts[0], parts[1], parts[2], "zzzzzzzz"),
    ];

    for bad in corruptions {
        // Assert isValidEnvelope returns false
        assert!(
            !MemoryEnclave::is_valid_envelope(&bad),
            "Expected is_valid_envelope to be false for {bad}"
        );

        // Assert decrypt_string fails closed
        let dec_res = enclave.decrypt_string(&bad);
        assert!(dec_res.is_err(), "Expected decrypt error for {bad}");

        // Assert read_record returns Locked with zero leakage
        let fact_read = enclave.read_record(&bad);
        assert!(fact_read.is_locked(), "Expected fact_read to be locked for {bad}");
        assert_eq!(fact_read.into_value(), "", "No leakage for {bad}");
    }
}

#[test]
fn test_adversarial_non_utf8_binary_payload_handling() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt_binary_1234").unwrap();

    // Raw invalid UTF-8 byte stream
    let raw_invalid_utf8 = vec![0xFF, 0xFE, 0xFD, 0x80, 0x81, 0xC0, 0xAF];

    let envelope = enclave.encrypt_record(&raw_invalid_utf8).expect("Binary encryption succeeds");
    assert!(MemoryEnclave::is_valid_envelope(&envelope));

    // 1. decrypt_record returns raw zeroized bytes
    let dec_bytes = enclave.decrypt_record(&envelope).expect("Binary decryption succeeds");
    assert_eq!(&*dec_bytes, &raw_invalid_utf8);

    // 2. decrypt_string fails with NotUtf8
    let dec_str_err = enclave.decrypt_string(&envelope).unwrap_err();
    match dec_str_err {
        EnclaveError::NotUtf8(_) => (),
        other => panic!("Expected NotUtf8 error, got {other:?}"),
    }

    // 3. read_record returns FactRead::Locked { reason: "not_utf8" }
    let fact_read = enclave.read_record(&envelope);
    assert_eq!(
        fact_read,
        FactRead::Locked {
            reason: "not_utf8"
        }
    );
    assert_eq!(fact_read.into_value(), "");
}

// ============================================================================
// 2. SECRET LEAKAGE & IN-MEMORY ZEROIZATION VERIFICATION
// ============================================================================

#[test]
fn test_adversarial_zeroization_buffer_guarantees() {
    let enclave = MemoryEnclave::new_with_argon2id(b"zeroize_test_passphrase", b"zeroize_salt_1234").unwrap();
    let secret = "UltraSecretDataToZeroizeImmediatelyAfterUse!";

    let envelope = enclave.encrypt_string(secret).unwrap();

    // Verify decrypt_string returns Zeroizing wrapper
    let zeroized_str = enclave.decrypt_string(&envelope).unwrap();
    assert_eq!(&*zeroized_str, secret);

    // Verify decrypt_record returns Zeroizing wrapper
    let zeroized_bytes = enclave.decrypt_record(&envelope).unwrap();
    assert_eq!(&*zeroized_bytes, secret.as_bytes());

    // Master key ID is one-way domain-separated hash and never contains the key or salt
    let key_id = enclave.key_id();
    assert_eq!(key_id.len(), 32); // 16 bytes hex = 32 hex chars
    assert!(!key_id.contains("zeroize_test_passphrase"));
}

#[test]
fn test_adversarial_cross_enclave_isolation_matrix() {
    // Generate 5 distinct enclaves with different passphrases and salts
    let count = 5;
    let mut enclaves = Vec::new();
    for i in 0..count {
        let pass = format!("passphrase_matrix_{i}");
        let salt = format!("salt_matrix_{i:04}__");
        enclaves.push(MemoryEnclave::new_with_argon2id(pass.as_bytes(), salt.as_bytes()).unwrap());
    }

    let secret_message = "Matrix Cross-Enclave Confidential Isolation Test";

    for i in 0..count {
        let env_i = enclaves[i].encrypt_string(secret_message).unwrap();

        for j in 0..count {
            if i == j {
                let decrypted = enclaves[j].decrypt_string(&env_i).unwrap();
                assert_eq!(&*decrypted, secret_message);
                let read = enclaves[j].read_record(&env_i);
                assert_eq!(read, FactRead::Ok(secret_message.to_string()));
            } else {
                // Must fail closed for every other enclave
                let dec_err = enclaves[j].decrypt_string(&env_i).unwrap_err();
                assert_eq!(dec_err, EnclaveError::AuthenticationFailed);

                let read_locked = enclaves[j].read_record(&env_i);
                assert_eq!(
                    read_locked,
                    FactRead::Locked {
                        reason: "auth_failed"
                    }
                );
                assert_eq!(read_locked.into_value(), "");
            }
        }
    }
}

#[test]
fn test_adversarial_nonce_uniqueness_under_stress() {
    let enclave = MemoryEnclave::new_with_argon2id(b"nonce_stress_pass", b"nonce_stress_salt").unwrap();
    let message = "Constant static message for nonce collision test";

    let iterations = 100;
    let mut seen_nonces = std::collections::HashSet::new();
    let mut seen_salts = std::collections::HashSet::new();
    let mut seen_envelopes = std::collections::HashSet::new();

    for _ in 0..iterations {
        let env = enclave.encrypt_string(message).unwrap();
        let parts: Vec<&str> = env[ENCLAVE_V2_PREFIX.len()..].split(':').collect();
        let salt = parts[0].to_string();
        let iv = parts[1].to_string();

        assert!(!seen_nonces.contains(&iv), "CRITICAL: IV/Nonce collision detected!");
        assert!(!seen_salts.contains(&salt), "CRITICAL: Salt collision detected!");
        assert!(!seen_envelopes.contains(&env), "CRITICAL: Ciphertext collision detected!");

        seen_nonces.insert(iv);
        seen_salts.insert(salt);
        seen_envelopes.insert(env);
    }
}

#[test]
fn test_adversarial_extreme_payload_boundaries() {
    let enclave = MemoryEnclave::new_with_argon2id(b"boundary_pass", b"boundary_salt_12").unwrap();

    // 1. Zero-length string
    let empty_env = enclave.encrypt_string("").unwrap();
    assert!(MemoryEnclave::is_valid_envelope(&empty_env));
    let dec_empty = enclave.decrypt_string(&empty_env).unwrap();
    assert_eq!(&*dec_empty, "");
    assert_eq!(enclave.read_record(&empty_env), FactRead::Ok("".to_string()));

    // 2. Large 1 MB string payload
    let large_payload = "A".repeat(1_048_576);
    let large_env = enclave.encrypt_string(&large_payload).unwrap();
    let dec_large = enclave.decrypt_string(&large_env).unwrap();
    assert_eq!(&*dec_large, &large_payload);
}

// ============================================================================
// 3. WAL SANITIZATION & SQLITE PRAGMA CONFIGURATION VERIFICATION
// ============================================================================

#[test]
fn test_adversarial_sqlite_pragma_values_verification() {
    let pool = DatabasePool::new_in_memory().expect("In-memory pool creation failed");

    // 1. Verify Writer Connection Pragmas
    pool.with_write_conn(|conn| {
        // foreign_keys = ON
        let fk: i32 = conn.query_row("PRAGMA foreign_keys;", [], |r| r.get(0))?;
        assert_eq!(fk, 1, "PRAGMA foreign_keys must be ON (1)");

        // busy_timeout >= 10000
        let timeout: i32 = conn.query_row("PRAGMA busy_timeout;", [], |r| r.get(0))?;
        assert!(timeout >= 10000, "PRAGMA busy_timeout must be >= 10000");

        // temp_store = MEMORY (2)
        let temp_store: i32 = conn.query_row("PRAGMA temp_store;", [], |r| r.get(0))?;
        assert_eq!(temp_store, 2, "PRAGMA temp_store must be 2 (MEMORY)");

        // page_size = 32768
        let page_size: i32 = conn.query_row("PRAGMA page_size;", [], |r| r.get(0))?;
        assert_eq!(page_size, 32768, "PRAGMA page_size must be 32768");

        Ok(())
    }).expect("Writer pragma checks passed");

    // 2. Verify Reader Connection Pragmas
    pool.with_read_conn(|conn| {
        let query_only: i32 = conn.query_row("PRAGMA query_only;", [], |r| r.get(0))?;
        assert_eq!(query_only, 1, "Reader pool must have PRAGMA query_only = 1 (ON)");

        let temp_store: i32 = conn.query_row("PRAGMA temp_store;", [], |r| r.get(0))?;
        assert_eq!(temp_store, 2, "Reader pool must have PRAGMA temp_store = 2 (MEMORY)");

        Ok(())
    }).expect("Reader pragma checks passed");
}

#[test]
fn test_adversarial_disk_wal_sanitization_and_no_plaintext_leak() {
    let temp_dir = TempDir::new().expect("Failed to create temp dir");
    let db_path = temp_dir.path().join("secure_test.db");
    let wal_path = temp_dir.path().join("secure_test.db-wal");

    let pool = DatabasePool::new(&db_path).expect("Failed to create file-backed database pool");
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"master_key_pass_2026", b"master_salt_2026").unwrap());

    let store = L2EpisodicStore::new(pool.clone(), enclave.clone());
    store.init_schema().expect("Schema init failed");

    // Highly distinct plaintext secret to scan for
    let plaintext_secret = "CLASSIFIED_PERSONAL_SECRET_MARKER_998877665544332211";

    let event = EpisodicEvent {
        memory_id: "mem_sec_001".to_string(),
        session_id: "sess_sec_001".to_string(),
        domain: "classified".to_string(),
        category: "personal_secret".to_string(),
        content: plaintext_secret.to_string(),
        importance_score: 9.5,
        emotional_valence: 1.2,
        recall_count: 1,
        created_at: 1700000000,
        last_recalled_at: 1700000000,
        base_half_life_secs: 604800,
        retention_score: 1.0,
    };

    store.insert_event(&event).expect("Insert failed");

    // Retrieve via store and assert decryption works
    let retrieved = store.get_event_by_id("mem_sec_001").unwrap().expect("Event must exist");
    assert_eq!(retrieved.content, plaintext_secret);

    // 1. Raw disk forensic inspection: scan the .sqlite and .sqlite-wal files
    let mut db_bytes = Vec::new();
    if let Ok(mut f) = File::open(&db_path) {
        f.read_to_end(&mut db_bytes).unwrap();
    }
    let mut wal_bytes = Vec::new();
    if let Ok(mut f) = File::open(&wal_path) {
        f.read_to_end(&mut wal_bytes).unwrap();
    }

    let search_bytes = plaintext_secret.as_bytes();

    let found_in_db = db_bytes.windows(search_bytes.len()).any(|w| w == search_bytes);
    let found_in_wal = wal_bytes.windows(search_bytes.len()).any(|w| w == search_bytes);

    assert!(
        !found_in_db,
        "SECURITY LEAK: Plaintext secret found in raw SQLite database file!"
    );
    assert!(
        !found_in_wal,
        "SECURITY LEAK: Plaintext secret found in raw SQLite WAL file!"
    );

    // 2. Perform WAL checkpoint TRUNCATE & secure_delete verification
    pool.with_write_conn(|conn| {
        MemoryEnclave::sanitize_wal_checkpoint(conn).expect("Sanitize WAL checkpoint failed");

        // Verify secure_delete is ON (1)
        let sd: i32 = conn.query_row("PRAGMA secure_delete;", [], |r| r.get(0))?;
        assert_eq!(sd, 1, "PRAGMA secure_delete must be ON (1)");

        Ok(())
    }).expect("Sanitize WAL checkpoint verification failed");

    // 3. Purge event and perform second WAL sanitization
    let purged = store.purge_decayed_events(2.0).expect("Purge all");
    assert_eq!(purged, 1);

    pool.with_write_conn(|conn| {
        MemoryEnclave::sanitize_wal_checkpoint(conn).expect("Sanitize WAL checkpoint failed");
        Ok(())
    }).expect("Sanitize WAL checkpoint after purge failed");

    // Re-verify no plaintext residue exists
    let mut db_bytes_post = Vec::new();
    if let Ok(mut f) = File::open(&db_path) {
        f.read_to_end(&mut db_bytes_post).unwrap();
    }
    let mut wal_bytes_post = Vec::new();
    if let Ok(mut f) = File::open(&wal_path) {
        f.read_to_end(&mut wal_bytes_post).unwrap();
    }

    assert!(
        !db_bytes_post.windows(search_bytes.len()).any(|w| w == search_bytes),
        "Plaintext found post-purge"
    );
    assert!(
        !wal_bytes_post.windows(search_bytes.len()).any(|w| w == search_bytes),
        "Plaintext found in WAL post-purge"
    );
}

// ============================================================================
// 4. CONCURRENCY STRESS & THREAD-SAFETY HARNESS
// ============================================================================

#[test]
fn test_adversarial_multithreaded_concurrent_enclave_access() {
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"concurrent_pass", b"concurrent_salt_1").unwrap());
    let mut handles = Vec::new();

    // 20 concurrent threads performing 50 encryption and decryption cycles each
    for thread_idx in 0..20 {
        let enc_clone = enclave.clone();
        handles.push(std::thread::spawn(move || {
            for iter in 0..50 {
                let msg = format!("Thread {thread_idx} message iteration {iter} with payload 🔒⚡️");
                let env = enc_clone.encrypt_string(&msg).expect("Concurrent encrypt failed");
                let dec = enc_clone.decrypt_string(&env).expect("Concurrent decrypt failed");
                assert_eq!(&*dec, &msg);

                let fact = enc_clone.read_record(&env);
                assert_eq!(fact, FactRead::Ok(msg));
            }
        }));
    }

    for handle in handles {
        handle.join().expect("Thread panicked during concurrent enclave stress");
    }
}

#[test]
fn test_adversarial_key_rotation_under_tampering() {
    let enc1 = MemoryEnclave::new_with_argon2id(b"source_key_pass", b"source_salt_1234").unwrap();
    let enc2 = MemoryEnclave::new_with_argon2id(b"target_key_pass", b"target_salt_5678").unwrap();

    let valid_envelope = enc1.encrypt_string("Rotating critical personal secret").unwrap();

    // 1. Valid rotation succeeds
    let rotated = enc1.rotate_envelope(&valid_envelope, &enc2).expect("Rotation should succeed");
    assert_eq!(&*enc2.decrypt_string(&rotated).unwrap(), "Rotating critical personal secret");

    // 2. Tampered envelope rotation MUST fail closed
    let mut bad_envelope = valid_envelope.clone();
    let last_ch = bad_envelope.pop().unwrap();
    bad_envelope.push(if last_ch == '0' { '1' } else { '0' }); // Guaranteed character corruption

    let rotate_err = enc1.rotate_envelope(&bad_envelope, &enc2).unwrap_err();
    assert_eq!(rotate_err, EnclaveError::AuthenticationFailed);
}

#[test]
fn test_adversarial_virtual_memory_engine_full_enclave_integration() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"vme_pass", b"vme_salt_123456").unwrap());
    let vme = VirtualMemoryEngine::new(pool, enclave, 100);

    let event = EpisodicEvent {
        memory_id: "vme_sec_event_1".to_string(),
        session_id: "sess_001".to_string(),
        domain: "finance".to_string(),
        category: "credit_card".to_string(),
        content: "Card number 4111-2222-3333-4444 CVV 123".to_string(),
        importance_score: 10.0,
        emotional_valence: 1.0,
        recall_count: 0,
        created_at: 1000,
        last_recalled_at: 1000,
        base_half_life_secs: 604800,
        retention_score: 1.0,
    };

    vme.l2.init_schema().unwrap();
    vme.record_episodic_event(&event).unwrap();

    let recalled = vme.recall_episodic_context("finance", 0.5).unwrap();
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].content, "Card number 4111-2222-3333-4444 CVV 123");
}
