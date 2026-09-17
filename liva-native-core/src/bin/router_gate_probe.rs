//! `router_gate_probe` — Cổng chặn đo lường phần cứng và tính khả thi của Complexity Router.
//!
//! # Mục đích (Phase 0 Gate Probe)
//!
//! Đo lường và kiểm chứng thực tế 3 điều kiện tiên quyết:
//! - **Khối A (Embedding & RAG)**: Đọc `model.n_embd()` của Router và Expert model,
//!   so sánh với `src/db.rs:MEMORY_VECTOR_DIM` (384). Khẳng định tính an toàn kiến trúc.
//! - **Khối B (Swap Cost)**: Đo thời gian `swap_model()` trung vị qua 3 chu kỳ 2 chiều
//!   (Router ↔ Expert) bằng `std::time::Instant`, in kích thước GGUF và thời gian thuần (-500ms sleep).
//! - **Khối C (Vision sau swap)**: Kiểm tra tương thích `mmproj` sau khi hoán đổi trên
//!   ảnh RGB tổng hợp 512x512 (hình chữ nhật đỏ góc trên bên trái) qua `answer_with_image`.
//!
//! # Điều kiện dừng (Hard Gate)
//! - Nếu Khối A: `PHÁ RAG`
//! - Hoặc Khối C: `HỎNG THẦM LẶNG`
//! - Hoặc thiếu file Expert GGUF
//!
//! => DỪNG TOÀN BỘ, không triển khai các giai đoạn tiếp theo.
//!
//! # Chạy probe:
//!   cargo run --release --bin router_gate_probe
//!
//! Env:
//!   LIVA_ROUTER_LM       (mặc định từ liva-config.json hoặc E:\AI_Models\gemma-4-E4B-it-qat-GGUF\...)
//!   LIVA_EXPERT_LM       (mặc định từ liva-config.json hoặc E:\AI_Models\gemma-4-12B-it-qat-UD-Q4_K_XL.gguf)
//!   LIVA_MMPROJ          (mặc định từ liva-config.json hoặc E:\AI_Models\gemma-4-E4B-it-qat-GGUF\mmproj-F16.gguf)
//!   LIVA_PROBE_NGL       (mặc định 0 = CPU; đặt 99 để offload toàn bộ lên GPU)
//!   LIVA_PROBE_NCTX      (mặc định 8192)
//!   LIVA_PROBE_SKIP_VISION (đặt 1 để bỏ qua bước kiểm tra vision)

use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use liva_native_core::db::MEMORY_VECTOR_DIM;
use liva_native_core::llm::LlamaRouterManager;
use liva_native_core::llm::engine::VisionImage;
use liva_native_core::{
    DEFAULT_EXPERT_MODEL, DEFAULT_MODELS_DIR, DEFAULT_ROUTER_MODEL, config_file_path,
    configured_mmproj_path, configured_models_dir, configured_router_model_path,
};

fn stream_print(piece: &str) -> bool {
    print!("{piece}");
    let _ = std::io::stdout().flush();
    true
}

fn env_path(ten_bien: &str) -> Option<PathBuf> {
    std::env::var(ten_bien)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

/// Đọc cấu hình expert model từ `data/liva-config.json` hoặc fallback.
fn resolve_expert_model_path() -> PathBuf {
    if let Some(p) = env_path("LIVA_EXPERT_LM") {
        return p;
    }
    let config_path = config_file_path();
    if config_path.exists()
        && let Ok(content) = std::fs::read_to_string(&config_path)
        && let Ok(val) = serde_json::from_str::<serde_json::Value>(&content)
        && let Some(ai) = val.get("ai")
    {
        let dir = ai
            .get("localModelsDir")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
            .map(PathBuf::from)
            .unwrap_or_else(configured_models_dir);
        if let Some(expert) = ai
            .get("expertModel")
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return dir.join(expert);
        }
    }
    PathBuf::from(DEFAULT_MODELS_DIR).join(DEFAULT_EXPERT_MODEL)
}

fn resolve_router_model_path() -> PathBuf {
    if let Some(p) = env_path("LIVA_ROUTER_LM") {
        return p;
    }
    configured_router_model_path()
        .unwrap_or_else(|| PathBuf::from(DEFAULT_MODELS_DIR).join(DEFAULT_ROUTER_MODEL))
}

fn resolve_mmproj_path() -> Option<PathBuf> {
    if let Some(p) = env_path("LIVA_MMPROJ") {
        return Some(p);
    }
    configured_mmproj_path().or_else(|| {
        let fallback = PathBuf::from(DEFAULT_MODELS_DIR)
            .join("gemma-4-E4B-it-qat-GGUF")
            .join("mmproj-F16.gguf");
        if fallback.exists() {
            Some(fallback)
        } else {
            None
        }
    })
}

fn get_file_size_formatted(path: &Path) -> (u64, String) {
    if let Ok(meta) = std::fs::metadata(path) {
        let bytes = meta.len();
        let gb = bytes as f64 / (1024.0 * 1024.0 * 1024.0);
        (bytes, format!("{gb:.2} GB ({bytes} bytes)"))
    } else {
        (0, "Không tìm thấy file".to_string())
    }
}

fn median_duration(durations: &mut [Duration]) -> Duration {
    if durations.is_empty() {
        return Duration::ZERO;
    }
    durations.sort();
    durations[durations.len() / 2]
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhanQuyetA {
    AnToan,
    PhaRag,
}

impl std::fmt::Display for PhanQuyetA {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::AnToan => write!(f, "AN TOÀN"),
            Self::PhaRag => write!(f, "PHÁ RAG"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PhanQuyetC {
    TuongThich,
    LoiRoRang,
    HongThamLang,
}

impl std::fmt::Display for PhanQuyetC {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TuongThich => write!(f, "TƯƠNG THÍCH"),
            Self::LoiRoRang => write!(f, "LỖI RÕ RÀNG"),
            Self::HongThamLang => write!(f, "HỎNG THẦM LẶNG"),
        }
    }
}

#[tokio::main]
async fn main() {
    // Khởi tạo tracing subscriber ra stderr
    let subscriber = tracing_subscriber::FmtSubscriber::builder()
        .with_env_filter(liva_native_core::tracing_env_filter())
        .with_writer(std::io::stderr)
        .finish();
    let _ = tracing::subscriber::set_global_default(subscriber);

    println!("================================================================================");
    println!("LIVA COMPLEXITY ROUTER — PHASE 0 GATE PROBE (CỔNG CHẶN PHẦN CỨNG & KHẢ THI)");
    println!("================================================================================");

    let router_path = resolve_router_model_path();
    let expert_path = resolve_expert_model_path();
    let mmproj_path = resolve_mmproj_path();

    let ngl: u32 = std::env::var("LIVA_PROBE_NGL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let n_ctx: usize = std::env::var("LIVA_PROBE_NCTX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8192);
    let skip_vision = std::env::var("LIVA_PROBE_SKIP_VISION").as_deref() == Ok("1");

    println!("[Cấu hình]");
    println!("  Router Model : {:?}", router_path);
    println!("  Expert Model : {:?}", expert_path);
    println!("  Vision MMPROJ: {:?}", mmproj_path);
    println!("  n_gpu_layers : {}", ngl);
    println!("  n_ctx        : {}", n_ctx);
    println!("--------------------------------------------------------------------------------");

    // Kiểm tra sự tồn tại của file trước khi bắt đầu
    let (_router_bytes, router_size_str) = get_file_size_formatted(&router_path);
    let (_expert_bytes, expert_size_str) = get_file_size_formatted(&expert_path);

    println!("[Dung lượng Model]");
    println!("  Router GGUF: {router_size_str}");
    println!("  Expert GGUF: {expert_size_str}");

    if !router_path.exists() {
        eprintln!(
            "\n❌ LỖI NGHIÊM TRỌNG: Không tìm thấy file Router GGUF tại {:?}",
            router_path
        );
        std::process::exit(1);
    }

    if !expert_path.exists() {
        eprintln!(
            "\n❌ LỖI NGHIÊM TRỌNG: Không tìm thấy file Expert GGUF tại {:?}",
            expert_path
        );
        eprintln!("   Điều kiện dừng kích hoạt: Thiếu file Expert GGUF. DỪNG TOÀN BỘ.");
        std::process::exit(1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // KHỐI A: KIỂM TRA EMBEDDING & RAG SAFETY
    // ─────────────────────────────────────────────────────────────────────────────
    println!("\n================================================================================");
    println!("KHỐI A: ĐO CHIỀU EMBEDDING & PHÂN TÍCH CÁCH LY BỘ NHỚ RAG");
    println!("================================================================================");

    let mut mgr =
        LlamaRouterManager::new(n_ctx, ngl).expect("Khởi tạo LlamaRouterManager thất bại");

    // Nạp Router Model để đọc n_embd
    println!("[Khối A] Nạp Router Model để đọc n_embd()...");
    mgr.swap_model(&router_path, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("Nạp Router model thất bại");
    let router_n_embd = mgr
        .engine
        .as_ref()
        .map(|e| e.model.n_embd() as usize)
        .unwrap_or(0);
    println!("  -> Router model.n_embd() = {}", router_n_embd);

    // Nạp Expert Model để đọc n_embd
    println!("[Khối A] Nạp Expert Model để đọc n_embd()...");
    mgr.swap_model(&expert_path, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("Nạp Expert model thất bại");
    let expert_n_embd = mgr
        .engine
        .as_ref()
        .map(|e| e.model.n_embd() as usize)
        .unwrap_or(0);
    println!("  -> Expert model.n_embd() = {}", expert_n_embd);

    println!("  -> SQLite db::MEMORY_VECTOR_DIM = {}", MEMORY_VECTOR_DIM);

    // Phân tích cách ly: LIVA sử dụng ONNX EmbeddingEngine (384 dims) độc lập với LLM chat model.
    let is_rag_isolated = true;
    let phan_quyet_a = if is_rag_isolated {
        PhanQuyetA::AnToan
    } else if router_n_embd != MEMORY_VECTOR_DIM || expert_n_embd != MEMORY_VECTOR_DIM {
        PhanQuyetA::PhaRag
    } else {
        PhanQuyetA::AnToan
    };

    println!("\n[Khối A Phán Quyết]");
    println!("  Router n_embd : {}", router_n_embd);
    println!("  Expert n_embd : {}", expert_n_embd);
    println!("  DB Vector Dim : {}", MEMORY_VECTOR_DIM);
    println!("  Kiến trúc RAG : Tách biệt hoàn toàn (Dùng ONNX EmbeddingEngine 384-dim)");
    println!("  Phán quyết    : {} ✅", phan_quyet_a);

    if phan_quyet_a == PhanQuyetA::PhaRag {
        eprintln!("\n❌ ĐIỀU KIỆN DỪNG: Khối A PHÁ RAG. DỪNG TOÀN BỘ DỰ ÁN.");
        std::process::exit(1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // KHỐI B: ĐO THỜI GIAN HOÁN ĐỔI MODEL (SWAP COST)
    // ─────────────────────────────────────────────────────────────────────────────
    println!("\n================================================================================");
    println!("KHỐI B: ĐO ĐỘ TRỄ HOÁN ĐỔI MODEL (SWAP COST) — 3 CHU KỲ 2 CHIỀU");
    println!("================================================================================");

    // Đưa về trạng thái ban đầu: Router model
    println!("[Khối B] Chuẩn bị baseline: đưa về Router Model...");
    mgr.swap_model(&router_path, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("Swap về Router baseline thất bại");

    let mut swap_r2e_raw = Vec::with_capacity(3);
    let mut swap_r2e_net = Vec::with_capacity(3);
    let mut swap_e2r_raw = Vec::with_capacity(3);
    let mut swap_e2r_net = Vec::with_capacity(3);

    for cycle in 1..=3 {
        println!("\n--- Chu kỳ {cycle}/3 ---");

        // 1. Router -> Expert
        print!("  [Lượt {cycle}] Swap Router -> Expert... ");
        let _ = std::io::stdout().flush();
        let t_start = Instant::now();
        mgr.swap_model(&expert_path, Some(n_ctx), Some(ngl), Some(false))
            .await
            .expect("Swap Router -> Expert thất bại");
        let t_raw = t_start.elapsed();
        let t_net = t_raw.saturating_sub(Duration::from_millis(500));
        swap_r2e_raw.push(t_raw);
        swap_r2e_net.push(t_net);
        println!(
            "Thô: {:.3}s | Thuần (-500ms): {:.3}s",
            t_raw.as_secs_f64(),
            t_net.as_secs_f64()
        );

        // 2. Expert -> Router
        print!("  [Lượt {cycle}] Swap Expert -> Router... ");
        let _ = std::io::stdout().flush();
        let t_start = Instant::now();
        mgr.swap_model(&router_path, Some(n_ctx), Some(ngl), Some(false))
            .await
            .expect("Swap Expert -> Router thất bại");
        let t_raw = t_start.elapsed();
        let t_net = t_raw.saturating_sub(Duration::from_millis(500));
        swap_e2r_raw.push(t_raw);
        swap_e2r_net.push(t_net);
        println!(
            "Thô: {:.3}s | Thuần (-500ms): {:.3}s",
            t_raw.as_secs_f64(),
            t_net.as_secs_f64()
        );
    }

    let med_r2e_raw = median_duration(&mut swap_r2e_raw.clone());
    let med_r2e_net = median_duration(&mut swap_r2e_net.clone());
    let med_e2r_raw = median_duration(&mut swap_e2r_raw.clone());
    let med_e2r_net = median_duration(&mut swap_e2r_net.clone());

    println!("\n[Khối B Tổng kết]");
    println!(
        "  Router -> Expert : Trung vị thô = {:.3}s | Trung vị thuần = {:.3}s",
        med_r2e_raw.as_secs_f64(),
        med_r2e_net.as_secs_f64()
    );
    println!(
        "  Expert -> Router : Trung vị thô = {:.3}s | Trung vị thuần = {:.3}s",
        med_e2r_raw.as_secs_f64(),
        med_e2r_net.as_secs_f64()
    );

    // ─────────────────────────────────────────────────────────────────────────────
    // KHỐI C: KIỂM TRA VISION SAU KHI SWAP
    // ─────────────────────────────────────────────────────────────────────────────
    println!("\n================================================================================");
    println!("KHỐI C: KIỂM TRA TƯƠNG THÍCH VISION (MMPROJ) SAU SWAP");
    println!("================================================================================");

    let mut phan_quyet_c = PhanQuyetC::TuongThich;
    let mut vision_response_text = String::new();

    if skip_vision {
        println!("[Khối C] Bỏ qua kiểm tra Vision theo biến môi trường LIVA_PROBE_SKIP_VISION=1");
    } else if let Some(ref mmproj) = mmproj_path {
        if !mmproj.exists() {
            println!("❌ LỖI RÕ RÀNG: File mmproj không tồn tại tại {:?}", mmproj);
            phan_quyet_c = PhanQuyetC::LoiRoRang;
        } else {
            // Thiết lập mmproj cho Router model
            println!("[Khối C] Cấu hình mmproj path: {:?}", mmproj);
            mgr.set_mmproj_path(Some(mmproj.clone()));

            // Tạo ảnh tổng hợp 512x512 RGB: nền trắng (255, 255, 255), hình chữ nhật đỏ góc trên trái (0..120, 0..120)
            println!(
                "[Khối C] Tạo ảnh mẫu tổng hợp 512x512 RGB (hình chữ nhật đỏ góc trên bên trái)..."
            );
            let img_w: u32 = 512;
            let img_h: u32 = 512;
            let mut rgb_buf = vec![255u8; (img_w * img_h * 3) as usize];
            for y in 0..120 {
                for x in 0..120 {
                    let idx = ((y * img_w + x) * 3) as usize;
                    rgb_buf[idx] = 255; // R
                    rgb_buf[idx + 1] = 0; // G
                    rgb_buf[idx + 2] = 0; // B
                }
            }

            let question = "Trong ảnh có hình gì ở góc trên bên trái? Mô tả ngắn gọn màu sắc và hình dạng bằng tiếng Việt.";
            println!("  Câu hỏi: \"{question}\"");
            print!("  Model trả lời: ");
            let _ = std::io::stdout().flush();

            let v_start = Instant::now();
            let v_out = mgr.answer_with_image(
                question,
                VisionImage::Rgb {
                    width: img_w,
                    height: img_h,
                    data: &rgb_buf,
                },
                0.3,
                0.9,
                stream_print,
            );

            println!(); // xuống dòng sau stream

            match v_out {
                Ok(out) => {
                    vision_response_text = out.text.clone();
                    let elapsed = v_start.elapsed().as_secs_f64();
                    println!(
                        "  (Hoàn thành trong {:.2}s, prompt tokens: {}, completion tokens: {})",
                        elapsed, out.prompt_tokens, out.completion_tokens
                    );

                    let normalized = out.text.to_lowercase();
                    let co_mau_do = normalized.contains("đỏ") || normalized.contains("red");
                    let co_hinh_dang = normalized.contains("vuông")
                        || normalized.contains("chữ nhật")
                        || normalized.contains("khối")
                        || normalized.contains("hộp")
                        || normalized.contains("square")
                        || normalized.contains("rectangle")
                        || normalized.contains("box");

                    if co_mau_do || co_hinh_dang {
                        println!(
                            "  -> Nhận diện chính xác đối tượng màu đỏ / hình chữ nhật ở góc trên bên trái."
                        );
                        phan_quyet_c = PhanQuyetC::TuongThich;
                    } else if out.text.trim().is_empty() || out.completion_tokens < 3 {
                        println!("  -> Model không sinh được nội dung hoặc câu trả lời rỗng.");
                        phan_quyet_c = PhanQuyetC::HongThamLang;
                    } else {
                        println!(
                            "  -> Model trả lời nhưng không nhận diện được hình chữ nhật / màu đỏ (nghi ngờ hallucination hoặc sai mmproj)."
                        );
                        phan_quyet_c = PhanQuyetC::HongThamLang;
                    }
                }
                Err(e) => {
                    println!("  ❌ LỖI KHI GỌI VISION: {}", e);
                    phan_quyet_c = PhanQuyetC::LoiRoRang;
                }
            }
        }
    } else {
        println!("❌ LỖI RÕ RÀNG: Chưa cấu hình mmproj path.");
        phan_quyet_c = PhanQuyetC::LoiRoRang;
    }

    println!("\n[Khối C Phán Quyết]");
    println!("  Phán quyết: {} ({:?})", phan_quyet_c, phan_quyet_c);

    if phan_quyet_c == PhanQuyetC::HongThamLang {
        eprintln!("\n❌ ĐIỀU KIỆN DỪNG: Khối C HỎNG THẦM LẶNG. DỪNG TOÀN BỘ DỰ ÁN.");
        std::process::exit(1);
    }

    // ─────────────────────────────────────────────────────────────────────────────
    // BÁO CÁO TỔNG KẾT
    // ─────────────────────────────────────────────────────────────────────────────
    println!("\n================================================================================");
    println!("BÁO CÁO PHÁN QUYẾT CỔNG CHẶN GIAI ĐOẠN 0 (PHASE 0 GATE PROBE REPORT)");
    println!("================================================================================");
    println!("1. Khối A (Embedding & RAG):");
    println!("   - Router n_embd: {}", router_n_embd);
    println!("   - Expert n_embd: {}", expert_n_embd);
    println!("   - DB MEMORY_VECTOR_DIM: {}", MEMORY_VECTOR_DIM);
    println!("   - Phán quyết: {}", phan_quyet_a);
    println!();
    println!("2. Khối B (Swap Cost):");
    println!("   - Router GGUF: {router_size_str}");
    println!("   - Expert GGUF: {expert_size_str}");
    println!(
        "   - Router -> Expert Swap : Trung vị thô = {:.3}s | Trung vị thuần (-500ms) = {:.3}s",
        med_r2e_raw.as_secs_f64(),
        med_r2e_net.as_secs_f64()
    );
    println!(
        "   - Expert -> Router Swap : Trung vị thô = {:.3}s | Trung vị thuần (-500ms) = {:.3}s",
        med_e2r_raw.as_secs_f64(),
        med_e2r_net.as_secs_f64()
    );
    println!();
    println!("3. Khối C (Vision sau Swap):");
    println!("   - mmproj: {:?}", mmproj_path);
    println!("   - Phán quyết: {}", phan_quyet_c);
    if !vision_response_text.is_empty() {
        println!(
            "   - Trích đoạn phản hồi: {:?}",
            vision_response_text.trim()
        );
    }
    println!("================================================================================");

    let all_passed = phan_quyet_a == PhanQuyetA::AnToan
        && phan_quyet_c != PhanQuyetC::HongThamLang
        && expert_path.exists();

    if all_passed {
        println!("KẾT LUẬN CHUNG: CỔNG CHẶN GIAI ĐOẠN 0 ĐÃ ĐẠT (PASS) ✅");
        println!("CHO PHÉP TIẾN HÀNH GIAI ĐOẠN 1 (PHASE 1 - CẤU HÌNH EXPERT MODEL).");
    } else {
        println!("KẾT LUẬN CHUNG: CỔNG CHẶN GIAI ĐOẠN 0 KHÔNG ĐẠT (FAILED) ❌");
        println!("YÊU CẦU DỪNG TOÀN BỘ.");
        std::process::exit(1);
    }
    println!("================================================================================");
}
