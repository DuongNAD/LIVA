import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";
import { ObsidianVaultManager } from "../../src/memory/ObsidianVaultManager";
import { promises as fsp } from "node:fs";
import path from "node:path";

vi.mock("node:fs", () => ({
    promises: {
        stat: vi.fn(),
        readFile: vi.fn(),
        mkdir: vi.fn(),
        writeFile: vi.fn(),
        rename: vi.fn()
    }
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(),
        error: vi.fn(),
        warn: vi.fn()
    }
}));

describe("ObsidianVaultManager", () => {
    const vaultRoot = path.resolve("/mock/vault");
    let manager: ObsidianVaultManager;

    beforeEach(() => {
        manager = new ObsidianVaultManager(vaultRoot);
        vi.clearAllMocks();
    });

    it("should block Path Traversal attempts", async () => {
        await expect(
            manager.safeAppendInsights("../../../etc/passwd", "hack", 0)
        ).rejects.toThrow("SECURITY_VIOLATION: Path Traversal Attempted");

        // Đảm bảo không có fs module nào được gọi
        expect(fsp.writeFile).not.toHaveBeenCalled();
        expect(fsp.rename).not.toHaveBeenCalled();
    });

    it("should throw CONCURRENCY_ERROR if file was modified by user", async () => {
        vi.mocked(fsp.stat).mockResolvedValue({ mtimeMs: 200 } as any);
        
        await expect(
            // LIVA expected mtime = 100, nhưng file thực tế mtime = 200
            manager.safeAppendInsights("test.md", "insight", 100)
        ).rejects.toThrow("CONCURRENCY_ERROR: File modified by user during AI processing");

        expect(fsp.writeFile).not.toHaveBeenCalled();
    });

    it("should create file if it does not exist (ENOENT bypass)", async () => {
        const error = new Error("Not found") as any;
        error.code = "ENOENT";
        vi.mocked(fsp.stat).mockRejectedValue(error);
        vi.mocked(fsp.mkdir).mockResolvedValue(undefined);
        vi.mocked(fsp.writeFile).mockResolvedValue(undefined);
        vi.mocked(fsp.rename).mockResolvedValue(undefined);

        await manager.safeAppendInsights("new.md", "hello", 100);

        expect(fsp.writeFile).toHaveBeenCalled();
        expect(fsp.rename).toHaveBeenCalled();
    });

    it("should append content securely using atomic write", async () => {
        vi.mocked(fsp.stat).mockResolvedValue({ mtimeMs: 100 } as any);
        vi.mocked(fsp.readFile).mockResolvedValue("Old Content\n");
        vi.mocked(fsp.mkdir).mockResolvedValue(undefined);
        vi.mocked(fsp.writeFile).mockResolvedValue(undefined);
        vi.mocked(fsp.rename).mockResolvedValue(undefined);

        await manager.safeAppendInsights("test.md", "My Insight", 100);

        // Verify tmp writing
        expect(fsp.writeFile).toHaveBeenCalledWith(
            expect.stringContaining(".tmp"),
            expect.stringContaining("> [!ai] LIVA Graph Weaver:\n> My Insight"),
            "utf-8"
        );
        expect(fsp.rename).toHaveBeenCalled();
    });
});
