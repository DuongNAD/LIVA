import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "../utils/logger";
import { TokenCompressionService } from "./TokenCompressionService";
import { safeRename } from "../utils/FileUtils";

export class WorkingBuffer {
    // [v28] Dynamic context limit — synced from ConfigManager.contextWindowTokens
    // 1 token ≈ 4 chars. Reserves 30% for response tokens.
    private maxChars: number;
    private readonly BUFFER_FILE: string;
    private readonly SNAPSHOT_FILE: string;

    constructor(agentId: string, contextTokens: number = 8192) {
        this.maxChars = Math.floor(contextTokens * 0.7) * 4; // 70% for prompt, 30% for response
        const memDir = path.join(process.cwd(), "data", "agents", agentId, "memory");
        this.BUFFER_FILE = path.join(memDir, "working-buffer.md");
        this.SNAPSHOT_FILE = path.join(memDir, "working-snapshot.md");
        // Defer async dir-creation to microtask queue (outside constructor body)
        // Satisfies SonarQube S4738: async operations must not be called in constructors
        this._readyPromise = Promise.resolve().then(() => this.ensureDir(memDir)); // NOSONAR — intentional async init
    }

    /** Resolves when the storage directory is guaranteed to exist */
    public readonly _readyPromise: Promise<void>;

    private async ensureDir(dir: string) {
        try {
            await fs.mkdir(dir, { recursive: true });
        } catch (e: unknown) {
            logger.warn(`[WorkingBuffer] Lỗi tạo thư mục ${dir}: ${e instanceof Error ? e.message : String(e)}`);
        }
    }

    /** Update context limit at runtime (e.g., after ConfigManager initializes) */
    public updateContextLimit(tokens: number): void {
        this.maxChars = Math.floor(tokens * 0.7) * 4;
    }

    /**
     * Tính toán ngân sách Token và cảnh báo nếu sắp tràn ngữ cảnh
     */
    public async checkBudget(currentContextText: string): Promise<string> {
        await this._readyPromise; // [Audit M-16] Guard: ensure dir exists before any I/O
        const charCount = currentContextText.length;
        const usedRatio = charCount / this.maxChars;
        
        const budgetStr = `[context-budget: ${(usedRatio * 100).toFixed(1)}% used, ${Math.max(0, Math.floor((this.maxChars - charCount) / 4))} tokens remaining]`;

        if (usedRatio >= 0.78) {
            logger.warn(`[WorkingBuffer] Ngân sách Token nguy cấp (${(usedRatio * 100).toFixed(1)}%). Kích hoạt nén + snapshot phục hồi...`);
            // [Phase 1] Active compression at critical threshold
            try {
                const compressor = TokenCompressionService.getInstance();
                const compressed = await compressor.compress(currentContextText, 0.5);
                if (compressed.compressionRatio < 0.9) {
                    logger.info(`[WorkingBuffer/Compression] Critical threshold compression: ${compressed.originalTokens} → ${compressed.compressedTokens} tokens (${compressed.strategy})`);
                    // Save compressed snapshot for recovery
                    await this.createSnapshot(compressed.compressedText);
                } else {
                    await this.createSnapshot(currentContextText);
                }
            } catch {
                // Compression failed — fallback to raw snapshot
                await this.createSnapshot(currentContextText);
            }
        } else if (usedRatio >= 0.60) {
            logger.info(`[WorkingBuffer] Cảnh báo dung lượng ngữ cảnh: (${(usedRatio * 100).toFixed(1)}%).`);
            await this.writeDraftBuffer(currentContextText);
        }

        return budgetStr;
    }

    // [Audit H-8] Atomic write: .tmp → safeRename to prevent crash corruption
    private async writeDraftBuffer(context: string) {
        const draft = `# DANGER ZONE DRAFT\nTime: ${new Date().toISOString()}\n\n${context.slice(-5000)}`;
        const tmpPath = `${this.BUFFER_FILE}.tmp`;
        await fs.writeFile(tmpPath, draft, "utf-8");
        await safeRename(tmpPath, this.BUFFER_FILE);
    }

    // [Audit H-8] Atomic write: .tmp → safeRename to prevent crash corruption
    private async createSnapshot(context: string) {
        const snapshot = `# COMPACTION SNAPSHOT\nTime: ${new Date().toISOString()}\n\n[TRẠNG THÁI CUỐI TRƯỚC KHI NÉN NGỮ CẢNH]\n${context.slice(-10000)}`;
        const tmpPath = `${this.SNAPSHOT_FILE}.tmp`;
        await fs.writeFile(tmpPath, snapshot, "utf-8");
        await safeRename(tmpPath, this.SNAPSHOT_FILE);
    }

    /**
     * Clear all buffer state without creating a new instance.
     * Called during memory reset to prevent readonly reassignment.
     */
    public async clear(): Promise<void> {
        await this._readyPromise; // [Audit M-16] Guard: ensure dir exists
        try {
            await fs.writeFile(this.BUFFER_FILE, "", "utf-8");
            await fs.writeFile(this.SNAPSHOT_FILE, "", "utf-8");
        } catch { /* Files may not exist yet — safe to ignore */ }
    }
}
