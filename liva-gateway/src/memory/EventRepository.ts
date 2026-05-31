import { DatabaseWorkerBridge } from "./DatabaseWorkerBridge";
import { logger } from "../utils/logger";

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
    agentId: string;
}

export interface IDBCountRow {
    c: number;
}

export interface EventBrick {
    eventId: string;
    timestamp: number;
    phi: { facts: string[]; entities: string[] };
    psi: { sentiment: string; intent: string; relational: string };
    rawUserMsg: string;
    rawAiReply: string;
    domain?: string;
    category?: string;
    traceKeywords?: string[];
    last_accessed_at?: number;
    agentId?: string;
}

export interface TurnNode {
    turnId: string;
    temporal_anchor: number;
    userMsg: string;
    aiReply: string;
    createdAt: string;
}

/**
 * EventRepository
 * Encapsulates all event/turn layer operations asynchronously via DatabaseWorker.
 */
export class EventRepository {
    readonly #db: DatabaseWorkerBridge;
    readonly agentId: string;

    // Memory Touch — Debounced & Bounded
    #touchQueue: Set<string> = new Set();
    #touchFlushTimer: ReturnType<typeof setInterval> | null = null;
    #isTouchFlushing = false;
    static readonly TOUCH_QUEUE_CAPACITY = 1000;
    static readonly TOUCH_EARLY_FLUSH = 900;
    static readonly TOUCH_FLUSH_INTERVAL_MS = 15_000;

    constructor(db: DatabaseWorkerBridge, agentId: string = "liva_core") {
        this.#db = db;
        this.agentId = agentId;
    }

    /**
     * Start the debounced memory touch timer.
     * Called after construction by StructuredMemory.
     */
    public startTouchDebounce(): void {
        this.#touchFlushTimer = setInterval(() => {
            if (this.#touchQueue.size > 0 && !this.#isTouchFlushing) {
                Promise.resolve().then(() => this.flushTouchQueue());
            }
        }, EventRepository.TOUCH_FLUSH_INTERVAL_MS);
        if (this.#touchFlushTimer.unref) this.#touchFlushTimer.unref();
    }

    // ===========================
    // Memory Touch — Debounced & Bounded
    // ===========================

    public queueMemoryTouch(eventId: string): void {
        if (this.#touchQueue.size >= EventRepository.TOUCH_QUEUE_CAPACITY) return;
        this.#touchQueue.add(eventId);
        if (this.#touchQueue.size >= EventRepository.TOUCH_EARLY_FLUSH && !this.#isTouchFlushing) {
            Promise.resolve().then(() => this.flushTouchQueue());
        }
    }

    public async flushTouchQueue(): Promise<void> {
        if (this.#touchQueue.size === 0 || this.#isTouchFlushing) return;
        this.#isTouchFlushing = true;
        const items = Array.from(this.#touchQueue);
        this.#touchQueue.clear();
        try {
            const placeholders = items.map(() => '?').join(',');
            await this.#db.prepare(
                `UPDATE events SET last_accessed_at = ? WHERE eventId IN (${placeholders})`
            ).run(Date.now(), ...items);
            logger.debug(`[StructuredMemory/Touch] Flushed ${items.length} memory touches.`);
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[StructuredMemory/Touch] Flush failed: ${errMsg}`);
            items.forEach(item => this.#touchQueue.add(item));
        } finally {
            this.#isTouchFlushing = false;
        }
    }

    // ===========================
    // Event Brick CRUD (Dual-Perspective Φ/Ψ)
    // ===========================

    public async insertEvent(event: EventBrick): Promise<void> {
        const stmt = this.#db.prepare(`
            INSERT OR REPLACE INTO events 
            (eventId, timestamp, phi_facts, phi_entities, psi_sentiment, psi_intent, psi_relational, rawUserMsg, rawAiReply, consolidated, domain, category, trace_keywords, last_accessed_at, consolidation_status, retry_count, agentId)
            VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, 0, ?, ?, ?, ?, 'pending', 0, ?)
        `);
        await stmt.run(
            event.eventId,
            event.timestamp,
            JSON.stringify(event.phi.facts),
            JSON.stringify(event.phi.entities),
            event.psi.sentiment,
            event.psi.intent,
            event.psi.relational,
            event.rawUserMsg,
            event.rawAiReply,
            event.domain ?? 'General',
            event.category ?? 'Uncategorized',
            JSON.stringify(event.traceKeywords ?? []),
            event.last_accessed_at ?? 0,
            this.agentId
        );
        logger.debug(`[StructuredMemory] Inserted event ${event.eventId}`);
    }

    public async getUnconsolidatedEvents(): Promise<EventBrick[]> {
        const stmt = this.#db.prepare("SELECT * FROM events WHERE consolidated = 0 AND consolidation_status = 'pending' AND agentId = ? ORDER BY timestamp ASC");
        const rows = await stmt.all(this.agentId) as unknown as IDBEventRow[];
        return rows.map(r => this.mapEventRow(r));
    }

    public async getUnconsolidatedCount(): Promise<number> {
        const row = await this.#db.prepare("SELECT count(*) as c FROM events WHERE consolidated = 0 AND consolidation_status = 'pending' AND agentId = ?").get(this.agentId) as unknown as IDBCountRow;
        return row ? row.c : 0;
    }

    public async markConsolidated(eventIds: string[]): Promise<void> {
        if (eventIds.length === 0) return;
        const stmt = this.#db.prepare("UPDATE events SET consolidated = 1, consolidation_status = 'consolidated' WHERE eventId = ?");
        for (const id of eventIds) {
            await stmt.run(id);
        }
        logger.info(`[StructuredMemory] Marked ${eventIds.length} events as consolidated.`);
    }

    /**
     * [UHM-v3 DLQ] Mark events as dead-letter after 3 failed consolidation attempts.
     * DLQ events are excluded from getUnconsolidatedEvents() — prevents infinite retry loop.
     */
    public async markDLQ(eventIds: string[]): Promise<void> {
        if (eventIds.length === 0) return;
        const stmt = this.#db.prepare("UPDATE events SET consolidation_status = 'dlq' WHERE eventId = ?");
        for (const id of eventIds) {
            await stmt.run(id);
        }
        logger.warn(`[StructuredMemory/DLQ] Moved ${eventIds.length} events to Dead Letter Queue after 3 failed attempts.`);
    }

    /**
     * [UHM-v3 DLQ] Increment retry count for events that failed Zod validation during consolidation.
     */
    public async incrementRetryCount(eventIds: string[]): Promise<void> {
        if (eventIds.length === 0) return;
        const stmt = this.#db.prepare("UPDATE events SET retry_count = retry_count + 1 WHERE eventId = ?");
        for (const id of eventIds) {
            await stmt.run(id);
        }
    }

    public async gcOldEvents(retentionDays: number = 7): Promise<number> {
        const cutoffMs = Date.now() - (retentionDays * 24 * 60 * 60 * 1000);
        const stmt = this.#db.prepare("DELETE FROM events WHERE consolidated = 1 AND timestamp < ? AND agentId = ?");
        const result = await stmt.run(cutoffMs, this.agentId);
        if (result.changes > 0) {
            logger.info(`[StructuredMemory] GC: Removed ${result.changes} old consolidated events for agent ${this.agentId} (older than ${retentionDays} days).`);
        }
        return Number(result.changes);
    }

    public async deleteAllEvents(): Promise<void> {
        await this.#db.exec("DELETE FROM events");
        await this.#db.exec("DELETE FROM turn_layer_nodes");
        logger.warn("[StructuredMemory/GDPR] All events and turn nodes permanently erased.");
    }

    // ===========================
    // L1 Turn Layer Methods
    // ===========================

    public async insertTurnNode(turnId: string, temporal_anchor: number, userMsg: string, aiReply: string): Promise<void> {
        try {
            const query = this.#db.prepare(`
                INSERT INTO turn_layer_nodes (turnId, temporal_anchor, userMsg, aiReply, createdAt, agentId)
                VALUES (?, ?, ?, ?, ?, ?)
            `);
            await query.run(turnId, temporal_anchor, userMsg, aiReply, new Date().toISOString(), this.agentId);
        } catch (error) {
            logger.error(`[StructuredMemory] Error inserting turn node: ${error}`);
        }
    }

    public async getTurnsByTimeRange(fromTs: number, toTs: number): Promise<TurnNode[]> {
        const query = this.#db.prepare("SELECT * FROM turn_layer_nodes WHERE temporal_anchor >= ? AND temporal_anchor <= ? AND agentId = ? ORDER BY temporal_anchor ASC");
        return await query.all(fromTs, toTs, this.agentId) as unknown as TurnNode[];
    }

    public async getTurnsByIds(turnIds: string[]): Promise<TurnNode[]> {
        if (turnIds.length === 0) return [];
        const placeholders = turnIds.map(() => '?').join(',');
        const query = this.#db.prepare(`SELECT * FROM turn_layer_nodes WHERE turnId IN (${placeholders}) AND agentId = ? ORDER BY temporal_anchor ASC`);
        return await query.all(...turnIds, this.agentId) as unknown as TurnNode[];
    }

    // ===========================
    // Shutdown helpers
    // ===========================

    public async flushAndStop(): Promise<void> {
        if (this.#touchQueue.size > 0) {
            const items = Array.from(this.#touchQueue);
            this.#touchQueue.clear();
            try {
                const placeholders = items.map(() => '?').join(',');
                await this.#db.prepare(
                    `UPDATE events SET last_accessed_at = ? WHERE eventId IN (${placeholders})`
                ).run(Date.now(), ...items);
            } catch { /* ignore */ }
        }

        if (this.#touchFlushTimer) {
            clearInterval(this.#touchFlushTimer);
            this.#touchFlushTimer = null;
        }
    }

    // ===========================
    // Private
    // ===========================

    private mapEventRow(row: IDBEventRow): EventBrick {
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
            domain: row.domain ?? 'General',
            category: row.category ?? 'Uncategorized',
            traceKeywords: JSON.parse(row.trace_keywords || '[]'),
            last_accessed_at: row.last_accessed_at ?? 0,
        };
    }
}
