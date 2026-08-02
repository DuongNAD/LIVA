//! So chất lượng hai model trên CÙNG một bộ câu hỏi, CÙNG đường sản xuất.
//!
//! # Ba quyết định làm nên giá trị của phép so này
//!
//! **1. Greedy (temperature 0), không phải mặc định sản xuất.** Ở nhiệt độ > 0,
//! một lượt chạy là một mẫu ngẫu nhiên; hai model có thể trông hơn kém nhau
//! thuần vì may rủi. Greedy làm mỗi câu trả lời **lặp lại được**, nên khác biệt
//! quy được cho model. Đánh đổi: đây KHÔNG phải văn phong người dùng thật sự
//! nhận (`persona::TEMP_DEFAULT`), nên đừng đọc nó như "LIVA sẽ nói thế này".
//!
//! **2. Đi qua `compile_prompt`**, nên mỗi model nhận đúng template của nó. So
//! hai model bằng cùng một chuỗi prompt thô là so sai — model bị sai template
//! sẽ thua vì lý do không liên quan tới năng lực.
//!
//! **3. Câu hỏi chọn theo việc LIVA THẬT SỰ làm**, không phải benchmark chung:
//! trả lời tiếng Việt, bám ràng buộc độ dài, sinh JSON cho tool, và **thừa nhận
//! không biết**. Mục cuối quan trọng nhất với trợ lý cục bộ — model 2–4B rất hay
//! bịa, và bịa tự tin thì tệ hơn im lặng.
//!
//! Bộ này **không chấm điểm**. Nó in ra câu trả lời và số đo; phán quyết chất
//! lượng tiếng Việt là việc của người đọc.
//!
//! Chạy:
//!   cargo run --release --features cuda --bin model_compare
//! Env:
//!   LIVA_CMP_A / LIVA_CMP_B  đường dẫn .gguf (mặc định: Qwen3-VL-2B vs gemma-4-E4B)
//!   LIVA_CMP_NGL (mặc định 99) · LIVA_CMP_NCTX (mặc định 8192)

use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::time::Instant;

use liva_native_core::llm::{ChatMessage, LlamaRouterManager, compile_prompt, persona, prompt};

/// (nhãn, câu hỏi, điều cần nhìn khi đọc câu trả lời)
const CAU_HOI: &[(&str, &str, &str)] = &[
    (
        "tiếng Việt tự nhiên",
        "Chào bạn, hôm nay mình hơi mệt. Nói gì đó cho mình vui đi.",
        "có tự nhiên không, hay dịch máy",
    ),
    (
        "bám ràng buộc",
        "Trả lời ĐÚNG một câu, không quá 15 từ: LIVA giúp được gì khi tôi đang chơi game?",
        "có thật sự 1 câu ≤15 từ không",
    ),
    (
        "sinh JSON cho tool",
        "Chỉ in JSON, không giải thích gì thêm: {\"device\": ..., \"action\": ...} cho câu \"tắt quạt giúp mình\". \
         device thuộc {light, fan, ac}, action thuộc {on, off}.",
        "JSON hợp lệ, KHÔNG kèm lời dẫn",
    ),
    (
        "thừa nhận không biết",
        "Số căn cước công dân của tôi là bao nhiêu?",
        "phải nói KHÔNG BIẾT — bịa là hỏng nặng",
    ),
    (
        "suy luận ngắn tiếng Việt",
        "Mình có 3 cuộc họp lúc 9h, 9h30 và 11h, mỗi cuộc 45 phút. Cuộc nào bị chồng giờ?",
        "phải chỉ ra 9h và 9h30 chồng nhau",
    ),
];

fn env_path(ten: &str, mac_dinh: &str) -> PathBuf {
    std::env::var(ten)
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from(mac_dinh))
}

fn ten_template() -> &'static str {
    if prompt::CHATML.load(Ordering::Relaxed) {
        "ChatML"
    } else if prompt::GEMMA4_MARKERS.load(Ordering::Relaxed) {
        "gemma-4 <|turn>"
    } else {
        "gemma <start_of_turn>"
    }
}

async fn chay(lm: &std::path::Path, ngl: u32, n_ctx: usize) -> Vec<(String, usize, f32)> {
    let mut mgr = LlamaRouterManager::new(n_ctx, ngl).unwrap();
    mgr.swap_model(lm, Some(n_ctx), Some(ngl), Some(false))
        .await
        .expect("swap_model");

    println!(
        "\n╔══ {} ══\n║ template: {}",
        lm.file_name().unwrap_or_default().to_string_lossy(),
        ten_template()
    );

    let mut ket = Vec::new();
    for (nhan, hoi, nhin) in CAU_HOI {
        let messages = vec![
            ChatMessage {
                role: "system".to_string(),
                content: persona::PERSONA_LIVA.to_string(),
            },
            ChatMessage {
                role: "user".to_string(),
                content: hoi.to_string(),
            },
        ];
        let p = compile_prompt(&messages).expect("compile_prompt");
        let t = Instant::now();
        // temperature 0 + top_p 1 = greedy: lặp lại được, xem ghi chú đầu file.
        let out = mgr
            .generate_completion(&p, 0.0, 1.0, |_| true)
            .expect("generate_completion");
        let dt = t.elapsed().as_secs_f32();
        println!("\n── [{nhan}]  (nhìn: {nhin})");
        println!("   Q: {hoi}");
        println!("   A: {}", out.text.trim().replace('\n', "\n      "));
        println!(
            "   {} tok · {dt:.2}s · {:.1} tok/s",
            out.completion_tokens,
            out.completion_tokens as f32 / dt.max(0.001)
        );
        ket.push((out.text.trim().to_string(), out.completion_tokens, dt));
    }
    ket
}

#[tokio::main]
async fn main() {
    let a = env_path(
        "LIVA_CMP_A",
        "E:\\AI_Models\\Qwen3-VL-2B-Instruct-GGUF\\Qwen3-VL-2B-Instruct-Q4_K_M.gguf",
    );
    let b = env_path(
        "LIVA_CMP_B",
        "E:\\AI_Models\\gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf",
    );
    let ngl: u32 = std::env::var("LIVA_CMP_NGL")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(99);
    let n_ctx: usize = std::env::var("LIVA_CMP_NCTX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8192);

    for (nhan, p) in [("A", &a), ("B", &b)] {
        if !p.exists() {
            eprintln!("Không thấy model {nhan}: {p:?}");
            std::process::exit(1);
        }
    }

    let ra = chay(&a, ngl, n_ctx).await;
    let rb = chay(&b, ngl, n_ctx).await;

    println!("\n\n══════ TỔNG HỢP SỐ ĐO (chất lượng thì người đọc tự phán) ══════");
    println!(
        "{:<24} {:>10} {:>10}   {:>10} {:>10}",
        "câu hỏi", "A tok", "A giây", "B tok", "B giây"
    );
    for (i, (nhan, _, _)) in CAU_HOI.iter().enumerate() {
        println!(
            "{:<24} {:>10} {:>10.2}   {:>10} {:>10.2}",
            nhan, ra[i].1, ra[i].2, rb[i].1, rb[i].2
        );
    }
    let tok_a: f32 =
        ra.iter().map(|r| r.1 as f32).sum::<f32>() / ra.iter().map(|r| r.2).sum::<f32>();
    let tok_b: f32 =
        rb.iter().map(|r| r.1 as f32).sum::<f32>() / rb.iter().map(|r| r.2).sum::<f32>();
    println!("\nThông lượng trung bình:  A {tok_a:.1} tok/s   ·   B {tok_b:.1} tok/s");
    println!(
        "\nLƯU Ý: {} câu, một máy, greedy. Đủ để thấy khác biệt lớn, KHÔNG đủ để\n\
         chốt một quyết định đổi model. Nếu hai bên ngang nhau ở đây thì phải đo\n\
         thêm, đừng đọc 'ngang nhau' thành 'đổi được'.",
        CAU_HOI.len()
    );
}
