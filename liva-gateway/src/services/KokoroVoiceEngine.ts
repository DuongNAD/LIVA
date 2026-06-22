import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";
import { IVoiceEngine } from "./IVoiceEngine";
import { TTSFormatter } from "../utils/TTSFormatter";
import { Worker } from 'node:worker_threads';
import * as path from 'node:path';
import * as fs from 'node:fs';

const getFilename = () => {
  try {
    return __filename;
  } catch {
    try {
      return eval('import.meta.filename');
    } catch {
      return '';
    }
  }
};

const getDirname = () => {
  try {
    return __dirname;
  } catch {
    try {
      return eval('import.meta.dirname');
    } catch {
      return '';
    }
  }
};

/**
 * KokoroVoiceEngine — Zero-Python TTS using kokoro-js (ONNX) in worker thread.
 * Fallback Engine - Tự động yield Event Loop chống giật khựng giao diện.
 * [Optimization C2] Idle unload: Model unloads after 5 min inactivity to save ~82MB RAM.
 */
export class KokoroVoiceEngine extends EventEmitter implements IVoiceEngine {
  #worker: Worker | null = null;
  #initPromise: Promise<void> | null = null;
  #initResolve: (() => void) | null = null;
  #initReject: ((err: Error) => void) | null = null;
  #generateResolve: ((base64: string) => void) | null = null;
  #generateReject: ((err: Error) => void) | null = null;
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

  /** Await this to know when the TTS engine is ready */
  public readonly _initPromise: Promise<void>;

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    this._initPromise = Promise.resolve().then(() => this.#initModel());
    // Prevent unhandled promise rejection warnings if destroyed before initialization completes
    this._initPromise.catch(() => {});
  }

  #initModel(): Promise<void> {
    if (this.#initPromise) return this.#initPromise;

    this.#initPromise = new Promise<void>((resolve, reject) => {
      try {
        logger.info(`🎙️ [KokoroTTS] Initializing Kokoro-JS (${this.#MODEL_ID}, dtype=${this.#DTYPE}) in Worker Thread...`);
        
        this.#initResolve = resolve;
        this.#initReject = reject;
        
        const currentFile = getFilename() || "";
        const isDev = currentFile.endsWith(".ts") || currentFile.endsWith(".tsx");
        let w: Worker;

        if (isDev) {
          const workerPath = path.join(getDirname(), "..", "workers", "KokoroWorker.ts");
          w = new Worker(`
              require('tsx/cjs');
              require(${JSON.stringify(workerPath)});
          `, { eval: true });
        } else {
          const path1 = path.join(process.cwd(), "dist", "KokoroWorker.js");
          const path2 = path.join(getDirname(), "KokoroWorker.js");
          const workerJsPath = fs.existsSync(path1) ? path1 : path2;
          w = new Worker(workerJsPath);
        }
        this.#worker = w;

        w.on("message", (msg) => {
          if (this.#worker !== w) return;
          if (msg.type === "ready") {
            this.#isReady = true;
            logger.info(`✅ [KokoroTTS] Worker ready successfully!`);
            if (this.#initResolve) {
              this.#initResolve();
              this.#initResolve = null;
              this.#initReject = null;
            }
            // Process any queued text
            this.#processQueue();
            // Start idle unload timer
            this.#touchIdleTimer();
          } else if (msg.type === "audio_result") {
            if (this.#generateResolve) {
              this.#generateResolve(msg.base64);
              this.#generateResolve = null;
              this.#generateReject = null;
            }
          } else if (msg.type === "error") {
            const err = new Error(msg.message);
            logger.error(`❌ [KokoroTTS] Worker error: ${msg.message}`);
            
            try {
              w.terminate();
            } catch { /* ignore */ }
            this.#worker = null;
            this.#isReady = false;

            if (this.#initReject) {
              this.#initReject(err);
              this.#initResolve = null;
              this.#initReject = null;
            }
            if (this.#generateReject) {
              this.#generateReject(err);
              this.#generateResolve = null;
              this.#generateReject = null;
            }
          }
        });

        w.on("error", (err: Error) => {
          if (this.#worker !== w) return;
          try {
            w.terminate();
          } catch { /* ignore */ }
          this.#worker = null;
          this.#isReady = false;
          logger.error(`❌ [KokoroTTS] Worker thread error: ${err?.message || String(err)}`);
          if (this.#initReject) {
            this.#initReject(err instanceof Error ? err : new Error(String(err)));
            this.#initResolve = null;
            this.#initReject = null;
          }
          if (this.#generateReject) {
            this.#generateReject(err instanceof Error ? err : new Error(String(err)));
            this.#generateResolve = null;
            this.#generateReject = null;
          }
        });

        w.on("exit", (code) => {
          if (this.#worker !== w) return;
          logger.warn(`[KokoroTTS] Worker exited with code ${code}`);
          if (this.#initReject) {
            this.#initReject(new Error("Kokoro worker exited during initialization"));
            this.#initResolve = null;
            this.#initReject = null;
          }
          if (this.#generateReject) {
            this.#generateReject(new Error(`Worker thread exited with code ${code}`));
            this.#generateResolve = null;
            this.#generateReject = null;
          }
          this.#worker = null;
          this.#isReady = false;
        });

        w.postMessage({
          type: "init",
          modelId: this.#MODEL_ID,
          dtype: this.#DTYPE
        });

      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`❌ [KokoroTTS] Init failed: ${errMsg}`);
        reject(e instanceof Error ? e : new Error(errMsg));
      }
    }).finally(() => {
      this.#initPromise = null;
    });

    return this.#initPromise;
  }

  /**
   * [Optimization C2] Reset idle unload timer — called on every speak().
   * After IDLE_UNLOAD_MS of no activity, unload model to free ~82MB RAM.
   */
  #touchIdleTimer(): void {
    if (this.#idleUnloadTimer) {
      clearTimeout(this.#idleUnloadTimer);
    }
    this.#idleUnloadTimer = setTimeout(async () => {
      this.#idleUnloadTimer = null;
      if (this.#isReady && this.#worker && !this.#isProcessing) {
        logger.info(`[KokoroTTS] ♻️ Idle for ${this.#IDLE_UNLOAD_MS / 1000}s — unloading model/worker to free RAM.`);
        const w = this.#worker;
        this.#worker = null;
        this.#isReady = false;
        try {
          w.postMessage({ type: "dispose" });
        } catch { /* ignore */ }
        try {
          if (typeof w.terminate === "function") {
            await w.terminate();
          }
        } catch { /* ignore */ }
      }
    }, this.#IDLE_UNLOAD_MS);
    this.#idleUnloadTimer.unref(); // Don't block process exit
  }

  /**
   * [Optimization C2] Ensure model is loaded before generation.
   * Auto-reloads if previously unloaded by idle timer.
   */
  async #ensureLoaded(): Promise<boolean> {
    const customThis = this as unknown as { ensureLoaded?: () => Promise<boolean> };
    if (customThis.ensureLoaded) {
      return customThis.ensureLoaded();
    }
    if (this.#isReady && this.#worker) return true;
    if (this.#isDestroyed) return false;

    logger.info(`[KokoroTTS] 🔄 Reloading model/worker after idle unload...`);
    try {
      await this.#initModel();
    } catch {
      return false;
    }
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

    this.#isProcessing = true;
    try {
      const loaded = await this.#ensureLoaded();
      if (!loaded || this.#isDestroyed) {
        this.#queue = []; // Prevent infinite reloading loop in finally
        return;
      }

      while (this.#queue.length > 0 && !this.#isDestroyed) {
        const loaded = await this.#ensureLoaded();
        if (!loaded || !this.#worker) {
          logger.error(`[KokoroTTS] Worker died and failed to reload mid-queue.`);
          break;
        }
        const text = this.#queue.shift()!;
        try {
          const base64 = await new Promise<string>((resolve, reject) => {
            if (!this.#worker || !this.#isReady) {
              reject(new Error("Kokoro worker is not available"));
              return;
            }
            this.#generateResolve = resolve;
            this.#generateReject = reject;
            this.#worker.postMessage({
              type: "generate",
              text,
              voice: this.#VOICE
            });
          });

          if (this.#isDestroyed) break;

          this.emit("audio_base64", base64);
        } catch (e: unknown) {
          const errMsg = e instanceof Error ? e.message : String(e);
          logger.error(`[KokoroTTS] Generation failed for "${text.substring(0, 30)}...": ${errMsg}`);
        }

        // [CRITICAL] Nhường quyền cho Node.js Event Loop xử lý gRPC và WebSocket
        await new Promise(resolve => setTimeout(resolve, 0));
      }
    } finally {
      this.#isProcessing = false;
      if (this.#queue.length > 0 && !this.#isDestroyed) {
        this.#processQueue();
      } else if (!this.#isDestroyed) {
        this.#touchIdleTimer();
      }
    }
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
    if (this.#generateReject) {
      this.#generateReject(new Error("Preempted"));
      this.#generateResolve = null;
      this.#generateReject = null;
    }
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
    if (this.#initReject) {
      this.#initReject(new Error("Kokoro worker exited during initialization"));
      this.#initResolve = null;
      this.#initReject = null;
    }
    if (this.#generateReject) {
      this.#generateReject(new Error("KokoroVoiceEngine was destroyed during generation"));
      this.#generateResolve = null;
      this.#generateReject = null;
    }

    if (this.#worker) {
      const w = this.#worker;
      this.#worker = null;
      this.#isReady = false;
      try {
        w.postMessage({ type: "dispose" });
      } catch {
        // ignore
      }
      w.removeAllListeners();
      try {
        await w.terminate();
      } catch {
        /* ignore */
      }
    }
    this.removeAllListeners();
  }
}
