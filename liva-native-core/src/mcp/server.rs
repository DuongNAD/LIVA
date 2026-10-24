use crate::mcp::protocol::{CallToolRequest, CallToolResult, Tool, ToolContent, ToolList};
use schemars::{JsonSchema, schema_for};
use serde::Deserialize;
use serde_json::Value;
use std::path::{Path, PathBuf};

pub struct NativeMcpServer {
    vault_path: PathBuf,
}

/// Canonical của **tổ tiên tồn tại gần nhất** của `p` (kể cả chính `p`).
///
/// `canonicalize` chỉ chạy được trên thứ có thật, nên đây là cách duy nhất hỏi
/// được "đường dẫn này SẼ nằm ở đâu" cho một file chưa được tạo. Mỗi bước lên
/// một cấp; gặp thứ canonicalize được thì dừng — thứ đó đã giải hết mọi liên kết
/// trên đường đi, nên nó là điểm neo đúng để so containment.
///
/// `None` khi lần hết lên tới gốc mà không có gì tồn tại — caller phải coi đó là
/// TỪ CHỐI, không phải cho qua.
fn to_ton_tai_gan_nhat(p: &Path) -> Option<PathBuf> {
    let mut hien_tai = p;
    loop {
        if let Ok(that) = hien_tai.canonicalize() {
            return Some(that);
        }
        hien_tai = hien_tai.parent()?;
    }
}

#[derive(JsonSchema, Deserialize)]
struct ReadMarkdownArgs {
    path: String,
}

#[derive(JsonSchema, Deserialize)]
struct WriteMarkdownArgs {
    path: String,
    content: String,
}

#[derive(JsonSchema, Deserialize)]
struct SearchVaultArgs {
    query: String,
}

// Tham số của tool `control_smarthome`.
//
// Dùng lại ĐÚNG enum của `integrations::smart_home` thay vì `String` trơn. Bản
// trước là `{ device: String, command: String }` — một bản sao bị suy giảm của
// `smart_home::get_metadata()`, nơi đã khai báo sẵn
// `device: ["light","ac","fan"]` và `action: ["on","off"]`.
//
// Hai hệ quả của bản cũ, đo ở cổng G1 (26/07/2026, gemma-4-E4B):
//  - không có từ vựng trong schema ⇒ model sinh `"air conditioner"`, `"turn on"`;
//    chọn đúng tool 13/13 mà tham số sai 9/13.
//  - tên trường lệch: `command` ở đây nhưng `action` ở `smart_home::execute` và
//    ở `integration:smart_home_control`. Cùng một năng lực, hai tên.
//
// `action` là tên chuẩn (hai chỗ kia dùng nó). `command` giữ làm `alias` để
// caller cũ không vỡ, nhưng schema chỉ quảng cáo `action`.
//
// `//` chứ không `///`: schemars đưa doc comment vào schema thành `description`.
#[derive(JsonSchema, Deserialize)]
struct ControlSmartHomeArgs {
    device: crate::integrations::smart_home::SmartHomeDevice,
    #[serde(alias = "command")]
    action: crate::integrations::smart_home::SmartHomeAction,
}

impl NativeMcpServer {
    pub fn new(vault_path: &str) -> Self {
        Self {
            vault_path: PathBuf::from(vault_path),
        }
    }

    /// Bốn tool nội bộ. Mô tả **song ngữ + kèm ví dụ cách nói** từ 26/07/2026.
    ///
    /// Vì sao mô tả trông "dài quá cho một dòng description": nó là **dữ liệu
    /// truy hồi**, không phải chú thích cho người đọc. `tool_calling::CatalogTool::embed_text`
    /// ghép `name: description` rồi embed để xếp hạng; người dùng nói tiếng Việt,
    /// nên mô tả toàn tiếng Anh làm embedding mù. Đo trên `multilingual-e5-small`
    /// với bản mô tả cũ (4 chuỗi tiếng Anh ngắn):
    ///
    /// - biên (top1−top2) chỉ 0,0001–0,0251 với **mọi** câu ⇒ cả 4 tool "hơi
    ///   giống" bất kỳ câu nào như nhau
    /// - 8/11 câu trò chuyện đều rơi vào `search_vault` — mô tả chung chung nhất
    ///   thì hút hết
    /// - `"mở quạt lên giúp mình"` cho top-1 là `read_markdown`, không phải
    ///   `control_smarthome`
    ///
    /// Nên mục tiêu KHÔNG phải "dài hơn" mà là **đặc trưng hơn**: mỗi mô tả chứa
    /// đúng những từ mà *chỉ* người cần tool đó mới nói. Chi phí là token trong
    /// prompt chọn tool — đo được ở `tool_calling_probe`, và đó là phép đánh đổi
    /// phải kiểm bằng số, không phải đoán.
    pub fn list_tools(&self) -> ToolList {
        ToolList {
            tools: vec![
                Tool {
                    name: "read_markdown".to_string(),
                    description: "Đọc nội dung một file ghi chú markdown đã có trong vault. \
                         Read an existing markdown note from the vault."
                        .to_string(),
                    input_schema: schema_for!(ReadMarkdownArgs),
                },
                Tool {
                    name: "write_markdown".to_string(),
                    description: "Lưu hoặc tạo mới một file ghi chú markdown trong vault. \
                         Save or create a markdown note in the vault."
                        .to_string(),
                    input_schema: schema_for!(WriteMarkdownArgs),
                },
                Tool {
                    name: "search_vault".to_string(),
                    // CỐ TÌNH hẹp. Bản cũ ("Search the vault for a keyword") hút
                    // 8/11 câu trò chuyện vì "search"/"find" quá chung; thu hẹp
                    // xuống còn 2/11.
                    description: "Tìm từ khoá xuyên các ghi chú trong vault khi chưa biết file \
                         nào chứa nó. Full-text search across vault notes; not for general \
                         knowledge questions."
                        .to_string(),
                    input_schema: schema_for!(SearchVaultArgs),
                },
                Tool {
                    name: "control_smarthome".to_string(),
                    description: "Bật hoặc tắt thiết bị nhà thông minh: đèn, quạt, điều hoà. \
                         Turn a smart home device on or off: light, fan, air conditioner."
                        .to_string(),
                    input_schema: schema_for!(ControlSmartHomeArgs),
                },
                // Hai tool OS (U19). Dùng THẲNG struct của `integrations::os_control`
                // thay vì khai lại ở đây: bài học của `ControlSmartHomeArgs` là
                // một bản sao struct sẽ trôi khỏi bản gốc rồi lệch tên trường.
                Tool {
                    name: "control_volume".to_string(),
                    // Nói rõ "ĐỘ TO ... kể cả nhạc": câu "nhỏ nhạc lại" chứa chữ
                    // "nhạc" nên bị control_media hút mất ở lần đo đầu. Ranh giới
                    // đúng là ĐỘ TO (tool này) ≠ ĐANG PHÁT GÌ (tool kia).
                    description: "Chỉnh ĐỘ TO của âm thanh — to lên, nhỏ lại, kể cả khi đang \
                         phát nhạc — hoặc gạt tắt/bật tiếng. Change how LOUD the sound is, \
                         or toggle mute."
                        .to_string(),
                    input_schema: schema_for!(crate::integrations::os_control::VolumeArgs),
                },
                Tool {
                    name: "control_media".to_string(),
                    // "bật nhạc lên" bị chọn nhầm sang control_smarthome ở lần đo
                    // đầu (Qwen3-VL-2B): "bật/tắt" gắn quá chặt với thiết bị. Nên
                    // mô tả phải nói thẳng ranh giới NHẠC/VIDEO ≠ thiết bị trong nhà.
                    description: "Phát, tạm dừng, chuyển bài kế tiếp hoặc quay lại bài trước — \
                         KHÔNG đổi độ to, KHÔNG phải thiết bị trong nhà. \
                         Play, pause, next or previous track. Does not change loudness."
                        .to_string(),
                    input_schema: schema_for!(crate::integrations::os_control::MediaArgs),
                },
                // Tool DUY NHẤT đi ra Internet. Mô tả nói thẳng "hiện tại,
                // ngoài trời" để tách khỏi `control_smarthome` — câu "bật điều
                // hoà cho mát" nói về thiết bị trong nhà, không phải thời tiết,
                // và hai thứ đó dùng chung rất nhiều từ (nóng, lạnh, mát).
                Tool {
                    name: "get_weather".to_string(),
                    description: "Thời tiết HIỆN TẠI ngoài trời ở một địa điểm — nhiệt độ, \
                         mưa nắng, độ ẩm. KHÔNG điều khiển thiết bị nào. \
                         Current outdoor weather at a location. Needs Internet."
                        .to_string(),
                    input_schema: schema_for!(crate::integrations::weather::WeatherArgs),
                },
            ],
        }
    }

    /// Danh sách kỹ năng định dạng chuẩn JSON cho UI (`get_skills_list` và `system_status`).
    ///
    /// Mỗi phần tử chứa đủ 5 khoá bắt buộc (`name`, `category`, `short_desc`, `description`, `parameters`)
    /// kèm metadata phân quyền (`action_kind`, `default_policy`, `server`).
    pub fn list_skills(&self) -> Vec<Value> {
        self.list_tools()
            .tools
            .into_iter()
            .map(|tool| {
                let category = match tool.name.as_str() {
                    "read_markdown" | "write_markdown" | "search_vault" => "docs",
                    "control_smarthome" => "core",
                    "control_volume" | "control_media" => "system",
                    "get_weather" => "web",
                    _ => "extension",
                };
                let action_kind = match tool.name.as_str() {
                    "read_markdown" | "search_vault" | "get_weather" => "read",
                    _ => "write",
                };
                let default_policy = crate::llm::ExecPolicy::default_for_tool(
                    crate::llm::tool_calling::NATIVE_SERVER,
                    &tool.name,
                )
                .as_str();
                serde_json::json!({
                    "name": tool.name,
                    "category": category,
                    "short_desc": tool.description,
                    "description": tool.description,
                    "parameters": tool.input_schema,
                    "action_kind": action_kind,
                    "default_policy": default_policy,
                    "server": crate::llm::tool_calling::NATIVE_SERVER,
                })
            })
            .collect()
    }

    /// Ví dụ cách nói cho từng tool — **chỉ dùng để embed**, không vào prompt.
    ///
    /// Vì sao tách khỏi `description` (xem `tool_calling::CatalogTool::embed_extra`):
    /// nhồi những câu này vào `description` làm prompt phồng 193 → 417 token và độ
    /// trễ mỗi lượt chat 1877 → 3939 ms, trong khi LLM gần như không cần chúng —
    /// nó chỉ thấy 4 ứng viên. Còn *embedding* thì cần: người dùng nói tiếng Việt,
    /// mà `description` một mình không chứa "bật/tắt/mở/quạt/máy lạnh/ghi chú".
    ///
    /// Thêm ví dụ vào đây là an toàn về chi phí: nó không tốn token prompt nào.
    pub fn retrieval_examples() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "read_markdown",
                "đọc file ghi-chu.md · mở ghi chú hôm qua ra xem · nội dung file đó là gì · \
                 xem lại ghi chú · read the note about MCP · open that markdown file",
            ),
            (
                "write_markdown",
                "ghi lại đoạn này vào ghi chú · lưu vào file y.md · tạo ghi chú mới · \
                 chép cái này lại · write this down · save as a note",
            ),
            (
                "search_vault",
                "tìm trong ghi chú xem có gì về kiến trúc · ghi chú nào nói về X · \
                 tra trong vault · search the vault for mcp · find in my notes",
            ),
            (
                "control_smarthome",
                "bật đèn · tắt đèn giúp mình · mở quạt lên · tắt quạt đi · bật điều hoà · \
                 tắt máy lạnh · mở đèn phòng khách · turn on the light · turn off the fan · \
                 switch the air conditioner off",
            ),
            (
                "control_volume",
                "to lên · nhỏ lại · vặn nhỏ nhạc · giảm âm lượng · tăng âm lượng · \
                 tắt tiếng đi · bật tiếng lại · ồn quá · nghe không rõ · mở to hơn · \
                 turn it down · louder · mute the sound",
            ),
            (
                "control_media",
                "tạm dừng · dừng nhạc lại · phát tiếp đi · bật nhạc lên · bài kế tiếp · \
                 chuyển bài · qua bài khác · quay lại bài trước · pause the music · \
                 next song · resume playback",
            ),
            (
                "get_weather",
                // "hôm nay thế nào" và "có nên mang ô không" là hai cách hỏi
                // KHÔNG chứa chữ "thời tiết" — mà đó lại là cách người ta hỏi
                // thật. Thiếu chúng thì truy hồi trượt đúng ca phổ biến nhất.
                "thời tiết hôm nay thế nào · hà nội hôm nay nóng không · ngoài trời \
                 mưa không · có nên mang ô không · nhiệt độ bây giờ bao nhiêu · \
                 trời hôm nay ra sao · mai có mưa không · what's the weather · \
                 is it raining outside · how hot is it today",
            ),
        ]
    }

    // A helper to prevent path traversal
    /// `pub` từ 22/07/2026: đây là hàng rào ghim-dưới-vault duy nhất của core,
    /// và Telegram `/ls` `/cat` cần đúng hàng rào này — trước đó chúng gọi
    /// thẳng `read_dir`/`read_to_string` không lọc gì, tức ai lọt allow-list
    /// đọc được `.env`, vault, khoá **qua Internet** (lộ trình mục 0.7).
    pub fn resolve_path(&self, rel_path: &str) -> Result<PathBuf, String> {
        let p = Path::new(rel_path);
        // Windows drive prefix ("C:", "C:\...", "C:file"): trên Windows `join`
        // sẽ THAY THẾ cả path vì mang prefix ổ đĩa; trên Unix nó trông như tên
        // file bình thường nhưng là ý định thoát vault rõ ràng — chặn ở cả hai.
        let b = rel_path.as_bytes();
        let la_drive_prefix = b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic();
        if p.is_absolute()
            || p.has_root()
            || p.components().any(|c| c == std::path::Component::ParentDir)
            || la_drive_prefix
            // Nợ cross-platform: `\` là phân cách trên Windows nhưng là ký tự
            // thường trên Unix, nên `..\env` lọt qua các kiểm tra bên dưới khi
            // chạy trên macOS/Linux. Vault không có lý do chính đáng chứa tên
            // file mang `\` — từ chối thẳng để hành vi nhất quán mọi nền.
            || rel_path.contains('\\')
        {
            return Err("Invalid path (traversal detected)".to_string());
        }

        let full = self.vault_path.join(p);
        if !full.starts_with(&self.vault_path) {
            return Err("Invalid path (traversal detected)".to_string());
        }

        // Vault chưa tồn tại ⇒ chưa có liên kết nào để mà thoát qua, và từ chối
        // tất cả ở đây sẽ giết ca thật "người dùng chưa tạo vault lần nào".
        let Ok(goc_that) = self.vault_path.canonicalize() else {
            return Ok(full);
        };

        let neo = to_ton_tai_gan_nhat(&full)
            .ok_or_else(|| "Invalid path (cannot resolve containment)".to_string())?;
        if !neo.starts_with(&goc_that) {
            return Err("Invalid path (link escapes vault)".to_string());
        }
        Ok(full)
    }

    pub async fn call_tool(&self, req: CallToolRequest) -> Result<CallToolResult, String> {
        match req.name.as_str() {
            "read_markdown" => {
                let args: ReadMarkdownArgs =
                    serde_json::from_value(req.arguments).map_err(|e| e.to_string())?;
                let path = self.resolve_path(&args.path)?;
                match tokio::fs::read_to_string(&path).await {
                    Ok(content) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: content }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Error reading file {}: {}", args.path, e),
                        }],
                        is_error: true,
                    }),
                }
            }
            "write_markdown" => {
                let args: WriteMarkdownArgs =
                    serde_json::from_value(req.arguments).map_err(|e| e.to_string())?;
                let path = self.resolve_path(&args.path)?;
                if let Some(parent) = path.parent()
                    && let Err(e) = tokio::fs::create_dir_all(parent).await
                {
                    return Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Error creating parent directories: {}", e),
                        }],
                        is_error: true,
                    });
                }
                match tokio::fs::write(&path, &args.content).await {
                    Ok(_) => Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: "Success".to_string(),
                        }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text {
                            text: format!("Error writing file {}: {}", args.path, e),
                        }],
                        is_error: true,
                    }),
                }
            }
            "search_vault" => {
                let args: SearchVaultArgs =
                    serde_json::from_value(req.arguments).map_err(|e| e.to_string())?;

                let vault_path = self.vault_path.clone();
                let query = args.query.clone();

                let text_res = tokio::task::spawn_blocking(move || -> Result<String, String> {
                    if query.trim().is_empty() {
                        return Ok(format!("No matching files found for query '{}'", query));
                    }

                    let query_lower = query.to_lowercase();

                    struct VaultMatch {
                        path: String,
                        size: u64,
                        snippet: Option<String>,
                    }

                    fn search_dir(
                        dir: &Path,
                        vault_path: &Path,
                        query_lower: &str,
                        matches: &mut Vec<VaultMatch>,
                        scanned_count: &mut usize,
                        max_results: usize,
                        max_scan: usize,
                    ) -> std::io::Result<()> {
                        if matches.len() >= max_results || *scanned_count >= max_scan {
                            return Ok(());
                        }

                        const EXCLUDED_DIRS: &[&str] = &[
                            ".git",
                            ".obsidian",
                            ".trash",
                            "target",
                            "node_modules",
                            "archive",
                        ];
                        const MAX_FILE_SIZE: u64 = 2 * 1024 * 1024; // 2MB cap

                        if dir.is_dir() {
                            for entry in std::fs::read_dir(dir)? {
                                if matches.len() >= max_results || *scanned_count >= max_scan {
                                    break;
                                }

                                let entry = entry?;
                                let loai = entry.file_type()?;
                                if loai.is_symlink() {
                                    continue;
                                }

                                let path = entry.path();
                                if loai.is_dir() {
                                    let dir_name = entry.file_name();
                                    let dir_str = dir_name.to_string_lossy();
                                    if dir_str.starts_with('.')
                                        || EXCLUDED_DIRS
                                            .iter()
                                            .any(|&ex| dir_str.eq_ignore_ascii_case(ex))
                                    {
                                        continue;
                                    }
                                    search_dir(
                                        &path,
                                        vault_path,
                                        query_lower,
                                        matches,
                                        scanned_count,
                                        max_results,
                                        max_scan,
                                    )?;
                                } else if loai.is_file() {
                                    let ext = path
                                        .extension()
                                        .and_then(|s| s.to_str())
                                        .unwrap_or("")
                                        .to_lowercase();
                                    if ext != "md" && ext != "txt" {
                                        continue;
                                    }

                                    *scanned_count += 1;

                                    let file_size = match std::fs::metadata(&path) {
                                        Ok(meta) => {
                                            if meta.len() > MAX_FILE_SIZE {
                                                continue;
                                            }
                                            meta.len()
                                        }
                                        Err(_) => continue,
                                    };

                                    let Ok(rel_path) = path.strip_prefix(vault_path) else {
                                        continue;
                                    };
                                    let rel_str = rel_path.to_string_lossy().replace('\\', "/");
                                    let rel_lower = rel_str.to_lowercase();

                                    let Ok(content) = std::fs::read_to_string(&path) else {
                                        continue;
                                    };

                                    let name_matched = rel_lower.contains(query_lower);
                                    let content_match = content
                                        .lines()
                                        .find(|line| line.to_lowercase().contains(query_lower));

                                    if name_matched || content_match.is_some() {
                                        let snippet = content_match.map(|line| {
                                            let trimmed = line.trim();
                                            if trimmed.len() > 120 {
                                                let mut end = 120;
                                                while !trimmed.is_char_boundary(end) && end > 0 {
                                                    end -= 1;
                                                }
                                                format!("{}...", &trimmed[..end])
                                            } else {
                                                trimmed.to_string()
                                            }
                                        });

                                        matches.push(VaultMatch {
                                            path: rel_str,
                                            size: file_size,
                                            snippet,
                                        });
                                    }
                                }
                            }
                        }
                        Ok(())
                    }

                    let mut matches = Vec::new();
                    let mut scanned_count = 0;
                    const MAX_RESULTS: usize = 20;
                    const MAX_SCAN_FILES: usize = 1000;

                    if let Err(e) = search_dir(
                        &vault_path,
                        &vault_path,
                        &query_lower,
                        &mut matches,
                        &mut scanned_count,
                        MAX_RESULTS,
                        MAX_SCAN_FILES,
                    ) {
                        return Err(format!("Error reading vault directory: {}", e));
                    }

                    if matches.is_empty() {
                        Ok(format!("No matching files found for query '{}'", query))
                    } else {
                        let mut res = format!(
                            "Search results for '{}' ({} found):\n",
                            query,
                            matches.len()
                        );
                        for item in matches {
                            res.push_str(&format!("- {} ({} bytes)\n", item.path, item.size));
                            if let Some(snippet) = &item.snippet {
                                if !snippet.is_empty() {
                                    res.push_str(&format!("  Preview: {}\n", snippet));
                                }
                            }
                        }
                        Ok(res)
                    }
                })
                .await
                .map_err(|e| format!("Blocking search task panicked: {e}"))?;

                match text_res {
                    Ok(text) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: e }],
                        is_error: true,
                    }),
                }
            }
            "control_smarthome" => {
                let args: ControlSmartHomeArgs =
                    serde_json::from_value(req.arguments).map_err(|e| e.to_string())?;
                // TRUNG THỰC: chưa có tích hợp phần cứng thật (đồng bộ với
                // integrations::smart_home::execute) — không báo đã gửi/đã điều khiển.
                // Đi qua đúng `smart_home::execute` thay vì tự dựng câu riêng:
                // hai đường (từ khoá qua `tool_exec`, và LLM qua tool này) phải
                // cho CÙNG một câu trả lời, nếu không thì "hai đường khớp nhau"
                // chỉ đúng ở tên tool mà sai ở thứ người dùng nghe được.
                let payload = serde_json::json!({
                    "device": args.device,
                    "action": args.action,
                });
                match crate::integrations::smart_home::execute(payload) {
                    Ok(text) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    }),
                    Err(e) => Ok(CallToolResult {
                        content: vec![ToolContent::Text { text: e }],
                        is_error: true,
                    }),
                }
            }
            // Hai tool OS (U19). Đi thẳng vào `integrations::os_control` — cùng
            // lý do như `control_smarthome`: một năng lực, một câu trả lời, dù
            // tới qua đường từ khoá hay qua LLM.
            //
            // Lỗi được trả dưới dạng `is_error: true` chứ KHÔNG phải `Err`:
            // `Err` ở đây nghĩa là "không có tool đó", còn "có tool nhưng chạy
            // hỏng" là kết quả hợp lệ mà LLM cần đọc được để nói lại cho người
            // dùng — trộn hai thứ sẽ biến một lỗi UIPI thành "tool not found".
            "control_volume" => Ok(
                match crate::integrations::os_control::control_volume(req.arguments) {
                    Ok(text) => CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    },
                    Err(e) => CallToolResult {
                        content: vec![ToolContent::Text { text: e }],
                        is_error: true,
                    },
                },
            ),
            "get_weather" => Ok(
                match crate::integrations::weather::get_weather(req.arguments).await {
                    Ok(text) => CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    },
                    // `is_error: true` chứ không phải trả chuỗi rỗng: mất mạng
                    // phải đọc được thành "không lấy được vì cần Internet",
                    // không được biến thành một câu trả lời trống trông như
                    // LIVA không hiểu câu hỏi.
                    Err(e) => CallToolResult {
                        content: vec![ToolContent::Text { text: e }],
                        is_error: true,
                    },
                },
            ),
            "control_media" => Ok(
                match crate::integrations::os_control::control_media(req.arguments) {
                    Ok(text) => CallToolResult {
                        content: vec![ToolContent::Text { text }],
                        is_error: false,
                    },
                    Err(e) => CallToolResult {
                        content: vec![ToolContent::Text { text: e }],
                        is_error: true,
                    },
                },
            ),
            _ => Err(format!("Tool '{}' not found", req.name)),
        }
    }
}

#[cfg(test)]
mod sandbox_tests {
    use super::NativeMcpServer;
    use crate::mcp::protocol::CallToolRequest;

    /// Hồi quy cho lộ trình 0.7: đây là đúng các đường dẫn mà `/ls`/`/cat`
    /// Telegram TỪNG chấp nhận và đọc được qua Internet. Hàng rào này giờ là
    /// thứ duy nhất đứng giữa allow-list Telegram và toàn bộ ổ đĩa.
    #[test]
    fn chan_cac_duong_tan_cong_kinh_dien() {
        let s = NativeMcpServer::new("vault_test_goc");
        for xau in [
            r"..\.env",
            "../.env",
            "../../data/liva_vault.json",
            r"C:\Windows\System32\config\SAM",
            "/etc/passwd",
            r"\\may-khac\share\bi-mat.txt",
            // Windows drive-relative: KHÔNG tuyệt đối, KHÔNG có root, nhưng
            // `join` sẽ THAY THẾ path vì nó mang prefix ổ đĩa — chỉ lớp kiểm
            // `starts_with` sau join bắt được.
            "C:bi-mat.txt",
            "ghi-chu/../../.env",
        ] {
            assert!(
                s.resolve_path(xau).is_err(),
                "duong dan phai bi TU CHOI: {xau}"
            );
        }
    }

    /// Schema của `control_smarthome` phải MANG được từ vựng hợp lệ.
    ///
    /// Hồi quy của cổng G1 (26/07/2026): bản `{device: String, command: String}`
    /// không nói từ vựng, nên gemma-4-E4B sinh `"air conditioner"`/`"turn on"` —
    /// chọn đúng tool 13/13 mà tham số sai 9/13. Enum sửa đúng chỗ đó.
    #[test]
    fn schema_control_smarthome_khai_bao_tu_vung() {
        let tools = NativeMcpServer::new("v").list_tools();
        let t = tools
            .tools
            .iter()
            .find(|t| t.name == "control_smarthome")
            .expect("phải có tool");
        let s = serde_json::to_value(&t.input_schema).expect("schema ra JSON");

        // schemars đặt enum vào `definitions`, `properties` chỉ có `$ref`.
        let defs = s.get("definitions").expect("phải có definitions");
        let thiet_bi = defs["SmartHomeDevice"]["enum"]
            .as_array()
            .expect("device phải là enum, không phải string trơn");
        assert_eq!(thiet_bi.len(), 3, "light/ac/fan");
        let hanh_dong = defs["SmartHomeAction"]["enum"]
            .as_array()
            .expect("action phải là enum");
        assert_eq!(hanh_dong.len(), 2, "on/off");

        // Tên trường chuẩn là `action`, khớp `smart_home::execute` và
        // `integration:smart_home_control`.
        assert!(s["properties"].get("action").is_some());
        assert!(
            s["properties"].get("command").is_none(),
            "schema chỉ nên quảng cáo `action`; `command` chỉ là alias khi ĐỌC"
        );

        // Và doc comment KHÔNG được lọt vào schema: schemars nhét `///` vào
        // `description`, đi thẳng ra `mcp:list_tools` và phình prompt mọi caller.
        let mo_ta_dai = serde_json::to_string(&s).unwrap_or_default().len();
        assert!(
            mo_ta_dai < 900,
            "schema phình {mo_ta_dai} byte — có doc comment lọt vào description?"
        );
    }

    /// Tên cũ `command` phải vẫn đọc được: đổi tên trường là thay đổi phá vỡ với
    /// caller đã có, mà `mcp:call_tool` là cổng công khai.
    #[tokio::test]
    async fn van_nhan_ten_cu_command() {
        let s = NativeMcpServer::new("v");
        for (nhan, args) in [
            (
                "tên mới",
                serde_json::json!({ "device": "light", "action": "on" }),
            ),
            (
                "tên cũ",
                serde_json::json!({ "device": "light", "command": "on" }),
            ),
        ] {
            let r = s
                .call_tool(CallToolRequest {
                    name: "control_smarthome".to_string(),
                    arguments: args,
                })
                .await
                .unwrap_or_else(|e| panic!("{nhan} phải chạy được: {e}"));
            assert!(!r.is_error, "{nhan}");
        }

        // Từ vựng ngoài enum phải bị TỪ CHỐI, không âm thầm bỏ qua.
        assert!(
            s.call_tool(CallToolRequest {
                name: "control_smarthome".to_string(),
                arguments: serde_json::json!({ "device": "air conditioner", "action": "turn on" }),
            })
            .await
            .is_err(),
            "đúng thứ model sinh khi schema thiếu enum — phải lỗi rõ ràng"
        );
    }

    #[test]
    fn cho_phep_duong_dan_hop_le_trong_vault() {
        let s = NativeMcpServer::new("vault_test_goc");
        assert!(
            s.resolve_path("").is_ok(),
            "chuoi rong = goc vault (cho /ls mac dinh)"
        );
        assert!(s.resolve_path("ghi-chu.md").is_ok());
        assert!(s.resolve_path("thu-muc/con/tep.md").is_ok());
        // Ket qua phai nam DUOI vault
        let p = s.resolve_path("a/b.md").unwrap();
        assert!(p.starts_with("vault_test_goc"));
    }

    #[tokio::test]
    async fn search_vault_prunes_excluded_directories() {
        use crate::mcp::protocol::ToolContent;
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!("liva_vault_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp vault");

        // Create valid file
        fs::write(
            temp_dir.join("valid_note.md"),
            "This is a valid note with PRUNE_TEST_KEYWORD",
        )
        .expect("write valid note");

        // Create excluded directories and notes
        for dir_name in &[".git", ".obsidian", ".trash", "target", "node_modules", "Archive", "archive"] {
            let sub = temp_dir.join(dir_name);
            fs::create_dir_all(&sub).expect("create excluded dir");
            fs::write(
                sub.join("ignored.md"),
                "This file is inside an excluded directory with PRUNE_TEST_KEYWORD",
            )
            .expect("write ignored file");
        }

        let server = NativeMcpServer::new(temp_dir.to_str().unwrap());
        let res = server
            .call_tool(CallToolRequest {
                name: "search_vault".to_string(),
                arguments: serde_json::json!({ "query": "PRUNE_TEST_KEYWORD" }),
            })
            .await
            .expect("search_vault call succeeded");

        let _ = fs::remove_dir_all(&temp_dir);

        assert!(!res.is_error);
        if let ToolContent::Text { text } = &res.content[0] {
            assert!(
                text.contains("valid_note.md"),
                "Should find valid_note.md, got: {text}"
            );
            assert!(
                !text.contains(".git"),
                "Should prune .git directory, got: {text}"
            );
            assert!(
                !text.contains(".obsidian"),
                "Should prune .obsidian directory, got: {text}"
            );
            assert!(
                !text.contains("target"),
                "Should prune target directory, got: {text}"
            );
            assert!(
                !text.contains("node_modules"),
                "Should prune node_modules directory, got: {text}"
            );
            assert!(
                !text.contains("Archive/ignored.md") && !text.contains("archive/ignored.md"),
                "Should prune Archive directory, got: {text}"
            );
        } else {
            panic!("Expected Text content");
        }
    }

    #[tokio::test]
    async fn search_vault_caps_at_20_results_and_case_insensitive() {
        use crate::mcp::protocol::ToolContent;
        use std::fs;

        let temp_dir = std::env::temp_dir().join(format!("liva_vault_cap_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        fs::create_dir_all(&temp_dir).expect("create temp vault");

        // Create 25 matching files
        for i in 1..=25 {
            fs::write(
                temp_dir.join(format!("note_{:02}.md", i)),
                format!("Note content line {} containing CaseInsensitiveQUERY and preview text.", i),
            )
            .expect("write note");
        }

        let server = NativeMcpServer::new(temp_dir.to_str().unwrap());
        // Query using lowercase
        let res = server
            .call_tool(CallToolRequest {
                name: "search_vault".to_string(),
                arguments: serde_json::json!({ "query": "caseinsensitivequery" }),
            })
            .await
            .expect("search_vault call succeeded");

        let _ = fs::remove_dir_all(&temp_dir);

        assert!(!res.is_error);
        if let ToolContent::Text { text } = &res.content[0] {
            assert!(
                text.contains("20 found"),
                "Expected header to report 20 found, got: {text}"
            );
            let item_count = text.lines().filter(|l| l.starts_with("- ")).count();
            assert_eq!(
                item_count, 20,
                "Expected exactly 20 items capped, got: {item_count}"
            );
            assert!(
                text.contains("bytes)"),
                "Expected file size metadata in bytes, got: {text}"
            );
            assert!(
                text.contains("Preview:"),
                "Expected snippet preview, got: {text}"
            );
        } else {
            panic!("Expected Text content");
        }
    }
}
