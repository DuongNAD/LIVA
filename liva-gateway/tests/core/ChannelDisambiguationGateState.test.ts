import { describe, it, expect, vi, beforeEach } from "vitest";
import { AgentLoop } from "../../src/core/AgentLoop";
import { MemoryManager } from "../../src/MemoryManager";
import { SkillRegistry } from "../../src/SkillRegistry";
import { logger } from "../../src/utils/logger";
import {
    isAmbiguousChannel,
    buildPreferenceKey,
    buildPreferenceValue,
} from "../../src/core/ChannelDisambiguationGate";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
        child: vi.fn().mockReturnThis(),
    },
}));

// Mock ModelOrchestrator
vi.mock("../../src/core/ModelOrchestrator", () => {
    return {
        ModelOrchestrator: class {
            isReady() { return true; }
            startSingleExpert() { return Promise.resolve(); }
        }
    };
});

// Mock SemanticRouter
vi.mock("../../src/memory/SemanticRouter", () => {
    return {
        SemanticRouter: class {
            initialize() { return Promise.resolve(); }
            route() { return Promise.resolve({ route: "deep_reasoning" }); }
        }
    };
});

// Mock OpenAI
export const mockOpenAICreate = vi.fn();
vi.mock("openai", () => ({
    default: class OpenAI {
        chat = {
            completions: {
                create: mockOpenAICreate
            }
        }
    }
}));

// Helper to mock OpenAI streaming response
async function* makeMockStream(content: string) {
    yield { choices: [{ delta: { content } }] };
}

describe("ChannelDisambiguationGateState — Integration States", () => {
    let loop: AgentLoop;
    let mockMemory: any;
    let mockRegistry: any;

    beforeEach(() => {
        vi.clearAllMocks();

        mockMemory = {
            getStructuredMemoryPrompt: vi.fn().mockReturnValue(""),
            getHybridContext: vi.fn().mockReturnValue([]),
            addMessage: vi.fn(),
            updateLongTermMemory: vi.fn(),
            getUserProfile: vi.fn().mockResolvedValue({ language: "vi-VN" }),
            getLongTermMarkdown: vi.fn().mockReturnValue(""),
            getSessionState: vi.fn().mockResolvedValue(""),
            workingBuffer: { checkBudget: vi.fn().mockResolvedValue("") },
            getStructuredMemoryInstance: vi.fn().mockReturnValue({
                insertTurnNode: vi.fn(),
                getFact: vi.fn(),
                setFact: vi.fn()
            }),
            reflectionDaemon: { queueTurn: vi.fn() },
            consolidationCron: { touch: vi.fn() },
            getPreviousSessionContextPrompt: vi.fn().mockResolvedValue(""),
            clearSession: vi.fn(),
        };

        mockRegistry = {
            executeSkill: vi.fn(),
            getSemanticTopK: vi.fn().mockResolvedValue([]),
            getAllSkills: vi.fn().mockReturnValue([]),
        };

        process.env.LIVA_USE_NATIVE = "false";
        loop = new AgentLoop(mockMemory as any, mockRegistry as any);
    });

    it("should activate channel gate when messaging tool call is ambiguous", async () => {
        // Mock LLM to return an ambiguous messaging call
        mockOpenAICreate.mockResolvedValueOnce(makeMockStream(
            `<tool_call>{"name": "send_zalo_rpa", "arguments": {"targetName": "Khánh", "message": "mai đi chơi"}}</tool_call>`
        ));

        // Set up streams
        const streamChunks: string[] = [];
        loop.onStreamChunk = vi.fn().mockImplementation((chunk) => {
            streamChunks.push(chunk);
        });

        await (loop as any)._executeUserInput("nhắn tin cho Khánh hỏi mai đi chơi", false, false);
        
        // Wait for the background TaskLaneWorker to process the dispatched task
        await new Promise(r => setTimeout(r, 150));

        // Verify the gate intercepted and sent a clarification message
        expect(loop.onStreamChunk).toHaveBeenCalled();
        const fullClarificationText = streamChunks.join("");
        expect(fullClarificationText).toContain("Khánh");
        expect(fullClarificationText).toContain("💬 Zalo");
        expect(fullClarificationText).toContain("📘 Messenger");
        expect(fullClarificationText).toContain("📧 Email");
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Asking user to pick channel"));
    });

    it("should resolve and execute when user replies with a channel name", async () => {
        // 1. First trigger the gate
        mockOpenAICreate.mockResolvedValueOnce(makeMockStream(
            `<tool_call>{"name": "send_zalo_rpa", "arguments": {"targetName": "Khánh", "message": "mai đi chơi"}}</tool_call>`
        ));
        await (loop as any)._executeUserInput("nhắn tin cho Khánh", false, false);
        await new Promise(r => setTimeout(r, 150));
        
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Gate activated for"));

        // 2. Mock execute tool success
        mockRegistry.executeSkill.mockResolvedValue({ valid: true, resultStr: "Gửi zalo thành công" });

        // 3. User replies "Zalo"
        await (loop as any)._executeUserInput("Zalo", false, false);
        await new Promise(r => setTimeout(r, 150));

        // Verify it executed the Zalo tool and learned the preference
        expect(mockRegistry.executeSkill).toHaveBeenCalledWith("send_zalo_rpa", {
            targetName: "Khánh",
            message: "mai đi chơi"
        });
        expect(mockMemory.getStructuredMemoryInstance().setFact).toHaveBeenCalledWith(
            "channel_pref::khánh",
            "send_zalo_rpa:1",
            expect.objectContaining({ category: "channel_preference" })
        );
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Channel resolved: send_zalo_rpa"));
    });

    it("should cancel flow if user sends an unrelated query instead of channel choice", async () => {
        // 1. First trigger the gate
        mockOpenAICreate.mockResolvedValueOnce(makeMockStream(
            `<tool_call>{"name": "send_zalo_rpa", "arguments": {"targetName": "Khánh", "message": "mai đi chơi"}}</tool_call>`
        ));
        await (loop as any)._executeUserInput("nhắn tin cho Khánh", false, false);
        await new Promise(r => setTimeout(r, 150));
        
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Gate activated for"));

        // 2. User sends an unrelated query: "thời tiết hôm nay" (not a channel signal)
        mockOpenAICreate.mockResolvedValueOnce(makeMockStream("Thời tiết hôm nay nắng đẹp."));
        
        await (loop as any)._executeUserInput("thời tiết hôm nay thế nào", false, false);
        await new Promise(r => setTimeout(r, 150));

        // Verify the pending action was cancelled
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Discarding pending action"));
        expect(mockRegistry.executeSkill).not.toHaveBeenCalled();
    });

    it("should normalize Vietnamese diacritics and casings correctly for preference keys", () => {
        expect(buildPreferenceKey("Khánh")).toBe("channel_pref::khánh");
        expect(buildPreferenceKey("Lê Hoàng Dương")).toBe("channel_pref::lê hoàng dương");
        expect(buildPreferenceKey("  Thành  ")).toBe("channel_pref::thành");
    });

    it("should properly check bypass and gate triggers under different preference levels", () => {
        // No preference -> ambiguous
        expect(isAmbiguousChannel("nhắn tin", "send_zalo_rpa", "Khánh", null)).toBe(true);

        // Preference count < 3 -> ambiguous
        expect(isAmbiguousChannel("nhắn tin", "send_zalo_rpa", "Khánh", "send_zalo_rpa:2")).toBe(true);

        // Preference count >= 3 -> bypass (not ambiguous)
        expect(isAmbiguousChannel("nhắn tin", "send_zalo_rpa", "Khánh", "send_zalo_rpa:3")).toBe(false);

        // Preference count >= 3 but for a DIFFERENT tool -> still ambiguous
        expect(isAmbiguousChannel("nhắn tin", "send_zalo_rpa", "Khánh", "send_messenger_rpa:3")).toBe(true);
    });
});
