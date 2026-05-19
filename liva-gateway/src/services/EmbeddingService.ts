import { logger } from "../utils/logger";
import { Worker } from "node:worker_threads";
import * as path from "node:path";
import * as url from "node:url";

const __filename = url.fileURLToPath(import.meta.url);
const __dirname = path.dirname(__filename);

export class EmbeddingNotReadyError extends Error {
    name = "EmbeddingNotReadyError";
}

export class EmbeddingService {
    private static instance: EmbeddingService;

    private initPromise: Promise<void> | null = null;
    private isReady = false;
    private worker: Worker | null = null;

    private pendingRequests = new Map<string, { resolve: (val: any) => void, reject: (err: any) => void, timer?: NodeJS.Timeout }>();
    private requestCounter = 0;

    private constructor() {}

    public static getInstance(): EmbeddingService {
        if (!EmbeddingService.instance) {
            EmbeddingService.instance = new EmbeddingService();
        }
        return EmbeddingService.instance;
    }

    public async ensureReady(): Promise<void> {
        if (this.isReady) return;
        if (!this.initPromise) {
            this.initPromise = this._initModel();
        }
        return this.initPromise;
    }

    private async _initModel(): Promise<void> {
        try {
            logger.info(`[EmbeddingService] 🧠 Starting ONNX CPU Embedding Worker (all-MiniLM-L6-v2, 384D)...`);
            
            return new Promise((resolve, reject) => {
                const workerPath = path.join(__dirname, "..", "workers", "EmbeddingWorker.ts");
                
                // Using tsx directly via eval since it's a TS file
                this.worker = new Worker(`
                    require('tsx/cjs');
                    require(${JSON.stringify(workerPath)});
                `, { eval: true });

                this.worker.on("message", (msg) => this._handleWorkerMessage(msg, resolve, reject));
                
                this.worker.on("error", (err: unknown) => {
                    const e = err instanceof Error ? err : new Error(String(err));
                    logger.error(`[EmbeddingService] Worker error: ${e.message}`);
                    this.isReady = false;
                    this.initPromise = null;
                    reject(new EmbeddingNotReadyError("Worker error: " + e.message));
                });
                
                this.worker.on("exit", (code) => {
                    logger.warn(`[EmbeddingService] Worker exited with code ${code}`);
                    this.isReady = false;
                    this.initPromise = null;
                });

                this.worker.postMessage({ type: "init" });
            });
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[EmbeddingService] ⏳ Engine not ready yet: ${errMsg} — will auto-retry on next call.`);
            this.initPromise = null;
            throw e;
        }
    }

    private _handleWorkerMessage(msg: any, resolveInit: () => void, rejectInit: (err: any) => void) {
        if (msg.type === "ready") {
            this.isReady = true;
            logger.info(`[EmbeddingService] ✅ ONNX CPU Embedding Worker ready. Zero VRAM overhead.`);
            resolveInit();
            return;
        }

        if (msg.type === "error" && !msg.id) {
            rejectInit(new Error(msg.message));
            return;
        }

        if (msg.id && this.pendingRequests.has(msg.id)) {
            const req = this.pendingRequests.get(msg.id)!;
            if (req.timer) clearTimeout(req.timer);
            
            if (msg.type === "embed_result") {
                req.resolve(msg.vector);
            } else if (msg.type === "embed_batch_result") {
                req.resolve(msg.vectors);
            } else if (msg.type === "error") {
                req.reject(new EmbeddingNotReadyError(msg.message));
            }
            this.pendingRequests.delete(msg.id);
        }
    }

    public async embed(text: string): Promise<number[]> {
        await this.ensureReady();
        if (!this.isReady || !this.worker) throw new EmbeddingNotReadyError("Embedding Worker unavailable.");

        return new Promise((resolve, reject) => {
            const id = `req_${++this.requestCounter}`;
            this.pendingRequests.set(id, { resolve, reject });
            this.worker!.postMessage({ type: "embed", id, text });
        });
    }

    public async embedWithTimeout(text: string, timeoutMs: number = 2000): Promise<number[]> {
        await this.ensureReady();
        if (!this.isReady || !this.worker) throw new EmbeddingNotReadyError("Embedding Worker unavailable.");

        return new Promise((resolve, reject) => {
            const id = `req_${++this.requestCounter}`;
            
            const timer = setTimeout(() => {
                if (this.pendingRequests.has(id)) {
                    this.pendingRequests.delete(id);
                    reject(new EmbeddingNotReadyError("Embedding timeout"));
                }
            }, timeoutMs);

            this.pendingRequests.set(id, { resolve, reject, timer });
            this.worker!.postMessage({ type: "embed", id, text });
        }).catch((e: unknown) => {
            const errMsg = e instanceof Error ? e.message : String(e);
            if (!errMsg.includes("Embedding timeout")) {
                logger.warn(`[EmbeddingService] Non-timeout error: ${errMsg}`);
            }
            throw new EmbeddingNotReadyError("Embedding Worker unavailable or timeout.");
        }) as Promise<number[]>;
    }

    public async embedBatch(texts: string[]): Promise<Array<number[]>> {
        await this.ensureReady();
        if (!this.isReady || !this.worker || texts.length === 0) {
            throw new EmbeddingNotReadyError("Embedding Worker unavailable.");
        }

        return new Promise((resolve, reject) => {
            const id = `req_${++this.requestCounter}`;
            this.pendingRequests.set(id, { resolve, reject });
            this.worker!.postMessage({ type: "embed_batch", id, texts });
        });
    }

    // MRL is not supported by all-MiniLM-L6-v2, return vector unchanged
    public truncateMatryoshka(vector: number[], _targetDim: number = 256): number[] {
        return vector;
    }

    public truncateMatryoshkaFloat32(vector: Float32Array, _targetDim: number = 256): Float32Array {
        return vector;
    }

    public async embedTruncated(text: string, _targetDim: number): Promise<number[]> {
        return this.embed(text);
    }

    public get ready(): boolean {
        return this.isReady;
    }

    public get dimension(): number {
        return 384;
    }

    public get modelId(): string {
        return "onnx-cpu-worker";
    }

    public get supportsMRL(): boolean {
        return false;
    }

    public getDummyVector(): number[] {
        return new Array(this.dimension).fill(0.01);
    }

    public dispose(): void {
        if (this.worker) {
            this.worker.postMessage({ type: "dispose" });
            this.worker = null;
        }
        this.initPromise = null;
        this.isReady = false;
        
        for (const req of this.pendingRequests.values()) {
            if (req.timer) clearTimeout(req.timer);
            req.reject(new Error("EmbeddingService disposed"));
        }
        this.pendingRequests.clear();
        logger.info("[EmbeddingService] 🧹 Disposed. CPU Worker cleanup complete.");
    }
}
