#!/usr/bin/env node
// Nợ L11 — gate đối chiếu `.env.example` ↔ code đọc env thật.
//
// Vì sao cần: `.env.example` là hợp đồng với người beta. Biến chết trong đó
// khiến người ta cấu hình vô ích; biến code đọc mà tài liệu thiếu thì người
// dùng không biết nó tồn tại. Cả hai đều là "tài liệu lệch code" — loại nợ đã
// bị ghi nhận ít nhất hai lần (L11 gốc 26/07, rà lại 25/08).
//
// Phương pháp so sánh:
//   CODE = mọi literal `LIVA_[A-Z0-9_]+` trong `liva-native-core/src` và
//          `liva-desktop/src-tauri/src` (kể cả chỗ đọc gián tiếp qua helper).
//   DOC  = mọi dòng `^VAR=` trong `.env.example`.
// Hai danh sách cho phép (allowlist) ở dưới ghi rõ từng ngoại lệ CÓ CHỦ ĐÍCH,
// kèm lý do — không có mục nào "quên giải thích".
//
// Dùng:
//   node scripts/check-env-doc.mjs     # thoát 1 nếu lệch

import fs from 'node:fs'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')

const RS_DIRS = ['liva-native-core/src', 'liva-desktop/src-tauri/src']

/**
 * Biến CÓ trong code nhưng cố tình KHÔNG đưa vào `.env.example`, kèm lý do.
 * Chủ yếu là biến của các binary probe/bench (`src/bin/*`) — chúng cần tham số
 * chạy thử một lần, không thuộc hợp đồng cấu hình của người dùng cuối.
 */
const CHO_PHEP_THIEU_TRONG_DOC = new Set([
  // Probe/bench: so sánh model chat (llama.cpp vs piper…)
  'LIVA_CMP_NCTX',
  'LIVA_CMP_NGL',
  'LIVA_CMP_A',
  'LIVA_CMP_B',
  // Probe/bench: gemma-4-E4B vision probe
  'LIVA_GEMMA4_LM',
  'LIVA_GEMMA4_MMPROJ',
  'LIVA_GEMMA4_NCTX',
  'LIVA_GEMMA4_NGL',
  'LIVA_GEMMA4_SKIP_VISION',
  // Probe/bench: Qwen3-VL
  'LIVA_QWENVL_DIR',
  'LIVA_QWENVL_LM',
  'LIVA_QWENVL_MMPROJ',
  'LIVA_QWENVL_NCTX',
  'LIVA_QWENVL_NGL',
  'LIVA_QWENVL_SKIP_VISION',
  // Chỉ test harness dùng
  'LIVA_TEST_FLAG',
  // KHÔNG phải biến môi trường — đây là byte-label của Stronghold vault trong
  // `liva-desktop/src-tauri/src/lib.rs` (định danh khoá dẫn xuất), trùng hợp
  // trông giống tên env mà thôi.
  'LIVA_STRONGHOLD_PERSISTENT_SALT_KEY',
  'LIVA_DEFAULT_SECURE_PASSWORD',
  // KHÔNG phải biến môi trường — magic seal header trong keystore.rs (LIVA_KEY_V1)
  // và tiền tố mã pairing WhatsApp trong channels.rs (LIVA_PAIR_).
  'LIVA_KEY_V1',
  'LIVA_PAIR_',
])

/**
 * Biến CÓ trong `.env.example` nhưng KHÔNG có reader Rust, kèm lý do giữ lại.
 */
const CHO_PHEP_CHET_TRONG_DOC = new Set([
  // ApiManagementView.vue lưu vào .env cho tính năng CHƯA IMPLEMENT — giữ để
  // form UI không mất trường (đã chú thích [CHƯA IMPLEMENT] ngay trong file).
  'REMOTE_CONTROL_ENABLED',
  'TELEGRAM_CHAT_ID',
  'TELEGRAM_ADMIN_ID',
  'ZALO_APP_ID',
  'ZALO_APP_SECRET',
  'ZALO_OA_ACCESS_TOKEN',
  'ZALO_USER_ID',
  'EMAIL_HOST',
  'EMAIL_PORT',
  'EMAIL_USER',
  'EMAIL_PASS',
  // Cùng nhóm [CHƯA IMPLEMENT] với khối trên: ApiManagementView.vue lưu cho
  // nhà cung cấp cloud tương thích OpenAI khi tính năng đó được nối dây.
  'AI_PROVIDER',
  'AI_BASE_URL',
  'AI_MODEL',
  'AI_API_KEY',
])

function* docFiles(dir) {
  for (const entry of fs.readdirSync(dir, { withFileTypes: true })) {
    const full = path.join(dir, entry.name)
    if (entry.isDirectory()) yield* docFiles(full)
    else if (entry.name.endsWith('.rs')) yield full
  }
}

// occAll : MỌI literal ALL-CAPS trong .rs — để kiểm "biến được ghi nhưng không
//          ai đọc" (đủ rộng, vì reader có thể nằm trong crate thư viện như
//          teloxide đọc TELEGRAM_BOT_TOKEN hộ chúng ta).
// livaSet: literal `LIVA_*` — để kiểm ngược "code đọc mà tài liệu thiếu".
const occAll = new Set()
const livaSet = new Set()
for (const dir of RS_DIRS) {
  const absDir = path.join(ROOT, dir)
  if (!fs.existsSync(absDir)) continue
  for (const file of docFiles(absDir)) {
    const text = fs.readFileSync(file, 'utf8')
    for (const m of text.matchAll(/\b[A-Z][A-Z0-9_]{2,}\b/g)) {
      occAll.add(m[0])
    }
    for (const m of text.matchAll(/\bLIVA_[A-Z0-9_]+\b/g)) {
      livaSet.add(m[0])
    }
  }
}

const docText = fs.readFileSync(path.join(ROOT, '.env.example'), 'utf8')
const docSet = new Set()
for (const line of docText.split('\n')) {
  const m = line.match(/^([A-Z0-9_]+)=/)
  if (m) docSet.add(m[1])
}

const loi = []

for (const v of [...docSet].sort()) {
  if (!occAll.has(v) && !CHO_PHEP_CHET_TRONG_DOC.has(v)) {
    loi.push(
      `.env.example khai "${v}" nhưng không file .rs nào nhắc tới nó — xoá khỏi example ` +
        `(hoặc thêm vào CHO_PHEP_CHET_TRONG_DOC nếu cố ý giữ)`,
    )
  }
}
for (const v of [...livaSet].sort()) {
  if (!docSet.has(v) && !CHO_PHEP_THIEU_TRONG_DOC.has(v)) {
    loi.push(
      `code đọc "${v}" nhưng .env.example không đề cập — bổ sung tài liệu ` +
        `(hoặc thêm vào CHO_PHEP_THIEU_TRONG_DOC nếu chỉ dùng bởi probe/bench)`,
    )
  }
}

if (loi.length > 0) {
  console.error('❌ .env.example lệch với code:')
  for (const l of loi) console.error(`  - ${l}`)
  process.exit(1)
}
console.log(
  `✅ .env.example khớp code: ${docSet.size} biến được ghi, ${livaSet.size} biến LIVA_* trong code ` +
    `(${CHO_PHEP_THIEU_TRONG_DOC.size} ngoại lệ probe/test được chấp nhận).`,
)
