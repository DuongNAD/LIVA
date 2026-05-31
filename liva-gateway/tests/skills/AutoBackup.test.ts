import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { promises as fsp } from "node:fs";
import * as path from "node:path";
import * as os from "node:os";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

import { metadata, execute } from "../../src/skills/personal/AutoBackup";

describe("AutoBackup", () => {
    const testDir = path.join(os.tmpdir(), `liva_backup_test_${Date.now()}`);
    const testSrc = path.join(testDir, "source");
    const testDest = path.join(testDir, "backups");

    beforeEach(async () => {
        await fsp.mkdir(path.join(testSrc, "sub"), { recursive: true });
        await fsp.writeFile(path.join(testSrc, "file1.txt"), "hello world");
        await fsp.writeFile(path.join(testSrc, "sub", "file2.txt"), "nested file");
    });

    afterEach(async () => {
        await fsp.rm(testDir, { recursive: true, force: true });
    });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("auto_backup");
        expect(metadata.parameters.required).toContain("source_paths");
    });

    it("should reject empty source_paths", async () => {
        const result = await execute({ source_paths: [] });
        expect(result).toContain("Error");
    });

    it("should backup files successfully", async () => {
        const result = await execute({
            source_paths: [testSrc],
            destination: testDest,
            name: "test_backup",
        });
        expect(result).toContain("✅");
        expect(result).toContain("test_backup");
        expect(result).toContain("Files:");
    });

    it("should skip non-existent sources", async () => {
        const result = await execute({
            source_paths: ["/nonexistent/path/abc123"],
        });
        expect(result).toContain("Error");
        expect(result).toContain("None");
    });
});
