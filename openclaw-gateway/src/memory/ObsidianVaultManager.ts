import { safeRename } from '../utils/FileUtils';
import { promises as fsp } from "node:fs";
import path from "node:path";
import crypto from "node:crypto";
import { logger } from "../utils/logger";

export class ObsidianVaultManager {
    readonly #vaultRoot: string;
    readonly #activeLocks: Set<string> = new Set(); // Ngăn chặn xung đột Agent đa luồng

    constructor(vaultRootPath: string) {
        this.#vaultRoot = path.resolve(vaultRootPath);
    }

    /**
     * BẮT BUỘC: Xác thực Path Traversal Guard (Zero-Trust)
     */
    #getSafePath(relativePath: string): string {
        const targetPath = path.resolve(this.#vaultRoot, relativePath);
        if (!targetPath.startsWith(this.#vaultRoot)) {
            logger.error({ relativePath }, "SecurityGuard: Phát hiện hành vi Path Traversal");
            throw new Error("SECURITY_VIOLATION: Path Traversal Attempted");
        }
        return targetPath;
    }

    /**
     * BẮT BUỘC: Thuật toán Ghi đè An toàn (Optimistic Concurrency + Atomic Write)
     */
    public async safeAppendInsights(relativePath: string, insightBlock: string, expectedMtimeMs: number): Promise<void> {
        const targetPath = this.#getSafePath(relativePath);
        
        // 1. Process-Level Mutex Lock
        if (this.#activeLocks.has(targetPath)) {
            throw new Error("LOCKED: File is being modified by another LIVA process");
        }
        
        this.#activeLocks.add(targetPath);
        
        try {
            // 2. Kiểm tra tồn tại và Mtime (Chống xóa mất dữ liệu do User vừa gõ tay)
            let existingContent = "";
            try {
                const stat = await fsp.stat(targetPath);
                if (stat.mtimeMs > expectedMtimeMs) {
                    throw new Error("CONCURRENCY_ERROR: File modified by user during AI processing");
                }
                existingContent = await fsp.readFile(targetPath, "utf-8");
            } catch (err: unknown) {
            const errMsg = err instanceof Error ? err.message : String(err);
                if (err instanceof Error && (err as NodeJS.ErrnoException).code !== 'ENOENT' && !errMsg.startsWith("CONCURRENCY_ERROR")) throw err;
                if (errMsg.startsWith("CONCURRENCY_ERROR")) throw err;
            }

            // 3. Xử lý Append-Only: Chỉ chèn vào cuối file
            const appendSeparator = existingContent.length > 0 && !existingContent.endsWith('\n') ? '\n\n' : '\n';
            const newContent = `${existingContent}${appendSeparator}> [!ai] LIVA Graph Weaver:\n> ${insightBlock.replace(/\n/g, '\n> ')}\n`;

            // 4. Atomic Write Pattern (.tmp -> rename)
            const tmpPath = `${targetPath}.${crypto.randomUUID()}.tmp`;
            
            await fsp.mkdir(path.dirname(targetPath), { recursive: true });
            await fsp.writeFile(tmpPath, newContent, "utf-8");
            await safeRename(tmpPath, targetPath); // Hoàn thành ghi đè nguyên tử an toàn tuyệt đối

            logger.info({ file: relativePath }, "ObsidianVault: Atomic Write Success");
        } finally {
            this.#activeLocks.delete(targetPath); // LUÔN DỌN DẸP Ở FINALLY
        }
    }

    /**
     * Đọc nội dung file an toàn
     */
    public async readNote(relativePath: string): Promise<{ content: string; mtimeMs: number }> {
        const targetPath = this.#getSafePath(relativePath);
        if (this.#activeLocks.has(targetPath)) {
            throw new Error("LOCKED: File is currently being written by another process. Please try again later.");
        }
        
        try {
            const stat = await fsp.stat(targetPath);
            const content = await fsp.readFile(targetPath, "utf-8");
            return { content, mtimeMs: stat.mtimeMs };
        } catch (err: unknown) {
        const errMsg = err instanceof Error ? err.message : String(err);
            if (err instanceof Error && (err as NodeJS.ErrnoException).code === 'ENOENT') {
                throw new Error("FILE_NOT_FOUND: The requested note does not exist.");
            }
            throw err;
        }
    }

    /**
     * Ghi đè hoặc tạo mới an toàn
     */
    public async createOrOverwriteNote(relativePath: string, content: string): Promise<void> {
        const targetPath = this.#getSafePath(relativePath);
        
        // 1. Process-Level Mutex Lock
        if (this.#activeLocks.has(targetPath)) {
            throw new Error("LOCKED: File is being modified by another LIVA process");
        }
        
        this.#activeLocks.add(targetPath);
        
        try {
            // 2. Atomic Write Pattern (.tmp -> rename)
            const tmpPath = `${targetPath}.${crypto.randomUUID()}.tmp`;
            
            await fsp.mkdir(path.dirname(targetPath), { recursive: true });
            await fsp.writeFile(tmpPath, content, "utf-8");
            await safeRename(tmpPath, targetPath);

            logger.info({ file: relativePath }, "ObsidianVault: Atomic Write/Create Success");
        } finally {
            this.#activeLocks.delete(targetPath);
        }
    }
}
