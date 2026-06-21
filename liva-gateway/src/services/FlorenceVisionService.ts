/**
 * FlorenceVisionService — Main-thread Bridge to FlorenceWorker (Worker Thread)
 * =========================================================================
 *
 * Spawns the FlorenceWorker thread and provides a clean asynchronous API
 * for initialization, image processing, and resource cleanup.
 */

import { Worker } from "node:worker_threads";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { randomUUID } from "node:crypto";
import { logger } from "../utils/logger";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

export interface FlorenceResult {
    text: string;
    coordinates: number[][];
}

interface PendingRequest {
    resolve: (value: FlorenceResult) => void;
    reject: (reason: Error) => void;
}

export class FlorenceVisionService {
    #worker: Worker | null = null;
    #isInitialized = false;
    #pendingRequests = new Map<string, PendingRequest>();

    /**
     * Initializes the Florence worker thread and loads the ONNX model.
     * Resolves when the worker thread returns a "ready" message.
     * @param modelPath Absolute path to the Florence-2 ONNX model file
     */
    async initialize(modelPath: string): Promise<void> {
        if (this.#isInitialized) {
            return;
        }

        return new Promise<void>((resolve, reject) => {
            const workerPath = path.join(_dirname, "..", "workers", "FlorenceWorker.ts");

            try {
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

                // Setup 15-second initialization timeout guard
                const timeout = setTimeout(() => {
                    this.#cleanupWorker();
                    reject(new Error("Florence worker initialization timed out (15s)"));
                }, 15000);

                this.#worker.on("message", (msg: { type: string; id?: string; text?: string; coordinates?: number[][]; message?: string }) => {
                    if (!msg || typeof msg !== "object") return;

                    switch (msg.type) {
                        case "ready":
                            clearTimeout(timeout);
                            this.#isInitialized = true;
                            logger.info("[FlorenceVisionService] ✅ Florence-2 worker thread initialized and ready");
                            resolve();
                            break;

                        case "result":
                            if (msg.id) {
                                const request = this.#pendingRequests.get(msg.id);
                                if (request) {
                                    request.resolve({
                                        text: msg.text || "",
                                        coordinates: msg.coordinates || []
                                    });
                                    this.#pendingRequests.delete(msg.id);
                                }
                            }
                            break;

                        case "error":
                            if (msg.id) {
                                const request = this.#pendingRequests.get(msg.id);
                                if (request) {
                                    request.reject(new Error(msg.message || "Unknown worker error"));
                                    this.#pendingRequests.delete(msg.id);
                                }
                            } else {
                                logger.error(`[FlorenceVisionService] ❌ Worker error: ${msg.message}`);
                                if (!this.#isInitialized) {
                                    clearTimeout(timeout);
                                    reject(new Error(msg.message || "Initialization failed"));
                                }
                            }
                            break;
                    }
                });

                this.#worker.on("error", (err: Error) => {
                    logger.error(`[FlorenceVisionService] ❌ Worker crashed: ${err.message}`);
                    this.#handleWorkerCrash(err);
                });

                this.#worker.on("exit", (code: number) => {
                    if (code !== 0) {
                        logger.warn(`[FlorenceVisionService] Worker thread exited with code ${code}`);
                    }
                    this.#handleWorkerCrash(new Error(`Worker thread exited with code ${code}`));
                });

                // Post initialization command to worker
                this.#worker.postMessage({ type: "init", modelPath });

            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                reject(new Error(`Failed to spawn Florence worker: ${msg}`));
            }
        });
    }

    /**
     * Submits an image buffer to the worker for inference.
     * Resolves when the result is returned from the worker.
     */
    async processImage(buffer: Buffer): Promise<FlorenceResult> {
        if (!this.#isInitialized || !this.#worker) {
            throw new Error("FlorenceVisionService is not initialized. Call initialize() first.");
        }

        const id = randomUUID();
        return new Promise<FlorenceResult>((resolve, reject) => {
            this.#pendingRequests.set(id, { resolve, reject });
            this.#worker!.postMessage({
                type: "process",
                id,
                buffer
            });
        });
    }

    /**
     * Gracefully terminates the worker thread and cleans up all pending promises.
     */
    async dispose(): Promise<void> {
        this.#cleanupWorker();
    }

    #cleanupWorker(): void {
        this.#isInitialized = false;

        // Reject all pending requests
        for (const [id, request] of this.#pendingRequests.entries()) {
            request.reject(new Error("FlorenceVisionService was disposed"));
            this.#pendingRequests.delete(id);
        }

        if (this.#worker) {
            this.#worker.postMessage({ type: "dispose" });
            this.#worker.terminate().catch(() => {});
            this.#worker = null;
        }
    }

    #handleWorkerCrash(error: Error): void {
        this.#isInitialized = false;

        // Reject all pending requests with the crash error
        for (const [id, request] of this.#pendingRequests.entries()) {
            request.reject(new Error(`FlorenceWorker crashed: ${error.message}`));
            this.#pendingRequests.delete(id);
        }
    }
}
