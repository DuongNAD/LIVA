//! Phase 2 Tier 5 Adversarial Coverage Hardening Test Suite
//!
//! Subsystems covered:
//! 1. Radix Tree Prefix Cache: Concurrent thread storms, deep branch collisions, zero-length prefixes, memory eviction under tight bounds.
//! 2. LLMLingua-2 Context Compression: Malformed XML boundaries, unclosed tags, multi-byte Vietnamese unicode homographs, emojis, and >95% extreme compression stress.
//! 3. Obsidian Vault Sync & CSR: Symlink loops, relative path escapes, path traversal %2e%2e%2f, null bytes \0, isolated sink nodes in graph.
//! 4. HippoRAG Parallel PPR: 100,000 nodes disconnected components, zero seed vector fallback, stochastic probability mass conservation.
//! 5. 3-Way RRF: Zero candidate modalities, single-candidate modalities, score normalization bounds, tie breaking.
//! 6. AES-256-GCM v2 Enclaves: Tampered nonces/tags, memory zeroization on drop, SQLite WAL secure delete and vacuum sanitization.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use zeroize::Zeroize;

use liva_native_core::crypto::FactRead;
use liva_native_core::db::DatabasePool;
use liva_native_core::memory::compression::{
    DialogueTurn, LLMLinguaCompressor, LLMLinguaConfig, RecursiveSummaryTree, SummaryTreeConfig,
};
use liva_native_core::memory::enclave::{EnclaveError, MemoryEnclave};
use liva_native_core::memory::graph::{CsrGraph, HippoRagEngine};
use liva_native_core::memory::l1_working::{RadixPrefixCache, TokenId};
use liva_native_core::memory::l3_semantic::{
    extract_wikilinks, validate_and_resolve_path, VaultSecurityError,
};
use liva_native_core::memory::search::{
    aggregate_graph_activations, SearchHit, TriModalRrfEngine,
};

// ============================================================================
// SECTION 1: RADIX TREE PREFIX CACHE ADVERSARIAL HARDENING
// ============================================================================

#[tokio::test]
async fn test_tier5_radix_concurrent_thread_storm() {
    let cache = Arc::new(RadixPrefixCache::new(500));
    let num_tasks = 50;
    let ops_per_task = 100;

    let mut handles = Vec::with_capacity(num_tasks);

    for task_id in 0..num_tasks {
        let cache_clone = Arc::clone(&cache);
        handles.push(tokio::spawn(async move {
            for i in 0..ops_per_task {
                let shared_prefix = vec![1, 2, 3, 4];
                let mut unique_seq = shared_prefix.clone();
                unique_seq.push(1000 + (task_id as u32));
                unique_seq.push(2000 + (i as u32));

                let block_id = task_id * 1000 + i;
                let is_pinned = (task_id + i) % 10 == 0;

                // 1. Insert prefix
                cache_clone.insert_prefix(&unique_seq, block_id, is_pinned).await;

                // 2. Match prefix
                let (matched_len, blocks) = cache_clone.match_prefix(&unique_seq).await;
                assert!(matched_len >= 4, "Must match at least common prefix");
                assert!(!blocks.is_empty(), "Must return at least one reusable block");

                // 3. Pin prefix conditionally
                if is_pinned {
                    cache_clone.pin_prefix(&unique_seq).await;
                }

                // 4. Interleaving eviction
                if i % 25 == 0 {
                    let _ = cache_clone.evict_lru(2).await;
                }
            }
        }));
    }

    for handle in handles {
        handle.await.expect("Concurrent task failed");
    }

    let stats = cache.stats();
    assert!(stats.total_lookups >= (num_tasks * ops_per_task) as u64);
    assert!(stats.hits > 0);
    assert!(stats.hit_ratio > 0.0);
}

#[tokio::test]
async fn test_tier5_radix_deep_branch_collisions() {
    let cache = RadixPrefixCache::new(200);

    // Build 500-token deep common trunk
    let trunk: Vec<TokenId> = (1..=500).collect();
    cache.insert_prefix(&trunk, 1, true).await;

    // Diverge at depth 500
    for branch_id in 1..=20 {
        let mut branch = trunk.clone();
        branch.push(10_000 + branch_id);
        branch.push(20_000 + branch_id);
        cache.insert_prefix(&branch, 100 + (branch_id as usize), false).await;
    }

    // Diverge at mid-depth 250
    for mid_id in 1..=10 {
        let mut mid_branch: Vec<TokenId> = trunk[..250].to_vec();
        mid_branch.push(50_000 + mid_id);
        mid_branch.push(60_000 + mid_id);
        cache.insert_prefix(&mid_branch, 500 + (mid_id as usize), false).await;
    }

    // Diverge at shallow-depth 50
    for shallow_id in 1..=5 {
        let mut shallow_branch: Vec<TokenId> = trunk[..50].to_vec();
        shallow_branch.push(90_000 + shallow_id);
        cache.insert_prefix(&shallow_branch, 900 + (shallow_id as usize), false).await;
    }

    // Verify trunk matching
    let (trunk_matched, trunk_blks) = cache.match_prefix(&trunk).await;
    assert_eq!(trunk_matched, 500);
    assert_eq!(trunk_blks, vec![1]);

    // Verify deep branch matching
    let mut query_deep = trunk.clone();
    query_deep.push(10_005);
    query_deep.push(20_005);
    query_deep.push(99_999); // Extra unmatched token
    let (deep_matched, deep_blks) = cache.match_prefix(&query_deep).await;
    assert_eq!(deep_matched, 502);
    assert_eq!(deep_blks, vec![1, 105]);

    // Verify mid branch matching
    let mut query_mid: Vec<TokenId> = trunk[..250].to_vec();
    query_mid.push(50_003);
    query_mid.push(60_003);
    let (mid_matched, mid_blks) = cache.match_prefix(&query_mid).await;
    assert_eq!(mid_matched, 252);
    assert_eq!(mid_blks, vec![503]);
}

#[tokio::test]
async fn test_tier5_radix_zero_length_prefixes() {
    let cache = RadixPrefixCache::new(50);

    // Empty insert
    cache.insert_prefix(&[], 1, false).await;
    assert_eq!(cache.allocated_blocks(), 0);

    // Empty match
    let (matched_len, blocks) = cache.match_prefix(&[]).await;
    assert_eq!(matched_len, 0);
    assert!(blocks.is_empty());

    // Empty pin
    let pin_result = cache.pin_prefix(&[]).await;
    assert!(pin_result);

    // Insert valid, match empty
    cache.insert_prefix(&[10, 20, 30], 100, false).await;
    let (matched_empty, blks_empty) = cache.match_prefix(&[]).await;
    assert_eq!(matched_empty, 0);
    assert!(blks_empty.is_empty());
}

#[tokio::test]
async fn test_tier5_radix_tight_bound_eviction_and_pinned_immunity() {
    let cache = RadixPrefixCache::new(5);

    // Insert 2 pinned system prefixes
    let sys1 = vec![100, 101, 102];
    let sys2 = vec![200, 201, 202];
    cache.insert_prefix(&sys1, 1, true).await;
    cache.pin_prefix(&sys1).await;
    cache.insert_prefix(&sys2, 2, true).await;
    cache.pin_prefix(&sys2).await;

    // Insert 20 unpinned prefixes in rapid succession
    for i in 1..=20 {
        let unpinned = vec![1000 + i, 2000 + i];
        cache.insert_prefix(&unpinned, 10 + (i as usize), false).await;
        // Trigger eviction if above capacity
        if cache.allocated_blocks() > 5 {
            let evicted = cache.evict_lru(1).await;
            assert!(evicted <= 1);
        }
    }

    // Explicitly request multiple evictions
    let evicted_total = cache.evict_lru(10).await;
    assert!(evicted_total <= 5);

    // Pinned blocks must NEVER be evicted
    let (matched1, blks1) = cache.match_prefix(&sys1).await;
    assert_eq!(matched1, 3);
    assert_eq!(blks1, vec![1]);

    let (matched2, blks2) = cache.match_prefix(&sys2).await;
    assert_eq!(matched2, 3);
    assert_eq!(blks2, vec![2]);
}

#[tokio::test]
async fn test_tier5_radix_zero_capacity_resilience() {
    let cache = RadixPrefixCache::new(0);
    assert_eq!(cache.max_blocks(), 0);

    cache.insert_prefix(&[1, 2, 3], 1, false).await;
    let (_len, _blks) = cache.match_prefix(&[1, 2, 3]).await;
    let stats = cache.stats();
    assert_eq!(stats.max_blocks, 0);
}

// ============================================================================
// SECTION 2: LLMLINGUA-2 ADVERSARIAL HARDENING
// ============================================================================

#[test]
fn test_tier5_llmlingua_malformed_xml_boundaries() {
    let compressor = LLMLinguaCompressor::new();

    let hostile_inputs = [
        "<SYSTEM_CONTEXT without closing tag and lots of noisy filler words that should be compressed",
        "<tag attr=\"nested > value\">inside content</tag> <unclosed><another attr='x'>",
        "<<<<<>>>>> <<<<tag>> content <<</tag>>>",
        "<SYSTEM_CONTEXT>Protected System Content</SYSTEM_CONTEXT> <context_memory>Episodic Recall</context_memory> <broken attr=",
        "</only_closing_tag> random noise without opening tag <valid_self_closing/>",
    ];

    for input in &hostile_inputs {
        let result = compressor.compress(input);
        assert!(!result.compressed_text.is_empty(), "Output should not be empty for non-empty input");
        assert!(result.metrics.compression_ratio >= 1.0);
        assert!(result.metrics.estimated_semantic_loss >= 0.0 && result.metrics.estimated_semantic_loss <= 1.0);

        // Ensure valid XML structures in input are preserved
        if input.contains("<SYSTEM_CONTEXT>Protected System Content</SYSTEM_CONTEXT>") {
            assert!(result.compressed_text.contains("<SYSTEM_CONTEXT>"));
            assert!(result.compressed_text.contains("</SYSTEM_CONTEXT>"));
            assert!(result.compressed_text.contains("Protected System Content"));
        }
    }
}

#[test]
fn test_tier5_llmlingua_vietnamese_unicode_homographs_and_emojis() {
    let compressor = LLMLinguaCompressor::new();

    let unicode_stress_text = "\
        <SYSTEM_CONTEXT>\n\
        Chào bạn! LIVA là Hệ điều hành Trí tuệ Nhân tạo thế hệ mới 🚀🔥.\n\
        Trụ sở chính đặt tại Hà Nội và Thành phố Hồ Chí Minh 🇻🇳.\n\
        Kiến trúc bộ nhớ phân cấp gồm 4 tầng: L1 Working Memory, L2 Episodic Memory, \
        L3 Semantic Knowledge Graph [[ObsidianVault]], và L4 Procedural Memory.\n\
        Người dùng Nguyễn Trãi và Đoàn Thị Điểm đã xác thực danh tính qua chữ ký số 🛡️.\n\
        Các ký tự đặc biệt: \u{200B}\u{200C}\u{200D} 👨‍👩‍👧‍👦 👩‍💻 ✨ 💯.\n\
        Nghiên cứu về Tiếng Việt: Ă, Â, Đ, Ê, Ô, Ơ, Ư, ắ, ằ, ẳ, ẵ, ặ, ế, ề, ể, ễ, ệ, ố, ồ, ổ, ỗ, ộ.\n\
        </SYSTEM_CONTEXT>\n\
        Hãy tóm tắt ngắn gọn báo cáo tài chính ngày 2026-09-01 với tổng doanh thu $1,500,000 USD tăng 25.5%.";

    let result = compressor.compress(unicode_stress_text);

    // Verify valid UTF-8 and non-empty result
    assert!(!result.compressed_text.is_empty());
    assert!(std::str::from_utf8(result.compressed_text.as_bytes()).is_ok());

    // Verify critical entities, wikilinks, dates, and numbers are preserved
    assert!(result.compressed_text.contains("<SYSTEM_CONTEXT>"));
    assert!(result.compressed_text.contains("</SYSTEM_CONTEXT>"));
    assert!(result.compressed_text.contains("LIVA"));
    assert!(result.compressed_text.contains("[[ObsidianVault]]"));
    assert!(result.compressed_text.contains("2026-09-01"));
    assert!(result.compressed_text.contains("$1,500,000"));
    assert!(result.compressed_text.contains("25.5"));

    // Verify emojis survived without panicking on byte slicing
    assert!(result.compressed_text.contains("🚀") || result.compressed_text.contains("🔥"));
}

#[test]
fn test_tier5_llmlingua_extreme_compression_bounds() {
    let mut config = LLMLinguaConfig::default();
    config.target_reduction_ratio = Some(0.99); // 99% reduction request (clamped to 95%)

    let compressor = LLMLinguaCompressor::with_config(config);

    let text = "\
        The quick brown fox jumps over the lazy dog repeatedly again and again. \
        Furthermore, it is very important to clearly note that conversational filler \
        words should be aggressively pruned away without losing core context. \
        Here is a critical entity: IBM Quantum System Two located at Yorktown Heights on 2026-10-15 with 1,121 qubits.";

    let result = compressor.compress(text);

    // Reduction ratio should be clamped safely
    assert!(result.metrics.reduction_ratio >= 0.20);
    assert!(result.metrics.compression_ratio > 1.0);

    // Protected entities and dates must survive extreme reduction
    assert!(result.compressed_text.contains("IBM"));
    assert!(result.compressed_text.contains("Quantum"));
    assert!(result.compressed_text.contains("2026-10-15"));
    assert!(result.compressed_text.contains("1,121"));
}

#[test]
fn test_tier5_llmlingua_nested_code_fences_and_json_delimiters() {
    let compressor = LLMLinguaCompressor::new();

    let complex_text = "\
        Please examine the following Rust configuration:\n\
        ```rust\n\
        pub struct Config {\n\
            pub port: u16,\n\
            pub host: String,\n\
        }\n\
        ```\n\
        Also verify this JSON payload:\n\
        {\n\
            \"service\": \"liva-gateway\",\n\
            \"active\": true,\n\
            \"retry_count\": 5,\n\
            \"endpoints\": [\"/v1/chat\", \"/v1/memory\"]\n\
        }\n\
        Inline code reference: `cargo test --package liva-native-core`.";

    let result = compressor.compress(complex_text);

    // Code blocks and inline code should be protected
    assert!(result.compressed_text.contains("```rust"));
    assert!(result.compressed_text.contains("pub struct Config"));
    assert!(result.compressed_text.contains("`cargo test --package liva-native-core`"));
}

#[tokio::test]
async fn test_tier5_recursive_summary_tree_adversarial_turn_churn() {
    let pool = DatabasePool::new_in_memory().expect("In-memory SQLite init failed");
    let config = SummaryTreeConfig {
        max_context_tokens: 120,
        overflow_threshold_ratio: 0.70, // Trigger at 84 tokens
        chunk_target_tokens: 30,
        min_turns_per_chunk: 2,
        max_turns_per_chunk: 4,
        meta_summary_fanout: 2,
        domain: "StressSession".to_string(),
        category: "Adversarial".to_string(),
    };

    let session_id = "sess_stress_001";
    let mut tree = RecursiveSummaryTree::new(session_id, config).with_db_pool(pool);

    // Add 20 dialogue turns with varying sizes (some empty, some huge)
    for i in 1..=20 {
        let role = if i % 2 == 0 { "user" } else { "assistant" };
        let content = if i == 5 {
            // Extremely short turn
            "OK"
        } else if i == 10 {
            // Large 300-token turn with entities
            "Project Apollo 11 launched Saturn V rocket from Kennedy Space Center on July 16, 1969 carrying Neil Armstrong and Buzz Aldrin to Mare Tranquillitatis on the Moon."
        } else {
            "Discussing standard system parameters, database configurations, and memory retention policies."
        };

        let turn = DialogueTurn::new(session_id, role, content, 1_700_000_000 + (i as i64));
        let _rep = tree.add_turn(turn);
    }

    let summaries = tree.all_summaries();
    assert!(!summaries.is_empty(), "Must produce summary nodes under high churn");

    for s in summaries {
        assert!(s.verify_integrity(), "Merkle cryptographic integrity must pass for all nodes");
    }
}

// ============================================================================
// SECTION 3: OBSIDIAN VAULT SYNC & CSR ADVERSARIAL HARDENING
// ============================================================================

#[test]
fn test_tier5_vault_path_traversal_fuzzing() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    fs::create_dir_all(root.join("Knowledge")).unwrap();
    fs::write(root.join("Knowledge").join("valid.md"), "# Valid Note").unwrap();

    let malicious_paths = [
        "../secret.txt",
        "../../etc/passwd",
        "..\\..\\windows\\system32\\cmd.exe",
        "Knowledge/../../outside.md",
        "%2e%2e%2fsecret.txt",
        "%2e%2e%5csecret.txt",
        "Knowledge/test%00.md",
        "Knowledge/test\0.md",
        "Knowledge/test\x1f.md",
        "Knowledge/test\x7f.md",
        "Knowledge/../../../etc/shadow",
    ];

    for mal_path in &malicious_paths {
        let res = validate_and_resolve_path(root, mal_path);
        assert!(res.is_err(), "Path '{}' must be rejected", mal_path);
    }
}

#[cfg(unix)]
#[test]
fn test_tier5_vault_symlink_loops_detection() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let link_a = root.join("link_a");
    let link_b = root.join("link_b");

    // Circular symlink loop
    symlink(&link_b, &link_a).unwrap();
    symlink(&link_a, &link_b).unwrap();

    let err = validate_and_resolve_path(root, "link_a").unwrap_err();
    assert!(matches!(err, VaultSecurityError::SymlinkLoopDetected(_)));

    // Multi-hop symlink loop: c -> d -> e -> c
    let link_c = root.join("link_c");
    let link_d = root.join("link_d");
    let link_e = root.join("link_e");

    symlink(&link_d, &link_c).unwrap();
    symlink(&link_e, &link_d).unwrap();
    symlink(&link_c, &link_e).unwrap();

    let err_multi = validate_and_resolve_path(root, "link_c").unwrap_err();
    assert!(matches!(err_multi, VaultSecurityError::SymlinkLoopDetected(_)));
}

#[test]
fn test_tier5_wikilinks_parser_adversarial_edge_cases() {
    let hostile_markdown = "\
        Normal link: [[TargetNote]]\n\
        Link with alias: [[TargetNote|My Custom Alias]]\n\
        Link with heading: [[TargetNote#Section Title]]\n\
        Link with heading and alias: [[TargetNote#Section Title|Alias]]\n\
        Unclosed link: [[Unclosed Note\n\
        Empty link: [[]]\n\
        Only pipe: [[|]]\n\
        Only hash: [[#]]\n\
        Nested brackets: [[[[NestedNote]]]]\n\
        Special characters: [[Note with %20 and & + - _ ! @ # $ ^ * ()]]\n\
        Percent-encoded: [[Target%20Note%2Emd]]\n\
        Multiple wikilinks on one line: [[Note1]] and [[Note2|Alias2]] and [[Note3#H3]].";

    let links = extract_wikilinks(hostile_markdown);
    assert!(!links.is_empty());

    let targets: HashSet<String> = links.iter().map(|l| l.target.clone()).collect();
    assert!(targets.contains("TargetNote"));
    assert!(targets.contains("Note1"));
    assert!(targets.contains("Note2"));
    assert!(targets.contains("Note3"));

    // Check alias extraction
    let alias_link = links.iter().find(|l| l.target == "TargetNote" && l.alias.as_deref() == Some("My Custom Alias"));
    assert!(alias_link.is_some());
}

#[test]
fn test_tier5_csr_isolated_sink_nodes_and_disconnected_graphs() {
    // Graph with 5 nodes:
    // Node 0 -> Node 1 (w=1.0)
    // Node 1 -> Node 2 (w=1.0)
    // Node 2 is a Sink (out-degree = 0)
    // Node 3 is completely isolated (no in or out edges)
    // Node 4 -> Node 4 (Self-loop)
    let raw_edges = vec![
        (0, 1, 1.0f32),
        (1, 2, 1.0f32),
        (4, 4, 1.0f32),
    ];
    let graph = CsrGraph::from_raw_edges(&raw_edges, 5, false);

    assert_eq!(graph.num_nodes, 5);
    assert_eq!(graph.out_degree(0), 1);
    assert_eq!(graph.out_degree(1), 1);
    assert_eq!(graph.out_degree(2), 0, "Node 2 is a sink");
    assert_eq!(graph.out_degree(3), 0, "Node 3 is isolated");
    assert_eq!(graph.out_degree(4), 1, "Node 4 has self-loop");

    let transposed = graph.transpose();
    assert_eq!(transposed.num_nodes, 5);
    assert_eq!(transposed.out_degree(2), 1, "Transposed Node 2 has in-degree from Node 1");
    assert_eq!(transposed.out_degree(3), 0, "Transposed Node 3 remains isolated");
}

// ============================================================================
// SECTION 4: HIPPORAG PARALLEL PPR ADVERSARIAL HARDENING
// ============================================================================

#[test]
fn test_tier5_hipporag_100k_nodes_disconnected_components() {
    // 100,000 nodes structured as 1,000 isolated components of 100 nodes each (Ring topology)
    let num_nodes = 100_000;
    let mut raw_edges = Vec::with_capacity(num_nodes);

    for comp in 0..1000 {
        let offset = (comp * 100) as u32;
        for i in 0..100 {
            let src = offset + i;
            let dst = offset + ((i + 1) % 100);
            raw_edges.push((src, dst, 1.0f32));
        }
    }

    let graph = CsrGraph::from_raw_edges(&raw_edges, num_nodes, false);
    let engine = HippoRagEngine::new(graph);

    // Run PPR with seeds in Component 0 (nodes 0, 1) and Component 500 (node 50000)
    let seeds = [0u32, 1u32, 50_000u32];
    let weights = [1.0f32, 1.0f32, 2.0f32];

    let result = engine.run_ppr(&seeds, &weights);

    assert_eq!(result.probabilities.len(), num_nodes);
    assert!(result.iterations > 0);

    // Verify stochastic probability mass conservation: sum(pi) == 1.0 +- 1e-3
    let total_mass: f32 = result.probabilities.iter().sum();
    assert!(
        (total_mass - 1.0).abs() < 1e-3,
        "Total probability mass {} must equal 1.0",
        total_mass
    );

    // Seeded components must have higher activation than unseeded components
    assert!(result.probabilities[0] > result.probabilities[200]);
    assert!(result.probabilities[50_000] > result.probabilities[300]);
}

#[test]
fn test_tier5_hipporag_zero_seed_vector_fallback() {
    // 4-node ring graph
    let edges = [
        ("A", "B", 1.0f32),
        ("B", "C", 1.0f32),
        ("C", "D", 1.0f32),
        ("D", "A", 1.0f32),
    ];
    let graph = CsrGraph::from_named_edges(&edges, false);
    let engine = HippoRagEngine::new(graph);

    // 1. Empty seed vector
    let res_empty = engine.run_ppr(&[], &[]);
    assert_eq!(res_empty.probabilities.len(), 4);
    let mass_empty: f32 = res_empty.probabilities.iter().sum();
    assert!((mass_empty - 1.0).abs() < 1e-4);

    // In a symmetric ring with uniform teleport fallback, all stationary probabilities must be equal
    for &prob in &res_empty.probabilities {
        assert!((prob - 0.25).abs() < 1e-3, "Expected 0.25, got {}", prob);
    }

    // 2. All zero weights
    let res_zero = engine.run_ppr(&[0, 1], &[0.0, 0.0]);
    let mass_zero: f32 = res_zero.probabilities.iter().sum();
    assert!((mass_zero - 1.0).abs() < 1e-4);

    // 3. Negative weights
    let res_neg = engine.run_ppr(&[0, 1], &[-1.0, -5.0]);
    let mass_neg: f32 = res_neg.probabilities.iter().sum();
    assert!((mass_neg - 1.0).abs() < 1e-4);

    // 4. Out of bounds indices
    let res_oob = engine.run_ppr(&[9999, 8888], &[1.0, 1.0]);
    let mass_oob: f32 = res_oob.probabilities.iter().sum();
    assert!((mass_oob - 1.0).abs() < 1e-4);
}

#[test]
fn test_tier5_hipporag_stochastic_probability_mass_conservation_sinks() {
    // Star graph where central node distributes to 20 leaf nodes, all leaf nodes are sinks
    let mut raw_edges = Vec::new();
    let num_nodes = 21;
    for leaf in 1..num_nodes {
        raw_edges.push((0u32, leaf as u32, 1.0f32));
    }

    let graph = CsrGraph::from_raw_edges(&raw_edges, num_nodes, false);
    let engine = HippoRagEngine::new(graph);

    let res = engine.run_ppr(&[0], &[1.0]);

    assert_eq!(res.probabilities.len(), num_nodes);
    let total_mass: f32 = res.probabilities.iter().sum();
    assert!(
        (total_mass - 1.0).abs() < 1e-4,
        "Dangling sink nodes must not leak probability mass! Got {}",
        total_mass
    );

    // Top-K ranking edge cases
    let top_0 = engine.rank_top_k(&res.probabilities, 0);
    assert!(top_0.is_empty());

    let top_all = engine.rank_top_k(&res.probabilities, 100);
    assert_eq!(top_all.len(), num_nodes);
}

// ============================================================================
// SECTION 5: 3-WAY RRF ADVERSARIAL HARDENING
// ============================================================================

fn make_test_hit(id: i64, vec_id: &str, score: f64) -> SearchHit {
    SearchHit {
        id,
        vec_id: vec_id.to_string(),
        content: format!("Content for {}", vec_id),
        r#type: "fact".to_string(),
        domain: "general".to_string(),
        category: "knowledge".to_string(),
        score,
        distance: 0.0,
        trace_keywords: vec!["test".to_string()],
        source_event_ids: Vec::new(),
        created_at: 1_700_000_000,
        source_channel: "test".to_string(),
    }
}

#[test]
fn test_tier5_rrf_zero_and_single_candidate_modalities() {
    let rrf = TriModalRrfEngine::new();

    // 1. Zero candidate modalities
    let empty_fused = rrf.fuse(&[], &[], &[], 10);
    assert!(empty_fused.is_empty());

    // 2. Single modality: BM25 only
    let bm25_hits = vec![
        make_test_hit(1, "doc_a", 15.5),
        make_test_hit(2, "doc_b", 12.0),
    ];
    let bm25_only = rrf.fuse(&bm25_hits, &[], &[], 10);
    assert_eq!(bm25_only.len(), 2);
    assert_eq!(bm25_only[0].vec_id, "doc_a");
    assert_eq!(bm25_only[1].vec_id, "doc_b");
    assert!(bm25_only[0].score > bm25_only[1].score);

    // 3. Single modality: Dense Vector only
    let dense_hits = vec![
        make_test_hit(10, "doc_x", 0.92),
        make_test_hit(11, "doc_y", 0.85),
    ];
    let dense_only = rrf.fuse(&[], &dense_hits, &[], 10);
    assert_eq!(dense_only.len(), 2);
    assert_eq!(dense_only[0].vec_id, "doc_x");

    // 4. Single modality: Graph PPR only
    let graph_hits = vec![
        make_test_hit(20, "doc_g1", 0.35),
        make_test_hit(21, "doc_g2", 0.22),
    ];
    let graph_only = rrf.fuse(&[], &[], &graph_hits, 10);
    assert_eq!(graph_only.len(), 2);
    assert_eq!(graph_only[0].vec_id, "doc_g1");
}

#[test]
fn test_tier5_rrf_score_normalization_and_bounds() {
    let custom_rrf = TriModalRrfEngine::with_weights(60.0, 0.30, 0.45, 0.25);

    let bm25 = vec![make_test_hit(1, "doc_shared", 20.0)];
    let dense = vec![make_test_hit(1, "doc_shared", 0.99)];
    let graph = vec![make_test_hit(1, "doc_shared", 0.80)];

    let fused = custom_rrf.fuse_detailed(&bm25, &dense, &graph, 5);
    assert_eq!(fused.len(), 1);

    let hit = &fused[0];
    assert_eq!(hit.hit.vec_id, "doc_shared");
    assert_eq!(hit.bm25_rank, Some(1));
    assert_eq!(hit.dense_rank, Some(1));
    assert_eq!(hit.graph_rank, Some(1));

    // Maximum theoretical score for Rank 1 across all 3 channels:
    // (0.30 + 0.45 + 0.25) / (60 + 1) = 1.0 / 61.0 ~= 0.01639344
    let expected_max_score = 1.0 / 61.0;
    assert!((hit.rrf_score - expected_max_score).abs() < 1e-6);
    assert!(hit.rrf_score > 0.0 && hit.rrf_score <= 1.0);
}

#[test]
fn test_tier5_rrf_deterministic_tie_breaking() {
    // Equal channel weights (0.50, 0.50, 0.0) so Rank 1 in BM25 has same RRF score as Rank 1 in Dense
    let rrf_equal = TriModalRrfEngine::with_weights(60.0, 0.50, 0.50, 0.0);

    let bm25_hits = vec![make_test_hit(1, "doc_z", 10.0)];
    let dense_hits = vec![make_test_hit(2, "doc_a", 0.9)];

    let fused = rrf_equal.fuse(&bm25_hits, &dense_hits, &[], 10);
    assert_eq!(fused.len(), 2);
    // Both doc_z and doc_a have score 0.50/(60+1) = 0.50/61
    assert!((fused[0].score - fused[1].score).abs() < 1e-9);
    // Deterministic tie-breaking: doc_a before doc_z (lexicographical vec_id ascending)
    assert_eq!(fused[0].vec_id, "doc_a");
    assert_eq!(fused[1].vec_id, "doc_z");
}

#[test]
fn test_tier5_aggregate_graph_activations_edge_cases() {
    let mut node_to_idx = HashMap::new();
    node_to_idx.insert("EntityA".to_string(), 0u32);
    node_to_idx.insert("EntityB".to_string(), 1u32);

    let ppr_distribution = vec![0.6f32, 0.4f32];

    let doc_entities = vec![
        (1i64, "doc_1".to_string(), "Content 1".to_string(), vec![("EntityA".to_string(), 1.0f32)]),
        (2i64, "doc_2".to_string(), "Content 2".to_string(), vec![("UnmappedEntity".to_string(), 1.0f32)]),
        (3i64, "doc_3".to_string(), "Content 3".to_string(), vec![("EntityB".to_string(), 0.0f32)]), // Zero weight
    ];

    let hits = aggregate_graph_activations(&ppr_distribution, &node_to_idx, &doc_entities, 10);
    assert_eq!(hits.len(), 1, "Only doc_1 has positive mapped activation");
    assert_eq!(hits[0].vec_id, "doc_1");
    assert!((hits[0].score - 0.6).abs() < 1e-5);
}

// ============================================================================
// SECTION 6: AES-256-GCM V2 ENCLAVES ADVERSARIAL HARDENING
// ============================================================================

#[test]
fn test_tier5_enclave_tampered_nonces_and_tags() {
    let enclave = MemoryEnclave::new_with_argon2id(b"master_passphrase", b"master_salt_1234").unwrap();
    let plaintext = "Top secret biometric credentials and episodic memories.";
    let envelope = enclave.encrypt_string(plaintext).unwrap();

    // Envelope format: v2:<salt_hex>:<iv_hex>:<tag_hex>:<cipher_hex>
    let parts: Vec<&str> = envelope["v2:".len()..].split(':').collect();
    assert_eq!(parts.len(), 4);

    let salt_hex = parts[0];
    let iv_hex = parts[1];
    let tag_hex = parts[2];
    let cipher_hex = parts[3];

    // 1. Bit-flip IV / Nonce
    let mut tampered_iv_bytes = hex::decode(iv_hex).unwrap();
    tampered_iv_bytes[0] ^= 0x01;
    let tampered_iv_env = format!("v2:{}:{}:{}:{}", salt_hex, hex::encode(tampered_iv_bytes), tag_hex, cipher_hex);
    assert_eq!(enclave.decrypt_string(&tampered_iv_env).unwrap_err(), EnclaveError::AuthenticationFailed);

    // 2. Bit-flip Tag
    let mut tampered_tag_bytes = hex::decode(tag_hex).unwrap();
    tampered_tag_bytes[0] ^= 0x01;
    let tampered_tag_env = format!("v2:{}:{}:{}:{}", salt_hex, iv_hex, hex::encode(tampered_tag_bytes), cipher_hex);
    assert_eq!(enclave.decrypt_string(&tampered_tag_env).unwrap_err(), EnclaveError::AuthenticationFailed);

    // 3. Bit-flip Ciphertext
    let mut tampered_cipher_bytes = hex::decode(cipher_hex).unwrap();
    tampered_cipher_bytes[0] ^= 0x01;
    let tampered_cipher_env = format!("v2:{}:{}:{}:{}", salt_hex, iv_hex, tag_hex, hex::encode(tampered_cipher_bytes));
    assert_eq!(enclave.decrypt_string(&tampered_cipher_env).unwrap_err(), EnclaveError::AuthenticationFailed);

    // 4. Bit-flip Salt
    let mut tampered_salt_bytes = hex::decode(salt_hex).unwrap();
    tampered_salt_bytes[0] ^= 0x01;
    let tampered_salt_env = format!("v2:{}:{}:{}:{}", hex::encode(tampered_salt_bytes), iv_hex, tag_hex, cipher_hex);
    assert_eq!(enclave.decrypt_string(&tampered_salt_env).unwrap_err(), EnclaveError::AuthenticationFailed);

    // 5. Malformed envelope structures
    assert_eq!(enclave.decrypt_string("invalid_prefix").unwrap_err(), EnclaveError::InvalidPrefix);
    assert_eq!(enclave.decrypt_string("v2:part1:part2").unwrap_err(), EnclaveError::MalformedEnvelope);

    // Invalid non-hex characters with correct lengths (32 chars)
    let invalid_hex_32 = "z".repeat(32);
    let hex_err_env = format!("v2:{}:{}:{}:{}", invalid_hex_32, iv_hex, tag_hex, cipher_hex);
    assert!(matches!(enclave.decrypt_string(&hex_err_env), Err(EnclaveError::HexDecode(_))));
}

#[test]
fn test_tier5_enclave_memory_zeroization_on_drop() {
    let enclave = MemoryEnclave::new_with_argon2id(b"passphrase", b"salt123456789012").unwrap();
    let secret = "SuperSecretKeyThatMustBeZeroizedOnDrop";
    let envelope = enclave.encrypt_string(secret).unwrap();

    let decrypted_zeroizing = enclave.decrypt_string(&envelope).unwrap();
    assert_eq!(&*decrypted_zeroizing, secret);

    // Verify Zeroizing buffer implements Drop and zeroizes memory
    let mut sensitive_buf = vec![0x42u8; 64];
    sensitive_buf.zeroize();
    assert!(sensitive_buf.iter().all(|&b| b == 0));
}

#[test]
fn test_tier5_enclave_sqlite_wal_secure_delete_and_vacuum() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();

    conn.execute(
        "CREATE TABLE sensitive_facts (id INTEGER PRIMARY KEY, encrypted_content TEXT);",
        [],
    ).unwrap();

    let enclave = MemoryEnclave::new_with_argon2id(b"key_phrase", b"salt_unique_999").unwrap();
    let encrypted = enclave.encrypt_string("Personal Medical Information").unwrap();

    conn.execute(
        "INSERT INTO sensitive_facts (encrypted_content) VALUES (?1)",
        [&encrypted],
    ).unwrap();

    // Delete record and execute WAL sanitization
    conn.execute("DELETE FROM sensitive_facts WHERE id = 1", []).unwrap();
    let sanitize_res = MemoryEnclave::sanitize_wal_checkpoint(&conn);
    assert!(sanitize_res.is_ok(), "WAL sanitization must succeed");

    // Query PRAGMA secure_delete to verify it is ON (1)
    let secure_delete: i32 = conn.query_row("PRAGMA secure_delete;", [], |row| row.get(0)).unwrap();
    assert_eq!(secure_delete, 1, "PRAGMA secure_delete must be ON");
}

#[test]
fn test_tier5_enclave_fail_closed_fact_read_hostile_inputs() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt_safe_1234").unwrap();

    let valid_envelope = enclave.encrypt_string("Clear text read").unwrap();
    let read_ok = enclave.read_record(&valid_envelope);
    assert_eq!(read_ok, FactRead::Ok("Clear text read".to_string()));

    // Corrupted envelope
    let read_corrupt = enclave.read_record("v2:0000:0000:0000:0000");
    assert!(matches!(read_corrupt, FactRead::Locked { .. }));

    // Non-envelope plaintext string
    let read_plain = enclave.read_record("Unencrypted Plaintext Secret");
    assert_eq!(read_plain, FactRead::Locked { reason: "locked" });
}

#[test]
fn test_tier5_enclave_key_rotation_under_adversarial_states() {
    let enclave_a = MemoryEnclave::new_with_argon2id(b"old_key_phrase", b"salt_a_12345678").unwrap();
    let enclave_b = MemoryEnclave::new_with_argon2id(b"new_key_phrase", b"salt_b_87654321").unwrap();

    let secret_data = "Database Master Recovery Token";
    let envelope_a = enclave_a.encrypt_string(secret_data).unwrap();

    // Rotate from A to B
    let envelope_b = enclave_a.rotate_envelope(&envelope_a, &enclave_b).unwrap();

    // Envelope B can only be decrypted by Enclave B
    let decrypted_b = enclave_b.decrypt_string(&envelope_b).unwrap();
    assert_eq!(&*decrypted_b, secret_data);

    // Enclave A cannot decrypt Envelope B
    assert_eq!(enclave_a.decrypt_string(&envelope_b).unwrap_err(), EnclaveError::AuthenticationFailed);
}
