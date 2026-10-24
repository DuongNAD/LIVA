//! Milestone 1 Adversarial Challenge: FactTrie Bounded Scan & Complexity Test
//!
//! Empirically challenges and stress-tests:
//! 1. Linear O(L) scaling vs quadratic O(L^2) blowup for large inputs (> 10,000 to 100,000 characters).
//! 2. Pathological inputs: high repetition of prefixes (e.g., thousands of 'a's against deep 'a' trie branch).
//! 3. Search depth capping boundary at MAX_MATCH_DEPTH = 64 characters (depth 63, 64, 65).
//! 4. Multibyte UTF-8 and Vietnamese diacritic strings of massive length without panic or memory leaks.

use liva_storage::FactTrie;
use std::time::Instant;

#[test]
fn test_fact_trie_large_input_bounded_scan_no_quadratic_blowup() {
    let mut trie = FactTrie::new();

    // Populate trie with pathological overlapping prefixes
    trie.insert("a", "fact_a1");
    trie.insert("aa", "fact_a2");
    trie.insert("aaa", "fact_a3");
    trie.insert("aaaa", "fact_a4");
    trie.insert(&"a".repeat(32), "fact_a32");
    trie.insert(&"a".repeat(64), "fact_a64");
    trie.insert("secret_target_fact", "fact_target");
    trie.insert("thủ đô hà nội nghìn năm văn hiến", "fact_hanoi");

    // 1. Benchmark 10,000 characters of repeated 'a's (pathological case)
    let input_10k = format!(
        "{} secret_target_fact {}",
        "a".repeat(5000),
        "a".repeat(5000)
    );
    assert!(input_10k.len() > 10_000);

    let t0 = Instant::now();
    let matches_10k = trie.search(&input_10k);
    let dur_10k = t0.elapsed();

    assert!(matches_10k.contains(&"fact_target".to_string()));
    assert!(matches_10k.contains(&"fact_a1".to_string()));
    assert!(matches_10k.contains(&"fact_a64".to_string()));

    println!(
        "FactTrie 10k chars scan completed in {:?} ({:.3} ms)",
        dur_10k,
        dur_10k.as_secs_f64() * 1000.0
    );

    // Bounded execution SLA: 10,000 characters must execute well under 50ms (even in debug unoptimized)
    assert!(
        dur_10k.as_millis() < 50,
        "FactTrie 10k search took {:?}, exceeding 50ms bound!",
        dur_10k
    );

    // 2. Benchmark 50,000 characters
    let input_50k = format!(
        "{} thủ đô hà nội nghìn năm văn hiến {}",
        "a".repeat(25000),
        "a".repeat(25000)
    );
    let t1 = Instant::now();
    let matches_50k = trie.search(&input_50k);
    let dur_50k = t1.elapsed();

    assert!(matches_50k.contains(&"fact_hanoi".to_string()));
    println!(
        "FactTrie 50k chars scan completed in {:?} ({:.3} ms)",
        dur_50k,
        dur_50k.as_secs_f64() * 1000.0
    );

    // 3. Benchmark 100,000 characters
    let input_100k = format!(
        "{} secret_target_fact {}",
        "a".repeat(50000),
        "a".repeat(50000)
    );
    let t2 = Instant::now();
    let matches_100k = trie.search(&input_100k);
    let dur_100k = t2.elapsed();

    assert!(matches_100k.contains(&"fact_target".to_string()));
    println!(
        "FactTrie 100k chars scan completed in {:?} ({:.3} ms)",
        dur_100k,
        dur_100k.as_secs_f64() * 1000.0
    );

    // 4. Complexity Ratio Verification:
    // If O(L^2), 100k / 10k = 10x size -> 100x duration.
    // If O(L) bounded, 100k / 10k = 10x size -> ~10x duration (with small constant overhead).
    let ratio_100k_to_10k = dur_100k.as_secs_f64() / dur_10k.as_secs_f64().max(0.0001);
    println!(
        "Scaling ratio (100k / 10k): {:.2}x (O(L) theoretical is ~10x, O(L^2) would be ~100x)",
        ratio_100k_to_10k
    );

    assert!(
        ratio_100k_to_10k < 35.0,
        "Scaling ratio {:.2}x indicates super-linear / quadratic blowup!",
        ratio_100k_to_10k
    );
}

#[test]
fn test_fact_trie_depth_boundary_at_64() {
    let mut trie = FactTrie::new();

    // Key of exact length 63
    let key_63 = "a".repeat(63);
    // Key of exact length 64
    let key_64 = "b".repeat(64);
    // Key of exact length 65
    let key_65 = "c".repeat(65);

    trie.insert(&key_63, "fact_63");
    trie.insert(&key_64, "fact_64");
    trie.insert(&key_65, "fact_65");

    // Search for exact 63 chars: must match
    let res_63 = trie.search(&key_63);
    assert!(
        res_63.contains(&"fact_63".to_string()),
        "Key of length 63 must match"
    );

    // Search for exact 64 chars: must match (MAX_MATCH_DEPTH = 64 allows depths 0..=63, which is 64 characters)
    let res_64 = trie.search(&key_64);
    assert!(
        res_64.contains(&"fact_64".to_string()),
        "Key of length 64 must match"
    );

    // Search for key of 65 chars: because search is capped at 64 characters per window,
    // key_65 is not matched by search(key_65)
    let res_65 = trie.search(&key_65);
    assert!(
        !res_65.contains(&"fact_65".to_string()),
        "Key of length 65 exceeds MAX_MATCH_DEPTH (64) and must not match via bounded search"
    );

    // But search_prefix should still find key_65 because search_prefix is not depth-capped
    let prefix_65 = trie.search_prefix(&key_65);
    assert!(
        prefix_65.contains(&"fact_65".to_string()),
        "search_prefix is un-capped and must find key_65"
    );
}

#[test]
fn test_fact_trie_adversarial_multibyte_vietnamese_large_input() {
    let mut trie = FactTrie::new();
    trie.insert("trí tuệ nhân tạo", "fact_ai");
    trie.insert("học sâu đa phương thức", "fact_deep_learning");

    // Construct 30,000 characters of Vietnamese prose with diacritics
    let block = "Dự án phát triển hệ thống trợ lý ảo thông minh LIVA thế hệ mới. ";
    let repeated = block.repeat(450); // ~64 chars * 450 = ~28,800 chars
    let full_text = format!(
        "{repeated} Chúng tôi nghiên cứu trí tuệ nhân tạo và học sâu đa phương thức hôm nay. {repeated}"
    );

    assert!(full_text.chars().count() > 30_000);

    let t0 = Instant::now();
    let matches = trie.search(&full_text);
    let dur = t0.elapsed();

    assert!(matches.contains(&"fact_ai".to_string()));
    assert!(matches.contains(&"fact_deep_learning".to_string()));
    assert_eq!(matches.len(), 2);

    println!(
        "Vietnamese 30k chars scan took {:?} ({:.3} ms)",
        dur,
        dur.as_secs_f64() * 1000.0
    );
    assert!(
        dur.as_millis() < 100,
        "Multibyte large input scan must complete within 100ms"
    );
}
