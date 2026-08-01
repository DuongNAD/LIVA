//! MCP **client** stdio — chiều LIVA gọi RA ngoài (rung G0).
//!
//! Trước file này LIVA chỉ là MCP *server* (`mcp/server.rs`, 4 tool nội bộ).
//! Chiều còn lại là một `ProcessWrapper` spawn + write-line + read-line, không
//! handshake, không tương quan id, không đọc `mcp_config.json`, và **không nơi
//! nào tham chiếu** — code mồ côi. Xem
//! `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md` §3 (G0).
//!
//! Vì sao đáng làm trước mọi thứ khác: nó mở khoá *mọi* MCP server, không
//! riêng một cái nào. `mcp_config.example.json` liệt kê postgres/redis/github
//! từ lâu nhưng chưa có bộ đọc phía Rust, nên tới giờ vẫn là trang trí.
//!
//! ## Ba chỗ dễ mất một buổi debug, đã xử lý tường minh ở đây
//!
//! 1. **Drain stderr là bắt buộc.** Server con ghi đầy pipe stderr rồi *treo*,
//!    và treo kiểu đó trông y hệt "model đang suy nghĩ". Xem
//!    [`spawn_stderr_drain`].
//! 2. **Vòng đọc stdout không được chết vì một dòng rác.** Nó chết thì MỌI
//!    request đang chờ treo tới hết timeout — cũng lại biểu hiện ra như "server
//!    im lặng". Xem [`spawn_reader`].
//! 3. **`id` trên dây không phải luôn là chuỗi.** Xem [`wire_id`].

use crate::mcp::protocol::{
    CallToolRequest, CallToolResult, JsonRpcError, JsonRpcNotification, JsonRpcRequest,
    JsonRpcResponse, Tool, ToolList,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command};
use tokio::sync::{Mutex, oneshot};
use tokio::task::JoinHandle;
use tracing::{debug, info, warn};

/// Bản giao thức MCP mà client này khai báo lúc handshake.
///
/// Chọn `2024-11-05` (bản đầu) một cách CÓ Ý, không phải vì lạc hậu: nó là bản
/// duy nhất mọi server MCP ngoài kia đều nhận, kể cả các server `npx` trong
/// `mcp_config.example.json`. Server trả bản của nó trong `initialize`; ta ghi
/// log chứ không ép, vì client này chỉ dùng `tools/*` — phần không đổi qua các
/// bản.
pub const CLIENT_PROTOCOL_VERSION: &str = "2024-11-05";

/// Tên client gửi cho server trong `initialize.clientInfo`.
pub const CLIENT_NAME: &str = "liva-native-core";

const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Chặn trên số trang `tools/list`. Server hỏng có thể trả cursor vòng tròn;
/// khi chạm chặn này ta CẮT và ghi log rõ, không im lặng coi như đã đủ.
const MAX_TOOL_PAGES: usize = 50;

/// Cắt log dòng dài để một server nói nhiều không làm ngập log.
const LOG_LINE_CAP: usize = 300;

/// Số dòng stderr cuối giữ lại cho mỗi server, để in kèm khi nó chết.
/// Xem [`spawn_stderr_drain`] về việc vì sao chỉ `debug!` là không đủ.
const STDERR_KEEP: usize = 20;

/// Thời gian chờ mỗi request, `LIVA_MCP_TIMEOUT_MS` ghi đè.
///
/// Giá trị lạ rơi về mặc định kèm `warn` — cùng triết lý với [`crate::env_flag`]:
/// một biến gõ sai không đáng làm hỏng tiến trình, nhưng cũng không được âm
/// thầm đổi hành vi.
fn request_timeout() -> Duration {
    const KEY: &str = "LIVA_MCP_TIMEOUT_MS";
    match std::env::var(KEY) {
        Ok(raw) => match raw.trim().parse::<u64>() {
            Ok(ms) if ms > 0 => Duration::from_millis(ms),
            _ => {
                warn!("{KEY}=\"{raw}\" không phải số ms dương; dùng {DEFAULT_TIMEOUT_MS}");
                Duration::from_millis(DEFAULT_TIMEOUT_MS)
            }
        },
        Err(_) => Duration::from_millis(DEFAULT_TIMEOUT_MS),
    }
}

// ── Cấu hình: mcp_config.json ───────────────────────────────────────────────

/// Một server MCP trong `mcp_config.json`.
///
/// Khuôn theo `mcp_config.example.json`: `mcpServers.{tên}.{command,args,env}`.
/// Trường lạ bị bỏ qua (mặc định của serde) — cần thế, vì file mẫu có `_comment`
/// nằm ngay trong khối server.
#[derive(Debug, Clone, Deserialize)]
pub struct McpServerConfig {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
    /// Biến môi trường thêm cho tiến trình con. **Có thể chứa token** (file mẫu
    /// đặt `GITHUB_PERSONAL_ACCESS_TOKEN` ở đây), nên [`McpClientRegistry::list_servers`]
    /// chỉ trả về TÊN biến, không bao giờ trả giá trị.
    #[serde(default)]
    pub env: BTreeMap<String, String>,
    /// Giữ khai báo trong file nhưng không dùng. Tiện hơn xoá rồi gõ lại.
    #[serde(default)]
    pub disabled: bool,
}

#[derive(Debug, Default, Deserialize)]
struct McpConfigFile {
    #[serde(default, rename = "mcpServers")]
    mcp_servers: BTreeMap<String, McpServerConfig>,
}

/// Đường dẫn `mcp_config.json`: `LIVA_MCP_CONFIG` nếu có, ngược lại
/// `mcp_config.json` cạnh chỗ chạy.
pub fn default_config_path() -> PathBuf {
    match std::env::var("LIVA_MCP_CONFIG") {
        Ok(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
        _ => PathBuf::from("mcp_config.json"),
    }
}

/// Phân tích nội dung `mcp_config.json`, đã lọc các mục không dùng được.
///
/// Bỏ hai loại mục:
/// - tên bắt đầu bằng `_` — quy ước "chú thích/chỗ giữ chỗ". File mẫu có
///   `_clerk_VERIFY_PACKAGE` với tên package là placeholder; spawn nó chắc chắn
///   lỗi, nên nó không được xuất hiện như một server thật.
/// - `disabled: true`.
///
/// Tách khỏi I/O để test được mà không cần chạm đĩa.
pub fn parse_config(text: &str) -> Result<BTreeMap<String, McpServerConfig>, String> {
    // Notepad/PowerShell `Out-File` hay để lại BOM UTF-8; serde_json coi đó là
    // ký tự lạ ở vị trí 0 và báo lỗi khó hiểu.
    let clean = text.strip_prefix('\u{feff}').unwrap_or(text);
    let parsed: McpConfigFile =
        serde_json::from_str(clean).map_err(|e| format!("JSON không hợp lệ: {e}"))?;
    Ok(parsed
        .mcp_servers
        .into_iter()
        .filter(|(name, cfg)| !name.starts_with('_') && !cfg.disabled)
        .collect())
}

/// Đọc + phân tích `mcp_config.json`.
pub fn load_config(path: &Path) -> Result<BTreeMap<String, McpServerConfig>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        if e.kind() == std::io::ErrorKind::NotFound {
            format!(
                "chưa có file cấu hình {} — sao chép mcp_config.example.json rồi sửa, \
                 hoặc trỏ LIVA_MCP_CONFIG tới file khác",
                path.display()
            )
        } else {
            format!("không đọc được {}: {e}", path.display())
        }
    })?;
    parse_config(&text).map_err(|e| format!("{}: {e}", path.display()))
}

// ── Client cho MỘT server ───────────────────────────────────────────────────

type Pending = Arc<Mutex<HashMap<String, oneshot::Sender<JsonRpcResponse>>>>;

/// Kết quả handshake, giữ lại để `mcp_client:list_servers` chứng minh được là
/// handshake ĐÃ chạy thật chứ không chỉ spawn xong tiến trình.
#[derive(Debug, Clone)]
pub struct HandshakeInfo {
    pub protocol_version: String,
    pub server_info: Value,
}

/// Một MCP server ngoài, nói JSON-RPC 2.0 qua stdio.
///
/// Vòng đời: [`connect`](Self::connect) spawn tiến trình con, dựng hai task
/// (drain stderr + đọc stdout), rồi handshake. `Drop` abort hai task và kill
/// tiến trình con.
pub struct McpStdioClient {
    name: String,
    child: Mutex<Child>,
    stdin: Mutex<ChildStdin>,
    pending: Pending,
    next_id: AtomicU64,
    timeout: Duration,
    closed: Arc<AtomicBool>,
    handshake: OnceLock<HandshakeInfo>,
    reader_task: JoinHandle<()>,
    stderr_task: JoinHandle<()>,
}

impl McpStdioClient {
    /// Spawn server con và handshake xong mới trả về.
    ///
    /// Trả `Err` là đã dọn sạch: `Self` chưa kịp ra khỏi hàm thì `Drop` cũng đã
    /// kill tiến trình con.
    pub async fn connect(name: &str, cfg: &McpServerConfig) -> Result<Self, String> {
        let program = resolve_program(&cfg.command);
        let mut cmd = Command::new(&program);
        cmd.args(&cfg.args)
            .envs(&cfg.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            // Lưới an toàn thứ hai sau `impl Drop`: nếu tiến trình LIVA đi
            // xuống bất thường, tokio vẫn kill con.
            .kill_on_drop(true);
        // Không để cửa sổ console đen nháy lên mỗi lần spawn `npx` trong app
        // desktop.
        #[cfg(windows)]
        cmd.creation_flags(windows_sys::Win32::System::Threading::CREATE_NO_WINDOW);

        let mut child = cmd
            .spawn()
            .map_err(|e| format!("không spawn được MCP server '{name}' (lệnh: {program}): {e}"))?;

        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| format!("MCP server '{name}': không lấy được stdin"))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("MCP server '{name}': không lấy được stdout"))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("MCP server '{name}': không lấy được stderr"))?;

        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let closed = Arc::new(AtomicBool::new(false));
        let recent_stderr = Arc::new(std::sync::Mutex::new(
            std::collections::VecDeque::with_capacity(STDERR_KEEP),
        ));

        let client = Self {
            name: name.to_string(),
            child: Mutex::new(child),
            stdin: Mutex::new(stdin),
            pending: Arc::clone(&pending),
            next_id: AtomicU64::new(1),
            timeout: request_timeout(),
            closed: Arc::clone(&closed),
            handshake: OnceLock::new(),
            stderr_task: spawn_stderr_drain(name.to_string(), stderr, Arc::clone(&recent_stderr)),
            reader_task: spawn_reader(name.to_string(), stdout, pending, closed, recent_stderr),
        };

        info!(server = %name, "đã spawn MCP server: {program}");
        client.handshake().await?;
        Ok(client)
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// `true` khi server đã đóng stdout (thoát hoặc crash).
    pub fn is_closed(&self) -> bool {
        self.closed.load(Ordering::SeqCst)
    }

    pub fn handshake_info(&self) -> Option<&HandshakeInfo> {
        self.handshake.get()
    }

    /// `initialize` → `notifications/initialized`.
    ///
    /// Thứ tự này không thương lượng được: gọi `tools/*` trước khi
    /// `initialize` xong thì server trả lỗi khó đọc (hoặc im).
    async fn handshake(&self) -> Result<(), String> {
        let params = json!({
            "protocolVersion": CLIENT_PROTOCOL_VERSION,
            "capabilities": {},
            "clientInfo": { "name": CLIENT_NAME, "version": env!("CARGO_PKG_VERSION") },
        });
        let res = self.request("initialize", Some(params)).await?;

        let protocol_version = res
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("(server không khai báo)")
            .to_string();
        let server_info = res.get("serverInfo").cloned().unwrap_or(Value::Null);

        self.notify("notifications/initialized", None).await?;

        let _ = self.handshake.set(HandshakeInfo {
            protocol_version: protocol_version.clone(),
            server_info,
        });
        info!(server = %self.name, "MCP handshake xong — giao thức {protocol_version}");
        Ok(())
    }

    /// Gửi một request và chờ đúng hồi âm của nó.
    ///
    /// Tương quan id theo đúng mẫu `pending_replies` của
    /// [`crate::agent::dispatcher`]: map id → `oneshot::Sender`, timeout thì
    /// tự rút khoá ra khỏi map để hồi âm đến muộn không đọng lại mãi.
    pub async fn request(&self, method: &str, params: Option<Value>) -> Result<Value, String> {
        if self.is_closed() {
            return Err(format!(
                "MCP server '{}' đã đóng kết nối; không gửi được {method}",
                self.name
            ));
        }

        let id = self.next_id.fetch_add(1, Ordering::SeqCst).to_string();
        let req = JsonRpcRequest::new(id.clone(), method.to_string(), params);
        let line = serde_json::to_string(&req)
            .map_err(|e| format!("không dựng được JSON-RPC request {method}: {e}"))?;

        let (tx, rx) = oneshot::channel();
        self.pending.lock().await.insert(id.clone(), tx);

        if let Err(e) = self.write_line(&line).await {
            self.pending.lock().await.remove(&id);
            return Err(e);
        }

        match tokio::time::timeout(self.timeout, rx).await {
            Ok(Ok(resp)) => match resp.error {
                Some(err) => Err(format!(
                    "[{}] {method} lỗi {}: {}",
                    self.name, err.code, err.message
                )),
                None => Ok(resp.result.unwrap_or(Value::Null)),
            },
            Ok(Err(_)) => Err(format!(
                "[{}] {method}: kênh hồi âm bị huỷ — tiến trình con đã chết?",
                self.name
            )),
            Err(_) => {
                self.pending.lock().await.remove(&id);
                Err(format!(
                    "[{}] {method}: quá {} ms không hồi âm",
                    self.name,
                    self.timeout.as_millis()
                ))
            }
        }
    }

    /// Gửi notification (không có id, không chờ hồi âm).
    pub async fn notify(&self, method: &str, params: Option<Value>) -> Result<(), String> {
        let note = JsonRpcNotification {
            jsonrpc: "2.0".to_string(),
            method: method.to_string(),
            params,
        };
        let line = serde_json::to_string(&note)
            .map_err(|e| format!("không dựng được JSON-RPC notification {method}: {e}"))?;
        self.write_line(&line).await
    }

    async fn write_line(&self, line: &str) -> Result<(), String> {
        let mut stdin = self.stdin.lock().await;
        // Mutex ở đây vừa cho phép `&self` vừa tuần tự hoá ghi — bắt buộc với
        // giao thức phân dòng: hai request ghi xen nhau là hỏng cả hai.
        let mut buf = String::with_capacity(line.len() + 1);
        buf.push_str(line);
        buf.push('\n');
        stdin
            .write_all(buf.as_bytes())
            .await
            .map_err(|e| format!("ghi stdin của '{}' thất bại: {e}", self.name))?;
        stdin
            .flush()
            .await
            .map_err(|e| format!("flush stdin của '{}' thất bại: {e}", self.name))
    }

    /// `tools/list`, đi hết các trang.
    ///
    /// Phân trang không phải chuyện lý thuyết: server nhiều tool trả `nextCursor`,
    /// và lấy một trang rồi coi là xong nghĩa là **cắt âm thầm** danh sách năng
    /// lực — đúng loại lỗi không ai phát hiện ra.
    pub async fn list_tools(&self) -> Result<ToolList, String> {
        let mut tools: Vec<Tool> = Vec::new();
        let mut cursor: Option<String> = None;

        for page in 0..MAX_TOOL_PAGES {
            let params = match &cursor {
                Some(c) => json!({ "cursor": c }),
                None => json!({}),
            };
            let res = self.request("tools/list", Some(params)).await?;
            let this_page: ToolList = serde_json::from_value(res.clone())
                .map_err(|e| format!("[{}] tools/list trả về khuôn lạ: {e}", self.name))?;
            tools.extend(this_page.tools);

            cursor = res
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                return Ok(ToolList { tools });
            }
            debug!(server = %self.name, "tools/list: xong trang {}, còn cursor", page + 1);
        }

        warn!(
            server = %self.name,
            "tools/list vượt {MAX_TOOL_PAGES} trang — CẮT danh sách ở {} tool",
            tools.len()
        );
        Ok(ToolList { tools })
    }

    /// `tools/call`.
    pub async fn call_tool(&self, req: CallToolRequest) -> Result<CallToolResult, String> {
        let res = self
            .request(
                "tools/call",
                Some(json!({ "name": &req.name, "arguments": &req.arguments })),
            )
            .await?;
        serde_json::from_value(res).map_err(|e| {
            format!(
                "[{}] tools/call('{}') trả về khuôn lạ: {e}",
                self.name, req.name
            )
        })
    }
}

impl Drop for McpStdioClient {
    fn drop(&mut self) {
        self.reader_task.abort();
        self.stderr_task.abort();
        // `get_mut` được vì Drop cho `&mut self` — không cần lock.
        if let Err(e) = self.child.get_mut().start_kill() {
            // Con đã tự thoát là ca bình thường, không phải lỗi.
            debug!(server = %self.name, "không kill được tiến trình con (có thể đã thoát): {e}");
        }
    }
}

// ── Hai task nền ────────────────────────────────────────────────────────────

/// Đổ stderr của server con vào `tracing`, và giữ lại N dòng cuối.
///
/// **Không phải tuỳ chọn.** Pipe stderr có dung lượng hữu hạn: không ai đọc
/// thì server con block ở lần ghi làm đầy pipe và đứng im vô hạn. Biểu hiện
/// bên ngoài không phân biệt được với "model đang suy nghĩ", nên đây là loại
/// lỗi ngốn cả buổi để tìm.
///
/// **Vì sao phải giữ lại `recent` chứ không chỉ log rồi quên:** khi server con
/// chết, thứ giải thích *tại sao* nằm ở stderr — và người vận hành không thể
/// biết trước để bật debug *trước* lần crash. Nên dòng thường ở `debug` (server
/// tử tế chỉ ghi banner, không nên spam log mặc định), còn khi server CHẾT thì
/// [`spawn_reader`] in lại `recent` ở mức `warn`: thấy được mà không ồn.
///
/// Lịch sử, vì nó giải thích tại sao chuyện này từng tệ hơn nhiều: tới
/// 26/07/2026 cả gateway lẫn vỏ Tauri đều dựng subscriber bằng
/// `.with_max_level(Level::INFO)` **cứng**, nên `RUST_LOG` vô tác dụng và **mọi**
/// `debug!` trong crate là code chết. Đo được lúc đó: `server-filesystem` ghi
/// `"Secure MCP Filesystem Server running on stdio"` (46 byte) ra stderr, drain
/// đọc được, log tuyệt đối im. Nay đã có [`crate::tracing_env_filter`], nên
/// `RUST_LOG=info,liva_native_core::mcp=debug` xem được cả dòng thường.
fn spawn_stderr_drain(
    name: String,
    stderr: ChildStderr,
    recent: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stderr).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let text = line.trim();
                    if text.is_empty() {
                        continue;
                    }
                    let cat = truncate(text);
                    debug!(server = %name, "stderr: {cat}");
                    // Khoá std::sync là đúng ở đây: không có `.await` nào bên
                    // trong, nên không có chuyện giữ khoá qua điểm nhường.
                    // `poison` không đáng làm chết task đọc log.
                    let mut giu = match recent.lock() {
                        Ok(g) => g,
                        Err(e) => e.into_inner(),
                    };
                    if giu.len() == STDERR_KEEP {
                        giu.pop_front();
                    }
                    giu.push_back(cat);
                }
                Ok(None) => break,
                Err(e) => {
                    debug!(server = %name, "ngừng đọc stderr: {e}");
                    break;
                }
            }
        }
    })
}

/// Lấy `recent` ra dưới dạng một chuỗi để in kèm lúc server chết.
fn stderr_gan_day(recent: &std::sync::Mutex<std::collections::VecDeque<String>>) -> String {
    let giu = match recent.lock() {
        Ok(g) => g,
        Err(e) => e.into_inner(),
    };
    if giu.is_empty() {
        "(server không ghi gì ra stderr)".to_string()
    } else {
        giu.iter()
            .map(|l| format!("\n    | {l}"))
            .collect::<String>()
    }
}

/// Đọc stdout, định tuyến từng hồi âm về đúng người chờ.
///
/// Nguyên tắc xuyên suốt: **không `?`, không panic, không `break` vì dữ liệu
/// xấu.** Vòng này chết là mọi request đang chờ treo tới hết timeout, và biểu
/// hiện ra ngoài y hệt "server không trả lời" — che mất nguyên nhân thật.
fn spawn_reader(
    name: String,
    stdout: ChildStdout,
    pending: Pending,
    closed: Arc<AtomicBool>,
    recent_stderr: Arc<std::sync::Mutex<std::collections::VecDeque<String>>>,
) -> JoinHandle<()> {
    tokio::spawn(async move {
        let mut lines = BufReader::new(stdout).lines();
        loop {
            match lines.next_line().await {
                Ok(Some(line)) => {
                    let text = line.trim();
                    if text.is_empty() {
                        continue;
                    }
                    let Ok(value) = serde_json::from_str::<Value>(text) else {
                        warn!(server = %name, "bỏ qua dòng stdout không phải JSON: {}", truncate(text));
                        continue;
                    };
                    if value.get("method").is_some() {
                        // Notification, hoặc request server→client (sampling/roots).
                        // Ta khai báo `capabilities: {}` lúc handshake nên server
                        // đúng chuẩn không gửi request nào — bỏ qua là đúng, và
                        // vì thế task này không cần chạm tới stdin.
                        debug!(server = %name, "bỏ qua tin nhắn hướng vào: {}", truncate(text));
                        continue;
                    }
                    route_response(&name, &pending, &value).await;
                }
                Ok(None) => {
                    closed.store(true, Ordering::SeqCst);
                    // In lại stderr ở `warn` để nó thấy được ở cấu hình MẶC
                    // ĐỊNH: không ai biết trước lần crash để mà bật debug sẵn
                    // (xem `spawn_stderr_drain`).
                    warn!(
                        server = %name,
                        "server đóng stdout (EOF) — stderr {} dòng cuối:{}",
                        STDERR_KEEP,
                        stderr_gan_day(&recent_stderr)
                    );
                    fail_all(&name, &pending, "server đã đóng stdout (EOF)").await;
                    break;
                }
                Err(e) => {
                    closed.store(true, Ordering::SeqCst);
                    let reason = format!("lỗi đọc stdout: {e}");
                    warn!(
                        server = %name,
                        "{reason} — stderr {} dòng cuối:{}",
                        STDERR_KEEP,
                        stderr_gan_day(&recent_stderr)
                    );
                    fail_all(&name, &pending, &reason).await;
                    break;
                }
            }
        }
        debug!(server = %name, "vòng đọc stdout kết thúc");
    })
}

async fn route_response(name: &str, pending: &Pending, value: &Value) {
    match wire_id(value) {
        Some(id) => {
            let waiter = pending.lock().await.remove(&id);
            match waiter {
                Some(tx) => {
                    // `Err` chỉ nghĩa là bên chờ đã bỏ đi (timeout) — không sao.
                    let _ = tx.send(to_response(id, value));
                }
                None => warn!(
                    server = %name,
                    "hồi âm id '{id}' không có ai chờ (đến muộn hoặc trùng)"
                ),
            }
        }
        None => deliver_orphan(name, pending, value).await,
    }
}

/// Hồi âm `id: null` — không có chủ hiển nhiên.
///
/// JSON-RPC 2.0 cho phép ca này khi server lỗi TRƯỚC lúc đọc được id (parse
/// error / invalid request). Nếu đúng MỘT request đang bay thì nó là chủ khả dĩ
/// duy nhất, nên giao lỗi cho nó. Đây không phải đoán cho vui: để nó chờ hết
/// timeout sẽ báo sai thành "server không hồi âm" và **chôn mất thông điệp lỗi
/// mà server đã nói ra**. Nhiều request đang bay thì không đoán — ghi log rồi bỏ.
async fn deliver_orphan(name: &str, pending: &Pending, value: &Value) {
    let mut guard = pending.lock().await;
    if guard.len() == 1
        && let Some(id) = guard.keys().next().cloned()
        && let Some(tx) = guard.remove(&id)
    {
        warn!(
            server = %name,
            "hồi âm id:null — giao cho request duy nhất đang chờ (id {id}): {}",
            truncate(&value.to_string())
        );
        let _ = tx.send(to_response(id, value));
        return;
    }
    warn!(
        server = %name,
        "bỏ hồi âm không có id ({} request đang chờ): {}",
        guard.len(),
        truncate(&value.to_string())
    );
}

/// Trả lỗi cho toàn bộ request đang chờ khi kết nối đứt.
async fn fail_all(name: &str, pending: &Pending, reason: &str) {
    let waiting: Vec<(String, oneshot::Sender<JsonRpcResponse>)> =
        pending.lock().await.drain().collect();
    if waiting.is_empty() {
        return;
    }
    warn!(
        server = %name,
        "{reason} — huỷ {} request đang chờ", waiting.len()
    );
    for (id, tx) in waiting {
        let _ = tx.send(JsonRpcResponse::error(
            id,
            -32000,
            format!("[{name}] {reason}"),
            None,
        ));
    }
}

// ── Chuẩn hoá khuôn hồi âm ──────────────────────────────────────────────────

/// Chuẩn hoá `id` trên dây về `String`.
///
/// `JsonRpcResponse.id` trong `protocol.rs` là `String` **không** `Option`, nên
/// deserialize thẳng vào nó sẽ FAIL với hai ca hoàn toàn hợp chuẩn: id kiểu SỐ
/// (JSON-RPC và MCP đều cho phép) và `id: null` (lỗi ở tầng giao thức). Fail ở
/// tầng serde thì hồi âm biến mất và biểu hiện ra ngoài như "server không trả
/// lời".
///
/// Xử lý tại chỗ này thay vì đổi kiểu trong `protocol.rs`: kiểu đó đang là hợp
/// đồng của `mcp/server.rs` (chiều LIVA *làm* server), nơi id luôn do client
/// gửi tới và luôn là chuỗi. Đổi nó sẽ lan sang một đường chạy đang tốt mà
/// không được gì.
///
/// Client này phát id là chuỗi thập phân (`"1"`, `"2"`, …), nên server nào
/// coerce sang số thì `Number::to_string()` vẫn khớp lại đúng khoá.
fn wire_id(value: &Value) -> Option<String> {
    match value.get("id") {
        Some(Value::String(s)) => Some(s.clone()),
        Some(Value::Number(n)) => Some(n.to_string()),
        _ => None,
    }
}

/// Dựng [`JsonRpcResponse`] từ JSON thô với `id` đã chuẩn hoá.
///
/// `error` sai khuôn (ví dụ `code` là chuỗi) KHÔNG được biến thành "không có
/// lỗi" — đó là cách chắc chắn nhất để một lời gọi thất bại trông như thành
/// công rỗng. Ca đó thành lỗi nội bộ, giữ nguyên JSON gốc trong `data`.
fn to_response(id: String, value: &Value) -> JsonRpcResponse {
    let error = value.get("error").filter(|raw| !raw.is_null()).map(|raw| {
        serde_json::from_value::<JsonRpcError>(raw.clone()).unwrap_or_else(|_| JsonRpcError {
            code: -32603,
            message: format!(
                "error không đúng khuôn JSON-RPC: {}",
                truncate(&raw.to_string())
            ),
            data: Some(raw.clone()),
        })
    });
    JsonRpcResponse {
        jsonrpc: "2.0".to_string(),
        id,
        result: value.get("result").cloned(),
        error,
    }
}

fn truncate(s: &str) -> String {
    let total = s.chars().count();
    if total <= LOG_LINE_CAP {
        return s.to_string();
    }
    let head: String = s.chars().take(LOG_LINE_CAP).collect();
    format!("{head}… (+{} ký tự)", total - LOG_LINE_CAP)
}

/// Tìm file thực thi cho `command`.
///
/// Trên Windows `std::process::Command` **không** áp `PATHEXT`:
/// `Command::new("npx")` thất bại "program not found" vì trên đĩa chỉ có
/// `npx.cmd`. Cả ba server mẫu trong `mcp_config.example.json` gọi `npx`/`docker`,
/// nên không xử lý chỗ này thì file mẫu vẫn là trang trí — đúng thứ G0 hứa sửa.
///
/// Không tìm được thì trả nguyên `command` để lỗi spawn của HĐH nói thật, thay
/// vì ta bịa ra một thông báo riêng.
#[cfg(windows)]
fn resolve_program(command: &str) -> String {
    if command.contains('/') || command.contains('\\') || Path::new(command).extension().is_some() {
        return command.to_string();
    }
    let Some(paths) = std::env::var_os("PATH") else {
        return command.to_string();
    };
    for dir in std::env::split_paths(&paths) {
        for ext in ["exe", "cmd", "bat"] {
            let candidate = dir.join(format!("{command}.{ext}"));
            if candidate.is_file() {
                return candidate.to_string_lossy().into_owned();
            }
        }
    }
    command.to_string()
}

#[cfg(not(windows))]
fn resolve_program(command: &str) -> String {
    command.to_string()
}

// ── Registry ────────────────────────────────────────────────────────────────

/// Tập các MCP server ngoài, nối lười theo `mcp_config.json`.
///
/// Nối lười có ý: cấu hình có thể liệt kê 5 server, mà một phiên chỉ dùng 1.
/// Spawn cả 5 lúc khởi động là 5 tiến trình Node/Docker cho không.
///
/// Cấu hình được đọc lại từ đĩa ở mỗi lần tra, nên sửa `mcp_config.json` có
/// hiệu lực ngay, không cần khởi động lại LIVA.
pub struct McpClientRegistry {
    config_path: PathBuf,
    servers: Mutex<HashMap<String, Arc<McpStdioClient>>>,
}

impl McpClientRegistry {
    pub fn new(config_path: PathBuf) -> Self {
        Self {
            config_path,
            servers: Mutex::new(HashMap::new()),
        }
    }

    pub fn config_path(&self) -> &Path {
        &self.config_path
    }

    pub fn configured(&self) -> Result<BTreeMap<String, McpServerConfig>, String> {
        load_config(&self.config_path)
    }

    /// Lấy client đang sống, hoặc nối mới.
    ///
    /// Giữ lock suốt lúc `connect().await` là CỐ Ý: nó tuần tự hoá việc nối, nên
    /// hai lệnh tới cùng lúc không spawn hai tiến trình con cho cùng một server.
    ///
    /// Client đã chết (server crash) bị loại và nối lại — server con hỏng không
    /// làm hỏng vĩnh viễn cả phiên.
    pub async fn get_or_connect(&self, name: &str) -> Result<Arc<McpStdioClient>, String> {
        let mut guard = self.servers.lock().await;

        if matches!(guard.get(name), Some(c) if c.is_closed()) {
            warn!(server = %name, "MCP server đã chết — nối lại");
            guard.remove(name);
        }
        if let Some(existing) = guard.get(name) {
            return Ok(Arc::clone(existing));
        }

        let configured = self.configured()?;
        let cfg = configured.get(name).ok_or_else(|| {
            let known = configured.keys().cloned().collect::<Vec<_>>().join(", ");
            if known.is_empty() {
                format!(
                    "{} không khai báo server MCP nào dùng được",
                    self.config_path.display()
                )
            } else {
                format!(
                    "không có server MCP tên '{name}' trong {} (đang có: {known})",
                    self.config_path.display()
                )
            }
        })?;

        let client = Arc::new(McpStdioClient::connect(name, cfg).await?);
        guard.insert(name.to_string(), Arc::clone(&client));
        Ok(client)
    }

    pub async fn list_tools(&self, name: &str) -> Result<ToolList, String> {
        self.get_or_connect(name).await?.list_tools().await
    }

    pub async fn call_tool(
        &self,
        name: &str,
        req: CallToolRequest,
    ) -> Result<CallToolResult, String> {
        self.get_or_connect(name).await?.call_tool(req).await
    }

    /// Ảnh chụp cấu hình + trạng thái nối, cho `mcp_client:list_servers`.
    ///
    /// Trả `envKeys` (tên biến) chứ **không** trả `env` (giá trị): file mẫu đặt
    /// `GITHUB_PERSONAL_ACCESS_TOKEN` ngay trong đó, và lệnh này đi ra WebSocket.
    pub async fn list_servers(&self) -> Value {
        let config_exists = self.config_path.exists();
        let (servers, error) = match self.configured() {
            Ok(configured) => {
                let live = self.servers.lock().await;
                let rows: Vec<Value> = configured
                    .iter()
                    .map(|(name, cfg)| {
                        let client = live.get(name);
                        json!({
                            "name": name,
                            "command": cfg.command,
                            "args": cfg.args,
                            "envKeys": cfg.env.keys().collect::<Vec<_>>(),
                            "connected": client.is_some_and(|c| !c.is_closed()),
                            "protocolVersion": client
                                .and_then(|c| c.handshake_info())
                                .map(|h| h.protocol_version.clone()),
                            "serverInfo": client
                                .and_then(|c| c.handshake_info())
                                .map_or(Value::Null, |h| h.server_info.clone()),
                        })
                    })
                    .collect();
                (rows, Value::Null)
            }
            Err(e) => (Vec::new(), Value::String(e)),
        };
        json!({
            "configPath": self.config_path.display().to_string(),
            "configExists": config_exists,
            "servers": servers,
            "error": error,
        })
    }
}

static GLOBAL: OnceLock<Arc<McpClientRegistry>> = OnceLock::new();

/// Registry dùng chung cho cả tiến trình.
///
/// Vì sao KHÔNG nằm trong `AppState`: mỗi client giữ một **tiến trình con
/// thật**, mà `AppState` được dựng ở 9 chỗ (gateway, vỏ Tauri, 5 test/bin).
/// Đặt vào đó nghĩa là mỗi chỗ dựng lên một bầy `npx` riêng. Tiến trình con là
/// tài nguyên phạm vi TIẾN TRÌNH, nên registry cũng phải vậy.
///
/// [`McpClientRegistry`] vẫn là struct thường: test dựng bản riêng của nó và
/// không bao giờ chạm cái global này.
pub fn global_registry() -> Arc<McpClientRegistry> {
    Arc::clone(GLOBAL.get_or_init(|| Arc::new(McpClientRegistry::new(default_config_path()))))
}

// ── Test ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ---- chuẩn hoá id trên dây (bẫy "server không trả lời") ----

    #[test]
    fn id_chuoi_giu_nguyen() {
        let v = json!({ "jsonrpc": "2.0", "id": "7", "result": {} });
        assert_eq!(wire_id(&v).as_deref(), Some("7"));
    }

    /// MCP cho phép id kiểu số. Client phát id thập phân dạng chuỗi nên server
    /// coerce sang số vẫn phải khớp lại đúng khoá đang chờ.
    #[test]
    fn id_so_khop_lai_dung_khoa() {
        let v = json!({ "jsonrpc": "2.0", "id": 7, "result": {} });
        assert_eq!(wire_id(&v).as_deref(), Some("7"));
    }

    #[test]
    fn id_null_hoac_thieu_thi_khong_co_chu() {
        assert!(wire_id(&json!({ "jsonrpc": "2.0", "id": null })).is_none());
        assert!(wire_id(&json!({ "jsonrpc": "2.0", "result": {} })).is_none());
    }

    // ---- dựng lại hồi âm ----

    #[test]
    fn giu_nguyen_result_va_error_dung_khuon() {
        let ok = to_response("1".into(), &json!({ "result": { "a": 1 } }));
        assert_eq!(ok.result, Some(json!({ "a": 1 })));
        assert!(ok.error.is_none());

        let err = to_response(
            "2".into(),
            &json!({ "error": { "code": -32601, "message": "Method not found" } }),
        );
        let e = err.error.expect("phải có error");
        assert_eq!(e.code, -32601);
        assert_eq!(e.message, "Method not found");
    }

    /// `error` sai khuôn KHÔNG được im lặng thành "không có lỗi" — lời gọi thất
    /// bại mà trông như thành công rỗng là ca tệ nhất.
    #[test]
    fn error_sai_khuon_van_la_error() {
        let r = to_response("3".into(), &json!({ "error": { "code": "oops" } }));
        let e = r.error.expect("error sai khuôn vẫn phải là error");
        assert_eq!(e.code, -32603);
        assert!(e.data.is_some(), "phải giữ lại JSON gốc để chẩn đoán");
    }

    #[test]
    fn error_null_khong_phai_la_loi() {
        let r = to_response("4".into(), &json!({ "result": 1, "error": null }));
        assert!(r.error.is_none());
    }

    // ---- tương quan id ----

    #[tokio::test]
    async fn dinh_tuyen_dung_nguoi_cho_va_bo_id_la() {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx1, rx1) = oneshot::channel();
        let (tx2, rx2) = oneshot::channel();
        {
            let mut g = pending.lock().await;
            g.insert("1".to_string(), tx1);
            g.insert("2".to_string(), tx2);
        }

        // Hồi âm về NGƯỢC thứ tự gửi — đúng lý do phải có tương quan id.
        route_response("t", &pending, &json!({ "id": "2", "result": "hai" })).await;
        route_response("t", &pending, &json!({ "id": 1, "result": "mot" })).await;

        assert_eq!(rx1.await.unwrap().result, Some(json!("mot")));
        assert_eq!(rx2.await.unwrap().result, Some(json!("hai")));
        assert!(pending.lock().await.is_empty(), "phải rút hết khoá đã dùng");

        // id lạ: chỉ ghi log, không panic, không đọng lại.
        route_response("t", &pending, &json!({ "id": "999", "result": null })).await;
        assert!(pending.lock().await.is_empty());
    }

    /// Có đúng một request đang bay → lỗi `id: null` phải tới được nó, thay vì
    /// để nó chờ hết timeout rồi báo sai thành "server im lặng".
    #[tokio::test]
    async fn loi_id_null_giao_cho_request_duy_nhat() {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert("1".to_string(), tx);

        route_response(
            "t",
            &pending,
            &json!({ "id": null, "error": { "code": -32700, "message": "Parse error" } }),
        )
        .await;

        let got = rx.await.expect("phải được giao, không bị bỏ");
        assert_eq!(got.error.expect("có error").code, -32700);
    }

    /// Hai request đang bay thì KHÔNG đoán — cả hai đều còn nguyên trong map.
    #[tokio::test]
    async fn loi_id_null_khong_doan_khi_nhieu_request() {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx1, _rx1) = oneshot::channel();
        let (tx2, _rx2) = oneshot::channel();
        {
            let mut g = pending.lock().await;
            g.insert("1".to_string(), tx1);
            g.insert("2".to_string(), tx2);
        }
        route_response(
            "t",
            &pending,
            &json!({ "id": null, "error": { "code": -1, "message": "x" } }),
        )
        .await;
        assert_eq!(pending.lock().await.len(), 2);
    }

    #[tokio::test]
    async fn dut_ket_noi_thi_moi_request_dang_cho_deu_nhan_loi() {
        let pending: Pending = Arc::new(Mutex::new(HashMap::new()));
        let (tx, rx) = oneshot::channel();
        pending.lock().await.insert("1".to_string(), tx);

        fail_all("t", &pending, "server đã đóng stdout (EOF)").await;

        let got = rx.await.expect("phải nhận lỗi, không treo tới timeout");
        assert!(got.error.expect("có error").message.contains("EOF"));
        assert!(pending.lock().await.is_empty());
    }

    // ---- stderr giữ lại để in khi server chết ----

    /// Ca server-chết phải in lại được stderr ở mức mặc định — người vận hành
    /// không thể bật debug *trước* lần crash. Test này canh chính cái đó.
    #[test]
    fn giu_n_dong_stderr_cuoi_va_in_lai_duoc() {
        let recent = std::sync::Mutex::new(std::collections::VecDeque::new());

        assert!(
            stderr_gan_day(&recent).contains("không ghi gì"),
            "server im lặng phải nói rõ là im lặng, không trả chuỗi rỗng khó hiểu"
        );

        // Đổ nhiều hơn hạn để kiểm vòng đệm giữ dòng CUỐI, không phải dòng ĐẦU.
        {
            let mut g = recent.lock().unwrap();
            for i in 0..(STDERR_KEEP + 5) {
                if g.len() == STDERR_KEEP {
                    g.pop_front();
                }
                g.push_back(format!("dong {i}"));
            }
        }
        let ra = stderr_gan_day(&recent);
        assert_eq!(
            recent.lock().unwrap().len(),
            STDERR_KEEP,
            "vòng đệm phải chặn ở STDERR_KEEP"
        );
        assert!(
            ra.contains(&format!("dong {}", STDERR_KEEP + 4)),
            "phải giữ dòng CUỐI (panic của server nằm ở cuối), nhận được: {ra}"
        );
        assert!(
            !ra.contains("dong 0\n") && !ra.ends_with("dong 0"),
            "dòng đầu phải bị đẩy ra"
        );
    }

    // ---- mcp_config.json ----

    /// Khuôn thật của `mcp_config.example.json`, gồm cả mục `_`-prefix có
    /// `_comment` bên trong và mục thiếu `args`/`env`.
    #[test]
    fn doc_dung_khuon_file_mau() {
        let cfg = parse_config(
            r#"{
              "mcpServers": {
                "postgres": { "command": "npx", "args": ["-y", "@modelcontextprotocol/server-postgres"] },
                "khong-args": { "command": "docker" },
                "co-env": { "command": "x", "env": { "TOKEN": "bi-mat" } },
                "tat": { "command": "x", "disabled": true },
                "_clerk_VERIFY_PACKAGE": {
                  "_comment": "placeholder, spawn se loi",
                  "command": "npx",
                  "args": ["-y", "YOUR_PACKAGE_HERE"]
                }
              }
            }"#,
        )
        .expect("phải phân tích được");

        assert_eq!(
            cfg.keys().cloned().collect::<Vec<_>>(),
            vec!["co-env", "khong-args", "postgres"],
            "mục _-prefix và disabled phải bị loại"
        );
        assert_eq!(cfg["postgres"].args.len(), 2);
        assert!(cfg["khong-args"].args.is_empty(), "thiếu args → rỗng");
        assert!(cfg["khong-args"].env.is_empty(), "thiếu env → rỗng");
        assert_eq!(cfg["co-env"].env["TOKEN"], "bi-mat");
    }

    /// PowerShell `Out-File` để lại BOM; không bỏ thì serde báo lỗi ở vị trí 0
    /// và người đọc không hiểu tại sao file "trông đúng" lại sai.
    #[test]
    fn bo_bom_utf8() {
        let cfg = parse_config("\u{feff}{\"mcpServers\":{\"a\":{\"command\":\"x\"}}}")
            .expect("BOM không được làm hỏng");
        assert!(cfg.contains_key("a"));
    }

    #[test]
    fn thieu_mcp_servers_thi_rong_chu_khong_loi() {
        assert!(parse_config("{}").expect("hợp lệ").is_empty());
    }

    #[test]
    fn json_sai_thi_bao_loi() {
        assert!(parse_config("{ khong phai json }").is_err());
        // `command` là trường bắt buộc — thiếu nó thì không spawn được gì.
        assert!(parse_config(r#"{"mcpServers":{"a":{"args":[]}}}"#).is_err());
    }

    #[test]
    fn file_thieu_thi_bao_loi_doc_duoc() {
        let e = load_config(Path::new("khong-ton-tai-dau-mcp_config.json")).expect_err("phải lỗi");
        assert!(
            e.contains("mcp_config.example.json"),
            "lỗi phải chỉ ra cách sửa, nhận được: {e}"
        );
    }

    // ---- tìm file thực thi ----

    #[test]
    fn duong_dan_co_san_thi_giu_nguyen() {
        assert_eq!(resolve_program("node.exe"), "node.exe");
        assert_eq!(resolve_program("C:\\a\\b.cmd"), "C:\\a\\b.cmd");
    }

    /// Đây là chỗ `Command::new("npx")` thất bại trên Windows: std không áp
    /// PATHEXT. `cmd` luôn có trong System32 nên là mẫu thử ổn định.
    #[cfg(windows)]
    #[test]
    fn windows_tim_duoc_duoi_thuc_thi() {
        let got = resolve_program("cmd");
        assert!(
            got.to_lowercase().ends_with("cmd.exe"),
            "phải giải ra cmd.exe, nhận được: {got}"
        );
    }
}
