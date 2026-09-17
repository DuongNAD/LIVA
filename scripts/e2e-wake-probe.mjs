/**
 * e2e-wake-probe.mjs — kiểm cổng đánh thức `OP_WAKE_PROBE` qua WebSocket THẬT.
 *
 * Gửi một file WAV lên core (tự hạ mẫu về 16 kHz nếu cần) và in ra phán quyết
 * kèm transcript core nghe được. Đây là đường DUY NHẤT kiểm được toàn bộ chuỗi
 * widget → wire → classifier/STT → so cụm từ; `cargo test` chỉ chạm phần so
 * chuỗi, không bao giờ đi qua socket hay STT thật.
 *
 * Thoát: 0 = đánh thức, 2 = từ chối, 1 = lỗi. Dùng WebSocket có sẵn của
 * Node 22 nên không cần package `ws`.
 *
 *   # Terminal 1 — giữ stdin MỞ (core đọc stdin cho IPC, EOF là nó thoát)
 *   $env:LIVA_SERVER_PORT="8099"; $env:LIVA_DB_IN_MEMORY="1"
 *   .\target\debug\liva-native-core.exe
 *
 *   # Terminal 2
 *   node scripts/e2e-wake-probe.mjs <wav-path> 8099
 *
 * Chưa có clip? Sinh bằng chính TTS của LIVA rồi hạ mẫu:
 *   .\target\debug\tts_piper_probe.exe models/piper/vi_VN-vais1000-medium.onnx `
 *     "Hey Liva, bật nhạc lên giúp tôi" out.wav
 *   ffmpeg -i out.wav -ar 16000 -ac 1 out16k.wav
 *
 * Kiểm 27/07/2026 (giọng Piper tổng hợp, Nemotron thật, 3/3 đúng):
 *   "Hey Liva, bật nhạc lên giúp tôi"               → đánh thức
 *   "Hôm nay trời đẹp quá, đi ăn cơm không"         → từ chối
 *   "Hey Liva, what is the weather today in Hanoi"  → đánh thức
 */
import fs from 'node:fs';

const OP_WAKE_PROBE = 0x05;
const wavPath = process.argv[2];
const port = process.argv[3] || process.env.PORT || '8099';
const timeoutMs = parseInt(process.env.PROBE_TIMEOUT_MS || '15000', 10);

if (!wavPath) {
  console.error('Cách dùng: node scripts/e2e-wake-probe.mjs <wav-path> [port]');
  process.exit(1);
}
if (!fs.existsSync(wavPath)) {
  console.error(`  ✗ LỖI: Không tìm thấy file WAV: ${wavPath}`);
  process.exit(1);
}

function readWavMono16k(path) {
  const buf = fs.readFileSync(path);
  if (buf.toString('ascii', 0, 4) !== 'RIFF') throw new Error('không phải RIFF');

  let pos = 12;
  let fmt = null;
  let data = null;
  while (pos + 8 <= buf.length) {
    const id = buf.toString('ascii', pos, pos + 4);
    const size = buf.readUInt32LE(pos + 4);
    const body = buf.subarray(pos + 8, pos + 8 + size);
    if (id === 'fmt ') {
      fmt = {
        format: body.readUInt16LE(0),
        channels: body.readUInt16LE(2),
        rate: body.readUInt32LE(4),
        bits: body.readUInt16LE(14),
      };
    } else if (id === 'data') {
      data = body;
    }
    pos += 8 + size + (size % 2);
  }
  if (!fmt || !data) throw new Error('thiếu chunk fmt/data');
  console.log(`  WAV: ${fmt.rate} Hz, ${fmt.channels} kênh, ${fmt.bits} bit`);

  const n = Math.floor(data.length / 2 / fmt.channels);
  const out = new Float32Array(n);
  for (let i = 0; i < n; i++) {
    out[i] = data.readInt16LE(i * 2 * fmt.channels) / 32768;
  }
  return { samples: out, rate: fmt.rate };
}

function frame(op, seq, payloadBytes) {
  const head = Buffer.alloc(9);
  head.writeUInt8(op, 0);
  head.writeUInt32LE(seq >>> 0, 1);
  head.writeUInt32LE(payloadBytes.length, 5);
  return Buffer.concat([head, payloadBytes]);
}

/** Nội suy tuyến tính về 16 kHz — đủ cho mục đích kiểm đường dây. */
function resampleTo16k(input, fromRate) {
  if (fromRate === 16000) return input;
  const ratio = fromRate / 16000;
  const out = new Float32Array(Math.floor(input.length / ratio));
  for (let i = 0; i < out.length; i++) {
    const src = i * ratio;
    const i0 = Math.floor(src);
    const i1 = Math.min(i0 + 1, input.length - 1);
    const frac = src - i0;
    out[i] = input[i0] * (1 - frac) + input[i1] * frac;
  }
  return out;
}

const { samples: raw, rate } = readWavMono16k(wavPath);
let samples = resampleTo16k(raw, rate);
if (rate !== 16000) console.log(`  Hạ mẫu ${rate} → 16000 Hz`);

// Calibrate audio padding to match real client widget (LivaWakeWorker pre-roll ring buffer)
// and satisfy openWakeWord minimum receptive window (>= 16 embeddings / 1.96s).
const SAMPLE_RATE = 16000;
const MIN_TOTAL_SAMPLES = Math.floor(SAMPLE_RATE * 2.5); // 2.5s (40,000 samples)
const MAX_TOTAL_SAMPLES = Math.floor(SAMPLE_RATE * 3.5); // 3.5s (56,000 samples)
const TRAILING_PAD_SAMPLES = Math.floor(SAMPLE_RATE * 0.30); // 0.30s trailing hangover

// Default nominal leading silence: 1.75s (28,000 samples)
// Ensures short clips (0.8s - 1.2s) receive 1.5s - 2.0s leading silence,
// placing the wake word utterance at the activation tail of the 2.5s - 3.25s receptive field.
let leadingPadSamples = Math.floor(SAMPLE_RATE * 1.75);

// 1. Ensure total clip length is at least 2.5s (WAKE_PAD_TARGET_SAMPLES)
if (leadingPadSamples + samples.length + TRAILING_PAD_SAMPLES < MIN_TOTAL_SAMPLES) {
  leadingPadSamples = MIN_TOTAL_SAMPLES - samples.length - TRAILING_PAD_SAMPLES;
}

// 2. Bound leading pad so total clip does not exceed MAX_TOTAL_SAMPLES (3.5s)
// This prevents truncating speech audio at the end of longer clips.
if (leadingPadSamples + samples.length + TRAILING_PAD_SAMPLES > MAX_TOTAL_SAMPLES) {
  leadingPadSamples = MAX_TOTAL_SAMPLES - samples.length - TRAILING_PAD_SAMPLES;
}

// 3. Keep at least 0.25s leading silence to protect initial consonant ("h" in "hey")
leadingPadSamples = Math.max(Math.floor(SAMPLE_RATE * 0.25), leadingPadSamples);

const leadingPad = new Float32Array(leadingPadSamples);
const trailingPad = new Float32Array(TRAILING_PAD_SAMPLES);
const padded = new Float32Array(leadingPad.length + samples.length + trailingPad.length);
padded.set(leadingPad, 0);
padded.set(samples, leadingPad.length);
padded.set(trailingPad, leadingPad.length + samples.length);
samples = padded;

// Core will silently drop frames if duration is outside [0.3s, 4.0s] (WAKE_PROBE_MIN_SECS..=WAKE_PROBE_MAX_SECS)
const MAX = Math.floor(SAMPLE_RATE * 3.5);
const clip = samples.length > MAX ? samples.subarray(0, MAX) : samples;
const clipDuration = clip.length / SAMPLE_RATE;
if (clipDuration < 0.3 || clipDuration > 4.0) {
  console.error(`  ✗ LỖI: Độ dài clip (${clipDuration.toFixed(2)}s) ngoài phạm vi hợp lệ [0.3s, 4.0s]. Core sẽ bỏ qua.`);
  process.exit(1);
}
console.log(
  `  Gửi ${clipDuration.toFixed(2)} s audio (${clip.length} mẫu: ` +
  `${(leadingPad.length / SAMPLE_RATE).toFixed(2)}s pre-roll + ` +
  `${(raw.length / rate).toFixed(2)}s speech + ` +
  `${(trailingPad.length / SAMPLE_RATE).toFixed(2)}s post-roll)`
);

const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
let done = false;

const timer = setTimeout(() => {
  if (!done) {
    console.log(`  ✗ HẾT GIỜ — core không trả lời trong ${(timeoutMs / 1000).toFixed(0)} s`);
    try { ws.close(); } catch {}
    process.exit(1);
  }
}, timeoutMs);

ws.addEventListener('open', () => {
  const bytes = Buffer.from(clip.buffer, clip.byteOffset, clip.byteLength);
  ws.send(frame(OP_WAKE_PROBE, 1, bytes));
});

ws.addEventListener('message', (ev) => {
  if (typeof ev.data !== 'string') return;
  let msg;
  try {
    msg = JSON.parse(ev.data);
  } catch {
    return;
  }
  if (msg.event !== 'wake_word_triggered' && msg.event !== 'wake_probe_rejected') return;

  done = true;
  clearTimeout(timer);
  const woke = msg.event === 'wake_word_triggered';
  const tier = msg.payload?.tier ?? 'unknown';
  const score = msg.payload?.score !== null && msg.payload?.score !== undefined
    ? Number(msg.payload.score).toFixed(3)
    : 'N/A';
  const transcript = msg.payload?.transcript ?? '';

  console.log(`  → ${woke ? '✓ ĐÁNH THỨC' : '✗ TỪ CHỐI'}  (${msg.event})`);
  console.log(`  → tầng quyết định : ${tier} | điểm classifier: ${score}`);
  if (transcript) {
    console.log(`  → STT nghe ra     : ${JSON.stringify(transcript)}`);
  } else if (tier === 'classifier' && woke) {
    console.log(`  → STT nghe ra     : (bỏ qua STT vì classifier tầng 1 đã xác nhận)`);
  } else {
    console.log(`  → STT nghe ra     : "" (không nhận dạng được từ khóa)`);
  }

  try { ws.close(); } catch {}
  process.exit(woke ? 0 : 2);
});

ws.addEventListener('error', (e) => {
  console.log('  ✗ lỗi socket:', e.message ?? e);
  process.exit(1);
});
