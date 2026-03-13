import { EventEmitter } from 'events';
import OpenAI from 'openai';
import * as dotenv from 'dotenv';
import { SkillRegistry, AgentSkill } from './SkillRegistry';
import { WebSocketServer, WebSocket } from 'ws';
import { MemoryManager } from './MemoryManager';
import { logger } from './utils/logger';

dotenv.config();

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
                            const profileContext = userProfile 
                                ? `Thông tin người dùng hiện tại: ${JSON.stringify(userProfile)}. Hãy sử dụng thông tin này để xưng hô và cá nhân hóa câu trả lời.` 
                                : "";

                            logger.info(`Đang suy luận (Inference) cùng ngữ cảnh...`);
                            
                            try {
                                // 2. GỌI MÔ HÌNH VỚI CÔNG CỤ (LLM Call with Tools)
                                const response = await aiClient.chat.completions.create({
                                    model: activeModel,
                                    messages: [
                                        { 
                                            role: "system", 
                                            content: `Bạn là Liva, một nàng thơ AI thông minh, tinh tế và duyên dáng. Bạn CHỈ ĐƯỢC PHÉP trả lời bằng tiếng Việt, tuyệt đối không sử dụng ngôn ngữ khác. Hãy trả lời ngắn gọn, tự nhiên. \n${profileContext}` 
                                        },
                                        { role: "user", content: userText }
                                    ],
                                    tools: this.registry.getAllSkills().map(skill => ({
                                        type: "function" as const,
                                        function: {
                                            name: skill.name,
                                            description: skill.description,
                                            parameters: skill.parameters
                                        }
                                    })),
                                    tool_choice: "auto", // Để Liva tự quyết định có nên dùng kỹ năng cập nhật hồ sơ không
                                    temperature: 0.3,
                                    max_tokens: 150
                                });

                                logger.debug("RAW AI Response (Các bước suy luận của AI):", response);

                                const responseMessage = response.choices[0].message;

                                // 3. XỬ LÝ NẾU LIVA QUYẾT ĐỊNH DÙNG KỸ NĂNG (Tool Execution)
                                if (responseMessage.tool_calls) {
                                    logger.info('AI yêu cầu kích hoạt kỹ năng (Tool requested)!', responseMessage.tool_calls);
                                    
                                    for (const toolCall of responseMessage.tool_calls) {
                                        if (toolCall.type !== 'function') continue;
                                        const functionName = toolCall.function.name;
                                        const functionArgs = JSON.parse(toolCall.function.arguments);
                                        
                                        logger.info(`Đang chạy hàm: ${functionName}`, functionArgs);
                                        const result = await this.registry.executeSkill(functionName, functionArgs);
                                        logger.info(`Kết quả chạy hàm ${functionName}:`, result);
                                    }
                                    
                                    // Báo cáo lại cho giao diện
                                    this.broadcastUIEvent('ai_thinking_end');
                                    this.broadcastUIEvent('ai_spoken_response', { 
                                        text: "Dạ, em đã thực thi mệnh lệnh công cụ xong rồi ạ!" 
                                    });
                                } 
                                // 4. XỬ LÝ NẾU LIVA CHỈ TRẢ LỜI BÌNH THƯỜNG
                                else {
                                    const replyText = responseMessage.content || "Xin lỗi Anh, em chưa rõ ý này ạ.";
                                    logger.info(`Liva phản hồi (AI Response): "${replyText}"`);
                                    
                                    this.broadcastUIEvent('ai_thinking_end');
                                    this.broadcastUIEvent('ai_spoken_response', { text: replyText });
                                }

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

    // Kỹ năng B: Lấy thời gian (System Time)
    const getTimeSkill: AgentSkill = {
        name: "get_system_time",
        description: "Lấy thời gian hiện tại",
        parameters: { type: "object", properties: {}, required: [] },
        execute: async () => new Date().toLocaleString('vi-VN')
    };
    registry.registerSkill(getTimeSkill);

    // 3. Khởi động Lõi Gateway
    const gateway = new GatewayControlPlane(memory, registry);

    console.log('✅ Lõi hệ thống (Core System) đã khởi động toàn diện. Chờ Liva kết nối...');
}

bootstrap();