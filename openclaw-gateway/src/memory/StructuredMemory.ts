import { DatabaseSync } from "node:sqlite";
import * as fs from "node:fs";
import * as path from "node:path";
import { logger } from "../utils/logger";

/**
 * StructuredMemory — Key-Value Persistent Memory Store (SQLite)
 * ====================================================
 * Complements the RAG-based vector memory with deterministic,
 * human-readable structured facts that are injected directly
 * into the system prompt.
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

import * as crypto from "node:crypto";

// ===========================
// Types
// ===========================

// [LIVA-UHM Phase 3] L1 - Turn Layer Node
export interface TurnNode {
    turnId: string;
    temporal_anchor: number; // Unix timestamp
    userMsg: string;
    aiReply: string;
    createdAt: string;       // ISO timestamp
}

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

// ===========================
// [v4.0] Encryption helpers (AES-256-GCM)
// ===========================

const ENCRYPTION_KEY = process.env.ENCRYPTION_KEY
    || crypto.createHash("sha256").update("LIVA_FALLBACK_SECRET_KEY").digest("base64").substring(0, 32);
const IV_LENGTH = 16;

function encryptValue(text: string): string {
    const iv = crypto.randomBytes(IV_LENGTH);
    const cipher = crypto.createCipheriv("aes-256-gcm", Buffer.from(ENCRYPTION_KEY), iv);
    let encrypted = cipher.update(text, "utf8", "hex");
    encrypted += cipher.final("hex");
    const authTag = cipher.getAuthTag().toString("hex");
    return `${iv.toString("hex")}:${authTag}:${encrypted}`;
}

function decryptValue(text: string): string {
    try {
        const parts = text.split(":");
        if (parts.length !== 3) return text; // Plain-text fallback for pre-v4 data
        const iv = Buffer.from(parts[0], "hex");
        const authTag = Buffer.from(parts[1], "hex");
        const encryptedText = parts[2];
        const decipher = crypto.createDecipheriv("aes-256-gcm", Buffer.from(ENCRYPTION_KEY), iv);
        decipher.setAuthTag(authTag);
        let decrypted = decipher.update(encryptedText, "hex", "utf8");
        decrypted += decipher.final("utf8");
        return decrypted;
    } catch {
        return text; // Backward compat: return raw if decryption fails
    }
}

export class StructuredMemory {
    private readonly storePath: string;
    private readonly db: DatabaseSync;
    private evictionTimer: NodeJS.Timeout | null = null;

    constructor(agentId: string = "liva_core") {
        const baseDir = path.join(process.cwd(), "data", "agents", agentId);
        if (!fs.existsSync(baseDir)) {
            fs.mkdirSync(baseDir, { recursive: true });
        }
        this.storePath = path.join(baseDir, "structured_memory.sqlite");
        
        // Connect to SQLite
        this.db = new DatabaseSync(this.storePath);
        this.initStore();
        
        // Migrate old JSON if exists
        this.migrateFromJson(path.join(baseDir, "structured_memory.json"));

        // [v4.0] Background eviction loop — non-blocking, doesn't prevent shutdown
        this.evictionTimer = setInterval(() => {
            try { this.evictExpired(); } catch { /* non-critical */ }
        }, 60_000);
        this.evictionTimer.unref();
    }

    private initStore(): void {
        // 🔒 [v4.0] Enterprise SQLite Tuning — WAL + Concurrency + Disk Safety
        this.db.exec("PRAGMA journal_mode = WAL");
        this.db.exec("PRAGMA synchronous = NORMAL");
        this.db.exec("PRAGMA busy_timeout = 5000");        // [W-8] Wait up to 5s instead of SQLITE_BUSY crash
        this.db.exec("PRAGMA wal_autocheckpoint = 1000");   // [W-9] Prevent unbounded WAL disk bloat
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
    }

    /**
     * 🔒 [Audit Fix C-3] Close SQLite connection for clean shutdown.
     * Called from MemoryManager.dispose() / CoreKernel.shutdown().
     */
    public close(): void {
        try {
            if (this.evictionTimer) {
                clearInterval(this.evictionTimer);
                this.evictionTimer = null;
            }
            this.db.close();
            logger.info("[StructuredMemory] SQLite connection closed.");
        } catch (e: any) {
            logger.warn(`[StructuredMemory] Close error (non-critical): ${e.message}`);
        }
    }

    private migrateFromJson(jsonPath: string): void {
         if (fs.existsSync(jsonPath)) {
             try {
                 const raw = fs.readFileSync(jsonPath, "utf-8");
                 const parsed = JSON.parse(raw);
                 if (parsed.facts && Array.isArray(parsed.facts)) {
                     const stmt = this.db.prepare("INSERT OR IGNORE INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category) VALUES (?, ?, ?, ?, ?, ?, ?)");
                     for (const fact of parsed.facts) {
/* istanbul ignore next */
                         stmt.run(fact.key, fact.value, fact.createdAt, fact.updatedAt, fact.ttlDays || null, fact.source, fact.category || null);
                     }
                     logger.info(`[StructuredMemory] Migrated ${parsed.facts.length} facts from JSON to SQLite`);
                     fs.renameSync(jsonPath, jsonPath + ".bak");
                 }
             } catch (e) {
                 logger.warn(`[StructuredMemory] JSON migration failed: ${e}`);
             }
         }
    }

    // ===========================
    // CRUD Operations
    // ===========================

    /**
     * Set a fact (create or update)
     */
    public setFact(
        key: string,
        value: string,
        options: { ttlDays?: number; source?: string; category?: string } = {}
    ): void {
        // Validate
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
        const encryptedValue = encryptValue(value);

        const stmt = this.db.prepare(`
            INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET 
                value = excluded.value, 
                updatedAt = excluded.updatedAt,
                ttlDays = COALESCE(excluded.ttlDays, facts.ttlDays),
                source = excluded.source,
                category = COALESCE(excluded.category, facts.category),
                importance = excluded.importance,
                confidenceScore = excluded.confidenceScore
        `);
        
        stmt.run(key, encryptedValue, now, now, ttlDays, source, category, importance, confidenceScore, sourceTurnId);
        
        const changes = this.db.prepare("SELECT changes() as c").get() as any;
        if (changes.c > 0) {
           logger.info(`[StructuredMemory] Saved fact: "${key}"`);
        }

        // FIFO eviction if at capacity
        this.enforceCapacity();
    }

    private enforceCapacity(): void {
        const currentCount = this.count;
        if (currentCount > MAX_FACTS) {
            const over = currentCount - MAX_FACTS;
            // [v4.0] Evict by importance ASC first, then updatedAt ASC (G-6)
            const stmt = this.db.prepare(`
                DELETE FROM facts WHERE key IN (
                    SELECT key FROM facts ORDER BY importance ASC, updatedAt ASC LIMIT ?
                )
            `);
            stmt.run(over);
            logger.warn(`[StructuredMemory] Evicted ${over} oldest facts (FIFO capacity)`);
        }
    }

    /**
     * Get a single fact by key
     */
    public getFact(key: string): StructuredFact | null {
        // [v4.0] eviction moved to background timer — no longer blocks reads (W-2)
        const stmt = this.db.prepare("SELECT * FROM facts WHERE key = ?");
        const row = stmt.get(key) as any;
        return row ? this.mapRow(row) : null;
    }

    /**
     * Get all active facts (after TTL eviction)
     */
    public getAllFacts(): StructuredFact[] {
        // [v4.0] eviction moved to background timer — no longer blocks reads (W-2)
        const stmt = this.db.prepare("SELECT * FROM facts ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all() as any[]).map(r => this.mapRow(r));
    }

    /**
     * Delete a fact by key
     */
    public deleteFact(key: string): boolean {
        const stmt = this.db.prepare("DELETE FROM facts WHERE key = ?");
        const changes = stmt.run(key).changes;
        if (changes > 0) {
            logger.info(`[StructuredMemory] Deleted fact: "${key}"`);
            return true;
        }
        return false;
    }

    /**
     * Get facts by category
     */
    public getFactsByCategory(category: string): StructuredFact[] {
        // [v4.0] eviction moved to background timer
        const stmt = this.db.prepare("SELECT * FROM facts WHERE category = ? ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all(category) as any[]).map(r => this.mapRow(r));
    }

    /**
     * Get fact count
     */
    public get count(): number {
        const row = this.db.prepare("SELECT count(*) as c FROM facts").get() as any;
        return row.c;
    }

    // ===========================
    // System Prompt Injection
    // ===========================

    /**
     * Format all facts for injection into the system prompt.
     * Returns empty string if no facts exist.
     */
    public formatForSystemPrompt(): string {
        const facts = this.getAllFacts(); // getAllFacts() already calls evictExpired()
        if (facts.length === 0) return "";

        let output = "\n[BỘ NHỚ CẤU TRÚC — Kiến thức đã được xác nhận]\n";

        // Group by category
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

    /**
     * Remove expired facts based on TTL
     */
    private evictExpired(): void {
        const now = Date.now();
        const stmt = this.db.prepare("SELECT key, createdAt, ttlDays FROM facts WHERE ttlDays IS NOT NULL");
        const checkRows = stmt.all() as any[];
        
        let evicted = 0;
        const deleteStmt = this.db.prepare("DELETE FROM facts WHERE key = ?");
        
        for (const row of checkRows) {
            const created = new Date(row.createdAt).getTime();
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

    private mapRow(row: any): StructuredFact {
        return {
            key: row.key,
            value: decryptValue(row.value), // [v4.0] Decrypt at read time (W-7)
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            ttlDays: row.ttlDays,
            source: row.source,
            category: row.category,
/* istanbul ignore next */
/* istanbul ignore next */
            importance: row.importance ?? 0.5,
/* istanbul ignore next */
/* istanbul ignore next */
            confidenceScore: row.confidenceScore ?? 1.0,
            sourceTurnId: row.sourceTurnId ?? undefined,
        };
    }

    // ===========================
    // [v4.0] GDPR Compliance — Right to be Forgotten (W-10)
    // ===========================

    /** Hard-delete ALL facts. Called from MemoryManager.purgeUserContext(). */
    public deleteAllFacts(): void {
        this.db.exec("DELETE FROM facts");
        logger.warn("[StructuredMemory/GDPR] All facts permanently erased.");
    }

    /** Hard-delete ALL events. Called from MemoryManager.purgeUserContext(). */
    public deleteAllEvents(): void {
        this.db.exec("DELETE FROM events");
        this.db.exec("DELETE FROM turn_layer_nodes");
        logger.warn("[StructuredMemory/GDPR] All events and turn nodes permanently erased.");
    }

    /** [v4.0] Soft-deprecate a fact's importance (for Fact Reconciliation G-9). */
    public setFactImportance(key: string, importance: number): void {
        const stmt = this.db.prepare("UPDATE facts SET importance = ? WHERE key = ?");
        stmt.run(importance, key);
    }

    // ===========================
    // [LIVA-UHM] Event Persistence (Dual-Perspective Φ/Ψ)
    // ===========================

    /**
     * Insert a new event brick from ReflectionDaemon.
     */
    public insertEvent(event: EventBrick): void {
        const stmt = this.db.prepare(`
            INSERT OR REPLACE INTO events 
            (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0)
        `);
        stmt.run(
            event.eventId,
            event.timestamp,
            JSON.stringify(event.phi.facts),
            JSON.stringify(event.phi.entities),
            event.psi.sentiment,
            event.psi.intent,
            event.psi.relational,
            event.rawUserMsg,
            event.rawAiReply
        );
        logger.debug(`[StructuredMemory] Inserted event ${event.eventId}`);
    }

    /**
     * Get all unconsolidated events (for ConsolidationCron).
     */
    public getUnconsolidatedEvents(): EventBrick[] {
        const stmt = this.db.prepare("SELECT * FROM events WHERE consolidated = 0 ORDER BY timestamp ASC");
        return (stmt.all() as any[]).map(r => this.mapEventRow(r));
    }

    /**
     * Get count of pending unconsolidated events (for Cold-start Preflight Check).
     */
    public getUnconsolidatedCount(): number {
        const row = this.db.prepare("SELECT count(*) as c FROM events WHERE consolidated = 0").get() as any;
        return row.c;
    }

    /**
     * Mark events as consolidated after successful L2 synthesis.
     */
    public markConsolidated(eventIds: string[]): void {
/* istanbul ignore next */
        if (eventIds.length === 0) return;
        const stmt = this.db.prepare("UPDATE events SET consolidated = 1 WHERE eventId = ?");
        for (const id of eventIds) {
            stmt.run(id);
        }
        logger.info(`[StructuredMemory] Marked ${eventIds.length} events as consolidated.`);
    }

    /**
     * Garbage collect old consolidated events (L1 = working buffer, not permanent store).
     * Deletes consolidated events older than retentionDays.
     */
    public gcOldEvents(retentionDays: number = 7): number {
        const cutoffMs = Date.now() - (retentionDays * 24 * 60 * 60 * 1000);
        const stmt = this.db.prepare("DELETE FROM events WHERE consolidated = 1 AND timestamp < ?");
        const result = stmt.run(cutoffMs);
/* istanbul ignore next */
        if (result.changes > 0) {
            logger.info(`[StructuredMemory] GC: Removed ${result.changes} old consolidated events (older than ${retentionDays} days).`);
        }
        return result.changes;
    }

    // ===========================
    // [LIVA-UHM Phase 3] L1 Turn Layer Methods
    // ===========================

    public insertTurnNode(turnId: string, temporal_anchor: number, userMsg: string, aiReply: string): void {
        try {
            const query = this.db.prepare(`
                INSERT INTO turn_layer_nodes (turnId, temporal_anchor, userMsg, aiReply, createdAt)
                VALUES (?, ?, ?, ?, ?)
            `);
            query.run(turnId, temporal_anchor, userMsg, aiReply, new Date().toISOString());
        } catch (error) {
            logger.error(`[StructuredMemory] Error inserting turn node: ${error}`);
        }
    }

    public getTurnsByTimeRange(fromTs: number, toTs: number): TurnNode[] {
        const query = this.db.prepare("SELECT * FROM turn_layer_nodes WHERE temporal_anchor >= ? AND temporal_anchor <= ? ORDER BY temporal_anchor ASC");
        return query.all(fromTs, toTs) as TurnNode[];
    }

    public getTurnsByIds(turnIds: string[]): TurnNode[] {
        if (turnIds.length === 0) return [];
        const placeholders = turnIds.map(() => '?').join(',');
        const query = this.db.prepare(`SELECT * FROM turn_layer_nodes WHERE turnId IN (${placeholders}) ORDER BY temporal_anchor ASC`);
        return query.all(...turnIds) as TurnNode[];
    }

    private mapEventRow(row: any): EventBrick {
        return {
            eventId: row.eventId,
            timestamp: row.timestamp,
            phi: {
                facts: JSON.parse(row.phi_facts || "[]"),
                entities: JSON.parse(row.phi_entities || "[]"),
            },
            psi: {
                sentiment: row.psi_sentiment || "",
                intent: row.psi_intent || "",
                relational: row.psi_relational || "",
            },
            rawUserMsg: row.rawUserMsg || "",
            rawAiReply: row.rawAiReply || "",
        };
    }
}

// ===========================
// [LIVA-UHM] Event Brick Type
// ===========================

export interface EventBrick {
    eventId: string;
    timestamp: number;
    phi: { facts: string[]; entities: string[] };
    psi: { sentiment: string; intent: string; relational: string };
    rawUserMsg: string;
    rawAiReply: string;
}
