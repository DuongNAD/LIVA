---
title: "Đề xuất tích hợp OpenSpace (HKUDS)"
updated: 2026-07-29
commit: 185f33a
stale-ok: e69f47d
status: living
owns:
  - de-xuat-openspace-g0-g4
  - phan-ra-openspace-lay-va-tu-choi
covers:
  - liva-native-core/src/mcp/client.rs
  - liva-native-core/src/mcp/protocol.rs
  - liva-native-core/tests/mcp_client_e2e.rs
  - scripts/e2e-mcp-server.mjs
  - scripts/verify-mcp-real.mjs
  - liva-native-core/src/llm/tool_calling.rs
  - liva-native-core/src/bin/tool_calling_probe.rs
  - liva-native-core/src/skills/loader.rs
  - liva-native-core/src/skills/store.rs
  - liva-native-core/src/skills/ranker.rs
  - liva-native-core/src/skills/signals.rs
  - liva-native-core/tests/skills_commands.rs
  - liva-native-core/src/db.rs
  - liva-native-core/src/integrations/smart_home.rs
  - liva-native-core/src/mcp/server.rs
  - liva-native-core/src/agent/graph.rs
  - liva-native-core/src/agent/dispatcher.rs
  - liva-native-core/src/evolution/mod.rs
  - liva-native-core/src/evolution/sandbox.rs
  - mcp_config.example.json
---
# Đề xuất tích hợp OpenSpace (HKUDS)

[⬆ Mục lục](../README.md) · [◀ Lộ trình sửa lỗi và nâng cấp](03-lo-trinh-sua-loi-va-nang-cap.md)

---

> **Luận điểm:** OpenSpace *đề xuất*, sandbox của LIVA *phán quyết*. Lấy phần LIVA không tự
> xây rẻ được (tiến hoá skill do LLM dẫn), từ chối phần phá vỡ định vị local-first
> (`execute_task`, cloud). Nhưng **không rung nào tới được nếu chưa có MCP client thật.**

> **Cập nhật 25–26/07/2026 — G0 đã xong và đã commit** (`8e7511f` → `4f5e326`).
> `mcp/client.rs` không còn là code mồ côi, và đã chạy với MCP server ngoài THẬT. Xem §3 G0 để
> biết cái gì đã đo được và cái gì chưa.
>
> **Nợ tài liệu G0 — ĐÃ TRẢ 26/07/2026.** Bốn tài liệu dưới đây từng mô tả trạng thái TRƯỚC G0;
> nay đã sửa, kèm bảng kiểm kê mồ côi tính lại từ đo trực tiếp:
>
> | Tài liệu | Đã sửa gì |
> |---|---|
> | [01-ban-ve/05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) | bảng §1, node mermaid (đỏ → xanh), §8.4, danh sách mồ côi §9.2 |
> | [01-ban-ve/09](../01-ban-ve/09-tich-hop-ngoai.md) | §9.0, §9.1.1 viết lại hẳn, §9.6; hai "bug tiềm ẩn" của bản cũ nay ghi rõ là **đã hết** |
> | [01-ban-ve/10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) | kiểm kê: **bốn** → **ba** thành phần, 1 311 → **1 338 dòng**, 7,0% → **4,4%** (mẫu số đo lại 18 687 → **30 530**); `mcp/client.rs` ra khỏi subgraph mồ côi |
> | [03-danh-gia/01](01-doi-chieu-tuyen-bo-vs-thuc-te.md) | đã được sửa ở `e2ecdf1` |
>
> Sửa thêm một chỗ **không nằm trong bốn cái trên**, phát hiện khi rà:
> [03-danh-gia/02](02-no-ky-thuat-va-rui-ro.md) §"hàm `pub` 0 caller" khẳng định
> `ProcessWrapper::{send_request, read_response}` vẫn 0 ref — **hai hàm đó không còn tồn tại**;
> và `JsonRpcResponse::error` nay có ref thật (`client.rs:645`).
>
> Tỉ lệ mồ côi tụt 7,0% → 4,4% vì **hai** lý do độc lập, đừng đọc thành một: `client.rs` rời
> danh sách, **và** crate lớn lên 18 687 → 30 530 dòng.

Đối tượng khảo sát: <https://github.com/HKUDS/OpenSpace> — MIT, ~6.9k sao, tạo 24/03/2026,
còn commit hằng ngày. Tự mô tả là "the skill management layer for AI agents": theo dõi chất
lượng skill từ kết quả task thật, tiến hoá skill theo ba trigger (FIX / DERIVED / CAPTURED),
lưu SQLite + version DAG, expose ra ngoài bằng MCP server. Python 3.12 + LiteLLM.

Prompt bàn giao để bắt tay làm G0: [openspace-g0-mcp-client-prompt.md](../04-quy-trinh/prompts/openspace-g0-mcp-client-prompt.md)

---

## 1. Hiện trạng LIVA — đã kiểm chứng trên mã tại commit `bcd6a73`

| Thành phần | Thực tế | Nhãn |
|---|---|---|
| MCP **server** | `mcp/server.rs` — `NativeMcpServer`, **6 tool**: `read_markdown`, `write_markdown`, `search_vault`, `control_smarthome`, + `control_volume` / `control_media` (thêm ở U19, `6b5b87b`). Ra ngoài qua `mcp:list_tools` / `mcp:call_tool` trong `lib.rs` | **[OK]** |
| MCP **client** | `mcp/client.rs` (rewrite 25/07/2026) — `McpStdioClient` + `McpClientRegistry`: handshake `initialize`→`notifications/initialized`, tương quan id qua `HashMap<String, oneshot::Sender>`, `tools/list` (có phân trang) + `tools/call`, drain stderr trong task riêng, timeout mỗi request, kill child khi drop, đọc `mcp_config.json`. Ra ngoài qua `mcp_client:list_servers` / `mcp_client:list_tools` / `mcp_client:call_tool` trong `lib.rs` | **[OK]** — đã chạy với server ngoài thật (`npx`), xem §3 G0 |
| Kiểu MCP | `mcp/protocol.rs` — `JsonRpcRequest/Response/Notification/Error`, `Tool`, `ToolList`, `CallToolRequest`, `CallToolResult`, `ToolContent`. Từ 25/07/2026 đã dùng được cho **cả hai chiều**: 4 attribute serde sửa chỗ khuôn-đọc lệch chuẩn MCP (xem §3 G0) | **[OK]** |
| Chọn hành động | `route_intent` (khớp token cứng) vẫn là **đường nhanh**, và từ 26/07/2026 có thêm `llm/tool_calling.rs` — LLM chọn tool từ schema thật, truy hồi top-k bằng embedder. Cổng 13/13 trên **cả** `gemma-4-E4B` **và** `Qwen3-VL-2B` (model router thực tế). **Mặc định TẮT** (`LIVA_TOOL_CALLING=1`) vì đo được **+2501 ms trung vị** cho mỗi câu chat | **[MỘT PHẦN]** — đúng/sai đã đo xong, chưa bật mặc định vì chi phí; xem §3 G1 |
| Swarm đa agent | `agent/dispatcher.rs` — khung truyền tin + `pending_replies` chạy được, nhưng role `Code` trả chuỗi hardcode | **[MỘT PHẦN]** |
| Tự sửa | `evolution/mod.rs` — `SelfCorrectionLoop` + `Sandbox` + `BackupGuard`: vá, chạy test, rollback. Có stress test riêng | **[OK]** |
| DB | `db.rs` — đã có `PRAGMA user_version` + `SCHEMA_VERSION` + danh sách migration tuyến tính. Thêm bảng là an toàn | **[OK]** |
| RAG | `llm/embedder.rs`, 384-dim e5-small, `LIVA_RAG_TOP_K` mặc định 3, `n_ctx` mặc định 4096, beta chạy model 2–4B | **[OK]** |
| `mcp_config.example.json` | Đã có bộ đọc phía Rust (`mcp::client::load_config`): `mcpServers.{tên}.{command,args,env}`, thêm `disabled`, bỏ mục tên `_`-prefix (mục `_clerk_VERIFY_PACKAGE` là placeholder). File thật `mcp_config.json` đã vào `.gitignore` vì khối `env` chứa token | **[OK]** |

### 1.1 Chẩn đoán

Lỗ hổng lớn nhất **không phải** "thiếu metric chất lượng skill". Là hai thứ nằm dưới nó:

1. ~~**LIVA chưa gọi được ra ngoài.** Là MCP server, không phải MCP client.~~ **Đã mở
   (25/07/2026, G0)** — nhưng chỉ là *đường ống*: chưa có thứ gì phía LLM tự quyết định gọi
   nó, đó là G1.
2. **LIVA chưa có tầng skill nào để đo.** ~~`route_intent` là bảng từ khoá cứng, không phải bộ
   chọn năng lực mở rộng được.~~ Bộ chọn mở rộng được **đã có** (26/07/2026, G1:
   `ToolCatalog` + truy hồi embedder + LLM chọn). Nhưng thứ nó chọn *từ* vẫn chỉ là 6 tool nội
   bộ cộng tool của server MCP ngoài. ~~**chưa có kho skill nào**~~ **Kho skill đã có**
   (26/07/2026, G2: `src/skills/`, 3 bảng, 5 lệnh) — nhưng G1 **chưa** chọn từ nó, xem §3 G2.

Cửa thứ hai vẫn đóng — nay vì thiếu **kho** để chọn, không còn vì thiếu **bộ chọn**.

---

## 2. Phân rã OpenSpace: lấy gì, từ chối gì

`skill_engine/` của họ ≈ **1.4 MB Python / 55 file**. Port tay là phi thực tế — nên phải
chia theo giá trị thay vì nhận hoặc bỏ cả khối.

| Bộ phận OpenSpace | Quyết định | Lý do |
|---|---|---|
| Định dạng skill (`SKILL.md` + `.skill_id`) | **Lấy** — chỉ là quy ước thư mục | Tương thích luôn với skill của Claude Code trong `.claude/skills/` |
| Xếp hạng (`skill_ranker.py`) | **Lấy** — BM25 → embedding rerank, ngưỡng tiền lọc 10, giữ `top_k × 3` ứng viên | Nhỏ. LIVA đã có embedder |
| Taxonomy tín hiệu chất lượng (`signals/types.py`) | **Lấy** — chỉ là phân loại | `tool_call_failed`, `tool_failure_affects_skill`, `skill_selection_not_invoked`, `tool_semantic_issue` + các trường `actionability` / `evidence_status` / `failure_signature` / `merge_key` |
| Tiến hoá (`authoring`, `admission`, `validator`, `behavior_eval`, `candidates`) | **Không port** — ~280 KB Python do LLM dẫn | Viết lại là nhiều tháng và sẽ ra bản tệ hơn. Dùng sidecar |
| `execute_task` | **Từ chối** | Nó tự chạy vòng lặp LLM riêng → hai vòng agent chồng nhau, LIVA tụt xuống thành vỏ voice cho một agent Python |
| `cloud_browse_skills`, `upload_skill`, `cloud_auth_flow` | **Từ chối** | Đường dữ liệu đi ra ngoài, mâu thuẫn trực tiếp định vị offline |

### 2.1 Một đính chính về OpenSpace

`skill_ranker.py` của họ **không** có trọng số chất lượng — xếp hạng thuần BM25 + cosine.
Tín hiệu chất lượng chảy vào *tiến hoá*, không vào *truy hồi*. Đây là chỗ LIVA làm hơn được
với chi phí rất thấp (mục G3), không phải chỗ đi copy.

---

## 3. Lộ trình 5 rung — mỗi rung có giá trị độc lập

### G0 — MCP client thật **[ĐÃ XONG 25–26/07/2026]**

`ProcessWrapper` (49 dòng, mồ côi) → `mcp/client.rs` 1 143 dòng gồm test, cộng
`tests/mcp_client_e2e.rs` (442), `scripts/e2e-mcp-server.mjs` (184) và
`scripts/verify-mcp-real.mjs` (306).

| Hạng mục trong phạm vi | Trạng thái |
|---|---|
| handshake `initialize` → `notifications/initialized` | xong |
| tương quan id `HashMap<String, oneshot::Sender<JsonRpcResponse>>` | xong — đúng mẫu `pending_replies` của `agent/dispatcher.rs` |
| `tools/list` + `tools/call` trả kiểu trong `protocol.rs` | xong, `tools/list` đi hết phân trang `nextCursor` (chặn 50 trang rồi CẮT có ghi log) |
| drain stderr vào `tracing` (task riêng) | xong |
| timeout mỗi request (`LIVA_MCP_TIMEOUT_MS`, mặc định 30 000 ms) | xong |
| vòng đời child (`kill_on_drop` + `Drop` abort 2 task rồi `start_kill`) | xong |
| đọc `mcp_config.json` (`LIVA_MCP_CONFIG` ghi đè) | xong |
| 3 lệnh `mcp_client:*` trong `handle_command` | xong |

**Bốn sửa đổi trong `protocol.rs`** — đều là chiều ĐỌC, chiều ghi của `mcp/server.rs` không
đổi hành vi:

1. `Tool.description` + `Tool.input_schema` → `#[serde(default)]`. Cả hai là tuỳ chọn phía
   server MCP; thiếu một cái làm **cả** `tools/list` fail deserialize.
2. `ToolContent::Image` → `rename_all = "camelCase"` ở cấp *variant*. Trên dây là `mimeType`;
   `rename_all` cấp enum chỉ đổi tên variant nên không với tới trường.
3. `ToolContent::Unsupported` (`#[serde(other)]`) — `audio`/`resource`/`resource_link`. Không
   có nó thì một phần tử `content` lạ làm hỏng **toàn bộ** lời gọi, kể cả khi phần text cần
   dùng nằm ngay bên cạnh.
4. `CallToolResult.content` → `#[serde(default)]` (MCP 2025-06-18 cho phép chỉ có
   `structuredContent`).

**`JsonRpcResponse.id` KHÔNG đổi** dù nó là `String` không `Option`. Chuẩn hoá id nằm ở
`client::wire_id`: id kiểu số → `to_string()`, `id: null` → không có chủ. Đổi kiểu sẽ lan sang
`mcp/server.rs` — nơi id luôn do client gửi tới và luôn là chuỗi — mà không được gì.

**Ngoài phạm vi đã ghi nhưng cần cho lời hứa "file mẫu thành thật":**
`client::resolve_program` quét PATH theo đuôi `.exe`/`.cmd`/`.bat` trên Windows, vì
`std::process::Command` **không** áp `PATHEXT` — `Command::new("npx")` báo "program not found"
khi trên đĩa chỉ có `npx.cmd`, và cả ba server mẫu đều gọi `npx`/`docker`.

#### Đã kiểm chứng được gì

Cổng: `cargo test` (**377 đạt / 0 trượt / 1 ignored**, trong đó 21 unit + 4 e2e là của G0),
`cargo clippy --all-targets -- -D warnings` (**0 warning**, đo bằng `--message-format=short`
rồi grep `": warning:"`), và `cargo check --all-targets --features experimental` (0 warning).

Một trong bốn e2e kiểm riêng lớp *dispatch*: ba arm `mcp_client:*` phải tồn tại trong
`handle_command` chứ không rơi vào nhánh `_ =>` "Unknown command" — loại lỗi chỉ lộ ra khi có
người gõ lệnh thật.

E2E `liva-native-core/tests/mcp_client_e2e.rs` spawn `scripts/e2e-mcp-server.mjs` — một MCP
server stdio thật, **cố tình thô bạo**: echo id kiểu số, chèn dòng không phải JSON, gửi
notification không ai yêu cầu, trả hồi âm ngược thứ tự gửi, trả `id: null`, và đổ ~400 KB ra
stderr bằng `writeSync` trước khi trả lời.

Cái cuối là kiểm chứng **thật** cho bẫy drain stderr, không phải kiểm chứng trên giấy: buffer
pipe của HĐH ~64 KB, nên client không drain sẽ chặn tiến trình con giữa đợt ghi. Đã đo bằng
cách tạm phá task drain — `tools/call` hết giờ đúng như dự đoán, và test đỏ. Khôi phục thì
xanh lại trong 0,49 s.

#### Đã chạy với MCP server ngoài THẬT — 26/07/2026

Chạy lại được: `node scripts/verify-mcp-real.mjs` (cần mạng lần đầu để `npx` tải package, nên
**không** vào CI). Script tự dựng `mcp_config.json` riêng trong thư mục tạm và trỏ vào bằng
`LIVA_MCP_CONFIG` — không đọc cấu hình riêng của máy nào, nên chạy được trên máy sạch.

Kiểm trên chuỗi đầy đủ `WebSocket → handle_command → McpClientRegistry → npx → server thật`,
không phải gọi hàm trong tiến trình. **15/15 đạt.** Hai server, cả hai không cần credential:

| Server | Kết quả đo |
|---|---|
| `@modelcontextprotocol/server-everything` 2026.7.4 | 13 tool sau 2,7–8,1 s (biến động theo npm cache). `serverInfo` = `mcp-servers/everything` 2.0.0, giao thức thoả thuận `2024-11-05` |
| `@modelcontextprotocol/server-filesystem` 2026.7.10 | 14 tool, nối **song song** cùng server trên; `read_file` đọc được file thật trên đĩa |

Bốn điều trước đây chỉ "suy ra", nay đã **đo**:

1. **`resolve_program` với `npx` thật.** Log: `đã spawn MCP server: C:\Program Files\nodejs\npx.cmd`
   — đúng cơ chế PATHEXT mà `std::process::Command` không tự làm.
2. **Bản sửa `mimeType`.** `get-tiny-image` trả PNG 5380 byte base64; khoá trên dây là
   `data,mimeType,type`, `mimeType=image/png`. Trước bản sửa nhánh này không thể parse.
3. **Lọc cấu hình trên file thật:** 5 mục khai báo → 3 mục dùng được (`_`-prefix và `disabled`
   bị loại), và nối lười đúng (`connected=false` cho tới lần gọi đầu).
4. **Server chết không còn im lặng.** Server cố tình crash sau khi ghi stderr: lỗi tới người
   gọi sau **36 ms** (`initialize lỗi -32000: server đã đóng stdout (EOF)`), kèm nguyên stack
   trace của nó ở mức `WARN`.

Điểm 4 là **lỗi do chính đợt kiểm này lộ ra**, và nó lớn hơn phạm vi MCP: bản đầu của
`spawn_stderr_drain` log ở `debug!`, nhưng **cả** `main.rs` **lẫn** vỏ Tauri đều dựng subscriber
bằng `.with_max_level(Level::INFO)` **cứng, không `EnvFilter`** — nên `RUST_LOG` bị bỏ qua và
**mọi `debug!` trong toàn crate là code chết**, không riêng MCP. Đo trực tiếp:
`server-filesystem` ghi `"Secure MCP Filesystem Server running on stdio"` (46 byte), drain đọc
được, log tuyệt đối im.

Đã sửa cả hai tầng:

1. **`crate::tracing_env_filter()`** — chính sách filter dùng CHUNG cho gateway và vỏ Tauri (cùng
   lý do `resolve_and_rekey` nằm ở `lib.rs`: không để hai vỏ trôi dạt). `RUST_LOG` không đặt →
   `info`, **giữ đúng hành vi cũ**; đặt và hợp lệ → dùng nguyên; đặt nhưng sai cú pháp →
   `eprintln!` cảnh báo rồi rơi về `info`, không âm thầm đổi hành vi log. Cần bật feature
   `env-filter` ở **cả hai** `Cargo.toml` — nó không phải default feature của
   `tracing-subscriber` 0.3, và chính chỗ thiếu đó là gốc của lỗi.
2. **Vòng đệm 20 dòng stderr cuối mỗi server**, in lại ở `WARN` khi server chết — vì người vận
   hành không thể bật debug *trước* lần crash.

Đo sau khi sửa: `RUST_LOG=info,liva_native_core::mcp=debug` cho **9 dòng DEBUG** của tầng mcp
(trước đó là 0 ở mọi cấu hình), gồm cả banner stderr của `server-everything`
(`"Starting default (STDIO) server..."`). Không đặt `RUST_LOG` thì hành vi y như cũ — script
kiểm chứng chạy cả hai chiều: **15/15** không có `RUST_LOG`, **16/16** khi có.

Lưu ý cú pháp `EnvFilter` để không mất công: directive tường minh **thay thế** mặc định, nên
`RUST_LOG=liva_native_core::mcp=debug` cho *chỉ* mcp và tắt phần còn lại. Muốn giữ cả info thì
`RUST_LOG=info,liva_native_core::mcp=debug`.

#### Vẫn chưa kiểm chứng được gì — đọc trước khi tin

- **Chưa chạy với server cần credential.** `server-postgres` (0.6.2) và `server-redis`
  (2025.4.25) trong file mẫu còn trên npm nhưng cần DB/Redis thật; `github-mcp-server` cần
  Docker + PAT. Ba mục đó vẫn chưa spawn lần nào.
- **Chưa có ai gọi 3 lệnh này trong đường chạy bình thường.** Không UI, không LLM — chỉ tới
  được qua WebSocket/IPC gõ tay hoặc script kiểm chứng. Vòng tool-calling là G1.
- Request server→client (`sampling`/`roots`) bị **bỏ qua có ý** — client khai báo
  `capabilities: {}` nên server đúng chuẩn không gửi.
- **Vỏ Tauri chỉ được COMPILE-CHECK**, chưa chạy thật với `RUST_LOG`. `cargo check -p liva-desktop`
  xanh (cổng CI), nhưng phần đo `debug!` ở trên là trên gateway (`main.rs`), không phải trong app
  Tauri. Cả hai dùng chung `tracing_env_filter()` nên rủi ro lệch là thấp — nhưng thấp không phải
  là đã đo.

### G1 — Vòng tool-calling **[ĐÃ XONG 26/07/2026, mặc định TẮT]**

`llm/tool_calling.rs` (mới) + nối vào `router` của `agent/graph.rs`. Bật bằng
`LIVA_TOOL_CALLING=1`.

**Cổng nghiệm thu: 13/13 trên CẢ HAI model.** `tool_calling_probe` + embedder
`multilingual-e5-small`. LLM chọn đúng `control_smarthome` với tham số **trùng khớp
`route_intent`** cho cả 10 câu smart-home (gồm tiếng Việt "bật đèn", "tắt máy lạnh"), và trả
`NONE` đúng cho cả 3 câu trò chuyện — trong đó có ca hồi quy `"let's get back on track"`.

| Model | Kết quả | Chi phí mỗi lượt (debug build) |
|---|---|---|
| `gemma-4-E4B-it-qat-UD-Q4_K_XL` (4,2 GB) | **13/13** | chưa đo |
| `Qwen3-VL-2B-Instruct-Q4_K_M` (1,1 GB) — **model router thực tế** | **13/13** | trung vị **2501 ms**, dải 1227–3709 ms |

Model 2B đạt đúng bằng model 4B là điều đáng chú ý: sau ba bản sửa ở §"bốn phát hiện", nhiệm vụ
này không còn cần model to. Prompt ~1111 ký tự (≈277 token) — vẫn thừa sức nằm trong `n_ctx` 4096 cùng persona, RAG và lịch
sử. Con số 1877 ms / 193 token là của bản mô tả tool ngắn, TRƯỚC 26/07 chiều; xem bảng ba biến
thể ở mục ngưỡng.

```powershell
.\target\debug\tool_calling_probe.exe                    # tầng 0+1, cần models/embedding
.\target\debug\tool_calling_probe.exe <đường dẫn .gguf>   # thêm tầng 2 = cổng thật
```

#### Bốn phát hiện, mỗi cái đổi một con số

Đường đi của cổng này là **0/13 → 4/13 → 3/13 → 13/13**. Từng bước:

1. **0/13 — prompt thô không qua chat template.** `generate_completion` nhận prompt trần thì
   gemma trả về **chuỗi rỗng** cho cả 13 câu. Trông y như "model không chọn được tool", trong
   khi thật ra model chưa hề được hỏi. Mọi caller khác trong crate đều qua `compile_prompt`;
   nay `compile_selection_prompt` bắt buộc điều đó.
2. **4/13 — schema không mang từ vựng.** `ControlSmartHomeArgs` là `{device: String, command:
   String}`, nên model sinh `"air conditioner"`, `"turn on"`, `"turn_on"` — **hợp lý với thông
   tin nó có**, sai với thứ `execute` nhận. Chọn tool đã đúng 13/13 ngay từ bước này; chỉ tham
   số sai. Đó là **schema thiếu thông tin, không phải model dở**.
3. **3/13 — `$ref` của schemars.** Sau khi thay `String` bằng enum thật, điểm *tụt*: schemars
   đặt mọi kiểu có tên vào `definitions` và để `properties` chỉ có `{"$ref": …}`, nên
   `render_params` không thấy `type` lẫn `enum` và in ra `any` — **ít thông tin hơn cả `String`
   trơn**. Giải ref xong thì prompt mang `action*: "on"|"off", device*: "light"|"ac"|"fan"`.
4. **13/13.**

Phát hiện 2 lộ ra một chỗ lệch có từ trước: `mcp/server.rs` giữ một **bản sao bị suy giảm** của
`smart_home::get_metadata()` — bên kia đã khai báo sẵn enum `device: ["light","ac","fan"]` /
`action: ["on","off"]`, còn bên này là `String` trơn và đặt tên `command` thay vì `action`. Nay
dùng lại đúng enum của `integrations::smart_home`, `action` là tên chuẩn, `command` giữ làm
`serde(alias)` để caller cũ không vỡ, và `call_tool` gọi thẳng `smart_home::execute` nên hai
đường trả về **cùng một câu** cho người dùng, không chỉ cùng tên tool.

#### Ba ràng buộc định hình thiết kế

- **`n_ctx` 4096 + model 2–4B** ⇒ truy hồi top-k (mặc định 4) bằng embedder, và render tham số
  **một dòng gọn** thay vì dump schema thô (một `schema_for!` đã ~200 token).
- **`generate_completion` không có grammar/JSON mode** ⇒ hợp đồng output là hai dòng có tiền tố
  và tool chọn **bằng SỐ**, không bằng tên. Bộ parse khoan dung với lời dẫn, code fence, chữ
  thường, thứ tự lộn; **không** khoan dung với số ngoài phạm vi và ARGS hỏng — hai ca đó rơi về
  `route_intent` thay vì chạy bừa.
- **Prompt injection** (§4) ⇒ **chọn** tool và **được phép chạy** tool là hai chuyện tách rời.
  `ExecPolicy` là allowlist: tool nội bộ không-ghi tự chạy được; `write_markdown` **không**, dù
  là tool nội bộ, vì ghi file do injection lái là thiệt hại không hoàn lại; mọi tool từ server
  ngoài chỉ **đề xuất**, mở bằng `LIVA_MCP_AUTOEXEC=server/tool` hoặc `server/*`.

#### Một lỗ trong bản G1 đầu, do người khác tìm ra — và đã bịt

Bản G1 gốc chỉ kiểm `ExecPolicy` trên **đường LLM chọn**. Hai lệnh IPC gọi tool **trực tiếp** thì
không kiểm gì, nên bất kỳ client nào nối được vào lớp lệnh đều gọi được `write_markdown` **và mọi
tool trên mọi server MCP ngoài**, bất kể `LIVA_TOOL_CALLING` bật hay tắt. Với WS 8002 chưa có xác
thực (chỉ allow-list `Origin`), "client nối được" là hàng rào mỏng.

Phát hiện này **không phải của tôi** — nó đến từ
[03-danh-gia/02 §C1.1](02-no-ky-thuat-va-rui-ro.md) (`d88508e`), và nó đúng: khi báo cáo G1 tôi
không nói rõ rằng `ExecPolicy` chỉ gác một trong hai đường.

Đã bịt bằng `tool_calling::guard_direct_call`, gọi ở **cả hai** arm:

| Nhánh | Trước | Nay |
|---|---|---|
| `mcp:call_tool` (6 tool nội bộ, đều ghim dưới vault) | không kiểm | `write_markdown` bị chặn; 5 tool còn lại vẫn qua |
| `mcp_client:call_tool` (**mọi** tool, **mọi** server ngoài) | không kiểm | mặc định **từ chối hết** |

Nhánh thứ hai nghiêm trọng hơn nhánh mà §C1.1 nêu ban đầu — nó tới được tiến trình `npx`/`docker`
của người lạ. Caller hợp pháp phải **khai báo quyền tường minh**; đó là lý do
`scripts/verify-mcp-real.mjs` và `tests/integration_tests.rs` nay đặt `LIVA_MCP_AUTOEXEC` kèm
comment giải thích, **không** phải cửa hậu cho test.

Đo, không suy luận: bỏ dòng đó khỏi script kiểm chứng làm 4 mục `call_tool` **đỏ ngay**
(15/15 → 11/15); đặt lại thì xanh. Cộng hai test hồi quy trong `tests/mcp_client_e2e.rs` chứng minh
hàng rào nằm **trong arm**, không chỉ tồn tại như một hàm.

**Không nhận nhiều hơn thực tế:** bản vá này chỉ đóng hai lệnh MCP. Mọi lệnh khác trên cùng đường
WS 8002 không xác thực vẫn mở (`llm:swap_model` là §C2). Bản vá đúng là allow-list lệnh theo kênh —
đề xuất (3) ở §C1, **vẫn chưa làm**.

#### Vì sao mặc định TẮT — nay là một số đo, không phải phỏng đoán

Cổng đúng/sai đã xanh trên cả hai model, nên lý do còn lại **chỉ là chi phí**: đo được **trung
vị 2501 ms** (dải 1227–3709) thêm vào *mỗi* câu chat trên `Qwen3-VL-2B`. Với trợ lý thoại đó là
~2,5 giây chờ cho mọi lượt nói, kể cả "hôm nay thế nào" — trả bằng trải nghiệm để đổi một
năng lực chưa có UI nào gọi tới.

`route_intent` đi **trước** (0 token) và làm **fallback** khi output LLM không đọc được, nên bật
G1 không làm chậm đường smart-home theo từ khoá — chỉ làm chậm đường *chat*, tức đúng phần đông
lượt nói.

Cảnh báo về phép đo: **build debug**. `Cargo.toml` gốc ghim `llama-cpp-2`/`llama-cpp-sys-2` ở
`opt-level = 3` ngay trong profile dev, nên phần suy luận — thứ chiếm gần hết thời gian — đã
được tối ưu; con số ở release khó khác nhiều. Nhưng chưa đo, nên đừng trích nó như số của
release.

#### Ngưỡng tiền lọc — đo ba lần, và lần thứ ba thì ĐƯỢC

> **Kết luận hiện tại (26/07/2026):** với mô tả tool ngắn, ngưỡng **không** dùng được. Sau khi
> tách văn bản-để-embed khỏi `description`, ngưỡng **tách bạch được** — khoảng trống 0,0159 trên
> corpus 20 câu. Ba lần đo ở dưới, theo đúng thứ tự đã chạy.
>
> | | (A) mô tả ngắn, gốc | (B) nhồi ví dụ vào `description` | (C) tách `embed_extra` |
> |---|---|---|---|
> | Cổng G1 (Qwen3-VL-2B) | 13/13 | 13/13 | **13/13** |
> | Ngưỡng điểm top-1 | ❌ chồng 3 ca | ❌ chồng 1 ca | ✅ **trống 0,0159** |
> | cần tool (n=9) | 0,8067–0,8591 | 0,8302–0,9051 | 0,8357–0,9116 |
> | trò chuyện (n=11) | 0,7745–0,8124 | 0,7721–0,8329 | 0,7695–**0,8198** |
> | `"mở quạt lên giúp mình"` | ❌ `read_markdown` | ✅ | ✅ (biên 0,0266) |
> | Prompt | ~193 token | ~417 | **~277** |
> | Độ trễ trung vị | 1877 ms | **3939 ms** | **2501 ms** |
> | Ngưỡng trên BIÊN (top1−top2) | ❌ chồng 6 | ❌ | ❌ chồng 7 |
>
> **(B) là một hồi quy tôi tự gây ra rồi tự đo ra:** nhồi ví dụ cách nói vào `description` sửa
> được truy hồi nhưng làm **đắt gấp đôi** đúng cái đang là nút cổ chai. Gốc là hai mục đích khác
> nhau bị nhồi vào một trường — ví dụ cách nói giúp *embedding* rất nhiều và giúp *LLM* gần như
> không (nó chỉ thấy 4 ứng viên).
>
> **(C)** tách chúng ra: `CatalogTool::embed_extra` chỉ vào chuỗi embed, **không bao giờ** vào
> prompt (có test canh đúng bất biến đó). Ví dụ cách nói nằm ở
> `NativeMcpServer::retrieval_examples()`. Tool từ server MCP ngoài để rỗng và hành xử y như cũ.
>
> **Vẫn KHÔNG được chốt hằng số vào code.** Khoảng trống 0,0159 đo trên **20 câu, một máy, một
> model embedding**. Đủ để nói "đáng làm tiếp", không đủ để hard-code `0.828`. Nếu cắm hằng số đó
> vào bây giờ, ca sai sẽ là **bỏ sót lệnh thật** — hướng sai đắt hơn hẳn hướng còn lại.

<details>
<summary>Ba lần đo, chi tiết (mở ra nếu cần đối chiếu)</summary>

**(A) — bản mô tả ngắn gốc: ngưỡng KHÔNG dùng được**

Giả thuyết: chỉ chạy lượt LLM khi truy hồi vượt một ngưỡng tương đồng, bỏ hẳn nó cho câu rõ ràng
là trò chuyện. Đo trên corpus 20 câu (9 cần tool: smart-home + đọc/ghi/tìm vault; 11 trò chuyện
thuần) bằng `tool_calling_probe`:

| Tín hiệu | Cần tool (n=9) | Trò chuyện (n=11) | Kết quả |
|---|---|---|---|
| Điểm cosine top-1 | 0,8067 … 0,8591 | 0,7745 … **0,8124** | **chồng nhau** — 3 câu trò chuyện ≥ câu cần-tool thấp nhất |
| Biên (top1 − top2) | 0,0013 … 0,0251 | 0,0001 … **0,0107** | **chồng nhau** — 6 câu |

**Cả hai đều chết.** Ba câu trò chuyện vượt ngưỡng dưới của nhóm cần-tool: "cảm ơn nhé" (0,8124),
"kể cho mình một chuyện vui" (0,8123), "mình tên gì nhỉ" (0,8109). Nên G1 **không** bật mặc định
được bằng cách này, và ~1,9 s không tránh được bằng một ngưỡng trên điểm embedder.

Hai điều dữ liệu này phơi ra, không phải thứ đi tìm:

1. **Toàn bộ điểm nằm trong 0,77–0,86.** Dải hẹp là bản chất họ E5 (cosine luôn cao) — nên ngưỡng
   **tuyệt đối** là ý tồi với model này về mặt cấu trúc, không chỉ với corpus này.
2. **Biên cực nhỏ với mọi câu** (0,0001–0,0251): với E5, cả 4 tool đều "hơi giống" bất kỳ câu nào
   như nhau. Gốc rễ là **mô tả tool quá ngắn và toàn tiếng Anh** (`"Control a smart home
   device"`) trong khi người dùng nói tiếng Việt. Đó chỉ ra hướng sửa thật: **viết mô tả tool dài
   hơn, song ngữ, kèm ví dụ cách nói** — nó cải thiện *cả* độ chính xác truy hồi *lẫn* biên, và
   đo lại được bằng đúng probe này. Chưa làm.

Một ca trượt cụ thể cùng nguyên nhân: `"mở quạt lên giúp mình"` cho top-1 là **`read_markdown`**
(0,8069), không phải `control_smarthome`. Hiện vô hại vì catalog chỉ có 4 tool và `top_k = 4` nên
tool đúng vẫn vào prompt — nhưng nó sẽ thành lỗi thật ngay khi catalog lớn lên, tức ngay ở G2.

**Điều (2) đã được làm và đo** — xem bảng ba biến thể ở đầu mục. Kết quả: mô tả đặc trưng hơn sửa
được ca `"mở quạt lên giúp mình"`, tăng biên ~4×, và kéo `search_vault` từ chỗ hút 8/11 câu trò
chuyện xuống còn 2/11. Nhưng "lực hút" **chuyển chỗ** chứ không mất: giờ 7/11 câu trò chuyện rơi
vào `control_smarthome` — mô tả giàu nhất. Đó là lý do ngưỡng trên **biên** vẫn không dùng được ở
cả ba biến thể, còn ngưỡng trên **điểm** thì được ở (C).

Một hệ quả về cách đọc probe: tầng 1 nay **chỉ phán quyết chiều dương** (câu smart-home phải cho
`control_smarthome` top-1). Chiều âm chỉ ghi nhận, không phán quyết — vì `top_k = 4` bằng đúng số
tool nội bộ nên MỌI tool vào prompt bất kể thứ hạng, và danh tính top-1 của một câu trò chuyện
không ảnh hưởng hành vi. Ví dụ: `"let's get back on track"` cho top-1 là `control_smarthome`
nhưng điểm 0,7695 — **thấp nhất cả corpus** — và LLM vẫn trả `NONE` đúng.

</details>

#### Chưa kiểm chứng

- **Chi phí ở build RELEASE chưa đo** (xem cảnh báo ở mục trên). Và chưa đo trên model nhỏ hơn
  2B, hay trên model ngoài hai cái đã thử.
- **Đường trùng token (khi thiếu embedder) là MÙ.** Đo được: 0 điểm cho *mọi* câu, kể cả tiếng
  Anh ("turn on the light" không chia token nào với "Control a smart home device"). Nó chỉ giữ
  cho code không sập, không phải một đường dùng được — **G1 trên thực tế CẦN embedder**. Với 4
  tool nội bộ chuyện này bị che vì `top_k = 4` bằng đúng số tool; thêm một server ngoài
  (`server-filesystem` có 14 tool) là `control_smarthome` bị đẩy khỏi top-4 với mọi câu.
- **Chưa đo với tool từ server MCP ngoài** trong catalog (`LIVA_TOOL_CALLING_SERVERS`).
- **Chưa có UI nào** hiển thị nhánh "chỉ đề xuất", nên `ProposeOnly` hiện chỉ chèn một câu vào
  hội thoại để LLM nói lại.

### ~~G1 — Vòng tool-calling~~ (mô tả kế hoạch ban đầu, giữ để đối chiếu)

Cho LLM chọn tool từ schema `tools/list` thay vì khớp từ khoá.

Ràng buộc quyết định thiết kế: `n_ctx` 4096 + model 2–4B → **không thể** nhét 50 schema vào
prompt. Vậy dùng chính embedder để truy hồi top-k tool, chỉ chèn ngần đó. Giữ `route_intent`
làm đường nhanh và fallback — nó rẻ, và đã xử lý đúng cách nói tiếng Việt ("bật đèn giúp mình").

**Cổng:** đường keyword và đường LLM phải khớp nhau trên corpus smart-home; giữ nguyên ca hồi
quy "back on track" không được hiểu thành lệnh bật điều hoà.

### G2 — Kho skill cục bộ, thuần Rust **[ĐÃ XONG 26/07/2026]**

`liva-native-core/src/skills/` — `loader.rs` (đọc `SKILL.md`), `store.rs` (3 bảng + DAG),
`ranker.rs` (BM25 → embedder rerank). Migration **4** trong `db.rs`. **30 test** (26 unit + 4 e2e
qua lớp lệnh).

| Hạng mục trong phạm vi | Trạng thái |
|---|---|
| Nhận định dạng thư mục `SKILL.md` | xong — front-matter `name`/`description`, **đúng khuôn Claude Code** |
| Ba bảng qua khung migration sẵn có | xong — `skills`, `skill_versions` (DAG qua `parent_id`), `skill_signals` |
| Truy hồi = BM25 + embedder | xong — BM25 tiền lọc, embedder rerank |

Ra ngoài qua **5 lệnh**: `skills:sync` · `skills:list` · `skills:search` · `skills:history` ·
`skills:pin_ids`. Có lệnh là điều kiện để kho **không phải code mồ côi** — thứ mà tài liệu §10 vừa
mất một đợt để dọn.

#### Ba quyết định đáng nói

**1. Danh tính là `.skill_id`, không phải `name` hay đường dẫn.** Đổi tên thư mục hay sửa `name:`
thì lịch sử và tín hiệu đã tích luỹ vẫn còn — đó là cả lý do OpenSpace có file này, và §2 nói
"lấy". Chưa ghim thì id **dẫn xuất tất định** từ `name` (nên quét lại không sinh bản ghi trùng),
nhưng **không** bền qua đổi `name:`; có một test ghi rõ giới hạn đó để không ai tưởng ngược lại.

**2. `load_*` THUẦN ĐỌC — ghim danh tính là lệnh riêng.** Bản đầu của loader tự sinh UUID rồi ghi
`.skill_id`. Hậu quả lộ ra ngay ở lần chạy test đầu: ca "đọc 7 skill thật trong `.claude/skills/`"
đã **tạo 7 file mới trong cây nguồn**. Một hàm tên `load_` mà sửa đĩa là bẫy — nó biến mọi lượt
quét, kể cả quét chỉ để xem, thành một thay đổi cần review. Nay `pin_skill_ids` / `skills:pin_ids`
là hành động có tên riêng, và có test hồi quy canh đúng chuyện `load` không ghi gì.

**3. BM25 là *recall booster*, không phải cửa chặn.** Đây là bài học đo được ở G1 áp sang: xếp
hạng theo trùng token trên mô tả **tiếng Anh** là **mù hoàn toàn** với câu **tiếng Việt** (0 điểm
mọi câu). Skill trong repo cũng mô tả bằng tiếng Anh. Nên nếu BM25 quyết định danh sách ứng viên
một cách cứng nhắc, bộ rerank **bị bỏ đói** — không bao giờ thấy skill đúng. Quy tắc tường minh:
BM25 ra quá ít ứng viên thì lấy **toàn bộ** skill rồi để embedder xếp. Có test cho đúng ca này.

"Ngưỡng tiền lọc 10" ở §2 được hiểu là **số ứng viên tối thiểu**, không phải ngưỡng điểm — vì điểm
tuyệt đối của E5 nằm trong dải hẹp nên ngưỡng điểm là ý tồi (đã đo ở G1).

#### Đã kiểm chứng

Cổng: `cargo test` **0** · `clippy --all-targets -D warnings` **0** ·
`check --all-targets --features experimental` **0** · `check -p liva-desktop` **0** (đo bằng
`LASTEXITCODE`, không qua `2>&1`).

Dữ liệu kiểm gồm **7 skill thật** trong `.claude/skills/` của repo này, không chỉ fixture tự viết
— nên định dạng được kiểm trên thứ tồn tại độc lập với code này.

#### Chưa làm — và một cái CỐ Ý chưa làm

- **CỐ Ý chưa nối skill vào prompt chọn tool của G1.** Ngân sách prompt ở G1 được đo với 6 tool
  (~295 token, +2380 ms). Thêm N skill đổi hẳn kinh tế của `top_k`, và nối bừa vào đó là thêm một
  hồi quy **chưa ai đo**. Đây là việc tiếp theo, và nó cần đúng `tool_calling_probe` để đo lại.
- **Chưa có thư mục `skills/` mặc định trong repo.** `LIVA_SKILLS_DIR` hoặc tham số `path` trỏ vào
  đâu cũng được; mặc định là `skills` và chưa tồn tại, nên `skills:sync` báo lỗi đọc được.
  Mặc định **không** phải `.claude/skills` — đó là cây của Claude Code, và LIVA ghi `.skill_id`
  vào thư mục skill nên không nên tự ý sửa cây của công cụ khác.
- **Chưa đo truy hồi skill với embedder thật** (chỉ có unit test dùng embedder giả). Cần một probe
  như `tool_calling_probe`.
- `skill_signals` **chỉ được dựng bảng và cho phép ghi**. Dùng tín hiệu làm prior khi xếp hạng là
  G3. Cột đã lấy đúng taxonomy §2 để G3 không phải migrate lại.

### G3 — Sổ cái chất lượng **[ĐÃ XONG 26/07/2026]**

Ghi một dòng mỗi lần gọi tool/skill theo taxonomy ở §2, rồi dùng nó làm **prior trong xếp hạng** —
đúng chỗ OpenSpace bỏ trống (§2.1). `liva-native-core/src/skills/signals.rs` (mới) +
`store.rs::signal_tallies` + `ranker.rs::rank_skills_with_prior`. **Không** cần migration: cột đã
lấy đúng taxonomy từ G2, đúng như dự tính ghi ở đó.

**+15 test** (12 unit ở `signals.rs`/`store.rs`, 3 e2e qua lớp lệnh) → tổng 45 unit + 7 e2e.

Ra ngoài qua **2 lệnh mới**: `skills:signal` (ghi) · `skills:signals` (đọc, kèm chính con số prior
đang dùng). `skills:search` giờ trả thêm `priorApplied`, `relevanceRank`, `qualityPenalty` — prior
phải **giải thích được**, không thì nó là một hộp đen đổi thứ tự tìm kiếm.

#### Ba quyết định đáng nói

**1. Đếm `merge_key` phân biệt, KHÔNG đếm dòng — và đây là một lỗi của G2 được tìm ra khi làm G3.**
G2 để lại `signal_counts` dùng `COUNT(*)`. Nhưng `merge_key` được chính G2 định nghĩa là "hai tín
hiệu cùng khoá là *cùng một vấn đề* quan sát nhiều lần". Nên `COUNT(*)` là con số **sai** cho prior:
một sự cố lặp 20 lần đọc thành 20 lỗi, đủ dìm chết một skill vốn chỉ có một vấn đề. `signal_tallies`
đếm khoá phân biệt; `signal_counts` giữ nguyên cho việc chẩn đoán ("chuyện này xảy ra mấy lần rồi?").
Hai lệnh `skills:signals` phơi **cả hai** con số cạnh nhau, vì chỗ chúng lệch nhau chính là thông tin.

Một cái bẫy SQL trong đó: `COUNT(DISTINCT merge_key)` **không đếm NULL**, mà cột cho phép NULL. Tín
hiệu chưa có khoá gộp là tín hiệu chưa ai gom ⇒ mỗi dòng là một vấn đề riêng. Thiếu nhánh
`SUM(CASE WHEN merge_key IS NULL ...)` thì cả nhóm đó biến mất khỏi prior — im lặng.

**2. Prior cộng trên THỨ HẠNG, không cộng trên điểm.** Điểm ở `rank_skills` là cosine (dải hẹp
0,77–0,91 **đo ở G1**) HOẶC BM25 (dải rộng 0…~10) tuỳ có embedder hay không. Một hằng số trừ vào
điểm sẽ **vô hình** ở thang này và **áp đảo** ở thang kia — cùng một tham số cho hai hành vi khác
hẳn. Cộng trên thứ hạng thì tham số đọc được và **có chặn trên**: một skill tệ nhất mức tụt nhiều
nhất 3 bậc, không bao giờ lật được một khoảng cách liên quan lớn. Truy hồi vẫn do liên quan quyết
định; chất lượng chỉ phá thế cân bằng.

Prior can thiệp **sau rerank, trước khi cắt `top_k`**. Đặt sau khi cắt thì nó chỉ đảo thứ tự *trong*
`top_k`, không bao giờ đẩy được một skill tệ ra khỏi kết quả — tức mất một nửa tác dụng, và là nửa
quan trọng hơn. Có test riêng cho đúng thứ tự này.

**3. Tín hiệu bị phản chứng KHÔNG trừ điểm.** `evidence_status = "refuted"` nhân trọng số 0;
`confirmed` = 1,0; chưa ghi = 0,5. Nếu `refuted` vẫn trừ thì một lời phàn nàn **đã được chứng minh
là sai** vẫn làm hỏng skill vĩnh viễn, và đường hồi phục duy nhất là đi xoá bản ghi — sổ cái trở
thành thứ phải dọn thay vì thứ đọc được.

#### Đã kiểm chứng

Cổng: `cargo test` **0** (419 lib + 7 e2e) · `clippy --all-targets -D warnings` **0** ·
`check --all-targets --features experimental` **0** · `check -p liva-desktop` **0**.

**Độ nhạy của prior được đo, không phải suy ra.** Test qua lớp lệnh ghim con số thật: **một** tín
hiệu `tool_failure_affects_skill` đã xác minh là **chưa đủ** lật một skill đang xếp nhất; **11 lần
quan sát cùng một `merge_key` vẫn chưa đủ**; **hai vấn đề phân biệt** mới lật. Đó là hệ quả số học
của `BAO_HOA = 2` và `LAMBDA_HANG = 3`, nên đổi hai hằng đó mà không đổi test là làm hỏng lặng lẽ.

**Hai quyết định trên được phủ định, không chỉ được test.** Sửa SQL về `COUNT(*)` ⇒ **3 test đỏ** ở
cả hai tầng (unit + dispatch). Sửa `refuted` từ 0 thành 1,0 ⇒ **2 test đỏ**. Cả hai đã phục hồi và
kiểm bằng `diff` là khớp byte với bản gốc.

#### Chưa làm — và giới hạn nằm ở schema

- **Chưa có ghi tín hiệu TỰ ĐỘNG.** Đây là giới hạn thật, không phải việc bỏ dở: cột `skill_id` là
  `NOT NULL REFERENCES skills(skill_id)`, nên mọi tín hiệu **phải** gắn với một skill. Mà
  `mcp:call_tool` thấy tool lỗi nhưng **không biết** skill nào đang tham gia — LIVA hiện chưa có
  đường nào *gọi* skill (việc nối skill vào prompt chọn tool của G1 vẫn là việc treo từ G2). Đoán hộ
  ở tầng đó là **gán tội sai**, và sổ cái sai còn tệ hơn sổ cái rỗng vì nó dịch chuyển truy hồi. Nên
  G3 dừng ở chỗ đúng: hạ tầng + lệnh, để người gọi — nơi duy nhất biết — quy trách.
- **Trọng số chưa được hiệu chuẩn trên dữ liệu thật.** Bốn trọng số kind (1,0 / 1,0 / 0,5 / 0,25),
  `BAO_HOA = 2`, `LAMBDA_HANG = 3` là phán đoán có lập luận, **không** phải kết quả đo trên log sử
  dụng — vì chưa có log sử dụng nào. Hiệu chuẩn được chỉ sau khi có ghi tự động.
- **Chưa dùng `actionability` và `failure_signature`.** Hai cột được ghi và đọc lại nguyên vẹn nhưng
  không vào công thức prior. `failure_signature` là thứ G4 cần để nạp vào sidecar OpenSpace.

### G4 — Tiến hoá: sidecar, không port

Cắm `openspace-mcp` như một MCP server tuỳ chọn, **allowlist chỉ `search_skills` + `fix_skill`**,
nạp `failure_signature` của LIVA vào. Mặc định tắt sau cờ env; cloud vô hiệu hoá cứng.

Phân vai lành mạnh: **OpenSpace đề xuất bản vá, `Sandbox` + `SelfCorrectionLoop` của LIVA chạy
test và quyết định nhận hay rollback.** Biên giới tin cậy ở lại trong Rust.

---

## 4. Rủi ro và cách chặn

| Rủi ro | Xử lý |
|---|---|
| **Rò dữ liệu** | Không đặt `OPENSPACE_CLOUD_API_KEY`; allowlist tool phải *loại* `cloud_*` và `upload_skill` — vô hiệu hoá cứng, không phải "không dùng tới" |
| **Chuỗi cung ứng skill** | Skill cộng đồng = shell script người lạ viết, chạy trong trợ lý có quyền điều khiển thiết bị và ghi file. Bắt buộc qua `Sandbox`; không bao giờ auto-exec skill vừa import |
| **Prompt injection** | Nội dung skill là *dữ liệu*, không phải lệnh. `SKILL.md` tải về không được phép lái vòng tool-calling ở G1 |
| **Ngân sách máy** | Python 3.12 + LiteLLM là tiến trình nặng. Opt-in, ngoài tiến trình (cô lập crash), **không bao giờ là build-dep của Rust core** |
| **Cổng CI** | clippy là hard gate 0 warning; đo bằng `--message-format=short` rồi grep `": warning:"` (grep `^src/` cho kết quả zero giả). Docs cần frontmatter `title/updated/commit/status` và trích dẫn `file:dòng` phải verify được |

---

## 5. Khuyến nghị

~~**Làm G0 ngay**, tách hẳn khỏi quyết định có dùng OpenSpace hay không.~~ **Xong 25/07/2026.**

~~Việc đáng làm tiếp **không phải G1**, mà là chạy G0 với một MCP server ngoài thật.~~
**Đã làm 26/07/2026** — và nó đúng như dự đoán: lộ ra một lỗi mà server mock không thể lộ (log
`debug!` vô hình vì `main.rs` hard-code `Level::INFO`, khiến stderr của server chết bị chôn).
Xem §3 G0.

~~1. Đổi subscriber sang `EnvFilter`.~~ **Xong 26/07/2026** — xem §3 G0. Nó mở lại `debug!` cho
**toàn crate**, không riêng MCP.

~~2. G1 — vòng tool-calling.~~ **Xong 26/07/2026** (mặc định TẮT) — xem §3 G1.

Việc đáng làm tiếp bây giờ, theo thứ tự:

1. ~~Đo G1 trên `Qwen3-VL-2B`.~~ **Xong — 13/13.** ~~Đo điểm cosine để tìm ngưỡng tiền lọc.~~
   **Xong — KHÔNG dùng được**, cả điểm tuyệt đối lẫn biên đều chồng nhau (§3 G1). Việc tiếp theo
   cho G1 là thứ dữ liệu đó chỉ ra: **viết lại mô tả 4 tool nội bộ cho dài hơn, song ngữ, kèm ví
   dụ cách nói**, rồi đo lại bằng . Nó cải thiện cả truy hồi lẫn biên.
2. ~~**G2 — kho skill cục bộ.**~~ **Xong 26/07/2026** — xem §3 G2.
3. ~~**G3 — sổ cái chất lượng.**~~ **Xong 26/07/2026** (hạ tầng + prior; ghi tự động thì chưa —
   xem §3 G3 "Chưa làm").

G2 và G3 làm LIVA mạnh lên kể cả khi không bao giờ chạm vào OpenSpace.

Việc đáng làm tiếp bây giờ là **thứ đang chặn cả hai đầu**: nối skill vào vòng tool-calling của G1.
Nó là việc treo từ G2 (§3 G2 "CỐ Ý chưa nối"), và giờ G3 cho thêm một lý do — không có đường *gọi*
skill thì không có tín hiệu tự động nào để ghi, nên prior của G3 chỉ chạy trên dữ liệu do người gọi
tự khai. Việc đó cần đúng `tool_calling_probe` để đo lại ngân sách prompt, vì thêm N skill đổi hẳn
kinh tế của `top_k`.

G4 chỉ nên xét **sau khi** sổ cái G3 có số liệu **tự động** trả lời được câu hỏi thật: *skill của
LIVA có fail đủ nhiều để đáng dựng cả cỗ máy tiến hoá không.* Lưu ý cái bẫy ở đây: G3 xong **không**
có nghĩa là câu hỏi đó đã trả lời được — sổ cái đang rỗng, và một sổ cái rỗng đọc giống hệt "không
có lỗi nào". Dựng G4 trước rồi đi tìm lý do là cách chắc chắn nhất để có thêm 1.4 MB phụ thuộc mà
không có thêm năng lực nào.
