import { EventEmitter } from "events";
import { logger } from "../utils/logger";
import { TaskLane, MessageTask, TaskState, AuthorityToken, AgentPhase } from "../types/AgentTypes";

export class TaskLaneWorker {
    #queue: MessageTask[] = [];
    #isProcessing = false;
    #lane: TaskLane;
    #maxConcurrency: number;
    #activeTasks: number = 0;

    constructor(lane: TaskLane, taskBus: EventEmitter) {
        this.#lane = lane;
        // Phân bổ Concurrency: LLM xử lý tuần tự (Tránh tràn VRAM), các Job nền/UI xử lý đa luồng đồng thời
        this.#maxConcurrency = (lane === TaskLane.LLM_REASONING) ? 1 : 4;

        taskBus.on(lane as string, (task: MessageTask, token: AuthorityToken<AgentPhase>) => {
            this.#queue.push(task);
            if (!this.#isProcessing) {
                this.processQueue(token).catch(e => logger.error(`[Worker ${lane}] Lỗi Controller:`, e));
            }
        });
    }

    private async processQueue(token: AuthorityToken<AgentPhase>) {
        this.#isProcessing = true;
        let executionCycles = 0;
        const MAX_CYCLES = 100;

        while (this.#queue.length > 0 && executionCycles < MAX_CYCLES) {
            const slotsAvailable = this.#maxConcurrency - this.#activeTasks;
            if (slotsAvailable <= 0) {
                // Hàng đợi Full - Chờ các tiến trình đang xử lý rảnh tay
                await new Promise(r => setTimeout(r, 50));
                continue;
            }

            const tasksToStart = this.#queue.splice(0, slotsAvailable);
            if (tasksToStart.length === 0) continue;

            executionCycles++;
            this.#activeTasks += tasksToStart.length;

            tasksToStart.forEach(task => {
                task.state = TaskState.EXECUTING;
                const executionPromise = task.execute(token);
                let timeoutId: NodeJS.Timeout;
                const timeoutPromise = new Promise((_, reject) =>
                    timeoutId = setTimeout(() => reject(new Error("Task execution timed out (Chain Breaker)")), 300000)
                );
                
                Promise.race([executionPromise, timeoutPromise])
                    .then(() => { task.state = TaskState.COMPLETED; })
                    .catch(error => {
                        task.state = TaskState.FAILED;
                        logger.error(`[TaskLaneWorker ${this.#lane}] Lỗi tại [$${task.id}] (State: ${task.state}):`, error);
                    })
                    .finally(() => {
                        clearTimeout(timeoutId);
                        this.#activeTasks--;
                    });
            });
        }

        if (executionCycles >= MAX_CYCLES && this.#queue.length > 0) {
            logger.error(`[CRITICAL] Phá vỡ vòng lặp vô tận (Chain Breaker) trên Lane: ${this.#lane}. Queue dropped.`);
            this.#queue = [];
        }

        // Chờ nốt các task lơ lửng kết thúc trước khi đánh dấu Rảnh Rỗi (Idle) hoàn toàn
        while (this.#activeTasks > 0) {
            await new Promise(r => setTimeout(r, 50));
        }

        this.#isProcessing = false;
    }
}
