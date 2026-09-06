#!/usr/bin/env node
// voice-turn-latency — bộ gom p50/p95 cho bốn mốc lượt thoại (VC-4a).
//
// Đọc log `RUST_LOG` của gateway, tách các dòng mốc do
// `liva-native-core/src/webrtc/pipeline.rs#TurnMilestone::fire` phát ra:
//
//   ... Voice turn milestone turn_epoch=7 milestone="first_token" elapsed_ms=412
//
// gom theo `turn_epoch`, và in p50/p95 cho từng mốc cùng ba khoảng phân rã
// (STT / LLM-first-token / TTS-đến-frame-loa). Turn latency == TTFA ==
// elapsed_ms của `first_speaker_frame` — con số người dùng cảm nhận.
//
// Cách dùng:
//
//   RUST_LOG=... cargo run > gateway.log     # hoặc tee
//   node scripts/voice-turn-latency.mjs gateway.log
//   tail -n 5000 gateway.log | node scripts/voice-turn-latency.mjs
//
// Quy ước giống `ttft_bench`: phân vị nearest-rank, và khi n < 20 thì p95
// CHÍNH LÀ giá trị lớn nhất — chương trình tự in cảnh báo đó thay vì để người
// đọc tự suy ra.
//
// Lượt bị thiếu mốc thì bị LOẠI và được liệt kê thẳng — không âm thầm bỏ qua
// (điều kiện nghiệm thu VC-4a). Nếu một mốc xuất hiện nhiều lần trong một lượt,
// giá trị ĐẦU tiên thắng và số bản sao bị đếm riêng.
import fs from 'node:fs'
import os from 'node:os'

export const REQUIRED_MILESTONES = [
  'vad_end',
  'stt_done',
  'first_token',
  'first_speaker_frame',
]

// Thứ tự trường cố định bởi macro `info!` trong pipeline.rs:
// turn_epoch → milestone → elapsed_ms.
const MILESTONE_LINE_RE =
  /turn_epoch=(\d+)\s+milestone="([a-z_]+)"\s+elapsed_ms=(\d+)/

// tracing-subscriber 0.3.23 (fmt/format/mod.rs:1332–1338) tô TÊN TRƯỜNG bằng
// italic() và dấu '=' bằng dimmed() — HAI lần paint riêng — nên chuỗi
// "turn_epoch=" không bao giờ xuất hiện nguyên vẹn trong log thật, KỂ CẢ khi đổ
// ra file (không chỗ nào gọi .with_ansi(false): main.rs và liva-desktop lib.rs
// đều dùng mặc định). Lột sạch mã màu trước khi khớp; đừng nới regex cho phép
// escape chen giữa — một chỗ lột là đúng chỗ duy nhất.
const ANSI_RE = /\x1b\[[0-9;]*m/g

/** Tách một dòng log thành bản ghi mốc, hoặc `null` nếu không phải dòng mốc. */
export function parseMilestoneLine(line) {
  const match = MILESTONE_LINE_RE.exec(line.replace(ANSI_RE, ''))
  if (!match) return null
  const milestone = match[2]
  if (!REQUIRED_MILESTONES.includes(milestone)) return null
  return {
    epoch: Number(match[1]),
    milestone,
    elapsedMs: Number(match[3]),
  }
}

/**
 * Gom bản ghi mốc theo epoch.
 *
 * Trả `{ turns, rejected, duplicateCount }`:
 * - `turns`: lượt đủ cả bốn mốc, mỗi mốc giữ giá trị đầu tiên gặp.
 * - `rejected`: lượt thiếu mốc, kèm danh sách tên mốc thiếu.
 * - `duplicateCount`: số bản ghi mốc lặp (giá trị sau bị bỏ).
 */
export function collectTurns(entries) {
  const byEpoch = new Map()
  let duplicateCount = 0
  for (const entry of entries) {
    const turn = byEpoch.get(entry.epoch) ?? { ms: {}, seen: {} }
    if (turn.seen[entry.milestone]) {
      duplicateCount += 1
      continue
    }
    turn.seen[entry.milestone] = true
    turn.ms[entry.milestone] = entry.elapsedMs
    byEpoch.set(entry.epoch, turn)
  }

  const turns = []
  const rejected = []
  for (const [epoch, turn] of byEpoch) {
    const missing = REQUIRED_MILESTONES.filter(
      (name) => turn.ms[name] === undefined,
    )
    if (missing.length === 0) {
      turns.push({ epoch, ms: turn.ms })
    } else {
      rejected.push({ epoch, missing })
    }
  }
  turns.sort((a, b) => a.epoch - b.epoch)
  rejected.sort((a, b) => a.epoch - b.epoch)
  return { turns, rejected, duplicateCount }
}

/**
 * Phân vị nearest-rank — cùng khuôn `ttft_bench#phan_vi`: chỉ số nhỏ nhất mà
 * >= p% mẫu nằm dưới nó. Trả `null` cho mẫu rỗng.
 */
export function percentile(sortedValues, p) {
  if (sortedValues.length === 0) return null
  const rank = Math.max(1, Math.ceil((p / 100) * sortedValues.length))
  return sortedValues[Math.min(rank, sortedValues.length) - 1]
}

function stats(values) {
  const sorted = [...values].sort((a, b) => a - b)
  return {
    n: sorted.length,
    min: sorted[0],
    p50: percentile(sorted, 50),
    p95: percentile(sorted, 95),
    max: sorted[sorted.length - 1],
  }
}

/**
 * Phân rã MỘT lượt thành các khoảng thời gian. Xuất riêng để test khẳng định
 * từng giá trị với số tính tay — không tự tính lại số học trong test.
 *
 * Lưu ý `cancelFlushMs`: t0 của lượt lấy TRƯỚC `cancel_active_operations()`
 * nhưng mốc `vad_end` phát SAU khi hàm đó xong, nên elapsed_ms của `vad_end`
 * chính là chi phí huỷ lượt cũ + flush — LUÔN khác 0 trong log thật. Bốn hàng
 * dưới cộng đúng bằng `turnLatencyMs`.
 */
export function decompose(turn) {
  return {
    cancelFlushMs: turn.ms.vad_end,
    sttMs: turn.ms.stt_done - turn.ms.vad_end,
    llmFirstTokenMs: turn.ms.first_token - turn.ms.stt_done,
    ttsToAudioMs: turn.ms.first_speaker_frame - turn.ms.first_token,
    // Turn latency (VAD SpeechEnd → frame loa đầu tiên) chính là TTFA.
    turnLatencyMs: turn.ms.first_speaker_frame,
  }
}

/**
 * Tổng hợp thống kê: p50/p95 từng mốc, bốn hàng phân rã (qua [`decompose`]),
 * và turn latency (= TTFA = elapsed_ms của `first_speaker_frame`).
 */
export function summarize(turns) {
  const pick = (name) => turns.map((turn) => turn.ms[name])
  const col = (name) => turns.map((turn) => decompose(turn)[name])
  return {
    n: turns.length,
    milestones: Object.fromEntries(
      REQUIRED_MILESTONES.map((name) => [name, stats(pick(name))]),
    ),
    cancelFlushMs: stats(col('cancelFlushMs')),
    sttMs: stats(col('sttMs')),
    llmFirstTokenMs: stats(col('llmFirstTokenMs')),
    ttsToAudioMs: stats(col('ttsToAudioMs')),
    turnLatencyMs: stats(col('turnLatencyMs')),
  }
}

function fmtStats(label, s) {
  return `${label.padEnd(34)} n=${String(s.n).padStart(3)}  p50=${String(s.p50).padStart(6)} ms  p95=${String(s.p95).padStart(6)} ms  max=${String(s.max).padStart(6)} ms`
}

function machineConfig() {
  const cpus = os.cpus()
  return [
    '╭─ Cấu hình máy đo ────────────────────────────────────────',
    `│ OS               : ${os.platform()} ${os.release()} (${os.arch()})`,
    `│ CPU              : ${cpus[0]?.model ?? '--'} × ${cpus.length} lõi`,
    `│ RAM              : ${(os.totalmem() / 1024 ** 3).toFixed(1)} GiB`,
    `│ Node             : ${process.version}`,
    '│ Build/GPU/model  : PHẢI chép từ lần chạy gateway (RUST_LOG env,',
    '│                    model manifest, feature CUDA/Vulkan) — script không đọc được.',
    '╰──────────────────────────────────────────────────────────',
  ]
}

/** Dự báo cáo đầy đủ (mảng dòng). Xuất riêng để test khẳng định nội dung. */
export function renderReport(summary, { rejected, duplicateCount }) {
  const lines = [...machineConfig(), '']
  lines.push(fmtStats('Turn latency / TTFA', summary.turnLatencyMs))
  lines.push(
    fmtStats('├ Huỷ lượt cũ + flush (t0→vad_end)', summary.cancelFlushMs),
  )
  lines.push(fmtStats('├ STT (vad_end→stt_done)', summary.sttMs))
  lines.push(
    fmtStats('├ LLM tới token đầu (→first_token)', summary.llmFirstTokenMs),
  )
  lines.push(fmtStats('└ TTS tới frame loa (→speaker)', summary.ttsToAudioMs))
  lines.push('')
  for (const name of REQUIRED_MILESTONES) {
    lines.push(fmtStats(`Mốc ${name}`, summary.milestones[name]))
  }
  lines.push('')
  if (summary.n < 20) {
    lines.push(
      `⚠️  n = ${summary.n} < 20: p95 ở đây CHÍNH LÀ giá trị lớn nhất, không phải ước lượng đuôi.`,
    )
  }
  if (rejected.length > 0) {
    lines.push(`⚠️  Loại ${rejected.length} lượt thiếu mốc (không âm thầm):`)
    for (const r of rejected) {
      lines.push(`   · epoch ${r.epoch}: thiếu ${r.missing.join(', ')}`)
    }
  }
  if (duplicateCount > 0) {
    lines.push(
      `ℹ️  ${duplicateCount} dòng mốc lặp đã bỏ (giá trị đầu tiên thắng).`,
    )
  }
  return lines
}

async function main(argv) {
  const source = argv[0] ?? '-'
  const text =
    source === '-' ? fs.readFileSync(0, 'utf8') : fs.readFileSync(source, 'utf8')

  const entries = text
    .split('\n')
    .map(parseMilestoneLine)
    .filter(Boolean)
  const collected = collectTurns(entries)

  if (collected.turns.length === 0) {
    console.error(
      `voice-turn-latency: KHÔNG có lượt nào đủ cả ${REQUIRED_MILESTONES.length} mốc.` +
        (entries.length === 0
          ? ' Không tìm thấy dòng mốc nào — kiểm tra RUST_LOG có bật level info cho webrtc::pipeline.'
          : ` Có ${collected.rejected.length} lượt bị loại vì thiếu mốc.`),
    )
    process.exitCode = 1
    return
  }

  const summary = summarize(collected.turns)
  console.log(renderReport(summary, collected).join('\n'))
}

if (import.meta.url === `file://${process.argv[1]}`) {
  await main(process.argv.slice(2))
}
