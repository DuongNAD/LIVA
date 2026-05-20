/**
 * ReadGoogleSheet.test.ts — Coverage Tests
 * Mocks the auth wrapper and googleapis module to test
 * Google Sheets reading logic without real network calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the auth wrapper
vi.mock("../../src/utils/googleAuth", () => ({
    getGoogleAuthClient: vi.fn().mockResolvedValue({ type: "mock-auth" }),
}));

// Mock googleapis
const mockGetValues = vi.fn().mockResolvedValue({
    data: {
        values: [
            ["Header1", "Header2"],
            ["Val1", "Val2"],
        ],
    },
});

vi.mock("googleapis", () => ({
    google: {
        sheets: vi.fn(() => ({
            spreadsheets: {
                values: {
                    get: mockGetValues,
                },
            },
        })),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { metadata, execute } from "../../src/skills/docs/ReadGoogleSheet";

describe("ReadGoogleSheet Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockGetValues.mockResolvedValue({
            data: {
                values: [
                    ["Header1", "Header2"],
                    ["Val1", "Val2"],
                ],
            },
        });
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("read_google_sheet");
        });

        it("should require spreadsheetId and range", () => {
            expect(metadata.parameters.required).toContain("spreadsheetId");
            expect(metadata.parameters.required).toContain("range");
        });
    });

    describe("execute — happy path", () => {
        it("should read a sheet and return formatted text", async () => {
            const result = await execute({
                spreadsheetId: "sheet-123",
                range: "Sheet1!A1:B2",
            });

            expect(result).toContain("[Bảng dữ liệu từ Sheet1!A1:B2]");
            expect(result).toContain("Header1 | Header2");
            expect(result).toContain("Val1 | Val2");
            expect(mockGetValues).toHaveBeenCalled();
        });
    });

    describe("execute — edge cases", () => {
        it("should handle empty or null values response", async () => {
            mockGetValues.mockResolvedValueOnce({
                data: { values: null },
            });

            const result = await execute({
                spreadsheetId: "sheet-123",
                range: "Sheet1!A1:B2",
            });

            expect(result).toContain("Không tìm thấy dữ liệu nào");
        });

        it("should handle empty array response", async () => {
            mockGetValues.mockResolvedValueOnce({
                data: { values: [] },
            });

            const result = await execute({
                spreadsheetId: "sheet-123",
                range: "Sheet1!A1:B2",
            });

            expect(result).toContain("Không tìm thấy dữ liệu nào");
        });
    });

    describe("execute — error handling", () => {
        it("should return error message when Sheets API fails", async () => {
            mockGetValues.mockRejectedValueOnce(new Error("API limits exceeded"));

            const result = await execute({
                spreadsheetId: "sheet-123",
                range: "Sheet1!A1:B2",
            });

            expect(result).toContain("❌");
            expect(result).toContain("API limits exceeded");
        });
    });
});
