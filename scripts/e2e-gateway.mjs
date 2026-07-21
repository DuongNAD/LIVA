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
// Chạy được ở build DEBUG và nên thế: `vision:ask` thất bại ngay ở debug, đúng
// kịch bản trước đây khiến client treo 120 giây.
//
// KHÔNG nằm trong CI: cần model weights (gitignored) và một tiến trình sống.
// Thoát 1 nếu có mục nào trượt.

import net from 'node:net'
import crypto from 'node:crypto'
import { EventEmitter } from 'node:events'

const PORT = Number(process.env.PORT || 8099)
const ORIGIN_OK = 'http://localhost:5173'
const ORIGIN_XAU = 'http://evil.example.com'

// ─── Client WebSocket tối giản ──────────────────────────────────────────────

function ketNoi({ host = '127.0.0.1', port, path = '/ws', origin, timeout = 10000 }) {
  return new Promise((resolve) => {
    const bus = new EventEmitter()
    const key = crypto.randomBytes(16).toString('base64')
    const sock = net.connect({ host, port })
    let daBatTay = false
    let dem = Buffer.alloc(0)

    const hong = (ly) => { try { sock.destroy() } catch { /* đã đóng */ } ; resolve({ ok: false, ly }) }
    const gio = setTimeout(() => hong('timeout bắt tay'), timeout)
    sock.on('error', (e) => { clearTimeout(gio); hong(String(e.message || e)) })

    sock.on('connect', () => {
      sock.write([
        `GET ${path} HTTP/1.1`,
        `Host: ${host}:${port}`,
        'Upgrade: websocket',
        'Connection: Upgrade',
        `Sec-WebSocket-Key: ${key}`,
        'Sec-WebSocket-Version: 13',
        ...(origin ? [`Origin: ${origin}`] : []),
        '', '',
      ].join('\r\n'))
    })

    sock.on('data', (chunk) => {
      dem = Buffer.concat([dem, chunk])

      if (!daBatTay) {
        const het = dem.indexOf('\r\n\r\n')
        if (het < 0) return
        const status = Number(dem.subarray(0, het).toString('latin1').split('\r\n')[0].split(' ')[1])
        dem = dem.subarray(het + 4)
        clearTimeout(gio)
        if (status !== 101) return hong('HTTP ' + status)
        daBatTay = true
        resolve({ ok: true, ws: { on: bus.on.bind(bus), off: bus.off.bind(bus), send: gui, close: () => sock.destroy() } })
      }

      // Vòng lặp: một chunk TCP có thể chứa nhiều frame, hoặc nửa frame.
      for (;;) {
        if (dem.length < 2) return
        const fin = (dem[0] & 0x80) !== 0
        const op = dem[0] & 0x0f
        const coMask = (dem[1] & 0x80) !== 0
        let len = dem[1] & 0x7f
        let off = 2
        if (len === 126) { if (dem.length < 4) return; len = dem.readUInt16BE(2); off = 4 }
        else if (len === 127) { if (dem.length < 10) return; len = Number(dem.readBigUInt64BE(2)); off = 10 }
        const maskLen = coMask ? 4 : 0
        if (dem.length < off + maskLen + len) return
        let body = dem.subarray(off + maskLen, off + maskLen + len)
        if (coMask) {
          const m = dem.subarray(off, off + 4)
          body = Buffer.from(body.map((b, i) => b ^ m[i % 4]))
        }
        dem = dem.subarray(off + maskLen + len)
        if (op === 0x8) { sock.destroy(); bus.emit('close'); return }
        if (op === 0x9) { guiFrame(0xa, body); continue }
        if (op === 0x1 && fin) bus.emit('message', body)
      }
    })

    // Client BẮT BUỘC mask payload (RFC 6455 §5.3); server thì không.
    function guiFrame(opcode, payload) {
      const mask = crypto.randomBytes(4)
      const p = Buffer.from(payload)
      const masked = Buffer.from(p.map((b, i) => b ^ mask[i % 4]))
      let head
      if (p.length < 126) head = Buffer.from([0x80 | opcode, 0x80 | p.length])
      else if (p.length < 65536) {
        head = Buffer.alloc(4)
        head[0] = 0x80 | opcode; head[1] = 0x80 | 126; head.writeUInt16BE(p.length, 2)
      } else {
        head = Buffer.alloc(10)
        head[0] = 0x80 | opcode; head[1] = 0x80 | 127; head.writeBigUInt64BE(BigInt(p.length), 2)
      }
      sock.write(Buffer.concat([head, mask, masked]))
    }
    function gui(text) { guiFrame(0x1, Buffer.from(text, 'utf8')) }
  })
}

// ─── Bộ kiểm chứng ──────────────────────────────────────────────────────────

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

// Hạn giờ RIÊNG và NGẮN cho mỗi lệnh: nếu đường lỗi hỏng trở lại, bộ kiểm phải
// đỏ nhanh chứ không treo — treo là đúng triệu chứng của cái bug này.
const goiLenh = (ws, lenh, payload = {}, hanGio = 25000) =>
  new Promise((resolve) => {
    const t = setTimeout(() => { ws.off('message', nghe); resolve({ event: null, ly: `không hồi âm trong ${hanGio}ms` }) }, hanGio)
    const nghe = (raw) => {
      let d
      try { d = JSON.parse(raw.toString()) } catch { return }
      if (d.event === `${lenh}_response` || d.event === `${lenh}_error`) {
        clearTimeout(t); ws.off('message', nghe); resolve(d)
      }
    }
    ws.on('message', nghe)
    ws.send(JSON.stringify({ event: lenh, payload }))
  })

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
  ghi('MCP server đã nối vào lớp lệnh',
    mcp.event === 'mcp:list_tools_response' && Array.isArray(mcp.payload?.tools),
    Array.isArray(mcp.payload?.tools) ? `${mcp.payload.tools.length} tool` : (mcp.event ?? mcp.ly))

  const t0 = Date.now()
  const vis = await goiLenh(ws, 'vision:ask', { question: 'Trên màn hình có gì?' }, 30000)
  const dt = Date.now() - t0
  const loi = vis.event === 'vision:ask_error'
  // Ở build release có model thì đây là `_response`; ở debug là `_error`. Cả
  // hai đều ĐẠT — điều cần chứng minh là có hồi âm, không phải im lặng.
  ghi('vision:ask có hồi âm (không treo)', vis.event !== null,
    `${dt}ms, ${loi ? 'lỗi: ' + String(vis.payload?.error).slice(0, 70) : 'trả lời thành công'}`)
  ghi('Hồi âm nhanh hơn hẳn timeout 120 s của UI', vis.event !== null && dt < 30000, `${dt}ms`)

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
