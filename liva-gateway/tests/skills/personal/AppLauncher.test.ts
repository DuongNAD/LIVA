import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("node:child_process", () => ({
    exec: vi.fn((_cmd: any, cb: any) => { cb(null, "", ""); return {} as any; })
}));

import { exec } from "node:child_process";
import { execute, metadata } from "../../../src/skills/personal/AppLauncher";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

describe("Skill - AppLauncher", () => {
    beforeEach(() => { vi.clearAllMocks(); });
    afterEach(() => { setPlatform(realPlatform); });

    it("should export metadata", () => { expect(metadata.name).toBe("app_launcher"); });

    it("uses `open -a` on darwin", async () => {
        setPlatform("darwin");
        const result = await execute({ appName: "Safari" });
        expect(result).toContain("LAUNCHER SUCCESS");
        expect(vi.mocked(exec).mock.calls[0][0]).toBe('open -a "Safari"');
    });

    it("falls back to bare `open` when `open -a` fails on darwin", async () => {
        setPlatform("darwin");
        vi.mocked(exec)
            .mockImplementationOnce(((_c: any, cb: any) => { cb(new Error("not found"), "", ""); return {} as any; }) as any)
            .mockImplementationOnce(((_c: any, cb: any) => { cb(null, "", ""); return {} as any; }) as any);
        await execute({ appName: "SomeApp" });
        expect(vi.mocked(exec).mock.calls[1][0]).toBe('open "SomeApp"');
    });

    it("uses PowerShell Start-Process on win32", async () => {
        setPlatform("win32");
        await execute({ appName: "chrome" });
        expect(vi.mocked(exec).mock.calls[0][0]).toContain("powershell.exe");
        expect(vi.mocked(exec).mock.calls[0][0]).toContain("Start-Process");
    });

    it("strips injection characters from appName", async () => {
        setPlatform("darwin");
        await execute({ appName: "Safari; rm -rf /" });
        // ';' and '/' stripped → safe arg
        expect(vi.mocked(exec).mock.calls[0][0]).toBe('open -a "Safari rm -rf "');
    });

    it("returns LAUNCHER ERROR on invalid args", async () => {
        const result = await execute({});
        expect(result).toContain("LAUNCHER ERROR");
    });
});
