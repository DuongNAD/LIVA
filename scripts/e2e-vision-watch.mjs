#!/usr/bin/env node
// Kiểm chứng chuỗi "canh chừng vùng màn hình" qua WebSocket thật:
// capture (lấy kích thước + mồi baseline) → add_region → get_changed_regions.
//
// Vì sao cần: `find_changes`/`diff_region` là thuật toán được test kỹ nhất repo
// nhưng chưa từng chạy trên đường sống nào — UI sắp nối vào đây, và bài học
// của phiên này là đường sống phải được chứng minh TRƯỚC khi client dựa vào.
// `diff_region` từ chối vùng vượt biên khung hình (không tự kẹp), nên client
// bắt buộc lấy kích thước thật từ `vision:capture` chứ không đoán từ CSS px.
//
// ## Chạy
//   # Gateway release đang chạy (xem đầu e2e-memory.mjs), rồi:
//   node scripts/e2e-vision-watch.mjs        # mặc định cổng 8099
//
// KHÔNG nằm trong CI: cần tiến trình sống + màn hình thật. Thoát 1 nếu trượt.

import { ketNoi, goiLenh } from './lib/ws-client.mjs'

const PORT = Number(process.env.PORT || 8099)

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

const main = async () => {
  console.log(`Gateway: ws://127.0.0.1:${PORT}/ws\n`)
  const kn = await ketNoi({ port: PORT, origin: 'http://localhost:5173' })
  if (!kn.ok) {
    console.log(`❌ Không kết nối được: ${kn.ly}`)
    process.exit(1)
  }
  const ws = kn.ws

  // 1. capture: lấy kích thước khung thật (đồng thời mồi last_frame làm baseline)
  const cap = await goiLenh(ws, 'vision:capture', {}, 30000)
  const w = cap.payload?.width
  const h = cap.payload?.height
  ghi('vision:capture trả kích thước khung', Number.isInteger(w) && Number.isInteger(h), `${w}×${h}`)
  if (!Number.isInteger(w)) { ws.close(); return ket }

  // 2. add_region: toàn màn hình, đúng kích thước vừa đo
  const region = { id: 'e2e-watch', name: 'Toàn màn hình', x: 0, y: 0, width: w, height: h, threshold: 0.02 }
  const add = await goiLenh(ws, 'vision:add_region', region)
  ghi('vision:add_region nhận vùng đúng biên', add.payload?.success === true,
    add.event === 'vision:add_region_error' ? add.payload?.error : '')

  // 3. get_changed_regions lần 1: so với baseline từ capture ở bước 1.
  //    Màn hình có thể tĩnh hoặc động — chỉ khẳng định HỢP ĐỒNG: đúng vùng,
  //    difference nằm trong [0,1].
  const l1 = await goiLenh(ws, 'vision:get_changed_regions', {}, 30000)
  const r1 = Array.isArray(l1.payload) ? l1.payload.find((r) => r.region_id === 'e2e-watch') : null
  ghi('get_changed_regions trả kết quả cho vùng đã đăng ký', !!r1,
    r1 ? `difference=${r1.difference.toFixed(4)}, is_changed=${r1.is_changed}` : JSON.stringify(l1.payload ?? l1.ly).slice(0, 120))
  if (r1) ghi('difference nằm trong [0,1]', r1.difference >= 0 && r1.difference <= 1)

  // 4. gọi lại ngay: hai frame sát nhau, màn hình đứng yên thì difference nhỏ.
  //    Phép kiểm mềm (log, không đỏ) — máy đang phát video sẽ khác 0 là đúng.
  const l2 = await goiLenh(ws, 'vision:get_changed_regions', {}, 30000)
  const r2 = Array.isArray(l2.payload) ? l2.payload.find((r) => r.region_id === 'e2e-watch') : null
  ghi('lần 2 vẫn trả kết quả hợp lệ', !!r2 && r2.difference >= 0 && r2.difference <= 1,
    r2 ? `difference=${r2.difference.toFixed(4)}` : '')

  // 5. dọn: remove_region
  const rm = await goiLenh(ws, 'vision:remove_region', { id: 'e2e-watch' })
  ghi('vision:remove_region dọn được vùng', rm.payload?.success === true)

  // 6. vùng vượt biên phải bị TỪ CHỐI có lý do (hợp đồng mà UI dựa vào)
  const qua = await goiLenh(ws, 'vision:add_region',
    { id: 'e2e-oob', name: 'quá biên', x: 0, y: 0, width: w + 1000, height: h, threshold: 0.02 })
  await goiLenh(ws, 'vision:get_changed_regions', {}, 30000).then(async (res) => {
    const oob = Array.isArray(res.payload) ? null : res.payload?.error
    ghi('vùng vượt biên bị từ chối với thông điệp rõ', res.event === 'vision:get_changed_regions_error' && String(oob).includes('exceed'),
      String(oob ?? '').slice(0, 90))
    await goiLenh(ws, 'vision:remove_region', { id: 'e2e-oob' })
  })
  void qua

  ws.close()
  return ket
}

main()
  .then((r) => {
    const truot = r.filter((x) => !x.dat).length
    console.log(`\n${r.length - truot}/${r.length} đạt`)
    process.exit(truot > 0 ? 1 : 0)
  })
  .catch((e) => { console.error('LỖI:', e); process.exit(1) })
