#!/usr/bin/env node
// Dò toạ độ `file:dòng` trong bộ tài liệu.
//
// Vì sao cần: tài liệu LIVA trích dẫn dày đặc theo dạng `path/to/file.rs:123`.
// Mỗi lần mã nguồn dịch chuyển, các toạ độ đó âm thầm trỏ sai — không có gì
// báo lỗi, và người đọc chỉ phát hiện khi mở file ra thấy dòng đó là thứ khác.
// `docs-check.mjs` kiểm liên kết và front-matter; script này kiểm TOẠ ĐỘ.
//
// Nó chỉ bắt được lỗi CƠ HỌC (file không tồn tại, số dòng vượt quá độ dài
// file). Nó KHÔNG biết dòng đó có đúng nội dung được nhắc tới hay không —
// phần ngữ nghĩa vẫn phải người/agent đọc. Đó là giới hạn cố ý: một bộ dò
// cơ học chạy trong một giây có ích hơn một bộ dò thông minh không ai chạy.
//
// Dùng:
//   node scripts/docs-citations.mjs            # báo cáo cho người đọc
//   node scripts/docs-citations.mjs --json     # cho agent/CI tiêu thụ
//
// Thoát 1 nếu có toạ độ hỏng.

import fs from 'node:fs'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')
const DOCS = path.join(ROOT, 'docs')

// Khảo sát gốc được ĐÓNG BĂNG có chủ đích để đối chiếu về sau — toạ độ trong
// đó phản ánh mã nguồn tại thời điểm khảo sát và không được sửa theo hiện tại.
const FROZEN = ['00-bao-cao-khao-sat-goc-2026-07.md']

// Dấu `.` được phép ở đầu để bắt `.github/workflows/test.yml:78`.
const CITATION = /([.A-Za-z0-9_][A-Za-z0-9_/.\\-]*\.(?:rs|ts|tsx|vue|toml|json|yml|yaml|cjs|mjs|ps1|md)):(\d+)(?:-(\d+))?/g

// Đường dẫn cố ý trỏ ra ngoài repo (mã crate của bên thứ ba trong ~/.cargo,
// hoặc ví dụ minh hoạ trong văn xuôi). Không phải lỗi tài liệu.
const NGOAI_REPO = [/^ort-[\d.]+(-rc\.\d+)?\//, /^file\.rs$/, /^path\/to\//]

const listDocs = (dir) =>
  fs.readdirSync(dir, { withFileTypes: true }).flatMap((e) => {
    const p = path.join(dir, e.name)
    if (e.isDirectory()) return e.name === '99-luu-tru' ? [] : listDocs(p)
    if (!e.name.endsWith('.md')) return []
    if (FROZEN.includes(e.name)) return []
    return [p]
  })

// Bỏ khối mã: trong ví dụ code, `foo.rs:12` thường là output mẫu chứ không
// phải trích dẫn. Quét theo DÒNG vì hàng rào ``` có thể thụt lề trong danh
// sách đánh số — regex một phát sẽ bỏ sót đúng những chỗ đó.
const stripCode = (text) => {
  let fence = null
  return text
    .split('\n')
    .map((line) => {
      const m = line.match(/^\s*(`{3,}|~{3,})/)
      if (m) {
        if (fence === null) { fence = m[1][0].repeat(3); return '' }
        if (m[1][0].repeat(3) === fence) { fence = null; return '' }
        return ''
      }
      if (fence !== null) return ''
      // Bỏ đoạn gạch ngang `~~…~~`: tài liệu dùng nó để giữ lại trích dẫn
      // LỊCH SỬ ("~~signaling.rs:24~~ — đã xoá"). Đó là chủ ý, không phải lỗi;
      // nếu báo động ở đây thì người viết sẽ bị ép xoá luôn phần bối cảnh.
      let l = line.replace(/~~[\s\S]*?~~/g, ' ')
      // Thay inline-code không chứa `:` bằng KHOẢNG TRẮNG, không phải chuỗi
      // rỗng — xoá hẳn sẽ dán hai token cạnh nhau thành đường dẫn ma
      // (`LIVA_TTS_VIENEU` + `tts/mod.rs:158` -> `LIVA_TTS_VIENEUtts/mod.rs`).
      return l.replace(/`[^`]*`/g, (s) => (s.includes(':') ? s : ' '))
    })
    .join('\n')
}

const lineCache = new Map()
const lineCount = (rel) => {
  if (lineCache.has(rel)) return lineCache.get(rel)
  const abs = path.join(ROOT, rel)
  let n = null
  try {
    if (fs.statSync(abs).isFile()) {
      const txt = fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n')
      // File kết thúc bằng newline không có thêm một dòng rỗng ở cuối — đếm
      // dư một dòng sẽ khiến bộ dò bỏ lọt đúng các toạ độ vượt biên 1 dòng.
      n = txt.split('\n').length - (txt.endsWith('\n') ? 1 : 0)
    }
  } catch { n = null }
  lineCache.set(rel, n)
  return n
}

// Tài liệu trích dẫn theo nhiều kiểu: đường dẫn đầy đủ từ gốc repo, đường dẫn
// tương đối từ liva-native-core, tuyệt đối Windows, và rất thường xuyên là
// TÊN FILE TRẦN (`main.rs:446`, `vad.rs:49`). Kiểu cuối chỉ giải được bằng
// cách dò ngược toàn repo, và có thể trùng tên (`mod.rs` có hàng chục bản).
const IGNORE_DIRS = new Set([
  'node_modules', 'target', 'dist', '.git', 'models', '.gitnexus', 'docs',
  'venv', '__pycache__', '.venv',
])

const index = new Map() // hậu tố đường dẫn -> [rel...]
const addToIndex = (rel) => {
  const parts = rel.split('/')
  for (let i = parts.length - 1; i >= 0; i--) {
    const suffix = parts.slice(i).join('/')
    if (!index.has(suffix)) index.set(suffix, [])
    index.get(suffix).push(rel)
  }
}
const walk = (dir) => {
  let entries
  try { entries = fs.readdirSync(path.join(ROOT, dir), { withFileTypes: true }) } catch { return }
  for (const e of entries) {
    if (IGNORE_DIRS.has(e.name)) continue
    const rel = dir === '.' ? e.name : `${dir}/${e.name}`
    if (e.isDirectory()) walk(rel)
    else addToIndex(rel)
  }
}
walk('.')

const resolveRel = (raw) => {
  const p = raw.replace(/\\/g, '/').replace(/^[A-Za-z]:\/Project\/LIVA\//i, '')
  if (NGOAI_REPO.some((re) => re.test(p))) return { rel: null, ngoaiRepo: true }
  const hits = index.get(p)
  // Nếu hậu tố khớp nhiều file (`Cargo.toml`, `mod.rs`, `main.rs`…) thì không
  // kết luận được — kể cả khi tình cờ có một bản ở gốc repo. Ưu tiên bản gốc
  // sẽ cho kết quả SAI: `Cargo.toml:71-139` trong tài liệu nói về
  // liva-native-core, không phải Cargo.toml workspace 14 dòng.
  if (hits && hits.length > 1) return { rel: null, ambiguous: hits.length }
  if (hits && hits.length === 1) return { rel: hits[0] }
  if (lineCount(p) !== null) return { rel: p }
  return { rel: null }
}

const docs = listDocs(DOCS).sort()
const findings = []
let total = 0
let skipped = 0

for (const abs of docs) {
  const rel = path.relative(ROOT, abs).replace(/\\/g, '/')
  const raw = fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n')
  const lines = raw.split('\n')
  const stripped = stripCode(raw).split('\n')

  stripped.forEach((line, i) => {
    for (const m of line.matchAll(CITATION)) {
      const [, file, startS, endS] = m
      // Bỏ qua tự trích dẫn tài liệu (`docs/....md:12` hầu như không xuất hiện)
      if (file.endsWith('.md')) continue
      total++
      const { rel: target, ambiguous, ngoaiRepo } = resolveRel(file)
      if (ambiguous || ngoaiRepo) { skipped++; continue } // trùng tên hoặc ngoài repo
      if (target === null) {
        findings.push({ doc: rel, docLine: i + 1, cite: m[0], loai: 'file-khong-ton-tai', context: lines[i]?.trim().slice(0, 160) })
        continue
      }
      const n = lineCount(target)
      const start = Number(startS)
      const end = endS ? Number(endS) : start
      if (start < 1 || end > n) {
        findings.push({ doc: rel, docLine: i + 1, cite: m[0], loai: 'so-dong-vuot-file', target, fileLines: n, context: lines[i]?.trim().slice(0, 160) })
      }
    }
  })
}

if (process.argv.includes('--json')) {
  console.log(JSON.stringify({ total, docs: docs.length, findings }, null, 2))
} else {
  console.log(`Đã quét ${docs.length} tài liệu, ${total} toạ độ file:dòng.\n`)
  if (findings.length === 0) {
    console.log('✅ Không có toạ độ hỏng về mặt cơ học.')
    console.log('   (Lưu ý: script này KHÔNG kiểm được dòng đó có đúng nội dung hay không.)')
  } else {
    const byDoc = new Map()
    for (const f of findings) {
      if (!byDoc.has(f.doc)) byDoc.set(f.doc, [])
      byDoc.get(f.doc).push(f)
    }
    console.log(`❌ ${findings.length} toạ độ hỏng trong ${byDoc.size} tài liệu:\n`)
    for (const [doc, fs_] of [...byDoc].sort((a, b) => b[1].length - a[1].length)) {
      console.log(`  ${doc}  (${fs_.length})`)
      for (const f of fs_.slice(0, 12)) {
        const why = f.loai === 'file-khong-ton-tai'
          ? 'file không tồn tại'
          : `file chỉ có ${f.fileLines} dòng`
        console.log(`    dòng ${f.docLine}: ${f.cite}  — ${why}`)
      }
      if (fs_.length > 12) console.log(`    … và ${fs_.length - 12} chỗ nữa`)
      console.log()
    }
  }
}

process.exit(findings.length > 0 ? 1 : 0)
