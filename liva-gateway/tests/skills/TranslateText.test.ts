import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

import { metadata, execute } from "../../src/skills/core/TranslateText";
import { safeFetch } from "../../src/utils/HttpClient";

describe("TranslateText", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("translate_text");
        expect(metadata.parameters.required).toContain("text");
        expect(metadata.parameters.required).toContain("target_language");
    });

    it("should reject empty text", async () => {
        const result = await execute({ text: "", target_language: "English" });
        expect(result).toContain("Error");
    });

    it("should reject missing target_language", async () => {
        const result = await execute({ text: "hello", target_language: "" });
        expect(result).toContain("Error");
    });

    it("should reject text exceeding max length", async () => {
        const longText = "A".repeat(3001);
        const result = await execute({ text: longText, target_language: "English" });
        expect(result).toContain("too long");
    });

    it("should call LLM and return translation", async () => {
        (safeFetch as any).mockResolvedValueOnce({
            json: async () => ({ choices: [{ message: { content: "Xin chào" } }] }),
        });
        const result = await execute({ text: "Hello", target_language: "Vietnamese" });
        expect(result).toContain("Xin chào");
        expect(result).toContain("Vietnamese");
    });

    it("should handle LLM failure", async () => {
        (safeFetch as any).mockRejectedValueOnce(new Error("Connection refused"));
        const result = await execute({ text: "Hello", target_language: "French" });
        expect(result).toContain("error");
    });
});
