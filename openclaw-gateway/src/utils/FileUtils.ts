import { promises as fsp } from "node:fs";
import { logger } from "./logger.js";

/**
 * Windows File System Retry Guard
 * On Windows, renaming files can sometimes throw EPERM or EBUSY because Windows Defender
 * or another background process temporarily holds a lock on the file.
 * This wrapper retries the rename operation with exponential backoff.
 */
export async function safeRename(oldPath: string, newPath: string, maxRetries = 3, baseDelay = 50): Promise<void> {
    let attempt = 0;
    while (attempt < maxRetries) {
        try {
            await fsp.rename(oldPath, newPath);
            return;
        } catch (error: any) {
            attempt++;
            if (attempt >= maxRetries) {
                logger.error(`[safeRename] Failed to rename ${oldPath} to ${newPath} after ${maxRetries} attempts: ${error.message}`);
                throw error;
            }
            const isLockError = error.code === 'EPERM' || error.code === 'EBUSY' || error.code === 'EACCES';
            if (!isLockError) {
                throw error;
            }
            const delay = baseDelay * Math.pow(2, attempt - 1);
            await new Promise(resolve => setTimeout(resolve, delay));
        }
    }
}
