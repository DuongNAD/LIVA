// Test cho bộ dò trích dẫn tài liệu — riêng nhánh "ngoài repo / gitlink".
//
// Vì sao nhánh này đáng có test trong khi phần còn lại của script thì chưa:
// nó là nhánh **nới lỏng** duy nhất được thêm vào một bộ kiểm mà cả file
// `docs-citations.mjs` sinh ra để diệt kiểu lỗi "luôn xanh". Một nhánh nới
// lỏng viết rộng quá tay sẽ nuốt luôn nhóm `file-khong-ton-tai` — tức mọi lỗi
// gõ nhầm đường dẫn hoá im lặng — và **không ai phát hiện được**, vì triệu
// chứng của nó chính là "cổng vẫn xanh".
//
// Nên hai test dưới đây đi thành một CẶP, và cặp đó mới là phép kiểm:
//   1. đường dẫn dưới gitlink  → PHẢI được bỏ qua  (nếu không, CI đỏ vĩnh viễn)
//   2. đường dẫn gõ nhầm       → PHẢI vẫn báo hỏng (nếu không, cổng thành đồ trang trí)
// Bỏ test (2) thì test (1) một mình sẽ xanh cả khi ai đó nới thành "bỏ qua mọi
// đường dẫn không tồn tại".
//
// Bối cảnh lịch sử: ngày 29/07/2026 `docs-citations` XANH trên máy dev và ĐỎ
// trên CI cho cùng một commit. Máy dev có `models/nemotron-asr/` (repo git lồng
// + LFS, tải out-of-band); git ghi thư mục đó là gitlink (mode 160000) nhưng
// repo không có `.gitmodules`, nên bản checkout của CI chỉ có thư mục RỖNG.
//
// Chạy: node --test scripts/docs-citations.test.mjs
//
// ⚠️ Lưu ý: tính tới 29/07/2026 CI **không** chạy bất kỳ file `scripts/*.test.mjs`
// nào (xem `.github/workflows/test.yml`) — kể cả hai file test đã có từ trước.
// Test này vì thế là lưới an toàn cho người chạy tay, chưa phải một cổng.

import { test } from 'node:test'
import assert from 'node:assert/strict'
import fs from 'node:fs'
import path from 'node:path'
import { execFileSync } from 'node:child_process'

const REPO = path.resolve(import.meta.dirname, '..')

/** Thư mục gitlink đầu tiên trong index, hoặc `null` nếu repo không có cái nào. */
function gitlinkDauTien() {
  const ra = execFileSync('git', ['ls-files', '-s'], { cwd: REPO, encoding: 'utf8' })
  const dong = ra.split('\n').find((l) => l.startsWith('160000 '))
  return dong ? dong.split('\t')[1] : null
}

/**
 * Viết một tài liệu tạm dưới `docs/`, chạy bộ dò, rồi xoá.
 *
 * Phải đặt trong `docs/` thật vì bộ dò quét cứng thư mục đó. Xoá trong
 * `finally` để một assert đỏ không bỏ lại rác làm hỏng lần chạy sau — đúng
 * kiểu nhiễu khiến người ta nghi nhầm mã nguồn.
 */
function doVoiTaiLieu(noiDung) {
  const rel = `docs/_test-citations-${process.pid}.md`
  const abs = path.join(REPO, rel)
  fs.writeFileSync(
    abs,
    `---\ntitle: "test tạm"\nupdated: 2026-07-29\nstatus: living\nowns: []\ncovers: []\n---\n# test tạm\n\n${noiDung}\n`,
  )
  try {
    const ra = execFileSync('node', ['scripts/docs-citations.mjs', '--json'], {
      cwd: REPO,
      encoding: 'utf8',
    })
    const bao = JSON.parse(ra)
    return (bao.findings ?? []).filter((f) => f.doc === rel)
  } catch (e) {
    // Thoát 1 khi có neo hỏng — vẫn có JSON trên stdout, đó mới là thứ cần đọc.
    const bao = JSON.parse(e.stdout)
    return (bao.findings ?? []).filter((f) => f.doc === rel)
  } finally {
    fs.rmSync(abs, { force: true })
  }
}

test('toạ độ dưới cây gitlink được bỏ qua, không báo hỏng', () => {
  const gitlink = gitlinkDauTien()
  assert.ok(
    gitlink,
    'repo không còn gitlink nào — test này mất đối tượng, hãy xoá nó thay vì để xanh rỗng',
  )

  // Cố ý trỏ vào một file KHÔNG tồn tại kể cả trên máy dev: nếu test dùng file
  // có thật thì nó sẽ xanh nhờ nhánh hỏi-đĩa cũ, và không kiểm được nhánh mới.
  const ket = doVoiTaiLieu(`Trỏ vào gitlink: \`${gitlink}/khong-ton-tai-o-dau-ca.rs:1\``)

  assert.deepEqual(
    ket,
    [],
    'nội dung dưới gitlink không thể có ở bất kỳ checkout nào ⇒ phải xử như "ngoài repo"',
  )
})

test('đường dẫn gõ nhầm NGOÀI gitlink vẫn phải báo hỏng', () => {
  // `normalize.rs` — thiếu chữ "r" so với `normalizer.rs` có thật. Đây là ca
  // giữ cho nhánh nới lỏng ở trên không bị viết rộng thành "bỏ qua mọi thứ
  // không tồn tại".
  const ket = doVoiTaiLieu('Gõ nhầm: `liva-native-core/src/tts/normalize.rs:1`')

  assert.equal(ket.length, 1, 'đường dẫn không tồn tại ngoài gitlink phải bị bắt')
  assert.equal(ket[0].loai, 'file-khong-ton-tai')
})

test('citation checker chỉ lập chỉ mục file được git theo dõi', () => {
  const rel = `tools/wakeword/work/_test-citations-untracked-${process.pid}.rs`
  const abs = path.join(REPO, rel)
  fs.mkdirSync(path.dirname(abs), { recursive: true })
  fs.writeFileSync(abs, 'pub fn untracked_only() {}\n')

  try {
    const ket = doVoiTaiLieu(`File sinh cục bộ không được tính là nguồn: \`${rel}:1\``)
    assert.equal(ket.length, 1, 'file không track phải vắng khỏi chỉ mục citation')
    assert.equal(ket[0].loai, 'file-khong-ton-tai')
  } finally {
    fs.rmSync(abs, { force: true })
  }
})

test('citation checker bỏ qua đúng snapshot FREEZE từ document inventory', () => {
  const raw = execFileSync('node', ['scripts/docs-citations.mjs', '--json'], {
    cwd: REPO,
    encoding: 'utf8',
  })
  const report = JSON.parse(raw)

  assert.ok(
    report.frozenDocsSkipped.includes('docs/03-danh-gia/00-bao-cao-khao-sat-goc-2026-07.md'),
    'snapshot khảo sát gốc phải được lấy từ disposition FREEZE',
  )
  assert.ok(
    report.frozenDocsSkipped.includes('docs/03-danh-gia/04-de-xuat-tich-hop-openspace.md'),
    'đề xuất OpenSpace đã FREEZE không được so citation với HEAD',
  )
  assert.ok(
    report.frozenDocsSkipped.includes('docs/01-ban-ve/03-duong-ong-thoai.md'),
    'khảo sát voice cũ đã hoàn tất di trú phải được bỏ qua như snapshot FREEZE',
  )
  assert.ok(
    !report.frozenDocsSkipped.includes('docs/03-he-thong-con/voice.md'),
    'tài liệu voice canonical KEEP phải tiếp tục được quét',
  )
  assert.ok(
    report.frozenDocsSkipped.includes('docs/01-ban-ve/05-agent-bo-nho-va-tien-hoa.md'),
    'khảo sát agent/memory/evolution cũ phải được bỏ qua như snapshot FREEZE',
  )
  assert.ok(
    !report.frozenDocsSkipped.includes('docs/03-he-thong-con/agent-tools.md'),
    'tài liệu agent/tools canonical KEEP phải tiếp tục được quét',
  )
})
