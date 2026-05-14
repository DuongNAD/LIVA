import { describe, it, expect, vi, beforeEach } from "vitest";
import { PromptBuilder } from "../../src/core/PromptBuilder";
import { MemoryManager } from "../../src/MemoryManager";
import { SensoryManager } from "../../src/memory/SensoryManager";
import { HeraCompass } from "../../src/memory/HeraCompass";

vi.mock("../../src/MemoryManager");
vi.mock("../../src/memory/SensoryManager");
vi.mock("../../src/memory/HeraCompass");

describe("PromptBuilder", () => {
    let memoryManager: vi.Mocked<MemoryManager>;
    let sensoryManager: vi.Mocked<SensoryManager>;
    let heraCompass: vi.Mocked<HeraCompass>;

    beforeEach(() => {
        vi.clearAllMocks();
        
        memoryManager = new MemoryManager(null as any) as any;
        sensoryManager = new SensoryManager() as any;
        heraCompass = new HeraCompass(null as any) as any;

        memoryManager.getUserProfile = vi.fn().mockResolvedValue({ name: "User", current_location: "" });
        memoryManager.getStructuredMemoryPrompt = vi.fn().mockReturnValue("Structured memory block");
        memoryManager.getLongTermMarkdown = vi.fn().mockResolvedValue("Long term memory content of sufficient length................................");
        memoryManager.getSessionState = vi.fn().mockResolvedValue("Current session state is active");
        memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue({ vecReady: false, searchAnchors: vi.fn().mockReturnValue([]) });
        memoryManager.workingBuffer = { checkBudget: vi.fn().mockResolvedValue("Budget: OK") } as any;
        memoryManager.getHybridContext = vi.fn().mockResolvedValue([]);

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
            
            // Total budget is 6000. L3+L1 = 5950. Remaining is 50. Session is 200.
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

        it("should inject L2 anchors if FF_ENABLE_L2_INJECTION is true", async () => {
            process.env.FF_ENABLE_L2_INJECTION = "true";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchAnchors: vi.fn().mockReturnValue(["Semantic Anchor 1", "Semantic Anchor 2"])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            // Mock the dynamic import of EmbeddingService
            vi.doMock("../../src/services/EmbeddingService", () => ({
                EmbeddingService: {
                    getInstance: vi.fn().mockReturnValue({
                        embed: vi.fn().mockResolvedValue(new Array(384).fill(0.1))
                    })
                }
            }));

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).toContain("<context_memory>");
            expect(context).toContain("Semantic Anchor 1");
            
            delete process.env.FF_ENABLE_L2_INJECTION;
        });

        it("should handle L2 timeout gracefully", async () => {
            process.env.FF_ENABLE_L2_INJECTION = "true";
            
            const structuredMemoryMock = {
                vecReady: true,
                searchAnchors: vi.fn().mockReturnValue([])
            };
            memoryManager.getStructuredMemoryInstance = vi.fn().mockReturnValue(structuredMemoryMock);

            // Mock EmbeddingService to simulate timeout
            vi.doMock("../../src/services/EmbeddingService", () => ({
                EmbeddingService: {
                    getInstance: vi.fn().mockReturnValue({
                        embed: vi.fn().mockImplementation(() => new Promise((resolve) => setTimeout(resolve, 2000)))
                    })
                }
            }));

            const context = await PromptBuilder.buildContextPrompt(memoryManager, "Hanoi", sensoryManager, "factual_recall", "Search term");
            
            expect(context).not.toContain("<context_memory>");
            
            delete process.env.FF_ENABLE_L2_INJECTION;
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
            
            const messages = await PromptBuilder.prepareFullAiMessages("Hi", memoryManager, { location: "Location", timezone: "Asia/Ho_Chi_Minh" }, tools);
            
            expect(messages.length).toBe(2);
            expect(messages[0].role).toBe("system");
            expect(messages[0].content).toContain("Budget: OK");
            expect(messages[1].role).toBe("user");
        });
    });
});
