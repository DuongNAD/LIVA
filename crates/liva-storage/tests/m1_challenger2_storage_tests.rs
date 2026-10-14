//! Empirical Challenger M1.2 Test Harness: ActiveRecallManager & register_sqlite_vec
//!
//! Objectives:
//! 1. ActiveRecallManager: Empty strings, pure whitespace, punctuation, uppercase/lowercase,
//!    Vietnamese Unicode diacritics ("Hà Nội", "Đà Nẵng", "TP. Hồ Chí Minh").
//!    Guarantee empty answers never falsely match and valid answers match reliably.
//! 2. register_sqlite_vec: Concurrency stress across multiple threads with OnceLock;
//!    ensure no double initialization, data races, or memory leaks occur.

use liva_storage::{
    ActiveRecallManager, FactChallenge, register_connection_sqlite_vec, register_sqlite_vec,
};
use rusqlite::Connection;
use std::sync::Arc;
use std::thread;

/// 1.1 Empty strings, pure whitespace, and punctuation edge cases for ActiveRecallManager
#[test]
fn test_active_recall_empty_and_whitespace_immunity() {
    let mut arm = ActiveRecallManager::new();

    // A. Register challenge with empty expected answer -> should be rejected on trigger
    arm.register_challenge("test_empty", "fact_empty", None, "");
    assert!(
        arm.try_intercept_turn("test_empty", "sess_empty").is_none(),
        "Empty expected answer must never issue a challenge"
    );

    // B. Register challenge with pure whitespace expected answer -> should be rejected
    arm.register_challenge("test_ws", "fact_ws", None, "   \t\n\r  ");
    assert!(
        arm.try_intercept_turn("test_ws", "sess_ws").is_none(),
        "Whitespace expected answer must never issue a challenge"
    );

    // C. Valid challenge, but user responds with empty string or pure whitespace
    arm.register_challenge(
        "thủ đô",
        "fact_capital",
        Some("Thủ đô của Việt Nam là gì?".to_string()),
        "Hà Nội",
    );

    // Trigger challenge
    let q = arm.try_intercept_turn("Cho tôi hỏi về thủ đô", "sess_user_empty");
    assert_eq!(q.as_deref(), Some("Thủ đô của Việt Nam là gì?"));

    // User response: empty string
    let rep_empty = arm.try_intercept_turn("", "sess_user_empty");
    assert_eq!(
        rep_empty.as_deref(),
        Some("Chưa chính xác. Đáp án đúng là: Hà Nội."),
        "Empty user input must evaluate to incorrect, never falsely match"
    );

    // Re-trigger and test whitespace response
    let _ = arm.try_intercept_turn("thủ đô", "sess_user_ws");
    let rep_ws = arm.try_intercept_turn("    \t\n   ", "sess_user_ws");
    assert_eq!(
        rep_ws.as_deref(),
        Some("Chưa chính xác. Đáp án đúng là: Hà Nội."),
        "Whitespace user input must evaluate to incorrect, never falsely match"
    );

    // Re-trigger and test pure punctuation response
    let _ = arm.try_intercept_turn("thủ đô", "sess_user_punct");
    let rep_punct = arm.try_intercept_turn("!?. , : ; - --", "sess_user_punct");
    assert_eq!(
        rep_punct.as_deref(),
        Some("Chưa chính xác. Đáp án đúng là: Hà Nội."),
        "Pure punctuation user input must evaluate to incorrect"
    );
}

/// 1.2 Vietnamese Unicode diacritics, casing, and word-token matching in ActiveRecallManager
#[test]
fn test_active_recall_vietnamese_unicode_and_casing_reliability() {
    let mut arm = ActiveRecallManager::new();

    // Register 3 authentic Vietnamese challenges
    arm.register_challenge(
        "hà nội",
        "fact_hanoi",
        Some("Thủ đô Việt Nam là gì?".to_string()),
        "Hà Nội",
    );
    arm.register_challenge(
        "đà nẵng",
        "fact_danang",
        Some("Thành phố cầu Rồng là đâu?".to_string()),
        "Đà Nẵng",
    );
    arm.register_challenge(
        "hồ chí minh",
        "fact_hcm",
        Some("Đô thị lớn nhất miền Nam là gì?".to_string()),
        "TP. Hồ Chí Minh",
    );

    // --- Challenge 1: "Hà Nội" ---
    // Test: all lowercase
    let _ = arm.try_intercept_turn("hà nội xinh đẹp", "s_hanoi_1");
    let r1 = arm.try_intercept_turn("hà nội", "s_hanoi_1");
    assert_eq!(r1.as_deref(), Some("Chính xác! Đáp án là: Hà Nội."));

    // Test: all UPPERCASE with Vietnamese diacritics
    let _ = arm.try_intercept_turn("Hà Nội mùa thu", "s_hanoi_2");
    let r2 = arm.try_intercept_turn("HÀ NỘI", "s_hanoi_2");
    assert_eq!(r2.as_deref(), Some("Chính xác! Đáp án là: Hà Nội."));

    // Test: mixed case with extra whitespace and sentence context
    let _ = arm.try_intercept_turn("Tôi yêu hà nội", "s_hanoi_3");
    let r3 = arm.try_intercept_turn("Chắc chắn là   hÀ    NộI   rồi!", "s_hanoi_3");
    assert_eq!(r3.as_deref(), Some("Chính xác! Đáp án là: Hà Nội."));

    // Test: Incorrect diacritics should NOT match ("Ha Noi" != "Hà Nội")
    let _ = arm.try_intercept_turn("hà nội", "s_hanoi_4");
    let r4 = arm.try_intercept_turn("Ha Noi", "s_hanoi_4");
    assert_eq!(
        r4.as_deref(),
        Some("Chưa chính xác. Đáp án đúng là: Hà Nội."),
        "Missing diacritics must not falsely match precomposed Vietnamese letters"
    );

    // --- Challenge 2: "Đà Nẵng" ---
    // Test: Lowercase 'đà nẵng'
    let _ = arm.try_intercept_turn("đà nẵng thành phố biển", "s_dn_1");
    let r_dn1 = arm.try_intercept_turn("đà nẵng", "s_dn_1");
    assert_eq!(r_dn1.as_deref(), Some("Chính xác! Đáp án là: Đà Nẵng."));

    // Test: Uppercase 'ĐÀ NẴNG' (including capital Đ and Ẵ)
    let _ = arm.try_intercept_turn("đà nẵng", "s_dn_2");
    let r_dn2 = arm.try_intercept_turn("ĐÀ NẴNG", "s_dn_2");
    assert_eq!(r_dn2.as_deref(), Some("Chính xác! Đáp án là: Đà Nẵng."));

    // Test: Mismatched city with same prefix ("Đà Lạt")
    let _ = arm.try_intercept_turn("đà nẵng", "s_dn_3");
    let r_dn3 = arm.try_intercept_turn("Thành phố Đà Lạt sương mù", "s_dn_3");
    assert_eq!(
        r_dn3.as_deref(),
        Some("Chưa chính xác. Đáp án đúng là: Đà Nẵng."),
        "Partial prefix overlap ('Đà') must not falsely match different target"
    );

    // --- Challenge 3: "TP. Hồ Chí Minh" ---
    // Test: Lowercase with period
    let _ = arm.try_intercept_turn("hồ chí minh", "s_hcm_1");
    let r_hcm1 = arm.try_intercept_turn("tp. hồ chí minh", "s_hcm_1");
    assert_eq!(
        r_hcm1.as_deref(),
        Some("Chính xác! Đáp án là: TP. Hồ Chí Minh.")
    );

    // Test: Full uppercase
    let _ = arm.try_intercept_turn("hồ chí minh", "s_hcm_2");
    let r_hcm2 = arm.try_intercept_turn("TP. HỒ CHÍ MINH", "s_hcm_2");
    assert_eq!(
        r_hcm2.as_deref(),
        Some("Chính xác! Đáp án là: TP. Hồ Chí Minh.")
    );

    // Test: Conversational phrasing containing target
    let _ = arm.try_intercept_turn("hồ chí minh", "s_hcm_3");
    let r_hcm3 = arm.try_intercept_turn("Mình nghĩ là TP. Hồ Chí Minh nhé!", "s_hcm_3");
    assert_eq!(
        r_hcm3.as_deref(),
        Some("Chính xác! Đáp án là: TP. Hồ Chí Minh.")
    );
}

/// 1.3 FactChallenge struct contract and Trie refresh operations
#[test]
fn test_active_recall_manager_trie_refresh_and_fact_challenge() {
    let mut arm = ActiveRecallManager::default();

    let facts = vec![
        ("thủ đô hà nội", "fact_hn"),
        ("thành phố huế", "fact_hue"),
        ("vịnh hạ long", "fact_hl"),
    ];
    arm.refresh_trie(facts);

    let found_hn = arm.trie().search("Đi du lịch thủ đô Hà Nội");
    assert_eq!(found_hn, vec!["fact_hn".to_string()]);

    let found_hue = arm.trie().search("Ghé thăm thành phố Huế mộng mơ");
    assert_eq!(found_hue, vec!["fact_hue".to_string()]);

    let found_hl = arm.trie().search("Vịnh Hạ Long rất hùng vĩ");
    assert_eq!(found_hl, vec!["fact_hl".to_string()]);

    // Verify FactChallenge equality and debug formatting
    let fc1 = FactChallenge {
        fact_key: "k1".to_string(),
        question: Some("q1".to_string()),
        expected_answer: "a1".to_string(),
    };
    let fc2 = fc1.clone();
    assert_eq!(fc1, fc2);
    assert!(format!("{fc1:?}").contains("FactChallenge"));
}

/// 2. Concurrency stress test for register_sqlite_vec across multiple threads
#[test]
fn test_register_sqlite_vec_concurrent_multithreaded_stress() {
    const NUM_THREADS: usize = 16;
    const ITERATIONS_PER_THREAD: usize = 10;

    let barrier = Arc::new(std::sync::Barrier::new(NUM_THREADS));
    let mut handles = Vec::with_capacity(NUM_THREADS);

    for thread_id in 0..NUM_THREADS {
        let b = Arc::clone(&barrier);
        handles.push(thread::spawn(move || {
            // Synchronize start to maximize contention on OnceLock
            b.wait();

            for iter in 0..ITERATIONS_PER_THREAD {
                // 1. Concurrent register_sqlite_vec invocation
                let reg_res = register_sqlite_vec();
                assert!(
                    reg_res.is_ok(),
                    "Thread {thread_id} iter {iter}: register_sqlite_vec failed: {:?}",
                    reg_res.err()
                );

                // 2. Open new in-memory connection; auto_extension should initialize sqlite-vec
                let conn = Connection::open_in_memory().unwrap_or_else(|e| {
                    panic!("Thread {thread_id} iter {iter}: open_in_memory: {e}")
                });

                // 3. Verify vec_version() functions properly
                let version: String = conn
                    .query_row("SELECT vec_version()", [], |r| r.get(0))
                    .unwrap_or_else(|e| panic!("Thread {thread_id} iter {iter}: vec_version: {e}"));
                assert!(
                    version.starts_with('v'),
                    "Thread {thread_id} iter {iter}: Invalid version: {version}"
                );

                // 4. Create virtual table and perform vector operation
                let table_name = format!("vec_t_{thread_id}_{iter}");
                conn.execute(
                    &format!("CREATE VIRTUAL TABLE {table_name} USING vec0(emb float[4])"),
                    [],
                )
                .unwrap_or_else(|e| panic!("Thread {thread_id} iter {iter}: create vtab: {e}"));

                // Insert dummy 4-dimensional vector
                let dummy_floats = [0.1f32, 0.2, 0.3, 0.4];
                let blob = bytemuck::cast_slice::<f32, u8>(&dummy_floats);
                conn.execute(
                    &format!("INSERT INTO {table_name}(rowid, emb) VALUES (1, ?)"),
                    [blob],
                )
                .unwrap_or_else(|e| panic!("Thread {thread_id} iter {iter}: insert: {e}"));

                let (matched_id, dist): (i64, f32) = conn
                    .query_row(
                        &format!(
                            "SELECT rowid, distance FROM {table_name} WHERE emb MATCH ? AND k = 1"
                        ),
                        [blob],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .unwrap_or_else(|e| panic!("Thread {thread_id} iter {iter}: query: {e}"));

                assert_eq!(matched_id, 1);
                assert!(
                    dist < 0.001,
                    "Exact match distance should be ~0.0, got: {dist}"
                );
            }
        }));
    }

    for (i, h) in handles.into_iter().enumerate() {
        h.join()
            .unwrap_or_else(|_| panic!("Thread {i} panicked during sqlite-vec concurrency stress"));
    }
}

/// 2.2 Verify explicit connection registration under repetition
#[test]
fn test_register_connection_sqlite_vec_repetition() {
    for _ in 0..10 {
        let conn = Connection::open_in_memory().expect("open in memory");
        let res = register_connection_sqlite_vec(&conn);
        assert!(res.is_ok(), "explicit registration must succeed");
        let ver: String = conn
            .query_row("SELECT vec_version()", [], |r| r.get(0))
            .expect("query version");
        assert!(ver.starts_with('v'));
    }
}
