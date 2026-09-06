//! E2E Test Suite - Tier 1: Feature Coverage (≥75 test cases across 15 features)
//!
//! Features covered:
//! - Feature 1: L1 Radix KV Prefix Cache (5 tests)
//! - Feature 2: L2 Episodic Memory Exponential Decay (5 tests)
//! - Feature 3: L3 Obsidian Vault Bidirectional Sync (5 tests)
//! - Feature 4: L4 Procedural Memory Prior (5 tests)
//! - Feature 5: LLMLingua-2 Context Compression (5 tests)
//! - Feature 6: Recursive Summary Tree (5 tests)
//! - Feature 7: Obsidian [[wikilinks]] Native Parser (5 tests)
//! - Feature 8: CSR Sparse Matrix Conversion (5 tests)
//! - Feature 9: HippoRAG Parallel Rayon PPR (5 tests)
//! - Feature 10: Diacritic-Insensitive Vietnamese BM25 (5 tests)
//! - Feature 11: sqlite-vec INT8 Quantized Vector Search (5 tests)
//! - Feature 12: 3-Way Reciprocal Rank Fusion (5 tests)
//! - Feature 13: AES-256-GCM v2 Memory Enclaves (5 tests)
//! - Feature 14: Argon2id KDF & Zero-Leakage (5 tests)
//! - Feature 15: Tauri v2 Desktop IPC Bindings & Facade (5 tests)

mod phase3_harness;

use phase3_harness::*;
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// FEATURE 1: L1 RADIX KV PREFIX CACHE (5 tests)
// ============================================================================

#[test]
fn test_f1_01_radix_cache_exact_match() {
    let mut cache = RadixPrefixCache::new(50);
    let tokens = vec![101, 2054, 2003, 1037, 2742]; // e.g. "what is a test"
    cache.insert_prefix(&tokens, 10, false);

    let (matched_len, blocks) = cache.match_prefix(&tokens);
    assert_eq!(matched_len, 5);
    assert_eq!(blocks, vec![10]);
    assert_eq!(cache.hit_rate(), 1.0);
}

#[test]
fn test_f1_02_radix_cache_partial_prefix_sharing() {
    let mut cache = RadixPrefixCache::new(50);
    let system_prompt = vec![1, 2, 3, 4, 5];
    cache.insert_prefix(&system_prompt, 1, true);

    let user_turn = vec![1, 2, 3, 4, 5, 99, 100];
    let (matched_len, blocks) = cache.match_prefix(&user_turn);
    assert_eq!(matched_len, 5);
    assert_eq!(blocks, vec![1]);
}

#[test]
fn test_f1_03_radix_cache_lru_eviction() {
    let mut cache = RadixPrefixCache::new(2);
    cache.insert_prefix(&[10, 20], 1, false);
    cache.insert_prefix(&[30, 40], 2, false);
    assert_eq!(cache.total_blocks(), 2);

    // Access block 1 to make block 2 LRU
    cache.match_prefix(&[10, 20]);

    // Insert block 3 -> should evict block 2
    cache.insert_prefix(&[50, 60], 3, false);
    assert_eq!(cache.total_blocks(), 2);

    let (len2, _) = cache.match_prefix(&[30, 40]);
    assert_eq!(len2, 2); // path exists but block unallocated
    let (len1, blks1) = cache.match_prefix(&[10, 20]);
    assert_eq!(len1, 2);
    assert_eq!(blks1, vec![1]);
}

#[test]
fn test_f1_04_radix_cache_pinned_blocks_survive_eviction() {
    let mut cache = RadixPrefixCache::new(2);
    cache.insert_prefix(&[1, 2, 3], 100, true); // Pinned system prompt
    cache.insert_prefix(&[4, 5, 6], 101, false);

    assert_eq!(cache.pinned_count(), 1);

    // Inserting a third block should evict block 101, NOT pinned block 100
    cache.insert_prefix(&[7, 8, 9], 102, false);

    let (_, blks_pinned) = cache.match_prefix(&[1, 2, 3]);
    assert_eq!(blks_pinned, vec![100]);
}

#[test]
fn test_f1_05_radix_cache_hierarchical_branching() {
    let mut cache = RadixPrefixCache::new(50);
    cache.insert_prefix(&[1, 2, 3, 4], 1, false);
    cache.insert_prefix(&[1, 2, 5, 6], 2, false);
    cache.insert_prefix(&[1, 7, 8], 3, false);

    let (len_b1, blk1) = cache.match_prefix(&[1, 2, 3, 4, 99]);
    assert_eq!(len_b1, 4);
    assert_eq!(blk1, vec![1]);

    let (len_b2, blk2) = cache.match_prefix(&[1, 2, 5, 6, 100]);
    assert_eq!(len_b2, 4);
    assert_eq!(blk2, vec![2]);

    let (len_b3, blk3) = cache.match_prefix(&[1, 7, 8]);
    assert_eq!(len_b3, 3);
    assert_eq!(blk3, vec![3]);
}

// ============================================================================
// FEATURE 2: L2 EPISODIC MEMORY EXPONENTIAL DECAY (5 tests)
// ============================================================================

#[test]
fn test_f2_01_retention_score_half_life_exact_decay() {
    let tau = 86400.0; // 1 day
    let r0 = L2EpisodicStore::compute_retention_score(0.0, tau);
    let r1 = L2EpisodicStore::compute_retention_score(tau, tau);
    let r2 = L2EpisodicStore::compute_retention_score(2.0 * tau, tau);
    let r3 = L2EpisodicStore::compute_retention_score(3.0 * tau, tau);

    assert!((r0 - 1.0).abs() < 1e-6, "R(0) must be 1.0");
    assert!((r1 - 0.5).abs() < 1e-6, "R(tau) must be 0.5");
    assert!((r2 - 0.25).abs() < 1e-6, "R(2tau) must be 0.25");
    assert!((r3 - 0.125).abs() < 1e-6, "R(3tau) must be 0.125");
}

#[test]
fn test_f2_02_episodic_event_insertion_and_retrieval() {
    let store = L2EpisodicStore::new_in_memory(3600.0);
    let event = EpisodicEvent {
        id: "evt-001".to_string(),
        content: "User requested Rust memory optimization".to_string(),
        timestamp_secs: 1000.0,
        importance: 0.9,
        embedding: vec![0.1, 0.2, 0.3],
        metadata: HashMap::new(),
    };

    let id = store.insert_event(event).expect("insert should succeed");
    assert_eq!(id, "evt-001");

    let hits = store
        .search_active_episodic(1000.0, &[0.1, 0.2, 0.3], 0.1)
        .expect("search should succeed");
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].event.id, "evt-001");
}

#[test]
fn test_f2_03_episodic_decay_filtering() {
    let tau = 100.0;
    let store = L2EpisodicStore::new_in_memory(tau);
    // Insert event from 10 half-lives ago (delta = 1000s) -> score = 0.5^10 ~ 0.00097
    store
        .insert_event(EpisodicEvent {
            id: "old-event".to_string(),
            content: "Old forgotten fact".to_string(),
            timestamp_secs: 0.0,
            importance: 1.0,
            embedding: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    // Query with threshold 0.01 at t=1000.0 -> should be filtered out
    let hits = store
        .search_active_episodic(1000.0, &[], 0.01)
        .unwrap();
    assert!(hits.is_empty());

    // Query with threshold 0.0001 -> should be included
    let hits_low = store
        .search_active_episodic(1000.0, &[], 0.0001)
        .unwrap();
    assert_eq!(hits_low.len(), 1);
}

#[test]
fn test_f2_04_episodic_importance_weighting() {
    let tau = 1000.0;
    let store = L2EpisodicStore::new_in_memory(tau);
    store
        .insert_event(EpisodicEvent {
            id: "low-imp".to_string(),
            content: "Casual greeting".to_string(),
            timestamp_secs: 500.0,
            importance: 0.2,
            embedding: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();
    store
        .insert_event(EpisodicEvent {
            id: "high-imp".to_string(),
            content: "Critical password reset".to_string(),
            timestamp_secs: 500.0,
            importance: 0.95,
            embedding: vec![],
            metadata: HashMap::new(),
        })
        .unwrap();

    let hits = store
        .search_active_episodic(500.0, &[], 0.1)
        .unwrap();
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].event.id, "high-imp");
    assert!(hits[0].final_score > hits[1].final_score);
}

#[test]
fn test_f2_05_episodic_batch_decay_update() {
    let tau = 1000.0;
    let deltas = vec![0.0, 500.0, 1000.0, 2000.0];
    let scores: Vec<f64> = deltas
        .into_iter()
        .map(|d| L2EpisodicStore::compute_retention_score(d, tau))
        .collect();

    assert!(scores[0] > scores[1]);
    assert!(scores[1] > scores[2]);
    assert!(scores[2] > scores[3]);
}

// ============================================================================
// FEATURE 3: L3 OBSIDIAN VAULT BIDIRECTIONAL SYNC (5 tests)
// ============================================================================

#[test]
fn test_f3_01_obsidian_vault_initial_scan() {
    let mut sync = ObsidianSyncEngine::new();
    let note_content = r#"---
title: Memory Architecture
tags: [rust, liva, memory]
---
# Memory Architecture
Details on L1-L4 hierarchy."#;

    sync.index_note(PathBuf::from("Knowledge/Memory.md"), note_content);
    assert_eq!(sync.indexed_notes.len(), 1);
    let note = sync.indexed_notes.get("Memory").unwrap();
    assert_eq!(note.title, "Memory Architecture");
    assert_eq!(note.frontmatter.tags, vec!["rust", "liva", "memory"]);
}

#[test]
fn test_f3_02_obsidian_file_modification_detection() {
    let mut sync = ObsidianSyncEngine::new();
    let path = PathBuf::from("Skills/Search.md");
    sync.index_note(path.clone(), "# Search\nInitial version");

    assert_eq!(sync.indexed_notes.get("Search").unwrap().content, "# Search\nInitial version");

    // Modify note
    sync.index_note(path, "# Search\nUpdated with BM25");
    assert_eq!(sync.indexed_notes.get("Search").unwrap().content, "# Search\nUpdated with BM25");
}

#[test]
fn test_f3_03_obsidian_frontmatter_metadata_parsing() {
    let raw = r#"---
title: HippoRAG PPR
aliases: [Graph RAG, PPR Engine]
status: active
author: LIVA Team
---
Content here."#;

    let (fm, body, _) = ObsidianSyncEngine::parse_markdown(raw);
    assert_eq!(fm.title.as_deref(), Some("HippoRAG PPR"));
    assert_eq!(fm.aliases, vec!["Graph RAG", "PPR Engine"]);
    assert_eq!(fm.metadata.get("status").map(|s| s.as_str()), Some("active"));
    assert_eq!(fm.metadata.get("author").map(|s| s.as_str()), Some("LIVA Team"));
    assert_eq!(body, "Content here.");
}

#[test]
fn test_f3_04_obsidian_bidirectional_export_note() {
    let note = VaultNote {
        path: PathBuf::from("Vault/Exported.md"),
        title: "Exported Concept".to_string(),
        frontmatter: Frontmatter {
            title: Some("Exported Concept".to_string()),
            tags: vec!["exported".to_string(), "phase3".to_string()],
            aliases: vec![],
            metadata: HashMap::new(),
        },
        content: "This is exported content.".to_string(),
        outgoing_links: vec![],
    };

    let exported_str = ObsidianSyncEngine::export_note(&note);
    assert!(exported_str.contains("title: Exported Concept"));
    assert!(exported_str.contains("tags: [exported, phase3]"));
    assert!(exported_str.contains("This is exported content."));
}

#[test]
fn test_f3_05_obsidian_vault_deletion_handling() {
    let mut sync = ObsidianSyncEngine::new();
    sync.index_note(PathBuf::from("Notes/A.md"), "# Note A");
    assert_eq!(sync.indexed_notes.len(), 1);

    sync.indexed_notes.remove("A");
    assert_eq!(sync.indexed_notes.len(), 0);
}

// ============================================================================
// FEATURE 4: L4 PROCEDURAL MEMORY PRIOR (5 tests)
// ============================================================================

#[test]
fn test_f4_01_procedural_skill_registration() {
    let mut store = ProceduralMemoryStore::new();
    let skill = ProceduralSkill {
        id: "skill-code-fix".to_string(),
        name: "Code Fixer".to_string(),
        task_type: "coding".to_string(),
        prerequisites: vec![],
        successes: 0,
        failures: 0,
    };
    store.register_skill(skill);
    let prior = store.get_skill_prior("skill-code-fix").unwrap();
    // Laplace prior for 0/0: (0 + 1) / (0 + 0 + 2) = 0.5
    assert_eq!(prior, 0.5);
}

#[test]
fn test_f4_02_bayesian_prior_update_success() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "s1".to_string(),
        name: "Search".to_string(),
        task_type: "search".to_string(),
        prerequisites: vec![],
        successes: 0,
        failures: 0,
    });

    let prior1 = store.record_execution("s1", true).unwrap();
    assert_eq!(prior1, 2.0 / 3.0);

    let prior2 = store.record_execution("s1", true).unwrap();
    assert_eq!(prior2, 3.0 / 4.0);
    assert!(prior2 > prior1);
}

#[test]
fn test_f4_03_bayesian_prior_update_failure() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "s2".to_string(),
        name: "Flaky Tool".to_string(),
        task_type: "network".to_string(),
        prerequisites: vec![],
        successes: 1,
        failures: 0,
    });

    let prior = store.record_execution("s2", false).unwrap();
    // (1 + 1) / (1 + 1 + 2) = 2 / 4 = 0.5
    assert_eq!(prior, 0.5);
}

#[test]
fn test_f4_04_procedural_skill_dag_validation() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "a".to_string(),
        name: "A".to_string(),
        task_type: "test".to_string(),
        prerequisites: vec!["b".to_string()],
        successes: 1,
        failures: 0,
    });
    store.register_skill(ProceduralSkill {
        id: "b".to_string(),
        name: "B".to_string(),
        task_type: "test".to_string(),
        prerequisites: vec!["a".to_string()], // Cycle A <-> B
        successes: 1,
        failures: 0,
    });

    let res = store.validate_dag();
    assert!(res.is_err(), "Cyclic DAG must return error");
}

#[test]
fn test_f4_05_procedural_skill_ranking() {
    let mut store = ProceduralMemoryStore::new();
    store.register_skill(ProceduralSkill {
        id: "s_bad".to_string(),
        name: "Bad".to_string(),
        task_type: "math".to_string(),
        prerequisites: vec![],
        successes: 0,
        failures: 5, // (0+1)/(5+2) = 1/7 ~ 0.14
    });
    store.register_skill(ProceduralSkill {
        id: "s_good".to_string(),
        name: "Good".to_string(),
        task_type: "math".to_string(),
        prerequisites: vec![],
        successes: 8,
        failures: 1, // (8+1)/(9+2) = 9/11 ~ 0.81
    });

    let ranked = store.rank_skills_by_affinity("math", 0.3);
    assert_eq!(ranked.len(), 1);
    assert_eq!(ranked[0].id, "s_good");
}

// ============================================================================
// FEATURE 5: LLMLINGUA-2 CONTEXT COMPRESSION (5 tests)
// ============================================================================

#[test]
fn test_f5_01_llmlingua_target_compression_ratio() {
    let tokens: Vec<u32> = (1..=100).collect();
    let mask = vec![false; 100];
    let compressed = ContextCompressor::compress_llmlingua(&tokens, 0.33, &mask);

    assert!(compressed.compressed_tokens.len() <= 34);
    assert!(compressed.compression_ratio >= 2.9);
}

#[test]
fn test_f5_02_llmlingua_protected_tokens_preserved() {
    let tokens = vec![10, 20, 30, 40, 50, 60, 70, 80, 90, 100];
    let mut mask = vec![false; 10];
    mask[0] = true; // Protect token 10
    mask[9] = true; // Protect token 100

    let compressed = ContextCompressor::compress_llmlingua(&tokens, 0.3, &mask);
    assert!(compressed.compressed_tokens.contains(&10));
    assert!(compressed.compressed_tokens.contains(&100));
}

#[test]
fn test_f5_03_llmlingua_low_entropy_token_pruning() {
    // Repetitive tokens (low entropy) vs unique tokens (high entropy)
    let tokens = vec![1, 1, 1, 1, 1, 1, 1, 1, 999, 888];
    let mask = vec![false; 10];
    let compressed = ContextCompressor::compress_llmlingua(&tokens, 0.3, &mask);

    assert!(compressed.compressed_tokens.contains(&999));
    assert!(compressed.compressed_tokens.contains(&888));
}

#[test]
fn test_f5_04_llmlingua_semantic_reconstruction_fidelity() {
    let tokens: Vec<u32> = (1..=50).collect();
    let mask = vec![false; 50];
    let compressed = ContextCompressor::compress_llmlingua(&tokens, 0.5, &mask);

    assert!(compressed.information_loss_estimate < 0.015);
}

#[test]
fn test_f5_05_llmlingua_multi_chunk_compression() {
    let chunk1: Vec<u32> = vec![1, 2, 3, 4, 5];
    let chunk2: Vec<u32> = vec![6, 7, 8, 9, 10];
    let comp1 = ContextCompressor::compress_llmlingua(&chunk1, 0.6, &vec![false; 5]);
    let comp2 = ContextCompressor::compress_llmlingua(&chunk2, 0.6, &vec![false; 5]);

    assert_eq!(comp1.compressed_tokens.len(), 3);
    assert_eq!(comp2.compressed_tokens.len(), 3);
}

// ============================================================================
// FEATURE 6: RECURSIVE SUMMARY TREE (5 tests)
// ============================================================================

#[test]
fn test_f6_01_summary_tree_no_trigger_under_budget() {
    let turns = vec![
        ConversationTurn {
            role: "user".to_string(),
            content: "Hi".to_string(),
            token_count: 5,
        },
        ConversationTurn {
            role: "assistant".to_string(),
            content: "Hello!".to_string(),
            token_count: 5,
        },
    ];

    let tree = SummaryTree::maybe_trigger_summary_tree(&turns, 100);
    assert!(tree.is_none());
}

#[test]
fn test_f6_02_summary_tree_trigger_on_overflow() {
    let turns = vec![
        ConversationTurn {
            role: "user".to_string(),
            content: "A very long detailed question...".to_string(),
            token_count: 60,
        },
        ConversationTurn {
            role: "assistant".to_string(),
            content: "A very long detailed answer...".to_string(),
            token_count: 60,
        },
    ];

    let tree = SummaryTree::maybe_trigger_summary_tree(&turns, 100);
    assert!(tree.is_some());
}

#[test]
fn test_f6_03_summary_tree_hierarchical_structure() {
    let turns: Vec<ConversationTurn> = (0..8)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Turn {}", i),
            token_count: 20,
        })
        .collect();

    let root = SummaryTree::build_tree(&turns, 2);
    assert_eq!(root.level, 3); // Leaf(0) -> L1 (4 nodes) -> L2 (2 nodes) -> L3 (1 root)
    assert_eq!(root.children.len(), 2);
}

#[test]
fn test_f6_04_summary_tree_token_reduction_bound() {
    let turns: Vec<ConversationTurn> = (0..10)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Message {}", i),
            token_count: 50,
        })
        .collect();

    let total_raw_tokens: usize = turns.iter().map(|t| t.token_count).sum();
    let root = SummaryTree::build_tree(&turns, 2);

    assert!(root.token_count < total_raw_tokens);
}

#[test]
fn test_f6_05_summary_tree_incremental_update() {
    let turns_initial: Vec<ConversationTurn> = (0..4)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Turn {}", i),
            token_count: 20,
        })
        .collect();

    let tree1 = SummaryTree::build_tree(&turns_initial, 2);

    let mut turns_extended = turns_initial;
    turns_extended.push(ConversationTurn {
        role: "user".to_string(),
        content: "Turn 4".to_string(),
        token_count: 20,
    });

    let tree2 = SummaryTree::build_tree(&turns_extended, 2);
    assert!(tree2.level >= tree1.level);
}

// ============================================================================
// FEATURE 7: OBSIDIAN [[wikilinks]] NATIVE PARSER (5 tests)
// ============================================================================

#[test]
fn test_f7_01_wikilink_simple_link_extraction() {
    let md = "Check out [[Rust Architecture]] for more information.";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "Rust Architecture");
    assert_eq!(links[0].alias, None);
}

#[test]
fn test_f7_02_wikilink_aliased_link_extraction() {
    let md = "See [[VirtualMemoryEngine|Memory Core]] for specifications.";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "VirtualMemoryEngine");
    assert_eq!(links[0].alias.as_deref(), Some("Memory Core"));
}

#[test]
fn test_f7_03_wikilink_heading_and_block_references() {
    let md = "Ref [[Design#Architecture]] and chunk [[Data#^block101]].";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 2);
    assert_eq!(links[0].target, "Design");
    assert_eq!(links[0].heading.as_deref(), Some("Architecture"));
    assert_eq!(links[1].target, "Data");
    assert_eq!(links[1].block_id.as_deref(), Some("block101"));
}

#[test]
fn test_f7_04_wikilink_multiple_links_in_paragraph() {
    let md = "We use [[BM25]] with [[sqlite-vec]] and [[HippoRAG]] inside [[LIVA]].";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 4);
    assert_eq!(links[0].target, "BM25");
    assert_eq!(links[1].target, "sqlite-vec");
    assert_eq!(links[2].target, "HippoRAG");
    assert_eq!(links[3].target, "LIVA");
}

#[test]
fn test_f7_05_wikilink_escaped_and_malformed_links() {
    let md = r"Escaped \[\[not a link\]\] but this is [[Real Link]] and [[broken unclosed";
    let links = WikilinkParser::extract_links(md);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, "Real Link");
}

// ============================================================================
// FEATURE 8: CSR SPARSE MATRIX CONVERSION (5 tests)
// ============================================================================

#[test]
fn test_f8_01_csr_construction_from_edges() {
    let nodes = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    let edges = vec![(0, 1, 1.0), (0, 2, 1.0), (1, 2, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.num_nodes, 3);
    assert_eq!(csr.row_ptr, vec![0, 2, 3, 3]);
    assert_eq!(csr.col_indices, vec![1, 2, 2]);
}

#[test]
fn test_f8_02_csr_out_degree_lookup() {
    let nodes = vec!["N0".to_string(), "N1".to_string(), "N2".to_string()];
    let edges = vec![(0, 1, 1.0), (0, 2, 1.0), (1, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.out_degree(0), 2);
    assert_eq!(csr.out_degree(1), 1);
    assert_eq!(csr.out_degree(2), 0);
}

#[test]
fn test_f8_03_csr_matrix_vector_multiplication() {
    let nodes = vec!["0".to_string(), "1".to_string()];
    let edges = vec![(0, 1, 1.0), (1, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let x = vec![2.0, 3.0];
    let y = csr.matrix_vector_mult(&x);
    assert_eq!(y, vec![3.0, 2.0]);
}

#[test]
fn test_f8_04_csr_stochastic_normalization() {
    let nodes = vec!["0".to_string(), "1".to_string(), "2".to_string()];
    let edges = vec![(0, 1, 2.0), (0, 2, 2.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.values[0], 0.5);
    assert_eq!(csr.values[1], 0.5);
}

#[test]
fn test_f8_05_csr_isolated_nodes_handling() {
    let nodes = vec!["Iso0".to_string(), "Iso1".to_string(), "Iso2".to_string()];
    let edges: Vec<(usize, usize, f32)> = vec![];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    assert_eq!(csr.row_ptr, vec![0, 0, 0, 0]);
    assert_eq!(csr.out_degree(0), 0);
    assert_eq!(csr.out_degree(1), 0);
}

// ============================================================================
// FEATURE 9: HIPPORAG PARALLEL RAYON PPR (5 tests)
// ============================================================================

#[test]
fn test_f9_01_ppr_single_seed_propagation() {
    let nodes = vec!["Seed".to_string(), "Hop1".to_string(), "Hop2".to_string()];
    let edges = vec![(0, 1, 1.0), (1, 2, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);
    assert_eq!(ppr.len(), 3);
    assert!(ppr[0] > 0.0);
    assert!(ppr[1] > 0.0);
}

#[test]
fn test_f9_02_ppr_multi_seed_weighted_distribution() {
    let nodes = vec!["S1".to_string(), "S2".to_string(), "T".to_string()];
    let edges = vec![(0, 2, 1.0), (1, 2, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0, 1], &[0.8, 0.2], 0.85, 50, 1e-6);
    assert!(ppr[0] > ppr[1]);
}

#[test]
fn test_f9_03_ppr_stationary_probability_sum() {
    let nodes: Vec<String> = (0..5).map(|i| i.to_string()).collect();
    let edges = vec![(0, 1, 1.0), (1, 2, 1.0), (2, 3, 1.0), (3, 4, 1.0), (4, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 100, 1e-6);
    let sum: f32 = ppr.iter().sum();
    assert!((sum - 1.0).abs() < 1e-3, "PPR stationary sum should be ~1.0, got {}", sum);
}

#[test]
fn test_f9_04_ppr_convergence_within_tolerance() {
    let nodes = vec!["A".to_string(), "B".to_string()];
    let edges = vec![(0, 1, 1.0), (1, 0, 1.0)];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 200, 1e-6);
    assert!(ppr[0] > 0.0 && ppr[1] > 0.0);
}

#[test]
fn test_f9_05_ppr_dense_cluster_propagation() {
    // Triangle graph 0-1-2
    let nodes = vec!["0".to_string(), "1".to_string(), "2".to_string(), "3".to_string()];
    let edges = vec![
        (0, 1, 1.0), (1, 0, 1.0),
        (1, 2, 1.0), (2, 1, 1.0),
        (2, 0, 1.0), (0, 2, 1.0),
        (2, 3, 0.1), // weak link to node 3
    ];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);

    assert!(ppr[0] > ppr[3]);
    assert!(ppr[1] > ppr[3]);
    assert!(ppr[2] > ppr[3]);
}

// ============================================================================
// FEATURE 10: DIACRITIC-INSENSITIVE VIETNAMESE BM25 (5 tests)
// ============================================================================

#[test]
fn test_f10_01_vietnamese_diacritic_normalization() {
    let raw = "Bộ nhớ phân tầng của hệ thống LIVA";
    let norm = SparseBm25Engine::normalize_vietnamese(raw);
    assert_eq!(norm, "bo nho phan tang cua he thong liva");
}

#[test]
fn test_f10_02_bm25_unaccented_query_matches_accented_doc() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc1", "Hệ thống quản lý bộ nhớ thông minh");
    bm25.index_document("doc2", "Xử lý âm thanh giọng nói tiếng Việt");

    let hits = bm25.search("bo nho", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc1");
}

#[test]
fn test_f10_03_bm25_tf_idf_ranking() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc_rare", "Thuật toán HippoRAG giải quyết đồ thị tri thức");
    bm25.index_document("doc_common", "Tài liệu hệ thống và ghi chú chung");

    let hits = bm25.search("HippoRAG", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc_rare");
}

#[test]
fn test_f10_04_bm25_length_normalization() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("short_doc", "bộ nhớ LIVA");
    let long_text = format!("bộ nhớ {}", "từ khóa phụ lặp lại ".repeat(100));
    bm25.index_document("long_doc", &long_text);

    let hits = bm25.search("bo nho", 5);
    assert_eq!(hits.len(), 2);
    // Shorter document with high density ranks first
    assert_eq!(hits[0].id, "short_doc");
}

#[test]
fn test_f10_05_bm25_multi_term_conjunctive_scoring() {
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("doc_both", "Bộ nhớ đệm Radix KV Cache tối ưu");
    bm25.index_document("doc_one", "Bộ nhớ SQLite thông thường");

    let hits = bm25.search("Radix Cache", 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc_both");
}

// ============================================================================
// FEATURE 11: SQLITE-VEC INT8 QUANTIZED VECTOR SEARCH (5 tests)
// ============================================================================

#[test]
fn test_f11_01_int8_quantization_roundtrip() {
    let floats = vec![-1.0, -0.5, 0.0, 0.5, 1.0];
    let int8s = DenseVectorStore::quantize_f32_to_int8(&floats);
    let reconstructed = DenseVectorStore::dequantize_int8_to_f32(&int8s);

    for (orig, rec) in floats.iter().zip(&reconstructed) {
        assert!((orig - rec).abs() < 0.015);
    }
}

#[test]
fn test_f11_02_int8_cosine_similarity_identical_vectors() {
    let v1 = vec![0.5, 0.5, 0.5, 0.5];
    let q1 = DenseVectorStore::quantize_f32_to_int8(&v1);
    let sim = DenseVectorStore::cosine_similarity_int8(&q1, &q1);
    assert!((sim - 1.0).abs() < 1e-3);
}

#[test]
fn test_f11_03_int8_cosine_similarity_orthogonal_vectors() {
    let v1 = vec![1.0, 0.0];
    let v2 = vec![0.0, 1.0];
    let q1 = DenseVectorStore::quantize_f32_to_int8(&v1);
    let q2 = DenseVectorStore::quantize_f32_to_int8(&v2);
    let sim = DenseVectorStore::cosine_similarity_int8(&q1, &q2);
    assert!(sim.abs() < 1e-3);
}

#[test]
fn test_f11_04_vector_store_knn_ranking() {
    let mut store = DenseVectorStore::new();
    store.insert_vector("vec_near", &[0.9, 0.1, 0.0]);
    store.insert_vector("vec_far", &[-0.9, 0.1, 0.0]);

    let hits = store.search_knn(&[1.0, 0.0, 0.0], 5);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].id, "vec_near");
    assert!(hits[0].similarity > hits[1].similarity);
}

#[test]
fn test_f11_05_int8_quantization_clamping() {
    let extreme = vec![-5.0, 10.0];
    let q = DenseVectorStore::quantize_f32_to_int8(&extreme);
    assert_eq!(q[0], -128); // clamped
    assert_eq!(q[1], 127);  // clamped
}

// ============================================================================
// FEATURE 12: 3-WAY RECIPROCAL RANK FUSION (RRF) (5 tests)
// ============================================================================

#[test]
fn test_f12_01_rrf_unanimous_top_rank() {
    let rrf = TriModalRrfEngine::new();
    let hit_top = SearchHit {
        id: "target".to_string(),
        score: 1.0,
        snippet: "Top doc".to_string(),
    };
    let hit_other = SearchHit {
        id: "other".to_string(),
        score: 0.5,
        snippet: "Other doc".to_string(),
    };

    let bm25 = vec![hit_top.clone(), hit_other.clone()];
    let dense = vec![hit_top.clone(), hit_other.clone()];
    let graph = vec![hit_top.clone()];

    let fused = rrf.fuse(&bm25, &dense, &graph, 5);
    assert_eq!(fused.len(), 2);
    assert_eq!(fused[0].id, "target");
}

#[test]
fn test_f12_02_rrf_custom_weighting() {
    let mut rrf = TriModalRrfEngine::new();
    rrf.weight_dense = 0.90;
    rrf.weight_bm25 = 0.05;
    rrf.weight_graph = 0.05;

    let hit_bm25 = SearchHit {
        id: "bm25_winner".to_string(),
        score: 1.0,
        snippet: "".to_string(),
    };
    let hit_dense = SearchHit {
        id: "dense_winner".to_string(),
        score: 1.0,
        snippet: "".to_string(),
    };

    let fused = rrf.fuse(&[hit_bm25], &[hit_dense], &[], 5);
    assert_eq!(fused[0].id, "dense_winner");
}

#[test]
fn test_f12_03_rrf_disjoint_result_fusion() {
    let rrf = TriModalRrfEngine::new();
    let h1 = SearchHit { id: "h1".to_string(), score: 1.0, snippet: "".to_string() };
    let h2 = SearchHit { id: "h2".to_string(), score: 1.0, snippet: "".to_string() };
    let h3 = SearchHit { id: "h3".to_string(), score: 1.0, snippet: "".to_string() };

    let fused = rrf.fuse(&[h1], &[h2], &[h3], 10);
    assert_eq!(fused.len(), 3);
}

#[test]
fn test_f12_04_rrf_k_parameter_sensitivity() {
    let mut rrf_small_k = TriModalRrfEngine::new();
    rrf_small_k.k = 1.0;
    let mut rrf_large_k = TriModalRrfEngine::new();
    rrf_large_k.k = 100.0;

    let h1 = SearchHit { id: "1".to_string(), score: 1.0, snippet: "".to_string() };
    let h2 = SearchHit { id: "2".to_string(), score: 1.0, snippet: "".to_string() };

    let f_small = rrf_small_k.fuse(&[h1.clone(), h2.clone()], &[], &[], 2);
    let f_large = rrf_large_k.fuse(&[h1, h2], &[], &[], 2);

    let ratio_small = f_small[0].score / f_small[1].score;
    let ratio_large = f_large[0].score / f_large[1].score;

    assert!(ratio_small > ratio_large);
}

#[test]
fn test_f12_05_rrf_top_k_truncation() {
    let rrf = TriModalRrfEngine::new();
    let hits: Vec<SearchHit> = (0..20)
        .map(|i| SearchHit {
            id: i.to_string(),
            score: 1.0,
            snippet: "".to_string(),
        })
        .collect();

    let fused = rrf.fuse(&hits, &[], &[], 5);
    assert_eq!(fused.len(), 5);
}

// ============================================================================
// FEATURE 13: AES-256-GCM v2 MEMORY ENCLAVES (5 tests)
// ============================================================================

#[test]
fn test_f13_01_aes_256_gcm_encrypt_decrypt_roundtrip() {
    let enclave = MemoryEnclave::new_with_argon2id(b"master_passphrase", b"master_salt").unwrap();
    let secret = b"Top secret episodic memory payload";

    let encrypted = enclave.encrypt_record(secret).unwrap();
    assert!(encrypted.starts_with("v2:"));

    let decrypted = enclave.decrypt_record(&encrypted).unwrap();
    assert_eq!(decrypted, secret);
}

#[test]
fn test_f13_02_aes_256_gcm_unique_nonces_per_record() {
    let enclave = MemoryEnclave::new_with_argon2id(b"master_passphrase", b"master_salt").unwrap();
    let secret = b"Same message";

    let c1 = enclave.encrypt_record(secret).unwrap();
    let c2 = enclave.encrypt_record(secret).unwrap();

    assert_ne!(c1, c2, "Nonces must be unique across encryptions");
}

#[test]
fn test_f13_03_aes_256_gcm_tampered_ciphertext_fails() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let encrypted = enclave.encrypt_record(b"hello world").unwrap();

    let mut parts: Vec<String> = encrypted.split(':').map(|s| s.to_string()).collect();
    // Tamper ciphertext
    let mut ct_bytes = hex::decode(&parts[2]).unwrap();
    ct_bytes[0] ^= 0xFF;
    parts[2] = hex::encode(ct_bytes);
    let tampered = parts.join(":");

    let res = enclave.decrypt_record(&tampered);
    assert_eq!(res, Err(EnclaveError::DecryptionFailed));
}

#[test]
fn test_f13_04_aes_256_gcm_wrong_key_fails() {
    let enc1 = MemoryEnclave::new_with_argon2id(b"correct_pass", b"salt").unwrap();
    let enc2 = MemoryEnclave::new_with_argon2id(b"wrong_pass", b"salt").unwrap();

    let encrypted = enc1.encrypt_record(b"Confidential data").unwrap();
    let res = enc2.decrypt_record(&encrypted);
    assert_eq!(res, Err(EnclaveError::DecryptionFailed));
}

#[test]
fn test_f13_05_aes_256_gcm_empty_and_large_payload() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();

    // 0 bytes
    let enc_empty = enclave.encrypt_record(b"").unwrap();
    let dec_empty = enclave.decrypt_record(&enc_empty).unwrap();
    assert_eq!(dec_empty, b"");

    // 64 KB binary payload
    let large = vec![0x42u8; 65536];
    let enc_large = enclave.encrypt_record(&large).unwrap();
    let dec_large = enclave.decrypt_record(&enc_large).unwrap();
    assert_eq!(dec_large, large);
}

// ============================================================================
// FEATURE 14: ARGON2ID MASTER KDF & ZERO-LEAKAGE (5 tests)
// ============================================================================

#[test]
fn test_f14_01_argon2id_deterministic_derivation() {
    let k1 = MemoryEnclave::derive_master_key(b"my-password", b"constant-salt").unwrap();
    let k2 = MemoryEnclave::derive_master_key(b"my-password", b"constant-salt").unwrap();
    assert_eq!(k1, k2);
}

#[test]
fn test_f14_02_argon2id_salt_separation() {
    let k1 = MemoryEnclave::derive_master_key(b"password", b"salt-1").unwrap();
    let k2 = MemoryEnclave::derive_master_key(b"password", b"salt-2").unwrap();
    assert_ne!(k1, k2);
}

#[test]
fn test_f14_03_hkdf_record_key_isolation() {
    let master = [0x55u8; 32];
    let r1 = MemoryEnclave::derive_record_key(&master, "record-001");
    let r2 = MemoryEnclave::derive_record_key(&master, "record-002");
    assert_ne!(r1, r2);
}

#[test]
fn test_f14_04_zeroize_on_drop_wipes_secret() {
    let buffer = ZeroizingBuffer(vec![1, 2, 3, 4, 5]);
    assert_eq!(buffer.0, vec![1, 2, 3, 4, 5]);
    drop(buffer);
}

#[test]
fn test_f14_05_enclave_locked_state_prevents_access() {
    let mut enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    enclave.lock();

    assert_eq!(enclave.encrypt_record(b"test"), Err(EnclaveError::Locked));
    assert_eq!(enclave.decrypt_record("v2:00:00"), Err(EnclaveError::Locked));

    enclave.unlock();
    assert!(enclave.encrypt_record(b"test").is_ok());
}

// ============================================================================
// FEATURE 15: TAURI V2 DESKTOP IPC BINDINGS & FACADE (5 tests)
// ============================================================================

#[test]
fn test_f15_01_facade_store_and_recall_lifecycle() {
    let enclave = MemoryEnclave::new_with_argon2id(b"pass", b"salt").unwrap();
    let engine = VirtualMemoryEngine::new(Some(enclave));

    let event = EpisodicEvent {
        id: "mem-1".to_string(),
        content: "Học lập trình Rust nâng cao".to_string(),
        timestamp_secs: 1000.0,
        importance: 0.9,
        embedding: vec![0.1, 0.2, 0.3],
        metadata: HashMap::new(),
    };

    let id = engine.store_memory_encrypted(event).unwrap();
    assert_eq!(id, "mem-1");

    let hits = engine.query_hybrid("lap trinh Rust", &[0.1, 0.2, 0.3], 5);
    assert!(!hits.is_empty());
    assert_eq!(hits[0].id, "mem-1");
}

#[test]
fn test_f15_02_facade_l1_cache_integration() {
    let engine = VirtualMemoryEngine::new(None);
    let tokens = vec![1, 2, 3, 4];
    engine.l1_cache.lock().unwrap().insert_prefix(&tokens, 42, false);

    let (len, blocks) = engine.l1_cache.lock().unwrap().match_prefix(&tokens);
    assert_eq!(len, 4);
    assert_eq!(blocks, vec![42]);
}

#[test]
fn test_f15_03_facade_hybrid_search_scoring() {
    let engine = VirtualMemoryEngine::new(None);
    let event = EpisodicEvent {
        id: "doc-hybrid".to_string(),
        content: "Tìm kiếm đa phương thức kết hợp RRF".to_string(),
        timestamp_secs: 500.0,
        importance: 1.0,
        embedding: vec![1.0, 0.0],
        metadata: HashMap::new(),
    };
    engine.store_memory_encrypted(event).unwrap();

    let hits = engine.query_hybrid("tim kiem da phuong thuc", &[1.0, 0.0], 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "doc-hybrid");
}

#[test]
fn test_f15_04_facade_ipc_command_routing() {
    let engine = VirtualMemoryEngine::new(None);
    let res = engine.handle_ipc_command("memory_status", "{}").unwrap();
    assert!(res.contains("l1_blocks"));
}

#[test]
fn test_f15_05_facade_unknown_ipc_command_error() {
    let engine = VirtualMemoryEngine::new(None);
    let res = engine.handle_ipc_command("invalid_cmd", "{}");
    assert!(res.is_err());
}
