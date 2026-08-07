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
