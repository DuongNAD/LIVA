/**
 * MoonshineSTTService — Streaming STT Service
 * =================================================================
 * [v31 Pillar 4: Native Streaming STT]
 *
 * Main-thread EventEmitter bridge to MoonshineWorker (worker_thread).
 */

import { Worker } from "node:worker_threads";
import { EventEmitter } from "node:events";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { logger } from "../utils/logger";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));
const DEFAULT_MODEL_DIR = "./models/moonshine";

export class MoonshineSTTService extends EventEmitter {
    #worker: Worker | null = null;
    #isReady = false;
    #pendingFinalResolve: ((text: string) => void) | null = null;

    constructor() {
        super();
        logger.info("[MoonshineSTT] Khởi tạo Moonshine STT Service.");
    }

    /**
     * Initialize the Moonshine worker thread.
     */
    async initialize(): Promise<void> {
        const modelDir = process.env.MOONSHINE_MODEL_DIR || DEFAULT_MODEL_DIR;
        const absoluteModelDir = path.isAbsolute(modelDir)
            ? modelDir
            : path.resolve(process.cwd(), modelDir);

        return new Promise<void>((resolve, reject) => {
            const workerPath = path.join(_dirname, "..", "workers", "MoonshineWorker.ts");

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
                reject(new Error("MoonshineWorker initialization timed out (30s)"));
            }, 30000);

            this.#worker.on("message", (msg: { type: string; text?: string; message?: string }) => {
                switch (msg.type) {
                    case "ready":
                        this.#isReady = true;
                        clearTimeout(timeout);
                        logger.info("[MoonshineSTT] ✅ Moonshine Worker ready.");
                        resolve();
                        break;

                    case "partial":
                        this.emit("transcription_partial", msg.text ?? "");
                        break;

                    case "final":
                        this.emit("transcription_ready", msg.text ?? "");
                        if (this.#pendingFinalResolve) {
                            this.#pendingFinalResolve(msg.text ?? "");
                            this.#pendingFinalResolve = null;
                        }
                        break;

                    case "log":
                        logger.debug(`[MoonshineSTT Worker] ${msg.message}`);
                        break;

                    case "error":
                        logger.error(`[MoonshineSTT] Worker error: ${msg.message}`);
                        if (!this.#isReady) {
                            clearTimeout(timeout);
                            reject(new Error(msg.message));
                        }
                        break;
                }
            });

            this.#worker.on("error", (err: Error) => {
                logger.error(`[MoonshineSTT] Worker crashed: ${err.message}`);
                this.#isReady = false;
            });

            this.#worker.on("exit", (code) => {
                if (code !== 0) {
                    logger.warn(`[MoonshineSTT] Worker exited with code ${code}`);
                }
                this.#isReady = false;
            });

            this.#worker.postMessage({
                type: "init",
                modelDir: absoluteModelDir
            });
        });
    }

    /**
     * Push audio chunk WITHOUT silence timer.
     * @param chunk Raw PCM Buffer from frontend representing mono 16kHz Float32 PCM.
     */
    public pushAudioChunkOnly(chunk: Buffer): void {
        if (!this.#worker || !this.#isReady) {
            logger.warn("[MoonshineSTT] Worker not ready, dropping audio chunk.");
            return;
        }

        if (chunk.byteLength < 4) return;

        const numSamples = Math.floor(chunk.byteLength / 4);
        if (numSamples === 0) return;

        // Ensure aligned buffer allocation for Float32Array
        const aligned = new ArrayBuffer(numSamples * 4);
        new Uint8Array(aligned).set(new Uint8Array(chunk.buffer, chunk.byteOffset, numSamples * 4));
        const float32 = new Float32Array(aligned);

        this.#worker.postMessage(
            { type: "audio_chunk", buffer: float32, isLast: false },
            [float32.buffer]
        );
    }

    /**
     * Start streaming session.
     */
    public startStreaming(): void {
        if (this.#worker && this.#isReady) {
            this.#worker.postMessage({ type: "reset" });
        }
    }

    /**
     * Stop streaming session and get the final transcription.
     */
    public async stopStreaming(): Promise<string> {
        if (!this.#worker || !this.#isReady) {
            return "";
        }

        return new Promise<string>((resolve) => {
            this.#pendingFinalResolve = resolve;

            const emptyChunk = new Float32Array(0);
            this.#worker!.postMessage(
                { type: "audio_chunk", buffer: emptyChunk, isLast: true },
                [emptyChunk.buffer]
            );
        });
    }

    /**
     * Dispose the STT service and release the worker thread.
     */
    public dispose(): void {
        logger.info("[MoonshineSTT] Disposing Moonshine STT Service...");
        if (this.#worker) {
            this.#worker.postMessage({ type: "dispose" });
            this.#worker.terminate().catch(() => {});
            this.#worker = null;
        }
        this.#isReady = false;
        this.#pendingFinalResolve = null;
        this.removeAllListeners();
    }
}
