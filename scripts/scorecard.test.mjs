import test from 'node:test'
import assert from 'node:assert/strict'

import { chamTheoBac, demUnwrapNgoaiTest, cham, TIEU_CHI } from './scorecard.mjs'
import { sinhHtml } from './scorecard-html.mjs'

// ─── chamTheoBac ────────────────────────────────────────────────────────────

test('chamTheoBac: hướng "len" — càng lớn càng nhiều điểm', () => {
  const bac = [[600, 6], [400, 5], [200, 3], [50, 1]]
  assert.equal(chamTheoBac(700, bac, 'len'), 6)
  assert.equal(chamTheoBac(600, bac, 'len'), 6, 'đúng ngưỡng phải ăn bậc đó')
  assert.equal(chamTheoBac(599, bac, 'len'), 5)
  assert.equal(chamTheoBac(49, bac, 'len'), 0, 'dưới bậc thấp nhất là 0')
})

test('chamTheoBac: hướng "xuong" — càng nhỏ càng nhiều điểm', () => {
  const bac = [[0, 5], [5, 3], [20, 1]]
  assert.equal(chamTheoBac(0, bac, 'xuong'), 5)
  assert.equal(chamTheoBac(3, bac, 'xuong'), 3)
  assert.equal(chamTheoBac(21, bac, 'xuong'), 0)
})

// Đây là bất biến quan trọng nhất của file: một chỉ báo KHÔNG đo được phải kéo
// điểm xuống, không được im lặng coi như đạt. Cổng luôn xanh là cổng vô dụng —
// bài học `voice_stress` đỏ cả tháng mà CI vẫn xanh.
test('chamTheoBac: không đo được thì 0 điểm, không phải điểm tối đa', () => {
  const bac = [[0, 5], [5, 3]]
  assert.equal(chamTheoBac(null, bac, 'xuong'), 0)
  assert.equal(chamTheoBac(undefined, bac, 'xuong'), 0)
  assert.equal(chamTheoBac(NaN, bac, 'xuong'), 0)
})

// ─── demUnwrapNgoaiTest ─────────────────────────────────────────────────────

test('demUnwrapNgoaiTest: bỏ qua unwrap trong #[cfg(test)]', () => {
  const src = `
fn that(x: Option<u8>) -> u8 { x.unwrap() }
#[cfg(test)]
mod tests {
  #[test] fn a() { foo().unwrap(); bar().unwrap(); }
}`
  // Trong test, unwrap() panic CHÍNH LÀ cách báo lỗi — đếm gộp sẽ đẩy người
  // đọc đi dọn đúng chỗ mà backlog cấm dọn.
  assert.equal(demUnwrapNgoaiTest(src), 1)
})

test('demUnwrapNgoaiTest: file không có khối test thì đếm tất', () => {
  assert.equal(demUnwrapNgoaiTest('a.unwrap(); b.unwrap();'), 2)
})

test('demUnwrapNgoaiTest: file sạch trả 0', () => {
  assert.equal(demUnwrapNgoaiTest('fn a() -> u8 { 1 }'), 0)
})

// ─── cham ───────────────────────────────────────────────────────────────────

/** Bản đo giả, đủ trường để mọi chỉ báo chạy được. */
const soDoGia = {
  libRsDong: 1788, clippyWarnings: 0, fmtSach: true, todos: 0, unwrapSanXuat: 95,
  eslintCoVue: true, soTest: 689, fileTestTichHop: 20, coE2E: true,
  coCongCoverage: true, binCoAssert: 5, binChayDuoc: 2, tyLeBinChayDuoc: 0.4,
  buocCI: 38, jobCI: 3, congCoLoi: ['fmt', 'clippy', 'cargo-deny', 'npm audit', 'vue-tsc', 'e2e'],
  buildTrongCIChinh: false, coPrePush: true, coCongDocs: true, neoHong: 0,
  docLoiThoi: 8, tyLeKiemDuoc: 0.39, coLenhDev: true, coSetupModels: false,
  coPreflight: true, coCargoDeny: true, fileTestBaoMat: 4, coKhoaMacDinh: true,
  dongRust: 52222, dongWeb: 20469, sha: 'abc1234', nhanh: 'mac-v2',
  doDacDatLuc: '2026-08-27T00:00:00.000Z', doLuc: '2026-08-27T00:00:00.000Z',
}

test('cham: tổng bằng đúng tổng các tiêu chí, và không vượt trần', () => {
  const d = cham(soDoGia)
  assert.equal(d.tong, d.tieuChi.reduce((s, t) => s + t.diem, 0))
  assert.equal(d.tran, 100, 'bảy tiêu chí phải cộng lại đúng 100')
  assert.ok(d.tong <= d.tran)
})

test('cham: mỗi tiêu chí không vượt trần của chính nó', () => {
  for (const tc of cham(soDoGia).tieuChi) {
    assert.ok(tc.diem <= tc.tran, `${tc.ten}: ${tc.diem} > ${tc.tran}`)
  }
})

test('TIEU_CHI: trần mỗi tiêu chí bằng tổng trần các chỉ báo con', () => {
  for (const tc of TIEU_CHI) {
    const tong = tc.chiBao.reduce((s, c) => s + c.tran, 0)
    assert.equal(tong, tc.tran, `${tc.ten}: chỉ báo cộng ${tong}, khai trần ${tc.tran}`)
  }
})

test('TIEU_CHI: chỉ báo chấm tay BẮT BUỘC có reviewedAt', () => {
  // Không có ngày rà thì người đọc không biết con số cũ tới đâu — đúng cái bẫy
  // mà phiếu chấm này sinh ra để tránh.
  for (const tc of TIEU_CHI) {
    for (const cb of tc.chiBao.filter((c) => c.loai === 'tay')) {
      assert.match(cb.reviewedAt ?? '', /^\d{4}-\d{2}-\d{2}$/, `${tc.ten} / ${cb.ten}`)
      assert.ok(cb.diem <= cb.tran, `${cb.ten}: điểm tay vượt trần`)
    }
  }
})

test('cham: mã nguồn tệ đi thì điểm phải tụt', () => {
  const tot = cham(soDoGia).tong
  const te = cham({ ...soDoGia, clippyWarnings: 40, todos: 60, unwrapSanXuat: 500, soTest: 10 }).tong
  assert.ok(te < tot, `phải tụt điểm: tốt ${tot}, tệ ${te}`)
})

test('cham: sửa được khuyết điểm thì điểm phải tăng', () => {
  const gio = cham(soDoGia).tong
  const sau = cham({ ...soDoGia, coSetupModels: true, buildTrongCIChinh: true }).tong
  assert.ok(sau > gio, `phải tăng điểm: giờ ${gio}, sau ${sau}`)
})

// ─── sinhHtml ───────────────────────────────────────────────────────────────

test('sinhHtml: nhúng đúng tổng điểm và sha đang đo', () => {
  const ban = { soDo: soDoGia, diem: cham(soDoGia), sinhLuc: '2026-08-27T00:00:00.000Z' }
  const html = sinhHtml(ban)
  assert.match(html, new RegExp(`>${ban.diem.tong}<`), 'tổng điểm phải có trong trang')
  assert.match(html, /abc1234/, 'sha phải có trong trang')
  assert.match(html, /<title>Phiếu chấm LIVA<\/title>/)
})

test('sinhHtml: định nghĩa màu cho cả ba trạng thái theme', () => {
  const html = sinhHtml({ soDo: soDoGia, diem: cham(soDoGia), sinhLuc: '2026-08-27T00:00:00.000Z' })
  assert.match(html, /:root\{/, 'phải có palette sáng trên :root trần')
  assert.match(html, /prefers-color-scheme:dark/, 'phải có nhánh theo hệ điều hành')
  assert.match(html, /:root\[data-theme="dark"\]/, 'phải có nhánh người dùng chọn tay')
})

test('sinhHtml: thoát HTML trong dữ liệu, không để chèn thẻ', () => {
  const doc = { ...soDoGia, nhanh: '<script>alert(1)</script>' }
  const html = sinhHtml({ soDo: doc, diem: cham(doc), sinhLuc: '2026-08-27T00:00:00.000Z' })
  assert.ok(!html.includes('<script>alert(1)</script>'), 'phải bị thoát')
  assert.match(html, /&lt;script&gt;/)
})

// ─── bẫy cấu trúc ───────────────────────────────────────────────────────────
//
// Lỗi thật đã mắc khi viết file này: `eslintCoVue` được TIEU_CHI đọc nhưng
// `doDac` không bao giờ đặt. Nó không nổ — chỉ lặng lẽ thành `undefined`, rồi
// rơi vào nhánh sai và cho điểm sai. Đúng cái kiểu hỏng mà cả phiếu chấm này
// sinh ra để chống: sai mà vẫn xanh.

test('mọi trường TIEU_CHI đọc đều phải được doDac sinh ra', async () => {
  const src = await import('node:fs').then((fs) =>
    fs.readFileSync(new URL('./scorecard.mjs', import.meta.url), 'utf8'))

  const thanTIEU_CHI = src.slice(src.indexOf('export const TIEU_CHI'), src.indexOf('// ─── thu thập số đo'))
  const thanDoDac = src.slice(src.indexOf('export function doDac'))

  const doc = [...new Set([...thanTIEU_CHI.matchAll(/\bm\.([A-Za-z][\w]*)/g)].map((x) => x[1]))]
  const dat = new Set([...thanDoDac.matchAll(/\bm\.([A-Za-z][\w]*)\s*=/g)].map((x) => x[1]))

  const thieu = doc.filter((k) => !dat.has(k))
  assert.deepEqual(thieu, [], `chỉ báo đọc trường mà doDac không đặt: ${thieu.join(', ')}`)
})
