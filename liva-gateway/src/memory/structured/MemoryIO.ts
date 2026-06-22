import { promises as fsp, constants as fsc } from "node:fs";

import { logger } from "../../utils/logger";
import { safeRename } from '../../utils/FileUtils';
import { EncryptionEngine } from "../EncryptionEngine";
import type { StructuredMemory } from "../StructuredMemory";
import type { IDBFactRow, IDBCountRow, StructuredFact } from "./types";

export class MemoryIO {
    #factTouchBuffer: Map<string, number> = new Map();
    #factTouchTimer: NodeJS.Timeout | null = null;

    static readonly FACT_TOUCH_FLUSH_MS = 60_000;
    static readonly MAX_FACTS = 50;
    static readonly MAX_KEY_LENGTH = 100;
    static readonly MAX_VALUE_LENGTH = 1000;

    constructor(private readonly parent: StructuredMemory) {}

    public getFact(key: string): StructuredFact | null {
        const stmt = this.parent.db.prepare("SELECT * FROM facts WHERE key = ?");
        const row = stmt.get(key) as unknown as IDBFactRow;
        if (!row) return null;
        this.parent.touchFact(key);
        return this.mapRow(row);
    }

    public async setFact(
        key: string,
        value: string,
        options: { ttlDays?: number; source?: string; category?: string } = {}
    ): Promise<void> {
        key = key.trim().substring(0, MemoryIO.MAX_KEY_LENGTH);
        value = value.trim().substring(0, MemoryIO.MAX_VALUE_LENGTH);
        
        if (!key || !value) {
            logger.warn("[StructuredMemory] Attempted to set empty key or value");
            return;
        }

        const now = new Date().toISOString();
        const ttlDays = options.ttlDays ?? null;
        const source = options.source || "agent";
        const category = options.category ?? null;

        const importance = options.source === "user" ? 1.0
            : options.source === "consolidation" ? 0.7 : 0.5;
        const confidenceScore = 1.0;
        const sourceTurnId = null;

        const encryptedValue = EncryptionEngine.encrypt(value);

        const res = await this.parent.dbBridge.run(`
            INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId, memory_strength, last_accessed_at, access_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1.0, ?, 0)
            ON CONFLICT(key) DO UPDATE SET 
                value = excluded.value, 
                updatedAt = excluded.updatedAt,
                ttlDays = COALESCE(excluded.ttlDays, facts.ttlDays),
                source = excluded.source,
                category = COALESCE(excluded.category, facts.category),
                importance = excluded.importance,
                confidenceScore = excluded.confidenceScore,
                memory_strength = MAX(facts.memory_strength, 0.8),
                last_accessed_at = excluded.last_accessed_at,
                access_count = facts.access_count + 1
        `, [key, encryptedValue, now, now, ttlDays, source, category, importance, confidenceScore, sourceTurnId, Date.now()]);
        
        if (res.changes > 0) {
           logger.info(`[StructuredMemory] Saved fact: "${key}"`);
        }

        await this.enforceCapacity();
    }

    public async setFactsBatch(
        facts: Array<{ key: string; value: string; options?: { ttlDays?: number; source?: string; category?: string } }>
    ): Promise<void> {
        if (facts.length === 0) return;

        const now = new Date().toISOString();
        const paramSets: any[][] = [];
        
        for (const fact of facts) {
            const key = fact.key.trim().substring(0, MemoryIO.MAX_KEY_LENGTH);
            const value = fact.value.trim().substring(0, MemoryIO.MAX_VALUE_LENGTH);
            
            if (!key || !value) {
                logger.warn("[StructuredMemory] Attempted to set empty key or value in batch");
                continue;
            }

            const options = fact.options ?? {};
            const ttlDays = options.ttlDays ?? null;
            const source = options.source || "agent";
            const category = options.category ?? null;

            const importance = source === "user" ? 1.0
                : source === "consolidation" ? 0.7 : 0.5;
            const confidenceScore = 1.0;
            const sourceTurnId = null;

            const encryptedValue = EncryptionEngine.encrypt(value);

            paramSets.push([
                key, encryptedValue, now, now, ttlDays, source, category, importance, confidenceScore, sourceTurnId, Date.now()
            ]);
        }

        if (paramSets.length === 0) return;

        const sql = `
            INSERT INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category, importance, confidenceScore, sourceTurnId, memory_strength, last_accessed_at, access_count)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 1.0, ?, 0)
            ON CONFLICT(key) DO UPDATE SET 
                value = excluded.value, 
                updatedAt = excluded.updatedAt,
                ttlDays = COALESCE(excluded.ttlDays, facts.ttlDays),
                source = excluded.source,
                category = COALESCE(excluded.category, facts.category),
                importance = excluded.importance,
                confidenceScore = excluded.confidenceScore,
                memory_strength = MAX(facts.memory_strength, 0.8),
                last_accessed_at = excluded.last_accessed_at,
                access_count = facts.access_count + 1
        `;

        await this.parent.dbBridge.runBatch(sql, paramSets);
        logger.info(`[StructuredMemory] Saved ${paramSets.length} facts in batch`);

        await this.enforceCapacity();
    }

    public async deleteFact(key: string): Promise<boolean> {
        const res = await this.parent.dbBridge.run("DELETE FROM facts WHERE key = ?", [key]);
        if (res.changes > 0) {
            logger.info(`[StructuredMemory] Deleted fact: "${key}"`);
            return true;
        }
        return false;
    }

    public getAllFacts(): StructuredFact[] {
        const stmt = this.parent.db.prepare("SELECT * FROM facts ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all() as unknown as IDBFactRow[]).map(r => this.mapRow(r));
    }

    public getFactsByCategory(category: string): StructuredFact[] {
        const stmt = this.parent.db.prepare("SELECT * FROM facts WHERE category = ? ORDER BY importance DESC, updatedAt DESC");
        return (stmt.all(category) as unknown as IDBFactRow[]).map(r => this.mapRow(r));
    }

    public get count(): number {
        const row = this.parent.db.prepare("SELECT count(*) as c FROM facts").get() as unknown as IDBCountRow;
        return row.c;
    }

    public async setFactImportance(key: string, importance: number): Promise<void> {
        await this.parent.dbBridge.run("UPDATE facts SET importance = ? WHERE key = ?", [importance, key]);
    }

    public async deleteAllFacts(): Promise<void> {
        await this.parent.dbBridge.exec("DELETE FROM facts");
        logger.warn("[StructuredMemory/GDPR] All facts permanently erased.");
    }

    public touchFact(key: string): void {
        if (this.parent.isClosed) return;
        this.#factTouchBuffer.set(key, Date.now());
        if (!this.#factTouchTimer) {
            this.#factTouchTimer = setTimeout(() => {
                this.flushFactTouches().catch(err => {
                    logger.error(`[StructuredMemory] Error flushing fact touches: ${err}`);
                });
            }, MemoryIO.FACT_TOUCH_FLUSH_MS);
            this.#factTouchTimer.unref();
        }
    }

    public async flushFactTouches(): Promise<void> {
        if (this.#factTouchBuffer.size === 0) return;
        const entries = Array.from(this.#factTouchBuffer.entries());
        this.#factTouchBuffer.clear();
        if (this.#factTouchTimer) { clearTimeout(this.#factTouchTimer); this.#factTouchTimer = null; }

        try {
            const paramSets = entries.map(([key, ts]) => [ts, key]);
            await this.parent.dbBridge.runBatch(
                "UPDATE facts SET memory_strength = 1.0, last_accessed_at = ?, access_count = access_count + 1 WHERE key = ?",
                paramSets
            );
            logger.debug(`[StructuredMemory/Touch] Flushed ${entries.length} fact touches.`);
        } catch (e: unknown) {
            // Re-queue failed entries
            for (const [key, ts] of entries) {
                this.#factTouchBuffer.set(key, ts);
            }
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.warn(`[StructuredMemory/Touch] Flush failed (re-queued): ${errMsg}`);
        }
    }

    public async applyFactDecay(decayRate: number): Promise<{ decayed: number; archived: number }> {
        const now = Date.now();
        const MS_PER_DAY = 86_400_000;
        const ARCHIVE_THRESHOLD = 0.1;
        const SPACING_COEFFICIENT = 0.15;
        const CHUNK_SIZE = 1000;

        const fetchStmt = this.parent.db.prepare(`
            SELECT key, memory_strength, last_accessed_at, access_count
            FROM facts
            WHERE last_accessed_at > 0
              AND last_accessed_at < ?
              AND key > ?
            ORDER BY key ASC
            LIMIT ?
        `);

        let decayedCount = 0;
        let archivedCount = 0;
        let lastKey = "";

        while (true) {
            const chunk = fetchStmt.all(now - MS_PER_DAY, lastKey, CHUNK_SIZE) as Array<{
                key: string; memory_strength: number; last_accessed_at: number; access_count: number;
            }>;

            if (chunk.length === 0) break;

            const updates: (string | number)[][] = [];
            const deletes: string[][] = [];

            for (const fact of chunk) {
                const daysSince = (now - fact.last_accessed_at) / MS_PER_DAY;
                const dynamicLambda = decayRate / (1 + SPACING_COEFFICIENT * (fact.access_count || 0));
                const currentStrength = fact.memory_strength ?? 1.0;
                const newStrength = currentStrength * Math.exp(-dynamicLambda * daysSince);

                if (newStrength < ARCHIVE_THRESHOLD) {
                    deletes.push([fact.key]);
                    archivedCount++;
                } else if (Math.abs(currentStrength - newStrength) > 0.01) {
                    updates.push([newStrength, fact.key]);
                    decayedCount++;
                }
                lastKey = fact.key;
            }

            const statements = [];
            if (updates.length > 0) {
                statements.push({ sql: "UPDATE facts SET memory_strength = ? WHERE key = ?", paramSets: updates });
            }
            if (deletes.length > 0) {
                statements.push({ sql: "DELETE FROM facts WHERE key = ?", paramSets: deletes });
            }

            if (statements.length > 0) {
                await this.parent.dbBridge.transactionBatch(statements);
            }

            if (chunk.length < CHUNK_SIZE) break;
            await new Promise(resolve => setImmediate(resolve)); // Yield to Event Loop
        }

        return { decayed: decayedCount, archived: archivedCount };
    }

    public async enforceCapacity(): Promise<void> {
        const res = await this.parent.dbBridge.run(`
            DELETE FROM facts WHERE key NOT IN (
                SELECT key FROM facts 
                ORDER BY importance DESC, updatedAt DESC, rowid DESC 
                LIMIT ?
            )
        `, [MemoryIO.MAX_FACTS]);
        if (res.changes > 0) {
            logger.warn(`[StructuredMemory] Evicted ${res.changes} oldest facts (FIFO capacity)`);
        }
    }

    public async evictExpired(): Promise<void> {
        const now = Date.now();
        const checkRows = this.parent.db.prepare("SELECT key, createdAt, ttlDays FROM facts WHERE ttlDays IS NOT NULL").all() as unknown as IDBFactRow[];
        
        let evicted = 0;
        const keysToDelete: string[][] = [];
        
        for (const row of checkRows) {
            const created = new Date(row.createdAt).getTime();
            if (row.ttlDays === null) continue;
            const ttlMs = row.ttlDays * 24 * 60 * 60 * 1000;
            if ((now - created) > ttlMs) {
                keysToDelete.push([row.key]);
                evicted++;
            }
        }

        if (keysToDelete.length > 0) {
            await this.parent.dbBridge.runBatch("DELETE FROM facts WHERE key = ?", keysToDelete);
        }

        if (evicted > 0) {
            logger.info(`[StructuredMemory] TTL eviction: removed ${evicted} expired facts`);
        }
    }

    public async migrateFromJson(jsonPath: string): Promise<void> {
        try {
            await fsp.access(jsonPath, fsc.F_OK);
        } catch {
            return; // File does not exist — nothing to migrate
        }

        try {
            const raw = await fsp.readFile(jsonPath, "utf-8");
            const parsed = JSON.parse(raw);
            if (parsed.facts && Array.isArray(parsed.facts)) {
                const stmt = this.parent.db.prepare("INSERT OR IGNORE INTO facts (key, value, createdAt, updatedAt, ttlDays, source, category) VALUES (?, ?, ?, ?, ?, ?, ?)");
                for (const fact of parsed.facts) {
                    const encryptedValue = EncryptionEngine.encrypt(fact.value);
                    stmt.run(fact.key, encryptedValue, fact.createdAt, fact.updatedAt, fact.ttlDays || null, fact.source, fact.category || null);
                }
                logger.info(`[StructuredMemory] Migrated ${parsed.facts.length} facts from JSON to SQLite`);
                await safeRename(jsonPath, jsonPath + ".bak");
            }
        } catch (e) {
            if (!process.env.VITEST) {
                logger.warn(`[StructuredMemory] JSON migration failed: ${e}`);
            }
        }
    }

    public formatForSystemPrompt(): string {
        const facts = this.getAllFacts()
            .filter(f => (f.memoryStrength ?? 1.0) >= 0.2)
            .slice(0, 20);
        if (facts.length === 0) return "";

        let output = "\n[BỘ NHỚ CẤU TRÚC — Kiến thức đã được xác nhận]\n";

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

    private mapRow(row: IDBFactRow): StructuredFact {
        return {
            key: row.key,
            value: EncryptionEngine.decrypt(row.value),
            createdAt: row.createdAt,
            updatedAt: row.updatedAt,
            ttlDays: row.ttlDays ?? undefined,
            source: row.source ?? "System",
            category: row.category ?? undefined,
            importance: row.importance ?? 0.5,
            confidenceScore: row.confidenceScore ?? 1.0,
            sourceTurnId: row.sourceTurnId ?? undefined,
            memoryStrength: row.memory_strength ?? 1.0,
            lastAccessedAt: row.last_accessed_at ?? 0,
            accessCount: row.access_count ?? 0,
        };
    }

    public saveBriefing(briefing: {
        id: string;
        topics: string;
        content: string;
        source?: string;
        ttlHours?: number;
    }): void {
        const ttl = (briefing.ttlHours ?? 24) * 60 * 60 * 1000;
        const now = Date.now();
        this.parent.db.prepare(`
            INSERT OR REPLACE INTO daily_briefings (id, created_at, topics, content, is_read, source, expires_at)
            VALUES (?, ?, ?, ?, 0, ?, ?)
        `).run(briefing.id, now, briefing.topics, briefing.content, briefing.source ?? "tavily", now + ttl);
        logger.info(`[v24 ShadowDigest] 📰 Briefing cached: ${briefing.id} (TTL: ${briefing.ttlHours ?? 24}h)`);
    }

    public getUnreadBriefings(limit: number = 5): Array<{ id: string; topics: string; content: string; created_at: number }> {
        const now = Date.now();
        return this.parent.db.prepare(`
            SELECT id, topics, content, created_at FROM daily_briefings
            WHERE is_read = 0 AND expires_at > ?
            ORDER BY created_at DESC LIMIT ?
        `).all(now, limit) as Array<{ id: string; topics: string; content: string; created_at: number }>;
    }

    public markBriefingRead(id: string): void {
        this.parent.db.prepare("UPDATE daily_briefings SET is_read = 1 WHERE id = ?").run(id);
    }

    public cleanExpiredBriefings(): number {
        const result = this.parent.db.prepare("DELETE FROM daily_briefings WHERE expires_at < ?").run(Date.now());
        return (result as { changes: number }).changes;
    }

    public getTasks(): Array<{ id: string; title: string; description: string; status: string; priority: string; result: string; created_at: number; updated_at: number }> {
        return this.parent.db.prepare(
            "SELECT id, title, description, status, priority, result, created_at, updated_at FROM tasks ORDER BY created_at DESC"
        ).all() as unknown as Array<{ id: string; title: string; description: string; status: string; priority: string; result: string; created_at: number; updated_at: number }>;
    }

    public addTask(task: { id: string; title: string; description?: string; priority?: string }): void {
        const now = Date.now();
        this.parent.db.prepare(
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
        this.parent.db.prepare(`UPDATE tasks SET ${fields.join(", ")} WHERE id = ?`).run(...values);
    }

    public deleteTask(id: string): void {
        this.parent.db.prepare("DELETE FROM tasks WHERE id = ?").run(id);
    }

    public async close(): Promise<void> {
        if (this.#factTouchTimer) { clearTimeout(this.#factTouchTimer); this.#factTouchTimer = null; }
        await this.flushFactTouches();
    }
}
