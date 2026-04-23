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
        // Reset singleton instance via reflection for clean tests
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
});
