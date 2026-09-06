// Test cho scripts/voice-turn-latency.mjs — bộ gom mốc lượt thoại (VC-4a).
//
// Fixture là log mẫu CỐ ĐỊNH với số liệu đã tính tay trước: 3 lượt đủ mốc,
// 2 lượt thiếu mốc (bị loại và phải được liệt kê), 1 dòng mốc lặp, vài dòng
// nhiễu. p50/p85/p95 kỳ vọng ghi thẳng trong assert — sửa parser làm trôi số
// là test đỏ.
import test from 'node:test'
import assert from 'node:assert/strict'

import {
  collectTurns,
  decompose,
  parseMilestoneLine,
  percentile,
  renderReport,
  REQUIRED_MILESTONES,
  summarize,
} from './voice-turn-latency.mjs'

const NOISE = [
  '2026-08-25T10:00:00.123456Z  INFO liva_native_core::webrtc::session: Voice runtime ready',
  '2026-08-25T10:00:01.000000Z  INFO liva_native_core::webrtc::pipeline: 🎙️ [VAD] Speech END detected. Processing audio...',
  '2026-08-25T10:00:02.000000Z DEBUG liva_native_core::webrtc::vad: frame prob=0.91',
]

function milestone(epoch, milestone_, elapsedMs) {
  return (
    `2026-08-25T10:00:03.${String(epoch).padStart(3, '0')}000Z  INFO ` +
    `liva_native_core::webrtc::pipeline: Voice turn milestone ` +
    `turn_epoch=${epoch} milestone="${milestone_}" elapsed_ms=${elapsedMs}`
  )
}

// ── Dòng log CÓ ANSI — chặn lớp lỗi "fixture sạch mà log thật bẩn" ──────────
//
// KHÔNG thể bắt dòng mốc từ binary thật trên máy này (`models/` trống → không
// có lượt thoại), nên dòng dưới đây DỰNG TAY theo đúng khuôn của
// tracing-subscriber 0.3.23, `fmt/format/mod.rs:1332–1338`: tên trường bọc
// italic `\x1b[3m…\x1b[0m`, dấu '=' bọc dimmed `\x1b[2m…\x1b[0m` — HAI lần
// paint riêng. Tiền tố dòng (timestamp/target) chép từ log thật của
// `./target/debug/liva-native-core` đo 25/08/2026 (19/40 dòng có ESC dù đổ ra
// file). Nếu subscriber đổi khuôn, sửa helper này theo nguồn đó.
const ESC = '\x1b'
const ITAL = (s) => `${ESC}[3m${s}${ESC}[0m`
const DIM = (s) => `${ESC}[2m${s}${ESC}[0m`
const EQ = DIM('=')

function ansiMilestone(epoch, milestone_, elapsedMs) {
  return (
    `${DIM('2026-08-25T10:00:06.000000Z')} ${ESC}[32m INFO${ESC}[0m ` +
    `${DIM('liva_native_core::webrtc::pipeline')}${DIM(':')} ` +
    `Voice turn milestone ${ITAL('turn_epoch')}${EQ}${epoch} ` +
    `${ITAL('milestone')}${EQ}"${milestone_}" ` +
    `${ITAL('elapsed_ms')}${EQ}${elapsedMs}`
  )
}

// Log mẫu: lượt 1–3 đủ; lượt 6 đủ nhưng TOÀN BỘ dòng có ANSI (như log thật);
// lượt 4 thiếu first_speaker_frame; lượt 5 chỉ có vad_end;
// epoch 2 có một dòng stt_done lặp (giá trị đầu tiên phải thắng); một dòng mốc
// tên lạ phải bị bỏ qua.
const FIXTURE_LINES = [
  ...NOISE,
  milestone(1, 'vad_end', 0),
  milestone(1, 'stt_done', 120),
  milestone(2, 'vad_end', 12),
  // Dòng lạ — không phải mốc đã biết, parser phải bỏ.
  '2026-08-25T10:00:04.000Z  INFO x: Voice turn milestone turn_epoch=99 milestone="mystery" elapsed_ms=5',
  milestone(1, 'first_token', 450),
  milestone(2, 'stt_done', 98),
  milestone(1, 'first_speaker_frame', 610),
  milestone(2, 'first_token', 390),
  milestone(2, 'stt_done', 999), // LẶP — bị bỏ, giữ 98
  milestone(2, 'first_speaker_frame', 520),
  milestone(3, 'vad_end', 0),
  milestone(3, 'stt_done', 140),
  milestone(3, 'first_token', 500),
  milestone(3, 'first_speaker_frame', 700),
  // Lượt 2 và 6 có vad_end KHÁC 0 — như log thật: elapsed_ms của vad_end là chi
  // phí huỷ lượt cũ + flush, không bao giờ đúng 0 ngoài unit test. Không có dòng
  // này thì phép trừ vad_end trong decompose/summarize biến mất mà test vẫn xanh.
  ansiMilestone(6, 'vad_end', 35),
  ansiMilestone(6, 'stt_done', 130),
  ansiMilestone(6, 'first_token', 480),
  ansiMilestone(6, 'first_speaker_frame', 800),
  milestone(4, 'vad_end', 0),
  milestone(4, 'stt_done', 110),
  milestone(4, 'first_token', 420),
  milestone(5, 'vad_end', 0),
]

const ENTRIES = FIXTURE_LINES.map(parseMilestoneLine).filter(Boolean)

test('parser đọc đúng ba trường có cấu trúc của dòng mốc', () => {
  const parsed = parseMilestoneLine(milestone(7, 'first_token', 412))
  assert.deepEqual(parsed, {
    epoch: 7,
    milestone: 'first_token',
    elapsedMs: 412,
  })
})

test('parser bỏ dòng nhiễu và mốc tên lạ', () => {
  for (const line of NOISE) {
    assert.equal(parseMilestoneLine(line), null)
  }
  assert.equal(
    parseMilestoneLine(
      'x: Voice turn milestone turn_epoch=99 milestone="mystery" elapsed_ms=5',
    ),
    null,
  )
})

test('gom lượt: 4 lượt đủ, 2 lượt bị loại kèm tên mốc thiếu, 1 bản sao', () => {
  const { turns, rejected, duplicateCount } = collectTurns(ENTRIES)

  assert.deepEqual(
    turns.map((t) => t.epoch),
    [1, 2, 3, 6],
  )
  assert.deepEqual(rejected, [
    { epoch: 4, missing: ['first_speaker_frame'] },
    {
      epoch: 5,
      missing: ['stt_done', 'first_token', 'first_speaker_frame'],
    },
  ])
  assert.equal(duplicateCount, 1)
  // Giá trị ĐẦU tiên thắng: stt_done của epoch 2 là 98, không phải 999.
  assert.equal(turns.find((t) => t.epoch === 2).ms.stt_done, 98)
})

// Test hồi quy cho lớp lỗi "fixture sạch mà log thật bẩn": dòng mốc thật luôn
// có mã màu ANSI (tracing-subscriber tô tên trường italic, dấu '=' dimmed —
// hai lần paint riêng), kể cả khi đổ ra file. Bản parser TRƯỚC khi lột ANSI
// làm test này ĐỎ — đúng ý đồ.
test('dòng log THẬT có ANSI vẫn parse đúng', () => {
  const parsed = parseMilestoneLine(ansiMilestone(7, 'first_token', 412))
  assert.deepEqual(parsed, {
    epoch: 7,
    milestone: 'first_token',
    elapsedMs: 412,
  })
  // Cả một lượt ghép từ toàn bộ dòng ANSI phải đủ bốn mốc.
  const ansiTurn = [
    ansiMilestone(8, 'vad_end', 0),
    ansiMilestone(8, 'stt_done', 100),
    ansiMilestone(8, 'first_token', 300),
    ansiMilestone(8, 'first_speaker_frame', 450),
  ]
    .map(parseMilestoneLine)
    .filter(Boolean)
  const { turns, rejected } = collectTurns(ansiTurn)
  assert.equal(turns.length, 1)
  assert.equal(turns[0].epoch, 8)
  assert.equal(turns[0].ms.first_speaker_frame, 450)
  assert.equal(rejected.length, 0)
})

test('percentile nearest-rank khớp khuôn ttft_bench', () => {
  assert.deepEqual(percentile([], 50), null)
  assert.deepEqual(percentile([100, 200, 300], 50), 200) // ceil(0.5*3)=2
  assert.deepEqual(percentile([100, 200, 300], 95), 300) // ceil(0.95*3)=3 → max khi n nhỏ
  assert.deepEqual(percentile([42], 50), 42)
  assert.deepEqual(percentile([42], 95), 42)
})

// Phân rã từng lượt phải ra ĐÚNG số tính tay — đây là chỗ bắt phép trừ vad_end:
// nếu sttMs bỏ quãng huỷ/flush thì lượt 2 (12→86) và lượt 6 (35→95) lệch ngay.
test('decompose phân rã đúng từng lượt, gồm lượt có vad_end khác 0', () => {
  const { turns } = collectTurns(ENTRIES)
  const byEpoch = Object.fromEntries(turns.map((t) => [t.epoch, t]))

  assert.deepEqual(decompose(byEpoch[1]), {
    cancelFlushMs: 0,
    sttMs: 120,
    llmFirstTokenMs: 330,
    ttsToAudioMs: 160,
    turnLatencyMs: 610,
  })
  assert.deepEqual(decompose(byEpoch[2]), {
    cancelFlushMs: 12,
    sttMs: 86, // 98 − 12: nếu trừ sai/vơi, số này lệch đầu tiên
    llmFirstTokenMs: 292,
    ttsToAudioMs: 130,
    turnLatencyMs: 520,
  })
  assert.deepEqual(decompose(byEpoch[6]), {
    cancelFlushMs: 35,
    sttMs: 95, // 130 − 35
    llmFirstTokenMs: 350,
    ttsToAudioMs: 320,
    turnLatencyMs: 800,
  })
})

test('summarize ra đúng p50/p95 đã tính tay trên fixture', () => {
  const { turns } = collectTurns(ENTRIES)
  const summary = summarize(turns)

  assert.equal(summary.n, 4)
  // Turn latency == TTFA == first_speaker_frame: [610, 520, 700, 800]
  assert.equal(summary.turnLatencyMs.p50, 610) // ceil(0.5*4)=2 → giá trị thứ 2
  assert.equal(summary.turnLatencyMs.p95, 800) // ceil(0.95*4)=4 → max khi n nhỏ
  assert.equal(summary.turnLatencyMs.max, 800)
  // Huỷ/flush: elapsed_ms của vad_end — [0, 12, 0, 35]
  assert.equal(summary.cancelFlushMs.p50, 0)
  assert.equal(summary.cancelFlushMs.p95, 35)
  assert.equal(summary.cancelFlushMs.max, 35)
  // STT (stt_done − vad_end): [120, 86, 140, 95] — KHÔNG phải pick(stt_done)
  // [120, 98, 140, 130]: p50 của bản đột biến là 120 ≠ 95 → test đỏ.
  assert.equal(summary.sttMs.p50, 95)
  assert.equal(summary.sttMs.p95, 140)
  // LLM tới token đầu: [330, 292, 360, 350]
  assert.equal(summary.llmFirstTokenMs.p50, 330)
  assert.equal(summary.llmFirstTokenMs.p95, 360)
  // TTS tới frame loa: [160, 130, 200, 320]
  assert.equal(summary.ttsToAudioMs.p50, 160)
  assert.equal(summary.ttsToAudioMs.p95, 320)
  for (const name of REQUIRED_MILESTONES) {
    assert.equal(summary.milestones[name].n, 4)
  }
})

test('báo cáo nói ra lượt bị loại, bản sao, và cảnh báo n<20', () => {
  const collected = collectTurns(ENTRIES)
  const summary = summarize(collected.turns)
  const report = renderReport(summary, collected).join('\n')

  assert.match(report, /n = 4 < 20/)
  assert.match(report, /Loại 2 lượt thiếu mốc/)
  assert.match(report, /epoch 4: thiếu first_speaker_frame/)
  assert.match(report, /1 dòng mốc lặp đã bỏ/)
  assert.match(report, /Turn latency \/ TTFA/)
  assert.match(report, /Huỷ lượt cũ \+ flush/)
})
