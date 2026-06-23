import OpenAI from "openai";
import type { ChatCompletionMessageParam } from "openai/resources/chat/completions";
import { createHash, randomUUID } from "node:crypto";
import { logger } from "../../utils/logger";
import { safeFetch } from "../../utils/HttpClient";
import { TraceContext } from "../../utils/TraceContext";
import { SyscallPriority } from "../../kernel/SyscallInterface";
import { Scheduler } from "../../kernel/Scheduler";
import { ConfigManager } from "../config/ConfigManager";
import { ToolCallExtractor } from "../stream/ToolCallExtractor";
import type { ToolCall } from "../stream/ToolCallExtractor";
import { StreamSanitizer } from "../stream/StreamSanitizer";
import { PersistentQueue } from "../queue/PersistentQueue";
import { TaskQueue, TaskPriority } from "../TaskQueue";
import { ToolExecutionOrchestrator } from "../orchestrators/ToolExecutionOrchestrator";
import { LTCOrchestrator } from "../orchestrators/LTCOrchestrator";
import { GemmaLLMProvider } from "../../providers/llm/GemmaLLMProvider";
import { SensoryManager } from "../../memory/SensoryManager";
import type { ChannelType } from "../../channels/ChannelNormalizer";
import { AgentPhase, TaskLane, AuthorityToken } from "../../types/AgentTypes";

import {
    isAmbiguousChannel,
    resolveChannelFromReply,
    buildClarificationMessage,
    buildPreferenceKey,
    buildPreferenceValue,
    MESSAGING_TOOLS,
    type PendingChannelAction,
} from "../ChannelDisambiguationGate";

import type { AgentLoop } from "../AgentLoop";
import type { MemoryManager } from "../../MemoryManager";
import type { SkillRegistry } from "../../SkillRegistry";
import type { ILLMProvider } from "../../providers/ILLMProvider";

const SYSTEM_FALLBACK_SIGNATURES = [
    "Xin lỗi Anh, em chưa rõ ý này",
    "JSON Parsing Error",
    "Mất kết nối",
    "Lỗi hệ thống",
    "AI Core"
];

export class ToolExecutionEngine {
    public toolOrchestrator: ToolExecutionOrchestrator;
    private ltcOrchestrator: LTCOrchestrator;
    private memory: MemoryManager;
    private registry: SkillRegistry;
    
    public pendingQueue = new PersistentQueue();
    public queueDaemonActive = false;
    public queueDaemonRef: ReturnType<typeof setInterval> | null = null;

    private streamSanitizer = new StreamSanitizer();
    private toolCallExtractor = new ToolCallExtractor();

    public pendingChannelAction: PendingChannelAction | null = null;
    public activeMessagingIntent: {
        toolName: string;
        targetName?: string;
        emailUid?: number | string;
        timestamp: number;
    } | null = null;

    public activeAgentLoopRef: AgentLoop | null = null;

    constructor(memory: MemoryManager, registry: SkillRegistry, aiRouterClient: ILLMProvider) {
        this.memory = memory;
        this.registry = registry;
        
        this.toolOrchestrator = new ToolExecutionOrchestrator(registry, aiRouterClient as unknown as OpenAI);
        this.ltcOrchestrator = new LTCOrchestrator(memory, aiRouterClient as unknown as OpenAI);

        this.toolOrchestrator.onExecApprovalRequired = async (toolName, command, reason) => {
            if (this.activeAgentLoopRef?.onExecApprovalRequired) {
                return await this.activeAgentLoopRef.onExecApprovalRequired(toolName, command, reason);
            }
            logger.warn(`[Zero-Trust] Không có UI gắn kết để duyệt lệnh. Tự động từ chối lệnh nguy hiểm.`);
            return { approved: false };
        };
    }

    public startQueueDaemon(activeAgentLoop: AgentLoop) {
        if (this.queueDaemonActive) return;
        this.queueDaemonActive = true;
        
        this.queueDaemonRef = setInterval(async () => {
            const isZaloEmpty = this.pendingQueue.isEmpty("zalo");
            const isTelegramEmpty = this.pendingQueue.isEmpty("telegram");
            if (isZaloEmpty && isTelegramEmpty) {
                if (this.queueDaemonRef) clearInterval(this.queueDaemonRef);
                this.queueDaemonRef = null;
                this.queueDaemonActive = false;
                return;
            }
            try {
                const res = await safeFetch(`http://127.0.0.1:${activeAgentLoop.Orchestrator.routerPort}/`, {}, 2000);
                if (res.status) {
                    if (!isZaloEmpty) {
                        const backlog = this.pendingQueue.dequeueAll("zalo");
                        logger.info(`🟢 [Zalo Queue] 7B Router đã sống lại! Đang xả kho ${backlog.length} tin nhắn Zalo bị giam...`);
                        for (const msg of backlog) {
                            activeAgentLoop.handleUserInput(msg);
                        }
                    }
                    if (!isTelegramEmpty) {
                        const backlog = this.pendingQueue.dequeueAll("telegram");
                        logger.info(`🟢 [Telegram Queue] 7B Router đã sống lại! Đang xả kho ${backlog.length} tin nhắn Telegram bị giam...`);
                        for (const msg of backlog) {
                            activeAgentLoop.handleUserInput(msg);
                        }
                    }
                }
            } catch (e) { void e; }
        }, 15000);
    }

    public execute(
        userText: string,
        isHeartbeat: boolean,
        bypassRateLimit: boolean,
        isDryRun: boolean,
        activeAgentLoop: AgentLoop
    ): void {
        const currentTurn = activeAgentLoop.loopStateManager.activeTurnCount;
        const guard = <T extends (...args: never[]) => unknown>(cbName: string, cb?: T): T => {
            return ((...args: unknown[]) => {
                if (currentTurn !== activeAgentLoop.loopStateManager.activeTurnCount) {
                    logger.info(`[Turn Guard] 🤫 Suppressed stale ${cbName} execution for turn ${currentTurn} (active: ${activeAgentLoop.loopStateManager.activeTurnCount})`);
                    return;
                }
                return cb?.(...args as never[]);
            }) as unknown as T;
        };

        const turnOnThinkingStart = guard('onThinkingStart', activeAgentLoop.onThinkingStart);
        const turnOnThinkingEnd = guard('onThinkingEnd', activeAgentLoop.onThinkingEnd);
        const turnOnStreamStart = guard('onStreamStart', activeAgentLoop.onStreamStart);
        const turnOnStreamChunk = guard('onStreamChunk', activeAgentLoop.onStreamChunk);
        const turnOnThoughtChunk = guard('onThoughtChunk', activeAgentLoop.onThoughtChunk);
        const turnOnSpokenResponse = guard('onSpokenResponse', activeAgentLoop.onSpokenResponse);
        const turnOnRecoveryReset = guard('onRecoveryReset', activeAgentLoop.onRecoveryReset);
        const turnOnLatencyMask = guard('onLatencyMask', activeAgentLoop.onLatencyMask);

        const dispatchToken = activeAgentLoop.authority.issueToken(activeAgentLoop.currentPhase);
        activeAgentLoop.dispatch({
            id: `voice-cmd-${Date.now()}`,
            lane: TaskLane.LLM_REASONING,
            data: { text: userText },
            execute: (executionToken: AuthorityToken<AgentPhase>) => {
                return activeAgentLoop.loopStateManager.turnStorage.run(currentTurn, async () => {
                    if (!activeAgentLoop.authority.verify(executionToken, activeAgentLoop.currentPhase)) {
                        throw new Error("Invalid execution token in LLM Lane");
                    }
                    
                    if (!isHeartbeat) {
                        this.memory.consolidationCron?.touch();
                    }

                    if (!isHeartbeat) {
                        if (turnOnThinkingStart) turnOnThinkingStart();
                    }

                    activeAgentLoop.loopStateManager.spokenTokenCount = 0;
                    activeAgentLoop.loopStateManager.currentStreamedText = "";
                    activeAgentLoop.loopStateManager.wasBargedIn = false;

                    if (isDryRun) {
                        await this.memory.clearSession();
                        logger.info(`[Dry Run] Đã dọn dẹp ngữ cảnh để tránh tràn bộ nhớ.`);
                    }

                    logger.info(`Đang Load Ngữ Cảnh...`);
                    if (this.activeMessagingIntent && !isHeartbeat) {
                        const intent = this.activeMessagingIntent;
                        this.activeMessagingIntent = null;

                        const isCancel = ["thôi", "hủy", "cancel", "không gửi nữa"].some(kw => userText.toLowerCase().includes(kw));
                        if (isCancel) {
                            const cancelText = "Dạ sếp, em đã hủy lệnh gửi tin nhắn.";
                            await this.memory.addMessage("user", userText);
                            await this.memory.addMessage("assistant", cancelText);

                            if (turnOnThinkingEnd) turnOnThinkingEnd();
                            if (turnOnStreamStart) await turnOnStreamStart();
                            if (turnOnStreamChunk) await turnOnStreamChunk(cancelText);
                            if (turnOnSpokenResponse) turnOnSpokenResponse(cancelText);

                            this.sendExecutionDoneIfActive(activeAgentLoop);
                            return;
                        }

                        if (turnOnThinkingEnd) turnOnThinkingEnd();
                        
                        try {
                            let finalArgs: Record<string, unknown> = {};
                            if (intent.toolName.includes("email")) {
                                if (intent.toolName === "reply_email") {
                                    finalArgs = { originalUid: intent.emailUid, body_text: userText };
                                } else {
                                    finalArgs = { to: intent.targetName, subject: "Message from LIVA", body_text: userText };
                                }
                            } else {
                                finalArgs = { targetName: intent.targetName, message: userText };
                            }

                            const result = await this.toolOrchestrator.executeWithReflection(intent.toolName, finalArgs);
                            const reply = result.valid ? result.resultStr : `Xin lỗi sếp, em không thể gửi tin nhắn lúc này.`;

                            await this.memory.addMessage("user", userText);
                            await this.memory.addMessage("assistant", reply);

                            if (turnOnStreamStart) await turnOnStreamStart();
                            if (turnOnStreamChunk) await turnOnStreamChunk(reply);
                            if (turnOnSpokenResponse) turnOnSpokenResponse(reply);

                        } catch (e: unknown) {
                            const errMsg = e instanceof Error ? e.message : String(e);
                            logger.warn(`[MessagingIntent] Tool execution failed: ${errMsg}`);
                        } finally {
                            this.sendExecutionDoneIfActive(activeAgentLoop);
                        }
                        return;
                    }

                    if (this.pendingChannelAction && !isHeartbeat) {
                        const pending = this.pendingChannelAction;
                        const age = Date.now() - pending.timestamp;

                        if (age > 120000) {
                            logger.info(`[ChannelGate] Pending action expired (${Math.round(age / 1000)}s). Discarding.`);
                            this.pendingChannelAction = null;
                        } else {
                            const resolvedTool = resolveChannelFromReply(userText);
                            if (resolvedTool) {
                                this.pendingChannelAction = null;
                                logger.info(`[ChannelGate] ✅ Channel resolved: ${resolvedTool} for "${pending.recipientName}"`);

                                if (turnOnThinkingEnd) turnOnThinkingEnd();

                                try {
                                    const mergedArgs = { targetName: pending.recipientName, message: pending.message };
                                    const finalArgs = resolvedTool === "send_email"
                                        ? { to: pending.recipientName, subject: "Message from LIVA", body_text: pending.message }
                                        : mergedArgs;

                                    const result = await this.toolOrchestrator.executeWithReflection(resolvedTool, finalArgs);
                                    const reply = result.valid ? result.resultStr : `Xin lỗi, em không thể gửi tin nhắn lúc này.`;

                                    await this.memory.addMessage("user", userText);
                                    await this.memory.addMessage("assistant", reply);

                                    if (turnOnStreamStart) await turnOnStreamStart();
                                    if (turnOnStreamChunk) await turnOnStreamChunk(reply);
                                    if (turnOnSpokenResponse) turnOnSpokenResponse(reply);

                                    const sm = this.memory.getStructuredMemoryInstance();
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
                                    this.sendExecutionDoneIfActive(activeAgentLoop);
                                }
                                return;
                            }
                            logger.info(`[ChannelGate] User replied with non-channel text. Discarding pending action.`);
                            this.pendingChannelAction = null;
                        }
                    }

                    if (!isHeartbeat) {
                        const cacheHit = activeAgentLoop.semanticCache.get(userText);
                        if (cacheHit) {
                            const reply = cacheHit.response;
                            
                            await this.memory.addMessage("user", userText);
                            await this.memory.addMessage("assistant", reply);

                            if (turnOnThinkingEnd) turnOnThinkingEnd();
                            if (turnOnStreamStart) await turnOnStreamStart();
                            if (turnOnStreamChunk) await turnOnStreamChunk(reply);
                            if (turnOnSpokenResponse) turnOnSpokenResponse(reply);

                            this.sendExecutionDoneIfActive(activeAgentLoop);
                            return;
                        }
                    }

                    try {
                        const compiled = await activeAgentLoop.promptCompiler.compilePrompt(
                            userText,
                            isHeartbeat,
                            isDryRun,
                            this.memory,
                            this.registry,
                            activeAgentLoop.semanticRouter
                        );

                        if (compiled.cachedAction && !isDryRun) {
                            const { toolName, toolArgs } = compiled.cachedAction;
                            const args = toolArgs;
                            if (this.isMissingMessagingPayload(toolName, args)) {
                                this.activeMessagingIntent = {
                                    toolName,
                                    targetName: (args?.targetName as string) || (args?.to as string) || "",
                                    emailUid: args?.originalUid as string | number | undefined,
                                    timestamp: Date.now(),
                                };
                                const targetDisplay = this.activeMessagingIntent.targetName || "cuộc hội thoại hiện tại";
                                const platformName = toolName.includes("zalo") ? "Zalo" : toolName.includes("messenger") ? "Messenger" : "Email";
                                const responseText = `Dạ sếp, anh muốn gửi nội dung gì cho **${targetDisplay}** qua ${platformName} ạ?`;

                                await this.memory.addMessage("user", userText);
                                await this.memory.addMessage("assistant", responseText);

                                if (turnOnThinkingEnd) turnOnThinkingEnd();
                                if (turnOnStreamStart) await turnOnStreamStart();
                                if (turnOnStreamChunk) await turnOnStreamChunk(responseText);
                                if (turnOnSpokenResponse) turnOnSpokenResponse(responseText);

                                this.sendExecutionDoneIfActive(activeAgentLoop);
                                return;
                            }

                            logger.info(`⚡ [v24 L0.5] Direct tool execution: ${toolName} (bypass LLM)`);

                            if (!isHeartbeat && turnOnThinkingEnd) turnOnThinkingEnd();

                            try {
                                const result = await this.toolOrchestrator.executeWithReflection(toolName, toolArgs);
                                const finalReplyL05 = result.valid
                                    ? `${result.resultStr}`
                                    : `Xin lỗi, em không thực hiện được lệnh này lúc này.`;

                                await this.memory.addMessage("user", userText);
                                await this.memory.addMessage("assistant", finalReplyL05);

                                if (turnOnStreamStart) await turnOnStreamStart();
                                if (turnOnStreamChunk) await turnOnStreamChunk(finalReplyL05);
                                if (turnOnSpokenResponse) turnOnSpokenResponse(finalReplyL05);

                                this.sendExecutionDoneIfActive(activeAgentLoop);
                                return;
                            } catch (e: unknown) {
                                const errMsg = e instanceof Error ? e.message : String(e);
                                logger.warn(`[v24 L0.5] Cached action failed, falling through to LLM: ${errMsg}`);
                                if (turnOnRecoveryReset) await turnOnRecoveryReset();
                            }
                        }

                        const isHeavyRoute = compiled.route === 'deep_reasoning' || compiled.route === 'system_command';
                        if (isHeavyRoute && turnOnLatencyMask) {
                            turnOnLatencyMask(compiled.route);
                        }

                        const ctx = TraceContext.getStore();
                        if (isHeavyRoute && ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                            const adapter = activeAgentLoop.channelRouter?.getAdapter(ctx.channel as ChannelType);
                            if (adapter) {
                                adapter.sendText(ctx.userId, "⚡ Dạ thưa sếp, yêu cầu này cần xử lý chuyên sâu. LIVA đang tiến hành đánh giá và chạy nghiên cứu ngầm, có thể mất từ 15-30s. Sếp vui lòng đợi em một chút nhé! 🤖").catch((e: unknown) => {
                                    logger.warn(`[AgentLoop] Remote mid-flight warning failed: ${e instanceof Error ? e.message : String(e)}`);
                                });
                            }
                        }

                        let isFinished = false;
                        let turnCount = 0;
                        let finalReply = "";
                        let isExpertAwake = activeAgentLoop.Orchestrator.currentModelType === "expert";
                        if (isExpertAwake) {
                            activeAgentLoop.Orchestrator.touchExpertCooldown();
                            logger.info(`[AgentLoop] Expert model already active (Cooldown TTL window). Reusing.`);
                        }
                        const allExecutedTools: string[] = [];
                        let parsedToolCalls: ToolCall[] = [];
                        const actionHistory = new Set<string>();
                        let currentQuery = userText;

                        const nowStr = new Date().toLocaleString("vi-VN", {
                            timeZone: activeAgentLoop.promptCompiler.currentSystemTimezone || "Asia/Ho_Chi_Minh",
                        });
                        const dynamicContext = `\n\n<DYNAMIC_CONTEXT>\nSystem Time: ${nowStr}\nUser's Real-Time Location (via IP/GPS): ${activeAgentLoop.promptCompiler.currentSystemLocation}\n</DYNAMIC_CONTEXT>`;

                        const executionMessages = structuredClone(compiled.aiMessages);
                        if (executionMessages.length > 0 && executionMessages[0].role === "system") {
                            executionMessages[0].content += compiled.dynamicContextBlock;
                        } else {
                            executionMessages.unshift({
                                role: "system",
                                content: compiled.dynamicContextBlock
                            });
                        }

                        const generateText = async (
                            inferenceMsgs: ChatCompletionMessageParam[],
                            useExpert: boolean = false,
                            maxTokens: number = 2500,
                        ) => {
                            const cfgMgr = ConfigManager.getInstance();
                            let client = useExpert ? activeAgentLoop.aiExpertClient : activeAgentLoop.aiRouterClient;
                            let usingTarget = cfgMgr.aiProvider === "cloud"
                                ? (cfgMgr.env.AI_MODEL)
                                : (useExpert ? "local-ghost-expert" : "local-ghost-router");

                            if (!activeAgentLoop.Orchestrator.isReady()) {
                                logger.warn("[Circuit Breaker] Local AI Yielded/Offline. Routing to Cloud Fallback...");
                                if (!cfgMgr.env.FALLBACK_AI_BASE_URL || !cfgMgr.env.FALLBACK_AI_API_KEY) {
                                    throw new Error("Local engine offline/restarting and no cloud fallback configured");
                                }
                                client = new GemmaLLMProvider(new OpenAI({
                                    baseURL: cfgMgr.env.FALLBACK_AI_BASE_URL,
                                    apiKey: cfgMgr.env.FALLBACK_AI_API_KEY,
                                    timeout: 60000,
                                }));
                                usingTarget = cfgMgr.env.FALLBACK_AI_MODEL;
                            }

                            let tempParam = 0.3;
                            let maxTokensParam = maxTokens;
                            let topPParam = 0.9;
                            try {
                                const cfg = await ConfigManager.getInstance().getLivaConfig();
                                if (cfg?.ai?.temperature !== undefined) tempParam = cfg.ai.temperature;
                                if (cfg?.ai?.maxTokens !== undefined) maxTokensParam = cfg.ai.maxTokens;
                                if (cfg?.ai?.topP !== undefined) topPParam = cfg.ai.topP;
                            } catch {
                                // Silently fallback
                            }

                            if (turnCount > 1 && tempParam > 0.5) {
                                tempParam = 0.5;
                            }

                            const stream = (await Scheduler.getInstance().emitSyscall({
                                type: "syscall_infer",
                                priority: SyscallPriority.SRT,
                                payload: {
                                    client,
                                    usingTarget,
                                    localMsgs: inferenceMsgs,
                                    tempParam,
                                    maxTokensParam,
                                    topPParam
                                }
                            })) as AsyncIterable<unknown> & {
                                [Symbol.asyncIterator]: () => AsyncIterator<unknown, unknown, unknown>;
                            };

                            const originalIterator = stream[Symbol.asyncIterator];
                            if (originalIterator) {
                                stream[Symbol.asyncIterator] = function () {
                                    const iterator = originalIterator.call(stream);
                                    return {
                                        next: (arg?: unknown) => activeAgentLoop.loopStateManager.turnStorage.run(currentTurn, () => iterator.next(arg)),
                                        return: iterator.return ? (arg?: unknown) => activeAgentLoop.loopStateManager.turnStorage.run(currentTurn, () => iterator.return!(arg)) : undefined,
                                        throw: iterator.throw ? (arg?: unknown) => activeAgentLoop.loopStateManager.turnStorage.run(currentTurn, () => iterator.throw!(arg)) : undefined,
                                        [Symbol.asyncIterator]() { return this; }
                                    };
                                };
                            }

                            activeAgentLoop.loopStateManager.streamAbortController = new AbortController();
                            const abortSignal = activeAgentLoop.loopStateManager.streamAbortController.signal;

                            this.streamSanitizer.reset();
                            let streamChunkBuffer = "";
                            let thoughtChunkBuffer = "";

                            for await (const chunk of stream as AsyncIterable<OpenAI.Chat.ChatCompletionChunk>) {
                                if (abortSignal.aborted) {
                                    logger.info("[Barge-in] 🛑 LLM stream killed by AbortController.");
                                    break;
                                }

                                const rawToken = chunk.choices[0]?.delta?.content || "";
                                const isFinish = !!chunk.choices[0]?.finish_reason;
                                const result = this.streamSanitizer.process(rawToken, isFinish);

                                if (result.action === "emit" && !isHeartbeat) {
                                    if (!this.streamSanitizer.streamStarted) {
                                        activeAgentLoop.loopStateManager.sendActorEvent({ type: 'STREAM_START' });
                                        if (turnOnStreamStart) await turnOnStreamStart();
                                        this.streamSanitizer.markStreamStarted();
                                    }
                                    activeAgentLoop.loopStateManager.spokenTokenCount++;
                                    activeAgentLoop.loopStateManager.currentStreamedText += result.cleanToken;

                                    streamChunkBuffer += result.cleanToken;
                                    if (/[.,!?;:\n]/.test(result.cleanToken) || streamChunkBuffer.length >= 16) {
                                        if (turnOnStreamChunk) await turnOnStreamChunk(streamChunkBuffer);
                                        streamChunkBuffer = "";
                                    }
                                } else if (result.action === "emit_thought" && !isHeartbeat) {
                                    if (!this.streamSanitizer.streamStarted) {
                                        activeAgentLoop.loopStateManager.sendActorEvent({ type: 'STREAM_START' });
                                        if (turnOnStreamStart) await turnOnStreamStart();
                                        this.streamSanitizer.markStreamStarted();
                                    }

                                    thoughtChunkBuffer += result.cleanToken;
                                    if (/[.,!?;:\n]/.test(result.cleanToken) || thoughtChunkBuffer.length >= 16) {
                                        if (turnOnThoughtChunk) await turnOnThoughtChunk(thoughtChunkBuffer);
                                        thoughtChunkBuffer = "";
                                    }
                                }
                            }

                            if (streamChunkBuffer.length > 0 && !isHeartbeat) {
                                if (turnOnStreamChunk) await turnOnStreamChunk(streamChunkBuffer);
                            }
                            if (thoughtChunkBuffer.length > 0 && !isHeartbeat) {
                                if (turnOnThoughtChunk) await turnOnThoughtChunk(thoughtChunkBuffer);
                            }

                            activeAgentLoop.loopStateManager.streamAbortController = null;
                            return this.streamSanitizer.getFullContent();
                        };

                        const MAX_ITERATIONS = 5;

                        while (!isFinished && turnCount < MAX_ITERATIONS) {
                            turnCount++;

                            if (turnCount === MAX_ITERATIONS) {
                                isFinished = true;
                                finalReply = `LIVA đã thử 5 hướng tiếp cận khác nhau nhưng vẫn gặp rào cản kỹ thuật. Quá trình xử lý phức tạp vượt quá mức trần an toàn của vòng lặp.\nAnh Dương vui lòng hướng dẫn thêm cho em hoặc thử chẻ nhỏ yêu cầu này ra giúp em nhé!`;
                                logger.info("Graceful Exit: LLM chạm mốc lặp 5 lần vướng ngõ cụt.");
                                if (turnOnStreamStart) await turnOnStreamStart();
                                if (turnOnStreamChunk) await turnOnStreamChunk(finalReply);
                                break;
                            }

                            logger.info(`Đang đập cánh luồng Tư Duy bằng [$${isExpertAwake ? "Expert Model 26B" : "Router Model 4B"}] (Vòng #${turnCount})...`);

                            if (turnCount === 1) {
                                executionMessages.push({
                                    role: "user",
                                    content: userText + dynamicContext
                                });
                            } else {
                                executionMessages.push({
                                    role: "user",
                                    content: currentQuery
                                });
                            }

                            activeAgentLoop.promptCompiler.runTokenGuard(executionMessages);

                            const responseRawText = await generateText(
                                executionMessages,
                                isExpertAwake
                            );
                            logger.debug({ response: responseRawText }, `RAW AI Response (Turn ${turnCount}):`);

                            const extraction = this.toolCallExtractor.extract(responseRawText || "");
                            const contentText = extraction.cleanedContent;
                            parsedToolCalls = extraction.parsedToolCalls;

                            if (parsedToolCalls.length > 0) {
                                logger.info({ parsedToolCalls }, `AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`);

                                if (parsedToolCalls.length === 1 && MESSAGING_TOOLS.has(parsedToolCalls[0].name)) {
                                    const toolCall = parsedToolCalls[0];
                                    const toolArgs = this.toolCallExtractor.parseArguments(toolCall.name, toolCall.arguments);
                                    const recipientName = toolArgs?.targetName || toolArgs?.to || "";

                                    let channelPref: string | null = null;
                                    const sm = this.memory.getStructuredMemoryInstance();
                                    if (sm && recipientName) {
                                        const prefFact = sm.getFact(buildPreferenceKey(recipientName));
                                        channelPref = prefFact?.value || null;
                                    }

                                    if (isAmbiguousChannel(userText, toolCall.name, recipientName, channelPref)) {
                                        this.pendingChannelAction = {
                                            recipientName,
                                            message: toolArgs?.message || toolArgs?.body_text || "",
                                            originalUserText: userText,
                                            timestamp: Date.now(),
                                        };

                                        const clarification = buildClarificationMessage(recipientName);
                                        logger.info(`[ChannelGate] 🔔 Gate activated for "${recipientName}". Asking user to pick channel.`);

                                        await this.memory.addMessage("user", userText);
                                        await this.memory.addMessage("assistant", clarification);

                                        if (turnOnThinkingEnd) turnOnThinkingEnd();
                                        if (turnOnStreamStart) await turnOnStreamStart();
                                        if (turnOnStreamChunk) await turnOnStreamChunk(clarification);
                                        if (turnOnSpokenResponse) turnOnSpokenResponse(clarification);

                                        this.sendExecutionDoneIfActive(activeAgentLoop);
                                        return;
                                    }
                                }

                                let finalToolResults = "";
                                executionMessages.push({ role: "assistant", content: responseRawText });

                                const SEQUENTIAL_TOOLS = new Set([
                                    "handoff_to_expert", "write_local_file", "delete_local_file",
                                    "execute_command", "send_zalo_bot", "send_email",
                                    "update_memory", "update_session_state", "update_core_profile",
                                    "git_sync_project", "create_google_doc", "append_google_doc",
                                ]);

                                interface PreparedTool {
                                    toolCall: ToolCall;
                                    functionName: string;
                                    functionArgs: Record<string, unknown> | null;
                                    actionHash: string;
                                    isSequential: boolean;
                                    isDuplicate: boolean;
                                }

                                const preparedTools: PreparedTool[] = [];
                                for (const toolCall of parsedToolCalls) {
                                    const functionName = toolCall.name;

                                    if (functionName === "handoff_to_expert") {
                                        preparedTools.push({
                                            toolCall, functionName, functionArgs: typeof toolCall.arguments === 'string' ? null : (toolCall.arguments as Record<string, unknown>),
                                            actionHash: "", isSequential: true, isDuplicate: false,
                                        });
                                        continue;
                                    }

                                    const functionArgs = this.toolCallExtractor.parseArguments(functionName, toolCall.arguments) as Record<string, unknown> | null;

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

                                const missingPayloadTool = preparedTools.find(pt => this.isMissingMessagingPayload(pt.functionName, pt.functionArgs));
                                if (missingPayloadTool) {
                                    const pt = missingPayloadTool;
                                    this.activeMessagingIntent = {
                                        toolName: pt.functionName,
                                        targetName: (pt.functionArgs?.targetName as string) || (pt.functionArgs?.to as string) || "",
                                        emailUid: pt.functionArgs?.originalUid as string | number | undefined,
                                        timestamp: Date.now(),
                                    };
                                    const targetDisplay = this.activeMessagingIntent.targetName || "cuộc hội thoại hiện tại";
                                    const platformName = pt.functionName.includes("zalo") ? "Zalo" : pt.functionName.includes("messenger") ? "Messenger" : "Email";
                                    const responseText = `Dạ sếp, anh muốn gửi nội dung gì cho **${targetDisplay}** qua ${platformName} ạ?`;

                                    await this.memory.addMessage("user", userText);
                                    await this.memory.addMessage("assistant", responseText);

                                    if (turnOnThinkingEnd) turnOnThinkingEnd();
                                    if (turnOnStreamStart) await turnOnStreamStart();
                                    if (turnOnStreamChunk) await turnOnStreamChunk(responseText);
                                    if (turnOnSpokenResponse) turnOnSpokenResponse(responseText);

                                    this.sendExecutionDoneIfActive(activeAgentLoop);
                                    return;
                                }

                                const executeSingleTool = async (pt: PreparedTool): Promise<string> => {
                                    if (pt.functionName === "handoff_to_expert") {
                                        logger.warn(`🚀 [Handoff] Router gọi cứu viện. Hot-swapping to Expert model...`);
                                        
                                        Scheduler.getInstance().emitSyscall({
                                            type: "syscall_a2a_message",
                                            priority: SyscallPriority.HRT,
                                            payload: {
                                                sender: "Router-4B",
                                                receiver: "Expert-26B",
                                                message: `Handoff Transfer. User Query: ${userText}`
                                            }
                                        }).catch(() => {});

                                        const swapNotification = "⚡ Em đang tắt model nhẹ và nạp model Chuyên Gia 26B vào VRAM, chờ em khoảng 10-15 giây...\n";
                                        if (turnOnStreamStart) await turnOnStreamStart();
                                        if (turnOnStreamChunk) await turnOnStreamChunk(swapNotification);

                                        if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                                            const adapter = activeAgentLoop.channelRouter?.getAdapter(ctx.channel as ChannelType);
                                            if (adapter) {
                                                adapter.sendText(ctx.userId, "🔥 LIVA: Tác vụ này khá căng nên em đang đẩy Chuyên Gia 26B lên VRAM! Chờ em 10-15 giây...").catch((e: unknown) => {
                                                    logger.warn(`[Handoff] Remote handoff warning failed: ${e instanceof Error ? e.message : String(e)}`);
                                                });
                                            }
                                        }

                                        const swapSuccess = await activeAgentLoop.Orchestrator.swapToExpert();
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

                                    logger.info(pt.functionArgs, `Đang chạy hàm: ${pt.functionName}`);
                                    let executionResult: { valid: boolean; resultStr: string; rawObj: unknown };
                                    
                                    if (globalThis.kernelInstance?.ui) {
                                        globalThis.kernelInstance.ui.broadcastUIEvent("test_tool_execution", {
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
                                                toolOrchestrator: this.toolOrchestrator,
                                                functionName: pt.functionName,
                                                functionArgs: pt.functionArgs
                                            }
                                        });
                                    }
                                    logger.info({ rawObj: executionResult.rawObj }, `Kết quả chạy hàm ${pt.functionName} (Valid: ${executionResult.valid})`);

                                    if (executionResult.valid) {
                                        activeAgentLoop.semanticRouter.recordAction(userText, pt.functionName, pt.functionArgs).catch(() => {});
                                        return `[RESULTS FROM TOOL ${pt.functionName}]:\n[EXTERNAL_DATA_START]\n${executionResult.resultStr}\n[EXTERNAL_DATA_END]\n\n`;
                                    } else {
                                        logger.warn(`Tool ${pt.functionName} bị Reflection chặn hoặc báo lỗi Runtime.`);
                                        return `[SYSTEM_WARNING]: Tool execution failed: "${executionResult.resultStr}". Please analyze the failure and pivot to a different approach (e.g., try 'web_search' or 'web_browser') in your next thought, rather than apologizing to the user.\n\n`;
                                    }
                                };

                                const parallelTools = preparedTools.filter(pt => !pt.isSequential);
                                const sequentialTools = preparedTools.filter(pt => pt.isSequential);

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

                                if (!contentText && parsedToolCalls.length === 0 && turnCount < MAX_ITERATIONS - 1) {
                                    logger.warn(`[AgentLoop] ⚠️ LLM output thought-only response (no visible text). Re-prompting...`);
                                    currentQuery = `[SYSTEM]: Your previous response contained only internal thinking with no visible text for the user. Please respond DIRECTLY and naturally to the user's message. Do not use thinking blocks — just speak.`;
                                    if (turnOnRecoveryReset) await turnOnRecoveryReset();
                                    continue;
                                }

                                isFinished = true;
                                const sanitizedReply = (contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.")
                                    .replace(/<thought>[\s\S]*?<\/thought>/g, "")
                                    .replace(/<scratchpad>[\s\S]*?<\/scratchpad>/g, "")
                                    .replace(/<thought>[^<]*$/g, "")
                                    .replace(/<scratchpad>[^<]*$/g, "")
                                    .replace(/<tool_call>[\s\S]*?<\/tool_call>/g, "")
                                    .replace(/<\/?tool_call>/g, "")
                                    .replace(/<\/?start_of_turn>/g, "")
                                    .replace(/<\/?end_of_turn>/g, "")
                                    .replace(/<tool_call\b/g, "")
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

                        if (isExpertAwake && activeAgentLoop.Orchestrator.currentModelType === "expert") {
                            activeAgentLoop.Orchestrator.touchExpertCooldown();
                        }

                        if (!isHeartbeat || !finalReply.includes("HEARTBEAT_OK")) {
                            await this.memory.addMessage("user", userText);

                            let actualReply = finalReply;
                            if (activeAgentLoop.loopStateManager.wasBargedIn && activeAgentLoop.loopStateManager.currentStreamedText.trim()) {
                                let truncated = activeAgentLoop.loopStateManager.currentStreamedText.trim();
                                truncated = truncated.replace(/<[^>]*$/g, '');
                                truncated = truncated.replace(/<(tool_call|thinking|context)[^>]*>(?:(?!<\/\1>)[\s\S])*$/g, '');
                                truncated = truncated.trim();
                                const truncatedReply = (truncated || "...") + " <interrupted>";
                                logger.info(`[Barge-in] 📝 XML-Safe Memory truncated: stored ${truncatedReply.length} chars (original: ${finalReply.length})`);
                                await this.memory.addMessage("assistant", truncatedReply);
                                actualReply = truncatedReply;
                            } else {
                                await this.memory.addMessage("assistant", finalReply);
                                const isSystemFallback = SYSTEM_FALLBACK_SIGNATURES.some(signature => 
                                    finalReply.includes(signature)
                                );
                                const isReady = activeAgentLoop.Orchestrator.isReady();
                                if (parsedToolCalls.length === 0 && isReady && !isSystemFallback) {
                                    activeAgentLoop.semanticCache.set(userText, finalReply);
                                } else {
                                    logger.info(`[SemanticCache] Skipped caching for: "${userText}" (isReady: ${isReady}, isSystemFallback: ${isSystemFallback})`);
                                }
                            }

                            const structuredMem = this.memory.getStructuredMemoryInstance();
                            if (structuredMem) {
                                try {
                                    const turnId = randomUUID();
                                    await structuredMem.insertTurnNode(turnId, Date.now(), userText, actualReply);
                                    
                                    if (this.memory.reflectionDaemon) {
                                        this.memory.reflectionDaemon.queueTurn(userText, actualReply);
                                        if (typeof this.memory.markLastTurnReflected === "function") {
                                            this.memory.markLastTurnReflected();
                                        }
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
                            if (turnOnThinkingEnd) turnOnThinkingEnd();
                        }

                        if (isHeartbeat && !finalReply.includes("HEARTBEAT_OK")) {
                            if (turnOnStreamStart) turnOnStreamStart();
                            if (turnOnStreamChunk) turnOnStreamChunk(finalReply);
                        }

                        if (turnOnSpokenResponse) turnOnSpokenResponse(finalReply);

                        if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                            const adapter = activeAgentLoop.channelRouter?.getAdapter(ctx.channel as ChannelType);
                            if (adapter) {
                                await adapter.sendText(ctx.userId, finalReply);
                            }
                        }

                        TaskQueue.wrapMemoryTask(
                            () => this.ltcOrchestrator.summarizeAndStore(userText, finalReply),
                            `LTC-summarizeAndStore-${Date.now()}`,
                            TaskPriority.HIGH
                        ).catch((e: unknown) => {
                            logger.warn(`[AgentLoop] LTC queue task failed: ${e instanceof Error ? e.message : String(e)}`);
                        });

                    } catch (error: unknown) {
                        const errMsg = error instanceof Error ? error.message : String(error);
                        const isNetworkError = errMsg.includes("ECONNREFUSED") || 
                                               errMsg.includes("fetch failed") || 
                                               errMsg.includes("timeout") || 
                                               errMsg.includes("AbortError") || 
                                               errMsg.includes("14 UNAVAILABLE") ||
                                               errMsg.includes("no cloud fallback configured");

                        const isEmptyOutputError = errMsg.includes("model output must contain") || errMsg.includes("empty");
                        if (isEmptyOutputError) {
                            logger.warn(`[AgentLoop] ⚠️ LLM generated empty output (thought-only). Responding with friendly fallback.`);
                            if (turnOnThinkingEnd) turnOnThinkingEnd();
                            const fallback = "Xin chào! LIVA đây, em có thể giúp gì cho Anh ạ? 😊";
                            await this.memory.addMessage("user", userText);
                            await this.memory.addMessage("assistant", fallback);
                            if (turnOnRecoveryReset) await turnOnRecoveryReset();
                            if (turnOnStreamStart) await turnOnStreamStart();
                            if (turnOnStreamChunk) await turnOnStreamChunk(fallback);
                            if (turnOnSpokenResponse) turnOnSpokenResponse(fallback);
                            this.sendExecutionDoneIfActive(activeAgentLoop);
                            return;
                        }

                        logger.error("Lỗi kết nối Ghost Server:\n" + (error instanceof Error ? error.stack : String(error)));
                        if (turnOnThinkingEnd) turnOnThinkingEnd();

                        const isVramYielded = errMsg.includes("VRAM yielded") || errMsg.includes("embedding unavailable");

                        if (isVramYielded) {
                            logger.warn("[AgentLoop] VRAM was yielded mid-request. Responding gracefully.");
                            if (turnOnSpokenResponse) {
                                turnOnSpokenResponse("Anh ơi, em vừa nhường GPU cho game của anh rồi nên tạm thời không xử lý được. Khi nào tắt game, em sẽ tự động quay lại phục vụ nhé!");
                            }
                            this.sendExecutionDoneIfActive(activeAgentLoop);
                            return;
                        }

                        if (isNetworkError) {
                            if (process.env.VITEST) {
                                logger.warn("🛑 Mất kết nối HTTP tới llama-server (AI Core). Đang tự phục hồi...");
                            } else {
                                logger.error("🛑 Mất kết nối HTTP tới llama-server (AI Core). Đang tự phục hồi...");
                            }
                            activeAgentLoop.Orchestrator.startAnomalyDetection();
                            activeAgentLoop.Orchestrator.restartRouter();
                        }

                        const ctx = TraceContext.getStore();
                        if (ctx && ctx.channel && ctx.channel !== "ui" && ctx.userId) {
                            const adapter = activeAgentLoop.channelRouter?.getAdapter(ctx.channel as ChannelType);
                            if (isNetworkError) {
                                logger.warn(`🤖 [${ctx.channel} Suspend Queue]: Sếp chờ chút nha! Server AI đang tiến hóa (VRAM bị chiếm). Tạm lưu tin nhắn: "${userText}"`);
                                this.pendingQueue.enqueue(ctx.channel, userText);
                                this.startQueueDaemon(activeAgentLoop);
                                this.sendExecutionDoneIfActive(activeAgentLoop);
                                return;
                            } else {
                                if (adapter) {
                                    await adapter.sendText(ctx.userId, `❌ Lỗi hệ thống ${ctx.channel}: ${errMsg}`);
                                }
                            }
                        } else {
                            if (isNetworkError) {
                                const netErrStr = errMsg.includes("no cloud fallback configured")
                                    ? "Hệ thống AI cục bộ đang bận hoặc đang khởi động lại. Vui lòng đợi trong giây lát để hệ thống tự phục hồi... 😊"
                                    : "Mất kết nối với AI Core. Đang tự động khôi phục VRAM...";
                                if (turnOnStreamStart) await turnOnStreamStart();
                                if (turnOnStreamChunk) await turnOnStreamChunk(netErrStr);
                                if (turnOnSpokenResponse) turnOnSpokenResponse(netErrStr);
                                this.sendExecutionDoneIfActive(activeAgentLoop);
                                return;
                            } else {
                                const sysErrStr = `❌ Lỗi AI: ${errMsg}`;
                                if (turnOnStreamStart) await turnOnStreamStart();
                                if (turnOnStreamChunk) await turnOnStreamChunk(sysErrStr);
                                if (turnOnSpokenResponse) turnOnSpokenResponse(sysErrStr);
                            }
                        }
                        activeAgentLoop.loopStateManager.sendActorEvent({ type: 'EXECUTION_ERROR', error });
                    } finally {
                        this.sendExecutionDoneIfActive(activeAgentLoop);
                    }
                });
            }
        }, dispatchToken);
    }

    public sendExecutionDoneIfActive(activeAgentLoop: AgentLoop): void {
        const currentState = activeAgentLoop.loopStateManager.getCurrentStateValue();
        if (currentState !== 'idle') {
            activeAgentLoop.loopStateManager.sendActorEvent({ type: 'EXECUTION_DONE' });
        }
    }

    private isMissingMessagingPayload(toolName: string, args: Record<string, unknown> | undefined | null): boolean {
        const messagingTools = [
            "reply_zalo_rpa", "send_zalo_rpa", "send_zalo_bot",
            "reply_messenger_rpa", "send_messenger_rpa",
            "reply_email", "send_email"
        ];
        if (!messagingTools.includes(toolName)) {
            return false;
        }
        if (!args) return true;
        if (toolName.includes("email")) {
            const bodyText = args.body_text;
            return !bodyText || String(bodyText).trim() === "";
        }
        const msgVal = args.message;
        return !msgVal || String(msgVal).trim() === "";
    }

    public shutdown(): void {
        if (this.queueDaemonRef) {
            clearInterval(this.queueDaemonRef);
            this.queueDaemonRef = null;
            this.queueDaemonActive = false;
        }
        this.pendingQueue.dispose();
    }
}
