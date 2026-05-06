import { logger } from "../utils/logger";
import { FF } from "../utils/FeatureFlags";

/**
 * EmbeddingService — Singleton Embedding Pipeline (Promise Lock)
 * ==============================================================
 * Shared embedding service using @huggingface/transformers v4.
 * Eliminates duplicate model loading across MemoryManager, LanceMemory, LearningLog.
 *
 * Architecture:
 *   - Singleton pattern ensures exactly ONE model instance in memory.
 *   - Promise Lock ensures concurrent callers during boot wait for the same init,
 *     preventing triple-load race conditions.
 *   - WebGPU → CPU fallback for hardware compatibility.
 *   - Feature Flag gated: FF.NOMIC_EMBED selects model at init time.
 *
 * Models:
 *   - nomic-embed-text-v1.5: 768D (MRL truncatable), 8192 ctx, ~150-300MB ONNX
 *   - all-MiniLM-L6-v2:      384D fixed, 512 ctx, ~80MB ONNX (legacy fallback)
 *
 * Matryoshka Representation Learning (MRL):
 *   nomic-embed-text-v1.5 supports truncation of output vectors to lower dimensions
 *   (768 → 512 → 256 → 128 → 64) with minimal quality loss. Use `embedWithTruncation()`
 *   for storage-optimized embeddings. Truncation uses Float32Array hot-path math
 *   to avoid V8 GC pressure from Array.prototype.reduce().
 *
 * Usage:
 *   const service = EmbeddingService.getInstance();
 *   const vector = await service.embed("Hello world");
 *   const truncated = service.truncateMatryoshka(vector, 256);
 *
 * @replaces @xenova/transformers (BANNED per AI_CONTEXT.md)
 */

// Dynamic import type — @huggingface/transformers is ESM
type FeatureExtractionPipeline = any;

// ===========================
// Model Configuration
// ===========================

interface ModelConfig {
    modelId: string;
    dimension: number;
    contextLength: number;
    supportsMRL: boolean;
}

const MODEL_NOMIC: ModelConfig = {
    modelId: "nomic-ai/nomic-embed-text-v1.5",
    dimension: 768,
    contextLength: 8192,
    supportsMRL: true,
};

const MODEL_MINILM: ModelConfig = {
    modelId: "Xenova/all-MiniLM-L6-v2",
    dimension: 384,
    contextLength: 512,
    supportsMRL: false,
};

// ===========================
// Main Class
// ===========================

export class EmbeddingService {
    private static instance: EmbeddingService;
    private embedder: FeatureExtractionPipeline | null = null;

    /** Promise Lock: prevents triple-load when 3 components call ensureReady() at boot */
    private initPromise: Promise<void> | null = null;
    private isReady = false;

    /** Active model configuration (resolved at init time from feature flag) */
    private activeConfig: ModelConfig = MODEL_MINILM;

    private constructor() {}

    public static getInstance(): EmbeddingService {
        if (!EmbeddingService.instance) {
            EmbeddingService.instance = new EmbeddingService();
        }
        return EmbeddingService.instance;
    }

    /**
     * Ensure the embedding model is loaded.
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
            // Resolve model from feature flag at init time
            this.activeConfig = FF.isEnabled("NOMIC_EMBED") ? MODEL_NOMIC : MODEL_MINILM;

            logger.info(
                `[EmbeddingService] 🧠 Loading ${this.activeConfig.modelId} (${this.activeConfig.dimension}D, ${this.activeConfig.contextLength} ctx)...`
            );

            // Dynamic import — @huggingface/transformers is ESM-only
            const { pipeline } = await import("@huggingface/transformers");

            // Try WebGPU first for 10x speed, fallback to WASM CPU
            try {
                this.embedder = await pipeline("feature-extraction", this.activeConfig.modelId, {
                    device: "webgpu",
                });
                logger.info("[EmbeddingService] 🚀 WebGPU acceleration enabled!");
            } catch {
                logger.info("[EmbeddingService] ⚠️ WebGPU unavailable, using WASM CPU fallback...");
                this.embedder = await pipeline("feature-extraction", this.activeConfig.modelId, {
                    device: "cpu",
                });
                logger.info("[EmbeddingService] ✅ WASM CPU embedder initialized.");
            }

            this.isReady = true;
            logger.info(
                `[EmbeddingService] ✅ Model ready: ${this.activeConfig.modelId} (${this.activeConfig.dimension}D). Shared across all memory subsystems.`
            );
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[EmbeddingService] ❌ Init failed: ${errMsg}`);
            // Don't rethrow — callers use getDummyVector() fallback
        }
    }

    // ===========================
    // Public API
    // ===========================

    /**
     * Generate embedding vector for input text.
     * Returns normalized vector at active model dimension, or dummy vector on failure.
     */
    public async embed(text: string): Promise<number[]> {
        await this.ensureReady();
        if (!this.embedder) return this.getDummyVector();

        try {
            const output = await this.embedder(text, {
                pooling: "mean",
                normalize: true,
            });
            return Array.from(output.data);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[EmbeddingService] Embedding failed, using dummy: ${errMsg}`);
            return this.getDummyVector();
        }
    }

    /**
     * Generate embedding with timeout guard (for hot-path usage).
     * Falls back to dummy vector if embedding takes longer than timeoutMs.
     */
    public async embedWithTimeout(text: string, timeoutMs: number = 2000): Promise<number[]> {
        await this.ensureReady();
        if (!this.embedder) return this.getDummyVector();

        let timeoutId: NodeJS.Timeout;
        try {
            const embedPromise = this.embedder(text, { pooling: "mean", normalize: true });
            const timeoutPromise = new Promise<null>((_, reject) => {
                timeoutId = setTimeout(() => reject(new Error("Embedding timeout")), timeoutMs);
            });

            const output = await Promise.race([embedPromise, timeoutPromise]);
            clearTimeout(timeoutId!);

            if (output) return Array.from((output as any).data);
            return this.getDummyVector();
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            clearTimeout(timeoutId!);
            logger.warn(`[EmbeddingService] Timeout/error (${timeoutMs}ms): ${errMsg}`);
            return this.getDummyVector();
        }
    }

    /**
     * Batch embedding — generate vectors for multiple texts in a single pipeline call.
     * HuggingFace pipeline natively supports string[] input for batched tensor processing.
     * ~5-10x faster than sequential embed() calls for large anchor sets.
     *
     * @param texts    Array of strings to embed
     * @returns        Array of vectors at active dimension (dummy vectors for failures)
     */
    public async embedBatch(texts: string[]): Promise<number[][]> {
        await this.ensureReady();
        if (!this.embedder || texts.length === 0) return texts.map(() => this.getDummyVector());

        try {
            const output = await this.embedder(texts, {
                pooling: "mean",
                normalize: true,
            });

            // HuggingFace returns a single flat tensor for batch — reshape into per-text vectors
            const dim = this.activeConfig.dimension;
            const results: number[][] = [];
            for (let i = 0; i < texts.length; i++) {
                const start = i * dim;
                const vec = Array.from(output.data.slice(start, start + dim) as Float32Array);
                results.push(vec.length === dim ? vec : this.getDummyVector());
            }
            return results;
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[EmbeddingService] Batch embedding failed, using dummy vectors: ${errMsg}`);
            return texts.map(() => this.getDummyVector());
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

    /** Get the active model ID */
    public get modelId(): string {
        return this.activeConfig.modelId;
    }

    /** Check if Matryoshka truncation is supported */
    public get supportsMRL(): boolean {
        return this.activeConfig.supportsMRL;
    }

    /** Get a dummy vector at the active dimension (for fallback) */
    public getDummyVector(): number[] {
        return new Array(this.activeConfig.dimension).fill(0.01);
    }

    /** Cleanup — release model from memory */
    public dispose(): void {
        if (this.embedder && typeof this.embedder.dispose === 'function') {
            try {
                this.embedder.dispose();
            } catch (e) {
                // Ignore dispose errors
            }
        }
        this.embedder = null;
        this.initPromise = null;
        this.isReady = false;
        logger.info("[EmbeddingService] 🧹 Disposed. Model freed from RAM.");
    }
}
