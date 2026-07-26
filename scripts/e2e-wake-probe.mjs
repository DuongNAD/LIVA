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
 *     "Này Liva ơi, bật nhạc lên giúp tôi" out.wav
 *   ffmpeg -i out.wav -ar 16000 -ac 1 out16k.wav
 *
 * Kiểm 27/07/2026 (giọng Piper tổng hợp, Nemotron thật, 3/3 đúng):
 *   "Này Liva ơi, bật nhạc lên giúp tôi"            → đánh thức
 *   "Hôm nay trời đẹp quá, đi ăn cơm không"         → từ chối
 *   "Hey Liva, what is the weather today in Hanoi"  → đánh thức
 */
import fs from 'node:fs';

const OP_WAKE_PROBE = 0x05;
const wavPath = process.argv[2];
const port = process.argv[3] || process.env.PORT || '8099';

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

// Widget luôn gửi kèm pre-roll im lặng; mô phỏng để giống đường thật.
const pad = new Float32Array(16000 * 0.25);
const padded = new Float32Array(pad.length + samples.length + pad.length);
padded.set(samples, pad.length);
samples = padded;

// Core từ chối probe > 4 s trước khi chạy STT — cắt cho lọt cửa.
const MAX = 16000 * 3.5;
const clip = samples.length > MAX ? samples.subarray(0, MAX) : samples;
console.log(`  Gửi ${(clip.length / 16000).toFixed(2)} s audio (${clip.length} mẫu)`);

const ws = new WebSocket(`ws://127.0.0.1:${port}/ws`);
let done = false;

const timer = setTimeout(() => {
  if (!done) {
    console.log('  ✗ HẾT GIỜ — core không trả lời trong 90 s');
    process.exit(1);
  }
}, 90000);

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
  console.log(`  → ${woke ? '✓ ĐÁNH THỨC' : '✗ TỪ CHỐI'}  (${msg.event})`);
  console.log(`  → core nghe ra: ${JSON.stringify(msg.payload?.transcript ?? '')}`);
  ws.close();
  process.exit(woke ? 0 : 2);
});

ws.addEventListener('error', (e) => {
  console.log('  ✗ lỗi socket:', e.message ?? e);
  process.exit(1);
});
