import OpenAI from 'openai';
import { MemoryManager } from '../MemoryManager';
import { SkillRegistry } from '../SkillRegistry';
import { logger } from '../utils/logger';
import { BASE_SYSTEM_PROMPT } from '../system_prompt';
import { SensoryManager } from '../memory/SensoryManager';

export enum TaskLane {
    UI_INTERACTION = 'ui_interaction',
    LLM_REASONING = 'llm_reasoning',
    BACKGROUND_JOB = 'background_job'
}

export interface MessageTask {
    id: string;
    lane: TaskLane;
    data: any;
    execute: () => Promise<void>; 
}

export class AgentLoop {
    private aiClient: OpenAI;
    private routerModel: string;
    private expertModel: string;
    private memory: MemoryManager;
    private registry: SkillRegistry;
    
    // Callbacks to interact with UI
    public onThinkingStart?: () => void;
    public onThinkingEnd?: () => void;
    public onSpokenResponse?: (text: string) => void;

    private lanes: Map<TaskLane, MessageTask[]> = new Map();
    private activeLanes: Set<TaskLane> = new Set();
    public currentSystemLocation = "Vị trí không xác định";

    constructor(memory: MemoryManager, registry: SkillRegistry) {
        this.memory = memory;
        this.registry = registry;
        
        Object.values(TaskLane).forEach(lane => {
            this.lanes.set(lane, []);
        });

        const provider = process.env.AI_PROVIDER || 'local';
        if (provider === 'openai') {
            logger.info('🌐 [System] Chế độ Đám mây (Cloud API Mode) đã kích hoạt.');
            this.aiClient = new OpenAI({ 
                baseURL: 'https://generativelanguage.googleapis.com/v1beta/openai/',
                apiKey: process.env.OPENAI_API_KEY 
            });
            this.routerModel = process.env.CLOUD_MODEL_NAME || 'gpt-4o-mini';
            this.expertModel = process.env.CLOUD_EXPERT_MODEL || 'gpt-4o';
        } else {
            logger.info('💻 [System] Chế độ Cascade (Local Router-Expert Mode) đã kích hoạt.');
            this.aiClient = new OpenAI({ baseURL: 'http://localhost:8000/v1', apiKey: 'local-no-key' });
            this.routerModel = process.env.ROUTER_MODEL_NAME || 'Qwen2.5-7B-Instruct';
            this.expertModel = process.env.EXPERT_MODEL_NAME || 'Gemma-4-26B-A4B-it-NVFP4';
        }
    }

    public setSystemLocation(loc: string) {
        this.currentSystemLocation = loc;
    }

    public dispatch(task: MessageTask): void {
        const queue = this.lanes.get(task.lane);
        if (queue) {
            queue.push(task);
            this.processLane(task.lane);
        }
    }

    private async processLane(lane: TaskLane): Promise<void> {
        if (this.activeLanes.has(lane)) return; 
        
        this.activeLanes.add(lane);
        const queue = this.lanes.get(lane);

        while (queue && queue.length > 0) {
            const task = queue.shift();
            if (task) {
                try {
                    await task.execute();
                } catch (error) {
                    logger.error(`[AgentLoop] Lỗi tại [${task.id}]:`, error);
                }
            }
        }
        this.activeLanes.delete(lane);
    }

    public handleUserInput(userText: string) {
        this.dispatch({
            id: `voice-cmd-${Date.now()}`,
            lane: TaskLane.LLM_REASONING,
            data: { text: userText },
            execute: async () => {
                if (this.onThinkingStart) this.onThinkingStart();
                
                const userProfile = await this.memory.getUserProfile();
                if (userProfile) {
                    userProfile.current_location = this.currentSystemLocation;
                }
                
                const sensoryPrompt = SensoryManager.getInstance().injectSensoryPrompt();
                
                const profileContext = userProfile 
                    ? `\n\nTHÔNG TIN NGƯỜI DÙNG HIỆN TẠI (User Profile):\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, Khách xưng hô và Vị trí này để phục vụ người dùng)` 
                    : "";
                
                const finalContext = profileContext + sensoryPrompt;

                logger.info(`Đang suy luận (Inference) cùng ngữ cảnh...`);
                
                try {
                    const shortTermHistory = await this.memory.getHybridContext(userText, 6);
                    const messageHistory: OpenAI.Chat.ChatCompletionMessageParam[] = shortTermHistory.map(msg => ({
                        role: (msg.role === 'system' ? 'system' : msg.role) as "system" | "user" | "assistant",
                        content: msg.content
                    }));

                    const toolsDef = this.registry.getAllSkills().map(skill => ({
                        name: skill.name,
                        description: skill.description,
                        parameters: skill.parameters
                    }));
                    
                    // [Cascade] Kỹ năng Handoff (Chuyển giao)
                    toolsDef.push({
                        name: "handoff_to_expert",
                        description: "Kích hoạt AI Chuyên Gia (26B) để giải quyết nhiệm vụ phức tạp, phân tích dữ liệu hoặc dùng Tool phức tạp. HÃY dùng lệnh này nếu người dùng yêu cầu task nặng.",
                        parameters: { type: "object", properties: { reason: { type: "string", description: "Lý do cần chuyển giao" } }, required: ["reason"] }
                    });
                    
                    const nowStr = new Date().toLocaleString('vi-VN', { timeZone: 'Asia/Ho_Chi_Minh' });
                    const customToolPrompt = `# Tools\n\nYou may call one or more functions to assist with the user query.\n\nYou are provided with function signatures within <tools></tools> XML tags:\n<tools>\n${JSON.stringify(toolsDef, null, 2)}\n</tools>\n\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\n<tool_call>\n{"name": <function-name>, "arguments": <args-json-object>}\n</tool_call>\n\nHƯỚNG DẪN THÊM:\n- HÃY GỌI MỘT TOOL NGAY NẾU BẠN CẦN LÀM NHIỆM VỤ THAY VÌ LUYÊN THUYÊN.\n- NẾU NHIỆM VỤ QUÁ LỚN: Sử dụng ngay 'handoff_to_expert'.\n\nNGỮ CẢNH HỆ THỐNG:\n- Thời gian: ${nowStr}`;

                    let aiMessages: OpenAI.Chat.ChatCompletionMessageParam[] = [
                        { role: "system", content: `${BASE_SYSTEM_PROMPT}\n\n${customToolPrompt}${finalContext}` },
                        ...messageHistory,
                        { role: "user", content: userText }
                    ];

                    let isFinished = false;
                    let turnCount = 0;
                    let finalReply = "";
                    let currentComputeModel = this.routerModel;
                    let isExpertAwake = false;
                    const allExecutedTools: string[] = [];

                    while (!isFinished && turnCount < 4) {
                        turnCount++;
                        logger.info(`Đang suy luận mượt mà bằng [${currentComputeModel}] (Vòng #${turnCount})...`);

                        const response = await this.aiClient.chat.completions.create({
                            model: currentComputeModel,
                            messages: aiMessages,
                            temperature: 0.3,
                            max_tokens: 1000,
                            stop: ["<|im_end|>", "<|im_start|>user", "\nuser"]
                        });

                        logger.debug(`RAW AI Response (Turn ${turnCount}):`, response);
                        const responseMessage = response.choices[0].message;
                        let contentText = responseMessage.content || "";
                        let parsedToolCalls: any[] = [];

                        if (responseMessage.tool_calls && responseMessage.tool_calls.length > 0) {
                            for (const tc of responseMessage.tool_calls) {
                                if (tc.type === 'function' && tc.function) {
                                    parsedToolCalls.push({ name: tc.function.name, arguments: tc.function.arguments });
                                }
                            }
                        } else if (contentText.includes('<tool_call>')) {
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
                                    contentText = contentText.replace(regex, '').trim();
                                }
                            } catch (e) {
                                logger.error("Lỗi Regex Parse Multi-Tool:", e);
                            }
                        } else if (contentText.includes('{"name":') && contentText.includes('}')) {
                            try {
                                const match = contentText.match(/(\{(?:[^{}]|(?!<)\{(?:[^{}]|(?!<)\{.*?\})*?\})*\})/);
                                if (match) {
                                    const toolJson = JSON.parse(match[1].trim());
                                    if (toolJson.name) parsedToolCalls = [toolJson];
                                    contentText = contentText.replace(match[1], '').trim();
                                }
                            } catch (e) {}
                        }

                        if (parsedToolCalls.length > 0) {
                            logger.info(`AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`, parsedToolCalls);
                            let finalToolResults = "";
                            aiMessages.push({ role: "assistant", content: responseMessage.content || `Đang gọi chức năng: ${parsedToolCalls.map(t=>t.name).join(', ')}` });

                            for (const toolCall of parsedToolCalls) {
                                const functionName = toolCall.name;
                                
                                // Logic Cascade Handoff
                                if (functionName === "handoff_to_expert") {
                                    logger.warn(`🚀 [Cascade Routing] Router đã nhường quyền. Đang đánh thức chuyên gia (${this.expertModel})...`);
                                    currentComputeModel = this.expertModel;
                                    isExpertAwake = true;
                                    finalToolResults += `[Hệ thống]: Đã chuyển giao ngữ cảnh thành công cho Mô hình Chuyên Gia (Expert). Bạn hiện đang nắm quyền điều khiển. Dữ liệu gốc ở trên. Không cần báo cáo quy trình chuyển giao, hãy làm luôn tác vụ nhé.\n\n`;
                                    continue;
                                }

                                allExecutedTools.push(functionName);
                                let functionArgs = {};
                                try {
                                    let argsStr = toolCall.arguments;
                                    if (typeof argsStr === 'string') {
                                        argsStr = argsStr.replace(/\n/g, "\\n").replace(/\r/g, "\\r").replace(/\t/g, "\\t");
                                        functionArgs = JSON.parse(argsStr);
                                    } else {
                                        functionArgs = argsStr;
                                    }
                                } catch(e) {
                                    logger.error(`Lỗi Parse JSON Argument của kỹ năng ${functionName}:`, e);
                                }
                                
                                logger.info(`Đang chạy hàm: ${functionName}`, functionArgs);
                                const result = await this.registry.executeSkill(functionName, functionArgs);
                                logger.info(`Kết quả chạy hàm ${functionName}:`, result);
                                
                                let resultStr = typeof result === 'string' ? result : JSON.stringify(result);
                                
                                if (functionName === 'read_emails') {
                                    logger.warn(`[Gateway Pre-Filter] Đang kích hoạt màng lọc AI phụ để dọn dẹp Email Rác...`);
                                    try {
                                        const filterResponse = await this.aiClient.chat.completions.create({
                                            model: currentComputeModel,
                                            messages: [
                                                { role: "system", content: `Bạn đang đóng vai trò là một trợ lý ảo phân loại email. Hãy đọc danh sách Email thô bên dưới, nhẹ nhàng loại bỏ các email rác hoặc quảng cáo (như Shopee, Lazada) và chỉ giữ lại những email thực sự liên quan đến yêu cầu của anh Dương. Tóm tắt một cách ngắn gọn, súc tích nhé.` },
                                                { role: "user", content: `Yêu Cầu Gốc Của Người Dùng: "${userText}"\n\nDanh sách Email Thô Cần Lọc:\n${resultStr}` }
                                            ],
                                            temperature: 0.1, max_tokens: 1500
                                        });
                                        resultStr = `[DỮ LIỆU EMAIL ĐÃ ĐƯỢC CHẮT LỌC SẠCH SẼ BỞI HỆ THỐNG]:\n` + (filterResponse.choices[0].message.content || resultStr);
                                        logger.info("Đã lọc rác thành công!");
                                    } catch(e) {
                                        logger.error(`Lỗi màng lọc Pre-Filter`, e);
                                    }
                                } else {
                                    const CHUNK_SIZE = 3500;
                                    if (resultStr.length > CHUNK_SIZE && !functionName.includes('zalo')) {
                                        logger.warn(`Dữ liệu đầu ra quá lớn (${resultStr.length} ký tự). Kích hoạt nén dữ liệu...`);
                                        let compressedChunks = "";
                                        for (let i = 0; i < resultStr.length; i += CHUNK_SIZE) {
                                            const chunk = resultStr.slice(i, i + CHUNK_SIZE);
                                            try {
                                                const chunkSumResponse = await this.aiClient.chat.completions.create({
                                                    model: currentComputeModel,
                                                    messages: [{ role: "system", content: "Nén gọn đoạn log này lại, bỏ các chi tiết thừa." }, { role: "user", content: chunk }],
                                                    temperature: 0.1, max_tokens: 500
                                                });
                                                compressedChunks += `(Phần ${Math.floor(i/CHUNK_SIZE)+1}): ` + (chunkSumResponse.choices[0].message.content || "") + "\n";
                                            } catch(e) {}
                                        }
                                        resultStr = `[DỮ LIỆU ĐÃ NÉN TỰ ĐỘNG]:\n${compressedChunks}`;
                                    }
                                }
                                
                                finalToolResults += `[Hệ thống trả kết quả từ ${functionName}]:\n${resultStr}\n\n`;
                            }
                            
                            let nextActionPrompt = `[DỮ LIỆU TỪ CÔNG CỤ VỪA CHẠY]:\n${finalToolResults}`;
                            const executedTools = parsedToolCalls.map(t => t.name).join(', ');
                            
                            if (!executedTools.includes('zalo') && turnCount < 4 && userText.toLowerCase().includes('zalo')) {
                                nextActionPrompt += `\n[Gợi ý tự động]: Bạn vừa chạy xong công cụ [${executedTools}]. Kết quả đã có ở trên. Anh Dương có nhờ gửi qua Zalo, bạn hãy tổng hợp lại thông tin quan trọng nhất và dùng công cụ \`send_zalo_bot\` để gửi cho anh ấy nhé. Tránh việc chỉ nói "Em sẽ gửi ngay" mà quên không thực hiện thật.`;
                            } else {
                                nextActionPrompt += `\n[Hệ thống]: Dữ liệu từ công cụ đã sẵn sàng. Bạn hãy tự nhiên tổng hợp lại và trả lời anh Dương nhé.`;
                            }

                            aiMessages.push({ role: "user", content: nextActionPrompt });

                        } else {
                            const userRequestedZalo = userText.toLowerCase().includes('zalo');
                            const isZaloExecuted = allExecutedTools.includes('send_zalo_bot');
                            if (userRequestedZalo && !isZaloExecuted && turnCount < 4) {
                                logger.warn(`[Auto-Correction] LLM quên gọi send_zalo_bot. Nhắc nhở ở Turn ${turnCount+1}`);
                                aiMessages.push({ role: "assistant", content: contentText || "Em sẽ gửi Zalo ngay ạ." });
                                aiMessages.push({ role: "user", content: `[Nhắc nhở nhẹ]: Liva ơi, anh Dương muốn gửi thông tin này qua Zalo. Bạn hãy xuất ra thẻ <tool_call> để gọi \`send_zalo_bot\` và đem thông tin này qua Zalo cho anh ấy nhé.` });
                                continue; 
                            }

                            isFinished = true;
                            finalReply = contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.";
                            logger.info(`Liva phản hồi cuối (AI Final Response): "${finalReply}"`);
                        }
                    }

                    await this.memory.addMessage('user', userText);
                    await this.memory.addMessage('assistant', finalReply);

                    SensoryManager.getInstance().flush(); // Gỡ bỏ cảm giác sau 1 vòng lặp để giữ context sạch

                    if (this.onThinkingEnd) this.onThinkingEnd();
                    if (this.onSpokenResponse) this.onSpokenResponse(finalReply);

                } catch (error: any) {
                    logger.error("Lỗi kết nối API:", error.message);
                    if (this.onThinkingEnd) this.onThinkingEnd();
                }
            }
        });
    }
}
