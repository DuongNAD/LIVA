use std::collections::HashMap;
use std::sync::Arc;
use rusqlite::Connection;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{DatabasePool, MetadataFilter};
use liva_native_core::memory::graph::{CsrGraph, HippoRagEngine};
use liva_native_core::memory::search::{
    aggregate_graph_activations, cosine_distance_to_similarity, cosine_similarity_f32,
    cosine_similarity_int8, dot_product_int8, normalize_vietnamese_query,
    prepare_fts5_query, quantize_int8_scaled, quantize_unit_int8, remove_diacritics,
    tokenize_normalized, Bm25Config, DenseCandidate, DenseCandidateInt8,
    DenseVecEngine, SearchHit, SparseBm25Engine, TriModalRrfEngine,
};
use liva_native_core::memory::{MemoryEnclave, VirtualMemoryEngine};

// =========================================================================
// 1. Vietnamese Diacritic Normalization Tests
// =========================================================================

#[test]
fn test_vietnamese_diacritic_removal_nfc() {
    let input = "Hệ thống bộ nhớ phân tầng LIVA và công cụ tìm kiếm lai 3 kênh RRF";
    let normalized = remove_diacritics(input);
    assert_eq!(
        normalized,
        "He thong bo nho phan tang LIVA va cong cu tim kiem lai 3 kenh RRF"
    );

    let vowels = "àáảãạ âầấẩẫậ ăằắẳẵặ èéẻẽẹ êềếểễệ ìíỉĩị òóỏõọ ôồốổỗộ ơờớởỡợ ùúủũụ ưừứửữự ỳýỷỹỵ đ Đ";
    let stripped_vowels = remove_diacritics(vowels);
    assert_eq!(
        stripped_vowels,
        "aaaaa aaaaaa aaaaaa eeeee eeeeee iiiii ooooo oooooo oooooo uuuuu uuuuuu yyyyy d D"
    );
}

#[test]
fn test_vietnamese_diacritic_removal_nfd_combining_marks() {
    // NFD decomposed representation: 'e' + U+0302 (circumflex) + U+0301 (acute) = 'ế'
    let nfd_sample = format!("Ti{}ng Vi{}t", "e\u{0302}\u{0301}", "e\u{0302}\u{0323}");
    let normalized = remove_diacritics(&nfd_sample);
    assert_eq!(normalized, "Tieng Viet");

    let nfd_da_nang = format!("{}a N{}ng", "\u{0110}", "a\u{0306}\u{0303}");
    let normalized_da_nang = remove_diacritics(&nfd_da_nang);
    assert_eq!(normalized_da_nang, "Da Nang");
}

#[test]
fn test_vietnamese_query_normalization_and_tokenization() {
    let query = "   Tìm Kiếm   Bộ Nhớ [[Obsidian]] & Đồ Thị!   ";
    let norm_query = normalize_vietnamese_query(query);
    assert_eq!(norm_query, "tim kiem bo nho [[obsidian]] & do thi!");

    let tokens = tokenize_normalized(query);
    assert_eq!(
        tokens,
        vec!["tim", "kiem", "bo", "nho", "obsidian", "do", "thi"]
    );
}

#[test]
fn test_fts5_query_preparation() {
    let query = "bộ nhớ LIVA";
    let fts_query = prepare_fts5_query(query);
    assert_eq!(
        fts_query,
        "(\"bộ\"* OR \"bo\"*) AND (\"nhớ\"* OR \"nho\"*) AND \"liva\"*"
    );

    let empty_query = "   ";
    let fts_empty = prepare_fts5_query(empty_query);
    assert_eq!(fts_empty, "\"\"*");

    let special_query = "công nghệ AI \"2026\" (PPR & RRF)";
    let fts_special = prepare_fts5_query(special_query);
    assert_eq!(
        fts_special,
        "(\"công\"* OR \"cong\"*) AND (\"nghệ\"* OR \"nghe\"*) AND \"ai\"* AND \"2026\"* AND \"ppr\"* AND \"rrf\"*"
    );
}

// =========================================================================
// 2. Sparse Okapi BM25 Scoring & Retrieval Tests
// =========================================================================

#[test]
fn test_bm25_in_memory_scoring_and_ranking() {
    let bm25 = SparseBm25Engine::new();

    let docs = vec![
        (1, "vec_1".to_string(), "Hệ thống bộ nhớ LIVA hỗ trợ L1 L2 L3 L4".to_string(), "local".to_string(), "memory".to_string()),
        (2, "vec_2".to_string(), "Obsidian Vault đồng bộ hai chiều với kiến trúc tri thức".to_string(), "local".to_string(), "obsidian".to_string()),
        (3, "vec_3".to_string(), "Bộ nhớ L2 suy giảm theo thời gian với hàm mũ Ebbinghaus".to_string(), "local".to_string(), "memory".to_string()),
        (4, "vec_4".to_string(), "Mã hóa AES-256-GCM bảo mật dữ liệu người dùng ở trạng thái nghỉ".to_string(), "local".to_string(), "security".to_string()),
    ];

    let hits = bm25.search_in_memory("bộ nhớ L2", &docs, 10);
    assert!(!hits.is_empty());
    // Both Doc 1 and Doc 3 match all 3 query tokens ("bo", "nho", "l2")
    assert!(hits.iter().any(|h| h.id == 1));
    assert!(hits.iter().any(|h| h.id == 3));
    assert!(hits[0].score > 0.0);
    assert_eq!(hits[0].source_channel, "sparse_bm25");

    // Diacritic-insensitive query: "bo nho" matches "Bộ nhớ"
    let hits_diacritic = bm25.search_in_memory("bo nho liva", &docs, 10);
    assert!(!hits_diacritic.is_empty());
    assert_eq!(hits_diacritic[0].id, 1); // Doc 1 contains "Hệ thống bộ nhớ LIVA"
}

#[test]
fn test_bm25_custom_parameters() {
    let custom_bm25 = SparseBm25Engine::with_config(Bm25Config { k1: 1.5, b: 0.8 });
    let docs = vec![
        (1, "v1".to_string(), "Rust high performance memory engine".to_string(), "local".to_string(), "tech".to_string()),
        (2, "v2".to_string(), "Python slow legacy backend".to_string(), "local".to_string(), "tech".to_string()),
    ];

    let hits = custom_bm25.search_in_memory("Rust engine", &docs, 5);
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].id, 1);
}

// =========================================================================
// 3. Dense Vector INT8 Quantization & Similarity Tests
// =========================================================================

#[test]
fn test_int8_quantization_unit_vectors() {
    let float_vec = vec![0.0f32, 1.0, -1.0, 0.5, -0.5];
    let int8_vec = quantize_unit_int8(&float_vec);

    assert_eq!(int8_vec[0], 0);
    assert_eq!(int8_vec[1], 127);
    assert_eq!(int8_vec[2], -127);
    assert_eq!(int8_vec[3], 64);
    assert_eq!(int8_vec[4], -64);
}

#[test]
fn test_int8_scaled_quantization() {
    let unnormalized = vec![10.0f32, 20.0, 30.0, 40.0, 50.0];
    let int8_scaled = quantize_int8_scaled(&unnormalized);

    assert_eq!(int8_scaled.len(), 5);
    assert!(int8_scaled[0] < int8_scaled[4]);
    assert_eq!(int8_scaled[2], 0); // Mean is 30.0, so offset is 0
}

#[test]
fn test_int8_dot_product_and_cosine_similarity() {
    let vec_a = vec![100i8, 50, 0, -50];
    let vec_b = vec![100i8, 50, 0, -50];
    let vec_c = vec![-100i8, -50, 0, 50];
    let vec_d = vec![0i8, 0, 100, 0];

    assert_eq!(dot_product_int8(&vec_a, &vec_b), 100*100 + 50*50 + 0 + 50*50);
    let sim_same = cosine_similarity_int8(&vec_a, &vec_b);
    assert!((sim_same - 1.0).abs() < 1e-4);

    let sim_opposite = cosine_similarity_int8(&vec_a, &vec_c);
    assert!((sim_opposite - (-1.0)).abs() < 1e-4);

    let sim_orthogonal = cosine_similarity_int8(&vec_a, &vec_d);
    assert_eq!(sim_orthogonal, 0.0);
}

#[test]
fn test_cosine_similarity_f32() {
    let v1 = vec![1.0f32, 0.0, 0.0];
    let v2 = vec![1.0f32, 0.0, 0.0];
    let v3 = vec![0.0f32, 1.0, 0.0];

    assert!((cosine_similarity_f32(&v1, &v2) - 1.0).abs() < 1e-6);
    assert_eq!(cosine_similarity_f32(&v1, &v3), 0.0);
}

#[test]
fn test_cosine_distance_to_similarity_scaling() {
    // distance 0.0 -> 1.0
    assert_eq!(cosine_distance_to_similarity(0.0), 1.0);

    // distance 120.0 -> 1.0 - (1.0)^2 / 2.0 = 0.5
    assert!((cosine_distance_to_similarity(120.0) - 0.5).abs() < 1e-6);

    // distance >= 120.0 * sqrt(2) ~ 169.7 -> 0.0
    assert_eq!(cosine_distance_to_similarity(300.0), 0.0);
}

#[test]
fn test_dense_vec_in_memory_search() {
    let engine = DenseVecEngine::new();

    let query = vec![1.0f32, 0.0, 0.0];
    let candidates = vec![
        DenseCandidate {
            id: 1,
            vec_id: "doc_exact".to_string(),
            content: "Exact match vector".to_string(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "test".to_string(),
            vector: vec![1.0, 0.0, 0.0],
            decay_weight: 1.0,
            created_at: 1000,
        },
        DenseCandidate {
            id: 2,
            vec_id: "doc_partial".to_string(),
            content: "Partial match vector".to_string(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "test".to_string(),
            vector: vec![0.707, 0.707, 0.0],
            decay_weight: 1.0,
            created_at: 1000,
        },
        DenseCandidate {
            id: 3,
            vec_id: "doc_decayed".to_string(),
            content: "Decayed exact vector".to_string(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "test".to_string(),
            vector: vec![1.0, 0.0, 0.0],
            decay_weight: 0.3,
            created_at: 1000,
        },
    ];

    let hits = engine.search_in_memory(&query, &candidates, 5);
    assert_eq!(hits.len(), 3);
    assert_eq!(hits[0].vec_id, "doc_exact");
    assert!((hits[0].score - 1.0).abs() < 1e-4);
    assert_eq!(hits[1].vec_id, "doc_partial");
    assert_eq!(hits[2].vec_id, "doc_decayed");
    assert!((hits[2].score - 0.3).abs() < 1e-4);
}

#[test]
fn test_dense_vec_int8_in_memory_search() {
    let engine = DenseVecEngine::new();

    let query_int8 = vec![127i8, 0, 0];
    let candidates = vec![
        DenseCandidateInt8 {
            id: 10,
            vec_id: "int8_match".to_string(),
            content: "INT8 Match".to_string(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "test".to_string(),
            vector_int8: vec![127i8, 0, 0],
            decay_weight: 1.0,
            created_at: 2000,
        },
        DenseCandidateInt8 {
            id: 20,
            vec_id: "int8_ortho".to_string(),
            content: "INT8 Orthogonal".to_string(),
            r#type: "fact".to_string(),
            domain: "local".to_string(),
            category: "test".to_string(),
            vector_int8: vec![0i8, 127, 0],
            decay_weight: 1.0,
            created_at: 2000,
        },
    ];

    let hits = engine.search_in_memory_int8(&query_int8, &candidates, 5);
    assert_eq!(hits.len(), 2);
    assert_eq!(hits[0].vec_id, "int8_match");
    assert!((hits[0].score - 1.0).abs() < 1e-4);
    assert_eq!(hits[1].vec_id, "int8_ortho");
    assert_eq!(hits[1].score, 0.0);
}

// =========================================================================
// 4. HippoRAG Graph Activation Aggregation Tests
// =========================================================================

#[test]
fn test_graph_activation_aggregation() {
    let edges = vec![
        ("Note_A", "Note_B", 1.0f32),
        ("Note_B", "Note_C", 1.0f32),
        ("Note_C", "Note_A", 1.0f32),
    ];
    let graph = CsrGraph::from_named_edges(&edges, true);
    let engine = HippoRagEngine::new(graph);

    // Seed from Note_A
    let ppr = engine.run_ppr_by_names(&[("Note_A", 1.0)]);
    assert_eq!(ppr.len(), 3);

    let mut node_to_idx = HashMap::new();
    for (idx, name) in engine.graph.idx_to_node.iter().enumerate() {
        node_to_idx.insert(name.clone(), idx as u32);
    }

    let ppr_probs: Vec<f32> = engine
        .graph
        .idx_to_node
        .iter()
        .map(|name| ppr.iter().find(|(n, _)| n == name).map(|(_, p)| *p).unwrap_or(0.0))
        .collect();

    let doc_entities = vec![
        (1, "doc_a".to_string(), "Doc mentioning Note_A".to_string(), vec![("Note_A".to_string(), 1.0f32)]),
        (2, "doc_b".to_string(), "Doc mentioning Note_B".to_string(), vec![("Note_B".to_string(), 1.0f32)]),
        (3, "doc_ab".to_string(), "Doc mentioning Note_A and Note_B".to_string(), vec![("Note_A".to_string(), 1.0f32), ("Note_B".to_string(), 0.5f32)]),
    ];

    let graph_hits = aggregate_graph_activations(&ppr_probs, &node_to_idx, &doc_entities, 5);
    assert_eq!(graph_hits.len(), 3);
    assert_eq!(graph_hits[0].vec_id, "doc_ab"); // Combines activation of Note_A + 0.5 * Note_B
    assert_eq!(graph_hits[0].source_channel, "hipporag_graph");
}

// =========================================================================
// 5. 3-Way Reciprocal Rank Fusion (RRF) Engine Tests
// =========================================================================

#[test]
fn test_3way_rrf_mathematical_formulation() {
    let rrf = TriModalRrfEngine::with_weights(60.0, 0.30, 0.45, 0.25);

    let bm25_hits = vec![
        SearchHit::new(1, "doc_1", "Doc 1 Content", "fact", "local", "test", 10.0, "sparse_bm25"),
        SearchHit::new(2, "doc_2", "Doc 2 Content", "fact", "local", "test", 8.0, "sparse_bm25"),
    ];

    let dense_hits = vec![
        SearchHit::new(2, "doc_2", "Doc 2 Content", "fact", "local", "test", 0.95, "dense_vec"),
        SearchHit::new(3, "doc_3", "Doc 3 Content", "fact", "local", "test", 0.85, "dense_vec"),
    ];

    let graph_hits = vec![
        SearchHit::new(2, "doc_2", "Doc 2 Content", "fact", "local", "test", 0.40, "hipporag_graph"),
        SearchHit::new(1, "doc_1", "Doc 1 Content", "fact", "local", "test", 0.30, "hipporag_graph"),
    ];

    let fused = rrf.fuse_detailed(&bm25_hits, &dense_hits, &graph_hits, 10);
    assert_eq!(fused.len(), 3);

    // Expected Scores:
    // doc_2: rank 2 in bm25, rank 1 in dense, rank 1 in graph
    // score = 0.30 / (60 + 2) + 0.45 / (60 + 1) + 0.25 / (60 + 1)
    //       = 0.30 / 62 + 0.45 / 61 + 0.25 / 61 = 0.0048387 + 0.0073770 + 0.0040983 = 0.016314
    let doc_2_detail = fused.iter().find(|d| d.hit.vec_id == "doc_2").unwrap();
    let expected_doc_2_rrf = 0.30 / 62.0 + 0.45 / 61.0 + 0.25 / 61.0;
    assert!((doc_2_detail.rrf_score - expected_doc_2_rrf).abs() < 1e-6);
    assert_eq!(doc_2_detail.bm25_rank, Some(2));
    assert_eq!(doc_2_detail.dense_rank, Some(1));
    assert_eq!(doc_2_detail.graph_rank, Some(1));

    // doc_1: rank 1 in bm25, missing in dense, rank 2 in graph
    // score = 0.30 / (60 + 1) + 0.0 + 0.25 / (60 + 2)
    let doc_1_detail = fused.iter().find(|d| d.hit.vec_id == "doc_1").unwrap();
    let expected_doc_1_rrf = 0.30 / 61.0 + 0.25 / 62.0;
    assert!((doc_1_detail.rrf_score - expected_doc_1_rrf).abs() < 1e-6);
    assert_eq!(doc_1_detail.bm25_rank, Some(1));
    assert_eq!(doc_1_detail.dense_rank, None);
    assert_eq!(doc_1_detail.graph_rank, Some(2));

    // doc_3: missing in bm25, rank 2 in dense, missing in graph
    // score = 0.45 / (60 + 2)
    let doc_3_detail = fused.iter().find(|d| d.hit.vec_id == "doc_3").unwrap();
    let expected_doc_3_rrf = 0.45 / 62.0;
    assert!((doc_3_detail.rrf_score - expected_doc_3_rrf).abs() < 1e-6);
    assert_eq!(doc_3_detail.dense_rank, Some(2));

    // Winner must be doc_2 because it's strong across all 3 channels
    assert_eq!(fused[0].hit.vec_id, "doc_2");
}

#[test]
fn test_3way_rrf_deterministic_tie_breaking() {
    let rrf = TriModalRrfEngine::new();

    // Two hits with identical RRF contributions
    let bm25_hits = vec![
        SearchHit::new(20, "doc_z", "Doc Z", "fact", "local", "test", 5.0, "sparse_bm25"),
        SearchHit::new(10, "doc_a", "Doc A", "fact", "local", "test", 5.0, "sparse_bm25"),
    ];

    let fused = rrf.fuse(&bm25_hits, &[], &[], 5);
    assert_eq!(fused.len(), 2);
    // Rank 1 gets higher score than Rank 2
    assert_eq!(fused[0].vec_id, "doc_z");
    assert_eq!(fused[1].vec_id, "doc_a");

    // Same rank in different disjoint channels
    let rrf_symmetric = TriModalRrfEngine::with_weights(60.0, 0.5, 0.5, 0.0);
    let ch1 = vec![SearchHit::new(1, "doc_beta", "B", "fact", "local", "t", 1.0, "c1")];
    let ch2 = vec![SearchHit::new(2, "doc_alpha", "A", "fact", "local", "t", 1.0, "c2")];

    let fused_sym = rrf_symmetric.fuse(&ch1, &ch2, &[], 5);
    assert_eq!(fused_sym.len(), 2);
    // Both have identical RRF score (0.5 / 61.0), tie-breaker sorts vec_id ascending: doc_alpha then doc_beta
    assert_eq!(fused_sym[0].vec_id, "doc_alpha");
    assert_eq!(fused_sym[1].vec_id, "doc_beta");
}

// =========================================================================
// 6. SQLite Database End-to-End Integration Tests
// =========================================================================

fn setup_test_sqlite_db() -> (Connection, Arc<MemoryEnclave>, Arc<VirtualMemoryEngine>) {
    let conn = Connection::open_in_memory().expect("open in-memory sqlite");

    // Initialize schema
    conn.execute_batch(
        "
        CREATE TABLE vectors_meta (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            vec_id TEXT UNIQUE NOT NULL,
            type TEXT NOT NULL,
            content TEXT NOT NULL,
            domain TEXT NOT NULL DEFAULT 'local',
            category TEXT NOT NULL DEFAULT 'general',
            trace_keywords TEXT NOT NULL DEFAULT '[]',
            source_event_ids TEXT NOT NULL DEFAULT '[]',
            decay_weight REAL NOT NULL DEFAULT 1.0,
            access_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL
        );

        CREATE VIRTUAL TABLE vectors_fts USING fts5(
            content,
            tokenize = 'unicode61 remove_diacritics 0'
        );

        CREATE TABLE l3_nodes (
            id TEXT PRIMARY KEY,
            label TEXT NOT NULL,
            properties TEXT DEFAULT '{}'
        );

        CREATE TABLE l3_edges (
            source TEXT NOT NULL,
            target TEXT NOT NULL,
            relation TEXT NOT NULL,
            weight REAL DEFAULT 1.0,
            obsolete INTEGER DEFAULT 0,
            PRIMARY KEY(source, target, relation)
        );

        CREATE TABLE episodic_memory (
            memory_id TEXT PRIMARY KEY,
            session_id TEXT NOT NULL,
            domain TEXT NOT NULL DEFAULT 'local',
            category TEXT NOT NULL,
            content_encrypted TEXT NOT NULL,
            importance_score REAL NOT NULL DEFAULT 5.0,
            emotional_valence REAL NOT NULL DEFAULT 1.0,
            recall_count INTEGER NOT NULL DEFAULT 0,
            created_at INTEGER NOT NULL,
            last_recalled_at INTEGER NOT NULL,
            base_half_life_secs INTEGER NOT NULL DEFAULT 604800,
            retention_score REAL NOT NULL DEFAULT 1.0
        );
        ",
    )
    .expect("setup sqlite schema");

    let enclave = Arc::new(
        MemoryEnclave::new_with_argon2id(b"master_test_passphrase", b"0123456789abcdef")
            .expect("init enclave"),
    );

    let pool = DatabasePool::new_in_memory().expect("init db pool");
    let vme = Arc::new(VirtualMemoryEngine::new(pool, enclave.clone(), 128));

    (conn, enclave, vme)
}

#[test]
fn test_sqlite_fts5_and_hybrid_integration() {
    let (conn, _enclave, vme) = setup_test_sqlite_db();

    // Insert test documents into vectors_meta and vectors_fts
    let docs = vec![
        ("doc_1", "fact", "Hệ thống bộ nhớ phân tầng 4 lớp của LIVA", "local", "memory"),
        ("doc_2", "fact", "Obsidian Knowledge Graph và liên kết wikilinks", "local", "obsidian"),
        ("doc_3", "fact", "Công cụ tìm kiếm lai 3 kênh Reciprocal Rank Fusion", "local", "search"),
    ];

    let now = chrono::Utc::now().timestamp_millis();
    for (vec_id, r_type, content, domain, category) in &docs {
        conn.execute(
            "INSERT INTO vectors_meta (vec_id, type, content, domain, category, created_at) VALUES (?, ?, ?, ?, ?, ?)",
            rusqlite::params![vec_id, r_type, content, domain, category, now],
        )
        .expect("insert meta");

        let rowid = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO vectors_fts (rowid, content) VALUES (?, ?)",
            rusqlite::params![rowid, content],
        )
        .expect("insert fts");
    }

    // 1. Test BM25 FTS5 Search
    let bm25_hits = vme
        .search_engine
        .sparse_engine
        .search_fts5(&conn, "bo nho liva", 5, None)
        .expect("fts5 search");
    assert!(!bm25_hits.is_empty());
    assert_eq!(bm25_hits[0].vec_id, "doc_1");

    // 2. Test 3-Way Search Facade with Graph
    let graph_edges = vec![
        ("Obsidian", "Wikilinks", 1.0f32),
        ("Wikilinks", "Graph", 1.0f32),
    ];
    let csr = CsrGraph::from_named_edges(&graph_edges, true);
    let graph_engine = HippoRagEngine::new(csr);

    let doc_entity_mappings = vec![
        (1, "doc_1".to_string(), "Doc 1".to_string(), vec![("Memory".to_string(), 1.0f32)]),
        (2, "doc_2".to_string(), "Doc 2".to_string(), vec![("Obsidian".to_string(), 1.0f32)]),
        (3, "doc_3".to_string(), "Doc 3".to_string(), vec![("Search".to_string(), 1.0f32)]),
    ];

    let crypto_engine = EncryptionEngine::new("test_encryption_key_32_bytes_str!");
    let hybrid_hits = vme
        .search_hybrid(
            &conn,
            &crypto_engine,
            "Obsidian Knowledge",
            None,
            Some(&graph_engine),
            Some(&[("Obsidian", 1.0f32)]),
            Some(&doc_entity_mappings),
            5,
            None,
        )
        .expect("search hybrid");

    assert!(!hybrid_hits.is_empty());
    assert_eq!(hybrid_hits[0].vec_id, "doc_2");
    assert_eq!(hybrid_hits[0].source_channel, "3way_rrf");
}

// =========================================================================
// 7. Edge Cases & Boundary Conditions
// =========================================================================

#[test]
fn test_edge_cases_empty_and_extremes() {
    let rrf = TriModalRrfEngine::new();

    // All channels empty
    let empty_res = rrf.fuse(&[], &[], &[], 10);
    assert!(empty_res.is_empty());

    // Single channel only
    let single_hit = vec![SearchHit::new(1, "solo", "Solo hit", "fact", "local", "c", 1.0, "bm25")];
    let single_res = rrf.fuse(&single_hit, &[], &[], 5);
    assert_eq!(single_res.len(), 1);
    assert_eq!(single_res[0].vec_id, "solo");
    let expected_single_score = 0.30 / (60.0 + 1.0);
    assert!((single_res[0].score - expected_single_score).abs() < 1e-6);

    // K = 0 edge clamp
    let rrf_zero_k = TriModalRrfEngine::with_weights(0.0, 0.30, 0.45, 0.25);
    let clamped_res = rrf_zero_k.fuse(&single_hit, &[], &[], 5);
    // K clamps to 1.0, rank is 1 -> score = 0.30 / (1.0 + 1.0) = 0.15
    assert!((clamped_res[0].score - 0.15).abs() < 1e-6);

    // Zero vector similarity
    let zero_vec = vec![0.0f32; 384];
    let sim_zero = cosine_similarity_f32(&zero_vec, &zero_vec);
    assert_eq!(sim_zero, 0.0);
}

#[test]
fn test_metadata_filtering_contract() {
    let filter = MetadataFilter {
        r#type: Some("fact".to_string()),
        domain: Some("local".to_string()),
        category: Some("memory".to_string()),
        created_after: Some(1000),
        created_before: Some(2000),
    };

    assert_eq!(filter.domain.as_deref(), Some("local"));
    assert_eq!(filter.category.as_deref(), Some("memory"));
}
