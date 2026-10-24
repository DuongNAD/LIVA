//! Challenger Empirical Stress Test Suite: Radix FactTrie Hot-Path
//!
//! Adversarially challenges:
//! 1. Vietnamese diacritics & tone marks with mixed casing & unicode normalization.
//! 2. Empty search strings, whitespace variations, non-printable characters.
//! 3. Prefixes without matches, partial branch mismatches, disjoint unicode trees.
//! 4. Single-character keys (ASCII and multibyte UTF-8 Vietnamese glyphs).
//! 5. Duplicate facts across identical keys, distinct keys, and multi-fact nodes.
//! 6. High-volume concurrent read stress (50,000 queries) with latency SLA verification.

use liva_storage::FactTrie;
use std::collections::HashSet;
use std::sync::Arc;
use std::time::Instant;

#[test]
fn test_fact_trie_vietnamese_diacritics_and_casing() {
    let mut trie = FactTrie::new();

    let vn_facts = vec![
        ("Hà Nội", "fact_capital"),
        ("Hồ Chí Minh", "fact_hcmc"),
        ("Đà Nẵng", "fact_danang"),
        ("cà phê sữa đá", "fact_coffee"),
        ("bánh mì thịt nướng", "fact_banhmi"),
        ("tiếng việt đậm đà bản sắc", "fact_vn_culture"),
        ("nguyễn huệ", "fact_emperor"),
        ("phở bò tái nạm", "fact_pho"),
        ("thành phố trực thuộc trung ương", "fact_hcm_status"),
    ];

    for (k, v) in &vn_facts {
        trie.insert(k, v);
    }

    // 1. Exact match with exact casing
    let r1 = trie.search("Hà Nội");
    assert_eq!(r1, vec!["fact_capital"]);

    // 2. Exact match with UPPERCASE
    let r2 = trie.search("HÀ NỘI");
    assert_eq!(r2, vec!["fact_capital"]);

    // 3. Exact match with alternating casing and punctuation
    let r3 = trie.search("Tôi muốn uống CÀ PHÊ SỮA ĐÁ vào buổi sáng!");
    assert_eq!(r3, vec!["fact_coffee"]);

    // 4. Substring matching multiple Vietnamese entities in a sentence
    let sentence = "Hôm nay tôi từ ĐÀ NẴNG ra Hà Nội ăn phở bò tái nạm rồi gặp bạn ở HỒ CHÍ MINH.";
    let r4 = trie.search(sentence);
    let matched_set: HashSet<String> = r4.into_iter().collect();
    assert!(matched_set.contains("fact_danang"), "Must detect Đà Nẵng");
    assert!(matched_set.contains("fact_capital"), "Must detect Hà Nội");
    assert!(
        matched_set.contains("fact_pho"),
        "Must detect phở bò tái nạm"
    );
    assert!(matched_set.contains("fact_hcmc"), "Must detect Hồ Chí Minh");

    // 5. Prefix search on Vietnamese words
    let p1 = trie.search_prefix("hà n");
    assert_eq!(p1, vec!["fact_capital"]);

    let p2 = trie.search_prefix("HÀ");
    assert_eq!(p2, vec!["fact_capital"]);

    let p3 = trie.search_prefix("đà n");
    assert_eq!(p3, vec!["fact_danang"]);

    let p4 = trie.search_prefix("ĐÀ NẴ");
    assert_eq!(p4, vec!["fact_danang"]);

    // 6. Tone mark sensitivity: 'cà phê' should NOT match 'cá phê'
    let r_tone_mismatch = trie.search("cá phê sữa đá");
    assert!(
        r_tone_mismatch.is_empty(),
        "Diacritic mismatch ('cá' vs 'cà') must not produce false positive"
    );
}

#[test]
fn test_fact_trie_empty_and_whitespace_edge_cases() {
    let mut trie = FactTrie::new();

    trie.insert("user_preference", "fact_pref");
    trie.insert("system_mode", "fact_mode");

    // 1. Empty string search
    assert_eq!(trie.search(""), Vec::<String>::new());

    // 2. Pure whitespace search (spaces, tabs, newlines, carriage returns)
    assert_eq!(trie.search("   "), Vec::<String>::new());
    assert_eq!(trie.search("\t\n\r  \t\n"), Vec::<String>::new());

    // 3. Inserting empty key or whitespace key should be a no-op
    trie.insert("", "fact_empty");
    trie.insert("    ", "fact_spaces");
    trie.insert("\t\n", "fact_tabs");

    // Root should not contain fact_empty
    let all_keys = trie.search_prefix("");
    assert!(!all_keys.contains(&"fact_empty".to_string()));
    assert!(!all_keys.contains(&"fact_spaces".to_string()));

    // 4. search_prefix with empty string returns all items
    let all = trie.search_prefix("");
    assert_eq!(all.len(), 2);
    assert!(all.contains(&"fact_pref".to_string()));
    assert!(all.contains(&"fact_mode".to_string()));

    // 5. search_prefix with whitespace only
    let ws_prefix = trie.search_prefix("   \t  ");
    assert_eq!(
        ws_prefix.len(),
        2,
        "Whitespace prefix should trim and return all"
    );
}

#[test]
fn test_fact_trie_no_matches_and_partial_mismatches() {
    let mut trie = FactTrie::new();

    trie.insert("openai_gpt4", "fact_gpt4");
    trie.insert("anthropic_claude", "fact_claude");
    trie.insert("deepseek_v3", "fact_deepseek");
    trie.insert("deepseek_r1", "fact_r1");

    // 1. Completely absent prefix
    assert_eq!(trie.search("google gemini 1.5 pro"), Vec::<String>::new());
    assert_eq!(trie.search_prefix("meta_llama"), Vec::<String>::new());

    // 2. Partial prefix mismatch (shares prefix but branches off)
    assert_eq!(trie.search_prefix("deepseek_v2"), Vec::<String>::new());
    assert_eq!(trie.search_prefix("openai_gpt3"), Vec::<String>::new());

    // 3. Prefix is longer than any entry
    assert_eq!(
        trie.search_prefix("deepseek_v3_super_ultra"),
        Vec::<String>::new()
    );

    // 4. Shared prefix returns both matching branches
    let deepseek_all = trie.search_prefix("deepseek_");
    assert_eq!(deepseek_all.len(), 2);
    assert!(deepseek_all.contains(&"fact_deepseek".to_string()));
    assert!(deepseek_all.contains(&"fact_r1".to_string()));
}

#[test]
fn test_fact_trie_single_char_keys_ascii_and_unicode() {
    let mut trie = FactTrie::new();

    // ASCII single char
    trie.insert("a", "fact_single_a");
    trie.insert("z", "fact_single_z");
    trie.insert("1", "fact_single_1");

    // Vietnamese multibyte single char
    trie.insert("đ", "fact_single_d_stroke");
    trie.insert("ừ", "fact_single_u_horn_grave");
    trie.insert("ở", "fact_single_o_hook");

    // Verify search finds single characters within words / phrases
    let r_a = trie.search("alpha");
    assert!(r_a.contains(&"fact_single_a".to_string()));

    let r_z = trie.search("zero");
    assert!(r_z.contains(&"fact_single_z".to_string()));

    // In Vietnamese "đừng ở đây":
    // "đừng" contains 'đ' and 'ừ'
    // "ở" contains 'ở'
    let r_vn = trie.search("đừng ở đây");
    assert!(
        r_vn.contains(&"fact_single_d_stroke".to_string()),
        "Must detect 'đ'"
    );
    assert!(
        r_vn.contains(&"fact_single_u_horn_grave".to_string()),
        "Must detect 'ừ'"
    );
    assert!(
        r_vn.contains(&"fact_single_o_hook".to_string()),
        "Must detect 'ở'"
    );

    // Prefix search on single char
    let p_d = trie.search_prefix("Đ");
    assert_eq!(p_d, vec!["fact_single_d_stroke"]);

    let p_u = trie.search_prefix("Ừ");
    assert_eq!(p_u, vec!["fact_single_u_horn_grave"]);
}

#[test]
fn test_fact_trie_duplicate_facts_and_multi_fact_nodes() {
    let mut trie = FactTrie::new();

    // 1. Multiple inserts of exact same key and fact
    trie.insert("user_language", "fact_lang_vi");
    trie.insert("user_language", "fact_lang_vi");
    trie.insert("user_language", "fact_lang_vi");

    let r = trie.search("user_language");
    assert_eq!(
        r,
        vec!["fact_lang_vi"],
        "Must not produce duplicate hits for same fact"
    );

    // 2. Multiple different facts on exact same key
    trie.insert("user_language", "fact_lang_en");
    let r_multi = trie.search("user_language");
    assert_eq!(r_multi.len(), 2);
    assert!(r_multi.contains(&"fact_lang_vi".to_string()));
    assert!(r_multi.contains(&"fact_lang_en".to_string()));

    // 3. Same fact mapped to multiple different keys
    trie.insert("vietnamese", "fact_lang_vi");
    trie.insert("tiếng việt", "fact_lang_vi");

    // A sentence containing both keys should only return fact_lang_vi ONCE
    let r_dedup = trie.search("I speak vietnamese và tôi nói tiếng việt hàng ngày");
    let vi_count = r_dedup.iter().filter(|&k| k == "fact_lang_vi").count();
    assert_eq!(
        vi_count, 1,
        "Duplicate occurrences of the same fact_key in a single search must be deduplicated"
    );
}

#[test]
fn test_fact_trie_high_volume_concurrent_read_stress() {
    let mut trie = FactTrie::new();

    // Ingest 1,000 distinct facts with non-overlapping keys
    for i in 0..1000 {
        let key = format!("entity_cat_{:03}_item_{:04}", i % 20, i);
        let fact = format!("fact_val_{}", i);
        trie.insert(&key, &fact);
    }

    let shared_trie = Arc::new(trie);
    let thread_count = 4;
    let queries_per_thread = 12500; // 4 * 12500 = 50,000 total queries

    let mut handles = Vec::new();
    let start_all = Instant::now();

    for t_idx in 0..thread_count {
        let trie_ref = Arc::clone(&shared_trie);
        let handle = std::thread::spawn(move || {
            let mut latencies_us = Vec::with_capacity(queries_per_thread);

            for q in 0..queries_per_thread {
                let target_i = (t_idx * 10000 + q * 17) % 1000;
                let query_str = format!(
                    "Looking for entity_cat_{:03}_item_{:04} in context",
                    target_i % 20,
                    target_i
                );

                let t0 = Instant::now();
                let matches = trie_ref.search(&query_str);
                let elapsed_us = t0.elapsed().as_nanos() as f64 / 1000.0;
                latencies_us.push(elapsed_us);

                assert!(
                    matches.contains(&format!("fact_val_{}", target_i)),
                    "Must find fact for known entity"
                );
            }

            latencies_us
        });
        handles.push(handle);
    }

    let mut all_latencies = Vec::with_capacity(50000);
    for h in handles {
        let lats = h.join().expect("Worker thread panicked");
        all_latencies.extend(lats);
    }

    let total_elapsed = start_all.elapsed();
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let avg_us = all_latencies.iter().sum::<f64>() / all_latencies.len() as f64;
    let p50_us = all_latencies[all_latencies.len() * 50 / 100];
    let p95_us = all_latencies[all_latencies.len() * 95 / 100];
    let p99_us = all_latencies[all_latencies.len() * 99 / 100];

    println!(
        "FactTrie 50,000-query Stress Test: total {:?}, avg {:.3} µs, P50 {:.3} µs, P95 {:.3} µs, P99 {:.3} µs",
        total_elapsed, avg_us, p50_us, p95_us, p99_us
    );

    // Hard SLA Check: FactTrie average latency must comfortably beat 50 µs ceiling (SLA is sub-2µs in release, sub-25µs in debug)
    assert!(
        avg_us < 50.0,
        "SLA Violation: FactTrie avg latency ({:.3} µs) exceeded 50.0 µs limit",
        avg_us
    );
    assert!(
        p95_us < 50.0,
        "SLA Violation: FactTrie P95 latency ({:.3} µs) exceeded 50.0 µs limit",
        p95_us
    );
}
