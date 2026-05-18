/**
 * system_prompt.test.ts — Verify prompt generation
 */
import { describe, it, expect } from "vitest";
import { getBaseSystemPrompt, SystemContext } from "../../src/system_prompt";

const mockContext: SystemContext = {
    name: "Dương",
    birthYear: "2000",
    nationality: "Việt Nam",
    language: "vi-VN",
    hobbies: "Coding, AI research",
    aiTone: "Friendly",
    location: "Hanoi",
    timezone: "Asia/Ho_Chi_Minh",
};

describe("getBaseSystemPrompt", () => {
    it("should return a non-empty string", () => {
        const prompt = getBaseSystemPrompt(mockContext);
        expect(prompt).toBeDefined();
        expect(typeof prompt).toBe("string");
        expect(prompt.length).toBeGreaterThan(100);
    });

    it("should contain the AI name 'Liva'", () => {
        const prompt = getBaseSystemPrompt(mockContext);
        expect(prompt).toContain("Liva");
    });

    it("should contain security constraints section", () => {
        const prompt = getBaseSystemPrompt(mockContext);
        expect(prompt).toContain("<SECURITY_CONSTRAINTS>");
        expect(prompt).toContain("SYSTEM_INTEGRITY");
    });

    it("should contain user profile info from context", () => {
        const prompt = getBaseSystemPrompt(mockContext);
        // The prompt injects user profile from context
        expect(prompt).toContain("<CONTEXT>");
        expect(prompt).toContain("Việt Nam");
    });

    it("should contain tool usage instructions", () => {
        const prompt = getBaseSystemPrompt(mockContext);
        expect(prompt).toContain("TOOL CALLING");
    });
});
