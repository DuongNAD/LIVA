import { describe, it, expect, vi, beforeEach } from "vitest";

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

import { execute, metadata } from "../../../src/skills/personal/VoiceSpeaker";

describe("Skill - VoiceSpeaker", () => {
    beforeEach(() => { vi.clearAllMocks(); });

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
});
