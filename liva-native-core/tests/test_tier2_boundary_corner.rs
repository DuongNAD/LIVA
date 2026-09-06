//! E2E Test Suite - Tier 2: Boundary Value Analysis & Edge Case Hardening (≥75 test cases across 15 features)
//!
//! Boundaries covered:
//! - Feature 1: L1 Radix KV Cache Boundaries (5 tests)
//! - Feature 2: L2 Episodic Memory Decay Boundaries (5 tests)
//! - Feature 3: L3 Obsidian Vault Sync Boundaries (5 tests)
//! - Feature 4: L4 Procedural Memory Prior Boundaries (5 tests)
//! - Feature 5: LLMLingua-2 Context Compression Boundaries (5 tests)
//! - Feature 6: Recursive Summary Tree Boundaries (5 tests)
//! - Feature 7: Obsidian [[wikilinks]] Parser Boundaries (5 tests)
//! - Feature 8: CSR Sparse Matrix Boundaries (5 tests)
//! - Feature 9: HippoRAG Parallel Rayon PPR Boundaries (5 tests)
//! - Feature 10: Diacritic-Insensitive BM25 Boundaries (5 tests)
//! - Feature 11: sqlite-vec INT8 Search Boundaries (5 tests)
//! - Feature 12: 3-Way Reciprocal Rank Fusion Boundaries (5 tests)
//! - Feature 13: AES-256-GCM v2 Memory Enclaves Boundaries (5 tests)
//! - Feature 14: Argon2id KDF & Zero-Leakage Boundaries (5 tests)
//! - Feature 15: Tauri v2 Desktop IPC Bindings Boundaries (5 tests)

mod phase3_harness;

use phase3_harness::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;

// ============================================================================
// FEATURE 1: L1 RADIX KV CACHE BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b1_01_radix_empty_tokens_query_and_insert() {
    let mut cache = RadixPrefixCache::new(10);
    cache.insert_prefix(&[], 1, false);
    assert_eq!(cache.total_blocks(), 0);

    let (len, blks) = cache.match_prefix(&[]);
    assert_eq!(len, 0);
    assert!(blks.is_empty());
}

#[test]
fn test_b1_02_radix_zero_capacity_eviction() {
    let mut cache = RadixPrefixCache::new(0);
    cache.insert_prefix(&[1, 2, 3], 1, false);
    // At 0 capacity, eviction triggers immediately on insertion
    assert_eq!(cache.total_blocks(), 0);
}

#[test]
fn test_b1_03_radix_single_token_trie_depth() {
    let mut cache = RadixPrefixCache::new(10);
    cache.insert_prefix(&[999], 1, false);
    let (len, blks) = cache.match_prefix(&[999]);
    assert_eq!(len, 1);
    assert_eq!(blks, vec![1]);
}

#[test]
fn test_b1_04_radix_all_pinned_eviction_starvation() {
    let mut cache = RadixPrefixCache::new(2);
    cache.insert_prefix(&[1, 2], 1, true); // Pinned
    cache.insert_prefix(&[3, 4], 2, true); // Pinned

    // All blocks pinned -> evict_lru returns None
    assert_eq!(cache.evict_lru(), None);
    assert_eq!(cache.pinned_count(), 2);
}

#[test]
fn test_b1_05_radix_large_token_sequence_depth() {
    let mut cache = RadixPrefixCache::new(10);
    let large_seq: Vec<u32> = (0..1000).collect();
    cache.insert_prefix(&large_seq, 99, false);

    let (len, blks) = cache.match_prefix(&large_seq);
    assert_eq!(len, 1000);
    assert_eq!(blks, vec![99]);
}

// ============================================================================
// FEATURE 2: L2 EPISODIC MEMORY DECAY BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b2_01_decay_negative_delta_time_clamped() {
    let r = L2EpisodicStore::compute_retention_score(-100.0, 86400.0);
    assert_eq!(r, 1.0, "Negative delta time must be clamped to 1.0");
}

#[test]
fn test_b2_02_decay_zero_half_life_guard() {
    let r = L2EpisodicStore::compute_retention_score(100.0, 0.0);
    assert_eq!(r, 0.0, "Zero half-life should return 0.0 without NaN/panic");
}

#[test]
fn test_b2_03_decay_near_infinite_delta_time() {
    let r = L2EpisodicStore::compute_retention_score(1e15, 86400.0);
    assert_eq!(r, 0.0);
}

#[test]
fn test_b2_04_episodic_empty_content_and_embedding() {
    let store = L2EpisodicStore::new_in_memory(1000.0);
    let event = EpisodicEvent {
        id: "empty_evt".to_string(),
        content: "".to_string(),
        timestamp_secs: 0.0,
        importance: 0.0,
        embedding: vec![],
        metadata: HashMap::new(),
    };
    assert!(store.insert_event(event).is_ok());
    let hits = store.search_active_episodic(0.0, &[], 0.0).unwrap();
    assert_eq!(hits.len(), 1);
}

#[test]
fn test_b2_05_episodic_retention_threshold_one_exact() {
    let store = L2EpisodicStore::new_in_memory(1000.0);
    store
        .insert_event(EpisodicEvent {
            id: "old".to_string(),
            content: "old".to_string(),
            timestamp_secs: 0.0,
            importance: 1.0,
            embedding: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Query at delta = 1.0 with threshold 1.0 -> should filter out (R < 1.0)
    let hits = store.search_active_episodic(1.0, &[], 1.0).unwrap();
    assert!(hits.is_empty());
}

// ============================================================================
// FEATURE 3: L3 OBSIDIAN VAULT SYNC BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b3_01_obsidian_empty_markdown_file() {
    let (fm, body, links) = ObsidianSyncEngine::parse_markdown("");
    assert!(fm.title.is_none());
    assert!(body.is_empty());
    assert!(links.is_empty());
}

#[test]
fn test_b3_02_obsidian_malformed_yaml_frontmatter_resilience() {
    let malformed = "---\ntitle: unclosed frontmatter without trailing three hyphens\ncontent";
    let (fm, body, _) = ObsidianSyncEngine::parse_markdown(malformed);
    // Should fallback cleanly treating entire text as body
    assert!(fm.title.is_none());
    assert_eq!(body, malformed);
}

#[test]
fn test_b3_03_obsidian_non_ascii_unicode_filepaths() {
    let mut sync = ObsidianSyncEngine::new();
    let content = "# Tiêu đề tiếng Việt\nNội dung ghi chú.";
    sync.index_note(PathBuf::from("Kỹ năng/Bộ nhớ LIVA.md"), content);

    assert!(sync.indexed_notes.contains_key("Bộ nhớ LIVA"));
}

#[test]
fn test_b3_04_obsidian_deep_nested_vault_hierarchy() {
    let mut sync = ObsidianSyncEngine::new();
    let path = PathBuf::from("Level1/Level2/Level3/Level4/DeepNote.md");
    sync.index_note(path, "# Deep Note");

    assert!(sync.indexed_notes.contains_key("DeepNote"));
}

#[test]
fn test_b3_05_obsidian_duplicate_title_collision() {
    let mut sync = ObsidianSyncEngine::new();
    sync.index_note(PathBuf::from("DirA/Note.md"), "Version A");
    sync.index_note(PathBuf::from("DirB/Note.md"), "Version B");

    // Last write wins in title index
    assert_eq!(sync.indexed_notes.len(), 1);
    assert_eq!(sync.indexed_notes.get("Note").unwrap().content, "Version B");
}

// ============================================================================
// FEATURE 4: L4 PROCEDURAL MEMORY PRIOR BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b4_01_procedural_self_referencing_prerequisite() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "self_ref".to_string(),
        name: "Self Loop".to_string(),
        task_type: "misc".to_string(),
        prerequisites: vec!["self_ref".to_string()],
        successes: 0,
        failures: 0,
    });

    assert!(store.validate_dag().is_err());
}

#[test]
fn test_b4_02_procedural_unregistered_prerequisite_error() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "child".to_string(),
        name: "Child".to_string(),
        task_type: "misc".to_string(),
        prerequisites: vec!["missing_parent".to_string()],
        successes: 0,
        failures: 0,
    });

    let res = store.validate_dag();
    assert!(res.is_err());
    assert!(res.unwrap_err().contains("Unregistered prerequisite"));
}

#[test]
fn test_b4_03_procedural_extreme_execution_count_stability() {
    let prior = ProceduralMemoryStore::calculate_bayesian_prior(1_000_000, 0);
    assert!(!prior.is_nan());
    assert!(!prior.is_infinite());
    assert!(prior > 0.99999);
}

#[test]
fn test_b4_04_procedural_unseen_task_type_affinity() {
    let store = ProceduralMemoryStore::new();
    let ranked = store.rank_skills_by_affinity("non_existent_domain", 0.0);
    assert!(ranked.is_empty());
}

#[test]
fn test_b4_05_procedural_disconnected_subgraphs_dag_valid() {
    let mut store = ProceduralMemoryStore::new();
    // Subgraph 1: A -> B
    store.register_skill(ProceduralSkill {
        id: "b".to_string(),
        name: "B".to_string(),
        task_type: "t".to_string(),
        prerequisites: vec![],
        successes: 1,
        failures: 0,
    });
    store.register_skill(ProceduralSkill {
        id: "a".to_string(),
        name: "A".to_string(),
        task_type: "t".to_string(),
        prerequisites: vec!["b".to_string()],
        successes: 1,
        failures: 0,
    });
    // Subgraph 2: X -> Y
    store.register_skill(ProceduralSkill {
        id: "y".to_string(),
        name: "Y".to_string(),
        task_type: "t".to_string(),
        prerequisites: vec![],
        successes: 1,
        failures: 0,
    });
    store.register_skill(ProceduralSkill {
        id: "x".to_string(),
        name: "X".to_string(),
        task_type: "t".to_string(),
        prerequisites: vec!["y".to_string()],
        successes: 1,
        failures: 0,
    });

    assert!(store.validate_dag().is_ok());
}

// ============================================================================
// FEATURE 5: LLMLINGUA-2 CONTEXT COMPRESSION BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b5_01_llmlingua_empty_token_stream() {
    let comp = ContextCompressor::compress_llmlingua(&[], 0.5, &[]);
    assert_eq!(comp.original_count, 0);
    assert!(comp.compressed_tokens.is_empty());
}

#[test]
fn test_b5_02_llmlingua_target_ratio_zero() {
    let tokens = vec![1, 2, 3, 4, 5];
    let comp = ContextCompressor::compress_llmlingua(&tokens, 0.0, &vec![false; 5]);
    // Should retain at least 1 token
    assert_eq!(comp.compressed_tokens.len(), 1);
}

#[test]
fn test_b5_03_llmlingua_target_ratio_one_or_greater() {
    let tokens = vec![1, 2, 3, 4, 5];
    let comp = ContextCompressor::compress_llmlingua(&tokens, 1.5, &vec![false; 5]);
    assert_eq!(comp.compressed_tokens.len(), 5);
}

#[test]
fn test_b5_04_llmlingua_all_tokens_protected_mask() {
    let tokens = vec![1, 2, 3, 4, 5];
    let all_prot = vec![true; 5];
    let comp = ContextCompressor::compress_llmlingua(&tokens, 0.2, &all_prot);
    // When all are protected, takes target_len top protected tokens preserving order
    assert!(!comp.compressed_tokens.is_empty());
}

#[test]
fn test_b5_05_llmlingua_monolithic_identical_tokens() {
    let tokens = vec![42; 20];
    let comp = ContextCompressor::compress_llmlingua(&tokens, 0.5, &vec![false; 20]);
    assert_eq!(comp.compressed_tokens.len(), 10);
    for &t in &comp.compressed_tokens {
        assert_eq!(t, 42);
    }
}

// ============================================================================
// FEATURE 6: RECURSIVE SUMMARY TREE BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b6_01_summary_tree_empty_turn_list() {
    let root = SummaryTree::build_tree(&[], 2);
    assert_eq!(root.id, "root_empty");
    assert_eq!(root.token_count, 0);
}

#[test]
fn test_b6_02_summary_tree_single_massive_turn_overflow() {
    let turns = vec![ConversationTurn {
        role: "user".to_string(),
        content: "Huge payload...".to_string(),
        token_count: 5000,
    }];
    let tree = SummaryTree::maybe_trigger_summary_tree(&turns, 1000);
    assert!(tree.is_some());
}

#[test]
fn test_b6_03_summary_tree_odd_chunk_partitioning() {
    let turns: Vec<ConversationTurn> = (0..3)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Turn {}", i),
            token_count: 10,
        })
        .collect();

    let root = SummaryTree::build_tree(&turns, 2);
    assert_eq!(root.level, 2); // 3 leaves -> 2 nodes -> 1 root
}

#[test]
fn test_b6_04_summary_tree_zero_token_turns() {
    let turns = vec![
        ConversationTurn { role: "user".to_string(), content: "".to_string(), token_count: 0 },
        ConversationTurn { role: "assistant".to_string(), content: "".to_string(), token_count: 0 },
    ];
    let root = SummaryTree::build_tree(&turns, 2);
    assert_eq!(root.children.len(), 2);
}

#[test]
fn test_b6_05_summary_tree_extreme_depth_scaling() {
    let turns: Vec<ConversationTurn> = (0..32)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Turn {}", i),
            token_count: 10,
        })
        .collect();

    let root = SummaryTree::build_tree(&turns, 2);
    // 32 -> 16 -> 8 -> 4 -> 2 -> 1 (5 summary levels)
    assert_eq!(root.level, 5);
}

// ============================================================================
// FEATURE 7: OBSIDIAN [[wikilinks]] PARSER BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b7_01_wikilink_empty_brackets() {
    let md = "[[]] and [[   ]]";
    let links = WikilinkParser::extract_links(md);
    assert!(links.is_empty());
}

#[test]
fn test_b7_02_wikilink_nested_brackets() {
    let md = "[[Outer [[Inner]]]]";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "Inner");
}

#[test]
fn test_b7_03_wikilink_empty_alias_or_heading() {
    let md = "[[Note|]] and [[Heading#]]";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target, "Note");
    assert_eq!(links[0].alias, None);
    assert_eq!(links[1].target, "Heading");
    assert_eq!(links[1].heading, None);
}

#[test]
fn test_b7_04_wikilink_multiline_rejection() {
    let md = "[[First Line\nSecond Line]]";
    let links = WikilinkParser::extract_links(md);
    assert!(links.is_empty(), "Multiline wikilinks should be rejected");
}

#[test]
fn test_b7_05_wikilink_massive_density() {
    let links_text = (0..500)
        .map(|i| format!("[[Target_{}]]", i))
        .collect::<Vec<_>>()
        .join(" ");

    let extracted = WikilinkParser::extract_links(&links_text);
    assert_eq!(extracted.len(), 500);
}

// ============================================================================
// FEATURE 8: CSR SPARSE MATRIX BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b8_01_csr_empty_graph_zero_nodes() {
    let csr = CsrGraph::from_nodes_and_edges(&[], &[]);
    assert_eq!(csr.num_nodes, 0);
    assert_eq!(csr.row_ptr, vec![0]);
    assert!(csr.col_indices.is_empty());
}

#[test]
fn test_b8_02_csr_single_node_no_edges() {
    let csr = CsrGraph::from_nodes_and_edges(&["Alone".to_string()], &[]);
    assert_eq!(csr.num_nodes, 1);
    assert_eq!(csr.row_ptr, vec![0, 0]);
    assert_eq!(csr.out_degree(0), 0);
}

#[test]
fn test_b8_03_csr_out_of_bounds_edge_filtered() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let edges = vec![(0, 1, 1.0), (0, 999, 1.0)]; // 999 is invalid node
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.col_indices.len(), 1);
    assert_eq!(csr.col_indices[0], 1);
}

#[test]
fn test_b8_04_csr_self_loop_edge() {
    let nodes = vec!["A".to_string()];
    let edges = vec![(0, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.out_degree(0), 1);
    assert_eq!(csr.col_indices[0], 0);
}

#[test]
fn test_b8_05_csr_fully_connected_clique() {
    let n = 4;
    let nodes: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    let mut edges = Vec::new();
    for i in 0..n {
        for j in 0..n {
            if i != j {
                edges.push((i, j, 1.0));
            }
        }
    }

    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);
    for i in 0..n {
        assert_eq!(csr.out_degree(i), n - 1);
    }
}

// ============================================================================
// FEATURE 9: HIPPORAG PPR BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b9_01_ppr_empty_seed_list_fallback() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let edges = vec![(0, 1, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[], &[], 0.85, 20, 1e-6);
    assert_eq!(ppr.len(), 2);
    assert!(ppr[0] > 0.0 && ppr[1] > 0.0);
}

#[test]
fn test_b9_02_ppr_invalid_seed_index_out_of_bounds() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &[]);

    // Seed index 99 is out of bounds
    let ppr = csr.personalized_pagerank(&[99], &[1.0], 0.85, 20, 1e-6);
    assert_eq!(ppr.len(), 2);
}

#[test]
fn test_b9_03_ppr_damping_factor_zero() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let edges = vec![(0, 1, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    // Damping factor 0.0 means 0 random walk transition, pure seed weight
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.0, 10, 1e-6);
    assert_eq!(ppr[0], 1.0);
    assert_eq!(ppr[1], 0.0);
}

#[test]
fn test_b9_04_ppr_damping_factor_near_one() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let edges = vec![(0, 1, 1.0), (1, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.999, 100, 1e-6);
    assert_eq!(ppr.len(), 2);
}

#[test]
fn test_b9_05_ppr_disconnected_components_seed_isolation() {
    let nodes = vec!["A1".to_string(), "A2".to_string(), "B1".to_string(), "B2".to_string()];
    let edges = vec![(0, 1, 1.0), (1, 0, 1.0), (2, 3, 1.0), (3, 2, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);
    // Nodes in component B should have ~0 score
    assert!(ppr[0] > 0.3);
    assert!(ppr[1] > 0.3);
    assert!(ppr[2] < 1e-4);
    assert!(ppr[3] < 1e-4);
}

// ============================================================================
// FEATURE 10: DIACRITIC-INSENSITIVE BM25 BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b10_01_bm25_empty_query_string() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc1", "test document");
    let hits = bm25.search("", 10);
    assert!(hits.is_empty());
}

#[test]
fn test_b10_02_bm25_all_punctuation_query() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc1", "test document");
    let hits = bm25.search("!@#$%^&*()", 10);
    assert!(hits.is_empty());
}

#[test]
fn test_b10_03_bm25_empty_document_index() {
    let bm25 = SparseBm25Engine::new();
    let hits = bm25.search("hello", 10);
    assert!(hits.is_empty());
}

#[test]
fn test_b10_04_bm25_all_unseen_vocabulary_terms() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc1", "tiếng Việt chuẩn");
    let hits = bm25.search("completely_unseen_word_12345", 10);
    assert!(hits.is_empty());
}

#[test]
fn test_b10_05_bm25_extreme_repetition_tf_saturation() {
    let mut bm25 = SparseBm25Engine::new();
    let repeated = "liva ".repeat(500);
    bm25.index_document("doc_rep", &repeated);
    let hits = bm25.search("liva", 5);

    assert_eq!(hits.len(), 1);
    assert!(hits[0].score > 0.0);
}

// ============================================================================
// FEATURE 11: SQLITE-VEC INT8 SEARCH BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b11_01_int8_all_zero_vectors_similarity() {
    let z1 = vec![0i8, 0i8, 0i8];
    let z2 = vec![0i8, 0i8, 0i8];
    let sim = DenseVectorStore::cosine_similarity_int8(&z1, &z2);
    assert_eq!(sim, 0.0);
}

#[test]
fn test_b11_02_int8_nan_and_infinity_in_vector() {
    let f = vec![f32::NAN, f32::INFINITY, -f32::INFINITY];
    let q = DenseVectorStore::quantize_f32_to_int8(&f);
    assert_eq!(q[0], 0);
    assert_eq!(q[1], 127);
    assert_eq!(q[2], -128);
}

#[test]
fn test_b11_03_int8_dimension_mismatch_guard() {
    let a = vec![1i8, 2i8];
    let b = vec![1i8, 2i8, 3i8];
    let sim = DenseVectorStore::cosine_similarity_int8(&a, &b);
    assert_eq!(sim, 0.0);
}

#[test]
fn test_b11_04_int8_maximum_dynamic_range() {
    let v1 = vec![127i8, 127i8];
    let v2 = vec![-128i8, -128i8];
    let sim = DenseVectorStore::cosine_similarity_int8(&v1, &v2);
    assert!(sim < -0.99);
}

#[test]
fn test_b11_05_int8_empty_vector_similarity() {
    let sim = DenseVectorStore::cosine_similarity_int8(&[], &[]);
    assert_eq!(sim, 0.0);
}

// ============================================================================
// FEATURE 12: 3-WAY RECIPROCAL RANK FUSION BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b12_01_rrf_all_modality_lists_empty() {
    let rrf = TriModalRrfEngine::new();
    let res = rrf.fuse(&[], &[], &[], 10);
    assert!(res.is_empty());
}

#[test]
fn test_b12_02_rrf_top_k_zero() {
    let rrf = TriModalRrfEngine::new();
    let hit = SearchHit { id: "1".to_string(), score: 1.0, snippet: "".to_string() };
    let res = rrf.fuse(&[hit], &[], &[], 0);
    assert!(res.is_empty());
}

#[test]
fn test_b12_03_rrf_all_weights_zero() {
    let mut rrf = TriModalRrfEngine::new();
    rrf.weight_bm25 = 0.0;
    rrf.weight_dense = 0.0;
    rrf.weight_graph = 0.0;

    let hit = SearchHit { id: "1".to_string(), score: 1.0, snippet: "".to_string() };
    let res = rrf.fuse(&[hit], &[], &[], 5);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].score, 0.0);
}

#[test]
fn test_b12_04_rrf_single_modality_populated() {
    let rrf = TriModalRrfEngine::new();
    let hit = SearchHit { id: "doc1".to_string(), score: 1.0, snippet: "".to_string() };
    let res = rrf.fuse(&[hit], &[], &[], 5);
    assert_eq!(res.len(), 1);
    assert_eq!(res[0].id, "doc1");
}

#[test]
fn test_b12_05_rrf_massive_candidate_list_top_k() {
    let rrf = TriModalRrfEngine::new();
    let hits: Vec<SearchHit> = (0..5000)
        .map(|i| SearchHit {
            id: format!("doc_{}", i),
            score: 1.0,
            snippet: "".to_string(),
        })
        .collect();

    let res = rrf.fuse(&hits, &[], &[], 10);
    assert_eq!(res.len(), 10);
}

// ============================================================================
// FEATURE 13: AES-256-GCM v2 MEMORY ENCLAVES BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b13_01_enclave_malformed_envelope_missing_parts() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let res = enclave.decrypt_record("v2:only_one_part");
    assert_eq!(res, Err(EnclaveError::MalformedEnvelope));
}

#[test]
fn test_b13_02_enclave_truncated_nonce_hex() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    // Nonce must be 12 bytes = 24 hex chars. Here it is 4 hex chars = 2 bytes
    let res = enclave.decrypt_record("v2:1234:5678abcd");
    assert_eq!(res, Err(EnclaveError::MalformedEnvelope));
}

#[test]
fn test_b13_03_enclave_corrupt_hex_characters() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let res = enclave.decrypt_record("v2:zzzzzzzzzzzzzzzzzzzzzzzz:abcdef");
    assert_eq!(res, Err(EnclaveError::MalformedEnvelope));
}

#[test]
fn test_b13_04_enclave_truncated_auth_tag() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let encrypted = enclave.encrypt_record(b"hello").unwrap();
    let truncated = &encrypted[..encrypted.len() - 4];
    let res = enclave.decrypt_record(truncated);
    assert!(res.is_err());
}

#[test]
fn test_b13_05_enclave_unsupported_version_prefix() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let res = enclave.decrypt_record("v1:00112233445566778899aabb:ccddeeff");
    assert_eq!(res, Err(EnclaveError::MalformedEnvelope));
}

// ============================================================================
// FEATURE 14: ARGON2ID KDF & ZERO-LEAKAGE BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b14_01_argon2id_empty_passphrase_and_salt() {
    let key = MemoryEnclave::derive_master_key(b"", b"").unwrap();
    assert_eq!(key.len(), 32);
}

#[test]
fn test_b14_02_argon2id_huge_passphrase() {
    let huge_pass = vec![0x41u8; 65536];
    let key = MemoryEnclave::derive_master_key(&huge_pass, b"salt").unwrap();
    assert_eq!(key.len(), 32);
}

#[test]
fn test_b14_03_hkdf_empty_record_id() {
    let master = [0u8; 32];
    let rec_key = MemoryEnclave::derive_record_key(&master, "");
    assert_eq!(rec_key.len(), 32);
}

#[test]
fn test_b14_04_zeroizing_buffer_reallocation() {
    let mut buf = ZeroizingBuffer(Vec::new());
    buf.0.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8]);
    assert_eq!(buf.0.len(), 8);
    drop(buf);
}

#[test]
fn test_b14_05_enclave_multiple_lock_unlock_cycles() {
    let mut enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    for _ in 0..5 {
        enclave.lock();
        assert_eq!(enclave.encrypt_record(b"x"), Err(EnclaveError::Locked));
        enclave.unlock();
        assert!(enclave.encrypt_record(b"x").is_ok());
    }
}

// ============================================================================
// FEATURE 15: TAURI V2 DESKTOP IPC BINDINGS BOUNDARIES (5 tests)
// ============================================================================

#[test]
fn test_b15_01_ipc_empty_payload() {
    let engine = VirtualMemoryEngine::new(None);
    let res = engine.handle_ipc_command("memory_recall", "");
    assert!(res.is_ok());
}

#[test]
fn test_b15_02_ipc_invalid_json_payload() {
    let engine = VirtualMemoryEngine::new(None);
    let res = engine.handle_ipc_command("memory_status", "{invalid json");
    assert!(res.is_ok());
}

#[test]
fn test_b15_03_ipc_extremely_large_payload() {
    let engine = VirtualMemoryEngine::new(None);
    let large_query = "a ".repeat(50_000);
    let res = engine.handle_ipc_command("memory_recall", &large_query);
    assert!(res.is_ok());
}

#[test]
fn test_b15_04_ipc_rapid_fire_concurrent_requests() {
    let engine = Arc::new(VirtualMemoryEngine::new(None));
    let mut handles = Vec::new();

    for i in 0..10 {
        let eng = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            let res = eng.handle_ipc_command("memory_status", "{}");
            assert!(res.is_ok());
            let q_res = eng.handle_ipc_command("memory_recall", &format!("query {}", i));
            assert!(q_res.is_ok());
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_b15_05_ipc_null_characters_in_command_or_payload() {
    let engine = VirtualMemoryEngine::new(None);
    let res = engine.handle_ipc_command("memory_recall\0injected", "payload\0with\0nulls");
    // Unknown command due to null injection
    assert!(res.is_err());
}
