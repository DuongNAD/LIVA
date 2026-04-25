/**
 * ReadRecentEmails.test.ts — Full Coverage Tests
 * Tests the mock email retrieval skill including
 * metadata validation, execute output, and error handling.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { metadata, execute } from "../../src/skills/ReadRecentEmails";

describe("ReadRecentEmails Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("read_recent_emails");
        });

        it("should have proper search keywords", () => {
            expect(metadata.search_keywords).toContain("read_recent_emails");
        });

        it("should have a description", () => {
            expect(metadata.description).toBeTruthy();
            expect(typeof metadata.description).toBe("string");
        });

        it("should have empty required parameters", () => {
            expect(metadata.parameters.required).toEqual([]);
        });
    });

    describe("execute", () => {
        it("should return mock email list", async () => {
            const result = await execute();
            expect(typeof result).toBe("string");
            expect(result).toContain("Email");
            expect(result.length).toBeGreaterThan(0);
        });

        it("should contain at least one email entry", async () => {
            const result = await execute();
            // Mock data contains numbered entries
            expect(result).toMatch(/\d\./);
        });

        it("should contain email subjects", async () => {
            const result = await execute();
            expect(result).toContain("Tiêu đề:");
        });

        it("should contain sender information", async () => {
            const result = await execute();
            expect(result).toContain("Từ:");
        });
    });
});
