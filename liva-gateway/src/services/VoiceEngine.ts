import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { IVoiceEngine } from "./IVoiceEngine";
import { TTSFormatter } from "../utils/TTSFormatter";
import { EdgeTTSClient } from "./EdgeTTSClient";
import * as path from "node:path";
import * as fs from "node:fs/promises"; // [Audit H-9] Use async-only import

/**
 * VoiceEngine v4 — Direct Node.js Edge-TTS (No Python Process)
 * =============================================================
 * [Optimization C1] Replaced Python voice_engine.py WS relay with
 * direct EdgeTTSClient. Saves ~60MB RAM + eliminates IPC overhead.
 *
 * Architecture (v4):
 *   LLM tokens → TTSFormatter (clause chunking) → EdgeTTSClient.synthesize()
 *   → base64 audio → emit("audio_base64") → UI WebSocket
 *
 * Previous Architecture (v3):
 *   LLM tokens → TTSFormatter → WS → Python (edge_tts) → WS → Gateway → UI
 *
 * [P5] TTSFormatter: Gom token thành câu hoàn chỉnh + sanitize trước khi phát âm.
 */
export class VoiceEngine extends EventEmitter implements IVoiceEngine {
  #edgeTTS: EdgeTTSClient = new EdgeTTSClient();
  #ttsFormatter: TTSFormatter = new TTSFormatter();
  #ttsQueue: string[] = [];
  #isProcessing: boolean = false;
  #isDestroyed: boolean = false;
  // 🔒 [Memory Fix #1] Giới hạn hàng đợi để tránh phình RAM
  private readonly MAX_QUEUE_SIZE = 50;

  constructor() {
    super();
    logger.info(`🗣️ [VoiceEngine v4] Khởi tạo: Edge-TTS trực tiếp (không cần Python process).`);
    // Sync voice profile from config on startup
    this.#syncVoiceProfileFromConfig();
  }

  /**
   * Chuyển đổi voice profile trên Edge-TTS
   * @param voiceId - Edge-TTS voice ID (e.g. "vi-VN-HoaiMyNeural", "en-US-AvaMultilingualNeural")
   */
  public setVoiceProfile(voiceId: string) {
    this.#edgeTTS.setVoice(voiceId);
  }

  /**
   * [v25] Đọc voice config từ liva-config.json và đồng bộ
   */
  async #syncVoiceProfileFromConfig() {
    try {
      const configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
      const data = await fs.readFile(configPath, "utf8");
      const config = JSON.parse(data);
      const activeProfile = config?.voice?.activeProfile;
      if (activeProfile && activeProfile !== "default") {
        this.setVoiceProfile(activeProfile);
      }
    } catch {
      // Config not found or malformed — use default voice
    }
  }

  /**
   * Speak a text directly (one-shot TTS).
   * Synthesizes and emits audio_base64 event.
   */
  public async speak(text: string): Promise<boolean> {
    if (this.#isDestroyed) return false;
    if (!text.trim()) return false;

    try {
      const audioBuffer = await this.#edgeTTS.synthesize(text);
      if (audioBuffer) {
        const base64 = audioBuffer.toString("base64");
        this.emit("audio_base64", base64);
        // [Optimization C4] Also emit raw buffer for binary protocol
        this.emit("audio_buffer", audioBuffer);
        return true;
      }
      return false;
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.warn({ err: errMsg, context: "VoiceEngine" }, "Edge-TTS synthesis failed");
      return false;
    }
  }

  /**
   * [P5] Hứng luồng Token từ não AI, gom thành câu hoàn chỉnh + sanitize
   * rồi đẩy vào TTS queue. Chống TTS Stuttering.
   */
  public pushTokens(token: string) {
    if (this.#isDestroyed) return;

    const sentence = this.#ttsFormatter.pushToken(token);
    if (sentence && sentence.trim().length > 0) {
      this.#enqueue(sentence);
    }
  }

  /**
   * [P5] Flush buffer cuối stream — gửi nốt câu cuối cùng còn sót.
   */
  public flushTTS() {
    if (this.#isDestroyed) return;

    const remainder = this.#ttsFormatter.flush();
    if (remainder && remainder.trim().length > 0) {
      this.#enqueue(remainder);
    }
  }

  /**
   * Ngắt lời / barge-in
   */
  public preempt() {
    logger.warn(`[VoiceEngine] 🛑 Nhận lệnh Preempt! Dừng TTS.`);
    this.#ttsFormatter.reset();
    this.#ttsQueue = [];
  }

  /**
   * 🔒 [Memory Fix #2] Dọn dẹp hoàn toàn khi Gateway đóng
   */
  public async destroy(): Promise<void> {
    logger.info(`[VoiceEngine] 🧹 Đang dọn dẹp tài nguyên...`);
    this.#isDestroyed = true;
    this.#ttsFormatter.reset();
    this.#ttsQueue = [];
    this.removeAllListeners();
  }

  /**
   * Push text into TTS queue and trigger processing.
   */
  #enqueue(text: string): void {
    if (this.#ttsQueue.length < this.MAX_QUEUE_SIZE) {
      this.#ttsQueue.push(text);
      this.#processQueue();
    } else {
      logger.warn(`[VoiceEngine] ⚠️ TTS queue full (${this.MAX_QUEUE_SIZE}). Dropping chunk to protect RAM.`);
    }
  }

  /**
   * Drain TTS queue sequentially. Yields Event Loop between items
   * to prevent blocking WebSocket and gRPC handlers.
   */
  async #processQueue(): Promise<void> {
    if (this.#isProcessing || this.#isDestroyed) return;

    this.#isProcessing = true;

    while (this.#ttsQueue.length > 0 && !this.#isDestroyed) {
      const text = this.#ttsQueue.shift()!;
      try {
        const audioBuffer = await this.#edgeTTS.synthesize(text);
        if (audioBuffer && !this.#isDestroyed) {
          const base64 = audioBuffer.toString("base64");
          this.emit("audio_base64", base64);
          // [Optimization C4] Also emit raw buffer for binary protocol
          this.emit("audio_buffer", audioBuffer);
        }
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.warn(`[VoiceEngine] TTS queue item failed: ${errMsg}`);
      }

      // Yield Event Loop
      await new Promise(resolve => setTimeout(resolve, 0));
    }

    this.#isProcessing = false;
  }
}
