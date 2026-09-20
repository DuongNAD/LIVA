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

export interface AnchoredTimeline {
  cues: VisemeCue[];
  /** ctx.currentTime lúc mẫu PCM đầu tiên của chunk bắt đầu phát. */
  anchorSec: number;
  /** Thời điểm kết thúc chunk (anchor + độ dài audio) — quá mốc là hết hiệu lực. */
  endSec: number;
}

/**
 * TimelineQueue — hàng đợi FIFO quản lý timeline viseme cho streaming TTS (F5).
 *
 * Khắc phục khiếm khuyết biến đơn `anchored`:
 * 1. Tích lũy thời lượng (multi-chunk duration accumulation): Audio streaming
 *    được phân mảnh thành các frame 100ms (`SPEAKER_FRAME_DURATION_MS = 100`),
 *    trong khi `OP_VISME` mang toàn bộ timeline của cả mệnh đề (ví dụ 1500ms).
 *    Các frame âm thanh kế tiếp của cùng mệnh đề sẽ tích lũy `endSec` thay vì
 *    bị bỏ qua hay làm miệng đóng sớm sau 100ms đầu tiên.
 * 2. Ngăn ngừa clobbering do frame kế đến sớm: Chunks của mệnh đề kế tiếp được
 *    xếp lịch trước trong AudioContext sẽ được đẩy vào FIFO queue thay vì ghi
 *    đè ngay lập tức lên timeline đang phát, bảo đảm khẩu hình của câu hiện tại
 *    vẫn hoạt động trọn vẹn.
 */
export class TimelineQueue {
  private queue: AnchoredTimeline[] = [];
  private pendingCues: VisemeCue[] | null = null;

  /** Đặt timeline mới cho chunk sắp tới (thay thế pending cũ nếu còn). */
  setPending(cues: VisemeCue[]): void {
    this.pendingCues = cues;
  }

  /** Đọc-thôi: timeline đang chờ neo. */
  getPending(): readonly VisemeCue[] {
    return this.pendingCues ?? [];
  }

  /**
   * Neo pending timeline vào thời điểm bắt đầu phát của chunk kế tiếp hoặc
   * tích lũy thời lượng cho chunk liên tiếp của cùng mệnh đề.
   */
  noteChunkScheduled(
    startCtxSec: number,
    durationSec: number,
    expectedSeqId?: number,
  ): void {
    void expectedSeqId;
    const safeDuration = Math.max(0, durationSec);
    const chunkEndSec = startCtxSec + safeDuration;

    if (this.pendingCues && this.pendingCues.length > 0) {
      // Neo pending timeline mới vào queue (FIFO)
      this.queue.push({
        cues: this.pendingCues,
        anchorSec: startCtxSec,
        endSec: chunkEndSec,
      });
      this.pendingCues = null;
    } else if (this.queue.length > 0) {
      // Chunk liên tiếp của cùng mệnh đề đang phát: tích lũy thời lượng
      const current = this.queue[this.queue.length - 1];
      // Nếu chunk liên tiếp hoặc nằm trong ngưỡng jitter (200ms)
      if (startCtxSec <= current.endSec + 0.2) {
        current.endSec = Math.max(current.endSec, chunkEndSec);
      }
    }
  }

  /**
   * Viseme đang hiệu lực tại `ctxTimeSec`, hoặc `null` khi không có timeline /
   * chưa neo / audio của mẩu đã phát hết (⇒ caller rơi về RMS).
   */
  currentViseme(ctxTimeSec: number): string | null {
    // Dọn dẹp các timeline đã kết thúc hoàn toàn trong quá khứ (>1.0s)
    while (this.queue.length > 1 && this.queue[0].endSec + 1.0 < ctxTimeSec) {
      this.queue.shift();
    }

    // Tìm timeline đang hiệu lực trong queue (ưu tiên timeline sau nếu có crossfade)
    let activeTimeline: AnchoredTimeline | null = null;
    for (let i = this.queue.length - 1; i >= 0; i--) {
      const item = this.queue[i];
      if (ctxTimeSec >= item.anchorSec && ctxTimeSec <= item.endSec) {
        activeTimeline = item;
        break;
      }
    }

    if (!activeTimeline) return null;

    const elapsedMs = Math.round((ctxTimeSec - activeTimeline.anchorSec) * 1000);

    // Tìm cue cuối có tMs <= elapsedMs (cues tăng ngặt theo bất biến từ parser).
    let lo = 0;
    let hi = activeTimeline.cues.length - 1;
    let found: VisemeCue | null = null;
    while (lo <= hi) {
      const mid = (lo + hi) >> 1;
      if (activeTimeline.cues[mid].tMs <= elapsedMs) {
        found = activeTimeline.cues[mid];
        lo = mid + 1;
      } else {
        hi = mid - 1;
      }
    }
    return found ? found.v : null;
  }

  /** Barge-in / lượt mới: xoá sạch trạng thái viseme. */
  reset(): void {
    this.pendingCues = null;
    this.queue = [];
  }

  /** Đọc-thôi: danh sách các timeline đã neo đang trong queue. */
  getQueue(): readonly AnchoredTimeline[] {
    return this.queue;
  }
}

const defaultTimelineQueue = new TimelineQueue();

/** Đặt timeline mới cho chunk sắp tới (thay thế pending cũ nếu còn). */
export function setVisemeTimeline(cues: VisemeCue[]): void {
  defaultTimelineQueue.setPending(cues);
}

/**
 * Neo pending timeline vào thời điểm bắt đầu phát của chunk kế tiếp hoặc tích
 * lũy thời lượng cho các chunk 100ms nối tiếp nhau.
 */
export function noteChunkScheduled(
  startCtxSec: number,
  durationSec: number,
  expectedSeqId?: number,
): void {
  defaultTimelineQueue.noteChunkScheduled(startCtxSec, durationSec, expectedSeqId);
}

/**
 * Viseme đang hiệu lực tại `ctxTimeSec`, hoặc `null` khi không có timeline /
 * chưa neo / audio của mẩu đã phát hết (⇒ caller rơi về RMS).
 */
export function currentViseme(ctxTimeSec: number): string | null {
  return defaultTimelineQueue.currentViseme(ctxTimeSec);
}

/** Barge-in / lượt mới: xoá sạch trạng thái viseme. */
export function resetVisemes(): void {
  defaultTimelineQueue.reset();
}

/** Đọc-thôi: timeline đang chờ neo (dùng để kiểm thử nối dây UI — VC-8). */
export function pendingVisemeCues(): readonly VisemeCue[] {
  return defaultTimelineQueue.getPending();
}

/** Đọc-thôi: danh sách các timeline đã neo đang trong queue (dành cho kiểm thử F5). */
export function activeTimelineQueue(): readonly AnchoredTimeline[] {
  return defaultTimelineQueue.getQueue();
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
