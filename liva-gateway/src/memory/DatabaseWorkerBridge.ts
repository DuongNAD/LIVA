/**
 * DatabaseWorkerBridge — Main-thread Bridge to DatabaseWorker (Worker Thread)
 * ========================================================================
 * [v26 Enterprise-Ready Pillar 1]
 *
 * Provides a Promise-based asynchronous API for the main thread to interact
 * with SQLite database running in a separate worker_thread.
 */

import { Worker } from "node:worker_threads";
import * as path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { logger } from "../utils/logger";
import { randomUUID } from "node:crypto";
import { TraceContext } from "../utils/TraceContext";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));

const QUERY_TIMEOUT_MS = 30_000; // 30s query timeout
const MAX_RECOVERY_ATTEMPTS = 3;
const WATCHDOG_PING_MS = 10_000;
const WATCHDOG_TIMEOUT_MS = 25_000;

export class DatabaseWorkerBridge {
    #worker: Worker | null = null;
    #isReady = false;
    #dbPath: string;
    #options?: { allowExtension?: boolean };
    #pendingQueries = new Map<
        string,
        {
            resolve: (val: any) => void;
            reject: (err: Error) => void;
            timeoutId: NodeJS.Timeout;
            sqlForLog?: string;
        }
    >();
    #crashCount = 0;
    #watchdogInterval: ReturnType<typeof setInterval> | null = null;
    #lastPongTime = 0;
    #recoveryTimer: ReturnType<typeof setTimeout> | null = null;

    constructor(dbPath: string, options?: { allowExtension?: boolean }) {
        this.#dbPath = dbPath;
        this.#options = options;
    }

    /**
     * Start the worker thread and initialize connection
     */
    async initialize(): Promise<void> {
        return new Promise((resolve, reject) => {
            // Locate DatabaseWorker.ts. Under build, it will compile to DatabaseWorker.js
            const workerPath = path.join(_dirname, "..", "workers", "DatabaseWorker.ts");

            if (process.env.NODE_ENV === "production") {
                const prodWorkerPath = workerPath.replace(/\.ts$/, ".js");
                this.#worker = new Worker(prodWorkerPath);
            } else {
                const workerUrl = pathToFileURL(workerPath).href;
                this.#worker = new Worker(
                    `
                    import { register } from 'node:module';
                    import { pathToFileURL } from 'node:url';
                    register('tsx', pathToFileURL('./'), { data: {} });
                    import('${workerUrl.replace(/\\/g, "\\\\")}');
                    `,
                    {
                        eval: true,
                        execArgv: []
                    }
                );
            }

            const timeout = setTimeout(() => {
                reject(new Error("Database worker initialization timed out (15s)"));
            }, 15000);

            this.#worker.on("message", (msg: {
                id?: string;
                type: "ready" | "pong" | "result" | "error";
                data?: any;
                message?: string;
            }) => {
                if (msg.type === "ready") {
                    this.#isReady = true;
                    this.#lastPongTime = Date.now();
                    clearTimeout(timeout);
                    logger.info(`[DatabaseWorkerBridge] ✅ Database worker thread initialized for: ${path.basename(this.#dbPath)}`);
                    this.#startWatchdog();
                    resolve();
                    return;
                }

                if (msg.type === "pong") {
                    this.#lastPongTime = Date.now();
                    return;
                }

                if (msg.id) {
                    const pending = this.#pendingQueries.get(msg.id);
                    if (pending) {
                        clearTimeout(pending.timeoutId);
                        this.#pendingQueries.delete(msg.id);

                        if (msg.type === "result") {
                            pending.resolve(msg.data);
                        } else {
                            const err = new Error(msg.message || "Query failed");
                            pending.reject(err);
                        }
                    }
                } else if (msg.type === "error") {
                    logger.error(`[DatabaseWorkerBridge] ❌ Worker error: ${msg.message}`);
                    if (!this.#isReady) {
                        clearTimeout(timeout);
                        reject(new Error(msg.message));
                    }
                }
            });

            this.#worker.on("error", (err: Error) => {
                logger.error(`[DatabaseWorkerBridge] ❌ Database worker crashed: ${err.message}`);
                this.#isReady = false;
                this.#rejectAllPending(err);
                this.#attemptRecovery();
            });

            this.#worker.on("exit", (code) => {
                if (code !== 0) {
                    logger.warn(`[DatabaseWorkerBridge] Database worker exited with code ${code}`);
                }
                this.#isReady = false;
                this.#rejectAllPending(new Error("Database worker exited unexpectedly"));
            });

            // Send initialization payload
            this.#worker.postMessage({
                type: "init",
                dbPath: this.#dbPath,
                options: this.#options,
            });
        });
    }

    /**
     * Terminate all pending queries and reject their promises
     */
    #rejectAllPending(err: Error): void {
        for (const [id, query] of this.#pendingQueries.entries()) {
            clearTimeout(query.timeoutId);
            query.reject(err);
        }
        this.#pendingQueries.clear();
    }

    /**
     * Send query message to worker and wrap in a Promise
     */
    async #sendQuery<T = any>(
        type: "exec" | "run" | "all" | "get" | "backup",
        payload: { sql?: string; params?: any[]; backupPath?: string }
    ): Promise<T> {
        if (!this.#isReady || !this.#worker) {
            throw new Error("Database worker not ready or disposed");
        }

        const id = randomUUID();
        return new Promise<T>((resolve, reject) => {
            const timeoutId = setTimeout(() => {
                this.#pendingQueries.delete(id);
                reject(new Error(`Query timed out after ${QUERY_TIMEOUT_MS}ms. SQL: ${payload.sql || "N/A"}`));
            }, QUERY_TIMEOUT_MS);

            this.#pendingQueries.set(id, {
                resolve,
                reject,
                timeoutId,
                sqlForLog: payload.sql,
            });

            this.#worker!.postMessage({
                id,
                type,
                traceId: TraceContext.getTraceId(),
                ...payload,
            });
        });
    }

    async exec(sql: string): Promise<void> {
        return this.#sendQuery<void>("exec", { sql });
    }

    async run(sql: string, params?: any[]): Promise<{ changes: number; lastInsertRowid: number | null }> {
        return this.#sendQuery<{ changes: number; lastInsertRowid: number | null }>("run", { sql, params });
    }

    async all<T = any>(sql: string, params?: any[]): Promise<T[]> {
        return this.#sendQuery<T[]>("all", { sql, params });
    }

    async get<T = any>(sql: string, params?: any[]): Promise<T | null> {
        return this.#sendQuery<T | null>("get", { sql, params });
    }

    /**
     * Emulates DatabaseSync's prepare() method returning async methods for seamless migration.
     */
    prepare(sql: string) {
        return {
            all: async (...params: any[]): Promise<any[]> => this.all(sql, params),
            get: async (...params: any[]): Promise<any | null> => this.get(sql, params),
            run: async (...params: any[]): Promise<{ changes: number; lastInsertRowid: number | null }> => this.run(sql, params),
        };
    }

    /**
     * Automated Point-in-Time Snapshot via SQLite VACUUM INTO
     */
    async backup(backupPath: string): Promise<string> {
        const res = await this.#sendQuery<{ backupPath: string }>("backup", { backupPath });
        return res.backupPath;
    }

    get isReady(): boolean {
        return this.#isReady;
    }

    #startWatchdog(): void {
        this.#stopWatchdog();
        this.#lastPongTime = Date.now();

        this.#watchdogInterval = setInterval(() => {
            if (!this.#worker || !this.#isReady) return;

            // Ping worker
            this.#worker.postMessage({ type: "ping" });

            // Verify worker alive
            const silenceMs = Date.now() - this.#lastPongTime;
            if (silenceMs > WATCHDOG_TIMEOUT_MS) {
                logger.error(`[DatabaseWorkerBridge] 🏥 WATCHDOG: Database worker deadlock detected! Terminating worker...`);
                this.#isReady = false;
                this.#stopWatchdog();

                // Terminate frozen thread
                this.#worker.terminate().catch(() => {});
                this.#worker = null;

                this.#rejectAllPending(new Error("Database worker thread deadlocked"));
                this.#attemptRecovery();
            }
        }, WATCHDOG_PING_MS);
        this.#watchdogInterval.unref();
    }

    #stopWatchdog(): void {
        if (this.#watchdogInterval) {
            clearInterval(this.#watchdogInterval);
            this.#watchdogInterval = null;
        }
    }

    #attemptRecovery(): void {
        if (this.#crashCount >= MAX_RECOVERY_ATTEMPTS) {
            logger.error(`[DatabaseWorkerBridge] 🛑 Max database recovery attempts (${MAX_RECOVERY_ATTEMPTS}) reached. Database offline.`);
            return;
        }

        this.#crashCount++;
        const delay = 1000 * Math.pow(2, this.#crashCount);
        logger.warn(`[DatabaseWorkerBridge] 🔄 Recovering database worker (attempt ${this.#crashCount}) in ${delay}ms...`);

        if (this.#recoveryTimer) clearTimeout(this.#recoveryTimer);
        this.#recoveryTimer = setTimeout(async () => {
            this.#recoveryTimer = null;
            try {
                if (this.#worker) {
                    try { await this.#worker.terminate(); } catch {}
                    this.#worker = null;
                }
                await this.initialize();
                this.#crashCount = 0; // Reset
                logger.info("[DatabaseWorkerBridge] ✅ Database worker recovered successfully.");
            } catch (err: unknown) {
                const msg = err instanceof Error ? err.message : String(err);
                logger.warn(`[DatabaseWorkerBridge] Recovery failed: ${msg}`);
            }
        }, delay);
        this.#recoveryTimer.unref();
    }

    /**
     * Terminate database worker and cleanup
     */
    async dispose(): Promise<void> {
        this.#stopWatchdog();
        if (this.#recoveryTimer) {
            clearTimeout(this.#recoveryTimer);
            this.#recoveryTimer = null;
        }
        this.#rejectAllPending(new Error("Database worker bridge disposed"));

        if (this.#worker) {
            this.#worker.postMessage({ type: "close" });
            await this.#worker.terminate();
            this.#worker = null;
        }
        this.#isReady = false;
        this.#crashCount = 0;
    }
}
