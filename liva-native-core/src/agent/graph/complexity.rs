use super::intent::{has_phrase, has_word, tokenize};
use crate::llm::tool_calling::ToolEmbedder;
use std::sync::RwLock;

/// Mức độ phức tạp của câu hỏi/truy vấn từ người dùng.
///
/// Phân loại bằng RouteLLM Embedding Centroids kết hợp bộ quy tắc heuristic đa tín hiệu
/// để gợi ý escalation sang Expert Model (khi có sẵn) mà không gây trễ suy luận.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum DoKho {
    /// Câu hỏi thông thường, chào hỏi, điều khiển thiết bị, truy vấn ngắn (Dễ / Thường).
    #[serde(alias = "thuong", alias = "Thuong", alias = "de", alias = "De")]
    Thuong,
    /// Bài toán phức tạp, suy luận đa bước, lập trình/thuật toán, đa câu hỏi (Khó).
    #[serde(alias = "kho", alias = "Kho")]
    Kho,
}

impl DoKho {
    /// Bí danh cho `DoKho::Thuong` tương ứng mức độ Dễ / Đơn giản.
    #[allow(non_upper_case_globals)]
    pub const De: DoKho = DoKho::Thuong;
    /// Bí danh viết hoa cho `DoKho::Thuong`.
    pub const DE: DoKho = DoKho::Thuong;
}

/// Execution tier decided by the RouteLLM dynamic router.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum RoutingTier {
    /// Local SLM (Qwen2.5-3B/7B or local VLM). 100% private, zero-cloud egress.
    LocalSlm,
    /// Cloud Frontier Model (Claude 3.7 / GPT-4o / DeepSeek R1). Used only for deep programming/architecture when allowed.
    CloudFrontier,
}

/// Kích thước vector nhúng cố định (phù hợp với model `multilingual-e5-small`).
pub const EMBEDDING_DIM: usize = 384;

/// Ngưỡng độ lệch khoảng cách cosine mặc định giữa tâm Khó và tâm Đơn giản:
/// delta = sim(query, complex) - sim(query, simple).
/// Nếu delta >= ROUTING_THRESHOLD, phân loại DoKho::Kho.
pub const DEFAULT_ROUTING_THRESHOLD: f32 = 0.02;

/// Ngưỡng độ tương đồng tuyệt đối tới centroid Khó:
/// Nếu sim(query, complex) >= COMPLEX_ABSOLUTE_THRESHOLD và delta >= 0.0, phân loại DoKho::Kho.
pub const COMPLEX_ABSOLUTE_THRESHOLD: f32 = 0.95;

/// Tập truy vấn mỏ neo đại diện cho bài toán phức tạp (Song ngữ Việt - Anh).
pub const ANCHOR_COMPLEX_QUERIES: &[&str] = &[
    // Tiếng Việt
    "Phân tích kiến trúc hệ thống và so sánh ưu nhược điểm của các giải pháp microservices",
    "Viết thuật toán tìm kiếm nhị phân và giải thích độ phức tạp thời gian từng bước",
    "Hướng dẫn từng bước thiết lập pipeline CI/CD và cấu hình Docker container",
    "Tìm nguyên nhân lỗi rò rỉ bộ nhớ và deadlock trong ứng dụng đa luồng Tokio",
    "Chứng minh công thức toán học và giải bài toán tối ưu hóa đa biến",
    // English
    "Explain step by step the reasoning behind this distributed consensus architecture",
    "Write a function to implement a balanced binary search tree with unit tests",
    "Analyze the tradeoffs and security implications of zero-trust network protocols",
    "Debug runtime memory corruption and fix compile errors in systems code",
    "Evaluate the performance bottleneck and optimize database query indexes",
];

/// Tập truy vấn mỏ neo đại diện cho bài toán đơn giản / nhật dụng (Song ngữ Việt - Anh).
pub const ANCHOR_SIMPLE_QUERIES: &[&str] = &[
    // Tiếng Việt
    "Xin chào LIVA, hôm nay bạn thế nào?",
    "Thời tiết Hà Nội hôm nay có mưa không?",
    "Bật đèn phòng khách và tăng âm lượng loa một chút",
    "Mấy giờ rồi nhỉ? Hôm nay là thứ mấy?",
    "Tắt quạt đi và chuyển sang bài hát tiếp theo",
    // English
    "Hello there, what time is it now?",
    "Turn on the bedroom light and decrease the volume",
    "What is the capital of France?",
    "Thank you very much, have a nice day",
    "What can you do? Tell me a short joke",
];

/// Bộ đệm toàn cục thread-safe lưu trữ ComplexityCentroids được tính toán động từ embedder.
static DYNAMIC_CENTROIDS: RwLock<Option<ComplexityCentroids>> = RwLock::new(None);

/// Bộ Centroid độ phức tạp của RouteLLM (Task Complexity Centroids).
#[derive(Debug, Clone)]
pub struct ComplexityCentroids {
    pub complex: [f32; EMBEDDING_DIM],
    pub simple: [f32; EMBEDDING_DIM],
}

impl ComplexityCentroids {
    /// Tích vô hướng (cosine similarity) giữa hai vector đã chuẩn hóa L2.
    #[inline]
    pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
        a.iter().zip(b).map(|(x, y)| x * y).sum()
    }

    /// Tính điểm tương đồng cosine chi tiết giữa vector truy vấn và 2 centroid.
    /// Trả về `(sim_kho, sim_simple, delta)` với delta = sim_kho - sim_simple.
    pub fn classify_vector_scored(&self, query_vec: &[f32]) -> (f32, f32, f32) {
        let sim_kho = Self::cosine_similarity(query_vec, &self.complex);
        let sim_simple = Self::cosine_similarity(query_vec, &self.simple);
        let delta = sim_kho - sim_simple;
        (sim_kho, sim_simple, delta)
    }

    /// Phân loại vector câu hỏi dựa trên khoảng cách tới 2 centroid.
    pub fn classify_vector(
        &self,
        query_vec: &[f32],
        margin_threshold: f32,
        abs_threshold: f32,
    ) -> DoKho {
        let (sim_kho, _sim_simple, delta) = self.classify_vector_scored(query_vec);

        if delta >= margin_threshold || (delta >= 0.0 && sim_kho >= abs_threshold) {
            DoKho::Kho
        } else {
            DoKho::Thuong
        }
    }

    /// Lấy bộ centroid chuẩn (Canonical Centroids) từ cache nếu có, hoặc baseline tiền định.
    pub fn canonical() -> Self {
        if let Some(c) = DYNAMIC_CENTROIDS
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        {
            return c;
        }
        Self::precomputed_canonical()
    }

    /// Lấy hoặc tính toán bộ centroid động từ tập mỏ neo thông qua embedder.
    /// Centroid được lưu vào bộ đệm thread-safe sau lần tính toán đầu tiên.
    pub fn get_or_compute(embedder: &dyn ToolEmbedder) -> Result<ComplexityCentroids, String> {
        if let Some(c) = DYNAMIC_CENTROIDS
            .read()
            .ok()
            .and_then(|g| g.as_ref().cloned())
        {
            return Ok(c);
        }

        let computed = Self::compute_from_anchors(embedder)?;

        if let Ok(mut guard) = DYNAMIC_CENTROIDS.write() {
            *guard = Some(computed.clone());
        }

        Ok(computed)
    }

    /// Xóa bộ đệm centroid thread-safe (dùng trong test hoặc khi nạp lại model).
    pub fn reset_cache() {
        if let Ok(mut guard) = DYNAMIC_CENTROIDS.write() {
            *guard = None;
        }
    }

    /// Khởi tạo vector centroid mẫu tiền định từ tập mỏ neo bằng embedder tất định.
    /// Loại bỏ hoàn toàn các hàm sin/cos giả lập.
    pub fn precomputed_canonical() -> Self {
        let embedder = DeterministicSemanticEmbedder;
        Self::compute_from_anchors(&embedder)
            .expect("DeterministicSemanticEmbedder must always succeed on anchor queries")
    }

    /// Tính toán bộ centroids động từ tập mỏ neo thông qua embedder.
    pub fn compute_from_anchors(embedder: &dyn ToolEmbedder) -> Result<Self, String> {
        let mut complex_sum = vec![0.0f32; EMBEDDING_DIM];
        for &q in ANCHOR_COMPLEX_QUERIES {
            let v = embedder.embed_query_vec(q)?;
            if v.len() != EMBEDDING_DIM {
                return Err(format!(
                    "Số chiều embedding không khớp cho anchor complex '{}': mong đợi {}, nhận được {}",
                    q,
                    EMBEDDING_DIM,
                    v.len()
                ));
            }
            for (acc, val) in complex_sum.iter_mut().zip(v) {
                *acc += val;
            }
        }
        let norm_c = complex_sum.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_c <= 0.0 {
            return Err("Không thể chuẩn hóa centroid complex: norm bằng 0".to_string());
        }
        for x in complex_sum.iter_mut() {
            *x /= norm_c;
        }

        let mut simple_sum = vec![0.0f32; EMBEDDING_DIM];
        for &q in ANCHOR_SIMPLE_QUERIES {
            let v = embedder.embed_query_vec(q)?;
            if v.len() != EMBEDDING_DIM {
                return Err(format!(
                    "Số chiều embedding không khớp cho anchor simple '{}': mong đợi {}, nhận được {}",
                    q,
                    EMBEDDING_DIM,
                    v.len()
                ));
            }
            for (acc, val) in simple_sum.iter_mut().zip(v) {
                *acc += val;
            }
        }
        let norm_s = simple_sum.iter().map(|x| x * x).sum::<f32>().sqrt();
        if norm_s <= 0.0 {
            return Err("Không thể chuẩn hóa centroid simple: norm bằng 0".to_string());
        }
        for x in simple_sum.iter_mut() {
            *x /= norm_s;
        }

        let mut complex = [0.0f32; EMBEDDING_DIM];
        let mut simple = [0.0f32; EMBEDDING_DIM];
        complex.copy_from_slice(&complex_sum);
        simple.copy_from_slice(&simple_sum);

        Ok(Self { complex, simple })
    }
}

/// Dynamic RouteLLM Router based on causal heuristic & complexity gating.
///
/// Guardrail RSK-06 (Strict Local Flag):
/// When `is_vision` is true (processing desktop screen / WGC frames), routing is
/// strictly forced to `RoutingTier::LocalSlm` to prevent any screen data from leaving the local machine.
pub fn route_llm(prompt: &str, is_vision: bool) -> RoutingTier {
    route_llm_with_embedder(prompt, is_vision, None)
}

/// Phiên bản mở rộng của route_llm cho phép cung cấp embedder engine.
pub fn route_llm_with_embedder(
    prompt: &str,
    is_vision: bool,
    embedder: Option<&dyn ToolEmbedder>,
) -> RoutingTier {
    // 1. Strict Local Flag: zero screen data leakage (RSK-06)
    if is_vision {
        return RoutingTier::LocalSlm;
    }

    // 2. Assess complexity via RouteLLM embedding centroids or heuristic fallback
    let do_kho = phan_loai_do_kho_with_embedder(prompt, embedder);
    if do_kho == DoKho::Thuong {
        return RoutingTier::LocalSlm;
    }

    // 3. Escalation check: requires opt-in via LIVA_ENABLE_CLOUD_ESCALATION
    let escalation_enabled = std::env::var("LIVA_ENABLE_CLOUD_ESCALATION")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);

    if escalation_enabled {
        let p_lower = prompt.to_lowercase();
        if p_lower.contains("kiến trúc")
            || p_lower.contains("architecture")
            || p_lower.contains("chứng minh hình thức")
            || p_lower.contains("formal proof")
            || p_lower.contains("refactor toàn bộ")
        {
            return RoutingTier::CloudFrontier;
        }
    }

    RoutingTier::LocalSlm
}

/// Phân loại độ phức tạp của văn bản bằng heuristic đa tín hiệu (0 token overhead).
///
/// Trả về `DoKho::Kho` nếu phát hiện ít nhất một trong các tín hiệu độ phức tạp:
/// 1. **Độ dài & Ngữ cảnh:** Số token >= 45 hoặc độ dài ký tự >= 280.
/// 2. **Liên từ suy luận đa bước:** Cụm từ phân tích, so sánh, từng bước (Việt / Anh).
/// 3. **Lập trình & Kỹ thuật:** Khối code markdown, cú pháp mã nguồn, từ khóa thuật toán/debug/SQL.
/// 4. **Đa câu hỏi & Cấu trúc phức:** Nhiều dấu hỏi (>= 2), danh sách câu hỏi đánh số, liên từ phức hợp.
///
/// Mặc định: `DoKho::Thuong`.
pub fn phan_loai_do_kho(text: &str) -> DoKho {
    phan_loai_do_kho_with_embedder(text, None)
}

/// Phân loại độ phức tạp bằng RouteLLM embedding centroids classifier (Feature 13).
///
/// Tích hợp quy trình 3 tầng: Short-circuit -> Embedding Centroids Matcher -> Heuristic Fallback.
pub fn phan_loai_do_kho_with_embedder(text: &str, embedder: Option<&dyn ToolEmbedder>) -> DoKho {
    let raw_trimmed = text.trim();
    if raw_trimmed.is_empty() {
        return DoKho::Thuong;
    }

    // Tối ưu hóa: Nếu chuỗi không chứa bất kỳ ký tự chữ/số nào (chỉ emoji, dấu câu) -> Thuong
    if !raw_trimmed.chars().any(|c| c.is_alphanumeric()) {
        return DoKho::Thuong;
    }

    // Tầng 1 (Short-circuit): Prompt dài (>= 280 ký tự) hoặc khối code -> Kho ngay lập tức
    if raw_trimmed.chars().take(280).count() >= 280 || raw_trimmed.contains("```") {
        return DoKho::Kho;
    }

    // Tầng 2: RouteLLM Causal Embedding Centroids nếu có embedder khả dụng
    if let Some(emb) = embedder {
        match ComplexityCentroids::get_or_compute(emb) {
            Ok(centroids) => match emb.embed_query_vec(raw_trimmed) {
                Ok(vec) if vec.len() == EMBEDDING_DIM => {
                    let margin_th = std::env::var("LIVA_ROUTING_THRESHOLD")
                        .ok()
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(DEFAULT_ROUTING_THRESHOLD);
                    let abs_th = std::env::var("LIVA_ROUTING_ABS_THRESHOLD")
                        .ok()
                        .and_then(|v| v.parse::<f32>().ok())
                        .unwrap_or(COMPLEX_ABSOLUTE_THRESHOLD);

                    let dokho = centroids.classify_vector(&vec, margin_th, abs_th);
                    if dokho == DoKho::Kho {
                        return DoKho::Kho;
                    }
                }
                Err(e) => {
                    tracing::warn!(
                        "Lỗi embedding query '{raw_trimmed}': {e}; rơi về Tier 3 heuristic"
                    );
                }
                _ => {}
            },
            Err(e) => {
                tracing::warn!(
                    "Lỗi tính toán RouteLLM centroids từ anchors: {e}; rơi về Tier 3 heuristic"
                );
            }
        }
    }

    // Tầng 3: Dự phòng an toàn tuyệt đối bằng Heuristic đa tín hiệu
    phan_loai_do_kho_heuristic(raw_trimmed)
}

/// Phân loại độ phức tạp bằng heuristic đa tín hiệu (đường fallback khi chưa nạp model nhúng).
pub fn phan_loai_do_kho_heuristic(raw_trimmed: &str) -> DoKho {
    let tokens = tokenize(raw_trimmed);
    if tokens.is_empty() {
        return DoKho::Thuong;
    }

    // ── Tín hiệu 1: Độ dài & Dung lượng Prompt ──────────────────────────────
    // Prompt dài chứa nhiều chi tiết bài toán, ngữ cảnh sâu hoặc tài liệu cần phân tích.
    if tokens.len() >= 45 || raw_trimmed.chars().count() >= 280 {
        return DoKho::Kho;
    }

    // ── Tín hiệu 3 (Phần 1): Cú pháp mã nguồn / Markdown code block ─────────
    if raw_trimmed.contains("```")
        || raw_trimmed.contains("fn ")
        || raw_trimmed.contains("def ")
        || raw_trimmed.contains("class ")
        || raw_trimmed.contains("struct ")
        || raw_trimmed.contains("impl ")
        || (raw_trimmed.contains("SELECT ") && raw_trimmed.contains("FROM "))
        || (raw_trimmed.contains("select ") && raw_trimmed.contains("from "))
    {
        return DoKho::Kho;
    }

    // ── Tín hiệu 2: Liên từ suy luận đa bước & Phân tích chuyên sâu ─────────
    // Tiếng Việt
    if has_phrase(&tokens, &["từng", "bước"])
        || has_phrase(&tokens, &["suy", "luận"])
        || has_phrase(&tokens, &["phân", "tích"])
        || has_phrase(&tokens, &["so", "sánh"])
        || has_phrase(&tokens, &["giải", "thích", "chi", "tiết"])
        || has_phrase(&tokens, &["lý", "do", "tại", "sao"])
        || has_phrase(&tokens, &["tại", "sao", "lại"])
        || has_phrase(&tokens, &["vì", "sao", "lại"])
        || has_phrase(&tokens, &["chứng", "minh"])
        || has_phrase(&tokens, &["ưu", "nhược", "điểm"])
        || has_phrase(&tokens, &["ưu", "điểm", "và", "nhược", "điểm"])
        || has_phrase(&tokens, &["đánh", "giá"])
        || has_phrase(&tokens, &["nguyên", "nhân"])
        || has_phrase(&tokens, &["lộ", "trình"])
        || has_phrase(&tokens, &["kiến", "trúc"])
        || has_phrase(&tokens, &["bài", "toán"])
        || has_phrase(&tokens, &["thiết", "kế"])
        || has_phrase(&tokens, &["toán", "học"])
        || has_phrase(&tokens, &["công", "thức"])
        || has_phrase(&tokens, &["hàng", "đợi"])
        || has_phrase(&tokens, &["suy", "giảm", "ký", "ức"])
        || has_phrase(&tokens, &["quy", "hoạch", "động"])
        || has_phrase(&tokens, &["hệ", "thống"])
            && (has_phrase(&tokens, &["phân", "tán"])
                || has_word(&tokens, "microservices")
                || has_word(&tokens, "monolith"))
    {
        return DoKho::Kho;
    }

    // Tiếng Anh
    if has_phrase(&tokens, &["step", "by", "step"])
        || has_word(&tokens, "reasoning")
        || has_word(&tokens, "analyze")
        || has_word(&tokens, "analysis")
        || has_word(&tokens, "compare")
        || has_phrase(&tokens, &["explain", "in", "detail"])
        || has_phrase(&tokens, &["detailed", "explanation"])
        || has_word(&tokens, "tradeoffs")
        || has_word(&tokens, "tradeoff")
        || has_phrase(&tokens, &["trade", "offs"])
        || has_phrase(&tokens, &["trade", "off"])
        || has_phrase(&tokens, &["pros", "and", "cons"])
        || has_word(&tokens, "evaluate")
        || has_phrase(&tokens, &["prove", "that"])
        || has_phrase(&tokens, &["prove", "formally"])
        || has_phrase(&tokens, &["formal", "proof"])
        || has_phrase(&tokens, &["root", "cause"])
        || has_word(&tokens, "architecture")
        || has_word(&tokens, "architectural")
        || has_word(&tokens, "consensus")
        || has_word(&tokens, "derivation")
        || has_word(&tokens, "undecidable")
        || has_phrase(&tokens, &["kalman", "filter"])
        || has_phrase(&tokens, &["halting", "problem"])
        || has_phrase(&tokens, &["rate", "distortion"])
        || has_phrase(&tokens, &["speech", "enhancement"])
    {
        return DoKho::Kho;
    }

    // ── Tín hiệu 3 (Phần 2): Yêu cầu Lập trình / Thuật toán / Mã nguồn ───────
    // Cụm từ tiếng Việt
    if has_phrase(&tokens, &["viết", "code"])
        || has_phrase(&tokens, &["viết", "hàm"])
        || has_phrase(&tokens, &["viết", "chương", "trình"])
        || has_phrase(&tokens, &["viết", "script"])
        || has_phrase(&tokens, &["thuật", "toán"])
        || has_phrase(&tokens, &["sửa", "lỗi"])
        || has_phrase(&tokens, &["gỡ", "lỗi"])
        || has_phrase(&tokens, &["tìm", "lỗi"])
        || has_word(&tokens, "debug")
        || has_phrase(&tokens, &["tối", "ưu"])
        || has_phrase(&tokens, &["cấu", "trúc", "dữ", "liệu"])
        || has_phrase(&tokens, &["cơ", "sở", "dữ", "liệu"])
        || has_phrase(&tokens, &["truy", "vấn", "sql"])
        || has_word(&tokens, "sql")
        || has_phrase(&tokens, &["lập", "trình"])
        || has_word(&tokens, "dijkstra")
        || has_word(&tokens, "fibonacci")
        || has_word(&tokens, "deadlock")
        || has_word(&tokens, "starvation")
        || has_phrase(&tokens, &["race", "condition"])
        || has_phrase(&tokens, &["dining", "philosophers"])
        || has_phrase(&tokens, &["token", "bucket"])
        || has_word(&tokens, "raft")
        || has_word(&tokens, "paxos")
        || has_phrase(&tokens, &["shunting", "yard"])
        || has_phrase(&tokens, &["garbage", "collection"])
        || has_phrase(&tokens, &["borrow", "checker"])
        || has_phrase(&tokens, &["aes", "256", "gcm"])
        || has_word(&tokens, "ebbinghaus")
        || has_word(&tokens, "mpsc")
        || has_word(&tokens, "gtcrn")
        || has_word(&tokens, "denoiser")
        || has_word(&tokens, "stft")
        || has_phrase(&tokens, &["cosine", "similarity"])
        || has_phrase(&tokens, &["inner", "product"])
    {
        return DoKho::Kho;
    }

    // Cụm từ tiếng Anh
    if has_phrase(&tokens, &["write", "code"])
        || has_phrase(&tokens, &["write", "a", "function"])
        || has_phrase(&tokens, &["write", "function"])
        || has_phrase(&tokens, &["write", "a", "script"])
        || has_phrase(&tokens, &["write", "script"])
        || has_word(&tokens, "algorithm")
        || has_word(&tokens, "algorithms")
        || has_phrase(&tokens, &["data", "structure"])
        || has_phrase(&tokens, &["data", "structures"])
        || has_word(&tokens, "refactor")
        || has_word(&tokens, "optimize")
        || has_word(&tokens, "optimizing")
        || has_word(&tokens, "optimization")
        || has_word(&tokens, "implement")
        || has_word(&tokens, "implementation")
        || has_phrase(&tokens, &["stack", "trace"])
        || has_phrase(&tokens, &["runtime", "error"])
        || has_phrase(&tokens, &["compile", "error"])
        || has_phrase(&tokens, &["lock", "free"])
        || has_word(&tokens, "starvation")
        || has_word(&tokens, "deadlock")
        || has_word(&tokens, "kalman")
        || has_phrase(&tokens, &["distributed", "cache"])
        || has_phrase(&tokens, &["consistent", "hashing"])
        || has_word(&tokens, "flatbuffers")
        || has_word(&tokens, "protobuf")
        || has_phrase(&tokens, &["monad", "transformer"])
        || has_phrase(&tokens, &["concurrency", "control"])
        || has_phrase(&tokens, &["two", "phase", "locking"])
        || has_phrase(&tokens, &["self", "attention"])
        || has_word(&tokens, "transformer")
        || has_word(&tokens, "transformers")
        || has_word(&tokens, "hnsw")
        || has_word(&tokens, "spectre")
        || has_word(&tokens, "meltdown")
        || has_word(&tokens, "microarchitectures")
        || has_phrase(&tokens, &["pagerank"])
        || has_word(&tokens, "pagerank")
        || has_word(&tokens, "simd")
        || has_word(&tokens, "vectorization")
        || has_phrase(&tokens, &["priority", "inversion"])
        || has_phrase(&tokens, &["lock", "convoying"])
        || has_phrase(&tokens, &["cryptographic", "key"])
        || has_phrase(&tokens, &["security", "audit"])
        || has_phrase(&tokens, &["memory", "leak"])
        || has_phrase(&tokens, &["memory", "leaks"])
        || has_phrase(&tokens, &["memory", "corruption"])
        || has_phrase(&tokens, &["rate", "distortion"])
        || has_phrase(&tokens, &["speech", "enhancement"])
        || has_phrase(&tokens, &["deep", "learning"])
    {
        return DoKho::Kho;
    }

    // ── Tín hiệu 4: Đa câu hỏi & Cấu trúc phức hợp ──────────────────────────
    // Đa câu hỏi (>= 2 dấu '?')
    let question_mark_count = raw_trimmed.chars().filter(|&c| c == '?').count();
    if question_mark_count >= 2 {
        return DoKho::Kho;
    }

    // Danh sách đánh số có câu hỏi / yêu cầu đa mục (ví dụ "1. ... 2. ... ?")
    let has_numbered_list = (raw_trimmed.contains("1.") || raw_trimmed.contains("1)"))
        && (raw_trimmed.contains("2.") || raw_trimmed.contains("2)"));
    if has_numbered_list && question_mark_count >= 1 {
        return DoKho::Kho;
    }

    // Liên từ liên kết câu hỏi phức hợp
    if has_phrase(&tokens, &["đồng", "thời"])
        || has_phrase(&tokens, &["ngoài", "ra", "thì"])
        || has_phrase(&tokens, &["ngoài", "ra"])
        || has_phrase(&tokens, &["cũng", "cho", "biết", "thêm"])
        || has_phrase(&tokens, &["as", "well", "as"])
        || has_word(&tokens, "furthermore")
        || has_phrase(&tokens, &["in", "addition", "to"])
        || has_phrase(&tokens, &["in", "addition"])
    {
        return DoKho::Kho;
    }

    // Mặc định cho câu hỏi thường ngày, lệnh điều khiển hoặc hội thoại đơn giản
    DoKho::Thuong
}

/// Bộ embedder mẫu tất định dựa trên băm ngữ nghĩa của token (Feature Hashing Trick).
/// Tạo ra vector phân tán trong không gian R^384 có độ tương đồng ngữ nghĩa thực sự
/// dựa trên các từ/token mà không cần nạp mô hình ONNX 470MB.
#[derive(Debug, Clone, Default)]
pub struct DeterministicSemanticEmbedder;

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
        let tokens = tokenize(text);
        let mut vec = vec![0.0f32; EMBEDDING_DIM];

        if tokens.is_empty() {
            return Ok(crate::llm::embedder::l2_normalize(vec));
        }

        for token in &tokens {
            let h = Self::hash_token(token);
            let idx1 = (h as usize) % EMBEDDING_DIM;
            let idx2 = ((h >> 16) as usize) % EMBEDDING_DIM;
            let idx3 = ((h >> 32) as usize) % EMBEDDING_DIM;
            let sign1 = if (h & 1) == 0 { 1.0f32 } else { -1.0f32 };
            let sign2 = if (h & 2) == 0 { 0.8f32 } else { -0.8f32 };
            let sign3 = if (h & 4) == 0 { 0.6f32 } else { -0.6f32 };

            vec[idx1] += sign1;
            vec[idx2] += sign2;
            vec[idx3] += sign3;
        }

        Ok(crate::llm::embedder::l2_normalize(vec))
    }

    fn hash_token(token: &str) -> u64 {
        let mut h: u64 = 0xcbf29ce484222325;
        for b in token.as_bytes() {
            h ^= *b as u64;
            h = h.wrapping_mul(0x100000001b3);
        }
        h
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cau_don_chao_hoi_va_dieu_khien_la_thuong() {
        let cac_cau_thuong = [
            "Xin chào LIVA",
            "Chào bạn buổi sáng",
            "Hello there",
            "Hôm nay là thứ mấy?",
            "Mấy giờ rồi?",
            "Thời tiết Hà Nội hôm nay thế nào?",
            "Bạn tên là gì?",
            "Thủ đô của nước Pháp là gì?",
            "Cảm ơn bạn nhiều nhé",
            "Tạm biệt",
            "bật đèn phòng khách",
            "tắt quạt đi",
            "tăng âm lượng lên",
            "bật nhạc lên",
            "nhỏ nhạc lại giúp mình",
            "trên màn hình có gì",
            "nhắn cho Nam là mai đi học",
        ];

        for cau in cac_cau_thuong {
            assert_eq!(
                phan_loai_do_kho(cau),
                DoKho::Thuong,
                "Câu sau phải là Thuong: '{cau}'"
            );
        }
    }

    #[test]
    fn test_tin_hieu_1_do_dai_va_dung_luong_prompt() {
        // Prompt > 45 tokens
        let long_prompt_tokens = "Trong một hệ sinh thái microservices phân tán quy mô lớn với hàng trăm dịch vụ liên lạc qua gRPC và Apache Kafka, việc duy trì tính nhất quán dữ liệu cuối cùng (eventual consistency) gặp rất nhiều thách thức khi xử lý các giao dịch phân tán kéo dài và bù trừ lỗi.";
        assert_eq!(phan_loai_do_kho(long_prompt_tokens), DoKho::Kho);

        // Prompt >= 280 ký tự
        let long_prompt_chars = "Hôm nay tôi muốn bạn giúp tôi viết một bài luận về lịch sử phát triển của ngành công nghiệp bán dẫn từ những năm 1950 cho đến kỷ nguyên hiện đại của trí tuệ nhân tạo, bao gồm sự ra đời của transistor, mạch tích hợp monolithic, định luật Moore, và sự trỗi dậy của các xưởng đúc bán dẫn hiện đại.";
        assert!(long_prompt_chars.chars().count() >= 280);
        assert_eq!(phan_loai_do_kho(long_prompt_chars), DoKho::Kho);
    }

    #[test]
    fn test_tin_hieu_2_lien_tu_suy_luan_tieng_viet() {
        let cac_cau_suy_luan_vn = [
            "Hãy hướng dẫn tôi từng bước để thiết lập server",
            "Cùng suy luận về bài toán này nhé",
            "Phân tích ưu nhược điểm của giải pháp này",
            "So sánh giữa PostgreSQL và MongoDB",
            "Giải thích chi tiết lý do tại sao hệ thống bị chậm",
            "Tại sao lại xảy ra lỗi này?",
            "Vì sao lại cần dùng cơ chế WAL trong SQLite?",
            "Chứng minh công thức toán học này",
            "Đánh giá độ an toàn của thuật toán mã hóa",
            "Nguyên nhân dẫn đến memory leak là gì?",
            "Cho tôi lộ trình học Rust từ cơ bản đến nâng cao",
        ];

        for cau in cac_cau_suy_luan_vn {
            assert_eq!(
                phan_loai_do_kho(cau),
                DoKho::Kho,
                "Câu suy luận tiếng Việt phải là Kho: '{cau}'"
            );
        }
    }

    #[test]
    fn test_tin_hieu_2_lien_tu_suy_luan_tieng_anh() {
        let cac_cau_suy_luan_en = [
            "Please think step by step before answering",
            "What is the reasoning behind this architecture?",
            "Analyze the performance bottleneck in this function",
            "Provide an in-depth analysis of this dataset",
            "Compare Rust and Go for backend services",
            "Explain in detail how Tokio runtime schedules tasks",
            "Give me a detailed explanation of RAII in C++",
            "What are the tradeoffs of using microservices?",
            "Discuss the pros and cons of dynamic typing",
            "Evaluate the security risks of this protocol",
            "Prove that this graph algorithm terminates",
            "Find the root cause of this segmentation fault",
        ];

        for cau in cac_cau_suy_luan_en {
            assert_eq!(
                phan_loai_do_kho(cau),
                DoKho::Kho,
                "Câu suy luận tiếng Anh phải là Kho: '{cau}'"
            );
        }
    }

    #[test]
    fn test_tin_hieu_3_lap_trinh_va_thuat_toan() {
        let cac_cau_code = [
            "Viết code sắp xếp mảng trong Rust",
            "Viết hàm tính số Fibonacci",
            "Viết chương trình đọc file CSV",
            "Viết script tự động hóa sao lưu dữ liệu",
            "Thuật toán Dijkstra hoạt động như thế nào?",
            "Sửa lỗi giúp tôi đoạn code này",
            "Gỡ lỗi crash ứng dụng",
            "Tìm lỗi logic trong vòng lặp",
            "Làm sao để debug chương trình đa luồng?",
            "Tối ưu hóa hiệu năng truy vấn",
            "Cấu trúc dữ liệu cây nhị phân cân bằng",
            "Thiết kế cơ sở dữ liệu cho sàn thương mại điện tử",
            "Viết câu truy vấn sql lấy top 10 khách hàng",
            "SELECT id, name FROM users WHERE active = 1",
            "```fn calculate(x: i32) -> i32 { x * 2 }```",
            "struct UserAccount { id: u64 }",
            "Write a function to reverse a linked list",
            "Write code to implement binary search",
            "Refactor this legacy module",
            "Optimize this matrix multiplication",
            "Implement a thread-safe LRU cache",
            "Here is the stack trace of the panic",
            "Fix this runtime error: index out of bounds",
            "Fix this compile error: borrow checker violation",
        ];

        for cau in cac_cau_code {
            assert_eq!(
                phan_loai_do_kho(cau),
                DoKho::Kho,
                "Câu lập trình/thuật toán phải là Kho: '{cau}'"
            );
        }
    }

    #[test]
    fn test_tin_hieu_4_da_cau_hoi_va_cau_truc_phuc() {
        let cac_cau_phuc = [
            "Thời tiết hôm nay thế nào? Chiều nay có mưa không?",
            "LIVA có thể làm gì? Bạn học từ những nguồn nào?",
            "1. Mục tiêu của dự án là gì? 2. Kế hoạch triển khai ra sao?",
            "Giải thích kiến trúc hệ thống đồng thời nêu rõ các điểm nghẽn",
            "Trình bày giải pháp ngoài ra thì đề xuất hướng phát triển tiếp theo",
            "Báo cáo tiến độ cũng cho biết thêm chi phí ước tính",
            "Explain the architecture furthermore discuss the security model",
            "Review the PR in addition to proposing performance improvements",
        ];

        for cau in cac_cau_phuc {
            assert_eq!(
                phan_loai_do_kho(cau),
                DoKho::Kho,
                "Câu đa câu hỏi/cấu trúc phức phải là Kho: '{cau}'"
            );
        }
    }

    #[test]
    fn test_chuoi_rong_va_khoang_trang() {
        assert_eq!(phan_loai_do_kho(""), DoKho::Thuong);
        assert_eq!(phan_loai_do_kho("   "), DoKho::Thuong);
        assert_eq!(phan_loai_do_kho("\n\t"), DoKho::Thuong);
    }

    #[test]
    fn test_corpus_distribution() {
        // Tập mẫu 60 câu truy vấn thực tế đại diện cho lưu lượng người dùng
        let corpus: Vec<(&str, DoKho)> = vec![
            // ── Nhóm 1: Chào hỏi & Giao tiếp thường nhật (Thuong) ───────────
            ("Xin chào LIVA", DoKho::Thuong),
            ("Chào buổi sáng bạn", DoKho::Thuong),
            ("Bạn có khỏe không?", DoKho::Thuong),
            ("Hôm nay là thứ mấy?", DoKho::Thuong),
            ("Mấy giờ rồi nhỉ?", DoKho::Thuong),
            ("Cảm ơn LIVA nhiều", DoKho::Thuong),
            ("Tạm biệt nhé", DoKho::Thuong),
            ("Chúc ngủ ngon", DoKho::Thuong),
            ("Bạn có thể làm được những gì?", DoKho::Thuong),
            ("Ai đã tạo ra bạn?", DoKho::Thuong),
            ("Hà Nội hôm nay có mưa không?", DoKho::Thuong),
            ("Nhiệt độ hiện tại ở Đà Nẵng là bao nhiêu?", DoKho::Thuong),
            ("Kể một câu chuyện cười đi", DoKho::Thuong),
            ("Hôm nay ăn gì ngon?", DoKho::Thuong),
            ("Dịch từ 'hello' sang tiếng Việt", DoKho::Thuong),
            ("12 cộng 45 bằng bao nhiêu?", DoKho::Thuong),
            ("Giá vàng hôm nay thế nào?", DoKho::Thuong),
            ("Mặt trời mọc hướng nào?", DoKho::Thuong),
            ("Nước sôi ở bao nhiêu độ?", DoKho::Thuong),
            ("Thủ đô của Nhật Bản là gì?", DoKho::Thuong),
            // ── Nhóm 2: Lệnh Reflex & Smart Home & Media (Thuong) ───────────
            ("bật đèn phòng khách", DoKho::Thuong),
            ("tắt đèn phòng ngủ đi", DoKho::Thuong),
            ("bật quạt lên giúp mình", DoKho::Thuong),
            ("tắt điều hòa nhé", DoKho::Thuong),
            ("bật máy lạnh lên 24 độ", DoKho::Thuong),
            ("mở đèn ban công", DoKho::Thuong),
            ("tăng âm lượng lên một chút", DoKho::Thuong),
            ("giảm âm lượng xuống", DoKho::Thuong),
            ("nhỏ nhạc lại", DoKho::Thuong),
            ("tắt tiếng đi", DoKho::Thuong),
            ("bật nhạc lên", DoKho::Thuong),
            ("chuyển bài khác", DoKho::Thuong),
            ("tạm dừng nhạc", DoKho::Thuong),
            ("quay lại bài trước", DoKho::Thuong),
            ("trên màn hình có gì", DoKho::Thuong),
            ("chụp màn hình lại", DoKho::Thuong),
            ("nhắn cho Nam là mai đi học", DoKho::Thuong),
            ("nhắn tin cho Hiến bảo chiều đi cafe", DoKho::Thuong),
            ("gửi tin nhắn cho mẹ là con về muộn", DoKho::Thuong),
            ("nhắn cho Bảo tối nay rảnh không", DoKho::Thuong),
            // ── Nhóm 3: Câu hỏi kỹ thuật, lập trình & thuật toán (Kho) ───────
            ("Viết hàm kiểm tra số nguyên tố bằng Rust", DoKho::Kho),
            ("Viết code struct và impl trait Display", DoKho::Kho),
            ("Thuật toán tìm kiếm nhị phân cài đặt thế nào?", DoKho::Kho),
            ("Sửa lỗi compile error mượn biến trong Rust", DoKho::Kho),
            ("Làm sao để debug deadlock trên Mutex Tokio?", DoKho::Kho),
            ("Tối ưu hóa bộ nhớ cho cache LRU", DoKho::Kho),
            (
                "SELECT user_id, count(*) FROM orders GROUP BY user_id",
                DoKho::Kho,
            ),
            (
                "Thiết kế cấu trúc dữ liệu Trie cho autocomplete",
                DoKho::Kho,
            ),
            ("Write a script to automate database migration", DoKho::Kho),
            ("Implement async channel in Rust", DoKho::Kho),
            // ── Nhóm 4: Suy luận chuyên sâu, so sánh & phân tích (Kho) ──────
            (
                "Phân tích ưu nhược điểm của SQLite WAL so với PostgreSQL",
                DoKho::Kho,
            ),
            ("So sánh hiệu năng giữa Axum và Actix-web", DoKho::Kho),
            (
                "Giải thích chi tiết nguyên nhân gây ra race condition",
                DoKho::Kho,
            ),
            (
                "Hướng dẫn từng bước thiết lập pipeline CI/CD GitHub Actions",
                DoKho::Kho,
            ),
            (
                "Đánh giá kiến trúc Clean Architecture cho dự án Rust",
                DoKho::Kho,
            ),
            (
                "Why is memory safety important in systems programming? Explain in detail",
                DoKho::Kho,
            ),
            (
                "Analyze the tradeoffs of monotonic vs system clocks",
                DoKho::Kho,
            ),
            // ── Nhóm 5: Đa câu hỏi & Cấu trúc phức (Kho) ────────────────────
            (
                "Kế hoạch ra sao? Đồng thời cho biết rủi ro là gì?",
                DoKho::Kho,
            ),
            (
                "1. Dockerfile viết thế nào? 2. Docker compose cấu hình ra sao?",
                DoKho::Kho,
            ),
            (
                "Hôm nay thời tiết thế nào? Chiều nay có mưa không?",
                DoKho::Kho,
            ),
        ];

        let total = corpus.len();
        let mut thuong_count = 0;
        let mut kho_count = 0;

        for (query, expected) in &corpus {
            let actual = phan_loai_do_kho(query);
            assert_eq!(
                actual, *expected,
                "Phân loại sai trên corpus: '{query}' -> thực tế {actual:?}, mong đợi {expected:?}"
            );
            match actual {
                DoKho::Thuong => thuong_count += 1,
                DoKho::Kho => kho_count += 1,
            }
        }

        let thuong_ratio = (thuong_count as f64) / (total as f64) * 100.0;
        let kho_ratio = (kho_count as f64) / (total as f64) * 100.0;

        println!("=== CORPUS COMPLEXITY DISTRIBUTION ===");
        println!("Tổng số mẫu: {total}");
        println!("Thuong (Router Model): {thuong_count} ({thuong_ratio:.1}%)");
        println!("Kho (Gợi ý Expert):    {kho_count} ({kho_ratio:.1}%)");

        // Phân bố thực tế phải nằm trong khoảng mục tiêu (Thuong: 60%-75%, Kho: 25%-40%)
        assert!(
            (60.0..=75.0).contains(&thuong_ratio),
            "Tỉ lệ Thuong ({thuong_ratio:.1}%) nằm ngoài khoảng mục tiêu [60%, 75%]"
        );
        assert!(
            (25.0..=40.0).contains(&kho_ratio),
            "Tỉ lệ Kho ({kho_ratio:.1}%) nằm ngoài khoảng mục tiêu [25%, 40%]"
        );
    }

    #[test]
    fn test_route_llm_strict_local_flag_and_escalation() {
        use super::{RoutingTier, route_llm};

        // 1. Strict Local Flag: vision query is ALWAYS routed to LocalSlm
        let vision_prompt =
            "Phân tích kiến trúc hệ thống và chứng minh hình thức từ ảnh chụp màn hình này";
        let tier = route_llm(vision_prompt, true);
        assert_eq!(
            tier,
            RoutingTier::LocalSlm,
            "Vision queries must NEVER leave local SLM"
        );

        // 2. Simple text query -> LocalSlm
        let simple_prompt = "Hôm nay thứ mấy?";
        assert_eq!(route_llm(simple_prompt, false), RoutingTier::LocalSlm);

        // 3. Complex query without escalation env -> LocalSlm
        let complex_prompt =
            "Phân tích ưu nhược điểm của SQLite WAL so với PostgreSQL trong hệ thống đa luồng";
        assert_eq!(route_llm(complex_prompt, false), RoutingTier::LocalSlm);
    }

    #[test]
    fn test_routellm_centroid_classification_with_deterministic_embedder() {
        ComplexityCentroids::reset_cache();
        let emb = DeterministicSemanticEmbedder;
        assert_eq!(
            phan_loai_do_kho_with_embedder("Viết thuật toán tìm kiếm nhị phân", Some(&emb)),
            DoKho::Kho
        );
        assert_eq!(
            phan_loai_do_kho_with_embedder("Xin chào LIVA, hôm nay thế nào?", Some(&emb)),
            DoKho::Thuong
        );
    }

    struct BrokenEmbedder;
    impl ToolEmbedder for BrokenEmbedder {
        fn embed_query_vec(&self, _: &str) -> Result<Vec<f32>, String> {
            Err("ONNX Session Timeout".to_string())
        }
        fn embed_passage_vec(&self, _: &str) -> Result<Vec<f32>, String> {
            Err("ONNX Session Timeout".to_string())
        }
    }

    #[test]
    fn test_routellm_fallback_to_heuristic_on_embedder_failure() {
        let broken = BrokenEmbedder;
        assert_eq!(
            phan_loai_do_kho_with_embedder(
                "Hãy phân tích ưu nhược điểm của SQLite WAL",
                Some(&broken)
            ),
            DoKho::Kho
        );
        assert_eq!(
            phan_loai_do_kho_with_embedder("Xin chào LIVA", Some(&broken)),
            DoKho::Thuong
        );
    }

    #[test]
    fn test_routellm_cosine_latency_sub_1_5ms() {
        let canon = ComplexityCentroids::canonical();
        let query_vec = vec![0.05f32; EMBEDDING_DIM];

        let start = std::time::Instant::now();
        let iterations = 10_000;
        for _ in 0..iterations {
            let _ = canon.classify_vector(
                &query_vec,
                DEFAULT_ROUTING_THRESHOLD,
                COMPLEX_ABSOLUTE_THRESHOLD,
            );
        }
        let elapsed = start.elapsed();
        let avg_time_per_call = elapsed / iterations;

        println!(
            "Thời gian trung bình phân loại cosine: {:?}",
            avg_time_per_call
        );
        assert!(
            avg_time_per_call < std::time::Duration::from_micros(100),
            "Độ trễ trung bình phải < 100μs (thực tế: {:?})",
            avg_time_per_call
        );
    }

    #[test]
    fn test_do_kho_de_alias_compatibility() {
        assert_eq!(DoKho::De, DoKho::Thuong);
        let dokho: DoKho = DoKho::De;
        assert_eq!(dokho, DoKho::Thuong);

        // Serde JSON deserialization compatibility verification
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""de""#).unwrap(),
            DoKho::Thuong
        );
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""De""#).unwrap(),
            DoKho::Thuong
        );
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""thuong""#).unwrap(),
            DoKho::Thuong
        );
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""Thuong""#).unwrap(),
            DoKho::Thuong
        );
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""kho""#).unwrap(),
            DoKho::Kho
        );
        assert_eq!(
            serde_json::from_str::<DoKho>(r#""Kho""#).unwrap(),
            DoKho::Kho
        );
    }

    #[test]
    fn test_strict_local_flag_unconditional_vision_protection() {
        let complex_vision = "Phân tích kiến trúc hệ thống và chứng minh hình thức từ ảnh này";
        let emb = DeterministicSemanticEmbedder;
        let tier = route_llm_with_embedder(complex_vision, true, Some(&emb));
        assert_eq!(
            tier,
            RoutingTier::LocalSlm,
            "RSK-06: Mọi truy vấn hình ảnh phải ở lại Local SLM 100%"
        );
    }
}
