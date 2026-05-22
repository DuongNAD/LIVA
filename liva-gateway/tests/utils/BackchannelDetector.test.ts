import { describe, it, expect } from "vitest";
import { isBackchannel } from "@utils/BackchannelDetector";

describe("BackchannelDetector — Two-Stage Barge-in Classifier", () => {
    // ============================================================
    // Rule 1: Empty / whitespace → backchannel
    // ============================================================
    describe("Rule 1: Empty/whitespace", () => {
        it("should classify empty string as backchannel", () => {
            expect(isBackchannel("")).toBe(true);
        });

        it("should classify whitespace-only as backchannel", () => {
            expect(isBackchannel("   ")).toBe(true);
        });

        it("should classify tab/newline as backchannel", () => {
            expect(isBackchannel("\t\n")).toBe(true);
        });
    });

    // ============================================================
    // Rule 2: Exact match — known Vietnamese fillers
    // ============================================================
    describe("Rule 2: Vietnamese filler words", () => {
        const vnFillers = ["ừm", "ừ", "ờ", "à", "ạ", "vâng", "dạ", "rồi", "hả", "hở", "ơ", "ê"];
        for (const filler of vnFillers) {
            it(`should classify "${filler}" as backchannel`, () => {
                expect(isBackchannel(filler)).toBe(true);
            });
        }

        it("should be case-insensitive", () => {
            expect(isBackchannel("OK")).toBe(true);
            expect(isBackchannel("Okay")).toBe(true);
        });

        it('should classify "tiếp đi" as backchannel', () => {
            expect(isBackchannel("tiếp đi")).toBe(true);
        });

        it('should classify "nói tiếp" as backchannel', () => {
            expect(isBackchannel("nói tiếp")).toBe(true);
        });

        it('should classify "cứ nói" as backchannel', () => {
            expect(isBackchannel("cứ nói")).toBe(true);
        });
    });

    // ============================================================
    // Rule 2: Exact match — English fillers
    // ============================================================
    describe("Rule 2: English filler words", () => {
        const enFillers = ["yeah", "yes", "yep", "no", "nope", "right", "sure", "mm", "mhm", "i see", "got it", "go on", "continue"];
        for (const filler of enFillers) {
            it(`should classify "${filler}" as backchannel`, () => {
                expect(isBackchannel(filler)).toBe(true);
            });
        }
    });

    // ============================================================
    // Rule 3: Filler sound patterns (ừừừm, àààà)
    // ============================================================
    describe("Rule 3: Filler sound patterns", () => {
        it('should classify "hmmmm" as backchannel', () => {
            expect(isBackchannel("hmmmm")).toBe(true);
        });

        it('should classify "uhhhh" as backchannel', () => {
            expect(isBackchannel("uhhhh")).toBe(true);
        });

        it('should classify "mmmmm..." as backchannel (trailing punctuation)', () => {
            expect(isBackchannel("mmmmm...")).toBe(true);
        });

        it('should classify "àààà" as backchannel (Vietnamese diacritics)', () => {
            expect(isBackchannel("àààà")).toBe(true);
        });

        it('should classify "ờờờ!" as backchannel', () => {
            expect(isBackchannel("ờờờ!")).toBe(true);
        });

        it('should classify "uh" as backchannel (exact match)', () => {
            expect(isBackchannel("uh")).toBe(true);
        });

        it('should classify "ah..." as backchannel', () => {
            expect(isBackchannel("ah...")).toBe(true);
        });
    });

    // ============================================================
    // Rule 4: Multi-word filler combinations (<3 words, <10 chars)
    // ============================================================
    describe("Rule 4: Multi-word fillers", () => {
        it('should classify "ừ ừ" as backchannel (2 fillers)', () => {
            expect(isBackchannel("ừ ừ")).toBe(true);
        });

        it('should classify "ok ok" as backchannel', () => {
            expect(isBackchannel("ok ok")).toBe(true);
        });
    });

    // ============================================================
    // Real speech — should NOT be classified as backchannel
    // ============================================================
    describe("Real speech (NOT backchannel)", () => {
        it("should classify full sentence as real speech", () => {
            expect(isBackchannel("Tôi muốn hỏi về thời tiết")).toBe(false);
        });

        it("should classify question as real speech", () => {
            expect(isBackchannel("Hôm nay trời thế nào?")).toBe(false);
        });

        it("should classify command as real speech", () => {
            expect(isBackchannel("Mở file báo cáo")).toBe(false);
        });

        it("should classify multi-word meaningful phrase as real speech", () => {
            expect(isBackchannel("Em giúp anh viết email")).toBe(false);
        });

        it("should classify 3+ word phrase as real speech even if short", () => {
            expect(isBackchannel("tôi muốn biết")).toBe(false);
        });

        it("should classify English sentence as real speech", () => {
            expect(isBackchannel("Please help me with this")).toBe(false);
        });
    });

    // ============================================================
    // Edge cases
    // ============================================================
    describe("Edge Cases", () => {
        it("should handle text with leading/trailing whitespace", () => {
            expect(isBackchannel("  ừm  ")).toBe(true);
        });

        it("should handle mixed case filler", () => {
            expect(isBackchannel("UHM")).toBe(true);
        });
    });
});
