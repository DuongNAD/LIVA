import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EventEmitter } from 'node:events';
import { TaskLaneWorker } from '../../../src/core/orchestrators/TaskLaneWorker';
import { TaskLane, TaskState, AgentPhase } from '../../../src/types/AgentTypes';

describe('TaskLaneWorker - Queue & Chain Breaker', () => {
    let taskBus: EventEmitter;

    beforeEach(() => {
        taskBus = new EventEmitter();
        vi.useFakeTimers();
    });

    afterEach(() => {
        vi.restoreAllMocks();
        vi.clearAllTimers();
        vi.useRealTimers();
    });

    it('should wait when queue is full and active tasks reach maxConcurrency (Lines 36-37)', async () => {
        const worker = new TaskLaneWorker(TaskLane.LLM_REASONING, taskBus);
        // LLM_REASONING has maxConcurrency = 1

        const task1 = {
            id: 'task1',
            lane: TaskLane.LLM_REASONING,
            data: {},
            execute: vi.fn().mockImplementation(() => new Promise<void>(r => setTimeout(r, 200)))
        };

        const task2 = {
            id: 'task2',
            lane: TaskLane.LLM_REASONING,
            data: {},
            execute: vi.fn().mockImplementation(() => new Promise<void>(r => setTimeout(r, 50)))
        };

        // Fire both tasks
        taskBus.emit(TaskLane.LLM_REASONING, task1, { phase: AgentPhase.RUNNING, isValid: () => true });
        taskBus.emit(TaskLane.LLM_REASONING, task2, { phase: AgentPhase.RUNNING, isValid: () => true });

        // Advance 100ms. task1 is still running. task2 is blocked in queue!
        await vi.advanceTimersByTimeAsync(100);
        
        // At this point, task1.execute is called, but task2.execute is NOT.
        expect(task1.execute).toHaveBeenCalled();
        expect(task2.execute).not.toHaveBeenCalled();

        // Advance 200ms more. task1 finishes, task2 starts and finishes.
        await vi.advanceTimersByTimeAsync(200);

        expect(task2.execute).toHaveBeenCalled();
        expect(task2.state).toBe(TaskState.COMPLETED);
    });

    it('should catch task execute error and set state to FAILED (Lines 57-58)', async () => {
        const worker = new TaskLaneWorker(TaskLane.BACKGROUND_JOB, taskBus);
        const loggerErrorSpy = vi.spyOn((worker as any).logger, 'error').mockImplementation(() => {});

        const taskFail = {
            id: 'task-fail',
            lane: TaskLane.BACKGROUND_JOB,
            data: {},
            execute: vi.fn().mockRejectedValue(new Error('Mock Execution Error'))
        };

        taskBus.emit(TaskLane.BACKGROUND_JOB, taskFail, { phase: AgentPhase.RUNNING, isValid: () => true });

        // Advance timers to allow the promise to reject
        await vi.advanceTimersByTimeAsync(100);

        expect(taskFail.state).toBe(TaskState.FAILED);
        expect(loggerErrorSpy).toHaveBeenCalledWith(
            expect.stringContaining('[TaskLaneWorker background_job] Lỗi tại [$task-fail] (State: FAILED):'),
            expect.any(Error)
        );
    });

    it('should drop queue when execution cycles reach MAX_CYCLES (Lines 68-69)', async () => {
        const worker = new TaskLaneWorker(TaskLane.BACKGROUND_JOB, taskBus);
        const loggerErrorSpy = vi.spyOn((worker as any).logger, 'error').mockImplementation(() => {});

        // Emit 405 tasks because maxConcurrency is 4, each cycle pops 4 tasks.
        // The first task goes into the first processQueue call. The next 404 tasks are processed in the second processQueue call.
        // 100 cycles * 4 = 400 tasks. The remaining 4 tasks will trigger the chain breaker.
        for (let i = 0; i < 405; i++) {
            taskBus.emit(TaskLane.BACKGROUND_JOB, {
                id: `task-${i}`,
                lane: TaskLane.BACKGROUND_JOB,
                data: {},
                execute: vi.fn().mockResolvedValue(undefined)
            }, { phase: AgentPhase.RUNNING, isValid: () => true });
        }

        // Advance timers to process all tasks. 100 cycles with 50ms pause if slots filled,
        // but here they resolve immediately so it might process them very quickly.
        // Let's just advance time by a large amount.
        await vi.advanceTimersByTimeAsync(100000);

        expect(loggerErrorSpy).toHaveBeenCalledWith(
            expect.stringContaining('[CRITICAL] Phá vỡ vòng lặp vô tận (Chain Breaker)')
        );
    });
});
