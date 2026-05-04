import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../../src/utils/HttpClient", () => ({
    safeFetch: vi.fn()
}));

describe("Skill - WebSearch", () => {
    let originalEnv: NodeJS.ProcessEnv;
    let safeFetchMock: any;

    beforeEach(async () => {
        originalEnv = { ...process.env };
        vi.clearAllMocks();
        vi.resetModules();
        safeFetchMock = (await import("../../../src/utils/HttpClient")).safeFetch;
    });

    afterEach(() => {
        process.env = originalEnv;
    });

    it("should export correct metadata", async () => {
        const { metadata } = await import("../../../src/skills/web/WebSearch");
        expect(metadata.name).toBe("web_search");
        expect(metadata.parameters.required).toContain("query");
    });

    it("should use Tavily API when TAVILY_API_KEY is present and return AI summary with results", async () => {
        process.env.TAVILY_API_KEY = "test-tavily-key";
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockResolvedValueOnce({
            json: async () => ({
                answer: "AI Summary Test",
                results: [
                    { title: "Test Article", url: "http://test.com", content: "Test content", score: 1.0 }
                ]
            })
        });

        const result = await execute({ query: "Test Query" });

        expect(safeFetchMock).toHaveBeenCalledWith(
            "https://api.tavily.com/search",
            expect.objectContaining({
                method: "POST",
                body: expect.stringContaining("test-tavily-key")
            }),
            15000
        );
        expect(result).toContain("Tóm tắt AI: AI Summary Test");
        expect(result).toContain("Test Article");
        expect(result).toContain("http://test.com");
    });

    it("should use Tavily API but handle missing answer and results", async () => {
        process.env.TAVILY_API_KEY = "test-tavily-key";
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockResolvedValueOnce({
            json: async () => ({})
        });

        const result = await execute({ query: "Empty Query" });
        expect(result).toContain("[Web Search — Tavily] Kết quả cho \"Empty Query\":");
        expect(result).not.toContain("Tóm tắt AI");
    });

    it("should fallback to DuckDuckGo when TAVILY_API_KEY is not set", async () => {
        delete process.env.TAVILY_API_KEY;
        const { execute } = await import("../../../src/skills/web/WebSearch");

        const mockHtml = `
            <a class="result__url" href="//duckduckgo.com/l/?uddg=http%3A%2F%2Fexample.com&amp;rut=test">Example Title</a>
        `;
        safeFetchMock.mockResolvedValueOnce({
            text: async () => mockHtml
        });

        const result = await execute({ query: "DDG Query" });

        expect(safeFetchMock).toHaveBeenCalledWith(
            expect.stringContaining("html.duckduckgo.com/html"),
            expect.any(Object),
            10000
        );
        expect(result).toContain("Example Title");
        expect(result).toContain("http://example.com");
    });

    it("should fallback to DuckDuckGo and handle missing link decoding", async () => {
        delete process.env.TAVILY_API_KEY;
        const { execute } = await import("../../../src/skills/web/WebSearch");

        const mockHtml = `
            <a class="result__url" href="http://direct.com">Direct Title</a>
            <a class="result__url" href="//duckduckgo.com/l/?uddg=bad%link">Bad Link Title</a>
        `;
        safeFetchMock.mockResolvedValueOnce({
            text: async () => mockHtml
        });

        const result = await execute({ query: "DDG Query" });
        expect(result).toContain("Direct Title");
        expect(result).toContain("http://direct.com");
        expect(result).toContain("Bad Link Title");
    });

    it("should return empty message when DuckDuckGo finds no results", async () => {
        delete process.env.TAVILY_API_KEY;
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockResolvedValueOnce({
            text: async () => "<html>Empty HTML</html>"
        });

        const result = await execute({ query: "No Results" });
        expect(result).toBe("Không tìm thấy kết quả nào trên web cho \"No Results\".");
    });

    it("should fallback to DuckDuckGo when Tavily fails", async () => {
        process.env.TAVILY_API_KEY = "test-tavily-key";
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockRejectedValueOnce(new Error("Tavily Error"));
        safeFetchMock.mockResolvedValueOnce({
            text: async () => `<a class="result__url" href="http://fallback.com">Fallback Title</a>`
        });

        const result = await execute({ query: "Fallback Query" });
        expect(result).toContain("Fallback Title");
    });

    it("should return error when both Tavily and DuckDuckGo fail", async () => {
        process.env.TAVILY_API_KEY = "test-tavily-key";
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockRejectedValueOnce(new Error("Tavily Error"));
        safeFetchMock.mockRejectedValueOnce(new Error("DDG Error"));

        const result = await execute({ query: "Double Fail Query" });
        expect(result).toContain("Lỗi tìm kiếm (tất cả nguồn đều thất bại): Tavily: Tavily Error | DDG: DDG Error");
    });

    it("should return error when DDG fails and no Tavily key", async () => {
        delete process.env.TAVILY_API_KEY;
        const { execute } = await import("../../../src/skills/web/WebSearch");

        safeFetchMock.mockRejectedValueOnce(new Error("DDG Network Error"));

        const result = await execute({ query: "Single Fail Query" });
        expect(result).toBe("Lỗi tìm kiếm (Search error): DDG Network Error");
    });
});
