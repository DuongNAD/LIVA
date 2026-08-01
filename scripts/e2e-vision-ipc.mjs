#!/usr/bin/env node
// e2e-vision-ipc.mjs — đo NĂNG LỰC THỊ GIÁC thật, qua đường IPC stdin.
//
// # Vì sao tồn tại, thay vì kiểm trong `e2e-gateway.mjs`
//
// `vision:ask` KHÔNG nằm trong allow-list của principal `WebSocketRemote` kể từ
// `98efc55` (khép C1 — authorization theo principal), và điều đó là ĐÚNG:
// `sessions.issue(WebSocketRemote)` cố tình trả lỗi, remote không bao giờ nâng
// quyền được. Nhưng hệ quả là bộ e2e qua WebSocket **ngừng đo thị giác** mà vẫn
// báo xanh — đo thật 02/08/2026 cho `1ms — lỗi: principal WebSocketRemote is
// not authorized`. Một cổng LUÔN xanh mà không chứng minh gì còn nguy hiểm hơn
// một cổng nhấp nháy, vì không ai có lý do nghi ngờ nó.
//
// Đường stdin của lõi chạy dưới principal `LocalCli` (được phép mọi lệnh), nên
// đó là chỗ đúng để đo năng lực. Bộ này KHÔNG kiểm phân quyền — phần đó thuộc
// `e2e-gateway.mjs`.
//
// # Chạy
//   node scripts/e2e-vision-ipc.mjs                 # binary debug
//   node scripts/e2e-vision-ipc.mjs --release       # binary release
//   node scripts/e2e-vision-ipc.mjs --bin <đường dẫn> --luot 3
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
const SO_LUOT = Number(lay('--luot') || 3)
// Nạp model + chụp màn hình + suy luận. Trên CPU, U1a đo ~80 s MỘT lượt, nên
// ngân sách phải rộng; mục đích ở đây là phân loại kết quả, không phải bắt chậm.
const HAN_MOI_LUOT = Number(lay('--han') || 180000)

if (!fs.existsSync(BIN)) {
  console.error(`Không thấy binary: ${BIN}`)
  console.error(
    `Build trước:  cargo build ${profile === 'release' ? '--release ' : ''}--bin liva-native-core`,
  )
  process.exit(1)
}

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ` — ${chiTiet}` : ''}`)
}

console.log(`Binary: ${BIN}\nProfile: ${profile}\n`)

// Cổng 0 = để OS chọn: bộ này không nói chuyện qua WebSocket, nhưng lõi vẫn mở
// cổng. Không ghim số để KHÔNG bao giờ đụng một LIVA khác đang chạy trên máy.
const core = spawn(BIN, [], {
  cwd: ROOT,
  // stdout PHẢI là pipe — đây là kênh đọc hồi âm IPC. (Khác `e2e-gateway-ci.mjs`
  // dùng `inherit` để log chảy thẳng ra terminal; ở đây ta phải PHÂN TÍCH nó.)
  // stdin PHẢI mở: lõi thoát ngay khi gặp EOF.
  stdio: ['pipe', 'pipe', 'pipe'],
  env: {
    ...process.env,
    LIVA_SERVER_PORT: lay('--port') || '8097',
    LIVA_DB_IN_MEMORY: '1',
    LIVA_ENCRYPTION_KEY:
      process.env.LIVA_ENCRYPTION_KEY || '00000000000000000000000000000000',
  },
})

let dangTat = false
const tat = () => {
  if (dangTat) return
  dangTat = true
  try {
    core.stdin.end()
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

let thoatSom = null
core.on('exit', (ma) => {
  if (!dangTat) thoatSom = ma
})

// ─── Đọc hồi âm IPC: mỗi dòng stdout là một JSON ────────────────────────────
const dangCho = new Map() // id -> {resolve, t0}
let dem = ''
core.stdout.on('data', (buf) => {
  dem += buf.toString()
  let i
  while ((i = dem.indexOf('\n')) >= 0) {
    const dong = dem.slice(0, i).trim()
    dem = dem.slice(i + 1)
    if (!dong.startsWith('{')) continue
    let obj
    try {
      obj = JSON.parse(dong)
    } catch {
      continue
    }
    const cho = dangCho.get(obj?.id)
    if (cho) {
      dangCho.delete(obj.id)
      cho.resolve({ obj, dt: Date.now() - cho.t0 })
    }
  }
})

// Log lõi đi ra stderr; giữ lại để chứng minh GPU có vào cuộc hay không.
let nhatKy = ''
core.stderr.on('data', (b) => {
  nhatKy += b.toString()
})

const goiIpc = (id, command, payload, han) =>
  new Promise((resolve) => {
    const hen = setTimeout(() => {
      if (dangCho.delete(id)) resolve({ obj: null, dt: han })
    }, han)
    dangCho.set(id, {
      t0: Date.now(),
      resolve: (r) => {
        clearTimeout(hen)
        resolve(r)
      },
    })
    core.stdin.write(`${JSON.stringify({ id, command, payload })}\n`)
  })

const cho = (ms) => new Promise((r) => setTimeout(r, ms))

// ⚠️ Chờ MODEL NẠP XONG, không phải chờ kênh sống.
//
// Bản đầu của bộ này dùng `ping` làm tín hiệu sẵn sàng và **đo sai hoàn toàn**:
// `ping` hồi âm ngay khi vòng lặp IPC chạy, trong khi router model + mmproj
// được nạp BẤT ĐỒNG BỘ sau đó (`set_mmproj_path` nằm trong đường nạp router).
// Kết quả: cả 3 lượt trả `No mmproj (vision projector) configured` trong
// 199/49/47 ms, và log cho `0 lớp trên CUDA0`. Hai dấu hiệu đó — **quá nhanh**
// và **không có lớp nào trên GPU** — là thứ duy nhất tố cáo phép đo hỏng; nếu
// chỉ nhìn "có hồi âm" thì nó trông y hệt một kết luận hợp lệ.
//
// `llm:health_check` trả `model_loaded`, đó mới là điều kiện đúng.
let san = false
for (let i = 0; i < 60 && !san; i++) {
  if (thoatSom !== null) {
    console.error(`\nLõi thoát sớm (mã ${thoatSom}) — bộ kiểm không chạy được.`)
    process.exit(1)
  }
  const r = await goiIpc(`ping-${i}`, 'ping', {}, 2000)
  if (r.obj) san = true
  else await cho(2000)
}
ghi('Lõi trả lời được trên kênh IPC stdin', san, san ? '' : 'không hồi âm sau ~120s')
if (!san) {
  tat()
  process.exit(1)
}

let daNap = false
let hc = null
for (let i = 0; i < 90 && !daNap; i++) {
  const r = await goiIpc(`hc-${i}`, 'llm:health_check', {}, 5000)
  hc = r.obj?.data ?? null
  daNap = hc?.model_loaded === true
  if (!daNap) await cho(2000)
}
ghi('Router model đã nạp xong (llm:health_check.model_loaded)', daNap,
  daNap
    ? `n_gpu_layers=${hc?.n_gpu_layers} · ${String(hc?.model_path ?? '').split(/[\\/]/).pop()}`
    : 'model chưa nạp sau ~180s — mọi số đo sau đây sẽ VÔ NGHĨA')
if (!daNap) {
  tat()
  process.exit(1)
}

// ─── Đo vision ──────────────────────────────────────────────────────────────
const doLuot = []
for (let i = 1; i <= SO_LUOT; i++) {
  const r = await goiIpc(
    `vision-${i}`,
    'vision:ask',
    { question: 'Mô tả ngắn gọn những gì đang hiển thị trên màn hình.' },
    HAN_MOI_LUOT,
  )
  doLuot.push(r)
}

const loi = (r) => String(r.obj?.error ?? r.obj?.data?.error ?? '')
const chuoiOk = (r) => r.obj?.status === 'ok' && typeof r.obj?.data?.text === 'string'

// Ba kết cục PHẢI phân biệt được với nhau — gộp chúng lại chính là lỗi mà bộ
// e2e cũ mắc phải:
//   1. trả lời thật              ⇒ ĐẠT, có số đo
//   2. lỗi "cần build release"   ⇒ ĐẠT ở profile debug, đó là hành vi có chủ đích
//   3. lỗi phân quyền / im lặng  ⇒ TRƯỢT, bộ kiểm đang không đo thứ nó tưởng
const thatBaiPhanQuyen = doLuot.filter((r) => loi(r).includes('not authorized'))
ghi(
  'KHÔNG bị chặn phân quyền (principal stdin phải là LocalCli)',
  thatBaiPhanQuyen.length === 0,
  thatBaiPhanQuyen.length
    ? `${thatBaiPhanQuyen.length}/${SO_LUOT} lượt bị chặn — bộ kiểm này đang đo rỗng, xem đầu file`
    : '',
)

const imLang = doLuot.filter((r) => r.obj === null)
ghi(
  'Mọi lượt đều có hồi âm (không treo)',
  imLang.length === 0,
  imLang.length ? `${imLang.length}/${SO_LUOT} lượt không hồi âm trong ${HAN_MOI_LUOT} ms` : '',
)

const thanhCong = doLuot.filter(chuoiOk)
const canRelease = doLuot.filter((r) => /release build/i.test(loi(r)))

if (profile === 'debug' && canRelease.length === doLuot.length) {
  // Hành vi có chủ đích: guard `cfg!(all(windows, debug_assertions))` trong
  // `llm/engine.rs` biến một CRT assertion (abort) thành `Err` sạch.
  ghi('Debug: vision từ chối RÕ RÀNG thay vì abort', true,
    `${doLuot.length}/${doLuot.length} lượt trả "requires a release build"`)
} else {
  ghi('Vision trả nội dung thật', thanhCong.length === doLuot.length,
    `${thanhCong.length}/${doLuot.length} lượt có text`)
  for (const [i, r] of doLuot.entries()) {
    const t = r.obj?.data?.text
    console.log(
      `   lượt ${i + 1}: ${r.dt} ms — ${t ? `${String(t).replace(/\s+/g, ' ').slice(0, 70)}…` : loi(r) || 'không hồi âm'}`,
    )
  }
  if (thanhCong.length) {
    const ms = thanhCong.map((r) => r.dt).sort((a, b) => a - b)
    const p50 = ms[Math.floor(ms.length / 2)]
    console.log(`   p50 = ${p50} ms · min ${ms[0]} · max ${ms[ms.length - 1]}`)
  }
}

// Chứng GPU. U1a nói thẳng: thiếu hai dòng này thì con số đo được là của CPU.
// Không phải assertion (bản CPU hợp lệ) — nhưng phải HIỆN RA, vì một con số
// nhanh mà không biết nó nhanh nhờ đâu thì không dùng để so sánh được.
const coCuda = /ggml_cuda_init: found \d+ CUDA device/.test(nhatKy)
const soLop = (nhatKy.match(/assigned to device CUDA0/g) || []).length
console.log(
  `\nThiết bị: ${coCuda ? `CUDA — ${soLop} lớp trên CUDA0` : 'CPU (không thấy ggml_cuda_init)'}`,
)

tat()
const truot = ket.filter((k) => !k.dat).length
console.log(`\n${ket.length - truot}/${ket.length} đạt`)
process.exit(truot ? 1 : 0)
