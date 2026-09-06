//! Adversarial Stress & Chaos Test Suite for Milestone 2:
//! LLMLingua-2 Context Compression & Recursive Summary Tree
//!
//! Dimension 1: Deep recursive hierarchies (100+ turns, 500+ turns, multi-tier condensation)
//! Dimension 2: Extreme compression targets (90%-99% reduction, boundary token distributions)
//! Dimension 3: Nested XML, unicode, emojis, malformed tags, and multi-line code blocks
//! Dimension 4: Merkle tree cryptographic integrity validation under tampering
//! Dimension 5: High-load benchmarking & latency profiling

use tempfile::NamedTempFile;

use liva_native_core::db::DatabasePool;
use liva_native_core::memory::compression::{
    DialogueTurn, LLMLinguaCompressor, LLMLinguaConfig,
    RecursiveSummaryTree, SummaryNode, SummaryTreeConfig,
};

// ============================================================================
// DIMENSION 1: DEEP RECURSIVE HIERARCHIES (100+ TURNS & MULTI-CYCLE CONDENSATION)
// ============================================================================

#[test]
fn test_stress_deep_recursive_hierarchy_150_turns() {
    let tmp = NamedTempFile::new().unwrap();
    let db_path = tmp.path().to_path_buf();
    let pool = DatabasePool::new(&db_path).unwrap();

    let config = SummaryTreeConfig {
        max_context_tokens: 300,
        overflow_threshold_ratio: 0.70, // 210 tokens
        chunk_target_tokens: 50,
        min_turns_per_chunk: 2,
        max_turns_per_chunk: 4,
        meta_summary_fanout: 3,
        domain: "StressTesting".to_string(),
        category: "DeepHierarchy".to_string(),
    };

    let mut tree = RecursiveSummaryTree::new("session_deep_150", config).with_db_pool(pool.clone());
    let mut total_overflow_triggers = 0;

    // Ingest 150 dialogue turns
    for i in 1..=150 {
        let role = if i % 2 == 1 { "user" } else { "assistant" };
        let content = format!(
            "Turn #{}: Discussing topic [[Topic-{:03}]] regarding engineering milestone {} with parameter value {} and metrics.",
            i, i % 15, i, i * 10
        );
        let turn = DialogueTurn::new("session_deep_150", role, &content, 1000 + i as i64);

        if let Some(report) = tree.add_turn(turn) {
            if report.triggered {
                total_overflow_triggers += 1;
            }
        }
    }

    // Final flush
    if tree.is_overflow() {
        let rep = tree.condense_overflow();
        if rep.triggered {
            total_overflow_triggers += 1;
        }
    }

    assert!(total_overflow_triggers >= 10, "Must have triggered at least 10 condensation cycles, got {}", total_overflow_triggers);

    let all_summaries = tree.all_summaries();
    assert!(all_summaries.len() >= 15, "Should have produced extensive summary nodes, got {}", all_summaries.len());

    // Verify presence of Level 1 and Level 2+ nodes
    let level1_count = all_summaries.iter().filter(|s| s.level == 1).count();
    let level2_count = all_summaries.iter().filter(|s| s.level >= 2).count();
    assert!(level1_count >= 10, "Must have >= 10 Level 1 summaries, got {}", level1_count);
    assert!(level2_count >= 1, "Must have >= 1 Level 2+ meta-summaries, got {}", level2_count);

    // Verify cryptographic integrity of every single generated node
    for node in &all_summaries {
        assert!(node.verify_integrity(), "Node {} (level {}) failed Merkle integrity verification", node.node_id, node.level);
    }

    // Verify lineage drill-down from meta-summary covers constituent turns
    if let Some(meta) = all_summaries.iter().find(|s| s.level >= 2) {
        let turn_ids = tree.drill_down_turn_ids(&meta.node_id);
        assert!(!turn_ids.is_empty(), "Meta summary must trace back to raw turn IDs");
        assert!(turn_ids.len() >= 6, "Meta summary must aggregate multiple turns, got {}", turn_ids.len());
    }

    // Verify SQLite persistence
    let reader = pool.readers.get().unwrap();
    let count: i64 = reader.query_row("SELECT COUNT(*) FROM episodes WHERE session_id = 'session_deep_150'", [], |r| r.get(0)).unwrap();
    assert!(count >= 15, "All summary nodes must be persisted to SQLite episodes, got {}", count);
}

#[test]
fn test_stress_rapid_oversized_turn_ingestion() {
    let config = SummaryTreeConfig {
        max_context_tokens: 100,
        overflow_threshold_ratio: 0.80, // 80 tokens
        chunk_target_tokens: 30,
        min_turns_per_chunk: 2,
        max_turns_per_chunk: 10,
        meta_summary_fanout: 2,
        domain: "Stress".to_string(),
        category: "Oversized".to_string(),
    };

    let mut tree = RecursiveSummaryTree::new("session_giant", config);

    // Ingest a massive turn with 5,000 words (exceeding context limit by 50x in a single turn)
    let giant_content = "Word ".repeat(5000);
    let giant_turn = DialogueTurn::new("session_giant", "user", &giant_content, 1);
    let _rep1 = tree.add_turn(giant_turn);

    // Should not panic on a single turn even if overflowed (cannot condense when turns < min_turns_per_chunk)
    assert_eq!(tree.active_turn_count(), 1);

    // Add second turn: now min_turns_per_chunk (2) is satisfied, should trigger condensation immediately
    let t2 = DialogueTurn::new("session_giant", "assistant", "Acknowledged giant input.", 2);
    let rep2 = tree.add_turn(t2);

    assert!(rep2.is_some(), "Must trigger condensation upon reaching 2 turns");
    let report = rep2.unwrap();
    assert!(report.triggered);
    assert_eq!(report.condensed_turn_count, 2);
    assert_eq!(tree.active_turn_count(), 0);
}

// ============================================================================
// DIMENSION 2: EXTREME COMPRESSION TARGETS & EDGE CASE DISTRIBUTIONS
// ============================================================================

#[test]
fn test_stress_extreme_compression_ratios() {
    let text = "\
The quick brown fox jumps over the lazy dog. In computer science and artificial intelligence, \
context compression aims to minimize prompt token count while maximizing preserved semantic information. \
Furthermore, LLMLingua-2 provides deterministic entropy-based selective pruning with protected masks.";

    // 90% reduction
    let mut config_90 = LLMLinguaConfig::default();
    config_90.target_reduction_ratio = Some(0.90);
    let comp_90 = LLMLinguaCompressor::with_config(config_90);
    let res_90 = comp_90.compress(text);
    assert!(!res_90.compressed_text.is_empty());
    assert!(res_90.metrics.reduction_ratio >= 0.70, "Got {:.2}%", res_90.metrics.reduction_ratio * 100.0);

    // 95% reduction
    let mut config_95 = LLMLinguaConfig::default();
    config_95.target_reduction_ratio = Some(0.95);
    let comp_95 = LLMLinguaCompressor::with_config(config_95);
    let res_95 = comp_95.compress(text);
    assert!(!res_95.compressed_text.is_empty());

    // 99% reduction (near maximum clamp)
    let mut config_99 = LLMLinguaConfig::default();
    config_99.target_reduction_ratio = Some(0.99);
    let comp_99 = LLMLinguaCompressor::with_config(config_99);
    let res_99 = comp_99.compress(text);
    assert!(!res_99.compressed_text.is_empty());
    assert!(res_99.metrics.compressed_tokens >= 1, "Must retain at least 1 token");
}

#[test]
fn test_stress_edge_case_distributions() {
    let compressor = LLMLinguaCompressor::new();

    // 1. Repetitive identical tokens
    let repetitive = "repeat ".repeat(1000);
    let res_rep = compressor.compress(&repetitive);
    assert!(!res_rep.compressed_text.is_empty());
    assert!(res_rep.metrics.compressed_tokens < res_rep.metrics.original_tokens);

    // 2. All tokens are protected (100% protected input)
    let all_protected = "<SYSTEM_CONTEXT>[[EntityA]] [[EntityB]] $100 2026-09-02 75.5% 8.0ms ```rust fn main(){} ```</SYSTEM_CONTEXT>";
    let res_prot = compressor.compress(all_protected);
    assert_eq!(res_prot.metrics.preserved_protected_count, res_prot.metrics.protected_tokens_count);
    assert!(res_prot.compressed_text.contains("<SYSTEM_CONTEXT>"));
    assert!(res_prot.compressed_text.contains("[[EntityA]]"));
    assert!(res_prot.compressed_text.contains("```rust"));

    // 3. String with only whitespace / newlines / tabs
    let empty_like = "   \n\n\t\t   \r\n   ";
    let res_empty = compressor.compress(empty_like);
    assert_eq!(res_empty.compressed_text, "");
    assert_eq!(res_empty.metrics.original_tokens, 0);

    // 4. Very long single unspaced word (2000 chars)
    let giant_token = "A".repeat(2000);
    let res_giant = compressor.compress(&giant_token);
    assert_eq!(res_giant.metrics.original_tokens, 1);
    assert!(!res_giant.compressed_text.is_empty());
}

// ============================================================================
// DIMENSION 3: NESTED XML, UNICODE, EMOJIS, MALFORMED TAGS, MULTI-LINE CODE
// ============================================================================

#[test]
fn test_stress_nested_and_malformed_xml() {
    let compressor = LLMLinguaCompressor::new();

    let complex_xml = "\
<SYSTEM_CONTEXT>
<role_spec>
<memory_partition id=\"enclave_01\">
Nested system content that must strictly be protected.
</memory_partition>
</role_spec>
</SYSTEM_CONTEXT>

<context_memory>
<unclosed_tag attribute=\"val\">
Summary text with broken inner XML: <tag1> <tag2/> </tag3>
</context_memory>

Malformed tags: <<<>>> < /invalid tag > <> </>";

    let result = compressor.compress(complex_xml);

    assert!(result.compressed_text.contains("<SYSTEM_CONTEXT>"));
    assert!(result.compressed_text.contains("</SYSTEM_CONTEXT>"));
    assert!(result.compressed_text.contains("<context_memory>"));
    assert!(result.compressed_text.contains("</context_memory>"));
    assert!(result.compressed_text.contains("Nested system content"));
}

#[test]
fn test_stress_multilingual_unicode_and_emojis() {
    let compressor = LLMLinguaCompressor::new();

    let multilingual_text = "\
<SYSTEM_CONTEXT>
Hệ thống LIVA sử dụng kiến trúc bộ nhớ phân tầng L1–L4 kết hợp với Obsidian Graph RAG.
</SYSTEM_CONTEXT>

Tiếng Việt có dấu: Kiểm thử thuật toán nén ngữ cảnh LLMLingua-2 với các từ dừng như và, của, là, được, trong, có, với.
日本語テキスト: 日本語の形態素解析と階層的要約ツリーの圧縮テスト。
中文测试: 深度上下文压缩与递归摘要树验证，确保实体与关键信息完整保留。
RTL Arabic: ضغط السياق وشجرة الملخص العودية مع الحفاظ على الكيانات.
Emojis & ZWJ: 🚀🔥💻 👩‍💻👨‍👩‍👧‍👦 🎉✨ [[Wikilink_Đặc_Biệt]] $99.99 2026-09-02.";

    let result = compressor.compress(multilingual_text);

    // Validate no UTF-8 character boundary corruption or panic
    assert!(result.compressed_text.contains("Hệ thống LIVA"));
    assert!(result.compressed_text.contains("<SYSTEM_CONTEXT>"));
    assert!(result.compressed_text.contains("[[Wikilink_Đặc_Biệt]]"));
    assert!(result.compressed_text.contains("$99.99"));
    assert!(result.compressed_text.contains("2026-09-02"));
}

#[test]
fn test_stress_multiline_code_blocks_with_special_content() {
    let compressor = LLMLinguaCompressor::new();

    let code_payload = "\
Here is an intricate Rust implementation with embedded XML, JSON, and regex:
```rust
pub fn parse_data(raw: &str) -> Result<Payload, Error> {
    // <SYSTEM_CONTEXT> inside comment
    let json_str = r#\"{\"key\": 123, \"flags\": [true, false]}\"#;
    let wikilink_pattern = regex::Regex::new(r\"\\[\\[(.*?)\\]\\]\").unwrap();
    println!(\"Processing: {}\", raw);
    Ok(Payload { count: 42 })
}
```
And inline code `let x: f64 = 1.234;` along with `[[TargetEngine]]`.";

    let result = compressor.compress(code_payload);

    // Code block fences and contents must be preserved intact
    assert!(result.compressed_text.contains("```rust"));
    assert!(result.compressed_text.contains("pub fn parse_data"));
    assert!(result.compressed_text.contains("println!"));
    assert!(result.compressed_text.contains("`let x: f64 = 1.234;`") || result.compressed_text.contains("let x: f64 = 1.234;"));
    assert!(result.compressed_text.contains("[[TargetEngine]]"));
}

// ============================================================================
// DIMENSION 4: MERKLE TREE CRYPTOGRAPHIC INTEGRITY & TAMPERING
// ============================================================================

#[test]
fn test_stress_merkle_tree_tampering_detection() {
    let mut turns = Vec::new();
    let turn1 = DialogueTurn::with_id("t_01", "sess", "user", "Alpha message", 100);
    let turn2 = DialogueTurn::with_id("t_02", "sess", "assistant", "Beta response", 101);
    turns.push(turn1);
    turns.push(turn2);

    let title = "Summary Test".to_string();
    let abstract_text = "Abstract content covering Alpha and Beta.".to_string();
    let source_turn_ids = vec!["t_01".to_string(), "t_02".to_string()];
    let child_node_ids = vec![];

    let hash = SummaryNode::compute_merkle_hash(1, &title, &abstract_text, &source_turn_ids, &child_node_ids);

    let legitimate_node = SummaryNode {
        node_id: "node_100".to_string(),
        session_id: "sess".to_string(),
        level: 1,
        title: title.clone(),
        abstract_text: abstract_text.clone(),
        key_entities: vec!["Alpha".to_string(), "Beta".to_string()],
        source_turn_ids: source_turn_ids.clone(),
        child_node_ids: child_node_ids.clone(),
        merkle_hash: hash.clone(),
        token_count: 50,
        created_at: 1000,
        vector_embedding: None,
    };

    // 1. Legitimate node passes verification
    assert!(legitimate_node.verify_integrity(), "Legitimate node must pass integrity verification");

    // 2. Tampering Attack A: Single character modification in abstract_text
    let mut tampered_text = legitimate_node.clone();
    tampered_text.abstract_text = "Abstract content covering Alpha and Betaz.".to_string();
    assert!(!tampered_text.verify_integrity(), "Tampered abstract_text must fail verification");

    // 3. Tampering Attack B: Title alteration
    let mut tampered_title = legitimate_node.clone();
    tampered_title.title = "Summary Test (Modified)".to_string();
    assert!(!tampered_title.verify_integrity(), "Tampered title must fail verification");

    // 4. Tampering Attack C: Level elevation (Privilege escalation from Level 1 to Level 2)
    let mut tampered_level = legitimate_node.clone();
    tampered_level.level = 2;
    assert!(!tampered_level.verify_integrity(), "Tampered level must fail verification");

    // 5. Tampering Attack D: Injection of forged turn_id
    let mut tampered_turns = legitimate_node.clone();
    tampered_turns.source_turn_ids.push("forged_turn_999".to_string());
    assert!(!tampered_turns.verify_integrity(), "Injected turn ID must fail verification");

    // 6. Tampering Attack E: Deletion of constituent turn_id
    let mut tampered_turns_del = legitimate_node.clone();
    tampered_turns_del.source_turn_ids.pop();
    assert!(!tampered_turns_del.verify_integrity(), "Removed turn ID must fail verification");

    // 7. Canonical Turn Order Stability: Reordered source_turn_ids should still compute same canonical hash
    let mut reordered_turns = legitimate_node.clone();
    reordered_turns.source_turn_ids.swap(0, 1);
    assert!(reordered_turns.verify_integrity(), "Source turn IDs must be canonically sorted during hashing");

    // 8. Tampering Attack F: Child node tampering in Level 2 Meta-Summary
    let meta_hash = SummaryNode::compute_merkle_hash(2, "Meta", "Meta Abstract", &source_turn_ids, &["child_1".to_string(), "child_2".to_string()]);
    let mut meta_node = SummaryNode {
        node_id: "meta_200".to_string(),
        session_id: "sess".to_string(),
        level: 2,
        title: "Meta".to_string(),
        abstract_text: "Meta Abstract".to_string(),
        key_entities: vec![],
        source_turn_ids: source_turn_ids.clone(),
        child_node_ids: vec!["child_1".to_string(), "child_2".to_string()],
        merkle_hash: meta_hash,
        token_count: 80,
        created_at: 1000,
        vector_embedding: None,
    };
    assert!(meta_node.verify_integrity(), "Legitimate meta-summary must pass");

    meta_node.child_node_ids.push("child_forged_3".to_string());
    assert!(!meta_node.verify_integrity(), "Tampered child node IDs must fail verification");
}

#[test]
fn test_stress_500_turns_massive_load_and_stability() {
    let config = SummaryTreeConfig {
        max_context_tokens: 500,
        overflow_threshold_ratio: 0.75, // 375 tokens
        chunk_target_tokens: 60,
        min_turns_per_chunk: 3,
        max_turns_per_chunk: 8,
        meta_summary_fanout: 4,
        domain: "MassiveLoad".to_string(),
        category: "Stability".to_string(),
    };

    let mut tree = RecursiveSummaryTree::new("session_500", config);

    // Rapidly push 500 turns
    for i in 1..=500 {
        let turn = DialogueTurn::new(
            "session_500",
            if i % 2 == 1 { "user" } else { "assistant" },
            &format!("Turn #{} with payload containing technical metrics and references [[Topic_{}]].", i, i % 20),
            1000 + i,
        );
        tree.add_turn(turn);
    }

    if tree.is_overflow() {
        tree.condense_overflow();
    }

    // Assert system stayed within bounds
    assert!(tree.total_working_tokens() <= 500 * 2, "Context window must stay bounded under continuous ingestion");
    assert!(tree.all_summaries().len() >= 30, "Must create rich hierarchy across 500 turns");

    // Check all summaries verify
    for summary in tree.all_summaries() {
        assert!(summary.verify_integrity());
    }

    // Prompt rendering test
    let prompt = tree.render_working_prompt("SYSTEM INSTRUCTIONS: You are LIVA.");
    assert!(prompt.contains("<SYSTEM_CONTEXT>"));
    assert!(prompt.contains("<context_memory>"));
    assert!(prompt.contains("[Summary L"));
}

// ============================================================================
// DIMENSION 5: HIGH-LOAD BENCHMARKING & LATENCY PROFILING (100K TOKENS)
// ============================================================================

#[test]
fn test_stress_llmlingua_large_scale_10k_tokens_throughput() {
    let compressor = LLMLinguaCompressor::new();

    // Generate a realistic 10,000+ token prompt containing mixed prose, XML, code, wikilinks, and numbers
    let mut large_prompt = String::with_capacity(64_000);
    large_prompt.push_str("<SYSTEM_CONTEXT>\nYou are LIVA, executing long-context reasoning.\n</SYSTEM_CONTEXT>\n\n");

    for i in 1..=200 {
        large_prompt.push_str(&format!(
            "Paragraph #{}: In cognitive architecture session [[Session_{}]], the agent evaluated memory partition \
             with 100,000 nodes, resulting in 75.5% compression ratio and sub-8.0ms latency. \
             Furthermore, it is generally acknowledged that redundant discourse markers should be pruned away smoothly.\n\
             ```rust\nlet metric_{} = calculate_loss({});\n```\n",
            i, i % 10, i, i * 42
        ));
    }

    let start = std::time::Instant::now();
    let result = compressor.compress(&large_prompt);
    let elapsed = start.elapsed();

    println!(
        "Large Scale Benchmark (10K+ tokens): original_tokens={}, compressed_tokens={}, elapsed={:?}, compression_ratio={:.2}x",
        result.metrics.original_tokens,
        result.metrics.compressed_tokens,
        elapsed,
        result.metrics.compression_ratio
    );

    assert!(result.metrics.original_tokens >= 5000, "Must contain >= 5000 tokens, got {}", result.metrics.original_tokens);
    assert!(elapsed.as_millis() < 2500, "10K tokens compression must execute within 2500ms (unoptimized debug) / 100ms (release), took {:?}", elapsed);
    assert!(result.metrics.compression_ratio >= 1.5, "Must achieve at least 1.5x compression on mixed input");
}

#[test]
fn test_stress_summary_tree_repeated_drill_down_consistency() {
    let config = SummaryTreeConfig {
        max_context_tokens: 200,
        overflow_threshold_ratio: 0.60,
        chunk_target_tokens: 30,
        min_turns_per_chunk: 2,
        max_turns_per_chunk: 4,
        meta_summary_fanout: 2,
        domain: "Lineage".to_string(),
        category: "Consistency".to_string(),
    };

    let mut tree = RecursiveSummaryTree::new("session_lineage_consistency", config);

    let mut expected_turn_ids = Vec::new();
    for i in 1..=20 {
        let turn_id = format!("turn_{:03}", i);
        expected_turn_ids.push(turn_id.clone());
        let turn = DialogueTurn::with_id(&turn_id, "session_lineage_consistency", "user", &format!("Turn {} message payload.", i), 100 + i as i64);
        tree.add_turn(turn);
    }

    tree.condense_overflow();

    let all_summaries = tree.all_summaries();
    assert!(!all_summaries.is_empty());

    // Collect all turn IDs across all root summaries
    let mut collected_turn_ids = Vec::new();
    for summary in tree.active_root_summaries() {
        let drilled = tree.drill_down_turn_ids(&summary.node_id);
        collected_turn_ids.extend(drilled);
    }
    collected_turn_ids.sort();
    collected_turn_ids.dedup();

    // Check that every condensed turn is tracked
    assert!(!collected_turn_ids.is_empty(), "Must track condensed turn IDs");
    for id in &collected_turn_ids {
        assert!(expected_turn_ids.contains(id), "Drilled turn ID {} must be in original turns", id);
    }
}
