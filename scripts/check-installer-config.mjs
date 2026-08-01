#!/usr/bin/env node
// Kiểm tra cấu hình bộ cài Windows TRƯỚC khi tốn 20 phút build.
//
// Vì sao cần: `tauri build` chỉ báo lỗi những gì nó không đọc nổi. Nó KHÔNG báo
// khi bạn đóng gói thiếu một resource mà lõi cần lúc chạy, khi `licenseFile`
// trỏ vào file không tồn tại, hay khi bộ cài lặng lẽ quay về mặc định "cần
// Internet lúc cài". Những thứ đó chỉ lộ ra trên máy người dùng — tức là chỗ
// đắt nhất để phát hiện.
//
// Có một mục ở đây không hiển nhiên và đáng đọc kỹ: `data/liva-config.json`
// KHÔNG được đóng gói cạnh exe. `data_dir()` của lõi neo vào thư mục chứa file
// đó, nên nếu nó nằm trong thư mục cài thì database ký ức sẽ được tạo trong
// thư mục cài — và **gỡ cài đặt sẽ xoá ký ức của người dùng**.
//
// Dùng:
//   node scripts/check-installer-config.mjs          # thoát 1 nếu có lỗi
//   node scripts/check-installer-config.mjs --json

import fs from 'node:fs'
import path from 'node:path'

const ROOT = path.resolve(import.meta.dirname, '..')
const CONF_REL = 'liva-desktop/src-tauri/tauri.conf.json'

/** Thư mục gốc để giải các đường dẫn trong tauri.conf.json (chính là src-tauri/). */
const THU_MUC_CONF = 'liva-desktop/src-tauri'

const doc = (p) => JSON.parse(fs.readFileSync(p, 'utf8'))

/**
 * Trả về `{ loi: [], canhBao: [] }`. Tách khỏi `process.exit` để test gọi được.
 * @param {string} root thư mục gốc repo
 */
export function kiemTra(root = ROOT) {
  const loi = []
  const canhBao = []
  const G = (p) => path.join(root, p)

  let conf
  try {
    conf = doc(G(CONF_REL))
  } catch (e) {
    return { loi: [`không đọc được ${CONF_REL}: ${e.message}`], canhBao }
  }

  const b = conf.bundle ?? {}
  const w = b.windows ?? {}
  const nsis = w.nsis ?? {}

  // ── 1. Mục tiêu đóng gói ────────────────────────────────────────────────
  const targets = b.targets
  if (!Array.isArray(targets) || targets.length !== 1 || targets[0] !== 'nsis') {
    loi.push(
      `bundle.targets phải đúng ["nsis"] — nhận ${JSON.stringify(targets)}. ` +
        '"all" phát thêm MSI, mà MSI cài per-machine vào Program Files (thư mục ' +
        'chỉ đọc) trong khi NSIS cài per-user: hai bộ cài, hai ngữ nghĩa, không ai chọn.',
    )
  }
  if (nsis.installMode !== 'currentUser') {
    loi.push(
      `nsis.installMode phải là "currentUser" — nhận ${JSON.stringify(nsis.installMode)}. ` +
        'Chưa ký số thì cài per-machine vừa đòi quyền admin vừa làm thư mục cài không ghi được.',
    )
  }

  // ── 2. WebView2 ─────────────────────────────────────────────────────────
  const wv = w.webviewInstallMode
  if (!wv || wv.type !== 'offlineInstaller') {
    loi.push(
      `windows.webviewInstallMode.type phải là "offlineInstaller" — nhận ${JSON.stringify(wv?.type)}. ` +
        'Mặc định của Tauri là downloadBootstrapper, tức bộ cài CẦN INTERNET — ' +
        'mâu thuẫn thẳng với định vị chạy offline của LIVA.',
    )
  }

  // ── 3. Ngôn ngữ ─────────────────────────────────────────────────────────
  const langs = nsis.languages ?? []
  for (const l of ['Vietnamese', 'English']) {
    if (!langs.includes(l)) loi.push(`nsis.languages thiếu "${l}" — nhận ${JSON.stringify(langs)}`)
  }

  // ── 4. Siêu dữ liệu + giấy phép ─────────────────────────────────────────
  for (const k of ['publisher', 'copyright', 'shortDescription', 'licenseFile']) {
    if (!b[k]) loi.push(`bundle.${k} còn trống — bộ cài sẽ hiện "Unknown publisher"`)
  }
  if (b.licenseFile) {
    const p = path.resolve(G(THU_MUC_CONF), b.licenseFile)
    if (!fs.existsSync(p)) loi.push(`bundle.licenseFile trỏ vào file không tồn tại: ${p}`)
  }

  // ── 5. Icon ─────────────────────────────────────────────────────────────
  for (const ic of b.icon ?? []) {
    const p = path.resolve(G(THU_MUC_CONF), ic)
    if (!fs.existsSync(p)) loi.push(`bundle.icon thiếu file: ${p}`)
  }
  if (!(b.icon ?? []).some((i) => i.endsWith('.ico'))) {
    loi.push('bundle.icon phải có một file .ico — NSIS cần nó cho installer và shortcut')
  }

  // ── 6. Resource: có mặt, và KHÔNG có thứ không được có ───────────────────
  const res = b.resources ?? {}
  const nguon = Object.keys(res)
  for (const src of nguon) {
    const p = path.resolve(G(THU_MUC_CONF), src)
    if (!fs.existsSync(p)) {
      loi.push(
        `bundle.resources trỏ vào nguồn không tồn tại: ${src} → ${p}` +
          (src.includes('node_modules') ? ' (chạy `npm ci` trước khi build)' : ''),
      )
    }
  }
  const dich = Object.values(res)
  if (!dich.some((d) => d.endsWith('vec0.dll'))) {
    loi.push('bundle.resources thiếu vec0.dll — thiếu nó là KHÔNG MỞ ĐƯỢC database, chặn khởi động')
  }
  if (!dich.some((d) => d.endsWith('models-manifest.json'))) {
    loi.push(
      'bundle.resources thiếu data/models-manifest.json — không có nó thì màn hình ' +
        'chuẩn bị model không biết phải tải gì, và người dùng không có Node để chạy scripts/',
    )
  }
  for (const [src, d] of Object.entries(res)) {
    if (path.basename(d) === 'liva-config.json' || path.basename(src) === 'liva-config.json') {
      loi.push(
        'bundle.resources KHÔNG được chứa liva-config.json: `data_dir()` neo vào thư mục ' +
          'chứa file đó, nên database ký ức sẽ nằm trong thư mục cài và GỠ CÀI ĐẶT SẼ XOÁ ' +
          'KÝ ỨC người dùng. Đóng gói bản mẫu dưới tên khác rồi chép lúc chạy lần đầu.',
      )
    }
  }

  // ── 7. Frontend ─────────────────────────────────────────────────────────
  if (conf.build?.frontendDist !== '../../liva-ui/dist') {
    loi.push(
      `build.frontendDist phải là "../../liva-ui/dist" (giải từ src-tauri/, nên HAI cấp) — ` +
        `nhận ${JSON.stringify(conf.build?.frontendDist)}`,
    )
  }
  const hook = conf.build?.beforeBuildCommand
  if (!hook) {
    canhBao.push('build.beforeBuildCommand trống — `npx tauri build` chạy tay sẽ đóng gói dist CŨ')
  } else if (hook.includes('||') || hook.includes('&&')) {
    // Tauri chạy hook với cwd = thư mục app (`liva-desktop`, đo được 28/07/2026)
    // và **fail build khi hook trả mã khác 0**. Nối chuỗi bằng `||` chỉ có nghĩa
    // là ta không biết cwd và đang đoán — đoán trong bước đóng gói thì lần sai
    // sẽ lộ ra ở bản người dùng tải về.
    loi.push(
      `build.beforeBuildCommand phải là MỘT lệnh xác định, không có \`||\`/\`&&\`: ${hook}`,
    )
  }

  // ── 8. Ba cửa sổ phải dùng ba capability tách biệt ──────────────────────
  if (!fs.existsSync(G('liva-ui/public/setup.html'))) {
    loi.push('thiếu liva-ui/public/setup.html — cửa sổ chuẩn bị model sẽ trắng trơn')
  }
  const capabilityNames = ['widget', 'dashboard', 'setup']
  const enabledCapabilities = conf.app?.security?.capabilities
  if (
    !Array.isArray(enabledCapabilities) ||
    enabledCapabilities.length !== capabilityNames.length ||
    capabilityNames.some((name) => !enabledCapabilities.includes(name))
  ) {
    loi.push(
      `app.security.capabilities phải bật đúng ${JSON.stringify(capabilityNames)} — ` +
        `nhận ${JSON.stringify(enabledCapabilities)}`,
    )
  }
  for (const name of capabilityNames) {
    const rel = `liva-desktop/src-tauri/capabilities/${name}.json`
    try {
      const cap = doc(G(rel))
      if (cap.identifier !== name || JSON.stringify(cap.windows) !== JSON.stringify([name])) {
        loi.push(`${rel} phải chỉ gán đúng cửa sổ "${name}"`)
      }
      if (name === 'setup') {
        for (const permission of [
          'allow-native-ipc-call',
          'allow-native-ipc-call-stream',
          'allow-open-dashboard',
          'core:window:allow-close',
        ]) {
          if (!(cap.permissions ?? []).includes(permission)) {
            loi.push(`${rel} thiếu ${permission}`)
          }
        }
      }
    } catch (e) {
      loi.push(`không đọc được ${rel}: ${e.message}`)
    }
  }

  // ── 9. CSP + tài nguyên setup tĩnh ──────────────────────────────────────
  const csp = conf.app?.security?.csp ?? ''
  if (csp.includes("'unsafe-inline'")) {
    loi.push("app.security.csp KHÔNG được chứa 'unsafe-inline'")
  }
  for (const directive of ["script-src 'self'", "style-src 'self'", "object-src 'none'"]) {
    if (!csp.includes(directive)) {
      loi.push(`app.security.csp thiếu chỉ thị bắt buộc: ${directive}`)
    }
  }
  const publicDir = G('liva-ui/public')
  for (const name of fs.readdirSync(publicDir).filter((entry) => entry.endsWith('.html'))) {
    const rel = `liva-ui/public/${name}`
    try {
      const html = fs.readFileSync(G(rel), 'utf8')
      if (
        /<style(?:\s|>)/i.test(html) ||
        /<script(?![^>]*\bsrc\s*=)[^>]*>/i.test(html) ||
        /\sstyle\s*=/i.test(html)
      ) {
        loi.push(`${rel} KHÔNG được chứa inline script hoặc style`)
      }
    } catch (e) {
      loi.push(`không đọc được ${rel}: ${e.message}`)
    }
  }
  const setupRel = 'liva-ui/public/setup.html'
  try {
    for (const asset of ['setup.css', 'setup.js']) {
      if (!fs.existsSync(G(`liva-ui/public/${asset}`))) {
        loi.push(`${setupRel} tham chiếu tài nguyên không tồn tại: ${asset}`)
      }
    }
  } catch (e) {
    loi.push(`không đọc được ${setupRel}: ${e.message}`)
  }

  // ── 10. Manifest model + cổng chuỗi cung ứng ────────────────────────────
  //
  // Fail closed: thiếu hoặc sai `sha256` là LỖI, không phải cảnh báo. Kích thước
  // không phải bằng chứng về nội dung — ngày 28/07/2026 có bốn file trên máy dev
  // đúng từng byte mà hash khác nguồn.
  try {
    const m = doc(G('data/models-manifest.json'))
    if (!m.files?.length) loi.push('data/models-manifest.json không có file nào')
    const laHex = (s) => typeof s === 'string' && /^[0-9a-fA-F]{64}$/.test(s)
    if (!laHex(m.runtimeArtifacts?.vec0?.sha256)) {
      loi.push('manifest: runtimeArtifacts.vec0.sha256 phải là SHA-256 64 hex')
    }
    for (const f of m.files ?? []) {
      if (!m.groups?.[f.group]) loi.push(`manifest: ${f.dest} thuộc nhóm "${f.group}" chưa khai báo`)
      if (!f.url && !f.manual) loi.push(`manifest: ${f.dest} không có url thì phải có "manual"`)
      if (!(f.bytes > 0)) loi.push(`manifest: ${f.dest} thiếu kích thước tham chiếu`)
      if (f.url && !laHex(f.sha256)) {
        loi.push(
          `manifest: ${f.dest} có url nhưng sha256 ${f.sha256 ? 'sai định dạng' : 'bị THIẾU'} — ` +
            'mọi file tải về phải có SHA-256 64 hex để đối chiếu trước khi ghi ra đường dẫn thật',
        )
      }
      // Nhánh di động đổi nội dung dưới chân ta. Hash sẽ chặn được, nhưng chặn
      // xong thì người dùng không tải được gì nữa — nên đó vẫn là hỏng.
      if (f.url && /\/(resolve|raw)\/(main|master)\//.test(f.url)) {
        loi.push(
          `manifest: ${f.dest} còn trỏ nhánh di động (main/master) — ghim sang revision bất biến`,
        )
      }
    }
  } catch (e) {
    loi.push(`không đọc được data/models-manifest.json: ${e.message}`)
  }

  return { loi, canhBao }
}

// Chỉ chạy phần CLI khi được gọi trực tiếp, để test `import` được.
const chayTrucTiep =
  process.argv[1] && path.resolve(process.argv[1]) === path.resolve(import.meta.filename)

if (chayTrucTiep) {
  const { loi, canhBao } = kiemTra()
  if (process.argv.includes('--json')) {
    console.log(JSON.stringify({ loi, canhBao }, null, 2))
  } else {
    console.log('\nLIVA — kiểm tra cấu hình bộ cài Windows\n')
    for (const c of canhBao) console.log(`  ⚠ ${c}`)
    for (const l of loi) console.log(`  ✗ ${l}`)
    console.log(
      loi.length
        ? `\n  ❌ ${loi.length} lỗi — sửa trước khi build, đừng chờ 20 phút để hỏng.`
        : `\n  ✅ Cấu hình bộ cài hợp lệ.${canhBao.length ? ` (${canhBao.length} cảnh báo)` : ''}`,
    )
  }
  process.exit(loi.length ? 1 : 0)
}
