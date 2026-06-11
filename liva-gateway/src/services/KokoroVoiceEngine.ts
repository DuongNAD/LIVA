import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { IVoiceEngine } from "./IVoiceEngine";
import { TTSFormatter } from "../utils/TTSFormatter";
import { Worker } from "node:worker_threads";
import * as path from "node:path";

/**
 * KokoroVoiceEngine — Zero-Python TTS using kokoro-js (ONNX) offloaded to Worker Thread.
 * Fallback Engine - Tự động yield Event Loop chống giật khựng giao diện.
 * [Optimization C2] Idle unload: Model unloads after 5 min inactivity to save ~82MB RAM.
 */
export class KokoroVoiceEngine extends EventEmitter implements IVoiceEngine {
  #worker: Worker | null = null;
  #isReady = false;
  #ttsFormatter: TTSFormatter = new TTSFormatter();
  #queue: string[] = [];
  #isProcessing: boolean = false;
  #isDestroyed = false;
  #MAX_QUEUE_SIZE = 50;
  // [Optimization C2] Idle unload timer — free ~82MB RAM when Edge-TTS is stable
  #idleUnloadTimer: NodeJS.Timeout | null = null;
  readonly #IDLE_UNLOAD_MS = 5 * 60 * 1000; // 5 minutes

  // Configuration
  #MODEL_ID = "onnx-community/Kokoro-82M-v1.0-ONNX";
  #VOICE = "af_heart"; // Default voice
  #DTYPE: "q8" | "fp32" | "q4" = "q8"; // Best balance
  #initPromise: Promise<void> | null = null;
  #hasFailed = false;

  #pendingRequests = new Map<string, { resolve: (buf: Buffer) => void, reject: (err: Error) => void }>();
  #requestCounter = 0;

  /** Await this to know when the TTS engine is ready */
  public readonly _initPromise: Promise<void>;

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    this._initPromise = Promise.resolve().then(() => this.#initModel());
  }

  async #initModel(): Promise<void> {
    if (this.#isDestroyed) return;
    if (this.#initPromise) {
      return this.#initPromise;
    }

    if (this.#hasFailed) {
      logger.warn(`⚠️ [KokoroTTS] Bypassing initialization because it failed previously.`);
      return;
    }

    this.#initPromise = new Promise<void>((resolve, reject) => {
      try {
        if (this.#isDestroyed) {
          resolve();
          return;
        }
        logger.info(`🎙️ [KokoroTTS] Spawning Kokoro TTS Worker (${this.#MODEL_ID}, dtype=${this.#DTYPE})...`);

        const workerPath = path.join(import.meta.dirname, "..", "workers", "KokoroWorker.ts");
        this.#worker = new Worker(workerPath, {
            execArgv: ["--import", "tsx"]
        });

        this.#worker.on("message", (msg) => this.#handleWorkerMessage(msg, resolve, reject));

        this.#worker.on("error", (err: unknown) => {
          const e = err instanceof Error ? err : new Error(String(err));
          logger.error(`[KokoroTTS] Worker error: ${e.message}`);
          this.#cleanupWorker(`Worker error: ${e.message}`);
          reject(e);
        });

        this.#worker.on("exit", (code) => {
          logger.warn(`[KokoroTTS] Worker exited with code ${code}`);
          this.#cleanupWorker(`Worker exited with code ${code}`);
        });

        this.#worker.postMessage({ type: "init", modelId: this.#MODEL_ID, dtype: this.#DTYPE });
      } catch (e: unknown) {
        this.#hasFailed = true;
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`❌ [KokoroTTS] Init failed: ${errMsg}`);
        this.#initPromise = null;
        reject(e);
      }
    });

    return this.#initPromise;
  }

  #handleWorkerMessage(msg: any, resolveInit?: () => void, rejectInit?: (err: any) => void) {
    if (msg.type === "ready") {
      this.#isReady = true;
      logger.info(`[KokoroTTS] ✅ Kokoro TTS Worker ready.`);
      if (resolveInit) resolveInit();
      // Process any queued text
      this.#processQueue();
      // Start idle unload timer
      this.#touchIdleTimer();
      return;
    }

    if (msg.type === "error" && !msg.id) {
      logger.error(`[KokoroTTS] Worker error: ${msg.message}`);
      this.#hasFailed = true;
      if (rejectInit) rejectInit(new Error(msg.message));
      return;
    }

    if (msg.id && this.#pendingRequests.has(msg.id)) {
      const req = this.#pendingRequests.get(msg.id)!;
      this.#pendingRequests.delete(msg.id);
      if (msg.type === "generate_result") {
        req.resolve(Buffer.from(msg.wavBuffer));
      } else if (msg.type === "error") {
        req.reject(new Error(msg.message));
      }
    }
  }

  #cleanupWorker(errorMsg: string) {
    if (this.#worker && !this.#isReady) {
      this.#hasFailed = true;
    }
    this.#isReady = false;
    this.#worker = null;
    this.#initPromise = null;
    for (const req of this.#pendingRequests.values()) {
      req.reject(new Error(errorMsg));
    }
    this.#pendingRequests.clear();
  }

  /**
   * [Optimization C2] Reset idle unload timer — called on every speak().
   * After IDLE_UNLOAD_MS of no activity, unload model to free ~82MB RAM.
   */
  #touchIdleTimer(): void {
    if (this.#idleUnloadTimer) {
      clearTimeout(this.#idleUnloadTimer);
    }
    this.#idleUnloadTimer = setTimeout(() => {
      this.#idleUnloadTimer = null;
      if (this.#isReady && this.#worker && !this.#isProcessing) {
        logger.info(`[KokoroTTS] ♻️ Idle for ${this.#IDLE_UNLOAD_MS / 1000}s — unloading model to free RAM.`);
        this.#worker.postMessage({ type: "dispose" });
        this.#worker = null;
        this.#isReady = false;
        this.#initPromise = null;
      }
    }, this.#IDLE_UNLOAD_MS);
    this.#idleUnloadTimer.unref(); // Don't block process exit
  }

  /**
   * [Optimization C2] Ensure model is loaded before generation.
   * Auto-reloads if previously unloaded by idle timer.
   */
  async #ensureLoaded(): Promise<boolean> {
    if (this.#isReady && this.#worker) return true;
    if (this.#isDestroyed) return false;
    if (this.#hasFailed) {
      logger.warn(`⚠️ [KokoroTTS] Skip ensureLoaded because initialization failed previously.`);
      return false;
    }

    logger.info(`[KokoroTTS] 🔄 Reloading model after idle unload...`);
    await this.#initModel();
    return this.#isReady;
  }

  /**
   * Gọi API sinh giọng nói. Đẩy vào hàng đợi và kích hoạt processQueue.
   */
  public async speak(text: string): Promise<boolean> {
    if (this.#isDestroyed) return false;

    // [Optimization C2] Reset idle timer on every speak
    this.#touchIdleTimer();

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
    if (this.#isProcessing || this.#isDestroyed) return;
    if (this.#queue.length === 0) return;

    // [Optimization C2] Ensure model is loaded (may have been idle-unloaded)
    const loaded = await this.#ensureLoaded();
    if (!loaded) return;

    this.#isProcessing = true;

    while (this.#queue.length > 0 && !this.#isDestroyed) {
      const text = this.#queue.shift()!;
      try {
        const id = `req_${++this.#requestCounter}`;
        const requestPromise = new Promise<Buffer>((resolve, reject) => {
          this.#pendingRequests.set(id, { resolve, reject });
        });

        if (!this.#worker) {
          throw new Error("Worker was terminated unexpectedly");
        }

        this.#worker.postMessage({ type: "generate", id, text, voice: this.#VOICE });

        const wavBuffer = await requestPromise;

        if (this.#isDestroyed) break;

        // Convert to base64 WAV for UI playback
        const base64 = wavBuffer.toString("base64");
        this.emit("audio_base64", base64);
        this.emit("audio_buffer", wavBuffer);
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`[KokoroTTS] Generation failed for "${text.substring(0, 30)}...": ${errMsg}`);
      }

      // [CRITICAL] Nhường quyền cho Node.js Event Loop xử lý gRPC và WebSocket
      await new Promise(resolve => setTimeout(resolve, 0));
    }

    this.#isProcessing = false;
    this.#touchIdleTimer();
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
    for (const req of this.#pendingRequests.values()) {
      req.reject(new Error("Preempted"));
    }
    this.#pendingRequests.clear();
  }

  public async destroy(): Promise<void> {
    logger.info(`[KokoroTTS] 🧹 Disposing TTS engine...`);
    this.#isDestroyed = true;
    // [Optimization C2] Clear idle unload timer
    if (this.#idleUnloadTimer) {
      clearTimeout(this.#idleUnloadTimer);
      this.#idleUnloadTimer = null;
    }
    this.#ttsFormatter.reset();
    this.#queue = [];

    if (this.#worker) {
      this.#worker.postMessage({ type: "dispose" });
      this.#worker = null;
    }

    this.#isReady = false;
    this.#initPromise = null;
    this.#hasFailed = false;

    for (const req of this.#pendingRequests.values()) {
      req.reject(new Error("KokoroVoiceEngine destroyed"));
    }
    this.#pendingRequests.clear();

    this.removeAllListeners();
  }
}


