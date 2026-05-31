import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: { info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn() },
}));

vi.mock("node:fs", async () => {
    const actual: any = await vi.importActual("node:fs");
    return {
        ...actual,
        promises: {
            ...actual.promises,
            mkdir: vi.fn().mockResolvedValue(undefined),
            rename: vi.fn().mockResolvedValue(undefined),
        },
    };
});

vi.mock("qrcode", () => ({
    default: { toFile: vi.fn().mockResolvedValue(undefined) },
    toFile: vi.fn().mockResolvedValue(undefined),
}));

import { metadata, execute } from "../../src/skills/data/QRCodeTool";

describe("QRCodeTool", () => {
    beforeEach(() => { vi.clearAllMocks(); });

    it("should have correct metadata", () => {
        expect(metadata.name).toBe("qr_code_tool");
        expect(metadata.parameters.required).toContain("data");
    });

    it("should reject empty data", async () => {
        const result = await execute({ data: "" });
        expect(result).toContain("Error");
    });

    it("should generate QR code", async () => {
        const result = await execute({ data: "https://example.com", output_path: "C:\\temp\\test_qr.png" });
        expect(result).toContain("✅");
        expect(result).toContain("QR Code generated");
    });

    it("should use default desktop path when no output_path", async () => {
        const result = await execute({ data: "Hello QR" });
        expect(result).toContain("✅");
    });
});
