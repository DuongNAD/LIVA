import { beforeEach, describe, expect, it } from "vitest";
import {
  activeTimelineQueue,
  currentViseme,
  noteChunkScheduled,
  resetVisemes,
  setVisemeTimeline,
  TimelineQueue,
} from "../../src/utils/phonemeLipSync";
import type { VisemeCue } from "../../src/utils/speakerFrame";

describe("TimelineQueue Adversarial Stress Testing (Milestone 2)", () => {
  beforeEach(() => {
    resetVisemes();
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // CHALLENGE 1: Fragmented Audio Delivery (15x100ms frames = 1500ms clause)
  // ─────────────────────────────────────────────────────────────────────────────
  describe("Challenge 1: Fragmented Audio Delivery (15x100ms frames)", () => {
    it("duy trì viseme hoạt động xuyên suốt 1500ms và không bao giờ rơi về null sớm", () => {
      const clauseCues: VisemeCue[] = [
        { v: "nil", tMs: 0 },
        { v: "aa", tMs: 150 },
        { v: "ee", tMs: 450 },
        { v: "oh", tMs: 850 },
        { v: "ou", tMs: 1200 },
      ];

      setVisemeTimeline(clauseCues);

      // Mô phỏng 15 chunks 100ms được chuyển giao liên tiếp
      const startBase = 20.0;
      const frameDuration = 0.1; // 100ms
      const totalFrames = 15;
      const totalClauseDuration = 1.5; // 1500ms

      for (let i = 0; i < totalFrames; i++) {
        noteChunkScheduled(startBase + i * frameDuration, frameDuration);
      }

      const queue = activeTimelineQueue();
      expect(queue).toHaveLength(1);
      expect(queue[0].anchorSec).toBeCloseTo(startBase, 5);
      expect(queue[0].endSec).toBeCloseTo(startBase + totalClauseDuration, 5);

      // 1. Trước mốc bắt đầu nói
      expect(currentViseme(startBase - 0.001)).toBeNull();

      // 2. Kiểm tra mật độ cao: quét từng bước 5ms xuyên suốt toàn bộ 1500ms (301 mẫu kiểm tra)
      let nullDropCount = 0;
      const observedVisemes = new Set<string>();

      for (let offsetMs = 0; offsetMs <= 1500; offsetMs += 5) {
        const t = startBase + offsetMs / 1000.0;
        const v = currentViseme(t);
        if (v === null) {
          nullDropCount++;
        } else {
          observedVisemes.add(v);
        }
      }

      // Tuyệt đối không được rơi về null tại bất kỳ điểm nào trong [20.0, 21.5]
      expect(nullDropCount).toBe(0);
      // Toàn bộ các viseme trong câu đều phải xuất hiện
      expect(observedVisemes).toContain("nil");
      expect(observedVisemes).toContain("aa");
      expect(observedVisemes).toContain("ee");
      expect(observedVisemes).toContain("oh");
      expect(observedVisemes).toContain("ou");

      // 3. Kiểm tra ranh giới chính xác từng mốc âm vị
      expect(currentViseme(20.000)).toBe("nil"); // t=0ms
      expect(currentViseme(20.149)).toBe("nil"); // t=149ms < 150ms
      expect(currentViseme(20.150)).toBe("aa");  // t=150ms
      expect(currentViseme(20.449)).toBe("aa");  // t=449ms < 450ms
      expect(currentViseme(20.450)).toBe("ee");  // t=450ms
      expect(currentViseme(20.849)).toBe("ee");  // t=849ms < 850ms
      expect(currentViseme(20.850)).toBe("oh");  // t=850ms
      expect(currentViseme(21.199)).toBe("oh");  // t=1199ms < 1200ms
      expect(currentViseme(21.200)).toBe("ou");  // t=1200ms
      expect(currentViseme(21.500)).toBe("ou");  // t=1500ms (kết thúc mẩu)

      // 4. Ngay sau khi kết thúc 1500ms audio (+1ms) ⇒ trả về null (miệng đóng / fallback RMS)
      expect(currentViseme(21.501)).toBeNull();
      expect(currentViseme(21.600)).toBeNull();
    });

    it("xử lý phân mảnh không đều và jitter nhỏ (nhảy khung 60ms, 120ms, 80ms)", () => {
      const clauseCues: VisemeCue[] = [
        { v: "nil", tMs: 0 },
        { v: "aa", tMs: 200 },
        { v: "ih", tMs: 600 },
      ];
      setVisemeTimeline(clauseCues);

      // Phân mảnh không đều với jitter:
      // Frame 1: 30.0 -> 30.06 (60ms)
      // Frame 2: 30.06 -> 30.18 (120ms)
      // Frame 3: 30.20 (gap 20ms jitter) -> 30.28 (80ms)
      // Frame 4: 30.28 -> 30.40 (120ms)
      noteChunkScheduled(30.0, 0.06);
      noteChunkScheduled(30.06, 0.12);
      noteChunkScheduled(30.20, 0.08); // trong ngưỡng jitter 200ms (30.20 <= 30.18 + 0.2)
      noteChunkScheduled(30.28, 0.12);

      const queue = activeTimelineQueue();
      expect(queue).toHaveLength(1);
      expect(queue[0].endSec).toBeCloseTo(30.40, 5);

      // Viseme duy trì liên tục
      expect(currentViseme(30.05)).toBe("nil");
      expect(currentViseme(30.25)).toBe("aa");
      expect(currentViseme(30.35)).toBe("aa");
      expect(currentViseme(30.41)).toBeNull();
    });
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // CHALLENGE 2: Out-of-Order & Early-Arriving Chunks
  // ─────────────────────────────────────────────────────────────────────────────
  describe("Challenge 2: Out-of-Order & Early-Arriving Chunks", () => {
    it("Chunk N+1 xếp lịch trước 50ms không được ghi đè / clobber active speech của Chunk N", () => {
      // Chunk N (Clause 1): 10.0 -> 10.8s
      const cuesClause1: VisemeCue[] = [
        { v: "nil", tMs: 0 },
        { v: "aa", tMs: 200 },
        { v: "ee", tMs: 500 },
      ];
      setVisemeTimeline(cuesClause1);
      for (let i = 0; i < 8; i++) {
        noteChunkScheduled(10.0 + i * 0.1, 0.1);
      }

      // Chunk N+1 (Clause 2): đến sớm 50ms trước khi Clause 1 kết thúc (bắt đầu tại 10.75s)
      const cuesClause2: VisemeCue[] = [
        { v: "oh", tMs: 0 },
        { v: "ou", tMs: 250 },
      ];
      setVisemeTimeline(cuesClause2);
      // Clause 2 xếp lịch trước 6 chunks (10.75 -> 11.35s)
      for (let i = 0; i < 6; i++) {
        noteChunkScheduled(10.75 + i * 0.1, 0.1);
      }

      const queue = activeTimelineQueue();
      // Hàng đợi FIFO phải chứa cả 2 clause riêng biệt
      expect(queue).toHaveLength(2);
      expect(queue[0].anchorSec).toBeCloseTo(10.0, 5);
      expect(queue[0].endSec).toBeCloseTo(10.8, 5);
      expect(queue[1].anchorSec).toBeCloseTo(10.75, 5);
      expect(queue[1].endSec).toBeCloseTo(11.35, 5);

      // KHÔNG CLOBBER: Trong suốt thời gian Clause 1 đang nói (10.0 -> 10.74s),
      // viseme trả về PHẢI là của Clause 1 ("aa" và "ee"), hoàn toàn không bị Clause 2 ("oh") ghi đè!
      expect(currentViseme(10.10)).toBe("nil");
      expect(currentViseme(10.30)).toBe("aa");
      expect(currentViseme(10.60)).toBe("ee");
      expect(currentViseme(10.74)).toBe("ee");

      // Tại thời điểm crossfade 10.75s trở đi, ưu tiên chuyển giao sang Clause 2
      expect(currentViseme(10.75)).toBe("oh");
      expect(currentViseme(10.95)).toBe("oh");
      expect(currentViseme(11.05)).toBe("ou");
      expect(currentViseme(11.35)).toBe("ou");

      // Sau khi Clause 2 kết thúc
      expect(currentViseme(11.36)).toBeNull();
    });

    it("xếp lịch xen kẽ nhiều chunks giữa 2 mệnh đề không gây đảo lộn thứ tự FIFO", () => {
      const tq = new TimelineQueue();

      // Mệnh đề 1
      tq.setPending([{ v: "aa", tMs: 0 }]);
      tq.noteChunkScheduled(1.0, 0.2); // [1.0, 1.2]
      tq.noteChunkScheduled(1.2, 0.2); // [1.0, 1.4]

      // Mệnh đề 2 (đến sớm lúc mẩu 1 chưa phát xong)
      tq.setPending([{ v: "ih", tMs: 0 }]);
      tq.noteChunkScheduled(1.4, 0.2); // [1.4, 1.6]
      tq.noteChunkScheduled(1.6, 0.2); // [1.4, 1.8]

      // Mệnh đề 3
      tq.setPending([{ v: "ou", tMs: 0 }]);
      tq.noteChunkScheduled(1.8, 0.2); // [1.8, 2.0]

      const queue = tq.getQueue();
      expect(queue).toHaveLength(3);
      expect(queue[0].cues[0].v).toBe("aa");
      expect(queue[1].cues[0].v).toBe("ih");
      expect(queue[2].cues[0].v).toBe("ou");

      expect(tq.currentViseme(1.1)).toBe("aa");
      expect(tq.currentViseme(1.3)).toBe("aa");
      expect(tq.currentViseme(1.5)).toBe("ih");
      expect(tq.currentViseme(1.7)).toBe("ih");
      expect(tq.currentViseme(1.9)).toBe("ou");
      expect(tq.currentViseme(2.05)).toBeNull();
    });
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // CHALLENGE 3: Rapid Multi-Clause Bursts & Memory Leak Purging
  // ─────────────────────────────────────────────────────────────────────────────
  describe("Challenge 3: Rapid Multi-Clause Bursts & Memory Leak Purge", () => {
    it("xử lý 10 mệnh đề hội thoại dồn dập (burst), dọn sạch cues lỗi thời, không rò rỉ bộ nhớ", () => {
      const tq = new TimelineQueue();
      const numClauses = 10;
      const clauseDuration = 1.0; // 1 giây mỗi clause (10 chunks 100ms)

      // Xếp lịch 10 mệnh đề liên tiếp: [0, 1], [1, 2], ..., [9, 10]
      for (let c = 0; c < numClauses; c++) {
        const startSec = c * clauseDuration;
        const visemeChar = (["aa", "ee", "ih", "oh", "ou"] as const)[c % 5];
        tq.setPending([
          { v: "nil", tMs: 0 },
          { v: visemeChar, tMs: 100 },
        ]);

        for (let chunk = 0; chunk < 10; chunk++) {
          tq.noteChunkScheduled(startSec + chunk * 0.1, 0.1);
        }
      }

      // Lúc đầu, cả 10 clauses nằm trong queue trước khi đồng hồ chạy
      expect(tq.getQueue()).toHaveLength(10);

      // Khi thời gian trôi qua, gọi currentViseme mô phỏng render loop 60 FPS
      // Tại t = 0.5s: Clause 0 đang phát
      expect(tq.currentViseme(0.5)).toBe("aa");
      expect(tq.getQueue()).toHaveLength(10);

      // Tại t = 2.5s:
      // Clause 0 (endSec = 1.0): 1.0 + 1.0 = 2.0 < 2.5 ⇒ ĐÃ BỊ DỌN DẸP!
      // Queue chỉ còn 9 clauses
      expect(tq.currentViseme(2.5)).toBe("ih");
      expect(tq.getQueue().length).toBeLessThanOrEqual(9);

      // Tại t = 5.5s:
      // Clause 0, 1, 2, 3 (endSec <= 4.0, 4.0 + 1.0 = 5.0 < 5.5) ⇒ ĐÃ BỊ DỌN DẸP!
      expect(tq.currentViseme(5.5)).toBe("aa");
      expect(tq.getQueue().length).toBeLessThanOrEqual(6);

      // Tại t = 9.5s: Clause 9 đang phát
      expect(tq.currentViseme(9.5)).toBe("ou");
      expect(tq.getQueue().length).toBeLessThanOrEqual(2);

      // Tại t = 11.5s: Toàn bộ 10 clauses đã phát xong (>1.0s quá khứ)
      expect(tq.currentViseme(11.5)).toBeNull();
      // Cơ chế while(length > 1 && endSec + 1.0 < ctxTime) dọn dẹp còn đúng 1 entry cuối
      expect(tq.getQueue().length).toBe(1);

      // Reset xoá sạch hoàn toàn
      tq.reset();
      expect(tq.getQueue()).toHaveLength(0);
    });

    it("stress test chịu tải lớn: 50 mệnh đề liên tục không làm phình queue hay suy giảm hiệu năng", () => {
      const tq = new TimelineQueue();
      const totalClauses = 50;

      for (let c = 0; c < totalClauses; c++) {
        const startSec = c * 0.5; // mỗi clause 500ms
        tq.setPending([{ v: "aa", tMs: 0 }]);
        for (let chunk = 0; chunk < 5; chunk++) {
          tq.noteChunkScheduled(startSec + chunk * 0.1, 0.1);
        }

        // Mô phỏng render loop gọi currentViseme liên tục
        const currentClock = startSec + 0.25;
        const v = tq.currentViseme(currentClock);
        expect(v).toBe("aa");

        // Queue không bao giờ phình to quá mức nhờ cơ chế purge quá khứ
        // Mỗi clause 0.5s, threshold dọn dẹp là 1.0s, nên queue không vượt quá 5 entries
        expect(tq.getQueue().length).toBeLessThanOrEqual(5);
      }

      // Tại mốc thời gian sau khi kết thúc toàn bộ (+2s)
      const finalClock = totalClauses * 0.5 + 2.0;
      expect(tq.currentViseme(finalClock)).toBeNull();
      expect(tq.getQueue().length).toBe(1);
    });
  });

  // ─────────────────────────────────────────────────────────────────────────────
  // CHALLENGE 4: Adversarial Boundaries, Malformed Inputs & Edge Cases
  // ─────────────────────────────────────────────────────────────────────────────
  describe("Challenge 4: Adversarial Boundaries & Edge Cases", () => {
    it("xử lý an toàn khi durationSec <= 0 hoặc âm", () => {
      const tq = new TimelineQueue();
      tq.setPending([{ v: "aa", tMs: 0 }]);
      tq.noteChunkScheduled(10.0, 0); // 0 duration
      expect(tq.getQueue()).toHaveLength(1);
      expect(tq.getQueue()[0].endSec).toBe(10.0);

      tq.noteChunkScheduled(10.0, -0.5); // âm duration
      expect(tq.getQueue()[0].endSec).toBe(10.0);
    });

    it("bỏ qua chunk tới sau khoảng trống âm thanh lớn (>200ms jitter) khi không có pending timeline mới", () => {
      const tq = new TimelineQueue();
      tq.setPending([{ v: "aa", tMs: 0 }]);
      tq.noteChunkScheduled(10.0, 0.2); // endSec = 10.2
      // Chunk tiếp theo đến muộn sau 300ms (>200ms jitter) mà không có timeline mới
      tq.noteChunkScheduled(10.5, 0.2);
      // Không được kéo dài bừa bãi mẩu cũ qua khoảng lặng lớn
      expect(tq.getQueue()[0].endSec).toBeCloseTo(10.2, 5);
    });

    it("setPending liên tục nhiều lần ghi đè pending cũ an toàn mà không sinh orphaned entry", () => {
      const tq = new TimelineQueue();
      tq.setPending([{ v: "aa", tMs: 0 }]);
      tq.setPending([{ v: "ee", tMs: 0 }]);
      tq.setPending([{ v: "ou", tMs: 0 }]);

      expect(tq.getPending()).toEqual([{ v: "ou", tMs: 0 }]);
      tq.noteChunkScheduled(5.0, 0.3);

      expect(tq.getQueue()).toHaveLength(1);
      expect(tq.getQueue()[0].cues[0].v).toBe("ou");
    });

    it("reset() lập tức triệt tiêu cả pending lẫn active queues giữa chừng (Barge-in)", () => {
      const tq = new TimelineQueue();
      tq.setPending([{ v: "aa", tMs: 0 }]);
      tq.noteChunkScheduled(10.0, 0.5);
      tq.setPending([{ v: "ee", tMs: 0 }]); // pending đang chờ

      expect(tq.currentViseme(10.2)).toBe("aa");

      // Người dùng chen ngang (Barge-in OP_FLUSH)
      tq.reset();

      expect(tq.currentViseme(10.2)).toBeNull();
      expect(tq.getQueue()).toHaveLength(0);
      expect(tq.getPending()).toEqual([]);
    });
  });
});
