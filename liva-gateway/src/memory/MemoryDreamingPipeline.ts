import { safeRename } from "../utils/FileUtils.js";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { logger } from "../utils/logger.js";
import LRUCache from "lru-cache";
import { z } from "zod";
import { Worker } from "node:worker_threads";
import { fileURLToPath, pathToFileURL } from "node:url";

const _dirname = import.meta.dirname ?? path.dirname(fileURLToPath(import.meta.url));


export interface MemoryNode {
    id: string;
    hash: string;
    content: string;
    weight: number;
    lastAccessed: number;
}

export interface DreamingResult {
    originalSizeBytes: number;
    optimizedSizeBytes: number;
    diffPayload: string;
    proposedIndex: MemoryNode[];
    compressionRatio: number;
}

export const MemoryNodeSchema = z.object({
    id: z.string(),
    hash: z.string(),
    content: z.string(),
    weight: z.number(),
    lastAccessed: z.number(),
});

export const MemoryIndexSchema = z.array(MemoryNodeSchema);

export class MemoryDreamingPipeline {
    private readonly storeDir: string;
    private readonly logFilePath: string;
    private readonly indexFilePath: string;
    #isDreaming = false;

    private readonly indexCache = new LRUCache<string, MemoryNode[]>({
        max: 5,
        ttl: 10 * 60 * 1000 // 10 minutes TTL
    });

    constructor(agentId: string, memoryStoreDir?: string) {
        this.storeDir = memoryStoreDir ?? path.join(process.cwd(), "data", "agents", agentId, "memory_store");
        this.logFilePath = path.join(this.storeDir, "session_logs.jsonl");
        this.indexFilePath = path.join(this.storeDir, "index_summary.json");

        // Prevent directory traversal attacks
        const resolvedStoreDir = path.resolve(this.storeDir);
        const resolvedCwd = path.resolve(process.cwd());
        if (!resolvedStoreDir.startsWith(resolvedCwd)) {
            throw new Error(`Directory traversal detected: Memory store directory ${resolvedStoreDir} must be within workspace ${resolvedCwd}`);
        }
    }

    /**
     * Bootstrap the store directory and initialize the index file if it doesn't exist.
     */
    public async bootstrap(): Promise<void> {
        await fs.mkdir(this.storeDir, { recursive: true });
        try {
            await fs.access(this.indexFilePath);
        } catch {
            // Index file does not exist, initialize it atomically with empty array
            const tmpFile = `${this.indexFilePath}.tmp`;
            await fs.writeFile(tmpFile, "[]", "utf-8");
            await safeRename(tmpFile, this.indexFilePath);
        }
    }

    /**
     * Append a message log entry to the read-write session log zone.
     */
    public async appendSessionLog(role: "user" | "assistant" | "system", content: string): Promise<void> {
        if (!content || !content.trim()) {
            return;
        }

        await this.bootstrap();

        const logEntry = JSON.stringify({
            role,
            content: content.trim(),
            timestamp: Date.now(),
        });

        await fs.appendFile(this.logFilePath, logEntry + "\n", "utf-8");
    }

    /**
     * Read and validate the current memory index from disk (uses bounded cache).
     */
    public async loadIndex(): Promise<MemoryNode[]> {
        const cached = this.indexCache.get("index");
        if (cached) {
            return cached;
        }

        await this.bootstrap();

        try {
            const indexData = await fs.readFile(this.indexFilePath, "utf-8");
            const parsed = JSON.parse(indexData);
            const validation = MemoryIndexSchema.safeParse(parsed);
            if (validation.success) {
                this.indexCache.set("index", validation.data);
                return validation.data;
            } else {
                logger.warn(`[MemoryDreaming] Invalid index schema on disk, falling back to empty: ${validation.error.message}`);
                return [];
            }
        } catch (err: unknown) {
            const message = err instanceof Error ? err.message : String(err);
            logger.warn(`[MemoryDreaming] Failed to read index file: ${message}. Returning empty index.`);
            return [];
        }
    }

    /**
     * Check if dreaming sequence is in progress.
     */
    public get isDreaming(): boolean {
        return this.#isDreaming;
    }

    /**
     * Execute the dreaming sequence: scans logs, dedups with SHA-256,
     * updates weight, and ranks by weight to create an optimized proposed index.
     */
    public async executeDreamingSequence(): Promise<DreamingResult | null> {
        if (this.#isDreaming) {
            logger.warn("[MemoryDreaming] Dreaming sequence already in progress. Skipping.");
            throw new Error("Dreaming sequence already in progress");
        }

        this.#isDreaming = true;

        try {
            await this.bootstrap();

            let logStat: Awaited<ReturnType<typeof fs.stat>>;
            try {
                logStat = await fs.stat(this.logFilePath);
                if (logStat.size === 0) {
                    this.#isDreaming = false;
                    return null;
                }
            } catch {
                // Log file doesn't exist
                this.#isDreaming = false;
                return null;
            }

            const rawLogs = await fs.readFile(this.logFilePath, "utf-8");
            const lines = rawLogs.split("\n").filter((l) => l.trim() !== "");
            if (lines.length === 0) {
                this.#isDreaming = false;
                return null;
            }

            // Load existing index nodes
            const existingNodes = await this.loadIndex();

            let settled = false;
            return await new Promise<DreamingResult | null>((resolve, reject) => {
                const workerPath = path.join(_dirname, "..", "workers", "MemoryDreamingWorker.ts");
                let worker: Worker;

                if (process.env.NODE_ENV === "production") {
                    const prodWorkerPath = workerPath.replace(/\.ts$/, ".js");
                    worker = new Worker(prodWorkerPath);
                } else {
                    const workerUrl = pathToFileURL(workerPath).href;
                    worker = new Worker(
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

                const cleanup = () => {
                    settled = true;
                    worker.terminate().catch((err) => {
                        logger.error(`[MemoryDreaming] Failed to terminate worker: ${err.message}`);
                    });
                };

                worker.on("message", (msg: { ok: boolean; proposedIndex?: MemoryNode[]; diffPayload?: string; error?: string }) => {
                    if (settled) return;
                    if (msg.ok) {
                        try {
                            const proposedIndex = msg.proposedIndex || [];
                            const diffPayload = msg.diffPayload || "";

                            // Calculate size and compression metrics
                            const originalSizeBytes = Buffer.byteLength(rawLogs, "utf-8");
                            const existingIndexSize = Buffer.byteLength(JSON.stringify(existingNodes), "utf-8");
                            const totalInputSize = originalSizeBytes + existingIndexSize;
                            const serializedIndex = JSON.stringify(proposedIndex);
                            const optimizedSizeBytes = Buffer.byteLength(serializedIndex, "utf-8");
                            const compressionRatio = totalInputSize > 0 ? (totalInputSize - optimizedSizeBytes) / totalInputSize : 0;

                            cleanup();
                            resolve({
                                originalSizeBytes,
                                optimizedSizeBytes,
                                diffPayload,
                                proposedIndex,
                                compressionRatio,
                            });
                        } catch (err) {
                            cleanup();
                            reject(err);
                        }
                    } else {
                        cleanup();
                        reject(new Error(msg.error || "Worker encountered an error"));
                    }
                });

                worker.on("error", (err) => {
                    if (settled) return;
                    cleanup();
                    reject(err);
                });

                worker.on("exit", (code) => {
                    if (settled) return;
                    if (code !== 0) {
                        cleanup();
                        reject(new Error(`Worker stopped with exit code ${code}`));
                    }
                });

                worker.postMessage({ rawLogs, existingNodes });
            });
        } finally {
            this.#isDreaming = false;
        }
    }

    /**
     * Generate a Git-Diff styled report of proposed vs old memory nodes.
     */
    public generateDiffPayload(oldIndex: MemoryNode[], newIndex: MemoryNode[]): string {
        const oldMap = new Map(oldIndex.map((n) => [n.hash, n]));
        const newMap = new Map(newIndex.map((n) => [n.hash, n]));

        const lines: string[] = [];

        // Added nodes
        for (const [hash, node] of newMap.entries()) {
            if (!oldMap.has(hash)) {
                lines.push(`+ [ADDED] ID: ${node.id} (Weight: ${node.weight})`);
                lines.push(`+ ${node.content}`);
                lines.push("");
            }
        }

        // Modified nodes (weight changes)
        for (const [hash, node] of newMap.entries()) {
            const oldNode = oldMap.get(hash);
            if (oldNode && oldNode.weight !== node.weight) {
                lines.push(`~ [MODIFIED] ID: ${node.id}`);
                lines.push(`- Weight: ${oldNode.weight}`);
                lines.push(`+ Weight: ${node.weight}`);
                lines.push(`  Content: ${node.content}`);
                lines.push("");
            }
        }

        // Removed nodes
        for (const [hash, node] of oldMap.entries()) {
            if (!newMap.has(hash)) {
                lines.push(`- [REMOVED] ID: ${node.id} (Weight: ${node.weight})`);
                lines.push(`- ${node.content}`);
                lines.push("");
            }
        }

        return lines.join("\n").trim();
    }

    /**
     * Commit the approved proposed index atomically and purge raw logs.
     */
    public async commitApprovedMemory(proposedIndex: MemoryNode[]): Promise<void> {
        await this.bootstrap();

        // Validate structure first
        MemoryIndexSchema.parse(proposedIndex);

        // Atomic write index file
        const tmpFile = `${this.indexFilePath}.tmp`;
        await fs.writeFile(tmpFile, JSON.stringify(proposedIndex, null, 2), "utf-8");
        await safeRename(tmpFile, this.indexFilePath);

        // Update cache
        this.indexCache.set("index", proposedIndex);

        // Purge raw session logs (safe: no-op if file doesn't exist yet)
        try {
            await fs.writeFile(this.logFilePath, "", "utf-8");
        } catch (err: unknown) {
            const code = err && typeof err === "object" && "code" in err ? (err as { code: string }).code : undefined;
            const message = err instanceof Error ? err.message : String(err);
            if (code !== "ENOENT") {
                logger.warn(`[MemoryDreaming] Failed to purge session logs: ${message}`);
            }
        }
    }

    /**
     * Clean up timers or caches. Called during shutdown.
     */
    public dispose(): void {
        this.indexCache.clear();
        logger.info("[MemoryDreaming] Pipeline resources cleared.");
    }
}
