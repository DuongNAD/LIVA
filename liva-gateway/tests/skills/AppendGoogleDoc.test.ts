/**
 * AppendGoogleDoc.test.ts — Coverage Tests
 * Mocks the auth wrapper and googleapis module to test
 * the append logic without real API calls.
 */
import { describe, it, expect, vi, beforeEach } from "vitest";

// Mock the auth wrapper (not the deep SDK internals)
vi.mock("../../src/utils/googleAuth", () => ({
    getGoogleAuthClient: vi.fn().mockResolvedValue({ type: "mock-auth" }),
}));

// Mock googleapis at the top level — thin mock of docs API
const mockBatchUpdate = vi.fn().mockResolvedValue({});
const mockGet = vi.fn().mockResolvedValue({
    data: {
        body: {
            content: [
                { endIndex: 50 },
            ],
        },
    },
});

vi.mock("googleapis", () => ({
    google: {
        docs: vi.fn(() => ({
            documents: {
                get: mockGet,
                batchUpdate: mockBatchUpdate,
            },
        })),
    },
}));

vi.mock("../../src/utils/logger", () => ({
    logger: {
        info: vi.fn(), warn: vi.fn(), error: vi.fn(), debug: vi.fn(),
    },
}));

import { metadata, execute } from "../../src/skills/docs/AppendGoogleDoc";

describe("AppendGoogleDoc Skill", () => {
    beforeEach(() => {
        vi.clearAllMocks();
        // Reset default successful mock
        mockGet.mockResolvedValue({
            data: {
                body: {
                    content: [{ endIndex: 50 }],
                },
            },
        });
        mockBatchUpdate.mockResolvedValue({});
    });

    describe("metadata", () => {
        it("should export correct skill name", () => {
            expect(metadata.name).toBe("append_google_doc");
        });

        it("should require documentId and text", () => {
            expect(metadata.parameters.required).toContain("documentId");
            expect(metadata.parameters.required).toContain("text");
        });

        it("should have search keywords", () => {
            expect(metadata.search_keywords).toBeDefined();
            expect(metadata.search_keywords.length).toBeGreaterThan(0);
        });
    });

    describe("execute — happy path", () => {
        it("should append text and return success with link", async () => {
            const result = await execute({
                documentId: "test-doc-123",
                text: "Hello World",
            });

            expect(result).toContain("✅");
            expect(result).toContain("test-doc-123");
            expect(result).toContain("docs.google.com");
            expect(mockBatchUpdate).toHaveBeenCalled();
        });

        it("should use correct insert index (endIndex - 1)", async () => {
            await execute({ documentId: "doc-456", text: "New content" });

            const batchCall = mockBatchUpdate.mock.calls[0][0];
            const insertRequest = batchCall.requestBody.requests[0].insertText;
            expect(insertRequest.location.index).toBe(49); // endIndex(50) - 1
        });
    });

    describe("execute — edge cases", () => {
        it("should handle empty document body content", async () => {
            mockGet.mockResolvedValueOnce({
                data: { body: { content: null } },
            });

            const result = await execute({
                documentId: "empty-doc",
                text: "text",
            });
            expect(result).toContain("trống");
        });

        it("should handle document with no endIndex", async () => {
            mockGet.mockResolvedValueOnce({
                data: { body: { content: [{}] } },
            });

            const result = await execute({
                documentId: "no-index-doc",
                text: "text",
            });
            // Should still attempt to append (endIndexOfDoc defaults to 1)
            expect(typeof result).toBe("string");
        });
    });

    describe("execute — error handling", () => {
        it("should return error message when API fails", async () => {
            mockGet.mockRejectedValueOnce(new Error("API quota exceeded"));

            const result = await execute({
                documentId: "failing-doc",
                text: "text",
            });
            expect(result).toContain("❌");
            expect(result).toContain("API quota exceeded");
        });

        it("should handle batchUpdate failure", async () => {
            mockBatchUpdate.mockRejectedValueOnce(new Error("Permission denied"));

            const result = await execute({
                documentId: "no-perm-doc",
                text: "text",
            });
            expect(result).toContain("❌");
            expect(result).toContain("Permission denied");
        });
    });
});
