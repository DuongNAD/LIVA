/**
 * HITLGuard.test.ts — Human-in-the-Loop Safety Guard Tests
 * ==========================================================
 * Tests:
 * - Approval request emission
 * - User approval flow (resolve)
 * - User rejection flow (reject)
 * - Timeout auto-rejection (60s)
 * - Duplicate/expired response handling
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

// ============================================================
// Mocks
// ============================================================
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { HITLGuard, type HITLRequest } from "../../src/security/HITLGuard";

describe("HITLGuard — Human-in-the-Loop Safety", () => {
    beforeEach(() => {
        vi.useFakeTimers();
        // Clear any pending requests
        (HITLGuard as any).pendingRequests.clear();
    });

    afterEach(() => {
        vi.useRealTimers();
        (HITLGuard as any).pendingRequests.clear();
    });

    describe("requestApproval", () => {
        it("should emit hitl_request event with tool details", async () => {
            const emitSpy = vi.fn();
            HITLGuard.events.on("hitl_request", emitSpy);

            const promise = HITLGuard.requestApproval({
                toolName: "execute_command",
                args: { command: "rm -rf /tmp/test" },
                reason: "Destructive command detected"
            });

            expect(emitSpy).toHaveBeenCalledOnce();
            const emittedReq = emitSpy.mock.calls[0][0] as HITLRequest;
            expect(emittedReq.toolName).toBe("execute_command");
            expect(emittedReq.id).toMatch(/^hitl-/);

            // Respond to prevent dangling timeout causing unhandled rejection
            HITLGuard.respond(emittedReq.id, true);
            await promise;

            HITLGuard.events.removeListener("hitl_request", emitSpy);
        });

        it("should resolve with true when user approves", async () => {
            let capturedId: string = "";
            HITLGuard.events.on("hitl_request", (req: HITLRequest) => {
                capturedId = req.id;
            });

            const promise = HITLGuard.requestApproval({
                toolName: "send_zalo_bot",
                args: { message: "Hello" },
            });

            // Simulate user approval after emit
            await vi.advanceTimersByTimeAsync(10);
            HITLGuard.respond(capturedId, true);

            const result = await promise;
            expect(result).toBe(true);

            HITLGuard.events.removeAllListeners("hitl_request");
        });

        it("should reject with REJECTED_BY_USER when user denies", async () => {
            let capturedId: string = "";
            HITLGuard.events.on("hitl_request", (req: HITLRequest) => {
                capturedId = req.id;
            });

            const promise = HITLGuard.requestApproval({
                toolName: "execute_command",
                args: { command: "shutdown -s" },
            });

            await vi.advanceTimersByTimeAsync(10);
            HITLGuard.respond(capturedId, false);

            await expect(promise).rejects.toThrow("REJECTED_BY_USER");

            HITLGuard.events.removeAllListeners("hitl_request");
        });

        it("should auto-reject after 60s timeout", async () => {
            const promise = HITLGuard.requestApproval({
                toolName: "dangerous_tool",
                args: {},
            });

            // Attach catch handler BEFORE advancing timers to prevent unhandled rejection
            const rejectCatcher = promise.catch((err: Error) => err);

            // Fast-forward 60 seconds
            await vi.advanceTimersByTimeAsync(60001);

            const error = await rejectCatcher;
            expect(error).toBeInstanceOf(Error);
            expect(error.message).toBe("REJECTED_BY_TIMEOUT");
        });

        it("should clear timeout when user responds before timeout", async () => {
            let capturedId: string = "";
            HITLGuard.events.on("hitl_request", (req: HITLRequest) => {
                capturedId = req.id;
            });

            const promise = HITLGuard.requestApproval({
                toolName: "test_tool",
                args: {},
            });

            // Respond before timeout
            await vi.advanceTimersByTimeAsync(1000);
            HITLGuard.respond(capturedId, true);

            const result = await promise;
            expect(result).toBe(true);

            // Pending requests should be cleaned up
            expect((HITLGuard as any).pendingRequests.size).toBe(0);

            HITLGuard.events.removeAllListeners("hitl_request");
        });
    });

    describe("respond", () => {
        it("should handle response for non-existent ID gracefully", () => {
            // Should not throw
            HITLGuard.respond("nonexistent-id", true);
        });

        it("should handle double-response gracefully", async () => {
            let capturedId: string = "";
            HITLGuard.events.on("hitl_request", (req: HITLRequest) => {
                capturedId = req.id;
            });

            const promise = HITLGuard.requestApproval({
                toolName: "test",
                args: {},
            });

            await vi.advanceTimersByTimeAsync(10);

            // First response
            HITLGuard.respond(capturedId, true);
            await promise;

            // Second response — should not crash
            HITLGuard.respond(capturedId, false);

            HITLGuard.events.removeAllListeners("hitl_request");
        });
    });
});
