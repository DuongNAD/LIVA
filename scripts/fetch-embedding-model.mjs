#!/usr/bin/env node
// Tải model embedding cho bộ nhớ dài hạn của LIVA.
//
// Vì sao cần script này: thiếu model thì RAG im lặng bỏ qua — lõi vẫn chạy,
// vẫn trả lời, chỉ là **không nhớ gì**. Đó là kiểu hỏng tệ nhất: không có lỗi,
// không có gì đỏ, chỉ có một tính năng lặng lẽ vắng mặt. Lõi đã log WARN lúc
// khởi động, nhưng một lệnh chạy được thì tốt hơn một dòng cảnh báo.
//
// Model: intfloat/multilingual-e5-small — 384 chiều, đa ngữ (có tiếng Việt),
// đúng chiều mà bảng `vec0(embedding int8[384])` trong `db.rs` khai báo. Đổi
// sang model khác chiều sẽ khiến `memory:upsert_vector` từ chối vector.
//
// Hai lý do nữa model này khớp hợp đồng trong `llm/embedder.rs`:
//  - nó thuộc họ XLM-RoBERTa nên **không cần `token_type_ids`**, mà embedder
//    chỉ nạp đúng `input_ids` + `attention_mask`;
//  - họ E5 huấn luyện với tiền tố `query: ` / `passage: `, đúng thứ
//    `embed_query()` / `embed_passage()` đã thêm sẵn.
// Thay bằng model khác họ thì phải đọc lại cả hai điểm đó.
//
// Dùng:
//   node scripts/fetch-embedding-model.mjs
//   node scripts/fetch-embedding-model.mjs --dir D:/models/e5-small
//
// Cần mạng. Tải khoảng 130 MB. Nếu máy không có mạng, tải tay hai file rồi đặt
// vào thư mục đích — script chỉ là tiện ích, không phải bắt buộc.

import fs from 'node:fs'
import path from 'node:path'

const REPO = 'intfloat/multilingual-e5-small'
const BASE = `https://huggingface.co/${REPO}/resolve/main`

// `model.onnx` nằm trong thư mục con `onnx/` của repo; `tokenizer.json` ở gốc.
const FILES = [
  { url: `${BASE}/onnx/model.onnx`, ten: 'model.onnx', khoangMB: 118 },
  { url: `${BASE}/tokenizer.json`, ten: 'tokenizer.json', khoangMB: 17 },
]

const argDir = process.argv.indexOf('--dir')
const DICH = argDir > -1 && process.argv[argDir + 1]
  ? path.resolve(process.argv[argDir + 1])
  : path.resolve(import.meta.dirname, '..', 'models', 'embedding')

const mb = (n) => (n / 1024 / 1024).toFixed(1) + ' MB'

async function tai({ url, ten, khoangMB }) {
  const dich = path.join(DICH, ten)
  if (fs.existsSync(dich) && fs.statSync(dich).size > 1024) {
    console.log(`  ✓ ${ten} đã có (${mb(fs.statSync(dich).size)}) — bỏ qua`)
    return true
  }
  console.log(`  ↓ ${ten} (~${khoangMB} MB) từ ${REPO}`)

  let res
  try {
    res = await fetch(url, { redirect: 'follow' })
  } catch (e) {
    console.error(`  ✗ ${ten}: không kết nối được — ${e.message}`)
    return false
  }
  if (!res.ok) {
    console.error(`  ✗ ${ten}: HTTP ${res.status} ${res.statusText}`)
    return false
  }

  // Ghi ra file tạm rồi mới đổi tên: tải dở dang mà để nguyên tên thật sẽ bị
  // lần chạy sau tưởng là đã xong, và lõi thì bung lỗi parse ONNX khó hiểu.
  const tam = dich + '.dangtai'
  const buf = Buffer.from(await res.arrayBuffer())
  fs.writeFileSync(tam, buf)
  fs.renameSync(tam, dich)
  console.log(`  ✓ ${ten} — ${mb(buf.length)}`)
  return true
}

const main = async () => {
  console.log(`Thư mục đích: ${DICH}\n`)
  fs.mkdirSync(DICH, { recursive: true })

  let du = true
  for (const f of FILES) du = (await tai(f)) && du

  if (!du) {
    console.log('\n❌ Chưa đủ file. Tải tay cũng được:')
    for (const f of FILES) console.log(`   ${f.url}`)
    console.log(`   rồi đặt vào ${DICH}`)
    process.exit(1)
  }

  console.log('\n✅ Xong. Khởi động lại lõi — dòng WARN "Bo nho dai han TAT" sẽ biến mất.')
  if (DICH !== path.resolve(import.meta.dirname, '..', 'models', 'embedding')) {
    console.log(`   Thư mục không phải mặc định, nhớ đặt: LIVA_EMBEDDING_MODEL_DIR=${DICH}`)
  }
  console.log('   Số ký ức lấy ra mỗi lượt: LIVA_RAG_TOP_K (mặc định 3).')
}

main().catch((e) => { console.error('LỖI:', e); process.exit(1) })
