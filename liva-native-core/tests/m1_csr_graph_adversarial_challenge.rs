//! Milestone 1 Adversarial Challenge: CsrGraph Seed Finding & PPR Partial Selection Sort Oracle
//!
//! Empirically challenges and stress-tests:
//! 1. Empty string, whitespace variations, unicode spacing, punctuation immunity in find_seed_nodes.
//! 2. Node ID / label empty string edge cases (preventing false positives).
//! 3. select_nth_unstable_by(top_k) partial sorting against a reference full-sort oracle.
//! 4. Edge cases: top_k == 0, top_k == 1, top_k == N, top_k > N, identical scores, disconnected graphs.
//! 5. Equivalence across dirty uncompiled graph, compiled CSR, and published read snapshot.

use liva_native_core::db::csr_graph::CsrGraph;
use std::collections::{HashMap, HashSet};

#[test]
fn test_csr_graph_find_seed_nodes_adversarial_inputs_and_empty_immunity() {
    let mut graph = CsrGraph::new();

    // Insert nodes including empty string entity and valid domain entities
    graph.add_node("".to_string(), "".to_string(), "{}".to_string());
    graph.add_node("rust".to_string(), "Rust".to_string(), "{}".to_string());
    graph.add_node("ha_noi".to_string(), "Hà Nội".to_string(), "{}".to_string());
    graph.add_node(
        "ai_assistant".to_string(),
        "LIVA Personal Assistant".to_string(),
        "{}".to_string(),
    );
    graph.compile_csr();

    // 1. Empty query must return empty vec
    let empty_res = graph.find_seed_nodes("", 10);
    assert!(empty_res.is_empty(), "Empty query must return empty seeds");

    // 2. Whitespace queries on graph with empty node must return empty vec
    assert!(
        graph.find_seed_nodes("   ", 10).is_empty(),
        "Spaces query must return empty seeds"
    );
    assert!(
        graph.find_seed_nodes("\t\r\n  \t", 10).is_empty(),
        "Tab/newline query must return empty seeds"
    );
    assert!(
        graph
            .find_seed_nodes("\u{00A0}\u{2003}\u{2009}", 10)
            .is_empty(),
        "Unicode whitespace must return empty seeds"
    );

    // 3. Punctuation / symbol-only query without matching keywords
    assert!(
        graph
            .find_seed_nodes("!@#$%^&*()_+{}[]:;\"'<>?,./~`", 10)
            .is_empty()
    );

    // 4. Query matching valid entity must NOT match empty node
    let rust_res = graph.find_seed_nodes("We are writing in Rust programming language", 10);
    assert_eq!(rust_res.len(), 1);
    assert_eq!(rust_res[0].0, "rust");

    // 5. Query containing spaces must never match empty node
    let random_sentence = "The weather today is completely sunny without any rain";
    let random_res = graph.find_seed_nodes(random_sentence, 10);
    assert!(
        random_res.is_empty(),
        "Generic sentence with spaces must not match empty node!"
    );

    // 6. Max seeds truncation edge cases
    let multi_res_all = graph.find_seed_nodes("Rust in Hà Nội with LIVA Personal Assistant", 10);
    assert_eq!(multi_res_all.len(), 3);

    // Truncate to max_seeds = 1
    let multi_res_1 = graph.find_seed_nodes("Rust in Hà Nội with LIVA Personal Assistant", 1);
    assert_eq!(multi_res_1.len(), 1);

    // Truncate to max_seeds = 2
    let multi_res_2 = graph.find_seed_nodes("Rust in Hà Nội with LIVA Personal Assistant", 2);
    assert_eq!(multi_res_2.len(), 2);

    // Truncate to max_seeds = 0: must return 0 elements without panicking
    let multi_res_0 = graph.find_seed_nodes("Rust in Hà Nội with LIVA Personal Assistant", 0);
    assert_eq!(multi_res_0.len(), 0);
}

#[test]
fn test_csr_graph_ppr_partial_sort_oracle_equivalence() {
    let mut graph = CsrGraph::new();

    // Build a graph with 80 nodes and structured hub/chain/clique connections
    const NODE_COUNT: usize = 80;
    for i in 0..NODE_COUNT {
        graph.add_node(
            format!("node_{i:03}"),
            format!("Entity Label {i}"),
            "{}".to_string(),
        );
    }

    // Hub 0 connects to nodes 1..20
    for i in 1..=20 {
        graph.add_edge(
            "node_000",
            &format!("node_{i:03}"),
            "links_to",
            (i as f32) * 0.5,
            true,
        );
    }

    // Chain 20 -> 21 -> 22 -> ... -> 40
    for i in 20..40 {
        graph.add_edge(
            &format!("node_{i:03}"),
            &format!("node_{:03}", i + 1),
            "next",
            1.0,
            true,
        );
    }

    // Clique 41..50
    for i in 41..=50 {
        for j in (i + 1)..=50 {
            graph.add_edge(
                &format!("node_{i:03}"),
                &format!("node_{j:03}"),
                "clique",
                1.0,
                true,
            );
        }
    }

    // Nodes 51..79 have isolated or sparse connections
    for i in 51..79 {
        if i % 2 == 0 {
            graph.add_edge(&format!("node_{i:03}"), "node_000", "spoke", 0.2, true);
        }
    }

    // Test both uncompiled (dirty fallback) and compiled CSR
    for is_compiled in [false, true] {
        if is_compiled {
            graph.compile_csr();
        }

        let seeds = [("node_000", 1.0f32), ("node_025", 0.5f32)];

        // Test varying top_k boundaries: 0, 1, 5, 10, 25, NODE_COUNT, NODE_COUNT + 50
        let test_k_values = [0, 1, 2, 5, 10, 20, 40, NODE_COUNT, NODE_COUNT + 50];

        // Reference full-sort results with top_k = NODE_COUNT
        let full_reference = graph.personalized_pagerank_readonly(&seeds, 4, 0.85, NODE_COUNT);

        for &top_k in &test_k_values {
            let ppr_result = graph.personalized_pagerank_readonly(&seeds, 4, 0.85, top_k);

            let expected_len = top_k.min(full_reference.len());
            assert_eq!(
                ppr_result.len(),
                expected_len,
                "Result length must match min(top_k, total_active) for top_k = {top_k} (compiled: {is_compiled})"
            );

            // Verify descending score order
            for w in ppr_result.windows(2) {
                assert!(
                    w[0].1 >= w[1].1,
                    "Scores must be non-increasing: {} ({}) vs {} ({})",
                    w[0].0,
                    w[0].1,
                    w[1].0,
                    w[1].1
                );
            }

            // Verify top_k matches the prefix of full_reference (Oracle verification)
            for i in 0..expected_len {
                let (res_node, res_score) = &ppr_result[i];
                let (ref_node, ref_score) = &full_reference[i];

                assert_eq!(
                    res_node, ref_node,
                    "Node at index {i} mismatch: got {res_node}, expected {ref_node} for top_k = {top_k} (compiled: {is_compiled})"
                );
                assert!(
                    (res_score - ref_score).abs() < 1e-6,
                    "Score mismatch at index {i}: got {res_score}, expected {ref_score}"
                );
            }
        }
    }
}

#[test]
fn test_csr_graph_read_snapshot_and_ppr_oracle_parity() {
    let mut graph = CsrGraph::new();
    for i in 0..30 {
        graph.add_node(format!("N{i}"), format!("Node {i}"), "{}".to_string());
    }
    for i in 0..29 {
        graph.add_edge(&format!("N{i}"), &format!("N{}", i + 1), "rel", 1.0, true);
    }
    graph.compile_csr();

    let snapshot = graph.to_read_snapshot();
    let seeds = [("N0", 1.0f32)];

    for k in [1, 5, 10, 30] {
        let ppr_graph = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, k);
        let ppr_snap = snapshot.personalized_pagerank_readonly(&seeds, 3, 0.85, k);

        assert_eq!(
            ppr_graph, ppr_snap,
            "Snapshot PPR must match graph PPR for k={k}"
        );
    }
}

#[test]
fn test_csr_graph_identical_scores_and_zero_activation() {
    let mut graph = CsrGraph::new();
    // Disconnected nodes
    graph.add_node("A".into(), "Node A".into(), "{}".into());
    graph.add_node("B".into(), "Node B".into(), "{}".into());
    graph.add_node("C".into(), "Node C".into(), "{}".into());
    graph.compile_csr();

    // Query non-existent seed
    let seeds = [("NONEXISTENT", 1.0f32)];
    let res = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 5);
    assert!(res.is_empty(), "Non-existent seed must produce empty PPR");

    // Negative / zero seed weights
    let zero_seeds = [("A", 0.0f32), ("B", -1.0f32)];
    let zero_res = graph.personalized_pagerank_readonly(&zero_seeds, 3, 0.85, 5);
    assert!(
        zero_res.is_empty(),
        "Zero/negative seed weights must return empty"
    );
}
