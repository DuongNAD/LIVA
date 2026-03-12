import { EventEmitter } from 'events';
import OpenAI from 'openai';
import * as dotenv from 'dotenv';
import { SkillRegistry, AgentSkill } from './SkillRegistry';
// Nhập thư viện WebSocket mới cài
import { WebSocketServer, WebSocket } from 'ws'; 

dotenv.config();

function createAIClient() {
    const provider = process.env.AI_PROVIDER || 'local';
    if (provider === 'openai') {
        console.log('🌐 [System] Chế độ Đám mây (Cloud API Mode) đã kích hoạt.');
        return new OpenAI({ apiKey: process.env.OPENAI_API_KEY });
    } else {
        console.log('💻 [System] Chế độ Cục bộ (Local Engine Mode) đã kích hoạt.');
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
    
    // Khai báo máy chủ phát sóng (Broadcaster)
    private wss: WebSocketServer;
    private uiClient: WebSocket | null = null;

    constructor() {
        Object.values(TaskLane).forEach(lane => {
            this.lanes.set(lane, []);
        });

        // 1. Khởi tạo máy chủ WebSocket ở cổng 8080
        this.wss = new WebSocketServer({ port: 8080 });
        console.log('📡 [WebSocket] Máy chủ phát sóng đã mở tại cổng 8080');

        // 2. Lắng nghe Giao diện (UI) kết nối vào
        this.wss.on('connection', (ws: WebSocket) => {
            console.log('🔗 [WebSocket] Giao diện Liva (UI) đã kết nối thành công!');
            this.uiClient = ws;

            // XỬ LÝ DỮ LIỆU NHẬN ĐƯỢC TỪ GIAO DIỆN (Incoming Message Handler)
            ws.on('message', (message) => {
                const data = JSON.parse(message.toString());
                
                if (data.event === 'user_voice_command') {
                    const userText = data.payload.text;
                    console.log(`\n[Nhận Lệnh] Anh Dương vừa nói: "${userText}"`);
                    
                    // Đẩy câu nói vào hàng đợi để AI xử lý (Dispatch to Queue)
                    this.dispatch({
                        id: `voice-cmd-${Date.now()}`,
                        lane: TaskLane.LLM_REASONING,
                        data: { text: userText },
                        execute: async () => {
                            // 1. Bật hiệu ứng "Đang suy nghĩ"
                            this.broadcastUIEvent('ai_thinking_start');
                            
                            console.log(`-> Đang phân tích (Analyzing) ngữ nghĩa...`);
                            
                            let replyText = "";
                            try {
                                // GỌI MÔ HÌNH AI THỰC SỰ
                                const response = await aiClient.chat.completions.create({
                                    model: activeModel,
                                    messages: [
                                        { role: 'system', content: 'Bạn là trợ lý AI thông minh tên là Liva. Trả lời ngắn gọn, thân thiện bằng tiếng Việt.' },
                                        { role: 'user', content: userText }
                                    ],
                                });
                                replyText = response.choices[0]?.message?.content || "Xin lỗi, em không có câu trả lời vào lúc này.";
                            } catch (error) {
                                console.error("[Gateway/LLM] Lỗi gọi AI:", error);
                                replyText = "Dạ, em đang gặp chút sự cố kết nối, anh đợi em lát nhé.";
                            }
                            
                            // 2. Tắt hiệu ứng suy nghĩ
                            this.broadcastUIEvent('ai_thinking_end');
                            
                            // 3. TẠO CÂU TRẢ LỜI VÀ GỬI VỀ CHO GIAO DIỆN (Response Generation)
                            console.log(`-> Phản hồi: "${replyText}"`);
                            
                            this.broadcastUIEvent('ai_spoken_response', { text: replyText });
                        }
                    });
                }
            });

            ws.on('close', () => {
                console.log('❌ [WebSocket] Giao diện đã ngắt kết nối.');
                this.uiClient = null;
            });
        });
    }

    // Hàm bắn tín hiệu cho giao diện 3D
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

async function bootstrap() {
    const gateway = new GatewayControlPlane();
    const registry = new SkillRegistry();
    
    const getTimeSkill: AgentSkill = {
        name: "get_system_time",
        description: "Lấy thời gian hiện tại",
        parameters: { type: "object", properties: {}, required: [] },
        execute: async () => new Date().toLocaleString('vi-VN')
    };
    registry.registerSkill(getTimeSkill);

    // Kịch bản (Scenario): Cứ 10 giây, AI sẽ "suy nghĩ" một lần
    setInterval(() => {
        gateway.dispatch({
            id: `llm-thinking-${Date.now()}`,
            lane: TaskLane.LLM_REASONING,
            data: {},
            execute: async () => {
                console.log(`\n-> Đang mô phỏng tiến trình suy nghĩ của AI...`);
                
                // GỬI TÍN HIỆU: BẮT ĐẦU NGHĨ
                gateway.broadcastUIEvent('ai_thinking_start');
                
                // AI mất 3 giây để làm việc (Mock latency)
                await new Promise(resolve => setTimeout(resolve, 3000));
                
                // GỬI TÍN HIỆU: ĐÃ NGHĨ XONG
                gateway.broadcastUIEvent('ai_thinking_end');
                console.log(`-> Đã hoàn tất suy nghĩ!`);
            }
        });
    }, 10000);
}

bootstrap();