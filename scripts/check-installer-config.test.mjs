// Test cho bộ kiểm cấu hình bộ cài.
//
// Một validator không có test là một validator mà không ai biết nó còn kiểm gì:
// nó luôn xanh, kể cả khi một nhánh kiểm bị viết hỏng và không bao giờ chạy.
// Mỗi test dưới đây làm hỏng ĐÚNG MỘT thứ trong một bản sao cấu hình rồi đòi
// bộ kiểm phải bắt được — tức là kiểm chính cái bẫy, không phải kiểm cú pháp.
//
// Chạy: npm run test:installer

import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import os from 'node:os'
import { kiemTra } from './check-installer-config.mjs'

const REPO = path.resolve(import.meta.dirname, '..')
const CONF = 'liva-desktop/src-tauri/tauri.conf.json'

/** Dựng một repo giả: chép đúng những file bộ kiểm đụng tới, rồi cho sửa. */
function repoGia(sua = () => {}) {
  const goc = fs.mkdtempSync(path.join(os.tmpdir(), 'liva-installer-test-'))
  const chep = (rel) => {
    const dich = path.join(goc, rel)
    fs.mkdirSync(path.dirname(dich), { recursive: true })
    fs.copyFileSync(path.join(REPO, rel), dich)
  }
  chep(CONF)
  for (const name of ['widget', 'dashboard', 'setup']) {
    chep(`liva-desktop/src-tauri/capabilities/${name}.json`)
  }
  chep('data/models-manifest.json')
  chep('LICENSE')
  chep('liva-ui/public/setup.html')
  chep('liva-ui/public/setup.css')
  chep('liva-ui/public/setup.js')
  for (const ic of ['32x32.png', '128x128.png', '128x128@2x.png', 'icon.icns', 'icon.ico']) {
    chep(`liva-desktop/src-tauri/icons/${ic}`)
  }
  // Nguồn resource: chỉ cần TỒN TẠI, nội dung không quan trọng với bộ kiểm.
  for (const rel of ['node_modules/sqlite-vec-windows-x64/vec0.dll']) {
    const p = path.join(goc, rel)
    fs.mkdirSync(path.dirname(p), { recursive: true })
    fs.writeFileSync(p, '')
  }

  const conf = JSON.parse(fs.readFileSync(path.join(goc, CONF), 'utf8'))
  sua(conf)
  fs.writeFileSync(path.join(goc, CONF), JSON.stringify(conf, null, 2))
  return goc
}

const chay = (sua) => kiemTra(repoGia(sua))
const cham = (loi, manh) => loi.some((l) => l.includes(manh))

test('cấu hình thật trong repo phải hợp lệ', () => {
  const { loi } = kiemTra(REPO)
  assert.deepEqual(loi, [], 'cấu hình bộ cài đang có lỗi')
})

test('bản phát hành Windows mang version v1.0.0', () => {
  const conf = JSON.parse(fs.readFileSync(path.join(REPO, CONF), 'utf8'))
  assert.equal(conf.version, '1.0.0')
})

test('dashboard khởi tạo ẩn để policy first-run chọn đúng cửa sổ', () => {
  const conf = JSON.parse(fs.readFileSync(path.join(REPO, CONF), 'utf8'))
  const dashboard = conf.app.windows.find((window) => window.label === 'dashboard')
  assert.ok(dashboard, 'thiếu cấu hình cửa sổ dashboard')
  assert.equal(dashboard.visible, false)
})

test('bắt được MSI lọt vào targets', () => {
  const { loi } = chay((c) => {
    c.bundle.targets = 'all'
  })
  assert.ok(cham(loi, 'bundle.targets'), loi.join('\n'))
})

test('bắt được WebView2 quay về bootstrapper cần mạng', () => {
  const { loi } = chay((c) => {
    c.bundle.windows.webviewInstallMode = { type: 'downloadBootstrapper', silent: true }
  })
  assert.ok(cham(loi, 'offlineInstaller'), loi.join('\n'))
})

test('bắt được cài per-machine', () => {
  const { loi } = chay((c) => {
    c.bundle.windows.nsis.installMode = 'perMachine'
  })
  assert.ok(cham(loi, 'currentUser'), loi.join('\n'))
})

test('bắt được thiếu tiếng Việt trong bộ cài', () => {
  const { loi } = chay((c) => {
    c.bundle.windows.nsis.languages = ['English']
  })
  assert.ok(cham(loi, 'Vietnamese'), loi.join('\n'))
})

test('bắt được licenseFile trỏ vào chỗ không có', () => {
  const { loi } = chay((c) => {
    c.bundle.licenseFile = '../../KHONG_CO_FILE_NAY'
  })
  assert.ok(cham(loi, 'licenseFile'), loi.join('\n'))
})

test('bắt được thiếu vec0.dll — thứ chặn khởi động', () => {
  const { loi } = chay((c) => {
    c.bundle.resources = { '../../data/models-manifest.json': 'data/models-manifest.json' }
  })
  assert.ok(cham(loi, 'vec0.dll'), loi.join('\n'))
})

test('bắt được thiếu manifest model — màn hình chuẩn bị sẽ rỗng', () => {
  const { loi } = chay((c) => {
    c.bundle.resources = { '../../node_modules/sqlite-vec-windows-x64/vec0.dll': 'vec0.dll' }
  })
  assert.ok(cham(loi, 'models-manifest.json'), loi.join('\n'))
})

// Đây là cái bẫy đắt nhất trong cả bộ: nó không làm build hỏng, không làm app
// hỏng, chỉ xoá ký ức người dùng vào đúng lúc họ gỡ cài đặt.
test('bắt được liva-config.json bị đóng gói cạnh exe', () => {
  const { loi } = chay((c) => {
    c.bundle.resources['../../data/liva-config.json'] = 'data/liva-config.json'
  })
  assert.ok(cham(loi, 'XOÁ'), loi.join('\n'))
})

test('bắt được frontendDist sai số cấp', () => {
  const { loi } = chay((c) => {
    c.build.frontendDist = '../liva-ui/dist'
  })
  assert.ok(cham(loi, 'frontendDist'), loi.join('\n'))
})

test('bắt được cửa sổ setup chưa được cấp quyền', () => {
  const goc = repoGia()
  const p = path.join(goc, 'liva-desktop/src-tauri/capabilities/setup.json')
  const cap = JSON.parse(fs.readFileSync(p, 'utf8'))
  cap.windows = cap.windows.filter((w) => w !== 'setup')
  fs.writeFileSync(p, JSON.stringify(cap, null, 2))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'setup'), loi.join('\n'))
})

test('bắt được CSP cho phép inline script hoặc style', () => {
  const { loi } = chay((c) => {
    c.app.security.csp =
      "default-src 'self'; script-src 'self' 'unsafe-inline'; style-src 'self' 'unsafe-inline';"
  })
  assert.ok(cham(loi, 'unsafe-inline'), loi.join('\n'))
})

test('bắt được setup page chứa inline script hoặc style', () => {
  const goc = repoGia()
  const p = path.join(goc, 'liva-ui/public/setup.html')
  fs.writeFileSync(
    p,
    '<!doctype html><html><head><style>body{display:block}</style></head>' +
      '<body><script>window.inline = true</script></body></html>',
  )
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'inline'), loi.join('\n'))
})

test('bắt được mọi public HTML khác chứa inline script hoặc style', () => {
  const goc = repoGia()
  const p = path.join(goc, 'liva-ui/public/diagnostic.html')
  fs.writeFileSync(p, '<!doctype html><html><body><script>window.inline = true</script></body></html>')
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'diagnostic.html'), loi.join('\n'))
})

test('bắt được vec0 thiếu hash trong runtime trust manifest', () => {
  const goc = repoGia()
  const p = path.join(goc, 'data/models-manifest.json')
  const m = JSON.parse(fs.readFileSync(p, 'utf8'))
  delete m.runtimeArtifacts.vec0.sha256
  fs.writeFileSync(p, JSON.stringify(m))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'runtimeArtifacts.vec0.sha256'), loi.join('\n'))
})

test('bắt được beforeBuildCommand nối chuỗi bằng ||', () => {
  const { loi } = chay((c) => {
    c.build.beforeBuildCommand = 'npm run build:ui --prefix ../.. || npm run build:ui --prefix ..'
  })
  assert.ok(cham(loi, 'MỘT lệnh xác định'), loi.join('\n'))
})

// Cổng chuỗi cung ứng: ba cách hỏng, cả ba phải đỏ.
test('bắt được entry có url mà thiếu sha256', () => {
  const goc = repoGia()
  const p = path.join(goc, 'data/models-manifest.json')
  const m = JSON.parse(fs.readFileSync(p, 'utf8'))
  delete m.files.find((f) => f.url).sha256
  fs.writeFileSync(p, JSON.stringify(m))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'THIẾU'), loi.join('\n'))
})

test('bắt được sha256 sai định dạng', () => {
  const goc = repoGia()
  const p = path.join(goc, 'data/models-manifest.json')
  const m = JSON.parse(fs.readFileSync(p, 'utf8'))
  m.files.find((f) => f.url).sha256 = 'deadbeef'
  fs.writeFileSync(p, JSON.stringify(m))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'sai định dạng'), loi.join('\n'))
})

test('bắt được URL còn trỏ nhánh main/master', () => {
  const goc = repoGia()
  const p = path.join(goc, 'data/models-manifest.json')
  const m = JSON.parse(fs.readFileSync(p, 'utf8'))
  const f = m.files.find((x) => x.url?.includes('huggingface.co'))
  f.url = f.url.replace(/\/resolve\/[0-9a-f]{40}\//, '/resolve/main/')
  fs.writeFileSync(p, JSON.stringify(m))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'nhánh di động'), loi.join('\n'))
})

test('bắt được manifest có file không tải được mà cũng không có hướng dẫn', () => {
  const goc = repoGia()
  const p = path.join(goc, 'data/models-manifest.json')
  const m = JSON.parse(fs.readFileSync(p, 'utf8'))
  m.files.push({ group: 'stt', profile: 'full', dest: 'models/bi-an.onnx', url: null, bytes: 5 })
  fs.writeFileSync(p, JSON.stringify(m, null, 2))
  const { loi } = kiemTra(goc)
  assert.ok(cham(loi, 'manual'), loi.join('\n'))
})

test('profile full phan phoi du Parakeet-vi tu URL bat bien', () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(REPO, 'data/models-manifest.json'), 'utf8'),
  )
  const parakeet = manifest.files.filter((file) => file.group === 'stt-vi-hq')

  assert.deepEqual(
    parakeet.map((file) => file.dest),
    [
      'models/parakeet_vi.onnx',
      'models/model.onnx_data',
      'models/parakeet_vi_vocab.json',
    ],
  )
  for (const file of parakeet) {
    assert.equal(file.profile, 'full')
    assert.match(file.url, /\/resolve\/[0-9a-f]{40}\//)
    assert.match(file.sha256, /^[0-9a-f]{64}$/)
    assert.ok(file.bytes > 0)
  }
  const vocab = parakeet.find((file) => file.dest === 'models/parakeet_vi_vocab.json')
  assert.match(vocab.url, /\/240d82cc243f7cf47d100b293c7dff96e65a04c2\/vocab\.txt$/)
  assert.equal(vocab.sha256, '444bd313fa42719dd976e66515ae33cea5a375f45da8a7d7158f7db704799a77')
})

test('bao cao WER ghi dung sha256 graph Parakeet trong manifest', () => {
  const manifest = JSON.parse(
    fs.readFileSync(path.join(REPO, 'data/models-manifest.json'), 'utf8'),
  )
  const report = JSON.parse(
    fs.readFileSync(path.join(REPO, 'docs/05-chat-luong/wer-fleurs-vi.json'), 'utf8'),
  )
  const graph = manifest.files.find((file) => file.dest === 'models/parakeet_vi.onnx')

  assert.equal(report.parakeet_model_sha256, graph.sha256)
})

test('external weights ONNX dung ten _data khong the bi commit nham', () => {
  const gitignore = fs.readFileSync(path.join(REPO, '.gitignore'), 'utf8')
  assert.match(gitignore, /^\*\.onnx_data$/mu)
})
