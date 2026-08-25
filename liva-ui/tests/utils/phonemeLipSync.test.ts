import { beforeEach, describe, expect, it } from "vitest";
import { parseVisemePayload } from "../../src/utils/speakerFrame";
import {
  currentViseme,
  currentVisemeFromClock,
  noteChunkScheduled,
  resetVisemes,
  setVisemeClock,
  setVisemeTimeline,
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
});
