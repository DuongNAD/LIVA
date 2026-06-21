/**
 * NemotronSTTService — Streaming STT Bridge (Replaces WhisperNode)
 * =================================================================
 * [v31 Pillar 4: Native Streaming STT]
 *
 * Main-thread EventEmitter bridge to NemotronWorker (worker_thread).
 * Drop-in replacement for WhisperNode — preserves the exact same public API
 * so CoreKernel, VoiceOrchestrator, and ReactiveSync need zero logic changes.
 *
 * KEY DIFFERENCES FROM WhisperNode:
 *   - WhisperNode: Buffers ALL audio → speech_end → encode WAV → HTTP POST → full text
 *   - NemotronSTT: Streams audio chunks → worker ONNX → partial text in real-time → final text
 *
 * EVENTS EMITTED (same contract as WhisperNode):
 *   - "transcription_partial"  → Partial streaming text (real-time, word-by-word)
 *   - "transcription_ready"    → Final complete transcription (after speech_end)
 *   - "stt_fallback_activated" → Circuit breaker opened (3 failures)
 *   - "stt_fallback_deactivated" → Circuit breaker reset
 *
 * VRAM IMPACT: 0% — NemotronWorker uses onnxruntime-node CPU-only.
 * RAM IMPACT: ~700MB for INT4 quantized model.
 */

import { Worker } from "node:worker_threads";
import { EventEmitter } from "node:events";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { logger } from "../utils/logger";
import { ConfigManager } from "../core/config/ConfigManager";

// ESM-first: Node.js 22+ supports import.meta.dirname natively
// SEA fallback: esbuild CJS bundle provides __dirname
const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

/** Max automatic recovery attempts before permanently disabling STT */
const MAX_CRASH_RECOVERY = 3;

/** Watchdog: Ping interval (5 seconds) — same as VADWorkerBridge */
const WATCHDOG_PING_MS = 5_000;

/** Watchdog: Max time to wait for PONG before declaring deadlock (15 seconds) */
const WATCHDOG_TIMEOUT_MS = 15_000;

/** Circuit breaker: Open after this many consecutive failures */
const CIRCUIT_THRESHOLD = 3;

/** Circuit breaker: Auto-reset after 15 seconds */
const CIRCUIT_RESET_MS = 15_000;

/** Audio buffer cap: 60s of 16kHz Float32 PCM ≈ 3.84MB */
const MAX_AUDIO_BUFFER_BYTES = 16000 * 4 * 60;

/** Default model directory relative to gateway root */
const DEFAULT_MODEL_DIR = "./models/nemotron-asr";

export class NemotronSTTService extends EventEmitter {
    #worker: Worker | null = null;
    #isReady = false;
    #isDestroyed = false;
    #lastPartialText = "";

    // Audio buffer (for pushAudioChunkOnly compatibility)
    #audioBuffer: Float32Array[] = [];
    #totalBufferBytes = 0;
    #silenceTimer: NodeJS.Timeout | null = null;

    // Circuit Breaker (v25 Anti-DDoS — same pattern as WhisperNode)
    #isCircuitOpen = false;
    #consecutiveFailures = 0;
    #circuitTimer: NodeJS.Timeout | null = null;

    // Crash Recovery
    #crashCount = 0;
    #recoveryTimer: ReturnType<typeof setTimeout> | null = null;

    // Watchdog Heartbeat
    #watchdogInterval: ReturnType<typeof setInterval> | null = null;
    #lastPongTime = 0;

    // Streaming state
    #isStreaming = false;

    // Ngưỡng 800ms Silence — fallback for environments without VADWorkerBridge
    private readonly VAD_SILENCE_MS = 800;

    constructor() {
        super();
        logger.info(`[NemotronSTT] Khởi tạo Hệ thống Thính giác (Streaming STT). Chờ tín hiệu Float32 PCM.`);
    }

    // ═══════════════════════════════════════════════════════
    //  Worker Lifecycle
    // ═══════════════════════════════════════════════════════

    /**
     * Initialize the Nemotron worker thread and load the ONNX model.
     * Called during CoreKernel boot sequence.
     */
    async initialize(): Promise<void> {
        const config = ConfigManager.getInstance();
        const modelDir = process.env.NEMOTRON_MODEL_DIR || DEFAULT_MODEL_DIR;
        const language = process.env.NEMOTRON_LANGUAGE || "vi";

        // Resolve absolute model path
        const absoluteModelDir = path.isAbsolute(modelDir)
            ? modelDir
            : path.resolve(process.cwd(), modelDir);

        return new Promise((resolve, reject) => {
            const workerPath = path.join(_dirname, "..", "workers", "NemotronWorker.ts");

            if (process.env.NODE_ENV === "production") {
                const prodWorkerPath = workerPath.replace(/\.ts$/, ".js");
                this.#worker = new Worker(prodWorkerPath);
            } else {
                const workerUrl = pathToFileURL(workerPath).href;
                this.#worker = new Worker(
                    `
                    import { register } from 'node:module';
                    import { pathToFileURL } from 'node:url';
                    register('tsx', pathToFileURL('./'), { data: {} });
                    import('${workerUrl.replace(/\\/g, "\\\\")}');
                    `,
                    {
                        eval: true,
                        execArgv: []
                    }
                );
            }

            const timeout = setTimeout(() => {
                reject(new Error("NemotronWorker initialization timed out (30s)"));
            }, 30000);

            this.#worker.on("message", (msg: { type: string; text?: string; message?: string }) => {
                switch (msg.type) {
                    case "ready":
                        this.#isReady = true;
                        clearTimeout(timeout);
                        logger.info("[NemotronSTT] ✅ Nemotron ASR ready (Hybrid CPU/GPU via DirectML)");
                        this.#startWatchdog();
                        resolve();
                        break;

                    case "partial":
                        this.#onPartialTranscription(msg.text ?? "");
                        break;

                    case "final":
                        this.#onFinalTranscription(msg.text ?? "");
                        break;

                    case "pong":
                        this.#lastPongTime = Date.now();
                        break;

                    case "log":
                        logger.debug(`[NemotronSTT] ${msg.message}`);
                        break;

                    case "error":
                        logger.error(`[NemotronSTT] ❌ Worker error: ${msg.message}`);
                        this.#recordFailure();
                        if (!this.#isReady) {
                            clearTimeout(timeout);
                            reject(new Error(msg.message));
                        }
                        break;
                }
            });

            this.#worker.on("error", (err: Error) => {
                logger.error(`[NemotronSTT] ❌ Worker crashed: ${err.message}`);
                const wasReady = this.#isReady;
                this.#isReady = false;
                this.#attemptRecovery();
                if (!wasReady) {
                    clearTimeout(timeout);
                    reject(err);
                }
            });

            this.#worker.on("exit", (code) => {
                if (code !== 0) {
                    logger.warn(`[NemotronSTT] Worker exited with code ${code}`);
                }
                const wasReady = this.#isReady;
                this.#isReady = false;
                if (!wasReady) {
                    clearTimeout(timeout);
                    reject(new Error(`Worker exited with code ${code}`));
                }
            });

            // Send init command to worker
            this.#worker.postMessage({
                type: "init",
                modelDir: absoluteModelDir,
                language
            });
        });
    }

    // ═══════════════════════════════════════════════════════
    //  Circuit Breaker (same pattern as WhisperNode v25)
    // ═══════════════════════════════════════════════════════

    #recordFailure(): void {
        this.#consecutiveFailures++;
        if (this.#consecutiveFailures >= CIRCUIT_THRESHOLD) {
            const wasOpen = this.#isCircuitOpen;
            this.#isCircuitOpen = true;
            logger.error(`[NemotronSTT] CIRCUIT OPEN — Too many failures (${this.#consecutiveFailures}). Blocking for ${CIRCUIT_RESET_MS / 1000}s.`);

            if (!wasOpen) {
                this.emit("stt_fallback_activated");
            }

            if (this.#circuitTimer) clearTimeout(this.#circuitTimer);
            this.#circuitTimer = setTimeout(() => {
                this.#isCircuitOpen = false;
                this.#consecutiveFailures = 0;
                this.#circuitTimer = null;
                logger.info(`[NemotronSTT] Circuit reset — STT requests resumed.`);
                this.emit("stt_fallback_deactivated");
            }, CIRCUIT_RESET_MS);
        }
    }

    #recordSuccess(): void {
        this.#consecutiveFailures = 0;
    }

    public isCircuitOpen(): boolean {
        return this.#isCircuitOpen;
    }

    // ═══════════════════════════════════════════════════════
    //  STT Pipeline — Backward-Compatible Public API
    // ═══════════════════════════════════════════════════════

    /**
     * Push audio chunk WITH silence timer (legacy fallback path).
     * Used when VADWorkerBridge is not active.
     * @param chunk Raw PCM Float32Array from frontend
     */
    public pushAudioChunk(chunk: Float32Array): void {
        this.#accumulateBuffer(chunk);

        // Also forward to worker for streaming inference
        this.#forwardToWorker(chunk, false);

        // Reset silence timer
        if (this.#silenceTimer) {
            clearTimeout(this.#silenceTimer);
        }
        this.#silenceTimer = setTimeout(() => {
            this.#silenceTimer = null;
            this.triggerTranscription();
        }, this.VAD_SILENCE_MS);
    }

    /**
     * Push audio chunk WITHOUT silence timer.
     * Used when VADWorkerBridge is active — VAD controls transcription timing.
     * DIFFERENCE FROM WhisperNode: Also forwards to worker for streaming inference.
     * @param chunk Raw PCM Float32Array from frontend
     */
    public pushAudioChunkOnly(chunk: Float32Array): void {
        this.#accumulateBuffer(chunk);

        // [v31] Stream to worker for real-time partial transcription
        // WhisperNode only accumulated here — Nemotron starts inference immediately
        this.#forwardToWorker(chunk, false);
    }

    /**
     * Trigger final transcription — called by VADWorkerBridge on speech_end.
     * Sends the last chunk with isLast=true to finalize RNNT decoding.
     */
    public triggerTranscription(): void {
        if (this.#silenceTimer) {
            clearTimeout(this.#silenceTimer);
            this.#silenceTimer = null;
        }

        if (!this.#isStreaming) {
            // No audio was streamed — nothing to finalize
            return;
        }

        // Send empty final chunk to signal end-of-utterance
        if (this.#worker && this.#isReady) {
            const emptyChunk = new Float32Array(0);
            this.#worker.postMessage(
                { type: "audio_chunk", buffer: emptyChunk, isLast: true },
                [emptyChunk.buffer]
            );
        }

        // Clear the accumulation buffer
        this.#audioBuffer = [];
        this.#totalBufferBytes = 0;
    }

    /**
     * Emergency flush — called during Barge-in to discard all pending audio.
     */
    public flush(): void {
        this.#audioBuffer = [];
        this.#totalBufferBytes = 0;
        this.#lastPartialText = "";
        this.#isStreaming = false;

        if (this.#silenceTimer) {
            clearTimeout(this.#silenceTimer);
            this.#silenceTimer = null;
        }

        // Reset worker state for new utterance
        if (this.#worker && this.#isReady) {
            this.#worker.postMessage({ type: "reset" });
        }

        logger.debug(`[NemotronSTT] Buffer flushed due to Preemption.`);
    }

    /**
     * Full cleanup — release worker thread and all resources.
     */
    public destroy(): void {
        this.#isDestroyed = true;
        logger.info(`[NemotronSTT] Disposing STT engine...`);
        this.flush();

        // Stop watchdog
        this.#stopWatchdog();

        // Cancel recovery timer
        if (this.#recoveryTimer) {
            clearTimeout(this.#recoveryTimer);
            this.#recoveryTimer = null;
        }

        // Clear circuit breaker
        if (this.#circuitTimer) {
            clearTimeout(this.#circuitTimer);
            this.#circuitTimer = null;
        }
        this.#isCircuitOpen = false;
        this.#consecutiveFailures = 0;

        // Terminate worker
        if (this.#worker) {
            this.#worker.postMessage({ type: "dispose" });
            this.#worker.terminate().catch(() => {});
            this.#worker = null;
        }

        this.#isReady = false;
        this.#crashCount = 0;
        this.removeAllListeners();
    }

    // ═══════════════════════════════════════════════════════
    //  Internal — Audio Forwarding & Transcription Events
    // ═══════════════════════════════════════════════════════

    /**
     * Accumulate audio in buffer with cap (same as WhisperNode).
     */
    #accumulateBuffer(chunk: Float32Array): void {
        this.#audioBuffer.push(chunk);
        this.#totalBufferBytes += chunk.byteLength;

        // Drop oldest chunks when buffer exceeds 60s cap
        while (this.#totalBufferBytes > MAX_AUDIO_BUFFER_BYTES && this.#audioBuffer.length > 1) {
            const dropped = this.#audioBuffer.shift()!;
            this.#totalBufferBytes -= dropped.byteLength;
        }
    }

    /**
     * Forward audio chunk to NemotronWorker for streaming inference.
     */
    #forwardToWorker(chunk: Float32Array, isLast: boolean): void {
        if (!this.#worker || !this.#isReady || this.#isCircuitOpen) return;

        // Minimum audio threshold (same as WhisperNode: 4096 bytes)
        if (chunk.byteLength < 1024 && !isLast) return;

        if (chunk.length === 0 && !isLast) return;

        // Transfer to worker (zero-copy via transferable)
        // Since we are also passing the same Float32Array to VADWorker, copy it to avoid clearing the original buffer.
        const transferableChunk = new Float32Array(chunk);
        this.#worker.postMessage(
            { type: "audio_chunk", buffer: transferableChunk, isLast },
            [transferableChunk.buffer]
        );

        this.#isStreaming = true;
    }

    /**
     * Handle partial transcription from worker — emit for Speculative RAG Warming.
     */
    #onPartialTranscription(text: string): void {
        if (!text || text === this.#lastPartialText) return;

        this.#lastPartialText = text;
        this.#recordSuccess();

        // Emit for CoreKernel.#onTranscriptionPartial → speculativeWarm()
        this.emit("transcription_partial", text);
    }

    /**
     * Handle final transcription from worker — emit for BackchannelDetector + AgentLoop.
     */
    #onFinalTranscription(text: string): void {
        this.#isStreaming = false;
        this.#lastPartialText = "";
        this.#recordSuccess();

        // CRITICAL: Reset worker state for next utterance
        // Without this, encoder caches and decoder LSTM state persist
        // across utterances, causing blank-only output after first segment
        if (this.#worker && this.#isReady) {
            this.#worker.postMessage({ type: "reset" });
        }

        if (!text || !text.trim()) {
            logger.debug("[NemotronSTT] Empty final transcription — skipping.");
            return;
        }

        logger.info(`[NemotronSTT] Nhận dạng Giọng Nói: "${text}"`);
        this.emit("transcription_ready", text.trim());
    }

    // ═══════════════════════════════════════════════════════
    //  Watchdog Heartbeat (same pattern as VADWorkerBridge)
    // ═══════════════════════════════════════════════════════

    #startWatchdog(): void {
        this.#stopWatchdog();
        this.#lastPongTime = Date.now();

        this.#watchdogInterval = setInterval(() => {
            if (!this.#worker || !this.#isReady) return;

            this.#worker.postMessage({ type: "ping" });

            const silenceMs = Date.now() - this.#lastPongTime;
            if (silenceMs > WATCHDOG_TIMEOUT_MS) {
                logger.error(`[NemotronSTT] 🏥 WATCHDOG: No PONG for ${silenceMs}ms — silent deadlock! Terminating worker...`);
                this.#isReady = false;
                this.#stopWatchdog();

                if (this.#worker) {
                    this.#worker.terminate().catch(() => {});
                    this.#worker = null;
                }

                this.#attemptRecovery();
            }
        }, WATCHDOG_PING_MS);
        this.#watchdogInterval.unref();
    }

    #stopWatchdog(): void {
        if (this.#watchdogInterval) {
            clearInterval(this.#watchdogInterval);
            this.#watchdogInterval = null;
        }
    }

    // ═══════════════════════════════════════════════════════
    //  Auto-Recovery (same pattern as VADWorkerBridge v25)
    // ═══════════════════════════════════════════════════════

    #attemptRecovery(): void {
        if (this.#crashCount >= MAX_CRASH_RECOVERY) {
            logger.error(`[NemotronSTT] 🛑 Max recovery attempts (${MAX_CRASH_RECOVERY}) exceeded — STT permanently disabled.`);
            this.emit("stt_fallback_activated");
            return;
        }
        this.#crashCount++;
        const delay = Math.min(1000 * Math.pow(2, this.#crashCount), 30_000);
        logger.warn(`[NemotronSTT] 🔄 Recovery attempt ${this.#crashCount}/${MAX_CRASH_RECOVERY} in ${delay}ms`);

        if (this.#recoveryTimer) clearTimeout(this.#recoveryTimer);
        this.#recoveryTimer = setTimeout(async () => {
            this.#recoveryTimer = null;
            if (this.#isDestroyed) return;
            try {
                if (this.#worker) {
                    try { await this.#worker.terminate(); } catch { /* already dead */ }
                    this.#worker = null;
                }
                await this.initialize();
                this.#crashCount = 0;
                logger.info("[NemotronSTT] ✅ Worker recovered successfully.");
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[NemotronSTT] Recovery attempt ${this.#crashCount} failed: ${msg}`);
            }
        }, delay);
        this.#recoveryTimer.unref();
    }
}
