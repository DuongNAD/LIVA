/**
 * EncryptionEngine.test.ts — Sprint 4 Task 4.1 Tests
 * Tests AES-256-GCM encryption/decryption + Atomic Write file I/O
 */
import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("fs/promises", async () => {
    const memfs = await import("memfs");
    return memfs.fs.promises;
});
vi.mock("fs", async () => {
    const memfs = await import("memfs");
    return memfs.fs;
});
import { vol } from "memfs";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { EncryptionEngine } from "../../src/memory/EncryptionEngine";

describe("EncryptionEngine", () => {
    beforeEach(() => {
        vol.reset();
        vi.clearAllMocks();
    });

    describe("encrypt / decrypt", () => {
        it("should encrypt and decrypt a string correctly (round-trip)", () => {
            const plain = "Hello, LIVA! This is a secret.";
            const encrypted = EncryptionEngine.encrypt(plain);

            // Encrypted format: hex_iv:hex_authTag:hex_ciphertext
            const parts = encrypted.split(":");
            expect(parts.length).toBe(3);
            expect(encrypted).not.toBe(plain);

            const decrypted = EncryptionEngine.decrypt(encrypted);
            expect(decrypted).toBe(plain);
        });

        it("should handle empty string encryption", () => {
            const encrypted = EncryptionEngine.encrypt("");
            const decrypted = EncryptionEngine.decrypt(encrypted);
            expect(decrypted).toBe("");
        });

        it("should handle Unicode text", () => {
            const plain = "Chào bạn! 🚀 こんにちは 🎉";
            const encrypted = EncryptionEngine.encrypt(plain);
            const decrypted = EncryptionEngine.decrypt(encrypted);
            expect(decrypted).toBe(plain);
        });

        it("should return raw text for non-3-part format (backward compat)", () => {
            const rawText = "This is plain markdown content";
            expect(EncryptionEngine.decrypt(rawText)).toBe(rawText);
        });

        it("should return raw text when decryption fails (corrupted ciphertext)", () => {
            const corrupted = "00000000000000000000000000000000:00000000000000000000000000000000:deadbeef";
            const result = EncryptionEngine.decrypt(corrupted);
            // Should return corrupted text as-is (backward compat)
            expect(result).toBe(corrupted);
        });

        it("should produce different ciphertexts for same plaintext (random IV)", () => {
            const plain = "Same input, different output";
            const enc1 = EncryptionEngine.encrypt(plain);
            const enc2 = EncryptionEngine.encrypt(plain);
            expect(enc1).not.toBe(enc2); // Random IV means different ciphertext
        });
    });

    describe("writeFileEncrypted (Atomic Write)", () => {
        it("should write encrypted content using .tmp + rename (Atomic Write)", async () => {
            const fsPromises = await import("fs/promises");
            const filePath = "/data/test.enc";
            
            const writeSpy = vi.spyOn(fsPromises, "writeFile").mockResolvedValue(undefined);
            const renameSpy = vi.spyOn(fsPromises, "rename").mockResolvedValue(undefined);

            await EncryptionEngine.writeFileEncrypted(filePath, "secret data");

            // Should write to .tmp first
            expect(writeSpy).toHaveBeenCalledWith(`${filePath}.tmp`, expect.any(String), "utf-8");
            // Then rename atomically
            expect(renameSpy).toHaveBeenCalledWith(`${filePath}.tmp`, filePath);
        });
    });

    describe("readFileDecrypted", () => {
        it("should read and decrypt an encrypted file", async () => {
            const fsPromises = await import("fs/promises");
            const original = "Hello, decrypted world!";
            const encrypted = EncryptionEngine.encrypt(original);

            vi.spyOn(fsPromises, "readFile").mockResolvedValue(encrypted);

            const result = await EncryptionEngine.readFileDecrypted("/data/test.enc");
            expect(result).toBe(original);
        });

        it("should return empty string on file read error (ENOENT)", async () => {
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, "readFile").mockRejectedValue(new Error("ENOENT"));

            const result = await EncryptionEngine.readFileDecrypted("/nonexistent.enc");
            expect(result).toBe("");
        });

        it("should return raw text for non-encrypted file content", async () => {
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, "readFile").mockResolvedValue("# Plain Markdown Content");

            const result = await EncryptionEngine.readFileDecrypted("/data/plain.md");
            expect(result).toBe("# Plain Markdown Content");
        });
    });

    describe("initFileEncrypted", () => {
        it("should create encrypted file if it doesn't exist", async () => {
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, "access").mockRejectedValue(new Error("ENOENT"));
            const writeSpy = vi.spyOn(fsPromises, "writeFile").mockResolvedValue(undefined);
            vi.spyOn(fsPromises, "rename").mockResolvedValue(undefined);

            await EncryptionEngine.initFileEncrypted("/data/init.enc", "Default content");
            expect(writeSpy).toHaveBeenCalled();
        });

        it("should NOT overwrite existing file", async () => {
            const fsPromises = await import("fs/promises");
            vi.spyOn(fsPromises, "access").mockResolvedValue(undefined);
            const writeSpy = vi.spyOn(fsPromises, "writeFile");

            await EncryptionEngine.initFileEncrypted("/data/existing.enc", "Default content");
            expect(writeSpy).not.toHaveBeenCalled();
        });
    });
});
