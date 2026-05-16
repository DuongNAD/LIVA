import { describe, it, expect, vi, beforeEach } from "vitest";

const mockCopyFile = vi.fn();
const mockRename = vi.fn();
const mockUnlink = vi.fn();

vi.mock("node:fs/promises", () => ({
    copyFile: (...args: any[]) => mockCopyFile(...args),
    rename: (...args: any[]) => mockRename(...args),
    unlink: (...args: any[]) => mockUnlink(...args),
}));

const mockExistsSync = vi.fn();
vi.mock("node:fs", () => ({
    existsSync: (...args: any[]) => mockExistsSync(...args),
}));

vi.mock("../../src/utils/FileUtils", () => ({
    safeRename: (...args: any[]) => mockRename(...args)
}));

vi.mock("../../src/evolution/EvolutionLogger", () => ({
    evoLogger: { info: vi.fn(), warn: vi.fn(), error: vi.fn() }
}));

import { RollbackManager } from "../../src/evolution/RollbackManager";

describe("RollbackManager", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    it("backup should copy file if exists", async () => {
        mockExistsSync.mockReturnValueOnce(true);
        const ctx: any = { hypothesis: { targetFilePath: "test.ts" } };
        await RollbackManager.backup(ctx);
        expect(mockCopyFile).toHaveBeenCalledWith("test.ts", "test.ts.bak");
    });

    it("backup should throw if copy fails", async () => {
        mockExistsSync.mockReturnValueOnce(true);
        mockCopyFile.mockRejectedValueOnce(new Error("fail"));
        const ctx: any = { hypothesis: { targetFilePath: "test.ts" } };
        await expect(RollbackManager.backup(ctx)).rejects.toThrow("RollbackManager Backup Failed: fail");
    });

    it("restore should rename file if backup exists", async () => {
        mockExistsSync.mockReturnValueOnce(true);
        const ctx: any = { hypothesis: { targetFilePath: "test.ts" } };
        await RollbackManager.restore(ctx);
        expect(mockRename).toHaveBeenCalledWith("test.ts.bak", "test.ts");
    });

    it("cleanup should unlink file if backup exists", async () => {
        mockExistsSync.mockReturnValueOnce(true);
        const ctx: any = { hypothesis: { targetFilePath: "test.ts" } };
        await RollbackManager.cleanup(ctx);
        expect(mockUnlink).toHaveBeenCalledWith("test.ts.bak");
    });
});
