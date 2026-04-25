import { EventEmitter } from 'node:events';
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
      // Thay vì giả lập, gởi thẳng luồng Audio vào mô hình Whisper
      // Chúng ta sẽ ép buffer Float32 PCM thành 16kHz 16-bit WAV (chuẩn bắt buộc của whisper.cpp)
      if (fullBuffer.length > 4096) {
         logger.info(`[WhisperNode] ⚡ Đang đúc khuôn WAV và gọi xuống Whisper.cpp Core (GGUF-IQ)...`);
         
         const textResult = await this.realWhisperInference(fullBuffer);
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

  /**
   * Đúc File WAV trên RAM và đút vào mồm Whisper.cpp qua IPC Http
   */
  private async realWhisperInference(pcmFloat32Buffer: Buffer): Promise<string> {
    // 1. Chuyển Buffer JS thành Float32Array
    const float32Arr = new Float32Array(
      pcmFloat32Buffer.buffer, 
      pcmFloat32Buffer.byteOffset, 
      pcmFloat32Buffer.byteLength / 4
    );

    // 2. Ép kiểu chuẩn WAV Mono 16-bit 16000Hz PCM Encoding
    const sampleRate = 16000;
    const wavBuffer = Buffer.alloc(44 + float32Arr.length * 2);
    
    // Header WAV 44 byte chấn phái
    wavBuffer.write("RIFF", 0);
    wavBuffer.writeUInt32LE(36 + float32Arr.length * 2, 4);
    wavBuffer.write("WAVE", 8);
    wavBuffer.write("fmt ", 12);
    wavBuffer.writeUInt32LE(16, 16); 
    wavBuffer.writeUInt16LE(1, 20); 
    wavBuffer.writeUInt16LE(1, 22); 
    wavBuffer.writeUInt32LE(sampleRate, 24); 
    wavBuffer.writeUInt32LE(sampleRate * 2, 28); 
    wavBuffer.writeUInt16LE(2, 32); 
    wavBuffer.writeUInt16LE(16, 34); 
    wavBuffer.write("data", 36);
    wavBuffer.writeUInt32LE(float32Arr.length * 2, 40);

    // Bơm float32 nén thành int16 (tăng âm khuếch đại nhẹ)
    let offset = 44;
    for (let i = 0; i < float32Arr.length; i++) {
        let s = Math.max(-1, Math.min(1, float32Arr[i]));
        wavBuffer.writeInt16LE(s < 0 ? s * 0x8000 : s * 0x7FFF, offset);
        offset += 2;
    }

    // 3. Giao tiếp Zero-Copy Form Data -> Liva Whisper Inference
    const blob = new Blob([wavBuffer], { type: "audio/wav" });
    const fd = new FormData();
    fd.append('file', blob, 'liva_realtime_transcribe.wav');
    fd.append('response_format', 'text');

    const whisperEndpoint = process.env.WHISPER_URL || "http://127.0.0.1:8101/v1/audio/transcriptions";

    try {
        // 🔒 [Audit Fix C-5] safeFetch with 30s timeout (Whisper inference can be slow)
        const { safeFetch } = await import("../utils/HttpClient");
        const response = await safeFetch(whisperEndpoint, {
            method: 'POST',
            body: fd
        }, 30000);

        if (!response.ok) {
            throw new Error(`Whisper API trả về mã lỗi: ${response.status} ${response.statusText}`);
        }

        const textResponse = await response.text();
        return textResponse.trim();
    } catch (err: any) {
        logger.error(`[WhisperNode] 🔌 Mất kết nối tới Whisper Engine (Port 8101): ${err.message}`);
        return "";
    }
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

  /**
   * 🔒 [Audit Fix M-7] Full cleanup — matches WhisperJSNode interface
   */
  public destroy() {
    logger.info(`[WhisperNode] 🧹 Disposing STT engine...`);
    this.flush();
    this.removeAllListeners();
  }
}
