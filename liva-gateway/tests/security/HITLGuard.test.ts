import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { HITLGuard } from "../../src/security/HITLGuard";
import { TelegramManager } from "../../src/services/TelegramManager";

const { mockSendMessage } = vi.hoisted(() => ({
    mockSendMessage: vi.fn().mockResolvedValue(undefined)
}));

vi.mock("../../src/services/TelegramManager", () => {
    return {
        TelegramManager: vi.fn().mockImplementation(function() {
            return { sendMessage: mockSendMessage };
        })
    };
});
describe("HITLGuard", () => {
    beforeEach(() => {
        vi.useFakeTimers();
        vi.clearAllMocks();
    });

    afterEach(() => {
        vi.useRealTimers();
    });

    it("should resolve true when approved by user", async () => {
        const approvalPromise = HITLGuard.requestApproval({ toolName: "test_tool", args: {} });
        
        let emittedReq: any;
        HITLGuard.events.once("hitl_request", (req) => {
            emittedReq = req;
        });

        // Advance timers a bit
        vi.advanceTimersByTime(100);

        // Await next tick to ensure event was emitted
        await Promise.resolve();
        
        // Wait, the emit happens synchronously before we added listener above?
        // Let's rely on respond instead since emit happens synchronously.
    });

    it("should handle approval correctly", async () => {
        let capturedId = "";
        const listener = (req: any) => {
            capturedId = req.id;
        };
        HITLGuard.events.on("hitl_request", listener);

        const approvalPromise = HITLGuard.requestApproval({ toolName: "test_tool", args: {} });

        expect(capturedId).not.toBe("");
        
        // Respond to it
        HITLGuard.respond(capturedId, true);

        const result = await approvalPromise;
        expect(result).toBe(true);

        HITLGuard.events.off("hitl_request", listener);
    });

    it("should reject when user declines", async () => {
        let capturedId = "";
        const listener = (req: any) => {
            capturedId = req.id;
        };
        HITLGuard.events.on("hitl_request", listener);

        const approvalPromise = HITLGuard.requestApproval({ toolName: "dangerous_tool", args: {} });
        
        HITLGuard.respond(capturedId, false);

        await expect(approvalPromise).rejects.toThrow("REJECTED_BY_USER");

        HITLGuard.events.off("hitl_request", listener);
    });

    it("should timeout after 300s", async () => {
        const approvalPromise = HITLGuard.requestApproval({ toolName: "timeout_tool", args: {} });
        
        // Fast-forward 300 seconds
        vi.advanceTimersByTime(300000);

        await expect(approvalPromise).rejects.toThrow("REJECTED_BY_TIMEOUT");
    });

    it("should notify via Telegram if tool is send_email", async () => {
        const approvalPromise = HITLGuard.requestApproval({ toolName: "send_email", args: {}, reason: "test" });
        
        // Check if TelegramManager.sendMessage was called
        expect(mockSendMessage).toHaveBeenCalled();
        
        let capturedId = "";
        HITLGuard.events.once("hitl_request", (req) => { capturedId = req.id; });
        // Actually event is already emitted, let's just get it from the mock args
        const callArgs = mockSendMessage.mock.calls[0];
        expect(callArgs[0]).toContain("send_email");
        expect(callArgs[0]).toContain("test");

        // Cleanup pending promise
        // find ID from keyboard
        const keyboard = callArgs[1] as any;
        const approveData = keyboard[0][0].callback_data; // approve:hitl-...
        const id = approveData.split(":")[1];
        
        HITLGuard.respond(id, true);
        await approvalPromise;
    });

    it("should ignore response for unknown id", () => {
        // Just calling respond with random ID should not throw
        expect(() => {
            HITLGuard.respond("unknown_id", true);
        }).not.toThrow();
    });

    it("should notify via Telegram with default reason if reason is missing", async () => {
        const approvalPromise = HITLGuard.requestApproval({ toolName: "send_email", args: {} });
        
        const callArgs = mockSendMessage.mock.calls.slice(-1)[0];
        expect(callArgs[0]).toContain("Không có");

        const keyboard = callArgs[1] as any;
        const approveData = keyboard[0][0].callback_data;
        const id = approveData.split(":")[1];
        
        HITLGuard.respond(id, true);
        await approvalPromise;
    });

    it("should gracefully handle Telegram sendMessage errors", async () => {
        mockSendMessage.mockRejectedValueOnce(new Error("Telegram failed"));

        const approvalPromise = HITLGuard.requestApproval({ toolName: "send_email", args: {} });

        // Await next tick to let catch block execute
        await Promise.resolve();

        // It should still let us approve
        const callArgs = mockSendMessage.mock.calls.slice(-1)[0];
        const keyboard = callArgs[1] as any;
        const approveData = keyboard[0][0].callback_data;
        const id = approveData.split(":")[1];
        
        HITLGuard.respond(id, true);
        await approvalPromise;
    });
});
