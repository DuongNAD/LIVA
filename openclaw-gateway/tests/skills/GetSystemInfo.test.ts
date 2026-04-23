import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { execute, metadata } from "../../src/skills/GetSystemInfo";

// ============================================================
// Tests
// ============================================================
describe("GetSystemInfo Skill", () => {
    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("get_system_info");
        });

        it("should not require any parameters", () => {
            expect(metadata.parameters.required).toHaveLength(0);
        });
    });

    describe("System Info Retrieval", () => {
        it("should return a report containing OS info", async () => {
            const result = await execute();
            expect(result).toContain("Hệ điều hành");
        });

        it("should return a report containing CPU info", async () => {
            const result = await execute();
            expect(result).toContain("Bộ vi xử lý");
            expect(result).toContain("cores");
        });

        it("should return a report containing RAM info", async () => {
            const result = await execute();
            expect(result).toContain("RAM");
            expect(result).toContain("GB");
        });

        it("should return a non-empty string", async () => {
            const result = await execute();
            expect(typeof result).toBe("string");
            expect(result.length).toBeGreaterThan(50);
        });
    });
});
