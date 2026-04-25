import { EventEmitter } from 'node:events';
import { logger } from "../utils/logger";

/**
 * KokoroVoiceEngine — Zero-Python TTS using kokoro-js (ONNX)
 * ===========================================================
 * Drop-in replacement for the Python Edge-TTS VoiceEngine.
 * Runs 100% in Node.js. No Python, no cloud, no internet.
 *
 * Model: onnx-community/Kokoro-82M-v1.0-ONNX (q8 quantization)
 * Size: ~80MB, downloads on first use, cached locally.
 *
 * API contract matches VoiceEngine exactly:
 *   - pushTokens(token) → buffers tokens → splits on sentence boundary → generates audio
 *   - preempt() → clears buffer + cancels pending TTS
 *   - destroy() → cleans up
 *   - emits "audio_base64" with base64-encoded WAV chunks
 */
export class KokoroVoiceEngine extends EventEmitter {
  private tts: any = null;
  private isReady = false;
  private tokenBuffer: string = "";
  private pendingTextQueue: string[] = [];
  private readonly MAX_QUEUE_SIZE = 50;
  private isProcessing = false;
  private isDestroyed = false;

  // Configuration
  private readonly MODEL_ID = "onnx-community/Kokoro-82M-v1.0-ONNX";
  private readonly VOICE = "af_heart"; // Default voice
  private readonly DTYPE: "q8" | "fp32" | "q4" = "q8"; // Best balance

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    // This prevents uncaught promise rejection and satisfies SonarQube S4738
    this._initPromise = Promise.resolve().then(() => this.initModel()); // NOSONAR - deferred async init pattern, not a direct async call in constructor
  }

  /** Await this to know when the TTS engine is ready */
  public readonly _initPromise: Promise<void>;

  private async initModel() {
    try {
      logger.info(`🎙️ [KokoroTTS] Initializing Kokoro-JS (${this.MODEL_ID}, dtype=${this.DTYPE})...`);
      logger.info(`🎙️ [KokoroTTS] First run downloads ~80MB model. Cached for future use.`);

      // Dynamic import — kokoro-js is ESM-only
      const { KokoroTTS } = await import("kokoro-js");

      this.tts = await KokoroTTS.from_pretrained(this.MODEL_ID, {
        dtype: this.DTYPE,
        device: "cpu",
      });

      this.isReady = true;
      logger.info(`✅ [KokoroTTS] Model loaded! Voices: ${this.tts.list_voices().length}`);

      // Process any queued text
      this.processQueue();
    } catch (e: any) {
      logger.error(`❌ [KokoroTTS] Init failed: ${e.message}`);
      logger.info(`💡 [KokoroTTS] Falling back to silence mode. Install kokoro-js correctly or use Python VoiceEngine.`);
    }
  }

  /**
   * Buffer tokens from AI stream, split on sentence boundaries, generate audio.
   */
  public pushTokens(token: string) {
    if (this.isDestroyed) return;

    // Strip emotion tags before TTS (they're for the avatar, not speech)
    const cleanToken = token.replace(/^\[(happy|sad|angry|surprised|neutral|relaxed)\]/, "");
    this.tokenBuffer += cleanToken;

    // Split on sentence boundaries (. ? ! \n)
    const m = this.tokenBuffer.match(/([^.?!\n]+[.?!\n]+)/); // NOSONAR
    if (m && m.index !== undefined) { // NOSONAR
      const sentence = m[0].trim();
      this.tokenBuffer = this.tokenBuffer.substring(m.index + m[0].length).trimStart();
      if (sentence.length > 3) {
        this.enqueue(sentence);
      }
    }
  }

  private enqueue(text: string) {
    if (this.pendingTextQueue.length < this.MAX_QUEUE_SIZE) {
      this.pendingTextQueue.push(text);
      this.processQueue();
    } else {
      logger.warn(`[KokoroTTS] ⚠️ Queue full (${this.MAX_QUEUE_SIZE}). Dropping chunk.`);
    }
  }

  private async processQueue() {
    if (!this.isReady || this.isProcessing || this.isDestroyed) return;
    if (this.pendingTextQueue.length === 0) return;

    this.isProcessing = true;

    while (this.pendingTextQueue.length > 0 && !this.isDestroyed) {
      const text = this.pendingTextQueue.shift()!;
      try {
        const audio = await this.tts.generate(text, {
          voice: this.VOICE,
        });

        if (this.isDestroyed) break;

        // Convert to base64 WAV for UI playback
        // kokoro-js returns an AudioResult with .toBlob() or raw data
        const wavBuffer = audio.toWav();
        const base64 = Buffer.from(wavBuffer).toString("base64");
        this.emit("audio_base64", base64);
      } catch (e: any) {
        logger.error(`[KokoroTTS] Generation failed for "${text.substring(0, 30)}...": ${e.message}`);
      }
    }

    this.isProcessing = false;
  }

  /**
   * Interrupt / barge-in — stop current TTS immediately
   */
  public preempt() {
    logger.warn(`[KokoroTTS] 🛑 Preempt! Clearing queue.`);
    this.tokenBuffer = "";
    this.pendingTextQueue = [];
    // Note: kokoro-js generate() is not cancellable, but clearing the queue
    // prevents new sentences from being processed.
  }

  /**
   * Full cleanup
   */
  public destroy() {
    logger.info(`[KokoroTTS] 🧹 Disposing TTS engine...`);
    this.isDestroyed = true;
    this.tokenBuffer = "";
    this.pendingTextQueue = [];
    this.tts = null;
    this.isReady = false;
    this.removeAllListeners();
  }
}
