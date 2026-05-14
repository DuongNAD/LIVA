import { promises as fsp } from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import LRUCache from "lru-cache";
import { logger } from "../utils/logger";

export class BiDirectionalSyncWatcher {
    readonly #vaultRoot: string;
    // 🔒 [Audit C-6] Bounded cache to prevent OOM on large vaults
    readonly #fileHashes: LRUCache<string, string> = new LRUCache({ max: 10000, ttl: 1000 * 60 * 60 });
    readonly #debounceTimers: Map<string, NodeJS.Timeout> = new Map();
    readonly #DEBOUNCE_MS = 2000;
    
    #watcherController: AbortController | null = null;

    constructor(vaultRootPath: string) {
        this.#vaultRoot = path.resolve(vaultRootPath);
    }

    public async startWatching(): Promise<void> {
        this.#watcherController = new AbortController();
        const { signal } = this.#watcherController;

        try {
            const watcher = fsp.watch(this.#vaultRoot, { recursive: true, signal });
            
            for await (const event of watcher) {
                if (!event.filename || !event.filename.endsWith(".md")) continue;
                
                const filePath = path.join(this.#vaultRoot, event.filename);
                this.#handleFileChange(filePath);
            }
        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            if (err instanceof Error && err.name === 'AbortError') {
                logger.info("BiDirectionalSyncWatcher stopped.");
            } else {
                logger.error({ err: errMsg }, "BiDirectionalSyncWatcher crashed");
            }
        }
    }

    public stopWatching(): void {
        if (this.#watcherController) {
            this.#watcherController.abort();
            this.#watcherController = null;
        }

        // Cleanup timers
        for (const timer of this.#debounceTimers.values()) {
            clearTimeout(timer);
        }
        this.#debounceTimers.clear();
    }

    #handleFileChange(filePath: string): void {
        // Debounce logic
        if (this.#debounceTimers.has(filePath)) {
            clearTimeout(this.#debounceTimers.get(filePath));
        }

        const timer = setTimeout(() => {
            this.#processFile(filePath).catch(e => logger.error({ file: filePath, err: e.message }, "ProcessFile Error"));
            this.#debounceTimers.delete(filePath);
        }, this.#DEBOUNCE_MS);

        this.#debounceTimers.set(filePath, timer);
    }

    async #processFile(filePath: string): Promise<void> {
        try {
            const content = await fsp.readFile(filePath, "utf-8");
            const hash = crypto.createHash("sha256").update(content).digest("hex");

            const oldHash = this.#fileHashes.get(filePath);
            if (oldHash === hash) {
                // Nội dung không đổi, ignore sự kiện (Chống rác Event Loop)
                return;
            }

            this.#fileHashes.set(filePath, hash);
            logger.info({ file: filePath }, "SyncWatcher: File modified. Triggering Re-embed to StructuredMemory");
            
            // Re-embed to StructuredMemory (Boilerplate call)
            await this.#reEmbedToVectorMemory(filePath, content);
        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            if (err instanceof Error && (err as NodeJS.ErrnoException).code === 'ENOENT') {
                // File deleted
                this.#fileHashes.delete(filePath);
                logger.info({ file: filePath }, "SyncWatcher: File deleted. Removing from StructuredMemory");
            } else {
                throw err;
            }
        }
    }

    async #reEmbedToVectorMemory(filePath: string, content: string): Promise<void> {
        // Thực tế sẽ gọi StructuredMemory để nhúng Vector
        // Ví dụ: await sm.upsertVector({ ... });
    }
}
