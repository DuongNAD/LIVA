import { DatabaseSync } from "node:sqlite";
import { promises as fsp, constants as fsc } from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";
import { VectorRepository } from "./VectorRepository";
import { EventRepository } from "./EventRepository";
import type { EventBrick, TurnNode } from "./EventRepository";

// Re-export types so existing callers don't need to change imports
export type { EventBrick, TurnNode } from "./EventRepository";

/**
 * StructuredMemory — Key-Value Persistent Memory Store (SQLite)
 * ====================================================
 * Complements the RAG-based vector memory with deterministic,
 * human-readable structured facts that are injected directly
 * into the system prompt.
 * 
 * [Phase 3.3] Refactored to Repository Pattern:
 *   - VectorRepository: sqlite-vec operations
 *   - EventRepository: event brick CRUD + memory touch
 *   - StructuredMemory: facts CRUD + orchestration facade
 * 
 * Features:
 *   - Persistent SQLite storage per agent
 *   - TTL support (auto-expire facts after N days)
 *   - Size limit with FIFO eviction (max 50 facts)
 *   - Native SQLite synchronization (WAL + busy_timeout)
 *   - System prompt injection formatting
 *   - [v4.0] AES-256-GCM encryption-at-rest for fact values
 *   - [v4.0] Background TTL eviction (non-blocking)
 *   - [v4.0] GDPR purge support
 *   - [v4.0] Data lineage (confidenceScore, sourceTurnId)
 */

import { EncryptionEngine } from "./EncryptionEngine";

export interface IDBCountRow {
    c: number;
}

export interface IDBFactRow {
    key: string;
    value: string;
    createdAt: string;
    updatedAt: string;
    ttlDays: number | null;
    source: string | null;
    category: string | null;
    importance: number | null;
    confidenceScore: number | null;
    sourceTurnId: string | null;
    memory_strength: number | null;   // [UHM] Ebbinghaus decay (0.0-1.0)
    last_accessed_at: number | null;  // [UHM] Unix ms of last retrieval
}

export interface IDBEventRow {
    eventId: string;
    timestamp: number;
    phi_facts: string;
    phi_entities: string;
    psi_sentiment: string;
    psi_intent: string;
    psi_relational: string;
    rawUserMsg: string;
    rawAiReply: string;
    consolidated: number;
    domain: string | null;
    category: string | null;
    trace_keywords: string | null;
    last_accessed_at: number | null;
}


// ===========================
// Types
// ===========================

export interface StructuredFact {
    key: string;
    value: string;
    createdAt: string;      // ISO timestamp
    updatedAt: string;      // ISO timestamp
    ttlDays?: number;       // Auto-expire after N days (null = permanent)
    source: string;         // Who created this fact (user, agent, system)
    category?: string;      // Optional categorization
    importance?: number;    // [v4.0] 0.0-1.0 ranking for eviction priority
    confidenceScore?: number; // [v4.0] Data lineage — extraction confidence
    sourceTurnId?: string;  // [v4.0] Data lineage — originating turn ID
    memoryStrength?: number;  // [UHM] Ebbinghaus decay (0.0-1.0)
    lastAccessedAt?: number;  // [UHM] Unix ms of last retrieval
}

// ===========================
// Constants
// ===========================

const MAX_FACTS = 50;           // Maximum number of facts (FIFO eviction)
const MAX_KEY_LENGTH = 100;     // Maximum key length
const MAX_VALUE_LENGTH = 1000;  // Maximum value length per fact



// ===========================
// Main Class
// ===========================

export class StructuredMemory {
    private readonly storePath: string;
    private readonly db: DatabaseSync;
    private evictionTimer: NodeJS.Timeout | null = null;

    // [Phase 3.3] Extracted repositories
    readonly #vectorRepo: VectorRepository;
    readonly #eventRepo: EventRepository;

    // [UHM] Fact Touch Buffer — RAM accumulator, flushed every 60s
    #factTouchBuffer: Map<string, number> = new Map();
    #factTouchTimer: NodeJS.Timeout | null = null;
    static readonly FACT_TOUCH_FLUSH_MS = 60_000;

    // [v19] Preserved static constants for backward compat
    static readonly TOUCH_QUEUE_CAPACITY = EventRepository.TOUCH_QUEUE_CAPACITY;
    static readonly TOUCH_EARLY_FLUSH = EventRepository.TOUCH_EARLY_FLUSH;
    static readonly TOUCH_FLUSH_INTERVAL_MS = EventRepository.TOUCH_FLUSH_INTERVAL_MS;

    constructor(storePath: string) {
        this.storePath = storePath;

        // Connect to SQLite with extension loading enabled for sqlite-vec
        this.db = new DatabaseSync(this.storePath, { allowExtension: true });
        this.initStore();

        // [Phase 3.3] Initialize extracted repositories (shared DB connection)
        this.#vectorRepo = new VectorRepository(this.db);
        this.#eventRepo = new EventRepository(this.db);

        // Initialize vector store via repository
        this.#vectorRepo.init();

        // [v4.0] Background eviction loop — non-blocking, doesn't prevent shutdown
        this.evictionTimer = setInterval(() => {
            try { this.evictExpired(); } catch { /* non-critical */ }
        }, 60_000);
        this.evictionTimer.unref();

        // [v19] Start Memory Touch debounce timer (delegated to EventRepository)
        this.#eventRepo.startTouchDebounce();
    }

    /**
     * Async Factory — ensures directory exists and migrates legacy JSON
     * without blocking the Event Loop.
     */
    static async create(agentId: string = "liva_core"): Promise<StructuredMemory> {
        const baseDir = path.join(process.cwd(), "data", "agents", agentId);
        await fsp.mkdir(baseDir, { recursive: true });

        const storePath = path.join(baseDir, "structured_memory.sqlite");
        const instance = new StructuredMemory(storePath);

        // Migrate old JSON if exists (async, non-blocking)
        await instance.migrateFromJson(path.join(baseDir, "structured_memory.json"));

        return instance;
    }

    private initStore(): void {
        // 🔒 [v4.0] Enterprise SQLite Tuning — WAL + Concurrency + Disk Safety
        this.db.exec("PRAGMA journal_mode = WAL");
        this.db.exec("PRAGMA synchronous = NORMAL");
        this.db.exec("PRAGMA busy_timeout = 5000");        // [W-8] Wait up to 5s instead of SQLITE_BUSY crash
        this.db.exec("PRAGMA wal_autocheckpoint = 500");    // [UHM-v3] Smaller WAL → faster cold-start recovery
        this.db.exec("PRAGMA cache_size = -8192");          // [UHM-v3] 8MB page cache (default 2MB) — reduces I/O for hot queries
        // [UHM-v3] Memory-mapped I/O — Unix only (Windows NTFS causes OS-level hard lock / EBUSY)
        if (process.platform !== 'win32') {
            this.db.exec("PRAGMA mmap_size = 268435456");   // 256MB mmap for macOS/Linux
        }
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS facts (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                createdAt TEXT NOT NULL,
                updatedAt TEXT NOT NULL,
                ttlDays INTEGER,
                source TEXT NOT NULL,
                category TEXT,
                importance REAL DEFAULT 0.5,
                confidenceScore REAL DEFAULT 1.0,
                sourceTurnId TEXT
            )
        `);
        // [v4.0] Safe migration: add columns if they don't exist yet (idempotent)
        try { this.db.exec("ALTER TABLE facts ADD COLUMN importance REAL DEFAULT 0.5"); } catch { /* already exists */ }
        try { this.db.exec("ALTER TABLE facts ADD COLUMN confidenceScore REAL DEFAULT 1.0"); } catch { /* already exists */ }
        try { this.db.exec("ALTER TABLE facts ADD COLUMN sourceTurnId TEXT"); } catch { /* already exists */ }
        // [UHM] Ebbinghaus Forgetting Curve columns
        try { this.db.exec("ALTER TABLE facts ADD COLUMN memory_strength REAL DEFAULT 1.0"); } catch { /* already exists */ }
        try { this.db.exec("ALTER TABLE facts ADD COLUMN last_accessed_at INTEGER DEFAULT 0"); } catch { /* already exists */ }

        // [LIVA-UHM Phase 2] Events table for Dual-Perspective Extraction (Φ Factual + Ψ Relational)
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS events (
                eventId TEXT PRIMARY KEY,
                timestamp INTEGER NOT NULL,
                phi_facts TEXT,
                phi_entities TEXT,
                psi_sentiment TEXT,
                psi_intent TEXT,
                psi_relational TEXT,
                rawUserMsg TEXT,
                rawAiReply TEXT,
                consolidated INTEGER DEFAULT 0
            )
        `);

        // [H-MEM v18] Idempotent migration: add hierarchical metadata columns to events
        const columns = this.db.prepare("PRAGMA table_info(events)").all() as Array<{name: string}>;
        const colNames = new Set(columns.map(c => c.name));
        if (!colNames.has('domain')) {
            this.db.exec("ALTER TABLE events ADD COLUMN domain TEXT DEFAULT 'General'");
        }
        if (!colNames.has('category')) {
            this.db.exec("ALTER TABLE events ADD COLUMN category TEXT DEFAULT 'Uncategorized'");
        }
        if (!colNames.has('trace_keywords')) {
            this.db.exec("ALTER TABLE events ADD COLUMN trace_keywords TEXT DEFAULT '[]'");
        }
        if (!colNames.has('last_accessed_at')) {
            this.db.exec("ALTER TABLE events ADD COLUMN last_accessed_at INTEGER DEFAULT 0");
        }
        // [UHM-v3 DLQ] Consolidation status tracking — DEFAULT 'consolidated' for OLD data
        // ⚠️ Backward Compatibility Guard: existing events must NOT be re-processed
        if (!colNames.has('consolidation_status')) {
            this.db.exec("ALTER TABLE events ADD COLUMN consolidation_status TEXT DEFAULT 'consolidated'");
            this.db.exec("ALTER TABLE events ADD COLUMN retry_count INTEGER DEFAULT 0");
        }
        // [UHM-v3] Partial index — only pending events get scanned, zero cost for consolidated
        this.db.exec("CREATE INDEX IF NOT EXISTS idx_events_pending ON events(eventId) WHERE consolidation_status = 'pending'");

        // [v19] Legacy Cleanup: Drop old lance_dlq table if it exists
        this.db.exec("DROP TABLE IF EXISTS lance_dlq");

        // [v19] Dead Letter Queue — retained for vector delete retry
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS vector_dlq (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                delete_filter TEXT NOT NULL,
                status TEXT DEFAULT 'pending',
                retry_count INTEGER DEFAULT 0
            )
        `);

        // [LIVA-UHM Phase 3] L1 - Turn Layer Nodes (Raw Graph Nodes)
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS turn_layer_nodes (
                turnId TEXT PRIMARY KEY,
                temporal_anchor INTEGER NOT NULL,
                userMsg TEXT NOT NULL,
                aiReply TEXT NOT NULL,
                createdAt TEXT NOT NULL
            )
        `);

        // [LIVA v24] Shadow Digest Pipeline — Pre-computed daily briefings cache
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS daily_briefings (
                id TEXT PRIMARY KEY,
                created_at INTEGER NOT NULL,
                topics TEXT NOT NULL,
                content TEXT NOT NULL,
                is_read INTEGER DEFAULT 0,
                source TEXT DEFAULT 'tavily',
                expires_at INTEGER NOT NULL
            )
        `);
        // Auto-cleanup expired briefings on boot
        this.db.exec(`DELETE FROM daily_briefings WHERE expires_at < ${Date.now()}`);

        // [v25] Task Manager — Persistent user tasks
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS tasks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                description TEXT DEFAULT '',
                status TEXT DEFAULT 'pending',
                priority TEXT DEFAULT 'medium',
                result TEXT DEFAULT '',
                created_at INTEGER NOT NULL,
                updated_at INTEGER NOT NULL
            )
        `);
    }

    // ===========================
    // [Phase 3.3] Delegated Vector Operations
    // ===========================

    public initVecDimension(dimension: number): void {
        this.#vectorRepo.initVecDimension(dimension);
    }

    public upsertVector(record: {
        vecId: string; type: string; content: string; vector: number[];
        domain?: string; category?: string; traceKeywords?: string[]; fileTarget?: string;
        sourceEventIds?: string[];  // [UHM] L2→L1 positional pointers
    }): void {
        this.#vectorRepo.upsertVector(record);
    }

    public upsertVectorsBatch(records: Array<{
        vecId: string; type: string; content: string; vector: number[];
        domain?: string; category?: string; traceKeywords?: string[];
    }>): void {
        this.#vectorRepo.upsertVectorsBatch(records);
    }

    public searchSimilarVectors(
        queryVector: number[], topK?: number, typeFilter?: string
    ): Array<{ id: number; vecId: string; content: string; type: string; domain: string; category: string; distance: number; traceKeywords: string[]; sourceEventIds: string[] }> {
        return this.#vectorRepo.searchSimilarVectors(queryVector, topK, typeFilter);
    }

    public searchAnchors(queryVector: number[], limit?: number): string[] {
        return this.#vectorRepo.searchAnchors(queryVector, limit);
    }

    public searchAxiomsByVector(queryVector: number[], limit?: number): Array<{ text: string; traceKeywords: string }> {
        return this.#vectorRepo.searchAxiomsByVector(queryVector, limit);
    }

    // [UHM] Positional Index drill-down
    public searchWithDrilldown(queryVector: number[], topK?: number, typeFilter?: string) {
        return this.#vectorRepo.searchWithDrilldown(queryVector, topK, typeFilter);
    }

    public collectDrilldownEventIds(queryVector: number[], topK?: number, typeFilter?: string): string[] {
        return this.#vectorRepo.collectDrilldownEventIds(queryVector, topK, typeFilter);
    }

    public deleteVectorByContent(content: string): void {
        this.#vectorRepo.deleteVectorByContent(content);
    }

    public deleteVectorById(vecId: string): void {
        this.#vectorRepo.deleteVectorById(vecId);
    }

    public deleteAllVectors(): void {
        this.#vectorRepo.deleteAllVectors();
    }

    public get vectorCount(): number {
        return this.#vectorRepo.vectorCount;
    }

    public get vecReady(): boolean {
        return this.#vectorRepo.vecReady;
    }

    public pushToDLQ(filter: string): void {
        this.#vectorRepo.pushToDLQ(filter);
    }

    public processDLQ(): void {
        this.#vectorRepo.processDLQ();
    }

    // ===========================
    // [Phase 3.3] Delegated Event Operations
    // ===========================

    public queueMemoryTouch(eventId: string): void {
        this.#eventRepo.queueMemoryTouch(eventId);
    }

    public async flushTouchQueue(): Promise<void> {
        return this.#eventRepo.flushTouchQueue();
    }

    public insertEvent(event: EventBrick): void {
        this.#eventRepo.insertEvent(event);
    }

    public getUnconsolidatedEvents(): EventBrick[] {
        return this.#eventRepo.getUnconsolidatedEvents();
    }

    public getUnconsolidatedCount(): number {
        return this.#eventRepo.getUnconsolidatedCount();
    }

    public markConsolidated(eventIds: string[]): void {
        this.#eventRepo.markConsolidated(eventIds);
    }

    /** [UHM-v3 DLQ] Move events to Dead Letter Queue after 3 failed consolidation attempts. */
    public markDLQ(eventIds: string[]): void {
        this.#eventRepo.markDLQ(eventIds);
    }

    /** [UHM-v3 DLQ] Increment retry count for failed consolidation sessions. */
    public incrementRetryCount(eventIds: string[]): void {
        this.#eventRepo.incrementRetryCount(eventIds);
    }

    public gcOldEvents(retentionDays?: number): number {
        return this.#eventRepo.gcOldEvents(retentionDays);
    }

    public deleteAllEvents(): void {
        this.#eventRepo.deleteAllEvents();
    }

    public insertTurnNode(turnId: string, temporal_anchor: number, userMsg: string, aiReply: string): void {
        this.#eventRepo.insertTurnNode(turnId, temporal_anchor, userMsg, aiReply);
    }

    public getTurnsByTimeRange(fromTs: number, toTs: number): TurnNode[] {
        return this.#eventRepo.getTurnsByTimeRange(fromTs, toTs);
    }

    public getTurnsByIds(turnIds: string[]): TurnNode[] {
        return this.#eventRepo.getTurnsByIds(turnIds);
    }

    // ===========================
    // [UHM] Fact Touch Buffer & Ebbinghaus Forgetting Curve
    // ===========================

    /**
     * [UHM] Buffer a fact access in RAM — flushed every 60s.
     * Prevents Write Amplification from per-read UPDATE statements.
     * Touching resets memory_strength to 1.0 (spaced repetition reinforcement).
     */
    public touchFact(key: string): void {
        this.#factTouchBuffer.set(key, Date.now());
        if (!this.#factTouchTimer) {
            this.#factTouchTimer = setTimeout(() => {
                this.flushFactTouches();
            }, StructuredMemory.FACT_TOUCH_FLUSH_MS);
            this.#factTouchTimer.unref();
        }
    }

    /**
     * [UHM] Flush buffered fact touches to SQLite in a single atomic transaction.
     * Called periodically (60s), on shutdown, and on demand.
     */
    public flushFactTouches(): void {
        if (this.#factTouchBuffer.size === 0) return;
        const entries = Array.from(this.#factTouchBuffer.entries());
        this.#factTouchBuffer.clear();
        if (this.#factTouchTimer) { clearTimeout(this.#factTouchTimer); this.#factTouchTimer = null; }

        // [G8] Atomic transaction — single write I/O
        const stmt = this.db.prepare(
            "UPDATE facts SET memory_strength = 1.0, last_accessed_at = ? WHERE key = ?"
        );
        this.db.exec("BEGIN");
        try {
            for (const [key, ts] of entries) {
                stmt.run(ts, key);
            }
            this.db.exec("COMMIT");
            logger.debug(`[StructuredMemory/Touch] Flushed ${entries.length} fact touches.`);
        } catch (e: unknown) {
            try { this.db.exec("ROLLBACK"); } catch { /* ignore */ }
            // Re-queue failed entries
            for (const [key, ts] of entries) {
                this.#factTouchBuffer.set(key, ts);
            }
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/Touch] Flush failed (re-queued): ${errMsg}`);
        }
    }

    /**
     * [UHM] Apply Ebbinghaus forgetting curve to all facts.
     * Called from ConsolidationCron after each consolidation cycle.
     *
     * [G1] Uses Math.exp() in V8 — SQLite has NO exp() function.
     * [G8] Final writes wrapped in atomic transaction.
     * [G11] Chunked computation with setImmediate yield to prevent Event Loop blocking.
     *
     * Formula: S(t) = S₀ × e^(-λ × days_since_access)
     * λ = decayRate (default 0.1: ~60% remaining after 5 days without access)
     */
    public async applyMemoryDecay(decayRate: number = 0.1): Promise<{ decayed: number; archived: number }> {
        const now = Date.now();
        const MS_PER_DAY = 86_400_000;
        const ARCHIVE_THRESHOLD = 0.1;
        const CHUNK_SIZE = 500; // [G11] Yield CPU every 500 rows

        const facts = this.db.prepare(
            "SELECT key, memory_strength, last_accessed_at FROM facts"
        ).all() as Array<{ key: string; memory_strength: number; last_accessed_at: number }>;

        const toUpdate: Array<{ key: string; strength: number }> = [];
        const toDelete: string[] = [];

        // [G11] Process in chunks with CPU yielding to prevent Event Loop blocking
        for (let i = 0; i < facts.length; i += CHUNK_SIZE) {
            const chunk = facts.slice(i, i + CHUNK_SIZE);
            for (const fact of chunk) {
                const daysSince = (now - (fact.last_accessed_at || 0)) / MS_PER_DAY;
                if (daysSince < 1) continue; // Skip recently accessed

                // [G1] V8 Math.exp(), NOT SQLite exp()
                const newStrength = (fact.memory_strength ?? 1.0) * Math.exp(-decayRate * daysSince);

                if (newStrength < ARCHIVE_THRESHOLD) {
                    toDelete.push(fact.key);
                } else if (Math.abs(newStrength - (fact.memory_strength ?? 1.0)) > 0.01) {
                    toUpdate.push({ key: fact.key, strength: newStrength });
                }
            }
            // [G11] Yield CPU after each chunk — mandatory for >500 facts
            if (i + CHUNK_SIZE < facts.length) {
                await new Promise(resolve => setImmediate(resolve));
            }
        }

        // [G8] Single atomic transaction for all DB writes
        const updateStmt = this.db.prepare("UPDATE facts SET memory_strength = ? WHERE key = ?");
        const deleteStmt = this.db.prepare("DELETE FROM facts WHERE key = ?");
        this.db.exec("BEGIN");
        try {
            for (const u of toUpdate) updateStmt.run(u.strength, u.key);
            for (const k of toDelete) deleteStmt.run(k);
            this.db.exec("COMMIT");
        } catch {
            try { this.db.exec("ROLLBACK"); } catch { /* ignore */ }
        }

        return { decayed: toUpdate.length, archived: toDelete.length };
    }

    // ===========================
    // Facts CRUD (remains in StructuredMemory — core domain)
    // ===========================

    public setFact(
        key: string,
        value: string,
        options: { ttlDays?: number; source?: string; category?: string } = {}
    ): void {
        key = key.trim().substring(0, MAX_KEY_LENGTH);
        value = value.trim().substring(0, MAX_VALUE_LENGTH);
        
        if (!key || !value) {
            logger.warn("[StructuredMemory] Attempted to set empty key or value");
            return;
        }

        const now = new Date().toISOString();
        const ttlDays = options.ttlDays ?? null;
        const source = options.source || "agent";
        const category = options.category ?? null;

        // [v4.0] Importance scoring based on source
        const importance = options.source === "user" ? 1.0
/* istanbul ignore next */
/* istanbul ignore next */
            : options.source === "consolidation" ? 0.7 : 0.5;
        const confidenceScore = 1.0;
        const sourceTurnId = null;

        // [v4.0] Encrypt value at rest (W-7)
        const encryptedValue = EncryptionEngine.encrypt(value);

        const stmt = this.db.prepare(`
            INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId, memory_strength, last_accessed_at)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1.0, ?)
            ON CONFLICT(key) DO UPDATE SET 
                value = excluded.value, 
                updatedAt = excluded.updatedAt,
                ttlDays = COALESCE(excluded.ttlDays, facts.ttlDays),
                source = excluded.source,
                category = COALESCE(excluded.category, facts.category),
                importance = excluded.importance,
                confidenceScore = excluded.confidenceScore,
                memory_strength = MAX(facts.memory_strength, 0.8),
                last_accessed_at = excluded.last_accessed_at
        `);
        
        stmt.run(key, encryptedValue, now, now, ttlDays, source, category, importance, confidenceScore, sourceTurnId, Date.now());
        
        const changes = this.db.prepare("SELECT changes() as c").get() as unknown as IDBCountRow;
        if (changes.c > 0) {
           logger.info(`[StructuredMemory] Saved fact: "${key}"`);
        }

        this.enforceCapacity();
    }

    private enforceCapacity(): void {
        const currentCount = this.count;
        if (currentCount > MAX_FACTS) {
            const over = currentCount - MAX_FACTS;
            const stmt = this.db.prepare(`
                DELETE FROM facts WHERE key IN (
                    SELECT key FROM facts ORDER BY importance ASC, updatedAt ASC LIMIT ?
                )
            `);
            stmt.run(over);
            logger.warn(`[StructuredMemory] Evicted ${over} oldest facts (FIFO capacity)`);
        }
    }

    public getFact(key: string): StructuredFact | null {
        const stmt = this.db.prepare("SELECT * FROM facts WHERE key = ?");
        const row = stmt.get(key) as unknown as IDBFactRow;
        if (!row) return null;
        // [UHM] Touch via buffer — no direct I/O (prevents Write Amplification)
        this.touchFact(key);
        return this.mapRow(row);
    }

    public getAllFacts(): StructuredFact[] {
        const stmt = this.db.prepare("SELECT * FROM facts ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all() as unknown as IDBFactRow[]).map(r => this.mapRow(r));
    }

    public deleteFact(key: string): boolean {
        const stmt = this.db.prepare("DELETE FROM facts WHERE key = ?");
        const changes = stmt.run(key).changes;
        if (changes > 0) {
            logger.info(`[StructuredMemory] Deleted fact: "${key}"`);
            return true;
        }
        return false;
    }

    public getFactsByCategory(category: string): StructuredFact[] {
        const stmt = this.db.prepare("SELECT * FROM facts WHERE category = ? ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all(category) as unknown as IDBFactRow[]).map(r => this.mapRow(r));
    }

    public get count(): number {
        const row = this.db.prepare("SELECT count(*) as c FROM facts").get() as unknown as IDBCountRow;
        return row.c;
    }

    // ===========================
    // System Prompt Injection
    // ===========================

    public formatForSystemPrompt(): string {
        // [UHM] Filter out weak memories (Ebbinghaus decay threshold)
        const facts = this.getAllFacts().filter(f => (f.memoryStrength ?? 1.0) >= 0.2);
        if (facts.length === 0) return "";

        let output = "\n[BỘ NHỚ CẤU TRÚC — Kiến thức đã được xác nhận]\n";

        const categories = new Map<string, StructuredFact[]>();
        for (const fact of facts) {
/* istanbul ignore next */
            const cat = fact.category || "Chung";
/* istanbul ignore next */
            if (!categories.has(cat)) categories.set(cat, []);
            categories.get(cat)!.push(fact);
        }

        for (const [category, catFacts] of categories) {
            output += `\n## ${category}\n`;
            for (const fact of catFacts) {
                output += `- ${fact.key}: ${fact.value}\n`;
            }
        }

/* istanbul ignore next */
        output += `\n(Tổng: ${facts.length} kiến thức | Cập nhật lần cuối: ${facts[0]?.updatedAt || "N/A"})\n`;

        return output;
    }

    // ===========================
    // TTL Eviction
    // ===========================

    private evictExpired(): void {
        const now = Date.now();
        const stmt = this.db.prepare("SELECT key, createdAt, ttlDays FROM facts WHERE ttlDays IS NOT NULL");
        const checkRows = stmt.all() as unknown as IDBFactRow[];
        
        let evicted = 0;
        const deleteStmt = this.db.prepare("DELETE FROM facts WHERE key = ?");
        
        for (const row of checkRows) {
            const created = new Date(row.createdAt).getTime();
            if (row.ttlDays === null) continue;
            const ttlMs = row.ttlDays * 24 * 60 * 60 * 1000;
/* istanbul ignore next */
            if ((now - created) > ttlMs) {
                deleteStmt.run(row.key);
                evicted++;
            }
        }

        if (evicted > 0) {
            logger.info(`[StructuredMemory] TTL eviction: removed ${evicted} expired facts`);
        }
    }

    private mapRow(row: IDBFactRow): StructuredFact {
        return {
            key: row.key,
            value: EncryptionEngine.decrypt(row.value), // [v4.0] Decrypt at read time (W-7)
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            ttlDays: row.ttlDays ?? undefined,
            source: row.source ?? "System",
            category: row.category ?? undefined,
/* istanbul ignore next */
/* istanbul ignore next */
            importance: row.importance ?? 0.5,
/* istanbul ignore next */
/* istanbul ignore next */
            confidenceScore: row.confidenceScore ?? 1.0,
            sourceTurnId: row.sourceTurnId ?? undefined,
            memoryStrength: row.memory_strength ?? 1.0,
            lastAccessedAt: row.last_accessed_at ?? 0,
        };
    }

    // ===========================
    // [v4.0] GDPR Compliance — Right to be Forgotten (W-10)
    // ===========================

    public deleteAllFacts(): void {
        this.db.exec("DELETE FROM facts");
        logger.warn("[StructuredMemory/GDPR] All facts permanently erased.");
    }

    public setFactImportance(key: string, importance: number): void {
        const stmt = this.db.prepare("UPDATE facts SET importance = ? WHERE key = ?");
        stmt.run(importance, key);
    }

    // ===========================
    // Lifecycle
    // ===========================

    public close(): void {
        try {
            // [UHM/G10] Flush fact touches BEFORE closing DB — prevents data loss
            this.flushFactTouches();

            // Flush pending memory touches via EventRepository
            this.#eventRepo.flushAndStop();

            // Clean up timers
            if (this.#factTouchTimer) { clearTimeout(this.#factTouchTimer); this.#factTouchTimer = null; }
            if (this.evictionTimer) {
                clearInterval(this.evictionTimer);
                this.evictionTimer = null;
            }
            this.db.close();
            logger.info('[StructuredMemory] SQLite connection closed.');
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory] Close error (non-critical): ${errMsg}`);
        }
    }

    private async migrateFromJson(jsonPath: string): Promise<void> {
        try {
            await fsp.access(jsonPath, fsc.F_OK);
        } catch {
            return; // File does not exist — nothing to migrate
        }

        try {
            const raw = await fsp.readFile(jsonPath, "utf-8");
            const parsed = JSON.parse(raw);
            if (parsed.facts && Array.isArray(parsed.facts)) {
                const stmt = this.db.prepare("INSERT OR IGNORE INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category) VALUES (?, ?, ?, ?, ?, ?, ?)");
                for (const fact of parsed.facts) {
/* istanbul ignore next */
                    stmt.run(fact.key, fact.value, fact.createdAt, fact.updatedAt, fact.ttlDays || null, fact.source, fact.category || null);
                }
                logger.info(`[StructuredMemory] Migrated ${parsed.facts.length} facts from JSON to SQLite`);
                await fsp.rename(jsonPath, jsonPath + ".bak");
            }
        } catch (e) {
            logger.warn(`[StructuredMemory] JSON migration failed: ${e}`);
        }
    }

    /**
     * [UHM-v3] Atomic Snapshot Backup via VACUUM INTO.
     * 
     * ⚠️ NEVER use fs.promises.cp() on a running SQLite WAL DB — guaranteed corruption.
     * VACUUM INTO creates a consistent, standalone snapshot with WAL merged.
     * Uses tmp→rename pattern (AI_CONTEXT Rule 4.3 Atomic Write).
     * Called after successful ConsolidationCron cycle. Rotates max 2 backups.
     */
    public async createSnapshotBackup(): Promise<void> {
        const backupPath = this.storePath + '.backup';
        const tmpPath = backupPath + '.tmp';

        try {
            // Clean up stale tmp from previous crash
            try { await fsp.unlink(tmpPath); } catch { /* ENOENT ok */ }

            // SQLite VACUUM INTO: atomic freeze + WAL merge + write to new file
            this.db.exec(`VACUUM INTO '${tmpPath.replace(/'/g, "''")}'`);

            // Atomic rename (Rule 4.3)
            await fsp.rename(tmpPath, backupPath);
            logger.info('[StructuredMemory] Snapshot backup created successfully.');
        } catch (e) {
            const msg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory] Snapshot backup failed: ${msg}`);
            // Non-critical — don't crash the system
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
     * Expose raw DB handle for external operations (ConsolidationCron, etc.).
     */
    public getDb(): DatabaseSync {
        return this.db;
    }

    // ===========================
    // [LIVA v24] Shadow Digest — Daily Briefings Cache
    // ===========================

    /**
     * Save a pre-computed daily briefing to the cache.
     * Used by ProactiveDaemon after background news synthesis.
     */
    public saveBriefing(briefing: {
        id: string;
        topics: string;
        content: string;
        source?: string;
        ttlHours?: number;
    }): void {
        const ttl = (briefing.ttlHours ?? 24) * 60 * 60 * 1000;
        const now = Date.now();
        this.db.prepare(`
            INSERT OR REPLACE INTO daily_briefings (id, created_at, topics, content, is_read, source, expires_at)
            VALUES (?, ?, ?, ?, 0, ?, ?)
        `).run(briefing.id, now, briefing.topics, briefing.content, briefing.source ?? "tavily", now + ttl);
        logger.info(`[v24 ShadowDigest] 📰 Briefing cached: ${briefing.id} (TTL: ${briefing.ttlHours ?? 24}h)`);
    }

    /**
     * Get unread briefings (most recent first, limit 5).
     * Used by PromptBuilder for instant Pull-mode injection.
     */
    public getUnreadBriefings(limit: number = 5): Array<{ id: string; topics: string; content: string; created_at: number }> {
        const now = Date.now();
        return this.db.prepare(`
            SELECT id, topics, content, created_at FROM daily_briefings
            WHERE is_read = 0 AND expires_at > ?
            ORDER BY created_at DESC LIMIT ?
        `).all(now, limit) as Array<{ id: string; topics: string; content: string; created_at: number }>;
    }

    /**
     * Mark a briefing as read (after user has consumed it).
     */
    public markBriefingRead(id: string): void {
        this.db.prepare("UPDATE daily_briefings SET is_read = 1 WHERE id = ?").run(id);
    }

    /**
     * Clean up expired briefings (called periodically by ProactiveDaemon).
     */
    public cleanExpiredBriefings(): number {
        const result = this.db.prepare("DELETE FROM daily_briefings WHERE expires_at < ?").run(Date.now());
        return (result as { changes: number }).changes;
    }

    // ═══════════════════════════════════════════════════════
    //  [v25] Task Manager — Persistent CRUD
    // ═══════════════════════════════════════════════════════

    public getTasks(): Array<{ id: string; title: string; description: string; status: string; priority: string; result: string; created_at: number; updated_at: number }> {
        return this.db.prepare(
            "SELECT id, title, description, status, priority, result, created_at, updated_at FROM tasks ORDER BY created_at DESC"
        ).all() as any[];
    }

    public addTask(task: { id: string; title: string; description?: string; priority?: string }): void {
        const now = Date.now();
        this.db.prepare(
            "INSERT INTO tasks (id, title, description, priority, status, created_at, updated_at) VALUES (?, ?, ?, ?, 'pending', ?, ?)"
        ).run(task.id, task.title, task.description || "", task.priority || "medium", now, now);
    }

    public updateTask(id: string, updates: { status?: string; result?: string; title?: string; description?: string; priority?: string }): void {
        const fields: string[] = [];
        const values: (string | number | null)[] = [];
        if (updates.status !== undefined) { fields.push("status = ?"); values.push(updates.status); }
        if (updates.result !== undefined) { fields.push("result = ?"); values.push(updates.result); }
        if (updates.title !== undefined) { fields.push("title = ?"); values.push(updates.title); }
        if (updates.description !== undefined) { fields.push("description = ?"); values.push(updates.description); }
        if (updates.priority !== undefined) { fields.push("priority = ?"); values.push(updates.priority); }
        if (fields.length === 0) return;
        fields.push("updated_at = ?");
        values.push(Date.now());
        values.push(id);
        this.db.prepare(`UPDATE tasks SET ${fields.join(", ")} WHERE id = ?`).run(...values);
    }

    public deleteTask(id: string): void {
        this.db.prepare("DELETE FROM tasks WHERE id = ?").run(id);
    }
}
