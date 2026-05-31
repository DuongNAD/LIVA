import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { IVoiceEngine } from "./IVoiceEngine";
import { TTSFormatter } from "../utils/TTSFormatter";

/**
 * KokoroVoiceEngine — Zero-Python TTS using kokoro-js (ONNX)
 * Fallback Engine - Tự động yield Event Loop chống giật khựng giao diện.
 */
export class KokoroVoiceEngine extends EventEmitter implements IVoiceEngine {
  #tts: any = null;
  #isReady = false;
  #ttsFormatter: TTSFormatter = new TTSFormatter();
  #queue: string[] = [];
  #isProcessing: boolean = false;
  #isDestroyed = false;
  #MAX_QUEUE_SIZE = 50;

  // Configuration
  #MODEL_ID = "onnx-community/Kokoro-82M-v1.0-ONNX";
  #VOICE = "af_heart"; // Default voice
  #DTYPE: "q8" | "fp32" | "q4" = "q8"; // Best balance

  /** Await this to know when the TTS engine is ready */
  public readonly _initPromise: Promise<void>;

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    this._initPromise = Promise.resolve().then(() => this.#initModel());
  }

  async #initModel() {
    try {
      logger.info(`🎙️ [KokoroTTS] Initializing Kokoro-JS (${this.#MODEL_ID}, dtype=${this.#DTYPE})...`);
      
      const { KokoroTTS } = await import("kokoro-js");

      this.#tts = await KokoroTTS.from_pretrained(this.#MODEL_ID, {
        dtype: this.#DTYPE,
        device: "cpu",
      });

      this.#isReady = true;
      this.#tts.list_voices();
      logger.info(`✅ [KokoroTTS] Model loaded successfully!`);

      // Process any queued text
      this.#processQueue();
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`❌ [KokoroTTS] Init failed: ${errMsg}`);
    }
  }

  /**
   * Gọi API sinh giọng nói. Đẩy vào hàng đợi và kích hoạt processQueue.
   */
  public async speak(text: string): Promise<boolean> {
    if (this.#isDestroyed) return false;
    
    if (this.#queue.length < this.#MAX_QUEUE_SIZE) {
      this.#queue.push(text);
      this.#processQueue(); // Fire-and-forget, không block
    } else {
      logger.warn(`[KokoroTTS] ⚠️ Queue full (${this.#MAX_QUEUE_SIZE}). Dropping sentence.`);
    }
    
    return true; // Luôn trả về true với local fallback
  }

  /**
   * Vòng lặp xử lý hàng đợi bất đồng bộ.
   * QUAN TRỌNG: Phải nhường (yield) Event Loop ở cuối mỗi vòng lặp để Gateway không bị khựng.
   */
  async #processQueue() {
    if (!this.#isReady || this.#isProcessing || this.#isDestroyed) return;
    if (this.#queue.length === 0) return;

    this.#isProcessing = true;

    while (this.#queue.length > 0 && !this.#isDestroyed) {
      const text = this.#queue.shift()!;
      try {
        const audio = await this.#tts.generate(text, {
          voice: this.#VOICE,
        });

        if (this.#isDestroyed) break;

        // Convert to base64 WAV for UI playback
        const wavBuffer = audio.toWav();
        const base64 = Buffer.from(wavBuffer).toString("base64");
        this.emit("audio_base64", base64);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`[KokoroTTS] Generation failed for "${text.substring(0, 30)}...": ${errMsg}`);
      }

      // [CRITICAL] Nhường quyền cho Node.js Event Loop xử lý gRPC và WebSocket
      await new Promise(resolve => setTimeout(resolve, 0));
    }

    this.#isProcessing = false;
  }

  /**
   * [P5] Hứng token từ não AI, gom thành câu hoàn chỉnh + sanitize qua TTSFormatter.
   */
  public pushTokens(token: string) {
    if (this.#isDestroyed) return;

    const cleanToken = token.replace(/^\[(happy|sad|angry|surprised|neutral|relaxed)\]/, "");
    const sentence = this.#ttsFormatter.pushToken(cleanToken);
    if (sentence && sentence.trim().length > 0) {
      this.speak(sentence);
    }
  }

  /**
   * [P5] Flush buffer cuối stream — gửi nốt câu cuối cùng còn sót.
   */
  public flushTTS() {
    if (this.#isDestroyed) return;
    const remainder = this.#ttsFormatter.flush();
    if (remainder && remainder.trim().length > 0) {
      this.speak(remainder);
    }
  }

  public preempt() {
    logger.warn(`[KokoroTTS] 🛑 Preempt! Clearing queue.`);
    this.#ttsFormatter.reset();
    this.#queue = [];
  }

  public async destroy(): Promise<void> {
    logger.info(`[KokoroTTS] 🧹 Disposing TTS engine...`);
    this.#isDestroyed = true;
    this.#ttsFormatter.reset();
    this.#queue = [];
    this.#tts = null;
    this.#isReady = false;
    this.removeAllListeners();
  }
}
