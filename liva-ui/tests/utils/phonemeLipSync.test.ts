import { beforeEach, describe, expect, it } from "vitest";
import { parseVisemePayload } from "../../src/utils/speakerFrame";
import {
  activeTimelineQueue,
  currentViseme,
  currentVisemeFromClock,
  noteChunkScheduled,
  resetVisemes,
  setVisemeClock,
  setVisemeTimeline,
  TimelineQueue,
} from "../../src/utils/phonemeLipSync";
import type { VisemeCue } from "../../src/utils/speakerFrame";

// Timeline mẫu — số liệu tính tay: chunk 0.4 s, phoneme "ma" ⇒ Nil@0, Aa@200.
const CUES: VisemeCue[] = [
  { v: "nil", tMs: 0 },
  { v: "aa", tMs: 200 },
];

function visemePayloadJson(overrides: Record<string, unknown> = {}): Uint8Array {
  return new TextEncoder().encode(
    JSON.stringify({
      turn_epoch: 7,
      base_seq_id: 3,
      visemes: [
        { v: "nil", t_ms: 0 },
        { v: "aa", t_ms: 200 },
      ],
      ...overrides,
    }),
  );
}

describe("parseVisemePayload (VC-8)", () => {
  it("parse đúng payload hợp lệ (snake_case từ serde_json phía core)", () => {
    const tl = parseVisemePayload(visemePayloadJson());
    expect(tl).toEqual({
      turnEpoch: 7,
      baseSeqId: 3,
      cues: [
        { v: "nil", tMs: 0 },
        { v: "aa", tMs: 200 },
      ],
    });
  });

  it("loại payload JSON hỏng / thiếu trường / viseme lạ / tMs không tăng ngặt", () => {
    expect(parseVisemePayload(new TextEncoder().encode("{không phải json"))).toBeNull();
    expect(parseVisemePayload(new TextEncoder().encode('{"turnEpoch":7}'))).toBeNull();
    // Viseme ngoài whitelist — fail-closed cả timeline, không bỏ từng cue.
    expect(
      parseVisemePayload(
        new TextEncoder().encode(
          '{"turnEpoch":1,"baseSeqId":2,"cues":[{"v":"xx","tMs":0}]}',
        ),
      ),
    ).toBeNull();
    // tMs không tăng ngặt.
    expect(
      parseVisemePayload(
        new TextEncoder().encode(
          '{"turnEpoch":1,"baseSeqId":2,"cues":[{"v":"aa","tMs":5},{"v":"aa","tMs":5}]}',
        ),
      ),
    ).toBeNull();
  });
});

describe("phonemeLipSync registry (VC-8)", () => {
  beforeEach(() => {
    resetVisemes();
    setVisemeClock(null);
  });

  it("trả null khi chưa có timeline hoặc chưa neo vào chunk phát", () => {
    expect(currentViseme(1.0)).toBeNull(); // chưa có gì
    setVisemeTimeline(CUES);
    expect(currentViseme(1.0)).toBeNull(); // có timeline nhưng chưa neo
  });

  it("neo xong trả đúng viseme theo đồng hồ, quá cuối mẩu về null", () => {
    setVisemeTimeline(CUES);
    noteChunkScheduled(10.0, 0.4); // phát từ giây 10, dài 0.4 s

    expect(currentViseme(10.05)).toBe("nil"); // 50 ms < 200 ms
    expect(currentViseme(10.25)).toBe("aa"); // 250 ms ≥ 200 ms
    // Quá cuối mẩu (10.0 + 0.4) ⇒ hết hiệu lực, caller rơi về RMS.
    expect(currentViseme(10.45)).toBeNull();
    // Trước khi bắt đầu phát (buffer xếp lịch trước) ⇒ chưa áp.
    expect(currentViseme(9.99)).toBeNull();
  });

  it("timeline mới thay thế pending cũ; reset xoá cả pending lẫn anchored", () => {
    setVisemeTimeline(CUES);
    noteChunkScheduled(10.0, 0.4);
    setVisemeTimeline([{ v: "oh", tMs: 0 }]); // pending mới, chưa neo
    expect(currentViseme(10.3)).toBe("aa"); // timeline cũ vẫn hiệu lực tới hết mẩu

    noteChunkScheduled(20.0, 0.5); // neo timeline mới
    expect(currentViseme(20.1)).toBe("oh");

    resetVisemes();
    expect(currentViseme(20.1)).toBeNull();
  });

  it("currentVisemeFromClock trả null khi không cắm clock hoặc clock trả null", () => {
    expect(currentVisemeFromClock()).toBeNull();

    setVisemeTimeline(CUES);
    let fakeClockSec: number | null = 10.25;
    setVisemeClock(() => fakeClockSec);
    noteChunkScheduled(10.0, 0.4);
    expect(currentVisemeFromClock()).toBe("aa");

    fakeClockSec = null; // AudioContext chưa sẵn sàng
    expect(currentVisemeFromClock()).toBeNull();
  });

  it("F5: multi-chunk duration accumulation tích lũy thời lượng qua nhiều frame 100ms mà không đóng miệng sớm", () => {
    // Clause kéo dài 1500ms (1.5s) với các cues trải đều
    const clauseCues: VisemeCue[] = [
      { v: "nil", tMs: 0 },
      { v: "aa", tMs: 200 },
      { v: "oh", tMs: 600 },
      { v: "ou", tMs: 1000 },
    ];
    setVisemeTimeline(clauseCues);

    // Mô phỏng 15 chunks 100ms từ useSpeakerPlayback
    for (let i = 0; i < 15; i++) {
      noteChunkScheduled(10.0 + i * 0.1, 0.1);
    }

    const queue = activeTimelineQueue();
    expect(queue).toHaveLength(1);
    expect(queue[0].anchorSec).toBeCloseTo(10.0, 5);
    expect(queue[0].endSec).toBeCloseTo(11.5, 5); // 10.0 + 15 * 0.1 = 11.5s

    // Trước khi nói
    expect(currentViseme(9.99)).toBeNull();

    // Trong frame đầu (50ms)
    expect(currentViseme(10.05)).toBe("nil");

    // Khắc phục bug: sau 100ms (tại 10.25s / 250ms), miệng KHÔNG bị đóng, vẫn trả "aa"
    expect(currentViseme(10.25)).toBe("aa");

    // Tại 650ms (10.65s) trả "oh"
    expect(currentViseme(10.65)).toBe("oh");

    // Tại 1050ms (11.05s) trả "ou"
    expect(currentViseme(11.05)).toBe("ou");

    // Tại 1450ms (11.45s) vẫn trả "ou"
    expect(currentViseme(11.45)).toBe("ou");

    // Quá toàn bộ 1500ms (11.55s) mới hết hiệu lực và trả null
    expect(currentViseme(11.55)).toBeNull();
  });

  it("F5: FIFO TimelineQueue không bị clobber khi chunk của mệnh đề kế tiếp đến sớm", () => {
    // Mệnh đề 1: 10.0 -> 10.5s
    setVisemeTimeline([
      { v: "nil", tMs: 0 },
      { v: "aa", tMs: 200 },
    ]);
    for (let i = 0; i < 5; i++) {
      noteChunkScheduled(10.0 + i * 0.1, 0.1);
    }

    // Mệnh đề 2 đến sớm và được xếp lịch trước cho t=10.5 -> 10.8s
    setVisemeTimeline([
      { v: "oh", tMs: 0 },
      { v: "ee", tMs: 150 },
    ]);
    for (let i = 0; i < 3; i++) {
      noteChunkScheduled(10.5 + i * 0.1, 0.1);
    }

    // Cả 2 timeline phải cùng tồn tại trong queue
    const queue = activeTimelineQueue();
    expect(queue).toHaveLength(2);
    expect(queue[0].anchorSec).toBeCloseTo(10.0, 5);
    expect(queue[0].endSec).toBeCloseTo(10.5, 5);
    expect(queue[1].anchorSec).toBeCloseTo(10.5, 5);
    expect(queue[1].endSec).toBeCloseTo(10.8, 5);

    // Khi đồng hồ đang ở mệnh đề 1 (t=10.3s), trả viseme của mệnh đề 1 ("aa"), KHÔNG bị mệnh đề 2 ghi đè!
    expect(currentViseme(10.3)).toBe("aa");

    // Khi đồng hồ bước sang mệnh đề 2 (t=10.68s / 180ms của mẩu 2), trả "ee"
    expect(currentViseme(10.68)).toBe("ee");

    // Sau khi kết thúc cả 2 mệnh đề (t=10.85s), trả null
    expect(currentViseme(10.85)).toBeNull();
  });

  it("F5: TimelineQueue instance độc lập hoạt động chuẩn xác", () => {
    const tq = new TimelineQueue();
    tq.setPending([{ v: "ih", tMs: 0 }]);
    tq.noteChunkScheduled(5.0, 0.2);
    tq.noteChunkScheduled(5.2, 0.2);

    expect(tq.getQueue()).toHaveLength(1);
    expect(tq.getQueue()[0].endSec).toBeCloseTo(5.4, 5);
    expect(tq.currentViseme(5.1)).toBe("ih");
    expect(tq.currentViseme(5.3)).toBe("ih");
    expect(tq.currentViseme(5.45)).toBeNull();

    tq.reset();
    expect(tq.getQueue()).toHaveLength(0);
    expect(tq.currentViseme(5.1)).toBeNull();
  });
});
