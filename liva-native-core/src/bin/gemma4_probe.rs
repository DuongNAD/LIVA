//! gemma-4-E4B multimodal probe — kiểm cặp `model + mmproj` TRƯỚC khi đổi config.
//!
//! # Vì sao probe trước, đừng đổi config trước
//!
//! Đổi `data/liva-config.json` rồi mới xem có chạy không là cách nhanh nhất để
//! mất một buổi: nếu hỏng, bạn không biết hỏng vì template sai, vì mmproj không
//! khớp, hay vì thứ tự ảnh/text. Probe này tách từng khả năng ra, và quan trọng
//! nhất là **in ra template được nhận diện** — một template sai KHÔNG báo lỗi,
//! nó chỉ làm model trả lời lảm nhảm, nên nó phải được khẳng định tường minh
//! chứ không suy ra từ "câu trả lời trông hợp lý".
//!
//! Đi đúng ĐƯỜNG SẢN XUẤT (giống `qwen3vl_probe`): `swap_model` → nhận diện
//! template → `compile_prompt` → `answer_with_image`. Một harness viết tay sẽ
//! kiểm thứ không ai chạy.
//!
//! Chạy:
//!   cargo run --release --bin gemma4_probe [image.png]   (không tham số → chụp màn hình)
//!
//! Env:
//!   LIVA_GEMMA4_LM      (mặc định E:\AI_Models\gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf)
//!   LIVA_GEMMA4_MMPROJ  (mặc định E:\AI_Models\gemma-4-E4B-it-GGUF\mmproj-F16.gguf)
//!   LIVA_GEMMA4_NGL     (mặc định 0 = CPU; đặt 99 để đẩy hết lên GPU)
//!   LIVA_GEMMA4_NCTX    (mặc định 8192)
//!   LIVA_GEMMA4_SKIP_VISION=1

use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use liva_native_core::llm::engine::VisionImage;
use liva_native_core::llm::{ChatMessage, LlamaRouterManager, compile_prompt, persona, prompt};

fn stream_print(piece: &str) -> bool {
    print!("{piece}");
    let _ = std::io::stdout().flush();
    true
}

fn env_path(ten: &str, mac_dinh: &str) -> PathBuf {
    std::env::var(ten)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(mac_dinh))
}

#[tokio::main]
async fn main() {
    let lm = env_path(
        "LIVA_GEMMA4_LM",
        "E:\\AI_Models\\gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf",
    );
    let mmproj = env_path(
        "LIVA_GEMMA4_MMPROJ",
        "E:\\AI_Models\\gemma-4-E4B-it-GGUF\\mmproj-F16.gguf",
    );
    let ngl: u32 = std::env::var("LIVA_GEMMA4_NGL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let n_ctx: usize = std::env::var("LIVA_GEMMA4_NCTX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8192);
    let skip_vision = std::env::var("LIVA_GEMMA4_SKIP_VISION").as_deref() == Ok("1");

    // Kiểm CẢ HAI file trước khi nạp. Thiếu mmproj mà vẫn chạy được phần text sẽ
    // sinh ra một kết luận nửa vời ("gemma chạy được!") che mất việc vision chưa
    // hề được kiểm — đúng lớp lỗi mà bộ e2e gateway vừa phải sửa.
    for (nhan, p) in [("LM", &lm), ("mmproj", &mmproj)] {
        if !p.exists() {
            eprintln!("Không thấy {nhan} GGUF: {p:?}");
            std::process::exit(1);
        }
    }

    eprintln!("[bước] swap_model (ngl={ngl}, n_ctx={n_ctx}) ...");
    let t0 = Instant::now();
    let mut mgr = LlamaRouterManager::new(n_ctx, ngl).unwrap();
    mgr.swap_model(&lm, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("swap_model");
    mgr.set_mmproj_path(Some(mmproj.clone()));

    // ── (0) TEMPLATE: khẳng định tường minh, không suy ra từ chất lượng câu trả lời ──
    let is_chatml = prompt::CHATML.load(Ordering::Relaxed);
    let gemma4 = prompt::GEMMA4_MARKERS.load(Ordering::Relaxed);
    let ten_tpl = if is_chatml {
        "ChatML (<|im_start|>) — Qwen-style"
    } else if gemma4 {
        "gemma-4 (<|turn>)"
    } else {
        "gemma cổ điển (<start_of_turn>)"
    };
    println!("\n=== (0) TEMPLATE ĐƯỢC NHẬN DIỆN ===");
    println!("  {ten_tpl}");
    if is_chatml {
        println!(
            "  ❌ SAI: model gemma mà nhận diện ra ChatML. `compile_prompt` sẽ phát\n     \
             sai định dạng, và model sẽ trả lời lảm nhảm CHỨ KHÔNG báo lỗi.\n     \
             Kiểm `tokenizer.chat_template` nhúng trong GGUF trước khi đi tiếp."
        );
    } else {
        println!("  ✅ đi nhánh gemma trong compile_prompt");
    }
    eprintln!(
        "[bước] model sẵn sàng sau {:.2}s",
        t0.elapsed().as_secs_f32()
    );

    // In luôn 200 ký tự đầu của prompt đã biên dịch: đây là thứ DUY NHẤT chứng
    // minh marker thật sự được phát ra, thay vì chỉ đọc một cờ boolean.
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: persona::PERSONA_LIVA.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: "Bạn là ai?".to_string(),
        },
    ];
    let mau = compile_prompt(&messages).expect("compile_prompt");
    println!("\n  200 ký tự đầu của prompt thật:");
    println!("  {:?}", mau.chars().take(200).collect::<String>());

    // ── (1) TEXT ────────────────────────────────────────────────────────────
    println!("\n=== (1) TEXT (compile_prompt → generate) ===");
    let text_q = "Bạn là ai, và giúp được gì khi tôi đang bận chơi game?";
    let messages = vec![
        ChatMessage {
            role: "system".to_string(),
            content: persona::PERSONA_LIVA.to_string(),
        },
        ChatMessage {
            role: "user".to_string(),
            content: text_q.to_string(),
        },
    ];
    let prompt_text = compile_prompt(&messages).expect("compile_prompt");
    println!("User: {text_q}\nLIVA: ");
    let t = Instant::now();
    let out = mgr
        .generate_completion(&prompt_text, 0.7, 0.8, stream_print)
        .expect("generate_completion");
    let dt = t.elapsed().as_secs_f32();
    println!(
        "\n[text] {} tok trong {dt:.2}s ({:.1} tok/s)",
        out.completion_tokens,
        out.completion_tokens as f32 / dt.max(0.001)
    );

    if skip_vision {
        println!("\n(bỏ qua vision)");
        return;
    }

    // ── (2) VISION ──────────────────────────────────────────────────────────
    println!("\n=== (2) VISION (answer_with_image → mtmd) ===");
    let vis_q = "Trên màn hình đang hiển thị gì? Mô tả ngắn gọn bằng tiếng Việt.";
    println!("User: {vis_q}\nLIVA: ");
    let t = Instant::now();
    let args: Vec<String> = std::env::args().collect();
    let vout = if let Some(path) = args.get(1) {
        eprintln!("[bước] đọc ảnh từ file: {path}");
        let bytes = std::fs::read(path).expect("đọc file ảnh");
        mgr.answer_with_image(vis_q, VisionImage::Encoded(&bytes), 0.7, 0.8, stream_print)
    } else {
        let (vw, vh, rgb) =
            liva_native_core::vision::capture::capture_for_vision().expect("capture_for_vision");
        eprintln!("[bước] đã chụp vùng {vw}x{vh}");
        mgr.answer_with_image(
            vis_q,
            VisionImage::Rgb {
                width: vw,
                height: vh,
                data: &rgb,
            },
            0.7,
            0.8,
            stream_print,
        )
    };

    match vout {
        Ok(v) => {
            let vdt = t.elapsed().as_secs_f32();
            println!(
                "\n[vision] {} prompt tok, {} gen tok trong {vdt:.2}s ({:.1} tok/s)",
                v.prompt_tokens,
                v.completion_tokens,
                v.completion_tokens as f32 / vdt.max(0.001)
            );
            println!(
                "\n✅ Cặp gemma-4 + mmproj CHẠY ĐƯỢC trên đường sản xuất.\n   \
                 Đọc kỹ câu trả lời trước khi kết luận: đúng template thì nó MÔ TẢ\n   \
                 màn hình; sai mmproj thì nó vẫn trôi chảy nhưng nói về một ảnh\n   \
                 KHÔNG tồn tại. Chỉ 'có chữ' không phải bằng chứng."
            );
        }
        // KHÔNG `expect`: vision hỏng là kết cục cần ĐO, không phải panic. Thông
        // điệp lỗi ở đây chính là dữ liệu quý nhất của cả probe.
        Err(e) => {
            println!("\n❌ VISION HỎNG: {e}");
            println!(
                "   Đây là kết quả hợp lệ của probe, không phải probe hỏng. Ba nghi\n   \
                 can theo thứ tự: mmproj không khớp kiến trúc gemma-4 · build debug\n   \
                 (vision cần release trên Windows) · thứ tự ảnh/text."
            );
            std::process::exit(2);
        }
    }
}
