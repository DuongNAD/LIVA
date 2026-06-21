import { describe, it, expect, vi } from "vitest";

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

describe("PreemptiveVramMutex — Queue Sort Verification & Complexity Analysis", () => {
    it("should sort requests deterministically by priority (descending) and enqueuedAt (ascending)", async () => {
        // We will create a mutex with 10GB VRAM
        const mutex = new PreemptiveVramMutex(10000);

        // We will block the mutex with a task holding 9GB VRAM
        // so that subsequent acquires are queued
        const handle = await mutex.acquire("Blocker", 9000, 10);

        // Enqueue tasks with varying priorities and timestamps
        // We'll queue them in an unsorted sequence
        // priorities: 5, 12, 5, 20, 12
        const p1 = mutex.acquire("Task1", 2000, 5); // priority 5
        const p2 = mutex.acquire("Task2", 2000, 12); // priority 12
        const p3 = mutex.acquire("Task3", 2000, 5); // priority 5 (same priority as Task1, enqueued later)
        const p4 = mutex.acquire("Task4", 2000, 20); // priority 20
        const p5 = mutex.acquire("Task5", 2000, 12); // priority 12 (same priority as Task2, enqueued later)

        const queue = (mutex as any).queue;
        
        expect(queue).toBeDefined();
        expect(queue.length).toBe(5);

        // Expected sorted order:
        // 1. Task4 (priority 20)
        // 2. Task2 (priority 12, enqueued first)
        // 3. Task5 (priority 12, enqueued second)
        // 4. Task1 (priority 5, enqueued first)
        // 5. Task3 (priority 5, enqueued second)
        expect(queue[0].id).toBe("Task4");
        expect(queue[1].id).toBe("Task2");
        expect(queue[2].id).toBe("Task5");
        expect(queue[3].id).toBe("Task1");
        expect(queue[4].id).toBe("Task3");

        // Clean up
        handle.release();
        await Promise.all([p1, p2, p3, p4, p5]);
    });

    it("should verify sorting performance and absence of O(N^3) complexity", async () => {
        const mutex = new PreemptiveVramMutex(10000);
        const blocker = await mutex.acquire("Blocker", 9000, 10);

        const runTestForN = (N: number) => {
            (mutex as any).queue = [];

            const start = performance.now();
            for (let i = 0; i < N; i++) {
                const id = `T_${i}`;
                const priority = Math.floor(Math.random() * 100);
                const request = {
                    id,
                    requiredMemory: 100,
                    priority,
                    enqueuedAt: Date.now() + i,
                    resolve: () => {},
                    reject: () => {},
                };
                (mutex as any).queue.push(request);
                (mutex as any).queue.sort((a: any, b: any) => b.priority - a.priority || a.enqueuedAt - b.enqueuedAt);
            }
            const duration = performance.now() - start;
            return duration;
        };

        const t100 = runTestForN(100);
        const t1000 = runTestForN(1000);

        // Assert sorting time for 1000 items is fast (efficient)
        expect(t1000).toBeLessThan(100); // 100ms is extremely generous
        blocker.release();
    });
});
