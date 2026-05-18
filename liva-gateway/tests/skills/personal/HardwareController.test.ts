import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("node:child_process", () => ({
    exec: vi.fn()
}));

import { execute, metadata } from "../../../src/skills/personal/HardwareController";
import { exec } from "node:child_process";

describe("Skill - HardwareController", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => { cb(null, { stdout: "", stderr: "" }); return {} as any; });
    });

    it("should export metadata", () => { expect(metadata.name).toBe("hardware_controller"); });

    it("should set brightness", async () => {
        const result = await execute({ action: "set_brightness", level: 75 });
        expect(result).toContain("HARDWARE SUCCESS");
        expect(result).toContain("75%");
    });

    it("should set volume", async () => {
        const result = await execute({ action: "set_volume", level: 50 });
        expect(result).toContain("HARDWARE SUCCESS");
        expect(result).toContain("50%");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ action: "invalid", level: 50 });
        expect(result).toContain("HARDWARE ERROR");
    });

    it("should handle exec error", async () => {
        vi.mocked(exec).mockImplementation((cmd: any, cb: any) => { cb(new Error("WMI fail")); return {} as any; });
        const result = await execute({ action: "set_brightness", level: 50 });
        expect(result).toContain("HARDWARE ERROR");
    });
});
