/**
 * IStorageProvider — Cloud-Native Storage Abstraction (MEM-108)
 * ==============================================================
 * Abstract interface for LIVA's persistence layer. Decouples the
 * memory system from the concrete storage implementation (SQLite,
 * libSQL/Turso, PostgreSQL, etc.).
 *
 * Current Implementation: SQLiteStorageProvider (local)
 * Future Implementations:
 *   - TursoStorageProvider (libSQL edge replicas)
 *   - PostgresStorageProvider (managed cloud DB)
 *
 * This abstraction enables:
 *   1. Horizontal pod scaling (Kubernetes) by switching to a shared DB
 *   2. Offline-first sync via libSQL embedded replicas
 *   3. Zero-downtime storage migration via Feature Flags
 *
 * [v4.0] Phase 4 — Cloud-Native Readiness
 */

import { logger } from "../utils/logger";

// ===========================
// Abstract Interface
// ===========================

export interface IStorageProvider {
    /** Initialize connection (create tables, run migrations) */
    initialize(): Promise<void>;

    /** Close connection and release resources */
    close(): Promise<void>;

    // --- Key-Value Operations ---

    /** Get a single value by key from a table */
    get(table: string, key: string): Promise<Record<string, any> | null>;

    /** Get all rows from a table, optionally with filter */
    getAll(table: string, filter?: Record<string, any>): Promise<Record<string, any>[]>;

    /** Insert or update a row */
    upsert(table: string, key: string, data: Record<string, any>): Promise<void>;

    /** Delete a row by key */
    delete(table: string, key: string): Promise<boolean>;

    /** Delete all rows in a table */
    deleteAll(table: string): Promise<void>;

    /** Count rows in a table */
    count(table: string): Promise<number>;

    /** Execute a raw query (for complex operations) */
    exec(sql: string, params?: any[]): Promise<void>;
}

// ===========================
// Local SQLite Implementation
// ===========================

/**
 * SQLiteStorageProvider — Local-first storage using Node.js built-in SQLite.
 * This is the default provider for single-instance deployments.
 *
 * Future migration path:
 *   1. Implement TursoStorageProvider with same interface
 *   2. Toggle via Feature Flag: FF_STORAGE_PROVIDER=turso
 *   3. Data migrated via background sync job
 */
export class SQLiteStorageProvider implements IStorageProvider {
    private db: import("node:sqlite").DatabaseSync | null = null;
    private readonly dbPath: string;

    constructor(dbPath: string) {
        this.dbPath = dbPath;
    }

    async initialize(): Promise<void> {
        const { DatabaseSync } = await import("node:sqlite");
        this.db = new DatabaseSync(this.dbPath);
        this.db.exec("PRAGMA journal_mode = WAL");
        this.db.exec("PRAGMA synchronous = NORMAL");
        this.db.exec("PRAGMA busy_timeout = 5000");
        this.db.exec("PRAGMA wal_autocheckpoint = 1000");
        logger.info(`[SQLiteStorage] Initialized: ${this.dbPath}`);
    }

    async close(): Promise<void> {
        if (this.db) {
            this.db.close();
            this.db = null;
        }
    }

    async get(table: string, key: string): Promise<Record<string, any> | null> {
        if (!this.db) return null;
        const stmt = this.db.prepare(`SELECT * FROM ${table} WHERE key = ?`);
        const row = stmt.get(key) as Record<string, any> | undefined;
        return row ?? null;
    }

    async getAll(table: string, filter?: Record<string, any>): Promise<Record<string, any>[]> {
        if (!this.db) return [];
        if (filter && Object.keys(filter).length > 0) {
            const clauses = Object.keys(filter).map(k => `${k} = ?`).join(" AND ");
            const values = Object.values(filter);
            const stmt = this.db.prepare(`SELECT * FROM ${table} WHERE ${clauses}`);
            return stmt.all(...values) as Record<string, any>[];
        }
        return this.db.prepare(`SELECT * FROM ${table}`).all() as Record<string, any>[];
    }

    async upsert(table: string, key: string, data: Record<string, any>): Promise<void> {
        if (!this.db) return;
        const columns = Object.keys(data).join(", ");
        const placeholders = Object.keys(data).map(() => "?").join(", ");
        const updates = Object.keys(data).filter(k => k !== "key").map(k => `${k} = excluded.${k}`).join(", ");
        const sql = `INSERT INTO ${table} (${columns}) VALUES (${placeholders}) ON CONFLICT(key) DO UPDATE SET ${updates}`;
        this.db.prepare(sql).run(...Object.values(data));
    }

    async delete(table: string, key: string): Promise<boolean> {
        if (!this.db) return false;
        const result = this.db.prepare(`DELETE FROM ${table} WHERE key = ?`).run(key);
        return result.changes > 0;
    }

    async deleteAll(table: string): Promise<void> {
        if (!this.db) return;
        this.db.exec(`DELETE FROM ${table}`);
    }

    async count(table: string): Promise<number> {
        if (!this.db) return 0;
        const row = this.db.prepare(`SELECT count(*) as c FROM ${table}`).get() as { c: number } | undefined;
        return row?.c ?? 0;
    }

    async exec(sql: string): Promise<void> {
        if (!this.db) return;
        this.db.exec(sql);
    }
}

// ===========================
// Factory — Provider Selection
// ===========================

/**
 * Create a storage provider based on configuration.
 * Currently only supports SQLite. Future: check FF_STORAGE_PROVIDER
 * env var to select between sqlite, turso, postgres.
 */
export function createStorageProvider(dbPath: string): IStorageProvider {
    const providerType = process.env.STORAGE_PROVIDER || "sqlite";

    switch (providerType) {
        case "sqlite":
            return new SQLiteStorageProvider(dbPath);
        // Future implementations:
        // case "turso":
        //     return new TursoStorageProvider(process.env.TURSO_URL!, process.env.TURSO_TOKEN!);
        // case "postgres":
        //     return new PostgresStorageProvider(process.env.DATABASE_URL!);
        default:
            logger.warn(`[StorageFactory] Unknown provider "${providerType}", falling back to SQLite`);
            return new SQLiteStorageProvider(dbPath);
    }
}
