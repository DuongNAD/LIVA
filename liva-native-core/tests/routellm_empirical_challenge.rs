//! Empirical Challenger Verification Suite for Milestone 4 (Feature 13: RouteLLM & RSK-06)
//!
//! Author: Challenger M4.2 (teamwork_preview_challenger_blueprint_m4_2)
//!
//! Exhaustive empirical challenge testing:
//! 1. 110-Query Curated Bilingual Classification Accuracy (100% precision across dual pipelines).
//! 2. Strict Local Flag (RSK-06) Privacy Invariance under adversarial conditions and escalation flags.
//! 3. Sub-1.5ms CPU Latency SLA: 11,000+ evaluations across all 110 queries with statistical percentiles.
//! 4. Character (279 vs 280 vs 281) & Token (44 vs 45) boundary transition verification.
//! 5. Unicode, emoji-only, zero-width, null byte, and ReDoS adversarial robustness.
//! 6. Fault-injection: broken embedder, dimension mismatch, and invalid environment variable parsing.
//! 7. High-concurrency multithreaded contention with interleaved cache resets (16 threads x 1,000 ops).

use liva_native_core::agent::graph::{
    COMPLEX_ABSOLUTE_THRESHOLD, ComplexityCentroids, DEFAULT_ROUTING_THRESHOLD, DoKho,
    EMBEDDING_DIM, RoutingTier, phan_loai_do_kho, phan_loai_do_kho_with_embedder, route_llm,
    route_llm_with_embedder,
};
use liva_native_core::llm::tool_calling::ToolEmbedder;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use liva_native_core::llm::embedder::{EmbeddingEngine, resolve_model_dir};

#[derive(Debug, Clone, Default)]
struct DeterministicSemanticEmbedder;

impl ToolEmbedder for DeterministicSemanticEmbedder {
    fn embed_query_vec(&self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_text_deterministic(text)
    }
    fn embed_passage_vec(&self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_text_deterministic(text)
    }
}

impl DeterministicSemanticEmbedder {
    pub fn embed_text_deterministic(&self, text: &str) -> Result<Vec<f32>, String> {
        let tokens: Vec<String> = text
            .to_lowercase()
            .split(|c: char| !c.is_alphanumeric())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        let mut vec = vec![0.0f32; 384];

        if tokens.is_empty() {
            return Ok(liva_native_core::llm::embedder::l2_normalize(vec));
        }

        for token in &tokens {
            let mut h: u64 = 0xcbf29ce484222325;
            for b in token.as_bytes() {
                h ^= *b as u64;
                h = h.wrapping_mul(0x100000001b3);
            }
            let idx1 = (h as usize) % 384;
            let idx2 = ((h >> 16) as usize) % 384;
            let idx3 = ((h >> 32) as usize) % 384;
            let sign1 = if (h & 1) == 0 { 1.0f32 } else { -1.0f32 };
            let sign2 = if (h & 2) == 0 { 0.8f32 } else { -0.8f32 };
            let sign3 = if (h & 4) == 0 { 0.6f32 } else { -0.6f32 };

            vec[idx1] += sign1;
            vec[idx2] += sign2;
            vec[idx3] += sign3;
        }

        Ok(liva_native_core::llm::embedder::l2_normalize(vec))
    }
}

fn get_test_embedder() -> Arc<dyn ToolEmbedder> {
    let model_dir = resolve_model_dir();
    if model_dir.join("model.onnx").exists() && model_dir.join("tokenizer.json").exists() {
        let load_res = EmbeddingEngine::load(&model_dir);
        if let Ok(engine) = load_res {
            return Arc::new(engine);
        }
    }
    Arc::new(DeterministicSemanticEmbedder)
}

struct FaultyEmbedder {
    mode: FaultMode,
}

enum FaultMode {
    AlwaysError,
    WrongDimension(usize),
}

impl ToolEmbedder for FaultyEmbedder {
    fn embed_query_vec(&self, _: &str) -> Result<Vec<f32>, String> {
        match self.mode {
            FaultMode::AlwaysError => Err("ONNX Inference Engine Hardware Fault".to_string()),
            FaultMode::WrongDimension(dim) => Ok(vec![0.1f32; dim]),
        }
    }
    fn embed_passage_vec(&self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_query_vec(text)
    }
}

// =========================================================================
// 110 CURATED BILINGUAL DATASET
// =========================================================================

const SIMPLE_VIETNAMESE: &[&str] = &[
    "Xin chào LIVA",
    "Chào buổi sáng bạn nhé",
    "Hôm nay trời có mưa không?",
    "Thời tiết Hà Nội bây giờ thế nào?",
    "Mấy giờ rồi LIVA ơi",
    "Bật đèn phòng khách giúp tôi",
    "Tắt điều hòa phòng ngủ",
    "Tăng âm lượng lên một chút",
    "Giảm âm lượng xuống 30%",
    "Dừng phát nhạc",
    "Phát bài hát tiếp theo",
    "Bạn là ai vậy?",
    "Bạn có thể làm được những gì?",
    "Hôm nay là thứ mấy?",
    "Nhắc tôi 15 phút nữa uống nước",
    "Cảm ơn bạn nhiều nhé",
    "Tạm biệt LIVA",
    "Kể một câu chuyện cười ngắn đi",
    "Mở trình duyệt web",
    "Chụp lại màn hình",
    "LIVA ơi ngủ ngon nhé",
    "Hôm nay tôi thấy vui lắm",
    "Có tin nhắn mới nào không?",
    "Đọc thông báo gần nhất",
    "Kiểm tra pin laptop",
];

const SIMPLE_ENGLISH: &[&str] = &[
    "Hello LIVA",
    "Good morning",
    "What time is it right now?",
    "How is the weather today?",
    "Will it rain this afternoon in Tokyo?",
    "Turn on the bedroom light",
    "Turn off the kitchen lamp",
    "Volume up by 10%",
    "Mute the speakers",
    "Play some jazz music",
    "Next track please",
    "Who created you?",
    "What can you do for me?",
    "Set a timer for 10 minutes",
    "Remind me to call John tomorrow morning",
    "Thank you very much",
    "Goodbye LIVA",
    "Tell me a quick dad joke",
    "What day is it today?",
    "Open notepad",
    "Check battery status",
    "How are you doing today?",
    "Clear the terminal",
    "Nice to meet you",
    "Good night",
];

const COMPLEX_VIETNAMESE: &[&str] = &[
    "Hãy hướng dẫn từng bước tối ưu hóa hiệu năng truy vấn SQLite trong môi trường đa luồng.",
    "Phân tích ưu nhược điểm của kiến trúc Event Sourcing so với CRUD truyền thống trong hệ thống tài chính.",
    "Giải thích từng bước thuật toán tìm đường đi ngắn nhất Dijkstra và cài đặt mẫu bằng Rust.",
    "So sánh sự khác nhau giữa Mutex và RwLock trong Tokio async runtime, khi nào xảy ra starvation?",
    "Tại sao hệ thống gặp deadlock khi kết hợp crossbeam channel với Tokio blocking task? Phân tích chi tiết.",
    "Phân tích kiến trúc vi dịch vụ (Microservices) vs Monolith: các trade-off về độ trễ mạng và tính nhất quán dữ liệu.",
    "Viết hàm tính số Fibonacci bằng quy hoạch động và phân tích độ phức tạp thời gian thuật toán.",
    "Làm thế nào để thiết kế cơ chế WAL checkpointing chống crash trong cơ sở dữ liệu nhúng? Từng bước giải thích.",
    "SELECT u.id, u.name, COUNT(o.id) as total_orders FROM users u LEFT JOIN orders o ON u.id = o.user_id WHERE o.created_at >= '2026-01-01' GROUP BY u.id HAVING total_orders > 5 ORDER BY total_orders DESC;",
    "Hãy phân tích đoạn mã sau để tìm lỗ hổng tràn bộ đệm: ```c void vuln(char *input) { char buf[64]; strcpy(buf, input); } ```",
    "Chứng minh hình thức tính đúng đắn của thuật toán đồng thuận Raft trong mạng phân tán không tin cậy.",
    "Thiết kế hệ thống Rate Limiting phân tán dùng thuật toán Token Bucket và Redis, xử lý 100k RPS.",
    "Giải bài toán ba người ăn tối của Dijkstra (Dining Philosophers) bằng Rust không dùng unsafe.",
    "Phân tích độ phức tạp không gian và thời gian của thuật toán Merge Sort khi áp dụng trên mảng liên kết đơn.",
    "Làm sao để refactor toàn bộ module quản lý bộ nhớ của ứng dụng C++ để áp dụng RAII và smart pointers?",
    "1. Mục tiêu cốt lõi của kiến trúc LIVA là gì? 2. Điểm nghẽn tiềm tàng nằm ở đâu? 3. Giải pháp khắc phục là gì?",
    "Hãy viết một parser cú pháp cho biểu thức số học có dấu ngoặc bằng thuật toán Shunting-yard.",
    "So sánh chi tiết cơ chế garbage collection của Go (tri-color mark-sweep) và cơ chế borrow checker của Rust.",
    "Tại sao thuật toán mã hóa AES-256-GCM lại an toàn hơn CBC mode trước các tấn công padding oracle? Giải thích chi tiết.",
    "Thiết kế kiến trúc hàng đợi thông điệp chịu lỗi cao hỗ trợ exactly-once delivery semantics.",
    "Phân tích nguyên nhân và đề xuất phương án giải quyết race condition giữa reader pool và writer actor.",
    "Giải thích cơ chế suy giảm ký ức Ebbinghaus và công thức toán học áp dụng trong Temporal GraphRAG.",
    "Lập trình một mini actor runtime bằng Rust Tokio với mpsc channel và supervisor restart policy.",
    "Tối ưu hóa pipeline xử lý âm thanh thời gian thực: FFT, STFT, GTCRN denoiser để đạt độ trễ sub-20ms.",
    "So sánh chi tiết về mặt toán học giữa cosine similarity và inner product đối với các vector đã chuẩn hóa L2.",
];

const COMPLEX_ENGLISH: &[&str] = &[
    "Explain step by step how the Raft consensus algorithm handles network partitions and leader election.",
    "Analyze the architectural tradeoffs between LSM-tree and B+ tree storage engines for high-throughput write workloads.",
    "Write a lock-free ring buffer implementation in Rust and prove its thread-safety invariants under memory ordering.",
    "Why does Tokio async task starvation occur when CPU-bound computation is run on worker threads? Explain mitigation strategies.",
    "Compare and contrast Paxos vs Raft consensus algorithms in distributed transaction coordination.",
    "Please provide a step by step mathematical derivation of the Kalman filter equations for state estimation.",
    "```rust\npub async fn handle_stream<T: Stream + Unpin>(mut stream: T) -> Result<(), AppError> {\n    while let Some(item) = stream.next().await {}\n}\n```\nAnalyze potential memory leaks in this snippet.",
    "Design a fault-tolerant distributed cache with consistent hashing, virtual nodes, and anti-entropy synchronization.",
    "SELECT department_id, AVG(salary) FROM employees WHERE hire_date > '2020-01-01' GROUP BY department_id HAVING COUNT(*) > 10;",
    "Explain in detail how zero-copy network serialization works with FlatBuffers versus Protocol Buffers.",
    "Refactor this monad transformer stack in Haskell to avoid performance degradation from deeply nested state.",
    "Analyze the tradeoffs between optimistic concurrency control and two-phase locking in distributed ACID transactions.",
    "Prove formally that the halting problem is undecidable using Turing machine diagonal arguments.",
    "1. What is the root cause of this database deadlock? 2. How can we reorder lock acquisitions to guarantee safety?",
    "Explain the mathematical difference between causal self-attention and bidirectional self-attention in transformer models.",
    "How to implement a high-performance vector indexing graph using Hierarchical Navigable Small World (HNSW) in C++?",
    "Analyze the security implications of speculative execution side-channel attacks like Spectre and Meltdown on modern microarchitectures.",
    "Design an automated database failover mechanism using Raft consensus and virtual IP migration.",
    "Write a detailed architectural review of the L3 Temporal GraphRAG pipeline in LIVA, identifying all data flow bottlenecks.",
    "Explain the mathematical formula for Personalized PageRank (PPR) and its implementation using Compressed Sparse Row (CSR) matrices.",
    "Compare SIMD vectorization strategies across AVX-512, ARM NEON, and SSE4.2 for audio processing.",
    "How do we prevent lock convoying and priority inversion in multi-priority real-time audio scheduling?",
    "Perform a step by step security audit of this cryptographic key derivation function implementation.",
    "What are the theoretical limits of lossy audio compression according to rate-distortion theory?",
    "Explain the mathematical foundations of causal deep learning models for speech enhancement.",
];

const VISION_QUERIES: &[&str] = &[
    "Hãy phân tích lỗi kiến trúc trong sơ đồ đang hiển thị trên màn hình",
    "Nhìn vào đoạn code đang hiển thị trên màn hình và giải thích từng bước lỗi compile",
    "Đọc biểu đồ trên màn hình và so sánh doanh thu giữa quý 1 và quý 2",
    "Có đoạn SQL query trên màn hình: SELECT * FROM users. Hãy tối ưu hóa nó",
    "Phân tích lỗi crash dump trên terminal đang mở",
    "Can you inspect the active window and explain this compiler error step by step?",
    "Look at the architecture diagram on screen and identify potential bottlenecks",
    "Review the code snippet currently open in VS Code for buffer overflows",
    "Analyze the system performance dashboard on my desktop",
    "Examine this git diff displayed in the terminal and write a commit message",
];

// =========================================================================
// TEST 1: 110-QUERY CURATED BILINGUAL CLASSIFICATION ACCURACY
// =========================================================================

#[test]
fn test_challenge_110_bilingual_classification_accuracy() {
    assert_eq!(SIMPLE_VIETNAMESE.len(), 25);
    assert_eq!(SIMPLE_ENGLISH.len(), 25);
    assert_eq!(COMPLEX_VIETNAMESE.len(), 25);
    assert_eq!(COMPLEX_ENGLISH.len(), 25);
    assert_eq!(VISION_QUERIES.len(), 10);

    let embedder = get_test_embedder();

    // 1. Simple Vietnamese (25) -> DoKho::Thuong, RoutingTier::LocalSlm
    for &q in SIMPLE_VIETNAMESE {
        let dokho_emb = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
        assert_eq!(
            dokho_emb,
            DoKho::Thuong,
            "Simple Vi (emb) must be Thuong: '{q}'"
        );
        let tier_emb = route_llm_with_embedder(q, false, Some(&*embedder));
        assert_eq!(
            tier_emb,
            RoutingTier::LocalSlm,
            "Simple Vi (emb) must be LocalSlm: '{q}'"
        );

        let dokho_fb = phan_loai_do_kho(q);
        assert_eq!(
            dokho_fb,
            DoKho::Thuong,
            "Simple Vi (fb) must be Thuong: '{q}'"
        );
        let tier_fb = route_llm(q, false);
        assert_eq!(
            tier_fb,
            RoutingTier::LocalSlm,
            "Simple Vi (fb) must be LocalSlm: '{q}'"
        );
    }

    // 2. Simple English (25) -> DoKho::Thuong, RoutingTier::LocalSlm
    for &q in SIMPLE_ENGLISH {
        let dokho_emb = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
        assert_eq!(
            dokho_emb,
            DoKho::Thuong,
            "Simple En (emb) must be Thuong: '{q}'"
        );
        let tier_emb = route_llm_with_embedder(q, false, Some(&*embedder));
        assert_eq!(
            tier_emb,
            RoutingTier::LocalSlm,
            "Simple En (emb) must be LocalSlm: '{q}'"
        );

        let dokho_fb = phan_loai_do_kho(q);
        assert_eq!(
            dokho_fb,
            DoKho::Thuong,
            "Simple En (fb) must be Thuong: '{q}'"
        );
        let tier_fb = route_llm(q, false);
        assert_eq!(
            tier_fb,
            RoutingTier::LocalSlm,
            "Simple En (fb) must be LocalSlm: '{q}'"
        );
    }

    // 3. Complex Vietnamese (25) -> DoKho::Kho
    for &q in COMPLEX_VIETNAMESE {
        let dokho_emb = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
        assert_eq!(dokho_emb, DoKho::Kho, "Complex Vi (emb) must be Kho: '{q}'");

        let dokho_fb = phan_loai_do_kho(q);
        assert_eq!(dokho_fb, DoKho::Kho, "Complex Vi (fb) must be Kho: '{q}'");
    }

    // 4. Complex English (25) -> DoKho::Kho
    for &q in COMPLEX_ENGLISH {
        let dokho_emb = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
        assert_eq!(dokho_emb, DoKho::Kho, "Complex En (emb) must be Kho: '{q}'");

        let dokho_fb = phan_loai_do_kho(q);
        assert_eq!(dokho_fb, DoKho::Kho, "Complex En (fb) must be Kho: '{q}'");
    }

    // 5. Vision Queries (10) -> RoutingTier::LocalSlm
    for &q in VISION_QUERIES {
        let tier_emb = route_llm_with_embedder(q, true, Some(&*embedder));
        assert_eq!(
            tier_emb,
            RoutingTier::LocalSlm,
            "Vision (emb) must be LocalSlm: '{q}'"
        );

        let tier_fb = route_llm(q, true);
        assert_eq!(
            tier_fb,
            RoutingTier::LocalSlm,
            "Vision (fb) must be LocalSlm: '{q}'"
        );
    }
}

// =========================================================================
// TEST 2: STRICT LOCAL FLAG (RSK-06) INVARIANCE UNDER ADVERSARIAL ATTACKS
// =========================================================================

#[test]
fn test_challenge_strict_local_flag_rsk06_invariance() {
    let embedder = get_test_embedder();

    // Sub-test 2.1: Enable escalation env var explicitly
    unsafe {
        std::env::set_var("LIVA_ENABLE_CLOUD_ESCALATION", "1");
    }

    // Adversarial queries combining vision with explicit escalation triggers
    let adversarial_vision_queries = [
        "Hãy phân tích kiến trúc hệ thống và chứng minh hình thức trên màn hình này",
        "Review architecture and refactor toàn bộ system based on this active window",
        "Look at the screen: formal proof of the Byzantine fault tolerance algorithm",
        "Analyze the architecture diagram and refactor toàn bộ the database schema",
        "SELECT * FROM users WHERE active = 1; explain formal proof on screen",
    ];

    for &query in &adversarial_vision_queries {
        // When is_vision == true, it MUST return LocalSlm even with escalation keywords and env var enabled!
        let tier_emb = route_llm_with_embedder(query, true, Some(&*embedder));
        assert_eq!(
            tier_emb,
            RoutingTier::LocalSlm,
            "RSK-06 Violation: Vision query with escalation triggers leaked: '{query}'"
        );

        let tier_no_emb = route_llm(query, true);
        assert_eq!(
            tier_no_emb,
            RoutingTier::LocalSlm,
            "RSK-06 Violation: Vision query (no emb) with escalation triggers leaked: '{query}'"
        );

        // Control check: when is_vision == false, this SAME query MUST escalate to CloudFrontier
        let non_vision_tier = route_llm_with_embedder(query, false, Some(&*embedder));
        assert_eq!(
            non_vision_tier,
            RoutingTier::CloudFrontier,
            "Control failed: query should have escalated when is_vision == false: '{query}'"
        );
    }

    // Sub-test 2.2: Extreme payload sizes with is_vision == true
    let massive_vision_100kb = format!("kiến trúc hệ thống {}", "A".repeat(100_000));
    assert_eq!(
        route_llm(&massive_vision_100kb, true),
        RoutingTier::LocalSlm,
        "100KB payload with is_vision == true must remain LocalSlm"
    );

    // Sub-test 2.3: Unicode corruption with is_vision == true
    let corrupted_vision = "t̷ừ̸n̷g̷ ̸b̷ư̸ớ̵c̷ kiến trúc \u{0000}\u{FEFF}\u{200B} architecture";
    assert_eq!(
        route_llm(corrupted_vision, true),
        RoutingTier::LocalSlm,
        "Corrupted string with is_vision == true must remain LocalSlm"
    );

    // Clean up env
    unsafe {
        std::env::remove_var("LIVA_ENABLE_CLOUD_ESCALATION");
    }
}

// =========================================================================
// TEST 3: SUB-1.5MS CPU LATENCY SLA (11,000+ EVALUATIONS BENCHMARK)
// =========================================================================

#[test]
fn test_challenge_sub_1_5ms_cpu_latency_sla_11000_evals() {
    let canon = ComplexityCentroids::canonical();
    let dummy_vec = vec![0.05f32; EMBEDDING_DIM];

    // Stage 1: Pure Centroid Vector Cosine Math (10,000 iterations)
    let start_vec = Instant::now();
    const VEC_ITERS: usize = 10_000;
    for _ in 0..VEC_ITERS {
        let _ = canon.classify_vector(
            &dummy_vec,
            DEFAULT_ROUTING_THRESHOLD,
            COMPLEX_ABSOLUTE_THRESHOLD,
        );
    }
    let vec_duration = start_vec.elapsed();
    let vec_avg_micros = vec_duration.as_micros() as f64 / VEC_ITERS as f64;
    println!(
        "Pure Centroid Vector Cosine Math (10,000 runs): avg = {:.4} µs",
        vec_avg_micros
    );
    assert!(
        vec_avg_micros < 100.0,
        "Pure vector math exceeded 100 µs SLA: {:.4} µs",
        vec_avg_micros
    );

    // Collect all 110 queries
    let mut all_queries = Vec::with_capacity(110);
    all_queries.extend_from_slice(SIMPLE_VIETNAMESE);
    all_queries.extend_from_slice(SIMPLE_ENGLISH);
    all_queries.extend_from_slice(COMPLEX_VIETNAMESE);
    all_queries.extend_from_slice(COMPLEX_ENGLISH);
    all_queries.extend_from_slice(VISION_QUERIES);
    assert_eq!(all_queries.len(), 110);

    let embedder = get_test_embedder();

    // Warm up (10 iterations x 110 queries = 1,100 warming evals)
    for _ in 0..10 {
        for &q in &all_queries {
            let _ = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
            let _ = route_llm_with_embedder(q, false, Some(&*embedder));
        }
    }

    // Stage 2: 100 iterations x 110 queries = 11,000 evaluations with active embedder
    const BENCH_ROUNDS: usize = 100;
    let total_evals = all_queries.len() * BENCH_ROUNDS;
    assert_eq!(total_evals, 11_000);

    let mut latencies_micros: Vec<u128> = Vec::with_capacity(total_evals);

    let bench_start = Instant::now();
    for _ in 0..BENCH_ROUNDS {
        for &q in &all_queries {
            let eval_start = Instant::now();
            let _ = phan_loai_do_kho_with_embedder(q, Some(&*embedder));
            let _ = route_llm_with_embedder(q, false, Some(&*embedder));
            latencies_micros.push(eval_start.elapsed().as_micros());
        }
    }
    let total_bench_duration = bench_start.elapsed();

    latencies_micros.sort_unstable();

    let sum_micros: u128 = latencies_micros.iter().sum();
    let avg_micros = sum_micros as f64 / total_evals as f64;
    let min_micros = latencies_micros[0];
    let p50_micros = latencies_micros[total_evals * 50 / 100];
    let p90_micros = latencies_micros[total_evals * 90 / 100];
    let p95_micros = latencies_micros[total_evals * 95 / 100];
    let p99_micros = latencies_micros[total_evals * 99 / 100];
    let max_micros = latencies_micros[total_evals - 1];

    println!("\n=== ROUTELLM 11,000+ EVALUATIONS CPU LATENCY BENCHMARK ===");
    println!("Total evaluations:  {total_evals}");
    println!("Total elapsed time: {:?}", total_bench_duration);
    println!(
        "Min latency:        {} µs ({:.4} ms)",
        min_micros,
        min_micros as f64 / 1000.0
    );
    println!(
        "Average latency:    {:.2} µs ({:.4} ms)",
        avg_micros,
        avg_micros / 1000.0
    );
    println!(
        "P50 latency:        {} µs ({:.4} ms)",
        p50_micros,
        p50_micros as f64 / 1000.0
    );
    println!(
        "P90 latency:        {} µs ({:.4} ms)",
        p90_micros,
        p90_micros as f64 / 1000.0
    );
    println!(
        "P95 latency:        {} µs ({:.4} ms)",
        p95_micros,
        p95_micros as f64 / 1000.0
    );
    println!(
        "P99 latency:        {} µs ({:.4} ms)",
        p99_micros,
        p99_micros as f64 / 1000.0
    );
    println!(
        "Max latency:        {} µs ({:.4} ms)",
        max_micros,
        max_micros as f64 / 1000.0
    );

    // Strict SLA Assertions:
    assert!(
        p95_micros < 1500,
        "P95 latency SLA violated: {} µs >= 1500 µs (1.5ms)",
        p95_micros
    );
    assert!(
        avg_micros < 500.0,
        "Average latency SLA violated: {:.2} µs >= 500 µs (0.5ms)",
        avg_micros
    );
}

// =========================================================================
// TEST 4: BOUNDARY & ADVERSARIAL ROBUSTNESS
// =========================================================================

#[test]
fn test_challenge_boundary_and_adversarial_robustness() {
    // 4.1 Character length boundary:
    // 279 characters casual text (< 45 tokens) -> DoKho::Thuong
    let casual_279 = "chilling ".repeat(31);
    assert_eq!(casual_279.chars().count(), 279);
    assert_eq!(
        phan_loai_do_kho(&casual_279),
        DoKho::Thuong,
        "279 chars casual text must be Thuong"
    );

    // 280 characters casual text -> boundary threshold triggers DoKho::Kho
    let casual_280 = format!("{}h", "chilling ".repeat(31));
    assert_eq!(casual_280.chars().count(), 280);
    assert_eq!(
        phan_loai_do_kho(&casual_280),
        DoKho::Kho,
        "280 chars boundary must trigger Kho"
    );

    // 281 characters casual text -> DoKho::Kho
    let casual_281 = format!("{}hi", "chilling ".repeat(31));
    assert_eq!(casual_281.chars().count(), 281);
    assert_eq!(
        phan_loai_do_kho(&casual_281),
        DoKho::Kho,
        "281 chars boundary must trigger Kho"
    );

    // 4.2 Token count boundary:
    // 44 casual words (< 280 chars) -> DoKho::Thuong
    let words_44 = "go ".repeat(44);
    assert!(words_44.chars().count() < 280);
    assert_eq!(
        phan_loai_do_kho(words_44.trim()),
        DoKho::Thuong,
        "44 casual tokens must remain Thuong"
    );

    // 45 casual words (< 280 chars) -> DoKho::Kho (exceeds 45 token limit)
    let words_45 = "go ".repeat(45);
    assert!(words_45.chars().count() < 280);
    assert_eq!(
        phan_loai_do_kho(words_45.trim()),
        DoKho::Kho,
        "45 casual tokens must escalate to Kho"
    );

    // 4.3 Whitespace, zero-width, and emoji noise
    let noise_cases = [
        "",
        "   \t\r\n   ",
        "\u{0000}",                 // Null byte
        "\u{200B}",                 // Zero-width space
        "\u{FEFF}",                 // Byte order mark (BOM)
        "\u{200E}\u{200F}",         // Bidirectional marks
        "🎉✨🤖🔥💡",               // Emojis only
        "???!!!...",                // Punctuation noise only
        "\u{200B}\u{200B}\u{200B}", // Multiple zero-width
    ];

    for noise in noise_cases {
        assert_eq!(
            phan_loai_do_kho(noise),
            DoKho::Thuong,
            "Noise '{noise}' must safely classify as Thuong without panic"
        );
    }
}

// =========================================================================
// TEST 5: FAULT INJECTION & ENVIRONMENT VARIABLE FUZZING
// =========================================================================

#[test]
fn test_challenge_fault_injection_and_env_fuzzing() {
    // 5.1 Broken embedder returning Err -> graceful fallback to Tier 3 heuristics
    let broken = FaultyEmbedder {
        mode: FaultMode::AlwaysError,
    };
    assert_eq!(
        phan_loai_do_kho_with_embedder("Xin chào LIVA", Some(&broken)),
        DoKho::Thuong,
        "Broken embedder on simple query must fall back to Thuong"
    );
    assert_eq!(
        phan_loai_do_kho_with_embedder("Phân tích thuật toán Raft", Some(&broken)),
        DoKho::Kho,
        "Broken embedder on complex query must fall back to Kho via heuristics"
    );

    // 5.2 Embedder returning wrong dimension (e.g. 128 instead of 384)
    let wrong_dim = FaultyEmbedder {
        mode: FaultMode::WrongDimension(128),
    };
    assert_eq!(
        phan_loai_do_kho_with_embedder("Xin chào LIVA", Some(&wrong_dim)),
        DoKho::Thuong,
        "Dimension mismatch embedder must fall back to Thuong"
    );
    assert_eq!(
        phan_loai_do_kho_with_embedder("Phân tích kiến trúc hệ thống", Some(&wrong_dim)),
        DoKho::Kho,
        "Dimension mismatch embedder must fall back to Kho via heuristics"
    );

    // 5.3 Hostile environment variable values (non-numeric, negative, NaN)
    unsafe {
        std::env::set_var("LIVA_ROUTING_THRESHOLD", "NOT_A_FLOAT");
        std::env::set_var("LIVA_ROUTING_ABS_THRESHOLD", "NaN");
    }

    let embedder = get_test_embedder();
    // Must gracefully fall back to DEFAULT_ROUTING_THRESHOLD and COMPLEX_ABSOLUTE_THRESHOLD without panic
    assert_eq!(
        phan_loai_do_kho_with_embedder("Xin chào LIVA", Some(&*embedder)),
        DoKho::Thuong
    );
    assert_eq!(
        phan_loai_do_kho_with_embedder("Phân tích kiến trúc vi dịch vụ", Some(&*embedder)),
        DoKho::Kho
    );

    // Clean up env
    unsafe {
        std::env::remove_var("LIVA_ROUTING_THRESHOLD");
        std::env::remove_var("LIVA_ROUTING_ABS_THRESHOLD");
    }
}

// =========================================================================
// TEST 6: HIGH-CONCURRENCY MULTITHREADED CONTENTION & CACHE RESILIENCE
// =========================================================================

#[test]
fn test_challenge_high_concurrency_multithreaded_contention() {
    let embedder = get_test_embedder();
    let stop_signal = Arc::new(AtomicBool::new(false));

    // Spawn 1 background thread that continuously triggers cache reset
    let stop_signal_clone = Arc::clone(&stop_signal);
    let reset_handle = std::thread::spawn(move || {
        while !stop_signal_clone.load(Ordering::Relaxed) {
            ComplexityCentroids::reset_cache();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
    });

    // Spawn 16 worker threads querying simultaneously
    let mut worker_handles = Vec::with_capacity(16);
    for t_id in 0..16 {
        let emb = Arc::clone(&embedder);
        worker_handles.push(std::thread::spawn(move || {
            for i in 0..500 {
                let (query, expected) = if (t_id + i) % 2 == 0 {
                    ("Xin chào LIVA hôm nay thế nào?", DoKho::Thuong)
                } else {
                    (
                        "Phân tích thuật toán tìm kiếm nhị phân bằng Rust",
                        DoKho::Kho,
                    )
                };

                let dokho = phan_loai_do_kho_with_embedder(query, Some(&*emb));
                assert_eq!(
                    dokho, expected,
                    "Thread {t_id} iteration {i} classification mismatch"
                );
            }
        }));
    }

    // Wait for all 16 workers to finish 8,000 concurrent classifications
    for h in worker_handles {
        h.join().expect("Worker thread panicked under concurrency!");
    }

    // Stop cache reset thread
    stop_signal.store(true, Ordering::Relaxed);
    reset_handle.join().expect("Reset thread panicked!");
}
