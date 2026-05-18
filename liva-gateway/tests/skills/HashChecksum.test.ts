import { describe, it, expect, vi, afterEach } from "vitest";
import * as fs from "node:fs";
import * as crypto from "node:crypto";
import path from "node:path";
import os from "node:os";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import * as HashChecksum from "../../src/skills/data/HashChecksum";

describe("HashChecksum Skill", () => {
    const testDir = path.join(os.tmpdir(), "hash_checksum_test");
    const testFile = path.join(testDir, "test.txt");
    const testContent = "Hello LIVA System! This is a test file for hashing.";

    // Pre-compute expected hash
    const expectedSha256 = crypto.createHash("sha256").update(testContent).digest("hex");
    const expectedMd5 = crypto.createHash("md5").update(testContent).digest("hex");

    afterEach(() => {
        // Cleanup
        try { fs.unlinkSync(testFile); } catch {}
        try { fs.rmdirSync(testDir); } catch {}
    });

    it("should have correct metadata", () => {
        expect(HashChecksum.metadata.name).toBe("hash_checksum");
        expect(HashChecksum.metadata.parameters.required).toContain("filePath");
    });

    it("should compute SHA256 hash correctly", async () => {
        fs.mkdirSync(testDir, { recursive: true });
        fs.writeFileSync(testFile, testContent);

        const result = await HashChecksum.execute({ filePath: testFile, algorithm: "sha256" });
        expect(result).toContain("HASH RESULT");
        expect(result).toContain(expectedSha256);
        expect(result).toContain("SHA256");
    });

    it("should compute MD5 hash correctly", async () => {
        fs.mkdirSync(testDir, { recursive: true });
        fs.writeFileSync(testFile, testContent);

        const result = await HashChecksum.execute({ filePath: testFile, algorithm: "md5" });
        expect(result).toContain(expectedMd5);
    });

    it("should default to SHA256 when no algorithm specified", async () => {
        fs.mkdirSync(testDir, { recursive: true });
        fs.writeFileSync(testFile, testContent);

        const result = await HashChecksum.execute({ filePath: testFile });
        expect(result).toContain("SHA256");
        expect(result).toContain(expectedSha256);
    });

    it("should verify matching hash successfully", async () => {
        fs.mkdirSync(testDir, { recursive: true });
        fs.writeFileSync(testFile, testContent);

        const result = await HashChecksum.execute({
            filePath: testFile,
            algorithm: "sha256",
            verify: expectedSha256
        });
        expect(result).toContain("✅ KHỚP");
    });

    it("should detect hash mismatch", async () => {
        fs.mkdirSync(testDir, { recursive: true });
        fs.writeFileSync(testFile, testContent);

        const result = await HashChecksum.execute({
            filePath: testFile,
            algorithm: "sha256",
            verify: "0000000000000000000000000000000000000000000000000000000000000000"
        });
        expect(result).toContain("❌ KHÔNG KHỚP");
    });

    it("should handle non-existent file gracefully", async () => {
        const result = await HashChecksum.execute({ filePath: "/nonexistent/file.txt" });
        expect(result).toContain("ERROR");
    });

    it("should reject missing filePath", async () => {
        const result = await HashChecksum.execute({} as any);
        expect(result).toContain("ERROR");
    });
});
