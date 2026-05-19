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

        if (existing && existing.sql) {
            const match = existing.sql.match(/float\[(\d+)\]/);
            if (match) {
                const dim = parseInt(match[1], 10);
                logger.info(`[StructuredMemory/Vec] Detected existing vec_idx dimension: ${dim}D`);
                return dim;
            }
            return this.#vecDimension;
        }

        this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding float[${this.#vecDimension}])`);
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
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding float[${dimension}])`);
                logger.info(`[StructuredMemory/Vec] Recreated vec_idx: ${oldDim}D → ${dimension}D`);
            } else if (oldDim !== dimension && vecCount > 0) {
                logger.warn(`[StructuredMemory/Vec] Dimension mismatch (${oldDim}→${dimension}) with ${vecCount} existing vectors. Re-embedding required.`);
                this.#db.exec('DELETE FROM vec_idx');
                this.#db.exec('DELETE FROM vectors_meta');
                this.#db.exec('DROP TABLE vec_idx');
                this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding float[${dimension}])`);
                logger.warn(`[StructuredMemory/Vec] Cleared all vectors. ConsolidationCron will re-embed from L1.`);
            }
        } else {
            this.#db.exec(`CREATE VIRTUAL TABLE vec_idx USING vec0(embedding float[${dimension}])`);
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
            this.#db.prepare('INSERT INTO vec_idx(rowid, embedding) VALUES (?, ?)').run(BigInt(existing.id), blob);

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
            this.#db.prepare('INSERT INTO vec_idx(rowid, embedding) VALUES (?, ?)').run(BigInt(row.id), blob);

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
        typeFilter?: string
    ): Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; score: number; traceKeywords: string[]; sourceEventIds: string[] }> {
        if (!this.#vecReady) return [];

        const blob = new Uint8Array(new Float32Array(queryVector).buffer);
        const fetchK = typeFilter ? topK * 3 : topK;

        const rows = this.#db.prepare(`
            SELECT v.rowid, v.distance, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids, m.decay_weight
            FROM vec_idx v
            INNER JOIN vectors_meta m ON m.id = v.rowid
            WHERE v.embedding MATCH ? AND k = ?
        `).all(blob, fetchK) as unknown as IVecSearchRow[];

        let results = rows.map((r) => {
            const similarity = Math.max(0, (2.0 - (r.distance || 0)) / 2.0); // Normalize to 0-1
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
                    const raw = safeExtractJSON<unknown>(r.source_event_ids || '[]');
                    const parsed = EventIdsSchema.safeParse(raw);
                    return parsed.success ? parsed.data : [];
                })(),
            };
        });

        // [Ebbinghaus] Sort by finalScore descending (highest priority first)
        results.sort((a, b) => b.score - a.score);

        if (typeFilter) {
            results = results.filter(r => r.type === typeFilter);
        }

        return results.slice(0, topK);
    }

    public searchAnchors(queryVector: number[], limit: number = 5): string[] {
        return this.searchSimilarVectors(queryVector, limit, 'ANCHOR')
            .map(r => r.content);
    }

    public searchAxiomsByVector(queryVector: number[], limit: number = 3): Array<{ text: string; traceKeywords: string }> {
        return this.searchSimilarVectors(queryVector, limit, 'AXIOM')
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
        const results = this.searchSimilarVectors(queryVector, topK, typeFilter);
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
        typeFilter?: string
    ): Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; score: number; traceKeywords: string[]; sourceEventIds: string[] }> {
        if (!this.#vecReady) return [];

        // 1. Get Vector KNN search results
        const vectorResults = this.searchSimilarVectors(queryVector, topK * 3, typeFilter);

        // 2. Get FTS5 Text search results
        let ftsRows: any[] = [];
        try {
            // Escape double quotes in queryText to avoid FTS5 syntax errors
            const escapedQuery = queryText.replace(/"/g, '""');
            // Support simple prefix matching by adding *
            const cleanQuery = escapedQuery.trim().split(/\s+/).map(word => `${word}*`).join(" AND ");
            
            ftsRows = this.#db.prepare(`
                SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids
                FROM vectors_fts f
                INNER JOIN vectors_meta m ON m.id = f.rowid
                WHERE f.content MATCH ?
                LIMIT ?
            `).all(cleanQuery, topK * 3) as any[];
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/Vec] FTS5 search failed: ${errMsg}. Falling back to simple query...`);
            try {
                ftsRows = this.#db.prepare(`
                    SELECT f.rowid, m.vec_id, m.content, m.type, m.domain, m.category, m.trace_keywords, m.source_event_ids
                    FROM vectors_fts f
                    INNER JOIN vectors_meta m ON m.id = f.rowid
                    WHERE f.content MATCH ?
                    LIMIT ?
                `).all(queryText, topK * 3) as any[];
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
                const raw = safeExtractJSON<unknown>(r.source_event_ids || '[]');
                const parsed = EventIdsSchema.safeParse(raw);
                return parsed.success ? parsed.data : [];
            })(),
        }));

        if (typeFilter) {
            ftsResults = ftsResults.filter(r => r.type === typeFilter);
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
