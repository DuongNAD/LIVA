/**
 * BlueGreenRouter.test.ts — Git-native atomic deployment tests
 * Mocks child_process.execSync to avoid real git operations
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("fs", () => ({
    cpSync: vi.fn(),
    existsSync: vi.fn(),
    rmSync: vi.fn(),
    mkdirSync: vi.fn(),
    default: {
        cpSync: vi.fn(),
        existsSync: vi.fn(),
        rmSync: vi.fn(),
        mkdirSync: vi.fn()
    }
}));


vi.mock("child_process", () => ({
    execSync: vi.fn().mockReturnValue("main\n"),
}));

import { execSync } from "node:child_process";
import * as fs from "node:fs";
import { BlueGreenRouter } from "../../src/deployment/BlueGreenRouter";

describe("BlueGreenRouter", () => {
    let router: BlueGreenRouter;

    beforeEach(() => {
        vi.resetAllMocks();
        router = new BlueGreenRouter("/tmp/host");
        (fs.existsSync as any).mockReturnValue(true);
    });
    describe("deployToGreenBatch", () => {
        it("should deploy sandbox to host successfully", async () => {
            // Working tree is clean (git status returns empty)
            (execSync as any)
                .mockReturnValueOnce("main\n")  // getCurrentBranch
                .mockReturnValueOnce("")          // isWorkingTreeClean
                .mockReturnValueOnce("")          // git add -A
                .mockReturnValueOnce("");         // git commit

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
            expect(fs.cpSync).toHaveBeenCalled();
        });

        it("should stash dirty changes before deploy", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")        // getCurrentBranch
                .mockReturnValueOnce("M src/file.ts")  // isWorkingTreeClean (dirty)
                .mockReturnValueOnce("")               // git stash
                .mockReturnValueOnce("")               // git add -A
                .mockReturnValueOnce("");              // git commit

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
        });

        it("should fail when sandbox src/ does not exist", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            // existsSync is called for sandboxSrcPath — return false
            (fs.existsSync as any).mockReturnValue(false);

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });
    });

    describe("autoRollbackBatch", () => {
        it("should rollback via git checkout", async () => {
            (execSync as any)
                .mockReturnValueOnce("")  // git checkout -- src/
                .mockReturnValueOnce("")  // git clean -fd src/
                .mockReturnValueOnce(""); // git stash pop

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
        });

        it("should use filesystem fallback if git fails", async () => {
            (execSync as any).mockImplementation(() => { throw new Error("Git error"); });
            (fs.existsSync as any).mockReturnValue(true);

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
            expect(fs.cpSync).toHaveBeenCalled();
        });

        it("should return false when both git and filesystem fail", async () => {
            (execSync as any).mockImplementation(() => { throw new Error("Git error"); });
            (fs.existsSync as any).mockReturnValue(false);

            const result = await router.autoRollbackBatch();
            expect(result).toBe(false);
        });
    });

    describe("autoRollback (backward compat)", () => {
        it("should delegate to autoRollbackBatch", async () => {
            (execSync as any).mockReturnValue("");
            const result = await router.autoRollback("src/test.ts");
            expect(result).toBe(true);
        });
    });
});
