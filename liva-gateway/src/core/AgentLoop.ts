import OpenAI from "openai";
import { setup, createActor, assign } from "xstate";
import { EventEmitter } from 'node:events';
import { NativeIPCClient } from "../utils/NativeIPCClient";
import { createHash, randomUUID } from "node:crypto"; // 🔒 [Memory Fix #7] Dùng SHA1 hash thay JSON.stringify cho actionHash
import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { ModelOrchestrator } from "./ModelOrchestrator";
import { PromptBuilder } from "./PromptBuilder";
import { SemanticRouter } from "../memory/SemanticRouter";
import { SemanticCache } from "../memory/SemanticCache";
import { TraceContext } from "../utils/TraceContext";
import type { ChannelRouter } from "../channels/ChannelNormalizer";
import { AgentPhase, TaskLane, AuthorityToken, MessageTask } from "../types/AgentTypes";
import { ConfigManager } from "./config/ConfigManager";
import { CoreKernelAuthority } from "./CoreKernelAuthority";
import { ToolExecutionOrchestrator } from "./orchestrators/ToolExecutionOrchestrator";
import { LTCOrchestrator } from "./orchestrators/LTCOrchestrator";
import { TaskLaneWorker } from "./orchestrators/TaskLaneWorker";
import { StreamSanitizer } from "./stream/StreamSanitizer";
import { ToolCallExtractor } from "./stream/ToolCallExtractor";
import { PersistentQueue } from "./queue/PersistentQueue";
import { TaskQueue, TaskPriority } from "./TaskQueue";
import { Scheduler } from "../kernel/Scheduler";
import { SyscallPriority } from "../kernel/SyscallInterface";
import {
    isAmbiguousChannel,
    resolveChannelFromReply,
    buildClarificationMessage,
    buildPreferenceKey,
    buildPreferenceValue,
    MESSAGING_TOOLS,
    type PendingChannelAction,
} from "./ChannelDisambiguationGate";

// Định nghĩa danh sách các dấu hiệu nhận biết tin nhắn hệ thống/lỗi
const SYSTEM_FALLBACK_SIGNATURES = [
    "Xin lỗi Anh, em chưa rõ ý này",
    "JSON Parsing Error",
    "Mất kết nối",
    "Lỗi hệ thống",
    "AI Core"
];

export class AgentLoop {
    #orchestrator: ModelOrchestrator;
    #aiRouterClient: OpenAI | NativeIPCClient;
    #aiExpertClient: OpenAI | NativeIPCClient;
    #memory: MemoryManager;
    #registry: SkillRegistry;
    #authority: CoreKernelAuthority;

    // Evolved Sub-Agents
    #toolOrchestrator: ToolExecutionOrchestrator;
    #ltcOrchestrator: LTCOrchestrator;
    #semanticRouter: SemanticRouter;
    #semanticCache: SemanticCache;

    public onThinkingStart?: () => void | Promise<void>;
    public onThinkingEnd?: () => void | Promise<void>;
    public onStreamStart?: () => void | Promise<void>;
    public onStreamChunk?: (chunk: string) => void | Promise<void>;
    public onThoughtChunk?: (chunk: string) => void | Promise<void>;
    public onSpokenResponse?: (text: string) => void | Promise<void>;
    public onSystemBusy?: (message: string) => void | Promise<void>;  // [v25 FIX] System notification when busy
    public onExecApprovalRequired?: (toolName: string, command: string, reason: string) => Promise<{ approved: boolean; editedCommand?: string }>;
    public onToolStream?: (pt: any) => void | Promise<void>;
    public onRecoveryReset?: () => void | Promise<void>;

    // [v23 Pillar 3] Latency Masking — plays filler audio for heavy routes
    public onLatencyMask?: (route: string) => void | Promise<void>;

    public channelRouter: ChannelRouter | null = null;

    #taskBus: EventEmitter = new EventEmitter();
    #laneWorkers: Map<TaskLane, TaskLaneWorker> = new Map();
    #currentPhase: AgentPhase = AgentPhase.INITIALIZING;

    // [v26] Rate Limiter State
    private lastInputTime: number = 0;
    private readonly RATE_LIMIT_MS: number = 1000; // 1 second minimum between messages

    // V13: Zalo Downtime Queueing System — Now backed by SQLite (crash-resilient)
    #pendingQueue: PersistentQueue = new PersistentQueue();
    #queueDaemonActive = false;
    #queueDaemonRef: ReturnType<typeof setInterval> | null = null;

    // [v26 Phase 2] XState v5 Actor Model
    #stateMachineActor: ReturnType<typeof createActor>;

    // [Phase 3] Extracted stream processing modules
    #streamSanitizer: StreamSanitizer = new StreamSanitizer();
    #toolCallExtractor: ToolCallExtractor = new ToolCallExtractor();

    // [v22 Full-Duplex Pillar 2] Context-Aware Barge-in
    #streamAbortController: AbortController | null = null;
    #spokenTokenCount = 0;        // Tracks how many tokens were streamed to UI/TTS
    #currentStreamedText = "";    // Accumulates the text that was actually spoken
    #wasBargedIn = false;         // Flag: was the current response interrupted?

    // [v23 Pillar 2] Speculative RAG Warming — pre-fetched context cache
    #speculativeCache: { 
        route?: import("../memory/SemanticRouter").MemoryRoute; 
        activeKit?: import("../memory/SemanticRouter").SkillKit; 
        skills?: any[];
        aiMessages?: any[];
        dynamicContextBlock?: string;
    } | null = null;

    // [v27] Channel Disambiguation Gate — Pending State Machine
    // Stores the pending messaging action when gate asks user to pick a channel
    #pendingChannelAction: PendingChannelAction | null = null;
    static readonly PENDING_ACTION_TTL_MS = 120_000; // 2 minutes TTL

    #startQueueDaemon() {
        if (this.#queueDaemonActive) return;
        this.#queueDaemonActive = true;
        // 🔒 [P1-1.3] Store interval ref to prevent timer leak on shutdown
        this.#queueDaemonRef = setInterval(async () => {
            const isZaloEmpty = this.#pendingQueue.isEmpty("zalo");
            const isTelegramEmpty = this.#pendingQueue.isEmpty("telegram");
            if (isZaloEmpty && isTelegramEmpty) {
                if (this.#queueDaemonRef) clearInterval(this.#queueDaemonRef);
                this.#queueDaemonRef = null;
                this.#queueDaemonActive = false;
                return;
            }
            try {
                // 🔒 [Audit C-4] Ping Router port via safeFetch (handles HTTP 4xx/5xx properly)
                const res = await safeFetch(`http://127.0.0.1:${this.#orchestrator.routerPort}/`, {}, 2000);
                if (res.status) {
                    if (!isZaloEmpty) {
                        const backlog = this.#pendingQueue.dequeueAll("zalo");
                        logger.info(`🟢 [Zalo Queue] 7B Router đã sống lại! Đang xả kho ${backlog.length} tin nhắn Zalo bị giam...`);
                        for (const msg of backlog) {
                            this.handleUserInput(msg); // Trả lại Pipeline ngay lập tức
                        }
                    }
                    if (!isTelegramEmpty) {
                        const backlog = this.#pendingQueue.dequeueAll("telegram");
                        logger.info(`🟢 [Telegram Queue] 7B Router đã sống lại! Đang xả kho ${backlog.length} tin nhắn Telegram bị giam...`);
                        for (const msg of backlog) {
                            this.handleUserInput(msg); // Trả lại Pipeline ngay lập tức
                        }
                    }
                }
            } catch (e) { void e; }
        }, 15000); // Check 15s một lần
    }

    public currentSystemLocation = "Vị trí chưa xác định";
    public currentSystemTimezone = "Asia/Ho_Chi_Minh";

    constructor(memory: MemoryManager, registry: SkillRegistry) {
        this.#memory = memory;
        this.#registry = registry;
        this.#authority = CoreKernelAuthority.getInstance();
        this.#orchestrator = new ModelOrchestrator();
        this.#semanticRouter = new SemanticRouter();
        this.#semanticCache = new SemanticCache();

        // [HYBRID CLOUD-LOCAL] Router dùng Dynamic Port từ ModelOrchestrator
        // [v27 FIX] Unified env parsing via ConfigManager — Single Source of Truth
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

        // [LLM INFERENCE CLIENT]
        // LIVA_USE_NATIVE=true  → NativeIPCClient (gRPC port 8100, Python Engine)
        // LIVA_USE_NATIVE=false → OpenAI HTTP (port 8000, llama-server.exe C++)
        this.#aiRouterClient = USE_NATIVE_IPC
            ? new NativeIPCClient()
            : new OpenAI({
                baseURL: `http://127.0.0.1:${this.#orchestrator.routerPort}/v1`, // [DYNAMIC PORT]
                apiKey: "local-ghost-router", // Bypass credential
                timeout: 30000,
                maxRetries: 1
            });

        // Expert Client
        if (AI_PROVIDER === "cloud") {
            this.#aiExpertClient = new OpenAI({
                baseURL: expertUrl,
                apiKey: expertKey,
                timeout: 60000,
                maxRetries: 2
            });
        } else {
            // In Local Mode, Expert is the same engine as Router (Single Expert Architecture)
            this.#aiExpertClient = USE_NATIVE_IPC
                ? new NativeIPCClient()
                : new OpenAI({
                    baseURL: expertUrl,
                    apiKey: expertKey,
                    timeout: 60000,
                    maxRetries: 2
                });
        }

        // Mount Sub-Agents — #aiRouterClient is OpenAI|NativeIPCClient union; ToolExecutionOrchestrator expects OpenAI
        this.#toolOrchestrator = new ToolExecutionOrchestrator(registry, this.#aiRouterClient as unknown as OpenAI);
        this.#toolOrchestrator.onExecApprovalRequired = async (toolName, command, reason) => {
            if (this.onExecApprovalRequired) {
                return await this.onExecApprovalRequired(toolName, command, reason);
            }
            logger.warn(`[Zero-Trust] Không có UI gắn kết để duyệt lệnh. Tự động từ chối lệnh nguy hiểm.`);
            return { approved: false };
        };
        this.#ltcOrchestrator = new LTCOrchestrator(memory, this.#aiRouterClient as unknown as OpenAI);

        Object.values(TaskLane).forEach((lane) => {
            this.#laneWorkers.set(lane, new TaskLaneWorker(lane, this.#taskBus));
        });

        // ==========================================
        // [v26 Phase 2] XState v5 State Machine - Two-Stage Barge-in
        // ==========================================
        const agentMachine = setup({
            types: {
                context: {} as {
                    nextPendingMessage: string | null;
                    agentLoop: AgentLoop;
                },
                events: {} as
                    | { type: 'USER_INPUT'; text: string; isHeartbeat: boolean; bypassRateLimit: boolean; isDryRun?: boolean }
                    | { type: 'SPEECH_START' }
                    | { type: 'BARGE_IN' }
                    | { type: 'STREAM_START' }
                    | { type: 'EXECUTION_DONE' }
                    | { type: 'EXECUTION_ERROR'; error: any },
                input: {} as { agentLoop: AgentLoop }
            },
            actions: {
                queuePendingMessage: assign({
                    nextPendingMessage: ({ event }) => (event as any).text || null
                }),
                triggerAbort: ({ context }) => {
                    context.agentLoop._internalBargeIn();
                },
                notifyBusy: ({ context }) => {
                    if (context.agentLoop.onSystemBusy) {
                        context.agentLoop.onSystemBusy("Liva đang dừng suy nghĩ cũ để xử lý câu hỏi mới của bạn!");
                    }
                },
                startExecution: ({ context, event }) => {
                    if (event.type === 'USER_INPUT') {
                        context.agentLoop._executeUserInput(event.text, event.isHeartbeat, event.bypassRateLimit, event.isDryRun);
                    }
                },
                checkPendingMessage: ({ context }) => {
                    if (context.nextPendingMessage) {
                        const msg = context.nextPendingMessage;
                        // Execute on next tick to avoid synchronous loop
                        setTimeout(() => {
                            context.agentLoop.handleUserInput(msg, false, true);
                        }, 0);
                    }
                },
                clearPendingMessage: assign({
                    nextPendingMessage: null
                })
            }
        }).createMachine({
            id: 'agentLoop',
            initial: 'idle',
            context: ({ input }) => ({
                nextPendingMessage: null,
                agentLoop: input.agentLoop
            }),
            states: {
                idle: {
                    entry: ['checkPendingMessage', 'clearPendingMessage'],
                    on: {
                        USER_INPUT: {
                            target: 'thinking',
                            actions: ['startExecution']
                        },
                        BARGE_IN: {}, // Ignore
                        SPEECH_START: {} // Ignore
                    }
                },
                thinking: {
                    on: {
                        USER_INPUT: {
                            target: 'aborting',
                            actions: ['queuePendingMessage', 'triggerAbort', 'notifyBusy']
                        },
                        SPEECH_START: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        BARGE_IN: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        STREAM_START: {
                            target: 'streaming'
                        },
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                },
                streaming: {
                    on: {
                        USER_INPUT: {
                            target: 'aborting',
                            actions: ['queuePendingMessage', 'triggerAbort', 'notifyBusy']
                        },
                        SPEECH_START: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        BARGE_IN: {
                            target: 'aborting',
                            actions: ['triggerAbort']
                        },
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                },
                aborting: {
                    on: {
                        USER_INPUT: {
                            actions: ['queuePendingMessage', 'notifyBusy']
                        },
                        SPEECH_START: {}, // Already aborting
                        BARGE_IN: {}, // Already aborting
                        STREAM_START: {}, // Ignore
                        EXECUTION_DONE: { target: 'idle' },
                        EXECUTION_ERROR: { target: 'idle' }
                    }
                }
            }
        });

        this.#stateMachineActor = createActor(agentMachine, { input: { agentLoop: this } });
        this.#stateMachineActor.start();

        logger.info("💻 [System] Kiến trúc Single Expert Model (P4) + XState v5 đã nạp cốt lõi.");
    }

    public async initModels() {
        try {
            await this.#orchestrator.startSingleExpert(); // Start the GPU engine first
            await this.#semanticRouter.initialize(); // [Dynamic Gating] Init kit anchors
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error("Lỗi khi mồi Router Server:" + " " + errMsg);
        }
    }

    public get Orchestrator() {
        return this.#orchestrator;
    }

    public setSystemLocation(loc: string, tz: string = "Asia/Ho_Chi_Minh") {
        this.currentSystemLocation = loc;
        this.currentSystemTimezone = tz;
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

    public get isBusy(): boolean {
        // We consider the loop busy if the XState actor is NOT in 'idle'
        const state = this.#stateMachineActor.getSnapshot().value;
        return state !== 'idle';
    }

    public handleUserInput(userText: string, isHeartbeat: boolean = false, bypassRateLimit: boolean = false, isDryRun: boolean = false) {
        // --- V26 HARDENING GUARDRAILS ---

        // [Đề xuất 3] Rate Limiter chống Spam / Kẹt vòng lặp Bot (Bảo vệ CPU)
        const now = Date.now();
        if (!isHeartbeat && !bypassRateLimit) {
            if (now - this.lastInputTime < this.RATE_LIMIT_MS) {
                logger.warn(`[Rate Limiter] Thao tác quá nhanh! Bỏ qua tin nhắn: ${userText.substring(0, 50)}`);
                if (this.onSystemBusy) {
                    this.onSystemBusy("Bạn đang gửi tin nhắn quá nhanh. Vui lòng chậm lại 1 giây!");
                }
                return;
            }
            this.lastInputTime = now;
        }

        // [Đề xuất 2] VRAM Guard: Token Sliding Limit (Bảo vệ Llama.cpp khỏi Segfault)
        const MAX_INPUT_LENGTH = 20000; // Khoảng 6000 tokens
        if (userText.length > MAX_INPUT_LENGTH) {
            logger.warn(`[VRAM Guard] Từ chối input quá dài (${userText.length} ký tự). Tránh Segfault!`);
            if (this.onSystemBusy) {
                this.onSystemBusy(`Tin nhắn quá dài (${userText.length} ký tự). Vui lòng cắt ngắn dưới 20.000 ký tự để LIVA có thể đọc được!`);
            }
            return;
        }
        // --- END GUARDRAILS ---

        if (!this.#orchestrator.isReady() && (!process.env.FALLBACK_AI_BASE_URL || !process.env.FALLBACK_AI_API_KEY)) {
            logger.warn(`[Circuit Breaker] Local Daemon Yielded & No Cloud Fallback Configured.`);
            if (this.onSpokenResponse) this.onSpokenResponse("Hệ thống AI lõi đang bận xử lý ứng dụng nặng và không có kết nối đám mây dự phòng. Vui lòng chờ...");
            return;
        }
        
        // Dispatch to XState Actor
        this.#stateMachineActor.send({ type: 'USER_INPUT', text: userText, isHeartbeat, bypassRateLimit, isDryRun });
    }

    /**
     * [v26 Phase 2] Thực thi logic sinh Text. 
     * Hàm này ĐƯỢC GỌI BỞI XState Actor.
     */
    public _executeUserInput(userText: string, isHeartbeat: boolean, bypassRateLimit: boolean, isDryRun: boolean = false) {

        const dispatchToken = this.#authority.issueToken(this.#currentPhase);
        this.dispatch({
            id: `voice-cmd-${Date.now()}`,
            lane: TaskLane.LLM_REASONING,
            data: { text: userText },
            execute: async (executionToken: AuthorityToken<AgentPhase>) => {
                if (!this.#authority.verify(executionToken, this.#currentPhase)) throw new Error("Invalid execution token in LLM Lane");
                
                // [Memory Sync] Reset consolidation idle timer on user interaction
                if (!isHeartbeat) {
                    this.#memory.consolidationCron?.touch();
                }

                // MUTE BACKGROUND HEARTBEAT THINKING UI
                if (!isHeartbeat) {
                    if (this.onThinkingStart) this.onThinkingStart();
                }

                // [v22] Reset barge-in tracking for new response
                this.#spokenTokenCount = 0;
                this.#currentStreamedText = "";
                this.#wasBargedIn = false;

                if (isDryRun) {
                    await this.#memory.clearSession();
                    logger.info(`[Dry Run] Đã dọn dẹp ngữ cảnh để tránh tràn bộ nhớ.`);
                }

                logger.info(`Đang Load Ngữ Cảnh...`);

                // ===========================
                // [v27] Channel Disambiguation Gate — Pending State Resolution
                // When user replies with a channel name (e.g., "Zalo") after gate asked,
                // merge the pending action and execute directly without re-inferring.
                // ===========================
                if (this.#pendingChannelAction && !isHeartbeat) {
                    const pending = this.#pendingChannelAction;
                    const age = Date.now() - pending.timestamp;

                    if (age > AgentLoop.PENDING_ACTION_TTL_MS) {
                        // Expired — discard and proceed normally
                        logger.info(`[ChannelGate] Pending action expired (${Math.round(age / 1000)}s). Discarding.`);
                        this.#pendingChannelAction = null;
                    } else {
                        const resolvedTool = resolveChannelFromReply(userText);
                        if (resolvedTool) {
                            // User answered the channel question! Merge and execute.
                            this.#pendingChannelAction = null;
                            logger.info(`[ChannelGate] ✅ Channel resolved: ${resolvedTool} for "${pending.recipientName}"`);

                            if (this.onThinkingEnd) this.onThinkingEnd();

                            try {
                                const mergedArgs = { targetName: pending.recipientName, message: pending.message };
                                // For email, args are different (to, subject, body_text)
                                const finalArgs = resolvedTool === "send_email"
                                    ? { to: pending.recipientName, subject: "Message from LIVA", body_text: pending.message }
                                    : mergedArgs;

                                const result = await this.#toolOrchestrator.executeWithReflection(resolvedTool, finalArgs);
                                const reply = result.valid ? result.resultStr : `Xin lỗi, em không thể gửi tin nhắn lúc này.`;

                                await this.#memory.addMessage("user", userText);
                                await this.#memory.addMessage("assistant", reply);

                                if (this.onStreamStart) await this.onStreamStart();
                                if (this.onStreamChunk) await this.onStreamChunk(reply);
                                if (this.onSpokenResponse) this.onSpokenResponse(reply);

                                // Learn preference in StructuredMemory
                                const sm = this.#memory.getStructuredMemoryInstance();
                                if (sm) {
                                    const prefKey = buildPreferenceKey(pending.recipientName);
                                    const existingFact = sm.getFact(prefKey);
                                    const newValue = buildPreferenceValue(resolvedTool, existingFact?.value || null);
                                    await sm.setFact(prefKey, newValue, {
                                        source: "system",
                                        category: "channel_preference",
                                        ttlDays: 90,
                                    });
                                    logger.info(`[ChannelGate] 📝 Saved preference: ${prefKey} = ${newValue}`);
                                }
                            } catch (e: unknown) {
                                const errMsg = e instanceof Error ? e.message : String(e);
                                logger.warn(`[ChannelGate] Pending action execution failed: ${errMsg}`);
                            } finally {
                                this.#sendExecutionDoneIfActive();
                            }
                            return;
                        }
                        // User didn't reply with a channel → discard pending and process normally
                        logger.info(`[ChannelGate] User replied with non-channel text. Discarding pending action.`);
                        this.#pendingChannelAction = null;
                    }
                }

                // [v27 Phase 1] Semantic Cache (Phản xạ vô điều kiện)
                if (!isHeartbeat) {
                    const cacheHit = this.#semanticCache.get(userText);
                    if (cacheHit) {
                        const reply = cacheHit.response;
                        
                        await this.#memory.addMessage("user", userText);
                        await this.#memory.addMessage("assistant", reply);

                        if (this.onThinkingEnd) this.onThinkingEnd();
                        if (this.onStreamStart) await this.onStreamStart();
                        if (this.onStreamChunk) await this.onStreamChunk(reply);
                        if (this.onSpokenResponse) this.onSpokenResponse(reply);

                        this.#sendExecutionDoneIfActive();
                        return; // Ngắt luồng gọi LLM và trả kết quả ngay (0ms latency)
                    }
                }



                try {
                    // [v23 Pillar 2] Check speculative cache — skip route() if already pre-warmed
                    let routerResult;
                    let activeKit;
                    let cachedSkills: any[] | undefined;
                    let hydratedMessages: any[] | undefined;
                    let cachedDynamicContextBlock: string | undefined;
                    if (this.#speculativeCache?.route) {
                        routerResult = { route: this.#speculativeCache.route, activeKit: this.#speculativeCache.activeKit };
                        activeKit = this.#speculativeCache.activeKit;
                        cachedSkills = this.#speculativeCache.skills;
                        hydratedMessages = this.#speculativeCache.aiMessages;
                        cachedDynamicContextBlock = this.#speculativeCache.dynamicContextBlock;
                        logger.info(`[v23 Speculative] ⚡ Using pre-warmed route: ${routerResult.route} (0ms latency)`);
                    } else {
                        // [Dynamic Gating] Tiết lộ lũy tiến bằng SemanticRouter
                        routerResult = await this.#semanticRouter.route(userText);
                        activeKit = routerResult.activeKit;
                    }
                    this.#speculativeCache = null; // Consume cache

                    // ===========================
                    // [v24 L0.5] CACHED ACTION FAST-PATH
                    // If SemanticRouter returned a cachedAction, bypass LLM entirely
                    // and execute the tool directly via SkillRegistry.
                    // ===========================
                    if (routerResult.cachedAction && !isDryRun) {
                        const { toolName, toolArgs } = routerResult.cachedAction;
                        logger.info(`\u26A1 [v24 L0.5] Direct tool execution: ${toolName} (bypass LLM)`);

                        if (!isHeartbeat && this.onThinkingEnd) this.onThinkingEnd();

                        let l05Handled = false;
                        try {
                            const result = await this.#toolOrchestrator.executeWithReflection(toolName, toolArgs);
                            const finalReplyL05 = result.valid
                                ? `${result.resultStr}`
                                : `Xin lỗi, em không thực hiện được lệnh này lúc này.`;

                            await this.#memory.addMessage("user", userText);
                            await this.#memory.addMessage("assistant", finalReplyL05);

                            if (this.onStreamStart) await this.onStreamStart();
                            if (this.onStreamChunk) await this.onStreamChunk(finalReplyL05);
                            if (this.onSpokenResponse) this.onSpokenResponse(finalReplyL05);

                            // [v28 FIX] Only send DONE + return on SUCCESS.
                            // On failure, fall through to LLM inference loop below.
                            l05Handled = true;
                            this.#sendExecutionDoneIfActive();
                            return;
                        } catch (e: unknown) {
                            const errMsg = e instanceof Error ? e.message : String(e);
                            logger.warn(`[v24 L0.5] Cached action failed, falling through to LLM: ${errMsg}`);
                            // Do NOT return, do NOT send EXECUTION_DONE.
                            // Let code continue to the while(!isFinished) loop below.
                            if (this.onRecoveryReset) await this.onRecoveryReset();
                        }
                        // If l05Handled is false, we fall through to LLM inference
                    }

                    // [v23 Pillar 3] Latency Masking — emit filler audio for heavy routes
                    const isHeavyRoute = routerResult.route === 'deep_reasoning' || routerResult.route === 'system_command';
                    if (isHeavyRoute && this.onLatencyMask) {
                        this.onLatencyMask(routerResult.route);
                    }

                    // Remote channel mid-flight warning
                    const ctx = TraceContext.getStore();
                    if (isHeavyRoute && ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                        const adapter = this.channelRouter?.getAdapter(ctx.channel as any);
                        if (adapter) {
                            adapter.sendText(ctx.userId, "⚡ Dạ thưa sếp, yêu cầu này cần xử lý chuyên sâu. LIVA đang tiến hành đánh giá và chạy nghiên cứu ngầm, có thể mất từ 15-30s. Sếp vui lòng đợi em một chút nhé! 🤖").catch((e: unknown) => {
                                logger.warn(`[AgentLoop] Remote mid-flight warning failed: ${e instanceof Error ? e.message : String(e)}`);
                            });
                        }
                    }

                    // [Bypass] Ép bỏ qua gọi Tools đối với các luồng phiếm chỉ/chào hỏi
                    let filteredSkills = cachedSkills
                        || (routerResult.route === "chitchat" ? [] : await this.#registry.getSemanticTopK(userText, activeKit, 3));
                    
                    if (isDryRun) {
                        const match = userText.match(/mang tên "([^"]+)"/);
                        if (match && match[1]) {
                            const targetTool = this.#registry.getAllSkills().find(s => s.name === match[1]);
                            if (targetTool) {
                                filteredSkills = [targetTool];
                            }
                        }
                    }
                    const toolsDef = filteredSkills.map((skill: any) => ({
                        name: skill.name,
                        description: skill.description,
                        parameters: skill.parameters,
                    }));

                    let aiMessages: any[];
                    let dynamicContextBlock: string;
                    if (hydratedMessages) {
                        aiMessages = hydratedMessages;
                        dynamicContextBlock = cachedDynamicContextBlock || "";
                    } else {
                        const result = await PromptBuilder.prepareFullAiMessages(
                            userText,
                            this.#memory,
                            {
                                location: this.currentSystemLocation,
                                timezone: this.currentSystemTimezone
                            },
                            toolsDef,
                            routerResult.route // Pass route to optimize context
                        );
                        aiMessages = result.aiMessages;
                        dynamicContextBlock = result.dynamicContextBlock;
                    }

                    let isFinished = false;
                    let turnCount = 0;
                    let finalReply = "";
                    // [v29] Check if Expert is already loaded (from Cooldown TTL window)
                    let isExpertAwake = this.#orchestrator.currentModelType === "expert";
                    if (isExpertAwake) {
                        // Touch cooldown to prevent swap-back while user is active
                        this.#orchestrator.touchExpertCooldown();
                        logger.info(`[AgentLoop] Expert model already active (Cooldown TTL window). Reusing.`);
                    }
                    const allExecutedTools: string[] = [];
                    let parsedToolCalls: any[] = [];

                    // Deterministic Guardrail (Hàng rào chối từ hành động lặp)
                    const actionHistory = new Set<string>();

                    let currentQuery = userText;

                    const nowStr = new Date().toLocaleString("vi-VN", {
                        timeZone: this.currentSystemTimezone || "Asia/Ho_Chi_Minh",
                    });
                    const dynamicContext = `\n\n<DYNAMIC_CONTEXT>\nSystem Time: ${nowStr}\nUser's Real-Time Location (via IP/GPS): ${this.currentSystemLocation}\n</DYNAMIC_CONTEXT>`;

                    // 2-Tier Inference Array: Clone the session messages for temporary LLM inference
                    const executionMessages = [...aiMessages];

                    // Streaming Helper function — delegates token filtering to StreamSanitizer
                    const generateText = async (
                        inferenceMsgs: any[],
                        useExpert: boolean = false,
                        maxTokens: number = 2500,
                    ) => {
                        // [v28 FIX] Single Source of Truth — ConfigManager replaces raw process.env reads
                        const cfgMgr = ConfigManager.getInstance();
                        let client = useExpert ? this.#aiExpertClient : this.#aiRouterClient;
                        let usingTarget = cfgMgr.aiProvider === "cloud"
                            ? (cfgMgr.env.AI_MODEL)
                            : (useExpert ? "local-ghost-expert" : "local-ghost-router");

                        // [Circuit Breaker] Fallback to Cloud if local Daemon is offline/yielded
                        if (!this.#orchestrator.isReady()) {
                            logger.warn("[Circuit Breaker] Local AI Yielded/Offline. Routing to Cloud Fallback...");
                            client = new OpenAI({
                                baseURL: cfgMgr.env.FALLBACK_AI_BASE_URL,
                                apiKey: cfgMgr.env.FALLBACK_AI_API_KEY,
                                timeout: 60000,
                            });
                            usingTarget = cfgMgr.env.FALLBACK_AI_MODEL;
                        }

                        let tempParam = 0.3;
                        let maxTokensParam = maxTokens;
                        let topPParam = 0.9;
                        try {
                            // [v27 FIX] Cached config read via ConfigManager (30s TTL)
                            // Previously: re-read + JSON.parse from disk on every inference call
                            const cfg = await ConfigManager.getInstance().getLivaConfig();
                            if (cfg?.ai?.temperature !== undefined) tempParam = cfg.ai.temperature;
                            if (cfg?.ai?.maxTokens !== undefined) maxTokensParam = cfg.ai.maxTokens;
                            if (cfg?.ai?.topP !== undefined) topPParam = cfg.ai.topP;
                        } catch (e) {
                            // Silently fallback to defaults
                        }

                        // Tự động hãm độ sáng tạo (Temperature) khi phải tổng hợp kết quả (Vòng 2+)
                        // để tránh AI "phê đá" (hallucinate) làm hỏng cấu trúc câu.
                        if (turnCount > 1 && tempParam > 0.5) {
                            tempParam = 0.5;
                        }

                        // [v26 Phase 2] Emit Syscall instead of calling directly
                        const stream: any = await Scheduler.getInstance().emitSyscall({
                            type: "syscall_infer",
                            priority: SyscallPriority.SRT, // Soft Real-Time cho luồng suy luận chat
                            payload: {
                                client,
                                usingTarget,
                                localMsgs: inferenceMsgs,
                                tempParam,
                                maxTokensParam,
                                topPParam
                            }
                        });

                        // [v22] Create AbortController for barge-in stream killing
                        this.#streamAbortController = new AbortController();
                        const abortSignal = this.#streamAbortController.signal;

                        // [Phase 3] Delegate stream filtering to extracted StreamSanitizer
                        this.#streamSanitizer.reset();
                        // stream is AsyncIterable<any> from OpenAI streaming API — cannot narrow union type at runtime
                        for await (const chunk of stream as AsyncIterable<OpenAI.Chat.ChatCompletionChunk>) {
                            // [v22] Check abort signal — break immediately on barge-in
                            if (abortSignal.aborted) {
                                logger.info("[Barge-in] 🛑 LLM stream killed by AbortController.");
                                break;
                            }

                            const rawToken = chunk.choices[0]?.delta?.content || "";
                            const isFinish = !!chunk.choices[0]?.finish_reason;
                            const result = this.#streamSanitizer.process(rawToken, isFinish);

                            if (result.action === "emit" && !isHeartbeat) {
                                if (!this.#streamSanitizer.streamStarted) {
                                    this.#stateMachineActor.send({ type: 'STREAM_START' });
                                    if (this.onStreamStart) await this.onStreamStart();
                                    this.#streamSanitizer.markStreamStarted();
                                }
                                // [v22] Track spoken tokens for memory truncation
                                this.#spokenTokenCount++;
                                this.#currentStreamedText += result.cleanToken;
                                if (this.onStreamChunk) await this.onStreamChunk(result.cleanToken);
                            } else if (result.action === "emit_thought" && !isHeartbeat) {
                                if (!this.#streamSanitizer.streamStarted) {
                                    this.#stateMachineActor.send({ type: 'STREAM_START' });
                                    if (this.onStreamStart) await this.onStreamStart();
                                    this.#streamSanitizer.markStreamStarted();
                                }
                                if (this.onThoughtChunk) await this.onThoughtChunk(result.cleanToken);
                            }
                            // "mute", "buffer", "tool_call_detected" → no UI output
                        }
                        this.#streamAbortController = null;  // Clean up
                        return this.#streamSanitizer.getFullContent();
                    };

                    const MAX_ITERATIONS = 5;

                    while (!isFinished && turnCount < MAX_ITERATIONS) {
                        turnCount++;

                        if (turnCount === MAX_ITERATIONS) {
                            isFinished = true;
                            finalReply = `LIVA đã thử 5 hướng tiếp cận khác nhau nhưng vẫn gặp rào cản kỹ thuật. Quá trình xử lý phức tạp vượt quá mức trần an toàn của vòng lặp.\nAnh Dương vui lòng hướng dẫn thêm cho em hoặc thử chẻ nhỏ yêu cầu này ra giúp em nhé!`;
                            logger.info("Graceful Exit: LLM chạm mốc lặp 5 lần vướng ngõ cụt.");
                            // [VOICE FIX] Stream synchronous message to TTS before emitting final response
                            if (this.onStreamStart) await this.onStreamStart();
                            if (this.onStreamChunk) await this.onStreamChunk(finalReply);
                            break;
                        }

                        logger.info(`Đang đập cánh luồng Tư Duy bằng [$${isExpertAwake ? "Expert Model 26B" : "Router Model 4B"}] (Vòng #${turnCount})...`);

                        if (turnCount === 1) {
                            executionMessages.push({
                                role: "user",
                                content: userText + dynamicContextBlock + dynamicContext
                            });
                        } else {
                            executionMessages.push({
                                role: "user",
                                content: currentQuery
                            });
                        }

                        // [v28] TokenGuard — Safety net: trim if total prompt exceeds context window
                        const ctxLimit = ConfigManager.getInstance().contextWindowTokens;
                        const maxResp = 2500; // max_tokens for response
                        const safetyMargin = 256; // buffer for encoding overhead
                        const hardLimitChars = (ctxLimit - maxResp - safetyMargin) * 4;
                        const totalChars = executionMessages.reduce((sum: number, m: any) => sum + (m.content?.length || 0), 0);
                        if (totalChars > hardLimitChars) {
                            logger.warn(`[TokenGuard] ⚠️ Prompt ~${Math.ceil(totalChars / 4)} tokens exceeds safe limit ${ctxLimit - maxResp - safetyMargin}. Trimming last user message...`);
                            // Trim the last user message's dynamicContextBlock portion
                            const lastMsg = executionMessages[executionMessages.length - 1];
                            if (lastMsg?.role === "user" && lastMsg.content.length > hardLimitChars * 0.5) {
                                const excess = totalChars - hardLimitChars;
                                lastMsg.content = lastMsg.content.substring(0, lastMsg.content.length - excess - 100) + "\n[...context trimmed by TokenGuard]";
                            }
                        }

                        const responseRawText = await generateText(
                            executionMessages,
                            isExpertAwake
                        );
                        logger.debug({ response: responseRawText }, `RAW AI Response (Turn ${turnCount}):`);

                        // [Phase 3] Delegate tool call extraction to ToolCallExtractor
                        const extraction = this.#toolCallExtractor.extract(responseRawText || "");
                        const contentText = extraction.cleanedContent;
                        parsedToolCalls = extraction.parsedToolCalls;

                        if (parsedToolCalls.length > 0) {
                            logger.info({ parsedToolCalls }, `AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`);

                            // ===========================
                            // [v27] Channel Disambiguation Gate
                            // Intercept ambiguous messaging tool calls and ask user to pick channel
                            // ===========================
                            if (parsedToolCalls.length === 1 && MESSAGING_TOOLS.has(parsedToolCalls[0].name)) {
                                const toolCall = parsedToolCalls[0];
                                const toolArgs = this.#toolCallExtractor.parseArguments(toolCall.name, toolCall.arguments);
                                const recipientName = toolArgs?.targetName || toolArgs?.to || "";

                                // Check StructuredMemory for learned preference
                                let channelPref: string | null = null;
                                const sm = this.#memory.getStructuredMemoryInstance();
                                if (sm && recipientName) {
                                    const prefFact = sm.getFact(buildPreferenceKey(recipientName));
                                    channelPref = prefFact?.value || null;
                                }

                                if (isAmbiguousChannel(userText, toolCall.name, recipientName, channelPref)) {
                                    // Gate activated! Save pending action and ask user
                                    this.#pendingChannelAction = {
                                        recipientName,
                                        message: toolArgs?.message || toolArgs?.body_text || "",
                                        originalUserText: userText,
                                        timestamp: Date.now(),
                                    };

                                    const clarification = buildClarificationMessage(recipientName);
                                    logger.info(`[ChannelGate] 🔔 Gate activated for "${recipientName}". Asking user to pick channel.`);

                                    await this.#memory.addMessage("user", userText);
                                    await this.#memory.addMessage("assistant", clarification);

                                    if (this.onThinkingEnd) this.onThinkingEnd();
                                    if (this.onStreamStart) await this.onStreamStart();
                                    if (this.onStreamChunk) await this.onStreamChunk(clarification);
                                    if (this.onSpokenResponse) this.onSpokenResponse(clarification);

                                    this.#sendExecutionDoneIfActive();
                                    return;
                                }
                            }

                            let finalToolResults = "";

                            executionMessages.push({ role: "assistant", content: responseRawText });

                            // ⚡ [P0-1.1] Parallel Tool Execution
                            // Classify tools into sequential (side-effects, handoff) and parallel (read-only)
                            const SEQUENTIAL_TOOLS = new Set([
                                "handoff_to_expert", "write_local_file", "delete_local_file",
                                "execute_command", "send_zalo_bot", "send_email",
                                "update_memory", "update_session_state", "update_core_profile",
                                "git_sync_project", "create_google_doc", "append_google_doc",
                            ]);

                            // Pre-process: parse args and compute action hashes for all tools
                            interface PreparedTool {
                                toolCall: any;
                                functionName: string;
                                functionArgs: any;
                                actionHash: string;
                                isSequential: boolean;
                                isDuplicate: boolean;
                            }

                            const preparedTools: PreparedTool[] = [];
                            for (const toolCall of parsedToolCalls) {
                                const functionName = toolCall.name;

                                // Handoff is always sequential with special handling
                                if (functionName === "handoff_to_expert") {
                                    preparedTools.push({
                                        toolCall, functionName, functionArgs: toolCall.arguments,
                                        actionHash: "", isSequential: true, isDuplicate: false,
                                    });
                                    continue;
                                }

                                // [Phase 3] Delegate argument parsing to ToolCallExtractor
                                const functionArgs = this.#toolCallExtractor.parseArguments(functionName, toolCall.arguments);

                                // 🔒 [Memory Fix #7] SHA1 hash for duplicate detection
                                const actionHash = functionArgs
                                    ? createHash("sha1")
                                        .update(`${functionName}::${JSON.stringify(functionArgs).substring(0, 256)}`)
                                        .digest("hex")
                                    : "";
                                const isDuplicate = actionHash ? actionHistory.has(actionHash) : false;

                                preparedTools.push({
                                    toolCall, functionName, functionArgs, actionHash,
                                    isSequential: SEQUENTIAL_TOOLS.has(functionName) || (toolCall.requiresApproval === true),
                                    isDuplicate,
                                });
                            }

                            // Execute a single prepared tool (shared logic)
                            const executeSingleTool = async (pt: PreparedTool): Promise<string> => {
                                // Handoff — special case: Hot-Swap Router → Expert
                                if (pt.functionName === "handoff_to_expert") {
                                    logger.warn(`🚀 [Handoff] Router gọi cứu viện. Hot-swapping to Expert model...`);
                                    
                                    // [Phase 3] A2A Protocol: Agent-to-Agent message
                                    Scheduler.getInstance().emitSyscall({
                                        type: "syscall_a2a_message",
                                        priority: SyscallPriority.HRT,
                                        payload: {
                                            sender: "Router-4B",
                                            receiver: "Expert-26B",
                                            message: `Handoff Transfer. User Query: ${userText}`
                                        }
                                    }).catch(() => {});

                                    // [v29] Stream latency-masking notification to user
                                    const swapNotification = "⚡ Em đang tắt model nhẹ và nạp model Chuyên Gia 26B vào VRAM, chờ em khoảng 10-15 giây...\n";
                                    if (this.onStreamStart) await this.onStreamStart();
                                    if (this.onStreamChunk) await this.onStreamChunk(swapNotification);

                                    // Notify remote channels
                                    const ctx = TraceContext.getStore();
                                    if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                                        const adapter = this.channelRouter?.getAdapter(ctx.channel as any);
                                        if (adapter) {
                                            adapter.sendText(ctx.userId, "🔥 LIVA: Tác vụ này khá căng nên em đang đẩy Chuyên Gia 26B lên VRAM! Chờ em 10-15 giây...").catch((e: unknown) => {
                                                logger.warn(`[Handoff] Remote handoff warning failed: ${e instanceof Error ? e.message : String(e)}`);
                                            });
                                        }
                                    }

                                    // [v29] Perform actual model hot-swap via gRPC SwapModel
                                    const swapSuccess = await this.#orchestrator.swapToExpert();
                                    isExpertAwake = swapSuccess;

                                    if (swapSuccess) {
                                        logger.info(`[Handoff] ✅ Expert model is now active on VRAM.`);
                                        return `[SYSTEM]: Handoff Successful. Expert Model (26B) is now loaded and processing. Please serve the user immediately with your full reasoning capability.\n\n`;
                                    } else {
                                        logger.warn(`[Handoff] ❌ Expert swap failed. Falling back to Router model.`);
                                        return `[SYSTEM_ERROR]: Handoff failed! Could not swap to Expert model. Using current Router model to handle the request locally. Do your best.\n\n`;
                                    }
                                }

                                if (pt.functionArgs === null) {
                                    logger.warn(`Bỏ qua Kỹ năng ${pt.functionName} do LLM trả sai cấu trúc Arguments.`);
                                    return `[SYSTEM]: Cannot execute ${pt.functionName} because the Argument JSON is malformed. Please try again with standard Argument JSON structure.\n\n`;
                                }

                                if (pt.isDuplicate) {
                                    logger.warn(`🛑 Chặn LLM lặp lại hành động sai y hệt vòng trước: ${pt.functionName}`);
                                    return `[SYSTEM_WARNING]: Command rejected! You are repeating the exact same action "${pt.functionName}" with the identical failed parameters. Please adjust parameters, try a different tool, or respond to the user in their preferred language.\n\n`;
                                }

                                if (pt.actionHash) actionHistory.add(pt.actionHash);
                                allExecutedTools.push(pt.functionName);

                                logger.info(`Đang chạy hàm: ${pt.functionName}`, pt.functionArgs);
                                // [v26 Phase 2] Chuyển đổi thành Syscall thay vì gọi ToolOrchestrator trực tiếp
                                let executionResult: any;
                                
                                if ((globalThis as any).kernelInstance?.ui) {
                                    (globalThis as any).kernelInstance.ui.broadcastUIEvent("test_tool_execution", {
                                        toolName: pt.functionName
                                    });
                                }

                                if (isDryRun) {
                                    logger.info(`[Dry Run Mode] Bỏ qua thực thi thực tế hàm: ${pt.functionName}`);
                                    executionResult = { valid: true, resultStr: "Mock data success for dry run test.", rawObj: { dryRun: true } };
                                } else {
                                    executionResult = await Scheduler.getInstance().emitSyscall({
                                        type: "syscall_execute_tool",
                                        priority: pt.isSequential ? SyscallPriority.SRT : SyscallPriority.DT,
                                        payload: {
                                            toolOrchestrator: this.#toolOrchestrator,
                                            functionName: pt.functionName,
                                            functionArgs: pt.functionArgs
                                        }
                                    });
                                }
                                logger.info(`Kết quả chạy hàm ${pt.functionName} (Valid: ${executionResult.valid}):`, executionResult.rawObj);

                                if (executionResult.valid) {
                                    // [v24 L0.5] Record successful tool execution for future cache hits
                                    this.#semanticRouter.recordAction(userText, pt.functionName, pt.functionArgs).catch(() => {});
                                    return `[RESULTS FROM TOOL ${pt.functionName}]:\n[EXTERNAL_DATA_START]\n${executionResult.resultStr}\n[EXTERNAL_DATA_END]\n\n`;
                                } else {
                                    logger.warn(`Tool ${pt.functionName} bị Reflection chặn hoặc báo lỗi Runtime.`);
                                    return `[SYSTEM_WARNING]: Tool execution failed: "${executionResult.resultStr}". Please analyze the failure and pivot to a different approach (e.g., try 'web_search' or 'web_browser') in your next thought, rather than apologizing to the user.\n\n`;
                                }
                            };

                            // Split into parallel and sequential groups
                            const parallelTools = preparedTools.filter(pt => !pt.isSequential);
                            const sequentialTools = preparedTools.filter(pt => pt.isSequential);

                            // ⚡ Execute parallel tools first via Promise.allSettled
                            if (parallelTools.length > 1) {
                                logger.info(`⚡ [Parallel] Chạy ${parallelTools.length} tools đọc song song...`);
                                const parallelResults = await Promise.allSettled(
                                    parallelTools.map(pt => executeSingleTool(pt))
                                );
                                for (const result of parallelResults) {
                                    finalToolResults += result.status === "fulfilled"
                                        ? result.value
                                        : `[SYSTEM_ALERT]: Tool execution failed: ${(result as PromiseRejectedResult).reason?.message || "Unknown error"}\n\n`;
                                }
                            } else if (parallelTools.length === 1) {
                                finalToolResults += await executeSingleTool(parallelTools[0]);
                            }

                            // Execute sequential tools in order
                            for (const pt of sequentialTools) {
                                finalToolResults += await executeSingleTool(pt);
                            }

                            let nextActionPrompt = `[DATA FROM EXECUTED TOOLS]:\n${finalToolResults}`;
                            const executedTools = parsedToolCalls.map((t) => t.name).join(", ");

                            if (!executedTools.includes("zalo") && turnCount < MAX_ITERATIONS - 1 && userText.toLowerCase().includes("zalo")) {
                                nextActionPrompt += `\n[SUGGESTION]: Consider calling \`send_zalo_bot\` to report the results to the user via Zalo.`;
                            } else {
                                nextActionPrompt += `\n[SYSTEM]: The above is factual data retrieved from tools. Use this context to respond DIRECTLY to the user in their preferred language. Be natural, helpful, and concise. Do not use generic filler phrases like "I will search" or "I just found" - deliver the answer immediately!`;
                            }
                            currentQuery = nextActionPrompt;
                        } else {
                            executionMessages.push({ role: "assistant", content: responseRawText });

                            // [v27 FIX] Thought-Only Response Recovery
                            // When LLM generates only <thought>...</thought> without visible text,
                            // re-prompt once instead of showing fallback "Xin lỗi Anh, em chưa rõ ý này ạ."
                            if (!contentText && parsedToolCalls.length === 0 && turnCount < MAX_ITERATIONS - 1) {
                                logger.warn(`[AgentLoop] ⚠️ LLM output thought-only response (no visible text). Re-prompting...`);
                                currentQuery = `[SYSTEM]: Your previous response contained only internal thinking with no visible text for the user. Please respond DIRECTLY and naturally to the user's message. Do not use thinking blocks — just speak.`;
                                if (this.onRecoveryReset) await this.onRecoveryReset();
                                continue; // Re-infer with the nudge
                            }

                            isFinished = true;
                            // [SANITIZER] Strip leaked tool_call XML, thinking blocks, Gemma control tokens, and raw system error messages
                            const sanitizedReply = (contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.")
                                .replace(/<thought>[\s\S]*?<\/thought>/g, "")   // [v23 FIX] Strip complete thought blocks
                                .replace(/<scratchpad>[\s\S]*?<\/scratchpad>/g, "") // [v23 FIX] Strip scratchpad blocks
                                .replace(/<thought>[^<]*$/g, "")               // [v23 FIX] Strip unclosed <thought> at end
                                .replace(/<scratchpad>[^<]*$/g, "")            // [v23 FIX] Strip unclosed <scratchpad> at end
                                .replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "")
                                .replace(/<\/?tool_call>/g, "")
                                .replace(/<\/?start_of_turn>/g, "")
                                .replace(/<\/?end_of_turn>/g, "")
                                .replace(/<tool_call\b/g, "")     // partial tag fragment
                                .replace(/\{"name"\s*:\s*"[^"]*"\s*,\s*"arguments"\s*:\s*\{[^}]*\}\s*\}/g, "")
                                .trim();
                            
                            if (parsedToolCalls.length === 0 && (responseRawText.includes("<tool_call>") || responseRawText.includes('{"name"'))) {
                                logger.error(`[AgentLoop] LLM attempted to call a tool but generated invalid JSON syntax. Raw: ${responseRawText}`);
                                finalReply = "Hệ thống nhận được lệnh nhưng LLM tạo sai cú pháp kỹ năng (JSON Parsing Error). Vui lòng thử lại!";
                            } else {
                                finalReply = sanitizedReply || "Xin lỗi Anh, em chưa rõ ý này ạ.";
                            }
                            logger.info(`Liva phản hồi cuối (Final Response): "${finalReply}"`);
                        }
                    }

                    // [v29] Refresh Expert Cooldown TTL after inference completes
                    // This ensures the 3-min timer restarts from the last response, not from swap time
                    if (isExpertAwake && this.#orchestrator.currentModelType === "expert") {
                        this.#orchestrator.touchExpertCooldown();
                    }

                    if (!isHeartbeat || !finalReply.includes("HEARTBEAT_OK")) {
                        await this.#memory.addMessage("user", userText);

                        // [v23] XML-Safe Memory Truncation on Barge-in
                        // Strip dangling XML tags (e.g. unclosed <tool_call>) before adding <interrupted>
                        let actualReply = finalReply;
                        if (this.#wasBargedIn && this.#currentStreamedText.trim()) {
                            let truncated = this.#currentStreamedText.trim();
                            // Remove any unclosed XML tags at the end (e.g., "<tool_call", "<thinking")
                            truncated = truncated.replace(/<[^>]*$/g, '');
                            // Remove any complete but dangling XML tags that weren't closed
                            truncated = truncated.replace(/<(tool_call|thinking|context)[^>]*>(?:(?!<\/\1>)[\s\S])*$/g, '');
                            truncated = truncated.trim();
                            const truncatedReply = (truncated || "...") + " <interrupted>";
                            logger.info(`[Barge-in] 📝 XML-Safe Memory truncated: stored ${truncatedReply.length} chars (original: ${finalReply.length})`);
                            await this.#memory.addMessage("assistant", truncatedReply);
                            actualReply = truncatedReply;
                        } else {
                            await this.#memory.addMessage("assistant", finalReply);
                            // Lưu vào Semantic Cache nếu không có Tool Calls (Chỉ cache Pure Text Response)
                            // Ngăn cache nếu local engine không sẵn sàng (degraded/fallback) hoặc phản hồi là câu lỗi/hệ thống/fallback mặc định
                            const isSystemFallback = SYSTEM_FALLBACK_SIGNATURES.some(signature => 
                                finalReply.includes(signature)
                            );
                            const isReady = this.#orchestrator.isReady();
                            if (parsedToolCalls.length === 0 && isReady && !isSystemFallback) {
                                this.#semanticCache.set(userText, finalReply);
                            } else {
                                logger.info(`[SemanticCache] Skipped caching for: "${userText}" (isReady: ${isReady}, isSystemFallback: ${isSystemFallback})`);
                            }
                        }

                        // [Memory Sync] Save turn to turn_layer_nodes (L1) and queue in ReflectionDaemon (L2)
                        const structuredMem = this.#memory.getStructuredMemoryInstance();
                        if (structuredMem) {
                            try {
                                const turnId = randomUUID();
                                await structuredMem.insertTurnNode(turnId, Date.now(), userText, actualReply);
                                
                                if (this.#memory.reflectionDaemon) {
                                    this.#memory.reflectionDaemon.queueTurn(userText, actualReply);
                                    logger.info(`[Memory Sync] Turn queued in ReflectionDaemon. (Turn ID: ${turnId})`);
                                } else {
                                    logger.warn(`[Memory Sync] ReflectionDaemon not ready, skipped background queueing.`);
                                }
                            } catch (memErr) {
                                logger.error(`[Memory Sync] Failed to sync conversation to StructuredMemory: ${memErr instanceof Error ? memErr.message : String(memErr)}`);
                            }
                        }
                    }

                    SensoryManager.getInstance().flush();

                    if (!isHeartbeat) {
                        if (this.onThinkingEnd) this.onThinkingEnd();
                    }

                    // Emergency Heartbeat Speaker: If it's a heartbeat but there's a real response, stream it OUT LOUD!
                    if (isHeartbeat && !finalReply.includes("HEARTBEAT_OK")) {
                        if (this.onStreamStart) this.onStreamStart();
                        if (this.onStreamChunk) this.onStreamChunk(finalReply);
                    }

                    if (this.onSpokenResponse) this.onSpokenResponse(finalReply);

                    if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                        const adapter = this.channelRouter?.getAdapter(ctx.channel as any);
                        if (adapter) {
                            await adapter.sendText(ctx.userId, finalReply);
                        }
                    }

                    // [LTC] Đúc kết lại lượt hội thoại để nuôi dưỡng Working Concepts chạy nền
                    // [v26] Wrap vào TaskQueue để đảm bảo không có 2 luồng embedding chạy song song
                    // Nếu user chat liên tiếp 3-4 câu, các tác vụ LTC sẽ được xử lý TUẦN TỰ
                    TaskQueue.wrapMemoryTask(
                        () => this.#ltcOrchestrator.summarizeAndStore(userText, finalReply),
                        `LTC-summarizeAndStore-${Date.now()}`,
                        TaskPriority.HIGH
                    ).catch((e: any) => {
                        logger.warn(`[AgentLoop] LTC queue task failed: ${e?.message || e}`);
                    });

                } catch (error: unknown) {
                    const errMsg = error instanceof Error ? error.message : String(error);
                    const isNetworkError = errMsg.includes("ECONNREFUSED") || errMsg.includes("fetch failed") || errMsg.includes("timeout") || errMsg.includes("AbortError") || errMsg.includes("14 UNAVAILABLE");

                    // [v27 FIX] llama.cpp empty output error — model generated only thinking tokens
                    // that got stripped, resulting in empty output. This is NOT a fatal error.
                    const isEmptyOutputError = errMsg.includes("model output must contain") || errMsg.includes("empty");
                    if (isEmptyOutputError) {
                        logger.warn(`[AgentLoop] ⚠️ LLM generated empty output (thought-only). Responding with friendly fallback.`);
                        if (this.onThinkingEnd) this.onThinkingEnd();
                        const fallback = "Xin chào! LIVA đây, em có thể giúp gì cho Anh ạ? 😊";
                        await this.#memory.addMessage("user", userText);
                        await this.#memory.addMessage("assistant", fallback);
                        if (this.onRecoveryReset) await this.onRecoveryReset();
                        if (this.onStreamStart) await this.onStreamStart();
                        if (this.onStreamChunk) await this.onStreamChunk(fallback);
                        if (this.onSpokenResponse) this.onSpokenResponse(fallback);
                        this.#sendExecutionDoneIfActive();
                        return;
                    }

                    logger.error("Lỗi kết nối Ghost Server:" + " " + errMsg);
                    if (this.onThinkingEnd) this.onThinkingEnd();

                    const isVramYielded = errMsg.includes("VRAM yielded") || errMsg.includes("embedding unavailable");

                    // [v25 FIX] VRAMGuard mid-request: GPU was yielded to user's game/app
                    if (isVramYielded) {
                        logger.warn("[AgentLoop] VRAM was yielded mid-request. Responding gracefully.");
                        if (this.onSpokenResponse) {
                            this.onSpokenResponse("Anh ơi, em vừa nhường GPU cho game của anh rồi nên tạm thời không xử lý được. Khi nào tắt game, em sẽ tự động quay lại phục vụ nhé!");
                        }
                        this.#sendExecutionDoneIfActive();
                        return;
                    }

                    if (isNetworkError) {
                        logger.error("🛑 Mất kết nối HTTP tới llama-server (AI Core). Đang tự phục hồi...");
                        this.#orchestrator.startAnomalyDetection();
                        this.#orchestrator.restartRouter(); // Tái khởi động (Rewarm)
                    }

                    const ctx = TraceContext.getStore();
                    if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                        const adapter = this.channelRouter?.getAdapter(ctx.channel as any);
                        if (isNetworkError) {
                            logger.warn(`🤖 [${ctx.channel} Suspend Queue]: Sếp chờ chút nha! Server AI đang tiến hóa (VRAM bị chiếm). Tạm lưu tin nhắn: "${userText}"`);
                            this.#pendingQueue.enqueue(ctx.channel, userText);
                            this.#startQueueDaemon(); // Đánh thức Daemon rà quét và đợi
                            this.#sendExecutionDoneIfActive();
                            return;
                        } else {
                            if (adapter) {
                                await adapter.sendText(ctx.userId, `❌ Lỗi hệ thống ${ctx.channel}: ${errMsg}`);
                            }
                        }
                    } else {
                        if (isNetworkError) {
                            const netErrStr = "Mất kết nối với AI Core. Đang tự động khôi phục VRAM...";
                            if (this.onStreamStart) await this.onStreamStart();
                            if (this.onStreamChunk) await this.onStreamChunk(netErrStr);
                            if (this.onSpokenResponse) this.onSpokenResponse(netErrStr);
                            this.#sendExecutionDoneIfActive();
                            return;
                        } else {
                            const sysErrStr = `❌ Lỗi AI: ${errMsg}`;
                            if (this.onStreamStart) await this.onStreamStart();
                            if (this.onStreamChunk) await this.onStreamChunk(sysErrStr);
                            if (this.onSpokenResponse) this.onSpokenResponse(sysErrStr);
                        }
                    }
                    this.#stateMachineActor.send({ type: 'EXECUTION_ERROR', error });
                } finally {
                    // [v27 FIX] Prevent duplicate EXECUTION_DONE XState events.
                    // Multiple early-return paths already send EXECUTION_DONE before returning.
                    // The finally block must check state before sending to avoid double-transition.
                    this.#sendExecutionDoneIfActive();
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

    /**
     * [v27 FIX] Send EXECUTION_DONE only if the state machine is NOT already idle.
     * Prevents duplicate event race conditions when early-return paths + finally
     * block both attempt to transition to idle.
     */
    #sendExecutionDoneIfActive(): void {
        const currentState = this.#stateMachineActor.getSnapshot().value;
        if (currentState !== 'idle') {
            this.#stateMachineActor.send({ type: 'EXECUTION_DONE' });
        }
    }

    /**
     * [v26 Phase 2] Context-Aware Barge-in trigger via XState
     */
    public bargeIn(type: 'BARGE_IN' | 'SPEECH_START' = 'BARGE_IN'): void {
        this.#stateMachineActor.send({ type });
    }

    /**
     * [v22 Full-Duplex Pillar 2] Context-Aware Barge-in
     * Internal implementation called by XState Actor
     */
    public _internalBargeIn(): void {
        if (this.#streamAbortController) {
            this.#streamAbortController.abort();
            this.#streamAbortController = null;
            this.#wasBargedIn = true;
            logger.warn(`[Barge-in] 🛑 LLM stream aborted. Spoken: ${this.#spokenTokenCount} tokens, ${this.#currentStreamedText.length} chars.`);

            // [Phase 3] Bắn Syscall Snapshot Save để lưu trạng thái KV Cache đang dang dở
            const snapshotId = `snapshot-bargein-${Date.now()}`;
            const filePath = `E:\\AI_Models\\snapshots\\${snapshotId}.bin`;
            
            Scheduler.getInstance().emitSyscall({
                type: "syscall_snapshot_save",
                priority: SyscallPriority.HRT,
                payload: { slotId: 0, filePath }
            }).catch(() => {});
        }
    }

    /**
     * [v26.1 Pillar 2] Speculative Context Warming & Hydration
     * Pre-fetches SemanticRouter route, top-K skills, AND builds full PromptBuilder context
     * while user is still speaking. Results are cached and consumed by handleUserInput().
     */
    public async speculativeWarm(partialText: string): Promise<void> {
        try {
            const routerResult = await this.#semanticRouter.route(partialText);
            const skills = await this.#registry.getSemanticTopK(partialText, routerResult.activeKit, 3);
            
            const toolsDef = skills.map((skill: any) => ({
                name: skill.name,
                description: skill.description,
                parameters: skill.parameters,
            }));
            
            // [v26.1] Hydrate PromptBuilder using partial text
            const { aiMessages, dynamicContextBlock } = await PromptBuilder.prepareFullAiMessages(
                partialText,
                this.#memory,
                {
                    location: this.currentSystemLocation,
                    timezone: this.currentSystemTimezone
                },
                toolsDef,
                routerResult.route
            );
            
            this.#speculativeCache = {
                route: routerResult.route,
                activeKit: routerResult.activeKit,
                skills,
                aiMessages,
                dynamicContextBlock
            };
            logger.debug(`[v26.1 Speculative] 🔮 Cache hydrated: route=${routerResult.route}, skills=${skills.length}, promptReady=true (TTFT ~ 0ms)`);
        } catch {
            // Silently ignore — speculative warming is best-effort
            this.#speculativeCache = null;
        }
    }

    public clearSpeculativeCache(): void {
        this.#speculativeCache = null;
        logger.debug("[v26.1 Speculative] 🔮 Cache cleared due to user typing cancellation");
    }

    public async shutdown() {
        const termToken = this.#authority.issueToken(AgentPhase.TERMINATING);
        this.transitionTo(AgentPhase.TERMINATING, termToken);
        
        // 🔒 [P1-1.3] Clear Zalo queue daemon timer to prevent zombie intervals
        if (this.#queueDaemonRef) {
            clearInterval(this.#queueDaemonRef);
            this.#queueDaemonRef = null;
            this.#queueDaemonActive = false;
        }

        // [v26 Phase 2] Stop XState Actor
        if (this.#stateMachineActor) {
            this.#stateMachineActor.stop();
        }

        // [v26] Dispose TaskQueue to prevent zombie memory operations after shutdown
        TaskQueue.getInstance().dispose();

        // [Phase 3] Dispose persistent queue (closes SQLite connection)
        this.#pendingQueue.dispose();

        // Cầu chì cắt nguồn System Memory GC Daemons chống rò rỉ RAM
        if (this.#memory && typeof this.#memory.dispose === "function") {
            this.#memory.dispose();
        }

        // Bổ sung dòng này để dọn dẹp các timer chạy ngầm của Giác Quan
        SensoryManager.getInstance().dispose();

        await this.#orchestrator.dispose();
        logger.info("🛑 [System] AgentLoop đã đóng hoàn toàn.");
    }
}
