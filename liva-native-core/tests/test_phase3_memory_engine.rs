use std::sync::Arc;
use liva_native_core::db::DatabasePool;
use liva_native_core::memory::{
    classify_retention, compute_bayesian_expectation, compute_half_life_secs,
    compute_ranking_score, compute_retention, EpisodicEvent, ExecutionOutcome, FailureType,
    MemoryEnclave, ProceduralSkill, RadixPrefixCache, RetentionTier, VirtualMemoryEngine,
    ACTIVE_RETENTION_THRESHOLD, ARCHIVE_RETENTION_THRESHOLD, DORMANT_RETENTION_THRESHOLD,
};

#[tokio::test]
async fn test_l1_radix_prefix_cache_matching_and_reuse() {
    let cache = RadixPrefixCache::new(100);

    // 1. Insert System Prompt prefix (Tokens: 100, 101, 102, 103)
    let system_tokens = vec![100, 101, 102, 103];
    cache.insert_prefix(&system_tokens, 1, true).await;
    assert!(cache.pin_prefix(&system_tokens).await);

    // Match exact system tokens
    let (matched, blocks) = cache.match_prefix(&system_tokens).await;
    assert_eq!(matched, 4);
    assert_eq!(blocks, vec![1]);

    // 2. Insert Turn 1 tokens (Prefix: System tokens + 200, 201)
    let mut turn1_tokens = system_tokens.clone();
    turn1_tokens.extend_from_slice(&[200, 201]);
    cache.insert_prefix(&turn1_tokens, 2, false).await;

    // Match Turn 1 (reusing system block 1 + turn 1 block 2)
    let (matched1, blocks1) = cache.match_prefix(&turn1_tokens).await;
    assert_eq!(matched1, 6);
    assert_eq!(blocks1, vec![1, 2]);

    // 3. Insert Turn 2 branch diverging at turn tokens (System tokens + 300, 301)
    let mut turn2_tokens = system_tokens.clone();
    turn2_tokens.extend_from_slice(&[300, 301]);
    cache.insert_prefix(&turn2_tokens, 3, false).await;

    // Query Turn 2: should match system prefix (block 1) and Turn 2 leaf (block 3)
    let (matched2, blocks2) = cache.match_prefix(&turn2_tokens).await;
    assert_eq!(matched2, 6);
    assert_eq!(blocks2, vec![1, 3]);

    // Query non-existing sequence: should match only the common system prefix (block 1)
    let mut unrelated = system_tokens.clone();
    unrelated.extend_from_slice(&[999, 998]);
    let (matched_sys_only, blocks_sys_only) = cache.match_prefix(&unrelated).await;
    assert_eq!(matched_sys_only, 4);
    assert_eq!(blocks_sys_only, vec![1]);

    let stats = cache.stats();
    assert!(stats.hits >= 4);
    assert!(stats.hit_ratio > 0.0);
}

#[tokio::test]
async fn test_l1_radix_cache_eviction() {
    let cache = RadixPrefixCache::new(10);

    let pinned_seq = vec![1, 2, 3];
    cache.insert_prefix(&pinned_seq, 10, true).await;
    cache.pin_prefix(&pinned_seq).await;

    let unpinned1 = vec![10, 11, 12];
    let unpinned2 = vec![20, 21, 22];
    cache.insert_prefix(&unpinned1, 11, false).await;
    cache.insert_prefix(&unpinned2, 12, false).await;

    assert_eq!(cache.allocated_blocks(), 3);

    // Evict 1 unpinned leaf
    let evicted = cache.evict_lru(1).await;
    assert_eq!(evicted, 1);
    assert_eq!(cache.allocated_blocks(), 2);

    // Pinned prefix should remain intact
    let (matched, _) = cache.match_prefix(&pinned_seq).await;
    assert_eq!(matched, 3);
}

#[test]
fn test_l2_ebbinghaus_retention_mathematics() {
    let base_half_life = 604_800.0; // 7 days in seconds

    // Dynamic half-life growth with practice (power law of practice)
    let tau_0 = compute_half_life_secs(base_half_life, 0, 5.0, 1.0);
    let tau_3 = compute_half_life_secs(base_half_life, 3, 5.0, 1.0);
    let tau_10 = compute_half_life_secs(base_half_life, 10, 5.0, 1.0);
    assert!(tau_3 > tau_0, "Half-life must increase with recall reinforcements");
    assert!(tau_10 > tau_3, "Half-life must increase monotonically with practice");

    // Dynamic half-life scaling with semantic importance
    let tau_low_imp = compute_half_life_secs(base_half_life, 0, 1.0, 1.0);
    let tau_high_imp = compute_half_life_secs(base_half_life, 0, 10.0, 1.0);
    assert!(tau_high_imp > tau_low_imp, "High importance events must have longer retention");

    // Exponential decay curve: R(m, t) = 2^(- \Delta t / \tau)
    let tau = 1000.0;
    let r_0 = compute_retention(0.0, tau);
    assert!((r_0 - 1.0).abs() < 1e-6, "Retention at delta=0 must be 1.0");

    let r_tau = compute_retention(tau, tau);
    assert!((r_tau - 0.5).abs() < 1e-6, "Retention at delta=tau must be 0.5");

    let r_2tau = compute_retention(2.0 * tau, tau);
    assert!((r_2tau - 0.25).abs() < 1e-6, "Retention at delta=2*tau must be 0.25");

    // Tier classification boundaries
    assert_eq!(classify_retention(0.85), RetentionTier::Active);
    assert_eq!(classify_retention(ACTIVE_RETENTION_THRESHOLD), RetentionTier::Active);
    assert_eq!(classify_retention(0.20), RetentionTier::Dormant);
    assert_eq!(classify_retention(DORMANT_RETENTION_THRESHOLD), RetentionTier::Dormant);
    assert_eq!(classify_retention(0.05), RetentionTier::Archive);
    assert_eq!(classify_retention(ARCHIVE_RETENTION_THRESHOLD), RetentionTier::Archive);
    assert_eq!(classify_retention(0.01), RetentionTier::PurgeCandidate);
}

#[tokio::test]
async fn test_l2_episodic_store_lifecycle_and_sweep() {
    let pool = DatabasePool::new_in_memory().expect("In-memory SQLite init failed");
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"test_pwd", b"test_salt_123456").unwrap());
    let store = liva_native_core::memory::L2EpisodicStore::new(pool, enclave);
    store.init_schema().expect("Schema initialization failed");

    let t0 = 1_700_000_000;
    let event1 = EpisodicEvent {
        memory_id: "ep_001".to_string(),
        session_id: "sess_1".to_string(),
        domain: "user_preferences".to_string(),
        category: "coding_style".to_string(),
        content: "User prefers async/await and strict typing in Rust.".to_string(),
        importance_score: 8.0,
        emotional_valence: 1.2,
        recall_count: 0,
        created_at: t0,
        last_recalled_at: t0,
        base_half_life_secs: 1000,
        retention_score: 1.0,
    };

    let event2 = EpisodicEvent {
        memory_id: "ep_002".to_string(),
        session_id: "sess_1".to_string(),
        domain: "user_preferences".to_string(),
        category: "weather".to_string(),
        content: "Temporary weather check in Hanoi.".to_string(),
        importance_score: 2.0,
        emotional_valence: 0.9,
        recall_count: 0,
        created_at: t0,
        last_recalled_at: t0,
        base_half_life_secs: 100,
        retention_score: 1.0,
    };

    store.insert_event(&event1).unwrap();
    store.insert_event(&event2).unwrap();

    // Verify insertion and retrieval with encrypted content
    let retrieved1 = store.get_event_by_id("ep_001").unwrap().expect("Event not found");
    assert_eq!(retrieved1.content, event1.content);

    // Advance time by 300 seconds and sweep retention
    let t1 = t0 + 300;
    let sweep_report = store.sweep_retention(t1).unwrap();
    assert_eq!(sweep_report.total_processed, 2);

    // Event 2 (tau ~ 100s) has experienced 3 half-lives -> retention ~ 0.125 (Dormant)
    let updated2 = store.get_event_by_id("ep_002").unwrap().unwrap();
    assert!(updated2.retention_score < 0.35);

    // Record recall for event 2 to reinforce it
    let refreshed_score = store.record_recall("ep_002", t1).unwrap();
    assert_eq!(refreshed_score, 1.0);
    let recalled2 = store.get_event_by_id("ep_002").unwrap().unwrap();
    assert_eq!(recalled2.recall_count, 1);
    assert_eq!(recalled2.retention_score, 1.0);

    // Advance time far into future (100,000s) and verify purge candidate
    let t2 = t1 + 100_000;
    let sweep2 = store.sweep_retention(t2).unwrap();
    assert!(sweep2.purged_count >= 1);

    let purged = store.purge_decayed_events(0.02).unwrap();
    assert!(purged >= 1);
}

#[tokio::test]
async fn test_l4_procedural_bayesian_weighting() {
    let registry = liva_native_core::memory::L4ProceduralRegistry::new();

    let mut skill = ProceduralSkill::new(
        "web_search_v1".to_string(),
        "Web Search".to_string(),
        "Search information on the web".to_string(),
        "fn execute_search() { ... }".to_string(),
    );
    skill.tags = vec!["search".to_string(), "network".to_string()];
    registry.register_skill(skill).await;

    // Prior expectation
    let s0 = registry.get_skill("web_search_v1").await.unwrap();
    let expected_prior = compute_bayesian_expectation(s0.alpha, s0.beta);
    assert!((expected_prior - 2.0 / 3.0).abs() < 1e-6);

    // 1. Record successes
    registry.record_outcome("web_search_v1", &ExecutionOutcome::Success).await.unwrap();
    registry.record_outcome("web_search_v1", &ExecutionOutcome::Success).await.unwrap();

    let s1 = registry.get_skill("web_search_v1").await.unwrap();
    assert_eq!(s1.success_count, 2);
    assert_eq!(s1.alpha, 4.0);
    assert!(s1.expected_success_rate() > expected_prior);

    // 2. Record failure with severity
    registry.record_outcome(
        "web_search_v1",
        &ExecutionOutcome::Failure {
            failure_type: FailureType::Crash,
            severity: 1.5,
            reason: "Process segfault".to_string(),
        },
    ).await.unwrap();

    let s2 = registry.get_skill("web_search_v1").await.unwrap();
    assert_eq!(s2.failure_count, 1);
    assert_eq!(s2.failure_tallies.get("crash"), Some(&1));

    // 3. Skill ranking with similarity
    let skill2 = ProceduralSkill::new(
        "web_search_v2".to_string(),
        "Web Search v2".to_string(),
        "Upgraded search engine".to_string(),
        "fn execute_search_v2() { ... }".to_string(),
    );
    registry.register_skill(skill2).await;
    // Boost skill2 with 5 successes
    for _ in 0..5 {
        registry.record_outcome("web_search_v2", &ExecutionOutcome::Success).await.unwrap();
    }

    // Direct math verification of compute_ranking_score
    let (mult, score) = compute_ranking_score(0.90, 7.0, 1.0, 0);
    assert!(mult > 0.90);
    assert!(score > 0.80);

    let similarities = vec![
        ("web_search_v1".to_string(), 0.90),
        ("web_search_v2".to_string(), 0.90),
    ];

    let ranked = registry.rank_skills(&similarities).await;
    assert_eq!(ranked.len(), 2);
    // Skill 2 with higher success rate ranks ahead of skill 1 at equal similarity
    assert_eq!(ranked[0].skill.skill_id, "web_search_v2");
    assert_eq!(ranked[1].skill.skill_id, "web_search_v1");
}

#[tokio::test]
async fn test_virtual_memory_engine_unified_facade() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let enclave = Arc::new(MemoryEnclave::new_with_argon2id(b"facade_pass", b"salt_9999999999").unwrap());
    let engine = VirtualMemoryEngine::new(pool, enclave, 500);

    engine.l2().init_schema().expect("Schema init failed");

    // 1. L1 Working Memory through facade
    let tokens = vec![10, 20, 30, 40];
    engine.insert_working_prefix(&tokens, 100, true).await;
    let (matched, blocks) = engine.match_working_prefix(&tokens).await;
    assert_eq!(matched, 4);
    assert_eq!(blocks, vec![100]);

    // 2. L2 Episodic Memory through facade
    let event = EpisodicEvent {
        memory_id: "event_facade_1".to_string(),
        session_id: "session_001".to_string(),
        domain: "conversation".to_string(),
        category: "dialogue".to_string(),
        content: "Encrypted conversation event content".to_string(),
        importance_score: 7.0,
        emotional_valence: 1.0,
        recall_count: 0,
        created_at: 1000,
        last_recalled_at: 1000,
        base_half_life_secs: 10000,
        retention_score: 1.0,
    };
    engine.record_episodic_event(&event).unwrap();
    let active = engine.recall_episodic_context("conversation", 0.5).unwrap();
    assert_eq!(active.len(), 1);
    assert_eq!(active[0].content, event.content);

    // 3. L4 Procedural Memory through facade
    let skill = ProceduralSkill::new(
        "calculator_tool".to_string(),
        "Calculator".to_string(),
        "Evaluate math expressions".to_string(),
        "fn eval(expr: &str) -> f64".to_string(),
    );
    engine.l4().register_skill(skill).await;
    engine.record_skill_outcome("calculator_tool", &ExecutionOutcome::Success).await.unwrap();

    let ranked = engine.rank_procedural_skills(&[("calculator_tool".to_string(), 0.95)]).await;
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].skill.skill_id, "calculator_tool");
}
