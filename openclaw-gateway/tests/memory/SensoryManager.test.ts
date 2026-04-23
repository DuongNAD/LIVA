/**
 * SensoryManager.test.ts — Multi-Modal Sensory Perception Tests
 * ==============================================================
 * Tests:
 * - Singleton pattern
 * - Token generation (branded types)
 * - TTL expiry for sensory context
 * - GC timer cleanup (dispose)
 * - Prompt injection with context
 * - Flush behavior
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

// Mock active-win and clipboardy (external native modules)
vi.mock("active-win", () => ({
    default: vi.fn().mockResolvedValue({
        owner: { name: "Visual Studio Code" },
        title: "AgentLoop.ts — openclaw-gateway",
    }),
}));

vi.mock("clipboardy", () => ({
    default: {
        read: vi.fn().mockResolvedValue("const x = 42;"),
    },
}));

import { SensoryManager } from "../../src/memory/SensoryManager";

describe("SensoryManager", () => {
    beforeEach(() => {
        // Reset singleton
        (SensoryManager as any).instance = null;
    });

    afterEach(() => {
        // Clean up timers
        const instance = (SensoryManager as any).instance;
        if (instance) instance.dispose();
        (SensoryManager as any).instance = null;
    });

    describe("Singleton", () => {
        it("should be a singleton", () => {
            const a = SensoryManager.getInstance();
            const b = SensoryManager.getInstance();
            expect(a).toBe(b);
        });
    });

    describe("captureContext", () => {
        it("should capture active window and clipboard data", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();

            const data = sm.currentData;
            expect(data).not.toBeNull();
            expect(data!.activeApp).toBe("Visual Studio Code");
            expect(data!.windowTitle).toContain("AgentLoop.ts");
            expect(data!.clipboardText).toBe("const x = 42;");
            expect(data!.token).toBeDefined();
            expect(typeof data!.token).toBe("string");
        });

        it("should generate unique tokens for each capture", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();
            const token1 = sm.currentData!.token;

            // Tiny delay to ensure different timestamp
            await new Promise(r => setTimeout(r, 5));
            await sm.captureContext();
            const token2 = sm.currentData!.token;

            expect(token1).not.toBe(token2);
        });

        it("should freeze captured data (immutable)", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();
            const data = sm.currentData;

            expect(Object.isFrozen(data)).toBe(true);
        });
    });

    describe("injectSensoryPrompt", () => {
        it("should return empty string when no context captured", () => {
            const sm = SensoryManager.getInstance();
            const prompt = sm.injectSensoryPrompt();
            expect(prompt).toBe("");
        });

        it("should return formatted prompt after capture", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();

            const prompt = sm.injectSensoryPrompt();
            expect(prompt).toContain("<SystemSensory");
            expect(prompt).toContain("Visual Studio Code");
            expect(prompt).toContain("AgentLoop.ts");
            expect(prompt).toContain("const x = 42;");
            expect(prompt).toContain("</SystemSensory>");
        });

        it("should return empty string when context has expired (TTL)", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();

            // Manually backdating the captured data
            const contextMap = (sm as any)._contextMap;
            for (const [token, data] of contextMap.entries()) {
                const expired = { ...data, capturedAt: Date.now() - 60000 };
                contextMap.set(token, Object.freeze(expired));
            }

            const prompt = sm.injectSensoryPrompt();
            expect(prompt).toBe("");
        });
    });

    describe("flush", () => {
        it("should clear all context data", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();
            expect(sm.currentData).not.toBeNull();

            sm.flush();
            expect(sm.currentData).toBeNull();
        });
    });

    describe("dispose", () => {
        it("should clear GC timer and context data", async () => {
            const sm = SensoryManager.getInstance();
            await sm.captureContext();

            sm.dispose();

            expect(sm.currentData).toBeNull();
            expect((sm as any).gcTimer).toBeNull();
        });
    });
});
