import express from 'express';
import { createServer } from 'http';
import { WebSocketServer, WebSocket } from 'ws';
import { logger } from '../utils/logger';

export class VoiceRelayServer {
    private PYTHON_ENGINE_URL = 'ws://127.0.0.1:8002/ws';

    public start(port: number = 3000) {
        const app = express();
        const server = createServer(app);
        const wssUI = new WebSocketServer({ server });

        wssUI.on('connection', (uiSocket: WebSocket) => {
            logger.info('📱 [Voice Relay] Vue UI đã kết nối tới Cổng 3000');
            
            // Xây dựng cầu nối proxy tới Voice Engine
            const engineSocket = new WebSocket(this.PYTHON_ENGINE_URL);

            engineSocket.on('open', () => {
                logger.info('🧠 [Voice Relay] Bridge kết nối thành công với Liva Voice Engine (8002)');
            });

            // Luồng dữ liệu về: Voice Engine 8002 -> Vue UI 3000
            engineSocket.on('message', (data: WebSocket.RawData) => {
                if (uiSocket.readyState === WebSocket.OPEN) {
                    uiSocket.send(data.toString());
                }
            });

            // Luồng dữ liệu đi: Vue UI 3000 -> Voice Engine 8002
            uiSocket.on('message', (data: WebSocket.RawData) => {
                if (engineSocket.readyState === WebSocket.OPEN) {
                    engineSocket.send(data.toString());
                }
            });

            // Dọn dẹp connection khi client thoát
            uiSocket.on('close', () => {
                logger.info('📱 [Voice Relay] Vue UI đã ngắt kết nối');
                engineSocket.close();
            });
            
            engineSocket.on('close', () => uiSocket.close());
            engineSocket.on('error', (err) => logger.error(`[Voice Relay] Lỗi gọi Engine WS: ${err.message}`));
        });

        server.listen(port, () => {
            logger.info(`🚀 [Voice Relay] Listening at ws://localhost:${port}`);
        });
    }
}
