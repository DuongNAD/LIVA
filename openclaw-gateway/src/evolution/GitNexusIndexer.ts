import { exec } from "child_process";
import { promisify } from "util";
import { logger } from "../utils/logger";

const execAsync = promisify(exec);

export class GitNexusIndexer {
    private indexing: boolean = false;
    private debounceTimer: NodeJS.Timeout | null = null;

    public triggerIndex(delayMs: number = 5000) {
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
        }
        this.debounceTimer = setTimeout(() => {
            this.runIndex().catch(e => logger.error(`[GitNexusIndexer] Lỗi: ${e.message}`));
        }, delayMs);
    }

    private async runIndex() {
        if (this.indexing) return;
        this.indexing = true;
        try {
            logger.info("[GitNexusIndexer] Đang chạy phân tích GitNexus nền...");
            const { stdout, stderr } = await execAsync("npx gitnexus analyze --embeddings");
            logger.info(`[GitNexusIndexer] Hoàn tất: ${stdout}`);
            if (stderr) logger.warn(`[GitNexusIndexer] Cảnh báo: ${stderr}`);
        } catch (e: any) {
            logger.error(`[GitNexusIndexer] Thất bại: ${e.message}`);
        } finally {
            this.indexing = false;
        }
    }

    public dispose() {
        if (this.debounceTimer) {
            clearTimeout(this.debounceTimer);
            this.debounceTimer = null;
        }
    }
}
