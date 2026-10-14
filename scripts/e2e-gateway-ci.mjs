#!/usr/bin/env node
// Chạy lõi THẬT rồi kiểm nó đầu-cuối qua đường IPC stdin/stdout — một lệnh,
// không cần hai cửa sổ terminal.
//
// Vì sao cần: mọi test Rust khác gọi `handle_command` trong tiến trình với một
// state giả. Không cái nào chứng minh được rằng binary vừa build khởi động, nạp
// đúng engine, và trả lời đúng qua kênh mà vỏ Tauri / client thật dùng.
//
// ## Vì sao không còn là WebSocket
//
// Tới `f041c08` (21/09/2026) bộ này nói chuyện với gateway WebSocket :8002.
// Commit đó xoá hẳn `websocket.rs` ("zero open ports" — xem
// docs/01-kien-truc/04-giao-thuc-ipc-tauri-v2.md), nên bản cũ chỉ còn chờ 120 s
// một cổng không bao giờ mở rồi thoát 1. Kênh còn lại là stdin/stdout: mỗi dòng
// vào là `{"id","command","payload"}`, mỗi dòng ra là `{"id","status","data"|"error"}`.
//
// ## Lỗi mà bộ này được viết để bắt lại
//
// Cùng `f041c08` để engine của actor LLM không bao giờ được nạp model: mọi đường
// "nạp" chỉ ghi metadata, `llm:health_check` báo "healthy", và `chat:completion`
// trả về `"Simulated response to: <toàn bộ prompt>"` với status "ok". Không test
// nào đỏ trong 19 ngày. Vì vậy các khẳng định dưới đây kiểm NỘI DUNG câu trả lời,
// không chỉ việc có trả lời.
//
// Không cần model weights (CI không có): khi chưa nạp model, chat phải báo lỗi
// "No model loaded". Có model (máy dev) thì bộ kiểm thêm phần câu trả lời thật.
//
// Dùng:
//   node scripts/e2e-gateway-ci.mjs                 # build debug
//   node scripts/e2e-gateway-ci.mjs --release       # build release
//   node scripts/e2e-gateway-ci.mjs --bin path/to/exe
//
// Thoát 1 nếu có mục nào trượt.

import { spawn } from 'node:child_process'
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

if (!fs.existsSync(BIN)) {
  console.error(`Không thấy binary: ${BIN}`)
  console.error(`Build trước:  cargo build ${profile === 'release' ? '--release ' : ''}--bin liva-native-core`)
  process.exit(1)
}

console.log(`Lõi: ${BIN}\n`)

const core = spawn(BIN, [], {
  cwd: ROOT,
  // stdin PHẢI là pipe và phải mở: lõi đọc stdin cho IPC và thoát ngay khi gặp
  // EOF. Chạy nền với stdin đóng thì nó in "shutting down" rồi thoát 0 — trông
  // y hệt một lần chạy thành công.
  stdio: ['pipe', 'pipe', 'pipe'],
  env: {
    ...process.env,
    // DB tạm trong RAM: không đụng dữ liệu thật của người chạy.
    LIVA_DB_IN_MEMORY: '1',
    // Khoá cố định để không phụ thuộc DPAPI và không bật đường escrow.
    LIVA_ENCRYPTION_KEY:
      process.env.LIVA_ENCRYPTION_KEY || '00000000000000000000000000000000',
    // Bộ kiểm đọc log INFO của autoload để biết model đã nạp xong hay chưa.
    RUST_LOG: process.env.RUST_LOG || 'info',
  },
})

let stderr = ''
core.stderr.on('data', (d) => {
  stderr += d
})

// ─── Kênh IPC ───────────────────────────────────────────────────────────────

const choTraLoi = new Map() // id -> resolve(phản hồi cuối)
const manh = new Map() // id -> các mẩu token stream
let dem = 0
let buf = ''
core.stdout.on('data', (d) => {
  buf += d
  let i
  while ((i = buf.indexOf('\n')) >= 0) {
    const dong = buf.slice(0, i).trim()
    buf = buf.slice(i + 1)
    if (!dong) continue
    let m
    try {
      m = JSON.parse(dong)
    } catch {
      continue
    }
    const data = m?.data
    // Mẩu stream: `{id, status, data: {token, done}}`; phản hồi cuối mang `text`.
    if (data && typeof data.token === 'string' && data.text === undefined) {
      if (!data.done) {
        if (!manh.has(m.id)) manh.set(m.id, [])
        manh.get(m.id).push(data.token)
      }
      continue
    }
    const xong = choTraLoi.get(m.id)
    if (xong) {
      choTraLoi.delete(m.id)
      xong(m)
    }
  }
})

const goi = (command, payload = {}, timeoutMs = 30000) =>
  new Promise((resolve) => {
    const id = `e2e-${++dem}`
    const hen = setTimeout(() => {
      choTraLoi.delete(id)
      resolve({ id, status: 'timeout' })
    }, timeoutMs)
    choTraLoi.set(id, (m) => {
      clearTimeout(hen)
      resolve(m)
    })
    core.stdin.write(JSON.stringify({ id, command, payload }) + '\n')
  })

const goiTho = (dong, timeoutMs = 10000) =>
  new Promise((resolve) => {
    const hen = setTimeout(() => {
      choTraLoi.delete('unknown')
      resolve({ status: 'timeout' })
    }, timeoutMs)
    choTraLoi.set('unknown', (m) => {
      clearTimeout(hen)
      resolve(m)
    })
    core.stdin.write(dong + '\n')
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
    console.error(`\nLõi thoát sớm (mã ${ma}) — bộ kiểm không chạy tiếp được.`)
    const vet = stderr.split('\n').filter((l) => /panicked|error/i.test(l)).slice(-5)
    if (vet.length) console.error(vet.join('\n'))
    process.exit(1)
  }
})

// ─── Bộ kiểm chứng ──────────────────────────────────────────────────────────

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}
const loi = (m) => String(m?.error ?? m?.status ?? '')

// 1. Khởi động + trả lời. Nạp model (nếu có) diễn ra nền, nên chỉ cần hồi âm.
const t0 = Date.now()
let suc = { status: 'timeout' }
while (Date.now() - t0 < 120000) {
  suc = await goi('llm:health_check', {}, 5000)
  if (suc.status !== 'timeout') break
}
ghi('Lõi khởi động và trả lời qua stdin', suc.status === 'ok', `${Date.now() - t0} ms`)
if (suc.status !== 'ok') process.exit(1)

// Model nạp nền ở `spawn_background_services`. Chờ tới khi autoload đã QUYẾT ĐỊNH
// (nạp xong / thất bại / file bị từ chối / provider không phải local) rồi mới đọc
// trạng thái — đọc sớm hơn thì "chưa có model" chỉ là "chưa nạp xong".
const quyetDinh =
  /Router model loaded|Failed to load router model|Từ chối nạp router model|skipping router model load/
for (let i = 0; i < 600 && !quyetDinh.test(stderr); i++) {
  await new Promise((r) => setTimeout(r, 500))
}
const health = (await goi('llm:health_check')).data
const coModel = health?.model_loaded === true

// 2. Health không được nói dối: "healthy" chỉ khi engine thật sự có model.
ghi(
  'health_check khớp trạng thái engine thật',
  coModel ? ['healthy', 'busy'].includes(health.status) : ['offline', 'busy'].includes(health.status),
  `model_loaded=${health?.model_loaded} status=${health?.status}`,
)

// 3. Lệnh không tồn tại trả lỗi thay vì im lặng.
const la = await goi('khong_ton_tai_dau')
ghi('Lệnh không tồn tại trả status "error"', la.status === 'error', loi(la))

// 4. Dòng JSON hỏng vẫn có hồi âm.
const hong = await goiTho('{ day khong phai json')
ghi('JSON hỏng trả lỗi với id "unknown"', hong.status === 'error', loi(hong))

// Có model thì một lượt CPU có thể mất cả phút; không có model thì phải trả lỗi
// ngay — chờ lâu ở đó chỉ làm CI treo thay vì đỏ.
const HAN_LUOT = coModel ? 900000 : 30000

// 5. Chat không stream: KHÔNG BAO GIỜ được trả lại chính prompt như câu trả lời.
const hoi = { role: 'user', content: 'What is 2 + 2? Answer with just the number.' }
const chat = await goi('chat:completion', { messages: [hoi], stream: false }, HAN_LUOT)
const chu = String(chat?.data?.text ?? '')
const echo = /Simulated response|You are LIVA/.test(chu)
if (coModel) {
  ghi('chat:completion trả câu trả lời thật', chat.status === 'ok' && chu.trim() !== '' && !echo,
    JSON.stringify(chu.slice(0, 80)))
  ghi('chat:completion báo số token prompt thật', (chat?.data?.usage?.prompt_tokens ?? 0) > 0,
    `prompt_tokens=${chat?.data?.usage?.prompt_tokens}`)
} else {
  ghi('chat:completion khi chưa có model trả lỗi "No model loaded"',
    chat.status === 'error' && /No model loaded/.test(loi(chat)) && !echo,
    chat.status === 'ok' ? `TRẢ "ok": ${JSON.stringify(chu.slice(0, 80))}` : loi(chat))
}

// 6. Chat stream: không được làm sập lõi (bản cũ gọi `blocking_send` trong task
//    async — tokio panic ngay mẩu đầu tiên).
const streamed = await goi('chat:completion', {
  messages: [{ role: 'user', content: 'Name one primary color. One word.' }],
  stream: true,
}, HAN_LUOT)
const conSong = await goi('llm:health_check', {}, 10000)
ghi('chat:completion stream không làm sập lõi', conSong.status === 'ok',
  `stream status=${streamed.status}`)
if (coModel) {
  const so = (manh.get(streamed.id) || []).length
  ghi('Stream đẩy từng mẩu token về client', streamed.status === 'ok' && so > 0, `${so} mẩu`)
}

// 7. vision:ask có hồi âm nhanh và không treo. Gửi sẵn một ảnh PNG 1×1 để không
//    phụ thuộc màn hình của máy chạy CI.
const PNG_1x1 =
  'iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR4nGNgYGD4DwABBAEAwS2OUAAAAABJRU5ErkJggg=='
const tv = Date.now()
const vis = await goi('vision:ask', { question: 'What is in this image?', image: PNG_1x1 }, HAN_LUOT)
ghi('vision:ask có hồi âm, không treo', vis.status === 'ok' || vis.status === 'error',
  `${Date.now() - tv} ms — ${vis.status === 'ok' ? 'ok' : loi(vis)}`)

// 8. Không panic nào trong suốt phiên.
const panic = stderr.split('\n').filter((l) => /panicked at|Cannot block the current thread/.test(l))
ghi('stderr không có panic', panic.length === 0, panic[0] ?? '')

const truot = ket.filter((x) => !x.dat).length
console.log(`\n${ket.length - truot}/${ket.length} đạt${coModel ? ' (có model)' : ' (không có model)'}`)
tat()
process.exit(truot > 0 ? 1 : 0)
