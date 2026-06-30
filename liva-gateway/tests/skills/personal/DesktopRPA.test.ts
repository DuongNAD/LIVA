import { describe, it, expect, vi, beforeEach, afterEach } from "vitest";

const realPlatform = process.platform;
function setPlatform(p: string) {
    Object.defineProperty(process, "platform", { value: p, configurable: true });
}

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

vi.mock("node:child_process", () => ({
    // Support both exec(cmd, cb) and exec(cmd, opts, cb) signatures.
    exec: vi.fn((_cmd: string, optsOrCb: any, cb?: any) => {
        const callback = typeof optsOrCb === "function" ? optsOrCb : cb;
        callback(null, { stdout: "/path/screenshot.png", stderr: "" });
        return {} as any;
    })
}));

vi.mock("node:fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    writeFile: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined)
}));

import { execute, metadata } from "../../../src/skills/personal/DesktopRPA";
import { exec } from "node:child_process";
import { writeFile } from "node:fs/promises";

describe("Skill - DesktopRPA", () => {
    beforeEach(() => { vi.clearAllMocks(); });
    afterEach(() => { setPlatform(realPlatform); });

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

    describe("platform-specific commands", () => {
        it("darwin: screencapture / cliclick / osascript keystroke", async () => {
            setPlatform("darwin");

            await execute({ action: "take_screenshot" });
            expect(vi.mocked(exec).mock.calls[0][0]).toMatch(/^screencapture -x "/);

            vi.clearAllMocks();
            await execute({ action: "mouse_move", x: 100, y: 200 });
            expect(vi.mocked(exec).mock.calls[0][0]).toBe("cliclick m:100,200");

            vi.clearAllMocks();
            await execute({ action: "mouse_click", button: "double" });
            expect(vi.mocked(exec).mock.calls[0][0]).toBe("cliclick dc:.");

            vi.clearAllMocks();
            await execute({ action: "type_text", text: 'hi "there"' });
            expect((vi.mocked(writeFile).mock.calls[0][1] as string)).toContain('keystroke "hi \\"there\\""');
            expect(vi.mocked(exec).mock.calls[0][0]).toMatch(/^osascript "/);
        });

        it("darwin: helpful error when cliclick is missing", async () => {
            setPlatform("darwin");
            vi.mocked(exec).mockImplementationOnce(((_c: any, optsOrCb: any, cb?: any) => {
                const callback = typeof optsOrCb === "function" ? optsOrCb : cb;
                callback(new Error("not found"), null);
                return {} as any;
            }) as any);
            const result = await execute({ action: "mouse_move", x: 1, y: 2 });
            expect(result).toContain("RPA ERROR");
            expect(result).toContain("cliclick");
        });

        it("win32: PowerShell user32", async () => {
            setPlatform("win32");
            await execute({ action: "mouse_move", x: 5, y: 6 });
            expect(vi.mocked(exec).mock.calls[0][0]).toContain("powershell.exe");
        });
    });
});
