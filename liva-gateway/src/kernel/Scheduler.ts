import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { SyscallRequest, SyscallPriority, SyscallType } from "./SyscallInterface";
import { LlmCircuitBreaker } from "../core/LlmCircuitBreaker";
import { ConfigManager } from "../core/config/ConfigManager";

export class Scheduler {
    private static instance: Scheduler;
    
    // 3 Hàng đợi theo mức độ ưu tiên
    private queues: {
        [SyscallPriority.HRT]: SyscallRequest[];
        [SyscallPriority.SRT]: SyscallRequest[];
        [SyscallPriority.DT]: SyscallRequest[];
    } = {
        [SyscallPriority.HRT]: [],
        [SyscallPriority.SRT]: [],
        [SyscallPriority.DT]: []
    };

    private isProcessing: boolean = false;
    private isSuspended: boolean = false; // Bị đình chỉ khi VRAM bị chiếm dụng (VRAMGuard)

    private constructor() {}

    public static getInstance(): Scheduler {
        if (!Scheduler.instance) {
            Scheduler.instance = new Scheduler();
        }
        return Scheduler.instance;
    }

    /**
     * Nạp System Call vào hàng đợi
     */
    public emitSyscall<T>(request: Omit<SyscallRequest<T>, "id" | "resolve" | "reject">): Promise<T> {
        return new Promise((resolve, reject) => {
            const fullRequest: SyscallRequest<T> = {
                ...request,
                id: `syscall-${Date.now()}-${Math.floor(Math.random() * 10000)}`,
                resolve,
                reject
            };
            
            this.queues[request.priority].push(fullRequest);
            logger.debug(`[AIOS Kernel/Scheduler] Nạp Syscall: ${request.type} (Priority: ${SyscallPriority[request.priority]})`);
            this.processQueues();
        });
    }

    private async processQueues() {
        if (this.isProcessing || this.isSuspended) return;
        this.isProcessing = true;

        while (this.hasPendingTasks() && !this.isSuspended) {
            // Ưu tiên 1: Hard Real-Time (HRT)
            if (this.queues[SyscallPriority.HRT].length > 0) {
                const req = this.queues[SyscallPriority.HRT].shift();
                if (req) await this.executeSyscall(req);
                continue;
            }

            // Ưu tiên 2: Soft Real-Time (SRT)
            if (this.queues[SyscallPriority.SRT].length > 0) {
                const req = this.queues[SyscallPriority.SRT].shift();
                if (req) await this.executeSyscall(req);
                continue;
            }

            // Ưu tiên 3: Delay-Tolerant (DT)
            if (this.queues[SyscallPriority.DT].length > 0) {
                const req = this.queues[SyscallPriority.DT].shift();
                if (req) await this.executeSyscall(req);
                continue;
            }
        }

        this.isProcessing = false;
    }

    private hasPendingTasks(): boolean {
        return this.queues[SyscallPriority.HRT].length > 0 ||
               this.queues[SyscallPriority.SRT].length > 0 ||
               this.queues[SyscallPriority.DT].length > 0;
    }

    /**
     * Tạm dừng toàn bộ hàng đợi SRT và DT (HRT vẫn có thể bypass nếu cần)
     */
    public suspend() {
        this.isSuspended = true;
        logger.warn("[AIOS Kernel/Scheduler] 🛑 Đã đình chỉ hàng đợi (Suspend).");
    }
    
    /**
     * Khôi phục hàng đợi
     */
    public resume() {
        this.isSuspended = false;
        logger.info("[AIOS Kernel/Scheduler] 🟢 Đã khôi phục hàng đợi (Resume).");
        this.processQueues();
    }

    /**
     * Bơm SyscallHandler vào Kernel (Dependency Injection)
     * Thay vì xử lý trực tiếp, Scheduler sẽ uỷ quyền thực thi cho các Service lõi.
     */
    private async executeSyscall(req: SyscallRequest) {
        try {
            logger.info(`[AIOS Kernel] Đang thực thi Syscall: ${req.type} [${req.id}]`);
            let result: any = null;
            
            // NOTE: Bước tiếp theo sẽ cần tạo KernelHandler để map các Syscall này vào Engine thực tế
            switch (req.type) {
                case "syscall_infer":
                    const { client, usingTarget, localMsgs, tempParam, maxTokensParam, topPParam } = req.payload;
                    const cb = LlmCircuitBreaker.getInstance();
                    if (!cb.canExecute(usingTarget)) {
                        throw new Error(`[CircuitBreaker] LLM Service '${usingTarget}' is currently OPEN due to consecutive failures. Request blocked.`);
                    }
                    try {
                        result = await client.chat.completions.create({
                            model: usingTarget,
                            messages: localMsgs,
                            temperature: tempParam,
                            max_tokens: maxTokensParam,
                            top_p: topPParam,
                            stop: ["<end_of_turn>", "<|im_end|>", "\n---", "\nUser:", "\nAssistant:", "\nLIVA:"],
                            stream: true,
                        });
                        cb.recordSuccess(usingTarget);
                    } catch (err: any) {
                        cb.recordFailure(usingTarget, err.message || String(err));
                        throw err;
                    }
                    break;
                case "syscall_vector_search":
                    // Tương lai: Gọi MemoryManager
                    result = [];
                    break;
                case "syscall_execute_tool":
                    const { toolOrchestrator, functionName, functionArgs } = req.payload;
                    result = await toolOrchestrator.executeWithReflection(functionName, functionArgs);
                    break;
                case "syscall_snapshot_save":
                    try {
                        const { slotId, filePath } = req.payload;
                        const targetPort = ConfigManager.getInstance().isNativeMode ? 8100 : 8000;
                        const res = await safeFetch(`http://127.0.0.1:${targetPort}/slots/${slotId}?action=save`, {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ filepath: filePath })
                        }, 5000);
                        result = { success: res.status, filepath: filePath };
                        logger.info(`[Context Swapping] 💾 Đã lưu KV Cache Snapshot vào ${filePath}`);
                    } catch (e) {
                        logger.warn(`[Context Swapping] Không thể lưu cache: ${e}`);
                        result = { success: false };
                    }
                    break;
                case "syscall_snapshot_restore":
                    try {
                        const { slotId, filePath } = req.payload;
                        const targetPort = ConfigManager.getInstance().isNativeMode ? 8100 : 8000;
                        const res = await safeFetch(`http://127.0.0.1:${targetPort}/slots/${slotId}?action=restore`, {
                            method: "POST",
                            headers: { "Content-Type": "application/json" },
                            body: JSON.stringify({ filepath: filePath })
                        }, 5000);
                        result = { success: res.status, filepath: filePath };
                        logger.info(`[Context Swapping] 🔄 Đã phục hồi KV Cache Snapshot từ ${filePath}`);
                    } catch (e) {
                        logger.warn(`[Context Swapping] Không thể nạp lại cache: ${e}`);
                        result = { success: false };
                    }
                    break;
                case "syscall_a2a_message":
                    const { sender, receiver, message } = req.payload;
                    logger.info(`[A2A Protocol] ✉️ Giao tiếp chéo: [${sender}] -> [${receiver}]: ${String(message).substring(0, 50)}...`);
                    // TODO: Đẩy vào TaskBus hoặc Event Emitter của Agent đích
                    result = { delivered: true, timestamp: Date.now() };
                    break;
                default:
                    // Fallback pass-through
                    result = true;
                    break;
            }
            
            if (req.resolve) req.resolve(result);
        } catch (error) {
            logger.error(`[AIOS Kernel] Lỗi Syscall ${req.type}: ${error}`);
            if (req.reject) req.reject(error);
        }
    }
}
