#!/usr/bin/env node
//
// LIVA gateway — client tối giản, chạy được, 0 dependency
// =======================================================
//
// Đây là ví dụ đối chiếu cho §12 của
// `docs/01-ban-ve/02-giao-thuc-ipc-va-websocket.md`. Mục tiêu: một người chưa
// đọc repo sao chép file này, chạy, và thấy nó nói chuyện được với gateway —
// rồi mới đọc đặc tả đầy đủ.
//
// Cố ý KHÔNG import gì từ repo (kể cả `scripts/lib/ws-client.mjs`), vì một ví
// dụ mở đầu mà phải kéo theo một file khác của repo thì không còn là "sao chép
// một file rồi chạy" nữa. Đổi lại nó dài hơn: phần bắt tay + đọc frame chiếm
// khoảng hai phần ba. Client thật nên dùng thư viện WebSocket có sẵn.
//
// ## Chạy
//
//   # Cửa sổ 1 — giữ stdin MỞ. Lõi đọc stdin cho IPC và thoát khi gặp EOF;
//   # chạy nền với stdin đóng thì nó in "shutting down" rồi thoát 0, trông y
//   # hệt một lần chạy thành công.
//   $env:LIVA_SERVER_PORT="8099"; $env:LIVA_DB_IN_MEMORY="1"
//   .\target\debug\liva-native-core.exe
//
//   # Cửa sổ 2
//   node examples/gateway-quickstart.mjs          # mặc định cổng 8099
//   PORT=8002 node examples/gateway-quickstart.mjs
//
// Thoát 0 nếu cả ba bước đạt, 1 nếu có bước trượt.

import net from 'node:net'
import crypto from 'node:crypto'

const PORT = Number(process.env.PORT || 8099)
const HOST = '127.0.0.1'

// ─────────────────────────────────────────────────────────────────────────────
// BA ĐIỀU PHẢI BIẾT TRƯỚC KHI VIẾT DÒNG NÀO
// ─────────────────────────────────────────────────────────────────────────────
//
// 1. `Origin` bị lọc theo allow-list. Thiếu header này, hoặc gửi một origin lạ,
//    thì handshake trả về HTTP 403 chứ không phải 101 — và triệu chứng ở phía
//    client trông y hệt "server chưa chạy". Mặc định cho phép:
//    http://localhost:5173 · http://127.0.0.1:5173 · tauri://localhost ·
//    https://tauri.localhost. Mở rộng bằng biến LIVA_WS_ALLOWED_ORIGINS (CSV).
//
// 2. Giao thức TEXT là JSON `{event, payload}`. Hồi âm của lệnh `X` là
//    `X_response` khi thành công và `X_error` khi hỏng. Đừng đợi một tên khác.
//
// 3. Client BẮT BUỘC mask payload (RFC 6455 §5.3); server thì không. Quên mask
//    là bị ngắt kết nối mà không có thông báo nào dễ hiểu.
//
// VÀ MỘT ĐIỀU DỄ LÀM NGƯỜI MỚI TƯỞNG MÌNH SAI:
// Kết nối qua socket này mang principal `WebSocketRemote`, **không** phải toàn
// quyền. Nhiều lệnh bị chặn theo thiết kế — `mcp:*`, `vision:ask`… Và vì cổng
// phân quyền chạy TRƯỚC khi phân giải tên lệnh, một lệnh **gõ sai** cũng trả về
//
//     principal WebSocketRemote is not authorized for command '<tên bạn gõ>'
//
// chứ không phải "unknown command". Đọc câu đó là "sai tên HOẶC bị chặn", đừng
// đọc thành "tôi thiếu quyền". Muốn toàn quyền thì dùng đường IPC stdin, không
// phải WebSocket.
//
const ORIGIN = 'http://localhost:5173'

/** Mở kết nối. Trả `{ok, ws}` hoặc `{ok: false, ly}`. */
function ketNoi() {
  return new Promise((resolve) => {
    const sock = net.connect({ host: HOST, port: PORT })
    const nghe = new Map() // tên sự kiện → hàm xử lý
    let daBatTay = false
    let dem = Buffer.alloc(0)

    const hong = (ly) => { try { sock.destroy() } catch { /* đã đóng */ } resolve({ ok: false, ly }) }
    const gio = setTimeout(() => hong('quá hạn bắt tay'), 10_000)
    sock.on('error', (e) => { clearTimeout(gio); hong(String(e.message || e)) })

    sock.on('connect', () => {
      sock.write([
        'GET /ws HTTP/1.1',
        `Host: ${HOST}:${PORT}`,
        'Upgrade: websocket',
        'Connection: Upgrade',
        `Sec-WebSocket-Key: ${crypto.randomBytes(16).toString('base64')}`,
        'Sec-WebSocket-Version: 13',
        `Origin: ${ORIGIN}`,
        '', '',
      ].join('\r\n'))
    })

    sock.on('data', (chunk) => {
      dem = Buffer.concat([dem, chunk])

      if (!daBatTay) {
        const het = dem.indexOf('\r\n\r\n')
        if (het < 0) return // header chưa về đủ
        const ma = Number(dem.subarray(0, het).toString('latin1').split('\r\n')[0].split(' ')[1])
        dem = dem.subarray(het + 4)
        clearTimeout(gio)
        if (ma !== 101) return hong(`HTTP ${ma} (403 = Origin bị chặn)`)
        daBatTay = true
        resolve({ ok: true, ws: { goi, dong: () => sock.destroy() } })
      }

      // Một chunk TCP có thể chứa NHIỀU frame, hoặc nửa frame. Phải gom.
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
        if (dem.length < off + maskLen + len) return // frame chưa về đủ
        let than = dem.subarray(off + maskLen, off + maskLen + len)
        if (coMask) {
          const m = dem.subarray(off, off + 4)
          than = Buffer.from(than.map((b, i) => b ^ m[i % 4]))
        }
        dem = dem.subarray(off + maskLen + len)

        if (op === 0x8) { sock.destroy(); return }       // close
        if (op === 0x9) { khung(0xa, than); continue }   // ping → pong
        if (op === 0x2) continue                          // nhị phân: xem §5 của đặc tả
        if (op !== 0x1 || !fin) continue

        let d
        try { d = JSON.parse(than.toString('utf8')) } catch { continue }
        const xuLy = nghe.get(d.event)
        if (xuLy) { nghe.delete(d.event); xuLy(d) }
      }
    })

    function khung(opcode, payload) {
      const mask = crypto.randomBytes(4)
      const p = Buffer.from(payload)
      const che = Buffer.from(p.map((b, i) => b ^ mask[i % 4]))
      let dau
      if (p.length < 126) dau = Buffer.from([0x80 | opcode, 0x80 | p.length])
      else if (p.length < 65536) {
        dau = Buffer.alloc(4)
        dau[0] = 0x80 | opcode; dau[1] = 0x80 | 126; dau.writeUInt16BE(p.length, 2)
      } else {
        dau = Buffer.alloc(10)
        dau[0] = 0x80 | opcode; dau[1] = 0x80 | 127; dau.writeBigUInt64BE(BigInt(p.length), 2)
      }
      sock.write(Buffer.concat([dau, mask, che]))
    }

    /** Gửi một lệnh, đợi đúng `<lệnh>_response` HOẶC `<lệnh>_error` của nó. */
    function goi(lenh, payload = {}, hanGio = 20_000) {
      return new Promise((xong) => {
        const t = setTimeout(() => {
          nghe.delete(`${lenh}_response`); nghe.delete(`${lenh}_error`)
          xong({ event: null, ly: `không hồi âm trong ${hanGio}ms` })
        }, hanGio)
        const nhan = (d) => {
          clearTimeout(t)
          nghe.delete(`${lenh}_response`); nghe.delete(`${lenh}_error`)
          xong(d)
        }
        nghe.set(`${lenh}_response`, nhan)
        nghe.set(`${lenh}_error`, nhan)
        khung(0x1, Buffer.from(JSON.stringify({ event: lenh, payload }), 'utf8'))
      })
    }
  })
}

// ─────────────────────────────────────────────────────────────────────────────

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push(dat)
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

const main = async () => {
  console.log(`LIVA gateway: ws://${HOST}:${PORT}/ws\n`)

  const kn = await ketNoi()
  ghi('Bắt tay WebSocket', kn.ok, kn.ok ? `Origin: ${ORIGIN}` : kn.ly)
  if (!kn.ok) {
    console.log('\nGateway chưa chạy? Xem hướng dẫn ở đầu file.')
    return 1
  }
  const ws = kn.ws

  // Lệnh có thật, không cần model weights — hợp cho một lần bắt tay đầu tiên.
  const health = await ws.goi('llm:health_check')
  ghi('Lệnh hợp lệ trả *_response',
    health.event === 'llm:health_check_response',
    health.event ?? health.ly)

  // Lệnh sai tên PHẢI trả `*_error`, không được im lặng. Đây từng là một lỗi
  // thật: nhánh Err không gửi gì cả, nên client chỉ biết khi hết giờ chờ.
  // Lưu ý nội dung lỗi: nó nói về PHÂN QUYỀN, không nói "unknown command" —
  // xem khối "một điều dễ làm người mới tưởng mình sai" ở đầu file.
  const la = await ws.goi('lenh_khong_ton_tai')
  ghi('Lệnh sai tên trả *_error (không im lặng)',
    la.event === 'lenh_khong_ton_tai_error',
    la.payload?.error ?? la.event ?? la.ly)

  ws.dong()
  const trot = ket.filter(Boolean).length
  console.log(`\n${trot}/${ket.length} đạt`)
  return trot === ket.length ? 0 : 1
}

main().then((ma) => process.exit(ma))
