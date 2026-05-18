import { safeRename } from '../utils/FileUtils';
import * as fs from 'node:fs/promises';
import * as fsSync from "node:fs";
import { evoLogger } from "./EvolutionLogger";
import { EvolutionContext } from "./types";

export class RollbackManager {
    static async backup(ctx: EvolutionContext) {
        if (!ctx.hypothesis?.targetFilePath) return;
        const targetPath = ctx.hypothesis.targetFilePath;
        const backupPath = `${targetPath}.bak`;
        try {
            if (fsSync.existsSync(targetPath)) {
                await fs.copyFile(targetPath, backupPath);
                evoLogger.info(`[RollbackManager] Đã tạo file backup cho ${targetPath}`);
            }
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            evoLogger.error({ err: e }, `[RollbackManager] Lỗi khi tạo backup cho ${targetPath}`);
            throw new Error(`RollbackManager Backup Failed: ${errMsg}`);
        }
    }

    static async restore(ctx: EvolutionContext) {
        if (!ctx.hypothesis?.targetFilePath) return;
        const targetPath = ctx.hypothesis.targetFilePath;
        const backupPath = `${targetPath}.bak`;
        try {
            if (fsSync.existsSync(backupPath)) {
                // Sử dụng rename để đảm bảo Atomic operation theo chuẩn AI_CONTEXT.md
                await safeRename(backupPath, targetPath);
                evoLogger.info(`[RollbackManager] Đã khôi phục file ${targetPath} từ backup.`);
            }
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            evoLogger.error({ err: e }, `[RollbackManager] Lỗi khi khôi phục backup cho ${targetPath}`);
        }
    }

    static async cleanup(ctx: EvolutionContext) {
        if (!ctx.hypothesis?.targetFilePath) return;
        const targetPath = ctx.hypothesis.targetFilePath;
        const backupPath = `${targetPath}.bak`;
        try {
            if (fsSync.existsSync(backupPath)) {
                await fs.unlink(backupPath);
                evoLogger.info(`[RollbackManager] Đã dọn dẹp file backup rác ${backupPath}.`);
            }
        } catch (e: unknown) {
        const errMsg = e instanceof Error ? e.message : String(e);
            evoLogger.error({ err: e }, `[RollbackManager] Lỗi khi xóa file backup ${backupPath}`);
        }
    }
}
