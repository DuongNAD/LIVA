import { safeRename } from "../utils/FileUtils.js";
import * as fs from "node:fs/promises";
import * as path from "node:path";
import { createHash } from "node:crypto";
import { logger } from "../utils/logger.js";
import LRUCache from "lru-cache";
import { z } from "zod";
import { generateULID } from "../utils/ULID.js";
import { AsyncChunker } from "../utils/AsyncChunker.js";

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
            const errMsg = err instanceof Error ? err.message : String(err);
            logger.warn(`[MemoryDreaming] Failed to read index file: ${errMsg}. Returning empty index.`);
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
                    return null;
                }
            } catch {
                // Log file doesn't exist
                return null;
            }

            const rawLogs = await fs.readFile(this.logFilePath, "utf-8");
            const lines = rawLogs.split("\n").filter((l) => l.trim() !== "");
            if (lines.length === 0) {
                return null;
            }

            // Load existing index nodes
            const existingNodes = await this.loadIndex();
            const nodeMap = new Map<string, MemoryNode>();
            for (const node of existingNodes) {
                nodeMap.set(node.hash, { ...node });
            }

            // Process and deduplicate logs asynchronously using AsyncChunker
            await AsyncChunker.processNonBlocking(lines, (line) => {
                try {
                    const log = JSON.parse(line);
                    const content = log.content;
                    const timestamp = log.timestamp || Date.now();
                    if (!content || !content.trim()) {
                        return;
                    }

                    const hash = createHash("sha256").update(content.trim()).digest("hex");
                    const existing = nodeMap.get(hash);
                    if (existing) {
                        existing.weight += 1;
                        existing.lastAccessed = Math.max(existing.lastAccessed, timestamp);
                    } else {
                        nodeMap.set(hash, {
                            id: generateULID(),
                            hash,
                            content: content.trim(),
                            weight: 1,
                            lastAccessed: timestamp,
                        });
                    }
                } catch (err: unknown) {
                    const errMsg = err instanceof Error ? err.message : String(err);
                    logger.warn(`[MemoryDreaming] Failed to parse log line: ${line}. Error: ${errMsg}`);
                }
            }, 100);

            // Construct proposed index and sort by weight descending (importance ranking)
            const proposedIndex = Array.from(nodeMap.values()).sort((a, b) => b.weight - a.weight);

            // Calculate size and compression metrics
            const originalSizeBytes = Buffer.byteLength(rawLogs, "utf-8");
            const existingIndexSize = Buffer.byteLength(JSON.stringify(existingNodes), "utf-8");
            const totalInputSize = originalSizeBytes + existingIndexSize;
            const serializedIndex = JSON.stringify(proposedIndex);
            const optimizedSizeBytes = Buffer.byteLength(serializedIndex, "utf-8");
            const compressionRatio = totalInputSize > 0 ? (totalInputSize - optimizedSizeBytes) / totalInputSize : 0;

            const diffPayload = this.generateDiffPayload(existingNodes, proposedIndex);

            return {
                originalSizeBytes,
                optimizedSizeBytes,
                diffPayload,
                proposedIndex,
                compressionRatio,
            };
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
            const code = err && typeof err === 'object' && 'code' in err ? (err as { code: string }).code : undefined;
            if (code !== "ENOENT") {
                const errMsg = err instanceof Error ? err.message : String(err);
                logger.warn(`[MemoryDreaming] Failed to purge session logs: ${errMsg}`);
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
