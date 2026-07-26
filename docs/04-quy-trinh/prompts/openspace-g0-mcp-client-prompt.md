# Prompt bàn giao — G0: MCP client thật cho LIVA

> Dán nguyên khối dưới đây vào một session LIVA mới. Đã tự chứa: không cần đọc lại
> lịch sử hội thoại nào. Bối cảnh đầy đủ nằm ở
> `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md`.

---

## PROMPT

Làm **G0 — MCP client thật** cho `liva-native-core`, theo
`docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md`. Đọc file đó và `AGENTS.md` trước khi gõ
dòng code nào.

### Sự thật đã kiểm chứng ở commit `2fb27c1` — đừng khảo sát lại từ đầu

- `src/mcp/client.rs` chỉ có `ProcessWrapper`: spawn + write-line + read-line. **Không** handshake
  `initialize`, **không** tương quan id, **không** `tools/list`, **không** drain stderr. Và nó
  **không được tham chiếu ở bất kỳ đâu trong repo** — đang là code chết. Đây là file cần thay.
- `src/mcp/protocol.rs` **đã có đủ kiểu**: `JsonRpcRequest`, `JsonRpcResponse`, `JsonRpcNotification`,
  `JsonRpcError`, `Tool`, `ToolList`, `CallToolRequest`, `CallToolResult`, `ToolContent`.
  **Dùng lại, đừng định nghĩa kiểu mới song song.**
- Mẫu tương quan id đã có sẵn để bắt chước: `pending_replies:
  Arc<Mutex<HashMap<String, oneshot::Sender<_>>>>` trong `src/agent/dispatcher.rs`
  (`SwarmAgent::request_reply_internal`), kèm cả cách xử lý reply muộn/trùng.
- LIVA hiện là MCP **server**: `src/mcp/server.rs` (`NativeMcpServer`, 4 tool) expose qua
  `mcp:list_tools` / `mcp:call_tool` trong `lib.rs`. Client là chiều còn thiếu, không phải chiều này.
- `mcp_config.example.json` ở gốc repo **không có bộ đọc nào phía Rust**. Hiện là trang trí.

### Phạm vi — LÀM

1. MCP client stdio hoàn chỉnh trong `src/mcp/client.rs`:
   - handshake `initialize` → nhận capabilities → gửi `notifications/initialized`
   - tương quan id request↔response bằng map + `oneshot`
   - `tools/list` và `tools/call`, trả về đúng kiểu trong `protocol.rs`
   - **drain stderr của child vào `tracing`** trong task riêng
   - timeout mỗi request + vòng đời child tử tế (kill khi drop)
2. Bộ đọc `mcp_config.json` (khuôn `mcp_config.example.json`: `mcpServers.{name}.{command,args,env}`).
3. Nối vào `handle_command` như các lệnh mới, đặt tên nhất quán với cặp có sẵn — đề xuất
   `mcp_client:list_servers`, `mcp_client:list_tools`, `mcp_client:call_tool`.
4. Test: unit cho tương quan id + parse lỗi; **e2e spawn một stdio server tối giản** theo khuôn
   `scripts/e2e-gateway.mjs`.

### Phạm vi — KHÔNG LÀM

- **Không** làm vòng tool-calling do LLM sinh (đó là G1). Không sửa `route_intent` trong `agent/graph.rs`.
- **Không** cài OpenSpace, không thêm bất kỳ phụ thuộc Python nào. G0 độc lập hoàn toàn với OpenSpace.
- **Không** thêm bảng DB (đó là G2).
- **Không** `git commit` / `push` / `pull` — theo `AGENTS.md`, chỉ khi tôi yêu cầu tường minh.

### Bẫy đã biết — đọc trước khi debug

- **Drain stderr là bắt buộc, không phải tuỳ chọn.** Server con ghi đầy pipe stderr rồi treo, và
  treo kiểu đó trông y hệt "đang suy nghĩ". Đây là lỗi sẽ mất một buổi nếu bỏ qua.
- **`JsonRpcResponse.id` hiện là `String` không `Option`.** Theo JSON-RPC 2.0, response lỗi có thể
  mang `id: null`, và MCP cho phép id là số. Cả hai trường hợp sẽ làm serde parse fail và biểu hiện
  ra như "server không trả lời". Xử lý tường minh, và nếu phải sửa kiểu trong `protocol.rs` thì kiểm
  luôn `mcp/server.rs` không vỡ theo.
- **`initialize` phải xong trước mọi `tools/*`.** Gọi sớm thì server trả lỗi khó đọc.
- Chạy core thủ công: **giữ stdin MỞ**. Core đọc stdin cho IPC và thoát khi EOF — background nó với
  stdin đóng sẽ in "shutting down" rồi exit 0, trông đúng như một lần chạy thành công.

### Cổng nghiệm thu — phải xanh hết

```powershell
cd liva-native-core
cargo test
cargo clippy --all-targets -- -D warnings
```

- clippy là **hard gate 0 warning**. Đo bằng `--message-format=short` rồi grep `": warning:"` —
  grep `^src/` cho ra zero giả vì đường dẫn có tiền tố `liva-native-core\src\…`.
- Sau khi code xanh, cập nhật `docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md`: đổi nhãn dòng
  "MCP client" ở §1 từ **[THIẾU]** sang trạng thái đúng, rồi cập nhật `updated:` và `commit:` trong
  front-matter. Cuối cùng chạy:

```powershell
node scripts/docs-check.mjs --map
node scripts/docs-citations.mjs
```

### Báo cáo lại cho tôi

Nói rõ ba điều, đừng gộp thành "đã xong": (1) cổng nào đã chạy và kết quả thật, (2) chỗ nào phải
sửa `protocol.rs` và tại sao, (3) phần nào của phạm vi bị bỏ lại và lý do.
