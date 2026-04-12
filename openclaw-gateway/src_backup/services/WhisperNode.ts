import { EventEmitter } from "events";
import { logger } from "../utils/logger";

/**
 * Một cổng ASR (Speech-to-Text) độc lập, 
 * chuyên phục vụ cho luồng giao tiếp thời gian thực (Zero-copy IPC mentality).
 */
export class WhisperNode extends EventEmitter {
  private audioBuffer: Buffer[] = [];
  private isProcessing: boolean = false;
  private silenceTimer: NodeJS.Timeout | null = null;
  
  // Ngưỡng 800ms để quyết định người dùng đã kết thúc câu (Endpointing)
  private readonly VAD_SILENCE_MS = 800; 

  constructor() {
    super();
    logger.info(`👂 [WhisperNode] Khởi tạo Hệ thống Thính giác (VAD + Whisper). Chờ tín hiệu Float32 PCM.`);
  }

  /**
   * Hứng luồng âm thanh Mic cực nhỏ bắn từ UIController
   * @param chunk Khối nhị phân PCM
   */
  public pushAudioChunk(chunk: Buffer) {
    this.audioBuffer.push(chunk);

    // Kích hoạt lại cơ chế đo đếm Khoảng lặng
    if (this.silenceTimer) {
      clearTimeout(this.silenceTimer);
    }
    this.silenceTimer = setTimeout(() => this.processAudioIntents(), this.VAD_SILENCE_MS);
  }

  /**
   * Trích xuất STT khi nhận diện khoảng lặng, theo kiến trúc VAD.
   */
  private async processAudioIntents() {
    if (this.audioBuffer.length === 0 || this.isProcessing) return;

    this.isProcessing = true;
    logger.debug(`[WhisperNode] 🎙️ Đoạt ngưỡng Silence Threshold. Đang nén VRAM để trích xuất Text...`);

    const fullBuffer = Buffer.concat(this.audioBuffer);
    this.audioBuffer = []; // Wipe the buffer for the next sentence

    try {
      // Giả lập cơ chế Đẩy Lớp (Layer-wise Offloading) khi gọi lên Whisper Local
      // TODO: Thay bằng API Local Whisper.cpp thực tế
      if (fullBuffer.length > 4096) {
         // Chế biến mảng Buffer thành dạng base64 hoặc formData để đẩy qua ASR Model
         logger.info(`[WhisperNode] ⚡ Gọi xuống Whisper.cpp Core (GGUF-IQ)...`);
         
         const textResult = await this.mockWhisperInference();
         if (textResult) {
            logger.info(`[WhisperNode] 🎯 Nhận dạng Giọng Nói: "${textResult}"`);
            this.emit("transcription_ready", textResult);
         }
      }
    } catch (e: any) {
      logger.error(`[WhisperNode] ❌ Sụp đổ luồng ASR: ${e.message}`);
    } finally {
      this.isProcessing = false;
    }
  }

  private async mockWhisperInference(): Promise<string> {
    return new Promise(resolve => setTimeout(() => resolve("Chà, LIVA giải thích xuất sắc lắm!"), 400));
  }

  /**
   * Trạng thái Barge-in khẩn cấp để đánh giá ngắt luồng
   */
  public flush() {
    this.audioBuffer = [];
    if (this.silenceTimer) clearTimeout(this.silenceTimer);
    this.isProcessing = false;
    logger.debug(`[WhisperNode] 🧹 Buffer flushed due to Preemption.`);
  }
}
