import { EventEmitter } from 'node:events';
import { logger } from "../../utils/logger";
import { withSafeTimeout } from "../../utils/HttpClient";
import { TaskLane, MessageTask, TaskState, AuthorityToken, AgentPhase } from "../../types/AgentTypes";

export class TaskLaneWorker {
    #queue: MessageTask[] = [];
    #isProcessing = false;
    #lane: TaskLane;
    #maxConcurrency: number;
    #activeTasks: number = 0;
    private logger: any;

    constructor(lane: TaskLane, taskBus: EventEmitter) {
        this.#lane = lane;
        // Phân bổ Concurrency: LLM xử lý tuần tự (Tránh tràn VRAM), các Job nền/UI xử lý đa luồng đồng thời
        this.#maxConcurrency = (lane === TaskLane.LLM_REASONING) ? 1 : 4;
        this.logger = logger.child({ component: `TaskLaneWorker-${lane}` });

        taskBus.on(lane as string, (task: MessageTask, token: AuthorityToken<AgentPhase>) => {
            this.#queue.push(task);
            if (!this.#isProcessing) {
                this.processQueue(token).catch((e: unknown) => this.logger.error(`[Worker ${lane}] Lỗi Controller:`, e));
            }
        });
    }

    private async processQueue(token: AuthorityToken<AgentPhase>): Promise<void> {
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

            executionCycles++;
            this.#activeTasks += tasksToStart.length;

            tasksToStart.forEach(task => {
                task.state = TaskState.EXECUTING;

                // [v26 Audit Fix] Use withSafeTimeout instead of raw Promise.race + setTimeout
                // Prevents timer leak: withSafeTimeout clears timer in .finally() guaranteed
                withSafeTimeout(task.execute(token), 300000, `TaskLane-${this.#lane} Chain Breaker`)
                    .then(() => { task.state = TaskState.COMPLETED; })
                    .catch(error => {
                        task.state = TaskState.FAILED;
                        this.logger.error(`[TaskLaneWorker ${this.#lane}] Lỗi tại [$${task.id}] (State: ${task.state}):`, error);
                    })
                    .finally(() => {
                        this.#activeTasks--;
                    });
            });
        }

        if (executionCycles >= MAX_CYCLES && this.#queue.length > 0) {
            this.logger.error(`[CRITICAL] Phá vỡ vòng lặp vô tận (Chain Breaker) trên Lane: ${this.#lane}. Queue dropped.`);
            this.#queue = [];
        }

        // Chờ nốt các task lơ lửng kết thúc
        while (this.#activeTasks > 0) {
            await new Promise(r => setTimeout(r, 50));
        }

        // Nếu có task mới chèn vào trong lúc chờ, tiếp tục xử lý
        if (this.#queue.length > 0) {
            return this.processQueue(token);
        }

        this.#isProcessing = false;
    }
}
