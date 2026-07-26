---
title: "Đề xuất tích hợp OpenSpace (HKUDS)"
updated: 2026-07-25
commit: 2fb27c1
status: living
owns:
  - de-xuat-openspace-g0-g4
  - phan-ra-openspace-lay-va-tu-choi
covers:
  - liva-native-core/src/mcp/client.rs
  - liva-native-core/src/mcp/protocol.rs
  - liva-native-core/tests/mcp_client_e2e.rs
  - scripts/e2e-mcp-server.mjs
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

> **Cập nhật 25/07/2026 — G0 đã xong** (trong working tree tại HEAD `2fb27c1`, **chưa commit**).
> `mcp/client.rs` không còn là code mồ côi. Xem §3 G0 để biết cái gì đã đo được và cái gì chưa.
>
> **Nợ tài liệu G0 tạo ra, chưa trả.** Bốn tài liệu dưới đây vẫn mô tả trạng thái TRƯỚC G0 và
> nay **đã sai**; chúng đều có `liva-native-core/src/mcp/client.rs` trong `covers:`, nên
> `scripts/docs-check.mjs` sẽ tự gắn cờ lỗi thời ngay khi G0 được commit:
>
> | Tài liệu | Khẳng định đã sai |
> |---|---|
> | [01-ban-ve/05](../01-ban-ve/05-agent-bo-nho-va-tien-hoa.md) §1, §2, §8.4, §9.2 | `ProcessWrapper` "hoàn toàn mồ côi", node đỏ trong sơ đồ mermaid |
> | [01-ban-ve/09](../01-ban-ve/09-tich-hop-ngoai.md) §9.0, §9.1.1, §9.6 | "**[THIẾU]** — 0 caller", "wrapper process thô", bug `BufReader` mới mỗi lần đọc |
> | [01-ban-ve/10](../01-ban-ve/10-phu-thuoc-module-va-tra-cuu.md) §2, §3, §4 | "bốn thành phần mồ côi 1 311 dòng ≈ 7,0% crate" — `client.rs` không còn trong đó, và không còn 49 dòng |
> | [03-danh-gia/01](01-doi-chieu-tuyen-bo-vs-thuc-te.md) §113 | "`mcp/client.rs` (49 dòng) cũng không ai gọi" |
>
> Vì sao để lại: sửa đúng ba tài liệu đầu đòi **tính lại toàn bộ bảng kiểm kê mồ côi** (số
> dòng, phần trăm crate, node mermaid) — một việc riêng, không phải một dòng sửa. Ngoài ra §05
> và 03-danh-gia/01 đang có thay đổi chưa commit của phiên làm việc khác.

Đối tượng khảo sát: <https://github.com/HKUDS/OpenSpace> — MIT, ~6.9k sao, tạo 24/03/2026,
còn commit hằng ngày. Tự mô tả là "the skill management layer for AI agents": theo dõi chất
lượng skill từ kết quả task thật, tiến hoá skill theo ba trigger (FIX / DERIVED / CAPTURED),
lưu SQLite + version DAG, expose ra ngoài bằng MCP server. Python 3.12 + LiteLLM.

Prompt bàn giao để bắt tay làm G0: [openspace-g0-mcp-client-prompt.md](../04-quy-trinh/prompts/openspace-g0-mcp-client-prompt.md)

---

## 1. Hiện trạng LIVA — đã kiểm chứng trên mã tại commit `2fb27c1`

| Thành phần | Thực tế | Nhãn |
|---|---|---|
| MCP **server** | `mcp/server.rs` — `NativeMcpServer`, 4 tool: `read_markdown`, `write_markdown`, `search_vault`, `control_smarthome`. Ra ngoài qua `mcp:list_tools` / `mcp:call_tool` trong `lib.rs` | **[OK]** |
| MCP **client** | `mcp/client.rs` (rewrite 25/07/2026) — `McpStdioClient` + `McpClientRegistry`: handshake `initialize`→`notifications/initialized`, tương quan id qua `HashMap<String, oneshot::Sender>`, `tools/list` (có phân trang) + `tools/call`, drain stderr trong task riêng, timeout mỗi request, kill child khi drop, đọc `mcp_config.json`. Ra ngoài qua `mcp_client:list_servers` / `mcp_client:list_tools` / `mcp_client:call_tool` trong `lib.rs` | **[OK]** — với server *mock*; xem §3 G0 về giới hạn |
| Kiểu MCP | `mcp/protocol.rs` — `JsonRpcRequest/Response/Notification/Error`, `Tool`, `ToolList`, `CallToolRequest`, `CallToolResult`, `ToolContent`. Từ 25/07/2026 đã dùng được cho **cả hai chiều**: 4 attribute serde sửa chỗ khuôn-đọc lệch chuẩn MCP (xem §3 G0) | **[OK]** |
| Chọn hành động | `agent/graph.rs::route_intent` — khớp token cứng → `Intent{Vision, SmartHome, Chat}`. Comment trong mã tự ghi: chưa phải tool-calling do LLM sinh, "bước đó nằm ở lộ trình" | **[MỘT PHẦN]** |
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
2. **LIVA chưa có tầng skill nào để đo.** `route_intent` là bảng từ khoá cứng, không phải
   bộ chọn năng lực mở rộng được.

Cửa thứ hai vẫn đóng, nên mọi giá trị của OpenSpace vẫn nằm sau nó.

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

### G0 — MCP client thật **[ĐÃ XONG 25/07/2026]**

`ProcessWrapper` (49 dòng, mồ côi) → `mcp/client.rs` 1 034 dòng gồm test, cộng
`tests/mcp_client_e2e.rs` (442) và `scripts/e2e-mcp-server.mjs` (184).

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

Cổng: `cargo test` (**342 đạt / 0 trượt / 1 ignored**, trong đó 17 unit + 4 e2e là của G0),
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

#### Chưa kiểm chứng được gì — đọc trước khi tin

- **Chưa chạy với bất kỳ MCP server ngoài THẬT nào.** postgres/redis/github trong
  `mcp_config.example.json` chưa được spawn lần nào. Đường ống đã có, nhưng "postgres/github
  lập tức thành thật" hiện là **suy ra**, không phải đã đo.
- **`resolve_program` chỉ được kiểm bằng `cmd` → `cmd.exe`.** Việc `npx.cmd` spawn được suy ra
  từ hành vi có tài liệu của `std` (từ 1.77 nó gọi `.bat`/`.cmd` qua `cmd.exe` với escaping
  đúng), chưa đo trực tiếp.
- **Chưa có ai gọi 3 lệnh này.** Không UI, không LLM. Chúng chỉ tới được qua WebSocket/IPC gõ tay.
- Request server→client (`sampling`/`roots`) bị **bỏ qua có ý** — client khai báo
  `capabilities: {}` nên server đúng chuẩn không gửi.

### G1 — Vòng tool-calling

Cho LLM chọn tool từ schema `tools/list` thay vì khớp từ khoá.

Ràng buộc quyết định thiết kế: `n_ctx` 4096 + model 2–4B → **không thể** nhét 50 schema vào
prompt. Vậy dùng chính embedder để truy hồi top-k tool, chỉ chèn ngần đó. Giữ `route_intent`
làm đường nhanh và fallback — nó rẻ, và đã xử lý đúng cách nói tiếng Việt ("bật đèn giúp mình").

**Cổng:** đường keyword và đường LLM phải khớp nhau trên corpus smart-home; giữ nguyên ca hồi
quy "back on track" không được hiểu thành lệnh bật điều hoà.

### G2 — Kho skill cục bộ, thuần Rust

Nhận định dạng thư mục `SKILL.md`. Thêm bảng qua khung migration sẵn có: `skills`,
`skill_versions` (DAG qua `parent_id`), `skill_signals`. Truy hồi = BM25 + embedder hiện có.

Đây là chỗ LIVA có được **năng lực tích luỹ được** — thứ nó thiếu nhất hiện nay.

### G3 — Sổ cái chất lượng

Ghi một dòng mỗi lần gọi tool/skill theo taxonomy ở §2. Rồi dùng nó làm **prior trong xếp
hạng** — đúng chỗ OpenSpace bỏ trống. Nhỏ, đo được, không cần LLM.

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

Việc đáng làm tiếp **không phải G1**, mà là chạy G0 với một MCP server ngoài thật — cắm một
server `npx` vào `mcp_config.json` rồi gọi `mcp_client:list_tools`. Đó là phép thử rẻ nhất biến
"đã suy ra" thành "đã đo" ở đúng chỗ hiện còn suy ra (xem §3 G0), và nó sẽ lộ ra những chỗ lệch
khuôn mà server mock trong repo không thể lộ.

G2 và G3 làm LIVA mạnh lên kể cả khi không bao giờ chạm vào OpenSpace.

G4 chỉ nên xét **sau khi** G3 có số liệu trả lời được câu hỏi thật: *skill của LIVA có fail đủ
nhiều để đáng dựng cả cỗ máy tiến hoá không.* Dựng trước rồi đi tìm lý do là cách chắc chắn
nhất để có thêm 1.4 MB phụ thuộc mà không có thêm năng lực nào.
