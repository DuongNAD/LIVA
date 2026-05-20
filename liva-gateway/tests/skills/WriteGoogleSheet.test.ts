/**
 * WriteGoogleSheet.test.ts — Coverage Tests
 * Mocks the auth wrapper and googleapis module to test
 * Google Sheets writing/appending logic without real network calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the auth wrapper
vi.mock("../../src/utils/googleAuth", () => ({
    getGoogleAuthClient: vi.fn().mockResolvedValue({ type: "mock-auth" }),
}));

// Mock googleapis
const mockAppendValues = vi.fn().mockResolvedValue({
    data: {
        updates: {
            updatedCells: 6,
        },
    },
});

vi.mock("googleapis", () => ({
    google: {
        sheets: vi.fn(() => ({
            spreadsheets: {
                values: {
                    append: mockAppendValues,
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

import { metadata, execute } from "../../src/skills/docs/WriteGoogleSheet";

describe("WriteGoogleSheet Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockAppendValues.mockResolvedValue({
            data: {
                updates: {
                    updatedCells: 6,
                },
            },
        });
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("write_google_sheet");
        });

        it("should require spreadsheetId, range, and values", () => {
            expect(metadata.parameters.required).toContain("spreadsheetId");
            expect(metadata.parameters.required).toContain("range");
            expect(metadata.parameters.required).toContain("values");
        });
    });

    describe("execute — happy path", () => {
        it("should append values and return success statement", async () => {
            const result = await execute({
                spreadsheetId: "sheet-456",
                range: "Sheet1!A1",
                values: [["A", "B"], ["C", "D"]],
            });

            expect(result).toContain("✅");
            expect(result).toContain("6"); // updatedCells count
            expect(result).toContain("sheet-456");
            expect(mockAppendValues).toHaveBeenCalled();
            
            const appendCall = mockAppendValues.mock.calls[0][0];
            expect(appendCall.spreadsheetId).toBe("sheet-456");
            expect(appendCall.range).toBe("Sheet1!A1");
            expect(appendCall.valueInputOption).toBe("USER_ENTERED");
            expect(appendCall.insertDataOption).toBe("INSERT_ROWS");
            expect(appendCall.requestBody.values).toEqual([["A", "B"], ["C", "D"]]);
        });
    });

    describe("execute — edge cases", () => {
        it("should handle null updates or cells count in response", async () => {
            mockAppendValues.mockResolvedValueOnce({
                data: { updates: null },
            });

            const result = await execute({
                spreadsheetId: "sheet-456",
                range: "Sheet1!A1",
                values: [["A"]],
            });

            expect(result).toContain("✅");
            expect(result).toContain("undefined"); // updatedCells is undefined
        });
    });

    describe("execute — error handling", () => {
        it("should return error message when Sheets API fails", async () => {
            mockAppendValues.mockRejectedValueOnce(new Error("Write permission denied"));

            const result = await execute({
                spreadsheetId: "sheet-456",
                range: "Sheet1!A1",
                values: [["A"]],
            });

            expect(result).toContain("❌");
            expect(result).toContain("Write permission denied");
        });
    });
});
