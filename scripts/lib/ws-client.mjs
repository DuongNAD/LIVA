// Client WebSocket tối giản cho các bộ kiểm chứng đầu-cuối (e2e-gateway,
// e2e-memory). Tách riêng để hai script không mang hai bản sao 100 dòng —
// bản sao là cách chắc chắn nhất để một bên được sửa còn bên kia thì không.
//
// Tự dựng bằng `node:net` để KHÔNG thêm dependency (`ws`) chỉ cho kiểm chứng.
// Chỉ hỗ trợ frame text, không nối mảnh, không nén — vừa đủ cho giao thức
// text của LIVA.

import net from 'node:net'
import crypto from 'node:crypto'
import { EventEmitter } from 'node:events'

export function ketNoi({ host = '127.0.0.1', port, path = '/ws', origin, timeout = 10000 }) {
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

/// Gửi một lệnh và đợi đúng `<lệnh>_response` / `<lệnh>_error` của nó.
export const goiLenh = (ws, lenh, payload = {}, hanGio = 25000) =>
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

/// Gửi một sự kiện rồi đợi tới khi thấy sự kiện tên `tenKetThuc` (ví dụ gửi
/// `user_voice_command` và đợi `ai_spoken_response`). Trả về payload của sự
/// kiện kết thúc; các sự kiện trung gian (ai_stream_chunk…) được đếm lại.
export const guiVaDoi = (ws, tenGui, payload, tenKetThuc, hanGio = 180000) =>
  new Promise((resolve) => {
    let soChunk = 0
    const t = setTimeout(() => {
      ws.off('message', nghe)
      resolve({ ok: false, ly: `không thấy ${tenKetThuc} trong ${hanGio}ms`, soChunk })
    }, hanGio)
    const nghe = (raw) => {
      let d
      try { d = JSON.parse(raw.toString()) } catch { return }
      if (d.event === 'ai_stream_chunk') soChunk++
      if (d.event === tenKetThuc) {
        clearTimeout(t); ws.off('message', nghe)
        resolve({ ok: true, payload: d.payload, soChunk })
      }
    }
    ws.on('message', nghe)
    ws.send(JSON.stringify({ event: tenGui, payload }))
  })
