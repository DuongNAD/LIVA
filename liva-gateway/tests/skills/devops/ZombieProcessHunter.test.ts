import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

const mockExecAsync = vi.hoisted(() => vi.fn());

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
vi.mock("@security/HITLGuard", () => ({
    HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) },
}));
vi.mock("node:util", () => ({ promisify: () => mockExecAsync }));

import { execute, metadata } from "../../../src/skills/devops/ZombieProcessHunter";

describe("Skill - ZombieProcessHunter", () => {
    beforeEach(() => { vi.clearAllMocks(); });
    afterEach(() => { setPlatform(realPlatform); });

    it("exports metadata", () => {
        expect(metadata.name).toBe("zombie_process_hunter");
    });

    it("returns status without shelling out", async () => {
        const result = await execute({ action: "status" });
        expect(result).toContain("ZOMBIE STATUS");
        expect(mockExecAsync).not.toHaveBeenCalled();
    });

    describe("scan on darwin", () => {
        beforeEach(() => setPlatform("darwin"));

        it("parses `ps` output and filters by RAM threshold (rss KB → MB)", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout:
                    "1234 6291456 10:00 /Applications/Google Chrome.app/Contents/MacOS/Google Chrome\n" +
                    "5678 102400 0:05 node\n", // 100 MB → below default 5120 threshold, filtered out
            });
            const result = await execute({ action: "scan" });
            expect(mockExecAsync.mock.calls[0][0]).toContain("ps -axo");
            expect(result).toContain("Google Chrome");
            expect(result).toContain("6144"); // 6291456 KB / 1024
            expect(result).not.toContain("node");
        });

        it("reports a clean system when nothing exceeds the threshold", async () => {
            mockExecAsync.mockResolvedValueOnce({ stdout: "5678 102400 0:05 node\n" });
            const result = await execute({ action: "scan" });
            expect(result).toContain("Hệ thống sạch");
        });
    });

    describe("scan on win32", () => {
        beforeEach(() => setPlatform("win32"));

        it("parses PowerShell JSON output", async () => {
            mockExecAsync.mockResolvedValueOnce({
                stdout: JSON.stringify([{ Id: 1, ProcessName: "chrome", MemMB: 6000, CPU_Sec: 5 }]),
            });
            const result = await execute({ action: "scan" });
            expect(mockExecAsync.mock.calls[0][0]).toContain("powershell.exe");
            expect(result).toContain("chrome");
        });
    });
});
