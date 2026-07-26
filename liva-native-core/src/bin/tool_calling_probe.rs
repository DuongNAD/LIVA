//! Đo cổng nghiệm thu của G1: đường keyword và đường LLM có khớp nhau trên
//! corpus smart-home không.
//!
//! Vì sao là probe chứ không phải `#[test]`: cổng này cần **model thật** — model
//! embedding 470 MB cho phần truy hồi, và một model LLM 2–4 GB cho phần chọn
//! tool. Cả hai bị gitignore. Nhét vào `cargo test` sẽ biến bộ test thành thứ
//! chỉ chạy trên một máy. Cùng lý do đã có `vieneu_probe`, `qwen3vl_probe`, …
//!
//! Hai tầng đo, tầng sau cần tầng trước:
//!
//! 1. **Truy hồi** — `control_smarthome` có nằm top-1 cho câu smart-home không.
//!    Đây là điều kiện CẦN: tool không vào được prompt thì LLM không thể chọn nó.
//! 2. **Chọn tool** — LLM có chọn đúng tool đó, với tham số khớp `route_intent`
//!    không. Đây chính là câu "hai đường phải khớp nhau".
//!
//! ## Chạy
//!
//! ```powershell
//! # chỉ tầng 1 (nhanh, cần models/embedding)
//! .\target\debug\tool_calling_probe.exe
//! # cả hai tầng (cần thêm model LLM)
//! $env:LIVA_TOOL_CALLING="1"
//! .\target\debug\tool_calling_probe.exe "E:\AI_Models\gemma-4-E4B-it-qat-UD-Q4_K_XL.gguf"
//! ```
//!
//! Thoát 1 nếu có ca nào trượt.

use liva_native_core::agent::graph::{Intent, route_intent};
use liva_native_core::llm::embedder::{EmbeddingEngine, resolve_model_dir};
use liva_native_core::llm::tool_calling::{
    DEFAULT_TOP_K, NATIVE_SERVER, Selection, ToolCatalog, parse_selection, rank_tools,
    compile_selection_prompt, validate_arguments,
};
use liva_native_core::mcp::server::NativeMcpServer;
use std::path::PathBuf;

/// Corpus: đúng những cách nói mà `route_intent` khai báo là hiểu được, cộng ba
/// ca âm tính. Ca "back on track" là hồi quy — `contains("ac")` khớp "b-ac-k" và
/// `contains("on")` khớp "on" nên bản cũ hiểu thành lệnh bật điều hoà.
const CORPUS: &[(&str, Option<(&str, &str)>)] = &[
    ("bật đèn", Some(("light", "on"))),
    ("bật đèn giúp mình", Some(("light", "on"))),
    ("tắt đèn", Some(("light", "off"))),
    ("mở quạt", Some(("fan", "on"))),
    ("tắt quạt", Some(("fan", "off"))),
    ("bật điều hoà", Some(("ac", "on"))),
    ("tắt máy lạnh", Some(("ac", "off"))),
    ("turn on the light", Some(("light", "on"))),
    ("turn off the fan", Some(("fan", "off"))),
    ("turn on the ac", Some(("ac", "on"))),
    ("let's get back on track", None),
    ("hôm nay thế nào", None),
    ("kể cho mình một chuyện vui", None),
];

fn main() {
    let mut truot = 0usize;

    let server = NativeMcpServer::new("vault");
    let mut catalog = ToolCatalog::new();
    catalog.add_server(NATIVE_SERVER, &server.list_tools().tools);
    println!("Catalog: {} tool nội bộ\n", catalog.len());

    // ── Tầng 1: truy hồi ────────────────────────────────────────────────────
    let dir = resolve_model_dir();
    let mut embedder = match EmbeddingEngine::load(&dir) {
        Ok(e) => Some(e),
        Err(e) => {
            println!("!!! Không nạp được embedder ({}): {e}", dir.display());
            println!("!!! Tầng 1 KHÔNG được kiểm. G1 mà thiếu embedder thì truy hồi chỉ còn");
            println!("!!! trùng token — đo được là MÙ với mọi câu (0 điểm), nên đây không");
            println!("!!! phải chi tiết nhỏ.\n");
            None
        }
    };

    if embedder.is_some() {
        println!("── Tầng 1: truy hồi (control_smarthome phải top-1 cho câu smart-home) ──");
        for (cau, mong_doi) in CORPUS {
            let top = rank_tools(&catalog, cau, embedder.as_mut().map(|e| e as _), DEFAULT_TOP_K);
            let top1 = catalog.tools()[top[0]].name.as_str();
            let la_sh = mong_doi.is_some();
            let dat = if la_sh {
                top1 == "control_smarthome"
            } else {
                top1 != "control_smarthome"
            };
            if !dat {
                truot += 1;
            }
            println!(
                "{} {:<26} top-1={}",
                if dat { "✅" } else { "❌" },
                cau,
                top1
            );
        }
        println!();
    }

    // ── Tầng 0: route_intent phải giữ nguyên hành vi ────────────────────────
    println!("── Tầng 0: route_intent (đường nhanh) ──");
    for (cau, mong_doi) in CORPUS {
        let y = route_intent(cau);
        let dat = match (mong_doi, &y) {
            (Some((d, a)), Intent::SmartHome { device, action }) => device == d && action == a,
            (None, Intent::Chat) => true,
            _ => false,
        };
        if !dat {
            truot += 1;
        }
        println!(
            "{} {:<26} {:?}",
            if dat { "✅" } else { "❌" },
            cau,
            y
        );
    }
    println!();

    // ── Tầng 2: LLM chọn tool ───────────────────────────────────────────────
    let model = std::env::args().nth(1).map(PathBuf::from);
    let Some(model) = model else {
        println!("── Tầng 2: BỎ QUA — chưa truyền đường dẫn model LLM ──");
        println!("   Cổng \"hai đường khớp nhau\" CHƯA được đo. Chạy lại:");
        println!("   .\\target\\debug\\tool_calling_probe.exe <đường dẫn .gguf>");
        ket_thuc(truot);
    };
    if !model.exists() {
        println!("!!! Không có {}", model.display());
        ket_thuc(truot + 1);
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut llm = liva_native_core::llm::LlamaRouterManager::new(4096, 0).expect("router manager");
    println!("Đang nạp {} …", model.display());
    if let Err(e) = rt.block_on(llm.swap_model(&model, Some(4096), Some(0), Some(false))) {
        println!("!!! Không nạp được model: {e}");
        ket_thuc(truot + 1);
    }

    println!("\n── Tầng 2: LLM chọn tool, so với route_intent ──");
    let mut khop = 0usize;
    let mut tong = 0usize;
    let mut do_tre: Vec<u128> = Vec::new();
    let mut do_dai_prompt: Vec<usize> = Vec::new();
    for (cau, mong_doi) in CORPUS {
        let top = rank_tools(&catalog, cau, embedder.as_mut().map(|e| e as _), DEFAULT_TOP_K);
        let ung_vien: Vec<_> = top.iter().map(|&i| &catalog.tools()[i]).collect();
        // PHẢI qua chat template. Bản đầu của probe này truyền prompt thô và
        // gemma trả về chuỗi rỗng cho cả 13 câu — 0/13, trông y như "model không
        // chọn được tool" trong khi thật ra model chưa hề được hỏi.
        let prompt = match compile_selection_prompt(&ung_vien, cau) {
            Ok(p) => p,
            Err(e) => {
                println!("❌ {cau:<26} không dựng được prompt: {e}");
                truot += 1;
                continue;
            }
        };

        // temperature 0: đây là phân loại, không phải sáng tác.
        let t0 = std::time::Instant::now();
        let raw = match llm.generate_completion(&prompt, 0.0, 1.0, |_| true) {
            Ok(o) => o.text,
            Err(e) => {
                println!("❌ {cau:<26} LLM lỗi: {e}");
                truot += 1;
                continue;
            }
        };
        // Độ trễ là lý do THỨ HAI để G1 tắt mặc định (lý do thứ nhất là đúng/sai,
        // và cổng đã 13/13). Vòng này chạy THÊM cho mỗi câu chat, nên con số dưới
        // đây là thời gian mọi lượt nói phải trả — kể cả "hôm nay thế nào".
        do_tre.push(t0.elapsed().as_millis());
        do_dai_prompt.push(prompt.chars().count());
        let chon = parse_selection(&raw, ung_vien.len());

        tong += 1;
        let (dat, mo_ta) = match (&chon, mong_doi) {
            (Selection::Tool { index, arguments }, Some((d, a))) => {
                let t = ung_vien[*index];
                let hop_le = validate_arguments(&t.input_schema, arguments);
                // route_intent dùng `action`, MCP tool dùng `command` — cùng một
                // năng lực, hai tên tham số. Chỗ lệch này là thật, xem tài liệu 04.
                let arg_khop = arguments.get("device").and_then(|v| v.as_str()) == Some(*d)
                    && arguments
                        .get("command")
                        .or_else(|| arguments.get("action"))
                        .and_then(|v| v.as_str())
                        == Some(*a);
                (
                    t.name == "control_smarthome" && hop_le.is_ok() && arg_khop,
                    format!("{} {arguments}", t.name),
                )
            }
            (Selection::NoTool, None) => (true, "NONE (đúng)".to_string()),
            (khac, _) => (false, format!("{khac:?}")),
        };
        if dat {
            khop += 1;
        } else {
            truot += 1;
        }
        println!(
            "{} {:<26} {}",
            if dat { "✅" } else { "❌" },
            cau,
            mo_ta.chars().take(90).collect::<String>()
        );
    }
    println!("\nHai đường khớp nhau: {khop}/{tong}");

    if !do_tre.is_empty() {
        let mut sap = do_tre.clone();
        sap.sort_unstable();
        let trung_vi = sap[sap.len() / 2];
        let tong_ms: u128 = do_tre.iter().sum();
        println!(
            "\n── Chi phí: đây là thời gian THÊM cho MỖI câu chat khi bật G1 ──\n\
             trung vị {} ms · nhanh nhất {} ms · chậm nhất {} ms · trung bình {} ms\n\
             prompt ~{} ký tự (≈{} token, chia 4 — ước lượng thô)",
            trung_vi,
            sap[0],
            sap[sap.len() - 1],
            tong_ms / do_tre.len() as u128,
            do_dai_prompt.iter().sum::<usize>() / do_dai_prompt.len(),
            do_dai_prompt.iter().sum::<usize>() / do_dai_prompt.len() / 4,
        );
    }
    ket_thuc(truot);
}

fn ket_thuc(truot: usize) -> ! {
    if truot == 0 {
        println!("\n✅ Không có ca nào trượt.");
        std::process::exit(0);
    }
    println!("\n❌ {truot} ca trượt.");
    std::process::exit(1);
}
