const $ = (id) => document.getElementById(id)
const co = (n) => {
  if (n >= 1024 ** 3) return (n / 1024 ** 3).toFixed(2) + ' GB'
  if (n >= 1024 ** 2) return (n / 1024 ** 2).toFixed(1) + ' MB'
  return (n / 1024).toFixed(0) + ' KB'
}

const tauri = window.__TAURI__
const goi = (command, payload = {}) => tauri.core.invoke('native_ipc_call', { command, payload })

let dangTai = false

async function veTrangThai() {
  let st
  try {
    st = await goi('setup:status')
  } catch (e) {
    $('mota').innerHTML = '<span class="loi">Không đọc được danh sách model: ' + e + '</span>'
    return
  }

  $('danhsach').innerHTML = st.groups
    .map((g) => {
      const lop = g.ready ? 'ok' : g.required ? 'thieu' : 'tuychon'
      const nhan = g.ready ? 'sẵn sàng' : g.required ? 'CHƯA CÓ' : 'chưa bật'
      const hong = !g.ready && g.broken ? '<div class="hong">' + g.broken + '</div>' : ''
      return (
        '<div class="nhom"><span class="cham ' + lop + '"></span>' +
        '<span class="ten">' + g.name + '</span>' +
        '<span class="trangthai">' + nhan + '</span></div>' + hong
      )
    })
    .join('')

  const taiDuoc = st.missing.filter((f) => f.downloadable)
  const soByte = taiDuoc.reduce((s, f) => s + f.bytes, 0)

  if (!st.missing.length) {
    $('mota').textContent = 'Đã đủ model. LIVA dùng được ngay.'
    $('tai').disabled = true
    $('bo').textContent = 'Đóng'
  } else if (st.blocking) {
    $('mota').textContent =
      'LIVA còn thiếu model bắt buộc. Ứng dụng vẫn mở để bạn tải lại; các năng lực ghi ở nhóm thiếu chưa dùng được.'
  } else {
    $('mota').textContent = 'Model bắt buộc đã đủ. Phần còn thiếu chỉ là tính năng tuỳ chọn.'
  }

  if (taiDuoc.length && !dangTai) {
    $('tai').disabled = false
    $('tai').textContent = 'Tải ' + taiDuoc.length + ' file (~' + co(soByte) + ')'
  }

  const tuTay = st.missing.filter((f) => !f.downloadable)
  $('ghichu').textContent = tuTay.length
    ? tuTay.length + ' file phải tự chuẩn bị (không có nguồn tải công khai).'
    : ''

  try {
    const p = await goi('setup:paths')
    $('duongdan').textContent =
      'Model     : ' + p.resourceRoot + '\n' +
      'Model LLM : ' + p.llmDir + '\n' +
      'Dữ liệu   : ' + p.dataDir + '\n' +
      'Cấu hình  : ' + p.configFile
  } catch { /* không quan trọng bằng phần trên */ }
}

async function tai() {
  dangTai = true
  $('tai').disabled = true
  $('khoitai').hidden = false
  $('mota').textContent = 'Đang tải. Có thể đóng cửa sổ và làm việc khác — lần sau mở lại sẽ tải tiếp phần dở.'

  const reqId = 'setup-' + Date.now()
  const un = await tauri.event.listen('ipc-stream:' + reqId, (ev) => {
    const d = ev.payload && ev.payload.data
    if (!d || !d.progress) return
    const p = d.progress
    const pt = p.overall_total ? (p.overall_downloaded / p.overall_total) * 100 : 0
    $('thanh').value = pt
    $('tiendo').textContent =
      '[' + p.index + '/' + p.total_files + '] ' + p.dest +
      ' — ' + co(p.overall_downloaded) + ' / ' + co(p.overall_total)
  })

  try {
    const kq = await tauri.core.invoke('native_ipc_call_stream', {
      command: 'setup:fetch',
      payload: {},
      reqId,
    })
    $('tiendo').textContent = ''
    if (kq.failed && kq.failed.length) {
      $('mota').innerHTML =
        '<span class="loi">' + kq.failed.length +
        ' file tải hỏng. Bấm lại để tải tiếp — phần đã tải được giữ lại.</span>'
      $('tiendo').innerHTML = '<pre class="loi">' + kq.failed.join('\n') + '</pre>'
    } else {
      $('mota').textContent = 'Tải xong.'
    }
  } catch (e) {
    $('mota').innerHTML = '<span class="loi">Tải hỏng: ' + e + '</span>'
  } finally {
    un()
    dangTai = false
    $('thanh').value = 100
    await veTrangThai()
  }
}

$('tai').addEventListener('click', tai)
$('bo').addEventListener('click', () => {
  tauri.core.invoke('open_dashboard').catch(() => {})
  window.__TAURI__.window.getCurrentWindow().close()
})

veTrangThai()
