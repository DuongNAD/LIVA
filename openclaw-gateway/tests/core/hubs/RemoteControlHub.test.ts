import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() }
}));

import { RemoteControlHub } from "../../../src/core/hubs/RemoteControlHub";
import type { DependencyContainer } from "../../../src/core/DependencyContainer";

describe("RemoteControlHub", () => {
    let mockDeps: any;
    let hub: RemoteControlHub;
    let telegramHandlers: Record<string, Function>;
    let metaHandlers: Record<string, Function>;
    let cdpHandlers: Record<string, Function>;
    let approvalHandlers: Record<string, Function>;

    beforeEach(() => {
        vi.clearAllMocks();

        telegramHandlers = {};
        metaHandlers = {};
        cdpHandlers = {};
        approvalHandlers = {};

        mockDeps = {
            telegram: {
                on: vi.fn((event, handler) => { telegramHandlers[event] = handler; })
            },
            meta: {
                on: vi.fn((event, handler) => { metaHandlers[event] = handler; })
            },
            cdpBridge: {
                on: vi.fn((event, handler) => { cdpHandlers[event] = handler; }),
                isConnected: vi.fn().mockReturnValue(true),
                clickApprovalButton: vi.fn().mockResolvedValue(undefined)
            },
            securityGateway: {
                validateIncoming: vi.fn().mockReturnValue(null), // null = allowed
                classifyRisk: vi.fn().mockReturnValue("low")
            },
            sessions: {
                getOrCreateSession: vi.fn().mockReturnValue({ id: "session123" }),
                appendMessage: vi.fn()
            },
            nlTranslator: {
                translate: vi.fn().mockResolvedValue({ action: "test_action", confidence: 0.9 })
            },
            approvalEngine: {
                createApproval: vi.fn().mockReturnValue("approval123"),
                forwardToChannel: vi.fn().mockResolvedValue(undefined),
                resolveApproval: vi.fn(),
                on: vi.fn((event, handler) => { approvalHandlers[event] = handler; })
            },
            dispatch: vi.fn().mockResolvedValue(undefined),
            getDefaultRemoteSenderId: vi.fn().mockReturnValue("owner")
        };

        hub = new RemoteControlHub(mockDeps as unknown as DependencyContainer);
        hub.wireListeners();
    });

    describe("Telegram Pipeline", () => {
        it("should block message if security gateway rejects it", async () => {
            mockDeps.securityGateway.validateIncoming.mockReturnValue("Blocked sender");
            
            await telegramHandlers["message"]({ channel: "telegram", senderId: "bad_guy", text: "hi", senderName: "Bad" });
            
            expect(mockDeps.sessions.getOrCreateSession).not.toHaveBeenCalled();
            expect(mockDeps.dispatch).not.toHaveBeenCalled();
        });

        it("should process valid telegram message and dispatch agent_input", async () => {
            await telegramHandlers["message"]({ channel: "telegram", senderId: "good_guy", text: "hello", senderName: "Good" });
            
            expect(mockDeps.sessions.appendMessage).toHaveBeenCalled();
            expect(mockDeps.dispatch).toHaveBeenCalledWith("agent_input", expect.stringContaining("hello"));
        });
    });

    describe("Meta Pipeline", () => {
        it("should block meta message if security gateway rejects it", async () => {
            mockDeps.securityGateway.validateIncoming.mockReturnValue("Blocked");
            await metaHandlers["message"]({ channel: "meta", senderId: "bad", text: "hi", senderName: "Bad" });
            expect(mockDeps.dispatch).not.toHaveBeenCalled();
        });

        it("should process valid meta message", async () => {
            await metaHandlers["message"]({ channel: "meta", senderId: "good", text: "hey", senderName: "Good" });
            expect(mockDeps.dispatch).toHaveBeenCalledWith("agent_input", expect.stringContaining("hey"));
        });

        it("should handle meta postback for approval", async () => {
            await metaHandlers["postback"]({ senderId: "good", payload: "approve:123" });
            expect(mockDeps.approvalEngine.resolveApproval).toHaveBeenCalledWith("123", true);
        });
    });

    describe("CDP Pipeline", () => {
        it("should create approval and forward to UI and Telegram when IDE requests approval", async () => {
            await cdpHandlers["approval_required"]({ text: "rm -rf /", selector: ".btn" });
            
            expect(mockDeps.approvalEngine.createApproval).toHaveBeenCalled();
            expect(mockDeps.approvalEngine.forwardToChannel).toHaveBeenCalled();
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", expect.objectContaining({
                name: "exec_approval_required"
            }));
        });
        
        it("should handle forward to Telegram failure without crashing", async () => {
            mockDeps.approvalEngine.forwardToChannel.mockRejectedValue(new Error("Net fail"));
            await cdpHandlers["approval_required"]({ text: "safe", selector: ".btn" });
            // Should still broadcast to UI
            expect(mockDeps.dispatch).toHaveBeenCalledWith("ui_broadcast", expect.any(Object));
        });
    });

    describe("Approval Engine Events", () => {
        it("should click IDE button on approval granted", async () => {
            await approvalHandlers["approval_granted"]({ source: "antigravity" });
            expect(mockDeps.cdpBridge.clickApprovalButton).toHaveBeenCalledWith(true);
        });

        it("should click IDE reject button on approval denied", async () => {
            await approvalHandlers["approval_denied"]({ source: "antigravity" });
            expect(mockDeps.cdpBridge.clickApprovalButton).toHaveBeenCalledWith(false);
        });
    });
});
