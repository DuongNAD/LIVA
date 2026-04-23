import { DatabaseSync } from "node:sqlite";
import * as fs from "fs";
import * as path from "path";
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
 *   - Native SQLite synchronization
 *   - System prompt injection formatting
 */

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
    private db: DatabaseSync;

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
    }

    private initStore(): void {
        // 🔒 [Audit Fix C-3] Enable WAL Mode for concurrent read/write without SQLITE_BUSY
        this.db.exec("PRAGMA journal_mode = WAL");
        this.db.exec("PRAGMA synchronous = NORMAL");
        this.db.exec(`
            CREATE TABLE IF NOT EXISTS facts (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL,
                createdAt TEXT NOT NULL,
                updatedAt TEXT NOT NULL,
                ttlDays INTEGER,
                source TEXT NOT NULL,
                category TEXT
            )
        `);
    }

    /**
     * 🔒 [Audit Fix C-3] Close SQLite connection for clean shutdown.
     * Called from MemoryManager.dispose() / CoreKernel.shutdown().
     */
    public close(): void {
        try {
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

        const stmt = this.db.prepare(`
            INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category)
            VALUES (?, ?, ?, ?, ?, ?, ?)
            ON CONFLICT(key) DO UPDATE SET 
                value = excluded.value, 
                updatedAt = excluded.updatedAt,
                ttlDays = COALESCE(excluded.ttlDays, facts.ttlDays),
                source = excluded.source,
                category = COALESCE(excluded.category, facts.category)
        `);
        
        stmt.run(key, value, now, now, ttlDays, source, category);
        
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
            // Dọn các tin cũ nhất (theo updatedAt để giữ lại các fact hay được quan tâm/sửa đổi)
            const stmt = this.db.prepare(`
                DELETE FROM facts WHERE key IN (
                    SELECT key FROM facts ORDER BY updatedAt ASC LIMIT ?
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
        this.evictExpired();
        const stmt = this.db.prepare("SELECT * FROM facts WHERE key = ?");
        const row = stmt.get(key) as any;
        return row ? this.mapRow(row) : null;
    }

    /**
     * Get all active facts (after TTL eviction)
     */
    public getAllFacts(): StructuredFact[] {
        this.evictExpired();
        const stmt = this.db.prepare("SELECT * FROM facts ORDER BY updatedAt DESC");
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
        this.evictExpired();
        const stmt = this.db.prepare("SELECT * FROM facts WHERE category = ? ORDER BY updatedAt DESC");
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
            const cat = fact.category || "Chung";
            if (!categories.has(cat)) categories.set(cat, []);
            categories.get(cat)!.push(fact);
        }

        for (const [category, catFacts] of categories) {
            output += `\n## ${category}\n`;
            for (const fact of catFacts) {
                output += `- ${fact.key}: ${fact.value}\n`;
            }
        }

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
            value: row.value,
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            ttlDays: row.ttlDays,
            source: row.source,
            category: row.category
        };
    }
}
