/**
 * TelemetryProfiler.test.ts — Performance Metrics Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("fs", () => ({
    existsSync: vi.fn().mockReturnValue(false),
    mkdirSync: vi.fn(),
    promises: {
        readFile: vi.fn().mockResolvedValue(""),
        writeFile: vi.fn().mockResolvedValue(undefined),
        rename: vi.fn().mockResolvedValue(undefined),
    },
}));

import { TelemetryProfiler } from "../../src/core/TelemetryProfiler";

describe("TelemetryProfiler", () => {
    beforeEach(() => {
        // Reset static state
        (TelemetryProfiler as any).isInitialized = false;
        (TelemetryProfiler as any).pendingLogs = [];
        if ((TelemetryProfiler as any).flushTimer) {
            clearTimeout((TelemetryProfiler as any).flushTimer);
            (TelemetryProfiler as any).flushTimer = null;
        }
    });

    afterEach(() => {
        if ((TelemetryProfiler as any).flushTimer) {
            clearTimeout((TelemetryProfiler as any).flushTimer);
            (TelemetryProfiler as any).flushTimer = null;
        }
    });

    it("should initialize only once (idempotent)", () => {
        TelemetryProfiler.initialize();
        TelemetryProfiler.initialize();
        expect((TelemetryProfiler as any).isInitialized).toBe(true);
    });

    it("should track a fast async function and return its result", async () => {
        const result = await TelemetryProfiler.track("fast_task", async () => {
            return 42;
        });
        expect(result).toBe(42);
    });

    it("should propagate errors from tracked function", async () => {
        await expect(
            TelemetryProfiler.track("failing_task", async () => {
                throw new Error("Task crashed");
            })
        ).rejects.toThrow("Task crashed");
    });

    it("should auto-initialize on first track call", async () => {
        expect((TelemetryProfiler as any).isInitialized).toBe(false);
        await TelemetryProfiler.track("auto_init", async () => "ok");
        expect((TelemetryProfiler as any).isInitialized).toBe(true);
    });

    it("should track async function timing accurately", async () => {
        const start = Date.now();
        await TelemetryProfiler.track("timed_task", async () => {
            await new Promise(r => setTimeout(r, 50));
            return "done";
        });
        const elapsed = Date.now() - start;
        expect(elapsed).toBeGreaterThanOrEqual(40);
    });
});
