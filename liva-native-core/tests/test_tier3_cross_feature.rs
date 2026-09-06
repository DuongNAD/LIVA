//! E2E Test Suite - Tier 3: Cross-Feature Pairwise Interactions (≥15 test cases)
//!
//! Pairwise feature interactions:
//! - Pair 1: L1 Radix Cache + LLMLingua-2 Token Compression
//! - Pair 2: L2 Episodic Decay + AES-256-GCM v2 Memory Enclave
//! - Pair 3: L3 Obsidian Vault Sync + [[wikilinks]] Native Parser
//! - Pair 4: [[wikilinks]] Parser + CSR Sparse Matrix Conversion
//! - Pair 5: CSR Sparse Matrix + HippoRAG Parallel Rayon PPR
//! - Pair 6: HippoRAG PPR + 3-Way Reciprocal Rank Fusion (RRF)
//! - Pair 7: Diacritic-Insensitive BM25 + sqlite-vec INT8 Vector Search
//! - Pair 8: 3-Way RRF Hybrid Search + L2 Episodic Exponential Decay
//! - Pair 9: L4 Procedural Memory Prior + Tauri v2 Desktop IPC
//! - Pair 10: Recursive Summary Tree + L1 Radix Prefix Cache
//! - Pair 11: AES-256-GCM Enclave + Argon2id KDF & Zero-Leakage
//! - Pair 12: Obsidian Vault Sync + CSR Graph + HippoRAG PPR Re-ranking
//! - Pair 13: LLMLingua-2 Pruning + Recursive Summary Tree Compaction
//! - Pair 14: sqlite-vec INT8 Search + L3 Obsidian Semantic Notes
//! - Pair 15: Unified VirtualMemoryEngine Facade Full 4-Tier Pipeline

mod phase3_harness;

use phase3_harness::*;
use std::collections::HashMap;
use std::path::PathBuf;

// ============================================================================
// PAIR 1: L1 RADIX CACHE + LLMLINGUA-2 COMPRESSION
// ============================================================================

#[test]
fn test_p1_radix_cache_with_llmlingua_compression() {
    let mut cache = RadixPrefixCache::new(50);
    let raw_system_prompt: Vec<u32> = (1..=100).collect();
    let protected_mask = vec![true; 100]; // Protect key directives

    // Compress raw prompt from 100 to ~33 tokens
    let compressed = ContextCompressor::compress_llmlingua(&raw_system_prompt, 0.33, &protected_mask);
    assert!(compressed.compressed_tokens.len() <= 34);

    // Cache the compressed prefix in L1 Radix Cache
    cache.insert_prefix(&compressed.compressed_tokens, 1, true);

    // Incoming user query sharing compressed prefix
    let mut session_tokens = compressed.compressed_tokens.clone();
    session_tokens.extend_from_slice(&[999, 1000]);

    let (matched_len, block_ids) = cache.match_prefix(&session_tokens);
    assert_eq!(matched_len, compressed.compressed_tokens.len());
    assert_eq!(block_ids, vec![1]);
}

// ============================================================================
// PAIR 2: L2 EPISODIC DECAY + AES-256-GCM ENCLAVE
// ============================================================================

#[test]
fn test_p2_episodic_decay_with_aes_enclave() {
    let enclave = MemoryEnclave::new_with_argon2id(b"user_pass", b"salt123").unwrap();
    let store = L2EpisodicStore::new_in_memory(3600.0); // 1 hour half-life

    let raw_secret = "Private personal reflection at 10:00 AM";
    let encrypted = enclave.encrypt_record(raw_secret.as_bytes()).unwrap();

    let event = EpisodicEvent {
        id: "mem-secret-01".to_string(),
        content: encrypted,
        timestamp_secs: 1000.0,
        importance: 0.9,
        embedding: vec![0.5, 0.5],
        metadata: HashMap::new(),
    };
    store.insert_event(event).unwrap();

    // Query 1 hour later (delta = 3600.0s) -> retention score = 0.5
    let hits = store.search_active_episodic(4600.0, &[0.5, 0.5], 0.1).unwrap();
    assert_eq!(hits.len(), 1);

    // Decrypt the recalled memory
    let decrypted_bytes = enclave.decrypt_record(&hits[0].event.content).unwrap();
    let decrypted_text = String::from_utf8(decrypted_bytes).unwrap();
    assert_eq!(decrypted_text, raw_secret);
    assert!((hits[0].retention_score - 0.5).abs() < 1e-4);
}

// ============================================================================
// PAIR 3: L3 OBSIDIAN SYNC + WIKILINK PARSER
// ============================================================================

#[test]
fn test_p3_obsidian_sync_with_wikilink_parser() {
    let mut sync = ObsidianSyncEngine::new();
    let note1 = r#"---
title: Cognitive OS
tags: [architecture, liva]
---
LIVA uses [[Radix Cache]] for L1 and [[HippoRAG]] for Graph RAG."#;

    let note2 = r#"---
title: HippoRAG
tags: [graph, ppr]
---
Implements [[Personalized PageRank]] on [[CSR Matrix]]."#;

    sync.index_note(PathBuf::from("Knowledge/Cognitive_OS.md"), note1);
    sync.index_note(PathBuf::from("Knowledge/HippoRAG.md"), note2);

    assert_eq!(sync.indexed_notes.len(), 2);
    let cog_note = sync.indexed_notes.get("Cognitive_OS").unwrap();
    assert_eq!(cog_note.outgoing_links.len(), 2);
    assert_eq!(cog_note.outgoing_links[0].target, "Radix Cache");
    assert_eq!(cog_note.outgoing_links[1].target, "HippoRAG");
}

// ============================================================================
// PAIR 4: WIKILINK PARSER + CSR SPARSE MATRIX
// ============================================================================

#[test]
fn test_p4_wikilink_parser_with_csr_sparse_matrix() {
    let md_notes = vec![
        ("NodeA", "Links to [[NodeB]] and [[NodeC]]"),
        ("NodeB", "Links to [[NodeC]]"),
        ("NodeC", "Dead end leaf node"),
    ];

    let mut title_to_idx = HashMap::new();
    let mut titles = Vec::new();
    for (i, &(title, _)) in md_notes.iter().enumerate() {
        title_to_idx.insert(title.to_string(), i);
        titles.push(title.to_string());
    }

    let mut edges = Vec::new();
    for &(title, content) in &md_notes {
        let src_idx = *title_to_idx.get(title).unwrap();
        let links = WikilinkParser::extract_links(content);
        for link in links {
            if let Some(&dst_idx) = title_to_idx.get(&link.target) {
                edges.push((src_idx, dst_idx, 1.0f32));
            }
        }
    }

    let csr = CsrGraph::from_nodes_and_edges(&titles, &edges);
    assert_eq!(csr.num_nodes, 3);
    assert_eq!(csr.out_degree(0), 2); // NodeA -> NodeB, NodeC
    assert_eq!(csr.out_degree(1), 1); // NodeB -> NodeC
    assert_eq!(csr.out_degree(2), 0); // NodeC -> 0
}

// ============================================================================
// PAIR 5: CSR SPARSE MATRIX + HIPPORAG PPR
// ============================================================================

#[test]
fn test_p5_csr_sparse_matrix_with_hipporag_ppr() {
    let nodes = vec!["QueryEntity".to_string(), "Doc1".to_string(), "Doc2".to_string(), "Doc3".to_string()];
    let edges = vec![
        (0, 1, 1.0), // QueryEntity -> Doc1
        (1, 2, 1.0), // Doc1 -> Doc2
        (0, 3, 0.2), // QueryEntity -> Doc3 (weak)
    ];

    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);

    assert_eq!(ppr.len(), 4);
    assert!(ppr[1] > ppr[3], "Doc1 directly linked must have higher PPR than weak link Doc3");
}

// ============================================================================
// PAIR 6: HIPPORAG PPR + 3-WAY RRF FUSION
// ============================================================================

#[test]
fn test_p6_hipporag_ppr_with_3way_rrf_fusion() {
    let rrf = TriModalRrfEngine::new();

    let bm25_hits = vec![
        SearchHit { id: "doc_a".to_string(), score: 0.9, snippet: "".to_string() },
        SearchHit { id: "doc_b".to_string(), score: 0.8, snippet: "".to_string() },
    ];
    let dense_hits = vec![
        SearchHit { id: "doc_b".to_string(), score: 0.95, snippet: "".to_string() },
        SearchHit { id: "doc_c".to_string(), score: 0.85, snippet: "".to_string() },
    ];
    let graph_hits = vec![
        SearchHit { id: "doc_b".to_string(), score: 0.45, snippet: "".to_string() },
        SearchHit { id: "doc_a".to_string(), score: 0.30, snippet: "".to_string() },
    ];

    let fused = rrf.fuse(&bm25_hits, &dense_hits, &graph_hits, 3);
    assert_eq!(fused[0].id, "doc_b", "Doc B ranks #1 across modalities and wins RRF");
}

// ============================================================================
// PAIR 7: DIACRITIC BM25 + INT8 VECTOR SEARCH
// ============================================================================

#[test]
fn test_p7_diacritic_bm25_with_int8_vector_search() {
    let mut bm25 = SparseBm25Engine::new();
    let mut vec_store = DenseVectorStore::new();

    bm25.index_document("doc1", "Xử lý ngôn ngữ tự nhiên tiếng Việt");
    bm25.index_document("doc2", "Quản lý cơ sở dữ liệu phân tán");

    vec_store.insert_vector("doc1", &[0.8, 0.6]);
    vec_store.insert_vector("doc2", &[0.1, 0.9]);

    // Unaccented query
    let bm25_hits = bm25.search("ngon ngu tieng viet", 5);
    assert_eq!(bm25_hits.len(), 1);
    assert_eq!(bm25_hits[0].id, "doc1");

    let vec_hits = vec_store.search_knn(&[0.85, 0.55], 5);
    assert_eq!(vec_hits[0].id, "doc1");
}

// ============================================================================
// PAIR 8: 3-WAY RRF + EPISODIC DECAY SCORING
// ============================================================================

#[test]
fn test_p8_3way_rrf_with_episodic_decay_scoring() {
    let rrf = TriModalRrfEngine::new();
    let tau = 86400.0;

    let bm25 = vec![SearchHit { id: "old_hit".to_string(), score: 1.0, snippet: "".to_string() }];
    let dense = vec![SearchHit { id: "new_hit".to_string(), score: 1.0, snippet: "".to_string() }];

    let mut fused = rrf.fuse(&bm25, &dense, &[], 2);

    // Apply time decay: old_hit (10 days old), new_hit (1 hour old)
    let decay_old = L2EpisodicStore::compute_retention_score(10.0 * tau, tau); // ~0.00097
    let decay_new = L2EpisodicStore::compute_retention_score(3600.0, tau); // ~0.97

    for hit in &mut fused {
        if hit.id == "old_hit" {
            hit.score *= decay_old;
        } else if hit.id == "new_hit" {
            hit.score *= decay_new;
        }
    }

    fused.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
    assert_eq!(fused[0].id, "new_hit", "Recent memory boosted above old memory");
}

// ============================================================================
// PAIR 9: L4 PROCEDURAL MEMORY PRIOR + TAURI IPC
// ============================================================================

#[test]
fn test_p9_procedural_memory_prior_with_tauri_ipc() {
    let engine = VirtualMemoryEngine::new(None);

    engine.l4_skills.lock().unwrap().register_skill(ProceduralSkill {
        id: "skill-search".to_string(),
        name: "Search Tool".to_string(),
        task_type: "retrieval".to_string(),
        prerequisites: vec![],
        successes: 5,
        failures: 0,
    });

    let status_res = engine.handle_ipc_command("memory_status", "{}").unwrap();
    assert!(status_res.contains("l1_blocks"));

    let prior = engine.l4_skills.lock().unwrap().get_skill_prior("skill-search").unwrap();
    // (5 + 1) / (5 + 0 + 2) = 6/7 ~ 0.857
    assert!((prior - (6.0 / 7.0)).abs() < 1e-4);
}

// ============================================================================
// PAIR 10: SUMMARY TREE + L1 RADIX PREFIX CACHE
// ============================================================================

#[test]
fn test_p10_summary_tree_with_radix_prefix_cache() {
    let mut cache = RadixPrefixCache::new(50);
    let turns: Vec<ConversationTurn> = (0..6)
        .map(|i| ConversationTurn {
            role: "user".to_string(),
            content: format!("Long technical turn {}", i),
            token_count: 50,
        })
        .collect();

    // Context exceeds budget -> triggers summary tree
    let summary_node = SummaryTree::maybe_trigger_summary_tree(&turns, 100).unwrap();

    // Convert summary string to token sequence
    let summary_tokens: Vec<u32> = summary_node.summary.bytes().take(20).map(|b| b as u32).collect();

    // Cache summary tokens as pinned prefix
    cache.insert_prefix(&summary_tokens, 42, true);

    let (len, blocks) = cache.match_prefix(&summary_tokens);
    assert_eq!(len, summary_tokens.len());
    assert_eq!(blocks, vec![42]);
}

// ============================================================================
// PAIR 11: AES ENCLAVE + ARGON2ID KDF + ZERO LEAKAGE
// ============================================================================

#[test]
fn test_p11_aes_enclave_argon2id_kdf_zero_leakage() {
    let master_pass = b"argon2id_secure_master_password";
    let salt = b"argon2id_master_salt";

    let enclave = MemoryEnclave::new_with_argon2id(master_pass, salt).unwrap();
    let secret = b"super_sensitive_api_token";

    let envelope = enclave.encrypt_record(secret).unwrap();
    let decrypted_buffer = ZeroizingBuffer(enclave.decrypt_record(&envelope).unwrap());

    assert_eq!(decrypted_buffer.0, secret);
    drop(decrypted_buffer); // Zeroized
}

// ============================================================================
// PAIR 12: OBSIDIAN SYNC + CSR GRAPH + HIPPORAG PPR
// ============================================================================

#[test]
fn test_p12_obsidian_vault_sync_csr_graph_hipporag() {
    let mut sync = ObsidianSyncEngine::new();
    sync.index_note(PathBuf::from("A.md"), "# A\nRefers to [[B]]");
    sync.index_note(PathBuf::from("B.md"), "# B\nRefers to [[C]]");
    sync.index_note(PathBuf::from("C.md"), "# C\nTerminal note");

    let titles = vec!["A".to_string(), "B".to_string(), "C".to_string()];
    let edges = vec![(0, 1, 1.0f32), (1, 2, 1.0f32)];

    let csr = CsrGraph::from_nodes_and_edges(&titles, &edges);
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);

    assert!(ppr[0] > 0.0);
    assert!(ppr[1] > 0.0);
    assert!(ppr[2] > 0.0);
}

// ============================================================================
// PAIR 13: LLMLINGUA-2 PRUNING + RECURSIVE SUMMARY TREE
// ============================================================================

#[test]
fn test_p13_llmlingua_pruning_with_summary_tree() {
    let raw_tokens: Vec<u32> = (0..100).collect();
    let comp = ContextCompressor::compress_llmlingua(&raw_tokens, 0.4, &vec![false; 100]);

    let turn = ConversationTurn {
        role: "user".to_string(),
        content: format!("Compressed {} tokens", comp.compressed_tokens.len()),
        token_count: comp.compressed_tokens.len(),
    };

    let tree = SummaryTree::build_tree(&[turn.clone(), turn], 2);
    assert!(tree.token_count <= 45);
}

// ============================================================================
// PAIR 14: SQLITE-VEC INT8 + OBSIDIAN SEMANTIC NOTES
// ============================================================================

#[test]
fn test_p14_sqlite_vec_int8_with_obsidian_semantic_notes() {
    let mut sync = ObsidianSyncEngine::new();
    let mut vec_store = DenseVectorStore::new();

    sync.index_note(
        PathBuf::from("DeepLearning.md"),
        "# Deep Learning\nNeural networks and backpropagation.",
    );

    vec_store.insert_vector("DeepLearning", &[0.7, 0.7, 0.1]);

    let query_embedding = vec![0.75, 0.65, 0.1];
    let hits = vec_store.search_knn(&query_embedding, 1);

    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, "DeepLearning");
    assert!(hits[0].similarity > 0.95);
}

// ============================================================================
// PAIR 15: VIRTUAL MEMORY ENGINE FACADE FULL PIPELINE
// ============================================================================

#[test]
fn test_p15_virtual_memory_engine_full_tier_pipeline() {
    let enclave = MemoryEnclave::new_with_argon2id(b"master_key", b"salt").unwrap();
    let engine = VirtualMemoryEngine::new(Some(enclave));

    // 1. Store episodic memory
    let event = EpisodicEvent {
        id: "full_e2e_event".to_string(),
        content: "Kế hoạch phát triển LIVA Phase 3 hoàn chỉnh".to_string(),
        timestamp_secs: 1000.0,
        importance: 0.95,
        embedding: vec![0.8, 0.2, 0.1],
        metadata: HashMap::new(),
    };
    engine.store_memory_encrypted(event).unwrap();

    // 2. Hybrid search query
    let recall_hits = engine.query_hybrid("Ke hoach phat trien LIVA", &[0.8, 0.2, 0.1], 5);
    assert!(!recall_hits.is_empty());
    assert_eq!(recall_hits[0].id, "full_e2e_event");

    // 3. IPC invocation
    let ipc_res = engine.handle_ipc_command("memory_recall", "Ke hoach LIVA").unwrap();
    assert!(ipc_res.contains("full_e2e_event"));
}
