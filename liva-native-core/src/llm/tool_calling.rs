//! Vòng tool-calling do LLM dẫn (rung G1).
//!
//! Thay `route_intent` — bảng từ khoá cứng trong `agent/graph.rs` — bằng việc để
//! LLM chọn tool từ schema thật mà `tools/list` trả về. G0 đã làm cho
//! `tools/list` là thật (MCP client nói được với server ngoài); module này là
//! phần dùng nó.
//!
//! Xem `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md` §3 (G1).
//!
//! ## Ba ràng buộc định hình toàn bộ thiết kế
//!
//! 1. **`n_ctx` mặc định 4096, người dùng beta chạy model 2–4B.** Không thể nhét
//!    50 JSON Schema vào prompt. Nên: truy hồi top-k tool bằng embedder rồi chỉ
//!    chèn ngần đó, và render tham số ở dạng **gọn một dòng** thay vì dump schema
//!    thô. Một `schema_for!` của schemars đã ~200 token; bốn cái là hết ngân sách.
//!
//! 2. **`generate_completion` KHÔNG có grammar, không có JSON mode.** Không ép
//!    được model xuất JSON đúng khuôn. Nên hợp đồng output là **hai dòng có tiền
//!    tố** và tool được chọn **bằng SỐ**, không bằng tên: số thì model 2B không
//!    gõ sai chính tả, và tiền tố `TOOL:`/`ARGS:` cho phép định vị trường ngay cả
//!    khi model nói lan thêm vài câu. Bộ parse cố tình khoan dung.
//!
//! 3. **Prompt injection.** §4 của tài liệu 04: *"nội dung skill là dữ liệu,
//!    không phải lệnh; `SKILL.md` tải về không được phép lái vòng tool-calling ở
//!    G1"* và *"không bao giờ auto-exec skill vừa import"*. Nên việc **chọn** tool
//!    và việc **được phép chạy** tool là hai chuyện tách rời: xem [`ExecPolicy`].
//!    Mặc định, chỉ tool nội bộ không-ghi được tự chạy.
//!
//! ## Vì sao giữ `route_intent`
//!
//! Nó rẻ (không tốn một token LLM nào), và nó đã xử lý đúng cách nói tiếng Việt
//! ("bật đèn giúp mình"). Nó là **đường nhanh** đứng trước, và là **fallback** khi
//! LLM trả về thứ không parse được. Vòng LLM chỉ chạy khi đường nhanh nói "không
//! biết".

use crate::mcp::protocol::Tool;
use serde_json::{Value, json};

/// Tên "server" quy ước cho các tool nội bộ của `mcp::server::NativeMcpServer`.
pub const NATIVE_SERVER: &str = "native";

/// Tiền tố dòng tham số trong prompt. Là hằng số để test kiểm được **đúng dòng
/// đó** — cụm "tham số" cũng xuất hiện trong dòng hợp đồng output ở cuối prompt,
/// nên tìm chuỗi con sẽ cho kết quả sai.
const PARAM_PREFIX: &str = "   tham số (* = bắt buộc): ";

/// Số tool tối đa chèn vào prompt. Xem ràng buộc 1 ở đầu file.
///
/// 4 là con số có lý do: với `n_ctx` 4096, sau persona + RAG + lịch sử hội thoại
/// thì phần còn lại cho danh sách tool chỉ còn vài trăm token.
pub const DEFAULT_TOP_K: usize = 4;

/// Một tool ứng viên, đã phẳng hoá từ mọi nguồn (nội bộ + MCP server ngoài).
#[derive(Debug, Clone, PartialEq)]
pub struct CatalogTool {
    /// [`NATIVE_SERVER`] hoặc tên server trong `mcp_config.json`.
    pub server: String,
    pub name: String,
    pub description: String,
    /// JSON Schema của tham số, giữ nguyên như server khai báo.
    pub input_schema: Value,
    /// Văn bản **chỉ dùng để embed**, không bao giờ vào prompt.
    ///
    /// Vì sao tách khỏi `description`: hai mục đích khác nhau bị nhồi vào một
    /// trường. Đo được 26/07/2026 khi thử nhồi ví dụ cách nói vào `description`:
    ///
    /// | | mô tả ngắn | nhồi ví dụ vào description |
    /// |---|---|---|
    /// | truy hồi | `"mở quạt lên giúp mình"` trượt | đúng, biên tốt hơn ~4× |
    /// | prompt | ~193 token | ~417 token |
    /// | độ trễ | 1877 ms | **3939 ms** |
    ///
    /// Ví dụ cách nói giúp *embedding* rất nhiều và giúp *LLM* gần như không —
    /// LLM chỉ thấy 4 ứng viên và cần mô tả gọn. Nên chúng thuộc về đây, không
    /// thuộc `description`. Tool từ server MCP ngoài để rỗng và hành xử y như cũ.
    pub embed_extra: String,
}

impl CatalogTool {
    /// Chuỗi dùng để embed. Ghép tên + mô tả (+ [`Self::embed_extra`]) vì tên một
    /// mình quá ngắn để embedding phân biệt được (`echo` vs `add`), còn mô tả một
    /// mình thì mất tín hiệu khi người dùng gọi thẳng tên tool.
    pub fn embed_text(&self) -> String {
        let mut s = if self.description.is_empty() {
            self.name.clone()
        } else {
            format!("{}: {}", self.name, self.description)
        };
        if !self.embed_extra.is_empty() {
            s.push(' ');
            s.push_str(&self.embed_extra);
        }
        s
    }

    /// Định danh đầy đủ, dùng trong log và khi đối chiếu allowlist.
    pub fn qualified(&self) -> String {
        format!("{}/{}", self.server, self.name)
    }
}

/// Gộp tool từ nhiều nguồn thành một danh sách phẳng.
#[derive(Debug, Clone, Default)]
pub struct ToolCatalog {
    tools: Vec<CatalogTool>,
}

impl ToolCatalog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn tools(&self) -> &[CatalogTool] {
        &self.tools
    }

    pub fn is_empty(&self) -> bool {
        self.tools.is_empty()
    }

    pub fn len(&self) -> usize {
        self.tools.len()
    }

    /// Thêm tool của một server. `Tool.input_schema` là `schemars::RootSchema`;
    /// ở đây chuyển sang `Value` để catalog không phụ thuộc schemars và để tool
    /// từ server ngoài (chỉ có JSON thô) nằm cùng một kiểu.
    pub fn add_server(&mut self, server: &str, tools: &[Tool]) {
        for t in tools {
            let input_schema = serde_json::to_value(&t.input_schema).unwrap_or_else(|_| json!({}));
            self.tools.push(CatalogTool {
                server: server.to_string(),
                name: t.name.clone(),
                description: t.description.clone(),
                input_schema,
                embed_extra: String::new(),
            });
        }
    }

    /// Gắn văn bản chỉ-để-embed cho một tool. Bỏ qua im lặng nếu không có tool
    /// tên đó — bảng gợi ý và danh sách tool có thể lệch nhau khi tool bị xoá,
    /// và điều đó không đáng làm hỏng cả catalog.
    pub fn set_embed_extra(&mut self, server: &str, name: &str, extra: &str) {
        if let Some(t) = self
            .tools
            .iter_mut()
            .find(|t| t.server == server && t.name == name)
        {
            t.embed_extra = extra.to_string();
        }
    }

    pub fn find(&self, server: &str, name: &str) -> Option<&CatalogTool> {
        self.tools
            .iter()
            .find(|t| t.server == server && t.name == name)
    }
}

// ── Truy hồi top-k ──────────────────────────────────────────────────────────

/// Cửa hẹp để [`rank_tools`] không phụ thuộc cứng vào `EmbeddingEngine`.
///
/// Có trait này thì test xếp hạng chạy được **mà không nạp 470 MB ONNX** —
/// nếu không, mọi test về thứ tự tool sẽ phải kéo theo model thật, tức thực tế
/// là không có test nào.
pub trait ToolEmbedder {
    fn embed_query_vec(&mut self, text: &str) -> Result<Vec<f32>, String>;
    fn embed_passage_vec(&mut self, text: &str) -> Result<Vec<f32>, String>;
}

impl ToolEmbedder for crate::llm::embedder::EmbeddingEngine {
    fn embed_query_vec(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_query(text)
    }
    fn embed_passage_vec(&mut self, text: &str) -> Result<Vec<f32>, String> {
        self.embed_passage(text)
    }
}

/// Tách token theo ranh giới ký tự chữ-số Unicode.
///
/// Cùng quy tắc với `agent/graph.rs::tokenize` (bản đó `private`): dùng
/// `is_alphanumeric` chứ không phải `is_ascii_alphanumeric` để `đèn`, `bật`,
/// `tắt` là token trọn vẹn.
fn tokenize(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Xếp hạng theo trùng token — đường dùng khi **không có** embedder.
///
/// Không phải BM25 đầy đủ: chỉ đếm tỉ lệ token của câu hỏi xuất hiện trong
/// tên+mô tả tool. Đủ để không bao giờ trả về danh sách rỗng, và tất định nên
/// test được. Model embedding vắng mặt là ca THẬT (`AppState.embedder` là
/// `Option`, và weight bị gitignore) — không có đường này thì G1 tắt ngóm trên
/// máy chưa tải model.
fn keyword_scores(catalog: &ToolCatalog, query: &str) -> Vec<f32> {
    let q = tokenize(query);
    catalog
        .tools
        .iter()
        .map(|t| {
            if q.is_empty() {
                return 0.0;
            }
            let hay = tokenize(&t.embed_text());
            let hit = q.iter().filter(|w| hay.contains(w)).count();
            hit as f32 / q.len() as f32
        })
        .collect()
}

/// Tích vô hướng. Vector từ `EmbeddingEngine` đã chuẩn hoá L2 (xem
/// `embedder::embed_raw`), nên tích vô hướng CHÍNH LÀ cosine — không chuẩn hoá
/// lại để khỏi tốn công vô ích.
fn dot(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(x, y)| x * y).sum()
}

/// Chọn `k` tool liên quan nhất tới câu của người dùng.
///
/// Có embedder thì xếp bằng cosine; embedder lỗi hoặc vắng mặt thì rơi về
/// [`keyword_scores`]. Trả về **chỉ số** trong catalog gốc, giữ thứ tự giảm dần
/// theo điểm; hoà điểm thì giữ thứ tự khai báo (tất định, test được).
pub fn rank_tools(
    catalog: &ToolCatalog,
    query: &str,
    embedder: Option<&mut dyn ToolEmbedder>,
    k: usize,
) -> Vec<usize> {
    rank_tools_scored(catalog, query, embedder, k)
        .into_iter()
        .map(|(i, _)| i)
        .collect()
}

/// Như [`rank_tools`] nhưng trả kèm **điểm**.
///
/// Vì sao cần điểm chứ không chỉ thứ hạng: thứ hạng luôn có top-1, kể cả khi
/// **không tool nào liên quan** — "hôm nay thế nào" vẫn cho một tool đứng đầu.
/// Muốn bỏ hẳn lượt LLM cho câu trò chuyện (tiết kiệm ~1,9 s đo được ở
/// `tool_calling_probe`) thì phải so điểm với một ngưỡng, và muốn biết ngưỡng nào
/// hợp lý thì phải xem được điểm.
///
/// Với embedder, điểm là **cosine** trong `[-1, 1]` (vector đã chuẩn hoá L2).
/// Không có embedder, điểm là tỉ lệ token trùng trong `[0, 1]` — **thang khác
/// hẳn**, nên đừng dùng chung một ngưỡng cho hai đường.
pub fn rank_tools_scored(
    catalog: &ToolCatalog,
    query: &str,
    embedder: Option<&mut dyn ToolEmbedder>,
    k: usize,
) -> Vec<(usize, f32)> {
    if catalog.is_empty() || k == 0 {
        return Vec::new();
    }

    let scores = match embedder {
        Some(e) => match embed_scores(catalog, query, e) {
            Ok(s) => s,
            Err(err) => {
                tracing::warn!("xếp hạng tool bằng embedding thất bại ({err}); dùng trùng token");
                keyword_scores(catalog, query)
            }
        },
        None => keyword_scores(catalog, query),
    };

    let mut idx: Vec<usize> = (0..catalog.len()).collect();
    // `sort_by` ổn định, nên hoà điểm giữ nguyên thứ tự khai báo.
    idx.sort_by(|&a, &b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    idx.truncate(k);
    idx.into_iter().map(|i| (i, scores[i])).collect()
}

fn embed_scores(
    catalog: &ToolCatalog,
    query: &str,
    embedder: &mut dyn ToolEmbedder,
) -> Result<Vec<f32>, String> {
    let q = embedder.embed_query_vec(query)?;
    let mut out = Vec::with_capacity(catalog.len());
    for t in &catalog.tools {
        let v = embedder.embed_passage_vec(&t.embed_text())?;
        if v.len() != q.len() {
            return Err(format!(
                "số chiều lệch: query {} vs tool {}",
                q.len(),
                v.len()
            ));
        }
        out.push(dot(&q, &v));
    }
    Ok(out)
}

// ── Dựng prompt ─────────────────────────────────────────────────────────────

/// Rút tham số từ JSON Schema thành **một dòng gọn**.
///
/// Vì sao không dump schema thô: `schema_for!(ControlSmartHomeArgs)` của schemars
/// ra ~200 token (có `$schema`, `title`, `definitions`…). Bốn tool là hết ngân
/// sách context của model 2–4B ở `n_ctx` 4096. Dòng gọn giữ đúng thứ model cần
/// biết — tên trường, kiểu, cái nào bắt buộc.
///
/// `*` = bắt buộc. Không có `properties` thì trả chuỗi rỗng (tool không tham số).
pub fn render_params(schema: &Value) -> String {
    let props = match schema.get("properties").and_then(Value::as_object) {
        Some(p) if !p.is_empty() => p,
        _ => return String::new(),
    };
    let required: Vec<&str> = schema
        .get("required")
        .and_then(Value::as_array)
        .map(|a| a.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();

    let mut parts: Vec<String> = Vec::with_capacity(props.len());
    for (name, spec) in props {
        // PHẢI giải `$ref` trước: schemars đặt mọi kiểu có TÊN (enum, struct
        // lồng) vào `definitions` và để `properties` chỉ có `{"$ref": …}`. Không
        // giải thì `type` và `enum` đều không thấy ⇒ render ra `any`, tức ÍT
        // thông tin hơn cả bản `String` trơn trước đó. Đo được: bỏ bước này làm
        // cổng G1 tụt từ 4/13 xuống 3/13.
        let spec = &giai_ref(schema, spec);
        let dau = if required.contains(&name.as_str()) {
            "*"
        } else {
            ""
        };
        // Từ vựng enum quan trọng hơn tên kiểu. Đo được ở cổng G1 (26/07/2026):
        // schema chỉ nói `device: string` thì gemma-4-E4B sinh `"air conditioner"`
        // và `"turn on"` — hợp lý với thông tin nó có, và sai với thứ tool nhận.
        // In enum ra là **chọn đúng 13/13 nhưng tham số sai 9/13** thành khớp.
        //
        // schemars đặt enum ở `enum`; schema viết tay có thể dùng `oneOf`/`const`,
        // nên đọc cả hai chỗ.
        if let Some(vals) = doc_enum(spec) {
            parts.push(format!("{name}{dau}: {}", vals.join("|")));
            continue;
        }
        let kieu = spec.get("type").and_then(Value::as_str).unwrap_or("any");
        parts.push(format!("{name}{dau}: {kieu}"));
    }
    parts.join(", ")
}

/// Giải `$ref` nội bộ (`#/definitions/X`, `#/$defs/X`) về node thật.
///
/// Chỉ giải MỘT tầng và chỉ ref nội bộ: đủ cho mọi thứ schemars sinh ra, và
/// không mở đường cho ref ngoài (`http://…`) — schema tới từ server MCP ngoài là
/// dữ liệu không tin được, không phải thứ để đi tải theo.
fn giai_ref(root: &Value, spec: &Value) -> Value {
    let Some(r) = spec.get("$ref").and_then(Value::as_str) else {
        return spec.clone();
    };
    for kho in ["definitions", "$defs"] {
        if let Some(ten) = r.strip_prefix(&format!("#/{kho}/"))
            && let Some(node) = root.get(kho).and_then(|d| d.get(ten))
        {
            return node.clone();
        }
    }
    spec.clone()
}

/// Rút danh sách giá trị hợp lệ từ một node schema.
///
/// Ba khuôn gặp thật: `enum: [...]` (schemars cho enum unit), `oneOf` gồm các
/// `const`/`enum` (schemars khi enum có mô tả từng nhánh), và `const: x`.
fn doc_enum(spec: &Value) -> Option<Vec<String>> {
    let nhu_chuoi = |v: &Value| -> Option<String> {
        match v {
            Value::String(s) => Some(format!("\"{s}\"")),
            Value::Number(n) => Some(n.to_string()),
            Value::Bool(b) => Some(b.to_string()),
            _ => None,
        }
    };

    if let Some(a) = spec.get("enum").and_then(Value::as_array) {
        let v: Vec<String> = a.iter().filter_map(nhu_chuoi).collect();
        if !v.is_empty() {
            return Some(v);
        }
    }
    if let Some(c) = spec.get("const").and_then(&nhu_chuoi) {
        return Some(vec![c]);
    }
    if let Some(a) = spec.get("oneOf").and_then(Value::as_array) {
        let mut v = Vec::new();
        for nhanh in a {
            if let Some(c) = nhanh.get("const").and_then(&nhu_chuoi) {
                v.push(c);
            } else if let Some(e) = nhanh.get("enum").and_then(Value::as_array) {
                v.extend(e.iter().filter_map(&nhu_chuoi));
            }
        }
        if !v.is_empty() {
            return Some(v);
        }
    }
    None
}

/// Dựng prompt chọn tool.
///
/// Hợp đồng output là hai dòng có tiền tố, và tool chọn bằng SỐ — xem ràng buộc
/// 2 ở đầu file về việc không có grammar. Đánh số bắt đầu từ 1 vì model nhỏ hay
/// lẫn với chỉ số 0.
pub fn render_selection_prompt(tools: &[&CatalogTool], user_text: &str) -> String {
    let mut s = String::with_capacity(512);
    s.push_str(
        "Bạn là bộ chọn công cụ. Đọc câu của người dùng rồi chọn ĐÚNG MỘT công cụ \
         phù hợp, hoặc NONE nếu câu đó chỉ là trò chuyện.\n\nCÔNG CỤ:\n",
    );
    for (i, t) in tools.iter().enumerate() {
        s.push_str(&format!("{}. {}", i + 1, t.name));
        if !t.description.is_empty() {
            s.push_str(&format!(" — {}", t.description));
        }
        s.push('\n');
        let params = render_params(&t.input_schema);
        if !params.is_empty() {
            s.push_str(&format!("{PARAM_PREFIX}{params}\n"));
        }
    }
    s.push_str(&format!("\nCÂU NGƯỜI DÙNG: {user_text}\n"));
    s.push_str(
        "\nTrả lời ĐÚNG hai dòng, không thêm lời nào khác:\n\
         TOOL: <số công cụ, hoặc NONE>\n\
         ARGS: <JSON các tham số, hoặc {} nếu NONE>\n",
    );
    s
}

/// Bọc prompt chọn tool vào **chat template của model**.
///
/// Bắt buộc, không phải trang trí: đo được 26/07/2026 — truyền prompt THÔ vào
/// `generate_completion` làm gemma-4-E4B trả về **chuỗi rỗng** cho cả 13 câu
/// trong corpus. Mọi caller khác trong crate (`chat_completion` ở `graph.rs`,
/// `chat:completion` và `task_plan_chat` ở `lib.rs`) đều đi qua `compile_prompt`;
/// bỏ qua nó là bỏ qua `<start_of_turn>`/`<|turn>` mà model instruction-tuned cần
/// để biết đến lượt nó nói.
///
/// Một message `user` là đủ cho cả hai họ template (Gemma và ChatML).
pub fn compile_selection_prompt(tools: &[&CatalogTool], user_text: &str) -> Result<String, String> {
    crate::llm::compile_prompt(&[crate::llm::ChatMessage {
        role: "user".to_string(),
        content: render_selection_prompt(tools, user_text),
    }])
}

// ── Đọc output của LLM ──────────────────────────────────────────────────────

/// Kết quả đọc output của LLM.
#[derive(Debug, Clone, PartialEq)]
pub enum Selection {
    /// LLM chọn tool thứ `index` (đã quy về 0-based trong danh sách đã đưa vào prompt).
    Tool { index: usize, arguments: Value },
    /// LLM nói không tool nào phù hợp — đây là câu trò chuyện.
    NoTool,
    /// Không đọc được. Caller phải rơi về `route_intent`, KHÔNG được đoán.
    Unreadable(String),
}

/// Quét khối `{...}` cân ngoặc đầu tiên, bỏ qua ngoặc nằm trong chuỗi.
///
/// Cần tự quét chứ không `find('{')..rfind('}')`: model hay nói thêm văn xuôi có
/// dấu ngoặc, và JSON có thể chứa `}` bên trong chuỗi (`{"path":"a}b"}`).
fn first_json_object(s: &str) -> Option<&str> {
    let bytes = s.as_bytes();
    let start = s.find('{')?;
    let mut depth = 0usize;
    let mut in_str = false;
    let mut escaped = false;
    for i in start..bytes.len() {
        let c = bytes[i] as char;
        if in_str {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(&s[start..=i]);
                }
            }
            _ => {}
        }
    }
    None
}

/// Đọc output của LLM một cách khoan dung.
///
/// Khoan dung là bắt buộc, không phải chiều lòng: model 2–4B không có grammar sẽ
/// thêm lời dẫn ("Chắc chắn rồi! TOOL: 1"), bọc trong ```, hoặc đổi thứ tự dòng.
/// Cái KHÔNG khoan dung: số ngoài phạm vi, và args không phải object — hai ca đó
/// thành [`Selection::Unreadable`] để caller rơi về `route_intent` thay vì chạy
/// bừa một tool.
pub fn parse_selection(raw: &str, so_tool: usize) -> Selection {
    let thap = raw.to_lowercase();

    let sau_tool = match thap.find("tool:") {
        Some(p) => &thap[p + 5..],
        // Không có tiền tố `TOOL:`: thử đọc như một dòng chỉ có số/NONE. Nếu
        // không ra gì thì báo không đọc được.
        None => &thap[..],
    };
    let dong = sau_tool.lines().next().unwrap_or("").trim();

    if dong.contains("none") {
        return Selection::NoTool;
    }

    let so: Option<usize> = dong
        .split(|c: char| !c.is_ascii_digit())
        .find(|s| !s.is_empty())
        .and_then(|s| s.parse().ok());

    let Some(n) = so else {
        return Selection::Unreadable(format!(
            "không thấy số công cụ hay NONE trong: {}",
            cat_gon(raw)
        ));
    };
    if n == 0 || n > so_tool {
        return Selection::Unreadable(format!(
            "số công cụ {n} ngoài phạm vi 1..={so_tool} (output: {})",
            cat_gon(raw)
        ));
    }

    // ARGS: tìm khối JSON đầu tiên SAU `args:` nếu có tiền tố, không thì tìm
    // trong cả output. Thiếu hẳn thì coi như không tham số — tool không tham số
    // là ca hợp lệ, và `validate_arguments` sẽ chặn nếu schema đòi trường.
    let vung = match raw.to_lowercase().find("args:") {
        Some(p) => &raw[p + 5..],
        None => raw,
    };
    let arguments = match first_json_object(vung) {
        Some(khoi) => match serde_json::from_str::<Value>(khoi) {
            Ok(v) if v.is_object() => v,
            Ok(_) => return Selection::Unreadable("ARGS không phải object JSON".to_string()),
            Err(e) => {
                return Selection::Unreadable(format!("ARGS không phải JSON hợp lệ: {e}"));
            }
        },
        None => json!({}),
    };

    Selection::Tool {
        index: n - 1,
        arguments,
    }
}

fn cat_gon(s: &str) -> String {
    let t = s.trim().replace('\n', " ⏎ ");
    if t.chars().count() <= 120 {
        t
    } else {
        t.chars().take(120).collect::<String>() + "…"
    }
}

/// Kiểm tham số theo JSON Schema — ở mức tối thiểu nhưng đủ chặn hai ca thật.
///
/// KHÔNG phải bộ kiểm JSON Schema đầy đủ, và cố ý không phải: mục đích duy nhất
/// là chặn lời gọi rác trước khi nó tới tool. Hai thứ được kiểm:
///
/// 1. **Trường `required` phải có mặt.** Đây là ca hay xảy ra nhất: model chọn
///    đúng tool rồi bỏ trống tham số.
/// 2. **Kiểu nguyên thuỷ phải khớp** (`string`/number/`boolean`/array/object).
///
/// Trường lạ được BỎ QUA, không phải lỗi — serde ở phía tool cũng bỏ qua chúng,
/// nên chặn ở đây sẽ nghiêm hơn chính tool.
pub fn validate_arguments(schema: &Value, args: &Value) -> Result<(), String> {
    let Some(obj) = args.as_object() else {
        return Err("tham số phải là object JSON".to_string());
    };

    if let Some(required) = schema.get("required").and_then(Value::as_array) {
        let thieu: Vec<&str> = required
            .iter()
            .filter_map(Value::as_str)
            .filter(|k| !obj.contains_key(*k))
            .collect();
        if !thieu.is_empty() {
            return Err(format!("thiếu tham số bắt buộc: {}", thieu.join(", ")));
        }

        // Chuỗi RỖNG ở trường bắt buộc = model đang ĐOÁN, không phải gọi thật.
        //
        // Đo được 26/07/2026 khi catalog tăng từ 4 lên 6 tool: với câu "kể cho
        // mình một chuyện vui", Qwen3-VL-2B chọn một tool đọc file kèm
        // `{"path": ""}` thay vì trả NONE. Cổng G1 tụt 13/13 → 12/13. Bản kiểm cũ
        // cho qua vì `""` vừa CÓ MẶT vừa ĐÚNG KIỂU `string`.
        //
        // Chỉ áp cho trường BẮT BUỘC: chuỗi rỗng ở trường tuỳ chọn là quyền của
        // caller, còn ở trường bắt buộc thì không có nghĩa nào dùng được
        // (`path: ""`, `query: ""`).
        let rong: Vec<&str> = required
            .iter()
            .filter_map(Value::as_str)
            .filter(|k| {
                obj.get(*k)
                    .and_then(Value::as_str)
                    .is_some_and(|s| s.trim().is_empty())
            })
            .collect();
        if !rong.is_empty() {
            return Err(format!(
                "tham số bắt buộc rỗng (model đang đoán?): {}",
                rong.join(", ")
            ));
        }
    }

    if let Some(props) = schema.get("properties").and_then(Value::as_object) {
        for (name, gt) in obj {
            let Some(kieu) = props
                .get(name)
                .and_then(|s| s.get("type"))
                .and_then(Value::as_str)
            else {
                continue; // trường lạ, hoặc schema không nói kiểu
            };
            let khop = match kieu {
                "string" => gt.is_string(),
                "integer" => gt.is_i64() || gt.is_u64(),
                "number" => gt.is_number(),
                "boolean" => gt.is_boolean(),
                "array" => gt.is_array(),
                "object" => gt.is_object(),
                "null" => gt.is_null(),
                _ => true,
            };
            if !khop {
                return Err(format!(
                    "tham số '{name}' phải là {kieu}, nhận được {}",
                    loai_json(gt)
                ));
            }
        }
    }
    Ok(())
}

fn loai_json(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

// ── Được phép chạy hay chỉ được đề xuất ─────────────────────────────────────

/// Tool này được tự chạy, hay chỉ được đề xuất cho người dùng?
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecPolicy {
    /// Chạy ngay.
    Auto,
    /// Chỉ trả về đề xuất; người dùng (hoặc một cổng đồng ý) quyết định.
    ProposeOnly,
}

/// Tool nội bộ được auto-exec theo mặc định.
///
/// Danh sách là **allowlist**, không phải blocklist — thứ chưa biết thì mặc định
/// KHÔNG được chạy. `write_markdown` cố ý KHÔNG có trong đây dù nó là tool nội
/// bộ: nó GHI file, và một lời gọi ghi do prompt injection lái là thiệt hại
/// không hoàn lại được. `control_smarthome` có, để ngang bằng đúng những gì
/// `route_intent` vốn đã tự chạy.
///
/// `control_volume` và `control_media` (U19) đủ điều kiện vì **hoàn tác được
/// bằng đúng một lệnh ngược lại**: tăng ↔ giảm, còn mute và play/pause là công
/// tắc. Kịch bản xấu nhất khi prompt injection lái được chúng là loa to lên
/// hoặc nhạc dừng — khó chịu, không mất mát. Đó là ranh giới cho danh sách này:
/// **đảo ngược được thì cho tự chạy, không đảo ngược được thì phải hỏi.**
const NATIVE_AUTOEXEC: &[&str] = &[
    "read_markdown",
    "search_vault",
    "control_smarthome",
    "control_volume",
    "control_media",
];

impl ExecPolicy {
    /// Quyết định cho một tool cụ thể.
    ///
    /// Vì sao chọn/chạy là hai bước tách rời: §4 tài liệu 04 —
    /// *"nội dung skill là dữ liệu, không phải lệnh; `SKILL.md` tải về không được
    /// phép lái vòng tool-calling ở G1"*. LLM đọc văn bản từ người dùng, từ RAG,
    /// và (từ G0) từ output của server MCP ngoài. Mọi nguồn đó đều có thể chứa câu
    /// giả dạng chỉ thị. Nên **chọn** tool là gợi ý, **chạy** tool cần allowlist.
    ///
    /// `LIVA_MCP_AUTOEXEC` mở thêm bằng danh sách phẩy: `server/tool` cho từng
    /// tool, hoặc `server/*` cho cả server. Mặc định rỗng ⇒ mọi tool ngoài chỉ
    /// được đề xuất.
    pub fn for_tool(server: &str, name: &str) -> Self {
        if server == NATIVE_SERVER && NATIVE_AUTOEXEC.contains(&name) {
            return Self::Auto;
        }
        let Ok(raw) = std::env::var("LIVA_MCP_AUTOEXEC") else {
            return Self::ProposeOnly;
        };
        let khop = raw
            .split(',')
            .map(str::trim)
            .any(|muc| muc == format!("{server}/{name}") || muc == format!("{server}/*"));
        if khop { Self::Auto } else { Self::ProposeOnly }
    }
}

// ── Vòng đầy đủ, có chạm AppState ───────────────────────────────────────────

/// Một lời gọi tool đã chọn xong và đã kiểm tham số.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedCall {
    pub server: String,
    pub name: String,
    pub arguments: Value,
    pub policy: ExecPolicy,
}

fn tool_start_event(call: &ResolvedCall) -> Value {
    let label = match call.name.as_str() {
        "get_weather" => "Đang xem thời tiết…".to_string(),
        name => format!("Đang dùng {name}…"),
    };
    json!({
        "event": "tool_start",
        "payload": {
            "tool": call.name,
            "label": label,
        }
    })
}

fn tool_result_event(
    call: &ResolvedCall,
    result: &Result<crate::mcp::protocol::CallToolResult, String>,
) -> Value {
    match result {
        Ok(value) if !value.is_error => json!({
            "event": "tool_result",
            "payload": {
                "tool": call.name,
                "ok": true,
                "data": value,
            }
        }),
        Ok(value) => {
            let reason = value
                .content
                .iter()
                .find_map(|content| match content {
                    crate::mcp::protocol::ToolContent::Text { text } if !text.trim().is_empty() => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .unwrap_or("Công cụ không hoàn tất.");
            json!({
                "event": "tool_result",
                "payload": {
                    "tool": call.name,
                    "ok": false,
                    "reason": reason,
                }
            })
        }
        Err(reason) => json!({
            "event": "tool_result",
            "payload": {
                "tool": call.name,
                "ok": false,
                "reason": reason,
            }
        }),
    }
}

async fn send_tool_event(sender: Option<&tokio::sync::mpsc::Sender<String>>, event: Value) {
    let Some(sender) = sender else {
        return;
    };
    if let Ok(serialized) = serde_json::to_string(&event) {
        let _ = sender.send(serialized).await;
    }
}

async fn execute_with_events<F>(
    call: &ResolvedCall,
    sender: Option<&tokio::sync::mpsc::Sender<String>>,
    execution: F,
) -> Result<crate::mcp::protocol::CallToolResult, String>
where
    F: std::future::Future<Output = Result<crate::mcp::protocol::CallToolResult, String>>,
{
    send_tool_event(sender, tool_start_event(call)).await;
    let result = execution.await;
    send_tool_event(sender, tool_result_event(call, &result)).await;
    result
}

/// Vòng tool-calling có bật hay không. **Mặc định TẮT.**
///
/// Vì sao tắt: nó thêm MỘT lượt LLM nữa cho mỗi câu chat. Trên máy beta chạy
/// model 2–4B đó là thêm giây chờ thật cho *mọi* lượt nói, kể cả "hôm nay thế
/// nào" — tức làm trợ lý thoại tệ đi để đổi lấy một năng lực chưa ai gọi tới.
/// `route_intent` vẫn phủ đúng những năng lực đang có.
///
/// Điều kiện để bật mặc định: cổng "đường keyword và đường LLM khớp nhau trên
/// corpus smart-home" phải xanh với model thật (xem tài liệu 04 §3 G1).
pub fn enabled() -> bool {
    crate::env_flag("LIVA_TOOL_CALLING", false)
}

/// Server MCP ngoài được đưa vào catalog, theo `LIVA_TOOL_CALLING_SERVERS`.
///
/// Mặc định RỖNG ⇒ catalog chỉ có tool nội bộ. Không phải vì tool ngoài kém, mà
/// vì lấy `tools/list` của một server ngoài **spawn một tiến trình con** — làm
/// việc đó ngầm ở mỗi lượt chat là hành vi không ai mong đợi.
fn external_servers() -> Vec<String> {
    std::env::var("LIVA_TOOL_CALLING_SERVERS")
        .ok()
        .map(|raw| {
            raw.split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

/// Dựng catalog từ trạng thái ứng dụng: tool nội bộ + các server ngoài đã bật.
pub async fn build_catalog(state: &crate::AppState) -> ToolCatalog {
    let mut catalog = ToolCatalog::new();
    catalog.add_server(NATIVE_SERVER, &state.mcp_server.list_tools().tools);
    // Ví dụ cách nói cho 4 tool nội bộ — chỉ vào embedding, không vào prompt.
    for (ten, vi_du) in crate::mcp::server::NativeMcpServer::retrieval_examples() {
        catalog.set_embed_extra(NATIVE_SERVER, ten, vi_du);
    }

    for ten in external_servers() {
        match crate::mcp::client::global_registry().list_tools(&ten).await {
            Ok(list) => catalog.add_server(&ten, &list.tools),
            Err(e) => tracing::warn!("không lấy được tool của MCP server '{ten}': {e}"),
        }
    }
    catalog
}

/// Chạy trọn vòng: catalog → top-k → prompt → LLM → parse → kiểm tham số.
///
/// `None` nghĩa là "không có tool nào phù hợp" — caller phải xử đúng như trước
/// khi có G1, tức đi tiếp sang `chat_completion`. Mọi lỗi trên đường đi (LLM
/// hỏng, output không đọc được, tham số sai) đều thành `None` kèm log: rơi về
/// hành vi cũ luôn tốt hơn là chạy bừa một tool.
pub async fn select_tool(state: &crate::AppState, user_text: &str) -> Option<ResolvedCall> {
    if !enabled() {
        return None;
    }
    let catalog = build_catalog(state).await;
    if catalog.is_empty() {
        return None;
    }

    let top: Vec<usize> = {
        let mut guard = state.embedder.lock().await;
        match guard.as_mut() {
            Some(e) => rank_tools(&catalog, user_text, Some(e), DEFAULT_TOP_K),
            None => rank_tools(&catalog, user_text, None, DEFAULT_TOP_K),
        }
    };
    let ung_vien: Vec<&CatalogTool> = top.iter().map(|&i| &catalog.tools()[i]).collect();
    if ung_vien.is_empty() {
        return None;
    }

    let prompt = match compile_selection_prompt(&ung_vien, user_text) {
        Ok(p) => p,
        Err(e) => {
            tracing::warn!("chọn tool: không dựng được prompt ({e}); rơi về route_intent");
            return None;
        }
    };
    let raw = {
        let mut llm = state.llm.lock().await;
        // temperature 0 + top_p 1: đây là quyết định phân loại, không phải sáng
        // tác. Sampling ngẫu nhiên ở đây chỉ tạo ra kết quả không lặp lại được.
        match llm.generate_completion(&prompt, 0.0, 1.0, |_| true) {
            Ok(out) => out.text,
            Err(e) => {
                tracing::warn!("chọn tool: LLM lỗi ({e}); rơi về route_intent");
                return None;
            }
        }
    };

    match parse_selection(&raw, ung_vien.len()) {
        Selection::NoTool => None,
        Selection::Unreadable(ly) => {
            tracing::warn!("chọn tool: không đọc được output ({ly}); rơi về route_intent");
            None
        }
        Selection::Tool { index, arguments } => {
            let chon = ung_vien[index];
            if let Err(e) = validate_arguments(&chon.input_schema, &arguments) {
                tracing::warn!(
                    "chọn tool: {} có tham số không hợp lệ ({e}); rơi về route_intent",
                    chon.qualified()
                );
                return None;
            }
            let policy = ExecPolicy::for_tool(&chon.server, &chon.name);
            tracing::info!(
                "chọn tool: {} — {}",
                chon.qualified(),
                match policy {
                    ExecPolicy::Auto => "tự chạy",
                    ExecPolicy::ProposeOnly => "chỉ đề xuất",
                }
            );
            Some(ResolvedCall {
                server: chon.server.clone(),
                name: chon.name.clone(),
                arguments,
                policy,
            })
        }
    }
}

/// Cửa kiểm cho các lệnh IPC gọi tool **trực tiếp** (`mcp:call_tool`,
/// `mcp_client:call_tool`).
///
/// # Vì sao cần, dù [`execute_call`] đã kiểm
///
/// [`execute_call`] chỉ nằm trên đường G1 — nơi **LLM** chọn tool. Hai lệnh IPC
/// kia gọi thẳng `NativeMcpServer` / `McpClientRegistry` và **không** đi qua nó.
/// Nên tới 26/07/2026, bất kỳ client nào nối được vào lớp lệnh đều gọi được:
///
/// - `write_markdown` (ghi file trong vault), và
/// - **mọi tool trên mọi server MCP ngoài** khai trong `mcp_config.json` — tức
///   tiến trình `npx`/`docker` của người lạ, với đúng quyền mà chúng có,
///
/// bất kể `LIVA_TOOL_CALLING` bật hay tắt. Phát hiện này không phải của tôi:
/// xem `docs/03-danh-gia/02-no-ky-thuat-va-rui-ro.md` §C1.1.
///
/// Và "client nối được vào lớp lệnh" là hàng rào **mỏng**: WS 8002 chưa có xác
/// thực, chỉ có allow-list `Origin` (§C1 cùng tài liệu).
///
/// # Đây KHÔNG phải bản vá đủ
///
/// Nó chỉ đóng hai lệnh MCP. Các lệnh khác trên cùng đường không xác thực đó vẫn
/// mở (`llm:swap_model` là §C2). Bản vá đúng là **allow-list lệnh theo kênh** —
/// đề xuất (3) ở §C1, vẫn chưa làm.
pub fn guard_direct_call(server: &str, name: &str) -> Result<(), String> {
    match ExecPolicy::for_tool(server, name) {
        ExecPolicy::Auto => Ok(()),
        ExecPolicy::ProposeOnly => Err(format!(
            "tool '{server}/{name}' không nằm trong allowlist tự chạy, nên lớp lệnh từ chối gọi \
             nó. Nếu đây là caller hợp pháp và bạn thật sự muốn cho phép, đặt \
             LIVA_MCP_AUTOEXEC={server}/{name} (hoặc {server}/* cho cả server)."
        )),
    }
}

/// Chạy một lời gọi đã được [`select_tool`] chấp thuận.
///
/// Cửa an toàn cuối: kiểm lại `policy` ngay tại đây thay vì tin caller. Hai lớp
/// kiểm là cố ý — cửa duy nhất là cửa sẽ bị quên khi thêm caller thứ hai.
pub async fn execute_call(
    state: &crate::AppState,
    call: &ResolvedCall,
    event_sender: Option<&tokio::sync::mpsc::Sender<String>>,
) -> Result<crate::mcp::protocol::CallToolResult, String> {
    if ExecPolicy::for_tool(&call.server, &call.name) != ExecPolicy::Auto {
        return Err(format!(
            "{} không nằm trong allowlist tự chạy (đặt LIVA_MCP_AUTOEXEC nếu thật sự muốn)",
            call.qualified()
        ));
    }
    let req = crate::mcp::protocol::CallToolRequest {
        name: call.name.clone(),
        arguments: call.arguments.clone(),
    };
    let execution = async {
        if call.server == NATIVE_SERVER {
            state.mcp_server.call_tool(req).await
        } else {
            crate::mcp::client::global_registry()
                .call_tool(&call.server, req)
                .await
        }
    };
    execute_with_events(call, event_sender, execution).await
}

impl ResolvedCall {
    pub fn qualified(&self) -> String {
        format!("{}/{}", self.server, self.name)
    }

    /// Câu mô tả lời gọi để đưa vào hội thoại khi tool chỉ được **đề xuất**.
    ///
    /// Nói rõ là CHƯA chạy. Báo "đã làm" khi chưa làm là đúng thứ commit
    /// `2fb27c1` đã sửa ở `smart_home` ("thành công giả").
    pub fn proposal_text(&self) -> String {
        format!(
            "Tôi CHƯA chạy gì cả. Việc phù hợp có thể là gọi công cụ `{}` với tham số {} — \
             nhưng công cụ này không nằm trong danh sách được tự chạy, nên cần bạn xác nhận.",
            self.qualified(),
            self.arguments
        )
    }
}

// ── Test ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_start_event_dung_khuon_websocket_hien_co() {
        let call = ResolvedCall {
            server: NATIVE_SERVER.to_string(),
            name: "get_weather".to_string(),
            arguments: json!({ "location": "Hà Nội" }),
            policy: ExecPolicy::Auto,
        };

        assert_eq!(
            tool_start_event(&call),
            json!({
                "event": "tool_start",
                "payload": {
                    "tool": "get_weather",
                    "label": "Đang xem thời tiết…",
                }
            })
        );
    }

    #[test]
    fn tool_result_event_mang_du_lieu_that_khi_thanh_cong() {
        let call = ResolvedCall {
            server: NATIVE_SERVER.to_string(),
            name: "get_weather".to_string(),
            arguments: json!({ "location": "Hà Nội" }),
            policy: ExecPolicy::Auto,
        };
        let result = crate::mcp::protocol::CallToolResult {
            content: vec![crate::mcp::protocol::ToolContent::Text {
                text: "Hà Nội: 31°C, có mây, độ ẩm 70%.".to_string(),
            }],
            is_error: false,
        };

        assert_eq!(
            tool_result_event(&call, &Ok(result)),
            json!({
                "event": "tool_result",
                "payload": {
                    "tool": "get_weather",
                    "ok": true,
                    "data": {
                        "content": [{
                            "type": "text",
                            "text": "Hà Nội: 31°C, có mây, độ ẩm 70%."
                        }],
                        "isError": false,
                    }
                }
            })
        );
    }

    #[test]
    fn tool_result_event_bao_ly_do_khi_tool_tra_is_error() {
        let call = ResolvedCall {
            server: NATIVE_SERVER.to_string(),
            name: "get_weather".to_string(),
            arguments: json!({ "location": "Hà Nội" }),
            policy: ExecPolicy::Auto,
        };
        let result = crate::mcp::protocol::CallToolResult {
            content: vec![crate::mcp::protocol::ToolContent::Text {
                text: "Không lấy được thời tiết vì mất kết nối.".to_string(),
            }],
            is_error: true,
        };

        assert_eq!(
            tool_result_event(&call, &Ok(result)),
            json!({
                "event": "tool_result",
                "payload": {
                    "tool": "get_weather",
                    "ok": false,
                    "reason": "Không lấy được thời tiết vì mất kết nối.",
                }
            })
        );
    }

    #[test]
    fn tool_result_event_bao_ly_do_khi_loi_thuc_thi() {
        let call = ResolvedCall {
            server: NATIVE_SERVER.to_string(),
            name: "get_weather".to_string(),
            arguments: json!({ "location": "Hà Nội" }),
            policy: ExecPolicy::Auto,
        };

        assert_eq!(
            tool_result_event(&call, &Err("MCP transport đã đóng.".to_string())),
            json!({
                "event": "tool_result",
                "payload": {
                    "tool": "get_weather",
                    "ok": false,
                    "reason": "MCP transport đã đóng.",
                }
            })
        );
    }

    #[tokio::test]
    async fn tool_event_duoc_serialize_vao_kenh_text_websocket() {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        let event = json!({
            "event": "tool_start",
            "payload": { "tool": "get_weather", "label": "Đang xem thời tiết…" }
        });

        send_tool_event(Some(&tx), event.clone()).await;

        let sent = rx.recv().await.expect("phải có sự kiện WebSocket");
        assert_eq!(serde_json::from_str::<Value>(&sent).unwrap(), event);
    }

    #[tokio::test]
    async fn thuc_thi_tool_phat_start_truoc_result() {
        let call = ResolvedCall {
            server: NATIVE_SERVER.to_string(),
            name: "get_weather".to_string(),
            arguments: json!({ "location": "Hà Nội" }),
            policy: ExecPolicy::Auto,
        };
        let result = crate::mcp::protocol::CallToolResult {
            content: vec![crate::mcp::protocol::ToolContent::Text {
                text: "Hà Nội: 31°C, có mây.".to_string(),
            }],
            is_error: false,
        };
        let (tx, mut rx) = tokio::sync::mpsc::channel(2);

        let returned = execute_with_events(&call, Some(&tx), async { Ok(result.clone()) })
            .await
            .unwrap();

        assert_eq!(
            serde_json::to_value(returned).unwrap(),
            serde_json::to_value(result).unwrap()
        );
        let start: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        let finish: Value = serde_json::from_str(&rx.recv().await.unwrap()).unwrap();
        assert_eq!(start["event"], "tool_start");
        assert_eq!(finish["event"], "tool_result");
        assert_eq!(finish["payload"]["ok"], true);
    }

    fn tool(server: &str, name: &str, desc: &str, schema: Value) -> CatalogTool {
        CatalogTool {
            server: server.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            input_schema: schema,
            embed_extra: String::new(),
        }
    }

    fn catalog_smarthome() -> ToolCatalog {
        let mut c = ToolCatalog::new();
        c.tools.push(tool(
            NATIVE_SERVER,
            "control_smarthome",
            "Control a smart home device",
            json!({
                "type": "object",
                "required": ["device", "command"],
                "properties": {
                    "device": { "type": "string" },
                    "command": { "type": "string" }
                }
            }),
        ));
        c.tools.push(tool(
            NATIVE_SERVER,
            "read_markdown",
            "Read a markdown file from the vault",
            json!({
                "type": "object",
                "required": ["path"],
                "properties": { "path": { "type": "string" } }
            }),
        ));
        c.tools
            .push(tool(NATIVE_SERVER, "khong_tham_so", "Tool rỗng", json!({})));
        c
    }

    // ---- render tham số: chỗ quyết định ngân sách context ----

    #[test]
    fn render_tham_so_gon_va_danh_dau_bat_buoc() {
        let c = catalog_smarthome();
        let s = render_params(&c.tools()[0].input_schema);
        assert!(s.contains("device*: string"), "nhận được: {s}");
        assert!(s.contains("command*: string"), "nhận được: {s}");
        assert!(
            s.len() < 60,
            "phải GỌN — cả điểm của nó là không đốt context: {s}"
        );
    }

    #[test]
    fn tool_khong_tham_so_thi_khong_in_dong_tham_so() {
        let c = catalog_smarthome();
        assert_eq!(render_params(&c.tools()[2].input_schema), "");
        let ds: Vec<&CatalogTool> = vec![&c.tools()[2]];
        let p = render_selection_prompt(&ds, "làm gì đi");
        // Kiểm ĐÚNG dòng tham số qua tiền tố, không tìm chuỗi con "tham số":
        // cụm đó cũng nằm trong dòng hợp đồng output ở cuối prompt, nên bản đầu
        // của test này trượt dù code render hoàn toàn đúng.
        assert!(
            !p.contains(PARAM_PREFIX),
            "không nên có dòng tham số rỗng:\n{p}"
        );
        // Và tool CÓ tham số thì phải có đúng dòng đó.
        let ds2: Vec<&CatalogTool> = vec![&c.tools()[0]];
        assert!(render_selection_prompt(&ds2, "bật đèn").contains(PARAM_PREFIX));
    }

    /// Hồi quy của ca làm cổng G1 tụt 4/13 → 3/13.
    ///
    /// schemars đặt MỌI kiểu có tên (enum, struct lồng) vào `definitions` và để
    /// `properties` chỉ có `$ref`. Không giải ref thì không thấy `type` lẫn
    /// `enum` ⇒ render ra `any`, tức ÍT thông tin hơn cả `String` trơn. Đây là
    /// khuôn schema THẬT mà `schema_for!(ControlSmartHomeArgs)` sinh ra.
    #[test]
    fn giai_duoc_ref_cua_schemars_de_lay_tu_vung_enum() {
        let schema = json!({
            "type": "object",
            "required": ["action", "device"],
            "properties": {
                "action": { "$ref": "#/definitions/SmartHomeAction" },
                "device": { "$ref": "#/definitions/SmartHomeDevice" }
            },
            "definitions": {
                "SmartHomeAction": { "type": "string", "enum": ["on", "off"] },
                "SmartHomeDevice": { "type": "string", "enum": ["light", "ac", "fan"] }
            }
        });
        let s = render_params(&schema);
        assert!(
            s.contains(r#"action*: "on"|"off""#),
            "phải in TỪ VỰNG, không phải `any`: {s}"
        );
        assert!(s.contains(r#"device*: "light"|"ac"|"fan""#), "{s}");
        assert!(
            !s.contains("any"),
            "còn `any` nghĩa là ref chưa được giải: {s}"
        );
    }

    /// `$defs` là tên mới của `definitions` (JSON Schema 2019-09+); server MCP
    /// ngoài có thể dùng khuôn đó.
    #[test]
    fn giai_duoc_ca_defs_va_bo_qua_ref_ngoai() {
        let s = render_params(&json!({
            "type": "object",
            "properties": { "x": { "$ref": "#/$defs/X" } },
            "$defs": { "X": { "type": "string", "enum": ["a"] } }
        }));
        assert!(s.contains(r#"x: "a""#), "{s}");

        // Ref ngoài KHÔNG được đi tải theo — schema từ server ngoài là dữ liệu
        // không tin được. Không giải được thì render `any`, không phải panic.
        let s = render_params(&json!({
            "type": "object",
            "properties": { "x": { "$ref": "https://evil.example/schema.json" } }
        }));
        assert_eq!(s, "x: any");
    }

    #[test]
    fn doc_duoc_enum_kieu_oneof_va_const() {
        let s = render_params(&json!({
            "type": "object",
            "properties": {
                "a": { "oneOf": [ { "const": "x" }, { "const": "y" } ] },
                "b": { "const": 7 }
            }
        }));
        assert!(s.contains(r#"a: "x"|"y""#), "{s}");
        assert!(s.contains("b: 7"), "{s}");
    }

    /// Prompt phải nêu rõ hợp đồng output, vì không có grammar để ép.
    #[test]
    fn prompt_neu_ro_hop_dong_output() {
        let c = catalog_smarthome();
        let ds: Vec<&CatalogTool> = c.tools().iter().collect();
        let p = render_selection_prompt(&ds, "bật đèn");
        assert!(p.contains("TOOL:") && p.contains("ARGS:"));
        assert!(p.contains("NONE"), "phải cho model đường thoát");
        assert!(p.contains("1. control_smarthome"), "đánh số từ 1:\n{p}");
        assert!(p.contains("bật đèn"));
    }

    // ---- embed_extra: vào embedding, KHÔNG vào prompt ----

    /// Đây là bất biến giữ cả thiết kế đứng được. Nếu `embed_extra` lọt vào
    /// prompt thì ta quay về đúng bản đã đo là 3939 ms (so với 2501 ms) — tức
    /// mất sạch lý do tách trường ra.
    #[test]
    fn embed_extra_vao_embedding_nhung_khong_vao_prompt() {
        let mut c = catalog_smarthome();
        c.set_embed_extra(NATIVE_SERVER, "control_smarthome", "bật đèn · tắt máy lạnh");

        let t = c.find(NATIVE_SERVER, "control_smarthome").expect("có tool");
        assert!(
            t.embed_text().contains("bật đèn"),
            "phải vào chuỗi embed: {}",
            t.embed_text()
        );

        let ds: Vec<&CatalogTool> = c.tools().iter().collect();
        let p = render_selection_prompt(&ds, "bật đèn");
        assert!(
            !p.contains("tắt máy lạnh"),
            "embed_extra KHÔNG được vào prompt — cả điểm của nó là không đốt token:\n{p}"
        );
    }

    #[test]
    fn set_embed_extra_ten_la_thi_bo_qua_khong_panic() {
        let mut c = catalog_smarthome();
        c.set_embed_extra(NATIVE_SERVER, "khong-ton-tai", "x");
        c.set_embed_extra("server-la", "control_smarthome", "x");
        assert!(
            c.find(NATIVE_SERVER, "control_smarthome")
                .expect("có tool")
                .embed_extra
                .is_empty(),
            "gắn sai server/tên không được ghi bừa vào tool khác"
        );
    }

    /// Tool từ server MCP ngoài không có `embed_extra` ⇒ hành xử y như trước.
    #[test]
    fn khong_co_embed_extra_thi_embed_text_nhu_cu() {
        let c = catalog_smarthome();
        let t = &c.tools()[0];
        assert_eq!(
            t.embed_text(),
            format!("{}: {}", t.name, t.description),
            "không có extra thì đúng bằng `name: description`"
        );
    }

    // ---- đọc output: nơi model 2-4B sẽ làm mọi thứ trừ điều được yêu cầu ----

    #[test]
    fn doc_duoc_output_dung_khuon() {
        let s = parse_selection(
            "TOOL: 1\nARGS: {\"device\":\"light\",\"command\":\"on\"}",
            3,
        );
        assert_eq!(
            s,
            Selection::Tool {
                index: 0,
                arguments: json!({ "device": "light", "command": "on" })
            }
        );
    }

    /// Đây là output THỰC TẾ của model nhỏ: lời dẫn, khối ```, thứ tự lộn.
    #[test]
    fn khoan_dung_voi_output_lam_nham() {
        for (ten, raw) in [
            (
                "có lời dẫn",
                "Chắc chắn rồi!\nTOOL: 1\nARGS: {\"device\":\"light\",\"command\":\"on\"}",
            ),
            (
                "bọc code fence",
                "```\nTOOL: 1\nARGS: {\"device\":\"light\",\"command\":\"on\"}\n```",
            ),
            (
                "chữ thường",
                "tool: 1\nargs: {\"device\":\"light\",\"command\":\"on\"}",
            ),
            (
                "nói thêm sau",
                "TOOL: 1\nARGS: {\"device\":\"light\",\"command\":\"on\"}\nHy vọng giúp được bạn!",
            ),
        ] {
            match parse_selection(raw, 3) {
                Selection::Tool { index, arguments } => {
                    assert_eq!(index, 0, "{ten}");
                    assert_eq!(arguments["device"], json!("light"), "{ten}");
                }
                khac => panic!("{ten}: phải đọc được, nhận được {khac:?}"),
            }
        }
    }

    #[test]
    fn none_la_cau_tro_chuyen() {
        for raw in ["TOOL: NONE\nARGS: {}", "tool: none", "TOOL:NONE"] {
            assert_eq!(parse_selection(raw, 3), Selection::NoTool, "{raw:?}");
        }
    }

    /// Hai ca KHÔNG được khoan dung — chạy bừa một tool tệ hơn là bỏ qua.
    #[test]
    fn so_ngoai_pham_vi_va_args_hong_thi_bao_khong_doc_duoc() {
        assert!(matches!(
            parse_selection("TOOL: 9\nARGS: {}", 3),
            Selection::Unreadable(_)
        ));
        assert!(matches!(
            parse_selection("TOOL: 0\nARGS: {}", 3),
            Selection::Unreadable(_)
        ));
        assert!(matches!(
            parse_selection("TOOL: 1\nARGS: {device: light}", 3),
            Selection::Unreadable(_)
        ));
        assert!(
            matches!(
                parse_selection("tôi không biết", 3),
                Selection::Unreadable(_)
            ),
            "output rác phải thành Unreadable để caller rơi về route_intent"
        );
    }

    /// `}` bên trong chuỗi từng làm hỏng kiểu quét `find('{')..rfind('}')`.
    #[test]
    fn quet_ngoac_can_bo_qua_ngoac_trong_chuoi() {
        let s = parse_selection(r#"TOOL: 2 ARGS: {"path":"ghi-chu/a}b.md"} xong"#, 3);
        match s {
            Selection::Tool { index, arguments } => {
                assert_eq!(index, 1);
                assert_eq!(arguments["path"], json!("ghi-chu/a}b.md"));
            }
            khac => panic!("phải đọc được, nhận được {khac:?}"),
        }
    }

    #[test]
    fn thieu_args_thi_coi_nhu_rong_chu_khong_hong() {
        assert_eq!(
            parse_selection("TOOL: 3", 3),
            Selection::Tool {
                index: 2,
                arguments: json!({})
            }
        );
    }

    // ---- kiểm tham số ----

    #[test]
    fn thieu_truong_bat_buoc_thi_tu_choi() {
        let c = catalog_smarthome();
        let s = &c.tools()[0].input_schema;
        let e = validate_arguments(s, &json!({ "device": "light" })).expect_err("phải lỗi");
        assert!(e.contains("command"), "lỗi phải nói thiếu gì: {e}");
        assert!(validate_arguments(s, &json!({ "device": "light", "command": "on" })).is_ok());
    }

    /// Hồi quy của ca làm cổng G1 tụt 13/13 → 12/13 khi catalog tăng 4 → 6 tool:
    /// model chọn một tool đọc file kèm `{"path": ""}` cho câu "kể cho mình một
    /// chuyện vui". `""` vừa CÓ MẶT vừa đúng kiểu `string` nên bản kiểm cũ cho qua.
    #[test]
    fn chuoi_rong_o_truong_bat_buoc_bi_tu_choi() {
        let s = json!({
            "type": "object",
            "required": ["path"],
            "properties": { "path": { "type": "string" } }
        });
        for xau in ["", "   ", "\t\n"] {
            let e = validate_arguments(&s, &json!({ "path": xau }))
                .expect_err(&format!("{xau:?} phải bị từ chối"));
            assert!(e.contains("rỗng"), "lỗi phải nói rõ là rỗng: {e}");
        }
        assert!(validate_arguments(&s, &json!({ "path": "a.md" })).is_ok());
    }

    /// Nhưng chuỗi rỗng ở trường TUỲ CHỌN là quyền của caller.
    #[test]
    fn chuoi_rong_o_truong_tuy_chon_van_duoc() {
        let s = json!({
            "type": "object",
            "required": ["path"],
            "properties": {
                "path": { "type": "string" },
                "ghi_chu": { "type": "string" }
            }
        });
        assert!(validate_arguments(&s, &json!({ "path": "a.md", "ghi_chu": "" })).is_ok());
    }

    #[test]
    fn kieu_sai_thi_tu_choi_nhung_truong_la_thi_bo_qua() {
        let c = catalog_smarthome();
        let s = &c.tools()[0].input_schema;
        assert!(
            validate_arguments(s, &json!({ "device": 1, "command": "on" })).is_err(),
            "device là number → phải từ chối"
        );
        assert!(
            validate_arguments(
                s,
                &json!({ "device": "light", "command": "on", "la": true })
            )
            .is_ok(),
            "trường lạ phải được bỏ qua — serde phía tool cũng bỏ qua"
        );
    }

    #[test]
    fn args_khong_phai_object_thi_tu_choi() {
        assert!(validate_arguments(&json!({}), &json!("light")).is_err());
        assert!(validate_arguments(&json!({}), &json!([1, 2])).is_err());
    }

    // ---- xếp hạng ----

    /// Không có embedder là ca THẬT (`AppState.embedder` là `Option`, weight bị
    /// gitignore). Đường trùng token phải vẫn xếp đúng cho câu rõ ràng.
    #[test]
    fn khong_co_embedder_van_xep_dung() {
        let c = catalog_smarthome();
        let top = rank_tools(&c, "read the markdown file", None, 1);
        assert_eq!(c.tools()[top[0]].name, "read_markdown");

        let top = rank_tools(&c, "control the smart home device", None, 1);
        assert_eq!(c.tools()[top[0]].name, "control_smarthome");
    }

    #[test]
    fn top_k_cat_dung_so_luong_va_khong_bao_gio_rong() {
        let c = catalog_smarthome();
        assert_eq!(rank_tools(&c, "gì đó không liên quan", None, 2).len(), 2);
        assert_eq!(
            rank_tools(&c, "hoàn toàn không khớp gì", None, 1).len(),
            1,
            "điểm bằng 0 hết vẫn phải trả về ứng viên — để LLM tự nói NONE"
        );
        assert!(rank_tools(&ToolCatalog::new(), "x", None, 3).is_empty());
        assert!(rank_tools(&c, "x", None, 0).is_empty());
    }

    /// Embedder giả: tất định, không nạp model. Cho điểm cao cho tool có tên
    /// chứa từ khoá đã cắm sẵn.
    struct EmbedderGia;
    impl ToolEmbedder for EmbedderGia {
        fn embed_query_vec(&mut self, text: &str) -> Result<Vec<f32>, String> {
            Ok(vec![
                if text.contains("vault") { 1.0 } else { 0.0 },
                if text.contains("đèn") { 1.0 } else { 0.0 },
            ])
        }
        fn embed_passage_vec(&mut self, text: &str) -> Result<Vec<f32>, String> {
            Ok(vec![
                if text.contains("vault") { 1.0 } else { 0.0 },
                if text.contains("smart home") {
                    1.0
                } else {
                    0.0
                },
            ])
        }
    }

    #[test]
    fn co_embedder_thi_xep_theo_cosine() {
        let c = catalog_smarthome();
        // "đèn" không xuất hiện trong mô tả tool nào ⇒ đường trùng token mù,
        // nhưng embedder nối nó với "smart home".
        let top = rank_tools(&c, "bật đèn", Some(&mut EmbedderGia), 1);
        assert_eq!(
            c.tools()[top[0]].name,
            "control_smarthome",
            "đây chính là chỗ embedder hơn trùng token"
        );
    }

    struct EmbedderHong;
    impl ToolEmbedder for EmbedderHong {
        fn embed_query_vec(&mut self, _: &str) -> Result<Vec<f32>, String> {
            Err("model hỏng".to_string())
        }
        fn embed_passage_vec(&mut self, _: &str) -> Result<Vec<f32>, String> {
            Err("model hỏng".to_string())
        }
    }

    #[test]
    fn embedder_loi_thi_roi_ve_trung_token_chu_khong_hong() {
        let c = catalog_smarthome();
        let top = rank_tools(&c, "read the markdown file", Some(&mut EmbedderHong), 1);
        assert_eq!(c.tools()[top[0]].name, "read_markdown");
    }

    // ---- allowlist chạy tool ----

    static LOCK_ENV: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn with_autoexec<F: FnOnce()>(val: Option<&str>, f: F) {
        let _g = LOCK_ENV.lock().unwrap_or_else(|e| e.into_inner());
        let old = std::env::var("LIVA_MCP_AUTOEXEC").ok();
        match val {
            Some(v) => unsafe { std::env::set_var("LIVA_MCP_AUTOEXEC", v) },
            None => unsafe { std::env::remove_var("LIVA_MCP_AUTOEXEC") },
        }
        f();
        match old {
            Some(v) => unsafe { std::env::set_var("LIVA_MCP_AUTOEXEC", v) },
            None => unsafe { std::env::remove_var("LIVA_MCP_AUTOEXEC") },
        }
    }

    /// Mặc định: tool ngoài KHÔNG được tự chạy. Đây là hàng rào prompt injection
    /// của G1, không phải tuỳ chọn tiện lợi.
    #[test]
    fn mac_dinh_tool_ngoai_chi_duoc_de_xuat() {
        with_autoexec(None, || {
            assert_eq!(
                ExecPolicy::for_tool("filesystem", "write_file"),
                ExecPolicy::ProposeOnly
            );
            assert_eq!(
                ExecPolicy::for_tool("everything", "echo"),
                ExecPolicy::ProposeOnly
            );
        });
    }

    /// `write_markdown` là tool NỘI BỘ nhưng vẫn chỉ được đề xuất: nó ghi file,
    /// và ghi do injection lái là thiệt hại không hoàn lại.
    #[test]
    fn ghi_file_khong_bao_gio_tu_chay_theo_mac_dinh() {
        with_autoexec(None, || {
            assert_eq!(
                ExecPolicy::for_tool(NATIVE_SERVER, "write_markdown"),
                ExecPolicy::ProposeOnly
            );
            assert_eq!(
                ExecPolicy::for_tool(NATIVE_SERVER, "read_markdown"),
                ExecPolicy::Auto
            );
            assert_eq!(
                ExecPolicy::for_tool(NATIVE_SERVER, "control_smarthome"),
                ExecPolicy::Auto,
                "phải ngang bằng những gì route_intent vốn đã tự chạy"
            );
        });
    }

    /// Hàng rào cho lệnh IPC trực tiếp. Đây là chỗ tài liệu 02 §C1.1 chỉ ra là
    /// còn hở: `execute_call` chỉ gác đường G1, còn `mcp:call_tool` /
    /// `mcp_client:call_tool` gọi thẳng và không kiểm gì.
    #[test]
    fn guard_chan_tool_ghi_va_tool_ngoai_theo_mac_dinh() {
        with_autoexec(None, || {
            // Ghi file: chặn, dù là tool nội bộ.
            let e = guard_direct_call(NATIVE_SERVER, "write_markdown")
                .expect_err("write_markdown phải bị chặn theo mặc định");
            assert!(
                e.contains("LIVA_MCP_AUTOEXEC=native/write_markdown"),
                "lỗi phải nói CHÍNH XÁC cách mở, không chỉ 'bị từ chối': {e}"
            );

            // Tool trên server ngoài: chặn.
            assert!(
                guard_direct_call("filesystem", "write_file").is_err(),
                "tool server ngoài phải bị chặn — đây là ca nghiêm trọng hơn"
            );

            // Tool nội bộ chỉ-đọc và điều khiển đảo-ngược-được: cho qua, để không
            // phá những gì vốn đã dùng được qua lớp lệnh.
            for t in ["read_markdown", "search_vault", "control_smarthome"] {
                assert!(
                    guard_direct_call(NATIVE_SERVER, t).is_ok(),
                    "{t} phải vẫn gọi được"
                );
            }
        });
    }

    #[test]
    fn guard_mo_duoc_bang_env_dung_nhu_thong_bao_loi_noi() {
        with_autoexec(Some("native/write_markdown"), || {
            assert!(guard_direct_call(NATIVE_SERVER, "write_markdown").is_ok());
            assert!(
                guard_direct_call("filesystem", "write_file").is_err(),
                "mở một tool không được mở tool khác"
            );
        });
        with_autoexec(Some("filesystem/*"), || {
            assert!(guard_direct_call("filesystem", "write_file").is_ok());
        });
    }

    #[test]
    fn env_mo_duoc_tung_tool_va_ca_server() {
        with_autoexec(Some("everything/echo"), || {
            assert_eq!(ExecPolicy::for_tool("everything", "echo"), ExecPolicy::Auto);
            assert_eq!(
                ExecPolicy::for_tool("everything", "get-env"),
                ExecPolicy::ProposeOnly,
                "mở một tool không được mở cả server"
            );
        });
        with_autoexec(Some(" filesystem/* , everything/echo "), || {
            assert_eq!(
                ExecPolicy::for_tool("filesystem", "read_file"),
                ExecPolicy::Auto
            );
            assert_eq!(
                ExecPolicy::for_tool("khac", "read_file"),
                ExecPolicy::ProposeOnly
            );
        });
    }
}
