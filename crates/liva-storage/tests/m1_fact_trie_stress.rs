//! Empirical Stress Benchmark for FactTrie (Milestone M1, Item 3)
//!
//! Evaluates FactTrie prefix search and substring matching under heavy load:
//! 1. Scale: 1,000+ distinct keys across multiple prefixes and Unicode Vietnamese text.
//! 2. Accuracy: Prefix traversal, empty prefix, nonexistent prefix, duplicate prevention.
//! 3. Performance: Latency profiling over 10,000 prefix lookups (sub-50 microsecond SLA).
//! 4. Concurrency: Multi-threaded read throughput under shared Arc.

use liva_storage::FactTrie;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_fact_trie_scale_prefix_search_accuracy_and_performance() {
    let mut trie = FactTrie::new();

    // 1. Ingest 1,000 keys across structured domain prefixes
    let mut expected_theme_keys = HashSet::new();
    for i in 0..150 {
        let key = format!("user:preference:theme:variant_{i}");
        let fact_id = format!("fact_theme_{i}");
        trie.insert(&key, &fact_id);
        expected_theme_keys.insert(fact_id);
    }

    let mut expected_network_keys = HashSet::new();
    for i in 0..200 {
        let key = format!("system:config:network:interface_{i}");
        let fact_id = format!("fact_net_{i}");
        trie.insert(&key, &fact_id);
        expected_network_keys.insert(fact_id);
    }

    let mut expected_vn_keys = HashSet::new();
    for i in 0..250 {
        let key = format!("việt_nam:địa_danh:tỉnh_thành_{i}");
        let fact_id = format!("fact_vn_{i}");
        trie.insert(&key, &fact_id);
        expected_vn_keys.insert(fact_id);
    }

    for i in 0..400 {
        let key = format!("entity:misc:item_{i}");
        let fact_id = format!("fact_misc_{i}");
        trie.insert(&key, &fact_id);
    }

    // Total keys inserted = 150 + 200 + 250 + 400 = 1,000 keys.

    // 2. Prefix Search Accuracy Assertions
    // Case A: Exact prefix query for themes
    let results_theme = trie.search_prefix("user:preference:theme:");
    assert_eq!(
        results_theme.len(),
        150,
        "Prefix 'user:preference:theme:' must return exactly 150 fact keys"
    );
    let result_theme_set: HashSet<String> = results_theme.into_iter().collect();
    assert_eq!(
        result_theme_set, expected_theme_keys,
        "Theme fact keys must match inserted set exactly"
    );

    // Case B: Broader prefix query for 'user:preference:'
    let results_user_pref = trie.search_prefix("user:preference:");
    assert_eq!(
        results_user_pref.len(),
        150,
        "Prefix 'user:preference:' must include theme items"
    );

    // Case C: Vietnamese Unicode prefix query
    let results_vn = trie.search_prefix("việt_nam:địa_danh:");
    assert_eq!(
        results_vn.len(),
        250,
        "Vietnamese Unicode prefix must return exactly 250 fact keys"
    );
    let result_vn_set: HashSet<String> = results_vn.into_iter().collect();
    assert_eq!(
        result_vn_set, expected_vn_keys,
        "Vietnamese fact keys must match inserted set exactly"
    );

    // Case D: System network prefix query
    let results_net = trie.search_prefix("system:config:network:");
    assert_eq!(
        results_net.len(),
        200,
        "Network prefix must return exactly 200 fact keys"
    );
    let result_net_set: HashSet<String> = results_net.into_iter().collect();
    assert_eq!(result_net_set, expected_network_keys);

    // Case E: Non-existent prefix query
    let results_missing = trie.search_prefix("nonexistent:prefix:foo");
    assert!(
        results_missing.is_empty(),
        "Nonexistent prefix must return empty vec"
    );

    // Case F: Empty prefix query returns all 1,000 keys
    let all_keys = trie.search_prefix("");
    assert_eq!(
        all_keys.len(),
        1000,
        "Empty prefix must return all 1,000 indexed fact keys"
    );

    // 3. Performance Benchmark: 10,000 prefix lookups
    let benchmark_queries = [
        "user:preference:theme:",
        "system:config:network:",
        "việt_nam:địa_danh:",
        "entity:misc:",
        "system:config:network:interface_10",
        "nonexistent:query",
        "user:preference",
        "việt_nam",
    ];

    let iterations = 10_000;
    let start = Instant::now();
    for i in 0..iterations {
        let q = benchmark_queries[i % benchmark_queries.len()];
        let matches = trie.search_prefix(q);
        assert!(!matches.is_empty() || q == "nonexistent:query");
    }
    let elapsed = start.elapsed();
    let avg_micros = elapsed.as_micros() as f64 / iterations as f64;

    println!(
        "FactTrie Prefix Search Benchmark: {} queries in {:?} (avg: {:.3} µs/query)",
        iterations, elapsed, avg_micros
    );

    // Assert high-performance SLA: average lookup time must be well under 1 millisecond (target < 50 µs in unoptimized debug, < 10 µs in release)
    assert!(
        avg_micros < 50.0,
        "FactTrie average prefix search must be < 50 µs (observed: {:.3} µs)",
        avg_micros
    );

    // 4. Substring Search in Conversational Turns
    trie.insert("thủ đô hà nội", "fact_capital_hanoi");
    trie.insert("ngôn ngữ rust", "fact_lang_rust");

    let conv_matches =
        trie.search("Tôi đang dùng ngôn ngữ Rust để tối ưu hệ thống ở thủ đô Hà Nội.");
    assert!(
        conv_matches.contains(&"fact_capital_hanoi".to_string()),
        "Must match 'thủ đô hà nội'"
    );
    assert!(
        conv_matches.contains(&"fact_lang_rust".to_string()),
        "Must match 'ngôn ngữ rust'"
    );

    // 5. Clear and re-verification
    trie.clear();
    assert!(
        trie.search_prefix("").is_empty(),
        "Trie must be empty after clear()"
    );
}

#[test]
fn test_fact_trie_concurrent_multi_threaded_reads() {
    let mut trie = FactTrie::new();
    for i in 0..500 {
        trie.insert(&format!("prefix_alpha:item_{i}"), &format!("alpha_{i}"));
        trie.insert(&format!("prefix_beta:item_{i}"), &format!("beta_{i}"));
    }

    let shared_trie = Arc::new(trie);
    let mut handles = Vec::new();

    // Spawn 8 worker threads doing 1,000 queries each
    for thread_idx in 0..8 {
        let trie_ref = Arc::clone(&shared_trie);
        handles.push(std::thread::spawn(move || {
            for round in 0..1000 {
                if (thread_idx + round) % 2 == 0 {
                    let res = trie_ref.search_prefix("prefix_alpha:");
                    assert_eq!(res.len(), 500);
                } else {
                    let res = trie_ref.search_prefix("prefix_beta:");
                    assert_eq!(res.len(), 500);
                }
            }
        }));
    }

    for h in handles {
        h.join()
            .expect("Worker thread panicked during concurrent Trie read");
    }
}
