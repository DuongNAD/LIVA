//! Integration Test Suite for LIVA Phase 3: Hierarchical Memory L1–L4 & Obsidian Graph RAG (RFC-003)
//!
//! Verifies:
//! 1. Extended Tauri v2 IPC memory command handlers (`memory:*`, `delete_memory_fact`, `get_memory_data`).
//! 2. `VirtualMemoryEngine` unified facade coordinating L1–L4, Enclaves, Compression, HippoRAG PPR, and 3-Way RRF.
//! 3. Serialization/deserialization, error propagation, fallback behavior, and security invariants.

use std::fs;
use std::sync::Arc;
use tempfile::tempdir;
use serde_json::json;

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::DatabasePool;
use liva_native_core::memory::{
    EpisodicEvent, ExecutionOutcome, MemoryEnclave, ProceduralSkill,
    VirtualMemoryEngine,
};
use liva_native_core::{
    AppState, commands, llm, stt, tts,
};

/// Helper to create an isolated in-memory test AppState.
fn create_test_state() -> Arc<AppState> {
    let db = DatabasePool::new_in_memory().expect("In-memory SQLite database");
    let stt_manager = stt::SttManager::new("non-existent-model");
    let llm_manager = llm::LlamaRouterManager::new(2048, 0).expect("LLM manager");
    let mock_capturer = Arc::new(liva_native_core::vision::capture::MockScreenCapturer::new(
        64,
        64,
        liva_native_core::vision::capture::PixelFormat::Rgba,
    ));

    Arc::new(AppState {
        db,
        crypto: EncryptionEngine::new("00000000000000000000000000000000"),
        stt: tokio::sync::Mutex::new(stt_manager),
        tts: tokio::sync::Mutex::new(None),
        tts_player: tts::audio::TtsAudioPlayer::new(None),
        llm: tokio::sync::Mutex::new(llm_manager),
        vad: tokio::sync::Mutex::new(None),
        denoiser: tokio::sync::Mutex::new(None),
        turn_shadow: tokio::sync::Mutex::new(None),
        aec: tokio::sync::Mutex::new(None),
        mcp_server: Arc::new(liva_native_core::mcp::server::NativeMcpServer::new("test_vault")),
        embedder: tokio::sync::Mutex::new(None),
        vision: tokio::sync::Mutex::new(liva_native_core::vision::VisionManager::new(
            mock_capturer,
            liva_native_core::vision::VisionConfig::default(),
        )),
    })
}

/// Helper to create a valid temporary Obsidian Vault directory with standard templates.
fn create_temp_obsidian_vault() -> tempfile::TempDir {
    let tmp = tempdir().expect("Create tempdir for vault");
    let root = tmp.path();

    for dir in &["Skills", "Knowledge", "Rules", "Templates"] {
        fs::create_dir_all(root.join(dir)).expect("Create vault directory");
    }

    fs::write(
        root.join("Templates").join("Skill Template.md"),
        "---\ntitle: Skill Template\ntags:\n  - liva/templates\nauthor: System\nlast_update: 2026-09-01T00:00:00Z\n---\n# Skill Template\n",
    ).expect("Write Skill Template");

    fs::write(
        root.join("Templates").join("Knowledge Template.md"),
        "---\ntitle: Knowledge Template\ntags:\n  - liva/templates\nauthor: System\nlast_update: 2026-09-01T00:00:00Z\n---\n# Knowledge Template\n",
    ).expect("Write Knowledge Template");

    fs::write(
        root.join("Templates").join("Rule Template.md"),
        "---\ntitle: Rule Template\ntags:\n  - liva/templates\nauthor: System\nlast_update: 2026-09-01T00:00:00Z\n---\n# Rule Template\n",
    ).expect("Write Rule Template");

    tmp
}

// =========================================================================
// 1. IPC Memory Command: Context Compression (LLMLingua-2)
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_compress_context() {
    let state = create_test_state();

    let text = r#"
<SYSTEM_CONTEXT>
You are LIVA Cognitive Assistant v3.0, running on macOS arm64 architecture.
Follow strict integrity rules and protect user confidentiality.
</SYSTEM_CONTEXT>
Furthermore, as we discussed yesterday regarding the project roadmap,
we need to ensure that the memory engine connects seamlessly to [[Obsidian_Vault]]
and supports HippoRAG with Personalized PageRank (PPR) algorithms.
The database has 100,000 nodes and must achieve query latency under 8.0ms with 98.5% precision.
```rust
fn execute_retrieval() -> Result<(), Error> { Ok(()) }
```
Please verify that all tests pass without any warnings.
"#;

    let payload = json!({
        "text": text,
        "target_compression_ratio": 3.0,
        "preserve_xml": true,
        "preserve_code": true,
        "preserve_entities": true,
        "preserve_wikilinks": true,
        "preserve_numbers_dates": true,
    });

    let resp = commands::memory::handle(state.clone(), "memory:compress_context", payload)
        .await
        .expect("Handle memory:compress_context");

    assert_eq!(resp["success"], true);
    let compressed_text = resp["compressed_text"].as_str().expect("compressed_text");
    assert!(compressed_text.contains("<SYSTEM_CONTEXT>"));
    assert!(compressed_text.contains("</SYSTEM_CONTEXT>"));
    assert!(compressed_text.contains("```rust"));
    assert!(compressed_text.contains("[[Obsidian_Vault]]"));

    let comp_ratio = resp["compression_ratio"].as_f64().expect("compression_ratio");
    assert!(comp_ratio >= 1.5, "Compression ratio should be effective (got {comp_ratio})");

    let entity_preservation = resp["entity_preservation_ratio"].as_f64().expect("entity_preservation");
    assert!(entity_preservation >= 0.90, "Entity preservation should be high");

    // Test error on missing text
    let err_resp = commands::memory::handle(state.clone(), "memory:compress_context", json!({}))
        .await;
    assert!(err_resp.is_err(), "Missing text must return error");
}

// =========================================================================
// 2. IPC Memory Command: Recursive Summary Tree Condensation
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_condense_summary_tree() {
    let state = create_test_state();

    let turns = vec![
        json!({
            "turn_id": "turn_001",
            "role": "user",
            "content": "Hello LIVA, please explain the memory architecture of Phase 3.",
            "timestamp": 1_700_000_000,
        }),
        json!({
            "turn_id": "turn_002",
            "role": "assistant",
            "content": "Phase 3 consists of 4 tiers: L1 Working Memory, L2 Episodic Memory, L3 Semantic Memory with Obsidian Sync, and L4 Procedural Memory.",
            "timestamp": 1_700_000_010,
        }),
        json!({
            "turn_id": "turn_003",
            "role": "user",
            "content": "How does LLMLingua-2 token pruning work with the Recursive Summary Tree?",
            "timestamp": 1_700_000_020,
        }),
        json!({
            "turn_id": "turn_004",
            "role": "assistant",
            "content": "LLMLingua-2 prunes redundant low-entropy tokens while preserving entities. When working context exceeds the token budget threshold, the Recursive Summary Tree condenses older turns into episodic summaries.",
            "timestamp": 1_700_000_030,
        }),
    ];

    let payload = json!({
        "session_id": "sess_summary_ipc_1",
        "turns": turns,
        "config": {
            "max_context_tokens": 100, // Low threshold to force overflow
            "overflow_threshold_ratio": 0.50,
            "min_turns_per_chunk": 2,
            "max_turns_per_chunk": 4,
        },
        "force_condense": true,
        "persist_to_db": true,
    });

    let resp = commands::memory::handle(state.clone(), "memory:condense_summary_tree", payload)
        .await
        .expect("Handle memory:condense_summary_tree");

    assert_eq!(resp["success"], true);
    assert_eq!(resp["session_id"], "sess_summary_ipc_1");
    let working_tokens = resp["working_tokens"].as_u64().unwrap();
    assert!(working_tokens > 0);

    let rendered_prompt = resp["rendered_prompt"].as_str().unwrap();
    assert!(!rendered_prompt.is_empty());

    // Verify error on missing turns
    let err_resp = commands::memory::handle(state.clone(), "memory:condense_summary_tree", json!({ "session_id": "test" }))
        .await;
    assert!(err_resp.is_err(), "Missing turns must return error");
}

// =========================================================================
// 3. IPC Memory Command: Obsidian Vault Bidirectional Sync
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_sync_obsidian_vault() {
    let state = create_test_state();
    let vault_tmp = create_temp_obsidian_vault();
    let root = vault_tmp.path();

    // Create valid notes with frontmatter and wikilinks
    let note1_content = "---\ntitle: Rust Engine\ntags:\n  - liva/architecture\nauthor: Architect\nlast_update: 2026-09-01T12:00:00Z\n---\n# Rust Engine\nConnects to [[Memory_Subsystem]] and [[Obsidian_Sync]].\n";
    let note2_content = "---\ntitle: Memory Subsystem\ntags:\n  - liva/memory\nauthor: Specialist\nlast_update: 2026-09-01T12:00:00Z\n---\n# Memory Subsystem\nImplements L1-L4 cognitive memory tiers.\n";
    let note3_content = "---\ntitle: Obsidian Sync\ntags:\n  - liva/sync\nauthor: Specialist\nlast_update: 2026-09-01T12:00:00Z\n---\n# Obsidian Sync\nProvides real-time graph synchronization.\n";

    fs::write(root.join("Knowledge").join("Rust Engine.md"), note1_content).unwrap();
    fs::write(root.join("Knowledge").join("Memory Subsystem.md"), note2_content).unwrap();
    fs::write(root.join("Knowledge").join("Obsidian Sync.md"), note3_content).unwrap();

    let payload = json!({
        "vault_path": root.to_str().unwrap(),
        "sync_to_db": true,
        "bidirectional": true,
    });

    let resp = commands::memory::handle(state.clone(), "memory:sync_obsidian_vault", payload)
        .await
        .expect("Handle memory:sync_obsidian_vault");

    assert_eq!(resp["success"], true);
    assert!(resp["nodes_synced"].as_u64().unwrap() >= 3);
    assert!(resp["edges_synced"].as_u64().unwrap() >= 2);
    assert!(resp["csr_num_nodes"].as_u64().unwrap() >= 3);
    assert!(resp["csr_num_edges"].as_u64().unwrap() >= 2);

    // Test error on non-existent vault
    let err_resp = commands::memory::handle(
        state.clone(),
        "memory:sync_obsidian_vault",
        json!({ "vault_path": "/non/existent/vault/path/random_12345" }),
    ).await;
    assert!(err_resp.is_err(), "Non-existent vault must return error");
}

// =========================================================================
// 4. IPC Memory Command: HippoRAG Personalized PageRank (PPR)
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_run_hipporag_ppr() {
    let state = create_test_state();
    let vault_tmp = create_temp_obsidian_vault();
    let root = vault_tmp.path();

    // Create structured graph: A -> B -> C, A -> C
    let note_a = "---\ntitle: Node A\ntags:\n  - liva/graph\nauthor: Test\nlast_update: 2026-09-01T12:00:00Z\n---\nLinks: [[Node B]], [[Node C]].\n";
    let note_b = "---\ntitle: Node B\ntags:\n  - liva/graph\nauthor: Test\nlast_update: 2026-09-01T12:00:00Z\n---\nLinks: [[Node C]].\n";
    let note_c = "---\ntitle: Node C\ntags:\n  - liva/graph\nauthor: Test\nlast_update: 2026-09-01T12:00:00Z\n---\nTerminal knowledge node.\n";

    fs::write(root.join("Knowledge").join("Node A.md"), note_a).unwrap();
    fs::write(root.join("Knowledge").join("Node B.md"), note_b).unwrap();
    fs::write(root.join("Knowledge").join("Node C.md"), note_c).unwrap();

    let payload = json!({
        "vault_path": root.to_str().unwrap(),
        "seeds": [
            { "name": "Node A", "weight": 1.0 }
        ],
        "damping_factor": 0.15,
        "max_iterations": 20,
        "tolerance": 1e-6,
        "top_k": 3,
        "bidirectional": false,
    });

    let resp = commands::memory::handle(state.clone(), "memory:run_hipporag_ppr", payload)
        .await
        .expect("Handle memory:run_hipporag_ppr");

    assert_eq!(resp["success"], true);
    assert_eq!(resp["num_graph_nodes"], 3);
    assert!(resp["iterations"].as_u64().unwrap() >= 1);
    assert!(resp["elapsed_ms"].as_f64().unwrap() >= 0.0);

    let rankings = resp["top_k_rankings"].as_array().expect("top_k_rankings");
    assert!(!rankings.is_empty());

    // Test error on missing seeds
    let err_resp = commands::memory::handle(state.clone(), "memory:run_hipporag_ppr", json!({}))
        .await;
    assert!(err_resp.is_err(), "Missing seeds must return error");
}

// =========================================================================
// 5. IPC Memory Command: Episodic Retention Sweep & Decay
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_sweep_retention() {
    let state = create_test_state();

    // 1. Insert episodic events directly into L2 store
    let enclave = Arc::new(MemoryEnclave::new_from_master_key([0u8; 32]));
    let store = liva_native_core::memory::L2EpisodicStore::new(state.db.clone(), enclave);
    store.init_schema().expect("Schema init");

    let t0 = 1_700_000_000;
    let event1 = EpisodicEvent {
        memory_id: "sweep_ipc_01".to_string(),
        session_id: "session_sweep".to_string(),
        domain: "General".to_string(),
        category: "Test".to_string(),
        content: "High importance fact".to_string(),
        importance_score: 9.0,
        emotional_valence: 1.0,
        recall_count: 0,
        created_at: t0,
        last_recalled_at: t0,
        base_half_life_secs: 10_000,
        retention_score: 1.0,
    };
    let event2 = EpisodicEvent {
        memory_id: "sweep_ipc_02".to_string(),
        session_id: "session_sweep".to_string(),
        domain: "General".to_string(),
        category: "Test".to_string(),
        content: "Ephemeral transient fact".to_string(),
        importance_score: 1.0,
        emotional_valence: 0.5,
        recall_count: 0,
        created_at: t0,
        last_recalled_at: t0,
        base_half_life_secs: 100,
        retention_score: 1.0,
    };
    store.insert_event(&event1).unwrap();
    store.insert_event(&event2).unwrap();

    // 2. Invoke sweep_retention via IPC command
    let t1 = t0 + 500;
    let payload = json!({
        "current_timestamp": t1,
        "purge_threshold": 0.05,
    });

    let resp = commands::memory::handle(state.clone(), "memory:sweep_retention", payload)
        .await
        .expect("Handle memory:sweep_retention");

    assert_eq!(resp["success"], true);
    let report = &resp["report"];
    assert_eq!(report["total_processed"], 2);

    // 3. Test conversation sweep variant with maxAgeDays
    let conv_payload = json!({
        "maxAgeDays": 30,
        "batchLimit": 10,
        "dryRun": true,
    });
    let conv_resp = commands::memory::handle(state.clone(), "memory:sweep_retention", conv_payload)
        .await
        .expect("Handle conversation retention sweep");
    assert!(conv_resp.get("dry_run").or_else(|| conv_resp.get("dryRun")).is_some());
}

// =========================================================================
// 6. IPC Memory Command: Cryptographic Enclave WAL Sanitization
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_sanitize_enclave() {
    let state = create_test_state();

    let resp = commands::memory::handle(state.clone(), "memory:sanitize_enclave", json!({}))
        .await
        .expect("Handle memory:sanitize_enclave");

    assert_eq!(resp["success"], true);
    assert_eq!(resp["wal_sanitized"], true);
    assert!(resp["timestamp"].as_i64().unwrap() > 0);
}

// =========================================================================
// 7. IPC Memory Command: 3-Way Reciprocal Rank Fusion Hybrid Search
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_search_hybrid_3way_rrf() {
    let state = create_test_state();

    // 1. Insert vector & fact into SQLite
    let vector = vec![0.05f32; 384];
    let upsert_payload = json!({
        "vecId": "vec_test_3way_01",
        "type": "fact",
        "content": "HippoRAG accelerates graph retrieval using Personalized PageRank on Compressed Sparse Row.",
        "vector": vector,
        "domain": "local",
        "category": "architecture",
        "traceKeywords": ["HippoRAG", "PageRank", "CSR"],
    });

    commands::memory::handle(state.clone(), "memory:upsert_vector", upsert_payload)
        .await
        .expect("Upsert vector for hybrid search");

    // 2. Insert graph nodes and edges
    {
        let conn = state.db.writer.get().unwrap();
        conn.execute(
            "INSERT INTO l3_nodes (id, label, properties) VALUES ('HippoRAG', 'HippoRAG', '{}'), ('CSR', 'CSR', '{}')",
            [],
        ).unwrap();
        conn.execute(
            "INSERT INTO l3_edges (source, target, relation, weight, obsolete) VALUES ('HippoRAG', 'CSR', 'uses', 1.0, 0)",
            [],
        ).unwrap();
    }

    // 3. Search with 3-way RRF (sparse BM25 + dense vector + HippoRAG graph seeds)
    let search_payload = json!({
        "query_text": "HippoRAG PageRank CSR",
        "query_vector": vector,
        "type": "fact",
        "filter": {
            "type": "fact",
            "domain": "local"
        },
        "top_k": 5,
        "dense_weight": 0.45,
        "sparse_weight": 0.30,
        "graph_weight": 0.25,
        "graph_seeds": [
            { "name": "HippoRAG", "weight": 1.0 }
        ],
    });

    let resp = commands::memory::handle(state.clone(), "memory:search_hybrid", search_payload)
        .await
        .expect("Handle 3-Way search_hybrid");

    let results = resp.as_array().expect("Search results array");
    assert!(!results.is_empty(), "3-Way Hybrid Search should return matching hits");
    assert_eq!(results[0]["vec_id"], "vec_test_3way_01");
}

// =========================================================================
// 8. IPC Memory Command: Fact CRUD & Fail-Closed Semantics
// =========================================================================

#[tokio::test]
async fn test_ipc_memory_fact_crud_lifecycle() {
    let state = create_test_state();

    // 1. Set fact
    let set_payload = json!({
        "key": "user_theme_preference",
        "value": "dark_mode_nord",
        "category": "preferences",
        "importance": 0.9,
        "confidenceScore": 0.95,
        "memory_strength": 1.0,
        "source": "manual_setup",
        "createdAt": "2026-09-01T12:00:00Z",
        "updatedAt": "2026-09-01T12:00:00Z",
        "last_accessed_at": 1700000000,
        "access_count": 1,
    });

    let set_resp = commands::memory::handle(state.clone(), "memory:set_fact", set_payload)
        .await
        .expect("Handle memory:set_fact");
    assert_eq!(set_resp["success"], true);

    // 2. Get fact
    let get_resp = commands::memory::handle(
        state.clone(),
        "memory:get_fact",
        json!({ "key": "user_theme_preference" }),
    ).await.expect("Handle memory:get_fact");

    assert_eq!(get_resp["key"], "user_theme_preference");
    assert_eq!(get_resp["value"], "dark_mode_nord");

    // 3. Delete fact
    let del_resp = commands::memory::handle(
        state.clone(),
        "delete_memory_fact",
        json!({ "key": "user_theme_preference" }),
    ).await.expect("Handle delete_memory_fact");
    assert_eq!(del_resp["success"], true);

    // 4. Verify fact is deleted
    let get_deleted = commands::memory::handle(
        state.clone(),
        "memory:get_fact",
        json!({ "key": "user_theme_preference" }),
    ).await.expect("Handle memory:get_fact after delete");
    assert!(get_deleted.is_null());
}

// =========================================================================
// 9. Unified VirtualMemoryEngine Facade Full Integration
// =========================================================================

#[tokio::test]
async fn test_virtual_memory_engine_full_subsystem_coordination() {
    let pool = DatabasePool::new_in_memory().unwrap();
    let enclave = Arc::new(
        MemoryEnclave::new_with_argon2id(b"master_passphrase_phase3", b"unique_salt_9876543210")
            .expect("Argon2id enclave derivation"),
    );

    let vme = VirtualMemoryEngine::new(pool.clone(), enclave.clone(), 256);
    vme.l2().init_schema().expect("Init L2 schema");

    // Tier 1: L1 Working Memory (Radix Prefix Cache)
    let prompt_tokens = vec![101, 102, 103, 104, 105];
    vme.insert_working_prefix(&prompt_tokens, 42, true).await;
    assert!(vme.pin_working_prefix(&prompt_tokens).await);

    let (matched, blocks) = vme.match_working_prefix(&prompt_tokens).await;
    assert_eq!(matched, 5);
    assert_eq!(blocks, vec![42]);

    let stats = vme.working_prefix_stats();
    assert_eq!(stats.allocated_blocks, 1);
    assert!(stats.hits >= 1);

    // Tier 2: L2 Episodic Memory (Exponential Decay)
    let now = chrono::Utc::now().timestamp();
    let event = EpisodicEvent {
        memory_id: "vme_ep_001".to_string(),
        session_id: "vme_sess_1".to_string(),
        domain: "Security".to_string(),
        category: "Enclave".to_string(),
        content: "AES-256-GCM v2 Memory Enclave verified.".to_string(),
        importance_score: 9.5,
        emotional_valence: 1.0,
        recall_count: 0,
        created_at: now,
        last_recalled_at: now,
        base_half_life_secs: 50_000,
        retention_score: 1.0,
    };
    vme.record_episodic_event(&event).unwrap();
    let recalled = vme.recall_episodic_context("Security", 0.5).unwrap();
    assert_eq!(recalled.len(), 1);
    assert_eq!(recalled[0].content, event.content);

    let sweep_report = vme.sweep_episodic_retention(now + 1000).unwrap();
    assert_eq!(sweep_report.total_processed, 1);

    // Tier 3: L3 Semantic Memory (Obsidian Sync & HippoRAG Graph)
    let vault_tmp = create_temp_obsidian_vault();
    let root = vault_tmp.path();
    fs::write(
        root.join("Knowledge").join("Architecture.md"),
        "---\ntitle: Architecture\ntags:\n  - liva/knowledge\nauthor: Core\nlast_update: 2026-09-01T00:00:00Z\n---\n# Architecture\nConnects [[Security]] and [[Engines]].\n",
    ).unwrap();
    fs::write(
        root.join("Knowledge").join("Security.md"),
        "---\ntitle: Security\ntags:\n  - liva/knowledge\nauthor: Core\nlast_update: 2026-09-01T00:00:00Z\n---\n# Security\n",
    ).unwrap();
    fs::write(
        root.join("Knowledge").join("Engines.md"),
        "---\ntitle: Engines\ntags:\n  - liva/knowledge\nauthor: Core\nlast_update: 2026-09-01T00:00:00Z\n---\n# Engines\n",
    ).unwrap();

    let (sync, report) = vme.sync_obsidian_vault(root).expect("Sync vault");
    assert_eq!(report.invalid_files_count, 0);

    let hippo = vme.build_hipporag_from_vault(&sync, true);
    assert_eq!(hippo.graph.num_nodes, 3);
    let ppr_scores = hippo.run_ppr_by_names(&[("Architecture", 1.0)]);
    assert_eq!(ppr_scores.len(), 3);

    // Tier 4: L4 Procedural Memory (Bayesian Skill Weights)
    let skill = ProceduralSkill::new(
        "crypto_sanitize_tool".to_string(),
        "Crypto Sanitize".to_string(),
        "Sanitize SQLite WAL and memory enclave".to_string(),
        "fn sanitize() -> bool { true }".to_string(),
    );
    vme.l4().register_skill(skill).await;
    vme.record_skill_outcome("crypto_sanitize_tool", &ExecutionOutcome::Success).await.unwrap();

    let ranked = vme.rank_procedural_skills(&[("crypto_sanitize_tool".to_string(), 0.90)]).await;
    assert_eq!(ranked.len(), 1);
    assert!(ranked[0].final_rank_score > 0.80);

    // Compression Facade (LLMLingua-2 & Summary Tree)
    let comp_res = vme.compress_context(
        "This is an extraordinarily lengthy informational paragraph containing important details.",
        None,
    );
    assert!(!comp_res.compressed_text.is_empty());

    let tree = vme.create_summary_tree("vme_session_tree", None);
    assert_eq!(tree.session_id(), "vme_session_tree");

    // Enclave Encryption & WAL Sanitization
    let ciphertext = vme.encrypt_str("Confidential Secret Token 42").unwrap();
    let decrypted = vme.decrypt_str(&ciphertext).unwrap();
    assert_eq!(&*decrypted, "Confidential Secret Token 42");

    let conn = pool.writer.get().unwrap();
    vme.sanitize_wal(&conn).expect("Sanitize WAL");
}

// =========================================================================
// 10. Security & Traversal Invariants Verification
// =========================================================================

#[test]
fn test_security_invariants_and_path_containment() {
    let vault_tmp = create_temp_obsidian_vault();
    let root = vault_tmp.path();

    // Traversal attempts
    let traversals = [
        "../secret.txt",
        "../../etc/shadow",
        "%2e%2e%2fsecret.txt",
        "%2e%2e%5csecret.txt",
        "Knowledge/../../../root.txt",
    ];

    for path in &traversals {
        let res = liva_native_core::memory::validate_and_resolve_path(root, path);
        assert!(res.is_err(), "Path traversal '{path}' must be rejected");
    }

    // Fail-closed decryption on corrupted ciphertext
    let enclave = MemoryEnclave::new_from_master_key([1u8; 32]);
    let tampered = "v2:00112233445566778899aabbccddeeff:00112233445566778899aabbccddeeff:00112233445566778899aabbccddeeff:ffffffff";
    let read_res = enclave.read_record(tampered);
    assert!(read_res.is_locked(), "Tampered ciphertext must be locked");
    assert_eq!(read_res.into_value(), "", "Locked ciphertext must return empty string");
}
