use std::fs;
use tempfile::tempdir;

use liva_native_core::db::DatabasePool;
use liva_native_core::memory::graph::{CsrGraph, HippoRagEngine, PprConfig};
use liva_native_core::memory::l3_semantic::{
    extract_wikilinks, parse_frontmatter,
    validate_and_resolve_path, validate_frontmatter, Frontmatter, ObsidianVaultSync,
    VaultSecurityError,
};

// =========================================================================
// 1. Path Containment & Security Tests (validate_and_resolve_path)
// =========================================================================

#[test]
fn test_path_containment_valid_paths() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    fs::create_dir_all(root.join("Knowledge")).unwrap();
    fs::write(root.join("Knowledge").join("test.md"), "content").unwrap();

    let resolved = validate_and_resolve_path(root, "Knowledge/test.md").unwrap();
    assert!(resolved.exists());
    assert!(resolved.ends_with("Knowledge/test.md"));

    // Subdirectory and leading slashes
    let resolved_slash = validate_and_resolve_path(root, "/Knowledge/test.md").unwrap();
    assert_eq!(resolved, resolved_slash);
}

#[test]
fn test_path_containment_rejects_parent_traversal() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let err = validate_and_resolve_path(root, "../secret.txt").unwrap_err();
    assert!(matches!(err, VaultSecurityError::EscapesVault(_)));

    let err2 = validate_and_resolve_path(root, "Knowledge/../../etc/passwd").unwrap_err();
    assert!(matches!(err2, VaultSecurityError::EscapesVault(_)));
}

#[test]
fn test_path_containment_url_encoded_traversal() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    // %2e%2e%2f -> ../
    let err = validate_and_resolve_path(root, "%2e%2e%2fsecret.txt").unwrap_err();
    assert!(matches!(err, VaultSecurityError::EscapesVault(_)));

    // %2e%2e%5c -> ..\
    let err2 = validate_and_resolve_path(root, "%2e%2e%5csecret.txt").unwrap_err();
    assert!(matches!(err2, VaultSecurityError::EscapesVault(_)));
}

#[test]
fn test_path_containment_null_bytes_and_control_chars() {
    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let err_null = validate_and_resolve_path(root, "Knowledge/test\0.md").unwrap_err();
    assert_eq!(err_null, VaultSecurityError::ContainsNullBytes);

    let err_null_encoded = validate_and_resolve_path(root, "Knowledge/test%00.md").unwrap_err();
    assert_eq!(err_null_encoded, VaultSecurityError::ContainsNullBytes);

    let err_ctrl = validate_and_resolve_path(root, "Knowledge/test\x1f.md").unwrap_err();
    assert_eq!(err_ctrl, VaultSecurityError::ContainsControlChars);
}

#[cfg(unix)]
#[test]
fn test_path_containment_symlink_loop_detection() {
    use std::os::unix::fs::symlink;

    let tmp = tempdir().unwrap();
    let root = tmp.path();

    let link_a = root.join("link_a");
    let link_b = root.join("link_b");

    // Circular symlinks: link_a -> link_b and link_b -> link_a
    symlink(&link_b, &link_a).unwrap();
    symlink(&link_a, &link_b).unwrap();

    let err = validate_and_resolve_path(root, "link_a").unwrap_err();
    assert!(matches!(err, VaultSecurityError::SymlinkLoopDetected(_)));
}

// =========================================================================
// 2. YAML Frontmatter & Schema Validation Tests
// =========================================================================

#[test]
fn test_frontmatter_parsing_and_schema_validation() {
    let content = r#"---
title: MemoryArchitecture
author: System Architect
tags: [liva/knowledge, liva/memory, architecture]
last_update: 2026-09-01T12:00:00Z
status: active
---

# Document Body
This is the body of the memory architecture.
"#;

    let (fm, body) = parse_frontmatter(content).unwrap();
    assert_eq!(fm.title, "MemoryArchitecture");
    assert_eq!(fm.author, "System Architect");
    assert_eq!(fm.tags, vec!["liva/knowledge", "liva/memory", "architecture"]);
    assert_eq!(fm.last_update, "2026-09-01T12:00:00Z");
    assert_eq!(fm.status.as_deref(), Some("active"));
    assert!(body.contains("# Document Body"));

    let (errors, warnings) = validate_frontmatter(&fm, "MemoryArchitecture", false);
    assert!(errors.is_empty(), "Errors: {:?}", errors);
    assert!(warnings.is_empty(), "Warnings: {:?}", warnings);
}

#[test]
fn test_frontmatter_validation_errors() {
    // Missing liva/ tag and bad date
    let fm_invalid = Frontmatter {
        title: "WrongTitle".to_string(),
        tags: vec!["unprefixed_tag".to_string()],
        author: "".to_string(),
        last_update: "invalid-date".to_string(),
        status: None,
        custom: Default::default(),
    };

    let (errors, _warnings) = validate_frontmatter(&fm_invalid, "ActualFilename", false);
    assert!(errors.iter().any(|e| e.contains("does not match filename")));
    assert!(errors.iter().any(|e| e.contains("At least one tag must start with 'liva/'")));
    assert!(errors.iter().any(|e| e.contains("'author'")));
    assert!(errors.iter().any(|e| e.contains("valid ISO 8601 datetime")));
}

// =========================================================================
// 3. [[wikilinks]] Parser Tests
// =========================================================================

#[test]
fn test_wikilinks_extraction_all_formats() {
    let body = r#"
Overview of the system:
1. Standard link: [[RadixTree]]
2. Link with section anchor: [[RadixTree#CacheEviction]]
3. Link with alias: [[EpisodicMemory|Episodic Memory Layer]]
4. Link with section and alias: [[EpisodicMemory#HalfLife|Half Life Decay Formula]]
5. Multiple on one line: Check [[KnowledgeA]] and also [[KnowledgeB#Details|Section B]].
"#;

    let links = extract_wikilinks(body);
    assert_eq!(links.len(), 6);

    assert_eq!(links[0].target, "RadixTree");
    assert_eq!(links[0].section, None);
    assert_eq!(links[0].alias, None);
    assert_eq!(links[0].line, 3);

    assert_eq!(links[1].target, "RadixTree");
    assert_eq!(links[1].section.as_deref(), Some("CacheEviction"));
    assert_eq!(links[1].alias, None);

    assert_eq!(links[2].target, "EpisodicMemory");
    assert_eq!(links[2].section, None);
    assert_eq!(links[2].alias.as_deref(), Some("Episodic Memory Layer"));

    assert_eq!(links[3].target, "EpisodicMemory");
    assert_eq!(links[3].section.as_deref(), Some("HalfLife"));
    assert_eq!(links[3].alias.as_deref(), Some("Half Life Decay Formula"));

    assert_eq!(links[4].target, "KnowledgeA");
    assert_eq!(links[5].target, "KnowledgeB");
    assert_eq!(links[5].section.as_deref(), Some("Details"));
    assert_eq!(links[5].alias.as_deref(), Some("Section B"));
}

// =========================================================================
// 4. Obsidian Vault 2-Way Synchronization & SQLite Integration Tests
// =========================================================================

#[test]
fn test_obsidian_vault_full_scan_and_sqlite_sync() {
    let tmp = tempdir().unwrap();
    let root = tmp.path().to_path_buf();

    // Create required directories
    for dir in &["Skills", "Knowledge", "Rules", "Templates"] {
        fs::create_dir_all(root.join(dir)).unwrap();
    }

    // Create required templates
    fs::write(
        root.join("Templates").join("Skill Template.md"),
        "---\ntitle: {{title}}\ntags: [liva/skill]\nauthor: LIVA\nlast_update: 2026-09-01T00:00:00Z\n---\n",
    ).unwrap();
    fs::write(
        root.join("Templates").join("Knowledge Template.md"),
        "---\ntitle: {{title}}\ntags: [liva/knowledge]\nauthor: LIVA\nlast_update: 2026-09-01T00:00:00Z\n---\n",
    ).unwrap();
    fs::write(
        root.join("Templates").join("Rule Template.md"),
        "---\ntitle: {{title}}\ntags: [liva/rule]\nauthor: LIVA\nlast_update: 2026-09-01T00:00:00Z\n---\n",
    ).unwrap();

    // Create knowledge note A with link to note B
    fs::write(
        root.join("Knowledge").join("GraphArchitecture.md"),
        r#"---
title: GraphArchitecture
author: System Architect
tags: [liva/knowledge, liva/graph]
last_update: 2026-09-01T10:00:00Z
---
This document references [[HippoRAGAlgorithm#PPR]] and [[SkillRunner]].
"#,
    ).unwrap();

    // Create knowledge note B
    fs::write(
        root.join("Knowledge").join("HippoRAGAlgorithm.md"),
        r#"---
title: HippoRAGAlgorithm
author: System Architect
tags: [liva/knowledge, liva/algorithm]
last_update: 2026-09-01T10:00:00Z
---
HippoRAG Personalized PageRank algorithm details.
"#,
    ).unwrap();

    // Create skill note
    fs::write(
        root.join("Skills").join("SkillRunner.md"),
        r#"---
title: SkillRunner
author: Skill Author
tags: [liva/skill]
last_update: 2026-09-01T10:00:00Z
---
Skill execution engine.
"#,
    ).unwrap();

    // Create SQLite pool for L3 DB Sync
    let pool = DatabasePool::new_in_memory().unwrap();
    let mut sync_engine = ObsidianVaultSync::new(root.clone(), Some(pool.clone())).unwrap();

    let scan_report = sync_engine.scan_vault().unwrap();
    assert_eq!(scan_report.broken_links_count, 0, "No broken links expected: {:?}", scan_report.file_results);
    assert_eq!(scan_report.total_files_checked, 6); // 3 templates + 3 notes
    assert_eq!(scan_report.invalid_files_count, 0);

    // Sync to SQLite l3_nodes and l3_edges
    let (nodes_synced, edges_synced) = sync_engine.sync_to_db().unwrap();
    assert_eq!(nodes_synced, 3);
    assert_eq!(edges_synced, 2);

    // Query SQLite database to verify persistence
    let conn = pool.readers.get().unwrap();
    let node_count: i64 = conn.query_row("SELECT COUNT(*) FROM l3_nodes", [], |r| r.get(0)).unwrap();
    assert_eq!(node_count, 3);

    let edge_count: i64 = conn.query_row("SELECT COUNT(*) FROM l3_edges", [], |r| r.get(0)).unwrap();
    assert_eq!(edge_count, 2);

    // Build CSR Graph directly from vault notes
    let csr = sync_engine.build_csr_graph(false);
    assert_eq!(csr.node_count(), 3);
    assert_eq!(csr.edge_count(), 2);

    // Test 2-way write note
    let new_note = sync_engine.write_note(
        "Rules/SafetyRules.md",
        r#"---
title: SafetyRules
author: Security Officer
tags: [liva/rule]
last_update: 2026-09-01T12:00:00Z
---
Safety rules linking [[GraphArchitecture]].
"#,
    ).unwrap();
    assert_eq!(new_note.title, "SafetyRules");
    assert_eq!(new_note.links.len(), 1);
    assert!(root.join("Rules").join("SafetyRules.md").exists());
}

// =========================================================================
// 5. CSR Matrix & HippoRAG Personalized PageRank (PPR) Mathematical Tests
// =========================================================================

#[test]
fn test_csr_matrix_construction_and_transpose() {
    // Construct small graph:
    // 0 -> 1 (w=1.0)
    // 0 -> 2 (w=3.0)  ==> row 0 norm: 0->1 (0.25), 0->2 (0.75)
    // 1 -> 2 (w=2.0)  ==> row 1 norm: 1->2 (1.00)
    // 2 is a sink (out-degree 0)
    let edges = vec![
        ("Node0", "Node1", 1.0f32),
        ("Node0", "Node2", 3.0f32),
        ("Node1", "Node2", 2.0f32),
    ];

    let graph = CsrGraph::from_named_edges(&edges, false);
    assert_eq!(graph.node_count(), 3);
    assert_eq!(graph.edge_count(), 3);

    let idx0 = graph.node_index("Node0").unwrap();
    let idx1 = graph.node_index("Node1").unwrap();
    let idx2 = graph.node_index("Node2").unwrap();

    let (neighbors0, weights0) = graph.out_neighbors(idx0);
    assert_eq!(neighbors0.len(), 2);
    assert!((weights0[0] - 0.25).abs() < 1e-6);
    assert!((weights0[1] - 0.75).abs() < 1e-6);

    let (neighbors2, _) = graph.out_neighbors(idx2);
    assert_eq!(neighbors2.len(), 0); // Sink node

    // Test Transpose:
    // In transposed graph:
    // Node2 has in-edges from Node0 (0.75) and Node1 (1.00)
    let transposed = graph.transpose();
    let (in_neighbors2, _in_weights2) = transposed.out_neighbors(idx2);
    assert_eq!(in_neighbors2.len(), 2);
    assert!(in_neighbors2.contains(&idx0));
    assert!(in_neighbors2.contains(&idx1));
}

#[test]
fn test_hipporag_ppr_probability_conservation_and_convergence() {
    // 5-node cyclic graph with cross links
    let edges = vec![
        ("A", "B", 1.0f32),
        ("B", "C", 1.0f32),
        ("C", "D", 1.0f32),
        ("D", "E", 1.0f32),
        ("E", "A", 1.0f32),
        ("A", "C", 2.0f32),
    ];

    let graph = CsrGraph::from_named_edges(&edges, false);
    let engine = HippoRagEngine::new(graph);

    // Seed preference concentrated on "A"
    let seeds = vec![("A", 1.0f32)];
    let top_results = engine.run_ppr_top_k(&seeds, 5);

    assert_eq!(top_results.len(), 5);
    // Node A should receive highest rank due to direct teleportation + return flow
    assert_eq!(top_results[0].0, "A");

    // Conservation check: total probability mass must sum to 1.0
    let sum_prob: f32 = top_results.iter().map(|(_, p)| *p).sum();
    assert!(
        (sum_prob - 1.0).abs() < 1e-4,
        "Total probability mass must be conserved to 1.0, got: {}",
        sum_prob
    );
}

#[test]
fn test_hipporag_ppr_dangling_sink_node_handling() {
    // Star graph with sink leaf nodes: Central hub A points to Sinks B, C, D (no outgoing edges)
    let edges = vec![
        ("Hub", "Sink1", 1.0f32),
        ("Hub", "Sink2", 1.0f32),
        ("Hub", "Sink3", 1.0f32),
    ];

    let graph = CsrGraph::from_named_edges(&edges, false);
    let engine = HippoRagEngine::new(graph);

    let seeds = vec![("Hub", 1.0f32)];
    let results = engine.run_ppr_top_k(&seeds, 4);

    let sum_prob: f32 = results.iter().map(|(_, p)| *p).sum();
    assert!(
        (sum_prob - 1.0).abs() < 1e-4,
        "Probability mass with sink nodes must still strictly sum to 1.0, got: {}",
        sum_prob
    );
}

#[test]
fn test_hipporag_ppr_empty_seed_fallback_uniform_teleportation() {
    let edges = vec![
        ("X", "Y", 1.0f32),
        ("Y", "Z", 1.0f32),
        ("Z", "X", 1.0f32),
    ];

    let graph = CsrGraph::from_named_edges(&edges, false);
    let engine = HippoRagEngine::new(graph);

    // Empty seed vector -> fallback to uniform distribution
    let ppr_res = engine.run_ppr(&[], &[]);
    let sum_prob: f32 = ppr_res.probabilities.iter().sum();
    assert!((sum_prob - 1.0).abs() < 1e-4);

    // In a symmetric 3-node ring with uniform teleportation, all stationary probs should be ~ 1/3
    for &prob in &ppr_res.probabilities {
        assert!((prob - 0.333333).abs() < 1e-3);
    }
}

// =========================================================================
// 6. Large Scale HippoRAG 100k Node & 1M Edge Latency SLA Benchmark Test
//    Requirement R3 & Acceptance Criteria: <= 8.0ms latency, P95 <= 8.5ms
// =========================================================================

#[test]
fn test_hipporag_100k_nodes_latency_sla() {
    const NUM_NODES: usize = 100_000;
    const NUM_EDGES: usize = 1_000_000;

    println!("\n========================================================");
    println!("Generating 100,000 node & 1,000,000 edge synthetic graph...");
    let gen_start = std::time::Instant::now();
    let graph = HippoRagEngine::generate_synthetic_graph(NUM_NODES, NUM_EDGES);
    println!(
        "Graph generation completed in {:.2}ms (RAM: {:.2} MB)",
        gen_start.elapsed().as_secs_f64() * 1000.0,
        graph.memory_usage_bytes() as f64 / (1024.0 * 1024.0)
    );

    assert_eq!(graph.node_count(), NUM_NODES);
    assert!(graph.edge_count() >= 900_000);

    let engine = HippoRagEngine::with_config(
        graph,
        PprConfig {
            damping_factor: 0.15,
            max_iterations: 20,
            tolerance: 1e-6,
            chunk_size: 512,
        },
    );

    // Warm-up run
    let seed_indices = vec![42, 100, 500, 1000, 5000];
    let seed_weights = vec![1.0, 2.0, 1.5, 0.5, 3.0];
    let _warmup = engine.run_ppr(&seed_indices, &seed_weights);

    // Execute 20 timed evaluation runs
    const EVAL_RUNS: usize = 20;
    let mut latencies_ms = Vec::with_capacity(EVAL_RUNS);

    for run in 0..EVAL_RUNS {
        let seeds = vec![
            (run as u32) * 1000 % (NUM_NODES as u32),
            ((run + 1) as u32) * 2500 % (NUM_NODES as u32),
            ((run + 2) as u32) * 7000 % (NUM_NODES as u32),
        ];
        let weights = vec![2.0, 1.0, 0.5];

        let res = engine.run_ppr(&seeds, &weights);
        latencies_ms.push(res.elapsed_ms);

        let sum_mass: f32 = res.probabilities.iter().sum();
        assert!(
            (sum_mass - 1.0).abs() < 1e-3,
            "Probability conservation violated: {}",
            sum_mass
        );
    }

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mean_ms: f64 = latencies_ms.iter().sum::<f64>() / (EVAL_RUNS as f64);
    let median_ms = latencies_ms[EVAL_RUNS / 2];
    let p95_idx = ((EVAL_RUNS as f64) * 0.95).floor() as usize;
    let p95_ms = latencies_ms[p95_idx.min(EVAL_RUNS - 1)];
    let min_ms = latencies_ms[0];
    let max_ms = latencies_ms[EVAL_RUNS - 1];

    println!("--------------------------------------------------------");
    println!("HippoRAG Parallel Rayon PPR Performance Summary (100k nodes):");
    println!("  Min Latency:    {:.3} ms", min_ms);
    println!("  Mean Latency:   {:.3} ms (Target <= 8.0 ms)", mean_ms);
    println!("  Median Latency: {:.3} ms", median_ms);
    println!("  P95 Latency:    {:.3} ms (Target <= 8.5 ms)", p95_ms);
    println!("  Max Latency:    {:.3} ms", max_ms);
    println!("========================================================\n");

    let max_mean = if cfg!(debug_assertions) { 300.0 } else { 8.0 };
    let max_p95 = if cfg!(debug_assertions) { 500.0 } else { 8.5 };

    assert!(
        mean_ms <= max_mean,
        "Mean latency must be <= {:.1}ms in {} mode, got: {:.3}ms",
        max_mean,
        if cfg!(debug_assertions) { "debug" } else { "release" },
        mean_ms
    );
    assert!(
        p95_ms <= max_p95,
        "P95 latency must be <= {:.1}ms in {} mode, got: {:.3}ms",
        max_p95,
        if cfg!(debug_assertions) { "debug" } else { "release" },
        p95_ms
    );
}
