import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

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
    afterEach(() => { setPlatform(realPlatform); });

    describe("metadata", () => {
        it("should have correct name and parameters", () => {
            expect(ProcessManager.metadata.name).toBe("process_manager");
            expect(ProcessManager.metadata.parameters.required).toContain("action");
        });
    });

    describe("list action (win32)", () => {
        beforeEach(() => setPlatform("win32"));
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

    describe("search action (win32)", () => {
        beforeEach(() => setPlatform("win32"));
        it("should find matching processes", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout: JSON.stringify({
                    ProcessName: "llama-server", Id: 9999, CPU_Sec: 30.0, RAM_MB: 4096.0
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

    describe("macOS (darwin) ps parsing", () => {
        beforeEach(() => setPlatform("darwin"));

        it("lists processes parsed from `ps` output (rss KB → MB, basename of comm)", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout:
                    "1234 524288 10.5 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n" +
                    "5678 262144 5.2 node\n",
            });
            const result = await ProcessManager.execute({ action: "list", sortBy: "memory" });
            expect(mockExecAsync.mock.calls[0][0]).toContain("ps -axo");
            expect(result).toContain("Google Chrome");
            expect(result).toContain("1234");
            expect(result).toContain("512"); // 524288 KB / 1024 = 512 MB
        });

        it("filters processes by name on search", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout:
                    "9999 4194304 30.0 /usr/local/bin/llama-server\n" +
                    "5678 262144 5.2 node\n",
            });
            const result = await ProcessManager.execute({ action: "search", name: "llama" });
            expect(result).toContain("llama-server");
            expect(result).toContain("9999");
            expect(result).not.toContain("node (PID");
        });

        it("kills by pid with `kill -9` after HITL approval", async () => {
            mockExecAsync.mockResolvedValueOnce({ stdout: "" });
            const result = await ProcessManager.execute({ action: "kill", pid: 4242 });
            expect(mockExecAsync.mock.calls[0][0]).toBe("kill -9 4242");
            expect(result).toContain("PROCESS KILLED");
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
