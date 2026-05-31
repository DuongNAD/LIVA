import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("../../src/security/HITLGuard", () => ({
    HITLGuard: {
        events: { on: vi.fn(), emit: vi.fn(), removeListener: vi.fn(), removeAllListeners: vi.fn() },
        requestApproval: vi.fn().mockResolvedValue(true),
        respond: vi.fn(),
    },
}));

// Use vi.hoisted to fix initialization order
const mockExecAsync = vi.hoisted(() => vi.fn());

vi.mock("node:util", () => ({
    promisify: () => mockExecAsync,
}));

import * as ProcessManager from "../../src/skills/devops/ProcessManager";

describe("ProcessManager Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should have correct name and parameters", () => {
            expect(ProcessManager.metadata.name).toBe("process_manager");
            expect(ProcessManager.metadata.parameters.required).toContain("action");
        });
    });

    describe("list action", () => {
        it("should list top processes sorted by memory", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout: JSON.stringify([
                    { ProcessName: "chrome", Id: 1234, CPU_Sec: 10.5, RAM_MB: 512.3 },
                    { ProcessName: "node", Id: 5678, CPU_Sec: 5.2, RAM_MB: 256.1 },
                ]),
            });

            const result = await ProcessManager.execute({ action: "list", sortBy: "memory" });
            expect(result).toContain("chrome");
            expect(result).toContain("1234");
            expect(result).toContain("512.3");
        });
    });

    describe("search action", () => {
        it("should find matching processes", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout: JSON.stringify({
                    ProcessName: "llama-server", Id: 9999, CPU_Sec: 30.0, RAM_MB: 4096.0, StartTime: "2026-04-29 10:00:00"
                }),
            });

            const result = await ProcessManager.execute({ action: "search", name: "llama" });
            expect(result).toContain("llama-server");
            expect(result).toContain("9999");
        });

        it("should require name parameter", async () => {
            const result = await ProcessManager.execute({ action: "search" });
            expect(result).toContain("ERROR");
            expect(result).toContain("name");
        });

        it("should handle no matches gracefully", async () => {
            mockExecAsync.mockResolvedValueOnce({ stdout: "" });

            const result = await ProcessManager.execute({ action: "search", name: "nonexistent" });
            expect(result).toContain("Không tìm thấy");
        });
    });

    describe("kill action", () => {
        it("should require HITL approval before killing", async () => {
            const { HITLGuard } = await import("../../src/security/HITLGuard");

            mockExecAsync.mockResolvedValueOnce({ stdout: "" });

            await ProcessManager.execute({ action: "kill", pid: 1234 });
            expect(HITLGuard.requestApproval).toHaveBeenCalledOnce();
        });

        it("should block kill if HITL rejects", async () => {
            const { HITLGuard } = await import("../../src/security/HITLGuard");
            (HITLGuard.requestApproval as any).mockRejectedValueOnce(new Error("REJECTED_BY_USER"));

            const result = await ProcessManager.execute({ action: "kill", pid: 1234 });
            expect(result).toContain("BLOCKED");
        });

        it("should require pid or name", async () => {
            const result = await ProcessManager.execute({ action: "kill" });
            expect(result).toContain("ERROR");
        });
    });

    describe("validation", () => {
        it("should reject invalid action", async () => {
            const result = await ProcessManager.execute({ action: "invalid_action" });
            expect(result).toContain("ERROR");
        });
    });
});
