/**
 * system_prompt.test.ts — Verify prompt generation
 */
import { describe, it, expect } from "vitest";
import { getBaseSystemPrompt } from "../../src/system_prompt";

describe("getBaseSystemPrompt", () => {
    it("should return a non-empty string", () => {
        const prompt = getBaseSystemPrompt();
        expect(prompt).toBeDefined();
        expect(typeof prompt).toBe("string");
        expect(prompt.length).toBeGreaterThan(100);
    });

    it("should contain the AI name 'Liva'", () => {
        const prompt = getBaseSystemPrompt();
        expect(prompt).toContain("Liva");
    });

    it("should contain security instructions about external data", () => {
        const prompt = getBaseSystemPrompt();
        expect(prompt).toContain("EXTERNAL_DATA_START");
        expect(prompt).toContain("BẢO MẬT");
    });

    it("should contain current time info", () => {
        const prompt = getBaseSystemPrompt();
        // The prompt injects a formatted time string using vi-VN locale
        expect(prompt).toContain("THÔNG TIN THỜI GIAN");
    });

    it("should contain tool usage instructions", () => {
        const prompt = getBaseSystemPrompt();
        expect(prompt).toContain("tool_call");
    });
});
