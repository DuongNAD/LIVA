#!/usr/bin/env node
// Kiểm tra và tải model cho LIVA — `doctor` (chẩn đoán) và `fetch` (tải).
//
// Vì sao cần script này: weight bị gitignore, nên `git clone` xong LIVA **không
// chạy được**. `models/README.md` đã ghi đủ nguồn tải, nhưng đó là *hướng dẫn
// thủ công* cho hàng chục file / ~11,6 GB trải trên nhiều nguồn — không ai ngoài
// người viết nó cài nổi. Tệ hơn: thiếu model **không gây lỗi**. Lõi vẫn khởi
// động, vẫn nhận lệnh, chỉ là RAG im lặng bỏ qua, TTS rơi xuống backend khác,
// `vision:ask` báo lỗi lúc gọi chứ không phải lúc boot. Đó là kiểu hỏng tệ
// nhất: không có gì đỏ, chỉ có tính năng lặng lẽ vắng mặt.
//
// `doctor` biến sự vắng mặt im lặng đó thành một bảng: thiếu file nào → tính
// năng nào đang TẮT → sửa bằng lệnh gì.
//
// Dùng:
//   node scripts/models.mjs doctor
//   node scripts/models.mjs fetch                     # profile minimal
//   node scripts/models.mjs fetch --profile full
//   node scripts/models.mjs fetch --only stt,rag
//   node scripts/models.mjs fetch --llm-dir D:/AI_Models
//   node scripts/models.mjs fetch --dry-run
//   node scripts/models.mjs fetch --force    # tải lại cả file lệch kích thước
//
// Thoát 1 khi `doctor` thấy thiếu file BẮT BUỘC, hoặc khi `fetch` tải hỏng.
//
// Ghi chú kiểm chứng (26/07/2026): mọi URL trong MANIFEST đã được HEAD thử và
// trả 200; kích thước hai file Qwen3-VL khớp **từng byte** với bản đang chạy
// trên máy dev. Các `bytes` khác lấy từ chính máy dev nên là *tham chiếu* —
// lệch kích thước bị báo cảnh báo chứ không bị coi là hỏng (xem `soKichThuoc`).

import fs from 'node:fs'
import path from 'node:path'
import crypto from 'node:crypto'
import { Readable, Transform } from 'node:stream'
import { pipeline } from 'node:stream/promises'

const ROOT = path.resolve(import.meta.dirname, '..')
const CONFIG = path.join(ROOT, 'data', 'liva-config.json')

// Thư mục model LLM mặc định khi config chưa nói gì. KHÔNG dùng hằng
// `DEFAULT_MODELS_DIR` của lõi (`E:\AI_Models`) làm mặc định cho script: đó là
// đường dẫn máy dev, trên laptop người khác không có ổ E:. Đặt trong repo để
// `git clone` + một lệnh là chạy được; ai muốn để chỗ khác thì `--llm-dir`.
const LLM_DIR_MAC_DINH = path.join('models', 'llm')

// ---------------------------------------------------------------------------
// MANIFEST
//
// `groups` gom file theo KHẢ NĂNG, không theo nguồn tải — vì câu hỏi người dùng
// hỏi là "sao nó không nghe được", chứ không phải "file nào của HuggingFace".
// Ngữ nghĩa từng trường nằm trong `_doc` của chính file manifest.
// ---------------------------------------------------------------------------

// Danh sách model KHÔNG còn nằm trong file này: nó sống ở
// `data/models-manifest.json` và được đọc bởi CẢ script này lẫn trình tải model
// trong ứng dụng (`liva-native-core/src/setup`). Lý do: bản cài không có
// `scripts/` và không có Node, nên nếu danh sách chỉ nằm ở đây thì người dùng
// cuối không có cách nào tải model — mà chép nó sang Rust thành bản thứ hai thì
// hai bản sẽ lệch, và bên lệch là bên người dùng chạy.
const MANIFEST_PATH = path.join(ROOT, 'data', 'models-manifest.json')

/** Đọc manifest dùng chung, đổi sang tên trường mà phần còn lại của script dùng. */
function docManifest() {
  let raw
  try {
    raw = JSON.parse(fs.readFileSync(MANIFEST_PATH, 'utf8'))
  } catch (e) {
    console.error(`Không đọc được ${MANIFEST_PATH}: ${e.message}`)
    process.exit(2)
  }
  const nhom = {}
  for (const [khoa, g] of Object.entries(raw.groups ?? {})) {
    nhom[khoa] = { ten: g.name, batBuoc: !!g.required, hong: g.broken, ghiChu: g.note || undefined }
  }
  const manifest = (raw.files ?? []).map((f) => ({
    nhom: f.group,
    profile: f.profile,
    llm: !!f.llm,
    dich: f.dest,
    url: f.url ?? null,
    bytes: f.bytes,
    chinhXac: !!f.exactSize,
    huongDan: f.manual,
    sha256: f.sha256 ?? null,
  }))
  const laHex = (s) => typeof s === 'string' && /^[0-9a-fA-F]{64}$/.test(s)
  for (const m of manifest) {
    if (!nhom[m.nhom]) {
      console.error(`manifest hỏng: file ${m.dich} thuộc nhóm "${m.nhom}" không được khai báo`)
      process.exit(2)
    }
    // FAIL CLOSED. Không có hash thì không tải — chấp nhận một entry "tạm thời
    // chưa có hash" là mở đúng một khe cho thứ cả cổng này sinh ra để chặn, và
    // khe đó sẽ nằm ở file mà người thêm nó vội nhất.
    if (m.url && !laHex(m.sha256)) {
      console.error(
        `manifest hỏng: ${m.dich} có url nhưng sha256 ${m.sha256 ? 'sai định dạng' : 'bị thiếu'} ` +
          `— cần đúng 64 chữ số hex. KHÔNG tải khi chưa có gì để đối chiếu.`,
      )
      process.exit(2)
    }
  }
  return { NHOM: nhom, MANIFEST: manifest }
}

const { NHOM, MANIFEST } = docManifest()

// Các tên file lịch sử không còn được manifest quản lý. Doctor chỉ cảnh báo;
// không tự xoá vì người dùng có thể đang trỏ một cấu hình riêng vào file đó.
const FILE_MO_COI = [
  {
    dich: path.join('models', 'parakeet_vi.onnx.data'),
    thayBang: path.join('models', 'model.onnx_data'),
  },
]

// ---------------------------------------------------------------------------
// Tiện ích
// ---------------------------------------------------------------------------

// Đơn vị theo độ lớn: file config vài trăm KB mà in "0.6 MB" thì hai bản khác
// nhau 52 KB trông y hệt nhau — đúng lúc cần phân biệt thì lại không phân biệt được.
const co = (n) => {
  if (n >= 1024 ** 3) return (n / 1024 ** 3).toFixed(2) + ' GB'
  if (n >= 10 * 1024 ** 2) return (n / 1024 ** 2).toFixed(1) + ' MB'
  if (n >= 1024 ** 2) return (n / 1024 ** 2).toFixed(2) + ' MB'
  return (n / 1024).toFixed(0) + ' KB'
}

const doc = (p) => {
  try {
    return JSON.parse(fs.readFileSync(p, 'utf8'))
  } catch {
    return null
  }
}

/** Thư mục LLM: `--llm-dir` > config `ai.localModelsDir` > mặc định trong repo. */
function thuMucLlm(argLlmDir) {
  if (argLlmDir) return { duongDan: path.resolve(argLlmDir), nguon: '--llm-dir' }
  const cfg = doc(CONFIG)
  const tuConfig = cfg?.ai?.localModelsDir
  if (typeof tuConfig === 'string' && tuConfig.trim()) {
    return { duongDan: path.resolve(tuConfig), nguon: 'data/liva-config.json → ai.localModelsDir' }
  }
  return { duongDan: path.join(ROOT, LLM_DIR_MAC_DINH), nguon: 'mặc định của script' }
}

const duongDanThat = (m, llmDir) =>
  m.llm ? path.join(llmDir, m.dich) : path.join(ROOT, m.dich)

/**
 * Ba trạng thái, cố ý KHÔNG gộp "lệch kích thước" vào "hỏng".
 *
 * `chinhXac: true` (đã đối chiếu content-length với nguồn) thì lệch kích thước
 * đúng là hỏng. Còn lại `bytes` chỉ là số đo trên máy dev: nguồn ở nhánh
 * `main`/`master` có thể phát hành bản mới hợp lệ mà đổi kích thước. Báo đỏ
 * trong trường hợp đó là báo oan, và một cảnh báo hay bị báo oan thì chỉ vài
 * lần là người ta bỏ qua luôn cả cái đúng.
 */
function soKichThuoc(m, llmDir) {
  const p = duongDanThat(m, llmDir)
  if (!fs.existsSync(p)) return { trangThai: 'thieu', thuc: 0 }
  const thuc = fs.statSync(p).size
  if (thuc === m.bytes) return { trangThai: 'du', thuc }
  if (m.chinhXac) return { trangThai: 'hong', thuc }
  return { trangThai: 'lech', thuc }
}

const locTheoProfile = (profile) =>
  profile === 'full' ? MANIFEST : MANIFEST.filter((m) => m.profile === 'minimal')

// ---------------------------------------------------------------------------
// doctor
// ---------------------------------------------------------------------------

function doctor({ argLlmDir }) {
  const { duongDan: llmDir, nguon } = thuMucLlm(argLlmDir)
  const llmTonTai = fs.existsSync(llmDir)

  console.log('\nLIVA doctor — kiểm tra model\n')
  console.log(`  Thư mục repo : ${ROOT}`)
  console.log(`  Thư mục LLM  : ${llmDir}`)
  console.log(`                 (${nguon})${llmTonTai ? '' : '   ⚠ KHÔNG TỒN TẠI'}`)
  if (!llmTonTai) {
    console.log('\n  ⚠ Thư mục LLM không tồn tại. Trên máy khác máy dev, `ai.localModelsDir`')
    console.log('    trong data/liva-config.json thường trỏ vào ổ đĩa không có thật.')
    console.log(`    Sửa nó, hoặc chạy fetch với --llm-dir <đường dẫn>.`)
  }
  console.log('')

  for (const file of FILE_MO_COI) {
    const p = path.join(ROOT, file.dich)
    if (!fs.existsSync(p)) continue
    console.log(`  ⚠ File model mồ côi: ${file.dich} (${co(fs.statSync(p).size)})`)
    console.log(`    Manifest hiện dùng ${file.thayBang}; hãy kiểm tra cấu hình rồi xoá file cũ thủ công.`)
    console.log('')
  }

  const theoNhom = new Map()
  for (const m of MANIFEST) {
    if (!theoNhom.has(m.nhom)) theoNhom.set(m.nhom, [])
    theoNhom.get(m.nhom).push({ ...m, ...soKichThuoc(m, llmDir) })
  }

  let thieuBatBuoc = 0
  let tongThieuBytes = 0
  let soDu = 0
  let soTong = 0

  for (const [khoa, ds] of theoNhom) {
    const meta = NHOM[khoa]
    const thieu = ds.filter((m) => m.trangThai === 'thieu')
    const hong = ds.filter((m) => m.trangThai === 'hong')
    const lech = ds.filter((m) => m.trangThai === 'lech')
    soTong += ds.length
    soDu += ds.filter((m) => m.trangThai === 'du').length

    const sanSang = thieu.length === 0 && hong.length === 0
    const bieuTuong = sanSang ? '✓' : meta.batBuoc ? '✗' : '○'
    const nhan = sanSang ? 'sẵn sàng' : meta.batBuoc ? 'HỎNG' : 'tắt'
    console.log(`  ${bieuTuong} ${meta.ten.padEnd(42)} ${nhan}`)

    if (!sanSang) {
      if (meta.batBuoc) thieuBatBuoc += thieu.length + hong.length
      for (const m of [...thieu, ...hong]) {
        tongThieuBytes += m.bytes
        const vi = m.trangThai === 'hong' ? `sai kích thước (${co(m.thuc)})` : 'thiếu'
        console.log(`      · ${m.dich}  — ${vi}, cần ${co(m.bytes)}`)
        if (!m.url) console.log(`        ⚑ không tải tự động được: ${m.huongDan}`)
      }
      console.log(`      → ${meta.hong}`)
    }
    for (const m of lech) {
      console.log(`      ⚠ ${m.dich} — có nhưng lệch kích thước tham chiếu`)
      console.log(`        (${co(m.thuc)} so với ${co(m.bytes)}) — có thể là bản mới, hoặc tải dở.`)
    }
    if (meta.ghiChu) console.log(`      ℹ ${meta.ghiChu}`)
    console.log('')
  }

  console.log(`  Tổng: ${soDu}/${soTong} file đủ.`)
  if (tongThieuBytes > 0) console.log(`  Còn thiếu ~${co(tongThieuBytes)}.`)

  if (thieuBatBuoc > 0) {
    console.log('\n  ❌ Thiếu model BẮT BUỘC — LIVA sẽ khởi động nhưng không dùng được.')
    console.log('     Chạy:  npm run setup:models')
    process.exit(1)
  }
  console.log('\n  ✅ Đủ model bắt buộc.')
  const conThieu = MANIFEST.some((m) => soKichThuoc(m, llmDir).trangThai === 'thieu')
  if (conThieu) console.log('     Muốn đủ cả tính năng tuỳ chọn:  npm run setup:models -- --profile full')
}

// ---------------------------------------------------------------------------
// fetch
// ---------------------------------------------------------------------------

/**
 * Tải một file, có RESUME và RETRY.
 *
 * Vì sao phải stream chứ không `await res.arrayBuffer()` như
 * `fetch-embedding-model.mjs`: file lớn nhất ở đây là 1,03 GB (và nếu ai bật
 * parakeet thì 2,4 GB). Nạp trọn vào Buffer là vượt heap mặc định của Node và
 * chạm trần Buffer — script sẽ chết đúng ở file quan trọng nhất.
 *
 * Vì sao phải resume: đối tượng dùng script này tải vài GB qua mạng gia đình.
 * Rớt ở phút thứ 20 mà phải tải lại từ đầu thì script coi như vô dụng.
 */
const xoaDongTienTrinh = () => {
  if (process.stdout.isTTY) process.stdout.write('\r' + ' '.repeat(60) + '\r')
}

/** SHA-256 của một file đã có trên đĩa, đọc theo dòng. */
function bamFile(p) {
  return new Promise((giai, tu) => {
    const h = crypto.createHash('sha256')
    fs.createReadStream(p)
      .on('data', (c) => h.update(c))
      .on('error', tu)
      .on('end', () => giai(h.digest('hex')))
  })
}

/**
 * Lỗi hash — phải nói cả hai phía. Người nhận cần phân biệt "tải dở" với "file
 * này không phải file dự án công bố", và chỉ một con số thì không phân biệt được.
 */
const loiHash = (mongDoi, thuc) =>
  `SHA-256 KHÔNG khớp — file nhận được không phải file LIVA công bố.\n` +
  `        mong đợi ${mongDoi}\n        nhận     ${thuc}\n` +
  `        File tạm đã bị xoá. Nếu chạy lại vẫn lệch, ĐỪNG dùng nó.`

async function taiMotFile(m, dich, { soLan = 3 } = {}) {
  const tam = dich + '.dangtai'
  fs.mkdirSync(path.dirname(dich), { recursive: true })

  for (let lan = 1; lan <= soLan; lan++) {
    const daCo = fs.existsSync(tam) ? fs.statSync(tam).size : 0
    let hashLech = false
    try {
      const res = await fetch(m.url, {
        redirect: 'follow',
        headers: daCo > 0 ? { Range: `bytes=${daCo}-` } : {},
      })

      // 416 = server nói "không còn byte nào sau vị trí đó" ⇒ phần tạm đã đủ.
      // Vẫn phải băm rồi mới nhận: "đủ số byte" không phải bằng chứng nội dung.
      if (res.status === 416 && daCo > 0) {
        const thuc = await bamFile(tam)
        if (thuc !== m.sha256) {
          hashLech = true
          throw new Error(loiHash(m.sha256, thuc))
        }
        fs.renameSync(tam, dich)
        return { ok: true, bytes: daCo, tiepTuc: true }
      }
      if (!res.ok) throw new Error(`HTTP ${res.status} ${res.statusText}`)

      // 206 = chấp nhận Range ⇒ ghi tiếp. 200 = không chấp nhận ⇒ làm lại từ đầu.
      const tiepTuc = res.status === 206 && daCo > 0
      const conLai = Number(res.headers.get('content-length') || 0)
      const tong = tiepTuc ? daCo + conLai : conLai

      // Thanh tiến trình chỉ vẽ khi ra terminal thật. Khi bị pipe vào file hay
      // vào log CI, ký tự `\r` không xoá được gì — nó chỉ nằm lại trong log và
      // làm mọi dòng dính vào nhau.
      let daGhi = tiepTuc ? daCo : 0
      let mocIn = Date.now()

      // Băm TOÀN BỘ file, kể cả phần đã tải từ lần trước. Chỉ băm phần đuôi thì
      // cổng hash mất tác dụng đúng ở lượt tải nối tiếp — tức đúng lúc file đã
      // qua tay nhiều kết nối nhất.
      const hash = crypto.createHash('sha256')
      if (tiepTuc) {
        await new Promise((giai, tu) => {
          fs.createReadStream(tam)
            .on('data', (c) => hash.update(c))
            .on('error', tu)
            .on('end', giai)
        })
      }

      const dem = new Transform({
        transform(chunk, _enc, cb) {
          hash.update(chunk)
          daGhi += chunk.length
          if (process.stdout.isTTY && Date.now() - mocIn > 400) {
            mocIn = Date.now()
            const pt = tong ? ((daGhi / tong) * 100).toFixed(1) + '%' : co(daGhi)
            process.stdout.write(`\r      ${pt.padStart(7)}  ${co(daGhi)}${tong ? ' / ' + co(tong) : ''}   `)
          }
          cb(null, chunk)
        },
      })

      await pipeline(
        Readable.fromWeb(res.body),
        dem,
        fs.createWriteStream(tam, { flags: tiepTuc ? 'a' : 'w' }),
      )
      xoaDongTienTrinh()

      // Kiểm TRƯỚC khi đổi tên: file ở đường dẫn thật là file mà llama.cpp và
      // ONNX Runtime sẽ mở, nên không có gì chưa kiểm được phép tới đó.
      const thuc = hash.digest('hex')
      if (thuc !== m.sha256) {
        hashLech = true
        throw new Error(loiHash(m.sha256, thuc))
      }

      fs.renameSync(tam, dich)
      return { ok: true, bytes: fs.statSync(dich).size, tiepTuc }
    } catch (e) {
      xoaDongTienTrinh()
      const conThu = lan < soLan
      console.log(`      ✗ lần ${lan}/${soLan}: ${e.message}${conThu ? ' — thử lại…' : ''}`)
      // Hash lệch ⇒ XOÁ file tạm. Giữ lại một file đã sai nội dung rồi `Range:`
      // ghi tiếp lên nó là cách chắc chắn để mọi lần thử sau đều lệch vì đúng
      // cái lý do cũ. Lỗi mạng thì ngược lại: giữ để nối tiếp.
      if (hashLech) {
        try {
          fs.unlinkSync(tam)
        } catch {
          /* không có file tạm thì thôi */
        }
      }
      if (conThu) await new Promise((r) => setTimeout(r, lan * 2000))
      else return { ok: false, loi: e.message }
    }
  }
  return { ok: false, loi: 'hết lượt thử' }
}

/** Có file nào đích nằm dưới thư mục LLM không (để chỉ mkdir khi thật sự cần). */
const canThuMucLlm = (ds) => ds.some((m) => m.llm)

async function fetchModels({ profile, only, argLlmDir, dryRun, force }) {
  const { duongDan: llmDir, nguon } = thuMucLlm(argLlmDir)
  let ds = locTheoProfile(profile)
  if (only.length) ds = ds.filter((m) => only.includes(m.nhom))

  // Chỉ tải cái THIẾU hoặc HỎNG. File "lệch kích thước" thì để yên: nguồn ở
  // nhánh master có thể ra bản mới hợp lệ, và một `setup:models` tải lại vài
  // trăm MB mỗi lần chạy sẽ khiến người ta thôi không chạy nó nữa. Muốn ép về
  // đúng kích thước tham chiếu thì `--force`.
  const canTaiLai = force ? ['thieu', 'hong', 'lech'] : ['thieu', 'hong']
  const canTai = ds.filter((m) => canTaiLai.includes(soKichThuoc(m, llmDir).trangThai))
  const boQuaLech = ds.filter((m) => soKichThuoc(m, llmDir).trangThai === 'lech')
  const taiDuoc = canTai.filter((m) => m.url)
  const khongTaiDuoc = canTai.filter((m) => !m.url)
  const tongBytes = taiDuoc.reduce((s, m) => s + m.bytes, 0)

  console.log(`\nLIVA setup:models — profile "${profile}"\n`)
  console.log(`  Thư mục LLM : ${llmDir}`)
  console.log(`                (${nguon})`)
  console.log(`  Cần tải     : ${taiDuoc.length} file, ~${co(tongBytes)}`)
  if (khongTaiDuoc.length) {
    console.log(`  Bỏ qua      : ${khongTaiDuoc.length} file không có nguồn công khai`)
  }
  if (!force && boQuaLech.length) {
    console.log(`  Giữ nguyên  : ${boQuaLech.length} file có sẵn nhưng lệch kích thước (ép tải lại bằng --force)`)
  }
  console.log('')

  if (dryRun) {
    for (const m of taiDuoc) console.log(`  ↓ ${m.dich.padEnd(56)} ${co(m.bytes).padStart(9)}`)
    for (const m of khongTaiDuoc) console.log(`  ⚑ ${m.dich.padEnd(56)} ${'(tự chuẩn bị)'.padStart(9)}`)
    console.log('\n  (--dry-run: không tải gì.)')
    return
  }

  if (canThuMucLlm(taiDuoc)) fs.mkdirSync(llmDir, { recursive: true })

  let thanhCong = 0
  const hong = []
  for (const [i, m] of taiDuoc.entries()) {
    const dich = duongDanThat(m, llmDir)
    console.log(`  [${i + 1}/${taiDuoc.length}] ${m.dich}  (~${co(m.bytes)})`)
    const kq = await taiMotFile(m, dich)
    if (kq.ok) {
      thanhCong++
      const lech = m.bytes && kq.bytes !== m.bytes ? `  ⚠ lệch tham chiếu ${co(m.bytes)}` : ''
      console.log(`      ✓ ${co(kq.bytes)}${kq.tiepTuc ? ' (tải tiếp)' : ''}${lech}`)
    } else {
      hong.push({ m, loi: kq.loi })
      console.log(`      ✗ ${kq.loi}`)
    }
  }

  console.log('')
  if (khongTaiDuoc.length) {
    console.log('  Phải tự chuẩn bị (không có nguồn tải công khai):')
    for (const m of khongTaiDuoc) console.log(`    · ${m.dich} — ${m.huongDan}`)
    console.log('')
  }

  if (hong.length) {
    console.log(`  ❌ ${hong.length}/${taiDuoc.length} file tải hỏng:`)
    for (const { m, loi } of hong) {
      console.log(`    · ${m.dich} — ${loi}`)
      console.log(`      ${m.url}`)
    }
    console.log('\n  Chạy lại lệnh này — phần đã tải được giữ lại và tải tiếp, không mất từ đầu.')
    process.exit(1)
  }

  console.log(
    taiDuoc.length === 0
      ? '  ✅ Không có gì phải tải — đã đủ. Kiểm lại:  npm run doctor'
      : `  ✅ ${thanhCong}/${taiDuoc.length} file xong. Kiểm lại:  npm run doctor`,
  )
}

// ---------------------------------------------------------------------------
// CLI
// ---------------------------------------------------------------------------

function docThamSo(argv) {
  const lay = (ten) => {
    const i = argv.indexOf(ten)
    return i > -1 ? argv[i + 1] : null
  }
  const lenh = argv.find((a) => a === 'doctor' || a === 'fetch') || 'doctor'
  const profile = lay('--profile') === 'full' || argv.includes('full') ? 'full' : 'minimal'
  const only = (lay('--only') || '').split(',').map((s) => s.trim()).filter(Boolean)
  return {
    lenh,
    profile,
    only,
    argLlmDir: lay('--llm-dir'),
    dryRun: argv.includes('--dry-run'),
    force: argv.includes('--force'),
  }
}

const ts = docThamSo(process.argv.slice(2))

for (const k of ts.only) {
  if (!NHOM[k]) {
    console.error(`--only: không có nhóm "${k}". Nhóm hợp lệ: ${Object.keys(NHOM).join(', ')}`)
    process.exit(2)
  }
}

const chay = ts.lenh === 'fetch' ? fetchModels(ts) : Promise.resolve(doctor(ts))
chay.catch((e) => {
  console.error('\nLỖI:', e.message)
  process.exit(1)
})
