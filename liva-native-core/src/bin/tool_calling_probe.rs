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
    DEFAULT_TOP_K, NATIVE_SERVER, Selection, ToolCatalog, compile_selection_prompt,
    parse_selection, rank_tools, rank_tools_scored, validate_arguments,
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

/// Corpus RIÊNG để đo ngưỡng, rộng hơn `CORPUS`.
///
/// `CORPUS` chỉ có 3 câu âm tính — quá ít để nói bất cứ điều gì về một ngưỡng,
/// vì ngưỡng sai chính là ở phía âm tính. Ở đây thêm nhiều câu trò chuyện, và
/// thêm câu cần tool KHÁC smart-home (đọc/tìm vault) để không đo lệch về một tool.
///
/// `true` = có tool phù hợp; `false` = trò chuyện thuần.
const CORPUS_NGUONG: &[(&str, bool)] = &[
    // Cần tool — smart home
    ("bật đèn", true),
    ("tắt máy lạnh", true),
    ("mở quạt lên giúp mình", true),
    ("turn off the light please", true),
    // Cần tool — vault
    ("đọc file ghi-chu.md trong vault", true),
    ("tìm trong vault xem có gì về kiến trúc", true),
    ("mở ghi chú hôm qua ra xem", true),
    ("search the vault for mcp", true),
    ("ghi lại đoạn này vào ghi chú", true),
    // Trò chuyện thuần — KHÔNG tool nào phù hợp
    ("hôm nay thế nào", false),
    ("kể cho mình một chuyện vui", false),
    ("let's get back on track", false),
    ("bạn nghĩ sao về chuyện đó", false),
    ("mình hơi mệt", false),
    ("cảm ơn nhé", false),
    ("tại sao trời lại mưa", false),
    ("giải thích cho mình về hố đen", false),
    ("mình tên gì nhỉ", false),
    ("nói tiếng Anh đi", false),
    ("đếm từ 1 đến 5", false),
];

/// Đo xem điểm cosine top-1 có tách bạch "cần tool" khỏi "trò chuyện" không.
///
/// Đây là điều kiện để bật G1 mặc định mà không trả ~1,9 s cho mọi lượt chat: nếu
/// có khoảng trống giữa hai nhóm, một ngưỡng tiền lọc bỏ hẳn lượt LLM cho câu trò
/// chuyện. Nếu KHÔNG có khoảng trống thì ngưỡng là ý tồi, và biết điều đó cũng là
/// kết quả.
fn do_nguong(catalog: &ToolCatalog, embedder: &mut EmbeddingEngine) {
    println!("── Đo ngưỡng: điểm cosine top-1 ──");
    let mut co_tool: Vec<f32> = Vec::new();
    let mut tro_chuyen: Vec<f32> = Vec::new();

    // Giả thuyết thứ hai: BIÊN (top1 − top2). Nếu câu thật sự khớp một tool thì
    // khoảng cách tới tool thứ hai phải rộng hơn so với câu trò chuyện — nơi mọi
    // tool đều "hơi liên quan" như nhau.
    let mut bien_tool: Vec<f32> = Vec::new();
    let mut bien_chat: Vec<f32> = Vec::new();

    for (cau, can_tool) in CORPUS_NGUONG {
        let xh = rank_tools_scored(catalog, cau, Some(embedder), 2);
        let (i, diem) = xh[0];
        let bien = diem - xh.get(1).map(|(_, d)| *d).unwrap_or(diem);
        if *can_tool {
            co_tool.push(diem);
            bien_tool.push(bien);
        } else {
            tro_chuyen.push(diem);
            bien_chat.push(bien);
        }
        println!(
            "  {} {:<38} {:.4}  biên {:.4}  {}",
            if *can_tool { "T" } else { "·" },
            cau,
            diem,
            bien,
            catalog.tools()[i].name
        );
    }

    let min_tool = co_tool.iter().copied().fold(f32::MAX, f32::min);
    let max_tool = co_tool.iter().copied().fold(f32::MIN, f32::max);
    let min_chat = tro_chuyen.iter().copied().fold(f32::MAX, f32::min);
    let max_chat = tro_chuyen.iter().copied().fold(f32::MIN, f32::max);

    println!(
        "\n  cần tool  (n={}): {:.4} … {:.4}\n  trò chuyện (n={}): {:.4} … {:.4}",
        co_tool.len(),
        min_tool,
        max_tool,
        tro_chuyen.len(),
        min_chat,
        max_chat
    );

    if min_tool > max_chat {
        let giua = (min_tool + max_chat) / 2.0;
        println!(
            "\n  ✅ CÓ tách bạch. Khoảng trống {:.4} ({:.4} … {:.4}).\n     \
             Một ngưỡng ≈ {:.3} chia đúng cả {} ca trên corpus này.",
            min_tool - max_chat,
            max_chat,
            min_tool,
            giua,
            CORPUS_NGUONG.len()
        );
        println!(
            "     LƯU Ý: corpus {} câu, một máy, một model embedding. Đủ để nói\n     \
             \"đáng thử\", KHÔNG đủ để chốt một hằng số vào code.",
            CORPUS_NGUONG.len()
        );
    } else {
        let chong = tro_chuyen.iter().filter(|d| **d >= min_tool).count();
        println!(
            "\n  ❌ KHÔNG tách bạch: {} câu trò chuyện có điểm ≥ câu cần-tool thấp nhất\n     \
             ({:.4}). Ngưỡng đơn trên điểm top-1 sẽ hoặc bỏ sót lệnh thật, hoặc\n     \
             vẫn chạy LLM cho câu trò chuyện.",
            chong, min_tool
        );
        println!(
            "     Toàn bộ điểm nằm trong {:.2}–{:.2}: dải hẹp là bản chất họ E5 (cosine\n     \
             luôn cao), nên ngưỡng TUYỆT ĐỐI là ý tồi với model này, không chỉ với\n     \
             corpus này.",
            min_chat.min(min_tool),
            max_chat.max(max_tool)
        );
    }

    // Giả thuyết BIÊN.
    let min_bt = bien_tool.iter().copied().fold(f32::MAX, f32::min);
    let max_bt = bien_tool.iter().copied().fold(f32::MIN, f32::max);
    let min_bc = bien_chat.iter().copied().fold(f32::MAX, f32::min);
    let max_bc = bien_chat.iter().copied().fold(f32::MIN, f32::max);
    println!(
        "\n  Biên (top1−top2)  cần tool: {:.4} … {:.4}  ·  trò chuyện: {:.4} … {:.4}",
        min_bt, max_bt, min_bc, max_bc
    );
    if min_bt > max_bc {
        println!(
            "  ✅ BIÊN tách được (khoảng trống {:.4}) — đây là tín hiệu đáng thử tiếp.",
            min_bt - max_bc
        );
    } else {
        let chong = bien_chat.iter().filter(|d| **d >= min_bt).count();
        println!(
            "  ❌ Biên cũng KHÔNG tách được: {} câu trò chuyện có biên ≥ biên cần-tool\n     \
             thấp nhất. Cả hai giả thuyết rẻ đều chết.",
            chong
        );
    }
    println!();
}

fn main() {
    let mut truot = 0usize;

    let server = NativeMcpServer::new("vault");
    let mut catalog = ToolCatalog::new();
    catalog.add_server(NATIVE_SERVER, &server.list_tools().tools);
    for (ten, vi_du) in NativeMcpServer::retrieval_examples() {
        catalog.set_embed_extra(NATIVE_SERVER, ten, vi_du);
    }
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
        // Tầng 1 CHỈ kiểm chiều dương: câu smart-home phải đưa `control_smarthome`
        // lên top-1, vì tool không vào được prompt thì LLM không thể chọn nó.
        //
        // Chiều âm KHÔNG kiểm danh tính top-1, và đó là quyết định có lý do đo
        // được: `top_k = 4` bằng đúng số tool nội bộ, nên MỌI tool vào prompt bất
        // kể thứ hạng — danh tính top-1 của một câu trò chuyện không ảnh hưởng gì
        // tới hành vi. Ví dụ cụ thể: `"let's get back on track"` cho top-1 là
        // `control_smarthome` (embedding thấy nó giống câu ra lệnh tiếng Anh),
        // nhưng điểm của nó là 0,7695 — THẤP NHẤT trong cả corpus 20 câu — và LLM
        // vẫn trả NONE đúng. Thuộc tính thật sự quan trọng ở chiều âm là ĐIỂM nằm
        // dưới dải cần-tool, và mục "Đo ngưỡng" bên dưới đo đúng cái đó.
        //
        // Vẫn IN danh tính top-1 cho cả hai chiều để không che tín hiệu.
        println!("── Tầng 1: truy hồi (câu smart-home phải cho control_smarthome top-1) ──");
        for (cau, mong_doi) in CORPUS {
            let top = rank_tools(
                &catalog,
                cau,
                embedder.as_mut().map(|e| e as _),
                DEFAULT_TOP_K,
            );
            let top1 = catalog.tools()[top[0]].name.as_str();
            match mong_doi {
                Some(_) => {
                    let dat = top1 == "control_smarthome";
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
                // Không phán quyết — chỉ ghi nhận.
                None => println!("·  {cau:<26} top-1={top1}  (chiều âm: xem mục Đo ngưỡng)"),
            }
        }
        println!();
        if let Some(e) = embedder.as_mut() {
            do_nguong(&catalog, e);
        }
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
        println!("{} {:<26} {:?}", if dat { "✅" } else { "❌" }, cau, y);
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
        let top = rank_tools(
            &catalog,
            cau,
            embedder.as_mut().map(|e| e as _),
            DEFAULT_TOP_K,
        );
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
        // PHẢI theo đúng trình tự của `tool_calling::select_tool`: parse → KIỂM
        // THAM SỐ → tham số sai thì coi như không có tool (rơi về route_intent).
        //
        // Bản đầu của probe chỉ so output `parse_selection` thô, nên nó báo đỏ ca
        // `{"path": ""}` dù production xử lý đúng — probe đo một đường khác với
        // đường thật thì con số của nó không nói được gì về hành vi.
        let chon = match parse_selection(&raw, ung_vien.len()) {
            Selection::Tool { index, arguments } => {
                match validate_arguments(&ung_vien[index].input_schema, &arguments) {
                    Ok(()) => Selection::Tool { index, arguments },
                    Err(e) => Selection::Unreadable(format!("tham số bị từ chối: {e}")),
                }
            }
            khac => khac,
        };

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
            // Tham số bị từ chối cho câu trò chuyện = production rơi về
            // `route_intent` ⇒ ra Chat, tức ĐÚNG. Vẫn in lý do để không che việc
            // model đã chọn sai tool trước khi bị chặn.
            (Selection::Unreadable(ly), None) => (true, format!("không tool (bị chặn: {ly})")),
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
