import OpenAI from "openai";
import { EventEmitter } from 'node:events';
import { NativeIPCClient } from "../utils/NativeIPCClient";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { ModelOrchestrator } from "./ModelOrchestrator";
import { SemanticRouter } from "../memory/SemanticRouter";
import { SemanticCache } from "../memory/SemanticCache";
import type { ChannelRouter } from "../channels/ChannelNormalizer";
import { AgentPhase, TaskLane, AuthorityToken, MessageTask } from "../types/AgentTypes";
import { ConfigManager } from "./config/ConfigManager";
import { CoreKernelAuthority } from "./CoreKernelAuthority";
import { TaskLaneWorker } from "./orchestrators/TaskLaneWorker";
import { TaskQueue } from "./TaskQueue";
import { Scheduler } from "../kernel/Scheduler";
import { SyscallPriority } from "../kernel/SyscallInterface";
import { SensoryManager } from "../memory/SensoryManager";

// Import modular sub-modules
import { PromptCompiler } from "./loop/PromptCompiler";
import { LoopStateDelegate, LoopStateManager } from "./loop/LoopStateManager";
import { ToolExecutionEngine } from "./loop/ToolExecutionEngine";

export type AgentLoopEvent =
    | { type: 'USER_INPUT'; text: string; isHeartbeat: boolean; bypassRateLimit: boolean; isDryRun?: boolean }
    | { type: 'SPEECH_START' }
    | { type: 'BARGE_IN' }
    | { type: 'STREAM_START' }
    | { type: 'EXECUTION_DONE' }
    | { type: 'EXECUTION_ERROR'; error: unknown };

export class AgentLoop implements LoopStateDelegate {
    #orchestrator: ModelOrchestrator;
    #aiRouterClient: OpenAI | NativeIPCClient;
    #aiExpertClient: OpenAI | NativeIPCClient;
    #memory: MemoryManager;
    #registry: SkillRegistry;
    #authority: CoreKernelAuthority;
    #semanticRouter: SemanticRouter;
    #semanticCache: SemanticCache;

    #onThinkingStart?: () => void | Promise<void>;
    #onThinkingEnd?: () => void | Promise<void>;
    #onStreamStart?: () => void | Promise<void>;
    #onStreamChunk?: (chunk: string) => void | Promise<void>;
    #onThoughtChunk?: (chunk: string) => void | Promise<void>;
    #onSpokenResponse?: (text: string) => void | Promise<void>;
    #onRecoveryReset?: () => void | Promise<void>;
    #onLatencyMask?: (route: string) => void | Promise<void>;

    public onSystemBusy?: (message: string) => void | Promise<void>;
    public onExecApprovalRequired?: (toolName: string, command: string, reason: string) => Promise<{ approved: boolean; editedCommand?: string }>;
    public onToolStream?: (pt: unknown) => void | Promise<void>;

    public channelRouter: ChannelRouter | null = null;

    #taskBus: EventEmitter = new EventEmitter();
    #laneWorkers: Map<TaskLane, TaskLaneWorker> = new Map();
    #currentPhase: AgentPhase = AgentPhase.INITIALIZING;

    // Sub-modules
    public readonly promptCompiler: PromptCompiler;
    public readonly loopStateManager: LoopStateManager;
    public readonly toolExecutionEngine: ToolExecutionEngine;

    constructor(memory: MemoryManager, registry: SkillRegistry) {
        this.#memory = memory;
        this.#registry = registry;
        this.#authority = CoreKernelAuthority.getInstance();
        this.#orchestrator = new ModelOrchestrator();
        this.#semanticRouter = new SemanticRouter();
        this.#semanticCache = new SemanticCache();

        const configMgr = ConfigManager.getInstance();
        const AI_PROVIDER = configMgr.aiProvider;
        const USE_NATIVE_IPC = configMgr.isNativeMode;
        
        let expertUrl = `http://127.0.0.1:${this.#orchestrator.expertPort}/v1`;
        let expertKey = "local-ghost-expert";

        if (AI_PROVIDER === "cloud") {
            expertUrl = process.env.AI_BASE_URL || "";
            expertKey = process.env.AI_API_KEY || "";
            if (!expertUrl || !expertKey) {
                logger.error("🛑 [FATAL] Cấu hình Cloud API bị thiếu. Vui lòng kiểm tra AI_BASE_URL và AI_API_KEY trong file .env!");
                throw new Error("Missing Cloud API Credentials for Hybrid Mode!");
            }
            logger.info("☁️ [Hybrid Architecture] Mạch não E4B (Router) cắm Local, Cụm 26B (Expert) dùng Cloud API!");
        }

        this.#aiRouterClient = USE_NATIVE_IPC
            ? new NativeIPCClient()
            : new OpenAI({
                baseURL: `http://127.0.0.1:${this.#orchestrator.routerPort}/v1`,
                apiKey: "local-ghost-router",
                timeout: 30000,
                maxRetries: 1
            });

        if (AI_PROVIDER === "cloud") {
            this.#aiExpertClient = new OpenAI({
                baseURL: expertUrl,
                apiKey: expertKey,
                timeout: 60000,
                maxRetries: 2
            });
        } else {
            this.#aiExpertClient = USE_NATIVE_IPC
                ? new NativeIPCClient()
                : new OpenAI({
                    baseURL: expertUrl,
                    apiKey: expertKey,
                    timeout: 60000,
                    maxRetries: 2
                });
        }

        Object.values(TaskLane).forEach((lane) => {
            this.#laneWorkers.set(lane, new TaskLaneWorker(lane, this.#taskBus));
        });

        // Initialize sub-modules
        this.promptCompiler = new PromptCompiler();
        this.loopStateManager = new LoopStateManager(this);
        this.toolExecutionEngine = new ToolExecutionEngine(memory, registry, this.#aiRouterClient);
        this.toolExecutionEngine.activeAgentLoopRef = this;

        logger.info("💻 [System] Kiến trúc Single Expert Model (P4) + XState v5 đã nạp cốt lõi.");
    }

    public async initModels() {
        try {
            await this.#orchestrator.startSingleExpert();
            await this.#semanticRouter.initialize();
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error("Lỗi khi mồi Router Server:" + " " + errMsg);
        }
    }

    public get Orchestrator() {
        return this.#orchestrator;
    }

    public get aiRouterClient() {
        return this.#aiRouterClient;
    }

    public get aiExpertClient() {
        return this.#aiExpertClient;
    }

    public get authority() {
        return this.#authority;
    }

    public get currentPhase() {
        return this.#currentPhase;
    }

    public get semanticRouter() {
        return this.#semanticRouter;
    }

    public get semanticCache() {
        return this.#semanticCache;
    }

    public get toolOrchestrator() {
        return this.toolExecutionEngine.toolOrchestrator;
    }

    public set toolOrchestrator(val) {
        this.toolExecutionEngine.toolOrchestrator = val;
    }

    public get activeMessagingIntent() {
        return this.toolExecutionEngine.activeMessagingIntent;
    }

    public set activeMessagingIntent(val) {
        this.toolExecutionEngine.activeMessagingIntent = val;
    }

    public get currentSystemLocation() {
        return this.promptCompiler.currentSystemLocation;
    }

    public set currentSystemLocation(loc: string) {
        this.promptCompiler.currentSystemLocation = loc;
    }

    public get currentSystemTimezone() {
        return this.promptCompiler.currentSystemTimezone;
    }

    public set currentSystemTimezone(tz: string) {
        this.promptCompiler.currentSystemTimezone = tz;
    }

    public setSystemLocation(loc: string, tz: string = "Asia/Ho_Chi_Minh") {
        this.promptCompiler.setSystemLocation(loc, tz);
    }

    public get onThinkingStart() { return this.loopStateManager.wrapCallback(this.#onThinkingStart); }
    public set onThinkingStart(val) { this.#onThinkingStart = val; }

    public get onThinkingEnd() { return this.loopStateManager.wrapCallback(this.#onThinkingEnd); }
    public set onThinkingEnd(val) { this.#onThinkingEnd = val; }

    public get onStreamStart() { return this.loopStateManager.wrapCallback(this.#onStreamStart); }
    public set onStreamStart(val) { this.#onStreamStart = val; }

    public get onStreamChunk() { return this.loopStateManager.wrapCallback(this.#onStreamChunk); }
    public set onStreamChunk(val) { this.#onStreamChunk = val; }

    public get onThoughtChunk() { return this.loopStateManager.wrapCallback(this.#onThoughtChunk); }
    public set onThoughtChunk(val) { this.#onThoughtChunk = val; }

    public get onSpokenResponse() { return this.loopStateManager.wrapCallback(this.#onSpokenResponse); }
    public set onSpokenResponse(val) { this.#onSpokenResponse = val; }

    public get onRecoveryReset() { return this.loopStateManager.wrapCallback(this.#onRecoveryReset); }
    public set onRecoveryReset(val) { this.#onRecoveryReset = val; }

    public get onLatencyMask() { return this.loopStateManager.wrapCallback(this.#onLatencyMask); }
    public set onLatencyMask(val) { this.#onLatencyMask = val; }

    public get isBusy(): boolean {
        return this.loopStateManager.getCurrentStateValue() !== 'idle';
    }

    public dispatch(task: MessageTask, token: AuthorityToken<AgentPhase>): void {
        if (!this.#authority.verify(token, this.#currentPhase)) {
            throw new Error("Unauthorized Task Dispatch! Invalid Authority Token.");
        }
        this.#taskBus.emit(task.lane as string, task, token);
    }

    public async handleUserInput(userText: string, isHeartbeat: boolean = false, bypassRateLimit: boolean = false, isDryRun: boolean = false): Promise<void> {
        const currentTurn = ++this.loopStateManager.activeTurnCount;
        return this.loopStateManager.turnStorage.run(currentTurn, async () => {
            if (!this.loopStateManager.checkRateLimit(userText, isHeartbeat, bypassRateLimit)) {
                if (this.onSystemBusy) {
                    this.onSystemBusy("Bạn đang gửi tin nhắn quá nhanh. Vui lòng chậm lại 1 giây!");
                }
                return;
            }

            if (!this.loopStateManager.checkVramGuard(userText)) {
                if (this.onSystemBusy) {
                    this.onSystemBusy(`Tin nhắn quá dài (${userText.length} ký tự). Vui lòng cắt ngắn dưới 20.000 ký tự để LIVA có thể đọc được!`);
                }
                return;
            }

            if (!this.#orchestrator.isReady() && (this.#orchestrator.isWarmingUp || this.#orchestrator.isSwapping)) {
                logger.info("[AgentLoop] Engine is warming up or swapping models. Initiating dynamic wait loop up to 90 seconds...");
                if (this.onStreamStart) {
                    await this.onStreamStart();
                }
                const waitMsg = this.#orchestrator.isSwapping
                    ? "⚡ Đang hoán đổi mô hình trí tuệ nhân tạo, vui lòng đợi trong giây lát..."
                    : "⚡ Đang khởi động và nạp mô hình AI Core, vui lòng chờ khoảng 15-30 giây...";
                if (this.onStreamChunk) {
                    await this.onStreamChunk(waitMsg);
                }
                if (this.onSpokenResponse) {
                    await this.onSpokenResponse(waitMsg);
                }

                for (let i = 0; i < 90; i++) {
                    if (this.#orchestrator.isReady()) {
                        logger.info("[AgentLoop] Engine became ready during wait loop.");
                        break;
                    }
                    if (!this.#orchestrator.isWarmingUp && !this.#orchestrator.isSwapping) {
                        logger.info("[AgentLoop] Engine stopped warming up or swapping.");
                        break;
                    }
                    await new Promise((resolve) => setTimeout(resolve, 1000));
                }
            }

            if (!this.#orchestrator.isReady() && (!process.env.FALLBACK_AI_BASE_URL || !process.env.FALLBACK_AI_API_KEY)) {
                logger.warn(`[Circuit Breaker] Local Daemon Yielded & No Cloud Fallback Configured.`);
                if (this.onSpokenResponse) this.onSpokenResponse("Hệ thống AI lõi đang bận xử lý ứng dụng nặng và không có kết nối đám mây dự phòng. Vui lòng chờ...");
                return;
            }
            
            this.loopStateManager.sendActorEvent({ type: 'USER_INPUT', text: userText, isHeartbeat, bypassRateLimit, isDryRun });
        });
    }

    public _executeUserInput(text: string, isHeartbeat: boolean, bypassRateLimit: boolean, isDryRun?: boolean): void {
        this.toolExecutionEngine.execute(text, isHeartbeat, bypassRateLimit, isDryRun || false, this);
    }

    public executeUserInput(text: string, isHeartbeat: boolean, bypassRateLimit: boolean, isDryRun?: boolean): void {
        this._executeUserInput(text, isHeartbeat, bypassRateLimit, isDryRun);
    }

    public internalBargeIn(): void {
        const snapshotId = `snapshot-bargein-${Date.now()}`;
        const filePath = `E:\\AI_Models\\snapshots\\${snapshotId}.bin`;
        
        try {
            const syscallPromise = Scheduler.getInstance().emitSyscall({
                type: "syscall_snapshot_save",
                priority: SyscallPriority.HRT,
                payload: { slotId: 0, filePath }
            });
            
            if (syscallPromise && typeof syscallPromise.catch === 'function') {
                syscallPromise.catch((e: unknown) => {
                    const errMsg = e instanceof Error ? e.message : String(e);
                    logger.debug(`[Barge-in Snapshot] Lưu trạng thái VRAM bị từ chối ngầm: ${errMsg}`);
                });
            }
        } catch (syncError: unknown) {
            const errMsg = syncError instanceof Error ? syncError.message : String(syncError);
            logger.error(`[Barge-in Kernel Panic] Lỗi đồng bộ khi bắn Syscall: ${errMsg}`);
        }
    }

    public async unloadModel(): Promise<void> {
        await this.#orchestrator.killLlamaServer();
    }

    public bargeIn(type: 'BARGE_IN' | 'SPEECH_START' = 'BARGE_IN'): void {
        this.loopStateManager.bargeIn(type);
    }

    public async speculativeWarm(partialText: string): Promise<void> {
        await this.promptCompiler.speculativeWarm(partialText, this.#memory, this.#registry, this.#semanticRouter);
    }

    public clearSpeculativeCache(): void {
        this.promptCompiler.clearSpeculativeCache();
    }

    public async shutdown() {
        const termToken = this.#authority.issueToken(AgentPhase.TERMINATING);
        this.transitionTo(AgentPhase.TERMINATING, termToken);
        
        this.loopStateManager.shutdown();
        this.toolExecutionEngine.shutdown();

        TaskQueue.getInstance().dispose();

        if (this.#memory && typeof this.#memory.dispose === "function") {
            this.#memory.dispose();
        }

        SensoryManager.getInstance().dispose();

        await this.#orchestrator.dispose();
        logger.info("🛑 [System] AgentLoop đã đóng hoàn toàn.");
    }

    public transitionTo(phase: AgentPhase, token: AuthorityToken<AgentPhase>): void {
        if (!token || !this.#authority.verify(token, phase)) {
            throw new Error("Unauthorized State Transition Attempted! Invalid Token.");
        }
        this.#currentPhase = phase;
    }
}
