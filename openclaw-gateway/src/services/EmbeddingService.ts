import { logger } from "../utils/logger";
import { safeFetch } from "../utils/HttpClient";
import { FF } from "../utils/FeatureFlags";
import { NativeIPCClient } from "../utils/NativeIPCClient";
/**
 *   - Falls back to getDummyVector() on any error (never throws).
 *
 * Models (dimension determined by llama-server's loaded model):
 *   - nomic-embed-text-v1.5: 768D (MRL truncatable), 8192 ctx
 *   - all-MiniLM-L6-v2:      384D fixed, 512 ctx (legacy fallback)
 *
 * Matryoshka Representation Learning (MRL):
 *   nomic-embed-text-v1.5 supports truncation of output vectors to lower dimensions
 *   (768 → 512 → 256 → 128 → 64) with minimal quality loss. Use `truncateMatryoshka()`
 *   for storage-optimized embeddings. Truncation uses Float32Array hot-path math
 *   to avoid V8 GC pressure from Array.prototype.reduce().
 *
 * Dual-Path Architecture (v25):
 *   - LIVA_USE_NATIVE=true  → gRPC Embed RPC via NativeIPCClient (port 8100)
 *   - LIVA_USE_NATIVE=false → HTTP /v1/embeddings via safeFetch (port 8000)
 *
 * Usage:
 *   const service = EmbeddingService.getInstance();
 *   await service.ensureReady();
 *   const vector = await service.embed("Hello world");
 *   const truncated = service.truncateMatryoshka(vector, 256);
 *
 * @replaces @huggingface/transformers (BANNED per AI_CONTEXT §3)
 */

// ===========================
// Model Configuration
// ===========================

interface ModelConfig {
    dimension: number;
    contextLength: number;
    supportsMRL: boolean;
}

export class EmbeddingNotReadyError extends Error {
    name = "EmbeddingNotReadyError";
}

const MODEL_NOMIC: ModelConfig = {
    dimension: 768,
    contextLength: 8192,
    supportsMRL: true,
};

const MODEL_MINILM: ModelConfig = {
    dimension: 384,
    contextLength: 512,
    supportsMRL: false,
};

// ===========================
// Response Types (OpenAI-compatible /v1/embeddings)
// ===========================

interface EmbeddingResponseData {
    object: string;
    embedding: number[];
    index: number;
}

interface EmbeddingResponse {
    object: string;
    data: EmbeddingResponseData[];
    model: string;
    usage: { prompt_tokens: number; total_tokens: number };
}

// ===========================
// Main Class
// ===========================

export class EmbeddingService {
    private static instance: EmbeddingService;

    /** Promise Lock: prevents concurrent callers from racing during init */
    private initPromise: Promise<void> | null = null;
    private isReady = false;

    /** Active model configuration (resolved at init time from feature flag) */
    private activeConfig: ModelConfig = MODEL_MINILM;

    /** llama-server embedding API endpoint (HTTP path only) */
    private apiUrl: string = "";

    /** Whether to use NativeIPCClient gRPC for embeddings */
    private useNativeIPC = false;

    /** Lazy-initialized gRPC client (only created once, reused) */
    private nativeIpcClient: NativeIPCClient | null = null;

    private constructor() {}

    public static getInstance(): EmbeddingService {
        if (!EmbeddingService.instance) {
            EmbeddingService.instance = new EmbeddingService();
        }
        return EmbeddingService.instance;
    }



    /**
     * Ensure the embedding API is reachable.
     * Safe to call multiple times — Promise Lock guarantees single init.
     */
    public async ensureReady(): Promise<void> {
        if (this.isReady) return;
        if (!this.initPromise) {
            this.initPromise = this._initModel();
        }
        return this.initPromise;
    }

    private async _initModel(): Promise<void> {
        try {
            this.activeConfig = FF.isEnabled("NOMIC_EMBED") ? MODEL_NOMIC : MODEL_MINILM;

            const isNative = String(process.env.LIVA_USE_NATIVE).trim().toLowerCase() === "true";

            if (isNative) {
                // ═══════════════════════════════════════════════════
                // gRPC PATH: NativeIPCClient → Python Engine (port 8100)
                // Validates connectivity via HealthCheck RPC.
                // ═══════════════════════════════════════════════════
                logger.info(
                    `[EmbeddingService] 🧠 Native gRPC mode detected. Connecting to Engine (port 8100) for embeddings (${this.activeConfig.dimension}D)...`
                );

                this.nativeIpcClient = new NativeIPCClient();

                try {
                    const alive = await this.nativeIpcClient.healthCheck();
                    if (alive) {
                        this.useNativeIPC = true;
                        this.isReady = true;
                        logger.info(
                            `[EmbeddingService] ✅ Native gRPC Embed ready (${this.activeConfig.dimension}D). Shared context, zero VRAM overhead.`
                        );
                        return;
                    }
                    throw new Error("HealthCheck returned false");
                } catch {
                    this.initPromise = null;
                    throw new EmbeddingNotReadyError("Native gRPC engine not responding on port 8100.");
                }
            } else {
                // ═══════════════════════════════════════════════════
                // HTTP PATH: safeFetch → llama-server.exe (port 8000)
                // Legacy compatibility when LIVA_USE_NATIVE=false.
                // ═══════════════════════════════════════════════════
                this.apiUrl = process.env.LLM_ENDPOINT || `http://127.0.0.1:${process.env.LIVA_ROUTER_PORT || "8000"}/v1/embeddings`;

                logger.info(
                    `[EmbeddingService] 🧠 HTTP mode. Connecting to GPU Embedding API at ${this.apiUrl} (${this.activeConfig.dimension}D)...`
                );

                const healthUrl = process.env.LLM_ENDPOINT
                    ? process.env.LLM_ENDPOINT.replace("/embeddings", "/models")
                    : `http://127.0.0.1:${process.env.LIVA_ROUTER_PORT || "8000"}/v1/models`;

                try {
                    await safeFetch(healthUrl, {}, 5000);
                    this.isReady = true;
                    logger.info(
                        `[EmbeddingService] ✅ GPU Embedding API ready (${this.activeConfig.dimension}D). Zero CPU blocking.`
                    );
                } catch {
                    this.initPromise = null;
                    throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
                }
            }
        } catch (e: unknown) {
            // [v25 FIX] Silent Fallback — prevent Race Condition red errors at boot.
            // Python Engine is still loading model into VRAM (takes 2-5s).
            // DO NOT throw — reset initPromise so SemanticRouter's retry loop
            // can re-trigger ensureReady() after the engine comes online.
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[EmbeddingService] ⏳ Engine not ready yet: ${errMsg} — will auto-retry on next call.`);
            this.initPromise = null;
        }
    }

    // ===========================
    // Public API
    // ===========================

    /**
     * Generate embedding vector for input text.
     * Routes through gRPC (native) or HTTP (legacy) based on init mode.
     *
     * [v24 Guardrail] VRAMGuard interlock: if GPU is yielded to an external app
     * (game, renderer), throws EmbeddingNotReadyError immediately to prevent
     * sending gRPC calls to a sleeping engine.
     */
    public async embed(text: string): Promise<number[]> {
        await this.ensureReady();
        if (!this.isReady) throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");

        if (this.useNativeIPC && this.nativeIpcClient) {
            return this._embedViaNativeIPC(text);
        }

        return this._embedViaHTTP(text);
    }

    /**
     * gRPC embedding path — calls NativeIPCClient.embed() with 15s circuit breaker.
     * Vectors arrive L2-normalized from Python (zero CPU overhead on Node.js main thread).
     */
    private async _embedViaNativeIPC(text: string): Promise<number[]> {
        try {
            const response = await this.nativeIpcClient!.embed(text);

            if (response.data && response.data.length > 0 && response.data[0].embedding) {
                // Update dimension from engine response (auto-detect)
                if (response.dimensions > 0 && response.dimensions !== this.activeConfig.dimension) {
                    logger.info(`[EmbeddingService] Auto-detected dimension: ${response.dimensions}D (was ${this.activeConfig.dimension}D)`);
                    this.activeConfig = {
                        ...this.activeConfig,
                        dimension: response.dimensions,
                    };
                }
                return response.data[0].embedding;
            }

            throw new EmbeddingNotReadyError("gRPC Embed returned empty data.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error
                ? ((e as { cause?: { message?: string } }).cause?.message || e.message)
                : String(e);
            logger.warn(`[EmbeddingService] gRPC Embed failed: ${errMsg}`);
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        }
    }

    /**
     * HTTP embedding path — calls llama-server /v1/embeddings via safeFetch.
     * Used when LIVA_USE_NATIVE=false (legacy C++ llama-server mode).
     */
    private async _embedViaHTTP(text: string): Promise<number[]> {
        try {
            const res = await safeFetch(this.apiUrl, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ input: text }),
            }, 10000);

            const json: EmbeddingResponse = await res.json();

            if (json.data && json.data.length > 0 && json.data[0].embedding) {
                return json.data[0].embedding;
            }

            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error
                ? ((e as { cause?: { message?: string } }).cause?.message || e.message)
                : String(e);
            logger.warn(`[EmbeddingService] HTTP Embed failed: ${errMsg}`);
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        }
    }

    /**
     * Generate embedding with timeout guard (for hot-path usage).
     * Falls back to dummy vector if embedding takes longer than timeoutMs.
     */
    public async embedWithTimeout(text: string, timeoutMs: number = 2000): Promise<number[]> {
        await this.ensureReady();
        if (!this.isReady) throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");

        let timeoutId: NodeJS.Timeout;
        try {
            const embedPromise = this.embed(text);
            const timeoutPromise = new Promise<null>((_, reject) => {
                timeoutId = setTimeout(() => reject(new Error("Embedding timeout")), timeoutMs);
            });

            const output = await Promise.race([embedPromise, timeoutPromise]);
            if (output && Array.isArray(output)) return output;
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            if (!errMsg.includes("Embedding timeout")) {
                logger.warn(`[EmbeddingService] Non-timeout error: ${errMsg}`);
            }
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        } finally {
            clearTimeout(timeoutId!);
        }
    }

    /**
     * Batch embedding — generate vectors for multiple texts.
     * In native mode, sends full batch to Python for GPU-parallel computation.
     * ~3-5x faster than sequential embed() calls for ConsolidationCron anchor sets.
     *
     * @param texts    Array of strings to embed
     * @returns        Array of L2-normalized vectors
     */
    public async embedBatch(texts: string[]): Promise<Array<number[]>> {
        await this.ensureReady();
        if (!this.isReady || texts.length === 0) {
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        }

        if (this.useNativeIPC && this.nativeIpcClient) {
            return this._embedBatchViaNativeIPC(texts);
        }

        return this._embedBatchViaHTTP(texts);
    }

    /**
     * gRPC batch embedding path — sends entire array to Python in one RPC call.
     * Python processes batch with GPU and returns L2-normalized vectors.
     */
    private async _embedBatchViaNativeIPC(texts: string[]): Promise<Array<number[]>> {
        try {
            const response = await this.nativeIpcClient!.embed(texts);

            if (response.data && response.data.length === texts.length) {
                // Auto-detect dimension from first response
                if (response.dimensions > 0 && response.dimensions !== this.activeConfig.dimension) {
                    logger.info(`[EmbeddingService] Auto-detected dimension: ${response.dimensions}D (was ${this.activeConfig.dimension}D)`);
                    this.activeConfig = {
                        ...this.activeConfig,
                        dimension: response.dimensions,
                    };
                }

                return response.data.map((d) => {
                    if (d.embedding && d.embedding.length > 0) return d.embedding;
                    throw new EmbeddingNotReadyError("gRPC Embed returned empty embedding in batch.");
                });
            }

            throw new EmbeddingNotReadyError("gRPC Embed batch size mismatch.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error
                ? ((e as { cause?: { message?: string } }).cause?.message || e.message)
                : String(e);
            logger.warn(`[EmbeddingService] gRPC Batch Embed failed: ${errMsg}`);
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        }
    }

    /**
     * HTTP batch embedding path — llama-server /v1/embeddings natively supports string[] input.
     */
    private async _embedBatchViaHTTP(texts: string[]): Promise<Array<number[]>> {
        try {
            const res = await safeFetch(this.apiUrl, {
                method: "POST",
                headers: { "Content-Type": "application/json" },
                body: JSON.stringify({ input: texts }),
            }, 30000);

            const json: EmbeddingResponse = await res.json();

            if (json.data && json.data.length === texts.length) {
                return json.data.map((d) => {
                    if (d.embedding && d.embedding.length > 0) return d.embedding;
                    throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
                });
            }

            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error
                ? ((e as { cause?: { message?: string } }).cause?.message || e.message)
                : String(e);
            logger.warn(`[EmbeddingService] Batch embedding failed: ${errMsg}`);
            throw new EmbeddingNotReadyError("Embedding GPU unavailable or yielded.");
        }
    }

    // ===========================
    // Matryoshka Representation Learning (MRL)
    // ===========================

    /**
     * Truncate a full-dimension vector to a smaller dimension using Matryoshka truncation.
     * Only supported for nomic-embed-text-v1.5. For MiniLM, returns input unchanged.
     *
     * Performance: Uses Float32Array + for-loop to avoid V8 GC pressure on hot-path.
     * Valid target dims: 768, 512, 256, 128, 64
     *
     * @param vector    Full-dimension vector from embed()
     * @param targetDim Target dimension (must be <= current dimension)
     * @returns         Truncated and re-normalized vector
     */
    public truncateMatryoshka(vector: number[], targetDim: number): number[] {
        if (!this.activeConfig.supportsMRL) return vector;
        if (targetDim >= vector.length) return vector;

        // Hot-path: Float32Array + manual for-loop (no .reduce() / .map() GC pressure)
        const truncated = new Float32Array(targetDim);
        let normSq = 0;
        for (let i = 0; i < targetDim; i++) {
            truncated[i] = vector[i];
            normSq += truncated[i] * truncated[i];
        }

        // Re-normalize after truncation
        if (normSq > 0) {
            const invNorm = 1 / Math.sqrt(normSq);
            for (let i = 0; i < targetDim; i++) {
                truncated[i] *= invNorm;
            }
        }

        return Array.from(truncated);
    }

    /**
     * Embed text and immediately truncate to target dimension.
     * Convenience method combining embed() + truncateMatryoshka().
     */
    public async embedTruncated(text: string, targetDim: number): Promise<number[]> {
        const fullVec = await this.embed(text);
        return this.truncateMatryoshka(fullVec, targetDim);
    }

    // ===========================
    // Diagnostics & Config
    // ===========================

    /** Get status for diagnostics */
    public get ready(): boolean {
        return this.isReady;
    }

    /** Get the active embedding dimension */
    public get dimension(): number {
        return this.activeConfig.dimension;
    }

    /** Get the active model ID — returns API URL or gRPC mode indicator */
    public get modelId(): string {
        if (this.useNativeIPC) return "native-grpc-embed:8100";
        return this.apiUrl || "gpu-embedding-api";
    }

    /** Check if Matryoshka truncation is supported */
    public get supportsMRL(): boolean {
        return this.activeConfig.supportsMRL;
    }

    /** Get a dummy vector at the active dimension (for fallback) */
    public getDummyVector(): number[] {
        return new Array(this.activeConfig.dimension).fill(0.01);
    }

    /** Cleanup — release API client state */
    public dispose(): void {
        if (this.nativeIpcClient) {
            this.nativeIpcClient.destroy();
            this.nativeIpcClient = null;
        }
        this.apiUrl = "";
        this.initPromise = null;
        this.isReady = false;
        this.useNativeIPC = false;
        logger.info("[EmbeddingService] 🧹 Disposed. GPU API client cleanup complete.");
    }

}
