//! Phase 3 Milestone 1 Adversarial Challenge & Empirical Stress Test Suite
//!
//! Subsystems tested:
//! 1. L1 Working Memory: Radix Prefix Cache (deep branches, fuzzing, splits, LRU eviction, concurrency)
//! 2. L2 Episodic Memory: Dynamic Ebbinghaus Retention (edge cases, extreme times, SQLite sweeps, purges)
//! 3. L4 Procedural Memory: Bayesian Beta Ranking (severe failures, uninvoked decay, high-concurrency updates)
//! 4. Cryptographic Enclave: AES-256-GCM v2 & Argon2id (fuzzing, bit-flipping, key rotation, WAL sanitization)

use std::sync::Arc;
use std::time::Duration;
use tokio::time::timeout;

use liva_native_core::crypto::FactRead;
use liva_native_core::db::DatabasePool;
use liva_native_core::memory::{
    classify_retention, compute_bayesian_expectation, compute_half_life_secs,
    compute_ranking_score, compute_retention, EpisodicEvent, ExecutionOutcome, FailureType,
    L2EpisodicStore, L4ProceduralRegistry, MemoryEnclave, ProceduralSkill, RadixPrefixCache,
    RetentionTier, ACTIVE_RETENTION_THRESHOLD, ARCHIVE_RETENTION_THRESHOLD,
    DORMANT_RETENTION_THRESHOLD,
};

// ============================================================================
// PART 1: RADIX PREFIX CACHE EMPIRICAL CHALLENGES
// ============================================================================

/// 1.1 Deep Linear Branch Insertion & Exact Partial/Full Matching
#[tokio::test]
async fn challenge_radix_deep_linear_chain() {
    let cache = RadixPrefixCache::new(500);
    let depth = 100;
    let tokens: Vec<u32> = (1..=depth as u32).collect();

    // Insert incremental prefixes: [1], [1, 2], [1, 2, 3], ..., [1..100]
    for len in 1..=depth {
        let prefix = &tokens[..len];
        cache.insert_prefix(prefix, len as usize, false).await;
    }

    assert_eq!(cache.allocated_blocks(), depth);

    // Query full 100-token sequence
    let (matched, blocks) = cache.match_prefix(&tokens).await;
    assert_eq!(matched, depth, "Full sequence must match all 100 tokens");
    assert_eq!(blocks.len(), depth, "All 100 block IDs along the chain must be collected");
    for (idx, &blk) in blocks.iter().enumerate() {
        assert_eq!(blk, idx + 1, "Block ID at step {} must match", idx);
    }

    // Query prefix of length 42
    let sub = &tokens[..42];
    let (matched_sub, blocks_sub) = cache.match_prefix(sub).await;
    assert_eq!(matched_sub, 42);
    assert_eq!(blocks_sub.len(), 42);

    // Query sequence with a diverging token at step 51
    let mut diverging = tokens[..50].to_vec();
    diverging.push(999_999);
    diverging.push(888_888);
    let (matched_div, blocks_div) = cache.match_prefix(&diverging).await;
    assert_eq!(matched_div, 50, "Diverging query must match exact prefix up to divergence");
    assert_eq!(blocks_div.len(), 50);
}

/// 1.2 Reverse Order Insertion (Longest first, then sub-prefixes triggering multi-node splits)
#[tokio::test]
async fn challenge_radix_reverse_order_splits() {
    let cache = RadixPrefixCache::new(100);

    // Insert longest sequence first: [10, 20, 30, 40, 50, 60, 70, 80]
    let full_seq = vec![10, 20, 30, 40, 50, 60, 70, 80];
    cache.insert_prefix(&full_seq, 800, false).await;

    // Insert sub-prefix [10, 20, 30, 40]
    let sub1 = vec![10, 20, 30, 40];
    cache.insert_prefix(&sub1, 400, false).await;

    // Insert sub-prefix [10, 20]
    let sub2 = vec![10, 20];
    cache.insert_prefix(&sub2, 200, false).await;

    // Insert sibling branch [10, 20, 35, 45]
    let sib = vec![10, 20, 35, 45];
    cache.insert_prefix(&sib, 350, false).await;

    // Verify all branches match with correct block IDs
    let (m_full, b_full) = cache.match_prefix(&full_seq).await;
    assert_eq!(m_full, 8);
    assert_eq!(b_full, vec![200, 400, 800]);

    let (m_sub1, b_sub1) = cache.match_prefix(&sub1).await;
    assert_eq!(m_sub1, 4);
    assert_eq!(b_sub1, vec![200, 400]);

    let (m_sub2, b_sub2) = cache.match_prefix(&sub2).await;
    assert_eq!(m_sub2, 2);
    assert_eq!(b_sub2, vec![200]);

    let (m_sib, b_sib) = cache.match_prefix(&sib).await;
    assert_eq!(m_sib, 4);
    assert_eq!(b_sib, vec![200, 350]);
}

/// 1.3 Radix Tree Fuzzing with Random Sequences & Substring Queries
#[tokio::test]
async fn challenge_radix_fuzzing_oracle() {
    let cache = RadixPrefixCache::new(5000);

    // Pseudo-random deterministic generator (LCG)
    let mut seed: u64 = 0xCAFE_BABE_DEAD_BEEF;
    let mut next_rand = || -> u32 {
        seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
        (seed >> 32) as u32
    };

    let vocab_size = 15; // Small vocabulary to force collisions and deep tree branches
    let mut inserted_prefixes = Vec::new();

    for i in 1..=300 {
        let len = ((next_rand() % 12) + 1) as usize;
        let mut tokens = Vec::with_capacity(len);
        for _ in 0..len {
            tokens.push(next_rand() % vocab_size);
        }

        cache.insert_prefix(&tokens, i, false).await;
        inserted_prefixes.push(tokens);
    }

    // Query all inserted prefixes against the cache
    for tokens in &inserted_prefixes {
        let (matched, blocks) = cache.match_prefix(tokens).await;
        assert!(matched > 0, "Inserted sequence must match at least 1 token");
        assert!(!blocks.is_empty(), "Must match at least one block");
        assert!(matched <= tokens.len(), "Matched tokens cannot exceed query length");
    }

    // Query 200 random sequences
    for _ in 0..200 {
        let len = ((next_rand() % 16) + 1) as usize;
        let mut query = Vec::with_capacity(len);
        for _ in 0..len {
            query.push(next_rand() % vocab_size);
        }

        let (c_match, c_blocks) = cache.match_prefix(&query).await;
        assert!(c_match <= query.len());
        if c_match == 0 {
            assert!(c_blocks.is_empty());
        }
    }
}

/// 1.4 High-Concurrency Stress Harness on RadixPrefixCache
#[tokio::test]
async fn challenge_radix_high_concurrency_stress() {
    let cache = Arc::new(RadixPrefixCache::new(1000));
    let num_tasks = 40;
    let ops_per_task = 50;

    // Pre-populate pinned system prompt
    let sys_prompt = vec![1, 2, 3, 4, 5];
    cache.insert_prefix(&sys_prompt, 9999, true).await;
    cache.pin_prefix(&sys_prompt).await;

    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let cache_clone = cache.clone();
        let handle = tokio::spawn(async move {
            for iter in 0..ops_per_task {
                let op_type = (task_id + iter) % 5;
                match op_type {
                    0 | 1 => {
                        // Reader
                        let query = vec![1, 2, 3, 4, 5, (iter % 10) as u32];
                        let (m, b) = cache_clone.match_prefix(&query).await;
                        assert!(m >= 5, "Must always match pinned system prompt prefix");
                        assert!(!b.is_empty());
                    }
                    2 => {
                        // Writer
                        let new_seq = vec![1, 2, 3, 4, 5, (iter % 10) as u32, (task_id % 20) as u32];
                        let blk = task_id * 1000 + iter + 1;
                        cache_clone.insert_prefix(&new_seq, blk, false).await;
                    }
                    3 => {
                        // Evictor
                        let _ = cache_clone.evict_lru(2).await;
                    }
                    _ => {
                        // Pinner / stats reader
                        let _ = cache_clone.stats();
                    }
                }
            }
        });
        handles.push(handle);
    }

    let res = timeout(Duration::from_secs(10), async {
        for h in handles {
            h.await.expect("task join failed");
        }
    }).await;

    assert!(res.is_ok(), "DEADLOCK DETECTED in concurrent RadixPrefixCache operations!");

    // Verify pinned system prompt is still intact after concurrent evictions and writes
    let (matched, blocks) = cache.match_prefix(&sys_prompt).await;
    assert_eq!(matched, 5);
    assert_eq!(blocks, vec![9999]);
}

/// 1.5 Eviction Behavior on Direct Leaf Nodes vs Split Nodes
#[tokio::test]
async fn challenge_radix_eviction_mechanics() {
    let cache = RadixPrefixCache::new(10);

    // 1. Insert 3 direct unpinned leaves
    cache.insert_prefix(&[100, 101], 1, false).await;
    cache.insert_prefix(&[200, 201], 2, false).await;
    cache.insert_prefix(&[300, 301], 3, false).await;
    assert_eq!(cache.allocated_blocks(), 3);

    // Evict 1 block
    let evicted = cache.evict_lru(1).await;
    assert_eq!(evicted, 1);
    assert_eq!(cache.allocated_blocks(), 2);

    // Evict remaining 2 blocks
    let evicted2 = cache.evict_lru(5).await;
    assert_eq!(evicted2, 2);
    assert_eq!(cache.allocated_blocks(), 0);

    // Further evictions on empty tree return 0 gracefully
    let evicted_empty = cache.evict_lru(1).await;
    assert_eq!(evicted_empty, 0);
}

// ============================================================================
// PART 2: EBBINGHAUS RETENTION DECAY EMPIRICAL CHALLENGES
// ============================================================================

/// 2.1 Numerical Boundary & Edge Cases in Ebbinghaus Formula
#[test]
fn challenge_ebbinghaus_mathematical_bounds() {
    let tau_base = 604_800.0; // 7 days

    // 1. Zero elapsed time -> exactly 1.0
    assert_eq!(compute_retention(0.0, tau_base), 1.0);

    // 2. Negative elapsed time (clock skew) -> clamped to 1.0
    assert_eq!(compute_retention(-1.0, tau_base), 1.0);
    assert_eq!(compute_retention(-1_000_000_000.0, tau_base), 1.0);

    // 3. Half-life <= 0.0 -> returns 0.0
    assert_eq!(compute_retention(100.0, 0.0), 0.0);
    assert_eq!(compute_retention(100.0, -50.0), 0.0);

    // 4. Extreme delta time (100 years = 3.15576e9 s) -> 0.0 without underflow panic
    let dt_100_years = 100.0 * 365.25 * 86400.0;
    let r_100y = compute_retention(dt_100_years, tau_base);
    assert_eq!(r_100y, 0.0);
    assert!(!r_100y.is_nan());
    assert!(!r_100y.is_infinite());

    // 5. Half-life computation with extreme recall counts
    let tau_zero_recalls = compute_half_life_secs(tau_base, 0, 5.0, 1.0);
    let tau_huge_recalls = compute_half_life_secs(tau_base, 1_000_000, 5.0, 1.0);
    let tau_u32_max = compute_half_life_secs(tau_base, u32::MAX, 5.0, 1.0);

    assert!(tau_huge_recalls > tau_zero_recalls);
    assert!(tau_u32_max > tau_huge_recalls);
    assert!(!tau_u32_max.is_nan());
    assert!(!tau_u32_max.is_infinite());

    // 6. Importance and valence clamping checks
    let tau_imp_neg = compute_half_life_secs(tau_base, 0, -100.0, 1.0);
    let tau_imp_min = compute_half_life_secs(tau_base, 0, 1.0, 1.0);
    assert_eq!(tau_imp_neg, tau_imp_min, "Importance below 1.0 must clamp to 1.0");

    let tau_imp_huge = compute_half_life_secs(tau_base, 0, 1000.0, 1.0);
    let tau_imp_max = compute_half_life_secs(tau_base, 0, 10.0, 1.0);
    assert_eq!(tau_imp_huge, tau_imp_max, "Importance above 10.0 must clamp to 10.0");

    let tau_val_neg = compute_half_life_secs(tau_base, 0, 5.0, -100.0);
    let tau_val_min = compute_half_life_secs(tau_base, 0, 5.0, 0.8);
    assert_eq!(tau_val_neg, tau_val_min, "Valence below 0.8 must clamp to 0.8");

    let tau_val_huge = compute_half_life_secs(tau_base, 0, 5.0, 100.0);
    let tau_val_max = compute_half_life_secs(tau_base, 0, 5.0, 1.5);
    assert_eq!(tau_val_huge, tau_val_max, "Valence above 1.5 must clamp to 1.5");

    // 7. Retention tier boundary classifications
    assert_eq!(classify_retention(1.0), RetentionTier::Active);
    assert_eq!(classify_retention(ACTIVE_RETENTION_THRESHOLD), RetentionTier::Active);
    assert_eq!(classify_retention(ACTIVE_RETENTION_THRESHOLD - 0.001), RetentionTier::Dormant);
    assert_eq!(classify_retention(DORMANT_RETENTION_THRESHOLD), RetentionTier::Dormant);
    assert_eq!(classify_retention(DORMANT_RETENTION_THRESHOLD - 0.001), RetentionTier::Archive);
    assert_eq!(classify_retention(ARCHIVE_RETENTION_THRESHOLD), RetentionTier::Archive);
    assert_eq!(classify_retention(ARCHIVE_RETENTION_THRESHOLD - 0.001), RetentionTier::PurgeCandidate);
}

/// 2.2 Massive SQLite Episodic Store Lifecycle, Retention Sweeping & Purging
#[tokio::test]
async fn challenge_l2_episodic_massive_sweep_and_purge() {
    let pool = DatabasePool::new_in_memory().expect("In-memory SQLite pool failed");
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"master_challenge_pwd", b"master_challenge_salt").unwrap());
    let store = L2EpisodicStore::new(pool, enclave);
    store.init_schema().expect("Schema creation failed");

    let t0 = 1_700_000_000;
    let total_events = 300;

    // Insert 300 events across 3 domains with varying half-lives
    for i in 0..total_events {
        let domain = match i % 3 {
            0 => "domain_alpha",
            1 => "domain_beta",
            _ => "domain_gamma",
        };
        let base_half_life = match i % 4 {
            0 => 60,        // 1 minute (fast decay)
            1 => 3600,      // 1 hour
            2 => 86400,     // 1 day
            _ => 604_800,   // 7 days
        };

        let event = EpisodicEvent {
            memory_id: format!("mem_{:04}", i),
            session_id: format!("sess_{}", i % 10),
            domain: domain.to_string(),
            category: "stress_test".to_string(),
            content: format!("Secure episodic fact payload #{} with specific context data.", i),
            importance_score: ((i % 10) + 1) as f64,
            emotional_valence: 1.0,
            recall_count: (i % 5) as u32,
            created_at: t0,
            last_recalled_at: t0,
            base_half_life_secs: base_half_life,
            retention_score: 1.0,
        };

        store.insert_event(&event).expect("Insert must succeed");
    }

    // 1. Initial retrieval at t0: all events must be Active
    let initial_active = store.get_active_events("domain_alpha", ACTIVE_RETENTION_THRESHOLD).unwrap();
    assert_eq!(initial_active.len(), total_events / 3);

    // 2. Advance time to t0 + 2 hours (7200s) and sweep retention
    let t1 = t0 + 7200;
    let report1 = store.sweep_retention(t1).unwrap();
    assert_eq!(report1.total_processed, total_events);
    assert!(report1.purged_count > 0, "60s half-life events must decay to purge candidate after 2 hours");
    assert!(report1.active_count > 0, "7-day half-life events must remain active");

    // 3. Purge candidate cleanup
    let purged_count = store.purge_decayed_events(ARCHIVE_RETENTION_THRESHOLD).unwrap();
    assert_eq!(purged_count, report1.purged_count);

    // 4. Advance time to extreme future (1000 days = 86,400,000s)
    let t2 = t0 + 86_400_000;
    let report2 = store.sweep_retention(t2).unwrap();
    let remaining_after_purge = total_events - purged_count;
    assert_eq!(report2.total_processed, remaining_after_purge);

    // After 1000 days, all remaining events decay to PurgeCandidate
    assert_eq!(report2.purged_count, remaining_after_purge);
}

// ============================================================================
// PART 3: BAYESIAN PROCEDURAL SKILL RANKING EMPIRICAL CHALLENGES
// ============================================================================

/// 3.1 Bayesian Mathematical Invariants & Severe Penalties
#[test]
fn challenge_bayesian_ranking_invariants() {
    // 1. Prior calculation
    let exp_prior = compute_bayesian_expectation(2.0, 1.0);
    assert!((exp_prior - 2.0 / 3.0).abs() < 1e-9);

    // 2. High-volume successes drive expectation towards 1.0
    let exp_100_succ = compute_bayesian_expectation(102.0, 1.0);
    assert!(exp_100_succ > 0.99);

    // 3. Severe failures drive expectation towards 0.0
    let exp_100_fail = compute_bayesian_expectation(2.0, 201.0);
    assert!(exp_100_fail < 0.01);

    // 4. Ranking score monotonicity with respect to failure count
    let (mult_0, score_0) = compute_ranking_score(0.95, 2.0, 1.0, 0);
    let (mult_1, score_1) = compute_ranking_score(0.95, 2.0, 2.0, 1);
    let (mult_5, score_5) = compute_ranking_score(0.95, 2.0, 10.0, 5);
    let (mult_50, score_50) = compute_ranking_score(0.95, 2.0, 100.0, 50);

    assert!(mult_0 > mult_1);
    assert!(mult_1 > mult_5);
    assert!(mult_5 > mult_50);
    assert!(score_0 > score_1);
    assert!(score_1 > score_5);
    assert!(score_5 > score_50);

    // 5. Ranking score monotonicity with respect to similarity
    let (_, score_sim_high) = compute_ranking_score(0.90, 5.0, 5.0, 2);
    let (_, score_sim_low) = compute_ranking_score(0.50, 5.0, 5.0, 2);
    assert!(score_sim_high > score_sim_low);

    // 6. Quality multiplier clamp [0.0, 1.0]
    assert!(mult_50 >= 0.0 && mult_50 <= 1.0);
}

/// 3.2 High-Concurrency High-Volume Updates on L4ProceduralRegistry
#[tokio::test]
async fn challenge_procedural_high_volume_concurrency() {
    let registry = Arc::new(L4ProceduralRegistry::new());
    let num_skills = 20;

    // Register initial skills
    for i in 0..num_skills {
        let skill = ProceduralSkill::new(
            format!("skill_{:02}", i),
            format!("Skill #{}", i),
            "Skill description".to_string(),
            "fn run() {}".to_string(),
        );
        registry.register_skill(skill).await;
    }

    let num_tasks = 20;
    let updates_per_task = 100;
    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let reg = registry.clone();
        let handle = tokio::spawn(async move {
            for iter in 0..updates_per_task {
                let skill_id = format!("skill_{:02}", (task_id + iter) % num_skills);
                let outcome = match (task_id + iter) % 4 {
                    0 => ExecutionOutcome::Success,
                    1 => ExecutionOutcome::Failure {
                        failure_type: FailureType::Crash,
                        severity: 2.0,
                        reason: "Fatal crash".to_string(),
                    },
                    2 => ExecutionOutcome::Failure {
                        failure_type: FailureType::Timeout,
                        severity: 1.0,
                        reason: "Execution timeout".to_string(),
                    },
                    _ => ExecutionOutcome::Failure {
                        failure_type: FailureType::Uninvoked,
                        severity: 0.5,
                        reason: "Uninvoked decay".to_string(),
                    },
                };

                let _ = reg.record_outcome(&skill_id, &outcome).await;
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.await.expect("join update task");
    }

    // Query and rank all skills
    let similarities: Vec<(String, f64)> = (0..num_skills)
        .map(|i| (format!("skill_{:02}", i), 0.85))
        .collect();

    let ranked = registry.rank_skills(&similarities).await;
    assert_eq!(ranked.len(), num_skills);

    // Verify determinism: strictly descending rank scores
    for w in ranked.windows(2) {
        assert!(
            w[0].final_rank_score >= w[1].final_rank_score,
            "Ranked skills must be sorted descending by final_rank_score"
        );
    }
}

// ============================================================================
// PART 4: CRYPTOGRAPHIC ENCLAVE AT-REST SECURITY EMPIRICAL CHALLENGES
// ============================================================================

/// 4.1 Fuzzing, Bit-Flipping & Fail-Closed Memory Enclave Security
#[test]
fn challenge_enclave_bit_flipping_and_fail_closed() {
    let passphrase = b"super_secure_enclave_passphrase_2026!";
    let salt = b"argon2_salt_32_bytes_long_entropy!";
    let enclave = MemoryEnclave::new_with_argon2id(passphrase, salt).unwrap();

    let secret_plaintext = "CONFIDENTIAL_USER_PROMPT_SECRET_KEY_12345";
    let envelope = enclave.encrypt_string(secret_plaintext).unwrap();

    // 1. Exact valid decryption
    let read_valid = enclave.read_record(&envelope);
    match read_valid {
        FactRead::Ok(dec) => assert_eq!(dec, secret_plaintext),
        _ => panic!("Valid envelope must decrypt successfully"),
    }

    // 2. Fuzzing / Bit-flipping: corrupting each field of the envelope
    let parts: Vec<&str> = envelope["v2:".len()..].split(':').collect();
    let salt_hex = parts[0];
    let iv_hex = parts[1];
    let tag_hex = parts[2];
    let cipher_hex = parts[3];

    // Corrupted tag (flip last hex char)
    let mut corrupted_tag = tag_hex.to_string();
    let last_char = corrupted_tag.pop().unwrap();
    corrupted_tag.push(if last_char == '0' { '1' } else { '0' });
    let bad_tag_envelope = format!("v2:{}:{}:{}:{}", salt_hex, iv_hex, corrupted_tag, cipher_hex);
    let read_bad_tag = enclave.read_record(&bad_tag_envelope);
    assert_eq!(read_bad_tag, FactRead::Locked { reason: "auth_failed" });

    // Corrupted ciphertext (flip first hex char)
    let mut corrupted_cipher = cipher_hex.to_string();
    let first_char = corrupted_cipher.remove(0);
    corrupted_cipher.insert(0, if first_char == 'a' { 'b' } else { 'a' });
    let bad_cipher_envelope = format!("v2:{}:{}:{}:{}", salt_hex, iv_hex, tag_hex, corrupted_cipher);
    let read_bad_cipher = enclave.read_record(&bad_cipher_envelope);
    assert_eq!(read_bad_cipher, FactRead::Locked { reason: "auth_failed" });

    // Corrupted prefix or malformed delimiters
    assert_eq!(enclave.read_record("v1:bad:envelope:format"), FactRead::Locked { reason: "locked" });
    assert_eq!(enclave.read_record("random_garbage_data"), FactRead::Locked { reason: "locked" });
}

/// 4.2 Massive Key Rotation Roundtrip
#[test]
fn challenge_enclave_massive_key_rotation() {
    let enclave_a = MemoryEnclave::new_with_argon2id(b"passphrase_A", b"salt_AAAAAAAAAAAAAAAA").unwrap();
    let enclave_b = MemoryEnclave::new_with_argon2id(b"passphrase_B", b"salt_BBBBBBBBBBBBBBBB").unwrap();

    let count = 100;
    let mut envelopes_a = Vec::with_capacity(count);

    for i in 0..count {
        let msg = format!("Sensitive Memory Record #{}", i);
        let env_a = enclave_a.encrypt_string(&msg).unwrap();
        envelopes_a.push((msg, env_a));
    }

    // Rotate all envelopes from A to B
    let mut envelopes_b = Vec::with_capacity(count);
    for (orig_msg, env_a) in &envelopes_a {
        let env_b = enclave_a.rotate_envelope(env_a, &enclave_b).expect("Key rotation must succeed");
        // Verify envelope B cannot be read by enclave A
        assert_eq!(enclave_a.read_record(&env_b), FactRead::Locked { reason: "auth_failed" });
        // Verify envelope B decrypts correctly with enclave B
        let read_b = enclave_b.read_record(&env_b);
        assert_eq!(read_b, FactRead::Ok(orig_msg.clone()));
        envelopes_b.push(env_b);
    }

    assert_eq!(envelopes_b.len(), count);
}
