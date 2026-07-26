#!/usr/bin/env node
// Kiểm chứng MCP CLIENT của LIVA (rung G0) với MCP server NGOÀI THẬT, qua
// gateway thật + WebSocket thật.
//
// Vì sao cần, khi đã có `liva-native-core/tests/mcp_client_e2e.rs`: test đó chỉ
// nói chuyện với server mock trong repo (`e2e-mcp-server.mjs`). Mock không thể
// lộ những chỗ lệch giữa "đúng chuẩn trên giấy" và "server ngoài kia làm gì
// thật". Lần chạy đầu (26/07/2026) lộ ngay một lỗi mock không thấy được: drain
// stderr log ở `debug!`, mà `main.rs` dựng subscriber `.with_max_level(INFO)`
// CỨNG (không `EnvFilter`) — nên stderr của server con chết bị chôn hoàn toàn.
//
// Chuỗi được đo là chuỗi đầy đủ, không phải gọi hàm trong tiến trình:
//   WebSocket → handle_command → McpClientRegistry → npx → server MCP thật
//
// ## Chạy
//
//   cd liva-native-core; cargo build --bin liva-native-core     # nếu chưa có
//   node scripts/verify-mcp-real.mjs                            # PORT=8099 mặc định
//
// Script TỰ dựng mọi thứ trong thư mục tạm: `mcp_config.json` riêng (trỏ bằng
// `LIVA_MCP_CONFIG`), thư mục gốc cho server-filesystem, và một server cố tình
// chết. Nó KHÔNG đọc `mcp_config.json` của bạn — nên chạy được trên máy sạch và
// không phụ thuộc cấu hình riêng của ai.
//
// KHÔNG nằm trong CI: lần đầu cần mạng để `npx` tải hai package
// `@modelcontextprotocol/server-everything` và `-filesystem` (~10 s mỗi cái),
// và cần một tiến trình core sống. Thoát 1 nếu có mục nào trượt.

import net from 'node:net'
import fs from 'node:fs'
import os from 'node:os'
import path from 'node:path'
import { spawn } from 'node:child_process'
import { fileURLToPath } from 'node:url'

const GOC = path.resolve(path.dirname(fileURLToPath(import.meta.url)), '..')
const { ketNoi, goiLenh } = await import(new URL('./lib/ws-client.mjs', import.meta.url))

const PORT = Number(process.env.PORT || 8099)
const ORIGIN = 'http://localhost:5173'
const CORE = path.join(GOC, 'target', 'debug', 'liva-native-core.exe')

const ket = []
const ghi = (ten, dat, chiTiet = '') => {
  ket.push({ ten, dat })
  console.log(`${dat ? '✅' : '❌'} ${ten}${chiTiet ? '\n      ↳ ' + chiTiet : ''}`)
}

// ── Dựng sân chơi trong thư mục tạm ─────────────────────────────────────────

const tam = fs.mkdtempSync(path.join(os.tmpdir(), 'liva-mcp-verify-'))
const gocFs = path.join(tam, 'fs-root')
const tepThu = path.join(gocFs, 'thu.txt')
fs.mkdirSync(gocFs)
fs.writeFileSync(tepThu,
  'Day la tep thu cho MCP server-filesystem.\n' +
  'Dong hai: LIVA doc duoc file nay qua MCP client that.\n')

// Server cố tình chết: viết ra FILE thay vì `node -e` để không phải đánh vật
// với trích dẫn lồng nhau qua ba tầng shell.
const tepChet = path.join(tam, 'chet-ngay.mjs')
fs.writeFileSync(tepChet,
  "process.stderr.write('LOI GIA DINH: thieu bien moi truong FOO_TOKEN\\n')\n" +
  "process.stderr.write('    tai chuc-nang.js:42\\n')\n" +
  'process.exit(1)\n')

const tepCauHinh = path.join(tam, 'mcp_config.json')
fs.writeFileSync(tepCauHinh, JSON.stringify({
  mcpServers: {
    everything: { command: 'npx', args: ['-y', '@modelcontextprotocol/server-everything'] },
    filesystem: { command: 'npx', args: ['-y', '@modelcontextprotocol/server-filesystem', gocFs] },
    chet_ngay: { command: 'node', args: [tepChet] },
    // Hai mục dưới đây PHẢI bị bộ đọc loại — đó là điều được kiểm, không phải rác.
    _cho_giu_cho: { _comment: 'ten bat dau bang _ = cho giu cho, phai bi loai', command: 'khong-ton-tai' },
    tat_hoan_toan: { command: 'khong-ton-tai', disabled: true },
  },
}, null, 2) + '\n')

const doiCong = (port, hanGio = 30000) =>
  new Promise((giaiQuyet) => {
    const het = Date.now() + hanGio
    const thu = () => {
      const s = net.connect({ host: '127.0.0.1', port })
      s.on('connect', () => { s.destroy(); giaiQuyet(true) })
      s.on('error', () => {
        s.destroy()
        if (Date.now() > het) giaiQuyet(false)
        else setTimeout(thu, 300)
      })
    }
    thu()
  })

// ── Khởi động core ──────────────────────────────────────────────────────────
//
// stdin để 'pipe' và KHÔNG BAO GIỜ gọi `stdin.end()`: core đọc stdin cho IPC và
// thoát khi EOF. Chạy nền với stdin đóng sẽ in "shutting down" rồi exit 0 —
// trông đúng như một lần chạy thành công.

if (!fs.existsSync(CORE)) {
  console.log(`Chưa có ${CORE}\nChạy: cd liva-native-core; cargo build --bin liva-native-core`)
  process.exit(1)
}

const logCore = []
const core = spawn(CORE, [], {
  cwd: GOC,
  stdio: ['pipe', 'pipe', 'pipe'],
  env: {
    ...process.env,
    LIVA_SERVER_PORT: String(PORT),
    LIVA_DB_IN_MEMORY: '1',
    LIVA_ENCRYPTION_KEY: '0'.repeat(32),
    LIVA_MCP_CONFIG: tepCauHinh,
  },
})
core.stdout.on('data', (d) => logCore.push(String(d)))
core.stderr.on('data', (d) => logCore.push(String(d)))

const donDep = () => {
  try { core.stdin.end() } catch { /* đã đóng */ }
  try { core.kill() } catch { /* đã chết */ }
  try { fs.rmSync(tam, { recursive: true, force: true }) } catch { /* thôi */ }
}

const layText = (content) => (content ?? []).find((c) => c.type === 'text')?.text

// Log của core mang mã màu ANSI, và đường dẫn Windows CÓ dấu cách
// (`C:\Program Files\nodejs\npx.cmd`) — nên phải bỏ ANSI rồi cắt tới mốc
// ` server=`, không dùng `[^\s]+`.
const boAnsi = (s) => s.replace(/\u001b\[[0-9;]*m/g, '')
const timDongLog = (mau) =>
  boAnsi(logCore.join(''))
    .split('\n')
    .map((l) => l.trim())
    .filter((l) => mau.test(l))

// ── Bộ kiểm chứng ───────────────────────────────────────────────────────────

const chinh = async () => {
  if (!(await doiCong(PORT))) {
    console.log(`Core không mở được cổng ${PORT}. Log:\n` + logCore.join(''))
    return ket
  }
  console.log(`Gateway : ws://127.0.0.1:${PORT}/ws`)
  console.log(`Cấu hình: ${tepCauHinh}\n`)

  const kn = await ketNoi({ port: PORT, origin: ORIGIN })
  if (!kn.ok) { ghi('Kết nối WebSocket', false, kn.ly); return ket }
  const ws = kn.ws

  // 1. Bộ đọc cấu hình, trên file thật.
  const ds = await goiLenh(ws, 'mcp_client:list_servers')
  const hang = ds.payload?.servers ?? []
  const ten = hang.map((s) => s.name).sort()
  ghi('mcp_client:list_servers đọc được mcp_config.json',
    ds.event === 'mcp_client:list_servers_response' && ds.payload?.configExists === true,
    `configExists=${ds.payload?.configExists} · ${ds.payload?.configPath}`)
  ghi('Lọc đúng: bỏ mục _-prefix và mục disabled',
    ten.length === 3 && ten.join(',') === 'chet_ngay,everything,filesystem',
    `5 mục khai báo → còn [${ten.join(', ')}]`)
  ghi('Nối lười: chưa gọi thì chưa spawn',
    hang.every((s) => s.connected === false),
    hang.map((s) => `${s.name}=${s.connected}`).join(' '))
  ghi('Không trả giá trị biến môi trường (chỉ trả tên)',
    hang.every((s) => s.env === undefined && Array.isArray(s.envKeys)),
    'khối env có thể chứa token — lệnh này đi ra WebSocket')

  // 2. Spawn npx thật + handshake. Đây là chỗ `resolve_program` được đo:
  //    `std::process::Command` không áp PATHEXT nên "npx" phải thành "npx.cmd".
  const t0 = Date.now()
  const lt = await goiLenh(ws, 'mcp_client:list_tools', { server: 'everything' }, 120000)
  const tools = lt.payload?.tools ?? []
  ghi('npx spawn được + handshake xong (server-everything)',
    lt.event === 'mcp_client:list_tools_response' && tools.length > 0,
    tools.length
      ? `${tools.length} tool sau ${((Date.now() - t0) / 1000).toFixed(1)}s`
      : JSON.stringify(lt.payload ?? lt.ly).slice(0, 250))
  const dongSpawn = timDongLog(/đã spawn MCP server:/)
  const duongDanNpx = dongSpawn
    .map((l) => l.match(/đã spawn MCP server: (.+?)(?:\s+server=|$)/)?.[1]?.trim())
    .find((p) => p && /npx/i.test(p))
  ghi('resolve_program giải "npx" ra file thực thi thật',
    process.platform !== 'win32' || /npx\.(cmd|bat)$/i.test(duongDanNpx ?? ''),
    process.platform === 'win32'
      ? `spawn: ${duongDanNpx ?? '!! KHÔNG thấy dòng spawn nào chứa npx — bằng chứng thiếu'}`
      : `(không phải Windows — PATHEXT không liên quan) spawn: ${duongDanNpx ?? '?'}`)

  // 3. Handshake phải có bằng chứng, không chỉ "đã spawn".
  const ds2 = await goiLenh(ws, 'mcp_client:list_servers')
  const ev = (ds2.payload?.servers ?? []).find((s) => s.name === 'everything')
  ghi('Handshake có bằng chứng (protocolVersion + serverInfo từ server)',
    ev?.connected === true && !!ev?.protocolVersion,
    `protocolVersion=${ev?.protocolVersion} serverInfo=${JSON.stringify(ev?.serverInfo)}`)

  // 4. call_tool cơ bản.
  if (tools.some((t) => t.name === 'echo')) {
    const e = await goiLenh(ws, 'mcp_client:call_tool',
      { server: 'everything', name: 'echo', arguments: { message: 'xin chao tu LIVA' } }, 60000)
    const text = layText(e.payload?.content)
    ghi('call_tool(echo)', typeof text === 'string' && text.includes('xin chao tu LIVA'),
      text?.slice(0, 120) ?? JSON.stringify(e.payload ?? e.ly).slice(0, 200))
  }

  // 5. Content dạng ảnh — kiểm bản sửa `rename_all` cấp variant trong
  //    protocol.rs. Trên dây trường là `mimeType` CẢ HAI CHIỀU.
  const tenAnh = tools.find((t) => /image/i.test(t.name))?.name
  if (tenAnh) {
    const a = await goiLenh(ws, 'mcp_client:call_tool',
      { server: 'everything', name: tenAnh, arguments: {} }, 60000)
    const anh = (a.payload?.content ?? []).find((c) => c.type === 'image')
    const loai = anh?.mimeType
    ghi(`call_tool(${tenAnh}) đọc được content ảnh`,
      typeof loai === 'string' && loai.length > 0 && String(anh?.data).length > 100,
      anh ? `mimeType=${loai} · data ${String(anh.data).length}B`
        : JSON.stringify(a.payload ?? a.ly).slice(0, 250))
  }

  // 6. Content loại lạ không được làm vỡ cả lời gọi.
  const tenLa = tools.find((t) => /annotated|resource/i.test(t.name))?.name
  if (tenLa) {
    const l = await goiLenh(ws, 'mcp_client:call_tool',
      { server: 'everything', name: tenLa, arguments: {} }, 60000)
    ghi(`call_tool(${tenLa}) không vỡ vì content loại lạ`,
      Array.isArray(l.payload?.content),
      `loại content: [${(l.payload?.content ?? []).map((c) => c.type).join(', ')}]`)
  }

  // 7. Server thứ hai, song song — registry phải giữ được nhiều kết nối.
  const fs2 = await goiLenh(ws, 'mcp_client:list_tools', { server: 'filesystem' }, 120000)
  const fsTools = (fs2.payload?.tools ?? []).map((t) => t.name)
  ghi('Server thứ hai (server-filesystem) nối song song',
    fsTools.length > 0,
    fsTools.length ? `${fsTools.length} tool: ${fsTools.slice(0, 6).join(', ')}…`
      : JSON.stringify(fs2.payload ?? fs2.ly).slice(0, 250))

  const tenDoc = fsTools.find((t) => t === 'read_text_file') ?? fsTools.find((t) => /^read_file$/.test(t))
  if (tenDoc) {
    const r = await goiLenh(ws, 'mcp_client:call_tool',
      { server: 'filesystem', name: tenDoc, arguments: { path: tepThu } }, 60000)
    const text = layText(r.payload?.content)
    ghi(`call_tool(filesystem/${tenDoc}) đọc được file thật trên đĩa`,
      typeof text === 'string' && text.includes('Dong hai'),
      text?.replace(/\n/g, ' | ').slice(0, 130) ?? JSON.stringify(r.payload ?? r.ly).slice(0, 250))
  }

  // 8. Server CHẾT: lỗi phải tới ngay, và stderr của nó phải hiện ở mức mặc
  //    định. Trước bản sửa 26/07 thì drain chỉ log `debug!`, mà subscriber cứng
  //    ở INFO → stack trace của server chết biến mất hoàn toàn.
  const tChet = Date.now()
  const chet = await goiLenh(ws, 'mcp_client:list_tools', { server: 'chet_ngay' }, 60000)
  const dtChet = Date.now() - tChet
  ghi('Server chết → lỗi tới NGAY, không chờ hết timeout',
    chet.event === 'mcp_client:list_tools_error' && dtChet < 15000,
    `${dtChet}ms — ${String(chet.payload?.error ?? chet.ly).slice(0, 140)}`)
  const dongFoo = timDongLog(/FOO_TOKEN/)
  ghi('stderr của server chết HIỆN ở mức log mặc định (INFO)',
    dongFoo.length > 0,
    dongFoo.length
      ? `bắt được: ${dongFoo[0].slice(0, 110)}`
      : '!! KHÔNG thấy — drain đang log ở debug mà main.rs hard-code Level::INFO')

  // 9. Tên server lạ phải chỉ đường, không chỉ "not found".
  const xau = await goiLenh(ws, 'mcp_client:list_tools', { server: 'khong-ton-tai' })
  ghi('Tên server lạ → lỗi liệt kê server đang có',
    xau.event === 'mcp_client:list_tools_error'
      && /everything/.test(String(xau.payload?.error)),
    String(xau.payload?.error ?? xau.ly).slice(0, 160))

  ws.close()
  return ket
}

chinh()
  .then((r) => {
    const tho = logCore.join('')
    const vt = tho.indexOf('FOO_TOKEN')
    if (vt >= 0) {
      console.log('\n── stderr server chết, in lại ở WARN ──')
      console.log(tho.slice(Math.max(0, vt - 300), vt + 80).split('\n').slice(-5).join('\n'))
    }
    const truot = r.filter((x) => !x.dat).length
    console.log(`\n${r.length - truot}/${r.length} đạt`)
    donDep()
    process.exit(truot > 0 ? 1 : 0)
  })
  .catch((e) => { console.error('LỖI:', e); donDep(); process.exit(1) })
