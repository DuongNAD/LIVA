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
//  1. TẤT ĐỊNH  — `memory:search_hybrid` (server tự embed) phải trả về lượt
//     đã kể. Chứng minh persist ĐÃ GHI và truy hồi ngữ nghĩa TÌM THẤY.
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
//   node scripts/e2e-memory.mjs             # mặc định cổng 8099
//
// Cần: model LLM (data/liva-config.json → ai.localModelsDir) và model embedding
// (models/embedding/ — node scripts/fetch-embedding-model.mjs).
// KHÔNG nằm trong CI: cần weights (gitignored) và tiến trình sống. Thoát 1 nếu
// phép kiểm tất định trượt.

import { ketNoi, goiLenh, guiVaDoi } from './lib/ws-client.mjs'

const PORT = Number(process.env.PORT || 8099)
const ORIGIN = 'http://localhost:5173'

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

  // ── Phép kiểm TẤT ĐỊNH: kho nhớ phải chứa và TÌM THẤY lượt vừa kể ────────
  // (persist chạy xong TRƯỚC khi ai_spoken_response được gửi, nên tới đây là
  // ký ức đã nằm trong DB — không cần đợi.)
  const tim = await goiLenh(ws, 'memory:search_hybrid', { query_text: 'con mèo của tôi tên gì', top_k: 3 }, 30000)
  const ketQua = JSON.stringify(tim.payload ?? {})
  const thayBun = ketQua.includes('Bún')
  ghi('Truy hồi ngữ nghĩa tìm thấy ký ức (tất định)', tim.event === 'memory:search_hybrid_response' && thayBun,
    tim.event !== 'memory:search_hybrid_response' ? (tim.payload?.error ?? tim.ly)
      : thayBun ? 'kết quả chứa "Bún"' : 'KHÔNG thấy "Bún" trong: ' + ketQua.slice(0, 160))

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
