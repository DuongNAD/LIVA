import { logger } from "../utils/logger";

type Task<T> = () => Promise<T>;

export enum TaskPriority {
    LOW = 0,      // Background tasks (consolidation, digest)
    NORMAL = 1,   // Default
    HIGH = 2,     // User-interactive (LTC summarization)
    CRITICAL = 3  // VRAM-sensitive (must wait for idle)
}

/**
 * TaskQueue
 * ---------
 * Đảm bảo các tiến trình LLM (đặc biệt là chạy ngầm bằng IsolatedAgentTurn)
 * được thực thi TUẦN TỰ (Sequential). Tránh hiện tượng tạo ra nhiều connection
 * đồng thời vào Router/Expert Model gây tràn VRAM (Out of Memory).
 *
 * [v26] Mở rộng với:
 * - Priority queuing: HIGH tasks được xử lý trước LOW tasks
 * - wrapMemoryTask(): Wrapper helper cho MemoryManager operations
 * - Dọn dẹp queue khi shutdown
 */
export class TaskQueue {
    private static instance: TaskQueue;
    private queue: Array<{ task: Task<any>; priority: TaskPriority; label: string }> = [];
    private isProcessing: boolean = false;
    #needsSort: boolean = false;
    #isShutdown: boolean = false;

    private constructor() {}

    public static getInstance(): TaskQueue {
        if (!TaskQueue.instance) {
            TaskQueue.instance = new TaskQueue();
        }
        return TaskQueue.instance;
    }

    /**
     * Wrapper helper cho MemoryManager operations
     * Đảm bảo tất cả memory writes (embedding, consolidation, LTC) chạy tuần tự
     * qua cùng một GPU embedding pipeline
     */
    public static wrapMemoryTask<T>(
        task: () => Promise<T>,
        label: string,
        priority: TaskPriority = TaskPriority.HIGH
    ): Promise<T> {
        return TaskQueue.getInstance().enqueueWithPriority(task, label, priority);
    }

    /**
     * Thêm một tác vụ vào hàng đợi với độ ưu tiên
     * HIGH priority tasks được xử lý trước LOW priority tasks
     */
    public enqueueWithPriority<T>(
        task: Task<T>,
        label: string,
        priority: TaskPriority = TaskPriority.NORMAL
    ): Promise<T> {
        return new Promise((resolve, reject) => {
            const wrapped: Task<T> = async () => {
                if (this.#isShutdown) {
                    logger.warn(`[TaskQueue] Bỏ qua tác vụ đã shutdown: ${label}`);
                    reject(new Error("TaskQueue shutdown"));
                    throw new Error("TaskQueue shutdown");
                }
                try {
                    const result = await task();
                    resolve(result);
                    return result;
                } catch (error) {
                    reject(error);
                    throw error;
                }
            };

            // Insert at correct position based on priority
            // Higher priority items go earlier in the queue
            // For items with same priority, newer items go to the end
            let insertIndex = this.queue.length;
            for (let i = 0; i < this.queue.length; i++) {
                if (this.queue[i].priority < priority) {
                    insertIndex = i;
                    break;
                }
            }
            this.queue.splice(insertIndex, 0, { task: wrapped, priority, label });

            // Mark for re-sort if this is HIGH or CRITICAL priority and we're mid-processing
            // This ensures high-priority tasks added mid-processing get processed first
            if (priority >= TaskPriority.HIGH && this.isProcessing) {
                this.#needsSort = true;
            }

            const priorityLabel = TaskPriority[priority];
            logger.info(`[TaskQueue] Đã nạp tác vụ "${label}" (${priorityLabel}). Queue: ${this.queue.length}`);
            this.processQueue();
        });
    }

    /**
     * Thêm một tác vụ vào hàng đợi và trả về Promise kết quả (priority = NORMAL)
     */
    public enqueue<T>(task: Task<T>, label: string = "Anonymous"): Promise<T> {
        return this.enqueueWithPriority(task, label, TaskPriority.NORMAL);
    }

    /**
     * Xử lý hàng đợi một cách tuần tự
     * Sắp xếp lại queue theo priority trước khi xử lý
     */
    private async processQueue() {
        if (this.#isShutdown) return;
        if (this.queue.length === 0) return;

        // Use a flag to ensure only one processing loop runs at a time
        if (this.isProcessing) {
            // If already processing, just return - the current loop will pick up remaining items
            return;
        }
        this.isProcessing = true;

        do {
            // Re-sort if needed (when new HIGH priority tasks were enqueued while processing)
            if (this.#needsSort) {
                this.queue.sort((a, b) => b.priority - a.priority);
                this.#needsSort = false;
            }

            const item = this.queue.shift();
            if (item) {
                const { task, label } = item;
                logger.info(`[TaskQueue] Bắt đầu xử lý: "${label}" (Còn lại: ${this.queue.length})`);
                try {
                    await task();
                } catch (error) {
                    logger.error({ err: error }, `[TaskQueue] Lỗi tác vụ "${label}"`);
                }
            }
        } while (this.queue.length > 0 && !this.#isShutdown);

        this.isProcessing = false;
        if (!this.#isShutdown) {
            logger.info(`[TaskQueue] Hàng đợi đã trống. Nghỉ ngơi.`);
        }
    }

    /**
     * Dọn dẹp tài nguyên khi shutdown
     * Gọi từ CoreKernel.shutdown()
     */
    public dispose(): void {
        this.#isShutdown = true;
        this.queue = [];
        logger.info(`[TaskQueue] Đã dispose.`);
    }

    /**
     * Kiểm tra số lượng tác vụ đang chờ
     */
    public get pendingCount(): number {
        return this.queue.length;
    }

    /**
     * Kiểm tra queue có đang xử lý không
     */
    public get processing(): boolean {
        return this.isProcessing;
    }
}
