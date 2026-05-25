/**
 * DatabaseWorker.ts — Isolated SQLite Thread (Preventing Event Loop Blocking)
 * =========================================================================
 * [v26 Enterprise-Ready Pillar 1]
 *
 * Runs all synchronous node:sqlite and sqlite-vec/FTS5 operations in a
 * dedicated worker_thread, communicating asynchronously with the main thread.
 *
 * PROTOCOL (parentPort messages):
 * Parent → Worker:  { type: "init", dbPath: string, options?: { allowExtension?: boolean } }
 * Parent → Worker:  { id: string, type: "exec", sql: string }
 * Parent → Worker:  { id: string, type: "run", sql: string, params: any[] }
 * Parent → Worker:  { id: string, type: "all", sql: string, params: any[] }
 * Parent → Worker:  { id: string, type: "get", sql: string, params: any[] }
 * Parent → Worker:  { id: string, type: "backup", backupPath: string }
 * Parent → Worker:  { type: "close" }
 *
 * Worker → Parent:  { type: "ready" }
 * Worker → Parent:  { id: string, type: "result", data: any }
 * Worker → Parent:  { id: string, type: "error", message: string }
 */

import { parentPort } from "node:worker_threads";
import { DatabaseSync } from "node:sqlite";
import * as fs from "node:fs";
import * as path from "node:path";
import * as sqliteVec from "sqlite-vec";
import { TraceContext } from "../utils/TraceContext";
import { logger } from "../utils/logger";

let db: DatabaseSync | null = null;

function initialize(dbPath: string, options?: { allowExtension?: boolean }): void {
    try {
        // Ensure parent directory exists
        if (dbPath !== ":memory:") {
            const dir = path.dirname(dbPath);
            if (!fs.existsSync(dir)) {
                fs.mkdirSync(dir, { recursive: true });
            }
        }

        const backupPath = `${dbPath}.bak`;

        const openAndCheck = (targetPath: string): DatabaseSync => {
            const tempDb = new DatabaseSync(targetPath, {
                allowExtension: options?.allowExtension ?? true,
            });
            // Load sqlite-vec extension
            sqliteVec.load(tempDb);
            
            // integrity check
            const check = tempDb.prepare("PRAGMA integrity_check").get() as { integrity_check: string } | undefined;
            if (!check || check.integrity_check !== 'ok') {
                tempDb.close();
                throw new Error(`Integrity check failed: ${JSON.stringify(check)}`);
            }
            return tempDb;
        };

        try {
            db = openAndCheck(dbPath);
        } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
            
            // If primary is corrupt or fails to open, attempt backup restore
            if (dbPath !== ":memory:" && fs.existsSync(backupPath)) {
                try {
                    // Copy backup to primary (overwrite)
                    fs.copyFileSync(backupPath, dbPath);
                    
                    // Also delete wal/shm if they exist to prevent WAL synchronization conflicts
                    const shmPath = `${dbPath}-shm`;
                    const walPath = `${dbPath}-wal`;
                    if (fs.existsSync(shmPath)) {
                        try { fs.unlinkSync(shmPath); } catch {}
                    }
                    if (fs.existsSync(walPath)) {
                        try { fs.unlinkSync(walPath); } catch {}
                    }

                    // Re-open restored database
                    db = openAndCheck(dbPath);
                } catch (restoreErr: unknown) {
                    const restoreMsg = restoreErr instanceof Error ? restoreErr.message : String(restoreErr);
                    throw new Error(`Primary corrupt (${errMsg}) and backup restore failed: ${restoreMsg}`);
                }
            } else {
                throw new Error(`Primary corrupt (${errMsg}) and no backup file found.`);
            }
        }

        // Enterprise SQLite Performance Tuning
        db.exec("PRAGMA journal_mode = WAL");
        db.exec("PRAGMA synchronous = NORMAL");
        db.exec("PRAGMA busy_timeout = 5000");
        db.exec("PRAGMA wal_autocheckpoint = 500");
        db.exec("PRAGMA cache_size = -8192");
        db.exec("PRAGMA page_size = 32768");
        db.exec("PRAGMA mmap_size = 268435456"); // 256MB mmap

        parentPort?.postMessage({ type: "ready" });
    } catch (err: unknown) {
        const msg = err instanceof Error ? err.message : String(err);
        parentPort?.postMessage({ type: "error", message: `Database init failed: ${msg}` });
    }
}

function handleMessage(msg: {
    id?: string;
    type: "init" | "exec" | "run" | "all" | "get" | "backup" | "close" | "ping";
    dbPath?: string;
    options?: { allowExtension?: boolean };
    sql?: string;
    params?: any[];
    backupPath?: string;
    traceId?: string;
}): void {
    const { id, type, traceId } = msg;

    if (type === "init") {
        initialize(msg.dbPath!, msg.options);
        return;
    }

    if (type === "ping") {
        parentPort?.postMessage({ type: "pong" });
        return;
    }

    if (type === "close") {
        try {
            if (db) {
                db.close();
                db = null;
            }
            process.exit(0);
        } catch (err: unknown) {
            process.exit(1);
        }
    }

    const activeDb = db;
    if (!activeDb) {
        if (id) {
            parentPort?.postMessage({
                id,
                type: "error",
                message: "Database not initialized",
            });
        }
        return;
    }

    TraceContext.run(() => {
        try {
            switch (type) {
                case "exec": {
                    activeDb.exec(msg.sql!);
                    parentPort?.postMessage({ id, type: "result", data: null });
                    break;
                }
                case "run": {
                    const stmt = activeDb.prepare(msg.sql!);
                    const res = stmt.run(...(msg.params || []));
                    parentPort?.postMessage({
                        id,
                        type: "result",
                        data: {
                            changes: res.changes,
                            lastInsertRowid: res.lastInsertRowid ? Number(res.lastInsertRowid) : null,
                        },
                    });
                    break;
                }
                case "all": {
                    const stmt = activeDb.prepare(msg.sql!);
                    const res = stmt.all(...(msg.params || []));
                    parentPort?.postMessage({ id, type: "result", data: res });
                    break;
                }
                case "get": {
                    const stmt = activeDb.prepare(msg.sql!);
                    const res = stmt.get(...(msg.params || []));
                    parentPort?.postMessage({ id, type: "result", data: res });
                    break;
                }
                case "backup": {
                    const bPath = msg.backupPath!;
                    // Ensure backup folder exists
                    const dir = path.dirname(bPath);
                    if (!fs.existsSync(dir)) {
                        fs.mkdirSync(dir, { recursive: true });
                    }

                    // Delete old backup if exists to prevent SQLite error
                    if (fs.existsSync(bPath)) {
                        fs.unlinkSync(bPath);
                    }

                    // Atomic Snapshot via VACUUM INTO
                    const stmt = activeDb.prepare("VACUUM INTO ?");
                    stmt.run(bPath);

                    parentPort?.postMessage({ id, type: "result", data: { backupPath: bPath } });
                    break;
                }
                default:
                    throw new Error(`Unsupported message type: ${type}`);
            }
        } catch (err: unknown) {
            const message = err instanceof Error ? err.message : String(err);
            logger.error(`[DatabaseWorker] Query failed: ${message}. SQL: ${msg.sql || "N/A"}`);
            parentPort?.postMessage({ id, type: "error", message });
        }
    }, traceId);
}

parentPort?.on("message", handleMessage);
