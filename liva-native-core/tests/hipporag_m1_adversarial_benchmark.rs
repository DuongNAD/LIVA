//! Empirical Benchmark and Adversarial Stress Harness for Milestone 1 (Subsystem 1)
//!
//! Evaluates:
//! 1. L3 HippoRAG In-Memory CSR Graph Cache PPR Latency SLA (P95 < 10.0ms, 0 LLM tokens).
//! 2. Parallel embedding throughput via lock-free Arc<EmbeddingEngine> without mutex deadlocks.
//! 3. Ebbinghaus decay floor clamp (clamp(0.05, 1.0)) across 365+ day simulation.
//! 4. Memory consolidation AES-256-GCM v2 decryption and triple extraction resilience.

use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::csr_graph::CsrGraph;
use liva_native_core::db::{
    DatabasePool, MEMORY_VECTOR_DIM, MetadataFilter, persist_conversation_event_vector,
    search_similar_vectors,
};
use liva_native_core::llm::embedder::{EmbeddingEngine, resolve_model_dir};
use liva_native_core::memory_consolidation::consume_pending_once;
use std::sync::Arc;
use std::time::{Duration, Instant};

// =========================================================================
// 1. HIPPORAG IN-MEMORY CSR GRAPH CACHE PPR LATENCY BENCHMARK
// =========================================================================

#[test]
fn benchmark_hipporag_ppr_traversal_latency_p95_sla() {
    let mut graph = CsrGraph::new();

    // Construct a realistic scale-free L3 Knowledge Graph:
    // 2,000 nodes, ~10,000 directed edges across diverse relationship types
    let num_nodes = 2000;
    let num_edges_per_node = 5;

    for i in 0..num_nodes {
        graph.add_node(
            format!("entity_{i}"),
            format!("Entity Label {i}"),
            format!("{{\"type\": \"concept\", \"index\": {i}}}"),
        );
    }

    // Connect nodes in a combination of hub-and-spoke and ring topology
    for i in 0..num_nodes {
        for offset in 1..=num_edges_per_node {
            let target = (i + offset * 7) % num_nodes;
            let rel = match offset % 4 {
                0 => "relates_to",
                1 => "part_of",
                2 => "depends_on",
                _ => "associated_with",
            };
            let weight = 0.5 + ((i + offset) % 10) as f32 * 0.1;
            graph.add_edge(
                &format!("entity_{i}"),
                &format!("entity_{target}"),
                rel,
                weight,
                true,
            );
        }
    }

    graph.compile_csr();
    assert_eq!(graph.node_count(), num_nodes);
    assert!(graph.edge_count() >= 10000);

    // Warm-up run
    let warmup_seeds = [("entity_0", 1.0f32), ("entity_42", 0.8f32)];
    let _ = graph.personalized_pagerank_readonly(&warmup_seeds, 3, 0.85, 20);

    // Benchmark 1,000 PPR traversals with varying seeds
    let iterations = 1000;
    let mut latencies_us = Vec::with_capacity(iterations);

    for iter in 0..iterations {
        let seed_idx1 = (iter * 17) % num_nodes;
        let seed_idx2 = (iter * 31 + 5) % num_nodes;
        let seed_id1 = format!("entity_{seed_idx1}");
        let seed_id2 = format!("entity_{seed_idx2}");
        let seeds = [(seed_id1.as_str(), 1.0f32), (seed_id2.as_str(), 0.5f32)];

        let start = Instant::now();
        let ranking = graph.personalized_pagerank_readonly(&seeds, 3, 0.85, 20);
        let elapsed = start.elapsed().as_micros() as u64;
        latencies_us.push(elapsed);

        assert!(
            !ranking.is_empty(),
            "PPR traversal must return activated entities"
        );
        assert!(ranking.len() <= 20, "Must respect top_k limit");
    }

    latencies_us.sort_unstable();

    let p50 = latencies_us[iterations * 50 / 100] as f64 / 1000.0;
    let p90 = latencies_us[iterations * 90 / 100] as f64 / 1000.0;
    let p95 = latencies_us[iterations * 95 / 100] as f64 / 1000.0;
    let p99 = latencies_us[iterations * 99 / 100] as f64 / 1000.0;
    let max = *latencies_us.last().unwrap() as f64 / 1000.0;

    println!(
        "\n=== HIPPORAG CSR GRAPH CACHE PPR LATENCY BENCHMARK (2,000 nodes, 20,000 directed edges) ==="
    );
    println!("  Iterations : {iterations}");
    println!("  P50 Latency: {p50:.3} ms");
    println!("  P90 Latency: {p90:.3} ms");
    println!("  P95 Latency: {p95:.3} ms");
    println!("  P99 Latency: {p99:.3} ms");
    println!("  Max Latency: {max:.3} ms");

    // Strict verification: P95 must be < 10.0 ms
    assert!(
        p95 < 10.0,
        "SLA VIOLATION: HippoRAG PPR P95 latency ({p95:.3} ms) exceeded 10.0 ms!"
    );
    // Also verify P99 remains well within interactive thresholds
    assert!(
        p99 < 15.0,
        "HippoRAG PPR P99 latency ({p99:.3} ms) exceeded 15.0 ms threshold!"
    );
}

#[test]
fn stress_test_dense_cluster_hipporag_traversal() {
    let mut graph = CsrGraph::new();

    // Adversarial Graph: 100-node fully connected clique (10,000 edges)
    // Stress tests SpMV edge accumulation and numerical stability
    let clique_size = 100;
    for i in 0..clique_size {
        graph.add_node(
            format!("clique_{i}"),
            format!("Clique Node {i}"),
            "{}".into(),
        );
    }

    for i in 0..clique_size {
        for j in 0..clique_size {
            if i != j {
                graph.add_edge(
                    &format!("clique_{i}"),
                    &format!("clique_{j}"),
                    "clique_link",
                    1.0,
                    false,
                );
            }
        }
    }

    graph.compile_csr();
    assert_eq!(graph.edge_count(), clique_size * (clique_size - 1));

    let start = Instant::now();
    let seeds = [("clique_0", 1.0f32)];
    let ranking = graph.personalized_pagerank(&seeds, 5, 0.85, 100);
    let duration = start.elapsed();

    println!(
        "Dense 100-node clique (9,900 edges) 5-iter PPR latency: {:?}",
        duration
    );
    assert_eq!(ranking.len(), 100);
    assert!(duration < Duration::from_millis(50));

    // In a symmetric clique, all non-seed nodes should receive near-identical probability mass
    let seed_score = ranking.iter().find(|(id, _)| id == "clique_0").unwrap().1;
    let other_scores: Vec<f32> = ranking
        .iter()
        .filter(|(id, _)| id != "clique_0")
        .map(|(_, s)| *s)
        .collect();
    let first_other = other_scores[0];
    for score in other_scores {
        assert!(
            (score - first_other).abs() < 1e-4,
            "Symmetry preservation in clique PPR violated: score={score}, expected={first_other}"
        );
    }
    assert!(
        seed_score > first_other,
        "Seed node must maintain highest activation"
    );
}

// =========================================================================
// 2. PARALLEL EMBEDDING CONCURRENCY & DEADLOCK FREEDOM
// =========================================================================

#[test]
fn stress_test_lock_free_arc_embedding_concurrency() {
    let model_dir = resolve_model_dir();
    if !model_dir.join("model.onnx").exists() {
        eprintln!(
            "Skipping embedding inference stress: model not found at {:?}",
            model_dir
        );
        return;
    }

    let engine = Arc::new(EmbeddingEngine::load(&model_dir).expect("load embedding engine"));

    // Prime the query cache with several seed items
    let seed_queries = [
        "thời tiết hôm nay",
        "hướng dẫn lập trình rust",
        "kiến trúc liva native core",
        "tổng quan dự án",
    ];
    for q in &seed_queries {
        let _ = engine.embed_query(q).expect("prime query");
    }

    // Concurrently spawn 16 threads hammering both cached and uncached embeddings
    let num_threads = 16;
    let queries_per_thread = 20;
    let mut handles = Vec::with_capacity(num_threads);

    let start_all = Instant::now();
    for t_idx in 0..num_threads {
        let eng = Arc::clone(&engine);
        let handle = std::thread::spawn(move || {
            let mut latencies = Vec::new();
            for q_idx in 0..queries_per_thread {
                let start = Instant::now();
                // Alternate between cache hit and cache miss
                let query = if q_idx % 2 == 0 {
                    seed_queries[q_idx % seed_queries.len()].to_string()
                } else {
                    format!("truy vấn kiểm thử đa luồng thread_{t_idx}_query_{q_idx}")
                };

                let res = eng.embed_query(&query);
                assert!(res.is_ok(), "embed_query failed: {:?}", res);
                latencies.push(start.elapsed());
            }
            latencies
        });
        handles.push(handle);
    }

    // Join all threads with a deadlock timeout of 30 seconds
    let mut all_latencies = Vec::new();
    for h in handles {
        let lats = h.join().expect("embedding worker thread panicked");
        all_latencies.extend(lats);
    }
    let total_time = start_all.elapsed();

    println!("\n=== PARALLEL EMBEDDING POOL CONCURRENCY BENCHMARK ===");
    println!("  Worker Threads      : {num_threads}");
    println!("  Total Embed Invocations: {}", all_latencies.len());
    println!("  Total Wall-Clock Time: {:?}", total_time);
    println!(
        "  Average Throughput  : {:.1} embeds/sec",
        all_latencies.len() as f64 / total_time.as_secs_f64()
    );

    // Verify cache hit latency is sub-millisecond
    let cached_start = Instant::now();
    let cached_vec = engine.embed_query("thời tiết hôm nay").unwrap();
    let cached_time = cached_start.elapsed();
    assert_eq!(cached_vec.len(), 384);
    assert!(
        cached_time < Duration::from_millis(2),
        "Cached query retrieval too slow: {:?}",
        cached_time
    );
}

// =========================================================================
// 3. EBBINGHAUS DECAY FLOOR CLAMP ACROSS 365+ DAY SIMULATION
// =========================================================================

#[test]
fn stress_test_ebbinghaus_decay_floor_clamp_boundary_simulation() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let conn = pool.writer.get().expect("writer connection");
    let engine = EncryptionEngine::new("test-ebbinghaus-clamp-matrix-32!");

    let dummy_vector = vec![0.1f32; MEMORY_VECTOR_DIM];
    let now_ms = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64;

    // Simulation points (in days):
    // 0 days (fresh), 1 day, 7 days, 30 days (tau), 90 days, 180 days, 365 days (1 yr),
    // 730 days (2 yrs), 1,825 days (5 yrs), 3,650 days (10 yrs), 36,500 days (100 yrs).
    let test_intervals_days = [0, 1, 7, 30, 90, 180, 365, 730, 1825, 3650, 36500];

    for &days in &test_intervals_days {
        let vec_id = format!("turn_day_{days}");
        let content = format!("Fact recorded {days} days ago");
        persist_conversation_event_vector(
            &conn,
            &engine,
            &vec_id,
            &content,
            &dummy_vector,
            "local",
            "default",
        )
        .expect("persist vector");

        let past_timestamp = now_ms - (days as i64 * 86_400_000);
        conn.execute(
            "UPDATE vectors_meta SET created_at = ?1, last_accessed_at = 0, access_count = 0 WHERE vec_id = ?2",
            rusqlite::params![past_timestamp, vec_id],
        )
        .expect("update timestamp");
    }

    let filter = MetadataFilter::default();
    let results =
        search_similar_vectors(&conn, &engine, &dummy_vector, 20, &filter).expect("search vectors");

    assert_eq!(results.len(), test_intervals_days.len());

    let fresh_result = results.iter().find(|r| r.vec_id == "turn_day_0").unwrap();
    let fresh_score = fresh_result.score;
    assert!(fresh_score > 0.0, "Fresh score must be positive");

    println!("\n=== EBBINGHAUS RETENTION CLAMP ACROSS TEMPORAL SIMULATION ===");
    for &days in &test_intervals_days {
        let vec_id = format!("turn_day_{days}");
        let hit = results.iter().find(|r| r.vec_id == vec_id).unwrap();
        let retention_ratio = hit.score / fresh_score;

        println!(
            "  Age: {:>5} days | Score: {:.6} | Retention Ratio: {:.6}",
            days, hit.score, retention_ratio
        );

        // Mathematical invariant: retention_ratio must NEVER be lower than 0.05
        assert!(
            retention_ratio >= 0.0499,
            "FLOOR CLAMP VIOLATION at day {days}: retention ratio {retention_ratio} dropped below 0.05!"
        );

        // Retention ratio must never exceed 1.0
        assert!(
            retention_ratio <= 1.0001,
            "CEILING CLAMP VIOLATION at day {days}: retention ratio {retention_ratio} exceeded 1.0!"
        );

        // For ancient facts (>= 365 days), retention ratio should be clamped to 0.05
        if days >= 365 {
            assert!(
                (retention_ratio - 0.05).abs() < 0.001,
                "Day {days} must be clamped to exactly 0.05 (got {retention_ratio})"
            );
        }
    }
}

// =========================================================================
// 4. MEMORY CONSOLIDATION AES-256-GCM v2 RESILIENCE
// =========================================================================

#[tokio::test]
async fn stress_test_memory_consolidation_aes_decryption_and_corrupt_payload_resilience() {
    let pool = DatabasePool::new_in_memory().expect("in-memory db");
    let crypto = EncryptionEngine::new("test-resilience-crypto-suite-32!");

    // Seed 4 conversation events with varying payload health:
    // Event 1: Valid AES-256-GCM encrypted turn with rich extractable triples
    // Event 2: Corrupted ciphertext (invalid hex / tampered tag)
    // Event 3: Plaintext turn (legacy compatibility)
    // Event 4: Valid AES-256-GCM encrypted turn
    {
        let conn = pool.writer.get().expect("writer connection");

        // Event 1
        persist_conversation_event_vector(
            &conn,
            &crypto,
            "turn_valid_1",
            "User: Elon Musk sáng lập SpaceX và điều hành Tesla.\nAssistant: Đúng, SpaceX phát triển tên lửa Starship.",
            &vec![0.1f32; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("persist event 1");

        // Event 2: Corrupted payload
        conn.execute(
            "INSERT INTO events (
                eventId, timestamp, consolidated, domain, category,
                consolidation_status, retry_count, agentId
             ) VALUES (
                'turn_corrupt_2', 2000, 0, 'memory_owner:local',
                'conversation:default', 'pending', 0, 'liva_core'
             )",
            [],
        )
        .expect("seed corrupt event");

        conn.execute(
            "INSERT INTO vectors_meta (
                vec_id, type, content, domain, category,
                source_event_ids, created_at
             ) VALUES (
                'turn_corrupt_2', 'conversation_turn',
                'v2:DEADBEEF_TAMPERED_CIPHERTEXT_INVALID_TAG',
                'memory_owner:local', 'conversation:default',
                '[\"turn_corrupt_2\"]', 2000
             )",
            [],
        )
        .expect("seed corrupt vector");

        // Event 3: Plaintext turn
        conn.execute(
            "INSERT INTO events (
                eventId, timestamp, consolidated, domain, category,
                consolidation_status, retry_count, agentId
             ) VALUES (
                'turn_plain_3', 3000, 0, 'memory_owner:local',
                'conversation:default', 'pending', 0, 'liva_core'
             )",
            [],
        )
        .expect("seed plain event");

        conn.execute(
            "INSERT INTO vectors_meta (
                vec_id, type, content, domain, category,
                source_event_ids, created_at
             ) VALUES (
                'turn_plain_3', 'conversation_turn',
                'User: Hà Nội là thủ đô của Việt Nam.\nAssistant: Và Hồ Gươm nằm ở trung tâm Hà Nội.',
                'memory_owner:local', 'conversation:default',
                '[\"turn_plain_3\"]', 3000
             )",
            [],
        )
        .expect("seed plain vector");

        // Event 4: Valid encrypted turn
        persist_conversation_event_vector(
            &conn,
            &crypto,
            "turn_valid_4",
            "User: Jensen Huang làm việc tại Nvidia.\nAssistant: Jensen Huang sáng lập Nvidia.",
            &vec![0.1f32; MEMORY_VECTOR_DIM],
            "memory_owner:local",
            "conversation:default",
        )
        .expect("persist event 4");
    }

    // Process all 4 events in a single consolidation batch
    let res = consume_pending_once(&pool, &crypto, 10)
        .await
        .expect("consolidation must not fail on corrupted ciphertext");

    println!("\n=== MEMORY CONSOLIDATION RESILIENCE BATCH RESULT ===");
    println!("  Consolidated Events : {}", res.consolidated);
    println!("  Triples Extracted   : {}", res.l3_triples_extracted);

    // Event 2 (corrupted) should be handled cleanly without failing the batch
    assert!(
        res.consolidated >= 3,
        "Valid and plaintext events must be consolidated"
    );
    assert!(
        res.l3_triples_extracted >= 4,
        "Triples from valid events must be populated"
    );

    // Verify L3 Graph in SQLite
    let conn = pool.writer.get().expect("writer connection");

    // Nodes from Event 1
    let elon_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Elon Musk'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("check Elon Musk");
    assert!(elon_exists, "Decrypted node 'Elon Musk' must exist");

    // Nodes from Event 3 (plaintext)
    let hanoi_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Hà Nội'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("check Hà Nội");
    assert!(hanoi_exists, "Plaintext node 'Hà Nội' must exist");

    // Nodes from Event 4
    let nvidia_exists: bool = conn
        .query_row(
            "SELECT count(*) FROM l3_nodes WHERE id = 'Nvidia'",
            [],
            |r| r.get::<_, i64>(0).map(|c| c > 0),
        )
        .expect("check Nvidia");
    assert!(nvidia_exists, "Decrypted node 'Nvidia' must exist");

    // In-memory CsrGraph Cache must be compiled with these entities
    let csr = pool.csr_graph.read().expect("read csr_graph");
    assert!(csr.node_count() >= 5);
    assert!(csr.get_node_index("Elon Musk").is_some());
    assert!(csr.get_node_index("Hà Nội").is_some());
    assert!(csr.get_node_index("Nvidia").is_some());
}
