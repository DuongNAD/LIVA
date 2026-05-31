import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock logger before importing the module
vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn(),
        debug: vi.fn(),
    },
}));

import { PreemptiveVramMutex, VRAM_PRIORITY, type VramLockHandle } from "@core/PreemptiveVramMutex";

describe("PreemptiveVramMutex — Priority-Based VRAM Lock", () => {
    let mutex: PreemptiveVramMutex;

    beforeEach(() => {
        mutex = new PreemptiveVramMutex();
    });

    // ============================================================
    // Priority Constants
    // ============================================================
    describe("VRAM_PRIORITY constants", () => {
        it("should have USER_INTERACTIVE as highest priority (0)", () => {
            expect(VRAM_PRIORITY.USER_INTERACTIVE).toBe(0);
        });

        it("should have TELEMETRY as lowest priority (10)", () => {
            expect(VRAM_PRIORITY.TELEMETRY).toBe(10);
        });

        it("should have correct priority ordering", () => {
            expect(VRAM_PRIORITY.USER_INTERACTIVE).toBeLessThan(VRAM_PRIORITY.SYSTEM_CRITICAL);
            expect(VRAM_PRIORITY.SYSTEM_CRITICAL).toBeLessThan(VRAM_PRIORITY.BACKGROUND_INTEL);
            expect(VRAM_PRIORITY.BACKGROUND_INTEL).toBeLessThan(VRAM_PRIORITY.PROACTIVE);
            expect(VRAM_PRIORITY.PROACTIVE).toBeLessThan(VRAM_PRIORITY.TELEMETRY);
        });
    });

    // ============================================================
    // acquire() — No contention
    // ============================================================
    describe("acquire() — No contention", () => {
        it("should grant lock immediately when no lock is held", () => {
            const handle = mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(handle).not.toBeNull();
            expect(handle?.id).toBe("AgentLoop");
            expect(handle?.priority).toBe(0);
        });

        it("should return a valid AbortSignal", () => {
            const handle = mutex.acquire("AgentLoop", 0);
            expect(handle?.signal).toBeInstanceOf(AbortSignal);
            expect(handle?.signal.aborted).toBe(false);
        });

        it("should set isLocked() to true after acquire", () => {
            mutex.acquire("test", 5);
            expect(mutex.isLocked()).toBe(true);
        });

        it("should set isLocked() to false initially", () => {
            expect(mutex.isLocked()).toBe(false);
        });
    });

    // ============================================================
    // acquire() — Preemption (higher priority steals)
    // ============================================================
    describe("acquire() — Preemption", () => {
        it("should preempt lower priority holder for higher priority request", () => {
            const bgLock = mutex.acquire("ConsolidationCron", VRAM_PRIORITY.BACKGROUND_INTEL);
            expect(bgLock).not.toBeNull();

            // Higher priority request
            const userLock = mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(userLock).not.toBeNull();
            expect(userLock?.id).toBe("AgentLoop");
        });

        it("should abort the preempted holder's signal", () => {
            const bgLock = mutex.acquire("ConsolidationCron", VRAM_PRIORITY.BACKGROUND_INTEL);
            expect(bgLock?.signal.aborted).toBe(false);

            // Preempt
            mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(bgLock?.signal.aborted).toBe(true);
        });

        it("should reject when same-priority lock is already held", () => {
            mutex.acquire("AgentLoop1", VRAM_PRIORITY.USER_INTERACTIVE);
            const second = mutex.acquire("AgentLoop2", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(second).toBeNull();
        });

        it("should reject when higher-priority lock is already held", () => {
            mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE); // p=0
            const bg = mutex.acquire("Background", VRAM_PRIORITY.BACKGROUND_INTEL); // p=5
            expect(bg).toBeNull();
        });
    });

    // ============================================================
    // release()
    // ============================================================
    describe("release()", () => {
        it("should release lock and allow new acquisition", () => {
            const handle = mutex.acquire("task1", 5);
            handle?.release();
            expect(mutex.isLocked()).toBe(false);

            const handle2 = mutex.acquire("task2", 5);
            expect(handle2).not.toBeNull();
        });

        it("should be safe to call release multiple times", () => {
            const handle = mutex.acquire("task1", 5);
            handle?.release();
            handle?.release(); // second call should be no-op
            expect(mutex.isLocked()).toBe(false);
        });

        it("should only release if caller is the current holder", () => {
            const handle1 = mutex.acquire("task1", 10);
            // Preempt with higher priority
            const handle2 = mutex.acquire("task2", 0);
            
            // task1's release should be no-op (it was already preempted)
            handle1?.release();
            expect(mutex.isLocked()).toBe(true);
            expect(mutex.getCurrentHolder()?.id).toBe("task2");
            
            handle2?.release();
        });
    });

    // ============================================================
    // getCurrentHolder()
    // ============================================================
    describe("getCurrentHolder()", () => {
        it("should return null when no lock is held", () => {
            expect(mutex.getCurrentHolder()).toBeNull();
        });

        it("should return holder info when lock is held", () => {
            mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE);
            const holder = mutex.getCurrentHolder();
            expect(holder).not.toBeNull();
            expect(holder?.id).toBe("AgentLoop");
            expect(holder?.priority).toBe(0);
            expect(holder?.heldMs).toBeGreaterThanOrEqual(0);
        });

        it("should reflect preemption (new holder)", () => {
            mutex.acquire("Background", VRAM_PRIORITY.BACKGROUND_INTEL);
            mutex.acquire("AgentLoop", VRAM_PRIORITY.USER_INTERACTIVE);
            const holder = mutex.getCurrentHolder();
            expect(holder?.id).toBe("AgentLoop");
        });
    });

    // ============================================================
    // AbortSignal integration
    // ============================================================
    describe("AbortSignal integration", () => {
        it("should fire abort event on preemption", () => {
            const bgLock = mutex.acquire("Background", VRAM_PRIORITY.PROACTIVE);
            let aborted = false;
            bgLock?.signal.addEventListener("abort", () => { aborted = true; });

            mutex.acquire("User", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(aborted).toBe(true);
        });

        it("should include reason in abort signal", () => {
            const bgLock = mutex.acquire("Background", VRAM_PRIORITY.PROACTIVE);
            mutex.acquire("User", VRAM_PRIORITY.USER_INTERACTIVE);
            expect(bgLock?.signal.reason).toContain("Preempted");
        });
    });
});
