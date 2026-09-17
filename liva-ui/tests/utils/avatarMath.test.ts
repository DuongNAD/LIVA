/**
 * avatarMath.test.ts — hàm thuần của hệ hoạt ảnh avatar
 * ======================================================
 * Chuyển từ `tests/composables/useVRM.test.ts` ngày 06/08/2026 (mục U25).
 *
 * Bản cũ nhập năm hàm này từ `useVRM.ts` — một composable **mồ côi**, không có
 * call-site sản xuất nào. Bản thật sự chạy nằm trong `use3DModel.ts` dưới dạng
 * một bộ hàm riêng, trùng nội dung từng byte. Nghĩa là bộ test này xưa nay kiểm
 * một bản sao chứ không kiểm bản đang chạy; hai bản khớp nhau hoàn toàn do may.
 *
 * Nay chỉ còn một bản, ở `src/utils/avatarMath.ts`, và `use3DModel.ts` nhập từ
 * đó — nên test dưới đây kiểm đúng thứ đang chạy.
 *
 * Không cần mock THREE/VRM: đây là toán thuần, không đụng WebGL. Bản cũ phải
 * mock cả một cây module chỉ vì nó nhập xuyên qua một composable đồ hoạ.
 */
import { describe, it, expect } from "vitest";
import {
  lerp,
  easeOutQuad,
  easeInQuad,
  randomBlinkInterval,
  weightedRandom,
  MAX_SACCADE_AMPLITUDE_DEG,
  SACCADE_JUMP_DURATION_S,
  SACCADE_DRIFT_HALF_LIFE_S,
  randomSaccadeInterval,
  randomSaccadeDisplacement,
  smoothstep,
} from "../../src/utils/avatarMath";

describe("avatarMath — lerp", () => {
  it("should return start value when t=0", () => {
    expect(lerp(0, 100, 0)).toBe(0);
  });

  it("should return end value when t=1", () => {
    expect(lerp(0, 100, 1)).toBe(100);
  });

  it("should return midpoint when t=0.5", () => {
    expect(lerp(0, 100, 0.5)).toBe(50);
  });

  it("should handle negative values", () => {
    expect(lerp(-10, 10, 0.5)).toBe(0);
  });

  it("should handle t > 1 (extrapolation)", () => {
    expect(lerp(0, 100, 1.5)).toBe(150);
  });

  it("should handle identical start/end", () => {
    expect(lerp(42, 42, 0.7)).toBe(42);
  });
});

describe("avatarMath — easeOutQuad", () => {
  it("should return 0 at t=0", () => {
    expect(easeOutQuad(0)).toBe(0);
  });

  it("should return 1 at t=1", () => {
    expect(easeOutQuad(1)).toBe(1);
  });

  it("should be > linear at t=0.5 (fast start)", () => {
    expect(easeOutQuad(0.5)).toBe(0.75); // 0.5 * (2 - 0.5) = 0.75
    expect(easeOutQuad(0.5)).toBeGreaterThan(0.5);
  });

  it("should produce smooth values between 0 and 1", () => {
    for (let t = 0; t <= 1; t += 0.1) {
      const val = easeOutQuad(t);
      expect(val).toBeGreaterThanOrEqual(0);
      expect(val).toBeLessThanOrEqual(1);
    }
  });
});

describe("avatarMath — easeInQuad", () => {
  it("should return 0 at t=0", () => {
    expect(easeInQuad(0)).toBe(0);
  });

  it("should return 1 at t=1", () => {
    expect(easeInQuad(1)).toBe(1);
  });

  it("should be < linear at t=0.5 (slow start)", () => {
    expect(easeInQuad(0.5)).toBe(0.25); // 0.5 * 0.5 = 0.25
    expect(easeInQuad(0.5)).toBeLessThan(0.5);
  });
});

describe("avatarMath — randomBlinkInterval", () => {
  it("should return interval >= 2 seconds", () => {
    for (let i = 0; i < 100; i++) {
      expect(randomBlinkInterval()).toBeGreaterThanOrEqual(2);
    }
  });

  it("should return interval <= 9 seconds (2 + 4 + 3 max)", () => {
    for (let i = 0; i < 100; i++) {
      expect(randomBlinkInterval()).toBeLessThanOrEqual(9);
    }
  });

  it("should produce varied intervals (not constant)", () => {
    const intervals = new Set<number>();
    for (let i = 0; i < 20; i++) {
      intervals.add(Math.round(randomBlinkInterval() * 100));
    }
    // Should have some variation (at least 3 distinct values)
    expect(intervals.size).toBeGreaterThanOrEqual(3);
  });
});

describe("avatarMath — weightedRandom", () => {
  it("should return items from the options array", () => {
    const options = ["a", "b", "c"];
    const weights = [1, 1, 1];

    for (let i = 0; i < 50; i++) {
      const result = weightedRandom(options, weights);
      expect(options).toContain(result);
    }
  });

  it("should respect weights (heavily weighted option picked most)", () => {
    const options = ["rare", "common"];
    const weights = [0.01, 0.99]; // 'common' should appear ~99% of the time

    let commonCount = 0;
    const trials = 1000;
    for (let i = 0; i < trials; i++) {
      if (weightedRandom(options, weights) === "common") commonCount++;
    }

    // Should be at least 90% common (with overwhelming probability)
    expect(commonCount / trials).toBeGreaterThan(0.85);
  });

  it("should handle single-item array", () => {
    expect(weightedRandom(["only"], [1])).toBe("only");
  });

  it("should handle zero-weight items", () => {
    const options = ["never", "always"];
    const weights = [0, 1];

    for (let i = 0; i < 50; i++) {
      expect(weightedRandom(options, weights)).toBe("always");
    }
  });

  it("trả về phần tử ĐẦU khi mọi trọng số bằng 0", () => {
    // total = 0 ⇒ r = 0 ⇒ điều kiện `r <= 0` khớp ngay vòng lặp đầu tiên.
    // Không có câu trả lời "đúng" cho đầu vào này, nhưng nó tất định — đáng
    // khoá lại để một lần đổi sang `r < 0` không âm thầm làm lệch phân phối.
    expect(weightedRandom(["a", "b", "c"], [0, 0, 0])).toBe("a");
  });
});

describe("avatarMath — Fixational Eye Micro-Saccades Math", () => {
  it("randomSaccadeInterval tuân thủ nghiêm ngặt dải tần số 2-4 Hz (0.25s - 0.50s)", () => {
    const intervals: number[] = [];
    for (let i = 0; i < 200; i++) {
      const interval = randomSaccadeInterval();
      expect(interval).toBeGreaterThanOrEqual(0.25);
      expect(interval).toBeLessThanOrEqual(0.50);
      intervals.push(interval);
    }
    // Giá trị trung bình phải nằm quanh 0.28s - 0.38s (tương đương ~3 Hz)
    const avg = intervals.reduce((s, v) => s + v, 0) / intervals.length;
    expect(avg).toBeGreaterThanOrEqual(0.28);
    expect(avg).toBeLessThanOrEqual(0.38);
  });

  it("randomSaccadeDisplacement kẹp chặt biên độ trong phạm vi ±0.85° (±0.0148 rad)", () => {
    for (let i = 0; i < 200; i++) {
      const disp = randomSaccadeDisplacement(MAX_SACCADE_AMPLITUDE_DEG);
      expect(Math.abs(disp.yaw)).toBeLessThanOrEqual(0.85);
      expect(Math.abs(disp.pitch)).toBeLessThanOrEqual(0.85 * 0.70 + 0.001);
      // Khoảng cách Euclidean không vượt quá bán kính tối đa
      const radius = Math.sqrt(disp.yaw * disp.yaw + disp.pitch * disp.pitch);
      expect(radius).toBeLessThanOrEqual(0.8501);
      // Kiểm tra giá trị radian tương ứng: 0.85° ≈ 0.014835 rad
      const rad = radius * (Math.PI / 180);
      expect(rad).toBeLessThanOrEqual(0.01484);
    }
  });

  it("smoothstep tạo đường cong nội suy Hermite mượt mà với vận tốc đầu cuối bằng 0", () => {
    expect(smoothstep(0, 1, 0)).toBe(0);
    expect(smoothstep(0, 1, 1)).toBe(1);
    expect(smoothstep(0, 1, 0.5)).toBe(0.5); // 0.25 * (3 - 1) = 0.5
    // Đạo hàm d/dt(3t^2 - 2t^3) = 6t(1 - t) > 0 với t trong (0, 1)
    expect(smoothstep(0, 1, 0.1)).toBeCloseTo(0.028, 3);
    expect(smoothstep(0, 1, 0.9)).toBeCloseTo(0.972, 3);
  });

  it("hằng số SACCADE_JUMP_DURATION_S và SACCADE_DRIFT_HALF_LIFE_S đạt chuẩn sinh học", () => {
    expect(SACCADE_JUMP_DURATION_S).toBe(0.025); // 25ms
    expect(SACCADE_DRIFT_HALF_LIFE_S).toBe(0.20); // 200ms
  });
});
