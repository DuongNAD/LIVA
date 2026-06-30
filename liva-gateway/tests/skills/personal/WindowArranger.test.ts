import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("node:child_process", () => ({
    exec: vi.fn()
}));

import { execute, metadata } from "../../../src/skills/personal/WindowArranger";
import { exec } from "node:child_process";

describe("Skill - WindowArranger", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => { cb(null, { stdout: "" }); return {} as any; });
    });

    it("should export metadata", () => { expect(metadata.name).toBe("window_arranger"); });

    it("should snap window left", async () => {
        const result = await execute({ action: "snap_left" });
        expect(result).toContain("WINDOW SUCCESS");
        expect(result).toContain("snap_left");
    });

    it("should snap window right", async () => {
        const result = await execute({ action: "snap_right" });
        expect(result).toContain("WINDOW SUCCESS");
    });

    it("should maximize window", async () => {
        const result = await execute({ action: "maximize" });
        expect(result).toContain("WINDOW SUCCESS");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ action: "minimize" });
        expect(result).toContain("WINDOW ERROR");
    });

    it("should handle exec error", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => { cb(new Error("powershell fail")); return {} as any; });
        const result = await execute({ action: "snap_left" });
        expect(result).toContain("WINDOW ERROR");
    });

    describe("platform-specific commands", () => {
        afterEach(() => { setPlatform(realPlatform); });

        it("resizes the frontmost window via osascript on darwin", async () => {
            setPlatform("darwin");
            await execute({ action: "snap_right" });
            const cmd = vi.mocked(exec).mock.calls[0][0] as string;
            expect(cmd).toContain("osascript");
            expect(cmd).toContain("System Events");
            expect(cmd).toContain("set position of frontWin");
            expect(cmd).toContain("(sw / 2) as integer");
        });

        it("uses PowerShell user32 on win32", async () => {
            setPlatform("win32");
            await execute({ action: "maximize" });
            expect(vi.mocked(exec).mock.calls[0][0]).toContain("powershell.exe");
        });
    });
});
