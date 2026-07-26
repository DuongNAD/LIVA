#!/usr/bin/env node
// MCP server stdio TỐI GIẢN — bia thử cho MCP *client* của LIVA (rung G0).
//
// Vì sao cần: mọi test khác của tầng MCP gọi hàm trong tiến trình. Không cái nào
// chứng minh được rằng LIVA spawn được một tiến trình con thật, handshake xong,
// và ghép đúng hồi âm với đúng request khi chúng về ngược thứ tự. Cùng lý do
// `scripts/e2e-gateway.mjs` tồn tại cho tầng WebSocket.
//
// Server này KHÔNG phải server mẫu mực. Nó cố tình làm những việc mà một MCP
// server thật ngoài kia có quyền làm và client phải sống sót:
//
//   1. tool `noisy` đổ ~400KB ra stderr   → nếu client không drain, pipe đầy và
//      bằng `writeSync` rồi mới trả lời      tiến trình này TREO (bẫy số 1 của G0)
//   2. echo id của `tools/list` kiểu SỐ   → JSON-RPC cho phép; client nào ép id
//                                          là chuỗi sẽ mất hồi âm
//   3. chèn một dòng KHÔNG phải JSON      → vòng đọc của client không được chết
//   4. gửi notification không ai yêu cầu  → phải bỏ qua, không nhầm là hồi âm
//   5. tool `slow` trả về SAU tool gọi     → chứng minh tương quan id thật sự
//      sau nó
//   6. tool `nullid` trả `id: null`        → lỗi tầng giao thức, phải tới được
//                                          người gọi thay vì để họ chờ timeout
//   7. tool `no_desc` thiếu `description`  → trường tuỳ chọn trong MCP
//   8. tool `weird` trả content lạ         → một phần tử lạ không được làm hỏng
//                                          cả lời gọi
//
// Chạy tay để xem nó nói gì:
//   echo {"jsonrpc":"2.0","id":"1","method":"initialize","params":{}} | node scripts/e2e-mcp-server.mjs
//
// Bình thường thì `liva-native-core/tests/mcp_client_e2e.rs` spawn nó.

import { createInterface } from 'node:readline'
import { writeSync } from 'node:fs'

const ghiLog = (msg) => process.stderr.write(`[mock-mcp] ${msg}\n`)

// Đổ ĐỦ NHIỀU stderr để làm đầy pipe (buffer HĐH ~64KB) rồi mới trả lời.
//
// `writeSync(2, …)` là cố ý, không dùng `process.stderr.write`: trên Windows
// stderr dạng pipe của Node là async nên nó tự đệm trong tiến trình và không
// bao giờ block — tức không kiểm được gì. `writeSync` block thật khi pipe đầy.
//
// Client KHÔNG drain stderr sẽ treo tiến trình này ở đây vô hạn, và người gọi
// chỉ biết khi hết timeout 30s. Đó chính là bẫy số 1 của G0, và đây là chỗ duy
// nhất trong repo bắt được nó.
const bomStderr = (soDong) => {
  const dong = `[mock-mcp] rac de lam day pipe stderr ${'x'.repeat(120)}\n`
  for (let i = 0; i < soDong; i += 1) writeSync(2, dong)
}

const guiThô = (obj) => process.stdout.write(`${JSON.stringify(obj)}\n`)
const guiKetQua = (id, result) => guiThô({ jsonrpc: '2.0', id, result })
const guiLoi = (id, code, message) => guiThô({ jsonrpc: '2.0', id, error: { code, message } })

// Ba tool đủ để phủ các ca deserialize đáng lo.
const TOOLS = [
  {
    name: 'echo',
    description: 'Trả lại nguyên arguments dưới dạng text',
    inputSchema: { type: 'object', properties: { a: { type: 'number' } } },
  },
  // CỐ TÌNH thiếu `description` và `inputSchema`: cả hai là tuỳ chọn ở phía
  // server, và trước G0 thì thiếu một trong hai làm cả `tools/list` fail.
  { name: 'no_desc' },
  {
    name: 'slow',
    description: 'Trả lời sau 400ms — để kiểm tra hồi âm về ngược thứ tự',
    inputSchema: { type: 'object' },
  },
]

const noiDungText = (text) => ({ content: [{ type: 'text', text }], isError: false })

const xuLyGoiTool = (id, params) => {
  const ten = params?.name
  const args = params?.arguments ?? {}

  switch (ten) {
    case 'echo':
      guiKetQua(id, noiDungText(JSON.stringify(args)))
      return

    case 'no_desc':
      guiKetQua(id, noiDungText('khong co mo ta'))
      return

    case 'slow':
      // Không await: hồi âm này về SAU hồi âm của request gửi sau nó. Đây chính
      // là điều kiện làm client không có tương quan id trả sai dữ liệu.
      setTimeout(() => guiKetQua(id, noiDungText('cham nhung dung')), 400)
      return

    case 'boom':
      guiLoi(id, -32001, 'tool nay luon that bai')
      return

    case 'img':
      // `mimeType` (camelCase) — đúng khuôn MCP trên dây.
      guiKetQua(id, {
        content: [{ type: 'image', data: 'QUJD', mimeType: 'image/png' }],
        isError: false,
      })
      return

    case 'weird':
      // `resource` là loại content hợp lệ trong MCP mà client này chưa dùng.
      // Phần text bên cạnh vẫn phải tới được người gọi.
      guiKetQua(id, {
        content: [
          { type: 'resource', resource: { uri: 'file:///x', text: 'noi dung' } },
          { type: 'text', text: 'phan text van con' },
        ],
        isError: false,
      })
      return

    case 'noisy':
      // ~2500 × 160B ≈ 400KB, gấp nhiều lần buffer pipe.
      bomStderr(2500)
      guiKetQua(id, noiDungText('da bom stderr xong ma khong treo'))
      return

    case 'nullid':
      // Lỗi tầng giao thức: server chưa/không gán được id. Client phải giao nó
      // cho request duy nhất đang bay, không để người gọi chờ hết timeout.
      guiThô({ jsonrpc: '2.0', id: null, error: { code: -32700, message: 'khong doc duoc id' } })
      return

    case 'tool_error':
      // Lỗi Ở TRONG tool (khác lỗi giao thức): vẫn là result, cờ isError.
      guiKetQua(id, { content: [{ type: 'text', text: 'tool bao loi' }], isError: true })
      return

    default:
      guiLoi(id, -32602, `khong co tool '${ten}'`)
  }
}

const xuLy = (msg) => {
  const { id, method, params } = msg
  ghiLog(`nhan ${method} (id=${JSON.stringify(id)})`)

  switch (method) {
    case 'initialize':
      guiKetQua(id, {
        protocolVersion: params?.protocolVersion ?? '2024-11-05',
        capabilities: { tools: {} },
        serverInfo: { name: 'liva-mock-mcp', version: '0.1.0' },
      })
      return

    case 'notifications/initialized':
      // Hai dòng rác ngay sau handshake, đúng lúc client bắt đầu gửi `tools/*`:
      // một dòng không phải JSON và một notification không ai yêu cầu. Vòng đọc
      // của client phải bỏ qua cả hai và tiếp tục chạy.
      process.stdout.write('day khong phai JSON\n')
      guiThô({ jsonrpc: '2.0', method: 'notifications/message', params: { level: 'info' } })
      return

    case 'tools/list':
      // Echo id kiểu SỐ nếu nó là chuỗi số. Client của LIVA phát id thập phân
      // dạng chuỗi, nên nó phải chuẩn hoá lại mới khớp được khoá đang chờ.
      guiKetQua(/^\d+$/.test(String(id)) ? Number(id) : id, { tools: TOOLS })
      return

    case 'tools/call':
      xuLyGoiTool(id, params)
      return

    default:
      guiLoi(id, -32601, `khong ho tro method '${method}'`)
  }
}

ghiLog('san sang (stdio)')

createInterface({ input: process.stdin }).on('line', (line) => {
  const text = line.trim()
  if (!text) return
  try {
    xuLy(JSON.parse(text))
  } catch (e) {
    ghiLog(`bo dong khong phan tich duoc: ${e.message}`)
  }
})
