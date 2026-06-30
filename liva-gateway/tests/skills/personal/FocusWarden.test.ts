import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

const mockExecAsync = vi.hoisted(() => vi.fn(() => Promise.resolve({ stdout: "", stderr: "" })));
const fsMocks = vi.hoisted(() => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    readFile: vi.fn().mockResolvedValue("127.0.0.1 localhost\n"),
    writeFile: vi.fn().mockResolvedValue(undefined),
}));

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));
vi.mock("@security/HITLGuard", () => ({
    HITLGuard: { requestApproval: vi.fn().mockResolvedValue(true) },
}));
vi.mock("node:util", () => ({ promisify: () => mockExecAsync }));
vi.mock("node:fs/promises", () => fsMocks);
vi.mock("node:os", () => ({ homedir: () => "/Users/test" }));

import { execute, metadata } from "../../../src/skills/personal/FocusWarden";

describe("Skill - FocusWarden", () => {
    let stdoutSpy: ReturnType<typeof vi.spyOn>;
    beforeEach(() => {
        vi.clearAllMocks();
        fsMocks.readFile.mockResolvedValue("127.0.0.1 localhost\n");
        stdoutSpy = vi.spyOn(process.stdout, "write").mockReturnValue(true);
    });
    afterEach(() => {
        setPlatform(realPlatform);
        stdoutSpy.mockRestore();
    });

    it("exports metadata", () => {
        expect(metadata.name).toBe("focus_warden");
    });

    it("on darwin: backs up hosts, writes proposed file, and prints a manual sudo command (no auto-sudo)", async () => {
        setPlatform("darwin");
        const result = await execute({
            action: "start", durationMinutes: 10, blockSites: ["facebook.com"],
            killGames: false, playLofi: false,
        });

        expect(result).toContain("FOCUS SUCCESS");
        // Surfaces a manual command — never auto-runs sudo.
        expect(result).toContain('sudo cp "/Users/test/.liva/hosts.focus.');
        expect(result).toContain("/etc/hosts");
        expect(result).toContain("dscacheutil -flushcache");
        // Backup + proposed files written; nothing shelled out (no sudo executed).
        expect(fsMocks.writeFile).toHaveBeenCalledTimes(2);
        expect(mockExecAsync).not.toHaveBeenCalled();

        // stop prints the restore command (from the backup), still no auto-sudo.
        const stopResult = await execute({ action: "stop" });
        expect(stopResult).toContain('sudo cp "/Users/test/.liva/hosts.backup.');
        expect(mockExecAsync).not.toHaveBeenCalled();
    });

    it("on win32: writes hosts via elevated PowerShell, not a printed sudo command", async () => {
        setPlatform("win32");
        const result = await execute({
            action: "start", durationMinutes: 10, blockSites: ["facebook.com"],
            killGames: false, playLofi: false,
        });
        expect(result).toContain("FOCUS SUCCESS");
        expect(result).not.toContain("sudo cp");
        expect(mockExecAsync).toHaveBeenCalled();
        expect(mockExecAsync.mock.calls.some(c => String(c[0]).includes("powershell.exe"))).toBe(true);

        await execute({ action: "stop" });
    });
});
