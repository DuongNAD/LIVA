import { describe, it, expect, vi, beforeEach } from "vitest";
import type { Mocked } from "vitest";
import { PromptBuilder } from "../../src/core/PromptBuilder";
import { MemoryManager } from "../../src/MemoryManager";
import { SensoryManager } from "../../src/memory/SensoryManager";
import { HeraCompass } from "../../src/memory/HeraCompass";

vi.mock("../../src/MemoryManager");
vi.mock("../../src/memory/SensoryManager");
vi.mock("../../src/memory/HeraCompass");

const mockEmbed = vi.fn();
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            embed: (...args: any[]) => mockEmbed(...args)
        })
    }
}));
// [Phase 1] Mock compression service so budget tests have deterministic char counts
vi.mock("../../src/memory/TokenCompressionService", () => ({
    TokenCompressionService: {
        getInstance: () => ({
            compress: async (text: string) => ({
                compressedText: text,
                originalTokens: Math.ceil(text.length / 4),
                compressedTokens: Math.ceil(text.length / 4),
                compressionRatio: 1.0,
                strategy: 'none',
            }),
        }),
    },
    estimateTokens: (text: string) => {
        if (!text) return 0;
        return Math.ceil(text.trim().split(/\s+/).filter((w: string) => w.length > 0).length * 1.5);
    },
}));

const mockEmbed = vi.fn().mockResolvedValue(new Array(384).fill(0.1));
const mockEmbedBatch = vi.fn().mockImplementation(async (texts: string[]) => {
    return texts.map(() => new Array(384).fill(0.1));
});
vi.mock("../../src/services/EmbeddingService", () => ({
    EmbeddingService: {
        getInstance: () => ({
            embed: mockEmbed,
            embedBatch: mockEmbedBatch,
        }),
    },
}));

describe("PromptBuilder", () => {
    let memoryManager: Mocked<MemoryManager>;
    let sensoryManager: Mocked<SensoryManager>;
    let heraCompass: Mocked<HeraCompass>;

    beforeEach(() => {
        vi.clearAllMocks();
        mockEmbed.mockReset();
        mockEmbed.mockResolvedValue(new Array(384).fill(0.1));
        mockEmbedBatch.mockReset();
        mockEmbedBatch.mockImplementation(async (texts: string[]) => {
            return texts.map(() => new Array(384).fill(0.1));
        });
        
        memoryManager = new MemoryManager(null as any) as any;
        sensoryManager = new (SensoryManager as any)() as any;
        heraCompass = new (HeraCompass as any)(null as any) as any;

        memoryManager.getUserProfile = vi.fn().mockResolvedValue({ name: "User", current_location: "" });
        memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue("Structured memory block");
        memoryManager.getLongTermMarkdown = vi.fn().mockResolvedValue("Long term memory content of sufficient length................................");
        memoryManager.getSessionState = vi.fn().mockResolvedValue("Current session state is active");
        memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue({
            vecReady: false,
            searchHybridVectors: vi.fn().mockResolvedValue([]),
            searchAnchors: vi.fn().mockReturnValue([]),
            searchAnchorsWithScores: vi.fn().mockReturnValue([])
        });
        memoryManager.workingBuffer = { checkBudget: vi.fn().mockResolvedValue("Budget: OK") } as any;
        memoryManager.getHybridContext = vi.fn().mockResolvedValue([]);
        memoryManager.getPreviousSessionContextPrompt = vi.fn().mockResolvedValue("\n\n<PREVIOUS_SESSION_CONTEXT>\nMock previous turns\n</PREVIOUS_SESSION_CONTEXT>\n");

        sensoryManager.injectSensoryPrompt = vi.fn().mockReturnValue("[Sensory: Everything is fine]");

        SensoryManager.getInstance = vi.fn().mockReturnValue(sensoryManager);
        HeraCompass.getInstance = vi.fn().mockReturnValue(heraCompass);
    });

    describe("buildContextPrompt", () => {
        it("should return fast-exit for chitchat", async () => {
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "chitchat");
            expect(context).toContain("<USER_PROFILE>");
            expect(context).toContain("[Sensory:");
            expect(context).not.toContain("<LONG_TERM_MEMORY>");
        });

        it("should return fast-exit for system_command", async () => {
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "system_command");
            expect(context).toContain("<USER_PROFILE>");
            expect(context).toContain("[Sensory:");
            expect(context).not.toContain("<LONG_TERM_MEMORY>");
        });

        it("should combine all layers for full pipeline", async () => {
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            expect(context).toContain("<USER_PROFILE>");
            expect(context).toContain("Structured memory block");
            expect(context).toContain("<LONG_TERM_MEMORY>");
            expect(context).toContain("<SESSION_STATE>");
            expect(context).toContain("[Sensory: Everything is fine]");
        });

        it("should gracefully handle null user profile", async () => {
            memoryManager.getUserProfile = vi.fn().mockResolvedValue(null);
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            expect(context).not.toContain("<USER_PROFILE>");
        });

        it("should truncate session prompt gracefully if exceeding budget", async () => {
            memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue("A".repeat(5000));
            memoryManager.getLongTermMarkdown = vi.fn().mockResolvedValue("B".repeat(950));
            memoryManager.getSessionState = vi.fn().mockResolvedValue("C".repeat(200));

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            
            // Total budget is 6000. L3+L1 = 5950 (compression mocked as no-op). Remaining is 50. Session is 200.
            // It should truncate session.
            const sessionMatch = context.match(/<SESSION_STATE>\n([\s\S]*?)\n<\/SESSION_STATE>/);
            if (sessionMatch) {
                expect(sessionMatch[1].length).toBeLessThanOrEqual(50);
            }
        });

        it("should truncate session prompt at sentence boundary gracefully", async () => {
            // Budget is 6000
            const l3 = "A".repeat(5930);
            memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue(l3);
            memoryManager.getLongTermMarkdown = vi.fn().mockResolvedValue(""); // L1 empty
            
            // L3 + \n + L1 = 5931. Remaining budget ~ 69.
            // Wrapper for session is \n\n[TRẠNG THÁI PHIÊN...]\n -> ~ 40 chars
            // The session itself is "This is a sentence. And another one."
            memoryManager.getSessionState = vi.fn().mockResolvedValue("This is a sentence. And another one that is very long and should be cut.");

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            
            expect(context).toContain("This is a sentence.");
            expect(context).not.toContain("And another one");
        });

        it("should return rough truncated string if cutPoint is too early", async () => {
            // Target remainingBudget = 100.
            const l3 = "A".repeat(5899);
            memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue(l3);
            memoryManager.getLongTermMarkdown = vi.fn().mockResolvedValue("");
            
            // L3 + \n + L1 = 5900. Remaining = 100.
            // Wrapper is 40 chars. 100 * 0.5 = 50.
            // If session state has no newlines or periods, lastNewline is at index 39.
            // 39 > 50 is false.
            memoryManager.getSessionState = vi.fn().mockResolvedValue("B".repeat(200));

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            
            expect(context).toContain("B".repeat(60)); // 100 - 40 = 60 chars of B
        });

        it("should completely drop session prompt if remaining budget <= 0", async () => {
            memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue("A".repeat(6500));
            
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall");
            expect(context).not.toContain("<SESSION_STATE>");
        });

        it("should inject L2 anchors in CORRECT tier if best score is >= LIVA_CRAG_CORRECT_THRESHOLD", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.6";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.3";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: vi.fn().mockResolvedValue([
                    { content: "Semantic Anchor 1", score: 0.9, vecId: "vec_123", domain: "Code", createdAt: 1718000000000 },
                    { content: "Semantic Anchor 2", score: 0.8, vecId: "vec_456", domain: "Personal", createdAt: 1718000000000 }
                ])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            mockEmbed.mockResolvedValue(new Array(384).fill(0.1));
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).toContain("<context_memory>");
            expect(context).toContain("<chunk vec_id=\"vec_123\" domain=\"Code\" created_at=\"2024-06-10T06:13:20.000Z\">Semantic Anchor 1</chunk>");
            expect(context).not.toContain("memory_status");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should inject L2 anchors with ambiguity warning in AMBIGUOUS tier", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.6";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.3";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: vi.fn().mockResolvedValue([
                    { content: "Semantic Anchor 1", score: 0.45, vecId: "vec_123", domain: "Code", createdAt: 1718000000000 }
                ])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).toContain("<context_memory>");
            expect(context).toContain("<memory_status type=\"ambiguous\">");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should skip memory injection and inject incorrect status in INCORRECT tier", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.6";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.3";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: vi.fn().mockResolvedValue([
                    { content: "Semantic Anchor 1", score: 0.25, vecId: "vec_123", domain: "Code", createdAt: 1718000000000 }
                ])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).not.toContain("<context_memory>");
            expect(context).toContain("<memory_status type=\"incorrect\">");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should honor custom threshold environment variables", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.8";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.5";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: vi.fn().mockResolvedValue([
                    { content: "Semantic Anchor 1", score: 0.7, vecId: "vec_123", domain: "Code", createdAt: 1718000000000 }
                ])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            // Score 0.7 falls under 0.8 but is >= 0.5. So it should be AMBIGUOUS.
            expect(context).toContain("<context_memory>");
            expect(context).toContain("<memory_status type=\"ambiguous\">");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should handle L2 timeout gracefully", async () => {
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: vi.fn().mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 2000)))
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            mockEmbed.mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 2000)));
            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).not.toContain("<context_memory>");
        });

        it("should apply route-adaptive weights correctly", async () => {
            const searchHybridVectorsMock = vi.fn().mockResolvedValue([]);
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: searchHybridVectorsMock
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            // Test factual_recall route
            await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            expect(searchHybridVectorsMock).toHaveBeenCalledWith(
                "Search term",
                expect.any(Array),
                20,
                "ANCHOR",
                { dense: 0.4, sparse: 0.6 }
            );

            searchHybridVectorsMock.mockClear();

            // Test deep_reasoning route
            await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "deep_reasoning", "Search term");
            expect(searchHybridVectorsMock).toHaveBeenCalledWith(
                "Search term",
                expect.any(Array),
                20,
                "ANCHOR",
                { dense: 0.7, sparse: 0.3 }
            );

            searchHybridVectorsMock.mockClear();

            // Test other routes
            await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "news_briefing", "Search term");
            expect(searchHybridVectorsMock).not.toHaveBeenCalled();
        });

        it("should perform query decomposition for ambiguous queries", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.6";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.3";

            // Main query: "học máy và xử lý ngôn ngữ tự nhiên"
            // Splits on " và " into: "học máy", "xử lý ngôn ngữ tự nhiên"
            const searchHybridVectorsMock = vi.fn()
                .mockImplementation(async (queryText) => {
                    if (queryText === "học máy và xử lý ngôn ngữ tự nhiên") {
                        return [{ content: "Main Result", score: 0.25, vecId: "main_vec", domain: "AI", createdAt: 1718000000000 }];
                    }
                    if (queryText === "học máy") {
                        return [{ content: "Sub-query Result 1", score: 0.55, vecId: "sub_1", domain: "AI", createdAt: 1718000000000 }];
                    }
                    if (queryText === "xử lý ngôn ngữ tự nhiên") {
                        return [{ content: "Sub-query Result 2", score: 0.5, vecId: "sub_2", domain: "AI", createdAt: 1718000000000 }];
                    }
                    return [];
                });

            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: searchHybridVectorsMock
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "học máy và xử lý ngôn ngữ tự nhiên");

            // Verify query splitting and parallel search logic
            expect(searchHybridVectorsMock).toHaveBeenCalledWith("học máy", expect.any(Array), expect.any(Number), "ANCHOR", expect.any(Object));
            expect(searchHybridVectorsMock).toHaveBeenCalledWith("xử lý ngôn ngữ tự nhiên", expect.any(Array), expect.any(Number), "ANCHOR", expect.any(Object));

            // It should contain results from sub-queries in context
            expect(context).toContain("Sub-query Result 1");
            expect(context).toContain("Sub-query Result 2");
            expect(context).toContain("<memory_status type=\"ambiguous\">");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should perform query decomposition and classify as CORRECT (no ambiguity warning) when sub-query results have high scores", async () => {
            process.env.LIVA_CRAG_CORRECT_THRESHOLD = "0.6";
            process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD = "0.3";

            const searchHybridVectorsMock = vi.fn()
                .mockImplementation(async (queryText) => {
                    if (queryText === "học máy và xử lý ngôn ngữ tự nhiên") {
                        return [{ content: "Main Result", score: 0.45, vecId: "main_vec", domain: "AI", createdAt: 1718000000000 }];
                    }
                    if (queryText === "học máy") {
                        return [{ content: "Sub-query Result 1", score: 0.85, vecId: "sub_1", domain: "AI", createdAt: 1718000000000 }];
                    }
                    if (queryText === "xử lý ngôn ngữ tự nhiên") {
                        return [{ content: "Sub-query Result 2", score: 0.75, vecId: "sub_2", domain: "AI", createdAt: 1718000000000 }];
                    }
                    return [];
                });

            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: searchHybridVectorsMock
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "học máy và xử lý ngôn ngữ tự nhiên");

            expect(context).toContain("Sub-query Result 1");
            expect(context).toContain("Sub-query Result 2");
            expect(context).not.toContain("<memory_status type=\"ambiguous\">");
            expect(context).not.toContain("<memory_status type=\"incorrect\">");
            
            delete process.env.LIVA_CRAG_CORRECT_THRESHOLD;
            delete process.env.LIVA_CRAG_AMBIGUOUS_THRESHOLD;
        });

        it("should call embedBatch for parallel batch embeddings when query is decomposed", async () => {
            const searchHybridVectorsMock = vi.fn().mockResolvedValue([]);
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: searchHybridVectorsMock
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "học máy và xử lý ngôn ngữ tự nhiên");

            expect(mockEmbedBatch).toHaveBeenCalledWith(["học máy", "xử lý ngôn ngữ tự nhiên"]);
        });

        it("should handle embedBatch timeout/error gracefully", async () => {
            mockEmbedBatch.mockRejectedValueOnce(new Error("Timeout"));
            const searchHybridVectorsMock = vi.fn().mockResolvedValue([]);
            const structuredMemoryMock = {
                vecReady: true,
                searchHybridVectors: searchHybridVectorsMock
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "học máy và xử lý ngôn ngữ tự nhiên");
            expect(context).toBeDefined();
        });
    });

    describe("decomposeQuery", () => {
        it("should normalize NFC and split by sentence boundaries and coordinators", () => {
            const queries = PromptBuilder.decomposeQuery("Học máy và xử lý ngôn ngữ tự nhiên. Học sâu hoặc thị giác máy tính?");
            expect(queries).toEqual([
                "Học máy",
                "xử lý ngôn ngữ tự nhiên",
                "Học sâu",
                "thị giác máy tính"
            ]);
        });

        it("should handle prefix inheritance for interrogative prefixes and dangling clauses", () => {
            const queries = PromptBuilder.decomposeQuery("làm thế nào để học máy và tối ưu hóa nó?");
            expect(queries).toEqual([
                "làm thế nào để học máy",
                "làm thế nào để tối ưu hóa nó"
            ]);
        });

        it("should handle prefix inheritance for subject pronouns", () => {
            const queries = PromptBuilder.decomposeQuery("tôi muốn học Svelte và anh muốn học React");
            expect(queries).toEqual([
                "tôi muốn học Svelte",
                "anh muốn học React"
            ]);
        });

        it("should filter short or numeric-only parts", () => {
            const queries = PromptBuilder.decomposeQuery("học máy và 12345 và abc và tối ưu nó");
            expect(queries).toEqual([
                "học máy",
                "tối ưu nó"
            ]);
        });
    });

    describe("buildToolsPrompt", () => {
        it("should build tools prompt from cache or fresh", () => {
            const tools = [{ name: "test_tool", parameters: {} }];
            const res1 = PromptBuilder.buildToolsPrompt("Hello", tools);
            expect(res1).toContain("test_tool");
            expect(res1).toContain("handoff_to_expert");

            const res2 = PromptBuilder.buildToolsPrompt("Hello", tools);
            expect(res1).toBe(res2); // cached
        });

        it("should inject HeraCompass insights if available", () => {
            heraCompass.getRelatedInsight = vi.fn().mockReturnValue([{ actionable_rule: "Do not do X", tool_target: "test_tool" }]);
            
            const tools = [{ name: "test_tool", parameters: {} }];
            const res = PromptBuilder.buildToolsPrompt("New Hello", tools);
            
            expect(res).toContain("<EXPERIENCE_WARNINGS>");
            expect(res).toContain("Do not do X");
        });
        
        it("should safely ignore if HeraCompass throws", () => {
            heraCompass.getRelatedInsight = vi.fn().mockImplementation(() => { throw new Error("Not initialized"); });
            
            const tools = [{ name: "test_tool", parameters: {} }];
            const res = PromptBuilder.buildToolsPrompt("Another Hello", tools);
            
            expect(res).not.toContain("<EXPERIENCE_WARNINGS>");
        });

        it("should handle empty user text without crashing", () => {
            const tools = [{ name: "test_tool", parameters: {} }];
            const res = PromptBuilder.buildToolsPrompt("", tools);
            expect(res).toContain("test_tool");
        });
    });

    describe("prepareFullAiMessages", () => {
        it("should combine all contexts into messages", async () => {
            memoryManager.getHybridContext = vi.fn().mockResolvedValue([{ role: "user", content: "Hi" }]);
            const tools = [{ name: "tool", parameters: {} }];
            
            const result = await PromptBuilder.prepareFullAiMessages("Hi", memoryManager, { location: "Location", timezone: "Asia/Ho_Chi_Minh" }, tools);
            const messages = result.aiMessages;
            
            expect(messages.length).toBe(2);
            expect(messages[0].role).toBe("system");
            expect(messages[0].content).not.toContain("Budget: OK");
            expect(result.dynamicContextBlock).toContain("Budget: OK");
            expect(messages[1].role).toBe("user");
        });
    });
});
