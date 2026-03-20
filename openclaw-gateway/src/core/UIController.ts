import { EventEmitter } from 'events';
import { WebSocketServer, WebSocket } from 'ws';
import { logger } from '../utils/logger';

export class UIController extends EventEmitter {
    private wss: WebSocketServer;
    private uiClient: WebSocket | null = null;

    constructor(port: number = 8082) {
        super();
        this.wss = new WebSocketServer({ port });
        logger.info(`📡 [WebSocket] Máy chủ phát sóng đã mở tại cổng ${port}`);

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
                        // Emit văng lệnh lên CoreKernel
                        this.emit('user_input', userText);
                    }
                } catch (e) {
                    logger.error("[WebSocket] ❌ Lỗi parse JSON từ UI:", e);
                }
            });

            ws.on('close', () => {
                logger.info('❌ [WebSocket] Giao diện đã ngắt kết nối.');
                this.uiClient = null;
            });
        });
    }

    public broadcastUIEvent(event: string, payload: any = {}) {
        if (this.uiClient && this.uiClient.readyState === WebSocket.OPEN) {
            this.uiClient.send(JSON.stringify({ event, payload }));
        }
    }
}
