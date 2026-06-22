import { logger } from "../utils/logger";
import { Worker } from "node:worker_threads";
import * as path from "node:path";

export class FlashRankService {
    private static instance: FlashRankService;

    private initPromise: Promise<void> | null = null;
    private isReady = false;
    private worker: Worker | null = null;
    private pendingRequests = new Map<string, { resolve: (val: unknown) => void, reject: (err: unknown) => void, timer?: NodeJS.Timeout }>();
    private requestCounter = 0;

    private constructor() {}

    public static getInstance(): FlashRankService {
        if (!FlashRankService.instance) {
            FlashRankService.instance = new FlashRankService();
        }
        return FlashRankService.instance;
    }

    public async initialize(): Promise<void> {
        if (this.isReady) return;
        if (!this.initPromise) {
            this.initPromise = this._initWorker();
        }
        return this.initPromise;
    }

    private async _initWorker(): Promise<void> {
        return new Promise((resolve, reject) => {
            try {
                logger.info("[FlashRankService] Starting FlashRank CPU Reranker Worker...");
                const currentDir = import.meta.dirname || __dirname;
                const workerPath = path.join(currentDir, "..", "workers", "FlashRankWorker.ts");

                this.worker = new Worker(`
                    require('tsx/cjs');
                    require(${JSON.stringify(workerPath)});
                `, { eval: true });

                this.worker.on("message", (msg) => this._handleWorkerMessage(msg, resolve, reject));

                this.worker.on("error", (err: unknown) => {
                    const e = err instanceof Error ? err : new Error(String(err));
                    logger.error(`[FlashRankService] Worker error: ${e.message}`);
                    this.isReady = false;
                    this.initPromise = null;
                    reject(new Error("Worker error: " + e.message));
                });

                this.worker.on("exit", (code) => {
                    logger.warn(`[FlashRankService] Worker exited with code ${code}`);
                    this.isReady = false;
                    this.initPromise = null;
                });

                this.worker.postMessage({ type: "init" });
            } catch (err: unknown) {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.error(`[FlashRankService] Worker spawn failed: ${errMsg}`);
                this.initPromise = null;
                reject(err);
            }
        });
    }

    private _handleWorkerMessage(
        msg: { type: string; id?: string; message?: string; reranked?: unknown; mode?: string },
        resolveInit: () => void,
        rejectInit: (err: unknown) => void
    ) {
        if (msg.type === "ready") {
            this.isReady = true;
            logger.info(`[FlashRankService] ✅ FlashRank Worker ready (mode: ${msg.mode}).`);
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
            
            if (msg.type === "result") {
                req.resolve(msg.reranked);
            } else if (msg.type === "error") {
                req.reject(new Error(msg.message));
            }
            this.pendingRequests.delete(msg.id);
        }
    }

    public async rerank(query: string, documents: Array<string | Record<string, unknown>>): Promise<Array<Record<string, unknown> & { score: number }>> {
        if (documents.length === 0) return [];
        
        await this.initialize();

        if (!this.worker) {
            throw new Error("FlashRank worker is not running.");
        }

        const id = `req_${++this.requestCounter}_${Date.now()}`;
        
        return new Promise((resolve, reject) => {
            const timer = setTimeout(() => {
                this.pendingRequests.delete(id);
                reject(new Error(`FlashRank rerank timeout for request ${id}`));
            }, 10000); // 10s timeout

            this.pendingRequests.set(id, { 
                resolve: resolve as (val: unknown) => void, 
                reject: reject as (err: unknown) => void, 
                timer 
            });

            // Post only required fields to minimize serialization overhead
            const workerDocs = documents.map((doc) => {
                const content = typeof doc === "string" ? doc : (doc.content as string) || "";
                return { content };
            });

            this.worker!.postMessage({
                type: "rerank",
                id,
                query,
                documents: workerDocs
            });
        }).then((rerankedVal: unknown) => {
            const reranked = rerankedVal as Array<{ index: number; score: number }>;
            // Sort original documents by the returned score descending.
            // Map scores back using returned index.
            const scoredDocs = documents.map((doc, idx) => {
                const resultItem = reranked.find((r) => r.index === idx);
                const score = resultItem ? resultItem.score : 0.0;
                // Preserve original document structure and attach score
                if (typeof doc === "string") {
                    return { content: doc, score };
                } else {
                    return { ...doc, score };
                }
            });

            // Sort by score descending
            scoredDocs.sort((a, b) => b.score - a.score);
            return scoredDocs;
        });
    }

    public async dispose(): Promise<void> {
        if (this.worker) {
            this.worker.postMessage({ type: "dispose" });
            this.worker = null;
        }
        this.isReady = false;
        this.initPromise = null;
        for (const req of this.pendingRequests.values()) {
            if (req.timer) clearTimeout(req.timer);
            req.reject(new Error("FlashRankService disposed"));
        }
        this.pendingRequests.clear();
        logger.info("[FlashRankService] 🧹 Disposed. CPU Worker cleanup complete.");
    }
}
