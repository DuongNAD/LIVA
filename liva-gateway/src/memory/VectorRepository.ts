import { DatabaseWorkerBridge } from "./DatabaseWorkerBridge";
import { logger } from "../utils/logger";
import { z } from "zod";

// [UHM] Zod schema for source_event_ids — prevents LLM garbage from crashing json_each
const EventIdsSchema = z.array(z.string()).max(50);

// ===========================
// SQLite result row interfaces (eliminates `as any` casts)
// ===========================
interface ICountRow { c: number }
interface IIdRow { id: number }
interface INameRow { name: string }
interface IVecSearchRow {
    rowid: number;
    distance: number;
    vec_id: string;
    content: string;
    type: string;
    domain: string;
    category: string;
    trace_keywords: string;
    source_event_ids: string;
    decay_weight: number;
    access_count: number;
}
interface IFTSSearchRow {
    rowid: number;
    vec_id: string;
    content: string;
    type: string;
    domain: string;
    category: string;
    trace_keywords: string;
    source_event_ids: string;
}

export interface MetadataFilter {
    type?: string;
    domain?: string;
    category?: string;
    createdAfter?: number;
    createdBefore?: number;
}

/**
 * VectorRepository
 * Encapsulates all vector storage logic asynchronously via DatabaseWorker.
 */
export class VectorRepository {
    readonly #db: DatabaseWorkerBridge;
    #vecDimension: number = 384;
    #vecReady: boolean = false;

    #vectorTouchBuffer: Map<string, number> = new Map();
    #vectorTouchTimer: NodeJS.Timeout | null = null;
    static readonly VECTOR_TOUCH_FLUSH_MS = 15_000;

    constructor(db: DatabaseWorkerBridge) {
        this.#db = db;
    }

    /**
     * Initialize vector tables in SQLite.
     * Dimension is resolved from EmbeddingService at runtime via initVecDimension().
     * Default: 384D (all-MiniLM-L6-v2).
     */
    public async init(): Promise<void> {
        try {
            // Metadata table for vector records
            await this.#db.exec(`
                CREATE TABLE IF NOT EXISTS vectors_meta (
                    id INTEGER PRIMARY KEY AUTOINCREMENT,
                    vec_id TEXT UNIQUE NOT NULL,
                    type TEXT NOT NULL,
                    content TEXT NOT NULL,
                    domain TEXT DEFAULT 'General',
                    category TEXT DEFAULT 'Uncategorized',
                    trace_keywords TEXT DEFAULT '[]',
                    file_target TEXT,
                    created_at INTEGER NOT NULL,
                    last_accessed_at INTEGER DEFAULT 0,
                    decay_weight REAL DEFAULT 1.0,
                    access_count INTEGER DEFAULT 0
                )
            `);

            // FTS5 Virtual Table for full-text search
            await this.#db.exec(`
                CREATE VIRTUAL TABLE IF NOT EXISTS vectors_fts USING fts5(
                    content,
                    tokenize='porter'
                )
            `);

            // Detect dimension from existing table or use default
            this.#vecDimension = await this.#detectOrCreateVecTable();
            this.#vecReady = true;

            const count = (await this.#db.prepare('SELECT count(*) as c FROM vectors_meta').get() as ICountRow | null)?.c ?? 0;
            logger.info(`[StructuredMemory/Vec] ✅ sqlite-vec loaded (${this.#vecDimension}D, ${count} vectors).`);

            // [UHM] Positional Index: add source_event_ids column (idempotent)
            try { await this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN source_event_ids TEXT DEFAULT '[]'"); } catch { /* already exists */ }
            try { await this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN decay_weight REAL DEFAULT 1.0"); } catch { /* already exists */ }
            try { await this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN access_count INTEGER DEFAULT 0"); } catch { /* already exists */ }

            // Backfill existing meta records into vectors_fts if empty
            const ftsCount = (await this.#db.prepare('SELECT count(*) as c FROM vectors_fts').get() as ICountRow | null)?.c ?? 0;
            if (ftsCount === 0 && count > 0) {
                logger.info(`[StructuredMemory/Vec] Backfilling ${count} existing vectors into FTS5 virtual table...`);
                await this.#db.exec(`
                    INSERT INTO vectors_fts(rowid, content)
                    SELECT id, content FROM vectors_meta
                `);
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/Vec] ❌ sqlite-vec init failed: ${errMsg}`);
            this.#vecReady = false;
        }
    }

    async #detectOrCreateVecTable(): Promise<number> {
        const existing = await this.#db.prepare(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_idx'"
        ).get() as { sql: string } | null;

        // Tự động Migrate từ Float sang INT8 nếu tồn tại bảng float cũ
        if (existing && existing.sql) {
            const isFloat = existing.sql.includes('float[');
            const isInt8 = existing.sql.includes('int8[');
            
            const match = existing.sql.match(/(?:float|int8)\[(\d+)\]/);
            const dim = match ? parseInt(match[1], 10) : this.#vecDimension;

            if (isFloat) {
                logger.info(`[StructuredMemory/Vec] Detected old Float32 vec_idx (${dim}D). Migrating to INT8 Quantization...`);
                
                await this.#db.exec(`DROP TABLE IF EXISTS vec_idx_new`);
                await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx_new USING vec0(embedding int8[${dim}])`);
                
                // Migrate data using sqlite-vec built-in quantizer
                await this.#db.exec(`INSERT INTO vec_idx_new(rowid, embedding) SELECT rowid, vec_quantize_int8(embedding, 'unit') FROM vec_idx`);
                
                // Double copy to avoid RENAME TO shadow table bugs in sqlite-vec
                await this.#db.exec('DROP TABLE vec_idx');
                await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dim}])`);
                await this.#db.exec(`INSERT INTO vec_idx(rowid, embedding) SELECT rowid, embedding FROM vec_idx_new`);
                await this.#db.exec(`DROP TABLE vec_idx_new`);
                
                logger.info(`[StructuredMemory/Vec] ✅ Migration to INT8 complete. RAM footprint reduced by 75%.`);
            } else if (isInt8) {
                logger.info(`[StructuredMemory/Vec] Detected INT8 vec_idx dimension: ${dim}D`);
            }

            return dim;
        }

        await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${this.#vecDimension}])`);
        return this.#vecDimension;
    }

    /**
     * Set the embedding dimension. Must be called before first vector insert
     * if dimension differs from default 384. Recreates vec_idx if dimension changed.
     */
    public async initVecDimension(dimension: number): Promise<void> {
        if (dimension === this.#vecDimension && this.#vecReady) return;

        const oldDim = this.#vecDimension;
        this.#vecDimension = dimension;

        const existing = await this.#db.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='vec_idx'"
        ).get() as INameRow | null;

        if (existing) {
            const vecCount = (await this.#db.prepare('SELECT count(*) as c FROM vec_idx').get() as ICountRow | null)?.c ?? 0;
            if (vecCount === 0 && oldDim !== dimension) {
                await this.#db.exec('DROP TABLE vec_idx');
                await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
                logger.info(`[StructuredMemory/Vec] Recreated vec_idx: ${oldDim}D → ${dimension}D`);
            } else if (oldDim !== dimension && vecCount > 0) {
                logger.warn(`[StructuredMemory/Vec] Dimension mismatch (${oldDim}→${dimension}) with ${vecCount} existing vectors. Re-embedding required.`);
                await this.#db.exec('DELETE FROM vec_idx');
                await this.#db.exec('DELETE FROM vectors_meta');
                await this.#db.exec('DROP TABLE vec_idx');
                await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
                logger.warn(`[StructuredMemory/Vec] Cleared all vectors. ConsolidationCron will re-embed from L1.`);
            }
        } else {
            await this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
        }

        this.#vecReady = true;
    }

    // ===========================
    // L2 Vector CRUD Operations
    // ===========================

    public async upsertVector(record: {
        vecId: string;
        type: string;
        content: string;
        vector: number[];
        domain?: string;
        category?: string;
        traceKeywords?: string[];
        fileTarget?: string;
        sourceEventIds?: string[];  // [UHM] L2→L1 positional pointers (max 50)
    }, isRetry = false): Promise<void> {
        if (!this.#vecReady) return;

        // [G4] Cap at 50 entries to prevent RAM overflow
        const eventIds = JSON.stringify((record.sourceEventIds ?? []).slice(0, 50));

        // Use INSERT OR IGNORE to guarantee the row exists without throwing UNIQUE constraint
        const result = await this.#db.prepare(`
            INSERT OR IGNORE INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at, decay_weight, access_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 1.0, 0)
        `).run(
            record.vecId, record.type, record.content,
            record.domain ?? 'General', record.category ?? 'Uncategorized',
            JSON.stringify(record.traceKeywords ?? []), record.fileTarget ?? null,
            eventIds, Date.now()
        ) as { changes: number };

        const metaRow = await this.#db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(record.vecId) as IIdRow | null;
        if (!metaRow) return;

        if (result && result.changes === 0) {
            // Force UPDATE to ensure the latest data is present because INSERT OR IGNORE ignored it
            await this.#db.prepare(`
                UPDATE vectors_meta SET type=?, content=?, domain=?, category=?, trace_keywords=?, file_target=?, source_event_ids=?, last_accessed_at=?, decay_weight=1.0, access_count=access_count+1
                WHERE id=?
            `).run(
                record.type, record.content,
                record.domain ?? 'General', record.category ?? 'Uncategorized',
                JSON.stringify(record.traceKeywords ?? []), record.fileTarget ?? null,
                eventIds, Date.now(), metaRow.id
            );

            // Update vec_idx (remove old vector)
            await this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(metaRow.id));
        }

        const blob = new Uint8Array(new Float32Array(record.vector).buffer);
        // Dùng vec_quantize_int8 để chuyển Float32 -> INT8 trong C++ SQLite
        await this.#db.prepare('INSERT INTO vec_idx(rowid, embedding) VALUES (?, vec_quantize_int8(?, \'unit\'))').run(BigInt(metaRow.id), blob);

        // Synchronize with FTS5 virtual table
        try {
            await this.#db.prepare(`
                INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES (?, ?)
            `).run(BigInt(metaRow.id), record.content);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/Vec] Failed to sync FTS5 on upsert: ${errMsg}`);
        }
    }

    public async upsertVectorsBatch(records: Array<{
        vecId: string;
        type: string;
        content: string;
        vector: number[];
        domain?: string;
        category?: string;
        traceKeywords?: string[];
        sourceEventIds?: string[];
    }>): Promise<void> {
        if (!this.#vecReady || records.length === 0) return;

        const CHUNK_SIZE = 500;
        const totalRecords = records.length;
        const startTimeTotal = performance.now();

        for (let i = 0; i < totalRecords; i += CHUNK_SIZE) {
            const chunk = records.slice(i, i + CHUNK_SIZE);
            const metaParamSets: any[][] = [];
            const deleteVecParamSets: any[][] = [];
            const insertVecParamSets: any[][] = [];
            const ftsParamSets: any[][] = [];
            const now = Date.now();

            for (const record of chunk) {
                if (record.vector.length !== this.#vecDimension) {
                    throw new Error(`Dimension mismatch for inserted vector. Expected ${this.#vecDimension} dimensions but received ${record.vector.length}.`);
                }

                const eventIds = JSON.stringify((record.sourceEventIds ?? []).slice(0, 50));
                
                metaParamSets.push([
                    record.vecId, record.type, record.content,
                    record.domain ?? 'General', record.category ?? 'Uncategorized',
                    JSON.stringify(record.traceKeywords ?? []), eventIds, now
                ]);

                deleteVecParamSets.push([record.vecId]);

                const blob = new Uint8Array(new Float32Array(record.vector).buffer);
                insertVecParamSets.push([record.vecId, blob]);

                ftsParamSets.push([record.vecId, record.content]);
            }

            const statements = [
                {
                    sql: `
                        INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at, decay_weight, access_count)
                        VALUES (?, ?, ?, ?, ?, ?, NULL, ?, ?, 0, 1.0, 0)
                        ON CONFLICT(vec_id) DO UPDATE SET
                            type=excluded.type, content=excluded.content, domain=excluded.domain, category=excluded.category,
                            trace_keywords=excluded.trace_keywords, source_event_ids=excluded.source_event_ids,
                            last_accessed_at=excluded.last_accessed_at, decay_weight=1.0, access_count=vectors_meta.access_count+1
                    `,
                    paramSets: metaParamSets
                },
                {
                    sql: `DELETE FROM vec_idx WHERE rowid = (SELECT id FROM vectors_meta WHERE vec_id = ?)`,
                    paramSets: deleteVecParamSets
                },
                {
                    sql: `INSERT INTO vec_idx(rowid, embedding) VALUES ((SELECT id FROM vectors_meta WHERE vec_id = ?), vec_quantize_int8(?, 'unit'))`,
                    paramSets: insertVecParamSets
                },
                {
                    sql: `INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES ((SELECT id FROM vectors_meta WHERE vec_id = ?), ?)`,
                    paramSets: ftsParamSets
                }
            ];

            // IPC transactional write with exponential backoff & observability
            let attempts = 0;
            const maxAttempts = 3;
            let success = false;
            let lastErr: any = null;
            const chunkStartTime = performance.now();

            while (attempts < maxAttempts && !success) {
                try {
                    await (this.#db as any).transactionBatch(statements);
                    success = true;
                } catch (e: any) {
                    attempts++;
                    lastErr = e;
                    if (attempts < maxAttempts) {
                        const backoffDelay = 50 * Math.pow(2, attempts); // 100ms, 200ms
                        logger.warn(`[VectorRepository] Batch chunk write failed (attempt ${attempts}/${maxAttempts}), retrying in ${backoffDelay}ms. Error: ${e.message}`);
                        await new Promise(r => setTimeout(r, backoffDelay));
                    }
                }
            }

            if (!success) {
                logger.error(`[VectorRepository] Batch chunk write failed permanently after ${maxAttempts} attempts. Error: ${lastErr?.message}`);
                throw lastErr;
            }

            const chunkDuration = performance.now() - chunkStartTime;
            logger.debug(`[VectorRepository] Chunk [${i}-${Math.min(i + CHUNK_SIZE, totalRecords)}/${totalRecords}] written. Latency: ${chunkDuration.toFixed(2)}ms | Throughput: ${(chunk.length / (chunkDuration / 1000)).toFixed(2)} vectors/sec`);
        }

        const totalDuration = performance.now() - startTimeTotal;
        logger.info(`[VectorRepository] Completed upsertVectorsBatch for ${totalRecords} vectors. Total latency: ${totalDuration.toFixed(2)}ms.`);
    }

    public async searchSimilarVectors(
        queryVector: number[],
        topK: number = 5,
        filter?: MetadataFilter
    ): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> {
        if (!this.#vecReady) return [];

        const blob = new Uint8Array(new Float32Array(queryVector).buffer);
        const fetchK = filter && Object.keys(filter).length > 0 ? topK * 3 : topK;

        // Xây dựng điều kiện WHERE cho Metadata (B-Tree Pre-filtering)
        let metaConditions = "1=1";
        const metaParams: Array<string | number> = [];

        if (filter) {
            if (filter.type) { metaConditions += " AND type = ?"; metaParams.push(filter.type); }
            if (filter.domain) { metaConditions += " AND domain = ?"; metaParams.push(filter.domain); }
            if (filter.category) { metaConditions += " AND category = ?"; metaParams.push(filter.category); }
            if (filter.createdAfter) { metaConditions += " AND created_at >= ?"; metaParams.push(filter.createdAfter); }
            if (filter.createdBefore) { metaConditions += " AND created_at <= ?"; metaParams.push(filter.createdBefore); }
        }

        // Tối ưu Query Planner: Ép SQLite lọc B-Tree trước qua IN (SELECT id ...)
        const sql = `
            SELECT v.rowid, v.distance, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids, m.decay_weight
            FROM vec_idx v
            INNER JOIN vectors_meta m ON m.id = v.rowid
            WHERE v.embedding MATCH vec_quantize_int8(?, 'unit') 
              AND v.k = ?
              AND v.rowid IN (SELECT id FROM vectors_meta WHERE ${metaConditions})
        `;

        const rows = await this.#db.prepare(sql).all(blob, fetchK, ...metaParams) as unknown as IVecSearchRow[];

        const results = rows.map((r) => {
            const distF32 = (r.distance || 0) / 120.0;
            const similarity = Math.max(0, 1.0 - (distF32 * distF32) / 2.0); // Normalize to 0-1
            const decay = r.decay_weight ?? 1.0;
            const finalScore = similarity * decay;
            return {
                id: r.rowid,
                vecId: r.vec_id,
                content: r.content,
                type: r.type,
                domain: r.domain,
                category: r.category,
                distance: r.distance,
                score: finalScore,
                traceKeywords: JSON.parse(r.trace_keywords || '[]') as string[],
                sourceEventIds: (() => {
                    try {
                        const raw = JSON.parse(r.source_event_ids || '[]');
                        const parsed = EventIdsSchema.safeParse(raw);
                        return parsed.success ? parsed.data : [];
                    } catch {
                        return [];
                    }
                })(),
            };
        });

        // [Ebbinghaus] Sort by finalScore descending (highest priority first)
        results.sort((a, b) => b.score - a.score);

        const slice = results.slice(0, topK);
        for (const item of slice) {
            this.touchVector(item.vecId);
        }
        return slice;
    }

    public async searchAnchors(queryVector: number[], limit: number = 5): Promise<string[]> {
        const res = await this.searchSimilarVectors(queryVector, limit, { type: 'ANCHOR' });
        return res.map(r => r.content);
    }

    public async searchAnchorsWithScores(queryVector: number[], limit: number = 5): Promise<Array<{ content: string; score: number }>> {
        const res = await this.searchSimilarVectors(queryVector, limit, { type: 'ANCHOR' });
        return res.map(r => ({ content: r.content, score: r.score }));
    }

    public async searchAxiomsByVector(queryVector: number[], limit: number = 3): Promise<Array<{ text: string; traceKeywords: string }>> {
        const res = await this.searchSimilarVectors(queryVector, limit, { type: 'AXIOM' });
        return res.map(r => ({ text: r.content, traceKeywords: JSON.stringify(r.traceKeywords) }));
    }

    /**
     * [UHM] Search L2 vectors and collect source event IDs for L1 drill-down.
     * [G3] Uses json_each() — avoids SQLite 999 variable limit.
     * [G4] Caps at 50 event IDs per vector, validates JSON safely.
     */
    public async searchWithDrilldown(
        queryVector: number[],
        topK: number = 3,
        typeFilter?: string
    ): Promise<Array<{ vecId: string; content: string; type: string; distance: number; sourceEventIds: string[] }>> {
        const results = await this.searchSimilarVectors(queryVector, topK, typeFilter ? { type: typeFilter } : undefined);
        return results.map(r => ({
            vecId: r.vecId,
            content: r.content,
            type: r.type,
            distance: r.distance,
            sourceEventIds: r.sourceEventIds.slice(0, 50),
        }));
    }

    /**
     * [UHM] Collect all unique source event IDs from drill-down results.
     * Returns a deduplicated, capped array of event IDs for L1 lookup via json_each.
     */
    public async collectDrilldownEventIds(
        queryVector: number[],
        topK: number = 3,
        typeFilter?: string
    ): Promise<string[]> {
        const results = await this.searchWithDrilldown(queryVector, topK, typeFilter);
        const allIds = new Set<string>();
        for (const r of results) {
            for (const id of r.sourceEventIds) {
                allIds.add(id);
            }
        }
        return [...allIds];
    }

    public async deleteVectorByContent(content: string): Promise<void> {
        if (!this.#vecReady) return;
        const row = await this.#db.prepare('SELECT id FROM vectors_meta WHERE content = ?').get(content) as IIdRow | null;
        if (row) {
            await this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(row.id));
            await this.#db.prepare('DELETE FROM vectors_meta WHERE id = ?').run(BigInt(row.id));
            try {
                await this.#db.prepare('DELETE FROM vectors_fts WHERE rowid = ?').run(BigInt(row.id));
            } catch { /* ignore */ }
        }
    }

    public async deleteVectorById(vecId: string): Promise<void> {
        if (!this.#vecReady) return;
        const row = await this.#db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(vecId) as IIdRow | null;
        if (row) {
            await this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(row.id));
            await this.#db.prepare('DELETE FROM vectors_meta WHERE id = ?').run(BigInt(row.id));
            try {
                await this.#db.prepare('DELETE FROM vectors_fts WHERE rowid = ?').run(BigInt(row.id));
            } catch { /* ignore */ }
        }
    }

    public async deleteAllVectors(): Promise<void> {
        if (!this.#vecReady) return;
        await this.#db.exec('DELETE FROM vec_idx');
        await this.#db.exec('DELETE FROM vectors_meta');
        await this.#db.exec('DELETE FROM vectors_fts');
        logger.warn('[StructuredMemory/Vec/GDPR] All vectors permanently erased.');
    }

    public async getVectorCount(): Promise<number> {
        if (!this.#vecReady) return 0;
        const row = await this.#db.prepare('SELECT count(*) as c FROM vectors_meta').get() as ICountRow | null;
        return row ? row.c : 0;
    }

    public get vecReady(): boolean {
        return this.#vecReady;
    }

    // ===========================
    // DLQ — Compensating Transactions
    // ===========================

    public async pushToDLQ(filter: string): Promise<void> {
        try {
            await this.#db.prepare(
                "INSERT INTO vector_dlq (delete_filter, status, retry_count) VALUES (?, 'pending', 0)"
            ).run(filter);
            logger.warn('[StructuredMemory/DLQ] Queued failed delete filter for retry.');
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/DLQ] Failed to push: ${errMsg}`);
        }
    }

    public async processDLQ(): Promise<void> {
        try {
            const rows = await this.#db.prepare(
                "SELECT id, delete_filter, retry_count FROM vector_dlq WHERE status = 'pending'"
            ).all() as Array<{ id: number; delete_filter: string; retry_count: number }>;
            for (const row of rows) {
                if (row.retry_count >= 3) {
                    await this.#db.prepare("UPDATE vector_dlq SET status = 'dead_letter' WHERE id = ?").run(row.id);
                    logger.warn(`[StructuredMemory/DLQ] Marked entry ${row.id} as dead_letter.`);
                    continue;
                }
                try {
                    await this.deleteVectorByContent(row.delete_filter);
                    await this.#db.prepare('DELETE FROM vector_dlq WHERE id = ?').run(row.id);
                    logger.info(`[StructuredMemory/DLQ] Cleaned entry ${row.id}.`);
                } catch {
                    await this.#db.prepare('UPDATE vector_dlq SET retry_count = retry_count + 1 WHERE id = ?').run(row.id);
                }
            }
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/DLQ] processDLQ error: ${errMsg}`);
        }
    }

    /**
     * [v25 Hybrid RAG] Combined search using sqlite-vec (KNN) and FTS5 (BM25)
     * merged via Reciprocal Rank Fusion (RRF).
     * 
     * RRF Formula: Score = sum( 1 / (60 + rank) )
     * Returns top results matching both semantic meaning and keyword precision.
     */
    public async searchHybridVectors(
        queryText: string,
        queryVector: number[],
        topK: number = 5,
        filter?: MetadataFilter
    ): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> {
        if (!this.#vecReady) return [];

        // 1. Get Vector KNN search results (Pre-filtered)
        const vectorResults = await this.searchSimilarVectors(queryVector, topK * 3, filter);

        // Build Metadata Conditions for FTS
        let metaConditions = "1=1";
        const metaParams: Array<string | number> = [];

        if (filter) {
            if (filter.type) { metaConditions += " AND m.type = ?"; metaParams.push(filter.type); }
            if (filter.domain) { metaConditions += " AND m.domain = ?"; metaParams.push(filter.domain); }
            if (filter.category) { metaConditions += " AND m.category = ?"; metaParams.push(filter.category); }
            if (filter.createdAfter) { metaConditions += " AND m.created_at >= ?"; metaParams.push(filter.createdAfter); }
            if (filter.createdBefore) { metaConditions += " AND m.created_at <= ?"; metaParams.push(filter.createdBefore); }
        }

        // 2. Get FTS5 Text search results
        let ftsRows: IFTSSearchRow[] = [];
        try {
            // Escape double quotes in queryText to avoid FTS5 syntax errors
            const escapedQuery = queryText.replace(/"/g, '""');
            // Support simple prefix matching by adding * and quoting words
            const cleanQuery = escapedQuery.trim().split(/\s+/).filter(Boolean).map(word => `"${word}"*`).join(" AND ");
            
            ftsRows = await this.#db.prepare(`
                SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids
                FROM vectors_fts f
                INNER JOIN vectors_meta m ON m.id = f.rowid
                WHERE f.content MATCH ? AND ${metaConditions}
                LIMIT ?
            `).all(cleanQuery, ...metaParams, topK * 3) as unknown as IFTSSearchRow[];
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/Vec] FTS5 search failed: ${errMsg}. Falling back to simple query...`);
            try {
                ftsRows = await this.#db.prepare(`
                    SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids
                    FROM vectors_fts f
                    INNER JOIN vectors_meta m ON m.id = f.rowid
                    WHERE f.content MATCH ? AND ${metaConditions}
                    LIMIT ?
                `).all(queryText, ...metaParams, topK * 3) as unknown as IFTSSearchRow[];
            } catch {
                ftsRows = [];
            }
        }

        let ftsResults = ftsRows.map(r => ({
            id: r.rowid,
            vecId: r.vec_id,
            content: r.content,
            type: r.type,
            domain: r.domain,
            category: r.category,
            traceKeywords: JSON.parse(r.trace_keywords || '[]') as string[],
            sourceEventIds: (() => {
                try {
                    const raw = JSON.parse(r.source_event_ids || '[]');
                    const parsed = EventIdsSchema.safeParse(raw);
                    return parsed.success ? parsed.data : [];
                } catch {
                    return [];
                }
            })(),
        }));

        if (filter?.type) {
            ftsResults = ftsResults.filter(r => r.type === filter.type);
        }

        // 3. Perform Reciprocal Rank Fusion (RRF)
        const rrfMap = new Map<string, {
            id: number;
            vecId: string;
            content: string;
            type: string;
            domain: string;
            category: string;
            traceKeywords: string[];
            sourceEventIds: string[];
            score: number;
        }>();

        const K = 60; // Standard RRF constant

        // Add Vector Ranks
        vectorResults.forEach((item, index) => {
            const rank = index + 1;
            const score = 1 / (K + rank);
            rrfMap.set(item.vecId, {
                id: item.id,
                vecId: item.vecId,
                content: item.content,
                type: item.type,
                domain: item.domain,
                category: item.category,
                traceKeywords: item.traceKeywords,
                sourceEventIds: item.sourceEventIds,
                score: score
            });
        });

        // Add FTS Ranks
        ftsResults.forEach((item, index) => {
            const rank = index + 1;
            const score = 1 / (K + rank);
            const existing = rrfMap.get(item.vecId);
            if (existing) {
                existing.score += score;
            } else {
                rrfMap.set(item.vecId, {
                    id: item.id,
                    vecId: item.vecId,
                    content: item.content,
                    type: item.type,
                    domain: item.domain,
                    category: item.category,
                    traceKeywords: item.traceKeywords,
                    sourceEventIds: item.sourceEventIds,
                    score: score
                });
            }
        });

        // Sort by RRF score descending
        const sorted = Array.from(rrfMap.values())
            .sort((a, b) => b.score - a.score);

        const slice = sorted.slice(0, topK);
        for (const item of slice) {
            this.touchVector(item.vecId);
        }
        return slice;
    }

    /**
     * Buffer a vector touch in RAM to prevent write amplification.
     */
    public touchVector(vecId: string): void {
        this.#vectorTouchBuffer.set(vecId, Date.now());
        if (!this.#vectorTouchTimer) {
            this.#vectorTouchTimer = setTimeout(() => {
                this.flushVectorTouches().catch(err => {
                    logger.warn(`[VectorRepository] Background flush touches failed: ${err.message}`);
                });
            }, VectorRepository.VECTOR_TOUCH_FLUSH_MS);
            this.#vectorTouchTimer.unref();
        }
    }

    /**
     * Flush buffered vector touches to SQLite in a single transaction.
     */
    public async flushVectorTouches(): Promise<void> {
        if (this.#vectorTouchBuffer.size === 0) return;
        const entries = Array.from(this.#vectorTouchBuffer.entries());
        this.#vectorTouchBuffer.clear();
        if (this.#vectorTouchTimer) {
            clearTimeout(this.#vectorTouchTimer);
            this.#vectorTouchTimer = null;
        }

        try {
            const statements = [{
                sql: "UPDATE vectors_meta SET decay_weight = 1.0, last_accessed_at = ?, access_count = access_count + 1 WHERE vec_id = ?",
                paramSets: entries.map(([vecId, ts]) => [ts, vecId])
            }];
            await this.#db.transactionBatch(statements);
        } catch (e: unknown) {
            // Re-queue failed touches
            for (const [vecId, ts] of entries) {
                this.#vectorTouchBuffer.set(vecId, ts);
            }
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[VectorRepository] Failed to flush vector touches: ${errMsg}`);
        }
    }

    /**
     * Apply touch-reinforced spaced repetition memory decay to all vectors in L2.
     * Decayed vectors with weight below ARCHIVE_THRESHOLD (0.15) will be deleted.
     */
    public async applyVectorDecay(decayRate: number = 0.1): Promise<{ decayed: number; archived: number }> {
        if (!this.#vecReady) return { decayed: 0, archived: 0 };
        
        await this.flushVectorTouches();

        const now = Date.now();
        const MS_PER_DAY = 86_400_000;
        const ARCHIVE_THRESHOLD = 0.15;
        const CHUNK_SIZE = 500;
        const k = 0.1; // Reinforcement coefficient

        const fetchStmt = this.#db.prepare(`
            SELECT id, vec_id, content, decay_weight, last_accessed_at, created_at, access_count 
            FROM vectors_meta
            WHERE (
                (last_accessed_at > 0 AND last_accessed_at < ?)
                OR (last_accessed_at <= 0 AND created_at < ?)
            )
            AND id > ?
            ORDER BY id ASC
            LIMIT ?
        `);

        let totalDecayed = 0;
        let totalArchived = 0;
        let lastId = 0;

        while (true) {
            const timeThreshold = now - MS_PER_DAY;
            const vectors = await fetchStmt.all(timeThreshold, timeThreshold, lastId, CHUNK_SIZE) as Array<{ id: number; vec_id: string; content: string; decay_weight: number; last_accessed_at: number; created_at: number; access_count: number }>;
            
            if (vectors.length === 0) break;

            const toUpdate: Array<{ id: number; weight: number }> = [];
            const toDelete: number[] = [];

            for (const vec of vectors) {
                const baseTime = vec.last_accessed_at && vec.last_accessed_at > 0 ? vec.last_accessed_at : vec.created_at;
                const daysSince = (now - baseTime) / MS_PER_DAY;
                lastId = vec.id; // Update lastId for keyset pagination

                if (daysSince < 1) continue; // Skip recently accessed/created

                // Dynamic spaced repetition decay: S(t) = S0 * e^(- (lambda0 / (1 + k * n)) * t)
                const lambda = decayRate / (1 + k * (vec.access_count || 0));
                const newWeight = (vec.decay_weight ?? 1.0) * Math.exp(-lambda * daysSince);

                if (newWeight < ARCHIVE_THRESHOLD) {
                    toDelete.push(vec.id);
                } else if (Math.abs(newWeight - (vec.decay_weight ?? 1.0)) > 0.01) {
                    toUpdate.push({ id: vec.id, weight: newWeight });
                }
            }

            if (toUpdate.length > 0 || toDelete.length > 0) {
                const statements: Array<{ sql: string; paramSets: any[][] }> = [];
                if (toUpdate.length > 0) {
                    statements.push({
                        sql: "UPDATE vectors_meta SET decay_weight = ? WHERE id = ?",
                        paramSets: toUpdate.map(u => [u.weight, BigInt(u.id)])
                    });
                }
                if (toDelete.length > 0) {
                    statements.push({
                        sql: "DELETE FROM vec_idx WHERE rowid = ?",
                        paramSets: toDelete.map(id => [BigInt(id)])
                    });
                    statements.push({
                        sql: "DELETE FROM vectors_meta WHERE id = ?",
                        paramSets: toDelete.map(id => [BigInt(id)])
                    });
                    statements.push({
                        sql: "DELETE FROM vectors_fts WHERE rowid = ?",
                        paramSets: toDelete.map(id => [BigInt(id)])
                    });
                }
                await this.#db.transactionBatch(statements);
                totalDecayed += toUpdate.length;
                totalArchived += toDelete.length;
            }

            if (vectors.length < CHUNK_SIZE) break;
            await new Promise(resolve => setImmediate(resolve));
        }

        return { decayed: totalDecayed, archived: totalArchived };
    }
}
