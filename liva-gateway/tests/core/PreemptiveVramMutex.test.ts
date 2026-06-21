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

import { PreemptiveVramMutex } from "@core/PreemptiveVramMutex";

describe("PreemptiveVramMutex — Ordered Queue Lock with Handles", () => {
    let mutex: PreemptiveVramMutex;

    beforeEach(() => {
        mutex = new PreemptiveVramMutex(8000); // 8GB total VRAM
    });

    describe("acquire() & release() — No contention", () => {
        it("should acquire lock handle immediately when VRAM is available", async () => {
            const handle = await mutex.acquire("Task1", 2000, 5);
            expect(handle).toBeDefined();
            expect(handle.id).toBe("Task1");
            expect(handle.requiredMemory).toBe(2000);
            expect(handle.priority).toBe(5);
            expect(handle.signal.aborted).toBe(false);

            const status = mutex.getStatus();
            expect(status.allocatedVram).toBe(2000);
            expect(status.availableVram).toBe(6000);
        });

        it("should release VRAM and update status via handle.release()", async () => {
            const handle = await mutex.acquire("Task1", 2000, 5);
            handle.release();

            const status = mutex.getStatus();
            expect(status.allocatedVram).toBe(0);
            expect(status.availableVram).toBe(8000);
        });
    });

    describe("acquire() — Timeout behavior", () => {
        it("should timeout when request is blocked and timeoutMs is exceeded", async () => {
            // Occupy 7000 MB VRAM
            await mutex.acquire("Task1", 7000, 5);

            // Task2 needs 2000 MB (Total 9000 MB > 8000 MB), timeout in 100ms
            const acquirePromise = mutex.acquire("Task2", 2000, 5, 100);

            await expect(acquirePromise).rejects.toThrow("VRAM Acquisition Timeout");
        });
    });

    describe("Preemption behavior", () => {
        it("should preempt active low-priority tasks (< 10) when a high-priority task (>= 10) is blocked", async () => {
            // Task1 holds 6000 MB VRAM, priority 5 (low priority)
            const handle1 = await mutex.acquire("Task1", 6000, 5);
            expect(handle1.signal.aborted).toBe(false);

            let abortedByEvent = false;
            handle1.signal.addEventListener("abort", () => {
                abortedByEvent = true;
            });

            // Task2 has priority 12 (>= 10) and needs 4000 MB
            // It will trigger preemption on Task1 since 4000 MB is not available
            const handle2 = await mutex.acquire("Task2", 4000, 12);

            expect(abortedByEvent).toBe(true);
            expect(handle1.signal.aborted).toBe(true);
            
            // Task2 should be running and holding 4000 MB
            const status = mutex.getStatus();
            expect(status.allocatedVram).toBe(4000);
            expect(status.availableVram).toBe(4000);
            expect(handle2.signal.aborted).toBe(false);
        });

        it("should not preempt active tasks with priority >= 10", async () => {
            // Task1 holds 6000 MB VRAM, priority 10 (high priority)
            const handle1 = await mutex.acquire("Task1", 6000, 10);
            expect(handle1.signal.aborted).toBe(false);

            // Task2 has priority 15 (>= 10) and needs 4000 MB
            // It wants to run, but Task1 is priority 10 (cannot be preempted because it is >= 10)
            const acquirePromise = mutex.acquire("Task2", 4000, 15, 100);

            await expect(acquirePromise).rejects.toThrow("VRAM Acquisition Timeout");
            expect(handle1.signal.aborted).toBe(false);
        });
    });

    describe("Telemetry & Observability events", () => {
        it("should emit vram_wait_latency when a lock is granted", async () => {
            let waitLatencyEmitted = false;
            let eventPayload: any = null;

            mutex.eventEmitter.on("vram_wait_latency", (payload) => {
                waitLatencyEmitted = true;
                eventPayload = payload;
            });

            await mutex.acquire("Task1", 2000, 5);

            expect(waitLatencyEmitted).toBe(true);
            expect(eventPayload.id).toBe("Task1");
            expect(eventPayload.latencyMs).toBeGreaterThanOrEqual(0);
        });

        it("should emit vram_lock_hold_duration when a lock is released or preempted", async () => {
            let holdDurationEmitted = false;
            let eventPayload: any = null;

            mutex.eventEmitter.on("vram_lock_hold_duration", (payload) => {
                holdDurationEmitted = true;
                eventPayload = payload;
            });

            // Grant lock
            const handle = await mutex.acquire("Task1", 2000, 5);
            
            // Release lock
            handle.release();

            expect(holdDurationEmitted).toBe(true);
            expect(eventPayload.id).toBe("Task1");
            expect(eventPayload.durationMs).toBeGreaterThanOrEqual(0);
            expect(eventPayload.preempted).toBe(false);
        });

        it("should emit vram_lock_hold_duration with preempted=true on preemption", async () => {
            let holdDurationEmitted = false;
            let eventPayload: any = null;

            mutex.eventEmitter.on("vram_lock_hold_duration", (payload) => {
                holdDurationEmitted = true;
                eventPayload = payload;
            });

            // Grant low-priority lock
            await mutex.acquire("Task1", 6000, 5);

            // Preempt with high priority
            await mutex.acquire("Task2", 4000, 12);

            expect(holdDurationEmitted).toBe(true);
            expect(eventPayload.id).toBe("Task1");
            expect(eventPayload.durationMs).toBeGreaterThanOrEqual(0);
            expect(eventPayload.preempted).toBe(true);
        });
    });

    describe("Circuit Breaker behavior", () => {
        it("should trip circuit breaker after 3 failures in executeSafely", async () => {
            vi.useFakeTimers();
            let emergencyResetEmitted = false;
            mutex.eventEmitter.on("emergency_reset_required", () => {
                emergencyResetEmitted = true;
            });

            const failingTask = async () => {
                throw new Error("CUDA OOM mock");
            };

            // 1st failure
            await expect(mutex.executeSafely("Task1", 2000, failingTask)).rejects.toThrow("CUDA OOM mock");
            expect(emergencyResetEmitted).toBe(false);

            // 2nd failure
            await expect(mutex.executeSafely("Task2", 2000, failingTask)).rejects.toThrow("CUDA OOM mock");
            expect(emergencyResetEmitted).toBe(false);

            // 3rd failure - should trip circuit breaker
            await expect(mutex.executeSafely("Task3", 2000, failingTask)).rejects.toThrow("CUDA OOM mock");
            expect(emergencyResetEmitted).toBe(true);

            // Subsequent acquire should throw Circuit Open error
            await expect(mutex.acquire("Task4", 1000)).rejects.toThrow("CIRCUIT OPEN");

            vi.useRealTimers();
        });

        it("should recover and close circuit after cool-off and successful task execution", async () => {
            vi.useFakeTimers();

            const failingTask = async () => {
                throw new Error("CUDA OOM mock");
            };

            const successfulTask = async () => {
                return "success";
            };

            // Fail 3 times to trip the circuit
            await expect(mutex.executeSafely("Task1", 2000, failingTask)).rejects.toThrow();
            await expect(mutex.executeSafely("Task2", 2000, failingTask)).rejects.toThrow();
            await expect(mutex.executeSafely("Task3", 2000, failingTask)).rejects.toThrow();

            // Circuit is now Open, subsequent acquire fails
            await expect(mutex.acquire("Task4", 1000)).rejects.toThrow("CIRCUIT OPEN");

            // Advance timers by cool-off duration (10s)
            vi.advanceTimersByTime(11000);

            // Circuit should now be Half-Open / Closed (resumed)
            // A successful executeSafely should succeed and failureCount should be 0
            const result = await mutex.executeSafely("Task5", 2000, successfulTask);
            expect(result).toBe("success");

            // Verify that we can acquire VRAM normally again
            const handle = await mutex.acquire("Task6", 1000);
            expect(handle).toBeDefined();
            handle.release();

            vi.useRealTimers();
        });
    });

    // ════════════════════════════════════════════
    //  Graduated VRAM Degradation (v30)
    // ════════════════════════════════════════════
    describe("acquireWithGraduation() — Skip graduation", () => {
        it("should skip graduation when VRAM is sufficient", async () => {
            vi.useRealTimers();
            const freshMutex = new PreemptiveVramMutex(8000);
            const emitSpy = vi.spyOn(freshMutex.eventEmitter, "emit");
            const handle = await freshMutex.acquireWithGraduation("easy-task", 4000, 15, 5000);
            expect(handle).toBeDefined();
            expect(emitSpy).not.toHaveBeenCalledWith("avatar_demote", expect.anything());
            handle.release();
        });

        it("should delegate to acquire() for low-priority tasks (priority < 10)", async () => {
            vi.useRealTimers();
            const freshMutex = new PreemptiveVramMutex(8000);
            // Fill VRAM first
            const filler = await freshMutex.acquire("filler", 7000, 1, 5000);
            const emitSpy = vi.spyOn(freshMutex.eventEmitter, "emit");

            // Low priority — acquireWithGraduation delegates to acquire(), which times out
            await expect(
                freshMutex.acquireWithGraduation("low-prio", 4000, 5, 300)
            ).rejects.toThrow("VRAM Acquisition Timeout");

            // No avatar_demote should be emitted for low priority
            expect(emitSpy).not.toHaveBeenCalledWith("avatar_demote", expect.anything());
            filler.release();
        });
    });

    describe("acquireWithGraduation() — Graduated degradation events", () => {
        it("should emit eco mode event (Step 1) when VRAM is insufficient", async () => {
            vi.useRealTimers();
            const freshMutex = new PreemptiveVramMutex(8000);
            const filler = await freshMutex.acquire("filler", 7000, 1, 5000);
            const emitSpy = vi.spyOn(freshMutex.eventEmitter, "emit");

            // Release filler after 250ms so eco mode captures it
            setTimeout(() => filler.release(), 600);

            const handle = await freshMutex.acquireWithGraduation("expert-model", 4000, 15, 10000);
            expect(handle).toBeDefined();

            // Should have emitted eco mode before acquiring
            expect(emitSpy).toHaveBeenCalledWith("avatar_demote", { level: "eco", fps: 5 });

            handle.release();
        }, 15000);
    });

    describe("acquireWithGraduation() — Auto-restore on release", () => {
        it("should emit avatar_restore when graduated lock is released", async () => {
            vi.useRealTimers();
            const freshMutex = new PreemptiveVramMutex(8000);
            const filler = await freshMutex.acquire("filler", 6000, 1, 5000);
            const emitSpy = vi.spyOn(freshMutex.eventEmitter, "emit");

            // Release filler after eco mode delay so VRAM becomes available
            setTimeout(() => filler.release(), 600);

            const handle = await freshMutex.acquireWithGraduation("expert", 4000, 15, 10000);
            expect(handle).toBeDefined();

            // Release the graduated handle — should restore avatar
            handle.release();
            expect(emitSpy).toHaveBeenCalledWith("avatar_restore", { level: "normal" });
            expect(freshMutex.getAvatarDemoteLevel()).toBe("normal");
        }, 15000);
    });

    describe("getAvatarDemoteLevel()", () => {
        it("should return 'normal' initially", () => {
            expect(mutex.getAvatarDemoteLevel()).toBe("normal");
        });
    });
});
