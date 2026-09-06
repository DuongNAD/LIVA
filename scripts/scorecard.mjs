#!/usr/bin/env node

// Phiếu chấm LIVA — đo trạng thái dự án rồi chấm điểm theo luật ghi sẵn.
//
// ## Vì sao script này tồn tại
//
// Một phiếu chấm gõ tay đúng đúng MỘT ngày. Hôm sau mã nguồn đi tiếp, con số ở
// lại, và phiếu thành thứ tệ hơn cả không có: một tài liệu tự tin mà sai. Đó
// đúng là mô thức đã làm `voice_stress` đỏ suốt một tháng (25/08→27/08/2026) —
// phép kiểm nằm ngoài cổng thì không ai canh.
//
// Nên điểm ở đây KHÔNG gõ tay. Mỗi tiêu chí khai một danh sách chỉ báo, mỗi chỉ
// báo tự đo và tự quy ra điểm theo ngưỡng viết ngay tại chỗ. Đổi mã nguồn thì
// điểm tự đổi.
//
// ## Hai loại chỉ báo, và vì sao phải phân biệt
//
//   'do'   — đo được bằng máy, luôn tươi.
//   'tay'  — phán đoán người (ví dụ "kiến trúc có sạch không"). KHÔNG giả vờ đo
//            được: nó mang `reviewedAt`, và phiếu hiện rõ ngày rà gần nhất để
//            người đọc tự trừ hao.
//
// Trộn hai loại mà không dán nhãn chính là cách một phiếu chấm nói dối êm ái
// nhất — nó khoác vẻ khách quan lên một con số do người gõ.
//
// ## Chỉ báo đắt
//
// `cargo test` và `cargo clippy` cần biên dịch (phút, không phải giây), nên mặc
// định KHÔNG chạy. Chúng đọc lại giá trị đã ghi trong JSON kỳ trước kèm mốc thời
// gian, và phiếu hiện tuổi của số đó. Chạy `--full` để đo lại thật.
//
// Dùng:
//   node scripts/scorecard.mjs              # đo nhanh, in bảng
//   node scripts/scorecard.mjs --full       # kèm cargo test + clippy (chậm)
//   node scripts/scorecard.mjs --json       # chỉ in JSON
//   node scripts/scorecard.mjs --html=<f>   # sinh trang HTML

import fs from 'node:fs'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

const ROOT = path.resolve(import.meta.dirname, '..')
const DATA_PATH = path.join(ROOT, 'docs', '_data', 'scorecard.json')

const argv = process.argv.slice(2)
const FULL = argv.includes('--full')
const JSON_ONLY = argv.includes('--json')
const HTML_OUT = argv.find((a) => a.startsWith('--html='))?.slice('--html='.length) ?? null

// ─── tiện ích đo ────────────────────────────────────────────────────────────

/** Chạy lệnh, trả stdout. Không ném — đo hỏng thì trả null để chỉ báo tự hạ. */
export function sh(cmd, args, opts = {}) {
  try {
    return execFileSync(cmd, args, {
      cwd: ROOT, encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'],
      maxBuffer: 64 * 1024 * 1024, ...opts,
    })
  } catch (e) {
    // Nhiều lệnh đo (grep không khớp, cargo đỏ) thoát khác 0 nhưng vẫn có
    // stdout dùng được — lấy nó thay vì coi như hỏng.
    return e?.stdout ?? null
  }
}

const readIfExists = (rel) => {
  try { return fs.readFileSync(path.join(ROOT, rel), 'utf8') } catch { return null }
}

const exists = (rel) => fs.existsSync(path.join(ROOT, rel))

/** Đếm dòng khớp regex trong mọi file dưới các thư mục cho trước. */
export function demTrongCay(dirs, exts, re) {
  let n = 0
  const di = (d) => {
    let ents
    try { ents = fs.readdirSync(d, { withFileTypes: true }) } catch { return }
    for (const e of ents) {
      const p = path.join(d, e.name)
      if (e.isDirectory()) { if (e.name !== 'node_modules' && e.name !== 'target') di(p) }
      else if (exts.some((x) => e.name.endsWith(x))) {
        const txt = fs.readFileSync(p, 'utf8')
        n += (txt.match(re) ?? []).length
      }
    }
  }
  for (const d of dirs) di(path.join(ROOT, d))
  return n
}

/** Đếm dòng mã (bỏ dòng trắng) trong cây. */
export function demDongMa(dirs, exts) {
  let n = 0
  const di = (d) => {
    let ents
    try { ents = fs.readdirSync(d, { withFileTypes: true }) } catch { return }
    for (const e of ents) {
      const p = path.join(d, e.name)
      if (e.isDirectory()) { if (e.name !== 'node_modules' && e.name !== 'target') di(p) }
      else if (exts.some((x) => e.name.endsWith(x))) {
        n += fs.readFileSync(p, 'utf8').split('\n').filter((l) => l.trim()).length
      }
    }
  }
  for (const d of dirs) di(path.join(ROOT, d))
  return n
}

/**
 * `unwrap()` NGOÀI khối `#[cfg(test)]`.
 *
 * Trong test, `unwrap()` panic CHÍNH LÀ cơ chế báo lỗi — đếm gộp chúng vào sẽ
 * thổi phồng con số và đẩy người đọc đi dọn đúng chỗ không nên dọn (điều mà
 * mục "không nên làm" của backlog cấm thẳng). Xấp xỉ bằng cách coi mọi thứ sau
 * `#[cfg(test)]` đầu tiên trong file là test.
 */
export function demUnwrapNgoaiTest(txt) {
  const cut = txt.indexOf('#[cfg(test)]')
  const phanSanXuat = cut === -1 ? txt : txt.slice(0, cut)
  return (phanSanXuat.match(/\.unwrap\(\)/g) ?? []).length
}

// ─── luật chấm ──────────────────────────────────────────────────────────────

/**
 * Quy một số đo về điểm theo bậc thang.
 *
 * `bac` xếp TỐT trước, mỗi bậc `[nguong, diem]`. `huong` = 'len' nghĩa là càng
 * lớn càng tốt (số test), 'xuong' là càng nhỏ càng tốt (số cảnh báo).
 * Không đo được (`null`) trả 0 — im lặng coi như đạt là cách một cổng luôn xanh
 * mà chẳng kiểm gì.
 */
export function chamTheoBac(giaTri, bac, huong = 'len') {
  if (giaTri === null || giaTri === undefined || Number.isNaN(giaTri)) return 0
  for (const [nguong, diem] of bac) {
    if (huong === 'len' ? giaTri >= nguong : giaTri <= nguong) return diem
  }
  return 0
}

export const TIEU_CHI = [
  {
    id: 'kien-truc', ten: 'Kiến trúc & thiết kế', tran: 15,
    chiBao: [
      { ten: 'Tách handle_command khỏi lib.rs', loai: 'do', tran: 5,
        do: (m) => chamTheoBac(m.libRsDong, [[1500, 5], [1900, 4], [2400, 2]], 'xuong'),
        hien: (m) => `lib.rs ${m.libRsDong ?? '?'} dòng` },
      { ten: 'Tách workspace theo miền', loai: 'tay', tran: 5, diem: 5, reviewedAt: '2026-08-27',
        hien: () => 'lõi Rust ↔ vỏ Tauri ↔ UI Vue, tách sạch' },
      { ten: 'Luận điểm offline-first giữ nhất quán', loai: 'tay', tran: 5, diem: 4, reviewedAt: '2026-08-27',
        hien: () => 'từ chối TTS cloud + DuckDB-WASM; còn code mồ côi lộ từng đợt' },
    ],
  },
  {
    id: 'chat-luong-ma', ten: 'Chất lượng mã', tran: 15,
    chiBao: [
      { ten: 'clippy -D warnings', loai: 'do', tran: 5, dat: true,
        do: (m) => chamTheoBac(m.clippyWarnings, [[0, 5], [5, 3], [20, 1]], 'xuong'),
        hien: (m) => m.clippyWarnings === null ? 'chưa đo (--full)' : `${m.clippyWarnings} cảnh báo` },
      { ten: 'cargo fmt sạch', loai: 'do', tran: 2,
        do: (m) => (m.fmtSach ? 2 : 0),
        hien: (m) => (m.fmtSach ? 'sạch' : 'còn lệch') },
      { ten: 'Không còn TODO/FIXME', loai: 'do', tran: 2,
        do: (m) => chamTheoBac(m.todos, [[0, 2], [10, 1]], 'xuong'),
        hien: (m) => `${m.todos} điểm` },
      { ten: 'unwrap() trên đường production', loai: 'do', tran: 3,
        do: (m) => chamTheoBac(m.unwrapSanXuat, [[40, 3], [100, 2], [200, 1]], 'xuong'),
        hien: (m) => `${m.unwrapSanXuat} điểm` },
      { ten: 'ESLint gate cho cả .ts và .vue', loai: 'do', tran: 3,
        do: (m) => (m.eslintCoVue ? 3 : 1),
        hien: (m) => (m.eslintCoVue ? 'có parser SFC' : 'thiếu parser SFC') },
    ],
  },
  {
    id: 'kiem-thu', ten: 'Kiểm thử', tran: 20,
    chiBao: [
      { ten: 'Số test chạy thật', loai: 'do', tran: 6,
        do: (m) => chamTheoBac(m.soTest, [[600, 6], [400, 5], [200, 3], [50, 1]], 'len'),
        hien: (m) => m.soTest === null ? 'chưa đo (--full)' : `${m.soTest} test xanh` },
      { ten: 'File test tích hợp', loai: 'do', tran: 3,
        do: (m) => chamTheoBac(m.fileTestTichHop, [[15, 3], [8, 2], [3, 1]], 'len'),
        hien: (m) => `${m.fileTestTichHop} file` },
      { ten: 'E2E qua socket thật', loai: 'do', tran: 3,
        do: (m) => (m.coE2E ? 3 : 0),
        hien: (m) => (m.coE2E ? 'e2e-gateway + e2e-memory' : 'không có') },
      { ten: 'Cổng coverage cho UI', loai: 'do', tran: 3,
        do: (m) => (m.coCongCoverage ? 3 : 0),
        hien: (m) => (m.coCongCoverage ? 'có, chặn CI' : 'không có') },
      { ten: 'Probe binary chạy được không cần model', loai: 'do', tran: 5,
        // Trần cứng của dự án: model weights nằm ngoài repo nên phần lớn binary
        // kiểm chứng không chạy nổi ở bất cứ CI nào.
        do: (m) => chamTheoBac(m.tyLeBinChayDuoc, [[0.8, 5], [0.6, 4], [0.4, 2], [0.2, 1]], 'len'),
        hien: (m) => `${m.binChayDuoc}/${m.binCoAssert} bin có assert chạy được thiếu model` },
    ],
  },
  {
    id: 'ci', ten: 'CI & cổng tự động', tran: 15,
    chiBao: [
      { ten: 'Số bước là cổng', loai: 'do', tran: 5,
        do: (m) => chamTheoBac(m.buocCI, [[30, 5], [20, 4], [10, 2], [4, 1]], 'len'),
        hien: (m) => `${m.buocCI} bước / ${m.jobCI} job` },
      { ten: 'Bộ cổng cốt lõi có đủ', loai: 'do', tran: 6,
        do: (m) => Math.round((m.congCoLoi.length / 6) * 6),
        hien: (m) => m.congCoLoi.join(' · ') || 'thiếu' },
      { ten: 'Hook chặn trước khi rời máy', loai: 'do', tran: 2,
        do: (m) => (m.coPrePush ? 2 : 0),
        hien: (m) => (m.coPrePush ? 'pre-push kiểm sổ tài liệu' : 'không có') },
      { ten: 'Build ứng dụng chạy ở CI nhánh chính', loai: 'do', tran: 2,
        do: (m) => (m.buildTrongCIChinh ? 2 : 0),
        hien: (m) => (m.buildTrongCIChinh ? 'có' : 'chỉ chạy khi gắn tag') },
    ],
  },
  {
    id: 'tai-lieu', ten: 'Tài liệu', tran: 15,
    chiBao: [
      { ten: 'Cổng máy kiểm tài liệu', loai: 'do', tran: 5,
        do: (m) => (m.coCongDocs ? 5 : 0),
        hien: (m) => (m.coCongDocs ? 'docs-check + docs-citations' : 'không có') },
      { ten: 'Neo trích dẫn không hỏng', loai: 'do', tran: 4,
        do: (m) => chamTheoBac(m.neoHong, [[0, 4], [3, 2], [10, 1]], 'xuong'),
        hien: (m) => m.neoHong === null ? 'không đo được' : `${m.neoHong} neo hỏng` },
      { ten: 'Tài liệu lỗi thời', loai: 'do', tran: 3,
        do: (m) => chamTheoBac(m.docLoiThoi, [[3, 3], [10, 2], [25, 1]], 'xuong'),
        hien: (m) => m.docLoiThoi === null ? 'không đo được' : `${m.docLoiThoi} tài liệu cần rà` },
      { ten: 'Tỷ lệ trích dẫn kiểm được', loai: 'do', tran: 3,
        do: (m) => chamTheoBac(m.tyLeKiemDuoc, [[0.7, 3], [0.5, 2], [0.3, 1]], 'len'),
        hien: (m) => m.tyLeKiemDuoc === null ? 'không đo được'
          : `${Math.round(m.tyLeKiemDuoc * 100)}% ngoài vùng đông lạnh` },
    ],
  },
  {
    id: 'van-hanh', ten: 'Vận hành thật', tran: 10,
    chiBao: [
      { ten: 'Khởi động một lệnh', loai: 'do', tran: 3,
        do: (m) => (m.coLenhDev ? 3 : 0),
        hien: (m) => (m.coLenhDev ? 'npm run dev' : 'không có') },
      { ten: 'Lệnh soi tài nguyên trước khi chạy', loai: 'do', tran: 2,
        do: (m) => (m.coPreflight ? 2 : 0),
        hien: (m) => (m.coPreflight ? 'preflight' : 'không có') },
      { ten: 'Đường tải model tự động', loai: 'do', tran: 3,
        do: (m) => (m.coSetupModels ? 3 : 0),
        hien: (m) => (m.coSetupModels ? 'có script' : 'lấy ngoài luồng, thủ công') },
      { ten: 'Hạ cấp mềm khi thiếu tài nguyên', loai: 'tay', tran: 2, diem: 2, reviewedAt: '2026-08-27',
        hien: () => 'gateway vẫn boot, RAG no-op kèm cảnh báo nêu đúng thư mục' },
    ],
  },
  {
    id: 'bao-mat', ten: 'Bảo mật & riêng tư', tran: 10,
    chiBao: [
      { ten: 'Cổng advisory cho crate', loai: 'do', tran: 3,
        do: (m) => (m.coCargoDeny ? 3 : 0),
        hien: (m) => (m.coCargoDeny ? 'cargo-deny: advisories + licenses + sources' : 'không có') },
      { ten: 'Test bề mặt tấn công', loai: 'do', tran: 3,
        do: (m) => chamTheoBac(m.fileTestBaoMat, [[3, 3], [2, 2], [1, 1]], 'len'),
        hien: (m) => `${m.fileTestBaoMat} file (sandbox, uỷ quyền, crypto boot)` },
      { ten: 'Không còn khoá mã hoá mặc định', loai: 'do', tran: 2,
        do: (m) => (m.coKhoaMacDinh ? 0 : 2),
        hien: (m) => (m.coKhoaMacDinh ? 'DEFAULT_ENCRYPTION_KEY vẫn xuất xưởng' : 'đã gỡ') },
      { ten: 'Không có phép kiểm nào gọi mạng thật', loai: 'tay', tran: 2, diem: 2, reviewedAt: '2026-08-27',
        hien: () => 'bản sao cuối bắn ra api.telegram.org đã xoá 27/08' },
    ],
  },
]

// ─── thu thập số đo ─────────────────────────────────────────────────────────

export function doDac({ full = false, cu = {} } = {}) {
  const m = {}
  const now = new Date().toISOString()

  m.dongRust = demDongMa(['liva-native-core/src', 'liva-desktop/src-tauri/src'], ['.rs'])
  m.dongWeb = demDongMa(['liva-ui/src'], ['.ts', '.vue'])
  m.todos = demTrongCay(['liva-native-core/src', 'liva-ui/src'], ['.rs', '.ts', '.vue'], /\bTODO\b|\bFIXME\b/g)

  const libRs = readIfExists('liva-native-core/src/lib.rs')
  m.libRsDong = libRs ? libRs.split('\n').length : null

  // unwrap ngoài #[cfg(test)]
  let unwraps = 0
  const diUnwrap = (d) => {
    let ents
    try { ents = fs.readdirSync(d, { withFileTypes: true }) } catch { return }
    for (const e of ents) {
      const p = path.join(d, e.name)
      if (e.isDirectory()) diUnwrap(p)
      else if (e.name.endsWith('.rs')) unwraps += demUnwrapNgoaiTest(fs.readFileSync(p, 'utf8'))
    }
  }
  diUnwrap(path.join(ROOT, 'liva-native-core/src'))
  m.unwrapSanXuat = unwraps

  // test tích hợp + e2e + coverage
  let testFiles = []
  try {
    testFiles = fs.readdirSync(path.join(ROOT, 'liva-native-core/tests')).filter((f) => f.endsWith('.rs'))
  } catch { /* thư mục vắng — để rỗng, chỉ báo tự về 0 */ }
  m.fileTestTichHop = testFiles.length
  m.coE2E = exists('scripts/e2e-gateway.mjs') && exists('scripts/e2e-memory.mjs')

  // `.vue` không hề được lint tới 22/07/2026 vì config thiếu parser SFC — chỉ
  // báo này canh đúng chỗ đó, nên phải soi file config thật chứ không suy đoán.
  const eslintCfg = readIfExists('eslint.config.js') ?? ''
  m.eslintCoVue = /vue-eslint-parser/.test(eslintCfg)

  const vitest = readIfExists('liva-ui/vitest.config.ts') ?? ''
  m.coCongCoverage = /thresholds|lines\s*:/.test(vitest)

  // probe binary: bao nhiêu cái CÓ assert, và bao nhiêu chạy được khi thiếu model
  let bins = []
  try {
    bins = fs.readdirSync(path.join(ROOT, 'liva-native-core/src/bin')).filter((f) => f.endsWith('.rs'))
  } catch { /* không có thư mục bin */ }
  const CAN_MODEL = /\.onnx|\.gguf|MODEL_PATH|MODEL_DIR|model_dir/
  let coAssert = 0, chayDuoc = 0
  for (const b of bins) {
    const txt = fs.readFileSync(path.join(ROOT, 'liva-native-core/src/bin', b), 'utf8')
    if (!/assert/.test(txt)) continue          // benchmark thuần: không có gì để hỏng
    coAssert++
    if (!CAN_MODEL.test(txt)) chayDuoc++
  }
  m.binCoAssert = coAssert
  m.binChayDuoc = chayDuoc
  m.tyLeBinChayDuoc = coAssert ? chayDuoc / coAssert : 0

  // CI
  const wf = readIfExists('.github/workflows/test.yml') ?? ''
  m.buocCI = (wf.match(/- name:/g) ?? []).length
  // Chỉ đếm khoá nằm DƯỚI `jobs:`. Bản đầu đếm cả `push:`/`pull_request:` (nằm
  // dưới `on:`, cùng mức thụt 2 space) nên báo 4 job trong khi thật có 2.
  m.jobCI = (wf.slice(wf.search(/^jobs:$/m)).match(/^ {2}[a-z][\w-]*:$/gm) ?? []).length
  const CONG = [
    ['fmt', /cargo fmt/], ['clippy', /clippy/], ['cargo-deny', /cargo deny/],
    ['npm audit', /npm audit/], ['vue-tsc', /vue-tsc/], ['e2e', /e2e-gateway-ci/],
  ]
  m.congCoLoi = CONG.filter(([, re]) => re.test(wf)).map(([n]) => n)
  m.buildTrongCIChinh = /tauri build/.test(wf)
  m.coPrePush = exists('.husky/pre-push')
  m.coCargoDeny = /cargo deny/.test(wf)

  // tài liệu
  m.coCongDocs = exists('scripts/docs-check.mjs') && exists('scripts/docs-citations.mjs')
  const cite = sh('node', ['scripts/docs-citations.mjs', '--max-unchecked=207'])
  if (cite) {
    m.neoHong = /Không có neo hỏng/.test(cite)
      ? 0
      : Number(cite.match(/❌\s*(\d+)\s*neo hỏng/)?.[1] ?? 0)
    const quet = Number(cite.match(/([\d.]+) trích dẫn \(/)?.[1]?.replace('.', '') ?? 0)
    const dongLanh = Number(cite.match(/([\d.]+) trích dẫn — /)?.[1]?.replace('.', '') ?? 0)
    m.tyLeKiemDuoc = quet + dongLanh ? quet / (quet + dongLanh) : null
  } else { m.neoHong = null; m.tyLeKiemDuoc = null }

  const chk = sh('node', ['scripts/docs-check.mjs', '--strict-stale=docs/03-danh-gia'])
  m.docLoiThoi = chk ? (chk.match(/^ {2}• /gm) ?? []).length : null

  // vận hành
  const pkg = JSON.parse(readIfExists('package.json') ?? '{}')
  const scripts = pkg.scripts ?? {}
  m.coLenhDev = Boolean(scripts.dev)
  m.coSetupModels = Object.keys(scripts).some((k) => /setup:models|models:fetch|models:download/.test(k))
  // Dò cả cây `src/`: preflight sống ở `boot.rs`, không phải `commands/mod.rs`.
  // Bản đầu chỉ soi một file nên báo "không có" cho một lệnh đang tồn tại.
  m.coPreflight = demTrongCay(['liva-native-core/src'], ['.rs'], /preflight/gi) > 0
    || Object.keys(scripts).some((k) => /preflight/.test(k))

  // bảo mật
  m.fileTestBaoMat = testFiles.filter((f) =>
    /sandbox|authorization|crypto|trust|escape/.test(f)).length
  m.coKhoaMacDinh = /DEFAULT_ENCRYPTION_KEY/.test(readIfExists('liva-native-core/src/crypto.rs') ?? '')

  // ── chỉ báo đắt ──
  if (full) {
    const out = sh('cargo', ['test', '--workspace'])
    m.soTest = out
      ? (out.match(/^test result: ok\. (\d+) passed/gm) ?? [])
          .reduce((s, l) => s + Number(l.match(/(\d+) passed/)[1]), 0)
      : null
    const cl = sh('cargo', ['clippy', '--workspace', '--all-targets', '--message-format=short', '--', '-D', 'warnings'])
    m.clippyWarnings = cl === null ? null : (cl.match(/: warning:/g) ?? []).length
    m.fmtSach = sh('cargo', ['fmt', '--all', '--', '--check']) === ''
    m.doDacDatLuc = now
  } else {
    m.soTest = cu.soTest ?? null
    m.clippyWarnings = cu.clippyWarnings ?? null
    m.fmtSach = cu.fmtSach ?? false
    m.doDacDatLuc = cu.doDacDatLuc ?? null
  }

  m.sha = sh('git', ['rev-parse', '--short', 'HEAD'])?.trim() ?? null
  m.nhanh = sh('git', ['rev-parse', '--abbrev-ref', 'HEAD'])?.trim() ?? null
  m.doLuc = now
  return m
}

// ─── chấm ───────────────────────────────────────────────────────────────────

export function cham(m) {
  const ket = TIEU_CHI.map((tc) => {
    const chiBao = tc.chiBao.map((cb) => ({
      ten: cb.ten, loai: cb.loai, tran: cb.tran, reviewedAt: cb.reviewedAt ?? null,
      diem: cb.loai === 'tay' ? cb.diem : cb.do(m),
      hien: cb.hien(m),
    }))
    const diem = chiBao.reduce((s, c) => s + c.diem, 0)
    return { ...tc, chiBao, diem, tyLe: diem / tc.tran }
  })
  return {
    tieuChi: ket,
    tong: ket.reduce((s, t) => s + t.diem, 0),
    tran: ket.reduce((s, t) => s + t.tran, 0),
    tyLeTuDo: (() => {
      const all = ket.flatMap((t) => t.chiBao)
      return all.filter((c) => c.loai === 'do').reduce((s, c) => s + c.tran, 0)
        / all.reduce((s, c) => s + c.tran, 0)
    })(),
  }
}

// ─── chạy ───────────────────────────────────────────────────────────────────

if (process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename)) {
  let cu = {}
  try { cu = JSON.parse(fs.readFileSync(DATA_PATH, 'utf8')).soDo ?? {} } catch { /* lần đầu */ }

  const soDo = doDac({ full: FULL, cu })
  const diem = cham(soDo)
  const ban = { soDo, diem, sinhLuc: new Date().toISOString() }

  fs.mkdirSync(path.dirname(DATA_PATH), { recursive: true })
  fs.writeFileSync(DATA_PATH, `${JSON.stringify(ban, null, 2)}\n`)

  if (JSON_ONLY) {
    console.log(JSON.stringify(ban, null, 2))
  } else {
    console.log(`\n  PHIẾU CHẤM LIVA — ${soDo.sha ?? '?'} (${soDo.nhanh ?? '?'})\n`)
    for (const tc of diem.tieuChi) {
      const rong = 24
      const day = Math.round(tc.tyLe * rong)
      console.log(
        `  ${String(tc.diem).padStart(2)}/${String(tc.tran).padEnd(3)} ` +
        `${'█'.repeat(day)}${'·'.repeat(rong - day)}  ${tc.ten}`,
      )
      for (const cb of tc.chiBao) {
        const nhan = cb.loai === 'tay' ? `tay ${cb.reviewedAt}` : 'đo'
        console.log(`          ${String(cb.diem)}/${cb.tran}  ${cb.ten} — ${cb.hien}  [${nhan}]`)
      }
    }
    console.log(`\n  TỔNG: ${diem.tong}/${diem.tran}   ` +
      `(${Math.round(diem.tyLeTuDo * 100)}% trọng số là chỉ báo đo được)`)
    if (!FULL) console.log('  ⚠  cargo test/clippy đọc từ kỳ trước — chạy --full để đo lại.')
    console.log(`\n  Đã ghi: ${path.relative(ROOT, DATA_PATH)}\n`)
  }

  if (HTML_OUT) {
    const { sinhHtml } = await import('./scorecard-html.mjs')
    fs.writeFileSync(HTML_OUT, sinhHtml(ban))
    console.log(`  Đã sinh HTML: ${HTML_OUT}\n`)
  }
}
