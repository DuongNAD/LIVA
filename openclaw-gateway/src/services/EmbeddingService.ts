import { logger } from "../utils/logger";

/**
 * EmbeddingService — Singleton Embedding Pipeline (Promise Lock)
 * ==============================================================
 * Shared embedding service using @huggingface/transformers v3.
 * Eliminates duplicate model loading across MemoryManager, LanceMemory, LearningLog.
 * 
 * Architecture:
 *   - Singleton pattern ensures exactly ONE model instance (~140MB) in memory.
 *   - Promise Lock ensures concurrent callers during boot wait for the same init,
 *     preventing triple-load race conditions.
 *   - WebGPU → CPU fallback for hardware compatibility.
 *   - 384-dimension output (all-MiniLM-L6-v2).
 * 
 * Usage:
 *   const service = EmbeddingService.getInstance();
 *   const vector = await service.embed("Hello world");
 * 
 * @replaces @xenova/transformers (BANNED per AI_CONTEXT.md)
 */

// Dynamic import type — @huggingface/transformers is ESM
type FeatureExtractionPipeline = any;

/** Dummy vector (384D) returned when embedding fails — prevents crash in LanceDB search */
const DUMMY_VECTOR_384 = new Array(384).fill(0.01);

export class EmbeddingService {
    private static instance: EmbeddingService;
    private embedder: FeatureExtractionPipeline | null = null;
    
    /** Promise Lock: prevents triple-load when 3 components call ensureReady() at boot */
    private initPromise: Promise<void> | null = null;
    private isReady = false;

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
            logger.info("[EmbeddingService] 🧠 Loading HuggingFace all-MiniLM-L6-v2 (384D)...");

            // Dynamic import — @huggingface/transformers is ESM-only
            const { pipeline } = await import("@huggingface/transformers");

            // Try WebGPU first for 10x speed, fallback to WASM CPU
            try {
                this.embedder = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2", {
                    device: "webgpu",
                });
                logger.info("[EmbeddingService] 🚀 WebGPU acceleration enabled!");
            } catch {
                logger.info("[EmbeddingService] ⚠️ WebGPU unavailable, using WASM CPU fallback...");
                this.embedder = await pipeline("feature-extraction", "Xenova/all-MiniLM-L6-v2", {
                    device: "cpu",
                });
                logger.info("[EmbeddingService] ✅ WASM CPU embedder initialized.");
            }

            this.isReady = true;
            logger.info("[EmbeddingService] ✅ Model ready. Shared across all memory subsystems.");
        } catch (e: any) {
            logger.error(`[EmbeddingService] ❌ Init failed: ${e.message}`);
            // Don't rethrow — callers use getDummyVector() fallback
        }
    }

    /**
     * Generate embedding vector for input text.
     * Returns 384-dim normalized vector, or dummy vector on failure.
     */
    public async embed(text: string): Promise<number[]> {
        await this.ensureReady();
        if (!this.embedder) return DUMMY_VECTOR_384;

        try {
            const output = await this.embedder(text, {
                pooling: "mean",
                normalize: true,
            });
            return Array.from(output.data);
        } catch (e: any) {
            logger.warn(`[EmbeddingService] Embedding failed, using dummy: ${e.message}`);
            return DUMMY_VECTOR_384;
        }
    }

    /**
     * Generate embedding with timeout guard (for hot-path usage).
     * Falls back to dummy vector if embedding takes longer than timeoutMs.
     */
    public async embedWithTimeout(text: string, timeoutMs: number = 2000): Promise<number[]> {
        await this.ensureReady();
        if (!this.embedder) return DUMMY_VECTOR_384;

        let timeoutId: NodeJS.Timeout;
        try {
            const embedPromise = this.embedder(text, { pooling: "mean", normalize: true });
            const timeoutPromise = new Promise<null>((_, reject) => {
                timeoutId = setTimeout(() => reject(new Error("Embedding timeout")), timeoutMs);
            });

            const output = await Promise.race([embedPromise, timeoutPromise]);
            clearTimeout(timeoutId!);

            if (output) return Array.from((output as any).data);
            return DUMMY_VECTOR_384;
        } catch (e: any) {
            clearTimeout(timeoutId!);
            logger.warn(`[EmbeddingService] Timeout/error (${timeoutMs}ms): ${e.message}`);
            return DUMMY_VECTOR_384;
        }
    }

    /** Get status for diagnostics */
    public get ready(): boolean {
        return this.isReady;
    }

    /** Cleanup — release model from memory */
    public dispose(): void {
        this.embedder = null;
        this.initPromise = null;
        this.isReady = false;
        logger.info("[EmbeddingService] 🧹 Disposed. Model freed from RAM.");
    }
}
