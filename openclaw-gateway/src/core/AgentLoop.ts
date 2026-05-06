import OpenAI from "openai";
import { EventEmitter } from 'node:events';
import { NativeIPCClient } from "../utils/NativeIPCClient";
import { createHash } from "node:crypto"; // 🔒 [Memory Fix #7] Dùng SHA1 hash thay JSON.stringify cho actionHash
import { jsonrepair } from "jsonrepair";
import { SensoryManager } from "../memory/SensoryManager";
import { MemoryManager } from "../MemoryManager";
import { ZMAS_Guard } from "../security/ZMAS_Guard";
import { SkillRegistry } from "../SkillRegistry";
import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { notifyZalo } from "../utils/ZaloNotifier";
import { ModelOrchestrator } from "./ModelOrchestrator";
import { PromptBuilder } from "./PromptBuilder";
import { SemanticRouter } from "../memory/SemanticRouter";
import { AgentPhase, TaskLane, TaskState, AuthorityToken, MessageTask } from "../types/AgentTypes";
import { CoreKernelAuthority } from "./CoreKernelAuthority";
import { DualPortController } from "./orchestrators/DualPortController";
import { ToolExecutionOrchestrator } from "./orchestrators/ToolExecutionOrchestrator";
import { LTCOrchestrator } from "./orchestrators/LTCOrchestrator";
import { TaskLaneWorker } from "./orchestrators/TaskLaneWorker";

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
    #semanticRouter: SemanticRouter;

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
    #queueDaemonRef: ReturnType<typeof setInterval> | null = null;

    #startQueueDaemon() {
        if (this.#queueDaemonActive) return;
        this.#queueDaemonActive = true;
        // 🔒 [P1-1.3] Store interval ref to prevent timer leak on shutdown
        this.#queueDaemonRef = setInterval(async () => {
            if (this.#zaloPendingQueue.length === 0) {
                if (this.#queueDaemonRef) clearInterval(this.#queueDaemonRef);
                this.#queueDaemonRef = null;
                this.#queueDaemonActive = false;
                return;
            }
            try {
                // 🔒 [Audit C-4] Ping Router port via safeFetch (handles HTTP 4xx/5xx properly)
                const res = await safeFetch(`http://127.0.0.1:${this.#orchestrator.routerPort}/`, {}, 2000);
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
        this.#semanticRouter = new SemanticRouter();

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
        this.#dualPort = new DualPortController(this.#orchestrator, this.#authority);
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
            await this.#semanticRouter.initialize(); // [Dynamic Gating] Init kit anchors
            // Using the authorized token factory from ModelOrchestrator
            await this.#orchestrator.startRouter(ModelOrchestrator.getAuthorizedTokenFactory().issueToken("ROUTER_START_AUTH"));
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error("Lỗi khi mồi Router Server:" + " " + errMsg);
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
                    // [Dynamic Gating] Tiết lộ lũy tiến bằng SemanticRouter
                    const routerResult = await this.#semanticRouter.route(userText);
                    const activeKit = routerResult.activeKit;
                    
                    const filteredSkills = await this.#registry.getSemanticTopK(userText, activeKit, 3);
                    const toolsDef = filteredSkills.map((skill: any) => ({
                        name: skill.name,
                        description: skill.description,
                        parameters: skill.parameters,
                    }));

                    const aiMessages = await PromptBuilder.prepareFullAiMessages(
                        userText,
                        this.#memory,
                        this.currentSystemLocation,
                        toolsDef,
                        routerResult.route // Pass route to optimize context
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
                            } catch (e: unknown) {
                            const errMsg = e instanceof Error ? e.message : String(e);
                                logger.error("Lỗi Regex Parse Multi-Tool:" + " " + errMsg);
                            }
                        } else if (contentText.includes('{"name":') && contentText.includes("}")) {
                            // 🔒 [Audit P0-1.2] Safe JSON Fallback via indexOf + jsonrepair (AI_CONTEXT §4.6)
                            try {
                                const firstIdx = contentText.indexOf('{"name":');
                                const lastIdx = contentText.lastIndexOf("}");
                                if (firstIdx !== -1 && lastIdx > firstIdx) {
                                    const rawJson = contentText.substring(firstIdx, lastIdx + 1);
                                    const toolJson = JSON.parse(jsonrepair(rawJson));
                                    if (toolJson.name) parsedToolCalls = [toolJson];
                                    contentText = contentText.replace(rawJson, "").trim();
                                }
                            } catch (e: unknown) {
                            const errMsg = e instanceof Error ? e.message : String(e); void e; }
                        }

                        if (parsedToolCalls.length > 0) {
                            logger.info({ parsedToolCalls }, `AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`);
                            let finalToolResults = "";

                            aiMessages.push({ role: "user", content: currentQuery });
                            aiMessages.push({ role: "assistant", content: responseRawText });

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

                                let functionArgs: any = null;
                                try {
                                    let argsStr = toolCall.arguments;
                                    if (typeof argsStr === "string") {
                                        argsStr = argsStr.replaceAll("\n", "\\n").replaceAll("\r", "\\r").replaceAll("\t", "\\t");
                                        functionArgs = JSON.parse(argsStr);
                                    } else {
                                        functionArgs = argsStr;
                                    }
                                } catch (e: unknown) {
                                const errMsg = e instanceof Error ? e.message : String(e);
                                    logger.error(`Lỗi Parse JSON Argument định dạng hỏng kỹ năng ${functionName}`, errMsg);
                                }

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
                                // Handoff — special case
                                if (pt.functionName === "handoff_to_expert") {
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
                                        return `[Hệ thống]: Handoff Zero-Overhead Thành Công sang Expert Model (Cổng 8001 VRAM). Các tham số trước đó đã tự động được bê sang. Hãy phục vụ user ngay nhé.\n\n`;
                                    } else {
                                        return `[Hệ thống Lỗi]: Handoff thất bại! Có thể do VRAM bị tràn cứng. Đã chuyển lại cho Router Model xử lý cục bộ...\n\n`;
                                    }
                                }

                                if (pt.functionArgs === null) {
                                    logger.warn(`Bỏ qua Kỹ năng ${pt.functionName} do LLM trả sai cấu trúc Arguments.`);
                                    return `[Hệ thống]: Không thể chạy ${pt.functionName} vì Argument JSON bị định dạng sai. Vui lòng thử lại với khối Argument chuẩn.\n\n`;
                                }

                                if (pt.isDuplicate) {
                                    logger.warn(`🛑 Chặn LLM lặp lại hành động sai y hệt vòng trước: ${pt.functionName}`);
                                    return `[SYSTEM_ALERT]: Hệ thống từ chối thực thi! Bạn đang lặp lại chính xác hành động cũ "${pt.functionName}" với cùng một tham số đã thất bại ở lượt trước. LỆNH BẮT BUỘC: Bạn KHÔNG ĐƯỢC lặp lại tham số cũ. Hãy phân tích kỹ lỗi, điều chỉnh tham số, thử công cụ khác, hoặc gọi 'handoff_to_expert'.\n\n`;
                                }

                                if (pt.actionHash) actionHistory.add(pt.actionHash);
                                allExecutedTools.push(pt.functionName);

                                logger.info(`Đang chạy hàm: ${pt.functionName}`, pt.functionArgs);
                                const executionResult = await this.#toolOrchestrator.executeWithReflection(pt.functionName, pt.functionArgs);
                                logger.info(`Kết quả chạy hàm ${pt.functionName} (Valid: ${executionResult.valid}):`, executionResult.rawObj);

                                if (executionResult.valid) {
                                    return `[Hệ thống trả kết quả từ ${pt.functionName}]:\n[EXTERNAL_DATA_START]\n${executionResult.resultStr}\n[EXTERNAL_DATA_END]\n\n`;
                                } else {
                                    logger.warn(`Tool ${pt.functionName} bị Reflection chặn hoặc báo lỗi Runtime.`);
                                    return `[SYSTEM_ALERT]: Kỹ năng hỏng vì "${executionResult.resultStr}". LỆNH BẮT BUỘC: Kẻ chỉ trích nội bộ (Internal Critic) phát hiện Output vừa rồi là RÁC hoặc LỖI. Hãy ngưng ngay hành động lặp lại công cụ này và chuyển hướng (gọi công cụ khác, đổi tham số, hoặc handoff_to_expert).\n\n`;
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

                } catch (error: unknown) {
                const errMsg = error instanceof Error ? error.message : String(error);
                    logger.error("Lỗi kết nối Ghost Server:" + " " + errMsg);
                    if (this.onThinkingEnd) this.onThinkingEnd();

                    if (userText.includes("[Tin nhắn từ Zalo điện thoại]")) {
                        // V13: Đánh chặn Lỗi Timeout / Tắt Cổng lúc 26B Chiếm Dụng VRAM!
                        if (errMsg.includes("ECONNREFUSED") || errMsg.includes("fetch failed") || errMsg.includes("timeout")) {
                            logger.warn(`🤖 [Zalo Suspend Queue]: Sếp chờ chút nha! Server AI đang tiến hóa (VRAM bị chiếm). Tạm lưu tin nhắn: "${userText}"`);
                            this.#zaloPendingQueue.push(userText);
                            this.#startQueueDaemon(); // Đánh thức Daemmon rà quét và đợi
                            return;
                        } else {
                            await notifyZalo(`❌ Lỗi hệ thống Zalo: ${errMsg}`);
                        }
                    } else {
                        if (this.onSpokenResponse) {
                            this.onSpokenResponse(`❌ Văng Native AI: ${errMsg}`);
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
        
        // 🔒 [P1-1.3] Clear Zalo queue daemon timer to prevent zombie intervals
        if (this.#queueDaemonRef) {
            clearInterval(this.#queueDaemonRef);
            this.#queueDaemonRef = null;
            this.#queueDaemonActive = false;
        }

        // Cầu chì cắt nguồn System Memory GC Daemons chống rò rỉ RAM
        if (this.#memory && typeof this.#memory.dispose === "function") {
            this.#memory.dispose();
        }

        await this.#orchestrator.stopExpert();
        await this.#orchestrator.stopRouter();
        logger.info("🛑 [System] AgentLoop đã đóng hoàn toàn.");
    }
}
