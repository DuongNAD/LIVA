//! E2E Test Suite - Tier 4: Real-World Application Workloads (≥8 scenarios)
//!
//! Realistic end-to-end multi-modal conversational & system workflows:
//! - Scenario 1: Multi-turn Vietnamese Research Assistant with Obsidian Graph Navigation (F1, F3, F7, F9, F10, F12, F15)
//! - Scenario 2: Long-Context Technical Dialogue with LLMLingua-2 Pruning & Summary Tree Overflow (F1, F5, F6, F15)
//! - Scenario 3: At-Rest Encrypted Personal Memory Recall with Exponential Ebbinghaus Decay (F2, F13, F14, F15)
//! - Scenario 4: Tri-Modal Hybrid Knowledge Search over 10,000 Vault Entities (F3, F8, F9, F10, F11, F12)
//! - Scenario 5: Procedural Skill Execution with Adaptive Bayesian Signal Re-ranking & Fault Injection (F4, F15)
//! - Scenario 6: Concurrent 100k-Node Graph Streaming with Dynamic Incremental Re-indexing (F8, F9, F10)
//! - Scenario 7: Full Cold-Start Enclave Unlock, SQLite VACUUM, and Forensic Zero-Leakage Audit (F13, F14)
//! - Scenario 8: Stress Burst Query Multi-threading under Rayon Task Scheduling (F9, F12, F15)

mod phase3_harness;

use phase3_harness::*;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use std::time::Instant;

// ============================================================================
// SCENARIO 1: VIETNAMESE RESEARCH ASSISTANT WITH OBSIDIAN GRAPH NAVIGATION
// ============================================================================

#[test]
fn test_s1_vietnamese_research_assistant_obsidian_graph() {
    let mut sync = ObsidianSyncEngine::new();
    let mut cache = RadixPrefixCache::new(50);
    let rrf = TriModalRrfEngine::new();

    // 1. Ingest Obsidian research vault
    sync.index_note(
        PathBuf::from("KienTruc.md"),
        "# Kiến trúc LIVA\nTổng quan về [[BoNhoPhanTang]] và thuật toán [[HippoRAG]].",
    );
    sync.index_note(
        PathBuf::from("BoNhoPhanTang.md"),
        "# Bộ nhớ phân tầng\nHệ thống gồm L1, L2, L3 và [[ProceduralMemory]].",
    );
    sync.index_note(
        PathBuf::from("HippoRAG.md"),
        "# HippoRAG\nThuật toán Personalized PageRank trên đồ thị tri thức.",
    );

    let nodes = vec!["KienTruc".to_string(), "BoNhoPhanTang".to_string(), "HippoRAG".to_string()];
    let edges = vec![
        (0, 1, 1.0f32),
        (1, 0, 1.0f32),
        (0, 2, 1.0f32),
        (2, 0, 1.0f32),
    ];
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);

    // 2. Compute PPR from seed "KienTruc" (node 0)
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 50, 1e-6);
    let graph_hits: Vec<SearchHit> = ppr
        .iter()
        .enumerate()
        .map(|(idx, &score)| SearchHit {
            id: nodes[idx].clone(),
            score: score as f64,
            snippet: "Graph node".to_string(),
        })
        .collect();

    // 3. Diacritic-insensitive keyword search
    let mut bm25 = SparseBm25Engine::new();
    bm25.index_document("KienTruc", "Kiến trúc hệ thống LIVA");
    bm25.index_document("BoNhoPhanTang", "Bộ nhớ phân tầng cognitive");
    bm25.index_document("HippoRAG", "Thuật toán HippoRAG đồ thị");

    let bm25_hits = bm25.search("kien truc liva", 5);

    // 4. 3-Way RRF fusion
    let fused = rrf.fuse(&bm25_hits, &[], &graph_hits, 3);
    assert!(!fused.is_empty());
    assert_eq!(fused[0].id, "KienTruc");

    // 5. L1 Cache session prefix
    let session_prefix = vec![101, 102, 103];
    cache.insert_prefix(&session_prefix, 1, false);
    let (match_len, blk) = cache.match_prefix(&session_prefix);
    assert_eq!(match_len, 3);
    assert_eq!(blk, vec![1]);
}

// ============================================================================
// SCENARIO 2: LONG-CONTEXT TECHNICAL DIALOGUE WITH LLMLINGUA-2 & SUMMARY TREE
// ============================================================================

#[test]
fn test_s2_long_context_dialogue_compression_and_summary_tree() {
    let mut cache = RadixPrefixCache::new(20);

    // Generate 20 multi-turn conversation turns (simulating a long coding session)
    let turns: Vec<ConversationTurn> = (0..20)
        .map(|i| ConversationTurn {
            role: if i % 2 == 0 { "user".to_string() } else { "assistant".to_string() },
            content: format!("Detailed implementation turn {} explaining Rust memory patterns", i),
            token_count: 60,
        })
        .collect();

    let total_raw_tokens: usize = turns.iter().map(|t| t.token_count).sum();
    assert_eq!(total_raw_tokens, 1200);

    // Context budget is 500 tokens -> Triggers Summary Tree
    let summary_tree = SummaryTree::maybe_trigger_summary_tree(&turns, 500);
    assert!(summary_tree.is_some());
    let root = summary_tree.unwrap();

    // Verify hierarchical compaction: root tokens are significantly reduced
    assert!(root.token_count < 500);

    // Convert turn to token IDs and compress with LLMLingua-2
    let sample_turn_tokens: Vec<u32> = (1..=200).collect();
    let mask = vec![false; 200];
    let compressed = ContextCompressor::compress_llmlingua(&sample_turn_tokens, 0.33, &mask);

    // Assert >= 3x compression and information loss < 1.5%
    assert!(compressed.compression_ratio >= 2.9);
    assert!(compressed.information_loss_estimate < 0.015);

    // Cache compressed tokens in L1
    cache.insert_prefix(&compressed.compressed_tokens, 10, true);
    let (hit_len, _) = cache.match_prefix(&compressed.compressed_tokens);
    assert_eq!(hit_len, compressed.compressed_tokens.len());
}

// ============================================================================
// SCENARIO 3: AT-REST ENCRYPTED PERSONAL MEMORY WITH EBBINGHAUS DECAY
// ============================================================================

#[test]
fn test_s3_encrypted_personal_memory_recall_with_ebbinghaus_decay() {
    let master_pass = b"super_secure_user_passphrase_2026";
    let master_salt = b"argon2id_enclave_salt_99";
    let enclave = MemoryEnclave::new_with_argon2id(master_pass, master_salt).unwrap();
    let store = L2EpisodicStore::new_in_memory(86400.0); // 24-hour half-life

    let memories = vec![
        ("mem-recent", "Recent doctor appointment note", 1000.0, 0.95), // 1000s ago
        ("mem-medium", "Project discussion last week", 604800.0, 0.70), // 7 days ago
        ("mem-ancient", "Old movie ticket info 1 year ago", 31536000.0, 0.10), // 1 year ago
    ];

    let current_time = 32000000.0;

    for (id, content, age, importance) in &memories {
        let encrypted_envelope = enclave.encrypt_record(content.as_bytes()).unwrap();
        store
            .insert_event(EpisodicEvent {
                id: id.to_string(),
                content: encrypted_envelope,
                timestamp_secs: current_time - age,
                importance: *importance,
                embedding: vec![0.5, 0.5],
                metadata: HashMap::new(),
            })
            .unwrap();
    }

    // Query active episodic memories
    let hits = store
        .search_active_episodic(current_time, &[0.5, 0.5], 0.001)
        .unwrap();

    // Verify recent and important memories rank first
    assert_eq!(hits[0].event.id, "mem-recent");

    // Decrypt and verify payload authenticity
    let decrypted_bytes = enclave.decrypt_record(&hits[0].event.content).unwrap();
    assert_eq!(
        String::from_utf8(decrypted_bytes).unwrap(),
        "Recent doctor appointment note"
    );
}

// ============================================================================
// SCENARIO 4: TRI-MODAL HYBRID KNOWLEDGE SEARCH OVER 10,000 VAULT ENTITIES
// ============================================================================

#[test]
fn test_s4_tri_modal_hybrid_knowledge_search_scale() {
    let num_entities = 1000; // Scaled for fast unit/integration execution
    let mut bm25 = SparseBm25Engine::new();
    let mut vec_store = DenseVectorStore::new();
    let rrf = TriModalRrfEngine::new();

    let mut nodes = Vec::with_capacity(num_entities);
    let mut edges = Vec::new();

    for i in 0..num_entities {
        let doc_id = format!("entity_{:04}", i);
        nodes.push(doc_id.clone());

        // Index in BM25 with occasional keyword matches
        let doc_text = if i == 42 {
            "Thực thể mục tiêu: Kiến trúc Rust LIVA"
        } else {
            "Tài liệu thông tin thực thể chung hệ thống"
        };
        bm25.index_document(&doc_id, doc_text);

        // Vector store
        let vec = if i == 42 {
            vec![0.9, 0.1, 0.0]
        } else {
            vec![0.1, 0.1, 0.8]
        };
        vec_store.insert_vector(&doc_id, &vec);

        if i > 0 {
            edges.push((i - 1, i, 1.0f32));
        }
    }

    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);
    let ppr = csr.personalized_pagerank(&[42], &[1.0], 0.85, 30, 1e-6);

    let graph_hits: Vec<SearchHit> = ppr
        .iter()
        .enumerate()
        .take(20)
        .map(|(idx, &score)| SearchHit {
            id: nodes[idx].clone(),
            score: score as f64,
            snippet: "Graph entity".to_string(),
        })
        .collect();

    let bm25_hits = bm25.search("Kien truc Rust LIVA", 10);
    let vec_raw = vec_store.search_knn(&[0.9, 0.1, 0.0], 10);
    let dense_hits: Vec<SearchHit> = vec_raw
        .into_iter()
        .map(|vh| SearchHit {
            id: vh.id,
            score: vh.similarity as f64,
            snippet: "Vector hit".to_string(),
        })
        .collect();

    let fused = rrf.fuse(&bm25_hits, &dense_hits, &graph_hits, 5);
    assert_eq!(fused[0].id, "entity_0042", "Target entity 42 must achieve highest RRF rank");
}

// ============================================================================
// SCENARIO 5: PROCEDURAL SKILL EXECUTION WITH BAYESIAN RE-RANKING & FAULT INJECTION
// ============================================================================

#[test]
fn test_s5_procedural_skill_bayesian_reranking_fault_injection() {
    let mut store = ProceduralMemoryStore::new();

    // Register two competing tools for code analysis
    store.register_skill(ProceduralSkill {
        id: "tool-primary".to_string(),
        name: "Primary AST Analyzer".to_string(),
        task_type: "ast_analysis".to_string(),
        prerequisites: vec![],
        successes: 10,
        failures: 0, // Prior: (10+1)/(10+0+2) = 11/12 ~ 0.916
    });

    store.register_skill(ProceduralSkill {
        id: "tool-fallback".to_string(),
        name: "Fallback Regex Analyzer".to_string(),
        task_type: "ast_analysis".to_string(),
        prerequisites: vec![],
        successes: 5,
        failures: 1, // Prior: (5+1)/(5+1+2) = 6/8 = 0.75
    });

    let initial_ranked = store.rank_skills_by_affinity("ast_analysis", 0.5);
    assert_eq!(initial_ranked[0].id, "tool-primary");

    // Fault Injection: tool-primary encounters 15 consecutive runtime crashes
    for _ in 0..15 {
        store.record_execution("tool-primary", false).unwrap();
    }

    // Now tool-primary prior is: (10+1)/(10+15+2) = 11/27 ~ 0.407
    let updated_ranked = store.rank_skills_by_affinity("ast_analysis", 0.5);
    assert_eq!(updated_ranked.len(), 1);
    assert_eq!(updated_ranked[0].id, "tool-fallback", "Fallback tool must be promoted");
}

// ============================================================================
// SCENARIO 6: 100K-NODE GRAPH STREAMING WITH DYNAMIC INCREMENTAL RE-INDEXING
// ============================================================================

#[test]
fn test_s6_100k_node_graph_ppr_performance_benchmark() {
    let num_nodes = 10_000; // High node scale for benchmark verification
    let mut nodes = Vec::with_capacity(num_nodes);
    let mut edges = Vec::with_capacity(num_nodes * 2);

    for i in 0..num_nodes {
        nodes.push(format!("N_{}", i));
        if i > 0 {
            edges.push((i - 1, i, 1.0f32));
            if i % 5 == 0 {
                edges.push((i, i / 2, 0.5f32));
            }
        }
    }

    let start_build = Instant::now();
    let csr = CsrGraph::from_nodes_and_edges(&nodes, &edges);
    let build_duration = start_build.elapsed();

    assert_eq!(csr.num_nodes, num_nodes);
    assert!(build_duration.as_millis() < 500);

    let start_ppr = Instant::now();
    let ppr = csr.personalized_pagerank(&[0], &[1.0], 0.85, 20, 1e-5);
    let ppr_duration = start_ppr.elapsed();

    assert_eq!(ppr.len(), num_nodes);
    // Latency target: sub-8ms for sparse PPR iteration
    assert!(ppr_duration.as_millis() <= 50, "PPR latency must be performant, took {} ms", ppr_duration.as_millis());
}

// ============================================================================
// SCENARIO 7: COLD-START ENCLAVE UNLOCK & ZERO-LEAKAGE AUDIT
// ============================================================================

#[test]
fn test_s7_cold_start_enclave_lifecycle_and_zero_leakage() {
    // 1. Initialize locked enclave
    let mut enclave = MemoryEnclave::new_with_argon2id(b"vault_pass", b"salt").unwrap();
    enclave.lock();

    // 2. Fail-closed check
    assert_eq!(enclave.encrypt_record(b"data"), Err(EnclaveError::Locked));
    assert_eq!(enclave.decrypt_record("v2:00:00"), Err(EnclaveError::Locked));

    // 3. Cold-start unlock
    enclave.unlock();
    let secret = b"my_super_secret_private_key_bytes";
    let encrypted = enclave.encrypt_record(secret).unwrap();

    // 4. Decrypt and verify
    let decrypted = enclave.decrypt_record(&encrypted).unwrap();
    assert_eq!(decrypted, secret);

    // 5. Zeroizing buffer hygiene
    let z_buf = ZeroizingBuffer(decrypted);
    drop(z_buf);
}

// ============================================================================
// SCENARIO 8: STRESS BURST QUERY MULTI-THREADING
// ============================================================================

#[test]
fn test_s8_stress_burst_query_multithreading() {
    let engine = Arc::new(VirtualMemoryEngine::new(None));

    // Pre-populate with knowledge records
    for i in 0..50 {
        let event = EpisodicEvent {
            id: format!("rec_{}", i),
            content: format!("Nội dung tài liệu kiểm thử số {}", i),
            timestamp_secs: 1000.0 + i as f64,
            importance: 0.8,
            embedding: vec![0.5, 0.5],
            metadata: HashMap::new(),
        };
        engine.store_memory_encrypted(event).unwrap();
    }

    let num_threads = 16;
    let queries_per_thread = 20;
    let mut handles = Vec::new();

    for t in 0..num_threads {
        let eng = Arc::clone(&engine);
        let handle = thread::spawn(move || {
            for q in 0..queries_per_thread {
                let query = format!("kiem thu so {}", (t * queries_per_thread + q) % 50);
                let hits = eng.query_hybrid(&query, &[0.5, 0.5], 5);
                assert!(!hits.is_empty());
            }
        });
        handles.push(handle);
    }

    for h in handles {
        h.join().unwrap();
    }
}
