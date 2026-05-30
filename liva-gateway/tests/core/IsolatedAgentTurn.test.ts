import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { IsolatedAgentTurn } from "../../src/core/IsolatedAgentTurn";
import { MemoryManager } from "../../src/MemoryManager";
import { SkillRegistry } from "../../src/SkillRegistry";
import OpenAI from "openai";
import { TaskQueue } from "../../src/core/TaskQueue";

vi.mock("../../src/MemoryManager");
vi.mock("../../src/SkillRegistry");
vi.mock("openai");
vi.mock("../../src/core/TaskQueue");

describe("IsolatedAgentTurn", () => {
    let mockMemory: MemoryManager;
    let mockRegistry: SkillRegistry;
    let mockAiClient: OpenAI;
    let isolatedTurn: IsolatedAgentTurn;

    beforeEach(() => {
        vi.clearAllMocks();

        mockMemory = {
            getSessionState: vi.fn().mockResolvedValue("mock state"),
            getUserProfile: vi.fn().mockResolvedValue(null),
            getLongTermMarkdown: vi.fn().mockResolvedValue("long term memory mock"),
            getStructuredMemoryPrompt: vi.fn().mockReturnValue("structured memory mock"),
            getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
            addMessage: vi.fn(),
            workingBuffer: {
                checkBudget: vi.fn().mockResolvedValue("budget mock"),
                appendHistory: vi.fn(),
                flush: vi.fn()
            },
            getHybridContext: vi.fn().mockResolvedValue([{ role: "system", content: "mock context" }])
        } as unknown as MemoryManager;

        mockRegistry = {
            getAllSkills: vi.fn().mockReturnValue([{ name: "test_tool", parameters: {} }]),
            getSemanticTopK: vi.fn().mockResolvedValue([{ name: "test_tool", description: "Test tool", parameters: {} }]),
            executeSkill: vi.fn().mockResolvedValue("tool result"),
        } as unknown as SkillRegistry;

        mockAiClient = {
            chat: {
                completions: {
                    create: vi.fn().mockResolvedValue({
                        choices: [{ message: { content: "Task completed successfully." } }]
                    })
                }
            }
        } as unknown as OpenAI;

        // Mock TaskQueue enqueue to execute task immediately
        const mockTaskQueue = {
            enqueue: vi.fn().mockImplementation((task) => task())
        };
        vi.mocked(TaskQueue.getInstance).mockReturnValue(mockTaskQueue as any);

        isolatedTurn = new IsolatedAgentTurn(mockMemory, mockRegistry, mockAiClient);
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should run background turn wrapped in TaskQueue", async () => {
        const result = await isolatedTurn.runBackgroundTurn("Clean up system logs");
        
        expect(TaskQueue.getInstance().enqueue).toHaveBeenCalled();
        expect(result).toBe("Task completed successfully.");
        expect(mockAiClient.chat.completions.create).toHaveBeenCalledTimes(1);
    });

    it("should parse XML tool calls and execute them without UI blocking", async () => {
        // First LLM call returns a tool call
        vi.mocked(mockAiClient.chat.completions.create).mockResolvedValueOnce({
            choices: [{ message: { content: "Here is the call: <tool_call>{\"name\": \"test_tool\", \"arguments\": {}}</tool_call>" } }]
        } as any);

        // Second LLM call returns final answer
        vi.mocked(mockAiClient.chat.completions.create).mockResolvedValueOnce({
            choices: [{ message: { content: "Final Report Done" } }]
        } as any);

        const result = await isolatedTurn.runBackgroundTurn("Run test tool");

        expect(mockRegistry.executeSkill).toHaveBeenCalledWith("test_tool", {});
        expect(result).toBe("Final Report Done");
        expect(mockAiClient.chat.completions.create).toHaveBeenCalledTimes(2);
    });

    it("should ignore malformed JSON inside tool_call tag", async () => {
        vi.mocked(mockAiClient.chat.completions.create).mockResolvedValueOnce({
            choices: [{ message: { content: "Bad call: <tool_call>{bad json}</tool_call>" } }]
        } as any);

        const result = await isolatedTurn.runBackgroundTurn("Test bad json");

        expect(mockRegistry.executeSkill).not.toHaveBeenCalled();
        // [v27] ToolCallExtractor preserves malformed JSON in cleaned content for visibility
        // (previously silently stripped the entire block)
        expect(result).toContain("Bad call:");
    });

    it("should handle tool execution failure", async () => {
        mockRegistry.executeSkill = vi.fn().mockRejectedValue(new Error("Tool failed"));

        vi.mocked(mockAiClient.chat.completions.create).mockResolvedValueOnce({
            choices: [{ message: { content: "<tool_call>{\"name\": \"test_tool\", \"arguments\": {}}</tool_call>" } }]
        } as any);
        vi.mocked(mockAiClient.chat.completions.create).mockResolvedValueOnce({
            choices: [{ message: { content: "Report: Tool failed" } }]
        } as any);

        const result = await isolatedTurn.runBackgroundTurn("Test tool failure");

        expect(result).toBe("Report: Tool failed");
        expect(mockAiClient.chat.completions.create).toHaveBeenCalledTimes(2);
    });

    it("should handle unexpected errors during the turn", async () => {
        vi.mocked(mockAiClient.chat.completions.create).mockRejectedValueOnce(new Error("API Error"));

        const result = await isolatedTurn.runBackgroundTurn("Test api failure");

        expect(result).toBe("Lỗi hệ thống: API Error");
    });
});
