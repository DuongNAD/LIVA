//! Quantitative Performance Benchmarks for Storage and Routing (TICKET-16)
//!
//! Validates:
//! 1. `bench_storage_reader_checkout_latency`: Pooled reader checkout latency (SLA: P50 < 0.05ms, P95 < 0.2ms).
//! 2. `bench_storage_wal_write_throughput_and_concurrent_read_latency`: 1,000 writes with concurrent reads (SLA: reader P95 < 1.0ms).
//! 3. `bench_storage_vector_similarity_search_latency`: 1,000 vectors kNN query latency (SLA: P95 < 2.0ms).
//! 4. `bench_ai_router_decision_throughput_and_latency`: Intent P95 < 0.05ms, RouteLLM centroid math P95 < 0.5ms, throughput > 2,000 qps.
//! 5. `bench_agent_stategraph_node_transition_overhead`: Microbenchmark pure graph step transition (SLA: P95 < 0.02ms = 20µs).

use liva_native_core::agent::graph::{ComplexityCentroids, StateGraph, route_intent};
use liva_native_core::agent::state::AgentState;
use liva_native_core::crypto::EncryptionEngine;
use liva_native_core::db::{
    BatchConversationTurn, DatabasePool, MEMORY_VECTOR_DIM, MetadataFilter,
    persist_conversation_event_vectors_batch, search_similar_vectors,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

/// RAII Temporary Database Guard to ensure clean teardown of disk-backed SQLite files.
struct TempDbGuard(PathBuf);
impl Drop for TempDbGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_file(format!("{}-wal", self.0.display()));
        let _ = std::fs::remove_file(format!("{}-shm", self.0.display()));
    }
}

static BENCH_DB_COUNTER: AtomicU64 = AtomicU64::new(800);

fn create_bench_pool() -> (Arc<DatabasePool>, EncryptionEngine, TempDbGuard) {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let counter = BENCH_DB_COUNTER.fetch_add(1, Ordering::Relaxed);
    let rand_id = format!("{}_{}_{}", std::process::id(), nanos, counter);
    let db_path = std::env::temp_dir().join(format!("liva_bench_db_{rand_id}.sqlite"));
    let _ = std::fs::remove_file(&db_path);
    let _ = std::fs::remove_file(format!("{}-wal", db_path.display()));
    let _ = std::fs::remove_file(format!("{}-shm", db_path.display()));
    let pool = DatabasePool::new(&db_path).expect("failed to create benchmark DatabasePool");
    let crypto = EncryptionEngine::new("benchmarks-m3-test-key-32-bytes");
    (Arc::new(pool), crypto, TempDbGuard(db_path))
}

fn generate_normalized_vector(seed: usize) -> Vec<f32> {
    let mut vec = Vec::with_capacity(MEMORY_VECTOR_DIM);
    for i in 0..MEMORY_VECTOR_DIM {
        let val = ((seed * 31 + i * 17) % 1000) as f32 / 1000.0 - 0.5;
        vec.push(val);
    }
    let norm = vec.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        for x in vec.iter_mut() {
            *x /= norm;
        }
    }
    vec
}

// =========================================================================
// BENCHMARK 1: STORAGE READER CHECKOUT LATENCY SLA (P50 < 0.05ms, P95 < 0.2ms)
// =========================================================================

#[test]
fn bench_storage_reader_checkout_latency() {
    let (pool, _crypto, _guard) = create_bench_pool();

    let iterations = 10_000;
    let mut latencies_us = Vec::with_capacity(iterations);

    // Warm-up run
    for _ in 0..100 {
        let conn = pool.read_conn().expect("warmup checkout");
        drop(conn);
    }

    for _ in 0..iterations {
        let start = Instant::now();
        let conn = pool.read_conn().expect("checkout connection");
        let elapsed_us = start.elapsed().as_micros() as u64;
        latencies_us.push(elapsed_us);
        drop(conn);
    }

    latencies_us.sort_unstable();

    let p50_ms = latencies_us[iterations * 50 / 100] as f64 / 1000.0;
    let p90_ms = latencies_us[iterations * 90 / 100] as f64 / 1000.0;
    let p95_ms = latencies_us[iterations * 95 / 100] as f64 / 1000.0;
    let p99_ms = latencies_us[iterations * 99 / 100] as f64 / 1000.0;
    let max_ms = *latencies_us.last().unwrap() as f64 / 1000.0;

    println!("\n=== BENCHMARK 1: STORAGE READER CHECKOUT LATENCY (r2d2 SQLite Pool) ===");
    println!("  Iterations : {iterations}");
    println!("  P50 Latency: {p50_ms:.4} ms");
    println!("  P90 Latency: {p90_ms:.4} ms");
    println!("  P95 Latency: {p95_ms:.4} ms");
    println!("  P99 Latency: {p99_ms:.4} ms");
    println!("  Max Latency: {max_ms:.4} ms");

    // Assert SLA: P50 < 0.05ms, P95 < 0.2ms
    assert!(
        p50_ms < 0.05,
        "SLA VIOLATION: Reader checkout P50 ({p50_ms:.4}ms) exceeded 0.05ms ceiling!"
    );
    assert!(
        p95_ms < 0.2,
        "SLA VIOLATION: Reader checkout P95 ({p95_ms:.4}ms) exceeded 0.2ms ceiling!"
    );
}

// =========================================================================
// BENCHMARK 2: WAL WRITE THROUGHPUT & CONCURRENT READ LATENCY (P95 < 1.0ms)
// =========================================================================

#[tokio::test]
async fn bench_storage_wal_write_throughput_and_concurrent_read_latency() {
    let (pool, _crypto, _guard) = create_bench_pool();

    pool.with_writer(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS bench_throughput (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                seq INTEGER NOT NULL,
                data TEXT NOT NULL
            );",
        )
    })
    .expect("setup throughput table");

    let num_writes = 1_000;
    let stop_readers = Arc::new(AtomicBool::new(false));
    let reader_latencies_us = Arc::new(std::sync::Mutex::new(Vec::with_capacity(5_000)));

    // Spawn 4 background concurrent reader tasks
    let mut reader_handles = Vec::new();
    for _ in 0..4 {
        let pool_c = pool.clone();
        let stop_c = stop_readers.clone();
        let lats_c = reader_latencies_us.clone();

        reader_handles.push(tokio::spawn(async move {
            while !stop_c.load(Ordering::Relaxed) {
                let start = Instant::now();
                let _ = pool_c.with_reader(|conn| {
                    let count: i64 =
                        conn.query_row("SELECT COUNT(*) FROM bench_throughput;", [], |row| {
                            row.get(0)
                        })?;
                    Ok(count)
                });
                let elapsed = start.elapsed().as_micros() as u64;
                if let Ok(mut guard) = lats_c.lock() {
                    guard.push(elapsed);
                }
                tokio::task::yield_now().await;
            }
        }));
    }

    // Execute 1,000 writes in batches of 50 via DbActor
    let write_start = Instant::now();
    for batch_idx in 0..(num_writes / 50) {
        let res = pool
            .spawn_writer(move |conn| {
                let tx = conn.unchecked_transaction()?;
                let mut stmt = tx.prepare_cached(
                    "INSERT INTO bench_throughput (seq, data) VALUES (?1, 'benchmark_payload_val');",
                )?;
                for i in 0..50 {
                    let seq = batch_idx * 50 + i;
                    stmt.execute([seq])?;
                }
                drop(stmt);
                tx.commit()?;
                Ok(())
            })
            .await;
        assert!(res.is_ok(), "Write batch {batch_idx} failed: {:?}", res);
    }
    let write_duration = write_start.elapsed();
    let write_tps = num_writes as f64 / write_duration.as_secs_f64();

    // Ensure at least 50 samples before stopping readers
    while reader_latencies_us.lock().unwrap().len() < 50 {
        tokio::task::yield_now().await;
    }

    // Stop readers
    stop_readers.store(true, Ordering::SeqCst);
    for rh in reader_handles {
        rh.await.expect("reader handle");
    }

    let mut reader_lats = reader_latencies_us.lock().unwrap().clone();
    assert!(
        reader_lats.len() >= 50,
        "Must have captured sufficient reader samples during write benchmark"
    );
    reader_lats.sort_unstable();

    let r_p50 = reader_lats[reader_lats.len() * 50 / 100] as f64 / 1000.0;
    let r_p95 = reader_lats[reader_lats.len() * 95 / 100] as f64 / 1000.0;
    let r_p99 = reader_lats[reader_lats.len() * 99 / 100] as f64 / 1000.0;

    println!("\n=== BENCHMARK 2: WAL WRITE THROUGHPUT & CONCURRENT READ LATENCY ===");
    println!("  Writes Executed  : {num_writes} records");
    println!("  Write Duration   : {write_duration:?}");
    println!("  Write Throughput : {write_tps:.1} tx/sec");
    println!("  Reader Queries   : {}", reader_lats.len());
    println!("  Reader P50 Under Write Load: {r_p50:.3} ms");
    println!("  Reader P95 Under Write Load: {r_p95:.3} ms");
    println!("  Reader P99 Under Write Load: {r_p99:.3} ms");

    // Assert SLA: reader P95 under write load < 1.0ms
    assert!(
        r_p95 < 1.0,
        "SLA VIOLATION: Reader P95 under write load ({r_p95:.3}ms) exceeded 1.0ms ceiling!"
    );
}

// =========================================================================
// BENCHMARK 3: VECTOR SIMILARITY SEARCH LATENCY (1,000 VECTORS, P95 < 2.0ms)
// =========================================================================

#[test]
fn bench_storage_vector_similarity_search_latency() {
    let (pool, crypto, _guard) = create_bench_pool();

    let num_vectors = 1_000;
    let mut raw_vectors = Vec::with_capacity(num_vectors);
    for i in 0..num_vectors {
        raw_vectors.push(generate_normalized_vector(i));
    }

    // Populate 1,000 vectors into sqlite-vec virtual table `vec_idx`
    let conn = pool.writer.get().expect("writer conn");
    let batch_items: Vec<BatchConversationTurn> = (0..num_vectors)
        .map(|i| {
            let event_id = Box::leak(format!("vec_evt_{i}").into_boxed_str());
            let content = Box::leak(format!("Semantic content item number {i}").into_boxed_str());
            BatchConversationTurn {
                event_id,
                content,
                vector: &raw_vectors[i],
                domain: "benchmark",
                category: "memory",
            }
        })
        .collect();

    persist_conversation_event_vectors_batch(&conn, &crypto, &batch_items)
        .expect("batch insert 1,000 vectors into sqlite-vec");
    drop(conn);

    // Warmup queries
    let reader_conn = pool.read_conn().expect("reader conn");
    for w in 0..5 {
        let warmup_query = generate_normalized_vector(42 + w);
        let _ = search_similar_vectors(
            &reader_conn,
            &crypto,
            &warmup_query,
            5,
            &MetadataFilter::default(),
        )
        .expect("warmup vector search");
    }

    // Benchmark 500 kNN vector queries
    let query_iterations = 500;
    let mut latencies_us = Vec::with_capacity(query_iterations);

    for q_idx in 0..query_iterations {
        let query_vec = generate_normalized_vector(1000 + q_idx);
        let start = Instant::now();
        let results = search_similar_vectors(
            &reader_conn,
            &crypto,
            &query_vec,
            5,
            &MetadataFilter::default(),
        )
        .expect("kNN vector search");
        let elapsed_us = start.elapsed().as_micros() as u64;
        latencies_us.push(elapsed_us);
        assert!(!results.is_empty(), "kNN must return matching candidates");
    }

    latencies_us.sort_unstable();

    let p50_ms = latencies_us[query_iterations * 50 / 100] as f64 / 1000.0;
    let p90_ms = latencies_us[query_iterations * 90 / 100] as f64 / 1000.0;
    let p95_ms = latencies_us[query_iterations * 95 / 100] as f64 / 1000.0;
    let p99_ms = latencies_us[query_iterations * 99 / 100] as f64 / 1000.0;

    println!(
        "\n=== BENCHMARK 3: VECTOR SIMILARITY SEARCH LATENCY (1,000 vectors, 384-dim, sqlite-vec) ==="
    );
    println!("  Indexed Vectors : {num_vectors}");
    println!("  Query Count     : {query_iterations}");
    println!("  P50 Latency     : {p50_ms:.3} ms");
    println!("  P90 Latency     : {p90_ms:.3} ms");
    println!("  P95 Latency     : {p95_ms:.3} ms");
    println!("  P99 Latency     : {p99_ms:.3} ms");

    // Assert SLA: kNN query P95 < 2.0ms (4.0ms ceiling under unoptimized debug build)
    let p95_ceiling = if cfg!(debug_assertions) { 4.0 } else { 2.0 };
    assert!(
        p95_ms < p95_ceiling,
        "SLA VIOLATION: Vector similarity search P95 ({p95_ms:.3}ms) exceeded {p95_ceiling}ms ceiling!"
    );

    drop(reader_conn);
    drop(pool);
}

// =========================================================================
// BENCHMARK 4: AI ROUTER DECISION THROUGHPUT & LATENCY
// =========================================================================

#[test]
fn bench_ai_router_decision_throughput_and_latency() {
    let sample_queries = [
        "thời tiết Hà Nội hôm nay thế nào",
        "bật đèn phòng khách giúp mình",
        "tăng âm lượng lên một chút",
        "nhắn cho Nam bảo mai đi cà phê nhé",
        "nhìn màn hình xem có gì bất thường",
        "giải thích thuật toán tìm kiếm nhị phân",
        "tắt quạt đi",
        "chuyển bài khác giúp mình",
        "hôm nay trời có mưa không",
        "Xin chào LIVA",
    ];

    let iterations = 5_000;
    let mut intent_lats_us = Vec::with_capacity(iterations);

    let start_all = Instant::now();
    for i in 0..iterations {
        let q = sample_queries[i % sample_queries.len()];
        let start = Instant::now();
        let _ = route_intent(q);
        let elapsed = start.elapsed().as_micros() as u64;
        intent_lats_us.push(elapsed);
    }
    let total_time = start_all.elapsed();
    let throughput = iterations as f64 / total_time.as_secs_f64();

    intent_lats_us.sort_unstable();
    let intent_p50_ms = intent_lats_us[iterations * 50 / 100] as f64 / 1000.0;
    let intent_p95_ms = intent_lats_us[iterations * 95 / 100] as f64 / 1000.0;

    // Benchmark RouteLLM Centroid Cosine Math
    let centroids = ComplexityCentroids::canonical();
    let test_vec = generate_normalized_vector(99);
    let mut centroid_lats_us = Vec::with_capacity(iterations);

    for _ in 0..iterations {
        let start = Instant::now();
        let _ = centroids.classify_vector_scored(&test_vec);
        let elapsed = start.elapsed().as_micros() as u64;
        centroid_lats_us.push(elapsed);
    }

    centroid_lats_us.sort_unstable();
    let centroid_p50_ms = centroid_lats_us[iterations * 50 / 100] as f64 / 1000.0;
    let centroid_p95_ms = centroid_lats_us[iterations * 95 / 100] as f64 / 1000.0;

    println!("\n=== BENCHMARK 4: AI ROUTER DECISION THROUGHPUT & LATENCY ===");
    println!("  Iterations          : {iterations}");
    println!("  Overall Throughput  : {throughput:.1} queries/sec");
    println!("  Intent Route P50    : {intent_p50_ms:.4} ms");
    println!("  Intent Route P95    : {intent_p95_ms:.4} ms");
    println!("  Centroid Math P50   : {centroid_p50_ms:.4} ms");
    println!("  Centroid Math P95   : {centroid_p95_ms:.4} ms");

    // Assert SLAs:
    // - Intent classification P95 < 0.05ms (50µs)
    // - RouteLLM centroid math P95 < 0.5ms (500µs)
    // - Throughput > 2,000 queries/sec
    assert!(
        intent_p95_ms < 0.05,
        "SLA VIOLATION: Intent classification P95 ({intent_p95_ms:.4}ms) exceeded 0.05ms ceiling!"
    );
    assert!(
        centroid_p95_ms < 0.5,
        "SLA VIOLATION: Centroid cosine math P95 ({centroid_p95_ms:.4}ms) exceeded 0.5ms ceiling!"
    );
    assert!(
        throughput > 2000.0,
        "SLA VIOLATION: AI Router throughput ({throughput:.1} qps) below 2,000 qps floor!"
    );
}

// =========================================================================
// BENCHMARK 5: AGENT STATEGRAPH PURE NODE TRANSITION OVERHEAD (< 0.02ms / 20µs)
// =========================================================================

#[tokio::test]
async fn bench_agent_stategraph_node_transition_overhead() {
    let mut graph = StateGraph::new();

    // Construct a 10-node linear DAG: n0 -> n1 -> n2 -> ... -> n9 -> __END__
    for i in 0..10 {
        let node_name = format!("n{i}");
        let next_node = if i == 9 {
            "__END__".to_string()
        } else {
            format!("n{}", i + 1)
        };

        graph.add_node(&node_name, move |mut state: AgentState| {
            let next = next_node.clone();
            async move {
                state.current_node = next;
                Ok(state)
            }
        });
    }

    graph.set_entry_point("n0");

    let runs = 1_000;
    let mut transition_lats_ns = Vec::with_capacity(runs);

    // Warm-up run
    let init_state = AgentState {
        messages: vec![],
        current_node: "n0".to_string(),
        context: Default::default(),
    };
    let _ = graph.run(init_state.clone()).await.expect("warmup run");

    for _ in 0..runs {
        let start = Instant::now();
        let _ = graph.run(init_state.clone()).await.expect("graph run");
        let total_ns = start.elapsed().as_nanos() as u64;
        // 10 transitions per run
        let per_transition_ns = total_ns / 10;
        transition_lats_ns.push(per_transition_ns);
    }

    transition_lats_ns.sort_unstable();

    let p50_us = transition_lats_ns[runs * 50 / 100] as f64 / 1000.0;
    let p95_us = transition_lats_ns[runs * 95 / 100] as f64 / 1000.0;
    let p99_us = transition_lats_ns[runs * 99 / 100] as f64 / 1000.0;
    let p95_ms = p95_us / 1000.0;

    println!("\n=== BENCHMARK 5: AGENT STATEGRAPH PURE TRANSITION OVERHEAD ===");
    println!("  Total Runs          : {runs} (10,000 transitions)");
    println!(
        "  P50 Per-Transition  : {p50_us:.3} µs ({:.5} ms)",
        p50_us / 1000.0
    );
    println!("  P95 Per-Transition  : {p95_us:.3} µs ({p95_ms:.5} ms)");
    println!(
        "  P99 Per-Transition  : {p99_us:.3} µs ({:.5} ms)",
        p99_us / 1000.0
    );

    // Assert SLA: P95 < 0.02ms (20µs)
    assert!(
        p95_ms < 0.02,
        "SLA VIOLATION: StateGraph transition overhead P95 ({p95_ms:.5}ms = {p95_us:.2}µs) exceeded 0.02ms ceiling!"
    );
}
