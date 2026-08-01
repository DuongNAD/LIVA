//! `ttft_bench` — đo **Time To First Token** của đường sinh chữ, bằng một lệnh.
//!
//! ```powershell
//! cargo build --release --bin ttft_bench          # thêm --features cuda nếu có GPU
//! .\target\release\ttft_bench.exe                 # 10 lượt + 1 lượt làm nóng
//! .\target\release\ttft_bench.exe 20              # đổi số lượt
//! ```
//!
//! # Vì sao có file này
//!
//! `README` từng tự nhận dự án **chưa có benchmark TTFT** (mục U9 của
//! `docs/03-danh-gia/05-nang-cap-toan-dien.md`). Với hồ sơ dự thi, một con số
//! đo được và tái lập được mạnh hơn mọi tính từ. Điều kiện nghiệm thu của U9
//! ghi rõ: chạy bằng **một** lệnh, in kèm **cấu hình máy đo**, và không được
//! viết số nào chưa chạy — nên chương trình này in cả cấu hình lẫn số, cạnh
//! nhau, trong cùng một lần chạy.
//!
//! # Ba bẫy đo, và cách file này né từng cái
//!
//! **1. Cache tiền tố sẽ biến lượt 2..N thành phép đo cache, không phải TTFT.**
//! [`LlamaRouterManager::generate_completion`] so prompt mới với `last_tokens`
//! và **bỏ qua phần prefill đã trùng**. Gửi đúng một prompt 10 lần thì lượt đầu
//! đo prefill thật, chín lượt sau đo gần như số 0 — rồi p50 tụt xuống một con
//! số đẹp và sai. Ở đây mỗi lượt được gắn một **nhãn khác nhau ngay đầu
//! prompt**, nên tiền tố chung bằng 0 và mọi lượt đều prefill đủ. Muốn tự kiểm:
//! bỏ nhãn đi, số sẽ tụt hẳn một bậc — đó là dấu hiệu bạn đang đo cache.
//!
//! **2. "Token đầu tiên" có HAI nghĩa, và trên model suy luận chúng cách nhau
//! rất xa.** Engine lọc các kênh suy luận nội bộ (`<think>`, `<analysis>`…)
//! trước khi trả ra; trong lúc lọc, callback vẫn được gọi với **chuỗi rỗng**
//! làm nhịp tim để bên gọi huỷ được giữa chừng. Nên:
//!
//! - `TTFT thô` = lần callback **đầu tiên bất kỳ** — model đã sinh ra token đầu.
//! - `TTFT nhìn thấy` = mảnh **không rỗng** đầu tiên — lúc chữ thật sự hiện ra,
//!   và cũng là lúc TTS có thể bắt đầu nói.
//!
//! Con số người dùng cảm nhận được là cái thứ hai. In một số duy nhất ở đây là
//! tự chọn một trong hai định nghĩa rồi giấu chuyện đã chọn.
//!
//! **3. Lượt đầu tiên luôn đắt hơn** (dựng đồ thị tính toán, chạm trang mmap
//! lần đầu). Một lượt **làm nóng** chạy trước và **không** vào thống kê.
//!
//! # Đọc p95 cho đúng
//!
//! Với `n` nhỏ, p95 **chính là giá trị lớn nhất** — nó không phải một ước lượng
//! đuôi phân phối. Chương trình in thẳng cảnh báo đó khi `n < 20` thay vì để
//! người đọc tự suy ra.

use liva_native_core::llm::engine::LlamaRouterManager;
use liva_native_core::llm::prompt::{ChatMessage, compile_prompt};
use std::time::{Duration, Instant};

/// Số token nhìn thấy được sinh thêm sau token đầu, chỉ để có một con số
/// thông lượng đi kèm. Giữ nhỏ: mục tiêu của file này là TTFT, không phải
/// sinh trọn câu trả lời.
const TOKENS_SAU_TOKEN_DAU: usize = 32;

const SYSTEM_PROMPT: &str = "Bạn là LIVA, một trợ lý cá nhân chạy hoàn toàn ngoại tuyến. \
Trả lời ngắn gọn, chính xác, bằng tiếng Việt.";

const CAU_HOI: &str = "Giải thích ngắn gọn vì sao chạy mô hình ngôn ngữ ngay trên máy \
người dùng lại riêng tư hơn gọi API đám mây.";

fn phan_vi(da_sap_xep: &[Duration], p: f64) -> Duration {
    if da_sap_xep.is_empty() {
        return Duration::ZERO;
    }
    // Kiểu "nearest-rank": chỉ số nhỏ nhất mà >= p phần trăm mẫu nằm dưới nó.
    let hang = ((p / 100.0) * da_sap_xep.len() as f64).ceil().max(1.0) as usize;
    da_sap_xep[(hang - 1).min(da_sap_xep.len() - 1)]
}

fn ms(d: Duration) -> String {
    format!("{:.0} ms", d.as_secs_f64() * 1000.0)
}

fn in_cau_hinh_may(duong_dan_model: &std::path::Path, n_ctx: usize, n_gpu_layers: u32) {
    println!("╭─ Cấu hình máy đo ───────────────────────────────────────────");

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let backend = if cfg!(feature = "cuda") {
        "CUDA"
    } else if cfg!(feature = "vulkan") {
        "Vulkan"
    } else {
        "CPU (không có feature GPU nào được bật lúc build)"
    };
    println!("│ Build            : {profile} · backend {backend}");
    println!("│ n_ctx            : {n_ctx}   (LIVA_LLM_N_CTX, mặc định 4096 như boot.rs)");
    println!(
        "│ n_gpu_layers     : {n_gpu_layers}   (LIVA_LLM_N_GPU_LAYERS, mặc định 0 — \
         0 nghĩa là CHẠY CPU dù build có CUDA)"
    );

    println!("│ Lõi CPU luận lý  : {}", num_cpus_luan_ly());

    match liva_native_core::sysinfo::ram_bytes() {
        Some((tong, con_trong)) => println!(
            "│ RAM              : {:.1} GiB tổng · {:.1} GiB còn trống",
            tong as f64 / 1024.0 / 1024.0 / 1024.0,
            con_trong as f64 / 1024.0 / 1024.0 / 1024.0
        ),
        None => println!("│ RAM              : -- (không đọc được)"),
    }

    // `gpu_vram_bytes` trả (TỔNG, ĐANG DÙNG) — thứ tự này từng bị đảo một lần
    // và in ra "đang dùng nhiều hơn tổng"; xem bẫy 1 của U3.
    match liva_native_core::governor::gpu_vram_bytes() {
        Some((tong, dang_dung)) => println!(
            "│ VRAM             : {} MiB tổng · {} MiB đang dùng",
            tong / 1024 / 1024,
            dang_dung / 1024 / 1024
        ),
        None => println!("│ VRAM             : -- (không có NVML / không phải GPU NVIDIA)"),
    }

    let co_lon = std::fs::metadata(duong_dan_model)
        .map(|m| format!("{:.2} GiB", m.len() as f64 / 1024.0 / 1024.0 / 1024.0))
        .unwrap_or_else(|_| "?".into());
    println!("│ Model            : {}", duong_dan_model.display());
    println!("│ Kích thước       : {co_lon}");
    println!("╰─────────────────────────────────────────────────────────────");
    println!();
}

fn num_cpus_luan_ly() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(0)
}

/// Một lượt đo. Trả `(ttft_thô, ttft_nhìn_thấy, số_token_nhìn_thấy, thời_gian_sinh_thêm)`.
///
/// `nhan` đi vào **đầu** prompt để phá cache tiền tố — xem bẫy 1 ở đầu file.
fn mot_luot(
    llm: &mut LlamaRouterManager,
    nhan: usize,
) -> Result<(Duration, Option<Duration>, usize, Duration), String> {
    let messages = vec![
        ChatMessage {
            role: "system".into(),
            content: format!("[phiên #{nhan}] {SYSTEM_PROMPT}"),
        },
        ChatMessage {
            role: "user".into(),
            content: CAU_HOI.into(),
        },
    ];
    let prompt = compile_prompt(&messages)?;

    let bat_dau = Instant::now();
    let mut ttft_tho: Option<Duration> = None;
    let mut ttft_nhin_thay: Option<Duration> = None;
    let mut luc_co_token_dau: Option<Instant> = None;
    let mut so_token_nhin_thay = 0usize;

    llm.generate_completion(&prompt, 0.7, 0.9, |manh| {
        if ttft_tho.is_none() {
            ttft_tho = Some(bat_dau.elapsed());
        }
        if manh.is_empty() {
            // Nhịp tim trong lúc engine đang giấu khối suy luận. Không tính là
            // "chữ đã hiện ra", nhưng vẫn là bằng chứng model đang chạy.
            return true;
        }
        if ttft_nhin_thay.is_none() {
            ttft_nhin_thay = Some(bat_dau.elapsed());
            luc_co_token_dau = Some(Instant::now());
        }
        so_token_nhin_thay += 1;
        so_token_nhin_thay <= TOKENS_SAU_TOKEN_DAU
    })?;

    let thoi_gian_sinh_them = luc_co_token_dau
        .map(|t| t.elapsed())
        .unwrap_or(Duration::ZERO);

    Ok((
        ttft_tho.ok_or("Model không sinh ra token nào")?,
        ttft_nhin_thay,
        so_token_nhin_thay,
        thoi_gian_sinh_them,
    ))
}

#[tokio::main]
async fn main() -> Result<(), String> {
    let so_luot: usize = std::env::args()
        .nth(1)
        .and_then(|v| v.parse().ok())
        .unwrap_or(10);

    let duong_dan = liva_native_core::configured_router_model_path().ok_or(
        "Không có model router trong cấu hình. Đặt `ai.localModelsDir` + `ai.routerModel` \
         trong `data/liva-config.json`, và chạy lệnh này từ thư mục gốc repo.",
    )?;
    if !duong_dan.is_file() {
        return Err(format!(
            "Không thấy file model: {}\nChạy `npm run doctor` để biết thiếu gì.",
            duong_dan.display()
        ));
    }

    // Đọc y hệt cách `boot.rs` đọc, để số đo nói về cấu hình THẬT của app chứ
    // không phải một cấu hình riêng của benchmark.
    let n_ctx = std::env::var("LIVA_LLM_N_CTX")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(4096);
    let n_gpu_layers = std::env::var("LIVA_LLM_N_GPU_LAYERS")
        .ok()
        .and_then(|v| v.parse::<u32>().ok())
        .unwrap_or(0);

    in_cau_hinh_may(&duong_dan, n_ctx, n_gpu_layers);

    if cfg!(feature = "cuda") && n_gpu_layers == 0 {
        println!(
            "⚠  Build có CUDA nhưng LIVA_LLM_N_GPU_LAYERS = 0 ⇒ GPU ĐỨNG KHÔNG, \
             số dưới đây là số CPU.\n   Đặt LIVA_LLM_N_GPU_LAYERS=99 rồi đo lại.\n"
        );
    }

    print!("Đang nạp model… ");
    use std::io::Write;
    let _ = std::io::stdout().flush();
    let nap_tu = Instant::now();
    let mut llm = LlamaRouterManager::new(n_ctx, n_gpu_layers)?;
    llm.swap_model(&duong_dan, Some(n_ctx), Some(n_gpu_layers), Some(false))
        .await?;
    println!("xong sau {}.\n", ms(nap_tu.elapsed()));

    // Lượt làm nóng — KHÔNG vào thống kê. Bẫy 3 ở đầu file.
    print!("Lượt làm nóng (bỏ khỏi thống kê)… ");
    let _ = std::io::stdout().flush();
    let (tho_nong, _, _, _) = mot_luot(&mut llm, 0)?;
    println!("TTFT thô {}.\n", ms(tho_nong));

    let mut cac_tho = Vec::with_capacity(so_luot);
    let mut cac_nhin_thay = Vec::with_capacity(so_luot);
    let mut tong_token = 0usize;
    let mut tong_thoi_gian_sinh = Duration::ZERO;

    for i in 1..=so_luot {
        let (tho, nhin_thay, so_token, thoi_gian_sinh) = mot_luot(&mut llm, i)?;
        cac_tho.push(tho);
        if let Some(v) = nhin_thay {
            cac_nhin_thay.push(v);
        }
        tong_token += so_token;
        tong_thoi_gian_sinh += thoi_gian_sinh;
        println!(
            "  lượt {i:>2}/{so_luot}  TTFT thô {:>9}  ·  TTFT nhìn thấy {:>9}",
            ms(tho),
            nhin_thay.map(ms).unwrap_or_else(|| "không có".into())
        );
    }
    println!();

    cac_tho.sort();
    cac_nhin_thay.sort();

    println!("╭─ Kết quả · {so_luot} lượt ───────────────────────────────────");
    println!(
        "│ TTFT thô          p50 {:>9}   p95 {:>9}   min {:>9}   max {:>9}",
        ms(phan_vi(&cac_tho, 50.0)),
        ms(phan_vi(&cac_tho, 95.0)),
        ms(cac_tho[0]),
        ms(cac_tho[cac_tho.len() - 1])
    );
    if cac_nhin_thay.is_empty() {
        println!("│ TTFT nhìn thấy    — không lượt nào ra chữ (model chỉ sinh khối bị lọc?)");
    } else {
        println!(
            "│ TTFT nhìn thấy    p50 {:>9}   p95 {:>9}   min {:>9}   max {:>9}",
            ms(phan_vi(&cac_nhin_thay, 50.0)),
            ms(phan_vi(&cac_nhin_thay, 95.0)),
            ms(cac_nhin_thay[0]),
            ms(cac_nhin_thay[cac_nhin_thay.len() - 1])
        );
    }
    if tong_thoi_gian_sinh > Duration::ZERO && tong_token > so_luot {
        // Trừ token đầu ra khỏi tử số: đồng hồ thông lượng bắt đầu TỪ token đầu.
        let token_sau = (tong_token - cac_nhin_thay.len()) as f64;
        println!(
            "│ Thông lượng sau   {:.1} token/s   (trên {} token, tối đa {} mỗi lượt)",
            token_sau / tong_thoi_gian_sinh.as_secs_f64(),
            token_sau as usize,
            TOKENS_SAU_TOKEN_DAU
        );
    }
    println!("╰─────────────────────────────────────────────────────────────");

    if so_luot < 20 {
        println!(
            "\n⚠  n = {so_luot} < 20 ⇒ p95 ở đây CHÍNH LÀ giá trị lớn nhất, không phải \
             ước lượng đuôi phân phối.\n   Cần một con số p95 nói được điều gì đó thì chạy \
             `ttft_bench 50` trở lên."
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn phan_vi_tren_mau_rong_khong_panic() {
        assert_eq!(phan_vi(&[], 50.0), Duration::ZERO);
        assert_eq!(phan_vi(&[], 95.0), Duration::ZERO);
    }

    #[test]
    fn phan_vi_nearest_rank_dung_moc() {
        let mau: Vec<Duration> = (1..=10).map(|i| Duration::from_millis(i * 10)).collect();
        // p50 của 10 mẫu = phần tử thứ 5 (nearest-rank, 1-based) = 50 ms.
        assert_eq!(phan_vi(&mau, 50.0), Duration::from_millis(50));
        // p95 của 10 mẫu = phần tử thứ 10 = giá trị lớn nhất. Đây chính là lý do
        // chương trình cảnh báo khi n < 20.
        assert_eq!(phan_vi(&mau, 95.0), Duration::from_millis(100));
        assert_eq!(phan_vi(&mau, 100.0), Duration::from_millis(100));
    }

    #[test]
    fn phan_vi_mot_mau_luon_tra_chinh_no() {
        let mau = [Duration::from_millis(7)];
        assert_eq!(phan_vi(&mau, 50.0), Duration::from_millis(7));
        assert_eq!(phan_vi(&mau, 95.0), Duration::from_millis(7));
    }

    /// Nhãn phiên phải nằm ở **đầu** prompt, nếu không nó không phá được cache
    /// tiền tố và cả benchmark đo nhầm thứ khác (bẫy 1 ở đầu file).
    #[test]
    fn nhan_phien_nam_dau_prompt_va_khac_nhau_giua_cac_luot() {
        let dung = |nhan: usize| {
            compile_prompt(&[
                ChatMessage {
                    role: "system".into(),
                    content: format!("[phiên #{nhan}] {SYSTEM_PROMPT}"),
                },
                ChatMessage {
                    role: "user".into(),
                    content: CAU_HOI.into(),
                },
            ])
            .expect("compile_prompt phải dựng được prompt hai lượt")
        };

        let a = dung(1);
        let b = dung(2);
        assert_ne!(a, b, "hai lượt phải ra prompt khác nhau");

        let chung = a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count();
        assert!(
            chung < 40,
            "tiền tố chung {chung} byte là quá dài — nhãn phiên đã trôi khỏi đầu prompt, \
             cache tiền tố sẽ được dùng lại và benchmark đo nhầm"
        );
    }
}
