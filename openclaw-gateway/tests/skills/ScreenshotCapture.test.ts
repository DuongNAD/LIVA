import { describe, it, expect, vi } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

const mockExecAsync = vi.hoisted(() => vi.fn());

vi.mock("node:util", () => ({
    promisify: () => mockExecAsync,
}));

// Mock fs.promises for directory creation and stat
vi.mock("node:fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    stat: vi.fn().mockResolvedValue({ size: 102400 }),
}));

import * as ScreenshotCapture from "../../src/skills/personal/ScreenshotCapture";

describe("ScreenshotCapture Skill", () => {
    it("should have correct metadata", () => {
        expect(ScreenshotCapture.metadata.name).toBe("screenshot_capture");
    });

    it("should capture full screen by default", async () => {
        mockExecAsync.mockResolvedValueOnce({ stdout: "OK" });

        const result = await ScreenshotCapture.execute({});
        expect(result).toContain("SCREENSHOT SUCCESS");
        expect(result).toContain("Toàn màn hình");
        expect(mockExecAsync).toHaveBeenCalledOnce();
    });

    it("should capture active window when specified", async () => {
        mockExecAsync.mockResolvedValueOnce({ stdout: "OK" });

        const result = await ScreenshotCapture.execute({ region: "active" });
        expect(result).toContain("SCREENSHOT SUCCESS");
        expect(result).toContain("Cửa sổ active");
    });

    it("should use custom output path", async () => {
        mockExecAsync.mockResolvedValueOnce({ stdout: "OK" });

        const result = await ScreenshotCapture.execute({ outputPath: "test_screenshot.png" });
        expect(result).toContain("SCREENSHOT SUCCESS");
        expect(result).toContain("test_screenshot.png");
    });

    it("should handle PowerShell errors gracefully", async () => {
        mockExecAsync.mockRejectedValueOnce(new Error("PowerShell execution failed"));

        const result = await ScreenshotCapture.execute({});
        expect(result).toContain("SCREENSHOT ERROR");
        expect(result).toContain("PowerShell execution failed");
    });

    it("should reject invalid region", async () => {
        const result = await ScreenshotCapture.execute({ region: "invalid" });
        expect(result).toContain("ERROR");
    });
});
