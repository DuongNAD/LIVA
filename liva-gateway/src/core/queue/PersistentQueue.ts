import { DatabaseSync } from "node:sqlite";
import { logger } from "../../utils/logger";
import * as path from "node:path";
import * as fs from "node:fs";

/**
 * PersistentQueue — Crash-resilient message queue backed by SQLite.
 *
 * Replaces the volatile `#zaloPendingQueue: string[]` in AgentLoop
 * with a persistent queue that survives process restarts.
 *
 * Uses Node.js built-in `node:sqlite` (no native C++ deps).
 * Data lives at `data/agents/liva_core/pending_queue.db` by default.
 *
 * DESIGN: Lazy initialization — DB opens on first enqueue/dequeue,
 * not in the constructor. This prevents "database is locked" errors
 * when multiple instances share the same default path (e.g., tests).
 */

export class PersistentQueue {
    #db: DatabaseSync | null = null;
    #dbPath: string;

    constructor(dbPath?: string) {
        this.#dbPath = dbPath || path.join(process.cwd(), "data", "agents", "liva_core", "pending_queue.db");
    }

    /**
     * Lazy-open SQLite connection on first use.
     */
    #ensureDb(): DatabaseSync {
        if (this.#db) return this.#db;

        // Ensure parent directory exists
        // NOTE: Sync I/O is acceptable here because DatabaseSync itself is a sync API.
        // All operations in this class (enqueue/dequeue) are inherently synchronous.
        const dir = path.dirname(this.#dbPath);
        if (!fs.existsSync(dir)) {
            fs.mkdirSync(dir, { recursive: true });
        }

        this.#db = new DatabaseSync(this.#dbPath);
        this.#db.exec("PRAGMA journal_mode = WAL");
        this.#db.exec("PRAGMA synchronous = NORMAL");
        this.#db.exec(`
            CREATE TABLE IF NOT EXISTS pending_messages (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                channel TEXT NOT NULL,
                message TEXT NOT NULL,
                created_at INTEGER NOT NULL DEFAULT (unixepoch())
            )
        `);

        logger.info(`[PersistentQueue] Initialized at ${this.#dbPath}`);
        return this.#db;
    }

    /**
     * Add a message to the queue for a given channel.
     */
    public enqueue(channel: string, message: string): void {
        const db = this.#ensureDb();
        const stmt = db.prepare(
            "INSERT INTO pending_messages (channel, message) VALUES (?, ?)"
        );
        stmt.run(channel, message);
        logger.info(`[PersistentQueue] Enqueued message for channel "${channel}" (${message.substring(0, 50)}...)`);
    }

    /**
     * Remove and return all pending messages for a given channel.
     * Returns messages in FIFO order (oldest first).
     */
    public dequeueAll(channel: string): string[] {
        const db = this.#ensureDb();
        const rows = db.prepare(
            "SELECT id, message FROM pending_messages WHERE channel = ? ORDER BY id ASC"
        ).all(channel) as { id: number; message: string }[];

        if (rows.length === 0) return [];

        // Delete all dequeued messages
        const ids = rows.map(r => r.id);
        db.prepare(
            `DELETE FROM pending_messages WHERE id IN (${ids.map(() => '?').join(',')})`
        ).run(...ids);

        logger.info(`[PersistentQueue] Dequeued ${rows.length} messages for channel "${channel}"`);
        return rows.map(r => r.message);
    }

    /**
     * Get the number of pending messages for a channel.
     */
    public count(channel: string): number {
        // If DB hasn't been opened yet, queue is empty by definition
        if (!this.#db) return 0;
        const row = this.#db.prepare(
            "SELECT COUNT(*) as cnt FROM pending_messages WHERE channel = ?"
        ).get(channel) as { cnt: number } | undefined;
        return row?.cnt ?? 0;
    }

    /**
     * Check if the queue is empty for a given channel.
     */
    public isEmpty(channel: string): boolean {
        return this.count(channel) === 0;
    }

    /**
     * Dispose the queue and close the database connection.
     */
    public dispose(): void {
        if (!this.#db) return; // Nothing to close
        try {
            this.#db.close();
            this.#db = null;
            logger.info("[PersistentQueue] Database closed.");
        } catch (e: unknown) {
            const errMsg = e instanceof Error ? e.message : String(e);
            logger.error(`[PersistentQueue] Close failed: ${errMsg}`);
        }
    }
}
