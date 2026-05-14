/**
 * VADWorkerBridge — Main-thread Bridge to VADWorker (Worker Thread)
 * =================================================================
 * [v22 Full-Duplex Pillar 1]
 *
 * Provides an EventEmitter-based API for the main thread to interact
 * with the neural VAD running in a separate worker_thread.
 *
 * Events emitted:
 *   - "speech_start"  → User began speaking (transition from silence to speech)
 *   - "speech_end"    → User stopped speaking (transition from speech to silence)
 *   - "ready"         → VAD model loaded successfully
 *   - "error"         → Initialization or inference error
 *
 * Speech state machine (debounced):
 *   SILENCE → 3 consecutive speech frames → emit "speech_start" → SPEAKING
 *   SPEAKING → 8 consecutive silence frames → emit "speech_end" → SILENCE
 *
 * This debouncing prevents:
 *   - False starts from brief noise spikes
 *   - Premature end-of-speech from short pauses within a sentence
 */

import { Worker } from "node:worker_threads";
import { EventEmitter } from "node:events";
import * as path from "node:path";
import { fileURLToPath } from "node:url";
import { logger } from "../utils/logger";

// ESM-first: Node.js 22+ supports import.meta.dirname natively
// SEA fallback: esbuild CJS bundle provides __dirname
const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

/** Number of consecutive speech frames needed to trigger speech_start */
const SPEECH_START_THRESHOLD = 3;

/** Number of consecutive silence frames needed to trigger speech_end */
const SPEECH_END_THRESHOLD = 8;

/** Max automatic recovery attempts before permanently disabling VAD */
const MAX_CRASH_RECOVERY = 3;

/** v25 Watchdog: Ping interval (5 seconds) */
const WATCHDOG_PING_MS = 5_000;

/** v25 Watchdog: Max time to wait for PONG before declaring deadlock (15 seconds) */
const WATCHDOG_TIMEOUT_MS = 15_000;

export class VADWorkerBridge extends EventEmitter {
    #worker: Worker | null = null;
    #isReady = false;
    #isSpeaking = false;
    #consecutiveSpeechFrames = 0;
    #consecutiveSilenceFrames = 0;
    #crashCount = 0;
    #lastModelPath = "";
    #recoveryTimer: ReturnType<typeof setTimeout> | null = null;
    // v25 Watchdog Heartbeat
    #watchdogInterval: ReturnType<typeof setInterval> | null = null;
    #lastPongTime = 0;

    /**
     * Initialize the VAD worker thread and load the Silero model.
     * @param modelPath Absolute path to the Silero ONNX model file
     */
    async initialize(modelPath: string): Promise<void> {
        this.#lastModelPath = modelPath;
        return new Promise((resolve, reject) => {
            const workerPath = path.join(_dirname, "..", "workers", "VADWorker.ts");

            this.#worker = new Worker(workerPath, {
                // Use ts-node/esm loader for TypeScript support in development
                // In production (bundled), the .ts is compiled to .js
                execArgv: process.env.NODE_ENV === "production" ? [] : ["--loader", "tsx"],
            });

            const timeout = setTimeout(() => {
                reject(new Error("VAD worker initialization timed out (10s)"));
            }, 10000);

            this.#worker.on("message", (msg: { type: string; isSpeech?: boolean; confidence?: number; message?: string }) => {
                switch (msg.type) {
                    case "ready":
                        this.#isReady = true;
                        clearTimeout(timeout);
                        logger.info("[VADWorkerBridge] ✅ Neural VAD ready (Worker Thread)");
                        this.emit("ready");
                        resolve();
                        break;

                    case "vad_result":
                        this.#processVADResult(msg.isSpeech!, msg.confidence!);
                        break;

                    case "pong":
                        // v25 Watchdog: Worker responded to ping — it's alive
                        this.#lastPongTime = Date.now();
                        break;

                    case "error":
                        logger.error(`[VADWorkerBridge] ❌ Worker error: ${msg.message}`);
                        this.emit("error", msg.message);
                        if (!this.#isReady) {
                            clearTimeout(timeout);
                            reject(new Error(msg.message));
                        }
                        break;
                }
            });

            // v25: Start watchdog after initialization completes
            this.#startWatchdog();

            this.#worker.on("error", (err: Error) => {
                logger.error(`[VADWorkerBridge] ❌ Worker crashed: ${err.message}`);
                this.#isReady = false;
                this.#attemptRecovery();
            });

            this.#worker.on("exit", (code) => {
                if (code !== 0) {
                    logger.warn(`[VADWorkerBridge] Worker exited with code ${code}`);
                }
                this.#isReady = false;
            });

            // Send init command to worker
            this.#worker.postMessage({ type: "init", modelPath });
        });
    }

    /**
     * Send raw audio samples to the VAD worker for analysis.
     * Uses transferable ArrayBuffer for zero-copy transfer.
     * @param samples Float32Array of 16kHz mono PCM samples
     */
    pushAudioSamples(samples: Float32Array): void {
        if (!this.#isReady || !this.#worker) return;

        // Create a copy for transfer (original stays in caller's scope)
        const copy = new Float32Array(samples);
        this.#worker.postMessage(
            { type: "audio", buffer: copy },
            [copy.buffer]  // Transfer ownership — zero-copy
        );
    }

    /**
     * Process VAD result with debounced state machine.
     * Prevents jitter by requiring consecutive frames to confirm state change.
     */
    #processVADResult(isSpeech: boolean, _confidence: number): void {
        if (isSpeech) {
            this.#consecutiveSpeechFrames++;
            this.#consecutiveSilenceFrames = 0;

            if (!this.#isSpeaking && this.#consecutiveSpeechFrames >= SPEECH_START_THRESHOLD) {
                this.#isSpeaking = true;
                logger.debug("[VAD] 🎙️ SPEECH_START detected");
                this.emit("speech_start");
            }
        } else {
            this.#consecutiveSilenceFrames++;
            this.#consecutiveSpeechFrames = 0;

            if (this.#isSpeaking && this.#consecutiveSilenceFrames >= SPEECH_END_THRESHOLD) {
                this.#isSpeaking = false;
                logger.debug("[VAD] 🔇 SPEECH_END detected");
                this.emit("speech_end");
            }
        }
    }

    get isReady(): boolean {
        return this.#isReady;
    }

    get isSpeaking(): boolean {
        return this.#isSpeaking;
    }

    /**
     * v25 Watchdog Heartbeat — detect silent ONNX/WASM deadlocks.
     * Sends PING every 5s. If no PONG within 15s → worker is frozen → terminate & recover.
     */
    #startWatchdog(): void {
        this.#stopWatchdog();
        this.#lastPongTime = Date.now();

        this.#watchdogInterval = setInterval(() => {
            if (!this.#worker || !this.#isReady) return;

            // Send PING
            this.#worker.postMessage({ type: "ping" });

            // Check if last PONG was within deadline
            const silenceMs = Date.now() - this.#lastPongTime;
            if (silenceMs > WATCHDOG_TIMEOUT_MS) {
                logger.error(`[VADWorkerBridge] 🏥 WATCHDOG: No PONG for ${silenceMs}ms — silent deadlock detected! Terminating worker...`);
                this.#isReady = false;
                this.#stopWatchdog();

                // Force kill the frozen worker
                if (this.#worker) {
                    this.#worker.terminate().catch(() => {});
                    this.#worker = null;
                }

                // Trigger auto-recovery
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

    /**
     * v25 Hardening: Auto-recovery with exponential backoff.
     * Handles silent C++/WASM deadlocks (worker dies without "error" event).
     * Max 3 attempts: 1s → 2s → 4s, then permanently disabled.
     */
    #attemptRecovery(): void {
        if (this.#crashCount >= MAX_CRASH_RECOVERY) {
            logger.error(`[VADWorkerBridge] 🛑 Max recovery attempts (${MAX_CRASH_RECOVERY}) exceeded — VAD permanently disabled.`);
            this.emit("error", "VAD permanently disabled after repeated crashes");
            return;
        }
        this.#crashCount++;
        const delay = Math.min(1000 * Math.pow(2, this.#crashCount), 30_000);
        logger.warn(`[VADWorkerBridge] 🔄 Recovery attempt ${this.#crashCount}/${MAX_CRASH_RECOVERY} in ${delay}ms`);

        if (this.#recoveryTimer) clearTimeout(this.#recoveryTimer);
        this.#recoveryTimer = setTimeout(async () => {
            this.#recoveryTimer = null;
            try {
                // Ensure old worker is dead before spawning new one
                if (this.#worker) {
                    try { await this.#worker.terminate(); } catch { /* already dead */ }
                    this.#worker = null;
                }
                await this.initialize(this.#lastModelPath);
                this.#crashCount = 0; // Reset on successful recovery
                logger.info("[VADWorkerBridge] ✅ Worker recovered successfully.");
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[VADWorkerBridge] Recovery attempt ${this.#crashCount} failed: ${msg}`);
                // Next crash will trigger another recovery attempt
            }
        }, delay);
        this.#recoveryTimer.unref();
    }

    /**
     * Terminate the worker thread and release all resources.
     */
    async dispose(): Promise<void> {
        // Stop watchdog heartbeat
        this.#stopWatchdog();
        // Cancel any pending recovery
        if (this.#recoveryTimer) {
            clearTimeout(this.#recoveryTimer);
            this.#recoveryTimer = null;
        }
        if (this.#worker) {
            this.#worker.postMessage({ type: "dispose" });
            await this.#worker.terminate();
            this.#worker = null;
        }
        this.#isReady = false;
        this.#crashCount = 0;
        this.removeAllListeners();
    }
}
