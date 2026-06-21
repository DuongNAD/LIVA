import { describe, it, expect, vi, beforeEach } from "vitest";
import { execute as evaluateMath } from "../../src/skills/core/EvaluateMath";
import { execute as convertTimezone } from "../../src/skills/core/TimezoneConverter";
import { execute as dictionaryLookup } from "../../src/skills/core/DictionaryLookup";
import { GeminiAPI } from "../../src/tools/GeminiAPI";

vi.mock("../../src/utils/logger", () => ({
  logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/tools/GeminiAPI", () => ({
  GeminiAPI: {
    generateStructured: vi.fn(),
  },
}));

describe("Skill Verification Stress Tests", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  describe("EvaluateMath Edge Cases", () => {
    it("should handle division by zero or nested division by zero", async () => {
      const res1 = await evaluateMath({ expression: "10 / 0" });
      expect(res1).toContain("Error: Division by zero");

      const res2 = await evaluateMath({ expression: "2 / (3 - 3)" });
      expect(res2).toContain("Error: Division by zero");

      const res3 = await evaluateMath({ expression: "0 / 0" });
      expect(res3).toContain("Error: Division by zero");
    });

    it("should handle modulo by zero", async () => {
      const res1 = await evaluateMath({ expression: "10 % 0" });
      expect(res1).toContain("Error: Modulo by zero");

      const res2 = await evaluateMath({ expression: "2 % (3 - 3)" });
      expect(res2).toContain("Error: Modulo by zero");
    });

    it("should reject forbidden characters", async () => {
      const res1 = await evaluateMath({ expression: "2 + 3; alert(1)" });
      expect(res1).toContain("Error: Expression contains forbidden characters.");

      const res2 = await evaluateMath({ expression: "sin(pi) # comment" });
      expect(res2).toContain("Error: Expression contains forbidden characters.");
    });

    it("should handle invalid numbers and expressions gracefully", async () => {
      const res1 = await evaluateMath({ expression: "2.3.4" });
      expect(res1).toContain("Error");

      const res2 = await evaluateMath({ expression: "(2 + 3" });
      expect(res2).toContain("Error");

      const res3 = await evaluateMath({ expression: "2 + 3)" });
      expect(res3).toContain("Error");

      const res4 = await evaluateMath({ expression: "sin()" });
      expect(res4).toContain("Error");
    });

    it("should show behavior for math edge cases returning NaN or Infinity", async () => {
      // Math.log10(-10) -> NaN in JS, let's see how EvaluateMath handles it
      const res1 = await evaluateMath({ expression: "log(-10)" });
      expect(res1).toBe("Error: The calculation resulted in an undefined or infinite value.");

      // Math.pow(0, -1) -> Infinity in JS
      const res2 = await evaluateMath({ expression: "0 ^ -1" });
      expect(res2).toBe("Error: The calculation resulted in an undefined or infinite value.");
    });

    it("should handle extreme nested parentheses", async () => {
      // Create a string like (((((...((1))...)))))
      const depth = 200;
      const expr = "(".repeat(depth) + "1" + ")".repeat(depth);
      const res = await evaluateMath({ expression: expr });
      expect(res).toBe("Error: Maximum recursion depth exceeded");

      const depth50 = 50;
      const expr50 = "(".repeat(depth50) + "1" + ")".repeat(depth50);
      const res50 = await evaluateMath({ expression: expr50 });
      expect(res50).toBe("1");
    });
  });

  describe("TimezoneConverter Edge Cases", () => {
    it("should handle invalid timezone formats", async () => {
      const res1 = await convertTimezone({ targetTimezone: "Invalid/Tz" });
      expect(res1).toContain("Error: Invalid target timezone");

      const res2 = await convertTimezone({ targetTimezone: "UTC", sourceTimezone: "Invalid/Source" });
      expect(res2).toContain("Error: Invalid source timezone");
    });

    it("should handle invalid date-time formats", async () => {
      const res1 = await convertTimezone({ targetTimezone: "UTC", dateTimeStr: "not-a-date" });
      expect(res1).toContain("Error: Invalid date-time format");
    });

    it("should handle date-time with offset correctly", async () => {
      const res1 = await convertTimezone({
        targetTimezone: "America/New_York",
        dateTimeStr: "2026-06-21T12:00:00+07:00"
      });
      // 12:00+07:00 is 05:00 UTC, which in NY (EDT, UTC-4) is 01:00:00
      expect(res1).toContain("2026-06-21 01:00:00");
    });

    it("should handle out-of-bounds naive dates by standard JS Date wrapping", async () => {
      const res1 = await convertTimezone({
        targetTimezone: "UTC",
        sourceTimezone: "UTC",
        dateTimeStr: "2026-02-31 12:00:00"
      });
      // Feb 31 wraps to March 3
      expect(res1).toContain("2026-03-03 12:00:00");
    });

    it("should handle non-standard format parsing (system local timezone behavior)", async () => {
      // 2026/06/21 is not matching regexes, falls back to new Date()
      // Let's test if it successfully converts
      const res = await convertTimezone({
        targetTimezone: "UTC",
        sourceTimezone: "America/New_York",
        dateTimeStr: "2026/06/21"
      });
      expect(res).toContain("Error: Invalid date-time format");
    });
  });

  describe("DictionaryLookup Edge Cases", () => {
    it("should handle empty or whitespace word", async () => {
      const res1 = await dictionaryLookup({ word: "" });
      expect(res1).toContain("Error");

      const res2 = await dictionaryLookup({ word: "   " });
      expect(res2).toContain("Error");
    });

    it("should handle empty API response", async () => {
      (GeminiAPI.generateStructured as any).mockResolvedValueOnce({});
      const res = await dictionaryLookup({ word: "test" });
      expect(res).toContain("Failed to look up word");
    });

    it("should handle API failure", async () => {
      (GeminiAPI.generateStructured as any).mockRejectedValueOnce(new Error("API Connection Failed"));
      const res = await dictionaryLookup({ word: "test" });
      expect(res).toContain("Error: API Connection Failed");
    });

    it("should handle missing partsOfSpeech definitions/examples defensively if they return null/undefined", async () => {
      const mockResult = {
        word: "test",
        phonetics: "/test/",
        partsOfSpeech: [
          {
            pos: "noun",
            // definitions and examples missing (undefined)
          }
        ],
        synonyms: [],
        antonyms: [],
        translation: "thử nghiệm"
      };

      (GeminiAPI.generateStructured as any).mockResolvedValueOnce(mockResult);

      const res = await dictionaryLookup({ word: "test" });
      expect(res).toContain("# Dictionary Lookup: **test**");
      expect(res).not.toContain("Error");
    });

    it("should handle null partsOfSpeech and other top level fields safely", async () => {
      const mockResult = {
        word: "test",
        phonetics: "/test/",
        partsOfSpeech: null as any,
        synonyms: null as any,
        antonyms: null as any,
        translation: null as any
      };

      (GeminiAPI.generateStructured as any).mockResolvedValueOnce(mockResult);

      const res = await dictionaryLookup({ word: "test" });
      // Null partsOfSpeech, synonyms, antonyms, translation do not cause a crash
      // because they are guarded or short-circuited.
      expect(res).toContain("# Dictionary Lookup: **test**");
      expect(res).toContain("Phonetics:* `/test/`");
      expect(res).not.toContain("Error");
    });

    it("should handle null element in partsOfSpeech safely without crashing", async () => {
      const mockResult = {
        word: "test",
        phonetics: "/test/",
        partsOfSpeech: [null as any],
        synonyms: [],
        antonyms: [],
        translation: "thử nghiệm"
      };

      (GeminiAPI.generateStructured as any).mockResolvedValueOnce(mockResult);

      const res = await dictionaryLookup({ word: "test" });
      expect(res).toContain("# Dictionary Lookup: **test**");
      expect(res).not.toContain("Error");
    });
  });
});
