/**
 * SearchGoogleDrive.test.ts — Coverage Tests
 * Mocks the auth wrapper and googleapis module to test
 * Google Drive file searching logic without real network calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the auth wrapper
vi.mock("../../src/utils/googleAuth", () => ({
    getGoogleAuthClient: vi.fn().mockResolvedValue({ type: "mock-auth" }),
}));

// Mock googleapis
const mockListFiles = vi.fn().mockResolvedValue({
    data: {
        files: [
            { id: "file-1", name: "Report.pdf", mimeType: "application/pdf" },
            { id: "file-2", name: "Data.xlsx", mimeType: "application/vnd.ms-excel" },
        ],
    },
});

vi.mock("googleapis", () => ({
    google: {
        drive: vi.fn(() => ({
            files: {
                list: mockListFiles,
            },
        })),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { metadata, execute } from "../../src/skills/docs/SearchGoogleDrive";

describe("SearchGoogleDrive Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        mockListFiles.mockResolvedValue({
            data: {
                files: [
                    { id: "file-1", name: "Report.pdf", mimeType: "application/pdf" },
                    { id: "file-2", name: "Data.xlsx", mimeType: "application/vnd.ms-excel" },
                ],
            },
        });
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("search_google_drive");
        });

        it("should require query parameter", () => {
            expect(metadata.parameters.required).toContain("query");
        });
    });

    describe("execute — happy path", () => {
        it("should list files and return formatted output", async () => {
            const result = await execute({
                query: "name contains 'Report'",
            });

            expect(result).toContain("[Kết quả tìm kiếm trên Drive]");
            expect(result).toContain("Tên file: Report.pdf | ID: file-1");
            expect(result).toContain("Tên file: Data.xlsx | ID: file-2");
            expect(mockListFiles).toHaveBeenCalled();
            
            const listCall = mockListFiles.mock.calls[0][0];
            expect(listCall.q).toBe("name contains 'Report'");
            expect(listCall.fields).toBe("files(id, name, mimeType)");
            expect(listCall.pageSize).toBe(10);
        });
    });

    describe("execute — edge cases", () => {
        it("should handle empty or null files array in response", async () => {
            mockListFiles.mockResolvedValueOnce({
                data: { files: null },
            });

            const result = await execute({
                query: "non-existent-query",
            });

            expect(result).toContain("Không tìm thấy file nào khớp với truy vấn");
        });

        it("should handle empty files array", async () => {
            mockListFiles.mockResolvedValueOnce({
                data: { files: [] },
            });

            const result = await execute({
                query: "non-existent-query",
            });

            expect(result).toContain("Không tìm thấy file nào khớp với truy vấn");
        });
    });

    describe("execute — error handling", () => {
        it("should return error message when Drive API fails", async () => {
            mockListFiles.mockRejectedValueOnce(new Error("Invalid query syntax"));

            const result = await execute({
                query: "bad-syntax",
            });

            expect(result).toContain("❌");
            expect(result).toContain("Invalid query syntax");
        });
    });
});
