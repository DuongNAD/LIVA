//! Cổng nghiệm thu U19: hai tool OS (âm lượng, phát nhạc) có được chọn đúng
//! khi người dùng nói tiếng Việt tự nhiên không.
//!
//! Vì sao là probe riêng chứ không thêm vào `tool_calling_probe`: probe kia đo
//! cổng G1 — "đường keyword và đường LLM có khớp nhau trên corpus smart-home".
//! U19 hỏi một câu khác: hai tool **mới** có được truy hồi và chọn đúng không,
//! và **việc thêm chúng có làm hỏng cổng cũ không**. Trộn hai câu hỏi vào một
//! bộ đo sẽ làm không đọc được cái nào hỏng khi số tụt.
//!
//! ## Vì sao tầng 1 giờ mới thật sự có ý nghĩa
//!
//! Trước U19, catalog có **4 tool** và `top_k = 4` — mọi tool đều vào prompt bất
//! kể thứ hạng, nên truy hồi không ảnh hưởng gì tới hành vi. Thêm hai tool nữa
//! thành **6 > 4**: từ đây truy hồi **loại bớt tool khỏi prompt**, và một tool
//! bị loại thì LLM không có cách nào chọn nó. Đó là thay đổi hành vi thật mà
//! U19 mang lại, nên phải đo.
//!
//! ## Chạy
//!
//! ```powershell
//! # chỉ tầng 1 (nhanh, cần models/embedding)
//! .\target\debug\os_control_probe.exe
//! # cả hai tầng (cần thêm model LLM)
//! .\target\debug\os_control_probe.exe "E:\AI_Models\Qwen3-VL-2B-Instruct-GGUF\Qwen3-VL-2B-Instruct-Q4_K_M.gguf"
//! ```
//!
//! Thoát 1 nếu có ca nào trượt.

use liva_native_core::llm::embedder::{EmbeddingEngine, resolve_model_dir};
use liva_native_core::llm::tool_calling::{
    DEFAULT_TOP_K, NATIVE_SERVER, Selection, ToolCatalog, compile_selection_prompt, parse_selection,
    rank_tools, validate_arguments,
};
use liva_native_core::integrations::os_control::{MediaArgs, VolumeArgs};
use liva_native_core::mcp::server::NativeMcpServer;
use serde_json::Value;
use std::path::PathBuf;

/// Hành động sau khi ĐÃ phân giải qua chính struct mà tool dùng.
///
/// Vì sao không so chuỗi thô: alias là một phần hợp đồng của `os_control`
/// (`"pause"` nghĩa là `play_pause`). So chuỗi thô sẽ báo trượt những ca mà tool
/// thật sự làm đúng — tức đo sai thứ cần đo. Câu hỏi đúng là "tool sẽ LÀM gì",
/// không phải "model gõ chữ nào".
fn hanh_dong_chuan(tool: &str, args: &Value) -> Option<String> {
    let ra_chuoi = |v: Value| v.as_str().map(str::to_string);
    match tool {
        "control_volume" => serde_json::from_value::<VolumeArgs>(args.clone())
            .ok()
            .and_then(|a| serde_json::to_value(a.action).ok())
            .and_then(ra_chuoi),
        "control_media" => serde_json::from_value::<MediaArgs>(args.clone())
            .ok()
            .and_then(|a| serde_json::to_value(a.action).ok())
            .and_then(ra_chuoi),
        _ => args
            .get("action")
            .and_then(|v| v.as_str())
            .map(str::to_string),
    }
}

/// Câu → (tool mong đợi, trường `action` mong đợi).
///
/// Cố tình dùng cách nói ĐỜI THƯỜNG, không phải cách nói theo tên tool: người ta
/// nói "nhỏ nhạc lại" chứ không nói "giảm âm lượng hệ thống xuống 4 nấc". Nếu
/// corpus viết theo giọng tài liệu thì nó chỉ đo được chính nó.
const CORPUS: &[(&str, Option<(&str, &str)>)] = &[
    // ── control_volume ──────────────────────────────────────────────────────
    ("nhỏ nhạc lại giúp mình", Some(("control_volume", "down"))),
    ("to lên chút đi", Some(("control_volume", "up"))),
    ("tắt tiếng đi", Some(("control_volume", "mute"))),
    ("ồn quá, giảm âm lượng xuống", Some(("control_volume", "down"))),
    ("mở to hơn nữa", Some(("control_volume", "up"))),
    // ── control_media ───────────────────────────────────────────────────────
    ("tạm dừng nhạc", Some(("control_media", "play_pause"))),
    ("phát tiếp đi", Some(("control_media", "play_pause"))),
    ("chuyển bài khác", Some(("control_media", "next"))),
    ("quay lại bài trước", Some(("control_media", "previous"))),
    ("bật nhạc lên", Some(("control_media", "play_pause"))),
    // ── HỒI QUY: hai tool mới không được cướp câu của smart-home ────────────
    // "bật/tắt" xuất hiện ở cả ba tool, nên đây là chỗ dễ vỡ nhất khi mở rộng
    // danh mục. Nếu "bật đèn" bắt đầu rơi vào control_volume thì U19 đã làm
    // hỏng cổng 13/13 của G1 — phải thấy được ngay ở đây.
    ("bật đèn", Some(("control_smarthome", "on"))),
    ("tắt quạt", Some(("control_smarthome", "off"))),
    // ── âm tính ─────────────────────────────────────────────────────────────
    ("hôm nay thế nào", None),
    ("kể cho mình một chuyện vui", None),
];

fn ket_thuc(truot: usize) -> ! {
    if truot > 0 {
        println!("\n❌ {truot} ca trượt.");
        std::process::exit(1);
    }
    println!("\n✅ Tất cả đạt.");
    std::process::exit(0);
}

fn main() {
    let mut truot = 0usize;

    let server = NativeMcpServer::new("vault");
    let mut catalog = ToolCatalog::new();
    catalog.add_server(NATIVE_SERVER, &server.list_tools().tools);
    for (ten, vi_du) in NativeMcpServer::retrieval_examples() {
        catalog.set_embed_extra(NATIVE_SERVER, ten, vi_du);
    }
    println!("Catalog: {} tool nội bộ · top_k = {DEFAULT_TOP_K}", catalog.len());
    if catalog.len() > DEFAULT_TOP_K {
        println!(
            "→ {} tool bị loại khỏi prompt mỗi lượt: từ đây thứ hạng truy hồi CHI PHỐI hành vi.\n",
            catalog.len() - DEFAULT_TOP_K
        );
    }

    // ── Tầng 1: truy hồi ────────────────────────────────────────────────────
    let dir = resolve_model_dir();
    let mut embedder = match EmbeddingEngine::load(&dir) {
        Ok(e) => Some(e),
        Err(e) => {
            println!("!!! Không nạp được embedder ({}): {e}", dir.display());
            println!("!!! Tầng 1 KHÔNG được kiểm — truy hồi khi đó chỉ còn trùng token.\n");
            None
        }
    };

    if embedder.is_some() {
        println!("── Tầng 1: tool mong đợi có lọt vào prompt không ──");
        for (cau, mong_doi) in CORPUS {
            let Some((ten_mong_doi, _)) = mong_doi else {
                continue; // câu âm tính không có tool nào để đòi hỏi
            };
            let top = rank_tools(&catalog, cau, embedder.as_mut().map(|e| e as _), DEFAULT_TOP_K);
            let vao_prompt: Vec<&str> = top
                .iter()
                .map(|&i| catalog.tools()[i].name.as_str())
                .collect();
            let dat = vao_prompt.contains(ten_mong_doi);
            if !dat {
                truot += 1;
            }
            println!(
                "{} {cau:<28} mong đợi {ten_mong_doi:<18} · vào prompt: {}",
                if dat { "✅" } else { "❌" },
                vao_prompt.join(", ")
            );
        }
    }

    // ── Tầng 2: LLM chọn tool ───────────────────────────────────────────────
    let Some(model) = std::env::args().nth(1).map(PathBuf::from) else {
        println!("\n(Bỏ qua tầng 2 — truyền đường dẫn model GGUF để chạy.)");
        ket_thuc(truot);
    };
    if embedder.is_none() {
        println!("\n!!! Không có embedder thì tầng 2 đo lệch (mọi tool vào prompt theo thứ tự khai báo).");
    }

    let rt = tokio::runtime::Runtime::new().expect("tokio runtime");
    let mut llm = liva_native_core::llm::LlamaRouterManager::new(4096, 0).expect("router manager");
    println!("\nĐang nạp {} …", model.display());
    if let Err(e) = rt.block_on(llm.swap_model(&model, Some(4096), Some(0), Some(false))) {
        println!("!!! Không nạp được model: {e}");
        ket_thuc(truot + 1);
    }

    println!("\n── Tầng 2: LLM chọn tool + tham số ──");
    let mut do_tre: Vec<u128> = Vec::new();
    for (cau, mong_doi) in CORPUS {
        let top = rank_tools(&catalog, cau, embedder.as_mut().map(|e| e as _), DEFAULT_TOP_K);
        let ung_vien: Vec<_> = top.iter().map(|&i| &catalog.tools()[i]).collect();
        let prompt = match compile_selection_prompt(&ung_vien, cau) {
            Ok(p) => p,
            Err(e) => {
                println!("❌ {cau:<28} không dựng được prompt: {e}");
                truot += 1;
                continue;
            }
        };

        // temperature 0: đây là phân loại, không phải sáng tác.
        let t0 = std::time::Instant::now();
        let raw = match llm.generate_completion(&prompt, 0.0, 1.0, |_| true) {
            Ok(o) => o.text,
            Err(e) => {
                println!("❌ {cau:<28} LLM lỗi: {e}");
                truot += 1;
                continue;
            }
        };
        do_tre.push(t0.elapsed().as_millis());
        let chon = parse_selection(&raw, ung_vien.len());

        let (dat, mo_ta) = match (&chon, mong_doi) {
            (Selection::Tool { index, arguments }, Some((ten, action))) => {
                let t = ung_vien[*index];
                let hop_le = validate_arguments(&t.input_schema, arguments);
                let action_that = hanh_dong_chuan(&t.name, arguments);
                let dat = &t.name == ten && action_that.as_deref() == Some(*action) && hop_le.is_ok();
                // Khi không phân giải được, in THAM SỐ THÔ. Không có nó thì mọi
                // ca hỏng đều trông giống nhau và chỉ còn cách đoán — đúng thứ
                // đã làm mất một vòng đo.
                let hien_thi = match &action_that {
                    Some(a) => format!("action={a:?}"),
                    None => format!("KHÔNG phân giải được, thô = {arguments}"),
                };
                (
                    dat,
                    format!(
                        "chọn {} {}{}",
                        t.name,
                        hien_thi,
                        match hop_le {
                            Ok(()) => String::new(),
                            Err(e) => format!(" · schema: {e}"),
                        }
                    ),
                )
            }
            (Selection::NoTool, None) => (true, "không dùng tool — đúng".to_string()),
            (Selection::NoTool, Some((ten, _))) => (false, format!("nói không cần tool, đáng lẽ {ten}")),
            (Selection::Tool { index, .. }, None) => {
                (false, format!("chọn {} cho câu trò chuyện", ung_vien[*index].name))
            }
            // `Unreadable` KHÔNG tính là trượt tuyệt đối: hợp đồng của G1 là rơi
            // về `route_intent` khi không đọc được. Nhưng vẫn in ra, vì tỉ lệ
            // này cao nghĩa là prompt hoặc model đang không hợp nhau.
            (Selection::Unreadable(r), _) => (false, format!("không đọc được: {r}")),
        };
        if !dat {
            truot += 1;
        }
        println!("{} {cau:<28} {mo_ta}", if dat { "✅" } else { "❌" });
    }

    if !do_tre.is_empty() {
        let tong: u128 = do_tre.iter().sum();
        let mut sap = do_tre.clone();
        sap.sort_unstable();
        println!(
            "\nĐộ trễ chọn tool: trung bình {} ms · trung vị {} ms · cao nhất {} ms",
            tong / do_tre.len() as u128,
            sap[sap.len() / 2],
            sap[sap.len() - 1]
        );
        println!("(Đây là chi phí THÊM cho mỗi lượt chat khi LIVA_TOOL_CALLING=1.)");
    }

    ket_thuc(truot);
}
