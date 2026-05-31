import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { LACPProtocol } from "@core/LACPProtocol";

describe("LACPProtocol — LLM Agent Communication Protocol", () => {
    let lacp: LACPProtocol;

    beforeEach(() => {
        // Reset singleton for test isolation
        // @ts-expect-error — accessing private static for test reset
        LACPProtocol.instance = undefined;
        lacp = LACPProtocol.getInstance();
    });

    // ============================================================
    // Singleton
    // ============================================================
    describe("Singleton", () => {
        it("should return the same instance on multiple calls", () => {
            const a = LACPProtocol.getInstance();
            const b = LACPProtocol.getInstance();
            expect(a).toBe(b);
        });
    });

    // ============================================================
    // signMessage()
    // ============================================================
    describe("signMessage()", () => {
        it("should create a valid envelope with all fields", () => {
            const env = lacp.signMessage("AgentA", "AgentB", "PREPARE", { action: "test" });
            expect(env.txId).toBeTruthy();
            expect(env.senderAgent).toBe("AgentA");
            expect(env.targetAgent).toBe("AgentB");
            expect(env.phase).toBe("PREPARE");
            expect(env.payload).toEqual({ action: "test" });
            expect(env.timestamp).toBeGreaterThan(0);
            expect(env.jwsSignature).toBeTruthy();
        });

        it("should produce unique txIds", () => {
            const env1 = lacp.signMessage("A", "B", "PREPARE", {});
            const env2 = lacp.signMessage("A", "B", "PREPARE", {});
            expect(env1.txId).not.toBe(env2.txId);
        });

        it("should produce different signatures for different payloads", () => {
            const env1 = lacp.signMessage("A", "B", "PREPARE", { data: 1 });
            const env2 = lacp.signMessage("A", "B", "PREPARE", { data: 2 });
            expect(env1.jwsSignature).not.toBe(env2.jwsSignature);
        });
    });

    // ============================================================
    // verifyMessage()
    // ============================================================
    describe("verifyMessage()", () => {
        it("should verify a correctly signed message", () => {
            const env = lacp.signMessage("A", "B", "COMMIT", { x: 42 });
            expect(lacp.verifyMessage(env)).toBe(true);
        });

        it("should reject message without signature", () => {
            const env = lacp.signMessage("A", "B", "COMMIT", {});
            delete (env as any).jwsSignature;
            expect(lacp.verifyMessage(env)).toBe(false);
        });

        it("should reject tampered payload", () => {
            const env = lacp.signMessage("A", "B", "COMMIT", { key: "original" });
            env.payload = { key: "tampered" };
            expect(lacp.verifyMessage(env)).toBe(false);
        });

        it("should reject tampered senderAgent", () => {
            const env = lacp.signMessage("A", "B", "COMMIT", {});
            env.senderAgent = "Hacker";
            expect(lacp.verifyMessage(env)).toBe(false);
        });

        it("should reject tampered phase", () => {
            const env = lacp.signMessage("A", "B", "PREPARE", {});
            env.phase = "COMMIT";
            expect(lacp.verifyMessage(env)).toBe(false);
        });
    });

    // ============================================================
    // executeTwoPhaseCommit()
    // ============================================================
    describe("executeTwoPhaseCommit()", () => {
        it("should execute successfully when callback succeeds", async () => {
            const env = lacp.signMessage("A", "B", "PREPARE", { task: "deploy" });
            const callback = vi.fn().mockResolvedValue(true);

            const result = await lacp.executeTwoPhaseCommit(env, callback);
            expect(result).toBe(true);
            expect(callback).toHaveBeenCalledOnce();
        });

        it("should return false when JWS verification fails", async () => {
            const env = lacp.signMessage("A", "B", "PREPARE", {});
            env.senderAgent = "Tampered"; // Invalidate signature
            const callback = vi.fn();

            const result = await lacp.executeTwoPhaseCommit(env, callback);
            expect(result).toBe(false);
            expect(callback).not.toHaveBeenCalled();
        });

        it("should rollback when callback returns false", async () => {
            const env = lacp.signMessage("A", "B", "PREPARE", {});
            const callback = vi.fn().mockResolvedValue(false);

            const result = await lacp.executeTwoPhaseCommit(env, callback);
            expect(result).toBe(false);
        });

        it("should rollback when callback throws", async () => {
            const env = lacp.signMessage("A", "B", "PREPARE", {});
            const callback = vi.fn().mockRejectedValue(new Error("Network failure"));

            const result = await lacp.executeTwoPhaseCommit(env, callback);
            expect(result).toBe(false);
        });
    });
});
