use std::time::Instant;
use liva_native_core::memory::graph::{HippoRagEngine, PprConfig};

fn main() {
    const NUM_NODES: usize = 100_000;
    const NUM_EDGES: usize = 1_000_000;
    const BENCH_ITERATIONS: usize = 100;

    println!("================================================================================");
    println!("  LIVA Phase 3 — HippoRAG CSR Sparse Matrix & Parallel Rayon PPR Benchmark");
    println!("  Graph Scale: {} nodes, {} edges", NUM_NODES, NUM_EDGES);
    println!("  Iterations:  {} runs", BENCH_ITERATIONS);
    println!("================================================================================");

    let start_gen = Instant::now();
    let graph = HippoRagEngine::generate_synthetic_graph(NUM_NODES, NUM_EDGES);
    let gen_time = start_gen.elapsed().as_secs_f64() * 1000.0;
    let memory_mb = graph.memory_usage_bytes() as f64 / (1024.0 * 1024.0);

    println!("Graph generated in: {:.2} ms | CSR Working Set: {:.2} MB (L3-Cache Friendly)", gen_time, memory_mb);

    let engine = HippoRagEngine::with_config(
        graph,
        PprConfig {
            damping_factor: 0.15,
            max_iterations: 20,
            tolerance: 1e-6,
            chunk_size: 512,
        },
    );

    // Warm-up
    println!("\nWarming up Rayon thread pool and CPU caches...");
    for _ in 0..5 {
        let _ = engine.run_ppr(&[1, 10, 100, 1000], &[1.0, 2.0, 3.0, 4.0]);
    }

    println!("Executing {} benchmark iterations...", BENCH_ITERATIONS);
    let mut latencies_ms = Vec::with_capacity(BENCH_ITERATIONS);

    for i in 0..BENCH_ITERATIONS {
        let seeds = vec![
            ((i * 37) % NUM_NODES) as u32,
            ((i * 149 + 17) % NUM_NODES) as u32,
            ((i * 503 + 31) % NUM_NODES) as u32,
        ];
        let weights = vec![1.5, 2.5, 1.0];

        let res = engine.run_ppr(&seeds, &weights);
        latencies_ms.push(res.elapsed_ms);
    }

    latencies_ms.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mean_ms: f64 = latencies_ms.iter().sum::<f64>() / (BENCH_ITERATIONS as f64);
    let median_ms = latencies_ms[BENCH_ITERATIONS / 2];
    let p90_ms = latencies_ms[(BENCH_ITERATIONS as f64 * 0.90) as usize];
    let p95_ms = latencies_ms[(BENCH_ITERATIONS as f64 * 0.95) as usize];
    let p99_ms = latencies_ms[(BENCH_ITERATIONS as f64 * 0.99) as usize];
    let min_ms = latencies_ms[0];
    let max_ms = latencies_ms[BENCH_ITERATIONS - 1];

    println!("\n--------------------------------------------------------------------------------");
    println!("  BENCHMARK RESULTS (Personalized PageRank on 100,000 Nodes):");
    println!("--------------------------------------------------------------------------------");
    println!("  Min Latency:    {:.3} ms", min_ms);
    println!("  Mean Latency:   {:.3} ms  [Target: <= 8.0 ms]  --> {}", mean_ms, if mean_ms <= 8.0 { "PASS ✅" } else { "FAIL ❌" });
    println!("  Median Latency: {:.3} ms", median_ms);
    println!("  P90 Latency:    {:.3} ms", p90_ms);
    println!("  P95 Latency:    {:.3} ms  [Target: <= 8.5 ms]  --> {}", p95_ms, if p95_ms <= 8.5 { "PASS ✅" } else { "FAIL ❌" });
    println!("  P99 Latency:    {:.3} ms", p99_ms);
    println!("  Max Latency:    {:.3} ms", max_ms);
    println!("================================================================================\n");

    if mean_ms > 8.0 || p95_ms > 8.5 {
        eprintln!("SLA Violation: Latency exceeded acceptable threshold!");
        std::process::exit(1);
    }
}
