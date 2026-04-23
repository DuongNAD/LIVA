import { logger } from "../utils/logger";

type Task<T> = () => Promise<T>;

/**
 * TaskQueue
 * ---------
 * Đảm bảo các tiến trình LLM (đặc biệt là chạy ngầm bằng IsolatedAgentTurn)
 * được thực thi TUẦN TỰ (Sequential). Tránh hiện tượng tạo ra nhiều connection
 * đồng thời vào Router/Expert Model gây tràn VRAM (Out of Memory).
 */
export class TaskQueue {
    private static instance: TaskQueue;
    private queue: Task<any>[] = [];
    private isProcessing: boolean = false;

    private constructor() {}

    public static getInstance(): TaskQueue {
        if (!TaskQueue.instance) {
            TaskQueue.instance = new TaskQueue();
        }
        return TaskQueue.instance;
    }

    /**
     * Thêm một tác vụ vào hàng đợi và trả về Promise kết quả
     */
    public enqueue<T>(task: Task<T>): Promise<T> {
        return new Promise((resolve, reject) => {
            this.queue.push(async () => {
                try {
                    const result = await task();
                    resolve(result);
                } catch (error) {
                    reject(error);
                }
            });
            
            logger.info(`[TaskQueue] Đã nạp 1 tác vụ vào hàng đợi. Đang chờ xử lý: ${this.queue.length}`);
            this.processQueue();
        });
    }

    private async processQueue() {
        if (this.isProcessing) return;
        if (this.queue.length === 0) return;

        this.isProcessing = true;

        while (this.queue.length > 0) {
            const currentTask = this.queue.shift();
            if (currentTask) {
                logger.info(`[TaskQueue] Bắt đầu xử lý tác vụ... (Còn lại: ${this.queue.length})`);
                try {
                    await currentTask();
                } catch (error) {
                    logger.error(`[TaskQueue] Lỗi khi xử lý tác vụ: ${error}`);
                }
            }
        }

        this.isProcessing = false;
        logger.info(`[TaskQueue] Hàng đợi đã trống. Nghỉ ngơi.`);
    }
}
