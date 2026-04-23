import { EventEmitter } from "events";
import { logger } from "../utils/logger";

/**
 * WhisperJSNode — Zero-Python STT using @huggingface/transformers (ONNX)
 * ========================================================================
 * Drop-in replacement for WhisperNode (Python whisper.cpp HTTP).
 * Runs 100% in Node.js. No Python, no external server.
 *
 * Model: onnx-community/whisper-base (auto-downloaded, ~140MB)
 * API contract matches WhisperNode:
 *   - pushAudioChunk(buffer) → buffers PCM → VAD silence detection → STT
 *   - flush() → clears buffer on barge-in
 *   - emits "transcription_ready" with transcribed text
 */
export class WhisperJSNode extends EventEmitter {
  private audioBuffer: Buffer[] = [];
  private isProcessing: boolean = false;
  private silenceTimer: NodeJS.Timeout | null = null;
  private pipeline: any = null;
  private isReady = false;
  private isDestroyed = false;

  private readonly VAD_SILENCE_MS = 800;
  private readonly MODEL_ID = "onnx-community/whisper-base";

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    // This prevents uncaught promise rejection and satisfies SonarQube S4738
    this._initPromise = Promise.resolve().then(() => this.initModel());
  }

  /** Await this to know when the STT engine is ready */
  public readonly _initPromise: Promise<void>;

  private async initModel() {
    try {
      logger.info(`👂 [WhisperJS] Initializing HuggingFace Whisper (${this.MODEL_ID})...`);
      logger.info(`👂 [WhisperJS] First run downloads ~140MB model. Cached for future use.`);

      // Dynamic import — @huggingface/transformers is ESM
      const { pipeline } = await import("@huggingface/transformers");

      this.pipeline = await pipeline(
        "automatic-speech-recognition",
        this.MODEL_ID,
        {
          dtype: "q8",
          device: "cpu",
        }
      );

      this.isReady = true;
      logger.info(`✅ [WhisperJS] Whisper model loaded! Ready for transcription.`);
    } catch (e: any) {
      logger.error(`❌ [WhisperJS] Init failed: ${e.message}`);
      logger.info(`💡 [WhisperJS] Falling back to silence. Use LIVA_STT_ENGINE=http to use external Whisper server.`);
    }
  }

  /**
   * Receive PCM audio chunks from microphone
   */
  public pushAudioChunk(chunk: Buffer) {
    if (this.isDestroyed) return;
    this.audioBuffer.push(chunk);

    if (this.silenceTimer) {
      clearTimeout(this.silenceTimer);
    }
    this.silenceTimer = setTimeout(() => this.processAudio(), this.VAD_SILENCE_MS);
  }

  /**
   * Process accumulated audio when silence detected
   */
  private async processAudio() {
    if (this.audioBuffer.length === 0 || this.isProcessing) return;
    if (!this.isReady || !this.pipeline) {
      logger.warn(`[WhisperJS] Model not ready yet. Buffered audio discarded.`);
      this.audioBuffer = [];
      return;
    }

    this.isProcessing = true;
    logger.debug(`[WhisperJS] 🎙️ Silence detected. Processing audio...`);

    const fullBuffer = Buffer.concat(this.audioBuffer);
    this.audioBuffer = [];

    try {
      if (fullBuffer.length > 4096) {
        // Convert Float32 PCM to Float32Array for pipeline
        const float32Arr = new Float32Array(
          fullBuffer.buffer,
          fullBuffer.byteOffset,
          fullBuffer.byteLength / 4
        );

        // Create WAV in memory for the pipeline
        const wavBuffer = this.encodeWAV(float32Arr, 16000);

        // Run inference
        const result = await this.pipeline(
          new Blob([new Uint8Array(wavBuffer)], { type: "audio/wav" }),
          {
            language: "vi", // Vietnamese default (LIVA's primary language)
            task: "transcribe",
          }
        );

        const text = (result?.text || "").trim();
        if (text && text.length > 0) {
          logger.info(`[WhisperJS] 🎯 Transcription: "${text}"`);
          this.emit("transcription_ready", text);
        }
      }
    } catch (e: any) {
      logger.error(`[WhisperJS] ❌ Transcription failed: ${e.message}`);
    } finally {
      this.isProcessing = false;
    }
  }

  /**
   * Encode Float32 PCM → WAV format (16-bit, mono, 16kHz)
   */
  private encodeWAV(samples: Float32Array, sampleRate: number): Buffer {
    const wavBuffer = Buffer.alloc(44 + samples.length * 2);

    // WAV Header
    wavBuffer.write("RIFF", 0);
    wavBuffer.writeUInt32LE(36 + samples.length * 2, 4);
    wavBuffer.write("WAVE", 8);
    wavBuffer.write("fmt ", 12);
    wavBuffer.writeUInt32LE(16, 16);
    wavBuffer.writeUInt16LE(1, 20);   // PCM
    wavBuffer.writeUInt16LE(1, 22);   // Mono
    wavBuffer.writeUInt32LE(sampleRate, 24);
    wavBuffer.writeUInt32LE(sampleRate * 2, 28);
    wavBuffer.writeUInt16LE(2, 32);
    wavBuffer.writeUInt16LE(16, 34);
    wavBuffer.write("data", 36);
    wavBuffer.writeUInt32LE(samples.length * 2, 40);

    // Float32 → Int16
    let offset = 44;
    for (let i = 0; i < samples.length; i++) {
      const s = Math.max(-1, Math.min(1, samples[i]));
      wavBuffer.writeInt16LE(s < 0 ? s * 0x8000 : s * 0x7FFF, offset);
      offset += 2;
    }

    return wavBuffer;
  }

  /**
   * Barge-in flush
   */
  public flush() {
    this.audioBuffer = [];
    if (this.silenceTimer) clearTimeout(this.silenceTimer);
    this.isProcessing = false;
    logger.debug(`[WhisperJS] 🧹 Buffer flushed due to Preemption.`);
  }

  /**
   * Full cleanup
   */
  public destroy() {
    logger.info(`[WhisperJS] 🧹 Disposing STT engine...`);
    this.isDestroyed = true;
    this.flush();
    this.pipeline = null;
    this.isReady = false;
    this.removeAllListeners();
  }
}
