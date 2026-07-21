#!/usr/bin/env node
/**
 * docs-check.mjs — kiểm tra sức khoẻ bộ tài liệu trong docs/
 *
 * Chạy:
 *   node scripts/docs-check.mjs            # kiểm tra, thoát 1 nếu có lỗi
 *   node scripts/docs-check.mjs --map      # kiểm tra + sinh lại docs/_meta/ban-do-code-tai-lieu.md
 *   node scripts/docs-check.mjs --quiet    # chỉ in lỗi
 *
 * Kiểm 7 thứ:
 *   1. Front-matter YAML hợp lệ, đủ trường bắt buộc
 *   2. LỖI THỜI — file mã nguồn trong `covers` đã đổi kể từ commit ghi trong front-matter
 *   3. Liên kết markdown tương đối trỏ tới file có thật
 *   4. `covers` trỏ tới đường dẫn có thật trong repo
 *   5. Khoá `owns` không bị hai tài liệu cùng nhận
 *   6. Dòng "📌 Nguồn đầy đủ:" trỏ tới tài liệu thật sự sở hữu sự thật đó
 *   7. Khối ```mermaid đóng fence cân bằng
 *   + cảnh báo: file mã nguồn quan trọng chưa tài liệu nào `covers`
 */
import fs from 'node:fs'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

const REPO = path.resolve(path.join(import.meta.dirname, '..'))
const DOCS = path.join(REPO, 'docs')
const ARGS = new Set(process.argv.slice(2))
const QUIET = ARGS.has('--quiet')

const norm = (p) => p.split(path.sep).join('/')
const rel = (p) => norm(path.relative(REPO, p))

const errors = []
const warns = []
const err = (f, m) => errors.push(`${f}: ${m}`)
const warn = (f, m) => warns.push(`${f}: ${m}`)
const say = (...a) => { if (!QUIET) console.log(...a) }

// ---------------------------------------------------------------- thu thập
const docFiles = []
const walk = (d) => {
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    const p = path.join(d, e.name)
    if (e.isDirectory()) {
      if (e.name === '99-luu-tru' || e.name === 'assets' || e.name === 'prompts') continue
      walk(p)
    } else if (e.name.endsWith('.md')) docFiles.push(p)
  }
}
if (!fs.existsSync(DOCS)) { console.error('Không tìm thấy thư mục docs/'); process.exit(1) }
walk(DOCS)

// ------------------------------------------------- phân tích front-matter
/** Bộ phân tích YAML tối giản: chỉ đủ cho lược đồ phẳng + danh sách "- item". */
function parseFrontMatter(text) {
  if (!text.startsWith('---')) return null
  const end = text.indexOf('\n---', 3)
  if (end < 0) return null
  const body = text.slice(text.indexOf('\n') + 1, end)
  const out = {}
  let key = null
  for (const raw of body.split('\n')) {
    const line = raw.replace(/\r$/, '')
    if (!line.trim() || line.trim().startsWith('#')) continue
    const item = line.match(/^\s+-\s+(.*)$/)
    if (item && key) { out[key].push(item[1].trim().replace(/^["']|["']$/g, '')); continue }
    const kv = line.match(/^([A-Za-z_][A-Za-z0-9_]*):\s*(.*)$/)
    if (!kv) continue
    key = kv[1]
    const v = kv[2].trim()
    if (v === '' ) out[key] = []
    else if (v === '[]') out[key] = []
    else out[key] = v.replace(/^["']|["']$/g, '')
  }
  return { data: out, endIndex: end }
}

const docs = new Map() // relPath -> {fm, text}
for (const f of docFiles) {
  const text = fs.readFileSync(f, 'utf8')
  const fm = parseFrontMatter(text)
  docs.set(rel(f), { fm: fm?.data ?? null, text, abs: f })
}

const REQUIRED = ['title', 'updated', 'commit', 'status']
const VALID_STATUS = new Set(['living', 'frozen', 'index'])

// ------------------------------------------------------------ 1. lược đồ
for (const [p, d] of docs) {
  if (!d.fm) { err(p, 'thiếu front-matter YAML ở đầu file'); continue }
  for (const k of REQUIRED) if (!(k in d.fm)) err(p, `front-matter thiếu trường \`${k}\``)
  if (d.fm.status && !VALID_STATUS.has(d.fm.status))
    err(p, `status="${d.fm.status}" không hợp lệ (phải là living | frozen | index)`)
}

// ------------------------------------------------ 2. covers có thật + lỗi thời
const expandCovers = (globs) => {
  const out = []
  for (const g of globs || []) {
    if (g.endsWith('/*')) {
      const dir = path.join(REPO, g.slice(0, -2))
      if (!fs.existsSync(dir)) { out.push({ pattern: g, missing: true }); continue }
      out.push({ pattern: g, dir: g.slice(0, -2) })
    } else {
      const abs = path.join(REPO, g)
      out.push({ pattern: g, missing: !fs.existsSync(abs), file: g })
    }
  }
  return out
}

let gitOk = true
try { execFileSync('git', ['rev-parse', '--git-dir'], { cwd: REPO, stdio: 'pipe' }) }
catch { gitOk = false; warns.push('(git không khả dụng — bỏ qua kiểm tra lỗi thời)') }

const staleReport = []
for (const [p, d] of docs) {
  if (!d.fm) continue
  const covers = Array.isArray(d.fm.covers) ? d.fm.covers : []
  for (const c of expandCovers(covers)) if (c.missing) err(p, `covers trỏ tới đường dẫn không tồn tại: \`${c.pattern}\``)

  // Cover quá rộng làm tài liệu tự đánh dấu chính nó lỗi thời (mọi commit đều
  // khớp), biến cảnh báo lỗi thời thành nhiễu vô dụng. Liệt kê file gốc repo
  // một cách tường minh thay vì gom thành `./*`.
  for (const c of covers)
    if (['.', './*', '*', './', '**', '**/*', 'docs', 'docs/*'].includes(c.trim()))
      err(p, `covers có mục quá rộng \`${c}\` — hãy liệt kê tường minh từng file/thư mục con`)

  if (!gitOk || d.fm.status !== 'living' || !covers.length) continue

  const paths = covers.map((c) => (c.endsWith('/*') ? c.slice(0, -2) : c)).filter((c) => fs.existsSync(path.join(REPO, c)))
  if (!paths.length) continue
  let changed = ''
  try {
    changed = execFileSync('git', ['log', '--name-only', '--format=%h', `${d.fm.commit}..HEAD`, '--', ...paths],
      { cwd: REPO, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim()
  } catch {
    warn(p, `không đối chiếu được commit \`${d.fm.commit}\` (commit không tồn tại trong lịch sử?)`)
    continue
  }
  if (changed) {
    const files = [...new Set(changed.split('\n').filter((l) => l && !/^[0-9a-f]{7,}$/.test(l)))]
    staleReport.push({ doc: p, since: d.fm.commit, files })
  }
}

// ------------------------------------------------------------- 3. liên kết
/**
 * Gỡ khối ``` và inline `code` trước khi quét liên kết. Không làm việc này thì
 * regex trong tài liệu (ví dụ `\b0[35789](?:[\s.\-]?[0-9]){8,9}\b`) và template
 * đường dẫn trong ví dụ (`{03-ten-truoc}.md`) sẽ bị nhận nhầm là liên kết hỏng.
 * Thay bằng khoảng trắng cùng độ dài để giữ nguyên số dòng khi báo lỗi.
 */
const stripCode = (text) => {
  const out = []
  let fence = null // dấu fence đang mở: ``` hoặc ~~~ (giữ nguyên độ dài để khớp fence đóng)
  for (const line of text.split('\n')) {
    const m = line.match(/^(\s*)(`{3,}|~{3,})(.*)$/)
    if (fence) {
      out.push(' '.repeat(line.length))
      if (m && m[2][0] === fence[0] && m[2].length >= fence.length && !m[3].trim()) fence = null
      continue
    }
    if (m) { fence = m[2]; out.push(' '.repeat(line.length)); continue }
    out.push(line.replace(/`[^`]*`/g, (s) => ' '.repeat(s.length)))
  }
  return out.join('\n')
}

const LINK = /\[([^\]\n]{1,120})\]\(([^)\s]+)\)/g
for (const [p, d] of docs) {
  for (const m of stripCode(d.text).matchAll(LINK)) {
    let t = m[2]
    if (/^(https?:|mailto:|#)/.test(t)) continue
    t = t.split('#')[0]
    if (!t) continue
    const target = path.resolve(path.dirname(path.join(REPO, p)), decodeURIComponent(t))
    if (!fs.existsSync(target)) err(p, `liên kết hỏng → \`${m[2]}\``)
  }
}

// ------------------------------------------------------- 5. owns duy nhất
const ownerOf = new Map()
for (const [p, d] of docs) {
  for (const k of (Array.isArray(d.fm?.owns) ? d.fm.owns : [])) {
    if (ownerOf.has(k)) err(p, `khoá owns \`${k}\` đã được ${ownerOf.get(k)} nhận sở hữu`)
    else ownerOf.set(k, p)
  }
}

// ------------------------------------- 6. con trỏ "Nguồn đầy đủ" hợp lệ
const POINTER = /📌\s*Nguồn đầy đủ:\s*\[([^\]]+)\]\(([^)\s]+)\)/g
let pointers = 0
for (const [p, d] of docs) {
  for (const m of stripCode(d.text).matchAll(POINTER)) {
    pointers++
    const t = m[2].split('#')[0]
    const target = path.resolve(path.dirname(path.join(REPO, p)), decodeURIComponent(t))
    const key = rel(target)
    if (!fs.existsSync(target)) { err(p, `"Nguồn đầy đủ" trỏ tới file không tồn tại: \`${m[2]}\``); continue }
    const td = docs.get(key)
    if (td && Array.isArray(td.fm?.owns) && td.fm.owns.length === 0)
      warn(p, `"Nguồn đầy đủ" trỏ tới ${key} nhưng tài liệu đó không khai sở hữu sự thật nào`)
  }
}

// ------------------------------------------------------------ 7. mermaid
let mermaidCount = 0
for (const [p, d] of docs) {
  const fences = d.text.split('\n').filter((l) => l.trimStart().startsWith('```'))
  let open = 0
  for (const f of fences) { open = open === 0 ? 1 : 0 }
  if (open !== 0) err(p, 'khối ``` không cân bằng (thiếu fence đóng)')
  mermaidCount += (d.text.match(/^```mermaid/gm) || []).length
}

// ---------------------------------- cảnh báo: mã nguồn chưa được tài liệu hoá
const covered = new Set()
for (const [, d] of docs)
  for (const c of (Array.isArray(d.fm?.covers) ? d.fm.covers : []))
    covered.add(c.endsWith('/*') ? c.slice(0, -2) : c)
const isCovered = (f) => { for (const c of covered) if (f === c || f.startsWith(c + '/')) return true; return false }

const srcFiles = []
const walkSrc = (d, depth = 0) => {
  if (depth > 6 || !fs.existsSync(d)) return
  for (const e of fs.readdirSync(d, { withFileTypes: true })) {
    if (/^(node_modules|target|dist|build|__pycache__|\.gradle)$/.test(e.name)) continue
    const p = path.join(d, e.name)
    if (e.isDirectory()) walkSrc(p, depth + 1)
    else if (/\.(rs|ts|vue)$/.test(e.name)) srcFiles.push(rel(p))
  }
}
walkSrc(path.join(REPO, 'liva-native-core/src'))
walkSrc(path.join(REPO, 'liva-ui/src'))
walkSrc(path.join(REPO, 'liva-desktop/src-tauri/src'))
const uncovered = srcFiles.filter((f) => !isCovered(f) && !/\/(mod|index)\.(rs|ts)$/.test(f))

// -------------------------------------------------------------- sinh bản đồ
if (ARGS.has('--map')) {
  const rev = new Map()
  for (const [p, d] of docs)
    for (const c of (Array.isArray(d.fm?.covers) ? d.fm.covers : [])) {
      if (!rev.has(c)) rev.set(c, [])
      rev.get(c).push(p)
    }
  const rows = [...rev.entries()].sort(([a], [b]) => a.localeCompare(b)).map(([src, ds]) => {
    const links = ds.map((p) => `[${path.basename(p)}](../${norm(path.relative('docs', p))})`).join(' · ')
    return `| \`${src}\` | ${links} |`
  })
  const out = [
    '---',
    'title: "Bản đồ code ↔ tài liệu"',
    `updated: ${new Date().toISOString().slice(0, 10)}`,
    'commit: auto',
    'status: index',
    'owns: []',
    'covers: []',
    '---',
    '',
    '# Bản đồ code ↔ tài liệu',
    '',
    '> ⚙️ **File này được sinh tự động.** Đừng sửa tay — chạy `node scripts/docs-check.mjs --map`.',
    '',
    'Tra ngược: sửa file mã nguồn nào thì phải xem lại tài liệu nào. Dữ liệu lấy từ trường `covers`',
    'trong front-matter của từng tài liệu.',
    '',
    `Tổng: **${rev.size}** mục mã nguồn được tài liệu hoá bởi **${docs.size}** tài liệu.`,
    '',
    '| Mã nguồn | Tài liệu mô tả nó |',
    '|---|---|',
    ...rows,
    '',
    '## Chưa được tài liệu nào mô tả',
    '',
    uncovered.length
      ? uncovered.map((f) => `- \`${f}\``).join('\n')
      : '_Không có — mọi file mã nguồn đều nằm trong `covers` của ít nhất một tài liệu._',
    '',
  ].join('\n')
  const mapPath = path.join(DOCS, '_meta', 'ban-do-code-tai-lieu.md')
  fs.mkdirSync(path.dirname(mapPath), { recursive: true })
  fs.writeFileSync(mapPath, out, 'utf8')
  say(`✅ Đã sinh lại ${rel(mapPath)} (${rev.size} mục mã nguồn)`)
}

// ------------------------------------------------------------------ báo cáo
say('')
say('════════ KIỂM TRA TÀI LIỆU LIVA ════════')
say(`Tài liệu       : ${docs.size}`)
say(`Sơ đồ mermaid  : ${mermaidCount}`)
say(`Khoá sở hữu    : ${ownerOf.size}`)
say(`Con trỏ 📌      : ${pointers}`)
say(`Mã nguồn chưa tài liệu hoá : ${uncovered.length}`)

if (staleReport.length) {
  console.log('')
  console.log('⚠️  TÀI LIỆU CÓ THỂ ĐÃ LỖI THỜI (mã nguồn đổi sau commit ghi trong front-matter):')
  for (const s of staleReport) {
    console.log(`  • ${s.doc}  (ghi commit ${s.since})`)
    for (const f of s.files.slice(0, 8)) console.log(`      ↳ ${f}`)
    if (s.files.length > 8) console.log(`      ↳ … và ${s.files.length - 8} file nữa`)
  }
  console.log('   → Sửa tài liệu rồi cập nhật `updated:` và `commit:` trong front-matter.')
}

if (warns.length) {
  console.log('')
  console.log('CẢNH BÁO:')
  for (const w of warns) console.log('  ! ' + w)
}

if (errors.length) {
  console.log('')
  console.log('LỖI:')
  for (const e of errors) console.log('  ✗ ' + e)
  console.log('')
  console.log(`❌ ${errors.length} lỗi.`)
  process.exit(1)
}

say('')
say(staleReport.length ? '⚠️  Không có lỗi, nhưng có tài liệu cần rà lại (xem trên).' : '✅ Tài liệu sạch.')
process.exit(0)
