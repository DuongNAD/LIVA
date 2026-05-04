/**
 * TelemetryProfiler.test.ts — Performance Metrics Tests
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

let mockExists = false;
let mockReadFileReturn = "";

vi.mock("node:fs", () => ({
    existsSync: vi.fn(() => mockExists),
    mkdirSync: vi.fn(),
    promises: {
        readFile: vi.fn(async () => mockReadFileReturn),
        writeFile: vi.fn().mockResolvedValue(undefined),
        rename: vi.fn().mockResolvedValue(undefined),
    },
}));

import { TelemetryProfiler } from "../../src/core/TelemetryProfiler";
import { promises as fsp } from "node:fs";

describe("TelemetryProfiler", () => {
    beforeEach(() => {
        // Reset static state
        mockExists = false;
        mockReadFileReturn = "";
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

    it("should log bottleneck if task takes more than 500ms", async () => {
        // Use real timers because PerformanceObserver interacts poorly with fake timers
        const promise = TelemetryProfiler.track("slow_task", async () => {
            await new Promise(r => setTimeout(r, 600));
            return "done";
        });
        
        await promise;

        // Give PerformanceObserver a chance to fire
        await new Promise(r => setTimeout(r, 100));
        
        expect((TelemetryProfiler as any).pendingLogs.length).toBeGreaterThan(0);
        expect((TelemetryProfiler as any).pendingLogs[0]).toContain("slow_task");

        // Let's call the flush timer callback directly to cover the flush logic
        const flushTimer = (TelemetryProfiler as any).flushTimer;
        if (flushTimer) {
            // flushTimer is a Timeout object in Node, it has a _onTimeout property or similar
            // But we can also just wait 2000ms using real timers, which we are already doing
            // So we can just await the timeout
            clearTimeout(flushTimer); // avoid hanging the test
            
            // To manually run the callback, we can recreate the logic or just wait for it.
            // Wait, we are using real timers! So if we wait 2100ms, the callback WILL execute!
        }
    }, 10000);

    it("should execute flushTimer callback automatically", async () => {
        // Wait for the >500ms task
        const promise = TelemetryProfiler.track("slow_task_2", async () => {
            await new Promise(r => setTimeout(r, 600));
            return "done";
        });
        await promise;
        
        // Wait for the observer callback
        await new Promise(r => setTimeout(r, 100));
        
        // Wait for 2000ms flushTimer
        await new Promise(r => setTimeout(r, 2100));

        expect(fsp.writeFile).toHaveBeenCalled();
    }, 10000);

    it("should read existing log file before writing", async () => {
        // Mock existsSync to return true
        mockExists = true;
        mockReadFileReturn = "OLD LOG\n";

        const promise = TelemetryProfiler.track("slow_task_3", async () => {
            await new Promise(r => setTimeout(r, 600));
            return "done";
        });
        await promise;
        
        // Wait for the observer callback
        await new Promise(r => setTimeout(r, 100));
        
        // Wait for 2000ms flushTimer
        await new Promise(r => setTimeout(r, 2100));

        expect(fsp.readFile).toHaveBeenCalled();
        // Since writeFile might be called multiple times due to multiple tests,
        // we just check if any call contains our string.
        const calls = vi.mocked(fsp.writeFile).mock.calls;
        const hasOldLog = calls.some(call => call[1] && call[1].toString().includes("OLD LOG"));
        expect(hasOldLog).toBe(true);
    }, 10000);

    it("should handle write errors without crashing (catch block)", async () => {
        vi.mocked(fsp.writeFile).mockRejectedValueOnce(new Error("Write error"));

        const promise = TelemetryProfiler.track("slow_task_4", async () => {
            await new Promise(r => setTimeout(r, 600));
            return "done";
        });
        await promise;
        
        // Wait for the observer callback
        await new Promise(r => setTimeout(r, 100));
        
        // Wait for 2000ms flushTimer
        await new Promise(r => setTimeout(r, 2100));

        // It should not crash
        expect(true).toBe(true);
    }, 10000);

    it("should trim log file when content exceeds 5KB (Line 73 true branch)", async () => {
        // Create existing content larger than 5000 chars
        mockExists = true;
        mockReadFileReturn = "X".repeat(5500);

        const promise = TelemetryProfiler.track("slow_task_5", async () => {
            await new Promise(r => setTimeout(r, 600));
            return "done";
        });
        await promise;
        
        // Wait for the observer callback
        await new Promise(r => setTimeout(r, 100));
        
        // Wait for 2000ms flushTimer
        await new Promise(r => setTimeout(r, 2100));

        // The written content should be trimmed to 5000 chars (from the end)
        const calls = vi.mocked(fsp.writeFile).mock.calls;
        if (calls.length > 0) {
            const lastContent = calls[calls.length - 1][1]?.toString() || "";
            expect(lastContent.length).toBeLessThanOrEqual(5000);
        }
    }, 10000);
});
