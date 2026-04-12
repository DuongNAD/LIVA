import { EventEmitter } from "events";
import { logger } from "../utils/logger";

export interface TTSConfig {
  endpoint: string;
  voiceId: string;
  speed: number;
}

export class VoiceEngine extends EventEmitter {
  private config: TTSConfig;
  private isSynthesizing: boolean = false;
  private tokenBuffer: string = "";

  constructor(config: Partial<TTSConfig> = {}) {
    super();
    this.config = {
      endpoint: config.endpoint || "http://localhost:5002/api/tts", // Kokoro TTS Edge endpoint
      voiceId: config.voiceId || "af_heart", // Typical Kokoro/Piper voice
      speed: config.speed || 1.0,
    };
    logger.info(`🗣️ [VoiceEngine] Engine Khởi tạo: Sẵn sàng chế độ Edge TTS bằng Kokoro/Piper.`);
  }

  /**
   * Hứng luồng Token trả về từ Não 26B, phân tách theo ngữ nghĩa và nạp vào hàng đợi tổng hợp âm.
   * Đây là kĩ thuật Chronological Generation (Chỉ gửi 1 đoạn nhỏ để đọc đi đọc lại ngay).
   */
  public pushTokens(token: string) {
    this.tokenBuffer += token;
    
    // Tách câu dựa trên dấu (ngắt nghỉ mượt)
    const sentenceEndMatch = this.tokenBuffer.match(/([^.?!,:]+[.?!,:])/);
    if (sentenceEndMatch) {
      const sentenceToRead = sentenceEndMatch[0];
      this.tokenBuffer = this.tokenBuffer.replace(sentenceToRead, "").trim();
      
      if (sentenceToRead.length > 2) {
        this.synthesizeAudio(sentenceToRead);
      }
    }
  }

  /**
   * Gọi xuống Edge TTS mô phỏng giọng nói.
   * @param text Dòng chữ cần đọc
   */
  private async synthesizeAudio(text: string) {
    this.isSynthesizing = true;
    try {
      logger.debug(`[VoiceEngine] 🎵 Đang dồn VRAM tạo âm: "${text}"`);
      // Giả lập cuộc gọi Kokoro TTS trả về Binary PCM Buffer
      // TODO: Thay thế bằng luồng gọi Kokoro/Piper thông qua REST / WebSocket
      const response = await fetch(this.config.endpoint, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          text: text,
          voice: this.config.voiceId,
          speed: this.config.speed,
          response_format: "pcm"
        })
      });

      if (response.ok) {
        const arrayBuffer = await response.arrayBuffer();
        const buffer = Buffer.from(arrayBuffer);
        // Ngựa ô truyền dẫn Zero-Latency: Bắn ra ngoài ngay
        this.emit("audio_chunk", buffer);
      } else {
        logger.error(`[VoiceEngine] 📉 Biên dịch giọng lỗi: ${response.statusText}`);
      }
    } catch (e: any) {
      logger.error(`[VoiceEngine] ❌ Không thể kết nối với Edge TTS: ${e.message}`);
    } finally {
      this.isSynthesizing = false;
    }
  }

  /**
   * Xóa bộ đệm nếu Agent bị người dùng ngắt lời (Barge-in / Zero-Latency preemption)
   */
  public preempt() {
    logger.warn(`[VoiceEngine] 🛑 Nhận lệnh Preempt! Đóng băng dây thanh quản LIVA.`);
    this.tokenBuffer = "";
    this.isSynthesizing = false;
  }
}
