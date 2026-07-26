#!/usr/bin/env node
// Kiểm chứng ĐẦU-CUỐI bộ chọn giọng VieNeu (U17a) qua WebSocket thật.
//
// Vì sao cần một script riêng: unit test trong `tts/vieneu` chỉ chứng minh phần
// phân tích JSON. Nó KHÔNG chạm tới `handle_command`, không ghi cấu hình, và
// không chứng minh được rằng đổi giọng có tác dụng ngay trên engine đang nạp.
// Ba điều đó mới là nội dung của U17a.
//
// ## Chạy
//
//   # 1. Gateway — giữ stdin MỞ (lõi đọc stdin cho IPC, EOF là nó thoát)
//   $env:LIVA_SERVER_PORT="8099"; $env:LIVA_DB_IN_MEMORY="1"
//   .\target\debug\liva-native-core.exe
//
//   # 2. Cửa sổ khác
//   node scripts/e2e-vieneu-voice.mjs
//
// KHÔNG nằm trong CI: cần trọng số `models/vieneu/` (gitignored) và một tiến
// trình sống. Thoát 1 nếu có mục nào trượt.
//
// ## Về việc script này ghi vào cấu hình thật
//
// `voice:set_vieneu_voice` ghi xuống `data/liva-config.json` — đó chính là thứ
// cần kiểm (lựa chọn phải sống sót qua khởi động lại). Script **sao lưu nguyên
// văn file trước khi chạy và khôi phục ở cuối**, kể cả khi có mục trượt, để
// không đổi cấu hình của người đang dùng máy.

import fs from 'node:fs'
import path from 'node:path'
import { ketNoi, goiLenh } from './lib/ws-client.mjs'

const PORT = Number(process.env.PORT || 8099)
const ORIGIN = 'http://localhost:5173'
const CONFIG = path.join(import.meta.dirname, '..', 'data', 'liva-config.json')

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

const docTts = () => {
  try {
    return JSON.parse(fs.readFileSync(CONFIG, 'utf8')).tts ?? null
  } catch {
    return null
  }
}

const main = async () => {
  console.log(`Gateway: ws://127.0.0.1:${PORT}/ws\n`)

  const conn = await ketNoi({ port: PORT, origin: ORIGIN })
  if (!conn.ok) {
    console.log(`Không kết nối được (${conn.ly}). Gateway chưa chạy? Xem hướng dẫn ở đầu file.`)
    return ket
  }
  const ws = conn.ws

  // ── 1. Liệt kê — phải trả lời được NGAY CẢ KHI VieNeu đang tắt ────────────
  const ds = await goiLenh(ws, 'voice:list_vieneu_voices', {}, 20000)
  const voices = ds.payload?.voices
  ghi('Liệt kê giọng trả về danh sách', Array.isArray(voices) && voices.length > 0,
    Array.isArray(voices) ? `${voices.length} giọng` : (ds.payload?.error ?? ds.ly))
  if (!Array.isArray(voices) || voices.length < 2) {
    console.log('\nCần ít nhất 2 giọng để kiểm phần đổi giọng. Thiếu models/vieneu/? Chạy `npm run doctor`.')
    return ket
  }

  ghi('Đúng một giọng được đánh dấu mặc định',
    voices.filter((v) => v.is_default).length === 1)
  ghi('Mỗi giọng có đủ trường để hiển thị',
    voices.every((v) => v.name && typeof v.gender === 'string' && typeof v.region === 'string'))

  const [giongA, giongB] = voices
  const tuocBanDau = docTts()

  // ── 2. Tên giọng sai phải bị chặn TRƯỚC khi ghi cấu hình ──────────────────
  const bay = await goiLenh(ws, 'voice:set_vieneu_voice', { voice: 'Không Có Thật' }, 20000)
  ghi('Tên giọng sai bị từ chối', bay.event?.endsWith('_error') === true,
    bay.payload?.error ?? bay.event)
  ghi('Tên sai KHÔNG ghi gì vào cấu hình',
    JSON.stringify(docTts()) === JSON.stringify(tuocBanDau),
    'ghi tên sai xuống config sẽ làm lần khởi động sau nạp hỏng và im lặng rơi về Piper')

  // ── 3. Bật + chọn giọng (nạp ~500 MB, nên cho hạn rộng) ───────────────────
  const bat = await goiLenh(ws, 'voice:set_vieneu_voice',
    { voice: giongA.name, enabled: true }, 180000)
  ghi(`Bật VieNeu và chọn "${giongA.name}"`, bat.payload?.success === true,
    bat.payload?.applied ?? bat.payload?.error ?? bat.ly)
  ghi('Giọng đang dùng đúng bằng giọng vừa chọn', bat.payload?.current === giongA.name,
    String(bat.payload?.current))

  // ── 4. Lựa chọn phải nằm trong cấu hình — đây là phần "sống sót khởi động lại"
  const sauKhiBat = docTts()
  ghi('Cấu hình đã lưu tên giọng', sauKhiBat?.vieneuVoice === giongA.name,
    JSON.stringify(sauKhiBat))
  ghi('Cấu hình đã lưu trạng thái bật', sauKhiBat?.vieneuEnabled === true)

  // ── 5. Đổi sang giọng khác: phải là đổi TẠI CHỖ, không nạp lại ────────────
  const t0 = Date.now()
  const doi = await goiLenh(ws, 'voice:set_vieneu_voice', { voice: giongB.name }, 60000)
  const ms = Date.now() - t0
  ghi(`Đổi sang "${giongB.name}"`, doi.payload?.current === giongB.name,
    `${doi.payload?.applied ?? doi.payload?.error} · ${ms} ms`)
  // Nạp lại engine mất khoảng vài giây; đổi anchor là một phép nhân ma trận.
  // Ngưỡng 2000 ms đủ rộng để không đỏ vì máy chậm, vẫn đủ chặt để phát hiện
  // nếu ai đó lỡ đổi `set_voice` thành nạp lại toàn bộ.
  ghi('Đổi giọng là thao tác rẻ (không nạp lại engine)', ms < 2000, `${ms} ms`)

  // ── 6. Liệt kê lại phải thấy trạng thái mới ───────────────────────────────
  const ds2 = await goiLenh(ws, 'voice:list_vieneu_voices', {}, 20000)
  ghi('Liệt kê lại phản ánh giọng đang dùng', ds2.payload?.current === giongB.name)
  ghi('Liệt kê lại báo đang bật', ds2.payload?.enabled === true)

  // ── 7. Tắt được ───────────────────────────────────────────────────────────
  const tat = await goiLenh(ws, 'voice:set_vieneu_voice', { enabled: false }, 30000)
  ghi('Tắt VieNeu', tat.payload?.enabled === false && tat.payload?.current === null,
    tat.payload?.applied ?? tat.payload?.error)

  try { ws.close() } catch { /* đã đóng */ }
  return ket
}

const goc = fs.existsSync(CONFIG) ? fs.readFileSync(CONFIG, 'utf8') : null

// Khôi phục nguyên văn — kể cả khi trượt giữa chừng.
//
// ⚠️ Dùng `process.exitCode` chứ KHÔNG `process.exit()`: `process.exit()` kết
// thúc tiến trình ngay tại chỗ, nên `finally` không bao giờ chạy và cấu hình
// thật của người dùng bị bỏ lại ở trạng thái test. Đã dẫm phải đúng bẫy này
// lần chạy đầu tiên (26/07/2026) — file phải khôi phục bằng `git checkout`.
const khoiPhuc = () => {
  if (goc === null) return
  fs.writeFileSync(CONFIG, goc)
  console.log('↩️  Đã khôi phục data/liva-config.json về nguyên trạng.')
}

main()
  .then((ket) => {
    const truot = ket.filter((k) => !k.dat)
    console.log(`\n${ket.length - truot.length}/${ket.length} đạt`)
    process.exitCode = truot.length ? 1 : 0
  })
  .catch((e) => {
    console.error('Lỗi:', e)
    process.exitCode = 1
  })
  .finally(khoiPhuc)
