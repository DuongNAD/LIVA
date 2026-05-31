import { describe, it, expect, vi, beforeEach } from "vitest";
import { promisify } from "node:util";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));

// We need to create a mock that survives vi.mock hoisting.
// vi.hoisted() creates variables available inside vi.mock factories.
const { mockExecAsync } = vi.hoisted(() => ({
    mockExecAsync: vi.fn().mockResolvedValue({ stdout: "SUCCESS", stderr: "" })
}));

vi.mock("node:child_process", () => {
    const execFn = (...args: any[]) => { /* noop */ };
    (execFn as any)[promisify.custom] = mockExecAsync;
    return { exec: execFn };
});

vi.mock("node:fs/promises", () => ({
    mkdir: vi.fn().mockResolvedValue(undefined),
    unlink: vi.fn().mockResolvedValue(undefined)
}));

vi.mock("tesseract.js", () => ({
    default: { recognize: vi.fn().mockResolvedValue({ data: { text: "Hello World" } }) }
}));

vi.mock("clipboardy", () => ({
    default: { write: vi.fn().mockResolvedValue(undefined) }
}));

import { execute, metadata } from "../../../src/skills/data/ClipboardOCR";

describe("Skill - ClipboardOCR", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockExecAsync.mockResolvedValue({ stdout: "SUCCESS", stderr: "" });
    });

    it("should export metadata", () => { expect(metadata.name).toBe("clipboard_ocr"); });

    it("should OCR image from clipboard successfully", async () => {
        const result = await execute({ lang: "vie" });
        expect(result).toContain("OCR SUCCESS");
        expect(result).toContain("Hello World");
    });

    it("should handle no image in clipboard", async () => {
        mockExecAsync.mockResolvedValueOnce({ stdout: "NO_IMAGE", stderr: "" });
        const result = await execute({});
        expect(result).toContain("Không tìm thấy hình ảnh");
    });

    it("should handle empty OCR result", async () => {
        const tesseract = await import("tesseract.js");
        vi.mocked(tesseract.default.recognize).mockResolvedValueOnce({ data: { text: "  " } } as any);
        const result = await execute({});
        expect(result).toContain("không tìm thấy đoạn văn bản");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ lang: "chi" });
        expect(result).toContain("OCR ERROR");
        expect(result).toContain("Sai định dạng");
    });
});
