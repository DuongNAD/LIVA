import OpenAI from "openai";
import { EventEmitter } from 'node:events';
import { NativeIPCClient } from "../utils/NativeIPCClient";
import { createHash } from "node:crypto"; // 🔒 [Memory Fix #7] Dùng SHA1 hash thay JSON.stringify cho actionHash
import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { ZMAS_Guard } from "../security/ZMAS_Guard";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { notifyZalo } from "../utils/ZaloNotifier";
import { ModelOrchestrator } from "./ModelOrchestrator";
import { PromptBuilder } from "./PromptBuilder";

/**
 * [SINGULARITY UPGRADE] 
 * Implementation of TypeScript 5.x Branded Types for absolute type-level integrity.
 */
export type Brand<T, TBread> = T & { readonly __brand_identity: TBread };

export type AgentPhaseType = Brand<string, "AgentPhase">;
export type TaskLaneType = Brand<string, "TaskLane">;

// Factory functions for controlled creation of Branded Types
const createPhase = (p: string): AgentPhaseType => p as unknown as AgentPhaseType;
const createLane = (l: string): TaskLaneType => l as unknown as TaskLaneType;

export const AgentPhase = {
    INITIALIZING: createPhase("INITIALIZING"),
    RUNNING: createPhase("RUNNING"),
    PAUSING: createPhase("PAUSING"),
    TERMINATING: createPhase("TERMINATING"),
} as const;
export type AgentPhase = AgentPhaseType;

/**
 * [ZERO-TRUST TOKEN]
 * Uses Private Class Members (#) to prevent unauthorized access to the secret.
 */
export class AuthorityToken<S extends AgentPhase> {
    public readonly phase: S;
    #secret: string;

    constructor(phase: S, secret: string) {
        this.phase = phase;
        this.#secret = secret;
    }

    public isValid(expectedPhase: S, expectedSecret: string): boolean {
        return this.phase === expectedPhase && this.#secret === expectedSecret;
    }
}

/**
 * [KERNEL AUTHORITY]
 * Centralized authority for issuing and verifying tokens within the core orchestration loop.
 */
export class CoreKernelAuthority {
    #kernelSecret = "LIVA_KERNEL_CORE_99X_ALPHA";
    static #instance: CoreKernelAuthority;

    private constructor() { }

    public static getInstance(): CoreKernelAuthority {
        if (!CoreKernelAuthority.#instance) {
            CoreKernelAuthority.#instance = new CoreKernelAuthority();
        }
        return CoreKernelAuthority.#instance;
    }

    public issueToken<S extends AgentPhase>(phase: S): AuthorityToken<S> {
        return new AuthorityToken<S>(phase, this.#kernelSecret);
    }

    public verify<S extends AgentPhase>(token: AuthorityToken<S>, phase: S): boolean {
        return token.isValid(phase, this.#kernelSecret);
    }
}

export const TaskLane = {
    UI_INTERACTION: createLane("ui_interaction"),
    LLM_REASONING: createLane("llm_reasoning"),
    BACKGROUND_JOB: createLane("background_job"),
} as const;
export type TaskLane = TaskLaneType;

export enum TaskState {
    PENDING = "PENDING",
    EXECUTING = "EXECUTING",
    COMPLETED = "COMPLETED",
    FAILED = "FAILED"
}

export interface MessageTask {
    id: string;
    lane: TaskLane;
    data: any;
    state?: TaskState;
    execute: (token: AuthorityToken<AgentPhase>) => Promise<void>;
}

/** 
 * [NEW SUB-AGENT] 
 * DualPortController: Manages the lifecycle and circuit breaking of Router vs Expert.
 */
export class DualPortController {
    #orchestrator: ModelOrchestrator;
    #isExpertAwake = false;

    constructor(orchestrator: ModelOrchestrator) {
        this.#orchestrator = orchestrator;
    }

    get isExpertAwake() { return this.#isExpertAwake; }

    async ensureExpertReady(): Promise<boolean> {
        try {
            if (this.#isExpertAwake) return true;
            await this.#orchestrator.stopRouter();

            // Token issuance bound strictly to Core limits
            await this.#orchestrator.startExpert(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("EXPERT_START_AUTH"));
            this.#isExpertAwake = true;
            return true;
        } catch (e: any) {
            logger.error("[CircuitBreaker] VRAM Overload. Expert Load Failed. Falling back to Router.", e.message);
            await this.#orchestrator.startRouter(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH"));
            this.#isExpertAwake = false;
            return false;
        }
    }

    async releaseResources() {
        if (this.#isExpertAwake) {
            logger.info("🛡️ [CircuitBreaker] RAII Triggered: Giải phóng VRAM Expert để tránh kẹt Deadlock...");
            try {
                await this.#orchestrator.stopExpert();
            } catch (e) { void e; }
            this.#isExpertAwake = false;
        }
    }
}

/** 
 * [NEW SUB-AGENT] 
 * ToolExecutionOrchestrator: Handles execution and the crucial "Reflection" layer!
 */
export class ToolExecutionOrchestrator {
    #registry: SkillRegistry;
    #aiRouterClient: OpenAI;
    public onExecApprovalRequired?: (toolName: string, command: string, reason: string) => Promise<{ approved: boolean; editedCommand?: string }>;

    constructor(registry: SkillRegistry, routerClient: OpenAI) {
        this.#registry = registry;
        this.#aiRouterClient = routerClient;
    }

    async executeWithReflection(toolName: string, args: any): Promise<{ resultStr: string; valid: boolean; rawObj: any }> {
        try {
            const resultObj = await this.#registry.executeSkill(toolName, args);
            let resultStr = typeof resultObj === "string" ? resultObj : JSON.stringify(resultObj);

            const zmas = new ZMAS_Guard();
            resultStr = zmas.executeAutoRemediation(resultStr, toolName);

            if (resultStr.length > 2000) {
                logger.warn(`Dữ liệu dài (${resultStr.length} chars). Chuyển hướng chui qua [Sanitizer Sub-Agent]...`);
                resultStr = await this.sanitize(resultStr);
            }

            // [REFLECTION LAYER V2 — Rule-Based Validation]
            // Thay thế AI Reflection (chậm ~3s, sai lệch trên Router 4B) bằng heuristic nhanh O(1)
            const lowerResult = resultStr.toLowerCase();
            const isValid = resultStr.length > 5
                && !lowerResult.includes("traceback (most recent call last)")
                && !lowerResult.includes("error: spawn")
                && !lowerResult.includes("econnrefused")
                && !lowerResult.includes("timeout sandbox")
                && !(resultStr.startsWith("{") && resultStr.includes('"error"'));

            return { resultStr, valid: isValid, rawObj: resultObj };
        } catch (toolError: any) {
            return { resultStr: `Tool runtime error: ${toolError.message}`, valid: false, rawObj: null };
        }
    }

    private async sanitize(rawString: string): Promise<string> {
        try {
            const res = await this.#aiRouterClient.chat.completions.create({
                model: "router",
                messages: [
                    { role: "system", content: "You are a neutral data filter. ACCURATELY AND OBJECTIVELY SUMMARIZE the provided content. MUST NOT reply or address anyone, only return the raw summarized text." },
                    { role: "user", content: `Summarize:\n${rawString.substring(0, 6000)}` }
                ],
                temperature: 0.1,
            });
            return res.choices[0].message?.content || rawString.substring(0, 1500);
        } catch {
            return rawString.substring(0, 1500) + "\n\n[System: Data too large, safely trimmed]";
        }
    }
}

/** 
 * [NEW SUB-AGENT] 
 * LTCOrchestrator: The Cognitive Summarizer that builds "Working Concepts" out of short-term interactions.
 */
export class LTCOrchestrator {
    #memory: MemoryManager;
    #aiRouterClient: OpenAI;

    constructor(memory: MemoryManager, routerClient: OpenAI) {
        this.#memory = memory;
        this.#aiRouterClient = routerClient;
    }

    async summarizeAndStore(userQuery: string, finalReply: string) {
        try {
            const summaryPrompt = `Extract 1 OR MAXIMUM 2 core FACTS/DECISIONS from this chat snippet. Format as brief observations (e.g., "User provided X", "Agreed to do Y"). Max 15 words. If it is just a casual greeting with no new information, respond EXACTLY with 'NONE'.\n\nUser: ${userQuery}\nLIVA: ${finalReply}`;

            const reflection = await this.#aiRouterClient.chat.completions.create({
                model: "router",
                messages: [{ role: "user", content: summaryPrompt }],
                temperature: 0.1,
            });

            const fact = reflection.choices[0].message?.content?.trim();
            if (fact && fact.length > 3 && !fact.toUpperCase().includes("NONE")) {
                logger.info(`[LTC Engine] Đang đúc kết quy luật vào Ký Ức Dài Hạn: ${fact.substring(0, 50)}...`);
                await this.#memory.updateLongTermMemory("Working Concepts", [fact]);
            }
        } catch (e: any) {
            logger.error("[LTC Engine] Không thể trích xuất Concept:", e.message);
        }
    }
}

/** 
 * [NEW SUB-AGENT]
 * TaskLaneWorker: Subscribes to the TaskBus and processes tasks for a specific lane asynchronously.
 * Implements Pub/Sub Consumer Logic.
 */
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
                const timeoutPromise = new Promise((_, reject) =>
                    setTimeout(() => reject(new Error("Task execution timed out (Chain Breaker)")), 300000)
                );
                
                Promise.race([executionPromise, timeoutPromise])
                    .then(() => { task.state = TaskState.COMPLETED; })
                    .catch(error => {
                        task.state = TaskState.FAILED;
                        logger.error(`[TaskLaneWorker ${this.#lane}] Lỗi tại [$${task.id}] (State: ${task.state}):`, error);
                    })
                    .finally(() => {
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

/**
 * [AGENT LOOP - EVOLVED]
 * High-integrity orchestration loop with validated state transitions and private client management.
 */
export class AgentLoop {
    #orchestrator: ModelOrchestrator;
    #aiRouterClient: OpenAI | NativeIPCClient;
    #aiExpertClient: OpenAI;
    #memory: MemoryManager;
    #registry: SkillRegistry;
    #authority: CoreKernelAuthority;

    // Evolved Sub-Agents
    #dualPort: DualPortController;
    #toolOrchestrator: ToolExecutionOrchestrator;
    #ltcOrchestrator: LTCOrchestrator;

    public onThinkingStart?: () => void | Promise<void>;
    public onThinkingEnd?: () => void | Promise<void>;
    public onStreamStart?: () => void | Promise<void>;
    public onStreamChunk?: (chunk: string) => void | Promise<void>;
    public onSpokenResponse?: (text: string) => void | Promise<void>;
    public onExecApprovalRequired?: (toolName: string, command: string, reason: string) => Promise<{ approved: boolean; editedCommand?: string }>;

    #taskBus: EventEmitter = new EventEmitter();
    #laneWorkers: Map<TaskLane, TaskLaneWorker> = new Map();
    #currentPhase: AgentPhase = AgentPhase.INITIALIZING;

    public isBusy: boolean = false;

    // V13: Zalo Downtime Queueing System
    #zaloPendingQueue: string[] = [];
    #queueDaemonActive = false;

    #startQueueDaemon() {
        if (this.#queueDaemonActive) return;
        this.#queueDaemonActive = true;
        const interval = setInterval(async () => {
            if (this.#zaloPendingQueue.length === 0) {
                clearInterval(interval);
                this.#queueDaemonActive = false;
                return;
            }
            try {
                // Ping Router port
                const res = await fetch(`http://127.0.0.1:${this.#orchestrator.routerPort}/`, { signal: AbortSignal.timeout(2000) });
                if (res.status) {
                    logger.info(`🟢 [Zalo Queue] 7B Router đã sống lại! Đang xả kho ${this.#zaloPendingQueue.length} tin nhắn Zalo bị giam...`);
                    const backlog = [...this.#zaloPendingQueue];
                    this.#zaloPendingQueue = [];
                    for (const msg of backlog) {
                        this.handleUserInput(msg); // Trả lại Pipeline ngay lập tức
                    }
                }
            } catch (e) { void e; }
        }, 15000); // Check 15s một lần
    }

    public currentSystemLocation = "Vị trí không xác định";

    constructor(memory: MemoryManager, registry: SkillRegistry) {
        this.#memory = memory;
        this.#registry = registry;
        this.#authority = CoreKernelAuthority.getInstance();
        this.#orchestrator = new ModelOrchestrator();

        // [HYBRID CLOUD-LOCAL] Router dùng Dynamic Port từ ModelOrchestrator
        const AI_PROVIDER = process.env.AI_PROVIDER?.toLowerCase() || "local";
        const USE_NATIVE_IPC = process.env.LIVA_USE_NATIVE !== "false";
        
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

        this.#aiRouterClient = (USE_NATIVE_IPC)
            ? new NativeIPCClient()
            : new OpenAI({
                baseURL: `http://127.0.0.1:${this.#orchestrator.routerPort}/v1`, // [DYNAMIC PORT]
                apiKey: "local-ghost-router", // Bypass credential
                timeout: 30000,
                maxRetries: 1
            });

        // Expert Client (Hybrid Mode)
        this.#aiExpertClient = new OpenAI({
            baseURL: expertUrl,
            apiKey: expertKey,
            timeout: 60000,
            maxRetries: 2
        });

        // Mount Sub-Agents
        this.#dualPort = new DualPortController(this.#orchestrator);
        this.#toolOrchestrator = new ToolExecutionOrchestrator(registry, this.#aiRouterClient as any);
        this.#toolOrchestrator.onExecApprovalRequired = async (toolName, command, reason) => {
            if (this.onExecApprovalRequired) {
                return await this.onExecApprovalRequired(toolName, command, reason);
            }
            logger.warn(`[Zero-Trust] Không có UI gắn kết để duyệt lệnh. Tự động từ chối lệnh nguy hiểm.`);
            return { approved: false };
        };
        this.#ltcOrchestrator = new LTCOrchestrator(memory, this.#aiRouterClient as any);

        Object.values(TaskLane).forEach((lane) => {
            this.#laneWorkers.set(lane, new TaskLaneWorker(lane, this.#taskBus));
        });
        logger.info("💻 [System] Kiến trúc Orchestrator Mới (Dual-Port) đã nạp cốt lõi.");
    }

    public async initModels() {
        try {
            // Using the authorized token factory from ModelOrchestrator
            await this.#orchestrator.startRouter(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH"));
        } catch (e: any) {
            logger.error("Lỗi khi mồi Router Server:", e.message);
        }
    }

    public get Orchestrator() {
        return this.#orchestrator;
    }

    public setSystemLocation(loc: string) {
        this.currentSystemLocation = loc;
    }

    /**
     * [SECURE DISPATCH]
     * Validates the authority token against the current phase before allowing task execution.
     * Publishes the task to the TaskBus for asynchronous LaneWorker execution.
     */
    public dispatch(task: MessageTask, token: AuthorityToken<AgentPhase>): void {
        if (!this.#authority.verify(token, this.#currentPhase)) {
            throw new Error("Unauthorized Task Dispatch! Invalid Authority Token.");
        }
        // Emit task to the specific task lane (Pub/Sub pattern)
        this.#taskBus.emit(task.lane as string, task, token);
    }

    public handleUserInput(userText: string, isHeartbeat: boolean = false) {
        if (this.isBusy) {
            if (isHeartbeat) {
                logger.info(`[Heartbeat] ⚠️ Bỏ qua nhịp đập do AgentLoop đang bận.`);
                return;
            }
            logger.warn(`⚠️ Hệ thống đang bận xử lý tác vụ khác. Chặn: ${userText.substring(0, 50)}`);
            if (this.onSpokenResponse) this.onSpokenResponse("Liva đang bận một chút, xin anh đợi xíu nhé.");
            return;
        }
        
        this.isBusy = true;

        const dispatchToken = this.#authority.issueToken(this.#currentPhase);
        this.dispatch({
            id: `voice-cmd-${Date.now()}`,
            lane: TaskLane.LLM_REASONING,
            data: { text: userText },
            execute: async (executionToken: AuthorityToken<AgentPhase>) => {
                if (!this.#authority.verify(executionToken, this.#currentPhase)) throw new Error("Invalid execution token in LLM Lane");
                if (this.onThinkingStart) this.onThinkingStart();

                logger.info(`Đang Load Ngữ Cảnh...`);

                // Báo cáo Zalo Mid-flight khi bắt đầu nhận Job
                if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                    try {
                        await this.#registry.executeSkill("send_zalo_bot", {
                            message: "⚡ Dạ thưa sếp, LIVA đã tiếp nhận yêu cầu và đang đánh giá. Dự kiến mất 10-15s nếu là tìm kiếm mạng nhẹ, hoặc 1-2 phút nếu cần chuyển giao não chuyên gia. Xin sếp ráng nán lại chờ nha!"
                        });
                    } catch { }
                }

                try {
                    const toolsDef = this.#registry.getAllSkills().map((skill: any) => ({
                        name: skill.name,
                        description: skill.description,
                        parameters: skill.parameters,
                    }));

                    const aiMessages = await PromptBuilder.prepareFullAiMessages(
                        userText,
                        this.#memory,
                        this.currentSystemLocation,
                        toolsDef
                    );

                    let isFinished = false;
                    let turnCount = 0;
                    let finalReply = "";
                    let isExpertAwake = false;
                    const allExecutedTools: string[] = [];

                    // Deterministic Guardrail (Hàng rào chối từ hành động lặp)
                    const actionHistory = new Set<string>();

                    let currentQuery = userText;

                    // Streaming Helper function
                    const generateText = async (
                        msgs: any[],
                        newQuery: string,
                        useExpert: boolean = false,
                        maxTokens: number = 2500,
                    ) => {
                        const localMsgs = [...msgs, { role: "user", content: newQuery }];

                        // Quyết định dùng Router hay Expert
                        const client = useExpert ? this.#aiExpertClient : this.#aiRouterClient;
                        const usingTarget = process.env.AI_PROVIDER?.toLowerCase() === "cloud" 
                            ? (process.env.AI_MODEL || "gpt-4") 
                            : (useExpert ? "local-ghost-expert" : "local-ghost-router");

                        const stream = await client.chat.completions.create({
                            model: usingTarget,
                            messages: localMsgs,
                            temperature: 0.3,
                            max_tokens: maxTokens,
                            stream: true,
                        });

                        let fullContent = "";
                        let buffer = "";
                        let isToolCallMode = false;
                        let passedBufferCheck = false;

                        for await (const chunk of stream as any) {
                            const token = chunk.choices[0]?.delta?.content || "";
                            fullContent += token;

                            if (!passedBufferCheck) {
                                buffer += token;
                                if (buffer.length >= 15 || chunk.choices[0]?.finish_reason) {
                                    passedBufferCheck = true;

                                    const recentTail = buffer.slice(-30);
                                    if (
                                        recentTail.includes("<to") ||
                                        buffer.includes('{"name":') ||
                                        buffer.trim().startsWith("{")
                                    ) {
                                        isToolCallMode = true;
                                        logger.info("[Stream Mute] 🤫 LIVA đang nhẩm tính lệnh Kỹ năng ngầm...");
                                    } else {
                                        if (this.onStreamStart) this.onStreamStart();
                                        if (this.onStreamChunk) this.onStreamChunk(buffer);
                                    }
                                }
                            } else {
                                if (!isToolCallMode) {
                                    if (this.onStreamChunk) this.onStreamChunk(token);
                                }
                            }
                        }
                        return fullContent;
                    };

                    const MAX_ITERATIONS = 5;

                    while (!isFinished && turnCount < MAX_ITERATIONS) {
                        turnCount++;

                        if (turnCount === MAX_ITERATIONS) {
                            isFinished = true;
                            finalReply = `LIVA đã thử 5 hướng tiếp cận khác nhau nhưng vẫn gặp rào cản kỹ thuật. Quá trình xử lý phức tạp vượt quá mức trần an toàn của vòng lặp.\nAnh Dương vui lòng hướng dẫn thêm cho em hoặc thử chẻ nhỏ yêu cầu này ra giúp em nhé!`;
                            logger.info("Graceful Exit: LLM chạm mốc lặp 5 lần vướng ngõ cụt.");
                            break;
                        }

                        logger.info(`Đang đập cánh luồng Tư Duy bằng [$${isExpertAwake ? "Expert Model 26B" : "Router Model 4B"}] (Vòng #${turnCount})...`);

                        const responseRawText = await generateText(
                            aiMessages,
                            currentQuery,
                            isExpertAwake
                        );
                        logger.debug({ response: responseRawText }, `RAW AI Response (Turn ${turnCount}):`);

                        let contentText = responseRawText || "";
                        let parsedToolCalls: any[] = [];

                        // XML Tool Parser
                        if (contentText.includes("<tool_call>")) {
                            try {
                                const regex = /<tool_call>([\s\S]*?)<\/tool_call>/g;
                                const matches = [...contentText.matchAll(regex)];
                                if (matches && matches.length > 0) {
                                    for (const match of matches) {
                                        if (match[1]) {
                                            const toolJson = JSON.parse(match[1].trim());
                                            parsedToolCalls.push(toolJson);
                                        }
                                    }
                                    contentText = contentText.replaceAll(regex, "").trim();
                                }
                            } catch (e: any) {
                                logger.error("Lỗi Regex Parse Multi-Tool:", e.message);
                            }
                        } else if (contentText.includes('{"name":') && contentText.includes("}")) {
                            // JSON Fallback
                            try {
                                const match = contentText.match(/(\{(?:[^{}]|(?!<)\{(?:[^{}]|(?!<)\{.*?\})*?\})\})/); // NOSONAR
                                if (match) {
                                    const toolJson = JSON.parse(match[1].trim());
                                    if (toolJson.name) parsedToolCalls = [toolJson];
                                    contentText = contentText.replace(match[1], "").trim();
                                }
                            } catch (e: any) { void e; }
                        }

                        if (parsedToolCalls.length > 0) {
                            logger.info({ parsedToolCalls }, `AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`);
                            let finalToolResults = "";

                            aiMessages.push({ role: "user", content: currentQuery });
                            aiMessages.push({ role: "assistant", content: responseRawText });

                            for (const toolCall of parsedToolCalls) {
                                const functionName = toolCall.name;

                                // Logic Cascade Handoff KHÔNG ĐỘT TỬ
                                if (functionName === "handoff_to_expert") {
                                    logger.warn(`🚀 [Handoff] Router gọi cứu viện. Đang ép 26B lên VRAM GPU (Router nghỉ ngơi giữ chỗ)...`);
                                    if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                                        try {
                                            await this.#registry.executeSkill("send_zalo_bot", {
                                                message: "🔥 LIVA: Tá vụ này khá căng nên em đang đẩy não Chuyên Gia 26B lên VRAM! Không cần reload toàn bộ hệ thống nữa nên chỉ chờ khoảng 5s..."
                                            });
                                        } catch (e) { }
                                    }

                                    const isAwake = await this.#dualPort.ensureExpertReady();
                                    isExpertAwake = isAwake;
                                    if (isAwake) {
                                        finalToolResults += `[Hệ thống]: Handoff Zero-Overhead Thành Công sang Expert Model (Cổng 8001 VRAM). Các tham số trước đó đã tự động được bê sang. Hãy phục vụ user ngay nhé.\n\n`;
                                    } else {
                                        finalToolResults += `[Hệ thống Lỗi]: Handoff thất bại! Có thể do VRAM bị tràn cứng. Đã chuyển lại cho Router Model xử lý cục bộ...\n\n`;
                                    }
                                    continue;
                                }

                                allExecutedTools.push(functionName);

                                let functionArgs: any = null;
                                try {
                                    let argsStr = toolCall.arguments;
                                    if (typeof argsStr === "string") {
                                        argsStr = argsStr.replaceAll("\n", "\\n").replaceAll("\r", "\\r").replaceAll("\t", "\\t");
                                        functionArgs = JSON.parse(argsStr);
                                    } else {
                                        functionArgs = argsStr;
                                    }
                                } catch (e: any) {
                                    logger.error(`Lỗi Parse JSON Argument định dạng hỏng kỹ năng ${functionName}`, e.message);
                                }

                                if (functionArgs === null) {
                                    logger.warn(`Bỏ qua Kỹ năng ${functionName} do LLM trả sai cấu trúc Arguments.`);
                                    finalToolResults += `[Hệ thống]: Không thể chạy ${functionName} vì Argument JSON bị định dạng sai. Vui lòng thử lại với khối Argument chuẩn.\n\n`;
                                    continue;
                                }

                                logger.info(`Đang chạy hàm: ${functionName}`, functionArgs);

                                // 🔒 [Memory Fix #7] Dùng SHA1 hash thay vì JSON.stringify ngêm vào Set
                                // JSON.stringify(functionArgs) có thể lên tới hàng KB (nếu args chứa nội dung file code)
                                // SHA1 luôn cho ra 40 ký tự → Set luôn ổn định về bộ nhớ
                                const actionHash = createHash("sha1")
                                    .update(`${functionName}::${JSON.stringify(functionArgs).substring(0, 256)}`)
                                    .digest("hex");
                                if (actionHistory.has(actionHash)) {
                                    logger.warn(`🛑 Chặn LLM lặp lại hành động sai y hệt vòng trước: ${functionName}`);
                                    finalToolResults += `[SYSTEM_ALERT]: Hệ thống từ chối thực thi! Bạn đang lặp lại chính xác hành động cũ "${functionName}" với cùng một tham số đã thất bại ở lượt trước. LỆNH BẮT BUỘC: Bạn KHÔNG ĐƯỢC lặp lại tham số cũ. Hãy phân tích kỹ lỗi, điều chỉnh tham số, thử công cụ khác, hoặc gọi 'handoff_to_expert'.\n\n`;
                                    continue;
                                }
                                actionHistory.add(actionHash);

                                // Use [ToolExecutionOrchestrator] for execution with built-in reflection and loop-prevention!
                                const executionResult = await this.#toolOrchestrator.executeWithReflection(functionName, functionArgs);
                                logger.info(`Kết quả chạy hàm ${functionName} (Valid: ${executionResult.valid}):`, executionResult.rawObj);

                                if (executionResult.valid) {
                                    finalToolResults += `[Hệ thống trả kết quả từ ${functionName}]:\n[EXTERNAL_DATA_START]\n${executionResult.resultStr}\n[EXTERNAL_DATA_END]\n\n`;
                                } else {
                                    logger.warn(`Tool ${functionName} bị Reflection chặn hoặc báo lỗi Runtime.`);
                                    finalToolResults += `[SYSTEM_ALERT]: Kỹ năng hỏng vì "${executionResult.resultStr}". LỆNH BẮT BUỘC: Kẻ chỉ trích nội bộ (Internal Critic) phát hiện Output vừa rồi là RÁC hoặc LỖI. Hãy ngưng ngay hành động lặp lại công cụ này và chuyển hướng (gọi công cụ khác, đổi tham số, hoặc handoff_to_expert).\n\n`;
                                }
                            }

                            let nextActionPrompt = `[DỮ LIỆU TỪ CÔNG CỤ VỪA CHẠY]:\n${finalToolResults}`;
                            const executedTools = parsedToolCalls.map((t) => t.name).join(", ");

                            if (!executedTools.includes("zalo") && turnCount < MAX_ITERATIONS - 1 && userText.toLowerCase().includes("zalo")) {
                                nextActionPrompt += `\n[Gợi ý]: Hãy gọi \`send_zalo_bot\` để gửi Zalo cho Sếp.`;
                            } else {
                                nextActionPrompt += `\n[Hệ thống]: Dữ liệu đã ráp nối. Vui lòng dựa vào đó để phản hồi trực tiếp cho người dùng. Đừng luẩn quẩn nữa.`;
                            }
                            currentQuery = nextActionPrompt;
                        } else {
                            aiMessages.push({ role: "user", content: currentQuery });
                            aiMessages.push({ role: "assistant", content: responseRawText });

                            isFinished = true;
                            finalReply = contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.";
                            logger.info(`Liva phản hồi cuối (Final Response): "${finalReply}"`);
                        }
                    }

                    await this.#memory.addMessage("user", userText);
                    await this.#memory.addMessage("assistant", finalReply);

                    SensoryManager.getInstance().flush();

                    if (this.onThinkingEnd) this.onThinkingEnd();
                    if (this.onSpokenResponse) this.onSpokenResponse(finalReply);

                    if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                        await notifyZalo(finalReply);
                    }

                    // [LTC] Đúc kết lại lượt hội thoại để nuôi dưỡng Working Concepts chạy nền không block UI
                    this.#ltcOrchestrator.summarizeAndStore(userText, finalReply).catch((e: any) => { });

                } catch (error: any) {
                    logger.error("Lỗi kết nối Ghost Server:", error.message);
                    if (this.onThinkingEnd) this.onThinkingEnd();

                    if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                        // V13: Đánh chặn Lỗi Timeout / Tắt Cổng lúc 26B Chiếm Dụng VRAM!
                        if (error.message.includes("ECONNREFUSED") || error.message.includes("fetch failed") || error.message.includes("timeout")) {
                            logger.warn(`🤖 [Zalo Suspend Queue]: Sếp chờ chút nha! Server AI đang tiến hóa (VRAM bị chiếm). Tạm lưu tin nhắn: "${userText}"`);
                            this.#zaloPendingQueue.push(userText);
                            this.#startQueueDaemon(); // Đánh thức Daemmon rà quét và đợi
                            return;
                        } else {
                            await notifyZalo(`❌ Lỗi hệ thống Zalo: ${error.message}`);
                        }
                    } else {
                        if (this.onSpokenResponse) {
                            this.onSpokenResponse(`❌ Văng Native AI: ${error.message}`);
                        }
                    }
                } finally {
                    // [CIRCUIT BREAKER] Guaranteed Resource Release regardless of API crashes
                    await this.#dualPort.releaseResources();
                    this.isBusy = false;
                }
            },
        }, dispatchToken);
    }

    /**
     * [SECURE TRANSITION]
     * Validates the authority token against the target phase before allowing state change.
     */
    private transitionTo(phase: AgentPhase, token: AuthorityToken<AgentPhase>): void {
        if (!token || !this.#authority.verify(token, phase)) {
            throw new Error("Unauthorized State Transition Attempted! Invalid Token.");
        }
        this.#currentPhase = phase;
        logger.info(`🔄 [State Machine] Chuyển sang trạng thái: ${phase}`);
    }

    public async shutdown() {
        const termToken = this.#authority.issueToken(AgentPhase.TERMINATING);
        this.transitionTo(AgentPhase.TERMINATING, termToken);
        
        // Cầu chì cắt nguồn System Memory GC Daemons chống rò rỉ RAM
        if (this.#memory && typeof this.#memory.dispose === "function") {
            this.#memory.dispose();
        }

        await this.#orchestrator.stopExpert();
        await this.#orchestrator.stopRouter();
        logger.info("🛑 [System] AgentLoop đã đóng hoàn toàn.");
    }
}
