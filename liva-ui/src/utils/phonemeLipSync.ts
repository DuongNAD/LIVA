/**
 * phonemeLipSync.ts — registry timeline viseme của lượt đang nói (VC-8).
 * ====================================================================
 * Core gửi `OP_VISME` ngay TRƯỚC các frame loa của cùng mẩu trên cùng kênh
 * FIFO, nên thứ tự xử lý phía client bảo đảm: timeline tới trước audio. Khi
 * chunk PCM tương ứng được xếp lịch phát (`noteChunkScheduled`), timeline được
 * "neo" vào đồng hồ AudioContext; `currentViseme()` trả mã viseme tại thời điểm
 * hiện tại để vòng render bẻ khẩu hình.
 *
 * Không có timeline / chưa neo / đã quá cuối mẩu ⇒ `null` — caller giữ nguyên
 * đường lip-sync RMS cũ. Đây chính là cơ chế dự phòng khi backend fallback
 * (Kokoro) không phát timeline.
 */
import type { VisemeCue } from "./speakerFrame";

interface AnchoredTimeline {
  cues: VisemeCue[];
  /** ctx.currentTime lúc mẫu PCM đầu tiên của chunk bắt đầu phát. */
  anchorSec: number;
  /** Thời điểm kết thúc chunk (anchor + độ dài audio) — quá mốc là hết hiệu lực. */
  endSec: number;
}

let pendingCues: VisemeCue[] | null = null;
let anchored: AnchoredTimeline | null = null;

/** Đặt timeline mới cho chunk sắp tới (thay thế pending cũ nếu còn). */
export function setVisemeTimeline(cues: VisemeCue[]): void {
  pendingCues = cues;
}

/**
 * Neo pending timeline vào thời điểm bắt đầu phát của chunk kế tiếp. Gọi từ
 * callback lập lịch của useSpeakerPlayback. `expectedSeqId` chỉ dùng để log
 * lệch nhịp — kênh FIFO bảo đảm chunk kế tiếp chính là chunk của timeline.
 */
export function noteChunkScheduled(
  startCtxSec: number,
  durationSec: number,
  expectedSeqId?: number,
): void {
  void expectedSeqId;
  if (!pendingCues || pendingCues.length === 0) return;
  anchored = {
    cues: pendingCues,
    anchorSec: startCtxSec,
    endSec: startCtxSec + Math.max(0, durationSec),
  };
  pendingCues = null;
}

/**
 * Viseme đang hiệu lực tại `ctxTimeSec`, hoặc `null` khi không có timeline /
 * chưa neo / audio của mẩu đã phát hết (⇒ caller rơi về RMS).
 */
export function currentViseme(ctxTimeSec: number): string | null {
  const a = anchored;
  if (!a || ctxTimeSec < a.anchorSec || ctxTimeSec > a.endSec) return null;
  const elapsedMs = Math.round((ctxTimeSec - a.anchorSec) * 1000);

  // Tìm cue cuối có tMs <= elapsedMs (cues tăng ngặt theo bất biến từ parser).
  let lo = 0;
  let hi = a.cues.length - 1;
  let found: VisemeCue | null = null;
  while (lo <= hi) {
    const mid = (lo + hi) >> 1;
    if (a.cues[mid].tMs <= elapsedMs) {
      found = a.cues[mid];
      lo = mid + 1;
    } else {
      hi = mid - 1;
    }
  }
  return found ? found.v : null;
}

/** Barge-in / lượt mới: xoá sạch trạng thái viseme. */
export function resetVisemes(): void {
  pendingCues = null;
  anchored = null;
}

// ── Đồng hồ ─────────────────────────────────────────────────────
// Registry không tự biết AudioContext; nơi sở hữu useSpeakerPlayback cắm một
// clock provider MỘT lần (trả giây theo cùng đồng hồ ctx.currentTime).

let clockFn: (() => number | null) | null = null;

export function setVisemeClock(fn: (() => number | null) | null): void {
  clockFn = fn;
}

/** Tiện ích cho vòng render: viseme hiện tại theo clock đã cắm, hoặc `null`. */
export function currentVisemeFromClock(): string | null {
  const t = clockFn?.() ?? null;
  return t === null ? null : currentViseme(t);
}
