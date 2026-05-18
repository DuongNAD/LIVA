import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { TaskQueue } from "../../src/core/TaskQueue";
import { logger } from "../../src/utils/logger";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        warn: vi.fn(),
        error: vi.fn()
    }
}));

describe("TaskQueue", () => {
    let taskQueue: TaskQueue;

    beforeEach(() => {
        vi.clearAllMocks();
        // Reset singleton instance for clean tests
        (TaskQueue as any).instance = undefined;
        taskQueue = TaskQueue.getInstance();
    });

    afterEach(() => {
        vi.restoreAllMocks();
    });

    it("should be a singleton", () => {
        const queue2 = TaskQueue.getInstance();
        expect(taskQueue).toBe(queue2);
    });

    it("should execute queued tasks sequentially", async () => {
        const executionOrder: number[] = [];
        
        const task1 = async () => {
            await new Promise(resolve => setTimeout(resolve, 50));
            executionOrder.push(1);
            return "Task 1 Done";
        };

        const task2 = async () => {
            await new Promise(resolve => setTimeout(resolve, 10)); // Shorter delay but queued later
            executionOrder.push(2);
            return "Task 2 Done";
        };

        const task3 = async () => {
            executionOrder.push(3);
            return "Task 3 Done";
        };

        // Fire them concurrently into the queue
        const p1 = taskQueue.enqueue(task1);
        const p2 = taskQueue.enqueue(task2);
        const p3 = taskQueue.enqueue(task3);

        const results = await Promise.all([p1, p2, p3]);

        // Despite task2 being faster, taskQueue forces sequential execution
        expect(executionOrder).toEqual([1, 2, 3]);
        expect(results).toEqual(["Task 1 Done", "Task 2 Done", "Task 3 Done"]);
        
        expect(logger.info).toHaveBeenCalledWith(expect.stringContaining("Hàng đợi đã trống"));
    });

    it("should not halt queue when a task fails", async () => {
        const executionOrder: number[] = [];

        const task1 = async () => {
            executionOrder.push(1);
            throw new Error("Task 1 Failed");
        };

        const task2 = async () => {
            executionOrder.push(2);
            return "Task 2 Done";
        };

        const p1 = taskQueue.enqueue(task1);
        const p2 = taskQueue.enqueue(task2);

        await expect(p1).rejects.toThrow("Task 1 Failed");
        const res2 = await p2;

        expect(executionOrder).toEqual([1, 2]);
        expect(res2).toBe("Task 2 Done");
    });

    it("should catch unexpected errors during processQueue", async () => {
        // Directly mutate the private queue to inject a throwing task
        (taskQueue as any).queue.push({
            task: async () => {
                throw new Error("Unexpected synchronous error");
            },
            priority: 0,
            label: "test-task"
        });
        
        // Trigger processing
        await (taskQueue as any).processQueue();
        
        expect(logger.error).toHaveBeenCalled();
    });

    it("should return early from processQueue when queue is empty (Line 47)", async () => {
        // Queue is empty — processQueue should exit immediately
        await (taskQueue as any).processQueue();
        // Should not set isProcessing or call any logging about task processing
        expect(logger.info).not.toHaveBeenCalledWith(expect.stringContaining("Bắt đầu xử lý"));
    });

    it("should handle shift() returning undefined in processQueue (Line 53 false)", async () => {
        // Simulate a race condition where queue.shift() returns undefined
        let shiftCallCount = 0;
        (taskQueue as any).queue = {
            length: 1,
            sort: () => { /* mock sort for new queue structure */ },
            shift: () => {
                shiftCallCount++;
                // First call: return undefined to trigger the `if (item)` false branch
                (taskQueue as any).queue.length = 0;
                return undefined;
            }
        };
        
        await (taskQueue as any).processQueue();
        expect(shiftCallCount).toBe(1);
    });

    it("should process HIGH priority tasks before LOW priority tasks when enqueued concurrently", async () => {
        // Use the fresh taskQueue from beforeEach
        const executionOrder: string[] = [];
        
        // Blocking task to hold the queue processing
        let releaseBlocker: () => void;
        const blocker = new Promise<void>(resolve => { releaseBlocker = resolve; });
        const p0 = taskQueue.enqueue(async () => {
            await blocker;
        }, "blocker");

        // Fire both enqueues WITHOUT awaiting (simulate concurrent enqueue)
        const p1 = taskQueue.enqueueWithPriority(
            async () => {
                executionOrder.push("LOW");
            },
            "low-task",
            0 // LOW
        );
        
        const p2 = taskQueue.enqueueWithPriority(
            async () => {
                executionOrder.push("HIGH");
            },
            "high-task",
            2 // HIGH
        );
        
        // Now release the blocker
        releaseBlocker!();

        // Wait for all to complete
        await Promise.all([p0, p1, p2]);
        
        // HIGH should execute before LOW due to priority sorting
        expect(executionOrder).toEqual(["HIGH", "LOW"]);
    });

    it("should handle dispose and clear queue", async () => {
        // Add a task to the queue
        (taskQueue as any).queue.push({
            task: async () => { return "should not run"; },
            priority: 0,
            label: "dispose-test"
        });
        
        // Dispose should clear the queue
        taskQueue.dispose();
        
        expect((taskQueue as any).queue.length).toBe(0);
        expect(taskQueue.pendingCount).toBe(0);
    });
});
