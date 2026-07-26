#!/usr/bin/env node
// Kiểm tra và tải model cho LIVA — `doctor` (chẩn đoán) và `fetch` (tải).
//
// Vì sao cần script này: weight bị gitignore, nên `git clone` xong LIVA **không
// chạy được**. `models/README.md` đã ghi đủ nguồn tải, nhưng đó là *hướng dẫn
// thủ công* cho ~26 file / ~3,7 GB trải trên 6 nguồn khác nhau — không ai ngoài
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
// `nhom` gom file theo KHẢ NĂNG, không theo nguồn tải — vì câu hỏi người dùng
// hỏi là "sao nó không nghe được", chứ không phải "file nào của HuggingFace".
// `bytes` là kích thước tham chiếu đo trên máy dev đang chạy được.
// `llm: true` nghĩa là file nằm dưới thư mục LLM (config `ai.localModelsDir`),
// không phải dưới `models/`.
// `url: null` = KHÔNG tải tự động được (tự export/tự train) — `doctor` vẫn báo
// thiếu, nhưng `fetch` bỏ qua và in hướng dẫn thay vì giả vờ làm được.
// ---------------------------------------------------------------------------

const HF = 'https://huggingface.co'
const NEMOTRON = `${HF}/onnx-community/nemotron-3.5-asr-streaming-0.6b-onnx-int4/resolve/main`
const PIPER = `${HF}/rhasspy/piper-voices/resolve/main`
const QWEN = `${HF}/unsloth/Qwen3-VL-2B-Instruct-GGUF/resolve/main`
const VIENEU = `${HF}/pnnbao-ump/VieNeu-TTS-v3-Turbo/resolve/main`
const MOSS = `${HF}/OpenMOSS-Team/MOSS-Audio-Tokenizer-Nano-ONNX/resolve/main`
const LIVEKIT = 'https://github.com/livekit/rust-sdks/raw/main/livekit-wakeword'

const NHOM = {
  chat: {
    ten: 'Chat (LLM router)',
    batBuoc: true,
    hong: 'Không chat được — lõi chạy nhưng mọi câu hỏi trả lỗi "no model loaded".',
    ghiChu: 'Đường dẫn lấy từ data/liva-config.json → ai.localModelsDir + ai.routerModel.',
  },
  vision: {
    ten: 'Nhìn màn hình (vision:ask)',
    batBuoc: false,
    hong: 'vision:ask báo lỗi lúc gọi. Chụp màn hình + diff vùng vẫn chạy.',
    ghiChu: 'Cần build RELEASE mới chạy được (debug bung assert CRT-mix).',
  },
  stt: {
    ten: 'Nghe (STT Nemotron)',
    batBuoc: true,
    hong: 'Không nghe được — mọi lệnh voice:stt_* trả lỗi.',
    ghiChu: 'Override thư mục: LIVA_STT_MODEL_DIR.',
  },
  'tts-vi': {
    ten: 'Nói tiếng Việt (Piper)',
    batBuoc: true,
    hong: 'Không có giọng tiếng Việt.',
    ghiChu: 'Override thư mục: LIVA_TTS_PIPER_DIR.',
  },
  'tts-en': {
    ten: 'Nói tiếng Anh (Piper)',
    batBuoc: false,
    hong: 'Không có giọng tiếng Anh; tiếng Việt vẫn nói được.',
  },
  'tts-premium': {
    ten: 'Giọng đẹp VieNeu (opt-in)',
    batBuoc: false,
    hong: 'LIVA_TTS_VIENEU=1 sẽ log lỗi rồi rơi xuống Piper.',
    ghiChu: 'Tự hồi quy, RTF ~1,75 trên CPU — chậm hơn realtime, dùng như tier chất lượng.',
  },
  rag: {
    ten: 'Bộ nhớ dài hạn (RAG)',
    batBuoc: true,
    hong: 'LIVA KHÔNG NHỚ GÌ. Không có lỗi, chỉ log WARN "Bo nho dai han TAT" lúc boot.',
    ghiChu: 'Override: LIVA_EMBEDDING_MODEL_DIR · số ký ức mỗi lượt: LIVA_RAG_TOP_K.',
  },
  vad: {
    ten: 'Cắt lượt nói (VAD)',
    batBuoc: true,
    hong: 'Không biết người dùng nói xong lúc nào — thoại full-duplex hỏng.',
    ghiChu: 'Override: LIVA_VAD_MODEL_PATH · tắt: LIVA_VAD_ENABLED=0.',
  },
  denoise: {
    ten: 'Khử ồn trước VAD (GTCRN)',
    batBuoc: false,
    hong: 'Chạy không khử ồn — STT kém hơn trong phòng ồn, không lỗi.',
    ghiChu: 'Bật mặc định; tắt bằng LIVA_DENOISE_ENABLED=0.',
  },
  'turn-shadow': {
    ten: 'End-of-turn ngữ nghĩa (shadow)',
    batBuoc: false,
    hong: 'Không có gì hỏng — tính năng này tắt mặc định.',
    ghiChu: 'Bật bằng LIVA_TURN_SHADOW_ENABLED=1.',
  },
  wake: {
    ten: 'Wake-word ("hey LIVA")',
    batBuoc: false,
    hong: 'LIVA_WAKE_MODE=trained_model sẽ không chạy. Chế độ asr_prefix vẫn dùng được.',
    ghiChu: 'Với tiếng Việt nên dùng LIVA_WAKE_MODE=asr_prefix — model vi tự train FPPH 19,4, chưa đủ tốt.',
  },
  'stt-vi-hq': {
    ten: 'STT tiếng Việt chất lượng cao (Parakeet)',
    batBuoc: false,
    hong: 'LIVA_STT_VI_ENGINE=parakeet sẽ không chạy; Nemotron vẫn nghe được (WER kém hơn ~3×).',
    ghiChu: 'KHÔNG tải tự động được — phải tự export từ NeMo trong WSL, xem models/README.md.',
  },
}

const MANIFEST = [
  // --- chat: router LLM -----------------------------------------------------
  { nhom: 'chat', profile: 'minimal', llm: true, dich: 'Qwen3-VL-2B-Instruct-GGUF/Qwen3-VL-2B-Instruct-Q4_K_M.gguf', url: `${QWEN}/Qwen3-VL-2B-Instruct-Q4_K_M.gguf`, bytes: 1107410624, chinhXac: true },

  // --- vision: projector cho Qwen3-VL --------------------------------------
  // Q8_0 (445 MB) cũng chạy được và nhẹ hơn, nhưng data/liva-config.json đang
  // trỏ mmproj-F16 nên tải đúng cái config nói, không tự ý đổi sau lưng.
  { nhom: 'vision', profile: 'full', llm: true, dich: 'Qwen3-VL-2B-Instruct-GGUF/mmproj-F16.gguf', url: `${QWEN}/mmproj-F16.gguf`, bytes: 819395232, chinhXac: true },

  // --- stt: Nemotron RNN-T --------------------------------------------------
  // Chỉ 7 file này được mã Rust đọc thật (`stt/engine.rs` mở encoder/decoder/
  // joint, `stt/tokenizer.rs` mở tokenizer.json). Các file *_config.json và
  // vocab.txt trong repo gốc KHÔNG có chỗ nào đọc — không tải cho nhẹ.
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/encoder.onnx', url: `${NEMOTRON}/encoder.onnx`, bytes: 2677548 },
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/encoder.onnx.data', url: `${NEMOTRON}/encoder.onnx.data`, bytes: 690089984 },
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/decoder.onnx', url: `${NEMOTRON}/decoder.onnx`, bytes: 4696 },
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/decoder.onnx.data', url: `${NEMOTRON}/decoder.onnx.data`, bytes: 59785216 },
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/joint.onnx', url: `${NEMOTRON}/joint.onnx`, bytes: 2136 },
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/joint.onnx.data', url: `${NEMOTRON}/joint.onnx.data`, bytes: 37830656 },
  // 642 525 byte là bản GỐC (LF). Máy dev có bản 694 801 vì bị một editor lưu
  // lại thành CRLF kèm đổi tên khoá `pretokenizer` → `pre_tokenizer: null` —
  // đó là lý do `models/nemotron-asr` vĩnh viễn hiện "modified content".
  // Kiểm lại crate `tokenizers` 0.21: visitor bỏ qua khoá lạ (`_ => {}`) và
  // thiếu `pre_tokenizer` thì builder mặc định None ⇒ hai bản cho ra CÙNG một
  // tokenizer. Bản gốc tải về dùng được, không cần vá gì.
  { nhom: 'stt', profile: 'minimal', dich: 'models/nemotron-asr/tokenizer.json', url: `${NEMOTRON}/tokenizer.json`, bytes: 642525 },

  // --- tts ------------------------------------------------------------------
  { nhom: 'tts-vi', profile: 'minimal', dich: 'models/piper/vi_VN-vais1000-medium.onnx', url: `${PIPER}/vi/vi_VN/vais1000/medium/vi_VN-vais1000-medium.onnx`, bytes: 63201294 },
  { nhom: 'tts-en', profile: 'full', dich: 'models/piper/en_US-lessac-medium.onnx', url: `${PIPER}/en/en_US/lessac/medium/en_US-lessac-medium.onnx`, bytes: 63201294 },

  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/vieneu_prefill.onnx', url: `${VIENEU}/onnx_update/vieneu_prefill.onnx`, bytes: 324499 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/vieneu_decode_step.onnx', url: `${VIENEU}/onnx_update/vieneu_decode_step.onnx`, bytes: 306134 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/vieneu_acoustic_cached.onnx', url: `${VIENEU}/onnx_update/vieneu_acoustic_cached.onnx`, bytes: 7207223 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/vieneu_backbone_shared.data', url: `${VIENEU}/onnx_update/vieneu_backbone_shared.data`, bytes: 415319040 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/vieneu_v3_heads.npz', url: `${VIENEU}/onnx_update/vieneu_v3_heads.npz`, bytes: 52219622 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/moss_audio_tokenizer_decode_full.onnx', url: `${MOSS}/moss_audio_tokenizer_decode_full.onnx`, bytes: 681902 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/moss_audio_tokenizer_decode_shared.data', url: `${MOSS}/moss_audio_tokenizer_decode_shared.data`, bytes: 44198912 },
  { nhom: 'tts-premium', profile: 'full', dich: 'models/vieneu/sea_g2p.bin', url: 'https://raw.githubusercontent.com/pnnbao97/sea-g2p/main/python/sea_g2p/sea_g2p.bin', bytes: 50086196 },

  // --- rag ------------------------------------------------------------------
  { nhom: 'rag', profile: 'minimal', dich: 'models/embedding/model.onnx', url: `${HF}/intfloat/multilingual-e5-small/resolve/main/onnx/model.onnx`, bytes: 470268510 },
  { nhom: 'rag', profile: 'minimal', dich: 'models/embedding/tokenizer.json', url: `${HF}/intfloat/multilingual-e5-small/resolve/main/tokenizer.json`, bytes: 17082730 },

  // --- thoại phụ trợ --------------------------------------------------------
  { nhom: 'vad', profile: 'minimal', dich: 'models/silero_vad_v6.onnx', url: 'https://github.com/snakers4/silero-vad/raw/master/src/silero_vad/data/silero_vad.onnx', bytes: 2327524 },
  { nhom: 'denoise', profile: 'minimal', dich: 'models/gtcrn_simple.onnx', url: 'https://github.com/k2-fsa/sherpa-onnx/releases/download/speech-enhancement-models/gtcrn_simple.onnx', bytes: 535638 },
  { nhom: 'turn-shadow', profile: 'full', dich: 'models/smart_turn_v3.2_cpu.onnx', url: `${HF}/pipecat-ai/smart-turn-v3/resolve/main/smart-turn-v3.2-cpu.onnx`, bytes: 8679182 },

  // --- wake-word ------------------------------------------------------------
  { nhom: 'wake', profile: 'full', dich: 'models/wakeword_melspec.onnx', url: `${LIVEKIT}/onnx/melspectrogram.onnx`, bytes: 1087958 },
  { nhom: 'wake', profile: 'full', dich: 'models/wakeword_embedding.onnx', url: `${LIVEKIT}/onnx/embedding_model.onnx`, bytes: 1326578 },
  // Classifier tự train — không có nguồn công khai.
  { nhom: 'wake', profile: 'full', dich: 'models/wake_liva_en.onnx', url: null, bytes: 184477, huongDan: 'Tự train (2026-07-04). Dùng LIVA_WAKE_THRESHOLD=0.77 với model này.' },

  // --- không tải tự động được ----------------------------------------------
  { nhom: 'stt-vi-hq', profile: 'full', dich: 'models/parakeet_vi.onnx', url: null, bytes: 41914813, huongDan: 'Tự export từ huggingface.co/nvidia/parakeet-ctc-0.6b-Vietnamese qua NeMo (WSL).' },
  { nhom: 'stt-vi-hq', profile: 'full', dich: 'models/parakeet_vi.onnx.data', url: null, bytes: 2435002372, huongDan: 'Đi kèm parakeet_vi.onnx, phải cùng thư mục.' },
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

async function taiMotFile(m, dich, { soLan = 3 } = {}) {
  const tam = dich + '.dangtai'
  fs.mkdirSync(path.dirname(dich), { recursive: true })

  for (let lan = 1; lan <= soLan; lan++) {
    const daCo = fs.existsSync(tam) ? fs.statSync(tam).size : 0
    try {
      const res = await fetch(m.url, {
        redirect: 'follow',
        headers: daCo > 0 ? { Range: `bytes=${daCo}-` } : {},
      })

      // 416 = server nói "không còn byte nào sau vị trí đó" ⇒ phần tạm đã đủ.
      if (res.status === 416 && daCo > 0) {
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
      const dem = new Transform({
        transform(chunk, _enc, cb) {
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

      fs.renameSync(tam, dich)
      return { ok: true, bytes: fs.statSync(dich).size, tiepTuc }
    } catch (e) {
      xoaDongTienTrinh()
      const conThu = lan < soLan
      console.log(`      ✗ lần ${lan}/${soLan}: ${e.message}${conThu ? ' — thử lại…' : ''}`)
      // Giữ nguyên .dangtai để lần sau resume; chỉ chờ rồi thử tiếp.
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
  const profile = lay('--profile') === 'full' ? 'full' : 'minimal'
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
