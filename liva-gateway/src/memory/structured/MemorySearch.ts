import { logger } from "../../utils/logger";
import { FlashRankService } from "../../services/FlashRankService";
import type { VectorRepository } from "../VectorRepository";
import type { StructuredMemory } from "../StructuredMemory";

export class MemorySearch {
    #vectorQueue: Array<{
        vecId: string; type: string; content: string; vector: number[];
        domain?: string; category?: string; traceKeywords?: string[]; fileTarget?: string;
        sourceEventIds?: string[];
    }> = [];
    #vectorQueueTimer: NodeJS.Timeout | null = null;

    constructor(
        private readonly parent: StructuredMemory,
        private readonly vectorRepo: VectorRepository
    ) {}

    public async initVecDimension(dimension: number): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.initVecDimension(dimension);
    }

    public upsertVector(record: {
        vecId: string; type: string; content: string; vector: number[];
        domain?: string; category?: string; traceKeywords?: string[]; fileTarget?: string;
        sourceEventIds?: string[];
    }): void {
        if (this.parent.isClosed) {
            logger.debug("[StructuredMemory] upsertVector ignored: StructuredMemory is closed");
            return;
        }
        this.#vectorQueue.push(record);
        if (this.#vectorQueue.length >= 50) {
            this.flushVectorQueue().catch(err => {
                logger.error(`[StructuredMemory] Error in background flush: ${err}`);
            });
        } else if (!this.#vectorQueueTimer) {
            this.#vectorQueueTimer = setTimeout(() => {
                this.flushVectorQueue().catch(err => {
                    logger.error(`[StructuredMemory] Error in background flush: ${err}`);
                });
            }, 10_000);
            this.#vectorQueueTimer.unref();
        }
    }

    public async flushVectorQueue(): Promise<void> {
        if (this.#vectorQueue.length === 0) return;
        const records = [...this.#vectorQueue];
        this.#vectorQueue = [];
        if (this.#vectorQueueTimer) {
            clearTimeout(this.#vectorQueueTimer);
            this.#vectorQueueTimer = null;
        }
        try {
            await this.parent.ensureInitialized();
            await this.vectorRepo.upsertVectorsBatch(records);
            logger.debug(`[StructuredMemory] Flushed ${records.length} vectors to database.`);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory] Failed to flush vector queue: ${errMsg}`);
        }
    }

    public async upsertVectorsBatch(records: Array<{
        vecId: string; type: string; content: string; vector: number[];
        domain?: string; category?: string; traceKeywords?: string[];
        sourceEventIds?: string[];
    }>): Promise<void> {
        await this.flushVectorQueue(); // Keep order intact
        await this.parent.ensureInitialized();
        await this.vectorRepo.upsertVectorsBatch(records);
    }

    public async searchSimilarVectors(
        queryVector: number[], topK?: number, typeFilter?: string
    ): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.searchSimilarVectors(queryVector, topK, typeFilter ? { type: typeFilter } : undefined);
    }

    public async searchHybridVectors(
        queryText: string,
        queryVector: number[],
        topK?: number,
        typeFilter?: string
    ): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.searchHybridVectors(queryText, queryVector, topK, typeFilter ? { type: typeFilter } : undefined);
    }

    public async searchAnchors(queryVector: number[], limit?: number): Promise<string[]> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.searchAnchors(queryVector, limit);
    }

    public async searchAnchorsWithScores(queryVector: number[], limit?: number, queryText?: string): Promise<Array<{ content: string; score: number }>> {
        await this.parent.ensureInitialized();
        let results = await this.vectorRepo.searchAnchorsWithScores(queryVector, limit);
        if (queryText && results.length > 0) {
            results = (await FlashRankService.getInstance().rerank(queryText, results)) as typeof results;
            results = results.slice(0, 3);
        }
        return results;
    }

    public async searchAxiomsByVector(queryVector: number[], limit?: number): Promise<Array<{ text: string; traceKeywords: string }>> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.searchAxiomsByVector(queryVector, limit);
    }

    public async searchWithDrilldown(queryVector: number[], topK?: number, typeFilter?: string) {
        await this.parent.ensureInitialized();
        return this.vectorRepo.searchWithDrilldown(queryVector, topK, typeFilter);
    }

    public async collectDrilldownEventIds(queryVector: number[], topK?: number, typeFilter?: string): Promise<string[]> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.collectDrilldownEventIds(queryVector, topK, typeFilter);
    }

    public async deleteVectorByContent(content: string): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.deleteVectorByContent(content);
    }

    public async deleteVectorById(vecId: string): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.deleteVectorById(vecId);
    }

    public async deleteAllVectors(): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.deleteAllVectors();
    }

    public async getVectorCount(): Promise<number> {
        await this.parent.ensureInitialized();
        return this.vectorRepo.getVectorCount();
    }

    public get vecReady(): boolean {
        return this.vectorRepo.vecReady;
    }

    public async pushToDLQ(filter: string): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.pushToDLQ(filter);
    }

    public async processDLQ(): Promise<void> {
        await this.parent.ensureInitialized();
        await this.vectorRepo.processDLQ();
    }

    public async applyVectorDecay(decayRate: number): Promise<{ decayed: number; archived: number }> {
        return this.vectorRepo.applyVectorDecay(decayRate);
    }

    public async flushVectorTouches(): Promise<void> {
        await this.vectorRepo.flushVectorTouches();
    }

    public async close(): Promise<void> {
        if (this.#vectorQueueTimer) {
            clearTimeout(this.#vectorQueueTimer);
            this.#vectorQueueTimer = null;
        }
        await this.flushVectorQueue();
        await this.vectorRepo.flushVectorTouches();
    }
}
