import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("node:child_process", () => ({
    exec: vi.fn((cmd: any, cb: any) => { cb(null, "", ""); return {} as any; })
}));
vi.mock("node:fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined)
}));

import { exec } from "node:child_process";
import { writeFile } from "node:fs/promises";
import { execute, metadata } from "../../../src/skills/personal/VoiceSpeaker";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

describe("Skill - VoiceSpeaker", () => {
    beforeEach(() => { vi.clearAllMocks(); });
    afterEach(() => { setPlatform(realPlatform); });

    it("should export metadata", () => { expect(metadata.name).toBe("voice_speaker"); });

    it("should speak text", async () => {
        const result = await execute({ text: "Hello World" });
        expect(result).toContain("VOICE SUCCESS");
    });

    it("should escape quotes in text", async () => {
        const result = await execute({ text: "It's a test" });
        expect(result).toContain("VOICE SUCCESS");
    });

    it("should accept volume and rate", async () => {
        const result = await execute({ text: "Test", volume: 50, rate: 3 });
        expect(result).toContain("VOICE SUCCESS");
    });

    it("should handle ZodError for missing text", async () => {
        const result = await execute({});
        expect(result).toContain("VOICE ERROR");
    });

    it("uses the macOS `say` command on darwin", async () => {
        setPlatform("darwin");
        await execute({ text: "Xin chào", volume: 50, rate: 0 });
        const cmd = vi.mocked(exec).mock.calls[0][0] as string;
        expect(cmd).toMatch(/^say -r \d+ -f /);
        // volume 50 → inline [[volm 0.50]] prefix written to the temp file
        const written = vi.mocked(writeFile).mock.calls[0][1] as string;
        expect(written).toContain("[[volm 0.50]]");
    });

    it("uses PowerShell System.Speech on win32", async () => {
        setPlatform("win32");
        await execute({ text: "Hello" });
        const cmd = vi.mocked(exec).mock.calls[0][0] as string;
        expect(cmd).toContain("powershell.exe");
        const written = vi.mocked(writeFile).mock.calls[0][1] as string;
        expect(written).toContain("System.Speech");
    });
});
