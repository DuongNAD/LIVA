/**
 * ApprovalEngine.test.ts — HITL Approval Flow Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { ApprovalEngine } from "../../src/core/ApprovalEngine";
import type { ChannelAdapter } from "../../src/channels/ChannelNormalizer";

describe("ApprovalEngine", () => {
    let engine: ApprovalEngine;

    beforeEach(() => {
        vi.useFakeTimers();
        engine = new ApprovalEngine(60_000); // 60s TTL for tests
    });

    afterEach(() => {
        engine.dispose();
        vi.useRealTimers();
    });

    describe("createApproval", () => {
        it("should create a pending approval with unique ID", () => {
            const id = engine.createApproval("antigravity", "rm -rf /tmp", "Cleaning temp files");
            expect(id).toBeTruthy();
            expect(engine.getApproval(id)).toBeDefined();
            expect(engine.getApproval(id)?.source).toBe("antigravity");
        });

        it("should track risk level", () => {
            const id = engine.createApproval("vscode", "npm install", "Installing deps", "moderate");
            expect(engine.getApproval(id)?.risk).toBe("moderate");
        });

        it("should increment pending count", () => {
            expect(engine.pendingCount).toBe(0);
            engine.createApproval("antigravity", "cmd1", "ctx1");
            engine.createApproval("vscode", "cmd2", "ctx2");
            expect(engine.pendingCount).toBe(2);
        });
    });

    describe("resolveApproval", () => {
        it("should resolve approval as approved", () => {
            const handler = vi.fn();
            engine.on("approval_granted", handler);

            const id = engine.createApproval("antigravity", "git push", "Pushing code");
            engine.resolveApproval(id, true);

            expect(handler).toHaveBeenCalledTimes(1);
            expect(handler.mock.calls[0][0].approved).toBe(true);
            expect(handler.mock.calls[0][0].resolvedAt).toBeDefined();
        });

        it("should resolve approval as rejected", () => {
            const handler = vi.fn();
            engine.on("approval_denied", handler);

            const id = engine.createApproval("antigravity", "rm -rf /", "Deleting everything");
            engine.resolveApproval(id, false);

            expect(handler).toHaveBeenCalledTimes(1);
            expect(handler.mock.calls[0][0].approved).toBe(false);
        });

        it("should not resolve already resolved approval", () => {
            const handler = vi.fn();
            engine.on("approval_granted", handler);

            const id = engine.createApproval("vscode", "cmd", "ctx");
            engine.resolveApproval(id, true);
            engine.resolveApproval(id, false); // Should be ignored

            expect(handler).toHaveBeenCalledTimes(1);
        });

        it("should handle non-existent approval gracefully", () => {
            expect(() => engine.resolveApproval("nonexistent", true)).not.toThrow();
        });
    });

    describe("forwardToChannel", () => {
        it("should call channel adapter's sendApprovalCard", async () => {
            const mockAdapter: ChannelAdapter = {
                channelName: "telegram",
                sendText: vi.fn(),
                sendApprovalCard: vi.fn().mockResolvedValue(undefined),
                sendScreenshot: vi.fn(),
            };

            const id = engine.createApproval("antigravity", "npm run build", "Building project", "moderate");
            await engine.forwardToChannel(id, mockAdapter, "123456");

            expect(mockAdapter.sendApprovalCard).toHaveBeenCalledTimes(1);
            expect(mockAdapter.sendApprovalCard).toHaveBeenCalledWith(
                "123456",
                expect.stringContaining("ANTIGRAVITY"),
                expect.stringContaining("npm run build"),
                id
            );

            // Should track forwarding metadata
            const approval = engine.getApproval(id);
            expect(approval?.forwardedTo).toBe("telegram");
            expect(approval?.forwardedSenderId).toBe("123456");
        });

        it("should format emojis correctly for dangerous and safe risks", async () => {
            const mockAdapter: ChannelAdapter = {
                channelName: "telegram",
                sendText: vi.fn(),
                sendApprovalCard: vi.fn().mockResolvedValue(undefined),
                sendScreenshot: vi.fn(),
            };

            const idDanger = engine.createApproval("antigravity", "rm -rf", "ctx", "dangerous");
            await engine.forwardToChannel(idDanger, mockAdapter, "123");
            expect(mockAdapter.sendApprovalCard).toHaveBeenCalledWith(
                "123",
                expect.stringContaining("🔴"),
                expect.stringContaining("rm -rf"),
                idDanger
            );

            mockAdapter.sendApprovalCard = vi.fn().mockResolvedValue(undefined);
            const idSafe = engine.createApproval("antigravity", "ls", "ctx", "safe");
            await engine.forwardToChannel(idSafe, mockAdapter, "123");
            expect(mockAdapter.sendApprovalCard).toHaveBeenCalledWith(
                "123",
                expect.stringContaining("🟢"),
                expect.stringContaining("ls"),
                idSafe
            );
        });

        it("should throw for non-existent approval", async () => {
            const mockAdapter: ChannelAdapter = {
                channelName: "telegram",
                sendText: vi.fn(),
                sendApprovalCard: vi.fn(),
                sendScreenshot: vi.fn(),
            };

            await expect(engine.forwardToChannel("fake", mockAdapter, "123"))
                .rejects.toThrow("Not found");
        });
    });

    describe("TTL Expiry", () => {
        it("should expire stale approvals after TTL", () => {
            const expiredHandler = vi.fn();
            engine.on("approval_expired", expiredHandler);

            const id = engine.createApproval("antigravity", "cmd", "ctx");
            expect(engine.pendingCount).toBe(1);

            // Advance time past TTL (60s)
            vi.advanceTimersByTime(61_000);
            // Trigger expiry check (runs every 30s)
            vi.advanceTimersByTime(30_000);

            expect(expiredHandler).toHaveBeenCalledTimes(1);
            expect(engine.getApproval(id)).toBeUndefined();
        });
    });

    describe("Audit Trail", () => {
        it("should record all actions in audit trail", () => {
            const id = engine.createApproval("vscode", "test", "ctx");
            engine.resolveApproval(id, true);

            const trail = engine.getAuditTrail();
            expect(trail.length).toBeGreaterThanOrEqual(2);
            expect(trail[0].action).toBe("created");
            expect(trail[1].action).toBe("approved");
        });

        it("should limit audit trail to 500 entries", () => {
            for (let i = 0; i < 510; i++) {
                engine.createApproval("vscode", `cmd_${i}`, "ctx");
            }

            const trail = engine.getAuditTrail(1000);
            expect(trail.length).toBeLessThanOrEqual(500);
        });
    });

    describe("Lifecycle", () => {
        it("should dispose cleanly", () => {
            engine.dispose();
            expect(() => engine.dispose()).not.toThrow();
        });
    });
});
