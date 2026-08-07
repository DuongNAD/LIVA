#!/usr/bin/env node
//
// Kiểm chứng bề mặt tương thích OpenAI của LIVA — 0 dependency
// ============================================================
//
// Bộ này dùng `node:http` thuần thay vì gói `openai`. Lý do: kết quả nghiệm thu
// phải tái lập được mà không bắt ai cài thêm gì, và repo không phải gánh một
// dependency chỉ để chạy một bộ kiểm. Bản đo bằng SDK chính chủ đã chạy riêng
// một lần (v7.4.0, 6/6) và ghi trong mục U28 của
// `docs/03-danh-gia/05-nang-cap-toan-dien.md`.
//
// ## Chạy
//
//   # Cửa sổ 1 — giữ stdin MỞ (lõi thoát khi gặp EOF trên stdin)
//   $env:LIVA_SERVER_PORT="8099"; $env:LIVA_OPENAI_PORT="8003"
//   $env:LIVA_DB_IN_MEMORY="1"; $env:LIVA_TTS_VIENEU="1"
//   .\target\debug\liva-native-core.exe
//
//   # Cửa sổ 2
//   node examples/openai-api-check.mjs          # mặc định cổng 8003
//   OPENAI_PORT=8003 node examples/openai-api-check.mjs
//
// Thoát 0 nếu mọi mục đạt, 1 nếu có mục trượt.

import http from 'node:http'

const PORT = Number(process.env.OPENAI_PORT || 8003)
const HOST = '127.0.0.1'

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push(dat)
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? ' — ' + chiTiet : ''}`)
}

/** Một request HTTP. `raw: true` trả Buffer thô (dùng cho WAV). */
function goi(method, path, body, { raw = false, hanGio = 180_000 } = {}) {
  return new Promise((xong, hong) => {
    const payload = body === undefined ? null : Buffer.from(JSON.stringify(body), 'utf8')
    const req = http.request(
      {
        host: HOST,
        port: PORT,
        path,
        method,
        headers: payload
          ? { 'Content-Type': 'application/json', 'Content-Length': payload.length }
          : {},
        timeout: hanGio,
      },
      (res) => {
        const mieng = []
        res.on('data', (c) => mieng.push(c))
        res.on('end', () => {
          const buf = Buffer.concat(mieng)
          if (raw) return xong({ status: res.statusCode, headers: res.headers, buf })
          try {
            xong({ status: res.statusCode, headers: res.headers, json: JSON.parse(buf.toString('utf8')) })
          } catch {
            xong({ status: res.statusCode, headers: res.headers, text: buf.toString('utf8') })
          }
        })
      },
    )
    req.on('error', hong)
    req.on('timeout', () => { req.destroy(); hong(new Error(`quá hạn ${hanGio}ms`)) })
    if (payload) req.write(payload)
    req.end()
  })
}

/** Gom một luồng SSE thành danh sách sự kiện đã tách. */
function goiSse(path, body, hanGio = 180_000) {
  return new Promise((xong, hong) => {
    const payload = Buffer.from(JSON.stringify(body), 'utf8')
    const req = http.request(
      {
        host: HOST, port: PORT, path, method: 'POST',
        headers: { 'Content-Type': 'application/json', 'Content-Length': payload.length },
        timeout: hanGio,
      },
      (res) => {
        let dem = ''
        const su = []
        res.setEncoding('utf8')
        res.on('data', (c) => {
          dem += c
          // Sự kiện SSE ngăn nhau bằng một dòng trống.
          let cat
          while ((cat = dem.indexOf('\n\n')) >= 0) {
            const khoi = dem.slice(0, cat).trim()
            dem = dem.slice(cat + 2)
            if (khoi.startsWith('data: ')) su.push(khoi.slice(6))
          }
        })
        res.on('end', () => xong({ status: res.statusCode, headers: res.headers, su }))
      },
    )
    req.on('error', hong)
    req.on('timeout', () => { req.destroy(); hong(new Error(`quá hạn ${hanGio}ms`)) })
    req.write(payload)
    req.end()
  })
}

/**
 * Chờ router LLM nạp xong.
 *
 * Cổng HTTP mở NGAY khi boot, còn model thì nạp bất đồng bộ sau đó — nên có một
 * cửa sổ vài chục giây mà mọi request đều trả `"No model loaded"`. Không chờ thì
 * bộ kiểm này báo đỏ vì một lý do không liên quan gì tới bề mặt API, và người
 * chạy sẽ đi tìm lỗi ở đúng chỗ không có lỗi. (Đo 06/08/2026: mở cổng ở giây
 * ~20, model sẵn sàng muộn hơn nhiều.)
 */
async function doiModel(hanGio = 420_000) {
  const batDau = Date.now()
  let daBao = false
  const bao = () => {
    if (daBao) return
    console.log('⏳ Router LLM đang nạp (cổng mở trước, model nạp sau)…')
    daBao = true
  }

  for (;;) {
    // Hạn từng lượt phải RỘNG. Request đầu tiên không "trả lỗi rồi thôi" — nó
    // CHẶN sau khoá của engine tới khi model nạp xong. Đo 06/08/2026 trên máy
    // dev: 95 giây với Qwen3-VL-2B. Đặt hạn 60s ở đây thì bộ kiểm tự đánh
    // trượt mình ngay trước lúc mọi thứ sẵn sàng.
    let thu
    try {
      thu = await goi('POST', '/v1/chat/completions', {
        model: 'liva-local',
        messages: [{ role: 'user', content: 'ping' }],
      }, { hanGio: 240_000 })
    } catch {
      bao() // quá hạn = vẫn đang nạp; thử lại tới khi hết ngân sách chung
      thu = null
    }

    if (thu?.status === 200) {
      if (daBao) console.log(`   sẵn sàng sau ${Math.round((Date.now() - batDau) / 1000)}s\n`)
      return true
    }
    // Lỗi KHÁC "No model loaded" là lỗi thật — để các bài kiểm bên dưới báo cáo
    // đúng chỗ, đừng nuốt ở đây.
    if (thu && !/no model loaded/i.test(thu.json?.error?.message ?? '')) return true

    if (Date.now() - batDau > hanGio) {
      console.log(`⏳ Model vẫn chưa nạp sau ${Math.round(hanGio / 1000)}s — chạy tiếp và để kết quả nói.\n`)
      return false
    }
    bao()
    await new Promise((r) => setTimeout(r, 5_000))
  }
}

const main = async () => {
  console.log(`LIVA OpenAI API: http://${HOST}:${PORT}/v1\n`)

  await doiModel()

  // 1 — /v1/models. SDK gọi đường này đầu tiên để dò kết nối.
  const models = await goi('GET', '/v1/models')
  ghi('GET /v1/models', models.status === 200 && Array.isArray(models.json?.data) && models.json.data.length > 0,
    models.json?.data?.map((m) => m.id).join(', ') ?? String(models.status))

  // 2 — chat không stream.
  const chat = await goi('POST', '/v1/chat/completions', {
    model: 'liva-local',
    messages: [{ role: 'user', content: 'Thủ đô của Việt Nam là thành phố nào?' }],
  })
  const noiDung = chat.json?.choices?.[0]?.message?.content
  ghi('POST /v1/chat/completions', chat.status === 200 && typeof noiDung === 'string' && noiDung.length > 0,
    JSON.stringify(noiDung ?? chat.json))
  ghi('hình dạng hồi âm đúng chuẩn OpenAI',
    chat.json?.object === 'chat.completion' && chat.json?.choices?.[0]?.finish_reason === 'stop')
  ghi('có usage token', (chat.json?.usage?.total_tokens ?? 0) > 0, `${chat.json?.usage?.total_tokens} token`)

  // 3 — tag điều khiển avatar KHÔNG được rò ra API. Đây là hồi quy: lượt đo
  //     đầu tiên ngày 06/08/2026 trả về "[happy][wave] Xin chào bạn nhé."
  ghi('không rò tag điều khiển avatar', typeof noiDung === 'string' && !/^\s*\[[a-z_]+\]/.test(noiDung),
    typeof noiDung === 'string' ? JSON.stringify(noiDung.slice(0, 40)) : '')

  // 4 — SSE.
  const sse = await goiSse('/v1/chat/completions', {
    model: 'liva-local', stream: true,
    messages: [{ role: 'user', content: 'Đếm từ một tới ba.' }],
  })
  const chuoi = sse.su
    .filter((s) => s !== '[DONE]')
    .map((s) => { try { return JSON.parse(s) } catch { return null } })
    .filter(Boolean)
  const chu = chuoi.map((c) => c.choices?.[0]?.delta?.content ?? '').join('')
  ghi('stream: true trả text/event-stream', (sse.headers['content-type'] ?? '').includes('text/event-stream'))
  ghi('luồng SSE có nội dung', chu.length > 0, JSON.stringify(chu))
  ghi('luồng kết đúng cách', sse.su.at(-1) === '[DONE]'
    && chuoi.some((c) => c.choices?.[0]?.finish_reason === 'stop'),
    `${sse.su.length} sự kiện`)

  // 5 — TTS. Kiểm cả header WAV, không chỉ mã trạng thái: một thân rỗng dán
  //     nhãn audio/wav vẫn trả 200.
  const noi = await goi('POST', '/v1/audio/speech', {
    model: 'liva-local', voice: 'default', input: 'Xin chào, mình là Liva.',
  }, { raw: true })
  const wav = noi.buf ?? Buffer.alloc(0)
  const laWav = wav.length > 44 && wav.subarray(0, 4).toString() === 'RIFF' && wav.subarray(8, 12).toString() === 'WAVE'
  ghi('POST /v1/audio/speech trả WAV', noi.status === 200 && laWav, `${wav.length} byte`)
  if (laWav) {
    // Cỡ ghi trong header phải khớp thân thật, nếu không trình phát cắt sớm.
    const riff = wav.readUInt32LE(4)
    const data = wav.readUInt32LE(40)
    ghi('header WAV khớp cỡ file thật', riff === wav.length - 8 && data === wav.length - 44,
      `${wav.readUInt16LE(22)} kênh · ${wav.readUInt32LE(24)} Hz · ${wav.readUInt16LE(34)}-bit`)
  }

  // 6 — đường dẫn lạ phải trả lỗi ĐÚNG HÌNH DẠNG OpenAI; SDK đọc error.message.
  const la = await goi('GET', '/v1/khong-ton-tai')
  ghi('đường dẫn lạ trả lỗi hình dạng OpenAI',
    la.status === 404 && typeof la.json?.error?.message === 'string')

  const dat = ket.filter(Boolean).length
  console.log(`\n${dat}/${ket.length} đạt`)
  return dat === ket.length ? 0 : 1
}

main().then((ma) => process.exit(ma)).catch((e) => {
  console.error(`\n❌ ${e.message}\nLIVA chưa chạy, hoặc chưa đặt LIVA_OPENAI_PORT? Xem hướng dẫn ở đầu file.`)
  process.exit(1)
})
