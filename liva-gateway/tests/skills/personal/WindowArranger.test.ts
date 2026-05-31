import { describe, it, expect, vi, beforeEach } from "vitest";

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
});
