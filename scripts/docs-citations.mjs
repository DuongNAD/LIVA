#!/usr/bin/env node
// Dò trích dẫn mã nguồn trong bộ tài liệu — hai dạng neo:
//
//   `path/to/file.rs:123`      TOẠ ĐỘ  (cũ)  — chết mỗi khi mã dịch chuyển
//   `path/to/file.rs#ten_ham`  KÝ HIỆU (mới) — sống sót qua refactor
//
// ## Vì sao đổi sang neo ký hiệu (26/07/2026)
//
// Bản trước chỉ hiểu toạ độ, và nó có HAI lỗ hổng, cả hai đều thuộc loại "luôn
// xanh" — thứ nguy hiểm nhất trong một bộ kiểm:
//
// 1. **Nó bỏ qua 39% trích dẫn mà không nói.** Tài liệu hay viết tên file trần
//    (`lib.rs:1055`, `main.rs:958`). Tên trần trùng nhau nên bộ dò không kết
//    luận được, `skipped++` rồi **im lặng** — biến `skipped` chưa từng được in
//    ra. Đo ngày viết lại: **853/2 184 trích dẫn (39,1%) chưa bao giờ được
//    kiểm**, trong đó có TOÀN BỘ 383 trích `lib.rs` và 274 trích `main.rs` —
//    đúng hai file bị trích nhiều nhất và cũng là hai file sắp bị tách.
//
// 2. **Toạ độ còn trong file thì nó cho qua, dù nội dung đã khác.** Việc gộp
//    hai đường boot (26/07) rút `main.rs` 1 245 → 998 dòng; **243 trích dẫn**
//    lập tức vượt biên và hàng trăm cái khác trỏ sang nội dung khác hẳn. Cổng
//    CI vẫn xanh, vì cả 274 cái đều nằm trong nhóm bị bỏ qua ở (1).
//
// Neo ký hiệu đóng cả hai: tên ký hiệu **định vị được file** (hết mơ hồ), và
// **sống sót khi mã dịch chuyển**. Ký hiệu bị xoá/đổi tên thì báo LỖI THẬT —
// đó chính là lúc tài liệu cần người đọc lại.
//
// ## Dùng
//
//   node scripts/docs-citations.mjs                    # báo cáo
//   node scripts/docs-citations.mjs --json             # cho agent/CI
//   node scripts/docs-citations.mjs --max-unchecked=853 # chốt chống thụt lùi
//   node scripts/docs-citations.mjs --suggest           # gợi ý neo ký hiệu (ĐỌC, không tự áp)
//
// ## Vì sao KHÔNG có `--fix`
//
// Bản nháp của script này từng có `--fix` để đổi hàng loạt. Chạy thử trên đúng
// một tài liệu (`06-thi-giac-passive-va-governor.md`, 94 toạ độ) là đủ để thấy
// nó phải bị bỏ:
//
//     capture.rs:118-146  →  capture.rs#region_rgb        (tài liệu nói `capture_for_vision`)
//     vision/diff.rs:256  →  vision/diff.rs#has_pixel_changed  (nói `DiffEngine::diff_region`)
//     governor.rs:97      →  governor.rs#busy_cpu_threshold    (nói `external_cpu_percent`)
//
// Công cụ chỉ biết ký hiệu nào đang bao dòng đó **hôm nay**. Toạ độ trong tài
// liệu đã lỗi thời từ những đợt refactor trước, nên tự động hoá không "di cư"
// mà **ghim cái sai lại vĩnh viễn** — và tệ hơn: sau khi ghim, bộ dò báo XANH
// cho nó, vì ký hiệu đó có thật.
//
// Đổi một trích dẫn là việc NGỮ NGHĨA: phải đọc câu văn quanh nó để biết nó
// đang nói về cái gì. `--suggest` chỉ đưa ứng viên cho người đọc cân nhắc.
//
// Thoát 1 nếu có neo hỏng, hoặc số trích dẫn không kiểm được vượt `--max-unchecked`.

import fs from 'node:fs'
import path from 'node:path'
import { bocKyHieu, kyHieuBao, hoTroKyHieu } from './lib/symbols.mjs'

const ROOT = path.resolve(import.meta.dirname, '..')
const DOCS = path.join(ROOT, 'docs')

// Khảo sát gốc được ĐÓNG BĂNG có chủ đích để đối chiếu về sau — toạ độ trong
// đó phản ánh mã nguồn tại thời điểm khảo sát và không được sửa theo hiện tại.
const FROZEN = ['00-bao-cao-khao-sat-goc-2026-07.md']

const DUOI = '(?:rs|ts|tsx|vue|toml|json|yml|yaml|cjs|mjs|ps1|md)'
// Dấu `.` được phép ở đầu để bắt `.github/workflows/test.yml:78`.
const DUONG_DAN = `[.A-Za-z0-9_][A-Za-z0-9_/.\\\\-]*\\.${DUOI}`
const CITATION = new RegExp(`(${DUONG_DAN})(?::(\\d+)(?:-(\\d+))?|#([A-Za-z_$][\\w$]*(?:::[A-Za-z_$][\\w$]*)?))`, 'g')

// Đường dẫn cố ý trỏ ra ngoài repo (mã crate bên thứ ba trong ~/.cargo, hoặc ví
// dụ minh hoạ trong văn xuôi). Không phải lỗi tài liệu.
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
      const l = line.replace(/~~[\s\S]*?~~/g, ' ')
      // Thay inline-code không chứa `:`/`#` bằng KHOẢNG TRẮNG, không phải chuỗi
      // rỗng — xoá hẳn sẽ dán hai token cạnh nhau thành đường dẫn ma
      // (`LIVA_TTS_VIENEU` + `tts/mod.rs:158` -> `LIVA_TTS_VIENEUtts/mod.rs`).
      return l.replace(/`[^`]*`/g, (s) => (s.includes(':') || s.includes('#') ? s : ' '))
    })
    .join('\n')
}

// ─── Chỉ mục file + ký hiệu ─────────────────────────────────────────────────

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

const docCache = new Map()
const docFile = (rel) => {
  if (docCache.has(rel)) return docCache.get(rel)
  let v = null
  try {
    const abs = path.join(ROOT, rel)
    if (fs.statSync(abs).isFile()) {
      const txt = fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n')
      // File kết thúc bằng newline không có thêm một dòng rỗng ở cuối — đếm dư
      // một dòng sẽ khiến bộ dò bỏ lọt đúng các toạ độ vượt biên 1 dòng.
      const soDong = txt.split('\n').length - (txt.endsWith('\n') ? 1 : 0)
      v = { soDong, kyHieu: hoTroKyHieu(rel) ? bocKyHieu(rel, txt) : [] }
    }
  } catch { v = null }
  docCache.set(rel, v)
  return v
}

/** Ứng viên file cho một đường dẫn trong tài liệu (có thể nhiều nếu tên trần). */
const ungVien = (raw) => {
  const p = raw.replace(/\\/g, '/').replace(/^[A-Za-z]:\/Project\/LIVA\//i, '')
  if (NGOAI_REPO.some((re) => re.test(p))) return { ngoaiRepo: true, ds: [] }
  const hits = index.get(p)
  if (hits?.length) return { ds: hits }
  return { ds: docFile(p) ? [p] : [] }
}

/** Tìm ký hiệu `id` trong một file. Trả `{line}` hoặc `null`. */
const timKyHieu = (rel, id) => {
  const f = docFile(rel)
  if (!f) return null
  const khop = f.kyHieu.filter((k) => k.id === id)
  if (khop.length === 1) return khop[0]
  if (khop.length > 1) return { ...khop[0], trung: khop.length }
  // Tên trần: chấp nhận khi DUY NHẤT trong file (`#loaded_backends` thay vì
  // `#TtsManager::loaded_backends`) — tài liệu đọc dễ hơn, và trùng thì báo lỗi.
  const theoTen = f.kyHieu.filter((k) => k.ten === id)
  if (theoTen.length === 1) return theoTen[0]
  if (theoTen.length > 1) return { ...theoTen[0], trung: theoTen.length }
  return null
}

// ─── Quét ───────────────────────────────────────────────────────────────────

const argv = process.argv.slice(2)
const maxUnchecked = (() => {
  const a = argv.find((x) => x.startsWith('--max-unchecked='))
  return a ? Number(a.split('=')[1]) : null
})()

const docs = listDocs(DOCS).sort()
const findings = []
const khongKiem = [] // trích dẫn không kết luận được (tên file mơ hồ)
const deXuat = []    // gợi ý đổi toạ độ -> neo ký hiệu
let total = 0
let neoKyHieu = 0

for (const abs of docs) {
  const relDoc = path.relative(ROOT, abs).replace(/\\/g, '/')
  const raw = fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n')
  const lines = raw.split('\n')
  const stripped = stripCode(raw).split('\n')

  stripped.forEach((line, i) => {
    for (const m of line.matchAll(CITATION)) {
      const [ca, file, startS, endS, sym] = m
      // Bỏ qua tự trích dẫn tài liệu (`docs/….md:12` hầu như không xuất hiện).
      if (file.endsWith('.md')) continue
      total++
      const ctx = lines[i]?.trim().slice(0, 160)
      const { ds, ngoaiRepo } = ungVien(file)
      if (ngoaiRepo) continue

      // ── Neo ký hiệu ──────────────────────────────────────────────────────
      if (sym) {
        neoKyHieu++
        // Ký hiệu tự định vị được file: chỉ giữ ứng viên nào CÓ ký hiệu đó.
        const co = ds.map((r) => [r, timKyHieu(r, sym)]).filter(([, k]) => k)
        if (co.length === 0) {
          findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'ky-hieu-khong-ton-tai', context: ctx,
            ungVien: ds.slice(0, 3) })
        } else if (co.length > 1) {
          findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'ky-hieu-mo-ho-giua-file', context: ctx,
            ungVien: co.map(([r]) => r).slice(0, 3) })
        } else if (co[0][1].trung) {
          findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'ky-hieu-trung-trong-file', context: ctx,
            target: co[0][0], trung: co[0][1].trung })
        }
        continue
      }

      // ── Toạ độ (cũ) ──────────────────────────────────────────────────────
      const start = Number(startS)
      const end = endS ? Number(endS) : start
      if (ds.length === 0) {
        findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'file-khong-ton-tai', context: ctx })
        continue
      }
      if (ds.length > 1) {
        // Tên file trùng nhau, nhưng SỐ DÒNG vẫn thu hẹp được: chỉ giữ những
        // ứng viên đủ dài để chứa toạ độ đó.
        const vua = ds.filter((r) => {
          const f = docFile(r)
          return f && start >= 1 && end <= f.soDong
        })
        if (vua.length === 0) {
          // KHÔNG ứng viên nào chứa nổi dòng này ⇒ hỏng thật, bất kể tài liệu
          // định trỏ vào file nào. Đây là nhóm mà bản cũ bỏ qua im lặng — và
          // là nơi 243 trích dẫn `main.rs` chết sau đợt gộp boot 26/07 đã trốn.
          findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'vuot-moi-ung-vien',
            ungVien: ds.map((r) => `${r} (${docFile(r)?.soDong} dòng)`), context: ctx })
          continue
        }
        if (vua.length > 1) {
          khongKiem.push({ doc: relDoc, docLine: i + 1, cite: ca, file, ungVien: vua, start, end, context: ctx })
          continue
        }
        // Đúng một ứng viên chứa được ⇒ coi như đã định vị.
        const f1 = docFile(vua[0])
        const bao1 = kyHieuBao(f1.kyHieu, start)
        if (bao1) deXuat.push({ doc: relDoc, docLine: i + 1, cite: ca, target: vua[0], moi: `${file}#${bao1.id}`, sym: bao1.id })
        continue
      }
      const target = ds[0]
      const f = docFile(target)
      if (start < 1 || end > f.soDong) {
        findings.push({ doc: relDoc, docLine: i + 1, cite: ca, loai: 'so-dong-vuot-file', target,
          fileLines: f.soDong, context: ctx })
        continue
      }
      const bao = kyHieuBao(f.kyHieu, start)
      if (bao) deXuat.push({ doc: relDoc, docLine: i + 1, cite: ca, target, moi: `${file}#${bao.id}`, sym: bao.id })
    }
  })
}

// ─── Báo cáo ────────────────────────────────────────────────────────────────

const vuotChot = maxUnchecked !== null && khongKiem.length > maxUnchecked

if (argv.includes('--json')) {
  console.log(JSON.stringify({
    total, neoKyHieu, docs: docs.length,
    khongKiem: khongKiem.length, maxUnchecked, findings, deXuat: deXuat.length,
  }, null, 2))
} else if (argv.includes('--suggest')) {
  console.log(`${deXuat.length} toạ độ có ứng viên neo ký hiệu.\n`)
  console.log('⚠ ĐÂY LÀ ỨNG VIÊN, KHÔNG PHẢI ĐÁP ÁN. Công cụ chỉ biết ký hiệu nào đang')
  console.log('  bao dòng đó HÔM NAY; toạ độ đã lỗi thời sẽ cho ra ứng viên sai. Đọc câu')
  console.log('  văn quanh trích dẫn để xác nhận trước khi đổi — ví dụ thật đã gặp:')
  console.log('    governor.rs:97 → #busy_cpu_threshold, trong khi tài liệu nói `external_cpu_percent`.\n')
  const theoDoc = new Map()
  for (const d of deXuat) {
    if (!theoDoc.has(d.doc)) theoDoc.set(d.doc, [])
    theoDoc.get(d.doc).push(d)
  }
  const locDoc = argv.find((a) => a.startsWith('docs/'))
  for (const [doc, ds] of [...theoDoc].sort((a, b) => b[1].length - a[1].length)) {
    if (locDoc && doc !== locDoc) continue
    console.log(`  ${doc}  (${ds.length})`)
    // Không cắt bớt khi người dùng đã chỉ đích danh một tài liệu: lúc đó họ
    // đang ngồi đối chiếu, cần thấy đủ.
    const hien = locDoc ? ds : ds.slice(0, 5)
    for (const d of hien) console.log(`    ${d.cite}  →  ${d.moi}`)
    if (!locDoc && ds.length > 5) console.log(`    … và ${ds.length - 5} chỗ nữa (xem đủ: --suggest ${doc})`)
    console.log()
  }
  if (khongKiem.length) {
    console.log(`⚠ Còn ${khongKiem.length} trích dẫn KHÔNG kết luận được (tên file trùng nhau).`)
    console.log('  Chúng cần ghi rõ đường dẫn hoặc đổi sang neo ký hiệu bằng tay.')
  }
} else {
  console.log(`Đã quét ${docs.length} tài liệu, ${total} trích dẫn (${neoKyHieu} neo ký hiệu, ${total - neoKyHieu} toạ độ).\n`)

  if (khongKiem.length) {
    const theoFile = new Map()
    for (const k of khongKiem) theoFile.set(k.file, (theoFile.get(k.file) || 0) + 1)
    const chot = maxUnchecked === null ? '' : ` (chốt: ${maxUnchecked})`
    console.log(`${vuotChot ? '❌' : '⚠'}  ${khongKiem.length} trích dẫn KHÔNG kiểm được${chot} — tên file trùng nhau nên không biết trỏ vào đâu:`)
    for (const [f, n] of [...theoFile].sort((a, b) => b[1] - a[1]).slice(0, 8)) {
      console.log(`     ${String(n).padStart(4)}  ${f}  → ${index.get(f)?.length ?? 0} file trùng tên`)
    }
    if (theoFile.size > 8) console.log(`     … và ${theoFile.size - 8} tên file nữa`)
    console.log('     Sửa bằng neo ký hiệu:  node scripts/docs-citations.mjs --suggest\n')
  }

  if (findings.length === 0) {
    console.log('✅ Không có neo hỏng trong số trích dẫn kiểm được.')
    if (deXuat.length) console.log(`   ${deXuat.length} toạ độ có thể chuyển sang neo ký hiệu (--suggest).`)
  } else {
    const byDoc = new Map()
    for (const f of findings) {
      if (!byDoc.has(f.doc)) byDoc.set(f.doc, [])
      byDoc.get(f.doc).push(f)
    }
    console.log(`❌ ${findings.length} neo hỏng trong ${byDoc.size} tài liệu:\n`)
    for (const [doc, fs_] of [...byDoc].sort((a, b) => b[1].length - a[1].length)) {
      console.log(`  ${doc}  (${fs_.length})`)
      for (const f of fs_.slice(0, 12)) {
        const why = {
          'file-khong-ton-tai': 'file không tồn tại',
          'so-dong-vuot-file': `file chỉ có ${f.fileLines} dòng`,
          'vuot-moi-ung-vien': `không file nào đủ dài: ${f.ungVien?.join(', ')}`,
          'ky-hieu-khong-ton-tai': `không có ký hiệu đó${f.ungVien?.length ? ` (đã tìm trong ${f.ungVien.join(', ')})` : ''}`,
          'ky-hieu-mo-ho-giua-file': `ký hiệu có ở nhiều file: ${f.ungVien?.join(', ')}`,
          'ky-hieu-trung-trong-file': `ký hiệu xuất hiện ${f.trung} lần trong ${f.target} — ghi rõ Kiểu::tên`,
        }[f.loai]
        console.log(`    dòng ${f.docLine}: ${f.cite}  — ${why}`)
      }
      if (fs_.length > 12) console.log(`    … và ${fs_.length - 12} chỗ nữa`)
      console.log()
    }
  }
}

process.exit(findings.length > 0 || vuotChot ? 1 : 0)
