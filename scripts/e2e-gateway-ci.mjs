#!/usr/bin/env node
// Chạy gateway THẬT rồi cho `e2e-gateway.mjs` kiểm nó — một lệnh, không cần
// hai cửa sổ terminal.
//
// Vì sao cần: `e2e-gateway.mjs` là bộ kiểm chứng DUY NHẤT chạm tới lớp dispatch
// qua một socket thật (mọi test khác gọi `handle_command` trong tiến trình và
// không bao giờ đụng ổ cắm — đó chính là khoảng trống từng che một nhánh `Err`
// bị nuốt). Nhưng nó đòi một tiến trình sống ở cửa sổ khác, nên **CI không chạy
// nó**, nên khoảng trống ấy vẫn còn nguyên trên đường tự động.
//
// Đầu file `e2e-gateway.mjs` từng ghi "KHÔNG nằm trong CI: cần model weights".
// **Đo lại 26/07/2026: sai.** Cho mọi biến model trỏ vào đường dẫn không tồn
// tại rồi chạy → vẫn **8/8 đạt**. Lõi khởi động được mà không cần weight nào
// (LLM autoload chỉ log WARN, TTS rơi xuống backend khác, STT/embedder tắt êm),
// và cả 8 mục kiểm đều nói về giao thức chứ không về chất lượng model. Thứ duy
// nhất thật sự cần là `vec0` — do npm `sqlite-vec` cung cấp, tức là `npm ci` đã
// có sẵn trên CI.
//
// Dùng:
//   node scripts/e2e-gateway-ci.mjs                       # build debug
//   node scripts/e2e-gateway-ci.mjs --release             # build release
//   node scripts/e2e-gateway-ci.mjs --bin path/to/exe --port 8099
//
// Thoát bằng đúng mã thoát của `e2e-gateway.mjs`.

import { spawn } from 'node:child_process'
import net from 'node:net'
import path from 'node:path'
import fs from 'node:fs'

const ROOT = path.resolve(import.meta.dirname, '..')
const argv = process.argv.slice(2)
const lay = (ten) => {
  const i = argv.indexOf(ten)
  return i > -1 ? argv[i + 1] : null
}

const profile = argv.includes('--release') ? 'release' : 'debug'
const BIN = path.resolve(
  ROOT,
  lay('--bin') || path.join('target', profile, 'liva-native-core.exe'),
)
// Cổng riêng, KHÔNG phải 8002: nếu lỡ có một LIVA khác đang chạy trên máy dev
// thì bộ kiểm sẽ lặng lẽ kiểm nhầm tiến trình đó và báo xanh cho mã chưa build.
const PORT = Number(lay('--port') || 8099)

if (!fs.existsSync(BIN)) {
  console.error(`Không thấy binary: ${BIN}`)
  console.error(`Build trước:  cargo build ${profile === 'release' ? '--release ' : ''}--bin liva-native-core`)
  process.exit(1)
}

const cho = (ms) => new Promise((r) => setTimeout(r, ms))
const congMo = (port) =>
  new Promise((r) => {
    const s = net.connect({ host: '127.0.0.1', port })
    s.on('connect', () => {
      s.destroy()
      r(true)
    })
    s.on('error', () => r(false))
  })

// TIỀN KIỂM, và nó không phải là nghi lễ: lần chạy đầu của script này đã báo
// "8/8 đạt" cho binary RELEASE trong khi thật ra đang nói chuyện với một tiến
// trình DEBUG còn sót trên cùng cổng. Vòng chờ bên dưới thấy cổng mở là đi
// tiếp, không hề biết mình kiểm nhầm ai. Một bộ kiểm có thể xanh cho mã chưa
// từng chạy còn tệ hơn không có bộ kiểm.
if (await congMo(PORT)) {
  console.error(`Cổng ${PORT} ĐANG CÓ tiến trình khác lắng nghe.`)
  console.error('Bộ kiểm sẽ nói chuyện với tiến trình đó chứ không phải binary vừa build.')
  console.error(`Tắt nó rồi chạy lại, hoặc dùng --port <cổng khác>.`)
  process.exit(1)
}

console.log(`Gateway: ${BIN}\n`)

const core = spawn(BIN, [], {
  cwd: ROOT,
  // stdin PHẢI là pipe và phải mở: lõi đọc stdin cho IPC và thoát ngay khi gặp
  // EOF. Chạy nền với stdin đóng thì nó in "shutting down" rồi thoát 0 — trông
  // y hệt một lần chạy thành công.
  stdio: ['pipe', 'inherit', 'inherit'],
  env: {
    ...process.env,
    LIVA_SERVER_PORT: String(PORT),
    // DB tạm trong RAM: không đụng dữ liệu thật của người chạy.
    LIVA_DB_IN_MEMORY: '1',
    // Khoá cố định để không phụ thuộc DPAPI và không bật đường escrow.
    LIVA_ENCRYPTION_KEY:
      process.env.LIVA_ENCRYPTION_KEY || '00000000000000000000000000000000',
  },
})

let dangTat = false
const tat = () => {
  if (dangTat) return
  dangTat = true
  try {
    core.stdin.end() // EOF trên stdin = tín hiệu tắt sạch của lõi
  } catch {
    /* đã đóng */
  }
  // Rồi mới cưỡng chế. Chỉ `stdin.end()` là không đủ khi lõi đang kẹt ở một
  // lượt nạp model; và một tiến trình sót lại sẽ giữ cổng, khiến LẦN CHẠY SAU
  // âm thầm kiểm nhầm nó (đúng cái bẫy mà phần tiền kiểm ở trên vừa dựng rào).
  try {
    core.kill('SIGKILL')
  } catch {
    /* đã chết */
  }
}
process.on('exit', tat)
process.on('SIGINT', () => process.exit(130))
core.on('exit', (ma) => {
  if (!dangTat) {
    console.error(`\nGateway thoát sớm (mã ${ma}) — bộ kiểm không chạy được.`)
    process.exit(1)
  }
})

// Nạp model có thể mất vài chục giây trên máy chậm; chờ tối đa 120 s.
let san = false
for (let i = 0; i < 240 && !san; i++) {
  san = await congMo(PORT)
  if (!san) await cho(500)
}
if (!san) {
  console.error(`Gateway không mở cổng ${PORT} sau 120 s.`)
  process.exit(1)
}

// `e2e-gateway.mjs` tự gọi `main()` lúc import và tự `process.exit` — handler
// 'exit' ở trên lo phần tắt gateway.
process.env.PORT = String(PORT)
await import('./e2e-gateway.mjs')
