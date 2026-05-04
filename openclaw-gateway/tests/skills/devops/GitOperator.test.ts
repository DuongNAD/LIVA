import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn()
}));

vi.mock("@security/HITLGuard", () => ({
    HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) }
}));

import { execute, metadata } from "../../../src/skills/devops/GitOperator";
import { exec } from "node:child_process";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - GitOperator", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(exec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, { stdout: "On branch main", stderr: "" });
            return {} as any;
        });
    });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("git_operator");
    });

    it("should run read-only git status without HITL", async () => {
        const result = await execute({ action: "status" });
        expect(HITLGuard.requestApproval).not.toHaveBeenCalled();
        expect(result).toContain("GIT SUCCESS");
    });

    it("should run git log without HITL", async () => {
        const result = await execute({ action: "log" });
        expect(result).toContain("GIT SUCCESS");
    });

    it("should require HITL for commit", async () => {
        const result = await execute({ action: "commit", args: ["-m", "fix bug"] });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("GIT SUCCESS");
    });

    it("should require HITL for push", async () => {
        const result = await execute({ action: "push", args: ["origin", "main"] });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
    });

    it("should block if HITL denied for push", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("Denied"));
        const result = await execute({ action: "push" });
        expect(result).toContain("GIT ACTION BLOCKED");
    });

    it("should truncate long output", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, { stdout: "x".repeat(4000), stderr: "" });
            return {} as any;
        });
        const result = await execute({ action: "diff" });
        expect(result).toContain("Output bị cắt bớt");
    });

    it("should sanitize shell-dangerous args", async () => {
        const result = await execute({ action: "status", args: ["--oneline; rm -rf /"] });
        expect(result).toContain("GIT SUCCESS");
    });

    it("should handle exec failure", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(new Error("command failed"), { stdout: "", stderr: "" });
            return {} as any;
        });
        const result = await execute({ action: "status" });
        expect(result).toContain("GIT ERROR");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ action: "invalid_action" });
        expect(result).toContain("GIT ERROR");
        expect(result).toContain("Sai định dạng");
    });

    it("should include stderr in output", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, { stdout: "OK", stderr: "warning: something" });
            return {} as any;
        });
        const result = await execute({ action: "status" });
        expect(result).toContain("[STDERR]");
    });
});
