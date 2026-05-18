import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import WebSocket from "ws";
import { IVoiceEngine } from "./IVoiceEngine";
import { safeFetch } from "../utils/HttpClient";
import { TTSFormatter } from "../utils/TTSFormatter";
import * as path from "node:path";
import * as fs from "node:fs";

/**
 * VoiceEngine v3 - Relay âm thanh từ Python voice_engine.py (edge_tts)
 * Sử dụng Kiến trúc Hybrid với safeFetch cho HTTP request.
 * [P5] TTSFormatter: Gom token thành câu hoàn chỉnh + sanitize trước khi phát âm.
 */
export class VoiceEngine extends EventEmitter implements IVoiceEngine {
  private ws: WebSocket | null = null;
  #reconnectTimer: NodeJS.Timeout | null = null;
  private voicePyUrl = "ws://127.0.0.1:8002/ws";
  #ttsFormatter: TTSFormatter = new TTSFormatter();
  private pendingTextQueue: string[] = [];
  // 🔒 [Memory Fix #1] Giới hạn hàng đợi để tránh phình RAM khi Python Engine offline lâu
  private readonly MAX_QUEUE_SIZE = 50;
  #hasLoggedDisconnect = false;

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
        this.#hasLoggedDisconnect = false;
        // [v25] Đồng bộ voice profile từ config khi kết nối lại
        this.#syncVoiceProfileFromConfig();
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
        } catch (e) { void e; }
      });

      this.ws.on("close", () => {
        if (!this.#hasLoggedDisconnect) {
            logger.warn("⚠️ [VoiceEngine] Mất kết nối Python Engine. Sẽ tự động kết nối lại ngầm...");
            this.#hasLoggedDisconnect = true;
        }
        this.ws = null;
        if (this.#reconnectTimer) clearTimeout(this.#reconnectTimer);
        this.#reconnectTimer = setTimeout(() => this.connect(), 5000);
      });

      this.ws.on("error", (err) => {
        // Suppress WS error log since it's handled by 'close'
        // logger.debug(`[VoiceEngine] Lỗi WS (sẽ tự retry): ${err.message}`);
      });
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? e.message : String(e);
      logger.error(`[VoiceEngine] Không thể tạo kết nối: ${errMsg}`);
    }
  }

  private sendToVoicePy(text: string) {
    if (this.ws && this.ws.readyState === WebSocket.OPEN) { // NOSONAR
      this.ws.send(JSON.stringify({ type: "tts", text }));
    } else {
      // 🔒 [Memory Fix #1] Chống phình hàng đợi: chỉ nhét vào nếu chưa đầy
      if (this.pendingTextQueue.length < this.MAX_QUEUE_SIZE) {
        this.pendingTextQueue.push(text);
      } else {
        logger.warn(`[VoiceEngine] ⚠️ pendingTextQueue đầy (${this.MAX_QUEUE_SIZE}). Bỏ qua chunk để bảo vệ RAM.`);
      }
    }
  }

  /**
   * Chuyển đổi voice profile trên Python Voice Engine (Edge-TTS)
   * @param voiceId - Edge-TTS voice ID (e.g. "vi-VN-HoaiMyNeural", "en-US-AvaMultilingualNeural")
   */
  public setVoiceProfile(voiceId: string) {
    logger.info(`[VoiceEngine] 🎤 Chuyển giọng → ${voiceId}`);
    if (this.ws && this.ws.readyState === WebSocket.OPEN) { // NOSONAR
      this.ws.send(JSON.stringify({ type: "set_voice", voice: voiceId }));
    } else {
      logger.warn(`[VoiceEngine] ⚠️ Chưa kết nối Python Engine. Lưu voice profile để áp dụng sau.`);
    }
  }

  /**
   * [v25] Đọc voice config từ liva-config.json và đồng bộ với Python Engine
   */
  #syncVoiceProfileFromConfig() {
    try {
      const configPath = path.join(process.cwd(), "..", "data", "liva-config.json");
      const config = JSON.parse(fs.readFileSync(configPath, "utf8"));
      const activeProfile = config?.voice?.activeProfile;
      if (activeProfile && activeProfile !== "default") {
        this.setVoiceProfile(activeProfile);
      }
    } catch {
      // Config not found or malformed — use default voice
    }
  }

  /**
   * Gọi API Python TTS qua HTTP sử dụng safeFetch (Rule 4.1)
   */
  public async speak(text: string): Promise<boolean> {
    try {
      // Gọi sang API của tiến trình Python với timeout 3000ms
      const res = await safeFetch("http://127.0.0.1:8002/tts", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ text })
      }, 3000);

      if (res.ok) {
        const data = await res.json();
        if (data && data.audio) {
          this.emit("audio_base64", data.audio);
        }
        return true;
      } else {
        logger.warn({ context: "VoiceEngine" }, `Python TTS API trả về lỗi HTTP: ${res.status}`);
        return false;
      }
    } catch (e: unknown) {
      const errMsg = e instanceof Error ? ((e.cause instanceof Error ? e.cause.message : null) || e.message) : String(e);
      logger.warn({ err: errMsg, context: "VoiceEngine" }, "Không thể kết nối Python TTS");
      return false;
    }
  }

  /**
   * [P5] Hứng luồng Token từ não AI, gom thành câu hoàn chỉnh + sanitize
   * rồi gửi sang Python TTS. Chống TTS Stuttering.
   */
  public pushTokens(token: string) {
    const sentence = this.#ttsFormatter.pushToken(token);
    if (sentence && sentence.trim().length > 0) {
      this.sendToVoicePy(sentence);
    }
  }

  /**
   * [P5] Flush buffer cuối stream — gửi nốt câu cuối cùng còn sót.
   */
  public flushTTS() {
    const remainder = this.#ttsFormatter.flush();
    if (remainder && remainder.trim().length > 0) {
      this.sendToVoicePy(remainder);
    }
  }

  /**
   * Ngắt lời / barge-in
   */
  public preempt() {
    logger.warn(`[VoiceEngine] 🛑 Nhận lệnh Preempt! Dừng TTS.`);
    this.#ttsFormatter.reset();
    this.pendingTextQueue = []; // 🔒 [Memory Fix] Xả sạch hàng đợi khi bị ngắt lời
    if (this.ws && this.ws.readyState === WebSocket.OPEN) { // NOSONAR
      this.ws.send(JSON.stringify({ type: "interrupt" }));
    }
  }

  /**
   * 🔒 [Memory Fix #2] Dọn dẹp hoàn toàn khi Gateway đóng (Tránh Zombie Timer)
   */
  public async destroy(): Promise<void> {
    logger.info(`[VoiceEngine] 🧹 Đang dọn dẹp tài nguyên...`);
    if (this.#reconnectTimer) {
      clearTimeout(this.#reconnectTimer);
      this.#reconnectTimer = null;
    }
    if (this.ws) {
      this.ws.removeAllListeners(); // Gỡ bỏ tất cả event listener trước khi đóng
      this.ws.close();
      this.ws = null;
    }
    this.pendingTextQueue = [];
    this.#ttsFormatter.reset();
    this.removeAllListeners();
  }
}

