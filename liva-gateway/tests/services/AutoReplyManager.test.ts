import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

// Mock livaEngine
const mockCreateCompletion = vi.fn().mockResolvedValue({
    choices: [{ message: { content: "Drafted AI Response" } }]
});
vi.mock("../../src/utils/LivaEngine", () => ({
    livaEngine: {
        chat: {
            completions: {
                create: (...args: any[]) => mockCreateCompletion(...args)
            }
        }
    }
}));

// Mock SkillRegistry
const mockExecuteSkill = vi.fn().mockResolvedValue("Executed Skill Result");
vi.mock("../../src/SkillRegistry", () => ({
    SkillRegistry: {
        getInstance: () => ({
            executeSkill: mockExecuteSkill
        })
    }
}));

// Mock ConfigManager
const mockGet = vi.fn().mockReturnValue({
    LIVA_AUTO_RESPONDER_ENABLED: true
});
const mockGetLivaConfig = vi.fn().mockResolvedValue({
    autoReply: {
        enabled: true,
        rules: [
            {
                channel: "zalo",
                senderFilter: "Boss",
                instructions: "Nói em bận họp",
                mode: "hitl"
            },
            {
                channel: "email",
                senderFilter: "*",
                instructions: "Auto email response",
                mode: "autonomous"
            }
        ]
    }
});
vi.mock("../../src/core/config/ConfigManager", () => ({
    ConfigManager: {
        getInstance: () => ({
            get: mockGet,
            getLivaConfig: mockGetLivaConfig
        })
    }
}));

// Mock HITLGuard
const mockRequestApproval = vi.fn().mockResolvedValue(true);
vi.mock("../../src/security/HITLGuard", () => ({
    HITLGuard: {
        requestApproval: (...args: any[]) => mockRequestApproval(...args)
    }
}));

import { AutoReplyManager } from "../../src/services/AutoReplyManager";
import { ChannelRouter, NormalizedMessage } from "../../src/channels/ChannelNormalizer";
import { SessionOrchestrator } from "../../src/core/SessionOrchestrator";

describe("AutoReplyManager", () => {
    let manager: AutoReplyManager;
    let mockChannelRouter: ChannelRouter;
    let mockAdapter: any;
    let sessions: SessionOrchestrator;

    beforeEach(() => {
        vi.useFakeTimers();

        mockAdapter = {
            sendText: vi.fn().mockResolvedValue(undefined)
        };
        mockChannelRouter = {
            getAdapter: vi.fn().mockReturnValue(mockAdapter),
            register: vi.fn(),
            replyText: vi.fn(),
            sendApproval: vi.fn(),
            getRegisteredChannels: vi.fn()
        } as unknown as ChannelRouter;

        sessions = new SessionOrchestrator();
        manager = new AutoReplyManager(mockChannelRouter, sessions);

        mockCreateCompletion.mockClear();
        mockExecuteSkill.mockClear();
        mockRequestApproval.mockClear();
        mockAdapter.sendText.mockClear();
        mockGet.mockReturnValue({ LIVA_AUTO_RESPONDER_ENABLED: true });
        mockGetLivaConfig.mockResolvedValue({
            autoReply: {
                enabled: true,
                rules: [
                    {
                        channel: "zalo",
                        senderFilter: "Boss",
                        instructions: "Nói em bận họp",
                        mode: "hitl"
                    },
                    {
                        channel: "email",
                        senderFilter: "*",
                        instructions: "Auto email response",
                        mode: "autonomous"
                    }
                ]
            }
        });
    });

    afterEach(() => {
        vi.useRealTimers();
        sessions.dispose();
    });

    it("should return false and do nothing when responder is globally disabled", async () => {
        mockGet.mockReturnValue({ LIVA_AUTO_RESPONDER_ENABLED: false });
        
        const msg: NormalizedMessage = {
            channel: "zalo",
            senderId: "boss_zalo_id",
            senderName: "Boss",
            text: "Cần báo cáo gấp",
            timestamp: Date.now(),
            rawPayload: {}
        };

        const result = await manager.handleIncomingMessage(msg);
        expect(result).toBe(false);
        expect(mockCreateCompletion).not.toHaveBeenCalled();
    });

    it("should match Zalo Boss rule, debounce 8s, consolidate messages, and request HITL approval", async () => {
        const msg1: NormalizedMessage = {
            channel: "zalo",
            senderId: "boss_zalo_id",
            senderName: "Boss",
            text: "Alo em",
            timestamp: Date.now(),
            rawPayload: {}
        };

        const msg2: NormalizedMessage = {
            channel: "zalo",
            senderId: "boss_zalo_id",
            senderName: "Boss",
            text: "Có báo cáo chưa?",
            timestamp: Date.now(),
            rawPayload: {}
        };

        // 1. Send first message
        let result = await manager.handleIncomingMessage(msg1);
        expect(result).toBe(true); // Should intercept
        expect(mockCreateCompletion).not.toHaveBeenCalled(); // Debouncing, not processed yet

        // Advance timers by 4s (should not trigger yet)
        await vi.advanceTimersByTimeAsync(4000);
        expect(mockCreateCompletion).not.toHaveBeenCalled();

        // 2. Send second message from same sender (resets debounce timer)
        result = await manager.handleIncomingMessage(msg2);
        expect(result).toBe(true);

        // Advance timers by another 5s (total 9s since first, but 5s since second, so should not trigger)
        await vi.advanceTimersByTimeAsync(5000);
        expect(mockCreateCompletion).not.toHaveBeenCalled();

        // Advance another 3s (triggers 8s from second message)
        await vi.advanceTimersByTimeAsync(3000);

        // Verify completion generated with consolidated text
        expect(mockCreateCompletion).toHaveBeenCalledTimes(1);
        const completionCallArgs = mockCreateCompletion.mock.calls[0][0];
        expect(completionCallArgs.messages[1].content).toContain("Alo em\nCó báo cáo chưa?");

        // Verify HITL request was sent with combined context
        expect(mockRequestApproval).toHaveBeenCalledTimes(1);
        expect(mockRequestApproval).toHaveBeenCalledWith({
            toolName: "auto_reply",
            args: {
                channel: "zalo",
                recipient: "Boss",
                incomingMessage: "Có báo cáo chưa?",
                draftReply: "Drafted AI Response"
            },
            reason: `[Tự động trả lời] Gửi tin nhắn đến Boss (ZALO) với nội dung: "Drafted AI Response"`
        });

        // Verify history is stored in SessionOrchestrator
        const history = sessions.getSessionHistory(`zalo_boss_zalo_id`);
        expect(history.length).toBe(2);
        expect(history[0].text).toBe("Alo em\nCó báo cáo chưa?");
        expect(history[1].text).toBe("Drafted AI Response");
    });

    it("should match Email rule, trigger LLM, and execute autonomous reply_email skill", async () => {
        const msg: NormalizedMessage = {
            channel: "email",
            senderId: "test@example.com",
            senderName: "Partner",
            text: "Hello Liva",
            timestamp: Date.now(),
            rawPayload: { uid: 123 }
        };

        const result = await manager.handleIncomingMessage(msg);
        expect(result).toBe(true);

        // Trigger debounce timeout
        await vi.advanceTimersByTimeAsync(8000);

        expect(mockCreateCompletion).toHaveBeenCalledTimes(1);
        expect(mockRequestApproval).not.toHaveBeenCalled();
        expect(mockExecuteSkill).toHaveBeenCalledWith("reply_email", {
            originalUid: 123,
            body_text: "Drafted AI Response",
            bypassHITL: true
        });
    });

    it("should retrieve session history and inject it as context in subsequent LLM prompts", async () => {
        const sessionId = `zalo_boss_zalo_id`;
        sessions.getOrCreateSession("boss_zalo_id", "zalo");
        
        // Populate existing history
        sessions.appendMessage(sessionId, {
            channel: "zalo", senderId: "boss_zalo_id", senderName: "Boss", text: "Hi", timestamp: Date.now() - 10000, rawPayload: {}
        });
        sessions.appendMessage(sessionId, {
            channel: "zalo", senderId: "ai", senderName: "LIVA", text: "Hello Boss", timestamp: Date.now() - 5000, rawPayload: {}
        });

        const msg: NormalizedMessage = {
            channel: "zalo",
            senderId: "boss_zalo_id",
            senderName: "Boss",
            text: "Are you busy?",
            timestamp: Date.now(),
            rawPayload: {}
        };

        await manager.handleIncomingMessage(msg);
        await vi.advanceTimersByTimeAsync(8000);

        expect(mockCreateCompletion).toHaveBeenCalledTimes(1);
        const completionCallArgs = mockCreateCompletion.mock.calls[0][0];
        
        // Assert system prompt includes the conversational history
        const systemPrompt = completionCallArgs.messages[0].content;
        expect(systemPrompt).toContain("LỊCH SỬ TRÒ CHUYỆN GẦN ĐÂY:");
        expect(systemPrompt).toContain("Hi");
        expect(systemPrompt).toContain("Hello Boss");
    });

    it("should ignore messages from owner to prevent loops", async () => {
        vi.stubEnv("TELEGRAM_CHAT_ID", "12345");
        
        const msg: NormalizedMessage = {
            channel: "telegram",
            senderId: "12345",
            senderName: "Owner",
            text: "Ping Liva",
            timestamp: Date.now(),
            rawPayload: {}
        };

        const result = await manager.handleIncomingMessage(msg);
        expect(result).toBe(false);
        expect(mockCreateCompletion).not.toHaveBeenCalled();
    });
});
