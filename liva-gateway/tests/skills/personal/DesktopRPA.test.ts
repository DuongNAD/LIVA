import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    exec: vi.fn((cmd: string, cb: Function) => cb(null, { stdout: "/path/screenshot.png" }))
}));

vi.mock("node:fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined)
}));

import { execute, metadata } from "../../../src/skills/personal/DesktopRPA";
import { exec } from "node:child_process";

describe("Skill - DesktopRPA", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export correct metadata", () => {
        expect(metadata.name).toBe("desktop_rpa");
        expect(metadata.parameters.required).toContain("action");
    });

    it("should take screenshot", async () => {
        const result = await execute({ action: "take_screenshot" });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("chụp toàn bộ màn hình");
    });

    it("should move mouse to coordinates", async () => {
        const result = await execute({ action: "mouse_move", x: 100, y: 200 });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("(100, 200)");
    });

    it("should fail mouse_move without coordinates", async () => {
        const result = await execute({ action: "mouse_move" });
        expect(result).toContain("RPA ERROR");
    });

    it("should click mouse (left)", async () => {
        const result = await execute({ action: "mouse_click" });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("left");
    });

    it("should click mouse (right)", async () => {
        const result = await execute({ action: "mouse_click", button: "right" });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("right");
    });

    it("should double click mouse", async () => {
        const result = await execute({ action: "mouse_click", button: "double" });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("double");
    });

    it("should type text", async () => {
        const result = await execute({ action: "type_text", text: "hello world" });
        expect(result).toContain("RPA SUCCESS");
        expect(result).toContain("gõ tự động");
    });

    it("should fail type_text without text", async () => {
        const result = await execute({ action: "type_text" });
        expect(result).toContain("RPA ERROR");
    });

    it("should return error for ZodError", async () => {
        const result = await execute({ action: "invalid_action" });
        expect(result).toContain("RPA ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
