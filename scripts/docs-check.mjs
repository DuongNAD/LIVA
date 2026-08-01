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
 *   3. Liên kết markdown tương đối trỏ tới file có thật — **và** neo `#anchor`
 *      trỏ tới một tiêu đề có thật (xem §3a)
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

// `--strict-stale=docs/03-danh-gia[,docs/khac]` — với các thư mục này, LỖI THỜI
// là LỖI chứ không phải cảnh báo.
//
// # Vì sao cần escape valve `stale-ok`, và vì sao siết thô sẽ phản tác dụng
//
// Đo ngày 26/07/2026: **18/30 commit gần nhất** chạm `liva-native-core/src/`,
// mà ba tài liệu `03-danh-gia/01,02,03` đều khai `covers: liva-native-core/src/*`.
// Siết thô ⇒ gate đỏ ở 60% commit, và cách dập duy nhất là sửa `commit:` trong
// front-matter. Một gate nổ liên tục mà dập được bằng một dòng hash sẽ bị dập
// MÙ — biến cảnh báo trung thực hôm nay thành xanh DỐI, tức tệ hơn hiện trạng.
//
// Nên tách hai lời khẳng định vốn đang bị gộp làm một:
//   commit:   "tôi đã đối chiếu NỘI DUNG tài liệu tới commit này"
//   stale-ok: "tôi đã ĐỌC DIFF tới commit này và xác nhận không cần sửa gì"
//
// Cả hai đều là một dòng, nhưng chỉ cái sau là trung thực khi bạn không sửa gì.
// `stale-ok` còn grep được để kiểm toán: "tài liệu nào đang sống nhờ stale-ok,
// và nó cũ bao lâu rồi".
const STRICT_STALE = (() => {
  const raw = [...ARGS].find((a) => a.startsWith('--strict-stale'))
  if (!raw) return []
  const val = raw.includes('=') ? raw.slice(raw.indexOf('=') + 1) : ''
  if (!val.trim()) {
    console.error('--strict-stale cần giá trị, ví dụ: --strict-stale=docs/03-danh-gia')
    process.exit(2)
  }
  return val.split(',').map((s) => s.trim().replace(/\/+$/, '')).filter(Boolean)
})()
const inStrictScope = (p) => STRICT_STALE.some((d) => p === d || p.startsWith(d + '/'))

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
    // Cho phép `-` trong tên khoá để đọc được `stale-ok:`. Không khoá hiện có
    // nào chứa `-`, nên nới ở đây không đổi hành vi với tài liệu cũ.
    const kv = line.match(/^([A-Za-z_][A-Za-z0-9_-]*):\s*(.*)$/)
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
  // Chuẩn hoá CRLF -> LF. Trên Windows, git checkout với core.autocrlf=true trả
  // về file CRLF, còn file mới tạo thường là LF — cùng một nội dung cho kết quả
  // khác nhau giữa máy local và CI nếu không chuẩn hoá. (Đã từng làm CI đỏ.)
  const text = fs.readFileSync(f, 'utf8').replace(/\r\n/g, '\n')
  const fm = parseFrontMatter(text)
  docs.set(rel(f), { fm: fm?.data ?? null, text, abs: f })
}

// Ba file markdown ở GỐC repo. Chúng không có front-matter (và không cần), nên
// nằm ngoài `docs` Map — nhưng vẫn phải được kiểm liên kết.
//
// Vì sao thêm (26/07/2026, U6): đây là ba file được đọc NHIỀU NHẤT — mọi phiên
// agent đọc `AGENTS.md` + `CLAUDE.md`, mọi người mới đọc `README.md` — mà lại là
// ba file DUY NHẤT nằm ngoài mọi cổng kiểm liên kết. Hệ quả đo được: con trỏ
// `AGENTS.md` → `LIVA_NATIVE_MIGRATION_PLAN.md` chết từ 21/07/2026 (file được
// chuyển vào `docs/99-luu-tru/`) và sống sót 5 ngày, tốn thời gian của mọi phiên
// đi tìm nó.
const ROOT_DOCS = ['AGENTS.md', 'CLAUDE.md', 'README.md']
const rootDocs = new Map()
for (const f of ROOT_DOCS) {
  const abs = path.join(REPO, f)
  if (fs.existsSync(abs)) rootDocs.set(f, { text: fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n'), abs })
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
let headSha = 'HEAD'
try {
  execFileSync('git', ['rev-parse', '--git-dir'], { cwd: REPO, stdio: 'pipe' })
  // Dùng trong thông điệp lỗi để người sửa copy-paste được ngay, thay vì phải
  // tự chạy `git rev-parse` rồi mới biết điền gì vào front-matter.
  headSha = execFileSync('git', ['rev-parse', '--short', 'HEAD'],
    { cwd: REPO, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim() || 'HEAD'
} catch { gitOk = false; warns.push('(git không khả dụng — bỏ qua kiểm tra lỗi thời)') }

/** true nếu đường dẫn bị .gitignore loại trừ (nên vắng mặt trên clone sạch là bình thường). */
const isIgnored = (p) => {
  if (!gitOk) return false
  try {
    execFileSync('git', ['check-ignore', '-q', p.endsWith('/*') ? p.slice(0, -2) : p],
      { cwd: REPO, stdio: 'pipe' })
    return true
  } catch { return false }
}

const staleReport = []
for (const [p, d] of docs) {
  if (!d.fm) continue
  const covers = Array.isArray(d.fm.covers) ? d.fm.covers : []
  for (const c of expandCovers(covers)) {
    if (!c.missing) continue
    // File bị .gitignore (ví dụ data/user_profile.json — sinh lúc chạy) không có
    // trên clone sạch của CI. Tài liệu vẫn được phép mô tả nó, nên chỉ cảnh báo.
    if (isIgnored(c.pattern)) warn(p, `covers trỏ tới đường dẫn bị gitignore, không có trên clone sạch: \`${c.pattern}\``)
    else err(p, `covers trỏ tới đường dẫn không tồn tại: \`${c.pattern}\``)
  }

  // Cover quá rộng làm tài liệu tự đánh dấu chính nó lỗi thời (mọi commit đều
  // khớp), biến cảnh báo lỗi thời thành nhiễu vô dụng. Liệt kê file gốc repo
  // một cách tường minh thay vì gom thành `./*`.
  for (const c of covers)
    if (['.', './*', '*', './', '**', '**/*', 'docs', 'docs/*'].includes(c.trim()))
      err(p, `covers có mục quá rộng \`${c}\` — hãy liệt kê tường minh từng file/thư mục con`)

  if (!gitOk || d.fm.status !== 'living' || !covers.length) continue

  const paths = covers.map((c) => (c.endsWith('/*') ? c.slice(0, -2) : c)).filter((c) => fs.existsSync(path.join(REPO, c)))
  if (!paths.length) continue

  /** File trong `covers` đã đổi kể từ `base`; null nếu `base` không có trong lịch sử. */
  const changedSince = (base) => {
    try {
      const out = execFileSync('git', ['log', '--name-only', '--format=%h', `${base}..HEAD`, '--', ...paths],
        { cwd: REPO, encoding: 'utf8', stdio: ['pipe', 'pipe', 'pipe'] }).trim()
      return [...new Set(out.split('\n').filter((l) => l && !/^[0-9a-f]{7,}$/.test(l)))]
    } catch { return null }
  }

  const files = changedSince(d.fm.commit)
  if (files === null) {
    warn(p, `không đối chiếu được commit \`${d.fm.commit}\` (commit không tồn tại trong lịch sử?)`)
    continue
  }
  if (!files.length) continue

  // Tài liệu ĐANG lỗi thời. `stale-ok` là lời khẳng định riêng — "đã đọc diff tới
  // commit này, không cần sửa gì" — nên nó chỉ dập được đúng phần diff nó phủ.
  // Thay đổi phát sinh SAU `stale-ok` vẫn nổi lên như thường.
  const staleOk = typeof d.fm['stale-ok'] === 'string' ? d.fm['stale-ok'].trim() : ''
  let acknowledged = false
  if (staleOk) {
    const rest = changedSince(staleOk)
    if (rest === null) err(p, `\`stale-ok: ${staleOk}\` không phải commit có trong lịch sử`)
    else if (!rest.length) acknowledged = true
  }
  if (acknowledged) continue

  const strict = inStrictScope(p)
  staleReport.push({ doc: p, since: d.fm.commit, files, strict, staleOk })
  if (strict) {
    errors.push(
      `${p}: LỖI THỜI — ${files.length} file trong \`covers\` đã đổi kể từ \`${d.fm.commit}\`` +
      (staleOk ? ` (\`stale-ok: ${staleOk}\` không phủ hết)` : '') +
      `. Sửa nội dung rồi đặt \`commit: ${headSha}\`, HOẶC nếu đọc diff thấy không cần sửa gì thì đặt \`stale-ok: ${headSha}\``,
    )
  }
}

// ------------------------------------------------------------- 3. liên kết
/**
 * Gỡ khối ``` và inline `code` trước khi quét liên kết. Không làm việc này thì
 * regex trong tài liệu (ví dụ `\b0[35789](?:[\s.\-]?[0-9]){8,9}\b`) và template
 * đường dẫn trong ví dụ (`{03-ten-truoc}.md`) sẽ bị nhận nhầm là liên kết hỏng.
 * Thay bằng khoảng trắng cùng độ dài để giữ nguyên số dòng khi báo lỗi.
 */
const stripCode = (text, keepInline = false) => {
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
    // `keepInline`: GIỮ lại inline `code`. Bắt buộc cho bộ dò đường dẫn tuyệt đối —
    // quy ước của bộ tài liệu là bọc mọi đường dẫn trong backtick, nên xoá inline
    // code sẽ làm bộ dò mù đúng thứ nó cần thấy. Chính con trỏ chết trong
    // `AGENTS.md` (bug sinh ra luật này) nằm trong backtick.
    out.push(keepInline ? line : line.replace(/`[^`]*`/g, (s) => ' '.repeat(s.length)))
  }
  return out.join('\n')
}

// --------------------------------------------------- 3a. neo `#anchor` có thật
//
// Vì sao cần (đo 01/08/2026): bản trước của vòng lặp §3 làm đúng một dòng
// `t = t.split('#')[0]` — tức nó kiểm FILE rồi **vứt neo đi**. Quét tay hôm đó
// tìm ra **4 neo hỏng / 5 chỗ dùng** trên 76 liên kết, và `docs-citations.mjs`
// cũng không thấy vì nó chỉ kiểm trích dẫn MÃ NGUỒN. Tức là mục lục nội bộ của
// bộ tài liệu chưa từng được kiểm bởi bất cứ cổng nào.
//
// Cả 4 đều cùng một chế độ hỏng: **đổi tiêu đề mà quên liên kết trỏ tới** — nối
// thêm đuôi ("(đo lại đủ, thay bảng 26/07)"), gạch ngang tiêu đề khi đánh dấu
// XONG, hoặc đổi hẳn chữ. Không lần nào người sửa biết mình vừa làm hỏng gì.
/**
 * Sinh slug đúng cách GitHub sinh neo tiêu đề.
 *
 * **Bẫy đã cắn khi viết bộ dò tay:** dùng `\s+` để gom khoảng trắng sẽ báo
 * 26/26 liên kết hỏng trong khi thực tế chỉ 2. Em dash `—` bị **xoá**, nhưng hai
 * khoảng trắng quanh nó **ở lại** và thành `--`. Phải thay TỪNG khoảng trắng
 * bằng một `-`, không gộp. Cùng họ bẫy với `.` trong `` `Input.dispatchKeyEvent` ``:
 * dấu chấm bị xoá hẳn (→ `inputdispatchkeyevent`), không đổi thành `-` như
 * GitLab — đoán sai một ký tự là hỏng cả liên kết.
 *
 * **Bẫy thứ hai, do chính cổng này bắt được ở lần chạy đầu.** `trim()` phải chạy
 * TRƯỚC bước xoá ký tự, đúng thứ tự của `github-slugger`. Bản đầu trim sau, nên
 * `## 🔮 Future Roadmap` ra `future-roadmap` trong khi GitHub ra `-future-roadmap`
 * (emoji bị xoá, **khoảng trắng sau nó ở lại**) — và cổng báo 2 liên kết ĐÚNG
 * trong `README.md` là hỏng. Một cổng mới báo dương tính giả ngay ngày đầu là
 * cách nhanh nhất để nó bị vô hiệu hoá, nên thứ tự hai dòng này là phần đắt
 * nhất của cả khối.
 */
const slugify = (raw) => raw
  // `[text](url)` trong tiêu đề: GitHub slug theo TEXT hiển thị, không theo url.
  .replace(/\[([^\]]*)\]\([^)]*\)/g, '$1')
  .toLowerCase()
  .trim()
  // Giữ chữ (mọi bảng mã — tài liệu này là tiếng Việt), số, `_`, `-`, khoảng
  // trắng. Xoá phần còn lại: dấu câu, `**`, `~~`, backtick, emoji.
  .replace(/[^\p{L}\p{N}\s_-]/gu, '')
  .replace(/\s/g, '-')

/** Tập neo của một file .md; `null` nếu không đọc được. Có cache vì nhiều file cùng trỏ tới một đích. */
const anchorCache = new Map()
const anchorsOf = (abs) => {
  if (anchorCache.has(abs)) return anchorCache.get(abs)
  let set = null
  try {
    const text = fs.readFileSync(abs, 'utf8').replace(/\r\n/g, '\n')
    set = new Set()
    // Trùng slug thì GitHub thêm hậu tố `-1`, `-2`… theo thứ tự xuất hiện.
    // Thiếu bước này thì mọi liên kết tới tiêu đề trùng tên đều báo hỏng oan.
    const seen = new Map()
    // `keepInline = true`: GIỮ inline `code`. Tiêu đề như "U21 — Sổ đo mỗi lượt
    // — `turn_telemetry`" mà bị xoá inline code sẽ sinh slug thiếu hẳn phần đuôi.
    for (const m of stripCode(text, true).matchAll(/^#{1,6}[ \t]+(.+?)[ \t]*#*$/gm)) {
      const base = slugify(m[1])
      if (!base) continue
      const n = seen.get(base) ?? 0
      seen.set(base, n + 1)
      set.add(n === 0 ? base : `${base}-${n}`)
    }
    // Neo HTML đặt tay (`<a id="x">`) cũng là neo hợp lệ trên GitHub.
    for (const m of text.matchAll(/<a\s[^>]*\b(?:id|name)=["']([^"']+)["']/gi)) set.add(m[1])
  } catch { set = null }
  anchorCache.set(abs, set)
  return set
}

const LINK = /\[([^\]\n]{1,120})\]\(([^)\s]+)\)/g
let anchorsChecked = 0
for (const [p, d] of [...docs, ...rootDocs]) {
  // Snapshot `frozen` giữ liên kết đúng với cấu trúc tài liệu LÚC CHỤP — cùng
  // nguyên tắc đã áp cho citation, đường dẫn tuyệt đối và con trỏ 📌 ở dưới.
  const frozen = d.fm?.status === 'frozen'
  for (const m of stripCode(d.text).matchAll(LINK)) {
    const raw = m[2]
    if (/^(https?:|mailto:)/.test(raw)) continue
    const cut = raw.indexOf('#')
    const filePart = cut < 0 ? raw : raw.slice(0, cut)
    const frag = cut < 0 ? '' : decodeURIComponent(raw.slice(cut + 1))
    // Không có phần file ⇒ neo trỏ vào chính tài liệu đang đọc.
    const target = filePart
      ? path.resolve(path.dirname(path.join(REPO, p)), decodeURIComponent(filePart))
      : path.join(REPO, p)
    if (filePart && !fs.existsSync(target)) { err(p, `liên kết hỏng → \`${raw}\``); continue }
    if (!frag || frozen || !target.endsWith('.md')) continue
    const anchors = anchorsOf(target)
    if (!anchors) continue
    anchorsChecked++
    if (!anchors.has(frag))
      err(p, `neo không tồn tại → \`${raw}\` — tiêu đề đã đổi mà liên kết chưa sửa?`)
  }
}

// --------------------------------- 3b. đường dẫn TUYỆT ĐỐI trỏ vào repo, đã chết
//
// Lớp lỗi mà bộ kiểm liên kết KHÔNG bắt được: `E:\Project\LIVA\X` viết thẳng
// trong văn xuôi, không phải link markdown. Khi `X` bị chuyển chỗ, không gì báo.
// Đó đúng là hình dạng của con trỏ chết trong `AGENTS.md` (U6).
//
// Chỉ soi đường dẫn trỏ VÀO CHÍNH REPO. Đường dẫn tuyệt đối ra ngoài
// (`E:\AI_Models`, `C:\Program Files\...`) là tài liệu hoá môi trường máy người
// dùng — hợp lệ, và có ~59 chỗ như thế; bắt lỗi chúng chỉ tạo nhiễu.
//
// `(?<![A-Za-z])` chặn khớp nhầm `s://` bên trong `https://`.
const ABS_IN_REPO = /(?<![A-Za-z])[A-Za-z]:[\\/]Project[\\/]LIVA[\\/][^\s`)|,;"'*]*/g
for (const [p, d] of [...docs, ...rootDocs]) {
  // Tài liệu `frozen` là ảnh chụp lịch sử có chủ đích — toạ độ trong đó phản ánh
  // cây mã lúc khảo sát và KHÔNG được cập nhật theo hiện tại (cùng lý do
  // `docs-citations.mjs` bỏ qua chúng).
  if (d.fm?.status === 'frozen') continue
  // `keepInline = true`: chỉ bỏ khối ``` (ví dụ lệnh), GIỮ inline `code`.
  for (const m of stripCode(d.text, true).matchAll(ABS_IN_REPO)) {
    let raw = m[0].replace(/\\\\/g, '\\')
    let sub = raw.replace(/\\/g, '/').replace(/^[A-Za-z]:\/Project\/LIVA\/?/i, '').replace(/[.,:;]+$/, '')
    // Bỏ mục có dấu lược (`...`, `…`) — đó là mẫu minh hoạ, không phải đường dẫn thật.
    if (!sub || sub.includes('...') || sub.includes('…')) continue
    if (fs.existsSync(path.join(REPO, sub))) continue
    // File bị .gitignore (`.env`, `data/*.json` sinh lúc chạy) vắng mặt là bình
    // thường — tài liệu vẫn được phép mô tả chúng.
    if (isIgnored(sub)) continue
    err(p, `đường dẫn tuyệt đối trỏ vào repo nhưng KHÔNG tồn tại: \`${raw}\` — file đã chuyển chỗ hay bị xoá?`)
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
  // Snapshot frozen giữ con trỏ đúng với cấu trúc tài liệu tại thời điểm chụp.
  // Không ép chúng trỏ tới owner hiện hành — cùng nguyên tắc với citation,
  // absolute-path và stale checks ở trên.
  if (d.fm?.status === 'frozen') continue
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
say(`Neo #anchor    : ${anchorsChecked}`)
say(`Mã nguồn chưa tài liệu hoá : ${uncovered.length}`)

if (STRICT_STALE.length) say(`Lỗi thời = LỖI ở  : ${STRICT_STALE.join(', ')}`)

const printStale = (list) => {
  for (const s of list) {
    console.log(`  • ${s.doc}  (ghi commit ${s.since}${s.staleOk ? `, stale-ok ${s.staleOk}` : ''})`)
    for (const f of s.files.slice(0, 8)) console.log(`      ↳ ${f}`)
    if (s.files.length > 8) console.log(`      ↳ … và ${s.files.length - 8} file nữa`)
  }
}

const staleBlocking = staleReport.filter((s) => s.strict)
const staleWarnOnly = staleReport.filter((s) => !s.strict)

if (staleBlocking.length) {
  console.log('')
  console.log('❌ LỖI THỜI — CHẶN (thư mục nằm trong --strict-stale):')
  printStale(staleBlocking)
  console.log('')
  console.log('   Hai cách sửa, chọn theo việc bạn THỰC SỰ đã làm:')
  console.log(`     1. Có sửa nội dung  → cập nhật \`updated:\` + \`commit: ${headSha}\``)
  console.log(`     2. Đọc diff, không cần sửa gì → thêm/sửa \`stale-ok: ${headSha}\``)
  console.log('   Đừng dùng (1) khi bạn chỉ làm (2) — `commit:` là lời khẳng định về NỘI DUNG.')
  console.log('   Xem diff cần đọc:  git log <commit>..HEAD -- <đường dẫn trong covers>')
}

if (staleWarnOnly.length) {
  console.log('')
  console.log('⚠️  TÀI LIỆU CÓ THỂ ĐÃ LỖI THỜI (mã nguồn đổi sau commit ghi trong front-matter):')
  printStale(staleWarnOnly)
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
