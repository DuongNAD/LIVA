/**
 * WebSearch.test.ts — Test suite for WebSearch skill
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

// Mock safeFetch
const mockSafeFetch = vi.fn();
vi.mock("../../src/utils/HttpClient", () => ({
    safeFetch: (...args: any[]) => mockSafeFetch(...args),
}));

describe("WebSearch Skill", () => {
    let originalEnv: string | undefined;

    beforeEach(() => {
        vi.clearAllMocks();
        vi.resetModules();
        originalEnv = process.env.TAVILY_API_KEY;
    });

    afterEach(() => {
        if (originalEnv !== undefined) {
            process.env.TAVILY_API_KEY = originalEnv;
        } else {
            delete process.env.TAVILY_API_KEY;
        }
    });

    const loadSkill = async () => {
        return await import("../../src/skills/web/WebSearch");
    };

    describe("DuckDuckGo Fallback (No Tavily Key)", () => {
        beforeEach(() => {
            delete process.env.TAVILY_API_KEY;
        });

        it("should parse DDG HTML results correctly", async () => {
            const skill = await loadSkill();
            
            const mockHTML = `
                <div>
                    <a class="result__url" href="//duckduckgo.com/l/?uddg=https%3A%2F%2Fexample.com&rut=1">Example Title</a>
                    <a class="result__url" href="https://direct.com">Direct Title</a>
                </div>
            `;
            
            mockSafeFetch.mockResolvedValueOnce({
                text: () => Promise.resolve(mockHTML)
            });

            const result = await skill.execute({ query: "test query" });
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            expect(mockSafeFetch.mock.calls[0][0]).toContain("duckduckgo.com");
            expect(result).toContain("Top 2 bài viết");
            expect(result).toContain("Example Title");
            expect(result).toContain("https://example.com"); // decoded
            expect(result).toContain("Direct Title");
            expect(result).toContain("https://direct.com");
        });

        it("should return empty message if no results", async () => {
            const skill = await loadSkill();
            
            mockSafeFetch.mockResolvedValueOnce({
                text: () => Promise.resolve("<div>No results</div>")
            });

            const result = await skill.execute({ query: "test query" });
            expect(result).toContain("Không tìm thấy kết quả nào");
        });

        it("should return error message if fetch fails", async () => {
            const skill = await loadSkill();
            
            mockSafeFetch.mockRejectedValueOnce(new Error("Network Error"));

            const result = await skill.execute({ query: "test query" });
            expect(result).toContain("Lỗi tìm kiếm (Search error): Network Error");
        });
    });

    describe("Tavily API (With API Key)", () => {
        beforeEach(() => {
            process.env.TAVILY_API_KEY = "test-key";
        });

        it("should call Tavily API and format results correctly", async () => {
            const skill = await loadSkill();
            
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({
                    answer: "This is a summary answer.",
                    results: [
                        { title: "Res 1", url: "http://res1.com", content: "Content 1", score: 0.9 }
                    ]
                })
            });

            const result = await skill.execute({ query: "test query" });
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(1);
            expect(mockSafeFetch.mock.calls[0][0]).toContain("api.tavily.com");
            
            const body = JSON.parse(mockSafeFetch.mock.calls[0][1].body);
            expect(body.api_key).toBe("test-key");
            expect(body.query).toBe("test query");

            expect(result).toContain("[Web Search — Tavily]");
            expect(result).toContain("This is a summary answer");
            expect(result).toContain("Res 1");
            expect(result).toContain("http://res1.com");
            expect(result).toContain("Content 1");
        });

        it("should fallback to DuckDuckGo if Tavily fails", async () => {
            const skill = await loadSkill();
            
            // Tavily fails
            mockSafeFetch.mockRejectedValueOnce(new Error("Tavily Down"));
            
            // DDG succeeds
            const mockHTML = `<div><a class="result__url" href="https://fallback.com">Fallback Res</a></div>`;
            mockSafeFetch.mockResolvedValueOnce({
                text: () => Promise.resolve(mockHTML)
            });

            const result = await skill.execute({ query: "test query" });
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(2);
            expect(mockSafeFetch.mock.calls[0][0]).toContain("api.tavily.com");
            expect(mockSafeFetch.mock.calls[1][0]).toContain("duckduckgo.com");
            
            expect(result).toContain("[Web Search — DuckDuckGo Fallback]");
            expect(result).toContain("Fallback Res");
        });

        it("should report both errors if Tavily and fallback DDG both fail", async () => {
            const skill = await loadSkill();
            
            // Tavily fails
            mockSafeFetch.mockRejectedValueOnce(new Error("Tavily Down"));
            // DDG fails
            mockSafeFetch.mockRejectedValueOnce(new Error("DDG Down"));

            const result = await skill.execute({ query: "test query" });
            
            expect(mockSafeFetch).toHaveBeenCalledTimes(2);
            expect(result).toContain("Lỗi tìm kiếm (tất cả nguồn đều thất bại)");
            expect(result).toContain("Tavily Down");
            expect(result).toContain("DDG Down");
        });
        
        it("should handle Tavily result without answer or results", async () => {
            const skill = await loadSkill();
            
            mockSafeFetch.mockResolvedValueOnce({
                json: () => Promise.resolve({})
            });

            const result = await skill.execute({ query: "test query" });
            expect(result).toContain("[Web Search — Tavily]");
            expect(result).not.toContain("Tóm tắt AI");
            expect(result).not.toContain("Top");
        });
    });
});
