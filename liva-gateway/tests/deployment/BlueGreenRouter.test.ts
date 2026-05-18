/**
 * BlueGreenRouter.test.ts — Safe Rollback via Physical Snapshot tests
 * Mocks child_process and fs to avoid real git/filesystem operations
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("node:fs/promises", () => ({
    cp: vi.fn(),
    access: vi.fn(),
    rm: vi.fn(),
    mkdir: vi.fn(),
    default: {
        cp: vi.fn(),
        access: vi.fn(),
        rm: vi.fn(),
        mkdir: vi.fn()
    }
}));

vi.mock("child_process", () => ({
    execSync: vi.fn().mockReturnValue("main\n"),
    execFileSync: vi.fn().mockReturnValue(""),
}));

import { execSync, execFileSync } from "node:child_process";
import * as fsp from "node:fs/promises";
import { BlueGreenRouter } from "../../src/evolution/BlueGreenRouter";

describe("BlueGreenRouter", () => {
    let router: BlueGreenRouter;

    beforeEach(() => {
        vi.resetAllMocks();
        router = new BlueGreenRouter("/tmp/host");
        (fsp.access as any).mockResolvedValue(undefined);
    });

    describe("deployToGreenBatch", () => {
        it("should create rollback snapshot before deployment", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
            expect(fsp.cp).toHaveBeenCalled();
        });

        it("should deploy sandbox to host successfully", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
            expect(fsp.cp).toHaveBeenCalled();
        });

        it("should stash dirty changes before deploy", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("M src/file.ts")
                .mockReturnValueOnce("");

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(true);
        });

        it("should fail when sandbox src/ does not exist", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            (fsp.access as any)
                .mockRejectedValueOnce(new Error())  // ROLLBACK_BAK_DIR cleanup check
                .mockRejectedValueOnce(new Error())  // sandboxSrcPath check
                .mockResolvedValueOnce(undefined);  // rollback restore check

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });

        it("should rollback on deployment error and clean up", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            (fsp.cp as any)
                .mockResolvedValueOnce(undefined) // snapshot creation
                .mockRejectedValueOnce(new Error("Copy failed")); // deploy
            (fsp.access as any).mockResolvedValue(undefined);

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });

        it("should fail gracefully if rollback snapshot creation fails", async () => {
            (execSync as any)
                .mockReturnValueOnce("main\n")
                .mockReturnValueOnce("");
            (fsp.cp as any).mockRejectedValue(new Error("Disk full"));
            (fsp.access as any)
                .mockRejectedValueOnce(new Error());  // No existing snapshot to clean

            const result = await router.deployToGreenBatch("/tmp/sandbox");
            expect(result).toBe(false);
        });
    });

    describe("autoRollbackBatch", () => {
        it("should rollback from physical snapshot (not git checkout)", async () => {
            (fsp.access as any)
                .mockResolvedValueOnce(undefined)   // ROLLBACK_BAK_DIR exists
                .mockResolvedValueOnce(undefined)   // originalSrcPath exists (for rmSync)
                .mockResolvedValueOnce(undefined);  // cleanup check
            (execSync as any).mockReturnValue("");

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
            const execCalls = (execSync as any).mock.calls.map((c: any) => c[0]);
            expect(execCalls).not.toContain("git checkout -- src/");
            expect(execCalls).not.toContain("git clean -fd src/");
        });

        it("should use legacy .src.blue.bak fallback if snapshot not found", async () => {
            (fsp.access as any)
                .mockRejectedValueOnce(new Error())  // ROLLBACK_BAK_DIR not found
                .mockResolvedValueOnce(undefined)   // legacy .src.blue.bak exists
                .mockResolvedValueOnce(undefined);  // originalSrcPath exists
            (execSync as any).mockReturnValue("");

            const result = await router.autoRollbackBatch();
            expect(result).toBe(true);
            expect(fsp.cp).toHaveBeenCalled();
        });

        it("should return false when no rollback source is available", async () => {
            (fsp.access as any).mockRejectedValue(new Error());
            (execSync as any).mockReturnValue("");

            const result = await router.autoRollbackBatch();
            expect(result).toBe(false);
        });

        it("should return false on fatal rollback error", async () => {
            (fsp.access as any).mockImplementation(() => { throw new Error("FS crash"); });

            const result = await router.autoRollbackBatch();
            expect(result).toBe(false);
        });
    });

    describe("autoRollback (backward compat)", () => {
        it("should delegate to autoRollbackBatch", async () => {
            (fsp.access as any).mockResolvedValue(undefined);
            (execSync as any).mockReturnValue("");
            const result = await router.autoRollback("src/test.ts");
            expect(result).toBe(true);
        });
    });
});
