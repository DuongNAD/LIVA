#!/usr/bin/env node
// Kiểm chứng ĐẦU-CUỐI gateway LIVA qua WebSocket thật.
//
// Vì sao cần: mọi test khác trong repo là unit/integration trong tiến trình.
// Không cái nào chứng minh được rằng một client bên ngoài mở socket, gửi lệnh
// và nhận đúng hồi âm. Chính khoảng trống đó đã che một lỗi thật suốt thời
// gian dài: nhánh `Err` của `handle_command` không gửi gì cả, nên mọi lệnh
// thất bại biến mất im lặng và client chỉ biết khi hết giờ chờ.
//
// Script tự dựng client WebSocket bằng `node:net` để KHÔNG thêm dependency
// (`ws`) chỉ cho một bộ kiểm chứng. Chỉ hỗ trợ frame text, không nối mảnh,
// không nén — vừa đủ cho giao thức text của LIVA.
//
// ## Chạy
//
//   # 1. Khởi động gateway (giữ stdin mở — nó đọc stdin cho IPC và tắt khi EOF)
//   $env:LIVA_SERVER_PORT="8099"; $env:LIVA_DB_IN_MEMORY="1"
//   .\target\debug\liva-native-core.exe
//
//   # 2. Ở cửa sổ khác
//   node scripts/e2e-gateway.mjs            # mặc định cổng 8099
//   PORT=8002 node scripts/e2e-gateway.mjs
//
// Chạy được ở build DEBUG và nên thế: mọi assertion ở đây là về **giao thức và
// phân quyền**, không phụ thuộc model weights hay profile build.
//
// Bộ này KHÔNG đo năng lực thị giác. Từ `98efc55`, `vision:ask` bị chặn cho
// principal `WebSocketRemote`, nên ở kênh này nó chỉ chứng minh việc **từ chối
// đúng**. Đo thị giác thật: `scripts/e2e-vision-ipc.mjs` (đường IPC stdin).
//
// CI gọi qua `e2e-gateway-ci.mjs`, tự dựng tiến trình sống; các assertion giao thức
// không phụ thuộc model weights.
// Thoát 1 nếu có mục nào trượt.

import { ketNoi, goiLenh } from './lib/ws-client.mjs'

const PORT = Number(process.env.PORT || 8099)
const ORIGIN_OK = 'http://localhost:5173'
const ORIGIN_XAU = 'http://evil.example.com'

// ─── Bộ kiểm chứng ──────────────────────────────────────────────────────────

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

const main = async () => {
  console.log(`Gateway: ws://127.0.0.1:${PORT}/ws\n`)

  const xau = await ketNoi({ port: PORT, origin: ORIGIN_XAU })
  ghi('Origin lạ bị từ chối', !xau.ok, xau.ok ? 'ĐƯỢC NHẬN — allow-list hỏng!' : xau.ly)
  if (xau.ok) try { xau.ws.close() } catch { /* đã đóng */ }

  const tot = await ketNoi({ port: PORT, origin: ORIGIN_OK })
  ghi('Origin hợp lệ được nhận', tot.ok, tot.ok ? '' : tot.ly)
  if (!tot.ok) {
    console.log('\nGateway chưa chạy? Xem hướng dẫn ở đầu file.')
    return ket
  }
  const ws = tot.ws

  const health = await goiLenh(ws, 'llm:health_check')
  ghi('Lệnh chạy được trả *_response', health.event === 'llm:health_check_response',
    health.event ?? health.ly)

  // Đây là hồi quy của lỗi "nhánh Err bị nuốt": trước khi sửa, lệnh sai tên
  // không sinh ra BẤT KỲ thông điệp nào và client chỉ biết khi hết giờ.
  const la = await goiLenh(ws, 'khong_ton_tai_dau', {}, 15000)
  ghi('Lệnh không tồn tại trả *_error thay vì im lặng',
    la.event === 'khong_ton_tai_dau_error',
    la.event ? la.payload?.error : la.ly + '  ← ĐÂY LÀ LỖI CŨ TÁI PHÁT')
  ghi('Payload lỗi kèm tên lệnh và lý do',
    la.payload?.command === 'khong_ton_tai_dau' && typeof la.payload?.error === 'string')

  const mcp = await goiLenh(ws, 'mcp:list_tools')
  ghi('Remote principal bị chặn khỏi MCP',
    mcp.event === 'mcp:list_tools_error'
      && String(mcp.payload?.error).includes('not authorized'),
    mcp.payload?.error ?? mcp.event ?? mcp.ly)

  // ⚠️ ĐỌC TRƯỚC KHI "KHÔI PHỤC" BẢN CŨ CỦA KHỐI NÀY.
  //
  // Tới 01/08/2026 khối này khẳng định `vision:ask có hồi âm (không treo)` và
  // chấp nhận MỌI hồi âm, kể cả `_error`. Nó được viết cho hai profile: debug
  // trả lỗi "cần build release" trong vài ms, release trả mô tả ảnh thật.
  //
  // Từ `98efc55` (khép C1 — authorization theo principal) nó **đo rỗng**:
  // `vision:ask` không nằm trong allow-list của `WebSocketRemote`, nên hồi âm
  // luôn là lỗi phân quyền trả về sau ~1 ms. Test vẫn xanh, và cái xanh đó
  // không còn chứng minh gì về thị giác. Đo thật ngày 02/08 trên bản CUDA:
  // `1ms — lỗi: principal WebSocketRemote is not authorized for command
  // 'vision:ask'`. Nghiêm trọng hơn ba ca nhấp nháy cùng phiên, vì nó LUÔN
  // xanh nên không ai có lý do nghi ngờ.
  //
  // Sửa bằng cách đổi thứ khối này chứng minh, chứ KHÔNG cho e2e mượn quyền:
  // `sessions.issue(WebSocketRemote)` cố tình trả lỗi (`websocket.rs`), remote
  // không bao giờ nâng quyền được — đó là thiết kế đúng, không phải thiếu sót.
  // Nên ở kênh này, thứ đáng khẳng định là **vision bị từ chối đúng**.
  //
  // Năng lực thị giác thật được đo ở đường IPC stdin (principal `LocalCli`),
  // xem `scripts/e2e-vision-ipc.mjs`.
  const t0 = Date.now()
  const vis = await goiLenh(ws, 'vision:ask', { question: 'Trên màn hình có gì?' }, 30000)
  const dt = Date.now() - t0
  ghi('Remote principal bị chặn khỏi vision:ask',
    vis.event === 'vision:ask_error'
      && String(vis.payload?.error).includes('not authorized'),
    vis.payload?.error ?? vis.event ?? vis.ly)
  // Giữ lại hồi quy chống-treo: bug cũ làm client chờ hết 120 s vì nhánh Err
  // bị nuốt. Từ chối phân quyền phải xảy ra ở tầng định tuyến, trước mọi việc
  // nặng, nên ngân sách ở đây chặt hơn nhiều so với bản cũ.
  ghi('Không rơi vào kiểu treo-120s của bug cũ', vis.event !== null && dt < 5000,
    `${dt}ms (phải < 5s — từ chối xảy ra trước khi chụp màn hình)`)

  ws.close()
  return ket
}

main()
  .then((r) => {
    const truot = r.filter((x) => !x.dat).length
    console.log(`\n${r.length - truot}/${r.length} đạt`)
    process.exit(truot > 0 ? 1 : 0)
  })
  .catch((e) => { console.error('LỖI:', e); process.exit(1) })
