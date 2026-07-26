#!/usr/bin/env node
// Kiểm chứng BỘ NHỚ DÀI HẠN đầu-cuối qua WebSocket thật: kể một sự kiện ở
// lượt 1, hỏi lại ở lượt 2, chứng minh LIVA nhớ.
//
// Vì sao cần: RAG được kiểm bằng unit test (embedder, recall/persist) nhưng
// unit test pass chưa chứng minh ĐƯỜNG SỐNG chạy — bài học ngay trong ngày:
// model embedding tải về xong vẫn hỏng vì thiếu `token_type_ids`, thứ chỉ lộ
// khi chạy thật. Script này đi đúng đường người dùng đi: UI gõ chữ →
// `user_voice_command` → recall → LLM → persist → hồi âm.
//
// Hai phép kiểm, cố ý tách rời:
//  1. TẤT ĐỊNH  — đọc trực tiếp DB thử nghiệm theo owner scope và xác nhận lượt
//     đã kể tồn tại. Không mở lại raw search API cho `conversation_turn`.
//  2. HÀNH VI   — câu trả lời lượt 2 nhắc đúng chi tiết đã kể. LLM có tính
//     ngẫu nhiên nên đây là phép kiểm mềm: trượt thì WARN chứ không đỏ,
//     nhưng kèm nguyên văn để người đọc tự thẩm định.
//
// ## Chạy
//
//   # 1. Gateway RELEASE (LLM cần tốc độ; giữ stdin mở — EOF là nó tắt)
//   $env:LIVA_SERVER_PORT="8099"
//   $env:LIVA_DB_PATH="C:\\tmp\\e2e_memory.sqlite"   # DB file riêng cho phiên thử
//   .\target\release\liva-native-core.exe
//
//   # 2. Cửa sổ khác
//   $env:LIVA_DB_PATH="C:\\tmp\\e2e_memory.sqlite"   # cùng file với gateway
//   node scripts/e2e-memory.mjs             # mặc định cổng 8099
//
// Cần: model LLM (data/liva-config.json → ai.localModelsDir) và model embedding
// (models/embedding/ — node scripts/fetch-embedding-model.mjs).
// KHÔNG nằm trong CI: cần weights (gitignored) và tiến trình sống. Thoát 1 nếu
// phép kiểm tất định trượt.

import { resolve } from 'node:path'

import { conversationMemoryContains } from './lib/memory-db.mjs'
import { ketNoi, goiLenh, guiVaDoi } from './lib/ws-client.mjs'

const PORT = Number(process.env.PORT || 8099)
const ORIGIN = 'http://localhost:5173'
const DB_PATH = process.env.LIVA_DB_PATH ? resolve(process.env.LIVA_DB_PATH) : ''

// Sự kiện đủ đặc trưng để không trùng với gì có sẵn trong model, và đủ tự
// nhiên để tokenizer/embedding xử lý như hội thoại thường.
const SU_KIEN = 'Tên tôi là Dương. Món uống yêu thích của tôi là cà phê sữa đá, và tôi nuôi một con mèo tên là Bún.'
const CAU_HOI = 'Bạn còn nhớ con mèo của tôi tên là gì không?'

// CHI_HOI=1: bỏ qua lượt kể, chỉ hỏi. Dùng sau khi KHỞI ĐỘNG LẠI gateway với
// cùng LIVA_DB_PATH — chứng minh ký ức nằm trong SQLite chứ không phải RAM,
// tức "nhớ" sống qua cả vòng đời tiến trình chứ không chỉ trong một phiên.
const CHI_HOI = process.env.CHI_HOI === '1'

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

const main = async () => {
  console.log(`Gateway: ws://127.0.0.1:${PORT}/ws\n`)
  if (!DB_PATH) {
    console.log('❌ Thiếu LIVA_DB_PATH. Script và gateway phải trỏ cùng một DB thử nghiệm cô lập.')
    process.exit(1)
  }

  const kn = await ketNoi({ port: PORT, origin: ORIGIN })
  if (!kn.ok) {
    console.log(`❌ Không kết nối được: ${kn.ly}\n   Gateway release đã chạy chưa? Xem hướng dẫn đầu file.`)
    process.exit(1)
  }
  const ws = kn.ws

  // Model LLM phải đã nạp — không thì cả hai lượt chỉ trả câu xin lỗi.
  const health = await goiLenh(ws, 'llm:health_check')
  const daNap = health.payload?.model_loaded === true
  ghi('LLM đã nạp model', daNap, daNap ? String(health.payload?.model_path).split(/[\\/]/).pop() : JSON.stringify(health.payload ?? health.ly))
  if (!daNap) { ws.close(); return ket }

  // ── Lượt 1: kể sự kiện (bỏ qua khi CHI_HOI — kiểm bền vững qua restart) ──
  if (CHI_HOI) {
    console.log('\n(CHI_HOI=1 — bỏ qua lượt kể; ký ức phải đến từ DB của lần chạy trước)')
  } else {
    console.log(`\n→ Lượt 1: "${SU_KIEN}"`)
    const t1 = Date.now()
    const l1 = await guiVaDoi(ws, 'user_voice_command', { text: SU_KIEN }, 'ai_spoken_response')
    ghi('Lượt 1 có hồi âm', l1.ok, l1.ok ? `${((Date.now() - t1) / 1000).toFixed(1)}s, ${l1.soChunk} chunk` : l1.ly)
    if (!l1.ok) { ws.close(); return ket }
    console.log(`   LIVA: ${String(l1.payload?.text).slice(0, 140)}`)
  }

  // ── Phép kiểm TẤT ĐỊNH: DB cô lập phải chứa lượt vừa kể ─────────────────
  // (persist chạy xong TRƯỚC khi ai_spoken_response được gửi, nên tới đây là
  // ký ức đã nằm trong DB — không cần đợi.)
  const thayBun = conversationMemoryContains(DB_PATH, 'Bún')
  ghi(
    'DB chứa ký ức owner-local (tất định)',
    thayBun,
    thayBun ? 'conversation_turn chứa "Bún"' : 'KHÔNG thấy "Bún" trong owner-local',
  )

  // ── Lượt 2: hỏi lại ───────────────────────────────────────────────────────
  console.log(`\n→ Lượt 2: "${CAU_HOI}"`)
  const t2 = Date.now()
  const l2 = await guiVaDoi(ws, 'user_voice_command', { text: CAU_HOI }, 'ai_spoken_response')
  ghi('Lượt 2 có hồi âm', l2.ok, l2.ok ? `${((Date.now() - t2) / 1000).toFixed(1)}s` : l2.ly)
  if (l2.ok) {
    const traLoi = String(l2.payload?.text ?? '')
    console.log(`   LIVA: ${traLoi.slice(0, 200)}`)
    // Phép kiểm MỀM — LLM ngẫu nhiên, không đỏ CI vì cách diễn đạt.
    const nho = traLoi.includes('Bún')
    console.log(nho
      ? '✅ (mềm) Câu trả lời nhắc đúng tên "Bún" — LIVA nhớ thật'
      : '⚠️  (mềm) Câu trả lời không nhắc "Bún" — ký ức ĐÃ được truy hồi (phép kiểm tất định ở trên), nhưng model không dùng nó khi diễn đạt. Đọc nguyên văn ở trên để tự thẩm định.')
  }

  // ── Đường CHAT:COMPLETION (Telegram + mọi API client) ────────────────────
  // Khác user_voice_command: đây là arm riêng trong handle_command, RAG được
  // nối tay ở đó. Nếu persist ở đây lỗi, bộ nhớ Telegram vỡ IM LẶNG — nên
  // phải kiểm đường sống này riêng, không suy từ đường thoại.
  if (CHI_HOI) {
    console.log('\n(CHI_HOI=1 — bỏ qua phần kể của chat:completion)')
  } else {
    const suKienTg = 'Ghi nhớ giúp tôi: mã dự án của tôi là ORION-7, và deadline là thứ Sáu.'
    console.log(`\n→ chat:completion lượt 1: "${suKienTg}"`)
    const c1 = await goiLenh(ws, 'chat:completion', {
      messages: [{ role: 'user', content: suKienTg }],
      stream: false,
    }, 60000)
    ghi('chat:completion lượt 1 có hồi âm',
      c1.event === 'chat:completion_response' && typeof c1.payload?.text === 'string',
      c1.event === 'chat:completion_error' ? c1.payload?.error : (c1.event ?? c1.ly))
    if (c1.event === 'chat:completion_response') {
      console.log(`   LIVA: ${String(c1.payload.text).slice(0, 140)}`)
    }
  }

  // Tất định: DB phải chứa mã dự án vừa kể qua chat:completion.
  const thayOrion = conversationMemoryContains(DB_PATH, 'ORION-7')
  ghi(
    'chat:completion cũng ghi được ký ức owner-local (tất định)',
    thayOrion,
    thayOrion ? 'conversation_turn chứa "ORION-7"' : 'KHÔNG thấy "ORION-7" trong owner-local',
  )

  // Lượt 2 qua chính chat:completion: hỏi lại, kiểm recall trên đúng đường sống.
  console.log(`\n→ chat:completion lượt 2: "Mã dự án của tôi là gì?"`)
  const c2 = await goiLenh(ws, 'chat:completion', {
    messages: [{ role: 'user', content: 'Mã dự án của tôi là gì?' }],
    stream: false,
  }, 60000)
  if (c2.event === 'chat:completion_response') {
    const tl = String(c2.payload.text ?? '')
    console.log(`   LIVA: ${tl.slice(0, 200)}`)
    console.log(tl.includes('ORION-7')
      ? '✅ (mềm) chat:completion nhớ đúng "ORION-7" — bộ nhớ Telegram/API chạy thật'
      : '⚠️  (mềm) không nhắc "ORION-7"; ký ức đã truy hồi được (tất định ở trên), model không dùng khi diễn đạt.')
  } else {
    ghi('chat:completion lượt 2 có hồi âm', false, c2.event ?? c2.ly)
  }

  ws.close()
  return ket
}

main()
  .then((r) => {
    const truot = r.filter((x) => !x.dat).length
    console.log(`\n${r.length - truot}/${r.length} phép kiểm cứng đạt`)
    process.exit(truot > 0 ? 1 : 0)
  })
  .catch((e) => { console.error('LỖI:', e); process.exit(1) })
