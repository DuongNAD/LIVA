import { EventEmitter } from "events";
import { logger } from "../utils/logger";
import WebSocket from "ws";

/**
 * VoiceEngine v2 - Relay âm thanh từ Python voice_engine.py (edge_tts) qua WebSocket
 * Python service chạy trên port 8002, nhận text -> trả về audio base64 MP3
 */
export class VoiceEngine extends EventEmitter {
  private ws: WebSocket | null = null;
  private reconnectTimer: NodeJS.Timeout | null = null;
  private voicePyUrl = "ws://127.0.0.1:8002/ws";
  private tokenBuffer: string = "";
  private pendingTextQueue: string[] = [];

  constructor() {
    super();
    this.connect();
    logger.info(`🗣️ [VoiceEngine] Khởi tạo: Đang kết nối tới Python Edge-TTS (port 8002)...`);
  }

  private connect() {
    try {
      this.ws = new WebSocket(this.voicePyUrl);

      this.ws.on("open", () => {
        logger.info("✅ [VoiceEngine] Đã kết nối tới Python Voice Engine (8002).");
        // Xả hàng đợi nếu có text chờ
        while (this.pendingTextQueue.length > 0) {
          const txt = this.pendingTextQueue.shift()!;
          this.sendToVoicePy(txt);
        }
      });

      this.ws.on("message", (raw: Buffer) => {
        try {
          const msg = JSON.parse(raw.toString());
          if (msg.type === "audio" && msg.data) {
            // Relay base64 audio về UI qua event
            this.emit("audio_base64", msg.data);
          }
        } catch (e) {}
      });

      this.ws.on("close", () => {
        logger.warn("⚠️ [VoiceEngine] Mất kết nối Python Engine. Tự kết nối lại sau 5s...");
        this.ws = null;
        this.reconnectTimer = setTimeout(() => this.connect(), 5000);
      });

      this.ws.on("error", (err) => {
        logger.debug(`[VoiceEngine] Lỗi WS (sẽ tự retry): ${err.message}`);
      });
    } catch (e: any) {
      logger.error(`[VoiceEngine] Không thể tạo kết nối: ${e.message}`);
    }
  }

  private sendToVoicePy(text: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "tts", text }));
    } else {
      this.pendingTextQueue.push(text);
    }
  }

  /**
   * Hứng luồng Token từ não AI, gộp thành câu rồi gửi sang Python TTS.
   */
  public pushTokens(token: string) {
    this.tokenBuffer += token;
    const m = this.tokenBuffer.match(/([^.?!\n]+[.?!\n]+)/);
    if (m) {
      const sentence = m[0].trim();
      this.tokenBuffer = this.tokenBuffer.replace(m[0], "").trim();
      if (sentence.length > 3) {
        this.sendToVoicePy(sentence);
      }
    }
  }

  /**
   * Ngắt lời / bàrge-in
   */
  public preempt() {
    logger.warn(`[VoiceEngine] 🛑 Nhận lệnh Preempt! Dừng TTS.`);
    this.tokenBuffer = "";
    if (this.ws && this.ws.readyState === WebSocket.OPEN) {
      this.ws.send(JSON.stringify({ type: "interrupt" }));
    }
  }
}
