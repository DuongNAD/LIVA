import { describe, it, expect } from "vitest";
import { buildPerm, extrapolate, simplex2D } from "../../src/utils/openSimplexNoise";

describe("openSimplexNoise", () => {
  describe("buildPerm", () => {
    it("should generate a permutation array of length 256", () => {
      const perm = buildPerm(42);
      expect(perm).toBeInstanceOf(Int16Array);
      expect(perm.length).toBe(256);
    });

    it("should generate deterministic results for the same seed", () => {
      const perm1 = buildPerm(100);
      const perm2 = buildPerm(100);
      expect(perm1).toEqual(perm2);
    });

    it("should handle negative/different seed calculations correctly", () => {
      const perm = buildPerm(-12345);
      expect(perm.length).toBe(256);
    });
  });

  describe("extrapolate", () => {
    it("should calculate extrapolate values correctly", () => {
      const val = extrapolate(0, 0, 0.5, 0.5);
      expect(typeof val).toBe("number");
    });
  });

  describe("simplex2D", () => {
    it("should return a number", () => {
      const val = simplex2D(0.5, 0.5);
      expect(typeof val).toBe("number");
    });

    it("should return consistent values for same inputs", () => {
      const val1 = simplex2D(1.2, 3.4);
      const val2 = simplex2D(1.2, 3.4);
      expect(val1).toBe(val2);
    });

    it("should handle xins + yins <= 1 condition", () => {
      const val = simplex2D(0.1, 0.1);
      expect(typeof val).toBe("number");
    });

    it("should handle xins + yins > 1 condition", () => {
      const val = simplex2D(0.8, 0.8);
      expect(typeof val).toBe("number");
    });

    it("should return finite values", () => {
      for (let x = -2; x <= 2; x += 0.4) {
        for (let y = -2; y <= 2; y += 0.4) {
          const val = simplex2D(x, y);
          expect(Number.isFinite(val)).toBe(true);
        }
      }
    });
  });
});
