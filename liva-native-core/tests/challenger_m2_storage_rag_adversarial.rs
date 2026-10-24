//! Challenger Empirical Stress Test Suite: Storage, Memory & RAG Hot-Paths
//!
//! Adversarially challenges:
//! 1. FTS5 BM25 Ranking:
//!    - Rank ordering consistency across multiple search terms & term frequencies.
//!    - Special syntax characters: quotes, unbalanced quotes, asterisks, boolean AND/OR, punctuation, empty query.
//!    - Hybrid search fusion score consistency and rank monotonicity.
//! 2. HippoRAG SpMV & CSR Graph:
//!    - Empty graph (0 nodes, 0 edges).
//!    - Single-node graph (isolated, self-loop, non-existent seed).
//!    - Disconnected components (isolation boundary verification).
//!    - Zero weights, negative weights, zero seed weights (no NaN, no division by zero).
//!    - Top-K clamping when K > N, K = 0, K = usize::MAX.
//!    - Dirty vs compiled CSR execution parity.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::csr_graph::CsrGraph;
use liva_native_core::db::{
    DatabasePool, MetadataFilter, search_fts_vectors, search_hybrid_vectors, upsert_vector,
};

// =========================================================================
// 1. FTS5 BM25 RANKING & ADVERSARIAL SYNTAX TESTS
// =========================================================================

#[test]
fn test_fts5_bm25_rank_ordering_consistency() {
    let pool = DatabasePool::new_in_memory().expect("Failed to create in-memory SQLite pool");
    let conn = pool
        .writer
        .get()
        .expect("Failed to acquire writer connection");
    let engine = EncryptionEngine::new("fts5-bm25-test-key-32-bytes-long");
    let dummy_vec = vec![0.0f32; 384];

    let filter = MetadataFilter {
        r#type: None,
        domain: None,
        category: None,
        created_after: None,
        created_before: None,
    };

    // Document 1: Low term frequency for 'Rust'
    upsert_vector(
        &conn,
        &engine,
        "doc_low",
        "doc",
        "Rust is a systems programming language focusing on safety and speed.",
        &dummy_vec,
        Some("tech"),
        Some("lang"),
        Some(&["rust".to_string()]),
        None,
        None,
    )
    .expect("Failed to upsert doc_low");

    // Document 2: High term frequency for 'Rust' (repeated 4 times)
    upsert_vector(
        &conn,
        &engine,
        "doc_high",
        "doc",
        "Rust Rust Rust is exceptionally fast and memory safe. Rust ensures memory safety without GC.",
        &dummy_vec,
        Some("tech"),
        Some("lang"),
        Some(&["rust".to_string()]),
        None,
        None,
    )
    .expect("Failed to upsert doc_high");

    // Document 3: Non-matching document
    upsert_vector(
        &conn,
        &engine,
        "doc_other",
        "doc",
        "Python and JavaScript are dynamically typed scripting languages.",
        &dummy_vec,
        Some("tech"),
        Some("lang"),
        Some(&["python".to_string()]),
        None,
        None,
    )
    .expect("Failed to upsert doc_other");

    // Query 'Rust'
    let results = search_fts_vectors(&conn, "Rust", 5, &filter).expect("FTS search failed");

    assert_eq!(results.len(), 2, "Must find exactly 2 matching documents");
    // Under BM25 ranking (ORDER BY f.rank), higher term frequency produces a more negative score,
    // hence doc_high MUST appear first.
    assert_eq!(
        results[0].vec_id, "doc_high",
        "BM25 ordering violation: doc_high must rank first due to higher term frequency"
    );
    assert_eq!(
        results[1].vec_id, "doc_low",
        "doc_low must rank second behind doc_high"
    );
}

#[test]
fn test_fts5_multi_term_conjunction_ranking() {
    let pool = DatabasePool::new_in_memory().expect("Failed to create in-memory SQLite pool");
    let conn = pool
        .writer
        .get()
        .expect("Failed to acquire writer connection");
    let engine = EncryptionEngine::new("fts5-bm25-test-key-32-bytes-long");
    let dummy_vec = vec![0.0f32; 384];

    let filter = MetadataFilter {
        r#type: None,
        domain: None,
        category: None,
        created_after: None,
        created_before: None,
    };

    // Doc A: Matches all 3 terms: 'memory', 'safety', 'concurrency'
    upsert_vector(
        &conn,
        &engine,
        "doc_a",
        "doc",
        "Rust guarantees memory safety and fearless concurrency in systems programming.",
        &dummy_vec,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Doc B: Matches 2 terms: 'memory', 'safety'
    upsert_vector(
        &conn,
        &engine,
        "doc_b",
        "doc",
        "Memory safety is critical for preventing security vulnerabilities.",
        &dummy_vec,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Doc C: Matches only 1 term: 'concurrency'
    upsert_vector(
        &conn,
        &engine,
        "doc_c",
        "doc",
        "High-performance concurrency patterns in distributed systems.",
        &dummy_vec,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Query 2 terms: "memory safety"
    let results_2 = search_fts_vectors(&conn, "memory safety", 5, &filter).unwrap();
    // Default prepare_fts_query joins terms with AND, so both doc_a and doc_b must match
    assert_eq!(results_2.len(), 2);
    let ids: Vec<&str> = results_2.iter().map(|r| r.vec_id.as_str()).collect();
    assert!(ids.contains(&"doc_a"));
    assert!(ids.contains(&"doc_b"));
    assert!(!ids.contains(&"doc_c"));
}

#[test]
fn test_fts5_special_syntax_characters_and_edge_cases() {
    let pool = DatabasePool::new_in_memory().expect("Failed to create in-memory SQLite pool");
    let conn = pool
        .writer
        .get()
        .expect("Failed to acquire writer connection");
    let engine = EncryptionEngine::new("fts5-bm25-test-key-32-bytes-long");
    let dummy_vec = vec![0.0f32; 384];

    let filter = MetadataFilter {
        r#type: None,
        domain: None,
        category: None,
        created_after: None,
        created_before: None,
    };

    upsert_vector(
        &conn,
        &engine,
        "doc_special",
        "doc",
        "Testing C++ and C# with pointers *ptr, email admin@liva.ai, and \"quoted text\"!",
        &dummy_vec,
        None,
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // 1. Query with double quotes (should be escaped by prepare_fts_query)
    let r_quote = search_fts_vectors(&conn, "\"quoted text\"", 5, &filter);
    assert!(r_quote.is_ok(), "FTS query with quotes must not error");
    let matches = r_quote.unwrap();
    assert!(!matches.is_empty(), "Must find doc_special");

    // 2. Query with unbalanced quotes
    let r_unbal = search_fts_vectors(&conn, "admin \"quoted", 5, &filter);
    assert!(
        r_unbal.is_ok(),
        "FTS query with unbalanced quotes must not panic/error"
    );

    // 3. Query with asterisks
    let r_ast = search_fts_vectors(&conn, "ptr*", 5, &filter);
    assert!(r_ast.is_ok(), "FTS query with asterisks must not error");

    // 4. Query with FTS boolean operators: AND, OR, NOT
    let r_and = search_fts_vectors(&conn, "pointers AND email", 5, &filter);
    assert!(r_and.is_ok(), "FTS query with boolean AND must not error");

    let r_or = search_fts_vectors(&conn, "pointers OR missing_term_xyz", 5, &filter);
    assert!(r_or.is_ok(), "FTS query with boolean OR must not error");

    // 5. Query with punctuation / special symbols
    let r_sym = search_fts_vectors(&conn, "C++ C# (admin@liva.ai)", 5, &filter);
    assert!(
        r_sym.is_ok(),
        "FTS query with symbols/parentheses must not error"
    );

    // 6. Non-matching query returns empty Vec cleanly
    let r_none = search_fts_vectors(&conn, "completely_unrelated_query_12345", 5, &filter).unwrap();
    assert!(r_none.is_empty());
}

#[test]
fn test_hybrid_search_fusion_consistency() {
    let pool = DatabasePool::new_in_memory().expect("Failed to create in-memory SQLite pool");
    let conn = pool
        .writer
        .get()
        .expect("Failed to acquire writer connection");
    let engine = EncryptionEngine::new("fts5-bm25-test-key-32-bytes-long");

    let vec_tech = vec![1.0f32; 384];
    let vec_finance = vec![-1.0f32; 384];

    let filter = MetadataFilter {
        r#type: None,
        domain: None,
        category: None,
        created_after: None,
        created_before: None,
    };

    upsert_vector(
        &conn,
        &engine,
        "v_tech",
        "doc",
        "Deep learning and transformer neural networks in PyTorch",
        &vec_tech,
        Some("ai"),
        None,
        None,
        None,
        None,
    )
    .unwrap();

    upsert_vector(
        &conn,
        &engine,
        "v_finance",
        "doc",
        "Quarterly earnings report and cash flow financial balance sheet",
        &vec_finance,
        Some("finance"),
        None,
        None,
        None,
        None,
    )
    .unwrap();

    // Hybrid query aligning with tech both sparsely ("neural networks") and densely (vec_tech)
    let hybrid_results = search_hybrid_vectors(
        &conn,
        &engine,
        "neural networks",
        &vec_tech,
        5,
        &filter,
        1.0,
        1.0,
    )
    .expect("Hybrid search failed");

    assert_eq!(hybrid_results.len(), 2);
    assert_eq!(
        hybrid_results[0].vec_id, "v_tech",
        "v_tech must rank first in hybrid search"
    );
    assert!(
        hybrid_results[0].score > hybrid_results[1].score,
        "Aggregated score must strictly decrease"
    );
}

// =========================================================================
// 2. HIPPORAG SPMV & CSR GRAPH ADVERSARIAL STRESS TESTS
// =========================================================================

#[test]
fn test_hipporag_empty_graph() {
    let mut empty_graph = CsrGraph::new();
    assert_eq!(empty_graph.node_count(), 0);
    assert_eq!(empty_graph.edge_count(), 0);

    // PPR on empty graph with no seeds
    let r1 = empty_graph.personalized_pagerank(&[], 3, 0.85, 10);
    assert!(r1.is_empty(), "Empty graph must return empty result");

    // PPR on empty graph with seeds
    let r2 = empty_graph.personalized_pagerank(&[("non_existent", 1.0)], 3, 0.85, 10);
    assert!(
        r2.is_empty(),
        "Empty graph with seed must return empty result"
    );

    // Readonly PPR on empty graph
    let r3 = empty_graph.personalized_pagerank_readonly(&[], 3, 0.85, 10);
    assert!(
        r3.is_empty(),
        "Readonly on empty graph must return empty result"
    );

    let r4 = empty_graph.personalized_pagerank_readonly(&[("ghost", 2.0)], 3, 0.85, 10);
    assert!(
        r4.is_empty(),
        "Readonly with missing seed must return empty result"
    );
}

#[test]
fn test_hipporag_single_node_graph() {
    let mut graph = CsrGraph::new();
    let idx = graph.add_node(
        "lone_node".to_string(),
        "Lone Node".to_string(),
        "{}".to_string(),
    );
    assert_eq!(idx, 0);
    assert_eq!(graph.node_count(), 1);

    // 1. Isolated single node (0 edges)
    graph.compile_csr();
    let res_isolated = graph.personalized_pagerank_readonly(&[("lone_node", 1.0)], 3, 0.85, 10);
    assert_eq!(res_isolated.len(), 1);
    assert_eq!(res_isolated[0].0, "lone_node");
    assert!(!res_isolated[0].1.is_nan(), "Score must not be NaN");
    assert!(
        !res_isolated[0].1.is_infinite(),
        "Score must not be infinite"
    );

    // 2. Single node with self-loop edge
    graph.add_edge("lone_node", "lone_node", "self_ref", 1.0, false);
    graph.compile_csr();
    let res_self_loop = graph.personalized_pagerank(&[("lone_node", 1.0)], 3, 0.85, 10);
    assert_eq!(res_self_loop.len(), 1);
    assert_eq!(res_self_loop[0].0, "lone_node");
    // With a self-loop, probability mass is fully conserved: 1.0
    assert!(
        (res_self_loop[0].1 - 1.0).abs() < 1e-4,
        "Self-loop must conserve total probability mass to ~1.0, got {}",
        res_self_loop[0].1
    );

    // 3. Query with seed that does not exist in single node graph
    let res_missing_seed = graph.personalized_pagerank_readonly(&[("unknown", 1.0)], 3, 0.85, 10);
    assert!(res_missing_seed.is_empty());
}

#[test]
fn test_hipporag_disconnected_components_isolation() {
    let mut graph = CsrGraph::new();

    // Component 1: alpha <-> beta <-> gamma
    graph.add_edge("alpha", "beta", "connected", 1.0, true);
    graph.add_edge("beta", "gamma", "connected", 1.0, true);

    // Component 2: delta <-> epsilon (completely isolated from Component 1)
    graph.add_edge("delta", "epsilon", "connected", 1.0, true);

    graph.compile_csr();
    assert_eq!(graph.node_count(), 5);

    // Activate seed exclusively in Component 1
    let results = graph.personalized_pagerank_readonly(&[("alpha", 1.0)], 3, 0.85, 10);
    let activated_nodes: Vec<String> = results.iter().map(|(id, _)| id.clone()).collect();

    // Component 1 nodes must be present
    assert!(activated_nodes.contains(&"alpha".to_string()));
    assert!(activated_nodes.contains(&"beta".to_string()));
    assert!(activated_nodes.contains(&"gamma".to_string()));

    // Component 2 nodes must NEVER be activated
    assert!(
        !activated_nodes.contains(&"delta".to_string()),
        "Isolation breach: delta must not be activated from alpha"
    );
    assert!(
        !activated_nodes.contains(&"epsilon".to_string()),
        "Isolation breach: epsilon must not be activated from alpha"
    );
}

#[test]
fn test_hipporag_zero_and_negative_weights_adversarial() {
    let mut graph = CsrGraph::new();

    // Edges with zero weights and negative weights
    graph.add_edge("n1", "n2", "rel_zero", 0.0, true);
    graph.add_edge("n2", "n3", "rel_neg", -5.0, true);
    graph.add_edge("n3", "n4", "rel_pos", 1.0, true);

    graph.compile_csr();

    // 1. Seed with zero weight should return empty (seed_sum <= 0.0)
    let r_zero_seed = graph.personalized_pagerank_readonly(&[("n1", 0.0)], 3, 0.85, 10);
    assert!(r_zero_seed.is_empty(), "Zero seed weight must return empty");

    // 2. Seed with negative weight should return empty (clamped to 0)
    let r_neg_seed = graph.personalized_pagerank_readonly(&[("n1", -10.0)], 3, 0.85, 10);
    assert!(
        r_neg_seed.is_empty(),
        "Negative seed weight must return empty"
    );

    // 3. Normal seed traversing zero / negative weight graph
    let r_normal = graph.personalized_pagerank_readonly(&[("n1", 1.0)], 3, 0.85, 10);
    assert!(!r_normal.is_empty());
    for (id, score) in &r_normal {
        assert!(!score.is_nan(), "Score for {} must not be NaN", id);
        assert!(
            !score.is_infinite(),
            "Score for {} must not be infinite",
            id
        );
        assert!(*score >= 0.0, "Score for {} must be non-negative", id);
    }
}

#[test]
fn test_hipporag_top_k_clamping_and_boundaries() {
    let mut graph = CsrGraph::new();

    // Create 5-node star graph with center 'hub'
    for i in 1..=4 {
        graph.add_edge("hub", &format!("leaf_{i}"), "connects", 1.0, true);
    }
    graph.compile_csr();
    assert_eq!(graph.node_count(), 5);

    // 1. K > N (top_k = 100 on 5-node graph)
    let r_large_k = graph.personalized_pagerank_readonly(&[("hub", 1.0)], 3, 0.85, 100);
    assert_eq!(
        r_large_k.len(),
        5,
        "When top_k > N, exactly N nodes must be returned without panic"
    );

    // 2. K = usize::MAX (extreme upper bound)
    let r_max_k = graph.personalized_pagerank_readonly(&[("hub", 1.0)], 3, 0.85, usize::MAX);
    assert_eq!(r_max_k.len(), 5, "usize::MAX top_k must clamp cleanly to N");

    // 3. K = 0 (boundary lower bound)
    let r_zero_k = graph.personalized_pagerank_readonly(&[("hub", 1.0)], 3, 0.85, 0);
    assert_eq!(r_zero_k.len(), 0, "top_k = 0 must return 0 nodes");

    // 4. K = 1 (top-1 only)
    let r_one_k = graph.personalized_pagerank_readonly(&[("hub", 1.0)], 3, 0.85, 1);
    assert_eq!(r_one_k.len(), 1);
    assert_eq!(r_one_k[0].0, "hub", "Hub must be the top-ranked entity");
}

#[test]
fn test_hipporag_compiled_vs_dirty_parity() {
    let mut graph_compiled = CsrGraph::new();
    let mut graph_dirty = CsrGraph::new();

    let edges = [
        ("user", "preference_vietnamese", "has_pref", 1.0),
        ("preference_vietnamese", "danang", "located_in", 0.8),
        ("danang", "central_vietnam", "part_of", 0.9),
        ("user", "role_developer", "works_as", 1.0),
        ("role_developer", "rust_lang", "codes_in", 0.7),
    ];

    for &(u, v, r, w) in &edges {
        graph_compiled.add_edge(u, v, r, w, true);
        graph_dirty.add_edge(u, v, r, w, true);
    }

    // graph_compiled is compiled into flat CSR arrays
    graph_compiled.compile_csr();
    assert!(!graph_compiled.is_dirty());

    // graph_dirty remains uncompiled (dirty = true)
    assert!(graph_dirty.is_dirty());

    let seeds = [("user", 1.0f32), ("rust_lang", 0.5f32)];

    let ppr_compiled = graph_compiled.personalized_pagerank_readonly(&seeds, 3, 0.85, 10);
    let ppr_dirty = graph_dirty.personalized_pagerank_readonly(&seeds, 3, 0.85, 10);

    assert_eq!(ppr_compiled.len(), ppr_dirty.len());

    // Verify ordering and numerical scores match within floating-point tolerance
    for i in 0..ppr_compiled.len() {
        assert_eq!(
            ppr_compiled[i].0, ppr_dirty[i].0,
            "Node at rank {} must match between compiled and dirty execution",
            i
        );
        let diff = (ppr_compiled[i].1 - ppr_dirty[i].1).abs();
        assert!(
            diff < 1e-4,
            "Score divergence at rank {} for node {}: compiled={:.5}, dirty={:.5}",
            i,
            ppr_compiled[i].0,
            ppr_compiled[i].1,
            ppr_dirty[i].1
        );
    }
}
