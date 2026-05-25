import { DatabaseSync } from "node:sqlite";
import * as sqliteVec from "sqlite-vec";
import { logger } from "../utils/logger";
import { z } from "zod";
import { safeExtractJSON } from "../utils/JsonExtractor";

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
 * VectorRepository — Extracted sqlite-vec operations from StructuredMemory.
 *
 * Encapsulates all vector storage logic:
 *   - sqlite-vec extension loading + dimension management
 *   - Vector CRUD (upsert, search, delete)
 *   - KNN similarity search with post-filtering
 *   - Dead Letter Queue (DLQ) for failed delete retries
 *
 * Uses a shared DatabaseSync instance (owned by StructuredMemory).
 * Does NOT create or close the connection — lifecycle managed externally.
 */

export class VectorRepository {
    readonly #db: DatabaseSync;
    #vecDimension: number = 384;
    #vecReady: boolean = false;

    constructor(db: DatabaseSync) {
        this.#db = db;
    }

    /**
     * Initialize sqlite-vec extension and create vector tables.
     * Dimension is resolved from EmbeddingService at runtime via initVecDimension().
     * Default: 384D (all-MiniLM-L6-v2).
     */
    public init(): void {
        try {
            // Load sqlite-vec native extension
            sqliteVec.load(this.#db);

            // Metadata table for vector records
            this.#db.exec(`
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
            this.#db.exec(`
                CREATE VIRTUAL TABLE IF NOT EXISTS vectors_fts USING fts5(
                    content,
                    tokenize='porter'
                )
            `);

            // Detect dimension from existing table or use default
            this.#vecDimension = this.#detectOrCreateVecTable();
            this.#vecReady = true;

            const count = (this.#db.prepare('SELECT count(*) as c FROM vectors_meta').get() as ICountRow | undefined)?.c ?? 0;
            logger.info(`[StructuredMemory/Vec] ✅ sqlite-vec loaded (${this.#vecDimension}D, ${count} vectors).`);

            // [UHM] Positional Index: add source_event_ids column (idempotent)
            try { this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN source_event_ids TEXT DEFAULT '[]'"); } catch { /* already exists */ }
            try { this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN decay_weight REAL DEFAULT 1.0"); } catch { /* already exists */ }
            try { this.#db.exec("ALTER TABLE vectors_meta ADD COLUMN access_count INTEGER DEFAULT 0"); } catch { /* already exists */ }

            // Backfill existing meta records into vectors_fts if empty
            const ftsCount = (this.#db.prepare('SELECT count(*) as c FROM vectors_fts').get() as ICountRow | undefined)?.c ?? 0;
            if (ftsCount === 0 && count > 0) {
                logger.info(`[StructuredMemory/Vec] Backfilling ${count} existing vectors into FTS5 virtual table...`);
                this.#db.exec(`
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
    #detectOrCreateVecTable(): number {
        const existing = this.#db.prepare(
            "SELECT sql FROM sqlite_master WHERE type='table' AND name='vec_idx'"
        ).get() as { sql: string } | undefined;

        // Tự động Migrate từ Float sang INT8 nếu tồn tại bảng float cũ
        if (existing && existing.sql) {
            const isFloat = existing.sql.includes('float[');
            const isInt8 = existing.sql.includes('int8[');
            
            const match = existing.sql.match(/(?:float|int8)\[(\d+)\]/);
            const dim = match ? parseInt(match[1], 10) : this.#vecDimension;

            if (isFloat) {
                logger.info(`[StructuredMemory/Vec] Detected old Float32 vec_idx (${dim}D). Migrating to INT8 Quantization...`);
                
                this.#db.exec(`DROP TABLE IF EXISTS vec_idx_new`);
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx_new USING vec0(embedding int8[${dim}])`);
                
                // Migrate data using sqlite-vec built-in quantizer
                this.#db.exec(`INSERT INTO vec_idx_new(rowid, embedding) SELECT rowid, vec_quantize_int8(embedding, 'unit') FROM vec_idx`);
                
                // Double copy to avoid RENAME TO shadow table bugs in sqlite-vec
                this.#db.exec('DROP TABLE vec_idx');
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dim}])`);
                this.#db.exec(`INSERT INTO vec_idx(rowid, embedding) SELECT rowid, embedding FROM vec_idx_new`);
                this.#db.exec(`DROP TABLE vec_idx_new`);
                
                logger.info(`[StructuredMemory/Vec] ✅ Migration to INT8 complete. RAM footprint reduced by 75%.`);
            } else if (isInt8) {
                logger.info(`[StructuredMemory/Vec] Detected INT8 vec_idx dimension: ${dim}D`);
            }

            return dim;
        }

        this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${this.#vecDimension}])`);
        return this.#vecDimension;
    }

    /**
     * Set the embedding dimension. Must be called before first vector insert
     * if dimension differs from default 384. Recreates vec_idx if dimension changed.
     */
    public initVecDimension(dimension: number): void {
        if (dimension === this.#vecDimension && this.#vecReady) return;

        const oldDim = this.#vecDimension;
        this.#vecDimension = dimension;

        const existing = this.#db.prepare(
            "SELECT name FROM sqlite_master WHERE type='table' AND name='vec_idx'"
        ).get() as INameRow | undefined;

        if (existing) {
            const vecCount = (this.#db.prepare('SELECT count(*) as c FROM vec_idx').get() as ICountRow | undefined)?.c ?? 0;
            if (vecCount === 0 && oldDim !== dimension) {
                this.#db.exec('DROP TABLE vec_idx');
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
                logger.info(`[StructuredMemory/Vec] Recreated vec_idx: ${oldDim}D → ${dimension}D`);
            } else if (oldDim !== dimension && vecCount > 0) {
                logger.warn(`[StructuredMemory/Vec] Dimension mismatch (${oldDim}→${dimension}) with ${vecCount} existing vectors. Re-embedding required.`);
                this.#db.exec('DELETE FROM vec_idx');
                this.#db.exec('DELETE FROM vectors_meta');
                this.#db.exec('DROP TABLE vec_idx');
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
                logger.warn(`[StructuredMemory/Vec] Cleared all vectors. ConsolidationCron will re-embed from L1.`);
            }
        } else {
            this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding int8[${dimension}])`);
        }

        this.#vecReady = true;
    }

    // ===========================
    // L2 Vector CRUD Operations
    // ===========================

    public upsertVector(record: {
        vecId: string;
        type: string;
        content: string;
        vector: number[];
        domain?: string;
        category?: string;
        traceKeywords?: string[];
        fileTarget?: string;
        sourceEventIds?: string[];  // [UHM] L2→L1 positional pointers (max 50)
    }): void {
        if (!this.#vecReady) return;

        // [G4] Cap at 50 entries to prevent RAM overflow
        const eventIds = JSON.stringify((record.sourceEventIds ?? []).slice(0, 50));

        const existing = this.#db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(record.vecId) as IIdRow | undefined;

        if (existing) {
            this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(existing.id));
            this.#db.prepare(`
                UPDATE vectors_meta SET type=?, content=?, domain=?, category=?, trace_keywords=?, file_target=?, source_event_ids=?, last_accessed_at=?, decay_weight=1.0, access_count=access_count+1
                WHERE vec_id=?
            `).run(
                record.type, record.content,
                record.domain ?? 'General', record.category ?? 'Uncategorized',
                JSON.stringify(record.traceKeywords ?? []), record.fileTarget ?? null,
                eventIds, Date.now(), record.vecId
            );
            const blob = new Uint8Array(new Float32Array(record.vector).buffer);
            // Dùng vec_quantize_int8 để chuyển Float32 -> INT8 trong C++ SQLite
            this.#db.prepare('INSERT INTO vec_idx(rowid, embedding) VALUES (?, vec_quantize_int8(?, \'unit\'))').run(BigInt(existing.id), blob);

            // Synchronize with FTS5 virtual table
            try {
                this.#db.prepare(`
                    INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES (?, ?)
                `).run(BigInt(existing.id), record.content);
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[StructuredMemory/Vec] Failed to sync FTS5 on update: ${errMsg}`);
            }
        } else {
            this.#db.prepare(`
                INSERT INTO vectors_meta (vec_id, type, content, domain, category, trace_keywords, file_target, source_event_ids, created_at, last_accessed_at, decay_weight, access_count)
                VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, 1.0, 0)
            `).run(
                record.vecId, record.type, record.content,
                record.domain ?? 'General', record.category ?? 'Uncategorized',
                JSON.stringify(record.traceKeywords ?? []), record.fileTarget ?? null,
                eventIds, Date.now()
            );
            const row = this.#db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(record.vecId) as IIdRow | undefined;
            if (!row) return;
            const blob = new Uint8Array(new Float32Array(record.vector).buffer);
            // Dùng vec_quantize_int8 để chuyển Float32 -> INT8 trong C++ SQLite
            this.#db.prepare('INSERT INTO vec_idx(rowid, embedding) VALUES (?, vec_quantize_int8(?, \'unit\'))').run(BigInt(row.id), blob);

            // Synchronize with FTS5 virtual table
            try {
                this.#db.prepare(`
                    INSERT OR REPLACE INTO vectors_fts (rowid, content) VALUES (?, ?)
                `).run(BigInt(row.id), record.content);
            } catch (e: unknown) {
                const errMsg = e instanceof Error ? e.message : String(e);
                logger.warn(`[StructuredMemory/Vec] Failed to sync FTS5 on insert: ${errMsg}`);
            }
        }
    }

    public upsertVectorsBatch(records: Array<{
        vecId: string;
        type: string;
        content: string;
        vector: number[];
        domain?: string;
        category?: string;
        traceKeywords?: string[];
        sourceEventIds?: string[];
    }>): void {
        if (!this.#vecReady || records.length === 0) return;
        this.#db.exec("BEGIN");
        try {
            for (const record of records) {
                this.upsertVector(record);
            }
            this.#db.exec("COMMIT");
        } catch (e: unknown) {
            try { this.#db.exec("ROLLBACK"); } catch { /* ignore */ }
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/Vec] Batch upsert transaction rolled back: ${errMsg}`);
            throw e;
        }
    }



    public searchSimilarVectors(
        queryVector: number[],
        topK: number = 5,
        filter?: MetadataFilter
    ): Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; score: number; traceKeywords: string[]; sourceEventIds: string[] }> {
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

        const rows = this.#db.prepare(sql).all(blob, fetchK, ...metaParams) as unknown as IVecSearchRow[];

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

        return results.slice(0, topK);
    }

    public searchAnchors(queryVector: number[], limit: number = 5): string[] {
        return this.searchSimilarVectors(queryVector, limit, { type: 'ANCHOR' })
            .map(r => r.content);
    }

    public searchAnchorsWithScores(queryVector: number[], limit: number = 5): Array<{ content: string; score: number }> {
        return this.searchSimilarVectors(queryVector, limit, { type: 'ANCHOR' })
            .map(r => ({ content: r.content, score: r.score }));
    }

    public searchAxiomsByVector(queryVector: number[], limit: number = 3): Array<{ text: string; traceKeywords: string }> {
        return this.searchSimilarVectors(queryVector, limit, { type: 'AXIOM' })
            .map(r => ({ text: r.content, traceKeywords: JSON.stringify(r.traceKeywords) }));
    }

    /**
     * [UHM] Search L2 vectors and collect source event IDs for L1 drill-down.
     * [G3] Uses json_each() — avoids SQLite 999 variable limit.
     * [G4] Caps at 50 event IDs per vector, validates JSON safely.
     */
    public searchWithDrilldown(
        queryVector: number[],
        topK: number = 3,
        typeFilter?: string
    ): Array<{ vecId: string; content: string; type: string; distance: number; sourceEventIds: string[] }> {
        const results = this.searchSimilarVectors(queryVector, topK, typeFilter ? { type: typeFilter } : undefined);
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
    public collectDrilldownEventIds(
        queryVector: number[],
        topK: number = 3,
        typeFilter?: string
    ): string[] {
        const results = this.searchWithDrilldown(queryVector, topK, typeFilter);
        const allIds = new Set<string>();
        for (const r of results) {
            for (const id of r.sourceEventIds) {
                allIds.add(id);
            }
        }
        return [...allIds];
    }

    public deleteVectorByContent(content: string): void {
        if (!this.#vecReady) return;
        const row = this.#db.prepare('SELECT id FROM vectors_meta WHERE content = ?').get(content) as IIdRow | undefined;
        if (row) {
            this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(row.id));
            this.#db.prepare('DELETE FROM vectors_meta WHERE id = ?').run(BigInt(row.id));
            try {
                this.#db.prepare('DELETE FROM vectors_fts WHERE rowid = ?').run(BigInt(row.id));
            } catch { /* ignore */ }
        }
    }

    public deleteVectorById(vecId: string): void {
        if (!this.#vecReady) return;
        const row = this.#db.prepare('SELECT id FROM vectors_meta WHERE vec_id = ?').get(vecId) as IIdRow | undefined;
        if (row) {
            this.#db.prepare('DELETE FROM vec_idx WHERE rowid = ?').run(BigInt(row.id));
            this.#db.prepare('DELETE FROM vectors_meta WHERE id = ?').run(BigInt(row.id));
            try {
                this.#db.prepare('DELETE FROM vectors_fts WHERE rowid = ?').run(BigInt(row.id));
            } catch { /* ignore */ }
        }
    }

    public deleteAllVectors(): void {
        if (!this.#vecReady) return;
        this.#db.exec('DELETE FROM vec_idx');
        this.#db.exec('DELETE FROM vectors_meta');
        this.#db.exec('DELETE FROM vectors_fts');
        logger.warn('[StructuredMemory/Vec/GDPR] All vectors permanently erased.');
    }

    public get vectorCount(): number {
        if (!this.#vecReady) return 0;
        return (this.#db.prepare('SELECT count(*) as c FROM vectors_meta').get() as ICountRow | undefined)?.c ?? 0;
    }

    public get vecReady(): boolean {
        return this.#vecReady;
    }

    // ===========================
    // DLQ — Compensating Transactions
    // ===========================

    public pushToDLQ(filter: string): void {
        try {
            this.#db.prepare(
                "INSERT INTO vector_dlq (delete_filter, status, retry_count) VALUES (?, 'pending', 0)"
            ).run(filter);
            logger.warn('[StructuredMemory/DLQ] Queued failed delete filter for retry.');
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/DLQ] Failed to push: ${errMsg}`);
        }
    }

    public processDLQ(): void {
        try {
            const rows = this.#db.prepare(
                "SELECT id, delete_filter, retry_count FROM vector_dlq WHERE status = 'pending'"
            ).all() as Array<{ id: number; delete_filter: string; retry_count: number }>;
            for (const row of rows) {
                if (row.retry_count >= 3) {
                    this.#db.prepare("UPDATE vector_dlq SET status = 'dead_letter' WHERE id = ?").run(row.id);
                    logger.warn(`[StructuredMemory/DLQ] Marked entry ${row.id} as dead_letter.`);
                    continue;
                }
                try {
                    this.deleteVectorByContent(row.delete_filter);
                    this.#db.prepare('DELETE FROM vector_dlq WHERE id = ?').run(row.id);
                    logger.info(`[StructuredMemory/DLQ] Cleaned entry ${row.id}.`);
                } catch {
                    this.#db.prepare('UPDATE vector_dlq SET retry_count = retry_count + 1 WHERE id = ?').run(row.id);
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
    public searchHybridVectors(
        queryText: string,
        queryVector: number[],
        topK: number = 5,
        filter?: MetadataFilter
    ): Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; score: number; traceKeywords: string[]; sourceEventIds: string[] }> {
        if (!this.#vecReady) return [];

        // 1. Get Vector KNN search results (Pre-filtered)
        const vectorResults = this.searchSimilarVectors(queryVector, topK * 3, filter);

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
            // Support simple prefix matching by adding *
            const cleanQuery = escapedQuery.trim().split(/\s+/).map(word => `${word}*`).join(" AND ");
            
            ftsRows = this.#db.prepare(`
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
                ftsRows = this.#db.prepare(`
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

        return sorted.slice(0, topK);
    }
}
