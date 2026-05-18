import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("@utils/logger", () => ({
    logger: { info: vi.fn(), error: vi.fn(), warn: vi.fn(), debug: vi.fn() }
}));
vi.mock("clipboardy", () => ({
    default: {
        read: vi.fn().mockResolvedValue("copied text"),
        write: vi.fn().mockResolvedValue(undefined)
    }
}));

import { execute, metadata } from "../../../src/skills/personal/ClipboardManager";

describe("Skill - ClipboardManager", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should export metadata", () => { expect(metadata.name).toBe("clipboard_manager"); });

    it("should read clipboard content", async () => {
        const result = await execute({ action: "read" });
        expect(result).toContain("CLIPBOARD DATA");
        expect(result).toContain("copied text");
    });

    it("should return empty message when clipboard empty", async () => {
        const clip = await import("clipboardy");
        vi.mocked(clip.default.read).mockResolvedValueOnce("");
        const result = await execute({ action: "read" });
        expect(result).toContain("CLIPBOARD EMPTY");
    });

    it("should write to clipboard", async () => {
        const result = await execute({ action: "write", content: "new content" });
        expect(result).toContain("CLIPBOARD WRITE SUCCESS");
    });

    it("should error when write is missing content", async () => {
        const result = await execute({ action: "write" });
        expect(result).toContain("CLIPBOARD ERROR");
    });

    it("should handle ZodError", async () => {
        const result = await execute({ action: "invalid" });
        expect(result).toContain("CLIPBOARD ERROR");
    });
});
