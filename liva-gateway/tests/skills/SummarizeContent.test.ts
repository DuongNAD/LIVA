import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn(),
}));

import { metadata, execute } from "../../src/skills/web/SummarizeContent";
import { safeFetch } from "../../src/utils/HttpClient";

describe("SummarizeContent", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("summarize_content");
        expect(metadata.parameters.properties.style.enum).toContain("bullet_points");
    });

    it("should reject when no url or text provided", async () => {
        const result = await execute({});
        expect(result).toContain("Error");
    });

    it("should reject content too short", async () => {
        const result = await execute({ text: "Short." });
        expect(result).toContain("too short");
    });

    it("should summarize raw text", async () => {
        (safeFetch as any).mockResolvedValueOnce({
            json: async () => ({ choices: [{ message: { content: "This is a summary." } }] }),
        });
        const result = await execute({ text: "A".repeat(100), style: "brief" });
        expect(result).toContain("Summary");
        expect(result).toContain("This is a summary.");
    });

    it("should fetch URL and summarize", async () => {
        // First call: fetch URL
        (safeFetch as any)
            .mockResolvedValueOnce({ text: async () => "<html><body><p>This is a very long article about artificial intelligence and machine learning that contains many important details about the future of technology.</p></body></html>" })
            // Second call: LLM summary
            .mockResolvedValueOnce({ json: async () => ({ choices: [{ message: { content: "Article summary." } }] }) });

        const result = await execute({ url: "https://example.com/article" });
        expect(result).toContain("Article summary.");
        expect(result).toContain("example.com");
    });

    it("should handle URL fetch failure", async () => {
        (safeFetch as any).mockRejectedValueOnce(new Error("DNS lookup failed"));
        const result = await execute({ url: "https://badurl.test" });
        expect(result).toContain("Failed to fetch");
    });
});
