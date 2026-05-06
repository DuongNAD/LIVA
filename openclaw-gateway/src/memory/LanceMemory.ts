import * as lancedb from "@lancedb/lancedb";
import * as path from "node:path";
// V16: Migrated to shared EmbeddingService singleton (replaces per-class pipeline loading)
import { EmbeddingService } from "../services/EmbeddingService";
import { logger } from "../utils/logger";
import { FF } from "../utils/FeatureFlags";

/**
 * LanceMemoryManager — L2 Semantic Vector Storage
 * =================================================
 * Phase 1 RAG Upgrade:
 *   - Dynamic dimension from EmbeddingService (384D MiniLM or 768D Nomic)
 *   - Table versioning: auto-creates new table when dimension changes
 *   - Hybrid Search ready (Vector + BM25 FTS via Tantivy)
 *
 * @module LanceMemory
 */

/** Current schema version — bump when dimension or schema changes */
const TABLE_VERSION = "v16";

export class LanceMemoryManager {
    private db: lancedb.Connection | null = null;
    private table: lancedb.Table | null = null;

    // V16: Shared EmbeddingService (Singleton — single model load for entire system)
    private readonly embeddingService: EmbeddingService;

    constructor(embeddingService?: EmbeddingService) {
        this.embeddingService = embeddingService ?? EmbeddingService.getInstance();
    }

    /**
     * Get the table name based on current embedding dimension.
     * Auto-versioning: when model changes (384D→768D), a new table is created.
     */
    private getTableName(): string {
        const dim = this.embeddingService.dimension;
        return `episodic_reflexion_${TABLE_VERSION}_${dim}d`;
    }

    async connect() {
        const dbDir = path.join(process.cwd(), "data", "lancedb");
        this.db = await lancedb.connect(dbDir);

        try {
            const dim = this.embeddingService.dimension;
            logger.info(`[LanceDB] 💿 Initializing via shared EmbeddingService (${dim}D)...`);
            await this.embeddingService.ensureReady();
            logger.info("[LanceDB] ✅ EmbeddingService connected.");
        } catch(e) {
            logger.error(`[LanceDB] Lỗi kết nối EmbeddingService: ${e}`);
        }

        try {
            this.table = await this.db.openTable(this.getTableName());
        } catch {
            // Table doesn't exist yet — will be created on first addMemory()
        }
    }

    private async getEmbeddings(text: string): Promise<number[]> {
        return this.embeddingService.embed(text);
    }

    async addMemory(type: "DEAD-END" | "SUCCESS" | "AXIOM" | "ANCHOR", content: string, fileTarget: string) {
        if (!this.db) await this.connect();

        const timestamp = Date.now();
        const vector = await this.getEmbeddings(content);

        const data = [{
            vector,
            text: content,
            type,
            fileTarget,
            timestamp
        }];

        if (!this.table) {
            this.table = await this.db!.createTable(this.getTableName(), data);
            // Create FTS index for hybrid search (Vector + BM25)
            await this.table.createIndex("text", { config: lancedb.Index.fts() });
            logger.info(`[LanceDB] 🟢 Created FTS Index on 'text' column for table ${this.getTableName()}`);
        } else {
            await this.table.add(data);
        }
    }

    async addSemanticAnchor(summary: string, turnIds: string[], timestamp: number): Promise<void> {
        if (!this.db) await this.connect();
        const vector = await this.getEmbeddings(summary);

        const data = [{
            vector,
            text: summary,
            type: "ANCHOR",
            fileTarget: JSON.stringify(turnIds),
            timestamp
        }];

        if (!this.table) {
            this.table = await this.db!.createTable(this.getTableName(), data);
            // Create FTS index for hybrid search (Vector + BM25)
            await this.table.createIndex("text", { config: lancedb.Index.fts() });
            logger.info(`[LanceDB] 🟢 Created FTS Index on 'text' column for table ${this.getTableName()}`);
        } else {
            await this.table.add(data);
        }
    }

    async searchMemory(query: string, limit: number = 3): Promise<string[]> {
        if (!this.db) await this.connect();
        if (!this.table) return [];

        const queryVector = await this.getEmbeddings(query);
        try {
            let results;
            if (FF.isEnabled("HYBRID_SEARCH")) {
                const reranker = await lancedb.rerankers.RRFReranker.create();
                results = await this.table.query()
                    .nearestTo(queryVector)
                    .fullTextSearch(query)
                    .limit(limit)
                    .rerank(reranker)
                    .toArray();
            } else {
                results = await this.table.vectorSearch(queryVector).limit(limit).toArray();
            }
            return results.map((r: unknown) => { const row = r as {type: string, fileTarget: string, text: string}; return `[${row.type}] (Target: ${row.fileTarget}): ${row.text}`; });
        } catch {
            return [];
        }
    }

    async searchAnchors(query: string, limit: number = 5): Promise<string[]> {
        if (!this.db) await this.connect();
        if (!this.table) return [];

        const queryVector = await this.getEmbeddings(query);
        try {
            let results;
            if (FF.isEnabled("HYBRID_SEARCH")) {
                const reranker = await lancedb.rerankers.RRFReranker.create();
                results = await this.table.query()
                    .nearestTo(queryVector)
                    .fullTextSearch(query)
                    .where("type = 'ANCHOR'")
                    .limit(limit)
                    .rerank(reranker)
                    .toArray();
            } else {
                // Filter by ANCHOR type
                results = await this.table.vectorSearch(queryVector)
                                        .where("type = 'ANCHOR'")
                                        .limit(limit)
                                        .toArray();
            }
            return results.map((r: unknown) => (r as {text: string}).text);
        } catch {
            return [];
        }
    }

    async getAllEpisodicMemories(): Promise<unknown[]> {
        if (!this.db) await this.connect();
        if (!this.table) return [];
        try {
            // Get all memories that are not AXIOM
            const results = await this.table.query().where("type != 'AXIOM'").toArray();
            return results;
        } catch {
            return [];
        }
    }

    async clearEpisodicMemories() {
        if (!this.db || !this.table) return;
        try {
            await this.table.delete("type != 'AXIOM'");
        } catch {}
    }

    /**
     * [v4.0] GDPR: Delete vectors matching a filter expression.
     * Called from MemoryManager.purgeUserContext() for Right to be Forgotten.
     */
    async deleteVectors(filter: string): Promise<void> {
        if (!this.db || !this.table) return;
        try {
            await this.table.delete(filter);
            logger.info(`[LanceDB/GDPR] Deleted vectors matching: ${filter}`);
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[LanceDB/GDPR] Delete failed: ${errMsg}`);
        }
    }

    /**
     * [v4.0] Graceful shutdown — release LanceDB file locks (W-3).
     * Called from MemoryManager.dispose() / CoreKernel.shutdown().
     */
    async dispose(): Promise<void> {
        this.table = null;
        this.db = null;
        logger.info("[LanceDB] Connection disposed.");
    }
}
