/**
 * GitSyncProject.test.ts — Git Auto-Sync Skill Tests
 * =====================================================
 * Tests: path validation, git operations, error handling.
 * child_process and fs are FULLY MOCKED — NO real git or disk ops.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("fs/promises", () => ({
    lstat: vi.fn(),
    access: vi.fn(),
    default: { lstat: vi.fn(), access: vi.fn() }
}));


// ============================================================
// Mock child_process and fs BEFORE importing the skill
// ============================================================
vi.mock("node:fs", async (importOriginal) => {
    const actual = await importOriginal<typeof import("node:fs")>();
    const existsSync = vi.fn();
    const access = vi.fn();
    return {
        ...actual,
        default: { ...actual.default, existsSync, promises: { ...actual.promises, access } },
        existsSync,
        promises: { ...actual.promises, access }
    };
});


import * as fs from "node:fs";

const mockExecFile = vi.fn();

vi.mock("child_process", () => ({
    execFile: (...args: any[]) => mockExecFile(...args),
    exec: (...args: any[]) => mockExecFile(...args),
}));

vi.mock("util", () => ({
    promisify: () => mockExecFile,
}));

const { execute, metadata } = await import("../../src/skills/GitSyncProject");

describe("GitSyncProject", () => {
    beforeEach(() => {
        vi.resetAllMocks();
        (fs.existsSync as any).mockReturnValue(true);
        (fs.promises.access as any).mockResolvedValue(undefined);
    });

    describe("metadata", () => {
        it("should have correct skill name", () => {
            expect(metadata.name).toBe("git_sync_project");
        });
        it("should require projectName and commitMessage", () => {
            expect(metadata.parameters.required).toContain("projectName");
            expect(metadata.parameters.required).toContain("commitMessage");
        });
    });

    describe("execute() — Happy Path", () => {
        it("should successfully run git add, commit, and push", async () => {
            // git add → success
            mockExecFile.mockResolvedValueOnce({ stdout: "", stderr: "" });
            // git commit → success
            mockExecFile.mockResolvedValueOnce({ stdout: "1 file changed, 3 insertions(+)", stderr: "" });
            // git push → success
            mockExecFile.mockResolvedValueOnce({ stdout: "To github.com:user/repo.git", stderr: "Everything up-to-date" });

            (fs.promises.access as any).mockResolvedValue(undefined);

            const result = await execute({ projectName: "LIVA", commitMessage: "feat: add new skill" });

            expect(result).toContain("PUSH CODE THÀNH CÔNG");
            expect(mockExecFile).toHaveBeenCalledTimes(3);
        });
    });

    describe("execute() — Error Paths", () => {
        it("should return error if project directory does not exist", async () => {
            (fs.existsSync as any).mockReturnValue(false);

            const result = await execute({ projectName: "NonExistentProject", commitMessage: "test" });

            expect(result).toContain("LỖI");
            expect(result).toContain("NonExistentProject");
            expect(mockExecFile).not.toHaveBeenCalled();
        });

        it("should return error if .git directory does not exist", async () => {
            // Project exists but no .git
            (fs.existsSync as any).mockImplementation((p: string) => !p.endsWith(".git"));

            const result = await execute({ projectName: "NoGitProject", commitMessage: "test" });

            expect(result).toContain("LỖI HỆ THỐNG");
            expect(result).toContain(".git");
        });

        it("should handle 'nothing to commit' gracefully", async () => {
            // git add → success
            mockExecFile.mockResolvedValueOnce({ stdout: "", stderr: "" });
            // git commit → nothing to commit error
            const commitError = new Error("Command failed");
            (commitError as any).stdout = "nothing to commit, working tree clean";
            mockExecFile.mockRejectedValueOnce(commitError);

            (fs.promises.access as any).mockResolvedValue(undefined);

            const result = await execute({ projectName: "CleanProject", commitMessage: "test" });

            expect(result).toContain("THÔNG BÁO");
            expect(result).toContain("không có thay đổi");
        });

        it("should handle git add failure", async () => {
            mockExecFile.mockRejectedValueOnce(new Error("fatal: not a git repository"));

            (fs.promises.access as any).mockResolvedValue(undefined);

            const result = await execute({ projectName: "BrokenRepo", commitMessage: "test" });

            expect(result).toContain("LỖI");
            expect(result).toContain("git add");
        });

        it("should handle git push failure (network error)", async () => {
            // git add → success
            mockExecFile.mockResolvedValueOnce({ stdout: "", stderr: "" });
            // git commit → success
            mockExecFile.mockResolvedValueOnce({ stdout: "1 file changed", stderr: "" });
            // git push → failure
            const pushError = new Error("fatal: unable to access 'https://github.com/...'");
            (pushError as any).stderr = "Could not resolve host";
            (pushError as any).stdout = "";
            mockExecFile.mockRejectedValueOnce(pushError);

            (fs.promises.access as any).mockResolvedValue(undefined);

            const result = await execute({ projectName: "NoNetProject", commitMessage: "test" });

            expect(result).toContain("LỖI PUSH");
        });
    });
});
