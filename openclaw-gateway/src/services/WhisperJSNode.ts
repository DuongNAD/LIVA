import { EventEmitter } from 'node:events';
import { Worker } from 'node:worker_threads';
import * as path from 'node:path';
import { fileURLToPath } from 'node:url';
import { logger } from "../utils/logger";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

/**
 * WhisperJSNode — Zero-Python STT using @huggingface/transformers (ONNX)
 * ========================================================================
 * Drop-in replacement for WhisperNode.
 * Uses worker_threads + Zero-Copy IPC (Transferable Objects) to ensure the 
 * Node.js Main Event Loop is NEVER blocked by heavy ONNX inference.
 */
export class WhisperJSNode extends EventEmitter {
  private audioBuffer: Buffer[] = [];
  private isProcessing: boolean = false;
  private silenceTimer: NodeJS.Timeout | null = null;
  private worker: Worker | null = null;
  private isReady = false;
  private isDestroyed = false;

  private readonly VAD_SILENCE_MS = 800;

  constructor() {
    super();
    // Defer async init to microtask queue (outside constructor body)
    this._initPromise = Promise.resolve().then(() => this.initWorker()); // NOSONAR
  }

  /** Await this to know when the STT engine is ready */
  public readonly _initPromise: Promise<void>;

  private async initWorker(): Promise<void> {
    return new Promise((resolve) => {
      try {
        logger.info(`👂 [WhisperJS] Spawning Worker Thread for STT Inference...`);
        
        // Ensure TSX or ESBuild can resolve the worker
        const workerPath = path.join(_dirname, "..", "workers", "WhisperWorker.ts");
        
        // Spawn worker with TSX loader if running in dev or test mode
        const isDevOrTest = process.argv.includes('--dev') || process.env.VITEST || process.env.NODE_ENV === 'test';
        const execArgv = isDevOrTest ? ['--import', 'tsx'] : [];
        
        this.worker = new Worker(workerPath, { execArgv });

        this.worker.on("message", (msg) => {
          if (msg.type === "ready") {
            this.isReady = true;
            logger.info(`✅ [WhisperJS] Worker ready! Transcriber loaded in separate thread.`);
            resolve();
          } else if (msg.type === "transcription") {
            logger.info(`[WhisperJS] 🎯 Transcription: "${msg.text}"`);
            this.emit("transcription_ready", msg.text);
          } else if (msg.type === "error") {
            logger.error(`❌ [WhisperJS] Worker Error: ${msg.message}`);
          }
        });

        this.worker.on("error", (err: Error) => {
          logger.error(`❌ [WhisperJS] Worker Crash: ${err.message}`);
        });

        this.worker.postMessage({ type: "init" });
      } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
        logger.error(`❌ [WhisperJS] Init failed: ${errMsg}`);
        resolve();
      }
    });
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
  private processAudio() {
    if (this.audioBuffer.length === 0 || this.isProcessing) return;
    if (!this.isReady || !this.worker) {
      logger.warn(`[WhisperJS] Worker not ready yet. Buffered audio discarded.`);
      this.audioBuffer = [];
      return;
    }

    this.isProcessing = true;
    logger.debug(`[WhisperJS] 🎙️ Silence detected. Sending to Worker (Zero-Copy IPC)...`);

    const fullBuffer = Buffer.concat(this.audioBuffer);
    this.audioBuffer = [];

    if (fullBuffer.length > 4096) {
      // 1. Convert Buffer to Float32Array
      const float32Arr = new Float32Array(
        fullBuffer.buffer,
        fullBuffer.byteOffset,
        fullBuffer.byteLength / 4
      );

      // 2. Extract underlying ArrayBuffer
      const arrayBuffer = float32Arr.buffer;

      // 3. ZERO-COPY TRANSFER: Pass ownership to worker
      this.worker.postMessage(
        { type: "process", buffer: arrayBuffer },
        [arrayBuffer] // transferList
      );
    }
    
    // We set isProcessing false immediately because the Worker handles it asynchronously.
    // Real barge-in will flush the buffers anyway.
    this.isProcessing = false;
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
    logger.info(`[WhisperJS] 🧹 Disposing Worker Thread...`);
    this.isDestroyed = true;
    this.flush();
    if (this.worker) {
      this.worker.terminate();
      this.worker = null;
    }
    this.isReady = false;
    this.removeAllListeners();
  }
}

