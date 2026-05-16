import { safeRename } from '../utils/FileUtils';
/**
 * EncryptionEngine — Sprint 4 Task 4.1
 *
 * Centralized AES-256-GCM encryption/decryption engine.
 * Previously duplicated in MemoryManager.ts AND StructuredMemory.ts.
 *
 * Features:
 *   - AES-256-GCM with random IV + AuthTag
 *   - Atomic Write helper (.tmp → rename) for encrypted file I/O
 *   - Backward-compatible: non-3-part plaintext is returned as-is
 *   - Single ENCRYPTION_KEY derivation (no more duplicate constants)
 *
 * Usage:
 *   import { EncryptionEngine } from "./EncryptionEngine";
 *   const encrypted = EncryptionEngine.encrypt("secret");
 *   const plain     = EncryptionEngine.decrypt(encrypted);
 *   await EncryptionEngine.writeFileEncrypted(path, content);
 *   const content   = await EncryptionEngine.readFileDecrypted(path);
 */

import * as crypto from "node:crypto";
import * as fs from "node:fs/promises";
import { logger } from "../utils/logger";

// ===========================
// Constants (Single Source of Truth)
// ===========================

const IV_LENGTH = 16;
const AES_256_KEY_LENGTH = 32;

/**
 * Derive a 32-byte key from env LIVA_ENCRYPTION_KEY.
 * PRODUCTION MANDATORY: This variable MUST be set. No fallback.
 */
function loadEncryptionKey(): string {
    const envKey = process.env.LIVA_ENCRYPTION_KEY;
    if (!envKey) {
        const msg = "[EncryptionEngine] FATAL: LIVA_ENCRYPTION_KEY is not set. Cannot start without encryption key. Set a 32-byte key in your .env file.";
        logger.fatal(msg);
        throw new Error(msg);
    }
    if (Buffer.byteLength(envKey, "utf-8") !== AES_256_KEY_LENGTH) {
        const msg = `[EncryptionEngine] FATAL: LIVA_ENCRYPTION_KEY must be exactly ${AES_256_KEY_LENGTH} bytes for AES-256. Got ${Buffer.byteLength(envKey, "utf-8")} bytes.`;
        logger.fatal(msg);
        throw new Error(msg);
    }
    return envKey;
}

export class EncryptionEngine {

    static #cachedKey: Buffer | null = null;

    static get #key(): Buffer {
        if (!this.#cachedKey) {
            this.#cachedKey = Buffer.from(loadEncryptionKey());
        }
        return this.#cachedKey;
    }

    /**
     * Encrypt plaintext using AES-256-GCM.
     * Output format: `<hex IV>:<hex AuthTag>:<hex ciphertext>`
     */
    static encrypt(text: string): string {
        const iv = crypto.randomBytes(IV_LENGTH);
        const cipher = crypto.createCipheriv("aes-256-gcm", this.#key, iv);
        let encrypted = cipher.update(text, "utf8", "hex");
        encrypted += cipher.final("hex");
        const authTag = cipher.getAuthTag().toString("hex");
        return `${iv.toString("hex")}:${authTag}:${encrypted}`;
    }

    /**
     * Decrypt AES-256-GCM ciphertext.
     * Returns raw text on failure or if input isn't in `iv:tag:cipher` format
     * (backward-compatible with pre-v4 plaintext data).
     */
    static decrypt(text: string): string {
        try {
            const parts = text.split(":");
            if (parts.length !== 3) return text; // Plain-text fallback
            const iv = Buffer.from(parts[0], "hex");
            const authTag = Buffer.from(parts[1], "hex");
            const encryptedText = parts[2];
            const decipher = crypto.createDecipheriv("aes-256-gcm", this.#key, iv);
            decipher.setAuthTag(authTag);
            let decrypted = decipher.update(encryptedText, "hex", "utf8");
            decrypted += decipher.final("utf8");
            return decrypted;
        } catch {
            return text; // Backward compat: return raw if decryption fails
        }
    }

    /**
     * Write encrypted content to a file using Atomic Write pattern.
     * Writes to `.tmp` first, then renames atomically to prevent corruption.
     */
    static async writeFileEncrypted(filePath: string, content: string): Promise<void> {
        const encrypted = EncryptionEngine.encrypt(content);
        const tmpPath = `${filePath}.tmp`;
        await fs.writeFile(tmpPath, encrypted, "utf-8");
        await safeRename(tmpPath, filePath);
    }

    /**
     * Read and decrypt a file. Returns empty string on failure.
     */
    static async readFileDecrypted(filePath: string): Promise<string> {
        try {
            const raw = await fs.readFile(filePath, "utf-8");
            return EncryptionEngine.decrypt(raw);
        } catch {
            return "";
        }
    }

    /**
     * Write encrypted content to a file using Atomic Write, only if file doesn't exist yet.
     * Used for one-time initialization of memory template files.
     */
    static async initFileEncrypted(filePath: string, defaultContent: string): Promise<void> {
        try {
            await fs.access(filePath);
        } catch {
            // File doesn't exist — create it
            await EncryptionEngine.writeFileEncrypted(filePath, defaultContent);
            logger.info(`[EncryptionEngine] Initialized encrypted file: ${filePath}`);
        }
    }
}
