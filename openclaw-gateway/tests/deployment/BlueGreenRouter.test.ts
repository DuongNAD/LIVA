/**
 * BlueGreenRouter.test.ts — Safe Rollback via Physical Snapshot tests
 * Mocks child_process and fs to avoid real git/filesystem operations
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
    execFileSync: vi.fn().mockReturnValue(""),
}));

import { execSync, execFileSync } from "node:child_process";
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
        it("should create rollback snapshot before deployment", async () => {
            // Working tree is clean (git status returns empty)
            (execSync as any)
                .mockReturnValueOnce("main\n")  // getCurrentBranch
                .mockReturnValueOnce("");         // isWorkingTreeClean

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
            // Verify snapshot was created: cpSync called for snapshot + deploy + cleanup
            expect(fs.cpSync).toHaveBeenCalled();
        });

        it("should deploy sandbox to host successfully", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")  // getCurrentBranch
                .mockReturnValueOnce("");         // isWorkingTreeClean

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
            expect(fs.cpSync).toHaveBeenCalled();
        });

        it("should stash dirty changes before deploy", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")        // getCurrentBranch
                .mockReturnValueOnce("M src/file.ts")  // isWorkingTreeClean (dirty)
                .mockReturnValueOnce("");               // git stash

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
        });

        it("should fail when sandbox src/ does not exist", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            // existsSync returns true for rollback creation, then false for sandbox check
            (fs.existsSync as any)
                .mockReturnValueOnce(false)  // ROLLBACK_BAK_DIR cleanup check
                .mockReturnValueOnce(false)  // sandboxSrcPath check
                .mockReturnValueOnce(true);  // rollback restore check

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });

        it("should rollback on deployment error and clean up", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")  // getCurrentBranch
                .mockReturnValueOnce("");         // isWorkingTreeClean
            // First cpSync succeeds (snapshot), second throws (deploy)
            (fs.cpSync as any)
                .mockImplementationOnce(() => {}) // snapshot creation
                .mockImplementationOnce(() => { throw new Error("Copy failed"); }); // deploy
            (fs.existsSync as any).mockReturnValue(true);

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });

        it("should fail gracefully if rollback snapshot creation fails", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            // cpSync throws on snapshot creation
            (fs.cpSync as any).mockImplementation(() => { throw new Error("Disk full"); });
            (fs.existsSync as any)
                .mockReturnValueOnce(false);  // No existing snapshot to clean

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });
    });

    describe("autoRollbackBatch", () => {
        it("should rollback from physical snapshot (not git checkout)", async () => {
            (fs.existsSync as any)
                .mockReturnValueOnce(true)   // ROLLBACK_BAK_DIR exists
                .mockReturnValueOnce(true)   // originalSrcPath exists (for rmSync)
                .mockReturnValueOnce(true);  // cleanup check
            (execSync as any).mockReturnValue(""); // git stash pop

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
            // Verify NO git checkout -- src/ or git clean -fd src/ was called
            const execCalls = (execSync as any).mock.calls.map((c: any) => c[0]);
            expect(execCalls).not.toContain("git checkout -- src/");
            expect(execCalls).not.toContain("git clean -fd src/");
        });

        it("should use legacy .src.blue.bak fallback if snapshot not found", async () => {
            (fs.existsSync as any)
                .mockReturnValueOnce(false)  // ROLLBACK_BAK_DIR not found
                .mockReturnValueOnce(true)   // legacy .src.blue.bak exists
                .mockReturnValueOnce(true);  // originalSrcPath exists
            (execSync as any).mockReturnValue("");

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
            expect(fs.cpSync).toHaveBeenCalled();
        });

        it("should return false when no rollback source is available", async () => {
            (fs.existsSync as any).mockReturnValue(false);
            (execSync as any).mockReturnValue("");

            const result = await router.autoRollbackBatch();
            expect(result).toBe(false);
        });

        it("should return false on fatal rollback error", async () => {
            (fs.existsSync as any).mockImplementation(() => { throw new Error("FS crash"); });

            const result = await router.autoRollbackBatch();
            expect(result).toBe(false);
        });
    });

    describe("autoRollback (backward compat)", () => {
        it("should delegate to autoRollbackBatch", async () => {
            (fs.existsSync as any).mockReturnValue(true);
            (execSync as any).mockReturnValue("");
            const result = await router.autoRollback("src/test.ts");
            expect(result).toBe(true);
        });
    });
});
