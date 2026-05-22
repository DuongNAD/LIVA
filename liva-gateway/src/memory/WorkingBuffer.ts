import * as fs from 'node:fs/promises';
import * as path from "node:path";
import { logger } from "../utils/logger";

export class WorkingBuffer {
    // Giả định: 1 token ~ 4 chars. Max context 64k tokens = 256,000 chars
    private readonly MAX_CHARS = 256000;
    private readonly BUFFER_FILE: string;
    private readonly SNAPSHOT_FILE: string;

    constructor(agentId: string) {
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

    /**
     * Tính toán ngân sách Token và cảnh báo nếu sắp tràn ngữ cảnh
     */
    public async checkBudget(currentContextText: string): Promise<string> {
        const charCount = currentContextText.length;
        const usedRatio = charCount / this.MAX_CHARS;
        
        const budgetStr = `[context-budget: ${(usedRatio * 100).toFixed(1)}% used, ${Math.max(0, Math.floor((this.MAX_CHARS - charCount) / 4))} tokens remaining]`;

        if (usedRatio >= 0.78) {
            logger.warn(`[WorkingBuffer] Ngân sách Token nguy cấp (${(usedRatio * 100).toFixed(1)}%). Tạo snapshot phục hồi (Compaction Recovery)...`);
            await this.createSnapshot(currentContextText);
        } else if (usedRatio >= 0.60) {
            logger.info(`[WorkingBuffer] Cảnh báo dung lượng ngữ cảnh: (${(usedRatio * 100).toFixed(1)}%).`);
            await this.writeDraftBuffer(currentContextText);
        }

        return budgetStr;
    }

    private async writeDraftBuffer(context: string) {
        // Ghi nháp 5000 ký tự cuối vào buffer để chuẩn bị nén
        const draft = `# DANGER ZONE DRAFT\nTime: ${new Date().toISOString()}\n\n${context.slice(-5000)}`;
        await fs.writeFile(this.BUFFER_FILE, draft, "utf-8");
    }

    private async createSnapshot(context: string) {
        // Tạo bản snapshot toàn diện để không bị mất trí nhớ khi nén
        const snapshot = `# COMPACTION SNAPSHOT\nTime: ${new Date().toISOString()}\n\n[TRẠNG THÁI CUỐI TRƯỚC KHI NÉN NGỮ CẢNH]\n${context.slice(-10000)}`;
        await fs.writeFile(this.SNAPSHOT_FILE, snapshot, "utf-8");
    }

    /**
     * Clear all buffer state without creating a new instance.
     * Called during memory reset to prevent readonly reassignment.
     */
    public async clear(): Promise<void> {
        try {
            await fs.writeFile(this.BUFFER_FILE, "", "utf-8");
            await fs.writeFile(this.SNAPSHOT_FILE, "", "utf-8");
        } catch { /* Files may not exist yet — safe to ignore */ }
    }
}
