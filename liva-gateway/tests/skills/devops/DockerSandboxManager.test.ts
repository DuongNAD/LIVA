import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn((cmd: string, opts: any, cb?: Function) => {
        const callback = cb || opts;
        callback(null, "mocked output", "");
    })
}));

vi.mock("@security/HITLGuard", () => ({
    HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) }
}));

import { execute, metadata } from "../../../src/skills/devops/DockerSandboxManager";
import { HITLGuard } from "../../../src/security/HITLGuard";

describe("Skill - DockerSandboxManager", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("docker_sandbox_manager");
    });

    it("should run docker sandbox with HITL approval", async () => {
        const { exec: mockExec } = await import("node:child_process");
        vi.mocked(mockExec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, "Hello World", "");
            return {} as any;
        });

        const result = await execute({ image: "python:3.10-alpine", script: "echo hello" });
        expect(HITLGuard.requestApproval).toHaveBeenCalled();
        expect(result).toContain("DOCKER SUCCESS");
        expect(result).toContain("Hello World");
    });

    it("should block if HITL denied", async () => {
        vi.mocked(HITLGuard.requestApproval).mockRejectedValueOnce(new Error("User denied"));
        const result = await execute({ image: "node:18", script: "malicious()" });
        expect(result).toContain("DOCKER BLOCKED");
    });

    it("should handle docker exec error", async () => {
        const { exec: mockExec } = await import("node:child_process");
        vi.mocked(mockExec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback({ code: 1, name: "Error" }, "partial", "error msg");
            return {} as any;
        });

        const result = await execute({ image: "python:3.10", script: "exit 1" });
        expect(result).toContain("DOCKER FAILED");
    });

    it("should truncate long output", async () => {
        const { exec: mockExec } = await import("node:child_process");
        vi.mocked(mockExec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, "x".repeat(4000), "e".repeat(2000));
            return {} as any;
        });

        const result = await execute({ image: "node:18", script: "spam()" });
        expect(result).toContain("TRUNCATED");
    });

    it("should return ZodError for missing required fields", async () => {
        const result = await execute({ image: "" });
        expect(result).toContain("DOCKER ERROR");
        expect(result).toContain("Sai định dạng");
    });

    it("should sanitize dangerous characters in image name", async () => {
        const { exec: mockExec } = await import("node:child_process");
        vi.mocked(mockExec).mockImplementation((cmd: any, opts: any, cb?: any) => {
            const callback = cb || opts;
            callback(null, "safe", "");
            return {} as any;
        });

        const result = await execute({ image: "python;rm -rf /", script: "echo ok" });
        expect(result).toContain("DOCKER SUCCESS");
    });
});
