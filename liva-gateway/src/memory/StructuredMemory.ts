import { safeRename } from '../utils/FileUtils';
import { DatabaseSync } from "node:sqlite";
import { promises as fsp, mkdirSync } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { VectorRepository } from "./VectorRepository";
import { EventRepository } from "./EventRepository";
import { GraphRepository } from "./GraphRepository";
import type { EventBrick, TurnNode } from "./EventRepository";
import { DatabaseWorkerBridge } from "./DatabaseWorkerBridge";
import * as sqliteVec from "sqlite-vec";
import type { PersonalityState } from "./PersonalityEvolution";

// Re-export types so existing callers don't need to change imports
export type { EventBrick, TurnNode } from "./EventRepository";
export type { PersonalityState } from "./PersonalityEvolution";
import type { IDBCountRow, IDBFactRow, IDBEventRow, StructuredFact } from "./structured/types";
export type { IDBCountRow, IDBFactRow, IDBEventRow, StructuredFact };

import { MemoryIO } from "./structured/MemoryIO";
import { MemorySearch } from "./structured/MemorySearch";
import { MemoryConsolidator } from "./structured/MemoryConsolidator";
import { initStore } from "./structured/MemorySchema";
import { createMemory } from "./structured/MemoryFactory";



export class StructuredMemory {
    private static readonly instances = new Map<string, StructuredMemory>();
    private readonly storePath: string;
    public readonly db: DatabaseSync;
    public readonly dbBridge: DatabaseWorkerBridge;
    #evictionTimer: NodeJS.Timeout | null = null;
    #initPromise: Promise<void> | null = null;
    #isInitialized = false;
    #isClosed = false;

    // [Phase 3.3] Extracted repositories
    readonly #vectorRepo: VectorRepository;
    readonly #eventRepo: EventRepository;
    readonly #graphRepo: GraphRepository;

    public readonly agentId: string;

    // [v19] Preserved static constants for backward compat
    static readonly TOUCH_QUEUE_CAPACITY = EventRepository.TOUCH_QUEUE_CAPACITY;
    static readonly TOUCH_EARLY_FLUSH = EventRepository.TOUCH_EARLY_FLUSH;
    static readonly TOUCH_FLUSH_INTERVAL_MS = EventRepository.TOUCH_FLUSH_INTERVAL_MS;
    static readonly FACT_TOUCH_FLUSH_MS = 60_000;

    // Sub-module components
    readonly #io: MemoryIO;
    readonly #search: MemorySearch;
    readonly #consolidator: MemoryConsolidator;

    constructor(storePath: string, agentId: string = "liva_core") {
        this.storePath = storePath;
        this.agentId = agentId;

        // [Fix Issue 5] Ensure parent directory exists before opening database
        if (this.storePath !== ":memory:") {
            mkdirSync(path.dirname(this.storePath), { recursive: true });
        }

        // Connect to SQLite with extension loading enabled for sqlite-vec
        this.db = new DatabaseSync(this.storePath, { allowExtension: true });
        sqliteVec.load(this.db);
        this.db.exec("PRAGMA journal_mode = WAL");
        this.db.exec("PRAGMA busy_timeout = 5000");
        initStore(this.db, this.agentId);

        // Instantiate dbBridge
        this.dbBridge = new DatabaseWorkerBridge(this.storePath, { allowExtension: true });

        // [Phase 3.3] Initialize extracted repositories (shared DB connection)
        this.#vectorRepo = new VectorRepository(this.dbBridge);
        this.#eventRepo = new EventRepository(this.dbBridge, this.agentId);
        this.#graphRepo = new GraphRepository(this.dbBridge);

        // Instantiate sub-modules
        this.#io = new MemoryIO(this);
        this.#search = new MemorySearch(this, this.#vectorRepo);
        this.#consolidator = new MemoryConsolidator(this, this.#eventRepo, this.#graphRepo, this.#io, this.#search);

        this.#evictionTimer = setInterval(() => {
            (async () => {
                try {
                    await this.evictExpired();
                } catch { /* non-critical */ }
            })();
        }, 60_000);
        this.#evictionTimer.unref();
    }

    public get isClosed(): boolean {
        return this.#isClosed;
    }

    /**
     * Async Factory — ensures directory exists and migrates legacy JSON
     * without blocking the Event Loop.
     */
    static async create(agentId: string = "liva_core", customStorePath?: string): Promise<StructuredMemory> {
        return createMemory(
            agentId,
            customStorePath,
            StructuredMemory.instances,
            (storePath, aid) => new StructuredMemory(storePath, aid)
        );
    }

    public async initialize(): Promise<void> {
        if (this.#isInitialized) return;
        if (!this.#initPromise) {
            this.#initPromise = (async () => {
                await this.dbBridge.initialize();

                // Initialize repositories
                await this.#vectorRepo.init();
                await this.#graphRepo.init();

                // Start Memory Touch debounce timer (delegated to EventRepository)
                this.#eventRepo.startTouchDebounce();

                // [H-MEM] Self-healing mechanism (Cơ chế tự phục hồi FTS5 sau khi bị ép tắt)
                try {
                    try { this.db.prepare('SELECT 1 FROM vectors_fts LIMIT 1').get(); } catch {}
                    
                    const checkResults = this.db.prepare("PRAGMA integrity_check").all() as Array<{ integrity_check: string }>;
                    
                    const hasError = checkResults.some(r => r.integrity_check !== 'ok');
                    if (hasError) {
                        logger.warn(`[H-MEM] Database corruption detected: ${JSON.stringify(checkResults)}. Initiating FTS5 rebuild...`);
                        this.db.exec("INSERT INTO vectors_fts(vectors_fts) VALUES('rebuild');");
                        logger.info("[H-MEM] FTS5 index rebuilt successfully. Data integrity restored.");
                    }
                } catch (e: unknown) {
                    const errMsg = e instanceof Error ? e.message : String(e);
                    logger.warn(`[H-MEM] Self-healing check failed: ${errMsg}`);
                }

                this.#isInitialized = true;
            })();
        }
        return this.#initPromise;
    }

    public async ensureInitialized(): Promise<void> {
        if (this.#isInitialized) return;
        if (this.#initPromise) {
            await this.#initPromise;
            return;
        }
        await this.initialize();
    }

    public getDbBridge(): DatabaseWorkerBridge {
        return this.dbBridge;
    }



    // ===========================
    // MemoryIO Delegated Methods
    // ===========================

    public getFact(key: string): StructuredFact | null { return this.#io.getFact(key); }
    public async setFact(key: string, value: string, options: { ttlDays?: number; source?: string; category?: string } = {}): Promise<void> { return this.#io.setFact(key, value, options); }
    public async setFactsBatch(facts: Array<{ key: string; value: string; options?: { ttlDays?: number; source?: string; category?: string } }>): Promise<void> { return this.#io.setFactsBatch(facts); }
    public async deleteFact(key: string): Promise<boolean> { return this.#io.deleteFact(key); }
    public getAllFacts(): StructuredFact[] { return this.#io.getAllFacts(); }
    public getFactsByCategory(category: string): StructuredFact[] { return this.#io.getFactsByCategory(category); }
    public get count(): number { return this.#io.count; }
    public async setFactImportance(key: string, importance: number): Promise<void> { return this.#io.setFactImportance(key, importance); }
    public async deleteAllFacts(): Promise<void> { return this.#io.deleteAllFacts(); }
    public touchFact(key: string): void { this.#io.touchFact(key); }
    public async flushFactTouches(): Promise<void> { return this.#io.flushFactTouches(); }
    private async evictExpired(): Promise<void> { return this.#io.evictExpired(); }
    public async migrateFromJson(jsonPath: string): Promise<void> { return this.#io.migrateFromJson(jsonPath); }
    public formatForSystemPrompt(): string { return this.#io.formatForSystemPrompt(); }
    public saveBriefing(briefing: { id: string; topics: string; content: string; source?: string; ttlHours?: number }): void { this.#io.saveBriefing(briefing); }
    public getUnreadBriefings(limit: number = 5): Array<{ id: string; topics: string; content: string; created_at: number }> { return this.#io.getUnreadBriefings(limit); }
    public markBriefingRead(id: string): void { this.#io.markBriefingRead(id); }
    public cleanExpiredBriefings(): number { return this.#io.cleanExpiredBriefings(); }
    public getTasks(): Array<{ id: string; title: string; description: string; status: string; priority: string; result: string; created_at: number; updated_at: number }> { return this.#io.getTasks(); }
    public addTask(task: { id: string; title: string; description?: string; priority?: string }): void { this.#io.addTask(task); }
    public updateTask(id: string, updates: { status?: string; result?: string; title?: string; description?: string; priority?: string }): void { this.#io.updateTask(id, updates); }
    public deleteTask(id: string): void { this.#io.deleteTask(id); }

    // ===========================
    // MemorySearch Delegated Methods
    // ===========================

    public async initVecDimension(dimension: number): Promise<void> { return this.#search.initVecDimension(dimension); }
    public upsertVector(record: { vecId: string; type: string; content: string; vector: number[]; domain?: string; category?: string; traceKeywords?: string[]; fileTarget?: string; sourceEventIds?: string[] }): void { this.#search.upsertVector(record); }
    public async flushVectorQueue(): Promise<void> { return this.#search.flushVectorQueue(); }
    public async upsertVectorsBatch(records: Array<{ vecId: string; type: string; content: string; vector: number[]; domain?: string; category?: string; traceKeywords?: string[]; sourceEventIds?: string[] }>): Promise<void> { return this.#search.upsertVectorsBatch(records); }
    public async searchSimilarVectors(queryVector: number[], topK?: number, typeFilter?: string): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> { return this.#search.searchSimilarVectors(queryVector, topK, typeFilter); }
    public async searchHybridVectors(queryText: string, queryVector: number[], topK?: number, typeFilter?: string): Promise<Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; score: number; traceKeywords: string[]; sourceEventIds: string[] }>> { return this.#search.searchHybridVectors(queryText, queryVector, topK, typeFilter); }
    public async searchAnchors(queryVector: number[], limit?: number): Promise<string[]> { return this.#search.searchAnchors(queryVector, limit); }
    public async searchAnchorsWithScores(queryVector: number[], limit?: number, queryText?: string): Promise<Array<{ content: string; score: number }>> { return this.#search.searchAnchorsWithScores(queryVector, limit, queryText); }
    public async searchAxiomsByVector(queryVector: number[], limit?: number): Promise<Array<{ text: string; traceKeywords: string }>> { return this.#search.searchAxiomsByVector(queryVector, limit); }
    public async searchWithDrilldown(queryVector: number[], topK?: number, typeFilter?: string) { return this.#search.searchWithDrilldown(queryVector, topK, typeFilter); }
    public async collectDrilldownEventIds(queryVector: number[], topK?: number, typeFilter?: string): Promise<string[]> { return this.#search.collectDrilldownEventIds(queryVector, topK, typeFilter); }
    public async deleteVectorByContent(content: string): Promise<void> { return this.#search.deleteVectorByContent(content); }
    public async deleteVectorById(vecId: string): Promise<void> { return this.#search.deleteVectorById(vecId); }
    public async deleteAllVectors(): Promise<void> { return this.#search.deleteAllVectors(); }
    public async getVectorCount(): Promise<number> { return this.#search.getVectorCount(); }
    public get vecReady(): boolean { return this.#search.vecReady; }
    public async pushToDLQ(filter: string): Promise<void> { return this.#search.pushToDLQ(filter); }
    public async processDLQ(): Promise<void> { return this.#search.processDLQ(); }
    public async flushVectorTouches(): Promise<void> { return this.#search.flushVectorTouches(); }

    // ===========================
    // MemoryConsolidator Delegated Methods
    // ===========================

    public queueMemoryTouch(eventId: string): void { this.#consolidator.queueMemoryTouch(eventId); }
    public async flushTouchQueue(): Promise<void> { return this.#consolidator.flushTouchQueue(); }
    public async insertEvent(event: EventBrick): Promise<void> { return this.#consolidator.insertEvent(event); }
    public getPersonalityStateSync(): PersonalityState { return this.#consolidator.getPersonalityStateSync(); }
    public async getPersonalityState(): Promise<PersonalityState> { return this.#consolidator.getPersonalityState(); }
    public async updatePersonalityState(state: Partial<PersonalityState>): Promise<void> { return this.#consolidator.updatePersonalityState(state); }
    public async getUnconsolidatedEvents(): Promise<EventBrick[]> { return this.#consolidator.getUnconsolidatedEvents(); }
    public async getUnconsolidatedCount(): Promise<number> { return this.#consolidator.getUnconsolidatedCount(); }
    public async markConsolidated(eventIds: string[]): Promise<void> { return this.#consolidator.markConsolidated(eventIds); }
    public async markDLQ(eventIds: string[]): Promise<void> { return this.#consolidator.markDLQ(eventIds); }
    public async incrementRetryCount(eventIds: string[]): Promise<void> { return this.#consolidator.incrementRetryCount(eventIds); }
    public async gcOldEvents(retentionDays?: number): Promise<number> { return this.#consolidator.gcOldEvents(retentionDays); }
    public async deleteAllEvents(): Promise<void> { return this.#consolidator.deleteAllEvents(); }
    public async insertTurnNode(turnId: string, temporal_anchor: number, userMsg: string, aiReply: string): Promise<void> { return this.#consolidator.insertTurnNode(turnId, temporal_anchor, userMsg, aiReply); }
    public async getTurnsByTimeRange(fromTs: number, toTs: number): Promise<TurnNode[]> { return this.#consolidator.getTurnsByTimeRange(fromTs, toTs); }
    public async getTurnsByIds(turnIds: string[]): Promise<TurnNode[]> { return this.#consolidator.getTurnsByIds(turnIds); }
    get graph() { return this.#consolidator.graph; }
    public async applyMemoryDecay(decayRate: number = 0.1): Promise<{ decayed: number; archived: number }> { return this.#consolidator.applyMemoryDecay(decayRate); }

    // ===========================
    // DB Transaction Management (for ConsolidationPipeline)
    // ===========================

    public async beginTransaction(): Promise<void> { await this.dbBridge.exec("BEGIN TRANSACTION"); }
    public async commitTransaction(): Promise<void> { await this.dbBridge.exec("COMMIT"); }
    public async rollbackTransaction(): Promise<void> { try { await this.dbBridge.exec("ROLLBACK"); } catch {} }

    // ===========================
    // Lifecycle
    // ===========================

    public async close(): Promise<void> {
        this.#isClosed = true;
        try {
            // Clean up from static instances registry
            StructuredMemory.instances.delete(this.storePath);

            // Clean up timers
            if (this.#evictionTimer) {
                clearInterval(this.#evictionTimer);
                this.#evictionTimer = null;
            }

            // Ordered shutdown to prevent write loss
            await this.#io.close();
            await this.#search.close();
            await this.#consolidator.close();

            // Close sync DB handle AFTER all flushes complete
            if (this.db) {
                this.db.close();
                // @ts-expect-error - Force nulling db Sync reference on close
                this.db = null;
            }
            
            // Dispose async worker LAST
            if (this.dbBridge) {
                await this.dbBridge.dispose();
            }

            logger.info('[StructuredMemory] SQLite connection closed.');
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory] Close error (non-critical): ${errMsg}`);
        }
    }

    /**
     * [UHM-v3] Atomic Snapshot Backup via VACUUM INTO.
     */
    public async createSnapshotBackup(): Promise<void> {
        const backupPath = this.storePath + '.backup';
        const tmpPath = backupPath + '.tmp';

        try {
            try { await fsp.unlink(tmpPath); } catch { /* ENOENT ok */ }

            await this.ensureInitialized();
            await this.dbBridge.backup(tmpPath);

            await safeRename(tmpPath, backupPath);
            logger.info('[StructuredMemory] Snapshot backup created successfully.');
        } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory] Snapshot backup failed: ${msg}`);
            try { await fsp.unlink(tmpPath); } catch { /* cleanup */ }
        }
    }

    /**
     * Get the filesystem path of the SQLite database file.
     */
    public getDbPath(): string {
        return this.storePath;
    }

    /**
     * Expose raw DB handle for external operations.
     */
    public getDb(): DatabaseSync {
        return this.db;
    }
}
