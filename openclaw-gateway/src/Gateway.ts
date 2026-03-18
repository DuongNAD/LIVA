import { EventEmitter } from 'events';
import OpenAI from 'openai';
import * as dotenv from 'dotenv';
import { SkillRegistry, AgentSkill } from './SkillRegistry';
import { WebSocketServer, WebSocket } from 'ws';
import { MemoryManager } from './MemoryManager';
import { logger } from './utils/logger';
import { BASE_SYSTEM_PROMPT } from './system_prompt';

dotenv.config();

let currentSystemLocation = "Vị trí không xác định";

async function fetchSystemLocation() {
    try {
        logger.info('🌍 [System] Đang dò tìm vị trí hiện tại của thiết bị qua IP...');
        const ipRes = await fetch("http://ip-api.com/json/");
        const ipData = await ipRes.json();
        if (ipData && ipData.status === "success") {
            currentSystemLocation = `Thành phố ${ipData.city || ipData.regionName}, ${ipData.country} (Tọa độ: ${ipData.lat}, ${ipData.lon})`;
            logger.info(`📍 [System] Đã chốt vị trí thiết bị tại: ${currentSystemLocation}`);
        } else {
            logger.warn('⚠️ [System] Không thể lấy vị trí IP, sẽ dùng mặc định.');
        }
    } catch (e: any) {
        logger.warn(`⚠️ [System] Lỗi khi tra cứu IP định vị: ${e.message}`);
    }
}

function createAIClient() {
    const provider = process.env.AI_PROVIDER || 'local';
    if (provider === 'openai') {
        logger.info('🌐 [System] Chế độ Đám mây (Cloud API Mode) đã kích hoạt.');
        return new OpenAI({ 
            baseURL: 'https://generativelanguage.googleapis.com/v1beta/openai/',
            apiKey: process.env.OPENAI_API_KEY 
        });
    } else {
        logger.info('💻 [System] Chế độ Cục bộ (Local Engine Mode) đã kích hoạt.');
        return new OpenAI({ baseURL: 'http://localhost:8000/v1', apiKey: 'local-no-key' });
    }
}

function getActiveModelName(): string {
    return process.env.AI_PROVIDER === 'openai' 
        ? (process.env.CLOUD_MODEL_NAME || 'gpt-4o-mini') 
        : (process.env.LOCAL_MODEL_NAME || 'local-model');
}

const aiClient = createAIClient();
const activeModel = getActiveModelName();

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

export class GatewayControlPlane {
    private lanes: Map<TaskLane, MessageTask[]> = new Map();
    private activeLanes: Set<TaskLane> = new Set();
    private eventEmitter = new EventEmitter();
    
    private wss: WebSocketServer;
    private uiClient: WebSocket | null = null;
    
    // Khai báo bộ nhớ và bộ kỹ năng ở cấp độ toàn cục của Gateway
    public memory: MemoryManager;
    public registry: SkillRegistry;

    constructor(memory: MemoryManager, registry: SkillRegistry) {
        this.memory = memory;
        this.registry = registry;

        Object.values(TaskLane).forEach(lane => {
            this.lanes.set(lane, []);
        });

        this.wss = new WebSocketServer({ port: 8082 });
        logger.info('📡 [WebSocket] Máy chủ phát sóng đã mở tại cổng 8082');

        this.wss.on('connection', (ws) => {
            logger.info('🔗 [WebSocket] Giao diện Liva (UI) đã kết nối thành công!');
            this.uiClient = ws;

            ws.on('message', (message) => {
                const rawData = message.toString();
                logger.debug(`📥 RAW Message from UI:`, rawData);
                try {
                    const data = JSON.parse(rawData);
                    
                    if (data.event === 'user_voice_command') {
                        const userText = data.payload.text;
                        logger.info(`[Nhận Lệnh] Anh Dương vừa nói/gõ:`, userText);
                    
                    this.dispatch({
                        id: `voice-cmd-${Date.now()}`,
                        lane: TaskLane.LLM_REASONING,
                        data: { text: userText },
                        execute: async () => {
                            this.broadcastUIEvent('ai_thinking_start');
                            
                            // 1. TRÍCH XUẤT NGỮ CẢNH (Context Extraction)
                            const userProfile = await this.memory.getUserProfile();
                            if (userProfile) {
                                // Cập nhật vị trí tạm thời vào Profile trên RAM
                                userProfile.current_location = currentSystemLocation;
                            }
                            const profileContext = userProfile 
                                ? `\n\nTHÔNG TIN NGƯỜI DÙNG HIỆN TẠI (User Profile):\n${JSON.stringify(userProfile, null, 2)}\n(Hãy sử dụng Tên, Khách xưng hô và Vị trí này để phục vụ người dùng)` 
                                : "";

                            logger.info(`Đang suy luận (Inference) cùng ngữ cảnh...`);
                            
                            try {
                                // Lấy lịch sử ngắn hạn (Short-term memory)
                                const shortTermHistory = await this.memory.getShortTermHistory();
                                
                                // Chuyển đổi lịch sử sang định dạng OpenAI
                                const messageHistory: OpenAI.Chat.ChatCompletionMessageParam[] = shortTermHistory.map(msg => ({
                                    role: (msg.role === 'system' ? 'system' : msg.role) as "system" | "user" | "assistant",
                                    content: msg.content
                                }));

                                // 2. GỌI MÔ HÌNH VỚI NHỮNG HÀM TÙY CHỈNH
                                const toolsDef = this.registry.getAllSkills().map(skill => ({
                                    name: skill.name,
                                    description: skill.description,
                                    parameters: skill.parameters
                                }));
                                const nowStr = new Date().toLocaleString('vi-VN', { timeZone: 'Asia/Ho_Chi_Minh' });
                                const customToolPrompt = `# Tools

You may call one or more functions to assist with the user query.

You are provided with function signatures within <tools></tools> XML tags:
<tools>
${JSON.stringify(toolsDef, null, 2)}
</tools>

For each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:
<tool_call>
{"name": <function-name>, "arguments": <args-json-object>}
</tool_call>

CHÚ Ý QUAN TRỌNG VỀ ZALO: Nếu người dùng bảo "gửi qua zalo", HÃY SỬ DỤNG CÔNG CỤ \`send_zalo_bot\` (bot chạy ngầm siêu nhanh). Tóm tắt những mail quan trọng trước khi gửi.
Nếu có nhiều công việc cần làm, HÃY GỌI CÔNG CỤ ĐẦU TIÊN NGAY BÂY GIỜ. Chờ hệ thống trả về kết quả rồi mới gọi tiếp công cụ thứ hai.

NGỮ CẢNH HỆ THỐNG HIỆN TẠI (Được cung cấp ngầm bởi HĐH):
- Thời gian: ${nowStr} (UTC+7)
Bạn HÃY sử dụng các thông tin CÓ SẴN (thời gian, profile) để phục vụ người dùng mà KHÔNG CẦN HỎI LẠI họ tên, tuổi hay địa chỉ trừ phi người dùng muốn tra cứu 1 nơi khác.`;

                                let aiMessages: OpenAI.Chat.ChatCompletionMessageParam[] = [
                                    { 
                                        role: "system", 
                                        content: `${BASE_SYSTEM_PROMPT}\n\n${customToolPrompt}${profileContext}`
                                    },
                                    ...messageHistory,
                                    { 
                                        role: "user", 
                                        content: userText 
                                    }
                                ];

                                let isFinished = false;
                                let turnCount = 0;
                                let finalReply = "";
                                const allExecutedTools: string[] = [];

                                while (!isFinished && turnCount < 4) {
                                    turnCount++;
                                    logger.info(`Đang suy luận (Vòng lặp Agentic #${turnCount})...`);

                                    const response = await aiClient.chat.completions.create({
                                        model: activeModel,
                                        messages: aiMessages,
                                        temperature: 0.3,
                                        max_tokens: 1000,
                                        stop: ["<|im_end|>", "<|im_start|>user", "\nuser"]
                                    });

                                    logger.debug(`RAW AI Response (Turn ${turnCount}):`, response);

                                    const responseMessage = response.choices[0].message;
                                    let contentText = responseMessage.content || "";
                                    let parsedToolCalls: any[] = [];

                                    // Ưu tiên 1: Native Llama.cpp function bounds
                                    if (responseMessage.tool_calls && responseMessage.tool_calls.length > 0) {
                                        for (const tc of responseMessage.tool_calls) {
                                            if (tc.type === 'function' && tc.function) {
                                                parsedToolCalls.push({ name: tc.function.name, arguments: tc.function.arguments });
                                            }
                                        }
                                    }
                                    // Ưu tiên 2: Parse từ Text `<tool_call>` do Fine-tune Unsloth đổ ra
                                    else if (contentText.includes('<tool_call>')) {
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
                                                // Xóa sạch text để tránh in ra màn hình chuỗi JSON rác
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

                                    // XỬ LÝ NẾU CÓ TOOL CALL
                                    if (parsedToolCalls.length > 0) {
                                        logger.info(`AI gọi ${parsedToolCalls.length} kỹ năng trong Turn ${turnCount}:`, parsedToolCalls);
                                        let finalToolResults = "";

                                        // Append the AI's action to the history so it doesn't loop forever
                                        aiMessages.push({ role: "assistant", content: responseMessage.content || `Đang gọi chức năng: ${parsedToolCalls.map(t=>t.name).join(', ')}` });

                                        for (const toolCall of parsedToolCalls) {
                                            const functionName = toolCall.name;
                                            allExecutedTools.push(functionName);
                                            let functionArgs = {};
                                            try {
                                                let argsStr = toolCall.arguments;
                                                if (typeof argsStr === 'string') {
                                                    // Sanitize literal newlines in the stringified JSON
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
                                            
                                            // ===== KHIÊN BẢO VỆ 1: CỔNG LỌC DỮ LIỆU EMAIL (GATEWAY PRE-FILTER) =====
                                            // Ép buộc TẤT CẢ dữ liệu trả về từ read_emails phải đi qua màng lọc Zero-Shot để vứt bỏ rác (Shopee/Lazada)
                                            if (functionName === 'read_emails') {
                                                logger.warn(`[Gateway Pre-Filter] Đang kích hoạt màng lọc AI phụ để dọn dẹp Email Rác trước khi đưa vào não chính...`);
                                                try {
                                                    const filterResponse = await aiClient.chat.completions.create({
                                                        model: activeModel,
                                                        messages: [
                                                            { 
                                                                role: "system", 
                                                                content: `Bạn là BỘ CHẮT LỌC BIÊN TẬP. Nhiệm vụ: Đọc danh sách Email thô và Dấu Vết Yêu Cầu của người dùng. Hãy XÓA VĨNH VIỄN những email rác/quảng cáo (VD: Lazada, Shopee, Sale) HOẶC những mail không khớp với yêu cầu của người dùng. Chỉ giữ lại và tóm tắt siêu ngắn gọn những mail ĐÁNG GIÁ.` 
                                                            },
                                                            { 
                                                                role: "user", 
                                                                content: `Yêu Cầu Gốc Của Người Dùng: "${userText}"\n\nDanh sách Email Thô Cần Lọc:\n${resultStr}` 
                                                            }
                                                        ],
                                                        temperature: 0.1, // Low temp for strict filtering
                                                        max_tokens: 1500
                                                    });
                                                    resultStr = `[DỮ LIỆU EMAIL ĐÃ ĐƯỢC CHẮT LỌC SẠCH SẼ BỞI HỆ THỐNG]:\n` + (filterResponse.choices[0].message.content || resultStr);
                                                    logger.info("Đã lọc rác thành công!");
                                                } catch(e) {
                                                    logger.error(`Lỗi màng lọc Pre-Filter`, e);
                                                }
                                            } else {
                                                // Default Map-Reduce for other massive tool outputs
                                                const CHUNK_SIZE = 3500;
                                                if (resultStr.length > CHUNK_SIZE && !functionName.includes('zalo')) {
                                                    logger.warn(`Dữ liệu đầu ra quá lớn (${resultStr.length} ký tự). Kích hoạt cơ chế nén dữ liệu...`);
                                                    let compressedChunks = "";
                                                    for (let i = 0; i < resultStr.length; i += CHUNK_SIZE) {
                                                        const chunk = resultStr.slice(i, i + CHUNK_SIZE);
                                                        try {
                                                            const chunkSumResponse = await aiClient.chat.completions.create({
                                                                model: activeModel,
                                                                messages: [
                                                                    { role: "system", content: "Nén gọn đoạn log này lại, bỏ các chi tiết thừa." },
                                                                    { role: "user", content: chunk }
                                                                ],
                                                                temperature: 0.1,max_tokens: 500
                                                            });
                                                            compressedChunks += `(Phần ${Math.floor(i/CHUNK_SIZE)+1}): ` + (chunkSumResponse.choices[0].message.content || "") + "\n";
                                                        } catch(e) {}
                                                    }
                                                    resultStr = `[DỮ LIỆU ĐÃ NÉN TỰ ĐỘNG]:\n${compressedChunks}`;
                                                }
                                            }
                                            // =========================================================================
                                            
                                            finalToolResults += `[Hệ thống trả kết quả từ ${functionName}]:\n${resultStr}\n\n`;
                                        }
                                        
                                        // Feed the Tool Results back into the AI to let it decide the next step
                                        let nextActionPrompt = `[DỮ LIỆU TỪ CÔNG CỤ VỪA CHẠY]:\n${finalToolResults}`;
                                        const executedTools = parsedToolCalls.map(t => t.name).join(', ');
                                        
                                        if (!executedTools.includes('zalo') && turnCount < 4) {
                                            nextActionPrompt += `\n[HỆ THỐNG CẢNH BÁO BẮT BUỘC]: Bạn vừa chạy xong công cụ [${executedTools}].\n1. KẾT QUẢ CỦA CÔNG CỤ Ở TRÊN.\n2. YÊU CẦU GỐC LÀ: "${userText}".\n3. NẾU yêu cầu gốc cần "gửi Zalo", BẠN PHẢI suy nghĩ và CHỈ TÓM TẮT những thông tin khớp với yêu cầu (VD: nếu nhắc đến 'quan trọng', hãy bỏ qua quảng cáo, rác). Sau đó tổng hợp chúng và gọi tool \`send_zalo_bot\` để gửi Zalo ngay lập tức!\n4. NGHIÊM CẤM TRẢ LỜI MỒM NHƯ "Dạ em sẽ gửi ngay".`;
                                        } else {
                                            nextActionPrompt += `\n[HỆ THỐNG]: Các công cụ đã được thực thi xong. Hãy tổng hợp lại kết quả và phản hồi người dùng một cách thân thiện, ngắn gọn.`;
                                        }

                                        aiMessages.push({
                                            role: "user",
                                            content: nextActionPrompt
                                        });

                                    } else {
                                        // KHÔNG GỌI TOOL NỮA -> ĐÂY LÀ KẾT QUẢ CUỐI CÙNG

                                        // AUTO-CORRECTION: Ép LLM gọi Zalo nếu user yêu cầu mà LLM quên
                                        const userRequestedZalo = userText.toLowerCase().includes('zalo');
                                        const isZaloExecuted = allExecutedTools.includes('send_zalo_bot');
                                        if (userRequestedZalo && !isZaloExecuted && turnCount < 4) {
                                            logger.warn(`[Auto-Correction] LLM quên gọi send_zalo_bot. Ép buộc LLM gọi lại ở Turn ${turnCount+1}`);
                                            
                                            // Lưu lại câu trả lời văn bản của AI vào lịch sử tạm thời
                                            aiMessages.push({ role: "assistant", content: contentText || "Em sẽ gửi Zalo ngay ạ." });
                                            
                                            // Nhồi thêm System Prompt ép mạnh LLM phải gọi <tool_call>
                                            aiMessages.push({ 
                                                role: "user", 
                                                content: `[HỆ THỐNG CẢNH BÁO CAO NHẤT]: Yêu cầu gốc là "${userText}". Bạn VỪA QUÊN CHƯA GỌI CÔNG CỤ ZALO. Hãy dựa vào nội dung kết quả, CHẮT LỌC dữ liệu khớp với yêu cầu, và LẬP TỨC XUẤT RA MÃ <tool_call> ĐÚNG CHUẨN JSON CHO \`send_zalo_bot\`. NGHIÊM CẤM TRẢ LỜI MỒM LẦN NỮA!` 
                                            });
                                            continue; // Bắt AI chạy tiếp vòng lặp
                                        }

                                        isFinished = true;
                                        finalReply = contentText || "Xin lỗi Anh, em chưa rõ ý này ạ.";
                                        logger.info(`Liva phản hồi cuối (AI Final Response): "${finalReply}"`);
                                    }
                                }

                                // KẾT THÚC CHUỖI SUY LUẬN
                                await this.memory.addMessage('user', userText);
                                await this.memory.addMessage('assistant', finalReply);

                                this.broadcastUIEvent('ai_thinking_end');
                                this.broadcastUIEvent('ai_spoken_response', { text: finalReply });

                            } catch (error: any) {
                                logger.error("Lỗi kết nối API:", error.message);
                                this.broadcastUIEvent('ai_thinking_end');
                            }
                        }
                    });
                }
                } catch (e) {
                    console.error("[WebSocket] ❌ Lỗi parse JSON từ UI:", e);
                }
            });

            ws.on('close', () => {
                console.log('❌ [WebSocket] Giao diện đã ngắt kết nối.');
                this.uiClient = null;
            });
        });
    }

    public broadcastUIEvent(event: string, payload: any = {}) {
        if (this.uiClient && this.uiClient.readyState === WebSocket.OPEN) {
            this.uiClient.send(JSON.stringify({ event, payload }));
        }
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
                    console.error(`[Gateway] Lỗi tại [${task.id}]:`, error);
                }
            }
        }
        this.activeLanes.delete(lane);
    }
}

// ==========================================
// KHỞI CHẠY HỆ THỐNG VÀ ĐĂNG KÝ KỸ NĂNG
// ==========================================
async function bootstrap() {
    // 0. Nạp Ngữ cảnh Hệ Thống (Múi giờ / Định Vị)
    await fetchSystemLocation();

    // 1. Khởi tạo Trí nhớ (Data Persistence)
    const memory = new MemoryManager('liva_core');
    await memory.initialize();

    // 2. Khởi tạo Bảng điều khiển Kỹ năng (Skill Registry)
    const registry = new SkillRegistry();
    await registry.registerLocalSkills();
    
    // Kỹ năng A: Cập nhật hồ sơ (Update Profile)
    const updateProfileSkill: AgentSkill = {
        name: "update_core_profile",
        description: "Cập nhật hồ sơ tĩnh của người dùng khi có yêu cầu thay đổi (ví dụ: tuổi, nghề nghiệp, quê quán).",
        parameters: {
            type: "object",
            properties: {
                age: { type: "number", description: "Tuổi mới của người dùng" },
                profession: { type: "string", description: "Nghề nghiệp mới của người dùng" },
                location: { type: "string", description: "Quê quán / Nơi ở mới" }
            },
            required: []
        },
        execute: async (args: any) => {
            // Lưu dữ liệu mới thẳng vào tệp JSON
            await memory.updateUserProfile(args);
            return "Đã cập nhật thành công (Successfully updated)";
        }
    };
    registry.registerSkill(updateProfileSkill);

    // 3. Khởi động Lõi Gateway
    const gateway = new GatewayControlPlane(memory, registry);

    console.log('✅ Lõi hệ thống (Core System) đã khởi động toàn diện. Chờ Liva kết nối...');
}

bootstrap();